//! Add-On bodies (`physics.local`): rigid bodies and joints an Add-On's
//! client code asks for, simulated on this player's PC only.
//!
//! Each running Add-On gets its own world, so Add-Ons never see or touch
//! each other's bodies. Bodies collide with the map, bricks and terrain as
//! this client has them, and the players, vehicles and shots it draws push
//! them one way ([`crate::local_physics`]), exactly like brick debris:
//! they are cosmetic, never push back on gameplay, and nothing about them
//! goes over the network.
//!
//! Requests arrive from the Add-On's last frame and are applied before the
//! next step; what the step leaves is what the Add-On reads next frame.
use crate::building::Building;
use crate::local_physics::{
    MAX_STEPS, PUSHER_REACH, Pusher, Pushers, STEP, Shot, Shots, Surroundings,
};
use anyhow::{Result, ensure};
use bri_client_sandbox::bodies::{BodySpec, BodyState, JointSpec, PhysicsCommand, Shape};
use glam::{Quat, Vec3};
use rapier3d::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Momentum a projectile gives each body it passes, per unit of speed (as
/// for debris).
const PROJECTILE_MASS: f32 = 0.2;
/// Fastest a shot or a push leaves a body.
const MAX_SPEED: f32 = 40.0;

struct Body {
    handle: RigidBodyHandle,
    /// Radius of a sphere round its shape, from the body's origin.
    extent: f32,
    shared: bool,
    group: u32,
}

/// A hold for this frame's steps ([`PhysicsCommand::Hold`]).
struct Hold {
    handle: RigidBodyHandle,
    point: Vec3,
    target: Vec3,
    velocity: Vec3,
    max_accel: f32,
}
/// How a hold pulls: a spring of this frequency (radians/s), critically
/// damped, so a held body swings to the target without ringing.
const HOLD_FREQUENCY: f32 = 18.0;

/// Bodies of the same nonzero group (kept in each collider's user data)
/// never touch.
struct Groups;
impl PhysicsHooks for Groups {
    fn filter_contact_pair(&self, context: &PairFilterContext) -> Option<SolverFlags> {
        let group = |c: ColliderHandle| context.colliders.get(c).map_or(0, |c| c.user_data);
        let (a, b) = (group(context.collider1), group(context.collider2));
        if a != 0 && a == b {
            None
        } else {
            Some(SolverFlags::COMPUTE_RIGID_IMPULSES)
        }
    }
}

fn vector(v: Vec3) -> Vector {
    Vector::from_array(v.to_array())
}
fn vec3(v: Vector) -> Vec3 {
    Vec3::from_array(v.to_array())
}
fn quat(r: Rotation) -> Quat {
    Quat::from_array(r.to_array())
}
fn rotation(q: Quat) -> Rotation {
    Rotation::from_array(q.to_array())
}

pub struct AddOnPhysics {
    world: PhysicsWorld,
    bodies: BTreeMap<u32, Body>,
    surroundings: Surroundings,
    pushers: Pushers,
    shots: Shots,
    accumulator: f32,
    holds: Vec<Hold>,
    snapshot: Arc<BTreeMap<u32, BodyState>>,
}

impl Default for AddOnPhysics {
    fn default() -> Self {
        Self {
            world: crate::local_physics::new_world(),
            bodies: BTreeMap::new(),
            surroundings: Surroundings::default(),
            pushers: Pushers::default(),
            shots: Shots::default(),
            accumulator: 0.0,
            holds: Vec::new(),
            snapshot: Arc::default(),
        }
    }
}

impl AddOnPhysics {
    pub fn len(&self) -> usize {
        self.bodies.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }
    /// The Add-On's bodies as last simulated, for its next frame.
    pub fn snapshot(&self) -> Arc<BTreeMap<u32, BodyState>> {
        self.snapshot.clone()
    }

    /// Whether `body` is here and its Add-On shares it.
    pub fn shares(&self, body: u32) -> bool {
        self.bodies.get(&body).is_some_and(|b| b.shared)
    }
    /// Apply one frame's requests, in the order the Add-On made them. The
    /// sandbox already checked them; a handle this world does not know
    /// (a body created and removed in one frame) is skipped. Requests from
    /// other Add-Ons (pushes and holds) reach only shared bodies.
    pub fn apply(&mut self, commands: &[PhysicsCommand]) {
        for command in commands {
            match *command {
                PhysicsCommand::Create { body, spec } => self.create(body, &spec),
                PhysicsCommand::Joint { a, b, spec } => self.join(a, b, &spec),
                PhysicsCommand::Remove { body } => {
                    if let Some(b) = self.bodies.remove(&body) {
                        self.world.remove_body_with_colliders(b.handle, true);
                    }
                }
                PhysicsCommand::Push { body, velocity } => {
                    if let Some(b) = self.bodies.get(&body) {
                        let rb = &mut self.world.bodies[b.handle];
                        let v = (vec3(rb.linvel()) + Vec3::from(velocity))
                            .clamp_length_max(bri_client_sandbox::bodies::MAX_SPEED);
                        rb.set_linvel(vector(v), true);
                    }
                }
                PhysicsCommand::Hold {
                    body,
                    point,
                    target,
                    velocity,
                    max_accel,
                } => {
                    if let Some(b) = self.bodies.get(&body) {
                        self.world.wake_up(b.handle, true);
                        self.holds.push(Hold {
                            handle: b.handle,
                            point: Vec3::from(point),
                            target: Vec3::from(target),
                            velocity: Vec3::from(velocity),
                            max_accel,
                        });
                    }
                }
            }
        }
    }
    /// Pull each held point toward its target for one step of `dt`.
    fn hold(&mut self, dt: f32) {
        for hold in &self.holds {
            let rb = &mut self.world.bodies[hold.handle];
            let point = vec3(rb.position().transform_point(vector(hold.point)));
            let at = vec3(rb.velocity_at_point(vector(point)));
            let w = HOLD_FREQUENCY;
            let accel = ((hold.target - point) * (w * w) + (hold.velocity - at) * (2.0 * w))
                .clamp_length_max(hold.max_accel);
            // Cancel gravity too, so a held body hangs where it is held.
            let accel = accel + Vec3::Y * crate::local_physics::GRAVITY;
            let impulse = accel * rb.mass() * dt;
            rb.apply_impulse_at_point(vector(impulse), vector(point), true);
        }
    }

    fn create(&mut self, id: u32, spec: &BodySpec) {
        let (shape, extent) = match spec.shape {
            Shape::Box([x, y, z]) => (
                ColliderBuilder::cuboid(x, y, z),
                Vec3::new(x, y, z).length(),
            ),
            Shape::Ball(r) => (ColliderBuilder::ball(r), r),
            Shape::Capsule(r, h) => (ColliderBuilder::capsule_y(h, r), r + h),
        };
        let offset = Vec3::from(spec.offset);
        let mut collider = shape
            .translation(vector(offset))
            .density(spec.density)
            .friction(spec.friction)
            .restitution(spec.bounce)
            .user_data(u128::from(spec.group));
        if spec.group != 0 {
            collider = collider.active_hooks(ActiveHooks::FILTER_CONTACT_PAIRS);
        }
        let body = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(
                vector(Vec3::from(spec.position)),
                rotation(Quat::from_array(spec.rotation)),
            ))
            .linvel(vector(Vec3::from(spec.velocity)))
            .angvel(vector(Vec3::from(spec.spin)))
            .linear_damping(spec.linear_damping)
            .angular_damping(spec.angular_damping)
            .ccd_enabled(true);
        let (handle, _) = self.world.insert(body, collider);
        if let Some(old) = self.bodies.insert(
            id,
            Body {
                handle,
                extent: extent + offset.length(),
                shared: spec.shared,
                group: spec.group,
            },
        ) {
            self.world.remove_body_with_colliders(old.handle, true);
        }
    }

    /// A ball joint pinned at the spec's world anchor, its twist axis along
    /// the spec's axis, limited round where the bodies are now.
    fn join(&mut self, a: u32, b: u32, spec: &JointSpec) {
        let (Some(first), Some(second)) = (self.bodies.get(&a), self.bodies.get(&b)) else {
            return;
        };
        let frame = Pose::from_parts(
            vector(Vec3::from(spec.anchor)),
            rotation(Quat::from_rotation_arc(Vec3::X, Vec3::from(spec.axis))),
        );
        let local = |h: RigidBodyHandle| self.world.bodies[h].position().inv_mul(&frame);
        let (frame1, frame2) = (local(first.handle), local(second.handle));
        let joint = SphericalJointBuilder::new()
            .local_frame1(frame1)
            .local_frame2(frame2)
            .contacts_enabled(false)
            .limits(JointAxis::AngX, [-spec.twist, spec.twist])
            .limits(JointAxis::AngY, [-spec.swing, spec.swing])
            .limits(JointAxis::AngZ, [-spec.swing, spec.swing])
            .motor_velocity(JointAxis::AngX, 0.0, spec.friction)
            .motor_velocity(JointAxis::AngY, 0.0, spec.friction)
            .motor_velocity(JointAxis::AngZ, 0.0, spec.friction);
        let (h1, h2) = (first.handle, second.handle);
        self.world.insert_impulse_joint(h1, h2, joint);
    }

    /// Advance by `dt` seconds with this frame's pushers and shots.
    pub fn advance(
        &mut self,
        dt: f32,
        building: &Building,
        pushers: &[Pusher],
        shots: &[Shot],
    ) -> Result<()> {
        ensure!(dt.is_finite() && dt >= 0.0, "Invalid Add-On physics frame time");
        let holds = !self.holds.is_empty();
        if self.bodies.is_empty() {
            self.holds.clear();
            self.accumulator = 0.0;
            self.surroundings.clear(&mut self.world);
            self.pushers.clear(&mut self.world);
            if !self.snapshot.is_empty() {
                self.snapshot = Arc::default();
            }
            return Ok(());
        }
        let centers: Vec<(Vec3, f32)> = self
            .bodies
            .values()
            .map(|b| (vec3(self.world.bodies[b.handle].translation()), b.extent))
            .collect();
        self.pushers.update(&mut self.world, pushers, |p| {
            let reach = PUSHER_REACH + p.half.max_element();
            centers
                .iter()
                .any(|(c, extent)| c.distance(p.center) < reach + extent)
        });
        let owners: HashMap<RigidBodyHandle, u64> = self
            .bodies
            .iter()
            .map(|(id, b)| (b.handle, u64::from(*id)))
            .collect();
        for strike in self.shots.strike(&self.world, shots, &owners) {
            let rb = &mut self.world.bodies[strike.handle];
            let impulse = strike.direction * PROJECTILE_MASS * strike.speed;
            rb.apply_impulse_at_point(vector(impulse), vector(strike.point), true);
            let v = vec3(rb.linvel()).clamp_length_max(MAX_SPEED);
            rb.set_linvel(vector(v), true);
        }
        self.surroundings.sync(&mut self.world, building);
        self.accumulator += dt;
        let mut steps = (self.accumulator / STEP) as u32;
        if steps > MAX_STEPS {
            self.accumulator = 0.0;
            steps = MAX_STEPS;
        } else {
            self.accumulator -= steps as f32 * STEP;
        }
        if steps > 0 {
            let seconds = steps as f32 * STEP;
            let boxes: Vec<_> = self
                .bodies
                .values()
                .filter(|b| !self.world.bodies[b.handle].is_sleeping())
                .map(|b| {
                    let rb = &self.world.bodies[b.handle];
                    Surroundings::reach(
                        vec3(rb.translation()),
                        vec3(rb.linvel()),
                        b.extent,
                        seconds,
                    )
                })
                .collect();
            self.surroundings
                .load(&mut self.world, building, &boxes, |_| false)?;
            for step in 1..=steps {
                self.pushers.drive(&mut self.world, step as f32 / steps as f32);
                if holds {
                    self.hold(STEP);
                }
                self.world.step_with_events(&Groups, &());
            }
            self.pushers.settle();
        }
        // Holds last one frame: an Add-On holding sends them every frame.
        self.holds.clear();
        // A body the solver threw to infinity is gone.
        let lost: Vec<u32> = self
            .bodies
            .iter()
            .filter(|(_, b)| !self.world.bodies[b.handle].position().is_finite())
            .map(|(id, _)| *id)
            .collect();
        for id in lost {
            if let Some(b) = self.bodies.remove(&id) {
                self.world.remove_body_with_colliders(b.handle, true);
            }
        }
        self.snapshot = Arc::new(
            self.bodies
                .iter()
                .map(|(id, b)| {
                    let rb = &self.world.bodies[b.handle];
                    (
                        *id,
                        BodyState {
                            position: vec3(rb.translation()).to_array(),
                            rotation: quat(*rb.rotation()).to_array(),
                            velocity: vec3(rb.linvel()).to_array(),
                            spin: vec3(rb.angvel()).to_array(),
                            resting: rb.is_sleeping(),
                            shared: b.shared,
                            group: b.group,
                            mass: rb.mass(),
                            radius: b.extent,
                        },
                    )
                })
                .collect(),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_client_sandbox::bodies::body_spec;

    fn floor() -> Building {
        crate::brick_debris::tests::building(&[]).0
    }

    fn spec(position: Vec3, group: u32) -> BodySpec {
        let mut r = [0.0; bri_client_sandbox::bodies::BODY_RECORD];
        r[..4].copy_from_slice(&[0.0, 0.25, 0.25, 0.25]);
        r[7..10].copy_from_slice(&position.to_array());
        r[13] = 1.0;
        r[20] = 1.0;
        r[21] = 0.8;
        r[23] = group as f32;
        r[25] = 0.5;
        body_spec(&r).unwrap()
    }

    fn run(physics: &mut AddOnPhysics, building: &Building, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            physics.advance(1.0 / 60.0, building, &[], &[]).unwrap();
        }
    }

    #[test]
    fn bodies_fall_land_and_are_reported_the_frame_after_they_are_made() {
        let building = floor();
        let mut physics = AddOnPhysics::default();
        physics.apply(&[PhysicsCommand::Create {
            body: 1,
            spec: spec(Vec3::new(0.0, 5.0, 0.0), 0),
        }]);
        assert!(physics.snapshot().is_empty(), "not simulated yet");
        run(&mut physics, &building, 3.0);
        let state = physics.snapshot()[&1];
        assert!(
            (0.1..0.5).contains(&state.position[1]),
            "rests on the ground: {state:?}"
        );
        physics.apply(&[PhysicsCommand::Push {
            body: 1,
            velocity: [0.0, 10.0, 0.0],
        }]);
        physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
        assert!(physics.snapshot()[&1].velocity[1] > 5.0, "pushed up");
        physics.apply(&[PhysicsCommand::Remove { body: 1 }]);
        physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
        assert!(physics.snapshot().is_empty() && physics.is_empty());
    }

    #[test]
    fn a_joint_holds_two_bodies_together_and_a_group_lets_them_overlap() {
        let building = floor();
        let mut physics = AddOnPhysics::default();
        // Two boxes overlapping by half, jointed at the top one's base.
        physics.apply(&[
            PhysicsCommand::Create {
                body: 1,
                spec: spec(Vec3::new(0.0, 4.0, 0.0), 3),
            },
            PhysicsCommand::Create {
                body: 2,
                spec: spec(Vec3::new(0.25, 4.0, 0.0), 3),
            },
            PhysicsCommand::Joint {
                a: 1,
                b: 2,
                spec: JointSpec {
                    anchor: [0.125, 4.0, 0.0],
                    axis: [1.0, 0.0, 0.0],
                    swing: 0.5,
                    twist: 0.2,
                    friction: 0.0,
                },
            },
        ]);
        physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
        let start = physics.snapshot();
        let gap = |s: &BTreeMap<u32, BodyState>| {
            Vec3::from(s[&1].position).distance(Vec3::from(s[&2].position))
        };
        assert!(
            (gap(&start) - 0.25).abs() < 0.02,
            "same group: not pushed apart"
        );
        run(&mut physics, &building, 2.0);
        let end = physics.snapshot();
        assert!(end[&1].position[1] < 1.5, "fell together: {end:?}");
        assert!(gap(&end) < 0.6, "held by the joint: {}", gap(&end));
    }

    #[test]
    fn a_hold_carries_a_body_to_its_target_and_it_keeps_its_momentum_after() {
        let building = floor();
        let mut physics = AddOnPhysics::default();
        physics.apply(&[PhysicsCommand::Create {
            body: 1,
            spec: spec(Vec3::new(0.0, 0.3, 0.0), 0),
        }]);
        run(&mut physics, &building, 0.5);
        let target = Vec3::new(2.0, 3.0, 0.0);
        for _ in 0..90 {
            physics.apply(&[PhysicsCommand::Hold {
                body: 1,
                point: [0.25, 0.0, 0.0],
                target: target.to_array(),
                velocity: [0.0; 3],
                max_accel: 400.0,
            }]);
            physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
        }
        let held = physics.snapshot()[&1];
        assert!(
            Vec3::from(held.position).distance(target) < 0.5,
            "held up at the target: {held:?}"
        );
        // Thrown: the push flies on once the hold stops.
        physics.apply(&[PhysicsCommand::Push {
            body: 1,
            velocity: [20.0, 0.0, 0.0],
        }]);
        physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
        assert!(physics.snapshot()[&1].velocity[0] > 15.0);
    }

    #[test]
    fn a_player_walking_through_bodies_shoves_them() {
        let building = floor();
        let mut physics = AddOnPhysics::default();
        physics.apply(&[PhysicsCommand::Create {
            body: 1,
            spec: spec(Vec3::new(0.0, 0.3, 0.0), 0),
        }]);
        run(&mut physics, &building, 1.0);
        for i in 0..60 {
            let x = -2.0 + i as f32 * 0.05;
            let pusher = Pusher {
                id: 9,
                center: Vec3::new(x, 1.2, 0.0),
                rotation: Quat::IDENTITY,
                half: Vec3::new(0.6, 1.2, 0.6),
            };
            physics
                .advance(1.0 / 60.0, &building, &[pusher], &[])
                .unwrap();
        }
        assert!(physics.snapshot()[&1].position[0] > 0.5, "shoved along +x");
    }
}
