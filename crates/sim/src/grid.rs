//! Integer build occupancy is independent of solver and selection geometry.
use anyhow::{Result, ensure};
use bri_content::brick::Brick as Mesh;
use bri_world::{Brick, BrickId};
use std::collections::{BTreeMap, BTreeSet};
pub const CELL: [f32; 3] = [0.5, 0.2, 0.5];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub min: [i32; 3],
    pub size: [i32; 3],
}
impl Bounds {
    pub fn new(brick: &Brick, mesh: &Mesh) -> Result<Self> {
        let [w, d] = mesh.footprint_studs.map(|v| v as i32);
        let h = mesh.height_plates as i32;
        let size = if brick.quarter_turns.is_multiple_of(2) {
            [w, h, d]
        } else {
            [d, h, w]
        };
        let mut min = [0; 3];
        for axis in 0..3 {
            let cell = [0.5_f64, 0.2, 0.5][axis];
            let lower = f64::from(brick.position[axis]) - f64::from(size[axis]) * cell * 0.5;
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
        mesh.attachment_rows[((d - 1 - lz) * h + (h - 1 - y)) as usize].as_bytes()[lx as usize]
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
fn bucket_span(bounds: Bounds) -> ([i32; 3], [i32; 3]) {
    let step = [16, 40, 16];
    let min: [i32; 3] = std::array::from_fn(|a| bounds.min[a].div_euclid(step[a]));
    let max: [i32; 3] = std::array::from_fn(|a| (bounds.max()[a] - 1).div_euclid(step[a]));
    (min, max)
}
#[derive(Default)]
pub struct Index {
    buckets: BTreeMap<(i32, i32, i32), BTreeSet<BrickId>>,
    bounds: BTreeMap<BrickId, Bounds>,
}
impl Index {
    pub fn insert(&mut self, id: BrickId, bounds: Bounds) {
        self.remove(id);
        for key in keys(bounds) {
            self.buckets.entry(key).or_default().insert(id);
        }
        self.bounds.insert(id, bounds);
    }
    pub fn remove(&mut self, id: BrickId) {
        if let Some(bounds) = self.bounds.remove(&id) {
            for key in keys(bounds) {
                if let Some(bucket) = self.buckets.get_mut(&key) {
                    bucket.remove(&id);
                    if bucket.is_empty() {
                        self.buckets.remove(&key);
                    }
                }
            }
        }
    }
    pub fn query(&self, bounds: Bounds) -> BTreeSet<BrickId> {
        let (min, max) = bucket_span(bounds);
        let count = (0..3).fold(1u64, |n, a| {
            n.saturating_mul((i64::from(max[a]) - i64::from(min[a]) + 1).max(0) as u64)
        });
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
                .flat_map(|(_, ids)| ids)
                .filter(|id| self.bounds[id].intersection(bounds).is_some())
                .copied()
                .collect();
        }
        keys(bounds)
            .filter_map(|key| self.buckets.get(&key))
            .flatten()
            .filter(|id| self.bounds[id].intersection(bounds).is_some())
            .copied()
            .collect()
    }
    pub fn bounds(&self, id: BrickId) -> Bounds {
        self.bounds[&id]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_world::ContentRef;
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
