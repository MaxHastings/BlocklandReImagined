//! Lag compensation for aimed shots. A player's game draws everyone else
//! a little in the past (their poses arrive late and are smoothed), so a
//! shot aimed true on their screen would miss a moving body on the host.
//! Each move a player sends says which host tick they saw the others at
//! (`MoveInput::seen`); the host keeps where every body stood over the
//! last half second, and an aim ray or hitscan shot (never a flying
//! projectile, which lives in the host's present) meets the other bodies
//! where that shooter saw them. Bounded to [`MAX_TICKS`] back, so a slow
//! or lying client cannot reach further into the past.
use bri_world::OwnerId;
use glam::Vec3;
use rapier3d::prelude::ColliderHandle;
use std::collections::{BTreeMap, VecDeque};

/// The furthest back a shot meets other bodies: half a second.
pub const MAX_TICKS: u64 = 60;

/// Where every body stood, tick by tick, for [`MAX_TICKS`].
#[derive(Debug, Default, Clone)]
pub struct History {
    feet: BTreeMap<OwnerId, VecDeque<(u64, Vec3)>>,
}

/// One body to meet elsewhere: its collider, moved by `offset`.
#[derive(Debug, Clone, Copy)]
pub struct Moved {
    pub owner: OwnerId,
    pub collider: ColliderHandle,
    pub offset: Vec3,
}

/// This tick's rewinds: for each shooter who saw the past, the bodies
/// they saw elsewhere than they stand now.
#[derive(Debug, Default, Clone)]
pub struct Rewound {
    by_shooter: BTreeMap<OwnerId, Vec<Moved>>,
}
impl Rewound {
    pub fn for_shooter(&self, shooter: OwnerId) -> Option<&[Moved]> {
        self.by_shooter.get(&shooter).map(Vec::as_slice)
    }
    pub fn is_empty(&self) -> bool {
        self.by_shooter.is_empty()
    }
}

impl History {
    /// Note where each body stands at `tick`; bodies not given are
    /// forgotten.
    pub fn record(&mut self, tick: u64, bodies: impl IntoIterator<Item = (OwnerId, Vec3)>) {
        let mut seen = Vec::new();
        for (owner, feet) in bodies {
            if !feet.is_finite() {
                continue;
            }
            seen.push(owner);
            let track = self.feet.entry(owner).or_default();
            // A body back from elsewhere (a respawn) starts a new track.
            if track.back().is_some_and(|(t, _)| *t >= tick) {
                track.clear();
            }
            track.push_back((tick, feet));
            while track
                .front()
                .is_some_and(|(t, _)| tick.saturating_sub(*t) > MAX_TICKS)
            {
                track.pop_front();
            }
        }
        self.feet.retain(|owner, _| seen.contains(owner));
    }
    /// Where `owner` stood at `tick`, or at the nearest tick kept after it.
    pub fn feet_at(&self, owner: OwnerId, tick: u64) -> Option<Vec3> {
        let track = self.feet.get(&owner)?;
        track
            .iter()
            .find(|(t, _)| *t >= tick)
            .or(track.back())
            .map(|(_, feet)| *feet)
    }
    /// The bodies each shooter saw elsewhere, at `now`: `shooters` gives
    /// each the tick they saw at (0 for not known), `bodies` each body's
    /// collider and where it stands now.
    pub fn rewound(
        &self,
        now: u64,
        shooters: impl IntoIterator<Item = (OwnerId, u64)>,
        bodies: &[(OwnerId, ColliderHandle, Vec3)],
    ) -> Rewound {
        let mut out = Rewound::default();
        for (shooter, seen) in shooters {
            if seen == 0 || seen >= now {
                continue;
            }
            let at = now - (now - seen).min(MAX_TICKS);
            let moved: Vec<Moved> = bodies
                .iter()
                .filter(|(owner, ..)| *owner != shooter)
                .filter_map(|(owner, collider, feet)| {
                    let offset = self.feet_at(*owner, at)? - *feet;
                    (offset.length_squared() > 1e-6).then_some(Moved {
                        owner: *owner,
                        collider: *collider,
                        offset,
                    })
                })
                .collect();
            if !moved.is_empty() {
                out.by_shooter.insert(shooter, moved);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shooter_meets_bodies_where_they_saw_them_within_half_a_second() {
        let mut history = History::default();
        let handle = ColliderHandle::from_raw_parts(1, 0);
        // A runner moving one unit a tick along x.
        for tick in 1..=100 {
            history.record(
                tick,
                [(7, Vec3::new(tick as f32, 0.0, 0.0)), (3, Vec3::ZERO)],
            );
        }
        let now = Vec3::new(100.0, 0.0, 0.0);
        let bodies = [(7, handle, now), (3, handle, Vec3::ZERO)];
        // Seen 10 ticks ago: ten units back. The shooter itself and a body
        // that stood still are not moved.
        let r = history.rewound(100, [(3, 90)], &bodies);
        let moved = r.for_shooter(3).unwrap();
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].owner, 7);
        assert!((moved[0].offset - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);
        // A claim older than half a second reaches back only that far.
        let r = history.rewound(100, [(3, 1)], &bodies);
        let back = -r.for_shooter(3).unwrap()[0].offset.x;
        assert!((back - MAX_TICKS as f32).abs() <= 1.0, "{back}");
        // Not known, or from the future: nothing moves.
        assert!(history.rewound(100, [(3, 0), (5, 200)], &bodies).is_empty());
    }
}
