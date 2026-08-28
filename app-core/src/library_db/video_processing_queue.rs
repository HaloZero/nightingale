//! Live in-progress state for the karaoke-video / YouTube-video pipelines.
//!
//! Unlike `karaoke_video_status` (a cache of what's on disk) and
//! `karaoke_video_runs` (an append-only history log), this table tracks
//! *right now, in this instant*: which songs are queued or actively being
//! processed by `karaoke_video::bulk_karaoke_video` or one of the
//! fire-and-forget single-song commands in `client/src-server`. One row per
//! `(file_hash, kind)`, deleted on completion (success or failure alike --
//! this table only answers "how many are in flight," not "did it work").
//!
//! `started_at` doubles as a race-guard token: `mark_processing` returns the
//! exact value it wrote, and the caller must pass that same value back to
//! `video_queue_clear` so a bulk pass and a concurrent single-song action on
//! the same song can't delete each other's still-in-progress row.

use rusqlite::params;

use super::connection::{with_conn, with_conn_mut};

pub fn video_queue_mark_queued_many(kind: &str, file_hashes: &[String]) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        for file_hash in file_hashes {
            tx.execute(
                "INSERT INTO video_processing_queue (file_hash, kind, stage, started_at)
                 VALUES (?1, ?2, 'queued', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(file_hash, kind) DO UPDATE SET stage = 'queued'",
                params![file_hash, kind],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
}

/// Returns the `started_at` token written for this row -- pass it back to
/// `video_queue_clear` to guard against clearing a different in-flight run.
pub fn video_queue_mark_processing(file_hash: &str, kind: &str) -> rusqlite::Result<String> {
    with_conn_mut(|c| {
        c.query_row(
            "INSERT INTO video_processing_queue (file_hash, kind, stage, started_at)
             VALUES (?1, ?2, 'processing', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(file_hash, kind) DO UPDATE SET
               stage = 'processing',
               started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             RETURNING started_at",
            params![file_hash, kind],
            |r| r.get(0),
        )
    })
}

pub fn video_queue_clear(file_hash: &str, kind: &str, started_at: &str) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "DELETE FROM video_processing_queue
             WHERE file_hash = ?1 AND kind = ?2 AND started_at = ?3",
            params![file_hash, kind, started_at],
        )?;
        Ok(())
    })
}

/// Startup sweep -- whatever worker owned a row died with the last process,
/// so nothing is actually in flight right now (same reasoning as
/// `AnalysisQueue::clear()` in `startup()`, but with no restore/re-enqueue
/// step: video generation isn't a durable job, it just gets re-triggered by
/// the next freshness-checked bulk/single action).
pub fn video_queue_clear_all() -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute("DELETE FROM video_processing_queue", [])?;
        Ok(())
    })
}

pub fn video_queue_load_rows() -> rusqlite::Result<Vec<(String, String, String)>> {
    with_conn(|c| {
        let mut stmt =
            c.prepare("SELECT file_hash, kind, stage FROM video_processing_queue")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        rows.collect()
    })
}
