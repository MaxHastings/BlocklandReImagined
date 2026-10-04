//! Modern map loading and mode changes do not depend on legacy lighting preparation.
//! Split by behavior so serial software-GPU verification fits the per-binary watchdog.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use common::*;
use glam::Vec3;

/// Real converted content, source recovery only: no legacy bake, cleanup or
/// residual preparation. Captures are generated under target, never committed.
#[test]
#[ignore = "requires generated v20 content; bounded offscreen inspection"]
fn modern_dynamic_real_maps_without_legacy_preparation() -> Result<()> {
    use bri_render::terrain_scene::GpuTerrain;
    use std::{path::PathBuf, time::Instant};
    let content = std::env::var_os("BRI_CONTENT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    );
    let bundle = std::env::var_os("BRI_MODERN_BUNDLE").map_or_else(
        || bri_package::testing::pack_dir(&content, "map_bundle"),
        PathBuf::from,
    );
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("modern-lighting");
    std::fs::create_dir_all(&out)?;
    let (device, queue) = gpu()?;
    let (width, height) = (960, 540);
    let target = color_target(&device, wgpu::TextureFormat::Rgba8Unorm, width, height);
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, width, height).create_view(&Default::default());
    let bundle_data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("bundle.json"))?)?;
    let parameters = bri_render::lighting_parameters::Parameters::read(&bundle)?;
    for record in bundle_data["maps"].as_array().unwrap() {
        let id = record["id"].as_str().unwrap();
        let name = id.rsplit('/').next().unwrap().trim_end_matches(".mis");
        let started = Instant::now();
        let map = bri_render::scene_loader::load_map_bundle_dynamic(&bundle, id)?;
        let recovery_ms = started.elapsed().as_secs_f64() * 1000.;
        assert!(map.scene.lightmap_bases.is_empty());
        let lights = parameters.lights(id)?;
        assert_eq!(map.modern_lights.as_deref(), Some(lights.as_slice()));
        let settings = ShadowSettings {
            light_cubes: true,
            ..ShadowSettings::BEST
        };
        let mut renderer = SceneRenderer::with_settings(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            1,
            Some(settings),
        );
        renderer.set_dynamic_lights(&device, &queue, &lights)?;
        let scene = renderer.upload(&device, &queue, &map.scene)?;
        let spawn = Vec3::from(map.scene.spawn);
        let eye = spawn + Vec3::new(0., 3., 0.);
        let mut camera = Camera::perspective(
            eye.to_array(),
            (eye + Vec3::new(30., -3., 30.)).to_array(),
            width as f32 / height as f32,
            1.1,
            0.05,
            4000.,
        );
        camera.apply_environment(&map.scene);
        camera.ambient[3] = 3.;
        let mut terrain = map
            .terrain
            .into_iter()
            .map(|t| GpuTerrain::upload(&renderer, &device, &queue, t.into(), 4000.))
            .collect::<Result<Vec<_>>>()?;
        for t in &mut terrain {
            t.update(&device, &queue, &[eye], 4000.)?;
        }
        let terrain_draws: Vec<_> = terrain.iter().flat_map(GpuTerrain::draws).collect();
        let mut frame_ms = Vec::new();
        for frame in 0..22 {
            let start = Instant::now();
            renderer.update_camera(&queue, &camera);
            let mut encoder = device.create_command_encoder(&Default::default());
            renderer.render_shadows_with_geometry(
                &mut encoder,
                ShadowCasters {
                    scenes: &[],
                    instances: &[],
                },
                ShadowCasters {
                    scenes: &[],
                    instances: &[],
                },
                ShadowCasters {
                    scenes: &[&scene],
                    instances: &terrain_draws,
                },
            );
            renderer.render_with_instances(
                &mut encoder,
                &view,
                &depth,
                &[&scene],
                &terrain_draws,
                Some(wgpu::Color {
                    r: 1.,
                    g: 0.,
                    b: 1.,
                    a: 1.,
                }),
            );
            queue.submit([encoder.finish()]);
            device.poll(wgpu::PollType::wait_indefinitely())?;
            if frame >= 6 {
                frame_ms.push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
        // Read the final frame without an extra render or legacy preparation.
        let row = (width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            target.size(),
        );
        queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let mapped = buffer.slice(..).get_mapped_range()?;
        let pixels: Vec<u8> = mapped
            .chunks_exact(row as usize)
            .flat_map(|r| r[..width as usize * 4].iter().copied())
            .collect();
        assert!(
            pixels.chunks_exact(4).any(|p| p[..3] != [255, 0, 255]),
            "{name} did not draw over the diagnostic clear colour"
        );
        image::save_buffer(
            out.join(format!("{name}-dynamic.png")),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )?;
        frame_ms.sort_by(f64::total_cmp);
        eprintln!(
            "{name}: {} prepared lights loaded with geometry in {recovery_ms:.1} ms; 16 warm submitted/waited frames at 960x540 p50 {:.2} ms max {:.2} ms; {out:?}",
            lights.len(),
            frame_ms[8],
            frame_ms[15]
        );
    }
    Ok(())
}

#[test]
fn modern_loader_requires_neither_baked_images_nor_legacy_preparation() -> Result<()> {
    use bri_render::{
        lighting_parameters::{FILE, Parameters, fingerprint},
        scene_loader,
    };
    use std::{collections::BTreeMap, path::PathBuf};
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("modern-loader-{}", std::process::id()));
    let maps = bri_render::testing::rooms();
    bri_render::testing::write_bundle(&dir, &maps)?;
    let mut bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("bundle.json"))?)?;
    for lighting in bundle["lighting"].as_object_mut().unwrap().values_mut() {
        lighting["interiors"] =
            serde_json::json!([{"node":0,"detail":0,"slot":0,"file":"missing-illumination.png"}]);
    }
    std::fs::write(dir.join("bundle.json"), serde_json::to_vec(&bundle)?)?;
    for filename in bundle["assets"]
        .as_object()
        .unwrap()
        .values()
        .filter_map(|v| v.as_str())
    {
        if filename.ends_with(".interior.json") {
            let path = dir.join(filename);
            let mut interior: bri_content::interior::Interior =
                serde_json::from_slice(&std::fs::read(&path)?)?;
            for detail in &mut interior.details {
                for lightmap in &mut detail.lightmaps {
                    lightmap.png = vec![0, 1, 2, 3]; // deliberately not an image
                }
            }
            std::fs::write(path, serde_json::to_vec(&interior)?)?;
        }
    }
    let prepared = Parameters {
        schema_version: 1,
        bundle_sha256: fingerprint(&dir)?,
        maps: maps
            .iter()
            .map(|m| (m.id.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>(),
    };
    std::fs::write(dir.join(FILE), serde_json::to_vec(&prepared)?)?;
    // Declared illumination paths are absent; embedded bases remain source
    // corrupt asset bytes but the modern loader never decodes them.
    for map in &maps {
        let modern = scene_loader::load_map_bundle_dynamic(&dir, &map.id)?;
        assert!(modern.scene.lightmap_bases.is_empty());
        assert_eq!(modern.modern_lights, Some(Vec::new()));
        assert!(
            scene_loader::load_map_bundle(&dir, &map.id).is_err(),
            "compatibility loader continues to require its authored illumination"
        );
    }
    // Metadata drift is detected without ever opening those missing images.
    bundle["scope"] = "changed source inputs".into();
    std::fs::write(dir.join("bundle.json"), serde_json::to_vec(&bundle)?)?;
    assert!(Parameters::read(&dir).is_err());
    let fallback = scene_loader::load_map_bundle_dynamic(&dir, &maps[0].id)?;
    assert!(
        fallback
            .scene
            .omissions
            .iter()
            .any(|s| s.contains("live sun/ambient only"))
    );
    assert_eq!(fallback.modern_lights, Some(Vec::new()));
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn modern_shadows_off_stays_live_and_mode_switches_do_not_leak_residuals() -> Result<()> {
    let (device, queue) = gpu()?;
    let target = color_target(&device, wgpu::TextureFormat::Rgba8Unorm, 128, 128);
    let mut renderer =
        SceneRenderer::with_settings(&device, wgpu::TextureFormat::Rgba8Unorm, 1, None);
    let sun = Vec3::NEG_Y;
    let data = floor(sun, 0.);
    let scene = renderer.upload(&device, &queue, &data)?;
    let mut lighting = lamp_lighting(Vec3::new(0., 12., 0.));
    lighting.lights[0].color = [0.25; 3];
    lighting.lights[0].inner = 20.;
    lighting.lights[0].outer = 40.;
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1., 1.4, 0.05, 400.);
    camera.sun_color = [0.; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.];
    let mut frame = |renderer: &mut SceneRenderer, mode: f32| -> Result<i32> {
        camera.ambient[3] = mode;
        renderer.update_camera(&queue, &camera);
        let pixels = render(&device, &queue, renderer, &target, &[&scene], &[], &[])?;
        Ok(i32::from(pixels[((64 * 128 + 64) * 4) as usize + 1]))
    };
    for _ in 0..2 {
        renderer.set_light_volume(
            &device,
            &queue,
            Some(&bri_render::light_volume::LightVolume {
                origin: [-100.; 3],
                cell: 200.,
                dims: [1; 3],
                texels: vec![[255; 4]],
                rays: 0,
            }),
        )?;
        renderer.set_map_lighting(&device, &queue, Some(&lighting), false)?;
        assert!(
            (frame(&mut renderer, 2.)? - 77).abs() <= 2,
            "Unified keeps its legacy surface"
        );
        renderer.set_dynamic_lights(&device, &queue, &lighting.lights)?;
        let live = frame(&mut renderer, 3.)?;
        assert!(live > 80 && live < 130, "live unshadowed lamp {live}");
        renderer.set_map_light_tints(&queue, &[Vec3::ZERO]);
        assert!(
            (frame(&mut renderer, 3.)? - 26).abs() <= 2,
            "Dynamic retains only explicit ambient, neither map residual nor baked illumination"
        );
        assert!(
            (frame(&mut renderer, 0.)? - 77).abs() <= 2,
            "Classic surface stays legacy"
        );
    }
    Ok(())
}
