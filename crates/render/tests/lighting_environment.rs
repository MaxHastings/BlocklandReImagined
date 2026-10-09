//! Map lightmaps and water follow the live day/night environment.
//! Split by behavior so serial software-GPU verification fits the per-binary watchdog.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use common::*;
use glam::{Mat4, Vec3};

/// The admin Environment (bri_content::atmosphere) relights the map's
/// lightmaps from what they were baked with: an unchanged environment draws
/// exactly as before, night takes the baked sun away, more ambient light
/// raises everything, and a moved sun lights the texels the bake left open.
#[test]
fn a_changed_environment_relights_the_maps_lightmaps() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (128u32, 128u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let sun = Vec3::new(0.0, -1.0, 1.0);
    let map = renderer.upload(&device, &queue, &floor(sun, 0.6))?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.).to_array();
    camera.sun_color = [0.6, 0.6, 0.6, 1.];
    camera.ambient = [0.3, 0.3, 0.3, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    let points = [Vec3::new(-5.0, 0.0, 5.0), Vec3::new(5.0, 0.0, 5.0)];
    let baked = camera;
    let authored = bri_content::atmosphere::Live {
        sun_direction: sun.to_array(),
        sun_toward: (-sun).to_array(),
        direct_light: [0.6; 3],
        ambient_light: [0.3; 3],
        shadow_color: None,
        fog_start: 0.0,
        fog_end: 0.0,
        fog_color: [0.0; 3],
        sky_tint: [1.0; 3],
        flare: ([0.0; 4], 1.0),
        vignette: None,
        enhanced_sky: false,
    };
    let mut run = |live: Option<&bri_content::atmosphere::Live>| -> Result<[i32; 2]> {
        let mut camera = baked;
        if let Some(live) = live {
            camera.apply_atmosphere(live);
        }
        renderer.update_camera(&queue, &camera);
        let pixels = render(&device, &queue, &mut renderer, &target, &[&map], &[], &[])?;
        Ok(points.map(|p| at(&pixels, p)))
    };
    let [lit, shade] = run(None)?;
    assert!(lit > shade + 60, "{lit} {shade}");
    assert_eq!(run(Some(&authored))?, [lit, shade]);
    // No sun: the baked sun goes, the static light stays.
    let night = bri_content::atmosphere::Live {
        direct_light: [0.0; 3],
        ..authored
    };
    let [night_lit, night_shade] = run(Some(&night))?;
    assert!((night_lit - shade).abs() <= 3, "{night_lit} {shade}");
    assert!((night_shade - shade).abs() <= 3, "{night_shade} {shade}");
    // More ambient light raises both halves by that much.
    let bright = bri_content::atmosphere::Live {
        ambient_light: [0.5; 3],
        ..authored
    };
    let [bright_lit, bright_shade] = run(Some(&bright))?;
    assert!(
        (bright_shade - shade - 51).abs() <= 3,
        "{bright_shade} {shade}"
    );
    assert!(bright_lit >= lit + 40, "{bright_lit} {lit}");
    // The sun overhead: the half the bake lit takes it at full strength;
    // the half in the map's own shadow stays shaded.
    let noon = bri_content::atmosphere::Live {
        sun_direction: [0.0, -1.0, 0.0],
        ..authored
    };
    let [noon_lit, noon_shade] = run(Some(&noon))?;
    assert!((noon_lit - 230).abs() <= 4, "{noon_lit}");
    assert!((noon_shade - shade).abs() <= 3, "{noon_shade} {shade}");
    Ok(())
}

/// The water surface, shore and authored reflection darken together, in
/// plain and depth-mapped paths and at both near and far viewing distances.
#[test]
fn water_follows_the_live_day_night_environment() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 128, 128);
    for depth_mapped in [false, true] {
        let mut scene = SceneData::default();
        let mut water = bri_content::water::Water::volume([-30.0, -2.0, -30.0], [30.0, 0.0, 30.0]);
        water.depth_mask = depth_mapped;
        water.reflection = Some(water.surface.clone());
        water.reflection_intensity = 0.5;
        water.wave_amplitude = 0.0;
        bri_render::water_scene::append(
            &mut scene,
            &water,
            [0; 3],
            ([0.5; 4], 8.0),
            false,
            |_, _| None,
        )?;
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, None);
        let mesh = renderer.upload(&device, &queue, &scene)?;
        for mode in [0.0, 3.0] {
            for distance in [12.0, 100.0] {
                let mut camera =
                    Camera::perspective([0.0, distance, 0.1], [0.0; 3], 1.0, 1.2, 0.05, 400.0);
                camera.apply_environment(&scene);
                camera.ambient[3] = mode;
                let day = camera;
                renderer.update_camera(&queue, &day);
                let pixels = render(&device, &queue, &mut renderer, &target, &[&mesh], &[], &[])?;
                if mode == 3.0 {
                    let mut poison = day;
                    poison.baked_sun_direction = [1.0, 0.0, 0.0, 1.0];
                    poison.baked_sun_color = [50.0; 4];
                    poison.baked_ambient = [50.0; 4];
                    renderer.update_camera(&queue, &poison);
                    let after =
                        render(&device, &queue, &mut renderer, &target, &[&mesh], &[], &[])?;
                    assert_eq!(
                        pixels, after,
                        "modern water ignores baked environment uniforms"
                    );
                }
                let middle = (64 * 128 + 64) * 4;
                let daytime = pixels[middle..middle + 3]
                    .iter()
                    .map(|c| u32::from(*c))
                    .sum::<u32>();
                camera.baked_sun_direction = day.sun_direction;
                camera.baked_sun_direction[3] = 1.0;
                camera.baked_sun_color = day.sun_color;
                camera.baked_ambient = day.ambient;
                camera.sun_color = [0.01, 0.01, 0.02, 0.0];
                camera.ambient = [0.02, 0.025, 0.04, mode];
                renderer.update_camera(&queue, &camera);
                let pixels = render(&device, &queue, &mut renderer, &target, &[&mesh], &[], &[])?;
                let nighttime = pixels[middle..middle + 3]
                    .iter()
                    .map(|c| u32::from(*c))
                    .sum::<u32>();
                assert!(
                    daytime > 100 && nighttime * 3 < daytime,
                    "water mode={mode}, depth={depth_mapped}, distance={distance}: day={daytime}, night={nighttime}"
                );
            }
        }
    }
    Ok(())
}
