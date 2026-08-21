#!/usr/bin/env python3
"""One-off: scan a library for embedded lyrics that don't actually belong to
the song -- not corrupted text, but the *wrong* text entirely (a Star Wars
opening crawl, lyrics in a completely different script/language than the
rest of the song, the same lyrics duplicated across unrelated songs). This
is the signature of a bad automated match (e.g. a lyrics-fetch script
pulling a bogus/joke submission, or a batch script pairing the wrong lyrics
file with the wrong song), not an encoding bug.

Read-only -- never modifies anything. Flags a file if any of these fire:

  - replacement-char / control-chars: still worth flagging as a basic
    sanity check, but these mean corruption, not necessarily a wrong match.
  - script-mismatch: the lyrics are mostly a different writing system
    (CJK, Cyrillic, Arabic, Hebrew, Hangul) than the song's own tagged
    language would suggest -- e.g. Japanese text on a song tagged/expected
    as English. This is what catches your "Japanese" example precisely,
    without guessing at word meaning.
  - known-placeholder: matches a short list of famous "obviously not song
    lyrics" texts (Star Wars crawl, Lorem Ipsum). Add more signatures to
    PLACEHOLDER_SIGNATURES as you find them.
  - duplicate-across-songs: the exact same lyrics text is embedded in more
    than one song with a different artist/title -- a strong sign a
    batch/matching script attached the same (wrong) result to multiple
    files. No text analysis needed for this one, just an exact-text group-by.

Usage:
    python3 check_embedded_lyrics.py LIBRARY_ROOT [--script-threshold 0.3]
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
]

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
    args = parser.parse_args()

    if not args.root.is_dir():
        print(f"Library root not found: {args.root}", file=sys.stderr)
        return 1

    checked = 0
    entries: list[tuple[Path, str, str, str]] = []  # path, artist, title, text

    for audio_path in sorted(iter_audio_files(args.root)):
        checked += 1
        text = get_lyrics_text(audio_path)
        if not text or not text.strip():
            continue
        artist, title = get_artist_title(audio_path)
        entries.append((audio_path, artist, title, text.strip()))

    print(f"Scanned {checked} audio files, {len(entries)} had embedded lyrics.\n")

    # Per-file content checks.
    flagged: list[tuple[Path, str, str, list[str], str]] = []
    for path, artist, title, text in entries:
        reasons = analyze(text, args.script_threshold)
        if reasons:
            flagged.append((path, artist, title, reasons, text))

    # Exact-duplicate-across-different-songs check.
    by_text: dict[str, list[tuple[Path, str, str]]] = defaultdict(list)
    for path, artist, title, text in entries:
        by_text[normalize_for_dedup(text)].append((path, artist, title))

    duplicate_groups = [
        group
        for group in by_text.values()
        if len({(artist, title) for _, artist, title in group}) > 1
    ]

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
        for path, artist, title in group:
            print(f"  {artist} - {title}  ({path})")
        print()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
