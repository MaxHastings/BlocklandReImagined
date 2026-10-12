//! Bounded search of unchecked space from dated evidence. This owns no sight,
//! navigation or controls: the existing brain walks each returned destination.
use super::{Knowledge, Vec3, flat};

const PROBES: usize = 4;
const RADIUS: f32 = 3.5;
const ARRIVAL: f32 = 1.25;
/// A probe on a deck is not reached from the floor beneath it, nor in the
/// middle of the jet up to it: within this of its height.
const ARRIVAL_RISE: f32 = 1.5;
const ANCHOR_TICKS: u64 = 360;
const PROBE_TICKS: u64 = 120;

#[derive(Default)]
pub(super) struct State {
    evidence: Option<Knowledge>,
    heading: Vec3,
    points: [Vec3; PROBES + 1],
    next: usize,
    /// When the current destination times out: set as the bot first heads
    /// for it, and again whenever it stops waiting for its route.
    until: Option<u64>,
    expired: bool,
}
impl State {
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// A real observation (or dated allied evidence) replaces the search.
    /// Two observed positions establish direction; unseen transforms never do.
    pub(super) fn observe(&mut self, mut evidence: Knowledge, viewer: Vec3) {
        if let Some(old) = self
            .evidence
            .filter(|old| old.subject == evidence.subject && old.observed == evidence.observed)
        {
            evidence.expires = evidence.expires.min(old.expires);
        }
        let movement = self
            .evidence
            .filter(|old| old.subject == evidence.subject && old.observed < evidence.observed)
            .map_or(Vec3::ZERO, |old| flat(evidence.at - old.at));
        let heading = if movement.length_squared() > 0.01 {
            movement.normalize()
        } else if self
            .evidence
            .is_some_and(|old| old.subject == evidence.subject)
            && self.heading.length_squared() > 0.5
        {
            self.heading
        } else {
            flat(evidence.at - viewer).normalize_or(Vec3::NEG_Z)
        };
        let side = Vec3::new(-heading.z, 0.0, heading.x);
        self.evidence = Some(evidence);
        self.heading = heading;
        self.points = [
            evidence.at,
            evidence.at + heading * RADIUS,
            evidence.at + side * RADIUS,
            evidence.at - side * RADIUS,
            evidence.at - heading * RADIUS,
        ];
        self.next = 0;
        self.until = None;
        self.expired = false;
    }

    /// The bot is still finding its route to the current destination: the
    /// time it is given there starts once it has one (or none), so a
    /// search that waits its turn for route finding is not over before it
    /// takes a step.
    pub(super) fn routing(&mut self) {
        self.until = None;
    }

    /// Whether every destination for this evidence was tried and none
    /// found it: it searched, and has nowhere left to look.
    pub(super) fn exhausted_for(&self, evidence: Knowledge) -> bool {
        !self.expired
            && self.next > PROBES
            && self.evidence.is_some_and(|old| {
                old.subject == evidence.subject
                    && old.observed == evidence.observed
                    && old.at.distance_squared(evidence.at) <= 0.0001
            })
    }

    /// Last-known anchor, then at most four unchecked probes. Failure means
    /// the caller's existing bounded route ended/failed at its current probe.
    /// Repeated calls cannot refresh the evidence's deadline or retry a probe.
    /// A probe is reached within `reach` of it (half a vehicle's footprint
    /// for a driver), and never less than a pedestrian's arrival.
    pub(super) fn next(
        &mut self,
        evidence: Knowledge,
        feet: Vec3,
        tick: u64,
        mut route_failed: bool,
        reach: f32,
    ) -> Option<Vec3> {
        if self.evidence.is_none_or(|old| {
            old.subject != evidence.subject
                || old.observed != evidence.observed
                || old.at.distance_squared(evidence.at) > 0.0001
        }) {
            self.observe(evidence, feet);
            route_failed = false;
        }
        // Keep the earlier expiry if the same evidence was re-shared.
        let expires = self.evidence?.expires.min(evidence.expires);
        if tick >= expires {
            self.expired = true;
            return None;
        }
        while self.next < self.points.len() {
            let point = self.points[self.next];
            let bound = if self.next == 0 {
                ANCHOR_TICKS
            } else {
                PROBE_TICKS
            };
            let until = *self.until.get_or_insert(tick + bound);
            if flat(point - feet).length() <= ARRIVAL.max(reach)
                && (point.y - feet.y).abs() <= ARRIVAL_RISE.max(reach)
                || route_failed
                || tick >= until
            {
                self.next += 1;
                self.until = None;
                route_failed = false;
                continue;
            }
            return Some(point);
        }
        None
    }

    pub(super) fn phase(&self) -> &'static str {
        if self.evidence.is_none() {
            "no dated evidence"
        } else if self.expired {
            "dated evidence expired"
        } else if self.next == 0 {
            "last observed position"
        } else if self.next <= PROBES {
            "unchecked-space probe"
        } else {
            "bounded search exhausted"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn evidence(observed: u64, at: Vec3) -> Knowledge {
        Knowledge {
            subject: 17,
            at,
            observed,
            expires: 1000,
        }
    }
    #[test]
    fn a_probe_counts_as_reached_within_the_searchers_own_reach() {
        let mut s = State::default();
        let e = evidence(1, Vec3::ZERO);
        // Two away: a pedestrian is not there yet; a jeep's side is.
        assert_eq!(s.next(e, Vec3::X * 2.0, 2, false, 0.0), Some(Vec3::ZERO));
        assert_ne!(s.next(e, Vec3::X * 2.0, 3, false, 2.5), Some(Vec3::ZERO));
    }

    #[test]
    fn anchor_then_unchecked_space_uses_only_observed_direction() {
        let mut s = State::default();
        s.observe(evidence(1, Vec3::ZERO), Vec3::Z * 8.0);
        let e = evidence(2, Vec3::X);
        s.observe(e, Vec3::Z * 8.0);
        assert_eq!(s.next(e, Vec3::Z * 8.0, 10, false, 0.0), Some(Vec3::X));
        assert_eq!(s.next(e, Vec3::X, 11, false, 0.0), Some(Vec3::X * 4.5));
        assert_eq!(s.phase(), "unchecked-space probe");
    }
    #[test]
    fn failed_probe_advances_instead_of_restarting() {
        let mut s = State::default();
        let e = evidence(1, Vec3::ZERO);
        let first = s.next(e, Vec3::Z * 8.0, 2, false, 0.0).unwrap();
        let next = s.next(e, Vec3::Z * 8.0, 3, true, 0.0).unwrap();
        assert_ne!(first, next);
        assert_eq!(s.next(e, Vec3::Z * 8.0, 4, false, 0.0), Some(next));
    }
    #[test]
    fn five_failed_destinations_exhaust_without_an_extra_route() {
        let mut s = State::default();
        let e = evidence(1, Vec3::ZERO);
        assert!(s.next(e, Vec3::Z * 8.0, 2, false, 0.0).is_some());
        for tick in 3..7 {
            assert!(s.next(e, Vec3::Z * 8.0, tick, true, 0.0).is_some());
        }
        assert_eq!(s.next(e, Vec3::Z * 8.0, 7, true, 0.0), None);
        assert_eq!(s.next(e, Vec3::Z * 8.0, 8, false, 0.0), None);
        assert_eq!(s.phase(), "bounded search exhausted");
    }
    #[test]
    fn unchanged_failed_routes_time_out_with_a_fixed_bound() {
        let mut s = State::default();
        let mut e = evidence(1, Vec3::ZERO);
        e.expires = 10000;
        assert!(s.next(e, Vec3::Z * 8.0, 2, false, 0.0).is_some());
        for tick in [362, 482, 602, 722] {
            assert!(s.next(e, Vec3::Z * 8.0, tick, false, 0.0).is_some());
        }
        assert_eq!(s.next(e, Vec3::Z * 8.0, 842, false, 0.0), None);
    }
    #[test]
    fn waiting_for_a_route_does_not_use_up_a_destination() {
        let mut s = State::default();
        let mut e = evidence(1, Vec3::ZERO);
        e.expires = 100_000;
        let feet = Vec3::Z * 8.0;
        assert_eq!(s.next(e, feet, 0, false, 0.0), Some(Vec3::ZERO));
        // Its route takes ten seconds to find.
        for tick in 1..1200 {
            s.routing();
            assert_eq!(s.next(e, feet, tick, false, 0.0), Some(Vec3::ZERO));
        }
        // Then it has the whole bound to walk it, from the last tick it
        // waited.
        assert_eq!(s.next(e, feet, 1200, false, 0.0), Some(Vec3::ZERO));
        assert_eq!(s.next(e, feet, 1199 + 359, false, 0.0), Some(Vec3::ZERO));
        assert_ne!(s.next(e, feet, 1199 + 360, false, 0.0), Some(Vec3::ZERO));
    }
    #[test]
    fn exhaustion_is_for_the_evidence_it_searched() {
        let mut s = State::default();
        let e = evidence(1, Vec3::ZERO);
        for tick in 2..8 {
            s.next(e, Vec3::Z * 8.0, tick, tick > 2, 0.0);
        }
        assert!(s.exhausted_for(e));
        assert!(!s.exhausted_for(evidence(9, Vec3::X)));
    }
    #[test]
    fn relayed_evidence_cannot_extend_expiry() {
        let mut s = State::default();
        let mut e = evidence(1, Vec3::ZERO);
        e.expires = 10;
        assert!(s.next(e, Vec3::Z * 8.0, 2, false, 0.0).is_some());
        e.expires = 1000;
        assert_eq!(s.next(e, Vec3::Z * 8.0, 10, false, 0.0), None);
        assert_eq!(s.phase(), "dated evidence expired");
        assert_eq!(s.next(e, Vec3::Z * 8.0, 11, false, 0.0), None);
    }
    #[test]
    fn newer_observation_restarts_with_the_new_anchor() {
        let mut s = State::default();
        let old = evidence(1, Vec3::ZERO);
        s.next(old, Vec3::Z * 8.0, 2, false, 0.0);
        s.next(old, Vec3::Z * 8.0, 3, true, 0.0);
        let new = evidence(4, Vec3::X * 12.0);
        assert_eq!(s.next(new, Vec3::Z * 8.0, 5, true, 0.0), Some(new.at));
        assert_eq!(s.phase(), "last observed position");
    }
}
