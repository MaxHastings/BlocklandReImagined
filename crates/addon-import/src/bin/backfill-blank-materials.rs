//! Temporary content fix-up (throwaway tool).
//!
//! Brick `.glb`s written before the placeholder-material change carry their
//! runtime-coloured surfaces (plain player paint and signed offsets) as
//! material-less primitives or plain placeholders, so an editor like Blender
//! sees them as white geometry with no slots. The exporter now gives every
//! such primitive a per-surface placeholder material — `blank` on the top,
//! `blank-side`, `blank-bottom-edge`, `blank-bottom-loop`, `blank-ramp`
//! elsewhere, `blank-print` for prints — each referencing the installed
//! shared stock surface picture (the pre-composited-over-white copy beside
//! the brick-materials bundle: what a default-painted brick shows in game)
//! by relative URI from the file's own folder, plus a full
//! `bri_brick.paint_offsets` marker (null per plain-paint quad), which the
//! lowering reads back as dynamic colour — the game keeps painting those
//! surfaces at runtime exactly as before and ignores the placeholder's factor
//! and texture. The picture is shared by every brick, so referencing it keeps
//! each file small; an embedded copy of it would bloat the whole set past the
//! shared geometry byte budget. This tool
//! migrates every brick file: it lowers with `brick_from_glb` and re-exports
//! with `export_brick_with_surfaces`, replacing the file only where the bytes
//! change (a brick without any runtime-coloured quad is byte-identical and
//! stays clean).
//!
//! A brick that no longer lowers (for instance one re-saved through Blender,
//! which drops the `bri_brick` extras) is reported with its reason and left
//! untouched — this tool never overwrites a file it cannot rebuild exactly.
//!
//! Usage: `backfill-blank-materials CONTENT_ROOT [--apply] [--skip NAME ...]`
//!
//! Dry-run by default; writes BACKFILL-BLANK-MATERIALS.json into the root
//! either way. The shared surface pictures come from the first
//! `brick-materials` bundle found under the content root (none, or no
//! pre-composited copies beside it: placeholders ship plain and a warning
//! goes to stderr). A reference is a few dozen bytes per file; the report
//! totals the bytes added (negative where files shrink off an earlier
//! embedded copy). `--skip` names top-level content directories to leave
//! alone.
//!
//! TEMPORARY TOOL: delete this file and its `[[bin]]` entry once content is
//! migrated onto the placeholder materials.

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
        eprintln!("usage: backfill-blank-materials CONTENT_ROOT [--apply] [--skip NAME ...]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if report.failed > 0 {
                println!(
                    "BACKFILL-BLANK-MATERIALS: {} problem(s); see BACKFILL-BLANK-MATERIALS.json. Nothing was written for them.",
                    report.failed
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("backfill-blank-materials: {error:#}");
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
/// not need the pass are decided without touching their binary data).
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

/// Is this a brick file? The asset-level `bri_brick_meta` extra is the mark.
fn is_brick(json: &Value) -> bool {
    json.get("asset")
        .and_then(|a| a.get("extras"))
        .and_then(|e| e.get("bri_brick_meta"))
        .map(Value::is_object)
        .unwrap_or(false)
}

/// How many primitives of this file reference a placeholder material
/// (`blank` or its per-surface suffixed forms).
fn blank_prims(json: &Value) -> usize {
    let Some(mats) = json.get("materials").and_then(Value::as_array) else {
        return 0;
    };
    let blank_indexes: Vec<u64> = mats
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m["name"]
                .as_str()
                .is_some_and(|n| n == "blank" || n.starts_with("blank-"))
        })
        .map(|(i, _)| i as u64)
        .collect();
    if blank_indexes.is_empty() {
        return 0;
    }
    json.get("meshes")
        .and_then(Value::as_array)
        .map(|meshes| {
            meshes
                .iter()
                .filter_map(|m| m.get("primitives").and_then(Value::as_array))
                .flatten()
                .filter(|p| blank_indexes.contains(&p["material"].as_u64().unwrap_or(u64::MAX)))
                .count()
        })
        .unwrap_or(0)
}

struct FileReport {
    file: String,
    status: &'static str, // "updated", "clean" or "failed"
    reason: Option<String>,
    blank_prims: Option<usize>,
}

struct Report {
    mode: &'static str,
    root: String,
    skipped_dirs: Vec<String>,
    textured: bool,
    files: Vec<FileReport>,
    bricks_seen: usize,
    updated: usize,
    clean: usize,
    failed: usize,
    bytes_added: i64,
}

impl Report {
    fn summary_line(&self) -> String {
        format!(
            "{}: {} brick file(s) seen, {} gained placeholder materials ({} primitive(s), {} byte(s) added), {} already clean, {} problem(s)",
            self.mode,
            self.bricks_seen,
            self.updated,
            self.files.iter().filter_map(|f| f.blank_prims).sum::<usize>(),
            self.bytes_added,
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
                "backfill-blank-materials: no brick-materials bundle found under {}; placeholders will ship plain",
                root.display()
            );
            SurfaceTextures::default()
        }
    };

    let textured = surfaces.top.is_some() && surfaces.side.is_some();

    let mut report = Report {
        mode: if apply { "apply" } else { "dry-run" },
        root: root.display().to_string(),
        skipped_dirs: skip.clone(),
        textured,
        files: Vec::new(),
        bricks_seen: 0,
        updated: 0,
        clean: 0,
        failed: 0,
        bytes_added: 0,
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
                    blank_prims: None,
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
                    blank_prims: None,
                });
                continue;
            }
        };
        if !is_brick(&json) {
            continue; // Not a brick file at all.
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
                    blank_prims: None,
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
                    blank_prims: None,
                });
                continue;
            }
        };

        if packed == bytes {
            // Already migrated (or no runtime-coloured quad): nothing to do.
            report.clean += 1;
            report.files.push(FileReport {
                file: rel.display().to_string(),
                status: "clean",
                reason: None,
                blank_prims: Some(blank_prims(&json)),
            });
            continue;
        }

        let new_json = glb_json(&packed).expect("the exporter's own output parses");
        if apply {
            fs::write(&p, &packed).with_context(|| format!("cannot write {}", p.display()))?;
        }
        report.bytes_added += i64::try_from(packed.len()).unwrap_or(i64::MAX) - i64::try_from(bytes.len()).unwrap_or(0);
        report.updated += 1;
        by_dir.get_mut(&top).unwrap().0 += 1;
        report.files.push(FileReport {
            file: rel.display().to_string(),
            status: if apply { "updated" } else { "would-update" },
            reason: None,
            blank_prims: Some(blank_prims(&new_json)),
        });
    }

    let text = serde_json::to_vec_pretty(&json!({
        "mode": report.mode,
        "root": report.root,
        "shared_surface_refs": report.textured,
        "bytes_added": report.bytes_added,
        "skipped_dirs": report.skipped_dirs,
        "by_top_level_dir": by_dir.into_iter().map(|(dir, (updated, seen))| json!({
            "dir": dir, "updated": updated, "seen": seen,
        })).collect::<Vec<_>>(),
        "files": report.files.iter().map(|f| {
            let mut v = json!({ "file": f.file, "status": f.status });
            if let Some(r) = &f.reason {
                v["reason"] = json!(r);
            }
            if let Some(b) = f.blank_prims {
                v["blank_material_primitives"] = json!(b);
            }
            v
        }).collect::<Vec<_>>(),
    }))
    .context("report serialisation")?;
    fs::write(root.join("BACKFILL-BLANK-MATERIALS.json"), text)
        .context("cannot write the report into the content root")?;
    Ok(report)
}
