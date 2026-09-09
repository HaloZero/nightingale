---
root: false
targets: ['*']
description: 'Security boundaries and karaoke-domain correctness'
---

# Security and karaoke-domain correctness

## Trust boundaries

- Self-hosted does not mean trusted. Default listeners to loopback unless configuration explicitly opts into wider exposure. Document LAN/public binding consequences.
- Authenticate privileged or mutating endpoints and authorize each operation against its resource. Reverse-proxy authentication must be an explicit deployment contract, not an assumption.
- Validate payload size, shape, identifiers, paths, URLs, byte ranges, media metadata, and analyzer messages before use. Canonicalize and constrain filesystem paths to approved roots; prevent traversal and symlink escape.
- Constrain origins, CORS, WebSocket origins, and content types. Use secure cookie/token handling if browser authentication is added. Never place secrets in frontend bundles, URLs, logs, or persisted plaintext by default.
- Tauri capabilities use least privilege. New commands, plugins, process access, filesystem scopes, remote origins, updater behavior, and CSP changes require explicit security review. Never construct shell commands from user input.
- Keep CSP enabled and narrow for production. Scope development exceptions to development.
- Dependencies, downloaded ML models, vendor binaries, Python environments, update artifacts, and build scripts are supply-chain boundaries. Verify integrity where supported, keep lockfiles, deny unknown Cargo sources, and audit advisories/licenses.
- Treat Plex, Jellyfin, Navidrome, LRCLIB, Pixabay, and analyzer credentials/tokens as secrets. Redact them from logs, URLs shown to users, diagnostics, and errors.

## Karaoke domain

- Preserve source media and user-entered titles, artist names, lyrics, spelling, scripts, and locale. Do not destructively normalize identity-bearing text or overwrite source files without explicit action.
- Keep timestamps, durations, sample rates, frame counts, frequencies, MIDI notes, pitch classes, cents, key shifts, tempo ratios, scores, and confidence values semantically distinct. Make units, ranges, clocks, rounding, and conversion boundaries explicit.
- Preserve lyric timing and format semantics across plain lyrics, LRC, Enhanced LRC, generated alignments, CJK tokenization/romanization, and UltraStar data. Do not silently discard precision or reinterpret timing.
- Treat song, source, profile, playlist, analysis, and score identifiers as opaque. Do not use list positions, paths, display names, or mutable labels as identity unless existing persisted contract explicitly does.
- Keep objective media facts separate from generated analysis, inferred metadata, subjective quality, and player scoring. Missing/uncertain analysis must not become fabricated certainty.
- Playback, lyrics, guide vocals, pitch scoring, tempo/key shifts, microphone monitoring, video, and UI highlighting must use deliberate synchronization semantics. Pause, seek, resume, latency compensation, and rate changes must not make clocks drift silently.
- Serialization and persistence changes require compatibility and migration review. Never silently reinterpret existing caches, databases, profiles, settings, playlists, scores, or analysis results.
- Do not invent scoring, privacy, retention, source ownership, metadata precedence, cache invalidation, or conflict-resolution rules. Ask for missing invariants and keep approved behavior in shared domain code.
