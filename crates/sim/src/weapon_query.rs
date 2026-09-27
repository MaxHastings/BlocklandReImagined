//! Weapon queries against the same native collision world used by players.
use crate::simulation::Simulation;
use bri_weapons::{ActorId, Filter, Hit, Nearby, Query, TargetId};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;

pub struct WeaponQuery<'a> {
    pub simulation: &'a Simulation,
    /// Host-owned gameplay policy. This is never supplied by a packet.
    pub affect: &'a dyn Fn(ActorId, TargetId) -> bool,
    pub catch: &'a dyn Fn(ActorId, ActorId) -> bool,
    pub truncated_targets: usize,
}

fn target(tag: u128) -> Option<TargetId> {
    if tag == u128::MAX {
        Some(TargetId::Map(0))
    } else if tag >> 64 == 1 && tag as u64 != 0 {
        Some(TargetId::Actor(ActorId(tag as u64)))
    } else if tag > 0 && tag <= u128::from(u64::MAX) {
        Some(TargetId::Brick(tag as u64))
    } else {
        None
    }
}

impl Query for WeaponQuery<'_> {
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
        let ray = Ray::new(origin, Vector::from_array((delta / distance).to_array()));
        self.simulation
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
                    position: start + delta / distance * hit.time_of_impact,
                    normal: Vec3::from_array(hit.normal.to_array()),
                    fraction: hit.time_of_impact / distance,
                    color,
                })
            })
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
            let Some(target @ TargetId::Actor(_)) = target(collider.user_data) else {
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

    fn visible(&mut self, from: Vec3, nearby: &Nearby) -> bool {
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
        self.simulation
            .physics
            .query_pipeline_with_filter(
                QueryFilter::default()
                    .exclude_sensors()
                    .predicate(&predicate),
            )
            .cast_ray(&ray, (distance - advance - 0.001).max(0.), true)
            .is_none()
    }
    fn can_affect(&self, source: ActorId, target: TargetId) -> bool {
        (self.affect)(source, target)
    }
    fn can_catch(&self, source: ActorId, target: ActorId) -> bool {
        (self.catch)(source, target)
    }
}
