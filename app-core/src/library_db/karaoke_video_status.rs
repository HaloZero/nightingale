//! Cache of which songs have a rendered karaoke video (reel background)
//! and/or a YouTube-background karaoke video on disk, and which
//! `karaoke_video::RENDER_VERSION` produced it -- one row per song -- see
//! `migrations::ensure_karaoke_video_status_table`/
//! `ensure_karaoke_video_version_columns` for why this is a separate table
//! rather than columns on `songs` (same reasoning as
//! `youtube_video_lookups`: a rendered artifact's presence isn't a song
//! property, it's a cache of what `karaoke_video::ensure_karaoke_video` /
//! `ensure_youtube_background_karaoke_video` have produced on disk). This
//! table is the source of truth; `karaoke_video::record_karaoke_video_status`
//! copies it onto `Song.karaoke_video_version`/`youtube_karaoke_video_version`
//! after every write so the song list can show it per row without an extra
//! query.
//!
//! `0` means "no video of that flavor rendered"; any other value is the
//! `RENDER_VERSION` that produced the currently-cached one -- there's no
//! history of past versions, just whatever's on disk right now (a
//! re-render overwrites in place).

use rusqlite::params;

use super::connection::{with_conn, with_conn_mut};

pub fn set_karaoke_video_version(file_hash: &str, version: u32) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO karaoke_video_status (file_hash, karaoke_video_version, youtube_karaoke_video_version)
             VALUES (?1, ?2, 0)
             ON CONFLICT(file_hash) DO UPDATE SET karaoke_video_version = excluded.karaoke_video_version",
            params![file_hash, version],
        )?;
        Ok(())
    })
}

pub fn set_youtube_karaoke_video_version(file_hash: &str, version: u32) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO karaoke_video_status (file_hash, karaoke_video_version, youtube_karaoke_video_version)
             VALUES (?1, 0, ?2)
             ON CONFLICT(file_hash) DO UPDATE SET youtube_karaoke_video_version = excluded.youtube_karaoke_video_version",
            params![file_hash, version],
        )?;
        Ok(())
    })
}

/// `(karaoke_video_version, youtube_karaoke_video_version)`, both `0` if no
/// row exists yet (never rendered).
pub fn get_karaoke_video_versions(file_hash: &str) -> rusqlite::Result<(u32, u32)> {
    with_conn(|c| {
        c.query_row(
            "SELECT karaoke_video_version, youtube_karaoke_video_version FROM karaoke_video_status WHERE file_hash = ?1",
            [file_hash],
            |r| Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)? as u32)),
        )
    })
    .or_else(|e| {
        if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
            Ok((0, 0))
        } else {
            Err(e)
        }
    })
}
