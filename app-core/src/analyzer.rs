use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use ts_rs::TS;

use crate::cache::{CacheDir, models_dir};
use crate::config::AppConfig;
use crate::error::NightingaleError;
use crate::library_db;
use crate::library_model::LibraryMenuFilters;
use crate::lyrics::{fetch_lrclib_lyrics, local_lyrics_path, write_lyrics_file};
use crate::song::{Song, SongOrigin, TranscriptSource, compute_file_hash, read_transcript_meta};
use crate::source::active_source;

// ─── Analysis queue (persisted to disk) ──────────────────────────────

/// Coarse cause for a `QueuedStatus::Failed`, so the UI can group failures
/// without pattern-matching the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum FailureKind {
    /// Couldn't convert/fetch the audio into a form analysis can read.
    AudioPrep,
    /// The analyzer server process failed to start.
    ServerStartup,
    /// The analyzer server crashed mid-analysis (after a retry).
    ServerCrash,
    /// CUDA ran out of GPU memory (after a retry).
    GpuOom,
    /// The whisperx worker itself reported an error for this song.
    Worker,
    /// The pipeline reported success but no transcript file was produced.
    MissingOutput,
    /// Rows written before `FailureKind` existed; kind wasn't recorded.
    Other,
}

impl FailureKind {
    fn as_db_str(self) -> &'static str {
        match self {
            FailureKind::AudioPrep => "audio_prep",
            FailureKind::ServerStartup => "server_startup",
            FailureKind::ServerCrash => "server_crash",
            FailureKind::GpuOom => "gpu_oom",
            FailureKind::Worker => "worker",
            FailureKind::MissingOutput => "missing_output",
            FailureKind::Other => "other",
        }
    }

    fn from_db_str(s: &str) -> Self {
        match s {
            "audio_prep" => FailureKind::AudioPrep,
            "server_startup" => FailureKind::ServerStartup,
            "server_crash" => FailureKind::ServerCrash,
            "gpu_oom" => FailureKind::GpuOom,
            "worker" => FailureKind::Worker,
            "missing_output" => FailureKind::MissingOutput,
            _ => FailureKind::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum QueuedStatus {
    Queued,
    Analyzing(usize),
    Failed {
        kind: FailureKind,
        message: String,
        /// Set via `acknowledge_failures`; resets to false on every new failure.
        acknowledged: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct AnalysisQueue {
    pub entries: HashMap<String, QueuedStatus>,
}

impl AnalysisQueue {
    pub fn load() -> Self {
        let entries = library_db::analysis_queue_load_rows()
            .map(|rows| {
                rows.into_iter()
                    .map(|(h, st, pct, msg, kind, acknowledged)| {
                        let status = match st.as_str() {
                            "queued" => QueuedStatus::Queued,
                            "analyzing" => QueuedStatus::Analyzing(pct.unwrap_or(0) as usize),
                            "failed" => QueuedStatus::Failed {
                                kind: kind.as_deref().map(FailureKind::from_db_str).unwrap_or(FailureKind::Other),
                                message: msg.unwrap_or_default(),
                                acknowledged,
                            },
                            _ => QueuedStatus::Queued,
                        };
                        (h, status)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { entries }
    }

    pub fn save(&self) {
        let rows: Vec<_> = self
            .entries
            .iter()
            .map(|(k, v)| match v {
                QueuedStatus::Queued => (k.clone(), "queued".to_string(), None, None, None, false),
                QueuedStatus::Analyzing(p) => {
                    (k.clone(), "analyzing".to_string(), Some(*p as i64), None, None, false)
                }
                QueuedStatus::Failed { kind, message, acknowledged } => (
                    k.clone(),
                    "failed".to_string(),
                    None,
                    Some(message.clone()),
                    Some(kind.as_db_str().to_string()),
                    *acknowledged,
                ),
            })
            .collect();
        let _ = library_db::analysis_queue_save_rows(&rows);
    }

    pub fn clear() {
        let _ = library_db::analysis_queue_clear();
    }
}

/// Acknowledges exactly `file_hashes` as `kind` failures, so a failure that
/// lands after the caller's snapshot isn't swept in too.
pub fn acknowledge_failures(kind: FailureKind, file_hashes: Vec<String>) {
    let _ = library_db::analysis_queue_acknowledge_failures(kind.as_db_str(), &file_hashes);
}
use crate::vendor::{analyzer_dir, ffmpeg_path, python_path, silent_command};

// ─── Server process ──────────────────────────────────────────────────

static SERVER_PID: AtomicU32 = AtomicU32::new(0);

/// Device string from the most recent server handshake (e.g. "cuda", "mps",
/// "cpu"); recorded for the analysis-timing log.
static LAST_DEVICE: Mutex<Option<String>> = Mutex::new(None);

fn last_device() -> Option<String> {
    LAST_DEVICE.lock().unwrap().clone()
}

/// 1-minute system load average via `sysctl vm.loadavg` (e.g. `{ 7.71 5.24
/// 4.50 }`) -- a no-`sudo` proxy for other processes (Plex transcodes,
/// downloads, ...) competing for CPU/GPU while separation runs.
fn load_avg_1m() -> Option<f64> {
    let output = Command::new("sysctl").args(["-n", "vm.loadavg"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .find_map(|tok| tok.parse::<f64>().ok())
}

/// One system sample (GPU utilization/clock/temp, CPU utilization, memory
/// pressure) via `macmon pipe -s 1` (https://github.com/vladkens/macmon).
/// Unlike `powermetrics`, it needs no `sudo`, so it's safe to shell out to
/// on every analysis run. Every field is `None` -- not an error -- if
/// `macmon` isn't installed, isn't on `PATH`, or its output doesn't parse as
/// expected; this is a best-effort diagnostic and must never block or fail
/// an analysis run.
struct MacmonSnapshot {
    gpu_active_ratio: Option<f64>,
    gpu_freq_mhz: Option<i64>,
    gpu_temp_c: Option<f64>,
    cpu_active_ratio: Option<f64>,
    /// Fraction of physical RAM in use (`memory.ram_usage / ram_total`), as
    /// a proxy for memory pressure -- macmon doesn't surface macOS's actual
    /// Normal/Warn/Critical pressure level, only raw usage.
    mem_pressure_ratio: Option<f64>,
}

fn macmon_snapshot() -> Option<MacmonSnapshot> {
    let output = Command::new("macmon")
        .args(["pipe", "-s", "1", "-i", "200"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let mem = json.get("memory");
    let ram_usage = mem.and_then(|m| m.get("ram_usage")).and_then(|v| v.as_f64());
    let ram_total = mem.and_then(|m| m.get("ram_total")).and_then(|v| v.as_f64());
    Some(MacmonSnapshot {
        gpu_active_ratio: json.get("gpu_active_ratio").and_then(|v| v.as_f64()),
        gpu_freq_mhz: json.get("gpu_freq_mhz").and_then(|v| v.as_i64()),
        gpu_temp_c: json
            .get("temp")
            .and_then(|t| t.get("gpu_temp_avg"))
            .and_then(|v| v.as_f64()),
        cpu_active_ratio: json.get("cpu_active_ratio").and_then(|v| v.as_f64()),
        mem_pressure_ratio: match (ram_usage, ram_total) {
            (Some(used), Some(total)) if total > 0.0 => Some(used / total),
            _ => None,
        },
    })
}

/// `load_avg_1m()` + `macmon_snapshot()`, bundled as the one contention
/// reading taken for a song's analysis-timing row.
struct ContentionSnapshot {
    load_avg_1m: Option<f64>,
    macmon: Option<MacmonSnapshot>,
}

/// How far into the separation stage to sample `ContentionSnapshot`. A
/// snapshot taken at attempt-start (the previous approach) mostly caught the
/// GPU idle -- separation hadn't ramped up yet -- so `gpu_active_ratio` read
/// 0 on well over half of sampled songs even though separation is the stage
/// that actually drives the GPU. Waiting until separation has been running
/// for a while gives a reading that reflects what's actually contending for
/// the GPU/CPU during the expensive part of the pipeline.
const SEPARATION_SNAPSHOT_DELAY: Duration = Duration::from_secs(120);

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60);

struct ServerProcess {
    child: Child,
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let pid = self.child.id();
        info!("[analyzer] Killing server process (pid={pid})");
        SERVER_PID.store(0, Ordering::SeqCst);
        if let Ok(stream) = self.writer.get_ref().try_clone() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static ANALYZER_SERVER: LazyLock<Mutex<Option<ServerProcess>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Deserialize)]
struct ReadyHandshake {
    port: u16,
    token: String,
    #[serde(default)]
    device: Option<String>,
}

fn drain_lines_to_log<R: BufRead + Send + 'static>(mut reader: R, label: &'static str) {
    std::thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty() {
                        info!("[analyzer {label}] {trimmed}");
                    }
                }
            }
        }
    });
}

fn read_ready_handshake<R: BufRead>(reader: &mut R) -> Result<ReadyHandshake, NightingaleError> {
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Err(NightingaleError::Other(
                "Analyzer server exited before handshake".into(),
            ));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(value) if value.get("event").and_then(|v| v.as_str()) == Some("ready") => {
                return serde_json::from_value::<ReadyHandshake>(value).map_err(|e| {
                    NightingaleError::Other(format!("Malformed ready handshake: {e}"))
                });
            }
            _ => {
                info!("[analyzer stdout] {trimmed}");
            }
        }
    }
}

fn connect_and_authenticate(
    port: u16,
    token: &str,
) -> Result<(BufReader<TcpStream>, BufWriter<TcpStream>), NightingaleError> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let stream = TcpStream::connect_timeout(&addr, HANDSHAKE_TIMEOUT).map_err(|e| {
        NightingaleError::Other(format!("Failed to connect to analyzer server: {e}"))
    })?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;

    let writer_stream = stream
        .try_clone()
        .map_err(|e| NightingaleError::Other(format!("Failed to clone analyzer socket: {e}")))?;
    let mut reader = BufReader::new(stream);
    let mut writer = BufWriter::new(writer_stream);

    let hello = serde_json::json!({"type": "hello", "token": token});
    writer.write_all(serde_json::to_string(&hello).unwrap().as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;

    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
        return Err(NightingaleError::Other(
            "Analyzer server closed connection during handshake".into(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(line.trim())?;
    if value.get("type").and_then(|v| v.as_str()) != Some("hello_ack") {
        return Err(NightingaleError::Other(format!(
            "Analyzer auth failed: {}",
            line.trim()
        )));
    }

    reader.get_ref().set_read_timeout(None)?;
    reader.get_ref().set_write_timeout(None)?;

    Ok((reader, writer))
}

fn spawn_server() -> Result<ServerProcess, NightingaleError> {
    let python = python_path();
    let script = analyzer_dir().join("server.py");
    let models = models_dir();
    let ffmpeg = ffmpeg_path();
    let ffmpeg_dir = ffmpeg.parent().unwrap_or(std::path::Path::new("."));
    let path_env = if let Some(existing) = std::env::var_os("PATH") {
        let mut paths = std::env::split_paths(&existing).collect::<Vec<_>>();
        paths.insert(0, ffmpeg_dir.to_path_buf());
        std::env::join_paths(paths).unwrap_or(existing)
    } else {
        ffmpeg_dir.as_os_str().to_os_string()
    };

    let mut cmd = silent_command(&python);
    cmd.env("PATH", &path_env)
        .env("TORCH_HOME", models.join("torch"))
        .env("HF_HOME", models.join("huggingface"))
        .env("FFMPEG_PATH", &ffmpeg)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONWARNINGS", "ignore")
        .env("PYTORCH_ENABLE_MPS_FALLBACK", "1")
        .env("PYTORCH_CUDA_ALLOC_CONF", "expandable_segments:True")
        .env("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
        .env("NLTK_DATA", models.join("nltk_data"))
        .env("NEMO_CACHE_DIR", models.join("nemo"))
        .env("ONNX_ASR_CACHE_DIR", models.join("onnx_asr"))
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| NightingaleError::Other(format!("Failed to start analyzer server: {e}")))?;
    let pid = child.id();
    SERVER_PID.store(pid, Ordering::SeqCst);
    info!("[analyzer] Server process spawned (pid={pid})");

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            SERVER_PID.store(0, Ordering::SeqCst);
            return Err(NightingaleError::Other(
                "Failed to capture server stdout".into(),
            ));
        }
    };
    let mut stdout_reader = BufReader::new(stdout);

    let handshake = match read_ready_handshake(&mut stdout_reader) {
        Ok(h) => h,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            SERVER_PID.store(0, Ordering::SeqCst);
            return Err(e);
        }
    };
    if let Some(device) = handshake.device.as_deref() {
        info!(
            "[analyzer] Handshake ok: device={device} port={}",
            handshake.port
        );
    } else {
        info!("[analyzer] Handshake ok: port={}", handshake.port);
    }
    *LAST_DEVICE.lock().unwrap() = handshake.device.clone();

    let (reader, writer) = match connect_and_authenticate(handshake.port, &handshake.token) {
        Ok(pair) => pair,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            SERVER_PID.store(0, Ordering::SeqCst);
            return Err(e);
        }
    };

    drain_lines_to_log(stdout_reader, "stdout");
    if let Some(stderr) = child.stderr.take() {
        drain_lines_to_log(BufReader::new(stderr), "stderr");
    }

    Ok(ServerProcess {
        child,
        reader,
        writer,
    })
}

fn ensure_server(
    guard: &mut std::sync::MutexGuard<Option<ServerProcess>>,
) -> Result<(), NightingaleError> {
    if guard.is_some() {
        return Ok(());
    }
    let server = spawn_server()?;
    **guard = Some(server);
    Ok(())
}

// ─── Queue state ─────────────────────────────────────────────────────

struct AnalyzerState {
    queue: VecDeque<String>,
    active_hash: Option<String>,
    worker_running: bool,
}

static ANALYZER: LazyLock<Mutex<AnalyzerState>> = LazyLock::new(|| {
    Mutex::new(AnalyzerState {
        queue: VecDeque::new(),
        active_hash: None,
        worker_running: false,
    })
});

static FORCE_TRANSCRIBE: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Hashes whose queued job should only run stem separation (key detect +
/// separation) and keep the already-written LRC-provided transcript.
static STEMS_ONLY: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Mark a hash so its next analysis pass separates stems without transcribing,
/// preserving the transcript built from provided LRC.
pub fn mark_stems_only(file_hash: &str) {
    STEMS_ONLY.lock().unwrap().insert(file_hash.to_string());
}

// ─── Helpers ─────────────────────────────────────────────────────────

fn update_queue_status(file_hash: &str, status: QueuedStatus) {
    let (st, pct, msg, kind, acknowledged) = match &status {
        QueuedStatus::Queued => ("queued", None, None::<String>, None::<&'static str>, false),
        QueuedStatus::Analyzing(p) => ("analyzing", Some(*p as i64), None, None, false),
        QueuedStatus::Failed { kind, message, acknowledged } => {
            ("failed", None, Some(message.clone()), Some(kind.as_db_str()), *acknowledged)
        }
    };
    let _ = library_db::analysis_queue_upsert_row(file_hash, st, pct, msg.as_deref(), kind, acknowledged);
}

fn remove_from_queue(file_hash: &str) {
    let _ = library_db::analysis_queue_delete(file_hash);
}

/// Single-song counterpart to `remove_from_queue_all`, same pairing as
/// `enqueue_one`/`enqueue_all` (as opposed to `remove_from_queue`, called
/// internally once analysis finishes): also has to drop the hash from
/// `ANALYZER.queue`, or the worker would just pick it back up despite the
/// row being gone.
pub fn remove_from_queue_one(file_hash: &str) {
    let mut state = ANALYZER.lock().unwrap();
    state.queue.retain(|h| h != file_hash);
    drop(state);
    remove_from_queue(file_hash);
}

pub(crate) fn update_song_analyzed(
    file_hash: &str,
    is_analyzed: bool,
    language: Option<String>,
    transcript_source: Option<TranscriptSource>,
    key: Option<String>,
    tempo: Option<f64>,
) {
    let Some(mut song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        return;
    };
    song.is_analyzed = is_analyzed;
    song.language = language;
    song.transcript_source = transcript_source;
    if is_analyzed {
        song.key = key;
        if let Some(value) = tempo {
            song.tempo = value;
        }
        // LRC-provided songs without stem separation are flagged in the
        // transcript; mirror that onto the song so playback hides the guide.
        song.no_stems = read_transcript_meta(&CacheDir::new(), file_hash).no_stems;
    } else {
        song.key = None;
        song.override_key = None;
        song.tempo = 1.0;
        song.key_offset = 0;
        song.no_stems = false;
    }
    let _ = library_db::update_song_fields(file_hash, &song);
}

fn ensure_worker_running(state: &mut AnalyzerState) {
    if !state.worker_running && !state.queue.is_empty() {
        state.worker_running = true;
        spawn_worker();
    }
}

// ─── Public API ──────────────────────────────────────────────────────

pub(crate) fn is_usdx_song(file_hash: &str) -> bool {
    library_db::load_song_by_hash(file_hash)
        .ok()
        .flatten()
        .map(|s| s.usdx.is_some())
        .unwrap_or(false)
}

pub fn enqueue_one(file_hash: &str) {
    if is_usdx_song(file_hash) {
        return;
    }
    let mut state = ANALYZER.lock().unwrap();
    if state.active_hash.as_deref() == Some(file_hash) {
        return;
    }
    if !state.queue.iter().any(|h| h == file_hash) {
        state.queue.push_back(file_hash.to_string());
        update_queue_status(file_hash, QueuedStatus::Queued);
    }
    ensure_worker_running(&mut state);
}

pub fn enqueue_all(filters: &LibraryMenuFilters) {
    let queue = AnalysisQueue::load();
    let mut state = ANALYZER.lock().unwrap();

    let pending_hashes =
        library_db::iter_file_hashes_filtered_not_analyzed(filters).unwrap_or_default();

    let mut newly_queued = Vec::new();
    for file_hash in pending_hashes {
        let dominated = !queue.entries.contains_key(&file_hash);
        if dominated
            && state.active_hash.as_deref() != Some(&file_hash)
            && !state.queue.iter().any(|h| h == &file_hash)
        {
            state.queue.push_back(file_hash.clone());
            newly_queued.push(file_hash);
        }
    }

    let should_start = !state.worker_running && !state.queue.is_empty();
    if should_start {
        state.worker_running = true;
    }
    drop(state);

    for hash in &newly_queued {
        let _ = library_db::analysis_queue_upsert_row(hash, "queued", None, None, None, false);
    }

    if should_start {
        spawn_worker();
    }
}

pub fn shutdown_server() {
    let pid = SERVER_PID.swap(0, Ordering::SeqCst);
    if pid != 0 {
        info!("[analyzer] Graceful shutdown of server (pid={pid})");
        if let Ok(mut guard) = ANALYZER_SERVER.try_lock() {
            if let Some(server) = guard.as_mut() {
                let _ = server.writer.write_all(b"{\"type\":\"quit\"}\n");
                let _ = server.writer.flush();
            }
        }
        std::thread::spawn(move || {
            let _ = Command::new("kill").args([&pid.to_string()]).status();
            std::thread::sleep(std::time::Duration::from_secs(3));
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        });
    }
}

pub fn delete_cache(file_hash: &str) {
    if is_usdx_song(file_hash) {
        return;
    }
    let cache = CacheDir::new();
    cache.delete_song_cache(file_hash);
    update_song_analyzed(file_hash, false, None, None, None, None);
}

pub fn reanalyze_transcript(file_hash: &str, language: Option<String>) {
    if is_usdx_song(file_hash) {
        return;
    }

    if let Some(lang) = language {
        if !lang.is_empty() {
            let mut config = AppConfig::load();
            config.set_language_override(file_hash.to_string(), lang);
            config.save();
        }
    }
    reanalyze(file_hash, false);
}

pub fn reanalyze_full(file_hash: &str) {
    if is_usdx_song(file_hash) {
        return;
    }

    reanalyze(file_hash, true);
}

pub fn realign(file_hash: &str, language: Option<String>) {
    if is_usdx_song(file_hash) {
        return;
    }

    if let Some(lang) = language.as_ref().filter(|lang| !lang.is_empty()) {
        let mut config = AppConfig::load();
        config.set_language_override(file_hash.to_string(), lang.clone());
        config.save();
    }

    let cache = CacheDir::new();
    let previous_language = library_db::load_song_by_hash(file_hash)
        .ok()
        .flatten()
        .and_then(|song| song.language);
    materialize_lyrics_from_transcript(&cache, file_hash);
    let _ = std::fs::remove_file(cache.transcript_path(file_hash));
    cache.delete_transcript_variants(file_hash);
    update_song_analyzed(
        file_hash,
        false,
        language.or(previous_language),
        None,
        None,
        None,
    );
    enqueue_one(file_hash);
}

pub fn reanalyze_force_transcribe(file_hash: &str) {
    if is_usdx_song(file_hash) {
        return;
    }

    FORCE_TRANSCRIBE
        .lock()
        .unwrap()
        .insert(file_hash.to_string());

    reanalyze(file_hash, false);
}

/// Bulk "Full reanalysis": every already-analyzed, non-USDX song matching
/// `filters` (see `library_db::iter_file_hashes_filtered_full_reanalyzable`)
/// -- songs that aren't yet analyzed at all are already covered by
/// `enqueue_all`, and USDX songs never support reanalysis (see
/// `reanalyze_full`'s own guard). Reuses `reanalyze_full` per song rather
/// than duplicating its logic; safe to call in a loop since it only marks
/// the song unanalyzed and pushes onto the single-worker queue via
/// `enqueue_one` -- it doesn't spawn any work itself. Returns how many
/// songs were queued.
pub fn reanalyze_all_full(filters: &LibraryMenuFilters) -> usize {
    let hashes = library_db::iter_file_hashes_filtered_full_reanalyzable(filters).unwrap_or_default();
    for hash in &hashes {
        reanalyze_full(hash);
    }
    hashes.len()
}

/// Bulk "Refetch lyrics & align" -- see `iter_file_hashes_filtered_realignable`
/// for eligibility. `language` is `Some` only when called from the bulk
/// "Change language" flow (mode = force); `None` for the plain refetch
/// action, matching `reanalyze_transcript`'s own per-song signature.
pub fn reanalyze_all_transcript(filters: &LibraryMenuFilters, language: Option<String>) -> usize {
    let hashes = library_db::iter_file_hashes_filtered_realignable(filters).unwrap_or_default();
    for hash in &hashes {
        reanalyze_transcript(hash, language.clone());
    }
    hashes.len()
}

/// Bulk "Force transcribe" -- see `iter_file_hashes_filtered_realignable`.
pub fn reanalyze_all_force_transcribe(filters: &LibraryMenuFilters) -> usize {
    let hashes = library_db::iter_file_hashes_filtered_realignable(filters).unwrap_or_default();
    for hash in &hashes {
        reanalyze_force_transcribe(hash);
    }
    hashes.len()
}

/// Bulk "Realign" -- see `iter_file_hashes_filtered_realignable`. `language`
/// is `Some` only from the bulk "Change language" flow (mode = realign).
pub fn realign_all(filters: &LibraryMenuFilters, language: Option<String>) -> usize {
    let hashes = library_db::iter_file_hashes_filtered_realignable(filters).unwrap_or_default();
    for hash in &hashes {
        realign(hash, language.clone());
    }
    hashes.len()
}

/// Bulk "Remove from queue" -- see `iter_file_hashes_filtered_queued` for
/// eligibility (excludes songs currently being analyzed). Unlike the other
/// bulk actions this doesn't enqueue further work; it's done synchronously
/// by the time it returns.
pub fn remove_from_queue_all(filters: &LibraryMenuFilters) -> usize {
    let hashes = library_db::iter_file_hashes_filtered_queued(filters).unwrap_or_default();
    for hash in &hashes {
        remove_from_queue_one(hash);
    }
    hashes.len()
}

/// "Refresh metadata": re-reads title/artist/album/duration/album art/
/// lyrics-source-flags straight from the song's file (see
/// `Song::refresh_metadata`) without touching anything analysis-derived --
/// unlike every other action in this file, it never calls `enqueue_one` or
/// marks the song unanalyzed. Exists mainly to recover from a cover-art (or
/// similar) cache file being deleted outside the app: a normal rescan only
/// ever re-derives these fields for brand-new paths (see
/// `source::folder::scan`'s `already_processed` filter), so an
/// already-known song's stale, now-broken `album_art_path` would otherwise
/// never get fixed. No-ops for remote-source/USDX songs (nothing local to
/// re-read); see `iter_file_hashes_filtered_refreshable`.
pub fn refresh_metadata(file_hash: &str) {
    let Some(mut song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        return;
    };
    if !matches!(song.origin, SongOrigin::LocalFile) || song.usdx.is_some() {
        return;
    }
    let cache = CacheDir::new();
    song.refresh_metadata(&cache);
    let _ = library_db::update_song_fields(file_hash, &song);
}

/// Bulk "Refresh metadata" -- see `iter_file_hashes_filtered_refreshable`
/// and `refresh_metadata`. Runs on a background thread: each song is a
/// separate `LIBRARY_DB` lock/unlock around blocking file I/O (tag reads,
/// cover writes), so doing this inline in the command handler for a whole
/// (potentially large) filtered set used to hold up the shared connection
/// for the entire batch, stalling every other command until it finished.
pub fn refresh_metadata_all(filters: &LibraryMenuFilters) -> usize {
    let hashes = library_db::iter_file_hashes_filtered_refreshable(filters).unwrap_or_default();
    let count = hashes.len();
    std::thread::spawn(move || {
        for hash in &hashes {
            refresh_metadata(hash);
        }
    });
    count
}

fn reanalyze(file_hash: &str, full: bool) {
    let cache = CacheDir::new();
    if full {
        cache.delete_song_cache(file_hash);
    } else {
        let _ = std::fs::remove_file(cache.transcript_path(file_hash));
        cache.delete_transcript_variants(file_hash);
        let _ = std::fs::remove_file(cache.lyrics_path(file_hash));
    }
    update_song_analyzed(file_hash, false, None, None, None, None);
    enqueue_one(file_hash);
}

fn materialize_lyrics_from_transcript(cache: &CacheDir, file_hash: &str) {
    if cache.lyrics_path(file_hash).is_file() {
        return;
    }

    let transcript_path = cache.transcript_path(file_hash);
    let Ok(data) = std::fs::read_to_string(&transcript_path) else {
        return;
    };

    #[derive(Deserialize)]
    struct Segment {
        #[serde(default)]
        text: String,
    }

    #[derive(Deserialize)]
    struct TranscriptShape {
        #[serde(default)]
        segments: Vec<Segment>,
    }

    let Ok(parsed) = serde_json::from_str::<TranscriptShape>(&data) else {
        return;
    };

    let lines: Vec<String> = parsed
        .segments
        .into_iter()
        .map(|s| s.text.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if lines.is_empty() {
        return;
    }

    if let Err(e) = write_lyrics_file(cache, file_hash, &lines) {
        warn!("[analyzer] Failed to materialize lyrics from transcript for {file_hash}: {e}");
    }
}

// ─── Worker ──────────────────────────────────────────────────────────

fn spawn_worker() {
    std::thread::spawn(|| {
        let cache = CacheDir::new();

        loop {
            let file_hash = {
                let mut state = ANALYZER.lock().unwrap();
                match state.queue.pop_front() {
                    Some(hash) => {
                        state.active_hash = Some(hash.clone());
                        hash
                    }
                    None => {
                        state.worker_running = false;
                        state.active_hash = None;
                        return;
                    }
                }
            };

            process_song(&file_hash, &cache);

            let mut state = ANALYZER.lock().unwrap();
            state.active_hash = None;
        }
    });
}

fn process_song(initial_hash: &str, cache: &CacheDir) {
    let Some(song) = library_db::load_song_by_hash(initial_hash).ok().flatten() else {
        warn!("[analyzer] Song with hash {initial_hash} not found in store, skipping");
        return;
    };

    let (song, local_path, file_hash_owned) = match prepare_audio_for_analysis(&song, cache) {
        Ok(out) => out,
        Err(e) => {
            warn!("[analyzer] Failed to prepare audio for analysis: {e}");
            update_queue_status(
                initial_hash,
                QueuedStatus::Failed {
                    kind: FailureKind::AudioPrep,
                    message: format!("audio prep failed: {e}"),
                    acknowledged: false,
                },
            );
            return;
        }
    };
    let file_hash = file_hash_owned.as_str();

    info!(
        "[analyzer] Starting analysis: {} (hash={})",
        local_path.display(),
        file_hash
    );

    update_queue_status(file_hash, QueuedStatus::Analyzing(0));

    // Stems-only: keep the LRC-provided transcript and just separate stems.
    // The intent may have been keyed by the pre-rekey hash for remote songs.
    let stems_only = {
        let mut set = STEMS_ONLY.lock().unwrap();
        set.remove(file_hash) || set.remove(initial_hash)
    };
    if stems_only && file_hash != initial_hash {
        // Move the pre-written transcript to the rekeyed hash so the pass can
        // patch it in place.
        let _ = std::fs::rename(
            cache.transcript_path(initial_hash),
            cache.transcript_path(file_hash),
        );
    }

    let config = AppConfig::load();
    let skip_lrclib = stems_only || FORCE_TRANSCRIBE.lock().unwrap().remove(file_hash);
    // Local lyrics (a `.lrc` sidecar or a tag embedded in the file itself)
    // take priority over the LRCLIB network lookup when the user has opted
    // in via `use_external_lyrics`: whichever is found first is the one
    // that lands in the shared lyrics cache, and the other check just sees
    // it already there (see local_lyrics_path's doc comment). Off, analysis
    // behaves exactly as before this setting existed -- LRCLIB, then ASR.
    let lyrics_path = if skip_lrclib {
        None
    } else if config.use_external_lyrics() {
        local_lyrics_path(&song, cache).or_else(|| fetch_lrclib_lyrics(&song, cache))
    } else {
        fetch_lrclib_lyrics(&song, cache)
    };

    let mut cmd_json = serde_json::json!({
        "type": "analyze",
        "audio_path": local_path.to_string_lossy(),
        "cache_path": cache.path.to_string_lossy(),
        "hash": file_hash,
        "model": config.whisper_model(),
        "beam_size": config.beam_size(),
        "batch_size": config.batch_size(),
        "separator": config.separator(),
        "engine": config.asr_engine(),
        "align_backend": config.align_backend(),
        "vocal_detection_threshold_pct": config.vocal_detection_threshold_pct(),
    });

    if stems_only {
        cmd_json["skip_transcription"] = serde_json::json!(true);
    }

    if let Some(ref lp) = lyrics_path {
        cmd_json["lyrics"] = serde_json::json!(lp.to_string_lossy());
    }
    let language_hint = config
        .language_override(file_hash)
        .map(str::to_string)
        .or_else(|| lyrics_path.as_ref().and_then(|_| song.language.clone()))
        .filter(|lang| {
            // "unknown"/empty is not a real language: passing it as a forced
            // alignment language crashes whisperx, so let the worker detect it.
            let normalized = lang.trim().to_ascii_lowercase();
            !normalized.is_empty() && normalized != "unknown" && normalized != "und"
        });
    if let Some(lang) = language_hint {
        cmd_json["language"] = serde_json::json!(lang);
    }

    let json_str = serde_json::to_string(&cmd_json).unwrap();
    let mut retried = false;

    loop {
        let mut guard = ANALYZER_SERVER.lock().unwrap();

        if let Err(e) = ensure_server(&mut guard) {
            warn!("[analyzer] Failed to start server: {e}");
            update_queue_status(
                file_hash,
                QueuedStatus::Failed {
                    kind: FailureKind::ServerStartup,
                    message: e.to_string(),
                    acknowledged: false,
                },
            );
            return;
        }

        let server = guard.as_mut().unwrap();
        let attempt_start = Instant::now();
        match send_and_monitor(server, &json_str, Some(file_hash)) {
            Ok(SongResult::Done(stage_timings, contention_snapshot)) => {
                let total_ms = attempt_start.elapsed().as_millis() as u64;
                info!("[analyzer:timing] hash={file_hash} stage=total ms={total_ms}");
                if config.track_analysis_timings() {
                    record_analysis_timing(
                        file_hash,
                        &config,
                        &stage_timings,
                        contention_snapshot,
                        total_ms,
                    );
                }
                finalize_song(file_hash, cache);
                return;
            }
            Ok(SongResult::Oom) => {
                warn!("[analyzer] CUDA OOM, killing server to free GPU memory");
                *guard = None;

                if !retried {
                    retried = true;
                    info!("[analyzer] Respawning server and retrying with clean GPU");
                    update_queue_status(file_hash, QueuedStatus::Analyzing(0));
                    continue;
                }
                update_queue_status(
                    file_hash,
                    QueuedStatus::Failed {
                        kind: FailureKind::GpuOom,
                        message: "CUDA out of memory".into(),
                        acknowledged: false,
                    },
                );
                return;
            }
            Ok(SongResult::Error(msg)) => {
                update_queue_status(
                    file_hash,
                    QueuedStatus::Failed {
                        kind: FailureKind::Worker,
                        message: msg,
                        acknowledged: false,
                    },
                );
                return;
            }
            Err(e) => {
                warn!("[analyzer] Server crashed: {e}");
                *guard = None;

                if !retried {
                    retried = true;
                    info!("[analyzer] Respawning server and retrying");
                    update_queue_status(file_hash, QueuedStatus::Analyzing(0));
                    continue;
                }
                update_queue_status(
                    file_hash,
                    QueuedStatus::Failed {
                        kind: FailureKind::ServerCrash,
                        message: format!("Server crashed: {e}"),
                        acknowledged: false,
                    },
                );
                return;
            }
        }
    }
}

fn finalize_song(file_hash: &str, cache: &CacheDir) {
    if cache.transcript_exists(file_hash) {
        if let Err(err) = crate::playback::ensure_playable_source_video(file_hash) {
            warn!("[analyzer] Playable source-video conversion failed for {file_hash}: {err}");
        }
        let meta = read_transcript_meta(cache, file_hash);
        remove_from_queue(file_hash);
        update_song_analyzed(
            file_hash,
            true,
            meta.language,
            Some(meta.source),
            meta.key,
            Some(meta.tempo),
        );
        info!("[analyzer] Analysis complete for {file_hash}");
    } else {
        update_queue_status(
            file_hash,
            QueuedStatus::Failed {
                kind: FailureKind::MissingOutput,
                message: "Transcript file not found after analysis".into(),
                acknowledged: false,
            },
        );
    }
}

// ─── LRC (play-original) preparation ─────────────────────────────────

/// Prepare an LRC-provided song that plays over its original mix, without
/// routing it through the analysis status queue.
///
/// The analyzer-free work runs synchronously so the song is immediately
/// playable: materialize the audio, rekey remote rows to the content hash, and
/// mark the song ready (source=Lrc, no_stems). None of this touches the
/// analyzer server, so it never stalls behind a running analysis.
///
/// The musical key is then detected on a background thread (which contends on
/// the analyzer server) and patched in once it lands, so the key/tempo controls
/// unlock later without blocking playback.
pub fn prepare_lrc_no_stems(file_hash: &str) -> Result<(), NightingaleError> {
    let cache = CacheDir::new();
    let Some(song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        return Err(NightingaleError::Other("Song not found".into()));
    };

    // Materialize the audio and, for remote sources, rekey the row to the
    // content hash so all downstream cache files follow the usual layout.
    let (mut song, local_path, real_hash) = prepare_audio_for_analysis(&song, &cache)?;
    let real_hash = real_hash.to_string();

    // A rekey moves the row — carry the transcript we wrote under the original
    // hash across so the key pass can patch it in place.
    if real_hash != file_hash {
        let _ = std::fs::rename(
            cache.transcript_path(file_hash),
            cache.transcript_path(&real_hash),
        );
    }

    // Mark ready right away (key still unknown) so playback over the original
    // mix is available immediately, before the key detection runs.
    song.is_analyzed = true;
    song.transcript_source = Some(TranscriptSource::Lrc);
    song.key = None;
    song.override_key = None;
    song.tempo = 1.0;
    song.key_offset = 0;
    song.no_stems = true;
    library_db::update_song_fields(&real_hash, &song)
        .map_err(|e| NightingaleError::Other(e.to_string()))?;
    let _ = crate::playback::ensure_playable_source_video(&real_hash);

    // Detect the key off-queue in the background; patch it onto the row once it
    // lands so the key/tempo shift controls unlock without blocking playback.
    std::thread::spawn(move || {
        let cache = CacheDir::new();
        if let Err(e) = run_key_pass(&cache, &local_path, &real_hash) {
            warn!("[analyzer] LRC key detection failed for {real_hash}: {e}");
            return;
        }
        let meta = read_transcript_meta(&cache, &real_hash);
        if let Some(mut updated) = library_db::load_song_by_hash(&real_hash).ok().flatten() {
            updated.key = meta.key;
            let _ = library_db::update_song_fields(&real_hash, &updated);
        }
        info!("[analyzer] LRC key detection complete for {real_hash}");
    });
    Ok(())
}

/// Run a key-only analysis pass (no transcription, no stem separation) against
/// the running analyzer server, keeping it off the status queue. On success the
/// detected key is patched into the existing transcript by the pipeline.
fn run_key_pass(
    cache: &CacheDir,
    local_path: &Path,
    file_hash: &str,
) -> Result<(), NightingaleError> {
    let config = AppConfig::load();
    let cmd_json = serde_json::json!({
        "type": "analyze",
        "audio_path": local_path.to_string_lossy(),
        "cache_path": cache.path.to_string_lossy(),
        "hash": file_hash,
        "model": config.whisper_model(),
        "beam_size": config.beam_size(),
        "batch_size": config.batch_size(),
        "separator": config.separator(),
        "engine": config.asr_engine(),
        "align_backend": config.align_backend(),
        "vocal_detection_threshold_pct": config.vocal_detection_threshold_pct(),
        // Key only: keep the provided LRC transcript and the original mix.
        "skip_transcription": true,
        "skip_separation": true,
    });
    let json_str = serde_json::to_string(&cmd_json).unwrap();

    let mut retried = false;
    loop {
        let mut guard = ANALYZER_SERVER.lock().unwrap();
        ensure_server(&mut guard)?;
        let server = guard.as_mut().unwrap();
        // `None` progress hash keeps this off the status pipe (no queue rows).
        match send_and_monitor(server, &json_str, None) {
            Ok(SongResult::Done(..)) => return Ok(()),
            Ok(SongResult::Oom) | Err(_) => {
                *guard = None;
                if !retried {
                    retried = true;
                    continue;
                }
                return Err(NightingaleError::Other("key detection failed".into()));
            }
            Ok(SongResult::Error(msg)) => {
                return Err(NightingaleError::Other(msg));
            }
        }
    }
}

// ─── Audio materialization for non-local sources ─────────────────────

/// Make sure the song's audio is present on disk and the row is keyed by the
/// true Blake3 hash before analysis kicks off. For `LocalFile` songs this is a
/// no-op. For Jellyfin songs we download once into `cache/sources/<hash>.<ext>`
/// then rekey the DB row + analysis queue from the placeholder id-hash to the
/// content hash so all downstream cache files (`<hash>_instrumental.mp3` etc.)
/// follow the existing convention.
fn prepare_audio_for_analysis(
    song: &Song,
    cache: &CacheDir,
) -> Result<(Song, PathBuf, String), NightingaleError> {
    match &song.origin {
        SongOrigin::LocalFile => Ok((song.clone(), song.path.clone(), song.file_hash.clone())),
        // Both remote origins go through the active source's
        // `ensure_local_media` and then get rekeyed to the true Blake3 hash.
        SongOrigin::Jellyfin { .. } | SongOrigin::Navidrome { .. } | SongOrigin::Plex { .. } => {
            let source = active_source()?
                .ok_or_else(|| NightingaleError::Other("no active library source".into()))?;
            let downloaded_path = source.ensure_local_media(song, cache)?;

            let real_hash = compute_file_hash(&downloaded_path)?;
            if real_hash == song.file_hash {
                return Ok((song.clone(), downloaded_path, song.file_hash.clone()));
            }

            let ext = downloaded_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("bin");
            let new_source_path = cache
                .path
                .join("sources")
                .join(format!("{real_hash}.{ext}"));

            if new_source_path != downloaded_path {
                if let Some(parent) = new_source_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if new_source_path.is_file() {
                    let _ = std::fs::remove_file(&downloaded_path);
                } else {
                    std::fs::rename(&downloaded_path, &new_source_path)?;
                }
            }

            let mut updated = song.clone();
            updated.file_hash = real_hash.clone();
            updated.path = new_source_path.clone();

            library_db::rekey_song(&song.file_hash, &real_hash, &updated).map_err(|e| {
                NightingaleError::Other(format!("failed to rekey remote song: {e}"))
            })?;

            Ok((updated, new_source_path, real_hash))
        }
    }
}

// ─── Server communication ────────────────────────────────────────────

enum SongResult {
    Done(Vec<(String, u64)>, Option<ContentionSnapshot>),
    Oom,
    Error(String),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerEvent {
    Progress {
        pct: u32,
        #[serde(default)]
        msg: String,
    },
    Starting {
        stage: String,
    },
    Timing {
        stage: String,
        ms: u64,
    },
    Done,
    Error {
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        msg: String,
    },
    #[serde(other)]
    Unknown,
}

/// Persist one analysis run's per-stage timings + the settings that produced
/// them, gated on `AppConfig::track_analysis_timings`.
fn record_analysis_timing(
    file_hash: &str,
    config: &AppConfig,
    stage_timings: &[(String, u64)],
    contention_snapshot: Option<ContentionSnapshot>,
    total_ms: u64,
) {
    let stage_ms = |name: &str| -> Option<u64> {
        stage_timings
            .iter()
            .find(|(stage, _)| stage == name)
            .map(|(_, ms)| *ms)
    };
    let device = last_device();
    let macmon = contention_snapshot.as_ref().and_then(|s| s.macmon.as_ref());
    let row = library_db::AnalysisTimingRow {
        file_hash,
        device: device.as_deref(),
        whisper_model: config.whisper_model(),
        beam_size: config.beam_size(),
        batch_size: config.batch_size(),
        separator: config.separator(),
        asr_engine: config.asr_engine(),
        align_backend: config.align_backend(),
        vocal_detection_threshold_pct: config.vocal_detection_threshold_pct(),
        key_detect_ms: stage_ms("key_detect"),
        separation_ms: stage_ms("separation"),
        transcribe_ms: stage_ms("transcribe"),
        align_ms: stage_ms("align"),
        load_avg_1m: contention_snapshot.as_ref().and_then(|s| s.load_avg_1m),
        gpu_active_ratio: macmon.and_then(|m| m.gpu_active_ratio),
        gpu_freq_mhz: macmon.and_then(|m| m.gpu_freq_mhz),
        gpu_temp_c: macmon.and_then(|m| m.gpu_temp_c),
        cpu_active_ratio: macmon.and_then(|m| m.cpu_active_ratio),
        mem_pressure_ratio: macmon.and_then(|m| m.mem_pressure_ratio),
        total_ms,
    };
    if let Err(e) = library_db::insert_analysis_timing(&row) {
        warn!("[analyzer] Failed to record analysis timing: {e}");
    }
}

fn send_and_monitor(
    server: &mut ServerProcess,
    json_cmd: &str,
    progress_hash: Option<&str>,
) -> Result<SongResult, NightingaleError> {
    server.writer.write_all(json_cmd.as_bytes())?;
    server.writer.write_all(b"\n")?;
    server.writer.flush()?;

    let mut line_buf = String::new();
    let mut stage_timings: Vec<(String, u64)> = Vec::new();
    // Set once "starting" fires for the separation stage; polled once that
    // stage's "timing" event confirms it ran long enough for the delayed
    // sample to have fired (see `SEPARATION_SNAPSHOT_DELAY`).
    let mut separation_snapshot_rx: Option<mpsc::Receiver<ContentionSnapshot>> = None;
    let mut contention_snapshot: Option<ContentionSnapshot> = None;
    loop {
        line_buf.clear();
        let bytes = server.reader.read_line(&mut line_buf)?;

        if bytes == 0 {
            return Err("Server closed connection unexpectedly".into());
        }

        let line = line_buf.trim();
        if line.is_empty() {
            continue;
        }

        let event: ServerEvent = match serde_json::from_str(line) {
            Ok(ev) => ev,
            Err(e) => {
                warn!("[analyzer] Skipping unparseable event: {e}; line={line:?}");
                continue;
            }
        };

        match event {
            ServerEvent::Progress { pct, msg } => {
                if !msg.is_empty() {
                    info!("[analyzer] progress {pct}% {msg}");
                }
                if let Some(hash) = progress_hash {
                    update_queue_status(hash, QueuedStatus::Analyzing(pct as usize));
                }
            }
            ServerEvent::Starting { stage } => {
                if stage == "separation" {
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        std::thread::sleep(SEPARATION_SNAPSHOT_DELAY);
                        let _ = tx.send(ContentionSnapshot {
                            load_avg_1m: load_avg_1m(),
                            macmon: macmon_snapshot(),
                        });
                    });
                    separation_snapshot_rx = Some(rx);
                }
            }
            ServerEvent::Timing { stage, ms } => {
                match progress_hash {
                    Some(hash) => info!("[analyzer:timing] hash={hash} stage={stage} ms={ms}"),
                    None => info!("[analyzer:timing] stage={stage} ms={ms}"),
                }
                if stage == "separation" {
                    // Only collect the sample if separation actually ran
                    // long enough for it to have fired; otherwise leave the
                    // row's contention fields empty rather than reading a
                    // snapshot taken after separation already finished. By
                    // this point the delay has already elapsed in real time,
                    // so the recv is just picking up an already-sent value
                    // -- the timeout is a safety net, not an expected wait.
                    if let Some(rx) = separation_snapshot_rx.take() {
                        if ms >= SEPARATION_SNAPSHOT_DELAY.as_millis() as u64 {
                            contention_snapshot = rx.recv_timeout(Duration::from_secs(5)).ok();
                        }
                    }
                }
                stage_timings.push((stage, ms));
            }
            ServerEvent::Done { .. } => {
                return Ok(SongResult::Done(stage_timings, contention_snapshot));
            }
            ServerEvent::Error { kind, msg } => {
                let kind_s = kind.as_deref().unwrap_or("generic");
                if kind_s == "oom" {
                    return Ok(SongResult::Oom);
                }
                let msg = if msg.is_empty() {
                    "Unknown error".to_string()
                } else {
                    msg
                };
                return Ok(SongResult::Error(msg));
            }
            ServerEvent::Unknown => {
                warn!("[analyzer] Ignoring unknown event: {line}");
            }
        }
    }
}
