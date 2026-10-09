//! "Sky: Enhanced": the procedural atmosphere follows the real sun, Original
//! keeps the map's own sky textures, and far geometry fogs toward whichever
//! sky is drawn.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_content::environment::{Environment, Fog, Image};
use bri_render::{environment_scene, scene::*, shadow::ShadowSettings};
use common::*;
use glam::Vec3;

const SIZE: (u32, u32) = (96, 96);

/// A sky of one flat colour a face, with fog from `fog_start` to 1000.
fn sky(fog_start: f32) -> SceneData {
    let mut out = SceneData::default();
    let mut faces = Vec::new();
    for i in 0..6 {
        out.images.push(SceneImage {
            label: format!("face-{i}"),
            width: 1,
            height: 1,
            rgba: vec![200, 60, 200, 255],
            srgb: true,
        });
        faces.push(Image {
            file: format!("face-{i}.png"),
            source: format!("face-{i}"),
            sha256: "0".repeat(64),
            width: 1,
            height: 1,
        });
    }
    let env = Environment {
        schema_version: 2,
        source_materials: "fixture".into(),
        source_sha256: "0".repeat(64),
        faces,
        reflection: None,
        clouds: vec![],
        textures: true,
        bottom: false,
        horizon_band: false,
        solid_color: [0.3; 3],
        fog: Fog {
            start: fog_start,
            end: 1000.0,
            color: [0.5, 0.6, 0.5],
        },
        warnings: vec![],
    };
    let faces: Vec<usize> = (1..=6).collect();
    environment_scene::append(&mut out, &env, &faces, &[]).unwrap();
    out
}

fn toward(azimuth: f32, elevation: f32) -> Vec3 {
    let (a, e) = (azimuth.to_radians(), elevation.to_radians());
    Vec3::new(a.sin() * e.cos(), e.sin(), a.cos() * e.cos())
}

struct Shot {
    pixels: Vec<u8>,
}
impl Shot {
    fn at(&self, x: u32, y: u32) -> [i32; 3] {
        let i = ((y * SIZE.0 + x) * 4) as usize;
        [0, 1, 2].map(|c| i32::from(self.pixels[i + c]))
    }
}

/// The scene drawn looking along +Z, pitched up, with the sun at `sun`
/// (a direction toward it) under `enhanced`, plus `extra` geometry.
fn shoot(enhanced: bool, sun: Vec3, scene: &SceneData, extra: Option<&SceneData>) -> Result<Shot> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let target = color_target(&device, format, SIZE.0, SIZE.1);
    let sky = renderer.upload(&device, &queue, scene)?;
    let uploaded = extra
        .map(|e| renderer.upload(&device, &queue, e))
        .transpose()?;
    let mut camera = Camera::perspective([0., 2., 0.], [0., 2. + 0.35, 1.], 1.0, 1.3, 0.1, 3000.0);
    camera.apply_environment(scene);
    camera.sun_direction = (-sun).extend(0.).to_array();
    camera.sky_sun = sun.extend(0.).to_array();
    camera.set_enhanced_sky(enhanced);
    renderer.update_camera(&queue, &camera);
    let mut scenes = vec![&sky];
    scenes.extend(uploaded.as_ref());
    let pixels = render(&device, &queue, &mut renderer, &target, &scenes, &[], &[])?;
    if let Ok(dir) = std::env::var("BRI_SKY_DUMP") {
        let name = format!(
            "{dir}/{}-{:.0}{}.ppm",
            if enhanced { "enhanced" } else { "original" },
            sun.y.asin().to_degrees(),
            if extra.is_some() { "-wall" } else { "" }
        );
        let mut ppm = format!("P6\n{} {}\n255\n", SIZE.0, SIZE.1).into_bytes();
        ppm.extend(pixels.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]));
        let _ = std::fs::write(name, ppm);
    }
    Ok(Shot { pixels })
}

#[test]
fn original_keeps_the_map_sky_whatever_the_sun_does() -> Result<()> {
    let scene = sky(500.0);
    let noon = shoot(false, toward(30., 70.), &scene, None)?;
    let night = shoot(false, toward(30., -40.), &scene, None)?;
    assert_eq!(noon.pixels, night.pixels);
    // The authored faces are magenta, not a sky blue.
    let [r, g, b] = noon.at(48, 4);
    assert!(r > g + 30 && b > g + 30, "{r} {g} {b}");
    Ok(())
}

#[test]
fn enhanced_sky_runs_from_blue_noon_through_sunset_to_night() -> Result<()> {
    let scene = sky(500.0);
    let noon = shoot(true, toward(30., 70.), &scene, None)?;
    let [r, g, b] = noon.at(48, 3);
    assert!(b > g && g > r && b > 120, "zenith {r} {g} {b}");
    // The horizon is paler than the zenith above it.
    let low = noon.at(48, 62);
    assert!(low[0] > r, "horizon {low:?} zenith {r}");
    let sunset = shoot(true, toward(30., 1.5), &scene, None)?;
    let [r, _, b] = sunset.at(48, 60);
    assert!(r > b + 30, "sunset horizon red {r} blue {b}");
    let night = shoot(true, toward(30., -40.), &scene, None)?;
    for y in [3, 30, 60] {
        let p = night.at(48, y);
        assert!(p.iter().all(|c| *c < 70) && p[2] >= p[0], "night {p:?}");
    }
    // The sky changes with the sun, unlike Original's.
    assert_ne!(noon.pixels, sunset.pixels);
    Ok(())
}

/// A wall of white bricks, far and tall enough to fill the middle of the
/// frame, fogs toward the sky drawn beside it.
fn wall(distance: f32) -> SceneData {
    cuboid(
        Vec3::new(-400., -50., distance),
        Vec3::new(400., 60., distance + 4.),
    )
}

#[test]
fn far_geometry_fogs_toward_the_enhanced_sky() -> Result<()> {
    let scene = sky(100.0);
    let far = wall(900.0);
    for sun in [toward(30., 70.), toward(30., 1.5), toward(30., -40.)] {
        let shot = shoot(true, sun, &scene, Some(&far))?;
        // Where the wall meets the sky beside its top edge: the wall's
        // pixel is within a few levels of the sky's, no silhouette.
        let rows: Vec<[i32; 3]> = (0..SIZE.1).map(|y| shot.at(48, y)).collect();
        let jump = rows
            .windows(2)
            .map(|w| (0..3).map(|c| (w[0][c] - w[1][c]).abs()).max().unwrap())
            .max()
            .unwrap();
        assert!(jump < 28, "a {jump}-level edge in {rows:?}");
    }
    Ok(())
}

/// With `BRI_SKY_DUMP=<dir>`: the day, from midday to night, as PPM frames
/// for a human look (Original beside Enhanced at each sun height).
#[test]
fn the_day_for_a_human_look() -> Result<()> {
    if std::env::var("BRI_SKY_DUMP").is_err() {
        return Ok(());
    }
    let scene = sky(300.0);
    for elevation in [60., 20., 6., 0.5, -6., -30.] {
        for enhanced in [false, true] {
            shoot(enhanced, toward(30., elevation), &scene, None)?;
        }
    }
    Ok(())
}
