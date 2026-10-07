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
/// A push not yet given: an impulse, at a point or through the centre,
/// given as a force over the next step.
struct Kick {
    handle: RigidBodyHandle,
    impulse: Vec3,
    point: Option<Vec3>,
}
/// How far ahead (units) soft CCD looks for contacts: one step at the
/// fastest a body may go (`MAX_SPEED`), so nothing passes through a plate.
const SOFT_CCD: f32 = bri_client_sandbox::bodies::MAX_SPEED * STEP;
/// Joint solver passes per step. A ragdoll's joints are impulse joints
/// (a multibody, solved exactly, cost 1 ms a frame per tumbling Blockhead
/// on Max's PC, so a few deaths at once put the Ragdoll over its physics
/// budget and the game stopped it). Four passes keep a thrown Blockhead's
/// joints within 0.004 of each other, against 0.027 with one.
const JOINT_PASSES: usize = 4;
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
    kicks: Vec<Kick>,
    snapshot: Arc<BTreeMap<u32, BodyState>>,
}

impl Default for AddOnPhysics {
    fn default() -> Self {
        Self {
            world: {
                // No swept CCD, whose position clamping tears a ragdoll's
                // limbs apart on landing; bodies use soft CCD instead.
                let mut world = crate::local_physics::new_world();
                world.integration_parameters.max_ccd_substeps = 0;
                world.integration_parameters.num_internal_pgs_iterations = JOINT_PASSES;
                world
            },
            bodies: BTreeMap::new(),
            surroundings: Surroundings::default(),
            pushers: Pushers::default(),
            shots: Shots::default(),
            accumulator: 0.0,
            holds: Vec::new(),
            kicks: Vec::new(),
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
                        let rb = &self.world.bodies[b.handle];
                        let change = Vec3::from(velocity)
                            .clamp_length_max(bri_client_sandbox::bodies::MAX_SPEED);
                        self.kicks.push(Kick {
                            handle: b.handle,
                            impulse: change * rb.mass(),
                            point: None,
                        });
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
    /// The mass a force on `handle` moves: every body jointed to it (a
    /// ragdoll held by one hand), itself included.
    fn carried_mass(&self, handle: RigidBodyHandle) -> f32 {
        let joints = &self.world.impulse_joints;
        let mut seen = std::collections::HashSet::from([handle]);
        let mut next = vec![handle];
        while let Some(body) = next.pop() {
            for (a, b, _, _) in joints.attached_joints(body) {
                for other in [a, b] {
                    if seen.insert(other) {
                        next.push(other);
                    }
                }
            }
        }
        seen.iter().map(|h| self.world.bodies[*h].mass()).sum()
    }
    /// This step's forces: the pushes waiting (`kicks`, all at once) and
    /// each hold pulling its point toward its target. Returns the bodies
    /// to clear after the step.
    fn forces(&mut self, dt: f32) -> Vec<RigidBodyHandle> {
        let mut touched = Vec::new();
        for kick in std::mem::take(&mut self.kicks) {
            let rb = &mut self.world.bodies[kick.handle];
            let force = vector(kick.impulse / dt);
            match kick.point {
                Some(point) => rb.add_force_at_point(force, vector(point), true),
                None => rb.add_force(force, true),
            }
            touched.push(kick.handle);
        }
        for i in 0..self.holds.len() {
            let hold = &self.holds[i];
            let mass = self.carried_mass(hold.handle);
            let hold = &self.holds[i];
            let rb = &mut self.world.bodies[hold.handle];
            let point = vec3(rb.position().transform_point(vector(hold.point)));
            let at = vec3(rb.velocity_at_point(vector(point)));
            let w = HOLD_FREQUENCY;
            let accel = ((hold.target - point) * (w * w) + (hold.velocity - at) * (2.0 * w))
                .clamp_length_max(hold.max_accel);
            // Cancel gravity too, so a held body hangs where it is held.
            let accel = accel + Vec3::Y * crate::local_physics::GRAVITY;
            rb.add_force_at_point(vector(accel * mass), vector(point), true);
            touched.push(hold.handle);
        }
        touched
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
            // Predictive (soft) CCD: it adds contacts ahead of a fast body
            // instead of moving it back along its sweep, which would pull
            // one limb away from the rest.
            .soft_ccd_prediction(SOFT_CCD);
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
        self.world
            .insert_impulse_joint(first.handle, second.handle, joint);
    }

    /// Advance by `dt` seconds with this frame's pushers and shots.
    pub fn advance(
        &mut self,
        dt: f32,
        building: &Building,
        pushers: &[Pusher],
        shots: &[Shot],
    ) -> Result<()> {
        ensure!(
            dt.is_finite() && dt >= 0.0,
            "Invalid Add-On physics frame time"
        );
        if self.bodies.is_empty() {
            self.holds.clear();
            self.kicks.clear();
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
            // No faster than a hit can make it, however light the body.
            let mass = self.world.bodies[strike.handle].mass();
            let impulse = (strike.direction * PROJECTILE_MASS * strike.speed)
                .clamp_length_max(MAX_SPEED * mass);
            self.kicks.push(Kick {
                handle: strike.handle,
                impulse,
                point: Some(strike.point),
            });
        }
        let rebuilt = self.surroundings.sync(&mut self.world, building);
        if rebuilt {
            // A brick that appeared under or in a resting body wakes it.
            for body in self.bodies.values() {
                let rb = &self.world.bodies[body.handle];
                if !rb.is_sleeping() {
                    continue;
                }
                let around =
                    Surroundings::reach(vec3(rb.translation()), Vec3::ZERO, body.extent, 0.0);
                if self
                    .surroundings
                    .load(&mut self.world, building, &[around], |_| false)?
                    > 0
                {
                    self.world.wake_up(body.handle, true);
                }
            }
        }
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
                self.pushers
                    .drive(&mut self.world, step as f32 / steps as f32);
                let touched = self.forces(STEP);
                self.world.step_with_events(&Groups, &());
                for handle in touched {
                    if let Some(rb) = self.world.bodies.get_mut(handle) {
                        rb.reset_forces(false);
                    }
                }
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
    fn a_body_at_top_speed_stops_on_a_thin_brick() {
        // Soft CCD (no swept clamping, which tore ragdoll joints apart)
        // must still stop the fastest body on one brick high in the air.
        let (building, _) = crate::brick_debris::tests::building(&[(7, [0.0, 5.1, 0.0])]);
        let mut physics = AddOnPhysics::default();
        let mut body = spec(Vec3::new(0.0, 9.0, 0.0), 0);
        body.velocity = [0.0, -bri_client_sandbox::bodies::MAX_SPEED, 0.0];
        physics.apply(&[PhysicsCommand::Create {
            body: 1,
            spec: body,
        }]);
        // A 1x1 brick: it lands, bounces and tumbles off the edge later.
        let mut lowest = f32::MAX;
        for _ in 0..15 {
            physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
            lowest = lowest.min(physics.snapshot()[&1].position[1]);
        }
        assert!(lowest > 5.3, "stopped on the brick's top: {lowest}");
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
    fn a_build_change_elsewhere_leaves_resting_bodies_alone() {
        // Max, v0.2.5: ragdolls stopped working in busy minigames. Any brick
        // changing anywhere (a door, a blinking light) dropped every brick
        // round every body and woke them all, so ragdolls never rested and
        // their physics outran the Add-On's budget, which stops it.
        let (mut building, mut world) =
            crate::brick_debris::tests::building(&[(7, [0.0, 5.1, 0.0])]);
        let mut physics = AddOnPhysics::default();
        physics.apply(&[PhysicsCommand::Create {
            body: 1,
            spec: spec(Vec3::new(0.0, 6.0, 0.0), 0),
        }]);
        run(&mut physics, &building, 3.0);
        assert!(physics.snapshot()[&1].resting, "settled on the brick");
        let solid = physics.surroundings.len();
        // A brick far away comes and goes.
        for _ in 0..4 {
            if world.bricks.remove(&8).is_none() {
                world.bricks.insert(
                    8,
                    bri_world::Brick::new(
                        bri_world::ContentRef::Resolved("brick".into()),
                        [40.0, 0.3, 40.0],
                        1,
                    ),
                );
            }
            assert!(building.sync_world(&world).unwrap(), "the build changed");
            physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
            assert!(physics.snapshot()[&1].resting, "left resting");
            assert_eq!(physics.surroundings.len(), solid, "nothing rebuilt");
        }
        // The brick it lies on goes: it falls to the ground.
        world.bricks.remove(&7);
        building.sync_world(&world).unwrap();
        run(&mut physics, &building, 2.0);
        let y = physics.snapshot()[&1].position[1];
        assert!(y < 1.0, "fell when its brick went: {y}");
    }

    #[test]
    fn a_brick_built_into_a_resting_body_wakes_it() {
        let (mut building, mut world) = crate::brick_debris::tests::building(&[]);
        let mut physics = AddOnPhysics::default();
        physics.apply(&[PhysicsCommand::Create {
            body: 1,
            spec: spec(Vec3::new(0.6, 1.0, 0.0), 0),
        }]);
        run(&mut physics, &building, 3.0);
        assert!(physics.snapshot()[&1].resting, "settled on the ground");
        world.bricks.insert(
            7,
            bri_world::Brick::new(
                bri_world::ContentRef::Resolved("brick".into()),
                [1.0, 0.3, 0.0],
                1,
            ),
        );
        building.sync_world(&world).unwrap();
        let before = physics.snapshot()[&1].position;
        run(&mut physics, &building, 2.0);
        let after = physics.snapshot()[&1].position;
        assert!(
            after != before,
            "the brick built into it moved it: {after:?}"
        );
    }

    #[test]
    fn a_hold_lifts_everything_jointed_to_the_body_it_holds() {
        // The Gravity Gun holding a ragdoll by one hand carries the whole
        // Blockhead, not just the hand's own weight.
        let building = floor();
        let mut physics = AddOnPhysics::default();
        let mut commands = Vec::new();
        for (n, x) in [0.0, 0.5, 1.0].into_iter().enumerate() {
            commands.push(PhysicsCommand::Create {
                body: n as u32 + 1,
                spec: spec(Vec3::new(x, 0.3, 0.0), 3),
            });
        }
        for (a, x) in [(1, 0.25), (2, 0.75)] {
            commands.push(PhysicsCommand::Joint {
                a,
                b: a + 1,
                spec: JointSpec {
                    anchor: [x, 0.3, 0.0],
                    axis: [1.0, 0.0, 0.0],
                    swing: 1.0,
                    twist: 1.0,
                    friction: 0.0,
                },
            });
        }
        physics.apply(&commands);
        let target = Vec3::new(0.0, 3.0, 0.0);
        for _ in 0..120 {
            physics.apply(&[PhysicsCommand::Hold {
                body: 1,
                point: [0.0; 3],
                target: target.to_array(),
                velocity: [0.0; 3],
                max_accel: 400.0,
            }]);
            physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
        }
        let held = physics.snapshot()[&1];
        assert!(
            Vec3::from(held.position).distance(target) < 0.5,
            "held up with the rest hanging from it: {held:?}"
        );
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
                shape: None,
            };
            physics
                .advance(1.0 / 60.0, &building, &[pusher], &[])
                .unwrap();
        }
        assert!(physics.snapshot()[&1].position[0] > 0.5, "shoved along +x");
    }

    mod blockhead {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../client-sandbox/tests/support/blockhead.rs"
        ));
    }

    /// The Ragdoll showcase Add-On (`packages/showcase/ragdoll`) driving
    /// this world as the game does: a player dies at a run, the body flops
    /// to the ground and settles in one piece.
    #[test]
    fn the_ragdoll_add_on_falls_in_one_piece_and_settles() {
        use bri_client_sandbox::{AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, World};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/ragdoll");
        let code = AddOnCode::load(&dir).unwrap().unwrap();
        let mut addon = Sandbox::new()
            .unwrap()
            .start_in(&code, Budgets::untimed(), TrustLevel::Sandboxed, 0)
            .unwrap();
        let feet = [0.0, 0.0, 0.0];
        let world = Arc::new(World {
            local: 1,
            players: vec![bri_client_sandbox::world::Player {
                id: 7,
                alive: false,
                feet,
                velocity: [4.0, 0.0, 0.0],
                ..Default::default()
            }],
            skeletons: [(7, blockhead::blockhead(feet))].into(),
            ..Default::default()
        });
        let building = floor();
        let mut physics = AddOnPhysics::default();
        let mut posed = 0;
        for _ in 0..240 {
            let out = addon
                .frame(FrameInput {
                    dt: 1.0 / 60.0,
                    world: world.clone(),
                    bodies: physics.snapshot(),
                    ..Default::default()
                })
                .unwrap();
            posed = out.poses.first().map_or(0, |p| p.nodes.len());
            physics.apply(&out.physics);
            physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
        }
        let bodies = physics.snapshot();
        assert_eq!((bodies.len(), posed), (9, 9));
        let torso = Vec3::from(bodies.values().next().unwrap().position);
        for (id, body) in bodies.iter() {
            let at = Vec3::from(body.position);
            assert!(at.is_finite(), "{id}: {body:?}");
            assert!(
                (-0.1..1.5).contains(&at.y),
                "{id} lies on the ground: {body:?}"
            );
            assert!(at.distance(torso) < 2.0, "{id} stays joined: {body:?}");
            assert!(
                Vec3::from(body.velocity).length() < 1.0,
                "{id} settles: {body:?}"
            );
        }
        assert!(torso.x > 0.3, "carried on as it fell: {torso}");
    }

    /// How far apart each joint's two halves are now, for the Ragdoll's
    /// joints as made (`joints`: first body, second body, anchor), given
    /// where the bodies were made (`made`).
    /// A map floor as interiors are: one layer of triangles, facing up or
    /// (authored the other way round) down.
    fn map_floor(up: bool, y: f32) -> Building {
        let s = 100.0;
        let points = vec![
            Vector::new(-s, y, -s),
            Vector::new(s, y, -s),
            Vector::new(s, y, s),
            Vector::new(-s, y, s),
        ];
        let indices = if up {
            vec![[0, 3, 2], [0, 2, 1]]
        } else {
            vec![[0, 2, 3], [0, 1, 2]]
        };
        let floor = ColliderBuilder::trimesh_with_flags(
            points,
            indices,
            rapier3d::prelude::TriMeshFlags::FIX_INTERNAL_EDGES,
        )
        .unwrap();
        let definitions = bri_sim::definitions::Definitions {
            entries: Default::default(),
        };
        Building::new(definitions, vec![floor]).unwrap()
    }

    /// The Ragdoll Add-On's corpse of a Blockhead standing at `feet`,
    /// thrown by `kick` at frame 20; the lowest any body got in 5 s.
    fn ragdoll_lowest(building: &Building, feet: [f32; 3], kick: [f32; 3]) -> f32 {
        use bri_client_sandbox::{AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, World};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/ragdoll");
        let code = AddOnCode::load(&dir).unwrap().unwrap();
        let mut addon = Sandbox::new()
            .unwrap()
            .start_in(&code, Budgets::untimed(), TrustLevel::Sandboxed, 0)
            .unwrap();
        let mut world = World {
            local: 1,
            players: vec![bri_client_sandbox::world::Player {
                id: 7,
                alive: false,
                feet,
                ..Default::default()
            }],
            skeletons: [(7, blockhead::blockhead(feet))].into(),
            ..Default::default()
        };
        let mut physics = AddOnPhysics::default();
        let mut lowest = f32::MAX;
        for frame in 0..300 {
            world.players[0].velocity = if frame == 20 { kick } else { [0.0; 3] };
            let out = addon
                .frame(FrameInput {
                    dt: 1.0 / 60.0,
                    world: Arc::new(world.clone()),
                    bodies: physics.snapshot(),
                    ..Default::default()
                })
                .unwrap();
            physics.apply(&out.physics);
            physics.advance(1.0 / 60.0, building, &[], &[]).unwrap();
            for body in physics.snapshot().values() {
                lowest = lowest.min(body.position[1]);
            }
        }
        lowest
    }

    #[test]
    fn a_ragdoll_lies_on_a_map_floor_whichever_way_it_faces() {
        // Max, v0.1.8: "my ragdoll sometimes fall through the bedroom
        // floor". A floor whose triangles face down dropped every contact
        // (lowest -11 standing, -25 blasted down) before map triangles were
        // made solid on both sides.
        for up in [true, false] {
            let building = map_floor(up, 0.0);
            for kick in [[0.0; 3], [5.0, -40.0, 0.0], [8.0, 25.0, 15.0]] {
                let lowest = ragdoll_lowest(&building, [0.0; 3], kick);
                assert!(
                    lowest > 0.0,
                    "floor facing {}, thrown {kick:?}: fell to {lowest}",
                    if up { "up" } else { "down" }
                );
            }
        }
    }

    #[test]
    fn bodies_stand_on_the_map_they_are_in_now() {
        // The same body on a map with its floor at 0, then on another map
        // whose floor is 2 lower: the old floor goes.
        let mut physics = AddOnPhysics::default();
        physics.apply(&[PhysicsCommand::Create {
            body: 1,
            spec: spec(Vec3::new(0.0, 4.0, 0.0), 0),
        }]);
        run(&mut physics, &map_floor(true, 0.0), 2.0);
        let y = physics.snapshot()[&1].position[1];
        assert!((0.0..1.0).contains(&y), "rests on the first map: {y}");
        run(&mut physics, &map_floor(true, -2.0), 2.0);
        let y = physics.snapshot()[&1].position[1];
        assert!((-2.0..-1.0).contains(&y), "rests on the new map: {y}");
    }

    #[test]
    fn a_ragdoll_slides_down_a_ramp_and_stays_down() {
        // Max, v0.1.8: on some ramps the ragdoll "goes down and then
        // magically climbs back up": it was pulled back towards its corpse,
        // which stays where the player died.
        use bri_client_sandbox::{AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, World};
        // A 50 degree roof falling towards +x, through the feet: steeper
        // than the limbs' friction holds.
        let slope = 1.2;
        let points = vec![
            Vector::new(-20.0, 20.0 * slope, -20.0),
            Vector::new(20.0, -20.0 * slope, -20.0),
            Vector::new(20.0, -20.0 * slope, 20.0),
            Vector::new(-20.0, 20.0 * slope, 20.0),
        ];
        let ramp = ColliderBuilder::trimesh_with_flags(
            points,
            vec![[0, 3, 2], [0, 2, 1]],
            rapier3d::prelude::TriMeshFlags::FIX_INTERNAL_EDGES,
        )
        .unwrap();
        let definitions = bri_sim::definitions::Definitions {
            entries: Default::default(),
        };
        // Level ground where the roof ends.
        let ground = ColliderBuilder::cuboid(40.0, 1.0, 40.0).translation(Vector::new(
            0.0,
            -20.0 * slope - 1.0,
            0.0,
        ));
        let building = Building::new(definitions, vec![ramp, ground]).unwrap();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/ragdoll");
        let code = AddOnCode::load(&dir).unwrap().unwrap();
        let mut addon = Sandbox::new()
            .unwrap()
            .start_in(&code, Budgets::untimed(), TrustLevel::Sandboxed, 0)
            .unwrap();
        let feet = [0.0, 0.0, 0.0];
        let world = Arc::new(World {
            local: 1,
            players: vec![bri_client_sandbox::world::Player {
                id: 7,
                alive: false,
                feet,
                ..Default::default()
            }],
            skeletons: [(7, blockhead::blockhead(feet))].into(),
            ..Default::default()
        });
        let mut physics = AddOnPhysics::default();
        let (mut lowest, mut climbed) = (f32::MAX, 0.0f32);
        for _ in 0..300 {
            let out = addon
                .frame(FrameInput {
                    dt: 1.0 / 60.0,
                    world: world.clone(),
                    bodies: physics.snapshot(),
                    ..Default::default()
                })
                .unwrap();
            physics.apply(&out.physics);
            physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
            // On the roof, before it tumbles onto the ground at its foot.
            if let Some(body) = physics.snapshot().values().next()
                && body.position[0] < 15.0
            {
                lowest = lowest.min(body.position[1]);
                climbed = climbed.max(body.position[1] - lowest);
            }
        }
        let body = *physics.snapshot().values().next().unwrap();
        assert!(body.position[0] > 15.0, "slid down the roof: {body:?}");
        assert!(climbed < 0.3, "climbed {climbed} back up the ramp");
    }

    #[test]
    fn jointed_bodies_keep_the_motion_they_were_made_with() {
        // A ragdoll keeps its corpse's motion and its pop once joined.
        let building = floor();
        let mut physics = AddOnPhysics::default();
        let mut commands = Vec::new();
        for (n, x) in [0.0, 0.5, 1.0].into_iter().enumerate() {
            let mut body = spec(Vec3::new(x, 4.0, 0.0), 3);
            body.velocity = [4.0, 2.0, 0.0];
            commands.push(PhysicsCommand::Create {
                body: n as u32 + 1,
                spec: body,
            });
        }
        for (a, x) in [(1, 0.25), (2, 0.75)] {
            commands.push(PhysicsCommand::Joint {
                a,
                b: a + 1,
                spec: JointSpec {
                    anchor: [x, 4.0, 0.0],
                    axis: [1.0, 0.0, 0.0],
                    swing: 1.0,
                    twist: 1.0,
                    friction: 0.0,
                },
            });
        }
        physics.apply(&commands);
        physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
        for (id, body) in physics.snapshot().iter() {
            let v = Vec3::from(body.velocity);
            assert!(
                v.distance(Vec3::new(4.0, 2.0 - 20.0 / 60.0, 0.0)) < 0.05,
                "{id} kept its motion: {v}"
            );
        }
    }

    pub(crate) fn joint_stretch(
        joints: &[(u32, u32, Vec3)],
        made: &BTreeMap<u32, (Vec3, Quat)>,
        now: &BTreeMap<u32, BodyState>,
    ) -> f32 {
        let at = |body: u32, anchor: Vec3| {
            let (p0, r0) = made[&body];
            let local = r0.inverse() * (anchor - p0);
            let state = now[&body];
            Vec3::from(state.position) + Quat::from_array(state.rotation) * local
        };
        joints
            .iter()
            .filter(|(a, b, _)| now.contains_key(a) && now.contains_key(b))
            .map(|(a, b, anchor)| at(*a, *anchor).distance(at(*b, *anchor)))
            .fold(0.0, f32::max)
    }

    /// The Ragdoll's joints hold under a fall and a blast: the limbs never
    /// come apart where they meet.
    #[test]
    fn the_ragdoll_stays_joined_through_a_blast() {
        use bri_client_sandbox::{AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, World};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/ragdoll");
        let code = AddOnCode::load(&dir).unwrap().unwrap();
        let mut addon = Sandbox::new()
            .unwrap()
            .start_in(&code, Budgets::untimed(), TrustLevel::Sandboxed, 0)
            .unwrap();
        let feet = [0.0, 0.0, 0.0];
        let mut world = World {
            local: 1,
            players: vec![bri_client_sandbox::world::Player {
                id: 7,
                alive: false,
                feet,
                ..Default::default()
            }],
            skeletons: [(7, blockhead::blockhead(feet))].into(),
            ..Default::default()
        };
        let building = floor();
        let mut physics = AddOnPhysics::default();
        let (mut made, mut joints) = (BTreeMap::new(), Vec::new());
        let mut worst: f32 = 0.0;
        for frame in 0..240 {
            // A rocket throws the corpse a little after it fell.
            world.players[0].velocity = if frame == 40 {
                [8.0, 25.0, 15.0]
            } else {
                [0.0; 3]
            };
            let out = addon
                .frame(FrameInput {
                    dt: 1.0 / 60.0,
                    world: Arc::new(world.clone()),
                    bodies: physics.snapshot(),
                    ..Default::default()
                })
                .unwrap();
            for command in &out.physics {
                match command {
                    PhysicsCommand::Create { body, spec } => {
                        made.insert(
                            *body,
                            (Vec3::from(spec.position), Quat::from_array(spec.rotation)),
                        );
                    }
                    PhysicsCommand::Joint { a, b, spec } => {
                        joints.push((*a, *b, Vec3::from(spec.anchor)))
                    }
                    _ => {}
                }
            }
            physics.apply(&out.physics);
            physics.advance(1.0 / 60.0, &building, &[], &[]).unwrap();
            if frame == 41 {
                // Thrown: every limb flies up.
                let slowest = physics
                    .snapshot()
                    .values()
                    .map(|b| b.velocity[1])
                    .fold(f32::INFINITY, f32::min);
                assert!(slowest > 10.0, "the blast threw every limb: {slowest}");
            }
            let stretch = joint_stretch(&joints, &made, &physics.snapshot());
            if stretch > worst {
                println!("frame {frame}: joints {stretch:.3} apart");
            }
            worst = worst.max(stretch);
        }
        assert_eq!(joints.len(), 8);
        assert!(worst < 0.05, "joints came {worst} apart");
        // Landed from the throw on the floor, not through it.
        for (id, body) in physics.snapshot().iter() {
            assert!(body.position[1] > -0.1, "{id} fell through: {body:?}");
        }
    }
}
