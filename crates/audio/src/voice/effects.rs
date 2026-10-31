//! Bounded mono voice processing. Packages select parameters off the audio path;
//! this owns only native signal processing, never radio/channel policy.

/// Linear gain and optional first-order filters, applied before local volume
/// and the mix limiter. Zero disables a cutoff; clear speech is the default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Effects {
    pub gain: f32,
    pub high_pass_hz: f32,
    pub low_pass_hz: f32,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            gain: 1.0,
            high_pass_hz: 0.0,
            low_pass_hz: 0.0,
        }
    }
}

impl Effects {
    pub fn valid(self) -> bool {
        [self.gain, self.high_pass_hz, self.low_pass_hz]
            .into_iter()
            .all(f32::is_finite)
            && (0.0..=2.0).contains(&self.gain)
            && (self.high_pass_hz == 0.0 || (40.0..=4000.0).contains(&self.high_pass_hz))
            && (self.low_pass_hz == 0.0 || (100.0..=7500.0).contains(&self.low_pass_hz))
            && (self.high_pass_hz == 0.0
                || self.low_pass_hz == 0.0
                || self.high_pass_hz < self.low_pass_hz)
    }
}

pub(super) struct Processor {
    target: Effects,
    gain: f32,
    high: f32,
    low: f32,
    high_wet: f32,
    low_wet: f32,
    coefficients: [f32; 2],
    desired: [f32; 2],
    glide: f32,
    rate: u32,
}

impl Default for Processor {
    fn default() -> Self {
        Self {
            target: Effects::default(),
            gain: 1.0,
            high: 0.0,
            low: 0.0,
            high_wet: 0.0,
            low_wet: 0.0,
            coefficients: [1.0; 2],
            desired: [1.0; 2],
            glide: 0.0,
            rate: 0,
        }
    }
}

impl Processor {
    pub(super) fn set(&mut self, effects: Effects) {
        if effects.valid() && effects != self.target {
            self.target = effects;
            self.rate = 0;
        }
    }

    pub(super) fn process(&mut self, sample: f32, rate: u32) -> f32 {
        if self.target == Effects::default()
            && self.gain == 1.0
            && self.high_wet == 0.0
            && self.low_wet == 0.0
        {
            self.high = sample;
            self.low = sample;
            return sample;
        }
        let rate = rate.max(1);
        if self.rate != rate {
            self.rate = rate;
            let rate = rate as f32;
            self.glide = 1.0 - (-1.0 / (0.02 * rate)).exp();
            self.desired = [self.target.high_pass_hz, self.target.low_pass_hz].map(|hz| {
                if hz == 0.0 {
                    1.0
                } else {
                    1.0 - (-std::f32::consts::TAU * hz.min(rate * 0.45) / rate).exp()
                }
            });
        }
        // Sample-clock smoothing is independent of packet/render block sizes.
        fn approach(current: &mut f32, target: f32, glide: f32) {
            let next = *current + (target - *current) * glide;
            // A sub-ULP update must converge too, including at high output
            // rates. Otherwise clear/bypass can remain slightly wet forever.
            *current = if next == *current || (next - target).abs() < 1e-6 {
                target
            } else {
                next
            };
        }
        approach(&mut self.gain, self.target.gain, self.glide);
        approach(
            &mut self.high_wet,
            if self.target.high_pass_hz != 0.0 {
                1.0
            } else {
                0.0
            },
            self.glide,
        );
        approach(
            &mut self.low_wet,
            if self.target.low_pass_hz != 0.0 {
                1.0
            } else {
                0.0
            },
            self.glide,
        );
        for (coefficient, target) in self.coefficients.iter_mut().zip(self.desired) {
            approach(coefficient, target, self.glide);
        }
        self.high += (sample - self.high) * self.coefficients[0];
        let high = sample - self.high * self.high_wet;
        self.low += (high - self.low) * self.coefficients[1];
        (high + (self.low - high) * self.low_wet) * self.gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(effects: Effects, hz: f32, rate: u32) -> f32 {
        let mut filter = Processor::default();
        filter.set(effects);
        let mut power = 0.0;
        for i in 0..rate {
            let input = (i as f32 * std::f32::consts::TAU * hz / rate as f32).sin() * 0.25;
            let output = filter.process(input, rate);
            assert!(output.is_finite());
            if i >= rate / 2 {
                power += output * output;
            }
        }
        (power / (rate / 2) as f32).sqrt()
    }

    #[test]
    fn clear_is_exact_and_gain_is_linear_at_device_rates() {
        for rate in [8000, 16000, 44100, 48000, 96000] {
            let mut p = Processor::default();
            for input in [0.0, 0.25, -0.7, 0.6, 0.0] {
                assert_eq!(p.process(input, rate), input);
            }
            let dry = rms(Effects::default(), 440.0, rate);
            let quiet = rms(
                Effects {
                    gain: 0.25,
                    ..Default::default()
                },
                440.0,
                rate,
            );
            assert!((quiet / dry - 0.25).abs() < 0.001);
        }
    }

    #[test]
    fn invalid_native_parameters_do_not_change_the_clear_processor() {
        for effects in [
            Effects {
                gain: f32::NAN,
                ..Default::default()
            },
            Effects {
                gain: f32::INFINITY,
                ..Default::default()
            },
            Effects {
                gain: -0.1,
                ..Default::default()
            },
            Effects {
                gain: 2.1,
                ..Default::default()
            },
            Effects {
                high_pass_hz: 1.0,
                ..Default::default()
            },
            Effects {
                low_pass_hz: 8000.0,
                ..Default::default()
            },
            Effects {
                high_pass_hz: 3000.0,
                low_pass_hz: 1000.0,
                ..Default::default()
            },
        ] {
            assert!(!effects.valid());
            let mut p = Processor::default();
            p.set(effects);
            assert_eq!(p.process(0.5, 48000), 0.5);
        }
    }

    #[test]
    fn composed_filters_remove_low_and_high_bands_and_keep_speech_band() {
        let effects = Effects {
            gain: 1.0,
            high_pass_hz: 300.0,
            low_pass_hz: 3000.0,
        };
        for rate in [16000, 44100, 48000] {
            let dry = rms(Effects::default(), 1000.0, rate);
            let mid = rms(effects, 1000.0, rate);
            assert!(mid > dry * 0.75, "rate={rate}: speech band {mid}/{dry}");
            assert!(rms(effects, 40.0, rate) < mid * 0.2);
            assert!(rms(effects, 6500.0, rate) < mid * 0.6);
        }
    }

    #[test]
    fn live_changes_smooth_and_bypass_recovers_without_filter_tail() {
        let mut p = Processor::default();
        p.set(Effects {
            gain: 0.0,
            high_pass_hz: 1000.0,
            low_pass_hz: 2000.0,
        });
        let mut last = 0.5;
        for _ in 0..16000 {
            let next = p.process(0.5, 16000);
            assert!((next - last).abs() < 0.01);
            last = next;
        }
        assert!(last.abs() < 1e-5);
        p.set(Effects::default());
        for _ in 0..16000 {
            last = p.process(0.5, 16000);
        }
        assert_eq!(last, 0.5);
        // A revoked route drops its processor with its decoder, rather than
        // carrying filter memory to a different route/speaker generation.
        let mut fresh = Processor::default();
        assert_eq!(fresh.process(0.0, 48000), 0.0);
    }
}
