//! Explosion debris: the `DebrisData` pieces an explosion throws, such as a
//! wrecked Jeep's tires and body, the tank's turret and hull, the pirate
//! cannon's barrel, and the tank shell's spark streaks.
//!
//! Client-side and cosmetic, as in v20 (`Explosion::launchDebris` runs only
//! on clients). Launch follows `Explosion::launchDebris` and motion
//! `Debris::advanceTime`: Torque's own 9.81 gravity times `gravModifier`,
//! spin about the piece's x and up axes, bounces off static geometry that
//! reflect the velocity, take `friction` off the tangent and scale by
//! `elasticity`, and a fade over the last second. Models come from the
//! vehicle pack; trail emitters follow each piece.
use crate::weapon_debris::DebrisHit;
use bri_fx_runtime::SourceTransform;
use bri_sim::presentation::{Cue, CueKind};
use bri_weapons::debris::DebrisSpec;
use glam::{Mat4, Quat, Vec3};
use std::collections::BTreeMap;

/// Most pieces alive at once; further launches are dropped.
const MAX_PIECES: usize = 256;
/// Torque's hard-coded debris gravity (`Debris::computeNewState`).
const GRAVITY: f32 = 9.81;
/// `Debris` constructor: `mRadius`, the bounce ray's lead.
const RADIUS: f32 = 0.2;
/// Longest simulation step; a slow frame is split into these.
const MAX_STEP: f32 = 1.0 / 60.0;

struct Piece {
    id: u64,
    spec: usize,
    position: Vec3,
    last: Vec3,
    velocity: Vec3,
    rotation: Quat,
    /// Degrees per second about the piece's x and up axes.
    spin: Vec3,
    elasticity: f32,
    friction: f32,
    life: f32,
    bounces: i32,
    settled: bool,
}

/// One piece's trail emitter this frame.
pub struct Trail {
    pub key: (u64, u8),
    pub emitter: String,
    pub transform: SourceTransform,
}

pub struct ExplosionDebris {
    specs: Vec<DebrisSpec>,
    by_explosion: BTreeMap<String, usize>,
    pieces: Vec<Piece>,
    cursor: u64,
    next_id: u64,
    pub dropped: u64,
}

impl ExplosionDebris {
    /// Explosions whose debris model the vehicle pack lacks keep their
    /// trails and lose only the model.
    pub fn new(pack: &bri_weapons::Pack) -> Self {
        let mut specs = Vec::new();
        let mut by_explosion = BTreeMap::new();
        for (explosion, spec) in bri_weapons::debris::explosion_debris(pack) {
            by_explosion.insert(explosion, specs.len());
            specs.push(spec);
        }
        Self {
            specs,
            by_explosion,
            pieces: Vec::new(),
            cursor: 0,
            next_id: 1,
            dropped: 0,
        }
    }
    pub fn reset(&mut self, checkpoint_cursor: u64) {
        self.pieces.clear();
        self.cursor = checkpoint_cursor;
    }
    pub fn live_count(&self) -> usize {
        self.pieces.len()
    }
    /// Launch an explosion's debris from a `WeaponEffect` cue naming it.
    pub fn cue(&mut self, cue: &Cue) {
        if cue.id <= self.cursor {
            return;
        }
        self.cursor = cue.id;
        let CueKind::WeaponEffect {
            definition,
            direction,
            scale,
            ..
        } = &cue.kind
        else {
            return;
        };
        let Some(&index) = self.by_explosion.get(&definition.to_ascii_lowercase()) else {
            return;
        };
        // The explosion's normal: the surface it hit, else straight up.
        let axis = direction
            .map(Vec3::from)
            .filter(|v| v.is_finite() && v.length_squared() > 0.5)
            .map_or(Vec3::Y, Vec3::normalize);
        self.launch(index, Vec3::from(cue.position), axis, *scale, cue.id);
    }
    fn launch(&mut self, index: usize, at: Vec3, axis: Vec3, scale: f32, seed: u64) {
        let spec = &self.specs[index];
        let mut rng = Rng(seed ^ 0xde88_15e5_0000_0000);
        let count = spec.count as i64
            + rng.range_i(-(spec.count_variance as i64), spec.count_variance as i64);
        // `Explosion::launchDebris` starts every piece half a unit up.
        let origin = at + Vec3::Y * 0.5 * scale.clamp(0.01, 100.0);
        // A perpendicular to tip the launch direction by theta.
        let side = if axis.y.abs() < 0.999 {
            axis.cross(Vec3::Y)
        } else {
            axis.cross(Vec3::Z)
        }
        .normalize();
        for _ in 0..count.max(0) {
            if self.pieces.len() >= MAX_PIECES {
                self.dropped += 1;
                continue;
            }
            // `MathUtils::randomDir`: theta from the axis, then phi around it.
            let theta = rng.range(spec.theta[0], spec.theta[1]).to_radians();
            let phi = rng.range(spec.phi[0], spec.phi[1]).to_radians();
            let direction =
                Quat::from_axis_angle(axis, phi) * (Quat::from_axis_angle(side, theta) * axis);
            let mut speed = spec.launch_speed + spec.launch_variance * rng.range(-1.0, 1.0);
            // `Debris::onAdd`: a datablock velocity replaces the launch speed.
            if spec.speed != 0.0 {
                speed = spec.speed + rng.range(-spec.speed_variance, spec.speed_variance);
            }
            let bounces = spec.bounces as i32
                + rng.range_i(-(spec.bounce_variance as i64), spec.bounce_variance as i64) as i32;
            let life = spec.lifetime
                + (spec.lifetime_variance * 2.0 * rng.range(-1.0, 1.0) - spec.lifetime_variance);
            let x = rng.range(spec.spin[0], spec.spin[1]);
            let z = rng.range(spec.spin[0], spec.spin[1]) * rng.range(0.1, 0.5);
            let (mut spin, mut elasticity, mut friction) =
                (Vec3::new(x, z, 0.0), spec.elasticity, spec.friction);
            if let Some(base) = spec.radius_mass {
                let factor = base / RADIUS.max(base);
                spin *= factor;
                elasticity *= factor;
                friction *= factor;
            }
            self.pieces.push(Piece {
                id: self.next_id,
                spec: index,
                position: origin,
                last: origin,
                velocity: direction * speed,
                rotation: Quat::IDENTITY,
                spin,
                elasticity,
                friction,
                life,
                bounces,
                settled: false,
            });
            self.next_id += 1;
        }
    }
    /// Move every piece; `sweep` finds static geometry between two points.
    pub fn advance(&mut self, dt: f32, mut sweep: impl FnMut(Vec3, Vec3) -> Option<DebrisHit>) {
        let mut left = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        while left > 0.0 {
            let step = left.min(MAX_STEP);
            left -= step;
            for p in &mut self.pieces {
                p.life -= step;
                p.last = p.position;
                if p.life <= 0.0 || p.settled {
                    continue;
                }
                let spec = &self.specs[p.spec];
                // `Debris::rotate`: x then up, in the piece's own frame.
                let turn = p.spin * step;
                p.rotation = (p.rotation
                    * Quat::from_rotation_x(turn.x.to_radians())
                    * Quat::from_rotation_y(turn.y.to_radians()))
                .normalize();
                // `computeNewState`: gravity unless at terminal velocity.
                if spec.terminal_velocity <= 0.0001 || p.velocity.length() <= spec.terminal_velocity
                {
                    p.velocity.y -= GRAVITY * spec.gravity * step;
                } else {
                    p.velocity = p.velocity.normalize() * spec.terminal_velocity;
                }
                let next = p.position + p.velocity * step;
                let travel = next - p.position;
                let length = travel.length();
                if length <= 0.0 {
                    continue;
                }
                let dir = travel / length;
                let extent = next + dir * RADIUS;
                let hit = sweep(p.position, extent).filter(|h| {
                    h.fraction.is_finite() && h.normal.is_finite() && h.normal.length() > 0.5
                });
                let Some(hit) = hit else {
                    p.position = next;
                    continue;
                };
                // `Debris::bounce`.
                let n = hit.normal.normalize();
                let reflection = p.velocity - n * (p.velocity.dot(n) * 2.0);
                let tangent = reflection - n * reflection.dot(n);
                p.velocity = (reflection - tangent * p.friction) * p.elasticity;
                // Torque moves the unit direction by the ray fraction times
                // the share of the ray that was motion, then one step on.
                let move_percent = length / (length + RADIUS);
                p.position += dir * hit.fraction.clamp(0.0, 1.0) * move_percent + p.velocity * step;
                p.spin *= p.elasticity;
                p.bounces -= 1;
                if p.bounces <= 0 {
                    if spec.snap_on_max_bounce {
                        // Lie flat along its heading, a little above the ground.
                        let mut forward = p.rotation * Vec3::NEG_Z;
                        forward.y = 0.0;
                        p.rotation = if forward.length_squared() > 1e-6 {
                            Quat::from_rotation_arc(Vec3::NEG_Z, forward.normalize())
                        } else {
                            Quat::IDENTITY
                        };
                        p.position.y += 0.1;
                    }
                    if spec.static_on_max_bounce {
                        p.settled = true;
                        p.velocity = Vec3::ZERO;
                    }
                }
            }
            self.pieces.retain(|p| p.life > 0.0);
        }
    }
    /// Each live piece's model, transform and tint (faded over its last
    /// second when the datablock fades).
    pub fn models(&self) -> impl Iterator<Item = (&str, Mat4, [f32; 4])> + '_ {
        self.pieces.iter().filter_map(|p| {
            let spec = &self.specs[p.spec];
            if spec.model.is_empty() {
                return None;
            }
            let alpha = if spec.fade {
                p.life.clamp(0.0, 1.0)
            } else {
                1.0
            };
            Some((
                spec.model.as_str(),
                Mat4::from_rotation_translation(p.rotation, p.position),
                [1.0, 1.0, 1.0, alpha],
            ))
        })
    }
    /// Trail emitters, emitting away from each piece's motion
    /// (`Debris::updateEmitters`).
    pub fn trails(&self) -> Vec<Trail> {
        let mut out = Vec::new();
        for p in &self.pieces {
            let spec = &self.specs[p.spec];
            let axis = (-p.velocity).try_normalize().unwrap_or(Vec3::Y);
            for (i, emitter) in spec.emitters.iter().enumerate().take(2) {
                out.push(Trail {
                    key: (p.id, i as u8),
                    emitter: format!("v20/emitter/{}", emitter.to_ascii_lowercase()),
                    transform: SourceTransform {
                        position: p.position,
                        rotation: Quat::from_rotation_arc(Vec3::Y, axis),
                        velocity: p.velocity,
                    },
                });
            }
        }
        out
    }
}

/// Deterministic per-explosion randomness (splitmix64).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f32 {
        ((self.next() >> 40) as f32) / (1u32 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
    fn range_i(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec(name: &str) -> DebrisSpec {
        DebrisSpec {
            name: name.into(),
            model: "Add-Ons/Vehicle_Jeep/jeepTire.dts".into(),
            emitters: vec!["JeepTireDebrisTrailEmitter".into()],
            count: 4,
            count_variance: 0,
            theta: [40.0, 85.0],
            phi: [0.0, 360.0],
            launch_speed: 14.0,
            launch_variance: 3.0,
            speed: 0.0,
            speed_variance: 0.0,
            lifetime: 2.0,
            lifetime_variance: 0.0,
            spin: [-400.0, 200.0],
            elasticity: 0.5,
            friction: 0.2,
            bounces: 3,
            bounce_variance: 0,
            static_on_max_bounce: true,
            snap_on_max_bounce: false,
            fade: true,
            gravity: 2.0,
            terminal_velocity: 0.0,
            radius_mass: None,
        }
    }
    fn debris() -> ExplosionDebris {
        ExplosionDebris {
            specs: vec![spec("jeepTireDebris")],
            by_explosion: [("jeepexplosion".to_string(), 0)].into(),
            pieces: Vec::new(),
            cursor: 0,
            next_id: 1,
            dropped: 0,
        }
    }
    fn cue(id: u64, definition: &str, direction: Option<[f32; 3]>) -> Cue {
        Cue {
            id,
            tick: id,
            position: [0.0, 1.0, 0.0],
            kind: CueKind::WeaponEffect {
                source: bri_weapons::TargetId::Map(0),
                definition: definition.into(),
                node: String::new(),
                seconds: 0.0,
                image: None,
                hand: None,
                direction,
                scale: 1.0,
            },
        }
    }
    fn floor(from: Vec3, to: Vec3) -> Option<DebrisHit> {
        (from.y > 0.0 && to.y <= 0.0).then(|| DebrisHit {
            fraction: from.y / (from.y - to.y),
            normal: Vec3::Y,
        })
    }

    #[test]
    fn a_jeep_explosion_throws_four_tires_up_and_out_that_settle_and_fade() {
        let mut d = debris();
        d.cue(&cue(5, "JeepExplosion", None));
        d.cue(&cue(5, "JeepExplosion", None));
        d.cue(&cue(6, "rocketExplosion", None));
        assert_eq!(
            d.live_count(),
            4,
            "once per cue, only for debris explosions"
        );
        for p in &d.pieces {
            // 40 to 85 degrees off straight up, at 14 +/- 3.
            let angle = p.velocity.normalize().dot(Vec3::Y).acos().to_degrees();
            assert!((39.9..=85.1).contains(&angle), "{angle}");
            assert!((10.9..=17.1).contains(&p.velocity.length()));
            assert_eq!(p.position, Vec3::new(0.0, 1.5, 0.0));
        }
        assert_eq!(d.trails().len(), 4);
        assert_eq!(
            d.trails()[0].emitter,
            "v20/emitter/jeeptiredebristrailemitter"
        );
        for _ in 0..90 {
            d.advance(1.0 / 60.0, floor);
        }
        // Bouncing on the floor, never through it.
        assert!(
            d.pieces
                .iter()
                .all(|p| p.bounces < 3 && p.position.y >= -0.01)
        );
        // Half a second left: half faded.
        assert!(d.models().all(|(_, _, tint)| (tint[3] - 0.5).abs() < 0.02));
        d.advance(0.25, floor);
        d.advance(0.25, floor);
        d.advance(0.01, floor);
        assert_eq!(d.live_count(), 0);
    }

    #[test]
    fn debris_launches_around_the_surface_it_hit() {
        let mut d = debris();
        d.cue(&cue(1, "jeepexplosion", Some([1.0, 0.0, 0.0])));
        for p in &d.pieces {
            let angle = p.velocity.normalize().dot(Vec3::X).acos().to_degrees();
            assert!((39.9..=85.1).contains(&angle), "{angle}");
        }
    }
}
