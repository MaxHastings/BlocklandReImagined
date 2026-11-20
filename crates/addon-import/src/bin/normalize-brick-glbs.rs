//! Temporary content fix-up (throwaway tool).
//!
//! Brick `.glb`s written by the old exporter carry two things an editor cannot
//! read back as material data: interior panels both-wound back toward the
//! brick's centroid riding one shared black `void-black` primitive with a
//! `bri_brick.void` marker, and signed paint offsets that fully clamp to solid
//! black or white for any player paint (each channel at or past -1/+1) yet
//! still ride the dynamic `paint_offsets` marker, so an editor sees them as
//! plain placeholders instead of the literal colour the game draws. The
//! exporter no longer writes void primitives — those quads now ride their
//! surface's placeholder like any runtime-coloured quad (the in-game look is
//! unchanged: the lowering never read a void factor as an authored colour) —
//! and this tool bakes the fully-clamped offsets to literal colours so the
//! GLB carries exactly what the game draws. It lowers every brick with
//! `brick_from_glb`, bakes, re-exports with `export_brick_with_surfaces` (the
//! built-in round-trip check guarantees no authored value is lost), and
//! replaces a file only where the bytes change: a brick with neither a void
//! primitive nor a fully-clamped offset lowers back to byte-identical output
//! and stays clean.
//!
//! A brick that no longer lowers (for instance one re-saved through Blender,
//! which drops the `bri_brick` extras) is reported with its reason and left
//! untouched — this tool never overwrites a file it cannot rebuild exactly.
//! Baking uses alpha 1.0: v20 brick paints are opaque in practice, so the
//! difference from the offset path's inherited paint alpha only touches a
//! translucent-painted brick whose offsets fully clamp.
//!
//! Usage: `normalize-brick-glbs CONTENT_ROOT [--apply] [--skip NAME ...]`
//!
//! Dry-run by default; writes NORMALIZE-BRICK-GLBS.json into the root either
//! way. The shared surface pictures come from the first `brick-materials`
//! bundle found under the content root (none, or no pre-composited copies
//! beside it: placeholders ship plain and a warning goes to stderr).
//! `--skip` names top-level content directories to leave alone.
//!
//! TEMPORARY TOOL: delete this file and its `[[bin]]` entry once content is
//! migrated off the void primitives and fully-clamped offsets.

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
        eprintln!("usage: normalize-brick-glbs CONTENT_ROOT [--apply] [--skip NAME ...]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if report.failed > 0 {
                println!(
                    "NORMALIZE-BRICK-GLBS: {} problem(s); see NORMALIZE-BRICK-GLBS.json. Nothing was written for them.",
                    report.failed
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("normalize-brick-glbs: {error:#}");
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

/// How many of this file's primitives carry the legacy `bri_brick.void`
/// marker (the old exporter's black interior-panel placeholder).
fn void_prims(json: &Value) -> usize {
    json.get("meshes")
        .and_then(Value::as_array)
        .map(|meshes| {
            meshes
                .iter()
                .filter_map(|m| m.get("primitives").and_then(Value::as_array))
                .flatten()
                .filter(|p| p["extras"]["bri_brick"].get("void") == Some(&json!(true)))
                .count()
        })
        .unwrap_or(0)
}

/// A signed paint offset whose every channel clamps to its limit for any
/// player paint — each at most -1 (solid black) or at least +1 (solid white),
/// alike on all four vertices — is paint-independent and bakes to that
/// literal colour; anything still moving with the paint stays dynamic.
fn bake_clamped_offset(colors: Option<[[f32; 4]; 4]>) -> Option<[f32; 4]> {
    let Some(cs) = colors else { return None };
    if !cs.iter().all(|c| c[3] < 0.0 && [c[0], c[1], c[2]].iter().all(|v| v.is_finite())) {
        return None; // not a uniform signed offset (or no colour at all)
    }
    if !cs.iter().all(|c| [c[0], c[1], c[2]] == [cs[0][0], cs[0][1], cs[0][2]]) {
        return None; // per-vertex varying offset: the marker cannot carry it either
    }
    let mut out = [0.0f32; 4];
    for ch in 0..3 {
        match cs[0][ch] {
            o if o <= -1.0 => out[ch] = 0.0,
            o if o >= 1.0 => out[ch] = 1.0,
            _ => return None, // this channel still moves with the paint
        }
    }
    out[3] = 1.0;
    Some(out)
}

struct FileReport {
    file: String,
    status: &'static str, // "updated", "would-update", "clean" or "failed"
    reason: Option<String>,
    baked_quads: Option<usize>,
    legacy_void_prims: Option<usize>,
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
    quads_baked: usize,
    void_prims_dropped: usize,
}

impl Report {
    fn summary_line(&self) -> String {
        format!(
            "{}: {} brick file(s) seen, {} to update ({} quad(s) fully-clamped offset -> literal colour, {} legacy void primitive(s) dropped), {} already clean, {} problem(s)",
            self.mode,
            self.bricks_seen,
            self.updated,
            self.quads_baked,
            self.void_prims_dropped,
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
                "normalize-brick-glbs: no brick-materials bundle found under {}; placeholders will ship plain",
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
        quads_baked: 0,
        void_prims_dropped: 0,
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
                    baked_quads: None,
                    legacy_void_prims: None,
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
                    baked_quads: None,
                    legacy_void_prims: None,
                });
                continue;
            }
        };
        if !is_brick(&json) {
            continue; // Not a brick file at all.
        }

        report.bricks_seen += 1;
        by_dir.entry(top.clone()).or_insert((0, 0)).1 += 1;
        let legacy_void = void_prims(&json);

        // Lower and re-export through the very code that wrote the file. The
        // id only needs to be non-empty for validation; the content index
        // binds real identities at runtime.
        let mut brick = match brick_from_glb(&bytes, &format!("normalize/{}", rel.display())) {
            Ok(b) => b,
            Err(error) => {
                report.failed += 1;
                report.files.push(FileReport {
                    file: rel.display().to_string(),
                    status: "failed",
                    reason: Some(format!("does not lower back (edited by hand?): {error:#}")),
                    baked_quads: None,
                    legacy_void_prims: Some(legacy_void),
                });
                continue;
            }
        };

        // Bake the offsets that clamp to solid black or white for any paint:
        // they are literal colours in disguise and belong in a material.
        let mut baked = 0usize;
        for q in brick.quads.iter_mut() {
            if let Some(lit) = bake_clamped_offset(q.colors) {
                q.colors = Some([lit; 4]);
                baked += 1;
            }
        }

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
                    baked_quads: None,
                    legacy_void_prims: Some(legacy_void),
                });
                continue;
            }
        };

        if packed == bytes {
            // No void primitive and no fully-clamped offset: nothing to do.
            report.clean += 1;
            report.files.push(FileReport {
                file: rel.display().to_string(),
                status: "clean",
                reason: None,
                baked_quads: Some(0),
                legacy_void_prims: Some(legacy_void),
            });
            continue;
        }

        if apply {
            fs::write(&p, &packed).with_context(|| format!("cannot write {}", p.display()))?;
        }
        report.updated += 1;
        report.quads_baked += baked;
        report.void_prims_dropped += legacy_void;
        by_dir.get_mut(&top).unwrap().0 += 1;
        report.files.push(FileReport {
            file: rel.display().to_string(),
            status: if apply { "updated" } else { "would-update" },
            reason: None,
            baked_quads: Some(baked),
            legacy_void_prims: Some(legacy_void),
        });
    }

    let text = serde_json::to_vec_pretty(&json!({
        "mode": report.mode,
        "root": report.root,
        "shared_surface_refs": report.textured,
        "quads_baked": report.quads_baked,
        "legacy_void_primitives_dropped": report.void_prims_dropped,
        "skipped_dirs": report.skipped_dirs,
        "by_top_level_dir": by_dir.into_iter().map(|(dir, (updated, seen))| json!({
            "dir": dir, "updated": updated, "seen": seen,
        })).collect::<Vec<_>>(),
        "files": report.files.iter().map(|f| {
            let mut v = json!({ "file": f.file, "status": f.status });
            if let Some(r) = &f.reason {
                v["reason"] = json!(r);
            }
            if let Some(b) = f.baked_quads {
                v["baked_quads"] = json!(b);
            }
            if let Some(vv) = f.legacy_void_prims {
                v["legacy_void_primitives"] = json!(vv);
            }
            v
        }).collect::<Vec<_>>(),
    }))
    .context("report serialisation")?;
    fs::write(root.join("NORMALIZE-BRICK-GLBS.json"), text)
        .context("cannot write the report into the content root")?;
    Ok(report)
}
