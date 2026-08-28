mod analyzer;
mod audiodb;
mod cache;
mod cast_protocol;
mod chromecast;
mod config;
mod error;
mod library_db;
mod karaoke_video;
mod library_menu;
mod library_model;
mod lrc;
mod lyrics;
pub mod media_server;
mod parallel_analysis;
mod playback;
mod profile;
mod scanner;
mod search;
mod secret;
mod song;
mod source;
mod usdx;
mod vendor;
mod vendor_scripts;
mod video_queue;
mod video_sync;
mod youtube_video;

pub use analyzer::{
    AnalysisQueue, FailureKind, acknowledge_failures, delete_cache, enqueue_all, enqueue_one,
    realign, realign_all, reanalyze_all_force_transcribe, reanalyze_all_full,
    reanalyze_all_transcript, reanalyze_force_transcribe, reanalyze_full, reanalyze_transcript,
    refresh_metadata, refresh_metadata_all, remove_from_queue_all, remove_from_queue_one,
    set_song_language, shutdown_server,
};
pub use audiodb::{MusicVideoResult, find_music_video_for_hash};
pub use cache::{
    CacheDir, CachePaths, CacheStats, cache_roots, change_app_data_path, clear_models,
    clear_videos, default_nightingale_dir, nightingale_dir,
    normalized_target_path, reels_dir, same_path,
};
pub use cast_protocol::{CAST_NAMESPACE, CastReceiverMessage};
pub use chromecast::cast_song_to_configured_device;
pub use config::{AppConfig, ChromecastConfig, LibrarySource};
pub use karaoke_video::{
    KaraokeVideoBackfillReport, KaraokeVideoReady, YoutubeKaraokeVideoReady,
    backfill_karaoke_video_status_from_cache, best_karaoke_video_path, ensure_karaoke_video,
    ensure_karaoke_video_ready_payload, ensure_youtube_background_karaoke_video,
    ensure_youtube_karaoke_video, fetch_youtube_karaoke_video_all, force_rerender_karaoke_video_all,
    render_karaoke_video_all,
};
pub use library_db::{init_library, library_db_path};
pub use library_menu::{LibraryMenuItem, LibraryMenuItems, load_library_menu_items};
pub use library_model::{LibraryMenuFilters, LoadSongsParams, SongsMeta, SongsStore};
pub use lyrics::{
    LrclibCandidate, LyricsFile, apply_timed_lyrics, load_lyrics_file, provide_lrc,
    save_lyrics_and_realign, search_lrclib_for_hash,
};
pub use media_server::MediaEndpoint;
pub use parallel_analysis::{
    ensure_dispatcher_running as parallel_analysis_ensure_dispatcher_running,
    manual_ping as parallel_analysis_ping, song_at_path as parallel_analysis_song_at_path,
};
pub use playback::{
    AudioPaths, MAX_BACKGROUND_REELS, MAX_BULK_DOWNLOAD, PixabayVideoDownloaded, ShiftDone,
    ShiftResult, StemsReady, build_background_reels, count_background_reels,
    download_all_pixabay_videos, download_pixabay_videos, ensure_mp3_stems,
    ensure_mp3_stems_ready_payload, ensure_playable_source_video, get_audio_paths,
    get_cached_pixabay_videos, load_transcript, prefetch_one_per_flavor, shift_key,
    shift_key_done_payload, shift_tempo, shift_tempo_done_payload,
};
pub use profile::ProfileStore;
pub use scanner::start_scan;
pub use search::{
    find_alternative_analyzed_songs, find_best_matching_local_song, find_song_by_hash,
};
pub use song::{Song, SongOrigin};
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
pub use video_queue::{
    VideoProcessingQueue, VideoQueueEntry, VideoQueueKind, VideoQueueStage,
    clear as clear_video_queue, mark_processing as mark_video_queue_processing,
};
pub use video_sync::{SyncResult, detect_sync_offset_for_hash};
pub use youtube_video::{YoutubeBackground, ensure_youtube_background, ensure_youtube_video_downloaded};

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

    // Same reasoning as the AnalysisQueue::clear() above -- whatever process
    // owned a video_processing_queue row is gone, so nothing is actually in
    // flight. No restore step: video generation isn't a durable job, it's
    // just re-triggered by the next freshness-checked bulk/single action.
    video_queue::clear_all();

    let cache = CacheDir::new();

    if let Err(e) = library_db::rewrite_legacy_jellyfin_paths(&cache.path) {
        tracing::warn!("Failed to migrate legacy Jellyfin paths: {e}");
    }

    if let Err(e) = refresh_analyzer_scripts_if_ready() {
        tracing::warn!("Failed to refresh analyzer scripts: {e}");
    }

    if config.auto_analyze() {
        analyzer::enqueue_all(&LibraryMenuFilters::default());
    }

    Ok(())
}
