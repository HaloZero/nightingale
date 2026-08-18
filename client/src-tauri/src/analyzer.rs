use app_core::{
    delete_cache as core_delete_cache, enqueue_all as core_enqueue_all,
    enqueue_one as core_enqueue_one, realign as core_realign, realign_all as core_realign_all,
    reanalyze_all_force_transcribe as core_reanalyze_all_force_transcribe,
    reanalyze_all_full as core_reanalyze_all_full,
    reanalyze_all_transcript as core_reanalyze_all_transcript,
    reanalyze_force_transcribe as core_reanalyze_force_transcribe,
    reanalyze_full as core_reanalyze_full, reanalyze_transcript as core_reanalyze_transcript,
    refresh_metadata as core_refresh_metadata, refresh_metadata_all as core_refresh_metadata_all,
    remove_from_queue_all as core_remove_from_queue_all,
    remove_from_queue_one as core_remove_from_queue_one, shift_key_done_payload,
    shift_tempo_done_payload, LibraryMenuFilters,
};
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub fn enqueue_one(file_hash: String) {
    core_enqueue_one(&file_hash);
}

#[tauri::command]
pub fn enqueue_all(filters: LibraryMenuFilters) {
    core_enqueue_all(&filters);
}

#[tauri::command]
pub fn delete_song_cache(file_hash: String) {
    core_delete_cache(&file_hash);
}

#[tauri::command]
pub fn reanalyze_transcript(file_hash: String, language: Option<String>) {
    core_reanalyze_transcript(&file_hash, language);
}

#[tauri::command]
pub fn reanalyze_full(file_hash: String) {
    core_reanalyze_full(&file_hash);
}

#[tauri::command]
pub fn realign(file_hash: String, language: Option<String>) {
    core_realign(&file_hash, language);
}

#[tauri::command]
pub fn reanalyze_force_transcribe(file_hash: String) {
    core_reanalyze_force_transcribe(&file_hash);
}

#[tauri::command]
pub fn reanalyze_all_full(filters: LibraryMenuFilters) -> usize {
    core_reanalyze_all_full(&filters)
}

#[tauri::command]
pub fn reanalyze_all_transcript(filters: LibraryMenuFilters, language: Option<String>) -> usize {
    core_reanalyze_all_transcript(&filters, language)
}

#[tauri::command]
pub fn reanalyze_all_force_transcribe(filters: LibraryMenuFilters) -> usize {
    core_reanalyze_all_force_transcribe(&filters)
}

#[tauri::command]
pub fn realign_all(filters: LibraryMenuFilters, language: Option<String>) -> usize {
    core_realign_all(&filters, language)
}

#[tauri::command]
pub fn refresh_metadata(file_hash: String) {
    core_refresh_metadata(&file_hash);
}

#[tauri::command]
pub fn refresh_metadata_all(filters: LibraryMenuFilters) -> usize {
    core_refresh_metadata_all(&filters)
}

#[tauri::command]
pub fn remove_from_queue_one(file_hash: String) {
    core_remove_from_queue_one(&file_hash);
}

#[tauri::command]
pub fn remove_from_queue_all(filters: LibraryMenuFilters) -> usize {
    core_remove_from_queue_all(&filters)
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
