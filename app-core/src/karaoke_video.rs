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

/// Bump whenever this file's frame-rendering output changes in a way that
/// should force existing cached videos to re-render even though the
/// underlying transcript hasn't changed -- `is_fresh` compares this against
/// the per-song version recorded in the `karaoke_video_status` table (see
/// `library_db::get_karaoke_video_versions`/`record_karaoke_video_status`)
/// (mtime-only freshness can't detect a pure code/look change, since
/// neither the transcript nor its mtime changed).
///
/// ## Changelog
/// - v1: initial ab_glyph + ffmpeg overlay renderer -- title line and a
///   bare current-line lyrics line, no background pill, no next-line
///   preview.
/// - v2: current line now sits on a rounded, semi-transparent grey pill;
///   the upcoming line is previewed below it, smaller and dimmer, using
///   the same segment lead-in/lookahead rules as the live in-app lyrics
///   display (`lyrics-display.tsx`'s `findCurrentSegment`).
/// - v3: fixed `blend_pixel` overwriting (instead of alpha-compositing)
///   glyph pixels onto the pill -- v2 punched faint but visible streaky
///   holes through the pill along every glyph's antialiased edges, most
///   noticeable on the smaller next-line preview text.
pub(crate) const RENDER_VERSION: u32 = 3;

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
/// so it doesn't vanish mid-breath between segments -- matches the live
/// in-app lyrics display's `SEGMENT_LINGER` (`lyrics-display.tsx`).
const SEGMENT_LINGER_SECS: f64 = 0.5;
/// A line is already considered "current" starting this many seconds
/// before its nominal `start` -- matches the live display's `LYRICS_LEAD`.
const LYRICS_LEAD_SECS: f64 = 0.15;
/// Through a pause shorter than this, `find_current_segment` holds the
/// finished line current until the next line's lead-in begins (rather than
/// dropping to nothing mid-pause) -- matches the live display's
/// `COUNTDOWN_GAP_THRESHOLD`. The video doesn't render the live display's
/// countdown-number overlay, just reuses its gap-bridging threshold.
const COUNTDOWN_GAP_THRESHOLD_SECS: f64 = 3.5;

/// Next-line preview text size, and the current/next pill styling --
/// matches the live in-app lyrics display's current/next line treatment
/// (`lyrics-display.tsx`: `bg-black/40`/`bg-black/25` rounded pills, next
/// line rendered smaller and dimmer).
const NEXT_LYRICS_SIZE: f32 = 40.0;
const LYRICS_PILL_PAD_X: f32 = 40.0;
const LYRICS_PILL_PAD_Y: f32 = 22.0;
const LYRICS_PILL_RADIUS: f32 = 24.0;
const LYRICS_PILL_ALPHA: f32 = 0.40;
const NEXT_LYRICS_PILL_PAD_X: f32 = 28.0;
const NEXT_LYRICS_PILL_PAD_Y: f32 = 14.0;
const NEXT_LYRICS_PILL_RADIUS: f32 = 16.0;
const NEXT_LYRICS_PILL_ALPHA: f32 = 0.25;
/// Gap between the bottom of the current line's pill and the top of the
/// next line's pill.
const NEXT_LYRICS_PILL_GAP: f32 = 16.0;
/// Dim, desaturated grey for the next-line preview -- no accent/unsung
/// color split like the current line, since none of these words are sung
/// yet and won't be by the time they scroll up to become current.
const NEXT_LYRICS_COLOR: [u8; 3] = [190, 190, 190];

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

/// Result of `ensure_youtube_karaoke_video`/`ensure_best_karaoke_video` --
/// "fire a background thread, emit this on completion" shape, plus whether
/// TheAudioDB actually had a music video for this song at all, distinct
/// from a download/sync/render failure after one was found --
/// `music_video_found: false` always means `error: Some(...)` too (nothing
/// to render), but the reverse isn't true.
#[derive(Debug, Clone, Serialize)]
pub struct KaraokeVideoReady {
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
///
/// `force` skips that freshness check entirely and also re-downloads the
/// source video unconditionally (`ensure_youtube_video_downloaded`'s own
/// `force`), discarding whatever's cached -- the "no really, redo this one"
/// half of `ensure_best_karaoke_video`'s forced path. The AudioDB lookup
/// itself is untouched either way (still served from its own cache, see
/// `find_music_video_for_hash`) -- this forces a fresh download+render of
/// whatever video was already matched, not a fresh match.
pub fn ensure_youtube_karaoke_video(file_hash: &str, force: bool) -> KaraokeVideoReady {
    let pipeline_started = std::time::Instant::now();
    let cache = CacheDir::new();
    if !force
        && is_fresh(
            &cache.youtube_karaoke_video_path(file_hash),
            &cache.transcript_path(file_hash),
            file_hash,
            KaraokeVideoKind::Youtube,
        )
    {
        info!(
            "[youtube_karaoke_video] {file_hash}: already have a fresh YouTube-background render, skipping"
        );
        record_karaoke_video_run(&library_db::KaraokeVideoRunRow {
            file_hash,
            kind: "youtube",
            status: "skipped_fresh",
            error: None,
            lookup_ms: None,
            download_ms: None,
            render_ms: None,
            total_ms: pipeline_started.elapsed().as_millis() as u64,
        });
        return KaraokeVideoReady {
            file_hash: file_hash.to_string(),
            music_video_found: true,
            error: None,
        };
    }

    info!("[youtube_karaoke_video] {file_hash}: starting (lookup -> download -> render)");
    let lookup_started = std::time::Instant::now();

    let Some(video) = crate::audiodb::find_music_video_for_hash(file_hash) else {
        info!(
            "[youtube_karaoke_video] {file_hash}: no music video found, stopping (no render attempted)"
        );
        record_karaoke_video_run(&library_db::KaraokeVideoRunRow {
            file_hash,
            kind: "youtube",
            status: "no_video_found",
            error: None,
            lookup_ms: Some(lookup_started.elapsed().as_millis() as u64),
            download_ms: None,
            render_ms: None,
            total_ms: pipeline_started.elapsed().as_millis() as u64,
        });
        return KaraokeVideoReady {
            file_hash: file_hash.to_string(),
            music_video_found: false,
            error: Some("no official music video found for this song".to_string()),
        };
    };
    let lookup_ms = lookup_started.elapsed().as_millis() as u64;
    info!(
        "[youtube_karaoke_video] {file_hash}: found music video {} -- downloading",
        video.youtube_url
    );

    let download_started = std::time::Instant::now();
    if let Err(e) =
        crate::youtube_video::ensure_youtube_video_downloaded(file_hash, &video.youtube_url, force)
    {
        let download_ms = download_started.elapsed().as_millis() as u64;
        warn!(
            "[youtube_karaoke_video] {file_hash}: download failed after {:.1}s: {e}",
            download_ms as f64 / 1000.0
        );
        record_karaoke_video_run(&library_db::KaraokeVideoRunRow {
            file_hash,
            kind: "youtube",
            status: "error",
            error: Some(&e),
            lookup_ms: Some(lookup_ms),
            download_ms: Some(download_ms),
            render_ms: None,
            total_ms: pipeline_started.elapsed().as_millis() as u64,
        });
        return KaraokeVideoReady {
            file_hash: file_hash.to_string(),
            music_video_found: true,
            error: Some(format!("failed to download music video: {e}")),
        };
    }
    let download_ms = download_started.elapsed().as_millis() as u64;
    info!(
        "[youtube_karaoke_video] {file_hash}: download step done in {:.1}s -- rendering",
        download_ms as f64 / 1000.0
    );

    let render_started = std::time::Instant::now();
    let render_result = ensure_youtube_background_karaoke_video(file_hash, true);
    let render_ms = render_started.elapsed().as_millis() as u64;
    let total_ms = pipeline_started.elapsed().as_millis() as u64;
    match render_result {
        Ok(_) => {
            info!(
                "[youtube_karaoke_video] {file_hash}: render done in {:.1}s, pipeline total {:.1}s",
                render_ms as f64 / 1000.0,
                total_ms as f64 / 1000.0
            );
            record_karaoke_video_run(&library_db::KaraokeVideoRunRow {
                file_hash,
                kind: "youtube",
                status: "rendered",
                error: None,
                lookup_ms: Some(lookup_ms),
                download_ms: Some(download_ms),
                render_ms: Some(render_ms),
                total_ms,
            });
            KaraokeVideoReady {
                file_hash: file_hash.to_string(),
                music_video_found: true,
                error: None,
            }
        }
        Err(e) => {
            warn!(
                "[youtube_karaoke_video] {file_hash}: render failed after {:.1}s: {e}",
                render_ms as f64 / 1000.0
            );
            let error_msg = e.to_string();
            record_karaoke_video_run(&library_db::KaraokeVideoRunRow {
                file_hash,
                kind: "youtube",
                status: "error",
                error: Some(&error_msg),
                lookup_ms: Some(lookup_ms),
                download_ms: Some(download_ms),
                render_ms: Some(render_ms),
                total_ms,
            });
            KaraokeVideoReady {
                file_hash: file_hash.to_string(),
                music_video_found: true,
                error: Some(format!("failed to render karaoke video: {e}")),
            }
        }
    }
}

/// The single "get me a karaoke video" action exposed in the UI -- replaces
/// what used to be four separate actions (render/force-rerender reel,
/// fetch/force-refetch YouTube) with two: this, and this with `force: true`.
/// Always prefers a YouTube-background render (`ensure_youtube_karaoke_video`:
/// TheAudioDB lookup -> yt-dlp download -> render), falling back to the
/// reel-background pipeline (`ensure_karaoke_video`) only when no
/// YouTube-backed video could be produced -- no official music video found,
/// or one was found but couldn't be downloaded or confidently synced.
/// `music_video_found`/`error` describe the YouTube attempt specifically
/// (see `KaraokeVideoReady`'s doc comment); `error` is only set here
/// if *both* the YouTube attempt and the reel fallback fail.
///
/// `force` first clears both cached flavors via `clear_cached_karaoke_
/// videos` -- without that, a flavor that doesn't happen to regenerate this
/// run (e.g. a YouTube video is found this time where previously none was,
/// so the old reel fallback never gets touched) would keep looking fresh to
/// `is_fresh` and could still win the "best" pick over the fresh one.
pub fn ensure_best_karaoke_video(file_hash: &str, force: bool) -> KaraokeVideoReady {
    if force {
        clear_cached_karaoke_videos(file_hash);
    }

    let youtube_result = ensure_youtube_karaoke_video(file_hash, force);
    if youtube_result.error.is_none() {
        return youtube_result;
    }

    info!(
        "[karaoke_video] {file_hash}: no YouTube-background render available ({}), falling back \
         to reel background",
        youtube_result.error.as_deref().unwrap_or("unknown error")
    );
    match ensure_karaoke_video(file_hash, force) {
        Ok(_) => KaraokeVideoReady {
            file_hash: file_hash.to_string(),
            music_video_found: youtube_result.music_video_found,
            error: None,
        },
        Err(e) => KaraokeVideoReady {
            file_hash: file_hash.to_string(),
            music_video_found: youtube_result.music_video_found,
            error: Some(format!(
                "no YouTube-background render ({}) and reel fallback also failed: {e}",
                youtube_result.error.unwrap_or_default()
            )),
        },
    }
}

/// Deletes both cached karaoke-video flavors for `file_hash` (reel- and
/// YouTube-background) and zeroes their recorded version in
/// `karaoke_video_status`, mirroring the reset onto `Song.karaoke_video_
/// version`/`youtube_karaoke_video_version` same as `record_karaoke_video_
/// status_at_version` does for a successful render. Called by `ensure_best_
/// karaoke_video`'s `force` path before regenerating -- see that function's
/// doc comment for why a forced "start over" needs both flavors cleared
/// up front, not just whichever one this run ends up re-rendering.
fn clear_cached_karaoke_videos(file_hash: &str) {
    let cache = CacheDir::new();
    for path in [
        cache.karaoke_video_path(file_hash),
        cache.youtube_karaoke_video_path(file_hash),
    ] {
        if path.is_file() {
            if let Err(e) = std::fs::remove_file(&path) {
                warn!(
                    "[karaoke_video] {file_hash}: failed to remove {}: {e}",
                    path.display()
                );
            }
        }
    }
    if let Err(e) = library_db::set_karaoke_video_version(file_hash, 0) {
        warn!("[karaoke_video] {file_hash}: failed to clear reel version: {e}");
    }
    if let Err(e) = library_db::set_youtube_karaoke_video_version(file_hash, 0) {
        warn!("[karaoke_video] {file_hash}: failed to clear YouTube version: {e}");
    }
    if let Ok(Some(mut song)) = library_db::load_song_by_hash(file_hash) {
        song.karaoke_video_version = 0;
        song.youtube_karaoke_video_version = 0;
        if let Err(e) = library_db::update_song_fields(file_hash, &song) {
            warn!(
                "[karaoke_video] {file_hash}: failed to mirror cleared karaoke video status onto \
                 song: {e}"
            );
        }
    }
}

/// Shared bulk-dispatch shape for the karaoke video actions below:
/// resolve `filters` to eligible hashes (`iter_file_hashes_filtered_
/// karaoke_renderable`), hand the count back immediately, and run `action`
/// over each hash sequentially on its own background thread. Backgrounded
/// (like `analyzer::refresh_metadata_all`, unlike `reanalyze_all_full`)
/// because each call here does real ffmpeg/network work directly and
/// blocking -- there's no existing worker queue for karaoke video the way
/// there is for analysis. Sequential *within* one call, not
/// one-thread-per-song: N parallel ffmpeg encodes (or, for the YouTube
/// variant, N concurrent yt-dlp downloads racing past `youtube_video`'s own
/// throttle) would defeat the point of the throttling/single-flight care
/// already taken per-song.
///
/// `best_karaoke_video_all`/`force_best_karaoke_video_all` each call this
/// twice, once per `kind`, deliberately -- two independent single-file
/// walks on two independent threads, so a slow/throttled/failing YouTube
/// walk never stalls the (much faster, purely local) reel walk. See their
/// doc comments for why that split exists.
///
/// Logs a `(i/count)` position line before each song and a done/failed line
/// with that song's elapsed time after, plus a total-elapsed line when the
/// whole batch finishes -- `action` reports outcome via `Result` instead of
/// being fire-and-forget so a per-song failure actually surfaces here rather
/// than vanishing into a discarded `Result`.
fn bulk_karaoke_video(
    filters: &LibraryMenuFilters,
    kind: crate::video_queue::VideoQueueKind,
    action: fn(&str) -> Result<(), String>,
) -> usize {
    let hashes =
        library_db::iter_file_hashes_filtered_karaoke_renderable(filters).unwrap_or_default();
    let count = hashes.len();
    info!("[karaoke_video] bulk action starting for {count} eligible song(s)");
    crate::video_queue::mark_queued_many(kind, &hashes);
    std::thread::spawn(move || {
        let batch_started = std::time::Instant::now();
        for (i, hash) in hashes.iter().enumerate() {
            let position = i + 1;
            let label = song_label(hash);
            let song_started = std::time::Instant::now();
            info!("[karaoke_video] ({position}/{count}) starting {label}");
            let token = crate::video_queue::mark_processing(hash, kind);
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
            crate::video_queue::clear(hash, kind, &token);
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

/// Bulk counterpart to `ensure_best_karaoke_video` -- unlike that
/// per-song, YouTube-first-with-reel-fallback chain, this dispatches two
/// *independent* sweeps over the same eligible-song list, each on its own
/// background thread (two separate `bulk_karaoke_video` calls): one renders
/// reels directly, the other drives the YouTube lookup/download/render
/// pipeline. They used to be one combined sweep that tried YouTube first
/// and only rendered a reel on failure, all on a single thread -- since
/// TheAudioDB lookups and yt-dlp downloads are throttled at the source
/// (`audiodb::MIN_LOOKUP_INTERVAL`/`youtube_video::MIN_DOWNLOAD_INTERVAL`)
/// and have no timeout, a slow or hung YouTube step for one song stalled
/// reel rendering for every song behind it in the walk. Running them
/// independently means reel coverage for the whole filtered set finishes at
/// its own (CPU/ffmpeg-bound, much faster) pace regardless of how YouTube's
/// slower, less reliable pipeline is doing; `best_karaoke_video_path`
/// already prefers a YouTube render over reel whenever one exists on disk,
/// so casting picks up each song's YouTube video automatically whenever the
/// YouTube sweep gets to it -- no coordination needed between the two.
///
/// The reel sweep skips a song entirely (`has_fresh_youtube_video`) if it
/// already has a fresh YouTube-background render, so re-running this after
/// the YouTube sweep has filled in doesn't waste time re-rendering reels
/// nobody will see. The very first pass over a song can't know in advance
/// whether YouTube will succeed, so some reels that turn out to be
/// superseded by YouTube shortly after are an accepted tradeoff of not
/// blocking reel on YouTube's outcome.
pub fn best_karaoke_video_all(filters: &LibraryMenuFilters) -> usize {
    bulk_karaoke_video(filters, crate::video_queue::VideoQueueKind::Reel, |hash| {
        if has_fresh_youtube_video(hash) {
            return Ok(());
        }
        ensure_karaoke_video(hash, false)
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    bulk_karaoke_video(filters, crate::video_queue::VideoQueueKind::Youtube, |hash| {
        ensure_youtube_karaoke_video(hash, false)
            .error
            .map_or(Ok(()), Err)
    })
}

/// Bulk "Force" counterpart -- same two independent sweeps as
/// `best_karaoke_video_all`, but each unconditionally regenerates its own
/// flavor (skipping the freshness/`has_fresh_youtube_video` checks) instead
/// of clearing both flavors together the way the per-song forced action
/// does. Deliberately simpler than that: force-refreshing reels
/// library-wide shouldn't also wipe out already-good YouTube videos (or
/// vice versa), so each sweep only ever touches its own cached flavor.
pub fn force_best_karaoke_video_all(filters: &LibraryMenuFilters) -> usize {
    bulk_karaoke_video(filters, crate::video_queue::VideoQueueKind::Reel, |hash| {
        ensure_karaoke_video(hash, true)
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    bulk_karaoke_video(filters, crate::video_queue::VideoQueueKind::Youtube, |hash| {
        ensure_youtube_karaoke_video(hash, true)
            .error
            .map_or(Ok(()), Err)
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
    let started = std::time::Instant::now();
    let video_path = CacheDir::new().karaoke_video_path(file_hash);
    let result = render_karaoke_video_to(
        file_hash,
        force,
        &video_path,
        KaraokeVideoKind::Reel,
        |duration_secs| {
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
    let total_ms = started.elapsed().as_millis() as u64;

    match &result {
        Ok(outcome) => {
            record_karaoke_video_status(file_hash, KaraokeVideoKind::Reel);
            record_karaoke_video_run(&library_db::KaraokeVideoRunRow {
                file_hash,
                kind: "reel",
                status: if outcome.skipped_fresh {
                    "skipped_fresh"
                } else {
                    "rendered"
                },
                error: None,
                lookup_ms: None,
                download_ms: None,
                render_ms: outcome.render_ms,
                total_ms,
            });
        }
        Err(e) => {
            let error_msg = e.to_string();
            record_karaoke_video_run(&library_db::KaraokeVideoRunRow {
                file_hash,
                kind: "reel",
                status: "error",
                error: Some(&error_msg),
                lookup_ms: None,
                download_ms: None,
                render_ms: None,
                total_ms,
            });
        }
    }

    result.map(|outcome| outcome.path)
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

    let sync = crate::video_sync::ensure_synced_offset(file_hash)
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
    let result = render_karaoke_video_to(
        file_hash,
        force,
        &video_path,
        KaraokeVideoKind::Youtube,
        move |_duration_secs| Some(background),
    );
    if result.is_ok() {
        record_karaoke_video_status(file_hash, KaraokeVideoKind::Youtube);
    }
    result.map(|outcome| outcome.path)
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
/// rendered successfully with the current `RENDER_VERSION`, then mirrors
/// the table's current combined state onto
/// `Song.karaoke_video_version`/`youtube_karaoke_video_version` so the song
/// list can show it per row without an extra query. Only ever called after
/// a successful render -- a failed render never invalidates a
/// pre-existing successful one (see `render_karaoke_video_to`'s
/// tmp-file-then-rename), so there's no corresponding "mark absent" path.
///
/// Skips the write entirely if this flavor is already recorded at the
/// current version: both `ensure_karaoke_video`/
/// `ensure_youtube_background_karaoke_video` call this on every `Ok`
/// return, including the freshness-check fast path that does no actual
/// rendering (e.g. every cast via `best_karaoke_video_path`) -- without
/// this check that path would do a full side-table write plus
/// song-payload read/write on every call, defeating the point of it being
/// a fast path. Still self-heals for libraries that had karaoke videos on
/// disk from before this table existed, or before per-version tracking
/// existed: the first `Ok` after upgrade finds a stale/absent version and
/// backfills it once.
fn record_karaoke_video_status(file_hash: &str, kind: KaraokeVideoKind) {
    record_karaoke_video_status_at_version(file_hash, kind, RENDER_VERSION);
}

/// Shared by `record_karaoke_video_status` (always stamps the *current*
/// `RENDER_VERSION`, since it's only called right after a real render) and
/// `backfill_karaoke_video_status_from_cache` (stamps a fixed `1`, since a
/// cache file discovered with no tracking row at all necessarily predates
/// per-version tracking, and therefore the pill/next-line preview from
/// `RENDER_VERSION` 2).
fn record_karaoke_video_status_at_version(file_hash: &str, kind: KaraokeVideoKind, version: u32) {
    let (reel_version, youtube_version) =
        library_db::get_karaoke_video_versions(file_hash).unwrap_or_default();
    let already_recorded = match kind {
        KaraokeVideoKind::Reel => reel_version == version,
        KaraokeVideoKind::Youtube => youtube_version == version,
    };
    if already_recorded {
        return;
    }

    let write_result = match kind {
        KaraokeVideoKind::Reel => library_db::set_karaoke_video_version(file_hash, version),
        KaraokeVideoKind::Youtube => {
            library_db::set_youtube_karaoke_video_version(file_hash, version)
        }
    };
    if let Err(e) = write_result {
        warn!("[karaoke_video] {file_hash}: failed to record karaoke video status: {e}");
        return;
    }

    let Ok(Some(mut song)) = library_db::load_song_by_hash(file_hash) else {
        return;
    };
    song.karaoke_video_version = match kind {
        KaraokeVideoKind::Reel => version,
        KaraokeVideoKind::Youtube => reel_version,
    };
    song.youtube_karaoke_video_version = match kind {
        KaraokeVideoKind::Youtube => version,
        KaraokeVideoKind::Reel => youtube_version,
    };
    if let Err(e) = library_db::update_song_fields(file_hash, &song) {
        warn!("[karaoke_video] {file_hash}: failed to mirror karaoke video status onto song: {e}");
    }
}

/// Thin wrapper around `library_db::insert_karaoke_video_run` so call sites
/// don't each have to handle the (unlikely, but possible) insert failure --
/// a failure to log a run is worth a warning, never worth failing the
/// actual render/fetch over.
fn record_karaoke_video_run(row: &library_db::KaraokeVideoRunRow) {
    if let Err(e) = library_db::insert_karaoke_video_run(row) {
        warn!(
            "[karaoke_video] {}: failed to record run: {e}",
            row.file_hash
        );
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

        let (reel_version, youtube_version) =
            library_db::get_karaoke_video_versions(file_hash).unwrap_or_default();
        let already_recorded = match kind {
            KaraokeVideoKind::Reel => reel_version != 0,
            KaraokeVideoKind::Youtube => youtube_version != 0,
        };
        if already_recorded {
            continue;
        }

        match library_db::load_song_by_hash(file_hash) {
            Ok(Some(_)) => {
                // Fixed `1`, not `RENDER_VERSION`: a cache file with no
                // tracking row at all necessarily predates per-version
                // tracking (see `record_karaoke_video_status_at_version`'s
                // doc comment).
                record_karaoke_video_status_at_version(file_hash, kind, 1);
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
    if has_fresh_youtube_video(file_hash) {
        info!("[karaoke_video] {file_hash}: using existing YouTube-background render");
        return Ok(CacheDir::new().youtube_karaoke_video_path(file_hash));
    }

    ensure_karaoke_video(file_hash, false)
}

/// True if a fresh YouTube-background render already exists for
/// `file_hash` -- factored out of `best_karaoke_video_path`'s inline check
/// since the decoupled bulk reel sweep (`best_karaoke_video_all`) also
/// needs it, to skip rendering a reel nobody will see once YouTube has
/// already produced a nicer video for that song.
fn has_fresh_youtube_video(file_hash: &str) -> bool {
    let cache = CacheDir::new();
    let youtube_path = cache.youtube_karaoke_video_path(file_hash);
    youtube_path.is_file()
        && is_fresh(
            &youtube_path,
            &cache.transcript_path(file_hash),
            file_hash,
            KaraokeVideoKind::Youtube,
        )
}

/// Outcome of `render_karaoke_video_to`, distinguishing "nothing to do,
/// already fresh" from "actually rendered" so callers can log the right
/// `karaoke_video_runs.status` (see `record_karaoke_video_run`) instead of
/// only ever being able to say "succeeded."
struct RenderOutcome {
    path: std::path::PathBuf,
    skipped_fresh: bool,
    /// The `render_and_encode` + atomic-publish duration alone, excluding
    /// song/transcript/stems loading -- `None` when `skipped_fresh`.
    render_ms: Option<u64>,
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
    kind: KaraokeVideoKind,
    select_background: impl FnOnce(f64) -> Option<BackgroundSource>,
) -> Result<RenderOutcome, NightingaleError> {
    let cache = CacheDir::new();
    let transcript_path = cache.transcript_path(file_hash);

    if !force && is_fresh(video_path, &transcript_path, file_hash, kind) {
        return Ok(RenderOutcome {
            path: video_path.to_path_buf(),
            skipped_fresh: true,
            render_ms: None,
        });
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
    let render_started = std::time::Instant::now();
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
    let render_ms = render_started.elapsed().as_millis() as u64;

    Ok(RenderOutcome {
        path: video_path.to_path_buf(),
        skipped_fresh: false,
        render_ms: Some(render_ms),
    })
}

/// A cached video is fresh only if it's both newer than the transcript it
/// was rendered from *and* was rendered by the current `RENDER_VERSION` --
/// mtime comparison alone can't catch a pure look/code change (bumping
/// `RENDER_VERSION` on its own doesn't touch the transcript or its mtime),
/// so every existing cached video is force-re-rendered exactly once after
/// a version bump, picking up the new pipeline's look the next time it's
/// requested (rendered, or cast via `best_karaoke_video_path`).
fn is_fresh(
    video_path: &std::path::Path,
    transcript_path: &std::path::Path,
    file_hash: &str,
    kind: KaraokeVideoKind,
) -> bool {
    let (Ok(video_meta), Ok(transcript_meta)) =
        (video_path.metadata(), transcript_path.metadata())
    else {
        return false;
    };
    let (Ok(video_time), Ok(transcript_time)) = (video_meta.modified(), transcript_meta.modified())
    else {
        return false;
    };
    if video_time < transcript_time {
        return false;
    }

    let (reel_version, youtube_version) =
        library_db::get_karaoke_video_versions(file_hash).unwrap_or_default();
    let recorded_version = match kind {
        KaraokeVideoKind::Reel => reel_version,
        KaraokeVideoKind::Youtube => youtube_version,
    };
    recorded_version == RENDER_VERSION
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

type Segment = (f64, f64, Vec<(String, f64, f64)>);

/// Finds which segment should be treated as "current" at `t`, mirroring
/// `findCurrentSegment` in `lyrics-display.tsx`: a finished line is held
/// current through a short pause (`COUNTDOWN_GAP_THRESHOLD_SECS`) until the
/// next line's lead-in begins, and a line already in its lead-in window
/// jumps ahead of one that just finished. Always returns an index -- even
/// well before the first line starts or after the last one ends -- since
/// the live display separately gates *visibility* on `is_segment_active`,
/// not on this lookup. Unlike the live display this takes no scan-start
/// hint: each output frame is computed independently from scratch anyway
/// (no animation-frame budget to protect), so a plain linear scan over
/// what's normally a few hundred segments is simplest.
fn find_current_segment(segments: &[Segment], t: f64) -> usize {
    for i in 0..segments.len() {
        let (_start, end, _) = &segments[i];
        if t >= end + SEGMENT_LINGER_SECS {
            if let Some((next_start, _, _)) = segments.get(i + 1) {
                let gap_to_next = next_start - end;
                if gap_to_next < COUNTDOWN_GAP_THRESHOLD_SECS && t < next_start - LYRICS_LEAD_SECS
                {
                    return i;
                }
            }
            continue;
        }

        if let Some((next_start, _, _)) = segments.get(i + 1) {
            if t >= next_start - LYRICS_LEAD_SECS {
                continue;
            }
        }
        return i;
    }
    segments.len().saturating_sub(1)
}

/// Whether the segment `find_current_segment` picked should actually be
/// shown at `t` -- false during a long gap before the first/between lines,
/// matching the live display's `isActive` check.
fn is_segment_active(segment: &Segment, t: f64) -> bool {
    t >= segment.0 - LYRICS_LEAD_SECS && t <= segment.1 + SEGMENT_LINGER_SECS
}

fn render_frame(
    buf: &mut [u8],
    font: &FontArc,
    title_line: &str,
    segments: &[Segment],
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

    if segments.is_empty() {
        return;
    }
    let idx = find_current_segment(segments, t);
    if !is_segment_active(&segments[idx], t) {
        return;
    }

    // Two states: white until a word starts, then it flips to the song's
    // accent color and stays that color (classic progressive karaoke fill)
    // rather than reverting to white once it's done.
    let (_, _, words) = &segments[idx];
    let colored: Vec<(&str, [u8; 3])> = words
        .iter()
        .map(|(word, start, _end)| {
            let color = if t < *start { UNSUNG_COLOR } else { accent_color };
            (word.as_str(), color)
        })
        .collect();
    let pill_bottom = draw_pill_line(
        buf,
        font,
        PxScale::from(LYRICS_SIZE),
        &colored,
        LYRICS_BASELINE_Y,
        LYRICS_PILL_PAD_X,
        LYRICS_PILL_PAD_Y,
        LYRICS_PILL_RADIUS,
        LYRICS_PILL_ALPHA,
    );

    // Preview of the upcoming line, smaller and dimmer -- same "show the
    // next line" behavior as the live display's next-line pill, just
    // without its countdown-number overlay (this renderer has no separate
    // countdown treatment at all, live or otherwise).
    if let Some((_, _, next_words)) = segments.get(idx + 1) {
        if !next_words.is_empty() {
            let next_scale = PxScale::from(NEXT_LYRICS_SIZE);
            let next_ascent = font.as_scaled(next_scale).ascent();
            let next_baseline =
                pill_bottom + NEXT_LYRICS_PILL_GAP + next_ascent + NEXT_LYRICS_PILL_PAD_Y;
            let next_colored: Vec<(&str, [u8; 3])> = next_words
                .iter()
                .map(|(word, _, _)| (word.as_str(), NEXT_LYRICS_COLOR))
                .collect();
            draw_pill_line(
                buf,
                font,
                next_scale,
                &next_colored,
                next_baseline,
                NEXT_LYRICS_PILL_PAD_X,
                NEXT_LYRICS_PILL_PAD_Y,
                NEXT_LYRICS_PILL_RADIUS,
                NEXT_LYRICS_PILL_ALPHA,
            );
        }
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

fn measure_words_width(font: &FontArc, scale: PxScale, words: &[(&str, [u8; 3])]) -> f32 {
    if words.is_empty() {
        return 0.0;
    }
    let space_w = measure_width(font, scale, " ");
    words.iter().map(|(w, _)| measure_width(font, scale, w)).sum::<f32>()
        + space_w * (words.len() - 1) as f32
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
    let total = measure_words_width(font, scale, words);
    let mut x = (WIDTH as f32 - total) / 2.0;
    for (i, (word, color)) in words.iter().enumerate() {
        draw_text(buf, font, scale, word, x, baseline_y, *color);
        x += measure_width(font, scale, word);
        if i + 1 < words.len() {
            x += space_w;
        }
    }
}

/// Draws one lyrics line on top of a semi-transparent grey rounded pill
/// sized to fit it -- matching the live in-app lyrics display's current/
/// next line pills (`lyrics-display.tsx`). No-ops (drawing nothing, pill
/// included) if `words` is empty. Returns the pill's bottom y so a
/// following line (the next-line preview) can be stacked directly below
/// it, using this line's own baseline if there's no pill to anchor to.
#[allow(clippy::too_many_arguments)]
fn draw_pill_line(
    buf: &mut [u8],
    font: &FontArc,
    scale: PxScale,
    words: &[(&str, [u8; 3])],
    baseline_y: f32,
    pad_x: f32,
    pad_y: f32,
    radius: f32,
    pill_alpha: f32,
) -> f32 {
    if words.is_empty() {
        return baseline_y;
    }

    let width = measure_words_width(font, scale, words);
    let scaled = font.as_scaled(scale);
    // `descent()` is negative (extends below the baseline), so subtracting
    // it moves *down* from the baseline -- same convention `ascent()`
    // (positive, above the baseline) relies on for `top` below.
    let top = baseline_y - scaled.ascent() - pad_y;
    let bottom = baseline_y - scaled.descent() + pad_y;
    let half_w = width / 2.0 + pad_x;
    let center_x = WIDTH as f32 / 2.0;

    draw_pill(buf, center_x - half_w, top, center_x + half_w, bottom, radius, pill_alpha);
    draw_words_centered(buf, font, scale, words, baseline_y);
    bottom
}

/// Alpha-blends a semi-transparent grey rounded rectangle into `buf` using
/// a standard rounded-box signed-distance field (clamp the pixel into the
/// inner, non-rounded rect, then compare its distance from that clamped
/// point to `radius`) with a 1px antialiased edge. Always plain black at
/// `alpha` -- matching the live display's `bg-black/40`/`bg-black/25`
/// pills (visually a dark, slightly-transparent grey against most
/// backgrounds), not a literal grey color.
fn draw_pill(buf: &mut [u8], x0: f32, y0: f32, x1: f32, y1: f32, radius: f32, alpha: f32) {
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let r = radius.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
    let xi0 = x0.floor().max(0.0) as i32;
    let xi1 = x1.ceil().min(WIDTH as f32) as i32;
    let yi0 = y0.floor().max(0.0) as i32;
    let yi1 = y1.ceil().min(HEIGHT as f32) as i32;

    for y in yi0..yi1 {
        for x in xi0..xi1 {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            let cx = fx.clamp(x0 + r, x1 - r);
            let cy = fy.clamp(y0 + r, y1 - r);
            let dist = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            // Inside the shape once `dist <= r`; ramp coverage down to 0
            // over a 1px band past that edge for antialiasing.
            let coverage = (r - dist + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let idx = ((y as u32 * WIDTH + x as u32) * 4) as usize;
            buf[idx] = 0;
            buf[idx + 1] = 0;
            buf[idx + 2] = 0;
            buf[idx + 3] = (alpha * coverage * 255.0) as u8;
        }
    }
}

/// Source-over alpha-composites `color` at `coverage` opacity onto whatever
/// is already at `(x, y)`, instead of blindly overwriting it. This matters
/// once glyphs can be drawn on top of a non-transparent backdrop (the pill,
/// added for `RENDER_VERSION` 2): a glyph's antialiased edge pixels have
/// low but nonzero `coverage`, and overwriting outright (the pre-pill
/// behavior, safe back when every backdrop pixel was fully transparent)
/// replaced the pill's own alpha at those pixels with the glyph's much
/// lower edge alpha -- visually, a faint but very visible streaky/torn
/// look punched through the pill along every glyph's antialiased edges,
/// most noticeable on the smaller next-line preview text. Straight (not
/// premultiplied) alpha compositing, matching how alpha is stored
/// everywhere else in this buffer.
fn blend_pixel(buf: &mut [u8], x: i32, y: i32, color: [u8; 3], coverage: f32) {
    if x < 0 || y < 0 || x as u32 >= WIDTH || y as u32 >= HEIGHT || coverage <= 0.0 {
        return;
    }
    let idx = ((y as u32 * WIDTH + x as u32) * 4) as usize;
    let src_a = coverage.clamp(0.0, 1.0);
    let dst_a = buf[idx + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        buf[idx + 3] = 0;
        return;
    }
    for c in 0..3 {
        let src_c = color[c] as f32;
        let dst_c = buf[idx + c] as f32;
        let out_c = (src_c * src_a + dst_c * dst_a * (1.0 - src_a)) / out_a;
        buf[idx + c] = out_c.round().clamp(0.0, 255.0) as u8;
    }
    buf[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}
