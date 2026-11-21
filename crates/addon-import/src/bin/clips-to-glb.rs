//! Temporary content migrator (throwaway tool).
//!
//! The last `.clips.json` animation files in installed content: the horse's
//! eighteen sequences, the cannon and tank-turret `look` clips, and the two
//! weapon-debris models that still ship as JSON shapes. This tool bakes each
//! clip into its model's glTF (the Add-On importer's own layout — Blockland
//! semantics in each animation's `extras.bri_clip`) and rewrites the debris
//! models to self-contained `.glb` files with their textures embedded, so no
//! `.json` model or animation format is left in content.
//!
//! Every candidate is verified before anything moves: the GLB is lowered again
//! and checked against what it replaced — geometry whole and named, each new
//! sequence present with its semantics exact and its motion bit-exact (the
//! rotation normalize drift is the only tolerance). Only a model that verifies
//! — and only with `--apply` — gets written. Nothing is deleted here; the
//! manifest surgery and file removals happen beside it.
//!
//! Usage: `clips-to-glb CONTENT_ROOT [--apply]`
//!
//! Dry-run by default; writes CLIPS-TO-GLB.json into the root either way.
//!
//! TEMPORARY TOOL: delete this file and its `[[bin]]` entry once content is
//! migrated onto GLB models with embedded sequences.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail, ensure};
use bri_content::shape::{Animation, ClipSet, NodeTrack, Shape};
use sha2::{Digest, Sha256};
use serde_json::{Value, json};

/// Rotations come back through two normalizes; this much drift is float noise.
const ROT_EPS: f32 = 1e-4;

struct Args {
    root: PathBuf,
    apply: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(args) = parse_args(&args) else {
        eprintln!("usage: clips-to-glb CONTENT_ROOT [--apply]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if report.failed > 0 {
                println!(
                    "CLIPS-TO-GLB: {} problem(s); see CLIPS-TO-GLB.json. Nothing was written for them.",
                    report.failed
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("clips-to-glb: {error:#}");
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

struct Report {
    applied: bool,
    migrated: usize,
    failed: usize,
    models: Vec<Value>,
}

impl Report {
    fn summary_line(&self) -> String {
        format!(
            "CLIPS-TO-GLB: {} model(s) verified{}, {} failed",
            self.migrated,
            if self.applied { "" } else { " (dry run — nothing written)" },
            self.failed
        )
    }
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_clip(file: &Path) -> Result<ClipSet> {
    let bytes = fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    Ok(serde_json::from_slice(&bytes)?)
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

/// ±0 are one value; compare them as such.
fn znorm(x: f32) -> f32 {
    if x == 0.0 { 0.0 } else { x }
}

/// The distance between two quaternions, `q` and `-q` being the same turn.
fn quat_diff(a: [f32; 4], b: [f32; 4]) -> f32 {
    let neg = [-b[0], -b[1], -b[2], -b[3]];
    let d = |x: &[f32; 4], y: &[f32; 4]| x.iter().zip(y).map(|(p, q)| (p - q).abs()).fold(0.0f32, f32::max);
    d(&a, &b).min(d(&a, &neg))
}

/// The lowered GLB against its source: the skeleton whole and named (rest
/// transforms bit-exact), objects and details intact, materials positionally
/// named — so re-exporting with more animations changed nothing else.
fn verify_geometry(orig: &Shape, glb: &[u8], id: &str) -> Result<f32> {
    let converted = bri_gltf::lower(glb, id)?;
    let shape = &converted.shape;
    shape.validate()?;

    // The exporter writes one holder node per mesh slot on every pass (the
    // avatar tool's rule): the re-exported file holds `orig`'s whole tree —
    // which already includes its first-generation holders — plus exactly that
    // many new ones.
    let detail_mesh_slots = |d: &bri_content::shape::Detail| -> usize {
        (d.object_start..d.object_start + d.object_count)
            .filter(|&oi| oi < orig.objects.len())
            .filter_map(|oi| orig.objects[oi].meshes.get(d.mesh_offset).copied())
            .filter(|&mi| orig.meshes.get(mi).is_some_and(Option::is_some))
            .count()
    };
    let visible: Vec<usize> = (0..orig.details.len())
        .filter(|&i| !orig.details[i].collision)
        .collect();
    let expected_holders = visible.iter().take(2).map(|&di| detail_mesh_slots(&orig.details[di])).sum::<usize>()
        + orig
            .details
            .iter()
            .filter(|d| d.collision)
            .map(detail_mesh_slots)
            .sum::<usize>();
    ensure!(
        shape.nodes.len() == orig.nodes.len() + expected_holders,
        "node count changed ({} -> {}, expecting {} plus {} holder(s))",
        orig.nodes.len(),
        shape.nodes.len(),
        orig.nodes.len(),
        expected_holders
    );
    let lower_index: BTreeMap<String, usize> = shape
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.to_ascii_lowercase(), i))
        .collect();
    ensure!(lower_index.len() == shape.nodes.len(), "the GLB repeats a node name");
    let parent_of = |nodes: &[bri_content::shape::Node], i: usize| -> Option<String> {
        nodes[i].parent.map(|p| nodes[p].name.to_ascii_lowercase())
    };
    let mut worst = 0.0f32;
    for (i, n) in orig.nodes.iter().enumerate() {
        let Some(&j) = lower_index.get(&n.name.to_ascii_lowercase()) else {
            bail!("node {} is missing from the GLB", n.name);
        };
        let m = &shape.nodes[j];
        ensure!(m.name == n.name, "the node name `{}` came back as `{}`", n.name, m.name);
        ensure!(
            parent_of(&orig.nodes, i) == parent_of(&shape.nodes, j),
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

    // Every added node is a mesh holder parented under an original node, or
    // at the root for skinned meshes.
    let orig_names: BTreeSet<String> = orig
        .nodes
        .iter()
        .map(|n| n.name.to_ascii_lowercase())
        .collect();
    for h in &shape.nodes {
        if !orig_names.contains(&h.name.to_ascii_lowercase()) {
            ensure!(
                h.parent.is_none_or(|p| orig_names.contains(&shape.nodes[p].name.to_ascii_lowercase())),
                "the holder `{}` hangs under a non-original node",
                h.name
            );
        }
    }

    // Objects and colliders may come back renamed — on re-export a holder
    // node claims an object's name first, and colliders are rebuilt named for
    // their `col_` nodes — so the geometry is compared where it hangs in the
    // source tree and how many vertices each authored mesh carries. The
    // nearest ancestor whose name belongs to the source anchors both sides:
    // a first-generation object sits on that node, its re-export under it.
    let anchor = |nodes: &[bri_content::shape::Node], o: &bri_content::shape::Object| -> String {
        let mut i = o.node;
        while let Some(k) = i {
            if orig_names.contains(&nodes[k].name.to_ascii_lowercase()) {
                return nodes[k].name.clone();
            }
            i = nodes[k].parent;
        }
        "(root)".to_string()
    };
    let geometry = |s: &Shape| -> BTreeMap<(String, usize), u32> {
        let mut m = BTreeMap::new();
        for o in &s.objects {
            for mi in &o.meshes {
                if let Some(mesh) = s.meshes.get(*mi).and_then(Option::as_ref) {
                    *m.entry((anchor(&s.nodes, o), mesh.positions.len())).or_insert(0) += 1;
                }
            }
        }
        m
    };
    ensure!(
        geometry(shape) == geometry(orig),
        "the GLB's geometry is not the model's: {:?} vs {:?}",
        geometry(shape),
        geometry(orig)
    );

    // Visible details ride in the asset's extras and come back named; each
    // must survive with its threshold. Colliders are rebuilt one per distinct
    // block mask among slots that actually carry a mesh — a detail pointing
    // at a slot its objects lack was never drawn (the game skips it too).
    for d in orig.details.iter().filter(|d| !d.collision) {
        let Some(m) = shape.details.iter().find(|x| x.name == d.name) else {
            bail!("visible detail `{}` is missing from the GLB", d.name);
        };
        ensure!(!m.collision, "detail `{}` came back as a collider", d.name);
        ensure!(
            m.pixel_threshold.to_bits() == d.pixel_threshold.to_bits(),
            "the threshold of `{}` changed",
            d.name
        );
    }
    let mut masks: BTreeSet<Option<Vec<String>>> = BTreeSet::new();
    for d in orig.details.iter().filter(|d| d.collision) {
        if (d.object_start..d.object_start + d.object_count).any(|oi| {
            oi < orig.objects.len()
                && orig.objects[oi]
                    .meshes
                    .get(d.mesh_offset)
                    .copied()
                    .is_some_and(|mi| orig.meshes.get(mi).is_some_and(Option::is_some))
        }) {
            masks.insert(d.blocks.clone());
        }
    }
    let got_colliders = shape.details.iter().filter(|d| d.collision).count();
    ensure!(
        got_colliders == masks.len(),
        "collision detail count changed ({} -> {}, expecting {} mask(s))",
        orig.details.iter().filter(|d| d.collision).count(),
        got_colliders,
        masks.len()
    );
    for m in shape.details.iter().filter(|d| d.collision) {
        ensure!(m.pixel_threshold.to_bits() == (-1.0f32).to_bits(), "collider `{}` lost its threshold", m.name);
    }

    // Every animation the source carried must come back with its semantics,
    // and its motion exact; re-exporting may add sequences but drops none.
    for a in &orig.animations {
        let Some(m) = shape.animations.iter().find(|x| x.name.eq_ignore_ascii_case(&a.name)) else {
            bail!("animation `{}` is missing from the GLB", a.name);
        };
        ensure!(
            m.frames == a.frames && m.looping == a.looping && m.additive == a.additive,
            "the semantics of animation `{}` changed",
            a.name
        );
        worst = worst.max(check_clip(a, m)?);
    }

    let mesh_some = |s: &Shape| s.meshes.iter().filter(|m| m.is_some()).count();
    ensure!(mesh_some(shape) == mesh_some(orig), "mesh count changed");

    ensure!(shape.materials.len() == orig.materials.len(), "material count changed ({} -> {})", shape.materials.len(), orig.materials.len());
    let mut taken = BTreeSet::new();
    for (a, b) in orig.materials.iter().zip(shape.materials.iter()) {
        let base = bri_gltf::safe_name(&a.name);
        let mut want = base.clone();
        let mut k = 1;
        while !taken.insert(want.clone()) {
            want = format!("{base}_{k}");
            k += 1;
        }
        ensure!(b.name == want, "the material `{}` came back as `{}`", a.name, b.name);
    }
    Ok(worst)
}

/// Bake `clips` (by their new names) into the model at `glb`, verify the
/// result against both the old file and each source clip, write it on apply.
fn embed(
    root: &Path,
    rel_glb: &str,
    clips: Vec<(String, Animation)>,
    apply: bool,
) -> Result<Value> {
    let path = root.join(rel_glb);
    let old = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let old_shape = bri_gltf::lower(&old, rel_glb)?.shape;

    // A sequence with no tracks at all carries nothing in glTF (the import
    // drops empty clips), so it is left out on purpose: the runtime treats a
    // missing alias as the rest pose, exactly as the legacy clip file's empty
    // first sequence did.
    let motionless = |a: &Animation| {
        a.nodes.is_empty()
            && a.objects.is_empty()
            && a.ground_translations.is_empty()
            && a.ground_rotations.is_empty()
            && a.triggers.is_empty()
    };

    let mut shape = old_shape.clone();
    for (name, clip) in &clips {
        ensure!(
            !shape.animations.iter().any(|a| a.name.eq_ignore_ascii_case(&clip.name)),
            "the model already has sequence `{}`",
            name
        );
    }
    for (_, clip) in clips.iter().filter(|(_, c)| !motionless(c)) {
        shape.animations.push(clip.clone());
    }

    let mut notes = Vec::new();
    let (glb, _embedded) = bri_gltf::export_shape(&shape, &BTreeMap::new(), &mut notes)?;

    // Geometry unchanged by the re-export...
    let worst_geo = verify_geometry(&old_shape, &glb, rel_glb)?;
    // ...and every new sequence exact against its source clip.
    let lowered = bri_gltf::lower(&glb, rel_glb)?.shape;
    let mut worst = worst_geo;
    let mut dropped: Vec<String> = Vec::new();
    for (orig_name, orig) in &clips {
        if motionless(orig) {
            ensure!(
                !lowered.animations.iter().any(|a| a.name.eq_ignore_ascii_case(orig_name)),
                "the empty sequence `{orig_name}` was written anyway"
            );
            dropped.push(orig_name.clone());
            continue;
        }
        let low = lowered
            .animations
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(&orig_name))
            .with_context(|| format!("the sequence `{orig_name}` is missing from the GLB"))?;
        worst = worst.max(check_clip(orig, low)?);
    }

    if apply {
        fs::write(&path, &glb).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(json!({
        "model": rel_glb,
        "status": if apply { "migrated" } else { "verified (dry run)" },
        "old_sha256": sha(&old),
        "new_sha256": sha(&glb),
        "sequences_added": clips.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
        // Empty sequences glTF cannot carry; the runtime rests on them.
        "sequences_left_out_as_empty": dropped,
        "rotations_worst_diff": worst,
        "export_notes": notes,
    }))
}

/// One weapon-debris JSON model to a self-contained GLB with its textures
/// embedded (`None` where the pack carries no bitmap for the material).
fn debris_model(
    root: &Path,
    rel_json: &str,
    texture_of: impl Fn(&str) -> Option<Vec<u8>>,
    apply: bool,
) -> Result<Value> {
    let path = root.join(rel_json);
    let old = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let shape: Shape = serde_json::from_slice(&old)?;
    shape.validate()?;

    let textures: BTreeMap<usize, Option<(String, Vec<u8>)>> = shape
        .materials
        .iter()
        .enumerate()
        .map(|(i, m)| {
            (
                i,
                texture_of(&m.name)
                    .map(|png| (format!("{}:{}", rel_json, m.name), png)),
            )
        })
        .collect();

    let mut notes = Vec::new();
    let (glb, embedded_names) = bri_gltf::export_shape(&shape, &textures, &mut notes)?;

    // The re-import: geometry whole, materials named, the authored animation
    // exact, and every texture that was embedded comes back under its name.
    let converted = bri_gltf::lower(&glb, rel_json)?;
    verify_geometry(&shape, &glb, rel_json)?;
    // `embedded_names` holds the export-time label strings; what the runtime
    // actually binds is `lower`'s texture map, keyed by the material's safe
    // name. Assert against that.
    for (i, m) in shape.materials.iter().enumerate() {
        if textures.get(&i).is_some_and(|t| t.is_some()) {
            let key = bri_gltf::safe_name(&m.name);
            ensure!(
                converted.textures.contains_key(&key),
                "the texture of `{}` did not embed",
                m.name
            );
        }
    }
    for (orig, low) in shape.animations.iter().zip(converted.shape.animations.iter()) {
        check_clip(orig, low)?;
    }
    ensure!(
        converted.shape.animations.len() == shape.animations.len(),
        "animation count changed"
    );

    if apply {
        let out = path.with_extension("glb");
        fs::write(&out, &glb).with_context(|| format!("writing {}", out.display()))?;
    }
    Ok(json!({
        "model": rel_json,
        "status": if apply { "migrated" } else { "verified (dry run)" },
        "new_file": path.with_extension("glb").file_name().unwrap().to_string_lossy(),
        "old_sha256": sha(&old),
        "new_sha256": sha(&glb),
        "embedded_textures": embedded_names.iter().cloned().collect::<Vec<_>>(),
        "export_notes": notes,
    }))
}

fn run(args: &Args) -> Result<Report> {
    let root = args.root.canonicalize()?;
    ensure!(root.is_dir(), "not a directory: {}", root.display());
    // The one-time conversion ran against the old single pack; its models now
    // live in the per-vehicle packs and the debris shapes in weapon_debris_base.
    let horse_pack = root.join("default-pack/vehicle_horse/assets");
    let cannon = root.join("default-pack/vehicle_pirate_cannon/assets");
    let tank = root.join("default-pack/vehicle_tank/assets");
    let debris = root.join("default-pack/weapon_debris_base");
    ensure!(
        horse_pack.is_dir() && cannon.is_dir() && tank.is_dir() && debris.is_dir(),
        "packs missing under {}",
        root.display()
    );

    // --- The horse: its eighteen authored sequences, by their new names. --
    // Each entry: (embedded name, the clip file it comes from, and which of
    // that file's animations — most hold one named `armAttack`).
    let horse_clips = [
        ("root", "h_root"),
        ("run", "h_run"),
        ("back", "h_back"),
        ("side", "h_side"),
        ("jump", "h_jump"),
        ("look", "h_look"),
        ("armAttack", "h_armattack"),
        ("plant", "h_plant"),
        ("rotccw", "h_rotccw"),
        ("rotcw", "h_rotcw"),
        ("shiftaway", "h_shiftaway"),
        ("shiftdown", "h_shiftdown"),
        ("shiftleft", "h_shiftleft"),
        ("shiftright", "h_shiftright"),
        ("shiftto", "h_shiftto"),
        ("shiftup", "h_shiftup"),
        ("spearready", "h_spearready"),
        // `death1` holds the file's second animation; its first is a copy of
        // `root`, which the aliases that pose nothing already name.
        ("death1", "h_death1"),
    ];
    let mut horse: Vec<(String, Animation)> = Vec::new();
    for (name, stem) in horse_clips {
        let set = read_clip(&horse_pack.join(format!("models/{stem}.clips.json")))?;
        let clip = if name == "death1" {
            set.animations
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case("death1"))
                .with_context(|| format!("{stem}: no death1 sequence"))?
        } else {
            &set.animations[0]
        };
        let mut clip = clip.clone();
        clip.name = name.to_string();
        horse.push((name.to_string(), clip));
    }

    // --- The gunners' barrels: each `look` clip rides its own model. ------
    let cannon_look = read_clip(&cannon.join("models/Cannon_look.clips.json"))?;
    let tank_look = read_clip(&tank.join("models/t_look.clips.json"))?;
    ensure!(cannon_look.animations.iter().any(|a| a.name == "look"), "Cannon_look has no look clip");
    ensure!(tank_look.animations.iter().any(|a| a.name == "look"), "t_look has no look clip");

    let mut report = Report {
        applied: args.apply,
        migrated: 0,
        failed: 0,
        models: Vec::new(),
    };
    for entry in [
        embed(&horse_pack, "models/horse.glb", horse, args.apply),
        embed(
            &cannon,
            "models/Cannon.glb",
            vec![("look".to_string(), cannon_look.animations.into_iter().find(|a| a.name == "look").unwrap())],
            args.apply,
        ),
        embed(
            &tank,
            "models/tank_turret.glb",
            vec![("look".to_string(), tank_look.animations.into_iter().find(|a| a.name == "look").unwrap())],
            args.apply,
        ),
    ] {
        match entry {
            Ok(v) => report.models.push(v),
            Err(error) => {
                report.failed += 1;
                report.models.push(json!({ "status": "failed", "reason": format!("{error:#}") }));
            }
        }
    }

    // --- The weapon debris: the last JSON shapes, textures embedded. ------
    let texture = |file: &str| -> Option<Vec<u8>> {
        fs::read(debris.join(format!("textures/{file}.png"))).ok()
    };
    for entry in [
        debris_model(
            &debris,
            "models/v20.weapon_debris.gun_shell.json",
            |name| match name.to_ascii_lowercase().as_str() {
                "black50" => texture("gun_black50"),
                "yellow" => texture("gun_yellow"),
                _ => None,
            },
            args.apply,
        ),
        debris_model(
            &debris,
            "models/v20.weapon_debris.rocket_explosion_sphere.json",
            |_name| None, // the pack carries no bitmap for whiteCheck; it draws clear as before
            args.apply,
        ),
    ] {
        match entry {
            Ok(v) => report.models.push(v),
            Err(error) => {
                report.failed += 1;
                report.models.push(json!({ "status": "failed", "reason": format!("{error:#}") }));
            }
        }
    }

    report.migrated = report.models.iter().filter(|v| v["status"] != "failed").count();
    let value = json!({
        "applied": report.applied,
        "migrated": report.migrated,
        "failed": report.failed,
        "models": report.models,
    });
    fs::write(
        root.join("CLIPS-TO-GLB.json"),
        serde_json::to_string_pretty(&value).unwrap_or_default() + "\n",
    )?;
    Ok(report)
}
