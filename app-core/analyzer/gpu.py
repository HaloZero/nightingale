"""Centralized GPU lifecycle helpers for the analyzer.

Every model load in the analyzer goes through ``gpu_model`` so its weights are
guaranteed to be evicted from VRAM regardless of how the call exits, and so the
VRAM cost of each phase is logged for visibility.
"""

from __future__ import annotations

import gc
from contextlib import contextmanager

import torch


def _cuda_available() -> bool:
    try:
        return torch.cuda.is_available()
    except Exception:
        return False


def vram_snapshot() -> dict:
    """Return current/peak VRAM usage in MiB, or empty dict on non-CUDA."""
    if not _cuda_available():
        return {}
    try:
        torch.cuda.synchronize()
    except Exception:
        pass
    mib = 1024 * 1024
    return {
        "allocated": torch.cuda.memory_allocated() // mib,
        "reserved": torch.cuda.memory_reserved() // mib,
        "peak_alloc": torch.cuda.max_memory_allocated() // mib,
        "peak_reserved": torch.cuda.max_memory_reserved() // mib,
    }


def log_vram(tag: str) -> None:
    snap = vram_snapshot()
    if not snap:
        return
    print(
        f"[nightingale:LOG] [vram:{tag}] "
        f"alloc={snap['allocated']}MiB reserved={snap['reserved']}MiB "
        f"peak={snap['peak_alloc']}MiB peak_reserved={snap['peak_reserved']}MiB",
        flush=True,
    )


def reset_peak_stats() -> None:
    if not _cuda_available():
        return
    try:
        torch.cuda.reset_peak_memory_stats()
    except Exception:
        pass


def hard_free_gpu(tag: str = "") -> None:
    """Aggressively free GPU memory: gc x2, sync, empty_cache, ipc_collect."""
    for _ in range(2):
        gc.collect()
    if _cuda_available():
        try:
            torch.cuda.synchronize()
        except Exception:
            pass
        try:
            torch.cuda.empty_cache()
        except Exception:
            pass
        try:
            torch.cuda.ipc_collect()
        except Exception:
            pass
    if tag:
        log_vram(f"after_free:{tag}")


def move_to_cpu(model) -> None:
    """Best-effort eviction of model weights from VRAM to host RAM."""
    if model is None:
        return
    for attr in ("model", "model_run_obj", "separator"):
        inner = getattr(model, attr, None)
        if inner is not None and inner is not model:
            try:
                move_to_cpu(inner)
            except Exception:
                pass
    try:
        to = getattr(model, "to", None)
        if callable(to):
            to("cpu")
            return
    except Exception:
        pass
    try:
        cpu = getattr(model, "cpu", None)
        if callable(cpu):
            cpu()
    except Exception:
        pass


def release(model, name: str = "") -> None:
    """Move ``model`` to CPU, drop the reference, and free GPU memory."""
    try:
        move_to_cpu(model)
    except Exception:
        pass
    del model
    hard_free_gpu(name)


@contextmanager
def gpu_model(name: str):
    """Context manager that guarantees GPU cleanup for the model(s) it holds.

    Usage:
        with gpu_model("demucs") as held:
            model = load_demucs()
            held.append(model)
            ...

    On exit (success or exception), every model in ``held`` is moved to CPU,
    references are dropped, and GPU memory is forcibly reclaimed.
    """
    log_vram(f"before_load:{name}")
    holder: list = []
    try:
        yield holder
    finally:
        for m in holder:
            try:
                move_to_cpu(m)
            except Exception:
                pass
        holder.clear()
        hard_free_gpu(name)


def end_of_song_cleanup() -> None:
    """Best-effort full reset between queued songs.

    Calls into per-backend free hooks (kept for backwards compat) and then runs
    the bulletproof free pass. Safe to call repeatedly.
    """
    try:
        import parakeet
        parakeet.free_models()
    except Exception:
        pass
    hard_free_gpu("end_of_song")


def release_idle_caches() -> None:
    """Release caches that are worth keeping warm between back-to-back songs
    but not while the server is sitting idle with an empty queue.

    Unlike `end_of_song_cleanup` (run after every song, CUDA/general only),
    this also drops `whisper_mlx`'s cached model object and clears MLX's own
    buffer cache. MLX arrays live in Apple Silicon's unified memory, so that
    cache is real host RAM the process would otherwise hold onto indefinitely
    -- there's no CUDA-style VRAM boundary making it show up separately.
    Reloading a ~3GB MLX Whisper model on the next song is an acceptable cost
    for the queue-drained case; it's the every-song case this is deliberately
    *not* called from.
    """
    try:
        import whisper_mlx
        whisper_mlx.free_model()
    except Exception:
        pass
    try:
        import mlx.core as mx
        # `synchronize()` first is required, not cosmetic: MLX's Metal work
        # is async, so the buffer behind the array `free_model()` just
        # dropped may still be in flight when `clear_cache()` runs. Without
        # this, `clear_cache()` sees an empty cache (the buffer hasn't landed
        # in it yet) and silently frees nothing -- confirmed by measuring
        # `mx.get_cache_memory()` before/after on a real array.
        mx.synchronize()
        mx.clear_cache()
    except Exception:
        pass
    end_of_song_cleanup()
