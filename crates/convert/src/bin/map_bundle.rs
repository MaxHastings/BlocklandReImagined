//! Package converted map placements and their original interior/terrain textures.
use anyhow::{Context, Result, ensure};
use bri_content::{
    Terrain,
    interior::Interior,
    scene::{Kind, Scene},
    shape::Shape,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn texture(root: &Path, reference: &str) -> Result<Option<(String, Vec<u8>)>> {
    ensure!(
        bri_content::brick_materials::safe_relative(reference),
        "Unsafe texture reference"
    );
    for ext in ["png", "jpg", "jpeg"] {
        let candidate = format!("{reference}.{ext}");
        if let Some(bytes) = bri_convert::environment::read_original(root, &candidate)? {
            return Ok(Some((candidate, bytes)));
        }
    }
    Ok(None)
}
fn shape_texture(root: &Path, source: &str, name: &str) -> Result<Option<String>> {
    ensure!(
        bri_content::brick_materials::safe_relative(name),
        "Unsafe shape material name"
    );
    let mut directory = source.rsplit_once('/').context("Shape lacks directory")?.0;
    let root_depth = if source
        .get(..8)
        .is_some_and(|s| s.eq_ignore_ascii_case("Add-Ons/"))
    {
        2
    } else {
        1
    };
    // The original TS material lookup walks parents; trees share images one
    // directory above their models. Never search outside the authored mod root.
    loop {
        let candidate = format!("{directory}/{name}");
        if texture(root, &candidate)?.is_some() {
            return Ok(Some(candidate));
        }
        if directory.split('/').count() <= root_depth {
            break;
        }
        let Some((parent, _)) = directory.rsplit_once('/') else {
            break;
        };
        directory = parent;
    }
    Ok(None)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 4,
        "Usage: map_bundle <v20-root> <converted-dir> <new-output-dir> [--core-script <recovered-core-cs>] <mission-virtual-path> ..."
    );
    let root = PathBuf::from(&args[0]).canonicalize()?;
    let content = PathBuf::from(&args[1]).canonicalize()?;
    let output = PathBuf::from(&args[2]);
    let mut missions = Vec::new();
    let mut core_script = None;
    let mut at = 3;
    while at < args.len() {
        if args[at] == "--core-script" {
            ensure!(core_script.is_none(), "Duplicate core script");
            at += 1;
            core_script = Some(PathBuf::from(args.get(at).context("Missing core script")?));
        } else {
            ensure!(!args[at].starts_with("--"), "Unknown map-bundle option");
            missions.push(&args[at]);
        }
        at += 1;
    }
    ensure!(!missions.is_empty(), "No mission paths supplied");
    // The bundle is a shared package whose hash every player must match, so
    // its map order cannot follow the order a caller happened to list them.
    missions.sort();
    missions.dedup();
    ensure!(!output.exists(), "Use a new bundle directory");
    let parent = output
        .parent()
        .context("Output has no parent")?
        .canonicalize()?;
    ensure!(
        !parent.starts_with(&root) && !parent.starts_with(&content),
        "Output must be outside original/native source content"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(content.join("manifest.json"))?)?;
    let records = manifest["records"]
        .as_array()
        .context("Missing conversion manifest")?;
    let lookup = |reference: &str| -> Result<&str> {
        let path = reference.strip_prefix("v20/").unwrap_or(reference);
        let found: Vec<_> = records
            .iter()
            .filter(|r| {
                r["virtual_path"]
                    .as_str()
                    .is_some_and(|p| p.eq_ignore_ascii_case(path))
            })
            .collect();
        ensure!(found.len() == 1, "Missing/ambiguous asset {reference}");
        let original_path = found[0]["virtual_path"]
            .as_str()
            .context("Missing original source path")?;
        let original = bri_convert::environment::read_original(&root, original_path)?
            .with_context(|| format!("Missing reference asset {original_path}"))?;
        ensure!(
            found[0]["source_sha256"].as_str()
                == Some(format!("{:x}", Sha256::digest(&original)).as_str()),
            "Converted source differs from reference: {original_path}"
        );
        let file = found[0]["output"]
            .as_str()
            .with_context(|| format!("Asset conversion failed {reference}"))?;
        ensure!(
            !file.contains(['/', '\\', ':']),
            "Unsafe native content filename"
        );
        Ok(file)
    };
    std::fs::create_dir_all(&output)?;
    let mut assets = BTreeMap::new();
    let mut maps = Vec::new();
    let mut bindings = Vec::new();
    let mut textures = BTreeMap::new();
    let mut unresolved = Vec::new();
    let mut lighting = BTreeMap::new();
    let mut environments = BTreeMap::new();
    let mut waters = BTreeMap::new();
    let mut declarations = bri_convert::effect_script::Declarations::default();
    let mut script_hash = None;
    if let Some(path) = core_script {
        let text = std::fs::read_to_string(path)?;
        script_hash = Some(format!("{:x}", Sha256::digest(text.as_bytes())));
        declarations.read_classes(
            &text,
            "base/server/scripts/allGameScripts.cs",
            &["StaticShapeData"],
        )?;
    }
    let mut scenes = Vec::new();
    for path in &missions {
        let scene_file = lookup(path)?;
        let mut scene: Scene = serde_json::from_slice(&std::fs::read(content.join(scene_file))?)?;
        if script_hash.is_some() {
            for node in &mut scene.nodes {
                if matches!(node.kind, Kind::DatablockModel) {
                    let name = node
                        .properties
                        .get("datablock")
                        .context("Static shape lacks datablock")?;
                    let declaration = declarations
                        .entries
                        .iter()
                        .find(|d| d.name.eq_ignore_ascii_case(name))
                        .with_context(|| {
                            format!("Missing literal static shape datablock {name}")
                        })?;
                    let path = declaration
                        .fields
                        .get("shapefile")
                        .context("Static datablock lacks shapeFile")?;
                    let path = if let Some(p) = path.strip_prefix("~/") {
                        format!("base/{p}")
                    } else {
                        path.clone()
                    };
                    ensure!(
                        bri_content::brick_materials::safe_relative(&path),
                        "Unsafe datablock shape path"
                    );
                    node.asset = Some(format!("v20/{}", path.to_lowercase()));
                    if name.eq_ignore_ascii_case("LCD") {
                        node.properties
                            .insert("native_initial_sequence".into(), "time0".into());
                        node.properties.insert(
                            "native_behavior_pending".into(),
                            "Clock minute advancement and timed explosions".into(),
                        );
                    } else if name.eq_ignore_ascii_case("LCDColon") {
                        node.properties
                            .insert("native_initial_sequence".into(), "blink".into());
                        node.properties.insert(
                            "native_behavior_pending".into(),
                            "Clock colon blinking".into(),
                        );
                    } else {
                        node.properties.insert(
                            "native_behavior_pending".into(),
                            "Static object damage, destruction and repair".into(),
                        );
                    }
                }
            }
        }
        scenes.push((scene_file.to_string(), scene));
    }
    let mut export_texture = |reference: &str| -> Result<Option<String>> {
        if let Some((source, bytes)) = texture(&root, reference)? {
            let file = format!(
                "{:x}.{}",
                Sha256::digest(&bytes),
                source.rsplit('.').next().unwrap()
            );
            std::fs::write(output.join(&file), bytes)?;
            textures.insert(source, file.clone());
            Ok(Some(file))
        } else {
            Ok(None)
        }
    };
    for (scene_file, scene) in &scenes {
        std::fs::write(output.join(scene_file), serde_json::to_vec(scene)?)?;
        maps.push(serde_json::json!({"id":scene.id,"name":scene.name,"file":scene_file}));
        for node in &scene.nodes {
            let Some(id) = &node.asset else {
                continue;
            };
            if assets.contains_key(id) {
                continue;
            }
            let file = lookup(id)?;
            std::fs::copy(content.join(file), output.join(file))?;
            assets.insert(id.clone(), file.to_owned());
            let source = records
                .iter()
                .find(|r| r["output"].as_str() == Some(file))
                .unwrap()["virtual_path"]
                .as_str()
                .unwrap();
            if file.ends_with(".interior.json") {
                let interior: Interior =
                    serde_json::from_slice(&std::fs::read(content.join(file))?)?;
                interior.validate()?;
                for (detail, d) in interior.details.iter().enumerate() {
                    for (material, name) in d.materials.iter().enumerate().filter(|(i, _)| {
                        d.surfaces
                            .iter()
                            .any(|s| s.material == *i && !s.triangles.is_empty())
                    }) {
                        let reference = format!("{}/{}", source.rsplit_once('/').unwrap().0, name);
                        let texture = export_texture(&reference)?;
                        if texture.is_none() {
                            unresolved.push(reference.clone());
                        }
                        bindings.push(serde_json::json!({"asset":id,"detail":detail,"material":material,"texture":texture,"source":reference}));
                    }
                }
            } else if file.ends_with(".shape.json") {
                let shape: Shape = serde_json::from_slice(&std::fs::read(content.join(file))?)?;
                shape.validate()?;
                let skins: std::collections::BTreeSet<_> = scenes
                    .iter()
                    .flat_map(|(_, s)| &s.nodes)
                    .filter(|n| n.asset.as_ref() == Some(id))
                    .map(|n| n.properties.get("skinname").map_or("", String::as_str))
                    .collect();
                for skin in skins {
                    ensure!(
                        skin.len() <= 64
                            && skin.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                        "Invalid authored skin name"
                    );
                    for (slot, material) in shape.materials.iter().enumerate() {
                        let name = if !skin.is_empty()
                            && let Some(suffix) = material.name.strip_prefix("base.")
                        {
                            format!("{skin}.{suffix}")
                        } else {
                            material.name.clone()
                        };
                        let reference = shape_texture(&root, source, &name)?;
                        let texture = if let Some(reference) = &reference {
                            export_texture(reference)?
                        } else {
                            None
                        };
                        if texture.is_none() {
                            unresolved.push(format!("{source} material {}", material.name));
                        }
                        bindings.push(serde_json::json!({"asset":id,"shape_material":slot,"skin":skin,"texture":texture,"source":reference}));
                    }
                }
            } else if file.ends_with(".terrain.json") {
                let terrain: Terrain = serde_json::from_slice(&std::fs::read(content.join(file))?)?;
                for layer in terrain.layers {
                    let texture = export_texture(&layer.material)?;
                    if texture.is_none() {
                        unresolved.push(layer.material.clone());
                    }
                    bindings.push(serde_json::json!({"asset":id,"terrain_layer":layer.slot,"texture":texture,"source":layer.material}));
                }
            }
        }
    }
    // Converted terrain placements: spacing, origin, repetition, empty squares,
    // detail and emboss-bump parameters with packaged original textures.
    let mut terrains = BTreeMap::new();
    for (_, scene) in &scenes {
        let mut instances = Vec::new();
        for (index, node) in scene.nodes.iter().enumerate() {
            if !matches!(node.kind, Kind::Terrain) {
                continue;
            }
            let id = node.asset.as_ref().context("Terrain lacks asset")?;
            let file = assets
                .get(id)
                .context("Terrain asset missing from bundle")?;
            let terrain: Terrain = serde_json::from_slice(&std::fs::read(output.join(file))?)?;
            let mut resolve =
                |path: &str| -> Result<Option<bri_content::terrain_field::TerrainTexture>> {
                    let stem = bri_convert::terrain::image_stem(path);
                    let found = if path.len() != stem.len() {
                        bri_convert::environment::read_original(&root, path)?
                            .map(|bytes| (path.to_string(), bytes))
                    } else {
                        None
                    };
                    let Some((source, bytes)) = (match found {
                        Some(found) => Some(found),
                        None => texture(&root, stem)?,
                    }) else {
                        return Ok(None);
                    };
                    let file = format!(
                        "{:x}.{}",
                        Sha256::digest(&bytes),
                        source.rsplit('.').next().unwrap().to_lowercase()
                    );
                    std::fs::write(output.join(&file), bytes)?;
                    textures.insert(source.clone(), file.clone());
                    Ok(Some(bri_content::terrain_field::TerrainTexture {
                        file,
                        source,
                    }))
                };
            instances.push(bri_convert::terrain::instance(
                scene,
                index,
                terrain.side,
                &mut resolve,
            )?);
        }
        if !instances.is_empty() {
            terrains.insert(scene.id.clone(), instances);
        }
    }
    for (path, (_, scene)) in missions.iter().zip(&scenes) {
        if let Some(environment) = bri_convert::environment::convert(&root, path, scene, &output)? {
            environments.insert(scene.id.clone(), environment);
        }
        waters.insert(
            scene.id.clone(),
            bri_convert::water::convert(&root, scene, environments.get(&scene.id), &output)?,
        );
        let source = |id: &str| -> Result<String> {
            let file = assets.get(id).context("Unknown lit asset")?;
            records
                .iter()
                .find(|r| r["output"].as_str() == Some(file.as_str()))
                .and_then(|r| r["virtual_path"].as_str())
                .map(str::to_owned)
                .context("Lit asset has no conversion record")
        };
        // Mission lighting is baked from the originals; `.ml` caches are not read.
        lighting.insert(
            scene.id.clone(),
            bri_convert::scene_lighting::bake_scene(
                &root,
                scene,
                &assets,
                &content,
                &source,
                terrains.get(&scene.id).map_or(&[][..], Vec::as_slice),
                &output,
            )
            .with_context(|| format!("Lighting {}", scene.id))?,
        );
    }
    let report = serde_json::json!({"schema_version":1,"maps":maps,"assets":assets,"textures":textures,"bindings":bindings,"lighting":lighting,"environments":environments,"waters":waters,"terrains":terrains,"static_datablocks":declarations.entries,"static_script_sha256":script_hash,"static_script_diagnostics":declarations.diagnostics,"unresolved_textures":unresolved,"scope":"native architecture/terrain/static models/water, original textures/lighting/skies and initial static shape states; remaining dynamic map behavior, water fidelity and weather require integration"});
    std::fs::write(
        output.join("bundle.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "{} maps, {} direct native assets, {} original textures, {} unresolved textures",
        maps.len(),
        assets.len(),
        textures.len(),
        unresolved.len()
    );
    ensure!(unresolved.is_empty(), "Bundle has unresolved textures");
    Ok(())
}
