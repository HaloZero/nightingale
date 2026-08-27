//! Cache of `audiodb::find_music_video_for_hash`'s TheAudioDB lookup result,
//! one row per song -- see `migrations::ensure_youtube_video_lookups_table`
//! for why this is a separate table rather than columns on `songs`.

use rusqlite::{OptionalExtension, params};

use super::connection::{with_conn, with_conn_mut};

/// A cached lookup outcome. `youtube_url: None` means TheAudioDB was
/// checked and had nothing for this song -- distinct from no row existing
/// at all, which means it's never been looked up.
pub struct YoutubeVideoLookupRow {
    pub youtube_url: Option<String>,
    pub track_name: Option<String>,
    pub artist_name: Option<String>,
}

/// `Ok(None)` means never looked up -- caller should call TheAudioDB and
/// `record_youtube_video_lookup` the result. `Ok(Some(row))` means it was,
/// whether or not a video was actually found (`row.youtube_url`).
pub fn get_youtube_video_lookup(file_hash: &str) -> rusqlite::Result<Option<YoutubeVideoLookupRow>> {
    with_conn(|c| {
        c.query_row(
            "SELECT youtube_url, track_name, artist_name FROM youtube_video_lookups WHERE file_hash = ?1",
            [file_hash],
            |r| {
                Ok(YoutubeVideoLookupRow {
                    youtube_url: r.get(0)?,
                    track_name: r.get(1)?,
                    artist_name: r.get(2)?,
                })
            },
        )
        .optional()
    })
}

pub fn record_youtube_video_lookup(
    file_hash: &str,
    youtube_url: Option<&str>,
    track_name: Option<&str>,
    artist_name: Option<&str>,
) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO youtube_video_lookups (file_hash, youtube_url, track_name, artist_name, looked_up_at)
             VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(file_hash) DO UPDATE SET
               youtube_url = excluded.youtube_url,
               track_name = excluded.track_name,
               artist_name = excluded.artist_name,
               looked_up_at = excluded.looked_up_at",
            params![file_hash, youtube_url, track_name, artist_name],
        )?;
        Ok(())
    })
}
