//! Coarse wall-clock timing for `parallel_analysis` dispatches -- distinct
//! from `analysis_timings`, which only has per-stage data for runs the
//! *local* analyzer pipeline actually executed and so has nothing to say
//! about a song offloaded to a peer. One row per successful dispatch,
//! covering ping/path-lookup + (trigger + poll, if the peer hadn't already
//! analyzed it) + downloading results + local finalize, all as a single
//! `total_ms`. Read with `scripts/analysis_progress.py`.

use rusqlite::params;

use super::connection::with_conn_mut;

pub struct ParallelAnalysisTimingRow<'a> {
    pub file_hash: &'a str,
    pub peer_url: &'a str,
    /// Whether the peer already had this song analyzed when we checked (a
    /// near-instant fetch-only dispatch), as opposed to us triggering and
    /// waiting on it.
    pub already_analyzed_on_peer: bool,
    /// How many `POLL_INTERVAL` ticks it took the peer to finish, or `None`
    /// when `already_analyzed_on_peer` (no polling happened).
    pub poll_attempts: Option<u32>,
    pub total_ms: u64,
}

pub fn insert_parallel_analysis_timing(row: &ParallelAnalysisTimingRow) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO parallel_analysis_timings (
                file_hash, peer_url, started_at, already_analyzed_on_peer, poll_attempts, total_ms
            ) VALUES (
                ?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?3, ?4, ?5
            )",
            params![
                row.file_hash,
                row.peer_url,
                row.already_analyzed_on_peer,
                row.poll_attempts,
                row.total_ms as i64,
            ],
        )?;
        Ok(())
    })
}
