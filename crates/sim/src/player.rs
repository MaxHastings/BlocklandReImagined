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
    pub jet_boost: f32,
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
    pub fluid_drag: f32,
    pub crouch_forward: f32,
    pub crouch_backward: f32,
    pub crouch_sideways: f32,
    pub acceleration: f32,
    pub air_control: f32,
    pub gravity: f32,
    pub jump_speed: f32,
    pub jet_acceleration: f32,
    pub jet_horizontal_acceleration: f32,
    pub max_jet_rise: f32,
    pub max_jet_forward: f32,
    pub step_height: f32,
    pub ground_snap: f32,
    pub slope_degrees: f32,
}
impl Default for PlayerTuning {
    fn default() -> Self {
        // Speeds, runForce/mass, air control, jumpForce/mass and runSurfaceAngle:
        // recovered PlayerStandardArmor. Dimensions/eyes/gravity/jet/step behavior
        // remain explicit adaptation assumptions; see docs/player-simulation.md.
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
            fluid_drag: 0.1,
            crouch_forward: 3.0,
            crouch_backward: 2.0,
            crouch_sideways: 2.0,
            acceleration: 48.0,
            air_control: 0.1,
            gravity: 20.0,
            jump_speed: 12.0,
            jet_acceleration: 35.0,
            jet_horizontal_acceleration: 48.0,
            max_jet_rise: 25.0,
            max_jet_forward: 33.0,
            step_height: 0.6,
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
            self.fluid_drag,
            self.crouch_forward,
            self.crouch_backward,
            self.crouch_sideways,
            self.acceleration,
            self.air_control,
            self.gravity,
            self.jump_speed,
            self.jet_acceleration,
            self.jet_horizontal_acceleration,
            self.max_jet_rise,
            self.max_jet_forward,
            self.step_height,
            self.ground_snap,
            self.slope_degrees,
        ];
        ensure!(
            values
                .iter()
                .all(|n| n.is_finite() && *n > 0.0 && *n <= 1000.0)
                && self.crouch_height < self.stand_height
                && self.slope_degrees < 90.0,
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
            RigidBodyBuilder::kinematic_position_based().pose(pose),
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
                jet_boost: 0.0,
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
            RigidBodyBuilder::kinematic_position_based().pose(pose),
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
                && state.pitch.is_finite()
                && state.jet_boost.is_finite()
                && (0.0..=1.0).contains(&state.jet_boost),
            "Invalid authoritative player correction"
        );
        physics.colliders[self.collider].set_shape(self.tuning.shape(state.crouched));
        self.state = state;
        self.contacts.clear();
        self.synchronize_pose(physics);
        Ok(())
    }
    /// Prediction replay moves only this kinematic proxy, never other dynamics.
    pub fn synchronize_pose(&self, physics: &mut PhysicsWorld) {
        let pose = self
            .tuning
            .pose(Vec3::from(self.state.feet), self.state.crouched);
        physics.bodies[self.body].set_position(pose, true);
        physics.bodies[self.body].set_next_kinematic_position(pose);
        physics.detect_collisions(&(), &());
    }
    pub fn eye(&self) -> Vec3 {
        self.state.eye(&self.tuning)
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
        let target_boost = if input.jet && input.crouch { 1.0 } else { 0.0 };
        self.state.jet_boost += (target_boost - self.state.jet_boost).clamp(-8.0 * dt, 8.0 * dt);
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
        let normalize = (input.forward * input.forward + input.right * input.right)
            .sqrt()
            .max(1.0);
        let desired = (forward * input.forward * if input.forward >= 0.0 { fs } else { bs }
            + right * input.right * ss)
            / normalize;
        let mut velocity = Vec3::from(self.state.velocity);
        let horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
        let acceleration = t.acceleration
            * if was_grounded || liquid.is_some() {
                1.0
            } else {
                t.air_control
            };
        // In the air, releasing movement preserves momentum; ground friction stops it.
        let horizontal = if was_grounded || input.forward != 0.0 || input.right != 0.0 {
            horizontal + (desired - horizontal).clamp_length_max(acceleration * dt)
        } else {
            horizontal
        };
        velocity.x = horizontal.x;
        velocity.z = horizontal.z;
        let jumped = input.jump && !self.state.jump_held && was_grounded;
        self.state.jump_held = input.jump;
        if jumped {
            velocity.y = t.jump_speed;
        }
        if !was_grounded || jumped || input.jet {
            velocity.y -= t.gravity * dt;
        } else {
            velocity.y = 0.0;
        }
        if input.jet {
            let boost = self.state.jet_boost;
            velocity.y = (velocity.y + t.jet_acceleration * (1.0 - boost) * dt).min(t.max_jet_rise);
            let aim = self.state.forward();
            let increase = (t.max_jet_forward - velocity.dot(aim))
                .clamp(0.0, t.jet_horizontal_acceleration * boost * dt);
            velocity += aim * increase;
        }
        if let Some((water, coverage)) = liquid {
            let buoyancy = water.density / t.density * coverage;
            if buoyancy > 1.0 || velocity.length_squared() > 0.0 || !was_grounded {
                velocity.y += buoyancy * t.gravity * dt;
            }
            velocity *= (1.0 - t.fluid_drag * water.viscosity * coverage * dt).clamp(0.0, 1.0);
        }
        velocity.y = velocity.y.max(-80.0);
        let rising = velocity.y > 0.0;
        let shape = t.shape(self.state.crouched);
        let pose = t.pose(feet, self.state.crouched);
        let controller = KinematicCharacterController {
            offset: CharacterLength::Absolute(0.005),
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(t.step_height),
                min_width: CharacterLength::Absolute(0.1),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: t.slope_degrees.to_radians(),
            min_slope_slide_angle: t.slope_degrees.to_radians(),
            snap_to_ground: if velocity.y <= 0.0 && !input.jet {
                Some(CharacterLength::Absolute(t.ground_snap))
            } else {
                None
            },
            ..Default::default()
        };
        let mut contacts = BTreeSet::new();
        let mut normals = Vec::new();
        let motion = controller.move_shape(
            dt,
            &query,
            shape.as_ref(),
            &pose,
            Vector::from_array((velocity * dt).to_array()),
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
        // Remove blocked velocity, avoiding accumulation against ceilings/walls.
        for normal in normals {
            let into = velocity.dot(normal);
            if into < 0.0 {
                velocity -= normal * into;
            }
        }
        self.state.grounded = motion.grounded && !rising && !input.jet && !jumped;
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
