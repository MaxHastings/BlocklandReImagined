//! Integer build occupancy is independent of solver and selection geometry.
use anyhow::{Result, ensure};
use bri_content::brick::Brick as Mesh;
use bri_world::{Brick, BrickId};
use rustc_hash::FxHashMap;
use std::collections::BTreeSet;
pub const CELL: [f32; 3] = [0.5, 0.2, 0.5];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub min: [i32; 3],
    pub size: [i32; 3],
}
impl Bounds {
    pub fn new(brick: &Brick, mesh: &Mesh) -> Result<Self> {
        Self::at(brick.position, brick.quarter_turns, mesh)
    }
    /// [`Self::new`] for a brick of `mesh` at `position`, turned
    /// `quarter_turns`.
    pub fn at(position: [f32; 3], quarter_turns: u8, mesh: &Mesh) -> Result<Self> {
        let [w, d] = mesh.footprint_studs.map(|v| v as i32);
        let h = mesh.height_plates as i32;
        let size = if quarter_turns.is_multiple_of(2) {
            [w, h, d]
        } else {
            [d, h, w]
        };
        let mut min = [0; 3];
        for axis in 0..3 {
            let cell = [0.5_f64, 0.2, 0.5][axis];
            let lower = f64::from(position[axis]) - f64::from(size[axis]) * cell * 0.5;
            let grid = (lower / cell).round();
            ensure!(
                (lower - grid * cell).abs() < 0.001,
                "Brick is off the stud/plate grid"
            );
            min[axis] = grid as i32;
        }
        Ok(Self { min, size })
    }
    /// The position nearest `position` at which a brick of `mesh`, turned
    /// `quarter_turns`, sits on the stud and plate grid: the same position
    /// when it already does.
    pub fn snapped(position: [f32; 3], quarter_turns: u8, mesh: &Mesh) -> [f32; 3] {
        let [w, d] = mesh.footprint_studs.map(f64::from);
        let h = f64::from(mesh.height_plates);
        let size = if quarter_turns.is_multiple_of(2) {
            [w, h, d]
        } else {
            [d, h, w]
        };
        std::array::from_fn(|axis| {
            let cell = [0.5_f64, 0.2, 0.5][axis];
            let half = size[axis] * cell * 0.5;
            let lower = f64::from(position[axis]) - half;
            ((lower / cell).round() * cell + half) as f32
        })
    }
    pub fn max(self) -> [i32; 3] {
        std::array::from_fn(|a| self.min[a] + self.size[a])
    }
    pub fn expanded(self, n: i32) -> Self {
        Self {
            min: self.min.map(|v| v - n),
            size: self.size.map(|v| v + 2 * n),
        }
    }
    pub fn intersection(self, other: Self) -> Option<Self> {
        let min = std::array::from_fn(|a| self.min[a].max(other.min[a]));
        let max: [i32; 3] = std::array::from_fn(|a| self.max()[a].min(other.max()[a]));
        let size = std::array::from_fn(|a| max[a] - min[a]);
        size.iter().all(|v| *v > 0).then_some(Self { min, size })
    }
    pub fn cell(self, p: [i32; 3], turns: u8, mesh: &Mesh) -> u8 {
        let [x, y, z] = std::array::from_fn(|a| p[a] - self.min[a]);
        if x < 0 || y < 0 || z < 0 || x >= self.size[0] || y >= self.size[1] || z >= self.size[2] {
            return b'-';
        }
        let [w, d] = mesh.footprint_studs.map(|v| v as i32);
        let h = mesh.height_plates as i32;
        let (lx, lz) = match turns {
            0 => (x, z),
            1 => (z, d - 1 - x),
            2 => (w - 1 - x, d - 1 - z),
            3 => (w - 1 - z, x),
            _ => return b'-',
        };
        // A BLB's first depth slice is its largest y, which the converter
        // turns into our smallest z (`z = -y`).
        mesh.attachment_rows[(lz * h + (h - 1 - y)) as usize].as_bytes()[lx as usize]
    }
    fn any(self, mut f: impl FnMut([i32; 3]) -> bool) -> bool {
        let max = self.max();
        for z in self.min[2]..max[2] {
            for y in self.min[1]..max[1] {
                for x in self.min[0]..max[0] {
                    if f([x, y, z]) {
                        return true;
                    }
                }
            }
        }
        false
    }
}
/// Whether two boxes share part of a face: they meet on one axis and
/// overlap by at least a cell on the other two. Side by side, stacked or
/// hung beneath, studs or not; boxes meeting only along an edge or at a
/// corner do not share a face.
pub fn share_face(a: Bounds, b: Bounds) -> bool {
    let (a_max, b_max) = (a.max(), b.max());
    let mut meeting = 0;
    for axis in 0..3 {
        if a_max[axis] == b.min[axis] || b_max[axis] == a.min[axis] {
            meeting += 1;
        } else if a.min[axis] >= b_max[axis] || b.min[axis] >= a_max[axis] {
            return false;
        }
    }
    meeting == 1
}
pub fn overlaps(a: (&Brick, &Mesh, Bounds), b: (&Brick, &Mesh, Bounds)) -> bool {
    a.2.intersection(b.2).is_some_and(|intersection| {
        intersection.any(|p| {
            a.2.cell(p, a.0.quarter_turns, a.1) != b'-'
                && b.2.cell(p, b.0.quarter_turns, b.1) != b'-'
        })
    })
}
pub fn connected(a: (&Brick, &Mesh, Bounds), b: (&Brick, &Mesh, Bounds)) -> bool {
    connected_where(a, b, |_, _| true)
}
/// The same rotated authored attachment cells, with a placement-time test for
/// whether the space between each matching pair actually permits a connection.
pub(crate) fn connected_where(
    a: (&Brick, &Mesh, Bounds),
    b: (&Brick, &Mesh, Bounds),
    allowed: impl FnMut([i32; 3], [i32; 3]) -> bool,
) -> bool {
    connected_shapes_where(
        (a.0.quarter_turns, a.1, a.2),
        (b.0.quarter_turns, b.1, b.2),
        allowed,
    )
}
/// Compact placement preflight uses the identical authored attachment predicate
/// without retaining a whole gameplay brick or copying its settings/events.
pub(crate) fn connected_shapes_where(
    a: (u8, &Mesh, Bounds),
    b: (u8, &Mesh, Bounds),
    mut allowed: impl FnMut([i32; 3], [i32; 3]) -> bool,
) -> bool {
    let mut cells = ConnectionCells::new(a.2, b.2);
    while let Some((cell, neighbor, attached)) = cells.next(a.0, a.1, b.0, b.1) {
        if attached && allowed(cell, neighbor) {
            return true;
        }
    }
    false
}
/// One raw attachment cell per step, including authored empty cells. Callers
/// can charge work before advancing; no large mesh hides an inner scan.
#[derive(Clone)]
pub(crate) struct ConnectionCells {
    a: Bounds,
    b: Bounds,
    direction: i32,
    next: Option<[i32; 3]>,
    intersection: Option<Bounds>,
}
impl ConnectionCells {
    pub(crate) fn new(a: Bounds, b: Bounds) -> Self {
        let mut out = Self {
            a,
            b,
            direction: -1,
            next: None,
            intersection: None,
        };
        out.begin();
        out
    }
    fn begin(&mut self) {
        let shifted = Bounds {
            min: [self.b.min[0], self.b.min[1] - self.direction, self.b.min[2]],
            size: self.b.size,
        };
        self.intersection = self.a.intersection(shifted);
        self.next = self.intersection.map(|b| b.min);
    }
    pub(crate) fn next(
        &mut self,
        at: u8,
        am: &Mesh,
        bt: u8,
        bm: &Mesh,
    ) -> Option<([i32; 3], [i32; 3], bool)> {
        if self.next.is_none() && self.direction == -1 {
            self.direction = 1;
            self.begin();
        }
        let p = self.next?;
        let bounds = self.intersection.unwrap();
        let max = bounds.max();
        let mut next = p;
        next[0] += 1;
        if next[0] == max[0] {
            next[0] = bounds.min[0];
            next[1] += 1;
        }
        if next[1] == max[1] {
            next[1] = bounds.min[1];
            next[2] += 1;
        }
        self.next = (next[2] < max[2]).then_some(next);
        let mut neighbor = p;
        neighbor[1] += self.direction;
        let ca = self.a.cell(p, at, am);
        let cb = self.b.cell(neighbor, bt, bm);
        let attached = if self.direction == 1 {
            b"bu".contains(&ca) && b"bd".contains(&cb)
        } else {
            b"bd".contains(&ca) && b"bu".contains(&cb)
        };
        Some((p, neighbor, attached))
    }
}
/// A bounded spatial query cursor. Each step visits at most one bucket entry
/// (or one empty bucket), without eagerly collecting a potentially huge set.
pub(crate) struct QueryCursor {
    bounds: Bounds,
    min: [i32; 3],
    max: [i32; 3],
    key: Option<[i32; 3]>,
    entry: usize,
}
impl QueryCursor {
    pub(crate) fn new(bounds: Bounds) -> Self {
        let (min, max) = bucket_span(bounds);
        Self {
            bounds,
            min,
            max,
            key: Some(min),
            entry: 0,
        }
    }
    pub(crate) fn step(&mut self, index: &Index) -> Option<Option<BrickId>> {
        let key = self.key?;
        let entries = index.bucket_bounds((key[0], key[1], key[2]));
        if let Some(&(id, bounds)) = entries.get(self.entry) {
            self.entry += 1;
            let admitted = bounds.intersection(self.bounds).is_some_and(|overlap| {
                // Report an id only in the first bucket shared by both boxes.
                bucket_span(overlap).0 == key
            });
            return Some(admitted.then_some(id));
        }
        self.entry = 0;
        let mut next = key;
        next[2] += 1;
        if next[2] > self.max[2] {
            next[2] = self.min[2];
            next[1] += 1;
        }
        if next[1] > self.max[1] {
            next[1] = self.min[1];
            next[0] += 1;
        }
        self.key = (next[0] <= self.max[0]).then_some(next);
        Some(None)
    }
}
// Eight-unit buckets. All authored cells remain in the templates, not expanded
// into one hash entry per voxel for every placed brick.
fn keys(bounds: Bounds) -> impl Iterator<Item = (i32, i32, i32)> {
    let (min, max) = bucket_span(bounds);
    (min[0]..=max[0]).flat_map(move |x| {
        (min[1]..=max[1]).flat_map(move |y| (min[2]..=max[2]).map(move |z| (x, y, z)))
    })
}
/// Bucket edge in grid cells (eight native units on every axis).
pub(crate) const BUCKET: [i32; 3] = [16, 40, 16];
pub(crate) fn bucket_span(bounds: Bounds) -> ([i32; 3], [i32; 3]) {
    let min: [i32; 3] = std::array::from_fn(|a| bounds.min[a].div_euclid(BUCKET[a]));
    let max: [i32; 3] = std::array::from_fn(|a| (bounds.max()[a] - 1).div_euclid(BUCKET[a]));
    (min, max)
}
/// Buckets pierced by a ray, nearest first, each with the ray distance at which
/// the ray leaves it (3D DDA). `direction` must be normalized.
pub fn ray_buckets(
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
) -> Vec<((i32, i32, i32), f32)> {
    let size: [f32; 3] = std::array::from_fn(|a| CELL[a] * BUCKET[a] as f32);
    let mut bucket: [i32; 3] = std::array::from_fn(|a| (origin[a] / size[a]).floor() as i32);
    let step: [i32; 3] = std::array::from_fn(|a| direction[a].signum() as i32);
    let mut next: [f32; 3] = std::array::from_fn(|a| {
        if direction[a] > 0.0 {
            ((bucket[a] + 1) as f32 * size[a] - origin[a]) / direction[a]
        } else if direction[a] < 0.0 {
            (bucket[a] as f32 * size[a] - origin[a]) / direction[a]
        } else {
            f32::INFINITY
        }
    });
    let delta: [f32; 3] = std::array::from_fn(|a| size[a] / direction[a].abs());
    let mut out = Vec::new();
    loop {
        let axis = (0..3).min_by(|a, b| next[*a].total_cmp(&next[*b])).unwrap();
        let exit = next[axis];
        out.push(((bucket[0], bucket[1], bucket[2]), exit.min(max_distance)));
        if exit >= max_distance || exit.is_nan() {
            return out;
        }
        bucket[axis] += step[axis];
        next[axis] += delta[axis];
    }
}
/// Spatial hash of brick bounds. Buckets hold ids in ascending order, so
/// every query and bucket walk is as deterministic as an ordered map.
#[derive(Default)]
pub struct Index {
    /// Each bucket's bricks with their bounds inline, so a query scans
    /// memory instead of looking every candidate up.
    buckets: FxHashMap<(i32, i32, i32), Vec<(BrickId, Bounds)>>,
    bounds: crate::id_map::IdMap<Bounds>,
}

impl Index {
    pub fn insert(&mut self, id: BrickId, bounds: Bounds) {
        self.remove(id);
        for key in keys(bounds) {
            let bucket = self.buckets.entry(key).or_default();
            if let Err(at) = bucket.binary_search_by_key(&id, |(id, _)| *id) {
                bucket.insert(at, (id, bounds));
            }
        }
        self.bounds.insert(id, bounds);
    }
    pub fn remove(&mut self, id: BrickId) {
        if let Some(bounds) = self.bounds.remove(id) {
            for key in keys(bounds) {
                if let Some(bucket) = self.buckets.get_mut(&key) {
                    if let Ok(at) = bucket.binary_search_by_key(&id, |(id, _)| *id) {
                        bucket.remove(at);
                    }
                    if bucket.is_empty() {
                        self.buckets.remove(&key);
                    }
                }
            }
        }
    }
    pub fn query(&self, bounds: Bounds) -> BTreeSet<BrickId> {
        let mut out = BTreeSet::new();
        self.any(bounds, |id| {
            out.insert(id);
            false
        });
        out
    }
    /// Whether `hit` holds for any brick whose bounds meet `bounds`,
    /// without collecting them. A brick spanning several buckets may be
    /// offered more than once.
    pub fn any(&self, bounds: Bounds, mut hit: impl FnMut(BrickId) -> bool) -> bool {
        let (min, max) = bucket_span(bounds);
        let count = (0..3).fold(1u64, |n, a| {
            n.saturating_mul((i64::from(max[a]) - i64::from(min[a]) + 1).max(0) as u64)
        });
        let meets = |b: &Bounds| b.intersection(bounds).is_some();
        // A long diagonal visibility ray can have a huge, mostly empty box.
        // Scan occupied buckets when that is cheaper than enumerating empty space.
        if count > self.buckets.len() as u64 {
            return self
                .buckets
                .iter()
                .filter(|((x, y, z), _)| {
                    *x >= min[0]
                        && *x <= max[0]
                        && *y >= min[1]
                        && *y <= max[1]
                        && *z >= min[2]
                        && *z <= max[2]
                })
                .flat_map(|(_, entries)| entries)
                .any(|(id, b)| meets(b) && hit(*id));
        }
        keys(bounds)
            .filter_map(|key| self.buckets.get(&key))
            .flatten()
            .any(|(id, b)| meets(b) && hit(*id))
    }
    pub fn bounds(&self, id: BrickId) -> Bounds {
        *self.bounds.get(id).expect("an indexed brick")
    }
    pub fn get(&self, id: BrickId) -> Option<Bounds> {
        self.bounds.get(id).copied()
    }
    /// Each brick whose bounds meet `bounds`, without allocating; a brick
    /// spanning several buckets may be visited more than once.
    pub fn visit(&self, bounds: Bounds, mut f: impl FnMut(BrickId, Bounds)) {
        for key in keys(bounds) {
            for &(id, found) in self.buckets.get(&key).into_iter().flatten() {
                if found.intersection(bounds).is_some() {
                    f(id, found);
                }
            }
        }
    }
    /// How many buckets hold bricks.
    pub fn occupied(&self) -> usize {
        self.buckets.len()
    }
    /// The buckets holding bricks, in no order.
    pub fn occupied_keys(&self) -> impl Iterator<Item = (i32, i32, i32)> + '_ {
        self.buckets.keys().copied()
    }
    /// Each brick registered in bucket `key`, with its bounds: a whole
    /// bucket at a time, for work spread over ticks.
    pub fn bucket_bounds(&self, key: (i32, i32, i32)) -> &[(BrickId, Bounds)] {
        self.buckets.get(&key).map_or(&[], Vec::as_slice)
    }
    /// Bricks registered in one bucket from [`ray_buckets`].
    pub fn bucket(&self, key: (i32, i32, i32)) -> impl Iterator<Item = BrickId> + '_ {
        self.buckets
            .get(&key)
            .into_iter()
            .flatten()
            .map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_world::ContentRef;
    /// A brick saved off its grid (a save made where it had another size)
    /// moves to the nearest grid position, under half a cell; one on it
    /// stays put, turned or not.
    #[test]
    fn a_brick_off_its_grid_snaps_to_the_nearest_cell() {
        let odd = crate::testing::definition("odd", [3, 5], 2, Default::default(), false).mesh;
        assert!(Bounds::at([0.0, 0.1, 0.0], 0, &odd).is_err());
        let snapped = Bounds::snapped([0.0, 0.1, 0.0], 0, &odd);
        assert!(Bounds::at(snapped, 0, &odd).is_ok(), "{snapped:?}");
        for axis in 0..3 {
            let moved = (snapped[axis] - [0.0, 0.1, 0.0][axis]).abs();
            assert!(moved <= [0.25, 0.1, 0.25][axis] + 1e-6, "{snapped:?}");
        }
        let turned = Bounds::snapped([0.0, 0.1, 0.0], 1, &odd);
        assert!(Bounds::at(turned, 1, &odd).is_ok(), "{turned:?}");
        let on = [0.25, 0.2, 0.75];
        assert!(Bounds::at(on, 0, &odd).is_ok());
        assert_eq!(Bounds::snapped(on, 0, &odd), on);
    }
    #[test]
    fn faces_are_shared_side_by_side_and_stacked_never_at_edges() {
        let b = |min: [i32; 3], size: [i32; 3]| Bounds { min, size };
        let plate = b([0, 0, 0], [2, 1, 1]);
        // Beside it, above it and below it, partly overlapping the face.
        assert!(share_face(plate, b([2, 0, 0], [1, 3, 1])));
        assert!(share_face(plate, b([1, 1, 0], [4, 1, 4])));
        assert!(share_face(b([-3, -3, -3], [4, 3, 4]), plate));
        // A gap, an edge, a corner and an overlap are not faces.
        assert!(!share_face(plate, b([3, 0, 0], [1, 1, 1])));
        assert!(!share_face(plate, b([2, 1, 0], [1, 1, 1])));
        assert!(!share_face(plate, b([2, 1, 1], [1, 1, 1])));
        assert!(!share_face(plate, b([1, 0, 0], [2, 1, 1])));
    }
    #[test]
    fn sparse_large_query_uses_occupied_buckets_and_exact_bounds() {
        let mut index = Index::default();
        index.insert(
            1,
            Bounds {
                min: [-1000, -1000, -1000],
                size: [1; 3],
            },
        );
        index.insert(
            2,
            Bounds {
                min: [1000, 1000, 1000],
                size: [1; 3],
            },
        );
        index.insert(
            3,
            Bounds {
                min: [2000, 2000, 2000],
                size: [1; 3],
            },
        );
        assert_eq!(
            index.query(Bounds {
                min: [-1_000_000; 3],
                size: [1_001_001; 3]
            }),
            [1, 2].into()
        );
        assert_eq!(
            index.query(Bounds {
                min: [-999, -1000, -1000],
                size: [1; 3]
            }),
            BTreeSet::new()
        );
    }
    fn mesh() -> Mesh {
        Mesh {
            schema_version: 1,
            id: "asymmetric".into(),
            footprint_studs: [2, 1],
            height_plates: 1,
            attachment_rows: vec!["b-".into()],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![],
        }
    }
    /// A stock ramp grid: one plate-high cell per row, slices in BLB order.
    fn ramp(id: &str, rows: &[&str]) -> Mesh {
        Mesh {
            id: id.into(),
            footprint_studs: [1, rows.len() as u32 / 3],
            height_plates: 3,
            attachment_rows: rows.iter().map(|r| (*r).into()).collect(),
            ..mesh()
        }
    }
    fn placed(mesh: &Mesh, position: [f32; 3], turns: u8) -> (Brick, Bounds) {
        let mut brick = Brick::new(ContentRef::Resolved(mesh.id.clone()), position, 1);
        brick.quarter_turns = turns;
        let bounds = Bounds::new(&brick, mesh).unwrap();
        (brick, bounds)
    }
    #[test]
    fn blb_slices_run_from_the_far_end_like_the_mesh() {
        // 1x3ramp.blb: its stud top is in the last slice, at BLB y -1.5..-0.5,
        // which the converted mesh puts at +z.
        let r = ramp("1x3ramp", &["-", "x", "d", "x", "x", "d", "u", "x", "d"]);
        let (_, bounds) = placed(&r, [0.25, 0.3, 0.75], 0);
        assert_eq!(bounds.min, [0, 0, 0]);
        assert_eq!(bounds.cell([0, 2, 2], 0, &r), b'u');
        assert_eq!(bounds.cell([0, 2, 0], 0, &r), b'-');
        let (_, turned) = placed(&r, [0.25, 0.3, 0.75], 2);
        assert_eq!(turned.cell([0, 2, 0], 2, &r), b'u');
    }
    /// Ramp pairs from the stock saves (Arch of Constantine) that v20 loads
    /// in full: each fills the other's empty wedge cell, so neither overlaps.
    #[test]
    fn stock_ramp_pairs_fit_into_each_others_wedges() {
        let ramp_1x2 = ramp("1x2ramp", &["x", "x", "d", "u", "x", "d"]);
        let rampup_1x3 = ramp("1x3rampup", &["u", "x", "-", "u", "x", "x", "u", "x", "d"]);
        let crest_1x2 = ramp("1x2cresthigh", &["x", "x", "d", "x", "x", "d"]);
        for ((a, pa, ta), (b, pb, tb)) in [
            (
                (&ramp_1x2, [0.75, 297.7, 144.0], 0),
                (&rampup_1x3, [0.75, 298.1, 143.25], 2),
            ),
            (
                (&rampup_1x3, [-6.75, 298.1, 162.25], 0),
                (&crest_1x2, [-6.75, 297.7, 161.5], 0),
            ),
        ] {
            let (ba, bounds_a) = placed(a, pa, ta);
            let (bb, bounds_b) = placed(b, pb, tb);
            assert!(bounds_a.intersection(bounds_b).is_some());
            assert!(!overlaps((&ba, a, bounds_a), (&bb, b, bounds_b)));
            // Read with the slices reversed, as before this was fixed, the
            // pair collides and the load skips one of them.
            let reversed = |m: &Mesh| Mesh {
                attachment_rows: m
                    .attachment_rows
                    .chunks(3)
                    .rev()
                    .flatten()
                    .cloned()
                    .collect(),
                ..m.clone()
            };
            let (a, b) = (reversed(a), reversed(b));
            assert!(overlaps((&ba, &a, bounds_a), (&bb, &b, bounds_b)));
        }
    }
    /// Every pair of grid cells, one plate apart, against v20's reading of
    /// them (blocklandv20.exe 0x53c1d0): a cell that is not `-` is solid,
    /// `u` and `b` take the cell above, `d` and `b` the cell below. Two solid
    /// cells overlap side by side; stacked, they join only up onto down.
    #[test]
    fn every_cell_pair_places_and_joins_as_v20_reads_the_grid() {
        let cells = [b'b', b'u', b'd', b'x', b'-'];
        let one = |cell: u8| Mesh {
            footprint_studs: [1, 1],
            attachment_rows: vec![(cell as char).to_string()],
            ..mesh()
        };
        let up = |c: u8| c == b'u' || c == b'b';
        let down = |c: u8| c == b'd' || c == b'b';
        for lower in cells {
            for upper in cells {
                let (a, b) = (one(lower), one(upper));
                let (ba, bounds_a) = placed(&a, [0.25, 0.1, 0.25], 0);
                let (bb, same) = placed(&b, [0.25, 0.1, 0.25], 0);
                assert_eq!(
                    overlaps((&ba, &a, bounds_a), (&bb, &b, same)),
                    lower != b'-' && upper != b'-',
                    "{} in {}",
                    upper as char,
                    lower as char
                );
                let (bb, above) = placed(&b, [0.25, 0.3, 0.25], 0);
                let joined = up(lower) && down(upper);
                assert_eq!(
                    connected((&ba, &a, bounds_a), (&bb, &b, above)),
                    joined,
                    "{} on {}",
                    upper as char,
                    lower as char
                );
                assert_eq!(
                    connected((&bb, &b, above), (&ba, &a, bounds_a)),
                    joined,
                    "{} under {}",
                    lower as char,
                    upper as char
                );
            }
        }
    }
    #[test]
    fn rotation_preserves_voids_and_vertical_attachment_faces() {
        let m = mesh();
        for turn in 0..4 {
            let mut b = Brick::new(
                ContentRef::Resolved(m.id.clone()),
                if turn % 2 == 0 {
                    [0.5, 0.1, 0.25]
                } else {
                    [0.25, 0.1, 0.5]
                },
                1,
            );
            b.quarter_turns = turn;
            let bounds = Bounds::new(&b, &m).unwrap();
            let occupied = match turn {
                0 => [0, 0, 0],
                1 => [0, 0, 0],
                2 => [1, 0, 0],
                _ => [0, 0, 1],
            };
            assert_eq!(bounds.cell(occupied, turn, &m), b'b');
            let mut above = b.clone();
            above.position[1] += 0.2;
            let ab = Bounds::new(&above, &m).unwrap();
            assert!(!overlaps((&b, &m, bounds), (&above, &m, ab)));
            assert!(connected((&b, &m, bounds), (&above, &m, ab)));
        }
        let mut b = Brick::new(ContentRef::Resolved(m.id.clone()), [0.5, 0.1, 0.25], 1);
        let original = Bounds::new(&b, &m).unwrap();
        b.position[0] += 0.1;
        assert!(Bounds::new(&b, &m).is_err());
        let mut index = Index::default();
        index.insert(1, original);
        assert!(index.query(original).contains(&1));
        index.remove(1);
        assert!(index.query(original).is_empty());
    }
    #[test]
    fn connection_filter_checks_only_rotated_attachment_pairs_and_accepts_any_clear_pair() {
        let mut m = mesh();
        m.attachment_rows = vec!["bb".into()];
        for turn in 0..4 {
            let position = if turn % 2 == 0 {
                [0.5, 0.1, 0.25]
            } else {
                [0.25, 0.1, 0.5]
            };
            let (mut lower, _) = placed(&m, position, turn);
            lower.quarter_turns = turn;
            let bounds = Bounds::new(&lower, &m).unwrap();
            let mut upper = lower.clone();
            upper.position[1] += 0.2;
            let above = Bounds::new(&upper, &m).unwrap();
            let mut visited = Vec::new();
            assert!(connected_where(
                (&upper, &m, above),
                (&lower, &m, bounds),
                |a, b| {
                    assert_eq!(a[0], b[0]);
                    assert_eq!(a[2], b[2]);
                    assert_eq!(a[1], b[1] + 1);
                    visited.push((a, b));
                    visited.len() == 2
                }
            ));
            assert_eq!(
                visited.len(),
                2,
                "a blocked first connector must not hide another clear one"
            );
            let mut blocked = 0;
            assert!(!connected_where(
                (&upper, &m, above),
                (&lower, &m, bounds),
                |_, _| {
                    blocked += 1;
                    false
                }
            ));
            assert_eq!(blocked, 2);
        }
    }
    #[test]
    fn support_query_cursor_visits_only_one_entry_and_deduplicates_spanning_bricks() {
        let mut index = Index::default();
        let bounds = Bounds {
            min: [-1, 0, -1],
            size: [34, 1, 34],
        };
        for id in 1..=200 {
            index.insert(id, bounds);
        }
        let mut cursor = QueryCursor::new(bounds);
        let mut found = BTreeSet::new();
        let mut steps = 0;
        while let Some(candidate) = cursor.step(&index) {
            steps += 1;
            if let Some(id) = candidate {
                assert!(found.insert(id));
            }
            if steps == 8 {
                assert_eq!(found.len(), 8, "one entry per work unit");
            }
        }
        assert_eq!(found.len(), 200);
        assert!(
            steps > 200,
            "duplicate buckets still charge scanned entries"
        );
    }
    #[test]
    fn attachment_cursor_resumes_inside_a_large_authored_face() {
        let mut m = mesh();
        m.footprint_studs = [1000, 1000];
        m.attachment_rows = vec!["b".repeat(1000); 1000];
        let lower = Bounds {
            min: [0, 0, 0],
            size: [1000, 1, 1000],
        };
        let upper = Bounds {
            min: [0, 1, 0],
            size: [1000, 1, 1000],
        };
        let mut cells = ConnectionCells::new(lower, upper);
        for x in 0..32 {
            assert_eq!(cells.next(0, &m, 0, &m), Some(([x, 0, 0], [x, 1, 0], true)));
        }
        let mut resumed = cells.clone();
        assert_eq!(
            resumed.next(0, &m, 0, &m),
            Some(([32, 0, 0], [32, 1, 0], true))
        );
    }
}
