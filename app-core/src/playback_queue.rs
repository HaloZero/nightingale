use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::{Song, SongsStore};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackQueueEntry {
    pub id: String,
    pub song: Song,
    pub tempo: f64,
    pub key_offset: i32,
}

#[derive(Debug, Default)]
pub struct PlaybackQueue {
    entries: Mutex<VecDeque<PlaybackQueueEntry>>,
    next_id: AtomicU64,
}

impl PlaybackQueue {
    pub fn entries(&self) -> Result<Vec<PlaybackQueueEntry>, String> {
        self.entries
            .lock()
            .map(|entries| entries.iter().cloned().collect())
            .map_err(|_| "playback queue lock poisoned".to_string())
    }

    pub fn add(
        &self,
        file_hash: &str,
        tempo: f64,
        key_offset: i32,
    ) -> Result<Vec<PlaybackQueueEntry>, String> {
        let song = SongsStore::load_by_hashes(&[file_hash.to_string()])
            .into_iter()
            .next()
            .ok_or_else(|| "song not found".to_string())?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "playback queue lock poisoned".to_string())?;

        entries.push_back(PlaybackQueueEntry {
            id,
            song,
            tempo,
            key_offset,
        });

        Ok(entries.iter().cloned().collect())
    }

    pub fn remove(&self, id: &str) -> Result<Vec<PlaybackQueueEntry>, String> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "playback queue lock poisoned".to_string())?;

        entries.retain(|entry| entry.id != id);

        Ok(entries.iter().cloned().collect())
    }

    pub fn clear(&self) -> Result<Vec<PlaybackQueueEntry>, String> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "playback queue lock poisoned".to_string())?;

        entries.clear();

        Ok(Vec::new())
    }
}
