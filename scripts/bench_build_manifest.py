#!/usr/bin/env python3
"""Build bench_out/player_manifest.json for scripts/bench_player.html.

The player is a static page served by `python3 -m http.server` from the repo
root -- it can't list directories itself, so this script is the one place
that knows how to turn "whatever transcripts happen to exist on disk" into
the JSON the page fetches.

song_path/song_note come from the results CSVs (the actual historical
record of what ran), not from bench_analyze.py's SONGS -- that list is the
*next* sweep to run and can move on without leaving the player unable to
find audio for transcripts that already exist from a past sweep. Re-run
this after any bench_analyze.py run adds new transcripts.

Usage:
    python3 scripts/bench_build_manifest.py
"""

import csv
import json
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = REPO_ROOT / "bench_out"


def load_song_info() -> dict[str, dict]:
    """slug -> {"note": ..., "path": ...}, from the newest CSV row per slug."""
    info: dict[str, dict] = {}
    for csv_path in sorted(OUT_DIR.glob("results-*.csv")):
        with csv_path.open(newline="") as f:
            for row in csv.DictReader(f):
                slug = row.get("song_slug")
                if slug and row.get("song_path"):
                    info[slug] = {"note": row.get("song_note", ""), "path": row["song_path"]}
    return info


def load_timings() -> dict[tuple[str, str], dict]:
    """(song_slug, config_id) -> {"transcribe_or_align_ms": int|None, "key_detect_ms": int|None},
    from the newest CSV row for that pair. transcribe_or_align_ms is the fair
    per-config comparison metric (see bench_analyze.py's module docstring) --
    it's what varies between a full-transcription config and a lyrics-alignment
    one, unlike total_wall_ms which is skewed by stem-separation caching."""
    timings: dict[tuple[str, str], dict] = {}
    for csv_path in sorted(OUT_DIR.glob("results-*.csv")):
        with csv_path.open(newline="") as f:
            for row in csv.DictReader(f):
                slug, config_id = row.get("song_slug"), row.get("config_id")
                if not slug or not config_id:
                    continue

                def as_int(v):
                    try:
                        return int(v)
                    except (TypeError, ValueError):
                        return None

                timings[(slug, config_id)] = {
                    "transcribe_or_align_ms": as_int(row.get("transcribe_or_align_ms")),
                    "key_detect_ms": as_int(row.get("key_detect_ms")),
                }
    return timings


def main() -> None:
    song_info = load_song_info()
    timings = load_timings()
    transcripts_root = OUT_DIR / "transcripts"

    songs = []
    for slug_dir in sorted(p for p in transcripts_root.iterdir() if p.is_dir()) if transcripts_root.is_dir() else []:
        slug = slug_dir.name
        config_ids = sorted(p.stem for p in slug_dir.glob("*.json"))
        if not config_ids:
            continue
        info = song_info.get(slug)
        if not info:
            print(f"  !! {slug}: transcripts exist but no CSV row has its song_path -- skipping")
            continue
        audio_path = REPO_ROOT / info["path"]
        configs = [
            {"id": cid, "ms": timings.get((slug, cid), {}).get("transcribe_or_align_ms")}
            for cid in config_ids
        ]
        songs.append(
            {
                "slug": slug,
                "note": info["note"],
                "audio_url": "/" + info["path"],
                "audio_exists": audio_path.is_file(),
                "configs": configs,
            }
        )

    manifest_path = OUT_DIR / "player_manifest.json"
    manifest_path.write_text(json.dumps({"songs": songs}, indent=2))
    print(f"Wrote {manifest_path} ({len(songs)} song(s))")
    for s in songs:
        flag = "" if s["audio_exists"] else "  !! audio file missing"
        print(f"  {s['slug']:25s} {len(s['configs'])} config(s){flag}")


if __name__ == "__main__":
    main()
