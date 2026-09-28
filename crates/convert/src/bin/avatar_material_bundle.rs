//! One-time native avatar package. Copies original PNG bytes without resampling.
use anyhow::{Context, Result, ensure};
use bri_content::avatar::{Appearance, Package, Rig, Texture};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "Oversized avatar input");
    Ok(bytes)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn original_png(root: &Path, id: &str) -> Result<(Vec<u8>, String)> {
    ensure!(
        bri_content::brick_materials::safe_relative(id),
        "Unsafe source image path"
    );
    let name = format!("{id}.png");
    if root.join(&name).is_file() {
        return Ok((read(&root.join(&name), 16 * 1024 * 1024)?, name));
    }
    let words: Vec<_> = id.split('/').collect();
    ensure!(
        words.len() >= 3 && words[0].eq_ignore_ascii_case("add-ons"),
        "Missing original image {id}"
    );
    let archive = root.join("Add-Ons").join(format!("{}.zip", words[1]));
    let member = format!("{}.png", words[2..].join("/"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive)?)?;
    let matches: Vec<_> = zip
        .file_names()
        .enumerate()
        .filter(|(_, n)| n.eq_ignore_ascii_case(&member))
        .map(|(i, _)| i)
        .collect();
    ensure!(
        matches.len() == 1,
        "Missing/ambiguous avatar archive image {id}"
    );
    let mut bytes = vec![];
    zip.by_index(matches[0])?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Oversized avatar archive image"
    );
    Ok((bytes, format!("Add-Ons/{}.zip::{member}", words[1])))
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 4,
        "Usage: avatar_material_bundle <original-install-read-only> <native-rig-directory> <native-ui-pack> <new-output-directory>"
    );
    let original = PathBuf::from(&args[0]).canonicalize()?;
    let rig_root = PathBuf::from(&args[1]).canonicalize()?;
    let ui_root = PathBuf::from(&args[2]).canonicalize()?;
    let output = PathBuf::from(&args[3]);
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let output = parent.join(output.file_name().context("Output must name a directory")?);
    ensure!(
        ![&original, &rig_root, &ui_root]
            .iter()
            .any(|p| output.starts_with(p)),
        "Output must be outside input directories"
    );
    ensure!(
        std::fs::symlink_metadata(&output).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "Use a new output directory"
    );
    let rig_bytes = read(&rig_root.join("rig.json"), 64 * 1024 * 1024)?;
    let rig: Rig = serde_json::from_slice(&rig_bytes)?;
    rig.validate()?;
    let ui_bytes = read(&ui_root.join("ui-pack.json"), 64 * 1024 * 1024)?;
    let ui: serde_json::Value = serde_json::from_slice(&ui_bytes)?;
    let avatar = &ui["data"]["avatar"];
    let prefs: BTreeMap<String, String> = serde_json::from_value(ui["data"]["prefs"].clone())?;
    let prefs: BTreeMap<_, _> = prefs
        .into_iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v))
        .collect();
    let pref = |key: &str| -> Result<&str> {
        Ok(prefs
            .get(&format!("$pref::avatar::{key}"))
            .with_context(|| format!("Missing original avatar default {key}"))?
            .as_str())
    };
    let parts: BTreeMap<String, Vec<String>> = serde_json::from_value(avatar["parts"].clone())?;
    let mut defaults = Appearance {
        parts: BTreeMap::new(),
        colors: BTreeMap::new(),
        face: pref("facename")?.into(),
        decal: pref("decalname")?.into(),
    };
    // v20 prefs hold list positions; the pack names the part. The accent's
    // list is the one its hat allows, and accent 0 with none allowed is none.
    let index = |slot: &str| -> Result<usize> { Ok(pref(slot)?.parse()?) };
    let accents: BTreeMap<String, Vec<String>> =
        serde_json::from_value(avatar["accents_allowed"].clone())?;
    let hat = parts
        .get("hat")
        .and_then(|c| c.get(index("hat").ok()?))
        .map(|h| h.to_ascii_lowercase())
        .unwrap_or_default();
    for (slot, choices) in &parts {
        let at = index(slot)?;
        let choices = if slot == "accent" {
            accents.get(&hat)
        } else {
            Some(choices)
        };
        let name = match choices.and_then(|c| c.get(at)) {
            Some(name) => name.to_ascii_lowercase(),
            None if slot == "accent" && at == 0 => "none".into(),
            None => anyhow::bail!("Original avatar default {slot}:{at} names no part"),
        };
        defaults.parts.insert(slot.clone(), name);
    }
    for slot in [
        "head",
        "torso",
        "hat",
        "accent",
        "pack",
        "secondpack",
        "hip",
        "rarm",
        "larm",
        "rhand",
        "lhand",
        "rleg",
        "lleg",
    ] {
        let color: Vec<f32> = pref(&format!("{slot}color"))?
            .split_whitespace()
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        defaults.colors.insert(
            slot.into(),
            color
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid default avatar color"))?,
        );
    }
    let mut package = Package {
        schema_version: 1,
        id: rig.id.clone(),
        rig: "rig.json".into(),
        rig_sha256: hash(&rig_bytes),
        parts,
        accents_allowed: accents,
        faces: serde_json::from_value(avatar["faces"].clone())?,
        decals: serde_json::from_value(avatar["decals"].clone())?,
        surfaces: BTreeMap::new(),
        textures: BTreeMap::new(),
        defaults,
    };
    for material in &rig.shape.materials {
        let name = material.name.to_ascii_lowercase();
        if !["face", "decal"].contains(&name.as_str()) {
            package
                .surfaces
                .insert(name.clone(), format!("base/data/shapes/{name}"));
        }
    }
    let mut files = BTreeMap::new();
    for id in package
        .faces
        .iter()
        .chain(&package.decals)
        .chain(package.surfaces.values())
    {
        let (bytes, source) = original_png(&original, id)?;
        let dimensions =
            image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png)
                .into_dimensions()?;
        let sha256 = hash(&bytes);
        let file = format!("{sha256}.png");
        package.textures.insert(
            id.clone(),
            Texture {
                file: file.clone(),
                sha256,
                source,
                width: dimensions.0,
                height: dimensions.1,
            },
        );
        files.insert(file, bytes);
    }
    package.validate()?;
    let outfit = package.resolve(&package.defaults)?;
    ensure!(
        outfit.nodes.keys().all(|name| rig
            .shape
            .objects
            .iter()
            .any(|o| o.name.eq_ignore_ascii_case(name))),
        "Default outfit refers to absent node"
    );
    std::fs::create_dir(&output)?;
    std::fs::write(output.join("rig.json"), rig_bytes)?;
    for (name, bytes) in &files {
        std::fs::write(output.join(name), bytes)?;
    }
    std::fs::write(
        output.join("avatar.json"),
        serde_json::to_vec_pretty(&package)?,
    )?;
    let report = serde_json::json!({"faces":package.faces.len(),"decals":package.decals.len(),"surfaces":package.surfaces.len(),"textures":package.textures.len(),"unique_pngs":files.len(),"ui_catalog_sha256":hash(&ui_bytes),"copied_original_bytes":true,"original_install_written":false,"scope":"Stock rig plus face/decal catalog from native UI pack; distribution provenance retains earlier inventory limits"});
    std::fs::write(
        output.join("conversion-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
