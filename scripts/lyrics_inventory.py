#!/usr/bin/env python3
"""Standalone lyrics-availability inventory for a music directory.

Scans MUSIC_DIR (edit the constant below) for audio files and records, per
song, whether it has a `.lrc` sidecar and/or embedded lyrics tags (ID3
USLT / MP4 (c)lyr / Vorbis LYRICS, depending on format) -- the same two
signals Nightingale itself tracks as has_lrc_file/has_embedded_lyrics, but
computed independently here with no dependency on the app's own DB/cache.

Results are cached to disk (CACHE_PATH) so re-running is instant. The scan
itself only ever runs when the cache is missing or --refresh is passed --
otherwise every run just queries whatever's already cached.

Usage:
    python3 scripts/lyrics_inventory.py                    # build cache if missing, print summary
    python3 scripts/lyrics_inventory.py --refresh           # force a fresh rescan
    python3 scripts/lyrics_inventory.py "search term"        # query title/artist/album (case-insensitive substring)
    python3 scripts/lyrics_inventory.py --refresh "term"      # rescan, then query
    python3 scripts/lyrics_inventory.py --missing            # list every song with neither source
"""

from __future__ import annotations

import argparse
import json
import random
import re
import requests
import sys
import time
import urllib.parse
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path

import mutagen

# --- Edit these for your setup -------------------------------------------
MUSIC_DIR = Path("/Users/rohandhaimade/Library/CloudStorage/Dropbox/iTunes/iTunes Media/Music")
CACHE_PATH = Path.home() / ".cache" / "nightingale" / "lyrics_inventory.json"
# ---------------------------------------------------------------------------

AUDIO_EXTENSIONS = {".mp3", ".flac", ".ogg", ".opus", ".wav", ".m4a", ".aac", ".wma"}
PROGRESS_EVERY = 200

LYRICA_URL = "http://127.0.0.1:9999/lyrics/"
_LRC_TIMESTAMP_RE = re.compile(r"^(\[\d+:\d+(?:\.\d+)?\])+")


@dataclass
class SongEntry:
    path: str
    title: str
    artist: str
    album: str
    genre: str
    has_lrc_file: bool
    has_embedded_lyrics: bool


def collect_audio_paths(root: Path) -> list[Path]:
    return [p for p in root.rglob("*") if p.is_file() and p.suffix.lower() in AUDIO_EXTENSIONS]


def read_common_tags(path: Path) -> tuple[str, str, str, str]:
    try:
        audio = mutagen.File(path, easy=True)
    except Exception:
        audio = None
    if audio is None or audio.tags is None:
        return (path.stem, "", "", "")
    tags = audio.tags
    title = (tags.get("title") or [path.stem])[0]
    artist = (tags.get("artist") or [""])[0]
    album = (tags.get("album") or [""])[0]
    genre = (tags.get("genre") or [""])[0]
    return (title, artist, album, genre)


def has_embedded_lyrics(path: Path) -> bool:
    try:
        audio = mutagen.File(path)
    except Exception:
        return False
    if audio is None or audio.tags is None:
        return False
    tags = audio.tags

    # ID3 (mp3): unsynchronized lyrics frames.
    if hasattr(tags, "getall"):
        for frame in tags.getall("USLT"):
            if (getattr(frame, "text", "") or "").strip():
                return True

    # MP4/M4A: (c)lyr atom.
    if "\xa9lyr" in tags:
        if any(str(v).strip() for v in tags["\xa9lyr"]):
            return True

    # FLAC/Ogg Vorbis comments: key naming varies by tagger.
    for key in ("lyrics", "unsyncedlyrics", "lyrics-eng"):
        if key in tags:
            vals = tags[key]
            if any(str(v).strip() for v in vals):
                return True

    return False


def has_sidecar_lrc(path: Path) -> bool:
    return path.with_suffix(".lrc").is_file()


def scan(root: Path) -> list[SongEntry]:
    paths = collect_audio_paths(root)
    total = len(paths)
    print(f"Found {total} audio file(s) under {root}")

    entries: list[SongEntry] = []
    for i, path in enumerate(paths, 1):
        title, artist, album, genre = read_common_tags(path)
        entries.append(
            SongEntry(
                path=str(path),
                title=title,
                artist=artist,
                album=album,
                genre=genre,
                has_lrc_file=has_sidecar_lrc(path),
                has_embedded_lyrics=has_embedded_lyrics(path),
            )
        )
        if i % PROGRESS_EVERY == 0 or i == total:
            print(f"  ...{i}/{total}")

    return entries


def load_cache() -> list[SongEntry] | None:
    if not CACHE_PATH.is_file():
        return None
    try:
        data = json.loads(CACHE_PATH.read_text())
    except (OSError, json.JSONDecodeError):
        return None
    try:
        return [SongEntry(**row) for row in data.get("songs", [])]
    except TypeError:
        # Cache predates a SongEntry field (e.g. genre) -- treat as a miss
        # rather than crashing, so it just triggers one fresh scan.
        return None


def save_cache(root: Path, entries: list[SongEntry]) -> None:
    CACHE_PATH.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "directory": str(root),
        "scanned_at": datetime.now(timezone.utc).isoformat(),
        "songs": [asdict(e) for e in entries],
    }
    CACHE_PATH.write_text(json.dumps(payload, indent=2, ensure_ascii=False))
    print(f"Cached {len(entries)} song(s) to {CACHE_PATH}")


def print_summary(entries: list[SongEntry]) -> None:
    total = len(entries)
    lrc = sum(1 for e in entries if e.has_lrc_file)
    embedded = sum(1 for e in entries if e.has_embedded_lyrics)
    either = sum(1 for e in entries if e.has_lrc_file or e.has_embedded_lyrics)
    neither = total - either
    print(f"Total songs:        {total}")
    print(f"With .lrc file:     {lrc}")
    print(f"With embedded:      {embedded}")
    print(f"With either:        {either}")
    print(f"With neither:       {neither}")


def parse_excluded_genres(raw: str) -> set[str]:
    return {g.strip().lower() for g in raw.split(",") if g.strip()}


def matches(entry: SongEntry, needle: str) -> bool:
    return (
        needle in entry.title.lower()
        or needle in entry.artist.lower()
        or needle in entry.album.lower()
    )


def short_path(path_str: str) -> str:
    """Last 2 folders + filename, e.g. '.../Hybrid Theory/In The End.m4a'."""
    parts = Path(path_str).parts
    tail = parts[-3:] if len(parts) >= 3 else parts
    prefix = ".../" if len(parts) > len(tail) else ""
    return prefix + str(Path(*tail))


def print_rows(entries: list[SongEntry]) -> None:
    if not entries:
        print("No matches.")
        return
    title_w = min(max((len(e.title) for e in entries), default=5), 40)
    artist_w = min(max((len(e.artist) for e in entries), default=6), 25)
    album_w = min(max((len(e.album) for e in entries), default=5), 30)
    genre_w = min(max((len(e.genre) for e in entries), default=5), 30)
    header = f"{'Title':<{title_w}}  {'Artist':<{artist_w}}  {'Album':<{album_w}}  {'Genre':<{genre_w}}  LRC  Embedded  Path"
    print(header)
    print("-" * len(header))
    for e in entries:
        print(
            f"{e.title[:title_w]:<{title_w}}  {e.artist[:artist_w]:<{artist_w}}  "
            f"{e.album[:album_w]:<{album_w}}  {e.genre[:genre_w]:<{genre_w}}  {'Y' if e.has_lrc_file else '.':<3}  "
            f"{'Y' if e.has_embedded_lyrics else '.':<8}  {short_path(e.path)}"
        )
    print(f"\n{len(entries)} song(s)")

def fetch_lyrica(artist: str, title: str, timestamps: bool) -> dict | None:
    """One request to the local Lyrica server (see
    https://github.com/Wilooper/Lyrica). Returns the parsed `data` object on
    success, or None on any failure -- network error, non-200, or a
    `{"status": "error"}` envelope. Note that a `timestamps=true` request can
    itself come back *successfully* with `hasTimestamps: false` when only
    untimed lyrics exist (the server already tried the synced sources and
    fell back internally) -- that's a valid result, not a failure, so
    callers should check `data["hasTimestamps"]`, not treat every response
    to a `timestamps=true` request as timed.
    """
    params = {"artist": artist, "song": title}
    if timestamps:
        params["timestamps"] = "true"
    url = f"{LYRICA_URL}?{urllib.parse.urlencode(params)}"
    try:
        resp = requests.get(url, timeout=30)
    except requests.RequestException as exc:
        print(f"  request failed: {exc}")
        return None
    if resp.status_code != 200:
        print(f"  HTTP {resp.status_code}")
        return None
    try:
        payload = resp.json()
    except ValueError:
        print("  non-JSON response")
        return None
    if payload.get("status") != "success":
        message = payload.get("error", {}).get("message", "unknown error")
        print(f"  {message}")
        return None
    return payload.get("data") or None


def plain_lyrics_from_timed(data: dict) -> str:
    """Derives untimed lyrics from a hasTimestamps=true response, preferring
    the clean per-line `timed_lyrics[].text` entries over stripping
    timestamps out of the LRC-formatted `lyrics` string by hand."""
    lines = data.get("timed_lyrics") or []
    if lines:
        return "\n".join(entry.get("text", "") for entry in lines if entry.get("text"))
    stripped = (_LRC_TIMESTAMP_RE.sub("", line).strip() for line in (data.get("lyrics") or "").splitlines())
    return "\n".join(line for line in stripped if line)


def embed_plain_lyrics(path: Path, lyrics: str) -> bool:
    """Writes unsynchronized lyrics into whatever tag format the file
    actually uses -- ID3 USLT for mp3, the MP4 (c)lyr atom for m4a/aac,
    Vorbis Comment LYRICS for flac/ogg/opus. Returns True on success."""
    suffix = path.suffix.lower()
    try:
        if suffix == ".mp3":
            from mutagen.id3 import ID3, ID3NoHeaderError, USLT

            try:
                tags = ID3(path)
            except ID3NoHeaderError:
                tags = ID3()
            tags.delall("USLT")
            tags.add(USLT(encoding=3, lang="eng", desc="", text=lyrics))
            tags.save(path)
            return True
        if suffix in (".m4a", ".aac"):
            from mutagen.mp4 import MP4

            tags = MP4(path)
            tags["\xa9lyr"] = [lyrics]
            tags.save()
            return True
        if suffix in (".flac", ".ogg", ".opus"):
            audio = mutagen.File(path)
            if audio is None:
                return False
            audio["LYRICS"] = lyrics
            audio.save()
            return True
    except Exception as exc:
        print(f"  failed to embed lyrics: {exc}")
        return False
    print(f"  don't know how to embed lyrics into {suffix} files")
    return False


def fetch_entries(entries: list[SongEntry]) -> list[SongEntry]:
    modified_entries = []
    for e in entries:
        print(f"Looking for lyrics for {e.title} by {e.artist}")
        path = Path(e.path)
        found_lrc = e.has_lrc_file
        found_embedded = e.has_embedded_lyrics

        data = fetch_lyrica(e.artist, e.title, timestamps=True)

        if data and data.get("hasTimestamps"):
            timed_text = data.get("lyrics") or ""
            lrc_path = path.with_suffix(".lrc")
            if lrc_path.is_file():
                print("  .lrc already exists, leaving it alone")
                found_lrc = True
            elif timed_text:
                lrc_path.write_text(timed_text)
                found_lrc = True
                print(f"  wrote synced lyrics to {lrc_path.name}")

            plain_text = plain_lyrics_from_timed(data)
            if plain_text and embed_plain_lyrics(path, plain_text):
                found_embedded = True
                print("  embedded plain lyrics (derived from timed) into tags")
        else:
            # No usable response from the timestamps=true request (either it
            # failed outright, or -- see fetch_lyrica's docstring -- it may
            # already have returned untimed lyrics with hasTimestamps=false,
            # in which case `data` is truthy and we skip straight to using
            # it below instead of making a redundant second request).
            if data is None:
                print("  no timed lyrics -- trying untimed")
                time.sleep(random.randint(5, 15))
                data = fetch_lyrica(e.artist, e.title, timestamps=False)

            plain_text = (data or {}).get("lyrics") or ""
            if plain_text and embed_plain_lyrics(path, plain_text):
                found_embedded = True
                print("  embedded untimed lyrics into tags")
            elif not plain_text:
                print("  no lyrics found at all")

        sleep_time = random.randint(5, 15)
        print(f"Waiting {sleep_time} seconds")
        time.sleep(sleep_time)

        e.has_lrc_file = found_lrc
        e.has_embedded_lyrics = found_embedded
        modified_entries.append(e)

    return modified_entries



def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("query", nargs="?", help="Case-insensitive substring to match against title/artist/album")
    parser.add_argument("--refresh", action="store_true", help="Force a fresh rescan of MUSIC_DIR, ignoring any existing cache")
    parser.add_argument("--missing", action="store_true", help="List every song with neither a .lrc file nor embedded lyrics")
    parser.add_argument("--excludegenres", default="", help="Comma-delimited list of genres to exclude")
    args = parser.parse_args()

    if not MUSIC_DIR.is_dir():
        sys.exit(f"MUSIC_DIR does not exist: {MUSIC_DIR} -- edit the constant at the top of this script")

    entries = None if args.refresh else load_cache()
    if entries is None:
        entries = scan(MUSIC_DIR)
        save_cache(MUSIC_DIR, entries)

    excluded = parse_excluded_genres(args.excludegenres)
    if excluded:
        before = len(entries)
        entries = [e for e in entries if e.genre.strip().lower() not in excluded]
        print(f"Excluding {before - len(entries)} song(s) in genres: {', '.join(sorted(excluded))}")

    if args.missing:
        missing_entries = [e for e in entries if not e.has_lrc_file and not e.has_embedded_lyrics]
        missing_entries_updated = fetch_entries(missing_entries)

        print_rows(missing_entries_updated)
        return

    if args.query:
        needle = args.query.lower()
        print_rows([e for e in entries if matches(e, needle)])
        return

    print_summary(entries)


if __name__ == "__main__":
    main()
