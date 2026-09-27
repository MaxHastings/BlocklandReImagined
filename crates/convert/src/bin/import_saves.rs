//! Convert an original saves directory without touching its contents.
use anyhow::{Context, Result, ensure};
use bri_content::brick::Catalog;
use bri_world::{ContentRef, persistence};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        (3..=4).contains(&args.len()),
        "Usage: import_saves <saves-dir> <stock-catalog.json> <new-output-dir> [native-effects.json]"
    );
    let root = PathBuf::from(&args[0]);
    let catalog: Catalog = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let output = PathBuf::from(&args[2]);
    let effects: Option<bri_content::effects::Library> = args
        .get(3)
        .map(|path| -> Result<_> { Ok(serde_json::from_slice(&std::fs::read(path)?)?) })
        .transpose()?;
    ensure!(!output.exists(), "Use a new output directory");
    std::fs::create_dir_all(output.join("provenance"))?;
    let mut reports = Vec::new();
    let mut errors = 0;
    let mut total = 0;
    for entry in walkdir::WalkDir::new(&root).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file()
            || !entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("bls"))
        {
            continue;
        }
        let bytes = std::fs::read(entry.path())?;
        let sha = format!("{:x}", Sha256::digest(&bytes));
        let relative = entry
            .path()
            .strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        std::fs::write(
            output.join("provenance").join(format!("{sha}.source.bls")),
            &bytes,
        )?;
        let map = entry
            .path()
            .parent()
            .and_then(|p| p.file_name())
            .context("Missing map directory")?
            .to_string_lossy()
            .to_lowercase();
        let name = entry.path().file_stem().unwrap().to_string_lossy();
        let map_id = format!("v20/add-ons/map_{map}/{map}.mis");
        match bri_convert::bls::read(&bytes, &catalog, &name, &map_id) {
            Ok(mut world) => {
                let effect_bindings = effects
                    .as_ref()
                    .map(|effects| bri_convert::effect_bindings::bind(&mut world, effects))
                    .transpose()?;
                let file = format!("{sha}.world.json");
                persistence::save_new(&output.join(&file), &world)?;
                let loaded = persistence::load(&output.join(&file))?;
                ensure!(loaded == world, "Native roundtrip changed world state");
                let mut diagnostics = BTreeMap::<String, usize>::new();
                for r in world.bricks.values().flat_map(|b| &b.source_records) {
                    if let Some(d) = &r.diagnostic {
                        *diagnostics.entry(d.clone()).or_default() += 1;
                    }
                }
                let missing = world
                    .bricks
                    .values()
                    .filter(|b| matches!(b.definition, ContentRef::Unresolved { .. }))
                    .count();
                let events: usize = world.bricks.values().map(|b| b.events.len()).sum();
                total += world.bricks.len();
                reports.push(serde_json::json!({"source":relative,"sha256":sha,"file":file,"bricks":world.bricks.len(),"missing_brick_definitions":missing,"adapted_events":events,"effect_bindings":effect_bindings,"diagnostics":diagnostics,"native_roundtrip":"passed"}));
            }
            Err(e) => {
                errors += 1;
                reports.push(
                    serde_json::json!({"source":relative,"sha256":sha,"error":format!("{e:#}")}),
                );
            }
        }
    }
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":1,"saves":reports,"bricks":total,"errors":errors,"scope":"offline world conversion and exact native state roundtrip; unresolved content and unsupported behaviors remain diagnostic, no gameplay equivalence claim"}),
        )?,
    )?;
    println!(
        "{} saves, {total} bricks, {errors} parse failures",
        reports.len()
    );
    ensure!(errors == 0, "Some saves failed; see report.json");
    Ok(())
}
