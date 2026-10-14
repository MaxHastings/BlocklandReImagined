//! A map spot light lights only its cone in the Dynamic mode.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use common::*;
use glam::Vec3;

#[test]
fn dynamic_spot_lights_only_its_cone() -> Result<()> {
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
    let lights = vec![MapLight {
        position: [0.0, 12.0, 0.0],
        color: [0.8; 3],
        inner: 0.0,
        outer: 40.0,
        channel: None,
    }];
    let floor = renderer.upload(&device, &queue, &floor(Vec3::NEG_Y, 0.0))?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = Vec3::NEG_Y.extend(0.0).to_array();
    camera.sun_color = [0.; 4];
    camera.ambient = [0., 0., 0., 3.];
    // The floor under the light, and about 9 units to the side: 37
    // degrees out from straight down.
    let frame = |renderer: &mut SceneRenderer| -> Result<(i32, i32)> {
        let mut out = (0, 0);
        for _ in 0..6 {
            renderer.update_camera(&queue, &camera);
            let pixels = render_with_map(
                &device,
                &queue,
                renderer,
                &target,
                &[&floor],
                &[],
                &[],
                &[&floor],
            )?;
            let green = |x: usize| i32::from(pixels[(64 * 128 + x) * 4 + 1]);
            out = (green(64), green(96));
        }
        Ok(out)
    };
    renderer.set_dynamic_lights(&device, &queue, &lights)?;
    let (_, side_all_round) = frame(&mut renderer)?;
    renderer.set_map_light_spots(&queue, &[(0, spot_words(Vec3::NEG_Y, 10.0, 20.0))]);
    let (under, side) = frame(&mut renderer)?;
    assert!(
        side_all_round > 40,
        "an omni light reaches the side: {side_all_round}"
    );
    assert!(under > 80, "the cone lights the floor under it: {under}");
    assert!(
        side < side_all_round / 4,
        "outside the cone stays dark: {side}"
    );
    Ok(())
}
