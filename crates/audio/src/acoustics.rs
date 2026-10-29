//! Per-voice cosmetic acoustics, independent of placement, loop phase and fades.

/// Listener-side gain and low-pass blend. No serialized game/wire state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Acoustics {
    pub gain: f32,
    /// 0 keeps the exact dry signal; 1 selects a 1200Hz one-pole low-pass.
    pub muffle: f32,
}

impl Acoustics {
    pub const CLEAR: Self = Self {
        gain: 1.0,
        muffle: 0.0,
    };

    fn sanitized(self) -> Self {
        Self {
            gain: if self.gain.is_finite() {
                self.gain.clamp(0.0, 1.0)
            } else {
                1.0
            },
            muffle: if self.muffle.is_finite() {
                self.muffle.clamp(0.0, 1.0)
            } else {
                0.0
            },
        }
    }
}

pub(crate) struct VoiceAcoustics {
    target: Acoustics,
    current: Acoustics,
    low: [f32; 2],
    rate: u32,
    glide: f32,
    low_coefficient: f32,
}

impl Default for VoiceAcoustics {
    fn default() -> Self {
        Self {
            target: Acoustics::CLEAR,
            current: Acoustics::CLEAR,
            low: [0.0; 2],
            rate: 0,
            glide: 0.0,
            low_coefficient: 0.0,
        }
    }
}

impl VoiceAcoustics {
    pub fn set(&mut self, target: Acoustics) {
        self.target = target.sanitized();
    }

    pub fn process(&mut self, input: [f32; 2], rate: u32) -> [f32; 2] {
        if self.target == Acoustics::CLEAR && self.current == Acoustics::CLEAR {
            self.low = input;
            return input;
        }
        let rate = rate.max(1);
        if self.rate != rate {
            self.rate = rate;
            let rate = rate as f32;
            // Recompute only on device-rate changes, never per sample.
            self.glide = 1.0 - (-1.0 / (0.25 * rate)).exp();
            let cutoff = 1200.0_f32.min(rate * 0.45);
            self.low_coefficient = 1.0 - (-std::f32::consts::TAU * cutoff / rate).exp();
        }
        // Sample-clock smoothing is independent of render blocks or FPS.
        self.current.gain += (self.target.gain - self.current.gain) * self.glide;
        self.current.muffle += (self.target.muffle - self.current.muffle) * self.glide;
        if (self.current.gain - self.target.gain).abs() < 0.00001 {
            self.current.gain = self.target.gain;
        }
        if (self.current.muffle - self.target.muffle).abs() < 0.00001 {
            self.current.muffle = self.target.muffle;
        }
        std::array::from_fn(|channel| {
            self.low[channel] += (input[channel] - self.low[channel]) * self.low_coefficient;
            (input[channel] + (self.low[channel] - input[channel]) * self.current.muffle)
                * self.current.gain
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_is_exact_and_shelter_removes_more_high_frequency_energy() {
        let rate = 48000;
        let energy = |hz: f32, target: Acoustics| {
            let mut voice = VoiceAcoustics::default();
            voice.set(target);
            let mut sum = 0.0;
            for n in 0..rate * 3 {
                let sample = (n as f32 * std::f32::consts::TAU * hz / rate as f32).sin() * 0.2;
                let out = voice.process([sample, -sample], rate);
                if target == Acoustics::CLEAR {
                    assert_eq!(out, [sample, -sample]);
                }
                if n > rate * 2 {
                    sum += out[0] * out[0];
                }
            }
            sum
        };
        let blocked = Acoustics {
            gain: 0.3,
            muffle: 1.0,
        };
        let low = energy(200.0, blocked) / energy(200.0, Acoustics::CLEAR);
        let high = energy(8000.0, blocked) / energy(8000.0, Acoustics::CLEAR);
        assert!(low > 0.07 && low < 0.1, "{low}");
        assert!(high < low * 0.05, "low={low} high={high}");
    }
}
