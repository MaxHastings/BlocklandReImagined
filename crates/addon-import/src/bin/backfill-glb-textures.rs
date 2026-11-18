//! Temporary content fix-up (throwaway tool).
//!
//! Converted model `.glb`s carry no textures: Blockland models name their
//! pictures and the game resolves them at runtime, so the migration could only
//! embed what each pack shipped itself. This tool closes that gap for the
//! models whose presentation tables still record the per-material references
//! (`presentation.json` `models[*].textures[i]`): a reference to one solid
//! colour becomes glTF's `baseColorFactor` on the material (a few bytes of
//! JSON, no picture), and a patterned picture is embedded in the file as-is.
//! Either way the `.glb` looks identical in Blender and in game: at runtime
//! the presentation binding still wins where it names an installed file.
//!
//! Geometry buffers are copied through byte-for-byte — only the JSON chunk,
//! the buffer length and appended image data change. Materials that already
//! carry a texture, or whose reference is empty (`""` means "untextured on
//! purpose"), are left alone. References resolve by their presentation-table
//! id (the same way the game does at runtime); an id provided by several packs
//! must be byte-identical or it is ambiguous and skipped with its reason in
//! the report.
//!
//! Usage: `backfill-glb-textures CONTENT_ROOT [--apply]`
//!
//! Dry-run by default; writes BACKFILL-REPORT.json into the root either way.
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
        eprintln!("usage: backfill-glb-textures CONTENT_ROOT [--apply]");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(report) => {
            println!("{}", report.summary_line());
            if report.failed > 0 {
                println!(
                    "BACKFILL-REPORT: {} problem(s); see BACKFILL-REPORT.json. Nothing was written for them.",
                    report.failed
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("backfill-glb-textures: {error:#}");
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

/// Every installed picture, by the texture id its presentation table gives it
/// (`base/data/shapes/blank.png`). That is exactly how the game resolves a
/// model's texture references at runtime, so this tool does too.
fn texture_index(root: &Path) -> BTreeMap<String, Vec<PathBuf>> {
    let mut index: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for p in walk(root) {
        if p.file_name().and_then(|n| n.to_str()) != Some("presentation.json") {
            continue;
        }
        let Ok(raw) = fs::read(&p) else { continue };
        let Ok(value): serde_json::Result<Value> = serde_json::from_slice(&raw) else {
            continue; // Not a presentation table (or unreadable); nothing to do.
        };
        let Some(table_dir) = p.parent() else { continue };
        let Some(textures) = value.get("textures").and_then(Value::as_object) else {
            continue; // A table without pictures.
        };
        for (id, entry) in textures {
            let Some(file) = entry.get("file").and_then(Value::as_str) else { continue };
            let path = table_dir.join(file);
            if path.is_file() {
                index.entry(id.clone()).or_default().push(path);
            }
        }
    }
    for v in index.values_mut() {
        v.sort();
        v.dedup();
    }
    index
}

struct MaterialFix {
    index: usize,
    name: String,
    reference: String,
    source_file: String,
    kind: &'static str, // "solid" or "embedded"
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
            "{}: {} model(s) touched, {} material(s) backfilled ({} solid, {} embedded), {} problem(s)",
            self.mode,
            self.models.len(),
            self.fixed_materials,
            self.models.iter().map(|m| m.fixes.iter().filter(|f| f.kind == "solid").count()).sum::<usize>(),
            self.models.iter().map(|m| m.fixes.iter().filter(|f| f.kind == "embedded").count()).sum::<usize>(),
            self.failed
        )
    }
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

/// What a texture reference resolves to, and where it came from.
enum Resolved {
    /// Every installed copy is one solid colour (the sRGB pixel).
    Solid([u8; 4], String),
    /// A patterned picture as PNG bytes, ready to embed in the GLB.
    Embedded(Vec<u8>, String),
}

/// The installed file a texture reference names, by its presentation-table id.
/// Files inside the model's own pack win over other packs; several providers
/// must be byte-identical or the reference is ambiguous. The picture is then
/// classified by decoded content: one solid colour needs no bytes at all.
fn resolve_reference(
    index: &BTreeMap<String, Vec<PathBuf>>,
    root: &Path,
    glb_dir: &Path,
    reference: &str,
) -> Result<Option<Resolved>> {
    let Some(candidates) = index.get(reference) else {
        return Ok(None); // No installed table provides this texture id.
    };
    let pack: Vec<u8> = glb_dir
        .strip_prefix(root)
        .unwrap_or(glb_dir)
        .components()
        .next()
        .map(|c| c.as_os_str().as_encoded_bytes().to_vec())
        .unwrap_or_default();
    let local: Vec<&PathBuf> = candidates
        .iter()
        .filter(|p| {
            p.strip_prefix(root)
                .ok()
                .and_then(|rel| rel.components().next())
                .map(|c| c.as_os_str().as_encoded_bytes() == pack.as_slice())
                .unwrap_or(false)
        })
        .collect();
    let pool: Vec<&PathBuf> = if local.is_empty() {
        candidates.iter().collect()
    } else {
        local
    };

    // Several providers must agree byte-for-byte; then the picture is read once.
    let mut first: Option<(String, Vec<u8>)> = None;
    for c in pool.iter() {
        let bytes = fs::read(c).with_context(|| format!("{} does not read", c.display()))?;
        if (bytes.len() as u64) > MAX_TEXTURE {
            bail!("texture {reference} is too large to embed");
        }
        match &first {
            Some((_, seen)) if *seen != bytes => {
                bail!("texture {reference} is provided with different bytes by several packs")
            }
            None => first = Some((c.strip_prefix(root).unwrap_or(c).display().to_string(), bytes)),
            _ => {}
        }
    }
    let Some((source_file, bytes)) = first else {
        return Ok(None);
    };

    let img = image::load_from_memory(&bytes)
        .with_context(|| format!("texture {reference} is not a decodable picture"))?;
    let rgba = img.to_rgba8();
    let pixel = *rgba.pixels().next().unwrap();
    if rgba.pixels().all(|px| *px == pixel) {
        let solid = [pixel[0], pixel[1], pixel[2], pixel[3]];
        return Ok(Some(Resolved::Solid(solid, source_file)));
    }
    // A patterned picture rides in as-is (PNG where it is not already one).
    let png = if bytes.starts_with(b"\x89PNG") {
        bytes
    } else {
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ColorType::Rgba8.into())?;
        out
    };
    Ok(Some(Resolved::Embedded(png, source_file)))
}

fn run(args: &Args) -> Result<Report> {
    let root = &args.root;
    let apply = args.apply;
    let index = texture_index(root);

    let mut report = Report {
        mode: if apply { "apply" } else { "dry-run" },
        root: root.display().to_string(),
        models: Vec::new(),
        fixed_materials: 0,
        failed: 0,
    };

    for p in walk(root) {
        if p.file_name().and_then(|n| n.to_str()) != Some("presentation.json") {
            continue;
        }
        let dir = p.parent().context("presentation.json has no parent")?;
        let Ok(raw) = fs::read(&p) else { continue };
        let Ok(value): serde_json::Result<Value> = serde_json::from_slice(&raw) else {
            continue; // Not a presentation table (or unreadable); nothing to do.
        };
        let Some(models) = value.get("models").and_then(Value::as_object) else {
            continue;
        };

        for (id, entry) in models {
            let Some(refs) = entry.get("textures").and_then(Value::as_array) else {
                continue;
            };
            if refs.iter().all(|r| r.as_str().unwrap_or_default().is_empty()) {
                continue; // No references at all.
            }
            let Some(file) = entry.get("file").and_then(Value::as_str) else {
                report.failed += 1;
                report.models.push(ModelReport {
                    file: id.clone(),
                    fixes: Vec::new(),
                    notes: vec!["model table entry has no file".into()],
                });
                continue;
            };
            // The table is usually one level above the model files.
            let Some(glb_path) = [dir, dir.parent().unwrap_or(dir)]
                .iter()
                .map(|d| d.join(file))
                .find(|c| c.is_file())
            else {
                report.failed += 1;
                report.models.push(ModelReport {
                    file: format!("{id} -> {file}"),
                    fixes: Vec::new(),
                    notes: vec!["model file not found".into()],
                });
                continue;
            };
            if glb_path.extension().and_then(|e| e.to_str()) != Some("glb") {
                continue; // Still JSON (or something else); not this tool's job.
            }

            let bytes = fs::read(&glb_path)
                .with_context(|| format!("{} does not read", glb_path.display()))?;
            let (mut json, bin) = split_glb(&bytes)?;
            let Some(mats) = json.get("materials").and_then(Value::as_array) else {
                continue; // No materials to fix.
            };

            // Pass 1 only decides what each material needs, so pass 2 can
            // append bufferViews/images/textures without a live borrow of the
            // materials array.
            let mut fixes: Vec<MaterialFix> = Vec::new();
            let mut notes: Vec<String> = Vec::new();
            for (i, mat) in mats.iter().enumerate() {
                let Some(pbr) = mat.get("pbrMetallicRoughness") else { continue };
                if pbr.get("baseColorTexture").is_some() {
                    continue; // Already carries its picture.
                }
                let Some(reference) = refs.get(i).and_then(Value::as_str) else {
                    notes.push(format!("material {} has no reference in the table", i));
                    continue;
                };
                if reference.is_empty() {
                    continue; // "" is "untextured on purpose".
                }
                match resolve_reference(&index, root, glb_path.parent().unwrap_or(dir), reference) {
                    Ok(Some(Resolved::Solid(pixel, source_file))) => {
                        // One solid colour: glTF's factor carries it, no
                        // picture at all.
                        fixes.push(MaterialFix {
                            index: i,
                            name: mat.get("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
                            reference: reference.to_owned(),
                            source_file,
                            kind: "solid",
                            buffer_view: None,
                            bytes: Vec::new(),
                            factor: Some([
                                linear_byte(pixel[0]),
                                linear_byte(pixel[1]),
                                linear_byte(pixel[2]),
                                pixel[3] as f32 / 255.0,
                            ]),
                        });
                    }
                    Ok(Some(Resolved::Embedded(png, source_file))) => {
                        // A patterned picture rides in as PNG.
                        fixes.push(MaterialFix {
                            index: i,
                            name: mat.get("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
                            reference: reference.to_owned(),
                            source_file,
                            kind: "embedded",
                            buffer_view: None, // assigned by pass 2
                            bytes: png,
                            factor: None,
                        });
                    }
                    Ok(None) => {
                        notes.push(format!("material {} reference {reference} is not provided by any installed table", i));
                    }
                    Err(error) => {
                        report.failed += 1;
                        notes.push(format!("material {}: {error:#}", i));
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

            report.fixed_materials += fixes.len();
            if apply {
                fs::write(&glb_path, packed)
                    .with_context(|| format!("cannot write {}", glb_path.display()))?;
            }
            let rel = glb_path.strip_prefix(root).unwrap_or(&glb_path);
            report.models.push(ModelReport {
                file: rel.display().to_string(),
                fixes,
                notes,
            });
        }
    }

    let text = serde_json::to_vec_pretty(&json!({
        "mode": report.mode,
        "root": report.root,
        "models": report.models.iter().map(|m| json!({
            "file": m.file,
            "materials": m.fixes.iter().map(|f| json!({
                "index": f.index,
                "name": f.name,
                "reference": f.reference,
                "source_file": f.source_file,
                "kind": f.kind,
                "bytes": f.bytes.len(),
            })).collect::<Vec<_>>(),
            "notes": m.notes,
        })).collect::<Vec<_>>(),
    }))?;
    fs::write(root.join("BACKFILL-REPORT.json"), text)?;
    Ok(report)
}
