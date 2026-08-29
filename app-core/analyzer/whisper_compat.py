"""PyTorch / device compatibility helpers for Nightingale analyzer."""

import time

import torch

from gpu import hard_free_gpu, log_vram, reset_peak_stats, vram_snapshot, gpu_model

_original_torch_load = torch.load


def _patched_torch_load(*args, **kwargs):
    kwargs["weights_only"] = False
    return _original_torch_load(*args, **kwargs)


torch.load = _patched_torch_load


def _default_progress_sink(pct: int, msg: str):
    print(f"[nightingale:PROGRESS:{pct}] {msg}", flush=True)


_progress_sink = _default_progress_sink


def set_progress_sink(fn):
    global _progress_sink
    _progress_sink = fn or _default_progress_sink


def progress(pct: int, msg: str):
    _progress_sink(pct, msg)


def _default_starting_sink(stage: str):
    print(f"[nightingale:STARTING] stage={stage}", flush=True)


_starting_sink = _default_starting_sink


def set_starting_sink(fn):
    global _starting_sink
    _starting_sink = fn or _default_starting_sink


def starting(stage: str):
    """Announce that `stage` is about to begin -- pairs with the matching
    `timing(stage, ...)` call once it finishes, so logs show both ends of a
    stage instead of only the completion popping up with no lead-in."""
    _starting_sink(stage)


def _default_timing_sink(stage: str, ms: int):
    s = round(ms / 1000)
    m = ms / 60000
    print(f"[nightingale:TIMING] stage={stage} ms={ms} s={s} m={m:.1f}", flush=True)


_timing_sink = _default_timing_sink


def set_timing_sink(fn):
    global _timing_sink
    _timing_sink = fn or _default_timing_sink


def timing(stage: str, started_at: float):
    """Report the elapsed time (ms) for `stage` since `started_at`
    (a `time.perf_counter()` reading taken when the stage began)."""
    ms = int((time.perf_counter() - started_at) * 1000)
    _timing_sink(stage, ms)


_align_backend = "whisperx"


def set_align_backend(name):
    """Select the forced-alignment backend:

    - ``"whisperx"`` (default): pure-Python CTC Viterbi.
    - ``"ctc"``: torchaudio ``forced_align`` C++/CUDA kernel.
    - ``"qwen"``: Qwen3-ForcedAligner token-classification model. Handled
      directly by the transcribe/align callers (see ``qwen_align``); when a song
      falls outside its support (unsupported language, over-length audio, or any
      failure) the callers fall through to the wav2vec2 path, where ``_run_align``
      treats any non-``"ctc"`` backend as ``whisperx``.
    """
    global _align_backend
    _align_backend = name if name in ("whisperx", "ctc", "qwen") else "whisperx"


def get_align_backend() -> str:
    return _align_backend


# Which backend actually ran the most recent wav2vec2-path alignment --
# "ctc" or "whisperx" -- as opposed to `_align_backend`, the *configured*
# setting. They can differ: a `"ctc"`-configured song falls through to
# `whisperx.align` on a non-OOM `ctc_align` failure, and a `"qwen"`-configured
# song that's unsupported (language/length) falls through to this same
# wav2vec2 path, where `_run_align` treats any non-"ctc" backend as
# "whisperx". Set by `_run_align` right before each actual alignment call, so
# callers (`align.py`, `transcribe.py`) can record provenance accurately
# after `align_with_fallback` returns.
_effective_align_backend: str | None = None


def _set_effective_align_backend(name: str) -> None:
    global _effective_align_backend
    _effective_align_backend = name


def get_effective_align_backend() -> str | None:
    return _effective_align_backend


def detect_device() -> str:
    if torch.cuda.is_available():
        return "cuda"
    if torch.backends.mps.is_available():
        return "mps"
    return "cpu"


def align_device_for(device: str) -> str:
    return "cpu" if device == "mps" else device


def compute_type_for(device: str) -> str:
    if device != "cuda":
        return "float32"
    major, _ = torch.cuda.get_device_capability()
    if major >= 7:
        return "float16"
    return "int8"


def is_oom(err):
    lower = str(err).lower()
    return "out of memory" in lower or "outofmemoryerror" in lower


def free_gpu():
    """Backwards-compatible alias for :func:`gpu.hard_free_gpu`."""
    hard_free_gpu()


def _run_align(raw_segments, audio, language, device, model_name=None):
    import whisperx
    key_suffix = model_name or language
    with gpu_model(f"wav2vec2:{key_suffix}:{device}") as held:
        if model_name:
            print(
                f"[nightingale:LOG] Loading align model '{model_name}' for "
                f"language='{language}' on device='{device}'",
                flush=True,
            )
        else:
            print(
                f"[nightingale:LOG] Loading align model for language='{language}' on device='{device}'",
                flush=True,
            )
        try:
            align_model, metadata = whisperx.load_align_model(
                language_code=language, device=device, model_name=model_name,
            )
        except ValueError as e:
            # No wav2vec2 default for this language (e.g. a stale/forced
            # `language_override`, or a detected language language.py's own
            # alignable-candidate steering couldn't route around). Retrying
            # with "en" beats hard-failing the whole song -- word-level
            # timing degrades but the song still gets a transcript. Only for
            # the auto-selected-model path: an explicit `model_name` (cjk's
            # custom checkpoints) failing means something else is wrong and
            # should surface as-is.
            if model_name is None and "No default align-model" in str(e):
                print(
                    f"[nightingale:LOG] {e}; falling back to 'en' align model",
                    flush=True,
                )
                language = "en"
                align_model, metadata = whisperx.load_align_model(
                    language_code=language, device=device, model_name=model_name,
                )
            else:
                raise
        held.append(align_model)

        if get_align_backend() == "ctc":
            try:
                import ctc_align
                print(
                    f"[nightingale:LOG] Aligning with torchaudio forced_align (ctc) on {device}",
                    flush=True,
                )
                result = ctc_align.ctc_align(
                    raw_segments, align_model, metadata, audio, device,
                )
                _set_effective_align_backend("ctc")
                return result
            except Exception as e:
                if is_oom(e):
                    # Let align_with_fallback's OOM handling retry (it re-enters
                    # _run_align, keeping the fast ctc path, and finally drops to
                    # CPU) instead of silently switching to the slow whisperx path.
                    raise
                print(
                    f"[nightingale:LOG] ctc align backend failed ({e}); "
                    f"falling back to whisperx.align",
                    flush=True,
                )

        result = whisperx.align(raw_segments, align_model, metadata, audio, device)
        _set_effective_align_backend("whisperx")
        return result


def align_with_fallback(raw_segments, audio, language, device, pre_align_cleanup=None, model_name=None):
    """Run whisperx.align with OOM fallback: retry after cleanup, then CPU.

    ``model_name`` overrides the default WhisperX align model for the given
    language (e.g. swap in slplab's hiragana CTC for ``ja``).
    """
    try:
        return _run_align(raw_segments, audio, language, device, model_name=model_name)
    except Exception as e:
        if not is_oom(e):
            raise
        log_vram("oom:align_attempt1")

    if pre_align_cleanup:
        print(
            f"[nightingale:LOG] Alignment OOM, freeing other models and retrying on {device}",
            flush=True,
        )
        try:
            pre_align_cleanup()
        except Exception:
            pass
        try:
            return _run_align(raw_segments, audio, language, device, model_name=model_name)
        except Exception as e2:
            if not is_oom(e2):
                raise
            log_vram("oom:align_attempt2")

    print("[nightingale:LOG] Alignment OOM, falling back to CPU", flush=True)
    return _run_align(raw_segments, audio, language, "cpu", model_name=model_name)
