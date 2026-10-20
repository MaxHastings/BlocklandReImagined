//! Weapon queries against the same native collision world used by players.
use crate::simulation::{Simulation, hit_normal};
use bri_weapons::{
    ActorId, ContactResponse, Filter, Hit, Liquid, Nearby, ProjectileContact, Query, TargetId,
};
use glam::{Quat, Vec3};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;
use std::collections::BTreeMap;

pub struct WeaponQuery<'a> {
    pub simulation: &'a Simulation,
    /// Host-owned gameplay policy. This is never supplied by a packet.
    pub affect: &'a dyn Fn(ActorId, TargetId) -> bool,
    /// Explosion splash policy (adds the minigame's `selfDamage`).
    pub affect_radius: &'a dyn Fn(ActorId, TargetId) -> bool,
    /// Whether the target is the source's teammate or ally in a mini-game
    /// with weapon damage on ([`Query::is_ally`]).
    pub ally: &'a dyn Fn(ActorId, TargetId) -> bool,
    pub catch: &'a dyn Fn(ActorId, ActorId) -> bool,
    /// Zero-delay `onProjectileHit -> Projectile` event rows by brick.
    pub responses: &'a BTreeMap<u64, ContactResponse>,
    pub truncated_targets: usize,
    /// Script-moved map shapes shots can hit (the Tutorial's targets).
    pub shapes: &'a [ShapeTarget],
    /// Lag compensation: where each shooter saw the other bodies. Aim
    /// rays and hitscan shots (no projectile age) meet them there.
    pub rewound: Option<&'a crate::rewind::Rewound>,
}

/// A moving map shape outside the physics world: its collision parts at
/// `transform` (rotation and translation only).
pub struct ShapeTarget {
    pub id: u64,
    pub parts: Vec<SharedShape>,
    pub rotation: Quat,
    pub translation: Vec3,
}
impl ShapeTarget {
    /// The first contact of a ray, as time along it and the surface normal.
    fn cast(&self, ray: &Ray, reach: f32) -> Option<(f32, Vec3)> {
        let pose = Pose::from_parts(
            Vector::from_array(self.translation.to_array()),
            self.rotation,
        );
        self.parts
            .iter()
            .filter_map(|part| part.cast_ray_and_get_normal(&pose, ray, reach, true))
            .map(|hit| (hit.time_of_impact, Vec3::from_array(hit.normal.to_array())))
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }
}

/// How many bodies, one behind another, a ray that misses their bones goes on through.
const MAX_BODY_PASSES: usize = 8;
/// How far past a body's face a ray goes on from, clear of the box.
const PAST_BODY: f32 = 0.001;

fn target(tag: u128) -> Option<TargetId> {
    if tag == u128::MAX {
        Some(TargetId::Map(0))
    } else if tag >> 64 == 1 && tag as u64 != 0 {
        Some(TargetId::Actor(ActorId(tag as u64)))
    } else if tag >> 64 == 2 && tag as u64 != 0 {
        Some(TargetId::Vehicle(tag as u64))
    } else if tag >> 64 == 3 {
        Some(TargetId::Entity(tag as u64))
    } else if tag > 0 && tag <= u128::from(u64::MAX) {
        Some(TargetId::Brick(tag as u64))
    } else {
        None
    }
}

/// `ray_hits` for a brick, which rays test by its own collision.
fn brick_ray_hits(brick: &bri_world::Brick, d: &crate::definitions::Definition) -> bool {
    brick.raycast && d.special != crate::definitions::Special::Water
}

/// Whether projectiles and explosion sight lines hit this collider. For a
/// brick that is v20's Ray Casting setting, the same one tools and clicks
/// obey; Colliding only decides whether bodies pass through. Water bricks are
/// zones, never surfaces. Other sensors are never hit.
fn ray_hits(simulation: &Simulation, collider: &Collider, target: TargetId) -> bool {
    match target {
        TargetId::Brick(id) => simulation.state().bricks.get(&id).is_some_and(|brick| {
            brick.raycast
                && simulation
                    .definitions
                    .get(brick)
                    .is_ok_and(|d| d.special != crate::definitions::Special::Water)
        }),
        _ => !collider.is_sensor(),
    }
}

impl WeaponQuery<'_> {
    /// The shift of `actor`'s body for a shooter who saw it elsewhere (lag
    /// compensation), or none.
    fn rewound_offset(&self, filter: Filter, actor: ActorId) -> Vec3 {
        self.rewound
            .filter(|_| {
                filter.players && !filter.world_only && filter.projectile_age_ticks.is_none()
            })
            .and_then(|r| r.for_shooter(filter.source.0))
            .and_then(|moved| moved.iter().find(|m| m.owner == actor.0))
            .map_or(Vec3::ZERO, |m| m.offset)
    }
    /// The first thing a ray meets, bodies as boxes.
    fn sweep_boxes(&mut self, start: Vec3, end: Vec3, filter: Filter) -> Option<Hit> {
        let delta = end - start;
        let distance = delta.length();
        if !start.is_finite() || !end.is_finite() || !(0.000001..=10000.).contains(&distance) {
            return None;
        }
        let origin = Vector::from_array(start.to_array());
        let simulation = self.simulation;
        // The bodies this shooter saw elsewhere: met there, not here.
        let moved = self
            .rewound
            .filter(|_| {
                filter.players && !filter.world_only && filter.projectile_age_ticks.is_none()
            })
            .and_then(|r| r.for_shooter(filter.source.0))
            .unwrap_or_default();
        let predicate = |_: ColliderHandle, collider: &Collider| {
            let Some(target) = target(collider.user_data) else {
                return false;
            };
            // Bricks by their own collision, below.
            if let TargetId::Brick(_) = target {
                return false;
            }
            if !ray_hits(simulation, collider, target) {
                return false;
            }
            if let TargetId::Entity(_) = target
                && (!filter.players || filter.world_only)
            {
                return false;
            }
            if let TargetId::Actor(actor) = target {
                if !filter.players || filter.world_only {
                    return false;
                }
                if moved.iter().any(|m| m.owner == actor.0) {
                    return false;
                }
                // Aim rays never hit the shooter's body. Projectiles may return
                // after leaving it. This explicit native exit rule still needs
                // original-engine source-grace fidelity calibration.
                if actor == filter.source
                    && (filter.projectile_age_ticks.is_none()
                        || collider.shape().contains_point(collider.position(), origin))
                {
                    return false;
                }
            }
            true
        };
        let direction = delta / distance;
        let ray = Ray::new(origin, Vector::from_array(direction.to_array()));
        let terrain =
            self.simulation
                .terrain_ray(start, direction, distance)
                .map(|(time, normal)| Hit {
                    target: TargetId::Map(0),
                    position: start + direction * time,
                    normal: hit_normal(normal, direction),
                    fraction: time / distance,
                    color: None,
                });
        // A ray never hits the brick it starts inside, so a projectile can
        // leave the brick it spawned in (event `spawnProjectile` starts at
        // the brick's centre).
        let brick = self
            .simulation
            .brick_ray(start, direction, distance, |id, brick, d| {
                brick_ray_hits(brick, d)
                    && !(filter.projectile_age_ticks.is_some()
                        && simulation.brick_contains(id, start))
            })
            .ok()
            .flatten()
            .and_then(|hit| {
                let id = hit.brick?;
                let brick = self.simulation.state().bricks.get(&id)?;
                Some(Hit {
                    target: TargetId::Brick(id),
                    position: hit.position,
                    normal: hit.normal,
                    fraction: hit.distance / distance,
                    color: self
                        .simulation
                        .state()
                        .palette
                        .get(usize::from(brick.color))
                        .map(|c| [c[0], c[1], c[2]]),
                })
            });
        // The nearest brick or terrain hit bounds the physics ray: on a
        // dense build the ray otherwise walks the whole broad phase along
        // its length only to turn down every brick chunk it passes. A hair past
        // it, so a body level with that hit still wins the tie as it did.
        let near = [&brick, &terrain]
            .into_iter()
            .flatten()
            .map(|hit| hit.fraction * distance + 1e-3)
            .fold(distance, f32::min);
        let physical = self
            .simulation
            .physics
            .query_pipeline_with_filter(
                QueryFilter::default()
                    // A bullet meets what lists shots; a projectile in flight, what lists sweeps.
                    .groups(bri_physics::blocks::query_groups(
                        if filter.projectile_age_ticks.is_none() {
                            bri_physics::blocks::SHOT
                        } else {
                            bri_physics::blocks::SWEEP
                        },
                    ))
                    .predicate(&predicate),
            )
            .cast_ray_and_get_normal(&ray, near, true)
            .and_then(|(handle, hit)| {
                let target = target(self.simulation.physics.colliders[handle].user_data)?;
                Some(Hit {
                    target,
                    position: start + direction * hit.time_of_impact,
                    normal: hit_normal(Vec3::from_array(hit.normal.to_array()), direction),
                    fraction: hit.time_of_impact / distance,
                    color: None,
                })
            });
        let near_physical = physical.as_ref().map_or(near, |h| h.fraction * distance);
        let rewound = moved
            .iter()
            .filter_map(|m| {
                let collider = self.simulation.physics.colliders.get(m.collider)?;
                let pose = Pose::from_parts(
                    collider.position().translation + Vector::from_array(m.offset.to_array()),
                    collider.position().rotation,
                );
                let hit =
                    collider
                        .shape()
                        .cast_ray_and_get_normal(&pose, &ray, near_physical, true)?;
                Some((m.owner, hit))
            })
            .min_by(|a, b| a.1.time_of_impact.total_cmp(&b.1.time_of_impact))
            .map(|(owner, hit)| Hit {
                target: TargetId::Actor(ActorId(owner)),
                position: start + direction * hit.time_of_impact,
                normal: hit_normal(Vec3::from_array(hit.normal.to_array()), direction),
                fraction: hit.time_of_impact / distance,
                color: None,
            });
        // Script-moved shapes stop shots and aim rays, not world-only probes.
        let shape = (!filter.world_only)
            .then(|| {
                self.shapes
                    .iter()
                    .filter_map(|s| s.cast(&ray, distance).map(|hit| (s.id, hit)))
                    .min_by(|a, b| a.1.0.total_cmp(&b.1.0))
            })
            .flatten()
            .map(|(id, (time, normal))| Hit {
                target: TargetId::Shape(id),
                position: start + direction * time,
                normal: hit_normal(normal, direction),
                fraction: time / distance,
                color: None,
            });
        [physical, rewound, brick, terrain, shape]
            .into_iter()
            .flatten()
            .min_by(|a, b| a.fraction.total_cmp(&b.fraction))
    }

    /// Line of sight to a radius target through the map and bricks.
    pub fn visible(&mut self, from: Vec3, nearby: &Nearby) -> bool {
        let delta = nearby.center - from;
        let distance = delta.length();
        if !from.is_finite() || !nearby.center.is_finite() || !distance.is_finite() {
            return false;
        }
        if distance < 0.000001 {
            return true;
        }
        let simulation = self.simulation;
        // Bricks by their own collision (`brick_ray`); the map here.
        let predicate = |_: ColliderHandle, collider: &Collider| {
            matches!(
                target(collider.user_data),
                Some(t @ TargetId::Map(_)) if ray_hits(simulation, collider, t)
            )
        };
        let direction = delta / distance;
        let advance = 0.001_f32.min(distance * 0.5);
        let ray = Ray::new(
            Vector::from_array((from + direction * advance).to_array()),
            Vector::from_array(direction.to_array()),
        );
        let reach = (distance - advance - 0.001).max(0.);
        self.simulation
            .terrain_ray(from + direction * advance, direction, reach)
            .is_none()
            && self
                .simulation
                .physics
                .query_pipeline_with_filter(
                    QueryFilter::default()
                        .groups(bri_physics::blocks::query_groups(bri_physics::blocks::RAY))
                        .predicate(&predicate),
                )
                .cast_ray(&ray, reach, true)
                .is_none()
            && self
                .simulation
                .brick_ray(
                    from + direction * advance,
                    direction,
                    reach,
                    |_, brick, d| brick_ray_hits(brick, d),
                )
                .is_ok_and(|hit| hit.is_none())
    }
}

impl Query for WeaponQuery<'_> {
    fn surface_kind(&mut self, target: TargetId, position: Vec3, normal: Vec3) -> Option<&str> {
        match target {
            TargetId::Map(_) => self.simulation.map_surface_at(position, normal),
            TargetId::Brick(_) => Some(crate::map::BRICK_SURFACE),
            _ => None,
        }
    }
    fn passage(&mut self, start: Vec3, end: Vec3) -> Option<(f32, glam::Affine3A)> {
        self.simulation
            .passages()
            .first(start, end)
            .map(|(passage, t)| (t, passage.carry))
    }
    fn liquid(&mut self, bottom: Vec3, height: f32) -> Option<Liquid> {
        let (water, coverage) = self.simulation.liquid_at(bottom.to_array(), height)?;
        Some(Liquid {
            coverage,
            density: water.density,
            viscosity: water.viscosity,
        })
    }
    fn on_contact(&mut self, contact: &ProjectileContact) -> ContactResponse {
        match contact.target {
            TargetId::Brick(brick) => self
                .responses
                .get(&brick)
                .copied()
                .unwrap_or(ContactResponse::Continue),
            _ => ContactResponse::Continue,
        }
    }
    fn sweep(&mut self, start: Vec3, end: Vec3, filter: Filter) -> Option<Hit> {
        let total = (end - start).length();
        let mut from = start;
        // A body that declares hit bones is met at its bones, not its box:
        // a ray through the box that misses every bone goes on past it.
        for _ in 0..MAX_BODY_PASSES {
            let hit = self.sweep_boxes(from, end, filter)?;
            let measured = |hit: Hit| Hit {
                fraction: if total > 0.0 {
                    (hit.position - start).length() / total
                } else {
                    0.0
                },
                ..hit
            };
            let TargetId::Actor(actor) = hit.target else {
                return Some(measured(hit));
            };
            let Some(body) = self.simulation.body_bones(actor.0) else {
                return Some(measured(hit));
            };
            let body = body.shifted(self.rewound_offset(filter, actor));
            let direction = (end - from).normalize_or_zero();
            if let Some((_, time, normal)) = body.first_hit(from, direction, (end - from).length())
            {
                return Some(measured(Hit {
                    position: from + direction * time,
                    normal: hit_normal(normal, direction),
                    ..hit
                }));
            }
            // Past the box: on from where the ray leaves it.
            let inside = hit.position + direction * PAST_BODY;
            let out = body.leave(inside, direction)?;
            from = inside + direction * (out + PAST_BODY);
            if (from - start).length() >= total {
                return None;
            }
        }
        None
    }

    fn sweep_box(
        &mut self,
        start: Vec3,
        end: Vec3,
        half: Vec3,
        rotation: Quat,
        filter: Filter,
    ) -> Option<Hit> {
        let delta = end - start;
        let distance = delta.length();
        if !start.is_finite()
            || !end.is_finite()
            || !half.is_finite()
            || half.min_element() < 0.0
            || !(0.000001..=10000.).contains(&distance)
        {
            return None;
        }
        // Items collide with the world only: map, terrain and bricks (solid
        // bricks in their chunk colliders).
        let predicate = |_: ColliderHandle, collider: &Collider| {
            crate::chunks::is_chunk(collider.user_data)
                || matches!(
                    target(collider.user_data),
                    Some(TargetId::Map(_) | TargetId::Brick(_))
                )
                || (!filter.world_only && target(collider.user_data).is_some())
        };
        let shape = Cuboid::new(Vector::from_array(half.max(Vec3::splat(0.001)).to_array()));
        let pose = Pose::from_parts(Vector::from_array(start.to_array()), rotation);
        let physical = self
            .simulation
            .physics
            .query_pipeline_with_filter(
                QueryFilter::default()
                    .exclude_sensors()
                    .groups(bri_physics::blocks::query_groups(
                        bri_physics::blocks::SWEEP,
                    ))
                    .predicate(&predicate),
            )
            .cast_shape(
                &pose,
                Vector::from_array(delta.to_array()),
                &shape,
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    stop_at_penetration: false,
                    compute_impact_geometry_on_penetration: true,
                    ..Default::default()
                },
            )
            .and_then(|(handle, hit)| {
                let tag = self.simulation.physics.colliders[handle].user_data;
                let target = if crate::chunks::is_chunk(tag) {
                    // A chunk collider: the brick where the box touched it
                    // (chunk colliders sit at the origin, so the witness is
                    // in world space).
                    let witness = Vec3::from_array(hit.witness1.to_array());
                    TargetId::Brick(self.simulation.brick_near(witness)?)
                } else {
                    target(tag)?
                };
                Some(Hit {
                    target,
                    position: start + delta * hit.time_of_impact,
                    normal: hit_normal(Vec3::from_array(hit.normal1.to_array()), delta),
                    fraction: hit.time_of_impact,
                    color: None,
                })
            });
        // Terrain is a heightfield outside Rapier: sweep the box's lowest point.
        let bottom = Vec3::Y * bri_weapons::ItemBounds::lowest(half, rotation);
        let direction = delta / distance;
        let terrain = self
            .simulation
            .terrain_ray(start - bottom, direction, distance)
            .map(|(time, normal)| Hit {
                target: TargetId::Map(0),
                position: start + direction * time,
                normal: hit_normal(normal, direction),
                fraction: time / distance,
                color: None,
            });
        match (physical, terrain) {
            (Some(a), Some(b)) => Some(if b.fraction < a.fraction { b } else { a }),
            (a, b) => a.or(b),
        }
    }

    fn radius(&mut self, center: Vec3, radius: f32, limit: usize) -> Vec<Nearby> {
        if !center.is_finite() || !radius.is_finite() || !(0.0..=10000.).contains(&radius) {
            return Vec::new();
        }
        let point = Vector::from_array(center.to_array());
        let area = Aabb::new(point - Vector::splat(radius), point + Vector::splat(radius));
        let query = self.simulation.physics.query_pipeline();
        let mut found = BTreeMap::new();
        for (_, collider) in query.intersect_aabb_conservative(area) {
            let Some(target @ (TargetId::Actor(_) | TargetId::Vehicle(_) | TargetId::Entity(_))) =
                target(collider.user_data)
            else {
                continue;
            };
            let bounds = collider.compute_aabb();
            let closest = point.clamp(bounds.mins, bounds.maxs);
            let distance = (closest - point).length();
            if distance <= radius {
                found.entry(target).or_insert(Nearby {
                    target,
                    center: Vec3::from_array(bounds.center().to_array()),
                    distance,
                });
            }
        }
        let mut found: Vec<_> = found.into_values().collect();
        found.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then(a.target.cmp(&b.target))
        });
        self.truncated_targets += found.len().saturating_sub(limit);
        found.truncate(limit);
        found
    }

    fn can_affect(&self, source: ActorId, target: TargetId) -> bool {
        (self.affect)(source, target)
    }
    fn can_affect_radius(&self, source: ActorId, target: TargetId) -> bool {
        (self.affect_radius)(source, target)
    }
    fn vehicle_frame(&mut self, id: u64) -> Option<(Vec3, Quat)> {
        let tag = (2u128 << 64) | u128::from(id);
        let physics = &self.simulation.physics;
        let (_, collider) = physics.colliders.iter().find(|(_, c)| c.user_data == tag)?;
        let pose = match collider.parent() {
            Some(body) => *physics.bodies.get(body)?.position(),
            None => *collider.position(),
        };
        Some((Vec3::from_array(pose.translation.to_array()), pose.rotation))
    }
    fn can_catch(&self, source: ActorId, target: ActorId) -> bool {
        (self.catch)(source, target)
    }
    fn is_ally(&self, source: ActorId, target: TargetId) -> bool {
        (self.ally)(source, target)
    }
}
