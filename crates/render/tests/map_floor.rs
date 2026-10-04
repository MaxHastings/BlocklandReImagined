//! The drawn floor moves with the collision floor onto the plate lattice.
use anyhow::Result;
use bri_content::scene::PLATE_HEIGHT;
use std::path::{Path, PathBuf};

/// `map`'s floor, authored at `floor` off the plate lattice, is drawn on
/// the nearest plate plane, and its spawn (authored at `spawn`) rises or
/// sinks with it.
fn floor_is_drawn_on_the_plate_lattice(
    bundle: &Path,
    map: &str,
    floor: f32,
    spawn: f32,
) -> Result<()> {
    let lifted = (floor / PLATE_HEIGHT).round() * PLATE_HEIGHT;
    let lift = lifted - floor;
    assert!(
        lift.abs() > 0.01,
        "the fixture floor must be off the lattice"
    );
    let data = bri_render::scene_loader::load_map_bundle(bundle, map)?.scene;
    assert!(
        (data.spawn[1] - (spawn + lift)).abs() < 1e-3,
        "{:?}",
        data.spawn
    );
    let near = |y: f32| {
        data.vertices
            .iter()
            .filter(|v| v.normal[1] > 0.99 && (v.position[1] - y).abs() < 1e-3)
            .count()
    };
    assert!(near(lifted) > 0, "floor not at {lifted}");
    assert_eq!(near(floor), 0, "floor left at its authored height");
    Ok(())
}

#[test]
fn fixture_floors_are_drawn_on_the_plate_lattice() -> Result<()> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("map-floor-{}", std::process::id()));
    let maps = bri_render::testing::rooms();
    bri_render::testing::write_bundle(&dir, &maps)?;
    let result = maps.iter().try_for_each(|m| {
        floor_is_drawn_on_the_plate_lattice(&dir, &m.id, m.floor_height(), m.spawn_position().y)
    });
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
#[ignore = "requires generated v20 content"]
fn bedroom_carpet_is_drawn_on_the_plate_lattice() -> Result<()> {
    let content = std::env::var_os("BRI_CONTENT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    );
    // Authored at 287.415 over the carpet at 286.312; both rise 0.088.
    floor_is_drawn_on_the_plate_lattice(
        &bri_package::testing::pack_dir(&content, "map_bundle"),
        "v20/add-ons/map_bedroom/bedroom.mis",
        286.312,
        287.415,
    )
}
