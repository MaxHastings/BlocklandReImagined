//! Dynamic illumination uses current lamps and geometry throughout cache refreshes.
//! Split by behavior so serial software-GPU verification fits the per-binary watchdog.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use common::*;
use glam::{Mat4, Vec3};

/// Modern Dynamic ignores contradictory legacy shares and residuals. Current
/// geometry hides a point light from map surfaces and objects alike; removing
/// that geometry invalidates its cached cube. Live casters use bounded slots.
#[test]
fn dynamic_lighting_lights_map_surfaces_live_from_every_light() -> Result<()> {
    use bri_render::map_lighting::MapLight;
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let sun = Vec3::new(0.0, -1.0, 0.3);
    // 0.05 left over everywhere, no baked sun; the light reaches the
    // texels in front of the wall (x = 4, texel column 11) and not behind.
    let floor_data = dynamic_floor(
        sun,
        |_| [13, 13, 13, 0],
        &[0],
        |_, x| if x <= 10 { 1.0 } else { 0.0 },
    );
    let mut wall = cuboid(Vec3::new(4.0, 0.0, -10.0), Vec3::new(4.2, 6.0, 10.0));
    wall.images = vec![SceneImage::white()];
    wall.materials[0] = Material::surface("wall", 0, 0);
    let mut lighting = lamp_lighting(Vec3::new(0.0, 12.0, 0.0));
    lighting.lights = vec![MapLight {
        position: [0.0, 12.0, 0.0],
        color: [0.8; 3],
        inner: 0.0,
        outer: 40.0,
        channel: None,
    }];
    for texel in &mut lighting.visibility.texels {
        *texel = [0; 8];
    }
    lighting.visibility.dims = [0; 3];
    lighting.visibility.texels.clear();
    lighting.visibility.origin = [f32::NAN; 3];
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.0, 0.0, 0.0, 3.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // The floor in front of the wall, behind it and under the slab; the
    // blocks' tops in front and behind.
    let points = [
        Vec3::new(-6.0, 0.0, -1.0),
        Vec3::new(6.5, 0.0, -1.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(-6.0, 0.5, 5.0),
        Vec3::new(6.5, 0.5, 5.0),
    ];
    let expected = |p: Vec3| {
        let distance = p.distance(Vec3::new(0.0, 12.0, 0.0));
        let lit = 0.8 * (40.0 - distance) / 40.0 * 12.0 / distance;
        (lit * 255.0).round() as i32
    };
    for lamps in [0, ShadowSettings::BEST.lamps] {
        let settings = ShadowSettings {
            lamps,
            light_cubes: true,
            ..ShadowSettings::BEST
        };
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        renderer.set_map_lighting(&device, &queue, Some(&lighting), true)?;
        let floor = renderer.upload(&device, &queue, &floor_data)?;
        let mut wall = renderer.upload(&device, &queue, &wall)?;
        let slab = renderer.upload(
            &device,
            &queue,
            &cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0)),
        )?;
        let front_block = renderer.upload(
            &device,
            &queue,
            &cuboid(Vec3::new(-7.0, 0.0, 4.0), Vec3::new(-5.0, 0.5, 6.0)),
        )?;
        let behind_block = renderer.upload(
            &device,
            &queue,
            &cuboid(Vec3::new(5.5, 0.0, 4.0), Vec3::new(7.5, 0.5, 6.0)),
        )?;
        let frame = |renderer: &mut SceneRenderer, wall: &GpuScene| -> Result<[i32; 5]> {
            renderer.update_camera(&queue, &camera);
            let pixels = render_with_map(
                &device,
                &queue,
                renderer,
                &target,
                &[&floor, &front_block, &behind_block],
                &[&slab],
                &[],
                &[&floor, wall],
            )?;
            Ok(points.map(|p| at(&pixels, p)))
        };
        let [front, behind, under, lit_block, hidden_block] = frame(&mut renderer, &wall)?;
        assert_eq!(
            frame(&mut renderer, &wall)?,
            [front, behind, under, lit_block, hidden_block],
            "lamps {lamps}"
        );
        assert!(
            front >= expected(points[0]) - 3 && front <= expected(points[0]) + 35,
            "lamps {lamps}: front {front}"
        );
        assert!(behind <= 2, "lamps {lamps}: behind {behind}");
        if lamps == 0 {
            // No slot, no brick shadow: the texels see only the map.
            assert!(under >= expected(points[2]) - 3, "under {under}");
        } else {
            assert!(under <= 3, "under the slab {under}");
        }
        assert!(lit_block > 100, "lamps {lamps}: block in front {lit_block}");
        assert!(
            hidden_block < 10,
            "lamps {lamps}: block behind the wall {hidden_block}"
        );
        renderer.set_map_light_tints(&queue, &[Vec3::ZERO]);
        let off = frame(&mut renderer, &wall)?;
        assert!(
            off[..3].iter().all(|v| *v <= 2),
            "lamps {lamps}: switched off {off:?}"
        );
        assert!(off[3] < 10, "lamps {lamps}: block switched off {}", off[3]);
        renderer.set_map_light_tints(&queue, &[Vec3::ONE]);
        wall.hide_indices(std::slice::from_ref(&(0..36)));
        let removed = frame(&mut renderer, &wall)?;
        assert!(
            removed[1] > 80 && removed[4] > 80,
            "geometry removal relights floor and object: {removed:?}"
        );
    }
    Ok(())
}

/// A warmed 24-lamp geometry cache needs six frames to refresh. Invalidating
/// unrelated geometry must not remove those lamps from final illumination.
/// A new source has no compatible history and lights immediately while its
/// current geometry cubes warm, without sampling any legacy lighting input.
#[test]
fn dynamic_cube_refresh_never_blacks_out_current_lamps() -> Result<()> {
    use bri_render::map_lighting::MapLight;
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 128, 128);
    let settings = ShadowSettings {
        lamps: 0,
        light_cubes: true,
        ..ShadowSettings::BEST
    };
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
    let lights = vec![
        MapLight {
            position: [0.0, 12.0, 0.0],
            color: [0.8 / 24.0; 3],
            inner: 0.0,
            outer: 40.0,
            channel: None,
        };
        24
    ];
    renderer.set_dynamic_lights(&device, &queue, &lights)?;
    let floor = renderer.upload(&device, &queue, &floor(Vec3::NEG_Y, 0.0))?;
    // Outside every lamp's reach: hiding this batch changes the exact map
    // cache key, but cannot change the illumination at the floor's center.
    let mut unrelated = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(80.0, 0.0, -2.0), Vec3::new(81.0, 2.0, 2.0)),
    )?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = Vec3::NEG_Y.extend(0.0).to_array();
    camera.sun_color = [0.; 4];
    camera.ambient = [0., 0., 0., 3.];
    let frame = |renderer: &mut SceneRenderer, unrelated: &GpuScene| -> Result<i32> {
        renderer.update_camera(&queue, &camera);
        let pixels = render_with_map(
            &device,
            &queue,
            renderer,
            &target,
            &[&floor],
            &[],
            &[],
            &[&floor, unrelated],
        )?;
        Ok(i32::from(pixels[(64 * 128 + 64) * 4 + 1]))
    };
    let mut baseline = 0;
    for _ in 0..6 {
        baseline = frame(&mut renderer, &unrelated)?;
    }
    assert!(baseline > 80, "warmed lamp illumination: {baseline}");
    unrelated.hide_indices(std::slice::from_ref(&(0..36)));
    for refresh in 0..6 {
        let value = frame(&mut renderer, &unrelated)?;
        assert!(
            (value - baseline).abs() <= 3,
            "geometry refresh frame {refresh}: {value}, warmed {baseline}"
        );
    }
    // Rebinding creates a new source buffer and discards the old projectors.
    // Its unavailable cube sentinel must mean temporarily unshadowed lamps.
    renderer.set_dynamic_lights(&device, &queue, &lights)?;
    for warmup in 0..6 {
        let value = frame(&mut renderer, &unrelated)?;
        assert!(value > 80, "new source warmup frame {warmup}: {value}");
    }
    Ok(())
}

/// Poisoned legacy sun masks cannot shadow Dynamic. Current geometry does,
/// on both originally bright and originally dark texels. Night is live too.
#[test]
fn dynamic_sun_ignores_baked_masks_and_uses_current_geometry() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let settings = ShadowSettings {
        light_cubes: true,
        ..ShadowSettings::LOW
    };
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
    let mut lighting = lamp_lighting(Vec3::new(0.0, 12.0, 0.0));
    lighting.lights.clear();
    for texel in &mut lighting.visibility.texels {
        *texel = [255, 0, 0, 0, 0, 0, 0, 0];
    }
    renderer.set_map_lighting(&device, &queue, Some(&lighting), true)?;
    let sun = Vec3::new(0.0, -1.0, 0.0);
    let floor_data = dynamic_floor(
        sun,
        |x| [51, 51, 51, if x < 8 { 255 } else { 0 }],
        &[],
        |_, _| 0.0,
    );
    let floor = renderer.upload(&device, &queue, &floor_data)?;
    let slab = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-8.0, 4.0, -6.0), Vec3::new(-4.0, 4.3, -2.0)),
    )?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.6, 0.6, 0.6, 0.];
    camera.ambient = [0.2, 0.2, 0.2, 3.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // The baked sunlit half, the baked shaded half, and under the slab on
    // the sunlit half.
    let points = [
        Vec3::new(-6.0, 0.0, 6.0),
        Vec3::new(6.0, 0.0, 6.0),
        Vec3::new(-6.0, 0.0, -4.0),
    ];
    renderer.update_camera(&queue, &camera);
    let pixels = render_with_map(
        &device,
        &queue,
        &mut renderer,
        &target,
        &[&floor],
        &[&slab],
        &[],
        &[&floor],
    )?;
    let [sunlit, shaded, under] = points.map(|p| at(&pixels, p));
    // Live ambient 0.2 plus the current sun's 0.6 on both halves; ambient
    // alone under the current brick shadow.
    assert!(sunlit >= 200, "{sunlit}");
    assert!(
        (shaded - sunlit).abs() <= 3,
        "legacy dark half is live: {shaded}/{sunlit}"
    );
    assert!((under - 51).abs() <= 4, "{under}");
    let mut poison = floor_data.clone();
    for image in poison.images.iter_mut().skip(1) {
        for texel in image.rgba.chunks_exact_mut(4) {
            texel.copy_from_slice(&[255, 0, 255, 255]);
        }
    }
    let poisoned = renderer.upload(&device, &queue, &poison)?;
    renderer.update_camera(&queue, &camera);
    camera.baked_sun_direction = [1.0, 0.0, 0.0, 1.0];
    camera.baked_sun_color = [50.0; 4];
    camera.baked_ambient = [50.0; 4];
    renderer.update_camera(&queue, &camera);
    let after = render_with_map(
        &device,
        &queue,
        &mut renderer,
        &target,
        &[&poisoned],
        &[&slab],
        &[],
        &[&poisoned],
    )?;
    assert_eq!(
        pixels, after,
        "all baked illumination/visibility bytes are irrelevant"
    );
    camera.sun_color = [0.; 4];
    renderer.update_camera(&queue, &camera);
    let night = render_with_map(
        &device,
        &queue,
        &mut renderer,
        &target,
        &[&poisoned],
        &[&slab],
        &[],
        &[&poisoned],
    )?;
    for p in points {
        assert!((at(&night, p) - 51).abs() <= 3);
    }
    Ok(())
}
