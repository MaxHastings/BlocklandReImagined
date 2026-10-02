//! Surfaces a millimetre apart stay apart far away: a mirror over its
//! brick, a model's layered faces. With forward depth and the game's 0.05
//! near plane they fought from about 30 units out; reversed depth keeps
//! them apart across the whole view.
use anyhow::Result;
use bri_render::scene::*;
use glam::{Mat4, Vec3};

/// The game's near and far planes (client `FAR_PLANE`).
const NEAR: f32 = 0.05;
const FAR: f32 = 4000.0;

#[test]
fn a_millimetre_keeps_its_order_at_every_distance() {
    let camera = Camera::perspective([0.0; 3], [0.0, 0.0, -1.0], 16.0 / 9.0, 1.2, NEAR, FAR);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let depth = |d: f32| view_projection.project_point3(Vec3::new(0.0, 0.0, -d)).z;
    for d in [1.0, 10.0, 30.0, 100.0, 300.0, 1000.0] {
        let (near, far) = (depth(d), depth(d + 0.001));
        assert!((0.0..=1.0).contains(&near), "{d}: {near}");
        // Nearer is larger, by several float steps of the depth buffer.
        let steps = (near - far) / (near * f32::EPSILON);
        assert!(steps >= 4.0, "{d}: {near} vs {far} is {steps} steps");
    }
    assert!((depth(NEAR) - NEAR_DEPTH).abs() < 1e-6);
    assert!((depth(FAR) - FAR_DEPTH).abs() < 1e-6);
}

/// A flat square at height `y`, facing up, in one colour.
fn floor(y: f32, half: f32, color: [f32; 4]) -> SceneData {
    let mut data = SceneData::default();
    data.materials.push(Material::vertex_lit("floor", 0));
    let corners = [
        Vec3::new(-half, y, -half),
        Vec3::new(-half, y, half),
        Vec3::new(half, y, half),
        Vec3::new(half, y, -half),
    ];
    data.vertices.extend(corners.map(|p| SceneVertex {
        position: p.to_array(),
        normal: [0.0, 1.0, 0.0],
        uv: [0.0; 2],
        lightmap_uv: [0.0; 2],
        color,
        fx: [0.0; 4],
    }));
    data.indices.extend([0, 1, 2, 0, 2, 3]);
    data.batches.push(MeshBatch {
        indices: 0..6,
        material: 0,
        center: [0.0, y, 0.0],
    });
    data
}

#[test]
fn a_surface_a_millimetre_above_another_covers_it_far_away() -> Result<()> {
    let (device, queue) = pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await?;
        anyhow::Ok(adapter.request_device(&Default::default()).await?)
    })?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let (width, height) = (128u32, 128u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, None);
    // Red below, green a millimetre above, drawn red last so a tie or a
    // lost millimetre shows red.
    let green = renderer.upload(&device, &queue, &floor(0.001, 400.0, [0.0, 1.0, 0.0, 1.0]))?;
    let red = renderer.upload(&device, &queue, &floor(0.0, 400.0, [1.0, 0.0, 0.0, 1.0]))?;
    // Looking down at a slant onto the floor 150 to 350 units away.
    let mut camera = Camera::perspective([0.0, 60.0, 0.0], [0.0, 0.0, -250.0], 1.0, 0.8, NEAR, FAR);
    camera.sun_direction = [0.0, -1.0, 0.0, 0.0];
    camera.sun_color = [0.5, 0.5, 0.5, 0.0];
    camera.ambient = [0.5, 0.5, 0.5, 0.0];
    renderer.update_camera(&queue, &camera);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth precision target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, width, height).create_view(&Default::default());
    let row = (width * 4).div_ceil(256) * 256;
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render_with_instances(
        &mut encoder,
        &view,
        &depth,
        &[&green, &red],
        &[],
        Some(wgpu::Color::BLACK),
    );
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
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
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let (mut floor_pixels, mut red_pixels) = (0, 0);
    for line in mapped.chunks_exact(row as usize) {
        for pixel in line[..width as usize * 4].chunks_exact(4) {
            if pixel[0] > 8 || pixel[1] > 8 {
                floor_pixels += 1;
                if pixel[0] > pixel[1] {
                    red_pixels += 1;
                }
            }
        }
    }
    assert!(
        floor_pixels > 4000,
        "the floor fills the view: {floor_pixels}"
    );
    assert_eq!(red_pixels, 0, "the lower surface showed through");
    Ok(())
}
