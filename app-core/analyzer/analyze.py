#!/usr/bin/env python3
"""
Nightingale Song Analyzer
Separates vocals/instrumentals with Demucs and transcribes lyrics with WhisperX.

Usage:
    python analyze.py <audio_path> <output_dir> [--hash <file_hash>]

Outputs (in output_dir):
    {hash}_instrumental_{key}_{tempo}.mp3
    {hash}_vocals_{key}_{tempo}.mp3
    {hash}_transcript.json

Progress protocol (parsed by Rust app):
    [nightingale:PROGRESS:<percent>] <message>
"""

import argparse
import hashlib
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from whisper_compat import progress, detect_device, set_align_backend
from audio import set_vocal_threshold_pct
from pipeline import run_pipeline


def compute_hash(path: str) -> str:
    h = hashlib.blake2b(digest_size=16)
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    parser = argparse.ArgumentParser(description="Nightingale Song Analyzer")
    parser.add_argument("audio_path", help="Path to the audio file")
    parser.add_argument("output_dir", help="Directory to write output files")
    parser.add_argument("--hash", dest="file_hash", help="Pre-computed file hash")
    parser.add_argument("--model", default="large-v3", help="Whisper model name")
    parser.add_argument("--beam-size", type=int, default=5, help="Beam size for decoding")
    parser.add_argument("--batch-size", type=int, default=16, help="Batch size for transcription")
    parser.add_argument("--separator", default="karaoke", choices=["karaoke", "demucs"],
                        help="Stem separation method: karaoke (UVR, cleaner) or demucs (faster)")
    parser.add_argument("--engine", default="whisper", choices=["whisper", "parakeet", "whisper_mlx"],
                        help="Transcription engine: whisper (default), parakeet (NeMo on CUDA, ONNX "
                             "elsewhere), or whisper_mlx (Whisper on Apple Silicon's GPU via MLX; "
                             "falls back to whisper elsewhere)")
    parser.add_argument("--align-backend", dest="align_backend", default="whisperx",
                        choices=["whisperx", "ctc", "qwen"],
                        help="Forced-alignment backend: whisperx (default, Python Viterbi), "
                             "ctc (torchaudio forced_align C++/CUDA kernel), or qwen "
                             "(Qwen3-ForcedAligner)")
    parser.add_argument("--vocal-threshold", dest="vocal_threshold", type=float, default=None,
                        help="RMS threshold (fraction of peak, 0-1) for start/end vocal "
                             "detection. Lower keeps more edge audio; default 0.15")
    parser.add_argument("--lyrics", help="Path to pre-fetched lyrics JSON (align-only mode)")
    parser.add_argument("--language", default=None, help="Override automatic language detection")
    parser.add_argument("--skip-transcription", dest="skip_transcription", action="store_true",
                        help="Only run key detection and stem separation; skip transcription "
                             "entirely (e.g. to pre-warm the stems cache for a song)")
    args = parser.parse_args()

    audio_path = os.path.abspath(args.audio_path)
    output_dir = os.path.abspath(args.output_dir)

    if not os.path.isfile(audio_path):
        print(f"[nightingale] ERROR: File not found: {audio_path}", file=sys.stderr)
        sys.exit(1)

    file_hash = args.file_hash or compute_hash(audio_path)
    progress(0, "Starting analysis...")

    device = detect_device()
    set_align_backend(args.align_backend)
    set_vocal_threshold_pct(args.vocal_threshold)

    run_pipeline(
        audio_path, output_dir, file_hash, device,
        model_name=args.model,
        beam_size=args.beam_size,
        batch_size=args.batch_size,
        separator=args.separator,
        engine=args.engine,
        lyrics_path=args.lyrics,
        language_override=args.language,
        skip_transcription=args.skip_transcription,
    )

    progress(100, "DONE")


if __name__ == "__main__":
    main()
