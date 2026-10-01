//! Selections worked a slice at a time, so a stack or a box of a million
//! bricks is found over many ticks without holding one up: each step does
//! what a share of [`work`] allows and picks up where it stopped. A brick
//! removed meanwhile is passed over; one planted meanwhile may be missed.
use super::*;
use crate::grid::{BUCKET, bucket_span};

/// What copy work costs, in units of a quarter of a microsecond of a
/// release build (measured on copies of 100,000 to 1,000,000 plates; see
/// `docs/progress.md`).
/// A tick's share of copy work is counted in these, so the same work
/// always takes the same ticks.
pub mod work {
    /// One brick a box scan looks at.
    pub const SCAN: u32 = 1;
    /// One brick read or repainted (a capture, a trust check, paint).
    pub const EDIT: u32 = 2;
    /// One brick removed.
    pub const REMOVE: u32 = 16;
    /// One brick a stack selection looks around.
    pub const SEARCH: u32 = 16;
    /// One brick put back as it was.
    pub const RESTORE: u32 = 26;
    /// One brick planted, or checked for planting.
    pub const PLANT: u32 = 22;
    /// One brick an undo breaks, with what it holds up looked at.
    pub const BREAK: u32 = 24;
    /// More for one joined to bricks outside the copy: it breaks on its
    /// own, after a look at what it would leave hanging, and the solid
    /// bricks round it are rebuilt (measured in a world of 500,000).
    pub const CHAIN: u32 = 40;
    /// One brick of a chunk built again because a brick beside it changed
    /// ([`super::Simulation::charge_rebuilds`]).
    pub const REBUILD: u32 = 3;
}

/// Take `cost` from `budget`, or say there is not enough left.
pub fn spend(budget: &mut u32, cost: u32) -> bool {
    if *budget < cost {
        return false;
    }
    *budget -= cost;
    true
}

/// A stack selection ([`Simulation::select_stack`]) worked a slice at a
/// time.
pub struct StackScan {
    start: BrickId,
    reach: StackReach,
    limit: usize,
    bottom: i32,
    top: i32,
    seen: crate::id_map::IdSet,
    /// The next brick taken to look around.
    next: usize,
    pub selection: Selection,
    done: bool,
}
impl StackScan {
    pub fn new(sim: &Simulation, start: BrickId, reach: StackReach, limit: usize) -> Result<Self> {
        ensure!(sim.state().bricks.contains_key(&start), "Unknown brick");
        let first = sim.index.bounds(start);
        Ok(Self {
            start,
            reach,
            limit,
            bottom: first.min[1],
            top: first.max()[1],
            seen: crate::id_map::IdSet::from_iter([start]),
            next: 0,
            selection: Selection {
                bricks: vec![start],
                ..Selection::default()
            },
            done: false,
        })
    }
    pub fn is_done(&self) -> bool {
        self.done
    }
    /// Look around the bricks taken as far as `budget` allows ([`work`]);
    /// true once the stack is complete or at its limit.
    pub fn step(
        &mut self,
        sim: &Simulation,
        budget: &mut u32,
        mut admit: impl FnMut(&Brick) -> bool,
    ) -> Result<bool> {
        let world = sim.state();
        while !self.done {
            let Some(&id) = self.selection.bricks.get(self.next) else {
                self.done = true;
                break;
            };
            if !spend(budget, work::SEARCH) {
                break;
            }
            self.next += 1;
            // Gone since it was taken.
            let Some(here) = sim.index.get(id) else {
                continue;
            };
            for other in sim.connected_bricks(id)? {
                let there = sim.index.bounds(other);
                let above = there.min[1] >= here.max()[1];
                if id == self.start && above != self.reach.up {
                    continue;
                }
                if self.reach.limited
                    && (if self.reach.up {
                        there.min[1] < self.bottom
                    } else {
                        there.max()[1] > self.top
                    })
                {
                    continue;
                }
                if self.seen.contains(other) {
                    continue;
                }
                if self.selection.bricks.len() >= self.limit {
                    self.selection.limit_reached = true;
                    self.done = true;
                    break;
                }
                self.seen.insert(other);
                if !admit(&world.bricks[&other]) {
                    self.selection.refused += 1;
                    continue;
                }
                self.selection.bricks.push(other);
            }
        }
        Ok(self.done)
    }
}

/// A box selection ([`Simulation::select_box`]) worked a slice at a time:
/// the occupied buckets of the box from the bottom up, each brick taken
/// from the bucket holding its lowest corner within the box. A layer of
/// buckets is complete before its bricks join the selection, lowest plate
/// first, so the selection stays lowest first and a limit keeps the
/// lowest bricks.
pub struct BoxScan {
    area: Bounds,
    limited: bool,
    limit: usize,
    /// The occupied buckets the box meets, bottom layer first.
    keys: Vec<(i32, i32, i32)>,
    next: usize,
    /// The layer of buckets being looked through.
    layer: Option<i32>,
    /// This layer's bricks by their lowest plate.
    rows: BTreeMap<i32, Vec<BrickId>>,
    pub selection: Selection,
    done: bool,
}
impl BoxScan {
    pub fn new(sim: &Simulation, area: Bounds, limited: bool, limit: usize) -> Self {
        let (min, max) = bucket_span(area);
        let span = (0..3).fold(1u64, |n, a| {
            n.saturating_mul((i64::from(max[a]) - i64::from(min[a]) + 1).max(0) as u64)
        });
        let within = |&(x, y, z): &(i32, i32, i32)| {
            (min[0]..=max[0]).contains(&x)
                && (min[1]..=max[1]).contains(&y)
                && (min[2]..=max[2]).contains(&z)
        };
        // The box's buckets, or the occupied ones, whichever are fewer.
        let mut keys: Vec<(i32, i32, i32)> = if span <= sim.index.occupied() as u64 {
            (min[1]..=max[1])
                .flat_map(|y| {
                    (min[0]..=max[0]).flat_map(move |x| (min[2]..=max[2]).map(move |z| (x, y, z)))
                })
                .filter(|&key| !sim.index.bucket_bounds(key).is_empty())
                .collect()
        } else {
            sim.index.occupied_keys().filter(within).collect()
        };
        keys.sort_unstable_by_key(|&(x, y, z)| (y, x, z));
        Self {
            area,
            limited,
            limit,
            keys,
            next: 0,
            layer: None,
            rows: BTreeMap::new(),
            selection: Selection::default(),
            done: false,
        }
    }
    pub fn is_done(&self) -> bool {
        self.done
    }
    /// Look through the box's buckets as far as `budget` allows
    /// ([`work`]); true once the box is done or the limit passed.
    pub fn step(
        &mut self,
        sim: &Simulation,
        budget: &mut u32,
        mut admit: impl FnMut(&Brick) -> bool,
    ) -> bool {
        let world = sim.state();
        let area = self.area;
        let outer = area.max();
        let home = |b: Bounds| -> (i32, i32, i32) {
            let corner: [i32; 3] = std::array::from_fn(|a| b.min[a].max(area.min[a]));
            (
                corner[0].div_euclid(BUCKET[0]),
                corner[1].div_euclid(BUCKET[1]),
                corner[2].div_euclid(BUCKET[2]),
            )
        };
        while !self.done {
            let Some(&key) = self.keys.get(self.next) else {
                self.flush_layer();
                self.done = true;
                break;
            };
            if self.layer.is_some_and(|y| y != key.1) {
                self.flush_layer();
                if self.done {
                    break;
                }
            }
            // A whole bucket at a time: at most a few thousand bricks.
            if *budget == 0 {
                break;
            }
            self.layer = Some(key.1);
            let entries = sim.index.bucket_bounds(key);
            *budget = budget.saturating_sub(work::SCAN * (entries.len() as u32 + 1));
            self.next += 1;
            for &(id, b) in entries {
                if b.intersection(area).is_none() || home(b) != key {
                    continue;
                }
                let max = b.max();
                if self.limited
                    && !(0..3).all(|a| b.min[a] >= area.min[a] && max[a] <= outer[a])
                {
                    continue;
                }
                if !admit(&world.bricks[&id]) {
                    self.selection.refused += 1;
                    continue;
                }
                self.rows.entry(b.min[1]).or_default().push(id);
            }
        }
        self.done
    }
    /// The finished layer's bricks into the selection, lowest first; past
    /// the limit, the selection is complete.
    fn flush_layer(&mut self) {
        for (_, ids) in std::mem::take(&mut self.rows) {
            self.selection.bricks.extend(ids);
        }
        if self.selection.bricks.len() > self.limit {
            self.selection.bricks.truncate(self.limit);
            self.selection.limit_reached = true;
            self.done = true;
        }
    }
}
