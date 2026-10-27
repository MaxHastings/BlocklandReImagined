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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bri_content::brick::{BRICK_SCHEMA, Face as Side, Quad, Surface, Vertex};

    /// A plain box brick, `width` x `depth` studs and `plates` high, its
    /// six sides wound counterclockwise seen from outside.
    pub(crate) fn box_mesh(id: &str, width: u32, depth: u32, plates: u32) -> BrickMesh {
        let half = Vec3::new(
            width as f32 * STUD,
            plates as f32 * PLATE,
            depth as f32 * STUD,
        ) * 0.5;
        let sides = [
            (Side::Top, Vec3::Y),
            (Side::Bottom, Vec3::NEG_Y),
            (Side::North, Vec3::NEG_Z),
            (Side::South, Vec3::Z),
            (Side::East, Vec3::X),
            (Side::West, Vec3::NEG_X),
        ];
        let quads = sides
            .into_iter()
            .map(|(face, normal)| {
                let (u, v) = normal.any_orthonormal_pair();
                let corners = wind(
                    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                        .into_iter()
                        .map(|(a, b)| (normal + u * a + v * b) * half)
                        .collect(),
                    normal,
                );
                Quad {
                    face,
                    surface: if face == Side::Top {
                        Surface::Top
                    } else {
                        Surface::Side
                    },
                    vertices: std::array::from_fn(|i| Vertex {
                        position: corners[i].to_array(),
                        normal: normal.to_array(),
                        uv: [0.0; 2],
                    }),
                    colors: None,
                }
            })
            .collect();
        BrickMesh {
            schema_version: BRICK_SCHEMA,
            id: id.into(),
            footprint_studs: [width, depth],
            height_plates: plates,
            attachment_rows: vec!["b".repeat(width as usize); (depth * plates) as usize],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads,
        }
    }

    /// The scene a brick's look draws, straight from its quads.
    fn scene(mesh: &BrickMesh) -> SceneData {
        let mut scene = SceneData::default();
        for quad in &mesh.quads {
            let first = scene.vertices.len() as u32;
            scene
                .vertices
                .extend(quad.vertices.iter().map(|v| SceneVertex {
                    position: v.position,
                    normal: v.normal,
                    uv: v.uv,
                    lightmap_uv: [0.0; 2],
                    color: [1.0; 4],
                    fx: [0.0; 4],
                }));
            scene
                .indices
                .extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
        scene.batches.push(MeshBatch {
            indices: 0..scene.indices.len() as u32,
            material: 0,
            center: [0.0; 3],
            zone: None,
        });
        scene
    }

    fn area(scene: &SceneData, batch: &MeshBatch) -> f32 {
        scene.indices[batch.indices.start as usize..batch.indices.end as usize]
            .chunks_exact(3)
            .map(|t| {
                let [a, b, c] =
                    [0, 1, 2].map(|i| Vec3::from(scene.vertices[t[i] as usize].position));
                (b - a).cross(c - a).length() * 0.5
            })
            .sum()
    }

    fn volume(hull: &[Vec3]) -> f32 {
        use rapier3d::prelude::*;
        let points: Vec<Vector> = hull
            .iter()
            .map(|p| Vector::from_array(p.to_array()))
            .collect();
        ColliderBuilder::convex_hull(&points)
            .expect("a shard is a solid")
            .density(1.0)
            .build()
            .mass()
    }

    /// Fixed "random" leans, for repeatable cracks.
    fn leaning(leans: &[f32]) -> impl FnMut() -> f32 + '_ {
        let mut i = 0;
        move || {
            i += 1;
            leans[(i - 1) % leans.len()]
        }
    }

    #[test]
    fn plastic_cracks_between_studs_and_only_where_there_is_room() {
        let impact = Vec3::new(0.1, 0.0, 0.3);
        let count = |w, d, p| cracks(&box_mesh("b", w, d, p), impact, leaning(&[0.7, -1.0])).len();
        assert_eq!(count(1, 1, 3), 0, "a 1x1 brick stays whole");
        assert_eq!(count(1, 1, 1), 0, "a 1x1 plate stays whole");
        assert_eq!(count(1, 4, 3), 1, "a 1x4 breaks in two");
        assert_eq!(count(2, 4, 3), 2, "a 2x4 breaks in four");
        assert_eq!(count(2, 4, 1), 2, "a 2x4 plate breaks in four");
        assert_eq!(count(4, 4, 12), 3, "a 4x cube cracks across its height too");
    }

    #[test]
    fn a_crack_stays_within_half_a_stud_of_the_row_nearest_the_blow() {
        let mesh = box_mesh("b", 2, 8, 3);
        let half = mesh.half_size();
        // Struck near one end: the crack takes the row nearest it, never
        // the brick's own end, however it leans.
        for impact in [Vec3::new(0.2, 0.1, -1.9), Vec3::new(-0.4, 0.0, 0.6)] {
            for lean in [-1.0, 1.0] {
                let found = cracks(&mesh, impact, leaning(&[lean]));
                let across = found
                    .iter()
                    .max_by(|a, b| a.normal.z.abs().total_cmp(&b.normal.z.abs()))
                    .expect("cracked across its length");
                let row = ((impact.z + half.z) / STUD).round().clamp(1.0, 7.0) * STUD - half.z;
                for corner in [-1.0, 1.0]
                    .into_iter()
                    .flat_map(|x| [-1.0, 1.0].map(move |y| Vec3::new(x * half.x, y * half.y, 0.0)))
                {
                    // Where the crack crosses this edge of the brick.
                    let z =
                        (across.offset - across.normal.x * corner.x - across.normal.y * corner.y)
                            / across.normal.z;
                    assert!((z - row).abs() <= STUD * 0.5 + 1e-4, "{z} off row {row}");
                    assert!(z.abs() < half.z, "crack left through an end");
                }
            }
        }
    }

    #[test]
    fn shards_fill_the_brick_exactly() {
        for (w, d, p) in [(2, 4, 3), (1, 4, 3), (4, 4, 12), (2, 2, 1)] {
            let mesh = box_mesh("b", w, d, p);
            let half = mesh.half_size();
            let found = cracks(
                &mesh,
                Vec3::new(0.05, 0.1, -0.2),
                leaning(&[0.9, -0.6, 0.3]),
            );
            let pieces = shards(half, &found);
            assert_eq!(pieces.len(), 1 << found.len(), "{w}x{d}x{p}");
            let total: f32 = pieces.iter().map(|s| volume(&s.hull)).sum();
            let whole = 8.0 * half.x * half.y * half.z;
            assert!((total - whole).abs() < whole * 1e-3, "{total} of {whole}");
            for s in &pieces {
                for p in &s.hull {
                    let p = *p + s.center;
                    assert!(
                        (p.abs() - half).max_element() < 1e-4,
                        "{p} outside the brick"
                    );
                }
            }
        }
    }

    #[test]
    fn each_shard_draws_its_part_of_the_brick_and_caps_its_cuts() {
        let mesh = box_mesh("b", 2, 4, 3);
        let half = mesh.half_size();
        let whole = scene(&mesh);
        let skin = area(&whole, &whole.batches[0]);
        let found = cracks(&mesh, Vec3::new(0.1, 0.0, 0.4), leaning(&[0.5, -0.8]));
        let (mut outside, mut cuts) = (0.0, 0.0);
        for shard in shards(half, &found) {
            let drawn = shard_scene(&whole, half, &found, &shard);
            assert_eq!(drawn.batches.len(), 2, "its share of the skin and its cuts");
            outside += area(&drawn, &drawn.batches[0]);
            cuts += area(&drawn, &drawn.batches[1]);
            // Every corner is on its side of each crack, about its middle.
            for v in &drawn.vertices {
                let p = Vec3::from(v.position) + shard.center;
                for (i, crack) in found.iter().enumerate() {
                    assert!(crack.distance(p) * side(shard.index, i) > -1e-4);
                }
            }
            // Cuts face out of the shard and are wound to be seen.
            let batch = &drawn.batches[1];
            for t in drawn.indices[batch.indices.start as usize..batch.indices.end as usize]
                .chunks_exact(3)
            {
                let [a, b, c] = [0, 1, 2].map(|i| drawn.vertices[t[i] as usize]);
                let n = Vec3::from(a.normal);
                let wound = (Vec3::from(b.position) - Vec3::from(a.position))
                    .cross(Vec3::from(c.position) - Vec3::from(a.position));
                assert!(wound.dot(n) > 0.0, "a cut wound away from its normal");
                assert!(
                    Vec3::from(a.position).dot(n) > -1e-4,
                    "a cut facing into its shard"
                );
            }
        }
        assert!((outside - skin).abs() < skin * 1e-3, "{outside} of {skin}");
        // Each crack is cut twice, once on each side; across a 2x4 brick each
        // is at least as big as the brick's section along it.
        let section = 2.0 * half.y * 2.0 * half.x + 2.0 * half.y * 2.0 * half.z;
        assert!(cuts > section * 2.0 * 0.999, "{cuts}");
    }
}
