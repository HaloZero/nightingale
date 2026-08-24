//! Source-agnostic scan dispatcher. File walking and remote-provider sync
//! live in their respective `source` adapters. This module owns:
//!  - resolving the configured `LibrarySource` and instantiating the right adapter
//!  - bumping the cancellation generation
//!  - spawning the scan thread
//!  - exposing `SongsStore` load/load_meta entry points used by the bridge

use tracing::warn;

use crate::{
    analyzer,
    cache::CacheDir,
    config::AppConfig,
    library_db,
    library_model::{LibraryMenuFilters, LoadSongsParams, SongTarget, SongsMeta, SongsStore},
    source::{ScanContext, active_source_from_config},
};

impl SongsStore {
    pub fn load_all() -> Self {
        let processed = library_db::load_all_songs().unwrap_or_default();
        let (folder, count) = library_db::read_library_meta().unwrap_or((String::new(), 0));
        let processed_count = processed.len();
        let analyzed_count = processed.iter().filter(|song| song.is_analyzed).count();
        SongsStore {
            count,
            folder,
            processed,
            processed_count,
            analyzed_count,
        }
    }

    pub fn load(params: &LoadSongsParams) -> Self {
        library_db::load_songs_page(params).unwrap_or_else(|_| SongsStore {
            count: 0,
            folder: String::new(),
            processed: Vec::new(),
            processed_count: 0,
            analyzed_count: 0,
        })
    }

    pub fn load_by_hashes(file_hashes: &[String]) -> Vec<crate::song::Song> {
        library_db::load_songs_by_hashes(file_hashes).unwrap_or_default()
    }

    pub fn load_meta() -> SongsMeta {
        library_db::load_meta_sql().unwrap_or_default()
    }
}

/// Trigger a scan using whatever `LibrarySource` is currently configured.
/// Returns immediately; the scan runs on a background thread.
pub fn start_scan() {
    let scan_generation = library_db::bump_scan_generation();

    let source = match active_source_from_config(&AppConfig::load()) {
        Ok(Some(s)) => s,
        Ok(None) => {
            warn!("[scanner] No library source configured; ignoring scan request");
            return;
        }
        Err(e) => {
            warn!("[scanner] Failed to instantiate library source: {e}");
            return;
        }
    };

    // If the active source's identity changed (folder → Jellyfin, different
    // Jellyfin server, different folder path) the rows already in the DB
    // belong to a different library and would otherwise stick around — each
    // source's per-scan pruning only touches its own rows. Wipe everything
    // up front so the upcoming scan starts from a clean slate.
    let new_label = source.label();
    let (existing_label, _) = library_db::read_library_meta().unwrap_or_default();
    if existing_label != new_label {
        let _ = library_db::replace_all_songs_sorted(&[]);
        let _ = library_db::analysis_queue_clear();
        let _ = library_db::update_library_meta(&new_label, 0);
    }

    std::thread::spawn(move || {
        let cache = CacheDir::new();
        let ctx = ScanContext {
            generation: scan_generation,
            cache: &cache,
        };
        if let Err(e) = source.scan(&ctx) {
            warn!("[scanner] Scan failed: {e}");
            return;
        }

        if library_db::scan_generation_is_current(scan_generation)
            && AppConfig::load().auto_analyze()
        {
            let _ = analyzer::enqueue(SongTarget::Filter {
                filters: LibraryMenuFilters::default(),
            });
        }
    });
}
