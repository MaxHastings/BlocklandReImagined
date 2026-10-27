//! A plastic brick cracked by a hard blow: the shards its debris becomes.
//!
//! Plastic lets go at its joints; only a blow that lands on the brick itself
//! cracks it, into a few large pieces, never crumbs. A crack runs across the
//! brick between two rows of studs (or, up a tall brick, between two brick
//! heights), at the row nearest the blow, and wanders off that row by at
//! most half a stud as it crosses: so every shard keeps at least half a stud
//! of brick each side of a crack, and an axis only one stud (or one brick)
//! long never cracks. A 1x1 stays whole, a 1x4 breaks in two and a 2x4 in
//! four.
//!
//! Only a plain box cracks ([`bri_sim::definitions::Definition::is_plain_box`]):
//! its shards are that box cut by the cracks, so the brick's own faces are
//! clipped to each shard and the cut faces are capped in its plastic.
//! Everything here is in the brick's own frame (no quarter turns) and is
//! cosmetic: it decides how debris looks, never what is standing.
use bri_content::brick::{Brick as BrickMesh, PLATE, STUD};
use bri_render::scene::{MeshBatch, SceneData, SceneVertex};
use glam::Vec3;

/// Most cracks a brick takes: one across each axis.
pub const MAX_CRACKS: usize = 3;
/// Most shards: each crack halves every shard it crosses.
pub const MAX_SHARDS: usize = 1 << MAX_CRACKS;
/// Plates in one brick's height: the unit a tall brick cracks between.
const BRICK_PLATES: f32 = 3.0;

/// A crack: the points `x` with `normal . x == offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crack {
    pub normal: Vec3,
    pub offset: f32,
}
impl Crack {
    fn distance(&self, p: Vec3) -> f32 {
        self.normal.dot(p) - self.offset
    }
}

/// The cracks a blow at `impact` (brick frame) puts across `mesh`;
/// `random` gives numbers in [-1, 1] (the same on every client for one
/// cue). Empty when no axis is long enough to crack.
pub fn cracks(mesh: &BrickMesh, impact: Vec3, mut random: impl FnMut() -> f32) -> Vec<Crack> {
    let half = mesh.half_size();
    let unit = Vec3::new(STUD, BRICK_PLATES * PLATE, STUD);
    let mut out = Vec::new();
    for axis in 0..3 {
        // Whole units along this axis; a crack needs one each side.
        let count = (2.0 * half[axis] / unit[axis] + 1e-3).floor();
        if count < 2.0 {
            continue;
        }
        let along = ((impact[axis] + half[axis]) / unit[axis])
            .round()
            .clamp(1.0, count - 1.0);
        let line = -half[axis] + along * unit[axis];
        // Leaning across each other axis moves the crack at most a quarter
        // unit at that axis' ends: half a unit off its line in all.
        let mut normal = Vec3::ZERO;
        normal[axis] = 1.0;
        for other in (0..3).filter(|&o| o != axis) {
            normal[other] = -random() * 0.25 * unit[axis] / half[other];
        }
        let length = normal.length();
        out.push(Crack {
            normal: normal / length,
            offset: line / length,
        });
    }
    out
}

/// One shard: its side of every crack (bit `i` of `index` set: the far
/// side of crack `i`), where its middle is and the corners of its convex
/// hull about that middle.
#[derive(Clone, Debug)]
pub struct Shard {
    pub index: u8,
    pub center: Vec3,
    pub hull: Vec<Vec3>,
}

fn side(index: u8, crack: usize) -> f32 {
    if index >> crack & 1 == 1 { -1.0 } else { 1.0 }
}

/// The shards `cracks` cut a box of `half` size into, skipping any a
/// crack leaves empty.
pub fn shards(half: Vec3, cracks: &[Crack]) -> Vec<Shard> {
    (0..1u8 << cracks.len())
        .filter_map(|index| {
            let faces = cell_faces(half, cracks, index);
            let mut points: Vec<Vec3> = Vec::new();
            for p in faces.iter().flat_map(|f| f.polygon.iter()) {
                if points.iter().all(|q| q.distance_squared(*p) > 1e-8) {
                    points.push(*p);
                }
            }
            if points.len() < 4 {
                return None;
            }
            let center = points.iter().copied().sum::<Vec3>() / points.len() as f32;
            Some(Shard {
                index,
                center,
                hull: points.into_iter().map(|p| p - center).collect(),
            })
        })
        .collect()
}

/// One face of a shard: wound counterclockwise seen from outside.
struct Face {
    normal: Vec3,
    polygon: Vec<Vec3>,
    /// The crack it was cut along; None for the brick's own sides.
    cut: Option<usize>,
}

/// Shard `index` of a box of `half` size as polygons: the box's faces
/// clipped to it, then its cut faces.
fn cell_faces(half: Vec3, cracks: &[Crack], index: u8) -> Vec<Face> {
    let mut faces = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let mut normal = Vec3::ZERO;
            normal[axis] = sign;
            faces.push(Face {
                normal,
                polygon: square(normal, half[axis], half.max_element() * 2.0),
                cut: None,
            });
        }
    }
    // Each crack's cut, seen from this shard: outward is away from its side.
    for (i, crack) in cracks.iter().enumerate() {
        let normal = -crack.normal * side(index, i);
        faces.push(Face {
            normal,
            polygon: square(normal, -crack.offset * side(index, i), half.length() * 2.0),
            cut: Some(i),
        });
    }
    // A hair of slack keeps each side, which lies on its own plane.
    let slack = 1e-5 * half.max_element();
    for face in &mut faces {
        for axis in 0..3 {
            for sign in [-1.0, 1.0] {
                face.polygon = clip(
                    &face.polygon,
                    |p| half[axis] + slack - sign * p[axis],
                    |a, b, t| a.lerp(*b, t),
                );
            }
        }
        for (i, crack) in cracks.iter().enumerate() {
            if face.cut != Some(i) {
                face.polygon = clip(
                    &face.polygon,
                    |p| crack.distance(p) * side(index, i),
                    |a, b, t| a.lerp(*b, t),
                );
            }
        }
    }
    faces.retain(|f| f.polygon.len() >= 3);
    faces
}

/// A square of side `2 * size` on the plane `normal . x == offset`, wound
/// counterclockwise about `normal`.
fn square(normal: Vec3, offset: f32, size: f32) -> Vec<Vec3> {
    let (u, v) = normal.any_orthonormal_pair();
    let middle = normal * offset;
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .into_iter()
        .map(|(a, b)| middle + (u * a + v * b) * size)
        .collect();
    wind(corners, normal)
}

/// `polygon` wound counterclockwise about `normal`.
fn wind(mut polygon: Vec<Vec3>, normal: Vec3) -> Vec<Vec3> {
    let mut area = Vec3::ZERO;
    for (i, a) in polygon.iter().enumerate() {
        area += a.cross(polygon[(i + 1) % polygon.len()]);
    }
    if area.dot(normal) < 0.0 {
        polygon.reverse();
    }
    polygon
}

/// Sutherland-Hodgman: the part of `polygon` where `inside` is at least 0,
/// new corners made by `lerp`.
fn clip<V: Clone>(
    polygon: &[V],
    inside: impl Fn(Vec3) -> f32,
    lerp: impl Fn(&V, &V, f32) -> V,
) -> Vec<V>
where
    V: Positioned,
{
    let mut out = Vec::with_capacity(polygon.len() + 1);
    for (i, a) in polygon.iter().enumerate() {
        let b = &polygon[(i + 1) % polygon.len()];
        let (da, db) = (inside(a.position()), inside(b.position()));
        if da >= 0.0 {
            out.push(a.clone());
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(lerp(a, b, da / (da - db)));
        }
    }
    out
}

trait Positioned {
    fn position(&self) -> Vec3;
}
impl Positioned for Vec3 {
    fn position(&self) -> Vec3 {
        *self
    }
}
impl Positioned for SceneVertex {
    fn position(&self) -> Vec3 {
        Vec3::from(self.position)
    }
}

fn lerp_vertex(a: &SceneVertex, b: &SceneVertex, t: f32) -> SceneVertex {
    let mix = |x: &[f32], y: &[f32], out: &mut [f32]| {
        for ((o, x), y) in out.iter_mut().zip(x).zip(y) {
            *o = x + (y - x) * t;
        }
    };
    let mut v = *a;
    mix(&a.position, &b.position, &mut v.position);
    mix(&a.normal, &b.normal, &mut v.normal);
    mix(&a.uv, &b.uv, &mut v.uv);
    mix(&a.lightmap_uv, &b.lightmap_uv, &mut v.lightmap_uv);
    mix(&a.color, &b.color, &mut v.color);
    // `fx` packs ids: it is the brick's, whole, never blended.
    v
}

/// Shard `shard` of the brick `scene` draws (built at the origin of its
/// own frame, as debris looks are): the brick's faces on its side of every
/// crack and its cut faces in the brick's plastic, about its middle.
pub fn shard_scene(scene: &SceneData, half: Vec3, cracks: &[Crack], shard: &Shard) -> SceneData {
    let mut out = SceneData {
        vertices: Vec::new(),
        indices: Vec::new(),
        batches: Vec::new(),
        ..scene.clone()
    };
    let keep = |polygon: Vec<SceneVertex>| {
        cracks
            .iter()
            .enumerate()
            .fold(polygon, |polygon, (i, crack)| {
                clip(
                    &polygon,
                    |p| crack.distance(p) * side(shard.index, i),
                    lerp_vertex,
                )
            })
    };
    // The plastic of the brick's sides paints the cuts.
    let mut plastic = None;
    for batch in &scene.batches {
        let start = out.indices.len() as u32;
        for triangle in
            scene.indices[batch.indices.start as usize..batch.indices.end as usize].chunks_exact(3)
        {
            let corners: Vec<SceneVertex> = triangle
                .iter()
                .map(|&i| scene.vertices[i as usize])
                .collect();
            if plastic.is_none() && corners.iter().all(|v| v.normal[1].abs() < 0.5) {
                plastic = Some((batch.material, corners[0]));
            }
            fan(&mut out, keep(corners), shard.center);
        }
        let end = out.indices.len() as u32;
        if end > start {
            out.batches.push(MeshBatch {
                indices: start..end,
                material: batch.material,
                center: (Vec3::from(batch.center) - shard.center).to_array(),
                zone: batch.zone,
            });
        }
    }
    let Some((material, template)) = plastic.or_else(|| {
        let batch = scene.batches.first()?;
        Some((
            batch.material,
            scene.vertices[scene.indices[batch.indices.start as usize] as usize],
        ))
    }) else {
        return out;
    };
    let start = out.indices.len() as u32;
    for face in cell_faces(half, cracks, shard.index)
        .into_iter()
        .filter(|f| f.cut.is_some())
    {
        let normal = face.normal;
        let polygon = face
            .polygon
            .into_iter()
            .map(|p| SceneVertex {
                position: p.to_array(),
                normal: normal.to_array(),
                uv: [0.0; 2],
                lightmap_uv: [0.0; 2],
                ..template
            })
            .collect();
        fan(&mut out, polygon, shard.center);
    }
    let end = out.indices.len() as u32;
    if end > start {
        out.batches.push(MeshBatch {
            indices: start..end,
            material,
            center: [0.0; 3],
            zone: None,
        });
    }
    out
}

/// Triangulate a convex `polygon` into `scene`, moved by `-center`.
fn fan(scene: &mut SceneData, polygon: Vec<SceneVertex>, center: Vec3) {
    if polygon.len() < 3 {
        return;
    }
    let first = scene.vertices.len() as u32;
    scene.vertices.extend(polygon.iter().map(|v| SceneVertex {
        position: (Vec3::from(v.position) - center).to_array(),
        ..*v
    }));
    for i in 1..polygon.len() as u32 - 1 {
        scene.indices.extend([first, first + i, first + i + 1]);
    }
}
