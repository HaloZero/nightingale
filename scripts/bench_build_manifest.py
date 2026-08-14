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
this after any bench_analyze.py run adds new transcripts, or after
scripts/bench_score_accuracy.py fills in the CSVs' `accuracy` column.

`song_path` was dropped from the CSV schema in a later bench_analyze.py
revision (path is deterministic per slug via SONGS, so it was redundant to
record every run) -- CSVs written since then have no song_path column at
all. For those, fall back to bench_analyze.py's current SONGS entry for the
slug, so a sweep's own songs don't silently disappear from the player just
because their CSV predates -- or postdates -- the column's removal.

Usage:
    python3 scripts/bench_build_manifest.py
"""

import csv
import json
from pathlib import Path

import bench_analyze

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = REPO_ROOT / "bench_out"


def load_song_info() -> dict[str, dict]:
    """slug -> {"note": ..., "path": ...}, from the newest CSV row per slug,
    falling back to bench_analyze.py's SONGS for slugs whose CSV rows have no
    song_path column (see module docstring)."""
    info: dict[str, dict] = {}
    for csv_path in sorted(OUT_DIR.glob("results-*.csv")):
        with csv_path.open(newline="") as f:
            for row in csv.DictReader(f):
                slug = row.get("song_slug")
                if slug and row.get("song_path"):
                    info[slug] = {"note": row.get("song_note", ""), "path": row["song_path"]}

    for song in bench_analyze.SONGS:
        if song["slug"] not in info:
            info[song["slug"]] = {"note": song.get("note", ""), "path": song["path"]}

    return info


def load_timings() -> dict[tuple[str, str], dict]:
    """(song_slug, config_id) -> {"transcribe_or_align_ms": int|None,
    "key_detect_ms": int|None, "accuracy": float|None}, from the newest CSV
    row for that pair. transcribe_or_align_ms is the fair per-config
    comparison metric (see bench_analyze.py's module docstring) -- it's what
    varies between a full-transcription config and a lyrics-alignment one,
    unlike total_wall_ms which is skewed by stem-separation caching.
    accuracy is word accuracy vs. the song's real lyrics, filled in by
    scripts/bench_score_accuracy.py (blank/None until that's been run, or
    for songs with no lyrics reference)."""
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

                def as_float(v):
                    try:
                        return float(v)
                    except (TypeError, ValueError):
                        return None

                timings[(slug, config_id)] = {
                    "transcribe_or_align_ms": as_int(row.get("transcribe_or_align_ms")),
                    "key_detect_ms": as_int(row.get("key_detect_ms")),
                    "accuracy": as_float(row.get("accuracy")),
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
            print(f"  !! {slug}: transcripts exist but no song_path (CSV or SONGS) -- skipping")
            continue
        audio_path = REPO_ROOT / info["path"]
        configs = [
            {
                "id": cid,
                "ms": timings.get((slug, cid), {}).get("transcribe_or_align_ms"),
                "accuracy": timings.get((slug, cid), {}).get("accuracy"),
            }
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
