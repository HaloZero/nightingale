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
    /// Populated when the run transcribed vocals from scratch via ASR;
    /// `None` when it aligned to known lyrics instead (see `align_ms`), or
    /// when transcription was skipped entirely (stems-only runs).
    pub transcribe_ms: Option<u64>,
    /// Populated when the run aligned known lyrics to the vocal stem rather
    /// than transcribing from scratch. Exactly one of `transcribe_ms` /
    /// `align_ms` is set per row -- whichever is `Some` says whether that
    /// run used lyrics.
    pub align_ms: Option<u64>,
    /// 1-minute system load average (`sysctl vm.loadavg`), sampled
    /// `SEPARATION_SNAPSHOT_DELAY` (120s) into the separation stage, as a
    /// cheap proxy for other processes (Plex transcodes, downloads, ...)
    /// competing for CPU/GPU while separation is actually running. `None`
    /// when separation was skipped, served from cache, or finished before
    /// the snapshot fired -- an attempt-start sample was tried first but
    /// mostly caught the GPU idle before separation had ramped up.
    pub load_avg_1m: Option<f64>,
    /// GPU utilization (0.0-1.0), clock speed, and die temperature from the
    /// same mid-separation `macmon` sample (https://github.com/vladkens/macmon)
    /// as `load_avg_1m` -- confirms whether stem separation is actually
    /// landing on the GPU and whether it's thermal-throttled. All three are
    /// `None` if `macmon` isn't installed or the sample fails; this is a
    /// best-effort diagnostic, not a requirement.
    pub gpu_active_ratio: Option<f64>,
    pub gpu_freq_mhz: Option<i64>,
    pub gpu_temp_c: Option<f64>,
    /// SoC-wide CPU utilization (0.0-1.0) from the same `macmon` sample as
    /// the GPU fields above -- a finer-grained companion to `load_avg_1m`.
    pub cpu_active_ratio: Option<f64>,
    /// Fraction of physical RAM in use at sample time, also from `macmon` --
    /// a proxy for memory pressure (macmon doesn't expose macOS's actual
    /// Normal/Warn/Critical pressure level, only raw usage).
    pub mem_pressure_ratio: Option<f64>,
    pub total_ms: u64,
}

pub fn insert_analysis_timing(row: &AnalysisTimingRow) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO analysis_timings (
                file_hash, started_at, device, whisper_model, beam_size, batch_size,
                separator, asr_engine, align_backend, vocal_detection_threshold_pct,
                key_detect_ms, separation_ms, transcribe_ms, align_ms, load_avg_1m,
                gpu_active_ratio, gpu_freq_mhz, gpu_temp_c, cpu_active_ratio,
                mem_pressure_ratio, total_ms
            ) VALUES (
                ?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18,
                ?19, ?20
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
                row.transcribe_ms.map(|v| v as i64),
                row.align_ms.map(|v| v as i64),
                row.load_avg_1m,
                row.gpu_active_ratio,
                row.gpu_freq_mhz,
                row.gpu_temp_c,
                row.cpu_active_ratio,
                row.mem_pressure_ratio,
                row.total_ms as i64,
            ],
        )?;
        Ok(())
    })
}
