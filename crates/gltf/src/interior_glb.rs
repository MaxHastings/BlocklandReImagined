//! An interior `.glb` (see [`super::interior_export`]) back to the native
//! [`Interior`] and its embedded diffuse textures, so a load path that used
//! to read an interior's JSON can open the `.glb` instead and hand down the
//! same type. What glTF has no word for comes from the `BRI_interior`
//! extension; what it does (positions, normals, UVs, lightmap UVs, triangle
//! indices) comes from its primitives. Authored convex hulls and vehicle
//! collision were dropped from the format: nothing at runtime reads them.
use super::interior_export::EXTENSION;
use super::read::Document;
use anyhow::{Context, Result, ensure};
use bri_content::interior::{Detail, Interior, Lightmap, Surface, Vertex};
use serde_json::Value;

/// The interior a `.glb` carries, with its embedded diffuse textures.
pub struct InteriorGlb {
    pub interior: Interior,
    /// `textures[d][j]`: material j of detail d's image bytes; `None` where
    /// the `.glb` embeds none (the game would have drawn it untextured).
    pub textures: Vec<Vec<Option<Vec<u8>>>>,
}

impl InteriorGlb {
    /// Read an interior out of a parsed document. Fails when the document is
    /// not an interior at all, so callers can tell their files apart by trying.
    pub fn from_document(doc: &Document) -> Result<InteriorGlb> {
        let ext = doc
            .json["extensions"]
            .get(EXTENSION)
            .context("the glTF carries no BRI_interior extension")?;
        ensure!(
            ext["schema"].as_u64() == Some(1),
            "BRI_interior schema {} is not 1",
            ext["schema"]
        );
        let id = ext["id"]
            .as_str()
            .with_context(|| "BRI_interior has no id")?
            .to_owned();

        let render_prims = prims(doc, ext.get("render_mesh"))?;
        let collision_prims = prims(doc, ext.get("collision_mesh"))?;

        let mut details: Vec<Detail> = Vec::new();
        let mut textures: Vec<Vec<Option<Vec<u8>>>> = Vec::new();
        for (d, de) in array(ext, "details").iter().enumerate() {
            details.push(read_detail(doc, &id, d, d, de, &render_prims, &collision_prims)?);
            textures.push(detail_textures(doc, &id, d, de)?);
        }
        let mut subobjects: Vec<Detail> = Vec::new();
        if let Some(subs) = ext.get("subobjects").and_then(Value::as_array) {
            for (s, de) in subs.iter().enumerate() {
                // Detail numbering continues past the interior's own.
                let d = details.len() + s;
                subobjects.push(read_detail(doc, &id, d, s, de, &render_prims, &collision_prims)?);
                textures.push(detail_textures(doc, &id, d, de)?);
            }
        }

        let interior = Interior {
            schema_version: 1,
            id,
            details,
            subobjects,
            vehicle_collision: None, // dropped from the format
        };
        interior
            .validate()
            .with_context(|| "the .glb's interior data does not validate")?;
        Ok(InteriorGlb { interior, textures })
    }
}

fn array<'a>(v: &'a Value, key: &str) -> Vec<&'a Value> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

/// The primitives of the named mesh, or none where the interior has no such
/// geometry at all.
fn prims(doc: &Document, mesh: Option<&Value>) -> Result<Option<Vec<Value>>> {
    let Some(i) = mesh.and_then(Value::as_u64) else {
        return Ok(None);
    };
    let m = doc
        .json["meshes"]
        .as_array()
        .context("the interior names a mesh but the glTF has none")?
        .get(i as usize)
        .with_context(|| format!("BRI_interior names missing mesh {i}"))?;
    Ok(Some(
        m["primitives"]
            .as_array()
            .context("the interior's render mesh has no primitives")?
            .clone(),
    ))
}

/// The image bytes a detail's materials embed, by material index.
fn detail_textures(doc: &Document, id: &str, d: usize, de: &Value) -> Result<Vec<Option<Vec<u8>>>> {
    let mut out = Vec::new();
    for (_j, m) in array(de, "materials").iter().enumerate() {
        match m.get("image").and_then(Value::as_u64) {
            None => out.push(None),
            Some(i) => out.push(Some(
                doc.image(i as usize, None).with_context(|| format!(
                    "{id} material {} (detail {d}) names missing image {i}",
                    m["name"]
                ))?,
            )),
        }
    }
    Ok(out)
}

fn read_detail(
    doc: &Document,
    id: &str,
    d: usize, // numbering across the whole interior (details then subobjects)
    s: usize, // position within its own list, for messages
    de: &Value,
    render_prims: &Option<Vec<Value>>,
    collision_prims: &Option<Vec<Value>>,
) -> Result<Detail> {
    let names: Vec<String> = array(de, "materials")
        .iter()
        .map(|m| m["name"].as_str().unwrap_or_default().to_owned())
        .collect();

    // Lightmaps first: the surfaces reference them by slot.
    let mut lightmaps: Vec<Lightmap> = Vec::new();
    for (slot, lm) in array(de, "lightmaps").iter().enumerate() {
        let image = lm["image"].as_u64().with_context(|| format!(
            "{id} detail {s} lightmap {slot} names no image"
        ))?;
        let png = doc.image(image as usize, None).with_context(|| format!(
            "{id} detail {s} lightmap {slot} cannot read its image"
        ))?;
        let auxiliary_png = match lm.get("auxiliary_image").and_then(Value::as_u64) {
            Some(i) => Some(
                doc.image(i as usize, None).with_context(|| format!(
                    "{id} detail {s} lightmap {slot} cannot read its auxiliary image"
                ))?,
            ),
            None => None,
        };
        lightmaps.push(Lightmap {
            png,
            auxiliary_png,
            keep: lm["keep"].as_bool().unwrap_or(false),
        });
    }

    let mut surfaces = Vec::new();
    for (slot, se) in array(de, "surfaces").iter().enumerate() {
        let material = se["material"]
            .as_u64()
            .with_context(|| format!("{id} detail {s} surface {slot} names no material"))? as usize;
        ensure!(
            material < names.len(),
            "{id} detail {s} surface {slot} names missing material {material}"
        );
        let flags = se["flags"].as_u64().unwrap_or(0) as u8;
        let lightmap = opt_slot(se.get("lightmap"), "a lightmap", id, s, slot, lightmaps.len())?;
        let alarm_lightmap =
            opt_slot(se.get("alarm_lightmap"), "an alarm lightmap", id, s, slot, lightmaps.len())?;
        match se.get("primitive").and_then(Value::as_u64) {
            None => surfaces.push(Surface {
                source_index: 0,
                material,
                flags,
                vertices: vec![],
                triangles: vec![],
                lightmap,
                alarm_lightmap,
            }),
            Some(p) => {
                let prim = render_prims
                    .as_ref()
                    .and_then(|ps| ps.get(p as usize))
                    .with_context(|| format!("{id} detail {s} surface {slot} names missing primitive {p}"))?;
                surfaces.push(read_surface(
                    doc, id, d, slot, material, flags, lightmap, alarm_lightmap, prim,
                )?);
            }
        }
    }

    let collision_triangles = match de.get("collision_primitive").and_then(Value::as_u64) {
        None => vec![],
        Some(i) => {
            let prim = collision_prims
                .as_ref()
                .and_then(|ps| ps.get(i as usize))
                .with_context(|| format!("{id} detail {s} names missing collision primitive {i}"))?;
            let what = format!("{id} detail {s} collision");
            let pos = rows_of(doc, &what, prim, "POSITION", 3)?;
            ensure!(
                pos.len() % 3 == 0,
                "{what} has {} position(s), not a multiple of 3",
                pos.len()
            );
            pos.chunks(3)
                .map(|t| {
                    let a = [t[0][0], t[0][1], t[0][2]];
                    let b = [t[1][0], t[1][1], t[1][2]];
                    let c = [t[2][0], t[2][1], t[2][2]];
                    [a, b, c]
                })
                .collect()
        }
    };

    Ok(Detail {
        minimum_pixels: de["minimum_pixels"].as_u64().unwrap_or(0) as u32,
        materials: names,
        surfaces,
        lightmaps,
        collision_triangles,
        convex_hulls: vec![], // dropped from the format (unused at runtime)
        ambient: bytes4(de.get("ambient"), id, s, "ambient")?,
        alarm_ambient: bytes4(de.get("alarm_ambient"), id, s, "alarm ambient")?,
        has_alarm: de["has_alarm"].as_bool().unwrap_or(false),
    })
}

fn opt_slot(
    v: Option<&Value>,
    what: &str,
    id: &str,
    d: usize,
    s: usize,
    count: usize,
) -> Result<Option<usize>> {
    let Some(i) = v.and_then(Value::as_u64) else {
        return Ok(None);
    };
    // (`>` not `<`: a condition ending in `< ident` confuses the macro's
    // expression parser into reading on through the message.)
    ensure!(
        count > i as usize,
        "{id} detail {d} surface {s} uses {what} slot {i}, past its {count} lightmap(s)"
    );
    Ok(Some(i as usize))
}

fn read_surface(
    doc: &Document,
    id: &str,
    d: usize,
    s: usize,
    material: usize,
    flags: u8,
    lightmap: Option<usize>,
    alarm_lightmap: Option<usize>,
    prim: &Value,
) -> Result<Surface> {
    let what = format!("{id} detail {d} surface {s}");
    let positions = rows_of(doc, &what, prim, "POSITION", 3)?;
    let normals = rows_of(doc, &what, prim, "NORMAL", 3)?;
    let uv = rows_of(doc, &what, prim, "TEXCOORD_0", 2)?;
    let lm_uv = rows_of(doc, &what, prim, "TEXCOORD_1", 2)?;
    ensure!(
        positions.len() == normals.len()
            && positions.len() == uv.len()
            && positions.len() == lm_uv.len(),
        "{what} has mismatched attribute counts"
    );
    let idx = prim["indices"]
        .as_u64()
        .with_context(|| format!("{what} has no indices"))?;
    let flat: Vec<u32> = doc
        .uints(idx as usize)
        .with_context(|| format!("{what} cannot read its indices"))?
        .into_iter()
        .flatten()
        .collect();
    ensure!(
        flat.len() % 3 == 0,
        "{what} has {} index(es), not a multiple of 3",
        flat.len()
    );
    let triangles: Vec<[u32; 3]> = flat.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
    ensure!(
        triangles
            .iter()
            .all(|t| t.iter().all(|&i| (i as usize) < positions.len())),
        "{what} has a triangle past its vertices"
    );
    let vertices: Vec<Vertex> = positions
        .iter()
        .zip(normals.iter())
        .zip(uv.iter())
        .zip(lm_uv.iter())
        .map(|(((p, n), u), l)| Vertex {
            position: [p[0], p[1], p[2]],
            normal: [n[0], n[1], n[2]],
            uv: [u[0], u[1]],
            lightmap_uv: [l[0], l[1]],
        })
        .collect();
    Ok(Surface {
        source_index: 0,
        material,
        flags,
        vertices,
        triangles,
        lightmap,
        alarm_lightmap,
    })
}

/// Accessor `name` of a primitive as rows of floats, checked to be `width`-wide.
fn rows_of(doc: &Document, what: &str, prim: &Value, name: &str, width: usize) -> Result<Vec<Vec<f32>>> {
    let idx = prim["attributes"][name]
        .as_u64()
        .with_context(|| format!("{what} has no {name}"))?;
    let rows = doc
        .floats(idx as usize)
        .with_context(|| format!("{what} cannot read its {name}"))?;
    ensure!(
        rows.iter().all(|r| r.len() == width),
        "{what} {name} rows are not {width}-wide"
    );
    Ok(rows)
}

fn bytes4(v: Option<&Value>, id: &str, d: usize, what: &str) -> Result<[u8; 4]> {
    let a = v.and_then(Value::as_array).with_context(|| format!("{id} detail {d} has no {what}"))?;
    ensure!(a.len() == 4, "{id} detail {d} {what} is not 4 values");
    let mut out = [0u8; 4];
    for i in 0..4 {
        out[i] = a[i]
            .as_u64()
            .with_context(|| format!("{id} detail {d} {what}[{i}] is not a number"))? as u8;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interior_export::export;

    fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba(rgba));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    fn vertex(p: [f32; 3], n: [f32; 3], uv: [f32; 2], luv: [f32; 2]) -> Vertex {
        Vertex { position: p, normal: n, uv, lightmap_uv: luv }
    }

    fn interior() -> Interior {
        Interior {
            schema_version: 1,
            id: "test".to_owned(),
            details: vec![Detail {
                minimum_pixels: 4,
                materials: vec!["WOOD".into(), "STONE".into()],
                surfaces: vec![
                    Surface {
                        source_index: 0,
                        material: 0,
                        flags: 1,
                        vertices: vec![
                            vertex([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0], [0.0, 0.0]),
                            vertex([1.5, 0.25, -3.75], [0.0, 1.0, 0.0], [1.0, 0.0], [0.5, 0.0]),
                            vertex([2.0; 3], [0.0, 1.0, 0.0], [2.0, 0.0], [1.0, 0.0]),
                            vertex([0.5, -1.0, 4.0], [0.0, 1.0, 0.0], [0.5, 1.0], [0.25, 0.75]),
                        ],
                        triangles: vec![[0, 1, 2], [2, 1, 3]],
                        lightmap: Some(0),
                        alarm_lightmap: None,
                    },
                    Surface {
                        source_index: 0,
                        material: 1,
                        flags: 0,
                        vertices: vec![
                            vertex([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0], [0.0, 0.0]),
                            vertex([1.0; 3], [1.0, 0.0, 0.0], [1.0, 0.0], [0.5, 0.0]),
                            vertex([2.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0], [1.0, 0.0]),
                        ],
                        triangles: vec![[0, 1, 2]],
                        lightmap: None,
                        alarm_lightmap: None,
                    },
                    // Empty: no primitive in the .glb, kept as an empty surface.
                    Surface {
                        source_index: 0,
                        material: 0,
                        flags: 0,
                        vertices: vec![],
                        triangles: vec![],
                        lightmap: Some(0),
                        alarm_lightmap: None,
                    },
                ],
                lightmaps: vec![Lightmap {
                    png: png(4, 4, [9, 8, 7, 6]),
                    auxiliary_png: Some(png(2, 1, [5, 4, 3, 2])),
                    keep: true,
                }],
                collision_triangles: vec![[[0.0; 3], [1.0; 3], [2.0, 0.0, 0.0]]],
                convex_hulls: vec![],
                ambient: [1, 2, 3, 4],
                alarm_ambient: [5, 6, 7, 8],
                has_alarm: true,
            }],
            subobjects: vec![],
            vehicle_collision: None,
        }
    }

    #[test]
    fn round_trip() {
        let i = interior();
        let mut notes = Vec::new();
        let texs: Vec<Vec<Option<(String, Vec<u8>)>>> =
            vec![vec![Some(("wood.png".into(), png(2, 2, [1, 2, 3, 4]))), None]];
        let (glb, embedded) = export(&i, &texs, &mut notes).unwrap();
        assert!(embedded.contains("wood.png"));
        // STONE is drawn by a surface but resolves to no texture.
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("STONE"), "{:?}", notes);

        let doc = Document::from_bytes(&glb, None).unwrap();
        let back = InteriorGlb::from_document(&doc).unwrap();
        assert_eq!(back.interior.id, "test");
        let d = &back.interior.details[0];
        assert_eq!(d.materials, vec!["WOOD", "STONE"]);
        assert_eq!(d.surfaces.len(), 3);
        // Surface 0: geometry, UVs and lightmap slot survive exactly.
        assert_eq!(d.surfaces[0].vertices.len(), 4);
        assert_eq!(d.surfaces[0].vertices[1].position, [1.5, 0.25, -3.75]);
        assert_eq!(d.surfaces[0].vertices[3].lightmap_uv, [0.25, 0.75]);
        assert_eq!(d.surfaces[0].triangles, vec![[0, 1, 2], [2, 1, 3]]);
        assert_eq!(d.surfaces[0].material, 0);
        assert_eq!(d.surfaces[0].flags, 1);
        assert_eq!(d.surfaces[0].lightmap, Some(0));
        // Surface 2 was empty and stays empty.
        assert!(d.surfaces[2].vertices.is_empty() && d.surfaces[2].triangles.is_empty());
        assert_eq!(d.surfaces[2].material, 0);
        // Lightmap sheets come back as their own bytes.
        assert_eq!(d.lightmaps.len(), 1);
        assert_eq!(d.lightmaps[0].png, png(4, 4, [9, 8, 7, 6]));
        assert_eq!(
            d.lightmaps[0].auxiliary_png.as_deref(),
            Some(png(2, 1, [5, 4, 3, 2]).as_slice())
        );
        assert!(d.lightmaps[0].keep);
        // Collision soup.
        assert_eq!(d.collision_triangles, vec![[[0.0; 3], [1.0; 3], [2.0, 0.0, 0.0]]]);
        // Detail scalars.
        assert_eq!(d.minimum_pixels, 4);
        assert_eq!(d.ambient, [1, 2, 3, 4]);
        assert_eq!(d.alarm_ambient, [5, 6, 7, 8]);
        assert!(d.has_alarm);
        // Embedded diffuse textures by (detail, material).
        assert_eq!(
            back.textures[0][0].as_deref(),
            Some(png(2, 2, [1, 2, 3, 4]).as_slice())
        );
        assert_eq!(back.textures[0][1], None);
    }

    #[test]
    fn empty_interior_has_no_meshes() {
        let mut i = interior();
        i.id = "slate".into();
        for d in &mut i.details {
            d.surfaces.clear();
            d.lightmaps.clear();
            d.collision_triangles.clear();
        }
        let mut notes = Vec::new();
        let texs: Vec<Vec<Option<(String, Vec<u8>)>>> = vec![vec![None, None]];
        let (glb, _) = export(&i, &texs, &mut notes).unwrap();
        assert!(notes.is_empty());
        let doc = Document::from_bytes(&glb, None).unwrap();
        assert!(doc.json["meshes"].as_array().map(Vec::is_empty).unwrap_or(true));
        let back = InteriorGlb::from_document(&doc).unwrap();
        assert!(back.interior.details[0].surfaces.is_empty());
    }

    #[test]
    fn a_plain_model_is_not_an_interior() {
        let g = crate::Gltf::new();
        let doc = Document::from_bytes(&g.glb(), None).unwrap();
        assert!(InteriorGlb::from_document(&doc).is_err());
    }
}
