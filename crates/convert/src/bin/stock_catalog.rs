use anyhow::{Context, Result, ensure};
use bri_content::brick::Brick;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() >= 3,
        "Usage: stock_catalog <recovered-stock-script> <converted-content-dir> <new-output-dir> [brick-addon.zip ...]"
    );
    let source = std::fs::read(&args[0])?;
    let mut catalog = bri_convert::catalog::read_stock(std::str::from_utf8(&source)?)?;
    let mut addon_sources = Vec::new();
    for addon in args.iter().skip(3) {
        use std::io::Read;
        let path = PathBuf::from(addon);
        let name = path
            .file_stem()
            .context("Missing add-on name")?
            .to_str()
            .context("Non-Unicode add-on name")?;
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&path)?)?;
        let mut bytes = Vec::new();
        archive
            .by_name("server.cs")?
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16 * 1024 * 1024, "Oversized add-on script");
        let added = bri_convert::catalog::read_at(
            std::str::from_utf8(&bytes)?,
            &format!("Add-Ons/{name}"),
        )?;
        addon_sources.push(serde_json::json!({"addon":name,"script_sha256":format!("{:x}",Sha256::digest(&bytes)),"declarations":added.bricks.len(),"scope":"constant declarations only; script callbacks require native adaptation"}));
        for brick in added.bricks {
            ensure!(
                !catalog.bricks.iter().any(|b| b.id == brick.id),
                "Ambiguous add-on catalog entry {}",
                brick.display_name
            );
            catalog.bricks.push(brick);
        }
    }
    let content = PathBuf::from(&args[1]);
    let mut save_names = std::collections::BTreeMap::new();
    let mut name_overrides = Vec::new();
    for brick in &catalog.bricks {
        if let Some(previous) =
            save_names.insert(brick.display_name.to_lowercase(), brick.id.clone())
        {
            name_overrides.push(serde_json::json!({"name":brick.display_name,"previous":previous,"resolved":brick.id,"rule":"last declaration wins, as in v20's save-name table"}));
        }
    }
    let hidden: Vec<_> = catalog
        .bricks
        .iter()
        .filter(|b| !b.selectable())
        .map(|b| &b.id)
        .collect();
    let output = PathBuf::from(&args[2]);
    ensure!(!output.exists(), "Use a new output directory");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(content.join("manifest.json"))?)?;
    let records = manifest["records"]
        .as_array()
        .context("Missing conversion records")?;
    let mut resolved = Vec::new();
    let mut missing = Vec::new();
    let mut collision_pending = Vec::new();
    let mut collision_bodies = Vec::new();
    for entry in &catalog.bricks {
        let expected = entry
            .mesh_id
            .strip_prefix("v20/")
            .context("Invalid catalog ID")?;
        let matches: Vec<_> = records
            .iter()
            .filter(|r| {
                r["virtual_path"]
                    .as_str()
                    .is_some_and(|v| v.eq_ignore_ascii_case(expected))
            })
            .collect();
        if matches.len() != 1 || matches[0]["output"].is_null() {
            missing.push(entry.mesh_id.clone());
            continue;
        }
        let file = matches[0]["output"]
            .as_str()
            .context("Invalid output path")?;
        ensure!(
            !file.contains(['/', '\\', ':']),
            "Invalid converted filename"
        );
        let brick: Brick = serde_json::from_slice(&std::fs::read(content.join(file))?)?;
        brick.validate()?;
        if brick.needs_external_collision || entry.collision_source.is_some() {
            let native = entry.collision_source.as_ref().and_then(|source| {
                records.iter().find(|r| {
                    r["virtual_path"]
                        .as_str()
                        .is_some_and(|p| p.eq_ignore_ascii_case(source))
                })
            });
            if let Some(file) = native.and_then(|r| r["output"].as_str()) {
                ensure!(
                    !file.contains(['/', '\\', ':']),
                    "Invalid collision output path"
                );
                let shape = serde_json::from_slice(&std::fs::read(content.join(file))?)?;
                collision_bodies.push(
                    bri_convert::collision::bake(entry.id.clone(), &brick, Some(&shape))
                        .with_context(|| format!("Collision for {}", entry.id))?,
                );
            } else {
                collision_pending
                    .push(serde_json::json!({"brick":entry.id,"source":entry.collision_source}));
            }
        } else {
            collision_bodies.push(bri_convert::collision::bake(
                entry.id.clone(),
                &brick,
                None,
            )?);
        }
        resolved.push(
            serde_json::json!({"id":entry.id,"display_name":entry.display_name,"native_mesh":file}),
        );
    }
    std::fs::create_dir(&output)?;
    std::fs::write(
        output.join("stock-catalog.json"),
        serde_json::to_vec_pretty(&catalog)?,
    )?;
    let collision_count = collision_bodies.len();
    let library = bri_content::collision::CollisionLibrary {
        schema_version: 1,
        bodies: collision_bodies,
    };
    std::fs::write(
        output.join("native-collisions.json"),
        serde_json::to_vec_pretty(&library)?,
    )?;
    let report = serde_json::json!({"script_sha256":format!("{:x}",Sha256::digest(&source)),"addon_sources":addon_sources,"brick_count":catalog.bricks.len(),"hidden_definitions":hidden,"save_name_overrides":name_overrides,"resolved_meshes":resolved,"missing_meshes":missing,"external_collision_pending":collision_pending,"native_collision_bodies":collision_count,"scope":"stock catalog, native mesh and collision recipes; icons, prints and scripted gameplay callbacks require native adaptation"});
    std::fs::write(
        output.join("catalog-audit.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Stock catalog: {} entries, {} mesh references resolved, {} missing; {} collision shapes pending",
        catalog.bricks.len(),
        resolved.len(),
        missing.len(),
        collision_pending.len()
    );
    ensure!(missing.is_empty(), "Missing native catalog meshes");
    ensure!(
        collision_pending.is_empty(),
        "Missing native collision shapes"
    );
    Ok(())
}
