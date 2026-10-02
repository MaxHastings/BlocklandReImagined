//! Brick faces hidden by their neighbours, as v20 culls them from the BLB
//! COVERAGE records: a face is left out of its chunk when the neighbours
//! touching it, whose own touching face "hides adjacent", cover at least the
//! face's required area (studs across by plates up for sides, studs square
//! for top and bottom, which is one grid cell each). Only opaque, visible,
//! undisplaced bricks cover, so nothing seen through paint or waves opens.
use bri_content::brick::Brick as BrickMesh;
use bri_net::protocol::PublicWorld;
use bri_sim::grid::{Bounds, Index};
use bri_world::{Brick, ContentRef};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet};

/// Outward mesh-space directions of `bri_content::brick::FACES` top, bottom,
/// north, east, south, west (the order of `coverage`).
const MESH_FACES: [Vec3; 6] = [
    Vec3::Y,
    Vec3::NEG_Y,
    Vec3::NEG_Z,
    Vec3::X,
    Vec3::Z,
    Vec3::NEG_X,
];

/// A world axis (0 x, 1 y, 2 z) and whether the face looks along +axis.
type Side = (usize, bool);

fn world_side(brick: &Brick, face: usize) -> Side {
    let d = brick.transform().transform_vector3(MESH_FACES[face]);
    let axis = (0..3)
        .max_by(|a, b| d[*a].abs().total_cmp(&d[*b].abs()))
        .unwrap_or(1);
    (axis, d[axis] > 0.0)
}
fn mesh_face(brick: &Brick, (axis, positive): Side) -> usize {
    let mut d = Vec3::ZERO;
    d[axis] = if positive { 1.0 } else { -1.0 };
    let local = brick.transform().inverse().transform_vector3(d);
    (0..6)
        .max_by(|a, b| {
            MESH_FACES[*a]
                .dot(local)
                .total_cmp(&MESH_FACES[*b].dot(local))
        })
        .unwrap_or(0)
}
pub fn mesh<'a>(brick: &Brick, meshes: &'a BTreeMap<String, BrickMesh>) -> Option<&'a BrickMesh> {
    match &brick.definition {
        ContentRef::Resolved(definition) => meshes.get(definition),
        _ => None,
    }
}
pub fn bounds(brick: &Brick, meshes: &BTreeMap<String, BrickMesh>) -> Option<Bounds> {
    Bounds::new(brick, mesh(brick, meshes)?).ok()
}

/// Everything a chunk build needs to decide which faces are hidden.
pub struct Covers<'a> {
    pub index: &'a Index,
    pub world: &'a PublicWorld,
    pub meshes: &'a BTreeMap<String, BrickMesh>,
    /// Bricks drawn elsewhere or not at all: they cover nothing.
    pub left_out: &'a BTreeSet<u64>,
    pub invalid: &'a BTreeSet<u64>,
}

impl Covers<'_> {
    /// Whether `id` hides what it touches (before its per-face flags).
    fn covering(&self, id: u64) -> Option<(&Brick, &BrickMesh)> {
        let brick = self.world.bricks.get(&id)?;
        let opaque = self
            .world
            .palette
            .get(usize::from(brick.color))
            .is_some_and(|c| c[3] >= 1.0);
        if !brick.visible
            || !opaque
            || brick.shape_effect != 0
            || self.left_out.contains(&id)
            || self.invalid.contains(&id)
        {
            return None;
        }
        let mesh = mesh(brick, self.meshes)?;
        mesh.coverage.as_ref()?;
        Some((brick, mesh))
    }
    /// Faces of brick `id` to leave out, as bits of `FACES` order.
    pub fn hidden(&self, id: u64, brick: &Brick, mesh: &BrickMesh) -> u8 {
        let Some(coverage) = mesh.coverage.as_ref() else {
            return 0;
        };
        // Waves move a brick's faces off its neighbours.
        if brick.shape_effect != 0 {
            return 0;
        }
        let Some(own) = self.index.get(id) else {
            return 0;
        };
        let mut hidden = 0;
        let mut seen = Vec::new();
        for (face, cover) in coverage.iter().enumerate() {
            if cover.required_area <= 0.0 {
                continue;
            }
            let (axis, positive) = world_side(brick, face);
            let plane = if positive {
                own.max()[axis]
            } else {
                own.min[axis]
            };
            // The one-cell slab just outside the face.
            let mut slab = own;
            slab.min[axis] = if positive { plane } else { plane - 1 };
            slab.size[axis] = 1;
            let mut area = 0i64;
            seen.clear();
            self.index.visit(slab, |other, found| {
                if other == id || seen.contains(&other) {
                    return;
                }
                seen.push(other);
                let touching = if positive {
                    found.min[axis] == plane
                } else {
                    found.max()[axis] == plane
                };
                if !touching {
                    return;
                }
                let Some((neighbour, neighbour_mesh)) = self.covering(other) else {
                    return;
                };
                let facing = mesh_face(neighbour, (axis, !positive));
                if !neighbour_mesh
                    .coverage
                    .as_ref()
                    .is_some_and(|c| c[facing].hides_adjacent)
                {
                    return;
                }
                let mut overlap = 1i64;
                for a in (0..3).filter(|a| *a != axis) {
                    let low = own.min[a].max(found.min[a]);
                    let high = own.max()[a].min(found.max()[a]);
                    overlap *= i64::from((high - low).max(0));
                }
                area += overlap;
            });
            if area as f32 >= cover.required_area {
                hidden |= 1 << face;
            }
        }
        hidden
    }
}

/// Bricks whose hidden faces may change when a brick at `bounds` appears,
/// disappears or changes: everything touching it.
pub fn neighbours(index: &Index, bounds: Bounds, out: &mut BTreeSet<u64>) {
    index.visit(bounds.expanded(1), |id, _| {
        out.insert(id);
    });
}
