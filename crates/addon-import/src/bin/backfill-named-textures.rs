//! Temporary content fix-up (throwaway tool).
//!
//! Migrated models whose materials name their pictures (`grey`, `black50`,
//! ...) draw correctly in game — the client resolves each material's name to
//! a picture file shipped beside it, exactly as Torque did — but an editor
//! like Blender sees only glTF's `baseColorFactor`, and these files carry
//! none: every such part looks white. This tool closes that gap for models
//! whose pictures are named after their materials (the way the client finds
//! them): one solid colour becomes the material's `baseColorFactor` (a few
//! bytes of JSON, no picture), a patterned picture is embedded in the file
//! as-is, and a material with no picture but an authored
//! `extras.bri_legacy.color` bakes that colour. Either way the `.glb` looks
//! like it does in game: at runtime the installed picture or the legacy
//! colour still wins where present, so nothing on screen changes.
//!
//! A material named `blank` (case-insensitive) is paintable at runtime and
//! stays white on purpose; materials that already carry a texture or a
//! non-white factor are left alone. Geometry buffers are copied through
//! byte-for-byte — only the JSON chunk, the buffer length and appended image
//! data change. Every file it would write is lowered with `bri_gltf::lower`
//! (the game's own path) before anything touches disk; a file that no longer
//! lowers is reported and left untouched.
//!
//! Usage: `backfill-named-textures CONTENT_ROOT [--apply]`
//!
//! Dry-run by default; writes BACKFILL-NAMED-TEXTURES.json into the root
//! either way.
//!
//! TEMPORARY TOOL: delete this file and its `[[bin]]` entry once content is
//! backfilled.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail, ensure};
use image::ImageEncoder;
use serde_json::{Value, json};

const MAX_TEXTURE: u64 = 16 * 1024 * 1024;

struct Args {
    root: PathBuf,
    apply: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(args) = parse_args(&args) else {
        eprintln!("usage: backfill-named-textures CONTENT_ROOT [--apply]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if report.failed > 0 {
                println!(
                    "BACKFILL-NAMED-TEXTURES: {} problem(s); see BACKFILL-NAMED-TEXTURES.json. Nothing was written for them.",
                    report.failed
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("backfill-named-textures: {error:#}");
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

/// The GLB's JSON and binary chunks.
fn split_glb(bytes: &[u8]) -> Result<(Value, Vec<u8>)> {
    ensure!(bytes.starts_with(b"glTF"), "not a glb");
    let length = u32le(bytes, 8) as u64;
    ensure!(length as usize <= bytes.len(), "truncated glb");
    let mut json_chunk: Option<Vec<u8>> = None;
    let mut bin_chunk: Vec<u8> = Vec::new();
    let mut off = 12usize;
    while off + 8 <= length as usize {
        let (clen, ctype) = (u32le(bytes, off) as usize, u32le(bytes, off + 4));
        off += 8;
        match ctype {
            0x4E4F534A => json_chunk = Some(bytes[off..off + clen].to_vec()), // JSON
            0x004E4942 => bin_chunk.extend_from_slice(&bytes[off..off + clen]), // BIN
            _ => {}
        }
        off += (clen + 3) & !3;
    }
    let json = serde_json::from_slice(json_chunk.as_deref().unwrap_or(b"null"))?;
    Ok((json, bin_chunk))
}

fn u32le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// A picture's sRGB byte as glTF's linear factor channel.
fn linear_byte(b: u8) -> f32 {
    let c = b as f32 / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Re-pack the GLB: original binary chunk first, then each appended picture.
/// The appends' offsets are worked out before packing and written into
/// `json`, which also takes its final buffer length; one pack is enough.
fn build_glb(json: &mut Value, old_bin: &[u8], appends: &[(usize, Vec<u8>)]) -> Result<Vec<u8>> {
    // Where each appended picture lands, 4-aligned after the last.
    let mut offsets = vec![0usize; appends.len()];
    let mut len = old_bin.len();
    for (k, (_, data)) in appends.iter().enumerate() {
        while len % 4 != 0 {
            len += 1;
        }
        offsets[k] = len;
        len += data.len();
    }
    if let Some(bvs) = json.get_mut("bufferViews").and_then(Value::as_array_mut) {
        for (k, (view, _)) in appends.iter().enumerate() {
            bvs[*view]["byteOffset"] = json!(offsets[k]);
        }
    }
    if let Some(bufs) = json.get_mut("buffers").and_then(Value::as_array_mut) {
        if !bufs.is_empty() {
            bufs[0]["byteLength"] = json!(len);
        }
    }
    let mut j = serde_json::to_vec(json)?;
    while j.len() % 4 != 0 {
        j.push(b' '); // JSON chunk pads with spaces
    }
    let mut bin = old_bin.to_vec();
    for (k, (_, data)) in appends.iter().enumerate() {
        while bin.len() < offsets[k] {
            bin.push(0);
        }
        bin.extend_from_slice(data);
    }
    ensure!(bin.len() == len, "appended layout drifted");
    let total = 12 + 8 + j.len() + if bin.is_empty() { 0 } else { 8 + bin.len() };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
    out.extend_from_slice(&j);
    if !bin.is_empty() {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x004E4942u32.to_le_bytes());
        out.extend_from_slice(&bin);
    }
    Ok(out)
}

/// Push into one of the GLB JSON's top-level arrays, creating it where
/// absent; the new entry's index.
fn push_array(json: &mut Value, key: &str, value: Value) -> usize {
    if !json.get(key).and_then(Value::as_array).is_some() {
        json[key] = json!([]);
    }
    let a = json[key].as_array_mut().unwrap();
    a.push(value);
    a.len() - 1
}

/// What the picture named after a material resolves to.
enum NamedPicture {
    /// One solid colour (the sRGB pixel).
    Solid([u8; 4], String),
    /// A patterned picture as PNG bytes, ready to embed in the GLB.
    Embedded(Vec<u8>, String),
}

/// The picture files inside the model's own pack whose name is the material's
/// (case-insensitive, any image extension): beside the model first, then
/// anywhere in the pack — the same order the client tries at runtime. Several
/// providers must be byte-identical or the reference is ambiguous and refused.
fn named_picture(
    pictures: &BTreeMap<String, Vec<PathBuf>>,
    pack_dir: &Path,
    glb_path: &Path,
    name: &str,
) -> Result<Option<NamedPicture>> {
    let stem = name.to_ascii_lowercase();
    if stem.is_empty() {
        return Ok(None); // No file can carry an empty base name.
    }
    // Confine the tree-wide index to this model's own pack.
    let Some(paths) = pictures.get(&stem) else {
        return Ok(None);
    };
    let candidates: Vec<&PathBuf> = paths.iter().filter(|p| p.starts_with(pack_dir)).collect();
    if candidates.is_empty() {
        return Ok(None);
    }
    // Beside the model wins; everything else must agree byte-for-byte.
    let glb_dir = glb_path.parent().unwrap_or(pack_dir);
    let beside: Vec<&PathBuf> = candidates.iter().filter(|p| p.parent() == Some(glb_dir)).copied().collect();
    let pool: &[&PathBuf] = if !beside.is_empty() { &beside } else { &candidates };

    let mut first_bytes: Option<Vec<u8>> = None;
    for p in pool {
        let bytes = fs::read(p).with_context(|| format!("{} does not read", p.display()))?;
        match first_bytes.as_ref() {
            Some(first) => ensure!(*first == bytes, "the picture named `{name}` is provided with different bytes by several files"),
            None => first_bytes = Some(bytes),
        }
    }
    let first = first_bytes.unwrap();

    // The report names the file that won.
    let source = pool.iter().find(|p| p.parent() == Some(glb_dir)).unwrap_or(&pool[0]).to_path_buf();

    let img = image::load_from_memory(&first)
        .with_context(|| format!("the picture named `{name}` is not a decodable image"))?;
    let rgba = img.to_rgba8();
    let pixel = *rgba.pixels().next().unwrap();
    if rgba.pixels().all(|px| *px == pixel) {
        return Ok(Some(NamedPicture::Solid(
            [pixel[0], pixel[1], pixel[2], pixel[3]],
            source.display().to_string(),
        )));
    }
    // A patterned picture rides in as-is (PNG where it is not already one).
    let png = if first.starts_with(b"\x89PNG") {
        first.clone()
    } else {
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ColorType::Rgba8.into())?;
        out
    };
    Ok(Some(NamedPicture::Embedded(png, source.display().to_string())))
}

struct MaterialFix {
    index: usize,
    name: String,
    source_file: String,
    kind: &'static str, // "solid", "embedded" or "legacy-colour"
    /// The bufferView the embedded picture appends to (none for a solid).
    buffer_view: Option<usize>,
    bytes: Vec<u8>,
    /// A solid colour as glTF's linear baseColorFactor (solids only).
    factor: Option<[f32; 4]>,
}

struct ModelReport {
    file: String,
    fixes: Vec<MaterialFix>,
    notes: Vec<String>,
    validated: bool,
}

struct Report {
    mode: &'static str,
    root: String,
    models: Vec<ModelReport>,
    fixed_materials: usize,
    failed: usize,
}

impl Report {
    fn summary_line(&self) -> String {
        format!(
            "{}: {} model(s) touched, {} material(s) backfilled ({} solid, {} embedded, {} legacy-colour), {} problem(s)",
            self.mode,
            self.models.len(),
            self.fixed_materials,
            self.models.iter().map(|m| m.fixes.iter().filter(|f| f.kind == "solid").count()).sum::<usize>(),
            self.models.iter().map(|m| m.fixes.iter().filter(|f| f.kind == "embedded").count()).sum::<usize>(),
            self.models.iter().map(|m| m.fixes.iter().filter(|f| f.kind == "legacy-colour").count()).sum::<usize>(),
            self.failed
        )
    }
}

fn run(args: &Args) -> Result<Report> {
    let root = &args.root;
    let apply = args.apply;

    // Every picture in the tree, by lower-case stem, so a model's pack can be
    // searched without re-walking it per material.
    let mut pictures: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for p in walk(root) {
        let Some(file_name) = p.file_name().and_then(|n| n.to_str()) else { continue };
        let Some((base, ext)) = file_name.rsplit_once('.') else { continue };
        if !["png", "jpg", "jpeg"].contains(&ext.to_ascii_lowercase().as_str()) {
            continue;
        }
        pictures.entry(base.to_ascii_lowercase()).or_default().push(p);
    }

    let mut report = Report {
        mode: if apply { "apply" } else { "dry-run" },
        root: root.display().to_string(),
        models: Vec::new(),
        fixed_materials: 0,
        failed: 0,
    };

    for p in walk(root) {
        if p.extension().and_then(|e| e.to_str()) != Some("glb") {
            continue;
        }
        let bytes = match fs::read(&p) {
            Ok(b) => b,
            Err(error) => {
                report.failed += 1;
                report.models.push(ModelReport {
                    file: p.strip_prefix(root).unwrap_or(&p).display().to_string(),
                    fixes: Vec::new(),
                    notes: vec![format!("{error:#}")],
                    validated: false,
                });
                continue;
            }
        };
        let (mut json, bin) = match split_glb(&bytes) {
            Ok(x) => x,
            Err(error) => {
                report.failed += 1;
                report.models.push(ModelReport {
                    file: p.strip_prefix(root).unwrap_or(&p).display().to_string(),
                    fixes: Vec::new(),
                    notes: vec![format!("{error:#}")],
                    validated: false,
                });
                continue;
            }
        };
        let Some(mats) = json.get("materials").and_then(Value::as_array) else {
            continue; // No materials to fix.
        };

        // The top-level pack directory the picture search is confined to:
        // a model may only lean on pictures its own Add-On ships.
        let rel = p.strip_prefix(root).unwrap_or(&p);
        let pack_dir = match rel.components().next() {
            Some(c) => root.join(c.as_os_str()),
            None => continue, // A .glb directly under the root has no pack.
        };

        let mut fixes: Vec<MaterialFix> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        for (i, mat) in mats.iter().enumerate() {
            let Some(pbr) = mat.get("pbrMetallicRoughness") else { continue };
            if pbr.get("baseColorTexture").is_some() {
                continue; // Already carries its picture.
            }
            let name = mat.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
            if name.is_empty() {
                notes.push(format!("material {} has no name to look up", i));
                continue;
            }
            let bcf: [f32; 4] = pbr["baseColorFactor"]
                .as_array()
                .map(|a| a.iter().map(|v| v.as_f64().unwrap_or(1.0) as f32).collect::<Vec<_>>())
                .filter(|v: &Vec<f32>| v.len() == 4)
                .map(|v| [v[0], v[1], v[2], v[3]])
                .unwrap_or([1.0; 4]);
            let white = bcf.iter().all(|c| (c - 1.0).abs() < 1e-6);
            if !white {
                continue; // Already carries a colour of its own.
            }

            match named_picture(&pictures, &pack_dir, &p, &name) {
                Ok(Some(NamedPicture::Solid(pixel, source_file))) => {
                    if name.eq_ignore_ascii_case("blank") {
                        notes.push(format!("material `{name}` is paintable; it stays white"));
                        continue;
                    }
                    let factor = [
                        linear_byte(pixel[0]),
                        linear_byte(pixel[1]),
                        linear_byte(pixel[2]),
                        1.0, // the game flattens opaque blends to solid
                    ];
                    if factor.iter().zip(bcf).all(|(a, b)| (a - b).abs() < 1e-6) {
                        continue; // The factor already says this colour.
                    }
                    fixes.push(MaterialFix {
                        index: i,
                        name: name.clone(),
                        source_file,
                        kind: "solid",
                        buffer_view: None,
                        bytes: Vec::new(),
                        factor: Some(factor),
                    });
                }
                Ok(Some(NamedPicture::Embedded(png, source_file))) => {
                    if (png.len() as u64) > MAX_TEXTURE {
                        report.failed += 1;
                        notes.push(format!("material `{name}`'s picture is too large to embed"));
                        continue;
                    }
                    fixes.push(MaterialFix {
                        index: i,
                        name: name.clone(),
                        source_file,
                        kind: "embedded",
                        buffer_view: None, // assigned by pass 2
                        bytes: png,
                        factor: None,
                    });
                }
                Ok(None) => {
                    // No picture named after it in the pack. An authored
                    // legacy colour still says what solid it is.
                    let Some(color) = mat["extras"]["bri_legacy"]["color"]
                        .as_array()
                        .filter(|a| a.len() == 4)
                    else {
                        if !name.eq_ignore_ascii_case("blank") && !name.starts_with("brick-") {
                            notes.push(format!("material `{name}` has no picture in the pack and no legacy colour"));
                        }
                        continue;
                    };
                    let baked = [
                        color[0].as_f64().unwrap_or(1.0) as f32,
                        color[1].as_f64().unwrap_or(1.0) as f32,
                        color[2].as_f64().unwrap_or(1.0) as f32,
                        1.0, // the game flattens opaque blends to solid
                    ];
                    if baked.iter().zip(bcf).all(|(a, b)| (a - b).abs() < 1e-6) {
                        continue; // The factor already says this colour.
                    }
                    fixes.push(MaterialFix {
                        index: i,
                        name: name.clone(),
                        source_file: "the material's own bri_legacy.color".into(),
                        kind: "legacy-colour",
                        buffer_view: None,
                        bytes: Vec::new(),
                        factor: Some(baked),
                    });
                }
                Err(error) => {
                    report.failed += 1;
                    notes.push(format!("material `{name}`: {error:#}"));
                }
            }
        }

        if fixes.is_empty() {
            continue; // Nothing to do for this model.
        }

        // Pass 2 writes the decisions into the GLB's JSON; each embedded
        // picture gets its bufferView and texture index here, so build_glb
        // can pack them in one go.
        for f in &mut fixes {
            if let Some(factor) = f.factor {
                json["materials"][f.index]["pbrMetallicRoughness"]["baseColorFactor"] = json!(factor);
            } else {
                // The arrays are appended before the material is touched,
                // so no borrow of it survives across push_array.
                let bv = push_array(
                    &mut json,
                    "bufferViews",
                    json!({ "buffer": 0, "byteOffset": -1i64, "byteLength": f.bytes.len() }),
                );
                let img_index = push_array(&mut json, "images", json!({ "bufferView": bv, "mimeType": "image/png" }));
                let tex_index = push_array(&mut json, "textures", json!({ "source": img_index }));
                let mat = &mut json["materials"][f.index];
                mat["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": tex_index });
                mat["pbrMetallicRoughness"]["baseColorFactor"] = json!([1.0, 1.0, 1.0, 1.0]); // the picture carries the colour
                f.buffer_view = Some(bv);
            }
        }

        let appends: Vec<(usize, Vec<u8>)> = fixes
            .iter()
            .filter_map(|f| f.buffer_view.map(|v| (v, f.bytes.clone())))
            .collect();
        let packed = build_glb(&mut json, &bin, &appends)?;

        // The game's own lowering must accept the result before it is offered.
        let id = p.strip_prefix(root).unwrap_or(&p).display().to_string();
        match bri_gltf::lower(&packed, &id) {
            Ok(_) => {}
            Err(error) => {
                report.failed += 1;
                notes.push(format!("the rewritten file no longer lowers: {error:#}"));
                let rel = p.strip_prefix(root).unwrap_or(&p);
                report.models.push(ModelReport {
                    file: rel.display().to_string(),
                    fixes,
                    notes,
                    validated: false,
                });
                continue; // Leave the original on disk.
            }
        }

        report.fixed_materials += fixes.len();
        if apply {
            fs::write(&p, packed).with_context(|| format!("cannot write {}", p.display()))?;
        }
        let rel = p.strip_prefix(root).unwrap_or(&p);
        report.models.push(ModelReport {
            file: rel.display().to_string(),
            fixes,
            notes,
            validated: true,
        });
    }

    let text = serde_json::to_vec_pretty(&json!({
        "mode": report.mode,
        "root": report.root,
        "models": report.models.iter().map(|m| json!({
            "file": m.file,
            "validated": m.validated,
            "materials": m.fixes.iter().map(|f| json!({
                "index": f.index,
                "name": f.name,
                "source_file": f.source_file,
                "kind": f.kind,
                "bytes": f.bytes.len(),
            })).collect::<Vec<_>>(),
            "notes": m.notes,
        })).collect::<Vec<_>>(),
    }))?;
    fs::write(root.join("BACKFILL-NAMED-TEXTURES.json"), text)?;
    Ok(report)
}
