//! Compare the offline lighting bake against a map bundle whose lighting came
//! from the classic engine's own mission caches (for example map-bundle-015,
//! built from a secondary install's `.ml` files). Offscreen, CPU only.
use anyhow::{Context, Result, ensure};
use bri_content::{scene::Scene, terrain_field::TerrainInstance};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

fn rgb(path: &std::path::Path) -> Result<image::RgbImage> {
    Ok(image::open(path)
        .with_context(|| format!("Reading {}", path.display()))?
        .to_rgb8())
}

/// Mean and 95th percentile absolute channel difference, and the share of
/// channels more than 16 levels apart.
fn difference(a: &image::RgbImage, b: &image::RgbImage) -> Result<Value> {
    ensure!(a.dimensions() == b.dimensions(), "Lightmap sizes differ");
    let mut diffs: Vec<u8> = a
        .as_raw()
        .iter()
        .zip(b.as_raw())
        .map(|(x, y)| x.abs_diff(*y))
        .collect();
    let mean = diffs.iter().map(|&d| f64::from(d)).sum::<f64>() / diffs.len() as f64;
    let far = diffs.iter().filter(|&&d| d > 16).count() as f64 / diffs.len() as f64;
    diffs.sort_unstable();
    let p95 = diffs[diffs.len() * 95 / 100];
    let signed = a
        .as_raw()
        .iter()
        .zip(b.as_raw())
        .map(|(x, y)| f64::from(*x) - f64::from(*y))
        .sum::<f64>()
        / diffs.len() as f64;
    Ok(
        json!({"mean_abs":mean,"mean_signed_baked_minus_cache":signed,"p95_abs":p95,"share_over_16":far}),
    )
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 4,
        "Usage: lighting_compare <v20-root> <converted-dir> <cache-lit-bundle-dir> <new-scratch-dir>"
    );
    let root = PathBuf::from(&args[0]).canonicalize()?;
    let content = PathBuf::from(&args[1]).canonicalize()?;
    let bundle_dir = PathBuf::from(&args[2]).canonicalize()?;
    let scratch = PathBuf::from(&args[3]);
    ensure!(!scratch.exists(), "Use a new scratch directory");
    std::fs::create_dir_all(&scratch)?;
    let manifest: Value = serde_json::from_slice(&std::fs::read(content.join("manifest.json"))?)?;
    let records = manifest["records"].as_array().context("Missing records")?;
    let bundle: Value = serde_json::from_slice(&std::fs::read(bundle_dir.join("bundle.json"))?)?;
    let assets: BTreeMap<String, String> = serde_json::from_value(bundle["assets"].clone())?;
    let source = |id: &str| -> Result<String> {
        let file = assets.get(id).context("Unknown asset")?;
        records
            .iter()
            .find(|r| r["output"].as_str() == Some(file))
            .and_then(|r| r["virtual_path"].as_str())
            .map(str::to_owned)
            .context("Asset has no conversion record")
    };
    let mut report = BTreeMap::new();
    for map in bundle["maps"].as_array().context("Missing maps")? {
        let id = map["id"].as_str().context("Map id")?;
        let file = map["file"].as_str().context("Map file")?;
        let scene: Scene = serde_json::from_slice(&std::fs::read(bundle_dir.join(file))?)?;
        let terrains: Vec<TerrainInstance> =
            serde_json::from_value(bundle["terrains"][id].clone()).unwrap_or_default();
        let out = scratch.join(id.replace('/', "_"));
        std::fs::create_dir_all(&out)?;
        let started = std::time::Instant::now();
        let baked = bri_convert::scene_lighting::bake_scene(
            &root,
            &scene,
            &assets,
            &bundle_dir,
            &source,
            &terrains,
            &out,
        )
        .with_context(|| format!("Baking {id}"))?;
        let seconds = started.elapsed().as_secs_f64();
        let cached = &bundle["lighting"][id];
        let mut entries = Vec::new();
        for (kind, key) in [("terrain", "node"), ("interiors", "slot")] {
            for mine in baked[kind].as_array().into_iter().flatten() {
                let theirs = cached[kind].as_array().into_iter().flatten().find(|c| {
                    c["node"] == mine["node"]
                        && (kind == "terrain"
                            || (c["detail"] == mine["detail"] && c[key] == mine[key]))
                });
                let Some(theirs) = theirs else {
                    entries.push(json!({"kind":kind,"node":mine["node"],"detail":mine["detail"],"slot":mine["slot"],"cache":null}));
                    continue;
                };
                let a = rgb(&out.join(mine["file"].as_str().unwrap()))?;
                let b = rgb(&bundle_dir.join(theirs["file"].as_str().unwrap()))?;
                entries.push(json!({"kind":kind,"node":mine["node"],"detail":mine["detail"],"slot":mine["slot"],
                    "baked":mine["file"],"cache":theirs["file"],"difference":difference(&a, &b)?}));
            }
        }
        println!("{id}: {} lightmaps baked in {seconds:.1}s", entries.len());
        for e in &entries {
            if !e["difference"].is_null() && e["kind"] == "terrain" {
                println!("  terrain {}", e["difference"]);
            }
        }
        let compared: Vec<_> = entries
            .iter()
            .filter(|e| !e["difference"].is_null())
            .collect();
        if !compared.is_empty() {
            let mean = compared
                .iter()
                .map(|e| e["difference"]["mean_abs"].as_f64().unwrap())
                .sum::<f64>()
                / compared.len() as f64;
            println!(
                "  {} cached lightmaps compared, mean |diff| {mean:.2}",
                compared.len()
            );
        }
        report.insert(id.to_string(), json!({"seconds":seconds,"entries":entries}));
    }
    std::fs::write(
        scratch.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}
