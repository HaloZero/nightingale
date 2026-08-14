#!/usr/bin/env python3
"""List every genre tag value found under MUSIC_DIR, with song counts.

Reuses lyrics_inventory's scan cache (same MUSIC_DIR/CACHE_PATH) rather than
rescanning -- pass --refresh to force a fresh scan first (e.g. after tagging
changes).

Usage:
    python3 scripts/list_genres.py              # genre -> count, sorted by count desc
    python3 scripts/list_genres.py --refresh     # force a fresh rescan first
"""

from __future__ import annotations

import argparse
from collections import Counter

import lyrics_inventory as inv


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--refresh", action="store_true", help="Force a fresh rescan of MUSIC_DIR, ignoring any existing cache")
    args = parser.parse_args()

    if not inv.MUSIC_DIR.is_dir():
        raise SystemExit(f"MUSIC_DIR does not exist: {inv.MUSIC_DIR} -- edit the constant in lyrics_inventory.py")

    entries = None if args.refresh else inv.load_cache()
    if entries is None:
        entries = inv.scan(inv.MUSIC_DIR)
        inv.save_cache(inv.MUSIC_DIR, entries)

    counts = Counter(e.genre.strip() or "(none)" for e in entries)
    for e in entries:
        if e.genre.strip() == "" or e.genre.strip() == "Audiobook":
            print(e, e.genre)

    width = max((len(g) for g in counts), default=5)
    for genre, count in counts.most_common():
        print(f"{genre:<{width}}  {count}")
    print(f"\n{len(counts)} distinct genre(s) across {len(entries)} song(s)")


if __name__ == "__main__":
    main()
