//! Removing brick colliders without refitting the whole broad phase.
//!
//! Taking a collider out of a rapier world makes its next step refit the
//! entire broad-phase tree (removals leave merged flags in their ancestors):
//! 25 ms with a million bricks, for one brick. A few removals instead park
//! their colliders far below the world, which the tree takes as moved leaves
//! (a re-insertion each); a big removal pays the refit once and takes every
//! parked collider out with it. The host's `Simulation` and the client's
//! `CollisionMirror` both remove bricks this way.
use glam::Vec3;
use rapier3d::prelude::*;

/// A removal of at least 1/`BATCH` of the world's colliders takes them out
/// of the world, paying one refit (the tree refits anyway past that share of
/// changed leaves); smaller ones park them.
const BATCH: usize = 16;
/// Once parked colliders reach 1/`CROWDED` of the world's, or `MOST`, the
/// next removal takes them all out.
const CROWDED: usize = 8;
const MOST: usize = 65_536;
/// Where parked colliders wait: far below anything that falls, spaced so the
/// largest brick never overlaps its neighbour.
const DEPTH: f32 = -100_000.0;
const SPACING: f32 = 64.0;

#[derive(Default)]
pub struct Parking {
    parked: Vec<ColliderHandle>,
}

impl Parking {
    /// Take these brick colliders out of play. Bodies sleeping on one wake,
    /// as a removal wakes them.
    pub fn remove(&mut self, physics: &mut PhysicsWorld, handles: &[ColliderHandle]) {
        let colliders = physics.colliders.len();
        let batch = handles.len() * BATCH >= colliders
            || self.parked.len() * CROWDED >= colliders
            || self.parked.len() >= MOST;
        if batch {
            // Each parked collider still holds the shape it had, a whole
            // chunk's: freed here, hundreds of them cost a tick.
            let gone: Vec<_> = handles
                .iter()
                .copied()
                .chain(self.parked.drain(..))
                .filter_map(|handle| physics.remove_collider(handle))
                .collect();
            crate::drop_later::drop_later(gone);
            return;
        }
        for &handle in handles {
            let spot = self.parked.len();
            let collider = &mut physics.colliders[handle];
            let aabb = collider.compute_aabb();
            collider.set_sensor(true);
            collider.user_data = 0;
            collider.set_translation(Vector::new(
                (spot % 1024) as f32 * SPACING,
                DEPTH,
                (spot / 1024) as f32 * SPACING,
            ));
            self.parked.push(handle);
            wake_resting(physics, aabb);
        }
    }
}

/// Wake sleeping bodies within a unit of `aabb`.
pub fn wake_resting(physics: &mut PhysicsWorld, aabb: Aabb) {
    let (min, max) = (
        Vec3::from(aabb.mins.to_array()) - Vec3::splat(1.0),
        Vec3::from(aabb.maxs.to_array()) + Vec3::splat(1.0),
    );
    let resting: Vec<_> = physics
        .bodies
        .iter()
        .filter(|(_, body)| {
            let p = Vec3::from(body.translation().to_array());
            body.is_dynamic() && body.is_sleeping() && p.cmpge(min).all() && p.cmple(max).all()
        })
        .map(|(handle, _)| handle)
        .collect();
    for handle in resting {
        physics.wake_up(handle, true);
    }
}
