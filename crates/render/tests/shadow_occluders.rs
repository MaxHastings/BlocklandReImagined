//! A player standing on a brick tower shades the tower top, not the floor
//! beneath it: non-casting surfaces still stop shadows (occluder map).
use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use glam::{Mat4, Vec3};

/// Axis-aligned box with outward normals, one white vertex-lit material.
fn cuboid(min: Vec3, max: Vec3) -> SceneData {
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
            fx: [0.0; 4],
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
fn shadows_land_only_on_the_first_surface_they_reach() -> Result<()> {
    let (device, queue) = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await?;
        anyhow::Ok(adapter.request_device(&Default::default()).await?)
    })?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let floor = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-10., -0.5, -10.), Vec3::new(10., 0., 10.)),
    )?;
    let tower = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-1., 0., -1.), Vec3::new(1., 4., 1.)),
    )?;
    let player = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-0.3, 4.05, -0.3), Vec3::new(0.3, 5., 0.3)),
    )?;
    let sun = Vec3::new(0.3, -1.0, 0.0);
    let mut camera = Camera::perspective(
        [8., 7., 6.],
        [1., 1.5, 0.],
        width as f32 / height as f32,
        1.0,
        0.05,
        400.0,
    );
    camera.sun_direction = sun.extend(0.).to_array();
    camera.sun_color = [0.7, 0.7, 0.7, 0.];
    camera.ambient = [0.3, 0.3, 0.3, 0.];
    renderer.update_camera(&queue, &camera);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow occluder target"),
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
    let frame = |casters: &[&GpuScene], occluders: &[&GpuScene]| -> Result<Vec<u8>> {
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render_shadows(
            &mut encoder,
            ShadowCasters {
                scenes: casters,
                instances: &[],
            },
            ShadowCasters {
                scenes: occluders,
                instances: &[],
            },
        );
        renderer.render(
            &mut encoder,
            &view,
            &depth,
            &[&floor, &tower, &player],
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
    // Luminance of the pixel showing a world point.
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // The player's shadow along the sun: on the tower top, and where it
    // would reach the floor if the tower did not stop it.
    let tower_top = Vec3::new(0.15, 4.0, 0.0);
    let floor_below = Vec3::new(1.35, 0.0, 0.0);

    let unshadowed = frame(&[], &[&floor, &tower])?;
    let fixed = frame(&[&player], &[&floor, &tower])?;
    let see_through = frame(&[&player], &[])?;
    let lit_top = at(&unshadowed, tower_top);
    let lit_floor = at(&unshadowed, floor_below);
    assert!(
        at(&fixed, tower_top) < lit_top - 20,
        "player shades the tower top"
    );
    assert!(
        (at(&fixed, floor_below) - lit_floor).abs() <= 3,
        "tower stops the shadow"
    );
    assert!(
        at(&see_through, floor_below) < lit_floor - 20,
        "without occluders the shadow would pass through (test sensitivity)"
    );
    // Occluders never cast shadows of their own.
    assert_eq!(unshadowed, frame(&[], &[])?);
    Ok(())
}
