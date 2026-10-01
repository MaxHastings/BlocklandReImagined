//! Made-up weather packs for tests that have no converted weather pack.
//! Every value here is invented; nothing is read from an original mission.
use crate::{
    Definition, Placement, TextureRecord, WeatherManifest, WeatherPack, WeatherTexture, sha256,
};
use std::{collections::BTreeMap, sync::Arc};

/// A `width` x `height` texture whose alpha rises across it to `peak`.
pub fn texture(id: &str, width: u32, height: u32, peak: u8) -> WeatherTexture {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let t = (x + y) as f32 / (width + height - 2).max(1) as f32;
            rgba.extend_from_slice(&[230, 235, 255, (t * f32::from(peak)).round() as u8]);
        }
    }
    WeatherTexture {
        id: id.into(),
        width,
        height,
        rgba,
    }
}

fn record(t: &WeatherTexture) -> TextureRecord {
    TextureRecord {
        file: format!("{}.png", t.id.replace('/', "_")),
        sha256: String::new(),
        rgba_sha256: sha256(&t.rgba),
        width: t.width,
        height: t.height,
        source_paths: vec![],
        alpha_policy: "native_fixture".into(),
    }
}

/// A rain definition `id` drawing `drop` drops and `splash` splashes.
pub fn rain(id: &str, drop: &str, splash: Option<&str>) -> Definition {
    Definition {
        id: id.into(),
        drop_texture: drop.into(),
        splash_texture: splash.map(str::to_owned),
        drop_radius: 0.75,
        splash_radius: 0.2,
        true_billboards: false,
        splash_seconds: 0.25,
        drop_animation_seconds: 0.,
        animate_splashes: true,
        drops_per_side: 2,
        splashes_per_side: 2,
    }
}

/// A placement of `definition` on `map` with `drops` drops in a 10 x 10
/// box at the origin, falling with the wind and following the camera.
pub fn placement(map: &str, definition: &str, drops: u32) -> Placement {
    Placement {
        id: format!("{map}#{definition}"),
        map_id: map.into(),
        definition: definition.into(),
        position: [0.; 3],
        drops,
        width: 10.,
        height: 10.,
        speed_per_tick: [0.2, 0.3],
        mass: [0.75, 0.85],
        turbulence_amplitude: 0.1,
        turbulence_radians_per_tick: 0.2,
        use_turbulence: false,
        rotate_with_camera_velocity: true,
        collision: true,
        follow_camera: true,
        use_wind: true,
        authored_sky_wind: [0.; 3],
        reference_wind_velocity: [0.; 3],
        original_fields: BTreeMap::new(),
    }
}

/// A manifest of `definitions` and `placements` over `textures`.
pub fn manifest(
    definitions: Vec<Definition>,
    placements: Vec<Placement>,
    textures: &[WeatherTexture],
) -> WeatherManifest {
    WeatherManifest {
        schema_version: 1,
        legacy_tick_seconds: 0.032,
        definitions,
        placements,
        textures: textures.iter().map(|t| (t.id.clone(), record(t))).collect(),
        sources: Vec::new(),
        assumptions: Vec::new(),
    }
}

/// One rain definition (`rain`, drops and splashes from one opaque 4x4
/// atlas) placed with 64 drops on map `map`, after `change`.
pub fn pack(change: impl FnOnce(&mut WeatherManifest)) -> Arc<WeatherPack> {
    let atlas = WeatherTexture {
        id: "atlas".into(),
        width: 4,
        height: 4,
        rgba: vec![255; 4 * 4 * 4],
    };
    let mut m = manifest(
        vec![rain("rain", "atlas", Some("atlas"))],
        vec![placement("map", "rain", 64)],
        std::slice::from_ref(&atlas),
    );
    change(&mut m);
    WeatherPack::from_parts(m, vec![atlas]).unwrap()
}

/// Two maps' weather like a small stock pack: translucent rain with
/// splashes on `fixture/storm` and splash-free snow on `fixture/winter`,
/// each with its own wind, over three translucent textures.
pub fn showcase_pack() -> Arc<WeatherPack> {
    let textures = vec![
        texture("fixture/rain", 8, 16, 60),
        texture("fixture/splash", 8, 8, 140),
        texture("fixture/snow", 8, 8, 200),
    ];
    let mut snow = rain("fixture/snow", "fixture/snow", None);
    snow.drop_radius = 0.3;
    snow.drops_per_side = 2;
    snow.splashes_per_side = 1;
    let mut storm = placement("fixture/storm", "fixture/rain", 96);
    storm.reference_wind_velocity = [2.0, 0.0, -1.0];
    storm.use_turbulence = true;
    (storm.width, storm.height) = (30., 40.);
    let mut winter = placement("fixture/winter", "fixture/snow", 48);
    winter.speed_per_tick = [0.12, 0.16];
    winter.reference_wind_velocity = [-0.5, 0.0, 0.5];
    winter.use_turbulence = true;
    (winter.width, winter.height) = (24., 30.);
    let m = manifest(
        vec![
            rain("fixture/rain", "fixture/rain", Some("fixture/splash")),
            snow,
        ],
        vec![storm, winter],
        &textures,
    );
    WeatherPack::from_parts(m, textures).unwrap()
}
