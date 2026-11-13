//! Per-run analysis timing log.
//!
//! Optional diagnostic table: one row per analyzer pipeline run, recording how
//! long each stage took alongside the settings that produced it. Writing to
//! this table is gated behind `AppConfig::track_analysis_timings` (default
//! on) at the call site in `analyzer.rs` -- this module just inserts rows.

use diesel::prelude::*;

use crate::error::NightingaleError;

use super::connection::with_conn_mut;
use super::schema::analysis_timings;
use super::sql_functions::strftime;

pub(crate) struct AnalysisTimingRow<'a> {
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

pub(crate) fn insert_analysis_timing(row: &AnalysisTimingRow<'_>) -> Result<(), NightingaleError> {
    with_conn_mut(|conn| {
        diesel::insert_into(analysis_timings::table)
            .values((
                analysis_timings::file_hash.eq(row.file_hash),
                analysis_timings::started_at.eq(strftime("%Y-%m-%dT%H:%M:%fZ", "now")),
                analysis_timings::device.eq(row.device),
                analysis_timings::whisper_model.eq(row.whisper_model),
                analysis_timings::beam_size.eq(row.beam_size as i64),
                analysis_timings::batch_size.eq(row.batch_size as i64),
                analysis_timings::separator.eq(row.separator),
                analysis_timings::asr_engine.eq(row.asr_engine),
                analysis_timings::align_backend.eq(row.align_backend),
                analysis_timings::vocal_detection_threshold_pct
                    .eq(row.vocal_detection_threshold_pct),
                analysis_timings::key_detect_ms.eq(row.key_detect_ms.map(|v| v as i64)),
                analysis_timings::separation_ms.eq(row.separation_ms.map(|v| v as i64)),
                analysis_timings::transcribe_ms.eq(row.transcribe_ms.map(|v| v as i64)),
                analysis_timings::align_ms.eq(row.align_ms.map(|v| v as i64)),
                analysis_timings::load_avg_1m.eq(row.load_avg_1m),
                analysis_timings::gpu_active_ratio.eq(row.gpu_active_ratio),
                analysis_timings::gpu_freq_mhz.eq(row.gpu_freq_mhz),
                analysis_timings::gpu_temp_c.eq(row.gpu_temp_c),
                analysis_timings::cpu_active_ratio.eq(row.cpu_active_ratio),
                analysis_timings::mem_pressure_ratio.eq(row.mem_pressure_ratio),
                analysis_timings::total_ms.eq(row.total_ms as i64),
            ))
            .execute(conn)?;
        Ok(())
    })
}
