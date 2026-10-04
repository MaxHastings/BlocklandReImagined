//! Per-instance clip planes: a body part way through a portal draws twice,
//! each copy cut at the opening, so half of it shows on either side.
use anyhow::Result;
use bri_render::scene::*;
use glam::{Mat4, Vec3};

/// A white unlit-ish square in the z = 0 plane facing +Z.
fn square(half: f32) -> SceneData {
    let mut data = SceneData::default();
    data.materials.push(Material::vertex_lit("white", 0));
    let corners = [
        Vec3::new(-half, -half, 0.0),
        Vec3::new(half, -half, 0.0),
        Vec3::new(half, half, 0.0),
        Vec3::new(-half, half, 0.0),
    ];
    data.vertices.extend(corners.map(|p| SceneVertex {
        position: p.to_array(),
        normal: [0.0, 0.0, 1.0],
        uv: [0.0; 2],
        lightmap_uv: [0.0; 2],
        color: [1.0; 4],
        fx: [0.0; 4],
    }));
    data.indices.extend([0, 1, 2, 0, 2, 3]);
    data.batches.push(MeshBatch {
        indices: 0..6,
        material: 0,
        center: [0.0; 3],
    });
    data
}

#[test]
fn a_cut_instance_draws_only_its_side_and_two_cut_copies_make_it_whole() -> Result<()> {
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
    let (width, height) = (64u32, 64u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, None);
    let scene = renderer.upload(&device, &queue, &square(1.0))?;
    let mut camera = Camera::perspective([0.0, 0.0, 3.0], [0.0; 3], 1.0, 1.0, 0.05, 100.0);
    camera.ambient = [1.0, 1.0, 1.0, 0.0];
    renderer.update_camera(&queue, &camera);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("clip plane target"),
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
    let frame = |transforms: &[SceneTransform], clips: &[ClipPlane]| -> Result<Vec<u8>> {
        let mut instances = GpuInstances::new(&device, 2)?;
        instances.update_clipped(&queue, transforms, clips)?;
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render_with_instances(
            &mut encoder,
            &view,
            &depth,
            &[],
            &[(&scene, &instances)],
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
    let lit = |image: &[u8], x: u32, y: u32| image[((y * width + x) * 4) as usize] > 0;
    let at = SceneTransform::default();
    let whole = frame(&[at], &[])?;
    assert!(
        lit(&whole, 20, 32) && lit(&whole, 44, 32),
        "the square fills the middle"
    );
    // Kept where x >= 0: the right half only.
    let right = frame(&[at], &[[1.0, 0.0, 0.0, 0.0]])?;
    assert!(
        !lit(&right, 20, 32) && lit(&right, 44, 32),
        "only the right half draws"
    );
    // The cut half drawn by a second copy (as a portal's partner would,
    // here in the same place): the two halves meet with no gap.
    let both = frame(&[at, at], &[[1.0, 0.0, 0.0, 0.0], [-1.0, 0.0, 0.0, 0.0]])?;
    let worst = whole
        .iter()
        .zip(&both)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(
        worst <= 2,
        "two cut halves differ from the whole by {worst}"
    );
    // A copy moved elsewhere carries its own cut with it.
    let moved = SceneTransform {
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -50.0)),
        ..at
    };
    let far = frame(
        &[at, moved],
        &[[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, -1.0, -40.0]],
    )?;
    assert!(
        lit(&far, 31, 32),
        "the far copy, beyond its cut, shows past the cut half"
    );
    let gone = frame(
        &[at, moved],
        &[[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, -1.0, -60.0]],
    )?;
    assert!(!lit(&gone, 31, 32), "the far copy, cut away, draws nothing");
    assert!(
        GpuInstances::new(&device, 2)?
            .update_clipped(&queue, &[at, at], &[KEEP_ALL])
            .is_err(),
        "one clip plane per instance"
    );
    Ok(())
}
