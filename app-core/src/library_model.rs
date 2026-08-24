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
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum SongTarget {
    Hashes { hashes: Vec<String> },
    Filter { filters: LibraryMenuFilters },
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
    /// Count of songs matching the current filter that are already analyzed
    /// -- lets the frontend gate analyzed-only bulk actions (e.g. full
    /// reanalysis, refetch lyrics & align) without paging in every song in
    /// the filtered set.
    #[serde(default)]
    pub analyzed_count: usize,
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
