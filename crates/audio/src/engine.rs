//! The mixer. Runs wherever the output adapter calls `render` (device callback
//! thread, or inline for null/offline output). It performs no file I/O, no
//! lookups by name, and no allocation in steady state except when a streamed
//! clip (music) loops or refills its small decode buffer.

use bri_console::Clamp;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::bank::{ClipData, SoundAsset};
use crate::command::{AudioEvent, Command, CullReason, EntityKey, Placement, SoundHandle};
use crate::decode::StreamDecoder;
use crate::schema::Bus;
use crate::spatial::{GainCurve, Listener, Vec3, finite3, pan_gains, torque_attenuation};

const SQRT_HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Voice budget and prioritisation. Defaults reproduce the TGE-family limits.
#[derive(Debug, Clone, PartialEq)]
pub struct VoicePolicy {
    /// Audible voices mixed at once (TGE `MAX_AUDIOSOURCES` = 16).
    pub max_real_voices: usize,
    /// Inaudible looping voices kept for later revival.
    pub max_virtual_voices: usize,
    /// One-shots whose gain (volume x channel x attenuation, excluding
    /// master) is at or below this are not started (TGE `MIN_GAIN`).
    pub min_start_gain: f32,
    /// Virtual loopers revive only above this gain (TGE `MIN_UNCULL_GAIN`).
    pub uncull_gain: f32,
    /// Minimum time a looper stays virtual (TGE `MIN_UNCULL_PERIOD`).
    pub uncull_delay_ms: u32,
    /// Optional cap on simultaneous instances of the same sound (not vanilla).
    pub max_instances_per_sound: Option<usize>,
}

impl Default for VoicePolicy {
    fn default() -> Self {
        Self {
            max_real_voices: 16,
            max_virtual_voices: 128,
            min_start_gain: 0.05,
            uncull_gain: 0.1,
            uncull_delay_ms: 500,
            max_instances_per_sound: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub voices: VoicePolicy,
    pub gain_curve: GainCurve,
    /// Internal processing block (commands, culling and gain ramps update per block).
    pub block_frames: usize,
    /// Fade applied to explicit stops so they do not click.
    pub declick_ms: f32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            voices: VoicePolicy::default(),
            gain_curve: GainCurve::TorqueTable,
            block_frames: 256,
            declick_ms: 5.0,
        }
    }
}

/// Counters shared with the game thread (lock-free).
#[derive(Debug, Default)]
pub struct SharedStats {
    pub real_voices: AtomicU32,
    pub virtual_voices: AtomicU32,
    pub suspended_voices: AtomicU32,
    pub started: AtomicU64,
    pub finished: AtomicU64,
    pub stopped: AtomicU64,
    /// Voices removed after starting (evicted by louder sounds, stream errors).
    pub culled: AtomicU64,
    /// Requests never started (inaudible, pressure, limits, music disabled).
    pub rejected: AtomicU64,
    pub virtualized: AtomicU64,
    pub revived: AtomicU64,
    pub commands: AtomicU64,
    pub events_dropped: AtomicU64,
    pub frames: AtomicU64,
    pub clipped_samples: AtomicU64,
    pub non_finite_samples: AtomicU64,
    /// f32 bits of the largest absolute output sample in the last block.
    pub last_block_peak: AtomicU32,
    /// f32 bits of the largest absolute output sample ever rendered.
    pub peak: AtomicU32,
}

struct StreamState {
    decoder: StreamDecoder,
    /// Interleaved decoded frames; `base` frames have been discarded before it.
    buf: Vec<f32>,
    ended: bool,
}

struct Voice {
    handle: SoundHandle,
    asset: Arc<SoundAsset>,
    entity: Option<EntityKey>,
    /// `None` = listener-relative 2D.
    position: Option<Vec3>,
    gain_scale: f32,
    real: bool,
    /// Music disabled: not mixed, restarts from the top when re-enabled.
    suspended: bool,
    /// Source frame position (fractional) within the clip / stream buffer.
    cursor: f64,
    stream: Option<StreamState>,
    last: [f32; 2],
    target: [f32; 2],
    score: f32,
    culled_at: u64,
    /// Remaining/total frames of a stop fade.
    stop: Option<(u32, u32)>,
    done: Option<AudioEvent>,
}

impl Voice {
    fn looping(&self) -> bool {
        self.asset.playback.looping
    }
    fn is_music(&self) -> bool {
        self.asset.playback.bus == Bus::Music
    }
}

pub(crate) struct Engine {
    cfg: EngineConfig,
    commands: rtrb::Consumer<Command>,
    events: rtrb::Producer<AudioEvent>,
    stats: Arc<SharedStats>,
    voices: Vec<Voice>,
    listener: Listener,
    /// What the listener also hears through (`Command::SetWindows`).
    windows: Vec<crate::spatial::Window>,
    master: f32,
    channel_gain: [f32; 9],
    music_gain: f32,
    music_enabled: bool,
    frames_rendered: u64,
    step_scale: f64,
    declick_frames: u32,
}

impl Engine {
    pub(crate) fn new(
        cfg: EngineConfig,
        commands: rtrb::Consumer<Command>,
        events: rtrb::Producer<AudioEvent>,
        stats: Arc<SharedStats>,
        master: f32,
        channel_gain: [f32; 9],
        music_enabled: bool,
    ) -> Self {
        let sample_rate = cfg.sample_rate.max(1);
        let capacity = cfg.voices.max_real_voices + cfg.voices.max_virtual_voices;
        let declick_frames = ((cfg.declick_ms.max(0.0) / 1000.0) * sample_rate as f32) as u32;
        Self {
            step_scale: 1.0 / f64::from(sample_rate),
            declick_frames,
            voices: Vec::with_capacity(capacity),
            listener: Listener::default(),
            windows: Vec::with_capacity(crate::spatial::MAX_WINDOWS),
            master: sanitize_gain(master),
            channel_gain: channel_gain.map(sanitize_gain),
            music_gain: 1.0,
            music_enabled,
            frames_rendered: 0,
            cfg,
            commands,
            events,
            stats,
        }
    }

    pub(crate) fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    /// A replacement output device may run at another rate: re-derive the
    /// per-rate constants and keep the engine clock continuous. Voices keep
    /// playing; they resample from their source rate every block.
    #[cfg_attr(not(feature = "cpal-output"), allow(dead_code))]
    pub(crate) fn set_sample_rate(&mut self, rate: u32) {
        let rate = rate.max(1);
        let old = self.cfg.sample_rate.max(1);
        if rate == old {
            return;
        }
        self.frames_rendered = self.frames_rendered * u64::from(rate) / u64::from(old);
        self.cfg.sample_rate = rate;
        self.step_scale = 1.0 / f64::from(rate);
        self.declick_frames = ((self.cfg.declick_ms.max(0.0) / 1000.0) * rate as f32) as u32;
    }

    /// Render interleaved output (`out.len()` must be a multiple of channels).
    pub(crate) fn render(&mut self, out: &mut [f32]) {
        let ch = usize::from(self.cfg.channels.max(1));
        let block = self.cfg.block_frames.max(16) * ch;
        for chunk in out.chunks_mut(block) {
            self.render_block(chunk);
        }
    }

    fn now_ms(&self) -> u64 {
        self.frames_rendered * 1000 / u64::from(self.cfg.sample_rate.max(1))
    }

    fn emit(&mut self, e: AudioEvent) {
        if self.events.push(e).is_err() {
            self.stats.events_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn render_block(&mut self, out: &mut [f32]) {
        let ch = usize::from(self.cfg.channels.max(1));
        let frames = out.len() / ch;
        out.fill(0.0);
        let mut applied = 0u64;
        while let Ok(cmd) = self.commands.pop() {
            self.apply(cmd);
            applied += 1;
            if applied >= 4096 {
                break; // keep the block bounded; the rest applies next block
            }
        }
        if applied > 0 {
            self.stats.commands.fetch_add(applied, Ordering::Relaxed);
        }

        for i in 0..self.voices.len() {
            self.update_targets(i);
        }
        self.revive_virtual();

        for i in 0..self.voices.len() {
            let v = &mut self.voices[i];
            if v.done.is_some() || v.suspended {
                continue;
            }
            if v.real {
                mix_voice(v, out, frames, ch, self.cfg.sample_rate, self.step_scale);
            } else {
                advance_virtual(v, frames, self.cfg.sample_rate);
            }
        }

        // Retire voices; order of events follows voice order (deterministic).
        let mut i = 0;
        while i < self.voices.len() {
            if let Some(ev) = self.voices[i].done {
                let v = self.voices.swap_remove(i);
                match ev {
                    AudioEvent::Finished { .. } => {
                        self.stats.finished.fetch_add(1, Ordering::Relaxed)
                    }
                    AudioEvent::Stopped { .. } => {
                        self.stats.stopped.fetch_add(1, Ordering::Relaxed)
                    }
                    _ => self.stats.culled.fetch_add(1, Ordering::Relaxed),
                };
                self.emit(ev);
                drop(v);
            } else {
                i += 1;
            }
        }

        let mut peak = 0f32;
        let mut clipped = 0u64;
        let mut non_finite = 0u64;
        for s in out.iter_mut() {
            if !s.is_finite() {
                *s = 0.0;
                non_finite += 1;
            }
            let a = s.abs();
            peak = peak.max(a);
            if a > 1.0 {
                clipped += 1;
                *s = s.clamped(-1.0, 1.0);
            }
        }
        if clipped > 0 {
            self.stats
                .clipped_samples
                .fetch_add(clipped, Ordering::Relaxed);
        }
        if non_finite > 0 {
            self.stats
                .non_finite_samples
                .fetch_add(non_finite, Ordering::Relaxed);
        }
        self.stats
            .last_block_peak
            .store(peak.to_bits(), Ordering::Relaxed);
        if peak > f32::from_bits(self.stats.peak.load(Ordering::Relaxed)) {
            self.stats.peak.store(peak.to_bits(), Ordering::Relaxed);
        }
        self.frames_rendered += frames as u64;
        self.stats
            .frames
            .fetch_add(frames as u64, Ordering::Relaxed);
        self.publish_counts();
    }

    fn publish_counts(&self) {
        let (mut real, mut virt, mut susp) = (0u32, 0u32, 0u32);
        for v in &self.voices {
            if v.suspended {
                susp += 1;
            } else if v.real {
                real += 1;
            } else {
                virt += 1;
            }
        }
        self.stats.real_voices.store(real, Ordering::Relaxed);
        self.stats.virtual_voices.store(virt, Ordering::Relaxed);
        self.stats.suspended_voices.store(susp, Ordering::Relaxed);
    }

    /// (score, [left, right]) for a voice at its current placement.
    fn gains(
        &self,
        asset: &SoundAsset,
        position: Option<Vec3>,
        gain_scale: f32,
    ) -> (f32, [f32; 2]) {
        let pb = &asset.playback;
        let channel = self
            .channel_gain
            .get(usize::from(pb.channel))
            .copied()
            .unwrap_or(1.0);
        let mut base = pb.gain * gain_scale * channel;
        if pb.bus == Bus::Music {
            base *= self.music_gain;
        }
        let mono = asset.data.channels() == 1;
        let (att, pan) = match (pb.spatial, position) {
            // OpenAL does not spatialise multi-channel buffers.
            (Some(sp), Some(pos)) if mono => {
                let (ear, d) = crate::spatial::heard(&self.listener, &self.windows, pos);
                (
                    torque_attenuation(d, sp.reference_distance, sp.max_distance),
                    pan_gains(ear, pos),
                )
            }
            _ => (1.0, (SQRT_HALF, SQRT_HALF)),
        };
        let score = base * att;
        let amp = self.cfg.gain_curve.apply(score * self.master);
        let g = if mono {
            [amp * pan.0, amp * pan.1]
        } else {
            [amp, amp]
        };
        (score, g)
    }

    fn update_targets(&mut self, i: usize) {
        let (score, g) = {
            let v = &self.voices[i];
            self.gains(&v.asset, v.position, v.gain_scale)
        };
        let v = &mut self.voices[i];
        v.score = score;
        v.target = g;
    }

    fn real_count(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.real && !v.suspended && v.done.is_none())
            .count()
    }
    fn virtual_count(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| !v.real && v.done.is_none())
            .count()
    }

    /// Index of the quietest real voice with score below `score`.
    fn quietest_real_below(&self, score: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, v) in self.voices.iter().enumerate() {
            if v.real
                && !v.suspended
                && v.done.is_none()
                && v.stop.is_none()
                && v.score < score
                && best.is_none_or(|(_, s)| v.score < s)
            {
                best = Some((i, v.score));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Take the real voice at `i` away: loopers become virtual, one-shots end.
    fn cull_real(&mut self, i: usize) {
        let now = self.now_ms();
        let looping = self.voices[i].looping();
        let room = self.virtual_count_excluding(i) < self.cfg.voices.max_virtual_voices;
        if looping && room {
            let v = &mut self.voices[i];
            v.real = false;
            v.culled_at = now;
            v.last = [0.0; 2];
            let h = v.handle;
            self.stats.virtualized.fetch_add(1, Ordering::Relaxed);
            self.emit(AudioEvent::Virtualized { handle: h });
        } else {
            let v = &mut self.voices[i];
            let reason = if looping {
                CullReason::VirtualLimit
            } else {
                CullReason::VoicePressure
            };
            v.done = Some(AudioEvent::Culled {
                handle: v.handle,
                reason,
            });
            v.real = false;
        }
    }

    fn virtual_count_excluding(&self, i: usize) -> usize {
        self.voices
            .iter()
            .enumerate()
            .filter(|(j, v)| *j != i && !v.real && v.done.is_none())
            .count()
    }

    fn revive_virtual(&mut self) {
        let now = self.now_ms();
        let policy = self.cfg.voices.clone();
        for i in 0..self.voices.len() {
            let v = &self.voices[i];
            if v.real || v.suspended || v.done.is_some() {
                continue;
            }
            if now.saturating_sub(v.culled_at) < u64::from(policy.uncull_delay_ms)
                || v.score <= policy.uncull_gain
            {
                continue;
            }
            let score = v.score;
            if self.real_count() >= policy.max_real_voices {
                match self.quietest_real_below(score) {
                    Some(j) => self.cull_real(j),
                    None => continue,
                }
            }
            let v = &mut self.voices[i];
            v.real = true;
            v.last = [0.0; 2]; // fade in over one block
            let h = v.handle;
            self.stats.revived.fetch_add(1, Ordering::Relaxed);
            self.emit(AudioEvent::Revived { handle: h });
        }
    }

    fn apply(&mut self, cmd: Command) {
        match cmd {
            Command::Play {
                handle,
                asset,
                placement,
                gain_scale,
            } => self.start(handle, asset, placement, gain_scale),
            Command::Stop {
                handle,
                fade_frames,
            } => {
                if let Some(i) = self
                    .voices
                    .iter()
                    .position(|v| v.handle == handle && v.done.is_none())
                {
                    self.begin_stop(i, fade_frames);
                }
            }
            Command::StopEntity {
                entity,
                fade_frames,
            } => {
                for i in 0..self.voices.len() {
                    if self.voices[i].entity == Some(entity) && self.voices[i].done.is_none() {
                        self.begin_stop(i, fade_frames);
                    }
                }
            }
            Command::StopAll { fade_frames } => {
                for i in 0..self.voices.len() {
                    if self.voices[i].done.is_none() {
                        self.begin_stop(i, fade_frames);
                    }
                }
            }
            Command::UpdateEntity { entity, position } => {
                if finite3(position) {
                    for v in self.voices.iter_mut().filter(|v| v.entity == Some(entity)) {
                        v.position = Some(position);
                    }
                }
            }
            Command::SetSourcePosition { handle, position } => {
                if finite3(position)
                    && let Some(v) = self
                        .voices
                        .iter_mut()
                        .find(|v| v.handle == handle && v.position.is_some())
                {
                    v.position = Some(position);
                }
            }
            Command::SetListener(l) => {
                if l.is_valid() {
                    self.listener = l;
                }
            }
            Command::SetWindows(windows) => {
                self.windows.clear();
                self.windows.extend(
                    windows
                        .iter()
                        .filter(|w| w.is_valid())
                        .take(crate::spatial::MAX_WINDOWS),
                );
            }
            Command::SetMaster(g) => self.master = sanitize_gain(g),
            Command::SetChannel { channel, gain } => {
                if let Some(slot) = self.channel_gain.get_mut(usize::from(channel)) {
                    *slot = sanitize_gain(gain);
                }
            }
            Command::SetMusicGain(g) => self.music_gain = sanitize_gain(g),
            Command::SetMusicEnabled(on) => self.set_music_enabled(on),
        }
    }

    fn begin_stop(&mut self, i: usize, fade_frames: u32) {
        let fade = fade_frames.max(self.declick_frames);
        let v = &mut self.voices[i];
        if !v.real || v.suspended || fade == 0 {
            v.done = Some(AudioEvent::Stopped { handle: v.handle });
        } else if v.stop.is_none() {
            v.stop = Some((fade, fade));
        }
    }

    fn set_music_enabled(&mut self, on: bool) {
        if on == self.music_enabled {
            return;
        }
        self.music_enabled = on;
        let max_real = self.cfg.voices.max_real_voices;
        for i in 0..self.voices.len() {
            if !self.voices[i].is_music() || self.voices[i].done.is_some() {
                continue;
            }
            if on {
                // Vanilla re-applies emitter profiles: music restarts from the top.
                let real = self.real_count() < max_real;
                let v = &mut self.voices[i];
                v.suspended = false;
                v.real = real;
                v.cursor = 0.0;
                v.last = [0.0; 2];
                if let Some(s) = v.stream.as_mut() {
                    if s.decoder.restart().is_err() {
                        v.done = Some(AudioEvent::Culled {
                            handle: v.handle,
                            reason: CullReason::StreamError,
                        });
                    }
                    s.buf.clear();
                    s.ended = false;
                }
            } else {
                let v = &mut self.voices[i];
                if v.looping() {
                    v.suspended = true;
                    v.real = false;
                } else {
                    v.done = Some(AudioEvent::Stopped { handle: v.handle });
                }
            }
        }
    }

    fn start(
        &mut self,
        handle: SoundHandle,
        asset: Arc<SoundAsset>,
        placement: Placement,
        gain_scale: f32,
    ) {
        let gain_scale = sanitize_gain(gain_scale);
        let (entity, position) = match placement {
            Placement::Listener => (None, None),
            Placement::World(p) => (None, finite3(p).then_some(p)),
            Placement::Attached { entity, position } => (
                Some(entity),
                Some(if finite3(position) {
                    position
                } else {
                    self.listener.position
                }),
            ),
        };
        // A 3D description played without a position is 2D (vanilla alxPlay).
        let position = if asset.playback.spatial.is_some() {
            position
        } else {
            None
        };
        let looping = asset.playback.looping;
        let is_music = asset.playback.bus == Bus::Music;
        let policy = self.cfg.voices.clone();

        if let Some(max) = policy.max_instances_per_sound {
            let n = self
                .voices
                .iter()
                .filter(|v| v.done.is_none() && Arc::ptr_eq(&v.asset, &asset))
                .count();
            if n >= max {
                return self.reject(handle, CullReason::InstanceLimit);
            }
        }

        let (score, target) = self.gains(&asset, position, gain_scale);
        let suspended = is_music && !self.music_enabled;
        if suspended && !looping {
            return self.reject(handle, CullReason::MusicDisabled);
        }
        if !looping && score <= policy.min_start_gain {
            return self.reject(handle, CullReason::BelowMinimumGain);
        }

        let mut real = false;
        if !suspended {
            if self.real_count() < policy.max_real_voices {
                real = true;
            } else if let Some(j) = self.quietest_real_below(score) {
                self.cull_real(j);
                real = true;
            }
        }
        if !real && !suspended {
            if !looping {
                return self.reject(handle, CullReason::VoicePressure);
            }
            if self.virtual_count() >= policy.max_virtual_voices {
                return self.reject(handle, CullReason::VirtualLimit);
            }
        }
        if self.voices.len() >= self.voices.capacity() {
            // Retiring voices still occupy slots until the block ends.
            return self.reject(handle, CullReason::VirtualLimit);
        }

        let stream = match &asset.data {
            ClipData::Stream(s) => match StreamDecoder::open(
                s.bytes.clone(),
                Some(s.extension),
                s.sample_rate,
                s.channels,
            ) {
                Ok(decoder) => Some(StreamState {
                    decoder,
                    buf: Vec::with_capacity(8192),
                    ended: false,
                }),
                Err(_) => return self.reject(handle, CullReason::StreamError),
            },
            ClipData::Pcm(_) => None,
        };
        let now = self.now_ms();
        self.voices.push(Voice {
            handle,
            asset,
            entity,
            position,
            gain_scale,
            real,
            suspended,
            cursor: 0.0,
            stream,
            last: target,
            target,
            score,
            culled_at: now,
            stop: None,
            done: None,
        });
        self.stats.started.fetch_add(1, Ordering::Relaxed);
        if !real && !suspended {
            self.stats.virtualized.fetch_add(1, Ordering::Relaxed);
        }
        self.emit(AudioEvent::Started { handle, real });
    }

    fn reject(&mut self, handle: SoundHandle, reason: CullReason) {
        self.stats.rejected.fetch_add(1, Ordering::Relaxed);
        self.emit(AudioEvent::Culled { handle, reason });
    }
}

fn sanitize_gain(g: f32) -> f32 {
    if g.is_finite() {
        g.clamped(0.0, 4.0)
    } else {
        0.0
    }
}

/// Fetch one interpolated source frame (up to 2 channels) at `pos`.
#[inline]
fn pcm_frame(
    samples: &[f32],
    channels: usize,
    frames: usize,
    pos: f64,
    looping: bool,
) -> Option<[f32; 2]> {
    let i0 = pos as usize;
    if i0 >= frames {
        return None;
    }
    let t = (pos - i0 as f64) as f32;
    let i1 = if i0 + 1 < frames {
        Some(i0 + 1)
    } else if looping {
        Some(0)
    } else {
        None
    };
    let get = |i: usize, c: usize| samples[i * channels + c.min(channels - 1)];
    let mut f = [0.0f32; 2];
    for (c, slot) in f.iter_mut().enumerate().take(channels.min(2)) {
        let a = get(i0, c);
        let b = i1.map_or(0.0, |i| get(i, c));
        *slot = a + (b - a) * t;
    }
    if channels == 1 {
        f[1] = f[0];
    }
    Some(f)
}

fn mix_voice(
    v: &mut Voice,
    out: &mut [f32],
    frames: usize,
    out_ch: usize,
    out_rate: u32,
    _step_scale: f64,
) {
    let src_rate = v.asset.data.sample_rate();
    let src_ch = usize::from(v.asset.data.channels().max(1));
    let step = f64::from(src_rate) / f64::from(out_rate.max(1))
        * f64::from(v.asset.playback.pitch.max(0.01));
    let looping = v.looping();
    let mono_src = src_ch == 1;
    let from = v.last;
    let to = v.target;
    let inv = 1.0 / frames.max(1) as f32;

    for n in 0..frames {
        // Stop fade envelope.
        let env = match v.stop.as_mut() {
            Some((remaining, total)) => {
                if *remaining == 0 {
                    v.done = Some(AudioEvent::Stopped { handle: v.handle });
                    break;
                }
                let e = *remaining as f32 / (*total).max(1) as f32;
                *remaining -= 1;
                e
            }
            None => 1.0,
        };
        let t = n as f32 * inv;
        let gl = (from[0] + (to[0] - from[0]) * t) * env;
        let gr = (from[1] + (to[1] - from[1]) * t) * env;

        let frame = match (&v.asset.data, v.stream.as_mut()) {
            (ClipData::Pcm(pcm), _) => {
                let total = pcm.frames();
                let f = pcm_frame(&pcm.samples, src_ch, total, v.cursor, looping);
                if f.is_some() {
                    v.cursor += step;
                    if looping && total > 0 && v.cursor >= total as f64 {
                        v.cursor %= total as f64;
                    }
                }
                f
            }
            (ClipData::Stream(_), Some(st)) => match stream_frame(st, src_ch, v.cursor, looping) {
                Ok(Some(f)) => {
                    v.cursor += step;
                    compact_stream(st, src_ch, &mut v.cursor);
                    Some(f)
                }
                Ok(None) => None,
                Err(()) => {
                    v.done = Some(AudioEvent::Culled {
                        handle: v.handle,
                        reason: CullReason::StreamError,
                    });
                    break;
                }
            },
            (ClipData::Stream(_), None) => None,
        };
        let Some(f) = frame else {
            v.done = Some(AudioEvent::Finished { handle: v.handle });
            break;
        };
        let base = n * out_ch;
        if out_ch == 1 {
            let s = if mono_src {
                f[0] * (gl + gr) * SQRT_HALF
            } else {
                (f[0] * gl + f[1] * gr) * 0.5
            };
            out[base] += s;
        } else {
            out[base] += f[0] * gl;
            out[base + 1] += f[1] * gr;
        }
    }
    v.last = to;
}

/// Ensure frames `i0` and `i0+1` are decoded, then interpolate.
fn stream_frame(
    st: &mut StreamState,
    ch: usize,
    pos: f64,
    looping: bool,
) -> Result<Option<[f32; 2]>, ()> {
    let need = pos as usize + 2;
    while st.buf.len() / ch < need {
        if st.ended {
            if looping {
                st.decoder.restart().map_err(|_| ())?;
                st.ended = false;
            } else {
                break;
            }
        }
        match st.decoder.next_chunk(&mut st.buf) {
            Ok(true) => {}
            Ok(false) => {
                st.ended = true;
                if !looping {
                    break;
                }
            }
            Err(_) => return Err(()),
        }
    }
    let frames = st.buf.len() / ch;
    Ok(pcm_frame(&st.buf, ch, frames, pos, false))
}

fn compact_stream(st: &mut StreamState, ch: usize, cursor: &mut f64) {
    let consumed = *cursor as usize;
    if consumed >= 4096 {
        st.buf.drain(..consumed * ch);
        *cursor -= consumed as f64;
    }
}

/// Virtual voices keep time so revived loops continue in phase.
fn advance_virtual(v: &mut Voice, frames: usize, out_rate: u32) {
    let step = f64::from(v.asset.data.sample_rate()) / f64::from(out_rate.max(1));
    match &v.asset.data {
        ClipData::Pcm(pcm) => {
            let total = pcm.frames() as f64;
            v.cursor += step * frames as f64;
            if total > 0.0 {
                if v.looping() {
                    v.cursor %= total;
                } else if v.cursor >= total {
                    v.done = Some(AudioEvent::Finished { handle: v.handle });
                }
            }
        }
        // Streams pause while virtual (no decode work); they resume in place.
        ClipData::Stream(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_replacement_output_rate_keeps_the_engine_clock_continuous() {
        let (_commands, consumer) = rtrb::RingBuffer::new(4);
        let (producer, _events) = rtrb::RingBuffer::new(4);
        let mut engine = Engine::new(
            EngineConfig::default(),
            consumer,
            producer,
            Arc::default(),
            1.0,
            [1.0; 9],
            true,
        );
        let mut out = vec![0.0; 48_000 * 2];
        engine.render(&mut out);
        assert_eq!(engine.now_ms(), 1000);
        engine.set_sample_rate(44_100);
        assert_eq!(engine.now_ms(), 1000);
        assert_eq!(engine.config().sample_rate, 44_100);
        assert_eq!(engine.step_scale, 1.0 / 44_100.0);
        let mut out = vec![0.0; 44_100 * 2];
        engine.render(&mut out);
        assert_eq!(engine.now_ms(), 2000);
    }
}
