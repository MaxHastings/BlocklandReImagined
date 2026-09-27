//! Real output through cpal (feature `cpal-output`).
//!
//! The cpal stream is created and owned by a dedicated thread so the runtime
//! handle stays `Send` on every platform. The mixer lives inside the device
//! callback. Nothing here runs unless the caller explicitly asks for
//! `OutputKind::Device`.

use std::sync::mpsc;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::engine::{Engine, EngineConfig};
use crate::error::AudioError;

pub(crate) struct DeviceHost {
    shutdown: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl DeviceHost {
    pub(crate) fn open(
        mut cfg: EngineConfig,
        make_engine: impl FnOnce(EngineConfig) -> Engine + Send + 'static,
    ) -> Result<(Self, u32, u16), AudioError> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(u32, u16), AudioError>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
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
                let mut stream_cfg = supported.config();
                // The mixer writes stereo (or mono); extra device channels get silence.
                cfg.sample_rate = stream_cfg.sample_rate;
                let device_channels = stream_cfg.channels.max(1);
                cfg.channels = device_channels.min(2);
                stream_cfg.buffer_size = cpal::BufferSize::Default;
                let engine = make_engine(cfg.clone());
                let result = match supported.sample_format() {
                    SampleFormat::F32 => {
                        build::<f32>(&device, &stream_cfg, engine, device_channels)
                    }
                    SampleFormat::I16 => {
                        build::<i16>(&device, &stream_cfg, engine, device_channels)
                    }
                    SampleFormat::U16 => {
                        build::<u16>(&device, &stream_cfg, engine, device_channels)
                    }
                    SampleFormat::I32 => {
                        build::<i32>(&device, &stream_cfg, engine, device_channels)
                    }
                    other => Err(AudioError::Device(format!(
                        "unsupported device sample format {other:?}"
                    ))),
                };
                let stream = match result {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ = ready_tx.send(Err(AudioError::Device(e.to_string())));
                    return;
                }
                let _ = ready_tx.send(Ok((cfg.sample_rate, cfg.channels)));
                // Keep the stream alive until the runtime is dropped.
                let _ = stop_rx.recv();
                drop(stream);
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok((sr, ch))) => Ok((
                Self {
                    shutdown: Some(stop_tx),
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
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut engine: Engine,
    device_channels: u16,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let mix_ch = usize::from(engine.config().channels.max(1));
    let dev_ch = usize::from(device_channels.max(1));
    // Preallocated; the callback never grows it.
    let mut mix = vec![0.0f32; 4096 * mix_ch];
    let silence = T::from_sample(0.0f32);
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |data: &mut [T], _| {
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
            |err| {
                // Device errors (e.g. unplugged headset) are reported, never panicked on.
                eprintln!("bri-audio: output stream error: {err}");
            },
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))
}
