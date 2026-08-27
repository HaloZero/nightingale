use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LibraryMenuItem {
    pub value: String,
    pub label: String,
    #[serde(rename = "analysedCount")]
    pub analysed_count: u64,
    #[serde(rename = "queuedCount")]
    pub queued_count: u64,
    #[serde(rename = "analysingCount")]
    pub analysing_count: u64,
    pub count: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LibraryMenuItems {
    pub hot: Vec<LibraryMenuItem>,
    pub no_metadata: Vec<LibraryMenuItem>,
    pub lyrics: Vec<LibraryMenuItem>,
    pub karaoke_video: Vec<LibraryMenuItem>,
    pub artists: Vec<LibraryMenuItem>,
    pub albums: Vec<LibraryMenuItem>,
    pub genres: Vec<LibraryMenuItem>,
    pub playlists: Vec<LibraryMenuItem>,
    pub languages: Vec<LibraryMenuItem>,
}

pub fn load_library_menu_items() -> rusqlite::Result<LibraryMenuItems> {
    crate::library_db::query_library_menu_items()
}
