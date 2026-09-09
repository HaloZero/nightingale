use app_core::{PlaybackSession, PlaybackSessionStore};
use tauri::{AppHandle, Emitter, State};

const SESSION_CHANGED_EVENT: &str = "playback-session-changed";

#[tauri::command]
pub(crate) fn load_playback_session(
    sessions: State<'_, PlaybackSessionStore>,
) -> Result<Option<PlaybackSession>, String> {
    sessions.load()
}

#[tauri::command]
pub(crate) fn save_playback_session(
    app: AppHandle,
    sessions: State<'_, PlaybackSessionStore>,
    session: PlaybackSession,
) -> Result<PlaybackSession, String> {
    let session = sessions.save(session)?;
    app.emit(SESSION_CHANGED_EVENT, &session)
        .map_err(|error| error.to_string())?;
    Ok(session)
}
