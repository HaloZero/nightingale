//! Read-only playlist storage shared by local and remote media sources.

use std::collections::HashSet;

use rusqlite::params;

use super::connection::with_conn_mut;

#[derive(Debug, Clone)]
pub(crate) struct PlaylistDefinition {
    pub id: String,
    pub name: String,
    /// Local song paths or remote media item ids, depending on `key_kind`.
    pub song_keys: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PlaylistSongKeyKind<'a> {
    LocalPath,
    RemoteItemId { origin_kind: &'a str },
}

/// Atomically replace playlist navigation data for the active library source.
/// Entries not present in the scanned song catalogue are ignored.
pub(crate) fn replace_all_playlists(
    playlists: &[PlaylistDefinition],
    key_kind: PlaylistSongKeyKind<'_>,
) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        tx.execute("DELETE FROM playlists", [])?;

        {
            let mut insert_playlist =
                tx.prepare("INSERT INTO playlists (id, name) VALUES (?1, ?2)")?;
            let mut insert_entry = tx.prepare(
                "INSERT INTO playlist_songs (playlist_id, song_id, position)
                 VALUES (?1, ?2, ?3)",
            )?;
            let mut find_local = tx.prepare("SELECT id FROM songs WHERE path = ?1 LIMIT 1")?;
            let mut find_remote = tx.prepare(
                "SELECT id FROM songs
                 WHERE json_extract(payload, '$.origin.kind') = ?1
                   AND json_extract(payload, '$.origin.item_id') = ?2
                 LIMIT 1",
            )?;

            for playlist in playlists {
                if playlist.id.is_empty() || playlist.name.trim().is_empty() {
                    continue;
                }
                insert_playlist.execute(params![playlist.id, playlist.name.trim()])?;

                // Duplicate entries break React song identity and add little value in
                // navigation. Keep first occurrence and its upstream ordering.
                let mut seen_song_ids = HashSet::new();
                for (position, key) in playlist.song_keys.iter().enumerate() {
                    let song_id = match key_kind {
                        PlaylistSongKeyKind::LocalPath => {
                            find_local.query_row([key], |r| r.get::<_, i64>(0)).ok()
                        }
                        PlaylistSongKeyKind::RemoteItemId { origin_kind } => find_remote
                            .query_row(params![origin_kind, key], |r| r.get::<_, i64>(0))
                            .ok(),
                    };
                    let Some(song_id) = song_id else {
                        continue;
                    };
                    if !seen_song_ids.insert(song_id) {
                        continue;
                    }
                    insert_entry.execute(params![playlist.id, song_id, position as i64])?;
                }
            }
        }

        tx.commit()
    })
}
