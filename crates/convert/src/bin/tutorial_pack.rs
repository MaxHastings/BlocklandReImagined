//! Assemble the native tutorial pack from the bound Map_Tutorial worlds
//! (`import_saves` then `bind_world_events` over the add-on's three saves),
//! its `targetSetup.txt`, and the target models and textures in
//! `Map_Tutorial.zip`.
use anyhow::{Context, Result, ensure};
use bri_content::tutorial::{PACK_INDEX, PACK_SCHEMA, PackIndex, TARGET_SHAPES, TARGET_SKINS};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::PathBuf};

/// Largest file read from the archive.
const MAX_FILE: u64 = 16 << 20;

fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 4,
        "Usage: tutorial_pack <bound-tutorial-worlds-dir> <targetSetup.txt> <Map_Tutorial.zip> <new-output-dir>"
    );
    let output = &args[3];
    ensure!(!output.exists(), "Use a new output directory");
    let mut parts = BTreeMap::new();
    for entry in std::fs::read_dir(&args[0])? {
        let path = entry?.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".world.json") {
            continue;
        }
        let world = bri_world::persistence::load(&path)?;
        ensure!(
            world.map_id == "v20/add-ons/map_tutorial/tutorial.mis",
            "{name} is not a Tutorial world"
        );
        parts.insert(world.name.to_ascii_lowercase(), (name, path));
    }
    let part = |key: &str| {
        parts
            .get(key)
            .with_context(|| format!("Missing {key} world"))
    };
    let (part1, part1_path) = part("tutorial_part1")?;
    let (part2, part2_path) = part("tutorial_part2")?;
    let setup = std::fs::read(&args[1])?;
    let (targets, targets_end_ms) =
        bri_convert::tutorial::target_schedule(&String::from_utf8_lossy(&setup))?;

    // The add-on's root files by lower-case name, as the shapes name them.
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&args[2])?)?;
    let mut read = |name: &str| -> Result<Option<Vec<u8>>> {
        let Some(index) = (0..archive.len()).find(|&i| {
            archive
                .by_index(i)
                .is_ok_and(|f| f.name().eq_ignore_ascii_case(name))
        }) else {
            return Ok(None);
        };
        let file = archive.by_index(index)?;
        ensure!(file.size() <= MAX_FILE, "{name} is too large");
        let mut data = Vec::new();
        file.take(MAX_FILE).read_to_end(&mut data)?;
        Ok(Some(data))
    };
    std::fs::create_dir_all(output.join("provenance"))?;
    let store = |data: &[u8], extension: &str| -> Result<String> {
        let file = format!("{:x}.{extension}", Sha256::digest(data));
        std::fs::write(output.join(&file), data)?;
        Ok(file)
    };
    let mut shapes = BTreeMap::new();
    let mut materials = std::collections::BTreeSet::new();
    for id in TARGET_SHAPES {
        let name = id.rsplit('/').next().unwrap();
        let data = read(name)?.with_context(|| format!("Map_Tutorial.zip has no {name}"))?;
        let (shape, provenance) = bri_convert::shape::read_dts(&data, id.to_string())?;
        shape.validate()?;
        for warning in &provenance.warnings {
            println!("{name}: {warning}");
        }
        for material in &shape.materials {
            let material = material.name.to_ascii_lowercase();
            // A skinnable material takes every skin `launchTarget` sets.
            if let Some(rest) = material.strip_prefix("base.") {
                for skin in TARGET_SKINS {
                    materials.insert(format!("{skin}.{rest}"));
                }
            } else {
                materials.insert(material);
            }
        }
        let file = store(&serde_json::to_vec(&shape)?, "shape.json")?;
        shapes.insert(id.to_string(), file);
    }
    let mut textures = BTreeMap::new();
    for material in materials {
        let data = read(&material)?
            .with_context(|| format!("Map_Tutorial.zip has no target texture {material}"))?;
        let extension = if material.ends_with(".jpg") {
            "jpg"
        } else {
            "png"
        };
        image::load_from_memory(&data)
            .with_context(|| format!("Decoding target texture {material}"))?;
        textures.insert(material, store(&data, extension)?);
    }

    let index = PackIndex {
        schema_version: PACK_SCHEMA,
        part1: part1.clone(),
        part2: part2.clone(),
        targets,
        targets_end_ms,
        shapes,
        textures,
    };
    index.validate()?;
    std::fs::copy(part1_path, output.join(part1))?;
    std::fs::copy(part2_path, output.join(part2))?;
    let sha = format!("{:x}", Sha256::digest(&setup));
    std::fs::write(
        output.join("provenance").join(format!("{sha}.source.txt")),
        &setup,
    )?;
    std::fs::write(output.join(PACK_INDEX), serde_json::to_vec_pretty(&index)?)?;
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 2,
            "part1": part1,
            "part2": part2,
            "targets": index.targets.len(),
            "targets_end_ms": index.targets_end_ms,
            "target_setup_sha256": sha,
            "target_shapes": index.shapes,
            "target_textures": index.textures,
            "scope": "Tutorial brick layouts, target schedule and target models; lesson rules are native code in bri-sim"
        }))?,
    )?;
    println!(
        "{} targets over {} ms; worlds {part1}, {part2}; {} target shapes, {} textures",
        index.targets.len(),
        index.targets_end_ms,
        index.shapes.len(),
        index.textures.len()
    );
    Ok(())
}
