//! Assemble the native tutorial pack from the bound Map_Tutorial worlds
//! (`import_saves` then `bind_world_events` over the add-on's three saves)
//! and its `targetSetup.txt`.
use anyhow::{Context, Result, ensure};
use bri_content::tutorial::{PACK_INDEX, PACK_SCHEMA, PackIndex};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 3,
        "Usage: tutorial_pack <bound-tutorial-worlds-dir> <targetSetup.txt> <new-output-dir>"
    );
    let output = &args[2];
    ensure!(!output.exists(), "Use a new output directory");
    let mut parts = std::collections::BTreeMap::new();
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
    let index = PackIndex {
        schema_version: PACK_SCHEMA,
        part1: part1.clone(),
        part2: part2.clone(),
        targets,
        targets_end_ms,
    };
    index.validate()?;
    std::fs::create_dir_all(output.join("provenance"))?;
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
            "schema_version": 1,
            "part1": part1,
            "part2": part2,
            "targets": index.targets.len(),
            "targets_end_ms": index.targets_end_ms,
            "target_setup_sha256": sha,
            "scope": "Tutorial brick layouts and target schedule; lesson rules are native code in bri-sim"
        }))?,
    )?;
    println!(
        "{} targets over {} ms; worlds {part1}, {part2}",
        index.targets.len(),
        index.targets_end_ms
    );
    Ok(())
}
