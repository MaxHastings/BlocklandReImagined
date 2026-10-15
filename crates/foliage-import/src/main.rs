use anyhow::{Context, Result, ensure};
use bri_foliage::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4,
        "usage: bri-foliage-import SOURCE_ROOT NATIVE_MAP_BUNDLE NEW_OUTPUT"
    );
    let source = Path::new(&args[1]);
    let bundle = Path::new(&args[2]);
    let out = Path::new(&args[3]);
    ensure!(!out.exists(), "output exists");
    let zip_path = source.join("Add-Ons/Map_Bedroom.zip");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&zip_path)?)?;
    let mut mission = String::new();
    zip.by_name("bedroom.mis")?.read_to_string(&mut mission)?;
    let mut pack = FoliagePack {
        schema_version: 1,
        definitions: vec![],
        textures: vec![],
        // The bundle's directory name, not where this machine keeps it: the
        // pack is shared, so its bytes cannot depend on a local path.
        source_bundle: std::path::Path::new(&args[2])
            .file_name()
            .map_or_else(|| args[2].clone(), |n| n.to_string_lossy().into_owned()),
    };
    let mut images = vec![];
    let mut scenes: Vec<_> = std::fs::read_dir(bundle)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".scene.json"))
        .collect();
    scenes.sort();
    for scene_path in scenes {
        let scene: Value = serde_json::from_slice(&std::fs::read(scene_path)?)?;
        let scene_id = scene["id"].as_str().context("scene id")?;
        for (index, node) in scene["nodes"]
            .as_array()
            .context("nodes")?
            .iter()
            .enumerate()
        {
            if node["kind"] != "foliage" {
                continue;
            }
            ensure!(
                scene_id.ends_with("map_bedroom/bedroom.mis"),
                "new foliage map requires source provenance"
            );
            let f: BTreeMap<String, String> = serde_json::from_value(node["properties"].clone())?;
            let (rel, grass) = replicator_image(&f)?;
            let prefix = if grass { "grass" } else { "foliage" };
            let relative = Path::new(&rel);
            ensure!(
                !relative.is_absolute()
                    && relative
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "unsafe source resource"
            );
            let parent = source.join(relative.parent().unwrap());
            let file = std::fs::read_dir(parent)?
                .filter_map(|e| e.ok())
                .find(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(relative.file_name().unwrap().to_str().unwrap())
                })
                .context("missing original foliage image")?
                .path();
            let bytes = std::fs::read(file)?;
            let sha = hash(&bytes);
            let texture = pack.textures.len();
            pack.textures.push(Texture {
                id: format!("v20.foliage.texture.{prefix}"),
                path: format!("{sha}.png"),
                sha256: sha,
                source: rel.clone(),
            });
            images.push(bytes);
            let transform = node["transform"].as_array().context("native transform")?;
            let class = if grass {
                "fxGrassReplicator"
            } else {
                "fxFoliageReplicator"
            };
            let line = mission
                .lines()
                .position(|l| l.contains(&format!("new {class}(")))
                .context("source class declaration")?
                + 1;
            let definition = Definition::from_replicator(
                scene_id,
                index,
                [12, 13, 14].map(|k| transform[k].as_f64().unwrap_or(0.0) as f32),
                &f,
                texture,
                Evidence {
                    source: "Add-Ons/Map_Bedroom.zip::bedroom.mis".into(),
                    sha256: hash(mission.as_bytes()),
                    line,
                    fields: f.clone(),
                    adaptations: vec![],
                },
            )?;
            pack.definitions.push(definition);
        }
    }
    pack.validate()?;
    std::fs::create_dir(out)?;
    for (texture, bytes) in pack.textures.iter().zip(images) {
        std::fs::write(out.join(&texture.path), bytes)?;
    }
    std::fs::write(out.join("foliage.json"), serde_json::to_vec_pretty(&pack)?)?;
    pack.images(out)?;
    println!(
        "{} replicators / {} requested plants / {} original textures",
        pack.definitions.len(),
        pack.definitions.iter().map(|d| d.count).sum::<u32>(),
        pack.textures.len()
    );
    Ok(())
}
