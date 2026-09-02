#!/usr/bin/env python3
"""Report karaoke-video render progress from the Nightingale library DB.

Locates config.json the same way app-core does (NIGHTINGALE_DATA_PATH env
var, else ~/.nightingale), reads `data_path` from it to find songs.db, then
reports how many songs have a rendered karaoke video for each pipeline
("reel" background, "youtube" background) out of the analyzed-song pool
(rendering requires a transcript, so unanalyzed songs aren't candidates),
and estimates how long the remaining library will take at the recent
per-stage rate. Per-stage timings (lookup/download/render) come from the
`karaoke_video_runs` table -- see
app-core/src/library_db/karaoke_video_runs.rs -- which logs one row per
`ensure_karaoke_video`/`ensure_youtube_karaoke_video` invocation regardless
of outcome, so non-render outcomes (skipped_fresh, no_video_found, error)
are broken out separately rather than folded into the render-rate average.

Usage:
    python3 scripts/video_progress.py              # last 24 hours
    python3 scripts/video_progress.py --hours 6     # last 6 hours
    python3 scripts/video_progress.py --hours 168   # last week
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
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--hours",
        type=float,
        default=24.0,
        help="lookback window in hours for the 'recently rendered' stats (default: 24)",
    )
    return parser.parse_args()


def report_pipeline(
    conn: sqlite3.Connection,
    *,
    kind: str,
    label: str,
    version_column: str,
    candidate_pool: int,
    cutoff: str,
    hours: float,
    window: str,
) -> None:
    current = conn.execute(
        f"SELECT COUNT(*) FROM karaoke_video_status WHERE {version_column} > 0"
    ).fetchone()[0]
    remaining = max(candidate_pool - current, 0)

    pct = (current / candidate_pool * 100) if candidate_pool else 0.0
    print(f"{label}:")
    print(f"  Rendered:   {current} / {candidate_pool} analyzed songs ({pct:.1f}%)")
    print(f"  Remaining:  {remaining}")

    status_counts = dict(
        conn.execute(
            """
            SELECT status, COUNT(*)
            FROM karaoke_video_runs
            WHERE kind = ? AND started_at >= ?
            GROUP BY status
            """,
            (kind, cutoff),
        ).fetchall()
    )
    rendered_window = status_counts.get("rendered", 0)
    if not status_counts:
        print(f"  No {kind} runs in the past {window}.")
        print()
        return

    parts = [f"{count} {status}" for status, count in sorted(status_counts.items())]
    print(f"  Last {window}: " + ", ".join(parts))

    if rendered_window > 0:
        row = conn.execute(
            """
            SELECT AVG(lookup_ms), AVG(download_ms), AVG(render_ms), AVG(total_ms)
            FROM karaoke_video_runs
            WHERE kind = ? AND started_at >= ? AND status = 'rendered'
            """,
            (kind, cutoff),
        ).fetchone()
        avg_lookup_ms, avg_download_ms, avg_render_ms, avg_total_ms = row

        if avg_lookup_ms is not None:
            print(f"    Avg lookup:   {fmt_duration(avg_lookup_ms / 1000)}")
        if avg_download_ms is not None:
            print(f"    Avg download: {fmt_duration(avg_download_ms / 1000)}")
        if avg_render_ms is not None:
            print(f"    Avg render:   {fmt_duration(avg_render_ms / 1000)}")
        avg_total_seconds = avg_total_ms / 1000
        print(f"    Avg total:    {fmt_duration(avg_total_seconds)}")

        rate_per_hour = rendered_window / hours
        print(f"    Wall-clock rate: {rate_per_hour:.2f} renders/hour "
              f"({rendered_window} renders / {window}, includes any idle/non-render time --"
              f" renders aren't triggered back-to-back)")

        if remaining > 0:
            active_eta = remaining * avg_total_seconds
            print(f"  Estimated time remaining ({remaining} songs):")
            print(f"    If rendering continuously (avg {fmt_duration(avg_total_seconds)}/song): "
                  f"~{fmt_duration(active_eta)}")
            if rate_per_hour > 0:
                wall_eta_hours = remaining / rate_per_hour
                print(f"    At last-{window} wall-clock rate ({rate_per_hour:.2f}/hour): "
                      f"~{fmt_duration(wall_eta_hours * 3600)}")
    elif remaining > 0:
        print(f"  No successful renders in the past {window} -- can't estimate time remaining.")
    print()


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

    print("Nightingale Karaoke Video Progress")
    print("=" * 35)
    print(f"Config:   {cfg_path}")
    print(f"Database: {db_path}")
    print()

    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        analyzed_songs = conn.execute(
            "SELECT COALESCE(SUM(is_analyzed), 0) FROM songs"
        ).fetchone()[0]
        print(f"Analyzed songs (candidate pool): {analyzed_songs}")
        print()

        cutoff = (datetime.now(timezone.utc) - timedelta(hours=args.hours)).strftime(
            "%Y-%m-%dT%H:%M:%S.%fZ"
        )

        try:
            report_pipeline(
                conn,
                kind="reel",
                label="Reel background",
                version_column="karaoke_video_version",
                candidate_pool=analyzed_songs,
                cutoff=cutoff,
                hours=args.hours,
                window=window,
            )
            report_pipeline(
                conn,
                kind="youtube",
                label="YouTube background",
                version_column="youtube_karaoke_video_version",
                candidate_pool=analyzed_songs,
                cutoff=cutoff,
                hours=args.hours,
                window=window,
            )
        except sqlite3.OperationalError as exc:
            # `karaoke_video_runs`/`karaoke_video_status` don't exist yet --
            # an older app-core that's never run the migration creating
            # them, or this instance has never rendered a karaoke video.
            print(f"Karaoke video tables not available yet: {exc}", file=sys.stderr)
            return 1
    finally:
        conn.close()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
