//! Trips through the openings of linked bricks (portals): what went
//! through, when and the carry, kept a little while. Whatever follows
//! something across a portal reads them from here, each from where it last
//! looked: holds of the Gravity Gun, and bots after their enemy or carried
//! through themselves.
use super::*;
use bri_package_runtime::ops::ObjectRef;
use glam::Affine3A;
use std::collections::{BTreeMap, VecDeque};

/// Ticks a trip stays on record (a quarter second at most matters).
const KEEP_TICKS: u64 = 120;

/// One trip through an opening.
#[derive(Clone, Copy, Debug)]
pub(super) struct Crossing {
    /// Running count: every trip has a higher number than those before.
    pub number: u64,
    pub tick: u64,
    pub object: ObjectRef,
    pub carry: Affine3A,
}

#[derive(Default)]
pub(super) struct Crossings {
    recent: VecDeque<Crossing>,
    count: u64,
    /// Persistent frames only for live controlled bodies; recent trips may expire.
    frames: BTreeMap<ObjectRef, bri_content::passage::PassageFrame>,
}
impl Crossings {
    /// The number of the last trip so far (0 before any).
    pub fn count(&self) -> u64 {
        self.count
    }
    /// The trips numbered after `seen`, oldest first.
    pub fn since(&self, seen: u64) -> impl Iterator<Item = &Crossing> {
        self.recent.iter().filter(move |c| c.number > seen)
    }
    /// `object`'s last trip on record.
    pub fn last_of(&self, object: ObjectRef) -> Option<&Crossing> {
        self.recent.iter().rev().find(|c| c.object == object)
    }
    pub fn frame(&self, object: ObjectRef) -> bri_content::passage::PassageFrame {
        self.frames.get(&object).copied().unwrap_or_default()
    }
    pub fn forget(&mut self, object: ObjectRef) {
        self.frames.remove(&object);
    }
    fn note(&mut self, tick: u64, object: ObjectRef, carry: Affine3A) {
        if matches!(object, ObjectRef::Player(_) | ObjectRef::Vehicle(_)) {
            self.frames.entry(object).or_default().advance(&carry);
        }
        while self
            .recent
            .front()
            .is_some_and(|c| c.tick + KEEP_TICKS < tick)
        {
            self.recent.pop_front();
        }
        self.count += 1;
        self.recent.push_back(Crossing {
            number: self.count,
            tick,
            object,
            carry,
        });
    }
}

impl Session {
    /// The current connection's walking frame, independent of renderer geometry.
    pub fn passage_frame(&self, owner: OwnerId) -> bri_content::passage::PassageFrame {
        self.crossings.frame(ObjectRef::Player(owner))
    }
    /// `object` (a player, vehicle or entity) went through an opening.
    pub(super) fn crossed(&mut self, object: ObjectRef, carry: Affine3A) {
        let tick = self.simulation.state().tick;
        self.crossings.note(tick, object, carry);
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;
    #[test]
    fn controlled_frames_outlive_recent_trip_history_and_leave_with_the_object() {
        let mut crossings = Crossings::default();
        let object = ObjectRef::Vehicle(7);
        let carry = Affine3A::from_rotation_translation(
            glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            glam::Vec3::new(4., 0., 8.),
        );
        crossings.note(0, object, carry);
        let frame = crossings.frame(object);
        crossings.note(KEEP_TICKS + 1, ObjectRef::Player(1), Affine3A::IDENTITY);
        assert!(
            crossings.last_of(object).is_none(),
            "recent trip intentionally expired"
        );
        assert_eq!(
            crossings.frame(object),
            frame,
            "canonical frame cannot expire"
        );
        crossings.forget(object);
        assert_eq!(
            crossings.frame(object),
            Default::default(),
            "removed object's frame cannot leak to a new body"
        );
    }
}
