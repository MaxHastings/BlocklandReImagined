//! One sight-ray budget for every bot query in a tick (`perception`).
//!
//! Combat sight, watcher polls, arming checks, and the team and surprise
//! lanes' sightlines all ask [`Session::bot_sees`] (or
//! [`Session::bot_sees_player`]) instead of casting rays themselves. The
//! tick's rays are capped at [`SIGHT_RAYS`]: ordinary queries share
//! [`ORDINARY_RAYS`], and each bot has [`TARGET_RAYS`] of its own for
//! checking its current target or attacker, so those always run. An
//! ordinary answer is cached per (viewer, subject) for [`CACHE_TICKS`]
//! while both ends stay put; once the ordinary share is spent, a stale
//! answer (or "unseen") stands in until the next tick.
//!
//! A player is seen at the eye or, failing that, at the chest (a head
//! behind a pole still shows a body). Vehicles with seats occlude too,
//! except the ones the viewer or the subject sit in and the body the viewer
//! is pushing; seatless bodies (balls, crates) do not.
use super::*;
use bri_content::passage::Way;
use rapier3d::prelude::*;

/// Rays one tick may cast for ordinary sight queries, all bots together:
/// scans for enemies, watcher polls, arming and the team and surprise
/// checks.
pub(super) const ORDINARY_RAYS: usize = 128;
/// Rays each bot may cast a tick checking its current target or attacker
/// (eye and chest, each with the vehicle test); beyond them its target
/// queries count as ordinary.
pub(super) const TARGET_RAYS: usize = 4;
/// Bots the target share is sized for (twice the server's 16).
const MOST_BOTS: usize = 32;
/// Every ray a tick may cast: the ordinary share and the target reserve.
pub(super) const SIGHT_RAYS: usize = ORDINARY_RAYS + TARGET_RAYS * MOST_BOTS;
/// An ordinary answer serves this many ticks...
const CACHE_TICKS: u64 = 6;
/// ...while neither end moves more than this.
const CACHE_SLACK: f32 = 0.5;
/// Where the chest is, from the feet toward the eye (a share of the body,
/// so it follows the body's scale).
const CHEST: f32 = 0.55;
/// How far below the eye a shot aims at a body of scale 1: the upper chest.
const AIM_DROP: f32 = 0.5;

/// What is looked at, for the cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::session) enum Subject {
    /// A player's eye.
    Eye(OwnerId),
    /// A player's chest.
    Chest(OwnerId),
    /// A vehicle (it does not hide itself).
    Vehicle(u64),
    /// An item brick.
    Brick(u64),
    /// A dropped item.
    Drop(u64),
}
/// Whether a query may dip into the target reserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::session) enum Urgency {
    /// The bot's current target or attacker.
    Target,
    Ordinary,
}
#[derive(Clone, Copy, Debug)]
struct Cached {
    tick: u64,
    from: Vec3,
    to: Vec3,
    way: Option<Way>,
}
/// The tick's ray ledger and the answer cache.
#[derive(Debug, Default)]
pub(in crate::session) struct Sightlines {
    tick: u64,
    /// Ordinary rays cast so far this tick.
    ordinary: usize,
    /// Each viewer's target rays this tick.
    targeted: BTreeMap<OwnerId, usize>,
    cache: BTreeMap<(OwnerId, Subject), Cached>,
}
impl Sightlines {
    fn begin(&mut self, tick: u64) {
        if tick != self.tick {
            self.tick = tick;
            self.ordinary = 0;
            self.targeted.clear();
            self.cache
                .retain(|_, c| tick < c.tick.saturating_add(4 * CACHE_TICKS));
        }
    }
    /// A cached answer for these ends: fresh only, or any age.
    fn cached(
        &self,
        key: (OwnerId, Subject),
        from: Vec3,
        to: Vec3,
        fresh: bool,
    ) -> Option<Option<Way>> {
        self.cache
            .get(&key)
            .filter(|c| {
                !fresh
                    || (self.tick < c.tick + CACHE_TICKS
                        && c.from.distance(from) <= CACHE_SLACK
                        && c.to.distance(to) <= CACHE_SLACK)
            })
            .map(|c| c.way)
    }
    /// Take `rays` from the tick's budget for `viewer`, if `urgency` may:
    /// a target query from its own share while that lasts, anything else
    /// from the ordinary share.
    fn spend(&mut self, viewer: OwnerId, urgency: Urgency, rays: usize) -> bool {
        if urgency == Urgency::Target
            && (self.targeted.len() < MOST_BOTS || self.targeted.contains_key(&viewer))
        {
            let used = self.targeted.entry(viewer).or_default();
            if *used + rays <= TARGET_RAYS {
                *used += rays;
                return true;
            }
        }
        let ok = self.ordinary + rays <= ORDINARY_RAYS;
        if ok {
            self.ordinary += rays;
        }
        debug_assert!(self.cast() <= SIGHT_RAYS);
        ok
    }
    /// Rays cast so far this tick.
    fn cast(&self) -> usize {
        self.ordinary + self.targeted.values().sum::<usize>()
    }
    fn store(&mut self, key: (OwnerId, Subject), from: Vec3, to: Vec3, way: Option<Way>) {
        self.cache.insert(
            key,
            Cached {
                tick: self.tick,
                from,
                to,
                way,
            },
        );
    }
}

/// Where a shot at a body with its eye at `eye` aims: the upper chest,
/// lower on a giant and higher on a tiny one, so the aim error (an angle)
/// misses a small body more and a big one less.
pub(super) fn aim_point(eye: Vec3, scale: f32) -> Vec3 {
    eye - Vec3::Y * AIM_DROP * scale
}

/// A player is seen at the eye, or else at the chest.
pub(super) fn eye_or_chest(
    mut sees: impl FnMut(Subject, Vec3) -> Option<Way>,
    owner: OwnerId,
    feet: Vec3,
    eye: Vec3,
) -> Option<Way> {
    sees(Subject::Eye(owner), eye).or_else(|| sees(Subject::Chest(owner), feet.lerp(eye, CHEST)))
}

impl Session {
    /// Take `rays` from the tick's ordinary share for a scan of `viewer`'s
    /// own (a look round for a place to go, open sky): false when the share
    /// is spent, and the scan waits for a later tick.
    pub(in crate::session) fn bot_spend_rays(&self, viewer: OwnerId, rays: usize) -> bool {
        let tick = self.simulation.state().tick;
        let mut lines = self
            .bots
            .sightlines
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        lines.begin(tick);
        lines.spend(viewer, Urgency::Ordinary, rays)
    }
    /// Whether `viewer` at `from` sees `to` within `reach`, through the
    /// shared budget (module doc). `subject` names it for the cache; `None`
    /// is never cached. The other lanes' sight checks call this.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::session) fn bot_sees(
        &self,
        viewer: OwnerId,
        subject: Option<Subject>,
        from: Vec3,
        to: Vec3,
        reach: f32,
        urgency: Urgency,
    ) -> Option<Way> {
        let tick = self.simulation.state().tick;
        let mut lines = self
            .bots
            .sightlines
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        lines.begin(tick);
        let key = subject.map(|s| (viewer, s));
        if urgency == Urgency::Ordinary
            && let Some(answer) = key.and_then(|k| lines.cached(k, from, to, true))
        {
            return answer;
        }
        // The world ray, and the vehicle ray on a straight way.
        if !lines.spend(viewer, urgency, 2) {
            return key.and_then(|k| lines.cached(k, from, to, false)).flatten();
        }
        let way = self
            .simulation
            .eyes_see(from, to, reach)
            .filter(|way| way.carry.is_some() || !self.vehicle_between(viewer, subject, from, to));
        if let Some(k) = key {
            lines.store(k, from, to, way);
        }
        way
    }
    /// Whether `viewer` at `from` sees player `owner` within `reach`: at
    /// the eye, or else at the chest.
    pub(in crate::session) fn bot_sees_player(
        &self,
        viewer: OwnerId,
        owner: OwnerId,
        from: Vec3,
        reach: f32,
        urgency: Urgency,
    ) -> Option<Way> {
        let peer = self.peers.get(&owner)?;
        let feet = Vec3::from(peer.player.state().feet);
        eye_or_chest(
            |subject, to| self.bot_sees(viewer, Some(subject), from, to, reach, urgency),
            owner,
            feet,
            peer.player.eye(),
        )
    }
    /// A vehicle stands between `from` and `to`, other than the viewer's
    /// own or the subject's mount, or the body the viewer is pushing.
    fn vehicle_between(
        &self,
        viewer: OwnerId,
        subject: Option<Subject>,
        from: Vec3,
        to: Vec3,
    ) -> bool {
        let tag = |v: u64| super::super::vehicles::VEHICLE_TAG | u128::from(v);
        let own = |o: OwnerId| self.mounted(o).map(|(v, _)| tag(v));
        // The body it is pushing it looks round, as a player would.
        let handled = self
            .bots
            .claims
            .owner_claim(viewer, self.simulation.state().tick)
            .and_then(|c| match c.resource {
                super::claims::Resource::Body { vehicle } => Some(tag(vehicle)),
                _ => None,
            });
        let skip = [
            own(viewer),
            handled,
            match subject {
                Some(Subject::Eye(o) | Subject::Chest(o)) => own(o),
                Some(Subject::Vehicle(v)) => Some(tag(v)),
                _ => None,
            },
        ];
        let to_far = to - from;
        let length = to_far.length() - 0.5;
        if length <= 0.0 {
            return false;
        }
        // Only a vehicle with seats hides what is behind it; a seatless
        // body (a ball, a crate) is looked past, as a player looks over or
        // round one.
        let seated = |data: u128| {
            self.bots
                .objects
                .iter()
                .any(|v| tag(v.id.0) == data && !v.destroyed && !v.seats.is_empty())
        };
        let predicate = |_: ColliderHandle, c: &Collider| {
            c.user_data >> 64 == super::super::vehicles::VEHICLE_TAG >> 64
                && !skip.contains(&Some(c.user_data))
                && seated(c.user_data)
        };
        let direction = to_far.normalize();
        let ray = Ray::new(
            Vector::from_array(from.to_array()),
            Vector::from_array(direction.to_array()),
        );
        self.simulation
            .physics
            .query_pipeline_with_filter(
                QueryFilter::default()
                    .exclude_sensors()
                    .predicate(&predicate),
            )
            .cast_ray(&ray, length, true)
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_stay_under_budget_with_32_bots_and_targets_are_always_checked() {
        let mut lines = Sightlines::default();
        for tick in 1..50u64 {
            lines.begin(tick);
            for bot in 0..32u64 {
                // Every bot scans for others, as many as it likes...
                for _ in 0..40 {
                    lines.spend(bot, Urgency::Ordinary, 2);
                }
                // ...checks its target at eye and chest, whatever the
                // others spent...
                assert!(
                    lines.spend(bot, Urgency::Target, 2),
                    "tick {tick}, bot {bot}"
                );
                assert!(
                    lines.spend(bot, Urgency::Target, 2),
                    "tick {tick}, bot {bot}"
                );
                // ...and asks about a second subject as if urgent.
                lines.spend(bot, Urgency::Target, 2);
            }
            assert!(lines.ordinary <= ORDINARY_RAYS);
            assert!(lines.cast() <= SIGHT_RAYS, "{}", lines.cast());
        }
    }

    #[test]
    fn a_shot_aims_lower_on_a_giant_and_higher_on_a_tiny_body() {
        let eye = Vec3::new(0.0, 10.0, 0.0);
        let drop = |scale: f32| eye.y - aim_point(eye, scale).y;
        assert_eq!(drop(1.0), AIM_DROP);
        assert_eq!(drop(2.0), 2.0 * AIM_DROP);
        assert_eq!(drop(0.5), 0.5 * AIM_DROP);
    }

    #[test]
    fn a_cached_answer_serves_while_both_ends_stay_put() {
        let mut lines = Sightlines::default();
        lines.begin(10);
        let way = Some(Way {
            aim: Vec3::X,
            carry: None,
            length: 1.0,
        });
        let key = (1, Subject::Eye(2));
        lines.store(key, Vec3::ZERO, Vec3::X, way);
        assert_eq!(lines.cached(key, Vec3::ZERO, Vec3::X, true), Some(way));
        assert_eq!(lines.cached(key, Vec3::ZERO, Vec3::X * 3.0, true), None);
        lines.begin(10 + CACHE_TICKS);
        assert_eq!(lines.cached(key, Vec3::ZERO, Vec3::X, true), None);
        assert_eq!(lines.cached(key, Vec3::ZERO, Vec3::X, false), Some(way));
    }

    #[test]
    fn a_head_behind_a_pole_with_the_chest_in_view_is_seen() {
        let floor = rapier3d::prelude::ColliderBuilder::cuboid(50.0, 0.5, 50.0)
            .translation(Vector::new(0.0, -0.5, 0.0));
        // A thin bar across the line between the eyes, at eye height.
        let bar = rapier3d::prelude::ColliderBuilder::cuboid(0.6, 0.15, 0.15)
            .translation(Vector::new(0.0, 2.2, 0.0));
        let world = bri_world::World::new("Sight".into(), "test/map".into(), vec![[1.0; 4]]);
        let sim = crate::simulation::Simulation::new(
            world,
            crate::testing::definitions(),
            vec![floor.clone(), bar],
        )
        .unwrap();
        let from = Vec3::new(0.0, 2.2, -6.0);
        let (feet, eye) = (Vec3::new(0.0, 0.0, 6.0), Vec3::new(0.0, 2.2, 6.0));
        assert!(
            sim.sight(from, eye, 50.0).is_none(),
            "the bar hides the head"
        );
        let seen = eye_or_chest(|_, to| sim.sight(from, to, 50.0), 2, feet, eye);
        assert!(seen.is_some(), "the chest shows below the bar");
        // A full wall hides both.
        let wall = rapier3d::prelude::ColliderBuilder::cuboid(3.0, 3.0, 0.15)
            .translation(Vector::new(0.0, 3.0, 0.0));
        let world = bri_world::World::new("Sight".into(), "test/map".into(), vec![[1.0; 4]]);
        let walled = crate::simulation::Simulation::new(
            world,
            crate::testing::definitions(),
            vec![floor, wall],
        )
        .unwrap();
        assert!(eye_or_chest(|_, to| walled.sight(from, to, 50.0), 2, feet, eye).is_none());
    }
}
