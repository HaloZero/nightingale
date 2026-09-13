"""Lyrics alignment: align pre-fetched lyrics text to vocals audio using WhisperX."""

import json
import re

import cjk
from audio import detect_vocal_region
from gpu import gpu_model
from language import detect_language_multiwindow
from whisper_compat import (
    progress, align_device_for, compute_type_for, align_with_fallback, get_align_backend,
    get_effective_align_backend,
)


def align_lyrics(
    lyrics_path: str,
    vocals_path: str,
    device: str,
    model_name: str = "large-v3",
    language_override: str | None = None,
    whisper_model=None,
    pre_align_cleanup=None,
) -> dict:
    """Align pre-existing lyrics to vocals audio using WhisperX.

    Steps:
      1. Load lyrics text from JSON
      2. Load vocals audio and detect vocal region via RMS
      3. Detect language
      4. Probe multiple start offsets to find where vocals actually begin
      5. Run final forced alignment from the best offset
      6. Map aligned word timestamps back to original lyric lines
    """
    import whisperx

    progress(55, "Loading lyrics...")
    with open(lyrics_path, "r", encoding="utf-8") as f:
        lyrics_data = json.load(f)

    lines = lyrics_data.get("lines", [])
    print(f"[nightingale:LOG] Lyrics loaded: {len(lines)} lines", flush=True)

    raw_anchors = lyrics_data.get("line_anchors")
    if raw_anchors is not None and len(raw_anchors) != len(lines):
        print(
            f"[nightingale:LOG] line_anchors length {len(raw_anchors)} != "
            f"lines length {len(lines)}, ignoring",
            flush=True,
        )
        raw_anchors = None

    clean_lines: list[str] = []
    clean_anchors: list[float | None] = []
    for i, line in enumerate(lines):
        text = line.strip() if isinstance(line, str) else str(line).strip()
        if text:
            clean_lines.append(text)
            clean_anchors.append(raw_anchors[i] if raw_anchors is not None else None)

    have_anchors = raw_anchors is not None and any(a is not None for a in clean_anchors)
    if have_anchors:
        matched = sum(1 for a in clean_anchors if a is not None)
        print(
            f"[nightingale:LOG] Using LRCLIB timing anchors: {matched}/{len(clean_anchors)} lines matched",
            flush=True,
        )

    audio = whisperx.load_audio(vocals_path)
    duration_secs = len(audio) / 16000
    print(f"[nightingale:LOG] Vocals audio loaded: {len(audio)} samples ({duration_secs:.1f}s)", flush=True)

    progress(56, "Detecting vocal regions...")
    vocal_start, vocal_end = detect_vocal_region(audio)

    a_device = align_device_for(device)
    c_type = compute_type_for(device)

    if language_override and language_override.strip().lower() not in ("unknown", "und", ""):
        language = language_override
        print(f"[nightingale:LOG] Using language override: '{language}'", flush=True)
        progress(59, f"Language override: {language}")
    else:
        progress(58, "Detecting language...")
        with gpu_model(f"whisper:{model_name}:lang_detect") as held:
            model = whisperx.load_model(
                model_name, a_device, compute_type=c_type, task="transcribe",
            )
            held.append(model)
            language = detect_language_multiwindow(model, audio)
        print(f"[nightingale:LOG] Detected language: '{language}'", flush=True)
        progress(59, f"Detected language: {language}")

    progress(80, f"Final alignment from {vocal_start:.1f}s...")

    import qwen_align
    if get_align_backend() == "qwen" and qwen_align.is_supported(language):
        qwen_segments = _align_lyrics_qwen(
            clean_lines, audio, language, vocal_start, vocal_end, pre_align_cleanup,
            line_anchors=clean_anchors if have_anchors else None,
        )
        if qwen_segments is not None:
            progress(90, f"Alignment complete: {len(qwen_segments)} segments, lang={language}")
            if qwen_segments:
                print(f"[nightingale:LOG] First segment: '{qwen_segments[0]['text'][:100]}'", flush=True)
                print(f"[nightingale:LOG] Last segment: '{qwen_segments[-1]['text'][:100]}'", flush=True)
            return {
                "language": language, "segments": qwen_segments, "source": "lyrics",
                "align_backend": "qwen",
            }
        print(
            "[nightingale:LOG] Qwen lyrics alignment unavailable; falling back to wav2vec2 path",
            flush=True,
        )

    line_token_pairs: list[list[tuple[str, str]]] | None = None
    if cjk.is_cjk(language):
        line_token_pairs = [cjk.tokenize_for_alignment(line, language) for line in clean_lines]
        cleaned_lines = ["".join(r for _, r in toks) for toks in line_token_pairs]
        full_text = "".join(cleaned_lines)
        print(
            f"[nightingale:LOG] CJK alignment input: {len(full_text)} chars across "
            f"{sum(1 for c in cleaned_lines if c)} non-empty lines (lang={language}, "
            f"model={cjk.align_model_for(language) or 'default'})",
            flush=True,
        )
    else:
        full_text = " ".join(clean_lines)

    line_groups: list[int] | None = None
    if have_anchors:
        joiner = "" if cjk.is_cjk(language) else " "
        raw_segments, line_groups = _build_anchor_segments(
            cleaned_lines if cjk.is_cjk(language) else clean_lines,
            clean_anchors, vocal_start, vocal_end, joiner=joiner,
        )
        print(
            f"[nightingale:LOG] Anchor-bounded alignment: {len(raw_segments)} segments "
            f"for {len(clean_lines)} lines",
            flush=True,
        )
    else:
        raw_segments = [{"text": full_text, "start": vocal_start, "end": vocal_end}]

    align_result = align_with_fallback(
        raw_segments, audio, cjk.align_lang_code(language), a_device, pre_align_cleanup,
        model_name=cjk.align_model_for(language),
    )

    if cjk.is_cjk(language):
        segments = _map_chars_to_lines_cjk(
            align_result, clean_lines, line_token_pairs, language, line_groups=line_groups,
        )
    else:
        segments = _map_words_to_lines(align_result, clean_lines, line_groups=line_groups)
        if cjk.is_korean(language):
            for seg in segments:
                cjk.attach_reading(seg["words"], language)

    progress(90, f"Alignment complete: {len(segments)} segments, lang={language}")
    if segments:
        print(f"[nightingale:LOG] First segment: '{segments[0]['text'][:100]}'", flush=True)
        print(f"[nightingale:LOG] Last segment: '{segments[-1]['text'][:100]}'", flush=True)

    return {
        "language": language, "segments": segments, "source": "lyrics",
        "align_backend": get_effective_align_backend(),
    }


# Slop margin around each anchor window for LRCLIB timestamp rounding. This
# only needs to absorb that rounding, not the multi-second instrumental gap
# this whole mechanism exists to eliminate -- the anchor itself already pins
# the true window.
ANCHOR_PAD_SECONDS = 0.3


def _build_anchor_segments(
    texts: list[str],
    anchors: list[float | None],
    vocal_start: float,
    vocal_end: float,
    joiner: str = " ",
) -> tuple[list[dict], list[int]]:
    """Build one raw_segment per run of lines between two known anchors.

    A line with its own anchor starts a new run; any immediately-following
    unanchored lines join that same run rather than getting split out, since
    there's no anchor to tell us where the first line's singing actually
    ends -- an anchored line followed by a gap shares one (still small)
    window with that gap instead of being given an artificially wide window
    of its own. A leading run with no anchor at all (only possible before the
    first matched line) is bounded by `vocal_start` on the left.

    Returns ``(raw_segments, line_counts)`` where ``line_counts[i]`` is how
    many of ``texts`` (in order) segment ``i`` covers -- 1 for a normal
    anchored line with an anchored line, more for a merged run.
    ``sum(line_counts) == len(texts)``.
    """
    n = len(texts)
    segments: list[dict] = []
    line_counts: list[int] = []
    i = 0
    while i < n:
        run_start_anchor = anchors[i]
        j = i + 1 if run_start_anchor is not None else i
        while j < n and anchors[j] is None:
            j += 1
        # j now points at the next anchored line (or n).
        end_anchor = anchors[j] if j < n else None

        start = (
            max(vocal_start, run_start_anchor - ANCHOR_PAD_SECONDS)
            if run_start_anchor is not None
            else vocal_start
        )
        end = (
            min(vocal_end, end_anchor + ANCHOR_PAD_SECONDS)
            if end_anchor is not None
            else vocal_end
        )
        end = max(start, end)

        segments.append({"text": joiner.join(texts[i:j]), "start": start, "end": end})
        line_counts.append(j - i)
        i = j

    return segments, line_counts


def _normalize(word: str) -> str:
    return re.sub(r"[^\w]", "", word).lower()


def _collect_words(words: list[dict]) -> list[dict]:
    """Normalize one segment's raw aligned ``words`` list (see
    `_collect_aligned`'s docstring for the NaN-timestamp handling this
    preserves), keeping NaN-timestamp entries in-stream rather than dropping
    them."""
    out: list[dict] = []
    for w in words:
        text = w.get("word", "").strip()
        if not text:
            continue
        out.append({
            "word": text,
            "norm": _normalize(text),
            "start": w.get("start"),
            "end": w.get("end"),
            "score": w.get("score"),
        })
    return out


def _collect_aligned(align_result: dict) -> list[dict]:
    """Extract every aligned word in input-text order, including NaN-timestamp ones.

    WhisperX iterates input characters in order and groups them by a
    monotonically-increasing word index, so the resulting word list preserves
    lyric order. Words the CTC backtrack could not place are emitted without
    "start"/"end" keys (whisperx/alignment.py l.343-350); we keep those in the
    stream as None-timestamp entries so the line-mapper can treat them as
    missing rather than silently consuming a later occurrence's timestamp.
    """
    out: list[dict] = []
    for seg in align_result.get("segments", []):
        out.extend(_collect_words(seg.get("words", [])))
    return out


def _slice_chars_to_lines(
    aligned: list[dict],
    original_lines: list[str],
    line_token_pairs: list[list[tuple[str, str]]],
    language: str,
) -> tuple[list[dict], int]:
    """Attribute one flat char-timing stream (`aligned`) onto `original_lines`
    by cumulative alignment-char count. Shared by the legacy whole-song call
    (one call across every line) and the per-anchor-group call (one call per
    segment's own char stream against just its lines, scoped to that group so
    a single misaligned char there can no longer shift any other group's
    slice boundary)."""
    segments: list[dict] = []
    cursor = 0
    skipped_empty = 0

    for original, token_pairs in zip(original_lines, line_token_pairs):
        n = sum(len(r) for _, r in token_pairs)
        if n == 0:
            skipped_empty += 1
            continue

        slice_chars = aligned[cursor:cursor + n]
        cursor += len(slice_chars)

        if not token_pairs:
            continue
        surfaces = [s for s, _ in token_pairs]
        lengths = [len(r) for _, r in token_pairs]

        fb_start = next((c["start"] for c in slice_chars if c["start"] is not None), None)
        fb_end = next((c["end"] for c in reversed(slice_chars) if c["end"] is not None), None)

        entries = cjk.attribute_chars_to_tokens(
            surfaces, slice_chars,
            fallback_start=fb_start,
            fallback_end=fb_end,
            cleaned_lengths=lengths,
        )
        entries = cjk.merge_punct(entries)

        valid = [e for e in entries if e.get("start") is not None and e.get("end") is not None]
        if not valid:
            continue

        for e in valid:
            e["start"] = round(e["start"], 3)
            e["end"] = round(e["end"], 3)
            if e["end"] < e["start"]:
                e["end"] = e["start"]
            if "score" in e and e["score"] is not None:
                e["score"] = round(e["score"], 3)

        cjk.attach_reading(valid, language)

        seg_start = valid[0]["start"]
        seg_end = valid[-1]["end"]
        if seg_end < seg_start:
            seg_end = seg_start

        segments.append({
            "text": original,
            "start": seg_start,
            "end": seg_end,
            "words": valid,
        })

    return segments, skipped_empty


def _map_chars_to_lines_cjk(
    align_result: dict,
    original_lines: list[str],
    line_token_pairs: list[list[tuple[str, str]]],
    language: str,
    line_groups: list[int] | None = None,
) -> list[dict]:
    """Map per-character whisperx timestamps onto fugashi/jieba tokens.

    ``line_token_pairs[i]`` is the per-token (display_surface,
    alignment_text) decomposition of ``original_lines[i]``. Without anchors
    (``line_groups=None``), the aligner saw the concatenation of every
    alignment_text as a single segment and emitted one timed entry per char
    in input order; `_slice_chars_to_lines` slices that one stream per line
    by the line's total alignment-char count. With per-line anchors,
    ``align_result["segments"]`` already has one entry per ``line_groups``
    run (see `_build_anchor_segments`), so each run's own char stream is
    sliced against just its own lines instead of the whole song's.
    """
    if line_groups is None:
        aligned = _collect_aligned(align_result)
        timed_count = sum(1 for a in aligned if a["start"] is not None and a["end"] is not None)
        print(
            f"[nightingale:LOG] CJK alignment: {len(aligned)} chars emitted "
            f"({timed_count} timed, {len(aligned) - timed_count} NaN)",
            flush=True,
        )
        segments, skipped_empty = _slice_chars_to_lines(
            aligned, original_lines, line_token_pairs, language,
        )
    else:
        segments = []
        skipped_empty = 0
        cursor = 0
        for seg, count in zip(align_result.get("segments", []), line_groups):
            group_originals = original_lines[cursor:cursor + count]
            group_tokens = line_token_pairs[cursor:cursor + count]
            cursor += count
            aligned = _collect_words(seg.get("words", []))
            seg_segments, se = _slice_chars_to_lines(aligned, group_originals, group_tokens, language)
            segments.extend(seg_segments)
            skipped_empty += se

    for i in range(1, len(segments)):
        prev = segments[i - 1]
        cur = segments[i]
        if cur["start"] < prev["end"]:
            cur["start"] = prev["end"]
            if cur["end"] < cur["start"]:
                cur["end"] = cur["start"]

    total_words = sum(len(s["words"]) for s in segments)
    print(
        f"[nightingale:LOG] CJK lyrics alignment: {len(segments)} lines, "
        f"{total_words} tokens ({skipped_empty} empty lines skipped)",
        flush=True,
    )

    return _split_long_segments(segments, joiner="")


def _split_long_segments(segments: list[dict], joiner: str = " ") -> list[dict]:
    MAX_WORDS_PER_LINE = 10
    out: list[dict] = []
    for seg in segments:
        words = seg["words"]
        if len(words) <= MAX_WORDS_PER_LINE:
            out.append(seg)
            continue
        for chunk in [words[i:i+MAX_WORDS_PER_LINE] for i in range(0, len(words), MAX_WORDS_PER_LINE)]:
            out.append({
                "text": joiner.join(w["word"] for w in chunk),
                "start": chunk[0]["start"],
                "end": chunk[-1]["end"],
                "words": chunk,
            })
    return out


def _align_lyrics_qwen(
    clean_lines: list[str],
    audio,
    language: str,
    vocal_start: float,
    vocal_end: float,
    pre_align_cleanup=None,
    line_anchors: list[float | None] | None = None,
) -> list[dict] | None:
    """Align lyrics with Qwen3-ForcedAligner and map tokens back to lines.

    Without anchors, the whole vocal region is aligned in one pass against
    the newline-joined lyrics (newlines make line boundaries token boundaries
    and are dropped by Qwen's tokenizer), and the flat timed-token stream is
    sliced onto lines by cumulative kept-char count. With per-line anchors
    (see `_build_anchor_segments`), one bounded segment per anchor-run is
    aligned instead -- `qwen_align.qwen_align` already slices its own audio
    independently per input segment, so this is squarely within its designed
    usage (its own `MAX_SEGMENT_SECONDS` comment calls out "a single
    over-long segment (e.g. a whole-song lyrics pass)" as the case to avoid).
    Returns ``None`` on any qwen failure so the caller falls back to the
    wav2vec2 path.
    """
    import qwen_align

    line_groups: list[int] | None = None
    if line_anchors is not None and any(a is not None for a in line_anchors):
        raw_segments, line_groups = _build_anchor_segments(
            clean_lines, line_anchors, vocal_start, vocal_end, joiner="\n",
        )
    else:
        full_text = "\n".join(clean_lines)
        raw_segments = [{"text": full_text, "start": vocal_start, "end": vocal_end}]

    try:
        result = qwen_align.qwen_align_with_cpu_fallback(
            raw_segments, audio, language, pre_align_cleanup,
        )
    except qwen_align.QwenUnsupportedError as e:
        print(f"[nightingale:LOG] Qwen aligner unsupported: {e}", flush=True)
        return None
    except Exception as e:
        print(f"[nightingale:LOG] Qwen aligner failed: {e}", flush=True)
        return None

    segments = _map_qwen_units_to_lines(result, clean_lines, language, line_groups=line_groups)
    if not segments:
        return None

    total_words = sum(len(s["words"]) for s in segments)
    print(
        f"[nightingale:LOG] Qwen lyrics alignment: {len(segments)} lines, {total_words} tokens",
        flush=True,
    )
    return segments


def _slice_qwen_units_to_lines(units: list[dict], lines: list[str], language: str) -> list[dict]:
    """Slice one flat Qwen timed-token stream onto `lines` by kept-char
    count. Shared by the legacy whole-song call and the per-anchor-group
    call (one call per segment's own token stream against just its lines)."""
    segments: list[dict] = []
    cursor = 0

    for line_text in lines:
        need = cjk.qwen_kept_len(line_text)
        if need == 0:
            continue

        taken: list[dict] = []
        acc = 0
        while cursor < len(units) and acc < need:
            u = units[cursor]
            cursor += 1
            taken.append(u)
            acc += cjk.qwen_kept_len(u["word"])

        words: list[dict] = []
        for u in taken:
            if u["start"] is None or u["end"] is None:
                continue
            entry = {"word": u["word"], "start": round(u["start"], 3), "end": round(u["end"], 3)}
            if entry["end"] < entry["start"]:
                entry["end"] = entry["start"]
            if u["score"] is not None:
                entry["score"] = round(u["score"], 3)
            words.append(entry)

        if not words:
            continue

        if cjk.is_supported_lang(language):
            cjk.attach_reading(words, language)

        seg_start = words[0]["start"]
        seg_end = words[-1]["end"]
        if seg_end < seg_start:
            seg_end = seg_start

        segments.append({
            "text": line_text,
            "start": seg_start,
            "end": seg_end,
            "words": words,
        })

    return segments


def _map_qwen_units_to_lines(
    align_result: dict,
    clean_lines: list[str],
    language: str,
    line_groups: list[int] | None = None,
) -> list[dict]:
    """Slice Qwen's timed-token stream(s) onto lyric lines by kept-char count.

    Every Qwen token carries a timestamp (the model never drops units), and
    token surfaces concatenate to each line's kept content, so a running
    kept-char counter attributes tokens to lines without normalized-text
    matching. Readings are attached per line for CJK/Korean. Without anchors
    (``line_groups=None``), this slices one flat stream across every line;
    with anchors, ``align_result["segments"]`` already has one entry per
    ``line_groups`` run, so each run's own stream is sliced against just its
    own lines.
    """
    if line_groups is None:
        units = _collect_aligned(align_result)
        segments = _slice_qwen_units_to_lines(units, clean_lines, language)
    else:
        segments = []
        cursor = 0
        for seg, count in zip(align_result.get("segments", []), line_groups):
            group_lines = clean_lines[cursor:cursor + count]
            cursor += count
            units = _collect_words(seg.get("words", []))
            segments.extend(_slice_qwen_units_to_lines(units, group_lines, language))

    for i in range(1, len(segments)):
        prev = segments[i - 1]
        cur = segments[i]
        if cur["start"] < prev["end"]:
            cur["start"] = prev["end"]
            if cur["end"] < cur["start"]:
                cur["end"] = cur["start"]

    joiner = "" if cjk.is_cjk(language) else " "
    return _split_long_segments(segments, joiner=joiner)


_WORD_MATCH_LOOKAHEAD = 6


def _match_lines_against_words(aligned: list[dict], lines: list[str]) -> tuple[list[dict], int, int]:
    """Re-associate a flat aligned-word stream with `lines` by forward cursor
    + bounded lookahead, so a dropped word in a repeated phrase (e.g. the 2nd
    "love" of three) no longer causes downstream occurrences to inherit each
    other's timestamps. Shared by the legacy whole-song call (one call across
    every line) and the per-anchor-group call (one call per segment's own
    words against just its lines, so a mismatch is bounded to that group
    instead of the whole song). Returns
    ``(segments, missed_lyric_words, interpolated_drops)``.
    """
    ai = 0
    segments = []
    missed_lyric_words = 0
    interpolated_drops = 0

    for line_text in lines:
        word_entries = []
        for word_text in line_text.split():
            target = _normalize(word_text)
            matched = -1
            if target:
                limit = min(ai + _WORD_MATCH_LOOKAHEAD, len(aligned))
                for k in range(ai, limit):
                    if aligned[k]["norm"] == target:
                        matched = k
                        break

            if matched >= 0:
                a = aligned[matched]
                ai = matched + 1
                if a["start"] is not None and a["end"] is not None:
                    entry = {
                        "word": word_text,
                        "start": round(a["start"], 3),
                        "end": round(a["end"], 3),
                    }
                    if a["score"] is not None:
                        entry["score"] = round(a["score"], 3)
                else:
                    entry = {"word": word_text, "start": None, "end": None, "estimated": True}
                    interpolated_drops += 1
            else:
                entry = {"word": word_text, "start": None, "end": None, "estimated": True}
                missed_lyric_words += 1
            word_entries.append(entry)

        _interpolate_missing(word_entries)

        valid_words = [e for e in word_entries if e["start"] is not None]
        if not valid_words:
            continue

        seg_start = valid_words[0]["start"]
        seg_end = valid_words[-1]["end"]
        if seg_end < seg_start:
            seg_end = seg_start

        segments.append({
            "text": line_text,
            "start": seg_start,
            "end": seg_end,
            "words": valid_words,
        })

    return segments, missed_lyric_words, interpolated_drops


def _map_words_to_lines(
    align_result: dict, clean_lines: list[str], line_groups: list[int] | None = None,
) -> list[dict]:
    """Map aligned word timestamps back to original lyric lines.

    Without anchors (``line_groups=None``), this text-matches one flat
    aligned-word stream against every line (see `_match_lines_against_words`).
    With per-line anchors, ``align_result["segments"]`` already has one entry
    per ``line_groups`` run (see `_build_anchor_segments`), so each run's own
    words are matched against just its own lines -- a line with its own
    anchor needs no text-matching at all when its run covers exactly that one
    line, since the segment's words already *are* that line's words.
    """
    if line_groups is None:
        aligned = _collect_aligned(align_result)
        timed_count = sum(1 for a in aligned if a["start"] is not None and a["end"] is not None)
        print(
            f"[nightingale:LOG] Final alignment: {len(aligned)} words emitted "
            f"({timed_count} timed, {len(aligned) - timed_count} NaN)",
            flush=True,
        )
        segments, missed_lyric_words, interpolated_drops = _match_lines_against_words(aligned, clean_lines)
    else:
        segments = []
        missed_lyric_words = 0
        interpolated_drops = 0
        cursor = 0
        for seg, count in zip(align_result.get("segments", []), line_groups):
            group_lines = clean_lines[cursor:cursor + count]
            cursor += count
            aligned = _collect_words(seg.get("words", []))
            seg_segments, m, d = _match_lines_against_words(aligned, group_lines)
            segments.extend(seg_segments)
            missed_lyric_words += m
            interpolated_drops += d

    for i in range(1, len(segments)):
        prev = segments[i - 1]
        cur = segments[i]
        if cur["start"] < prev["end"]:
            cur["start"] = prev["end"]
            if cur["end"] < cur["start"]:
                cur["end"] = cur["start"]

    total_words = sum(len(s["words"]) for s in segments)
    print(
        f"[nightingale:LOG] Lyrics alignment: {len(segments)} lines preserved, "
        f"{total_words} words ({interpolated_drops} interpolated from NaN, "
        f"{missed_lyric_words} unmatched lyric words)",
        flush=True,
    )

    return _split_long_segments(segments, joiner=" ")


def _interpolate_missing(word_entries: list[dict]):
    """Fill in timestamps for words the aligner couldn't place, using neighbors."""
    unset = [i for i, e in enumerate(word_entries) if e["start"] is None]
    set_entries = [e for e in word_entries if e["start"] is not None]

    if not unset or not set_entries:
        return

    for ui in unset:
        prev_end = set_entries[0]["start"]
        next_start = set_entries[-1]["end"]
        for j in range(ui - 1, -1, -1):
            if word_entries[j]["start"] is not None:
                prev_end = word_entries[j]["end"]
                break
        for j in range(ui + 1, len(word_entries)):
            if word_entries[j]["start"] is not None:
                next_start = word_entries[j]["start"]
                break
        mid = (prev_end + next_start) / 2
        word_entries[ui]["start"] = round(prev_end, 3)
        word_entries[ui]["end"] = round(mid, 3)
