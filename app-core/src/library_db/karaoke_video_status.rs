//! Cache of which songs have a rendered karaoke video (reel background)
//! and/or a YouTube-background karaoke video on disk, one row per song --
//! see `migrations::ensure_karaoke_video_status_table` for why this is a
//! separate table rather than columns on `songs` (same reasoning as
//! `youtube_video_lookups`: a rendered artifact's presence isn't a song
//! property, it's a cache of what `karaoke_video::ensure_karaoke_video` /
//! `ensure_youtube_background_karaoke_video` have produced on disk). This
//! table is the source of truth; `karaoke_video::mirror_status_onto_song`
//! copies it onto `Song.has_karaoke_video`/`has_youtube_karaoke_video` after
//! every write so the song list can show it per row without an extra query.

use rusqlite::params;

use super::connection::{with_conn, with_conn_mut};

pub fn set_has_karaoke_video(file_hash: &str, has_karaoke_video: bool) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO karaoke_video_status (file_hash, has_karaoke_video, has_youtube_karaoke_video)
             VALUES (?1, ?2, 0)
             ON CONFLICT(file_hash) DO UPDATE SET has_karaoke_video = excluded.has_karaoke_video",
            params![file_hash, has_karaoke_video as i32],
        )?;
        Ok(())
    })
}

pub fn set_has_youtube_karaoke_video(
    file_hash: &str,
    has_youtube_karaoke_video: bool,
) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO karaoke_video_status (file_hash, has_karaoke_video, has_youtube_karaoke_video)
             VALUES (?1, 0, ?2)
             ON CONFLICT(file_hash) DO UPDATE SET has_youtube_karaoke_video = excluded.has_youtube_karaoke_video",
            params![file_hash, has_youtube_karaoke_video as i32],
        )?;
        Ok(())
    })
}

/// `(has_karaoke_video, has_youtube_karaoke_video)`, both `false` if no row
/// exists yet (never rendered).
pub fn get_karaoke_video_status(file_hash: &str) -> rusqlite::Result<(bool, bool)> {
    with_conn(|c| {
        c.query_row(
            "SELECT has_karaoke_video, has_youtube_karaoke_video FROM karaoke_video_status WHERE file_hash = ?1",
            [file_hash],
            |r| Ok((r.get::<_, i64>(0)? != 0, r.get::<_, i64>(1)? != 0)),
        )
    })
    .or_else(|e| {
        if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
            Ok((false, false))
        } else {
            Err(e)
        }
    })
}
