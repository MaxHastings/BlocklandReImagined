//! Room reverb for sound effects: a small bank of feedback delays and allpasses (a Schroeder reverb) that the engine feeds with
//! the effects mix and adds back, so a space can have a tail. The room is set with `Command::SetReverb`; a wet level of zero is
//! off and costs nothing.

use bri_console::Clamp;

/// Delay line lengths in seconds, mutually unrelated so the echoes do not line up (the classic Freeverb comb times).
const COMB_SECONDS: [f32; 4] = [0.0297, 0.0371, 0.0411, 0.0437];
/// Allpass lengths in seconds.
const ALLPASS_SECONDS: [f32; 2] = [0.0050, 0.0017];
/// Allpass feedback.
const ALLPASS_GAIN: f32 = 0.5;
/// Seconds over which the wet level and the decay follow a new room.
const GLIDE_SECONDS: f32 = 0.35;
/// Largest decay time asked for, in seconds.
pub const MAX_DECAY_SECONDS: f32 = 10.0;
/// The tail is this many decibels down after the decay time.
const DECAY_DB: f32 = 60.0;
/// The left and right channels' delays differ by this many frames at 48 kHz, to spread the tail.
const STEREO_SPREAD: usize = 23;

/// A room: how much of the effects mix returns as reverb and how long the tail lasts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Room {
    /// Linear level of the reverb added to the dry mix; 0 is a dry room.
    pub wet: f32,
    /// Seconds for the tail to fall 60 dB.
    pub decay_seconds: f32,
}

impl Room {
    pub const DRY: Self = Self {
        wet: 0.0,
        decay_seconds: 0.0,
    };

    pub fn sanitized(self) -> Self {
        let clean = |v: f32, high: f32| {
            if v.is_finite() {
                v.clamped(0.0, high)
            } else {
                0.0
            }
        };
        Self {
            wet: clean(self.wet, 1.0),
            decay_seconds: clean(self.decay_seconds, MAX_DECAY_SECONDS),
        }
    }
}

struct Delay {
    line: Vec<f32>,
    at: usize,
}

impl Delay {
    fn new(frames: usize) -> Self {
        Self {
            line: vec![0.0; frames.max(1)],
            at: 0,
        }
    }
    fn len(&self) -> usize {
        self.line.len()
    }
    fn comb(&mut self, input: f32, feedback: f32) -> f32 {
        let out = self.line[self.at];
        self.line[self.at] = input + out * feedback;
        self.at = (self.at + 1) % self.line.len();
        out
    }
    fn allpass(&mut self, input: f32) -> f32 {
        let held = self.line[self.at];
        let out = held - input;
        self.line[self.at] = input + held * ALLPASS_GAIN;
        self.at = (self.at + 1) % self.line.len();
        out
    }
}

struct Side {
    combs: Vec<Delay>,
    allpasses: Vec<Delay>,
}

impl Side {
    fn new(rate: u32, extra: usize) -> Self {
        let frames = |seconds: f32| (seconds * rate as f32) as usize + extra;
        Self {
            combs: COMB_SECONDS
                .iter()
                .map(|s| Delay::new(frames(*s)))
                .collect(),
            allpasses: ALLPASS_SECONDS
                .iter()
                .map(|s| Delay::new(frames(*s)))
                .collect(),
        }
    }
    fn process(&mut self, input: f32, rate: u32, decay_seconds: f32) -> f32 {
        let mut sum = 0.0;
        for comb in &mut self.combs {
            // A tail that falls DECAY_DB over `decay_seconds` loses this much each trip round the delay.
            let trip = comb.len() as f32 / rate as f32;
            let feedback = 10f32.powf(-DECAY_DB / 20.0 * trip / decay_seconds.max(0.05));
            sum += comb.comb(input, feedback);
        }
        let mut out = sum / COMB_SECONDS.len() as f32;
        for allpass in &mut self.allpasses {
            out = allpass.allpass(out);
        }
        out
    }
}

/// The reverb the engine runs on the effects mix.
pub struct Reverb {
    rate: u32,
    left: Side,
    right: Side,
    /// The room as it is now, gliding to `target`.
    now: Room,
    target: Room,
}

impl Reverb {
    pub fn new(rate: u32) -> Self {
        let rate = rate.max(1);
        let spread = STEREO_SPREAD * rate as usize / 48_000;
        Self {
            rate,
            left: Side::new(rate, 0),
            right: Side::new(rate, spread),
            now: Room::DRY,
            target: Room::DRY,
        }
    }

    pub fn set_room(&mut self, room: Room) {
        self.target = room.sanitized();
        if self.now.decay_seconds == 0.0 {
            self.now.decay_seconds = self.target.decay_seconds;
        }
    }

    pub fn set_rate(&mut self, rate: u32) {
        if rate != self.rate {
            *self = Self {
                now: self.now,
                target: self.target,
                ..Self::new(rate)
            };
        }
    }

    /// Whether the reverb is doing anything (wet, or still ringing out of a room just left).
    pub fn active(&self) -> bool {
        self.now.wet > 1e-4 || self.target.wet > 1e-4
    }

    /// Add the reverb of `effects` (interleaved, `channels` wide) to `out`.
    pub fn add_to(&mut self, effects: &[f32], out: &mut [f32], channels: usize) {
        let frames = effects.len() / channels.max(1);
        let glide = 1.0 / (GLIDE_SECONDS * self.rate as f32);
        for n in 0..frames {
            self.now.wet += (self.target.wet - self.now.wet) * glide;
            self.now.decay_seconds += (self.target.decay_seconds - self.now.decay_seconds) * glide;
            let base = n * channels;
            let mono = if channels >= 2 {
                0.5 * (effects[base] + effects[base + 1])
            } else {
                effects[base]
            };
            let decay = self.now.decay_seconds;
            let l = self.left.process(mono, self.rate, decay) * self.now.wet;
            out[base] += l;
            if channels >= 2 {
                let r = self.right.process(mono, self.rate, decay) * self.now.wet;
                out[base + 1] += r;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum()
    }

    fn click_then_silence(reverb: &mut Reverb, blocks: usize) -> Vec<f32> {
        let mut tail = Vec::new();
        for block in 0..blocks {
            let mut input = vec![0.0f32; 512];
            if block == 0 {
                input[0] = 1.0;
                input[1] = 1.0;
            }
            let mut out = vec![0.0f32; 512];
            reverb.add_to(&input, &mut out, 2);
            tail.extend(out);
        }
        tail
    }

    #[test]
    fn a_dry_room_adds_nothing() {
        let mut reverb = Reverb::new(48_000);
        assert!(!reverb.active());
        let tail = click_then_silence(&mut reverb, 20);
        assert_eq!(energy(&tail), 0.0);
    }

    #[test]
    fn a_wet_room_rings_after_a_click_and_a_longer_decay_rings_longer() {
        let ring = |decay_seconds: f32| {
            let mut reverb = Reverb::new(48_000);
            reverb.set_room(Room {
                wet: 1.0,
                decay_seconds,
            });
            // Let the room glide in before the click.
            for _ in 0..200 {
                let mut out = vec![0.0f32; 512];
                reverb.add_to(&vec![0.0; 512], &mut out, 2);
            }
            let tail = click_then_silence(&mut reverb, 100);
            energy(&tail[tail.len() / 2..])
        };
        let short = ring(0.4);
        let long = ring(3.0);
        assert!(short > 0.0);
        assert!(long > short * 10.0, "{long} vs {short}");
    }

    #[test]
    fn nonsense_rooms_are_clamped() {
        let room = Room {
            wet: f32::NAN,
            decay_seconds: 1e9,
        }
        .sanitized();
        assert_eq!(room.wet, 0.0);
        assert_eq!(room.decay_seconds, MAX_DECAY_SECONDS);
    }
}
