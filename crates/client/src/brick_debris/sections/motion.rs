//! Collision-checked translation when the bounded Rapier pool is occupied.
//! This path retains gravity, real compound gaps and actor contacts. It trades
//! angular and simultaneous constraint solving for cheaper shape sweeps, never collision
//! for a frozen render transform. Queries share the existing collision budget.
use super::*;
use bri_sim::grid::{Bounds, CELL, QueryCursor};
use rapier3d::parry::query::{ShapeCastOptions, cast_shapes, contact};
use std::sync::Arc;

type Owner = (u64, u64, usize);
#[derive(Clone, Copy)]
pub(super) struct Frame<'a> {
    pub dt: f32,
    pub building: &'a Building,
    pub dead: &'a BTreeSet<BrickId>,
    pub actors: &'a [(Pusher, Vec3)],
    pub scene: &'a Arc<RubbleScene>,
}
pub(super) struct RubbleScene {
    index: Index,
    shapes: Vec<(Option<Owner>, SharedShape, Pose)>,
}
pub(super) struct Neighbors {
    scene: Arc<RubbleScene>,
    query: QueryCursor,
}
fn bounds(low: Vec3, high: Vec3) -> Bounds {
    let min = std::array::from_fn(|a| (low[a] / CELL[a]).floor() as i32);
    Bounds {
        min,
        size: std::array::from_fn(|a| (high[a] / CELL[a]).ceil() as i32 - min[a] + 1),
    }
}
impl RubbleScene {
    pub(super) fn new(debris: &BrickDebris) -> Arc<Self> {
        let mut scene = Self {
            index: Index::default(),
            shapes: Vec::new(),
        };
        if debris.sections.values().all(|s| s.chunks.is_empty()) {
            return Arc::new(scene);
        }
        let mut insert = |owner, shape: SharedShape, pose| {
            let aabb = shape.compute_aabb(&pose);
            let id = scene.shapes.len() as u64;
            scene.index.insert(
                id,
                bounds(
                    Vec3::from_array(aabb.mins.to_array()),
                    Vec3::from_array(aabb.maxs.to_array()),
                ),
            );
            scene.shapes.push((owner, shape, pose));
        };
        for (key, s) in &debris.sections {
            for (i, c) in s.chunks.iter().enumerate().filter(|(_, c)| !c.released) {
                if let Some(shape) = &c.proxy {
                    let (at, turn) = c.pose(&debris.world);
                    insert(
                        Some((key.0, key.1, i)),
                        shape.clone(),
                        Pose::from_parts(
                            Vector::from_array(at.to_array()),
                            Rotation::from_array(turn.to_array()),
                        ),
                    );
                }
            }
        }
        for b in debris.bodies.values() {
            for collider in debris.world.bodies[b.handle].colliders() {
                let c = &debris.world.colliders[*collider];
                insert(None, c.shared_shape().clone(), *c.position());
            }
        }
        Arc::new(scene)
    }
}

impl Chunk {
    fn start_sweep(
        &mut self,
        building: &Building,
        delta: Vec3,
        scene: &Arc<RubbleScene>,
    ) -> Result<()> {
        self.sweep = Some(building.debris_sweep(
            self.center,
            self.turn,
            delta,
            self.proxy.as_ref().unwrap().clone(),
        )?);
        let (low, high) = self.sweep.as_ref().unwrap().bounds();
        self.neighbors = Some(Neighbors {
            scene: scene.clone(),
            query: QueryCursor::new(bounds(low, high)),
        });
        Ok(())
    }
    pub(super) fn advance_fallback(
        &mut self,
        frame: &Frame<'_>,
        owner: Owner,
        work: &mut u32,
    ) -> Result<()> {
        let Frame {
            dt,
            building,
            dead,
            actors,
            scene,
        } = *frame;
        self.debt = (self.debt + dt).min(0.1);
        if self.sweep.as_ref().is_some_and(|q| q.changed(building)) {
            self.sweep = None;
            self.neighbors = None;
        }
        let cost = crate::local_physics::COLLISION_QUERY_COST;
        if self.sweep.is_none() {
            if *work < cost || self.debt <= 0. {
                return Ok(());
            }
            let Some(shape) = &self.proxy else {
                return Ok(());
            };
            *work -= cost;
            self.sweep_dt = self.debt.min(1. / 30.);
            let step = self.sweep_dt;
            self.velocity.y -= GRAVITY * step;
            self.velocity *= (-0.15 * step).exp();
            let pose = Pose::from_parts(
                Vector::from_array(self.center.to_array()),
                Rotation::from_array(self.turn.to_array()),
            );
            let mut separation = Vec3::ZERO;
            for (actor, movement) in actors {
                if *work < cost * 2 {
                    break;
                }
                let half = (self.turn * Vec3::X).abs() * self.half.x
                    + (self.turn * Vec3::Y).abs() * self.half.y
                    + (self.turn * Vec3::Z).abs() * self.half.z;
                let actor_half = (actor.rotation * Vec3::X).abs() * actor.half.x
                    + (actor.rotation * Vec3::Y).abs() * actor.half.y
                    + (actor.rotation * Vec3::Z).abs() * actor.half.z;
                if (self.center - actor.center)
                    .abs()
                    .cmpgt(half + actor_half + movement.abs() + Vec3::splat(0.1))
                    .any()
                {
                    continue;
                }
                *work -= cost;
                let actor_pose = Pose::from_parts(
                    Vector::from_array(actor.center.to_array()),
                    Rotation::from_array(actor.rotation.to_array()),
                );
                let actor_shape = actor.shape.clone().unwrap_or_else(|| {
                    SharedShape::cuboid(actor.half.x, actor.half.y, actor.half.z)
                });
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
                    let closing = speed - self.velocity.dot(away);
                    if closing > 0. {
                        self.velocity += away * closing;
                    }
                    separation += away * depth;
                }
            }
            self.velocity = self.velocity.clamp_length_max(12.);
            self.slide = 0;
            self.start_sweep(building, self.velocity * step + separation, scene)?;
        }
        loop {
            let sweep = self.sweep.as_mut().unwrap();
            if let Some(neighbors) = &mut self.neighbors {
                loop {
                    if *work < cost {
                        return Ok(());
                    }
                    *work -= 1;
                    match neighbors.query.step(&neighbors.scene.index) {
                        None => {
                            self.neighbors = None;
                            break;
                        }
                        Some(Some(id)) => {
                            let (other, shape, pose) = &neighbors.scene.shapes[id as usize];
                            if *other != Some(owner) {
                                *work -= cost - 1;
                                sweep.test_shape(shape, pose)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            let Some(hit) = building.step_debris_sweep(sweep, work, |id| dead.contains(&id))?
            else {
                return Ok(());
            };
            self.center = sweep.from + sweep.delta * hit.map_or(1., |(t, _)| t.clamped(0., 1.));
            if let Some((travel, normal)) = hit {
                let normal = normal.normalize_or(Vec3::Y);
                self.center += normal * 0.002;
                let incoming = self.velocity.dot(normal);
                if incoming < 0. {
                    self.velocity -= normal * incoming;
                }
                self.velocity *= 0.85;
                self.spin *= 0.8;
                let mut remaining = sweep.delta * (1. - travel.clamped(0., 1.));
                remaining -= normal * remaining.dot(normal).min(0.);
                if self.slide < 3 && remaining.length_squared() > 1e-8 {
                    // A floor contact consumes only the normal movement. Continue
                    // checking tangential travel, including the next wall/corner.
                    self.slide += 1;
                    self.start_sweep(building, remaining, scene)?;
                    continue;
                }
            }
            self.debt = (self.debt - self.sweep_dt).max(0.);
            self.sweep = None;
            return Ok(());
        }
    }
}
