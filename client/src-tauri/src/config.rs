use app_core::AppConfig;

use crate::microphones::set_monitor_gain;

#[tauri::command]
pub(crate) fn load_config() -> AppConfig {
    AppConfig::load()
}

#[tauri::command]
pub(crate) fn save_config(config: AppConfig) -> AppConfig {
    let was_auto_analyze = AppConfig::load().auto_analyze();
    config.save();
    set_monitor_gain(config.mic_monitor_gain());
    if config.auto_analyze() && !was_auto_analyze {
        let _ = app_core::enqueue(app_core::SongTarget::Filter {
            filters: app_core::LibraryMenuFilters::default(),
        });
    }
    config
}
