//! A native brick (`bri_content::brick::Brick`, what a converted BLB is) as a
//! glTF binary: its quads drawn as triangles in the player's paint colour,
//! each quad's face and surface kept in `bri_brick` extras so the game can
//! lower it back to the very same brick, and the grid size, attachment rows,
//! coverage and collision boxes in the asset's `bri_brick_meta`. Blender sees
//! a plain mesh; only the game reads the extras.

use super::read::{Document, array};
use super::shape_export::point as mirror;
use super::write::Gltf;
use anyhow::{Context, Result, ensure};
use bri_content::brick::{
    BRICK_SCHEMA, Brick, CollisionBox, Coverage, Face, Quad, Surface, Vertex,
};
use serde_json::{Value, json};

/// The brick as a `.glb`. One mesh of triangle primitives grouped by the
/// quad's (face, surface); every quad keeps its own four vertices unshared so
/// [`brick_from_glb`] rebuilds it exactly. What glTF has no word for rides in
/// extras: per-primitive `bri_brick` and asset-level `bri_brick_meta`. Before
/// returning the output is lowered again with [`brick_from_glb`] and compared
/// to the input, so an exported brick always reads back as itself.
pub fn export_brick(brick: &Brick) -> Result<Vec<u8>> {
    let mut g = Gltf::new();

    // Group the quads by (face, surface); within a group each quad keeps its
    // own four vertices so the lowering can pair them back.
    let mut groups: Vec<(Face, Surface, Vec<usize>)> = Vec::new();
    for (i, q) in brick.quads.iter().enumerate() {
        match groups
            .iter_mut()
            .find(|(f, s, _)| *f == q.face && *s == q.surface)
        {
            Some((_, _, idx)) => idx.push(i),
            None => groups.push((q.face, q.surface, vec![i])),
        }
    }
    ensure!(!groups.is_empty(), "the brick has no quads to draw");

    let mut primitives = Vec::new();
    for (face, surface, quad_idx) in &groups {
        let positions: Vec<f32> = quad_idx
            .iter()
            .flat_map(|&i| {
                brick.quads[i]
                    .vertices
                    .iter()
                    .flat_map(|v| mirror(v.position))
            })
            .collect();
        let normals: Vec<f32> = quad_idx
            .iter()
            .flat_map(|&i| {
                brick.quads[i]
                    .vertices
                    .iter()
                    .flat_map(|v| mirror(v.normal))
            })
            .collect();
        let uvs: Vec<f32> = quad_idx
            .iter()
            .flat_map(|&i| brick.quads[i].vertices.iter().flat_map(|v| v.uv))
            .collect();
        let indices: Vec<u32> = (0..quad_idx.len())
            .flat_map(|k| {
                let b = 4 * k as u32;
                [b, b + 1, b + 2, b, b + 2, b + 3]
            })
            .collect();

        // The quads' vertex indices within this primitive (four per quad), so
        // the lowering rebuilds each quad without guessing pairings.
        let quads: Vec<Value> = (0..quad_idx.len())
            .map(|k| {
                json!([
                    4 * k as u32,
                    4 * k as u32 + 1,
                    4 * k as u32 + 2,
                    4 * k as u32 + 3
                ])
            })
            .collect();
        let mut extra = json!({
            "bri_brick": {
                "face": face,
                "surface": surface,
                "quads": quads,
            }
        });
        if quad_idx.iter().any(|&i| brick.quads[i].colors.is_some()) {
            // Aligned with `quads`; null where the quad carries no authored
            // colour (the player's paint shows).
            extra["bri_brick"]["colors"] = Value::Array(
                quad_idx
                    .iter()
                    .map(|&i| match &brick.quads[i].colors {
                        Some(c) => Value::Array(
                            c.iter()
                                .map(|v| Value::Array(v.iter().map(|x| json!(x)).collect()))
                                .collect(),
                        ),
                        None => Value::Null,
                    })
                    .collect(),
            );
        }

        let mut prim = json!({
            "attributes": {
                "POSITION": g.floats(&positions, 3),
                "NORMAL": g.floats(&normals, 3),
                "TEXCOORD_0": g.floats(&uvs, 2)
            },
            "indices": g.indices(&indices),
        });
        prim["extras"] = extra;
        primitives.push(prim);
    }

    let mesh_idx = g.mesh("brick", primitives);
    let mut node = json!({ "name": "brick" });
    node["mesh"] = json!(mesh_idx);
    g.node(node, None);

    // The grid and collision facts a plain shape export has no place for.
    let coverage: Option<Value> = brick.coverage.map(|c| {
        Value::Array(
            c.iter()
                .map(|x| json!({ "hides_adjacent": x.hides_adjacent, "required_area": x.required_area }))
                .collect(),
        )
    });
    g.json["asset"]["extras"] = json!({
        "bri_brick_meta": {
            "footprint_studs": brick.footprint_studs,
            "height_plates": brick.height_plates,
            "attachment_rows": brick.attachment_rows,
            "coverage": coverage,
            "needs_external_collision": brick.needs_external_collision,
            "collision_boxes": brick.collision_boxes,
        }
    });

    let glb = g.glb();
    // A .glb the game could not lower would be worse than none: read back
    // what was just written and refuse it while the brick is still at hand.
    let back = brick_from_glb(&glb, &brick.id)
        .with_context(|| "the exported brick glTF cannot be lowered back")?;
    // The lowering rebuilds the quads grouped by (face, surface), so a brick
    // whose file interleaves groups reads back reordered: compare as sets.
    let key = |q: &Quad| serde_json::to_string(q).unwrap_or_default();
    let mut original: Vec<String> = brick.quads.iter().map(key).collect();
    let mut read_back: Vec<String> = back.quads.iter().map(key).collect();
    original.sort_unstable();
    read_back.sort_unstable();
    ensure!(original == read_back, "quad round trip differs");
    ensure!(
        back.footprint_studs == brick.footprint_studs
            && back.height_plates == brick.height_plates
            && back.attachment_rows == brick.attachment_rows
            && back.coverage == brick.coverage
            && back.needs_external_collision == brick.needs_external_collision
            && back.collision_boxes == brick.collision_boxes,
        "brick grid or collision round trip differs"
    );
    Ok(glb)
}

/// The `.glb` an imported brick ships as, lowered to the native [`Brick`] the
/// game draws it from: quads rebuilt from each `bri_brick` primitive's
/// vertices and indices, grid facts from the asset's `bri_brick_meta`.
pub fn brick_from_glb(bytes: &[u8], id: &str) -> Result<Brick> {
    let doc = Document::from_bytes(bytes, None)?;
    let json = &doc.json;

    // Every node of scene 0 that carries a mesh (the exporter writes one).
    let mut stack: Vec<usize> = array(&json["scenes"][0], "nodes")
        .iter()
        .filter_map(|v| v.as_u64())
        .map(|v| v as usize)
        .collect();
    let mut quads = Vec::new();
    while let Some(n) = stack.pop() {
        for child in array(&json["nodes"][n], "children") {
            if let Some(c) = child.as_u64() {
                stack.push(c as usize);
            }
        }
        let Some(mesh) = json["nodes"][n]["mesh"].as_u64() else {
            continue;
        };
        for p in array(&json["meshes"][mesh as usize], "primitives") {
            let extra = &p["extras"]["bri_brick"];
            ensure!(
                extra.is_object(),
                "a brick primitive carries no bri_brick extra"
            );
            let positions = doc.floats(
                p["attributes"]["POSITION"]
                    .as_u64()
                    .context("brick primitive has no positions")? as usize,
            )?;
            let normals = match p["attributes"]["NORMAL"].as_u64() {
                Some(a) => Some(doc.floats(a as usize)?),
                None => None,
            };
            let uvs = match p["attributes"]["TEXCOORD_0"].as_u64() {
                Some(a) => Some(doc.floats(a as usize)?),
                None => None,
            };
            // The triangle index buffer is not needed: each quad names its own
            // four vertices in the extra.
            let face: Face = serde_json::from_value(extra["face"].clone())
                .with_context(|| "unknown brick face in bri_brick extra")?;
            let surface: Surface = serde_json::from_value(extra["surface"].clone())
                .with_context(|| "unknown brick surface in bri_brick extra")?;
            let colors: Option<Vec<Value>> = match extra.get("colors") {
                Some(Value::Array(c)) => Some(c.clone()),
                _ => None,
            };
            for (k, quad) in array(extra, "quads").iter().enumerate() {
                let idx: Vec<usize> = quad
                    .as_array()
                    .context("a brick quad names no vertices")?
                    .iter()
                    .map(|v| v.as_u64().context("quad vertex index").map(|v| v as usize))
                    .collect::<Result<Vec<usize>, _>>()?;
                ensure!(idx.len() == 4, "a brick quad does not name four vertices");
                let mut verts = [Vertex {
                    position: [0.0; 3],
                    normal: [0.0; 3],
                    uv: [0.0; 2],
                }; 4];
                for (slot, &vi) in idx.iter().enumerate() {
                    ensure!(
                        vi < positions.len(),
                        "a brick quad names a vertex the primitive does not have"
                    );
                    verts[slot].position = mirror(
                        positions[vi]
                            .as_slice()
                            .try_into()
                            .context("brick position row")?,
                    );
                    if let Some(normals) = &normals {
                        ensure!(
                            vi < normals.len(),
                            "brick normal shorter than its positions"
                        );
                        verts[slot].normal = mirror(
                            normals[vi]
                                .as_slice()
                                .try_into()
                                .context("brick normal row")?,
                        );
                    } else {
                        verts[slot].normal = [0.0, 1.0, 0.0];
                    }
                    if let Some(uvs) = &uvs {
                        ensure!(vi < uvs.len(), "brick UVs shorter than its positions");
                        verts[slot].uv = uvs[vi][..2].try_into().context("brick UV row")?;
                    }
                }
                quads.push(Quad {
                    face,
                    surface,
                    vertices: verts,
                    colors: match &colors {
                        Some(list) => match list.get(k) {
                            Some(Value::Null) | None => None,
                            Some(c) => {
                                Some(serde_json::from_value(c.clone()).context("quad colour")?)
                            }
                        },
                        None => None,
                    },
                });
            }
        }
    }
    ensure!(!quads.is_empty(), "the glTF holds no brick quads");

    // The grid facts; absent extras mean a plain mesh with no brick body.
    let meta = &json["asset"]["extras"]["bri_brick_meta"];
    ensure!(
        meta.is_object(),
        "the glTF carries no bri_brick_meta asset extra"
    );
    let coverage: Option<[Coverage; 6]> = match meta.get("coverage") {
        Some(Value::Null) | None => None,
        Some(c) => Some(serde_json::from_value(c.clone()).context("brick coverage")?),
    };
    #[allow(unreachable_patterns)] // `coverage` is absent or an array; null means none
    let brick = Brick {
        schema_version: BRICK_SCHEMA,
        id: id.to_owned(),
        footprint_studs: serde_json::from_value(meta["footprint_studs"].clone())
            .context("brick footprint")?,
        height_plates: meta["height_plates"].as_u64().context("brick height")? as u32,
        attachment_rows: serde_json::from_value(meta["attachment_rows"].clone())
            .context("brick attachment rows")?,
        collision_boxes: serde_json::from_value::<Vec<CollisionBox>>(
            meta.get("collision_boxes")
                .cloned()
                .unwrap_or(Value::Array(vec![])),
        )
        .context("brick collision boxes")?,
        needs_external_collision: meta["needs_external_collision"].as_bool().unwrap_or(false),
        coverage,
        quads,
    };
    brick.validate()?;
    Ok(brick)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::brick::{Face, Surface};

    /// A brick with quads on several faces and surfaces, some painted:
    /// export lowers it back to itself.
    #[test]
    fn a_brick_round_trips_through_its_glb() {
        let quad = |face: Face, surface: Surface, x: f32| Quad {
            face,
            surface,
            vertices: [
                Vertex {
                    position: [x, 0.0, -1.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                },
                Vertex {
                    position: [x + 0.5, 0.0, -1.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [1.0, 0.0],
                },
                Vertex {
                    position: [x + 0.5, 0.0, -0.5],
                    normal: [0.0, 1.0, 0.0],
                    uv: [1.0, 1.0],
                },
                Vertex {
                    position: [x, 0.0, -0.5],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 1.0],
                },
            ],
            colors: None,
        };
        let mut painted = quad(Face::North, Surface::Side, -0.5);
        painted.colors = Some([[1.0, 0.0, 0.0, 1.0]; 4]);
        let brick = Brick {
            schema_version: BRICK_SCHEMA,
            id: "test:brick/one".into(),
            footprint_studs: [2, 2],
            height_plates: 3,
            attachment_rows: vec![
                "uu".into(),
                "ux".into(),
                "xu".into(),
                "xx".into(),
                "dx".into(),
                "dd".into(),
            ],
            collision_boxes: vec![CollisionBox {
                center: [0.1, -0.2, 0.3],
                size: [0.5, 0.75, 0.5],
            }],
            needs_external_collision: true,
            coverage: Some([
                Coverage {
                    hides_adjacent: true,
                    required_area: 0.4,
                },
                Coverage {
                    hides_adjacent: false,
                    required_area: 0.1,
                },
                Coverage {
                    hides_adjacent: false,
                    required_area: 0.2,
                },
                Coverage {
                    hides_adjacent: false,
                    required_area: 0.2,
                },
                Coverage {
                    hides_adjacent: false,
                    required_area: 0.3,
                },
                Coverage {
                    hides_adjacent: true,
                    required_area: 1.0,
                },
            ]),
            quads: vec![
                quad(Face::Top, Surface::Top, -0.5),
                painted,
                quad(Face::Omni, Surface::Print, 0.25),
            ],
        };
        brick.validate().expect("fixture validates");

        let glb = export_brick(&brick).expect("export");
        let back = brick_from_glb(&glb, "test:brick/one").expect("lower back");
        assert_eq!(back, brick);
    }

    /// A plain shape mesh is not a brick: its primitives carry no
    /// `bri_brick` extra and the asset has no `bri_brick_meta`.
    #[test]
    fn a_plain_mesh_glb_is_not_a_brick() {
        let mut g = Gltf::new();
        let prim = json!({
            "attributes": {
                "POSITION": g.floats(&[-0.5, 0.0, -0.5, 0.5, 0.0, -0.5, 0.0, 0.9, -0.5], 3),
                "NORMAL": g.floats(&[0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0], 3)
            },
            "indices": g.indices(&[0, 1, 2])
        });
        let mesh = g.mesh("tri", vec![prim]);
        let mut node = json!({ "name": "tri" });
        node["mesh"] = json!(mesh);
        g.node(node, None);
        assert!(brick_from_glb(&g.glb(), "x").is_err());
    }
}
