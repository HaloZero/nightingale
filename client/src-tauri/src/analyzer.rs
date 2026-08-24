use app_core::{
    delete_cache as core_delete_cache, enqueue as core_enqueue, realign as core_realign,
    reanalyze_force_transcribe as core_reanalyze_force_transcribe,
    reanalyze_full as core_reanalyze_full, reanalyze_transcript as core_reanalyze_transcript,
    refresh_metadata as core_refresh_metadata, shift_key_done_payload, shift_tempo_done_payload,
    SongTarget,
};
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub fn enqueue(target: SongTarget) -> Result<usize, String> {
    core_enqueue(target)
}

#[tauri::command]
pub fn delete_song_cache(target: SongTarget) -> Result<usize, String> {
    core_delete_cache(target)
}

#[tauri::command]
pub fn reanalyze_transcript(target: SongTarget, language: Option<String>) -> Result<usize, String> {
    core_reanalyze_transcript(target, language)
}

#[tauri::command]
pub fn reanalyze_full(target: SongTarget) -> Result<usize, String> {
    core_reanalyze_full(target)
}

#[tauri::command]
pub fn realign(target: SongTarget, language: Option<String>) -> Result<usize, String> {
    core_realign(target, language)
}

#[tauri::command]
pub fn reanalyze_force_transcribe(target: SongTarget) -> Result<usize, String> {
    core_reanalyze_force_transcribe(target)
}

#[tauri::command]
pub async fn refresh_metadata(target: SongTarget) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || core_refresh_metadata(target))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn shift_key(
    app: AppHandle,
    file_hash: String,
    key: String,
    pitch_ratio: f64,
    key_offset: i32,
) {
    std::thread::spawn(move || {
        let payload = shift_key_done_payload(file_hash, key, pitch_ratio, key_offset);
        let _ = app.emit("shift-key-done", payload);
    });
}

#[tauri::command]
pub fn shift_tempo(app: AppHandle, file_hash: String, tempo: f64) {
    std::thread::spawn(move || {
        let payload = shift_tempo_done_payload(file_hash, tempo);
        let _ = app.emit("shift-tempo-done", payload);
    });
}
