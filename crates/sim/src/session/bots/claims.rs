//! Advisory reservations, separate from occupancy and action authority.
use super::OwnerId;
use std::collections::BTreeMap;

const MAX_CLAIMS: usize = 16;
const MAX_FAILURES: usize = 64;
const LEASE: u64 = 360;
const MAX_AGE: u64 = 1800;
const RETRY: u64 = 240;
const PROGRESS: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Resource {
    Seat { vehicle: u64, seat: u8 },
    Body { vehicle: u64 },
}

impl Resource {
    fn conflicts(self, other: Self) -> bool {
        self == other
            || self.vehicle() == other.vehicle()
                && (matches!(self, Self::Body { .. }) || matches!(other, Self::Body { .. }))
    }

    /// Claims coordinate one side. A seat is physically exclusive whoever
    /// wants it, but two opponents moving the same loose body are a contest,
    /// not a reservation: neither side's intention blocks the other's.
    fn contends(self, other: Self, allied: bool) -> bool {
        self.conflicts(other)
            && (allied || matches!(self, Self::Seat { .. }) || matches!(other, Self::Seat { .. }))
    }

    pub(super) fn vehicle(self) -> u64 {
        match self {
            Self::Seat { vehicle, .. } | Self::Body { vehicle } => vehicle,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Claim {
    pub owner: OwnerId,
    pub subject: OwnerId,
    pub resource: Resource,
    pub started: u64,
    pub deadline: u64,
    pub best_distance: f32,
}

#[derive(Default)]
pub(super) struct Claims {
    active: BTreeMap<OwnerId, Claim>,
    failures: BTreeMap<(OwnerId, Resource), u64>,
}

fn valid_distance(distance: f32) -> bool {
    distance.is_finite() && distance >= 0.0
}

impl Claims {
    pub(super) fn owner_claim(&self, owner: OwnerId, tick: u64) -> Option<Claim> {
        self.active
            .get(&owner)
            .copied()
            .filter(|c| tick < c.deadline)
    }

    #[cfg(test)]
    pub(super) fn resource_claim(&self, resource: Resource, tick: u64) -> Option<Claim> {
        self.contending_claim(resource, tick, |_| true)
    }

    /// The live claim that blocks `resource` for a caller to whom `allied`
    /// tells which claimants are on its side.
    pub(super) fn contending_claim(
        &self,
        resource: Resource,
        tick: u64,
        allied: impl Fn(OwnerId) -> bool,
    ) -> Option<Claim> {
        self.active
            .values()
            .copied()
            .find(|c| tick < c.deadline && c.resource.contends(resource, allied(c.owner)))
    }

    /// Live claimants of any resource on this vehicle.
    pub(super) fn claimants_on(
        &self,
        vehicle: u64,
        tick: u64,
    ) -> impl Iterator<Item = OwnerId> + '_ {
        self.active
            .values()
            .filter(move |c| tick < c.deadline && c.resource.vehicle() == vehicle)
            .map(|c| c.owner)
    }

    /// Current claimants, so a caller can classify them as allies first.
    pub(super) fn claimants(&self, tick: u64) -> impl Iterator<Item = OwnerId> + '_ {
        self.active
            .values()
            .filter(move |c| tick < c.deadline)
            .map(|c| c.owner)
    }

    pub(super) fn cooling_down(&self, owner: OwnerId, resource: Resource, tick: u64) -> bool {
        self.failures
            .get(&(owner, resource))
            .is_some_and(|until| tick < *until)
    }

    /// First eligible caller wins; the session chooses deterministic caller
    /// order. Reacquisition neither steals a claim nor renews its lifetime.
    pub(super) fn acquire(
        &mut self,
        owner: OwnerId,
        subject: OwnerId,
        resource: Resource,
        distance: f32,
        tick: u64,
        allied: impl Fn(OwnerId) -> bool,
    ) -> bool {
        self.prune(tick);
        if !valid_distance(distance) || self.cooling_down(owner, resource, tick) {
            return false;
        }
        if let Some(c) = self.active.get(&owner) {
            return c.resource == resource && c.subject == subject;
        }
        if self.active.len() >= MAX_CLAIMS
            || self.contending_claim(resource, tick, allied).is_some()
        {
            return false;
        }
        self.active.insert(
            owner,
            Claim {
                owner,
                subject,
                resource,
                started: tick,
                deadline: tick.saturating_add(LEASE),
                best_distance: distance,
            },
        );
        true
    }

    /// `physical_progress` means confirmed object movement caused by contact,
    /// not attempted input or an object's unrelated velocity. Invalid distance
    /// never renews a lease, even when physical progress is reported.
    pub(super) fn progress(
        &mut self,
        owner: OwnerId,
        distance: f32,
        physical_progress: bool,
        tick: u64,
    ) -> bool {
        self.prune(tick);
        if !valid_distance(distance) {
            return false;
        }
        let Some(c) = self.active.get_mut(&owner) else {
            return false;
        };
        let closer = distance <= c.best_distance - PROGRESS;
        if !closer && !physical_progress {
            return false;
        }
        if closer {
            c.best_distance = distance;
        }
        c.deadline = if physical_progress {
            // Confirmed, new distance reduction by this mover is continuing
            // useful work. A fixed age must not evict a long delivery.
            c.started = tick;
            tick.saturating_add(LEASE)
        } else {
            tick.saturating_add(LEASE)
                .min(c.started.saturating_add(MAX_AGE))
        };
        true
    }

    /// Remember only this owner's failed resource so alternatives remain
    /// available. Capacity evicts earliest expiry, then the ordered key.
    pub(super) fn fail(&mut self, owner: OwnerId, resource: Resource, tick: u64) {
        self.prune(tick);
        if self
            .active
            .get(&owner)
            .is_some_and(|c| c.resource == resource)
        {
            self.release_owner(owner);
        }
        self.remember_failure(owner, resource, tick.saturating_add(RETRY));
    }

    fn remember_failure(&mut self, owner: OwnerId, resource: Resource, until: u64) {
        let key = (owner, resource);
        if !self.failures.contains_key(&key) && self.failures.len() >= MAX_FAILURES {
            let oldest = self
                .failures
                .iter()
                .min_by_key(|(key, until)| (**until, **key))
                .map(|(key, _)| *key)
                .unwrap();
            self.failures.remove(&oldest);
        }
        self.failures.insert(key, until);
    }

    /// Preemption/success releases without punishing an otherwise useful task.
    pub(super) fn release_owner(&mut self, owner: OwnerId) -> Option<Claim> {
        self.active.remove(&owner)
    }

    /// An occupied, removed or invalidated resource releases its claimant.
    pub(super) fn release_resource(&mut self, resource: Resource) -> Option<Claim> {
        let owner = self.active.values().find(|c| c.resource == resource)?.owner;
        self.release_owner(owner)
    }

    pub(super) fn prune(&mut self, tick: u64) {
        let expired: Vec<Claim> = self
            .active
            .values()
            .copied()
            .filter(|c| tick >= c.deadline)
            .collect();
        self.active.retain(|_, c| tick < c.deadline);
        self.failures.retain(|_, until| tick < *until);
        for c in expired {
            let until = c.deadline.saturating_add(RETRY);
            if tick < until {
                self.remember_failure(c.owner, c.resource, until);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn body_manipulation_and_vehicle_crew_conflict_but_distinct_seats_do_not() {
        use super::*;
        let body = Resource::Body { vehicle: 17 };
        let driver = Resource::Seat {
            vehicle: 17,
            seat: 0,
        };
        let gunner = Resource::Seat {
            vehicle: 17,
            seat: 2,
        };
        let elsewhere = Resource::Seat {
            vehicle: 18,
            seat: 0,
        };
        let mut claims = Claims::default();
        assert!(claims.acquire(1, 1, body, 5.0, 10, |_| true));
        assert!(!claims.acquire(2, 2, driver, 5.0, 10, |_| true));
        assert!(!claims.acquire(3, 3, gunner, 5.0, 10, |_| true));
        assert!(claims.acquire(4, 4, elsewhere, 5.0, 10, |_| true));
        claims.release_owner(1);
        assert!(claims.acquire(2, 2, driver, 5.0, 11, |_| true));
        assert!(claims.acquire(3, 3, gunner, 5.0, 11, |_| true));
        assert!(!claims.acquire(1, 1, body, 5.0, 11, |_| true));
        assert_eq!(claims.resource_claim(body, 11).unwrap().owner, 2);
    }

    #[test]
    fn opponents_contest_one_body_while_allies_and_seats_stay_exclusive() {
        use super::*;
        let body = Resource::Body { vehicle: 21 };
        let seat = Resource::Seat {
            vehicle: 21,
            seat: 0,
        };
        // Owners 1 and 2 are one side; 3 is their opponent.
        let side = |o: OwnerId| if o == 3 { 1 } else { 0 };
        let mut claims = Claims::default();
        assert!(claims.acquire(1, 1, body, 5.0, 10, |o| side(o) == side(1)));
        assert!(
            !claims.acquire(2, 2, body, 5.0, 10, |o| side(o) == side(2)),
            "an ally's body intention coordinates the side"
        );
        assert!(
            claims.acquire(3, 3, body, 5.0, 10, |o| side(o) == side(3)),
            "an opponent contests the same body"
        );
        assert!(
            !claims.acquire(2, 2, seat, 5.0, 10, |o| side(o) == side(2)),
            "a seat on a claimed body remains exclusive"
        );
        claims.release_owner(1);
        claims.release_owner(3);
        assert!(claims.acquire(1, 1, seat, 5.0, 11, |o| side(o) == side(1)));
        assert!(
            !claims.acquire(3, 3, seat, 5.0, 11, |o| side(o) == side(3)),
            "physical occupancy intentions never become contests"
        );
    }

    use super::*;

    fn seat(vehicle: u64) -> Resource {
        Resource::Seat { vehicle, seat: 0 }
    }

    #[test]
    fn contenders_cannot_steal_and_each_owner_has_one_task() {
        let mut c = Claims::default();
        assert!(c.acquire(1, 9, seat(10), 8.0, 0, |_| true));
        assert!(!c.acquire(2, 9, seat(10), 1.0, 1, |_| true));
        assert!(!c.acquire(1, 9, seat(11), 1.0, 1, |_| true));
        assert!(!c.acquire(1, 8, seat(10), 1.0, 1, |_| true));
        assert_eq!(c.resource_claim(seat(10), 1).unwrap().owner, 1);
        assert!(c.acquire(1, 9, seat(10), 1.0, 300, |_| true));
        assert_eq!(c.owner_claim(1, 300).unwrap().deadline, LEASE);
        // Rotation/fairness belongs to the caller; after expiry a previous
        // contender can win without retaining priority for the old owner.
        assert!(c.acquire(2, 9, seat(10), 1.0, LEASE, |_| true));
        assert!(!c.acquire(1, 9, seat(10), 1.0, LEASE, |_| true));
    }

    #[test]
    fn only_meaningful_progress_renews_and_expiry_cannot_be_revived() {
        let mut c = Claims::default();
        assert!(c.acquire(1, 9, seat(10), 8.0, 0, |_| true));
        assert!(!c.progress(1, 7.9, false, 100));
        assert_eq!(c.owner_claim(1, 100).unwrap().deadline, LEASE);
        assert!(c.progress(1, 7.75, false, 100));
        assert_eq!(c.owner_claim(1, 100).unwrap().deadline, 100 + LEASE);
        assert!(c.progress(1, 9.0, true, 200));
        assert_eq!(c.owner_claim(1, 200).unwrap().best_distance, 7.75);
        assert!(!c.progress(1, 0.0, true, 200 + LEASE));
        assert!(c.owner_claim(1, 200 + LEASE).is_none());
        assert!(!c.acquire(1, 9, seat(10), 0.0, 200 + LEASE, |_| true));
        assert!(c.acquire(1, 9, seat(10), 0.0, 200 + LEASE + RETRY, |_| true));
    }

    #[test]
    fn useful_physical_progress_survives_old_age_but_idle_claims_expire() {
        let mut c = Claims::default();
        assert!(c.acquire(1, 9, Resource::Body { vehicle: 10 }, 100.0, 0, |_| true));
        for tick in (100..MAX_AGE * 3).step_by(100) {
            assert!(c.progress(1, 100.0, true, tick));
            assert_eq!(c.owner_claim(1, tick).unwrap().deadline, tick + LEASE);
        }
        let last = MAX_AGE * 3 - 100;
        assert!(c.owner_claim(1, last + LEASE - 1).is_some());
        assert!(c.owner_claim(1, last + LEASE).is_none());
    }

    #[test]
    fn preemption_and_resource_invalidation_release_immediately() {
        let mut c = Claims::default();
        assert!(c.acquire(1, 9, seat(10), 8.0, 0, |_| true));
        assert_eq!(c.release_owner(1).unwrap().resource, seat(10));
        assert!(c.acquire(2, 9, seat(10), 1.0, 1, |_| true));
        assert_eq!(c.release_resource(seat(10)).unwrap().owner, 2);
        assert!(c.acquire(1, 9, seat(10), 8.0, 2, |_| true));
        assert!(!c.cooling_down(1, seat(10), 2));
    }

    #[test]
    fn failed_resources_leave_alternatives_and_other_owners_available() {
        let mut c = Claims::default();
        assert!(c.acquire(1, 9, seat(10), 8.0, 0, |_| true));
        c.fail(1, seat(10), 100);
        assert!(c.owner_claim(1, 100).is_none());
        assert!(!c.acquire(1, 9, seat(10), 8.0, 101, |_| true));
        assert!(c.acquire(1, 9, seat(11), 8.0, 101, |_| true));
        assert!(c.acquire(2, 9, seat(10), 8.0, 101, |_| true));
        c.fail(1, seat(12), 102);
        assert_eq!(c.owner_claim(1, 102).unwrap().resource, seat(11));
        c.release_owner(1);
        c.release_owner(2);
        assert!(c.acquire(1, 9, seat(10), 8.0, 100 + RETRY, |_| true));
    }

    #[test]
    fn invalid_distances_cannot_acquire_or_renew() {
        let mut c = Claims::default();
        for distance in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1] {
            assert!(!c.acquire(1, 9, seat(10), distance, 0, |_| true));
        }
        assert!(c.acquire(1, 9, seat(10), 8.0, 0, |_| true));
        for distance in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1] {
            assert!(!c.progress(1, distance, true, 100));
        }
        assert_eq!(c.owner_claim(1, 100).unwrap().deadline, LEASE);
    }

    #[test]
    fn storage_is_bounded_and_pruning_and_eviction_are_deterministic() {
        let mut c = Claims::default();
        for owner in 0..MAX_CLAIMS as u64 {
            assert!(c.acquire(owner, 99, seat(owner), 1.0, 0, |_| true));
        }
        assert!(!c.acquire(99, 99, seat(99), 1.0, 0, |_| true));
        assert!(c.acquire(99, 99, seat(99), 1.0, LEASE, |_| true));
        for vehicle in 0..=MAX_FAILURES as u64 {
            c.fail(1, seat(vehicle), 400);
        }
        assert_eq!(c.failures.len(), MAX_FAILURES);
        assert!(!c.cooling_down(1, seat(0), 400));
        assert!(c.cooling_down(1, seat(1), 400));
        c.prune(400 + RETRY);
        assert!(c.failures.is_empty());
        c.prune(LEASE * 2);
        assert!(c.active.is_empty());
        assert_eq!(seat(3).vehicle(), 3);
        assert_eq!(Resource::Body { vehicle: 4 }.vehicle(), 4);
    }
}
