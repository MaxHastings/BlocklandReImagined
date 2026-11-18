//! A native interior — a map's static architecture: its render surfaces with
//! their lightmap sheets and its authored collision — as one self-contained
//! `.glb`: diffuse textures and lightmaps embedded beside the geometry, so a
//! map folder carries no loose image files for it. The `BRI_interior`
//! extension keeps what glTF has no word for (per-surface flags and lightmap
//! slots, per-detail ambient colours, collision as position-only primitives),
//! and [`super::interior_glb`] reads the result back to the native type, so a
//! load path changes only in which file it opens.
use super::write::Gltf;
use anyhow::{Context, Result, ensure};
use bri_content::interior::{Detail, Interior};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

/// The extension that carries an interior's native data beside its geometry.
pub const EXTENSION: &str = "BRI_interior";

fn push(g: &mut Gltf, key: &str, value: Value) -> usize {
    let a = g.json[key].as_array_mut().expect("array");
    a.push(value);
    a.len() - 1
}

/// A name the game keeps as-is: no control characters (it strips them), at
/// most 64 of them (it truncates there).
fn clean_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).take(64).collect()
}

/// Detail `d` of the interior, its own details first and then subobjects.
fn detail_of(interior: &Interior, d: usize) -> &Detail {
    if d < interior.details.len() {
        &interior.details[d]
    } else {
        &interior.subobjects[d - interior.details.len()]
    }
}

/// One embedded image, deduplicated by key across the whole interior. `file`
/// marks a real texture file (its name is reported as riding inside); lightmap
/// sheets use synthetic keys and are not.
fn embed(
    g: &mut Gltf,
    cache: &mut BTreeMap<String, (usize, usize)>, // key -> (image index, texture index)
    embedded_files: &mut BTreeSet<String>,
    key: &str,
    bytes: &[u8],
    file: bool,
) -> Result<(usize, usize)> {
    if let Some(t) = cache.get(key) {
        return Ok(*t);
    }
    let (data, mime): (Cow<'_, [u8]>, &'static str) = encoded(bytes)?;
    let view = g.view(&data);
    let image = push(g, "images", json!({ "bufferView": view, "mimeType": mime }));
    // Diffuse textures tile the way Torque's did; lightmap sheets are sampled
    // inside one sheet. (The game decodes its own samplers; this is what
    // Blender sees.)
    let wrap = if file { 10497 } else { 33071 };
    let sampler = push(g, "samplers", json!({ "wrapS": wrap, "wrapT": wrap }));
    let texture = push(g, "textures", json!({ "source": image, "sampler": sampler }));
    if file {
        embedded_files.insert(key.to_owned());
    }
    cache.insert(key.to_owned(), (image, texture));
    Ok((image, texture))
}

/// PNG and JPEG ride as-is; anything else is re-encoded to PNG.
fn encoded(bytes: &[u8]) -> Result<(Cow<'_, [u8]>, &'static str)> {
    if bytes.starts_with(b"\x89PNG") {
        Ok((Cow::Borrowed(bytes), "image/png"))
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        Ok((Cow::Borrowed(bytes), "image/jpeg"))
    } else {
        let image =
            image::load_from_memory(bytes).context("an interior texture is not an image the exporter reads")?;
        let mut png = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
        Ok((Cow::Owned(png), "image/png"))
    }
}

/// The interior as one `.glb`. `textures[d][j]` is material j of detail d's
/// image — its file path and bytes, or `None` where nothing resolved (the
/// game would have drawn it untextured); details 0..details.len() are the
/// interior's own, the rest its subobjects. What was dropped comes back in
/// `notes`, and the second return value is the set of texture files whose
/// bytes ride inside the `.glb`.
pub fn export(
    interior: &Interior,
    textures: &[Vec<Option<(String, Vec<u8>)>>],
    notes: &mut Vec<String>,
) -> Result<(Vec<u8>, BTreeSet<String>)> {
    let total = interior.details.len() + interior.subobjects.len();
    ensure!(
        textures.len() == total,
        "expected texture resolutions for {} detail(s), got {}",
        total,
        textures.len()
    );

    // --- Materials and their embedded images -----------------------------
    let mut g = Gltf::new();
    let mut cache: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut embedded_files: BTreeSet<String> = BTreeSet::new();
    // glTF material index of (detail d, material j), and the image each one
    // embeds (the extension records it; the reader takes its bytes from it).
    let mut mat_gltf: Vec<Vec<usize>> = Vec::with_capacity(total);
    let mut mat_image: Vec<Vec<Option<usize>>> = Vec::with_capacity(total);
    for d in 0..total {
        let det = detail_of(interior, d);
        ensure!(
            textures[d].len() == det.materials.len(),
            "detail {d} has {} material(s) but {} texture resolution(s)",
            det.materials.len(),
            textures[d].len()
        );
        // The materials a non-empty surface actually draws; the game binds
        // its list positionally, so every one is exported regardless.
        let used: BTreeSet<usize> = det
            .surfaces
            .iter()
            .filter(|s| !s.triangles.is_empty())
            .map(|s| s.material)
            .collect();
        let mut gltf_row = Vec::with_capacity(det.materials.len());
        let mut image_row = Vec::with_capacity(det.materials.len());
        for (j, name) in det.materials.iter().enumerate() {
            let mut mat = json!({
                "name": clean_name(name),
                // Torque surfaces are matte; the factor is 1 so the texture's
                // own colour shows.
                "pbrMetallicRoughness": {
                    "baseColorFactor": [1.0, 1.0, 1.0, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 1.0,
                },
            });
            match textures[d][j].as_ref() {
                Some((file, bytes)) => {
                    let (image, texture) = embed(&mut g, &mut cache, &mut embedded_files, file, bytes, true)?;
                    mat["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": texture });
                    image_row.push(Some(image));
                }
                None if used.contains(&j) => {
                    notes.push(format!(
                        "material {} (detail {d}) has no resolved texture; it draws untextured in the .glb",
                        clean_name(name)
                    ));
                    image_row.push(None);
                }
                None => image_row.push(None),
            }
            gltf_row.push(push(&mut g, "materials", mat));
        }
        mat_gltf.push(gltf_row);
        mat_image.push(image_row);
    }

    // --- Lightmap sheets ---------------------------------------------------
    let mut lm_image: Vec<Vec<Option<usize>>> = Vec::with_capacity(total);
    let mut aux_image: Vec<Vec<Option<usize>>> = Vec::with_capacity(total);
    for d in 0..total {
        let det = detail_of(interior, d);
        let mut row_i = Vec::with_capacity(det.lightmaps.len());
        let mut row_a = Vec::with_capacity(det.lightmaps.len());
        for (s, lm) in det.lightmaps.iter().enumerate() {
            let key = format!("{}/detail{}-sheet{}", interior.id, d, s);
            let (image, _) = embed(&mut g, &mut cache, &mut embedded_files, &key, &lm.png, false)?;
            row_i.push(Some(image));
            if let Some(aux) = &lm.auxiliary_png {
                let key = format!("{key}-aux");
                let (image, _) = embed(&mut g, &mut cache, &mut embedded_files, &key, aux, false)?;
                row_a.push(Some(image));
            } else {
                row_a.push(None);
            }
        }
        lm_image.push(row_i);
        aux_image.push(row_a);
    }

    // --- The render mesh: one primitive per non-empty surface --------------
    let mut prims = Vec::new();
    // Primitive index of each (detail d, surface s), in the render mesh.
    let mut prim_of: Vec<Vec<Option<usize>>> = Vec::with_capacity(total);
    for d in 0..total {
        let det = detail_of(interior, d);
        let mut row = Vec::with_capacity(det.surfaces.len());
        for (_s, sf) in det.surfaces.iter().enumerate() {
            if sf.triangles.is_empty() {
                row.push(None); // The reader keeps it as an empty surface.
                continue;
            }
            let pos: Vec<f32> = sf.vertices.iter().flat_map(|v| v.position).collect();
            let nor: Vec<f32> = sf.vertices.iter().flat_map(|v| v.normal).collect();
            let uv0: Vec<f32> = sf.vertices.iter().flat_map(|v| v.uv).collect();
            let uv1: Vec<f32> = sf.vertices.iter().flat_map(|v| v.lightmap_uv).collect();
            let idx: Vec<u32> = sf.triangles.iter().flatten().copied().collect();
            let p = json!({
                "attributes": {
                    "POSITION": g.floats(&pos, 3),
                    "NORMAL": g.floats(&nor, 3),
                    // The lightmap UV is a second set of coordinates: glTF's
                    // open attribute map admits it, and the reader takes it.
                    "TEXCOORD_0": g.floats(&uv0, 2),
                    "TEXCOORD_1": g.floats(&uv1, 2),
                },
                "indices": g.indices(&idx),
                "material": mat_gltf[d][sf.material],
            });
            row.push(Some(prims.len()));
            prims.push(p);
        }
        prim_of.push(row);
    }

    // --- The collision mesh: position-only triangle soup per detail --------
    let mut col_prims = Vec::new();
    let mut col_of: Vec<Option<usize>> = Vec::with_capacity(total);
    for d in 0..total {
        let det = detail_of(interior, d);
        if det.collision_triangles.is_empty() {
            col_of.push(None);
            continue;
        }
        let pos: Vec<f32> = det
            .collision_triangles
            .iter()
            .flatten()
            .flat_map(|v| *v)
            .collect();
        // A primitive without indices would draw as points, so the soup gets
        // its own.
        let idx: Vec<u32> = (0..det.collision_triangles.len())
            .flat_map(|t| [t as u32 * 3, t as u32 * 3 + 1, t as u32 * 3 + 2])
            .collect();
        col_prims.push(json!({
            "attributes": { "POSITION": g.floats(&pos, 3) },
            "indices": g.indices(&idx),
        }));
        col_of.push(Some(col_prims.len() - 1));
    }

    let render_mesh = if prims.is_empty() {
        Value::Null
    } else {
        json!(g.mesh("render", std::mem::take(&mut prims)))
    };
    let collision_mesh = if col_prims.is_empty() {
        Value::Null
    } else {
        json!(g.mesh("collision", std::mem::take(&mut col_prims)))
    };
    // A node with no mesh would not read back, so an empty interior simply has
    // no nodes (the slate plane is one).
    if !render_mesh.is_null() {
        g.node(json!({ "name": "render", "mesh": render_mesh.clone() }), None);
    }
    if !collision_mesh.is_null() {
        g.node(json!({ "name": "collision", "mesh": collision_mesh.clone() }), None);
    }

    // --- The extension ------------------------------------------------------
    let detail_json = |d: usize| -> Value {
        let det = detail_of(interior, d);
        json!({
            "materials": det.materials.iter().enumerate().map(|(j, name)| {
                json!({ "name": name, "image": mat_image[d][j] })
            }).collect::<Vec<_>>(),
            "surfaces": det.surfaces.iter().enumerate().map(|(s, sf)| {
                let mut o = json!({
                    "material": sf.material,
                    "flags": sf.flags,
                    "lightmap": sf.lightmap,
                    "alarm_lightmap": sf.alarm_lightmap,
                });
                if let Some(p) = prim_of[d][s] {
                    o["primitive"] = json!(p);
                } else {
                    o["primitive"] = Value::Null;
                }
                o
            }).collect::<Vec<_>>(),
            "lightmaps": det.lightmaps.iter().enumerate().map(|(s, lm)| {
                let mut o = json!({ "image": lm_image[d][s], "keep": lm.keep });
                if let Some(a) = aux_image[d][s] {
                    o["auxiliary_image"] = json!(a);
                } else {
                    o["auxiliary_image"] = Value::Null;
                }
                o
            }).collect::<Vec<_>>(),
            "collision_primitive": col_of[d],
            "minimum_pixels": det.minimum_pixels,
            "ambient": det.ambient.to_vec(),
            "alarm_ambient": det.alarm_ambient.to_vec(),
            "has_alarm": det.has_alarm,
        })
    };

    let hulls: usize = interior
        .details
        .iter()
        .chain(&interior.subobjects)
        .map(|d| d.convex_hulls.len())
        .sum();
    if hulls > 0 {
        notes.push(format!(
            "{hulls} authored convex hull(s) have no place in the .glb and were left out (the game collides from its triangles)"
        ));
    }
    if interior.vehicle_collision.is_some() {
        notes.push(
            "vehicle collision has no place in the .glb and was left out".to_owned(),
        );
    }

    g.json["extensionsUsed"] = json!([EXTENSION]);
    g.json["extensions"][EXTENSION] = json!({
        "schema": 1,
        "id": interior.id,
        "render_mesh": render_mesh,
        "collision_mesh": collision_mesh,
        "details": (0..interior.details.len()).map(&detail_json).collect::<Vec<_>>(),
        "subobjects": (0..interior.subobjects.len())
            .map(|d| detail_json(d + interior.details.len()))
            .collect::<Vec<_>>(),
    });

    let glb = g.glb();
    Ok((glb, embedded_files))
}
