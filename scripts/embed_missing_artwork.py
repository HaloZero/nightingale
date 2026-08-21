#!/usr/bin/env python3
"""One-off: find albums where some tracks have embedded artwork and others
don't, and (optionally) fix the ones missing it by copying the artwork from
a sibling track in the same folder.

An "album" here is just any directory that directly contains audio files --
not assumed to be exactly Artist/Album, so it works regardless of nesting.

Supports reading + writing artwork for: MP3 (ID3 APIC), MP4/M4A (covr),
FLAC (picture blocks), Ogg Vorbis/Opus (METADATA_BLOCK_PICTURE), and WAV
(ID3, same as MP3). WMA (ASF) artwork is only detected, never written --
mutagen has no high-level helper for ASF/WM/Picture and getting the binary
layout wrong risks a corrupt tag, so those tracks are always reported as
"skipped, unsupported" rather than touched.

Defaults to a dry run -- pass --apply to actually write anything. When
applying, every modified file gets a sibling ".bak" copy of the original
written first (skip with --no-backup only once you trust the results).

Usage:
    python3 embed_missing_artwork.py LIBRARY_ROOT [--apply] [--no-backup]
"""

import argparse
import base64
import shutil
import sys
from pathlib import Path

import mutagen
from mutagen.asf import ASF
from mutagen.flac import FLAC, Picture
from mutagen.id3 import APIC, ID3
from mutagen.mp3 import MP3
from mutagen.mp4 import MP4, MP4Cover
from mutagen.oggopus import OggOpus
from mutagen.oggvorbis import OggVorbis
from mutagen.wave import WAVE

# Matches app-core/src/source/folder.rs's AUDIO_EXTENSIONS.
AUDIO_EXTENSIONS = {"mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wma"}

WRITABLE = (MP3, MP4, FLAC, OggVorbis, OggOpus, WAVE)


def load(path: Path):
    try:
        return mutagen.File(path)
    except Exception:
        return None


def get_artwork(audio) -> tuple[bytes, str] | None:
    """First embedded picture's (data, mime), or None if there isn't one
    (or the format isn't one we know how to read pictures from)."""
    if audio is None:
        return None

    if isinstance(audio, MP4):
        covr = audio.tags.get("covr") if audio.tags else None
        if not covr:
            return None
        cov = covr[0]
        mime = "image/png" if cov.imageformat == MP4Cover.FORMAT_PNG else "image/jpeg"
        return bytes(cov), mime

    if isinstance(audio, FLAC):
        if not audio.pictures:
            return None
        pic = audio.pictures[0]
        return pic.data, pic.mime

    if isinstance(audio, (OggVorbis, OggOpus)):
        values = audio.tags.get("metadata_block_picture") if audio.tags else None
        if not values:
            return None
        pic = Picture(base64.b64decode(values[0]))
        return pic.data, pic.mime

    if isinstance(audio, ASF):
        # Detected for reporting purposes only -- never used as a source or
        # target for writing (see module docstring).
        return (b"", "") if "WM/Picture" in audio.tags else None

    # MP3, WAVE: both expose an ID3 tag with APIC frames.
    tags = getattr(audio, "tags", None)
    if tags is not None:
        for key in tags.keys():
            if key.startswith("APIC"):
                frame = tags[key]
                return frame.data, frame.mime
    return None


def embed_artwork(path: Path, audio, data: bytes, mime: str) -> None:
    if isinstance(audio, MP4):
        fmt = MP4Cover.FORMAT_PNG if mime == "image/png" else MP4Cover.FORMAT_JPEG
        audio.tags["covr"] = [MP4Cover(data, imageformat=fmt)]
        audio.save()
        return

    if isinstance(audio, FLAC):
        pic = Picture()
        pic.data = data
        pic.type = 3
        pic.mime = mime
        audio.clear_pictures()
        audio.add_picture(pic)
        audio.save()
        return

    if isinstance(audio, (OggVorbis, OggOpus)):
        pic = Picture()
        pic.data = data
        pic.type = 3
        pic.mime = mime
        b64 = base64.b64encode(pic.write()).decode("ascii")
        audio["metadata_block_picture"] = [b64]
        audio.save()
        return

    # MP3, WAVE
    if audio.tags is None:
        audio.add_tags()
    audio.tags.add(APIC(encoding=3, mime=mime, type=3, desc="Cover", data=data))
    audio.save()


def iter_album_groups(root: Path):
    for dirpath, _dirnames, filenames in __import__("os").walk(root):
        audio_files = [
            Path(dirpath) / name
            for name in filenames
            if name.rsplit(".", 1)[-1].lower() in AUDIO_EXTENSIONS
        ]
        if audio_files:
            yield Path(dirpath), sorted(audio_files)


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("root", type=Path, help="Library root to scan (any depth of Artist/Album/... folders)")
    parser.add_argument("--apply", action="store_true", help="Actually write files (default: dry run / report only)")
    parser.add_argument("--no-backup", action="store_true", help="Skip writing a .bak copy before modifying a file (only with --apply)")
    args = parser.parse_args()

    if not args.root.is_dir():
        print(f"Library root not found: {args.root}", file=sys.stderr)
        return 1

    fixed: list[tuple[Path, Path]] = []  # (target, source)
    unsupported: list[Path] = []
    unreadable: list[Path] = []
    fully_missing_groups: list[Path] = []  # no track in the group has art at all

    for album_dir, files in iter_album_groups(args.root):
        loaded = {f: load(f) for f in files}
        art = {}
        for f, audio in loaded.items():
            if audio is None:
                unreadable.append(f)
                continue
            art[f] = get_artwork(audio)

        with_art = [f for f, a in art.items() if a]
        without_art = [f for f in files if f in art and not art[f]]

        if not with_art:
            if without_art:
                fully_missing_groups.append(album_dir)
            continue
        if not without_art:
            continue  # every track already has art, nothing to do

        source_file = with_art[0]
        source_data, source_mime = art[source_file]

        for target in without_art:
            audio = loaded[target]
            if not isinstance(audio, WRITABLE):
                unsupported.append(target)
                continue

            fixed.append((target, source_file))
            if args.apply:
                if not args.no_backup:
                    shutil.copy2(target, target.with_suffix(target.suffix + ".bak"))
                embed_artwork(target, audio, source_data, source_mime)

    mode = "APPLIED" if args.apply else "DRY RUN (pass --apply to write anything)"
    print(f"=== {mode} ===\n")

    print(f"Fixed: {len(fixed)}")
    for target, source in fixed:
        verb = "embedded" if args.apply else "would embed"
        print(f"  [{verb}] {target} <- art from {source.name}")

    if unsupported:
        print(f"\nSkipped, unsupported format for writing (e.g. WMA): {len(unsupported)}")
        for f in unsupported:
            print(f"  {f}")

    if fully_missing_groups:
        print(f"\nAlbums where NO track has art (nothing to copy from): {len(fully_missing_groups)}")
        for d in fully_missing_groups:
            print(f"  {d}")

    if unreadable:
        print(f"\nUnreadable/unrecognized files: {len(unreadable)}")
        for f in unreadable:
            print(f"  {f}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
