//! How the client draws a package's item models: every manifest names the
//! model's own `.glb` (its real checksum), which the game lowers and binds —
//! its embedded bitmaps where it carries them, listed files otherwise. A
//! manifest that still ships a legacy `.shape.json` is a fault line whose
//! model draws nothing until its content converts to glTF; there is no JSON
//! fallback to draw from. A corrupt GLB (whose checksum the manifest
//! carries) is an Add-On fault that drops the model — the load itself still
//! succeeds (docs/progress record).

use anyhow::Result;
use bri_client::items::ItemAssets;
use bri_content::testing::{material, plain, png, rigid_shape, write_file};
use glam::Mat4;
use std::collections::BTreeMap;

/// A manifest that still names a legacy `.shape.json` is a fault line: the
/// file is never read and the model draws nothing.
#[test]
fn a_legacy_shape_json_model_is_a_fault_that_draws_nothing() -> Result<()> {
    use bri_client::testing::items::GUN_MODEL;
    use bri_weapons::testing::GUN_ITEM;
    let packs = bri_client::testing::items::write_packs()?;
    // `models/<stem>.glb` is the gun's model now; point its entry at a
    // legacy file written beside it.
    let stem = GUN_MODEL.trim_end_matches(".dts").replace('/', "_");
    std::fs::write(
        packs.presentation.join(format!("models/{stem}.shape.json")),
        b"legacy model bytes",
    )?;
    let pres_path = packs.presentation.join("presentation.json");
    let mut pres: serde_json::Value = serde_json::from_slice(&std::fs::read(&pres_path)?)?;
    pres["models"][GUN_MODEL]["file"] = serde_json::json!(format!("models/{stem}.shape.json"));
    std::fs::write(&pres_path, serde_json::to_vec(&pres)?)?;

    let assets = ItemAssets::load(&packs.presentation, &packs.weapons, &packs.root)?;
    assert!(
        assets
            .faults
            .iter()
            .any(|f| f.contains("legacy model file") && f.contains(".shape.json")),
        "the legacy entry is a fault line: {:?}",
        assets.faults
    );
    let scene = assets.item_scene(GUN_ITEM, Mat4::IDENTITY)?;
    assert!(scene.vertices.is_empty(), "no .glb means no model");
    Ok(())
}

/// An Add-On model whose `.glb` does not lower is a fault line that draws
/// nothing — there is no JSON beside it to fall back to; a valid sibling
/// model named by the same manifest draws without a fault.
#[test]
fn broken_glb_is_a_fault_that_draws_nothing() -> Result<()> {
    let packs = bri_client::testing::items::write_packs()?;
    let dir = tempfile::tempdir()?;

    // The part's own (empty) weapons pack, its presentation and physics.
    let weapons_bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version": bri_weapons::SCHEMA,
        "id": "glbtest",
    }))?;
    let weapons_sha = write_file(dir.path(), "weapons.json", &weapons_bytes)?;

    // Model x: garbage bytes the manifest names as its own .glb.
    let x_sha = write_file(dir.path(), "models/x.glb", b"not a glb")?;

    // Model y: one material, a valid GLB named by the manifest itself.
    let shape_y = rigid_shape(
        "glbtest/model/y.dts",
        &[("root", None, [0.0; 3])],
        &[(0, [0.0, 0.1, 0.0], [0.05, 0.2, 0.05], plain(0))],
        vec![material("c", "opaque")],
    );
    let mut notes = Vec::new();
    let (glb_y, _embedded) = bri_gltf::export_shape(&shape_y, &BTreeMap::new(), &mut notes)?;
    let y_sha = write_file(dir.path(), "models/y.glb", &glb_y)?;

    // A real texture for y's material.
    let tex_c = png(8, 8, |x, _| [(x * 17 % 256) as u8, 30, 120, 255])?;
    let sha_c = write_file(dir.path(), "textures/c.png", &tex_c)?;

    let physics_bytes = br#"{"schema_version":1,"items":{}}"#;
    let physics_sha = write_file(dir.path(), "item-physics.json", physics_bytes)?;
    let presentation = serde_json::json!({
        "schema_version": 2,
        "id": "glbtest",
        "weapons_sha256": weapons_sha,
        "item_physics_sha256": physics_sha,
        "models": {
            "glbtest/model/x.dts": {"file":"models/x.glb","sha256":x_sha,
                "source":"made up","source_sha256":"0".repeat(64),
                "textures":[],
                "bounds_min":[-1.0,-1.0,-1.0],"bounds_max":[1.0,1.0,1.0]},
            "glbtest/model/y.dts": {"file":"models/y.glb","sha256":y_sha,
                "source":"made up","source_sha256":"0".repeat(64),
                "textures":["glbtest/tex_c.png"],
                "bounds_min":[-1.0,-1.0,-1.0],"bounds_max":[1.0,1.0,1.0]},
        },
        "textures": {
            "glbtest/tex_c.png":{"file":"textures/c.png","sha256":sha_c,"width":8,"height":8,"source":"made up"},
        },
        "items": {}, "images": {}, "projectiles": {}, "diagnostics": [],
    });
    std::fs::write(
        dir.path().join("presentation.json"),
        serde_json::to_vec(&presentation)?,
    )?;

    let assets = ItemAssets::load_with(
        &packs.presentation,
        &packs.weapons,
        &packs.root,
        &[("glbtest".into(), dir.path().to_path_buf())],
    )?;
    assert!(
        assets
            .faults
            .iter()
            .any(|f| f.contains("GLB unusable") && f.contains("x.glb")),
        "the broken GLB is a fault line: {:?}",
        assets.faults
    );
    assert!(
        !assets.faults.iter().any(|f| f.contains("y.glb")),
        "the valid GLB loads without a fault: {:?}",
        assets.faults
    );
    // x draws nothing (there is no JSON to fall back to); y draws.
    assert!(
        !assets
            .presentation
            .models
            .contains_key("glbtest/model/x.dts"),
        "x was dropped from the presentation"
    );
    let scene_y = assets.mesh("glbtest/model/y.dts", [1.0; 4])?;
    assert!(!scene_y.data.vertices.is_empty(), "y drew from its .glb");
    Ok(())
}

/// New imports name the model's own `.glb` in the manifest with its real
/// checksum and ship no `.shape.json`: a valid GLB draws without a fault,
/// and corrupt bytes (whose checksum the manifest carries) are an Add-On
/// fault that drops the model — the load itself still succeeds.
#[test]
fn a_manifest_named_glb_loads_and_a_corrupt_one_is_a_fault() -> Result<()> {
    let packs = bri_client::testing::items::write_packs()?;
    let dir = tempfile::tempdir()?;

    // The part's own (empty) weapons pack.
    let weapons_bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version": bri_weapons::SCHEMA,
        "id": "glbtest",
    }))?;
    let weapons_sha = write_file(dir.path(), "weapons.json", &weapons_bytes)?;

    // Model z: a valid .glb named by the manifest itself.
    let shape_z = rigid_shape(
        "glbtest/model/z.dts",
        &[("root", None, [0.0; 3])],
        &[(0, [0.0, 0.1, 0.0], [0.05, 0.2, 0.05], plain(0))],
        vec![material("c", "opaque")],
    );
    let mut notes = Vec::new();
    let (glb_z, _embedded) = bri_gltf::export_shape(&shape_z, &BTreeMap::new(), &mut notes)?;
    let z_sha = write_file(dir.path(), "models/z.glb", &glb_z)?;

    // Model w: corrupt bytes whose checksum the manifest carries anyway.
    let w_bytes = b"not a glb";
    let w_sha = write_file(dir.path(), "models/w.glb", w_bytes)?;

    // A real texture for z's material.
    let tex_c = png(8, 8, |x, _| [(x * 17 % 256) as u8, 30, 120, 255])?;
    let sha_c = write_file(dir.path(), "textures/c.png", &tex_c)?;

    let physics_bytes = br#"{"schema_version":1,"items":{}}"#;
    let physics_sha = write_file(dir.path(), "item-physics.json", physics_bytes)?;
    let presentation = serde_json::json!({
        "schema_version": 2,
        "id": "glbtest",
        "weapons_sha256": weapons_sha,
        "item_physics_sha256": physics_sha,
        "models": {
            "glbtest/model/z.dts": {"file":"models/z.glb","sha256":z_sha,
                "source":"made up","source_sha256":"0".repeat(64),
                "textures":["glbtest/tex_c.png"],
                "bounds_min":[-1.0,-1.0,-1.0],"bounds_max":[1.0,1.0,1.0]},
            "glbtest/model/w.dts": {"file":"models/w.glb","sha256":w_sha,
                "source":"made up","source_sha256":"0".repeat(64),
                "textures":[],
                "bounds_min":[-1.0,-1.0,-1.0],"bounds_max":[1.0,1.0,1.0]},
        },
        "textures": {
            "glbtest/tex_c.png":{"file":"textures/c.png","sha256":sha_c,"width":8,"height":8,"source":"made up"},
        },
        "items": {}, "images": {}, "projectiles": {}, "diagnostics": [],
    });
    std::fs::write(
        dir.path().join("presentation.json"),
        serde_json::to_vec(&presentation)?,
    )?;

    let assets = ItemAssets::load_with(
        &packs.presentation,
        &packs.weapons,
        &packs.root,
        &[("glbtest".into(), dir.path().to_path_buf())],
    )?;
    assert!(
        !assets.faults.iter().any(|f| f.contains("z.glb")),
        "the valid GLB loads without a fault: {:?}",
        assets.faults
    );
    assert!(
        assets
            .faults
            .iter()
            .any(|f| f.contains("GLB unusable") && f.contains("w.glb")),
        "the corrupt GLB is a fault line: {:?}",
        assets.faults
    );
    let scene_z = assets.mesh("glbtest/model/z.dts", [1.0; 4])?;
    assert!(
        !scene_z.data.vertices.is_empty(),
        "z drew from its own .glb"
    );
    Ok(())
}
