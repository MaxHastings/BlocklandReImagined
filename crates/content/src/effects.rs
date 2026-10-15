//! Native effect definitions. All timings are seconds and all references native IDs.
use anyhow::{Result, ensure};
use bri_console::Clamp;
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
    /// Optional angular brightness falloff/cutoff, radians from source +Y.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angles: Option<[f32; 2]>,
    #[serde(default)]
    pub depth_test: bool,
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
pub struct ParticleVariation {
    /// One interpolated authored tint per particle, retained for its whole life.
    pub color: [[f32; 4]; 2],
    /// Multiplies every authored size key.
    pub size: [f32; 2],
    /// Initial camera-facing sprite rotation, sampled once in degrees.
    #[serde(default, skip_serializing_if = "zero_pair")]
    pub initial_spin_degrees: [f32; 2],
}
fn zero_pair(value: &[f32; 2]) -> bool {
    *value == [0.; 2]
}
/// One independently authored group at an effect's contact or attachment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Burst {
    pub emitter: String,
    /// Keep this group's particles in the moving attachment frame.
    pub follow_source: bool,
    pub count: [u32; 2],
    pub radius: [f32; 2],
    pub offset: [f32; 3],
    /// Local ejection axis, relative to the contact normal or socket.
    pub direction: [f32; 3],
}
impl Default for Burst {
    fn default() -> Self {
        Self {
            emitter: String::new(),
            follow_source: false,
            count: [1; 2],
            radius: [0.; 2],
            offset: [0.; 3],
            direction: [0., 1., 0.],
        }
    }
}
impl Burst {
    pub fn valid(&self) -> bool {
        self.count[0] <= self.count[1]
            && self.count[1] <= 32768
            && self
                .radius
                .iter()
                .all(|r| r.is_finite() && (0.0..=10000.0).contains(r))
            && self.radius[0] <= self.radius[1]
            && self
                .offset
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 1_000_000.)
            && self.direction.iter().all(|x| x.is_finite())
            && (0.0001..=1_000_000.).contains(&self.direction.iter().map(|x| x * x).sum::<f32>())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleAnimation {
    pub frames: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layouts: BTreeMap<String, SpriteLayout>,
    pub rate: [f32; 2],
    #[serde(default)]
    pub looping: bool,
    #[serde(default)]
    pub random_start: bool,
    #[serde(default)]
    pub reverse: bool,
}
/// Geometry relative to the particle's authored diameter, with a top-left image pivot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpriteLayout {
    pub scale: [f32; 2],
    pub pivot: [f32; 2],
}
impl Default for SpriteLayout {
    fn default() -> Self {
        Self {
            scale: [1.; 2],
            pivot: [0.5; 2],
        }
    }
}
impl SpriteLayout {
    pub fn reach(self) -> f32 {
        self.scale
            .iter()
            .zip(self.pivot)
            .map(|(s, p)| s * p.abs().max((1. - p).abs()))
            .sum()
    }
    pub fn valid(self) -> bool {
        self.scale
            .iter()
            .all(|v| v.is_finite() && (0.0001..=16.).contains(v))
            && self
                .pivot
                .iter()
                .all(|v| v.is_finite() && (-16.0..=16.).contains(v))
    }
}
impl ParticleAnimation {
    pub fn texture_at(&self, seconds: f32, rate: f32, start: usize, reverse: bool) -> &str {
        let count = self.frames.len();
        let frame = (seconds.max(0.) * rate) as usize;
        let index = if self.looping {
            if reverse {
                (start + count - frame % count) % count
            } else {
                (start + frame % count) % count
            }
        } else if reverse {
            start.saturating_sub(frame)
        } else {
            start.saturating_add(frame).min(count - 1)
        };
        &self.frames[index]
    }
}
/// Cosmetic point contacts. Collision geometry remains owned by the world adapter.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleContact {
    pub restitution: f32,
    /// Fraction of tangential velocity removed on each contact.
    pub friction: f32,
    /// Native speed below which a contacting particle rests.
    pub rest_speed: f32,
    #[serde(default)]
    pub die_on_contact: bool,
    #[serde(default)]
    pub die_at_rest: bool,
    #[serde(default)]
    pub stop_animation_at_rest: bool,
}
impl ParticleContact {
    pub fn valid(self) -> bool {
        [self.restitution, self.friction]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self.rest_speed.is_finite()
            && (0.0..=100.0).contains(&self.rest_speed)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Particle {
    pub id: String,
    pub texture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<ParticleAnimation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variation: Option<ParticleVariation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<ParticleContact>,
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
    pub fn textures(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.texture.as_str()).chain(
            self.animation
                .iter()
                .flat_map(|a| a.frames.iter().map(String::as_str)),
        )
    }
    pub fn sample(&self, age: f32) -> ([f32; 4], f32) {
        let age = age.clamped(0.0, 1.0);
        for pair in self.keys.windows(2) {
            if age <= pair[1].time {
                let span = pair[1].time - pair[0].time;
                let t = if span > 0.0 {
                    ((age - pair[0].time) / span).clamped(0.0, 1.0)
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
                && self.emitters.len() <= 4096
                && self.textures.len() <= 4096,
            "Invalid effect library"
        );
        // Validate before runtime sampling; original additive particles can have
        // authored alpha above one (the stock glow paint uses two).
        let mut ids = BTreeSet::new();
        for p in &self.particles {
            ensure!(
                p.contact.is_none_or(ParticleContact::valid),
                "Invalid particle contacts"
            );
            if let Some(v) = &p.variation {
                ensure!(
                    v.color
                        .iter()
                        .flatten()
                        .all(|c| c.is_finite() && (0.0..=64.0).contains(c))
                        && v.size
                            .iter()
                            .all(|s| s.is_finite() && (0.0..=10000.0).contains(s))
                        && v.size[0] <= v.size[1]
                        && v.initial_spin_degrees
                            .iter()
                            .all(|v| v.is_finite() && v.abs() <= 36000.)
                        && v.initial_spin_degrees[0] <= v.initial_spin_degrees[1],
                    "Invalid particle variation"
                );
            }
            ensure!(
                ids.insert(&p.id) && p.textures().all(|t| self.textures.contains_key(t)),
                "Duplicate particle or missing texture"
            );
            if let Some(a) = &p.animation {
                ensure!(
                    (1..=256).contains(&a.frames.len())
                        && a.rate
                            .iter()
                            .all(|r| r.is_finite() && (0.0..=240.0).contains(r))
                        && a.rate[0] <= a.rate[1]
                        && a.layouts.len() <= a.frames.len()
                        && a.layouts
                            .iter()
                            .all(|(frame, layout)| a.frames.contains(frame) && layout.valid()),
                    "Invalid particle animation"
                );
            }
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
                        && f.angles.is_none_or(|a| a
                            .iter()
                            .all(|v| v.is_finite() && (0.0..=std::f32::consts::PI).contains(v))
                            && a[0] < a[1])
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

/// A map's own particle emitters (the waterfalls and mist of an old
/// Blockland map's `ParticleEmitterNode`s), saved beside its bundle as
/// `map_effects.json` and drawn where the map places them. Particles draw
/// the game's own effect textures.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapEffects {
    pub schema_version: u32,
    pub particles: Vec<Particle>,
    pub emitters: Vec<Emitter>,
    pub sources: Vec<MapEmitter>,
}
/// One placed emitter: native position and rotation (its local up is the
/// direction it emits along), and the speed it passes on to particles that
/// inherit velocity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapEmitter {
    /// The scene node it came from.
    pub node: usize,
    pub emitter: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: f32,
}
impl MapEffects {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && self.particles.len() <= 256
                && self.emitters.len() <= 256
                && self.sources.len() <= 1024,
            "Invalid map effects"
        );
        for e in &self.emitters {
            ensure!(
                !e.particles.is_empty()
                    && e.particles
                        .iter()
                        .all(|p| self.particles.iter().any(|q| &q.id == p)),
                "Map emitter {} names a particle the map lacks",
                e.id
            );
        }
        for s in &self.sources {
            ensure!(
                self.emitters.iter().any(|e| e.id == s.emitter),
                "A map emitter names an unknown emitter {}",
                s.emitter
            );
            ensure!(
                s.position
                    .iter()
                    .chain(&s.rotation)
                    .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                    && s.velocity.is_finite()
                    && s.velocity.abs() <= 10_000.0,
                "Invalid map emitter placement"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sprite_sequences_hold_loop_and_reverse_at_authored_rates() {
        let mut a = ParticleAnimation {
            layouts: BTreeMap::new(),
            frames: vec!["a".into(), "b".into(), "c".into()],
            rate: [10.; 2],
            looping: false,
            random_start: false,
            reverse: true,
        };
        assert_eq!(a.texture_at(0.15, 10., 0, false), "b");
        assert_eq!(a.texture_at(5., 10., 0, false), "c");
        assert_eq!(a.texture_at(0.15, 10., 2, true), "b");
        assert_eq!(a.texture_at(5., 10., 2, true), "a");
        a.looping = true;
        assert_eq!(a.texture_at(0.3, 10., 0, false), "a");
        assert_eq!(a.texture_at(0.1, 10., 0, true), "c");
    }
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
