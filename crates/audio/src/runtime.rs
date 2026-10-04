//! Game-facing runtime handle and output adapters.
//!
//! Threading: [`AudioRuntime`] lives on the game/client thread and is `Send`.
//! Commands travel to the mixer through a bounded lock-free queue; events and
//! counters travel back. With [`OutputKind::Null`] or [`OutputKind::Offline`]
//! the mixer runs inline inside [`AudioRuntime::update`] / `render_offline`
//! (no thread, no device). The device adapter (feature `cpal-output`) runs the
//! mixer in the device callback, owned by a dedicated output thread.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::bank::{SoundAsset, SoundBank};
use crate::command::{AudioEvent, Command, EntityKey, Placement, SoundHandle, VolumeControl};
use crate::engine::{Engine, EngineConfig, SharedStats};
use crate::error::AudioError;
use crate::spatial::{Listener, Vec3};

/// Runtime configuration.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub engine: EngineConfig,
    /// Capacity of the game -> mixer command queue.
    pub command_capacity: usize,
    /// Capacity of the mixer -> game event queue (overflow is counted, not fatal).
    pub event_capacity: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            command_capacity: 4096,
            event_capacity: 4096,
        }
    }
}

/// Which output adapter to use. Opening a device is always an explicit choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    /// Mix inline and discard (headless servers, tests, missing device fallback).
    Null,
    /// Mix inline and keep the rendered samples (offline WAV evidence, tests).
    Offline,
    /// The default system output device (requires feature `cpal-output`).
    Device,
}

/// Snapshot of mixer counters.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AudioStats {
    pub real_voices: u32,
    pub virtual_voices: u32,
    pub suspended_voices: u32,
    pub started: u64,
    pub finished: u64,
    pub stopped: u64,
    pub culled: u64,
    pub rejected: u64,
    pub virtualized: u64,
    pub revived: u64,
    pub commands_applied: u64,
    pub commands_dropped: u64,
    pub events_dropped: u64,
    pub frames_rendered: u64,
    pub clipped_samples: u64,
    pub non_finite_samples: u64,
    pub last_block_peak: f32,
    pub peak: f32,
}

enum Host {
    Inline {
        engine: Box<Engine>,
        capture: Option<Vec<f32>>,
        scratch: Vec<f32>,
        carry: f64,
    },
    #[cfg(feature = "cpal-output")]
    /// Held for its `Drop`, which stops the output thread.
    Device { _output: crate::device::DeviceHost },
}

pub struct AudioRuntime {
    bank: Arc<SoundBank>,
    commands: rtrb::Producer<Command>,
    events: rtrb::Consumer<AudioEvent>,
    stats: Arc<SharedStats>,
    next_handle: u64,
    commands_dropped: u64,
    host: Host,
    sample_rate: u32,
    channels: u16,
    music_enabled: bool,
}

impl AudioRuntime {
    /// Create a runtime. `OutputKind::Device` fails with [`AudioError::NoDevice`]
    /// / [`AudioError::Device`] (or `Unsupported` without the feature); callers
    /// decide whether to fall back to `Null` — see [`AudioRuntime::open_or_null`].
    pub fn new(
        bank: Arc<SoundBank>,
        config: RuntimeConfig,
        output: OutputKind,
    ) -> Result<Self, AudioError> {
        let defaults = bank.defaults().clone();
        let stats = Arc::new(SharedStats::default());
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(config.command_capacity.max(16));
        let (ev_tx, ev_rx) = rtrb::RingBuffer::new(config.event_capacity.max(16));
        let music = defaults.play_music;
        let make_engine = {
            let stats = stats.clone();
            move |engine_cfg: EngineConfig| {
                Engine::new(
                    engine_cfg,
                    cmd_rx,
                    ev_tx,
                    stats,
                    defaults.master_volume,
                    defaults.channel_volumes,
                    music,
                )
            }
        };
        let (host, sample_rate, channels) = match output {
            OutputKind::Null | OutputKind::Offline => {
                let engine = Box::new(make_engine(config.engine.clone()));
                let (sr, ch) = (config.engine.sample_rate, config.engine.channels);
                let capture = (output == OutputKind::Offline).then(Vec::new);
                (
                    Host::Inline {
                        engine,
                        capture,
                        scratch: Vec::new(),
                        carry: 0.0,
                    },
                    sr,
                    ch,
                )
            }
            OutputKind::Device => {
                #[cfg(feature = "cpal-output")]
                {
                    let (dev, sr, ch) =
                        crate::device::DeviceHost::open(config.engine.clone(), make_engine)?;
                    (Host::Device { _output: dev }, sr, ch)
                }
                #[cfg(not(feature = "cpal-output"))]
                {
                    drop(make_engine);
                    return Err(AudioError::Unsupported(
                        "built without the `cpal-output` feature",
                    ));
                }
            }
        };
        Ok(Self {
            bank,
            commands: cmd_tx,
            events: ev_rx,
            stats,
            next_handle: 1,
            commands_dropped: 0,
            host,
            sample_rate,
            channels,
            music_enabled: music,
        })
    }

    /// Try the device; on failure return a `Null` runtime plus the reason, so a
    /// missing or broken device never prevents the game from running.
    pub fn open_or_null(bank: Arc<SoundBank>, config: RuntimeConfig) -> (Self, Option<AudioError>) {
        match Self::new(bank.clone(), config.clone(), OutputKind::Device) {
            Ok(rt) => (rt, None),
            Err(e) => {
                let rt =
                    Self::new(bank, config, OutputKind::Null).expect("inline output cannot fail");
                (rt, Some(e))
            }
        }
    }

    fn send(&mut self, cmd: Command) -> Result<(), AudioError> {
        self.commands.push(cmd).map_err(|_| {
            self.commands_dropped += 1;
            AudioError::QueueFull
        })
    }

    fn alloc_handle(&mut self) -> SoundHandle {
        let h = SoundHandle(self.next_handle);
        self.next_handle += 1;
        h
    }

    /// Play a sound by stable id or vanilla datablock name.
    pub fn play(&mut self, sound: &str, placement: Placement) -> Result<SoundHandle, AudioError> {
        self.play_scaled(sound, placement, 1.0)
    }

    /// Play with an extra linear gain multiplier (1.0 = authored).
    pub fn play_scaled(
        &mut self,
        sound: &str,
        placement: Placement,
        gain_scale: f32,
    ) -> Result<SoundHandle, AudioError> {
        let asset = self.bank.resolve(sound)?.clone();
        let handle = self.alloc_handle();
        self.send(Command::Play {
            handle,
            asset,
            placement,
            gain_scale,
        })?;
        Ok(handle)
    }

    /// Play a sound that is not in the bank: an Add-On's own clip,
    /// decoded by [`crate::bank::SoundAsset::decoded`].
    pub fn play_asset(
        &mut self,
        asset: Arc<SoundAsset>,
        placement: Placement,
        gain_scale: f32,
    ) -> Result<SoundHandle, AudioError> {
        let handle = self.alloc_handle();
        self.send(Command::Play {
            handle,
            asset,
            placement,
            gain_scale,
        })?;
        Ok(handle)
    }

    /// Play the sound bound to a vanilla trigger key from the pack's trigger table.
    pub fn play_trigger(
        &mut self,
        trigger_key: &str,
        placement: Placement,
    ) -> Result<SoundHandle, AudioError> {
        let id = self
            .bank
            .trigger(trigger_key)
            .ok_or_else(|| AudioError::UnknownSound(format!("trigger {trigger_key}")))?
            .to_string();
        self.play(&id, placement)
    }

    /// Stop one sound (with a short declick fade).
    pub fn stop(&mut self, handle: SoundHandle) -> Result<(), AudioError> {
        self.send(Command::Stop {
            handle,
            fade_frames: 0,
        })
    }

    pub fn stop_with_fade(&mut self, handle: SoundHandle, seconds: f32) -> Result<(), AudioError> {
        let fade_frames = self.seconds_to_frames(seconds);
        self.send(Command::Stop {
            handle,
            fade_frames,
        })
    }

    /// Stop everything (vanilla `alxStopAll`, e.g. on disconnect).
    pub fn stop_all(&mut self) -> Result<(), AudioError> {
        self.send(Command::StopAll { fade_frames: 0 })
    }

    /// The entity was deleted: stop every sound attached to it.
    pub fn despawn(&mut self, entity: EntityKey) -> Result<(), AudioError> {
        self.send(Command::StopEntity {
            entity,
            fade_frames: 0,
        })
    }

    /// Move every sound attached to `entity`. Call once per frame per moving
    /// entity that has attached sounds.
    pub fn update_entity(&mut self, entity: EntityKey, position: Vec3) -> Result<(), AudioError> {
        self.send(Command::UpdateEntity { entity, position })
    }

    /// Move one positional sound.
    pub fn set_source_position(
        &mut self,
        handle: SoundHandle,
        position: Vec3,
    ) -> Result<(), AudioError> {
        self.send(Command::SetSourcePosition { handle, position })
    }

    /// Update the listener (normally the camera) once per frame.
    pub fn set_listener(&mut self, listener: Listener) -> Result<(), AudioError> {
        self.send(Command::SetListener(listener))
    }

    /// The windows (portals) the listener also hears through
    /// ([`crate::spatial::Window`]): the first [`crate::spatial::MAX_WINDOWS`]
    /// valid ones are kept, replacing the last set. Send only on change.
    pub fn set_windows(&mut self, windows: &[crate::spatial::Window]) -> Result<(), AudioError> {
        let kept: Box<[_]> = windows
            .iter()
            .filter(|w| w.is_valid())
            .take(crate::spatial::MAX_WINDOWS)
            .copied()
            .collect();
        self.send(Command::SetWindows(kept))
    }

    pub fn set_volume(&mut self, control: VolumeControl, value: f32) -> Result<(), AudioError> {
        let cmd = match control {
            VolumeControl::Master => Command::SetMaster(value),
            VolumeControl::Interface => Command::SetChannel {
                channel: 1,
                gain: value,
            },
            VolumeControl::Effects => Command::SetChannel {
                channel: 2,
                gain: value,
            },
            VolumeControl::Message => Command::SetChannel {
                channel: 3,
                gain: value,
            },
            VolumeControl::Music => Command::SetMusicGain(value),
            VolumeControl::Channel(c) => Command::SetChannel {
                channel: c,
                gain: value,
            },
        };
        self.send(cmd)
    }

    /// Apply `bri_ui::UiAction::SetVolume { channel, value }` from the options
    /// screen (`"master"`, `"shell"`, `"sim"`; also accepts `"music"`).
    /// Returns `Ok(false)` for an unknown channel name.
    pub fn apply_ui_volume(&mut self, channel: &str, value: f32) -> Result<bool, AudioError> {
        let control = match channel {
            "master" => VolumeControl::Master,
            "shell" | "interface" => VolumeControl::Interface,
            "sim" | "effects" => VolumeControl::Effects,
            "message" => VolumeControl::Message,
            "music" => VolumeControl::Music,
            _ => return Ok(false),
        };
        self.set_volume(control, value)?;
        Ok(true)
    }

    /// `$Pref::Audio::PlayMusic`. Disabling suspends music loops (and rejects
    /// music one-shots); enabling restarts them from the top, as vanilla does.
    pub fn set_music_enabled(&mut self, enabled: bool) -> Result<(), AudioError> {
        self.music_enabled = enabled;
        self.send(Command::SetMusicEnabled(enabled))
    }

    pub fn music_enabled(&self) -> bool {
        self.music_enabled
    }

    /// Per-frame tick. Inline outputs mix `dt` seconds of audio here (fractional
    /// frames carry over); device output just keeps running.
    pub fn update(&mut self, dt_seconds: f32) {
        if !(dt_seconds.is_finite() && dt_seconds > 0.0) {
            return;
        }
        match &mut self.host {
            Host::Inline {
                engine,
                capture,
                scratch,
                carry,
            } => {
                let exact = f64::from(dt_seconds.min(1.0)) * f64::from(self.sample_rate) + *carry;
                let frames = exact.floor() as usize;
                *carry = exact - frames as f64;
                render_inline(engine, capture, scratch, frames, self.channels);
            }
            #[cfg(feature = "cpal-output")]
            Host::Device { .. } => {}
        }
    }

    /// Render exactly `frames` frames (inline outputs only) and return them.
    /// The samples are also appended to the capture for `Offline` output.
    pub fn render_offline(&mut self, frames: usize) -> Vec<f32> {
        let ch = usize::from(self.channels);
        match &mut self.host {
            Host::Inline {
                engine, capture, ..
            } => {
                let mut out = vec![0.0f32; frames * ch];
                engine.render(&mut out);
                if let Some(c) = capture {
                    c.extend_from_slice(&out);
                }
                out
            }
            #[cfg(feature = "cpal-output")]
            Host::Device { .. } => Vec::new(),
        }
    }

    /// Take everything captured so far (`Offline` output).
    pub fn take_capture(&mut self) -> Vec<f32> {
        match &mut self.host {
            Host::Inline {
                capture: Some(c), ..
            } => std::mem::take(c),
            _ => Vec::new(),
        }
    }

    /// Drain pending mixer events.
    pub fn drain_events(&mut self) -> Vec<AudioEvent> {
        let mut v = Vec::new();
        while let Ok(e) = self.events.pop() {
            v.push(e);
        }
        v
    }

    pub fn stats(&self) -> AudioStats {
        let s = &self.stats;
        AudioStats {
            real_voices: s.real_voices.load(Ordering::Relaxed),
            virtual_voices: s.virtual_voices.load(Ordering::Relaxed),
            suspended_voices: s.suspended_voices.load(Ordering::Relaxed),
            started: s.started.load(Ordering::Relaxed),
            finished: s.finished.load(Ordering::Relaxed),
            stopped: s.stopped.load(Ordering::Relaxed),
            culled: s.culled.load(Ordering::Relaxed),
            rejected: s.rejected.load(Ordering::Relaxed),
            virtualized: s.virtualized.load(Ordering::Relaxed),
            revived: s.revived.load(Ordering::Relaxed),
            commands_applied: s.commands.load(Ordering::Relaxed),
            commands_dropped: self.commands_dropped,
            events_dropped: s.events_dropped.load(Ordering::Relaxed),
            frames_rendered: s.frames.load(Ordering::Relaxed),
            clipped_samples: s.clipped_samples.load(Ordering::Relaxed),
            non_finite_samples: s.non_finite_samples.load(Ordering::Relaxed),
            last_block_peak: f32::from_bits(s.last_block_peak.load(Ordering::Relaxed)),
            peak: f32::from_bits(s.peak.load(Ordering::Relaxed)),
        }
    }

    pub fn bank(&self) -> &Arc<SoundBank> {
        &self.bank
    }

    /// `(sample_rate, channels)` actually used by the mixer.
    pub fn output_format(&self) -> (u32, u16) {
        (self.sample_rate, self.channels)
    }

    fn seconds_to_frames(&self, seconds: f32) -> u32 {
        if seconds.is_finite() && seconds > 0.0 {
            (seconds.min(60.0) * self.sample_rate as f32) as u32
        } else {
            0
        }
    }
}

fn render_inline(
    engine: &mut Engine,
    capture: &mut Option<Vec<f32>>,
    scratch: &mut Vec<f32>,
    frames: usize,
    channels: u16,
) {
    let ch = usize::from(channels);
    let block = engine.config().block_frames.max(16);
    let mut left = frames;
    while left > 0 {
        let n = left.min(block * 8);
        scratch.resize(n * ch, 0.0);
        engine.render(scratch);
        if let Some(c) = capture {
            c.extend_from_slice(scratch);
        }
        left -= n;
    }
}
