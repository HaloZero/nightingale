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
        # Vendored mlx_whisper (0.4.3) is patched locally with the
        # BeamSearchDecoder from ml-explore/mlx-examples#1429 (unmerged as of
        # 2026-08-13) -- upstream still raises NotImplementedError for
        # beam_size otherwise. See the patched decoding.py in the vendor venv.
        result = mlx_whisper.transcribe(
            audio,
            path_or_hf_repo=repo,
            language=language,
            task="transcribe",
            word_timestamps=True,
            beam_size=beam_size,
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
