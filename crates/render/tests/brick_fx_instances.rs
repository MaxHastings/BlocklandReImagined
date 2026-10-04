//! Brick FX follow the model matrix like positions do: a chrome brick drawn
//! at the origin and moved by an instance transform (falling debris) looks
//! the same as that brick built in place.
use anyhow::Result;
use bri_render::scene::*;
use glam::{Mat4, Vec3};

/// Axis-aligned box with outward normals, one white vertex-lit material,
/// every vertex carrying `fx`.
fn cuboid(min: Vec3, max: Vec3, fx: [f32; 4]) -> SceneData {
    let mut data = SceneData::default();
    data.materials.push(Material::vertex_lit("white", 0));
    let faces: [(Vec3, [Vec3; 4]); 6] = [
        (
            Vec3::Y,
            [
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(max.x, max.y, min.z),
            ],
        ),
        (
            Vec3::NEG_Y,
            [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(max.x, min.y, max.z),
                Vec3::new(min.x, min.y, max.z),
            ],
        ),
        (
            Vec3::X,
            [
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(max.x, min.y, max.z),
            ],
        ),
        (
            Vec3::NEG_X,
            [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(min.x, max.y, min.z),
            ],
        ),
        (
            Vec3::Z,
            [
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(max.x, min.y, max.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(min.x, max.y, max.z),
            ],
        ),
        (
            Vec3::NEG_Z,
            [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(max.x, min.y, min.z),
            ],
        ),
    ];
    for (normal, corners) in faces {
        let base = data.vertices.len() as u32;
        data.vertices.extend(corners.map(|p| SceneVertex {
            position: p.to_array(),
            normal: normal.to_array(),
            uv: [0.0; 2],
            lightmap_uv: [0.0; 2],
            color: [1.0; 4],
            fx,
        }));
        data.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    data.batches.push(MeshBatch {
        indices: 0..data.indices.len() as u32,
        material: 0,
        center: ((min + max) * 0.5).to_array(),
    });
    data
}

#[test]
fn chrome_on_a_moved_instance_matches_the_brick_built_in_place() -> Result<()> {
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
    let chrome = BrickFx::new(2, 0)?;
    let half = Vec3::splat(0.5);
    let at = Vec3::new(40.0, 3.0, -30.0);
    let in_place = renderer.upload(
        &device,
        &queue,
        &cuboid(at - half, at + half, chrome.encode(at.to_array(), 0, 2)?),
    )?;
    let at_origin = renderer.upload(
        &device,
        &queue,
        &cuboid(-half, half, chrome.encode([0.0; 3], 0, 2)?),
    )?;
    let mut moved = GpuInstances::new(&device, 1)?;
    moved.update(
        &queue,
        &[SceneTransform {
            transform: Mat4::from_translation(at),
            ..Default::default()
        }],
    )?;
    let mut camera = Camera::perspective(
        (at + Vec3::new(1.5, 1.2, 2.0)).to_array(),
        at.to_array(),
        1.0,
        1.0,
        0.05,
        100.0,
    );
    camera.sun_direction = [0.3, -1.0, 0.2, 0.0];
    camera.sun_color = [0.7, 0.7, 0.7, 0.0];
    camera.ambient = [0.3, 0.3, 0.3, 0.0];
    renderer.update_camera(&queue, &camera);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("brick fx target"),
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
    let frame =
        |scenes: &[&GpuScene], instances: &[(&GpuScene, &GpuInstances)]| -> Result<Vec<u8>> {
            let mut encoder = device.create_command_encoder(&Default::default());
            renderer.render_with_instances(
                &mut encoder,
                &view,
                &depth,
                scenes,
                instances,
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
            Ok(mapped
                .chunks_exact(row as usize)
                .flat_map(|line| line[..width as usize * 4].to_vec())
                .collect())
        };
    let built = frame(&[&in_place], &[])?;
    let instanced = frame(&[], &[(&at_origin, &moved)])?;
    assert!(built.iter().any(|&v| v > 0), "the brick is on screen");
    let worst = built
        .iter()
        .zip(&instanced)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(worst <= 2, "instanced chrome differs by {worst}");
    Ok(())
}
