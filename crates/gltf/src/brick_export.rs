//! A native brick (`bri_content::brick::Brick`, what a converted BLB is) as a
//! glTF binary: its quads drawn as triangles, each quad's face and surface
//! kept in `bri_brick` extras so the game can lower it back to the very same
//! brick, and the grid size, attachment rows, coverage and collision boxes in
//! the asset's `bri_brick_meta`. Authored colours live in the glTF channels
//! themselves — a shared material `baseColorFactor` for uniform quads, or a
//! per-vertex `COLOR_0` where a quad varies — so editing a material in Blender
//! is exactly what the game will draw. What glTF has no word for rides in the
//! extras: signed paint offsets (a surface recoloured relative to the player's
//! paint) as a per-quad `bri_brick.paint_offsets` marker, and on pre-migration
//! files only, the legacy `bri_brick.colors` dump. Runtime-coloured quads
//! (plain player paint and signed offsets) ride a placeholder `blank`
//! material — one per surface — so an editor sees every surface as the game
//! draws it default-painted; where [`SurfaceTextures`] names the installed
//! shared stock picture for that surface, the placeholder references it by
//! relative URI (the pre-composited-over-white copy beside the brick-materials
//! bundle) instead of embedding a copy in every file. The marker tells the
//! lowering to ignore the factor and texture and keep the colour dynamic.

use std::path::Path;

use super::read::{Document, array};
use super::shape_export::point as mirror;
use super::write::{Gltf, MaterialImage};
use anyhow::{Context, Result, ensure};
use bri_content::brick::{
    BRICK_SCHEMA, Brick, CollisionBox, Coverage, Face, Quad, Surface, Vertex,
};
use serde_json::{Value, json};

/// What a quad's authored colours become in glTF, or nothing where the colour
/// is decided by the player at runtime and must not be baked.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Baked {
    /// One literal colour across all four vertices: one shared material with
    /// that `baseColorFactor` (BLEND where it is transparent).
    Uniform([f32; 4]),
    /// Four distinct literal colours on the quad: a per-vertex `COLOR_0`
    /// attribute (window glass ships one). Kept so an authored gradient can
    /// never silently vanish.
    PerVertex,
}
/// The glTF half of the renderer's colour policy (the render crate's
/// `resolve_brick_vertex_color`): absent colours are the player's paint and a
/// negative alpha is a signed offset onto it, both dynamic; every other
/// literal colour bakes as-is, clamped into glTF's [0, 1] factor range. An
/// alpha above one would be rejected by the renderer anyway and fits no glTF
/// factor: such quads stay unbaked ([`irregular_colors`]).
fn bake_quad(colors: Option<[[f32; 4]; 4]>) -> Option<Baked> {
    let Some(cs) = colors else { return None };
    if cs.iter().any(|c| !(0.0..=1.0).contains(&c[3])) {
        return None; // paint offsets, or an alpha glTF factors cannot carry
    }
    let clamped: [[f32; 4]; 4] =
        cs.map(|c| [c[0].clamp(0., 1.), c[1].clamp(0., 1.), c[2].clamp(0., 1.), c[3]]);
    if clamped.iter().all(|c| *c == clamped[0]) {
        Some(Baked::Uniform(clamped[0]))
    } else {
        Some(Baked::PerVertex)
    }
}
/// A glTF material name for a baked colour: `brick-E6E600`, an alpha suffix
/// where the surface is transparent.
fn material_name(c: [f32; 4]) -> String {
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    let rgb = format!("{:02X}{:02X}{:02X}", byte(c[0]), byte(c[1]), byte(c[2]));
    if c[3] < 1. {
        format!("brick-{rgb}-A{:02X}", byte(c[3]))
    } else {
        format!("brick-{rgb}")
    }
}
/// A quad whose four vertices all carry the same signed offset onto the
/// player's paint (the negative-alpha sentinel): glTF has no word for it, so
/// its primitive names it in `bri_brick.paint_offsets`.
fn paint_offset(colors: Option<[[f32; 4]; 4]>) -> Option<[f32; 3]> {
    let Some(cs) = colors else { return None };
    if !cs.iter().all(|c| c[3] < 0.0 && [c[0], c[1], c[2]].iter().all(|v| v.is_finite())) {
        return None;
    }
    let same = cs
        .iter()
        .all(|c| [c[0], c[1], c[2]] == [cs[0][0], cs[0][1], cs[0][2]]);
    same.then_some([cs[0][0], cs[0][1], cs[0][2]])
}
/// A colour layout neither glTF channel nor the offset marker can carry: a
/// negative alpha mixed with non-negative ones within one quad, an offset whose
/// RGB varies per vertex, or an out-of-range alpha. Such quads keep the legacy
/// `bri_brick.colors` dump on export so no authored value is ever lost.
fn irregular_colors(colors: Option<[[f32; 4]; 4]>) -> bool {
    colors.is_some() && paint_offset(colors).is_none() && bake_quad(colors).is_none()
}
/// A run of quads as one primitive's payloads: interleaved positions, normals
/// and UVs (each quad keeps its own four vertices unshared), the two-triangle
/// index list, and each quad's four vertex indices within the primitive —
/// what [`brick_from_glb`] pairs back on without guessing.
fn quad_payload(brick: &Brick, quad_idx: &[usize]) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<u32>, Value) {
    let positions: Vec<f32> = quad_idx
        .iter()
        .flat_map(|&i| brick.quads[i].vertices.iter().flat_map(|v| mirror(v.position)))
        .collect();
    let normals: Vec<f32> = quad_idx
        .iter()
        .flat_map(|&i| brick.quads[i].vertices.iter().flat_map(|v| mirror(v.normal)))
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
    // The quads' vertex indices within this primitive (four per quad), so the
    // lowering rebuilds each quad without guessing pairings.
    let quads: Value = Value::Array(
        (0..quad_idx.len())
            .map(|k| {
                json!([
                    4 * k as u32,
                    4 * k as u32 + 1,
                    4 * k as u32 + 2,
                    4 * k as u32 + 3
                ])
            })
            .collect(),
    );
    (positions, normals, uvs, indices, quads)
}
/// A decoded stock brick-surface picture for a placeholder material:
/// `(width, height, RGBA8 rows)` — what the shared-picture tools composite
/// over white before writing the files [`SurfaceTextures`] names.
pub type SurfacePicture = (u32, u32, Vec<u8>);
/// The installed shared stock brick-surface pictures an exported brick's
/// runtime-coloured placeholders reference in editors: pre-composited-over-
/// white copies of what the game decal-composites over a default-painted
/// brick, stored once beside the `brick-materials` bundle. Each field is the
/// picture's path relative to the content root; absent pictures leave their
/// placeholder plain — [`export_brick`] is the all-absent case. `Print`
/// never carries one: its decal is chosen per placed brick at runtime
/// (unprinted draws bare paint).
#[derive(Default, Clone)]
pub struct SurfaceTextures {
    pub top: Option<String>,
    pub side: Option<String>,
    pub bottom_edge: Option<String>,
    pub bottom_loop: Option<String>,
    pub ramp: Option<String>,
}
impl SurfaceTextures {
    /// The shared picture a runtime-coloured quad of `surface` shows in
    /// editors, relative to the content root.
    pub fn for_surface(&self, surface: &Surface) -> Option<&str> {
        match surface {
            Surface::Top => self.top.as_deref(),
            Surface::Side => self.side.as_deref(),
            Surface::BottomEdge => self.bottom_edge.as_deref(),
            Surface::BottomLoop => self.bottom_loop.as_deref(),
            Surface::Ramp => self.ramp.as_deref(),
            // Prints are chosen per placed brick at runtime.
            Surface::Print => None,
        }
    }
}
/// A glTF image URI from the folder of a `.glb` being written (`from_dir`,
/// relative to the content root) to a shared picture (`target`, likewise):
/// enough `..` segments to climb out of the file's folder, then down. Pure
/// path math — no filesystem access; both arguments are forward-slash paths
/// under one root and `from_dir` names a directory only.
pub fn relative_uri(from_dir: &Path, target: &str) -> String {
    let from: Vec<String> = from_dir
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .filter(|s| *s != ".")
        .collect();
    let to: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();
    // The shared prefix counts directory components only, never the file.
    let dirs = to.len().saturating_sub(1);
    let common = from
        .iter()
        .zip(to.iter().take(dirs))
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = String::new();
    for _ in common..from.len() {
        out.push_str("../");
    }
    out.push_str(&to[common..].join("/"));
    out
}
/// The placeholder material name for a runtime-coloured surface: `blank` on
/// the top (Blockland's word for a paintable face), suffixed elsewhere so an
/// editor sees which stock surface decal rides it.
fn blank_name(surface: &Surface) -> &'static str {
    match surface {
        Surface::Top => "blank",
        Surface::Side => "blank-side",
        Surface::BottomEdge => "blank-bottom-edge",
        Surface::BottomLoop => "blank-bottom-loop",
        Surface::Ramp => "blank-ramp",
        Surface::Print => "blank-print",
    }
}
/// The brick as a `.glb`. One mesh of triangle primitives grouped by the
/// quad's (face, surface, baked colour); every quad keeps its own four
/// vertices unshared so [`brick_from_glb`] rebuilds it exactly. What glTF has
/// no word for rides in extras: per-primitive `bri_brick` and asset-level
/// `bri_brick_meta`. Before returning the output is lowered again with
/// [`brick_from_glb`] and compared to the input, so an exported brick always
/// reads back as itself.
pub fn export_brick(brick: &Brick) -> Result<Vec<u8>> {
    export_brick_with_surfaces(brick, &SurfaceTextures::default(), None)
}
/// The brick as a `.glb`, its runtime-coloured placeholders referencing the
/// installed shared stock surface pictures where `surfaces` names them (see
/// [`export_brick`] for the rest of the contract). `glb_dir` is this file's
/// own folder relative to the content root: with it, each named picture is
/// referenced by a relative URI from the file's folder; without it — or
/// where a surface has no installed picture — the placeholder ships plain.
pub fn export_brick_with_surfaces(
    brick: &Brick,
    surfaces: &SurfaceTextures,
    glb_dir: Option<&Path>,
) -> Result<Vec<u8>> {
    let mut g = Gltf::new();

    // Group the quads by (face, surface, baked colour); within a group each
    // quad keeps its own four vertices so the lowering can pair them back.
    // The baked colour decides the primitive's glTF channel; runtime-coloured
    // groups (plain paint, offsets) carry a placeholder `blank` material plus
    // the marker that keeps the game from baking it. Interior panels both-wound
    // back toward the brick's centroid are no exception: they ride their
    // surface's group like any other quad, so an editor sees exactly what the
    // lowering reads.
    let mut groups: Vec<(Face, Surface, Option<Baked>, Vec<usize>)> = Vec::new();
    for (i, q) in brick.quads.iter().enumerate() {
        let baked = bake_quad(q.colors);
        match groups
            .iter_mut()
            .find(|(f, s, b, _)| *f == q.face && *s == q.surface && *b == baked)
        {
            Some((_, _, _, idx)) => idx.push(i),
            None => groups.push((q.face, q.surface, baked, vec![i])),
        }
    }
    ensure!(!groups.is_empty(), "the brick has no quads to draw");

    // One glTF material per distinct uniform colour (exact float equality;
    // a file holds at most a handful); transparent ones get BLEND so Blender
    // matches the game's alpha.
    let mut materials: Vec<([f32; 4], usize)> = Vec::new();
    // Placeholder materials for runtime-coloured groups, one per surface so
    // each shows its own stock decal (kept out of the colour dedup on purpose).
    let mut blanks: Vec<(Surface, usize)> = Vec::new();
    let mut primitives = Vec::new();
    for (face, surface, baked, quad_idx) in &groups {
        let (positions, normals, uvs, indices, quads) = quad_payload(brick, quad_idx);
        let mut extra = json!({
            "bri_brick": {
                "face": face,
                "surface": surface,
                "quads": quads,
            }
        });
        let runtime = baked.is_none()
            && quad_idx.iter().all(|&i| {
                brick.quads[i].colors.is_none() || paint_offset(brick.quads[i].colors).is_some()
            });
        if runtime
            || quad_idx.iter().any(|&i| paint_offset(brick.quads[i].colors).is_some())
        {
            // glTF has no word for a signed offset onto the player's paint, so
            // those quads name it here: aligned with `quads`, `[r, g, b]` or null.
            extra["bri_brick"]["paint_offsets"] = Value::Array(
                quad_idx
                    .iter()
                    .map(|&i| match paint_offset(brick.quads[i].colors) {
                        Some([r, g, b]) => json!([r, g, b]),
                        None => Value::Null,
                    })
                    .collect(),
            );
        }
        if quad_idx.iter().any(|&i| irregular_colors(brick.quads[i].colors)) {
            // A layout no glTF channel can carry: keep the legacy dump so no
            // authored value is lost. Aligned with `quads`; null where the quad
            // carries no authored colour (the player's paint shows).
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
        // Authored colours are the glTF channels themselves; runtime-coloured
        // groups carry a placeholder `blank` material so editors see the
        // surface — the marker keeps the game from baking it as a literal.
        match baked {
            None if !runtime => {}
            None => {
                let index = match blanks.iter().find(|(s, _)| *s == *surface) {
                    Some((_, i)) => *i,
                    None => {
                        // The shared picture by relative URI from this file's
                        // folder; plain where either side is missing.
                        let picture = surfaces
                            .for_surface(surface)
                            .zip(glb_dir)
                            .map(|(target, dir)| MaterialImage::Uri(relative_uri(dir, target)));
                        let i = g.material(
                            blank_name(surface),
                            [1.0, 1.0, 1.0, 1.0],
                            picture,
                            json!({}),
                        );
                        blanks.push((*surface, i));
                        i
                    }
                };
                prim["material"] = json!(index);
            }
            Some(Baked::Uniform(color)) => {
                let index = match materials.iter().find(|(c, _)| *c == *color) {
                    Some((_, i)) => *i,
                    None => {
                        let i = g.material(
                            &material_name(*color),
                            *color,
                            None,
                            if color[3] < 1. { json!({ "alphaMode": "BLEND" }) } else { json!({}) },
                        );
                        materials.push((*color, i));
                        i
                    }
                };
                prim["material"] = json!(index);
            }
            Some(Baked::PerVertex) => {
                let mut data = Vec::with_capacity(quad_idx.len() * 16);
                for &i in quad_idx.iter() {
                    let cs = brick.quads[i]
                        .colors
                        .expect("a per-vertex group only holds coloured quads");
                    for c in cs.iter() {
                        data.push(c[0].clamp(0., 1.));
                        data.push(c[1].clamp(0., 1.));
                        data.push(c[2].clamp(0., 1.));
                        data.push(c[3]);
                    }
                }
                prim["attributes"]["COLOR_0"] = json!(g.floats(&data, 4));
            }
        }
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
    // glTF factors are [0, 1]: out-of-range legacy RGB clamps on the way
    // through a material or COLOR_0, so compare with that clamp applied.
    let key = |q: &Quad| {
        let colors = q.colors.map(|cs| {
            cs.map(|c| [c[0].clamp(0., 1.), c[1].clamp(0., 1.), c[2].clamp(0., 1.), c[3]])
        });
        serde_json::to_string(&(q.face, q.surface, &q.vertices, colors)).unwrap_or_default()
    };
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
/// vertices and indices, grid facts from the asset's `bri_brick_meta`. Colours
/// come from the glTF channels themselves — a shared material factor for
/// uniform quads, `COLOR_0` where they vary — with the extras carrying only
/// what glTF has no word for: the `paint_offsets` marker (and, on pre-migration
/// files, the legacy `colors` dump as a last resort).
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
            // Authored colours live in the glTF channels: COLOR_0 where a quad
            // varies, else the shared material's baseColorFactor. The extras
            // carry only what glTF has no word for — the paint-offset marker,
            // and on pre-migration files the legacy colour dump as a fallback.
            let color0 = match p["attributes"].get("COLOR_0").and_then(|v| v.as_u64()) {
                Some(a) => Some(doc.floats(a as usize)?),
                None => None,
            };
            let factor: Option<[f32; 4]> = match p.get("material").and_then(|v| v.as_u64()) {
                None => None,
                Some(mi) => {
                    let f = &json["materials"][mi as usize]["pbrMetallicRoughness"]["baseColorFactor"];
                    ensure!(
                        f.is_array() && f.as_array().unwrap().len() == 4,
                        "a brick material carries no four baseColorFactor entries"
                    );
                    let arr = f.as_array().unwrap();
                    ensure!(
                        arr.iter().all(|x| x.as_f64().is_some_and(f64::is_finite)),
                        "a brick material's baseColorFactor is not finite"
                    );
                    Some([0, 1, 2, 3].map(|i| arr[i].as_f64().unwrap() as f32))
                }
            };
            let offsets: Option<Vec<Value>> = match extra.get("paint_offsets") {
                Some(Value::Array(o)) => Some(o.clone()),
                _ => None,
            };
            if let Some(list) = &offsets {
                ensure!(
                    list.len() == array(extra, "quads").len(),
                    "bri_brick paint offsets do not align with the primitive's quads"
                );
            }
            let legacy_colors: Option<Vec<Value>> = match extra.get("colors") {
                Some(Value::Array(c)) => Some(c.clone()),
                _ => None,
            };
            // Legacy-only: pre-migration files may carry `bri_brick.void`
            // primitives (the old exporter's black placeholder for interior
            // panels); their factor never wins as an authored colour. New
            // exports do not write the marker.
            let void = extra.get("void").and_then(Value::as_bool).unwrap_or(false);
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
                    colors: match color0.as_ref() {
                        // Per-vertex authored gradient; a three-component row is opaque.
                        Some(rows) => {
                            let mut out = [[1.0; 4]; 4];
                            for (slot, &vi) in idx.iter().enumerate() {
                                ensure!(
                                    vi < rows.len(),
                                    "brick COLOR_0 shorter than its positions"
                                );
                                let row = &rows[vi];
                                out[slot] = [row[0], row[1], row[2], row.get(3).copied().unwrap_or(1.0)];
                            }
                            Some(out)
                        },
                        // Uniform authored colour: the material's factor as-is, so a
                        // hand edit in Blender is exactly what the game draws. A
                        // marker present means runtime-coloured (the `blank`
                        // placeholder or an offset), where the factor never wins; a
                        // legacy void primitive's black placeholder likewise never
                        // does.
                        None if !void && factor.is_some() && offsets.as_ref().is_none() => {
                            Some([factor.unwrap(); 4])
                        }
                        None => match offsets.as_ref() {
                            // The marker is authoritative where present: null means
                            // plain paint, not "look elsewhere".
                            Some(list) => match list.get(k) {
                                Some(Value::Null) | None => None,
                                Some(o) => {
                                    let o = o
                                        .as_array()
                                        .context("a brick paint offset is not an array")?;
                                    ensure!(
                                        o.len() == 3
                                            && o.iter().all(|x| x.as_f64().is_some_and(f64::is_finite)),
                                        "a brick paint offset carries no finite [r, g, b]"
                                    );
                                    {
                                        let c = [
                                            o[0].as_f64().unwrap() as f32,
                                            o[1].as_f64().unwrap() as f32,
                                            o[2].as_f64().unwrap() as f32,
                                            -1.0,
                                        ];
                                        Some([c; 4])
                                    }
                                }
                            },
                            // Pre-migration files: the legacy dump still decides.
                            None => match legacy_colors.as_ref() {
                                Some(list) => match list.get(k) {
                                    Some(Value::Null) | None => None,
                                    Some(c) => Some(
                                        serde_json::from_value(c.clone()).context("quad colour")?,
                                    ),
                                },
                                None => None,
                            },
                        },
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
        let mut offset = quad(Face::South, Surface::Side, -0.5);
        offset.colors = Some([[0.2, -0.1, 0.3, -1.0]; 4]); // signed paint offset
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
                offset,
                quad(Face::Omni, Surface::Print, 0.25),
            ],
        };
        brick.validate().expect("fixture validates");

        let glb = export_brick(&brick).expect("export");
        let back = brick_from_glb(&glb, "test:brick/one").expect("lower back");
        assert_eq!(back, brick);

        // The painted quad bakes to one material; each runtime-coloured
        // surface (plain paint and offset) carries its own per-surface
        // placeholder so an editor sees every surface, with the marker
        // keeping them dynamic.
        let doc = Document::from_bytes(&glb, None).expect("parse the glb");
        let mats = &doc.json["materials"];
        assert_eq!(mats.as_array().unwrap().len(), 4);
        let by_name = |name: &str| -> &Value {
            mats.as_array()
                .unwrap()
                .iter()
                .find(|m| m["name"] == json!(name))
                .unwrap_or_else(|| panic!("no material named {name}"))
        };
        assert_eq!(
            by_name("brick-FF0000")["pbrMetallicRoughness"]["baseColorFactor"],
            json!([1.0, 0.0, 0.0, 1.0])
        );
        // One placeholder per runtime-coloured surface: top, side and print.
        for name in ["blank", "blank-side", "blank-print"] {
            let blank_mat = by_name(name);
            assert_eq!(
                blank_mat["pbrMetallicRoughness"]["baseColorFactor"],
                json!([1.0, 1.0, 1.0, 1.0])
            );
            assert!(blank_mat.get("alphaMode").is_none());
        }
        // Every material carries explicit PBR factors: an omitted
        // metallicFactor defaults to 1.0 per the glTF spec, which editors
        // render as flat white metal.
        for m in mats.as_array().unwrap() {
            assert_eq!(m["pbrMetallicRoughness"]["metallicFactor"], json!(0.0));
            assert_eq!(m["pbrMetallicRoughness"]["roughnessFactor"], json!(1.0));
        }
        let prims = &doc.json["meshes"][0]["primitives"];
        assert_eq!(prims.as_array().unwrap().len(), 4);
        // Every primitive now carries a material: the painted one its colour,
        // the three runtime-coloured ones the placeholder.
        let with_material = prims
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p.get("material").is_some())
            .count();
        assert_eq!(with_material, 4);

        // The marker rides every runtime-coloured primitive: null for plain
        // paint, [r, g, b] for the offset. No colour dump anywhere.
        let with_marker = prims
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["extras"]["bri_brick"].get("paint_offsets").is_some())
            .count();
        assert_eq!(with_marker, 3);
        for p in prims.as_array().unwrap() {
            assert!(
                p["extras"]["bri_brick"].get("colors").is_none(),
                "no primitive keeps a legacy colour dump"
            );
        }

        let by_face_prim = |face: &str| -> &Value {
            prims.as_array()
                .unwrap()
                .iter()
                .find(|p| p["extras"]["bri_brick"]["face"].as_str() == Some(face))
                .unwrap_or_else(|| panic!("no primitive for face {face}"))
        };
        // The plain-paint top quad: placeholder material, all-null marker.
        let top_prim = by_face_prim("top");
        assert_eq!(
            top_prim["extras"]["bri_brick"]["paint_offsets"],
            json!([Value::Null])
        );
        // The offset quad rides in the marker alongside the placeholder.
        // The marker holds f32 values; expect them exactly as serialized.
        let offset_prim = by_face_prim("south");
        assert_eq!(
            offset_prim["extras"]["bri_brick"]["paint_offsets"],
            json!([[0.2f32 as f64, -0.1f32 as f64, 0.3f32 as f64]])
        );
    }

    /// The colour policy the game uses decides what bakes: literal colours
    /// become materials (transparent ones BLEND, out-of-range values clamp),
    /// and runtime-coloured quads ride the `blank` placeholder plus marker.
    #[test]
    fn baked_materials_follow_the_colour_semantics() {
        let quad = |face: Face, surface: Surface, x: f32| Quad {
            face,
            surface,
            vertices: [
                Vertex { position: [x, 0.0, -1.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] },
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
                Vertex { position: [x, 0.0, -0.5], normal: [0.0, 1.0, 0.0], uv: [0.0, 1.0] },
            ],
            colors: None,
        };
        let mut glass = quad(Face::North, Surface::Side, -0.5);
        glass.colors = Some([[0.0, 1.0, 0.0, 0.5]; 4]); // transparent pane
        let mut offset = quad(Face::South, Surface::Side, -0.5);
        offset.colors = Some([[0.2, -0.1, 0.3, -1.0]; 4]); // signed paint offset
        let mut hot = quad(Face::East, Surface::Side, -0.5);
        hot.colors = Some([[2.0, 1.5, -0.5, 1.0]; 4]); // provisional: out of range

        let brick = Brick {
            schema_version: BRICK_SCHEMA,
            id: "test:brick/semantics".into(),
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
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![glass, offset, hot],
        };
        brick.validate().expect("fixture validates");

        let glb = export_brick(&brick).expect("export");
        // In-range colours and the sentinel round-trip exactly; out-of-range
        // RGB clamps on its way through the material factor.
        let back = brick_from_glb(&glb, "test:brick/semantics").expect("lower back");
        let by_face = |f: Face| back.quads.iter().find(|q| q.face == f).unwrap();
        assert_eq!(by_face(Face::North).colors, Some([[0.0, 1.0, 0.0, 0.5]; 4]));
        assert_eq!(by_face(Face::South).colors, Some([[0.2, -0.1, 0.3, -1.0]; 4]));
        assert_eq!(by_face(Face::East).colors, Some([[1.0, 1.0, 0.0, 1.0]; 4]));

        let doc = Document::from_bytes(&glb, None).expect("parse the glb");
        let mats = &doc.json["materials"];
        // Glass (BLEND), hot (clamped) and the placeholder for the offset quad.
        assert_eq!(mats.as_array().unwrap().len(), 3);
        let by_name = |name: &str| -> &Value {
            mats.as_array()
                .unwrap()
                .iter()
                .find(|m| m["name"] == json!(name))
                .unwrap_or_else(|| panic!("no material named {name}"))
        };
        let glass_mat = by_name("brick-00FF00-A80");
        assert_eq!(glass_mat["alphaMode"].as_str(), Some("BLEND"));
        assert_eq!(
            glass_mat["pbrMetallicRoughness"]["baseColorFactor"],
            json!([0.0, 1.0, 0.0, 0.5])
        );
        let hot_mat = by_name("brick-FFFF00");
        assert!(hot_mat.get("alphaMode").is_none());
        assert_eq!(
            hot_mat["pbrMetallicRoughness"]["baseColorFactor"],
            json!([1.0, 1.0, 0.0, 1.0])
        );
        // The offset quad's surface is a side: its placeholder is suffixed.
        let blank_mat = by_name("blank-side");
        assert_eq!(
            blank_mat["pbrMetallicRoughness"]["baseColorFactor"],
            json!([1.0, 1.0, 1.0, 1.0])
        );

        let prims = &doc.json["meshes"][0]["primitives"];
        assert_eq!(prims.as_array().unwrap().len(), 3);
        let with_material = prims
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p.get("material").is_some())
            .count();
        assert_eq!(with_material, 3);

        // The offset quad rides in the marker alongside its placeholder; no
        // primitive keeps a legacy dump.
        let with_marker = prims
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["extras"]["bri_brick"].get("paint_offsets").is_some())
            .count();
        assert_eq!(with_marker, 1);
        for p in prims.as_array().unwrap() {
            assert!(
                p["extras"]["bri_brick"].get("colors").is_none(),
                "no primitive keeps a legacy colour dump"
            );
        }
    }

    /// Replace a GLB's JSON chunk, keeping its binary data verbatim: the
    /// stand-in for a hand edit through Blender or any other glTF editor.
    fn glb_with_json(glb: &[u8], json: &Value) -> Vec<u8> {
        let u32le = |at: usize| u32::from_le_bytes(glb[at..at + 4].try_into().unwrap());
        assert_eq!(&glb[0..4], b"glTF");
        let (clen, ctype) = (u32le(12) as usize, u32le(16));
        assert_eq!(ctype, 0x4E4F534A); // the first chunk is the JSON document
        let rest = &glb[20 + clen..];
        let mut body = serde_json::to_vec(json).unwrap();
        while body.len() % 4 != 0 {
            body.push(b' '); // glTF pads JSON chunks with spaces
        }
        let total = 12usize + 8 + body.len() + rest.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&glb[0..4]);
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(rest);
        out
    }

    /// The point of the material channels: a colour edited straight in the GLB
    /// is exactly what the game lowers, with no second file to edit.
    #[test]
    fn a_hand_edit_to_a_material_factor_is_what_the_game_reads() {
        let quad = |face: Face, surface: Surface| Quad {
            face,
            surface,
            vertices: [
                Vertex { position: [-0.5, 0.0, -1.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] },
                Vertex { position: [0.0, 0.0, -1.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0] },
                Vertex { position: [0.0, 0.0, -0.5], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0] },
                Vertex { position: [-0.5, 0.0, -0.5], normal: [0.0, 1.0, 0.0], uv: [0.0, 1.0] },
            ],
            colors: None,
        };
        let mut red = quad(Face::North, Surface::Side);
        red.colors = Some([[1.0, 0.0, 0.0, 1.0]; 4]);
        let brick = Brick {
            schema_version: BRICK_SCHEMA,
            id: "test:brick/edited".into(),
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
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![red, quad(Face::South, Surface::Side)],
        };
        brick.validate().expect("fixture validates");
        let glb = export_brick(&brick).expect("export");

        let doc = Document::from_bytes(&glb, None).expect("parse the glb");
        let mut json = doc.json.clone();
        json["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"] =
            json!([0.0, 0.0, 1.0, 1.0]); // someone made the trim blue in Blender
        let edited = glb_with_json(&glb, &json);

        let back = brick_from_glb(&edited, "test:brick/edited").expect("lower the edit");
        let north = back.quads.iter().find(|q| q.face == Face::North).unwrap();
        assert_eq!(north.colors, Some([[0.0, 0.0, 1.0, 1.0]; 4]));
        let south = back.quads.iter().find(|q| q.face == Face::South).unwrap();
        // The placeholder material plus its all-null marker still lower to
        // plain paint: the marker wins over the factor.
        assert_eq!(south.colors, None);
    }

    /// Pre-migration files still carry the legacy `bri_brick.colors` dump with
    /// no material or marker: they must lower exactly as before this change.
    #[test]
    fn the_legacy_colour_dump_still_loads_on_pre_migration_files() {
        let quad = |face: Face, surface: Surface| Quad {
            face,
            surface,
            vertices: [
                Vertex { position: [-0.5, 0.0, -1.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] },
                Vertex { position: [0.0, 0.0, -1.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0] },
                Vertex { position: [0.0, 0.0, -0.5], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0] },
                Vertex { position: [-0.5, 0.0, -0.5], normal: [0.0, 1.0, 0.0], uv: [0.0, 1.0] },
            ],
            colors: None,
        };
        let mut glass = quad(Face::North, Surface::Side);
        glass.colors = Some([[0.0, 1.0, 0.0, 0.5]; 4]);
        let mut offset = quad(Face::South, Surface::Side);
        offset.colors = Some([[0.2, -0.1, 0.3, -1.0]; 4]);
        let mut hot = quad(Face::East, Surface::Side);
        hot.colors = Some([[2.0, 1.5, -0.5, 1.0]; 4]); // legacy out-of-range literal
        let brick = Brick {
            schema_version: BRICK_SCHEMA,
            id: "test:brick/legacy".into(),
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
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![glass, offset, hot],
        };
        brick.validate().expect("fixture validates");
        let glb = export_brick(&brick).expect("export");

        // Strip the new channels back to what a pre-migration file held.
        let doc = Document::from_bytes(&glb, None).expect("parse the glb");
        let mut json = doc.json.clone();
        for p in json["meshes"][0]["primitives"].as_array_mut().unwrap() {
            let face = p["extras"]["bri_brick"]["face"].clone();
            p.as_object_mut().unwrap().remove("material");
            let extra = &mut p["extras"]["bri_brick"];
            extra.as_object_mut().unwrap().remove("paint_offsets");
            let row: Vec<f32> = match face.as_str() {
                Some("north") => vec![0.0, 1.0, 0.0, 0.5],
                Some("south") => vec![0.2, -0.1, 0.3, -1.0],
                _ => vec![2.0, 1.5, -0.5, 1.0],
            };
            // Pre-migration shape: one entry per quad, four vertex rows each.
            let rowv = Value::Array(row.iter().map(|x| json!(x)).collect());
            extra["colors"] = Value::Array(vec![Value::Array(vec![rowv; 4])]);
        }
        let legacy = glb_with_json(&glb, &json);

        let back = brick_from_glb(&legacy, "test:brick/legacy")
            .expect("pre-migration files still load");
        // The dump decides verbatim: sentinel and out-of-range values intact.
        let by_face = |f: Face| back.quads.iter().find(|q| q.face == f).unwrap();
        assert_eq!(by_face(Face::North).colors, Some([[0.0, 1.0, 0.0, 0.5]; 4]));
        assert_eq!(by_face(Face::South).colors, Some([[0.2, -0.1, 0.3, -1.0]; 4]));
        assert_eq!(by_face(Face::East).colors, Some([[2.0, 1.5, -0.5, 1.0]; 4]));
    }

    /// Runtime-coloured surfaces each carry their own placeholder material,
    /// named per surface, showing its installed stock decal where one is
    /// given; Print never carries a picture (its decal is chosen at runtime).
    #[test]
    fn runtime_surfaces_carry_their_own_placeholders() {
        let quad = |face: Face, surface: Surface| Quad {
            face,
            surface,
            vertices: [
                Vertex { position: [-0.5, 0.0, -1.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] },
                Vertex { position: [0.0, 0.0, -1.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0] },
                Vertex { position: [0.0, 0.0, -0.5], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0] },
                Vertex { position: [-0.5, 0.0, -0.5], normal: [0.0, 1.0, 0.0], uv: [0.0, 1.0] },
            ],
            colors: None,
        };
        let brick = Brick {
            schema_version: BRICK_SCHEMA,
            id: "test:brick/surfaces".into(),
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
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![
                quad(Face::Top, Surface::Top),
                quad(Face::North, Surface::Side),
                quad(Face::Omni, Surface::Print),
            ],
        };
        brick.validate().expect("fixture validates");

        let surfaces = SurfaceTextures {
            top: Some("pack/brick-materials-002/surfaces-composited/top.png".into()),
            side: Some("pack/brick-materials-002/surfaces-composited/side.png".into()),
            ..Default::default()
        };
        let glb = export_brick_with_surfaces(
            &brick,
            &surfaces,
            Some(Path::new("addons/one/assets/bricks")),
        )
        .expect("export");
        // The pictures never leak into the lowering: it still reads back as
        // plain paint.
        assert_eq!(
            brick_from_glb(&glb, "test:brick/surfaces").expect("lower back"),
            brick
        );

        let doc = Document::from_bytes(&glb, None).expect("parse the glb");
        let mats = &doc.json["materials"];
        let by_name = |name: &str| -> &Value {
            mats.as_array()
                .unwrap()
                .iter()
                .find(|m| m["name"] == json!(name))
                .unwrap_or_else(|| panic!("no material named {name}"))
        };
        // Top and Side reference their shared pictures by relative URI from
        // the file's folder; Print stays plain.
        let uri_of = |material: &str| -> String {
            let texture_index = by_name(material)["pbrMetallicRoughness"]["baseColorTexture"]
                ["index"]
                .as_u64()
                .unwrap_or_else(|| panic!("no base colour texture on {material}")) as usize;
            let source = doc.json["textures"][texture_index]["source"].as_u64().unwrap() as usize;
            doc.json["images"][source]["uri"]
                .as_str()
                .expect("the picture is a URI, not an embedded bufferView")
                .to_owned()
        };
        assert_eq!(
            uri_of("blank"),
            "../../../../pack/brick-materials-002/surfaces-composited/top.png"
        );
        assert_eq!(
            uri_of("blank-side"),
            "../../../../pack/brick-materials-002/surfaces-composited/side.png"
        );
        let print = by_name("blank-print");
        assert!(print["pbrMetallicRoughness"].get("baseColorTexture").is_none());
        assert_eq!(
            print["pbrMetallicRoughness"]["baseColorFactor"],
            json!([1.0, 1.0, 1.0, 1.0])
        );

        // No file folder: the same surfaces ship plain instead of dangling.
        let glb = export_brick_with_surfaces(&brick, &surfaces, None).expect("export");
        let doc = Document::from_bytes(&glb, None).expect("parse the glb");
        for name in ["blank", "blank-side"] {
            assert!(doc.json["materials"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["name"] == json!(name))
                .unwrap()["pbrMetallicRoughness"]
                .get("baseColorTexture")
                .is_none());
        }
    }

    /// The shared-picture URI climbs out of the file's folder by exactly as
    /// many levels as it has, sharing what it can with the target.
    #[test]
    fn relative_uri_climbs_and_shares() {
        assert_eq!(
            relative_uri(Path::new("addons/one/assets/bricks"), "default-pack/x/top.png"),
            "../../../../default-pack/x/top.png"
        );
        // A deeper file climbs deeper.
        assert_eq!(
            relative_uri(
                Path::new("addons/deep/content/assets/bricks"),
                "default-pack/x/top.png"
            ),
            "../../../../../default-pack/x/top.png"
        );
        // A file under the same top-level pack shares that component.
        assert_eq!(
            relative_uri(
                Path::new("default-pack/maps-pass-008/assets/bricks"),
                "default-pack/brick-materials-002/surfaces-composited/top.png"
            ),
            "../../../brick-materials-002/surfaces-composited/top.png"
        );
        // A file at the root climbs nowhere.
        assert_eq!(relative_uri(Path::new(""), "default-pack/x/top.png"), "default-pack/x/top.png");
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

    /// Interior panels both-wound back toward the brick's centroid are plain
    /// runtime-coloured quads: they ride their surface's placeholder like any
    /// other (no black material, no void marker), so an editor sees exactly
    /// what the game lowers and the round trip stays exact. Outward hulls keep
    /// their authored colours untouched.
    #[test]
    fn interior_quads_ride_their_surface_placeholders() {
        let quad = |face: Face,
                    surface: Surface,
                    verts: [[f32; 3]; 4],
                    normal: [f32; 3]| Quad {
            face,
            surface,
            vertices: verts.map(|p| Vertex { position: p, normal, uv: [0.0, 0.0] }),
            colors: None,
        };
        // A unit-cube hull wound front-outward (the brick's centroid is the
        // origin), plus two interior panels both-wound back toward it — one
        // plain-paint, one carrying a signed paint offset.
        let mut north = quad(
            Face::North,
            Surface::Side,
            [[-1.0, 1.0, -1.0], [1.0, 1.0, -1.0], [1.0, -1.0, -1.0], [-1.0, -1.0, -1.0]],
            [0.0, 0.0, -1.0],
        );
        north.colors = Some([[1.0, 0.0, 0.0, 1.0]; 4]); // authored trim
        let mut east = quad(
            Face::East,
            Surface::Side,
            [[1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [1.0, 1.0, 1.0], [1.0, -1.0, 1.0]],
            [1.0, 0.0, 0.0],
        );
        east.colors = Some([[0.0, 1.0, 0.0, 1.0]; 4]);
        let mut west = quad(
            Face::West,
            Surface::Side,
            [[-1.0, -1.0, 1.0], [-1.0, 1.0, 1.0], [-1.0, 1.0, -1.0], [-1.0, -1.0, -1.0]],
            [-1.0, 0.0, 0.0],
        );
        west.colors = Some([[0.0, 0.0, 1.0, 1.0]; 4]);
        let mut bottom = quad(
            Face::Bottom,
            Surface::Top,
            [[-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, -1.0, 1.0], [-1.0, -1.0, 1.0]],
            [0.0, -1.0, 0.0],
        );
        bottom.colors = Some([[1.0, 1.0, 0.0, 1.0]; 4]);
        let mut interior_b = quad(
            Face::Omni,
            Surface::Side,
            [[-0.2, 0.2, 0.5], [0.2, 0.2, 0.5], [0.2, -0.2, 0.5], [-0.2, -0.2, 0.5]],
            [0.0, 0.0, -1.0], // geometric normal points at the centroid
        );
        interior_b.colors = Some([[0.1, 0.2, 0.3, -1.0]; 4]); // signed paint offset
        let brick = Brick {
            schema_version: BRICK_SCHEMA,
            id: "test:brick/interior".into(),
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
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![
                quad(
                    Face::Top,
                    Surface::Top,
                    [[-1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0]],
                    [0.0, 1.0, 0.0],
                ),
                north,
                quad(
                    Face::South,
                    Surface::Side,
                    [[-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0]],
                    [0.0, 0.0, 1.0],
                ),
                east,
                west,
                bottom,
                quad(
                    Face::Omni,
                    Surface::Side,
                    [[-0.2, -0.2, -0.5], [0.2, -0.2, -0.5], [0.2, 0.2, -0.5], [-0.2, 0.2, -0.5]],
                    [0.0, 0.0, 1.0], // geometric normal points at the centroid
                ),
                interior_b,
            ],
        };
        brick.validate().expect("fixture validates");

        let glb = export_brick(&brick).expect("export");
        // The interior panels lower back verbatim like any runtime-coloured
        // quad: plain paint and the offset both dynamic, hull colours intact.
        let back = brick_from_glb(&glb, "test:brick/interior").expect("lower back");
        assert_eq!(back, brick);
        let by_face = |f: Face| back.quads.iter().find(|q| q.face == f).unwrap();
        assert_eq!(by_face(Face::North).colors, Some([[1.0, 0.0, 0.0, 1.0]; 4]));
        let omnis = back
            .quads
            .iter()
            .filter(|q| q.face == Face::Omni)
            .collect::<Vec<_>>();
        assert_eq!(omnis[0].colors, None);
        assert_eq!(omnis[1].colors, Some([[0.1, 0.2, 0.3, -1.0]; 4]));

        let doc = Document::from_bytes(&glb, None).expect("parse the glb");
        let mats = &doc.json["materials"];
        // Two placeholders and four authored colours — no void-black anywhere.
        assert_eq!(mats.as_array().unwrap().len(), 6);
        let by_name = |name: &str| -> (&Value, usize) {
            let (i, m) = mats
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .find(|(_, m)| m["name"] == json!(name))
                .unwrap_or_else(|| panic!("no material named {name}"));
            (m, i)
        };
        for name in [
            "blank",
            "brick-FF0000",
            "blank-side",
            "brick-00FF00",
            "brick-0000FF",
            "brick-FFFF00",
        ] {
            let (m, _) = by_name(name);
            assert_eq!(m["pbrMetallicRoughness"]["metallicFactor"], json!(0.0));
            assert_eq!(m["pbrMetallicRoughness"]["roughnessFactor"], json!(1.0));
        }

        let prims = &doc.json["meshes"][0]["primitives"];
        // One primitive per (face, surface, colour) group: the two interior
        // panels share their surface's placeholder with no void marker.
        assert_eq!(prims.as_array().unwrap().len(), 7);
        assert!(
            prims
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["extras"]["bri_brick"].get("void").is_none()),
            "no primitive carries the legacy void marker"
        );
        // The interior panels ride one shared side placeholder, their offsets
        // in the usual marker: null for plain paint, [r, g, b] for the signed
        // offset.
        let omni_prim = prims
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["extras"]["bri_brick"]["face"] == json!("omni"))
            .expect("the interior panels keep their primitive");
        assert_eq!(omni_prim["extras"]["bri_brick"]["surface"], json!("side"));
        assert_eq!(omni_prim["material"].as_u64(), Some(by_name("blank-side").1 as u64));
        assert_eq!(omni_prim["extras"]["bri_brick"]["quads"].as_array().unwrap().len(), 2);
        assert_eq!(
            omni_prim["extras"]["bri_brick"]["paint_offsets"],
            json!([Value::Null, [0.1f32 as f64, 0.2f32 as f64, 0.3f32 as f64]])
        );
    }
}
