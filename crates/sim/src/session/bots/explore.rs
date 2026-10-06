//! Exploring: going to find the game when it knows of no fight.
//!
//! A bot with nothing in sight, nothing remembered and enemies in its game
//! (which every player knows from who is playing) goes looking, as a person
//! does: somewhere it has not been lately, drawn toward where enemies were
//! last known, and not where a teammate is already going. It knows only
//! what it has seen, heard or been told (`Knowledge`), never where an
//! unseen enemy is now. The places it might go are what its eyes give it:
//! a point some way off in each direction, short of the first wall, with
//! floor under it.
use super::*;
use std::collections::BTreeMap;

/// The side of the cells it remembers visiting, in world units.
const CELL: f32 = 16.0;
/// Cells it remembers at most; the stalest is forgotten first.
const CELLS: usize = 256;
/// After this long a visited place is as worth looking at as a new one.
const STALE_SECONDS: f32 = 90.0;
/// Where enemies were last known, at most, and how long such a lead
/// draws it.
const LEADS: usize = 4;
const LEAD_SECONDS: f32 = 120.0;
/// Directions it considers, and how far off each place is, as a share of
/// its sight.
const DIRECTIONS: usize = 12;
const REACH_SHARE: f32 = 0.6;
/// How much a lead adds to a place near it, against staleness's 1; how
/// much a teammate already going near it takes off; and the seeded
/// variety between places alike.
const LEAD_PULL: f32 = 1.0;
const CROWD: f32 = 0.8;
const VARIETY: f32 = 0.3;
/// What a place as far off as it looks adds, against staleness's 1, and
/// what one straight ahead adds (one behind it takes as much off).
const FAR: f32 = 0.6;
const AHEAD: f32 = 0.4;
/// Within this of its place it has arrived.
const ARRIVED: f32 = 3.0;
/// A look round that found nowhere to go is not looked again until it has
/// moved this far or this long has passed.
const RESCAN_MOVE: f32 = 4.0;
const RESCAN_SECONDS: f32 = 2.0;
/// Rays a look round casts: one out each way, one down at each place.
pub(super) const SCAN_RAYS: usize = 2 * DIRECTIONS;

/// What a bot keeps for exploring.
#[derive(Clone, Debug, Default)]
pub(super) struct Explore {
    /// When it was last in each cell.
    visited: BTreeMap<(i32, i32), u64>,
    /// Where enemies were last known, and when.
    leads: Vec<(Vec3, u64)>,
    /// The place it is going to, while it explores.
    pub to: Option<Vec3>,
    /// Where and when a look round last found nowhere to go.
    boxed_in: Option<(Vec3, u64)>,
}

fn cell(at: Vec3) -> (i32, i32) {
    ((at.x / CELL).floor() as i32, (at.z / CELL).floor() as i32)
}

impl Explore {
    /// It is at `feet` at `tick`, knowing `memory`: the cell is visited, a
    /// known enemy is a lead, and a place reached is done.
    pub(super) fn note(&mut self, feet: Vec3, memory: Option<Knowledge>, tick: u64) {
        let here = cell(feet);
        if !self.visited.contains_key(&here)
            && self.visited.len() >= CELLS
            && let Some(stalest) = self
                .visited
                .iter()
                .min_by_key(|(c, at)| (**at, **c))
                .map(|(c, _)| *c)
        {
            self.visited.remove(&stalest);
        }
        self.visited.insert(here, tick);
        if let Some(k) = memory
            && self.leads.last().is_none_or(|(_, at)| *at < k.observed)
        {
            if self.leads.len() >= LEADS {
                self.leads.remove(0);
            }
            self.leads.push((k.at, k.observed));
        }
        if self.to.is_some_and(|to| flat(to - feet).length() < ARRIVED) {
            self.to = None;
        }
    }
    /// How worth looking at a place is: how long since it was there (a
    /// place never visited is new), and how near a recent lead.
    fn worth(&self, at: Vec3, tick: u64) -> f32 {
        let stale = self.visited.get(&cell(at)).map_or(1.0, |seen| {
            (tick.saturating_sub(*seen) as f32 / (STALE_SECONDS * 120.0)).min(1.0)
        });
        let lead = self
            .leads
            .iter()
            .map(|(lead, when)| {
                let fresh = 1.0 - (tick.saturating_sub(*when) as f32 / (LEAD_SECONDS * 120.0));
                let near = 1.0 - (flat(*lead - at).length() / (CELL * 4.0)).min(1.0);
                fresh.max(0.0) * near
            })
            .fold(0.0, f32::max);
        stale + LEAD_PULL * lead
    }
}

impl Session {
    /// Whether enemies play in its game: who is playing every player
    /// knows, not where they are.
    pub(super) fn bot_enemies_about(&self, bot: OwnerId, kind: &BotKind) -> bool {
        self.peers
            .iter()
            .any(|(o, p)| *o != bot && p.combat.alive && self.bot_enemy(bot, kind, *o))
    }

    /// The place to explore toward: of a point some way off in each
    /// direction, short of the first wall and on floor, the one most worth
    /// looking at (`Explore::worth`), less where a teammate already goes.
    /// The one it is going to, while it is still on its way. A look round
    /// spends its rays from the shared budget, and one that found nowhere
    /// is not repeated until it has moved or a while has passed.
    pub(super) fn bot_explore_target(
        &mut self,
        bot: OwnerId,
        feet: Vec3,
        eye: Vec3,
        body: &Body,
        intents: &[(OwnerId, claims::Intent)],
        tick: u64,
    ) -> Option<Vec3> {
        let brain = self.bots.brains.get(&bot)?;
        if let Some(to) = brain.explore.to {
            return Some(to);
        }
        if brain.explore.boxed_in.is_some_and(|(at, when)| {
            flat(at - feet).length() < RESCAN_MOVE
                && tick < when + (RESCAN_SECONDS * 120.0) as u64
        }) || !self.bot_spend_rays(bot, SCAN_RAYS)
        {
            return None;
        }
        let to = self.bot_explore_spot(bot, feet, eye, body, intents, tick);
        let explore = &mut self.bots.brains.get_mut(&bot)?.explore;
        explore.to = to;
        explore.boxed_in = to.is_none().then_some((feet, tick));
        to
    }

    /// One look round for the place most worth going to
    /// (`bot_explore_target`), its rays already spent.
    pub(super) fn bot_explore_spot(
        &mut self,
        bot: OwnerId,
        feet: Vec3,
        eye: Vec3,
        body: &Body,
        intents: &[(OwnerId, claims::Intent)],
        tick: u64,
    ) -> Option<Vec3> {
        let brain = self.bots.brains.get(&bot)?;
        let reach = brain.kind.sight * REACH_SHARE;
        // The way it faces: a person looking about keeps on more often than
        // they turn back the way they came.
        let facing = Vec3::new(brain.yaw.sin(), 0.0, -brain.yaw.cos());
        let turn = self.bots.brains.get_mut(&bot)?.random() * std::f32::consts::TAU;
        let mut best: Option<(f32, Vec3)> = None;
        for i in 0..DIRECTIONS {
            let angle = turn + i as f32 * std::f32::consts::TAU / DIRECTIONS as f32;
            let way = Vec3::new(angle.sin(), 0.0, angle.cos());
            let open = super::super::admin_players::world_ray(&self.simulation, eye, way, reach)
                .map_or(reach, |hit| (hit - 1.0).max(0.0));
            if open < CELL * 0.5 {
                continue;
            }
            let at = feet + way * open;
            let Some(down) = floor_below(&self.simulation, at, body) else {
                continue;
            };
            let at = at - Vec3::Y * (down - 0.5).max(0.0);
            let variety = self.bots.brains.get_mut(&bot)?.random() * VARIETY;
            let brain = &self.bots.brains[&bot];
            let crowd = intents
                .iter()
                .filter(|(_, i)| i.option == Behaviour::Explore as u8)
                .filter_map(|(_, i)| i.place)
                .any(|p| flat(p - at).length() < CELL * 2.0);
            // Farther is more ground opened up, and a walk with a purpose
            // rather than a turn about in the same room.
            let worth = brain.explore.worth(at, tick)
                + FAR * open / reach
                + AHEAD * way.dot(facing)
                - if crowd { CROWD } else { 0.0 }
                + variety;
            if best.is_none_or(|(b, _)| worth > b) {
                best = Some((worth, at));
            }
        }
        best.map(|(_, at)| at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_not_seen_lately_or_near_a_lead_is_worth_more() {
        let mut e = Explore::default();
        e.note(Vec3::ZERO, None, 0);
        let here = Vec3::new(1.0, 0.0, 1.0);
        let away = Vec3::new(100.0, 0.0, 100.0);
        assert!(e.worth(away, 10) > e.worth(here, 10), "new beats just visited");
        assert!(e.worth(here, 120 * 200) > 0.99, "stale again after a while");
        let lead = Knowledge {
            subject: 9,
            at: Vec3::new(-100.0, 0.0, 0.0),
            observed: 5,
            expires: 10,
        };
        e.note(Vec3::ZERO, Some(lead), 6);
        assert!(e.worth(lead.at, 20) > e.worth(away, 20), "a lead draws it");
    }
}
