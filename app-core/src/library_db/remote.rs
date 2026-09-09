//! Helpers for songs that live on a remote upstream (Jellyfin, Navidrome, …)
//! and are keyed by `origin.item_id` rather than by `path`.
//!
//! Everything here is generic over the `origin.kind` discriminator string,
//! so adding a new remote source costs nothing in this module — the source
//! just calls these four functions with its own kind tag (and the
//! `SongOrigin::cover_tag_mut` helper makes the cover-refresh path work
//! without any per-source `match`).
//!
//! Tracking remote songs by `item_id` is deliberate: after the analyzer
//! materialises and rekeys a row, the `file_hash` flips to the true Blake3
//! and the `path` points at the cache file — only `item_id` survives a full
//! round-trip from "freshly scanned" to "fully analysed" to "rescan-saw-this-again".

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::OptionalExtension;

use crate::song::Song;

use super::connection::{with_conn, with_conn_mut};
use super::songs::update_song_fields;

/// Item IDs of every row whose `origin.kind` matches `kind`. Used at the top
/// of each remote source's scan loop to figure out which upstream items we
/// already have rows for (so we only build/insert new ones and skip
/// re-fetching covers for known items).
pub(crate) fn load_remote_item_ids(kind: &str) -> rusqlite::Result<HashSet<String>> {
    with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT json_extract(payload, '$.origin.item_id')
             FROM songs
             WHERE json_extract(payload, '$.origin.kind') = ?1",
        )?;
        let rows = stmt.query_map([kind], |r| r.get::<_, Option<String>>(0))?;
        let ids: Vec<Option<String>> = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(ids.into_iter().flatten().collect())
    })
}

/// `item_id -> cover_tag` for every row of the given `kind`. A `None` value
/// means the row exists but we never recorded a cover tag for it.
pub(crate) fn load_remote_cover_tags(
    kind: &str,
) -> rusqlite::Result<HashMap<String, Option<String>>> {
    with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT json_extract(payload, '$.origin.item_id'),
                    json_extract(payload, '$.origin.cover_tag')
             FROM songs
             WHERE json_extract(payload, '$.origin.kind') = ?1",
        )?;
        let rows = stmt.query_map([kind], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
            ))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (id, tag) = row?;
            if let Some(id) = id {
                out.insert(id, tag);
            }
        }
        Ok(out)
    })
}

/// Run `fetch` to obtain a fresh cover path for the given remote item, then
/// update the row's `album_art_path` + payload accordingly. When `fetch`
/// returns `None` we also clear the stored `cover_tag` so the next scan
/// will retry (instead of skipping the item because the tag still matches).
pub(crate) fn refresh_remote_cover_for_item<F>(
    kind: &str,
    item_id: &str,
    fetch: F,
) -> rusqlite::Result<()>
where
    F: FnOnce(&str) -> Option<PathBuf>,
{
    let song_row = with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT file_hash, payload FROM songs
             WHERE json_extract(payload, '$.origin.kind') = ?1
               AND json_extract(payload, '$.origin.item_id') = ?2
             LIMIT 1",
        )?;
        stmt.query_row([kind, item_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .optional()
    })?;
    let Some((file_hash, payload)) = song_row else {
        return Ok(());
    };
    let mut song: Song = match serde_json::from_str(&payload) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let new_cover = fetch(item_id);
    if new_cover.is_none()
        && let Some(slot) = song.origin.cover_tag_mut()
    {
        *slot = None;
    }
    song.album_art_path = new_cover;
    update_song_fields(&file_hash, &song)
}

/// Prune rows of the given `kind` whose item id is no longer present
/// upstream. Other origins (folder, other remote kinds) are untouched.
pub(crate) fn delete_remote_songs_not_in_item_ids(
    kind: &str,
    item_ids: &[String],
) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        if item_ids.is_empty() {
            c.execute(
                "DELETE FROM songs WHERE json_extract(payload, '$.origin.kind') = ?1",
                [kind],
            )?;
            return Ok(());
        }
        let placeholders = (1..=item_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        // The leading bind is `kind`, the rest are the item ids; chain them
        // together for `params_from_iter`.
        let sql = format!(
            "DELETE FROM songs
             WHERE json_extract(payload, '$.origin.kind') = ?
               AND json_extract(payload, '$.origin.item_id') NOT IN ({placeholders})"
        );
        let binds = std::iter::once(kind).chain(item_ids.iter().map(|s| s.as_str()));
        c.execute(&sql, rusqlite::params_from_iter(binds))?;
        Ok(())
    })
}
