#!/usr/bin/env python3
"""Downloads every model the benchmark matrix (bench_analyze.py) needs.

Runs *inside the vendored venv*, invoked by warm_models.py -- not meant to
be run directly (it needs whisperx/mlx_whisper/onnx_asr/transformers on the
path, which only exist there). Reuses the real analyzer modules (whisperx,
parakeet, cjk, qwen_align, whisper_mlx) for model ids and loader functions
so this can't silently drift from what the pipeline actually loads at
runtime -- if those modules change, this changes with them.

No audio is processed; each model is loaded once (instantiate + drop) purely
to force its weights onto disk, so the timed sweep's first rows aren't
contaminated by one-time download latency.

Usage: <vendored-python> _warm_models_inner.py <analyzer_dir>
"""

import sys
import traceback

ANALYZER_DIR = sys.argv[1]
sys.path.insert(0, ANALYZER_DIR)

from whisper_compat import compute_type_for, detect_device  # noqa: E402

DEVICE = detect_device()
COMPUTE_TYPE = compute_type_for(DEVICE)
ALIGN_DEVICE = "cpu" if DEVICE == "mps" else DEVICE

# Matches bench_analyze.py's MODEL_SIZES and the languages the 4 benchmark
# songs actually use (3 English + 1 Japanese, Gundam Wing).
MODEL_SIZES = ["large-v3", "large-v3-turbo", "medium"]
ALIGN_LANGUAGES = ["en", "ja"]

results: list[tuple[str, bool, str]] = []


def attempt(label: str, fn) -> None:
    print(f"==> {label}", flush=True)
    try:
        fn()
        print("    ok", flush=True)
        results.append((label, True, ""))
    except Exception as e:
        print(f"    FAILED: {e}", flush=True)
        traceback.print_exc()
        results.append((label, False, str(e)))


def warm_whisper_ct2() -> None:
    import whisperx

    for size in MODEL_SIZES:
        def _load(size=size):
            model = whisperx.load_model(size, DEVICE, compute_type=COMPUTE_TYPE, task="transcribe")
            del model

        attempt(f"whisper CT2 model: {size}", _load)


def warm_align_models() -> None:
    import cjk
    import whisperx

    for lang in ALIGN_LANGUAGES:
        model_name = cjk.align_model_for(lang)  # None -> WhisperX's own default for the language

        def _load(lang=lang, model_name=model_name):
            align_model, metadata = whisperx.load_align_model(
                language_code=cjk.align_lang_code(lang), device=ALIGN_DEVICE, model_name=model_name,
            )
            del align_model, metadata

        label = f"align model ({lang}): {model_name or 'whisperx default'}"
        attempt(label, _load)


def warm_qwen() -> None:
    import qwen_align
    from transformers import AutoModelForTokenClassification, AutoProcessor

    def _load():
        AutoProcessor.from_pretrained(qwen_align.QWEN_MODEL_ID)
        AutoModelForTokenClassification.from_pretrained(qwen_align.QWEN_MODEL_ID)

    attempt(f"qwen align model: {qwen_align.QWEN_MODEL_ID}", _load)


def warm_parakeet() -> None:
    import parakeet

    attempt(f"parakeet onnx model: {parakeet.ONNX_MODEL_ID}", parakeet._load_onnx)


def warm_whisper_mlx() -> None:
    import whisper_mlx

    if not whisper_mlx.is_available():
        print(
            "==> whisper_mlx: mlx not importable on this machine -- skipping "
            "(whisper_mlx rows will silently fall back to Whisper; that's "
            "fine on Intel Macs/other platforms, but check this if you "
            "expect Apple Silicon here)",
            flush=True,
        )
        results.append(("whisper_mlx (all sizes)", False, "mlx not available on this machine"))
        return

    from mlx_whisper.load_models import load_model as load_mlx_model

    for size in MODEL_SIZES:
        repo = whisper_mlx._repo_for(size)

        def _load(repo=repo):
            model = load_mlx_model(repo)
            del model

        attempt(f"whisper_mlx model: {repo}", _load)


warm_whisper_ct2()
warm_align_models()
warm_qwen()
warm_parakeet()
warm_whisper_mlx()

print("\n=== Summary ===")
for label, ok, err in results:
    tag = "OK  " if ok else "FAIL"
    suffix = f" -- {err}" if err else ""
    print(f"  [{tag}] {label}{suffix}")

failed = [r for r in results if not r[1]]
if failed:
    print(f"\n{len(failed)} of {len(results)} model(s) failed to warm up.")
    sys.exit(1)

print(f"\nAll {len(results)} model(s) warmed successfully.")
