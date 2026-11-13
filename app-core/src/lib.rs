mod analyzer;
mod backgrounds;
mod cache;
mod chromecast;
mod config;
mod error;
mod library_db;
mod library_menu;
mod library_model;
mod lrc;
mod lyrics;
pub mod media_server;
pub mod nge_format;
mod playback;
mod playback_queue;
mod playback_session;
mod profile;
mod remote_control;
mod scanner;
mod search;
mod secret;
mod song;
mod song_export;
mod source;
mod usdx;
mod vendor;
mod vendor_scripts;

pub use analyzer::{
    AnalysisQueue, FailureKind, acknowledge_failures, cancel_analysis, delete_cache, enqueue,
    enqueue_all, enqueue_one, realign, realign_with_alt_backend, realign_with_backend,
    reanalyze_force_transcribe, reanalyze_full, reanalyze_transcript, refresh_metadata,
    remove_from_queue, remove_from_queue_one, set_song_language, shutdown_server,
};
pub use backgrounds::{
    BackgroundSelection, CustomBackground, CustomBackgroundKind, CustomBackgroundSource,
    MAX_BACKGROUND_UPLOAD_BYTES, add_custom_background_url, custom_background_kind_for_name,
    custom_background_max_bytes, import_custom_background_file, load_custom_background_shader,
    remove_custom_background, resolve_custom_background_path,
};
pub use cache::{
    CacheDir, CachePaths, CacheStats, cache_roots, change_app_data_path, clear_models,
    clear_videos, default_nightingale_dir, nightingale_dir, normalized_target_path, same_path,
    set_default_data_path,
};
pub use chromecast::cast_song_to_configured_device;
pub use config::{AppConfig, ChromecastConfig, LibrarySource, WindowState};
pub use library_db::{init_library, library_db_path};
pub use library_menu::{LibraryMenuItem, LibraryMenuItems, load_library_menu_items};
pub use library_model::{
    LibraryMenuFilters, LoadSongsParams, SongSort, SongSortColumn, SongTarget, SongsMeta,
    SongsStore, SortDirection,
};
pub use lyrics::{
    LineWindow, LrclibCandidate, LyricsFile, SidecarLrc, SidecarLrcKind, align_lrc_lyrics,
    apply_timed_lyrics, load_lyrics_file, load_sidecar_lrc, provide_lrc, save_lyrics_and_realign,
    search_lrclib_for_hash,
};
pub use media_server::{MediaEndpoint, local_media_roots};
pub use playback::{
    AudioPaths, PixabayVideoDownloaded, ShiftDone, ShiftResult, StemsReady,
    download_pixabay_videos, ensure_mp3_stems, ensure_mp3_stems_ready_payload,
    ensure_playable_source_video, get_audio_paths, get_cached_pixabay_videos, load_transcript,
    prefetch_one_per_flavor, shift_key, shift_key_done_payload, shift_tempo,
    shift_tempo_done_payload,
};
pub use playback_queue::{PlaybackQueue, PlaybackQueueEntry};
pub use playback_session::{PlaybackPlayer, PlaybackSession, PlaybackSessionStore};
pub use profile::ProfileStore;
pub use remote_control::{REMOTE_PORT, RemoteControl, RemoteInfo, lan_base, lan_ip};
pub use scanner::{CacheReconcileSummary, reconcile_cache, start_scan};
pub use search::{
    find_alternative_analyzed_songs, find_best_matching_local_song, find_song_by_hash,
};
pub use song::{Song, SongOrigin};
pub use song_export::{export_song_nge, exports_dir};
pub use source::{
    JellyfinAuth, JellyfinSource, MediaSource, NavidromeAuth, NavidromeSource, PlexAuth,
    PlexSource, SourceKind, active_source,
    jellyfin::{
        JellyfinHealth, JellyfinLibrary, JellyfinLoginResult, login as jellyfin_login,
        ping as jellyfin_ping, ping_current as jellyfin_ping_current,
    },
    navidrome::{
        NavidromeHealth, NavidromeLoginResult, login as navidrome_login, ping as navidrome_ping,
        ping_current as navidrome_ping_current,
    },
    plex::{
        PlexHealth, PlexPinPollResult, PlexPinStart, PlexSection, PlexServer,
        begin_pin as plex_begin_pin, manual_login as plex_manual_login, ping as plex_ping,
        ping_current as plex_ping_current, poll_pin as plex_poll_pin,
    },
};
pub use vendor::{
    SetupFolders, SetupProgress, SetupStep, clear_vendor_dir, is_ready, mark_ready,
    refresh_analyzer_scripts_if_ready, resolve_data_path_input, run_vendor_setup, step_create_venv,
    step_download_ffmpeg, step_download_uv, step_extract_scripts, step_install_packages,
    step_install_python,
};

pub fn startup() -> Result<(), String> {
    init_library().map_err(|e| e.to_string())?;

    let config = AppConfig::load();

    // The worker that owned any persisted rows died with the last process --
    // nothing is actually "analyzing" or dequeued right now. Snapshot the
    // hashes before wiping so `restore_analyze` can re-enqueue them through
    // the normal clean path afterward, rather than trusting the stale
    // status/percentage left behind. This also resets any `analyzing` rows
    // back to `queued`, since a stale `analyzing` row is excluded from the
    // "Remove from queue" bulk action (it assumes a live worker will revisit
    // it).
    let restore_hashes: Vec<String> = if config.restore_analyze() {
        AnalysisQueue::load().entries.into_keys().collect()
    } else {
        Vec::new()
    };

    AnalysisQueue::clear();

    analyzer::enqueue_many(&restore_hashes);

    let cache = CacheDir::new();

    if let Err(e) = library_db::rewrite_legacy_jellyfin_paths(&cache.path) {
        tracing::warn!("Failed to migrate legacy Jellyfin paths: {e}");
    }

    if let Err(e) = refresh_analyzer_scripts_if_ready() {
        tracing::warn!("Failed to refresh analyzer scripts: {e}");
    }

    if config.auto_analyze() {
        enqueue_all(&LibraryMenuFilters::default());
    }

    Ok(())
}
