#!/usr/bin/env python3
"""Export a song's cached transcript as an Enhanced LRC file.

Nightingale never writes an `.lrc` sidecar into your music folder itself
(see app-core/src/lyrics.rs / cache.rs -- fetched/aligned lyrics only ever
live in the app's own cache, as `<hash>_transcript.json`, word-level start/
end timestamps). This script reads that cached transcript and converts it
to Enhanced LRC (`[mm:ss.xx]<mm:ss.xx>word <mm:ss.xx>word ...`), the same
word-tagged format app-core/src/lrc.rs's parser understands, so you can
save it next to the audio file yourself (or anywhere else) if you want a
portable copy of the current alignment.

Locates songs.db and the cache dir the same way app-core does
(NIGHTINGALE_DATA_PATH env var, else ~/.nightingale, else whatever
`data_path`/`cache_paths.songs` config.json points at).

Usage:
    python3 scripts/export_lrc.py <file_hash>
    python3 scripts/export_lrc.py --search "toxic britney"
    python3 scripts/export_lrc.py --search "toxic" -o "/path/to/Toxic.lrc"
"""

from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
from pathlib import Path


def default_nightingale_dir() -> Path:
    env = os.environ.get("NIGHTINGALE_DATA_PATH")
    if env:
        return Path(env)
    return Path.home() / ".nightingale"


def config_path() -> Path:
    return default_nightingale_dir() / "config.json"


def load_config(cfg_path: Path) -> dict:
    try:
        return json.loads(cfg_path.read_text())
    except (OSError, json.JSONDecodeError):
        return {}


def resolve_relative(raw: str) -> Path:
    path = Path(raw)
    return path if path.is_absolute() else Path.cwd() / path


def resolve_data_path(cfg: dict) -> Path:
    raw = cfg.get("data_path")
    if not raw:
        return default_nightingale_dir()
    return resolve_relative(raw)


def resolve_cache_dir(cfg: dict, data_path: Path) -> Path:
    raw = (cfg.get("cache_paths") or {}).get("songs")
    if not raw:
        return data_path / "cache"
    return resolve_relative(raw)


def find_songs(conn: sqlite3.Connection, search: str) -> list[tuple[str, str, str, str]]:
    """(file_hash, title, artist, album) rows whose title or artist contains
    every word in `search`, case-insensitively."""
    words = search.lower().split()
    rows = conn.execute("SELECT file_hash, title, artist, album FROM songs").fetchall()
    matches = []
    for file_hash, title, artist, album in rows:
        haystack = f"{title} {artist}".lower()
        if all(word in haystack for word in words):
            matches.append((file_hash, title, artist, album))
    return matches


def format_timestamp(seconds: float) -> str:
    minutes = int(seconds // 60)
    secs = seconds - minutes * 60
    return f"{minutes:02d}:{secs:05.2f}"


def transcript_to_lrc(transcript: dict) -> str:
    lines = []
    for segment in transcript.get("segments", []):
        words = segment.get("words") or []
        if words:
            line_ts = format_timestamp(words[0]["start"])
            word_tags = "".join(
                f"<{format_timestamp(w['start'])}>{w['word']} " for w in words
            ).rstrip()
            lines.append(f"[{line_ts}]{word_tags}")
        else:
            # No word-level timing (e.g. stems-only/LRC-provided songs) --
            # fall back to a plain line-level tag from the segment's own span.
            line_ts = format_timestamp(segment["start"])
            text = segment.get("text", "").strip()
            if text:
                lines.append(f"[{line_ts}]{text}")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("file_hash", nargs="?", help="Exact file hash to export")
    parser.add_argument("--search", help="Find a song by title/artist substring instead")
    parser.add_argument(
        "-o", "--output", type=Path, help="Write to this path instead of stdout"
    )
    parser.add_argument(
        "--data-dir", type=Path, help="Nightingale data dir (default: resolved like the app itself)"
    )
    args = parser.parse_args()

    if not args.file_hash and not args.search:
        print("Provide either a file_hash or --search", file=sys.stderr)
        return 1

    cfg = load_config(config_path())
    data_path = args.data_dir or resolve_data_path(cfg)
    cache_dir = resolve_cache_dir(cfg, data_path)
    db_path = data_path / "songs.db"

    if not db_path.is_file():
        print(f"No database found at {db_path}", file=sys.stderr)
        return 1

    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)

    if args.search:
        matches = find_songs(conn, args.search)
        if not matches:
            print(f"No songs matched \"{args.search}\"", file=sys.stderr)
            return 1
        if len(matches) > 1:
            print(f"{len(matches)} songs matched \"{args.search}\" -- be more specific, or pass the hash directly:", file=sys.stderr)
            for file_hash, title, artist, album in matches:
                print(f"  {file_hash}  {artist} - {title} ({album})", file=sys.stderr)
            return 1
        file_hash, title, artist, _album = matches[0]
        print(f"Matched: {artist} - {title} ({file_hash})", file=sys.stderr)
    else:
        file_hash = args.file_hash

    transcript_path = cache_dir / f"{file_hash}_transcript.json"
    if not transcript_path.is_file():
        print(f"No cached transcript at {transcript_path} -- analyze this song first", file=sys.stderr)
        return 1

    transcript = json.loads(transcript_path.read_text())
    lrc_text = transcript_to_lrc(transcript)

    if args.output:
        args.output.write_text(lrc_text)
        print(f"Wrote {args.output}", file=sys.stderr)
    else:
        sys.stdout.write(lrc_text)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
