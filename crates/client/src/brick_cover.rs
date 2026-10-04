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

/// A conservative opaque rectangle on the actual tagged touching plane.
/// Planar rendered triangles may form one filled rectangle (native bottom
/// loop + trapezoid edges). Holes and disconnected patches retain faces;
/// logical volume alone never proves coverage. Only chunk rebuilds ask.
// Proof work is finite even for valid unfamiliar very detailed meshes.
const MAX_PROOF_QUADS: usize = 64;
const MAX_COVER_CELLS: i64 = 16_384;
/// Shared for one rebuild's chunk workers; geometry and authored alpha are
/// immutable during that build. Opaque paint has the same inherited alpha.
pub type FaceProofs = std::sync::Mutex<BTreeMap<usize, [Option<(Vec3, Vec3)>; 6]>>;
// v20's generated BRICK skin grows outward by this amount (converter
// brick::standard). Coverage remains on the authored stud/plate boundary.
const GRID_SKIN: f32 = 0.0012;
const MAX_PROOF_BREAKS: usize = 4096;
type Point = [f64; 2];
fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn authored_face_region(mesh: &BrickMesh, face: usize) -> Option<(Vec3, Vec3)> {
    if mesh.quads.len() > MAX_PROOF_QUADS {
        return None;
    }
    let direction = MESH_FACES[face];
    let axis = (0..3).find(|a| direction[*a] != 0.).unwrap();
    let axes = match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    let half = mesh.half_size();
    let plane = direction[axis] * half[axis];
    let mut triangles = Vec::new();
    for quad in mesh
        .quads
        .iter()
        .filter(|q| q.face == bri_content::brick::FACES[face])
    {
        if quad.colors.is_some_and(|colors| {
            colors.iter().any(|color| {
                !bri_render::scene::resolve_brick_vertex_color([1.; 4], Some(*color))
                    .is_ok_and(|c| c.rgba[3] >= 1.)
            })
        }) {
            return None;
        }
        let mut positions = quad.vertices.map(|v| Vec3::from(v.position));
        for p in &mut positions {
            // Only the tiny outward skin is normalized. Inset or distant
            // faces and geometry outside its declared footprint retain faces.
            for a in 0..3 {
                if p[a].abs() > half[a] {
                    if p[a].abs() - half[a] > GRID_SKIN + 0.0001 {
                        return None;
                    }
                    if a == axis {
                        p[a] = p[a].signum() * half[a];
                    }
                }
            }
            if (p[axis] - plane).abs() > 0.0001 {
                return None;
            }
        }
        let points = positions.map(|p| [f64::from(p[axes[0]]), f64::from(p[axes[1]])]);
        // Rendered quads use these two triangles. One-wide native bricks
        // include repeated vertices; their zero-area triangles cover nothing.
        for indices in [[0, 1, 2], [0, 2, 3]] {
            let triangle = indices.map(|i| points[i]);
            if cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0])) != 0. {
                triangles.push(triangle);
            }
        }
    }
    if triangles.is_empty() {
        return None;
    }
    let lo = [0, 1].map(|a| {
        triangles
            .iter()
            .flatten()
            .map(|p| p[a])
            .fold(f64::INFINITY, f64::min)
    });
    let hi = [0, 1].map(|a| {
        triangles
            .iter()
            .flatten()
            .map(|p| p[a])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    if (0..2).any(|a| hi[a] <= lo[a]) {
        return None;
    }
    let edges: Vec<_> = triangles
        .iter()
        .flat_map(|t| (0..3).map(move |i| (t[i], t[(i + 1) % 3])))
        .collect();
    let mut xs: Vec<_> = triangles.iter().flatten().map(|p| p[0]).collect();
    // Endpoint ordering can change only at vertices or edge intersections.
    // Including both makes a midpoint union proof exact within each strip.
    for (i, (a, b)) in edges.iter().enumerate() {
        let r = sub(*b, *a);
        for (c, d) in &edges[i + 1..] {
            let s = sub(*d, *c);
            let denominator = cross(r, s);
            if denominator == 0. {
                continue;
            }
            let t = cross(sub(*c, *a), s) / denominator;
            let u = cross(sub(*c, *a), r) / denominator;
            if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u) {
                xs.push(a[0] + t * r[0]);
                if xs.len() > MAX_PROOF_BREAKS {
                    return None;
                }
            }
        }
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    for window in xs.windows(2) {
        if window[0] == window[1] {
            continue;
        }
        let x = window[0] + (window[1] - window[0]) * 0.5;
        if !(x > window[0] && x < window[1]) {
            return None;
        }
        let mut spans = Vec::new();
        for triangle in &triangles {
            let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
            for i in 0..3 {
                let (mut a, mut b) = (triangle[i], triangle[(i + 1) % 3]);
                // Shared quilt edges calculate identical intersections even
                // when the neighbouring triangle winds them oppositely.
                if a[0] > b[0] {
                    std::mem::swap(&mut a, &mut b);
                }
                if a[0] == b[0] || x < a[0] || x > b[0] {
                    continue;
                }
                let y = a[1] + (x - a[0]) * (b[1] - a[1]) / (b[0] - a[0]);
                low = low.min(y);
                high = high.max(y);
            }
            if high > low {
                spans.push((low, high));
            }
        }
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut edge = lo[1];
        for (low, high) in spans {
            if low > edge {
                return None;
            }
            edge = edge.max(high);
        }
        if edge < hi[1] {
            return None;
        }
    }
    let mut min = Vec3::splat(plane);
    let mut max = min;
    for a in 0..2 {
        min[axes[a]] = lo[a] as f32;
        max[axes[a]] = hi[a] as f32;
    }
    Some((min, max))
}
fn whole_cell_boundary(value: f32, axis: usize, lower: bool) -> i32 {
    let cell = value / bri_sim::grid::CELL[axis];
    if (cell - cell.round()).abs() < 0.0001 {
        cell.round() as i32
    } else if lower {
        cell.ceil() as i32
    } else {
        cell.floor() as i32
    }
}

/// Everything a chunk build needs to decide which faces are hidden.
pub struct Covers<'a> {
    pub index: &'a Index,
    pub world: &'a PublicWorld,
    pub meshes: &'a BTreeMap<String, BrickMesh>,
    /// Bricks drawn elsewhere or not at all: they cover nothing.
    pub left_out: &'a BTreeSet<u64>,
    pub invalid: &'a BTreeSet<u64>,
    pub face_proofs: FaceProofs,
}

impl Covers<'_> {
    fn region(&self, mesh: &BrickMesh, face: usize) -> Option<(Vec3, Vec3)> {
        let mut cache = self
            .face_proofs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .entry(std::ptr::from_ref(mesh) as usize)
            .or_insert_with(|| std::array::from_fn(|face| authored_face_region(mesh, face)))[face]
    }

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
        let mut seen = BTreeSet::new();
        let mut covered = BTreeSet::new();
        for (face, cover) in coverage.iter().enumerate() {
            if cover.required_area <= 0.0 {
                continue;
            }
            let (axis, positive) = world_side(brick, face);
            let axes = match axis {
                0 => [1, 2],
                1 => [0, 2],
                _ => [0, 1],
            };
            let Some((local_min, local_max)) = self.region(mesh, face) else {
                continue;
            };
            let transform = brick.transform();
            let a = transform.transform_point3(local_min);
            let b = transform.transform_point3(local_max);
            let min = a.min(b);
            let max = a.max(b);
            // The actual projected quilt must first fill its rectangle. The
            // tiny outward skin may overlap logical cell edges, but projected
            // vertices are never moved: skew slivers and holes remain real.
            // A sparse face cannot be hidden by coverage outside its region.
            let target_lo = [0, 1].map(|a| whole_cell_boundary(min[axes[a]], axes[a], true));
            let target_hi = [0, 1].map(|a| whole_cell_boundary(max[axes[a]], axes[a], false));
            if (0..2).any(|a| {
                (min[axes[a]] - target_lo[a] as f32 * bri_sim::grid::CELL[axes[a]]).abs()
                    > GRID_SKIN + 0.0001
                    || (max[axes[a]] - target_hi[a] as f32 * bri_sim::grid::CELL[axes[a]]).abs()
                        > GRID_SKIN + 0.0001
            }) {
                continue;
            }
            let face_cells =
                i64::from(target_hi[0] - target_lo[0]) * i64::from(target_hi[1] - target_lo[1]);
            if face_cells <= 0
                || face_cells > MAX_COVER_CELLS
                || cover.required_area > face_cells as f32
            {
                continue;
            }
            let mut cell_checks = 0i64;
            let mut exceeded = false;
            let plane = if positive {
                own.max()[axis]
            } else {
                own.min[axis]
            };
            // The one-cell slab just outside the face.
            let mut slab = own;
            slab.min[axis] = if positive { plane } else { plane - 1 };
            slab.size[axis] = 1;
            covered.clear();
            seen.clear();
            self.index.visit(slab, |other, found| {
                if other == id
                    || exceeded
                    || covered.len() as i64 >= face_cells
                    || !seen.insert(other)
                {
                    return;
                }
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
                if covered.len() as i64 >= face_cells {
                    return;
                }
                let region = self.region(neighbour_mesh, facing);
                let Some((local_min, local_max)) = region else {
                    return;
                };
                let transform = neighbour.transform();
                let a = transform.transform_point3(local_min);
                let b = transform.transform_point3(local_max);
                let min = a.min(b);
                let max = a.max(b);
                let lo = [0, 1].map(|a| whole_cell_boundary(min[axes[a]], axes[a], true));
                let hi = [0, 1].map(|a| whole_cell_boundary(max[axes[a]], axes[a], false));
                let a0 = own.min[axes[0]]
                    .max(found.min[axes[0]])
                    .max(lo[0])
                    .max(target_lo[0]);
                let a1 = own.max()[axes[0]]
                    .min(found.max()[axes[0]])
                    .min(hi[0])
                    .min(target_hi[0]);
                let b0 = own.min[axes[1]]
                    .max(found.min[axes[1]])
                    .max(lo[1])
                    .max(target_lo[1]);
                let b1 = own.max()[axes[1]]
                    .min(found.max()[axes[1]])
                    .min(hi[1])
                    .min(target_hi[1]);
                for a in a0..a1 {
                    for b in b0..b1 {
                        cell_checks += 1;
                        if cell_checks > MAX_COVER_CELLS {
                            exceeded = true;
                            return;
                        }
                        let mut cell = found.min;
                        cell[axis] = if positive {
                            found.min[axis]
                        } else {
                            found.max()[axis] - 1
                        };
                        cell[axes[0]] = a;
                        cell[axes[1]] = b;
                        if found.cell(cell, neighbour.quarter_turns, neighbour_mesh) != b'-' {
                            covered.insert([a, b]);
                            if covered.len() as i64 >= face_cells {
                                return;
                            }
                        }
                    }
                }
            });
            if !exceeded && covered.len() as i64 >= face_cells {
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
