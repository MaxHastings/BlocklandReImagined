//! Native v20 vehicle data. No legacy reader or script execution is linked here.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
/// 5 adds the chase camera and seated look limits. 6 types the steering and
/// wheeled-flight fields (5 kept them only in `authored`), folds the
/// `FlyingWheeled` family into `Wheeled` and adds animation threads. 7 gives
/// each wheel its whole Torque tyre and the spring's anti-sway, for Torque's
/// own wheel forces.
pub const SCHEMA_VERSION: u32 = 7;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pack {
    pub schema_version: u32,
    pub definitions: Vec<Definition>,
    pub assets: Vec<Asset>,
    pub evidence: Vec<Evidence>,
    pub unresolved: Vec<String>,
    /// Keys are native vehicle ID + :: + authored sequence alias; values are native clips assets.
    pub animation_aliases: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {
    pub virtual_path: String,
    pub path: String,
    pub sha256: String,
    pub kind: String,
    pub source_sha256: String,
    /// Content-root-relative directory of the package holding `path`, set
    /// when packs from several packages are merged; None is this pack's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub sha256: String,
    pub line: usize,
    pub subject: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Family {
    /// Blockland's `WheeledVehicle`; it flies when `wheeled_flight` is set.
    Wheeled,
    /// Torque's `FlyingVehicle`: hovers, and `flight` holds its fields.
    Flying,
    Horse,
    Ball,
    Cannon,
    Rowboat,
    Turret,
    Skis,
    Tumble,
}
impl Family {
    /// Whether a Vehicle Spawn brick may hold it. Skis and a tumbling body
    /// are states of a player. v20 lists the Tank Turret on its own too.
    pub fn spawnable(self) -> bool {
        !matches!(self, Self::Skis | Self::Tumble)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transform {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.; 3],
            rotation: [0., 0., 0., 1.],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Seat {
    /// A clip authored in this seat's model-origin frame. If the rider owns
    /// it, preserve that clip's root; otherwise align its ordinary sit hip
    /// to this seat. None keeps legacy Blockhead-relative seat fitting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rider_pose: Option<String>,
    /// Exact mounted-image contracts whose resolved ready role the seat owns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rider_ready_policy: Option<SeatReadyPolicy>,
    /// Fallback hip point in the unscaled native seat-node frame (+Y up,
    /// forward -Z). Only used when the selected rider_pose is absent on a body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rider_fallback_hip: Option<[f32; 3]>,
    /// Authored first-person eye offset in the seat frame. No FOV or camera
    /// mode restriction. Host interaction rays use this same point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eye: Option<[f32; 3]>,
    pub node: String,
    pub transform: Transform,
    pub pose: String,
    pub controls: bool,
    pub weapon: bool,
    /// Where the rider's hands hold on, in the vehicle's frame as
    /// `transform` is: a gun's handles, a steering wheel. A gunner's turn
    /// and tip with the gun's aim. Every body reaches its arms there.
    #[serde(default, skip_serializing_if = "SeatHands::is_empty")]
    pub hands: SeatHands,
    /// A sound profile (a weapons pack's `sounds`, or the vehicle pack's)
    /// played where a rider boards this seat, in place of the base game's
    /// mount sound, and where they leave it. Empty plays the base game's
    /// mount sound on boarding and nothing on leaving.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub enter_sound: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub exit_sound: String,
}
/// Mirrors weapons::IMAGE_SLOTS; cross-crate regression guards this domain.
pub const SEAT_READY_IMAGE_SLOTS: usize = 4;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatReadyPose {
    Right,
    Left,
    Both,
}
impl SeatReadyPose {
    fn index(self) -> usize {
        match self {
            Self::Right => 0,
            Self::Left => 1,
            Self::Both => 2,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatHeldImage {
    pub slot: u8,
    pub image: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatReadyContract {
    pub images: Vec<SeatHeldImage>,
    pub pose: SeatReadyPose,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatReadyPolicy {
    pub contracts: Vec<SeatReadyContract>,
}
/// Eligible roles for one exact image set. The body must still resolve its role
/// and verify the active owned seat clip. No client or rendering dependency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeatReadyMatches {
    roles: [bool; 3],
}
impl SeatReadyMatches {
    pub fn covers(self, pose: SeatReadyPose) -> bool {
        self.roles[pose.index()]
    }
    /// Resolve the body's existing fallback provenance without changing it.
    /// Both clients and authoritative pose consumers can apply this decision.
    pub fn covers_resolved(
        self,
        requested: bri_content::body::Role,
        roles: &bri_content::body::RoleClips,
    ) -> bool {
        use bri_content::body::Role;
        if roles.clip(requested).is_none() {
            return false;
        }
        let resolved = roles
            .missing
            .iter()
            .find(|(missing, _)| *missing == requested)
            .map_or(Some(requested), |(_, fallback)| *fallback);
        match resolved {
            Some(Role::AimRight) => self.covers(SeatReadyPose::Right),
            Some(Role::AimLeft) => self.covers(SeatReadyPose::Left),
            Some(Role::AimBoth) => self.covers(SeatReadyPose::Both),
            _ => false,
        }
    }
}
fn ready_image_valid(slot: u8, image: &str) -> bool {
    usize::from(slot) < SEAT_READY_IMAGE_SLOTS
        && !image.is_empty()
        && image.len() <= 256
        && image.trim() == image
        && !image.chars().any(char::is_control)
}
impl SeatReadyPolicy {
    pub fn valid(&self) -> bool {
        !self.contracts.is_empty()
            && self.contracts.len() <= 16
            && self.contracts.iter().enumerate().all(|(i, c)| {
                !c.images.is_empty()
                    && c.images.len() <= SEAT_READY_IMAGE_SLOTS
                    && c.images.iter().enumerate().all(|(n, b)| {
                        ready_image_valid(b.slot, &b.image)
                            && c.images[..n].iter().all(|a| a.slot != b.slot)
                    })
                    && self.contracts[..i].iter().all(|other| {
                        other.pose != c.pose
                            || other.images.len() != c.images.len()
                            || !other.images.iter().all(|b| c.images.contains(b))
                    })
            })
    }
    pub fn matches(&self, images: &[(u8, &str)]) -> SeatReadyMatches {
        let mut found = SeatReadyMatches::default();
        if !self.valid()
            || images.is_empty()
            || images.len() > SEAT_READY_IMAGE_SLOTS
            || !images.iter().enumerate().all(|(i, (slot, id))| {
                ready_image_valid(*slot, id) && images[..i].iter().all(|(s, _)| s != slot)
            })
        {
            return found;
        }
        for c in &self.contracts {
            if c.images.len() == images.len()
                && c.images
                    .iter()
                    .all(|b| images.contains(&(b.slot, b.image.as_str())))
            {
                found.roles[c.pose.index()] = true;
            }
        }
        found
    }
}
impl Seat {
    pub fn ready_matches(&self, images: &[(u8, &str)]) -> SeatReadyMatches {
        self.rider_ready_policy
            .as_ref()
            .filter(|_| self.rider_pose.is_some())
            .map_or_else(SeatReadyMatches::default, |p| p.matches(images))
    }
}
/// A seat's hand holds: the rider's right and left hand.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatHands {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<[f32; 3]>,
    /// A node in the weapon attachment, evaluated with its animation and sway.
    /// When named, it owns that hand instead of the body-local point above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_node: Option<String>,
}
impl SeatHands {
    pub fn is_empty(&self) -> bool {
        self.right.is_none()
            && self.left.is_none()
            && self.right_node.is_none()
            && self.left_node.is_none()
    }
    pub fn node(&self, hand: usize) -> Option<&str> {
        match hand {
            0 => self.right_node.as_deref(),
            1 => self.left_node.as_deref(),
            _ => None,
        }
    }
    /// Right then left.
    pub fn each(&self) -> [Option<[f32; 3]>; 2] {
        [self.right, self.left]
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wheel {
    /// Hub node: the suspension's fully compressed mount point.
    pub position: [f32; 3],
    /// Torque derives this from the tire shape's bounds, not the datablock field.
    pub radius: f32,
    pub rest_length: f32,
    pub spring: f32,
    pub damping: f32,
    /// `WheeledVehicleSpring::antiSway`: pushes this side down by the
    /// difference in extension from the opposite wheel.
    pub anti_sway: f32,
    /// Resists the spring extending again, force per unit of extension
    /// speed over the spring's length, as `damping` resists compression.
    /// Torque's damper has no rebound (0, the default): a soft, long spring
    /// then throws the chassis back up. A loose suspension that soaks a
    /// landing without pogoing (an off-road truck) sets it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rebound_damping: f32,
    pub tire: Tire,
    pub steering: f32,
    /// Steering is already a wheel angle in radians. Off preserves the
    /// v20 squared-input curve. Useful for authored physical steering locks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub linear_steering: bool,
    pub powered: bool,
    pub model: String,
    /// Turns the tire model, authored with its hub axis along forward, so the
    /// axle lies along X with the tire's outer face pointing away from the chassis.
    pub model_rotation: [f32; 4],
}
/// `WheeledVehicleTire`: the tyre is a spring sideways and lengthways
/// (force per unit of deformation, damping on its rate, and relaxation that
/// lets the deformation go as the wheel spins), held inside a friction
/// circle of the wheel's load times the static friction, or the kinetic
/// friction once it slips.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tire {
    pub static_friction: f32,
    pub kinetic_friction: f32,
    pub lateral_force: f32,
    pub lateral_damping: f32,
    pub lateral_relaxation: f32,
    pub longitudinal_force: f32,
    pub longitudinal_damping: f32,
    pub longitudinal_relaxation: f32,
    /// The grip across the tyre as a share of the grip along it, stretching
    /// Torque's friction circle into an ellipse: Halo's Warthog grips 0.75
    /// along and 0.45 across at the front (0.6), 0.45 and 0.65 at the rear
    /// (1.44). 1, the default, is Torque's circle exactly.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub lateral_grip: f32,
}
/// `WheeledVehicleTire`'s constructor defaults.
impl Default for Tire {
    fn default() -> Self {
        Self {
            static_friction: 1.,
            kinetic_friction: 0.5,
            lateral_force: 10.,
            lateral_damping: 1.,
            lateral_relaxation: 1.,
            longitudinal_force: 10.,
            longitudinal_damping: 1.,
            longitudinal_relaxation: 1.,
            lateral_grip: 1.,
        }
    }
}
impl Tire {
    fn valid(&self) -> bool {
        [
            self.static_friction,
            self.kinetic_friction,
            self.lateral_force,
            self.lateral_damping,
            self.lateral_relaxation,
            self.longitudinal_force,
            self.longitudinal_damping,
            self.longitudinal_relaxation,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.)
            && self.lateral_grip.is_finite()
            && (0.05..=20.).contains(&self.lateral_grip)
    }
}
impl Wheel {
    /// This wheel's steer angle, right positive, for the vehicle's steering
    /// (`mSteering.x`, radians up to `maxSteeringAngle`), as
    /// `WheeledVehicle::updateForces` turns it (blocklandv20.exe 0x5746ea):
    /// the steering is squared (`-(s * |s|)`), and the wheel's axle is
    /// `right * cos + forward * sin * steering-factor`. A small turn of the
    /// mouse steers gently and full lock sharply, and a wheel steering
    /// against the front (the Tank's rear, -0.8) turns a little less.
    pub fn steer_angle(&self, steering: f32) -> f32 {
        let squared = if self.linear_steering {
            steering
        } else {
            steering * steering.abs()
        };
        (self.steering * squared.sin()).atan2(squared.cos())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Weapon {
    pub projectile: String,
    pub muzzle: Transform,
    pub pivot: [f32; 3],
    pub cooldown_ticks: u64,
    pub speed: f32,
    pub charge_ticks: u64,
    pub charge_steps: u8,
    pub sound: String,
    pub effect: String,
    /// Fires for as long as the trigger is held, spinning up and spreading
    /// as it goes (the Warthog's chaingun); left out, one shot a pull, at
    /// most every `cooldown_ticks`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic: Option<Automatic>,
    /// The kick each shot gives the vehicle, as a speed change in units/s
    /// (the impulse is this times its mass) back from the shot and up. Left
    /// out, v20's 5 for every gun but the cannon's 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recoil: Option<f32>,
    /// Authored [down, up] pitch endpoints for a look clip whose neutral
    /// sample is halfway through. Omitted clips retain v20's +/-90 degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look_animation_pitch: Option<[f32; 2]>,
    /// Weapon-frame muzzle node positions from full up to full down, sampled
    /// from the model's `look` clip at load (see `muzzle`). Empty without one.
    #[serde(skip)]
    pub look_muzzle: Vec<[f32; 3]>,
}
/// How a held trigger fires: Halo's `weap` rate of fire and error angle,
/// each rising from its first to its last value while the trigger stays
/// held. Letting go starts it again from the first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Automatic {
    /// Shots a second, from the pull to fully spun up.
    pub rate: [f32; 2],
    /// 120 Hz ticks from the first rate to the last.
    pub spin_up_ticks: u64,
    /// The widest a shot strays from the aim, in radians, from the pull to
    /// fully spread.
    #[serde(default)]
    pub spread: [f32; 2],
    /// 120 Hz ticks from the first spread to the last.
    #[serde(default)]
    pub spread_ticks: u64,
}
impl Automatic {
    fn along(values: [f32; 2], held: u64, ticks: u64) -> f32 {
        let t = if ticks == 0 {
            1.0
        } else {
            (held as f32 / ticks as f32).min(1.0)
        };
        values[0] + (values[1] - values[0]) * t
    }
    /// The ticks between shots after the trigger has been held `held` ticks.
    pub fn interval(&self, held: u64) -> u64 {
        let rate = Self::along(self.rate, held, self.spin_up_ticks);
        ((1.0 / (rate * crate::FIXED_DT)).round() as u64).max(1)
    }
    /// The widest a shot strays after the trigger has been held `held` ticks.
    pub fn spread(&self, held: u64) -> f32 {
        Self::along(self.spread, held, self.spread_ticks)
    }
    pub fn valid(&self) -> bool {
        self.rate
            .iter()
            .all(|r| r.is_finite() && (0.1..=120.0).contains(r))
            && self
                .spread
                .iter()
                .all(|s| s.is_finite() && (0.0..=0.5).contains(s))
            && self.spin_up_ticks <= 120 * 60
            && self.spread_ticks <= 120 * 60
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnergySettings {
    pub maximum: f32,
    pub minimum_jet: f32,
    pub drain_per_32ms: f32,
    pub recharge_per_32ms: f32,
    pub jet_force: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlightSettings {
    pub hover_height: f32,
    pub create_hover_height: f32,
    pub min_drag: f32,
    pub max_auto_speed: f32,
    pub auto_linear_force: f32,
    pub auto_angular_force: f32,
    pub auto_input_damping: f32,
    pub horizontal_surface_force: f32,
    pub vertical_surface_force: f32,
    pub steering_force: f32,
    pub steering_roll_force: f32,
    pub vertical_thrust_multiple: f32,
}
/// Blockland's flying forces on `WheeledVehicle` (blocklandv20.exe
/// `WheeledVehicle::updateForces` 0x5746a0, fields registered at 0x5703ea).
/// The thrust, lift and turning forces are `Definition::thrust`,
/// `reverse_thrust`, `lift`, `pitch_force`, `yaw_force` and `roll_force`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WheeledFlightSettings {
    /// `maxForwardVel`: thrust only below this speed along the nose; the
    /// surfaces bite fully at `stallSpeed` + this.
    pub max_forward_vel: f32,
    /// `maxReverseVel`: reverse thrust only below this speed.
    pub max_reverse_vel: f32,
    /// `horizontalSurfaceForce`: resists sideways air.
    pub horizontal_surface_force: f32,
    /// `verticalSurfaceForce`: resists air through the roof; what makes a
    /// raised nose climb.
    pub vertical_surface_force: f32,
    /// `stallSpeed`: below it the surfaces and turning forces do nothing.
    pub stall_speed: f32,
    /// `isSled` (datablock +0x378): the surfaces bite only while wheel 0
    /// touches the ground (0x57565f).
    pub sled: bool,
}
/// How a driver's keys and mouse steer a wheeled vehicle
/// (`WheeledVehicle::updateMove` 0x570be0; defaults from its constructor).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SteeringSettings {
    /// `steeringStrafeSteeringRate` (0x5716dc): steering a held strafe key
    /// adds per 32 ms tick, radians.
    pub strafe_rate: f32,
    /// `steeringUseAutoReturn`: a move with no mouse turn returns the
    /// steering toward straight.
    pub auto_return: bool,
    /// `steeringAutoReturnRate`: share returned per tick at full throttle.
    pub auto_return_rate: f32,
    /// `steeringAutoReturnMaxSpeed`: the throttle at which the return is full.
    pub auto_return_max_speed: f32,
    /// The driver's mouse looks around freely and the wheels steer toward
    /// where they look: the view holds its direction while the vehicle
    /// turns to meet it, at up to `strafe_rate` a 32 ms tick and no more
    /// than `max_steering`. Neither the strafe keys nor auto-return steer.
    /// Not a v20 mode; an Add-On's off-roader in the style of shooters
    /// where you drive where you look.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub toward_look: bool,
    /// In look steering, a held strafe key requests full left/right lock
    /// instead. Releasing it resumes steering toward the look.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keyboard_override: bool,
    /// The share of `max_steering` the driver can still turn to at
    /// `max_speed`, narrowing in a straight line from the full lock
    /// standing: a fast truck that turns sharply in a yard but cannot spin
    /// itself round at full speed sets about 0.4. 1, the default, is v20's
    /// lock at any speed.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub speed_lock: f32,
}
impl SteeringSettings {
    /// The share of the full lock the driver may steer to at `speed`.
    pub fn lock_at(&self, speed: f32, max_speed: f32) -> f32 {
        if self.speed_lock == 1. {
            return 1.;
        }
        let t = if max_speed > 0. {
            (speed.abs() / max_speed).min(1.)
        } else {
            1.
        };
        1. + (self.speed_lock - 1.) * t
    }
}
impl Default for SteeringSettings {
    fn default() -> Self {
        Self {
            strafe_rate: 0.1,
            auto_return: true,
            auto_return_rate: 0.9,
            auto_return_max_speed: 10.,
            toward_look: false,
            keyboard_override: false,
            speed_lock: 1.,
        }
    }
}
/// An animation the vehicle's model plays on its own, like a spinning
/// propeller (`ShapeBase::playThread(slot, sequence)` from a script such as
/// `onAdd`). Of a slot's threads, the first whose speed range holds the
/// vehicle's speed plays; one with no range always matches.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimationThread {
    /// Torque thread slot, 0 through 3. Threads on one slot replace each other.
    pub slot: u8,
    /// The model's sequence name.
    pub sequence: String,
    /// Playback rate: 1 is as authored, 2 twice as fast, and a negative rate
    /// plays backwards (`setThreadDir(slot, false)`).
    #[serde(default = "one")]
    pub rate: f32,
    /// Plays only at this speed or faster, units per second.
    #[serde(default)]
    pub min_speed: Option<f32>,
    /// Plays only below this speed.
    #[serde(default)]
    pub max_speed: Option<f32>,
}
fn one() -> f32 {
    1.
}
fn is_one(x: &f32) -> bool {
    *x == 1.
}
fn is_zero(x: &f32) -> bool {
    *x == 0.
}
/// An emitter a vehicle runs at one of its model's nodes while its speed is
/// in range: an Add-On script mounting an emitting image (`mountImage`)
/// under a speed test, like the Stunt Plane's wing-tip contrails.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trail {
    /// Require this wheel to touch the ground; speed then excludes motion
    /// along the suspension axis. None preserves airborne trails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_wheel: Option<usize>,
    /// The model node the image mounts at (`mount<mountPoint>`).
    pub node: String,
    /// The emitter's frame in the model: the node, then the image's offset
    /// and rotation. Its local up is the ejection axis.
    pub transform: Transform,
    /// Native emitter id: one of the definition's `effects`, or a base game
    /// emitter (`v20/emitter/<name>`).
    pub emitter: String,
    /// Emits only at this speed or faster, units per second.
    #[serde(default)]
    pub min_speed: Option<f32>,
    /// Emits only below this speed.
    #[serde(default)]
    pub max_speed: Option<f32>,
}
impl Trail {
    pub fn matches(&self, speed: f32) -> bool {
        self.min_speed.is_none_or(|m| speed >= m) && self.max_speed.is_none_or(|m| speed < m)
    }
}
/// Particle and emitter definitions a vehicle brings with it, in the base
/// game's effects library format. Their ids carry the vehicle package's
/// namespace; textures are the base game's.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VehicleEffects {
    #[serde(default)]
    pub particles: Vec<bri_content::effects::Particle>,
    #[serde(default)]
    pub emitters: Vec<bri_content::effects::Emitter>,
}
impl VehicleEffects {
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty() && self.emitters.is_empty()
    }
    /// Checks the definitions as the effects library would, textures aside:
    /// the client resolves those against the base game's.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.particles.len() <= 32 && self.emitters.len() <= 32,
            "too many vehicle effects"
        );
        let library = bri_content::effects::Library {
            schema_version: 1,
            lights: vec![],
            particles: self.particles.clone(),
            emitters: self.emitters.clone(),
            textures: self
                .particles
                .iter()
                .map(|p| (p.texture.clone(), "texture.png".to_owned()))
                .collect(),
        };
        library.validate()
    }
}
impl AnimationThread {
    pub fn matches(&self, speed: f32) -> bool {
        self.min_speed.is_none_or(|m| speed >= m) && self.max_speed.is_none_or(|m| speed < m)
    }
}
/// Third-person camera while riding (`Vehicle::getCameraTransform`; for
/// PlayerData mounts the player camera fields).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VehicleCamera {
    /// `cameraMaxDist`: distance behind the pivot.
    pub max_dist: f32,
    /// `cameraOffset` (`cameraVerticalOffset` on PlayerData): pivot height.
    pub offset: f32,
    /// `cameraTilt`: the view looks down this many radians.
    pub tilt: f32,
    /// `cameraLag`/`cameraDecay`: stock Torque's trailing camera. Blockland's
    /// `Vehicle::getCameraTransform` (0x56cc10) never reads them.
    pub lag: f32,
    pub decay: f32,
    /// The driver's third-person camera orbits the vehicle with the mouse
    /// instead of sitting level behind it. Not a v20 mode. Left out, v20's
    /// chase camera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit: Option<CameraOrbit>,
}
/// [`Definition::turret_sway`]: a damped spring that tips the turret
/// against the vehicle's acceleration in its own frame (back as it speeds
/// up, forward as it brakes, outward in a turn, down over a bump).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurretSway {
    /// Radians the turret tips per unit/s² of acceleration.
    pub response: f32,
    /// How fast it wobbles, cycles a second.
    #[serde(default = "sway_frequency")]
    pub frequency: f32,
    /// 0 rings on, 1 settles without overshooting.
    #[serde(default = "sway_damping")]
    pub damping: f32,
    /// The furthest it tips, degrees.
    #[serde(default = "sway_max")]
    pub max_degrees: f32,
}
fn sway_frequency() -> f32 {
    3.0
}
fn sway_damping() -> f32 {
    0.3
}
fn sway_max() -> f32 {
    4.0
}
impl TurretSway {
    pub fn valid(&self) -> bool {
        self.response.is_finite()
            && (0. ..=0.1).contains(&self.response)
            && self.frequency.is_finite()
            && (0.2..=20.).contains(&self.frequency)
            && self.damping.is_finite()
            && (0. ..=2.).contains(&self.damping)
            && self.max_degrees.is_finite()
            && (0. ..=30.).contains(&self.max_degrees)
    }
}
/// A free-orbit driver camera ([`VehicleCamera::orbit`]): it sits
/// `max_dist` from a pivot over the vehicle's box center, swung round by the
/// driver's look (its yaw and pitch, in the world for a `toward_look`
/// vehicle), and looks at the pivot. Looking down lifts it overhead, looking
/// up lowers it toward the ground; the same ground and wall check as the
/// chase camera keeps it out of both.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraOrbit {
    /// Lowest the camera swings, degrees of elevation over the pivot
    /// (negative is below it, looking up at the vehicle).
    pub min_pitch: f32,
    /// Highest it swings, degrees (90 is straight overhead, looking down).
    pub max_pitch: f32,
    /// The pivot's height over the box center, units: the camera looks at
    /// a point this far above the vehicle's middle.
    #[serde(default)]
    pub height: f32,
    /// How far the camera trails the vehicle's motion, seconds: the pivot
    /// follows the vehicle like a weight on a spring, a 0.1 lag closing
    /// about 63% of the gap each tenth of a second. 0 follows exactly.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub lag: f32,
}
impl CameraOrbit {
    /// The elevation range in radians, low then high.
    pub fn pitch_range(&self) -> (f32, f32) {
        (self.min_pitch.to_radians(), self.max_pitch.to_radians())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    pub datablock: String,
    pub name: String,
    pub family: Family,
    pub energy: EnergySettings,
    pub flight: Option<FlightSettings>,
    pub model: String,
    pub seats: Vec<Seat>,
    pub wheels: Vec<Wheel>,
    pub weapon: Option<Weapon>,
    pub attachment_model: Option<String>,
    pub attachment_collision_hulls: Vec<Vec<[f32; 3]>>,
    pub attachment_mount: Option<Transform>,
    pub attachment_fallback_seat: Option<Transform>,
    /// The turret (`attachment_model`) sways on a spring as the vehicle
    /// speeds up, brakes, turns and bounces. Drawn only; aim and shots
    /// are unchanged. Left out, the turret sits rigid, as in v20.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret_sway: Option<TurretSway>,
    pub collision_hulls: Vec<Vec<[f32; 3]>>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub mass: f32,
    /// Authored `massCenter`, chassis-local.
    pub mass_center: [f32; 3],
    /// Box whose inertia the rigid body uses: `massBox`, else the shape bounds.
    pub inertia_box: [f32; 3],
    pub density: f32,
    pub drag: f32,
    pub friction: f32,
    pub restitution: f32,
    pub max_damage: f32,
    pub burn_ticks: u64,
    pub invulnerable_ticks: u64,
    pub initial_explosion: Option<String>,
    pub final_explosion: Option<String>,
    pub initial_explosion_offset: f32,
    pub final_explosion_offset: f32,
    pub mount_distance: f32,
    pub engine_force: f32,
    pub engine_brake: f32,
    pub brake_force: f32,
    pub max_speed: f32,
    pub reverse_speed: f32,
    pub max_steering: f32,
    pub thrust: f32,
    pub reverse_thrust: f32,
    pub lift: f32,
    pub yaw_force: f32,
    pub pitch_force: f32,
    pub roll_force: f32,
    pub angular_drag: f32,
    pub jump_speed: f32,
    pub max_side_speed: f32,
    pub run_surface_angle: f32,
    pub impact_threshold: f32,
    pub impact_damage: f32,
    /// `steeringUseStrafeSteering`: the strafe keys steer. Otherwise the
    /// mouse steers and pitches the vehicle (Torque `mSteering`).
    pub strafe_steering: bool,
    /// Pulling the throttle against the way the vehicle rolls brakes it
    /// (with `brake_force`) until it stops, then drives the other way, as
    /// Halo's Warthog stops hard when you pull back. Off, v20's: the
    /// engine pushes back, which near top speed is next to nothing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub throttle_brakes: bool,
    /// A wheeled vehicle's engine stops pushing it backward at
    /// `reverse_speed`, as Halo's Warthog backs up at under half its
    /// forward speed. Off, v20's: wheeled vehicles back up as fast as they
    /// go forward (`maxBackwardSpeed` is only a hover vehicle's).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse_limited: bool,
    /// Seconds the vehicle may stand empty before it goes back to its
    /// spawn brick (Halo's abandoned Warthog). `None`, v20's: it stays
    /// where it was left. A mini-game's `vehicle_return_ms` overrides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_seconds: Option<f32>,
    /// The use key (the empty-hand click) on the vehicle within reach
    /// seats the player in the free seat nearest them; an upturned one
    /// still flips. Off, v20's: riders board
    /// only by landing on it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub board_on_use: bool,
    #[serde(default)]
    pub steering: SteeringSettings,
    /// Blockland's flying forces for a `Wheeled` or `Skis` vehicle; `None`
    /// is a car.
    #[serde(default)]
    pub wheeled_flight: Option<WheeledFlightSettings>,
    /// Animations the model plays by itself.
    #[serde(default)]
    pub threads: Vec<AnimationThread>,
    /// The engine's torque against wheel speed: `[share of the top wheel
    /// speed, share of engine_force]` points, by rising speed, joined by
    /// straight lines and held flat past the last. Empty is Torque's
    /// straight fall from full torque standing to none at `max_speed`
    /// (`[[0, 1], [1, 0]]`). A truck that pulls hard to near its top speed
    /// says `[[0, 1], [0.8, 0.9], [1, 0]]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub engine_curve: Vec<[f32; 2]>,
    /// Multiplies the gravity this vehicle falls with (v20's 20 units per
    /// second squared), and the weight its springs, hover and buoyancy
    /// carry. `None` is 1: a floaty off-roader that hangs in the air off a
    /// jump sets less.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gravity_scale: Option<f32>,
    /// Actor look pitch range, native up-positive radians, from PlayerData
    /// `maxLookAngle`/`minLookAngle`. Bounds a gunner's barrel.
    pub look_pitch: [f32; 2],
    /// PlayerData `maxUnderwaterForward/Backward/SideSpeed`.
    pub underwater_speeds: [f32; 3],
    pub camera: VehicleCamera,
    /// `setLookLimits(lookUpLimit, lookDownLimit)` while seated, as
    /// [down, up] fractions of the look range from straight down (0) to
    /// straight up (1); [0, 1] is unlimited.
    pub look_limits: [f32; 2],
    pub runover_speed: f32,
    pub runover_damage: f32,
    pub runover_push: f32,
    pub protect_direct: bool,
    pub protect_radius: bool,
    pub protect_burn: bool,
    /// Breaks bricks it strikes hard enough (an Add-On's wrecking ball).
    /// Which bricks break is the host's rule (v20's rocket brick damage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smash: Option<Smash>,
    /// Bowls players over where it may hurt them: a player it runs over
    /// tumbles away, so it rolls on through (a heavy ball ploughing through
    /// a crowd).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shove: bool,
    /// Harms nothing outside minigames: it breaks no bricks, hurts and
    /// bowls over no players and damages no vehicles there, whatever the
    /// host's rules for rockets are. It still pushes what it touches.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub harms_only_in_minigames: bool,
    /// At most this many of this vehicle per player at once, on top of the
    /// server's vehicle limits: a spawn brick past it tells its builder
    /// they already have that many.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_player: Option<u32>,
    /// How hard blasts push it, as a multiple of v20's rule (the impulse
    /// divided by its mass): a heavy toy that should still be knocked about
    /// by rockets sets more than 1. Only explosions and shots; contacts and
    /// a click's flip go by its mass alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blast_scale: Option<f32>,
    /// Emitters the vehicle runs at its nodes while its speed is in range.
    /// Cosmetic: each client draws them from the vehicle's presented motion.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trails: Vec<Trail>,
    /// Particles and emitters of this vehicle's own that its trails use.
    #[serde(default, skip_serializing_if = "VehicleEffects::is_empty")]
    pub effects: VehicleEffects,
    /// The engine's running sound: a loop at the vehicle that rises in
    /// pitch and volume with its speed. Cosmetic: each client plays it
    /// from the vehicle's presented motion. v20's vehicles have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_sound: Option<EngineSound>,
    /// Lamps at nodes of its model (headlights) that light the world while
    /// its driver has them on: the light key in the driving seat turns them
    /// on and off. Who has them on is shared; the light is each client's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lights: Vec<Lamp>,
    /// Tyres that skid: dust or smoke at each grounded wheel and a squeal
    /// while it slides across or spins along the ground. Cosmetic: each
    /// client derives the slide from the vehicle's presented motion.
    /// v20's vehicles have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skid: Option<Skid>,
    /// Authored fields retained as metadata only. Runtime reads typed native fields above.
    pub authored: BTreeMap<String, String>,
    pub adaptations: Vec<String>,
}
/// A lamp on a vehicle ([`Definition::lights`]): a point light at a node
/// of the vehicle's model, moved `offset` along the node's own axes (ahead
/// of a headlight's lens, so it lights the road rather than the bonnet).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lamp {
    #[serde(default)]
    pub activation: LampActivation,
    /// Native light definition for a mounted corona or other light presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    pub node: String,
    #[serde(default, skip_serializing_if = "is_origin")]
    pub offset: [f32; 3],
    /// Linear RGB; above 1 is brighter.
    pub color: [f32; 3],
    /// How far it reaches, units.
    pub radius: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LampActivation {
    #[default]
    Switch,
    Driver,
    Always,
}
impl LampActivation {
    pub fn enabled(self, switched: bool, driver: bool) -> bool {
        match self {
            Self::Switch => switched,
            Self::Driver => driver,
            Self::Always => true,
        }
    }
}
fn is_origin(v: &[f32; 3]) -> bool {
    *v == [0.0; 3]
}
/// [`Definition::skid`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skid {
    /// An emitter run at a sliding wheel's contact with the ground: one
    /// of the definition's `effects`, or a base game emitter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emitter: Option<String>,
    /// A looping sound played at the vehicle while any wheel slides,
    /// louder and higher the harder it slides (like `engine_sound`'s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    /// How fast a tyre must slide, in units per second, to skid.
    #[serde(default = "Skid::default_min_slip")]
    pub min_slip: f32,
    /// The slide at which the squeal is at its loudest.
    #[serde(default = "Skid::default_full_slip")]
    pub full_slip: f32,
}
impl Skid {
    fn default_min_slip() -> f32 {
        3.0
    }
    fn default_full_slip() -> f32 {
        12.0
    }
    pub fn validate(&self) -> Result<()> {
        let name = |n: &Option<String>| {
            n.as_deref().is_none_or(|n| {
                !n.trim().is_empty() && n.len() <= 128 && !n.chars().any(char::is_control)
            })
        };
        ensure!(
            name(&self.emitter)
                && name(&self.sound)
                && (self.emitter.is_some() || self.sound.is_some()),
            "skid names an emitter or a sound"
        );
        ensure!(
            self.min_slip.is_finite()
                && self.full_slip.is_finite()
                && (0.1..=100.0).contains(&self.min_slip)
                && self.full_slip > self.min_slip
                && self.full_slip <= 200.0,
            "skid min_slip must be 0.1 to 100, below full_slip (at most 200)"
        );
        Ok(())
    }
    /// How hard a tyre sliding at `slip` units per second skids, 0 (below
    /// `min_slip`) to 1 (at `full_slip` or more).
    pub fn strength(&self, slip: f32) -> f32 {
        if !(slip.is_finite() && slip >= self.min_slip) {
            return 0.0;
        }
        bri_console::Clamp::clamped(
            (slip - self.min_slip) / (self.full_slip - self.min_slip),
            0.0,
            1.0,
        )
        .max(0.05)
    }
}

/// A vehicle's engine sound ([`Definition::engine_sound`]). It plays while
/// someone sits in the seat that drives the vehicle (any seat, for a
/// vehicle no seat drives), and stops when they get out or the vehicle is
/// destroyed. Its pitch and volume, as multiples of the sound's own, run in
/// a straight line from `idle_*` standing still to `full_*` at
/// `full_speed`, and hold there past it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineSound {
    /// A looping sound of the vehicle Add-On's weapons pack (a key of its
    /// `sounds`, declared with `"looping": true`), or one of the game's.
    pub sound: String,
    #[serde(default = "EngineSound::default_idle_pitch")]
    pub idle_pitch: f32,
    #[serde(default = "EngineSound::default_full_pitch")]
    pub full_pitch: f32,
    #[serde(default = "EngineSound::default_idle_volume")]
    pub idle_volume: f32,
    #[serde(default = "EngineSound::default_full_volume")]
    pub full_volume: f32,
    /// Units per second. `None` is the vehicle's `max_speed` (20 for one
    /// without a top speed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_speed: Option<f32>,
}
impl EngineSound {
    fn default_idle_pitch() -> f32 {
        0.8
    }
    fn default_full_pitch() -> f32 {
        1.6
    }
    fn default_idle_volume() -> f32 {
        0.6
    }
    fn default_full_volume() -> f32 {
        1.0
    }
    /// The speed it reaches full pitch and volume at, for a vehicle whose
    /// top speed is `max_speed`.
    pub fn full_speed(&self, max_speed: f32) -> f32 {
        self.full_speed
            .unwrap_or(if max_speed > 0. { max_speed } else { 20. })
            .max(0.01)
    }
    /// (volume, pitch) at `speed` units per second, for a vehicle whose
    /// top speed is `max_speed`.
    pub fn at(&self, speed: f32, max_speed: f32) -> (f32, f32) {
        let t = if speed.is_finite() {
            bri_console::Clamp::clamped(speed / self.full_speed(max_speed), 0., 1.)
        } else {
            0.
        };
        (
            self.idle_volume + (self.full_volume - self.idle_volume) * t,
            self.idle_pitch + (self.full_pitch - self.idle_pitch) * t,
        )
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.sound.trim().is_empty() && self.sound.len() <= 128,
            "engine_sound names no sound"
        );
        ensure!(
            [self.idle_pitch, self.full_pitch]
                .iter()
                .all(|p| p.is_finite() && (0.25..=4.).contains(p)),
            "engine_sound pitch must be from 0.25 to 4"
        );
        ensure!(
            [self.idle_volume, self.full_volume]
                .iter()
                .all(|v| v.is_finite() && (0. ..=4.).contains(v)),
            "engine_sound volume must be from 0 to 4"
        );
        ensure!(
            self.full_speed
                .is_none_or(|s| s.is_finite() && s > 0. && s <= 10_000.),
            "engine_sound full_speed must be above 0"
        );
        Ok(())
    }
}
/// A vehicle that breaks what it hits: striking something at `speed` or
/// faster (units per second, into the surface) knocks out the brick it hit
/// and every brick within `radius` of the contact no larger than
/// `max_volume` (studs x studs x plates), thrown with `force`.
///
/// With `energy_per_volume`, breaking costs momentum: each brick costs its
/// volume times that many units of kinetic energy (half mass times speed
/// squared), nearest the contact first, until the hit's energy runs out.
/// Whatever it broke no longer holds it back: it carries on with the
/// energy left, so a hard enough hit punches straight through a wall.
///
/// With `wreck_speed`, it damages the vehicles it strikes where the host's
/// rules let its thrower damage them: nothing at `speed`, rising with the
/// square of the speed above it to the vehicle's whole health at
/// `wreck_speed`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Smash {
    pub speed: f32,
    #[serde(default)]
    pub radius: f32,
    pub max_volume: f32,
    #[serde(default = "Smash::default_force")]
    pub force: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_per_volume: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wreck_speed: Option<f32>,
}
impl Smash {
    fn default_force() -> f32 {
        10.
    }
    /// The share of a struck vehicle's health a hit at `speed` takes.
    pub fn wreck_share(&self, speed: f32) -> f32 {
        let Some(wreck) = self.wreck_speed else {
            return 0.;
        };
        let t = ((speed - self.speed) / (wreck - self.speed).max(0.001)).max(0.);
        (t * t).min(1.)
    }
}
/// How a seat's occupant controls things. Host input mapping, rider facing
/// and the client camera all follow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatRole {
    /// Turns freely in the seat and rides along.
    Passenger,
    /// Drives with the strafe keys steering; the mouse looks around.
    StrafeDriver,
    /// Drives with the mouse steering and pitching the vehicle.
    MouseDriver,
    /// Controls a player-type mount (horse, rowboat, cannon, turret): the
    /// mount faces where the rider looks.
    Actor,
    /// Aims an attached turret; the view turns with the hull.
    Gunner,
}
impl Definition {
    /// Families built from PlayerData: they move like players, not rigid bodies.
    pub fn is_actor(&self) -> bool {
        matches!(
            self.family,
            Family::Horse | Family::Rowboat | Family::Cannon | Family::Turret
        )
    }
    /// The colour a destroyed one is painted while it burns, over its spawn
    /// colour. v20's `WheeledVehicleData::Damage` and
    /// `FlyingVehicleData::Damage` (core scripts 18821, 18910) paint every
    /// node black (`setNodeColor("ALL", "0 0 0 1")`) the moment damage
    /// reaches `maxDamage`, so every vehicle of those classes, an Add-On's
    /// included, chars; PlayerData mounts die like players and keep theirs.
    /// The gravity this vehicle falls with: v20's, times `gravity_scale`.
    pub fn gravity(&self) -> f32 {
        crate::world::VEHICLE_GRAVITY * self.gravity_scale.unwrap_or(1.)
    }
    /// The share of `engine_force` a powered wheel gets turning at
    /// `share` of its top speed (`max_speed` over its radius), past which
    /// it gets none: `engine_curve`, else Torque's straight fall.
    pub fn engine_torque(&self, share: f32) -> f32 {
        if share > 1. {
            return 0.;
        }
        let Some(first) = self.engine_curve.first() else {
            return 1. - share;
        };
        if share <= first[0] {
            return first[1];
        }
        for w in self.engine_curve.windows(2) {
            let ([x0, y0], [x1, y1]) = (w[0], w[1]);
            if share <= x1 {
                return y0 + (y1 - y0) * (share - x0) / (x1 - x0);
            }
        }
        self.engine_curve.last().map_or(0., |p| p[1])
    }
    pub fn wreck_color(&self) -> Option<[f32; 4]> {
        matches!(self.family, Family::Wheeled | Family::Flying | Family::Ball)
            .then_some([0., 0., 0., 1.])
    }
    /// The emitters a wreck burns with until the final explosion: its
    /// `damageEmitter[0..3]`, by native emitter id (an Add-On's own from
    /// `effects`, else the base game's), each once. Torque's
    /// `Vehicle::updateDamageSmoke` runs emitter `i` past
    /// `damageLevelTolerance[i]` of `maxDamage`, so a destroyed vehicle runs
    /// every one it has; one without (a horse, a rowboat) does not burn.
    pub fn wreck_emitters(&self) -> Vec<String> {
        let mut ids: Vec<String> = vec![];
        for i in 0..3 {
            let Some(name) = self.authored.get(&format!("damageemitter[{i}]")) else {
                continue;
            };
            let name = name.trim().to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            let id = self
                .effects
                .emitters
                .iter()
                .find(|e| {
                    e.id.rsplit_once(":emitter/")
                        .is_some_and(|(_, n)| n == name)
                })
                .map_or_else(|| format!("v20/emitter/{name}"), |e| e.id.clone());
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }
    /// The actor supplying chassis controls. Actor guns share their weapon
    /// seat with the motor; wheel/flight vehicles use the controls flag.
    pub fn control_seat(&self) -> Option<usize> {
        self.seats
            .iter()
            .position(|s| s.controls)
            .or_else(|| self.is_actor().then(|| self.weapon_seat()).flatten())
    }
    pub fn weapon_seat(&self) -> Option<usize> {
        self.seats.iter().position(|s| s.weapon)
    }
    pub fn seat_role(&self, seat: usize) -> SeatRole {
        self.seat_role_for(seat, true)
    }
    /// The seat's role for a rider whose `$pref::Input::UseStrafeSteering`
    /// is `strafe_steering`: off, a strafe-steered vehicle's driver steers
    /// with the mouse (`amIStrafeSteering`, blocklandv20.exe 0x4d8010).
    pub fn seat_role_for(&self, seat: usize, strafe_steering: bool) -> SeatRole {
        let Some(s) = self.seats.get(seat) else {
            return SeatRole::Passenger;
        };
        if self.is_actor() && (s.controls || s.weapon) {
            SeatRole::Actor
        } else if s.weapon {
            SeatRole::Gunner
        } else if !s.controls {
            SeatRole::Passenger
        } else if self.steering.toward_look || (self.strafe_steering && strafe_steering) {
            SeatRole::StrafeDriver
        } else {
            SeatRole::MouseDriver
        }
    }
}
impl Pack {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        ensure!(
            std::fs::metadata(path)?.len() < 16 * 1024 * 1024,
            "vehicle pack too large"
        );
        let mut pack: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        pack.validate()?;
        pack.attach_muzzle_tracks(path.parent().unwrap_or(Path::new(".")))?;
        Ok(pack)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA_VERSION,
            "unknown vehicle schema"
        );
        ensure!(self.definitions.len() <= 128, "too many vehicles");
        ensure!(self.assets.len() <= 4096, "too many assets");
        let mut ids = std::collections::BTreeSet::new();
        for d in &self.definitions {
            ensure!(
                ids.insert(&d.id)
                    && (d.id.starts_with("v20.vehicle.")
                        || d.id
                            .split_once(':')
                            .is_some_and(|(_, rest)| rest.starts_with("vehicle/"))),
                "duplicate/invalid vehicle identity"
            );
            ensure!(d.per_player != Some(0), "a per-player cap of 0 allows none");
            ensure!(
                d.lights.len() <= 16
                    && d.lights.iter().all(|l| {
                        !l.node.is_empty()
                            && l.offset.iter().all(|v| v.is_finite() && v.abs() <= 100.)
                            && l.color
                                .iter()
                                .all(|c| c.is_finite() && (0. ..=8.).contains(c))
                            && l.radius.is_finite()
                            && (l.radius > 0. || l.effect.is_some())
                            && l.radius >= 0.
                            && l.radius <= 100.
                            && l.effect.as_ref().is_none_or(|id| {
                                !id.is_empty()
                                    && id.len() <= 160
                                    && !id.chars().any(char::is_control)
                            })
                    }),
                "{}: at most 16 lights, each on a node, colour 0 to 8 and radius above 0 up to 100",
                d.id
            );
            ensure!(
                d.blast_scale
                    .is_none_or(|s| s.is_finite() && s > 0. && s <= 100.),
                "blast_scale must be above 0 and at most 100"
            );
            ensure!(
                d.mass.is_finite() && d.mass > 0. && d.max_damage.is_finite() && d.max_damage > 0.,
                "invalid vehicle mass/damage"
            );
            ensure!(
                d.seats.len() <= 32 && d.wheels.len() <= 16,
                "too many seats/wheels"
            );
            ensure!(
                !d.collision_hulls.is_empty() && d.collision_hulls.len() <= 32,
                "missing/excessive collision geometry"
            );
            ensure!(
                [
                    d.energy.maximum,
                    d.energy.minimum_jet,
                    d.energy.drain_per_32ms,
                    d.energy.recharge_per_32ms,
                    d.energy.jet_force
                ]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.),
                "invalid energy parameters"
            );
            ensure!(
                d.flight.is_some() == (d.family == Family::Flying),
                "flying controller configuration mismatch"
            );
            if let Some(f) = &d.wheeled_flight {
                ensure!(
                    matches!(d.family, Family::Wheeled | Family::Skis)
                        && [
                            f.max_forward_vel,
                            f.max_reverse_vel,
                            f.horizontal_surface_force,
                            f.vertical_surface_force,
                            f.stall_speed,
                        ]
                        .iter()
                        .all(|v| v.is_finite() && *v >= 0.),
                    "invalid wheeled flight"
                );
            }
            let st = &d.steering;
            ensure!(
                [
                    st.strafe_rate,
                    st.auto_return_rate,
                    st.auto_return_max_speed
                ]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.),
                "invalid steering"
            );
            ensure!(d.threads.len() <= 16, "too many animation threads");
            for t in &d.threads {
                ensure!(
                    t.slot < 4
                        && !t.sequence.trim().is_empty()
                        && t.sequence.len() <= 64
                        && t.rate.is_finite()
                        && t.rate.abs() <= 100.
                        && [t.min_speed, t.max_speed]
                            .iter()
                            .flatten()
                            .all(|v| v.is_finite() && *v >= 0.),
                    "invalid animation thread"
                );
            }
            d.effects.validate()?;
            if let Some(engine) = &d.engine_sound {
                engine.validate()?;
            }
            if let Some(skid) = &d.skid {
                skid.validate()?;
            }
            ensure!(d.trails.len() <= 16, "too many trails");
            for t in &d.trails {
                ensure!(
                    t.contact_wheel.is_none_or(|i| i < d.wheels.len()),
                    "trail names an unknown contact wheel"
                );
                let q = glam::Quat::from_array(t.transform.rotation);
                ensure!(
                    !t.node.trim().is_empty()
                        && t.node.len() <= 64
                        && t.transform
                            .position
                            .iter()
                            .all(|v| v.is_finite() && v.abs() <= 1000.)
                        && q.is_finite()
                        && (q.length() - 1.).abs() < 1e-3
                        && [t.min_speed, t.max_speed]
                            .iter()
                            .flatten()
                            .all(|v| v.is_finite() && *v >= 0.),
                    "invalid trail"
                );
                ensure!(
                    t.emitter.starts_with("v20/emitter/")
                        || d.effects.emitters.iter().any(|e| e.id == t.emitter),
                    "trail names an unknown emitter {}",
                    t.emitter
                );
            }
            if let Some(f) = &d.flight {
                ensure!(
                    [
                        f.hover_height,
                        f.create_hover_height,
                        f.min_drag,
                        f.max_auto_speed,
                        f.auto_linear_force,
                        f.auto_angular_force,
                        f.auto_input_damping,
                        f.horizontal_surface_force,
                        f.vertical_surface_force,
                        f.steering_force,
                        f.steering_roll_force,
                        f.vertical_thrust_multiple
                    ]
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.),
                    "invalid flight parameters"
                );
                ensure!(
                    f.auto_input_damping <= 1. && f.min_drag > 0.,
                    "invalid flight damping"
                );
            }
            ensure!(
                d.engine_curve.len() <= 16
                    && d.engine_curve.iter().all(|[x, y]| x.is_finite()
                        && y.is_finite()
                        && (0. ..=1.).contains(x)
                        && (0. ..=4.).contains(y))
                    && d.engine_curve.windows(2).all(|w| w[0][0] < w[1][0]),
                "invalid engine curve: up to 16 [speed share 0..1, torque share 0..4] points by rising speed"
            );
            ensure!(
                d.steering.speed_lock.is_finite() && (0. ..=1.).contains(&d.steering.speed_lock),
                "invalid steering speed lock: 0 to 1"
            );
            ensure!(
                d.gravity_scale
                    .is_none_or(|g| g.is_finite() && (0. ..=4.).contains(&g)),
                "invalid gravity scale: 0 to 4"
            );
            ensure!(
                d.wheels
                    .iter()
                    .all(|w| w.rebound_damping.is_finite() && w.rebound_damping >= 0.),
                "invalid rebound damping"
            );
            if let Some(s) = &d.smash {
                ensure!(
                    [s.speed, s.radius, s.max_volume, s.force]
                        .iter()
                        .all(|v| v.is_finite() && *v >= 0.)
                        && s.speed >= 1.
                        && s.radius <= 8.
                        && s.force <= 200.
                        && s.energy_per_volume.is_none_or(|e| e.is_finite() && e > 0.)
                        && s.wreck_speed.is_none_or(|w| w.is_finite() && w > s.speed),
                    "invalid smash"
                );
            }
            let scalars = [
                d.density,
                d.drag,
                d.friction,
                d.restitution,
                d.engine_force,
                d.engine_brake,
                d.brake_force,
                d.max_speed,
                d.reverse_speed,
                d.mount_distance,
                d.max_steering,
                d.thrust,
                d.reverse_thrust,
                d.lift,
                d.yaw_force,
                d.pitch_force,
                d.roll_force,
                d.angular_drag,
                d.jump_speed,
                d.max_side_speed,
                d.run_surface_angle,
                d.impact_threshold,
                d.impact_damage,
                d.runover_speed,
                d.runover_damage,
                d.runover_push,
            ];
            ensure!(
                scalars.iter().all(|v| v.is_finite() && *v >= 0.),
                "invalid physics coefficient"
            );
            ensure!(
                d.density > 0. && d.burn_ticks < 120 * 3600 && d.invulnerable_ticks < 120 * 3600,
                "invalid timing/density"
            );
            ensure!(
                d.return_seconds.is_none_or(|s| (0.0..=3600.0).contains(&s)),
                "return_seconds must be 0 to 3600"
            );
            ensure!(
                d.bounds_min
                    .iter()
                    .chain(d.bounds_max.iter())
                    .all(|v| v.is_finite()),
                "invalid bounds"
            );
            ensure!(
                d.mass_center.iter().all(|v| v.is_finite())
                    && d.inertia_box.iter().all(|v| v.is_finite() && *v > 0.),
                "invalid mass properties"
            );
            ensure!(
                d.look_pitch
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= std::f32::consts::FRAC_PI_2 + 0.001)
                    && d.look_pitch[0] <= d.look_pitch[1]
                    && d.underwater_speeds
                        .iter()
                        .all(|v| v.is_finite() && *v >= 0.),
                "invalid actor look/swim parameters"
            );
            let c = &d.camera;
            ensure!(
                [c.max_dist, c.offset, c.tilt, c.lag, c.decay]
                    .iter()
                    .all(|v| v.is_finite())
                    && (0. ..=100.).contains(&c.max_dist)
                    && c.tilt.abs() <= std::f32::consts::FRAC_PI_2
                    && c.lag >= 0.
                    && c.decay >= 0.
                    && d.look_limits.iter().all(|v| (0. ..=1.).contains(v))
                    && d.look_limits[0] <= d.look_limits[1]
                    && c.orbit.is_none_or(|o| {
                        o.min_pitch.is_finite()
                            && o.max_pitch.is_finite()
                            && o.height.is_finite()
                            && -89. <= o.min_pitch
                            && o.min_pitch <= o.max_pitch
                            && o.max_pitch <= 89.
                            && o.height.abs() <= 20.
                            && o.lag.is_finite()
                            && (0. ..=2.).contains(&o.lag)
                    })
                    && d.turret_sway.is_none_or(|s| s.valid()),
                "invalid camera or look limits"
            );
            ensure!(
                d.seats.iter().filter(|s| s.controls).count() <= 1
                    && d.seats.iter().filter(|s| s.weapon).count() <= 1
                    && (d.is_actor() || !d.seats.iter().any(|s| s.controls && s.weapon)),
                "ambiguous vehicle control roles"
            );
            for seat in &d.seats {
                ensure!(
                    [&seat.enter_sound, &seat.exit_sound].iter().all(|s| {
                        s.len() <= 128 && !s.contains('.') && !s.chars().any(char::is_control)
                    }),
                    "invalid seat enter/exit sound: a sound profile name of at most 128 characters without a dot"
                );
                ensure!(
                    seat.rider_fallback_hip
                        .is_none_or(|p| seat.rider_pose.is_some()
                            && p.iter().all(|v| v.is_finite() && v.abs() <= 64.)),
                    "invalid authored rider fallback hip"
                );
                ensure!(
                    seat.rider_ready_policy
                        .as_ref()
                        .is_none_or(|p| seat.rider_pose.is_some() && p.valid()),
                    "invalid authored rider ready policy"
                );
                ensure!(
                    seat.eye
                        .is_none_or(|p| p.iter().all(|v| v.is_finite() && v.abs() <= 64.)),
                    "invalid seat eye"
                );
                ensure!(
                    seat.rider_pose.as_ref().is_none_or(|clip| !clip.is_empty()
                        && clip.len() <= 64
                        && !clip.chars().any(char::is_control)),
                    "invalid authored rider pose"
                );
            }
            for wheel in &d.wheels {
                ensure!(
                    wheel.position.iter().all(|v| v.is_finite())
                        && [wheel.radius, wheel.rest_length, wheel.spring, wheel.damping]
                            .iter()
                            .all(|v| v.is_finite() && *v > 0.)
                        && wheel.anti_sway.is_finite()
                        && wheel.anti_sway >= 0.
                        && wheel.tire.valid()
                        && wheel.steering.is_finite()
                        && (glam::Quat::from_array(wheel.model_rotation).length() - 1.).abs()
                            < 0.001,
                    "invalid wheel"
                );
            }
            ensure!(
                d.seats.iter().all(|seat| seat
                    .hands
                    .each()
                    .iter()
                    .flatten()
                    .all(|p| p.iter().all(|v| v.is_finite() && v.abs() <= 64.))),
                "invalid seat hands"
            );
            ensure!(
                d.seats.iter().enumerate().all(|(index, seat)| {
                    [seat.hands.node(0), seat.hands.node(1)]
                        .into_iter()
                        .flatten()
                        .all(|node| {
                            !node.is_empty()
                                && node.len() <= 64
                                && !node.chars().any(char::is_control)
                                && d.weapon_seat() == Some(index)
                                && d.attachment_model.is_some()
                        })
                }),
                "seat hand nodes need a weapon attachment seat and names up to 64 characters"
            );
            if let Some(w) = &d.weapon {
                ensure!(
                    w.look_animation_pitch.is_none_or(|[down, up]| {
                        down.is_finite()
                            && up.is_finite()
                            && (-std::f32::consts::FRAC_PI_2..0.).contains(&down)
                            && up <= std::f32::consts::FRAC_PI_2
                            && up > 0.
                    }),
                    "invalid weapon look animation pitch endpoints"
                );
                ensure!(
                    w.cooldown_ticks > 0
                        && w.speed.is_finite()
                        && w.speed > 0.
                        && w.charge_steps > 0
                        && w.charge_steps <= 64
                        && w.automatic
                            .as_ref()
                            .is_none_or(|a| a.valid() && w.charge_ticks == 0)
                        && w.recoil
                            .is_none_or(|r| r.is_finite() && (0.0..=100.0).contains(&r)),
                    "invalid weapon"
                );
            }
            ensure!(
                d.wheels.is_empty() || d.max_speed > 0.,
                "wheeled vehicle needs positive max speed"
            );
            ensure!(
                d.attachment_mount.is_none()
                    || (!d.attachment_collision_hulls.is_empty()
                        && d.attachment_fallback_seat.is_some()
                        && d.weapon_seat().is_some()),
                "incomplete attachment"
            );
            ensure!(
                d.attachment_collision_hulls.len() <= 32,
                "too many attachment hulls"
            );
            for hull in d
                .collision_hulls
                .iter()
                .chain(d.attachment_collision_hulls.iter())
            {
                ensure!(
                    hull.len() >= 4
                        && hull.len() <= 65536
                        && hull.iter().flatten().all(|n| n.is_finite()),
                    "invalid hull"
                );
            }
            for seat in &d.seats {
                ensure!(
                    seat.transform
                        .position
                        .iter()
                        .chain(seat.transform.rotation.iter())
                        .all(|x| x.is_finite()),
                    "invalid seat transform"
                );
            }
        }
        Ok(())
    }
    pub fn verify_assets(&self, root: impl AsRef<Path>) -> Result<()> {
        for asset in &self.assets {
            let p = Path::new(&asset.path);
            ensure!(
                !p.is_absolute()
                    && p.components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "unsafe asset path"
            );
            ensure!(
                std::fs::metadata(root.as_ref().join(p))?.len() < 64 * 1024 * 1024,
                "asset too large"
            );
            let bytes = std::fs::read(root.as_ref().join(p))?;
            ensure!(
                format!("{:x}", Sha256::digest(bytes)) == asset.sha256,
                "asset hash mismatch: {}",
                asset.path
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reordered_capabilities_and_ambiguous_roles_are_explicit() {
        let mut pack = crate::testing::pack();
        let d = pack
            .definitions
            .iter_mut()
            .find(|d| d.id == crate::testing::TANK)
            .unwrap();
        d.seats.swap(0, 2);
        assert_eq!(d.control_seat(), Some(2));
        assert_eq!(d.weapon_seat(), Some(0));
        assert_eq!(d.seat_role(0), SeatRole::Gunner);
        assert_ne!(d.seat_role(2), SeatRole::Gunner);
        pack.validate().unwrap();
        let d = pack
            .definitions
            .iter_mut()
            .find(|d| d.id == crate::testing::TANK)
            .unwrap();
        d.seats[2].weapon = true;
        assert!(
            pack.validate().is_err(),
            "one gun cannot be independently driven by two seats"
        );
    }

    /// `WheeledVehicle::updateForces` (0x5746ea): squared steering, and a
    /// wheel steering against the front turns by `atan(k tan(s|s|))`.
    #[test]
    fn wheels_steer_by_the_squared_steering() {
        let wheel = |steering: f32| Wheel {
            position: [0.0; 3],
            radius: 1.0,
            rest_length: 0.4,
            spring: 1.0,
            damping: 1.0,
            anti_sway: 0.0,
            rebound_damping: 0.,
            tire: Tire::default(),
            steering,
            linear_steering: false,
            powered: false,
            model: String::new(),
            model_rotation: [0.0, 0.0, 0.0, 1.0],
        };
        assert!((wheel(1.0).steer_angle(0.3) - 0.09).abs() < 1e-5);
        assert!((wheel(1.0).steer_angle(-0.3) + 0.09).abs() < 1e-5);
        assert!((wheel(1.0).steer_angle(0.9785) - 0.9785f32.powi(2)).abs() < 1e-5);
        let rear = wheel(-0.8).steer_angle(0.9785);
        assert!(
            (rear + (0.8 * 0.9785f32.powi(2).tan()).atan()).abs() < 1e-5,
            "{rear}"
        );
        assert_eq!(wheel(0.0).steer_angle(0.9785), 0.0);
        let mut linear = wheel(1.0);
        linear.linear_steering = true;
        let lock = 30.0_f32.to_radians();
        assert!((linear.steer_angle(lock) - lock).abs() < 1e-6);
        assert!((linear.steer_angle(-lock) + lock).abs() < 1e-6);
        // A speed lock is a share of the actual angle, not squared again.
        assert!((linear.steer_angle(lock * 0.6) - lock * 0.6).abs() < 1e-6);
    }
}
