//! Per-run analysis timing log.
//!
//! Optional diagnostic table: one row per analyzer pipeline run, recording how
//! long each stage took alongside the settings that produced it. Writing to
//! this table is gated behind `AppConfig::track_analysis_timings` (default
//! on) at the call site in `analyzer.rs` — this module just inserts rows.

use rusqlite::params;

use super::connection::with_conn_mut;

pub struct AnalysisTimingRow<'a> {
    pub file_hash: &'a str,
    pub device: Option<&'a str>,
    pub whisper_model: &'a str,
    pub beam_size: u32,
    pub batch_size: u32,
    pub separator: &'a str,
    pub asr_engine: &'a str,
    pub align_backend: &'a str,
    pub vocal_detection_threshold_pct: f64,
    pub key_detect_ms: Option<u64>,
    pub separation_ms: Option<u64>,
    pub transcribe_or_align_ms: Option<u64>,
    pub total_ms: u64,
}

pub fn insert_analysis_timing(row: &AnalysisTimingRow) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO analysis_timings (
                file_hash, started_at, device, whisper_model, beam_size, batch_size,
                separator, asr_engine, align_backend, vocal_detection_threshold_pct,
                key_detect_ms, separation_ms, transcribe_or_align_ms, total_ms
            ) VALUES (
                ?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13
            )",
            params![
                row.file_hash,
                row.device,
                row.whisper_model,
                row.beam_size,
                row.batch_size,
                row.separator,
                row.asr_engine,
                row.align_backend,
                row.vocal_detection_threshold_pct,
                row.key_detect_ms.map(|v| v as i64),
                row.separation_ms.map(|v| v as i64),
                row.transcribe_or_align_ms.map(|v| v as i64),
                row.total_ms as i64,
            ],
        )?;
        Ok(())
    })
}
