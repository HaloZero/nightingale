#!/usr/bin/env python3
"""One-off: merge stray .lrc files left behind by Apple Music's reorganization
back into the main Artist/Album library.

Assumes SOURCE mirrors the same Artist/Album/Song.lrc layout as DEST, just
incomplete. Matches strictly by relative path: for
SOURCE/<Artist>/<Album>/<Song>.lrc, looks for an audio file named <Song>.*
inside DEST/<Artist>/<Album>/ and drops the .lrc there.

Defaults to a dry run -- pass --apply to actually write anything. Copies by
default (source .lrc files are left alone); pass --move to delete the
source .lrc after a successful copy instead.

Usage:
    python3 merge_leftover_lrc.py SOURCE DEST [--apply] [--move]
"""

import argparse
import os
import shutil
import sys
import unicodedata
from pathlib import Path

# Matches app-core/src/source/folder.rs's AUDIO_EXTENSIONS, so a song is
# recognized here exactly the same way the app's own library scan would.
AUDIO_EXTENSIONS = ["mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wma"]


def nfc(s: str) -> str:
    # macOS commonly hands back NFD-decomposed filenames (accents as separate
    # combining characters); normalize both sides so "Beyoncé" written two
    # different ways still compares equal.
    return unicodedata.normalize("NFC", s)


def iter_leftover_lrc_files(source: Path, dest: Path):
    """Yield every .lrc under source, except any that live inside dest.

    dest is sometimes nested inside source (e.g. Apple Music's reorg left
    stray .lrc files in a folder that also still contains the real,
    already-organized library as a subfolder). Walking into dest would just
    re-discover .lrc files that are already correctly placed, misreport them
    as unmatched because their relative-to-source path is off by dest's own
    prefix, and waste time recursing through the whole real library. Pruned
    with os.walk (rglob can't skip a subtree) so it's never even descended
    into, not just filtered out afterward.
    """
    dest_resolved = dest.resolve()
    for dirpath, dirnames, filenames in os.walk(source):
        dirnames[:] = [d for d in dirnames if (Path(dirpath) / d).resolve() != dest_resolved]
        for name in filenames:
            if name.lower().endswith(".lrc"):
                yield Path(dirpath) / name


def find_matching_audio(dest_album_dir: Path, stem: str) -> Path | None:
    stem_nfc = nfc(stem)
    for ext in AUDIO_EXTENSIONS:
        candidate = dest_album_dir / f"{stem}.{ext}"
        if candidate.is_file():
            return candidate
    # Fall back to a directory listing + NFC comparison, in case the stem
    # itself differs only by unicode normalization.
    if dest_album_dir.is_dir():
        for entry in dest_album_dir.iterdir():
            if not entry.is_file():
                continue
            if entry.suffix.lstrip(".").lower() not in AUDIO_EXTENSIONS:
                continue
            if nfc(entry.stem) == stem_nfc:
                return entry
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("source", type=Path, help="Folder of leftover .lrc files (Artist/Album/Song.lrc)")
    parser.add_argument("dest", type=Path, help="Main organized library (Artist/Album/Song.<ext>)")
    parser.add_argument("--apply", action="store_true", help="Actually write files (default: dry run / report only)")
    parser.add_argument("--move", action="store_true", help="Delete the source .lrc after copying (default: copy, leave source alone)")
    args = parser.parse_args()

    source: Path = args.source
    dest: Path = args.dest

    if not source.is_dir():
        print(f"Source folder not found: {source}", file=sys.stderr)
        return 1
    if not dest.is_dir():
        print(f"Destination library not found: {dest}", file=sys.stderr)
        return 1

    merged: list[tuple[Path, Path]] = []
    already_present: list[Path] = []
    no_album_folder: list[Path] = []
    no_matching_audio: list[Path] = []

    for lrc_path in sorted(iter_leftover_lrc_files(source, dest)):
        rel = lrc_path.relative_to(source)
        dest_album_dir = dest / rel.parent

        if not dest_album_dir.is_dir():
            no_album_folder.append(rel)
            continue

        audio_path = find_matching_audio(dest_album_dir, lrc_path.stem)
        if audio_path is None:
            no_matching_audio.append(rel)
            continue

        target_lrc = audio_path.with_suffix(".lrc")
        if target_lrc.exists():
            already_present.append(rel)
            continue

        merged.append((lrc_path, target_lrc))
        if args.apply:
            shutil.copy2(lrc_path, target_lrc)
            if args.move:
                lrc_path.unlink()

    mode = "APPLIED" if args.apply else "DRY RUN (pass --apply to write anything)"
    print(f"=== {mode} ===\n")

    print(f"Merged: {len(merged)}")
    for src, dst in merged:
        verb = "moved" if args.move else "copied"
        print(f"  [{verb}] {src.relative_to(source)} -> {dst.relative_to(dest)}")

    if already_present:
        print(f"\nSkipped, destination already has an .lrc: {len(already_present)}")
        for rel in already_present:
            print(f"  {rel}")

    if no_matching_audio:
        print(f"\nSkipped, no matching audio file in that album folder: {len(no_matching_audio)}")
        for rel in no_matching_audio:
            print(f"  {rel}")

    if no_album_folder:
        print(f"\nSkipped, no matching Artist/Album folder in dest: {len(no_album_folder)}")
        for rel in no_album_folder:
            print(f"  {rel}")

    print(
        f"\nTotal .lrc scanned: {len(merged) + len(already_present) + len(no_matching_audio) + len(no_album_folder)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
