//! Real output through cpal (feature `cpal-output`).
//!
//! A dedicated thread owns the cpal stream so the runtime handle stays `Send`
//! on every platform. The mixer engine is shared with the device callback,
//! which only ever `try_lock`s it: the device thread touches the engine only
//! while no stream exists, so the lock is never contended during playback.
//! When the output is lost (a headset is unplugged) or the system default
//! output changes, the thread reopens the current default device with the
//! same engine: voices, volumes and the command queue survive and sound
//! resumes. Nothing here runs unless the caller explicitly asks for
//! `OutputKind::Device`.

use std::sync::{Arc, Mutex, MutexGuard, TryLockError, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, SampleFormat, SizedSample};

use crate::engine::{Engine, EngineConfig};
use crate::error::AudioError;

/// How often the device thread retries while no output device exists.
const RETRY: Duration = Duration::from_secs(2);

enum Control {
    Stop,
    /// The stream can no longer play: reopen the default device.
    Lost,
}

pub(crate) struct DeviceHost {
    control: mpsc::Sender<Control>,
    thread: Option<JoinHandle<()>>,
}

impl DeviceHost {
    pub(crate) fn open(
        mut cfg: EngineConfig,
        make_engine: impl FnOnce(EngineConfig) -> Engine + Send + 'static,
    ) -> Result<(Self, u32, u16), AudioError> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(u32, u16), AudioError>>();
        let (control, control_rx) = mpsc::channel::<Control>();
        let lost = control.clone();
        let thread = std::thread::Builder::new()
            .name("bri-audio-output".into())
            .spawn(move || {
                let host = cpal::default_host();
                let Some(device) = host.default_output_device() else {
                    let _ = ready_tx.send(Err(AudioError::NoDevice));
                    return;
                };
                let supported = match device.default_output_config() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ready_tx.send(Err(AudioError::Device(e.to_string())));
                        return;
                    }
                };
                // The mixer writes stereo (or mono); extra device channels get
                // silence. The engine keeps this layout across reopened devices.
                cfg.sample_rate = supported.sample_rate();
                cfg.channels = supported.channels().clamp(1, 2);
                let engine = Arc::new(Mutex::new(make_engine(cfg.clone())));
                let mut stream = match start(&device, supported, &engine, &lost) {
                    Ok(stream) => Some(stream),
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok((cfg.sample_rate, cfg.channels)));
                loop {
                    let wait = if stream.is_some() {
                        control_rx
                            .recv()
                            .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                    } else {
                        control_rx.recv_timeout(RETRY)
                    };
                    match wait {
                        Ok(Control::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Ok(Control::Lost) | Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Drop the dead stream first so its callback lets
                            // go of the engine before a new one takes it.
                            drop(stream.take());
                            stream = reopen(&host, &engine, &lost);
                        }
                    }
                }
                drop(stream);
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok((sr, ch))) => Ok((
                Self {
                    control,
                    thread: Some(thread),
                },
                sr,
                ch,
            )),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                Err(AudioError::Device(
                    "output thread exited during startup".into(),
                ))
            }
        }
    }
}

impl Drop for DeviceHost {
    fn drop(&mut self) {
        let _ = self.control.send(Control::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Open the current default device, preferring the engine's sample rate so a
/// replacement device changes nothing audible. None: no usable device yet.
fn reopen(
    host: &cpal::Host,
    engine: &Arc<Mutex<Engine>>,
    lost: &mpsc::Sender<Control>,
) -> Option<cpal::Stream> {
    let device = host.default_output_device()?;
    let default = device.default_output_config().ok()?;
    let rate = lock(engine).config().sample_rate;
    let supported = device
        .supported_output_configs()
        .ok()
        .and_then(|mut ranges| {
            ranges.find_map(|range| {
                (range.channels() == default.channels()
                    && range.sample_format() == default.sample_format())
                .then(|| range.try_with_sample_rate(rate))
                .flatten()
            })
        })
        .unwrap_or(default);
    lock(engine).set_sample_rate(supported.sample_rate());
    match start(&device, supported, engine, lost) {
        Ok(stream) => {
            eprintln!("bri-audio: output reopened on the default device");
            Some(stream)
        }
        Err(e) => {
            eprintln!("bri-audio: could not reopen output: {e}");
            None
        }
    }
}

/// Only the device thread blocks on the engine, and only while no stream
/// runs. A panic inside a render must not silence the game for good.
fn lock(engine: &Mutex<Engine>) -> MutexGuard<'_, Engine> {
    engine.lock().unwrap_or_else(|e| e.into_inner())
}

fn start(
    device: &cpal::Device,
    supported: cpal::SupportedStreamConfig,
    engine: &Arc<Mutex<Engine>>,
    lost: &mpsc::Sender<Control>,
) -> Result<cpal::Stream, AudioError> {
    let mut config = supported.config();
    config.buffer_size = cpal::BufferSize::Default;
    let channels = config.channels.max(1);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(device, &config, engine, channels, lost),
        SampleFormat::I16 => build::<i16>(device, &config, engine, channels, lost),
        SampleFormat::U16 => build::<u16>(device, &config, engine, channels, lost),
        SampleFormat::I32 => build::<i32>(device, &config, engine, channels, lost),
        other => Err(AudioError::Device(format!(
            "unsupported device sample format {other:?}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    Ok(stream)
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    engine: &Arc<Mutex<Engine>>,
    device_channels: u16,
    lost: &mpsc::Sender<Control>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let mix_ch = usize::from(lock(engine).config().channels.max(1));
    let dev_ch = usize::from(device_channels.max(1));
    // Preallocated; the callback never grows it.
    let mut mix = vec![0.0f32; 4096 * mix_ch];
    let silence = T::from_sample(0.0f32);
    let engine = engine.clone();
    let lost = lost.clone();
    let mut reported = false;
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |data: &mut [T], _| {
                let mut engine = match engine.try_lock() {
                    Ok(engine) => engine,
                    Err(TryLockError::Poisoned(e)) => e.into_inner(),
                    Err(TryLockError::WouldBlock) => {
                        data.fill(silence);
                        return;
                    }
                };
                let frames_total = data.len() / dev_ch;
                let mut done = 0;
                while done < frames_total {
                    let n = (frames_total - done).min(mix.len() / mix_ch);
                    let buf = &mut mix[..n * mix_ch];
                    engine.render(buf);
                    for f in 0..n {
                        let dst = &mut data[(done + f) * dev_ch..(done + f + 1) * dev_ch];
                        for (c, d) in dst.iter_mut().enumerate() {
                            *d = if c < mix_ch {
                                T::from_sample(buf[f * mix_ch + c])
                            } else {
                                silence
                            };
                        }
                    }
                    done += n;
                }
            },
            move |err| {
                // Never panic here. A stream that can no longer play (device
                // unplugged, default output changed) is reopened once.
                let dead = matches!(
                    err.kind(),
                    ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated
                );
                if dead && !reported {
                    reported = true;
                    let _ = lost.send(Control::Lost);
                }
                eprintln!("bri-audio: output stream error: {err}");
            },
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))
}
