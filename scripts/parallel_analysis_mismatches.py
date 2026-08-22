#!/usr/bin/env python3
"""Report songs `parallel_analysis` couldn't match against its peer.

`parallel_analysis` assumes both libraries mirror each other -- same path,
same content hash. Whenever that assumption doesn't hold for a song (the
peer has nothing at that path, or has something with a different hash), it's
recorded in the `parallel_analysis_mismatches` table instead of silently
retrying forever. This script reads that table (locating songs.db the same
way app-core does: NIGHTINGALE_DATA_PATH env var, else ~/.nightingale, else
whatever `data_path` config.json points at).

Usage:
    python3 scripts/parallel_analysis_mismatches.py               # table
    python3 scripts/parallel_analysis_mismatches.py --json         # machine-readable
    python3 scripts/parallel_analysis_mismatches.py --peer http://host:8080
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--json", action="store_true", help="print machine-readable JSON instead of a table")
    parser.add_argument("--peer", help="only show mismatches recorded against this peer URL")
    parser.add_argument(
        "--clear",
        metavar="FILE_HASH",
        help="delete the mismatch row for a specific file hash (e.g. once you've fixed it) and exit",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    cfg_path = config_path()
    if not cfg_path.is_file():
        print(f"No config found at {cfg_path}", file=sys.stderr)
        return 1

    data_path = resolve_data_path(cfg_path)
    db_path = data_path / "songs.db"
    if not db_path.is_file():
        print(f"No database found at {db_path}", file=sys.stderr)
        return 1

    if args.clear:
        conn = sqlite3.connect(db_path)
        try:
            cursor = conn.execute(
                "DELETE FROM parallel_analysis_mismatches WHERE file_hash = ?", (args.clear,)
            )
            conn.commit()
        finally:
            conn.close()
        if cursor.rowcount:
            print(f"Cleared mismatch for {args.clear}")
            return 0
        print(f"No mismatch recorded for {args.clear}", file=sys.stderr)
        return 1

    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        query = """
            SELECT m.file_hash, m.path, m.peer_url, m.peer_hash, m.detected_at,
                   s.title, s.artist
            FROM parallel_analysis_mismatches m
            LEFT JOIN songs s ON s.file_hash = m.file_hash
        """
        params: tuple = ()
        if args.peer:
            query += " WHERE m.peer_url = ?"
            params = (args.peer,)
        query += " ORDER BY m.detected_at DESC"
        rows = conn.execute(query, params).fetchall()
    finally:
        conn.close()

    if args.json:
        print(
            json.dumps(
                [
                    {
                        "file_hash": r[0],
                        "path": r[1],
                        "peer_url": r[2],
                        "peer_hash": r[3],
                        "detected_at": r[4],
                        "title": r[5],
                        "artist": r[6],
                    }
                    for r in rows
                ],
                indent=2,
            )
        )
        return 0

    if not rows:
        print("No parallel-analysis mismatches recorded.")
        return 0

    print(f"{len(rows)} parallel-analysis mismatch(es)")
    print("=" * 30)
    for file_hash, path, peer_url, peer_hash, detected_at, title, artist in rows:
        song_label = f"{artist} - {title}" if title else "(song no longer in library)"
        reason = f"peer has different hash: {peer_hash}" if peer_hash else "peer has nothing at this path"
        print()
        print(f"{song_label}")
        print(f"  path:       {path}")
        print(f"  local hash: {file_hash}")
        print(f"  peer:       {peer_url}")
        print(f"  reason:     {reason}")
        print(f"  detected:   {detected_at}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
