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

const SCHEMA_VERSION: i32 = 4;

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
    // Deliberately decoupled from SCHEMA_VERSION below (see its doc comment):
    // must run before the version early-return, since every DB already at
    // SCHEMA_VERSION would otherwise never reach it.
    ensure_lyrics_columns(conn)?;
    ensure_analysis_timings_columns(conn)?;
    ensure_genre_column(conn)?;
    ensure_analysis_queue_columns(conn)?;
    ensure_parallel_mismatch_columns(conn)?;
    ensure_parallel_analysis_timings_table(conn)?;
    ensure_youtube_video_lookups_table(conn)?;
    ensure_youtube_video_sync_table(conn)?;
    ensure_karaoke_video_status_table(conn)?;
    ensure_karaoke_video_runs_table(conn)?;
    ensure_video_processing_queue_table(conn)?;

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
                genre TEXT NOT NULL DEFAULT 'Unknown Genre',
                duration_secs REAL NOT NULL,
                album_art_path TEXT,
                is_analyzed INTEGER NOT NULL,
                language TEXT,
                transcript_source TEXT,
                is_video INTEGER NOT NULL,
                has_lrc_file INTEGER NOT NULL DEFAULT 0,
                has_embedded_lyrics INTEGER NOT NULL DEFAULT 0,
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
                failed_message TEXT,
                failed_kind TEXT,
                failed_acknowledged INTEGER NOT NULL DEFAULT 0
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
                transcribe_ms INTEGER,
                align_ms INTEGER,
                load_avg_1m REAL,
                gpu_active_ratio REAL,
                gpu_freq_mhz INTEGER,
                gpu_temp_c REAL,
                cpu_active_ratio REAL,
                mem_pressure_ratio REAL,
                total_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_analysis_timings_file_hash
                ON analysis_timings(file_hash);
        ",
        )?;
    }
    if v < 4 {
        conn.execute_batch(
            "
            -- Songs `parallel_analysis` expected to match on a peer instance
            -- (same path, same content hash) but didn't. One row per local
            -- song currently mismatched -- cleared (see
            -- `clear_parallel_analysis_mismatch`) once a later check finds a
            -- match, so this only ever reflects the current state, not a
            -- historical log.
            CREATE TABLE IF NOT EXISTS parallel_analysis_mismatches (
                file_hash TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                peer_url TEXT NOT NULL,
                peer_hash TEXT,
                peer_path TEXT,
                detected_at TEXT NOT NULL
            );
        ",
        )?;
    }
    conn.execute(&format!("PRAGMA user_version = {SCHEMA_VERSION}"), [])?;
    Ok(())
}

/// Adds `has_lrc_file` / `has_embedded_lyrics` to `songs` if either is
/// missing, without claiming a `SCHEMA_VERSION` bump. Deliberately decoupled
/// from the version-gated migrations above: claiming a specific version
/// number risks colliding with an unrelated migration landing around the
/// same time, while a plain existence check is safe to run on every startup
/// regardless of what version number the DB is actually on. No-ops on a
/// brand-new DB where `songs` doesn't exist yet -- the `v == 0` branch above
/// creates it with both columns already present.
fn ensure_lyrics_columns(conn: &Connection) -> rusqlite::Result<()> {
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'songs'",
        [],
        |r| r.get::<_, i64>(0),
    )? > 0;
    if !table_exists {
        return Ok(());
    }

    let existing: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info('songs')")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?
    };

    if !existing.contains("has_lrc_file") {
        conn.execute(
            "ALTER TABLE songs ADD COLUMN has_lrc_file INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !existing.contains("has_embedded_lyrics") {
        conn.execute(
            "ALTER TABLE songs ADD COLUMN has_embedded_lyrics INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}

/// Adds the `genre` column for DBs created before genre browsing existed.
/// Same version-decoupled existence-check shape as [`ensure_lyrics_columns`]
/// -- pre-existing rows read as "Unknown Genre" until the next scan
/// repopulates them from file tags / source metadata.
fn ensure_genre_column(conn: &Connection) -> rusqlite::Result<()> {
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'songs'",
        [],
        |r| r.get::<_, i64>(0),
    )? > 0;
    if !table_exists {
        return Ok(());
    }

    let existing: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info('songs')")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?
    };

    if !existing.contains("genre") {
        conn.execute(
            "ALTER TABLE songs ADD COLUMN genre TEXT NOT NULL DEFAULT 'Unknown Genre'",
            [],
        )?;
    }
    Ok(())
}

/// Adds `failed_kind` (app-core's `FailureKind`) and `failed_acknowledged`
/// for grouping/dismissing failure toasts. Old rows read as `Other` /
/// unacknowledged.
fn ensure_analysis_queue_columns(conn: &Connection) -> rusqlite::Result<()> {
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'analysis_queue'",
        [],
        |r| r.get::<_, i64>(0),
    )? > 0;
    if !table_exists {
        return Ok(());
    }

    let existing: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info('analysis_queue')")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?
    };

    if !existing.contains("failed_kind") {
        conn.execute("ALTER TABLE analysis_queue ADD COLUMN failed_kind TEXT", [])?;
    }
    if !existing.contains("failed_acknowledged") {
        conn.execute(
            "ALTER TABLE analysis_queue ADD COLUMN failed_acknowledged INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}

/// Adds `peer_path` to `parallel_analysis_mismatches`: recorded when a
/// "peer has nothing at this relative path" miss turns out to be a moved
/// file -- the same content hash found on the peer at a different path --
/// rather than a genuinely absent song. `NULL` for the ordinary "peer truly
/// has nothing" and "different hash at the same path" cases. Same
/// version-decoupled existence-check shape as [`ensure_lyrics_columns`].
fn ensure_parallel_mismatch_columns(conn: &Connection) -> rusqlite::Result<()> {
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'parallel_analysis_mismatches'",
        [],
        |r| r.get::<_, i64>(0),
    )? > 0;
    if !table_exists {
        return Ok(());
    }

    let existing: std::collections::HashSet<String> = {
        let mut stmt =
            conn.prepare("SELECT name FROM pragma_table_info('parallel_analysis_mismatches')")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?
    };

    if !existing.contains("peer_path") {
        conn.execute(
            "ALTER TABLE parallel_analysis_mismatches ADD COLUMN peer_path TEXT",
            [],
        )?;
    }
    Ok(())
}

/// Coarse wall-clock timing for `parallel_analysis` dispatches, distinct
/// from `analysis_timings` -- that table only has per-stage data for runs
/// the *local* analyzer pipeline actually executed, so it has nothing to
/// say about a song offloaded to a peer. `CREATE TABLE IF NOT EXISTS` is
/// itself idempotent, so (like the `ensure_*` functions above) this just
/// runs unconditionally on every connection rather than needing a
/// `SCHEMA_VERSION` bump.
fn ensure_parallel_analysis_timings_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS parallel_analysis_timings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_hash TEXT NOT NULL,
            peer_url TEXT NOT NULL,
            started_at TEXT NOT NULL,
            already_analyzed_on_peer INTEGER NOT NULL,
            poll_attempts INTEGER,
            total_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_parallel_analysis_timings_file_hash
            ON parallel_analysis_timings(file_hash);
        CREATE INDEX IF NOT EXISTS idx_parallel_analysis_timings_started_at
            ON parallel_analysis_timings(started_at);
    ",
    )
}

/// Caches `audiodb::find_music_video_for_hash`'s TheAudioDB lookup result
/// per song, deliberately separate from `songs` (a lookup outcome isn't a
/// song property, it's an external-API-call cache) so a repeat lookup for
/// the same song reads this table instead of hitting TheAudioDB's
/// 30-requests/minute free tier again. One row per song, upserted on every
/// lookup; `youtube_url IS NULL` means "looked up, TheAudioDB had nothing"
/// as distinct from "never looked up" (no row at all) -- see
/// `youtube_video_lookups::get_youtube_video_lookup`.
fn ensure_youtube_video_lookups_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS youtube_video_lookups (
            file_hash TEXT PRIMARY KEY,
            youtube_url TEXT,
            track_name TEXT,
            artist_name TEXT,
            looked_up_at TEXT NOT NULL
        );
    ",
    )
}

/// Cache of `video_sync::detect_sync_offset_for_hash`'s result -- see
/// `youtube_video_sync`'s module doc for why this exists (detection is
/// expensive, this makes sure it runs at most once per song). `NULL`
/// `video_offset_secs` with a row present means "detection ran, no
/// confident match" -- still cached, distinct from no row at all ("never
/// ran").
fn ensure_youtube_video_sync_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS youtube_video_sync (
            file_hash TEXT PRIMARY KEY,
            video_offset_secs REAL,
            confidence REAL,
            computed_at TEXT NOT NULL
        );
    ",
    )
}

/// Tracks whether a karaoke video (reel background) and/or a
/// YouTube-background karaoke video has been rendered for a song, one row
/// per song, upserted whenever `karaoke_video::ensure_karaoke_video` /
/// `ensure_youtube_background_karaoke_video` succeeds -- see
/// `karaoke_video_status::set_has_karaoke_video`/
/// `set_has_youtube_karaoke_video`. Separate from `songs` for the same
/// reason as `youtube_video_lookups` above: this is a cache of a rendered
/// artifact's presence on disk, not an intrinsic song property. No row
/// (or a `0` column) means "not rendered" -- there's no need to
/// distinguish "never attempted" from "attempted and failed" here the way
/// `youtube_video_lookups` does, since a failed render never deletes a
/// pre-existing successful one (see `render_karaoke_video_to`).
fn ensure_karaoke_video_status_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS karaoke_video_status (
            file_hash TEXT PRIMARY KEY,
            has_karaoke_video INTEGER NOT NULL DEFAULT 0,
            has_youtube_karaoke_video INTEGER NOT NULL DEFAULT 0
        );
    ",
    )
}

/// History log for the two karaoke-video pipelines (reel background,
/// YouTube background): one row per `karaoke_video::ensure_karaoke_video` /
/// `ensure_youtube_karaoke_video` invocation, recording which stage it
/// reached and how long each took -- same spirit as `analysis_timings`,
/// but (unlike that table) a row is written for *every* outcome, including
/// a freshness no-op or a failure, not just a completed render. Distinct
/// from `karaoke_video_status`: that table is "does this song have one
/// right now" (upserted, one row per song); this one is "everything that's
/// ever happened" (append-only, many rows per song over time).
fn ensure_karaoke_video_runs_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS karaoke_video_runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_hash TEXT NOT NULL,
            kind TEXT NOT NULL,
            started_at TEXT NOT NULL,
            status TEXT NOT NULL,
            error TEXT,
            lookup_ms INTEGER,
            download_ms INTEGER,
            render_ms INTEGER,
            total_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_karaoke_video_runs_file_hash
            ON karaoke_video_runs(file_hash);
        CREATE INDEX IF NOT EXISTS idx_karaoke_video_runs_started_at
            ON karaoke_video_runs(started_at);
    ",
    )
}

/// Live in-progress state for the same two pipelines -- see
/// `video_processing_queue`'s module doc for how this differs from the two
/// tables above (both are cache/history, not "in flight right now").
fn ensure_video_processing_queue_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS video_processing_queue (
            file_hash TEXT NOT NULL,
            kind TEXT NOT NULL,
            stage TEXT NOT NULL,
            started_at TEXT NOT NULL,
            PRIMARY KEY (file_hash, kind)
        );
        CREATE INDEX IF NOT EXISTS idx_video_processing_queue_kind
            ON video_processing_queue(kind);
    ",
    )
}

/// Splits the old combined `transcribe_or_align_ms` timing into separate
/// `transcribe_ms` / `align_ms` columns, so a row's stage timings alone show
/// whether that run transcribed from scratch or aligned to known lyrics
/// (exactly one of the two is populated; both are null for stems-only runs
/// that skip transcription entirely). The old column is left in place --
/// rather than migrated into the new ones, since a past combined duration
/// can't be split after the fact -- so pre-migration rows keep their
/// original value there and read as null in both new columns. Also adds
/// `load_avg_1m` (a cheap same-machine signal, no `sudo` required unlike
/// thermal pressure, for whether other processes were competing for
/// CPU/GPU when a run started) and `gpu_active_ratio` / `gpu_freq_mhz` /
/// `gpu_temp_c` / `cpu_active_ratio` / `mem_pressure_ratio` (all sampled via
/// the sudo-free `macmon` CLI if installed, to confirm separation actually
/// lands on the GPU, isn't thermal-throttled, and isn't contending with
/// other processes for CPU or memory).
fn ensure_analysis_timings_columns(conn: &Connection) -> rusqlite::Result<()> {
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'analysis_timings'",
        [],
        |r| r.get::<_, i64>(0),
    )? > 0;
    if !table_exists {
        return Ok(());
    }

    let existing: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info('analysis_timings')")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?
    };

    if !existing.contains("transcribe_ms") {
        conn.execute(
            "ALTER TABLE analysis_timings ADD COLUMN transcribe_ms INTEGER",
            [],
        )?;
    }
    if !existing.contains("align_ms") {
        conn.execute(
            "ALTER TABLE analysis_timings ADD COLUMN align_ms INTEGER",
            [],
        )?;
    }
    if !existing.contains("load_avg_1m") {
        conn.execute(
            "ALTER TABLE analysis_timings ADD COLUMN load_avg_1m REAL",
            [],
        )?;
    }
    if !existing.contains("gpu_active_ratio") {
        conn.execute(
            "ALTER TABLE analysis_timings ADD COLUMN gpu_active_ratio REAL",
            [],
        )?;
    }
    if !existing.contains("gpu_freq_mhz") {
        conn.execute(
            "ALTER TABLE analysis_timings ADD COLUMN gpu_freq_mhz INTEGER",
            [],
        )?;
    }
    if !existing.contains("gpu_temp_c") {
        conn.execute(
            "ALTER TABLE analysis_timings ADD COLUMN gpu_temp_c REAL",
            [],
        )?;
    }
    if !existing.contains("cpu_active_ratio") {
        conn.execute(
            "ALTER TABLE analysis_timings ADD COLUMN cpu_active_ratio REAL",
            [],
        )?;
    }
    if !existing.contains("mem_pressure_ratio") {
        conn.execute(
            "ALTER TABLE analysis_timings ADD COLUMN mem_pressure_ratio REAL",
            [],
        )?;
    }
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
