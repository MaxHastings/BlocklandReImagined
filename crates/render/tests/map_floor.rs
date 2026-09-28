//! The drawn floor moves with the collision floor onto the plate lattice.
use anyhow::Result;

#[test]
#[ignore = "needs generated content (map-bundle-016); set BRI_CONTENT"]
fn bedroom_carpet_is_drawn_on_the_plate_lattice() -> Result<()> {
    let content = std::env::var_os("BRI_CONTENT").map_or_else(
        || std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        std::path::PathBuf::from,
    );
    let map = bri_render::scene_loader::load_map_bundle(
        &content.join("map-bundle-016"),
        "v20/add-ons/map_bedroom/bedroom.mis",
    )?;
    let data = map.scene;
    // Authored at 287.415 over the carpet at 286.312; both rise 0.088.
    assert!((data.spawn[1] - 287.503).abs() < 1e-3, "{:?}", data.spawn);
    let near = |y: f32| {
        data.vertices
            .iter()
            .filter(|v| v.normal[1] > 0.99 && (v.position[1] - y).abs() < 1e-3)
            .count()
    };
    assert!(near(286.4) > 0, "carpet not at 286.4");
    assert_eq!(near(286.312), 0, "carpet left at its authored height");
    Ok(())
}
