use std::sync::Arc;

use app_core::{PlaybackQueue, PlaybackSessionStore};

use crate::events::EventBus;
use crate::jukebox::JukeboxStore;

#[derive(Clone)]
pub(crate) struct AppState {
    pub events: Arc<EventBus>,
    pub jukebox: Arc<JukeboxStore>,
    pub playback_queue: Arc<PlaybackQueue>,
    pub playback_sessions: Arc<PlaybackSessionStore>,
    pub data_path_pinned: bool,
    pub library_pinned: bool,
}

impl AppState {
    pub(crate) fn new(data_path_pinned: bool, library_pinned: bool) -> Self {
        Self {
            events: Arc::new(EventBus::new()),
            jukebox: Arc::new(JukeboxStore::new()),
            playback_queue: Arc::new(PlaybackQueue::default()),
            playback_sessions: Arc::new(PlaybackSessionStore::default()),
            data_path_pinned,
            library_pinned,
        }
    }
}
