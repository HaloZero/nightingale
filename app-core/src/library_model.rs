use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::song::Song;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LibraryMenuFilters {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub playlist: Option<String>,
    pub query: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub transcript_source: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    /// ISO language code (e.g. "en"), or "unknown" for songs with no
    /// language set yet. `Song.language` is only ever populated
    /// post-analysis (it doubles as the Whisper language hint/override), so
    /// this only meaningfully narrows analyzed songs.
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LoadSongsParams {
    pub search: Option<String>,
    pub filters: LibraryMenuFilters,
    pub skip: usize,
    pub take: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct SongsStore {
    pub count: usize,
    pub folder: String,
    pub processed: Vec<Song>,
    #[serde(default)]
    pub processed_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct SongsMeta {
    pub count: usize,
    pub folder: String,
    pub processed_count: usize,
    pub songs_count: usize,
    pub videos_count: usize,
    pub analyzed_count: usize,
}
