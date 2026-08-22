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
//!
//! Every network call and every dispatch decision logs through `tracing`
//! under the `[parallel_analysis]` prefix -- `RUST_LOG=info` (or `debug`)
//! shows the full trail for a stuck/failing peer (`nightingale.log` for the
//! Tauri build, stdout for `client/src-server`).

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
/// check against `url` directly -- deliberately *not* `AppConfig::load()`'s
/// `parallel_analysis_url`, since the button needs to test whatever's
/// currently typed in the field, not whatever was last saved (the save is a
/// separate, unawaited request from the settings page, so reading the saved
/// config here would race it and could ping a stale/empty URL). `false` if
/// `url` is blank.
pub fn manual_ping(url: &str) -> bool {
    info!("[parallel_analysis] manual ping requested: url={url:?}");
    let url = url.trim();
    if url.is_empty() {
        info!("[parallel_analysis] manual ping: blank url, not pinging");
        return false;
    }
    ping(url)
}

/// Server-side half of the peer protocol, exposed via the
/// `load_song_by_path` command: lets a peer instance check whether *this*
/// instance has the same file at the same path before offloading a song to
/// it (and, if the hashes then don't match, that the peer records why).
pub fn song_at_path(path: &Path) -> Option<Song> {
    library_db::load_song_by_path(&path.to_string_lossy())
        .ok()
        .flatten()
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
    let Some(url) = config.parallel_analysis_url() else {
        info!("[parallel_analysis] enabled but no peer url configured, not starting dispatcher");
        return;
    };
    if PEER_DOWN.load(Ordering::SeqCst) {
        info!(
            "[parallel_analysis] peer {url} known-down, not starting dispatcher (waiting on hourly backoff)"
        );
        return;
    }
    if crate::analyzer::try_start_parallel_dispatcher() {
        info!("[parallel_analysis] starting dispatcher for peer {url}");
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
    info!("[parallel_analysis] dispatcher: started");
    // Hashes this run has already had rejected/failed/timed-out by the peer
    // -- skipped on subsequent claims so one stuck song can't spin the loop.
    let mut skip: HashSet<String> = HashSet::new();

    loop {
        let config = AppConfig::load();
        if !config.parallel_analysis_enabled() {
            info!("[parallel_analysis] dispatcher: stopping, parallel analysis disabled");
            break;
        }
        let Some(base_url) = config.parallel_analysis_url().map(str::to_string) else {
            info!("[parallel_analysis] dispatcher: stopping, no peer url configured");
            break;
        };

        let Some(file_hash) = crate::analyzer::claim_from_back_excluding(&skip) else {
            info!("[parallel_analysis] dispatcher: stopping, nothing left to claim");
            break;
        };

        info!("[parallel_analysis] dispatcher: dispatching {file_hash} to {base_url}");
        match dispatch_one(&base_url, &file_hash) {
            DispatchOutcome::Done => {
                info!("[parallel_analysis] dispatcher: {file_hash} completed via peer");
            }
            DispatchOutcome::Rejected => {
                info!(
                    "[parallel_analysis] dispatcher: {file_hash} rejected by peer, returned to local queue"
                );
                skip.insert(file_hash);
            }
            DispatchOutcome::PeerDown => {
                warn!(
                    "[parallel_analysis] dispatcher: peer {base_url} went down mid-dispatch, stopping"
                );
                crate::analyzer::return_to_front(&file_hash);
                enter_down_backoff();
                break;
            }
        }
    }

    info!("[parallel_analysis] dispatcher: stopped");
    crate::analyzer::stop_parallel_dispatcher();
}

fn dispatch_one(base_url: &str, file_hash: &str) -> DispatchOutcome {
    if !ping(base_url) {
        warn!(
            "[parallel_analysis] {file_hash}: peer {base_url} did not respond to ping, treating as down"
        );
        return DispatchOutcome::PeerDown;
    }

    let Some(local_song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        warn!("[parallel_analysis] {file_hash}: no longer in local library, skipping");
        return DispatchOutcome::Rejected;
    };
    let path = local_song.path.to_string_lossy().into_owned();
    info!("[parallel_analysis] {file_hash}: checking peer for path {path:?}");

    // Libraries are assumed to mirror each other, so a peer lookup is done
    // by *path* (not hash): that's what lets a same-path-different-content
    // song be told apart from one the peer genuinely doesn't have, and
    // recorded in `parallel_analysis_mismatches` either way (see
    // `record_mismatch`) instead of silently falling back to local
    // processing with no trace of why.
    let peer_song = match peer_song_by_path(base_url, &path) {
        Ok(song) => song,
        Err(PeerError::Unreachable) => {
            warn!("[parallel_analysis] {file_hash}: peer unreachable during path lookup");
            return DispatchOutcome::PeerDown;
        }
        Err(PeerError::Other) => {
            warn!(
                "[parallel_analysis] {file_hash}: path lookup on peer failed (non-network), treating as no match"
            );
            None
        }
    };

    let peer_song = match peer_song {
        None => {
            info!("[parallel_analysis] {file_hash}: peer has nothing at {path:?}");
            record_mismatch(file_hash, &path, base_url, None);
            crate::analyzer::return_to_front(file_hash);
            return DispatchOutcome::Rejected;
        }
        Some(song) if song.file_hash != file_hash => {
            warn!(
                "[parallel_analysis] {file_hash}: peer has a different hash at {path:?} (peer hash={})",
                song.file_hash
            );
            record_mismatch(file_hash, &path, base_url, Some(&song.file_hash));
            crate::analyzer::return_to_front(file_hash);
            return DispatchOutcome::Rejected;
        }
        Some(song) => {
            info!(
                "[parallel_analysis] {file_hash}: matched on peer (already analyzed={})",
                song.is_analyzed
            );
            clear_mismatch(file_hash);
            song
        }
    };

    if !peer_song.is_analyzed {
        info!("[parallel_analysis] {file_hash}: triggering analysis on peer");
        match trigger(base_url, file_hash) {
            Ok(()) => {}
            Err(PeerError::Unreachable) => return DispatchOutcome::PeerDown,
            Err(PeerError::Other) => {
                warn!("[parallel_analysis] {file_hash}: failed to trigger analysis on peer");
                crate::analyzer::return_to_front(file_hash);
                return DispatchOutcome::Rejected;
            }
        }
        crate::analyzer::update_queue_status(file_hash, QueuedStatus::Analyzing(0));

        match poll_until_done(base_url, file_hash) {
            PollOutcome::Done => {
                info!("[parallel_analysis] {file_hash}: peer finished analyzing");
            }
            PollOutcome::GaveUp => {
                crate::analyzer::return_to_front(file_hash);
                return DispatchOutcome::Rejected;
            }
            PollOutcome::PeerDown => return DispatchOutcome::PeerDown,
        }
    } else {
        info!("[parallel_analysis] {file_hash}: already analyzed on peer, fetching results");
    }

    if fetch_results(base_url, file_hash) {
        info!("[parallel_analysis] {file_hash}: results fetched from peer, finalizing locally");
        crate::analyzer::finalize_peer_analysis(file_hash);
        DispatchOutcome::Done
    } else {
        warn!("[parallel_analysis] {file_hash}: failed to fetch results from peer");
        crate::analyzer::return_to_front(file_hash);
        DispatchOutcome::Rejected
    }
}

/// Records that `file_hash` (at `path`) didn't match on `peer_url` --
/// `peer_hash` is `None` when the peer had nothing at that path at all, or
/// `Some` with the peer's differing hash. Libraries are assumed to mirror
/// each other (`parallel_analysis` is offloading, not syncing), so this is
/// always worth surfacing rather than silently retrying forever; read the
/// table with `scripts/parallel_analysis_mismatches.py`.
fn record_mismatch(file_hash: &str, path: &str, peer_url: &str, peer_hash: Option<&str>) {
    if let Err(e) =
        library_db::record_parallel_analysis_mismatch(file_hash, path, peer_url, peer_hash)
    {
        warn!("[parallel_analysis] failed to record mismatch for {file_hash}: {e}");
    }
}

/// Clears a previously recorded mismatch once a later check finds a match --
/// keeps the table reflecting current state rather than growing forever.
fn clear_mismatch(file_hash: &str) {
    if let Err(e) = library_db::clear_parallel_analysis_mismatch(file_hash) {
        warn!("[parallel_analysis] failed to clear mismatch for {file_hash}: {e}");
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
    for attempt in 1..=POLL_ATTEMPTS {
        std::thread::sleep(POLL_INTERVAL);

        if !AppConfig::load().parallel_analysis_enabled() {
            info!("[parallel_analysis] {file_hash}: disabled mid-poll, giving up on peer");
            return PollOutcome::GaveUp;
        }

        match peer_queue_status(base_url, file_hash) {
            Ok(Some(QueuedStatus::Failed { message, .. })) => {
                warn!("[parallel_analysis] {file_hash}: peer reported failure: {message}");
                return PollOutcome::GaveUp;
            }
            Ok(Some(QueuedStatus::Analyzing(pct))) => {
                info!(
                    "[parallel_analysis] {file_hash}: peer analyzing ({pct}%) [poll {attempt}/{POLL_ATTEMPTS}]"
                );
                crate::analyzer::update_queue_status(file_hash, QueuedStatus::Analyzing(pct));
            }
            Ok(Some(QueuedStatus::Queued)) => {
                info!(
                    "[parallel_analysis] {file_hash}: still queued on peer [poll {attempt}/{POLL_ATTEMPTS}]"
                );
            }
            Ok(None) => {
                info!(
                    "[parallel_analysis] {file_hash}: no longer queued on peer, checking whether it finished"
                );
                return match peer_song(base_url, file_hash) {
                    Ok(Some(song)) if song.is_analyzed => PollOutcome::Done,
                    Ok(_) => {
                        warn!(
                            "[parallel_analysis] {file_hash}: peer dropped it from the queue without analyzing it"
                        );
                        PollOutcome::GaveUp
                    }
                    Err(PeerError::Unreachable) => PollOutcome::PeerDown,
                    Err(PeerError::Other) => PollOutcome::GaveUp,
                };
            }
            Err(PeerError::Unreachable) => {
                warn!("[parallel_analysis] {file_hash}: peer unreachable mid-poll");
                return PollOutcome::PeerDown;
            }
            Err(PeerError::Other) => {
                warn!(
                    "[parallel_analysis] {file_hash}: poll request failed (non-network) [poll {attempt}/{POLL_ATTEMPTS}]"
                );
            }
        }
    }
    warn!("[parallel_analysis] {file_hash}: timed out after {POLL_ATTEMPTS} polls (20 min), giving up on peer");
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
        warn!("[parallel_analysis] {file_hash}: failed to fetch transcript from peer");
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
            warn!(
                "[parallel_analysis] {file_hash}: failed to fetch stems from peer (instrumental_ok={instrumental_ok}, vocals_ok={vocals_ok})"
            );
            return false;
        }
    }

    // Lyrics is optional/best-effort -- its absence on the peer isn't fatal.
    if !download_to(base_url, file_hash, "lyrics", &cache.lyrics_path(file_hash)) {
        info!(
            "[parallel_analysis] {file_hash}: no lyrics file fetched from peer (missing or failed) -- continuing without it"
        );
    }

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
                info!(
                    "[parallel_analysis] backoff: parallel analysis disabled/repointed, abandoning backoff"
                );
                PEER_DOWN.store(false, Ordering::SeqCst);
                BACKOFF_RUNNING.store(false, Ordering::SeqCst);
                return;
            };

            info!("[parallel_analysis] backoff: rechecking peer {url}");
            if ping(&url) {
                info!("[parallel_analysis] backoff: peer {url} back online, resuming");
                PEER_DOWN.store(false, Ordering::SeqCst);
                BACKOFF_RUNNING.store(false, Ordering::SeqCst);
                ensure_dispatcher_running();
                return;
            }
            info!(
                "[parallel_analysis] backoff: peer {url} still down, rechecking again in {BACKOFF_INTERVAL:?}"
            );
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

fn classify_error(e: &ureq::Error) -> PeerError {
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

/// Users type peer addresses the way a browser address bar accepts them --
/// `192.168.1.170:8080`, no scheme -- but `ureq` requires an absolute URI
/// and fails to even parse a schemeless one (surfacing as "peer
/// unreachable" with nothing in the logs to explain why, since it never
/// gets far enough to attempt a connection). Default to `http://` so typing
/// it the browser way still works; an explicit `http://`/`https://` is left
/// alone.
fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

fn cmd_url(base_url: &str, name: &str) -> String {
    format!("{}/api/cmd/{name}", normalize_base_url(base_url))
}

fn media_url(base_url: &str, file_hash: &str, kind: &str) -> String {
    format!("{}/media/{file_hash}/{kind}", normalize_base_url(base_url))
}

fn ping(base_url: &str) -> bool {
    let url = format!("{}/api/bootstrap", normalize_base_url(base_url));
    info!("[parallel_analysis] ping: GET {url}");
    match CMD_AGENT.get(&url).call() {
        Ok(resp) => {
            info!("[parallel_analysis] ping: {url} -> HTTP {}", resp.status());
            true
        }
        Err(e) => {
            warn!("[parallel_analysis] ping: {url} failed: {e} ({e:?})");
            false
        }
    }
}

fn post_cmd(base_url: &str, name: &str, body: Value) -> Result<Value, PeerError> {
    let url = cmd_url(base_url, name);
    let resp = match CMD_AGENT.post(&url).send_json(body) {
        Ok(resp) => resp,
        Err(e) => {
            warn!("[parallel_analysis] POST {url} failed: {e} ({e:?})");
            return Err(classify_error(&e));
        }
    };
    let status = resp.status();
    resp.into_body().read_json::<Value>().map_err(|e| {
        warn!("[parallel_analysis] POST {url} (HTTP {status}) returned an unparseable body: {e}");
        PeerError::Other
    })
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

/// Looks the peer up by *path* rather than hash -- see `song_at_path` for
/// the server side of this call.
fn peer_song_by_path(base_url: &str, path: &str) -> Result<Option<Song>, PeerError> {
    let value = post_cmd(base_url, "load_song_by_path", json!({ "path": path }))?;
    serde_json::from_value(value).map_err(|_| PeerError::Other)
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
    let url = media_url(base_url, file_hash, kind);
    let resp = match DOWNLOAD_AGENT.get(&url).call() {
        Ok(resp) => resp,
        Err(e) => {
            warn!("[parallel_analysis] GET {url} failed: {e} ({e:?})");
            return false;
        }
    };

    let tmp = dest.with_extension("part");
    let write_result = (|| -> std::io::Result<()> {
        let mut body = resp.into_body();
        let mut reader = body.as_reader();
        let mut file = std::fs::File::create(&tmp)?;
        std::io::copy(&mut reader, &mut file)?;
        Ok(())
    })();

    match write_result {
        Ok(()) => match std::fs::rename(&tmp, dest) {
            Ok(()) => {
                info!("[parallel_analysis] {file_hash}: downloaded {kind} from {url}");
                true
            }
            Err(e) => {
                warn!("[parallel_analysis] {url}: downloaded {kind} but failed to move into place: {e}");
                let _ = std::fs::remove_file(&tmp);
                false
            }
        },
        Err(e) => {
            warn!("[parallel_analysis] {url}: failed writing {kind} to disk: {e}");
            let _ = std::fs::remove_file(&tmp);
            false
        }
    }
}
