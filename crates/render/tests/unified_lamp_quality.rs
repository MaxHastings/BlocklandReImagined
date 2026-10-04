//! Lamp shadow quality, lighting modes and moving versus kept casters.
//! Split by behavior so serial software-GPU verification fits the per-binary watchdog.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use common::*;
use glam::{Mat4, Vec3};

#[test]
fn map_lamps_cast_live_shadows_in_unified_modes_by_shadow_quality() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let lamp = Vec3::new(0.0, 12.0, 0.0);
    let floor = cuboid(Vec3::new(-10.0, -0.3, -10.0), Vec3::new(10.0, 0.0, 10.0));
    // A slab under the lamp, over the middle of the floor.
    let slab = cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0));
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = [0.0, -1.0, 0.3, 0.0];
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 0.];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // The camera sees the floor from above past the slab's edge; the
    // overhead lamp throws the slab's shadow wider, to |x| < 2 * 12 / 8 = 3.
    let shade = Vec3::new(2.6, 0.0, 0.0);
    let open = Vec3::new(6.0, 0.0, -1.0);
    // The slab as a moving caster (drawn into the lamp every frame), or as
    // a brick chunk (kept lamp faces), over three frames.
    let mut run_as = |settings: ShadowSettings, mode: f32, chunk: bool| -> Result<[i32; 2]> {
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        renderer.set_map_lighting(&device, &queue, Some(&lamp_lighting(lamp)), false)?;
        let floor = renderer.upload(&device, &queue, &floor)?;
        let slab = if chunk {
            let palette = renderer.upload(&device, &queue, &slab)?;
            renderer.upload_chunk(&device, &queue, &slab, &palette)?
        } else {
            renderer.upload(&device, &queue, &slab)?
        };
        camera.ambient[3] = mode;
        let mut frames = vec![];
        for _ in 0..3 {
            renderer.update_camera(&queue, &camera);
            let pixels = render(
                &device,
                &queue,
                &mut renderer,
                &target,
                &[&floor, &slab],
                &[&slab],
                &[],
            )?;
            frames.push([at(&pixels, shade), at(&pixels, open)]);
        }
        assert!(frames.windows(2).all(|w| w[0] == w[1]), "{frames:?}");
        Ok(frames[0])
    };
    let mut run = |settings: ShadowSettings, mode: f32| -> Result<[i32; 2]> {
        let moving = run_as(settings, mode, false)?;
        let kept = run_as(settings, mode, true)?;
        assert!(
            (moving[0] - kept[0]).abs() <= 12 && moving[1] == kept[1],
            "{moving:?} {kept:?}"
        );
        Ok(kept)
    };
    // Unified at Best: the slab shades the floor the lamp lights.
    let [shaded, open_floor] = run(ShadowSettings::BEST, 2.0)?;
    assert!(open_floor > 60, "{open_floor}");
    assert!(shaded < open_floor - 40, "{shaded} {open_floor}");
    // Medium casts from one lamp too.
    let [medium, _] = run(ShadowSettings::MEDIUM, 2.0)?;
    assert!(medium < open_floor - 40, "{medium} {open_floor}");
    // Low quality has no lamp shadows; the floor there, nearer the lamp, is
    // lit at least as brightly as the open floor.
    let [low, low_open] = run(ShadowSettings::LOW, 2.0)?;
    assert!(low >= low_open, "{low} {low_open}");
    // Classic never draws map lights or lamp shadows.
    let [classic, classic_open] = run(ShadowSettings::BEST, 0.0)?;
    assert!(
        (classic - classic_open).abs() <= 2,
        "{classic} {classic_open}"
    );
    Ok(())
}
