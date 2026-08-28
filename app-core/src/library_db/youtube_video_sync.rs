//! Cache of `video_sync::detect_sync_offset_for_hash`'s result, one row per
//! song -- see `migrations::ensure_youtube_video_sync_table` for why this is
//! a separate table. Detection is a real cost (ffmpeg-decodes both the
//! song's audio and the downloaded video's audio, then an O(n*m)
//! correlation search), so this exists purely to make sure it only ever
//! runs once per song -- see `video_sync::ensure_synced_offset`, the only
//! caller.

use rusqlite::{OptionalExtension, params};

use super::connection::{with_conn, with_conn_mut};

/// A cached detection outcome. `video_offset_secs: None` means detection
/// ran and found no confident match -- distinct from no row existing at
/// all, which means detection has never run for this song.
pub struct YoutubeVideoSyncRow {
    pub video_offset_secs: Option<f64>,
    pub confidence: Option<f32>,
}

/// `Ok(None)` means detection has never run -- caller should run it and
/// call `record_youtube_video_sync`. `Ok(Some(row))` means it has, whether
/// or not a confident match was found (`row.video_offset_secs`).
pub fn get_youtube_video_sync(file_hash: &str) -> rusqlite::Result<Option<YoutubeVideoSyncRow>> {
    with_conn(|c| {
        c.query_row(
            "SELECT video_offset_secs, confidence FROM youtube_video_sync WHERE file_hash = ?1",
            [file_hash],
            |r| {
                Ok(YoutubeVideoSyncRow {
                    video_offset_secs: r.get(0)?,
                    confidence: r.get::<_, Option<f64>>(1)?.map(|c| c as f32),
                })
            },
        )
        .optional()
    })
}

pub fn record_youtube_video_sync(
    file_hash: &str,
    video_offset_secs: Option<f64>,
    confidence: Option<f32>,
) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO youtube_video_sync (file_hash, video_offset_secs, confidence, computed_at)
             VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(file_hash) DO UPDATE SET
               video_offset_secs = excluded.video_offset_secs,
               confidence = excluded.confidence,
               computed_at = excluded.computed_at",
            params![file_hash, video_offset_secs, confidence.map(|c| c as f64)],
        )?;
        Ok(())
    })
}
