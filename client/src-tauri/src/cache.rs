use app_core::{clear_models, clear_videos, CacheStats};

#[tauri::command]
pub(crate) fn calculate_cache_stats() -> CacheStats {
    CacheStats::calculate()
}

#[tauri::command]
pub(crate) fn clear_videos_command() {
    clear_videos();
}

#[tauri::command]
pub(crate) fn clear_models_command() {
    clear_models();
}

#[tauri::command]
pub(crate) fn clear_all() {
    clear_models();
    clear_videos();
}
