//! Temporary content fix-up (throwaway tool).
//!
//! Brick `.glb`s written before the colour-channel overhaul still carry their
//! authored colours in the legacy `bri_brick.colors` extras dump. The game now
//! reads colours from the glTF channels themselves — a material
//! `baseColorFactor` for uniform quads, per-vertex `COLOR_0` where they vary —
//! with signed paint offsets riding in a `bri_brick.paint_offsets` marker.
//! This tool migrates every file that still carries a legacy dump: it lowers
//! the brick with `brick_from_glb` (which reads pre-migration files through
//! the documented fallback) and re-exports it with
//! `export_brick_with_surfaces`, which writes the channels, adds the markers
//! where needed, drops the dump, and gives runtime-coloured surfaces their
//! per-surface placeholder materials referencing the installed shared stock
//! surface pictures by relative URI from the file's own folder.
//! The file is replaced only where the bytes change; a brick whose colours
//! are all plain player paint carries no dump at all and is left alone.
//!
//! A brick that no longer lowers (for instance one re-saved through Blender,
//! which drops the `bri_brick` extras) is reported with its reason and left
//! untouched — this tool never overwrites a file it cannot rebuild exactly.
//!
//! Usage: `backfill-brick-materials CONTENT_ROOT [--apply] [--skip NAME ...]`
//!
//! Dry-run by default; writes BACKFILL-BRICK-MATERIALS.json into the root
//! either way. `--skip` names top-level content directories to leave alone,
//! e.g. `--skip render-pack --skip surda-pack`. Skipped packs keep working:
//! the loader's legacy fallback reads their dumps until they are migrated too.
//!
//! TEMPORARY TOOL: delete this file and its `[[bin]]` entry once content is
//! migrated off the legacy colour dump.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail, ensure};
use bri_addon_import::installed_brick_surfaces;
use bri_gltf::{brick_from_glb, export_brick_with_surfaces, SurfaceTextures};
use serde_json::{Value, json};

struct Args {
    root: PathBuf,
    apply: bool,
    skip: Vec<String>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(args) = parse_args(&args) else {
        eprintln!("usage: backfill-brick-materials CONTENT_ROOT [--apply] [--skip NAME ...]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if report.failed > 0 {
                println!(
                    "BACKFILL-BRICK-MATERIALS: {} problem(s); see BACKFILL-BRICK-MATERIALS.json. Nothing was written for them.",
                    report.failed
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("backfill-brick-materials: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Result<Args> {
    let mut root = None;
    let mut apply = false;
    let mut skip = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--apply" => apply = true,
            "--skip" => {
                let name = it
                    .next()
                    .with_context(|| "missing value for --skip")?;
                skip.push(name.clone());
            }
            other if !other.starts_with('-') => {
                ensure!(root.is_none(), "unexpected argument: {other}");
                root = Some(PathBuf::from(other));
            }
            _ => bail!("unknown flag: {a}"),
        }
    }
    let Some(root) = root else {
        bail!("missing CONTENT_ROOT")
    };
    let root = root.canonicalize().context("content root does not exist")?;
    Ok(Args { root, apply, skip })
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().and_then(|n| n.to_str()) == Some(".downloads") {
                    continue; // Staged downloads are not installed content.
                }
                stack.push(p);
            } else if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}

/// The GLB's JSON chunk (a brick is recognised by it alone, so files that do
/// not need baking are decided without touching their binary data).
fn glb_json(bytes: &[u8]) -> Result<Value> {
    ensure!(bytes.starts_with(b"glTF"), "not a glb");
    let length = u32le(bytes, 8) as usize;
    ensure!(length <= bytes.len(), "truncated glb");
    let mut json_chunk: Option<&[u8]> = None;
    let mut off = 12usize;
    while off + 8 <= length {
        let (clen, ctype) = (u32le(bytes, off) as usize, u32le(bytes, off + 4));
        off += 8;
        if ctype == 0x4E4F534A && clen > 0 {
            json_chunk = Some(&bytes[off..off + clen]); // JSON
        }
        off += (clen + 3) & !3;
    }
    let chunk = json_chunk.context("the glb has no JSON chunk")?;
    Ok(serde_json::from_slice(chunk)?)
}

fn u32le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Does this brick file still carry the legacy `bri_brick.colors` dump? That
/// is exactly what the migration rewrites into glTF channels and markers.
fn carries_legacy_colours(json: &Value) -> bool {
    let Some(meta) = json.get("asset").and_then(|a| a.get("extras")).and_then(|e| e.get("bri_brick_meta")) else {
        return false; // Not a brick file at all.
    };
    if !meta.is_object() {
        return false;
    }
    let Some(meshes) = json.get("meshes").and_then(Value::as_array) else {
        return true; // A brick with no meshes is broken anyway; let the lowering report it.
    };
    for mesh in meshes {
        let Some(prims) = mesh.get("primitives").and_then(Value::as_array) else {
            continue;
        };
        for p in prims {
            if p
                .get("extras")
                .and_then(|e| e.get("bri_brick"))
                .and_then(|b| b.get("colors"))
                .is_some()
            {
                return true; // The legacy dump is still here.
            }
        }
    }
    false
}

struct FileReport {
    file: String,
    status: &'static str, // "updated", "clean" or "failed"
    reason: Option<String>,
    materials: Option<usize>,
    /// Transparent (BLEND) materials among them.
    blended: Option<usize>,
    /// Primitives that carry a per-vertex COLOR_0 instead of a material
    /// (authored gradients; none in most files).
    color_attr_prims: Option<usize>,
}

struct Report {
    mode: &'static str,
    root: String,
    skipped_dirs: Vec<String>,
    files: Vec<FileReport>,
    bricks_seen: usize,
    updated: usize,
    clean: usize,
    failed: usize,
}

impl Report {
    fn summary_line(&self) -> String {
        format!(
            "{}: {} brick file(s) still on the legacy colour dump, {} migrated to glTF channels/markers ({} material(s), {} transparent), {} already clean, {} problem(s)",
            self.mode,
            self.bricks_seen,
            self.updated,
            self.files.iter().filter_map(|f| f.materials).sum::<usize>(),
            self.files.iter().filter_map(|f| f.blended).sum::<usize>(),
            self.clean,
            self.failed
        )
    }
}

fn run(args: &Args) -> Result<Report> {
    let root = &args.root;
    let apply = args.apply;
    let skip = &args.skip;

    // The stock surface pictures the placeholders will carry, once for every
    // file (the bundle lives under the content root; absent: plain factors).
    let surfaces = match installed_brick_surfaces(root) {
        Some(s) => s,
        None => {
            eprintln!(
                "backfill-brick-materials: no brick-materials bundle found under {}; placeholders will ship plain",
                root.display()
            );
            SurfaceTextures::default()
        }
    };

    let mut report = Report {
        mode: if apply { "apply" } else { "dry-run" },
        root: root.display().to_string(),
        skipped_dirs: skip.clone(),
        files: Vec::new(),
        bricks_seen: 0,
        updated: 0,
        clean: 0,
        failed: 0,
    };

    // A per-file note for the report's "by top-level directory" roll-up.
    let mut by_dir: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // dir -> (updated, seen)

    for p in walk(root) {
        if p.extension().and_then(|e| e.to_str()) != Some("glb") {
            continue;
        }
        let rel = p.strip_prefix(root).unwrap_or(&p);
        let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
        if skip.iter().any(|s| s == &top) {
            continue; // A protected pack; the user decides separately.
        }

        let bytes = match fs::read(&p).with_context(|| format!("{} does not read", p.display())) {
            Ok(b) => b,
            Err(error) => {
                report.failed += 1;
                report.files.push(FileReport {
                    file: rel.display().to_string(),
                    status: "failed",
                    reason: Some(format!("{error:#}")),
                    materials: None,
                    blended: None,
                    color_attr_prims: None,
                });
                continue;
            }
        };
        let json = match glb_json(&bytes) {
            Ok(j) => j,
            Err(error) => {
                report.failed += 1;
                report.files.push(FileReport {
                    file: rel.display().to_string(),
                    status: "failed",
                    reason: Some(format!("JSON chunk: {error:#}")),
                    materials: None,
                    blended: None,
                    color_attr_prims: None,
                });
                continue;
            }
        };
        if !carries_legacy_colours(&json) {
            continue; // Not a brick, or already migrated off the dump.
        }

        report.bricks_seen += 1;
        by_dir.entry(top.clone()).or_insert((0, 0)).1 += 1;

        // Lower and re-export through the very code that wrote the file. The
        // id only needs to be non-empty for validation; the content index
        // binds real identities at runtime.
        let brick = match brick_from_glb(&bytes, &format!("backfill/{}", rel.display())) {
            Ok(b) => b,
            Err(error) => {
                report.failed += 1;
                report.files.push(FileReport {
                    file: rel.display().to_string(),
                    status: "failed",
                    reason: Some(format!("does not lower back (edited by hand?): {error:#}")),
                    materials: None,
                    blended: None,
                    color_attr_prims: None,
                });
                continue;
            }
        };
        // The file's own folder under the content root, so its placeholders
        // reference the shared pictures by path from there.
        let packed = match export_brick_with_surfaces(&brick, &surfaces, rel.parent()) {
            Ok(b) => b,
            Err(error) => {
                report.failed += 1;
                report.files.push(FileReport {
                    file: rel.display().to_string(),
                    status: "failed",
                    reason: Some(format!("re-export failed: {error:#}")),
                    materials: None,
                    blended: None,
                    color_attr_prims: None,
                });
                continue;
            }
        };

        // Count what the new file gained, for the report.
        let new_json = glb_json(&packed).expect("the exporter's own output parses");
        let materials = new_json.get("materials").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
        let blended = new_json
            .get("materials")
            .and_then(Value::as_array)
            .map(|mats| {
                mats.iter()
                    .filter(|m| m.get("alphaMode").and_then(Value::as_str) == Some("BLEND"))
                    .count()
            })
            .unwrap_or(0);
        let color_attr_prims = new_json
            .get("meshes")
            .and_then(Value::as_array)
            .map(|meshes| {
                meshes
                    .iter()
                    .filter_map(|m| m.get("primitives").and_then(Value::as_array))
                    .flatten()
                    .filter(|p| p.get("attributes").and_then(|a| a.get("COLOR_0")).is_some())
                    .count()
            })
            .unwrap_or(0);

        if packed == bytes {
            // Nothing to bake after all (defensive; needs_baking said yes).
            report.clean += 1;
            report.files.push(FileReport {
                file: rel.display().to_string(),
                status: "clean",
                reason: None,
                materials: Some(0),
                blended: Some(0),
                color_attr_prims: Some(0),
            });
            continue;
        }

        if apply {
            fs::write(&p, &packed).with_context(|| format!("cannot write {}", p.display()))?;
        }
        report.updated += 1;
        by_dir.get_mut(&top).unwrap().0 += 1;
        report.files.push(FileReport {
            file: rel.display().to_string(),
            status: if apply { "updated" } else { "would-update" },
            reason: None,
            materials: Some(materials),
            blended: Some(blended),
            color_attr_prims: Some(color_attr_prims),
        });
    }

    let text = serde_json::to_vec_pretty(&json!({
        "mode": report.mode,
        "root": report.root,
        "skipped_dirs": report.skipped_dirs,
        "by_top_level_dir": by_dir.into_iter().map(|(dir, (updated, seen))| json!({
            "dir": dir, "updated": updated, "seen": seen,
        })).collect::<Vec<_>>(),
        "files": report.files.iter().map(|f| {
            let mut v = json!({ "file": f.file, "status": f.status });
            if let Some(r) = &f.reason {
                v["reason"] = json!(r);
            }
            if let Some(m) = f.materials {
                v["materials"] = json!(m);
                v["transparent_materials"] = json!(f.blended.unwrap_or(0));
                v["vertex_colour_primitives"] = json!(f.color_attr_prims.unwrap_or(0));
            }
            v
        }).collect::<Vec<_>>(),
    }))
    .context("report serialisation")?;
    fs::write(root.join("BACKFILL-BRICK-MATERIALS.json"), text)
        .context("cannot write the report into the content root")?;
    Ok(report)
}
