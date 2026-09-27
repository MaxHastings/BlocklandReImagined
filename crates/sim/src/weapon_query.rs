//! Weapon queries against the same native collision world used by players.
use crate::simulation::Simulation;
use bri_weapons::{
    ActorId, ContactResponse, Filter, Hit, Nearby, ProjectileContact, Query, TargetId,
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
    pub catch: &'a dyn Fn(ActorId, ActorId) -> bool,
    /// Zero-delay `onProjectileHit -> Projectile` event rows by brick.
    pub responses: &'a BTreeMap<u64, ContactResponse>,
    pub truncated_targets: usize,
}

fn target(tag: u128) -> Option<TargetId> {
    if tag == u128::MAX {
        Some(TargetId::Map(0))
    } else if tag >> 64 == 1 && tag as u64 != 0 {
        Some(TargetId::Actor(ActorId(tag as u64)))
    } else if tag >> 64 == 2 && tag as u64 != 0 {
        Some(TargetId::Vehicle(tag as u64))
    } else if tag > 0 && tag <= u128::from(u64::MAX) {
        Some(TargetId::Brick(tag as u64))
    } else {
        None
    }
}

impl WeaponQuery<'_> {
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
        let predicate = |_: ColliderHandle, collider: &Collider| {
            matches!(
                target(collider.user_data),
                Some(TargetId::Map(_) | TargetId::Brick(_))
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
                        .exclude_sensors()
                        .predicate(&predicate),
                )
                .cast_ray(&ray, reach, true)
                .is_none()
    }
}

impl Query for WeaponQuery<'_> {
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
        let delta = end - start;
        let distance = delta.length();
        if !start.is_finite() || !end.is_finite() || !(0.000001..=10000.).contains(&distance) {
            return None;
        }
        let origin = Vector::from_array(start.to_array());
        let predicate = |_: ColliderHandle, collider: &Collider| {
            let Some(target) = target(collider.user_data) else {
                return false;
            };
            if let TargetId::Actor(actor) = target {
                if !filter.players || filter.world_only {
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
                    normal,
                    fraction: time / distance,
                    color: None,
                });
        let physical = self
            .simulation
            .physics
            .query_pipeline_with_filter(
                QueryFilter::default()
                    .exclude_sensors()
                    .predicate(&predicate),
            )
            .cast_ray_and_get_normal(&ray, distance, true)
            .and_then(|(handle, hit)| {
                let target = target(self.simulation.physics.colliders[handle].user_data)?;
                let color = if let TargetId::Brick(id) = target {
                    self.simulation.state().bricks.get(&id).and_then(|brick| {
                        self.simulation
                            .state()
                            .palette
                            .get(usize::from(brick.color))
                            .map(|c| [c[0], c[1], c[2]])
                    })
                } else {
                    None
                };
                Some(Hit {
                    target,
                    position: start + direction * hit.time_of_impact,
                    normal: Vec3::from_array(hit.normal.to_array()),
                    fraction: hit.time_of_impact / distance,
                    color,
                })
            });
        match (physical, terrain) {
            (Some(a), Some(b)) => Some(if b.fraction < a.fraction { b } else { a }),
            (a, b) => a.or(b),
        }
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
        // Items collide with the world only: map, terrain and bricks.
        let predicate = |_: ColliderHandle, collider: &Collider| {
            matches!(
                target(collider.user_data),
                Some(TargetId::Map(_) | TargetId::Brick(_))
            ) || (!filter.world_only && target(collider.user_data).is_some())
        };
        let shape = Cuboid::new(Vector::from_array(half.max(Vec3::splat(0.001)).to_array()));
        let pose = Pose::from_parts(Vector::from_array(start.to_array()), rotation);
        let physical = self
            .simulation
            .physics
            .query_pipeline_with_filter(
                QueryFilter::default()
                    .exclude_sensors()
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
                Some(Hit {
                    target: target(self.simulation.physics.colliders[handle].user_data)?,
                    position: start + delta * hit.time_of_impact,
                    normal: Vec3::from_array(hit.normal1.to_array()),
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
                normal,
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
            let Some(target @ (TargetId::Actor(_) | TargetId::Vehicle(_))) =
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
    fn can_catch(&self, source: ActorId, target: ActorId) -> bool {
        (self.catch)(source, target)
    }
}
