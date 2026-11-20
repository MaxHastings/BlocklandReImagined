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

/// A file whose `bri_legacy.color` carries the functional colour of an opaque
/// solid material takes it over its solid baseColorFactor on import.
#[test]
fn bri_legacy_colour_wins_over_the_solid_factor() {
    let mut g = bri_gltf::Gltf::new();
    let positions = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
    let mesh = g.mesh(
        "tri",
        vec![serde_json::json!({ "attributes": { "POSITION": positions } })],
    );
    g.material(
        "blank",
        [1.0, 1.0, 1.0, 1.0],
        None,
        serde_json::json!({ "extras": { "bri_legacy": { "color": [1.0, 1.0, 1.0, 0.0] } } }),
    );
    g.node(serde_json::json!({ "mesh": mesh }), None);

    let doc = bri_gltf::Document::from_bytes(&g.glb(), None).unwrap();
    let opts = bri_gltf::Options::new("", "", bri_gltf::Kind::Item);
    let lowered = bri_gltf::convert(&doc, None, &opts, "legacy_colour", "").unwrap();
    assert_eq!(lowered.shape.materials[0].color, Some([1.0, 1.0, 1.0, 0.0]));
}

/// An opaque solid colour whose alpha carries a paint shift exports its factor
/// with full alpha (solid in editors) and keeps the functional colour in
/// `bri_legacy.color`; lowering restores it exactly.
#[test]
fn a_shifted_solid_colour_keeps_its_alpha_in_bri_legacy() {
    let mut shifted = bri_content::testing::material("black50", "opaque");
    shifted.color = Some([0.0, 0.0, 0.0, 0.5]);
    let shape = bri_content::testing::rigid_shape(
        "shift_test",
        &[("root", None, [0.0; 3])],
        &[(
            0,
            [0.0, 0.0, 0.0],
            [0.1, 0.2, 0.3],
            bri_content::testing::Paint { front: 1, rest: 0 },
        )],
        vec![bri_content::testing::material("plain", "opaque"), shifted],
    );
    let mut notes = Vec::new();
    let (glb, _embedded) = bri_gltf::export_shape(&shape, &BTreeMap::new(), &mut notes).unwrap();

    // Editors see the part solid: full alpha in the factor...
    let json = glb_json(&glb);
    let mat = json["materials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "black50")
        .unwrap();
    assert_eq!(
        mat["pbrMetallicRoughness"]["baseColorFactor"],
        serde_json::json!([0.0, 0.0, 0.0, 1.0])
    );
    // ...while the functional colour rides in bri_legacy.color.
    assert_eq!(
        mat["extras"]["bri_legacy"]["color"],
        serde_json::json!([0.0, 0.0, 0.0, 0.5])
    );

    // And a re-import restores it exactly.
    let doc = bri_gltf::Document::from_bytes(&glb, None).unwrap();
    let opts = bri_gltf::Options::new("", "", bri_gltf::Kind::Item);
    let lowered = bri_gltf::convert(&doc, None, &opts, "shift_test", "").unwrap();
    assert_eq!(lowered.shape.materials[1].color, Some([0.0, 0.0, 0.0, 0.5]));
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

/// A posed model's paint mask (glTF `COLOR_0`) rides into the shape: quantized to bytes, a colourless sibling primitive takes white (no
/// modulation).
#[test]
fn vertex_colours_are_a_paint_mask_for_posed_models() {
    let mut g = bri_gltf::Gltf::new();
    // Each primitive keeps its own positions: the mask must match them one for one.
    let masked = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 3);
    // VEC3: no alpha channel in the file, so it defaults to opaque.
    let mask = g.floats(&[1.0, 1.0, 1.0, 0.5, 0.5, 0.5, 0.0, 0.0, 0.0], 3);
    let bare = g.floats(&[2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 2.0, 1.0, 0.0], 3);
    let mesh = g.mesh(
        "mask",
        vec![
            serde_json::json!({ "attributes": { "POSITION": masked, "COLOR_0": mask } }),
            serde_json::json!({ "attributes": { "POSITION": bare } }),
        ],
    );
    g.material("blank", [1.0; 4], None, serde_json::json!({}));
    g.node(serde_json::json!({ "mesh": mesh }), None);
    let bytes = g.glb();

    let doc = bri_gltf::Document::from_bytes(&bytes, None).unwrap();
    let opts = bri_gltf::Options::new("", "", bri_gltf::Kind::Item);
    let lowered = bri_gltf::convert(&doc, None, &opts, "mask", "").unwrap();
    let mesh = lowered.shape.meshes[0].as_ref().unwrap();

    // Quantized to bytes: white stays full-bright, half dims to 128.
    assert_eq!(&mesh.colors[..3], &[[255u8; 4], [128, 128, 128, 255], [0, 0, 0, 255]]);
    // The colourless primitive takes white: no modulation.
    assert_eq!(&mesh.colors[3..], &[[255u8; 4]; 3]);
}

/// A clip this crate's exporter wrote carries its full Blockland semantics in
/// `extras.bri_clip`; lowering such a file must restore every field exactly —
/// name, length, layering, triggers and per-frame motion (bit-exact: exported
/// shapes carry unit node scales, so the re-import's scale bookkeeping is an
/// identity). Covers the three shapes of clip the exporter now keeps: an
/// additive overlay with custom priority and a non-default interpolation, an
/// absolute clip whose length no fps-30 guess would find (triggers and object
/// frames included), and an empty placeholder.
#[test]
fn exporter_written_clips_round_trip_field_for_field() {
    use bri_content::shape::{ObjectTrack, RotationInterpolation, Trigger};

    let mut shape = bri_content::testing::rigid_shape(
        "clip_test",
        &[("root", None, [0.0; 3]), ("arm", Some(0), [0.5, 0.8, 0.1])],
        &[(
            0,
            [0.0, 0.0, 0.0],
            [0.1, 0.2, 0.3],
            bri_content::testing::plain(0),
        )],
        vec![bri_content::testing::material("tex", "opaque")],
    );

    // An additive overlay: non-looping, custom priority and parent-frame
    // translations, a non-default rotation interpolation, four frames of
    // motion on `arm`.
    let additive = bri_content::shape::Animation {
        name: "additive_layer".into(),
        frames: 4,
        duration: 0.8,
        looping: false,
        additive: true,
        parent_translations: true,
        priority: 64,
        rotation_interpolation: RotationInterpolation::NormalizedLinear,
        nodes: vec![bri_content::shape::NodeTrack {
            node: "arm".into(),
            rotations: vec![[0.0, 0.0, 0.0, 1.0]; 4],
            translations: vec![
                [0.1, 0.2, 0.3],
                [0.4, -0.5, 0.6],
                [-0.7, 0.8, -0.9],
                [0.0, 0.0, 0.0],
            ],
            scales: vec![[1.0; 3], [1.5, 1.0, 1.0], [1.0, 2.0, 1.0], [0.5, 0.5, 0.5]],
            scale_rotations: Vec::new(),
        }],
        objects: Vec::new(),
        ground_translations: Vec::new(),
        ground_rotations: Vec::new(),
        triggers: Vec::new(),
    };

    // An absolute clip: odd frame count, a length no fps-30 derivation would
    // find, two triggers and an object's material-frame track.
    let triggered = bri_content::shape::Animation {
        name: "triggered_walk".into(),
        frames: 3,
        duration: 0.5,
        looping: true,
        additive: false,
        parent_translations: false,
        priority: 7,
        rotation_interpolation: RotationInterpolation::Spherical,
        nodes: vec![bri_content::shape::NodeTrack {
            node: "root".into(),
            rotations: Vec::new(),
            translations: vec![[0.1, 0.0, 0.2], [0.3, 0.4, 0.5], [0.6, -0.7, 0.8]],
            scales: Vec::new(),
            scale_rotations: Vec::new(),
        }],
        objects: vec![ObjectTrack {
            object: 0,
            visibility: vec![1.0, 0.5, 0.0],
            frames: vec![0, 1, 2],
            material_frames: vec![2, 7, 9],
        }],
        ground_translations: Vec::new(),
        ground_rotations: Vec::new(),
        triggers: vec![
            Trigger { state: 2_147_483_650, position: 0.6315779 },
            Trigger { state: 42, position: 0.1 },
        ],
    };

    // An empty placeholder clip (zero tracks), as `land` and `root` ride in a
    // real rig.
    let empty = bri_content::shape::Animation {
        name: "empty_placeholder".into(),
        frames: 2,
        duration: 0.0667,
        looping: false,
        additive: true,
        parent_translations: false,
        priority: 128,
        rotation_interpolation: RotationInterpolation::Spherical,
        nodes: Vec::new(),
        objects: Vec::new(),
        ground_translations: Vec::new(),
        ground_rotations: Vec::new(),
        triggers: Vec::new(),
    };

    shape.animations.extend([additive.clone(), triggered.clone(), empty.clone()]);
    let (lowered, _notes) = export_and_lower(&shape).unwrap();
    assert_eq!(lowered.animations.len(), 3);

    // The scalar semantics come back field for field.
    for (orig, low) in [additive, triggered, empty].into_iter().zip(lowered.animations.iter()) {
        assert_eq!(low.name, orig.name);
        assert_eq!(low.frames, orig.frames);
        assert_eq!(low.duration.to_bits(), orig.duration.to_bits());
        assert_eq!(low.looping, orig.looping);
        assert_eq!(low.additive, orig.additive);
        assert_eq!(low.priority, orig.priority);
        assert_eq!(low.parent_translations, orig.parent_translations);
        assert_eq!(low.rotation_interpolation, orig.rotation_interpolation);
        assert_eq!(low.ground_translations.len(), orig.ground_translations.len());
        assert_eq!(low.ground_rotations.len(), orig.ground_rotations.len());
        assert_eq!(low.triggers.len(), orig.triggers.len());
        for (o, l) in orig.triggers.iter().zip(low.triggers.iter()) {
            assert_eq!(l.state, o.state);
            assert_eq!(l.position.to_bits(), o.position.to_bits());
        }
    }

    // The additive overlay's per-frame motion comes back bit-exact, scales raw.
    let a = &lowered.animations[0];
    let track = a.nodes.iter().find(|t| t.node == "arm").expect("the arm's track");
    assert_eq!(track.translations.len(), 4);
    for (i, v) in [
        [0.1f32, 0.2, 0.3],
        [0.4, -0.5, 0.6],
        [-0.7, 0.8, -0.9],
        [0.0, 0.0, 0.0],
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(track.translations[i].to_vec().into_iter().map(f32::to_bits).collect::<Vec<_>>(), v.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
    }
    assert_eq!(track.rotations, vec![[0.0f32, 0.0, 0.0, 1.0]; 4]);
    for (i, want) in [[1.0f32; 3], [1.5, 1.0, 1.0], [1.0, 2.0, 1.0], [0.5, 0.5, 0.5]]
        .iter()
        .enumerate()
    {
        assert_eq!(track.scales[i].to_vec().into_iter().map(f32::to_bits).collect::<Vec<_>>(), want.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
    }

    // The absolute clip's translation-only track keeps only that path.
    let b = &lowered.animations[1];
    let track = b.nodes.iter().find(|t| t.node == "root").expect("the root's track");
    assert_eq!(track.translations.len(), 3);
    for (i, want) in [[0.1f32, 0.0, 0.2], [0.3, 0.4, 0.5], [0.6, -0.7, 0.8]]
        .iter()
        .enumerate()
    {
        assert_eq!(track.translations[i].to_vec().into_iter().map(f32::to_bits).collect::<Vec<_>>(), want.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
    }
    assert!(track.rotations.is_empty());
    assert!(track.scales.is_empty());
    // The object's per-frame tracks ride along.
    assert_eq!(b.objects.len(), 1);
    assert_eq!(b.objects[0].object, 0);
    assert_eq!(b.objects[0].visibility, vec![1.0f32, 0.5, 0.0]);
    assert_eq!(b.objects[0].frames, vec![0u32, 1, 2]);
    assert_eq!(b.objects[0].material_frames, vec![2u32, 7, 9]);

    // The placeholder clip rides with no tracks at all.
    assert!(lowered.animations[2].nodes.is_empty());
}

/// A see-through picture on an otherwise-opaque material earns alphaMode BLEND
/// in the .glb so editors show it transparent the way the game does; a fully-
/// opaque RGBA picture (the common stock case) does not, and the game's own
/// blend is kept intact in bri_legacy either way.
#[test]
fn a_seethrough_picture_on_an_opaque_material_earns_blend() {
    use image::{ImageBuffer, Rgba};

    let png = |alpha: u8| -> Vec<u8> {
        let img = ImageBuffer::from_fn(2, 2, |_, _| Rgba([10u8, 20, 30, alpha]));
        let mut b = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut b), image::ImageFormat::Png)
            .unwrap();
        b
    };

    // The glTF JSON chunk of an exported .glb.
    let json_of = |glb: &[u8]| -> Value {
        assert_eq!(&glb[..4], b"glTF");
        let mut off = 12;
        while off < glb.len() {
            let len = u32::from_le_bytes(glb[off..off + 4].try_into().unwrap()) as usize;
            let typ = u32::from_le_bytes(glb[off + 4..off + 8].try_into().unwrap());
            if typ == 0x4E4F534A {
                return serde_json::from_slice(&glb[off + 8..off + 8 + len]).unwrap();
            }
            off += 8 + len;
        }
        panic!("no JSON chunk");
    };

    // A fixture with a used opaque material and a visible mesh; skip where the
    // corpus is absent, like the other round-trip tests.
    let Some((shape, mi)) = corpus()
        .iter()
        .filter_map(|p| load_shape(p).ok())
        .find_map(|s| {
            for m in used_materials(&s).into_iter() {
                if let Some(i) = m {
                    if i < s.materials.len() && s.materials[i].blend.as_str() == "opaque" {
                        return Some((s, i));
                    }
                }
            }
            None
        })
    else {
        return;
    };

    // A transparent picture earns BLEND ...
    let mut tex = BTreeMap::new();
    tex.insert(mi, Some((format!("mat{mi}.png"), png(128))));
    let (glb, _) = bri_gltf::export_shape(&shape, &tex, &mut Vec::new()).unwrap();
    let jm = json_of(&glb);
    assert_eq!(jm["materials"][mi]["alphaMode"].as_str(), Some("BLEND"));
    // ... but the game's own blend is untouched.
    assert_eq!(
        jm["materials"][mi]["extras"]["bri_legacy"]["blend"].as_str(),
        Some("opaque")
    );

    // A fully-opaque RGBA picture does not (the common stock case).
    let mut tex = BTreeMap::new();
    tex.insert(mi, Some((format!("mat{mi}.png"), png(255))));
    let (glb, _) = bri_gltf::export_shape(&shape, &tex, &mut Vec::new()).unwrap();
    assert!(json_of(&glb)["materials"][mi].get("alphaMode").is_none());
}
