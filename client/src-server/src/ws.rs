use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};

use crate::events::EventEnvelope;
use crate::jukebox::{next_client_id, ClientId, JukeboxState};
use crate::state::AppState;

pub(crate) async fn handle_upgrade(
    State(state): State<AppState>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| run(state, socket))
}

async fn run(state: AppState, socket: WebSocket) {
    let client_id = next_client_id();
    tracing::debug!(%client_id, "ws connected");

    let (mut sender, mut receiver) = socket.split();
    let mut events = state.events.subscribe();

    // Snapshot the current jukebox state so the new client doesn't have to
    // wait for the next mutation to render its UI.
    if let Ok(text) = serde_json::to_string(&EventEnvelope {
        r#type: "jukebox".to_string(),
        payload: serde_json::to_value(state.jukebox.snapshot().await)
            .unwrap_or(serde_json::Value::Null),
    }) {
        let _ = sender.send(Message::Text(text)).await;
    }

    let outbound = {
        let mut sender = sender;
        async move {
            loop {
                match events.recv().await {
                    Ok(envelope) => {
                        let Ok(text) = serde_json::to_string(&envelope) else {
                            continue;
                        };
                        if sender.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        // Slow consumer: drop the backlog and keep the socket
                        // alive rather than disconnecting.
                        tracing::warn!(%client_id, %skipped, "ws lagged behind broadcast");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    };

    let inbound = {
        let state = state.clone();
        async move {
            while let Some(msg) = receiver.next().await {
                let Ok(msg) = msg else { break };
                if let Message::Text(text) = msg {
                    handle_client_message(&state, client_id, &text).await;
                } else if let Message::Close(_) = msg {
                    break;
                }
            }
        }
    };

    tokio::select! {
        _ = outbound => {},
        _ = inbound => {},
    }

    // Release any session-scoped state the client owned.
    let next = state
        .jukebox
        .mutate(|s| {
            if s.mic_owner == Some(client_id) {
                s.mic_owner = None;
            }
            if s.controller == Some(client_id) {
                s.controller = None;
            }
        })
        .await;
    broadcast_jukebox(&state, &next);

    tracing::debug!(%client_id, "ws disconnected");
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ClientFrame {
    #[serde(rename = "jukebox.claim_mic")]
    ClaimMic,
    #[serde(rename = "jukebox.release_mic")]
    ReleaseMic,
    #[serde(rename = "jukebox.set")]
    Set {
        #[serde(default)]
        force: bool,
        #[serde(flatten)]
        patch: JukeboxPatch,
    },
    #[serde(rename = "jukebox.pitch")]
    Pitch {
        #[serde(default)]
        hz: Option<f32>,
        #[serde(default)]
        rms: Option<f32>,
    },
    #[serde(rename = "jukebox.score")]
    Score { delta: i32 },
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct JukeboxPatch {
    #[serde(default)]
    current_song: Option<Option<String>>,
    #[serde(default)]
    paused: Option<bool>,
    #[serde(default)]
    position_ms: Option<u64>,
    #[serde(default)]
    theme: Option<Option<usize>>,
    #[serde(default)]
    score: Option<u32>,
}

#[derive(Debug, Serialize)]
struct DenyMessage {
    reason: &'static str,
}

async fn handle_client_message(state: &AppState, client_id: ClientId, raw: &str) {
    let Ok(frame) = serde_json::from_str::<ClientFrame>(raw) else {
        tracing::trace!("dropping malformed ws frame: {raw}");
        return;
    };

    match frame {
        ClientFrame::ClaimMic => {
            let next = state
                .jukebox
                .mutate(|s| {
                    if s.mic_owner.is_none() {
                        s.mic_owner = Some(client_id);
                        if s.controller.is_none() {
                            s.controller = Some(client_id);
                        }
                    }
                })
                .await;
            if next.mic_owner != Some(client_id) {
                state.events.emit(
                    "jukebox.deny",
                    &DenyMessage {
                        reason: "mic-owned",
                    },
                );
                return;
            }
            broadcast_jukebox(state, &next);
        }
        ClientFrame::ReleaseMic => {
            let next = state
                .jukebox
                .mutate(|s| {
                    if s.mic_owner == Some(client_id) {
                        s.mic_owner = None;
                    }
                })
                .await;
            broadcast_jukebox(state, &next);
        }
        ClientFrame::Set { force, patch } => {
            let next = state
                .jukebox
                .mutate(|s| {
                    if s.controller.is_none() || force {
                        s.controller = Some(client_id);
                    }
                    if s.controller != Some(client_id) {
                        return;
                    }
                    apply_patch(s, patch);
                })
                .await;
            broadcast_jukebox(state, &next);
        }
        ClientFrame::Pitch { hz, rms } => {
            let next = state
                .jukebox
                .mutate(|s| {
                    if s.mic_owner != Some(client_id) {
                        return;
                    }
                    s.pitch_hz = hz;
                    s.rms = rms;
                })
                .await;
            broadcast_jukebox(state, &next);
        }
        ClientFrame::Score { delta } => {
            let next = state
                .jukebox
                .mutate(|s| {
                    if delta.is_negative() {
                        let mag = (-delta) as u32;
                        s.score = s.score.saturating_sub(mag);
                    } else {
                        s.score = s.score.saturating_add(delta as u32);
                    }
                })
                .await;
            broadcast_jukebox(state, &next);
        }
    }
}

fn apply_patch(state: &mut JukeboxState, patch: JukeboxPatch) {
    if let Some(song) = patch.current_song {
        state.current_song = song;
    }
    if let Some(paused) = patch.paused {
        state.paused = paused;
    }
    if let Some(position_ms) = patch.position_ms {
        state.position_ms = position_ms;
    }
    if let Some(theme) = patch.theme {
        state.theme = theme;
    }
    if let Some(score) = patch.score {
        state.score = score;
    }
}

fn broadcast_jukebox(state: &AppState, snapshot: &JukeboxState) {
    state.events.emit("jukebox", snapshot);
}
