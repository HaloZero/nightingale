//! SQLite-backed library store.
//!
//! Split into focused submodules so each file owns one responsibility:
//!  - [`connection`] — the singleton `Connection` and `with_conn`/`with_conn_mut` guards
//!  - [`migrations`] — schema migrations, legacy `songs.json` import, and the one-shot
//!    Jellyfin path rewrite
//!  - [`analysis_queue`] — CRUD for the analyzer's persistent queue
//!  - [`songs`] — core song row CRUD, scan-aware inserts, rekey/update helpers
//!  - [`karaoke_video_status`] — per-song cache of whether a karaoke video /
//!    YouTube-background karaoke video has been rendered
//!  - [`karaoke_video_runs`] — append-only history log of every karaoke-video
//!    render/fetch attempt and what state it ended in
//!  - [`queries`] — search / pagination / library-menu aggregation queries
//!  - [`rebase`] — one-shot path rewrite when the data root moves
//!  - [`remote`] — generic helpers for non-local song origins (Jellyfin, Navidrome, …),
//!    keyed on `$.origin.kind` instead of duplicating SQL per source
//!
//! `mod.rs` is a thin barrel: it owns the small top-of-stack items
//! (`init_library`, `library_db_path`, the scan generation counter) and
//! re-exports everything else so external call sites keep writing
//! `library_db::foo(...)`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::cache::nightingale_dir;

mod analysis_queue;
mod analysis_timings;
mod connection;
mod karaoke_video_runs;
mod karaoke_video_status;
mod migrations;
mod parallel_mismatches;
mod parallel_timings;
mod playlists;
mod queries;
mod rebase;
pub mod remote;
mod songs;
mod video_processing_queue;
mod youtube_video_lookups;

pub use analysis_queue::{
    analysis_queue_acknowledge_failures, analysis_queue_clear, analysis_queue_delete,
    analysis_queue_load_rows, analysis_queue_save_rows, analysis_queue_upsert_row,
};
pub use analysis_timings::{AnalysisTimingRow, insert_analysis_timing};
pub use karaoke_video_runs::{KaraokeVideoRunRow, insert_karaoke_video_run};
pub use karaoke_video_status::{
    get_karaoke_video_status, set_has_karaoke_video, set_has_youtube_karaoke_video,
};
pub use migrations::rewrite_legacy_jellyfin_paths;
pub use parallel_mismatches::{clear_parallel_analysis_mismatch, record_parallel_analysis_mismatch};
pub use parallel_timings::{ParallelAnalysisTimingRow, insert_parallel_analysis_timing};
pub use playlists::{PlaylistDefinition, PlaylistSongKeyKind, replace_all_playlists};
pub use video_processing_queue::{
    video_queue_clear, video_queue_clear_all, video_queue_load_rows,
    video_queue_mark_processing, video_queue_mark_queued_many,
};
pub use queries::{
    iter_file_hashes_filtered_full_reanalyzable, iter_file_hashes_filtered_karaoke_renderable,
    iter_file_hashes_filtered_not_analyzed, iter_file_hashes_filtered_queued,
    iter_file_hashes_filtered_realignable, iter_file_hashes_filtered_refreshable,
    load_all_local_songs, load_meta_sql, load_songs_page, query_library_menu_items,
};
pub use rebase::{rebase_song_album_art_cache_paths, rebase_song_album_art_paths};
pub use songs::{
    append_songs_for_scan, delete_songs_not_in_paths, load_all_songs, load_song_by_hash,
    load_song_by_path, load_song_path_strings, load_songs_by_hashes, read_library_meta,
    rekey_song, replace_all_songs_sorted, update_library_meta, update_song_fields,
};
pub use youtube_video_lookups::{get_youtube_video_lookup, record_youtube_video_lookup};

/// Incremented at the start of each `start_scan` so in-flight scan threads stop writing
/// after the library is cleared or replaced (folder change / new scan).
static SCAN_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn bump_scan_generation() -> u64 {
    SCAN_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn scan_generation_is_current(generation: u64) -> bool {
    SCAN_GENERATION.load(Ordering::SeqCst) == generation
}

pub fn library_db_path() -> PathBuf {
    nightingale_dir().join("songs.db")
}

pub fn init_library() -> rusqlite::Result<()> {
    if connection::is_initialised() {
        return Ok(());
    }
    let conn = connection::open_connection(&library_db_path())?;
    connection::install(conn)?;
    analysis_queue::import_legacy_analysis_queue_json()?;
    migrations::maybe_start_songs_json_migration();
    Ok(())
}

pub fn reconnect_library_at_root(root: &Path) -> Result<(), String> {
    let db_path = root.join("songs.db");
    let conn = connection::open_connection(&db_path)
        .map_err(|e| format!("failed opening migrated songs db: {e}"))?;
    connection::replace_or_install(conn)
}
