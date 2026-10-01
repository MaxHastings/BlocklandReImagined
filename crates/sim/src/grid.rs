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
    for direction in [-1, 1] {
        let shifted = Bounds {
            min: [b.2.min[0], b.2.min[1] - direction, b.2.min[2]],
            size: b.2.size,
        };
        if a.2.intersection(shifted).is_some_and(|intersection| {
            intersection.any(|p| {
                let mut neighbor = p;
                neighbor[1] += direction;
                let ca = a.2.cell(p, a.0.quarter_turns, a.1);
                let cb = b.2.cell(neighbor, b.0.quarter_turns, b.1);
                if direction == 1 {
                    b"bu".contains(&ca) && b"bd".contains(&cb)
                } else {
                    b"bd".contains(&ca) && b"bu".contains(&cb)
                }
            })
        }) {
            return true;
        }
    }
    false
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
        self.buckets.get(&key).into_iter().flatten().map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_world::ContentRef;
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
                attachment_rows: m.attachment_rows.chunks(3).rev().flatten().cloned().collect(),
                ..m.clone()
            };
            let (a, b) = (reversed(a), reversed(b));
            assert!(overlaps((&ba, &a, bounds_a), (&bb, &b, bounds_b)));
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
}
