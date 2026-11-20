//! Temporary content migrator (throwaway tool).
//!
//! The base avatar pack still ships its rig as a JSON bundle (`rig.json`:
//! the shape plus a case-normalized sequence table), the last `.json` model
//! format left in installed content. This tool rewrites each such pack to a
//! self-contained `m.glb`: the same geometry, with every sequence riding
//! inside the file's glTF animations (its Blockland semantics — name, length,
//! looping, layering, triggers and object tracks — in each animation's
//! `extras.bri_clip`, exactly as the Add-On importer now writes). No textures
//! are embedded: the client binds the pack's own PNGs to materials by name at
//! runtime, so rendering stays byte-identical.
//!
//! Each candidate is verified before anything moves: the GLB is lowered again
//! and checked against the JSON rig field for field — scalar semantics exact,
//! per-frame translations and scales bit-exact, rotations within 1e-4 (the
//! worst difference reported). Only a pack that verifies — and only with
//! `--apply` — gets its files written: `m.glb` beside the old rig,
//! `avatar.json` re-pinned to it (`rig`, new sha256) and the JSON rig deleted.
//! A pack that fails verification is reported and left untouched.
//!
//! Usage: `avatar-to-glb CONTENT_ROOT [--apply]`
//!
//! Dry-run by default; writes AVATAR-TO-GLB.json into the root either way.
//!
//! TEMPORARY TOOL: delete this file and its `[[bin]]` entry once content is
//! migrated onto GLB rigs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail, ensure};
use bri_content::avatar::{Package, Rig};
use bri_content::shape::{Animation, Node, NodeTrack};
use sha2::{Digest, Sha256};
use serde_json::{Value, json};

/// The rig file a migrated pack points at.
const NEW_RIG: &str = "m.glb";
/// Rotations come back through two normalizes; this much drift is float noise.
const ROT_EPS: f32 = 1e-4;

struct Args {
    root: PathBuf,
    apply: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(args) = parse_args(&args) else {
        eprintln!("usage: avatar-to-glb CONTENT_ROOT [--apply]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if report.failed > 0 {
                println!(
                    "AVATAR-TO-GLB: {} problem(s); see AVATAR-TO-GLB.json. Nothing was written for them.",
                    report.failed
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("avatar-to-glb: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Result<Args> {
    let mut root = None;
    let mut apply = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--apply" => apply = true,
            other if !other.starts_with('-') => {
                ensure!(root.is_none(), "unexpected argument: {other}");
                root = Some(PathBuf::from(other));
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok(Args {
        root: root.context("missing CONTENT_ROOT")?,
        apply,
    })
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                // Hidden folders (research, VCS) and staged downloads are not content.
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with('.') || name == ".downloads" {
                    continue;
                }
                stack.push(p);
            } else if p.is_file() && p.file_name().is_some_and(|n| n == "avatar.json") {
                out.push(p);
            }
        }
    }
    out
}

struct Report {
    applied: bool,
    migrated: usize,
    skipped: usize,
    failed: usize,
    packs: Vec<Value>,
}

impl Report {
    fn summary_line(&self) -> String {
        format!(
            "AVATAR-TO-GLB: {} pack(s) verified to a GLB rig{}, {} skipped, {} failed",
            self.migrated,
            if self.applied { "" } else { " (dry run — nothing written)" },
            self.skipped,
            self.failed
        )
    }
}

fn run(args: &Args) -> Result<Report> {
    let root = args.root.canonicalize().with_context(|| {
        format!("content root {}", args.root.display())
    })?;
    ensure!(root.is_dir(), "not a directory: {}", root.display());
    let mut report = Report {
        applied: args.apply,
        migrated: 0,
        skipped: 0,
        failed: 0,
        packs: Vec::new(),
    };
    for avatar_json in walk(&root) {
        match migrate_pack(&avatar_json, &root, args.apply) {
            Ok(entry) => {
                match entry["status"].as_str() {
                    Some("migrated") | Some("verified (dry run)") => report.migrated += 1,
                    Some("skipped") => report.skipped += 1,
                    _ => report.failed += 1,
                }
                report.packs.push(entry);
            },
            Err(error) => {
                let rel = avatar_json.strip_prefix(&root).unwrap_or(&avatar_json);
                eprintln!("{}: {error:#}", rel.display());
                report.failed += 1;
                report.packs.push(json!({ "pack": rel.display().to_string(), "status": "failed", "reason": format!("{error:#}") }));
            }
        }
    }
    let value = json!({
        "applied": report.applied,
        "migrated": report.migrated,
        "skipped": report.skipped,
        "failed": report.failed,
        "packs": report.packs,
    });
    fs::write(
        root.join("AVATAR-TO-GLB.json"),
        serde_json::to_string_pretty(&value).unwrap_or_default() + "\n",
    )
    .with_context(|| format!("writing {}", root.display()))?;
    Ok(report)
}

fn migrate_pack(avatar_json: &Path, root: &Path, apply: bool) -> Result<Value> {
    let dir = avatar_json.parent().context("avatar.json has no folder")?;
    let rel = dir
        .strip_prefix(root)
        .unwrap_or(dir)
        .display()
        .to_string();
    let raw = fs::read(avatar_json).with_context(|| format!("reading {}", avatar_json.display()))?;
    let package: Package = serde_json::from_slice(&raw)
        .with_context(|| format!("parsing {}", avatar_json.display()))?;
    package.validate()?;

    if !package.rig.to_ascii_lowercase().ends_with(".json") {
        return Ok(json!({
            "pack": rel,
            "id": package.id,
            "status": "skipped",
            "reason": format!("the rig is already {}", package.rig),
        }));
    }

    let rig_path = dir.join(&package.rig);
    let bytes = fs::read(&rig_path).with_context(|| format!("reading {}", rig_path.display()))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == package.rig_sha256,
        "the rig of {} no longer matches its pin",
        package.id
    );
    let rig: Rig = serde_json::from_slice(&bytes)?;
    rig.validate()?;

    // The export shape: the same geometry with every sequence as its own
    // animation (BTreeMap order, so the file is deterministic).
    let mut shape = rig.shape.clone();
    shape.animations = rig.sequences.values().cloned().collect();
    let mut notes = Vec::new();
    let (glb, _embedded) = bri_gltf::export_shape(&shape, &BTreeMap::new(), &mut notes)?;

    // Prove the file carries exactly what the JSON said before anything moves.
    let (sequences, rotations_worst) = verify(&rig, &glb, &package.id)?;

    if apply {
        fs::write(dir.join(NEW_RIG), &glb)
            .with_context(|| format!("writing {}", dir.join(NEW_RIG).display()))?;
        // Re-pin avatar.json to the new file, touching nothing else.
        let mut v: Value = serde_json::from_slice(&raw)?;
        v["rig"] = json!(NEW_RIG);
        v["rig_sha256"] = json!(format!("{:x}", Sha256::digest(&glb)));
        fs::write(
            avatar_json,
            serde_json::to_string_pretty(&v).unwrap_or_default() + "\n",
        )
        .with_context(|| format!("writing {}", avatar_json.display()))?;
        fs::remove_file(&rig_path)
            .with_context(|| format!("removing {}", rig_path.display()))?;
    }

    Ok(json!({
        "pack": rel,
        "id": package.id,
        "status": if apply { "migrated" } else { "verified (dry run)" },
        "sequences": sequences,
        "rotations_worst_diff": rotations_worst,
        "json_rig_bytes": bytes.len(),
        "glb_bytes": glb.len(),
        "export_notes": notes,
    }))
}

/// The lowered GLB against the JSON rig: geometry whole and named, every
/// sequence present with its semantics exact and its motion bit-exact (the
/// rotations' normalize drift is the only tolerance; the worst of it comes
/// back for the report).
fn verify(rig: &Rig, glb: &[u8], id: &str) -> Result<(usize, f32)> {
    let converted = bri_gltf::lower(glb, id)?;
    let shape = &converted.shape;
    shape.validate()?;

    // --- The geometry ------------------------------------------------------
    // glTF has no word for an object bound to a bone without its own node, so
    // the exporter writes one holder per visible mesh slot (named after the
    // object; `_LOD1` twins and `col_` colliders likewise) and the re-import
    // keeps each as its own node. The skeleton must come back whole on top of
    // exactly that many additions — no more, no less.
    let expected_holders = {
        let visible: Vec<usize> = rig
            .shape
            .details
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.collision)
            .map(|(i, _)| i)
            .collect();
        let mut n = 0usize;
        for &di in visible.iter().take(2) {
            n += detail_mesh_slots(&rig.shape, &rig.shape.details[di]);
        }
        for d in rig.shape.details.iter().filter(|d| d.collision) {
            n += detail_mesh_slots(&rig.shape, d);
        }
        n
    };
    ensure!(
        shape.nodes.len() == rig.shape.nodes.len() + expected_holders,
        "node count changed ({} -> {}, expecting {} skeleton plus {} holder)",
        rig.shape.nodes.len(),
        shape.nodes.len(),
        rig.shape.nodes.len(),
        expected_holders
    );
    let lower_index: BTreeMap<String, usize> = shape
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.to_ascii_lowercase(), i))
        .collect();
    ensure!(lower_index.len() == shape.nodes.len(), "the GLB repeats a node name");
    let mut worst = 0.0f32;
    // Parents by lower-case name (either file may order nodes differently).
    let parent_of = |nodes: &[Node], i: usize| -> Option<String> {
        nodes[i].parent.map(|p| nodes[p].name.to_ascii_lowercase())
    };
    for (i, n) in rig.shape.nodes.iter().enumerate() {
        let Some(&j) = lower_index.get(&n.name.to_ascii_lowercase()) else {
            bail!("node {} is missing from the GLB", n.name);
        };
        let m = &shape.nodes[j];
        ensure!(m.name == n.name, "the node name `{}` came back as `{}`", n.name, m.name);
        ensure!(
            parent_of(&rig.shape.nodes, i) == parent_of(&shape.nodes, j),
            "the parent of {} changed",
            n.name
        );
        for (a, b) in n.translation.iter().zip(m.translation.iter()) {
            ensure!(
                znorm(*a).to_bits() == znorm(*b).to_bits(),
                "the rest translation of {} changed ({a} -> {b})",
                n.name
            );
        }
        worst = worst.max(quat_diff(n.rotation, m.rotation));
    }

    // Every added node is a mesh holder parented under one of the skeleton's
    // bones, or at the root for skinned meshes.
    let orig_names: BTreeSet<String> = rig
        .shape
        .nodes
        .iter()
        .map(|n| n.name.to_ascii_lowercase())
        .collect();
    for h in &shape.nodes {
        if !orig_names.contains(&h.name.to_ascii_lowercase()) {
            ensure!(
                h.parent.is_none_or(|p| orig_names.contains(&shape.nodes[p].name.to_ascii_lowercase())),
                "the holder `{}` hangs under a non-skeleton node",
                h.name
            );
        }
    }

    // Objects come back whole by name; scene traversal may order them
    // differently, so compare as a bag of lower-case names.
    let object_bag = |s: &bri_content::shape::Shape| {
        let mut m: BTreeMap<String, u32> = BTreeMap::new();
        for o in &s.objects {
            *m.entry(o.name.to_ascii_lowercase()).or_insert(0) += 1;
        }
        m
    };
    ensure!(
        object_bag(shape) == object_bag(&rig.shape),
        "the GLB's objects are not the rig's ({} vs {})",
        shape.objects.len(),
        rig.shape.objects.len()
    );

    // Details come back whole, in order.
    ensure!(
        shape.details.len() == rig.shape.details.len(),
        "detail count changed ({} -> {})",
        rig.shape.details.len(),
        shape.details.len()
    );
    for (a, b) in rig.shape.details.iter().zip(shape.details.iter()) {
        ensure!(
            a.name == b.name
                && a.collision == b.collision
                && a.pixel_threshold.to_bits() == b.pixel_threshold.to_bits(),
            "detail `{}` changed",
            a.name
        );
    }
    let mesh_some = |s: &bri_content::shape::Shape| s.meshes.iter().filter(|m| m.is_some()).count();
    ensure!(mesh_some(shape) == mesh_some(&rig.shape), "mesh count changed");

    // Materials come back positionally, named the way the loader names them
    // (`safe_name`, suffixed where two collide): the client binds the pack's
    // textures to those lower-case names.
    ensure!(
        shape.materials.len() == rig.shape.materials.len(),
        "material count changed ({} -> {})",
        rig.shape.materials.len(),
        shape.materials.len()
    );
    let mut taken = BTreeSet::new();
    for (a, b) in rig.shape.materials.iter().zip(shape.materials.iter()) {
        let base = bri_gltf::safe_name(&a.name);
        let mut want = base.clone();
        let mut k = 1;
        while !taken.insert(want.clone()) {
            want = format!("{base}_{k}");
            k += 1;
        }
        ensure!(b.name == want, "the material `{}` came back as `{}`", a.name, b.name);
    }

    // --- The sequences -----------------------------------------------------
    let lowered: BTreeMap<String, &Animation> = shape
        .animations
        .iter()
        .map(|a| (a.name.to_ascii_lowercase(), a))
        .collect();
    ensure!(lowered.len() == shape.animations.len(), "the GLB repeats an animation name");
    ensure!(
        lowered.len() == rig.sequences.len(),
        "sequence count changed ({} -> {})",
        rig.sequences.len(),
        lowered.len()
    );
    for (name, clip) in &rig.sequences {
        let low = lowered
            .get(name)
            .with_context(|| format!("the sequence `{name}` is missing from the GLB"))?;
        worst = worst.max(check_clip(clip, low)?);
    }

    Ok((rig.sequences.len(), worst))
}

/// One clip's semantics exact and its per-frame motion bit-exact (rotations
/// within [`ROT_EPS`]); returns the worst rotation difference seen.
fn check_clip(orig: &Animation, low: &Animation) -> Result<f32> {
    let name = orig.name.as_str();
    ensure!(low.frames == orig.frames, "the frame count of `{name}` changed ({} -> {})", orig.frames, low.frames);
    ensure!(
        low.duration.to_bits() == orig.duration.to_bits(),
        "the duration of `{name}` changed"
    );
    ensure!(
        low.looping == orig.looping
            && low.additive == orig.additive
            && low.priority == orig.priority
            && low.parent_translations == orig.parent_translations
            && low.rotation_interpolation == orig.rotation_interpolation,
        "the layering of `{name}` changed"
    );
    ensure!(low.triggers.len() == orig.triggers.len(), "the trigger count of `{name}` changed");
    for (a, b) in orig.triggers.iter().zip(low.triggers.iter()) {
        ensure!(
            a.state == b.state && a.position.to_bits() == b.position.to_bits(),
            "a trigger of `{name}` changed"
        );
    }
    ensure!(
        low.ground_translations.len() == orig.ground_translations.len()
            && low.ground_rotations.len() == orig.ground_rotations.len(),
        "the ground motion count of `{name}` changed"
    );
    for (i, (x, y)) in orig
        .ground_translations
        .iter()
        .zip(low.ground_translations.iter())
        .enumerate()
    {
        ensure!(
            x.iter().copied().map(f32::to_bits).eq(y.iter().copied().map(f32::to_bits)),
            "a ground translation row of `{name}` differs at {i}"
        );
    }
    ensure!(low.objects.len() == orig.objects.len(), "the object track count of `{name}` changed");
    for (a, b) in orig.objects.iter().zip(low.objects.iter()) {
        ensure!(a.object == b.object, "an object track of `{name}` moved to another object");
        ensure!(
            a.visibility.iter().copied().map(f32::to_bits)
                .eq(b.visibility.iter().copied().map(f32::to_bits)),
            "the visibility track of `{name}` changed"
        );
        ensure!(a.frames == b.frames && a.material_frames == b.material_frames, "an integer track of `{name}` changed");
    }

    // Per-node motion. A fully empty authored track rides nowhere; anything
    // else must come back with exactly the paths it had.
    let lowered: BTreeMap<String, &NodeTrack> = low
        .nodes
        .iter()
        .map(|t| (t.node.to_ascii_lowercase(), t))
        .collect();
    let mut worst = 0.0f32;
    for t in &orig.nodes {
        let lower = t.node.to_ascii_lowercase();
        let Some(m) = lowered.get(&lower) else {
            ensure!(
                t.rotations.is_empty() && t.translations.is_empty() && t.scales.is_empty(),
                "the track of `{}` was dropped from `{name}`",
                t.node
            );
            continue;
        };
        for (label, a, b) in [
            ("translations", &t.translations, &m.translations),
            ("scales", &t.scales, &m.scales),
        ] {
            if !a.is_empty() {
                ensure!(a.len() == b.len(), "the {label} of `{}` in `{name}` changed length", t.node);
                for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                    // Absolute scales re-import as magnitudes; the rest is exact.
                    let want: Vec<u32> = if orig.additive || label == "translations" {
                        x.iter().copied().map(f32::to_bits).collect()
                    } else {
                        x.iter().map(|v| v.abs().to_bits()).collect()
                    };
                    let got: Vec<u32> = y.iter().copied().map(f32::to_bits).collect();
                    ensure!(want == got, "the {label} of `{}` in `{name}` differ at frame {i}", t.node);
                }
            } else {
                ensure!(b.is_empty(), "the {label} of `{}` in `{name}` appeared", t.node);
            }
        }
        if !t.rotations.is_empty() {
            ensure!(!m.rotations.is_empty(), "the rotations of `{}` in `{name}` were dropped", t.node);
            ensure!(m.rotations.len() == t.rotations.len(), "the rotation count of `{}` in `{name}` changed", t.node);
            for (i, (x, y)) in t.rotations.iter().zip(m.rotations.iter()).enumerate() {
                worst = worst.max(quat_diff(*x, *y));
                ensure!(worst <= ROT_EPS, "the rotations of `{}` in `{name}` drift at frame {i} ({worst})", t.node);
            }
        } else {
            ensure!(m.rotations.is_empty(), "rotations of `{}` in `{name}` appeared", t.node);
        }
    }
    Ok(worst)
}

/// The objects of one detail that carry a mesh at its offset — exactly the
/// ones whose export writes a holder node.
fn detail_mesh_slots(shape: &bri_content::shape::Shape, d: &bri_content::shape::Detail) -> usize {
    (d.object_start..d.object_start + d.object_count)
        .filter(|&oi| oi < shape.objects.len())
        .filter_map(|oi| shape.objects[oi].meshes.get(d.mesh_offset).copied())
        .filter(|&mi| shape.meshes.get(mi).is_some_and(Option::is_some))
        .count()
}

/// ±0 are one value; compare them as such (authored files may carry signed
/// zeros, and a mirrored or omitted zero comes back unsigned).
fn znorm(x: f32) -> f32 {
    if x == 0.0 { 0.0 } else { x }
}

/// The distance between two quaternions, `q` and `-q` being the same turn:
/// the smaller per-component spread over both alignments.
fn quat_diff(a: [f32; 4], b: [f32; 4]) -> f32 {
    let neg = [-b[0], -b[1], -b[2], -b[3]];
    let d = |x: &[f32; 4], y: &[f32; 4]| x.iter().zip(y).map(|(p, q)| (p - q).abs()).fold(0.0f32, f32::max);
    d(&a, &b).min(d(&a, &neg))
}
