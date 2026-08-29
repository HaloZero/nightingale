#!/usr/bin/env python3
"""One-off backfill for the `align_backend` field on already-analyzed songs.

`align_backend` records which forced-alignment backend (whisperx/ctc/qwen)
actually produced a song's current word timing (see
app-core/src/song.rs::Song::align_backend and
app-core/analyzer/{align,transcribe,whisper_compat}.py). It's written going
forward by every new analysis/realign, but songs analyzed before this field
existed have neither the transcript JSON key nor the `songs.payload` copy --
there's nothing to read until either this backfill runs or the song is
realigned. This script stamps a value onto both without re-running any
actual alignment.

Only `Lyrics`- and `Generated`-sourced songs go through an aligner at all;
`Lrc` (timing came straight from a provided LRC file) and `Usdx` (its own
bundled timing) never do, so they're left alone.

Locates songs.db and the cache dir the same way app-core does:
NIGHTINGALE_DATA_PATH env var, else ~/.nightingale, else whatever
`data_path`/`cache_paths.songs` config.json points at (same resolution as
scripts/backfill_karaoke_video_status.py).

For each candidate song (analyzed, Lyrics/Generated source, no
`align_backend` in its DB payload yet):
  - if its `<hash>_transcript.json` already has `align_backend` set (a
    partial backfill, or a realign that ran after this script's Python-side
    changes shipped but before the DB copy caught up), sync that value into
    `payload` without touching the JSON;
  - otherwise, stamp the resolved backend value into both the transcript
    JSON and `payload`.

Backend value: `--backend {whisperx,ctc,qwen}` if given, else whatever
`align_backend` is set to in config.json (default `"whisperx"`, matching
`AppConfig::align_backend()`'s own fallback) -- the only backend that could
have run before this setting existed.

Idempotent and safe to re-run: a song whose `align_backend` is already set
in `payload` is skipped entirely.

Usage:
    python3 scripts/backfill_align_backend.py
    python3 scripts/backfill_align_backend.py --backend ctc
    python3 scripts/backfill_align_backend.py --dry-run
    python3 scripts/backfill_align_backend.py --data-dir /path/to/.nightingale
"""

from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
import time
from pathlib import Path

VALID_BACKENDS = ("whisperx", "ctc", "qwen")


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


def resolve_backend(cli_backend: str | None, cfg: dict) -> str:
    if cli_backend:
        return cli_backend
    configured = cfg.get("align_backend")
    return configured if configured in VALID_BACKENDS else "whisperx"


def find_candidates(conn: sqlite3.Connection) -> list[str]:
    rows = conn.execute(
        """
        SELECT file_hash FROM songs
        WHERE json_extract(payload, '$.is_analyzed') = 1
          AND json_extract(payload, '$.transcript_source') IN ('Lyrics', 'Generated')
          AND json_extract(payload, '$.align_backend') IS NULL
        """
    ).fetchall()
    return [r[0] for r in rows]


def backfill_one(
    conn: sqlite3.Connection, cache_dir: Path, file_hash: str, default_backend: str
) -> str:
    """Returns "backfilled", "synced" (JSON already had it, DB didn't), or
    "orphaned" (no readable transcript file)."""
    transcript_path = cache_dir / f"{file_hash}_transcript.json"
    try:
        transcript = json.loads(transcript_path.read_text())
    except (OSError, ValueError):
        return "orphaned"

    outcome = "backfilled"
    backend = transcript.get("align_backend")
    if backend in VALID_BACKENDS:
        outcome = "synced"
    else:
        backend = default_backend
        transcript["align_backend"] = backend
        transcript_path.write_text(json.dumps(transcript, ensure_ascii=False, indent=2))

    payload_row = conn.execute(
        "SELECT payload FROM songs WHERE file_hash = ?", (file_hash,)
    ).fetchone()
    payload = json.loads(payload_row[0])
    payload["align_backend"] = backend
    conn.execute(
        "UPDATE songs SET payload = ? WHERE file_hash = ?",
        (json.dumps(payload), file_hash),
    )
    return outcome


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--data-dir", type=Path, help="Nightingale data dir (default: resolved like the app itself)"
    )
    parser.add_argument(
        "--backend",
        choices=VALID_BACKENDS,
        help="Backend to stamp on songs missing one (default: config.json's align_backend, else whisperx)",
    )
    parser.add_argument(
        "--dry-run", action="store_true", help="Report what would change without writing anything"
    )
    args = parser.parse_args()

    cfg = load_config(config_path())
    data_path = args.data_dir or resolve_data_path(cfg)
    cache_dir = resolve_cache_dir(cfg, data_path)
    db_path = data_path / "songs.db"
    backend = resolve_backend(args.backend, cfg)

    if not db_path.is_file():
        print(f"No database found at {db_path}", file=sys.stderr)
        return 1

    started = time.monotonic()
    print(
        f"Scanning {db_path} for analyzed songs missing align_backend "
        f"(default backend: {backend})" + (" (dry run)" if args.dry_run else "")
    )

    conn = sqlite3.connect(db_path)
    backfilled = synced = orphaned = 0

    try:
        candidates = find_candidates(conn)
        for file_hash in candidates:
            if args.dry_run:
                transcript_path = cache_dir / f"{file_hash}_transcript.json"
                try:
                    transcript = json.loads(transcript_path.read_text())
                except (OSError, ValueError):
                    orphaned += 1
                    print(f"  {file_hash}: no readable transcript at {transcript_path}, skipping")
                    continue
                if transcript.get("align_backend") in VALID_BACKENDS:
                    synced += 1
                else:
                    backfilled += 1
                continue

            outcome = backfill_one(conn, cache_dir, file_hash, backend)
            if outcome == "orphaned":
                orphaned += 1
                print(f"  {file_hash}: no readable transcript, skipping")
            elif outcome == "synced":
                synced += 1
            else:
                backfilled += 1

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
        f"\nDone in {elapsed:.1f}s -- {len(candidates)} candidate(s): "
        f"{backfilled} backfilled with '{backend}', {synced} synced from existing transcript data, "
        f"{orphaned} orphaned (no transcript file)"
        + (" (dry run, nothing written)" if args.dry_run else "")
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
