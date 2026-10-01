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
    /// Bricks or ids moved in memory for one unit: a box scan's layer
    /// joining its selection, a copy's bricks moved round its pivot (a
    /// million of either took 4 to 6 ms at once).
    pub const MOVED_PER_UNIT: usize = 32;
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

/// Move what of `builder` `budget` allows round its pivot
/// ([`work::MOVED_PER_UNIT`]); true once it is all moved.
pub fn center_copy(builder: &mut crate::blueprint::CopyBuilder, budget: &mut u32) -> bool {
    let moved = builder.center(*budget as usize * work::MOVED_PER_UNIT);
    *budget -= moved.div_ceil(work::MOVED_PER_UNIT) as u32;
    builder.is_centered()
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
    /// Bricks taken and not yet looked around.
    pub fn queued(&self) -> usize {
        self.selection.bricks.len() - self.next
    }
    /// Look around the bricks taken as far as `budget` allows ([`work`]);
    /// true once the stack is complete or at its limit.
    pub fn step(
        &mut self,
        sim: &Simulation,
        budget: &mut u32,
        mut admit: impl FnMut(BrickId, &Brick) -> bool,
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
                if !admit(other, &world.bricks[&other]) {
                    self.selection.refused += 1;
                    continue;
                }
                self.selection.bricks.push(other);
            }
        }
        Ok(self.done)
    }
}

/// Bricks in one run of a box scan's row.
const RUN: usize = 16 * 1024;

/// A box selection ([`Simulation::select_box`]) worked a slice at a time:
/// the occupied buckets of the box from the bottom up, each brick taken
/// from the bucket holding its lowest corner within the box. A layer of
/// buckets is complete before its bricks join the selection, lowest plate
/// first, so the selection stays lowest first and a limit keeps the
/// lowest bricks. Joining is work like the rest, a slice at a time.
pub struct BoxScan {
    area: Bounds,
    limited: bool,
    limit: usize,
    /// The occupied buckets the box meets, bottom layer first.
    keys: Vec<(i32, i32, i32)>,
    next: usize,
    /// The layer of buckets being looked through.
    layer: Option<i32>,
    /// This layer's bricks by their lowest plate, in runs of at most
    /// [`RUN`]: a row of a million never grows (and is copied) at once.
    rows: BTreeMap<i32, Vec<Vec<BrickId>>>,
    /// Finished layers' rows still joining the selection, lowest first,
    /// and how many of the first have joined.
    joining: std::collections::VecDeque<Vec<BrickId>>,
    joined: usize,
    /// Bricks taken so far, joined or not.
    found: usize,
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
        // Reserved once here (at most the limit), not doubled and copied
        // mid-scan, where one grow of a big selection took a whole tick.
        // Untouched until bricks join it.
        let found: usize = keys
            .iter()
            .map(|&key| sim.index.bucket_bounds(key).len())
            .sum();
        let mut selection = Selection::default();
        selection.bricks.reserve(found.min(limit));
        Self {
            area,
            limited,
            limit,
            keys,
            next: 0,
            layer: None,
            rows: BTreeMap::new(),
            joining: Default::default(),
            joined: 0,
            found: 0,
            selection,
            done: false,
        }
    }
    pub fn is_done(&self) -> bool {
        self.done
    }
    /// Bricks taken so far.
    pub fn found(&self) -> usize {
        self.found
    }
    /// How far through the box's buckets, in percent.
    pub fn searched(&self) -> usize {
        (self.next * 100).checked_div(self.keys.len()).unwrap_or(100)
    }
    /// Look through the box's buckets as far as `budget` allows
    /// ([`work`]); true once the box is done or the limit passed.
    pub fn step(
        &mut self,
        sim: &Simulation,
        budget: &mut u32,
        mut admit: impl FnMut(BrickId, &Brick) -> bool,
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
            if !self.join(budget) || self.done {
                break;
            }
            let Some(&key) = self.keys.get(self.next) else {
                if self.rows.is_empty() {
                    self.done = true;
                } else {
                    self.finish_layer();
                }
                continue;
            };
            if self.layer.is_some_and(|y| y != key.1) {
                self.finish_layer();
                continue;
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
                if !admit(id, &world.bricks[&id]) {
                    self.selection.refused += 1;
                    continue;
                }
                self.found += 1;
                let runs = self.rows.entry(b.min[1]).or_default();
                match runs.last_mut() {
                    Some(run) if run.len() < RUN => run.push(id),
                    _ => runs.push(vec![id]),
                }
            }
        }
        self.done
    }
    /// The layer looked through: its rows wait to join, lowest first.
    fn finish_layer(&mut self) {
        self.joining
            .extend(std::mem::take(&mut self.rows).into_values().flatten());
        self.layer = None;
    }
    /// Finished rows into the selection as far as `budget` allows
    /// ([`work::MOVED_PER_UNIT`]); true once none wait. Past the limit,
    /// the selection is complete.
    fn join(&mut self, budget: &mut u32) -> bool {
        while let Some(row) = self.joining.front() {
            let room = *budget as usize * work::MOVED_PER_UNIT;
            if room == 0 {
                return false;
            }
            let rest = &row[self.joined..];
            let taking = rest.len().min(room);
            self.selection.bricks.extend_from_slice(&rest[..taking]);
            *budget -= taking.div_ceil(work::MOVED_PER_UNIT) as u32;
            self.joined += taking;
            if self.joined == row.len() {
                self.joining.pop_front();
                self.joined = 0;
            }
            if self.selection.bricks.len() > self.limit {
                self.selection.bricks.truncate(self.limit);
                self.selection.limit_reached = true;
                self.joining.clear();
                self.done = true;
                return true;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use bri_world::ContentRef;

    /// A floor of `side` by `side` plates, one row of bricks.
    fn floor(side: usize) -> Simulation {
        let mut world = World::new("Floor".into(), "floor".into(), vec![[1.0; 4]]);
        for x in 0..side {
            for z in 0..side {
                let position = [0.25 + x as f32 * 0.5, 0.1, 0.25 + z as f32 * 0.5];
                let brick = Brick::new(ContentRef::Resolved(testing::PLATE.into()), position, 0);
                world.bricks.insert(world.next_brick_id, brick);
                world.next_brick_id += 1;
            }
        }
        Simulation::new(world, testing::definitions(), vec![]).unwrap()
    }

    #[test]
    fn a_box_scan_joins_a_big_row_a_slice_at_a_time() {
        let sim = floor(200);
        let area = Bounds {
            min: [-10, -10, -10],
            size: [1000, 100, 1000],
        };
        let mut scan = BoxScan::new(&sim, area, false, usize::MAX);
        let budget = 100;
        let mut steps = 0;
        loop {
            let before = scan.selection.bricks.len();
            let mut left = budget;
            let done = scan.step(&sim, &mut left, |_, _| true);
            // A slice: never the 40,000-plate row at once.
            let joined = scan.selection.bricks.len() - before;
            assert!(joined <= budget as usize * work::MOVED_PER_UNIT, "{joined}");
            steps += 1;
            if done {
                break;
            }
        }
        assert_eq!(scan.selection.bricks.len(), 200 * 200);
        assert!(steps > 40_000 / (100 * work::MOVED_PER_UNIT));
        // The same bricks in the same order as all at once.
        let all = sim.select_box(area, false, usize::MAX, |_, _| true);
        assert_eq!(scan.selection.bricks, all.bricks);
        // Cut short at a limit inside a row.
        let limited = sim.select_box(area, false, 12_345, |_, _| true);
        assert_eq!(limited.bricks[..], all.bricks[..12_345]);
        assert!(limited.limit_reached);
    }

    #[test]
    fn a_copy_is_centered_a_slice_at_a_time() {
        let sim = floor(10);
        let mut builder = crate::blueprint::CopyBuilder::new("weapon/x");
        let mut whole = crate::blueprint::CopyBuilder::new("weapon/x");
        for brick in sim.state().bricks.values() {
            builder.push(brick, &sim.definitions).unwrap();
            whole.push(brick, &sim.definitions).unwrap();
        }
        let mut steps = 0;
        loop {
            let mut budget = 1;
            steps += 1;
            if center_copy(&mut builder, &mut budget) {
                break;
            }
            assert_eq!(budget, 0);
            let late = builder.push(&sim.state().bricks[&1], &sim.definitions);
            assert!(late.is_err());
        }
        assert_eq!(steps, 100usize.div_ceil(work::MOVED_PER_UNIT));
        let (sliced, whole) = (builder.finish().unwrap(), whole.finish().unwrap());
        assert_eq!(sliced.origin, whole.origin);
        assert_eq!(
            sliced.bricks.iter().map(|b| b.position).collect::<Vec<_>>(),
            whole.bricks.iter().map(|b| b.position).collect::<Vec<_>>()
        );
    }
}
