---
root: false
targets: ['*']
description: 'Karaoke playback, lyrics, audio, and media synchronization correctness'
---

# Playback and media correctness

- Maintain one explicit playback timeline. Audio stems, source video, timed lyrics, pitch capture, scoring, guide vocals, seek state, and visual effects must derive from compatible clocks with documented offsets and rate handling.
- Preserve synchronization through load, buffering, pause, resume, seek, key shift, tempo shift, intro/outro skips, device changes, and route transitions. Never repair visible drift with unrelated magic delays.
- Distinguish media time, wall-clock time, audio-context time, analyzer timestamps, and captured-input time. Convert at named boundaries with explicit units.
- Treat microphone permission, device loss, sample-rate changes, latency, and unavailable input as normal runtime states. Release streams, audio nodes, listeners, animation frames, and GPU/media resources when ownership ends.
- Scoring must not fabricate confidence or precision. Handle unvoiced frames, octave ambiguity, latency windows, transposition, and missing pitch data according to established domain behavior.
- Preserve authored or imported lyric boundaries and UltraStar note types. Any normalization must be reversible or limited to derived data; source lyrics and media remain unchanged.
- Audio/video URL and byte-range handling must validate bounds and content. Do not load arbitrary local paths or remote origins outside approved source adapters.
- Analyzer and vendor subprocess protocols require bounded messages, authenticated local IPC where implemented, actionable failures, deterministic cleanup, and no secrets or private media paths in user-facing errors.
- Playback controls must remain operable with keyboard, touch, and supported gamepad flows. Status must not rely solely on sound, color, animation, hover, or pointer precision.
- Manually validate playback changes with representative audio/video, timed/plain lyrics, pause/seek/resume, tempo/key changes, microphone unavailable/available states, and both Tauri and self-hosted transports when affected.
