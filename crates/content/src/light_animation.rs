//! map2dif's animated map lights: a strobe, pulse, pulse2, flicker or
//! runway light goes between its colour and a second colour over time.
//! The settings are the light node's properties (docs/modding/maps.md):
//! `speed` (map2dif's very slow to very fast, or 0 to 4) or `period` in
//! seconds, `color2` (off by default), `phase` (0 to 1, to set lights in a
//! row apart) and, for pulse2, `attack`, `sustain1`, `decay` and `sustain2`
//! in seconds. A runway light flashes once a period for `steps`' share of
//! it; give each light of a runway its place in the row as its `phase`.
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Strobe,
    Pulse,
    /// Up from the first colour, held, down again, held (seconds each).
    Pulse2([f32; 4]),
    Flicker,
    /// On for this share of the period.
    Runway(f32),
}

/// How an animated light's colour changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightAnimation {
    shape: Shape,
    period: f32,
    phase: f32,
    color: [f32; 3],
    color2: [f32; 3],
}

/// map2dif's speeds, slowest first: seconds per cycle.
const SPEEDS: [(&str, f32); 5] = [
    ("very slow", 4.0),
    ("slow", 2.0),
    ("normal", 1.0),
    ("fast", 0.5),
    ("very fast", 0.25),
];

impl LightAnimation {
    /// The animation of a light of `kind` with `properties`, whose colour
    /// times brightness is `color`; `None` for a steady light.
    pub fn read(
        kind: &str,
        properties: &BTreeMap<String, String>,
        color: [f32; 3],
        brightness: f32,
    ) -> Result<Option<Self>> {
        let number = |key: &str, default: f32| -> Result<f32> {
            let value = match properties.get(key) {
                Some(v) => v
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .with_context(|| format!("{key} must be a number of 0 or more"))?,
                None => default,
            };
            Ok(value)
        };
        let shape = match kind {
            "strobe" => Shape::Strobe,
            "pulse" => Shape::Pulse,
            "pulse2" => Shape::Pulse2([
                number("attack", 0.25)?,
                number("sustain1", 0.25)?,
                number("decay", 0.25)?,
                number("sustain2", 0.25)?,
            ]),
            "flicker" => Shape::Flicker,
            "runway" => Shape::Runway(1.0 / number("steps", 4.0)?.max(1.0)),
            _ => return Ok(None),
        };
        let period = match (shape, properties.get("period"), properties.get("speed")) {
            (Shape::Pulse2(t), _, _) => t.iter().sum(),
            (_, Some(_), _) => number("period", 1.0)?,
            (_, None, Some(speed)) => {
                let speed = speed.trim().to_ascii_lowercase().replace('_', " ");
                SPEEDS
                    .iter()
                    .enumerate()
                    .find(|(k, (name, _))| *name == speed || k.to_string() == speed)
                    .map(|(_, (_, s))| *s)
                    .with_context(
                        || "speed must be very slow, slow, normal, fast or very fast (or 0 to 4)",
                    )?
            }
            _ => 1.0,
        };
        ensure!(
            period > 0.001 && period <= 3600.0,
            "an animated light's cycle must be above 0.001 and at most 3600 seconds"
        );
        let color2 = match properties.get("color2") {
            Some(v) => {
                let c: Vec<f32> = v
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .ok()
                    .filter(|c: &Vec<f32>| {
                        c.len() == 3 && c.iter().all(|c| c.is_finite() && *c >= 0.0)
                    })
                    .context("color2 must be three numbers of 0 or more")?;
                [c[0] * brightness, c[1] * brightness, c[2] * brightness]
            }
            None => [0.0; 3],
        };
        Ok(Some(Self {
            shape,
            period,
            phase: number("phase", 0.0)?.fract(),
            color,
            color2,
        }))
    }

    /// How far from the first colour (0) to the second (1) at `seconds`.
    pub fn mix_at(&self, seconds: f64) -> f32 {
        let cycles = seconds / f64::from(self.period) + f64::from(self.phase);
        let t = cycles.fract() as f32;
        match self.shape {
            Shape::Strobe => f32::from(u8::from(t >= 0.5)),
            Shape::Pulse => 0.5 - 0.5 * (t * std::f32::consts::TAU).cos(),
            Shape::Pulse2([attack, sustain1, decay, _]) => {
                let s = t * self.period;
                if s < attack {
                    s / attack.max(1e-6)
                } else if s < attack + sustain1 {
                    1.0
                } else if s < attack + sustain1 + decay {
                    1.0 - (s - attack - sustain1) / decay.max(1e-6)
                } else {
                    0.0
                }
            }
            Shape::Flicker => {
                // A new random level eight times a cycle, the same on every
                // computer.
                let step = (cycles * 8.0).floor() as i64 as u64;
                let mut x = step.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                x ^= x >> 31;
                x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
                x ^= x >> 29;
                (x >> 40) as f32 / (1u64 << 24) as f32
            }
            Shape::Runway(on) => f32::from(u8::from(t >= on)),
        }
    }

    /// The light's colour at `seconds`, as a multiple of its own colour per
    /// channel (what the renderer's light tints take). A channel the light's
    /// own colour lacks follows its overall brightness.
    pub fn tint_at(&self, seconds: f64) -> [f32; 3] {
        let w = self.mix_at(seconds);
        let now: Vec<f32> = (0..3)
            .map(|c| self.color[c] + (self.color2[c] - self.color[c]) * w)
            .collect();
        let sum = |c: &[f32]| c.iter().sum::<f32>();
        let overall = if sum(&self.color) > 0.0 {
            sum(&now) / sum(&self.color)
        } else {
            0.0
        };
        [0, 1, 2].map(|c| {
            if self.color[c] > 1e-6 {
                now[c] / self.color[c]
            } else {
                overall
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(kind: &str, properties: &[(&str, &str)]) -> LightAnimation {
        let properties = properties
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        LightAnimation::read(kind, &properties, [1.0, 0.5, 0.0], 2.0)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn steady_lights_have_no_animation() {
        for kind in ["omni", "spot"] {
            assert!(
                LightAnimation::read(kind, &BTreeMap::new(), [1.0; 3], 1.0)
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn a_strobe_goes_off_and_on_at_its_speed() {
        let strobe = read("strobe", &[("speed", "fast")]);
        assert_eq!(strobe.tint_at(0.1), [1.0; 3]);
        assert_eq!(strobe.tint_at(0.3), [0.0; 3]);
        assert_eq!(strobe.tint_at(0.6), [1.0; 3]);
        // Half a cycle apart, the next light of a row is off when this is on.
        let next = read("strobe", &[("speed", "3"), ("phase", "0.5")]);
        assert_eq!(next.tint_at(0.1), [0.0; 3]);
    }

    #[test]
    fn a_pulse_fades_to_its_second_colour_and_back() {
        let pulse = read("pulse", &[("period", "2"), ("color2", "0 0.25 0.5")]);
        assert_eq!(pulse.tint_at(0.0), [1.0; 3]);
        // At the middle of the cycle it is color2 times brightness:
        // (0, 0.5, 1) against its own (1, 0.5, 0); blue follows the total.
        let [r, g, b] = pulse.tint_at(1.0);
        assert!(r.abs() < 1e-5 && (g - 1.0).abs() < 1e-5 && (b - 1.0).abs() < 1e-5);
    }

    #[test]
    fn pulse2_keeps_its_own_timing() {
        let p = read(
            "pulse2",
            &[
                ("attack", "1"),
                ("sustain1", "2"),
                ("decay", "1"),
                ("sustain2", "4"),
            ],
        );
        assert_eq!(p.mix_at(0.5), 0.5);
        assert_eq!(p.mix_at(2.0), 1.0);
        assert_eq!(p.mix_at(3.5), 0.5);
        assert_eq!(p.mix_at(6.0), 0.0);
        assert_eq!(p.mix_at(8.5), 0.5, "the cycle is the four times added");
    }

    #[test]
    fn flicker_and_runway() {
        let f = read("flicker", &[]);
        let levels: Vec<f32> = (0..16).map(|k| f.mix_at(k as f64 / 8.0 + 0.01)).collect();
        assert!(levels.iter().all(|l| (0.0..1.0).contains(l)));
        assert!(levels.windows(2).any(|w| w[0] != w[1]));
        assert_eq!(f.mix_at(3.3), read("flicker", &[]).mix_at(3.3));
        let runway = read("runway", &[("steps", "4")]);
        assert_eq!(runway.mix_at(0.1), 0.0);
        assert_eq!(runway.mix_at(0.5), 1.0);
    }

    #[test]
    fn bad_settings_are_named() {
        let bad = |k: &str, v: &str| {
            let p = BTreeMap::from([(k.to_string(), v.to_string())]);
            LightAnimation::read("strobe", &p, [1.0; 3], 1.0)
                .unwrap_err()
                .to_string()
        };
        assert!(bad("speed", "ludicrous").contains("very slow"));
        assert!(bad("color2", "red").contains("three numbers"));
        assert!(bad("period", "0").contains("cycle"));
    }
}
