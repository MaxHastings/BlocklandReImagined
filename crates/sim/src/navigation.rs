//! Prepared walking routes, executed directly by the ordinary motor.
use crate::nav::Body;
use bri_content::navigation::WalkingMap;
use glam::Vec3;
use std::{collections::VecDeque, sync::Mutex};
mod detour;

/// Choose where a bounded physical repair rejoins the retained corridor.
pub(crate) fn repair_join(plan: &mut Vec<crate::nav::Waypoint>, feet: Vec3, horizon: f32) -> usize {
    let mut previous = feet;
    let mut remaining = horizon;
    for index in 0..plan.len() {
        let next = plan[index].feet;
        let distance = Vec3::new(next.x - previous.x, 0., next.z - previous.z).length();
        if distance >= remaining && distance > 0. {
            if distance > remaining {
                let join = previous.lerp(next, remaining / distance);
                plan.insert(index, crate::nav::Waypoint::walk(join));
            }
            return index;
        }
        remaining -= distance;
        previous = next;
    }
    plan.len() - 1
}

pub struct Guidance {
    profile: bri_content::navigation::WalkingProfile,
    mesh: Mutex<detour::Mesh>,
}
impl Guidance {
    pub fn new(graph: WalkingMap) -> anyhow::Result<Self> {
        graph.validate()?;
        let mesh = detour::Mesh::new(&graph.mesh, &graph.profile)?;
        Ok(Self {
            profile: graph.profile,
            mesh: Mutex::new(mesh),
        })
    }
    fn supports(&self, b: &Body) -> bool {
        let p = &self.profile;
        !b.conservative
            && b.round == p.round
            && b.wall_step.map(|s| [s.lift, s.retry]) == p.wall_step
            && b.width <= p.width + 1e-4
            && b.height <= p.height + 1e-4
            && b.crouch_height <= p.crouch_height + 1e-4
            && b.step + 1e-4 >= p.step
            && b.crawl_step + 1e-4 >= p.crawl_step
            && b.drop + 1e-4 >= p.drop
            && b.floor_cos <= p.floor_cos + 1e-4
    }
    pub fn route(&self, body: &Body, from: Vec3, goal: Vec3) -> Option<VecDeque<Vec3>> {
        if !self.supports(body) {
            return None;
        }
        self.mesh
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .route(from, goal, body.step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::Waypoint;

    #[test]
    fn a_local_repair_rejoins_within_its_horizon_even_on_a_sparse_corridor() {
        let mut sparse: Vec<_> = [2., 16., 24.]
            .into_iter()
            .map(|x| Waypoint::walk(Vec3::new(x, 0., 0.)))
            .collect();
        let join = repair_join(&mut sparse, Vec3::ZERO, 7.);
        assert!(
            sparse[join].feet.distance(Vec3::ZERO) <= 7.001,
            "rejoin must fit the repair boundary: {:?}",
            sparse[join]
        );
        assert_eq!(sparse[join].feet, Vec3::new(7., 0., 0.));
        assert_eq!(
            sparse.last().unwrap().feet,
            Vec3::new(24., 0., 0.),
            "retain destination"
        );
        assert!(
            sparse[join + 1..]
                .iter()
                .any(|w| w.feet == Vec3::new(16., 0., 0.)),
            "retain the corridor beyond rejoin"
        );
        let mut dense: Vec<_> = (1..=24)
            .map(|x| Waypoint::walk(Vec3::new(x as f32, 0., 0.)))
            .collect();
        let dense_join = repair_join(&mut dense, Vec3::ZERO, 7.);
        assert_eq!(
            dense[dense_join].feet, sparse[join].feet,
            "sampling density does not change the local horizon"
        );
    }

    #[test]
    fn a_local_repair_follows_bends_and_interpolates_the_corridor_height() {
        let mut plan: Vec<_> = [Vec3::new(3., 1., 0.), Vec3::new(3., 5., 8.)]
            .into_iter()
            .map(Waypoint::walk)
            .collect();
        let join = repair_join(&mut plan, Vec3::ZERO, 7.);
        assert_eq!(plan[join].feet, Vec3::new(3., 3., 4.));
        assert_eq!(plan.last().unwrap().feet, Vec3::new(3., 5., 8.));
        let short_join = repair_join(&mut plan, Vec3::ZERO, 20.);
        assert_eq!(short_join, plan.len() - 1);
    }
}
