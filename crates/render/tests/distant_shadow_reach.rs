//! Distant receivers hand off to the stable whole-map layer: over the final
//! quarter of the shadow distance their sun read moves onto `far_map_lit`
//! (fitted once per map and sun, view-independent), so both the map's
//! surfaces and the casters keep shading them at a coarser texel size - the
//! layer is redrawn every frame with the casters on top of the map, so
//! moving things persist at range instead of fading out.
//! The hand off completes exactly at the shadow distance - past it no
//! view-fitted cascade is sampled any more: those boxes follow the view, so
//! reading them clamped makes distant shadows swim as the camera moves, and a
//! zoom refits every box so even a still eye sees them jump between notches.
//! Repro of the far-back buildings going fully lit - or phasing in and out as
//! the camera moves or zooms: their receivers used to be read from view-fitted
//! cascade boxes that no longer held them, so casters toggled as they entered
//! and left a moving slice.

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
    // A low sun behind the eye: its light travels across the whole scene in
    // the view direction. The face samples sit at a view depth of ~615 against
    // the LOW shadow distance of 100 - fully past the hand off ramp, so their
    // read is entirely the stable layer's (casters have faded out over it).
    let elevation = std::f32::consts::PI * 8.0 / 180.0; // 8 degrees
    let sun = Vec3::new(0.0, -elevation.sin(), elevation.cos()).normalize();
    // The eye looks down a row of casters toward a distant building.
    let eye = Vec3::new(0.0, 5.0, -160.0);
    let mut camera = Camera::perspective([eye.x, eye.y, eye.z], [0.0, 5.0, 300.0], 1.0, 1.2, 0.05, 1000.0);
    camera.sun_direction = sun.extend(0.).to_array();
    camera.sun_color = [0.6, 0.6, 0.6, 0.];
    camera.ambient = [0.2, 0.2, 0.2, 2.0]; // Unified: map surfaces read all layers.

    let floor_min = Vec3::new(-60.0, -0.3, -200.0);
    let floor_max = Vec3::new(60.0, 0.0, 520.0);
    let slab_min = Vec3::new(-20.0, 0.0, 100.0); // bricks (caster layer)
    let slab_max = Vec3::new(0.0, 60.0, 120.0);
    let wall_min = Vec3::new(0.0, 0.0, 100.0); // map surface (map layer)
    let wall_max = Vec3::new(20.0, 60.0, 120.0);
    let building_min = Vec3::new(-10.0, 0.0, 450.0);
    let building_max = Vec3::new(10.0, 40.0, 470.0);

    // The building's front face: the wall's shadow reaches its lower band
    // (the sun is low enough to lean that far); above it stays lit. The slab
    // covers x < 0, so point 0 sits in its shadow - the whole-map layer now
    // redraws the casters every frame, so a live caster's shade persists at
    // this range too (coarse, but present).
    let slab_band = Vec3::new(-5.0, 5.0, 450.0);
    let wall_shadow = Vec3::new(5.0, 5.0, 450.0);
    let open_face = Vec3::new(0.0, 30.0, 450.0);

    for pcss in [false, true] {
        let settings = ShadowSettings { pcss, ..ShadowSettings::LOW };
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        let floor = surface_box(&mut renderer, &device, &queue, floor_min, floor_max)?;
        let slab = renderer.upload(
            &device,
            &queue,
            &cuboid(slab_min, slab_max),
        )?;
        let wall = surface_box(&mut renderer, &device, &queue, wall_min, wall_max)?;
        let building = surface_box(&mut renderer, &device, &queue, building_min, building_max)?;
        // A small live caster off to the side: its shadow lands on the mid
        // face below - at a view depth of ~173 against the LOW distance of 100,
        // deep inside the old hand off band where clamped fitted reads used to be.
        let mid = renderer.upload(
            &device,
            &queue,
            &cuboid(Vec3::new(30.0, 0.0, -55.0), Vec3::new(36.0, 20.0, -45.0)),
        )?;

        // One render of the face samples; the map list changes between runs,
        // so the stable layer redraws (its key follows the drawn scenes).
        let mut run = |cam: &Camera, casters: &[&GpuScene], map: Vec<&GpuScene>| -> Result<[i32; 3]> {
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
                &map,
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
        let open = run(&camera, &[], vec![&floor, &building])?;
        assert!(open[2] > 150, "pcss={pcss}: open face {open:?}");
        assert!(open[0] > 150 && open[1] > 150, "pcss={pcss}: lit baseline {open:?}");

        // The wall's static shadow persists on the distant face: it is drawn
        // into the stable whole-map layer, which now serves this receiver.
        let shaded_wall = run(&camera, &[], vec![&floor, &wall, &building])?;
        assert!(
            shaded_wall[1] < open[1] - 30,
            "pcss={pcss}: wall shadow missing on the distant face {shaded_wall:?} vs {open:?}"
        );
        // The wall does not cover point 0: no false shadow there.
        assert!(
            (shaded_wall[0] - open[0]).abs() <= 8,
            "pcss={pcss}: false shadow on the slab band {shaded_wall:?} vs {open:?}"
        );

        // With the live caster added: its shadow persists at this range too
        // (the whole-map layer redraws casters every frame - coarse, but
        // present and steady), alongside the wall's static one.
        let both = run(&camera, &[&slab], vec![&floor, &wall, &building])?;
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

        // Zoom stability (regression): stepping the field of view refits every
        // cascade box while the eye stays put. The far face sits fully past
        // the hand off and the mid face where its old band used to be - both
        // reads are entirely the stable layer now, which no refit touches, so
        // the same pixels must come back at any zoom. Before the hand off
        // completed at the shadow distance those were clamped samples of boxes
        // that changed shape with every notch: shadows jumped and phased while
        // zooming in on distant buildings.
        let mut zoomed = camera;
        zoomed.view_projection = Camera::perspective(
            [eye.x, eye.y, eye.z],
            [0.0, 5.0, 300.0],
            1.0,
            0.6, // half the field of view: a zoomed-in look at the same faces
            0.05,
            1000.0,
        )
        .view_projection;
        let zoom_pixels = run(&zoomed, &[&slab], vec![&floor, &wall, &building])?;
        for (i, label) in ["slab band", "wall shadow", "open face"].into_iter().enumerate() {
            assert!(
                (zoom_pixels[i] - both[i]).abs() <= 2,
                "pcss={pcss}: distant face {label} moved under zoom {zoom_pixels:?} vs {both:?}"
            );
        }
        // The mid floor point: inside the old hand off band. Its read is the
        // whole-map layer alone now (the caster's shade comes from there too,
        // which no refit touches), so it must be steady under zoom - before,
        // its clamped fitted share swam with every refit.
        let mid_face = Vec3::new(33.0, 0.0, 10.0);
        let mut sample_mid = |cam: &Camera| -> Result<i32> {
            renderer.update_camera(&queue, cam);
            let target = color_target(&device, format, width, height);
            let pixels = render_with_map(
                &device,
                &queue,
                &mut renderer,
                &target,
                &[&floor],
                &[&mid],
                &[],
                &[&floor],
            )?;
            let view_projection = Mat4::from_cols_array(&cam.view_projection);
            let ndc = view_projection.project_point3(mid_face);
            let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
            let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
            Ok(i32::from(pixels[(y * width as usize + x) * 4 + 1]))
        };
        let mid_wide = sample_mid(&camera)?;
        let mid_zoom = sample_mid(&zoomed)?;
        assert!(
            (mid_zoom - mid_wide).abs() <= 2,
            "pcss={pcss}: mid face moved under zoom {mid_zoom} vs {mid_wide}"
        );
    }
    Ok(())
}
