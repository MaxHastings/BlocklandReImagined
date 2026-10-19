//! Light baked into a mesh's vertices from the lights that reach it.
//!
//! A map places lights that never move; a mesh set among them takes their light at each vertex once, when the map loads. Each light
//! has a colour, a radius and an effect; a bit per vertex says whether the light reaches that vertex (the map's own visibility
//! test), so nothing here traces. The vertex keeps its light as a colour that a lit surface multiplies, doubled, into its picture
//! (0.5 is neutral), so the sum is
//!
//! `colour = clamp(0.5 * ambient + sum_i  colour_i * glow_i * falloff_i * max(n . l_i, 0))`
//!
//! with `falloff` the cubic `1 - 3 t^2 + 2 t^3` of `t = distance / radius` for a point light, and `-n . direction` (no falloff) for
//! a directional one. A negative light subtracts. The result is saturated to 0..1 and then multiplied by the vertex's own colour.
use anyhow::{Result, ensure};
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Effect {
    /// Cubic falloff out to the light's radius.
    Point,
    /// Parallel light travelling along `direction`; reaches everywhere.
    Directional { direction: [f32; 3] },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Light {
    pub position: [f32; 3],
    /// Linear RGB, 0..1 per unit brightness.
    pub color: [f32; 3],
    pub radius: f32,
    /// A scale on the light's strength for this mesh.
    #[serde(default = "one")]
    pub glow: f32,
    pub effect: Effect,
    /// The light darkens instead.
    #[serde(default)]
    pub negative: bool,
}

fn one() -> f32 {
    1.0
}

/// The lights that reach one mesh and, per light, a bit per vertex (least significant bit first) for whether it reaches that vertex.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Set {
    /// The ambient light the mesh takes where no light reaches.
    #[serde(default)]
    pub ambient: [f32; 3],
    pub lights: Vec<Light>,
    /// One byte string per light, `ceil(vertices / 8)` long.
    pub visible: Vec<Vec<u8>>,
}

impl Set {
    pub fn validate(&self, vertices: usize) -> Result<()> {
        ensure!(
            self.visible.len() == self.lights.len(),
            "A mesh's vertex lights need a visibility mask each"
        );
        ensure!(self.lights.len() <= 256, "A mesh takes at most 256 lights");
        for mask in &self.visible {
            ensure!(
                mask.len() >= vertices.div_ceil(8),
                "A light's visibility mask is shorter than the mesh's vertices"
            );
        }
        for l in &self.lights {
            ensure!(
                l.position.iter().chain(&l.color).all(|v| v.is_finite())
                    && l.radius.is_finite()
                    && l.radius > 0.0
                    && l.glow.is_finite(),
                "A vertex light has a non-finite value or no radius"
            );
        }
        Ok(())
    }

    /// The light at vertex `index` (a position and unit normal in the lights' frame).
    pub fn at(&self, index: usize, position: Vec3, normal: Vec3) -> [f32; 3] {
        let mut sum = Vec3::from(self.ambient) * 0.5;
        for (light, mask) in self.lights.iter().zip(&self.visible) {
            if mask
                .get(index / 8)
                .is_none_or(|b| b >> (index % 8) & 1 == 0)
            {
                continue;
            }
            let strength = match light.effect {
                Effect::Point => {
                    let delta = Vec3::from(light.position) - position;
                    let distance = delta.length();
                    if distance > light.radius || distance <= 0.0 {
                        continue;
                    }
                    let t = distance / light.radius;
                    let facing = normal.dot(delta / distance);
                    if facing <= 0.0 {
                        continue;
                    }
                    (1.0 - 3.0 * t * t + 2.0 * t * t * t) * facing
                }
                Effect::Directional { direction } => {
                    let facing = -normal.dot(Vec3::from(direction).normalize_or_zero());
                    if facing <= 0.0 {
                        continue;
                    }
                    facing
                }
            };
            let add = Vec3::from(light.color) * (light.glow * strength);
            if light.negative {
                sum -= add;
            } else {
                sum += add;
            }
            sum = sum.clamp(Vec3::ZERO, Vec3::ONE);
        }
        sum.to_array()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(position: [f32; 3], radius: f32) -> Light {
        Light {
            position,
            color: [1.0, 0.5, 0.0],
            radius,
            glow: 1.0,
            effect: Effect::Point,
            negative: false,
        }
    }

    #[test]
    fn a_point_light_takes_the_cubic_falloff_and_facing() {
        let set = Set {
            ambient: [0.2, 0.2, 0.2],
            lights: vec![point([0.0, 2.0, 0.0], 4.0)],
            visible: vec![vec![0b1]],
        };
        set.validate(1).unwrap();
        // Straight below the light, half the radius away: t = 0.5, 1 - 0.75 + 0.25 = 0.5, facing 1.
        let c = set.at(0, Vec3::ZERO, Vec3::Y);
        assert!((c[0] - (0.1 + 0.5)).abs() < 1e-6, "{c:?}");
        assert!((c[1] - (0.1 + 0.25)).abs() < 1e-6);
        assert!((c[2] - 0.1).abs() < 1e-6);
        // Facing away, past the radius, or out of the mask: ambient only.
        assert_eq!(set.at(0, Vec3::ZERO, -Vec3::Y), [0.1; 3]);
        assert_eq!(set.at(0, Vec3::new(0.0, -10.0, 0.0), Vec3::Y), [0.1; 3]);
        assert_eq!(set.at(1, Vec3::ZERO, Vec3::Y), [0.1; 3]);
    }

    #[test]
    fn lights_saturate_and_a_negative_light_darkens() {
        let mut bright = point([0.0, 1.0, 0.0], 4.0);
        bright.glow = 10.0;
        let set = Set {
            ambient: [0.0; 3],
            lights: vec![bright],
            visible: vec![vec![1]],
        };
        assert_eq!(set.at(0, Vec3::ZERO, Vec3::Y)[0], 1.0);
        let mut dark = point([0.0, 1.0, 0.0], 4.0);
        dark.negative = true;
        dark.color = [1.0; 3];
        let both = Set {
            ambient: [0.4; 3],
            lights: vec![dark],
            visible: vec![vec![1]],
        };
        assert_eq!(both.at(0, Vec3::ZERO, Vec3::Y), [0.0; 3]);
    }

    #[test]
    fn a_directional_light_has_no_falloff() {
        let set = Set {
            ambient: [0.0; 3],
            lights: vec![Light {
                effect: Effect::Directional {
                    direction: [0.0, -1.0, 0.0],
                },
                color: [0.5; 3],
                ..point([0.0; 3], 1.0)
            }],
            visible: vec![vec![1]],
        };
        assert_eq!(set.at(0, Vec3::new(500.0, 0.0, 0.0), Vec3::Y), [0.5; 3]);
    }

    #[test]
    fn short_masks_and_broken_lights_are_refused() {
        let set = Set {
            ambient: [0.0; 3],
            lights: vec![point([0.0; 3], 1.0)],
            visible: vec![vec![0]],
        };
        assert!(set.validate(9).is_err());
        let mut bad = set.clone();
        bad.lights[0].radius = 0.0;
        assert!(bad.validate(1).is_err());
        bad.visible.clear();
        assert!(bad.validate(1).is_err());
    }
}
