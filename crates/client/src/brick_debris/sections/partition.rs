//! Bounded connected region growth, using original brick bounds. No type names.
use super::*;
use bri_sim::grid::{Bounds, Index, QueryCursor};
use std::{
    cmp::Reverse,
    collections::{BTreeSet, BinaryHeap},
};

pub(super) struct Partition {
    index: Index,
    remaining: BTreeSet<usize>,
    frontier: BinaryHeap<Reverse<(u64, usize)>>,
    queued: BTreeSet<usize>,
    current: Vec<usize>,
    scan: Option<(Bounds, QueryCursor)>,
    seed: [i32; 3],
    target: usize,
    pub groups: Vec<Vec<usize>>,
}
pub(super) fn bounds(p: &SectionPiece) -> Bounds {
    let half = (p.turn * Vec3::X).abs() * p.half.x
        + (p.turn * Vec3::Y).abs() * p.half.y
        + (p.turn * Vec3::Z).abs() * p.half.z;
    let lo = p.offset + p.origin - half;
    let hi = p.offset + p.origin + half;
    let min = std::array::from_fn(|a| (lo[a] / bri_sim::grid::CELL[a]).round() as i32);
    let max: [i32; 3] = std::array::from_fn(|a| (hi[a] / bri_sim::grid::CELL[a]).round() as i32);
    Bounds {
        min,
        size: std::array::from_fn(|a| (max[a] - min[a]).max(1)),
    }
}
impl Partition {
    pub fn new(index: Index, remaining: BTreeSet<usize>, target: usize) -> Self {
        Self {
            index,
            remaining,
            frontier: Default::default(),
            queued: Default::default(),
            current: Vec::new(),
            scan: None,
            seed: [0; 3],
            target: target.max(1),
            groups: Vec::new(),
        }
    }
    pub fn step(&mut self, work: &mut u32) -> bool {
        while *work > 0 {
            *work -= 1;
            if let Some((own, scan)) = &mut self.scan {
                match scan.step(&self.index) {
                    None => self.scan = None,
                    Some(Some(id)) => {
                        let id = id as usize;
                        let other = self.index.bounds(id as u64);
                        if self.remaining.contains(&id)
                            && (bri_sim::grid::share_face(*own, other)
                                || own.intersection(other).is_some())
                            && self.queued.insert(id)
                        {
                            let distance = (0..3)
                                .map(|a| {
                                    (i64::from(other.min[a]) - i64::from(self.seed[a]))
                                        .unsigned_abs()
                                })
                                .sum();
                            self.frontier.push(Reverse((distance, id)));
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if self.current.len() >= self.target
                || (self.frontier.is_empty() && !self.current.is_empty())
            {
                self.groups.push(std::mem::take(&mut self.current));
                self.frontier.clear();
                self.queued.clear();
            }
            if self.frontier.is_empty() {
                let Some(&id) = self.remaining.first() else {
                    return true;
                };
                self.seed = self.index.bounds(id as u64).min;
                self.frontier.push(Reverse((0, id)));
                self.queued.insert(id);
            }
            let Reverse((_, id)) = self.frontier.pop().unwrap();
            if !self.remaining.remove(&id) {
                continue;
            }
            self.current.push(id);
            let own = self.index.bounds(id as u64);
            self.scan = Some((own, QueryCursor::new(own.expanded(1))));
        }
        false
    }
}
