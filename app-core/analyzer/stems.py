"""Stem separation: lead vocals + instrumental."""

import os
import subprocess

import numpy as np
import soundfile as sf
import torch

from gpu import gpu_model
from whisper_compat import progress

KARAOKE_MODEL = "mel_band_roformer_karaoke_aufr33_viperx_sdr_10.1956.ckpt"


def _ensure_wav(audio_path: str, work_dir: str) -> str:
    """Convert input audio to WAV so plain `soundfile` can decode it.

    We deliberately avoid `torchaudio.load`/`torchaudio.save` here: in
    torchaudio >= 2.9 those go through `torchcodec`, which `dlopen`s the
    FFmpeg shared libraries (`libavcodec.so.X`, ...). Our vendor dir only
    ships a static `ffmpeg` binary, and target machines aren't guaranteed to
    have the FFmpeg shared libs installed, so torchcodec fails to load.
    """
    if audio_path.lower().endswith(".wav"):
        return audio_path
    wav_path = os.path.join(work_dir, "input.wav")
    ffmpeg = os.environ.get("FFMPEG_PATH", "ffmpeg")
    subprocess.run(
        [ffmpeg, "-y", "-i", audio_path, "-ar", "44100", "-ac", "2", "-v", "error", wav_path],
        check=True,
    )
    return wav_path


def _load_wav_as_tensor(path: str) -> tuple[torch.Tensor, int]:
    """Load a WAV file as a (channels, frames) float32 tensor via soundfile."""
    data, sr = sf.read(path, dtype="float32", always_2d=True)
    tensor = torch.from_numpy(np.ascontiguousarray(data.T))
    return tensor, sr


def _save_wav_pcm16(wav: torch.Tensor, path: str, samplerate: int) -> None:
    """Save a (channels, frames) tensor as 16-bit PCM WAV with rescale clipping."""
    wav = wav.detach().cpu().to(torch.float32)
    peak = float(wav.abs().max().item())
    if peak > 1.0:
        wav = wav / (1.01 * peak)
    sf.write(path, wav.numpy().T, samplerate, subtype="PCM_16")


def separate_stems(audio_path: str, work_dir: str, device: str) -> tuple[str, str]:
    """Run Demucs to separate vocals and instrumental stems.

    Returns (vocals_path, instrumental_path).
    """
    from demucs.apply import apply_model
    from demucs.pretrained import get_model

    vocals_path = os.path.join(work_dir, "vocals.wav")
    instrumental_path = os.path.join(work_dir, "instrumental.wav")
    actual_device = torch.device(device if device != "mps" else "cpu")

    with gpu_model("demucs") as held:
        progress(5, "Loading Demucs model...")
        model = get_model("htdemucs")
        held.append(model)
        model.to(actual_device)

        progress(10, "Loading audio file...")
        load_path = _ensure_wav(audio_path, work_dir)
        wav, sr = _load_wav_as_tensor(load_path)
        wav = wav.to(actual_device)

        ref = wav.mean(0)
        wav_centered = wav - ref.mean()
        wav_scaled = wav_centered / ref.abs().max().clamp(min=1e-8)

        progress(15, "Separating vocals from instrumentals...")
        sources = apply_model(
            model, wav_scaled[None], device=actual_device, shifts=1, overlap=0.25,
        )[0]

        source_names = model.sources
        vocals_idx = source_names.index("vocals")

        vocals = sources[vocals_idx] * ref.abs().max() + ref.mean()
        instrumental = wav - vocals

        progress(45, "Saving separated stems...")
        vocals_cpu = vocals.detach().cpu()
        instrumental_cpu = instrumental.detach().cpu()

        del wav, sources, wav_centered, wav_scaled, ref, vocals, instrumental

        _save_wav_pcm16(vocals_cpu, vocals_path, sr)
        _save_wav_pcm16(instrumental_cpu, instrumental_path, sr)
        del vocals_cpu, instrumental_cpu

    progress(50, "Stem separation complete")
    return vocals_path, instrumental_path


def _resolve_separator_output(output_files: list[str], work_dir: str, stem_tag: str) -> str | None:
    """Find an output file matching the given stem tag (e.g. '(Vocals)')."""
    for f in output_files:
        full = f if os.path.isabs(f) else os.path.join(work_dir, f)
        if os.path.isfile(full) and stem_tag in f:
            return full
    return None


def separate_stems_uvr(audio_path: str, work_dir: str, models_dir: str) -> tuple[str, str]:
    """Separate lead vocals from everything else using the UVR karaoke model.

    The karaoke model isolates lead vocals, leaving backing vocals in the
    instrumental stem — ideal for karaoke playback and cleaner alignment.

    Returns (vocals_path, instrumental_path).
    """
    from audio_separator.separator import Separator

    with gpu_model("uvr-karaoke") as held:
        progress(5, "Loading karaoke separation model...")
        separator = Separator(
            model_file_dir=models_dir,
            output_dir=work_dir,
        )
        held.append(separator)
        separator.load_model(KARAOKE_MODEL)

        progress(15, "Preparing audio for stem separation...")
        load_path = _ensure_wav(audio_path, work_dir)
        progress(20, "Separating vocals from instrumentals...")
        output_files = separator.separate(load_path)

    print(f"[nightingale:LOG] Separator outputs: {output_files}", flush=True)

    vocals = _resolve_separator_output(output_files, work_dir, "(Vocals)")
    instrumental = _resolve_separator_output(output_files, work_dir, "(Instrumental)")

    if not vocals or not instrumental:
        raise RuntimeError(f"Expected Vocals and Instrumental stems, got: {output_files}")

    progress(50, "Stem separation complete")
    return vocals, instrumental
