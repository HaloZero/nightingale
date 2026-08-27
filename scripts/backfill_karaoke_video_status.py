#!/usr/bin/env python3
"""One-off backfill for the `karaoke_video_status` table.

That table caches which songs have a rendered karaoke video (reel
background) and/or a YouTube-background karaoke video, so the song list can
show a mic/YouTube icon per row without stat-ing the cache on every list
render (see app-core/src/karaoke_video.rs and
app-core/src/library_db/karaoke_video_status.rs). It's populated going
forward whenever a render succeeds, but libraries that already had karaoke
videos rendered before that table existed won't show icons for them until
something re-triggers a render/cast for that song. This script backfills
them directly from what's already on disk.

Locates songs.db and the cache dir the same way app-core does:
NIGHTINGALE_DATA_PATH env var, else ~/.nightingale, else whatever
`data_path`/`cache_paths.songs` config.json points at. Scans
<cache_dir>/karaoke_videos/ for `{hash}.mp4` (reel) and `{hash}_youtube.mp4`
(YouTube) files, and for each one whose song still exists in the library:
  - upserts the matching flag in karaoke_video_status (only that flag --
    the other one, if already set, is left alone)
  - patches has_karaoke_video/has_youtube_karaoke_video into that song's
    `payload` JSON blob, mirroring what the Rust app does after a live
    render, so the song list picks it up on its next load with no other
    change needed

Idempotent and safe to re-run: a song whose flag is already set is skipped
entirely (no write), so running this after every deploy costs nothing once
the library's fully backfilled. Cache files with no matching song row
(song deleted/rescanned since the video was rendered) are counted and
left alone.

Usage:
    python3 scripts/backfill_karaoke_video_status.py
    python3 scripts/backfill_karaoke_video_status.py --dry-run
    python3 scripts/backfill_karaoke_video_status.py --data-dir /path/to/.nightingale
"""

from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
import time
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
    """Mirror app-core's `nightingale_dir()`: `data_path` from config.json
    if set, else the default (env-var-aware) dir."""
    raw = cfg.get("data_path")
    if not raw:
        return default_nightingale_dir()
    return resolve_relative(raw)


def resolve_cache_dir(cfg: dict, data_path: Path) -> Path:
    """Mirror app-core's `songs_cache_dir()`: `cache_paths.songs` from
    config.json if set, else `<data_path>/cache`."""
    raw = (cfg.get("cache_paths") or {}).get("songs")
    if not raw:
        return data_path / "cache"
    return resolve_relative(raw)


def classify(filename: str) -> tuple[str, str] | None:
    """Returns (file_hash, "reel"|"youtube") for a karaoke-video cache
    filename, or None if it doesn't match the naming convention
    (`{hash}.mp4` / `{hash}_youtube.mp4` -- see
    `CacheDir::karaoke_video_path`/`youtube_karaoke_video_path`)."""
    if not filename.endswith(".mp4"):
        return None
    stem = filename[: -len(".mp4")]
    if stem.endswith("_youtube"):
        return stem[: -len("_youtube")], "youtube"
    return stem, "reel"


def already_recorded(conn: sqlite3.Connection, file_hash: str, kind: str) -> bool:
    column = "has_karaoke_video" if kind == "reel" else "has_youtube_karaoke_video"
    row = conn.execute(
        f"SELECT {column} FROM karaoke_video_status WHERE file_hash = ?", (file_hash,)
    ).fetchone()
    return bool(row and row[0])


def backfill_one(conn: sqlite3.Connection, file_hash: str, kind: str) -> None:
    """Upserts the one flag for `kind` (leaving the other column alone) and
    patches both flags' current combined state into the song's payload
    JSON -- same two-step `record_karaoke_video_status` does in Rust."""
    column = "has_karaoke_video" if kind == "reel" else "has_youtube_karaoke_video"
    other_column = "has_youtube_karaoke_video" if kind == "reel" else "has_karaoke_video"
    conn.execute(
        f"""
        INSERT INTO karaoke_video_status (file_hash, {column}, {other_column})
        VALUES (?, 1, 0)
        ON CONFLICT(file_hash) DO UPDATE SET {column} = 1
        """,
        (file_hash,),
    )

    row = conn.execute(
        "SELECT has_karaoke_video, has_youtube_karaoke_video FROM karaoke_video_status WHERE file_hash = ?",
        (file_hash,),
    ).fetchone()
    has_karaoke_video, has_youtube_karaoke_video = bool(row[0]), bool(row[1])

    payload_row = conn.execute(
        "SELECT payload FROM songs WHERE file_hash = ?", (file_hash,)
    ).fetchone()
    payload = json.loads(payload_row[0])
    payload["has_karaoke_video"] = has_karaoke_video
    payload["has_youtube_karaoke_video"] = has_youtube_karaoke_video
    conn.execute(
        "UPDATE songs SET payload = ? WHERE file_hash = ?",
        (json.dumps(payload), file_hash),
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--data-dir", type=Path, help="Nightingale data dir (default: resolved like the app itself)"
    )
    parser.add_argument(
        "--dry-run", action="store_true", help="Report what would change without writing anything"
    )
    args = parser.parse_args()

    cfg = load_config(config_path())
    data_path = args.data_dir or resolve_data_path(cfg)
    cache_dir = resolve_cache_dir(cfg, data_path)
    videos_dir = cache_dir / "karaoke_videos"
    db_path = data_path / "songs.db"

    if not db_path.is_file():
        print(f"No database found at {db_path}", file=sys.stderr)
        return 1
    if not videos_dir.is_dir():
        print(f"No karaoke videos cache found at {videos_dir} -- nothing to backfill.")
        return 0

    started = time.monotonic()
    print(f"Scanning {videos_dir}" + (" (dry run)" if args.dry_run else ""))

    conn = sqlite3.connect(db_path)
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS karaoke_video_status (
            file_hash TEXT PRIMARY KEY,
            has_karaoke_video INTEGER NOT NULL DEFAULT 0,
            has_youtube_karaoke_video INTEGER NOT NULL DEFAULT 0
        )
        """
    )

    reel_found = youtube_found = 0
    reel_backfilled = youtube_backfilled = 0
    orphaned = 0

    try:
        for entry in sorted(videos_dir.iterdir()):
            if not entry.is_file():
                continue
            classified = classify(entry.name)
            if classified is None:
                continue
            file_hash, kind = classified
            if kind == "reel":
                reel_found += 1
            else:
                youtube_found += 1

            if already_recorded(conn, file_hash, kind):
                continue

            song_exists = conn.execute(
                "SELECT 1 FROM songs WHERE file_hash = ?", (file_hash,)
            ).fetchone()
            if not song_exists:
                orphaned += 1
                print(f"  {file_hash}: cached {kind} video but no matching song, skipping")
                continue

            if not args.dry_run:
                backfill_one(conn, file_hash, kind)
            if kind == "reel":
                reel_backfilled += 1
            else:
                youtube_backfilled += 1

        if args.dry_run:
            conn.rollback()
        else:
            conn.commit()
    except Exception:
        conn.rollback()
        raise
    finally:
        conn.close()

    elapsed = time.monotonic() - started
    print(
        f"\nDone in {elapsed:.1f}s -- reel {reel_backfilled}/{reel_found} backfilled, "
        f"YouTube {youtube_backfilled}/{youtube_found} backfilled, {orphaned} orphaned cache file(s)"
        + (" (dry run, nothing written)" if args.dry_run else "")
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
