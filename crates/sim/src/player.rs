//! Fixed-tick player motor. Inputs contain intentions, never a client position.
use anyhow::{Result, ensure};
use bri_world::{BrickId, OwnerId};
use glam::Vec3;
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::{
    control::{CharacterAutostep, CharacterLength, KinematicCharacterController},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Original engine tick; v20 per-tick constants are converted with it.
const TORQUE_TICK: f32 = 0.032;
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveInput {
    pub forward: f32,
    pub right: f32,
    pub yaw: f32,
    pub pitch: f32,
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
    pub grounded: bool,
    pub crouched: bool,
    pub jetting: bool,
    pub jump_held: bool,
}
impl PlayerState {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
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
    pub ground_snap: f32,
    pub slope_degrees: f32,
}
impl Default for PlayerTuning {
    fn default() -> Self {
        // Speeds, runForce/mass, air control, drag, jumpForce/mass, resistance
        // and runSurfaceAngle: recovered PlayerStandardArmor. Gravity, jet thrust,
        // jet lift and step height (maxStepHeight default): v20 engine constants.
        // Dimensions, eyes and ground snap remain adaptation assumptions; see
        // docs/player-simulation.md.
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
            ground_snap: 0.2,
            slope_degrees: 70.0,
        }
    }
}
impl PlayerTuning {
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
            self.forward,
            self.backward,
            self.sideways,
            self.underwater_forward,
            self.underwater_backward,
            self.underwater_sideways,
            self.density,
            self.swim_acceleration,
            self.swim_rise,
            self.dive_acceleration,
            self.crouch_forward,
            self.crouch_backward,
            self.crouch_sideways,
            self.acceleration,
            self.air_control,
            self.drag,
            self.gravity,
            self.jump_speed,
            self.jet_acceleration,
            self.jet_lift,
            self.horizontal_max_speed,
            self.horizontal_resist_speed,
            self.horizontal_resist_factor,
            self.up_max_speed,
            self.up_resist_speed,
            self.up_resist_factor,
            self.step_height,
            self.ground_snap,
            self.slope_degrees,
        ];
        ensure!(
            values
                .iter()
                .all(|n| n.is_finite() && *n > 0.0 && *n <= 1000.0)
                && self.crouch_height < self.stand_height
                && self.slope_degrees < 90.0
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
}
pub struct MotionEvents {
    pub jumped: bool,
    pub landed: bool,
    pub touched: Vec<BrickId>,
    /// Velocity removed by collision this tick (Torque `onImpact` vector).
    pub impact: Vec3,
}
impl Player {
    /// Feet are chosen by the server's map spawn service.
    pub fn spawn(
        physics: &mut PhysicsWorld,
        owner: OwnerId,
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
            ColliderBuilder::new(shape).user_data((1_u128 << 64) | u128::from(owner)),
        );
        physics.detect_collisions(&(), &());
        Ok(Self {
            state: PlayerState {
                owner,
                feet: feet.to_array(),
                velocity: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
                grounded: false,
                crouched: false,
                jetting: false,
                jump_held: false,
            },
            tuning,
            body,
            collider,
            contacts: BTreeSet::new(),
        })
    }
    /// Mirror an existing authoritative player (client prediction). Unlike
    /// `spawn`, the server already validated this position.
    pub fn attach(
        physics: &mut PhysicsWorld,
        state: PlayerState,
        tuning: PlayerTuning,
    ) -> Result<Self> {
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
        };
        player.restore(physics, state)?;
        Ok(player)
    }
    pub fn state(&self) -> &PlayerState {
        &self.state
    }
    /// Authoritative contact box, including current crouch dimensions. This is
    /// the same pose/shape used by the player motor, not a client pickup radius.
    pub fn world_bounds(&self) -> bri_weapons::ItemBounds {
        let half = Vec3::new(self.tuning.width * 0.5, 0., self.tuning.width * 0.5);
        let feet = Vec3::from(self.state.feet);
        bri_weapons::ItemBounds {
            min: (feet - half).to_array(),
            max: (feet + half + Vec3::Y * self.tuning.height(self.state.crouched)).to_array(),
        }
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
        let pose = self
            .tuning
            .pose(Vec3::from(self.state.feet), self.state.crouched);
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
            self.tuning
                .pose(Vec3::from(self.state.feet), self.state.crouched),
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
        let (fs, bs, ss) = if liquid.is_some_and(|(_, c)| c >= 0.9) {
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
        // v20 runs parallel to the contact surface: on a walkable slope the move
        // speed is along the slope, so ramps neither slow nor launch the player.
        let walkable = t.slope_degrees.to_radians().cos();
        let shape = t.shape(self.state.crouched);
        let pose = t.pose(feet, self.state.crouched);
        let ground = if was_grounded {
            query
                .cast_shape(
                    &pose,
                    Vector::new(0.0, -1.0, 0.0),
                    shape.as_ref(),
                    ShapeCastOptions {
                        max_time_of_impact: t.ground_snap,
                        compute_impact_geometry_on_penetration: true,
                        ..Default::default()
                    },
                )
                .map(|(_, hit)| Vec3::from(hit.normal1.to_array()))
                .filter(|normal| normal.y >= walkable)
        } else {
            None
        };
        let along_ground = match ground {
            Some(normal) => move_vec - normal * move_vec.dot(normal),
            None => move_vec,
        };
        let desired = along_ground.normalize_or_zero() * move_speed;
        let desired = Vec3::new(desired.x, 0.0, desired.z);
        let mut velocity = Vec3::from(self.state.velocity);
        let horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
        let horizontal = if was_grounded {
            horizontal + (desired - horizontal).clamp_length_max(t.acceleration * dt)
        } else if liquid.is_some() {
            // Swimming pushes along the move direction; water drag sets the speed.
            horizontal + move_vec.normalize_or_zero() * t.swim_acceleration * dt
        } else if input.jet {
            // Jets replace air control: they steer through the thrust vector.
            horizontal
        } else {
            horizontal
                + air_control_direction(horizontal, move_vec, move_speed)
                    * (move_speed * t.air_control).min(t.acceleration * t.air_control * dt)
        };
        velocity.x = horizontal.x;
        velocity.z = horizontal.z;
        let surface_y = ground.map_or(0.0, |normal| {
            -(normal.x * velocity.x + normal.z * velocity.z) / normal.y
        });
        let jumped = input.jump && !self.state.jump_held && was_grounded;
        self.state.jump_held = input.jump;
        if jumped {
            velocity.y = t.jump_speed;
        }
        if !was_grounded || jumped || input.jet {
            velocity.y -= t.gravity * dt;
        } else {
            velocity.y = surface_y;
        }
        if let Some((_, coverage)) = liquid {
            // v20: holding jump swims up (hard from a near standstill, less when
            // only partly submerged); holding crouch dives.
            if input.jump {
                let previous = Vec3::from(self.state.velocity);
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
            // Thrust leans into the move direction; it strengthens while falling.
            let mut thrust = (move_vec + Vec3::Y * t.jet_lift).normalize();
            let falling = -self.state.velocity[1];
            if falling > 0.0 {
                thrust.y *= 1.0 + 0.5 * (falling * 0.05).min(1.0);
            }
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
                if !(input.crouch && was_grounded) {
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
        let rising = velocity.y > surface_y.max(0.0);
        let controller = KinematicCharacterController {
            offset: CharacterLength::Absolute(0.005),
            // Players step up ledges while walking, not in mid-air.
            autostep: was_grounded.then_some(CharacterAutostep {
                max_height: CharacterLength::Absolute(t.step_height),
                min_width: CharacterLength::Absolute(0.1),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: t.slope_degrees.to_radians(),
            min_slope_slide_angle: t.slope_degrees.to_radians(),
            snap_to_ground: if !rising && !input.jet {
                Some(CharacterLength::Absolute(t.ground_snap))
            } else {
                None
            },
            ..Default::default()
        };
        // Players move one after another against each other's previous pose, so
        // each closes at most half its gap to another player per tick.
        let mut translation = velocity * dt;
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
                translation += normal * (into - gap * 0.5);
            }
        }
        let mut contacts = BTreeSet::new();
        let mut normals = Vec::new();
        let motion = controller.move_shape(
            dt,
            &query,
            shape.as_ref(),
            &pose,
            Vector::from_array(translation.to_array()),
            |c| {
                let tag = physics.colliders[c.handle].user_data;
                if let Ok(id) = u64::try_from(tag)
                    && id > 0
                {
                    contacts.insert(id);
                }
                normals.push(Vec3::from(c.hit.normal1.to_array()));
            },
        );
        let before_collision = velocity;
        let intended = velocity * dt;
        let moved = Vec3::from(motion.translation.to_array());
        let climbed = moved.y > intended.y.max(0.0) + 0.01;
        // Remove blocked velocity, avoiding accumulation against ceilings/walls.
        for normal in normals {
            let into = velocity.dot(normal);
            if into >= 0.0 {
                continue;
            }
            if normal.y >= walkable {
                if was_grounded && !jumped {
                    // Walking onto a ramp turns the run along it at the same speed.
                    let speed = velocity.length();
                    velocity = (velocity - normal * into).normalize_or_zero() * speed;
                } else {
                    velocity -= normal * into;
                }
                continue;
            }
            // A step riser the controller climbed over does not stop the player.
            let across = -Vec3::new(normal.x, 0.0, normal.z).normalize_or_zero();
            let expected = intended.dot(across);
            if climbed && expected > 0.0 && moved.dot(across) >= expected * 0.5 {
                continue;
            }
            velocity -= normal * into;
        }
        // Like v20's run surface, support persists while jetting until thrust lifts
        // the player: jetting along the floor keeps ground friction.
        self.state.grounded = motion.grounded && !rising && !jumped;
        if self.state.grounded {
            velocity.y = 0.0;
        }
        self.state.feet = (feet + Vec3::from(motion.translation.to_array())).to_array();
        self.state.velocity = velocity.to_array();
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
        physics.bodies[self.body]
            .set_next_kinematic_position(t.pose(Vec3::from(self.state.feet), self.state.crouched));
        let touched = contacts.difference(&self.contacts).copied().collect();
        self.contacts = contacts;
        Ok(MotionEvents {
            jumped,
            landed: !was_grounded && self.state.grounded,
            touched,
            impact: before_collision - velocity,
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
