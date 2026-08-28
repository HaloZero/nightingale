use app_core::{
    AudioPaths, PixabayVideoDownloaded, YoutubeBackground, ensure_mp3_stems_ready_payload,
};
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub fn load_transcript(file_hash: String) -> Result<serde_json::Value, String> {
    app_core::load_transcript(&file_hash).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_audio_paths(file_hash: String) -> AudioPaths {
    app_core::get_audio_paths(&file_hash)
}

#[tauri::command]
pub fn ensure_mp3_stems(app: AppHandle, file_hash: String) {
    std::thread::spawn(move || {
        let _ = app.emit("stems-ready", ensure_mp3_stems_ready_payload(file_hash));
    });
}

#[tauri::command]
pub fn ensure_playable_source_video(file_hash: String) -> Option<String> {
    app_core::ensure_playable_source_video(&file_hash)
        .ok()
        .flatten()
}

#[tauri::command]
pub fn load_youtube_background(file_hash: String) -> Option<YoutubeBackground> {
    app_core::ensure_youtube_background(&file_hash)
}

#[tauri::command]
pub fn fetch_pixabay_videos(app: AppHandle, flavor: String) -> Vec<String> {
    let cached = app_core::get_cached_pixabay_videos(&flavor);

    let flavor_clone = flavor.clone();
    std::thread::spawn(move || {
        app_core::download_pixabay_videos(&flavor_clone, move |path, evicted_path| {
            let _ = app.emit(
                "pixabay-video-downloaded",
                PixabayVideoDownloaded::new(flavor.clone(), path, evicted_path),
            );
        });
    });

    cached
}
