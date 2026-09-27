//! Derive a new numbered map bundle with converted terrain placements from an
//! existing bundle. Reads only generated native content (retained mission
//! fields, terrain assets and the packaged texture table); never the original
//! installation. The source bundle is left unchanged; the output must be new.
use anyhow::{Context, Result, ensure};
use bri_content::{
    Terrain,
    scene::{Kind, Scene},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

fn safe(name: &str) -> Result<&str> {
    ensure!(
        !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..",
        "Unsafe bundle filename {name}"
    );
    Ok(name)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 2,
        "Usage: terrain_bundle <existing-map-bundle> <new-map-bundle>"
    );
    let source = PathBuf::from(&args[0]).canonicalize()?;
    let output = PathBuf::from(&args[1]);
    ensure!(
        !output.exists(),
        "Output bundle already exists; use a fresh numbered directory"
    );
    let bundle_bytes = std::fs::read(source.join("bundle.json"))?;
    let mut bundle: serde_json::Value = serde_json::from_slice(&bundle_bytes)?;
    ensure!(
        bundle["schema_version"].as_u64() == Some(1),
        "Unsupported bundle schema"
    );
    let textures: BTreeMap<String, String> =
        serde_json::from_value(bundle["textures"].clone()).context("Bundle texture table")?;
    let mut terrains = BTreeMap::new();
    let mut report = Vec::new();
    for map in bundle["maps"].as_array().context("Bundle maps")? {
        let scene: Scene = serde_json::from_slice(&std::fs::read(
            source.join(safe(map["file"].as_str().context("Scene file")?)?),
        )?)?;
        let mut instances = Vec::new();
        for (index, node) in scene.nodes.iter().enumerate() {
            if !matches!(node.kind, Kind::Terrain) {
                continue;
            }
            let id = node.asset.as_ref().context("Terrain lacks asset")?;
            let file = safe(
                bundle["assets"][id]
                    .as_str()
                    .context("Terrain asset missing")?,
            )?;
            let terrain: Terrain = serde_json::from_slice(&std::fs::read(source.join(file))?)?;
            terrain.validate()?;
            let instance =
                bri_convert::terrain::instance(&scene, index, terrain.side, &mut |path| {
                    bri_convert::terrain::resolve_packaged(&textures, path)
                })?;
            report.push(serde_json::json!({
                "map": scene.id, "node": index, "terrain": id, "square_size": instance.square_size,
                "repeat": instance.repeat, "repeat_source": instance.repeat_source,
                "empty_squares": instance.empty_runs.iter().map(|r| r[1]).sum::<u32>(),
                "detail": instance.detail.as_ref().map(|t| &t.source),
                "bump": instance.bump.texture.as_ref().map(|t| &t.source),
                "diagnostics": instance.diagnostics,
            }));
            instances.push(instance);
        }
        if !instances.is_empty() {
            terrains.insert(scene.id.clone(), instances);
        }
    }
    std::fs::create_dir_all(&output)?;
    let mut copied = 0usize;
    for entry in std::fs::read_dir(&source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("Non-UTF-8 bundle filename")?;
        if name == "bundle.json" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &output.join(name))?;
        } else {
            std::fs::copy(entry.path(), output.join(name))?;
        }
        copied += 1;
    }
    bundle["terrains"] = serde_json::to_value(&terrains)?;
    bundle["terrain_derivation"] = serde_json::json!({
        "tool": "terrain_bundle", "converter": env!("CARGO_PKG_VERSION"),
        "source_bundle": source.file_name().and_then(|n| n.to_str()),
        "source_bundle_json_sha256": format!("{:x}", Sha256::digest(&bundle_bytes)),
        "instances": report,
    });
    std::fs::write(
        output.join("bundle.json"),
        serde_json::to_vec_pretty(&bundle)?,
    )?;
    println!(
        "{} terrain placements across {} maps; {copied} entries copied",
        terrains.values().map(Vec::len).sum::<usize>(),
        terrains.len()
    );
    Ok(())
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
