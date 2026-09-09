//! One-shot rewrite of `album_art_path` columns when the data root moves.
//!
//! Used by `cache::change_app_data_path` after the user picks a new data dir
//! and we have copied the on-disk files. Opens the *destination* DB directly
//! (the global `LIBRARY_DB` connection still points at the source root at
//! this moment) so we can rewrite paths before the app reconnects.

use std::path::Path;

use rusqlite::{Connection, params};

fn maybe_rebase_string_path(path: &str, old_root: &Path, new_root: &Path) -> Option<String> {
    let rel = Path::new(path).strip_prefix(old_root).ok()?;
    Some(new_root.join(rel).to_string_lossy().into_owned())
}

fn rebase_song_album_art_paths_in_db(
    db_path: &Path,
    old_root: &Path,
    new_root: &Path,
) -> Result<(), String> {
    if !db_path.is_file() || crate::cache::same_path(old_root, new_root) {
        return Ok(());
    }

    let conn = Connection::open(db_path)
        .map_err(|e| format!("failed opening songs db {:?}: {e}", db_path))?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("failed opening songs db transaction: {e}"))?;
    let mut stmt = tx
        .prepare("SELECT id, album_art_path, payload FROM songs")
        .map_err(|e| format!("failed preparing songs query: {e}"))?;

    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("failed querying songs: {e}"))?;

    let mut updates: Vec<(i64, Option<String>, String)> = Vec::new();
    for row in rows {
        let (id, album_art_path, payload) =
            row.map_err(|e| format!("failed reading songs row: {e}"))?;

        let mut changed = false;
        let mut new_album_art = album_art_path.clone();
        if let Some(current) = album_art_path.as_deref()
            && let Some(rebased) = maybe_rebase_string_path(current, old_root, new_root)
        {
            new_album_art = Some(rebased);
            changed = true;
        }

        let mut new_payload = payload.clone();
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&payload)
            && let Some(album_art_value) = value.get_mut("album_art_path")
            && let Some(current) = album_art_value.as_str()
            && let Some(rebased) = maybe_rebase_string_path(current, old_root, new_root)
        {
            *album_art_value = serde_json::Value::String(rebased);
            if let Ok(serialized) = serde_json::to_string(&value) {
                new_payload = serialized;
                changed = true;
            }
        }

        if changed {
            updates.push((id, new_album_art, new_payload));
        }
    }
    drop(stmt);

    for (id, album_art_path, payload) in updates {
        tx.execute(
            "UPDATE songs SET album_art_path = ?2, payload = ?3 WHERE id = ?1",
            params![id, album_art_path, payload],
        )
        .map_err(|e| format!("failed updating songs row {id}: {e}"))?;
    }

    tx.commit()
        .map_err(|e| format!("failed committing songs path rewrite: {e}"))?;
    Ok(())
}

pub(crate) fn rebase_song_album_art_paths(old_root: &Path, new_root: &Path) -> Result<(), String> {
    rebase_song_album_art_paths_in_db(&new_root.join("songs.db"), old_root, new_root)
}

pub(crate) fn rebase_song_album_art_cache_paths(
    old_cache: &Path,
    new_cache: &Path,
) -> Result<(), String> {
    rebase_song_album_art_paths_in_db(&super::library_db_path(), old_cache, new_cache)
}
