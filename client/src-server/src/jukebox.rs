use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use tokio::sync::RwLock;

pub(crate) type ClientId = u64;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_client_id() -> ClientId {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Single shared playback state for the jukebox session. The server is the
/// source of truth; every browser sees the same snapshot rebroadcast over WS.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct JukeboxState {
    pub current_song: Option<String>,
    pub paused: bool,
    pub position_ms: u64,
    pub pitch_hz: Option<f32>,
    pub rms: Option<f32>,
    pub mic_owner: Option<ClientId>,
    pub controller: Option<ClientId>,
    pub theme: Option<usize>,
    pub score: u32,
}

#[derive(Default)]
pub(crate) struct JukeboxStore {
    state: RwLock<JukeboxState>,
}

impl JukeboxStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn snapshot(&self) -> JukeboxState {
        self.state.read().await.clone()
    }

    pub(crate) async fn mutate<F>(&self, f: F) -> JukeboxState
    where
        F: FnOnce(&mut JukeboxState),
    {
        let mut guard = self.state.write().await;
        f(&mut guard);
        guard.clone()
    }
}
