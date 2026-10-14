//! `bri-import-addon map`: an old Blockland map Add-On (`Map_*.zip` or its
//! folder: a `.mis` mission with its `.dif` interiors, `.ter` terrain,
//! textures and sky) as a map Add-On, through the same converter that
//! writes v20's own maps (docs/architecture/map-packages.md, step 5).
//!
//! The converter reads a v20 install, so the Add-On is laid out as one in a
//! scratch folder: `Add-Ons/<Map_Name>` beside the install's `base/` (hard
//! links, or copies across drives) when `--v20` names one, for the
//! textures, skies and models a map borrows from the base game.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub struct Options {
    /// `Map_Name.zip`, or its folder.
    pub input: PathBuf,
    pub out: PathBuf,
    /// A v20 install (holding `base/`) for what the map borrows from it.
    pub v20: Option<PathBuf>,
    /// The mission to take when the Add-On holds more than one.
    pub mission: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: String,
}

/// What the import made.
pub struct Imported {
    pub map_id: String,
    pub mission: String,
    pub files: usize,
}

/// A scratch folder removed when dropped.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `from`'s files under `to`, hard-linked where the drive allows it.
fn mirror(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (path, target) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_dir() {
            mirror(&path, &target)?;
        } else if kind.is_file() && std::fs::hard_link(&path, &target).is_err() {
            std::fs::copy(&path, &target).with_context(|| format!("copying {}", path.display()))?;
        }
    }
    Ok(())
}

pub fn import(opts: &Options) -> Result<Imported> {
    ensure!(
        !opts.out.exists(),
        "{} already exists; give a new folder",
        opts.out.display()
    );
    let input = opts
        .input
        .canonicalize()
        .with_context(|| format!("{} is missing", opts.input.display()))?;
    let stem = input
        .file_stem()
        .context("the map Add-On has no name")?
        .to_string_lossy()
        .into_owned();
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "bri-map-import-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    )));
    let root = scratch.0.join("v20");
    std::fs::create_dir_all(root.join("Add-Ons"))?;
    std::fs::create_dir_all(root.join("base"))?;
    if input.is_dir() {
        mirror(&input, &root.join("Add-Ons").join(&stem))?;
    } else {
        std::fs::copy(&input, root.join("Add-Ons").join(format!("{stem}.zip")))?;
    }
    if let Some(v20) = &opts.v20 {
        let base = v20.join("base");
        ensure!(
            base.is_dir(),
            "{} has no base folder; --v20 names a Blockland v20 install",
            v20.display()
        );
        mirror(&base, &root.join("base"))?;
    }

    let converted = scratch.0.join("converted");
    bri_convert::convert_root::run(vec![root.clone().into(), converted.clone().into()])
        .context("converting the map's files")?;
    let manifest: Value = serde_json::from_slice(&std::fs::read(converted.join("manifest.json"))?)?;
    let prefix = format!("add-ons/{}/", stem.to_ascii_lowercase());
    let missions: Vec<String> = manifest["records"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["virtual_path"].as_str())
        .filter(|p| {
            p.to_ascii_lowercase().starts_with(&prefix) && p.to_ascii_lowercase().ends_with(".mis")
        })
        .map(str::to_owned)
        .collect();
    let mission = match (&opts.mission, missions.as_slice()) {
        (Some(wanted), _) => missions
            .iter()
            .find(|m| {
                let m = m.to_ascii_lowercase();
                m.ends_with(&format!("/{}", wanted.to_ascii_lowercase()))
                    || m.ends_with(&format!("/{}.mis", wanted.to_ascii_lowercase()))
            })
            .cloned()
            .with_context(|| {
                format!("no mission {wanted} in it; it has {}", missions.join(", "))
            })?,
        (None, [one]) => one.clone(),
        (None, []) => bail!("{} holds no .mis mission", input.display()),
        (None, many) => bail!(
            "it holds {} missions ({}); pick one with --mission",
            many.len(),
            many.join(", ")
        ),
    };

    let bundle_dir = scratch.0.join("bundle");
    bri_convert::map_bundle::run(vec![
        root.to_string_lossy().into_owned(),
        converted.to_string_lossy().into_owned(),
        bundle_dir.to_string_lossy().into_owned(),
        mission.clone(),
    ])
    .with_context(|| {
        format!(
            "packaging {mission}{}",
            if opts.v20.is_none() {
                " (if it names base textures or models, add --v20 with your Blockland v20 folder)"
            } else {
                ""
            }
        )
    })?;

    // One map under the Add-On's own name.
    let id = opts.id.clone().unwrap_or_else(|| {
        let lower = stem.to_ascii_lowercase();
        lower.strip_prefix("map_").unwrap_or(&lower).to_string()
    });
    let ns = crate::namespace_for(&id)?;
    let map_id = format!("{ns}:map/{id}");
    let mut bundle: Value =
        serde_json::from_slice(&std::fs::read(bundle_dir.join("bundle.json"))?)?;
    let entry = bundle["maps"][0].clone();
    let old = entry["id"]
        .as_str()
        .context("the bundle names no map")?
        .to_string();
    let scene_file = entry["file"]
        .as_str()
        .context("the bundle names no scene")?
        .to_string();
    let mut scene: Value = serde_json::from_slice(&std::fs::read(bundle_dir.join(&scene_file))?)?;
    let name = opts
        .name
        .clone()
        .or_else(|| entry["name"].as_str().map(str::to_owned))
        .unwrap_or_else(|| id.clone());
    scene["id"] = json!(map_id);
    scene["name"] = json!(name);
    bundle["maps"] = json!([{ "id": map_id, "name": name, "file": scene_file }]);
    if let Value::Object(top) = &mut bundle {
        for (_, value) in top.iter_mut() {
            if let Value::Object(by_map) = value
                && let Some(record) = by_map.remove(&old)
            {
                by_map.insert(map_id.clone(), record);
            }
        }
    }

    let map = opts.out.join("map");
    std::fs::create_dir_all(&map)?;
    let mut files = 0;
    for entry in std::fs::read_dir(&bundle_dir)? {
        let entry = entry?;
        let file = entry.file_name();
        if file == "bundle.json" || file == *scene_file.as_str() {
            continue;
        }
        std::fs::copy(entry.path(), map.join(&file))?;
        files += 1;
    }
    std::fs::write(map.join(&scene_file), serde_json::to_vec_pretty(&scene)?)?;
    std::fs::write(map.join("bundle.json"), serde_json::to_vec_pretty(&bundle)?)?;
    let manifest = json!({
        "schema_version": 1,
        "id": ns,
        "version": opts.version,
        "api": 1,
        "name": name,
        "description": format!("{name}, a Blockland map ({stem}) imported by bri-import-addon map."),
        "authors": [],
        "license": "proprietary",
        "provenance": { "source": format!("Blockland map Add-On {stem}, mission {mission}"), "notes": "Imported by bri-import-addon map. Set the licence of the original map." },
        "dependencies": {},
        "capabilities": [],
        "provides": [
            { "kind": "map", "id": map_id, "file": "map/bundle.json" },
        ],
    });
    std::fs::write(
        opts.out.join("package.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(Imported {
        map_id,
        mission,
        files: files + 3,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A v3 `.ter`: a slope rising along x, one grass layer.
    fn ter() -> Vec<u8> {
        let mut out = vec![3u8];
        for _y in 0..256 {
            for x in 0..256u16 {
                out.extend((x * 16).to_le_bytes());
            }
        }
        out.extend(vec![0u8; 65536]);
        let name = b"Add-Ons/Map_Hills/grass";
        out.push(name.len() as u8);
        out.extend(name);
        // The other seven layer names are empty.
        out.extend([0u8; 7]);
        out.extend(vec![255u8; 65536]);
        out.extend(0u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out
    }

    const MISSION: &str = "//--- OBJECT WRITE BEGIN ---\r\n\
new SimGroup(MissionGroup) {\r\n\
   new ScriptObject(MissionInfo) { name = \"Rolling Hills\"; };\r\n\
   new Sun() { azimuth = \"238\"; elevation = \"33\"; color = \"0.6 0.6 0.6 1\"; ambient = \"0.4 0.4 0.4 1\"; };\r\n\
   new TerrainBlock(Terrain) { terrainFile = \"Add-Ons/Map_Hills/hills.ter\"; squareSize = \"8\"; };\r\n\
   new SpawnSphere() { position = \"0 0 40\"; };\r\n\
};\r\n\
//--- OBJECT WRITE END ---\r\n";

    #[test]
    fn an_old_blockland_map_add_on_becomes_a_map_add_on() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("Map_Hills.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        let mut grass = Vec::new();
        image::RgbImage::from_pixel(4, 4, image::Rgb([40, 150, 40]))
            .write_to(
                &mut std::io::Cursor::new(&mut grass),
                image::ImageFormat::Png,
            )
            .unwrap();
        for (name, bytes) in [
            ("hills.mis", MISSION.as_bytes().to_vec()),
            ("hills.ter", ter()),
            ("grass.png", grass),
            ("description.txt", b"Title: Rolling Hills".to_vec()),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
        let out = dir.path().join("hills");
        let made = import(&Options {
            input: zip_path,
            out: out.clone(),
            v20: None,
            mission: None,
            id: None,
            name: None,
            version: "1.0.0".into(),
        })
        .unwrap();
        assert_eq!(made.map_id, "hills:map/hills");
        let check = bri_package_runtime::check::check(&out);
        assert!(check.ok, "{check}");
        let native = bri_sim::map::NativeMap::load(&out.join("map"), &made.map_id).unwrap();
        assert_eq!(native.terrain.len(), 1);
        assert_eq!(native.scene.name, "Rolling Hills");
        for drawn in [
            bri_render::scene_loader::load_map_bundle(&out.join("map"), &made.map_id).unwrap(),
            bri_render::scene_loader::load_map_bundle_dynamic(&out.join("map"), &made.map_id)
                .unwrap(),
        ] {
            assert_eq!(drawn.terrain.len(), 1);
        }
    }
}
