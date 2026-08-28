//! Casts a song already resolved on disk to the Chromecast configured in
//! `AppConfig.chromecast`. The device is hand-configured (host/port) rather
//! than discovered over mDNS -- we already know exactly which device to hit.
//!
//! Two receiver paths, chosen by `ChromecastConfig.receiver_app_id`:
//!  - `None` (default): Google's stock `DefaultMediaReceiver`, handed a URL
//!    to the raw audio or a pre-rendered karaoke-video MP4 via `media.load`
//!    -- the original behavior, unchanged.
//!  - `Some(app_id)`: our custom Cast Receiver (`client/src/pages/receiver`),
//!    launched by ID and driven by a `crate::cast_protocol::CastReceiverMessage`
//!    broadcast on a custom namespace instead of `media.load` -- the
//!    receiver fetches everything else (transcript, stems, background)
//!    itself, same-origin against this server.
//!
//! `cast_song_to_configured_device` is blocking (synchronous TCP via
//! `rust_cast`); callers on an async runtime must run it via
//! `tokio::task::spawn_blocking`.

use std::net::UdpSocket;
use std::sync::Once;
use std::time::Duration;

use rust_cast::{
    CastDevice,
    channels::{
        media::{IdleReason, Media, PlayerState, StreamType},
        receiver::CastDeviceApp,
    },
};
use tracing::{info, warn};

use crate::cast_protocol::{CAST_NAMESPACE, CastReceiverMessage};
use crate::config::ChromecastConfig;
use crate::error::NightingaleError;
use crate::song::Song;

const DEFAULT_SERVER_PORT: u16 = 8080;
const RECEIVER_DESTINATION_ID: &str = "receiver-0";

/// How long to wait after connecting to the freshly-launched custom
/// receiver's transport before broadcasting the load message -- the
/// receiver page needs time to finish loading its JS bundle and register
/// `context.addCustomMessageListener` before it can hear anything.
/// `broadcast_message` has no ack, so a message sent too early is silently
/// lost. MVP mitigation; tune against real device/network timing.
const RECEIVER_BOOTSTRAP_DELAY_MS: u64 = 3000;

static CRYPTO_PROVIDER_INIT: Once = Once::new();

/// `rust_cast`'s TLS connection is built on rustls 0.23, which -- when more
/// than one crypto backend is reachable in the dependency graph (as happens
/// here via transitive deps) -- refuses to pick one implicitly and panics on
/// first use instead. Installing a provider once up front avoids that.
fn ensure_crypto_provider() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

/// Finds this host's LAN-facing IP by "connecting" a UDP socket to a public
/// address (no packets are actually sent -- UDP `connect` just picks a local
/// route). Returns `None` if the host has no route to the outside world
/// (e.g. fully offline).
fn detect_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

fn server_port() -> u16 {
    std::env::var("NIGHTINGALE_BIND")
        .ok()
        .and_then(|bind| bind.rsplit(':').next().map(str::to_string))
        .and_then(|port| port.parse().ok())
        .unwrap_or(DEFAULT_SERVER_PORT)
}

fn server_base_url(config: &ChromecastConfig) -> Result<String, NightingaleError> {
    if let Some(base) = config.server_base_url.as_ref() {
        info!("[chromecast] using configured server_base_url: {base}");
        return Ok(base.trim_end_matches('/').to_string());
    }
    let ip = detect_lan_ip().ok_or_else(|| {
        NightingaleError::Other(
            "could not auto-detect this server's LAN IP; set chromecast.server_base_url in config.json".into(),
        )
    })?;
    let base = format!("http://{ip}:{}", server_port());
    info!(
        "[chromecast] auto-detected server_base_url: {base} -- if the Chromecast can't reach \
         this (e.g. this is a Docker container's internal IP, not the host's LAN IP), set \
         chromecast.server_base_url in config.json explicitly"
    );
    Ok(base)
}

/// Casts `song` (must be `SongOrigin::LocalFile`) to the device described by
/// `config`. `guide_volume` (0.0-1.0) only applies to the custom-receiver
/// path (`config.receiver_app_id`) -- the DefaultMediaReceiver path has no
/// live audio mixing to control, it just plays a URL. `force_custom_receiver`
/// is for exercising the custom-receiver path from a dedicated test trigger
/// (`client/src-server/src/cast.rs`'s `/api/customcast`) independent of
/// whichever path `receiver_app_id`'s presence would normally select --
/// still requires `receiver_app_id` to actually be configured, it just
/// turns "not configured" into a hard error here instead of a silent
/// fallback to DefaultMediaReceiver, since a caller asking to force the new
/// path wants to know it didn't happen, not get the old one instead.
pub fn cast_song_to_configured_device(
    config: &ChromecastConfig,
    song: &Song,
    guide_volume: Option<f64>,
    force_custom_receiver: bool,
) -> Result<(), NightingaleError> {
    ensure_crypto_provider();

    info!(
        "[chromecast] casting {:?} by {:?} (file_hash={}) to {}:{}",
        song.title, song.artist, song.file_hash, config.host, config.port
    );

    if force_custom_receiver && config.receiver_app_id.is_none() {
        return Err(NightingaleError::Other(
            "force_custom_receiver requested but chromecast.receiver_app_id is not set in \
             config.json"
                .to_string(),
        ));
    }

    if config.receiver_app_id.is_some() && config.karaoke_video {
        warn!(
            "[chromecast] karaoke_video is set but ignored -- receiver_app_id is also set, and \
             the custom receiver always renders background + lyrics live instead of playing a \
             pre-rendered video"
        );
    }

    let device = CastDevice::connect_without_host_verification(config.host.as_str(), config.port)
        .map_err(|e| NightingaleError::Other(format!("chromecast connect failed: {e:?}")))?;
    info!("[chromecast] connected to device at {}:{}", config.host, config.port);

    device
        .connection
        .connect(RECEIVER_DESTINATION_ID)
        .map_err(|e| NightingaleError::Other(format!("chromecast connection failed: {e:?}")))?;
    device.heartbeat.ping().ok();
    info!("[chromecast] receiver connection established");

    stop_running_apps(&device);

    match config.receiver_app_id.as_deref() {
        Some(app_id) => cast_via_custom_receiver(&device, app_id, song, guide_volume),
        None => cast_via_default_media_receiver(&device, config, song),
    }
}

/// `LAUNCH` on an app_id that's already running is effectively a no-op from
/// the device's perspective -- it doesn't go through a genuine idle ->
/// launched transition. That transition is what triggers the Chromecast's
/// HDMI-CEC "become active input" signal to the TV, so skipping it (e.g.
/// casting again while the receiver app is already idling from a previous
/// cast, or after the TV was manually switched to a different input) leaves
/// playback running but never brings the TV back to it -- media genuinely
/// plays, confirmed by `Status`, just not on screen. Explicitly stopping
/// whatever's already running first forces a real relaunch -- and therefore
/// a real CEC trigger -- every single cast, not just the first one after the
/// device was idle. Shared by both receiver paths below.
fn stop_running_apps(device: &CastDevice) {
    match device.receiver.get_status() {
        Ok(status) => {
            for existing in &status.applications {
                info!(
                    "[chromecast] stopping already-running app {:?} (session={}) before relaunch",
                    existing.display_name, existing.session_id
                );
                if let Err(e) = device.receiver.stop_app(existing.session_id.as_str()) {
                    warn!("[chromecast] failed to stop existing app session, continuing anyway: {e:?}");
                }
            }
        }
        Err(e) => warn!("[chromecast] get_status before launch failed, continuing anyway: {e:?}"),
    }
}

/// Original casting path: launch Google's stock `DefaultMediaReceiver` and
/// hand it a URL via `/api/asset` (either the song's raw audio, or a
/// pre-rendered karaoke-video MP4 when `config.karaoke_video`).
fn cast_via_default_media_receiver(
    device: &CastDevice,
    config: &ChromecastConfig,
    song: &Song,
) -> Result<(), NightingaleError> {
    let base_url = server_base_url(config)?;

    let (media_path, content_type) = if config.karaoke_video {
        info!("[chromecast] karaoke_video enabled; rendering/reusing cached karaoke video");
        let video_path = crate::karaoke_video::best_karaoke_video_path(&song.file_hash)?;
        (video_path, "video/mp4".to_string())
    } else {
        (
            song.path.clone(),
            mime_guess::from_path(&song.path)
                .first_or_octet_stream()
                .to_string(),
        )
    };

    let content_id = format!(
        "{base_url}/api/asset?path={}",
        urlencoding::encode(&media_path.to_string_lossy())
    );
    info!("[chromecast] content_id={content_id} content_type={content_type}");

    let app = device
        .receiver
        .launch_app(&CastDeviceApp::DefaultMediaReceiver)
        .map_err(|e| NightingaleError::Other(format!("chromecast app launch failed: {e:?}")))?;
    info!(
        "[chromecast] launched app transport_id={} session_id={}",
        app.transport_id, app.session_id
    );

    device
        .connection
        .connect(app.transport_id.as_str())
        .map_err(|e| NightingaleError::Other(format!("chromecast transport connect failed: {e:?}")))?;
    info!("[chromecast] transport connection established");

    let status = device
        .media
        .load(
            app.transport_id.as_str(),
            app.session_id.as_str(),
            &Media {
                content_id,
                content_type,
                stream_type: StreamType::Buffered,
                duration: None,
                metadata: None,
            },
        )
        .map_err(|e| NightingaleError::Other(format!("chromecast media load failed: {e:?}")))?;
    info!("[chromecast] load response: {status:?}");

    // `load` can come back as a normal (non-error) response whose entry
    // reports the receiver immediately gave up on the content -- e.g. it
    // couldn't reach `content_id` or didn't like `content_type`. Treat that
    // the same as a hard failure instead of silently reporting success.
    if let Some(entry) = status.entries.first() {
        if entry.player_state == PlayerState::Idle && entry.idle_reason == Some(IdleReason::Error)
        {
            return Err(NightingaleError::Other(
                "chromecast reported the load failed (player_state=IDLE, idle_reason=ERROR) -- \
                 check the device can actually reach content_id over the network"
                    .to_string(),
            ));
        }
    } else {
        warn!("[chromecast] load response had no status entries; can't confirm playback started");
    }

    // Give the receiver a moment to process the load before we drop the
    // connection -- playback keeps going on the device after we disconnect,
    // but disconnecting mid-handshake has been observed to abort the load.
    std::thread::sleep(Duration::from_millis(500));

    Ok(())
}

/// Builds the `Load` message broadcast to the custom receiver -- split out
/// from `cast_via_custom_receiver` so it's unit-testable without a device
/// connection.
fn build_load_message(song: &Song, guide_volume: Option<f64>) -> CastReceiverMessage {
    CastReceiverMessage::Load {
        file_hash: song.file_hash.clone(),
        guide_volume: guide_volume.map(|v| v.clamp(0.0, 1.0)),
    }
}

/// Custom-receiver casting path: launch our own receiver by `app_id` and
/// tell it what to play over `crate::cast_protocol::CAST_NAMESPACE` instead
/// of `media.load`. The receiver independently fetches the transcript,
/// audio stems, and background asset same-origin against this server once
/// it has `file_hash` -- nothing else needs to cross the Cast connection,
/// so we disconnect right after broadcasting (same fire-and-forget shape as
/// the DefaultMediaReceiver path above).
fn cast_via_custom_receiver(
    device: &CastDevice,
    app_id: &str,
    song: &Song,
    guide_volume: Option<f64>,
) -> Result<(), NightingaleError> {
    let app = device
        .receiver
        .launch_app(&CastDeviceApp::Custom(app_id.to_string()))
        .map_err(|e| NightingaleError::Other(format!("custom receiver launch failed: {e:?}")))?;
    info!(
        "[chromecast] launched custom receiver app_id={app_id} transport_id={} session_id={}",
        app.transport_id, app.session_id
    );

    device
        .connection
        .connect(app.transport_id.as_str())
        .map_err(|e| NightingaleError::Other(format!("chromecast transport connect failed: {e:?}")))?;
    info!("[chromecast] transport connection established");

    std::thread::sleep(Duration::from_millis(RECEIVER_BOOTSTRAP_DELAY_MS));

    let message = build_load_message(song, guide_volume);
    device
        .receiver
        .broadcast_message(CAST_NAMESPACE, &message)
        .map_err(|e| NightingaleError::Other(format!("chromecast broadcast_message failed: {e:?}")))?;
    info!("[chromecast] broadcast load message: {message:?}");

    std::thread::sleep(Duration::from_millis(500));

    Ok(())
}
