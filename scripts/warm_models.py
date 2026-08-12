#!/usr/bin/env python3
"""Pre-download every model the benchmark matrix (bench_analyze.py) needs.

Run this before starting the timed sweep. Without it, whichever song runs
first for a given (engine, model size, align backend) combination pays that
model's one-time download cost as part of its `transcribe_or_align_ms` --
for a large model that can be minutes of pure network time baked into what's
supposed to be an inference-speed measurement.

Delegates the actual downloading to _warm_models_inner.py, run inside the
vendored venv (the same one bench_analyze.py drives) so it has whisperx /
onnx_asr / mlx_whisper / transformers available and writes into the same
HF_HOME / ONNX_ASR_CACHE_DIR / TORCH_HOME the real pipeline reads from.

Usage:
    python3 scripts/warm_models.py
    python3 scripts/warm_models.py --data-dir /Volumes/MediaDiskPortable/nightingale
"""

import argparse
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bench_analyze import build_env, check_vendor_ready, default_data_dir, python_bin  # noqa: E402


def analyzer_dir(data_dir: Path) -> Path:
    return data_dir / "vendor" / "analyzer"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--data-dir", type=Path, default=default_data_dir(),
        help="Nightingale data dir (default: $NIGHTINGALE_DATA_PATH or ~/.nightingale)",
    )
    args = parser.parse_args()

    check_vendor_ready(args.data_dir)

    inner = Path(__file__).resolve().parent / "_warm_models_inner.py"
    cmd = [str(python_bin(args.data_dir)), str(inner), str(analyzer_dir(args.data_dir))]

    proc = subprocess.run(cmd, env=build_env(args.data_dir))
    sys.exit(proc.returncode)


if __name__ == "__main__":
    main()
