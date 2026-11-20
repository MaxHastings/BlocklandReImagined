//! A converted legacy model — the native [`Shape`] the DTS reader makes — as
//! a glTF binary for Blender, laid out so that exporting it, editing it and
//! re-importing it with `--kind body|item|prop` draws it the way the game
//! drew it (docs/modding/gltf-models.md): its meshes with their textures
//! beside their bones, each collision detail's objects as `col_` meshes
//! (their block masks in `bri_blocks`), and every clip on its nodes by name.
//!
//! Coordinates are turned glTF's way (+z forward): positions, normals and
//! translations mirrored in x and z; quaternions to (-x, y, -z, w); inverse
//! bind matrices conjugated through the same mirror. Each is the importer's
//! own undo formula, so a round trip lands back exactly where it started.
//! What glTF has no word for (a clip's true name and length, looping,
//! additive layering, object visibility frames, triggers, ground motion)
//! rides in each animation's `extras.bri_clip`, which this crate's importer
//! reads back as authoritative.
//! Before returning, [`export`] reads its own output back the way the game
//! lowers it and fails when the result would not draw there (a shape with no
//! visible mesh), so an exported `.glb` always loads.

use super::write::{Channel, Gltf};
use anyhow::{Context, Result, ensure};
use bri_content::shape::{Animation, Mesh, RotationInterpolation, Shape};
use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

/// Native (x right, y up, -z forward) to glTF (+z forward): the half turn
/// about Y that `--kind body|item|prop` undoes on import.
pub(super) fn point(v: [f32; 3]) -> [f32; 3] {
    [-v[0], v[1], -v[2]]
}

/// A native quaternion (x, y, z, w) to glTF's, the importer's `turn`.
fn turn(q: [f32; 4]) -> [f32; 4] {
    Quat::from_xyzw(-q[0], q[1], -q[2], q[3])
        .normalize()
        .to_array()
}

/// `C · m · C⁻¹` for an affine matrix, C = diag(−1, 1, −1).
fn conjugate(m: Mat4) -> Mat4 {
    let c = Mat4::from_scale(Vec3::new(-1.0, 1.0, -1.0));
    c * m * c
}

/// A name the importer will keep as-is: no control characters (it strips
/// them), at most 64 of them (it truncates there).
fn clean_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).take(64).collect()
}

/// Whether a visible mesh node named this would be read back as a collider.
fn collision_named(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("col_") || lower.starts_with("col-") || lower.starts_with("collision")
}

/// A scale the game would apply as one even stretch (the axis rotations then
/// cancel out of it and glTF's TRS holds it exactly).
fn is_uniform(s: &[f32; 3]) -> bool {
    let m = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    (s[0] - s[1]).abs() <= 1e-4 * m && (s[1] - s[2]).abs() <= 1e-4 * m
}

/// The Blockland semantics of a clip that glTF has no word for, as its
/// animation's `extras.bri_clip`: an importer that knows this table takes
/// the name, length, looping, layering and triggers from it instead of the
/// channels (which carry only per-node motion). Arrays ride only when they
/// are not empty.
fn clip_extras(a: &Animation) -> Value {
    let mut v = json!({
        "name": a.name,
        "frames": u64::try_from(a.frames).unwrap_or(u64::MAX),
        "duration": f64::from(a.duration),
        "looping": a.looping,
        "additive": a.additive,
        "priority": a.priority,
        "parent_translations": a.parent_translations,
        "rotation_interpolation": match a.rotation_interpolation {
            RotationInterpolation::Spherical => "spherical",
            RotationInterpolation::NormalizedLinear => "normalized_linear",
        },
    });
    if !a.triggers.is_empty() {
        v["triggers"] = Value::Array(
            a.triggers
                .iter()
                .map(|t| json!([u64::from(t.state), f64::from(t.position)]))
                .collect(),
        );
    }
    if !a.ground_translations.is_empty() {
        v["ground_translations"] = Value::Array(
            a.ground_translations
                .iter()
                .map(|g| g.iter().copied().map(f64::from).collect::<Vec<_>>())
                .map(Value::from)
                .collect(),
        );
    }
    if !a.ground_rotations.is_empty() {
        v["ground_rotations"] = Value::Array(
            a.ground_rotations
                .iter()
                .map(|g| g.iter().copied().map(f64::from).collect::<Vec<_>>())
                .map(Value::from)
                .collect(),
        );
    }
    if !a.objects.is_empty() {
        v["objects"] = Value::Array(
            a.objects
                .iter()
                .map(|o| {
                    json!({
                        "object": u64::try_from(o.object).unwrap_or(u64::MAX),
                        "visibility": o.visibility.iter().copied().map(f64::from).collect::<Vec<_>>(),
                        "frames": o.frames.iter().copied().map(u64::from).collect::<Vec<_>>(),
                        "material_frames": o.material_frames.iter().copied().map(u64::from).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        );
    }
    v
}

/// Node names, case-insensitively unique on export so the importer's own
/// deduping never renames one and animation channels keep finding them.
struct Names {
    taken: BTreeSet<String>,
    empty: usize,
}

impl Names {
    fn new() -> Self {
        Self {
            taken: BTreeSet::new(),
            empty: 0,
        }
    }

    /// The unique name for `base`; an empty one takes a counter.
    fn take(&mut self, base_in: &str) -> String {
        let base = if base_in.trim().is_empty() {
            self.empty += 1;
            format!("node{}", self.empty)
        } else {
            clean_name(base_in)
        };
        let mut name = base.clone();
        let mut k = 2;
        while !self.taken.insert(name.to_ascii_lowercase()) {
            name = format!("{base}_{k}");
            k += 1;
        }
        name
    }
}

/// A clip name the re-import will store without dropping a twin: the
/// importer's role match lowercases a name and folds spaces, dashes and dots
/// to underscores before comparing it with the roles (and their
/// underscore-free forms), so two clips that share any of those forms would
/// collapse into one; each form is reserved.
fn unique_clip(base_in: &str, taken: &mut BTreeSet<String>) -> String {
    let base = if base_in.trim().is_empty() {
        "clip".to_owned()
    } else {
        clean_name(base_in)
    };
    let norm = |name: &str| name.to_ascii_lowercase().replace([' ', '-', '.'], "_");
    let mut name = base.clone();
    let mut k = 1;
    loop {
        let lower = norm(&name);
        let stripped = lower.replace('_', "");
        // A folded form that is its own stripped form (a name without
        // underscores) reserves one string, not two.
        let same = stripped == lower;
        if !taken.contains(&lower) && (same || !taken.contains(&stripped)) {
            taken.insert(lower);
            if !same {
                taken.insert(stripped);
            }
            return name;
        }
        k += 1;
        name = format!("{base}_{k}");
    }
}

/// One entry of a glTF top-level array, pushed straight into the document.
fn push(g: &mut Gltf, key: &str, value: Value) -> usize {
    let a = g.json[key].as_array_mut().expect("array");
    a.push(value);
    a.len() - 1
}

/// A glTF accessor of u16 rows of four (JOINTS_0 on a model with more than
/// 256 bones; `Gltf::uints` writes those 8-bit).
fn u16_vec4(g: &mut Gltf, data: &[u32]) -> usize {
    let bytes: Vec<u8> = data
        .iter()
        .flat_map(|v| (*v as u16).to_le_bytes())
        .collect();
    let view = g.view(&bytes);
    push(
        g,
        "accessors",
        json!({
            "bufferView": view,
            "componentType": 5123,
            "count": data.len() / 4,
            "type": "VEC4"
        }),
    )
}

/// Everything one export shares: the document being written and the caches
/// that keep names unique and textures embedded once.
struct Builder<'a> {
    g: &'a mut Gltf,
    shape: &'a Shape,
    /// Native material index to its texture found beside the shape in the
    /// Add-On's source (the file's lower virtual path and bytes); `None`
    /// where Torque would have drawn the material untextured.
    textures: &'a BTreeMap<usize, Option<(String, Vec<u8>)>>,
    notes: &'a mut Vec<String>,
    names: Names,
    /// The materials a visible mesh actually draws; every native material is
    /// exported (the game binds its list positionally), but only these earn
    /// the per-material notes about what glTF approximated.
    used: BTreeSet<usize>,
    /// Native skeleton node to its glTF node index.
    node_index: Vec<usize>,
    material_of: BTreeMap<usize, usize>,
    tex_cache: BTreeMap<String, usize>,
    /// Whether each texture file carries a see-through pixel (checked once).
    tex_seethrough: BTreeMap<String, bool>,
    unlit: bool,
    /// Notes already given (a mesh several objects draw is built once each).
    noted: BTreeSet<String>,
}

impl<'a> Builder<'a> {
    /// First time a note for this key is due.
    fn once(&mut self, key: &str) -> bool {
        self.noted.insert(key.to_owned())
    }

    /// The glTF material of native index `i`, made on first use; `None` for a
    /// primitive that names none.
    fn material(&mut self, i: Option<usize>) -> Result<Option<usize>> {
        let Some(i) = i else { return Ok(None) };
        if let Some(&m) = self.material_of.get(&i) {
            return Ok(Some(m));
        }
        ensure!(
            i < self.shape.materials.len(),
            "a primitive names material {i}, which the model does not have"
        );
        let m = &self.shape.materials[i];
        let textured = self.textures.get(&i).and_then(|t| t.as_ref());
        // An opaque solid colour whose alpha carries a paint shift (blank,
        // black50...) writes the factor with full alpha, so editors see the
        // part solid; the functional colour rides in bri_legacy.color for an
        // exact re-import.
        let shifted = textured.is_none()
            && m.blend == "opaque"
            && m.color.is_some_and(|c| c[3] < 1.0);
        // A solid-colour material carries its factor (linear, glTF's space);
        // a picture or no colour keeps the default white.
        let base_color_factor = match (textured.is_none(), m.color) {
            (true, Some(c)) if shifted => json!([c[0], c[1], c[2], 1.0]),
            (true, Some(c)) => json!([c[0], c[1], c[2], c[3]]),
            _ => json!([1.0, 1.0, 1.0, 1.0]),
        };
        let mut mat = json!({
            "name": clean_name(&m.name),
            // Torque surfaces are matte; the factor is white so a texture's
            // own colour shows (the importer multiplies it into the pixels).
            "pbrMetallicRoughness": {
                "baseColorFactor": base_color_factor,
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
            },
        });
        match textured {
            Some((file, bytes)) => {
                let tex = self.embed_texture(file, bytes, m.wrap_u, m.wrap_v)?;
                mat["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": tex });
            }
            // A solid colour rides in its factor alone.
            None if m.color.is_some() => {}
            None if self.used.contains(&i) => self.notes.push(format!(
                "material {} has no texture in the Add-On; untextured in the .glb",
                m.name
            )),
            None => {}
        }
        // Bare-metal materials keep their look: the PBR factors say how metal
        // it is (so Blender shows real metal), and the native fields ride in
        // bri_legacy so a re-import draws exactly what the shape said.
        if let Some(metal) = &m.metal {
            mat["pbrMetallicRoughness"]["metallicFactor"] = json!(1.0);
            mat["pbrMetallicRoughness"]["roughnessFactor"] = json!(metal.roughness);
            if textured.is_none() && m.color.is_none() {
                // The reflectance colour is linear, like glTF's factor.
                mat["pbrMetallicRoughness"]["baseColorFactor"] =
                    json!([metal.color[0], metal.color[1], metal.color[2], 1.0]);
            }
        }
        // Blend: opaque needs nothing, alpha BLENDs (or MASKs where the game
        // cuts solid geometry out of it); additive and subtractive have no
        // glTF equivalent. An otherwise-opaque material whose picture is
        // see-through earns BLEND so editors show it transparent the way the
        // game does; the game still keys its own blending off bri_legacy.blend
        // (and ignores an opaque material's alpha), so in-game drawing is
        // untouched. Additive and subtractive keep their note above on purpose:
        // glTF has no add, and flipping them would soften a glow the game draws
        // from full-alpha pixels.
        let see_through = !matches!(m.blend.as_str(), "alpha" | "additive" | "subtractive")
            && match textured {
                Some((file, bytes)) => self.image_seethrough(file, bytes),
                None => false,
            };
        match m.blend.as_str() {
            "alpha" => {
                if let Some(cutoff) = m.alpha_cutoff {
                    mat["alphaMode"] = json!("MASK");
                    mat["alphaCutoff"] = json!(cutoff);
                } else {
                    mat["alphaMode"] = json!("BLEND");
                }
            }
            "additive" | "subtractive" if self.used.contains(&i) => self.notes.push(format!(
                "material {} blends {}; glTF has no such blend, so the .glb draws it opaque (the game reads the blend back from its extras)",
                m.name, m.blend
            )),
            _ if see_through => mat["alphaMode"] = json!("BLEND"),
            _ => {}
        }
        if m.unlit {
            mat["extensions"] = json!({ "KHR_materials_unlit": {} });
            self.unlit = true;
        }
        if m.double_sided {
            mat["doubleSided"] = json!(true);
        }
        // What glTF has no word for, kept where Blender can see it. The map
        // modifiers are materials of their own (the game's material list), so
        // they ride as the names of those.
        let map_name = |d: usize| -> Value {
            json!(
                self.shape
                    .materials
                    .get(d)
                    .map(|x| clean_name(&x.name))
                    .unwrap_or_else(|| format!("material{d}"))
            )
        };
        let mut legacy = json!({
            "blend": m.blend,
            "wrap_u": m.wrap_u,
            "wrap_v": m.wrap_v,
            "environment": m.environment,
            "mipmaps": m.mipmaps,
            "detail_scale": m.detail_scale,
            "reflectance": m.reflectance,
        });
        if let Some(d) = m.detail_map {
            legacy["detail_map"] = map_name(d);
        }
        if let Some(b) = m.bump_map {
            legacy["bump_map"] = map_name(b);
        }
        if let Some(r) = m.reflectance_map {
            legacy["reflectance_map"] = map_name(r);
        }
        // Always present (null where the shape says none), so a re-import can
        // tell "the shape had no overlay" from glTF's default.
        legacy["paint_overlay"] = m.paint_overlay.map(Value::Bool).unwrap_or(Value::Null);
        if shifted {
            let c = m.color.unwrap();
            legacy["color"] = json!([c[0], c[1], c[2], c[3]]);
        }
        if let Some(metal) = &m.metal {
            // The detail is a material index; this export writes the
            // materials in native order, so it survives the round trip.
            legacy["metal"] = serde_json::to_value(metal)
                .map_err(|e| anyhow::anyhow!("material {} has an unserialisable metal {e}", m.name))?;
        }
        mat["extras"] = json!({ "bri_legacy": legacy });
        let idx = push(self.g, "materials", mat);
        self.material_of.insert(i, idx);
        Ok(Some(idx))
    }

    /// The Add-On's texture file as an embedded glTF texture with the
    /// material's own wrap flags (`Gltf::texture` hardcodes repeat).
    fn embed_texture(
        &mut self,
        file: &str,
        bytes: &[u8],
        wrap_u: bool,
        wrap_v: bool,
    ) -> Result<usize> {
        if let Some(&t) = self.tex_cache.get(file) {
            return Ok(t);
        }
        let (data, mime): (Cow<'_, [u8]>, &str) = if bytes.starts_with(b"\x89PNG") {
            (Cow::Borrowed(bytes), "image/png")
        } else if bytes.starts_with(&[0xff, 0xd8]) {
            (Cow::Borrowed(bytes), "image/jpeg")
        } else {
            let image = image::load_from_memory(bytes)
                .with_context(|| format!("texture {file} is not an image the exporter reads"))?;
            let mut png = Vec::new();
            image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
            (Cow::Owned(png), "image/png")
        };
        let view = self.g.view(&data);
        let source = push(
            self.g,
            "images",
            json!({ "bufferView": view, "mimeType": mime }),
        );
        let sampler = push(
            self.g,
            "samplers",
            json!({
                "wrapS": if wrap_u { 10497 } else { 33071 },
                "wrapT": if wrap_v { 10497 } else { 33071 }
            }),
        );
        let t = push(
            self.g,
            "textures",
            json!({ "source": source, "sampler": sampler }),
        );
        self.tex_cache.insert(file.to_owned(), t);
        Ok(t)
    }

    /// Whether a picture carries any see-through pixel (alpha below one). Only
    /// PNGs do here; an all-opaque alpha channel still needs decoding to tell,
    /// so the fast path rejects non-PNG and channel-less PNGs without decoding.
    fn image_seethrough(&mut self, file: &str, bytes: &[u8]) -> bool {
        if let Some(&v) = self.tex_seethrough.get(file) {
            return v;
        }
        let png = bytes.len() >= 26 && bytes.starts_with(b"\x89PNG\r\n\x1a\n");
        let channel = png && matches!(bytes[25], 4 | 6); // gray+alpha or rgba
        let v = if !channel {
            false
        } else {
            image::load_from_memory(bytes)
                .map(|img| img.to_rgba8().pixels().any(|p| p[3] < 255))
                .unwrap_or(false)
        };
        self.tex_seethrough.insert(file.to_owned(), v);
        v
    }
}

/// One material group of a mesh being gathered: its vertices in first-seen
/// order (the object's own frame, turned to glTF axes) and the triangles'
/// local indices.
#[derive(Default)]
struct Group {
    /// Local index for each global vertex this group draws; `None` where it
    /// does not (a mesh's material groups share its one vertex list).
    locals: Vec<Option<u32>>,
    globals: Vec<usize>,
    positions: Vec<f32>,
    normals: Vec<f32>,
    uvs: Vec<f32>,
    indices: Vec<u32>,
}

/// The glTF mesh of a native one, drawn as its object draws it (that
/// vertex frame and material frame as the base, the other frames as morph
/// targets), its primitives grouped by material like the game draws them;
/// `None` where it has no triangles. A skinned mesh comes back with its
/// skin's index so its node may name it.
fn build_mesh(
    b: &mut Builder,
    name: &str,
    mesh_index: usize,
    mesh: &Mesh,
    with_materials: bool,
    base_frame: usize,
    material_frame: usize,
) -> Result<Option<(usize, Option<usize>)>> {
    let v = mesh.frame_vertices;
    ensure!(
        v > 0 && mesh.positions.len().is_multiple_of(v),
        "mesh `{name}` has no whole frames of vertices"
    );
    let frames = mesh.positions.len() / v;
    ensure!(
        mesh.normals.len() == mesh.positions.len(),
        "mesh `{name}`'s normals do not cover its vertices"
    );
    ensure!(
        mesh.uv.is_empty() || mesh.uv.len().is_multiple_of(v),
        "mesh `{name}`'s texture coordinates are not whole frames of vertices"
    );
    let uv_frames = if mesh.uv.is_empty() {
        0
    } else {
        mesh.uv.len() / v
    };

    // Group the triangles by material, deduping each group's vertices.
    let mut groups: BTreeMap<Option<usize>, Group> = BTreeMap::new();
    let mut dropped_triangles = 0;
    for prim in &mesh.primitives {
        let g = groups.entry(prim.material).or_insert_with(|| Group {
            locals: vec![None; v],
            ..Default::default()
        });
        for t in &prim.triangles {
            if !t.iter().all(|&x| (x as usize) < v) {
                dropped_triangles += 1;
                continue;
            }
            let mut local = [0u32; 3];
            for c in 0..3 {
                let x = t[c] as usize;
                if g.locals[x].is_none() {
                    let li = (g.positions.len() / 3) as u32;
                    g.positions
                        .extend(point(mesh.positions[base_frame * v + x]));
                    g.normals.extend(point(mesh.normals[base_frame * v + x]));
                    if uv_frames > 0 {
                        g.uvs.extend(mesh.uv[material_frame * v + x]);
                    }
                    g.locals[x] = Some(li);
                    g.globals.push(x);
                }
                local[c] = g.locals[x].unwrap();
            }
            g.indices.extend(local);
        }
    }
    if dropped_triangles > 0 && b.once(&format!("{mesh_index}:triangles")) {
        b.notes.push(format!(
            "{dropped_triangles} triangles of mesh `{name}` name vertices it does not have; they are left out"
        ));
    }

    // The skin, once: its joints' glTF nodes and the conjugated bind matrices.
    let (joints_gltf, ibms): (Vec<usize>, Vec<Mat4>) = match &mesh.skin {
        Some(skin) => {
            ensure!(
                skin.nodes.len() == skin.inverse_bind.len(),
                "mesh `{name}`'s skin has {} joints but {} bind matrices",
                skin.nodes.len(),
                skin.inverse_bind.len()
            );
            for j in &skin.nodes {
                ensure!(
                    *j < b.shape.nodes.len(),
                    "mesh `{name}`'s skin names a missing node"
                );
            }
            (
                skin.nodes.iter().map(|&j| b.node_index[j]).collect(),
                skin.inverse_bind
                    .iter()
                    .map(|m| conjugate(Mat4::from_cols_array(m)))
                    .collect(),
            )
        }
        None => (Vec::new(), Vec::new()),
    };

    // Which attributes actually move between the object's frame and the
    // others, so a target carries only those. Every primitive of the mesh
    // gets the same number of targets; skinned meshes keep none, as the game
    // draws them at their bind pose whatever the frames say.
    let animate = frames > 1 && mesh.skin.is_none();
    let (pos_varies, nrm_varies) = if animate {
        let base0 = base_frame * v;
        (
            (0..frames).any(|f| {
                f != base_frame
                    && (0..v).any(|x| mesh.positions[f * v + x] != mesh.positions[base0 + x])
            }),
            (0..frames).any(|f| {
                f != base_frame
                    && (0..v).any(|x| mesh.normals[f * v + x] != mesh.normals[base0 + x])
            }),
        )
    } else {
        (false, false)
    };

    let mut primitives = Vec::new();
    let mut dropped_bones = 0;
    let mut unweighted = 0;
    let mut non_unit_weights = 0;
    for (material, g) in &groups {
        if g.indices.is_empty() {
            continue;
        }
        let count = g.positions.len() / 3;
        let mut attributes = json!({ "POSITION": b.g.floats(&g.positions, 3) });
        attributes["NORMAL"] = json!(b.g.floats(&g.normals, 3));
        if uv_frames > 0 {
            attributes["TEXCOORD_0"] = json!(b.g.floats(&g.uvs, 2));
        }
        // Bone weights: raw, as the game applies them (it does not normalise),
        // at most four per vertex.
        if let Some(skin) = &mesh.skin {
            let mut per_v: Vec<Vec<(usize, f32)>> = vec![Vec::new(); count];
            for inf in &skin.influences {
                if inf.vertex >= v || inf.bone >= skin.nodes.len() {
                    continue; // A corrupt influence; the mesh's own triangles bound it.
                }
                let Some(li) = g.locals[inf.vertex] else {
                    continue;
                };
                per_v[li as usize].push((inf.bone, inf.weight));
            }
            let mut jdata = vec![0u32; 4 * count];
            let mut wdata = vec![0f32; 4 * count];
            for (li, pairs) in per_v.iter().enumerate() {
                if pairs.is_empty() {
                    unweighted += 1;
                    continue;
                }
                let total: f32 = pairs.iter().map(|(_, w)| *w).sum();
                if (total - 1.0).abs() > 1e-3 {
                    non_unit_weights += 1;
                }
                dropped_bones += pairs.len().saturating_sub(4);
                for (slot, (bone, weight)) in pairs.iter().take(4).enumerate() {
                    jdata[4 * li + slot] = *bone as u32;
                    wdata[4 * li + slot] = *weight;
                }
            }
            let joints_acc = if jdata.iter().all(|&j| j < 256) {
                b.g.uints(&jdata, 4) // 8-bit rows
            } else {
                u16_vec4(b.g, &jdata)
            };
            attributes["JOINTS_0"] = json!(joints_acc);
            attributes["WEIGHTS_0"] = json!(b.g.floats(&wdata, 4));
        }
        let mut prim = json!({ "attributes": attributes, "indices": b.g.indices(&g.indices) });
        if with_materials && let Some(m) = b.material(*material)? {
            prim["material"] = json!(m);
        }
        // The other frames as morph targets (the re-import ignores them;
        // Blender shows the frame animation).
        if animate && (pos_varies || nrm_varies) {
            let mut targets = Vec::with_capacity(frames - 1);
            for f in 0..frames {
                if f == base_frame {
                    continue;
                }
                let mut target = json!({});
                if pos_varies {
                    let data: Vec<f32> = g
                        .globals
                        .iter()
                        .flat_map(|&x| {
                            point([
                                mesh.positions[f * v + x][0]
                                    - mesh.positions[base_frame * v + x][0],
                                mesh.positions[f * v + x][1]
                                    - mesh.positions[base_frame * v + x][1],
                                mesh.positions[f * v + x][2]
                                    - mesh.positions[base_frame * v + x][2],
                            ])
                        })
                        .collect();
                    target["POSITION"] = json!(b.g.floats(&data, 3));
                }
                if nrm_varies {
                    let data: Vec<f32> = g
                        .globals
                        .iter()
                        .flat_map(|&x| {
                            point([
                                mesh.normals[f * v + x][0] - mesh.normals[base_frame * v + x][0],
                                mesh.normals[f * v + x][1] - mesh.normals[base_frame * v + x][1],
                                mesh.normals[f * v + x][2] - mesh.normals[base_frame * v + x][2],
                            ])
                        })
                        .collect();
                    target["NORMAL"] = json!(b.g.floats(&data, 3));
                }
                targets.push(target);
            }
            prim["targets"] = json!(targets);
        }
        primitives.push(prim);
    }
    if primitives.is_empty() {
        return Ok(None);
    }
    let mesh_idx = b.g.mesh(name, primitives);
    let skin_idx = match &mesh.skin {
        Some(_) => Some(b.g.skin(&joints_gltf, &ibms)),
        None => None,
    };
    if unweighted > 0 && b.once(&format!("{mesh_index}:unweighted")) {
        b.notes.push(format!(
            "{unweighted} vertices of mesh `{name}` carry no bone weights; the re-import gives them to its first bone"
        ));
    }
    if dropped_bones > 0 && b.once(&format!("{mesh_index}:bones")) {
        b.notes.push(format!(
            "mesh `{name}` weighs some vertices with more than four bones; only the first four ride in the .glb"
        ));
    }
    if non_unit_weights > 0 && b.once(&format!("{mesh_index}:weights")) {
        b.notes.push(format!(
            "{non_unit_weights} vertices of mesh `{name}` carry bone weights that do not sum to one (the game applies them raw); glTF normalises them on import"
        ));
    }
    Ok(Some((mesh_idx, skin_idx)))
}

/// The model as a `.glb`. `textures` maps each native material index to the
/// texture found beside the shape in the Add-On's source (the file's lower
/// virtual path and bytes), or `None` where Torque would have drawn it
/// untextured. What was dropped or approximated comes back in `notes`, and
/// the second return value is the set of those texture files' lower virtual
/// paths that ride inside the `.glb` (the caller may then skip writing them
/// out again). The result is checked by re-importing it as an item model; a
/// shape glTF cannot hold (no visible mesh) fails so the caller can keep its
/// legacy file.
pub fn export(
    shape: &Shape,
    textures: &BTreeMap<usize, Option<(String, Vec<u8>)>>,
    notes: &mut Vec<String>,
) -> Result<(Vec<u8>, BTreeSet<String>)> {
    let mut g = Gltf::new();
    let mut b = Builder {
        g: &mut g,
        shape,
        textures,
        notes,
        names: Names::new(),
        used: BTreeSet::new(),
        node_index: Vec::new(),
        material_of: BTreeMap::new(),
        tex_cache: BTreeMap::new(),
        tex_seethrough: BTreeMap::new(),
        unlit: false,
        noted: BTreeSet::new(),
    };

    // --- The skeleton ---------------------------------------------------
    let mut node_names = Vec::with_capacity(shape.nodes.len());
    for n in &shape.nodes {
        node_names.push(b.names.take(&n.name));
    }
    for (i, n) in shape.nodes.iter().enumerate() {
        let mut v = json!({ "name": node_names[i].clone() });
        if n.translation != [0.0; 3] {
            v["translation"] = json!(point(n.translation));
        }
        if n.rotation != [0.0, 0.0, 0.0, 1.0] {
            v["rotation"] = json!(turn(n.rotation));
        }
        b.node_index.push(b.g.node(v, None));
    }
    // Parent links (a parent may come after its children in the file).
    for (i, n) in shape.nodes.iter().enumerate() {
        let Some(p) = n.parent else { continue };
        ensure!(
            p < b.node_index.len(),
            "node `{}` names a missing parent",
            node_names[i]
        );
        let roots = b.g.json["scenes"][0]["nodes"].as_array_mut().unwrap();
        *roots = roots.iter().filter(|x| **x != json!(i)).cloned().collect();
        let children = &mut b.g.json["nodes"][p]["children"];
        if !children.is_array() {
            *children = json!([]);
        }
        children.as_array_mut().unwrap().push(json!(i));
    }

    // --- The meshes ------------------------------------------------------
    let mut mesh_nodes = 0usize;
    let mut empty_meshes = 0usize;

    let mut slot_nodes = [0usize; 2];
    let mut slot_names = [Vec::new(), Vec::new()];
    // Visible details: the first two (the second as each object's `_LOD1`
    // twin); later ones have no place in glTF. The re-import merges a twin
    // into its original when every twin has one (a distant LOD) and keeps an
    // unmatched twin's geometry of its own (a first-person copy), taking the
    // details' names, thresholds and written node names from the asset's
    // `bri_details`.
    let visible: Vec<usize> = shape
        .details
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.collision)
        .map(|(i, _)| i)
        .collect();
    if visible.len() > 2 {
        b.notes.push(format!(
            "{} model details beyond the first two have no place in glTF and were left out",
            visible.len() - 2
        ));
    }
    // Every native material gets its glTF material, in the shape's own
    // declaration order and before any mesh is written: a re-imported .glb
    // then keeps the full native material list positionally (the game binds
    // textures to it by position), including materials no visible triangle
    // uses. The set of drawn ones still gates the per-material notes.
    let mut used_materials: BTreeSet<usize> = BTreeSet::new();
    for &di in visible.iter().take(2) {
        let d = &shape.details[di];
        for oi in d.object_start..d.object_start + d.object_count {
            if oi >= shape.objects.len() {
                continue; // Shape::validate rules this out.
            }
            let o = &shape.objects[oi];
            let Some(&mi) = o.meshes.get(d.mesh_offset) else {
                continue;
            };
            let Some(m) = shape.meshes.get(mi).and_then(Option::as_ref) else {
                continue;
            };
            for p in &m.primitives {
                if let Some(i) = p.material {
                    used_materials.insert(i);
                }
            }
        }
    }
    b.used = used_materials;
    for i in 0..shape.materials.len() {
        b.material(Some(i))?;
    }

    let mut missing_slots = 0usize;
    for (slot, &di) in visible.iter().take(2).enumerate() {
        let d = &shape.details[di];
        for oi in d.object_start..d.object_start + d.object_count {
            if oi >= shape.objects.len() {
                continue; // Shape::validate rules this out.
            }
            let o = &shape.objects[oi];
            let Some(&mi) = o.meshes.get(d.mesh_offset) else {
                missing_slots += 1;
                continue;
            };
            let Some(m) = shape.meshes.get(mi).and_then(Option::as_ref) else {
                continue;
            };
            // The frame the object draws: its own, clamped to what the mesh
            // has (skinned meshes are always drawn at their bind pose).
            let frames = if m.frame_vertices > 0 {
                m.positions.len() / m.frame_vertices
            } else {
                1
            };
            let base_frame = if m.skin.is_some() {
                0
            } else {
                let f = o.frame as usize;
                if f >= frames && b.once(&format!("{mi}:frame")) {
                    b.notes.push(format!(
                        "object `{}` draws mesh {} at vertex frame {}, which has only {frames}; the .glb draws its last",
                        o.name, mi, f
                    ));
                }
                f.min(frames.saturating_sub(1))
            };
            let uv_frames = if m.uv.is_empty() {
                0
            } else {
                m.uv.len() / m.frame_vertices
            };
            let material_frame = if uv_frames == 0 {
                0
            } else {
                let mf = o.material_frame as usize;
                if mf >= uv_frames && b.once(&format!("{mi}:material_frame")) {
                    b.notes.push(format!(
                        "object `{}` draws mesh {} at material frame {}, which has only {uv_frames}; the .glb draws its last",
                        o.name, mi, mf
                    ));
                }
                if uv_frames > 1 && b.once(&format!("{mi}:uv_frames")) {
                    b.notes.push(format!(
                        "mesh {} carries {uv_frames} texture-coordinate frames (material frames); the .glb keeps only this object's",
                        mi
                    ));
                }
                mf.min(uv_frames - 1)
            };
            // A visible object named like a collider would be read back as
            // one, so it is renamed to stay drawn.
            let mut base = o.name.clone();
            if collision_named(&base) {
                b.notes.push(format!(
                    "object `{base}` is named like a collision mesh, which the re-import reads as a collider; the .glb names it `vis_{base}`"
                ));
                base = format!("vis_{base}");
            }
            let mut name = b.names.take(&base);
            if slot == 1 {
                name = b.names.take(&format!("{name}_LOD1"));
            }
            // Skinned meshes sit at the scene root with no transform: the game
            // applies none to their vertices, and glTF keeps a skin's node
            // childless.
            let parent = if m.skin.is_some() {
                None
            } else {
                o.node.map(|n| b.node_index[n])
            };
            match build_mesh(&mut b, &name, mi, m, true, base_frame, material_frame)? {
                Some((mesh_i, skin_i)) => {
                    let mut nv = json!({ "name": name.clone() });
                    nv["mesh"] = json!(mesh_i);
                    if let Some(s) = skin_i {
                        nv["skin"] = json!(s);
                    }
                    b.g.node(nv, parent);
                    mesh_nodes += 1;
                    slot_nodes[slot] += 1;
                    slot_names[slot].push(name.clone());
                }
                None => empty_meshes += 1,
            }
        }
    }

    // glTF has no word for a model's visible details, so their names,
    // thresholds and written nodes ride in the asset's extras; the re-import
    // restores them (a holder must still reach `detail9999`). The node names
    // mark which of the re-imported objects each slot wrote: scene traversal
    // may put a first-person copy ahead of its original.
    if !visible.is_empty() {
        let table = visible.iter().take(2).enumerate().map(|(slot, &di)| {
            json!({
                "name": shape.details[di].name,
                "pixel_threshold": shape.details[di].pixel_threshold,
                "count": slot_nodes[slot],
                "nodes": slot_names[slot].clone(),
            })
        });
        b.g.json["asset"]["extras"] = json!({ "bri_details": table.collect::<Vec<_>>() });
    }

    // Collision details: one `col_` node per object with a mesh at the
    // detail's offset. The re-import merges them into colliders by block mask.
    let mut collision_nodes = 0usize;
    for d in shape.details.iter().filter(|d| d.collision) {
        for oi in d.object_start..d.object_start + d.object_count {
            if oi >= shape.objects.len() {
                continue;
            }
            let o = &shape.objects[oi];
            let Some(&mi) = o.meshes.get(d.mesh_offset) else {
                missing_slots += 1; // The game skips it too.
                continue;
            };
            let Some(m) = shape.meshes.get(mi).and_then(Option::as_ref) else {
                continue;
            };
            let name = b.names.take(&format!("col_{}", o.name));
            if let Some((mesh_i, _skin)) = build_mesh(&mut b, &name, mi, m, false, 0, 0)? {
                let mut nv = json!({ "name": name.clone() });
                nv["mesh"] = json!(mesh_i);
                if let Some(list) = &d.blocks {
                    // The queries this detail blocks; the re-import reads
                    // them back as the collider's mask (none blocks every).
                    if bri_content::blocks::valid(list) {
                        nv["extras"] = json!({ "bri_blocks": list });
                    } else {
                        b.notes.push(format!(
                            "collision detail `{}` names query types the game does not know ({}); its collider blocks every query",
                            d.name,
                            list.join(", ")
                        ));
                    }
                }
                b.g.node(nv, o.node.map(|n| b.node_index[n]));
                collision_nodes += 1;
            }
        }
    }
    if missing_slots > 0 {
        b.notes.push(format!(
            "{missing_slots} detail objects point at a mesh slot they do not have; the game skips them too"
        ));
    }
    let special_colliders = shape
        .details
        .iter()
        .filter(|d| d.collision && (d.two_sided || d.shot.is_some()))
        .count();
    if special_colliders > 0 {
        b.notes.push(format!(
            "{special_colliders} collision details set two-sided or shot behaviour, which glTF has no word for; re-imported colliders use the defaults"
        ));
    }

    // --- The clips -------------------------------------------------------
    let mut node_by_name: BTreeMap<String, usize> = BTreeMap::new();
    let mut name_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (i, n) in shape.nodes.iter().enumerate() {
        let lower = n.name.to_ascii_lowercase();
        node_by_name.entry(lower.clone()).or_insert(i);
        *name_counts.entry(lower).or_insert(0) += 1;
    }
    let mut taken_clips: BTreeSet<String> = BTreeSet::new();
    let mut missing_nodes = 0usize;
    let mut negative_scales = 0usize;
    for a in &shape.animations {
        let f = a.frames;
        if a.duration < 1e-6 {
            b.notes.push(format!(
                "clip `{}` ({} frame(s), {:.4}s) holds no motion; left out",
                a.name, f, a.duration
            ));
            continue;
        }

        // Key times: one per frame, and for loops a seam key at the end that
        // repeats frame 0, so the re-import measures exactly this duration.
        let d = a.duration;
        let mut times = Vec::with_capacity(f + 1);
        if f == 1 {
            times.push(0.0);
        } else if a.looping {
            for i in 0..f {
                times.push(d * i as f32 / f as f32);
            }
            times.push(d);
        } else {
            for i in 0..f {
                times.push(d * i as f32 / (f - 1) as f32);
            }
        }

        let mut channels: Vec<(usize, &'static str, Vec<f32>)> = Vec::new();
        let mut noted_dups: BTreeSet<String> = BTreeSet::new();
        let mut baked = false;
        for t in &a.nodes {
            let lower = t.node.to_ascii_lowercase();
            let Some(&ni) = node_by_name.get(&lower) else {
                missing_nodes += 1;
                continue;
            };
            if name_counts[&lower] > 1 && !noted_dups.insert(lower.clone()) {
                b.notes.push(format!(
                    "clip `{}` animates `{}`, which {} nodes share; the first is used",
                    a.name, t.node, name_counts[&lower]
                ));
            }
            let gi = b.node_index[ni];
            if !t.translations.is_empty() {
                let mut values = Vec::with_capacity(3 * times.len());
                for k in 0..times.len() {
                    values.extend(point(t.translations[k % f]));
                }
                channels.push((gi, "translation", values));
            }
            // The game scales each frame by Q·D(s)·Q⁻¹ about an axis; glTF's
            // TRS has no room for the axis. Where every frame's scale is even
            // it cancels exactly; otherwise the axis bakes into the rotation
            // (a local-space difference, noted below).
            let bake = !t.scale_rotations.is_empty() && !t.scales.iter().all(is_uniform);
            baked |= bake;
            if !t.rotations.is_empty() || bake {
                let rest = shape.nodes[ni].rotation;
                let mut values = Vec::with_capacity(4 * times.len());
                for k in 0..times.len() {
                    let i = k % f;
                    let r = if t.rotations.is_empty() {
                        rest
                    } else {
                        t.rotations[i]
                    };
                    let q = if bake {
                        Quat::from_array(r) * Quat::from_array(t.scale_rotations[i])
                    } else {
                        Quat::from_array(r)
                    };
                    values.extend(turn(q.to_array()));
                }
                channels.push((gi, "rotation", values));
            }
            if !t.scales.is_empty() {
                // An additive clip's scales are stored raw by the re-import,
                // so a mirror there is no loss; only an absolute one loses it.
                if !a.additive && t.scales.iter().any(|s| s.iter().any(|x| *x < 0.0)) {
                    negative_scales += 1;
                }
                let mut values = Vec::with_capacity(3 * times.len());
                for k in 0..times.len() {
                    values.extend(t.scales[k % f]);
                }
                channels.push((gi, "scale", values));
            }
        }
        if baked {
            b.notes.push(format!(
                "clip `{}` scales some nodes about an axis that moves between frames; glTF carries no such axis, so it is baked into those keys' rotations (a local-space difference from the game)",
                a.name
            ));
        }
        let name = unique_clip(&a.name, &mut taken_clips);
        let chans: Vec<Channel> = channels
            .iter()
            .map(|(n, p, v)| (*n, *p, "LINEAR", times.as_slice(), v.as_slice()))
            .collect();
        // The clip's Blockland semantics (its true name and length, looping,
        // layering, triggers) ride in extras.bri_clip; the re-import takes
        // them as authoritative over what the channels alone can say. A clip
        // whose nodes are all missing still rides with empty channels.
        b.g.animation_extras(&name, &chans, clip_extras(a));
    }
    if negative_scales > 0 {
        b.notes.push(format!(
            "{negative_scales} node track(s) scale by a negative value (a mirror); the re-import takes the magnitudes"
        ));
    }
    if missing_nodes > 0 {
        b.notes.push(format!(
            "{missing_nodes} node track(s) name nodes the model does not have; left out"
        ));
    }

    // --- Finishing --------------------------------------------------------
    let node_count = b.g.json["nodes"].as_array().unwrap().len();
    if node_count > 1024 {
        b.notes.push(format!(
            "the model has {node_count} nodes; re-importing it as a body, item or prop allows at most 1024 (a map allows more)"
        ));
    }
    if empty_meshes > 0 {
        b.notes.push(format!(
            "{empty_meshes} mesh(es) had no triangles and were left out"
        ));
    }
    if b.unlit {
        b.g.json["extensionsUsed"] = json!(["KHR_materials_unlit"]);
    }
    // The texture files that ride inside the .glb, by their lower virtual
    // paths; gathered before `g` is consumed.
    let embedded: BTreeSet<String> = b.tex_cache.keys().cloned().collect();
    let glb = g.glb();
    // A .glb the game could not lower would be worse than none: read back
    // what was just written, exactly as the client lowers it, and refuse it
    // while the model can still fall back to its legacy file.
    let doc =
        super::Document::from_bytes(&glb, None).context("the exported glTF does not read back")?;
    super::convert(
        &doc,
        None,
        &super::Options::new("", "", super::Kind::Item),
        "export check",
        "",
    )
    .with_context(|| {
        format!(
            "the exported glTF cannot be re-imported as an item model ({} mesh node(s), {} collision node(s)); the model ships no file and presents as a placeholder cube",
            mesh_nodes, collision_nodes
        )
    })?;
    Ok((glb, embedded))
}
