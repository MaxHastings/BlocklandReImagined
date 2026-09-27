use anyhow::{Result, ensure};
use bri_fx_runtime::{gpu::EffectsRenderer, *};
use glam::Vec3;
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Instant};

fn camera() -> Camera {
    let eye = Vec3::new(0., 4., 18.);
    let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::new(0., 2., 0.), Vec3::Y);
    let inverse = view.inverse();
    Camera {
        view_projection: glam::camera::rh::proj::directx::orthographic(-8., 8., -8., 8., 0.1, 200.)
            * view,
        position: eye,
        right: inverse.x_axis.truncate(),
        up: inverse.y_axis.truncate(),
    }
}
struct Gallery<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    renderer: EffectsRenderer,
}
impl Gallery<'_> {
    fn render(
        &mut self,
        frames: &[FrameEffects],
        cols: u32,
        tile: u32,
        path: &std::path::Path,
    ) -> Result<serde_json::Value> {
        let rows = (frames.len() as u32).div_ceil(cols);
        let width = cols * tile;
        let height = rows * tile;
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("effects gallery"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("host scene depth"),
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            ..wgpu::TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            }
        });
        let cv = color.create_view(&Default::default());
        let dv = depth.create_view(&Default::default());
        let mut stats = Vec::new();
        for (i, frame) in frames.iter().enumerate() {
            stats.push(self.renderer.prepare(self.queue, &camera(), frame)?);
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("offscreen original effects"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &cv,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: if i == 0 {
                                wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.035,
                                    g: 0.045,
                                    b: 0.065,
                                    a: 1.,
                                })
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &dv,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let x = (i as u32 % cols) * tile;
                let y = (i as u32 / cols) * tile;
                pass.set_viewport(x as f32, y as f32, tile as f32, tile as f32, 0., 1.);
                pass.set_scissor_rect(x, y, tile, tile);
                self.renderer.render(&mut pass);
            }
            self.queue.submit([encoder.finish()]);
        }
        let stride = (width * 4).div_ceil(256) * 256;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gallery readback"),
            size: u64::from(stride) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            size,
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        rx.recv_timeout(std::time::Duration::from_secs(30))??;
        let mapped = readback.slice(..).get_mapped_range()?;
        let mut pixels = Vec::new();
        for row in mapped.chunks(stride as usize) {
            pixels.extend_from_slice(&row[..width as usize * 4]);
        }
        let distinct = pixels
            .chunks_exact(4)
            .filter(|p| p[0] > 90 || p[1] > 90 || p[2] > 90)
            .count();
        ensure!(
            distinct > 100,
            "Gallery has no meaningful original texture output"
        );
        image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)?;
        Ok(json!({"width":width,"height":height,"visible_bright_pixels":distinct,"frames":stats}))
    }
}
fn world(pack: &Arc<EffectsPack>, seed: u64) -> Result<EffectsWorld> {
    EffectsWorld::new(pack.clone(), EffectsLimits::default(), seed)
}
fn main() -> Result<()> {
    pollster::block_on(run())
}
async fn run() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 2,
        "Usage: offscreen_gallery <native-pack> <artifact-dir>"
    );
    std::fs::create_dir_all(&args[1])?;
    let pack = EffectsPack::load(&args[0])?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await?;
    let renderer = EffectsRenderer::new(
        &device,
        &queue,
        &pack,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Depth32Float,
        1,
        70000,
    )?;
    let mut gallery = Gallery {
        device: &device,
        queue: &queue,
        renderer,
    };
    let mut frames = Vec::new();
    let mut index = Vec::new();
    for (i, e) in pack.library.emitters.iter().enumerate() {
        let mut w = world(&pack, i as u64)?;
        w.start_emitter(
            &e.id,
            SourceTransform::default(),
            SourceOptions {
                colors: Some([[0.35, 0.7, 1., 1.]; 4]),
                ..Default::default()
            },
        )?;
        // Capture finite impact emitters near their burst; let continuous smoke build up.
        let time = if e.lifetime > 0. {
            e.lifetime.min(0.15) + 0.03
        } else {
            0.7
        };
        for _ in 0..60 {
            w.advance(time / 60., Vec3::new(0.4, 0., 0.))?;
        }
        index.push(
            json!({"index":i,"id":e.id,"name":e.name,"time":time,"particles":w.particle_count()}),
        );
        frames.push(w.snapshot(&camera()));
    }
    let emitters = gallery.render(&frames, 10, 160, &args[1].join("original-emitters.png"))?;
    let mut moving = world(&pack, 47)?;
    let id = "v20/emitter/playerjetemitter";
    let h = moving.start_emitter(id, SourceTransform::default(), SourceOptions::default())?;
    let mut moving_frames = Vec::new();
    for frame in 0..9 {
        for step in 0..20 {
            let t = (frame * 20 + step) as f32 / 120.;
            moving.update_source(
                h,
                SourceTransform {
                    position: Vec3::new((t * 3.).sin() * 4., t, 0.),
                    velocity: Vec3::new((t * 3.).cos() * 12., 1., 0.),
                    ..Default::default()
                },
            )?;
            moving.advance(1. / 120., Vec3::X * 0.2)?;
        }
        moving_frames.push(moving.snapshot(&camera()));
    }
    let moving_result = gallery.render(
        &moving_frames,
        3,
        320,
        &args[1].join("moving-player-jet.png"),
    )?;
    let mut explosions = Vec::new();
    let mut explosion_index = Vec::new();
    for c in &pack.manifest.composites {
        let mut w = world(&pack, 91)?;
        w.play_composite(&c.id, SourceTransform::default(), SourceOptions::default())?;
        for _ in 0..12 {
            w.advance(c.lifetime.min(0.1) / 12., Vec3::ZERO)?;
        }
        explosion_index.push(json!({"id":c.id,"particles":w.particle_count(),"lights":w.snapshot(&camera()).lights.len()}));
        explosions.push(w.snapshot(&camera()));
    }
    let composites = gallery.render(
        &explosions,
        8,
        192,
        &args[1].join("original-composites.png"),
    )?;
    let mut light_frames = Vec::new();
    let mut light_index = Vec::new();
    for l in pack.library.lights.iter().filter(|l| !l.name.is_empty()) {
        for t in [0.0, 0.27, 0.61] {
            let mut w = world(&pack, 77)?;
            w.start_light(&l.id, SourceTransform::default(), SourceOptions::default())?;
            w.advance(t, Vec3::ZERO)?;
            let f = w.snapshot(&camera());
            light_index.push(json!({"id":l.id,"time":t,"snapshot":f.lights.iter().map(|l|json!({"position":l.position.to_array(),"color":l.color.to_array(),"radius":l.radius})).collect::<Vec<_>>()}));
            light_frames.push(f);
        }
    }
    let lights = gallery.render(
        &light_frames,
        6,
        192,
        &args[1].join("original-light-flares.png"),
    )?;
    let mut crowded = EffectsWorld::new(
        pack.clone(),
        EffectsLimits {
            sources: 2048,
            particles: 20000,
            lights: 128,
            emissions_per_advance: 8192,
        },
        123,
    )?;
    for i in 0..1000 {
        crowded.start_emitter(
            "v20/emitter/playerjetemitter",
            SourceTransform {
                position: Vec3::new((i % 20) as f32, 0., (i / 20) as f32),
                ..Default::default()
            },
            SourceOptions::default(),
        )?;
    }
    let started = Instant::now();
    let mut samples = Vec::new();
    for _ in 0..240 {
        let start = Instant::now();
        crowded.advance(1. / 120., Vec3::X * 0.5)?;
        samples.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let total = started.elapsed().as_secs_f64();
    samples.sort_by(f64::total_cmp);
    let diagnostic = crowded.diagnostics();
    let frame = crowded.snapshot(&camera());
    let gpu_started = Instant::now();
    let gpu_stats = gallery.renderer.prepare(&queue, &camera(), &frame)?;
    let gpu_prepare_ms = gpu_started.elapsed().as_secs_f64() * 1000.;
    let report = json!({"adapter":adapter.get_info().name,"profile":if cfg!(debug_assertions){"debug"}else{"release"},"emitters":emitters,"emitter_index":index,"moving_attachment":moving_result,"composites":composites,"composite_index":explosion_index,"lights":lights,"light_index":light_index,"crowded":{"sources":1000,"steps":240,"seconds":total,"median_advance_ms":samples[120],"p95_advance_ms":samples[228],"diagnostics":diagnostic,"gpu_prepare_ms":gpu_prepare_ms,"gpu_stats":gpu_stats},"unresolved":pack.manifest.unresolved});
    std::fs::write(
        args[1].join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report["crowded"])?);
    Ok(())
}
