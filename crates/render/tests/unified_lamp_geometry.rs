//! Lamp shadows on map surfaces, instanced models and authoritative map visibility.
//! Split by behavior so serial software-GPU verification fits the per-binary watchdog.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use common::*;
use glam::{Mat4, Vec3};

/// A lamp the fit made brighter than a lightmap texel holds (0.55 fitted
/// against 0.3 baked): a live shadow from it takes only the lamp's share of
/// the fitted light plus ambient, never the texel's whole light.
#[test]
fn lamp_shadows_on_the_map_take_only_the_lamps_share() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::BEST));
    renderer.set_map_lighting(
        &device,
        &queue,
        Some(&lamp_lighting(Vec3::new(0.0, 12.0, 0.0))),
        false,
    )?;
    // Static light 0.3 everywhere and no sun colour: the map draws 0.3.
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let map = renderer.upload(&device, &queue, &floor(sun, 0.0))?;
    let slab = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0)),
    )?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
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
    renderer.update_camera(&queue, &camera);
    let pixels = render(
        &device,
        &queue,
        &mut renderer,
        &target,
        &[&map],
        &[&slab],
        &[],
    )?;
    let (shaded, lit) = (at(&pixels, shade), at(&pixels, open));
    assert!((lit - 77).abs() <= 2, "{lit}");
    // The lamp gives 0.554 of 0.654 fitted there: 0.3 * (1 - 0.554 / 0.654)
    // = 0.046 stays (12), where taking the lamp's whole claim left black.
    assert!((shaded - 12).abs() <= 4, "{shaded}");
    Ok(())
}

/// A player-sized instanced model (the way players, items and vehicles
/// draw) standing on a lightmapped floor beside a low map light, at High
/// shadows: its lamp shadow darkens the floor on the far side.
#[test]
fn an_instanced_model_casts_a_lamp_shadow_on_the_map() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::HIGH));
    // The light 6 above the floor, 12 from the model, as the Bedroom desk
    // lamp's lower light stands to a player on the dresser.
    renderer.set_map_lighting(
        &device,
        &queue,
        Some(&lamp_lighting(Vec3::new(-12.0, 6.0, 0.0))),
        false,
    )?;
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let map = renderer.upload(&device, &queue, &floor(sun, 0.0))?;
    let body = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-0.5, 0.0, -0.3), Vec3::new(0.5, 2.5, 0.3)),
    )?;
    let mut instances = GpuInstances::new(&device, 1)?;
    instances.update(&queue, &[SceneTransform::default()])?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.0];
    let target = color_target(&device, format, width, height);
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, width, height).create_view(&Default::default());
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    renderer.update_camera(&queue, &camera);
    let mut encoder = device.create_command_encoder(&Default::default());
    let models = [(&body, &instances)];
    renderer.render_shadows(
        &mut encoder,
        ShadowCasters {
            scenes: &[],
            instances: &models,
        },
        ShadowCasters {
            scenes: &[],
            instances: &[],
        },
    );
    renderer.render_with_instances(
        &mut encoder,
        &view,
        &depth,
        &[&map],
        &[],
        Some(wgpu::Color::BLACK),
    );
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
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    let pixels = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?
        .to_vec();
    let at = |point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[y * row as usize + x * 4 + 1])
    };
    // Past the model, away from the light, and the same distance off to the side.
    let (shaded, open) = (at(Vec3::new(2.5, 0.0, 0.0)), at(Vec3::new(-2.0, 0.0, 6.0)));
    assert!(open > 60, "{open}");
    assert!(shaded < open - 20, "{shaded} {open}");
    Ok(())
}

/// Beside furniture the visibility volume's few-unit cells can sit inside
/// the map and hide a lamp from the surfaces next to it, or show it through
/// a wall. A lamp with a shadow slot reads the map's own walls from its map
/// faces instead: under a slab its shadow shows where the volume hid the
/// lamp, and behind a wall, where the volume showed it, a slab takes away
/// no light the lamp never gave.
#[test]
fn shadowed_lamps_reach_past_the_map_walls_not_the_coarse_volume() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::BEST));
    // The volume hides the lamp everywhere left of x = 4 and shows it past.
    let mut lighting = lamp_lighting(Vec3::new(0.0, 12.0, 0.0));
    let dx = lighting.visibility.dims[0];
    for (i, texel) in lighting.visibility.texels.iter_mut().enumerate() {
        texel[1] = if i as u32 % dx >= 9 { 255 } else { 0 };
    }
    renderer.set_map_lighting(&device, &queue, Some(&lighting), false)?;
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let floor = renderer.upload(&device, &queue, &floor(sun, 0.0))?;
    // A map wall at x = 4 the lamp cannot see past.
    let mut wall = cuboid(Vec3::new(4.0, 0.0, -10.0), Vec3::new(4.2, 6.0, 10.0));
    wall.images = vec![SceneImage::white()];
    wall.materials[0] = Material::surface("wall", 0, 0);
    let wall = renderer.upload(&device, &queue, &wall)?;
    let slab = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0)),
    )?;
    let behind = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(5.0, 2.0, -3.0), Vec3::new(8.0, 2.3, 1.0)),
    )?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
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
    let points = [
        Vec3::new(2.6, 0.0, 0.0),
        Vec3::new(-6.0, 0.0, -1.0),
        Vec3::new(6.5, 0.0, -1.0),
    ];
    let mut frames = vec![];
    for _ in 0..2 {
        renderer.update_camera(&queue, &camera);
        let pixels = render_with_map(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&floor],
            &[&slab, &behind],
            &[],
            &[&floor, &wall],
        )?;
        frames.push(points.map(|p| at(&pixels, p)));
    }
    assert_eq!(frames[0], frames[1]);
    let [shaded, open, walled] = frames[0];
    // Unshadowed the floor shows as baked, its own map face never shading it.
    assert!((open - 77).abs() <= 2, "{open}");
    // The slab's full shadow, as where the volume shows the lamp.
    assert!((shaded - 12).abs() <= 4, "{shaded}");
    // Behind the wall the slab over it removes nothing.
    assert!((walled - 77).abs() <= 2, "{walled}");
    Ok(())
}
