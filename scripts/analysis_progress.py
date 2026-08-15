#!/usr/bin/env python3
"""Report analysis progress from the Nightingale library DB.

Locates config.json the same way app-core does (NIGHTINGALE_DATA_PATH env
var, else ~/.nightingale), reads `data_path` from it to find songs.db, then
reports how many songs were analyzed in the past 24 hours and estimates how
long the remaining library will take at that rate.

Usage:
    python3 scripts/analysis_progress.py
"""

from __future__ import annotations

import json
import os
import sqlite3
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path


def default_nightingale_dir() -> Path:
    env = os.environ.get("NIGHTINGALE_DATA_PATH")
    if env:
        return Path(env)
    return Path.home() / ".nightingale"


def config_path() -> Path:
    return default_nightingale_dir() / "config.json"


def resolve_data_path(cfg_path: Path) -> Path:
    """Mirror app-core's configured_data_path(): read `data_path` out of
    config.json, resolve it relative to cwd if it's not absolute, and fall
    back to the default dir if it's missing/empty/unreadable."""
    try:
        cfg = json.loads(cfg_path.read_text())
    except (OSError, json.JSONDecodeError):
        return default_nightingale_dir()

    raw = cfg.get("data_path")
    if not raw:
        return default_nightingale_dir()

    path = Path(raw)
    if not path.is_absolute():
        path = Path.cwd() / path
    return path


def fmt_duration(seconds: float) -> str:
    if seconds < 0:
        seconds = 0
    total = int(round(seconds))
    days, rem = divmod(total, 86400)
    hours, rem = divmod(rem, 3600)
    minutes, secs = divmod(rem, 60)
    parts = []
    if days:
        parts.append(f"{days}d")
    if hours or days:
        parts.append(f"{hours}h")
    if minutes or hours or days:
        parts.append(f"{minutes}m")
    parts.append(f"{secs}s")
    return " ".join(parts)


def main() -> int:
    cfg_path = config_path()
    if not cfg_path.is_file():
        print(f"No config found at {cfg_path}", file=sys.stderr)
        return 1

    data_path = resolve_data_path(cfg_path)
    db_path = data_path / "songs.db"
    if not db_path.is_file():
        print(f"No database found at {db_path}", file=sys.stderr)
        return 1

    print("Nightingale Analysis Progress")
    print("=" * 30)
    print(f"Config:   {cfg_path}")
    print(f"Database: {db_path}")
    print()

    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        total_songs, analyzed_songs = conn.execute(
            "SELECT COUNT(*), COALESCE(SUM(is_analyzed), 0) FROM songs"
        ).fetchone()
        remaining = total_songs - analyzed_songs

        cutoff = (datetime.now(timezone.utc) - timedelta(hours=24)).strftime(
            "%Y-%m-%dT%H:%M:%S.%fZ"
        )
        row = conn.execute(
            """
            SELECT COUNT(*), COUNT(DISTINCT file_hash), AVG(total_ms)
            FROM analysis_timings
            WHERE started_at >= ?
            """,
            (cutoff,),
        ).fetchone()
        events_24h, distinct_songs_24h, avg_ms_24h = row
    finally:
        conn.close()

    pct = (analyzed_songs / total_songs * 100) if total_songs else 0.0
    print(f"Library:    {total_songs} songs")
    print(f"Analyzed:   {analyzed_songs} songs ({pct:.1f}%)")
    print(f"Remaining:  {remaining} songs")
    print()

    print("Last 24 hours:")
    print(f"  Songs analyzed:   {distinct_songs_24h}")
    if events_24h != distinct_songs_24h:
        print(f"  Analysis runs:    {events_24h} (includes re-analysis)")

    if distinct_songs_24h == 0:
        print()
        print("No analysis activity in the past 24 hours -- can't estimate time remaining.")
        return 0

    avg_seconds = (avg_ms_24h or 0) / 1000
    wall_clock_rate_per_hour = distinct_songs_24h / 24
    print(f"  Avg time/song:    {fmt_duration(avg_seconds)}")
    print(f"  Wall-clock rate:  {wall_clock_rate_per_hour:.2f} songs/hour "
          f"({distinct_songs_24h} songs / 24h, includes any idle time)")
    print()

    if remaining <= 0:
        print("Library fully analyzed.")
        return 0

    print(f"Estimated time remaining ({remaining} songs):")
    if avg_seconds > 0:
        active_eta = remaining * avg_seconds
        print(f"  If analysis runs continuously (avg {fmt_duration(avg_seconds)}/song): "
              f"~{fmt_duration(active_eta)}")
    if wall_clock_rate_per_hour > 0:
        wall_eta_hours = remaining / wall_clock_rate_per_hour
        print(f"  At last-24h wall-clock rate ({wall_clock_rate_per_hour:.2f} songs/hour): "
              f"~{fmt_duration(wall_eta_hours * 3600)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
