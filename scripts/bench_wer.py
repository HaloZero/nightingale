#!/usr/bin/env python3
"""Word Error Rate of each completed config's transcript vs. whisper_large-v3_whisperx as reference."""
import json
import re
import sys
from pathlib import Path

OUT_DIR = Path(__file__).resolve().parent.parent / "bench_out"
SONGS = ["sexy_gonna_do_it", "dancing_round_the_clock", "one_week"]
REFERENCE_CONFIG = "whisper_large-v3_whisperx"

# whisperx-aligned candidates only (per user: assume whisperx is the best align backend),
# plus engines that don't have an align_backend choice (mlx). Parakeet dropped --
# it silently falls back to Whisper internally, so its output isn't real Parakeet data.
CANDIDATE_CONFIGS = [
    "whisper_large-v3-turbo_whisperx",
    "whisper_medium_whisperx",
    "whisper_mlx_large-v3_na",
    "whisper_mlx_large-v3-turbo_na",
    "whisper_mlx_medium_na",
]


def load_words(song: str, config_id: str) -> list[str]:
    path = OUT_DIR / "transcripts" / song / f"{config_id}.json"
    data = json.loads(path.read_text())
    text = " ".join(seg.get("text", "") for seg in data["segments"])
    text = text.lower()
    text = re.sub(r"[^\w\s']", " ", text)
    return text.split()


def wer(ref: list[str], hyp: list[str]) -> tuple[float, int, int, int, int]:
    # standard word-level Levenshtein (substitutions, insertions, deletions)
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

    # backtrack to classify edits
    i, j = n, m
    sub = ins = dele = 0
    while i > 0 or j > 0:
        if i > 0 and j > 0 and ref[i - 1] == hyp[j - 1]:
            i, j = i - 1, j - 1
        elif i > 0 and j > 0 and d[i][j] == d[i - 1][j - 1] + 1:
            sub += 1
            i, j = i - 1, j - 1
        elif j > 0 and d[i][j] == d[i][j - 1] + 1:
            ins += 1
            j -= 1
        else:
            dele += 1
            i -= 1
    rate = d[n][m] / n if n else 0.0
    return rate, d[n][m], sub, ins, dele


def main():
    for song in SONGS:
        ref_words = load_words(song, REFERENCE_CONFIG)
        print(f"=== {song} (reference: {REFERENCE_CONFIG}, {len(ref_words)} words) ===")
        results = []
        for config_id in CANDIDATE_CONFIGS:
            path = OUT_DIR / "transcripts" / song / f"{config_id}.json"
            if not path.exists():
                continue
            hyp_words = load_words(song, config_id)
            rate, edits, sub, ins, dele = wer(ref_words, hyp_words)
            accuracy = max(0.0, (1 - rate)) * 100
            results.append((config_id, rate, accuracy, edits, sub, ins, dele, len(hyp_words)))
        results.sort(key=lambda r: r[1])
        print(f"{'config_id':35s} {'WER':>7s} {'accuracy':>9s} {'edits':>6s} {'sub':>4s} {'ins':>4s} {'del':>4s} {'words':>6s}")
        for config_id, rate, accuracy, edits, sub, ins, dele, nwords in results:
            print(f"{config_id:35s} {rate:7.3f} {accuracy:8.1f}% {edits:6d} {sub:4d} {ins:4d} {dele:4d} {nwords:6d}")
        print()


if __name__ == "__main__":
    main()
