//! The microphone and speaker streams of the voice chat (feature
//! `cpal-output`).
//!
//! Like the game's own output ([`crate::device`]) the cpal streams live on a
//! dedicated thread, so [`VoiceChat`] stays `Send` on every platform. Voice
//! plays through its own output stream beside the game's mixer: the two never
//! share a lock, and a machine that cannot open a second stream loses the
//! voice chat, never the game's sound. A stream that fails sets
//! [`Shared::is_broken`]; the owner opens the voice chat again.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, TryLockError, mpsc};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};

use crate::error::AudioError;
use crate::spatial::{Listener, Vec3};
use crate::voice::{Activation, Downsampler, Encoder, Mixer, SAMPLE_RATE, Settings, Shared};

/// Finished frames waiting for the game thread; a stalled game drops the
/// newest rather than growing.
const OUTGOING_FRAMES: usize = 64;

/// What the thread that owns the streams reports once they are open.
struct Opened {
    output_rate: u32,
    has_output: bool,
    has_input: bool,
}

pub struct VoiceChat {
    shared: Arc<Shared>,
    mixer: Arc<Mutex<Mixer>>,
    frames: mpsc::Receiver<Vec<u8>>,
    opened: Opened,
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl VoiceChat {
    /// Open the microphone `input_device` names (the default one when None,
    /// or when no such device is plugged in) and the default speakers. Either
    /// may be missing (a machine with no microphone can still listen); both
    /// missing is an error. Disabled voice activity opens no microphone at all.
    pub fn open(settings: Settings, input_device: Option<String>) -> Result<Self, AudioError> {
        let shared = Arc::new(Shared::new(settings));
        let mixer = Arc::new(Mutex::new(Mixer::new(shared.clone())));
        let (frames_tx, frames) = mpsc::sync_channel(OUTGOING_FRAMES);
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Opened, AudioError>>();
        let (stop, stop_rx) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("bri-voice".into())
            .spawn({
                let shared = shared.clone();
                let mixer = mixer.clone();
                move || {
                    let host = cpal::default_host();
                    let output = open_output(&host, &mixer, &shared);
                    let input = if settings.activation == Activation::Disabled {
                        Err(AudioError::NoDevice)
                    } else {
                        open_input(&host, &shared, frames_tx, input_device.as_deref())
                    };
                    let (output_rate, has_output) = match &output {
                        Ok((_, rate)) => (*rate, true),
                        Err(_) => (0, false),
                    };
                    let has_input = input.is_ok();
                    if !has_output && !has_input {
                        let error = output.err().or(input.err()).unwrap_or(AudioError::NoDevice);
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                    let _ = ready_tx.send(Ok(Opened {
                        output_rate,
                        has_output,
                        has_input,
                    }));
                    // The streams play until told to stop.
                    let _ = stop_rx.recv();
                    drop((output, input));
                }
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(opened)) => Ok(Self {
                shared,
                mixer,
                frames,
                opened,
                stop,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(AudioError::Device(
                    "voice thread exited during startup".into(),
                ))
            }
        }
    }

    pub fn apply_settings(&self, settings: Settings) {
        self.shared.apply(settings);
    }
    /// The push-to-talk control went down or up.
    pub fn set_talking(&self, talking: bool) {
        self.shared.set_talking(talking);
    }
    /// Whether a microphone was found.
    pub fn has_input(&self) -> bool {
        self.opened.has_input
    }
    /// Whether speakers were found.
    pub fn has_output(&self) -> bool {
        self.opened.has_output
    }
    /// Whether frames are being sent right now.
    pub fn transmitting(&self) -> bool {
        self.shared.transmitting()
    }
    /// Whether the microphone heard a voice a moment ago, whether or not it
    /// is being sent (a meter shows it while Push To Talk is held).
    pub fn activity(&self) -> bool {
        self.shared.activity()
    }
    /// The microphone's recent level, 0..=1.
    pub fn level(&self) -> f32 {
        self.shared.level()
    }
    /// A device stream failed: drop this and open the voice chat again.
    pub fn is_broken(&self) -> bool {
        self.shared.broken.load(Ordering::Relaxed)
    }
    pub fn set_listener(&self, listener: Listener) {
        lock(&self.mixer).set_listener(listener);
    }
    /// A voice frame from `speaker`, who stands at `position`.
    pub fn hear(&self, speaker: u64, position: Vec3, sequence: u16, payload: &[u8]) {
        lock(&self.mixer).hear(speaker, position, sequence, payload);
    }
    /// Players heard in the last moment.
    pub fn speaking(&self) -> Vec<u64> {
        lock(&self.mixer).speaking(self.opened.output_rate.max(1))
    }
    /// Move the frames the microphone finished into `out`, oldest first.
    pub fn take_frames(&self, out: &mut Vec<Vec<u8>>) {
        out.extend(self.frames.try_iter());
    }
}

impl Drop for VoiceChat {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|d| d.name().to_owned())
        .filter(|n| !n.is_empty())
}

/// The capture callback currently supports these conversions into normalized f32.
/// Keep this check beside selection so the menu never advertises a device that
/// this backend path would immediately reject.
fn supported_input_format(format: SampleFormat) -> bool {
    matches!(
        format,
        SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16 | SampleFormat::I32
    )
}

fn usable_input_config(channels: u16, rate: u32, format: SampleFormat) -> bool {
    channels > 0 && rate >= SAMPLE_RATE && supported_input_format(format)
}

/// Prefer the backend default when this capture implementation can consume it.
/// Some devices report a default format (for example 24-bit integer) that the
/// callback does not convert, despite offering another usable format. In that
/// case pick a supported rate/format rather than hiding a valid microphone.
fn input_config(device: &cpal::Device) -> Option<cpal::SupportedStreamConfig> {
    if let Some(config) = device
        .default_input_config()
        .ok()
        .filter(|c| usable_input_config(c.channels(), c.sample_rate(), c.sample_format()))
    {
        return Some(config);
    }

    device
        .supported_input_configs()
        .ok()?
        .filter(|c| {
            c.channels() > 0
                && c.max_sample_rate() >= SAMPLE_RATE
                && supported_input_format(c.sample_format())
        })
        .filter_map(|c| {
            let min = c.min_sample_rate();
            let max = c.max_sample_rate();
            let rate = if min <= 48_000 && max >= 48_000 {
                48_000
            } else if min <= 44_100 && max >= 44_100 {
                44_100
            } else {
                max
            };
            c.try_with_sample_rate(rate)
        })
        .max_by_key(|c| {
            let rate = c.sample_rate();
            let common_rate = if rate == 48_000 {
                3
            } else if rate == 44_100 {
                2
            } else {
                1
            };
            let format = match c.sample_format() {
                SampleFormat::F32 => 4,
                SampleFormat::I32 => 3,
                SampleFormat::I16 => 2,
                SampleFormat::U16 => 1,
                _ => 0,
            };
            (common_rate, c.channels() == 1, format, rate)
        })
}

/// The names of the microphones and other input devices, in the order the
/// system lists them, without duplicates. Empty when none can be listed.
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut names: Vec<String> = host
        .input_devices()
        .into_iter()
        .flatten()
        .filter_map(|device| {
            let name = device_name(&device)?;
            // Enumerate only devices the current backend says can provide a
            // usable configuration for this capture path. Do not classify by
            // name: virtual microphones and loopback-backed inputs can be
            // legitimate voice sources. This probes capabilities, not streams.
            input_config(&device).map(|_| name)
        })
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names
}

fn lock(mixer: &Mutex<Mixer>) -> std::sync::MutexGuard<'_, Mixer> {
    mixer.lock().unwrap_or_else(|e| e.into_inner())
}

fn failed(shared: &Arc<Shared>, what: &'static str, err: &cpal::Error) {
    // Any stream callback error may stop capture without another callback to
    // clear the VAD/level atomics. Reopen instead of leaving a speaker stuck.
    shared.broken.store(true, Ordering::Relaxed);
    eprintln!("bri-voice: {what} stream error: {err}");
}

fn open_output(
    host: &cpal::Host,
    mixer: &Arc<Mutex<Mixer>>,
    shared: &Arc<Shared>,
) -> Result<(cpal::Stream, u32), AudioError> {
    let device = host.default_output_device().ok_or(AudioError::NoDevice)?;
    let supported = device
        .default_output_config()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    let rate = supported.sample_rate();
    let mut config = supported.config();
    config.buffer_size = cpal::BufferSize::Default;
    let channels = usize::from(config.channels.max(1));
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_output::<f32>(&device, &config, mixer, shared, channels, rate),
        SampleFormat::I16 => build_output::<i16>(&device, &config, mixer, shared, channels, rate),
        SampleFormat::U16 => build_output::<u16>(&device, &config, mixer, shared, channels, rate),
        SampleFormat::I32 => build_output::<i32>(&device, &config, mixer, shared, channels, rate),
        other => Err(AudioError::Device(format!(
            "unsupported output sample format {other:?}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    Ok((stream, rate))
}

fn build_output<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mixer: &Arc<Mutex<Mixer>>,
    shared: &Arc<Shared>,
    device_channels: usize,
    rate: u32,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let mix_channels = device_channels.min(2);
    let mut mix = vec![0.0f32; 4096 * mix_channels];
    let silence = T::from_sample(0.0f32);
    let mixer = mixer.clone();
    let shared = shared.clone();
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |data: &mut [T], _| {
                let mut mixer = match mixer.try_lock() {
                    Ok(mixer) => mixer,
                    Err(TryLockError::Poisoned(e)) => e.into_inner(),
                    Err(TryLockError::WouldBlock) => {
                        data.fill(silence);
                        return;
                    }
                };
                let total = data.len() / device_channels;
                let mut done = 0;
                while done < total {
                    let n = (total - done).min(mix.len() / mix_channels);
                    let block = &mut mix[..n * mix_channels];
                    mixer.render(block, mix_channels, rate);
                    for f in 0..n {
                        let at = (done + f) * device_channels;
                        for (c, out) in data[at..at + device_channels].iter_mut().enumerate() {
                            *out = if c < mix_channels {
                                T::from_sample(block[f * mix_channels + c])
                            } else {
                                silence
                            };
                        }
                    }
                    done += n;
                }
            },
            move |err| failed(&shared, "output", &err),
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))
}

fn open_input(
    host: &cpal::Host,
    shared: &Arc<Shared>,
    frames: mpsc::SyncSender<Vec<u8>>,
    wanted: Option<&str>,
) -> Result<cpal::Stream, AudioError> {
    let usable = |device: &cpal::Device| input_config(device).is_some();
    let device = wanted
        .and_then(|name| {
            host.input_devices()
                .ok()?
                .find(|d| device_name(d).as_deref() == Some(name) && usable(d))
        })
        // A saved input can disappear or become unsupported between listing
        // and opening. In that case fall back to the backend's usable default.
        .or_else(|| host.default_input_device().filter(usable))
        .ok_or(AudioError::NoDevice)?;
    let supported = input_config(&device).ok_or(AudioError::NoDevice)?;
    let rate = supported.sample_rate();
    let mut config = supported.config();
    config.buffer_size = cpal::BufferSize::Default;
    let channels = usize::from(config.channels.max(1));
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_input::<f32>(&device, &config, shared, frames, channels, rate),
        SampleFormat::I16 => build_input::<i16>(&device, &config, shared, frames, channels, rate),
        SampleFormat::U16 => build_input::<u16>(&device, &config, shared, frames, channels, rate),
        SampleFormat::I32 => build_input::<i32>(&device, &config, shared, frames, channels, rate),
        other => Err(AudioError::Device(format!(
            "unsupported input sample format {other:?}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    Ok(stream)
}

fn build_input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: &Arc<Shared>,
    frames: mpsc::SyncSender<Vec<u8>>,
    channels: usize,
    rate: u32,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let mut down = Downsampler::new(rate);
    let mut encoder = Encoder::new(shared.clone());
    let mut voice: Vec<f32> = Vec::with_capacity(4096);
    let error_shared = shared.clone();
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                voice.clear();
                for frame in data.chunks(channels) {
                    let sum: f32 = frame.iter().map(|&s| f32::from_sample(s)).sum();
                    if let Some(sample) = down.push(sum / frame.len() as f32) {
                        voice.push(sample);
                    }
                }
                // A full queue means the game thread stalled; the frame is
                // lost, which is what a stalled game would have done anyway.
                encoder.feed(&voice, &mut |frame| {
                    let _ = frames.try_send(frame);
                });
            },
            move |err| failed(&error_shared, "input", &err),
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))
}

#[cfg(test)]
mod input_device_tests {
    use super::*;

    #[test]
    fn input_list_filter_matches_capture_support_without_name_heuristics() {
        assert!(usable_input_config(1, 16_000, SampleFormat::F32));
        assert!(usable_input_config(2, 44_100, SampleFormat::I16));
        assert!(usable_input_config(1, 384_000, SampleFormat::I32));
        assert!(!usable_input_config(0, 48_000, SampleFormat::F32));
        assert!(!usable_input_config(1, 8_000, SampleFormat::F32));
        assert!(!usable_input_config(2, 48_000, SampleFormat::I24));
    }
}
