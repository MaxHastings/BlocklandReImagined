//! Translation-only fallback uses the same prepared collision window as
//! resident bodies. Each bounded motion step finishes against one current-frame
//! rubble snapshot; no paused query retains yesterday's moving obstacles.
use super::*;
use rapier3d::parry::{
    bounding_volume::Aabb,
    partitioning::{Bvh, BvhBuildStrategy},
    query::{ShapeCastOptions, cast_shapes, contact},
};

type Owner = (u64, u64, usize);
const MAX_STEP: f32 = crate::local_physics::STEP * crate::local_physics::MAX_STEPS as f32;
const SLIDES: u32 = 3;
pub(super) const MAX_SEPARATION: f32 = 0.5;

#[derive(Clone, Copy)]
pub(super) struct Frame<'a> {
    pub dt: f32,
    pub actors: &'a [(Pusher, Vec3)],
    pub scene: &'a RubbleScene,
}
struct RubbleBody {
    owner: Option<Owner>,
    shape: SharedShape,
    pose: Pose,
    velocity: Vec3,
}
pub(super) struct RubbleScene {
    bvh: Bvh,
    shapes: Vec<RubbleBody>,
}
#[derive(Clone, Copy)]
struct Hit {
    fraction: f32,
    normal: Vec3,
    surface_velocity: Vec3,
}
impl RubbleScene {
    pub(super) fn new(debris: &BrickDebris) -> Self {
        let mut shapes = Vec::new();
        for (key, s) in &debris.sections {
            for (i, c) in s.chunks.iter().enumerate().filter(|(_, c)| !c.released) {
                if let Some(shape) = &c.proxy {
                    let (at, turn) = c.pose(&debris.world);
                    let velocity = c.handle.map_or(c.velocity, |h| {
                        Vec3::from_array(debris.world.bodies[h].linvel().to_array())
                    });
                    shapes.push(RubbleBody {
                        owner: Some((key.0, key.1, i)),
                        shape: shape.clone(),
                        pose: Pose::from_parts(
                            Vector::from_array(at.to_array()),
                            Rotation::from_array(turn.to_array()),
                        ),
                        velocity,
                    });
                }
            }
        }
        if !shapes.is_empty() {
            for b in debris.bodies.values() {
                let rb = &debris.world.bodies[b.handle];
                for collider in rb.colliders() {
                    let c = &debris.world.colliders[*collider];
                    shapes.push(RubbleBody {
                        owner: None,
                        shape: c.shared_shape().clone(),
                        pose: *c.position(),
                        velocity: Vec3::from_array(rb.linvel().to_array()),
                    });
                }
            }
        }
        let boxes: Vec<_> = shapes
            .iter()
            .map(|s| {
                let aabb = s.shape.compute_aabb(&s.pose);
                let low = s.velocity.min(Vec3::ZERO) * MAX_STEP
                    - Vec3::Y * (GRAVITY * MAX_STEP * MAX_STEP);
                let high = s.velocity.max(Vec3::ZERO) * MAX_STEP;
                Aabb::new(
                    aabb.mins + Vector::from_array(low.to_array()),
                    aabb.maxs + Vector::from_array(high.to_array()),
                )
            })
            .collect();
        Self {
            bvh: Bvh::from_leaves(BvhBuildStrategy::default(), &boxes),
            shapes,
        }
    }
    fn cast(
        &self,
        owner: Owner,
        shape: &SharedShape,
        pose: &Pose,
        delta: Vec3,
        elapsed: f32,
        duration: f32,
    ) -> Result<Option<Hit>> {
        let box_at = shape.compute_aabb(pose);
        let bounds = Aabb::new(
            box_at.mins + Vector::from_array(delta.min(Vec3::ZERO).to_array()),
            box_at.maxs + Vector::from_array(delta.max(Vec3::ZERO).to_array()),
        );
        let mut nearest: Option<Hit> = None;
        for i in self.bvh.intersect_aabb(&bounds) {
            let other = &self.shapes[i as usize];
            if other.owner == Some(owner) {
                continue;
            }
            // Use observed motion. Applying gravity speculatively to every
            // obstacle would also make a supported bottom brick "fall" and
            // let the pieces above pass through its actual resting pose.
            let velocity = other.velocity;
            let movement = velocity * duration;
            let mut other_pose = other.pose;
            other_pose.translation += Vector::from_array((velocity * elapsed).to_array());
            if let Some(hit) = cast_shapes(
                &other_pose,
                Vector::from_array(movement.to_array()),
                other.shape.as_ref(),
                pose,
                Vector::from_array(delta.to_array()),
                shape.as_ref(),
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    stop_at_penetration: false,
                    ..Default::default()
                },
            )
            .map_err(|_| anyhow::anyhow!("Unsupported moving rubble sweep"))?
            {
                let normal = Vec3::from_array((other.pose.rotation * hit.normal1).to_array())
                    .normalize_or(Vec3::Y);
                // Co-moving or separating contact is not a new obstruction.
                if hit.time_of_impact == 0.0 && (delta - movement).dot(normal) >= 0.0 {
                    continue;
                }
                if nearest.is_none_or(|old| hit.time_of_impact < old.fraction) {
                    nearest = Some(Hit {
                        fraction: hit.time_of_impact,
                        normal,
                        surface_velocity: velocity,
                    });
                }
            }
        }
        Ok(nearest)
    }
}

impl Chunk {
    pub(super) fn advance_fallback(
        &mut self,
        frame: &Frame<'_>,
        owner: Owner,
        world: &PhysicsWorld,
        ready: bool,
        work: &mut u32,
    ) -> Result<()> {
        let Frame { dt, actors, scene } = *frame;
        self.debt = (self.debt + dt).min(0.1);
        let Some(shape) = &self.proxy else {
            return Ok(());
        };
        let half = (self.turn * Vec3::X).abs() * self.half.x
            + (self.turn * Vec3::Y).abs() * self.half.y
            + (self.turn * Vec3::Z).abs() * self.half.z;
        let near: Vec<_> = actors
            .iter()
            .filter(|(actor, movement)| {
                let actor_half = (actor.rotation * Vec3::X).abs() * actor.half.x
                    + (actor.rotation * Vec3::Y).abs() * actor.half.y
                    + (actor.rotation * Vec3::Z).abs() * actor.half.z;
                !(self.center - actor.center)
                    .abs()
                    .cmpgt(half + actor_half + movement.abs() + Vec3::splat(0.1))
                    .any()
            })
            .collect();
        let cost = crate::local_physics::COLLISION_QUERY_COST;
        // Reserve enough for a completed step, returning unused work to the
        // shared pool naturally. Never pause midway over a moving snapshot.
        let required = (2 * near.len() as u32 + 2 * (SLIDES + 1)) * cost;
        if !ready || *work < required || self.debt <= 0.0 {
            return Ok(());
        }
        let step = self.debt.min(MAX_STEP);
        let mut velocity = self.velocity - Vec3::Y * (GRAVITY * step);
        velocity *= (-0.15 * step).exp();
        let pose = Pose::from_parts(
            Vector::from_array(self.center.to_array()),
            Rotation::from_array(self.turn.to_array()),
        );
        let mut separation = Vec3::ZERO;
        for (actor, movement) in near {
            *work -= cost;
            let actor_pose = Pose::from_parts(
                Vector::from_array(actor.center.to_array()),
                Rotation::from_array(actor.rotation.to_array()),
            );
            let actor_shape = actor
                .shape
                .clone()
                .unwrap_or_else(|| SharedShape::cuboid(actor.half.x, actor.half.y, actor.half.z));
            let overlap = contact(
                &pose,
                shape.as_ref(),
                &actor_pose,
                actor_shape.as_ref(),
                0.03,
            )
            .map_err(|_| anyhow::anyhow!("Unsupported debris actor contact"))?;
            let response = if let Some(hit) = overlap {
                Some((
                    -Vec3::from_array(hit.normal1.to_array()),
                    (-hit.dist + 0.01).clamped(0., 0.5),
                ))
            } else if movement.length_squared() > 1e-8 {
                *work -= cost;
                let mut from = actor_pose;
                from.translation -= Vector::from_array(movement.to_array());
                cast_shapes(
                    &pose,
                    Vector::ZERO,
                    shape.as_ref(),
                    &from,
                    Vector::from_array(movement.to_array()),
                    actor_shape.as_ref(),
                    ShapeCastOptions {
                        max_time_of_impact: 1.,
                        ..Default::default()
                    },
                )
                .map_err(|_| anyhow::anyhow!("Unsupported debris actor sweep"))?
                .map(|hit| {
                    let away = -Vec3::from_array((pose.rotation * hit.normal1).to_array());
                    (
                        away,
                        ((1. - hit.time_of_impact) * movement.dot(away)).clamped(0., 0.5),
                    )
                })
            } else {
                None
            };
            if let Some((away, depth)) = response {
                let speed = (movement.dot(away) / dt.max(0.001)).clamped(0., 12.);
                let closing = speed - velocity.dot(away);
                if closing > 0. {
                    velocity += away * closing;
                }
                separation += away * depth;
            }
        }

        velocity = velocity.clamp_length_max(MAX_SPEED);
        let mut at = self.center;
        let mut separation = separation.clamp_length_max(MAX_SEPARATION);
        let mut elapsed = 0.0;
        let mut duration = step;
        let mut delta = velocity * step + separation;
        for _ in 0..=SLIDES {
            *work -= cost;
            let pose = Pose::from_parts(
                Vector::from_array(at.to_array()),
                Rotation::from_array(self.turn.to_array()),
            );
            let mut nearest = world
                .query_pipeline_with_filter(QueryFilter::only_fixed().exclude_sensors())
                .cast_shape(
                    &pose,
                    Vector::from_array(delta.to_array()),
                    shape.as_ref(),
                    ShapeCastOptions {
                        max_time_of_impact: 1.0,
                        stop_at_penetration: false,
                        ..Default::default()
                    },
                )
                .map(|(_, hit)| Hit {
                    fraction: hit.time_of_impact,
                    normal: Vec3::from_array(hit.normal1.to_array()).normalize_or(Vec3::Y),
                    surface_velocity: Vec3::ZERO,
                });
            if nearest.is_none_or(|hit| hit.fraction > 0.0) {
                *work -= cost;
                if let Some(hit) = scene.cast(owner, shape, &pose, delta, elapsed, duration)?
                    && nearest.is_none_or(|old| hit.fraction < old.fraction)
                {
                    nearest = Some(hit);
                }
            }
            let Some(hit) = nearest else {
                at += delta;
                break;
            };
            let travel = hit.fraction.clamped(0.0, 1.0);
            at += delta * travel + hit.normal * 0.002;
            let incoming = (velocity - hit.surface_velocity).dot(hit.normal);
            if incoming < 0.0 {
                velocity -= hit.normal * incoming;
            }
            velocity = (hit.surface_velocity + (velocity - hit.surface_velocity) * 0.85)
                .clamp_length_max(MAX_SPEED);
            self.spin *= 0.8;
            elapsed += duration * travel;
            duration *= 1.0 - travel;
            separation *= 1.0 - travel;
            separation -= hit.normal * separation.dot(hit.normal).min(0.0);
            delta = velocity * duration + separation;
            if delta.length_squared() <= 1e-8 {
                break;
            }
        }
        self.center = at;
        self.velocity = velocity;
        self.debt = (self.debt - step).max(0.0);
        Ok(())
    }
}
