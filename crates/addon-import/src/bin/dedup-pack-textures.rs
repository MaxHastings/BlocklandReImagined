//! TEMPORARY content fix-up (throwaway tool).
//!
//! The GLB pipeline made every model carry its own colours and pictures, so
//! the loose swatch files that used to stand in for a material's colour are
//! dead weight. This tool removes the ones nothing draws any more:
//!
//! - Item/weapon packs never look textures up by name at runtime (each
//!   material either binds its picture in `presentation.json` or falls back
//!   to the .glb's own bitmap, solid colour, or clear). A loose file whose
//!   name matches a material is drawn by nothing once no manifest binds it.
//! - Vehicle packs do resolve each material's name against their listed
//!   texture files; a picture may leave only where every model that names it
//!   draws exactly the same pixels from its embedded bitmap, solid colour,
//!   or blank paint instead — and never while it is the pack's last
//!   `blank.png` (the blank paint the vehicle falls back to).
//!
//! Manifests are edited with surgical text surgery: only the named entry is
//! deleted, every other byte stays put, so nothing unrelated churns in sync.
//! Dry run by default; pass `--apply` to delete. The report lands at the
//! root as `DEDUP-PACK-TEXTURES.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, ensure, Context, Result};
use regex::Regex;
use serde_json::Value;
use sha2::Digest;

/// A decoded picture the way the game holds it: sRGB RGBA bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pix {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}
impl Pix {
    fn from_image(img: image::RgbaImage) -> Self {
        let width = img.width();
        let height = img.height();
        Self { width, height, rgba: img.into_raw() }
    }
    /// The one colour every texel carries, if all of them do. A uniform
    /// picture samples to the same value at any UV, so it renders like a
    /// solid colour of that value whatever its size.
    fn uniform(&self) -> Option<[u8; 4]> {
        let first = self.rgba[..4].try_into().ok()?;
        (self.rgba.len() == 4 || self.rgba.chunks(4).all(|c| *c == first)).then_some(first)
    }
}

/// A material's render-relevant properties as bits, so f32s compare exactly.
fn color_bits(c: [f32; 4]) -> [u32; 4] {
    [c[0].to_bits(), c[1].to_bits(), c[2].to_bits(), c[3].to_bits()]
}

/// Two materials draw alike through the runtime's name-keyed lookups: same
/// embedded picture and same solid colour.
fn same_render(a: &GlbMat, b: &GlbMat) -> bool {
    a.embedded == b.embedded && a.color.map(color_bits) == b.color.map(color_bits)
}

/// Two pictures draw the same surface: byte-identical, or both uniform in
/// one shared colour (a 1x1 solid and a filled swatch sample alike).
fn renders_same(a: &Pix, b: &Pix) -> bool {
    (a.width == b.width && a.height == b.height && a.rgba == b.rgba)
        || match (a.uniform(), b.uniform()) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
}

/// A material the way the game's glTF lowering sees it: its name, the solid
/// colour a picture-less factor carries (`None` for white or textured), and
/// the bitmap an embedded `baseColorTexture` lowers to.
#[derive(Clone, Debug)]
struct GlbMat {
    name: String,
    color: Option<[f32; 4]>,
    embedded: Option<Pix>,
}

fn u32le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// The JSON and BIN chunks of a .glb (the same split the backfill tools use).
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

/// glTF's `baseColorFactor` is linear; pictures are sRGB. The same curve the
/// client applies when it turns a factor into picture bytes.
fn srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// The picture bytes one `images[]` entry carries: its BIN slice, or the
/// file a `uri` names beside the .glb.
fn image_bytes(doc: &Value, bin: &[u8], glb_dir: &Path, source_index: u64) -> Result<Vec<u8>> {
    let img = doc["images"]
        .as_array()
        .and_then(|a| a.get(source_index as usize))
        .ok_or_else(|| anyhow!("image index {source_index} is out of range"))?;
    if let Some(bv) = img.get("bufferView") {
        let i = bv.as_u64().unwrap_or(0) as usize;
        let view = doc["bufferViews"]
            .as_array()
            .and_then(|a| a.get(i))
            .ok_or_else(|| anyhow!("bufferView index {i} is out of range"))?;
        let start = view["byteOffset"].as_u64().unwrap_or(0) as usize;
        let len = view["byteLength"]
            .as_u64()
            .map(|n| n as usize)
            .filter(|&n| start + n <= bin.len())
            .ok_or_else(|| anyhow!("bufferView {i} runs past the BIN chunk"))?;
        return Ok(bin[start..start + len].to_vec());
    }
    if let Some(uri) = img["uri"].as_str() {
        let path = glb_dir.join(uri);
        ensure!(path.is_file(), "texture file `{}` is missing", path.display());
        return fs::read(&path).with_context(|| format!("reading {}", path.display()));
    }
    bail!("image entry has neither bufferView nor uri")
}

/// One .glb's materials lowered the way `crates/gltf` lowers them: a factor
/// wins over nothing, an embedded bitmap is tinted by it and flattened to
/// solid alpha unless the material says it fades.
fn glb_materials(path: &Path) -> Result<Vec<GlbMat>> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let (doc, bin) = split_glb(&bytes)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut out = Vec::new();
    for m in doc["materials"].as_array().into_iter().flatten() {
        let name = m["name"]
            .as_str()
            .ok_or_else(|| anyhow!("a material has no name"))?
            .to_string();
        // bri_legacy.color is the functional native colour of an opaque
        // solid material; where it is present it wins over the factor.
        let factor: [f32; 4] = match m["extras"]["bri_legacy"]["color"].as_array() {
            Some(a) if a.len() == 4 => [0, 1, 2, 3].map(|i| a[i].as_f64().unwrap_or(1.0) as f32),
            _ => m["pbrMetallicRoughness"]["baseColorFactor"]
                .as_array()
                .map(|a| {
                    [0, 1, 2, 3]
                        .map(|i| a.get(i).and_then(Value::as_f64).unwrap_or(1.0) as f32)
                })
                .unwrap_or([1.0; 4]),
        };
        let tint = [srgb(factor[0]), srgb(factor[1]), srgb(factor[2]), factor[3].clamp(0.0, 1.0)];
        // Keep the picture's alpha for glTF BLEND/MASK or a see-through
        // bri_legacy blend; only the OPAQUE default with no such blend
        // flattens to solid, as `make_material` does.
        let fades = matches!(m["alphaMode"].as_str(), Some("BLEND") | Some("MASK"))
            || matches!(
                m["extras"]["bri_legacy"]["blend"].as_str(),
                Some("alpha" | "additive" | "subtractive")
            );
        match m["pbrMetallicRoughness"].get("baseColorTexture") {
            Some(info) => {
                let index = info["index"].as_u64().unwrap_or(0);
                let raw = image_bytes(&doc, &bin, dir, index)?;
                let decoded = image::load_from_memory(&raw)
                    .with_context(|| "decoding the embedded texture")?
                    .to_rgba8();
                let mut pix = Pix::from_image(decoded);
                if tint != [1.0; 4] {
                    for p in pix.rgba.chunks_mut(4) {
                        for (channel, t) in p.iter_mut().zip(tint) {
                            *channel = (*channel as f32 * t).round() as u8;
                        }
                    }
                }
                if !fades {
                    for p in pix.rgba.chunks_mut(4) {
                        p[3] = 255;
                    }
                }
                out.push(GlbMat { name, color: None, embedded: Some(pix) });
            }
            None => {
                // A picture-less material still carries a uniform swatch in
                // the runtime's name map for every non-Item kind — even the
                // default white one, whose alpha flattens like any opaque.
                let mut rgba = [
                    (tint[0] * 255.0).round() as u8,
                    (tint[1] * 255.0).round() as u8,
                    (tint[2] * 255.0).round() as u8,
                    (tint[3] * 255.0).round() as u8,
                ];
                if !fades {
                    rgba[3] = 255;
                }
                let mut bytes = [0u8; 16];
                for p in bytes.chunks_mut(4) {
                    p.copy_from_slice(&rgba);
                }
                out.push(GlbMat {
                    // glTF's default white means "no colour" and stays None.
                    name,
                    color: (factor != [1.0; 4]).then_some(factor),
                    embedded: Some(Pix { width: 4, height: 4, rgba: bytes.to_vec() }),
                });
            },
        }
    }
    Ok(out)
}

/// One listed vehicle texture: its on-disk file and the pixels it decodes to.
#[derive(Clone)]
struct TexEntry {
    file: PathBuf,
    pix: Pix,
}

/// The texture a vehicle material names (the map is keyed by lower-case
/// virtual path): beside the model's folder first, then anywhere in the pack,
/// trying the name as written and then with each image extension. A copy of
/// `vehicles.rs::material_texture`, over pixels instead of `SceneImage`.
fn material_texture<'a>(
    textures: &'a BTreeMap<String, TexEntry>,
    folder: &str,
    material: &str,
) -> Option<&'a Pix> {
    const EXTENSIONS: [&str; 4] = ["", ".png", ".jpg", ".jpeg"];
    let name = material.to_ascii_lowercase();
    let files = || EXTENSIONS.iter().map(|ext| format!("{name}{ext}"));
    files()
        .find_map(|file| textures.get(&format!("{folder}/{file}")).map(|e| &e.pix))
        .or_else(|| {
            files().find_map(|file| {
                let suffix = format!("/{file}");
                textures
                    .iter()
                    .find(|(path, _)| path.ends_with(&suffix))
                    .map(|(_, e)| &e.pix)
            })
        })
}

/// The pack's blank paint: the first listed texture whose virtual path ends
/// in `/blank.png`, as `vehicles.rs` finds it.
fn blank_of(textures: &BTreeMap<String, TexEntry>) -> Option<&Pix> {
    textures
        .iter()
        .find(|(path, _)| path.ends_with("/blank.png"))
        .map(|(_, e)| &e.pix)
}

/// What one vehicle material draws from a texture map: the listed file, else
/// its embedded bitmap, else its solid colour, else the pack's blank paint.
fn runtime_image(
    textures: &BTreeMap<String, TexEntry>,
    folder: &str,
    mat: &GlbMat,
) -> Pix {
    if let Some(file) = material_texture(textures, folder, &mat.name) {
        return file.clone();
    }
    if let Some(pixels) = &mat.embedded {
        return pixels.clone();
    }
    if let Some(c) = mat.color {
        return Pix {
            width: 1,
            height: 1,
            rgba: vec![
                (srgb(c[0]) * 255.0).round() as u8,
                (srgb(c[1]) * 255.0).round() as u8,
                (srgb(c[2]) * 255.0).round() as u8,
                (c[3].clamp(0.0, 1.0) * 255.0).round() as u8,
            ],
        };
    }
    blank_of(textures).cloned().unwrap_or_else(|| Pix { width: 1, height: 1, rgba: vec![255; 4] })
}

/// Every `{`...`}` span in the text, string-aware: a brace inside quotes is
/// not structure. The files are small; one pass over each is cheap enough.
fn object_spans(text: &str) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let mut stack: Vec<usize> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut spans = Vec::new();
    for (i, &c) in b.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' => stack.push(i),
            b'}' => {
                if let Some(start) = stack.pop() {
                    spans.push((start, i + 1));
                }
            }
            _ => {}
        }
    }
    spans
}

/// The smallest `{`...`}` span containing `at`, if any.
fn enclosing_span(spans: &[(usize, usize)], at: usize) -> Option<(usize, usize)> {
    spans
        .iter()
        .filter(|&&(s, e)| s <= at && at < e)
        .min_by_key(|&(s, e)| e - s)
        .copied()
}

/// The span of the `"key":` head that owns the value object starting at
/// `value_start`, for deleting a whole key/value pair from an object.
fn pair_head(text: &str, value_start: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = value_start;
    while i > 0 && matches!(b[i - 1], b' ' | b'\t' | b'\n' | b'\r') {
        i -= 1;
    }
    if i == 0 || b[i - 1] != b':' {
        return None; // not a key/value pair (an array element)
    }
    i -= 1;
    while i > 0 && matches!(b[i - 1], b' ' | b'\t' | b'\n' | b'\r') {
        i -= 1;
    }
    if i < 2 || b[i - 1] != b'"' {
        return None; // the key's closing quote (need at least `":"`)
    }
    // Walk backward from just before the key's closing quote to its opening
    // quote, skipping escaped quotes inside it. Starting at the closing
    // quote itself would mistake it for the opening one.
    let mut j = i - 2;
    loop {
        if b[j] == b'"' {
            let mut k = j;
            while k > 0 && b[k - 1] == b'\\' {
                k -= 1;
            }
            if (j - k) % 2 == 0 {
                return Some(j); // an unescaped quote: the key's opening
            }
        }
        if j == 0 {
            break;
        }
        j -= 1;
    }
    None
}

/// Delete the objects (or key/value pairs, when `as_pairs`) a marker names.
/// For every regex hit, the enclosing object is parsed and must satisfy
/// `verify`; it is then removed with just enough comma to keep the container
/// valid. Returns the new text and how many entries went; zero removals for
/// a non-empty request is an error.
fn remove_marked(
    text: &str,
    marker: &Regex,
    as_pairs: bool,
    verify: impl Fn(&Value) -> bool,
) -> Result<(String, usize)> {
    let spans = object_spans(text);
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    for m in marker.find_iter(text) {
        let Some((s0, e)) = enclosing_span(&spans, m.start()) else { continue };
        let obj: Value = serde_json::from_str(&text[s0..e])
            .with_context(|| "marked object is not valid JSON")?;
        if !verify(&obj) {
            continue; // the marker matched something that is not this entry
        }
        let mut s = s0;
        if as_pairs {
            s = pair_head(text, s0).ok_or_else(|| anyhow!("marked value has no key head"))?;
        }
        // Swallow one separating comma: after, else before.
        let b = text.as_bytes();
        let mut end = e;
        let mut i = e;
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
        }
        if i < b.len() && b[i] == b',' {
            end = i + 1;
        } else {
            let mut j = s;
            while j > 0 && matches!(b[j - 1], b' ' | b'\t' | b'\n' | b'\r') {
                j -= 1;
            }
            if j > 0 && b[j - 1] == b',' {
                s = j - 1;
            }
        }
        cuts.push((s, end));
    }
    ensure!(!cuts.is_empty(), "the marker matched nothing removable");
    let mut cuts: Vec<(usize, usize)> = cuts;
    cuts.sort_unstable_by_key(|&(s, _)| s);
    for pair in cuts.windows(2) {
        ensure!(pair[0].1 <= pair[1].0, "two marked entries overlap");
    }
    let mut result = text.to_string();
    for (s, e) in cuts.iter().rev() {
        result.replace_range(s..e, "");
    }
    Ok((result, cuts.len()))
}

/// A string value that names `stem` as a texture file: the bare name with an
/// image extension, or any path ending in it. The extension is required — a
/// content identity like `base/data/shapes/gray50.png` pointing at another
/// pack's swatch, or a model path like `models/carbine.glb`, must not count.
fn ref_matches(value: &str, stem: &str) -> bool {
    let v = value.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg"].into_iter().any(|ext| {
        v.ends_with(&format!("/{stem}{ext}")) || v == format!("{stem}{ext}")
    })
}

/// Walk a manifest's string values, skipping the top-level keys `skip` (the
/// index arrays this tool rewrites itself), and report whether any value
/// names `stem`.
fn manifest_names(value: &Value, skip: &[&str], stem: &str) -> bool {
    let mut hit = false;
    fn walk(v: &Value, skip: &[&str], depth: usize, stem: &str, hit: &mut bool) {
        if *hit {
            return;
        }
        match v {
            Value::Object(map) => {
                for (k, child) in map {
                    // Only top-level keys are skippable; deeper structure is
                    // walked whole.
                    if depth == 0 && skip.contains(&k.as_str()) {
                        continue;
                    }
                    walk(child, skip, depth + 1, stem, hit);
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, skip, depth, stem, hit);
                }
            }
            Value::String(s) => {
                if ref_matches(s, stem) {
                    *hit = true;
                }
            }
            _ => {}
        }
    }
    walk(value, skip, 0, stem, &mut hit);
    hit
}

/// The needles a raw (unparsed or non-JSON) text is searched for: the stem's
/// name with each image extension. A bare `/{stem}` never counts — it would
/// match model paths and content identities, not texture files.
fn raw_needles(stem: &str) -> Vec<String> {
    vec![
        format!("{stem}.png"),
        format!("{stem}.jpg"),
        format!("{stem}.jpeg"),
    ]
}

/// A raw-text search for the stem's extensioned names as substrings.
fn raw_text_names(text: &str, needles: &[String]) -> bool {
    let low = text.to_ascii_lowercase();
    needles.iter().any(|n| low.contains(n.as_str()))
}

/// The merged vehicle assets exactly as `VehicleAssets::load_with` builds
/// them: every role-less package listed in content/packages.json that ships
/// an assets/vehicles.json, in list order (a later listing wins a virtual
/// path). Returns nothing when the merge cannot be built at all.
struct GlobalVehicles {
    tex: BTreeMap<String, TexEntry>,
    models: Vec<(String, String, PathBuf)>, // folder, label, glb path
    mats: BTreeMap<PathBuf, Vec<GlbMat>>,
    all_models_readable: bool,
}

fn build_global_vehicles(content_root: &Path, anomalies: &mut Vec<String>) -> Option<GlobalVehicles> {
    let ptext = fs::read_to_string(content_root.join("packages.json")).ok()?;
    let doc: Value = serde_json::from_str(&ptext).ok()?;
    let mut tex: BTreeMap<String, TexEntry> = BTreeMap::new();
    let mut models: Vec<(String, String, PathBuf)> = Vec::new();
    let mut mats: BTreeMap<PathBuf, Vec<GlbMat>> = BTreeMap::new();
    let mut all_models_readable = true;
    for e in doc["packages"].as_array().into_iter().flatten() {
        if e.get("role").is_some() {
            continue; // maps and the like never merge into vehicle assets
        }
        let Some(dir) = e["dir"].as_str() else { continue };
        let vj = content_root.join(dir).join("assets/vehicles.json");
        let Ok(vtext) = fs::read_to_string(&vj) else { continue };
        let Ok(vdoc): Result<Value, _> = serde_json::from_str(&vtext) else {
            anomalies.push(format!("global vehicle map: {} does not parse", vj.display()));
            return None; // refuse to guess the merge
        };
        let base = vj.parent().unwrap();
        for a in vdoc["assets"].as_array().into_iter().flatten() {
            let kind = a["kind"].as_str().unwrap_or("");
            let path = a["path"].as_str().unwrap_or("");
            let vpath = a["virtual_path"].as_str().unwrap_or(path);
            if kind == "texture" {
                let file = base.join(path);
                match fs::read(&file) {
                    Ok(bytes) => match image::load_from_memory(&bytes) {
                        Ok(decoded) => {
                            tex.insert(
                                vpath.to_ascii_lowercase(),
                                TexEntry { file, pix: Pix::from_image(decoded.to_rgba8()) },
                            );
                        }
                        Err(_) => anomalies.push(format!(
                            "global vehicle map: {} does not decode; the game skips it too",
                            file.display()
                        )),
                    },
                    Err(_) => anomalies.push(format!(
                        "global vehicle map: {} cannot be read; the game skips it too",
                        file.display()
                    )),
                }
            } else if kind == "model" && path.to_ascii_lowercase().ends_with(".glb") {
                let folder = vpath
                    .rsplit_once('/')
                    .map_or(String::new(), |(d, _)| d.to_ascii_lowercase());
                models.push((folder, vpath.to_string(), base.join(path)));
            }
        }
    }
    for (_, _, glb) in &models {
        match glb_materials(glb) {
            Ok(m) => { mats.insert(glb.clone(), m); }
            Err(e) => {
                all_models_readable = false;
                anomalies.push(format!("global vehicle map: cannot read {}: {e:#}", glb.display()));
            }
        }
    }
    Some(GlobalVehicles { tex, models, mats, all_models_readable })
}

/// Whether a non-JSON file is worth scanning: small, text-like (no NULs in
/// its head), and not an archive or media blob.
fn scannable_text(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if matches!(ext.as_str(), "zip" | "wasm" | "wav") {
        return None;
    }
    let meta = fs::metadata(path).ok()?;
    if meta.len() > 4 * 1024 * 1024 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.iter().take(8192).any(|&b| b == 0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[derive(Clone, Debug)]
struct Decision {
    file: String, // root-relative
    stem: String,
    decision: &'static str, // "delete" | "keep"
    reason: String,
    entries_removed: Vec<String>, // manifest labels, filled at plan time
}

/// One pack's plan: the decisions plus, per manifest to edit, the (field,
/// value-as-written) pairs whose owning entry resolves to a deleted file.
struct PackPlan {
    name: String,
    is_vehicle_pack: bool,
    decisions: Vec<Decision>,
    manifest_edits: BTreeMap<PathBuf, Vec<(String, String)>>,
}

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                walk_files(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let apply = args.iter().any(|a| a == "--apply");
    let root = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .context("usage: dedup-pack-textures <content-root/wip-pack> [--apply]")?;
    let root = PathBuf::from(root);
    ensure!(root.is_dir(), "no such directory: {}", root.display());

    let mut packs_scanned = 0usize;
    let mut candidates = 0usize;
    let mut deleted = 0usize;
    let mut kept = 0usize;
    let mut anomalies: Vec<String> = Vec::new();
    let mut pack_reports: Vec<Value> = Vec::new();

    // The merged vehicle texture map the running game actually uses, built
    // once from content/packages.json (the parent of this pack folder).
    let content_root = root.parent().unwrap_or(Path::new("."));
    let global_vehicles = build_global_vehicles(content_root, &mut anomalies);

    // Every top-level folder is one add-on.
    let mut pack_dirs: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&root)?.flatten() {
        if entry.path().is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
            pack_dirs.push(entry.path());
        }
    }
    pack_dirs.sort();

    for pack_dir in &pack_dirs {
        let pack = pack_dir.file_name().unwrap().to_string_lossy().to_string();
        packs_scanned += 1;

        // What the models' materials are called, from every .glb.
        let mut all_files: Vec<PathBuf> = Vec::new();
        walk_files(pack_dir, &mut all_files);
        let mut material_names: BTreeSet<String> = BTreeSet::new();
        let mut unreadable_glb = false;
        for g in all_files.iter().filter(|p| p.extension().is_some_and(|e| e == "glb")) {
            match glb_materials(g) {
                Ok(mats) => {
                    // Same-named materials are fine while the copies draw
                    // alike — the runtime's name-keyed lookups then see one
                    // picture either way. Differing copies make a lookup
                    // ambiguous; refuse to guess for this pack.
                    let mut by_name: BTreeMap<String, Vec<&GlbMat>> = BTreeMap::new();
                    for m in &mats {
                        by_name.entry(m.name.to_ascii_lowercase()).or_default().push(m);
                    }
                    if by_name.values().any(|grp| grp[1..].iter().any(|m| !same_render(&grp[0], m))) {
                        unreadable_glb = true;
                        anomalies.push(format!(
                            "{pack}: {} has same-named materials that draw differently; its candidates are kept",
                            g.display()
                        ));
                    } else {
                        for m in &mats {
                            material_names.insert(m.name.to_ascii_lowercase());
                        }
                    }
                }
                Err(e) => {
                    unreadable_glb = true;
                    anomalies.push(format!("{pack}: cannot read {}: {e:#}", g.display()));
                }
            }
        }
        if material_names.is_empty() {
            continue; // no models carry materials here: nothing to de-dupe
        }

        // The loose pictures, and which of them a material names.
        let candidates_list: Vec<(PathBuf, String)> = all_files
            .iter()
            .filter(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("png" | "jpg" | "jpeg")
                )
            })
            .filter_map(|p| {
                let stem = p.file_stem()?.to_string_lossy().to_ascii_lowercase();
                material_names.contains(&stem).then(|| (p.clone(), stem))
            })
            .collect();
        if candidates_list.is_empty() {
            continue;
        }

        // The pack's manifests, read once: parsed where possible, raw text
        // otherwise. Provenance and index files are excluded from protection
        // (this tool rewrites the index entries itself), as is presentation.json,
        // which gets its own precise binding check below.
        let mut jsons: Vec<(PathBuf, Result<Value, String>)> = Vec::new();
        for j in all_files.iter().filter(|p| p.extension().is_some_and(|e| e == "json")) {
            let name = j.file_name().and_then(|f| f.to_str()).unwrap_or("");
            if matches!(name, "import-report.json" | "content.json" | "import-generation.json" | "presentation.json") {
                continue;
            }
            match fs::read_to_string(j) {
                Ok(text) => match serde_json::from_str::<Value>(&text) {
                    Ok(doc) => jsons.push((j.clone(), Ok(doc))),
                    Err(e) => jsons.push((j.clone(), Err(format!("{e}: {text}")))), // JSONC or broken: raw scan
                },
                Err(e) => anomalies.push(format!("{pack}: cannot read {}: {e}", j.display())),
            }
        }
        // Non-JSON text worth a raw scan, read once. Provenance docs are
        // excluded: they record what was imported, not what the game draws.
        let mut texts: Vec<(PathBuf, String)> = Vec::new();
        for t in all_files.iter().filter(|p| p.extension().is_some_and(|e| e != "json")) {
            let name = t.file_name().and_then(|f| f.to_str()).unwrap_or("").to_ascii_lowercase();
            if name == "import-report.md" {
                continue;
            }
            if let Some(s) = scannable_text(t) {
                texts.push((t.clone(), s));
            }
        }

        // Vehicle packs draw from the merged map built above; a pack whose
        // vehicles.json cannot be read is skipped whole.
        let vehicles_json = pack_dir.join("assets/vehicles.json");
        let is_vehicle_pack = vehicles_json.is_file();
        if is_vehicle_pack {
            let Ok(vtext) = fs::read_to_string(&vehicles_json) else {
                anomalies.push(format!("{pack}: cannot read {}", vehicles_json.display()));
                continue;
            };
            match serde_json::from_str::<Value>(&vtext) {
                Ok(_) => {}
                Err(e) => {
                    anomalies.push(format!("{pack}: vehicles.json does not parse: {e}"));
                    continue;
                }
            }
        }

        // Every value an `"icon"` field carries in the pack's manifests: those
        // ids draw at runtime (the Add-On screen and item lists), so their
        // backing files stay.
        let mut icon_values: BTreeSet<String> = BTreeSet::new();
        fn collect_icons(v: &Value, out: &mut BTreeSet<String>) {
            match v {
                Value::Object(map) => {
                    for (k, child) in map {
                        if k == "icon" {
                            if let Some(s) = child.as_str() {
                                out.insert(s.to_ascii_lowercase());
                            }
                        }
                        collect_icons(child, out);
                    }
                }
                Value::Array(items) => {
                    for child in items {
                        collect_icons(child, out);
                    }
                }
                _ => {}
            }
        }
        for (_, parsed) in &jsons {
            if let Ok(doc) = parsed {
                collect_icons(doc, &mut icon_values);
            }
        }

        // presentation.json: the files a model actually binds or an icon
        // names. Every texture slot loads eagerly at startup, but only bound
        // and icon ids ever draw — an unbound, uniconed slot pointing at a
        // candidate is dead weight that goes with the file.
        let mut bound_files: BTreeSet<PathBuf> = BTreeSet::new();
        for base in ["assets/presentation.json", "content/assets/presentation.json"] {
            let pres = pack_dir.join(base);
            if !pres.is_file() {
                continue;
            }
            match fs::read_to_string(&pres).and_then(|t| Ok(serde_json::from_str::<Value>(&t)?)) {
                Ok(doc) => {
                    collect_icons(&doc, &mut icon_values);
                    let dir = pres.parent().unwrap();
                    let mut used: BTreeSet<String> = BTreeSet::new();
                    for m in doc["models"]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .map(|(_, v)| v)
                    {
                        for b in m["textures"].as_array().into_iter().flatten() {
                            if let Some(s) = b.as_str() {
                                if !s.is_empty() {
                                    used.insert(s.to_ascii_lowercase());
                                }
                            }
                        }
                    }
                    for (id, t) in doc["textures"].as_object().into_iter().flatten() {
                        let id_l = id.to_ascii_lowercase();
                        let iconed = icon_values.iter().any(|v| {
                            v == &id_l
                                || ["png", "jpg", "jpeg"]
                                    .iter()
                                    .any(|ext| format!("{v}.{ext}") == id_l)
                        });
                        if !used.contains(&id_l) && !iconed {
                            continue;
                        }
                        if let Some(file) = t["file"].as_str() {
                            let full = dir.join(file);
                            if fs::metadata(&full).is_ok() {
                                bound_files.insert(full);
                            }
                        }
                    }
                }
                Err(e) => anomalies.push(format!("{pack}: presentation.json unusable ({e}); its candidates are kept")),
            }
        }

        let mut plan = PackPlan {
            name: pack.clone(),
            is_vehicle_pack,
            decisions: Vec::new(),
            manifest_edits: BTreeMap::new(),
        };
        for (file, stem) in &candidates_list {
            candidates += 1;
            let rel = file.strip_prefix(&root).unwrap_or(file).display().to_string();

            // 1) Any runtime manifest that names the file keeps it.
            let mut protectors: Vec<String> = Vec::new();
            for (j, parsed) in &jsons {
                let name = j.file_name().unwrap().to_string_lossy().to_string();
                match parsed {
                    Ok(doc) => {
                        let skip = if name == "weapons.json" {
                            vec!["resources"]
                        } else if name == "vehicles.json" {
                            vec!["assets"]
                        } else {
                            Vec::new()
                        };
                        if manifest_names(doc, &skip, stem) {
                            protectors.push(name);
                        }
                    }
                    Err(raw) => {
                        // JSONC (bots.json's comments): search raw.
                        let needles = raw_needles(stem);
                        if raw_text_names(raw, &needles) {
                            protectors.push(format!("{name} (unparsed, raw match)"));
                        }
                    }
                }
            }
            // 2) Scripts and docs that name it.
            for (t, s) in &texts {
                if t == file {
                    continue;
                }
                let needles = raw_needles(stem);
                if raw_text_names(s, &needles) {
                    protectors.push(t.strip_prefix(pack_dir).unwrap_or(t).display().to_string());
                }
            }
            if !protectors.is_empty() {
                plan.decisions.push(Decision {
                    file: rel.clone(),
                    stem: stem.clone(),
                    decision: "keep",
                    reason: format!("referenced by {}", protectors.join(", ")),
                    entries_removed: Vec::new(),
                });
                kept += 1;
                continue;
            }

            // 3) A model binding it in presentation.json keeps it.
            let file_abs = fs::canonicalize(file).unwrap_or_else(|_| file.clone());
            if bound_files.iter().any(|b| same_path(b, &file_abs)) {
                plan.decisions.push(Decision {
                    file: rel.clone(),
                    stem: stem.clone(),
                    decision: "keep",
                    reason: "a model binds this texture in presentation.json".into(),
                    entries_removed: Vec::new(),
                });
                kept += 1;
                continue;
            }

            // 4) Vehicle packs: prove every drawing stays identical in the
            // merged map. A pack not listed in packages.json contributes no
            // textures to that merge, so its files are runtime-inert (the
            // manifest entries still go, to keep it consistent when enabled).
            if is_vehicle_pack {
                let Some(gv) = &global_vehicles else {
                    plan.decisions.push(Decision {
                        file: rel.clone(),
                        stem: stem.clone(),
                        decision: "keep",
                        reason: "the merged vehicle texture map could not be built; refusing to guess".into(),
                        entries_removed: Vec::new(),
                    });
                    kept += 1;
                    continue;
                };
                if unreadable_glb || !gv.all_models_readable {
                    plan.decisions.push(Decision {
                        file: rel.clone(),
                        stem: stem.clone(),
                        decision: "keep",
                        reason: "a model in a vehicle pack could not be read; refusing to guess".into(),
                        entries_removed: Vec::new(),
                    });
                    kept += 1;
                    continue;
                }
                let in_runtime = gv.tex.iter().any(|(_, e)| same_path(&e.file, &file_abs));
                if !in_runtime {
                    // Not part of the running merge: nothing drawn today
                    // depends on it. Fall through to deletion.
                } else {
                    let after_blank_ok = gv
                        .tex
                        .iter()
                        .filter(|(_, e)| !same_path(&e.file, &file_abs))
                        .any(|(k, _)| k.ends_with("/blank.png"));
                    if !after_blank_ok {
                        plan.decisions.push(Decision {
                            file: rel.clone(),
                            stem: stem.clone(),
                            decision: "keep",
                            reason: "it is the last blank paint texture in the merged vehicle map".into(),
                            entries_removed: Vec::new(),
                        });
                        kept += 1;
                        continue;
                    }
                    let after: BTreeMap<String, TexEntry> = gv
                        .tex
                        .iter()
                        .filter(|(_, e)| !same_path(&e.file, &file_abs))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    let mut changed: Vec<String> = Vec::new();
                    for (folder, label, glb) in &gv.models {
                        let Some(mats) = gv.mats.get(glb) else { continue };
                        for m in mats {
                            if !renders_same(&runtime_image(&gv.tex, folder, m), &runtime_image(&after, folder, m)) {
                                changed.push(format!("{label} material `{}`", m.name));
                            }
                        }
                    }
                    if !changed.is_empty() {
                        plan.decisions.push(Decision {
                            file: rel.clone(),
                            stem: stem.clone(),
                            decision: "keep",
                            reason: format!("removing it changes what {} draws", changed.join(", ")),
                            entries_removed: Vec::new(),
                        });
                        kept += 1;
                        continue;
                    }
                }
            }

            // 5) Deletable. Queue the index entries that name it.
            plan.decisions.push(Decision {
                file: rel.clone(),
                stem: stem.clone(),
                decision: "delete",
                reason: "no manifest or model draws it; the .glb carries its own colour".into(),
                entries_removed: Vec::new(),
            });
            deleted += 1;
        }

        // Queue the manifest edits for every deletion (dry run included, so
        // the report shows exactly what apply would touch).
        let deletions: Vec<PathBuf> = plan
            .decisions
            .iter()
            .filter(|d| d.decision == "delete")
            .map(|d| root.join(&d.file))
            .collect();
        if !deletions.is_empty() {
            queue_manifest_edits(&mut plan, pack_dir, &deletions);
            for d in plan.decisions.iter_mut().filter(|d| d.decision == "delete") {
                let abs = root.join(&d.file);
                for (manifest, edits) in &plan.manifest_edits {
                    let base = manifest.parent().unwrap_or(Path::new("."));
                    for (field, value) in edits {
                        if entry_resolves(base, field.as_str(), value.as_str(), &abs) {
                            d.entries_removed.push(format!(
                                "{}/{}: {} `{}`",
                                manifest.file_name().unwrap().to_string_lossy(),
                                "",
                                field,
                                value
                            ));
                        }
                    }
                }
            }
        }

        if !plan.decisions.is_empty() {
            let decisions: Vec<Value> = plan
                .decisions
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "file": d.file,
                        "stem": d.stem,
                        "decision": d.decision,
                        "reason": d.reason,
                        "entries_removed": d.entries_removed,
                    })
                })
                .collect();
            pack_reports.push(serde_json::json!({
                "pack": plan.name,
                "kind": if plan.is_vehicle_pack { "vehicle" } else { "item" },
                "decisions": decisions,
            }));
        }

        // Apply phase: surgical manifest edits, then the deletions.
        if apply && !deletions.is_empty() {
            apply_plan(&root, pack_dir, &mut plan, &mut anomalies)?;
        }
    }

    let report = serde_json::json!({
        "tool": "dedup-pack-textures",
        "generated": now_stamp(),
        "applied": apply,
        "totals": {
            "packs_scanned": packs_scanned,
            "candidates": candidates,
            "deleted": deleted,
            "kept": kept,
        },
        "anomalies": anomalies,
        "packs": pack_reports,
    });
    let report_text = serde_json::to_string_pretty(&report)? + "\n";
    fs::write(root.join("DEDUP-PACK-TEXTURES.json"), &report_text)
        .context("writing the report")?;

    println!(
        "{}: {candidates} material-named pictures across {packs_scanned} packs -> \
         {deleted} deleted, {kept} kept{} (report at {})",
        if apply { "APPLIED" } else { "dry run" },
        if anomalies.is_empty() { String::new() } else { format!( ", {} anomaly(ies)", anomalies.len()) },
        root.join("DEDUP-PACK-TEXTURES.json").display(),
    );
    for a in &anomalies {
        println!("  ANOMALY: {a}");
    }
    if !apply && deleted > 0 {
        println!("  re-run with --apply to delete; nothing was touched.");
    }
    Ok(())
}

fn now_stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

/// Path equality that tolerates case and a missing (deleted) target.
fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

/// Whether an entry's `field` value, resolved against the manifest's folder
/// or the pack root, lands on one of the files being deleted.
fn entry_resolves(base: &Path, _field: &str, value: &str, file: &Path) -> bool {
    for candidate in [base.join(value), base.parent().map(|p| p.join(value)).unwrap_or_default()] {
        if let Ok(full) = fs::canonicalize(&candidate) {
            if same_path(&full, file) {
                return true;
            }
        } else if candidate.to_string_lossy().eq_ignore_ascii_case(&file.to_string_lossy()) {
            return true;
        }
    }
    false
}

/// Queue the index entries that name a deleted file: content.json's
/// inventory, weapons.json resources, vehicles.json assets, and any orphaned
/// presentation.json texture slot. Each queue item is (field, value-as-written)
/// so apply can mark it exactly.
fn queue_manifest_edits(plan: &mut PackPlan, pack_dir: &Path, deletions: &[PathBuf]) {
    let mut add = |manifest: PathBuf, field: String, value: String| {
        plan.manifest_edits.entry(manifest).or_default().push((field, value));
    };

    // content.json (wherever it sits): inventory entries whose `file` lands on
    // a deletion.
    for base in ["assets/content.json", "content/assets/content.json"] {
        let cj = pack_dir.join(base);
        if !cj.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&cj) else { continue };
        let Ok(doc) = serde_json::from_str::<Value>(&text) else { continue };
        for e in doc["content"].as_array().into_iter().flatten() {
            if let Some(f) = e["file"].as_str() {
                if deletions.iter().any(|d| entry_resolves(cj.parent().unwrap(), "file", f, d)) {
                    add(cj.clone(), "file".into(), f.into());
                }
            }
        }
    }

    // weapons.json resources[]: pins whose native_file lands on a deletion.
    for base in ["assets/weapons.json", "content/assets/weapons.json"] {
        let wj = pack_dir.join(base);
        if !wj.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&wj) else { continue };
        let Ok(doc) = serde_json::from_str::<Value>(&text) else { continue };
        for e in doc["resources"].as_array().into_iter().flatten() {
            if let Some(f) = e["native_file"].as_str() {
                if deletions.iter().any(|d| entry_resolves(wj.parent().unwrap(), "native_file", f, d)) {
                    add(wj.clone(), "native_file".into(), f.into());
                }
            }
        }
    }

    // vehicles.json assets[]: the kind==texture entries that list a deletion.
    let vj = pack_dir.join("assets/vehicles.json");
    if vj.is_file() {
        let Ok(text) = fs::read_to_string(&vj) else { return };
        let Ok(doc) = serde_json::from_str::<Value>(&text) else { return };
        for e in doc["assets"].as_array().into_iter().flatten() {
            if e["kind"].as_str() != Some("texture") {
                continue;
            }
            if let Some(f) = e["path"].as_str() {
                if deletions.iter().any(|d| entry_resolves(vj.parent().unwrap(), "path", f, d)) {
                    add(vj.clone(), "path".into(), f.into());
                }
            }
        }
    }

    // presentation.json textures{}: slots whose file lands on a deletion. The
    // protection pass already proved no model binds any of these.
    for base in ["assets/presentation.json", "content/assets/presentation.json"] {
        let pj = pack_dir.join(base);
        if !pj.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&pj) else { continue };
        let Ok(doc) = serde_json::from_str::<Value>(&text) else { continue };
        for t in doc["textures"].as_object().into_iter().flatten().map(|(_, v)| v) {
            if let Some(f) = t["file"].as_str() {
                if deletions.iter().any(|d| entry_resolves(pj.parent().unwrap(), "file", f, d)) {
                    add(pj.clone(), "file".into(), f.into());
                }
            }
        }
    }
}

/// Apply one pack's plan: rewrite the marked manifests (verifying each), then
/// delete the files. Any failure aborts before a file goes.
fn apply_plan(
    root: &Path,
    pack_dir: &Path,
    plan: &mut PackPlan,
    anomalies: &mut Vec<String>,
) -> Result<()> {
    let deletions: Vec<PathBuf> = plan
        .decisions
        .iter()
        .filter(|d| d.decision == "delete")
        .map(|d| root.join(&d.file))
        .collect();

    // Compute every manifest's new text and verify it parses before writing
    // anything at all.
    let mut final_texts: BTreeMap<&PathBuf, String> = BTreeMap::new();
    for (manifest, edits) in &plan.manifest_edits {
        if edits.is_empty() {
            continue;
        }
        let text = fs::read_to_string(manifest)
            .with_context(|| format!("reading {}", manifest.display()))?;
        let mut next = text.clone();
        // Several inventory entries can name one file; remove_marked clears
        // every hit of a (field, value) in a single pass, so apply each unique
        // pair once — a second identical edit would match nothing and abort.
        let mut seen_edits: BTreeSet<(&str, &str)> = BTreeSet::new();
        for (field, value) in edits {
            if !seen_edits.insert((field.as_str(), value.as_str())) {
                continue;
            }
            // Mark the exact `"field": "value"` pair and remove its owning
            // object — or key/value head where it sits in an object.
            let marker = Regex::new(&format!(
                r#""{}"\s*:\s*"{}""#,
                regex::escape(field),
                regex::escape(value)
            ))?;
            let as_pairs = manifest.file_name().and_then(|f| f.to_str()) == Some("presentation.json");
            let base = manifest.parent().unwrap_or(Path::new("."));
            match remove_marked(&next, &marker, as_pairs, |obj| {
                obj.get(field.as_str()).and_then(Value::as_str).is_some_and(|v| {
                    deletions.iter().any(|d| entry_resolves(base, field, v, d))
                        // vehicles.json: never touch a non-texture asset.
                        && (manifest.file_name().and_then(|f| f.to_str()) != Some("vehicles.json")
                            || obj["kind"].as_str() == Some("texture"))
                })
            }) {
                Ok((new_text, _n)) => {
                    ensure!(
                        serde_json::from_str::<Value>(&new_text).is_ok(),
                        "{}: result does not parse",
                        manifest.display()
                    );
                    next = new_text;
                }
                Err(_) => {
                    anomalies.push(format!(
                        "{}: {} still names `{}` ({field}); leaving both in place — check the file by hand",
                        plan.name,
                        manifest.file_name().unwrap().to_string_lossy(),
                        value
                    ));
                    return Ok(()); // abort this pack before deleting anything
                }
            }
        }
        final_texts.insert(manifest, next);
    }

    for (manifest, new_text) in &final_texts {
        let old = fs::read_to_string(manifest)?;
        if old == *new_text {
            continue;
        }
        // Re-pin any aggregate that checksums this manifest's bytes.
        re_pin_aggregates(pack_dir, manifest, old.as_bytes(), new_text.as_bytes())?;
        fs::write(manifest, new_text).with_context(|| format!("writing {}", manifest.display()))?;
    }

    for d in plan.decisions.iter().filter(|d| d.decision == "delete") {
        let path = root.join(&d.file);
        fs::remove_file(&path).with_context(|| format!("deleting {}", path.display()))?;
    }
    Ok(())
}

/// Rewrite every aggregate pin in the pack that checksums `manifest`'s old
/// bytes to its new ones: keys ending `_sha256`, never the provenance
/// `source_*` pins, values equal to the old hash.
fn re_pin_aggregates(pack_dir: &Path, manifest: &Path, old_bytes: &[u8], new_bytes: &[u8]) -> Result<()> {
    let old = sha256_hex(old_bytes);
    let new = sha256_hex(new_bytes);
    let mut jsons: Vec<PathBuf> = Vec::new();
    walk_files(pack_dir, &mut jsons);
    for j in &jsons {
        if j == manifest || !j.extension().is_some_and(|e| e == "json") {
            continue;
        }
        let name = j.file_name().unwrap().to_string_lossy().to_string();
        if matches!(name.as_str(), "import-report.json" | "content.json" | "import-generation.json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(j) else { continue };
        let Ok(doc) = serde_json::from_str::<Value>(&text) else { continue };
        if !contains_pin(&doc, &old) {
            continue;
        }
        // Surgical: replace each quoted old hash that sits under a pin key.
        let re = Regex::new(&format!(r#""([a-z0-9_]+_sha256)"\s*:\s*"{old}""#))?;
        let new_text = re
            .replace_all(&text, |caps: &regex::Captures| {
                let key = caps[1].to_string();
                if key.starts_with("source") {
                    caps.get(0).unwrap().as_str().to_string() // provenance pins never move
                } else {
                    format!("\"{key}\": \"{new}\"")
                }
            })
            .into_owned();
        ensure!(
            serde_json::from_str::<Value>(&new_text).is_ok(),
            "{name}: re-pin result does not parse"
        );
        fs::write(j, new_text).with_context(|| format!("writing {}", j.display()))?;
    }
    Ok(())
}

fn contains_pin(value: &Value, hash: &str) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(k, v)| {
            (k.ends_with("_sha256") && !k.starts_with("source"))
                && matches!(v.as_str(), Some(h) if h == hash)
                || contains_pin(v, hash)
        }),
        Value::Array(items) => items.iter().any(|v| contains_pin(v, hash)),
        _ => false,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let d = sha2::Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}
