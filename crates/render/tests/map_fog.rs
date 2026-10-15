//! A map Add-On gives its own fog in its `bundle.json` map entry, and the
//! loaded scene carries it (no GPU).
use anyhow::Result;
use std::path::{Path, PathBuf};

/// The sample map Add-On's bundle, copied to `dir` with `fog` on its map.
fn sample_with_fog(dir: &Path, fog: serde_json::Value) -> Result<String> {
    let sample =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples/sample-map/map");
    std::fs::create_dir_all(dir)?;
    for entry in std::fs::read_dir(&sample)? {
        let path = entry?.path();
        std::fs::copy(&path, dir.join(path.file_name().unwrap()))?;
    }
    let file = dir.join("bundle.json");
    let mut bundle: serde_json::Value = serde_json::from_slice(&std::fs::read(&file)?)?;
    // This fixture deliberately has no sky: the shipped sample now has one.
    bundle.as_object_mut().unwrap().remove("environments");
    bundle["maps"][0]["fog"] = fog;
    std::fs::write(&file, serde_json::to_vec(&bundle)?)?;
    Ok(bundle["maps"][0]["id"].as_str().unwrap().to_owned())
}

#[test]
fn a_map_addon_brings_its_own_haze() -> Result<()> {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("map-fog-{}", std::process::id()));
    let id = sample_with_fog(
        &dir,
        serde_json::json!({
            "start": 40.0, "end": 900.0, "color": [0.7, 0.75, 0.8], "height": 25.0, "max": 0.8
        }),
    )?;
    let loaded = bri_render::scene_loader::load_map_bundle(&dir, &id);
    // Height and max may be left out: the standard fog.
    let plain = sample_with_fog(
        &dir,
        serde_json::json!({ "start": 40.0, "end": 900.0, "color": [0.7, 0.75, 0.8] }),
    )
    .and_then(|id| bri_render::scene_loader::load_map_bundle(&dir, &id));
    let bad = sample_with_fog(
        &dir,
        serde_json::json!({ "start": 40.0, "end": 900.0, "color": [0.7, 0.75, 0.8], "max": 0.0 }),
    )
    .and_then(|id| bri_render::scene_loader::load_map_bundle(&dir, &id));
    let _ = std::fs::remove_dir_all(&dir);
    let scene = loaded?.scene;
    let fog = scene.fog;
    assert_eq!(
        (fog.start, fog.end, fog.color),
        (40.0, 900.0, [0.7, 0.75, 0.8])
    );
    assert_eq!((fog.height, fog.max), (25.0, 0.8));
    assert!(fog.haze());
    // No sky of its own: the fog colour is behind the world.
    assert_eq!(scene.clear_color, [0.7, 0.75, 0.8, 1.0]);
    let mut camera = bri_render::scene::Camera::default();
    camera.apply_environment(&scene);
    assert_eq!(camera.haze[..2], [25.0, 0.8]);
    let plain = plain?.scene.fog;
    assert_eq!(
        (plain.height, plain.max),
        (bri_content::environment::FOG_HEIGHT, 1.0)
    );
    assert!(bad.is_err(), "a fog that hides nothing is refused");
    Ok(())
}
