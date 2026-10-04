//! Offline converter from a Blockland v20 installation plus the decompiled
//! vanilla client/server scripts to a native `bri-ui` pack.
//!
//! Inputs are only read. Outputs go to a caller-chosen directory, which must
//! stay out of version control because it contains original game content.
#![allow(
    clippy::disallowed_methods,
    reason = "f32::clamp here is not yet bri_console::Clamp::clamped"
)]

pub mod data;
pub mod gft;
pub mod layout;
pub mod torque;
pub mod vfs;

use anyhow::{Context, Result, ensure};
use bri_ui::schema::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Inputs {
    /// Root of the original v20 installation (read-only).
    pub v20_root: PathBuf,
    /// `allClientGuis-Vanilla.gui` decompile.
    pub client_gui: PathBuf,
    /// `allClientScripts-Vanilla.cs` decompile.
    pub client_scripts: PathBuf,
    /// `allGameScripts-Vanilla.cs` decompile.
    pub server_scripts: PathBuf,
    /// Stock (non-B4v21) `client/defaults.cs`.
    pub stock_client_defaults: PathBuf,
    /// Optional converted native catalog; import its original icon references.
    pub brick_catalog: Option<PathBuf>,
}

pub struct Report {
    pub pack: UiPack,
    pub files_written: usize,
    pub bytes_written: u64,
}

fn sha(b: &[u8]) -> String {
    let d = Sha256::digest(b);
    d.iter().map(|x| format!("{x:02x}")).collect()
}

fn read_text(p: &Path, sources: &mut Vec<SourceRecord>) -> Result<String> {
    ensure!(
        fs::metadata(p)?.len() <= 64 * 1024 * 1024,
        "oversized script input"
    );
    let mut b = fs::read(p).with_context(|| format!("reading {}", p.display()))?;
    // As LF: a git checkout gives these scripts CRLF on Windows and LF
    // elsewhere, and the pack records their hash and size.
    b.retain(|&c| c != b'\r');
    sources.push(SourceRecord {
        path: p.to_string_lossy().replace('\\', "/"),
        sha256: sha(&b),
        bytes: b.len() as u64,
    });
    // Decompiles are UTF-8; fall back to Latin-1 for stray bytes.
    Ok(match String::from_utf8(b.clone()) {
        Ok(s) => s,
        Err(_) => b.iter().map(|&c| c as char).collect(),
    })
}

struct Writer {
    out: PathBuf,
    files: usize,
    bytes: u64,
    records: Vec<SourceRecord>,
}

impl Writer {
    fn put(&mut self, rel: &str, data: &[u8]) -> Result<()> {
        ensure!(vfs::safe_relative(rel), "unsafe output path: {rel}");
        let p = self.out.join(rel);
        if let Some(d) = p.parent() {
            fs::create_dir_all(d)?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
            .with_context(|| format!("creating {}", p.display()))?;
        file.write_all(data)?;
        self.records.push(SourceRecord {
            path: rel.into(),
            sha256: sha(data),
            bytes: data.len() as u64,
        });
        self.files += 1;
        self.bytes += data.len() as u64;
        Ok(())
    }
}

// Requiring an existing parent and a fresh final directory avoids writes through
// pre-existing output symlinks, junctions, or hard-linked files. Canonicalization
// resolves aliases and `..` before comparing with the source installation.
fn output_destination(root: &Path, out: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .context("canonicalizing original installation")?;
    let absolute = std::path::absolute(out)?;
    let name = absolute
        .file_name()
        .context("output requires a final directory name")?;
    let parent = absolute
        .parent()
        .context("output requires a parent")?
        .canonicalize()
        .context("output parent must already exist")?;
    let destination = parent.join(name);
    #[cfg(windows)]
    let inside = PathBuf::from(destination.to_string_lossy().to_lowercase())
        .starts_with(PathBuf::from(root.to_string_lossy().to_lowercase()));
    #[cfg(not(windows))]
    let inside = destination.starts_with(&root);
    ensure!(
        !inside,
        "refusing to write inside the original installation"
    );
    ensure!(
        fs::symlink_metadata(&destination).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "output must be a new directory (existing paths are never overwritten)"
    );
    Ok(destination)
}

fn is_image(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg")
}

fn add_image(
    vfs: &vfs::Vfs,
    path: &str,
    pack: &mut UiPack,
    w: &mut Writer,
) -> Result<Option<String>> {
    let Some(real) = vfs.resolve(path).map(str::to_string) else {
        return Ok(None);
    };
    let id = layout::image_id(&real, "base/client/ui").context("image id")?;
    if pack.images.contains_key(&id) {
        return Ok(Some(id));
    }
    let bytes = vfs.read(&real)?;
    let (width, height) = match image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()?
        .into_dimensions()
    {
        Ok(d) => d,
        Err(e) => {
            pack.warnings
                .push(format!("{real}: unreadable image ({e}); skipped"));
            return Ok(None);
        }
    };
    let ext = real
        .rsplit('.')
        .next()
        .unwrap_or("png")
        .to_ascii_lowercase();
    ensure!(
        width > 0
            && height > 0
            && width <= 8192
            && height <= 8192
            && u64::from(width) * u64::from(height) <= 16_777_216,
        "oversized or empty image {real}"
    );
    let file = format!("images/{id}.{ext}");
    w.put(&file, &bytes)?;
    pack.images.insert(
        id.clone(),
        ImageEntry {
            file,
            width,
            height,
            sha256: sha(&bytes),
            source: vfs.describe(&real),
        },
    );
    Ok(Some(id))
}

/// Slice a Torque bitmap array (T3D guiTypes.cpp:558 algorithm).
pub fn slice_bitmap_array(img: &image::RgbaImage) -> Vec<[u32; 4]> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let sep = *img.get_pixel(0, 0);
    let mut rects = Vec::new();
    let mut y = 0;
    while y < h {
        if *img.get_pixel(0, y) == sep {
            y += 1;
            continue;
        }
        let mut x = 0;
        while x < w {
            if *img.get_pixel(x, y) == sep {
                x += 1;
                continue;
            }
            let sx = x;
            while x < w && *img.get_pixel(x, y) != sep {
                x += 1;
            }
            let mut sy = y;
            while sy < h && *img.get_pixel(sx, sy) != sep {
                sy += 1;
            }
            rects.push([sx, y, x - sx, sy - y]);
        }
        while y < h && *img.get_pixel(0, y) != sep {
            y += 1;
        }
    }
    rects
}

fn mission_info_name(mis: &str) -> Option<String> {
    let mut inside = false;
    for line in mis.lines() {
        let t = line.trim();
        if t.starts_with("new ScriptObject(MissionInfo)") {
            inside = true;
            continue;
        }
        if inside {
            if t.starts_with("};") {
                break;
            }
            if let Some(rest) = t.strip_prefix("name")
                && let Some(v) = rest.trim_start().strip_prefix('=')
            {
                let v = v.trim().trim_end_matches(';').trim().trim_matches('"');
                return Some(torque::unescape(v));
            }
        }
    }
    None
}

pub fn convert(inputs: &Inputs, out: &Path) -> Result<Report> {
    let destination = output_destination(&inputs.v20_root, out)?;
    fs::create_dir(&destination)?;
    let out = destination.as_path();
    let vfs = vfs::Vfs::open(&inputs.v20_root)?;
    let mut pack = UiPack {
        schema_version: PACK_SCHEMA_VERSION,
        generator: format!("bri-ui-import {}", env!("CARGO_PKG_VERSION")),
        warnings: vfs.warnings.clone(),
        ..Default::default()
    };
    let mut w = Writer {
        out: out.to_path_buf(),
        files: 0,
        bytes: 0,
        records: Vec::new(),
    };

    // 1. Images: all UI art, mission default, main-menu screenshots, print icons.
    let mut wanted: Vec<String> = vfs
        .list("base/client/ui/")
        .into_iter()
        .filter(|p| is_image(p) && !p.to_ascii_lowercase().contains("/cache/"))
        .collect();
    wanted.extend(
        vfs.list("base/data/missions/")
            .into_iter()
            .filter(|p| is_image(p)),
    );
    wanted.extend(vfs.list("screenshots/").into_iter().filter(|p| is_image(p)));
    for p in vfs.list("Add-Ons/") {
        let l = p.to_ascii_lowercase();
        if l.starts_with("add-ons/print_") && l.contains("/icons/") && is_image(&p) {
            wanted.push(p);
        }
    }
    if let Some(path) = &inputs.brick_catalog {
        #[derive(serde::Deserialize)]
        struct Icons {
            bricks: Vec<Icon>,
        }
        #[derive(serde::Deserialize)]
        struct Icon {
            icon_source: String,
        }
        let catalog: Icons = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(catalog.bricks.len() <= 4096, "excessive brick icon catalog");
        for entry in catalog.bricks {
            if entry.icon_source.is_empty() {
                continue;
            }
            match vfs.resolve_with_ext(&entry.icon_source, &[".png", ".jpg", ".jpeg"]) {
                Some(real) => wanted.push(real),
                None => pack
                    .warnings
                    .push(format!("catalog icon missing: {}", entry.icon_source)),
            }
        }
    }
    // Avatar face/decal lists (MainMenuGui::buildIFLs, c:14186): the base
    // entry first, then root-level PNGs of every Face_*/Decal_* add-on. Their
    // picker thumbnails live beside them under `thumbs/`; an add-on without
    // one shows its full image instead.
    let mut faces = vec!["base/data/shapes/player/faces/smiley".to_string()];
    let mut decals = vec!["base/data/shapes/player/decals/aaa-none".to_string()];
    let mut ifl: Vec<String> = vfs
        .list("Add-Ons/")
        .into_iter()
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            (l.starts_with("add-ons/face_") || l.starts_with("add-ons/decal_"))
                && l.ends_with(".png")
                && l.matches('/').count() == 2
        })
        .collect();
    ifl.sort_by_key(|p| p.to_ascii_lowercase());
    for p in &ifl {
        let Some(id) = layout::image_id(p, "") else {
            continue;
        };
        if id.starts_with("add-ons/face_") {
            faces.push(id)
        } else {
            decals.push(id)
        }
    }
    for id in faces.iter().chain(decals.iter()) {
        let (dir, name) = id.rsplit_once('/').unwrap_or(("", id));
        let thumb = format!("{dir}/thumbs/{name}");
        match vfs.resolve_with_ext(&thumb, &[".png", ".jpg"]) {
            Some(t) => wanted.push(t),
            None => {
                pack.warnings
                    .push(format!("avatar thumbnail missing: {thumb}"));
                if let Some(full) = vfs.resolve_with_ext(id, &[".png", ".jpg"]) {
                    wanted.push(full);
                }
            }
        }
    }
    pack.data.avatar.faces = faces;
    pack.data.avatar.decals = decals;
    for p in &wanted {
        add_image(&vfs, p, &mut pack, &mut w)?;
    }

    // 2. Fonts from the Torque font cache.
    for p in vfs.list("base/client/ui/cache/") {
        if !p.to_ascii_lowercase().ends_with(".gft") {
            continue;
        }
        let stem = Path::new(&p)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let Some((face, size)) = gft::face_and_size(&stem) else {
            pack.warnings
                .push(format!("{p}: unexpected font cache name"));
            continue;
        };
        let bytes = vfs.read(&p)?;
        let font = match gft::parse(&bytes) {
            Ok(f) => f,
            Err(e) => {
                pack.warnings.push(format!("{p}: {e:#}"));
                continue;
            }
        };
        let id = format!("{}_{}", face.to_ascii_lowercase(), size);
        let mut sheets = Vec::new();
        for (i, s) in font.sheets.iter().enumerate() {
            let file = format!("fonts/{}_{i}.png", id.replace(' ', "_"));
            w.put(&file, s)?;
            sheets.push(file);
        }
        pack.fonts.insert(
            id,
            FontEntry {
                face,
                size,
                line_height: font.line_height,
                baseline: font.baseline,
                sheets,
                glyphs: font.glyphs,
                source: p.clone(),
                sha256: sha(&bytes),
            },
        );
    }
    ensure!(
        !pack.fonts.is_empty(),
        "no font caches found under base/client/ui/cache"
    );

    // 3. Scripts.
    let mut sources = Vec::new();
    let gui_text = read_text(&inputs.client_gui, &mut sources)?;
    let client_text = read_text(&inputs.client_scripts, &mut sources)?;
    let server_text = read_text(&inputs.server_scripts, &mut sources)?;
    let defaults_text = read_text(&inputs.stock_client_defaults, &mut sources)?;
    if let Some(path) = &inputs.brick_catalog {
        let _ = read_text(path, &mut sources)?;
    }
    let gui_objects = torque::parse_objects(&gui_text);
    let script_objects = torque::parse_objects(&client_text);

    // 4. Styles (profiles from both files; GUI-file definitions override).
    let mut profiles = BTreeMap::new();
    for o in script_objects.iter().chain(gui_objects.iter()) {
        if o.class == "GuiControlProfile"
            && let Some(n) = &o.name
        {
            profiles.insert(n.clone(), o.clone());
        }
    }
    let font_keys: Vec<(String, u32, String)> = pack
        .fonts
        .iter()
        .map(|(id, f)| (f.face.to_ascii_lowercase(), f.size, id.clone()))
        .collect();
    let font_ids = |face: &str, size: u32| -> Option<String> {
        let face = face.to_ascii_lowercase();
        font_keys
            .iter()
            .filter(|(f, _, _)| *f == face)
            .min_by_key(|(_, s, _)| (*s as i64 - size as i64).abs())
            .or_else(|| {
                font_keys
                    .iter()
                    .filter(|(f, _, _)| f == "arial")
                    .min_by_key(|(_, s, _)| (*s as i64 - size as i64).abs())
            })
            .map(|(_, _, id)| id.clone())
    };
    let mut warnings = Vec::new();
    pack.styles = layout::styles(&profiles, &font_ids, &mut warnings);
    for (name, s) in &pack.styles {
        if let Some(b) = &s.bitmap
            && !pack.images.contains_key(b)
        {
            warnings.push(format!("style {name}: bitmap {b} not found"));
        }
    }

    // 5. Skins: every image a profile uses as a bitmap array.
    let array_images: BTreeSet<String> = pack
        .styles
        .values()
        .filter(|s| s.has_bitmap_array)
        .filter_map(|s| s.bitmap.clone())
        .collect();
    for id in array_images {
        let Some(entry) = pack.images.get(&id) else {
            continue;
        };
        let bytes = fs::read(out.join(&entry.file))?;
        let img = image::load_from_memory(&bytes)?.to_rgba8();
        let pieces = slice_bitmap_array(&img);
        if pieces.is_empty() {
            warnings.push(format!("skin {id}: no pieces"));
        }
        pack.skins.insert(id, SkinEntry { pieces });
    }

    // 6. Layouts: every authored top-level GUI (profiles excluded).
    let image_ids: BTreeSet<String> = pack.images.keys().cloned().collect();
    let has_image = |id: &str| image_ids.contains(id);
    for o in &gui_objects {
        if o.class == "GuiControlProfile" {
            continue;
        }
        if let Some(name) = &o.name {
            let c = layout::control(o, &has_image, &mut warnings);
            pack.layouts.insert(name.clone(), c);
        }
    }

    // 7. UI data.
    pack.data.brick_colorset = data::brick_colorset(&server_text)?;
    pack.data.avatar_colors = data::avatar_colors(&client_text)?;
    pack.data.prefs = data::top_level_assignments(&defaults_text);
    pack.data.favorites = data::favorites(&pack.data.prefs);
    pack.data.remap = data::remap_list(&client_text)?;
    pack.data.default_binds = data::default_binds(&client_text, &mut warnings)?;
    pack.data.global_binds = data::global_binds(&client_text);
    pack.data.event_tables = data::event_tables(&server_text);
    for slot in [
        "hat",
        "pack",
        "secondpack",
        "chest",
        "hip",
        "rarm",
        "larm",
        "rhand",
        "lhand",
        "rleg",
        "lleg",
    ] {
        let path = format!("base/data/shapes/player/{slot}.txt");
        match vfs.resolve(&path).map(str::to_string) {
            Some(real) => {
                let text = String::from_utf8_lossy(&vfs.read(&real)?).to_string();
                pack.data
                    .avatar
                    .parts
                    .insert(slot.to_string(), data::part_list(&text));
            }
            None => pack
                .warnings
                .push(format!("avatar part list missing: {path}")),
        }
    }
    if let Some(real) = vfs
        .resolve("base/data/shapes/player/accent.txt")
        .map(str::to_string)
    {
        let text = String::from_utf8_lossy(&vfs.read(&real)?).to_string();
        let (all, per) = data::accents(&text);
        pack.data.avatar.parts.insert("accent".to_string(), all);
        pack.data.avatar.accents_allowed = per;
    }

    // Help pages: `HelpDlg::onWake` lists `*.hfl` (v20 ships them in base/help/).
    for p in vfs.list("base/help/") {
        if !p.to_ascii_lowercase().ends_with(".hfl") {
            continue;
        }
        let name = Path::new(&p)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        pack.data.help.push(bri_ui::schema::HelpPage {
            name,
            text: data::help_text(&vfs.read(&p)?),
        });
    }
    pack.data.help.sort_by_key(|h| data::help_order(&h.name));
    if pack.data.help.is_empty() {
        pack.warnings.push("no help pages in base/help/".into());
    }

    // 8. Maps: every mission under Add-Ons (loose or zipped).
    let mut seen = BTreeSet::new();
    for p in vfs.list("Add-Ons/") {
        let l = p.to_ascii_lowercase();
        if !(l.starts_with("add-ons/map_") && l.ends_with(".mis") && seen.insert(l.clone())) {
            continue;
        }
        let stem = &p[..p.len() - 4];
        let mis = String::from_utf8_lossy(&vfs.read(&p)?).to_string();
        let display_name = mission_info_name(&mis).unwrap_or_else(|| {
            Path::new(stem)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        let description = vfs
            .resolve_with_ext(stem, &[".txt"])
            .map(|t| {
                vfs.read(&t)
                    .map(|b| String::from_utf8_lossy(&b).replace('\r', ""))
            })
            .transpose()?
            .unwrap_or_else(|| "...".into());
        let preview = match vfs.resolve_with_ext(stem, &[".png", ".jpg"]) {
            Some(img) => add_image(&vfs, &img, &mut pack, &mut w)?,
            None => None,
        };
        pack.maps.push(MapEntry {
            mission: p.clone(),
            display_name,
            description: description.trim_end().to_string(),
            preview,
        });
    }
    pack.maps
        .sort_by_key(|m| m.display_name.to_ascii_lowercase());

    pack.sources = sources;
    pack.warnings.extend(warnings);
    pack.warnings.push("Scope: this pack inventories installed UI assets and maps, including community/B4v21 additions; it is not a verified vanilla allowlist.".into());
    let json = serde_json::to_vec_pretty(&pack)?;
    w.put("ui-pack.json", &json)?;
    let manifest = serde_json::json!({
        "schema_version": 1,
        "scope": "installed content; stock classification remains separate",
        "inputs": vfs.source_records(),
        "script_inputs": pack.sources,
        "outputs": w.records,
    });
    w.put(
        "conversion-manifest.json",
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(Report {
        pack,
        files_written: w.files,
        bytes_written: w.bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_guard_resolves_parent_paths_and_never_overwrites() {
        let dir = std::env::temp_dir().join(format!(
            "bri-ui-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = dir.join("original");
        fs::create_dir_all(root.join("nested")).unwrap();
        assert!(output_destination(&root, &root.join("new")).is_err());
        assert!(output_destination(&root, &root.join("nested/../new")).is_err());
        assert!(output_destination(&root, &dir.join("original/../original/new")).is_err());
        assert!(output_destination(&root, &root).is_err());
        assert!(output_destination(&root, &dir.join("new")).is_ok());
        assert!(!root.join("new").exists());
        // Synthetic fixtures only; this test never opens the real installation.
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_unsafe_portable_output_paths() {
        for path in ["../x", "/x", "C:/x", "a/../x", "a\\x", "a/x:stream", "a/x."] {
            assert!(!vfs::safe_relative(path), "{path}");
        }
        assert!(vfs::safe_relative("images/base/client/ui/button.png"));
    }

    #[test]
    fn slices_separator_bitmap_array() {
        // Rows start where column 0 is not the separator (red), as in the v20 skins.
        // y0-1: pieces [0..2) and [3..5); y2: separator row; y3: piece [0..4).
        let red = image::Rgba([255, 0, 0, 255]);
        let blue = image::Rgba([0, 0, 255, 255]);
        let mut img = image::RgbaImage::from_pixel(6, 4, red);
        for y in 0..2 {
            for x in [0, 1, 3, 4] {
                img.put_pixel(x, y, blue);
            }
        }
        for x in 0..4 {
            img.put_pixel(x, 3, blue);
        }
        img.put_pixel(0, 0, red); // separator colour is defined by pixel (0,0)
        let r = slice_bitmap_array(&img);
        assert_eq!(r, vec![[0, 1, 2, 1], [3, 1, 2, 1], [0, 3, 4, 1]]);
    }

    #[test]
    fn mission_name() {
        let mis = "new SimGroup(MissionGroup) {\n   new ScriptObject(MissionInfo) {\n      name = \"Slopes\";\n   };\n};";
        assert_eq!(mission_info_name(mis).as_deref(), Some("Slopes"));
    }
}
