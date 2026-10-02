"""Forced alignment via Meta's MMS aligner (``torchaudio.pipelines.MMS_FA``).

Alternative to the default wav2vec2-base WhisperX/CTC path: same per-segment
``transcript``/``audio``/``device`` call shape and
``{"segments": [...], "word_segments": [...]}`` output contract as
:func:`ctc_align.ctc_align`, so callers don't need to know which ran.

MMS_FA's alphabet includes an explicit ``*`` (star/OOV) token, so characters
outside its English alphabet are substituted with it per-character rather
than dropping the whole word, instead of aborting alignment for that word
like a plain wav2vec2 CTC dictionary lookup would.

MMS_FA's pretrained weights are released under CC-BY-NC 4.0 (non-commercial);
confirm licensing fits the deployment before enabling this backend outside
development/evaluation.
"""

import re

import torch
import torchaudio

from whisperx.audio import SAMPLE_RATE

_WHITESPACE_RE = re.compile(r"\S+")


def load_model(device: str) -> dict:
    """Load the MMS_FA model/tokenizer/aligner for ``device``.

    Caller owns the returned model's GPU lifecycle (see ``gpu.gpu_model``).
    """
    bundle = torchaudio.pipelines.MMS_FA
    model = bundle.get_model().to(device)
    model.eval()
    return {
        "model": model,
        "dictionary": bundle.get_dict(),
        "tokenizer": bundle.get_tokenizer(),
        "aligner": bundle.get_aligner(),
    }


def _dictionary_words(text: str, dictionary: dict) -> list[str]:
    """Split ``text`` on whitespace, substituting the star token for any
    character outside ``dictionary`` instead of dropping the word -- one
    output entry per whitespace-separated input token (positionally aligned
    with ``text.split()``), so a word is only dropped if none of its
    characters are alignable at all.

    A glyph that maps to the blank id (MMS_FA's dictionary uses "-", like
    torchaudio's English wav2vec2 ASR models) can't be an alignment target --
    forced_align rejects any target sequence containing it -- so a literal
    hyphen in the transcript ("self-control") is treated as out-of-vocabulary
    here too, same as ctc_align.py does for its own blank glyph.
    """
    star = dictionary.get("*")
    blank_id = dictionary.get("-")
    words = []
    for raw_word in _WHITESPACE_RE.findall(text.lower()):
        chars = []
        for char in raw_word:
            code = dictionary.get(char)
            if code is not None and code != blank_id:
                chars.append(char)
            elif star is not None:
                chars.append("*")
        words.append("".join(chars))
    return words


def mms_align(transcript, model_state: dict, audio, device: str) -> dict:
    """Align a known transcript to audio using MMS_FA.

    Mirrors :func:`ctc_align.ctc_align`'s signature and return shape (minus
    the ``align_model_metadata``/CJK-specific parameters MMS_FA doesn't need):
    ``{"segments": [...], "word_segments": [...]}`` where each segment carries
    a ``words`` list of ``{"word", "start"?, "end"?, "score"?}`` entries.
    """
    if not torch.is_tensor(audio):
        if isinstance(audio, str):
            from whisperx.audio import load_audio
            audio = load_audio(audio)
        audio = torch.from_numpy(audio)
    if len(audio.shape) == 1:
        audio = audio.unsqueeze(0)

    max_duration = audio.shape[1] / SAMPLE_RATE

    model = model_state["model"]
    dictionary = model_state["dictionary"]
    tokenizer = model_state["tokenizer"]
    aligner = model_state["aligner"]

    aligned_segments = []
    for segment in transcript:
        t1 = segment["start"]
        t2 = segment["end"]
        text = segment["text"]
        avg_logprob = segment.get("avg_logprob")

        aligned_seg = {"start": t1, "end": t2, "text": text, "words": []}
        if avg_logprob is not None:
            aligned_seg["avg_logprob"] = avg_logprob

        original_words = text.split()
        dict_words = _dictionary_words(text, dictionary)
        keep = [i for i, w in enumerate(dict_words) if w]
        if not keep:
            print(
                f"[nightingale:LOG] mms-align: no alignable words in segment "
                f"('{text[:60]}'), resorting to original",
                flush=True,
            )
            aligned_segments.append(aligned_seg)
            continue

        if t1 >= max_duration:
            print(
                f"[nightingale:LOG] mms-align: segment start {t1:.1f}s beyond "
                f"audio duration {max_duration:.1f}s, skipping",
                flush=True,
            )
            aligned_segments.append(aligned_seg)
            continue

        f1 = int(t1 * SAMPLE_RATE)
        f2 = int(t2 * SAMPLE_RATE)
        waveform_segment = audio[:, f1:f2]
        lengths = None
        if waveform_segment.shape[-1] < 400:
            lengths = torch.as_tensor([waveform_segment.shape[-1]]).to(device)
            waveform_segment = torch.nn.functional.pad(
                waveform_segment, (0, 400 - waveform_segment.shape[-1])
            )

        try:
            with torch.inference_mode():
                emission, _ = model(waveform_segment.to(device), lengths=lengths)
                # Caller (``_run_align`` via ``align_device_for``) already maps
                # "mps" -> "cpu" before this runs, so `device` here is always
                # "cpu" or "cuda" -- both of which torchaudio's forced_align
                # kernel supports, same as ctc_align.py's equivalent call.
                emission = emission[0].detach().float().contiguous()
                token_spans = aligner(emission, tokenizer([dict_words[i] for i in keep]))
        except Exception as e:
            print(
                f"[nightingale:LOG] mms-align: alignment failed for segment "
                f"('{text[:60]}'): {e}; resorting to original",
                flush=True,
            )
            aligned_segments.append(aligned_seg)
            continue

        # `emission` was reassigned to `emission[0]` above, so dim 0 is now
        # time and dim 1 is the class/label dimension -- using size(1) here
        # would read the label count (29) as the frame count and wildly
        # inflate every timestamp.
        num_frames = emission.size(0)
        ratio = waveform_segment.size(1) / num_frames / SAMPLE_RATE

        seg_words = []
        for i, spans in zip(keep, token_spans):
            start = round(t1 + spans[0].start * ratio, 3)
            end = round(t1 + spans[-1].end * ratio, 3)
            score = round(sum(s.score for s in spans) / len(spans), 3)
            seg_words.append({"word": original_words[i], "start": start, "end": end, "score": score})

        aligned_seg["words"] = seg_words
        aligned_segments.append(aligned_seg)

    word_segments = []
    for segment in aligned_segments:
        word_segments += segment["words"]

    return {"segments": aligned_segments, "word_segments": word_segments}
