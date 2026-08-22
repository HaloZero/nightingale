#!/usr/bin/env python3
"""Report analysis progress from the Nightingale library DB.

Locates config.json the same way app-core does (NIGHTINGALE_DATA_PATH env
var, else ~/.nightingale), reads `data_path` from it to find songs.db, then
reports how many songs were analyzed in the lookback window and estimates
how long the remaining library will take at that rate. Also reports on
songs completed via `parallel_analysis` (peer offload) in the same window,
from the separate `parallel_analysis_timings` table -- `analysis_timings`
only has per-stage data for runs the local pipeline actually executed, so
it has nothing to say about songs a peer analyzed. The combined rate/ETA
below accounts for both, since offloaded songs shrink "remaining" too.

Usage:
    python3 scripts/analysis_progress.py              # last 24 hours
    python3 scripts/analysis_progress.py --hours 6     # last 6 hours
    python3 scripts/analysis_progress.py --hours 168   # last week
"""

from __future__ import annotations

import argparse
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


def fmt_window(hours: float) -> str:
    if hours == int(hours) and hours % 24 == 0 and hours > 24:
        return f"{int(hours // 24)}d"
    if hours == int(hours):
        return f"{int(hours)}h"
    return f"{hours}h"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--hours",
        type=float,
        default=24.0,
        help="lookback window in hours for the 'recently analyzed' stats (default: 24)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.hours <= 0:
        print("--hours must be positive", file=sys.stderr)
        return 1
    window = fmt_window(args.hours)

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

        cutoff = (datetime.now(timezone.utc) - timedelta(hours=args.hours)).strftime(
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
        events_window, distinct_songs_window, avg_ms_window = row

        parallel_events_window = 0
        parallel_distinct_window = 0
        parallel_avg_ms_window = None
        parallel_instant_window = 0
        combined_distinct_window = distinct_songs_window
        try:
            row = conn.execute(
                """
                SELECT COUNT(*), COUNT(DISTINCT file_hash), AVG(total_ms),
                       COALESCE(SUM(already_analyzed_on_peer), 0)
                FROM parallel_analysis_timings
                WHERE started_at >= ?
                """,
                (cutoff,),
            ).fetchone()
            (
                parallel_events_window,
                parallel_distinct_window,
                parallel_avg_ms_window,
                parallel_instant_window,
            ) = row

            combined_distinct_window = conn.execute(
                """
                SELECT COUNT(*) FROM (
                    SELECT file_hash FROM analysis_timings WHERE started_at >= ?
                    UNION
                    SELECT file_hash FROM parallel_analysis_timings WHERE started_at >= ?
                )
                """,
                (cutoff, cutoff),
            ).fetchone()[0]
        except sqlite3.OperationalError:
            # `parallel_analysis_timings` doesn't exist yet -- an older
            # app-core that's never run the migration creating it, or this
            # instance has never used parallel analysis. Not an error.
            pass
    finally:
        conn.close()

    pct = (analyzed_songs / total_songs * 100) if total_songs else 0.0
    print(f"Library:    {total_songs} songs")
    print(f"Analyzed:   {analyzed_songs} songs ({pct:.1f}%)")
    print(f"Remaining:  {remaining} songs")
    print()

    print(f"Last {window} (local analysis):")
    print(f"  Songs analyzed:   {distinct_songs_window}")
    if events_window != distinct_songs_window:
        print(f"  Analysis runs:    {events_window} (includes re-analysis)")

    avg_seconds = (avg_ms_window or 0) / 1000
    wall_clock_rate_per_hour = distinct_songs_window / args.hours
    if distinct_songs_window > 0:
        print(f"  Avg time/song:    {fmt_duration(avg_seconds)}")
        print(f"  Wall-clock rate:  {wall_clock_rate_per_hour:.2f} songs/hour "
              f"({distinct_songs_window} songs / {window}, includes any idle time)")

    if parallel_distinct_window > 0:
        parallel_avg_seconds = (parallel_avg_ms_window or 0) / 1000
        parallel_rate_per_hour = parallel_distinct_window / args.hours
        print()
        print(f"Last {window} (parallel analysis / peer offload):")
        print(f"  Songs completed:  {parallel_distinct_window}")
        if parallel_events_window != parallel_distinct_window:
            print(f"  Dispatches:       {parallel_events_window} (includes re-analysis)")
        if parallel_instant_window:
            print(f"  Already on peer:  {parallel_instant_window} "
                  f"(peer had already analyzed it -- fetch-only, no wait)")
        print(f"  Avg time/song:    {fmt_duration(parallel_avg_seconds)} "
              f"(wall-clock: trigger/fetch + wait, not just analysis)")
        print(f"  Rate:             {parallel_rate_per_hour:.2f} songs/hour")

    if combined_distinct_window == 0:
        print()
        print(f"No analysis activity in the past {window} -- can't estimate time remaining.")
        return 0

    combined_rate_per_hour = combined_distinct_window / args.hours
    if parallel_distinct_window > 0:
        print()
        print(f"Combined (local + parallel), last {window}:")
        print(f"  Songs analyzed:   {combined_distinct_window}")
        print(f"  Rate:             {combined_rate_per_hour:.2f} songs/hour")
    print()

    if remaining <= 0:
        print("Library fully analyzed.")
        return 0

    print(f"Estimated time remaining ({remaining} songs):")
    if avg_seconds > 0:
        active_eta = remaining * avg_seconds
        print(f"  If local analysis runs continuously (avg {fmt_duration(avg_seconds)}/song): "
              f"~{fmt_duration(active_eta)}")
    if combined_rate_per_hour > 0:
        wall_eta_hours = remaining / combined_rate_per_hour
        label = "combined local+parallel" if parallel_distinct_window > 0 else f"last-{window}"
        print(f"  At {label} wall-clock rate ({combined_rate_per_hour:.2f} songs/hour): "
              f"~{fmt_duration(wall_eta_hours * 3600)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
