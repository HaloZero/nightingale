#!/usr/bin/env python3
"""Benchmark harness for the Nightingale analyzer pipeline.

Runs `analyze.py` (the vendored, standalone CLI entry point for the full
stem-separation + transcription + alignment pipeline; see
`app-core/analyzer/analyze.py`) across a matrix of ASR configurations and
records per-stage timings to a CSV. This drives the vendored Python venv
directly and never touches the app's library DB or song cache -- safe to run
on any machine that has completed Nightingale's vendor setup, independent of
whether the app itself is installed/running there.

## Matrix

Engine, Whisper model size, and forced-alignment backend are the three
factors under test for this pass; beam size (16, the settings UI's max) and
batch size (8, the default) are held fixed -- see BASELINE below. The matrix
is pruned to skip combinations that are no-ops for a given engine:

  - Parakeet ignores both model size and align backend -- it produces its own
    word timings from a transducer decode (see `parakeet.py`) -> 1 config.
  - Whisper MLX ignores align backend -- it derives word timings via DTW over
    its own cross-attention (see `whisper_mlx.py`) but IS affected by model
    size -> 3 configs (one per model size).
  - Whisper is affected by both -> 3 model sizes x 3 align backends = 9
    configs.

Total: 13 configs x N songs.

## Stem-separation caching -- read this before comparing `total_wall_ms`

Stem separation (Demucs/UVR) runs *before* transcription and depends only on
the audio file + `separator` (held fixed at "karaoke" for this whole sweep),
never on engine/model/align_backend. So it produces byte-identical vocals
audio no matter which config runs afterward, and `separate_and_cache` reuses
it: only the *first* config run for a given song pays the real separation
cost (tens of seconds); every later config for that song gets a near-instant
cache hit. This is intentional -- re-separating 13x per song would burn
~80 extra minutes across the sweep for zero signal, since separation isn't
one of the axes under test and doesn't affect transcript content/accuracy
either way.

It DOES mean `total_wall_ms` is NOT directly comparable across rows for the
same song -- whichever config happened to run first "looks" ~90s slower for
a reason that has nothing to do with that config. The `separation_cached`
column flags this explicitly; the fair per-config comparison metric is
`transcribe_or_align_ms` (the only stage that actually varies with
engine/model/align_backend), not `total_wall_ms`.

Some of those 13 configs will *silently fall back* at runtime regardless of
what was requested -- e.g. Parakeet doesn't support Japanese, so on the
Gundam Wing song it always falls back to Whisper internally. That's real
signal, not noise, so every run's log is scanned for the engine actually
used and for fallback messages; both land in the CSV (`effective_engine`,
`fallback_detected`) so those rows can be told apart from a "real" run of
the requested engine during analysis.

## Accuracy

Not scored yet -- deliberately deferred. Every run's transcript.json is
archived under `<out-dir>/transcripts/<song>/<config_id>.json` so a
follow-up script can diff each variant against a chosen baseline/control
once the accuracy metric is decided. The CSV's `accuracy` column is left
blank as a placeholder.

## Usage

    python3 scripts/bench_analyze.py --list          # show the matrix, don't run
    python3 scripts/bench_analyze.py --smoke-test     # 1 song x 1 small config
    python3 scripts/bench_analyze.py                  # full sweep: 13 configs x 4 songs
    python3 scripts/bench_analyze.py --songs one_week rhythm_emotion
    python3 scripts/bench_analyze.py --engines whisper_mlx
    python3 scripts/bench_analyze.py --data-dir /path/to/.nightingale

Safe to interrupt (Ctrl-C) and re-run: already-completed (song, config) pairs
found in the existing CSV are skipped unless --force is passed. Runs are
strictly sequential -- one model loaded at a time; this pipeline is not
designed for concurrent GPU use, and running two at once would just contend
for the same GPU/VRAM and produce meaningless timings anyway.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import os
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# ─── Songs ─────────────────────────────────────────────────────────────
# Picked to cover different failure modes: fast/dense lyrics, non-English
# (which also exercises the Parakeet -> Whisper language fallback), a
# baseline "regular" song, and a mashup (two songs' worth of vocals/tempo
# changes back to back).

SONGS = [
    {
        "slug": "one_week",
        "path": "songs/Barenaked Ladies/All Their Greatest Hits/One Week.m4a",
        "note": "fast, dense lyrics",
    },
    {
        "slug": "rhythm_emotion",
        "path": "songs/Gundam Wing/Gundam Wing/Rhythm Emotion.mp3",
        "note": "non-English (Japanese) -- Parakeet always falls back here",
    },
    {
        "slug": "dancing_round_the_clock",
        "path": "songs/Happy Days/Happy Days/Dancing Round The Clock.m4a",
        "note": "regular baseline song",
    },
    {
        "slug": "not_fair_lion_man",
        "path": "songs/Lily Allen & Mumford and Sons/Internet Mashup/Not Fair Lion Man.mp3",
        "note": "mashup of two songs",
    },
]

# ─── Config matrix ─────────────────────────────────────────────────────

BASELINE = {
    "beam_size": 16,  # highest the settings UI allows (NumberButtonGroup 1..16)
    "batch_size": 8,  # UI default; not swept in this pass
    "separator": "karaoke",  # UI default; not swept in this pass
}

MODEL_SIZES = ["large-v3", "large-v3-turbo", "medium"]
ALIGN_BACKENDS = ["whisperx", "ctc", "qwen"]
ENGINES = ["whisper", "whisper_mlx", "parakeet"]

NOT_APPLICABLE = "n/a"


def build_matrix(engines: list[str] | None = None) -> list[dict]:
    engines = engines or ENGINES
    configs: list[dict] = []

    if "whisper" in engines:
        for model in MODEL_SIZES:
            for align in ALIGN_BACKENDS:
                configs.append({"engine": "whisper", "model": model, "align_backend": align})

    if "whisper_mlx" in engines:
        for model in MODEL_SIZES:
            configs.append({"engine": "whisper_mlx", "model": model, "align_backend": NOT_APPLICABLE})

    if "parakeet" in engines:
        configs.append({"engine": "parakeet", "model": NOT_APPLICABLE, "align_backend": NOT_APPLICABLE})

    for cfg in configs:
        cfg.update(BASELINE)
        cfg["config_id"] = config_id(cfg)

    return configs


def config_id(cfg: dict) -> str:
    return f"{cfg['engine']}_{cfg['model']}_{cfg['align_backend']}"


# ─── Vendor environment (mirrors app-core/src/analyzer.rs::spawn_server) ──


def default_data_dir() -> Path:
    env_path = os.environ.get("NIGHTINGALE_DATA_PATH")
    if env_path:
        return Path(env_path)
    return Path.home() / ".nightingale"


def python_bin(data_dir: Path) -> Path:
    vendor = data_dir / "vendor"
    if os.name == "nt":
        return vendor / "venv" / "Scripts" / "python.exe"
    return vendor / "venv" / "bin" / "python"


def ffmpeg_bin(data_dir: Path) -> Path:
    name = "ffmpeg.exe" if os.name == "nt" else "ffmpeg"
    return data_dir / "vendor" / name


def analyze_py_path(data_dir: Path) -> Path:
    return data_dir / "vendor" / "analyzer" / "analyze.py"


def build_env(data_dir: Path) -> dict:
    ffmpeg = ffmpeg_bin(data_dir)
    models = data_dir / "models"
    env = dict(os.environ)
    env["PATH"] = f"{ffmpeg.parent}{os.pathsep}{env.get('PATH', '')}"
    env["TORCH_HOME"] = str(models / "torch")
    env["HF_HOME"] = str(models / "huggingface")
    env["FFMPEG_PATH"] = str(ffmpeg)
    env["PYTHONIOENCODING"] = "utf-8"
    env["PYTHONWARNINGS"] = "ignore"
    env["PYTORCH_ENABLE_MPS_FALLBACK"] = "1"
    env["PYTORCH_CUDA_ALLOC_CONF"] = "expandable_segments:True"
    env["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"
    env["NLTK_DATA"] = str(models / "nltk_data")
    env["NEMO_CACHE_DIR"] = str(models / "nemo")
    env["ONNX_ASR_CACHE_DIR"] = str(models / "onnx_asr")
    return env


def check_vendor_ready(data_dir: Path) -> None:
    missing = [
        p
        for p in (python_bin(data_dir), ffmpeg_bin(data_dir), analyze_py_path(data_dir))
        if not p.is_file()
    ]
    if missing:
        joined = "\n  ".join(str(p) for p in missing)
        sys.exit(
            f"Vendor setup looks incomplete at {data_dir} -- missing:\n  {joined}\n"
            "Run Nightingale once on this machine and let first-run setup finish, "
            "then retry (or pass --data-dir)."
        )


# ─── Log parsing ────────────────────────────────────────────────────────

TIMING_RE = re.compile(r"\[nightingale:TIMING\] stage=(\S+) ms=(\d+)")
DEVICE_RE = re.compile(r"Using device: (\S+)")
ENGINE_USED_RE = re.compile(r"\[nightingale:LOG\] Transcription \((\S+)\):")
FALLBACK_RE = re.compile(r"falling back", re.IGNORECASE)


def parse_log(text: str) -> dict:
    timings = {stage: int(ms) for stage, ms in TIMING_RE.findall(text)}
    device_match = DEVICE_RE.search(text)
    engine_used_matches = ENGINE_USED_RE.findall(text)
    return {
        "timings": timings,
        "device": device_match.group(1) if device_match else "",
        "effective_engine": engine_used_matches[-1] if engine_used_matches else "",
        "fallback_detected": bool(FALLBACK_RE.search(text)),
    }


# ─── One run ────────────────────────────────────────────────────────────

CSV_FIELDS = [
    "run_id",
    "song_slug",
    "song_note",
    "song_path",
    "config_id",
    "requested_engine",
    "effective_engine",
    "whisper_model",
    "align_backend",
    "beam_size",
    "batch_size",
    "separator",
    "device",
    "key_detect_ms",
    "separation_ms",
    "separation_cached",
    "transcribe_or_align_ms",
    "total_wall_ms",
    "fallback_detected",
    "exit_code",
    "error",
    "transcript_path",
    "log_path",
    "accuracy",
]


def song_hash(slug: str) -> str:
    return hashlib.blake2b(slug.encode(), digest_size=8).hexdigest()


def run_one(
    cfg: dict,
    song: dict,
    data_dir: Path,
    out_dir: Path,
    run_id: str,
) -> dict:
    work_dir = out_dir / "work" / song["slug"]
    work_dir.mkdir(parents=True, exist_ok=True)
    transcripts_dir = out_dir / "transcripts" / song["slug"]
    transcripts_dir.mkdir(parents=True, exist_ok=True)
    logs_dir = out_dir / "logs" / song["slug"]
    logs_dir.mkdir(parents=True, exist_ok=True)

    audio_path = REPO_ROOT / song["path"]
    file_hash = song_hash(song["slug"])
    transcript_in_work_dir = work_dir / f"{file_hash}_transcript.json"
    # Detected *before* running: whether this song's stems were already on
    # disk going in, so `separation_ms` reflects a cache hit rather than a
    # fresh separation. Key/tempo aren't known ahead of time (they're
    # detected inside the pipeline), so glob rather than build the exact
    # filename -- tempo is always 1.0 and key is deterministic per song, so
    # this glob is unambiguous in practice (one match once separated).
    stems_cached_going_in = any(work_dir.glob(f"{file_hash}_vocals_*.mp3"))

    cmd = [
        str(python_bin(data_dir)),
        str(analyze_py_path(data_dir)),
        str(audio_path),
        str(work_dir),
        "--hash", file_hash,
        "--model", cfg["model"] if cfg["model"] != NOT_APPLICABLE else "large-v3",
        "--beam-size", str(cfg["beam_size"]),
        "--batch-size", str(cfg["batch_size"]),
        "--separator", cfg["separator"],
        "--engine", cfg["engine"],
        "--align-backend", cfg["align_backend"] if cfg["align_backend"] != NOT_APPLICABLE else "whisperx",
    ]

    print(f"  -> {song['slug']} / {cfg['config_id']}")
    if not audio_path.is_file():
        return _row(
            run_id, cfg, song, device="", timings={}, effective_engine="", fallback=False,
            wall_ms=0, exit_code=-1, error=f"audio file not found: {audio_path}",
            transcript_path="", log_path="", stems_cached_going_in=False,
        )

    started = time.perf_counter()
    proc = subprocess.Popen(
        cmd, cwd=str(REPO_ROOT), env=build_env(data_dir),
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1,
    )
    lines: list[str] = []
    try:
        assert proc.stdout is not None
        for line in proc.stdout:
            lines.append(line)
            if "[nightingale:TIMING]" in line or "[nightingale:PROGRESS:2]" in line or "falling back" in line.lower():
                print(f"     {line.rstrip()}")
        exit_code = proc.wait()
    except KeyboardInterrupt:
        proc.terminate()
        proc.wait()
        raise
    wall_ms = int((time.perf_counter() - started) * 1000)

    full_log = "".join(lines)
    log_path = logs_dir / f"{cfg['config_id']}.log"
    log_path.write_text(full_log, encoding="utf-8")

    parsed = parse_log(full_log)

    error = ""
    if exit_code != 0:
        tail = full_log[-500:].strip()
        error = f"exit_code={exit_code}: {tail}"

    transcript_path = ""
    if transcript_in_work_dir.is_file():
        archived = transcripts_dir / f"{cfg['config_id']}.json"
        shutil.copyfile(transcript_in_work_dir, archived)
        transcript_path = str(archived.relative_to(out_dir))
        # Clear it out of the shared work dir so the next config's run for
        # this song doesn't hit run_pipeline's "already analyzed, skipping"
        # short-circuit. Stems (vocals/instrumental mp3s) are left in place
        # on purpose -- those are keyed by hash+key+tempo, not by ASR
        # config, and detect_key/tempo are deterministic per song, so they
        # get reused automatically across every config for this song.
        transcript_in_work_dir.unlink()

    return _row(
        run_id, cfg, song,
        device=parsed["device"], timings=parsed["timings"],
        effective_engine=parsed["effective_engine"], fallback=parsed["fallback_detected"],
        wall_ms=wall_ms, exit_code=exit_code, error=error,
        transcript_path=transcript_path, log_path=str(log_path.relative_to(out_dir)),
        stems_cached_going_in=stems_cached_going_in,
    )


def _row(run_id, cfg, song, *, device, timings, effective_engine, fallback, wall_ms, exit_code, error, transcript_path, log_path, stems_cached_going_in) -> dict:
    return {
        "run_id": run_id,
        "song_slug": song["slug"],
        "song_note": song["note"],
        "song_path": song["path"],
        "config_id": cfg["config_id"],
        "requested_engine": cfg["engine"],
        "effective_engine": effective_engine,
        "whisper_model": cfg["model"],
        "align_backend": cfg["align_backend"],
        "beam_size": cfg["beam_size"],
        "batch_size": cfg["batch_size"],
        "separator": cfg["separator"],
        "device": device,
        "key_detect_ms": timings.get("key_detect", ""),
        "separation_ms": timings.get("separation", ""),
        "separation_cached": stems_cached_going_in,
        "transcribe_or_align_ms": timings.get("transcribe_or_align", ""),
        "total_wall_ms": wall_ms,
        "fallback_detected": fallback,
        "exit_code": exit_code,
        "error": error,
        "transcript_path": transcript_path,
        "log_path": log_path,
        "accuracy": "",
    }


# ─── CSV / resume ───────────────────────────────────────────────────────


def default_csv_filename() -> str:
    """Local-machine time, 24h clock, e.g. results-2026-08-12-19.csv.

    Computed once per invocation (not per row), so a single long-running
    sweep keeps writing to one file even as the clock hour rolls over --
    only *starting a new* invocation gets a new timestamped default. Pass
    --csv explicitly to resume a specific prior file regardless of when it
    was started.
    """
    return f"results-{datetime.now():%Y-%m-%d-%H}.csv"


def load_completed(csv_path: Path) -> set[tuple[str, str]]:
    if not csv_path.is_file():
        return set()
    done = set()
    with csv_path.open(newline="", encoding="utf-8") as f:
        for row in csv.DictReader(f):
            done.add((row["song_slug"], row["config_id"]))
    return done


def append_row(csv_path: Path, row: dict) -> None:
    is_new = not csv_path.is_file()
    with csv_path.open("a", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=CSV_FIELDS)
        if is_new:
            writer.writeheader()
        writer.writerow(row)
        f.flush()


# ─── CLI ────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--data-dir", type=Path, default=default_data_dir(), help="Nightingale data dir (default: $NIGHTINGALE_DATA_PATH or ~/.nightingale)")
    parser.add_argument("--out-dir", type=Path, default=REPO_ROOT / "bench_out", help="Where to write the CSV, per-run logs, and archived transcripts")
    parser.add_argument("--csv", type=Path, default=None, help="CSV path (default: <out-dir>/results-YYYY-MM-DD-HH.csv, local time, fresh per invocation)")
    parser.add_argument("--songs", nargs="+", metavar="SLUG", help="Only run these song slugs")
    parser.add_argument("--engines", nargs="+", choices=ENGINES, help="Only run these engines")
    parser.add_argument("--list", action="store_true", help="Print the planned matrix and exit without running anything")
    parser.add_argument("--force", action="store_true", help="Re-run (song, config) pairs already present in the CSV")
    parser.add_argument("--smoke-test", action="store_true", help="Run exactly one small config on one song, to verify the harness end-to-end before a full sweep")
    args = parser.parse_args()

    songs = SONGS
    if args.songs:
        songs = [s for s in SONGS if s["slug"] in args.songs]
        unknown = set(args.songs) - {s["slug"] for s in SONGS}
        if unknown:
            sys.exit(f"Unknown song slug(s): {', '.join(sorted(unknown))}. Known: {', '.join(s['slug'] for s in SONGS)}")

    configs = build_matrix(args.engines)

    if args.smoke_test:
        # Smallest real matrix entry: cheapest model size, default (fastest)
        # align backend, on a single song. It's a genuine member of the
        # matrix (not a special-cased fake run), so a clean result here also
        # counts as real sweep data -- nothing is wasted.
        configs = [c for c in configs if c["config_id"] == "whisper_medium_whisperx"] or configs[:1]
        songs = songs[:1]

    if args.list:
        print(f"{len(songs)} song(s) x {len(configs)} config(s) = {len(songs) * len(configs)} runs\n")
        print("Songs:")
        for s in songs:
            print(f"  {s['slug']:28s} {s['note']}")
        print("\nConfigs:")
        for c in configs:
            print(f"  {c['config_id']:32s} engine={c['engine']:12s} model={c['model']:14s} align={c['align_backend']}")
        return

    check_vendor_ready(args.data_dir)

    out_dir: Path = args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    csv_path = args.csv or (out_dir / default_csv_filename())

    completed = set() if args.force else load_completed(csv_path)
    run_id = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    plan = [(s, c) for s in songs for c in configs if (s["slug"], c["config_id"]) not in completed]
    skipped = len(songs) * len(configs) - len(plan)
    print(f"Data dir:  {args.data_dir}")
    print(f"Out dir:   {out_dir}")
    print(f"CSV:       {csv_path}")
    print(f"Plan:      {len(plan)} run(s) ({skipped} already in the CSV, skipped)\n")

    for i, (song, cfg) in enumerate(plan, 1):
        print(f"[{i}/{len(plan)}] {song['slug']} / {cfg['config_id']}")
        try:
            row = run_one(cfg, song, args.data_dir, out_dir, run_id)
        except KeyboardInterrupt:
            print("\nInterrupted -- results so far are saved in the CSV; re-run to resume.")
            return
        append_row(csv_path, row)
        status = "OK" if row["exit_code"] == 0 else f"FAILED ({row['error'][:120]})"
        print(f"     total={row['total_wall_ms']}ms device={row['device']} effective_engine={row['effective_engine']} {status}\n")

    print(f"Done. {len(plan)} run(s) written to {csv_path}")


if __name__ == "__main__":
    main()
