//! Offline native map light recovery. No GPU or original installation access.
use anyhow::{Context, Result, ensure};
use bri_render::{
    lighting_parameters::{FILE, Parameters, SourceLight, fingerprint},
    map_lighting::Bake,
    scene_loader::load_map_bundle,
};
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let force = args.iter().any(|a| a == "--force");
    let positional: Vec<_> = args
        .iter()
        .filter(|a| *a != "--check" && *a != "--force")
        .collect();
    ensure!(
        !positional.is_empty() && positional.len() <= 2 && !(check && force),
        "Usage: prepare_lighting <native-map-bundle> [output] [--check | --force]"
    );
    let root = PathBuf::from(positional[0]);
    let output = positional
        .get(1)
        .map_or_else(|| root.join(FILE), PathBuf::from);
    ensure!(
        !check || output == root.join(FILE),
        "--check validates the bundle's own sidecar"
    );
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("bundle.json"))?)?;
    let records = bundle["maps"].as_array().context("Bundle maps missing")?;
    let prepared = Parameters::read(&root).and_then(|p| {
        ensure!(
            p.maps.len() == records.len(),
            "Prepared map count differs from bundle"
        );
        for record in records {
            p.lights(record["id"].as_str().context("Map ID missing")?)?;
        }
        Ok(p)
    });
    if check || (!force && output == root.join(FILE) && prepared.is_ok()) {
        let prepared = prepared?;
        println!(
            "Validated {} prepared maps: {}",
            prepared.maps.len(),
            output.display()
        );
        return Ok(());
    }
    let mut maps = BTreeMap::new();
    for record in records {
        let id = record["id"].as_str().context("Map ID missing")?;
        let start = Instant::now();
        let scene = load_map_bundle(&root, id)?.scene;
        let lights: Vec<SourceLight> = Bake::new(&scene).map_or_else(Vec::new, |source| {
            source
                .recover_lights()
                .into_iter()
                .map(Into::into)
                .collect()
        });
        println!(
            "{id}: {} source lights, {:.3}s",
            lights.len(),
            start.elapsed().as_secs_f64()
        );
        maps.insert(id.to_owned(), lights);
    }
    let data = Parameters {
        schema_version: 1,
        bundle_sha256: fingerprint(&root)?,
        maps,
    };
    std::fs::write(&output, serde_json::to_vec_pretty(&data)?)?;
    println!("Prepared {} maps: {}", data.maps.len(), output.display());
    Ok(())
}
