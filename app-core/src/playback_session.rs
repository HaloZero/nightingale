use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::Song;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSession {
    pub song: Song,
    pub queue_playback: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playback_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct PlaybackSessionStore {
    session: Mutex<Option<PlaybackSession>>,
}

impl PlaybackSessionStore {
    pub fn load(&self) -> Result<Option<PlaybackSession>, String> {
        self.session
            .lock()
            .map(|session| session.clone())
            .map_err(|_| "playback session lock poisoned".to_string())
    }

    pub fn save(&self, session: PlaybackSession) -> Result<PlaybackSession, String> {
        let mut current = self
            .session
            .lock()
            .map_err(|_| "playback session lock poisoned".to_string())?;
        *current = Some(session.clone());
        Ok(session)
    }
}
