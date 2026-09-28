//! Fixed-tick player motor. Inputs contain intentions, never a client position.
use anyhow::{Result, ensure};
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
/// `PlayerStandardArmor.jumpDelay`: 3 Torque ticks of jumpable contact
/// between jumps, so holding jump hops again 96 ms after each landing.
const JUMP_DELAY_TICKS: u8 = 12;
/// `JumpSkipContactsMax` is 8: a jump stays available for 7 Torque ticks
/// (224 ms) after leaving a jumpable surface.
const JUMP_WINDOW_TICKS: u8 = 27;
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
    /// The player's datablock (`setDataBlock`).
    #[serde(default)]
    pub datablock: crate::player_types::PlayerType,
    /// Uniform `setScale` (`setPlayerScale`).
    #[serde(default = "unit")]
    pub scale: f32,
    /// Jet energy (`mEnergy`), up to the datablock's `maxEnergy`.
    #[serde(default = "full_energy")]
    pub energy: f32,
}
fn unit() -> f32 {
    1.0
}
fn full_energy() -> f32 {
    100.0
}
/// v20 jump bookkeeping (`Player::canJump` and the jump in `updateMove`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JumpState {
    /// Contact ticks left before another jump (`mJumpDelay`).
    pub delay: u8,
    /// Ticks since the last jumpable contact (`mJumpSurfaceLastContact`).
    pub since_contact: u8,
    /// Last jumpable surface normal (`mJumpSurfaceNormal`).
    pub normal: [f32; 3],
}
impl Default for JumpState {
    fn default() -> Self {
        Self {
            delay: 0,
            since_contact: JUMP_WINDOW_TICKS,
            normal: [0.0, 1.0, 0.0],
        }
    }
}
impl PlayerState {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }
    /// The motor constants of this player's datablock at its scale.
    pub fn tuning(&self) -> PlayerTuning {
        self.datablock.tuning().scaled(self.scale)
    }
    pub fn eye(&self, tuning: &PlayerTuning) -> Vec3 {
        Vec3::from(self.feet)
            + Vec3::Y
                * if self.crouched {
                    tuning.crouch_eye
                } else {
                    tuning.stand_eye
                }
    }
}
#[derive(Clone, Debug)]
pub struct PlayerTuning {
    pub width: f32,
    pub stand_height: f32,
    pub crouch_height: f32,
    pub stand_eye: f32,
    pub crouch_eye: f32,
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
}
impl Default for PlayerTuning {
    fn default() -> Self {
        // Speeds, runForce/mass, air control, drag, jumpForce/mass, resistance
        // and runSurfaceAngle: recovered PlayerStandardArmor. Gravity, jet thrust,
        // jet lift and step height (maxStepHeight default): v20 engine constants.
        // Box dimensions: the v20 datablock boxes at 0.25 engine scale. Eyes
        // remain an adaptation assumption; see docs/player-simulation.md.
        Self {
            width: 1.25,
            stand_height: 2.65,
            crouch_height: 1.0,
            stand_eye: 2.4,
            crouch_eye: 0.85,
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
            slope_degrees: 70.0,
            jump_surface_degrees: 80.0,
            jump_delay_ticks: JUMP_DELAY_TICKS,
            can_jet: true,
            max_energy: 100.0,
            recharge: 0.8 / TORQUE_TICK,
            min_jet_energy: 0.0,
            jet_drain: 0.0,
            swim_coverage: 0.9,
        }
    }
}
impl PlayerTuning {
    /// Torque `setScale` scales the player's box and eye; speeds, forces and
    /// the step height stay those of the datablock.
    pub fn scaled(mut self, scale: f32) -> Self {
        if scale.is_finite() && scale > 0.0 && scale != 1.0 {
            self.width *= scale;
            self.stand_height *= scale;
            self.crouch_height *= scale;
            self.stand_eye *= scale;
            self.crouch_eye *= scale;
        }
        self
    }
    fn height(&self, crouched: bool) -> f32 {
        if crouched {
            self.crouch_height
        } else {
            self.stand_height
        }
    }
    fn shape(&self, crouched: bool) -> SharedShape {
        SharedShape::cuboid(
            self.width * 0.5,
            self.height(crouched) * 0.5,
            self.width * 0.5,
        )
    }
    fn pose(&self, feet: Vec3, crouched: bool) -> Pose {
        Pose::translation(feet.x, feet.y + self.height(crouched) * 0.5, feet.z)
    }
    fn validate(&self) -> Result<()> {
        let values = [
            self.width,
            self.stand_height,
            self.crouch_height,
            self.stand_eye,
            self.crouch_eye,
            self.density,
            self.swim_acceleration,
            self.swim_rise,
            self.dive_acceleration,
            self.acceleration,
            self.air_control,
            self.drag,
            self.gravity,
            self.jet_acceleration,
            self.jet_lift,
            self.horizontal_max_speed,
            self.horizontal_resist_speed,
            self.horizontal_resist_factor,
            self.up_max_speed,
            self.up_resist_speed,
            self.up_resist_factor,
            self.slope_degrees,
            self.jump_surface_degrees,
        ];
        ensure!(
            values
                .iter()
                .all(|n| n.is_finite() && *n > 0.0 && *n <= 1000.0)
                && self.crouch_height <= self.stand_height
                && [self.max_energy, self.recharge, self.min_jet_energy, self.jet_drain]
                    .iter()
                    .all(|n| n.is_finite() && (0.0..=10000.0).contains(n))
                // Speeds and the step may be zero (`BallShootPlayer`).
                && [
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
                ]
                .iter()
                .all(|n| n.is_finite() && (0.0..=1000.0).contains(n))
                && self.slope_degrees < 90.0
                && self.jump_surface_degrees < 90.0
                && self.horizontal_resist_speed < self.horizontal_max_speed
                && self.up_resist_speed < self.up_max_speed,
            "Invalid player tuning"
        );
        Ok(())
    }
}
pub struct Player {
    state: PlayerState,
    tuning: PlayerTuning,
    body: RigidBodyHandle,
    collider: ColliderHandle,
    contacts: BTreeSet<BrickId>,
    /// Mounts keep their body at the feet, turned to their heading, so
    /// seats and weapons ride on its transform. Players centre it unturned.
    mount: bool,
}
pub struct MotionEvents {
    pub jumped: bool,
    pub landed: bool,
    pub touched: Vec<BrickId>,
    /// Velocity removed by collision this tick (Torque `onImpact` vector).
    pub impact: Vec3,
    /// Each collision's collider and the speed into its surface before the
    /// collision stopped it (Torque `Player::updatePos` `bd`).
    pub hits: Vec<(ColliderHandle, f32)>,
}
impl Player {
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
            owner > 0 && feet.is_finite() && feet.abs().max_element() <= 1_000_000.0,
            "Invalid player spawn"
        );
        let pose = tuning.pose(feet, false);
        let shape = tuning.shape(false);
        ensure!(
            physics
                .query_pipeline_with_filter(QueryFilter::default().exclude_sensors())
                .intersect_shape(pose, shape.as_ref())
                .next()
                .is_none(),
            "Player spawn is obstructed"
        );
        let (body, collider) = physics.insert(
            RigidBodyBuilder::kinematic_position_based()
                .pose(pose)
                .can_sleep(false),
            ColliderBuilder::new(shape).user_data(tag),
        );
        physics.detect_collisions(&(), &());
        requeue_new_body(physics, body);
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
                datablock: Default::default(),
                scale: 1.0,
                energy: tuning.max_energy,
            },
            tuning,
            body,
            collider,
            contacts: BTreeSet::new(),
            mount: false,
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
                datablock: Default::default(),
                scale: 1.0,
                energy: tuning.max_energy,
            },
            tuning,
            body,
            collider,
            contacts: BTreeSet::new(),
            mount: true,
        })
    }
    /// Restored or scripted motion (`setVelocity`, checkpoints).
    pub fn set_motion(&mut self, velocity: Vec3, grounded: bool) {
        if velocity.is_finite() {
            self.state.velocity = velocity.clamp_length_max(1000.0).to_array();
            self.state.grounded = grounded;
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
    pub fn attach(physics: &mut PhysicsWorld, state: PlayerState) -> Result<Self> {
        let tuning = state.tuning();
        tuning.validate()?;
        ensure!(state.owner > 0, "Invalid player owner");
        let pose = tuning.pose(Vec3::from(state.feet), state.crouched);
        let (body, collider) = physics.insert(
            RigidBodyBuilder::kinematic_position_based()
                .pose(pose)
                .can_sleep(false),
            ColliderBuilder::new(tuning.shape(state.crouched))
                .user_data((1_u128 << 64) | u128::from(state.owner)),
        );
        let mut player = Self {
            state: state.clone(),
            tuning,
            body,
            collider,
            contacts: BTreeSet::new(),
            mount: false,
        };
        player.restore(physics, state)?;
        requeue_new_body(physics, body);
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
    /// `setDataBlock`/`setScale`: new motor constants and box. Growing the box
    /// may overlap geometry; like Torque, the motor resolves it by moving.
    pub fn set_datablock(
        &mut self,
        physics: &mut PhysicsWorld,
        datablock: crate::player_types::PlayerType,
        scale: f32,
    ) -> Result<()> {
        ensure!(
            scale.is_finite() && (0.1..=10.0).contains(&scale),
            "Invalid player scale"
        );
        let tuning = datablock.tuning().scaled(scale);
        tuning.validate()?;
        self.state.datablock = datablock;
        self.state.scale = scale;
        self.state.energy = self.state.energy.min(tuning.max_energy);
        self.tuning = tuning;
        physics.colliders[self.collider].set_shape(self.tuning.shape(self.state.crouched));
        self.synchronize_pose(physics);
        Ok(())
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
    pub fn restore(&mut self, physics: &mut PhysicsWorld, state: PlayerState) -> Result<()> {
        ensure!(
            state.owner == self.state.owner
                && state
                    .feet
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                && state
                    .velocity
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1000.0)
                && state.yaw.is_finite()
                && state.pitch.is_finite(),
            "Invalid authoritative player correction"
        );
        if state.datablock != self.state.datablock || state.scale != self.state.scale {
            let tuning = state.tuning();
            tuning.validate()?;
            self.tuning = tuning;
        }
        physics.colliders[self.collider].set_shape(self.tuning.shape(state.crouched));
        self.state = state;
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
    pub fn eye(&self) -> Vec3 {
        self.state.eye(&self.tuning)
    }
    /// Corpses stop blocking players and weapons; respawn makes them solid.
    pub fn set_solid(&self, physics: &mut PhysicsWorld, solid: bool) {
        physics.colliders[self.collider].set_sensor(!solid);
    }
    /// Server relocation (spawn/respawn/teleport): clears motion state.
    pub fn teleport(&mut self, physics: &mut PhysicsWorld, feet: Vec3, yaw: f32) -> Result<()> {
        ensure!(
            feet.is_finite() && feet.abs().max_element() <= 1_000_000.0 && yaw.is_finite(),
            "Invalid teleport"
        );
        let mut state = self.state.clone();
        state.feet = feet.to_array();
        state.velocity = [0.0; 3];
        state.yaw = yaw;
        state.pitch = 0.0;
        state.grounded = false;
        state.crouched = false;
        state.jetting = false;
        self.restore(physics, state)
    }
    /// Ride a vehicle seat: position and facing come from the seat node.
    pub fn place(&mut self, physics: &mut PhysicsWorld, feet: Vec3, yaw: f32, velocity: Vec3) {
        if !feet.is_finite() || !yaw.is_finite() || !velocity.is_finite() {
            return;
        }
        self.state.feet = feet.to_array();
        self.state.velocity = velocity.to_array();
        self.state.yaw =
            (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        self.state.grounded = true;
        self.state.crouched = false;
        self.state.jetting = false;
        self.synchronize_pose(physics);
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
        physics.detect_collisions(&(), &());
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
        input.validate()?;
        let dt = bri_physics::FIXED_DT;
        let t = &self.tuning;
        // `canJet` and `minJetEnergy` gate the jets; jetting drains energy and
        // `rechargeRate` refills it every tick.
        let jet = input.jet && t.can_jet && self.state.energy >= t.min_jet_energy;
        let input = MoveInput { jet, ..input };
        let drain = if jet { t.jet_drain } else { 0.0 };
        self.state.energy =
            (self.state.energy - drain * dt + t.recharge * dt).clamp(0.0, t.max_energy);
        let was_grounded = self.state.grounded;
        let was_crouched = self.state.crouched;
        let feet = Vec3::from(self.state.feet);
        let filter = QueryFilter::default()
            .exclude_sensors()
            .exclude_rigid_body(self.body);
        let query = physics.query_pipeline_with_filter(filter);
        if input.crouch {
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
        self.state.yaw = input.yaw;
        self.state.pitch = input.pitch;
        self.state.head_yaw = input.head_yaw;
        self.state.jetting = input.jet;
        let forward = Vec3::new(input.yaw.sin(), 0.0, -input.yaw.cos());
        let right = Vec3::new(input.yaw.cos(), 0.0, input.yaw.sin());
        let liquid = waters
            .iter()
            .map(|w| {
                (
                    w,
                    w.coverage(feet.to_array(), t.height(self.state.crouched)),
                )
            })
            .filter(|(_, coverage)| *coverage >= 0.1)
            .max_by(|a, b| a.1.total_cmp(&b.1));
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
        .max(ss * input.right.abs());
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
        let soup = torque::Soup::gather(
            &query,
            &physics.bodies,
            body_box(feet).expanded(Vec3::splat(reach) + Vec3::Y * (step_reach + 0.05)),
            feet,
        );
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
        let mut acc = Vec3::new(0.0, -t.gravity * dt, 0.0);
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
            let run = pv - (velocity + acc);
            acc += run.clamp_length_max(t.acceleration * dt);
        } else if liquid.is_some() {
            // Swimming pushes along the move direction; water drag sets the speed.
            acc += move_vec.normalize_or_zero() * t.swim_acceleration * dt;
        } else if !input.jet {
            // Jets replace air control: they steer through the thrust vector.
            let horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
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
        // Blockland's canJump also refuses while rising faster than 3 unless
        // moving faster than 4 overall.
        let jumped = input.jump
            && jump.delay == 0
            && jump.since_contact < JUMP_WINDOW_TICKS
            && (previous.y <= 3.0 || previous.length() > 4.0)
            && previous.y <= MAX_JUMP_SPEED;
        if jumped {
            let normal = Vec3::from(jump.normal);
            let rise_scale = if previous.y <= MIN_JUMP_SPEED {
                1.0
            } else {
                1.0 - (previous.y - MIN_JUMP_SPEED) / (MAX_JUMP_SPEED - MIN_JUMP_SPEED)
            };
            // Facing away from the surface also pushes the jump along the move.
            let direction = move_vec.normalize_or_zero();
            let away = direction.dot(normal);
            if away > 0.0 {
                acc += direction * t.jump_speed * away;
            }
            acc.y += normal.y * t.jump_speed * rise_scale;
            jump.delay = t.jump_delay_ticks;
            jump.since_contact = JUMP_WINDOW_TICKS;
        } else if jump_contact.is_some() {
            jump.delay = jump.delay.saturating_sub(1);
            jump.since_contact = 0;
        } else {
            jump.since_contact = jump.since_contact.saturating_add(1).min(JUMP_WINDOW_TICKS);
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
        velocity.y = velocity.y.max(-80.0);
        // Players move one after another against each other's previous pose, so
        // each closes at most half its gap to another player per tick.
        let pose = t.pose(feet, self.state.crouched);
        let shape = t.shape(self.state.crouched);
        let translation = velocity * dt;
        let is_player = |_: ColliderHandle, collider: &Collider| collider.user_data >> 64 == 1;
        if let Some((direction, distance)) =
            translation.try_normalize().zip(Some(translation.length()))
            && let Some((_, hit)) = physics
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
            let normal = Vec3::from(hit.normal1.to_array());
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
                height,
                run_cos,
                jump_cos,
                max_step: t.step_height,
                step_reach,
                elasticity: torque::NORMAL_ELASTICITY,
                back_off: torque::BACK_OFF * torque_ticks,
                epsilon: torque::Epsilon::at(torque_ticks),
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
        // Standing on a run surface after the move (v20's run-surface contact),
        // and not still closing on it: a fall that stops within the contact
        // slab of a floor lands (and impacts) on the next tick's sweep.
        let end_contact = torque::find_contact(&soup, moved.feet, half, run_cos, jump_cos);
        self.state.grounded = end_contact.run
            && end_contact
                .normal
                .is_some_and(|n| velocity.dot(n) > -LANDING_SPEED);
        // Grounded idle motion need not produce a sweep callback. Include nearby
        // solid contacts so on-touch is an entry event, not a movement event.
        let end_pose = t.pose(Vec3::from(self.state.feet), self.state.crouched);
        for (_, collider) in
            query.intersect_aabb_conservative(shape.compute_aabb(&end_pose).loosened(0.02))
        {
            if let Ok(id) = u64::try_from(collider.user_data)
                && id > 0
                && rapier3d::parry::query::contact(
                    &end_pose,
                    shape.as_ref(),
                    collider.position(),
                    collider.shape(),
                    0.012,
                )
                .is_ok_and(|c| c.is_some_and(|c| c.dist <= 0.012))
            {
                contacts.insert(id);
            }
        }
        if was_crouched != self.state.crouched {
            physics.colliders[self.collider].set_shape(shape);
        }
        let pose = self.body_pose(Vec3::from(self.state.feet), self.state.crouched);
        physics.bodies[self.body].set_next_kinematic_position(pose);
        let touched = contacts.difference(&self.contacts).copied().collect();
        self.contacts = contacts;
        Ok(MotionEvents {
            jumped,
            landed: !was_grounded && self.state.grounded,
            touched,
            impact: before_collision - velocity,
            hits: moved.hit,
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

/// Rapier's collision-only pipeline (`detect_collisions`) consumes a new
/// body's pending changes without an island manager, so a body inserted
/// mid-session (a bot joining during a step) never entered an island and
/// tripped Rapier's island consistency check. Marking it modified again lets
/// the next physics step file it into an island.
fn requeue_new_body(physics: &mut PhysicsWorld, body: RigidBodyHandle) {
    let _ = physics.bodies.get_mut(body);
}
