//! Uploaded lightmap patches, brick changes and lamp switches update immediately.
//! Split by behavior so serial software-GPU verification fits the per-binary watchdog.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{map_lighting::TexelFix, scene::*, shadow::ShadowSettings};
use common::*;
use glam::{Mat4, Vec3};

/// Lightmap leak cleanup reaches an uploaded map: patched images draw at
/// once, in every lighting mode.
#[test]
fn patched_lightmaps_draw_without_a_new_upload() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (64u32, 64u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, None);
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let mut data = floor(sun, 0.0);
    let map = renderer.upload(&device, &queue, &data)?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 0.0];
    let target = color_target(&device, format, width, height);
    let centre =
        |pixels: &[u8]| i32::from(pixels[((height / 2 * width + width / 2) * 4 + 1) as usize]);
    for mode in [0.0, 2.0] {
        camera.ambient[3] = mode;
        renderer.update_camera(&queue, &camera);
        let before = centre(&render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&map],
            &[],
            &[],
        )?);
        assert!((before - 77).abs() <= 2, "{before}");
    }
    // Every texel of the drawn lightmap and its static light down to 30.
    let fixes: Vec<TexelFix> = (0..16 * 16)
        .flat_map(|index| {
            [
                TexelFix {
                    image: 1,
                    index,
                    rgba: [30, 30, 30, 255],
                },
                TexelFix {
                    image: 2,
                    index,
                    rgba: [30, 30, 30, 0],
                },
            ]
        })
        .collect();
    let changed = TexelFix::apply(&fixes, &mut data.images);
    assert_eq!(changed, vec![1, 2]);
    map.patch_images(&queue, &data.images, &changed)?;
    for mode in [0.0, 2.0] {
        camera.ambient[3] = mode;
        renderer.update_camera(&queue, &camera);
        let after = centre(&render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&map],
            &[],
            &[],
        )?);
        assert!((after - 30).abs() <= 2, "mode {mode}: {after}");
    }
    Ok(())
}

/// A brick placed under a lamp, or taken away, shows in (or leaves) the
/// lamp's shadow the very frame it changes, not when its kept face's turn
/// to refresh comes round (pharzedia's lagging shadows on the Bedroom
/// dresser).
#[test]
fn placed_and_removed_bricks_change_lamp_shadows_the_same_frame() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let lamp = Vec3::new(0.0, 12.0, 0.0);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::BEST));
    renderer.set_map_lighting(&device, &queue, Some(&lamp_lighting(lamp)), false)?;
    let floor = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-10.0, -0.3, -10.0), Vec3::new(10.0, 0.0, 10.0)),
    )?;
    let slab_data = cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0));
    let palette = renderer.upload(&device, &queue, &slab_data)?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = [0.0, -1.0, 0.3, 0.0];
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    let shade = Vec3::new(2.6, 0.0, 0.0);
    let open = Vec3::new(6.0, 0.0, -1.0);
    // Settle with the floor alone, then place and remove the slab (a new
    // chunk each time it is placed, as a rebuilt chunk is) frame by frame:
    // six faces refresh one a frame in turn, so waiting for a turn would
    // miss most of these frames.
    for _ in 0..8 {
        renderer.update_camera(&queue, &camera);
        render(&device, &queue, &mut renderer, &target, &[&floor], &[], &[])?;
    }
    for frame in 0..8 {
        let placed = frame % 2 == 0;
        let slab = renderer.upload_chunk(&device, &queue, &slab_data, &palette)?;
        let scenes: Vec<&GpuScene> = if placed {
            vec![&floor, &slab]
        } else {
            vec![&floor]
        };
        let casters: Vec<&GpuScene> = if placed { vec![&slab] } else { vec![] };
        renderer.update_camera(&queue, &camera);
        let pixels = render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &scenes,
            &casters,
            &[],
        )?;
        let (shaded, lit) = (at(&pixels, shade), at(&pixels, open));
        if placed {
            assert!(
                shaded < lit - 40,
                "frame {frame}: placed slab casts no shadow yet ({shaded} vs {lit})"
            );
        } else {
            assert!(
                shaded >= lit,
                "frame {frame}: removed slab still shades ({shaded} vs {lit})"
            );
        }
    }
    Ok(())
}

/// A map light switched off (a broken bulb) or recoloured (an Add-On) at
/// run time: it leaves objects and its share of the map's baked light, and
/// the rest of the baked light stays. Both with a shadowed lamp (Best) and
/// without lamp shadows (Low), where only the tint path runs.
#[test]
fn switched_off_and_recoloured_map_lights_leave_the_map_and_objects() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3, channel: usize| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + channel])
    };
    let open = Vec3::new(6.0, 0.0, -1.0);
    let block_top = Vec3::new(-5.0, 0.5, 0.0);
    for settings in [ShadowSettings::BEST, ShadowSettings::LOW] {
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        renderer.set_map_lighting(
            &device,
            &queue,
            Some(&lamp_lighting(Vec3::new(0.0, 12.0, 0.0))),
            false,
        )?;
        let map = renderer.upload(&device, &queue, &floor(sun, 0.0))?;
        let block = renderer.upload(
            &device,
            &queue,
            &cuboid(Vec3::new(-6.0, 0.0, -1.0), Vec3::new(-4.0, 0.5, 1.0)),
        )?;
        let mut frame = |tint: Vec3| -> Result<Vec<u8>> {
            renderer.set_map_light_tints(&queue, &[tint]);
            renderer.update_camera(&queue, &camera);
            render(
                &device,
                &queue,
                &mut renderer,
                &target,
                &[&map, &block],
                &[&block],
                &[],
            )
        };
        let on = frame(Vec3::ONE)?;
        let off = frame(Vec3::ZERO)?;
        let red = frame(Vec3::new(1.0, 0.0, 0.0))?;
        let back = frame(Vec3::ONE)?;
        // The map: 0.3 baked, of which the lamp gives 0.531 of 0.631
        // fitted there, so 0.3 * 0.1 / 0.631 = 0.048 (12) stays when it is off.
        let (map_on, map_off) = (at(&on, open, 1), at(&off, open, 1));
        assert!((map_on - 77).abs() <= 2, "{settings:?}: {map_on}");
        assert!((map_off - 12).abs() <= 4, "{settings:?}: {map_off}");
        // Red keeps the red share and drops the rest.
        assert!(
            (at(&red, open, 0) - 77).abs() <= 2 && (at(&red, open, 1) - 12).abs() <= 4,
            "{settings:?}"
        );
        // The block loses the lamp's light and keeps the ambient.
        let (block_on, block_off) = (at(&on, block_top, 1), at(&off, block_top, 1));
        assert!(
            block_off < block_on - 40,
            "{settings:?}: {block_off} vs {block_on}"
        );
        assert!(
            at(&red, block_top, 0) > at(&red, block_top, 1) + 40,
            "{settings:?}"
        );
        // Switched back, it draws as before.
        assert_eq!(at(&back, open, 1), map_on, "{settings:?}");
        assert_eq!(at(&back, block_top, 1), block_on, "{settings:?}");
    }
    Ok(())
}

/// A map light switched off (a broken bulb) leaves the same light in the
/// Unified mode, from the bake's per-texel shares: here a
/// light without a visibility channel (which the Unified modes could not
/// switch before), giving 64 levels of the floor's 77. Off, the floor keeps
/// its 13 in Unified; on, it shows as baked.
#[test]
fn unified_switchable_light_preserves_its_original_per_texel_output() -> Result<()> {
    use bri_render::map_lighting::MapLight;
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (128u32, 128u32);
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let floor_data = dynamic_floor(sun, |_| [13, 13, 13, 0], &[0], |_, _| 1.0);
    let mut lighting = lamp_lighting(Vec3::new(0.0, 12.0, 0.0));
    lighting.lights = vec![MapLight {
        position: [0.0, 12.0, 0.0],
        color: [64.0 / 255.0; 3],
        inner: 1000.0,
        outer: 2000.0,
        channel: None,
    }];
    let target = color_target(&device, format, width, height);
    {
        let mode = 2.0;
        let mut renderer =
            SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::BEST));
        renderer.set_map_lighting(&device, &queue, Some(&lighting), false)?;
        let floor = renderer.upload(&device, &queue, &floor_data)?;
        let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
        camera.sun_direction = sun.extend(0.0).to_array();
        camera.sun_color = [0.0; 4];
        camera.ambient = [0.0, 0.0, 0.0, mode];
        let mut frame = |tint: Vec3| -> Result<i32> {
            renderer.set_map_light_tints(&queue, &[tint]);
            renderer.update_camera(&queue, &camera);
            let pixels = render(&device, &queue, &mut renderer, &target, &[&floor], &[], &[])?;
            Ok(i32::from(
                pixels[((height / 2) * width + width / 2) as usize * 4 + 1],
            ))
        };
        let on = frame(Vec3::ONE)?;
        let off = frame(Vec3::ZERO)?;
        assert!((on - 77).abs() <= 2, "mode {mode}: on {on}");
        assert!((off - 13).abs() <= 2, "mode {mode}: off {off}");
    }
    Ok(())
}
