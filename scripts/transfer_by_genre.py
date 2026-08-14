#!/usr/bin/env python3
"""Copy every song in an allowed genre list from MUSIC_DIR to DEST_DIR,
preserving MUSIC_DIR's folder structure underneath DEST_DIR. A song's `.lrc`
sidecar (if present) is copied alongside it; embedded lyrics tags travel with
the audio file automatically since they're baked into it.

Genre matching is case-insensitive/trimmed against ALLOWED_GENRES below,
since the library has tagging inconsistencies (e.g. "Indie Rock" vs
"indie Rock").

Reuses lyrics_inventory's scan cache (same MUSIC_DIR/CACHE_PATH) rather than
rescanning -- pass --refresh to force a fresh scan first.

Already-copied files (same destination path + size) are skipped on rerun, so
this is safe to run again after adding genres or new songs.

Usage:
    python3 scripts/transfer_by_genre.py               # copy matching songs
    python3 scripts/transfer_by_genre.py --dry-run      # preview without copying
    python3 scripts/transfer_by_genre.py --refresh      # force a fresh rescan first
"""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path

import lyrics_inventory as inv

# --- Edit these for your setup -------------------------------------------
DEST_DIR = Path("/Volumes/MediaDiskPortable/songs")
ALLOWED_GENRES = {
    "rock", "pop", "classic rock", "musical", "pop rock", "alternative rock",
    "alternative", "punk rock", "animation", "hip hop", "ska", "comedy",
    "indie", "hard rock", "rap", "r&b", "house", "new wave", "indie rock",
    "oldies", "orchestra", "metal", "progressive rock", "electro rock",
    "indie pop", "dance", "folk", "electro", "funk", "alternative pop",
    "dance punk", "drum & bass", "disco", "television", "country",
    "rockabilly", "chorus", "new age", "soul", "dubstep", "carribean",
    "experimental", "longue", "grunge", "eurodance", "pop punk", "jazz",
    "easy listening", "shoegaze", "trap", "blues", "bluegrass", "reggae",
}
# ---------------------------------------------------------------------------


def matches_allowed_genre(genre: str) -> bool:
    return genre.strip().lower() in ALLOWED_GENRES


def copy_if_needed(src: Path, dest: Path, dry_run: bool) -> str:
    """Returns "copied", "skipped" (already present), or "missing" (src gone)."""
    if not src.is_file():
        return "missing"
    if dest.is_file() and dest.stat().st_size == src.stat().st_size:
        return "skipped"
    if dry_run:
        return "copied"
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dest)
    return "copied"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dry-run", action="store_true", help="Print what would be copied without touching DEST_DIR")
    parser.add_argument("--refresh", action="store_true", help="Force a fresh rescan of MUSIC_DIR, ignoring any existing cache")
    args = parser.parse_args()

    if not inv.MUSIC_DIR.is_dir():
        raise SystemExit(f"MUSIC_DIR does not exist: {inv.MUSIC_DIR} -- edit the constant in lyrics_inventory.py")
    if not args.dry_run and not DEST_DIR.parent.is_dir():
        raise SystemExit(f"DEST_DIR's parent volume isn't mounted: {DEST_DIR}")

    entries = None if args.refresh else inv.load_cache()
    if entries is None:
        entries = inv.scan(inv.MUSIC_DIR)
        inv.save_cache(inv.MUSIC_DIR, entries)

    matched = [e for e in entries if matches_allowed_genre(e.genre)]
    print(f"{len(matched)}/{len(entries)} song(s) match an allowed genre")
    if args.dry_run:
        print("(dry run -- nothing will be copied)")

    counts = {"copied": 0, "skipped": 0, "missing": 0}
    lrc_counts = {"copied": 0, "skipped": 0, "missing": 0}

    for i, e in enumerate(matched, 1):
        src = Path(e.path)
        rel = src.relative_to(inv.MUSIC_DIR)
        dest = DEST_DIR / rel

        result = copy_if_needed(src, dest, args.dry_run)
        counts[result] += 1
        if result == "missing":
            print(f"  !! missing source file: {src}")

        if e.has_lrc_file:
            lrc_src = src.with_suffix(".lrc")
            lrc_dest = dest.with_suffix(".lrc")
            lrc_result = copy_if_needed(lrc_src, lrc_dest, args.dry_run)
            lrc_counts[lrc_result] += 1

        if i % 200 == 0 or i == len(matched):
            print(f"  ...{i}/{len(matched)}")

    print(f"\nAudio: copied={counts['copied']} skipped={counts['skipped']} missing={counts['missing']}")
    print(f"LRC:   copied={lrc_counts['copied']} skipped={lrc_counts['skipped']} missing={lrc_counts['missing']}")


if __name__ == "__main__":
    main()
