//! Bodies that go limp when they die (`Look::ragdoll`), drawn on this screen
//! only: one rigid body per bone of the dead body's skeleton, jointed as
//! its archetype says, falling and tumbling in the world this client has
//! (the map, bricks and terrain). Like AA2's Karma ragdolls nothing about
//! them reaches the host or gameplay, and live players and shots pass
//! through them. Every screen simulates its own, so they do not tumble
//! identically.
//!
//! The simulation is the one Add-On bodies use
//! ([`crate::addon_physics::AddOnPhysics`]), in a world of its own, under
//! the same budget: a ragdoll that would not fit settles the oldest, and one
//! that still does not fit is refused, loudly, and the body plays its death
//! clip.
use crate::addon_physics::{AddOnPhysics, BoneShape, BoneSpec};
use crate::avatar::BoneFrames;
use crate::building::Building;
use anyhow::{Result, bail, ensure};
use bri_client_sandbox::bodies::{JointSpec, PhysicsCommand, PosedNode, Shape};
use bri_content::body::{RAGDOLL_SIZE, Ragdoll, RagdollShape};
use bri_sim::session::{DeathHit, DeathPose};
use glam::{Mat4, Quat, Vec3};
use std::collections::{BTreeMap, BTreeSet};

/// One body that has gone limp.
struct Live {
    /// Each bone's node in the rig and its body.
    bones: Vec<(usize, u32)>,
    /// Which body went limp first: the oldest makes room for a new one.
    serial: u64,
    /// Seconds since it went limp.
    age: f32,
    /// Seconds after which it is held where it lies (0: never).
    freeze: f32,
    frozen: bool,
}

/// Everything about a death the ragdoll starts from.
pub struct Death<'a> {
    pub ragdoll: &'a Ragdoll,
    pub pose: DeathPose,
    /// How fast the body was moving when it died.
    pub velocity: Vec3,
    /// The middle of the body, which a hit's offset is measured from.
    pub centre: Vec3,
}

#[derive(Default)]
pub struct Ragdolls {
    physics: AddOnPhysics,
    live: BTreeMap<u64, Live>,
    /// Owners whose body could not go limp, until they respawn: they play
    /// their death clip.
    refused: BTreeSet<u64>,
    /// The next body number, and the next ragdoll's.
    next: u32,
    serial: u64,
}

impl Ragdolls {
    pub fn is_live(&self, owner: u64) -> bool {
        self.live.contains_key(&owner)
    }
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
    /// Whether `owner`'s body could not go limp.
    pub fn is_refused(&self, owner: u64) -> bool {
        self.refused.contains(&owner)
    }

    /// Let go of `owner`'s body: it respawned or left.
    pub fn stop(&mut self, owner: u64) {
        self.refused.remove(&owner);
        if let Some(live) = self.live.remove(&owner) {
            let remove: Vec<_> = live
                .bones
                .iter()
                .map(|(_, body)| PhysicsCommand::Remove { body: *body })
                .collect();
            self.physics.apply(&remove);
        }
    }
    /// Keep only the bodies of owners `keep` says are still here.
    pub fn retain(&mut self, keep: impl Fn(u64) -> bool) {
        let gone: Vec<u64> = self.live.keys().copied().filter(|o| !keep(*o)).collect();
        for owner in gone {
            self.stop(owner);
        }
    }

    /// Where each bone of `owner`'s body is, for
    /// [`crate::avatar::AvatarMesh::override_nodes`].
    pub fn nodes(&self, owner: u64) -> Vec<PosedNode> {
        let Some(live) = self.live.get(&owner) else {
            return Vec::new();
        };
        let states = self.physics.snapshot();
        live.bones
            .iter()
            .filter_map(|(node, body)| {
                let state = states.get(body)?;
                Some((*node as u32, state.position, state.rotation))
            })
            .collect()
    }

    /// Simulate every body for `dt` seconds. Over the Add-On physics
    /// budget, the oldest moving body is laid still.
    pub fn advance(&mut self, dt: f32, building: &Building) -> Result<()> {
        for live in self.live.values_mut() {
            live.age += dt;
            if !live.frozen && live.freeze > 0.0 && live.age >= live.freeze {
                live.frozen = true;
                if let Some((_, first)) = live.bones.first() {
                    self.physics
                        .apply(&[PhysicsCommand::Freeze { body: *first }]);
                }
            }
        }
        let started = std::time::Instant::now();
        self.physics.advance(dt, building, &[], &[])?;
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        if ms > bri_client_sandbox::Budgets::default().physics_ms_per_frame {
            self.physics.settle_oldest();
        }
        Ok(())
    }

    /// Make `owner`'s body limp where `frames` has it standing, set moving
    /// by the death. The bones the archetype lists must all be in the rig,
    /// and each but the root must have a listed bone above it; otherwise
    /// nothing is made and the error says why.
    pub fn start(&mut self, owner: u64, frames: &BoneFrames, death: &Death) -> Result<()> {
        let started = self.try_start(owner, frames, death);
        if started.is_err() {
            self.refused.insert(owner);
        }
        started
    }
    fn try_start(&mut self, owner: u64, frames: &BoneFrames, death: &Death) -> Result<()> {
        let ragdoll = death.ragdoll;
        let budget = bri_client_sandbox::Budgets::default().bodies;
        ensure!(
            ragdoll.bones.len() <= budget,
            "its ragdoll has {} bones, over the {budget} bodies the physics budget allows",
            ragdoll.bones.len()
        );
        self.stop(owner);
        // The oldest bodies make room.
        while self.physics.len() + ragdoll.bones.len() > budget {
            let Some(oldest) = self
                .live
                .iter()
                .min_by_key(|(_, live)| live.serial)
                .map(|(owner, _)| *owner)
            else {
                bail!("ragdolls already fill the {budget} bodies the physics budget allows");
            };
            self.stop(oldest);
        }
        // The bone above each bone: the nearest listed node up the rig.
        let listed = |node: usize| frames.index.iter().position(|n| *n == node);
        let above: Vec<Option<usize>> = frames
            .index
            .iter()
            .map(|node| {
                let mut at = frames.parents.get(*node).copied().flatten();
                let mut steps = 0;
                while let Some(parent) = at {
                    if let Some(bone) = listed(parent) {
                        return Some(bone);
                    }
                    at = frames.parents.get(parent).copied().flatten();
                    steps += 1;
                    if steps > frames.parents.len() {
                        break;
                    }
                }
                None
            })
            .collect();
        for (i, (bone, up)) in ragdoll.bones.iter().zip(&above).enumerate() {
            ensure!(
                up.is_some() == (i > 0),
                "ragdoll bone {} has no listed bone above it in the rig, so nothing holds it to the body",
                bone.node
            );
        }

        let scale = frames.scale;
        // Every shape is found before anything is made, so a bad one leaves
        // nothing half built.
        let mut built = Vec::with_capacity(ragdoll.bones.len());
        for (i, bone) in ragdoll.bones.iter().enumerate() {
            let limb = limb_frame(i, frames, &above);
            let shapes = bone
                .shapes
                .iter()
                .map(|s| shape(s, scale, limb.as_ref()))
                .collect::<Result<Vec<_>>>()
                .map_err(|e| anyhow::anyhow!("ragdoll bone {}: {e}", bone.node))?;
            built.push(shapes);
        }
        let (velocity, spin, push) = motion(ragdoll, death);
        let first = self.next;
        let ids: Vec<u32> = (0..ragdoll.bones.len() as u32)
            .map(|i| first.wrapping_add(i))
            .collect();
        self.next = first.wrapping_add(ragdoll.bones.len() as u32);
        // Bones of one body never touch each other: the group is its first.
        let group = first.max(1);
        for (i, (bone, shapes)) in ragdoll.bones.iter().zip(built).enumerate() {
            let world = frames.world[frames.index[i]];
            let (_, rotation, position) = world.to_scale_rotation_translation();
            self.physics.create_bone(
                ids[i],
                &BoneSpec {
                    shapes,
                    mass: bone.mass,
                    position,
                    rotation,
                    velocity,
                    spin,
                    linear_damping: bone.linear_damping,
                    angular_damping: bone.angular_damping,
                    friction: ragdoll.friction,
                    bounce: ragdoll.bounce,
                    group,
                    max_speed: ragdoll.max_speed,
                    max_spin: ragdoll.max_spin,
                },
            );
        }
        for (i, bone) in ragdoll.bones.iter().enumerate() {
            let (Some(joint), Some(up)) = (&bone.joint, above[i]) else {
                continue;
            };
            let world: Mat4 = frames.world[frames.index[i]];
            let (_, node, anchor) = world.to_scale_rotation_translation();
            // The joint's x axis is the one it twists or hinges about.
            let frame = node
                * match joint.axis {
                    Some('y') => Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                    Some('z') => Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
                    _ => Quat::IDENTITY,
                };
            self.physics.join_framed(
                ids[up],
                ids[i],
                anchor,
                frame,
                &JointSpec {
                    anchor: anchor.to_array(),
                    axis: [1.0, 0.0, 0.0],
                    swing: joint.swing,
                    twist: joint.hinge.map_or(joint.twist, |[_, high]| high),
                    swing_second: joint.swing_second.unwrap_or(joint.swing),
                    hinge: joint.hinge.is_some(),
                    hinge_low: joint.hinge.map_or(0.0, |[low, _]| low),
                    friction: joint.damping,
                },
                if joint.hinge.is_some() {
                    joint.damping
                } else {
                    joint.twist_damping.unwrap_or(joint.damping)
                },
            );
        }
        // The hit's push, at the point it struck, on the bone nearest it.
        if let Some((point, velocity)) = push {
            let nearest = ragdoll
                .bones
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let at = frames.world[frames.index[i]].w_axis.truncate();
                    (i, at.distance_squared(point))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i);
            if let Some(i) = nearest {
                self.physics.apply(&[PhysicsCommand::PushAt {
                    body: ids[i],
                    velocity: velocity.to_array(),
                    point: point.to_array(),
                }]);
            }
        }
        self.serial += 1;
        self.live.insert(
            owner,
            Live {
                serial: self.serial,
                bones: frames
                    .index
                    .iter()
                    .copied()
                    .zip(ids.iter().copied())
                    .collect(),
                age: 0.0,
                freeze: ragdoll.freeze,
                frozen: false,
            },
        );
        Ok(())
    }
}

/// Where a [`RagdollShape::Limb`] lies in its bone's frame.
struct LimbFrame {
    /// The middle of the span, or of a bone with none from its node on.
    centre: Vec3,
    /// Turns a capsule built along y onto the bone.
    rotation: Quat,
    /// The bone's length to the bone below it, if it has one.
    span: Option<f32>,
}

/// The way bone `i` runs: from its node toward the nearest listed bone that
/// hangs from it, else its first child in the rig, else on the way its
/// parent ran. None where it has no way (a bone alone in its rig).
fn limb_frame(i: usize, frames: &BoneFrames, above: &[Option<usize>]) -> Option<LimbFrame> {
    let node = frames.index[i];
    let (_, turn, start) = frames.world[node].to_scale_rotation_translation();
    let end = above
        .iter()
        .position(|up| *up == Some(i))
        .map(|j| frames.index[j])
        .or_else(|| frames.parents.iter().position(|p| *p == Some(node)))
        .map(|n| frames.world[n].w_axis.truncate());
    let (way, span) = match end {
        Some(end) => ((end - start).try_normalize()?, Some(end.distance(start))),
        None => {
            let parent = frames.parents.get(node).copied().flatten()?;
            (
                (start - frames.world[parent].w_axis.truncate()).try_normalize()?,
                None,
            )
        }
    };
    let local = turn.inverse() * way;
    Some(LimbFrame {
        centre: turn.inverse() * (way * span.unwrap_or(0.0) * 0.5),
        rotation: Quat::from_rotation_arc(Vec3::Y, local),
        span,
    })
}

/// A bone's shape at the body's scale.
fn shape(shape: &RagdollShape, scale: f32, limb: Option<&LimbFrame>) -> Result<BoneShape> {
    let at = |offset: &[f32; 3]| Vec3::from(*offset) * scale;
    Ok(match shape {
        RagdollShape::Limb { radius, length } => {
            let Some(limb) = limb else {
                bail!("a limb needs a bone below it, or a parent to run on from");
            };
            let radius = radius * scale;
            let half = match (length, limb.span) {
                (Some(length), _) => length * scale * 0.5,
                (None, Some(span)) => (span * 0.5 - radius).max(*RAGDOLL_SIZE.start()),
                (None, None) => bail!("a limb on a bone with nothing below it needs a `length`"),
            };
            // With nothing below, it runs on from the node by its own length.
            let centre = match limb.span {
                Some(_) => limb.centre,
                None => limb.rotation * Vec3::Y * (half + radius),
            };
            BoneShape {
                shape: Shape::Capsule(radius, half),
                offset: centre,
                rotation: limb.rotation,
            }
        }
        RagdollShape::Capsule {
            radius,
            half_length,
            axis,
            offset,
        } => BoneShape {
            shape: Shape::Capsule(radius * scale, half_length * scale),
            offset: at(offset),
            // A capsule is built along y; lie it along the axis named.
            rotation: match axis {
                'x' => Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2),
                'z' => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                _ => Quat::IDENTITY,
            },
        },
        RagdollShape::Box {
            half,
            offset,
            rotation,
        } => BoneShape {
            shape: Shape::Box([half[0] * scale, half[1] * scale, half[2] * scale]),
            offset: at(offset),
            rotation: Quat::from_euler(glam::EulerRot::XYZ, rotation[0], rotation[1], rotation[2]),
        },
        RagdollShape::Ball { radius, offset } => BoneShape {
            shape: Shape::Ball(radius * scale),
            offset: at(offset),
            rotation: Quat::IDENTITY,
        },
    })
}

/// How a death sets the body moving: its speed and spin, and the push at
/// the point struck (point and velocity) if the hit had both a way and a
/// place. This is AA2's (`AGP_Pawn.PlayDyingAnimation`), in the archetype's
/// numbers.
fn motion(ragdoll: &Ragdoll, death: &Death) -> (Vec3, Vec3, Option<(Vec3, Vec3)>) {
    let hit: Option<&DeathHit> = death.pose.hit.as_ref();
    let kick = hit.map_or(ragdoll.kick, |h| h.kick);
    // A shot that kills a body lying prone moves it gently, and not up.
    let prone = hit.is_some_and(|h| h.prone_shot);
    let gentle = if prone { ragdoll.prone_scale } else { 1.0 };
    let direction = hit.and_then(|h| h.direction).map(Vec3::from);
    let shot = direction.map_or(Vec3::ZERO, |d| d * kick.speed);
    let v = death.velocity;
    let mut velocity = Vec3::new(
        v.x * ragdoll.carry[0],
        v.y * ragdoll.carry[1],
        v.z * ragdoll.carry[0],
    ) + shot;
    if v.y > -ragdoll.falling && !prone {
        velocity.y += kick.up_kick;
    }
    let spin = match (direction, hit.and_then(|h| h.point)) {
        (Some(_), Some(point)) => {
            // Across the ground the offset counts further than up it.
            let rel = Vec3::from(point) - death.centre;
            let rel = Vec3::new(
                rel.x * ragdoll.spin_scale * gentle,
                rel.y,
                rel.z * ragdoll.spin_scale * gentle,
            );
            rel.cross(shot) * ragdoll.inertia * gentle
        }
        // No way to the hit: tumble the way the death's own roll says.
        _ => random_direction(death.pose.roll) * ragdoll.random_spin,
    }
    .clamp_length_max(ragdoll.max_spin);
    let push = direction
        .zip(hit.and_then(|h| h.point))
        .map(|(d, p)| (Vec3::from(p), d * kick.impulse * gentle));
    (velocity.clamp_length_max(ragdoll.max_speed), spin, push)
}

/// A direction every screen picks the same way from a death's roll.
fn random_direction(roll: u8) -> Vec3 {
    let around = f32::from(roll) / 256.0 * std::f32::consts::TAU;
    Vec3::new(around.cos(), 0.0, around.sin())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::body::DeathKick;

    fn ragdoll() -> Ragdoll {
        serde_json::from_value(serde_json::json!({
            "bones": [
                {"node": "hip", "mass": 1.0,
                 "shapes": [{"kind": "ball", "radius": 0.15}]},
                {"node": "thigh", "mass": 0.5, "angular_damping": 1.0,
                 "shapes": [{"kind": "capsule", "radius": 0.08, "half_length": 0.15, "axis": "y"}],
                 "joint": {"swing": 0.8, "twist": 0.3, "damping": 1.0}},
                {"node": "shin", "mass": 0.5,
                 "shapes": [{"kind": "capsule", "radius": 0.07, "half_length": 0.15, "axis": "y"}],
                 "joint": {"swing": 0.0, "twist": 1.5, "hinge": [0.0, 1.5]}}
            ],
            "friction": 0.8, "max_speed": 40.0, "max_spin": 10.0, "freeze": 10.0,
            "kick": {"impulse": 10.0, "speed": 2.0, "up_kick": 0.5},
            "carry": [0.6, 1.0], "falling": 0.2, "inertia": 4.0, "spin_scale": 13.0,
            "prone_scale": 0.35, "random_spin": 9.0
        }))
        .unwrap()
    }

    /// A hip, a thigh below it and a shin below that, each 0.4 apart,
    /// standing with the hip at `height`.
    fn frames(height: f32) -> BoneFrames {
        BoneFrames {
            parents: vec![None, Some(0), Some(1)],
            index: vec![0, 1, 2],
            world: (0..3)
                .map(|i| Mat4::from_translation(Vec3::new(0.0, height - 0.4 * i as f32, 0.0)))
                .collect(),
            scale: 1.0,
        }
    }

    fn death(ragdoll: &Ragdoll) -> Death<'_> {
        Death {
            ragdoll,
            pose: DeathPose {
                from: None,
                roll: 0,
                hit: None,
            },
            velocity: Vec3::ZERO,
            centre: Vec3::new(0.0, 1.0, 0.0),
        }
    }

    fn run(ragdolls: &mut Ragdolls, seconds: f32) {
        let building = crate::brick_debris::tests::building(&[]).0;
        for _ in 0..(seconds * 60.0) as usize {
            ragdolls.advance(1.0 / 60.0, &building).unwrap();
        }
    }

    fn at(nodes: &[PosedNode], node: u32) -> Vec3 {
        Vec3::from(nodes.iter().find(|n| n.0 == node).unwrap().1)
    }

    #[test]
    fn a_dead_body_falls_limp_to_the_floor_with_its_bones_still_joined() {
        let ragdoll = ragdoll();
        let mut ragdolls = Ragdolls::default();
        ragdolls.start(7, &frames(2.0), &death(&ragdoll)).unwrap();
        assert!(
            ragdolls.is_live(7) && ragdolls.nodes(7).is_empty(),
            "not simulated yet"
        );
        run(&mut ragdolls, 3.0);
        let nodes = ragdolls.nodes(7);
        assert_eq!(nodes.len(), 3);
        let (hip, thigh, shin) = (at(&nodes, 0), at(&nodes, 1), at(&nodes, 2));
        assert!(hip.y < 0.5, "the hip lies on the floor: {hip:?}");
        // Each joint holds its bones the 0.4 apart they stood.
        for (a, b) in [(hip, thigh), (thigh, shin)] {
            assert!(
                (a.distance(b) - 0.4).abs() < 0.08,
                "bones stay joined: {a:?} {b:?}"
            );
        }
        ragdolls.stop(7);
        assert!(!ragdolls.is_live(7));
    }

    #[test]
    fn a_bone_with_none_listed_above_it_in_the_rig_is_refused_and_named() {
        let ragdoll = ragdoll();
        let mut ragdolls = Ragdolls::default();
        let mut orphaned = frames(2.0);
        orphaned.parents = vec![None; 3];
        let error = ragdolls
            .start(7, &orphaned, &death(&ragdoll))
            .unwrap_err()
            .to_string();
        assert!(error.contains("thigh"), "{error}");
        assert!(ragdolls.is_refused(7) && !ragdolls.is_live(7));
        ragdolls.stop(7);
        assert!(!ragdolls.is_refused(7), "a respawn may try again");
    }

    #[test]
    fn a_hit_moves_the_body_along_it_and_a_shot_at_a_prone_body_moves_it_gently() {
        let ragdoll = ragdoll();
        let hit = |prone_shot| DeathHit {
            direction: Some([1.0, 0.0, 0.0]),
            point: Some([0.0, 1.3, 0.0]),
            kick: DeathKick {
                impulse: 10.0,
                speed: 4.0,
                up_kick: 1.0,
            },
            prone_shot,
        };
        let died = |hit| {
            let mut d = death(&ragdoll);
            d.pose.hit = Some(hit);
            motion(&ragdoll, &d)
        };
        let (velocity, spin, push) = died(hit(false));
        assert_eq!(
            velocity,
            Vec3::new(4.0, 1.0, 0.0),
            "along the hit, kicked up"
        );
        assert!(spin.length() > 0.0 && spin.length() <= ragdoll.max_spin);
        let (point, push) = push.unwrap();
        assert_eq!(point, Vec3::new(0.0, 1.3, 0.0));
        assert_eq!(push, Vec3::new(10.0, 0.0, 0.0));
        let (prone, prone_spin, prone_push) = died(hit(true));
        assert_eq!(prone, Vec3::new(4.0, 0.0, 0.0), "no kick up when lying");
        assert!(prone_spin.length() < spin.length());
        assert!(
            (prone_push.unwrap().1.x - 3.5).abs() < 1e-5,
            "35% of the push"
        );
        // A body falling fast gets no kick up either.
        let mut falling = death(&ragdoll);
        falling.pose.hit = Some(hit(false));
        falling.velocity = Vec3::new(0.0, -5.0, 0.0);
        assert_eq!(motion(&ragdoll, &falling).0, Vec3::new(4.0, -5.0, 0.0));
    }

    #[test]
    fn a_death_with_no_way_to_it_spins_the_body_the_way_its_roll_says() {
        let ragdoll = ragdoll();
        let spin = |roll| {
            let mut d = death(&ragdoll);
            d.pose.roll = roll;
            motion(&ragdoll, &d)
        };
        let (velocity, a, push) = spin(0);
        assert_eq!(velocity, Vec3::new(0.0, 0.5, 0.0), "only the kick up");
        assert!(push.is_none());
        assert_eq!(a, spin(0).1, "every screen picks the same way");
        assert_ne!(a, spin(64).1);
        assert!((a.length() - ragdoll.random_spin).abs() < 1e-4);
    }

    #[test]
    fn a_limb_runs_from_its_bone_to_the_one_below_and_the_model_gives_only_sizes() {
        let frames = frames(2.0);
        let above = [None, Some(0), Some(1)];
        let limb = limb_frame(0, &frames, &above).unwrap();
        assert!((limb.span.unwrap() - 0.4).abs() < 1e-5);
        let thigh = RagdollShape::Limb {
            radius: 0.1,
            length: None,
        };
        let made = shape(&thigh, 1.0, Some(&limb)).unwrap();
        let Shape::Capsule(radius, half) = made.shape else {
            panic!("a capsule");
        };
        assert!(
            (radius - 0.1).abs() < 1e-5 && (half - 0.1).abs() < 1e-5,
            "0.4 between, 0.1 caps"
        );
        assert!(
            (made.offset - Vec3::new(0.0, -0.2, 0.0)).length() < 1e-5,
            "mid-bone"
        );
        assert!(
            (made.rotation * Vec3::Y - Vec3::NEG_Y).length() < 1e-5,
            "along the bone, toward the one below"
        );
        // The last bone has nothing below: it runs on the way the one above ran.
        let leaf = limb_frame(2, &frames, &above).unwrap();
        assert!(leaf.span.is_none());
        let foot = RagdollShape::Limb {
            radius: 0.1,
            length: Some(0.2),
        };
        let made = shape(&foot, 1.0, Some(&leaf)).unwrap();
        assert!((made.offset - Vec3::new(0.0, -0.2, 0.0)).length() < 1e-5);
        let bare = RagdollShape::Limb {
            radius: 0.1,
            length: None,
        };
        assert!(
            shape(&bare, 1.0, Some(&leaf)).is_err(),
            "no length to go by"
        );
    }
}
