//! A map wall and an independent caster shade one distant receiver. Check the
//! handoff, camera-independent sampling and invalidation after caster removal.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use common::*;
use glam::{Mat4, Vec3};

/// A map-surface box (drawn into the map layer), like the roofs in `map_walls`.
fn surface_box(
    renderer: &mut SceneRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    min: Vec3,
    max: Vec3,
) -> Result<GpuScene> {
    let mut data = cuboid(min, max);
    data.images.push(SceneImage::white());
    data.materials[0] = Material::surface("box", 0, 0);
    renderer.upload(device, queue, &data)
}

#[test]
fn distant_receivers_hand_off_to_the_stable_far_map() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let elevation = 8.0f32.to_radians();
    let sun = Vec3::new(0.0, -elevation.sin(), elevation.cos()).normalize();
    let camera_at = |z, fov| {
        let mut camera =
            Camera::perspective([0.0, 5.0, z], [0.0, 5.0, z + 1.0], 1.0, fov, 0.05, 1000.0);
        camera.sun_direction = sun.extend(0.).to_array();
        camera.sun_color = [0.6, 0.6, 0.6, 0.];
        camera.ambient = [0.2, 0.2, 0.2, 2.0];
        camera
    };
    let camera = camera_at(-160.0, 1.2);

    let floor_min = Vec3::new(-60.0, -0.3, -200.0);
    let floor_max = Vec3::new(60.0, 0.0, 520.0);
    let slab_min = Vec3::new(-20.0, 0.0, 100.0); // bricks (caster layer)
    let slab_max = Vec3::new(0.0, 60.0, 120.0);
    let wall_min = Vec3::new(0.0, 0.0, 100.0); // map surface (map layer)
    let wall_max = Vec3::new(20.0, 60.0, 120.0);
    let building_min = Vec3::new(-10.0, 0.0, 450.0);
    let building_max = Vec3::new(10.0, 40.0, 470.0);

    // Left and right sample positions receive the caster and wall shadows.
    // The high sample remains outside both shadows.
    let slab_band = Vec3::new(-5.0, 5.0, 450.0);
    let wall_shadow = Vec3::new(5.0, 5.0, 450.0);
    let open_face = Vec3::new(0.0, 30.0, 450.0);

    for pcss in [false, true] {
        let settings = ShadowSettings {
            pcss,
            ..ShadowSettings::LOW
        };
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        let floor = surface_box(&mut renderer, &device, &queue, floor_min, floor_max)?;
        let slab = renderer.upload(&device, &queue, &cuboid(slab_min, slab_max))?;
        let wall = surface_box(&mut renderer, &device, &queue, wall_min, wall_max)?;
        let building = surface_box(&mut renderer, &device, &queue, building_min, building_max)?;
        let mut run =
            |cam: &Camera, casters: &[&GpuScene], map: &[&GpuScene]| -> Result<[i32; 3]> {
                renderer.update_camera(&queue, cam);
                let target = color_target(&device, format, width, height);
                let pixels = render_with_map(
                    &device,
                    &queue,
                    &mut renderer,
                    &target,
                    &[&floor, &building],
                    casters,
                    &[],
                    map,
                )?;
                let view_projection = Mat4::from_cols_array(&cam.view_projection);
                Ok([slab_band, wall_shadow, open_face].map(|p| {
                    let ndc = view_projection.project_point3(p);
                    let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
                    let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
                    i32::from(pixels[(y * width as usize + x) * 4 + 1])
                }))
            };

        // Sanity: with nothing between the face and the sun, all three
        // samples read fully lit through the stable layer.
        let open = run(&camera, &[], &[&floor, &building])?;
        assert!(
            open.into_iter().all(|channel| channel > 150),
            "pcss={pcss}: {open:?}"
        );

        // The wall's static shadow persists on the distant face: it is drawn
        // into the stable whole-map layer, which now serves this receiver.
        let map = [&floor, &wall, &building];
        let shaded_wall = run(&camera, &[], &map)?;
        assert!(
            shaded_wall[1] < open[1] - 30,
            "pcss={pcss}: wall shadow missing on the distant face {shaded_wall:?} vs {open:?}"
        );
        // The wall does not cover point 0: no false shadow there.
        assert!(
            (shaded_wall[0] - open[0]).abs() <= 8,
            "pcss={pcss}: false shadow on the slab band {shaded_wall:?} vs {open:?}"
        );

        let both = run(&camera, &[&slab], &map)?;
        assert!(
            both[1] < open[1] - 30,
            "pcss={pcss}: wall shadow lost with the caster present {both:?} vs {open:?}"
        );
        assert!(
            both[0] < open[0] - 30,
            "pcss={pcss}: live caster shadow no longer persists on the distant face {both:?} vs {open:?}"
        );
        // Neither shades the upper band.
        assert!(
            (both[2] - open[2]).abs() <= 8,
            "pcss={pcss}: false shadow on the open face {both:?} vs {open:?}"
        );

        let zoom_pixels = run(&camera_at(-160.0, 0.6), &[&slab], &map)?;
        for (zoom, original) in zoom_pixels.into_iter().zip(both) {
            assert!(
                (zoom - original).abs() <= 2,
                "pcss={pcss}: distant samples moved under zoom {zoom_pixels:?} vs {both:?}"
            );
        }
        // Compare the same shadowed points halfway through the transition
        // (87.5 units from the eye). The two layers should not create a lit band.
        let handoff = run(&camera_at(362.5, 1.2), &[&slab], &map)?;
        assert!(
            handoff[0] <= both[0] + 28 && handoff[1] <= both[1] + 28,
            "pcss={pcss}: shadows lightened at the handoff {handoff:?} vs {both:?}"
        );
        let removed = run(&camera, &[], &map)?;
        for (after, before) in removed.into_iter().zip(shaded_wall) {
            assert!((after - before).abs() <= 2, "pcss={pcss}: stale caster");
        }
    }
    Ok(())
}
