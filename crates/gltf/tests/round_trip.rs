//! Round-trip fidelity of real DTS-derived shapes: export to `.glb` and lower
//! it back with `convert`, and what the game draws must survive — materials in
//! their declaration order with their fields intact, geometry totals, node
//! names and clips. The corpus is every `.shape.json` under
//! `tests/fixtures/models` (real DTS-derived shapes kept as fixtures: several
//! use their materials out of declaration order, some carry animation clips),
//! `packages/` (this repository's own legacy model files) and the historical
//! sources below; the tests skip where a folder is absent.

use anyhow::{Context, Result};
use bri_content::shape::{Animation, Mesh, Shape};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

/// Every `.shape.json` in the corpus (skipped where a folder is absent).
fn corpus() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for base in [
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models"),
        repo_root().join(".research/archive/defaults"),
        repo_root().join("content/addons/weapon_shotgun/assets/models"),
        repo_root().join("packages"),
    ] {
        if !base.is_dir() {
            continue;
        }
        let mut stack = vec![base];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(".shape.json"))
                {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The materials the exporter writes for `shape`: those its visible meshes use,
/// mirroring `export_shape`'s own pre-pass (the first two non-collision details).
fn used_materials(shape: &Shape) -> BTreeSet<Option<usize>> {
    let visible: Vec<usize> = shape
        .details
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.collision)
        .map(|(i, _)| i)
        .collect();
    let mut used = BTreeSet::new();
    for &di in visible.iter().take(2) {
        let d = &shape.details[di];
        for oi in d.object_start..d.object_start + d.object_count {
            if oi >= shape.objects.len() {
                continue;
            }
            let o = &shape.objects[oi];
            let Some(&mi) = o.meshes.get(d.mesh_offset) else {
                continue;
            };
            let Some(m) = shape.meshes.get(mi).and_then(Option::as_ref) else {
                continue;
            };
            for p in &m.primitives {
                used.insert(p.material);
            }
        }
    }
    used
}

/// The shape in `path`.
fn load_shape(path: &Path) -> Result<Shape> {
    serde_json::from_slice(&std::fs::read(path)?)
        .with_context(|| format!("reading {}", path.display()))
}

/// Export `original` to a `.glb` and lower it back; the exporter's notes ride
/// along where a test wants them.
fn export_and_lower(original: &Shape) -> Result<(Shape, Vec<String>)> {
    let mut notes = Vec::new();
    let (glb, _embedded) = bri_gltf::export_shape(original, &BTreeMap::new(), &mut notes)?;
    let doc = bri_gltf::Document::from_bytes(&glb, None)?;
    let opts = bri_gltf::Options::new("", "", bri_gltf::Kind::Item);
    let lowered = bri_gltf::convert(&doc, None, &opts, "round_trip", "")?;
    Ok((lowered.shape, notes))
}

/// Whether the exporter writes a mesh the re-import can draw: at least one
/// triangle in its first two visible details. A collision-only shape has no
/// `.glb` at all (the re-import needs a visible mesh); its `.shape.json` is
/// everything it gets, so the tests skip those.
fn drawable(shape: &Shape) -> bool {
    for &di in &visible_details(shape) {
        let d = &shape.details[di];
        for oi in d.object_start..d.object_start + d.object_count {
            if oi >= shape.objects.len() {
                continue;
            }
            let o = &shape.objects[oi];
            let Some(&mi) = o.meshes.get(d.mesh_offset) else {
                continue;
            };
            let Some(m) = shape.meshes.get(mi).and_then(Option::as_ref) else {
                continue;
            };
            if m.primitives.iter().any(|p| !p.triangles.is_empty()) {
                return true;
            }
        }
    }
    false
}

/// The name the exporter gives a clip or material (control characters gone,
/// at most 64 of them).
fn clean(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).take(64).collect()
}

/// The name the lowering gives a material the exporter named `name`: the
/// exporter's clean (no control characters, at most 64) through the importer's
/// safe form.
fn lowered_name(name: &str) -> String {
    bri_gltf::safe_name(&clean(name))
}

/// The names the re-import gives the shape's materials, simulating its dedup
/// over them in declaration order: the exporter writes a glTF material for
/// every native one (the importer keeps their array order), so a second
/// material with the same safe name takes `_1`, then `_2`, ...
fn lowered_material_names(original: &Shape) -> Vec<Option<String>> {
    let mut taken: BTreeSet<String> = BTreeSet::new();
    (0..original.materials.len())
        .map(|i| {
            let base = lowered_name(&original.materials[i].name);
            let mut name = base.clone();
            let mut k = 1usize;
            while !taken.insert(name.clone()) {
                name = format!("{base}_{k}");
                k += 1;
            }
            Some(name)
        })
        .collect()
}

/// Whether glTF can hold any of the clip's motion at all, mirroring
/// `export_shape`'s skips (each noted there): additive clips have no word in
/// glTF; one-frame or zero-length clips hold none; and a clip whose tracks
/// name no node of the shape — or carry no data — writes no channels.
fn representable(shape: &Shape, a: &Animation) -> bool {
    if a.additive || a.frames <= 1 || a.duration < 1e-6 {
        return false;
    }
    let nodes: BTreeSet<String> = shape
        .nodes
        .iter()
        .map(|n| n.name.to_ascii_lowercase())
        .collect();
    a.nodes.iter().any(|t| {
        nodes.contains(&t.node.to_ascii_lowercase())
            && (!t.translations.is_empty() || !t.rotations.is_empty() || !t.scales.is_empty())
    })
}

/// Record a difference between two values of the round-tripped material.
fn same<T: std::fmt::Debug>(
    problems: &mut Vec<String>,
    label: &str,
    name: &str,
    field: &str,
    a: T,
    b: T,
) {
    if format!("{a:?}") != format!("{b:?}") {
        problems.push(format!(
            "{label}: material `{name}` {field} changed: {a:?} -> {b:?}"
        ));
    }
}

#[test]
fn materials_keep_their_declaration_order_and_fields() {
    let corpus = corpus();
    assert!(
        !corpus.is_empty(),
        "the shape corpus is missing; run from the repository"
    );
    let mut problems: Vec<String> = Vec::new();
    for path in &corpus {
        let label = path.display().to_string();
        let original = match load_shape(path) {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{label}: read failed: {e:#}"));
                continue;
            }
        };
        if !drawable(&original) {
            continue; // no .glb exists for it
        }
        let (lowered, _notes) = match export_and_lower(&original) {
            Ok(pair) => pair,
            Err(e) => {
                problems.push(format!("{label}: export/lower failed: {e:#}"));
                continue;
            }
        };
        // Every native material comes back: one per glTF material, positionally,
        // with no helper materials appended (a Blockland .glb carries none).
        if lowered.materials.len() != original.materials.len() {
            problems.push(format!(
                "{label}: material count changed: {} -> {}",
                original.materials.len(),
                lowered.materials.len()
            ));
        }
        // Each material comes back under its own name, a twin of it taking the
        // importer's `_1`, `_2`, ... suffix.
        let names = lowered_material_names(&original);
        for (i, base) in names.iter().enumerate() {
            let Some(base) = base else {
                continue;
            };
            let m = &original.materials[i];
            let Some(x) = lowered.materials.iter().find(|x| x.name == *base) else {
                problems.push(format!(
                    "{label}: material `{}` ({}: {:?}) did not come back",
                    m.name, i, m.blend
                ));
                continue;
            };
            same(
                &mut problems,
                &label,
                &m.name,
                "blend",
                m.blend.as_str(),
                x.blend.as_str(),
            );
            same(&mut problems, &label, &m.name, "wrap_u", m.wrap_u, x.wrap_u);
            same(&mut problems, &label, &m.name, "wrap_v", m.wrap_v, x.wrap_v);
            same(&mut problems, &label, &m.name, "unlit", m.unlit, x.unlit);
            same(
                &mut problems,
                &label,
                &m.name,
                "environment",
                m.environment,
                x.environment,
            );
            same(
                &mut problems,
                &label,
                &m.name,
                "mipmaps",
                m.mipmaps,
                x.mipmaps,
            );
            same(
                &mut problems,
                &label,
                &m.name,
                "detail_scale",
                m.detail_scale,
                x.detail_scale,
            );
            same(
                &mut problems,
                &label,
                &m.name,
                "reflectance",
                m.reflectance,
                x.reflectance,
            );
            same(
                &mut problems,
                &label,
                &m.name,
                "paint_overlay",
                m.paint_overlay.unwrap_or(false),
                x.paint_overlay.unwrap_or(false),
            );
            same(
                &mut problems,
                &label,
                &m.name,
                "double_sided",
                m.double_sided,
                x.double_sided,
            );
            if m.alpha_cutoff != x.alpha_cutoff {
                problems.push(format!(
                    "{label}: material `{}` alpha_cutoff changed: {:?} -> {:?}",
                    m.name, m.alpha_cutoff, x.alpha_cutoff
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} round-trip problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// The first two non-collision details, as `export_shape` exports them.
fn visible_details(shape: &Shape) -> Vec<usize> {
    shape
        .details
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.collision)
        .map(|(i, _)| i)
        .take(2)
        .collect()
}

/// The materials a shape's visible meshes reference in the order they first appear.
fn first_use_order(shape: &Shape) -> Vec<usize> {
    let mut seen = BTreeSet::new();
    for &di in visible_details(shape).iter() {
        let d = &shape.details[di];
        for oi in d.object_start..d.object_start + d.object_count {
            if oi >= shape.objects.len() {
                continue;
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
                    seen.insert(i);
                }
            }
        }
    }
    // Reordered by first appearance, not value.
    let mut order: Vec<usize> = Vec::new();
    for &di in visible_details(shape).iter() {
        let d = &shape.details[di];
        for oi in d.object_start..d.object_start + d.object_count {
            if oi >= shape.objects.len() {
                continue;
            }
            let o = &shape.objects[oi];
            let Some(&mi) = o.meshes.get(d.mesh_offset) else {
                continue;
            };
            let Some(m) = shape.meshes.get(mi).and_then(Option::as_ref) else {
                continue;
            };
            for p in &m.primitives {
                if let Some(i) = p.material
                    && !order.contains(&i)
                {
                    order.push(i);
                }
            }
        }
    }
    let in_first_use: BTreeSet<usize> = order.iter().copied().collect();
    assert!(in_first_use == seen, "sanity");
    order
}

#[test]
fn materials_come_back_in_declaration_order() {
    let corpus = corpus();
    assert!(
        !corpus.is_empty(),
        "the shape corpus is missing; run from the repository"
    );
    // The shapes whose meshes use their materials out of declaration order are
    // the ones a first-use ordering would draw wrong: the corpus must include
    // some, or this test proves nothing.
    let mut out_of_order = 0usize;
    for path in &corpus {
        let original: Shape = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let declared: Vec<usize> = (0..original.materials.len())
            .filter(|i| used_materials(&original).contains(&Some(*i)))
            .collect();
        if first_use_order(&original) != declared && !declared.is_empty() {
            out_of_order += 1;
        }
    }
    assert!(
        out_of_order >= 5,
        "expected the corpus to include shapes that use materials out of declaration order (found {out_of_order}); the ordering test would prove nothing"
    );

    let mut problems: Vec<String> = Vec::new();
    for path in &corpus {
        let label = path.display().to_string();
        let original = match load_shape(path) {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{label}: read failed: {e:#}"));
                continue;
            }
        };
        if !drawable(&original) {
            continue; // no .glb exists for it
        }
        let (lowered, _notes) = match export_and_lower(&original) {
            Ok(pair) => pair,
            Err(e) => {
                problems.push(format!("{label}: export/lower failed: {e:#}"));
                continue;
            }
        };
        // The used materials, in declaration order, must sit in the lowered
        // list in that same order (helper materials may sit between them).
        let names = lowered_material_names(&original);
        let mut positions: Vec<(usize, usize)> = Vec::new();
        for (i, base) in names.iter().enumerate() {
            let Some(base) = base else {
                continue;
            };
            match lowered.materials.iter().position(|x| x.name == *base) {
                Some(pos) => positions.push((i, pos)),
                None => problems.push(format!(
                    "{label}: material `{}` missing from the lowered list",
                    original.materials[i].name
                )),
            }
        }
        let declared_positions: Vec<usize> = (0..original.materials.len())
            .filter(|i| names[*i].is_some())
            .filter_map(|i| positions.iter().find(|(k, _)| *k == i).map(|(_, p)| *p))
            .collect();
        let mut sorted = declared_positions.clone();
        sorted.sort_unstable();
        if declared_positions != sorted {
            problems.push(format!(
                "{label}: lowered materials are not in declaration order: {declared_positions:?}"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{} round-trip problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// A mesh's vertices counted as the exporter gathers them: deduped once per
/// material group, so a vertex several materials share is written once each
/// (its triangles are untouched); triangles naming no vertex are dropped.
fn grouped_geometry(mesh: &Mesh) -> (usize, usize) {
    let v = mesh.frame_vertices.max(1);
    let mut groups: BTreeMap<Option<usize>, Vec<bool>> = BTreeMap::new();
    let mut triangles = 0usize;
    for prim in &mesh.primitives {
        let g = groups
            .entry(prim.material)
            .or_insert_with(|| vec![false; v]);
        for t in &prim.triangles {
            if !t.iter().all(|&x| (x as usize) < v) {
                continue;
            }
            triangles += 1;
            for &c in t {
                g[c as usize] = true;
            }
        }
    }
    let vertices: usize = groups
        .values()
        .map(|g| g.iter().filter(|&&u| u).count())
        .sum();
    (vertices, triangles)
}

/// The vertices and triangles the exporter writes for `shape` and the re-import
/// must read back: its first two visible details' meshes plus every collision
/// detail's (`col_` nodes come back as colliders with their geometry), each
/// reference counted once.
fn expected_geometry(shape: &Shape) -> (usize, usize) {
    let mut vertices = 0usize;
    let mut triangles = 0usize;
    let add = |di: usize, vertices: &mut usize, triangles: &mut usize| {
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
            let (v, t) = grouped_geometry(m);
            *vertices += v;
            *triangles += t;
        }
    };
    for &di in &visible_details(shape) {
        add(di, &mut vertices, &mut triangles);
    }
    for (di, d) in shape.details.iter().enumerate() {
        if d.collision {
            add(di, &mut vertices, &mut triangles);
        }
    }
    (vertices, triangles)
}

#[test]
fn geometry_totals_are_kept() {
    let corpus = corpus();
    assert!(
        !corpus.is_empty(),
        "the shape corpus is missing; run from the repository"
    );
    let mut problems: Vec<String> = Vec::new();
    for path in &corpus {
        let label = path.display().to_string();
        let original = match load_shape(path) {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{label}: read failed: {e:#}"));
                continue;
            }
        };
        if !drawable(&original) {
            continue; // no .glb exists for it
        }
        let (lowered, _notes) = match export_and_lower(&original) {
            Ok(pair) => pair,
            Err(e) => {
                problems.push(format!("{label}: export/lower failed: {e:#}"));
                continue;
            }
        };
        let (vertices, triangles) = expected_geometry(&original);
        let lowered_vertices = lowered
            .meshes
            .iter()
            .flatten()
            .map(|m| m.positions.len())
            .sum::<usize>();
        let lowered_triangles = lowered
            .meshes
            .iter()
            .flatten()
            .flat_map(|m| &m.primitives)
            .map(|p| p.triangles.len())
            .sum::<usize>();
        if vertices != lowered_vertices || triangles != lowered_triangles {
            problems.push(format!(
                "{label}: geometry changed: {vertices} verts/{triangles} tris -> {lowered_vertices}/{lowered_triangles}"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{} round-trip problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn node_names_are_kept() {
    let corpus = corpus();
    assert!(
        !corpus.is_empty(),
        "the shape corpus is missing; run from the repository"
    );
    let mut problems: Vec<String> = Vec::new();
    for path in &corpus {
        let label = path.display().to_string();
        let original = match load_shape(path) {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{label}: read failed: {e:#}"));
                continue;
            }
        };
        if !drawable(&original) {
            continue; // no .glb exists for it
        }
        let (lowered, _notes) = match export_and_lower(&original) {
            Ok(pair) => pair,
            Err(e) => {
                problems.push(format!("{label}: export/lower failed: {e:#}"));
                continue;
            }
        };
        // Node names pass through the round trip as-is (control characters
        // gone, at most 64), deduped case-insensitively with a `_N` suffix on
        // either side; the lowered shape may also hold extra mesh and collider
        // nodes. Every original name must be among them.
        let lowered_names: BTreeSet<String> = lowered
            .nodes
            .iter()
            .map(|n| n.name.to_ascii_lowercase())
            .collect();
        for n in &original.nodes {
            let base = clean(&n.name).to_ascii_lowercase();
            if base.is_empty() {
                continue; // the importer names such a node `nodeN` by position
            }
            if !lowered_names.contains(&base)
                && !lowered_names
                    .iter()
                    .any(|x| x.starts_with(&format!("{base}_")))
            {
                problems.push(format!("{label}: node `{}` did not come back", n.name));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} round-trip problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn clips_are_kept() {
    let corpus = corpus();
    assert!(
        !corpus.is_empty(),
        "the shape corpus is missing; run from the repository"
    );
    let mut with_clips = 0usize;
    let mut problems: Vec<String> = Vec::new();
    for path in &corpus {
        let label = path.display().to_string();
        let original = match load_shape(path) {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{label}: read failed: {e:#}"));
                continue;
            }
        };
        if !drawable(&original) {
            continue; // no .glb exists for it
        }
        let (lowered, _notes) = match export_and_lower(&original) {
            Ok(pair) => pair,
            Err(e) => {
                problems.push(format!("{label}: export/lower failed: {e:#}"));
                continue;
            }
        };
        if original
            .animations
            .iter()
            .all(|a| !representable(&original, a))
        {
            continue; // glTF holds none of its motion (the exporter notes it)
        }
        with_clips += 1;
        // The re-import stores a clip under the role its name matches (an
        // `idle` comes back as `root`) or its safe form, so compare by that
        // identity; two originals sharing one collapse into it on import and
        // the game binds them alike.
        let lowered_ids: BTreeSet<String> = lowered
            .animations
            .iter()
            .map(|a| bri_gltf::testing::clip_identity(&a.name))
            .collect();
        for a in &original.animations {
            if !representable(&original, a) {
                continue; // glTF holds none of its motion (the exporter notes it)
            }
            // The exporter names an unnamed clip `clip`; compare by that base.
            let base = clean(&a.name);
            let base: &str = if base.trim().is_empty() {
                "clip"
            } else {
                &base
            };
            if !lowered_ids.contains(&bri_gltf::testing::clip_identity(base)) {
                problems.push(format!("{label}: clip `{}` did not come back", a.name));
            }
        }
    }
    assert!(
        with_clips > 0,
        "the corpus has no clips; the test would prove nothing"
    );
    assert!(
        problems.is_empty(),
        "{} round-trip problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn texture_bytes_survive_the_round_trip() {
    // A fully opaque picture, encoded by the same library that re-encodes on
    // the way down: the lowered material's PNG must be byte-for-byte it.
    let pixels = vec![
        255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255, 10, 20, 30, 255, 40,
        50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
    ];
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(4, 2, pixels).unwrap())
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();

    let shape = bri_content::testing::rigid_shape(
        "tex_test",
        &[("root", None, [0.0; 3])],
        &[(
            0,
            [0.0, 0.0, 0.0],
            [0.1, 0.2, 0.3],
            bri_content::testing::plain(0),
        )],
        vec![bri_content::testing::material("tex", "opaque")],
    );
    let mut notes = Vec::new();
    let (glb, _embedded) = bri_gltf::export_shape(
        &shape,
        &BTreeMap::from([(0usize, Some(("tex.png".to_string(), png.clone())))]),
        &mut notes,
    )
    .unwrap();
    let doc = bri_gltf::Document::from_bytes(&glb, None).unwrap();
    let opts = bri_gltf::Options::new("", "", bri_gltf::Kind::Item);
    let lowered = bri_gltf::convert(&doc, None, &opts, "tex_test", "").unwrap();
    assert_eq!(lowered.textures.get("tex"), Some(&png));
}

/// The JSON chunk of a `.glb` (it is always the file's first chunk).
fn glb_json(bytes: &[u8]) -> serde_json::Value {
    assert!(bytes.starts_with(b"glTF"));
    let length = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    serde_json::from_slice(&bytes[20..20 + length]).unwrap()
}

/// A minimal `.glb`: one triangle, one material with `factor` and no picture.
fn factor_glb(factor: [f32; 4]) -> Vec<u8> {
    let mut g = bri_gltf::Gltf::new();
    let positions = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
    let mesh = g.mesh(
        "tri",
        vec![serde_json::json!({ "attributes": { "POSITION": positions } })],
    );
    g.material("solid", factor, None, serde_json::json!({}));
    g.node(serde_json::json!({ "mesh": mesh }), None);
    g.glb()
}

#[test]
fn a_non_white_factor_without_a_picture_is_the_materials_colour() {
    let bytes = factor_glb([0.25, 0.5, 0.75, 1.0]);
    let doc = bri_gltf::Document::from_bytes(&bytes, None).unwrap();
    let opts = bri_gltf::Options::new("", "", bri_gltf::Kind::Item);
    let lowered = bri_gltf::convert(&doc, None, &opts, "factor", "").unwrap();
    assert_eq!(lowered.shape.materials.len(), 1);
    assert_eq!(lowered.shape.materials[0].color, Some([0.25, 0.5, 0.75, 1.0]));
}

#[test]
fn the_default_white_factor_is_no_colour() {
    let bytes = factor_glb([1.0; 4]);
    let doc = bri_gltf::Document::from_bytes(&bytes, None).unwrap();
    let opts = bri_gltf::Options::new("", "", bri_gltf::Kind::Item);
    let lowered = bri_gltf::convert(&doc, None, &opts, "factor_white", "").unwrap();
    assert_eq!(lowered.shape.materials[0].color, None);
}

#[test]
fn a_solid_colour_survives_the_round_trip_without_a_picture() {
    let mut solid = bri_content::testing::material("solid", "opaque");
    solid.color = Some([0.25, 0.5, 0.75, 1.0]);
    let shape = bri_content::testing::rigid_shape(
        "colour_test",
        &[("root", None, [0.0; 3])],
        &[(
            0,
            [0.0, 0.0, 0.0],
            [0.1, 0.2, 0.3],
            bri_content::testing::Paint { front: 1, rest: 0 },
        )],
        vec![bri_content::testing::material("plain", "opaque"), solid],
    );
    let mut notes = Vec::new();
    let (glb, _embedded) = bri_gltf::export_shape(&shape, &BTreeMap::new(), &mut notes).unwrap();

    // The colour rides in the factor alone: no picture in the file.
    let json = glb_json(&glb);
    assert!(json.get("images").and_then(Value::as_array).map_or(true, Vec::is_empty));
    let solid_mat = json["materials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "solid")
        .unwrap();
    assert_eq!(
        solid_mat["pbrMetallicRoughness"]["baseColorFactor"],
        serde_json::json!([0.25, 0.5, 0.75, 1.0])
    );

    // And it comes back as the material's colour.
    let doc = bri_gltf::Document::from_bytes(&glb, None).unwrap();
    let opts = bri_gltf::Options::new("", "", bri_gltf::Kind::Item);
    let lowered = bri_gltf::convert(&doc, None, &opts, "colour_test", "").unwrap();
    assert_eq!(lowered.shape.materials.len(), 2);
    assert_eq!(lowered.shape.materials[0].color, None);
    assert_eq!(lowered.shape.materials[1].color, Some([0.25, 0.5, 0.75, 1.0]));
}
