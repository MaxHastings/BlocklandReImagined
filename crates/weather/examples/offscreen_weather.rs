use anyhow::{Result, ensure};
use bri_weather::{gpu::WeatherRenderer, *};
use glam::{Mat4, Vec3};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}
fn camera(eye: Vec3, target: Vec3) -> (CameraState, Mat4) {
    let view = glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y);
    let inverse = view.inverse();
    (
        CameraState {
            position: eye,
            forward: (target - eye).normalize(),
            right: inverse.x_axis.truncate(),
            up: inverse.y_axis.truncate(),
            velocity: Vec3::ZERO,
        },
        bri_render::scene::perspective(60f32.to_radians(), 4. / 3., 0.1, 500.) * view,
    )
}
fn collision(ray: CollisionRay, roof: bool) -> Option<WeatherHit> {
    let delta = ray.end - ray.start;
    if delta.y.abs() < 1e-8 {
        return None;
    }
    let mut nearest = None;
    for (height, surface, limited) in [
        (0., WeatherSurface::Water, false),
        (8., WeatherSurface::Solid, true),
    ] {
        if limited && !roof {
            continue;
        }
        let t = (height - ray.start.y) / delta.y;
        let point = ray.start + delta * t;
        if !(0.0..=1.).contains(&t)
            || limited && !(point.x.abs() <= 8. && (-20.0..=-4.0).contains(&point.z))
        {
            continue;
        }
        if nearest.as_ref().is_none_or(|(previous, _)| t < *previous) {
            nearest = Some((
                t,
                WeatherHit {
                    position: point,
                    normal: Vec3::Y,
                    surface,
                },
            ));
        }
    }
    nearest.map(|(_, hit)| hit)
}
fn geometry(roof: bool) -> Vec<Vertex> {
    let mut out = Vec::new();
    let mut plane = |x0: f32, x1: f32, z0: f32, z1: f32, y: f32, color: [f32; 3]| {
        let points = [[x0, y, z0], [x1, y, z0], [x1, y, z1], [x0, y, z1]];
        for i in [0, 1, 2, 0, 2, 3] {
            out.push(Vertex {
                position: points[i],
                color,
            });
        }
    };
    for x in -20..20 {
        for z in -20..20 {
            let tone = if (x + z) % 2 == 0 { 0.035 } else { 0.06 };
            plane(
                x as f32 * 5.,
                (x + 1) as f32 * 5.,
                z as f32 * 5.,
                (z + 1) as f32 * 5.,
                0.,
                [tone * 0.7, tone, tone * 1.4],
            );
        }
    }
    if roof {
        plane(-8., 8., -20., -4., 8., [0.16, 0.19, 0.22]);
    }
    out
}
struct Probe<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    weather: WeatherRenderer,
    scene_pipeline: wgpu::RenderPipeline,
    scene_camera: wgpu::Buffer,
    scene_bind: wgpu::BindGroup,
}
impl<'a> Probe<'a> {
    fn new(device: &'a wgpu::Device, queue: &'a wgpu::Queue, pack: &WeatherPack) -> Result<Self> {
        let weather = WeatherRenderer::new(
            device,
            queue,
            pack,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Depth32Float,
            1,
            40000,
        )?;
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("headless collision fixture geometry"),source:wgpu::ShaderSource::Wgsl("@group(0) @binding(0) var<uniform> camera:mat4x4<f32>;struct Out{@builtin(position) p:vec4<f32>,@location(0) color:vec3<f32>} @vertex fn vs_main(@location(0) p:vec3<f32>,@location(1) c:vec3<f32>)->Out{var o:Out;o.p=camera*vec4(p,1.);o.color=c;return o;} @fragment fn fs_main(o:Out)->@location(0) vec4<f32>{return vec4(o.color,1.);}".into())});
        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("opaque roof/water query fixture"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(bri_render::scene::DEPTH_STRICTLY_NEARER),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let scene_camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fixture camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fixture camera"),
            layout: &scene_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: scene_camera.as_entire_binding(),
            }],
        });
        Ok(Self {
            device,
            queue,
            weather,
            scene_pipeline,
            scene_camera,
            scene_bind,
        })
    }
    fn render(
        &mut self,
        frame: &WeatherFrame,
        matrix: Mat4,
        vertices: &[Vertex],
        path: &Path,
    ) -> Result<serde_json::Value> {
        let width = 1024;
        let height = 768;
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let stats = self.weather.prepare(self.queue, matrix, frame)?;
        self.queue.write_buffer(
            &self.scene_camera,
            0,
            bytemuck::cast_slice(&matrix.to_cols_array()),
        );
        let texture = |format, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("offscreen weather"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let cv = color.create_view(&Default::default());
        let dv = depth.create_view(&Default::default());
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fixture triangles"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("weather with shared host depth"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &cv,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.018,
                            g: 0.026,
                            b: 0.041,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &dv,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(bri_render::scene::DEPTH_CLEAR),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !vertices.is_empty() {
                pass.set_pipeline(&self.scene_pipeline);
                pass.set_bind_group(0, &self.scene_bind, &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
            self.weather.render(&mut pass);
        }
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weather PNG readback"),
            size: u64::from(width * height * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
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
        let pixels = readback.slice(..).get_mapped_range()?;
        image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)?;
        Ok(
            json!({"drops":frame.drops,"splashes":frame.splashes,"render":stats,"width":width,"height":height,"pixel_sha256":sha256(&pixels)}),
        )
    }
}
fn main() -> Result<()> {
    pollster::block_on(run())
}
async fn run() -> Result<()> {
    let a: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        a.len() == 2,
        "Usage: offscreen_weather <native-pack> <artifact-dir>"
    );
    std::fs::create_dir_all(&a[1])?;
    let pack = WeatherPack::load(&a[0])?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await?;
    let (device, queue) = adapter.request_device(&Default::default()).await?;
    let mut probe = Probe::new(&device, &queue, &pack)?;
    let mut reports = Vec::new();
    for (name, suffix, roof, under) in [
        ("slopes-snow", "snowa", false, false),
        ("slate-rain-roof", "heavyrain", true, false),
        ("slate-rain-under-roof", "heavyrain", true, true),
    ] {
        let p = pack
            .manifest
            .placements
            .iter()
            .find(|p| p.definition.ends_with(suffix))
            .unwrap();
        let (mut camera, matrix) = if under {
            camera(Vec3::new(0., 3., -10.), Vec3::new(0., 3., -35.))
        } else {
            camera(Vec3::new(16., 12., 22.), Vec3::new(0., 4., -10.))
        };
        let mut world = WeatherWorld::new(pack.clone(), WeatherLimits::default(), 73)?;
        world.set_map(&p.map_id)?;
        world.set_environment(WeatherEnvironment {
            wind_velocity: Vec3::from_array(p.reference_wind_velocity),
        })?;
        for step in 0..240 {
            camera.velocity = Vec3::new((step as f32 * 0.1).sin(), 0., 0.);
            world.advance(1. / 60., camera, 1, &mut |r| collision(r, roof))?;
        }
        let frame = world.snapshot();
        let render = probe.render(
            &frame,
            matrix,
            &geometry(roof),
            &a[1].join(format!("{name}.png")),
        )?;
        reports.push(json!({"name":name,"map_id":p.map_id,"diagnostics":world.diagnostics(),"render":render}));
    }
    // Show complete source atlas cells at inspectable size, using the same shader/UV path.
    let mut atlas = WeatherFrame::default();
    let mut atlas_index = Vec::new();
    let mut row = 0;
    for (texture, image) in pack.textures.iter().enumerate() {
        let side: u32 = if image.id.ends_with("/rain") { 4 } else { 2 };
        for tile in 0..side * side {
            let x = tile % 4;
            let y = row + tile / 4;
            let uv = [
                (tile % side) as f32 / side as f32,
                (tile / side) as f32 / side as f32,
                ((tile % side) + 1) as f32 / side as f32,
                ((tile / side) + 1) as f32 / side as f32,
            ];
            atlas.instances.push(WeatherInstance {
                position: Vec3::new(x as f32 * 3. - 4.5, 9. - y as f32 * 3., 0.),
                right: Vec3::X * 1.25,
                up: Vec3::Y * 1.25,
                uv,
                texture: texture as u32,
                color: [1.; 4],
                splash: false,
            });
            atlas_index.push(json!({"texture":image.id,"tile":tile,"row":y,"column":x,"uv":uv}));
        }
        row += (side * side).div_ceil(4);
    }
    let matrix = glam::camera::rh::proj::directx::orthographic(-12., 12., -9., 9., 0.1, 100.)
        * glam::camera::rh::view::look_at_mat4(
            Vec3::new(0., 1.5, 20.),
            Vec3::new(0., 1.5, 0.),
            Vec3::Y,
        );
    let atlas_render = probe.render(&atlas, matrix, &[], &a[1].join("original-atlas-cells.png"))?;
    let rain = pack
        .manifest
        .placements
        .iter()
        .find(|p| p.definition.ends_with("heavyrain"))
        .unwrap();
    let (mut cam, _) = camera(Vec3::new(0., 4., 20.), Vec3::new(0., 4., -20.));
    let mut world = WeatherWorld::new(pack.clone(), WeatherLimits::default(), 21)?;
    world.set_map(&rain.map_id)?;
    world.set_environment(WeatherEnvironment {
        wind_velocity: Vec3::from_array(rain.reference_wind_velocity),
    })?;
    let mut samples = Vec::new();
    let start = Instant::now();
    for i in 0..600 {
        cam.position.x = i as f32 * 0.02;
        let started = Instant::now();
        world.advance(1. / 60., cam, 1, &mut |r| collision(r, true))?;
        samples.push(started.elapsed().as_secs_f64() * 1000.);
    }
    let elapsed = start.elapsed().as_secs_f64();
    samples.sort_by(f64::total_cmp);
    let frame = world.snapshot();
    let upload = Instant::now();
    let stats = probe.weather.prepare(&queue, Mat4::IDENTITY, &frame)?;
    let upload_ms = upload.elapsed().as_secs_f64() * 1000.;
    let report = json!({"adapter":adapter.get_info().name,"profile":if cfg!(debug_assertions){"debug"}else{"release"},"scenes":reports,"atlas":atlas_render,"atlas_index":atlas_index,"rain_benchmark":{"drops":world.drop_count(),"frames":600,"seconds":elapsed,"median_ms":samples[300],"p95_ms":samples[570],"diagnostics":world.diagnostics(),"upload_ms":upload_ms,"render":stats,"collision_fixture":"nearest bounded roof plane or water plane; host Rapier cost is not measured"},"assumptions":pack.manifest.assumptions});
    std::fs::write(
        a[1].join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report["rain_benchmark"])?
    );
    Ok(())
}
