//! Offline native map light recovery. No GPU or original installation access.
use anyhow::{Context, Result, ensure};
use bri_render::{
    light_volume::{BAKE_MAX_CELLS, BAKE_MIN_CELL, Baker},
    lighting_parameters::{
        FILE, Parameters, SourceLight, fingerprint, read_volume, volume_file, write_volume,
    },
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
    let prepared_lights = !force && output == root.join(FILE) && prepared.is_ok();
    // Each map's room light: what bodies standing in it take in Dynamic.
    let volumes = |root: &PathBuf, bake: bool| -> Result<usize> {
        let mut missing = 0;
        for record in records {
            let id = record["id"].as_str().context("Map ID missing")?;
            if !force && read_volume(root, id).is_some() {
                continue;
            }
            if !bake {
                println!(
                    "{id}: no current light volume ({}); run prepare_lighting without --check",
                    volume_file(id).unwrap_or_default()
                );
                missing += 1;
                continue;
            }
            let start = Instant::now();
            let scene = load_map_bundle(root, id)?.scene;
            if let Some(baker) = Baker::new(&scene) {
                let volume = baker.bake(BAKE_MIN_CELL, BAKE_MAX_CELLS);
                write_volume(root, id, &volume)?;
                println!(
                    "{id}: light volume {:?} cells, {:.3}s",
                    volume.dims,
                    start.elapsed().as_secs_f64()
                );
            } else {
                println!("{id}: no lightmapped geometry, so no light volume");
            }
        }
        Ok(missing)
    };
    if check || prepared_lights {
        let prepared = prepared?;
        // A bundle prepared before room lights existed is not invalid: --check
        // only says what is missing.
        let missing = volumes(&root, !check)?;
        println!(
            "Validated {} prepared maps: {}{}",
            prepared.maps.len(),
            output.display(),
            if missing > 0 {
                format!(" ({missing} without a light volume)")
            } else {
                String::new()
            }
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
    data.write_atomic(&root, &output)?;
    if output == root.join(FILE) {
        volumes(&root, true)?;
    }
    println!("Prepared {} maps: {}", data.maps.len(), output.display());
    Ok(())
}
