//! Songs `parallel_analysis` expected to match a peer instance (same path,
//! same content hash -- see `AppConfig::parallel_analysis_url`) but didn't.
//!
//! One row per currently-mismatched local song: `record` upserts a row when
//! a check fails, `clear` removes it once a later check finds a match. The
//! table is therefore always a live "what's wrong right now" view, not an
//! append-only history -- read it with `scripts/parallel_analysis_mismatches.py`.

use rusqlite::params;

use super::connection::with_conn_mut;

pub fn record_parallel_analysis_mismatch(
    file_hash: &str,
    path: &str,
    peer_url: &str,
    peer_hash: Option<&str>,
) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO parallel_analysis_mismatches (file_hash, path, peer_url, peer_hash, detected_at)
             VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(file_hash) DO UPDATE SET
               path = excluded.path,
               peer_url = excluded.peer_url,
               peer_hash = excluded.peer_hash,
               detected_at = excluded.detected_at",
            params![file_hash, path, peer_url, peer_hash],
        )?;
        Ok(())
    })
}

pub fn clear_parallel_analysis_mismatch(file_hash: &str) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "DELETE FROM parallel_analysis_mismatches WHERE file_hash = ?1",
            [file_hash],
        )?;
        Ok(())
    })
}
