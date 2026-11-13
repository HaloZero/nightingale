# Fork changes

This fork tracks upstream Nightingale (`original/master`) and rebases its own
feature set on top of it, rather than diverging permanently. This file lists
what this fork adds beyond upstream, so the difference is easy to audit
instead of buried in history. Upstream's own features are documented in
`CHANGELOG.md` as usual; this file only covers fork-specific additions.

Dropped: this fork does not carry upstream's karaoke-video rendering
(reel-background and YouTube-background video generation, their DB tables,
and the related Chromecast casting option). Plain video playback (a song's
own bundled video, USDX bundled video) is unaffected.

## Alignment backends

Additional lyric-alignment backends alongside upstream's defaults, selectable
via the existing align-backend setting:

- `wav2vec2_bert_align.py` / `wav2vec2_bert_2pass.py` — a two-pass Wav2Vec2-BERT
  aligner; pass one aligns against the full lyric block, pass two refines
  timing around detected pauses.
- `mms_align.py` — Meta MMS-based forced alignment.
- `echora_align.py` — Echora-based alignment.
- `whisper_mlx.py` — an MLX-backed Whisper transcription backend for Apple
  Silicon, with its own model-cache lifecycle (released when the local
  analysis queue goes idle, via the existing `idle_cleanup` protocol message).

## Genre browsing

Songs carry a `genre` field (read from Jellyfin/Navidrome/Plex tags where the
source provides one, otherwise `"Unknown Genre"`). The library sidebar gets a
**Genres** section alongside Artists/Albums/Playlists, and genre is a first-class
library filter.

## Local-lyrics preference

Songs track `has_lrc_file` and `has_embedded_lyrics` independently of whether
an AI alignment has ever run. The library exposes an **External Lyrics**
filter (has/no sidecar `.lrc` or tag-embedded lyrics) so a library with mixed
sourcing is easy to audit, distinct from upstream's on-demand sidecar-LRC
lookup.

## Chromecast

Casting to Chromecast-compatible devices and TVs via `rust_cast`, with voice
"play X" matching against the local library (`app-core/src/search.rs`).

## Play history

The active profile records a `PlayRecord` (profile, song hash, title, artist,
timestamp) the moment a song's audio becomes ready to play, independent of
whether it's ever scored. Persisted alongside score history in the same
profile store. Title/artist are a point-in-time snapshot for making the file
readable by eye — song hash remains the real identity. No viewer UI yet; this
is backend groundwork for a future play-history view.

## Playback

- **Screen wake lock**: the display is kept from dimming/locking while a song
  is actively playing (`client/src/bridge/wake-lock.ts`), released on pause,
  song end, or when the tab is backgrounded.
- **Mobile web skips decorative backgrounds**: on mobile web (not the Tauri
  app), shaders, Pixabay clips, custom backgrounds, and a video song's own
  bundled video are never fetched or mounted, so a phone browser only has to
  load audio and lyrics. Theme/flavor controls are hidden accordingly.
- **Row action buttons**: each song row offers a direct play button, plus a
  realign button for AI-aligned songs (re-run alignment without re-queuing a
  full analysis) and an align button for unanalyzed songs.

## Server diagnostics

The self-hosted media server logs a warning when resolving a media path
(canonicalizing the input or an allowed root, or loading `AppConfig`) takes
longer than 250ms, to help diagnose slow storage or network mounts.
