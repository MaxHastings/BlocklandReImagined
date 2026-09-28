//! The live sun uses each mission's `azimuth` and `elevation`, as v20 and the
//! lighting bake do, never its stale `direction` field.
use anyhow::Result;

#[test]
#[ignore = "needs generated content (map-bundle-017); set BRI_CONTENT"]
fn every_stock_map_is_lit_from_its_authored_sun_angles() -> Result<()> {
    let content = std::env::var_os("BRI_CONTENT").map_or_else(
        || std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        std::path::PathBuf::from,
    );
    // (map, azimuth, elevation) as authored in the vanilla reference.
    let suns = [
        ("map_bedroom/bedroom", 250.0, 35.0),
        ("map_bedroomdark/bedroomdark", 250.0, 90.0),
        ("map_construct/construct", 275.0, 58.0),
        ("map_destruct/destruct", 290.0, 90.0),
        ("map_halloween_slate/halloweenslate", 135.0, 20.0),
        ("map_kitchen/kitchen", 290.0, 35.0),
        ("map_kitchendark/kitchendark", 290.0, 90.0),
        ("map_skylands/skylands", 238.0, 21.0),
        ("map_slate/slate", 275.0, 58.0),
        ("map_slate_desert/slatedesert", 153.0, 45.0),
        ("map_slate_sea_revised/slatesearevised", 315.0, 45.0),
        ("map_slate_storm_revised/slatestormrevised", 100.0, 48.0),
        ("map_slopes/slopes", 0.0, 35.0),
        ("map_tutorial/tutorial", 275.0, 57.0),
    ];
    for (map, azimuth, elevation) in suns {
        let data = bri_render::scene_loader::load_map_bundle(
            &content.join("map-bundle-017"),
            &format!("v20/add-ons/{map}.mis"),
        )?
        .scene;
        let (yaw, pitch): (f32, f32) = (f32::to_radians(azimuth), f32::to_radians(elevation));
        // Toward the sun in Torque (x, y, z); the light points the other way,
        // in native (x, z, -y).
        let toward = [
            yaw.sin() * pitch.cos(),
            yaw.cos() * pitch.cos(),
            pitch.sin(),
        ];
        let expected = [-toward[0], -toward[2], toward[1]];
        let got = data.sun_direction;
        assert!(
            (0..3).all(|i| (got[i] - expected[i]).abs() < 1e-4),
            "{map}: {got:?} vs {expected:?}"
        );
    }
    Ok(())
}
