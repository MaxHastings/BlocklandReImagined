//! A map with more lights than the renderer shades keeps its plain lights as
//! overflow, and reads the sun node's enclosed ambient.
use anyhow::Result;
use serde_json::{Value, json};
use std::path::PathBuf;

fn light(name: &str, kind: &str, x: f32) -> Value {
    let mut properties = json!({
        "type": kind, "color": "1 0.5 0.25", "brightness": "2", "inner": "0", "outer": "12",
    });
    if kind == "spot" {
        properties["inner_cone"] = "0".into();
        properties["outer_cone"] = "45".into();
    }
    json!({
        "name": name, "parent": null, "kind": "light", "asset": null,
        "transform": [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., x, 1., 0., 1.],
        "properties": properties,
    })
}

#[test]
fn lights_past_the_shaded_budget_overflow_and_cones_are_left_out() -> Result<()> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("map-light-budget-{}", std::process::id()));
    let mut maps = bri_render::testing::rooms();
    maps.truncate(1);
    bri_render::testing::write_bundle(&dir, &maps)?;
    let bundle: Value = serde_json::from_slice(&std::fs::read(dir.join("bundle.json"))?)?;
    let file = dir.join(bundle["maps"][0]["file"].as_str().unwrap());
    let mut scene: Value = serde_json::from_slice(&std::fs::read(&file)?)?;
    let nodes = scene["nodes"].as_array_mut().unwrap();
    let sun = nodes.iter_mut().find(|n| n["kind"] == "sun").unwrap();
    sun["properties"]["enclosed"] = "0.1 0.2 0.3".into();
    // The cone and 23 plain lights fill the 24 shaded slots; 3 plain lights
    // overflow, and a later cone has no slot and no plain form.
    nodes.push(light("cone-first", "spot", 0.));
    for i in 0..26 {
        nodes.push(light(&format!("lamp{i}"), "omni", 1. + i as f32));
    }
    nodes.push(light("cone-late", "spot", 40.));
    std::fs::write(&file, serde_json::to_vec(&scene)?)?;
    let map = bri_render::scene_loader::load_map_bundle_dynamic(&dir, &maps[0].id)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(map.modern_lights.as_ref().map(Vec::len), Some(24));
    assert_eq!(map.light_spots.len(), 1);
    assert_eq!(map.overflow_lights.len(), 3);
    assert_eq!(map.overflow_lights[0].position[0], 24.);
    assert!(
        map.scene
            .omissions
            .iter()
            .any(|o| o.contains("only plain lights draw past them")),
        "{:?}",
        map.scene.omissions
    );
    assert_eq!(map.scene.enclosed_ambient, Some([0.1, 0.2, 0.3]));
    Ok(())
}
