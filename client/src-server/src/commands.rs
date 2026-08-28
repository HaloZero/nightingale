use app_core::{
    clear_video_queue, detect_sync_offset_for_hash, ensure_mp3_stems_ready_payload,
    find_music_video_for_hash, load_lyrics_file, mark_video_queue_processing,
    save_lyrics_and_realign, search_lrclib_for_hash, shift_key_done_payload,
    shift_tempo_done_payload, AnalysisQueue, AppConfig, CacheStats, FailureKind,
    LibraryMenuFilters, LibraryMenuItems, LibrarySource, LoadSongsParams, PixabayVideoDownloaded,
    ProfileStore, SongsStore, VideoProcessingQueue, VideoQueueKind,
};
use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::events::EventBus;
use crate::state::AppState;

/// HTTP error wrapper. JSON body matches what `webInvoke` reads on non-2xx.
pub struct ApiError(pub StatusCode, pub String);

impl ApiError {
    fn bad_request(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }

    fn internal(msg: impl Into<String>) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

pub type CmdResult = Result<Value, ApiError>;

/// Generic dispatcher that mirrors Tauri's `generate_handler!` table.
pub async fn handle_cmd(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let payload = body.map(|Json(v)| v).unwrap_or(Value::Null);
    let value = dispatch(state.events.clone(), &name, payload).await?;
    Ok(Json(value))
}

async fn dispatch(events: std::sync::Arc<EventBus>, name: &str, payload: Value) -> CmdResult {
    match name {
        // ── Init/window stubs ────────────────────────────────────────────
        "frontend_ready" | "window_immersive" | "minimize_window" => Ok(Value::Null),

        // ── Config ───────────────────────────────────────────────────────
        "load_config" => Ok(serde_json::to_value(AppConfig::load()).map_err(serde_err)?),
        "save_config" => save_config_cmd(payload),

        // ── Cache ────────────────────────────────────────────────────────
        "calculate_cache_stats" => {
            Ok(serde_json::to_value(CacheStats::calculate()).map_err(serde_err)?)
        }
        "clear_videos_command" => {
            app_core::clear_videos();
            Ok(Value::Null)
        }
        "clear_models_command" => {
            app_core::clear_models();
            Ok(Value::Null)
        }
        "clear_all" => {
            app_core::clear_models();
            app_core::clear_videos();
            Ok(Value::Null)
        }

        // ── Profile ──────────────────────────────────────────────────────
        "load_profiles" => Ok(serde_json::to_value(ProfileStore::load()).map_err(serde_err)?),
        "create_profile" => {
            let args: NameArgs = deserialize(payload)?;
            let mut store = ProfileStore::load();
            store.create_profile(args.name);
            Ok(Value::Null)
        }
        "switch_profile" => {
            let args: NameArgs = deserialize(payload)?;
            let mut store = ProfileStore::load();
            store.switch_profile(&args.name);
            Ok(Value::Null)
        }
        "delete_profile" => {
            let args: NameArgs = deserialize(payload)?;
            let mut store = ProfileStore::load();
            store.delete_profile(&args.name);
            Ok(Value::Null)
        }
        "add_score" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                song_hash: String,
                score: u32,
            }
            let args: Args = deserialize(payload)?;
            let mut store = ProfileStore::load();
            store.add_score(&args.song_hash, args.score);
            Ok(Value::Null)
        }

        // ── Scanner ──────────────────────────────────────────────────────
        "trigger_scan" => {
            app_core::start_scan();
            Ok(Value::Null)
        }
        "set_library_source" => {
            #[derive(Deserialize)]
            struct Args {
                source: LibrarySource,
            }
            let args: Args = deserialize(payload)?;
            let mut config = AppConfig::load();
            config.library_source = Some(args.source);
            config.last_folder = None;
            config.save();
            app_core::start_scan();
            Ok(serde_json::to_value(config).map_err(serde_err)?)
        }
        "clear_library_source" => {
            let mut config = AppConfig::load();
            config.library_source = None;
            config.last_folder = None;
            config.save();
            Ok(serde_json::to_value(config).map_err(serde_err)?)
        }
        "jellyfin_login" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                base_url: String,
                username: String,
                password: String,
            }
            let args: Args = deserialize(payload)?;
            let result =
                app_core::jellyfin_login(&args.base_url, &args.username, &args.password, None)
                    .map_err(|e| ApiError::bad_request(e.to_string()))?;
            Ok(serde_json::to_value(result).map_err(serde_err)?)
        }
        "jellyfin_ping" => {
            Ok(serde_json::to_value(app_core::jellyfin_ping_current()).map_err(serde_err)?)
        }
        "navidrome_login" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                base_url: String,
                username: String,
                password: String,
            }
            let args: Args = deserialize(payload)?;
            let result = app_core::navidrome_login(&args.base_url, &args.username, &args.password)
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            Ok(serde_json::to_value(result).map_err(serde_err)?)
        }
        "navidrome_ping" => {
            Ok(serde_json::to_value(app_core::navidrome_ping_current()).map_err(serde_err)?)
        }
        "plex_begin_pin" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                #[serde(default)]
                client_id: Option<String>,
            }
            let args: Args = deserialize(payload)?;
            let result =
                tokio::task::spawn_blocking(move || app_core::plex_begin_pin(args.client_id))
                    .await
                    .map_err(blocking_task_err)?
                    .map_err(|error| ApiError::bad_request(error.to_string()))?;
            Ok(serde_json::to_value(result).map_err(serde_err)?)
        }
        "plex_poll_pin" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                pin_id: String,
                client_id: String,
            }
            let args: Args = deserialize(payload)?;
            let result = tokio::task::spawn_blocking(move || {
                app_core::plex_poll_pin(&args.pin_id, &args.client_id)
            })
            .await
            .map_err(blocking_task_err)?
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
            Ok(serde_json::to_value(result).map_err(serde_err)?)
        }
        "plex_manual_login" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                base_url: String,
                access_token: String,
                #[serde(default)]
                client_id: Option<String>,
            }
            let args: Args = deserialize(payload)?;
            let result = tokio::task::spawn_blocking(move || {
                app_core::plex_manual_login(&args.base_url, &args.access_token, args.client_id)
            })
            .await
            .map_err(blocking_task_err)?
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
            Ok(serde_json::to_value(result).map_err(serde_err)?)
        }
        "plex_ping" => {
            let result = tokio::task::spawn_blocking(app_core::plex_ping_current)
                .await
                .map_err(blocking_task_err)?;
            Ok(serde_json::to_value(result).map_err(serde_err)?)
        }
        "parallel_analysis_ping" => {
            #[derive(Deserialize)]
            struct Args {
                url: String,
            }
            let args: Args = deserialize(payload)?;
            tracing::info!(
                "[cmd] parallel_analysis_ping received url={:?} (len={})",
                args.url,
                args.url.len()
            );
            let alive = tokio::task::spawn_blocking(move || app_core::parallel_analysis_ping(&args.url))
                .await
                .map_err(blocking_task_err)?;
            tracing::info!("[cmd] parallel_analysis_ping result alive={alive}");
            Ok(Value::Bool(alive))
        }
        "load_songs" => {
            #[derive(Deserialize)]
            struct Args {
                params: LoadSongsParams,
            }
            let args: Args = deserialize(payload)?;
            Ok(serde_json::to_value(SongsStore::load(&args.params)).map_err(serde_err)?)
        }
        "load_songs_by_hashes" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                file_hashes: Vec<String>,
            }
            let args: Args = deserialize(payload)?;
            Ok(
                serde_json::to_value(SongsStore::load_by_hashes(&args.file_hashes))
                    .map_err(serde_err)?,
            )
        }
        // Peer-to-peer lookup for `parallel_analysis`: a peer instance
        // checks whether *this* one has the same file at the same path
        // relative to its own library root (and, if so, whether the
        // content hash also matches) before offloading a song here.
        "load_song_by_path" => {
            #[derive(Deserialize)]
            struct Args {
                path: std::path::PathBuf,
            }
            let args: Args = deserialize(payload)?;
            Ok(serde_json::to_value(app_core::parallel_analysis_song_at_path(&args.path))
                .map_err(serde_err)?)
        }
        "load_songs_meta" => Ok(serde_json::to_value(SongsStore::load_meta()).map_err(serde_err)?),
        "load_analysis_queue" => {
            Ok(serde_json::to_value(AnalysisQueue::load()).map_err(serde_err)?)
        }
        "load_video_queue" => Ok(serde_json::to_value(VideoProcessingQueue::load()).map_err(serde_err)?),
        "load_library_menu_items" => {
            let items: LibraryMenuItems = app_core::load_library_menu_items()
                .map_err(|e| ApiError::internal(e.to_string()))?;
            Ok(serde_json::to_value(items).map_err(serde_err)?)
        }

        // ── Analyzer ─────────────────────────────────────────────────────
        "enqueue_one" => {
            let args: FileHashArgs = deserialize(payload)?;
            app_core::enqueue_one(&args.file_hash);
            Ok(Value::Null)
        }
        "enqueue_all" => {
            #[derive(Deserialize)]
            struct Args {
                filters: LibraryMenuFilters,
            }
            let args: Args = deserialize(payload)?;
            app_core::enqueue_all(&args.filters);
            Ok(Value::Null)
        }
        "delete_song_cache" => {
            let args: FileHashArgs = deserialize(payload)?;
            app_core::delete_cache(&args.file_hash);
            Ok(Value::Null)
        }
        "reanalyze_transcript" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                file_hash: String,
                #[serde(default)]
                language: Option<String>,
            }
            let args: Args = deserialize(payload)?;
            app_core::reanalyze_transcript(&args.file_hash, args.language);
            Ok(Value::Null)
        }
        "reanalyze_full" => {
            let args: FileHashArgs = deserialize(payload)?;
            app_core::reanalyze_full(&args.file_hash);
            Ok(Value::Null)
        }
        "realign" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                file_hash: String,
                #[serde(default)]
                language: Option<String>,
            }
            let args: Args = deserialize(payload)?;
            app_core::realign(&args.file_hash, args.language);
            Ok(Value::Null)
        }
        "reanalyze_force_transcribe" => {
            let args: FileHashArgs = deserialize(payload)?;
            app_core::reanalyze_force_transcribe(&args.file_hash);
            Ok(Value::Null)
        }
        "set_song_language" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                file_hash: String,
                #[serde(default)]
                language: Option<String>,
            }
            let args: Args = deserialize(payload)?;
            app_core::set_song_language(&args.file_hash, args.language);
            Ok(Value::Null)
        }
        "reanalyze_all_full" => {
            #[derive(Deserialize)]
            struct Args {
                filters: LibraryMenuFilters,
            }
            let args: Args = deserialize(payload)?;
            Ok(Value::from(app_core::reanalyze_all_full(&args.filters)))
        }
        "reanalyze_all_transcript" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                filters: LibraryMenuFilters,
                #[serde(default)]
                language: Option<String>,
            }
            let args: Args = deserialize(payload)?;
            Ok(Value::from(app_core::reanalyze_all_transcript(
                &args.filters,
                args.language,
            )))
        }
        "reanalyze_all_force_transcribe" => {
            #[derive(Deserialize)]
            struct Args {
                filters: LibraryMenuFilters,
            }
            let args: Args = deserialize(payload)?;
            Ok(Value::from(app_core::reanalyze_all_force_transcribe(
                &args.filters,
            )))
        }
        "realign_all" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                filters: LibraryMenuFilters,
                #[serde(default)]
                language: Option<String>,
            }
            let args: Args = deserialize(payload)?;
            Ok(Value::from(app_core::realign_all(
                &args.filters,
                args.language,
            )))
        }
        "refresh_metadata" => {
            let args: FileHashArgs = deserialize(payload)?;
            app_core::refresh_metadata(&args.file_hash);
            Ok(Value::Null)
        }
        "refresh_metadata_all" => {
            #[derive(Deserialize)]
            struct Args {
                filters: LibraryMenuFilters,
            }
            let args: Args = deserialize(payload)?;
            Ok(Value::from(app_core::refresh_metadata_all(&args.filters)))
        }
        "remove_from_queue_one" => {
            let args: FileHashArgs = deserialize(payload)?;
            app_core::remove_from_queue_one(&args.file_hash);
            Ok(Value::Null)
        }
        "remove_from_queue_all" => {
            #[derive(Deserialize)]
            struct Args {
                filters: LibraryMenuFilters,
            }
            let args: Args = deserialize(payload)?;
            Ok(Value::from(app_core::remove_from_queue_all(&args.filters)))
        }
        "acknowledge_analysis_failures" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                kind: FailureKind,
                file_hashes: Vec<String>,
            }
            let args: Args = deserialize(payload)?;
            app_core::acknowledge_failures(args.kind, args.file_hashes);
            Ok(Value::Null)
        }
        "shift_key" => shift_key_cmd(events, payload),
        "shift_tempo" => shift_tempo_cmd(events, payload),

        // ── Lyrics ───────────────────────────────────────────────────────
        "load_lyrics" => {
            let args: FileHashArgs = deserialize(payload)?;
            Ok(serde_json::to_value(load_lyrics_file(&args.file_hash)).map_err(serde_err)?)
        }
        "search_lrclib_lyrics" => {
            let args: FileHashArgs = deserialize(payload)?;
            Ok(serde_json::to_value(search_lrclib_for_hash(&args.file_hash)).map_err(serde_err)?)
        }
        "find_music_video" => {
            let args: FileHashArgs = deserialize(payload)?;
            Ok(serde_json::to_value(find_music_video_for_hash(&args.file_hash)).map_err(serde_err)?)
        }
        "download_youtube_video" => download_youtube_video_cmd(events, payload),
        "detect_video_sync_offset" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                file_hash: String,
                video_path: String,
            }
            let args: Args = deserialize(payload)?;
            let result =
                detect_sync_offset_for_hash(&args.file_hash, std::path::Path::new(&args.video_path))
                    .map_err(ApiError::bad_request)?;
            Ok(serde_json::to_value(result).map_err(serde_err)?)
        }
        "save_lyrics" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                file_hash: String,
                lines: Vec<String>,
            }
            let args: Args = deserialize(payload)?;
            save_lyrics_and_realign(&args.file_hash, args.lines).map_err(ApiError::bad_request)?;
            Ok(Value::Null)
        }
        "provide_lrc" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                file_hash: String,
                lrc_text: String,
                separate_stems: bool,
            }
            let args: Args = deserialize(payload)?;
            app_core::provide_lrc(&args.file_hash, &args.lrc_text, args.separate_stems)
                .map_err(ApiError::bad_request)?;
            Ok(Value::Null)
        }
        "apply_timed_lyrics" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                file_hash: String,
                lrc_text: String,
            }
            let args: Args = deserialize(payload)?;
            app_core::apply_timed_lyrics(&args.file_hash, &args.lrc_text)
                .map_err(ApiError::bad_request)?;
            Ok(Value::Null)
        }

        // ── Playback ─────────────────────────────────────────────────────
        "load_transcript" => {
            let args: FileHashArgs = deserialize(payload)?;
            app_core::load_transcript(&args.file_hash)
                .map_err(|e| ApiError::internal(e.to_string()))
        }
        "get_audio_paths" => {
            let args: FileHashArgs = deserialize(payload)?;
            Ok(
                serde_json::to_value(app_core::get_audio_paths(&args.file_hash))
                    .map_err(serde_err)?,
            )
        }
        "ensure_mp3_stems" => ensure_mp3_stems_cmd(events, payload),
        "ensure_playable_source_video" => {
            let args: FileHashArgs = deserialize(payload)?;
            let path = app_core::ensure_playable_source_video(&args.file_hash)
                .ok()
                .flatten();
            Ok(serde_json::to_value(path).map_err(serde_err)?)
        }
        "fetch_pixabay_videos" => fetch_pixabay_videos_cmd(events, payload),
        "download_all_pixabay_videos" => download_all_pixabay_videos_cmd(events, payload),
        "get_background_video_count" => get_background_video_count_cmd(payload),
        "get_background_reel_count" => get_background_reel_count_cmd(payload),
        "build_background_reels" => build_background_reels_cmd(events, payload),
        "render_karaoke_video" => render_karaoke_video_cmd(events, payload),
        "force_rerender_karaoke_video" => force_rerender_karaoke_video_cmd(events, payload),
        "fetch_youtube_karaoke_video" => fetch_youtube_karaoke_video_cmd(events, payload),
        "force_fetch_youtube_karaoke_video" => {
            force_fetch_youtube_karaoke_video_cmd(events, payload)
        }
        "render_karaoke_video_all" => {
            #[derive(Deserialize)]
            struct Args {
                filters: LibraryMenuFilters,
            }
            let args: Args = deserialize(payload)?;
            Ok(Value::from(app_core::render_karaoke_video_all(&args.filters)))
        }
        "force_rerender_karaoke_video_all" => {
            #[derive(Deserialize)]
            struct Args {
                filters: LibraryMenuFilters,
            }
            let args: Args = deserialize(payload)?;
            Ok(Value::from(app_core::force_rerender_karaoke_video_all(
                &args.filters,
            )))
        }
        "fetch_youtube_karaoke_video_all" => {
            #[derive(Deserialize)]
            struct Args {
                filters: LibraryMenuFilters,
            }
            let args: Args = deserialize(payload)?;
            Ok(Value::from(app_core::fetch_youtube_karaoke_video_all(
                &args.filters,
            )))
        }

        // ── Vendor ───────────────────────────────────────────────────────
        "is_ready" => Ok(Value::Bool(app_core::is_ready())),
        "trigger_setup" => crate::commands::vendor::trigger_setup(events, payload),

        // ── Mic (browser-side; no server-side state) ────────────────────
        "list_microphones" => Ok(Value::Array(vec![])),
        "start_mic_capture" | "stop_mic_capture" => Ok(Value::Null),

        _ => Err(ApiError(
            StatusCode::NOT_FOUND,
            format!("unknown command {name}"),
        )),
    }
}

fn serde_err(e: serde_json::Error) -> ApiError {
    ApiError::internal(format!("serialise: {e}"))
}

fn blocking_task_err(error: tokio::task::JoinError) -> ApiError {
    ApiError::internal(format!("blocking command failed: {error}"))
}

fn deserialize<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|e| ApiError::bad_request(format!("invalid args: {e}")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NameArgs {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileHashArgs {
    file_hash: String,
}

#[derive(Deserialize)]
struct SaveConfigArgs {
    config: AppConfig,
}

fn save_config_cmd(payload: Value) -> CmdResult {
    let SaveConfigArgs { config } = deserialize(payload)?;
    let previous = AppConfig::load();
    let was_auto_analyze = previous.auto_analyze();
    let was_parallel = (previous.parallel_analysis_enabled(), previous.parallel_analysis_url().map(str::to_string));
    let was_parallel_only = previous.parallel_analysis_only();
    config.save();
    if config.auto_analyze() && !was_auto_analyze {
        app_core::enqueue_all(&app_core::LibraryMenuFilters::default());
    }
    // Kick the dispatcher immediately on enable (or a URL change while
    // already enabled) rather than waiting for the next `enqueue_one` --
    // matches the `auto_analyze` transition handling above.
    let now_parallel = (config.parallel_analysis_enabled(), config.parallel_analysis_url().map(str::to_string));
    if now_parallel.0 && now_parallel != was_parallel {
        app_core::parallel_analysis_ensure_dispatcher_running();
    }
    // Turning `parallel_analysis_only` off can leave songs sitting queued
    // that the (gated-off) local worker never picked up -- kick it via the
    // same `enqueue_all` re-sweep the other transitions above use, rather
    // than waiting for the next song to be queued.
    if was_parallel_only && !config.parallel_analysis_only() {
        app_core::enqueue_all(&app_core::LibraryMenuFilters::default());
    }
    // Turning it *on* mid-run: the local worker finishes whatever song it's
    // already on, then stops draining the queue rather than being killed
    // outright (see the `parallel_analysis_only` check in `spawn_worker`'s
    // loop) -- but if the dispatcher had already exited (nothing left for it
    // to claim, since the local worker was ahead of it in the same queue),
    // nothing would otherwise wake it back up to pick up what the local
    // worker leaves behind. Kick it now rather than waiting for the next
    // `enqueue_one`/`enqueue_all`.
    if !was_parallel_only && config.parallel_analysis_only() {
        app_core::parallel_analysis_ensure_dispatcher_running();
    }
    // Web mode has no server-side cpal monitor stream, so `mic_monitor_gain`
    // is consumed entirely by the browser's monitor GainNode.
    serde_json::to_value(config).map_err(serde_err)
}

fn shift_key_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        file_hash: String,
        key: String,
        pitch_ratio: f64,
        key_offset: i32,
    }
    let args: Args = deserialize(payload)?;
    std::thread::spawn(move || {
        let payload =
            shift_key_done_payload(args.file_hash, args.key, args.pitch_ratio, args.key_offset);
        events.emit("shift-key-done", &payload);
    });
    Ok(Value::Null)
}

fn shift_tempo_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        file_hash: String,
        tempo: f64,
    }
    let args: Args = deserialize(payload)?;
    std::thread::spawn(move || {
        let payload = shift_tempo_done_payload(args.file_hash, args.tempo);
        events.emit("shift-tempo-done", &payload);
    });
    Ok(Value::Null)
}

fn ensure_mp3_stems_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    let args: FileHashArgs = deserialize(payload)?;
    std::thread::spawn(move || {
        events.emit(
            "stems-ready",
            &ensure_mp3_stems_ready_payload(args.file_hash),
        );
    });
    Ok(Value::Null)
}

fn fetch_pixabay_videos_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    #[derive(Deserialize)]
    struct Args {
        flavor: String,
    }
    let args: Args = deserialize(payload)?;
    let cached = app_core::get_cached_pixabay_videos(&args.flavor);

    let flavor_for_thread = args.flavor.clone();
    let events_clone = events.clone();
    std::thread::spawn(move || {
        let flavor_for_emit = flavor_for_thread.clone();
        app_core::download_pixabay_videos(&flavor_for_thread, move |path, evicted_path| {
            events_clone.emit(
                "pixabay-video-downloaded",
                &PixabayVideoDownloaded::new(flavor_for_emit.clone(), path, evicted_path),
            );
        });
    });

    Ok(json!(cached))
}

/// Cheap read-only count (just a directory listing, no download side
/// effects) -- lets the Settings UI show "N / cap cached" per flavor and
/// disable the download button once a flavor's already at
/// `MAX_BULK_DOWNLOAD`, without triggering `fetch_pixabay_videos_cmd`'s
/// rotation download as a side effect the way reusing that command would.
fn get_background_video_count_cmd(payload: Value) -> CmdResult {
    #[derive(Deserialize)]
    struct Args {
        flavor: String,
    }
    let args: Args = deserialize(payload)?;
    let count = app_core::get_cached_pixabay_videos(&args.flavor).len();

    Ok(json!({ "count": count, "cap": app_core::MAX_BULK_DOWNLOAD }))
}

/// Cheap read-only count of built reels for a flavor -- lets the Settings
/// UI show "N / max reels" and switch the build button's label to
/// "Regenerate reels" once a flavor already has a full set.
fn get_background_reel_count_cmd(payload: Value) -> CmdResult {
    #[derive(Deserialize)]
    struct Args {
        flavor: String,
    }
    let args: Args = deserialize(payload)?;
    let count = app_core::count_background_reels(&args.flavor);

    Ok(json!({ "count": count, "cap": app_core::MAX_BACKGROUND_REELS }))
}

/// Explicit, deliberately-triggered bulk download -- unlike
/// `fetch_pixabay_videos_cmd` above, this ignores the usual capped
/// rotation entirely (see `app_core::download_all_pixabay_videos`'s doc
/// comment for the real resource cost: potentially hundreds of videos,
/// multiple GB, several minutes). Fire-and-forget thread + progress
/// events, same shape as the other slow per-flavor/per-file commands in
/// this file.
fn download_all_pixabay_videos_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    #[derive(Deserialize)]
    struct Args {
        flavor: String,
    }
    let args: Args = deserialize(payload)?;

    let flavor_for_thread = args.flavor.clone();
    let events_clone = events.clone();
    std::thread::spawn(move || {
        let flavor_for_emit = flavor_for_thread.clone();
        app_core::download_all_pixabay_videos(&flavor_for_thread, move |message| {
            events_clone.emit(
                "pixabay-bulk-download-progress",
                &json!({ "flavor": flavor_for_emit, "message": message }),
            );
        });
        events.emit(
            "pixabay-bulk-download-done",
            &json!({ "flavor": args.flavor }),
        );
    });

    Ok(Value::Null)
}

/// Pre-builds a background reel pool for one flavor (`app_core::
/// build_background_reels`) karaoke video rendering picks from -- explicit,
/// one-time batch action per flavor. Same fire-and-forget-thread + progress
/// event pattern as `download_all_pixabay_videos_cmd`.
fn build_background_reels_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    #[derive(Deserialize)]
    struct Args {
        flavor: String,
    }
    let args: Args = deserialize(payload)?;

    let flavor_for_thread = args.flavor.clone();
    let events_clone = events.clone();
    std::thread::spawn(move || {
        let flavor_for_emit = flavor_for_thread.clone();
        app_core::build_background_reels(&flavor_for_thread, move |message| {
            events_clone.emit(
                "background-reels-progress",
                &json!({ "flavor": flavor_for_emit, "message": message }),
            );
        });
        events.emit("background-reels-done", &json!({ "flavor": args.flavor }));
    });
    Ok(Value::Null)
}

/// Downloads a song's official YouTube music video (`find_music_video`'s
/// result) as karaoke-video source footage -- can take anywhere from
/// seconds to over a minute (network + `MIN_DOWNLOAD_INTERVAL` throttling
/// in `youtube_video::ensure_youtube_video_downloaded`), so this is
/// fire-and-forget-thread + a single done event, same shape as the other
/// slow one-shot commands in this file.
fn download_youtube_video_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        file_hash: String,
        youtube_url: String,
    }
    let args: Args = deserialize(payload)?;

    std::thread::spawn(move || {
        let token = mark_video_queue_processing(&args.file_hash, VideoQueueKind::Youtube);
        let result =
            app_core::ensure_youtube_video_downloaded(&args.file_hash, &args.youtube_url, false);
        clear_video_queue(&args.file_hash, VideoQueueKind::Youtube, &token);
        events.emit(
            "youtube-video-download-done",
            &json!({
                "fileHash": args.file_hash,
                "path": result.as_ref().ok(),
                "error": result.err(),
            }),
        );
    });

    Ok(Value::Null)
}

/// Renders (or, with `force`, re-renders) a karaoke video without casting
/// it -- the explicit "make the video" action, independent of
/// `/api/cast`'s `chromecast.karaoke_video` path. Fire-and-forget thread +
/// `"karaoke-video-ready"` event, same shape as `ensure_mp3_stems_cmd`'s
/// `"stems-ready"`. With `force` omitted/false (the common case, e.g. a
/// bulk "render everything" pass), this is a no-op for a song whose video
/// is already fresh relative to its transcript -- see `ensure_karaoke_video`'s
/// `is_fresh` check -- so re-running it over a whole library only pays for
/// the songs that actually need it.
fn render_karaoke_video_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        file_hash: String,
        #[serde(default)]
        force: bool,
    }
    let args: Args = deserialize(payload)?;
    std::thread::spawn(move || {
        let token = mark_video_queue_processing(&args.file_hash, VideoQueueKind::Reel);
        let payload = app_core::ensure_karaoke_video_ready_payload(args.file_hash.clone(), args.force);
        clear_video_queue(&args.file_hash, VideoQueueKind::Reel, &token);
        events.emit("karaoke-video-ready", &payload);
    });
    Ok(Value::Null)
}

/// Same as `render_karaoke_video_cmd` but always `force`s a fresh render --
/// a distinct command rather than just exposing `force` on the one above so
/// the two are unambiguous, separately-triggerable UI actions (e.g. "render
/// if missing" vs. an explicit "no really, redo it" button), not one action
/// with a checkbox easy to leave on by accident.
fn force_rerender_karaoke_video_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    let args: FileHashArgs = deserialize(payload)?;
    std::thread::spawn(move || {
        let token = mark_video_queue_processing(&args.file_hash, VideoQueueKind::Reel);
        let payload = app_core::ensure_karaoke_video_ready_payload(args.file_hash.clone(), true);
        clear_video_queue(&args.file_hash, VideoQueueKind::Reel, &token);
        events.emit("karaoke-video-ready", &payload);
    });
    Ok(Value::Null)
}

/// The song-UI "fetch a YouTube video for this song and build a karaoke
/// video from it" action -- chains the AudioDB lookup (cached, see
/// `audiodb::find_music_video_for_hash`'s doc comment), the download, and a
/// forced karaoke-video re-render into one button. Fire-and-forget thread +
/// `"youtube-karaoke-video-ready"` event, same shape as the plain
/// `render_karaoke_video_cmd`/`force_rerender_karaoke_video_cmd` above. A
/// no-op if a fresh YouTube-background render already exists -- see
/// `force_fetch_youtube_karaoke_video_cmd` below for the "redo it anyway"
/// variant.
fn fetch_youtube_karaoke_video_cmd(events: std::sync::Arc<EventBus>, payload: Value) -> CmdResult {
    let args: FileHashArgs = deserialize(payload)?;
    std::thread::spawn(move || {
        let token = mark_video_queue_processing(&args.file_hash, VideoQueueKind::Youtube);
        let payload = app_core::ensure_youtube_karaoke_video(&args.file_hash, false);
        clear_video_queue(&args.file_hash, VideoQueueKind::Youtube, &token);
        events.emit("youtube-karaoke-video-ready", &payload);
    });
    Ok(Value::Null)
}

/// Same as `fetch_youtube_karaoke_video_cmd` but always `force`s a fresh
/// download and re-render, discarding whatever's cached -- for a bad
/// download or a stale render, not a wrong AudioDB match (the lookup itself
/// is still served from its own cache either way, see
/// `ensure_youtube_karaoke_video`'s doc comment). A distinct command rather
/// than a `force` flag on the one above, same reasoning as
/// `force_rerender_karaoke_video_cmd` vs. `render_karaoke_video_cmd`.
fn force_fetch_youtube_karaoke_video_cmd(
    events: std::sync::Arc<EventBus>,
    payload: Value,
) -> CmdResult {
    let args: FileHashArgs = deserialize(payload)?;
    std::thread::spawn(move || {
        let token = mark_video_queue_processing(&args.file_hash, VideoQueueKind::Youtube);
        let payload = app_core::ensure_youtube_karaoke_video(&args.file_hash, true);
        clear_video_queue(&args.file_hash, VideoQueueKind::Youtube, &token);
        events.emit("youtube-karaoke-video-ready", &payload);
    });
    Ok(Value::Null)
}

pub mod vendor;
