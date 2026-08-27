//! Downloads a song's official YouTube music video (found via
//! `crate::audiodb`) as source footage for a karaoke video, using the
//! vendored `yt-dlp` binary (`crate::vendor::ensure_ytdlp_downloaded`).
//!
//! yt-dlp doesn't go through an official rate-limited API -- it scrapes
//! YouTube's own player endpoints, same as a browser would -- so the real
//! risk isn't a quota, it's YouTube's anti-bot throttling if hit too fast
//! or too often from one IP. Downloads are serialized process-wide
//! (`LAST_DOWNLOAD_START`) with a minimum gap enforced between them
//! (`MIN_DOWNLOAD_INTERVAL`), so this stays a polite, one-at-a-time fetch
//! even if called in a loop (e.g. building karaoke videos across a whole
//! library) -- never parallel, never back-to-back.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::cache::CacheDir;
use crate::vendor::{ensure_ytdlp_downloaded, ffmpeg_path, silent_command};

/// Minimum quality to accept -- anything short of 1080p is rejected outright
/// (the format selector simply has nothing to match) rather than silently
/// falling back to a lower-res stream that karaoke video generation would
/// then be stuck with.
const MIN_HEIGHT: u32 = 1080;

/// However fast `yt-dlp` itself finishes, never start the *next* download
/// less than this long after the previous one started.
const MIN_DOWNLOAD_INTERVAL: Duration = Duration::from_secs(10);

static LAST_DOWNLOAD_START: Mutex<Option<Instant>> = Mutex::new(None);

/// Downloads `youtube_url` (e.g. `MusicVideoResult.youtube_url`) as an MP4
/// at least `MIN_HEIGHT`p into `CacheDir::youtube_video_path(file_hash)`.
/// Returns the cached path immediately without downloading if it's already
/// there -- a rerun (e.g. a bulk karaoke-video build) doesn't re-fetch.
pub fn ensure_youtube_video_downloaded(
    file_hash: &str,
    youtube_url: &str,
) -> Result<PathBuf, String> {
    let cache = CacheDir::new();
    let dest = cache.youtube_video_path(file_hash);
    if dest.is_file() {
        info!(
            "[youtube_video] {file_hash}: already downloaded at {}, skipping",
            dest.display()
        );
        return Ok(dest);
    }

    info!("[youtube_video] {file_hash}: ensuring yt-dlp is available");
    let ytdlp = ensure_ytdlp_downloaded().map_err(|e| {
        warn!("[youtube_video] {file_hash}: failed to get yt-dlp: {e}");
        e
    })?;

    throttle(file_hash);

    info!("[youtube_video] {file_hash}: downloading {youtube_url} -> {}", dest.display());
    let started = Instant::now();
    let tmp = dest.with_extension("part.mp4");
    let result = download(&ytdlp, youtube_url, &tmp);
    let elapsed = started.elapsed().as_secs_f64();

    match &result {
        Ok(()) => {
            if let Err(e) = std::fs::rename(&tmp, &dest) {
                warn!(
                    "[youtube_video] {file_hash}: downloaded but failed to move into place: {e}"
                );
                return Err(e.to_string());
            }
            info!(
                "[youtube_video] {file_hash}: downloaded in {elapsed:.1}s -> {}",
                dest.display()
            );
        }
        Err(e) => {
            warn!("[youtube_video] {file_hash}: download failed after {elapsed:.1}s: {e}");
            let _ = std::fs::remove_file(&tmp);
        }
    }
    result.map(|()| dest)
}

/// Blocks until at least `MIN_DOWNLOAD_INTERVAL` has passed since the last
/// download *started*, then records this one's start time. Serializes
/// downloads process-wide as a side effect of holding the lock for the
/// whole wait.
fn throttle(file_hash: &str) {
    let mut last = LAST_DOWNLOAD_START.lock().unwrap();
    if let Some(prev) = *last {
        let elapsed = prev.elapsed();
        if elapsed < MIN_DOWNLOAD_INTERVAL {
            let wait = MIN_DOWNLOAD_INTERVAL - elapsed;
            info!(
                "[youtube_video] {file_hash}: throttling, waiting {:.1}s before starting download",
                wait.as_secs_f64()
            );
            std::thread::sleep(wait);
        }
    }
    *last = Some(Instant::now());
}

fn download(ytdlp: &Path, youtube_url: &str, tmp: &Path) -> Result<(), String> {
    // Prefers H.264 (`vcodec^=avc1`) at >=1080p first -- cheaper to decode
    // downstream than VP9/AV1 (this pipeline already leans on hardware
    // H.264 elsewhere, see playback.rs's reel builder), and YouTube's H.264
    // ladder tops out at 1080p anyway so this also avoids pulling a much
    // larger 4K AV1/VP9 stream unnecessarily. Falls back to best video of
    // any codec >=1080p, then a combined stream >=1080p. No fallback below
    // 1080p at any tier -- yt-dlp errors out ("Requested format is not
    // available") instead of silently downloading lower quality.
    let format = format!(
        "bestvideo[height>={MIN_HEIGHT}][vcodec^=avc1]+bestaudio\
         /bestvideo[height>={MIN_HEIGHT}]+bestaudio\
         /best[height>={MIN_HEIGHT}]"
    );

    let output = silent_command(ytdlp)
        .args(["-f", &format])
        .args(["--merge-output-format", "mp4"])
        .arg("--ffmpeg-location")
        .arg(ffmpeg_path())
        .args(["--no-playlist", "--no-progress", "-o"])
        .arg(tmp)
        .arg(youtube_url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(format!(
            "yt-dlp exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}
