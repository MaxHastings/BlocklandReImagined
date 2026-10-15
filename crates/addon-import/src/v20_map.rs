//! `bri-import-addon map`: an old Blockland map Add-On (`Map_*.zip` or its
//! folder: a `.mis` mission with its `.dif` interiors, `.ter` terrain,
//! textures and sky) as a map Add-On, through the same converter that
//! writes v20's own maps (docs/architecture/map-packages.md, step 5).
//!
//! The converter reads a v20 install, so the Add-On is laid out as one in a
//! scratch folder: `Add-Ons/<Map_Name>` beside the install's `base/` (hard
//! links, or copies across drives) when `--v20` names one, for the
//! textures, skies and models a map borrows from the base game.
use anyhow::{Context, Result, ensure};
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
    /// One map per mission, in the missions' order.
    pub map_ids: Vec<String>,
    pub missions: Vec<String>,
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

/// Other Add-Ons named by the map's missions and terrains (a terrain's
/// layers can be another map's textures).
fn borrowed(input: &Path, stem: &str) -> Result<Vec<String>> {
    let mut missions = Vec::new();
    if input.is_dir() {
        for entry in std::fs::read_dir(input)? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mis") || e.eq_ignore_ascii_case("ter"))
            {
                missions.push(std::fs::read(path)?);
            }
        }
    } else {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(input)?)?;
        for i in 0..zip.len() {
            let mut member = zip.by_index(i)?;
            let name = member.name().to_ascii_lowercase();
            if (name.ends_with(".mis") || name.ends_with(".ter")) && member.size() < 16 << 20 {
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut member, &mut bytes)?;
                missions.push(bytes);
            }
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for text in missions {
        let text = String::from_utf8_lossy(&text).replace('\\', "/");
        let lower = text.to_ascii_lowercase();
        // `add-ons/Name/...` and the mission's mod-root form `~/Name/...`.
        let starts = lower
            .match_indices("add-ons/")
            .map(|(at, m)| at + m.len())
            .chain(lower.match_indices("~/").map(|(at, m)| at + m.len()));
        for start in starts {
            let name: String = text[start..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            // A folder name, so followed by a slash.
            let folder = text[start + name.len()..].starts_with('/');
            if folder && !name.is_empty() && !name.eq_ignore_ascii_case(stem) {
                names.insert(name);
            }
        }
    }
    Ok(names.into_iter().collect())
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
        // Other Add-Ons the mission borrows from (a v20 map's buildings).
        // They are looked for in the install's Add-Ons, then beside the map.
        let beside = input.parent().map(Path::to_path_buf);
        let places: Vec<PathBuf> = [Some(v20.join("Add-Ons")), beside]
            .into_iter()
            .flatten()
            .collect();
        for other in borrowed(&input, &stem)? {
            let found = places.iter().find_map(|place| {
                let zip = place.join(format!("{other}.zip"));
                let folder = place.join(&other);
                (zip.is_file() || folder.is_dir()).then_some((zip, folder))
            });
            match found {
                Some((zip, _)) if zip.is_file() => {
                    std::fs::copy(&zip, root.join("Add-Ons").join(format!("{other}.zip")))?;
                }
                Some((_, folder)) => mirror(&folder, &root.join("Add-Ons").join(&other))?,
                None => eprintln!(
                    "warning: the map uses files from {other}, which is not in {}",
                    places
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" or ")
                ),
            }
        }
    }

    let converted = scratch.0.join("converted");
    let conversion =
        bri_convert::convert_root::run(vec![root.clone().into(), converted.clone().into()]);
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(converted.join("manifest.json"))
            .with_context(|| format!("converting the map's files: {conversion:?}"))?,
    )?;
    // The reference also contains unrelated legacy bricks and editor shapes.
    // Failed base assets remain diagnostics; map_bundle requires a converted
    // output for every asset the chosen mission actually references.
    if let Err(error) = conversion {
        let mut fatal = Vec::new();
        for record in manifest["records"].as_array().into_iter().flatten() {
            if record["error"].is_null() {
                continue;
            }
            let diagnostic = format!("{}: {}", record["virtual_path"], record["error"]);
            if record["virtual_path"]
                .as_str()
                .is_some_and(|path| path.starts_with("base/"))
            {
                eprintln!("warning: reference asset did not convert: {diagnostic}");
            } else {
                fatal.push(diagnostic);
            }
        }
        fatal.extend(
            manifest["scan_errors"]
                .as_array()
                .into_iter()
                .flatten()
                .map(ToString::to_string),
        );
        ensure!(
            fatal.is_empty(),
            "converting the map's files: {error:#}: {}",
            fatal.join("; ")
        );
    }
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
    let mut chosen: Vec<String> = match &opts.mission {
        Some(wanted) => vec![
            missions
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
        ],
        // Every mission becomes a map of the one Add-On (a day and a night
        // version of the same place).
        None => missions.clone(),
    };
    ensure!(
        !chosen.is_empty(),
        "{} holds no .mis mission",
        input.display()
    );
    // map_bundle lists its maps in mission order.
    chosen.sort();

    // The maps under the Add-On's own name: one map takes the Add-On's id,
    // several take their missions' names. Each map is its own bundle (a
    // map's bundle.json lists just that map), in map/ for one and
    // map/<id>/ for several.
    let addon_id = opts.id.clone().unwrap_or_else(|| {
        let lower = stem.to_ascii_lowercase();
        lower.strip_prefix("map_").unwrap_or(&lower).to_string()
    });
    let ns = crate::namespace_for(&addon_id)?;
    let mut maps = Vec::new();
    let mut files = 0;
    for (k, mission) in chosen.iter().enumerate() {
        let bundle_dir = scratch.0.join(format!("bundle{k}"));
        bri_convert::map_bundle::run(vec![
            root.to_string_lossy().into_owned(),
            converted.to_string_lossy().into_owned(),
            bundle_dir.to_string_lossy().into_owned(),
            "--stand-in-missing-terrain-textures".into(),
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
        let id = if chosen.len() == 1 {
            addon_id.clone()
        } else {
            let stem = mission.rsplit('/').next().unwrap_or(mission);
            let stem = stem.rsplit_once('.').map_or(stem, |(stem, _)| stem);
            crate::namespace_for(stem)?
        };
        let map_id = format!("{ns}:map/{id}");
        let mut scene: Value =
            serde_json::from_slice(&std::fs::read(bundle_dir.join(&scene_file))?)?;
        let name = match (&opts.name, chosen.len()) {
            (Some(name), 1) => name.clone(),
            _ => entry["name"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| id.clone()),
        };
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
        let folder = if chosen.len() == 1 {
            "map".to_string()
        } else {
            format!("map/{id}")
        };
        let dir = opts.out.join(&folder);
        std::fs::create_dir_all(&dir)?;
        for entry in std::fs::read_dir(&bundle_dir)? {
            let entry = entry?;
            let file = entry.file_name();
            if file == "bundle.json" || file == *scene_file.as_str() {
                continue;
            }
            std::fs::copy(entry.path(), dir.join(&file))?;
            files += 1;
        }
        files += foliage(&root, mission, &map_id, &scene, &dir)?;
        files += emitters(&root, &stem, &ns, &scene, &dir)?;
        std::fs::write(dir.join(&scene_file), serde_json::to_vec_pretty(&scene)?)?;
        std::fs::write(dir.join("bundle.json"), serde_json::to_vec_pretty(&bundle)?)?;
        files += 2;
        maps.push((map_id, name, format!("{folder}/bundle.json")));
    }

    let name = match (&opts.name, maps.as_slice()) {
        (Some(name), _) => name.clone(),
        (None, [(_, name, _)]) => name.clone(),
        (None, _) => stem.clone(),
    };
    let provides: Vec<Value> = maps
        .iter()
        .map(|(id, _, file)| json!({ "kind": "map", "id": id, "file": file }))
        .collect();
    let manifest = json!({
        "schema_version": 1,
        "id": ns,
        "version": opts.version,
        "api": 1,
        "name": name,
        "description": format!("{name}, a Blockland map ({stem}) imported by bri-import-addon map."),
        "authors": [],
        "license": "proprietary",
        "provenance": { "source": format!("Blockland map Add-On {stem}, missions {}", chosen.join(", ")), "notes": "Imported by bri-import-addon map. Set the licence of the original map." },
        "dependencies": {},
        "capabilities": [],
        "provides": provides,
    });
    std::fs::write(
        opts.out.join("package.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(Imported {
        map_ids: maps.into_iter().map(|(id, _, _)| id).collect(),
        missions: chosen,
        files: files + 1,
    })
}

/// The map's grass and foliage replicators as a foliage pack beside its
/// bundle (`foliage.json` and its images), which the game places on the
/// map as it loads. Returns how many files it wrote.
fn foliage(root: &Path, mission: &str, map_id: &str, scene: &Value, dir: &Path) -> Result<usize> {
    use bri_foliage::{Definition, Evidence, FoliagePack, Texture};
    use sha2::{Digest, Sha256};
    let text = bri_convert::environment::read_original(root, mission)?.unwrap_or_default();
    let mut pack = FoliagePack {
        schema_version: 1,
        definitions: vec![],
        textures: vec![],
        source_bundle: "bundle.json".into(),
    };
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    for (index, node) in scene["nodes"].as_array().into_iter().flatten().enumerate() {
        if node["kind"] != "foliage" {
            continue;
        }
        let properties: std::collections::BTreeMap<String, String> =
            serde_json::from_value(node["properties"].clone())?;
        let (_, grass) = bri_foliage::replicator_image(&properties)?;
        let reference = &properties[if grass { "grassfile" } else { "foliagefile" }];
        let image = bri_convert::water::resource(root, mission, reference, dir)
            .with_context(|| format!("the image {reference} of a grass or foliage replicator"))?;
        let texture = match pack.textures.iter().position(|t| t.sha256 == image.sha256) {
            Some(texture) => texture,
            None => {
                pack.textures.push(Texture {
                    id: format!("{map_id}/foliage/texture/{}", pack.textures.len()),
                    path: image.file.clone(),
                    sha256: image.sha256.clone(),
                    source: image.source.clone(),
                });
                pack.textures.len() - 1
            }
        };
        // The replicator's declaration in the mission, for its evidence.
        let class = if grass {
            "new fxgrassreplicator("
        } else {
            "new fxfoliagereplicator("
        };
        let nth = seen.entry(class.into()).or_default();
        let line = String::from_utf8_lossy(&text)
            .lines()
            .enumerate()
            .filter(|(_, l)| l.to_ascii_lowercase().contains(class))
            .nth(*nth)
            .map_or(0, |(k, _)| k + 1);
        *nth += 1;
        let transform = node["transform"]
            .as_array()
            .context("a node without a transform")?;
        pack.definitions.push(Definition::from_replicator(
            map_id,
            index,
            [12, 13, 14].map(|k| transform.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32),
            &properties,
            texture,
            Evidence {
                source: mission.to_string(),
                sha256: format!("{:x}", Sha256::digest(&text)),
                line,
                fields: properties.clone(),
                adaptations: vec![],
            },
        )?);
    }
    if pack.definitions.is_empty() {
        return Ok(0);
    }
    pack.validate()?;
    pack.images(dir)?;
    std::fs::write(dir.join("foliage.json"), serde_json::to_vec_pretty(&pack)?)?;
    Ok(1)
}

/// The map's particle emitters (`ParticleEmitterNode`s: waterfalls, mist)
/// as `map_effects.json` beside its bundle, from the particle and emitter
/// datablocks its own scripts declare. Returns how many files it wrote.
fn emitters(root: &Path, stem: &str, ns: &str, scene: &Value, dir: &Path) -> Result<usize> {
    use bri_content::effects::{MapEffects, MapEmitter};
    use bri_convert::{effect_script::Declarations, effects};
    let placed: Vec<(usize, &Value)> = scene["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter(|(_, n)| n["properties"]["source_class"] == "particleemitternode")
        .collect();
    if placed.is_empty() {
        return Ok(0);
    }
    // The Add-On's own scripts, read for their datablocks only.
    let mut declarations = Declarations::default();
    for (origin, text) in scripts(root, stem)? {
        if let Err(e) = declarations.read(&text, &origin) {
            eprintln!("warning: {origin}: {e:#}; its effects are left out");
        }
    }
    let find = |class: &str, name: &str| {
        declarations
            .entries
            .iter()
            .rev()
            .find(|d| d.class.eq_ignore_ascii_case(class) && d.name.eq_ignore_ascii_case(name))
    };
    let rename = |id: &str| {
        let (kind, name) = id
            .strip_prefix("v20/")
            .and_then(|r| r.split_once('/'))
            .unwrap_or(("effect", id));
        format!("{ns}:{kind}/{name}")
    };
    let mut out = MapEffects {
        schema_version: 1,
        particles: vec![],
        emitters: vec![],
        sources: vec![],
    };
    for (index, node) in placed {
        let name = node["properties"]["emitter"].as_str().unwrap_or_default();
        let emitter_id = format!("{ns}:emitter/{}", name.to_ascii_lowercase());
        if !out.emitters.iter().any(|e| e.id == emitter_id) {
            let Some(d) = find("ParticleEmitterData", name) else {
                eprintln!(
                    "warning: the map places emitter {name}, which its scripts do not declare; it is left out"
                );
                continue;
            };
            let converted = effects::emitter(d, &Default::default()).and_then(|(mut e, _)| {
                let mut particles = vec![];
                for id in &e.particles {
                    let particle = id.rsplit('/').next().unwrap_or(id);
                    let pd = find("ParticleData", particle).with_context(|| {
                        format!("its particle {particle} is not declared by the map")
                    })?;
                    let (mut p, _) = effects::particle(pd)?;
                    p.id = rename(&p.id);
                    particles.push(p);
                }
                e.id = emitter_id.clone();
                e.particles = e.particles.iter().map(|p| rename(p)).collect();
                Ok((e, particles))
            });
            match converted {
                Ok((e, particles)) => {
                    for p in particles {
                        if !out.particles.iter().any(|q| q.id == p.id) {
                            out.particles.push(p);
                        }
                    }
                    out.emitters.push(e);
                }
                Err(e) => {
                    eprintln!("warning: emitter {name}: {e:#}; it is left out");
                    continue;
                }
            }
        }
        let transform = node["transform"]
            .as_array()
            .context("a node without a transform")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<_>>();
        let matrix = glam::Mat4::from_cols_slice(&transform);
        let (_, rotation, position) = matrix.to_scale_rotation_translation();
        out.sources.push(MapEmitter {
            node: index,
            emitter: emitter_id,
            position: position.to_array(),
            rotation: rotation.normalize().to_array(),
            velocity: node["properties"]["velocity"]
                .as_str()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(1.0),
        });
    }
    if out.sources.is_empty() {
        return Ok(0);
    }
    out.validate()?;
    std::fs::write(
        dir.join("map_effects.json"),
        serde_json::to_vec_pretty(&out)?,
    )?;
    Ok(1)
}

/// The Add-On's `.cs` scripts (folder or zip, as it was staged), sorted by
/// path, as (`Add-Ons/<stem>/path`, text).
fn scripts(root: &Path, stem: &str) -> Result<Vec<(String, String)>> {
    let mut out = vec![];
    let is_script = |name: &str| name.to_ascii_lowercase().ends_with(".cs");
    let folder = root.join("Add-Ons").join(stem);
    let zip = root.join("Add-Ons").join(format!("{stem}.zip"));
    if folder.is_dir() {
        let mut stack = vec![folder.clone()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d)? {
                let path = entry?.path();
                if path.is_dir() {
                    stack.push(path);
                } else if is_script(&path.to_string_lossy()) {
                    let relative = path
                        .strip_prefix(&folder)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    let text = String::from_utf8_lossy(&std::fs::read(&path)?).into_owned();
                    out.push((format!("Add-Ons/{stem}/{relative}"), text));
                }
            }
        }
    } else if zip.is_file() {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip)?)?;
        for k in 0..archive.len() {
            let mut entry = archive.by_index(k)?;
            let name = entry.name().replace('\\', "/");
            if entry.is_file() && is_script(&name) && entry.size() < 4 << 20 {
                let mut bytes = vec![];
                std::io::Read::read_to_end(&mut entry, &mut bytes)?;
                out.push((
                    format!("Add-Ons/{stem}/{name}"),
                    String::from_utf8_lossy(&bytes).into_owned(),
                ));
            }
        }
    }
    out.sort();
    Ok(out)
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
   new ParticleEmitterNode(Fall) { position = \"0 0 45\"; rotation = \"1 0 0 0\"; scale = \"1 1 1\"; dataBlock = \"GenericEmitterNode\"; emitter = \"HillsFallEmitter\"; velocity = \"1\"; };\r\n\
   new fxGrassReplicator(Grass) { position = \"0 0 40\"; seed = \"1376312589\"; grassFile = \"./tuft\"; grassCount = \"50\"; grassRetries = \"100\"; outerRadiusX = \"60\"; outerRadiusY = \"60\"; minWidth = \"1\"; maxWidth = \"2\"; minHeight = \"1\"; maxHeight = \"2\"; };\r\n\
};\r\n\
//--- OBJECT WRITE END ---\r\n";

    fn read_map_effects(path: &Path) -> bri_content::effects::MapEffects {
        let effects: bri_content::effects::MapEffects =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        effects.validate().unwrap();
        effects
    }

    #[test]
    fn broken_map_assets_remain_fatal_and_name_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("Map_Broken");
        std::fs::create_dir(&input).unwrap();
        std::fs::write(input.join("broken.dts"), b"invalid map shape").unwrap();
        let error = import(&Options {
            input,
            out: dir.path().join("out"),
            v20: None,
            mission: None,
            id: None,
            name: None,
            version: "1.0.0".into(),
        })
        .err()
        .expect("broken source assets must fail");
        assert!(
            format!("{error:#}")
                .to_ascii_lowercase()
                .contains("add-ons/map_broken/broken.dts"),
            "{error:#}"
        );
    }

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
            ("grass.png", grass.clone()),
            ("tuft.png", grass),
            ("hills_resources.cs", br#"datablock ParticleData(HillsFallParticle) { textureName = "base/data/particles/cloud"; lifetimeMS = 1000; };
datablock ParticleEmitterData(HillsFallEmitter) { ejectionPeriodMS = 10; periodVarianceMS = 2; particles = HillsFallParticle; };
"#.to_vec()),
            ("description.txt", b"Title: Rolling Hills".to_vec()),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
        // The plain Import (the Add-Ons screen's, and the release bundle's)
        // hands a map Add-On to this converter, with its report.
        let report = crate::import(&crate::Options {
            input: zip_path.clone(),
            out: dir.path().join("hills-plain"),
            reference: None,
            core: vec![],
            installed: None,
            version: "1.0.0".into(),
        })
        .unwrap();
        assert_eq!(report.package.id, "map_hills");
        assert_eq!(report.source.title, "Rolling Hills");
        assert_eq!(report.summary.verdict, "converted");
        assert!(dir.path().join("hills-plain/import-report.json").is_file());
        assert!(dir.path().join("hills-plain/map/bundle.json").is_file());

        let reference = dir.path().join("reference");
        std::fs::create_dir_all(reference.join("base")).unwrap();
        std::fs::write(reference.join("base/unused.dts"), b"invalid editor shape").unwrap();
        let out = dir.path().join("hills");
        let made = import(&Options {
            input: zip_path,
            out: out.clone(),
            v20: Some(reference),
            mission: None,
            id: None,
            name: None,
            version: "1.0.0".into(),
        })
        .unwrap();
        assert_eq!(made.map_ids, ["hills:map/hills"]);
        let check = bri_package_runtime::check::check(&out);
        assert!(check.ok, "{check}");
        let native = bri_sim::map::NativeMap::load(&out.join("map"), &made.map_ids[0]).unwrap();
        assert_eq!(native.terrain.len(), 1);
        assert_eq!(native.scene.name, "Rolling Hills");
        // Its grass comes as a foliage pack beside the bundle.
        let pack = bri_foliage::FoliagePack::load(out.join("map/foliage.json")).unwrap();
        assert_eq!(pack.definitions.len(), 1);
        assert_eq!(pack.definitions[0].scene, made.map_ids[0]);
        assert_eq!(pack.definitions[0].count, 50);
        assert_eq!(pack.images(out.join("map")).unwrap().len(), 1);
        // Its waterfall emitter, from the datablocks its script declares.
        let effects = read_map_effects(&out.join("map/map_effects.json"));
        assert_eq!(effects.emitters[0].id, "hills:emitter/hillsfallemitter");
        assert_eq!(effects.particles[0].texture, "base/data/particles/cloud");
        assert_eq!(effects.sources.len(), 1);
        assert_eq!(effects.sources[0].position, [0.0, 45.0, 0.0]);
        for drawn in [
            bri_render::scene_loader::load_map_bundle(&out.join("map"), &made.map_ids[0]).unwrap(),
            bri_render::scene_loader::load_map_bundle_dynamic(&out.join("map"), &made.map_ids[0])
                .unwrap(),
        ] {
            assert_eq!(drawn.terrain.len(), 1);
        }
    }
}
