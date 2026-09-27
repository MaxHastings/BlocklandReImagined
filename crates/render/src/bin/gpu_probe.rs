use anyhow::{Context, Result, ensure};
use std::{fs::File, io::BufWriter, path::PathBuf, time::Instant};

fn main() -> Result<()> {
    pollster::block_on(run())
}

async fn run() -> Result<()> {
    let destination = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or("artifacts/preflight".into()),
    );
    std::fs::create_dir_all(&destination)?;
    let started = Instant::now();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        })
        .await
        .context("No headless GPU adapter available")?;
    let info = adapter.get_info();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("BRI offscreen validation"),
            ..Default::default()
        })
        .await?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("probe triangle"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../probe.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("probe"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    const SIZE: u32 = 256;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (SIZE * SIZE * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("probe"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.02,
                        g: 0.03,
                        b: 0.04,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::from_secs(30)),
    })?;
    receive.recv_timeout(std::time::Duration::from_secs(30))??;
    let pixels = buffer.slice(..).get_mapped_range()?;
    let center = &pixels[((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize..][..4];
    ensure!(
        center[0] > 180 && center[1] > 50 && center[2] < 40 && center[3] == 255,
        "Triangle did not render: {center:?}"
    );
    ensure!(
        pixels[0] < 10 && pixels[1] < 10 && pixels[3] == 255,
        "Background pixel mismatch"
    );
    let mut png = png::Encoder::new(
        BufWriter::new(File::create(destination.join("gpu-probe.png"))?),
        SIZE,
        SIZE,
    );
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()?.write_image_data(&pixels)?;
    let report = serde_json::json!({
        "status": "passed", "adapter": info.name, "backend": format!("{:?}", info.backend),
        "driver": info.driver, "driver_info": info.driver_info, "center_rgba": center,
        "elapsed_ms": started.elapsed().as_millis(), "visible_window_created": false,
        "checks": ["WGSL pipeline", "offscreen triangle", "GPU readback", "foreground/background pixels"],
        "scope": "dependency integration only; not game rendering acceptance"
    });
    std::fs::write(
        destination.join("gpu-probe.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
