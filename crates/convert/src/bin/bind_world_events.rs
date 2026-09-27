//! Type imported worlds' wrench events against the native event catalog and
//! bind vehicle/music spawn bricks, writing a new world pass. Accepts schema 1
//! worlds (the retired minimal event model) and current worlds.
use anyhow::{Context, Result, ensure};
use bri_convert::events::{Aliases, bind};
use bri_world::persistence;
use std::path::PathBuf;

fn json(path: &PathBuf) -> Result<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(path).with_context(|| path.display().to_string())?)
        .with_context(|| path.display().to_string())
}

fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 7,
        "Usage: bind_world_events <worlds-dir> <events-catalog.json> <audio-manifest.json> <weapons.json> <vehicles.json> <effects.json> <new-output-dir>"
    );
    let catalog = bri_events::Catalog::load(&args[1])?;
    let effects: bri_content::effects::Library = serde_json::from_value(json(&args[5])?)?;
    let aliases = Aliases::from_packs(
        &json(&args[2])?,
        &json(&args[3])?,
        &json(&args[4])?,
        &effects,
    )?;
    let output = &args[6];
    ensure!(!output.exists(), "Use a new output directory");
    std::fs::create_dir_all(output)?;
    let mut entries: Vec<_> = std::fs::read_dir(&args[0])?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .map(|e| e.path())
        .collect();
    entries.sort();
    let mut reports = Vec::new();
    let (mut rows, mut runnable) = (0, 0);
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            // Original BLS provenance copies travel with the pass.
            let target = output.join(&name);
            std::fs::create_dir_all(&target)?;
            for file in std::fs::read_dir(&path)? {
                let file = file?;
                std::fs::copy(file.path(), target.join(file.file_name()))?;
            }
            continue;
        }
        if !name.ends_with(".world.json") {
            continue;
        }
        let mut value = json(&path)?;
        if value["schema_version"] == 1 {
            let object = value.as_object_mut().context("World is not an object")?;
            object.remove("next_event_order");
            object.remove("pending");
            object.insert("schema_version".into(), bri_world::WORLD_SCHEMA.into());
            for brick in object
                .get_mut("bricks")
                .and_then(|b| b.as_object_mut())
                .context("World has no bricks")?
                .values_mut()
            {
                brick["events"] = serde_json::json!([]);
            }
        }
        let mut world: bri_world::World = serde_json::from_value(value)?;
        let report = bind(&mut world, &catalog, &aliases)?;
        rows += report.rows;
        runnable += report.runnable;
        persistence::save_new(&output.join(&name), &world)?;
        ensure!(
            persistence::load(&output.join(&name))? == world,
            "Native roundtrip changed {name}"
        );
        reports.push(serde_json::json!({"file": name, "name": world.name, "events": report}));
    }
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "event_catalog": catalog.fingerprint(),
            "rows": rows,
            "runnable": runnable,
            "worlds": reports,
        }))?,
    )?;
    println!(
        "{} worlds, {rows} event rows, {runnable} runnable",
        reports.len()
    );
    Ok(())
}
