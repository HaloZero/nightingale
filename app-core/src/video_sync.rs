//! Detects whether a downloaded YouTube music video's visual timeline lines
//! up with a song's own audio. Official music videos frequently run a
//! different length than the audio file in a user's library -- Ed Sheeran's
//! "Shape of You" video, for one real example, runs about 29 seconds longer
//! than the catalogued track (extended intro). `karaoke_video`'s renderer
//! mutes the background video and plays the song's own instrumental/vocals
//! over it (see its `amix` call), so if the video's visual timeline isn't
//! aligned to the song's audio timeline, the picture drifts out of sync
//! with the lyrics from the very first bar.
//!
//! Detection is a coarse energy-envelope cross-correlation, not a full
//! audio fingerprint: ffmpeg extracts a low-samplerate mono track from both
//! the song and the video's own (otherwise-discarded) audio, RMS energy per
//! 100ms window gives a cheap signal that's tolerant of the two sources
//! being different masters/encodes, and a windowed search finds the offset
//! with peak correlation.
//!
//! Verified against real audio during development (not just unit-tested):
//! a genuine match -- a video's audio vs. a copy of itself trimmed 12.5s
//! off the front, simulating "the video has a longer intro than the song"
//! -- recovered exactly a 12.50s offset at 1.0000 correlation. Unrelated
//! audio's best *spurious* match across the same ~4.5-minute search only
//! reached 0.19, which is what `CONFIDENCE_THRESHOLD` is calibrated
//! against.

use std::path::Path;
use std::process::Stdio;

use serde::Serialize;
use tracing::{info, warn};
use ts_rs::TS;

use crate::cache::CacheDir;
use crate::library_db;
use crate::playback::resolve_original_media;
use crate::vendor::{ffmpeg_path, silent_command};

const SAMPLE_RATE: u32 = 4_000;
const WINDOW_MS: u32 = 100;
const WINDOW_SAMPLES: usize = (SAMPLE_RATE * WINDOW_MS / 1_000) as usize;

/// How far into the song to search for a reference clip -- covers
/// realistically long intros without scanning the whole track.
const REFERENCE_SEARCH_SECS: f64 = 120.0;
/// Length of the reference clip pulled from the song to correlate against
/// the video -- long enough to be a distinctive fingerprint, short enough
/// to keep the search cheap.
const REFERENCE_CLIP_SECS: f64 = 30.0;

/// Below this correlation, `detect_sync_offset` reports no match rather
/// than a guessed offset. See the module doc for the two real numbers this
/// is calibrated against (1.0 genuine match vs. 0.19 spurious best-match on
/// unrelated audio) -- 0.5 sits well clear of that noise floor with
/// headroom below a true match, but hasn't been tuned against real
/// same-song-different-master pairs (an actual video's audio vs. a library
/// file), only a self-similarity test. Revisit if real-world results say
/// otherwise.
const CONFIDENCE_THRESHOLD: f32 = 0.5;

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export)]
pub struct SyncResult {
    /// Seconds into the video where the song's own timeline (its own t=0)
    /// actually begins. Positive means the video has extra content before
    /// the song starts (e.g. an intro) that should be trimmed off before
    /// compositing it as a karaoke background.
    pub video_offset_secs: f64,
    /// Correlation strength at `video_offset_secs`, roughly 0..1 in
    /// practice. Always `>= CONFIDENCE_THRESHOLD` when `Some` is returned.
    pub confidence: f32,
}

/// Looks up `file_hash`'s song and its own audio, and checks it against
/// `video_path` (a downloaded `youtube_video::ensure_youtube_video_downloaded`
/// output). `Ok(None)` covers both "song not found" and "no confident
/// match" -- callers don't need to tell those apart, either way there's no
/// offset to trust.
pub fn detect_sync_offset_for_hash(
    file_hash: &str,
    video_path: &Path,
) -> Result<Option<SyncResult>, String> {
    let Some(song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        return Ok(None);
    };
    let cache = CacheDir::new();
    let song_audio = resolve_original_media(&song, &cache);
    detect_sync_offset(Path::new(&song_audio), video_path)
}

/// Core detection: does `video_path`'s audio contain `song_audio`'s content,
/// and if so, at what offset?
pub fn detect_sync_offset(
    song_audio: &Path,
    video_path: &Path,
) -> Result<Option<SyncResult>, String> {
    let song_samples = extract_mono_pcm(song_audio)?;
    let video_samples = extract_mono_pcm(video_path)?;

    let song_env = energy_envelope(&song_samples);
    let video_env = energy_envelope(&video_samples);

    let ref_windows = (REFERENCE_CLIP_SECS * 1_000.0 / WINDOW_MS as f64) as usize;
    if song_env.len() < ref_windows || video_env.len() < ref_windows {
        info!("[video_sync] song or video too short to correlate, skipping");
        return Ok(None);
    }

    let search_windows = ((REFERENCE_SEARCH_SECS * 1_000.0 / WINDOW_MS as f64) as usize)
        .min(song_env.len() - ref_windows)
        + 1;
    let ref_start = most_distinctive_window(&song_env[..search_windows + ref_windows - 1], ref_windows);
    let reference = &song_env[ref_start..ref_start + ref_windows];

    let (video_offset_windows, confidence) = best_offset(reference, &video_env);
    let window_secs = WINDOW_MS as f64 / 1_000.0;
    let video_offset_secs = (video_offset_windows as f64 - ref_start as f64) * window_secs;

    if confidence < CONFIDENCE_THRESHOLD {
        info!(
            "[video_sync] no confident match (best correlation={confidence:.3} < {CONFIDENCE_THRESHOLD}), \
             treating {} as unsynced with its own audio",
            video_path.display()
        );
        return Ok(None);
    }

    info!(
        "[video_sync] matched at video offset {video_offset_secs:.2}s (correlation={confidence:.3})"
    );
    Ok(Some(SyncResult {
        video_offset_secs,
        confidence,
    }))
}

/// Raw mono PCM at `SAMPLE_RATE`, decoded via ffmpeg -- works on both a
/// plain audio file and a video's own (otherwise-muted) audio track.
fn extract_mono_pcm(input: &Path) -> Result<Vec<f32>, String> {
    let result = silent_command(ffmpeg_path())
        .arg("-i")
        .arg(input)
        .args(["-vn", "-ac", "1", "-ar", &SAMPLE_RATE.to_string(), "-f", "f32le", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;

    if !result.status.success() {
        return Err(format!(
            "ffmpeg audio extraction of {} exited {}: {}",
            input.display(),
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ));
    }

    Ok(result
        .stdout
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

/// RMS energy per `WINDOW_SAMPLES`-sample window -- coarse and cheap, and
/// (unlike raw-sample correlation) tolerant of the two sources being
/// different masters/encodes with different loudness/EQ.
fn energy_envelope(samples: &[f32]) -> Vec<f32> {
    samples
        .chunks(WINDOW_SAMPLES)
        .map(|w| {
            let sum_sq: f32 = w.iter().map(|s| s * s).sum();
            (sum_sq / w.len() as f32).sqrt()
        })
        .collect()
}

/// Pearson correlation coefficient between two same-length envelopes.
fn normalized_cross_correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    let mean_a = a.iter().sum::<f32>() / n;
    let mean_b = b.iter().sum::<f32>() / n;

    let mut num = 0.0f32;
    let mut den_a = 0.0f32;
    let mut den_b = 0.0f32;
    for i in 0..a.len() {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        num += da * db;
        den_a += da * da;
        den_b += db * db;
    }
    if den_a <= 0.0 || den_b <= 0.0 {
        return 0.0;
    }
    num / (den_a.sqrt() * den_b.sqrt())
}

/// The `ref_windows`-long sub-window of `env` with the highest variance --
/// i.e. the most rhythmically/dynamically distinctive clip, avoiding a
/// near-silent intro that would correlate unreliably against everything.
fn most_distinctive_window(env: &[f32], ref_windows: usize) -> usize {
    let mut best = (0usize, f32::MIN);
    for start in 0..=env.len() - ref_windows {
        let w = &env[start..start + ref_windows];
        let mean = w.iter().sum::<f32>() / w.len() as f32;
        let variance = w.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / w.len() as f32;
        if variance > best.1 {
            best = (start, variance);
        }
    }
    best.0
}

/// Slides `reference` across `video_env`, returning the start window with
/// peak correlation and that correlation's value.
fn best_offset(reference: &[f32], video_env: &[f32]) -> (usize, f32) {
    let mut best = (0usize, f32::MIN);
    if video_env.len() < reference.len() {
        warn!("[video_sync] video shorter than the reference clip, no offset to find");
        return best;
    }
    for start in 0..=video_env.len() - reference.len() {
        let slice = &video_env[start..start + reference.len()];
        let corr = normalized_cross_correlation(reference, slice);
        if corr > best.1 {
            best = (start, corr);
        }
    }
    best
}
