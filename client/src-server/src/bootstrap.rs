use app_core::{AppConfig, SongsStore};
use axum::{extract::State, Json};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Bootstrap {
    config: AppConfig,
    songs_meta: app_core::SongsMeta,
    /// The data folder is fixed by the operator (via `NIGHTINGALE_DATA_PATH`),
    /// so the setup wizard hides the data-folder picker.
    data_path_pinned: bool,
    /// The library folder is fixed by the operator (via
    /// `NIGHTINGALE_LIBRARY_PATH`), so the UI hides the folder-select action.
    library_pinned: bool,
}

/// Replaces the `initialization_script` Tauri injects on window creation:
/// the web client awaits this once and seeds `window.__NIGHTINGALE_*` from
/// the response before mounting React.
pub(crate) async fn handle(State(state): State<AppState>) -> Json<Bootstrap> {
    let config = AppConfig::load();
    let songs_meta = SongsStore::load_meta();
    Json(Bootstrap {
        config,
        songs_meta,
        data_path_pinned: state.data_path_pinned,
        library_pinned: state.library_pinned,
    })
}
