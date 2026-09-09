use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tauri::ipc::Channel;
use tracing::{info, warn};
use ts_rs::TS;

/// Worker drains the cpal queue in fixed-size chunks before forwarding to the
/// JS side; smaller chunks lower IPC latency at the cost of more sends/sec.
const SAMPLE_CHUNK: usize = 512;
const AUDIO_QUEUE_CAP: usize = 24_000;
const PCM_QUEUE_CAP: usize = 24_000;
const MONITOR_START_BUFFER: usize = 512;
const DEFAULT_MONITOR_GAIN: f32 = 0.65;
const MAX_MONITOR_GAIN: f32 = 2.0;

static MONITOR_GAIN_BITS: AtomicU32 = AtomicU32::new(DEFAULT_MONITOR_GAIN.to_bits());

type SampleSink = Arc<dyn Fn(&[f32]) + Send + Sync>;
type SampleSource = Arc<Mutex<Box<dyn FnMut() -> f32 + Send>>>;

fn monitor_gain() -> f32 {
    f32::from_bits(MONITOR_GAIN_BITS.load(Ordering::Relaxed))
}

pub(crate) fn set_monitor_gain(gain: f32) {
    let clamped = gain.clamp(0.0, MAX_MONITOR_GAIN);
    MONITOR_GAIN_BITS.store(clamped.to_bits(), Ordering::Relaxed);
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub(crate) struct MicrophoneInfo {
    pub id: String,
    pub name: String,
    pub host: String,
}

/// Mono PCM frame streamed from Rust to JS. JS owns all DSP (pitch, reactive
/// analysis) and runs it on a sliding window built from these frames.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub(crate) struct MicSampleFrame {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(default)]
#[derive(Default)]
pub(crate) struct MicCaptureOptions {
    pub emit_audio: bool,
}

fn device_display_name(device: &cpal::Device) -> String {
    let Ok(desc) = device.description() else {
        return "(unknown)".into();
    };
    if let Some(friendly) = desc.extended().first() {
        return friendly.clone();
    }
    desc.to_string()
}

/// Returns all available audio host APIs on this platform with human-readable labels.
/// On Windows: includes both WASAPI and ASIO (when available)
/// On other platforms: uses the platform's default audio API
fn audio_hosts() -> Vec<(cpal::HostId, &'static str)> {
    cpal::available_hosts()
        .into_iter()
        .map(|id| {
            let label = match id {
                #[cfg(windows)]
                cpal::HostId::Wasapi => "WASAPI",
                #[cfg(windows)]
                cpal::HostId::Asio => "ASIO",
                #[cfg(not(windows))]
                _ => id.name(),
            };
            (id, label)
        })
        .collect()
}

/// Discovers all available input microphones across all audio host APIs.
/// Devices retain their host-qualified IDs so identically named WASAPI, ASIO,
/// and virtual inputs remain independently selectable.
#[tauri::command]
pub(crate) fn list_microphones() -> Result<Vec<MicrophoneInfo>, String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let mut errors = Vec::new();

    for (host_id, host_name) in audio_hosts() {
        let Ok(host) = cpal::host_from_id(host_id) else {
            continue;
        };
        let devices = match host.input_devices() {
            Ok(devices) => devices,
            Err(error) => {
                errors.push(format!("{host_name}: {error}"));
                continue;
            }
        };

        for device in devices {
            if device.default_input_config().is_err() {
                continue;
            }

            let name = device_display_name(&device);
            let raw_id = device
                .id()
                .map(|id| id.to_string())
                .unwrap_or_else(|_| name.clone());
            let id = format!("{host_name}:{raw_id}");
            if seen.insert(id.clone()) {
                out.push(MicrophoneInfo {
                    id,
                    name,
                    host: host_name.to_string(),
                });
            }
        }
    }

    if out.is_empty() && !errors.is_empty() {
        Err(format!("input devices: {}", errors.join("; ")))
    } else {
        Ok(out)
    }
}

fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / i16::MAX as f32
}

fn i32_to_f32(sample: i32) -> f32 {
    sample as f32 / i32::MAX as f32
}

fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

fn f32_to_u16(sample: f32) -> u16 {
    ((sample.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
}

fn f32_to_i32(sample: f32) -> i32 {
    (sample.clamp(-1.0, 1.0) * i32::MAX as f32) as i32
}

fn f32_to_u32(sample: f32) -> u32 {
    ((sample.clamp(-1.0, 1.0) * 0.5 + 0.5) * u32::MAX as f32) as u32
}

fn push_mapped_input<T, F>(data: &[T], push: &SampleSink, mut map: F)
where
    T: Copy,
    F: FnMut(T) -> f32,
{
    let floats: Vec<f32> = data.iter().copied().map(&mut map).collect();
    push(&floats);
}

fn strongest_input_channel(data: &[f32], channels: usize) -> usize {
    let mut energy = vec![0.0; channels];
    for frame in data.chunks(channels) {
        for (channel, sample) in frame.iter().enumerate() {
            energy[channel] += sample * sample;
        }
    }

    energy
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map_or(0, |(channel, _)| channel)
}

fn write_output_frames<T, F>(
    data: &mut [T],
    channels: usize,
    next_sample: &SampleSource,
    mut map: F,
) where
    T: Copy,
    F: FnMut(f32) -> T,
{
    let Ok(mut next_sample) = next_sample.try_lock() else {
        for sample in data {
            *sample = map(0.0);
        }
        return;
    };

    for frame in data.chunks_mut(channels) {
        let out_sample = map(next_sample());
        for out in frame {
            *out = out_sample;
        }
    }
}

static MIC_RUNNING: AtomicBool = AtomicBool::new(false);
static MIC_SHUTDOWN: once_cell::sync::Lazy<Arc<AtomicBool>> =
    once_cell::sync::Lazy::new(|| Arc::new(AtomicBool::new(false)));
static MONITOR_ENABLED: AtomicBool = AtomicBool::new(false);
static MIC_CHANNEL: once_cell::sync::Lazy<Arc<Mutex<Option<Channel<MicSampleFrame>>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));
static MIC_THREAD: once_cell::sync::Lazy<Mutex<Option<JoinHandle<()>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));
/// Serializes start/stop so concurrent IPC dispatches can't interleave a
/// teardown with a fresh spawn.
static MIC_OP_LOCK: once_cell::sync::Lazy<Mutex<()>> =
    once_cell::sync::Lazy::new(|| Mutex::new(()));

fn take_mic_thread() -> Option<JoinHandle<()>> {
    MIC_THREAD.lock().unwrap_or_else(|p| p.into_inner()).take()
}

fn stop_internal() {
    MIC_SHUTDOWN.store(true, Ordering::SeqCst);
    MONITOR_ENABLED.store(false, Ordering::SeqCst);
    /*
     * Drop the channel before joining: this triggers Tauri's `Channel` Drop,
     * which sends `{end: true}` to JS so the callback id is unregistered
     * cleanly. The mic loop also gets `None` next iteration and stops sending.
     */
    if let Ok(mut slot) = MIC_CHANNEL.lock() {
        *slot = None;
    }
    if let Some(handle) = take_mic_thread() {
        let _ = handle.join();
    }
    MIC_RUNNING.store(false, Ordering::SeqCst);
}

fn find_device(preferred: Option<&str>) -> Result<(cpal::Device, String), String> {
    if let Some(preference) = preferred {
        let hosts = audio_hosts();
        let qualified_preference = preference.split_once(':').filter(|(preferred_host, _)| {
            hosts
                .iter()
                .any(|(_, host_name)| host_name == preferred_host)
        });

        for (host_id, host_name) in hosts {
            if qualified_preference.is_some_and(|(preferred_host, _)| preferred_host != host_name) {
                continue;
            }

            let Ok(host) = cpal::host_from_id(host_id) else {
                continue;
            };
            let Ok(devices) = host.input_devices() else {
                continue;
            };
            for dev in devices {
                let display_name = device_display_name(&dev);
                let raw_id = dev
                    .id()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|_| display_name.clone());
                let id_matches = qualified_preference.map_or_else(
                    || raw_id == preference,
                    |(_, preferred_id)| raw_id == preferred_id,
                );
                // Name matching preserves preferences saved by older versions.
                if id_matches || (qualified_preference.is_none() && display_name == preference) {
                    return Ok((dev, display_name));
                }
            }
        }
        return Err(format!("Microphone '{preference}' not found"));
    }

    // No preference: use the system default input device
    let device = cpal::default_host()
        .default_input_device()
        .ok_or_else(|| "No default microphone found".to_string())?;
    let name = device_display_name(&device);
    Ok((device, name))
}

#[tauri::command]
pub(crate) fn start_mic_capture(
    preferred: Option<String>,
    options: Option<MicCaptureOptions>,
    on_samples: Channel<MicSampleFrame>,
) -> Result<String, String> {
    let _guard = MIC_OP_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    /*
     * Always tear down any prior session first. We used to short-circuit with
     * "already running" if MIC_RUNNING was true, but that hit a race where
     * the previous worker had already broken out on shutdown but not yet
     * cleared MIC_RUNNING — the new start would skip spawning, and capture
     * would silently die for the rest of the session.
     */
    stop_internal();

    let next_options = options.unwrap_or_default();
    MONITOR_ENABLED.store(next_options.emit_audio, Ordering::SeqCst);

    let (device, name) = match find_device(preferred.as_deref()) {
        Ok(pair) => pair,
        Err(e) => {
            MONITOR_ENABLED.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };

    if let Ok(mut slot) = MIC_CHANNEL.lock() {
        *slot = Some(on_samples);
    }

    MIC_SHUTDOWN.store(false, Ordering::SeqCst);
    MIC_RUNNING.store(true, Ordering::SeqCst);

    let device_name = name.clone();
    let shutdown = Arc::clone(&MIC_SHUTDOWN);

    let handle = std::thread::spawn(move || {
        run_mic_loop(device, &name, shutdown);
        MIC_RUNNING.store(false, Ordering::SeqCst);
    });

    if let Ok(mut slot) = MIC_THREAD.lock() {
        *slot = Some(handle);
    }

    Ok(device_name)
}

fn try_build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    pcm_shared: Arc<Mutex<VecDeque<f32>>>,
    audio_shared: Arc<Mutex<VecDeque<f32>>>,
) -> Option<cpal::Stream> {
    let ch = config.channels as usize;
    let push_samples: SampleSink = {
        let pcm_cb = Arc::clone(&pcm_shared);
        let audio_cb = Arc::clone(&audio_shared);
        Arc::new(move |data: &[f32]| {
            let mut mono_samples = Vec::with_capacity(data.len() / ch.max(1));
            let active_channel = strongest_input_channel(data, ch);
            for frame in data.chunks(ch) {
                mono_samples.push(frame.get(active_channel).copied().unwrap_or(0.0));
            }

            if let Ok(mut q) = pcm_cb.try_lock() {
                for sample in &mono_samples {
                    q.push_back(*sample);
                }
                while q.len() > PCM_QUEUE_CAP {
                    q.pop_front();
                }
            }

            if MONITOR_ENABLED.load(Ordering::Relaxed) {
                if let Ok(mut q) = audio_cb.try_lock() {
                    for sample in &mono_samples {
                        q.push_back(*sample);
                    }
                    while q.len() > AUDIO_QUEUE_CAP {
                        q.pop_front();
                    }
                }
            }
        })
    };

    use cpal::SampleFormat;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let push = push_samples.clone();
            device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| push(data),
                |err| warn!("[mic] stream error: {err}"),
                None,
            )
        }
        SampleFormat::I16 => {
            let push = push_samples.clone();
            device.build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    push_mapped_input(data, &push, i16_to_f32);
                },
                |err| warn!("[mic] stream error: {err}"),
                None,
            )
        }
        SampleFormat::I32 => {
            let push = push_samples.clone();
            device.build_input_stream(
                config,
                move |data: &[i32], _: &cpal::InputCallbackInfo| {
                    push_mapped_input(data, &push, i32_to_f32);
                },
                |err| warn!("[mic] stream error: {err}"),
                None,
            )
        }
        _ => return None,
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            warn!("[mic] build stream failed: {e}");
            return None;
        }
    };

    if let Err(e) = stream.play() {
        warn!("[mic] play failed: {e}");
        return None;
    }

    Some(stream)
}

fn try_build_output_stream(
    device: &cpal::Device,
    input_sample_rate: cpal::SampleRate,
    audio_shared: Arc<Mutex<VecDeque<f32>>>,
) -> Option<cpal::Stream> {
    let default_cfg = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            warn!("[mic] output config error: {e}");
            return None;
        }
    };
    let sample_format = default_cfg.sample_format();
    let config = cpal::StreamConfig {
        channels: default_cfg.channels(),
        sample_rate: default_cfg.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };
    let ch = config.channels as usize;
    let sample_rate_ratio = f64::from(input_sample_rate) / f64::from(config.sample_rate);

    let next_sample: SampleSource = {
        let audio_shared = Arc::clone(&audio_shared);
        let mut current = 0.0;
        let mut next = 0.0;
        let mut position = 0.0;
        let mut initialized = false;
        let mut buffered = VecDeque::new();
        Arc::new(Mutex::new(Box::new(move || -> f32 {
            if !MONITOR_ENABLED.load(Ordering::Relaxed) {
                return 0.0;
            }

            if !initialized {
                let Ok(mut queue) = audio_shared.try_lock() else {
                    return 0.0;
                };
                if queue.len() < MONITOR_START_BUFFER {
                    return 0.0;
                }
                buffered.extend(queue.drain(..));
                current = buffered.pop_front().unwrap_or(0.0);
                next = buffered.pop_front().unwrap_or(0.0);
                initialized = true;
            }

            let sample = current + (next - current) * position as f32;
            position += sample_rate_ratio;
            while position >= 1.0 {
                current = next;
                if buffered.is_empty() {
                    if let Ok(mut queue) = audio_shared.try_lock() {
                        buffered.extend(queue.drain(..));
                    }
                }
                next = buffered.pop_front().unwrap_or(0.0);
                position -= 1.0;
            }

            sample * monitor_gain()
        })))
    };

    use cpal::SampleFormat;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let next = Arc::clone(&next_sample);
            device.build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    write_output_frames(data, ch, &next, |sample| sample);
                },
                |err| warn!("[mic] output stream error: {err}"),
                None,
            )
        }
        SampleFormat::I16 => {
            let next = Arc::clone(&next_sample);
            device.build_output_stream(
                &config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    write_output_frames(data, ch, &next, f32_to_i16);
                },
                |err| warn!("[mic] output stream error: {err}"),
                None,
            )
        }
        SampleFormat::U16 => {
            let next = Arc::clone(&next_sample);
            device.build_output_stream(
                &config,
                move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                    write_output_frames(data, ch, &next, f32_to_u16);
                },
                |err| warn!("[mic] output stream error: {err}"),
                None,
            )
        }
        SampleFormat::I32 => {
            let next = Arc::clone(&next_sample);
            device.build_output_stream(
                &config,
                move |data: &mut [i32], _: &cpal::OutputCallbackInfo| {
                    write_output_frames(data, ch, &next, f32_to_i32);
                },
                |err| warn!("[mic] output stream error: {err}"),
                None,
            )
        }
        SampleFormat::U32 => {
            let next = Arc::clone(&next_sample);
            device.build_output_stream(
                &config,
                move |data: &mut [u32], _: &cpal::OutputCallbackInfo| {
                    write_output_frames(data, ch, &next, f32_to_u32);
                },
                |err| warn!("[mic] output stream error: {err}"),
                None,
            )
        }
        _ => {
            warn!("[mic] unsupported output sample format: {sample_format:?}");
            return None;
        }
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            warn!("[mic] build output stream failed: {e}");
            return None;
        }
    };
    if let Err(e) = stream.play() {
        warn!("[mic] output play failed: {e}");
        return None;
    }
    Some(stream)
}

fn drain_chunk(queue: &Mutex<VecDeque<f32>>) -> Option<Vec<f32>> {
    let mut q = queue.try_lock().ok()?;
    if q.len() < SAMPLE_CHUNK {
        return None;
    }
    Some(q.drain(..SAMPLE_CHUNK).collect())
}

fn run_mic_loop(device: cpal::Device, name: &str, shutdown: Arc<AtomicBool>) {
    let default_cfg = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            warn!("[mic] '{name}' config error: {e}");
            return;
        }
    };

    let sample_format = default_cfg.sample_format();
    let config = cpal::StreamConfig {
        channels: default_cfg.channels(),
        sample_rate: default_cfg.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };
    let sr = config.sample_rate;

    info!(
        "[mic] opening '{name}': {sr} Hz, {}ch, {sample_format:?}",
        config.channels
    );

    let pcm_shared = Arc::new(Mutex::new(VecDeque::<f32>::with_capacity(PCM_QUEUE_CAP)));
    let audio_shared = Arc::new(Mutex::new(VecDeque::<f32>::with_capacity(AUDIO_QUEUE_CAP)));
    let Some(_stream) = try_build_stream(
        &device,
        &config,
        sample_format,
        Arc::clone(&pcm_shared),
        Arc::clone(&audio_shared),
    ) else {
        warn!("[mic] failed to open '{name}'");
        return;
    };
    let monitor_stream = cpal::default_host()
        .default_output_device()
        .and_then(|output_device| {
            try_build_output_stream(&output_device, sr, Arc::clone(&audio_shared))
        });
    if monitor_stream.is_none() {
        warn!("[mic] no output monitoring stream available");
    }

    info!("[mic] active: {name}");

    let sleep_dur = std::time::Duration::from_millis(4);

    loop {
        std::thread::sleep(sleep_dur);

        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        while let Some(samples) = drain_chunk(&pcm_shared) {
            let channel = MIC_CHANNEL.lock().ok().and_then(|s| s.clone());
            if let Some(channel) = channel {
                let frame = MicSampleFrame {
                    sample_rate: sr,
                    samples,
                };
                if let Err(e) = channel.send(frame) {
                    warn!("[mic] channel send failed: {e}");
                }
            }
        }
    }
}

#[tauri::command]
pub(crate) fn stop_mic_capture() {
    let _guard = MIC_OP_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    stop_internal();
}
