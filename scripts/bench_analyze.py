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
batch size (8, the default) are held fixed -- see BASELINE below. Parakeet is
excluded entirely (it kept silently falling back to Whisper internally, even
on English input, so it wasn't testing what it claimed to). The matrix is
pruned to skip combinations that are no-ops for a given engine:

  - Whisper MLX ignores align backend -- it derives word timings via DTW over
    its own cross-attention (see `whisper_mlx.py`) but IS affected by model
    size -> 3 configs (one per model size).
  - Whisper is affected by both -> 3 model sizes x 3 align backends = 9
    configs.

Total: 12 configs x N songs.

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

A config can still *silently fall back* at runtime regardless of what was
requested -- that's real signal, not noise, so every run's log is scanned
for the engine actually used and for fallback messages; both land in the
CSV (`effective_engine`, `fallback_detected`) so those rows can be told
apart from a "real" run of the requested engine during analysis.

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
import queue
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# ─── Songs ─────────────────────────────────────────────────────────────
# Picked to cover different artist/genre types, all English, no mashups,
# no TV themes/soundtrack cues -- all mainstream studio releases with a
# definitive, citable source of lyrics (for WER scoring against a
# reference transcript).

SONGS = [
    {
        "slug": "in_the_end",
        "path": "songs/Linkin Park/Hybrid Theory/In The End.m4a",
        "note": "nu-metal/rock",
    },
    {
        "slug": "dog_days_are_over",
        "path": "songs/Florence And The Machine/Lungs/Dog Days Are Over.m4a",
        "note": "indie/alt-pop",
    },
    {
        "slug": "whats_my_age_again",
        "path": "songs/Blink 182/Enema of the State/What's My Age Again.mp3",
        "note": "pop-punk",
    },
    {
        "slug": "surrender",
        "path": "songs/Kasey Chambers/Carnival/Surrender.m4a",
        "note": "country/folk",
    },
    {
        "slug": "one_week",
        "path": "songs/Barenaked Ladies/All Their Greatest Hits/One Week.m4a",
        "note": "fast, dense lyrics",
    },
]

# ─── Config matrix ─────────────────────────────────────────────────────

BASELINE = {
    "beam_size": 16,  # highest the settings UI allows (NumberButtonGroup 1..16)
    "batch_size": 8,  # UI default; not swept in this pass
    "separator": "karaoke",  # UI default; not swept in this pass
}

MODEL_SIZES = ["large-v3", "large_v3_turbo"]
ALIGN_BACKENDS = ["whisperx", "ctc"]
ENGINES = ["whisper", "whisper_mlx"]

NOT_APPLICABLE = "na"


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
ENGINE_USED_RE = re.compile(r"\[nightingale:LOG\] Transcription \((\S+)\):")
FALLBACK_RE = re.compile(r"falling back", re.IGNORECASE)


def parse_log(text: str) -> dict:
    timings = {stage: int(ms) for stage, ms in TIMING_RE.findall(text)}
    engine_used_matches = ENGINE_USED_RE.findall(text)
    return {
        "timings": timings,
        "effective_engine": engine_used_matches[-1] if engine_used_matches else "",
        "fallback_detected": bool(FALLBACK_RE.search(text)),
    }


# ─── One run ────────────────────────────────────────────────────────────

CSV_FIELDS = [
    "run_id",
    "song_slug",
    "song_note",
    "config_id",
    "requested_engine",
    "effective_engine",
    "whisper_model",
    "align_backend",
    "beam_size",
    "batch_size",
    "key_detect_ms",
    "separation_ms",
    "separation_cached",
    "model_load_ms",
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


# If the child produces no output at all for this long, something is
# genuinely wedged mid-run (not just a slow transcribe) -- kill it.
IDLE_TIMEOUT_S = 60 * 60
# Once we've seen PROGRESS:100 (DONE), the pipeline's own work is done --
# give the process this long to actually exit before we conclude it's a
# leftover thread/grandchild holding stdout open and force-kill it. Losing a
# properly-completed run to a stuck exit would be silly, so this is short.
DONE_EXIT_GRACE_S = 30
# How often to print a "still running, last output was at HH:MM:SS" line
# during quiet stretches, so a tailed log always shows a recent, live
# timestamp instead of going silent for up to IDLE_TIMEOUT_S.
HEARTBEAT_INTERVAL_S = 120


def _kill_process_group(proc: subprocess.Popen) -> None:
    """Kill proc and everything in its process group (grandchildren included).

    proc is launched with start_new_session=True so it's its own group
    leader -- this is what lets us reap a stray child (e.g. a lingering
    worker thread/process holding the stdout pipe open) that plain
    proc.terminate() would miss, since that only signals the direct child.
    """
    try:
        pgid = os.getpgid(proc.pid)
    except ProcessLookupError:
        return
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            return
        try:
            proc.wait(timeout=10)
            return
        except subprocess.TimeoutExpired:
            continue


def _run_analyze(cmd: list[str], data_dir: Path) -> tuple[str, int, str, int]:
    """Spawn an analyze.py invocation, stream its output with the idle-timeout
    / DONE-detection machinery, and return (full_log, exit_code, killed_reason,
    wall_ms). Shared by run_one() and the separation warm-up path -- both are
    "run analyze.py, watch stdout, don't hang forever" with the only
    difference being which CLI args get passed in."""
    started = time.perf_counter()
    proc = subprocess.Popen(
        cmd, cwd=str(REPO_ROOT), env=build_env(data_dir),
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1,
        start_new_session=True,  # own process group -- see _kill_process_group
    )

    # Read stdout on a background thread rather than iterating proc.stdout
    # directly on the main thread: a plain `for line in proc.stdout` only
    # unblocks on pipe EOF, which requires *every* fd referencing the pipe's
    # write end to close. If a grandchild/lingering thread outlives the
    # child and keeps that fd open, the main thread would hang forever even
    # though the pipeline itself already finished and printed DONE. Running
    # the read on a thread lets the main loop enforce timeouts instead.
    line_q: "queue.Queue[str | None]" = queue.Queue()

    def _pump() -> None:
        try:
            assert proc.stdout is not None
            for line in proc.stdout:
                line_q.put(line)
        finally:
            line_q.put(None)  # sentinel: reader hit real EOF

    reader = threading.Thread(target=_pump, daemon=True)
    reader.start()

    lines: list[str] = []
    saw_done = False
    done_deadline: float | None = None
    killed_reason = ""
    last_activity = time.perf_counter()
    last_activity_wall = datetime.now().astimezone()  # device-local timezone
    try:
        while True:
            idle_deadline = done_deadline if done_deadline is not None else last_activity + IDLE_TIMEOUT_S
            remaining = max(0.0, idle_deadline - time.perf_counter())
            poll = min(remaining, HEARTBEAT_INTERVAL_S)
            try:
                line = line_q.get(timeout=poll)
            except queue.Empty:
                if time.perf_counter() >= idle_deadline:
                    killed_reason = (
                        f"child didn't exit within {DONE_EXIT_GRACE_S}s of printing DONE"
                        if saw_done else
                        f"no output for {IDLE_TIMEOUT_S}s"
                    )
                    break
                # Still within budget -- just a quiet stretch (e.g. transcribe
                # running with no interim progress lines). Print a heartbeat
                # so "how long has this been sitting here" is answerable at a
                # glance instead of a guess.
                idle_s = int(time.perf_counter() - last_activity)
                print(f"     ... last output at {last_activity_wall.strftime('%H:%M:%S %Z')} ({idle_s}s ago, still running)")
                continue
            if line is None:
                break  # real EOF -- process (and anything else holding the pipe) is gone
            lines.append(line)
            last_activity = time.perf_counter()
            last_activity_wall = datetime.now().astimezone()
            if (
                "[nightingale:STARTING]" in line
                or "[nightingale:TIMING]" in line
                or "Using device:" in line
                or "Transcribing:" in line
                or "falling back" in line.lower()
            ):
                print(f"     [{last_activity_wall.strftime('%H:%M:%S %Z')}] {line.rstrip()}")
            if not saw_done and "[nightingale:PROGRESS:100]" in line:
                saw_done = True
                done_deadline = time.perf_counter() + DONE_EXIT_GRACE_S

        if killed_reason:
            print(f"     !! {killed_reason} -- killing process group (pid {proc.pid})")
            _kill_process_group(proc)
            reader.join(timeout=5)

        try:
            exit_code = proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            # _kill_process_group already escalated to SIGKILL and waited;
            # if the process is still unreaped here (e.g. stuck in
            # uninterruptible D-state), don't let that wedge the sweep too.
            exit_code = -9
    except KeyboardInterrupt:
        _kill_process_group(proc)
        raise
    wall_ms = int((time.perf_counter() - started) * 1000)

    full_log = "".join(lines)
    return full_log, exit_code, killed_reason, wall_ms


def warm_song(song: dict, data_dir: Path, out_dir: Path) -> None:
    """Pre-populate this song's stems cache (key detection + separation only,
    no transcription) so every config's real run in the sweep gets a cache
    hit on separation, instead of whichever config happens to run first for
    a song paying the real ~14-90s cost. Not part of the timed sweep -- no
    CSV row is written."""
    work_dir = out_dir / "work" / song["slug"]
    work_dir.mkdir(parents=True, exist_ok=True)
    audio_path = REPO_ROOT / song["path"]
    file_hash = song_hash(song["slug"])

    if not audio_path.is_file():
        print(f"  !! {song['slug']}: audio file not found: {audio_path}")
        return

    if any(work_dir.glob(f"{file_hash}_vocals_*.mp3")):
        print(f"  -> {song['slug']}: stems already cached, nothing to warm")
        return

    print(f"  -> {song['slug']}: warming stems cache...")
    cmd = [
        str(python_bin(data_dir)),
        str(analyze_py_path(data_dir)),
        str(audio_path),
        str(work_dir),
        "--hash", file_hash,
        "--separator", BASELINE["separator"],
        "--skip-transcription",
    ]
    full_log, exit_code, killed_reason, wall_ms = _run_analyze(cmd, data_dir)

    # --skip-transcription still makes run_pipeline() write a stub
    # {hash}_transcript.json (key/tempo only, no words -- see pipeline.py's
    # skip_transcription branch). Left in place, the first *real* config run
    # for this song would see transcript_exists=True and hit run_pipeline's
    # "already analyzed, skipping" short-circuit, silently producing no
    # transcript at all. Delete it now, same as run_one() does after every
    # real run, so the sweep's own runs start from a clean slate.
    stub_transcript = work_dir / f"{file_hash}_transcript.json"
    if stub_transcript.is_file():
        stub_transcript.unlink()

    if exit_code != 0 or killed_reason:
        reason = killed_reason or f"exit_code={exit_code}"
        tail = full_log[-300:].strip()
        print(f"     !! warm-up failed ({reason}): {tail}")
    else:
        print(f"     done in {wall_ms}ms")


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
            run_id, cfg, song, timings={}, effective_engine="", fallback=False,
            wall_ms=0, exit_code=-1, error=f"audio file not found: {audio_path}",
            transcript_path="", log_path="", stems_cached_going_in=False,
        )

    full_log, exit_code, killed_reason, wall_ms = _run_analyze(cmd, data_dir)

    log_path = logs_dir / f"{cfg['config_id']}.log"
    log_path.write_text(full_log, encoding="utf-8")

    parsed = parse_log(full_log)
    saw_done = "[nightingale:PROGRESS:100]" in full_log

    if killed_reason and saw_done:
        # The pipeline itself completed (we saw DONE and have a full parsed
        # log) -- only the child's *exit* was wedged. Don't punish good data
        # with a FAILED row; just leave a breadcrumb.
        exit_code = 0
        error = f"note: {killed_reason}"
    elif exit_code != 0 or killed_reason:
        tail = full_log[-500:].strip()
        reason = killed_reason or f"exit_code={exit_code}"
        error = f"{reason}: {tail}"
        if killed_reason and exit_code == 0:
            # Kill signal didn't register as a non-zero code (edge case) --
            # force it so main()'s exit_code==0 check doesn't call this OK.
            exit_code = -9
    else:
        error = ""

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
        timings=parsed["timings"],
        effective_engine=parsed["effective_engine"], fallback=parsed["fallback_detected"],
        wall_ms=wall_ms, exit_code=exit_code, error=error,
        transcript_path=transcript_path, log_path=str(log_path.relative_to(out_dir)),
        stems_cached_going_in=stems_cached_going_in,
    )


def _row(run_id, cfg, song, *, timings, effective_engine, fallback, wall_ms, exit_code, error, transcript_path, log_path, stems_cached_going_in) -> dict:
    return {
        "run_id": run_id,
        "song_slug": song["slug"],
        "song_note": song["note"],
        "config_id": cfg["config_id"],
        "requested_engine": cfg["engine"],
        "effective_engine": effective_engine,
        "whisper_model": cfg["model"],
        "align_backend": cfg["align_backend"],
        "beam_size": cfg["beam_size"],
        "batch_size": cfg["batch_size"],
        "key_detect_ms": timings.get("key_detect", ""),
        "separation_ms": timings.get("separation", ""),
        "separation_cached": stems_cached_going_in,
        "model_load_ms": timings.get("model_load", ""),
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
    parser.add_argument("--warm-separation", action="store_true", help="Pre-populate the stems cache for the selected songs (key detection + separation only, no transcription, no CSV row) and exit without running the config sweep -- run this once before a fresh sweep so every config's separation_ms reflects a cache hit instead of whichever config happens to run first for a song paying the real cost")
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

    if args.warm_separation:
        print(f"Data dir:  {args.data_dir}")
        print(f"Out dir:   {out_dir}")
        print(f"Warming stems cache for {len(songs)} song(s)...\n")
        for song in songs:
            warm_song(song, args.data_dir, out_dir)
        return

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
        print(f"     total={row['total_wall_ms']}ms effective_engine={row['effective_engine']} {status}\n")

    print(f"Done. {len(plan)} run(s) written to {csv_path}")


if __name__ == "__main__":
    main()
