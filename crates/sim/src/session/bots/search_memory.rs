//! Bounded search of unchecked space from dated evidence. This owns no sight,
//! navigation or controls: the existing brain walks each returned destination.
use super::{Knowledge, Vec3, flat};

const PROBES: usize = 4;
const RADIUS: f32 = 3.5;
const ARRIVAL: f32 = 1.25;
const ANCHOR_TICKS: u64 = 360;
const PROBE_TICKS: u64 = 120;

#[derive(Default)]
pub(super) struct State {
    evidence: Option<Knowledge>,
    heading: Vec3,
    points: [Vec3; PROBES + 1],
    next: usize,
    started: Option<u64>,
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
        self.started = None;
        self.expired = false;
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
            let started = *self.started.get_or_insert(tick);
            let timeout = if self.next == 0 {
                ANCHOR_TICKS
            } else {
                PROBE_TICKS
            };
            if flat(point - feet).length() <= ARRIVAL.max(reach)
                || route_failed
                || tick.saturating_sub(started) >= timeout
            {
                self.next += 1;
                self.started = Some(tick);
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
