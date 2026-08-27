//! Per-run history log for the two karaoke-video pipelines.
//!
//! One row per `karaoke_video::ensure_karaoke_video` /
//! `ensure_youtube_karaoke_video` invocation, recording which stage it
//! reached and how long each took -- see
//! `migrations::ensure_karaoke_video_runs_table` for how this differs from
//! `karaoke_video_status`. Writing to this table is unconditional (unlike
//! `analysis_timings`, which is gated behind a config flag) since a failed
//! or no-op run is exactly the kind of thing worth keeping a record of
//! here, not just successes.

use rusqlite::params;

use super::connection::with_conn_mut;

pub struct KaraokeVideoRunRow<'a> {
    pub file_hash: &'a str,
    /// `"reel"` | `"youtube"`.
    pub kind: &'a str,
    /// `"rendered"` | `"skipped_fresh"` | `"no_video_found"` (youtube only)
    /// | `"error"`.
    pub status: &'a str,
    pub error: Option<&'a str>,
    /// TheAudioDB lookup stage -- youtube only.
    pub lookup_ms: Option<u64>,
    /// yt-dlp download stage -- youtube only.
    pub download_ms: Option<u64>,
    /// ffmpeg render stage -- both kinds; `None` when `status` is
    /// `skipped_fresh` or `no_video_found` (no render was attempted).
    pub render_ms: Option<u64>,
    pub total_ms: u64,
}

pub fn insert_karaoke_video_run(row: &KaraokeVideoRunRow) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO karaoke_video_runs (
                file_hash, kind, started_at, status, error,
                lookup_ms, download_ms, render_ms, total_ms
            ) VALUES (
                ?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?3, ?4,
                ?5, ?6, ?7, ?8
            )",
            params![
                row.file_hash,
                row.kind,
                row.status,
                row.error,
                row.lookup_ms.map(|v| v as i64),
                row.download_ms.map(|v| v as i64),
                row.render_ms.map(|v| v as i64),
                row.total_ms as i64,
            ],
        )?;
        Ok(())
    })
}
