//! A brick too big for a blast to knock out whole splits along its stud
//! grid instead: as if it had been built from the biggest bricks that blast
//! can break. The blast takes the pieces it reaches that it could break; the
//! rest stand in the brick's place as ordinary bricks while the brick waits,
//! hidden, to come back whole when it respawns.
//!
//! The split halves the brick again and again, the longest side first,
//! only where the blast reaches, so it is fine near the blow and coarse
//! away from it. Every piece is a plain brick of the catalog at exactly its
//! size, so pieces look and behave like bricks anyone could plant; the
//! pieces and what the blast took fill the brick exactly, nothing more.
//! Cuts across the brick take the edge of a brick standing on or under it
//! when one is near the middle, so a floor does not split under the middle
//! of a wall.
use bri_world::{Brick, BrickId};
use glam::Vec3;
use std::collections::BTreeMap;

use crate::definitions::Definitions;
use crate::grid::Bounds;

/// World size of one grid cell: a stud across, a plate up.
const CELL: [f32; 3] = [0.5, 0.2, 0.5];
/// Plates in a brick's height: tall cuts fall between brick heights.
const BRICK_PLATES: i32 = 3;

/// The plain bricks of the catalog by their size on the grid as placed
/// (studs along x, plates up, studs along z): the definition and the
/// quarter turns that give that size.
pub(crate) struct Pieces(BTreeMap<[i32; 3], (String, u8)>);
impl Pieces {
    /// Every plain box (`Definition::is_plain_box`) a player could break,
    /// without a print; for each size the first by id, so every host picks
    /// the same.
    pub(crate) fn new(definitions: &Definitions) -> Self {
        use bri_content::brick::Surface;
        let mut sizes = BTreeMap::new();
        for (id, d) in &definitions.entries {
            if !d.is_plain_box()
                || d.indestructible
                || d.mesh.quads.iter().any(|q| q.surface == Surface::Print)
            {
                continue;
            }
            let [w, depth] = d.mesh.footprint_studs.map(|v| v as i32);
            let h = d.mesh.height_plates as i32;
            for (size, turns) in [([w, h, depth], 0u8), ([depth, h, w], 1)] {
                sizes.entry(size).or_insert_with(|| (id.clone(), turns));
            }
        }
        Self(sizes)
    }
    fn get(&self, size: [i32; 3]) -> Option<&(String, u8)> {
        self.0.get(&size)
    }
}

/// What reaches the brick: the blast's middle and reach in world units, and
/// the biggest brick it can break (studs x studs x plates, as `canExplode`
/// counts it).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Reach {
    pub origin: Vec3,
    pub radius: f32,
    pub max_volume: f32,
}

/// One cell of the split, on the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    pub min: [i32; 3],
    pub size: [i32; 3],
}
impl Cell {
    fn volume(&self) -> i64 {
        self.size.iter().map(|&v| i64::from(v)).product()
    }
    fn world_box(&self) -> (Vec3, Vec3) {
        let low = Vec3::from_array(std::array::from_fn(|a| self.min[a] as f32 * CELL[a]));
        let high = Vec3::from_array(std::array::from_fn(|a| {
            (self.min[a] + self.size[a]) as f32 * CELL[a]
        }));
        (low, high)
    }
    pub(crate) fn center(&self) -> Vec3 {
        let (low, high) = self.world_box();
        (low + high) * 0.5
    }
    fn reached(&self, reach: &Reach) -> bool {
        let (low, high) = self.world_box();
        reach.origin.clamp(low, high).distance(reach.origin) <= reach.radius
    }
    /// The plain brick that fills this cell, at its place.
    pub(crate) fn brick(&self, pieces: &Pieces, like: &Brick) -> Option<Brick> {
        let (id, turns) = pieces.get(self.size)?;
        let mut brick = Brick::new(
            bri_world::ContentRef::Resolved(id.clone()),
            self.center().to_array(),
            like.owner,
        );
        brick.quarter_turns = *turns;
        brick.color = like.color;
        brick.color_effect = like.color_effect;
        brick.shape_effect = like.shape_effect;
        Some(brick)
    }
}

/// How a brick splits: the pieces left standing and the cells the blast
/// takes.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Split {
    pub standing: Vec<Cell>,
    pub taken: Vec<Cell>,
}

/// Split the brick filling `whole` for `reach`: None when it cannot (no
/// plain brick fits some piece, or more than `most` pieces would stand) or
/// when the blast takes none of it. `translucent` bricks count a quarter of
/// their volume, as `canExplode` counts them. `edges` are the grid lines,
/// along x and along z, where bricks standing on or under it end.
pub(crate) fn split(
    whole: Bounds,
    reach: &Reach,
    translucent: bool,
    pieces: &Pieces,
    edges: [&[i32]; 2],
    most: usize,
) -> Option<Split> {
    let mut out = Split::default();
    let mut todo = vec![Cell {
        min: whole.min,
        size: whole.size,
    }];
    while let Some(cell) = todo.pop() {
        let reached = cell.reached(reach);
        let volume = if translucent {
            (cell.volume() as f32 * 0.25).floor()
        } else {
            cell.volume() as f32
        };
        if reached && volume <= reach.max_volume && pieces.get(cell.size).is_some() {
            out.taken.push(cell);
            continue;
        }
        if !reached && pieces.get(cell.size).is_some() {
            out.standing.push(cell);
            if out.standing.len() > most {
                return None;
            }
            continue;
        }
        let (a, b) = halves(cell, edges)?;
        // Popped last-in first: the half nearer the blast splits first.
        todo.push(b);
        todo.push(a);
    }
    (!out.taken.is_empty()).then_some(out)
}

/// `cell` cut in two across its longest side: at the middle, or at the
/// edge of a brick on or under it within a quarter of the side from the
/// middle; a cut up a tall cell falls between brick heights where it can.
/// None for a single grid cell.
fn halves(cell: Cell, edges: [&[i32]; 2]) -> Option<(Cell, Cell)> {
    let axis = (0..3)
        .filter(|&a| cell.size[a] > 1)
        .max_by(|&a, &b| {
            (cell.size[a] as f32 * CELL[a])
                .total_cmp(&(cell.size[b] as f32 * CELL[b]))
                .then(b.cmp(&a))
        })?;
    let (low, length) = (cell.min[axis], cell.size[axis]);
    let middle = low + length / 2;
    let cut = match axis {
        1 if length > BRICK_PLATES => {
            let course = low + (length / 2 / BRICK_PLATES).max(1) * BRICK_PLATES;
            if course < low + length { course } else { middle }
        }
        1 => middle,
        _ => edges[axis / 2]
            .iter()
            .copied()
            .filter(|&e| e > low && e < low + length && (e - middle).abs() * 4 <= length)
            .min_by_key(|&e| ((e - middle).abs(), e))
            .unwrap_or(middle),
    };
    let mut a = cell;
    let mut b = cell;
    a.size[axis] = cut - low;
    b.min[axis] = cut;
    b.size[axis] = low + length - cut;
    Some((a, b))
}

/// Where the bricks `around` end along x and z, for [`split`]'s cuts.
pub(crate) fn edges(around: impl IntoIterator<Item = Bounds>) -> [Vec<i32>; 2] {
    let mut out = [Vec::new(), Vec::new()];
    for b in around {
        for (i, axis) in [0, 2].into_iter().enumerate() {
            out[i].push(b.min[axis]);
            out[i].push(b.min[axis] + b.size[axis]);
        }
    }
    for e in &mut out {
        e.sort_unstable();
        e.dedup();
    }
    out
}

/// Bricks the engine put in a split brick's place, by the brick they stand
/// in for, and back.
#[derive(Default)]
pub(crate) struct Fragments {
    by_whole: BTreeMap<BrickId, Vec<BrickId>>,
    whole_of: BTreeMap<BrickId, BrickId>,
}
impl Fragments {
    pub(crate) fn add(&mut self, whole: BrickId, pieces: &[BrickId]) {
        self.by_whole.entry(whole).or_default().extend(pieces);
        for &p in pieces {
            self.whole_of.insert(p, whole);
        }
    }
    /// The brick `piece` stands in for, if it is a piece.
    pub(crate) fn whole_of(&self, piece: BrickId) -> Option<BrickId> {
        self.whole_of.get(&piece).copied()
    }
    pub(crate) fn is_piece(&self, brick: BrickId) -> bool {
        self.whole_of.contains_key(&brick)
    }
    /// Forget a piece that is gone.
    pub(crate) fn forget(&mut self, piece: BrickId) {
        if let Some(whole) = self.whole_of.remove(&piece)
            && let Some(list) = self.by_whole.get_mut(&whole)
        {
            list.retain(|&p| p != piece);
        }
    }
    /// The pieces standing in for `whole`, forgotten: it is coming back.
    pub(crate) fn take(&mut self, whole: BrickId) -> Vec<BrickId> {
        let pieces = self.by_whole.remove(&whole).unwrap_or_default();
        for p in &pieces {
            self.whole_of.remove(p);
        }
        pieces
    }
    pub(crate) fn pieces(&self) -> impl Iterator<Item = BrickId> + '_ {
        self.whole_of.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plain bricks of every size up to `most` studs and plates.
    fn every(most: i32) -> Pieces {
        let mut sizes = BTreeMap::new();
        for x in 1..=most {
            for y in 1..=most {
                for z in 1..=most {
                    sizes.insert([x, y, z], (format!("{x}x{z}x{y}"), 0));
                }
            }
        }
        Pieces(sizes)
    }
    /// Cubes and plates only, as most catalogs have: 1x1 plates and bricks,
    /// and NxN cubes N plates... and 2^k x 2^k plates.
    fn sparse() -> Pieces {
        let mut sizes = BTreeMap::new();
        let mut n = 1;
        while n <= 64 {
            sizes.insert([n, 1, n], (format!("{n}x{n}f"), 0));
            sizes.insert([n, 3 * n, n], (format!("{n}xcube"), 0));
            n *= 2;
        }
        sizes.insert([1, 3, 1], ("1x1".into(), 0));
        Pieces(sizes)
    }
    fn cube(n: i32) -> Bounds {
        Bounds {
            min: [0, 0, 0],
            size: [n, 3 * n, n],
        }
    }
    fn volume(cells: &[Cell]) -> i64 {
        cells.iter().map(Cell::volume).sum()
    }

    #[test]
    fn the_pieces_and_what_the_blast_took_fill_the_brick_exactly() {
        let whole = cube(16);
        let reach = Reach {
            origin: Vec3::new(1.0, 2.0, 0.0),
            radius: 2.0,
            max_volume: 30.0,
        };
        for pieces in [every(48), sparse()] {
            let split = split(whole, &reach, false, &pieces, [&[], &[]], 10_000).unwrap();
            assert_eq!(
                volume(&split.standing) + volume(&split.taken),
                16 * 48 * 16,
                "nothing made, nothing lost"
            );
            // No two cells share a grid cell.
            let all: Vec<Cell> = split.standing.iter().chain(&split.taken).copied().collect();
            for (i, a) in all.iter().enumerate() {
                for b in &all[i + 1..] {
                    let apart = (0..3).any(|k| {
                        a.min[k] + a.size[k] <= b.min[k] || b.min[k] + b.size[k] <= a.min[k]
                    });
                    assert!(apart, "{a:?} overlaps {b:?}");
                }
            }
            for cell in &split.taken {
                assert!(cell.volume() as f32 <= reach.max_volume);
                assert!(cell.reached(&reach));
            }
            for cell in &split.standing {
                assert!(!cell.reached(&reach), "{cell:?} stood in the blast");
                assert!(pieces.get(cell.size).is_some());
            }
        }
    }

    #[test]
    fn it_is_fine_near_the_blow_and_coarse_away_from_it() {
        let whole = cube(64);
        let reach = Reach {
            origin: Vec3::new(0.0, 5.0, 0.0),
            radius: 1.5,
            max_volume: 30.0,
        };
        let split = split(whole, &reach, false, &sparse(), [&[], &[]], 10_000).unwrap();
        let biggest = split.standing.iter().map(Cell::volume).max().unwrap();
        assert_eq!(biggest, 32 * 96 * 32, "the far half stays one big cube");
        // A handful of levels, a few pieces each.
        assert!(split.standing.len() < 80, "{} pieces", split.standing.len());
        let far = split
            .standing
            .iter()
            .filter(|c| c.center().distance(reach.origin) > 10.0)
            .map(Cell::volume)
            .min()
            .unwrap();
        let near = split.standing.iter().map(Cell::volume).min().unwrap();
        assert!(near < far, "fine near the blow: {near} vs {far}");
    }

    #[test]
    fn a_blast_that_misses_or_cannot_break_even_a_plate_leaves_it_whole() {
        let pieces = every(16);
        let missed = Reach {
            origin: Vec3::new(50.0, 0.0, 0.0),
            radius: 2.0,
            max_volume: 30.0,
        };
        assert_eq!(split(cube(8), &missed, false, &pieces, [&[], &[]], 100), None);
        let weak = Reach {
            origin: Vec3::ZERO,
            radius: 2.0,
            max_volume: 0.5,
        };
        assert_eq!(split(cube(8), &weak, false, &pieces, [&[], &[]], 100), None);
    }

    #[test]
    fn too_many_pieces_leave_it_whole() {
        let reach = Reach {
            origin: Vec3::new(4.0, 4.0, 4.0),
            radius: 1.0,
            max_volume: 3.0,
        };
        assert!(split(cube(16), &reach, false, &every(48), [&[], &[]], 1000).is_some());
        assert_eq!(split(cube(16), &reach, false, &every(48), [&[], &[]], 4), None);
    }

    #[test]
    fn a_floor_splits_along_the_walls_standing_on_it() {
        let floor = Bounds {
            min: [0, 0, 0],
            size: [32, 1, 32],
        };
        // A wall along x = 10..11 stands on it; the blast is beside it.
        let walls = edges([Bounds {
            min: [10, 1, 0],
            size: [1, 30, 32],
        }]);
        let reach = Reach {
            origin: Vec3::new(4.0, 0.0, 8.0),
            radius: 1.0,
            max_volume: 4.0,
        };
        let split = split(
            floor,
            &reach,
            false,
            &every(32),
            [&walls[0], &walls[1]],
            1000,
        )
        .unwrap();
        // No standing piece ends under the middle of the wall.
        for cell in split.standing.iter().chain(&split.taken) {
            let (low, high) = (cell.min[0], cell.min[0] + cell.size[0]);
            assert!(!(low > 10 && low < 11) && !(high > 10 && high < 11));
        }
        let cut_at_wall = split
            .standing
            .iter()
            .any(|c| c.min[0] == 10 || c.min[0] + c.size[0] == 10 || c.min[0] == 11);
        assert!(cut_at_wall, "{:?}", split.standing);
    }
}
