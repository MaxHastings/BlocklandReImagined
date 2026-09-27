//! Offscreen snapshot of a native map from its spawn, rendered with the game's
//! scene renderer into a PNG. No window, input or original-file access.
//!
//! Usage: scene_snapshot <map-bundle-dir> <map-id> <out.png> [yaw-degrees] [pitch-degrees]
use anyhow::{Context, Result, ensure};
use bri_render::{
    scene::{Camera, SceneRenderer, create_depth},
    scene_loader::load_map_bundle,
};
use glam::Vec3;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        (3..=5).contains(&args.len()),
        "Usage: scene_snapshot <map-bundle-dir> <map-id> <out.png> [yaw-degrees] [pitch-degrees]"
    );
    let yaw = args
        .get(3)
        .map_or(Ok(0.0), |v| v.parse::<f32>())?
        .to_radians();
    let pitch = args
        .get(4)
        .map_or(Ok(-20.0), |v| v.parse::<f32>())?
        .to_radians();
    let scene = load_map_bundle(&PathBuf::from(&args[0]), &args[1])?.scene;
    let eye = Vec3::from(scene.spawn) + Vec3::Y * 2.4;
    let forward = Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    );

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .context("No headless GPU adapter")?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let (width, height) = (1280u32, 720u32);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snapshot"),
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
    let mut renderer = SceneRenderer::new(&device, format);
    let gpu = renderer.upload(&device, &queue, &scene)?;
    let mut camera = Camera::perspective(
        eye.to_array(),
        (eye + forward).to_array(),
        width as f32 / height as f32,
        90f32.to_radians(),
        0.05,
        4000.0,
    );
    camera.apply_environment(&scene);
    renderer.update_camera(&queue, &camera);
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render(
        &mut encoder,
        &view,
        &depth,
        &[&gpu],
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
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
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
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..width as usize * 4]);
    }
    image::save_buffer(&args[2], &pixels, width, height, image::ColorType::Rgba8)?;
    println!("Wrote {}", args[2]);
    Ok(())
}
