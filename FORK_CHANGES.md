# Fork changes

This fork tracks upstream Nightingale (`original/master`) and rebases its own
feature set on top of it, rather than diverging permanently. This file lists
what this fork adds beyond upstream, so the difference is easy to audit
instead of buried in history. Upstream's own features are documented in
`CHANGELOG.md` as usual; this file only covers fork-specific additions.

Dropped: this fork does not carry upstream's karaoke-video rendering
(reel-background and YouTube-background video generation and their DB
tables), Chromecast casting, play-history tracking, the self-hosted media
server's slow-path diagnostic logging, an MLX-backed Whisper transcription
backend for Apple Silicon, or the MMS/Echora forced-alignment backends.
Plain video playback (a song's own bundled video, USDX bundled video) is
unaffected.

## Alignment backends

An additional lyric-alignment backend alongside upstream's defaults,
selectable via the existing align-backend setting:

- `wav2vec2_bert_align.py` / `wav2vec2_bert_2pass.py` — a two-pass Wav2Vec2-BERT
  aligner; pass one aligns against the full lyric block, pass two refines
  timing around detected pauses.

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

## Database migration from the pre-rebuild fork

A library DB created by this fork before it was rebuilt on fresh upstream may
still carry tables from removed features (karaoke-video rendering,
parallel-analysis/peer offload). `SCHEMA_VERSION` is bumped to 5 with a
`version < 5` migration that drops those orphaned tables
(`karaoke_video_status`, `karaoke_video_runs`, `video_processing_queue`,
`youtube_video_lookups`, `youtube_video_sync`, `parallel_analysis_mismatches`,
`parallel_analysis_timings`) on first open. Verified against a real backup
copy of a pre-rebuild database: migrates cleanly, all library data and row
counts preserved, only the dead tables removed.
