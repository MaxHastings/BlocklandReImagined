//! Mirror images of bricks, for mirroring a copied build.
//!
//! A brick seen in a mirror is either the same brick turned (a plain brick,
//! a ramp seen side on) or another brick of the catalog (a left wedge and
//! its right twin). Which one is read from the bricks' own shapes, never
//! from a hand-kept list, so Add-On bricks mirror too: a brick's image is
//! the first brick, itself first, whose drawn surfaces and collision are
//! exactly its own reflected across its middle, in some quarter turn.
//!
//! A brick with no image anywhere in the catalog keeps its own shape and is
//! turned as a symmetric brick would be; it still covers the same cells.
use crate::definitions::{Definition, Definitions};
use bri_content::collision::Part;
use std::collections::BTreeMap;

/// What a brick becomes in a mirror across its own x axis: `definition`,
/// turned `turns` quarter turns (the way `Brick::quarter_turns` turns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorImage {
    pub definition: String,
    pub turns: u8,
    /// Whether the shape really is the reflection (false when the catalog
    /// has no brick for it and it was only turned).
    pub exact: bool,
}

/// A shape reduced to comparable points: each drawn quad and each collision
/// part as a sorted set of its corners, in thousandths of a unit.
type Signature = (Vec<Vec<[i32; 3]>>, Vec<Vec<[i32; 3]>>);

/// Mirror images found so far. Both the host and each player find the same
/// ones from the same catalog, so a mirrored copy's ghost is what plants.
#[derive(Debug, Default, Clone)]
pub struct Mirrors {
    images: BTreeMap<String, MirrorImage>,
    signatures: BTreeMap<String, Signature>,
}

impl Mirrors {
    /// The image of the brick `id`.
    pub fn image(&mut self, definitions: &Definitions, id: &str) -> MirrorImage {
        if let Some(image) = self.images.get(id) {
            return image.clone();
        }
        let image = self.find(definitions, id);
        self.images.insert(id.to_string(), image.clone());
        image
    }

    fn find(&mut self, definitions: &Definitions, id: &str) -> MirrorImage {
        let fallback = MirrorImage {
            definition: id.to_string(),
            turns: 0,
            exact: false,
        };
        let Some(source) = definitions.entries.get(id) else {
            return fallback;
        };
        let reflected = reflect(&self.signature(id, source));
        let [w, d] = source.mesh.footprint_studs;
        let height = source.mesh.height_plates;
        // Itself first, then the rest of the catalog in id order.
        let candidates = std::iter::once(id).chain(
            definitions
                .entries
                .iter()
                .filter(|(other, definition)| {
                    other.as_str() != id
                        && definition.mesh.height_plates == height
                        && {
                            let [ow, od] = definition.mesh.footprint_studs;
                            (ow, od) == (w, d) || (ow, od) == (d, w)
                        }
                })
                .map(|(other, _)| other.as_str()),
        );
        for candidate in candidates {
            let definition = &definitions.entries[candidate];
            let [cw, cd] = definition.mesh.footprint_studs;
            let signature = self.signature(candidate, definition).clone();
            for turns in 0..4u8 {
                // A turned candidate must cover the source's footprint.
                let covers = if turns % 2 == 0 {
                    (cw, cd) == (w, d)
                } else {
                    (cd, cw) == (w, d)
                };
                if covers && turned(&signature, turns) == reflected {
                    return MirrorImage {
                        definition: candidate.to_string(),
                        turns,
                        exact: true,
                    };
                }
            }
        }
        fallback
    }

    fn signature(&mut self, id: &str, definition: &Definition) -> &Signature {
        self.signatures
            .entry(id.to_string())
            .or_insert_with(|| signature(definition))
    }
}

fn quantize(p: [f32; 3]) -> [i32; 3] {
    p.map(|v| (v * 1000.0).round() as i32)
}

fn canonical(mut sets: Vec<Vec<[i32; 3]>>) -> Vec<Vec<[i32; 3]>> {
    for set in &mut sets {
        set.sort_unstable();
    }
    sets.sort_unstable();
    sets
}

fn signature(definition: &Definition) -> Signature {
    let quads = definition
        .mesh
        .quads
        .iter()
        .map(|q| q.vertices.iter().map(|v| quantize(v.position)).collect())
        .collect();
    let parts = definition
        .collision
        .parts
        .iter()
        .map(|part| match part {
            Part::Box { center, size } => (0..8)
                .map(|i: usize| {
                    quantize(std::array::from_fn(|a| {
                        center[a] + if i >> a & 1 == 0 { -0.5 } else { 0.5 } * size[a]
                    }))
                })
                .collect(),
            Part::Convex { vertices, .. } => vertices.iter().map(|v| quantize(*v)).collect(),
        })
        .collect();
    (canonical(quads), canonical(parts))
}

fn map(signature: &Signature, f: impl Fn([i32; 3]) -> [i32; 3]) -> Signature {
    let apply = |sets: &Vec<Vec<[i32; 3]>>| {
        canonical(
            sets.iter()
                .map(|set| set.iter().map(|p| f(*p)).collect())
                .collect(),
        )
    };
    (apply(&signature.0), apply(&signature.1))
}

/// Across the brick's own x axis: x becomes -x.
fn reflect(signature: &Signature) -> Signature {
    map(signature, |[x, y, z]| [-x, y, z])
}

/// Turned like `Brick::transform`: each quarter turn takes (x, z) to (-z, x).
fn turned(signature: &Signature, turns: u8) -> Signature {
    map(signature, |p| {
        let mut p = p;
        for _ in 0..turns % 4 {
            p = [-p[2], p[1], p[0]];
        }
        p
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::{
        brick::{Brick as Mesh, Face, Quad, Surface, Vertex},
        collision::CollisionBody,
    };
    use rapier3d::prelude::SharedShape;

    /// A brick drawn as one quad through the given corners.
    pub(crate) fn shaped(w: u32, d: u32, h: u32, corners: [[f32; 3]; 4]) -> Definition {
        let vertex = |position| Vertex {
            position,
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
        };
        Definition {
            mesh: Mesh {
                schema_version: 1,
                id: "shape".into(),
                footprint_studs: [w, d],
                height_plates: h,
                attachment_rows: vec!["b".repeat(w as usize); (d * h) as usize],
                collision_boxes: vec![],
                needs_external_collision: false,
                coverage: None,
                quads: vec![Quad {
                    face: Face::Top,
                    surface: Surface::Ramp,
                    vertices: corners.map(vertex),
                    colors: None,
                }],
            },
            collision: CollisionBody {
                id: "box".into(),
                parts: vec![Part::Box {
                    center: [0.0; 3],
                    size: [w as f32 * 0.5, h as f32 * 0.2, d as f32 * 0.5],
                }],
            },
            shape: SharedShape::cuboid(w as f32 * 0.25, h as f32 * 0.1, d as f32 * 0.25),
            indestructible: false,
            special: Default::default(),
            reflection: None,
        }
    }

    #[test]
    fn symmetric_bricks_are_their_own_image_and_twins_find_each_other() {
        // A flat 2x1 plate top: its own reflection unturned.
        let plate = shaped(
            2,
            1,
            1,
            [
                [-0.5, 0.1, -0.25],
                [0.5, 0.1, -0.25],
                [0.5, 0.1, 0.25],
                [-0.5, 0.1, 0.25],
            ],
        );
        // A 2x2 slope falling toward -z: symmetric across x too.
        let ramp = shaped(
            2,
            2,
            3,
            [
                [-0.5, -0.3, -0.5],
                [0.5, -0.3, -0.5],
                [0.5, 0.3, 0.5],
                [-0.5, 0.3, 0.5],
            ],
        );
        // A 2x2 corner slope rising toward +x and +z, and its twin rising
        // toward -x and +z.
        let right = shaped(
            2,
            2,
            3,
            [
                [-0.5, -0.3, -0.5],
                [0.5, 0.0, -0.5],
                [0.5, 0.3, 0.5],
                [-0.5, 0.0, 0.5],
            ],
        );
        let mut left = right.clone();
        for v in &mut left.mesh.quads[0].vertices {
            v.position[0] = -v.position[0];
        }
        // A corner slope whose twin is not in the catalog.
        let lonely = shaped(
            2,
            2,
            3,
            [
                [-0.5, -0.3, -0.5],
                [0.5, 0.1, -0.5],
                [0.5, 0.3, 0.5],
                [-0.5, 0.0, 0.5],
            ],
        );
        let definitions = Definitions {
            entries: [
                ("plate".to_string(), plate),
                ("ramp".to_string(), ramp),
                ("right".to_string(), right),
                ("left".to_string(), left),
                ("lonely".to_string(), lonely),
            ]
            .into(),
        };
        let mut mirrors = Mirrors::default();
        let image = |m: &mut Mirrors, id| {
            let i = m.image(&definitions, id);
            (i.definition, i.turns, i.exact)
        };
        assert_eq!(image(&mut mirrors, "plate"), ("plate".into(), 0, true));
        assert_eq!(image(&mut mirrors, "ramp"), ("ramp".into(), 0, true));
        // The corner slope is symmetric across its diagonal, so its mirror
        // is itself turned; the twin is only needed when it is not.
        let found = image(&mut mirrors, "right");
        assert!(found.2);
        assert_eq!(image(&mut mirrors, "left").2, true);
        assert_eq!(image(&mut mirrors, "lonely"), ("lonely".into(), 0, false));
        // Unknown bricks keep their shape.
        assert!(!mirrors.image(&definitions, "nothing").exact);
    }

    #[test]
    fn a_chiral_brick_mirrors_into_its_twin() {
        // A wedge whose top is not symmetric across any turn of itself:
        // three distinct heights on four corners.
        let right = shaped(
            2,
            1,
            3,
            [
                [-0.5, -0.3, -0.25],
                [0.5, 0.1, -0.25],
                [0.5, 0.3, 0.25],
                [-0.5, 0.1, 0.25],
            ],
        );
        let mut left = right.clone();
        for v in &mut left.mesh.quads[0].vertices {
            v.position[0] = -v.position[0];
        }
        let definitions = Definitions {
            entries: [("a-right".to_string(), right), ("b-left".to_string(), left)].into(),
        };
        let mut mirrors = Mirrors::default();
        let image = mirrors.image(&definitions, "a-right");
        assert_eq!((image.definition.as_str(), image.exact), ("b-left", true));
        let image = mirrors.image(&definitions, "b-left");
        assert_eq!((image.definition.as_str(), image.exact), ("a-right", true));
    }

    #[test]
    fn a_mirrored_copy_is_the_reflection_of_the_build() {
        let right = shaped(
            2,
            1,
            3,
            [
                [-0.5, -0.3, -0.25],
                [0.5, 0.1, -0.25],
                [0.5, 0.3, 0.25],
                [-0.5, 0.1, 0.25],
            ],
        );
        let mut left = right.clone();
        for v in &mut left.mesh.quads[0].vertices {
            v.position[0] = -v.position[0];
        }
        let definitions = Definitions {
            entries: [("a-right".to_string(), right), ("b-left".to_string(), left)].into(),
        };
        let brick = |id: &str, position: [f32; 3], turns: u8| {
            let mut b = bri_world::Brick::new(
                bri_world::ContentRef::Resolved(id.into()),
                position,
                1,
            );
            b.quarter_turns = turns;
            b
        };
        let build = [
            brick("a-right", [0.5, 0.3, 0.25], 0),
            brick("a-right", [0.25, 0.9, 1.0], 1),
            brick("b-left", [-1.0, 0.3, -0.75], 2),
            brick("a-right", [1.25, 0.3, -1.5], 3),
        ];
        let copy = crate::blueprint::Blueprint::capture("dup:weapon/tool", &build, &definitions)
            .unwrap();
        let mut mirrors = Mirrors::default();
        let (mirrored, inexact) = copy.mirrored(|id| mirrors.image(&definitions, id));
        assert_eq!(inexact, 0);
        let points = |bricks: &[bri_world::Brick], flip: bool| {
            let mut out: Vec<[i32; 3]> = bricks
                .iter()
                .flat_map(|b| {
                    let mesh = &definitions.get(b).unwrap().mesh;
                    // Every brick stays on the grid.
                    crate::grid::Bounds::new(b, mesh).unwrap();
                    let transform = b.transform();
                    mesh.quads[0]
                        .vertices
                        .iter()
                        .map(move |v| {
                            let mut p = transform
                                .transform_point3(glam::Vec3::from(v.position))
                                .to_array();
                            if flip {
                                p[0] = -p[0];
                            }
                            quantize(p)
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            out.sort_unstable();
            out
        };
        // Placed where it was taken, the mirrored copy is the reflection.
        let expected = points(&copy.placed([0.0; 3], 0), true);
        assert_eq!(points(&mirrored.placed([0.0; 3], 0), false), expected);
        // The twins swapped.
        let names: Vec<_> = mirrored
            .bricks
            .iter()
            .map(|b| match &b.definition {
                bri_world::ContentRef::Resolved(id) => id.clone(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(names, ["b-left", "b-left", "a-right", "b-left"]);
    }
}
