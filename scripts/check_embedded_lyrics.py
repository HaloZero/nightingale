#!/usr/bin/env python3
"""Scan a library for embedded lyrics that don't actually belong to the
song -- not corrupted text, but the *wrong* text entirely (a Star Wars
opening crawl, lyrics in a completely different script/language than the
rest of the song, the same lyrics duplicated across unrelated songs). This
is the signature of a bad automated match (e.g. a lyrics-fetch script
pulling a bogus/joke submission, or a batch script pairing the wrong lyrics
file with the wrong song), not an encoding bug.

By default, review is interactive: each flagged file is presented one at a
time (full lyrics text shown for duplicate-across-songs issues, since you
need to actually read them to tell which copy, if any, is wrong) and you
choose to clear the embedded lyrics, fetch a replacement from the local
Lyrica lyrics server (github.com/Wilooper/Lyrica, an LRCLIB-backed lookup
service -- same idea as lrcget -- must be running on 127.0.0.1:9999) keyed
off the file's own artist/title tags, or skip it. A fetched replacement is
always previewed before it's embedded. Pass --report-only for the old
scan-and-print, never-modifies-anything behavior.

Flags a file if any of these fire:

  - replacement-char / control-chars: still worth flagging as a basic
    sanity check, but these mean corruption, not necessarily a wrong match.
  - script-mismatch: the lyrics are mostly a different writing system
    (CJK, Cyrillic, Arabic, Hebrew, Hangul) than the song's own tagged
    language would suggest -- e.g. Japanese text on a song tagged/expected
    as English. This is what catches your "Japanese" example precisely,
    without guessing at word meaning.
  - known-placeholder: matches a short list of famous "obviously not song
    lyrics" texts (Star Wars crawl, Lorem Ipsum, "mashup"). Add more
    signatures to PLACEHOLDER_SIGNATURES as you find them.
  - duplicate-across-songs: the exact same lyrics text is embedded in more
    than one song with a different artist/title -- a strong sign a
    batch/matching script attached the same (wrong) result to multiple
    files. No text analysis needed for this one, just an exact-text group-by.

Two exclusions are applied before any of the above, since they'd otherwise
be constant false positives: songs tagged with a genre in IGNORED_GENRES
(anime/animation/orchestra/soundtrack/classical/instrumental/K-pop/J-rock/
new age/world -- genuinely-foreign-language or genuinely-lyricless music)
are skipped
outright, and any file whose "lyrics" are just an "instrumental" placeholder
is skipped too, so it's never offered for clearing.

Usage:
    python3 check_embedded_lyrics.py LIBRARY_ROOT [--script-threshold 0.3]
    python3 check_embedded_lyrics.py LIBRARY_ROOT --report-only
"""

import argparse
import os
import re
import sys
from collections import defaultdict
from pathlib import Path

import mutagen
from mutagen.asf import ASF
from mutagen.flac import FLAC
from mutagen.mp4 import MP4
from mutagen.oggopus import OggOpus
from mutagen.oggvorbis import OggVorbis

sys.path.insert(0, str(Path(__file__).parent))
from lyrics_inventory import embed_plain_lyrics, fetch_lyrica, plain_lyrics_from_timed  # noqa: E402

AUDIO_EXTENSIONS = {"mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wma"}

# (name, regex matching that script's Unicode ranges)
SCRIPT_RANGES = [
    ("CJK/Japanese", re.compile(r"[぀-ヿ㐀-䶿一-鿿豈-﫿]")),
    ("Korean", re.compile(r"[가-힯]")),
    ("Cyrillic", re.compile(r"[Ѐ-ӿ]")),
    ("Arabic", re.compile(r"[؀-ۿ]")),
    ("Hebrew", re.compile(r"[֐-׿]")),
]

PLACEHOLDER_SIGNATURES = [
    ("Star Wars crawl", ["a long time ago", "galaxy far", "far away"]),
    ("Lorem Ipsum", ["lorem ipsum", "dolor sit amet"]),
    ("Mashup placeholder", ["mashup"]),
]

# Genres where "wrong-looking" lyrics are actually normal -- anime/OST songs
# are often genuinely in Japanese, and scores/soundtracks routinely have no
# real lyrics at all -- so these are excluded from scanning entirely rather
# than flagged and then always dismissed.
IGNORED_GENRES = {
    "anime", "animation", "orchestra", "soundtrack", "classical", "instrumental",
    "k-pop", "kpop", "j-rock", "jrock", "new age", "world",
}

TOKEN_RE = re.compile(r"[A-Za-z']+")


def get_lyrics_text(path: Path) -> str | None:
    try:
        audio = mutagen.File(path)
    except Exception:
        return None
    if audio is None:
        return None

    if isinstance(audio, MP4):
        values = audio.tags.get("\xa9lyr") if audio.tags else None
        return "\n".join(values) if values else None

    if isinstance(audio, (FLAC, OggVorbis, OggOpus)):
        if not audio.tags:
            return None
        for key in ("lyrics", "unsyncedlyrics"):
            values = audio.tags.get(key)
            if values:
                return "\n".join(values)
        return None

    if isinstance(audio, ASF):
        values = audio.tags.get("WM/Lyrics") if audio.tags else None
        return str(values[0]) if values else None

    # MP3, WAVE: ID3 USLT frames.
    tags = getattr(audio, "tags", None)
    if tags is not None:
        for key in tags.keys():
            if key.startswith("USLT"):
                return tags[key].text
    return None


def get_artist_title(path: Path) -> tuple[str, str]:
    try:
        audio = mutagen.File(path, easy=True)
        artist = (audio.get("artist") or [""])[0] if audio else ""
        title = (audio.get("title") or [""])[0] if audio else ""
        return artist, title or path.stem
    except Exception:
        return "", path.stem


def get_genre(path: Path) -> str:
    """Best-effort genre lookup. ASF/WMA has no mutagen "easy" mapping for
    genre, so it needs the raw WM/Genre key like get_lyrics_text's WM/Lyrics
    special-case above."""
    try:
        audio = mutagen.File(path, easy=True)
    except Exception:
        audio = None
    if audio is not None and hasattr(audio, "get"):
        values = audio.get("genre")
        if values:
            return "; ".join(values)
    try:
        audio = mutagen.File(path)
    except Exception:
        return ""
    if isinstance(audio, ASF) and audio.tags:
        values = audio.tags.get("WM/Genre")
        if values:
            return str(values[0])
    return ""


def is_ignored_genre(genre: str) -> bool:
    lowered = genre.lower()
    return any(ignored in lowered for ignored in IGNORED_GENRES)


def is_instrumental_marker(text: str) -> bool:
    """True when the embedded "lyrics" are just a placeholder noting the
    track is instrumental -- legitimate, not a wrong match, so it should
    never be offered for clearing."""
    return "instrumental" in text.lower()


def script_mismatch(text: str, threshold: float) -> str | None:
    """Returns the dominant non-Latin script name if it makes up more than
    `threshold` of the alphabetic characters, else None. Songs that are
    *actually* in that language will also trigger this -- it's a signal to
    go look, not proof of a mismatch, but combined with knowing your library
    is mostly English/Latin-script it's a strong lead."""
    alpha_chars = [c for c in text if c.isalpha()]
    if len(alpha_chars) < 20:
        return None
    for name, pattern in SCRIPT_RANGES:
        matches = pattern.findall(text)
        if len(matches) / len(alpha_chars) >= threshold:
            return name
    return None


def known_placeholder(text: str) -> str | None:
    lowered = text.lower()
    for name, markers in PLACEHOLDER_SIGNATURES:
        if all(marker in lowered for marker in markers):
            return name
    return None


def analyze(text: str, script_threshold: float) -> list[str]:
    reasons = []

    if "�" in text:
        reasons.append("replacement-char")

    control_chars = [c for c in text if ord(c) < 32 and c not in "\n\r\t"]
    if control_chars:
        reasons.append(f"control-chars(x{len(control_chars)})")

    mismatch = script_mismatch(text, script_threshold)
    if mismatch:
        reasons.append(f"script-mismatch({mismatch})")

    placeholder = known_placeholder(text)
    if placeholder:
        reasons.append(f"known-placeholder({placeholder})")

    return reasons


def normalize_for_dedup(text: str) -> str:
    return re.sub(r"\s+", " ", text.strip().lower())


def clear_embedded_lyrics(path: Path) -> bool:
    """Removes the embedded lyrics tag from a file entirely (as opposed to
    lyrics_inventory.py's embed_plain_lyrics, which writes one). Returns
    True on success."""
    suffix = path.suffix.lower()
    try:
        if suffix == ".mp3" or suffix == ".wav":
            from mutagen.id3 import ID3, ID3NoHeaderError

            try:
                tags = ID3(path)
            except ID3NoHeaderError:
                return True
            tags.delall("USLT")
            tags.save(path)
            return True
        if suffix in (".m4a", ".aac"):
            audio = MP4(path)
            if audio.tags and "\xa9lyr" in audio.tags:
                del audio.tags["\xa9lyr"]
                audio.save()
            return True
        if suffix in (".flac", ".ogg", ".opus"):
            audio = mutagen.File(path)
            if audio is None:
                return False
            if audio.tags:
                for key in ("lyrics", "unsyncedlyrics"):
                    if key in audio.tags:
                        del audio[key]
                audio.save()
            return True
        if suffix in (".wma",):
            audio = ASF(path)
            if audio.tags and "WM/Lyrics" in audio.tags:
                del audio.tags["WM/Lyrics"]
                audio.save()
            return True
    except Exception as exc:
        print(f"  failed to clear lyrics: {exc}")
        return False
    print(f"  don't know how to clear lyrics from {suffix} files")
    return False


def prompt_action() -> str:
    """Prompts until the user answers c/f/n/q (case-insensitive), or returns
    'q' on EOF (e.g. piped input running out). Blank answer means skip."""
    while True:
        try:
            answer = input("  [c]lear / [f]etch replacement / [n]skip / [q]uit? ").strip().lower()
        except EOFError:
            print()
            return "q"
        if answer in ("c", "f", "n", "q"):
            return answer
        if answer == "":
            return "n"
        print("  please answer c, f, n, or q")


def prompt_yes_no(message: str) -> bool:
    while True:
        try:
            answer = input(message).strip().lower()
        except EOFError:
            print()
            return False
        if answer in ("y", "n", ""):
            return answer == "y"
        print("  please answer y or n")


def fetch_replacement_lyrics(artist: str, title: str) -> str | None:
    """Looks up replacement lyrics on the local Lyrica server by the file's
    own artist/title tags. Returns the plain text, or None if the lookup
    failed or came back empty -- callers should treat None as "couldn't get
    a replacement", not "confirmed no lyrics exist"."""
    print(f"  fetching replacement lyrics for {artist} - {title}...")
    data = fetch_lyrica(artist, title, timestamps=True)
    if data is None:
        return None
    text = plain_lyrics_from_timed(data) if data.get("hasTimestamps") else (data.get("lyrics") or "")
    text = text.strip()
    return text or None


def offer_replacement(path: Path, artist: str, title: str) -> bool:
    """Fetches a replacement, previews it, and embeds it only on explicit
    confirmation. Returns True if it embedded something."""
    replacement = fetch_replacement_lyrics(artist, title)
    if not replacement:
        print("  no replacement lyrics found.")
        return False
    print("  replacement lyrics:")
    print(replacement)
    if prompt_yes_no("  embed this instead? [y/N] "):
        if embed_plain_lyrics(path, replacement):
            print("  embedded replacement lyrics.")
            return True
    return False


def review_issue(path: Path, artist: str, title: str) -> str:
    """Runs the clear/fetch/skip/quit prompt loop for one file. A fetch that
    doesn't end in embedding loops back to the same prompt rather than
    silently moving on, since the underlying issue is still unresolved."""
    while True:
        action = prompt_action()
        if action == "q":
            return "quit"
        if action == "n":
            return "skipped"
        if action == "c":
            if clear_embedded_lyrics(path):
                print("  cleared.")
                return "cleared"
            return "skipped"
        if action == "f":
            if offer_replacement(path, artist, title):
                return "replaced"
            # No replacement embedded -- ask again for this same file.


def review_content_flagged(flagged: list[tuple[Path, str, str, list[str], str]], preview_chars: int) -> tuple[int, int, bool]:
    cleared = replaced = 0
    for path, artist, title, reasons, text in flagged:
        preview = text[:preview_chars].replace("\n", " / ")
        print(f"--- {artist} - {title} ---")
        print(f"  file: {path}")
        print(f"  flags: {', '.join(reasons)}")
        print(f"  preview: {preview}{'...' if len(text) > preview_chars else ''}")
        outcome = review_issue(path, artist, title)
        print()
        if outcome == "quit":
            return cleared, replaced, True
        cleared += outcome == "cleared"
        replaced += outcome == "replaced"
    return cleared, replaced, False


def review_duplicate_groups(duplicate_groups: list[list[tuple[Path, str, str, str]]]) -> tuple[int, int, bool]:
    cleared = replaced = 0
    for group in duplicate_groups:
        shared_text = group[0][3]
        print(f"--- shared by {len(group)} files ---")
        print(f"  lyrics:\n{shared_text}\n")
        for path, artist, title, _text in group:
            print(f"  {artist} - {title}  ({path})")
            outcome = review_issue(path, artist, title)
            if outcome == "quit":
                return cleared, replaced, True
            cleared += outcome == "cleared"
            replaced += outcome == "replaced"
        print()
    return cleared, replaced, False


def iter_audio_files(root: Path):
    for dirpath, _dirnames, filenames in os.walk(root):
        for name in filenames:
            if name.rsplit(".", 1)[-1].lower() in AUDIO_EXTENSIONS:
                yield Path(dirpath) / name


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("root", type=Path, help="Library root to scan")
    parser.add_argument("--script-threshold", type=float, default=0.3, help="Fraction of non-Latin-script characters before flagging (default 0.3)")
    parser.add_argument("--preview-chars", type=int, default=200, help="How many characters of the lyrics to print per flagged file (default 200)")
    parser.add_argument("--report-only", action="store_true", help="Just print flagged issues, don't prompt to clear them (never modifies files)")
    args = parser.parse_args()

    if not args.root.is_dir():
        print(f"Library root not found: {args.root}", file=sys.stderr)
        return 1

    checked = 0
    skipped_genre = 0
    skipped_instrumental = 0
    entries: list[tuple[Path, str, str, str]] = []  # path, artist, title, text

    for audio_path in sorted(iter_audio_files(args.root)):
        checked += 1
        text = get_lyrics_text(audio_path)
        if not text or not text.strip():
            continue
        text = text.strip()
        if is_ignored_genre(get_genre(audio_path)):
            skipped_genre += 1
            continue
        if is_instrumental_marker(text):
            skipped_instrumental += 1
            continue
        artist, title = get_artist_title(audio_path)
        entries.append((audio_path, artist, title, text))

    print(
        f"Scanned {checked} audio files, {len(entries)} had embedded lyrics worth reviewing "
        f"({skipped_genre} skipped by genre, {skipped_instrumental} skipped as instrumental placeholders).\n"
    )

    # Per-file content checks.
    flagged: list[tuple[Path, str, str, list[str], str]] = []
    for path, artist, title, text in entries:
        reasons = analyze(text, args.script_threshold)
        if reasons:
            flagged.append((path, artist, title, reasons, text))

    # Exact-duplicate-across-different-songs check.
    by_text: dict[str, list[tuple[Path, str, str, str]]] = defaultdict(list)
    for path, artist, title, text in entries:
        by_text[normalize_for_dedup(text)].append((path, artist, title, text))

    duplicate_groups = [
        group
        for group in by_text.values()
        if len({(artist, title) for _, artist, title, _text in group}) > 1
    ]

    if args.report_only:
        print(f"Flagged (content signals): {len(flagged)}")
        for path, artist, title, reasons, text in flagged:
            preview = text[: args.preview_chars].replace("\n", " / ")
            print(f"--- {artist} - {title} ---")
            print(f"  file: {path}")
            print(f"  flags: {', '.join(reasons)}")
            print(f"  preview: {preview}{'...' if len(text) > args.preview_chars else ''}")
            print()

        print(f"\nFlagged (same lyrics on different songs): {len(duplicate_groups)} group(s)")
        for group in duplicate_groups:
            print(f"--- shared by {len(group)} files ---")
            for path, artist, title, _text in group:
                print(f"  {artist} - {title}  ({path})")
            print()

        return 0

    print(f"Flagged (content signals): {len(flagged)}")
    print(f"Flagged (same lyrics on different songs): {len(duplicate_groups)} group(s)\n")

    cleared_a, replaced_a, quit_early = review_content_flagged(flagged, args.preview_chars)
    cleared_b = replaced_b = 0
    if not quit_early:
        cleared_b, replaced_b, quit_early = review_duplicate_groups(duplicate_groups)

    print(
        f"\nDone. Cleared embedded lyrics on {cleared_a + cleared_b} file(s), "
        f"replaced on {replaced_a + replaced_b} file(s)."
    )
    if quit_early:
        print("(quit before reviewing everything)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
