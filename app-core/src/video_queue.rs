//! In-flight visibility for the karaoke-video / YouTube-video pipelines --
//! the video equivalent of `analyzer::AnalysisQueue`, minus percentage and
//! failure tracking (see `library_db::video_processing_queue`'s doc comment
//! for why: this only answers "how many are in flight right now").
//!
//! Standalone rather than folded into `karaoke_video` or `youtube_video`
//! since it spans both -- neither module should own the other's `kind`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::library_db;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum VideoQueueKind {
    Reel,
    Youtube,
}

impl VideoQueueKind {
    fn as_db_str(self) -> &'static str {
        match self {
            Self::Reel => "reel",
            Self::Youtube => "youtube",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum VideoQueueStage {
    Queued,
    Processing,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VideoQueueEntry {
    pub file_hash: String,
    pub kind: VideoQueueKind,
    pub stage: VideoQueueStage,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VideoProcessingQueue {
    pub entries: Vec<VideoQueueEntry>,
}

impl VideoProcessingQueue {
    pub fn load() -> Self {
        let rows = library_db::video_queue_load_rows().unwrap_or_default();
        let entries = rows
            .into_iter()
            .filter_map(|(file_hash, kind, stage)| {
                let kind = match kind.as_str() {
                    "reel" => VideoQueueKind::Reel,
                    "youtube" => VideoQueueKind::Youtube,
                    other => {
                        tracing::warn!("[video_queue] unknown kind {other:?} for {file_hash}, dropping row");
                        return None;
                    }
                };
                let stage = match stage.as_str() {
                    "queued" => VideoQueueStage::Queued,
                    "processing" => VideoQueueStage::Processing,
                    other => {
                        tracing::warn!("[video_queue] unknown stage {other:?} for {file_hash}, dropping row");
                        return None;
                    }
                };
                Some(VideoQueueEntry { file_hash, kind, stage })
            })
            .collect();
        Self { entries }
    }
}

pub fn mark_queued_many(kind: VideoQueueKind, file_hashes: &[String]) {
    if let Err(e) = library_db::video_queue_mark_queued_many(kind.as_db_str(), file_hashes) {
        tracing::warn!("[video_queue] failed to mark {} hash(es) queued: {e}", file_hashes.len());
    }
}

/// Returns the `started_at` token to pass back to `clear`.
pub fn mark_processing(file_hash: &str, kind: VideoQueueKind) -> String {
    match library_db::video_queue_mark_processing(file_hash, kind.as_db_str()) {
        Ok(token) => token,
        Err(e) => {
            tracing::warn!("[video_queue] failed to mark {file_hash} processing: {e}");
            String::new()
        }
    }
}

pub fn clear(file_hash: &str, kind: VideoQueueKind, started_at: &str) {
    if let Err(e) = library_db::video_queue_clear(file_hash, kind.as_db_str(), started_at) {
        tracing::warn!("[video_queue] failed to clear {file_hash}: {e}");
    }
}

pub fn clear_all() {
    if let Err(e) = library_db::video_queue_clear_all() {
        tracing::warn!("[video_queue] failed to clear table on startup: {e}");
    }
}
