//! Native effect definitions. All timings are seconds and all references native IDs.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Curve {
    pub period: f32,
    pub linear: bool,
    pub values: Vec<f32>,
}
impl Curve {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.period.is_finite() && self.period > 0.0 && self.period <= 3600.0,
            "Invalid effect period"
        );
        ensure!(
            !self.values.is_empty()
                && self.values.len() <= 256
                && self.values.iter().all(|v| v.is_finite()),
            "Invalid effect curve"
        );
        Ok(())
    }
    pub fn sample(&self, seconds: f32) -> f32 {
        let position =
            seconds.max(0.0).rem_euclid(self.period) / self.period * (self.values.len() - 1) as f32;
        let i = (position.floor() as usize).min(self.values.len() - 1);
        if self.linear {
            self.values[i]
                + (self.values[(i + 1).min(self.values.len() - 1)] - self.values[i])
                    * position.fract()
        } else {
            self.values[i]
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flare {
    pub texture: String,
    pub color: [f32; 3],
    pub third_person: bool,
    pub constant_size: Option<f32>,
    pub near_size: f32,
    pub far_size: f32,
    pub near_distance: f32,
    pub far_distance: f32,
    pub fade_seconds: f32,
    pub blend_mode: u8,
    pub link_color: bool,
    pub link_size: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Light {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub color: [f32; 3],
    pub brightness: f32,
    pub radius: f32,
    pub color_curves: Option<[Curve; 3]>,
    pub brightness_curve: Option<Curve>,
    pub radius_curve: Option<Curve>,
    pub flare: Option<Flare>,
}
impl Light {
    pub fn sample(&self, seconds: f32) -> ([f32; 3], f32) {
        let brightness = if self.enabled {
            self.brightness_curve
                .as_ref()
                .map_or(self.brightness, |c| c.sample(seconds))
        } else {
            0.0
        };
        let color = std::array::from_fn(|i| {
            self.color_curves
                .as_ref()
                .map_or(self.color[i], |c| c[i].sample(seconds))
                * brightness
        });
        (
            color,
            self.radius_curve
                .as_ref()
                .map_or(self.radius, |c| c.sample(seconds)),
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleKey {
    pub time: f32,
    pub color: [f32; 4],
    pub size: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Particle {
    pub id: String,
    pub texture: String,
    pub alpha_blend: bool,
    pub lifetime: f32,
    pub lifetime_variance: f32,
    pub drag: f32,
    pub wind: f32,
    pub gravity: f32,
    pub inherited_velocity: f32,
    pub acceleration: f32,
    pub spin_degrees: f32,
    pub random_spin: [f32; 2],
    pub keys: Vec<ParticleKey>,
}
impl Particle {
    pub fn sample(&self, age: f32) -> ([f32; 4], f32) {
        let age = age.clamp(0.0, 1.0);
        for pair in self.keys.windows(2) {
            if age <= pair[1].time {
                let span = pair[1].time - pair[0].time;
                let t = if span > 0.0 {
                    ((age - pair[0].time) / span).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                return (
                    std::array::from_fn(|i| {
                        pair[0].color[i] + (pair[1].color[i] - pair[0].color[i]) * t
                    }),
                    pair[0].size + (pair[1].size - pair[0].size) * t,
                );
            }
        }
        let end = self.keys.last().expect("validated particle keys");
        (end.color, end.size)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Emitter {
    pub id: String,
    pub name: String,
    pub particles: Vec<String>,
    pub period: f32,
    pub period_variance: f32,
    pub speed: f32,
    pub speed_variance: f32,
    pub offset: f32,
    pub offset_variance: f32,
    pub theta_degrees: [f32; 2],
    pub phi_rate_degrees: f32,
    pub phi_variance_degrees: f32,
    pub lifetime: f32,
    pub lifetime_variance: f32,
    pub orient: bool,
    pub orient_on_velocity: bool,
    pub override_advance: bool,
    pub use_emitter_colors: bool,
    pub use_emitter_sizes: bool,
    pub use_placement_velocity: bool,
    pub node_time_scale: f32,
    pub point_node_time_scale: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub schema_version: u32,
    pub lights: Vec<Light>,
    pub particles: Vec<Particle>,
    pub emitters: Vec<Emitter>,
    pub textures: BTreeMap<String, String>,
}
impl Library {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && self.lights.len() <= 4096
                && self.particles.len() <= 4096
                && self.emitters.len() <= 4096,
            "Invalid effect library"
        );
        // Validate before runtime sampling; original additive particles can have
        // authored alpha above one (the stock glow paint uses two).
        let mut ids = BTreeSet::new();
        for p in &self.particles {
            ensure!(
                ids.insert(&p.id) && self.textures.contains_key(&p.texture),
                "Duplicate particle or missing texture"
            );
            ensure!(
                p.lifetime.is_finite()
                    && p.lifetime > 0.0
                    && p.lifetime <= 3600.0
                    && p.lifetime_variance >= 0.0
                    && p.lifetime_variance < p.lifetime,
                "Invalid particle lifetime"
            );
            ensure!(
                [
                    p.drag,
                    p.wind,
                    p.gravity,
                    p.inherited_velocity,
                    p.acceleration,
                    p.spin_degrees,
                    p.random_spin[0],
                    p.random_spin[1]
                ]
                .iter()
                .all(|v| v.is_finite())
                    && p.drag >= 0.0
                    && p.random_spin[0] <= p.random_spin[1],
                "Invalid particle motion"
            );
            ensure!(
                (2..=4).contains(&p.keys.len())
                    && p.keys[0].time == 0.0
                    && p.keys.last().unwrap().time >= 1.0,
                "Invalid particle keys"
            );
            let mut previous = 0.0;
            for k in &p.keys {
                ensure!(
                    k.time.is_finite()
                        && k.time >= previous
                        && k.time <= 2.0
                        && k.size.is_finite()
                        && k.size >= 0.0
                        && k.color
                            .iter()
                            .all(|v| v.is_finite() && (0.0..=64.0).contains(v)),
                    "Invalid particle key value"
                );
                previous = k.time;
            }
        }
        let particles = ids.clone();
        for e in &self.emitters {
            ensure!(
                ids.insert(&e.id)
                    && !e.particles.is_empty()
                    && e.particles.len() <= 32
                    && e.particles.iter().all(|p| particles.contains(p)),
                "Missing emitter particle or duplicate identity"
            );
            let nums = [
                e.period,
                e.period_variance,
                e.speed,
                e.speed_variance,
                e.offset,
                e.offset_variance,
                e.theta_degrees[0],
                e.theta_degrees[1],
                e.phi_rate_degrees,
                e.phi_variance_degrees,
                e.lifetime,
                e.lifetime_variance,
                e.node_time_scale,
                e.point_node_time_scale,
            ];
            ensure!(
                nums.iter().all(|v| v.is_finite())
                    && e.period > 0.0
                    && e.period_variance >= 0.0
                    && e.period_variance < e.period
                    && e.theta_degrees[0] >= 0.0
                    && e.theta_degrees[1] <= 180.0
                    && e.theta_degrees[0] <= e.theta_degrees[1]
                    && e.node_time_scale > 0.0
                    && e.point_node_time_scale > 0.0,
                "Invalid emitter values"
            );
        }
        for l in &self.lights {
            ensure!(
                ids.insert(&l.id)
                    && l.color
                        .iter()
                        .chain([&l.brightness, &l.radius])
                        .all(|v| v.is_finite() && *v >= 0.0),
                "Invalid light"
            );
            for curve in l
                .color_curves
                .iter()
                .flatten()
                .chain(l.brightness_curve.iter())
                .chain(l.radius_curve.iter())
            {
                curve.validate()?;
            }
            if let Some(f) = &l.flare {
                ensure!(
                    self.textures.contains_key(&f.texture)
                        && f.blend_mode <= 2
                        && f.far_distance > f.near_distance,
                    "Invalid flare"
                );
                ensure!(
                    f.color
                        .iter()
                        .chain([
                            &f.near_size,
                            &f.far_size,
                            &f.near_distance,
                            &f.far_distance,
                            &f.fade_seconds
                        ])
                        .chain(f.constant_size.iter())
                        .all(|v| v.is_finite() && *v >= 0.0),
                    "Invalid flare values"
                );
            }
        }
        for name in self.textures.values() {
            ensure!(
                !name.is_empty() && !name.contains(['/', '\\', ':']),
                "Invalid texture filename"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn animated_curves_preserve_steps_lerp_and_period() {
        let mut c = Curve {
            period: 2.0,
            linear: true,
            values: vec![0.0, 1.0, 0.0],
        };
        c.validate().unwrap();
        assert_eq!(c.sample(0.5), 0.5);
        assert_eq!(c.sample(1.0), 1.0);
        assert_eq!(c.sample(2.5), 0.5);
        c.linear = false;
        assert_eq!(c.sample(0.5), 0.0);
        assert_eq!(c.sample(1.5), 1.0);
        c.period = 0.0;
        assert!(c.validate().is_err());
    }
}
