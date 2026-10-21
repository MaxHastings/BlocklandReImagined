//! Fixed-tick player motor. Inputs contain intentions, never a client position.
use anyhow::{Result, ensure};
use bri_console::Clamp;
use bri_content::passage::Passages;
/// A brick or other contact id (`user_data`), and a player owner id.
type BrickId = u64;
type OwnerId = u64;
use crate::torque;
use glam::Vec3;
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Speed into a run surface below which a player in contact has landed.
const LANDING_SPEED: f32 = 1.0;
/// Original engine tick; v20 per-tick constants are converted with it.
pub const TORQUE_TICK: f32 = 0.032;
/// `PlayerStandardArmor.minJumpSpeed`/`maxJumpSpeed`: upward speeds over
/// which the jump impulse fades out.
const MIN_JUMP_SPEED: f32 = 20.0;
const MAX_JUMP_SPEED: f32 = 30.0;
/// `PlayerStandardArmor.maxFreelookAngle`: how far free look turns the head.
pub const MAX_FREELOOK: f32 = 3.0;
/// `PlayerStandardArmor.jumpDelay`: 3 Torque ticks between jumps. They run
/// down in the air too (updateMove 0x5AFAC3), so a held jump hops again on
/// the tick after landing. `PlayerTuning::jump_delay_ticks` counts 120 Hz
/// ticks.
const JUMP_DELAY_TICKS: u8 = 12;
/// `JumpSkipContactsMax` (canJump 0x5a2af8): a jump stays available until 8
/// Torque ticks pass without a jumpable surface.
const JUMP_WINDOW_TICKS: u8 = 8;
/// Time is counted in 1/3000 s: a 120 Hz step is 25 and a 32 ms Torque tick
/// 96, so the motor runs v20's ticks exactly inside the server's steps.
const STEP_PARTS: u8 = 25;
/// How far above its feet [`Player::room_for`] tests a new body, clear of
/// the floor the motor rests a hair inside.
const ROOM_LIFT: f32 = 0.05;
const TICK_PARTS: u8 = 96;
/// The collision group of a dead player's body. Player bodies move
/// through it (no player is blocked by a corpse, nor a corpse by another),
/// while weapons, vehicles and grabs still find it.
const CORPSE: Group = Group::GROUP_1;
/// What a player body's motion collides with: everything but corpses, and map collision that does not list moves
/// ([`bri_physics::blocks`]).
const MOVES_AGAINST: InteractionGroups = InteractionGroups::new(
    Group::ALL,
    bri_physics::blocks::query_filter_bits(bri_physics::blocks::MOVE).difference(CORPSE),
    InteractionTestMode::And,
);
/// Highest pass group ([`PlayerState::pass_group`]).
pub const MAX_PASS_GROUP: u8 = 15;
/// The collision group bit of pass group `group` (1 to 15), clear of the
/// corpse group's.
pub fn pass_bit(group: u8) -> Group {
    Group::from_bits_truncate(1 << (15 + u32::from(group.clamp(1, MAX_PASS_GROUP))))
}
/// The collision groups of a body in pass group `group` (0 for none):
/// it meets everything but another body of the same group.
pub fn pass_groups(group: u8) -> InteractionGroups {
    if group == 0 {
        InteractionGroups::all()
    } else {
        let bit = pass_bit(group);
        InteractionGroups::new(bit, Group::ALL.difference(bit), InteractionTestMode::And)
    }
}
/// What a body in pass group `group` moves against: everything but
/// corpses and the bodies of its own group.
pub fn moves_against(group: u8) -> InteractionGroups {
    if group == 0 {
        MOVES_AGAINST
    } else {
        let bit = pass_bit(group);
        InteractionGroups::new(
            bit,
            bri_physics::blocks::query_filter_bits(bri_physics::blocks::MOVE)
                .difference(CORPSE)
                .difference(bit),
            InteractionTestMode::And,
        )
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveInput {
    pub forward: f32,
    pub right: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// Free-look head turn relative to the body (`mHead.z`).
    #[serde(default)]
    pub head_yaw: f32,
    pub jump: bool,
    pub crouch: bool,
    pub jet: bool,
    /// The primary trigger, for a movement controller's own move button
    /// (`crate::controller::ControllerTick::attack`). Items fire from
    /// their own trigger messages as before; the motor ignores this.
    #[serde(default)]
    pub attack: bool,
    /// The host tick the player's game showed everyone else at when this
    /// move was made, 0 when not known: the host meets their aimed shots
    /// with other bodies where they saw them (`bri_sim::rewind`).
    #[serde(default)]
    pub seen: u64,
}
impl MoveInput {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.forward.is_finite()
                && self.right.is_finite()
                && self.forward.abs() <= 1.0
                && self.right.abs() <= 1.0,
            "Invalid movement axes"
        );
        ensure!(
            self.yaw.is_finite()
                && self.pitch.is_finite()
                && self.yaw.abs() <= std::f32::consts::PI
                && self.pitch.abs() <= std::f32::consts::FRAC_PI_2,
            "Invalid look angles"
        );
        ensure!(
            self.head_yaw.is_finite() && self.head_yaw.abs() <= MAX_FREELOOK,
            "Invalid head turn"
        );
        Ok(())
    }
}
/// The most [`PlayerState::speed_scale`] may be.
pub const MAX_SPEED_SCALE: f32 = 4.0;
/// The farthest a body's feet may be from the origin on any axis.
pub const MAX_FEET: f32 = 1_000_000.0;
/// The fastest a body may move. The host keeps every state it writes within
/// this and [`MAX_FEET`] ([`PlayerState::keep_in_bounds`]), and an
/// authoritative correction outside them is rejected
/// ([`PlayerState::check_bounds`]), so the two never disagree.
pub const MAX_SPEED: f32 = 1000.0;
/// A host correction this motor cannot take: one field's value is outside
/// what a player state may hold. The session can go on (the correction is
/// skipped or brought into bounds); a wrong owner is a different error.
#[derive(Debug, Clone, PartialEq)]
pub struct RejectedCorrection {
    pub field: &'static str,
    pub detail: String,
}
impl std::fmt::Display for RejectedCorrection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Invalid authoritative player correction: {} {}",
            self.field, self.detail
        )
    }
}
impl std::error::Error for RejectedCorrection {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub owner: OwnerId,
    pub feet: [f32; 3],
    pub velocity: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// Free-look head turn relative to `yaw`; drives the `headside` pose.
    #[serde(default)]
    pub head_yaw: f32,
    pub grounded: bool,
    pub crouched: bool,
    pub jetting: bool,
    #[serde(default)]
    pub jump: JumpState,
    /// What the player is (`setDataBlock`): an index into the session's
    /// archetype table, which both sides hold.
    #[serde(default)]
    pub archetype: crate::archetype::ArchetypeId,
    /// Uniform `setScale` (`setPlayerScale`).
    #[serde(default = "unit")]
    pub scale: f32,
    /// Jet energy (`mEnergy`), up to the datablock's `maxEnergy`.
    #[serde(default = "full_energy")]
    pub energy: f32,
    /// Share of the body's running, crouching and swimming speeds it moves
    /// at now, 0 to 4: an Add-On's `set_speed_scale` (slowed by a hit, a
    /// heavy gun). 1 leaves the body as its archetype made it.
    #[serde(default = "unit")]
    pub speed_scale: f32,
    /// The player is walking (an Add-On's `set_walking`): a body with momentum adds its `walking_share` of the acceleration.
    #[serde(default)]
    pub walking: bool,
    /// Where the motor is between v20's 32 ms ticks.
    #[serde(default)]
    pub tick: TorqueTick,
    /// A rope holding the body to a point (`Tether`), or none.
    #[serde(default)]
    pub tether: Option<Tether>,
    /// The archetype's movement controller's memory and animation intent
    /// (`crate::controller`); none while it is blank, as for every body
    /// without a controller.
    #[serde(default)]
    pub controller: Option<crate::controller::ControllerState>,
    /// An Add-On's pass group (`set_pass_group`), 0 for none: bodies and
    /// vehicles in the same group (1 to 15) move through each other and
    /// meet everything else. Part of the state so a client's prediction
    /// lets its player through the same bodies the host does.
    #[serde(default)]
    pub pass_group: u8,
    /// Lying prone and leaning, which an Add-On's rules set
    /// (`set_prone`, `set_lean`) for a body whose archetype can.
    #[serde(default)]
    pub posture: Posture,
}
/// [`PlayerState::posture`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Posture {
    /// Lying down: the body is its archetype's `prone` box, eye and speeds
    /// in place of its crouch ones, and stays crouched.
    pub prone: bool,
    /// How far it leans now, -1 (fully left) to 1 (fully right).
    pub lean: f32,
    /// The lean it is easing toward.
    pub lean_to: f32,
    /// Holding a ladder (a climb volume, `crate::climb`): it faces the
    /// rungs, gravity does not pull it and its move keys run it up and
    /// down.
    #[serde(default)]
    pub climbing: bool,
}
/// Longest a tether's rope may be, units: a bound on state a host or
/// player sends, past any v20 rope (the Grapple Rope's hook flies 800).
pub const MAX_TETHER_LENGTH: f32 = 1000.0;
/// Shortest a tether's rope may be reeled in to.
pub const MIN_TETHER_LENGTH: f32 = 1.0;
/// Fastest a tether may reel, units a second.
pub const MAX_TETHER_REEL: f32 = 80.0;
/// Strongest push the movement keys may give a body swinging on a tether,
/// units a second squared.
pub const MAX_TETHER_SWING: f32 = 60.0;
/// Fastest a tether's anchor may move, units a second.
pub const MAX_TETHER_DRIFT: f32 = 400.0;
/// A rope stretched this far past its length (something else carried the
/// body off: a teleport, an explosion's shove against a wall) breaks.
const TETHER_SNAP: f32 = 12.0;
/// Fastest a stretched rope pulls its body back in, units a second.
const TETHER_PULL: f32 = 60.0;
/// The movement keys stop adding to a swing past this speed along it.
const TETHER_SWING_TOP: f32 = 45.0;
/// How hard a reel brakes as it nears its mark, units a second squared: it
/// winds no faster than it could stop from in the length still to go, so a
/// body pulled in hard to a wall meets it gently.
const TETHER_STOP: f32 = 40.0;
/// How hard the winch keys' reel stops when the key is let go.
const TETHER_HALT: f32 = 200.0;
/// Slack under this much still counts as taut, for the swing push.
const TETHER_TAUT: f32 = 0.15;
/// A rope from a point to the body: the body may move freely within
/// `length` of `anchor` and no farther, so it swings like a pendulum and
/// keeps its speed, and the rope goes slack when the body comes closer.
/// It is part of the player's state, so client prediction runs the same
/// rope as the host.
///
/// The rope holds the body at its grip (`Tether::grip`), about where a
/// Blockhead's raised hands are. Each 32 ms tick, before the body moves:
/// the anchor moves on at `drift` (a rope tied to something moving); the
/// winch keys, when the rope has them (`keys`), set where it reels; the
/// length closes on `target` at `reel` units a second, braking to a stop
/// at it (a winch), and stalls rather than out-running a body held back;
/// while the rope is taut and the body is off the ground, the movement
/// keys push it along its swing at `swing` units a second squared, so a
/// player can pump a swing up from standing still; where the tick would
/// carry the grip past the rope's length, it is held to the rope (pulled
/// in at most `TETHER_PULL` a second faster than it was going), which is
/// what swings it round the anchor; and while it reels in, the body closes
/// no faster than it could stop from by the end, drawn straight along the
/// rope when it is `straight` (a grappling hook). The body still collides
/// as always: a rope does not pull anyone through a wall, and one
/// stretched `TETHER_SNAP` past its length breaks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Tether {
    pub anchor: [f32; 3],
    /// The rope's length now.
    pub length: f32,
    /// The length it reels toward.
    pub target: f32,
    /// Units a second it reels.
    pub reel: f32,
    /// Push from the movement keys while swinging, units a second squared.
    pub swing: f32,
    /// How fast the anchor moves, units a second: a rope tied to something
    /// moving (a vehicle, another player). The motor carries the anchor on
    /// at this speed each tick, so a player's own prediction keeps up with
    /// it between the host's updates.
    #[serde(default)]
    pub drift: [f32; 3],
    /// With `Some([shortest, longest])`, the jump key reels the rope in
    /// toward `shortest` and the crouch key pays it out toward `longest`
    /// while held; letting go of the key stops the rope where it is.
    #[serde(default)]
    pub keys: Option<[f32; 2]>,
    /// Which key is reeling now (1 jump, -1 crouch, 0 neither), so letting
    /// go of it stops only a reel the keys started.
    #[serde(default)]
    pub winding: i8,
    /// Reeled in, the body is drawn straight along the rope at the reel's
    /// speed, as a grappling hook's winch pulls (no swinging, no falling
    /// away under the line); otherwise it swings as a rope would.
    #[serde(default)]
    pub straight: bool,
}
impl Tether {
    pub fn validate(&self) -> Result<()> {
        let length = MIN_TETHER_LENGTH..=MAX_TETHER_LENGTH;
        ensure!(
            self.anchor
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                && length.contains(&self.length)
                && length.contains(&self.target)
                && (0.0..=MAX_TETHER_REEL).contains(&self.reel)
                && (0.0..=MAX_TETHER_SWING).contains(&self.swing)
                && self
                    .drift
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= MAX_TETHER_DRIFT)
                && self.keys.is_none_or(|[short, long]| {
                    length.contains(&short) && length.contains(&long) && short <= long
                })
                && (-1..=1).contains(&self.winding),
            "Invalid tether"
        );
        Ok(())
    }
    /// Where the rope takes hold of a body standing at `feet`: its raised
    /// hands, the same height crouched or standing so the rope never jumps.
    pub fn grip(feet: Vec3, tuning: &PlayerTuning) -> Vec3 {
        feet + Vec3::Y * tuning.stand_height * 0.85
    }
    /// How fast the rope winds now: `reel`, braking to a stop at `target`.
    fn rate(&self) -> f32 {
        let gap = (self.target - self.length).abs();
        self.reel.min((2.0 * TETHER_STOP * gap).sqrt() + 1.0)
    }
    /// The velocity a body at `feet` keeps this tick, `dt` seconds long,
    /// when it would move at `velocity`: see the type. `pushing` is the
    /// movement keys' direction and strength (zero on the ground), and
    /// `winch` which of the winch keys is held (1 jump, -1 crouch, 0
    /// neither). Returns false when the rope breaks.
    pub fn constrain(
        &mut self,
        feet: Vec3,
        tuning: &PlayerTuning,
        velocity: &mut Vec3,
        pushing: Vec3,
        winch: i8,
        dt: f32,
    ) -> bool {
        let mut anchor = Vec3::from(self.anchor) + Vec3::from(self.drift) * dt;
        if anchor.is_finite() {
            self.anchor = anchor.into();
        } else {
            anchor = Vec3::from(self.anchor);
        }
        if let Some([shortest, longest]) = self.keys {
            match winch {
                1 => self.target = shortest,
                -1 => self.target = longest,
                _ if self.winding != 0 => {
                    // Let go of the key: the winch brakes to a stop.
                    let rate = self.rate();
                    let stop = rate * rate / (2.0 * TETHER_HALT);
                    self.target = (self.length - f32::from(self.winding) * stop)
                        .clamped(shortest, longest)
                        .clamped(MIN_TETHER_LENGTH, MAX_TETHER_LENGTH);
                }
                _ => {}
            }
            self.winding = winch;
        }
        // A winch brakes as it nears its mark, so a body reeled up at full
        // speed is not flung on past the end of the rope.
        let gap = self.target - self.length;
        let rate = self.rate();
        let wound = self.length + gap.clamped(-rate * dt, rate * dt);
        let out = Self::grip(feet, tuning) - anchor;
        let distance = out.length();
        // A body held back (something in the way) stalls the winch rather
        // than the rope stretching until it breaks.
        self.length = if gap < 0.0 {
            wound.max((distance - 1.0).min(self.length))
        } else {
            wound
        };
        if distance > self.length + TETHER_SNAP {
            return false;
        }
        let Some(along) = out.try_normalize() else {
            return true;
        };
        if distance >= self.length - TETHER_TAUT && pushing != Vec3::ZERO {
            let across = pushing - along * pushing.dot(along);
            if let Some(direction) = across.try_normalize()
                && velocity.dot(direction) < TETHER_SWING_TOP
            {
                *velocity += direction * self.swing * pushing.length().min(1.0) * dt;
            }
        }
        // Where this tick would take the grip, held to the rope's length
        // (pulled in at most `TETHER_PULL` past where it would go): the
        // velocity is what gets it there. Projecting the position, not
        // pushing on the velocity, keeps a fast swing on a short rope
        // from gaining speed out of nothing.
        let ahead = out + *velocity * dt;
        let reach = ahead.length();
        let limit = self.length.max(reach - TETHER_PULL * dt);
        if reach > limit {
            *velocity = (ahead * (limit / reach) - out) / dt;
        }
        // Reeled in, the body is not let fly on past the end of the rope:
        // it closes on the anchor no faster than the winch takes rope in
        // (which eases off at the end), as a climber braking with a hand
        // on the rope, so a hard pull to a wall arrives gently.
        if gap < 0.0 && distance > self.target {
            let drift = Vec3::from(self.drift);
            // No faster than the winch, nor than the body could stop from
            // in what is left of its own way in.
            let allowed = rate.min((2.0 * TETHER_STOP * (distance - self.target)).sqrt() + 1.0);
            if self.straight {
                // Straight in along the rope, at the winch's speed.
                *velocity = drift - along * allowed;
            } else {
                let closing = -(*velocity - drift).dot(along);
                if closing > allowed {
                    *velocity += along * (closing - allowed);
                }
            }
        }
        true
    }
}
/// v20 moves a player once per 32 ms tick, and slides depend on it: Torque's
/// crease rule re-aims a wedged rider's whole speed along a lane once per
/// tick, and every drop, lip and seam is met with a 32 ms move. Running
/// `updateMove`/`updatePos` at 120 Hz instead carried a rider from the top of
/// "Mr.Block's Slides" 39 units down where 32 ms ticks carry it 353. So the
/// motor runs whole Torque ticks inside the server's 120 Hz steps.
/// `PlayerState::feet` is where the last tick left the body, as on v20's
/// server; `shown_feet` places it between the last two ticks, as v20's
/// client renders it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TorqueTick {
    /// The feet the last tick wrote; anything else moved the body since.
    pub feet: [f32; 3],
    /// Feet after the tick before it.
    pub from: [f32; 3],
    /// Time since the last tick, in 1/3000 s (below 96).
    pub phase: u8,
    /// Jump was held at some step since the last tick (a Move's trigger).
    pub jump: bool,
}
fn unit() -> f32 {
    1.0
}
fn full_energy() -> f32 {
    100.0
}
/// Jump/contact bookkeeping and the origin of the current airborne motion.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JumpState {
    /// Ticks left before another jump (`mJumpDelay`).
    pub delay: u8,
    /// Ticks since the last jumpable contact (`mJumpSurfaceLastContact`).
    pub since_contact: u8,
    /// Last jumpable surface normal (`mJumpSurfaceNormal`).
    pub normal: [f32; 3],
    /// The last blocking hit met a ceiling (v20 0x8A2): no jump until the
    /// next hit that does not.
    #[serde(default)]
    pub ceiling: bool,
    /// Accepted jets took over this flight. Persists through release until
    /// landing or an actual jump, independently of rendered/sample frames.
    pub jet_flight: bool,
}
impl Default for JumpState {
    fn default() -> Self {
        Self {
            delay: 0,
            since_contact: JUMP_WINDOW_TICKS,
            normal: [0.0, 1.0, 0.0],
            ceiling: false,
            jet_flight: false,
        }
    }
}
impl PlayerState {
    /// Bring feet and motion within [`MAX_FEET`] and [`MAX_SPEED`]: the
    /// fastest stays the fastest, in the same direction. Non-finite values
    /// are left for [`Self::check_bounds`] to refuse.
    pub fn keep_in_bounds(&mut self) {
        let velocity = Vec3::from(self.velocity);
        if velocity.is_finite() {
            self.velocity = velocity.clamp_length_max(MAX_SPEED).to_array();
        }
        let feet = Vec3::from(self.feet);
        if feet.is_finite() {
            self.feet = feet
                .clamp(Vec3::splat(-MAX_FEET), Vec3::splat(MAX_FEET))
                .to_array();
        }
    }
    /// Whether every value of this state is one a player may hold, naming
    /// the first field that is not.
    pub fn check_bounds(&self) -> std::result::Result<(), RejectedCorrection> {
        let reject = |field, detail| Err(RejectedCorrection { field, detail });
        if !self
            .feet
            .iter()
            .all(|v| v.is_finite() && v.abs() <= MAX_FEET)
        {
            return reject(
                "feet",
                format!(
                    "{:?} (finite, magnitude per axis <= {MAX_FEET} required)",
                    self.feet
                ),
            );
        }
        // The host keeps the speed (the length) within MAX_SPEED, so each
        // axis is within it too.
        if !self
            .velocity
            .iter()
            .all(|v| v.is_finite() && v.abs() <= MAX_SPEED)
        {
            return reject(
                "velocity",
                format!(
                    "{:?} (finite, magnitude per axis <= {MAX_SPEED} required)",
                    self.velocity
                ),
            );
        }
        if !(self.yaw.is_finite() && self.pitch.is_finite()) {
            return reject(
                "look",
                format!("yaw={} pitch={} (finite required)", self.yaw, self.pitch),
            );
        }
        if self.tick.phase >= TICK_PARTS {
            return reject(
                "motor tick",
                format!(
                    "phase={} (less than {TICK_PARTS} required)",
                    self.tick.phase
                ),
            );
        }
        if !self
            .tick
            .from
            .iter()
            .chain(&self.tick.feet)
            .all(|v| v.is_finite())
        {
            return reject(
                "motor tick",
                format!(
                    "from={:?} feet={:?} (finite required)",
                    self.tick.from, self.tick.feet
                ),
            );
        }
        if let Some(tether) = &self.tether
            && let Err(error) = tether.validate()
        {
            return reject("tether", format!("{tether:?}: {error}"));
        }
        if self.pass_group > MAX_PASS_GROUP {
            return reject(
                "pass group",
                format!("{} (at most {MAX_PASS_GROUP} required)", self.pass_group),
            );
        }
        Ok(())
    }
    /// This state as it is once `carry` (an opening's) has taken the body
    /// through: feet, where it is drawn from, motion and heading, with the
    /// body upright and its middle `middle` above the feet.
    pub fn carried(&self, carry: &glam::Affine3A, middle: f32) -> Self {
        let feet = |f: [f32; 3]| carry_feet(carry, Vec3::from(f), middle).to_array();
        let mut out = self.clone();
        out.feet = feet(self.feet);
        out.tick.feet = feet(self.tick.feet);
        out.tick.from = feet(self.tick.from);
        out.velocity = carry
            .transform_vector3(Vec3::from(self.velocity))
            .to_array();
        out.yaw = bri_content::passage::carried_yaw(carry, self.yaw);
        out.jump.normal = carry
            .transform_vector3(Vec3::from(self.jump.normal))
            .to_array();
        out
    }
    /// The way the body faces: its movement controller's own heading when
    /// it turns the body itself (Mario skidding round), else the look's yaw.
    pub fn body_yaw(&self) -> f32 {
        self.controller
            .and_then(|c| c.heading())
            .unwrap_or(self.yaw)
    }
    /// Where to draw the body: between the last two Torque ticks, `phase` of
    /// a tick along, so it moves smoothly at 120 Hz and any frame rate.
    pub fn shown_feet(&self) -> [f32; 3] {
        let tick = &self.tick;
        if tick.feet != self.feet {
            return self.feet;
        }
        Vec3::from(tick.from)
            .lerp(
                Vec3::from(self.feet),
                f32::from(tick.phase) / f32::from(TICK_PARTS),
            )
            .to_array()
    }
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }
    /// The `Eye` node: above the feet and slightly ahead of the body's
    /// facing (it follows the body, not the head's free look).
    pub fn eye(&self, tuning: &PlayerTuning) -> Vec3 {
        Vec3::from(self.feet)
            + Vec3::Y
                * match (self.crouched, self.posture.prone, tuning.prone) {
                    (_, true, Some(prone)) => prone.eye,
                    (true, ..) => tuning.crouch_eye,
                    _ => tuning.stand_eye,
                }
            + self.eye_ahead(tuning)
            + self.lean_offset(tuning)
    }
    /// How far a lean moves the eye: sideways, to the body's right for a
    /// lean right.
    pub fn lean_offset(&self, tuning: &PlayerTuning) -> Vec3 {
        let Some(lean) = tuning.lean else {
            return Vec3::ZERO;
        };
        Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()) * (self.posture.lean * lean.distance)
    }
    /// How far a lean tilts the view, radians (right positive).
    pub fn lean_tilt(&self, tuning: &PlayerTuning) -> f32 {
        tuning
            .lean
            .map_or(0.0, |lean| self.posture.lean * lean.angle)
    }
    /// How far a lean tilts the drawn body, radians (right positive): the
    /// view's tilt unless the lean sets its own `body_angle`.
    pub fn lean_body_tilt(&self, tuning: &PlayerTuning) -> f32 {
        tuning.lean.map_or(0.0, |lean| {
            self.posture.lean * lean.body_angle.unwrap_or(lean.angle)
        })
    }
    /// [`Self::eye`], or with `seated` the `Eye` node of the held `sit`
    /// pose (`/sit`, a seat whose pose is `sit`): it owns the node over
    /// crouching, so a seated player sees from the drawn head.
    pub fn posed_eye(&self, tuning: &PlayerTuning, seated: bool) -> Vec3 {
        if !seated {
            return self.eye(tuning);
        }
        Vec3::from(self.feet)
            + Vec3::Y * tuning.sit_eye
            + Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos()) * tuning.sit_eye_forward
    }
    /// How far the `Eye` node sits ahead of the body along its facing.
    pub fn eye_ahead(&self, tuning: &PlayerTuning) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos()) * tuning.eye_forward
    }
}
/// The collision body's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    /// v20's upright box, `width` wide and `stand_height` tall.
    #[default]
    Box,
    /// A sphere `width` across; both heights must equal the width.
    Ball,
    /// An upright cylinder `width` across and as tall as the box would be
    /// (an Unreal pawn): a wall's end or a door frame turns it aside where
    /// the box stops dead against the face.
    Cylinder,
}
/// How move input steers the body.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Steering {
    /// v20: the body faces where the player looks and strafes sideways.
    #[default]
    Strafe,
    /// A vehicle: left and right turn the body at `turn_rate`, the look
    /// direction is ignored and there is no sideways movement.
    Turn,
}
/// Motor constants (a `PlayerData` datablock). Every field has the standard
/// player's value by default, so data may name only what it changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlayerTuning {
    pub body: Body,
    pub steering: Steering,
    /// Radians a second a `turn` body turns at full left or right.
    pub turn_rate: f32,
    pub width: f32,
    pub stand_height: f32,
    pub crouch_height: f32,
    pub stand_eye: f32,
    pub crouch_eye: f32,
    /// How far the eye sits ahead of the box centre, along the facing.
    pub eye_forward: f32,
    /// Eye height above the feet at the end of the `sit` sequence.
    pub sit_eye: f32,
    /// How far that eye sits ahead of the box centre (negative: behind).
    pub sit_eye_forward: f32,
    pub forward: f32,
    pub backward: f32,
    pub sideways: f32,
    pub underwater_forward: f32,
    pub underwater_backward: f32,
    pub underwater_sideways: f32,
    pub density: f32,
    pub swim_acceleration: f32,
    pub swim_rise: f32,
    pub dive_acceleration: f32,
    pub crouch_forward: f32,
    pub crouch_backward: f32,
    pub crouch_sideways: f32,
    pub acceleration: f32,
    pub air_control: f32,
    pub drag: f32,
    pub gravity: f32,
    pub jump_speed: f32,
    pub jet_acceleration: f32,
    pub jet_lift: f32,
    pub horizontal_max_speed: f32,
    pub horizontal_resist_speed: f32,
    pub horizontal_resist_factor: f32,
    pub up_max_speed: f32,
    pub up_resist_speed: f32,
    pub up_resist_factor: f32,
    pub step_height: f32,
    /// A body standing on a floor stays on it down steps and slopes: when the ground falls away under a walking (not jumping)
    /// body by no more than its `step_height`, it is lowered to the floor below instead of falling. False for v20's bodies, which
    /// fall off every edge.
    pub follow_floor: bool,
    pub slope_degrees: f32,
    pub jump_surface_degrees: f32,
    /// `jumpDelay`, in 120 Hz ticks.
    pub jump_delay_ticks: u8,
    /// `canJet`.
    pub can_jet: bool,
    /// `maxEnergy`.
    pub max_energy: f32,
    /// `rechargeRate`, per second.
    pub recharge: f32,
    /// `minJetEnergy`: energy needed to keep jetting.
    pub min_jet_energy: f32,
    /// `jetEnergyDrain`, per second of jetting.
    pub jet_drain: f32,
    /// Water coverage from which the underwater speeds apply. A floating
    /// rowboat is always in the water.
    pub swim_coverage: f32,
    /// Falling damage by a source law, when the body has one: none for v20's
    /// bodies, which take `speed * SPEED_DAMAGE_SCALE` past `minImpactSpeed`.
    pub fall: Option<Fall>,
    /// Lying prone, when the body can: none for v20's bodies.
    pub prone: Option<Prone>,
    /// Leaning left and right, when the body can: none for v20's bodies.
    pub lean: Option<Lean>,
    /// Climbing ladders, when the body can: none for v20's bodies.
    pub climb: Option<crate::climb::Climb>,
    /// Getting out of geometry that has closed around the body, when it can:
    /// none for v20's bodies.
    pub unstick: Option<Unstick>,
    /// How far the body lifts to cross the lip of a wall it walks into
    /// (UE2's `moveSmooth` and `stepUp`: lift, move on, lower again, and
    /// failing that slide along the wall's face), when it moves that way:
    /// none for v20's bodies, which step only off a surface they stand on.
    pub wall_step: Option<WallStep>,
    /// Keeping speed (a held move accelerates the body, the floor's friction slows it, the air steers it less), when the body
    /// has it: none for v20's bodies, which are steered straight to the move's speed.
    pub momentum: Option<crate::momentum::Momentum>,
    /// The archetype's movement controller, bound from its module by the
    /// side that runs it (`Archetypes::bind_controllers`). Never data: the
    /// archetype carries the module, and a table read from data has none
    /// bound until then.
    #[serde(skip)]
    pub controller: Option<crate::controller::Controller>,
}
impl Default for PlayerTuning {
    fn default() -> Self {
        // Speeds, runForce/mass, air control, drag, jumpForce/mass, resistance
        // and runSurfaceAngle: recovered PlayerStandardArmor. Gravity, jet thrust,
        // jet lift and step height (maxStepHeight default): v20 engine constants.
        // Box dimensions: the v20 datablock boxes at 0.25 engine scale. Eyes:
        // m.dts `Eye` node, standing and at the end of the `crouch` and `sit`
        // sequences (Player::getRenderEyeTransform 0x5aafa0 reads the
        // animated node).
        // Ground snap remains an adaptation assumption; see
        // docs/player-simulation.md.
        Self {
            body: Body::Box,
            steering: Steering::Strafe,
            turn_rate: 3.0,
            width: 1.25,
            stand_height: 2.65,
            crouch_height: 1.0,
            stand_eye: 2.156_496_5,
            crouch_eye: 0.626_668_45,
            eye_forward: 0.141_154_87,
            sit_eye: 1.635_348_9,
            sit_eye_forward: -0.262_436_76,
            forward: 7.0,
            backward: 4.0,
            sideways: 6.0,
            underwater_forward: 8.4,
            underwater_backward: 7.8,
            underwater_sideways: 7.8,
            density: 0.7,
            // v20 swim pushes, per 32 ms Torque tick: 0.5 along the move
            // direction, 0.75 up while holding jump, 1 down while crouching.
            swim_acceleration: 0.5 / TORQUE_TICK,
            swim_rise: 0.75 / TORQUE_TICK,
            dive_acceleration: 1.0 / TORQUE_TICK,
            crouch_forward: 3.0,
            crouch_backward: 2.0,
            crouch_sideways: 2.0,
            acceleration: 48.0,
            air_control: 0.1,
            drag: 0.1,
            gravity: 20.0,
            jump_speed: 12.0,
            // Engine thrust 2000 / mass for players of mass 90 or more.
            jet_acceleration: 2000.0 / 90.0,
            jet_lift: 0.7,
            horizontal_max_speed: 68.0,
            horizontal_resist_speed: 33.0,
            horizontal_resist_factor: 0.35,
            up_max_speed: 80.0,
            up_resist_speed: 25.0,
            up_resist_factor: 0.3,
            step_height: 1.0,
            follow_floor: false,
            slope_degrees: 70.0,
            jump_surface_degrees: 80.0,
            jump_delay_ticks: JUMP_DELAY_TICKS,
            can_jet: true,
            max_energy: 100.0,
            recharge: 0.8 / TORQUE_TICK,
            min_jet_energy: 0.0,
            jet_drain: 0.0,
            swim_coverage: 0.9,
            fall: None,
            prone: None,
            lean: None,
            climb: None,
            unstick: None,
            wall_step: None,
            momentum: None,
            controller: None,
        }
    }
}
/// What overlaps a body set down on a spawn spot ([`Player::spot_near`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blocker {
    /// The collider's index in the physics world.
    pub collider: u32,
    /// Its `user_data` tag.
    pub tag: u128,
    /// The middle of its bounding box.
    pub centre: Vec3,
    /// The size of its bounding box: told apart a thin wall from a collider
    /// that spans the map.
    pub size: Vec3,
}
/// Where [`Player::spot_near`] puts a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    /// The spot is free (or the body has no `unstick` and keeps it).
    Clear(Vec3),
    /// The spot overlaps `blocker`; the body fits at `to`.
    Moved { to: Vec3, blocker: Blocker },
    /// The spot overlaps `blocker` and nothing near is free.
    Stuck { blocker: Blocker },
}
/// [`PlayerTuning::wall_step`]: a body that runs into a near-vertical wall
/// lifts by up to `lift`, makes the rest of its move and lowers again onto
/// what it stands on; if another wall stops that lifted move after more
/// than `retry` of travel, it steps again for what is left. Otherwise it
/// slides along the wall's face.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallStep {
    /// How far the body is lifted, above zero.
    pub lift: f32,
    /// The least travel a stopped lifted move needs to count, 0 or more.
    pub retry: f32,
}
impl WallStep {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.lift.is_finite() && self.lift > 0.0,
            "wall_step lift must be above zero"
        );
        ensure!(
            self.retry.is_finite() && self.retry >= 0.0,
            "wall_step retry must be 0 or more"
        );
        Ok(())
    }
}
/// [`PlayerTuning::unstick`]: a body that stays inside solid geometry (a
/// door or lift closed on it, a prop or a brick landed on it) is moved out
/// after `seconds` (0: at once). Tried in order, each only if the body then
/// fits: straight up (a body sunk into a floor), then a vote of four probes
/// around it (each a thin slice of half the body's radius, `probe` of the
/// radius away: at most two free ones count, and the body moves by the sum
/// of their offsets, twice as far with just one), then the nearest free
/// spot within `reach`, searched every `step` outward and upward. A body
/// set down on a spawn it does not fit moves the same way ([`Player::spot_near`]). Other
/// players never count: they move aside.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unstick {
    /// How long the body may stay embedded before it is moved, 0 or more.
    pub seconds: f32,
    /// How far from the body, in radii, each vote probe sits, above zero.
    #[serde(default = "Unstick::default_probe")]
    pub probe: f32,
    /// The farthest the body is moved, above zero.
    pub reach: f32,
    /// The spacing of the spots tried, above zero and at most `reach`.
    pub step: f32,
}
impl Unstick {
    fn default_probe() -> f32 {
        0.55
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.seconds.is_finite() && self.seconds >= 0.0,
            "Unstick seconds must be 0 or more"
        );
        ensure!(
            self.probe.is_finite() && self.probe > 0.0,
            "Unstick probe must be above zero"
        );
        ensure!(
            self.reach.is_finite() && self.reach > 0.0,
            "Unstick reach must be above zero"
        );
        ensure!(
            self.step.is_finite() && self.step > 0.0 && self.step <= self.reach,
            "Unstick step must be above zero and at most its reach"
        );
        Ok(())
    }
}
/// [`PlayerTuning::fall`]: striking the ground downward faster than
/// `threshold` units a second costs `(speed - threshold) * per_speed` health
/// (a body that falls less far takes none). Only the downward speed counts.
/// A game whose falling damage starts at a minimum and tops out sets `base`
/// (the harm of a fall at the threshold, 0 when left out) and `max` (the most
/// a fall costs, no limit when left out).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fall {
    pub threshold: f32,
    pub per_speed: f32,
    #[serde(default)]
    pub base: f32,
    #[serde(default)]
    pub max: Option<f32>,
}
impl Fall {
    /// The harm of landing at `speed` for a body at `scale` (the threshold
    /// grows with it): None when it is below the threshold.
    pub fn harm(&self, speed: f32, scale: f32) -> Option<f32> {
        let threshold = self.threshold * scale;
        (speed > threshold).then(|| {
            let harm = self.base + (speed - threshold) * self.per_speed;
            self.max.map_or(harm, |m| harm.min(m))
        })
    }
}
/// [`PlayerTuning::prone`]: the body lying down, in place of its crouch.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prone {
    /// The box's height lying down, above zero and at most the crouch's.
    pub height: f32,
    /// The eye's height above the feet lying down.
    pub eye: f32,
    /// Seconds the first-person eye takes to go between the standing or
    /// crouched height and the lying one, 0 to 5 (0: as quickly as a crouch).
    #[serde(default)]
    pub seconds: f32,
    /// Crawling speeds forward, backward and sideways.
    pub forward: f32,
    pub backward: f32,
    pub sideways: f32,
}
/// [`PlayerTuning::lean`]: the eye slides sideways and the view tilts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lean {
    /// How far the eye moves sideways at full lean, at most half the body's
    /// width: the eye never leaves the body, so a lean around a corner
    /// shows no more than it can be shot at.
    pub distance: f32,
    /// How far the view and the body tilt at full lean, radians, 0 to 0.6.
    #[serde(default = "Lean::default_angle")]
    pub angle: f32,
    /// How far the drawn body tilts at full lean when that is not the view's
    /// `angle` (a game that rolls the view but poses the body itself leaves
    /// it upright with 0), radians, 0 to 0.6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_angle: Option<f32>,
    /// Seconds from upright to full lean, 0.05 to 2.
    #[serde(default = "Lean::default_seconds")]
    pub seconds: f32,
}
impl Lean {
    fn default_angle() -> f32 {
        0.3
    }
    fn default_seconds() -> f32 {
        0.25
    }
}
impl PlayerTuning {
    /// Eye height above the feet, `fraction` of the way from standing (0)
    /// to crouched (1).
    pub fn eye_height(&self, fraction: f32) -> f32 {
        self.stand_eye - (self.stand_eye - self.crouch_eye) * fraction
    }
    /// Torque `setScale` scales the player's box and eye; speeds, forces and
    /// the step height stay those of the datablock.
    pub fn scaled(mut self, scale: f32) -> Self {
        if scale.is_finite() && scale > 0.0 && scale != 1.0 {
            self.width *= scale;
            self.stand_height *= scale;
            self.crouch_height *= scale;
            self.stand_eye *= scale;
            self.crouch_eye *= scale;
            self.eye_forward *= scale;
            self.sit_eye *= scale;
            self.sit_eye_forward *= scale;
            if let Some(prone) = &mut self.prone {
                prone.height *= scale;
                prone.eye *= scale;
            }
            if let Some(lean) = &mut self.lean {
                lean.distance *= scale;
            }
        }
        self
    }
    /// The tuning a body lying `prone` moves by: its crouch box, eye and
    /// speeds become the prone ones. Unchanged standing or without them.
    pub fn posed(&self, prone: bool) -> Self {
        let mut posed = self.clone();
        if prone && let Some(p) = self.prone {
            posed.crouch_height = p.height;
            posed.crouch_eye = p.eye;
            posed.crouch_forward = p.forward;
            posed.crouch_backward = p.backward;
            posed.crouch_sideways = p.sideways;
        }
        posed
    }
    fn height(&self, crouched: bool) -> f32 {
        if crouched {
            self.crouch_height
        } else {
            self.stand_height
        }
    }
    fn shape(&self, crouched: bool) -> SharedShape {
        match self.body {
            Body::Box => SharedShape::cuboid(
                self.width * 0.5,
                self.height(crouched) * 0.5,
                self.width * 0.5,
            ),
            Body::Ball => SharedShape::ball(self.width * 0.5),
            Body::Cylinder => SharedShape::cylinder(self.height(crouched) * 0.5, self.width * 0.5),
        }
    }
    fn pose(&self, feet: Vec3, crouched: bool) -> Pose {
        Pose::translation(feet.x, feet.y + self.height(crouched) * 0.5, feet.z)
    }
    pub fn validate(&self) -> Result<()> {
        let values = [
            self.width,
            self.stand_height,
            self.crouch_height,
            self.stand_eye,
            self.crouch_eye,
            self.sit_eye,
            self.density,
            self.swim_acceleration,
            self.swim_rise,
            self.dive_acceleration,
            self.gravity,
            self.jet_acceleration,
            self.jet_lift,
            self.horizontal_max_speed,
            self.horizontal_resist_speed,
            self.up_max_speed,
            self.up_resist_speed,
        ];
        ensure!(
            values
                .iter()
                .all(|n| n.is_finite() && *n > 0.0 && *n <= 1000.0)
                && self.crouch_height <= self.stand_height
                && [self.max_energy, self.recharge, self.min_jet_energy, self.jet_drain]
                    .iter()
                    .all(|n| n.is_finite() && (0.0..=10000.0).contains(n))
                // Speeds and the step may be zero (`BallShootPlayer`), and
                // so may running, steering and the surfaces a body can run
                // or jump on (Slayer's frozen countdown body).
                && [
                    self.acceleration,
                    self.air_control,
                    self.slope_degrees,
                    self.jump_surface_degrees,
                    self.forward,
                    self.backward,
                    self.sideways,
                    self.underwater_forward,
                    self.underwater_backward,
                    self.underwater_sideways,
                    self.crouch_forward,
                    self.crouch_backward,
                    self.crouch_sideways,
                    self.step_height,
                    self.jump_speed,
                    // No air drag or speed resistance: a controller that
                    // does its own (Mario's air drag).
                    self.drag,
                    self.horizontal_resist_factor,
                    self.up_resist_factor,
                ]
                .iter()
                .all(|n| n.is_finite() && (0.0..=1000.0).contains(n))
                && self.slope_degrees < 90.0
                && self.jump_surface_degrees < 90.0
                && self.horizontal_resist_speed < self.horizontal_max_speed
                && self.up_resist_speed < self.up_max_speed
                && [self.eye_forward, self.sit_eye_forward]
                    .iter()
                    .all(|n| n.is_finite() && n.abs() <= 1000.0)
                && self.turn_rate.is_finite()
                && (0.0..=20.0).contains(&self.turn_rate)
                && (self.body != Body::Ball
                    || (self.stand_height == self.width && self.crouch_height == self.width)),
            "Invalid player tuning"
        );
        if let Some(p) = &self.prone {
            ensure!(
                self.body != Body::Ball
                    && p.height.is_finite()
                    && p.height > 0.0
                    && p.height <= self.crouch_height
                    && p.eye.is_finite()
                    && (0.0..=self.crouch_height).contains(&p.eye)
                    && p.seconds.is_finite()
                    && (0.0..=5.0).contains(&p.seconds)
                    && [p.forward, p.backward, p.sideways]
                        .iter()
                        .all(|n| n.is_finite() && (0.0..=1000.0).contains(n)),
                "Invalid prone: height above 0 and at most the crouch's, eye within the crouch, seconds 0 to 5, speeds 0 to 1000"
            );
        }
        if let Some(l) = &self.lean {
            ensure!(
                l.distance.is_finite()
                    && l.distance >= 0.0
                    && l.distance <= self.width * 0.5 + 1e-4
                    && l.angle.is_finite()
                    && (0.0..=0.6).contains(&l.angle)
                    && l.body_angle
                        .is_none_or(|a| a.is_finite() && (0.0..=0.6).contains(&a))
                    && l.seconds.is_finite()
                    && (0.05..=2.0).contains(&l.seconds),
                "Invalid lean: distance 0 to half the body's width, angle 0 to 0.6, seconds 0.05 to 2"
            );
        }
        if let Some(c) = &self.climb {
            c.validate()?;
        }
        if let Some(u) = &self.unstick {
            u.validate()?;
        }
        if let Some(w) = &self.wall_step {
            w.validate()?;
        }
        if let Some(m) = &self.momentum {
            m.validate()?;
        }
        Ok(())
    }
}
pub struct Player {
    state: PlayerState,
    /// The tuning it moves by: `base` as its posture makes it
    /// ([`PlayerTuning::posed`]).
    tuning: PlayerTuning,
    /// Its archetype's tuning at its scale.
    base: PlayerTuning,
    body: RigidBodyHandle,
    collider: ColliderHandle,
    contacts: BTreeSet<BrickId>,
    /// Mounts keep their body at the feet, turned to their heading, so
    /// seats and weapons ride on its transform. Players centre it unturned.
    mount: bool,
    /// v20's crouch thread: the eye follows it, not the crouch flag.
    crouch: crate::crouch::CrouchThread,
    /// Held in the `sit` pose (`/sit`, or a seat that poses its rider so):
    /// the eye is the sit's. Set by whoever seats the body.
    seated: bool,
    mounted_eye_offset: Option<Vec3>,
    /// Jumps the body made without travelling between: server teleports
    /// and openings that carried it to their partner ([`Player::relocations`]).
    relocations: u64,
    /// The body is a corpse ([`Player::set_corpse`]).
    corpse: bool,
    /// Seconds the body has been inside solid geometry ([`Unstick`]).
    embedded_for: f32,
}
pub struct MotionEvents {
    /// A 32 ms Torque tick ran this step; the other events come only with one.
    pub ticked: bool,
    pub jumped: bool,
    pub landed: bool,
    pub touched: Vec<BrickId>,
    /// Velocity removed by collision this tick (Torque `onImpact` vector).
    pub impact: Vec3,
    /// Each collision's collider and the speed into its surface before the
    /// collision stopped it (Torque `Player::updatePos` `bd`).
    pub hits: Vec<(ColliderHandle, f32)>,
    pub contacts: Vec<crate::torque::SweepContact>,
    /// The body went through an opening this tick: the carry that took it
    /// to the partner's side (a player's view turns with it).
    pub passed: Option<glam::Affine3A>,
    /// The archetype's movement controller failed this tick (the motor ran
    /// the tick without it): its id and why.
    pub controller_fault: Option<String>,
}
/// Feet carried through an opening by their body's middle (`middle` above
/// them), so the body stays upright whichever way the opening turns it.
/// Half the height of the standard player at `scale` standing: a middle
/// good enough to tell which openings a replicated body went through.
pub fn nominal_middle(scale: f32) -> f32 {
    1.325 * scale
}
pub fn carry_feet(carry: &glam::Affine3A, feet: Vec3, middle: f32) -> Vec3 {
    carry.transform_point3(feet + Vec3::Y * middle) - Vec3::Y * middle
}
impl Player {
    /// Half the body's current height: where its middle is above the feet.
    pub fn middle(&self) -> f32 {
        self.tuning.height(self.state.crouched) * 0.5
    }
    /// Feet are chosen by the server's map spawn service.
    pub fn spawn(
        physics: &mut PhysicsWorld,
        owner: OwnerId,
        feet: Vec3,
        tuning: PlayerTuning,
    ) -> Result<Self> {
        Self::spawn_tagged(
            physics,
            owner,
            (1_u128 << 64) | u128::from(owner),
            feet,
            tuning,
        )
    }
    /// A character body for something other than a player (package
    /// entities): the same motor and shape, with the caller's collider tag so
    /// queries can tell it from players.
    pub fn spawn_tagged(
        physics: &mut PhysicsWorld,
        owner: OwnerId,
        tag: u128,
        feet: Vec3,
        tuning: PlayerTuning,
    ) -> Result<Self> {
        tuning.validate()?;
        ensure!(
            Self::clear(physics, feet, &tuning),
            "Player spawn is obstructed"
        );
        Self::insert_body(physics, owner, tag, feet, tuning)
    }
    /// Whether a standing body fits at `feet` without touching anything.
    pub fn clear(physics: &PhysicsWorld, feet: Vec3, tuning: &PlayerTuning) -> bool {
        feet.is_finite()
            && physics
                .query_pipeline_with_filter(
                    QueryFilter::default()
                        .exclude_sensors()
                        .groups(MOVES_AGAINST),
                )
                .intersect_shape(tuning.pose(feet, false), tuning.shape(false).as_ref())
                .next()
                .is_none()
    }
    /// A player body at `feet` even where it overlaps something, as v20's
    /// `spawnPlayer` does (and a respawn here does): used when every spawn
    /// point is built over, so the server never locks newcomers out.
    pub fn spawn_overlapping(
        physics: &mut PhysicsWorld,
        owner: OwnerId,
        feet: Vec3,
        tuning: PlayerTuning,
    ) -> Result<Self> {
        Self::insert_body(
            physics,
            owner,
            (1_u128 << 64) | u128::from(owner),
            feet,
            tuning,
        )
    }
    fn insert_body(
        physics: &mut PhysicsWorld,
        owner: OwnerId,
        tag: u128,
        feet: Vec3,
        tuning: PlayerTuning,
    ) -> Result<Self> {
        tuning.validate()?;
        ensure!(
            owner > 0 && feet.is_finite() && feet.abs().max_element() <= MAX_FEET,
            "Invalid player spawn"
        );
        let pose = tuning.pose(feet, false);
        let shape = tuning.shape(false);
        let (body, collider) = physics.insert(
            RigidBodyBuilder::kinematic_position_based()
                .pose(pose)
                .can_sleep(false),
            ColliderBuilder::new(shape).user_data(tag),
        );
        bri_physics::detect_collisions(physics);
        Ok(Self {
            state: PlayerState {
                owner,
                feet: feet.to_array(),
                velocity: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
                head_yaw: 0.0,
                grounded: false,
                crouched: false,
                jetting: false,
                jump: Default::default(),
                archetype: Default::default(),
                scale: 1.0,
                energy: tuning.max_energy,
                speed_scale: 1.0,
                walking: false,
                tick: TorqueTick::default(),
                tether: None,
                controller: None,
                pass_group: 0,
                posture: Posture::default(),
            },
            base: tuning.clone(),
            tuning,
            body,
            collider,
            contacts: BTreeSet::new(),
            mount: false,
            crouch: Default::default(),
            seated: false,
            mounted_eye_offset: None,
            relocations: 0,
            corpse: false,
            embedded_for: 0.0,
        })
    }
    /// Drive a player-type mount (horse, rowboat, cannon, turret) with the
    /// motor. Its kinematic body sits at the feet, turned to `yaw`, with its
    /// box collider raised by half its height.
    pub fn adopt(
        body: RigidBodyHandle,
        collider: ColliderHandle,
        feet: Vec3,
        yaw: f32,
        tuning: PlayerTuning,
    ) -> Result<Self> {
        tuning.validate()?;
        ensure!(
            feet.is_finite() && feet.abs().max_element() <= 1_000_000.0 && yaw.is_finite(),
            "Invalid mount pose"
        );
        Ok(Self {
            state: PlayerState {
                owner: 1,
                feet: feet.to_array(),
                velocity: [0.0; 3],
                yaw,
                pitch: 0.0,
                head_yaw: 0.0,
                grounded: false,
                crouched: false,
                jetting: false,
                jump: Default::default(),
                archetype: Default::default(),
                scale: 1.0,
                energy: tuning.max_energy,
                speed_scale: 1.0,
                walking: false,
                tick: TorqueTick::default(),
                tether: None,
                controller: None,
                pass_group: 0,
                posture: Posture::default(),
            },
            base: tuning.clone(),
            tuning,
            body,
            collider,
            contacts: BTreeSet::new(),
            mount: true,
            crouch: Default::default(),
            seated: false,
            mounted_eye_offset: None,
            relocations: 0,
            corpse: false,
            embedded_for: 0.0,
        })
    }
    /// Restored or scripted motion (`setVelocity`, checkpoints).
    pub fn set_motion(&mut self, velocity: Vec3, grounded: bool) {
        if velocity.is_finite() {
            self.state.velocity = velocity.clamp_length_max(MAX_SPEED).to_array();
            self.state.grounded = grounded;
            if grounded {
                self.state.jump.jet_flight = false;
            }
        }
    }
    pub fn body(&self) -> RigidBodyHandle {
        self.body
    }
    pub fn collider(&self) -> ColliderHandle {
        self.collider
    }
    /// The kinematic target for these feet: a player's box centre, or a
    /// mount's feet turned to its heading.
    fn body_pose(&self, feet: Vec3, crouched: bool) -> Pose {
        if self.mount {
            Pose::from_parts(
                Vector::from_array(feet.to_array()),
                glam::Quat::from_rotation_y(-self.state.yaw),
            )
        } else {
            self.tuning.pose(feet, crouched)
        }
    }
    /// Mirror an existing authoritative player (client prediction). Unlike
    /// `spawn`, the server already validated this position.
    /// `tuning` is the state's archetype at its scale.
    pub fn attach(
        physics: &mut PhysicsWorld,
        state: PlayerState,
        tuning: PlayerTuning,
    ) -> Result<Self> {
        tuning.validate()?;
        ensure!(state.owner > 0, "Invalid player owner");
        let posed = tuning.posed(state.posture.prone);
        let pose = posed.pose(Vec3::from(state.feet), state.crouched);
        let (body, collider) = physics.insert(
            RigidBodyBuilder::kinematic_position_based()
                .pose(pose)
                .can_sleep(false),
            ColliderBuilder::new(posed.shape(state.crouched))
                .user_data((1_u128 << 64) | u128::from(state.owner)),
        );
        let mut player = Self {
            state: state.clone(),
            tuning: tuning.posed(state.posture.prone),
            base: tuning.clone(),
            body,
            collider,
            contacts: BTreeSet::new(),
            mount: false,
            crouch: Default::default(),
            seated: false,
            mounted_eye_offset: None,
            relocations: 0,
            corpse: false,
            embedded_for: 0.0,
        };
        player.restore(physics, state, tuning)?;
        player.apply_groups(physics);
        Ok(player)
    }
    pub fn state(&self) -> &PlayerState {
        &self.state
    }
    pub fn tuning(&self) -> &PlayerTuning {
        &self.tuning
    }
    /// A new body starts with a full energy bar.
    pub fn refill_energy(&mut self) {
        self.state.energy = self.tuning.max_energy;
    }
    /// The share of its speeds the body moves at ([`PlayerState::speed_scale`]),
    /// 0 to 4.
    pub fn set_speed_scale(&mut self, scale: f32) -> Result<()> {
        ensure!(
            scale.is_finite() && (0.0..=MAX_SPEED_SCALE).contains(&scale),
            "Invalid speed scale: 0 to {MAX_SPEED_SCALE}"
        );
        self.state.speed_scale = scale;
        Ok(())
    }
    /// Whether the body walks ([`PlayerState::walking`]).
    pub fn set_walking(&mut self, walking: bool) {
        self.state.walking = walking;
    }
    /// `setDataBlock`/`setScale`: new motor constants and body. Growing the
    /// body may overlap geometry; like Torque, the motor resolves it by moving.
    /// `tuning` is the archetype's, at scale 1.
    pub fn set_archetype(
        &mut self,
        physics: &mut PhysicsWorld,
        archetype: crate::archetype::ArchetypeId,
        tuning: PlayerTuning,
        scale: f32,
    ) -> Result<()> {
        ensure!(
            scale.is_finite() && (0.1..=10.0).contains(&scale),
            "Invalid player scale"
        );
        let tuning = tuning.scaled(scale);
        tuning.validate()?;
        // A new body's controller starts afresh, never reading what
        // another controller left.
        if tuning.controller != self.tuning.controller {
            self.state.controller = None;
        }
        self.state.archetype = archetype;
        self.state.scale = scale;
        self.state.energy = self.state.energy.min(tuning.max_energy);
        // A body that cannot lie down or lean stands up straight.
        if tuning.prone.is_none() {
            self.state.posture.prone = false;
        }
        if tuning.lean.is_none() {
            self.state.posture.lean = 0.0;
            self.state.posture.lean_to = 0.0;
        }
        self.tuning = tuning.posed(self.state.posture.prone);
        self.base = tuning;
        physics.colliders[self.collider].set_shape(self.tuning.shape(self.state.crouched));
        self.synchronize_pose(physics);
        Ok(())
    }
    /// Whether this player, where it stands and as crouched as it is, has
    /// room to be `tuning` (an archetype at its scale): a body no wider and
    /// no taller than this one always has; a bigger one must not reach into
    /// anything that never moves (bricks, the map). Players and vehicles
    /// are left out, as they move aside.
    pub fn room_for(&self, physics: &PhysicsWorld, tuning: &PlayerTuning) -> bool {
        let crouched = self.state.crouched;
        if tuning.width <= self.tuning.width
            && tuning.height(crouched) <= self.tuning.height(crouched)
        {
            return true;
        }
        // The motor keeps feet a hair into the floor; test just above it.
        let feet = Vec3::from(self.state.feet) + Vec3::Y * ROOM_LIFT;
        let fixed = |_: ColliderHandle, c: &Collider| {
            c.parent()
                .is_none_or(|b| physics.bodies.get(b).is_some_and(|b| b.is_fixed()))
        };
        physics
            .query_pipeline_with_filter(
                QueryFilter::default()
                    .exclude_sensors()
                    .exclude_collider(self.collider)
                    .groups(moves_against(self.state.pass_group))
                    .predicate(&fixed),
            )
            .intersect_shape(tuning.pose(feet, crouched), tuning.shape(crouched).as_ref())
            .next()
            .is_none()
    }
    /// Authoritative contact box, including current crouch dimensions. This is
    /// the same pose/shape used by the player motor, not a client pickup radius.
    pub fn world_bounds(&self) -> ([f32; 3], [f32; 3]) {
        let half = Vec3::new(self.tuning.width * 0.5, 0., self.tuning.width * 0.5);
        let feet = Vec3::from(self.state.feet);
        (
            (feet - half).to_array(),
            (feet + half + Vec3::Y * self.tuning.height(self.state.crouched)).to_array(),
        )
    }
    /// Restore a trusted authoritative correction, including jump-edge state.
    /// `tuning` is the state's archetype at its scale.
    pub fn restore(
        &mut self,
        physics: &mut PhysicsWorld,
        state: PlayerState,
        tuning: PlayerTuning,
    ) -> Result<()> {
        // Keep the rejection at its existing boundary, but identify the
        // authoritative field so a disconnect report can locate its producer.
        ensure!(
            state.owner == self.state.owner,
            "Invalid authoritative player correction: owner {} differs from {}",
            state.owner,
            self.state.owner
        );
        state.check_bounds()?;
        // The collider's shape follows the tuning and the crouch flag (the
        // motor swaps it as the flag changes), so it is replaced only when
        // one of them did: a new shape marks the collider modified and the
        // broad and narrow phases redo its pairs on the next step.
        let reshape = tuning != self.base
            || state.crouched != self.state.crouched
            || state.posture.prone != self.state.posture.prone;
        if tuning != self.base {
            tuning.validate()?;
            self.base = tuning;
        }
        if reshape {
            self.tuning = self.base.posed(state.posture.prone);
        }
        if reshape {
            physics.colliders[self.collider].set_shape(self.tuning.shape(state.crouched));
        }
        let regroup = state.pass_group != self.state.pass_group;
        self.state = state;
        if regroup {
            self.apply_groups(physics);
        }
        self.contacts.clear();
        self.synchronize_pose(physics);
        Ok(())
    }
    /// Target this kinematic body at the current state. Like the motor, only
    /// the next kinematic pose is set: the physics step moves the body, which
    /// keeps Rapier's island bookkeeping consistent across teleports.
    pub fn synchronize_pose(&self, physics: &mut PhysicsWorld) {
        let pose = self.body_pose(Vec3::from(self.state.feet), self.state.crouched);
        physics.bodies[self.body].set_next_kinematic_position(pose);
    }
    /// Put the body at the current state at once, for a collision world that
    /// is queried but never stepped (prediction's copies of other players).
    pub fn place_now(&self, physics: &mut PhysicsWorld) {
        let pose = self.body_pose(Vec3::from(self.state.feet), self.state.crouched);
        physics.bodies[self.body].set_position(pose, true);
    }
    /// The eye on the crouch thread, the height the camera shows, and the
    /// `Eye` node's lead ahead of the body (`getEyePoint`).
    pub fn eye(&self) -> Vec3 {
        if let Some(offset) = self.mounted_eye_offset {
            return Vec3::from(self.state.feet) + offset;
        }
        if self.seated {
            return self.state.posed_eye(&self.tuning, true);
        }
        let fraction = self.crouch.eye_fraction(crate::crouch::CROUCH_SECONDS);
        Vec3::from(self.state.feet)
            + Vec3::Y * self.tuning.eye_height(fraction)
            + self.state.eye_ahead(&self.tuning)
            + self.state.lean_offset(&self.tuning)
    }
    /// Lie down or get up, for an Add-On's rules (`set_prone`). Lying down
    /// takes the prone box at once (no taller than the crouch's) and keeps
    /// the body crouched; getting up needs room for the crouch box, and
    /// the crouch key or the room above then decides whether it stands.
    /// False when it cannot: no prone for this body, or no room to rise.
    pub fn set_prone(&mut self, physics: &mut PhysicsWorld, prone: bool) -> bool {
        if prone == self.state.posture.prone {
            return true;
        }
        if prone {
            if self.base.prone.is_none() || self.mount {
                return false;
            }
            self.state.crouched = true;
        } else {
            let feet = Vec3::from(self.state.feet) + Vec3::Y * ROOM_LIFT;
            let shape = self.base.shape(true);
            let blocked = physics
                .query_pipeline_with_filter(
                    QueryFilter::default()
                        .exclude_sensors()
                        .exclude_rigid_body(self.body)
                        .groups(moves_against(self.state.pass_group)),
                )
                .intersect_shape(self.base.pose(feet, true), shape.as_ref())
                .next()
                .is_some();
            if blocked {
                return false;
            }
        }
        self.state.posture.prone = prone;
        self.tuning = self.base.posed(prone);
        physics.colliders[self.collider].set_shape(self.tuning.shape(self.state.crouched));
        self.synchronize_pose(physics);
        true
    }
    /// Lean toward `to`, -1 (left) to 1 (right), 0 upright, for an
    /// Add-On's rules (`set_lean`); the motor eases there. False for a body
    /// that cannot lean.
    pub fn set_lean(&mut self, to: f32) -> bool {
        if self.base.lean.is_none() || !to.is_finite() {
            return false;
        }
        self.state.posture.lean_to = to.clamped(-1.0, 1.0);
        true
    }
    /// Whether the body is held in the `sit` pose, which moves the eye.
    pub fn set_seated(&mut self, seated: bool) {
        self.seated = seated;
        if !seated {
            self.mounted_eye_offset = None;
        }
    }
    /// Derived by the mount owner each tick; every interaction uses eye().
    /// Store an offset so relocating the body cannot leave its eye behind.
    pub fn set_mounted_eye(&mut self, eye: Option<Vec3>) {
        self.mounted_eye_offset = eye
            .filter(|p| p.is_finite())
            .map(|p| p - Vec3::from(self.state.feet))
            .filter(|p| p.is_finite());
    }
    pub fn seated(&self) -> bool {
        self.seated
    }
    /// Corpses stop blocking players and weapons; respawn makes them solid.
    pub fn set_solid(&self, physics: &mut PhysicsWorld, solid: bool) {
        physics.colliders[self.collider].set_sensor(!solid);
    }
    /// A dead body stops blocking players (and being blocked by them) at
    /// once, but stays solid to everything else until it is cleared.
    pub fn set_corpse(&mut self, physics: &mut PhysicsWorld, corpse: bool) {
        self.corpse = corpse;
        self.apply_groups(physics);
    }
    /// Put the body in pass group `group` (0 for none, up to
    /// [`MAX_PASS_GROUP`]): it moves through the bodies and vehicles of the
    /// same group.
    pub fn set_pass_group(&mut self, physics: &mut PhysicsWorld, group: u8) -> Result<()> {
        ensure!(
            group <= MAX_PASS_GROUP,
            "Invalid pass group: 0 to {MAX_PASS_GROUP}"
        );
        self.state.pass_group = group;
        self.apply_groups(physics);
        Ok(())
    }
    /// The collider's groups as the corpse flag and pass group make them.
    fn apply_groups(&self, physics: &mut PhysicsWorld) {
        let groups = if self.corpse {
            InteractionGroups::new(CORPSE, Group::ALL, InteractionTestMode::And)
        } else {
            pass_groups(self.state.pass_group)
        };
        physics.colliders[self.collider].set_collision_groups(groups);
    }
    /// Server relocation (spawn/respawn/teleport): clears motion state.
    pub fn teleport(&mut self, physics: &mut PhysicsWorld, feet: Vec3, yaw: f32) -> Result<()> {
        ensure!(
            feet.is_finite() && feet.abs().max_element() <= MAX_FEET && yaw.is_finite(),
            "Invalid teleport"
        );
        let mut state = self.state.clone();
        state.feet = feet.to_array();
        state.velocity = [0.0; 3];
        // Spawn bricks and other server teleports can supply a full-turn
        // heading. Match the canonical input/weapon range without changing facing.
        state.yaw =
            (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        state.pitch = 0.0;
        state.grounded = false;
        state.crouched = false;
        state.jetting = false;
        state.jump.jet_flight = false;
        state.tether = None;
        self.restore(physics, state, self.tuning.clone())?;
        // A relocation is a jump, not travel: the body is there at once.
        // Left as the next kinematic pose, the step would sweep the body
        // from where it was at the jump's speed (a respawn across the map
        // is thousands of units a second), and the contact solver would
        // hand that speed to whatever it was touching: a corpse lying on a
        // parked jeep threw the jeep off the map as it respawned.
        let pose = self.body_pose(feet, false);
        physics.bodies[self.body].set_position(pose, true);
        self.relocations += 1;
        Ok(())
    }
    /// How many times the body jumped instead of travelling: each server
    /// teleport and each opening it passed through counts one. Whoever
    /// follows its path between two looks (swept region tests) treats a
    /// change as a jump to the new place, never as the line between.
    /// Riding a seat ([`Player::place`]) and authoritative corrections
    /// ([`Player::restore`]) are travel and do not count.
    pub fn relocations(&self) -> u64 {
        self.relocations
    }
    /// Ride a vehicle seat: position and facing come from the seat node,
    /// and motion from the vehicle, within what a player may hold (a rider
    /// of something faster than [`MAX_SPEED`] moves at it).
    pub fn place(&mut self, physics: &mut PhysicsWorld, feet: Vec3, yaw: f32, velocity: Vec3) {
        if !feet.is_finite() || !yaw.is_finite() || !velocity.is_finite() {
            return;
        }
        self.state.feet = feet.to_array();
        self.state.velocity = velocity.to_array();
        self.state.keep_in_bounds();
        self.state.yaw =
            (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        self.state.grounded = true;
        self.state.crouched = false;
        self.state.jetting = false;
        self.state.jump.jet_flight = false;
        self.state.tether = None;
        self.synchronize_pose(physics);
    }
    /// Tie the body to a point with a rope, or cut it (`None`).
    pub fn set_tether(&mut self, tether: Option<Tether>) -> Result<()> {
        if let Some(t) = &tether {
            t.validate()?;
        }
        self.state.tether = tether;
        Ok(())
    }
    /// A seated rider's look. `Player::updateMove` still turns `mHead` while
    /// mounted, so other players see the rider's head and arms follow the
    /// mouse pitch and free-look turn in the seat.
    pub fn look(&mut self, input: &MoveInput) {
        if input.validate().is_ok() {
            self.state.pitch = input.pitch;
            self.state.head_yaw = input.head_yaw;
        }
    }
    /// Scale the velocity: the level part by `horizontal`, the vertical part
    /// by `vertical`.
    pub fn scale_velocity(&mut self, horizontal: f32, vertical: f32) {
        if horizontal.is_finite() && vertical.is_finite() {
            let v = self.state.velocity;
            self.state.velocity = [v[0] * horizontal, v[1] * vertical, v[2] * horizontal];
        }
    }
    /// Add an impulse-derived velocity change (weapon knockback).
    pub fn push(&mut self, delta: Vec3) {
        if delta.is_finite() {
            let v = (Vec3::from(self.state.velocity) + delta).clamp_length_max(200.0);
            self.state.velocity = v.to_array();
            if delta.y > 0.0 {
                self.state.grounded = false;
            }
        }
    }
    /// A server tick in which this player's motor does not run (waiting for
    /// its next input). The kinematic body still receives its target pose so
    /// the physics step treats it like any other active kinematic body.
    pub fn hold(&self, physics: &mut PhysicsWorld) {
        physics.bodies[self.body].set_next_kinematic_position(
            self.body_pose(Vec3::from(self.state.feet), self.state.crouched),
        );
    }
    pub fn despawn(self, physics: &mut PhysicsWorld) {
        physics.remove_body(self.body);
        bri_physics::detect_collisions(physics);
    }
    /// One call per server tick, before PhysicsWorld::step. No variable client dt.
    pub fn step(&mut self, physics: &mut PhysicsWorld, input: MoveInput) -> Result<MotionEvents> {
        self.step_in_water(physics, input, &[])
    }
    /// The host supplies validated native map liquids; no client-supplied
    /// coverage or arbitrary force is accepted. Dry prediction uses `step`.
    pub fn step_in_water(
        &mut self,
        physics: &mut PhysicsWorld,
        input: MoveInput,
        waters: &[bri_content::water::Water],
    ) -> Result<MotionEvents> {
        self.step_among(physics, input, waters, &())
    }
    /// `step_in_water` in a world whose merged colliders `parts` names
    /// (the bricks of a chunk collider).
    pub fn step_among(
        &mut self,
        physics: &mut PhysicsWorld,
        input: MoveInput,
        waters: &[bri_content::water::Water],
        parts: &dyn crate::torque::PartTags,
    ) -> Result<MotionEvents> {
        self.step_through(physics, input, waters, parts, &Passages::default())
    }
    /// `step_among` in a world with openings bodies pass through: a body
    /// whose middle goes in through one comes out of its partner, turned
    /// and still moving (`MotionEvents::passed`).
    pub fn step_through(
        &mut self,
        physics: &mut PhysicsWorld,
        input: MoveInput,
        waters: &[bri_content::water::Water],
        parts: &dyn crate::torque::PartTags,
        passages: &Passages,
    ) -> Result<MotionEvents> {
        input.validate()?;
        let tick = &mut self.state.tick;
        // Anything that moved the feet (teleports, seats, older states)
        // starts drawing from there.
        if tick.feet != self.state.feet {
            tick.feet = self.state.feet;
            tick.from = self.state.feet;
        }
        tick.jump |= input.jump;
        tick.phase += STEP_PARTS;
        // Looking turns the body every step; moving waits for the tick.
        self.state.pitch = input.pitch;
        self.state.head_yaw = input.head_yaw;
        if self.tuning.steering == Steering::Strafe {
            self.state.yaw = input.yaw;
        }
        let events = if tick.phase >= TICK_PARTS {
            tick.phase -= TICK_PARTS;
            let input = MoveInput {
                jump: std::mem::take(&mut tick.jump),
                ..input
            };
            let before = self.state.feet;
            let events =
                self.torque_tick_among(physics, input, waters, TORQUE_TICK, parts, passages)?;
            // Drawn between the ticks on the side it came out of.
            let before = match &events.passed {
                Some(carry) => {
                    self.relocations += 1;
                    carry_feet(carry, Vec3::from(before), self.middle()).to_array()
                }
                None => before,
            };
            let tick = &mut self.state.tick;
            tick.from = before;
            tick.feet = self.state.feet;
            events
        } else {
            MotionEvents {
                ticked: false,
                jumped: false,
                landed: false,
                touched: vec![],
                impact: Vec3::ZERO,
                hits: vec![],
                contacts: vec![],
                passed: None,
                controller_fault: None,
            }
        };
        self.crouch.update(
            self.state.crouched,
            bri_physics::FIXED_DT,
            crate::crouch::CROUCH_SECONDS,
        );
        self.synchronize_pose(physics);
        Ok(events)
    }
    /// One v20 `updateMove` and `updatePos` of `dt` seconds from the last
    /// tick's feet. The motor always runs 32 ms ticks; tests may run others.
    #[doc(hidden)]
    pub fn torque_tick(
        &mut self,
        physics: &mut PhysicsWorld,
        input: MoveInput,
        waters: &[bri_content::water::Water],
        dt: f32,
    ) -> Result<MotionEvents> {
        self.torque_tick_among(physics, input, waters, dt, &(), &Passages::default())
    }
    /// [`PlayerTuning::unstick`]: counts the time the body spends inside
    /// solid geometry and, past the limit, returns the nearest free spot
    /// to move it to. Only what never gets out of the way counts (players
    /// do), and the body is tested a hair above its feet, clear of the floor
    /// the motor rests a hair inside.
    fn unstick(
        &mut self,
        physics: &PhysicsWorld,
        filter: QueryFilter<'_>,
        unstick: Unstick,
        dt: f32,
    ) -> Option<Vec3> {
        let solid = |_: ColliderHandle, c: &Collider| c.user_data >> 64 != 1;
        let query = physics.query_pipeline_with_filter(filter.predicate(&solid));
        let (pose_at, shape) = (
            |at: Vec3| {
                self.tuning
                    .pose(at + Vec3::Y * ROOM_LIFT, self.state.crouched)
            },
            self.tuning.shape(self.state.crouched),
        );
        let free = |at: Vec3| {
            query
                .intersect_shape(pose_at(at), shape.as_ref())
                .next()
                .is_none()
        };
        let feet = Vec3::from(self.state.feet);
        if free(feet) {
            self.embedded_for = 0.0;
            return None;
        }
        self.embedded_for += dt;
        if self.embedded_for < unstick.seconds {
            return None;
        }
        // Tried again after another `seconds` when nothing nearby is free.
        self.embedded_for = 0.0;
        self.search_free(physics, filter, unstick, feet)
    }
    /// The nearest spot to `feet` where the body fits, by the [`Unstick`]
    /// search: straight up, the vote of four probes, then rings outward and
    /// upward.
    fn search_free(
        &self,
        physics: &PhysicsWorld,
        filter: QueryFilter<'_>,
        unstick: Unstick,
        feet: Vec3,
    ) -> Option<Vec3> {
        let solid = |_: ColliderHandle, c: &Collider| c.user_data >> 64 != 1;
        let query = physics.query_pipeline_with_filter(filter.predicate(&solid));
        let free = |at: Vec3| {
            query
                .intersect_shape(
                    self.tuning
                        .pose(at + Vec3::Y * ROOM_LIFT, self.state.crouched),
                    self.tuning.shape(self.state.crouched).as_ref(),
                )
                .next()
                .is_none()
        };
        let lifts = (unstick.reach / unstick.step).floor() as u32;
        let rise = |k: u32| feet + Vec3::Y * (k as f32 * unstick.step);
        if let Some(spot) = (1..=lifts).map(rise).find(|spot| free(*spot)) {
            return Some(spot);
        }
        // The vote: thin slices around the body at its middle.
        let radius = self.tuning.width * 0.5;
        let slice = SharedShape::cuboid(radius * 0.5, ROOM_LIFT, radius * 0.5);
        let middle = Vec3::Y * (self.tuning.height(self.state.crouched) * 0.5);
        let mut moved = Vec3::ZERO;
        let mut votes = 0;
        for (x, z) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            if votes >= 2 {
                break;
            }
            let offset = Vec3::new(x, 0.0, z) * (radius * unstick.probe);
            let at = feet + middle + offset;
            if query
                .intersect_shape(Pose::translation(at.x, at.y, at.z), slice.as_ref())
                .next()
                .is_none()
            {
                votes += 1;
                moved += offset;
            }
        }
        if votes > 0 {
            let spot = feet + moved * if votes == 1 { 2.0 } else { 1.0 };
            if free(spot) {
                return Some(spot);
            }
        }
        const DIRECTIONS: u32 = 8;
        let mut spots: Vec<Vec3> = Vec::new();
        for up in 0..=lifts {
            for out in 1..=lifts {
                for k in 0..DIRECTIONS {
                    let angle = k as f32 / DIRECTIONS as f32 * std::f32::consts::TAU;
                    let away =
                        Vec3::new(angle.cos(), 0.0, angle.sin()) * (out as f32 * unstick.step);
                    spots.push(rise(up) + away);
                }
            }
        }
        spots.sort_by(|a, b| {
            a.distance_squared(feet)
                .total_cmp(&b.distance_squared(feet))
        });
        spots.into_iter().find(|spot| free(*spot))
    }
    /// Where a body set down at `feet` (a spawn) fits: `feet` itself when
    /// it is clear, else the nearest free spot by the [`Unstick`] search; a
    /// body with no `unstick` keeps `feet` whatever is there, as v20's
    /// `spawnPlayer` does.
    pub fn spot_near(&self, physics: &PhysicsWorld, feet: Vec3) -> Placement {
        let Some(unstick) = self.tuning.unstick else {
            return Placement::Clear(feet);
        };
        let filter = QueryFilter::default()
            .exclude_sensors()
            .exclude_rigid_body(self.body)
            .groups(moves_against(self.state.pass_group));
        let solid = |_: ColliderHandle, c: &Collider| c.user_data >> 64 != 1;
        let blocker = physics
            .query_pipeline_with_filter(filter.predicate(&solid))
            .intersect_shape(
                self.tuning
                    .pose(feet + Vec3::Y * ROOM_LIFT, self.state.crouched),
                self.tuning.shape(self.state.crouched).as_ref(),
            )
            .next()
            .map(|(handle, c)| {
                let aabb = c.compute_aabb();
                Blocker {
                    collider: handle.into_raw_parts().0,
                    tag: c.user_data,
                    centre: Vec3::from_array((aabb.mins + aabb.maxs).to_array()) * 0.5,
                    size: Vec3::from_array((aabb.maxs - aabb.mins).to_array()),
                }
            });
        let Some(blocker) = blocker else {
            return Placement::Clear(feet);
        };
        match self.search_free(physics, filter, unstick, feet) {
            Some(to) => Placement::Moved { to, blocker },
            None => Placement::Stuck { blocker },
        }
    }
    fn torque_tick_among(
        &mut self,
        physics: &mut PhysicsWorld,
        input: MoveInput,
        waters: &[bri_content::water::Water],
        dt: f32,
        parts: &dyn crate::torque::PartTags,
        passages: &Passages,
    ) -> Result<MotionEvents> {
        input.validate()?;
        let t = &self.tuning;
        // `canJet` and `minJetEnergy` gate the jets; jetting drains energy and
        // `rechargeRate` refills it every tick.
        let jet = input.jet && t.can_jet && self.state.energy >= t.min_jet_energy;
        let input = MoveInput { jet, ..input };
        let drain = if jet { t.jet_drain } else { 0.0 };
        self.state.energy =
            (self.state.energy - drain * dt + t.recharge * dt).clamped(0.0, t.max_energy);
        // A lean eases toward where the rules set it.
        let posture = &mut self.state.posture;
        match t.lean {
            Some(lean) => {
                let most = dt / lean.seconds;
                posture.lean += (posture.lean_to - posture.lean).clamped(-most, most);
            }
            None => {
                *posture = Posture {
                    prone: posture.prone,
                    climbing: posture.climbing,
                    ..Posture::default()
                }
            }
        }
        let was_grounded = self.state.grounded;
        let was_crouched = self.state.crouched;
        let mut feet = Vec3::from(self.state.feet);
        let filter = QueryFilter::default()
            .exclude_sensors()
            .exclude_rigid_body(self.body)
            .groups(moves_against(self.state.pass_group));
        let relocated = match t.unstick {
            Some(unstick) => self.unstick(physics, filter, unstick, dt),
            None => None,
        };
        if let Some(spot) = relocated {
            feet = spot;
            self.state.feet = spot.to_array();
            self.state.velocity = [0.0; 3];
            self.state.grounded = false;
            self.relocations += 1;
        }
        let t = &self.tuning;
        let query = physics.query_pipeline_with_filter(filter);
        // v20 updateMove (0x5ae2ea) crouches a fully submerged player as if
        // crouch were held, so swimmers under water use the crouch box.
        let submerged =
            bri_content::water::submersion(waters, feet.to_array(), t.height(self.state.crouched))
                .is_some_and(|(_, coverage)| coverage >= 1.0);
        // Lying down holds the (prone) crouch box until the rules get it up.
        if input.crouch || submerged || self.state.posture.prone {
            self.state.crouched = true;
        } else if self.state.crouched {
            let standing = t.shape(false);
            if query
                .intersect_shape(t.pose(feet, false), standing.as_ref())
                .next()
                .is_none()
            {
                self.state.crouched = false;
            }
        }
        let steer = if t.steering == Steering::Turn {
            let turned = self.state.yaw + input.right * t.turn_rate * dt;
            // Keep within the input's range, so a turn body's state is
            // always a valid look direction.
            let yaw = (turned + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            MoveInput {
                yaw,
                right: 0.0,
                ..input
            }
        } else {
            input
        };
        let input = steer;
        self.state.yaw = input.yaw;
        self.state.pitch = input.pitch;
        self.state.head_yaw = input.head_yaw;
        self.state.jetting = input.jet;
        let forward = Vec3::new(input.yaw.sin(), 0.0, -input.yaw.cos());
        let right = Vec3::new(input.yaw.cos(), 0.0, input.yaw.sin());
        let liquid =
            bri_content::water::submersion(waters, feet.to_array(), t.height(self.state.crouched))
                .filter(|(_, coverage)| *coverage >= 0.1);
        let (fs, bs, ss) = if liquid.is_some_and(|(_, c)| c >= t.swim_coverage) {
            (
                t.underwater_forward,
                t.underwater_backward,
                t.underwater_sideways,
            )
        } else if self.state.crouched {
            (t.crouch_forward, t.crouch_backward, t.crouch_sideways)
        } else {
            (t.forward, t.backward, t.sideways)
        };
        // v20 Player::updateMove: the raw move vector runs at the larger of its
        // directional speeds.
        let move_vec = forward * input.forward + right * input.right;
        let move_speed = if input.forward > 0.0 {
            fs * input.forward
        } else {
            bs * -input.forward
        }
        .max(ss * input.right.abs())
            * self.state.speed_scale;
        // The archetype's own movement rules decide first; the motor then
        // moves the body as always (`crate::controller`).
        let mut controller_fault = None;
        let mut gravity = t.gravity;
        let input = match &t.controller {
            Some(controller) => {
                let mut tick = crate::controller::ControllerTick {
                    dt,
                    scale: self.state.scale,
                    feet,
                    facing: forward,
                    wish: move_vec,
                    pitch: input.pitch,
                    grounded: self.state.grounded,
                    crouched: self.state.crouched,
                    submerged: liquid.is_some_and(|(_, c)| c >= t.swim_coverage),
                    jump: input.jump,
                    crouch: input.crouch,
                    jet: input.jet,
                    attack: input.attack,
                    gravity: t.gravity,
                    velocity: Vec3::from(self.state.velocity),
                    take_jump: false,
                    skip_gravity: false,
                    state: self.state.controller.unwrap_or_default(),
                };
                let reach = crate::controller::MAX_SWEEP * self.state.scale.max(1.0);
                let height = t.height(self.state.crouched);
                let half = t.width * 0.5;
                let around = torque::Box3 {
                    min: Vec3::new(feet.x - half, feet.y, feet.z - half),
                    max: Vec3::new(feet.x + half, feet.y + height, feet.z + half),
                }
                .expanded(Vec3::splat(reach + 0.2));
                let mut probe = crate::controller::Probe::new(
                    torque::Soup::gather(&query, &physics.bodies, around, feet, parts),
                    feet,
                    half,
                    height,
                    reach,
                );
                match controller.0.step(&mut tick, &mut probe) {
                    Ok(()) if tick.velocity.is_finite() => {
                        self.state.velocity = tick.velocity.clamp_length_max(MAX_SPEED).to_array();
                        self.state.controller = (tick.state
                            != crate::controller::ControllerState::default())
                        .then_some(tick.state);
                        if tick.skip_gravity {
                            gravity = 0.0;
                        }
                        MoveInput {
                            jump: input.jump && !tick.take_jump,
                            ..input
                        }
                    }
                    Ok(()) => {
                        controller_fault = Some(format!(
                            "{}: a velocity that is not a number",
                            controller.id()
                        ));
                        input
                    }
                    Err(why) => {
                        controller_fault = Some(format!("{}: {why}", controller.id()));
                        input
                    }
                }
            }
            None => input,
        };
        // A ladder: the body takes hold of one it faces and pushes toward,
        // runs along it while it holds on, and lets go when it leaves the
        // volume, jumps or comes down onto a floor (`crate::climb`).
        let mut input = input;
        let mut climb_velocity = None;
        if let Some(climb) = t.climb {
            let middle = feet + Vec3::Y * (t.height(self.state.crouched) * 0.5);
            match crate::climb::find(physics, middle) {
                None => self.state.posture.climbing = false,
                Some(volume) => {
                    let mut let_go = false;
                    if self.state.posture.climbing && input.jump {
                        // A hop off the rungs.
                        self.state.posture.climbing = false;
                        self.state.velocity[1] = climb.jump_speed;
                        input.jump = false;
                        let_go = true;
                    }
                    if !self.state.posture.climbing && !let_go {
                        let rising = self.state.velocity[1].abs();
                        let wish = move_vec;
                        let facing = volume.facing();
                        let airborne = !self.state.grounded;
                        self.state.posture.climbing = !self.state.posture.prone
                            && (!airborne || rising <= climb.grab_speed)
                            && forward.dot(facing) > climb.facing
                            && (wish.length() < 1e-3
                                || airborne
                                || wish.normalize().dot(facing) > climb.push);
                    }
                    if self.state.posture.climbing {
                        self.state.yaw = volume.yaw;
                        let along = volume.up * (input.forward * volume.speed);
                        let across = if volume.strafe {
                            right * (input.right * volume.speed)
                        } else {
                            Vec3::ZERO
                        };
                        // Drawn toward the wall until it touches or the far
                        // side of the volume is reached.
                        let facing = volume.facing();
                        let pull = if climb.wall_pull > 0.0
                            && (middle - volume.center).dot(facing) < volume.depth - 0.05
                        {
                            facing * climb.wall_pull
                        } else {
                            Vec3::ZERO
                        };
                        climb_velocity = Some(along + across + pull);
                    }
                }
            }
        } else {
            self.state.posture.climbing = false;
        }
        // Torque's convex working list: every solid polygon this tick can reach,
        // including a jump, a step and the contact slab under the feet.
        let half = t.width * 0.5;
        let height = t.height(self.state.crouched);
        let step_reach = t.step_height * self.state.scale;
        let previous = Vec3::from(self.state.velocity);
        let body_box = |at: Vec3| torque::Box3 {
            min: Vec3::new(at.x - half, at.y, at.z - half),
            max: Vec3::new(at.x + half, at.y + height, at.z + half),
        };
        let reach = (previous.length() + 30.0) * dt + 0.2;
        let region = body_box(feet).expanded(Vec3::splat(reach) + Vec3::Y * (step_reach + 0.05));
        let mut soup = torque::Soup::gather(&query, &physics.bodies, region, feet, parts);
        let middle = Vec3::Y * height * 0.5;
        soup.open_passages(
            &query,
            &physics.bodies,
            passages,
            feet + middle,
            region,
            parts,
        );
        // A body that starts inside another player (two spawned on one
        // spot) is not walled in by that player's faces: sweeping against
        // them stopped every move dead, so neither ever left. Players
        // otherwise never overlap (each closes half its gap per tick), and
        // the push below parts the two.
        let inside = players_inside(&query, body_box(feet));
        if !inside.is_empty() {
            soup.polys
                .retain(|p| p.kind != torque::Kind::Actor || !inside.contains(&p.collider));
        }
        let run_cos = t.slope_degrees.to_radians().cos();
        let jump_cos = t.jump_surface_degrees.to_radians().cos();
        let contact = torque::find_contact(&soup, feet, half, run_cos, jump_cos);
        // Per-tick epsilons of the 32 ms Torque tick, at this motor's rate.
        let torque_ticks = dt / TORQUE_TICK;
        let mut velocity = previous;
        // v20 updateMove: gravity always applies; a run surface cancels the part
        // into it and the run force (runForce / mass per tick) steers toward the
        // move along the surface. Steeper than runSurfaceAngle is not a run
        // surface, so gravity slides the player down it.
        let mut acc = Vec3::new(0.0, -gravity * dt, 0.0);
        // The move a jump pushes along: air control rewrites v20's moveVec
        // in place before the jump reads it (0x5AF4B5).
        let mut jump_move = move_vec;
        if let (true, Some(normal)) = (contact.run, contact.normal) {
            let into = -acc.dot(normal);
            if into > 0.0 {
                acc += normal * (into + 0.002 * torque_ticks);
                // Blockland rests below 0.0021 (TGE: 0.0001), so level ground
                // cancels the 0.002 lift exactly.
                if acc.length() < 0.0021 * torque_ticks {
                    acc = Vec3::ZERO;
                }
            }
            let mut pv = move_vec;
            let mut pvl = pv.length();
            // Parallel to the surface, across the move; jets skip this in v20.
            if pvl > 0.0 && !input.jet {
                let across = pv.cross(Vec3::Y) / pvl;
                let cv = normal - across * across.dot(normal);
                pv -= cv * pv.dot(cv);
                pvl = pv.length();
            }
            if pvl > 0.0 {
                pv *= move_speed / pvl;
            }
            if let Some(momentum) = &t.momentum {
                let base = velocity + acc;
                let next = if pv == Vec3::ZERO {
                    momentum.brake(base, dt)
                } else {
                    momentum.drive(
                        base,
                        pv,
                        dt,
                        if self.state.crouched {
                            momentum.crouched_share
                        } else if self.state.walking {
                            momentum.walking_share
                        } else {
                            1.0
                        },
                    )
                };
                acc += next - base;
            } else {
                let run = pv - (velocity + acc);
                acc += run.clamp_length_max(t.acceleration * dt);
            }
            // `updatePos` leaves every hit moving `NORMAL_ELASTICITY` away
            // from the surface, which v20's run force takes off again on
            // the next tick. A body whose controller owns its running
            // (its archetype's `acceleration` is 0, as the Halo and Mario
            // controllers' are) has no run force: it kept that speed, rose
            // off the floor until it left the contact slab, and was no
            // longer grounded, so its controller dropped a jump. For such a
            // body the motor takes off what the run force leaves, and the
            // same small speed into the surface: a push that slow (a shot
            // from a little above) moves the body too little each tick for
            // the sweep to see the floor, so it sank through it. Larger
            // speeds hit the floor as usual. Bodies without a controller
            // move exactly as v20's.
            if t.controller.is_some() {
                let away = (velocity + acc).dot(normal);
                if away.abs() <= torque::NORMAL_ELASTICITY * 1.01 {
                    acc -= normal * away;
                }
            }
        } else if liquid.is_some() {
            // Swimming pushes along the move direction; water drag sets the speed.
            acc += move_vec.normalize_or_zero() * t.swim_acceleration * dt;
        } else if !input.jet
            && let Some(momentum) = &t.momentum
        {
            let horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
            // A body whose way one step on is blocked cannot steer against it.
            let ahead = momentum.air_reach(horizontal, move_vec, dt);
            let blocked = momentum.air_control > crate::momentum::Momentum::BLOCKABLE_ABOVE
                && ahead.try_normalize().is_some_and(|direction| {
                    query
                        .cast_shape(
                            &t.pose(feet, self.state.crouched),
                            Vector::from_array(direction.to_array()),
                            t.shape(self.state.crouched).as_ref(),
                            ShapeCastOptions {
                                max_time_of_impact: ahead.length(),
                                ..Default::default()
                            },
                        )
                        .is_some()
                });
            if !blocked {
                acc += momentum.air_push(horizontal, move_vec, dt);
            }
        } else if !input.jet {
            // Jets replace air control: they steer through the thrust vector.
            let horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
            jump_move = air_control_move(horizontal, move_vec, move_speed);
            acc += air_control_direction(horizontal, move_vec, move_speed)
                * (move_speed * t.air_control).min(t.acceleration * t.air_control * dt);
        }
        // v20 jumps while jump is held, from any surface up to jumpSurfaceAngle,
        // shortly after leaving one, and adds the impulse to current velocity.
        let jump_contact = contact.normal.filter(|_| contact.jump);
        let jump = &mut self.state.jump;
        if let Some(normal) = jump_contact {
            jump.normal = normal.to_array();
        }
        // canJump (0x5A2AA0): no jump right after a ceiling hit, and none while
        // rising faster than 3 unless moving faster than 4 across the ground.
        // A body lying prone does not jump, nor one a rule has locked
        // (`set_speed_scale` 0: AA's `LockMovement` during a posture change,
        // whose `CanJump` is false).
        let can_jump = input.jump
            && !self.state.posture.prone
            && self.state.speed_scale > 0.0
            && jump.delay == 0
            && jump.since_contact < JUMP_WINDOW_TICKS
            && !jump.ceiling
            && (previous.y <= 3.0 || Vec3::new(previous.x, 0.0, previous.z).length() > 4.0);
        // Rising faster than maxJumpSpeed skips the jump and this tick's
        // bookkeeping both (0x5AF7AC).
        let too_fast = can_jump && previous.y > MAX_JUMP_SPEED;
        let jumped = can_jump && !too_fast;
        if too_fast {
        } else if jumped {
            let normal = Vec3::from(jump.normal);
            let rise_scale = if previous.y <= MIN_JUMP_SPEED {
                1.0
            } else {
                1.0 - (previous.y - MIN_JUMP_SPEED) / (MAX_JUMP_SPEED - MIN_JUMP_SPEED)
            };
            // Facing away from the surface also pushes the jump along the move.
            let direction = jump_move.normalize_or_zero();
            let away = direction.dot(normal);
            if away > 0.0 {
                acc += direction * t.jump_speed * away;
            }
            acc.y += normal.y * t.jump_speed * rise_scale;
            // `jump_delay_ticks` counts 120 Hz ticks; this counts Torque's.
            jump.delay = ((u16::from(t.jump_delay_ticks) * u16::from(STEP_PARTS)
                + u16::from(TICK_PARTS / 2))
                / u16::from(TICK_PARTS)) as u8;
            jump.since_contact = JUMP_WINDOW_TICKS;
        } else {
            // 0x5AFAC3: the delay runs down every tick, in the air too; contact
            // opens the window only once it has run out.
            jump.delay = jump.delay.saturating_sub(1);
            if jump_contact.is_some() && jump.delay == 0 {
                jump.since_contact = 0;
            } else {
                jump.since_contact = jump.since_contact.saturating_add(1).min(JUMP_WINDOW_TICKS);
            }
        }
        velocity += acc;
        if let Some((_, coverage)) = liquid {
            // v20: holding jump swims up (hard from a near standstill, less when
            // only partly submerged); holding crouch dives.
            if input.jump {
                velocity.y += dt
                    * if Vec3::new(previous.x, 0.0, previous.z).length() < 2.0 {
                        2.0 * t.swim_rise
                    } else if coverage <= 0.99 {
                        (coverage * 1.25 + 0.25) / 0.75 * t.swim_rise
                    } else {
                        t.swim_rise
                    };
            }
            if input.crouch {
                velocity.y -= t.dive_acceleration * dt;
            }
        }
        if input.jet {
            // Crouched jets push flat along the body's facing with no lift
            // (v20 updateMove near 0x5afbcf). Otherwise thrust leans into the
            // move direction and strengthens while falling.
            let thrust = if self.state.crouched {
                forward
            } else {
                let mut thrust = (move_vec + Vec3::Y * t.jet_lift).normalize();
                let falling = -previous.y;
                if falling > 0.0 {
                    thrust.y *= 1.0 + 0.5 * (falling * 0.05).min(1.0);
                }
                thrust
            };
            velocity += thrust * t.jet_acceleration * dt;
        }
        let horizontal_speed = Vec3::new(velocity.x, 0.0, velocity.z).length();
        if horizontal_speed > t.horizontal_resist_speed {
            let capped = horizontal_speed.min(t.horizontal_max_speed);
            let resisted =
                capped - (capped - t.horizontal_resist_speed) * t.horizontal_resist_factor * dt;
            velocity.x *= resisted / horizontal_speed;
            velocity.z *= resisted / horizontal_speed;
        }
        if velocity.y > t.up_resist_speed {
            let capped = velocity.y.min(t.up_max_speed);
            velocity.y = capped - (capped - t.up_resist_speed) * t.up_resist_factor * dt;
        }
        // v20 ShapeBase::updateContainer: water drag is drag * viscosity and
        // buoyancy is density-relative; crouching on the bottom holds the player
        // down. Falling into partial coverage blends toward air drag.
        let mut vertical_drag = t.drag;
        let drag = match liquid {
            Some((water, coverage)) => {
                velocity += Vec3::from(water.current) * coverage * dt;
                if !(input.crouch && contact.run) {
                    velocity.y += water.density / t.density * coverage * t.gravity * dt;
                }
                let drag = t.drag * water.viscosity;
                vertical_drag = if coverage < 0.99 && velocity.y < 0.0 {
                    (drag - t.drag) * coverage + t.drag
                } else {
                    drag
                };
                drag
            }
            None => t.drag,
        };
        let horizontal_keep = (1.0 - drag * dt).max(0.0);
        velocity.x *= horizontal_keep;
        velocity.z *= horizontal_keep;
        velocity.y *= (1.0 - vertical_drag * dt).max(0.0);
        // On a ladder the move keys set the speed outright: no gravity,
        // drag or run force.
        if let Some(on_ladder) = climb_velocity {
            velocity = on_ladder;
        }
        // A rope holds the body within its length of the anchor; the keys
        // pump a swing while it hangs.
        if let Some(mut tether) = self.state.tether {
            let pushing = if contact.run || liquid.is_some() {
                Vec3::ZERO
            } else {
                move_vec
            };
            let winch = match (input.jump, input.crouch) {
                (true, false) => 1,
                (false, true) => -1,
                _ => 0,
            };
            self.state.tether = tether
                .constrain(feet, t, &mut velocity, pushing, winch, dt)
                .then_some(tether);
        }
        // Players move one after another against each other's previous pose, so
        // each closes at most half its gap to another player per tick.
        let pose = t.pose(feet, self.state.crouched);
        let shape = t.shape(self.state.crouched);
        let translation = velocity * dt;
        let is_player = |_: ColliderHandle, collider: &Collider| collider.user_data >> 64 == 1;
        if let Some((direction, distance)) =
            translation.try_normalize().zip(Some(translation.length()))
            && let Some((other, hit)) = physics
                .query_pipeline_with_filter(filter.predicate(&is_player))
                .cast_shape(
                    &pose,
                    Vector::from_array(direction.to_array()),
                    shape.as_ref(),
                    ShapeCastOptions {
                        max_time_of_impact: distance * 2.0,
                        ..Default::default()
                    },
                )
        {
            // Already touching, the cast's normal is arbitrary (it pushed a
            // player met head-on sideways into the other). Boxes that touch
            // part along the axis they overlap least on.
            let normal = if hit.time_of_impact > 0.0 {
                Vec3::from(hit.normal1.to_array())
            } else {
                let (ours, theirs) = (
                    shape.compute_aabb(&pose),
                    physics.colliders[other].compute_aabb(),
                );
                let (a_min, a_max) = (v3(ours.mins), v3(ours.maxs));
                let (b_min, b_max) = (v3(theirs.mins), v3(theirs.maxs));
                let overlap = a_max.min(b_max) - a_min.max(b_min);
                let away = (a_min + a_max) - (b_min + b_max);
                let axis = if overlap.x <= overlap.y && overlap.x <= overlap.z {
                    Vec3::X
                } else if overlap.z <= overlap.y {
                    Vec3::Z
                } else {
                    Vec3::Y
                };
                axis * away.dot(axis).signum()
            };
            let gap = hit.time_of_impact * -direction.dot(normal);
            let into = -translation.dot(normal);
            if into > gap * 0.5 {
                velocity += normal * (into - gap * 0.5) / dt;
            }
        }
        let before_collision = velocity;
        // v20 Player::updatePos: sweep, slide and step through the polygons.
        let moved = torque::update_pos(
            &soup,
            &torque::Mover {
                half_width: half,
                round: t.body == Body::Cylinder,
                height,
                run_cos,
                jump_cos,
                max_step: t.step_height,
                step_reach,
                elasticity: torque::NORMAL_ELASTICITY,
                back_off: torque::BACK_OFF * torque_ticks,
                epsilon: torque::Epsilon::at(torque_ticks),
                wall_step: t.wall_step.map(|w| torque::WallStep {
                    lift: w.lift,
                    retry: w.retry,
                }),
            },
            feet,
            &mut velocity,
            dt,
        );
        let mut contacts: BTreeSet<BrickId> = moved
            .touched
            .iter()
            .filter_map(|tag| u64::try_from(*tag).ok().filter(|id| *id > 0))
            .collect();
        self.state.feet = moved.feet.to_array();
        self.state.velocity = velocity.to_array();
        // Coming down a ladder onto a floor steps off it.
        if self.state.posture.climbing && moved.floor && input.forward < 0.0 {
            self.state.posture.climbing = false;
        }
        // updatePos: landing on a floor reopens the jump window at once, so a
        // held jump hops on the next tick and a bunny hop loses one tick of
        // ground friction, not four.
        if moved.floor {
            self.state.jump.since_contact = 0;
        }
        if let Some(ceiling) = moved.ceiling {
            self.state.jump.ceiling = ceiling;
        }
        // Standing on a run surface after the move (v20's run-surface contact),
        // and not still closing on it: a fall that stops within the contact
        // slab of a floor lands (and impacts) on the next tick's sweep.
        let end_contact = torque::find_contact(&soup, moved.feet, half, run_cos, jump_cos);
        self.state.grounded = end_contact.run
            && end_contact
                .normal
                .is_some_and(|n| velocity.dot(n) > -LANDING_SPEED);
        // Down a stair or a slope: the floor that fell away under a walking body within a step's height is walked down onto, so
        // the body keeps its grip and its speed instead of falling the stair a step at a time.
        if t.follow_floor
            && was_grounded
            && !self.state.grounded
            && !jumped
            && velocity.y <= 0.0
            && !self.state.posture.climbing
            && let Some((_, hit)) = physics.query_pipeline_with_filter(filter).cast_shape(
                &t.pose(Vec3::from(self.state.feet), self.state.crouched),
                Vector::new(0.0, -1.0, 0.0),
                t.shape(self.state.crouched).as_ref(),
                ShapeCastOptions {
                    max_time_of_impact: step_reach,
                    ..Default::default()
                },
            )
            && hit.time_of_impact > 0.0
            && hit.normal1.y >= run_cos
        {
            self.state.feet[1] -= hit.time_of_impact;
            self.state.velocity[1] = 0.0;
            self.state.grounded = true;
            self.state.jump.since_contact = 0;
        }
        // Animation provenance belongs to the simulation: presentation can
        // skip all the jetting ticks, and pose datagrams can lose them too.
        // A real jump after release starts new motion, even if no rendered
        // frame observed the landing (or this is a valid late jump).
        self.state.jump.jet_flight =
            input.jet || (self.state.jump.jet_flight && !self.state.grounded && !jumped);
        // Its middle went in through an opening: out of the partner, turned
        // and moving on as it was.
        let (_, passed) = passages.travel(feet + middle, moved.feet + middle);
        if let Some(carry) = &passed {
            let out = carry_feet(carry, moved.feet, middle.y);
            self.state.feet = out.to_array();
            let velocity = carry.transform_vector3(velocity);
            self.state.velocity = velocity.to_array();
            self.state.yaw = bri_content::passage::carried_yaw(carry, self.state.yaw);
            self.state.jump.normal = carry
                .transform_vector3(Vec3::from(self.state.jump.normal))
                .to_array();
        }
        // A fall or a push past what a player may hold (a light body's long
        // fall, strong jets) goes on at the limit.
        self.state.keep_in_bounds();
        // Grounded idle motion need not produce a sweep callback. Include nearby
        // solid contacts so on-touch is an entry event, not a movement event.
        let end_pose = t.pose(Vec3::from(self.state.feet), self.state.crouched);
        let near = shape.compute_aabb(&end_pose).loosened(0.02);
        let touches = |pose: &Pose, other: &dyn Shape| {
            rapier3d::parry::query::contact(&end_pose, shape.as_ref(), pose, other, 0.012)
                .is_ok_and(|c| c.is_some_and(|c| c.dist <= 0.012))
        };
        for (_, collider) in query.intersect_aabb_conservative(near) {
            if let Some(compound) = collider.shape().as_compound()
                && parts.part_tag(collider.user_data, 0).is_some()
            {
                // A merged collider: each touched object by its own tag.
                let local = near.transform_by(&collider.position().inverse());
                for (tag, part) in
                    crate::torque::object_parts(compound, collider.user_data, parts, &local)
                {
                    let (sub, piece) = &compound.shapes()[part];
                    if let Ok(id) = u64::try_from(tag)
                        && id > 0
                        && !contacts.contains(&id)
                        && touches(&(*collider.position() * *sub), piece.as_ref())
                    {
                        contacts.insert(id);
                    }
                }
            } else if let Ok(id) = u64::try_from(collider.user_data)
                && id > 0
                && touches(collider.position(), collider.shape())
            {
                contacts.insert(id);
            }
        }
        if was_crouched != self.state.crouched {
            physics.colliders[self.collider].set_shape(shape);
        }
        let pose = self.body_pose(Vec3::from(self.state.feet), self.state.crouched);
        if passed.is_some() || relocated.is_some() {
            // Through an opening the body jumps to the partner, as a
            // teleport does: driven there as its next pose, the step would
            // sweep it across the gap and throw what it touched.
            physics.bodies[self.body].set_position(pose, true);
        } else {
            physics.bodies[self.body].set_next_kinematic_position(pose);
        }
        let touched = contacts.difference(&self.contacts).copied().collect();
        self.contacts = contacts;
        Ok(MotionEvents {
            ticked: true,
            jumped,
            landed: !was_grounded && self.state.grounded,
            touched,
            impact: before_collision - velocity,
            hits: moved.hit,
            contacts: moved.contacts,
            passed,
            controller_fault,
        })
    }
    /// A swept sphere keeps the third-person camera in front of architecture.
    pub fn camera(&self, physics: &PhysicsWorld, third_person: bool) -> Vec3 {
        let eye = self.eye();
        if !third_person {
            return eye;
        }
        let backward = -self.state.forward();
        let origin = Pose::translation(eye.x, eye.y, eye.z);
        let query = physics.query_pipeline_with_filter(
            QueryFilter::default()
                .exclude_sensors()
                .exclude_rigid_body(self.body),
        );
        let hit = query.cast_shape(
            &origin,
            Vector::from_array(backward.to_array()),
            &Ball::new(0.15),
            ShapeCastOptions {
                max_time_of_impact: 8.0,
                ..Default::default()
            },
        );
        eye + backward * hit.map_or(8.0, |(_, h)| (h.time_of_impact - 0.02).max(0.0))
    }
}
fn v3(v: Vector) -> Vec3 {
    Vec3::from_array(v.to_array())
}
/// How deep a player's box must reach into another's, along every axis,
/// to count as started inside it rather than resting against it.
const INSIDE_DEPTH: f32 = 0.02;
/// The other players' colliders the box `body` is already inside.
fn players_inside(query: &QueryPipeline<'_>, body: torque::Box3) -> Vec<ColliderHandle> {
    let shrunk = Aabb::new(
        Vector::from_array((body.min + INSIDE_DEPTH).to_array()),
        Vector::from_array((body.max - INSIDE_DEPTH).to_array()),
    );
    query
        .intersect_aabb_conservative(shrunk)
        .filter(|(_, c)| c.user_data >> 64 == 1)
        .filter(|(_, c)| {
            let theirs = c.compute_aabb();
            let (min, max) = (v3(theirs.mins), v3(theirs.maxs));
            (body.max.min(max) - body.min.max(min)).min_element() > INSIDE_DEPTH
        })
        .map(|(handle, _)| handle)
        .collect()
}
/// v20's moveVec after air control (0x5AF4B5): steering wider than about 25
/// degrees off fast enough travel becomes the half-difference of the two.
fn air_control_move(horizontal: Vec3, move_vec: Vec3, move_speed: f32) -> Vec3 {
    let speed = horizontal.length();
    if speed > 0.0 && move_speed <= speed {
        let along = horizontal / speed;
        let alignment = along.dot(move_vec);
        if alignment > 0.0 && alignment < 0.9 {
            return (move_vec - along) * 0.5;
        }
    }
    move_vec
}
/// v20 air control direction. Input pushes along the move vector, except that
/// momentum at or above the requested speed is never braked: steering within
/// about 25 degrees of travel adds nothing, and wider steering pushes only
/// between the travel and move directions.
fn air_control_direction(horizontal: Vec3, move_vec: Vec3, move_speed: f32) -> Vec3 {
    let speed = horizontal.length();
    if speed > 0.0 && move_speed <= speed {
        let along = horizontal / speed;
        let alignment = along.dot(move_vec);
        if alignment >= 0.9 {
            return Vec3::ZERO;
        }
        if alignment > 0.0 {
            return ((move_vec - along) * 0.5).normalize_or_zero();
        }
    }
    move_vec.normalize_or_zero()
}

#[cfg(test)]
mod fall_tests {
    use super::Fall;
    #[test]
    fn a_fall_starts_at_its_base_and_stops_at_its_max() {
        let law = Fall {
            threshold: 10.0,
            per_speed: 2.0,
            base: 5.0,
            max: Some(25.0),
        };
        assert_eq!(law.harm(10.0, 1.0), None);
        assert_eq!(law.harm(11.0, 1.0), Some(7.0));
        assert_eq!(law.harm(100.0, 1.0), Some(25.0));
        // A bigger body needs a harder landing before it hurts.
        assert_eq!(law.harm(15.0, 2.0), None);
        let plain = Fall {
            threshold: 10.0,
            per_speed: 2.0,
            base: 0.0,
            max: None,
        };
        assert_eq!(plain.harm(14.0, 1.0), Some(8.0));
    }
}
