#[macro_use]
mod common;
use anyhow::Result;
use bri_foliage::*;
use common::*;
use glam::Vec3;
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    name: String,
}
impl Gpu {
    fn new() -> Result<Self> {
        pollster::block_on(async {
            let i = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let a = i
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await?;
            let (d, q) = a.request_device(&wgpu::DeviceDescriptor::default()).await?;
            Ok(Self {
                device: d,
                queue: q,
                name: a.get_info().name,
            })
        })
    }
    fn frame(&self, r: &FoliageRenderer, clear_depth: f32) -> Result<Vec<u8>> {
        let size = wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        };
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let dv = depth.create_view(&Default::default());
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * 256 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut e = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = e.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &dv,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_depth),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            r.render(&mut pass);
        }
        e.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(1024),
                    rows_per_image: Some(256),
                },
            },
            size,
        );
        self.queue.submit([e.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        rx.recv_timeout(std::time::Duration::from_secs(30))??;
        let bytes = read.slice(..).get_mapped_range()?.to_vec();
        Ok(bytes)
    }
}
/// Places every definition against the fixture's world, renders the field
/// offscreen and checks sway, depth occlusion and upload bounds. Returns the
/// first frame and a probe report.
fn offscreen_gpu_sway_depth_and_upload_bounds(f: &Fixture) -> Result<(Vec<u8>, serde_json::Value)> {
    let p = &f.pack;
    let images = f.images();
    let w = f.world();
    let mut fields = vec![];
    for d in &p.definitions {
        let mut b = PlacementBuilder::new(d.clone())?;
        while !b.stats().completed {
            b.advance(4096, |r| trace(&w, r))?;
        }
        fields.push(b.finish()?);
    }
    let total: usize = fields.iter().map(|f| f.plants().len()).sum();
    let target = fields[0].plants()[0].position + Vec3::Y * 2.;
    let c = camera(target + Vec3::new(0., 4., 14.), target);
    let gpu = Gpu::new()?;
    let mut r = FoliageRenderer::new(
        &gpu.device,
        &gpu.queue,
        p,
        &images,
        fields,
        RenderConfig {
            target: wgpu::TextureFormat::Rgba8UnormSrgb,
            depth: wgpu::TextureFormat::Depth32Float,
            samples: 1,
        },
    )?;
    let stats = r.prepare(&gpu.queue, &c, 0., 500., 900.)?;
    assert!(stats.visible > 0 && stats.visible < total, "{stats:?}");
    assert_eq!(stats.upload_bytes, 128 + stats.visible * 4);
    assert!(stats.draw_calls <= p.definitions.len());
    let first = gpu.frame(&r, bri_render::scene::DEPTH_CLEAR)?;
    r.set_illumination([0.06, 0.08, 0.12])?;
    r.prepare(&gpu.queue, &c, 0., 500., 900.)?;
    let night = gpu.frame(&r, bri_render::scene::DEPTH_CLEAR)?;
    let brightness = |pixels: &[u8]| {
        pixels
            .chunks_exact(4)
            .map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2]))
            .sum::<u64>()
    };
    assert!(
        brightness(&night) * 3 < brightness(&first),
        "plants must darken at night"
    );
    r.set_illumination([1.0; 3])?;
    let colored = first
        .chunks_exact(4)
        .filter(|p| p[0] > 0 || p[1] > 0 || p[2] > 0)
        .count();
    assert!(colored > 100);
    r.prepare(&gpu.queue, &c, 4., 500., 900.)?;
    let second = gpu.frame(&r, bri_render::scene::DEPTH_CLEAR)?;
    assert_ne!(first, second);
    let occluded = gpu.frame(&r, bri_render::scene::NEAR_DEPTH)?;
    assert!(
        occluded
            .chunks_exact(4)
            .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0)
    );
    let previous = r.stats().visible;
    assert!(r.prepare(&gpu.queue, &c, f32::NAN, 0., 100.).is_err());
    assert_eq!(r.stats().visible, previous);
    let long = r.prepare_elapsed(&gpu.queue, &c, 604800.25, 500., 900.)?;
    assert_eq!(long.phase_rebases, 1);
    assert!(long.upload_bytes > long.resident_instance_bytes);
    assert!(
        gpu.frame(&r, bri_render::scene::DEPTH_CLEAR)?
            .chunks_exact(4)
            .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0)
    );
    let next = r.prepare_elapsed(&gpu.queue, &c, 604800.5, 500., 900.)?;
    assert_eq!(next.phase_rebases, 1);
    assert_eq!(next.upload_bytes, 128 + next.visible * 4);
    let start = std::time::Instant::now();
    for n in 0..200 {
        r.prepare(&gpu.queue, &c, n as f32 / 60., 500., 900.)?;
    }
    let micros = start.elapsed().as_secs_f64() * 1e6 / 200.;
    let report = serde_json::json!({"schema_version":1,"adapter":gpu.name,"stats":stats,"mean_prepare_microseconds_debug":micros,"colored_pixels":colored,"sway_light_changes_pixels":true,"depth_occlusion_passed":true,"offscreen_only":true,"native_terrain_static_collision":true,"not_subjective_acceptance":true});
    Ok((first, report))
}
#[test]
fn offscreen_gpu_sway_depth_and_upload_bounds_synthetic() -> Result<()> {
    offscreen_gpu_sway_depth_and_upload_bounds(&Fixture::synthetic())?;
    Ok(())
}
#[test]
#[ignore = "requires generated v20 content"]
fn original_native_foliage_offscreen_gpu_sway_depth_and_upload_bounds() -> Result<()> {
    let (first, report) = offscreen_gpu_sway_depth_and_upload_bounds(&Fixture::content())?;
    image::save_buffer(
        root().join("artifacts/native-foliage/offscreen.png"),
        &first,
        256,
        256,
        image::ColorType::Rgba8,
    )?;
    std::fs::write(
        root().join("artifacts/native-foliage/gpu-probe.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}
