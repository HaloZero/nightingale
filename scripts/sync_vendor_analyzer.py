#!/usr/bin/env python3
"""Copy app-core/analyzer/*.py straight into the vendor data dir's analyzer/,
bypassing the compiled app entirely.

`app-core/src/vendor_scripts.rs` embeds each analyzer .py file via
`include_str!` at *compile* time and only writes it out to
`<data_dir>/vendor/analyzer/` when the Tauri app calls
`refresh_analyzer_scripts_if_ready()` on startup -- so even running the app
won't pick up local edits without a `cargo build` first. bench_analyze.py
(and any other script driving the vendor python directly) reads whatever's
sitting in `<data_dir>/vendor/analyzer/`, so for iterating on analyzer code
without a Rust rebuild, this script is the fast path: same source files,
same destination, no compile step.

The file list is parsed out of vendor_scripts.rs's `FILES` array rather than
duplicated here, so it can't drift if a new analyzer file is added there.

Usage:
    python3 scripts/sync_vendor_analyzer.py
    python3 scripts/sync_vendor_analyzer.py --data-dir /path/to/.nightingale
    python3 scripts/sync_vendor_analyzer.py --dry-run   # report only, exit 1 if out of sync

check_sync() is also imported directly by bench_analyze.py to gate a sweep on
the vendor being current -- see its --skip-vendor-sync-check flag.
"""

import argparse
import filecmp
import os
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ANALYZER_SRC = REPO_ROOT / "app-core" / "analyzer"
VENDOR_SCRIPTS_RS = REPO_ROOT / "app-core" / "src" / "vendor_scripts.rs"

FILENAME_RE = re.compile(r'\("([\w.]+\.py)",')


def default_data_dir() -> Path:
    env_path = os.environ.get("NIGHTINGALE_DATA_PATH")
    if env_path:
        return Path(env_path)
    return Path.home() / ".nightingale"


def parse_file_list() -> list[str]:
    text = VENDOR_SCRIPTS_RS.read_text(encoding="utf-8")
    names = FILENAME_RE.findall(text)
    if not names:
        sys.exit(f"Couldn't find any (\"*.py\", ...) entries in {VENDOR_SCRIPTS_RS} -- did its format change?")
    return names


def vendor_analyzer_dir(data_dir: Path) -> Path:
    return data_dir / "vendor" / "analyzer"


def check_sync(data_dir: Path) -> tuple[list[str], list[str], list[str]]:
    """Compares app-core/analyzer/*.py against <data_dir>/vendor/analyzer/*.py
    without writing anything. Returns (out_of_sync, unchanged, missing_source)
    -- "out of sync" covers both a differing file and one absent from the
    vendor dir entirely (e.g. a file added since the vendor was last synced).
    """
    dest_dir = vendor_analyzer_dir(data_dir)
    out_of_sync, unchanged, missing = [], [], []
    for name in parse_file_list():
        src = ANALYZER_SRC / name
        dst = dest_dir / name
        if not src.is_file():
            missing.append(name)
        elif dst.is_file() and filecmp.cmp(src, dst, shallow=False):
            unchanged.append(name)
        else:
            out_of_sync.append(name)
    return out_of_sync, unchanged, missing


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--data-dir", type=Path, default=default_data_dir(), help="Nightingale data dir (default: $NIGHTINGALE_DATA_PATH or ~/.nightingale)")
    parser.add_argument("--dry-run", action="store_true", help="Report out-of-sync/missing files without copying; exit 1 if anything would change")
    args = parser.parse_args()

    dest_dir = vendor_analyzer_dir(args.data_dir)
    if not dest_dir.is_dir():
        sys.exit(f"Vendor analyzer dir not found: {dest_dir}\nRun Nightingale once to complete first-run setup, or pass --data-dir.")

    print(f"Source: {ANALYZER_SRC}")
    print(f"Dest:   {dest_dir}\n")

    out_of_sync, unchanged, missing = check_sync(args.data_dir)

    if args.dry_run:
        for name in out_of_sync:
            print(f"  OUT OF SYNC  {name}")
        for name in unchanged:
            print(f"  unchanged    {name}")
        for name in missing:
            print(f"  !! missing source file: {ANALYZER_SRC / name}")
        if out_of_sync or missing:
            print(f"\n{len(out_of_sync)} out of sync, {len(missing)} missing source file(s) -- run without --dry-run to sync.")
            sys.exit(1)
        print(f"\nAll {len(unchanged)} analyzer files in sync.")
        return

    for name in out_of_sync:
        (dest_dir / name).write_bytes((ANALYZER_SRC / name).read_bytes())
        print(f"  copied     {name}")
    for name in unchanged:
        print(f"  unchanged  {name}")
    for name in missing:
        print(f"  !! missing source file, skipped: {ANALYZER_SRC / name}")

    print(f"\n{len(out_of_sync)} copied, {len(unchanged)} already up to date, {len(missing)} missing")


if __name__ == "__main__":
    main()
