#!/usr/bin/env python3
"""Score every bench_analyze.py transcript against the song's real lyrics.

Reference is bench_out/lyrics/<slug>.json (the .lrc-derived ground truth
bench_analyze.py already builds for lyrics-align configs -- see its
build_lyrics_json), not another config's transcript: that's what makes ASR
rows and lyrics_align rows comparable on the same scale. ASR rows show how
much transcription drifted from the real words; lyrics_align rows forced-
align that exact text, so their accuracy should land near 100% and mainly
functions as a sanity check on the scorer itself (see bench_analyze.py's
"Lyrics-alignment configs" docstring section -- only their *timestamps* can
be wrong, not the text).

Fills the `accuracy` column (word accuracy = (1 - WER) * 100, clamped at 0)
in-place in every bench_out/results-*.csv row whose song has a lyrics
reference and whose transcript_path exists on disk. Songs without a
bench_out/lyrics/<slug>.json (no .lrc source, or lyrics not built yet) are
left with accuracy blank -- there's no ground truth to score against.

Usage:
    python3 scripts/bench_score_accuracy.py
    python3 scripts/bench_score_accuracy.py --csv bench_out/results-2026-08-13-13.csv
"""

from __future__ import annotations

import argparse
import csv
import json
import re
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = REPO_ROOT / "bench_out"

_WORD_RE = re.compile(r"[^\w\s']", re.UNICODE)


def normalize(text: str) -> list[str]:
    text = text.lower()
    text = _WORD_RE.sub(" ", text)
    return text.split()


def load_reference(slug: str) -> list[str] | None:
    """Real lyrics for `slug`, or None if bench_out/lyrics/<slug>.json doesn't
    exist -- there's nothing to score ASR/alignment output against for that
    song (see build_lyrics_json in bench_analyze.py, which produces it)."""
    path = OUT_DIR / "lyrics" / f"{slug}.json"
    if not path.is_file():
        return None
    data = json.loads(path.read_text(encoding="utf-8"))
    return normalize(" ".join(data.get("lines", [])))


def load_hypothesis(transcript_path: Path) -> list[str]:
    data = json.loads(transcript_path.read_text(encoding="utf-8"))
    text = " ".join(seg.get("text", "") for seg in data.get("segments", []))
    return normalize(text)


def word_error_rate(ref: list[str], hyp: list[str]) -> float:
    """Standard word-level Levenshtein distance / len(ref)."""
    n, m = len(ref), len(hyp)
    d = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n + 1):
        d[i][0] = i
    for j in range(m + 1):
        d[0][j] = j
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            if ref[i - 1] == hyp[j - 1]:
                d[i][j] = d[i - 1][j - 1]
            else:
                d[i][j] = 1 + min(d[i - 1][j], d[i][j - 1], d[i - 1][j - 1])
    return d[n][m] / n if n else 0.0


def score(slug: str, transcript_path: Path) -> float | None:
    ref = load_reference(slug)
    if ref is None or not transcript_path.is_file():
        return None
    hyp = load_hypothesis(transcript_path)
    rate = word_error_rate(ref, hyp)
    return round(max(0.0, 1 - rate) * 100, 1)


def update_csv(csv_path: Path) -> int:
    with csv_path.open(newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        fieldnames = reader.fieldnames
        rows = list(reader)

    malformed = sum(1 for r in rows if None in r or None in r.values())
    if malformed:
        print(f"  !! {csv_path.name}: {malformed} malformed row(s) (wrong column count) -- left untouched, not scored")

    changed = 0
    for row in rows:
        if None in row or None in row.values():
            continue  # malformed (extra/missing columns) -- don't touch, don't try to rewrite
        slug = row.get("song_slug")
        transcript_rel = row.get("transcript_path")
        if not slug or not transcript_rel:
            continue
        accuracy = score(slug, OUT_DIR / transcript_rel)
        if accuracy is None:
            continue
        new_value = str(accuracy)
        if row.get("accuracy") != new_value:
            row["accuracy"] = new_value
            changed += 1

    if changed:
        # Write to a temp file and swap it in atomically -- a mid-write crash
        # (e.g. a later malformed row DictWriter can't serialize) must never
        # leave csv_path partially overwritten/truncated.
        tmp_path = csv_path.with_suffix(csv_path.suffix + ".tmp")
        with tmp_path.open("w", newline="", encoding="utf-8") as f:
            writer = csv.DictWriter(f, fieldnames=fieldnames, extrasaction="ignore")
            writer.writeheader()
            writer.writerows(rows)
        tmp_path.replace(csv_path)
    return changed


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--csv", nargs="+", type=Path, help="CSV(s) to update (default: all bench_out/results-*.csv)")
    args = parser.parse_args()
    csv_paths = args.csv or sorted(OUT_DIR.glob("results-*.csv"))

    if not csv_paths:
        print(f"No results-*.csv found under {OUT_DIR}")
        return

    for csv_path in csv_paths:
        changed = update_csv(csv_path)
        print(f"{csv_path.name}: {'updated ' + str(changed) + ' row(s)' if changed else 'no changes'}")


if __name__ == "__main__":
    main()
