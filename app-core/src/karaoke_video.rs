//! Pre-renders a karaoke video (background + title/artist + word-timed
//! lyrics) for a song, cached per `file_hash` like every other per-song
//! artifact in this crate (stems, transcripts, playable-video transcodes --
//! see `cache::CacheDir`, `playback::ensure_playable_source_video`).
//!
//! Text is rasterized in pure Rust (`ab_glyph`) rather than relying on
//! ffmpeg's `subtitles=`/`ass=`/`drawtext` filters, which depend on
//! optional libraries (`libass`, `libfreetype`, `libfontconfig`) that are
//! not guaranteed to be present in a given ffmpeg build regardless of
//! version -- confirmed missing in a real, current `ffmpeg` build during
//! development of this feature. We only ever hand ffmpeg a background
//! video/color to decode and our text frames to composite on top of it
//! (`overlay`) -- both are basic, near-universal ffmpeg operations, unlike
//! the missing subtitle-rendering libs.
//!
//! Frames are RGBA: text pixels are opaque, everything else fully
//! transparent, so ffmpeg's `overlay` filter composites them onto whatever
//! background (a looped cached Pixabay video, or a synthetic solid color
//! when none is cached) without us needing to know its pixels ourselves.

use std::io::Write;
use std::process::Stdio;

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont, point};
use rand::prelude::IndexedRandom;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::cache::CacheDir;
use crate::error::NightingaleError;
use crate::library_db;
use crate::library_model::LibraryMenuFilters;
use crate::vendor::{ensure_font_downloaded, ffmpeg_path, silent_command};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
// 12fps was cheap to rasterize/pipe but visibly choppy once real (panning
// drone-shot) video backgrounds were in the mix, not just static color;
// 24fps still showed some judder, 30fps is standard video/TV smoothness.
const FRAME_RATE: f64 = 30.0;

/// Guide vocal level mixed under the instrumental, matching the live
/// in-app karaoke experience's guide-vocal concept (`use-audio-player.ts`'s
/// `guideVolume`) instead of either silence or a full-volume original mix.
const GUIDE_VOCAL_VOLUME: f64 = 0.2;

// Sizes/margins scaled 1.5x from the original 720p constants (1080/720).
const TITLE_SIZE: f32 = 45.0;
const LYRICS_SIZE: f32 = 66.0;
const TITLE_BASELINE_Y: f32 = 135.0;
const LYRICS_BASELINE_Y: f32 = (HEIGHT as f32) - 180.0;

/// Segment lingers this long past its nominal end before the line clears,
/// so it doesn't vanish mid-breath between segments.
const SEGMENT_LINGER_SECS: f64 = 0.4;

const BG_COLOR_HEX: &str = "0x0C0E18";
const TITLE_COLOR: [u8; 3] = [255, 255, 255];
/// Not-yet-sung words. Bright white rather than the old dim gray -- the
/// dim version read as barely legible against busy/bright backgrounds.
const UNSUNG_COLOR: [u8; 3] = [255, 255, 255];
/// Sung-word accent colors -- one is picked per render (see
/// `pick_accent_color`) so a batch of rendered videos doesn't all use the
/// exact same highlight color. All chosen for contrast against both the
/// white unsung text and a wide range of real video backgrounds.
const ACCENT_PALETTE: [[u8; 3]; 6] = [
    [255, 205, 80],  // gold
    [255, 120, 120], // coral
    [110, 220, 255], // cyan
    [140, 255, 150], // green
    [255, 140, 220], // pink
    [190, 150, 255], // violet
];

fn pick_accent_color() -> [u8; 3] {
    let mut rng = rand::rng();
    *ACCENT_PALETTE.choose(&mut rng).unwrap_or(&ACCENT_PALETTE[0])
}

#[derive(Deserialize)]
struct TranscriptWord {
    word: String,
    start: f64,
    end: f64,
}

#[derive(Deserialize)]
struct TranscriptSegment {
    start: f64,
    end: f64,
    #[serde(default)]
    words: Vec<TranscriptWord>,
}

#[derive(Deserialize)]
struct TranscriptDoc {
    segments: Vec<TranscriptSegment>,
}

/// Mirrors `playback::StemsReady`'s shape -- same "fire a background thread,
/// emit this on completion" pattern used by the `"render_karaoke_video"`
/// command.
#[derive(Debug, Clone, Serialize)]
pub struct KaraokeVideoReady {
    pub file_hash: String,
    pub error: Option<String>,
}

pub fn ensure_karaoke_video_ready_payload(file_hash: String, force: bool) -> KaraokeVideoReady {
    match ensure_karaoke_video(&file_hash, force) {
        Ok(_) => KaraokeVideoReady {
            file_hash,
            error: None,
        },
        Err(e) => KaraokeVideoReady {
            file_hash,
            error: Some(e.to_string()),
        },
    }
}

/// Result of `ensure_youtube_karaoke_video` -- same "fire a background
/// thread, emit this on completion" shape as `KaraokeVideoReady`, with one
/// extra field: whether TheAudioDB actually had a music video for this song
/// at all, distinct from a download/sync/render failure after one was
/// found -- `music_video_found: false` always means `error: Some(...)` too
/// (nothing to render), but the reverse isn't true.
#[derive(Debug, Clone, Serialize)]
pub struct YoutubeKaraokeVideoReady {
    pub file_hash: String,
    pub music_video_found: bool,
    pub error: Option<String>,
}

/// The explicit "fetch a YouTube video for this song and build a
/// YouTube-background karaoke video from it" action:
/// `audiodb::find_music_video_for_hash` (cached -- see its doc comment) to
/// find one, `youtube_video::ensure_youtube_video_downloaded` to fetch it,
/// then a forced `ensure_youtube_background_karaoke_video` render into its
/// own cache slot -- the existing reel-background video (if any) for this
/// song is untouched, both can exist side by side (see
/// `best_karaoke_video_path`). If no video exists on TheAudioDB at all,
/// `music_video_found` comes back `false` and no render is attempted. If
/// one exists but can't be downloaded or confidently synced to the song's
/// audio, `music_video_found` is `true` but `error` explains why there's
/// still no YouTube-background render.
///
/// No-ops the entire lookup/download/render chain if a YouTube-background
/// render already exists and is fresh relative to the transcript -- same
/// `is_fresh` freshness check `ensure_karaoke_video`/`best_karaoke_video_path`
/// use, so re-running this action (e.g. as part of a bulk fetch over a large
/// filtered set) doesn't re-hit TheAudioDB or re-render songs it already has
/// a good answer for. A stale transcript (re-analysis changed the lyrics/
/// timing) still forces a fresh lookup-through-render, same as the reel
/// path.
pub fn ensure_youtube_karaoke_video(file_hash: &str) -> YoutubeKaraokeVideoReady {
    let cache = CacheDir::new();
    if is_fresh(
        &cache.youtube_karaoke_video_path(file_hash),
        &cache.transcript_path(file_hash),
    ) {
        info!(
            "[youtube_karaoke_video] {file_hash}: already have a fresh YouTube-background render, skipping"
        );
        return YoutubeKaraokeVideoReady {
            file_hash: file_hash.to_string(),
            music_video_found: true,
            error: None,
        };
    }

    let pipeline_started = std::time::Instant::now();
    info!("[youtube_karaoke_video] {file_hash}: starting (lookup -> download -> render)");

    let Some(video) = crate::audiodb::find_music_video_for_hash(file_hash) else {
        info!(
            "[youtube_karaoke_video] {file_hash}: no music video found, stopping (no render attempted)"
        );
        return YoutubeKaraokeVideoReady {
            file_hash: file_hash.to_string(),
            music_video_found: false,
            error: Some("no official music video found for this song".to_string()),
        };
    };
    info!(
        "[youtube_karaoke_video] {file_hash}: found music video {} -- downloading",
        video.youtube_url
    );

    let download_started = std::time::Instant::now();
    if let Err(e) =
        crate::youtube_video::ensure_youtube_video_downloaded(file_hash, &video.youtube_url)
    {
        warn!(
            "[youtube_karaoke_video] {file_hash}: download failed after {:.1}s: {e}",
            download_started.elapsed().as_secs_f64()
        );
        return YoutubeKaraokeVideoReady {
            file_hash: file_hash.to_string(),
            music_video_found: true,
            error: Some(format!("failed to download music video: {e}")),
        };
    }
    info!(
        "[youtube_karaoke_video] {file_hash}: download step done in {:.1}s -- rendering",
        download_started.elapsed().as_secs_f64()
    );

    let render_started = std::time::Instant::now();
    match ensure_youtube_background_karaoke_video(file_hash, true) {
        Ok(_) => {
            info!(
                "[youtube_karaoke_video] {file_hash}: render done in {:.1}s, pipeline total {:.1}s",
                render_started.elapsed().as_secs_f64(),
                pipeline_started.elapsed().as_secs_f64()
            );
            YoutubeKaraokeVideoReady {
                file_hash: file_hash.to_string(),
                music_video_found: true,
                error: None,
            }
        }
        Err(e) => {
            warn!(
                "[youtube_karaoke_video] {file_hash}: render failed after {:.1}s: {e}",
                render_started.elapsed().as_secs_f64()
            );
            YoutubeKaraokeVideoReady {
                file_hash: file_hash.to_string(),
                music_video_found: true,
                error: Some(format!("failed to render karaoke video: {e}")),
            }
        }
    }
}

/// Shared bulk-dispatch shape for the three karaoke video actions below:
/// resolve `filters` to eligible hashes (`iter_file_hashes_filtered_
/// karaoke_renderable`), hand the count back immediately, and run `action`
/// over each hash sequentially on a background thread. Backgrounded (like
/// `analyzer::refresh_metadata_all`, unlike `reanalyze_all_full`) because
/// each call here does real ffmpeg work directly and blocking -- there's no
/// existing worker queue for karaoke video the way there is for analysis.
/// Sequential, not one-thread-per-song: N parallel ffmpeg encodes (or, for
/// the YouTube variant, N concurrent yt-dlp downloads racing past
/// `youtube_video`'s own throttle) would defeat the point of the
/// throttling/single-flight care already taken per-song.
///
/// Logs a `(i/count)` position line before each song and a done/failed line
/// with that song's elapsed time after, plus a total-elapsed line when the
/// whole batch finishes -- `action` reports outcome via `Result` instead of
/// being fire-and-forget so a per-song failure actually surfaces here rather
/// than vanishing into a discarded `Result`.
fn bulk_karaoke_video(
    filters: &LibraryMenuFilters,
    action: fn(&str) -> Result<(), String>,
) -> usize {
    let hashes =
        library_db::iter_file_hashes_filtered_karaoke_renderable(filters).unwrap_or_default();
    let count = hashes.len();
    info!("[karaoke_video] bulk action starting for {count} eligible song(s)");
    std::thread::spawn(move || {
        let batch_started = std::time::Instant::now();
        for (i, hash) in hashes.iter().enumerate() {
            let position = i + 1;
            let label = song_label(hash);
            let song_started = std::time::Instant::now();
            info!("[karaoke_video] ({position}/{count}) starting {label}");
            match action(hash) {
                Ok(()) => info!(
                    "[karaoke_video] ({position}/{count}) {label} done in {:.1}s",
                    song_started.elapsed().as_secs_f64()
                ),
                Err(e) => warn!(
                    "[karaoke_video] ({position}/{count}) {label} failed after {:.1}s: {e}",
                    song_started.elapsed().as_secs_f64()
                ),
            }
        }
        info!(
            "[karaoke_video] bulk action finished ({count} song(s)) in {:.1}s",
            batch_started.elapsed().as_secs_f64()
        );
    });
    count
}

/// `"{title} — {artist} ({file_hash})"` for bulk-progress logging, falling
/// back to the bare hash if the song can't be loaded (deleted mid-batch,
/// DB error) -- never worth failing or skipping the log line over.
fn song_label(file_hash: &str) -> String {
    match library_db::load_song_by_hash(file_hash) {
        Ok(Some(song)) => format!("{} — {} ({file_hash})", song.title, song.artist),
        _ => file_hash.to_string(),
    }
}

/// Bulk "Render karaoke video" -- no-ops per-song for anything already
/// fresh relative to its transcript, same as the per-song action.
pub fn render_karaoke_video_all(filters: &LibraryMenuFilters) -> usize {
    bulk_karaoke_video(filters, |hash| {
        ensure_karaoke_video(hash, false)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Bulk "Force re-render karaoke video" -- regenerates every eligible song
/// unconditionally, same as the per-song action.
pub fn force_rerender_karaoke_video_all(filters: &LibraryMenuFilters) -> usize {
    bulk_karaoke_video(filters, |hash| {
        ensure_karaoke_video(hash, true)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Bulk "Fetch YouTube karaoke video" -- runs `ensure_youtube_karaoke_video`
/// (lookup -> download -> render) per eligible song. TheAudioDB lookups are
/// already throttled at the source (`audiodb::MIN_LOOKUP_INTERVAL`) and
/// downloads at the source (`youtube_video::MIN_DOWNLOAD_INTERVAL`), and
/// this only ever calls one song at a time on one thread, so a large
/// filtered set with a cold lookup/download cache is slow here by design,
/// not accidentally -- that's the tradeoff for staying under TheAudioDB's
/// free tier and being polite to YouTube.
///
/// `music_video_found: false` is reported as `Ok` to the bulk dispatcher
/// (not a failure -- `ensure_youtube_karaoke_video` already logs that case
/// at info level); only an actual download/sync/render error counts as a
/// bulk-level failure.
pub fn fetch_youtube_karaoke_video_all(filters: &LibraryMenuFilters) -> usize {
    bulk_karaoke_video(filters, |hash| {
        let result = ensure_youtube_karaoke_video(hash);
        if result.music_video_found {
            result.error.map_or(Ok(()), Err)
        } else {
            Ok(())
        }
    })
}

/// Renders (or returns the cached) reel-background karaoke video for
/// `file_hash`. Blocking -- callers on an async runtime must run it via
/// `tokio::task::spawn_blocking`, same rule as
/// `chromecast::cast_song_to_configured_device`. `force` skips the
/// freshness check and re-renders unconditionally -- useful since the
/// background is randomly picked from the cached Pixabay clips each time,
/// so re-running gives a different background. Always uses the reel/
/// raw-clip pool, even if a downloaded YouTube video exists for this song
/// -- that's a separate, independently-cached artifact, see
/// `ensure_youtube_background_karaoke_video`/`best_karaoke_video_path`.
pub fn ensure_karaoke_video(file_hash: &str, force: bool) -> Result<std::path::PathBuf, NightingaleError> {
    let video_path = CacheDir::new().karaoke_video_path(file_hash);
    let result = render_karaoke_video_to(file_hash, force, &video_path, |duration_secs| {
        let background = select_background_video(duration_secs).map(|path| BackgroundSource {
            path,
            start_offset_secs: 0.0,
        });
        if background.is_none() {
            warn!(
                "[karaoke_video] no cached background videos or reels for any of \
                 {BACKGROUND_FLAVORS:?} -- falling back to solid color background (run the \
                 download_all_pixabay_videos action to populate a background cache)"
            );
        }
        background
    });
    if result.is_ok() {
        record_karaoke_video_status(file_hash, KaraokeVideoKind::Reel);
    }
    result
}

/// Renders (or returns the cached) YouTube-video-background karaoke video
/// for `file_hash`, into its own cache slot (`youtube_karaoke_video_path`)
/// separate from the reel-background one -- both can exist at once. Errors
/// out rather than falling back to the reel pool if no video is downloaded
/// yet, or it can't be confidently matched to the song's own audio
/// (`video_sync::detect_sync_offset_for_hash`): this function is
/// specifically "render the YouTube-flavored artifact," not "render
/// something, preferably from YouTube" -- callers that want the latter
/// should use `best_karaoke_video_path` instead.
pub fn ensure_youtube_background_karaoke_video(
    file_hash: &str,
    force: bool,
) -> Result<std::path::PathBuf, NightingaleError> {
    let cache = CacheDir::new();
    let youtube_source = cache.youtube_video_path(file_hash);
    if !youtube_source.is_file() {
        return Err(NightingaleError::Other(format!(
            "no downloaded YouTube video for {file_hash} -- download one first"
        )));
    }

    let sync = crate::video_sync::detect_sync_offset_for_hash(file_hash, &youtube_source)
        .map_err(NightingaleError::Other)?
        .filter(|s| s.video_offset_secs >= 0.0)
        .ok_or_else(|| {
            NightingaleError::Other(format!(
                "downloaded YouTube video for {file_hash} couldn't be confidently synced to the \
                 song's audio (or would need padding, not trimming, to line up)"
            ))
        })?;
    info!(
        "[karaoke_video] using downloaded YouTube video as background for {file_hash} \
         (offset={:.2}s, confidence={:.3})",
        sync.video_offset_secs, sync.confidence
    );
    let background = BackgroundSource {
        path: youtube_source.to_string_lossy().into_owned(),
        start_offset_secs: sync.video_offset_secs,
    };

    let video_path = cache.youtube_karaoke_video_path(file_hash);
    let result = render_karaoke_video_to(file_hash, force, &video_path, move |_duration_secs| {
        Some(background)
    });
    if result.is_ok() {
        record_karaoke_video_status(file_hash, KaraokeVideoKind::Youtube);
    }
    result
}

/// Which of the two independently-cached karaoke video flavors just
/// succeeded -- see `library_db::karaoke_video_status`'s doc comment for
/// why they're tracked in their own side table rather than as `songs`
/// columns.
#[derive(Clone, Copy)]
enum KaraokeVideoKind {
    Reel,
    Youtube,
}

/// Upserts the `karaoke_video_status` side table for whichever flavor just
/// rendered successfully, then mirrors the table's current combined state
/// onto `Song.has_karaoke_video`/`has_youtube_karaoke_video` so the song
/// list can show it per row without an extra query. Only ever called after
/// a successful render -- a failed render never invalidates a
/// pre-existing successful one (see `render_karaoke_video_to`'s
/// tmp-file-then-rename), so there's no corresponding "mark false" path.
///
/// Skips the write entirely if this flavor is already recorded: both
/// `ensure_karaoke_video`/`ensure_youtube_background_karaoke_video` call
/// this on every `Ok` return, including the freshness-check fast path that
/// does no actual rendering (e.g. every cast via `best_karaoke_video_path`)
/// -- without this check that path would do a full side-table write plus
/// song-payload read/write on every call, defeating the point of it being
/// a fast path. Still self-heals for libraries that had karaoke videos on
/// disk from before this table existed: the first `Ok` after upgrade finds
/// `already_recorded: false` and backfills it once.
fn record_karaoke_video_status(file_hash: &str, kind: KaraokeVideoKind) {
    let (has_karaoke_video, has_youtube_karaoke_video) =
        library_db::get_karaoke_video_status(file_hash).unwrap_or_default();
    let already_recorded = match kind {
        KaraokeVideoKind::Reel => has_karaoke_video,
        KaraokeVideoKind::Youtube => has_youtube_karaoke_video,
    };
    if already_recorded {
        return;
    }

    let write_result = match kind {
        KaraokeVideoKind::Reel => library_db::set_has_karaoke_video(file_hash, true),
        KaraokeVideoKind::Youtube => library_db::set_has_youtube_karaoke_video(file_hash, true),
    };
    if let Err(e) = write_result {
        warn!("[karaoke_video] {file_hash}: failed to record karaoke video status: {e}");
        return;
    }

    let Ok(Some(mut song)) = library_db::load_song_by_hash(file_hash) else {
        return;
    };
    song.has_karaoke_video = matches!(kind, KaraokeVideoKind::Reel) || has_karaoke_video;
    song.has_youtube_karaoke_video =
        matches!(kind, KaraokeVideoKind::Youtube) || has_youtube_karaoke_video;
    if let Err(e) = library_db::update_song_fields(file_hash, &song) {
        warn!("[karaoke_video] {file_hash}: failed to mirror karaoke video status onto song: {e}");
    }
}

/// Report from `backfill_karaoke_video_status_from_cache`.
#[derive(Debug, Default)]
pub struct KaraokeVideoBackfillReport {
    pub reel_files_found: usize,
    pub youtube_files_found: usize,
    pub reel_backfilled: usize,
    pub youtube_backfilled: usize,
    /// Cached video file whose hash has no matching row in `songs` (song
    /// deleted/rescanned away since the video was rendered) -- left alone,
    /// just counted.
    pub orphaned: usize,
}

/// One-off maintenance action for libraries that had karaoke videos
/// rendered before the `karaoke_video_status` table existed: lists
/// `karaoke_videos/` directly (cheaper than iterating every song and
/// stat-ing two paths each -- see `CacheDir::karaoke_video_path`/
/// `youtube_karaoke_video_path` for the `{hash}.mp4` / `{hash}_youtube.mp4`
/// naming this relies on) and backfills the status table for every file
/// found, same as `record_karaoke_video_status` does lazily on the next
/// successful render/cast. Idempotent -- already-recorded songs are
/// skipped -- so it's safe to run more than once (e.g. after every
/// deploy) rather than tracking whether it's "already been run".
pub fn backfill_karaoke_video_status_from_cache() -> KaraokeVideoBackfillReport {
    let started = std::time::Instant::now();
    let dir = CacheDir::new().path.join("karaoke_videos");
    info!("[karaoke_video] backfill: scanning {}", dir.display());

    let mut report = KaraokeVideoBackfillReport::default();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(
                "[karaoke_video] backfill: failed to read {}: {e}",
                dir.display()
            );
            return report;
        }
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".mp4") else {
            continue;
        };
        let (file_hash, kind, label) = if let Some(hash) = stem.strip_suffix("_youtube") {
            (hash, KaraokeVideoKind::Youtube, "YouTube")
        } else {
            (stem, KaraokeVideoKind::Reel, "reel")
        };

        match kind {
            KaraokeVideoKind::Reel => report.reel_files_found += 1,
            KaraokeVideoKind::Youtube => report.youtube_files_found += 1,
        }

        let (has_karaoke_video, has_youtube_karaoke_video) =
            library_db::get_karaoke_video_status(file_hash).unwrap_or_default();
        let already_recorded = match kind {
            KaraokeVideoKind::Reel => has_karaoke_video,
            KaraokeVideoKind::Youtube => has_youtube_karaoke_video,
        };
        if already_recorded {
            continue;
        }

        match library_db::load_song_by_hash(file_hash) {
            Ok(Some(_)) => {
                record_karaoke_video_status(file_hash, kind);
                match kind {
                    KaraokeVideoKind::Reel => report.reel_backfilled += 1,
                    KaraokeVideoKind::Youtube => report.youtube_backfilled += 1,
                }
            }
            Ok(None) => {
                report.orphaned += 1;
                info!(
                    "[karaoke_video] backfill: {file_hash} has a cached {label} video but no \
                     matching song, skipping"
                );
            }
            Err(e) => warn!("[karaoke_video] backfill: failed to load song {file_hash}: {e}"),
        }
    }

    info!(
        "[karaoke_video] backfill: done in {:.1}s -- reel {}/{} backfilled, YouTube {}/{} \
         backfilled, {} orphaned cache file(s)",
        started.elapsed().as_secs_f64(),
        report.reel_backfilled,
        report.reel_files_found,
        report.youtube_backfilled,
        report.youtube_files_found,
        report.orphaned,
    );
    report
}

/// Picks whichever karaoke video is best for `file_hash` to actually show
/// (casting, primarily): a YouTube-background render if one already exists
/// and is fresh relative to the transcript, otherwise the reel-background
/// one (rendering it now if missing/stale, same as `ensure_karaoke_video`
/// alone would). Deliberately never triggers a YouTube lookup/download/
/// render itself -- that's the separate, explicit, slow
/// `ensure_youtube_karaoke_video` action; this only *picks* from what's
/// already on disk for the YouTube side, so casting stays fast and never
/// surprises the user with a network fetch.
pub fn best_karaoke_video_path(file_hash: &str) -> Result<std::path::PathBuf, NightingaleError> {
    let cache = CacheDir::new();
    let youtube_path = cache.youtube_karaoke_video_path(file_hash);
    let transcript_path = cache.transcript_path(file_hash);

    if youtube_path.is_file() && is_fresh(&youtube_path, &transcript_path) {
        info!("[karaoke_video] {file_hash}: using existing YouTube-background render");
        return Ok(youtube_path);
    }

    ensure_karaoke_video(file_hash, false)
}

/// Shared render core for both `ensure_karaoke_video` and
/// `ensure_youtube_background_karaoke_video`: freshness check, load
/// song/transcript/stems, run `select_background` to pick this flavor's
/// background, render, and atomically publish to `video_path`. The two
/// callers differ only in `video_path` and how they resolve a background
/// for a given `duration_secs`.
fn render_karaoke_video_to(
    file_hash: &str,
    force: bool,
    video_path: &std::path::Path,
    select_background: impl FnOnce(f64) -> Option<BackgroundSource>,
) -> Result<std::path::PathBuf, NightingaleError> {
    let cache = CacheDir::new();
    let transcript_path = cache.transcript_path(file_hash);

    if !force && is_fresh(video_path, &transcript_path) {
        return Ok(video_path.to_path_buf());
    }

    let song = library_db::load_song_by_hash(file_hash)
        .map_err(|e| NightingaleError::Other(e.to_string()))?
        .ok_or_else(|| NightingaleError::Other(format!("song not found: {file_hash}")))?;

    let transcript_json = crate::playback::load_transcript(file_hash).map_err(|_| {
        NightingaleError::Other(format!(
            "song {file_hash} has no transcript yet -- analyze it before casting with karaoke video"
        ))
    })?;
    let transcript: TranscriptDoc = serde_json::from_value(transcript_json)?;

    let font_path = ensure_font_downloaded().map_err(NightingaleError::Other)?;
    let font_bytes = std::fs::read(&font_path)?;
    let font = FontArc::try_from_vec(font_bytes)
        .map_err(|e| NightingaleError::Other(format!("invalid karaoke font: {e}")))?;

    let title_line = format!("{} — {}", song.title, song.artist);
    let total_frames = ((song.duration_secs * FRAME_RATE).ceil() as u64).max(1);

    // Same instrumental+guide-vocal split the live player uses
    // (`use-audio-player.ts`), not the raw original mix -- `ensure_mp3_stems`
    // is the same pre-flight the live playback path calls before trusting
    // `get_audio_paths`' returned files actually exist on disk.
    crate::playback::ensure_mp3_stems(file_hash).map_err(|e| {
        NightingaleError::Other(format!("karaoke video: stems not ready for {file_hash}: {e}"))
    })?;
    let audio_paths = crate::playback::get_audio_paths(file_hash);

    let background = select_background(song.duration_secs);
    if let Some(bg) = &background {
        info!(
            "[karaoke_video] using background: {} (start_offset={:.2}s)",
            bg.path, bg.start_offset_secs
        );
    }

    let accent_color = pick_accent_color();

    info!(
        "[karaoke_video] rendering {file_hash} -> {} ({} frames @ {FRAME_RATE}fps, {}x{}, accent={accent_color:?})",
        video_path.display(), total_frames, WIDTH, HEIGHT
    );

    // ffmpeg infers the output container from the extension, so the temp
    // path must still end in `.mp4` (not `.mp4.tmp`) -- same convention as
    // `playback::convert_video_to_mp4`'s `{hash}.{pid}.tmp.mp4`.
    let tmp_path = video_path
        .parent()
        .ok_or_else(|| NightingaleError::Other("invalid karaoke video cache path".to_string()))?
        .join(format!("{file_hash}.{}.tmp.mp4", std::process::id()));
    render_and_encode(
        &font,
        &title_line,
        &song.title,
        &song.artist,
        &transcript.segments,
        total_frames,
        song.duration_secs,
        background.as_ref(),
        accent_color,
        &audio_paths.instrumental,
        audio_paths.vocals.as_deref(),
        &tmp_path,
    )?;
    std::fs::rename(&tmp_path, video_path)?;

    Ok(video_path.to_path_buf())
}

fn is_fresh(video_path: &std::path::Path, transcript_path: &std::path::Path) -> bool {
    let (Ok(video_meta), Ok(transcript_meta)) =
        (video_path.metadata(), transcript_path.metadata())
    else {
        return false;
    };
    let (Ok(video_time), Ok(transcript_time)) = (video_meta.modified(), transcript_meta.modified())
    else {
        return false;
    };
    video_time >= transcript_time
}

/// Background video flavors karaoke rendering will pull from, pooled
/// together rather than picked per-song -- same "don't repeat the same
/// look every time" motivation as `pick_accent_color`. Downloading/
/// building reels for a flavor (see the Settings UI) is what actually
/// populates its pool; flavors with nothing cached just contribute
/// nothing here.
const BACKGROUND_FLAVORS: [&str; 3] = ["nature", "underwater", "space"];

/// A background video for `render_and_encode`, plus how far into it to
/// start (0.0 for the reel/raw-clip pool, where the whole file is fair
/// game; non-zero only for a downloaded YouTube video trimmed to where the
/// song's own audio actually starts -- see
/// `ensure_youtube_background_karaoke_video`).
struct BackgroundSource {
    path: String,
    start_offset_secs: f64,
}

/// Prefers a pre-built reel (`playback::build_background_reels`, filenames
/// `reel_{target_secs}_{n}.mp4`), pooled across every flavor in
/// `BACKGROUND_FLAVORS`, that already covers `duration_secs` -- no
/// per-render scale/crop of a raw (sometimes 4K) clip, and far fewer
/// jump-cuts than looping a single 10-20s clip under a multi-minute song.
/// Falls back to a single raw cached clip (any flavor) if no reel
/// qualifies (song longer than every built reel, or none built yet), then
/// to `None` (solid color) if every flavor's cache is empty entirely.
fn select_background_video(duration_secs: f64) -> Option<String> {
    let mut reels: Vec<(f64, std::path::PathBuf)> = BACKGROUND_FLAVORS
        .iter()
        .flat_map(|flavor| {
            let reels_dir = crate::cache::reels_dir(flavor);
            std::fs::read_dir(&reels_dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let path = e.path();
                    let stem = path.file_stem()?.to_str()?; // "reel_{target_secs}_{n}"
                    let target: f64 = stem.split('_').nth(1)?.parse().ok()?;
                    Some((target, path))
                })
                .collect::<Vec<_>>()
        })
        .collect();

    if !reels.is_empty() {
        reels.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut rng = rand::rng();

        let smallest_qualifying = reels.iter().find(|(len, _)| *len >= duration_secs).map(|(len, _)| *len);
        let pool: Vec<&(f64, std::path::PathBuf)> = match smallest_qualifying {
            Some(len) => reels.iter().filter(|(l, _)| *l == len).collect(),
            // Song longer than every built reel -- fall back to the
            // longest one available (still looped for the excess by the
            // existing `-stream_loop -1` render path).
            None => reels.iter().filter(|(l, _)| *l == reels.last().unwrap().0).collect(),
        };
        if let Some((_, path)) = pool.choose(&mut rng) {
            return Some(path.to_string_lossy().into_owned());
        }
    }

    let raw_clips: Vec<String> = BACKGROUND_FLAVORS
        .iter()
        .flat_map(|flavor| crate::playback::get_cached_pixabay_videos(flavor))
        .collect();
    let mut rng = rand::rng();
    raw_clips.choose(&mut rng).cloned()
}

fn render_and_encode(
    font: &FontArc,
    title_line: &str,
    song_title: &str,
    song_artist: &str,
    segments: &[TranscriptSegment],
    total_frames: u64,
    duration_secs: f64,
    background: Option<&BackgroundSource>,
    accent_color: [u8; 3],
    instrumental_path: &str,
    vocals_path: Option<&str>,
    output_path: &std::path::Path,
) -> Result<(), NightingaleError> {
    let size = format!("{WIDTH}x{HEIGHT}");
    let frame_rate = FRAME_RATE.to_string();

    // Background is always composited via `overlay`, whether it's a real
    // looped Pixabay clip or a synthetic solid-color source -- one code
    // path instead of a "plain" vs. "overlay" branch.
    let mut cmd = silent_command(ffmpeg_path());
    cmd.arg("-y");
    match background {
        Some(bg) => {
            // `-ss` before `-i` is a fast, keyframe-level seek -- fine here
            // (this is a background loop, not something needing
            // frame-exact trimming). `-stream_loop -1` only matters as a
            // safety net if the trimmed remainder is shorter than the
            // song; `overlay=shortest=1` below already stops at whichever
            // of background/lyrics is shorter, so this never overruns.
            if bg.start_offset_secs > 0.0 {
                cmd.args(["-ss", &bg.start_offset_secs.to_string()]);
            }
            cmd.args(["-stream_loop", "-1", "-i", &bg.path]);
        }
        None => {
            let color_src = format!("color=c={BG_COLOR_HEX}:s={size}:d={duration_secs}");
            cmd.args(["-f", "lavfi", "-i", &color_src]);
        }
    }
    cmd.args(["-f", "rawvideo", "-pix_fmt", "rgba"])
        .args(["-s", &size])
        .args(["-r", &frame_rate])
        .args(["-i", "pipe:0"])
        .arg("-i")
        .arg(instrumental_path);
    if let Some(vocals) = vocals_path {
        cmd.arg("-i").arg(vocals);
    }

    let video_filter = format!(
        "[0:v]scale={WIDTH}:{HEIGHT}:force_original_aspect_ratio=increase,\
         crop={WIDTH}:{HEIGHT},fps={FRAME_RATE}[bg];[bg][1:v]overlay=shortest=1:format=auto[v]"
    );
    // Instrumental at full level, guide vocal mixed in quiet underneath --
    // same instrumental+guide-vocal split the live player uses, not the
    // raw original mix. `normalize=0` keeps the instrumental at its
    // original level; `amix`'s default normalization would otherwise
    // also (wrongly) quiet it down just because there are 2 inputs.
    let filter_complex = match vocals_path {
        Some(_) => format!(
            "{video_filter};[3:a]volume={GUIDE_VOCAL_VOLUME}[voc];\
             [2:a][voc]amix=inputs=2:duration=first:dropout_transition=0:normalize=0[a]"
        ),
        None => video_filter,
    };
    let audio_map = if vocals_path.is_some() { "[a]" } else { "2:a:0" };

    cmd.args(["-filter_complex", &filter_complex])
        .args(["-map", "[v]", "-map", audio_map])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "23"])
        .args(["-pix_fmt", "yuv420p"])
        .args(["-c:a", "aac", "-shortest"])
        // Without explicit metadata, players fall back to showing the raw
        // cache filename (the song's file_hash) as the title -- set real
        // tags so they show the song instead.
        .arg("-metadata")
        .arg(format!("title={song_title}"))
        .arg("-metadata")
        .arg(format!("artist={song_artist}"))
        .arg(output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| NightingaleError::Other(format!("failed to spawn ffmpeg: {e}")))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| NightingaleError::Other("ffmpeg stdin unavailable".to_string()))?;

    // Frames are generated lazily inside the writer thread (not
    // precomputed into a Vec) -- a few minutes of 720p RGB24 would be
    // gigabytes held in memory otherwise. Writing from a separate thread
    // (rather than the thread that calls `wait`) avoids the classic
    // pipe deadlock: if ffmpeg's stderr buffer fills while we're blocked
    // writing stdin, and nothing is draining stderr concurrently, both
    // sides stall forever.
    let font = font.clone();
    let title_line = title_line.to_string();
    let segments_owned: Vec<(f64, f64, Vec<(String, f64, f64)>)> = segments
        .iter()
        .map(|s| {
            (
                s.start,
                s.end,
                s.words
                    .iter()
                    .map(|w| (w.word.clone(), w.start, w.end))
                    .collect(),
            )
        })
        .collect();

    let writer = std::thread::spawn(move || -> Result<(), String> {
        let mut buf = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
        for i in 0..total_frames {
            let t = i as f64 / FRAME_RATE;
            render_frame(&mut buf, &font, &title_line, &segments_owned, t, accent_color);
            stdin.write_all(&buf).map_err(|e| e.to_string())?;
        }
        drop(stdin);
        Ok(())
    });

    let output = child
        .wait_with_output()
        .map_err(|e| NightingaleError::Other(format!("ffmpeg wait failed: {e}")))?;

    let write_result = writer
        .join()
        .map_err(|_| NightingaleError::Other("karaoke frame writer thread panicked".to_string()))?;

    if !output.status.success() {
        // ffmpeg itself failed -- its stderr is the useful signal here, not
        // whatever the writer thread saw as a consequence (often also a
        // broken pipe, since a crashed ffmpeg stops reading stdin too).
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NightingaleError::Other(format!(
            "ffmpeg karaoke render failed with status {}: {stderr}",
            output.status
        )));
    }

    // ffmpeg exited successfully. A broken-pipe error from the writer here
    // is an expected, harmless race, not a real failure: `-shortest` on the
    // muxer can make ffmpeg stop reading stdin slightly before we finish
    // writing every frame -- e.g. the stem audio's real duration can be a
    // touch shorter than the tag-derived `song.duration_secs` we sized the
    // frame count from (observed: 74.95s of actual stem audio vs. 75.4s of
    // tag duration for the same song). Only a *different* writer error
    // would indicate an actual problem.
    if let Err(e) = write_result {
        if !e.to_ascii_lowercase().contains("broken pipe") {
            return Err(NightingaleError::Other(e));
        }
        info!(
            "[karaoke_video] writer stopped early after ffmpeg finished (harmless -shortest race): {e}"
        );
    }

    Ok(())
}

fn render_frame(
    buf: &mut [u8],
    font: &FontArc,
    title_line: &str,
    segments: &[(f64, f64, Vec<(String, f64, f64)>)],
    t: f64,
    accent_color: [u8; 3],
) {
    // Fully transparent -- the real background (a looped Pixabay clip or a
    // synthetic solid color) is composited in by ffmpeg's `overlay` filter,
    // not drawn here.
    buf.fill(0);

    draw_line_centered(
        buf,
        font,
        PxScale::from(TITLE_SIZE),
        title_line,
        TITLE_BASELINE_Y,
        TITLE_COLOR,
    );

    if let Some((_, _, words)) = segments
        .iter()
        .find(|(start, end, _)| t >= *start && t <= *end + SEGMENT_LINGER_SECS)
    {
        // Two states: white until a word starts, then it flips to the
        // song's accent color and stays that color (classic progressive
        // karaoke fill) rather than reverting to white once it's done.
        let colored: Vec<(&str, [u8; 3])> = words
            .iter()
            .map(|(word, start, _end)| {
                let color = if t < *start { UNSUNG_COLOR } else { accent_color };
                (word.as_str(), color)
            })
            .collect();
        draw_words_centered(buf, font, PxScale::from(LYRICS_SIZE), &colored, LYRICS_BASELINE_Y);
    }
}

fn measure_width(font: &FontArc, scale: PxScale, text: &str) -> f32 {
    let scaled = font.as_scaled(scale);
    let mut width = 0.0f32;
    let mut prev: Option<GlyphId> = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            width += scaled.kern(p, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

fn draw_text(
    buf: &mut [u8],
    font: &FontArc,
    scale: PxScale,
    text: &str,
    start_x: f32,
    baseline_y: f32,
    color: [u8; 3],
) {
    let scaled = font.as_scaled(scale);
    let mut cursor_x = start_x;
    let mut prev: Option<GlyphId> = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            cursor_x += scaled.kern(p, id);
        }
        let glyph = id.with_scale_and_position(scale, point(cursor_x, baseline_y));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let x = bounds.min.x as i32 + gx as i32;
                let y = bounds.min.y as i32 + gy as i32;
                blend_pixel(buf, x, y, color, coverage);
            });
        }
        cursor_x += scaled.h_advance(id);
        prev = Some(id);
    }
}

fn draw_line_centered(
    buf: &mut [u8],
    font: &FontArc,
    scale: PxScale,
    text: &str,
    baseline_y: f32,
    color: [u8; 3],
) {
    let w = measure_width(font, scale, text);
    let x = (WIDTH as f32 - w) / 2.0;
    draw_text(buf, font, scale, text, x, baseline_y, color);
}

fn draw_words_centered(
    buf: &mut [u8],
    font: &FontArc,
    scale: PxScale,
    words: &[(&str, [u8; 3])],
    baseline_y: f32,
) {
    if words.is_empty() {
        return;
    }
    let space_w = measure_width(font, scale, " ");
    let total: f32 = words.iter().map(|(w, _)| measure_width(font, scale, w)).sum::<f32>()
        + space_w * (words.len() - 1) as f32;
    let mut x = (WIDTH as f32 - total) / 2.0;
    for (i, (word, color)) in words.iter().enumerate() {
        draw_text(buf, font, scale, word, x, baseline_y, *color);
        x += measure_width(font, scale, word);
        if i + 1 < words.len() {
            x += space_w;
        }
    }
}

/// Sets `(x, y)` to `color` with alpha = `coverage`, unconditionally. Used
/// for the final fill pass, which is meant to win outright over whatever
/// the outline passes left at that pixel. The real compositing against
/// whatever's actually behind this pixel happens later, in ffmpeg's
/// `overlay` filter.
fn blend_pixel(buf: &mut [u8], x: i32, y: i32, color: [u8; 3], coverage: f32) {
    if x < 0 || y < 0 || x as u32 >= WIDTH || y as u32 >= HEIGHT || coverage <= 0.0 {
        return;
    }
    let idx = ((y as u32 * WIDTH + x as u32) * 4) as usize;
    let alpha = (coverage.clamp(0.0, 1.0) * 255.0) as u8;
    buf[idx] = color[0];
    buf[idx + 1] = color[1];
    buf[idx + 2] = color[2];
    buf[idx + 3] = alpha;
}
