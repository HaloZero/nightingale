"""Whisper transcription via Apple's MLX framework (Apple Silicon only).

WhisperX's CTranslate2 backend has no MPS kernel, so on Mac the regular
Whisper path silently runs on CPU (see ``whisper_compat.compute_type_for`` /
the ``device == "mps"`` handling in ``transcribe.py``). This module runs
Whisper natively on the Metal GPU instead via the ``mlx_whisper`` package.

Like Parakeet, ``mlx_whisper`` emits word-level timestamps directly, so the
caller skips the wav2vec2 forced-alignment step entirely for this path.

Named ``whisper_mlx`` (not ``mlx_whisper``) so this module doesn't shadow the
pip package of the same name when both sit on ``sys.path``.
"""

MODEL_REPOS = {
    "large-v3": "mlx-community/whisper-large-v3-mlx",
    "large-v3-turbo": "mlx-community/whisper-large-v3-turbo",
    "medium": "mlx-community/whisper-medium-mlx",
    "small": "mlx-community/whisper-small-mlx",
    "base": "mlx-community/whisper-base-mlx",
    "tiny": "mlx-community/whisper-tiny-mlx",
}

_INITIAL_PROMPT = (
    "Everything before and including GO is INSTRUCTIONS. DON'T INCLUDE IN TRANSCRIPT. "
    "Song Lyrics transcript. Split lines with punctuation. "
    "No annotations or descriptions. "
    "GO"
)


def is_available() -> bool:
    """Whether the mlx_whisper package can actually run here (Apple Silicon only)."""
    try:
        import mlx.core  # noqa: F401
        import mlx_whisper  # noqa: F401
    except Exception:
        return False
    return True


def _repo_for(model_name: str) -> str:
    return MODEL_REPOS.get(model_name, f"mlx-community/whisper-{model_name}-mlx")


def transcribe(
    audio,
    model_name: str,
    beam_size: int,
    language: str | None = None,
) -> tuple[list[dict], str]:
    """Transcribe a 16kHz mono float32 array with word-level timestamps.

    Returns ``(words, language)``. Timestamps in ``words`` are relative to the
    start of ``audio`` — same convention as ``transcribe._transcribe_whisper``,
    so the caller re-offsets them onto the original timeline.
    """
    import mlx_whisper
    from gpu import gpu_model

    repo = _repo_for(model_name)

    with gpu_model(f"whisper-mlx:{model_name}"):
        # mlx_whisper has no beam search decoder (raises NotImplementedError
        # for any beam_size), so we ask for best_of instead: same first pass
        # (greedy at temperature=0) with the same knob repurposed to control
        # how many candidates get sampled if that first pass looks bad enough
        # to trigger mlx_whisper's own temperature-fallback retry. Keeps this
        # on the MLX/Metal path instead of raising and falling back to the
        # much slower CPU whisper path.
        result = mlx_whisper.transcribe(
            audio,
            path_or_hf_repo=repo,
            language=language,
            task="transcribe",
            word_timestamps=True,
            best_of=beam_size,
            initial_prompt=_INITIAL_PROMPT,
        )

    detected_language = result.get("language") or language or "en"

    words: list[dict] = []
    for seg in result.get("segments", []):
        for w in seg.get("words", []):
            text = (w.get("word") or "").strip()
            start = w.get("start")
            end = w.get("end")
            if not text or start is None or end is None:
                continue
            words.append({"word": text, "start": float(start), "end": float(end)})

    print(
        f"[nightingale:LOG] Whisper MLX ({repo}) produced {len(words)} words, lang='{detected_language}'",
        flush=True,
    )

    return words, detected_language
