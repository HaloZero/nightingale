//! Song row CRUD.
//!
//! Everything that creates, updates, deletes, or fetches `songs` rows lives
//! here. The hot helpers `song_to_payload`, `INSERT_SONG_SQL`,
//! `insert_song_row_prepared`, and `load_song_from_payload_column` are
//! `pub(crate)` so sibling submodules (queries, migrations) reuse them
//! without copy-pasting the column lists.

use rusqlite::params;

use crate::song::{Song, TranscriptSource};

use super::connection::{with_conn, with_conn_mut};
use super::scan_generation_is_current;

pub(crate) fn song_to_payload(song: &Song) -> rusqlite::Result<String> {
    serde_json::to_string(song).map_err(|e| {
        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )))
    })
}

pub(crate) fn transcript_source_to_db(t: Option<TranscriptSource>) -> Option<String> {
    t.map(|s| match s {
        TranscriptSource::Lyrics => "lyrics".to_string(),
        TranscriptSource::Generated => "generated".to_string(),
        TranscriptSource::Usdx => "usdx".to_string(),
        TranscriptSource::Lrc => "lrc".to_string(),
    })
}

pub(crate) const INSERT_SONG_SQL: &str = "\
INSERT INTO songs (path, file_hash, title, artist, album, duration_secs, album_art_path,
    is_analyzed, language, transcript_source, is_video, has_lrc_file, has_embedded_lyrics, payload)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)";

pub(crate) fn insert_song_row_prepared(
    stmt: &mut rusqlite::Statement<'_>,
    song: &Song,
) -> rusqlite::Result<()> {
    let payload = song_to_payload(song)?;
    let album_art = song
        .album_art_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());
    stmt.execute(params![
        song.path.to_string_lossy(),
        song.file_hash,
        song.title,
        song.artist,
        song.album,
        song.duration_secs,
        album_art,
        song.is_analyzed as i32,
        song.language,
        transcript_source_to_db(song.transcript_source),
        song.is_video as i32,
        song.has_lrc_file as i32,
        song.has_embedded_lyrics as i32,
        payload,
    ])?;
    Ok(())
}

pub(crate) fn load_song_from_payload_column(r: &rusqlite::Row<'_>) -> rusqlite::Result<Song> {
    let payload: String = r.get(0)?;
    serde_json::from_str(&payload).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

pub fn read_library_meta() -> rusqlite::Result<(String, usize)> {
    with_conn(|c| {
        c.query_row(
            "SELECT folder, scan_count FROM library_meta WHERE id = 1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize)),
        )
    })
}

pub fn update_library_meta(folder: &str, scan_count: usize) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "UPDATE library_meta SET folder = ?1, scan_count = ?2 WHERE id = 1",
            params![folder, scan_count as i64],
        )?;
        Ok(())
    })
}

pub fn load_song_path_strings() -> rusqlite::Result<std::collections::HashSet<String>> {
    with_conn(|c| {
        let mut stmt = c.prepare("SELECT path FROM songs")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let v: Vec<String> = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(v.into_iter().collect())
    })
}

pub fn append_songs(songs: &[Song]) -> rusqlite::Result<()> {
    if songs.is_empty() {
        return Ok(());
    }
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        {
            let mut stmt = tx.prepare(INSERT_SONG_SQL)?;
            for song in songs {
                insert_song_row_prepared(&mut stmt, song)?;
            }
        }
        tx.commit()?;
        Ok(())
    })
}

pub fn append_songs_for_scan(songs: &[Song], generation: u64) -> rusqlite::Result<()> {
    if songs.is_empty() || !scan_generation_is_current(generation) {
        return Ok(());
    }
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        {
            let mut stmt = tx.prepare(INSERT_SONG_SQL)?;
            for song in songs {
                if !scan_generation_is_current(generation) {
                    return Ok(());
                }
                insert_song_row_prepared(&mut stmt, song)?;
            }
        }
        if !scan_generation_is_current(generation) {
            return Ok(());
        }
        tx.commit()?;
        Ok(())
    })
}

pub fn replace_all_songs_sorted(songs: &[Song]) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        tx.execute("DELETE FROM songs", [])?;
        {
            let mut stmt = tx.prepare(INSERT_SONG_SQL)?;
            for song in songs {
                insert_song_row_prepared(&mut stmt, song)?;
            }
        }
        tx.commit()?;
        Ok(())
    })
}

pub fn delete_songs_not_in_paths(paths: &[String]) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        if paths.is_empty() {
            c.execute("DELETE FROM songs", [])?;
            return Ok(());
        }
        let placeholders = (1..=paths.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("DELETE FROM songs WHERE path NOT IN ({placeholders})");
        c.execute(
            &sql,
            rusqlite::params_from_iter(paths.iter().map(|s| s.as_str())),
        )?;
        Ok(())
    })
}

pub fn load_song_by_hash(file_hash: &str) -> rusqlite::Result<Option<Song>> {
    use rusqlite::OptionalExtension;
    with_conn(|c| {
        let mut stmt = c.prepare("SELECT payload FROM songs WHERE file_hash = ?1 LIMIT 1")?;
        let song = stmt
            .query_row([file_hash], |r| {
                let payload: String = r.get(0)?;
                serde_json::from_str::<Song>(&payload).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
            })
            .optional()?;
        Ok(song)
    })
}

/// Rewrite a song row keyed by `old_hash` so its `file_hash`, `path`, and
/// JSON payload reflect a freshly downloaded source whose true Blake3 differs
/// from the placeholder we initially stored. Also points any pending row in
/// `analysis_queue` at the new hash so the in-flight scan keeps working.
pub fn rekey_song(old_hash: &str, new_hash: &str, new_song: &Song) -> rusqlite::Result<()> {
    let payload = song_to_payload(new_song)?;
    let album_art = new_song
        .album_art_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        tx.execute(
            "UPDATE songs SET file_hash = ?2, path = ?3, payload = ?4, album_art_path = ?5,
                title = ?6, artist = ?7, album = ?8, duration_secs = ?9,
                is_analyzed = ?10, language = ?11, transcript_source = ?12, is_video = ?13,
                has_lrc_file = ?14, has_embedded_lyrics = ?15
             WHERE file_hash = ?1",
            params![
                old_hash,
                new_hash,
                new_song.path.to_string_lossy(),
                payload,
                album_art,
                new_song.title,
                new_song.artist,
                new_song.album,
                new_song.duration_secs,
                new_song.is_analyzed as i32,
                new_song.language,
                transcript_source_to_db(new_song.transcript_source),
                new_song.is_video as i32,
                new_song.has_lrc_file as i32,
                new_song.has_embedded_lyrics as i32,
            ],
        )?;
        // `analysis_queue.file_hash` is the PK; UPDATE-OR-IGNORE shape covers
        // the (extremely unlikely) case where a row already exists for the
        // new hash.
        tx.execute(
            "DELETE FROM analysis_queue WHERE file_hash = ?1",
            params![new_hash],
        )?;
        tx.execute(
            "UPDATE analysis_queue SET file_hash = ?2 WHERE file_hash = ?1",
            params![old_hash, new_hash],
        )?;
        tx.commit()?;
        Ok(())
    })
}

pub fn update_song_fields(file_hash: &str, song: &Song) -> rusqlite::Result<()> {
    let payload = song_to_payload(song)?;
    let album_art = song
        .album_art_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());
    with_conn_mut(|c| {
        c.execute(
            "UPDATE songs SET title = ?2, artist = ?3, album = ?4, duration_secs = ?5,
                album_art_path = ?6, is_analyzed = ?7, language = ?8, transcript_source = ?9,
                is_video = ?10, has_lrc_file = ?11, has_embedded_lyrics = ?12, payload = ?13
             WHERE file_hash = ?1",
            params![
                file_hash,
                song.title,
                song.artist,
                song.album,
                song.duration_secs,
                album_art,
                song.is_analyzed as i32,
                song.language,
                transcript_source_to_db(song.transcript_source),
                song.is_video as i32,
                song.has_lrc_file as i32,
                song.has_embedded_lyrics as i32,
                payload,
            ],
        )?;
        Ok(())
    })
}

pub fn load_all_songs() -> rusqlite::Result<Vec<Song>> {
    with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT payload FROM songs ORDER BY artist COLLATE NOCASE, title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], load_song_from_payload_column)?;
        rows.collect()
    })
}
