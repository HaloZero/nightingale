//! Peer-offload for the analysis queue ("parallel analysis"): when enabled
//! and pointed at another Nightingale instance's base URL, a dispatcher
//! drains the *back* of the local analysis queue (the local worker in
//! `analyzer.rs` drains the front) and hands those songs to the peer
//! instead, so two machines pointed at the same library can chew through a
//! backlog concurrently. See `analyzer::claim_from_back_excluding` for why
//! the two workers can't double-claim a hash.
//!
//! Every peer call goes through the same HTTP surface the browser client
//! uses (`/api/cmd/*`, `/media/<hash>/<kind>`) -- no separate protocol, no
//! auth beyond whatever already gates that port (matches the existing
//! self-hosted-server trust model: see `client/src-server`).

use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::analyzer::QueuedStatus;
use crate::cache::CacheDir;
use crate::config::AppConfig;
use crate::library_db;
use crate::song::{Song, read_transcript_meta};

/// How many times to poll the peer for completion, 60s apart (== 20 min).
const POLL_ATTEMPTS: u32 = 20;
const POLL_INTERVAL: Duration = Duration::from_secs(60);
/// How long to wait before re-checking a peer that failed a liveness check.
const BACKOFF_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Set when a liveness/connection check to the peer has failed; cleared once
/// the hourly backoff thread confirms it's back. Gates whether the
/// dispatcher is allowed to start a new run.
static PEER_DOWN: AtomicBool = AtomicBool::new(false);
/// Guards against spawning more than one hourly backoff thread at once.
static BACKOFF_RUNNING: AtomicBool = AtomicBool::new(false);

// ─── Public entry points ────────────────────────────────────────────────

/// Used by the "ping" button in settings: a fresh, synchronous liveness
/// check against the configured peer. `false` if no peer is configured.
pub fn manual_ping() -> bool {
    let config = AppConfig::load();
    let Some(url) = config.parallel_analysis_url() else {
        return false;
    };
    ping(url)
}

/// Starts the dispatcher thread if parallel analysis is enabled, a peer is
/// configured, the peer isn't known-down, and one isn't already running.
/// Called from the same places the local worker's `ensure_worker_running` is
/// (`enqueue_one`/`enqueue_all`), so it naturally starts whenever there's
/// fresh work and stops itself once the queue's drained. Also called
/// directly when the setting is toggled on (or the peer URL changes while
/// already on) so it doesn't wait for the next `enqueue_one`/`enqueue_all`.
pub fn ensure_dispatcher_running() {
    let config = AppConfig::load();
    if !config.parallel_analysis_enabled() {
        return;
    }
    if config.parallel_analysis_url().is_none() {
        return;
    }
    if PEER_DOWN.load(Ordering::SeqCst) {
        return;
    }
    if crate::analyzer::try_start_parallel_dispatcher() {
        std::thread::spawn(dispatcher_loop);
    }
}

// ─── Dispatcher ──────────────────────────────────────────────────────────

enum DispatchOutcome {
    /// Peer had (or produced) a finished analysis and results were pulled
    /// down successfully.
    Done,
    /// Peer doesn't have this song, or failed/timed out analyzing it, or the
    /// downloaded results were unusable -- handed back to the local queue;
    /// worth trying other queued songs against the peer.
    Rejected,
    /// Peer is unreachable -- handed back to the local queue, dispatcher
    /// stops entirely until the peer's confirmed alive again.
    PeerDown,
}

fn dispatcher_loop() {
    // Hashes this run has already had rejected/failed/timed-out by the peer
    // -- skipped on subsequent claims so one stuck song can't spin the loop.
    let mut skip: HashSet<String> = HashSet::new();

    loop {
        let config = AppConfig::load();
        if !config.parallel_analysis_enabled() {
            break;
        }
        let Some(base_url) = config.parallel_analysis_url().map(str::to_string) else {
            break;
        };

        let Some(file_hash) = crate::analyzer::claim_from_back_excluding(&skip) else {
            break;
        };

        match dispatch_one(&base_url, &file_hash) {
            DispatchOutcome::Done => {}
            DispatchOutcome::Rejected => {
                skip.insert(file_hash);
            }
            DispatchOutcome::PeerDown => {
                crate::analyzer::return_to_front(&file_hash);
                enter_down_backoff();
                break;
            }
        }
    }

    crate::analyzer::stop_parallel_dispatcher();
}

fn dispatch_one(base_url: &str, file_hash: &str) -> DispatchOutcome {
    if !ping(base_url) {
        return DispatchOutcome::PeerDown;
    }

    let Some(local_song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        // Song vanished locally (deleted mid-queue) -- nothing to hand off.
        return DispatchOutcome::Rejected;
    };

    let peer_song = match peer_song(base_url, file_hash) {
        Ok(song) => song,
        Err(PeerError::Unreachable) => return DispatchOutcome::PeerDown,
        Err(PeerError::Other) => None,
    };

    let Some(peer_song) = peer_song.filter(|s| s.path == local_song.path) else {
        crate::analyzer::return_to_front(file_hash);
        return DispatchOutcome::Rejected;
    };

    if !peer_song.is_analyzed {
        match trigger(base_url, file_hash) {
            Ok(()) => {}
            Err(PeerError::Unreachable) => return DispatchOutcome::PeerDown,
            Err(PeerError::Other) => {
                crate::analyzer::return_to_front(file_hash);
                return DispatchOutcome::Rejected;
            }
        }
        crate::analyzer::update_queue_status(file_hash, QueuedStatus::Analyzing(0));

        match poll_until_done(base_url, file_hash) {
            PollOutcome::Done => {}
            PollOutcome::GaveUp => {
                crate::analyzer::return_to_front(file_hash);
                return DispatchOutcome::Rejected;
            }
            PollOutcome::PeerDown => return DispatchOutcome::PeerDown,
        }
    }

    if fetch_results(base_url, file_hash) {
        crate::analyzer::finalize_peer_analysis(file_hash);
        DispatchOutcome::Done
    } else {
        crate::analyzer::return_to_front(file_hash);
        DispatchOutcome::Rejected
    }
}

enum PollOutcome {
    Done,
    GaveUp,
    PeerDown,
}

/// Polls the peer's queue every `POLL_INTERVAL` for up to `POLL_ATTEMPTS`
/// (== 20 minutes), mirroring reported progress onto the local queue row.
fn poll_until_done(base_url: &str, file_hash: &str) -> PollOutcome {
    for _ in 0..POLL_ATTEMPTS {
        std::thread::sleep(POLL_INTERVAL);

        if !AppConfig::load().parallel_analysis_enabled() {
            return PollOutcome::GaveUp;
        }

        match peer_queue_status(base_url, file_hash) {
            Ok(Some(QueuedStatus::Failed { message, .. })) => {
                warn!("[parallel_analysis] peer failed {file_hash}: {message}");
                return PollOutcome::GaveUp;
            }
            Ok(Some(QueuedStatus::Analyzing(pct))) => {
                crate::analyzer::update_queue_status(file_hash, QueuedStatus::Analyzing(pct));
            }
            Ok(Some(QueuedStatus::Queued)) => {}
            Ok(None) => {
                // No longer queued on the peer -- either it finished or it
                // was removed out from under us; check the song to tell.
                return match peer_song(base_url, file_hash) {
                    Ok(Some(song)) if song.is_analyzed => PollOutcome::Done,
                    Ok(_) => PollOutcome::GaveUp,
                    Err(PeerError::Unreachable) => PollOutcome::PeerDown,
                    Err(PeerError::Other) => PollOutcome::GaveUp,
                };
            }
            Err(PeerError::Unreachable) => return PollOutcome::PeerDown,
            Err(PeerError::Other) => {}
        }
    }
    PollOutcome::GaveUp
}

/// Copies the peer's finished transcript (and, unless the transcript marks
/// `no_stems`, the instrumental/vocals stems) plus any lyrics file into the
/// local cache, straight off `/media/<hash>/<kind>` (byte-for-byte, no JSON
/// round-trip). Returns whether a usable transcript landed locally.
fn fetch_results(base_url: &str, file_hash: &str) -> bool {
    let cache = CacheDir::new();

    if !download_to(
        base_url,
        file_hash,
        "transcript",
        &cache.transcript_path(file_hash),
    ) {
        return false;
    }

    let meta = read_transcript_meta(&cache, file_hash);
    if !meta.no_stems {
        let instrumental_ok = download_to(
            base_url,
            file_hash,
            "instrumental",
            &cache.instrumental_path(file_hash),
        );
        let vocals_ok = download_to(base_url, file_hash, "vocals", &cache.vocals_path(file_hash));
        if !instrumental_ok || !vocals_ok {
            return false;
        }
    }

    // Lyrics is optional/best-effort -- its absence on the peer isn't fatal.
    let _ = download_to(base_url, file_hash, "lyrics", &cache.lyrics_path(file_hash));

    true
}

// ─── Liveness backoff ────────────────────────────────────────────────────

/// Marks the peer down and, unless a backoff thread is already running,
/// spawns one that re-pings hourly until the peer's back (or the feature's
/// disabled/repointed), then resumes the dispatcher.
fn enter_down_backoff() {
    if BACKOFF_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    PEER_DOWN.store(true, Ordering::SeqCst);
    info!("[parallel_analysis] peer unreachable, pausing until it's back (rechecking hourly)");

    std::thread::spawn(|| {
        loop {
            std::thread::sleep(BACKOFF_INTERVAL);

            let config = AppConfig::load();
            let Some(url) = config
                .parallel_analysis_enabled()
                .then(|| config.parallel_analysis_url())
                .flatten()
                .map(str::to_string)
            else {
                // Disabled or repointed while backing off -- abandon; a
                // fresh check happens next time the dispatcher is started.
                PEER_DOWN.store(false, Ordering::SeqCst);
                BACKOFF_RUNNING.store(false, Ordering::SeqCst);
                return;
            };

            if ping(&url) {
                info!("[parallel_analysis] peer back online, resuming");
                PEER_DOWN.store(false, Ordering::SeqCst);
                BACKOFF_RUNNING.store(false, Ordering::SeqCst);
                ensure_dispatcher_running();
                return;
            }
        }
    });
}

// ─── Peer HTTP client ──────────────────────────────────────────────────

/// Distinguishes "couldn't reach the peer at all" (treated as the whole
/// peer being down) from "reached it but this particular call failed"
/// (treated as just this song being unfit for the peer).
enum PeerError {
    Unreachable,
    Other,
}

fn classify_error(e: ureq::Error) -> PeerError {
    match e {
        ureq::Error::StatusCode(_) => PeerError::Other,
        _ => PeerError::Unreachable,
    }
}

static CMD_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_recv_response(Some(Duration::from_secs(15)))
        .build();
    ureq::Agent::new_with_config(config)
});

/// No response-read timeout -- stem downloads can legitimately take a while
/// once bytes start flowing; only the initial connect is bounded.
static DOWNLOAD_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .build();
    ureq::Agent::new_with_config(config)
});

fn cmd_url(base_url: &str, name: &str) -> String {
    format!("{}/api/cmd/{name}", base_url.trim_end_matches('/'))
}

fn media_url(base_url: &str, file_hash: &str, kind: &str) -> String {
    format!("{}/media/{file_hash}/{kind}", base_url.trim_end_matches('/'))
}

fn ping(base_url: &str) -> bool {
    CMD_AGENT
        .get(format!("{}/api/bootstrap", base_url.trim_end_matches('/')))
        .call()
        .is_ok()
}

fn post_cmd(base_url: &str, name: &str, body: Value) -> Result<Value, PeerError> {
    let resp = CMD_AGENT
        .post(cmd_url(base_url, name))
        .send_json(body)
        .map_err(classify_error)?;
    resp.into_body()
        .read_json::<Value>()
        .map_err(|_| PeerError::Other)
}

#[derive(Deserialize)]
struct SongsByHashesResponse {
    processed: Vec<Song>,
}

fn peer_song(base_url: &str, file_hash: &str) -> Result<Option<Song>, PeerError> {
    let value = post_cmd(
        base_url,
        "load_songs_by_hashes",
        json!({ "fileHashes": [file_hash] }),
    )?;
    let response: SongsByHashesResponse =
        serde_json::from_value(value).map_err(|_| PeerError::Other)?;
    Ok(response
        .processed
        .into_iter()
        .find(|s| s.file_hash == file_hash))
}

#[derive(Deserialize)]
struct QueueResponse {
    entries: std::collections::HashMap<String, QueuedStatus>,
}

fn peer_queue_status(base_url: &str, file_hash: &str) -> Result<Option<QueuedStatus>, PeerError> {
    let value = post_cmd(base_url, "load_analysis_queue", Value::Null)?;
    let response: QueueResponse = serde_json::from_value(value).map_err(|_| PeerError::Other)?;
    Ok(response.entries.get(file_hash).cloned())
}

fn trigger(base_url: &str, file_hash: &str) -> Result<(), PeerError> {
    post_cmd(base_url, "enqueue_one", json!({ "fileHash": file_hash }))?;
    Ok(())
}

/// Streams `{base_url}/media/<hash>/<kind>` to `dest`, writing to a `.part`
/// sibling first and renaming into place so a dropped connection never
/// leaves a truncated file that `analyzer::finalize_peer_analysis` could
/// mistake for a finished one.
fn download_to(base_url: &str, file_hash: &str, kind: &str, dest: &Path) -> bool {
    let Ok(resp) = DOWNLOAD_AGENT
        .get(media_url(base_url, file_hash, kind))
        .call()
    else {
        return false;
    };

    let tmp = dest.with_extension("part");
    let copied = (|| -> std::io::Result<()> {
        let mut body = resp.into_body();
        let mut reader = body.as_reader();
        let mut file = std::fs::File::create(&tmp)?;
        std::io::copy(&mut reader, &mut file)?;
        Ok(())
    })()
    .is_ok();

    if copied {
        std::fs::rename(&tmp, dest).is_ok()
    } else {
        let _ = std::fs::remove_file(&tmp);
        false
    }
}
