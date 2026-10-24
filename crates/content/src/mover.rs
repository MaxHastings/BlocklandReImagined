//! Map movers: static models that follow authored keyframes (a swinging
//! door, a sliding gate, a turning valve handle), and climb volumes
//! (ladders). The engine owns the mechanism: the keyed motion, its easing,
//! the collision that moves with it and the state every player sees. A
//! package owns the policy: who may start one, when, and how fast
//! (`bri_package_runtime` ops `mover_goto` and `mover_stop`).
//!
//! A mover is a [`crate::scene::Kind::StaticModel`] node whose `mover`
//! property holds its [`MoverDef`] as JSON. The node's transform is the
//! mover at rest at key 0. Every key is an offset from that: a translation
//! in the map's world axes and a rotation vector (axis times angle, in
//! radians) about the node's origin applied in the world frame, so a
//! door's hinge is the model's origin and a quarter turn is `[0, tau/4,
//! 0]`. A rotation vector, unlike Euler angles, also holds more than one
//! turn, which a valve wheel needs.
use anyhow::{Result, ensure};
use bri_console::Clamp;
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most movers a scene may hold.
pub const MAX_MOVERS: usize = 512;
/// Most keys a mover may have.
pub const MAX_KEYS: usize = 16;
/// Ticks of the simulation a second (`bri_physics::FIXED_DT`).
pub const TICKS_PER_SECOND: f32 = 120.0;
/// Shortest travel a mover is given, seconds: Unreal's `PhysRate` floor.
pub const MIN_SECONDS: f32 = 0.005;
/// Largest progress per tick produced by the supported travel-duration floor.
/// A motion shorter than one simulation tick legitimately advances over 1.0.
pub const MAX_RATE: f32 = 1.0 / (MIN_SECONDS * TICKS_PER_SECOND);

/// One keyframe: an offset from the mover's placement.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoverKey {
    #[serde(default)]
    pub translation: [f32; 3],
    /// Axis times angle, radians.
    #[serde(default)]
    pub rotation: [f32; 3],
}

/// How a motion's progress turns into travel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Glide {
    /// Constant speed.
    Linear,
    /// Slow in and out: `3t^2 - 2t^3`.
    #[default]
    Smooth,
}

/// What a mover does when a body is in the way of its next step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encroach {
    /// It stops where it is and reports it is blocked, until told to move
    /// again.
    #[default]
    Stop,
    /// It moves through whatever is there.
    Ignore,
}

/// A continuous turn that needs no state: a fan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spin {
    /// Axis in the map's world frame, through the node's origin.
    pub axis: [f32; 3],
    /// Radians a second.
    pub rate: f32,
}

/// Loudest a mover sound may be, times the sound's own volume.
pub const MAX_SOUND_VOLUME: f32 = 2.0;

/// A mover's sounds, each a sound profile name. A motion toward key 0 is
/// closing and one away from it is opening, so a door, a gate and a
/// valve wheel all read the same. Every field is optional: a mover
/// without sounds is silent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoverSounds {
    /// Once, as it starts moving away from key 0.
    #[serde(default)]
    pub opening: Option<String>,
    /// Once, as it comes to rest away from key 0.
    #[serde(default)]
    pub opened: Option<String>,
    /// Once, as it starts moving toward key 0.
    #[serde(default)]
    pub closing: Option<String>,
    /// Once, as it comes to rest at key 0.
    #[serde(default)]
    pub closed: Option<String>,
    /// Looped for as long as it moves.
    #[serde(default)]
    pub moving: Option<String>,
    /// Times each sound's own volume, 0 to [`MAX_SOUND_VOLUME`].
    #[serde(default = "full_volume")]
    pub volume: f32,
}
fn full_volume() -> f32 {
    1.0
}

impl MoverSounds {
    pub fn validate(&self, mover: &str) -> Result<()> {
        ensure!(
            self.volume.is_finite() && (0.0..=MAX_SOUND_VOLUME).contains(&self.volume),
            "Mover {mover}: sound volume must be 0 to {MAX_SOUND_VOLUME}"
        );
        ensure!(
            [
                &self.opening,
                &self.opened,
                &self.closing,
                &self.closed,
                &self.moving
            ]
            .into_iter()
            .flatten()
            .all(|name| !name.is_empty() && name.len() <= 128),
            "Mover {mover}: a sound name is empty or over 128 bytes"
        );
        Ok(())
    }
    /// The one-shot for a motion that begins toward `to`.
    pub fn start(&self, to: u8) -> Option<&str> {
        let name = if to == 0 {
            &self.closing
        } else {
            &self.opening
        };
        name.as_deref()
    }
    /// The one-shot for a mover that comes to rest at `key`.
    pub fn rest(&self, key: u8) -> Option<&str> {
        let name = if key == 0 { &self.closed } else { &self.opened };
        name.as_deref()
    }
}

/// What a scene node's `mover` property says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoverDef {
    /// Stable authored identity.
    pub id: String,
    /// Key 0 is the placement itself and is added if the list is empty.
    #[serde(default)]
    pub keys: Vec<MoverKey>,
    #[serde(default)]
    pub glide: Glide,
    #[serde(default)]
    pub encroach: Encroach,
    #[serde(default)]
    pub spin: Option<Spin>,
    /// Whether the model collides. A decoration (a fan, a hidden sheet)
    /// need not.
    #[serde(default = "yes")]
    pub collide: bool,
    /// The package's own facts about this mover (its sounds, its trigger
    /// style, which side it opens from). The engine keeps them as written.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    /// What it sounds like when it moves.
    #[serde(default)]
    pub sounds: Option<MoverSounds>,
}
fn yes() -> bool {
    true
}

impl MoverDef {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty() && self.id.len() <= 64,
            "A mover needs an id of up to 64 bytes"
        );
        ensure!(
            !self.keys.is_empty() && self.keys.len() <= MAX_KEYS,
            "Mover {} needs 1 to {MAX_KEYS} keys",
            self.id
        );
        ensure!(
            self.keys
                .iter()
                .flat_map(|k| k.translation.iter().chain(&k.rotation))
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.0),
            "Mover {}: a key is not finite",
            self.id
        );
        ensure!(
            self.keys[0] == MoverKey::default(),
            "Mover {}: key 0 is the placement and must be zero",
            self.id
        );
        if let Some(spin) = &self.spin {
            ensure!(
                spin.axis.iter().chain([&spin.rate]).all(|v| v.is_finite())
                    && Vec3::from(spin.axis).length() > 1e-6,
                "Mover {}: spin needs a finite axis and rate",
                self.id
            );
        }
        if let Some(sounds) = &self.sounds {
            sounds.validate(&self.id)?;
        }
        ensure!(
            self.params.len() <= 64
                && self
                    .params
                    .iter()
                    .all(|(k, v)| k.len() <= 64 && v.len() <= 512),
            "Mover {}: too many or too long params",
            self.id
        );
        Ok(())
    }
    /// The pose of key `key` (clamped) as an offset transform.
    fn key(&self, key: usize) -> &MoverKey {
        &self.keys[key.min(self.keys.len() - 1)]
    }
    /// The offset from the placement at `at`: a motion's endpoints, eased.
    /// Translation and rotation vector blend linearly between the two keys
    /// (exact for rotations about one axis, which is what a door is).
    pub fn offset(&self, from: usize, to: usize, progress: f32) -> Offset {
        let (a, b) = (self.key(from), self.key(to));
        let t = self.glide.apply(progress);
        Offset {
            translation: Vec3::from(a.translation).lerp(Vec3::from(b.translation), t),
            rotation: Vec3::from(a.rotation).lerp(Vec3::from(b.rotation), t),
        }
    }
}

/// A mover's pose relative to its placement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Offset {
    pub translation: Vec3,
    /// Axis times angle, radians, about the node's origin.
    pub rotation: Vec3,
}
impl Offset {
    /// The mover's world transform: `placement` turned about its own
    /// origin by the rotation (in the world frame), then moved.
    pub fn apply(&self, placement: Mat4) -> Mat4 {
        let turn = Quat::from_scaled_axis(self.rotation);
        let (scale, rotation, origin) = placement.to_scale_rotation_translation();
        Mat4::from_scale_rotation_translation(scale, turn * rotation, origin + self.translation)
    }
}

impl Glide {
    /// Eased progress for raw progress `t` in 0 to 1.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamped(0.0, 1.0);
        match self {
            Glide::Linear => t,
            Glide::Smooth => t * t * (3.0 - 2.0 * t),
        }
    }
}

/// Where a mover is in its motion, and what every peer is told.
///
/// A motion runs from key `from` to key `to`; `alpha` is raw progress
/// (0 to 1) at tick `since`, advancing `rate` a tick while `moving`. Rest
/// at a key is `moving == false` with `alpha == 1` at `to` (or `alpha == 0`
/// at `from`). Every peer computes the same pose from this and the tick.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoverState {
    pub from: u8,
    pub to: u8,
    pub alpha: f32,
    pub since: u64,
    /// Progress a tick.
    pub rate: f32,
    pub moving: bool,
    /// Held by something in its way (see [`Encroach::Stop`]).
    pub blocked: bool,
}
impl Default for MoverState {
    fn default() -> Self {
        Self::at_rest(0)
    }
}
impl MoverState {
    pub fn at_rest(key: u8) -> Self {
        Self {
            from: key,
            to: key,
            alpha: 1.0,
            since: 0,
            rate: 0.0,
            moving: false,
            blocked: false,
        }
    }
    /// Raw progress at `tick`.
    pub fn alpha_at(&self, tick: u64) -> f32 {
        self.alpha_at_f(tick as f64)
    }
    /// Raw progress at a tick between ticks (a client draws between
    /// them).
    pub fn alpha_at_f(&self, tick: f64) -> f32 {
        if !self.moving {
            return self.alpha;
        }
        let elapsed = (tick - self.since as f64).max(0.0);
        (f64::from(self.alpha) + elapsed * f64::from(self.rate)).clamped(0.0, 1.0) as f32
    }
    /// Whether it is still travelling at `tick`.
    pub fn is_moving_at(&self, tick: u64) -> bool {
        self.moving && self.alpha_at(tick) < 1.0
    }
    /// The key it rests at, or None while it is between keys.
    pub fn resting_key(&self, tick: u64) -> Option<u8> {
        if self.is_moving_at(tick) {
            return None;
        }
        let alpha = self.alpha_at(tick);
        if alpha >= 1.0 {
            Some(self.to)
        } else if alpha <= 0.0 {
            Some(self.from)
        } else {
            None
        }
    }
    /// Begin travelling to `key` over `seconds` at `tick`, as Unreal's
    /// `Mover::InterpolateTo` does: turning back toward the key it left
    /// carries the distance already covered (`PhysAlpha = 1 - PhysAlpha`),
    /// so a door shut halfway closes in the time it had opened. A mover
    /// already heading to some other key, or between two keys it is not
    /// heading to, cannot be redirected: false.
    pub fn go_to(&mut self, tick: u64, key: u8, seconds: f32) -> bool {
        let alpha = self.alpha_at(tick);
        let moving = self.is_moving_at(tick);
        let heading = if moving { Some(self.to) } else { None };
        let next = if key == self.from && self.to != self.from {
            // Back the way it came: swap ends and keep the distance.
            Some((self.to, self.from, 1.0 - alpha))
        } else if !moving && alpha >= 1.0 && key != self.to {
            Some((self.to, key, 0.0))
        } else if !moving && alpha <= 0.0 && key != self.from {
            Some((self.from, key, 0.0))
        } else if heading == Some(key) || (!moving && self.resting_key(tick) == Some(key)) {
            // Already there, or on its way.
            return true;
        } else {
            None
        };
        let Some((from, to, alpha)) = next else {
            return false;
        };
        *self = Self {
            from,
            to,
            alpha,
            since: tick,
            rate: 1.0 / (seconds.max(MIN_SECONDS) * TICKS_PER_SECOND),
            moving: true,
            blocked: false,
        };
        true
    }
    /// Hold where it is.
    pub fn halt(&mut self, tick: u64, blocked: bool) {
        self.alpha = self.alpha_at(tick);
        self.since = tick;
        self.moving = false;
        self.blocked = blocked;
    }
    /// Settle a motion that has run its course.
    pub fn settle(&mut self, tick: u64) {
        if self.moving && self.alpha_at(tick) >= 1.0 {
            self.alpha = 1.0;
            self.since = tick;
            self.moving = false;
        }
    }
    /// The mover's offset from its placement at `tick`.
    pub fn offset(&self, def: &MoverDef, tick: u64) -> Offset {
        self.offset_f(def, tick as f64)
    }
    /// [`Self::offset`] at a tick between ticks.
    pub fn offset_f(&self, def: &MoverDef, tick: f64) -> Offset {
        let mut offset = def.offset(self.from as usize, self.to as usize, self.alpha_at_f(tick));
        if let Some(spin) = &def.spin {
            let turn = Vec3::from(spin.axis).normalize()
                * spin.rate
                * (tick / f64::from(TICKS_PER_SECOND)) as f32;
            // A spin turns the whole mover about its own origin, after the
            // keys.
            offset.rotation += turn;
        }
        offset
    }
}

/// A climb volume read from a scene node.
#[derive(Clone, Debug, PartialEq)]
pub struct Climb {
    pub node: usize,
    pub id: String,
    pub center: Vec3,
    /// Half extents in the node's own axes.
    pub half: Vec3,
    /// Orientation of the sensor box about +Y.
    pub yaw: f32,
    /// Facing of the climber, independent of the sensor box.
    pub facing_yaw: f32,
    /// Units a second along `up` at full push.
    pub speed: f32,
    /// Whether the strafe keys move sideways along the rungs.
    pub strafe: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door() -> MoverDef {
        MoverDef {
            id: "door".into(),
            keys: vec![
                MoverKey::default(),
                MoverKey {
                    translation: [0.0; 3],
                    rotation: [0.0, -std::f32::consts::FRAC_PI_2, 0.0],
                },
                MoverKey {
                    translation: [0.0; 3],
                    rotation: [0.0, std::f32::consts::FRAC_PI_2, 0.0],
                },
            ],
            glide: Glide::Smooth,
            encroach: Encroach::Stop,
            spin: None,
            collide: true,
            params: BTreeMap::new(),
            sounds: None,
        }
    }

    #[test]
    fn smooth_glide_is_slow_at_both_ends_and_half_way_at_half() {
        assert_eq!(Glide::Smooth.apply(0.0), 0.0);
        assert_eq!(Glide::Smooth.apply(1.0), 1.0);
        assert!((Glide::Smooth.apply(0.5) - 0.5).abs() < 1e-6);
        assert!(Glide::Smooth.apply(0.1) < 0.1);
        assert!(Glide::Smooth.apply(0.9) > 0.9);
    }

    #[test]
    fn a_door_opens_to_key_one_and_arrives_at_rest() {
        let def = door();
        let mut state = MoverState::default();
        assert_eq!(state.resting_key(0), Some(0));
        assert!(state.go_to(10, 1, 1.5));
        assert!(state.is_moving_at(10));
        assert_eq!(state.resting_key(100), None);
        // Past 1.5 s at 120 ticks a second it has arrived.
        let end = 10 + 180;
        assert_eq!(state.alpha_at(end), 1.0);
        assert_eq!(state.resting_key(end), Some(1));
        let offset = state.offset(&def, end);
        assert!((offset.rotation.y + std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        // Half way through, the swing is half open (smoothstep at 0.5).
        let half = state.offset(&def, 10 + 90);
        assert!((half.rotation.y + std::f32::consts::FRAC_PI_4).abs() < 1e-4);
    }

    #[test]
    fn closing_a_door_shut_halfway_takes_the_time_it_had_opened() {
        let mut state = MoverState::default();
        assert!(state.go_to(0, 1, 1.0));
        // A quarter of the way open (30 ticks of 120), turned back.
        assert!(state.go_to(30, 0, 1.0));
        assert_eq!((state.from, state.to), (1, 0));
        assert!((state.alpha - 0.75).abs() < 1e-6);
        // It reaches the frame after the 30 ticks it had travelled.
        assert!(state.is_moving_at(30 + 29));
        assert_eq!(state.resting_key(30 + 30), Some(0));
        assert_eq!(state.offset(&door(), 60), Offset::default());
    }

    #[test]
    fn a_door_held_by_something_in_the_way_waits_then_goes_back() {
        let mut state = MoverState::default();
        assert!(state.go_to(0, 1, 1.0));
        state.halt(60, true);
        assert!(state.blocked && !state.moving);
        assert_eq!(state.resting_key(500), None);
        assert!((state.alpha_at(500) - 0.5).abs() < 1e-6);
        // The way back carries what it had covered.
        assert!(state.go_to(500, 0, 1.0));
        assert!(!state.blocked);
        assert!((state.alpha - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_two_way_door_opens_either_side_from_shut_but_not_across() {
        let mut state = MoverState::default();
        assert!(state.go_to(0, 2, 1.0));
        assert!(!state.go_to(10, 1, 1.0), "cannot swing straight across");
        state.settle(500);
        assert_eq!(state.resting_key(500), Some(2));
        assert!(state.go_to(500, 0, 1.0));
        state.settle(700);
        assert_eq!(state.resting_key(700), Some(0));
        assert!(state.go_to(700, 1, 1.0));
    }

    #[test]
    fn a_spin_turns_by_the_clock_without_state() {
        let mut def = door();
        def.spin = Some(Spin {
            axis: [0.0, 1.0, 0.0],
            rate: std::f32::consts::PI,
        });
        let state = MoverState::default();
        let offset = state.offset(&def, 120);
        assert!((offset.rotation.y - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn offsets_move_a_placement_about_its_own_origin() {
        let placement = Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0));
        let offset = Offset {
            translation: Vec3::new(0.0, 1.0, 0.0),
            rotation: Vec3::new(0.0, std::f32::consts::FRAC_PI_2, 0.0),
        };
        let moved = offset.apply(placement);
        // The hinge stays at the origin (plus the translation) and local
        // +X turns a quarter about +Y to -Z.
        assert!((moved.w_axis.truncate() - Vec3::new(5.0, 1.0, 0.0)).length() < 1e-6);
        assert!((moved.transform_vector3(Vec3::X) - Vec3::NEG_Z).length() < 1e-5);
    }

    #[test]
    fn a_definition_must_start_at_the_placement_and_be_finite() {
        let mut def = door();
        assert!(def.validate().is_ok());
        def.keys[0].translation = [1.0, 0.0, 0.0];
        assert!(def.validate().is_err());
        def.keys[0] = MoverKey::default();
        def.keys[1].rotation[0] = f32::NAN;
        assert!(def.validate().is_err());
    }

    #[test]
    fn sounds_read_a_motion_toward_key_zero_as_closing() {
        let sounds: MoverSounds = serde_json::from_str(
            r#"{"opening": "door.open", "closed": "door.shut", "moving": "door.creak"}"#,
        )
        .unwrap();
        assert_eq!(sounds.volume, 1.0);
        assert_eq!(sounds.start(1), Some("door.open"));
        assert_eq!(sounds.start(0), None);
        assert_eq!(sounds.rest(0), Some("door.shut"));
        assert_eq!(sounds.rest(2), None);
        let mut def = door();
        def.sounds = Some(sounds);
        assert!(def.validate().is_ok());
        def.sounds.as_mut().unwrap().volume = MAX_SOUND_VOLUME + 0.5;
        assert!(def.validate().is_err());
    }
}
