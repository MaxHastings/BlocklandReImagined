//! Proximity voice chat: everything between the microphone, the wire and the
//! speakers that needs no hardware.
//!
//! * [`Codec`]: Opus wideband mono, 20 ms frames at 24 kbit/s with FEC.
//!   Packet playout reorders, recovers isolated loss and bounds concealment.
//! * [`Encoder`]: microphone samples in, gated and encoded frames out
//!   (push to talk or voice activation).
//! * [`Mixer`]: encoded frames from other players in, interleaved output
//!   samples out. Each speaker has a jitter buffer and is placed in the world
//!   with the same listener, panning and Torque distance rolloff the game's
//!   sounds use.
//!
//! The cpal streams that connect these to a microphone and speakers are in
//! `voice_device` (feature `cpal-output`).

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use bri_console::Clamp;

use crate::spatial::{Listener, Vec3, length, pan_gains, sub, torque_attenuation};

/// Voice is carried at this rate, whatever the devices run at.
pub const SAMPLE_RATE: u32 = 16_000;
/// 20 ms of voice.
pub const FRAME_SAMPLES: usize = 320;
/// Hard packet budget (Opus VBR normally uses about 60 bytes).
pub const FRAME_BYTES: usize = 256;
/// The distance at which a voice stops being heard, in world units, unless
/// settings say less. The host sends nobody a voice beyond its own range.
pub const DEFAULT_RANGE: f32 = 48.0;
/// Inside this distance a voice is at full volume.
const FULL_VOLUME_DISTANCE: f32 = 3.0;
/// Most speakers mixed at once; the rest are not heard.
pub const MAX_SPEAKERS: usize = 32;
/// Frames held before a speaker starts to play (the jitter buffer).
const PREBUFFER_FRAMES: usize = 2;
/// Most frames held for one speaker; older audio is dropped to stay current.
const MAX_BUFFERED_FRAMES: usize = 5;
/// Output samples of silence after which an idle speaker is forgotten.
const FORGET_AFTER_SECONDS: f32 = 4.0;
/// A speaker counts as speaking this long after their last audio.
const SPEAKING_HOLD_SECONDS: f32 = 0.4;

/// How the microphone opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Activation {
    /// Only while the push-to-talk control is held.
    #[default]
    PushToTalk,
    /// Whenever the microphone is louder than the threshold.
    Voice,
    /// Never: the microphone is not even opened. Other players are still
    /// heard.
    Disabled,
}

/// What the player chose. Applied live; nothing here needs a restart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// Off: nothing is sent and nothing is played.
    pub enabled: bool,
    pub activation: Activation,
    /// Voice activation opens above this microphone level, 0..=1.
    pub threshold: f32,
    /// Microphone gain, 0..=4.
    pub mic_gain: f32,
    /// Volume of other players' voices, 0..=2.
    pub volume: f32,
    /// Distance at which voices fade out, in world units.
    pub range: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            activation: Activation::PushToTalk,
            threshold: 0.02,
            mic_gain: 1.0,
            volume: 1.0,
            range: DEFAULT_RANGE,
        }
    }
}

impl Settings {
    /// The same settings with every value in its range (a pref can hold
    /// anything).
    pub fn sanitized(self) -> Self {
        let unit = |v: f32, lo: f32, hi: f32, fallback: f32| {
            if v.is_finite() {
                v.clamped(lo, hi)
            } else {
                fallback
            }
        };
        Self {
            enabled: self.enabled,
            activation: self.activation,
            threshold: unit(self.threshold, 0.0, 1.0, 0.02),
            mic_gain: unit(self.mic_gain, 0.0, 4.0, 1.0),
            volume: unit(self.volume, 0.0, 2.0, 1.0),
            range: unit(self.range, FULL_VOLUME_DISTANCE + 1.0, 200.0, DEFAULT_RANGE),
        }
    }
}

/// Settings and state shared between the game thread and the audio threads.
#[derive(Default)]
pub struct Shared {
    enabled: AtomicBool,
    voice_activation: AtomicBool,
    disabled: AtomicBool,
    talking: AtomicBool,
    threshold: AtomicU32,
    mic_gain: AtomicU32,
    volume: AtomicU32,
    range: AtomicU32,
    transmitting: AtomicBool,
    transmit_allowed: AtomicBool,
    activity: AtomicBool,
    level: AtomicU32,
    pub(crate) broken: AtomicBool,
}

fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}
fn store(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

impl Shared {
    pub fn new(settings: Settings) -> Self {
        let shared = Self::default();
        shared.apply(settings);
        shared.transmit_allowed.store(true, Ordering::Relaxed);
        shared
    }
    pub fn apply(&self, settings: Settings) {
        let s = settings.sanitized();
        self.enabled.store(s.enabled, Ordering::Relaxed);
        self.voice_activation
            .store(s.activation == Activation::Voice, Ordering::Relaxed);
        self.disabled
            .store(s.activation == Activation::Disabled, Ordering::Relaxed);
        store(&self.threshold, s.threshold);
        store(&self.mic_gain, s.mic_gain);
        store(&self.volume, s.volume);
        store(&self.range, s.range);
    }
    /// Session/focus/moderation gate, independent of the local input meter.
    pub fn set_transmit_allowed(&self, allowed: bool) {
        self.transmit_allowed.store(allowed, Ordering::Relaxed);
        if !allowed {
            self.transmitting.store(false, Ordering::Relaxed);
        }
    }
    /// The push-to-talk control went down or up.
    pub fn set_talking(&self, talking: bool) {
        self.talking.store(talking, Ordering::Relaxed);
    }
    /// Whether frames are being sent right now.
    pub fn transmitting(&self) -> bool {
        self.transmitting.load(Ordering::Relaxed)
    }
    /// Whether the microphone heard a voice a moment ago: its level was above
    /// the threshold within the last 200 ms. Independent of whether anything
    /// is being sent, so a meter can show it while Push To Talk is held.
    pub fn activity(&self) -> bool {
        self.activity.load(Ordering::Relaxed)
    }
    /// The microphone's recent level, 0..=1 (for a meter).
    pub fn level(&self) -> f32 {
        load(&self.level)
    }
    /// A device stream failed; the voice chat should be opened again.
    pub fn is_broken(&self) -> bool {
        self.broken.load(Ordering::Relaxed)
    }
    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

mod jitter;

/// Fixed-size packet storage keeps capture and playout free of per-frame
/// packet allocation. Byte zero marks the start of a new talk spurt.
#[derive(Clone, Copy)]
pub struct EncodedFrame {
    bytes: [u8; FRAME_BYTES],
    len: usize,
}
impl EncodedFrame {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > FRAME_BYTES {
            return None;
        }
        let mut frame = Self {
            bytes: [0; FRAME_BYTES],
            len: bytes.len(),
        };
        frame.bytes[..bytes.len()].copy_from_slice(bytes);
        Some(frame)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Opus VoIP, wideband mono at 24 kbit/s with in-band FEC. The network
/// envelopes are versioned by bri-net; every packet is exactly 20 ms.
pub struct Codec;
impl Codec {
    fn encoder() -> Option<opus::Encoder> {
        let mut encoder =
            opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip).ok()?;
        encoder.set_bitrate(opus::Bitrate::Bits(24_000)).ok()?;
        encoder.set_complexity(5).ok()?;
        encoder.set_inband_fec(true).ok()?;
        encoder.set_packet_loss_perc(10).ok()?;
        Some(encoder)
    }
    pub fn valid(bytes: &[u8]) -> bool {
        bytes.len() >= 2
            && bytes.len() <= FRAME_BYTES
            && bytes[0] <= 1
            && opus::packet::get_nb_samples(&bytes[1..], SAMPLE_RATE).ok() == Some(FRAME_SAMPLES)
            && opus::packet::get_nb_channels(&bytes[1..]).ok() == Some(opus::Channels::Mono)
    }
    /// Fixture helper. Live capture keeps one encoder across frames.
    pub fn encode(pcm: &[i16; FRAME_SAMPLES]) -> Vec<u8> {
        Self::encoder()
            .and_then(|mut encoder| encoder.encode_vec(pcm, FRAME_BYTES - 1).ok())
            .map(|packet| {
                let mut framed = vec![1];
                framed.extend(packet);
                framed
            })
            .unwrap_or_default()
    }
    pub fn decode(bytes: &[u8]) -> Option<[f32; FRAME_SAMPLES]> {
        if !Self::valid(bytes) {
            return None;
        }
        let mut out = [0.0; FRAME_SAMPLES];
        opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono)
            .ok()?
            .decode_float(&bytes[1..], &mut out, false)
            .ok()?;
        Some(out)
    }
}

/// Area-averaging resampler from a device rate down to [`SAMPLE_RATE`].
/// It accounts for fractional output-window boundaries, so common 44.1 kHz
/// devices don't alternate between unweighted two- and three-sample blocks.
pub struct Downsampler {
    ratio: f64,
    position: f64,
    sum: f64,
    count: f64,
}

impl Downsampler {
    pub fn new(input_rate: u32) -> Self {
        Self {
            ratio: f64::from(input_rate.max(SAMPLE_RATE)) / f64::from(SAMPLE_RATE),
            position: 0.0,
            sum: 0.0,
            count: 0.0,
        }
    }
    /// Feed one input sample; returns an output sample when one completes.
    /// Input samples represent unit-width intervals; an interval split by an
    /// output boundary contributes proportionally to both neighboring windows.
    pub fn push(&mut self, sample: f32) -> Option<f32> {
        let mut remaining = 1.0_f64;
        let mut output = None;
        let sample = f64::from(sample);
        while remaining > 1.0e-12 {
            let boundary = (self.ratio - self.position).max(0.0);
            let weight = remaining.min(boundary);
            self.sum += sample * weight;
            self.count += weight;
            self.position += weight;
            remaining -= weight;
            if self.position + 1.0e-9 >= self.ratio {
                output = Some((self.sum / self.count.max(1.0e-12)) as f32);
                self.position = 0.0;
                self.sum = 0.0;
                self.count = 0.0;
            }
        }
        output
    }
}

/// Microphone samples in, voice frames out.
pub struct Encoder {
    shared: std::sync::Arc<Shared>,
    pending: Vec<f32>,
    codec: Option<opus::Encoder>,
    sending: bool,
    hang: u32,
    /// Frames left in which the microphone still counts as active.
    active_hang: u32,
    sequence: u16,
}

/// Frames the activity flag stays up after the voice drops (200 ms).
const ACTIVITY_HANG: u32 = 10;
/// Frames a push-to-talk keeps sending after release, so the last syllable
/// is not clipped.
const PTT_HANG: u32 = 3;
/// Frames voice activation stays open after the voice drops below the
/// threshold (300 ms).
const VOICE_HANG: u32 = 15;

impl Encoder {
    pub fn new(shared: std::sync::Arc<Shared>) -> Self {
        Self {
            shared,
            pending: Vec::with_capacity(FRAME_SAMPLES * 2),
            codec: Codec::encoder(),
            sending: false,
            hang: 0,
            active_hang: 0,
            sequence: 0,
        }
    }
    /// Take microphone samples at [`SAMPLE_RATE`]; `send` gets each finished
    /// frame that should go out.
    pub fn feed(&mut self, samples: &[f32], send: &mut impl FnMut(u16, EncodedFrame)) {
        for &sample in samples {
            self.pending
                .push(if sample.is_finite() { sample } else { 0.0 });
            if self.pending.len() == FRAME_SAMPLES {
                self.frame(send);
                self.pending.clear();
            }
        }
    }
    fn frame(&mut self, send: &mut impl FnMut(u16, EncodedFrame)) {
        let gain = load(&self.shared.mic_gain);
        let mut pcm = [0i16; FRAME_SAMPLES];
        let mut energy = 0.0f32;
        for (out, &s) in pcm.iter_mut().zip(&self.pending) {
            let s = (s * gain).clamped(-1.0, 1.0);
            energy += s * s;
            *out = (s * 32767.0) as i16;
        }
        let rms = (energy / FRAME_SAMPLES as f32).sqrt();
        // A meter that rises at once and falls over about a quarter second.
        let level = load(&self.shared.level);
        store(&self.shared.level, rms.max(level * 0.85));
        let voice = self.shared.voice_activation.load(Ordering::Relaxed);
        let loud = rms > load(&self.shared.threshold);
        if loud {
            self.active_hang = ACTIVITY_HANG;
        } else {
            self.active_hang = self.active_hang.saturating_sub(1);
        }
        self.shared
            .activity
            .store(self.active_hang > 0, Ordering::Relaxed);
        let open = if voice {
            loud
        } else {
            self.shared.talking.load(Ordering::Relaxed)
        };
        let enabled = self.shared.enabled()
            && !self.shared.disabled.load(Ordering::Relaxed)
            && self.shared.transmit_allowed.load(Ordering::Relaxed);
        if !enabled {
            self.hang = 0;
        }
        if open && enabled {
            self.hang = if voice { VOICE_HANG } else { PTT_HANG };
        }
        let sending = enabled && (open || self.hang > 0);
        if !open && self.hang > 0 {
            self.hang -= 1;
        }
        if sending {
            if let Some(codec) = &mut self.codec {
                let mut frame = EncodedFrame {
                    bytes: [0; FRAME_BYTES],
                    len: 0,
                };
                frame.bytes[0] = u8::from(!self.sending);
                if !self.sending && codec.reset_state().is_err() {
                    self.shared.broken.store(true, Ordering::Relaxed);
                    return;
                }
                match codec.encode(&pcm, &mut frame.bytes[1..]) {
                    Ok(len) => {
                        frame.len = len + 1;
                        send(self.sequence, frame);
                    }
                    Err(_) => self.shared.broken.store(true, Ordering::Relaxed),
                }
            } else {
                self.shared.broken.store(true, Ordering::Relaxed);
            }
        }
        self.sequence = self.sequence.wrapping_add(1);
        self.sending = sending;
        self.shared.transmitting.store(sending, Ordering::Relaxed);
    }
}

/// Whether sequence number `new` is later than `old` (wrapping).
fn newer(old: u16, new: u16) -> bool {
    let ahead = new.wrapping_sub(old);
    ahead != 0 && ahead < 0x8000
}

struct Speaker {
    position: Vec3,
    buffer: VecDeque<f32>,
    playing: bool,
    jitter: jitter::Jitter,
    waiting: f32,
    /// The sample the output is interpolating from, and how far toward the
    /// next one it is.
    current: f32,
    phase: f32,
    /// What is left of the last sample after the voice stops, fading out.
    tail: f32,
    /// Gains used on the last block, so they change smoothly.
    gains: (f32, f32),
    idle_samples: u64,
    /// The loudest this speaker's voice was in the recent output, 0..=1:
    /// fast to rise with a loud frame, falling over about half a second so
    /// a name tag's blue shade tracks how loudly they are heard.
    level: f32,
}

impl Speaker {
    fn new(position: Vec3) -> Self {
        Self {
            position,
            buffer: VecDeque::with_capacity(FRAME_SAMPLES),
            playing: false,
            jitter: jitter::Jitter::new(),
            waiting: 0.0,
            current: 0.0,
            phase: 0.0,
            tail: 0.0,
            gains: (0.0, 0.0),
            idle_samples: 0,
            level: 0.0,
        }
    }
    fn next(&mut self, step: f32) -> f32 {
        if !self.playing && self.jitter.len() > 0 {
            self.waiting += step;
            if self.jitter.len() >= PREBUFFER_FRAMES || self.waiting >= FRAME_SAMPLES as f32 * 2.0 {
                self.playing = true;
                self.waiting = 0.0;
            }
        }
        if self.playing {
            if self.buffer.is_empty()
                && let Some(frame) = self.jitter.pop()
            {
                self.buffer.extend(frame);
            }
            let Some(&next) = self.buffer.front() else {
                self.playing = false;
                self.tail = self.current;
                return self.fade();
            };
            let sample = self.current + (next - self.current) * self.phase;
            self.phase += step;
            while self.phase >= 1.0 {
                self.phase -= 1.0;
                self.current = self.buffer.pop_front().unwrap_or(self.current);
            }
            self.idle_samples = 0;
            sample
        } else {
            self.fade()
        }
    }
    fn fade(&mut self) -> f32 {
        self.idle_samples += 1;
        if self.tail.abs() < 1e-4 {
            self.tail = 0.0;
            self.current = 0.0;
            self.phase = 0.0;
            return 0.0;
        }
        let out = self.tail;
        self.tail *= 0.995;
        out
    }
}

/// The other players' voices, ready to be played.
pub struct Mixer {
    listener: Listener,
    speakers: BTreeMap<u64, Speaker>,
    shared: std::sync::Arc<Shared>,
    /// Speakers dropped because [`MAX_SPEAKERS`] were already mixed.
    pub refused: u64,
    /// Frames that were not a valid voice frame.
    pub malformed: u64,
    limiter_gain: f32,
}

impl Mixer {
    pub fn new(shared: std::sync::Arc<Shared>) -> Self {
        Self {
            listener: Listener::default(),
            speakers: BTreeMap::new(),
            shared,
            refused: 0,
            malformed: 0,
            limiter_gain: 1.0,
        }
    }
    pub fn set_listener(&mut self, listener: Listener) {
        if listener.is_valid() {
            self.listener = listener;
        }
    }
    /// A frame from `speaker`, standing at `position`.
    pub fn hear(&mut self, speaker: u64, position: Vec3, sequence: u16, payload: &[u8]) {
        if !self.shared.enabled() || !position.iter().all(|v| v.is_finite()) {
            return;
        }
        if !Codec::valid(payload) {
            self.malformed += 1;
            return;
        };
        if !self.speakers.contains_key(&speaker) && self.speakers.len() >= MAX_SPEAKERS {
            self.refused += 1;
            return;
        }
        let entry = self
            .speakers
            .entry(speaker)
            .or_insert_with(|| Speaker::new(position));
        entry.position = position;
        entry.jitter.push(sequence, payload);
    }
    /// The gain a voice `distance` away is heard at.
    pub fn gain_at(distance: f32, range: f32) -> f32 {
        let linear = torque_attenuation(distance, FULL_VOLUME_DISTANCE, range);
        linear * linear
    }
    /// Purge buffered speech immediately when an actor is muted or leaves.
    pub fn forget(&mut self, speaker: u64) {
        self.speakers.remove(&speaker);
    }
    /// Players heard in the last moment.
    pub fn speaking(&self, sample_rate: u32) -> Vec<u64> {
        let hold = (SPEAKING_HOLD_SECONDS * sample_rate as f32) as u64;
        self.speakers
            .iter()
            .filter(|(_, s)| s.playing || s.idle_samples < hold)
            .map(|(&id, _)| id)
            .collect()
    }
    /// How loudly each speaker is being heard right now, 0..=1: the loudest
    /// of their recent output samples. A speaker who stops talking keeps a
    /// falling level for about half a second and then reads as silent.
    pub fn levels(&self) -> BTreeMap<u64, f32> {
        self.speakers.iter().map(|(&id, s)| (id, s.level)).collect()
    }
    /// Fill `out` (interleaved, `channels` wide) with the mix; one or two
    /// channels get the voices panned, more carry them on the first two.
    pub fn render(&mut self, out: &mut [f32], channels: usize, sample_rate: u32) {
        out.fill(0.0);
        let channels = channels.max(1);
        let frames = out.len() / channels;
        if frames == 0 || sample_rate == 0 {
            return;
        }
        let volume = load(&self.shared.volume);
        let range = load(&self.shared.range);
        let step = SAMPLE_RATE as f32 / sample_rate as f32;
        let forget = (FORGET_AFTER_SECONDS * sample_rate as f32) as u64;
        let listener = self.listener;
        // The level falls over about half a second of output, so the blue
        // shade eases off after a sentence ends.
        let decay = (frames as f32 / (0.5 * sample_rate as f32)).clamped(0.0, 1.0);
        for speaker in self.speakers.values_mut() {
            let distance = length(sub(speaker.position, listener.position));
            let gain = Self::gain_at(distance, range) * volume;
            let (left, right) = pan_gains(&listener, speaker.position);
            let target = if channels == 1 {
                (gain, gain)
            } else {
                // Equal-power pan gains are 0.707 each when centred.
                (
                    gain * left * std::f32::consts::SQRT_2,
                    gain * right * std::f32::consts::SQRT_2,
                )
            };
            let start = speaker.gains;
            let mut loudest = 0.0f32;
            for frame in 0..frames {
                let sample = speaker.next(step);
                let t = (frame + 1) as f32 / frames as f32;
                let l = start.0 + (target.0 - start.0) * t;
                let r = start.1 + (target.1 - start.1) * t;
                let base = frame * channels;
                out[base] += sample * l;
                if channels > 1 {
                    out[base + 1] += sample * r;
                }
                // Both ears: a speaker panned to one side is silent on the
                // other channel, so neither alone says how loud they are.
                loudest = loudest.max((sample * l).abs().max((sample * r).abs()));
            }
            speaker.gains = target;
            // Rise at once with a loud frame, then fall over the decay.
            speaker.level = speaker.level.max(loudest) * (1.0 - decay);
        }
        self.speakers
            .retain(|_, s| s.playing || s.idle_samples < forget);
        // A linked fast-attack limiter preserves relative speaker levels
        // without the hard-clipping distortion of crowded conversations.
        let peak = out.iter().fold(1.0_f32, |peak, v| peak.max(v.abs()));
        self.limiter_gain =
            (self.limiter_gain + frames as f32 / sample_rate as f32 * 2.0).min(1.0 / peak);
        for v in out.iter_mut() {
            *v *= self.limiter_gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn tone(frames: usize, amplitude: f32) -> Vec<f32> {
        (0..frames * FRAME_SAMPLES)
            .map(|i| amplitude * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 16_000.0).sin())
            .collect()
    }

    fn pcm(samples: &[f32]) -> [i16; FRAME_SAMPLES] {
        let mut out = [0i16; FRAME_SAMPLES];
        for (o, s) in out.iter_mut().zip(samples) {
            *o = (s * 32767.0) as i16;
        }
        out
    }

    #[test]
    fn a_frame_survives_the_codec() {
        let samples = tone(1, 0.5);
        let bytes = Codec::encode(&pcm(&samples));
        assert!(!bytes.is_empty() && bytes.len() <= FRAME_BYTES);
        let decoded = Codec::decode(&bytes).unwrap();
        assert!(decoded.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(decoded.iter().map(|s| s * s).sum::<f32>() > 0.1);
    }

    #[test]
    fn malformed_frames_are_refused() {
        assert!(Codec::decode(&[]).is_none());
        assert!(Codec::decode(&[0; FRAME_BYTES + 1]).is_none());
        assert!(Codec::decode(&[0xff]).is_none());
        // A valid Opus packet with the wrong duration cannot consume extra CPU.
        let mut encoder = Codec::encoder().unwrap();
        let short = encoder
            .encode_vec(&[0; FRAME_SAMPLES / 2], FRAME_BYTES)
            .unwrap();
        let mut framed = vec![1];
        framed.extend(short);
        assert!(!Codec::valid(&framed));
    }

    #[test]
    fn downsampling_hits_the_voice_rate() {
        let mut down = Downsampler::new(48_000);
        let out = (0..48_000).filter_map(|_| down.push(1.0)).count();
        assert_eq!(out, 16_000);
        let mut down = Downsampler::new(44_100);
        let mut samples = Vec::new();
        for _ in 0..44_100 {
            if let Some(sample) = down.push(1.0) {
                samples.push(sample);
            }
        }
        assert_eq!(samples.len(), 16_000, "stable 44.1 kHz conversion");
        assert!(samples.iter().all(|sample| (*sample - 1.0).abs() < 1e-6));
    }

    fn settings(activation: Activation) -> Arc<Shared> {
        Arc::new(Shared::new(Settings {
            activation,
            ..Settings::default()
        }))
    }

    fn feed(encoder: &mut Encoder, samples: &[f32]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        encoder.feed(samples, &mut |_, f| frames.push(f.as_bytes().to_vec()));
        frames
    }

    #[test]
    fn push_to_talk_sends_only_while_held_plus_a_short_tail() {
        let shared = settings(Activation::PushToTalk);
        let mut encoder = Encoder::new(shared.clone());
        let audio = tone(10, 0.5);
        assert!(feed(&mut encoder, &audio).is_empty(), "closed microphone");
        shared.set_talking(true);
        assert_eq!(feed(&mut encoder, &audio).len(), 10);
        assert!(shared.transmitting());
        shared.set_talking(false);
        assert_eq!(feed(&mut encoder, &audio).len(), PTT_HANG as usize);
        assert!(!shared.transmitting());
    }

    #[test]
    fn voice_activation_follows_the_level_and_holds_open() {
        let shared = settings(Activation::Voice);
        let mut encoder = Encoder::new(shared.clone());
        assert!(feed(&mut encoder, &vec![0.001; FRAME_SAMPLES * 5]).is_empty());
        assert_eq!(feed(&mut encoder, &tone(4, 0.5)).len(), 4);
        let quiet = feed(&mut encoder, &vec![0.0; FRAME_SAMPLES * 30]);
        assert_eq!(quiet.len(), VOICE_HANG as usize);
    }

    #[test]
    fn disabled_sends_nothing_whatever_the_key_or_voice_does() {
        let shared = settings(Activation::Disabled);
        shared.set_talking(true);
        let mut encoder = Encoder::new(shared.clone());
        assert!(feed(&mut encoder, &tone(10, 0.5)).is_empty());
        assert!(!shared.transmitting());
        // Back to push to talk without reopening anything.
        shared.apply(Settings::default());
        assert_eq!(feed(&mut encoder, &tone(3, 0.5)).len(), 3);
    }

    #[test]
    fn activity_follows_the_voice_with_a_short_hold_in_every_mode() {
        for mode in [
            Activation::PushToTalk,
            Activation::Voice,
            Activation::Disabled,
        ] {
            let shared = settings(mode);
            let mut encoder = Encoder::new(shared.clone());
            feed(&mut encoder, &vec![0.0; FRAME_SAMPLES * 3]);
            assert!(!shared.activity(), "{mode:?} silent");
            feed(&mut encoder, &tone(1, 0.5));
            assert!(shared.activity(), "{mode:?} speaking");
            feed(
                &mut encoder,
                &vec![0.0; FRAME_SAMPLES * (ACTIVITY_HANG as usize - 1)],
            );
            assert!(shared.activity(), "{mode:?} inside the hold");
            feed(&mut encoder, &vec![0.0; FRAME_SAMPLES]);
            assert!(!shared.activity(), "{mode:?} after the hold");
        }
    }

    #[test]
    fn a_disabled_microphone_sends_nothing() {
        let shared = Arc::new(Shared::new(Settings {
            enabled: false,
            ..Settings::default()
        }));
        shared.set_talking(true);
        let mut encoder = Encoder::new(shared);
        assert!(feed(&mut encoder, &tone(5, 0.5)).is_empty());
    }

    #[test]
    fn each_talk_spurt_marks_and_resets_its_codec_state() {
        let shared = settings(Activation::PushToTalk);
        let mut encoder = Encoder::new(shared.clone());
        shared.set_talking(true);
        let first = feed(&mut encoder, &tone(3, 0.5));
        assert_eq!(first[0][0], 1);
        assert_eq!(first[1][0], 0);
        shared.set_talking(false);
        feed(&mut encoder, &vec![0.0; FRAME_SAMPLES * 20]);
        shared.set_talking(true);
        let next = feed(&mut encoder, &tone(1, 0.5));
        assert_eq!(
            next[0], first[0],
            "same first frame after a fresh codec reset"
        );
    }

    #[test]
    fn preview_focus_and_server_gates_never_transmit_or_leak_a_tail() {
        let shared = settings(Activation::Voice);
        let mut encoder = Encoder::new(shared.clone());
        shared.set_transmit_allowed(false);
        assert!(feed(&mut encoder, &tone(10, 0.5)).is_empty());
        assert!(shared.activity(), "local meter still works");
        shared.set_transmit_allowed(true);
        assert_eq!(feed(&mut encoder, &tone(4, 0.5)).len(), 4);
        shared.set_transmit_allowed(false);
        assert!(feed(&mut encoder, &tone(5, 0.5)).is_empty());
        shared.set_transmit_allowed(true);
        assert!(
            feed(&mut encoder, &vec![0.0; FRAME_SAMPLES * 2]).is_empty(),
            "no stale hangover after a gate closes"
        );
    }

    #[test]
    fn all_thirty_one_remote_speakers_mix_and_mute_purges_buffered_audio() {
        let mut mixer = Mixer::new(Arc::new(Shared::new(Settings::default())));
        for owner in 1..32 {
            speak(&mut mixer, owner, [0.0, 0.0, -2.0], 4, 0);
        }
        let mut out = vec![0.0; 960 * 2];
        mixer.render(&mut out, 2, 48_000);
        assert_eq!(mixer.speaking(48_000).len(), 31);
        assert_eq!(mixer.refused, 0);
        assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        assert!(out.iter().any(|v| v.abs() > 0.1));
        for owner in 1..32 {
            mixer.forget(owner);
        }
        mixer.render(&mut out, 2, 48_000);
        assert!(
            out.iter().all(|v| *v == 0.0),
            "mute clears audio and its fading tail immediately"
        );
    }

    #[test]
    fn sequence_numbers_wrap() {
        assert!(newer(1, 2));
        assert!(!newer(2, 2));
        assert!(!newer(2, 1));
        assert!(newer(65_535, 0));
        assert!(!newer(0, 65_535));
    }

    fn speak(mixer: &mut Mixer, who: u64, at: Vec3, frames: usize, from: u16) {
        let audio = tone(frames, 0.5);
        for (n, chunk) in audio.chunks(FRAME_SAMPLES).enumerate() {
            let bytes = Codec::encode(&pcm(chunk));
            mixer.hear(who, at, from.wrapping_add(n as u16), &bytes);
        }
    }

    fn energy(out: &[f32], channels: usize, channel: usize) -> f32 {
        out.chunks(channels).map(|f| f[channel] * f[channel]).sum()
    }

    #[test]
    fn a_voice_on_the_right_is_heard_on_the_right() {
        let shared = Arc::new(Shared::new(Settings::default()));
        let mut mixer = Mixer::new(shared);
        // The default listener faces -Z with +X to its right.
        speak(&mut mixer, 7, [6.0, 0.0, 0.0], 8, 0);
        let mut out = vec![0.0; 4800 * 2];
        mixer.render(&mut out, 2, 48_000);
        let (left, right) = (energy(&out, 2, 0), energy(&out, 2, 1));
        assert!(
            right > 4.0 * left && right > 0.1,
            "left {left} right {right}"
        );
        assert_eq!(mixer.speaking(48_000), vec![7]);
    }

    #[test]
    fn voices_fade_with_distance_and_end_at_the_range() {
        let range = DEFAULT_RANGE;
        assert_eq!(Mixer::gain_at(1.0, range), 1.0);
        assert!(Mixer::gain_at(10.0, range) > Mixer::gain_at(30.0, range));
        assert_eq!(Mixer::gain_at(range, range), 0.0);
        assert_eq!(Mixer::gain_at(range + 10.0, range), 0.0);
        let shared = Arc::new(Shared::new(Settings::default()));
        let mut mixer = Mixer::new(shared);
        speak(&mut mixer, 1, [range + 5.0, 0.0, 0.0], 8, 0);
        let mut out = vec![0.0; 4800 * 2];
        mixer.render(&mut out, 2, 48_000);
        assert!(out.iter().all(|v| v.abs() < 1e-6), "out of range is silent");
    }

    #[test]
    fn outgoing_mode_and_incoming_volume_are_independent() {
        let shared = Arc::new(Shared::new(Settings {
            activation: Activation::Disabled,
            ..Settings::default()
        }));
        let mut mixer = Mixer::new(shared.clone());
        speak(&mut mixer, 3, [6.0, 0.0, 0.0], 8, 0);
        let mut out = vec![0.0; 4800 * 2];
        mixer.render(&mut out, 2, 48_000);
        assert!(
            energy(&out, 2, 1) > 0.1,
            "Disabled microphone, voices still heard"
        );
        // Volume 0 silences what is heard but the voice chat stays on, and
        // the outgoing side does not care either way.
        shared.apply(Settings {
            volume: 0.0,
            activation: Activation::PushToTalk,
            ..Settings::default()
        });
        speak(&mut mixer, 3, [6.0, 0.0, 0.0], 8, 100);
        mixer.render(&mut out, 2, 48_000);
        mixer.render(&mut out, 2, 48_000);
        assert!(out.iter().all(|v| v.abs() < 1e-6), "volume 0 is silence");
        assert_eq!(mixer.speaking(48_000), vec![3], "still tracked");
        shared.set_talking(true);
        let mut encoder = Encoder::new(shared.clone());
        assert_eq!(feed(&mut encoder, &tone(4, 0.5)).len(), 4);
    }

    #[test]
    fn late_and_duplicate_frames_are_dropped() {
        let shared = Arc::new(Shared::new(Settings::default()));
        let mut mixer = Mixer::new(shared);
        speak(&mut mixer, 1, [1.0, 0.0, 0.0], 4, 10);
        let held = mixer.speakers[&1].jitter.len();
        speak(&mut mixer, 1, [1.0, 0.0, 0.0], 4, 10);
        assert_eq!(mixer.speakers[&1].jitter.len(), held);
    }

    #[test]
    fn the_buffer_stays_bounded_and_speakers_are_capped() {
        let shared = Arc::new(Shared::new(Settings::default()));
        let mut mixer = Mixer::new(shared);
        speak(&mut mixer, 1, [1.0, 0.0, 0.0], 40, 0);
        assert!(mixer.speakers[&1].jitter.len() <= MAX_BUFFERED_FRAMES);
        for who in 0..(MAX_SPEAKERS as u64 + 4) {
            speak(&mut mixer, 100 + who, [1.0, 0.0, 0.0], 1, 0);
        }
        assert_eq!(mixer.speakers.len(), MAX_SPEAKERS);
        assert!(mixer.refused > 0);
        mixer.hear(1, [0.0; 3], 99, &[0xff]);
        assert_eq!(mixer.malformed, 1);
    }

    #[test]
    fn a_silent_speaker_is_forgotten_and_playback_underruns_quietly() {
        let shared = Arc::new(Shared::new(Settings::default()));
        let mut mixer = Mixer::new(shared);
        speak(&mut mixer, 1, [2.0, 0.0, 0.0], 4, 0);
        let mut out = vec![0.0; 48_000 / 2 * 2];
        for _ in 0..10 {
            mixer.render(&mut out, 2, 48_000);
        }
        assert!(mixer.speaking(48_000).is_empty());
        assert!(out.iter().all(|v| v.abs() < 1e-3), "ends in silence");
        for _ in 0..8 {
            mixer.render(&mut out, 2, 48_000);
        }
        assert!(mixer.speakers.is_empty());
    }

    #[test]
    fn speaker_levels_track_loudness_and_fall_after_silence() {
        let shared = Arc::new(Shared::new(Settings::default()));
        let mut mixer = Mixer::new(shared);
        speak(&mut mixer, 5, [2.0, 0.0, 0.0], 8, 0);
        let mut out = vec![0.0; 4800 * 2];
        mixer.render(&mut out, 2, 48_000);
        // A loud nearby voice reads as loud at once.
        assert!(mixer.levels()[&5] > 0.1, "loud speaker has a level");
        let loud = mixer.levels()[&5];
        // Silence: the level keeps falling over about half a second.
        for _ in 0..20 {
            mixer.render(&mut out, 2, 48_000);
        }
        assert!(mixer.levels()[&5] < loud / 10.0, "the level falls off");
        // A quiet voice reads quieter than a loud one at the same spot.
        let mut soft = Mixer::new(Arc::new(Shared::new(Settings::default())));
        let audio: Vec<f32> = (0..8 * FRAME_SAMPLES)
            .map(|i| 0.05 * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 16_000.0).sin())
            .collect();
        for (n, chunk) in audio.chunks(FRAME_SAMPLES).enumerate() {
            let bytes = Codec::encode(&pcm(chunk));
            soft.hear(5, [2.0, 0.0, 0.0], n as u16, &bytes);
        }
        soft.render(&mut out, 2, 48_000);
        assert!(
            soft.levels()[&5] < loud / 2.0,
            "quiet reads quieter than loud"
        );
    }

    #[test]
    fn mono_output_and_bad_settings_are_handled() {
        let shared = Arc::new(Shared::new(Settings {
            volume: f32::NAN,
            range: -5.0,
            mic_gain: 100.0,
            ..Settings::default()
        }));
        assert_eq!(load(&shared.volume), 1.0);
        assert!(load(&shared.range) >= FULL_VOLUME_DISTANCE);
        assert_eq!(load(&shared.mic_gain), 4.0);
        let mut mixer = Mixer::new(shared);
        speak(&mut mixer, 1, [2.0, 0.0, 0.0], 6, 0);
        let mut out = vec![0.0; 4800];
        mixer.render(&mut out, 1, 48_000);
        assert!(out.iter().any(|v| v.abs() > 0.01));
        mixer.render(&mut [], 2, 48_000);
    }
}
