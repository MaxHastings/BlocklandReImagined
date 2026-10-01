//! Trips through the openings of linked bricks (portals): what went
//! through, when and the carry, kept a little while. Whatever follows
//! something across a portal reads them from here, each from where it last
//! looked: holds of the Gravity Gun, and bots after their enemy or carried
//! through themselves.
use super::*;
use bri_package_runtime::ops::ObjectRef;
use glam::Affine3A;
use std::collections::VecDeque;

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
    fn note(&mut self, tick: u64, object: ObjectRef, carry: Affine3A) {
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
    /// `object` (a player, vehicle or entity) went through an opening.
    pub(super) fn crossed(&mut self, object: ObjectRef, carry: Affine3A) {
        let tick = self.simulation.state().tick;
        self.crossings.note(tick, object, carry);
    }
}
