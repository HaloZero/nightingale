//! Schema migrations + one-shot data migrations.
//!
//! Three flavours of work live here:
//!  - `configure` / `run_migrations` — PRAGMAs and the initial schema (run on
//!    every connection open, idempotent via `PRAGMA user_version`).
//!  - `maybe_start_songs_json_migration` — pre-SQL builds wrote a flat
//!    `songs.json`; promote it into the `songs` table on a background thread
//!    and surface progress through `is_song_migration_in_progress` / the
//!    `song_migration_*_count` accessors used by `queries::load_meta_sql`.
//!  - `rewrite_legacy_jellyfin_paths` — pre-2026-05 Jellyfin rows stored a
//!    pseudo URL in `path`; rewrite to the cache-file path the rest of the
//!    code expects. Kept here because it's an upgrade migration, not an
//!    ongoing helper.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rusqlite::{Connection, params};

use crate::cache::songs_path;
use crate::library_model::SongsStore;
use crate::song::{Song, SongOrigin};

use super::connection::{with_conn, with_conn_mut};
use super::songs::{append_songs, update_library_meta};

const SCHEMA_VERSION: i32 = 3;

static MIGRATING: AtomicBool = AtomicBool::new(false);
static MIGRATION_TOTAL: AtomicUsize = AtomicUsize::new(0);
static MIGRATION_DONE: AtomicUsize = AtomicUsize::new(0);

pub(super) fn is_song_migration_in_progress() -> bool {
    MIGRATING.load(Ordering::Acquire)
}

pub(super) fn song_migration_total() -> usize {
    MIGRATION_TOTAL.load(Ordering::Acquire)
}

pub(super) fn song_migration_done() -> usize {
    MIGRATION_DONE.load(Ordering::Acquire)
}

pub(super) fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA cache_size = -64000;
        PRAGMA mmap_size = 268435456;
    ",
    )
}

pub(super) fn run_migrations(conn: &Connection) -> rusqlite::Result<()> {
    let v: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if v >= SCHEMA_VERSION {
        return Ok(());
    }
    if v == 0 {
        conn.execute_batch(
            "
            CREATE TABLE library_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                folder TEXT NOT NULL DEFAULT '',
                scan_count INTEGER NOT NULL DEFAULT 0
            );
            INSERT INTO library_meta (id, folder, scan_count) VALUES (1, '', 0);

            CREATE TABLE songs (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                file_hash TEXT NOT NULL,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                album TEXT NOT NULL,
                duration_secs REAL NOT NULL,
                album_art_path TEXT,
                is_analyzed INTEGER NOT NULL,
                language TEXT,
                transcript_source TEXT,
                is_video INTEGER NOT NULL,
                payload TEXT NOT NULL
            );
            CREATE INDEX idx_songs_file_hash ON songs(file_hash);
            CREATE INDEX idx_songs_artist_title ON songs(artist COLLATE NOCASE, title COLLATE NOCASE);
            CREATE INDEX idx_songs_album ON songs(album COLLATE NOCASE);

            CREATE VIRTUAL TABLE songs_fts USING fts5(
                title,
                artist,
                album,
                content = 'songs',
                content_rowid = 'id'
            );

            CREATE TABLE analysis_queue (
                file_hash TEXT PRIMARY KEY,
                status TEXT NOT NULL CHECK (status IN ('queued', 'analyzing', 'failed')),
                analyzing_pct INTEGER,
                failed_message TEXT
            );
        ",
        )?;
    }
    if v < 2 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS playlists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS playlist_songs (
                playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                PRIMARY KEY (playlist_id, song_id)
            );
            CREATE INDEX IF NOT EXISTS idx_playlist_songs_order
                ON playlist_songs(playlist_id, position);
            CREATE INDEX IF NOT EXISTS idx_playlist_songs_song
                ON playlist_songs(song_id);
        ",
        )?;
    }
    if v < 3 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS analysis_timings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_hash TEXT NOT NULL,
                started_at TEXT NOT NULL,
                device TEXT,
                whisper_model TEXT NOT NULL,
                beam_size INTEGER NOT NULL,
                batch_size INTEGER NOT NULL,
                separator TEXT NOT NULL,
                asr_engine TEXT NOT NULL,
                align_backend TEXT NOT NULL,
                vocal_detection_threshold_pct REAL NOT NULL,
                key_detect_ms INTEGER,
                separation_ms INTEGER,
                transcribe_or_align_ms INTEGER,
                total_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_analysis_timings_file_hash
                ON analysis_timings(file_hash);
        ",
        )?;
    }
    conn.execute(&format!("PRAGMA user_version = {SCHEMA_VERSION}"), [])?;
    Ok(())
}

pub(super) fn maybe_start_songs_json_migration() {
    let json_path = songs_path();
    if !json_path.is_file() {
        return;
    }
    let count: i64 =
        match with_conn(|c| c.query_row("SELECT COUNT(*) FROM songs", [], |r| r.get(0))) {
            Ok(n) => n,
            Err(_) => return,
        };
    if count > 0 {
        return;
    }

    let Ok(data) = std::fs::read_to_string(&json_path) else {
        return;
    };
    let Ok(store) = serde_json::from_str::<SongsStore>(&data) else {
        return;
    };
    let total = store.processed.len();
    if total == 0 {
        let _ = update_library_meta(&store.folder, store.count);
        let _ = std::fs::rename(&json_path, json_path.with_extension("json.bak"));
        return;
    }

    MIGRATING.store(true, Ordering::Release);
    MIGRATION_TOTAL.store(total, Ordering::Release);
    MIGRATION_DONE.store(0, Ordering::Release);

    let folder = store.folder.clone();
    let scan_count = store.count;
    let processed = store.processed;

    std::thread::spawn(move || {
        const BATCH: usize = 50;
        let _ = update_library_meta(&folder, scan_count);
        let success = migrate_song_batches(&processed, BATCH, |chunk| append_songs(chunk));
        MIGRATING.store(false, Ordering::Release);
        if success {
            let _ = std::fs::rename(&json_path, json_path.with_extension("json.bak"));
        }
    });
}

fn migrate_song_batches<F>(processed: &[Song], batch: usize, mut append_fn: F) -> bool
where
    F: FnMut(&[Song]) -> rusqlite::Result<()>,
{
    for chunk in processed.chunks(batch) {
        if append_fn(chunk).is_err() {
            return false;
        }
        MIGRATION_DONE.fetch_add(chunk.len(), Ordering::AcqRel);
    }
    true
}

/// One-shot startup migration: pre-2026-05 builds stored an unmaterialised
/// Jellyfin row's `path` as `jellyfin://item/<id>`. The new code expects every
/// row to carry the future cache-file path so the `path.is_file()` check in
/// `ensure_local_media` works naturally. Rewrites legacy rows in-place.
pub fn rewrite_legacy_jellyfin_paths(cache_dir: &Path) -> rusqlite::Result<()> {
    let candidates = with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT file_hash, payload FROM songs
             WHERE json_extract(payload, '$.origin.kind') = 'jellyfin'
               AND path LIKE 'jellyfin://%'",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>()
    })?;

    if candidates.is_empty() {
        return Ok(());
    }

    let sources_dir = cache_dir.join("sources");

    with_conn_mut(|c| {
        let tx = c.transaction()?;
        {
            let mut stmt =
                tx.prepare("UPDATE songs SET path = ?2, payload = ?3 WHERE file_hash = ?1")?;
            for (file_hash, payload) in candidates {
                let Ok(mut song) = serde_json::from_str::<Song>(&payload) else {
                    continue;
                };
                let container = match &song.origin {
                    SongOrigin::Jellyfin { container, .. } => container.clone(),
                    _ => None,
                };
                let ext = container.as_deref().unwrap_or("bin");
                let new_path = sources_dir.join(format!("{file_hash}.{ext}"));
                song.path = new_path.clone();
                let Ok(new_payload) = serde_json::to_string(&song) else {
                    continue;
                };
                stmt.execute(params![file_hash, new_path.to_string_lossy(), new_payload])?;
            }
        }
        tx.commit()
    })
}
