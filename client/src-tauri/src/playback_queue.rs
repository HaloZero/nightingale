use app_core::{PlaybackQueue, PlaybackQueueEntry};
use tauri::{AppHandle, Emitter, State};

const QUEUE_CHANGED_EVENT: &str = "playback-queue-changed";

fn emit_queue(app: &AppHandle, entries: &[PlaybackQueueEntry]) -> Result<(), String> {
    app.emit(QUEUE_CHANGED_EVENT, entries)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn load_playback_queue(
    queue: State<'_, PlaybackQueue>,
) -> Result<Vec<PlaybackQueueEntry>, String> {
    queue.entries()
}

#[tauri::command]
pub(crate) fn add_playback_queue_entry(
    app: AppHandle,
    queue: State<'_, PlaybackQueue>,
    file_hash: String,
    tempo: f64,
    key_offset: i32,
) -> Result<Vec<PlaybackQueueEntry>, String> {
    let entries = queue.add(&file_hash, tempo, key_offset)?;
    emit_queue(&app, &entries)?;
    Ok(entries)
}

#[tauri::command]
pub(crate) fn remove_playback_queue_entry(
    app: AppHandle,
    queue: State<'_, PlaybackQueue>,
    id: String,
) -> Result<Vec<PlaybackQueueEntry>, String> {
    let entries = queue.remove(&id)?;
    emit_queue(&app, &entries)?;
    Ok(entries)
}

#[tauri::command]
pub(crate) fn clear_playback_queue(
    app: AppHandle,
    queue: State<'_, PlaybackQueue>,
) -> Result<Vec<PlaybackQueueEntry>, String> {
    let entries = queue.clear()?;
    emit_queue(&app, &entries)?;
    Ok(entries)
}
