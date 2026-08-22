//! Peer-offload for the analysis queue ("parallel analysis"): when enabled
//! and pointed at another Nightingale instance's base URL, a dispatcher
//! drains the *back* of the local analysis queue (the local worker in
//! `analyzer.rs` drains the front) and hands those songs to the peer
//! instead, so two machines pointed at the same library can chew through a
//! backlog concurrently. See `analyzer::claim_from_back_excluding` for why
//! the two workers can't double-claim a hash.
//!
//! Both instances are assumed to be `Folder`-sourced libraries mirroring the
//! same *relative* directory structure, not necessarily living at the same
//! absolute filesystem path (different usernames, mount points, or OSes are
//! fine). A song is only handed off once the peer confirms it has the same
//! file at that same relative path with the same content hash -- see
//! `relative_song_path`/`song_at_path`.
//!
//! Every peer call goes through the same HTTP surface the browser client
//! uses (`/api/cmd/*`, `/media/<hash>/<kind>`) -- no separate protocol, no
//! auth beyond whatever already gates that port (matches the existing
//! self-hosted-server trust model: see `client/src-server`).
//!
//! Every network call and every dispatch decision logs through `tracing`
//! under the `[parallel_analysis]` prefix -- `RUST_LOG=info` (or `debug`)
//! shows the full trail for a stuck/failing peer (`nightingale.log` for the
//! Tauri build, stdout for `client/src-server`). Per-song lines are tagged
//! with `describe()`'s `"<filename> (<hash>)"` label rather than a bare
//! hash, so a log is scannable without cross-referencing the library.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::analyzer::QueuedStatus;
use crate::cache::CacheDir;
use crate::config::{AppConfig, LibrarySource};
use crate::library_db;
use crate::song::{Song, read_transcript_meta};

/// How many times to poll the peer for completion, 3s apart (== 20 min).
const POLL_ATTEMPTS: u32 = 400;
const POLL_INTERVAL: Duration = Duration::from_secs(3);
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
    let alive = ping(url);
    // `PEER_DOWN.swap` returns the previous value -- if it was true, the
    // dispatcher is currently paused waiting on the hourly backoff thread's
    // next wake-up (up to an hour away). A successful manual ping is itself
    // proof the peer's back, so clear the flag and resume right away rather
    // than making the user wait on that thread. It's left running (not
    // touching `BACKOFF_RUNNING`); it'll just no-op harmlessly next time it
    // wakes, same as if it had found the peer alive itself.
    if alive && PEER_DOWN.swap(false, Ordering::SeqCst) {
        info!(
            "[parallel_analysis] manual ping: peer was marked down, clearing that and resuming dispatcher"
        );
        ensure_dispatcher_running();
    }
    alive
}

/// This instance's own folder-library root, if configured as a `Folder`
/// source -- the only source kind `parallel_analysis` can mirror against,
/// since remote-sourced libraries (Jellyfin/Navidrome/Plex) have no
/// meaningful "same relative path" of their own.
fn folder_root() -> Option<PathBuf> {
    match AppConfig::load().library_source {
        Some(LibrarySource::Folder { path }) => Some(path),
        _ => None,
    }
}

/// `song_path` relative to this instance's own library root -- what's sent
/// to the peer instead of the full absolute path. The two instances are
/// assumed to mirror the same *relative* directory structure under each
/// one's own root, not to live at the identical absolute filesystem path
/// (different usernames/mount points/OSes are fine as long as the folder
/// layout underneath matches). Returns `None` if this instance isn't
/// folder-sourced, or `song_path` somehow isn't under its own root.
fn relative_song_path(song_path: &Path) -> Option<PathBuf> {
    let root = folder_root()?;
    song_path.strip_prefix(&root).ok().map(Path::to_path_buf)
}

/// Server-side half of the peer protocol, exposed via the
/// `load_song_by_path` command: given a path *relative to the peer's own
/// library root* (see `relative_song_path`), resolves it against *this*
/// instance's own root and looks the result up -- lets a peer check whether
/// this instance has the same file at the same relative path before
/// offloading a song to it (and, if the hashes then don't match, that the
/// peer records why). Returns `None` (treated by the caller as "peer
/// doesn't have this song") if this instance isn't folder-sourced either.
pub fn song_at_path(relative_path: &Path) -> Option<Song> {
    let Some(root) = folder_root() else {
        warn!(
            "[parallel_analysis] server: got a load_song_by_path request for {relative_path:?} \
             but this instance isn't folder-sourced -- nothing to resolve it against"
        );
        return None;
    };
    let absolute = root.join(relative_path);
    let song = library_db::load_song_by_path(&absolute.to_string_lossy())
        .ok()
        .flatten();
    match &song {
        Some(s) => info!(
            "[parallel_analysis] server: load_song_by_path {relative_path:?} -> found, hash={} is_analyzed={}",
            s.file_hash, s.is_analyzed
        ),
        None => info!(
            "[parallel_analysis] server: load_song_by_path {relative_path:?} -> nothing at {absolute:?}"
        ),
    }
    song
}

/// "<filename> (<hash>)" label for log lines -- scanning logs for a bare
/// hash means cross-referencing the library by hand every time; this makes
/// the actual file obvious at a glance. Falls back to just the hash if the
/// song can't be found locally (e.g. deleted out from under an in-flight
/// dispatch).
fn describe(file_hash: &str) -> String {
    let filename = library_db::load_song_by_hash(file_hash)
        .ok()
        .flatten()
        .and_then(|song| song.path.file_name().map(|n| n.to_string_lossy().into_owned()));
    match filename {
        Some(name) => format!("{name} ({file_hash})"),
        None => file_hash.to_string(),
    }
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
    /// worth trying other queued songs against the peer. Carries a short
    /// human-readable reason so the dispatcher's own log line is
    /// self-contained -- no need to scroll up to find out why.
    Rejected(String),
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
        let label = describe(&file_hash);

        info!("[parallel_analysis] dispatcher: dispatching {label} to {base_url}");
        match dispatch_one(&base_url, &file_hash, &label) {
            DispatchOutcome::Done => {
                info!("[parallel_analysis] dispatcher: {label} completed via peer");
            }
            DispatchOutcome::Rejected(reason) => {
                info!(
                    "[parallel_analysis] dispatcher: {label} rejected by peer ({reason}), returned to local queue"
                );
                skip.insert(file_hash);
            }
            DispatchOutcome::PeerDown => {
                warn!(
                    "[parallel_analysis] dispatcher: peer {base_url} went down mid-dispatch ({label}), stopping"
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

fn dispatch_one(base_url: &str, file_hash: &str, label: &str) -> DispatchOutcome {
    // Coarse wall-clock timing for `parallel_analysis_timings`, recorded
    // only on a successful `Done` below -- covers this whole function, not
    // just the trigger/poll phase, since even the "already analyzed"
    // fetch-only path is worth knowing the cost of.
    let started = std::time::Instant::now();
    let mut poll_attempts: Option<u32> = None;

    if !ping(base_url) {
        warn!(
            "[parallel_analysis] {label}: peer {base_url} did not respond to ping, treating as down"
        );
        return DispatchOutcome::PeerDown;
    }

    let Some(local_song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        warn!("[parallel_analysis] {label}: no longer in local library, skipping");
        return DispatchOutcome::Rejected("no longer in local library".to_string());
    };
    // Recorded on a mismatch as the human-readable "which local file"
    // pointer (see `record_mismatch`); the peer lookup itself uses the
    // *relative* path below, not this.
    let local_path = local_song.path.to_string_lossy().into_owned();

    let Some(relative_path) = relative_song_path(&local_song.path) else {
        warn!(
            "[parallel_analysis] {label}: can't determine a library-relative path for {local_path:?} (not a folder-sourced library, or the file isn't under the configured library root) -- skipping peer offload"
        );
        crate::analyzer::return_to_front(file_hash);
        return DispatchOutcome::Rejected(
            "not folder-sourced, or file isn't under the library root".to_string(),
        );
    };
    let relative_path = relative_path.to_string_lossy().into_owned();
    info!("[parallel_analysis] {label}: checking peer for relative path {relative_path:?}");

    // Libraries are assumed to mirror the same relative structure, so a
    // peer lookup is done by *path* (not hash): that's what lets a
    // same-path-different-content song be told apart from one the peer
    // genuinely doesn't have, and recorded in `parallel_analysis_mismatches`
    // either way (see `record_mismatch`) instead of silently falling back
    // to local processing with no trace of why.
    let peer_song = match peer_song_by_path(base_url, &relative_path) {
        Ok(song) => song,
        Err(PeerError::Unreachable) => {
            warn!("[parallel_analysis] {label}: peer unreachable during path lookup");
            return DispatchOutcome::PeerDown;
        }
        Err(PeerError::Other) => {
            warn!(
                "[parallel_analysis] {label}: path lookup on peer failed (non-network), treating as no match"
            );
            None
        }
    };

    let peer_song = match peer_song {
        None => {
            info!(
                "[parallel_analysis] {label}: peer has nothing at relative path {relative_path:?}"
            );
            record_mismatch(file_hash, &local_path, base_url, None, label);
            crate::analyzer::return_to_front(file_hash);
            return DispatchOutcome::Rejected(format!(
                "peer has nothing at relative path {relative_path:?}"
            ));
        }
        Some(song) if song.file_hash != file_hash => {
            warn!(
                "[parallel_analysis] {label}: peer has a different hash at relative path {relative_path:?} (peer hash={})",
                song.file_hash
            );
            record_mismatch(file_hash, &local_path, base_url, Some(&song.file_hash), label);
            crate::analyzer::return_to_front(file_hash);
            return DispatchOutcome::Rejected(format!(
                "peer has a different hash at {relative_path:?} (peer hash={})",
                song.file_hash
            ));
        }
        Some(song) => {
            info!(
                "[parallel_analysis] {label}: matched on peer (already analyzed={})",
                song.is_analyzed
            );
            clear_mismatch(file_hash, label);
            song
        }
    };

    let already_analyzed_on_peer = peer_song.is_analyzed;

    if !peer_song.is_analyzed {
        info!("[parallel_analysis] {label}: triggering analysis on peer");
        match trigger(base_url, file_hash) {
            Ok(()) => {}
            Err(PeerError::Unreachable) => return DispatchOutcome::PeerDown,
            Err(PeerError::Other) => {
                warn!("[parallel_analysis] {label}: failed to trigger analysis on peer");
                crate::analyzer::return_to_front(file_hash);
                return DispatchOutcome::Rejected("failed to trigger analysis on peer".to_string());
            }
        }
        crate::analyzer::update_queue_status(file_hash, QueuedStatus::Analyzing(0));

        match poll_until_done(base_url, file_hash, label) {
            PollOutcome::Done(attempts) => {
                info!("[parallel_analysis] {label}: peer finished analyzing");
                poll_attempts = Some(attempts);
            }
            PollOutcome::GaveUp(reason) => {
                crate::analyzer::return_to_front(file_hash);
                return DispatchOutcome::Rejected(reason);
            }
            PollOutcome::PeerDown => return DispatchOutcome::PeerDown,
        }
    } else {
        info!("[parallel_analysis] {label}: already analyzed on peer, fetching results");
    }

    if !fetch_results(base_url, file_hash, label) {
        warn!("[parallel_analysis] {label}: failed to fetch results from peer");
        crate::analyzer::return_to_front(file_hash);
        return DispatchOutcome::Rejected("failed to fetch results from peer".to_string());
    }

    info!("[parallel_analysis] {label}: results fetched from peer, finalizing locally");
    // `finalize_peer_analysis` can still fail even after every download
    // reported success -- e.g. the transcript was written but the stems
    // ended up somewhere `CacheDir` doesn't look, or the transcript's
    // `no_stems` flag doesn't say what `fetch_results` thought it did.
    // Check its result rather than assuming "we downloaded fine" means "the
    // song is now marked analyzed".
    if crate::analyzer::finalize_peer_analysis(file_hash) {
        record_timing(
            file_hash,
            base_url,
            already_analyzed_on_peer,
            poll_attempts,
            started.elapsed().as_millis() as u64,
            label,
        );
        DispatchOutcome::Done
    } else {
        warn!(
            "[parallel_analysis] {label}: downloaded results from peer but local finalize \
             failed (see the [analyzer] Finalize failed line above for which file was missing) \
             -- returning to local queue"
        );
        crate::analyzer::return_to_front(file_hash);
        DispatchOutcome::Rejected(
            "downloaded results from peer but local finalize failed (see [analyzer] Finalize \
             failed log line above)"
                .to_string(),
        )
    }
}

/// Records that `file_hash` (at `path`) didn't match on `peer_url` --
/// `peer_hash` is `None` when the peer had nothing at that path at all, or
/// `Some` with the peer's differing hash. Libraries are assumed to mirror
/// each other (`parallel_analysis` is offloading, not syncing), so this is
/// always worth surfacing rather than silently retrying forever; read the
/// table with `scripts/parallel_analysis_mismatches.py`.
fn record_mismatch(
    file_hash: &str,
    path: &str,
    peer_url: &str,
    peer_hash: Option<&str>,
    label: &str,
) {
    if let Err(e) =
        library_db::record_parallel_analysis_mismatch(file_hash, path, peer_url, peer_hash)
    {
        warn!("[parallel_analysis] failed to record mismatch for {label}: {e}");
    }
}

/// Clears a previously recorded mismatch once a later check finds a match --
/// keeps the table reflecting current state rather than growing forever.
fn clear_mismatch(file_hash: &str, label: &str) {
    if let Err(e) = library_db::clear_parallel_analysis_mismatch(file_hash) {
        warn!("[parallel_analysis] failed to clear mismatch for {label}: {e}");
    }
}

/// Records a successful dispatch's wall-clock cost to
/// `parallel_analysis_timings` -- gated on the same `track_analysis_timings`
/// setting the local pipeline's own `analysis_timings` uses, since they're
/// the same "is this diagnostic worth the write" opt-in. Read the table with
/// `scripts/analysis_progress.py`.
fn record_timing(
    file_hash: &str,
    peer_url: &str,
    already_analyzed_on_peer: bool,
    poll_attempts: Option<u32>,
    total_ms: u64,
    label: &str,
) {
    if !AppConfig::load().track_analysis_timings() {
        return;
    }
    info!(
        "[parallel_analysis] {label}: total_ms={total_ms} already_analyzed_on_peer={already_analyzed_on_peer} poll_attempts={poll_attempts:?}"
    );
    let row = library_db::ParallelAnalysisTimingRow {
        file_hash,
        peer_url,
        already_analyzed_on_peer,
        poll_attempts,
        total_ms,
    };
    if let Err(e) = library_db::insert_parallel_analysis_timing(&row) {
        warn!("[parallel_analysis] {label}: failed to record parallel analysis timing: {e}");
    }
}

enum PollOutcome {
    /// Carries which poll attempt (1-based) confirmed completion, recorded
    /// in `parallel_analysis_timings`.
    Done(u32),
    /// Carries a short human-readable reason, propagated up into
    /// `DispatchOutcome::Rejected` so the dispatcher's own log line names
    /// the cause without needing to scroll up to find it.
    GaveUp(String),
    PeerDown,
}

/// Polls the peer's queue every `POLL_INTERVAL` for up to `POLL_ATTEMPTS`
/// (== 20 minutes), mirroring reported progress onto the local queue row.
fn poll_until_done(base_url: &str, file_hash: &str, label: &str) -> PollOutcome {
    // The "still going" states (Analyzing/Queued) repeat every tick for as
    // long as the peer's working -- at a 3s `POLL_INTERVAL` that's a lot of
    // near-identical lines. Only log those two when the status actually
    // changed since the last logged one, or every 5th poll as a heartbeat so
    // a long-running analysis still shows up periodically.
    let mut last_logged: Option<QueuedStatus> = None;

    for attempt in 1..=POLL_ATTEMPTS {
        std::thread::sleep(POLL_INTERVAL);

        if !AppConfig::load().parallel_analysis_enabled() {
            info!("[parallel_analysis] {label}: disabled mid-poll, giving up on peer");
            return PollOutcome::GaveUp("parallel analysis disabled mid-poll".to_string());
        }

        match peer_queue_status(base_url, file_hash) {
            Ok(Some(QueuedStatus::Failed { message, .. })) => {
                warn!("[parallel_analysis] {label}: peer reported failure: {message}");
                return PollOutcome::GaveUp(format!("peer reported failure: {message}"));
            }
            Ok(Some(status @ QueuedStatus::Analyzing(pct))) => {
                if last_logged.as_ref() != Some(&status) || attempt % 5 == 0 {
                    info!(
                        "[parallel_analysis] {label}: peer analyzing ({pct}%) [poll {attempt}/{POLL_ATTEMPTS}]"
                    );
                    last_logged = Some(status);
                }
                crate::analyzer::update_queue_status(file_hash, QueuedStatus::Analyzing(pct));
            }
            Ok(Some(status @ QueuedStatus::Queued)) => {
                if last_logged.as_ref() != Some(&status) || attempt % 5 == 0 {
                    info!(
                        "[parallel_analysis] {label}: still queued on peer [poll {attempt}/{POLL_ATTEMPTS}]"
                    );
                    last_logged = Some(status);
                }
            }
            Ok(None) => {
                info!(
                    "[parallel_analysis] {label}: no longer queued on peer, checking whether it finished"
                );
                return match peer_song(base_url, file_hash) {
                    Ok(Some(song)) if song.is_analyzed => {
                        info!("[parallel_analysis] {label}: peer confirms is_analyzed=true");
                        PollOutcome::Done(attempt)
                    }
                    Ok(Some(song)) => {
                        warn!(
                            "[parallel_analysis] {label}: gone from peer's queue but peer's own \
                             song row still has is_analyzed=false -- either a genuine failure, or a \
                             transient race right as the peer finished (will look stale if seen only \
                             once); is_video={} usdx={}",
                            song.is_video,
                            song.usdx.is_some()
                        );
                        PollOutcome::GaveUp(
                            "gone from peer's queue but peer still shows is_analyzed=false"
                                .to_string(),
                        )
                    }
                    Ok(None) => {
                        warn!(
                            "[parallel_analysis] {label}: gone from peer's queue and peer has no \
                             song row for this hash at all"
                        );
                        PollOutcome::GaveUp(
                            "gone from peer's queue and peer has no song row for this hash"
                                .to_string(),
                        )
                    }
                    Err(PeerError::Unreachable) => {
                        warn!(
                            "[parallel_analysis] {label}: peer unreachable while confirming finish"
                        );
                        PollOutcome::PeerDown
                    }
                    Err(PeerError::Other) => {
                        warn!(
                            "[parallel_analysis] {label}: load_songs_by_hashes call to peer failed \
                             (non-network, e.g. bad response body) while confirming finish"
                        );
                        PollOutcome::GaveUp(
                            "load_songs_by_hashes call to peer failed while confirming finish"
                                .to_string(),
                        )
                    }
                };
            }
            Err(PeerError::Unreachable) => {
                warn!("[parallel_analysis] {label}: peer unreachable mid-poll");
                return PollOutcome::PeerDown;
            }
            Err(PeerError::Other) => {
                warn!(
                    "[parallel_analysis] {label}: poll request failed (non-network) [poll {attempt}/{POLL_ATTEMPTS}]"
                );
            }
        }
    }
    warn!("[parallel_analysis] {label}: timed out after {POLL_ATTEMPTS} polls (20 min), giving up on peer");
    PollOutcome::GaveUp(format!("timed out after {POLL_ATTEMPTS} polls (20 min)"))
}

/// Copies the peer's finished transcript (and, unless the transcript marks
/// `no_stems`, the instrumental/vocals stems) plus any lyrics file into the
/// local cache, straight off `/media/<hash>/<kind>` (byte-for-byte, no JSON
/// round-trip). Returns whether a usable transcript landed locally.
fn fetch_results(base_url: &str, file_hash: &str, label: &str) -> bool {
    let cache = CacheDir::new();

    if !download_to(
        base_url,
        file_hash,
        "transcript",
        &cache.transcript_path(file_hash),
        label,
    ) {
        warn!("[parallel_analysis] {label}: failed to fetch transcript from peer");
        return false;
    }
    info!(
        "[parallel_analysis] {label}: transcript on disk, {} bytes",
        file_len(&cache.transcript_path(file_hash))
    );

    let meta = read_transcript_meta(&cache, file_hash);
    info!(
        "[parallel_analysis] {label}: transcript meta -- no_stems={} source={:?} key={:?} tempo={} language={:?}",
        meta.no_stems, meta.source, meta.key, meta.tempo, meta.language
    );
    if !meta.no_stems {
        let instrumental_ok = download_to(
            base_url,
            file_hash,
            "instrumental",
            &cache.instrumental_path(file_hash),
            label,
        );
        let vocals_ok = download_to(
            base_url,
            file_hash,
            "vocals",
            &cache.vocals_path(file_hash),
            label,
        );
        if !instrumental_ok || !vocals_ok {
            warn!(
                "[parallel_analysis] {label}: failed to fetch stems from peer (instrumental_ok={instrumental_ok}, vocals_ok={vocals_ok})"
            );
            return false;
        }
        info!(
            "[parallel_analysis] {label}: stems on disk, instrumental={} bytes, vocals={} bytes",
            file_len(&cache.instrumental_path(file_hash)),
            file_len(&cache.vocals_path(file_hash))
        );
    }

    // Lyrics is optional/best-effort -- its absence on the peer isn't fatal.
    if !download_to(
        base_url,
        file_hash,
        "lyrics",
        &cache.lyrics_path(file_hash),
        label,
    ) {
        info!(
            "[parallel_analysis] {label}: no lyrics file fetched from peer (missing or failed) -- continuing without it"
        );
    }

    // Same check `analyzer::finalize_song` is about to make -- logging it
    // here, before finalize runs, pins down whether a "finalize failed"
    // right after this is a download problem (this would already be false)
    // or something in finalize itself.
    info!(
        "[parallel_analysis] {label}: transcript_exists()={} going into finalize",
        cache.transcript_exists(file_hash)
    );

    true
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
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

fn peer_song(base_url: &str, file_hash: &str) -> Result<Option<Song>, PeerError> {
    let value = post_cmd(
        base_url,
        "load_songs_by_hashes",
        json!({ "fileHashes": [file_hash] }),
    )?;
    // `load_songs_by_hashes` (unlike `load_songs`) returns a bare
    // `Vec<Song>`, not a `SongsStore`-shaped `{ processed: [...] }` object --
    // deserializing into the wrapper here silently failed every call.
    let songs: Vec<Song> = serde_json::from_value(value).map_err(|_| PeerError::Other)?;
    Ok(songs.into_iter().find(|s| s.file_hash == file_hash))
}

/// Looks the peer up by *path relative to its own library root* rather than
/// hash or absolute path -- see `song_at_path` for the server side of this
/// call, which resolves `relative_path` against the peer's own root before
/// querying.
fn peer_song_by_path(base_url: &str, relative_path: &str) -> Result<Option<Song>, PeerError> {
    let value = post_cmd(
        base_url,
        "load_song_by_path",
        json!({ "path": relative_path }),
    )?;
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
fn download_to(base_url: &str, file_hash: &str, kind: &str, dest: &Path, label: &str) -> bool {
    let url = media_url(base_url, file_hash, kind);
    let resp = match DOWNLOAD_AGENT.get(&url).call() {
        Ok(resp) => resp,
        Err(e) => {
            warn!("[parallel_analysis] {label}: GET {url} failed: {e} ({e:?})");
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
                info!("[parallel_analysis] {label}: downloaded {kind} from {url}");
                true
            }
            Err(e) => {
                warn!(
                    "[parallel_analysis] {label}: downloaded {kind} from {url} but failed to move into place: {e}"
                );
                let _ = std::fs::remove_file(&tmp);
                false
            }
        },
        Err(e) => {
            warn!("[parallel_analysis] {label}: failed writing {kind} from {url} to disk: {e}");
            let _ = std::fs::remove_file(&tmp);
            false
        }
    }
}
