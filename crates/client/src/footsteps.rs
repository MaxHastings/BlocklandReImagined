//! Footfall sounds: each body whose archetype has `footsteps` plays one of
//! its sounds every `stride` it walks on the ground, chosen by the surface
//! under its feet (`Building::surface_at`). Cosmetic, on each client.
use bri_world::OwnerId;
use glam::Vec3;
use std::collections::BTreeMap;

/// Each walking body's feet when last seen, the ground covered since its
/// last footfall, and how many it has made.
#[derive(Default)]
pub struct Footfalls {
    walked: BTreeMap<OwnerId, (Vec3, f32, usize)>,
}

/// A body moving further than this between frames teleported or respawned.
const JUMP: f32 = 4.0;

impl Footfalls {
    /// Note `owner`'s feet at `feet`, on the ground or not, for a body
    /// that steps every `stride` units. Returns the footfall's count when
    /// the body has just covered another stride on the ground.
    pub fn advance(
        &mut self,
        owner: OwnerId,
        feet: Vec3,
        grounded: bool,
        stride: f32,
    ) -> Option<usize> {
        let (last, walked, count) = self.walked.entry(owner).or_insert((feet, 0.0, 0));
        let moved = Vec3::new(feet.x - last.x, 0.0, feet.z - last.z).length();
        *last = feet;
        if !grounded || !moved.is_finite() || moved > JUMP || stride.is_nan() || stride <= 0.0 {
            // Off the ground the count starts again, so landing steps first.
            *walked = if grounded { 0.0 } else { stride };
            return None;
        }
        *walked += moved;
        if *walked < stride {
            return None;
        }
        *walked %= stride;
        *count = count.wrapping_add(1);
        Some(*count)
    }
    /// Forget bodies no longer present.
    pub fn retain(&mut self, present: impl Fn(OwnerId) -> bool) {
        self.walked.retain(|owner, _| present(*owner));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_sounds_each_stride_walked_on_the_ground() {
        let mut f = Footfalls::default();
        let mut steps = 0;
        for i in 0..=100 {
            let feet = Vec3::new(i as f32 * 0.1, 0.0, 0.0);
            steps += usize::from(f.advance(1, feet, true, 1.2).is_some());
        }
        assert_eq!(steps, 8, "10 units at 1.2 a stride");
        // Standing still or in the air makes none; landing steps at once.
        let at = Vec3::new(10.0, 0.0, 0.0);
        assert!(f.advance(1, at, true, 1.2).is_none());
        assert!(f.advance(1, at + Vec3::X * 0.5, false, 1.2).is_none());
        assert!(f.advance(1, at + Vec3::X * 0.6, true, 1.2).is_some());
        // A teleport is not a step.
        assert!(f.advance(1, at + Vec3::X * 50.0, true, 1.2).is_none());
    }
}
