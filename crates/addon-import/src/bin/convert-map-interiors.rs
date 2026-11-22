//! Temporary in-place content migrator (throwaway tool).
//!
//! Converts every native model file named by a map bundle's `assets` — the
//! interior files (`*.interior.json`) and the static shape files
//! (`*.shape.json`) — to one self-contained `.glb`: geometry, diffuse
//! textures embedded beside each other, authored collision as position-only
//! primitives (a shape's collider block masks ride in extras) — rewrites the
//! bundle so the assets point at the `.glb`, and deletes the old JSON once
//! the new file reads back identically. While it is in a package it also
//! gives the hash-named baked lightmaps structural names
//! (`{scene}-node{N}-slot{S}.png`) from their lighting records, which carry
//! full provenance (scene id, node index and slot).
//!
//! Terrain files stay JSON on purpose: they are procedural data the runtime
//! reads as such. Texture files are never deleted — terrain bindings still
//! read them; only an interior's own JSON goes away.
//!
//! Usage: `convert-map-interiors CONTENT_ROOT [--apply]`
//!
//! Dry-run by default: computes and reports every change without touching any
//! file (except writing INTERIOR-MIGRATION-REPORT.json into the root). Pass
//! `--apply` to write the `.glb`s, rename the lightmaps, rewrite the bundles
//! and delete the old JSONs. A model that cannot ride in one `.glb` (a drawn
//! material resolves to no texture, or a shape's skins bind different
//! pictures to one material slot) is left as JSON with its reason in the
//! report.
//!
//! TEMPORARY TOOL: once all content is converted, delete this file and its
//! `[[bin]]` entry from Cargo.toml.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail, ensure};
use bri_content::interior::{Detail, Interior};
use bri_content::shape::Shape;
use serde_json::{Value, json};

const MAX_TEXTURE: u64 = 64 * 1024 * 1024;

struct Args {
    root: PathBuf,
    apply: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(args) = parse_args(&args) else {
        eprintln!("usage: convert-map-interiors CONTENT_ROOT [--apply]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if report.failed > 0 {
                println!(
                    "INTERIOR-MIGRATION-REPORT: {} failure(s); see INTERIOR-MIGRATION-REPORT.json. Their JSON files were left in place.",
                    report.failed
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("convert-map-interiors: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Result<Args> {
    let mut root = None;
    let mut apply = false;
    for a in args {
        match a.as_str() {
            "--apply" => apply = true,
            other if !other.starts_with('-') => {
                ensure!(root.is_none(), "unexpected argument: {other}");
                root = Some(PathBuf::from(other));
            }
            other => bail!("unknown flag: {other}"),
        }
    }
    let Some(root) = root else {
        bail!("missing CONTENT_ROOT")
    };
    let root = root.canonicalize().context("content root does not exist")?;
    Ok(Args { root, apply })
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                // Staged downloads are not installed content.
                if p.file_name().and_then(|n| n.to_str()) == Some(".downloads") {
                    continue;
                }
                stack.push(p);
            } else if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}

/// A 64-hex-character file stem: how the legacy pipeline named baked files.
fn is_hash_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".png") else {
        return false;
    };
    stem.len() == 64 && stem.chars().all(|c| c.is_ascii_hexdigit())
}

/// `v20/add-ons/map_bedroom/bedroom.mis` -> `v20-add-ons-map_bedroom-bedroom`.
fn structural_scene(key: &str) -> String {
    let key = key.strip_suffix(".mis").unwrap_or(key);
    key.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' })
        .collect()
}

struct InteriorPlan {
    asset: String,
    file: String,
    glb: String,
    surfaces: usize,
    triangles: usize,
    lightmaps: usize,
    embedded_textures: Vec<String>,
    notes: Vec<String>,
}

struct ShapePlan {
    asset: String,
    file: String,
    glb: String,
    materials: usize,
    visible_triangles: usize,
    collision_triangles: usize,
    embedded_textures: Vec<String>,
    notes: Vec<String>,
}

struct PackageReport {
    dir: String,
    interiors: Vec<InteriorPlan>,
    shapes: Vec<ShapePlan>,
    renamed_lightmaps: usize,
    copied_lightmaps: usize,
    failures: Vec<(String, String)>,
}

struct Report {
    mode: &'static str,
    root: String,
    packages: Vec<PackageReport>,
    failed: usize,
    converted: usize,
    shapes: usize,
    renamed: usize,
    copied: usize,
}

impl Report {
    fn summary_line(&self) -> String {
        format!(
            "{}: {} interior(s) + {} shape(s) to .glb, {} baked lightmap(s) renamed ({} copied), {} failure(s)",
            self.mode, self.converted, self.shapes, self.renamed, self.copied, self.failed
        )
    }
}

fn run(args: &Args) -> Result<Report> {
    let root = &args.root;
    let apply = args.apply;

    // --- Discover the map bundles -----------------------------------------
    let all_files = walk(root);
    let bundle_files: Vec<&PathBuf> = all_files
        .iter()
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some("bundle.json"))
        .collect();

    let mut report = Report {
        mode: if apply { "apply" } else { "dry-run" },
        root: root.display().to_string(),
        packages: Vec::new(),
        failed: 0,
        converted: 0,
        shapes: 0,
        renamed: 0,
        copied: 0,
    };

    for bundle_file in &bundle_files {
        let dir = bundle_file.parent().context("bundle.json has no parent")?;
        let mut bundle: Value = serde_json::from_slice(&fs::read(bundle_file).with_context(|| {
            format!("{} does not read", bundle_file.display())
        })?)
        .with_context(|| format!("{} is not JSON", bundle_file.display()))?;

        let assets: serde_json::Map<String, Value> = bundle
            .get("assets")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let bindings: Vec<Value> = bundle
            .get("bindings")
            .and_then(Value::as_array)
            .map(|a| a.clone())
            .unwrap_or_default();

        // --- The interior files ---------------------------------------------
        let mut interiors = Vec::new();
        let mut failures: Vec<(String, String)> = Vec::new();
        for (asset, file) in &assets {
            let Some(file) = file.as_str() else { continue };
            if !file.ends_with(".interior.json") {
                continue; // Already converted, or not an interior.
            }
            match convert_interior(dir, asset, file, &bindings, apply) {
                Ok(plan) => interiors.push(plan),
                Err(error) => failures.push((asset.clone(), format!("{error:#}"))),
            }
        }

        // --- The static shape files -------------------------------------------
        let mut shapes = Vec::new();
        for (asset, file) in &assets {
            let Some(file) = file.as_str() else { continue };
            if !file.ends_with(".shape.json") {
                continue; // Already converted, or not a shape.
            }
            match convert_shape(dir, asset, file, &bindings, apply) {
                Ok(plan) => shapes.push(plan),
                Err(error) => failures.push((asset.clone(), format!("{error:#}"))),
            }
        }

        // --- The hash-named baked lightmaps -----------------------------------
        // The bundle values are updated now; the disk moves wait for the
        // commit below, so a failed package leaves its files where they were.
        let mut renames: Vec<(String, String)> = Vec::new();
        let mut copies: Vec<(String, String)> = Vec::new();
        // A bake shared by several records is renamed once and copied for the
        // rest; renaming the same source twice would lose it.
        let mut claimed: BTreeSet<String> = BTreeSet::new();
        if let Some(lighting) = bundle.get_mut("lighting").and_then(Value::as_object_mut) {
            for (scene, value) in lighting.iter_mut() {
                let Some(records) = value.get_mut("interiors").and_then(Value::as_array_mut) else {
                    continue;
                };
                let stem = structural_scene(scene);
                for rec in records {
                    let Some(file) = rec
                        .get("file")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    if !is_hash_name(&file) {
                        continue; // Already structural (or not a hash).
                    }
                    let node = rec["node"].as_u64().unwrap_or(0);
                    let slot = rec["slot"].as_u64().unwrap_or(0);
                    let new = format!("{stem}-node{node}-slot{slot}.png");
                    if dir.join(&new).exists() {
                        failures.push((
                            scene.clone(),
                            format!("baked lightmap rename target {new} already exists"),
                        ));
                        continue;
                    }
                    *rec.get_mut("file").unwrap() = json!(new);
                    if claimed.insert(file.clone()) {
                        renames.push((file, new));
                    } else {
                        copies.push((file, new));
                    }
                }
            }
        }

        // --- Commit the package ------------------------------------------------
        if apply && (!interiors.is_empty() || !shapes.is_empty() || !renames.is_empty() || !copies.is_empty()) {
            // Copies first: a shared bake is copied out before its one rename
            // moves the source away.
            for (old, new) in &copies {
                fs::copy(dir.join(old), dir.join(new)).with_context(|| {
                    format!("cannot copy {} to {}", old, new)
                })?;
            }
            for (old, new) in &renames {
                fs::rename(dir.join(old), dir.join(new)).with_context(|| {
                    format!("cannot rename {} to {}", old, new)
                })?;
            }
            let converted_interiors: BTreeSet<&str> = interiors.iter().map(|p| p.asset.as_str()).collect();
            let converted_shapes: BTreeSet<&str> = shapes.iter().map(|p| p.asset.as_str()).collect();
            for (asset, file) in &assets {
                if let Some(f) = file.as_str() {
                    let done = (f.ends_with(".interior.json") && converted_interiors.contains(asset.as_str()))
                        || (f.ends_with(".shape.json") && converted_shapes.contains(asset.as_str()));
                    if done {
                        bundle["assets"][asset.as_str()] = json!(f.trim_end_matches(".json").to_string() + ".glb");
                        fs::remove_file(dir.join(f)).with_context(|| format!("cannot delete {f}"))?;
                    }
                }
            }
            let text = serde_json::to_vec_pretty(&bundle)?;
            fs::write(bundle_file, text)
                .with_context(|| format!("cannot rewrite {}", bundle_file.display()))?;
        }

        report.converted += interiors.len();
        report.shapes += shapes.len();
        report.renamed += renames.len();
        report.copied += copies.len();
        report.failed += failures.len();
        report.packages.push(PackageReport {
            dir: dir.strip_prefix(root).unwrap_or(dir).display().to_string(),
            interiors,
            shapes,
            renamed_lightmaps: renames.len(),
            copied_lightmaps: copies.len(),
            failures,
        });
    }

    let text = serde_json::to_vec_pretty(&json!({
        "mode": report.mode,
        "root": report.root,
        "packages": report.packages.iter().map(|p| json!({
            "dir": p.dir,
            "interiors": p.interiors.iter().map(|i| json!({
                "asset": i.asset,
                "file": i.file,
                "glb": i.glb,
                "surfaces": i.surfaces,
                "triangles": i.triangles,
                "lightmaps": i.lightmaps,
                "embedded_textures": i.embedded_textures,
                "notes": i.notes,
            })).collect::<Vec<_>>(),
            "shapes": p.shapes.iter().map(|s| json!({
                "asset": s.asset,
                "file": s.file,
                "glb": s.glb,
                "materials": s.materials,
                "visible_triangles": s.visible_triangles,
                "collision_triangles": s.collision_triangles,
                "embedded_textures": s.embedded_textures,
                "notes": s.notes,
            })).collect::<Vec<_>>(),
            "renamed_lightmaps": p.renamed_lightmaps,
            "copied_lightmaps": p.copied_lightmaps,
            "failures": p.failures.iter().map(|(a, e)| json!({"asset": a, "error": e})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))?;
    fs::write(root.join("INTERIOR-MIGRATION-REPORT.json"), text)?;
    Ok(report)
}

/// One interior JSON to its `.glb`: resolve the textures its surfaces draw,
/// export, read the result back exactly as the client will, and — when
/// applying — write the `.glb` beside its JSON.
fn convert_interior(
    dir: &Path,
    asset: &str,
    file: &str,
    bindings: &[Value],
    apply: bool,
) -> Result<InteriorPlan> {
    let interior: Interior = serde_json::from_slice(&fs::read(dir.join(file)).with_context(|| {
        format!("{} does not read", dir.join(file).display())
    })?)?;
    interior.validate()?;

    // Detail d, material j -> the texture file its binding names. Only
    // materials a non-empty surface actually draws are resolved; the rest may
    // be absent without consequence.
    let resolve = |d: usize, j: usize| -> Result<Option<Vec<u8>>> {
        let Some(b) = bindings.iter().find(|b| {
            b["asset"].as_str() == Some(asset)
                && b["detail"].as_u64() == Some(d as u64)
                && b["material"].as_u64() == Some(j as u64)
        }) else {
            return Ok(None);
        };
        let name = b["texture"]
            .as_str()
            .context("interior material binding has no texture file")?;
        if name.is_empty() {
            return Ok(None); // A cleared binding: the game drew it untextured.
        }
        let path = dir.join(name);
        let bytes = fs::read(&path)
            .with_context(|| format!("texture {} does not read", path.display()))?;
        ensure!(
            (bytes.len() as u64) <= MAX_TEXTURE,
            "texture {name} is larger than 64 MB"
        );
        Ok(Some(bytes))
    };

    let total = interior.details.len() + interior.subobjects.len();
    let mut rows: Vec<Vec<Option<(String, Vec<u8>)>>> = Vec::with_capacity(total);
    for d in 0..total {
        let det = if d < interior.details.len() {
            &interior.details[d]
        } else {
            &interior.subobjects[d - interior.details.len()]
        };

        // Which materials does this detail actually draw?
        let mut drawn = vec![false; det.materials.len()];
        for sf in &det.surfaces {
            if !sf.triangles.is_empty() && sf.material < drawn.len() {
                drawn[sf.material] = true;
            }
        }

        // A surface the game would draw must have its texture; untextured is a
        // hard error here rather than a silent white wall in-game.
        let mut row = Vec::with_capacity(det.materials.len());
        for (j, name) in det.materials.iter().enumerate() {
            if !drawn[j] {
                row.push(None);
                continue;
            }
            let bytes = resolve(d, j)?
                .ok_or_else(|| anyhow::anyhow!(
                    "material {name} of detail {d} is drawn by a surface but resolves to no texture"
                ))?;
            // Carry the file name through so the report can say what rides inside.
            let fname = bindings
                .iter()
                .find(|b| {
                    b["asset"].as_str() == Some(asset)
                        && b["detail"].as_u64() == Some(d as u64)
                        && b["material"].as_u64() == Some(j as u64)
                })
                .and_then(|b| b["texture"].as_str())
                .unwrap_or_default();
            row.push(Some((fname.to_owned(), bytes)));
        }
        rows.push(row);
    }

    let mut notes = Vec::new();
    let (glb, embedded) = bri_gltf::interior_export::export(&interior, &rows, &mut notes)?;
    let glb_name = file.trim_end_matches(".json").to_string() + ".glb";

    // Read the bytes back exactly as the client will, and compare what must
    // survive. Only then may the old JSON go away.
    let doc = bri_gltf::Document::from_bytes(&glb, None)
        .with_context(|| "the exported interior glTF does not read back")?;
    let back = bri_gltf::interior_glb::InteriorGlb::from_document(&doc)?;
    ensure!(back.interior.id == interior.id, "id did not survive");
    for (a, b) in interior.details.iter().zip(&back.interior.details) {
        check_detail(a, b)?;
    }
    for (a, b) in interior.subobjects.iter().zip(&back.interior.subobjects) {
        check_detail(a, b)?;
    }

    if apply {
        fs::write(dir.join(&glb_name), &glb).with_context(|| {
            format!("cannot write {}", dir.join(&glb_name).display())
        })?;
    }

    let surfaces: usize = interior
        .details
        .iter()
        .chain(&interior.subobjects)
        .map(|d| d.surfaces.len())
        .sum();
    let triangles: usize = interior
        .details
        .iter()
        .chain(&interior.subobjects)
        .flat_map(|d| d.surfaces.iter().filter(|s| !s.triangles.is_empty()))
        .map(|s| s.triangles.len())
        .sum();
    let lightmaps: usize = interior
        .details
        .iter()
        .chain(&interior.subobjects)
        .map(|d| d.lightmaps.len())
        .sum();

    Ok(InteriorPlan {
        asset: asset.to_owned(),
        file: file.to_owned(),
        glb: glb_name,
        surfaces,
        triangles,
        lightmaps,
        embedded_textures: embedded.into_iter().collect(),
        notes,
    })
}

fn check_detail(a: &Detail, b: &Detail) -> Result<()> {
    ensure!(a.materials == b.materials, "material list changed");
    ensure!(a.surfaces.len() == b.surfaces.len(), "surface count changed");
    for (sa, sb) in a.surfaces.iter().zip(&b.surfaces) {
        ensure!(sa.triangles.len() == sb.triangles.len(), "triangle count changed");
        if !sa.triangles.is_empty() {
            ensure!(
                sa.vertices.len() == sb.vertices.len() && sa.material == sb.material,
                "surface geometry did not survive"
            );
        }
    }
    ensure!(a.lightmaps.len() == b.lightmaps.len(), "lightmap count changed");
    for (la, lb) in a.lightmaps.iter().zip(&b.lightmaps) {
        ensure!(la.png == lb.png && la.keep == lb.keep, "lightmap bytes changed");
        ensure!(
            la.auxiliary_png.as_deref() == lb.auxiliary_png.as_deref(),
            "lightmap auxiliary did not survive"
        );
    }
    ensure!(
        a.collision_triangles.len() == b.collision_triangles.len(),
        "collision triangle count changed"
    );
    ensure!(
        a.minimum_pixels == b.minimum_pixels
            && a.ambient == b.ambient
            && a.alarm_ambient == b.alarm_ambient
            && a.has_alarm == b.has_alarm,
        "detail lighting did not survive"
    );
    Ok(())
}

/// One static shape JSON to its `.glb`: resolve the texture each material
/// slot draws (one per slot across every skin the bundle binds), export, and
/// read the result back exactly as the client will — the material slots must
/// survive positionally (the bundle binds them by index) and every collider
/// triangle the physics builds today must still be there. When applying, the
/// `.glb` is written beside its JSON.
fn convert_shape(
    dir: &Path,
    asset: &str,
    file: &str,
    bindings: &[Value],
    apply: bool,
) -> Result<ShapePlan> {
    let shape: Shape = serde_json::from_slice(&fs::read(dir.join(file)).with_context(|| {
        format!("{} does not read", dir.join(file).display())
    })?)?;
    shape.validate()?;

    // The game binds each material slot to exactly one texture per skin a
    // placement uses; a `.glb` carries one picture per slot, so every skin of
    // a slot must name the same file or the shape keeps its JSON.
    let mut textures: BTreeMap<usize, Option<(String, Vec<u8>)>> = BTreeMap::new();
    for (slot, _material) in shape.materials.iter().enumerate() {
        let files: BTreeSet<String> = bindings
            .iter()
            .filter(|b| {
                b["asset"].as_str() == Some(asset)
                    && b["shape_material"].as_u64() == Some(slot as u64)
            })
            .filter_map(|b| b["texture"].as_str())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        match files.iter().next() {
            None => {
                textures.insert(slot, None);
            }
            Some(name) if files.len() == 1 => {
                let path = dir.join(name);
                let bytes = fs::read(&path)
                    .with_context(|| format!("texture {} does not read", path.display()))?;
                ensure!(
                    (bytes.len() as u64) <= MAX_TEXTURE,
                    "texture {name} is larger than 64 MB"
                );
                textures.insert(slot, Some((name.clone(), bytes)));
            }
            _ => bail!(
                "material slot {slot} is bound to {} different texture files across skins; a .glb carries one per slot",
                files.len()
            ),
        }
    }

    let mut notes = Vec::new();
    let (glb, embedded) = bri_gltf::export_shape(&shape, &textures, &mut notes)?;
    let glb_name = file.trim_end_matches(".json").to_string() + ".glb";

    // Read the bytes back exactly as the client will.
    let back = bri_gltf::lower(&glb, asset)?.shape;
    back.validate()?;
    ensure!(
        back.materials.len() == shape.materials.len(),
        "material slots did not survive ({} -> {})",
        shape.materials.len(),
        back.materials.len()
    );

    // The triangles the physics builds from each side's collision details.
    let collision_triangles = |s: &Shape| -> Result<usize> {
        let mut pose = bri_content::animation::sample(s, None, 0.0)?;
        pose.visibility.fill(1.0);
        Ok(s
            .details
            .iter()
            .enumerate()
            .filter(|(_, d)| d.collision)
            .map(|(i, _)| {
                bri_content::animation::triangles(s, &pose, i, |_| true)
                    .map(|t| t.len())
            })
            .collect::<Result<Vec<_>>>()?
            .iter()
            .sum())
    };
    let before = collision_triangles(&shape)?;
    let after = collision_triangles(&back)?;
    ensure!(before == after, "collision triangles did not survive ({before} -> {after})");

    if apply {
        fs::write(dir.join(&glb_name), &glb).with_context(|| {
            format!("cannot write {}", dir.join(&glb_name).display())
        })?;
    }

    // What the scene draws: its first non-collision detail.
    let mut pose = bri_content::animation::sample(&shape, None, 0.0)?;
    pose.visibility.fill(1.0);
    let visible_triangles = shape
        .details
        .iter()
        .position(|d| !d.collision && d.pixel_threshold >= 0.0)
        .map(|i| bri_content::animation::triangles(&shape, &pose, i, |_| true).unwrap_or_default().len())
        .unwrap_or(0);

    Ok(ShapePlan {
        asset: asset.to_owned(),
        file: file.to_owned(),
        glb: glb_name,
        materials: shape.materials.len(),
        visible_triangles,
        collision_triangles: before,
        embedded_textures: embedded.into_iter().collect(),
        notes,
    })
}
