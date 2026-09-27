//! wgpu renderer for a [`DrawList`].
//!
//! - Positions are logical pixels × `scale` (UI scaling).
//! - Blending is non-premultiplied alpha in the original display (gamma)
//!   space, like Torque's fixed-function GUI. Render into a **non-sRGB**
//!   view (`Rgba8Unorm`/`Bgra8Unorm`). With an sRGB swapchain, create the
//!   surface with an extra non-sRGB `view_formats` entry and render the UI
//!   through that view.
//! - Clip rectangles become scissor rectangles.
//! - Host-supplied textures (brick icons, avatar preview) are registered with
//!   [`UiRenderer::set_external`] and referenced as `TexKey::External(id)`.

use crate::draw::{DrawCmd, DrawList, Filter};
use crate::geom::Rect;
use crate::pack::{Pack, TexKey};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [u8; 4],
}

const SHADER: &str = r#"
struct Screen { size: vec2<f32>, pad: vec2<f32> };
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VOut {
    var o: VOut;
    let ndc = vec2<f32>(pos.x / screen.size.x * 2.0 - 1.0, 1.0 - pos.y / screen.size.y * 2.0);
    o.pos = vec4<f32>(ndc, 0.0, 1.0);
    o.uv = uv;
    o.color = color;
    return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, i.uv) * i.color;
}
"#;

struct Tex {
    view: wgpu::TextureView,
    size: (u32, u32),
    groups: [Option<wgpu::BindGroup>; 2],
}

struct Batch {
    tex: Option<TexKey>,
    filter: Filter,
    scissor: [u32; 4],
    first: u32,
    count: u32,
}

pub struct UiRenderer {
    tex_layout: wgpu::BindGroupLayout,
    screen_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    samplers: [wgpu::Sampler; 2],
    textures: HashMap<TexKey, Tex>,
    missing: std::collections::BTreeSet<TexKey>,
    white: Tex,
    vbuf: Option<(wgpu::Buffer, u64)>,
    ubuf: wgpu::Buffer,
    screen_group: wgpu::BindGroup,
}

fn filter_index(f: Filter) -> usize {
    match f {
        Filter::Linear => 0,
        Filter::Nearest => 1,
    }
}

impl UiRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bri-ui"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bri-ui screen"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bri-ui texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bri-ui"),
            bind_group_layouts: &[Some(&screen_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let mk_sampler = |f: wgpu::FilterMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("bri-ui"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: f,
                min_filter: f,
                ..Default::default()
            })
        };
        let samplers = [
            mk_sampler(wgpu::FilterMode::Linear),
            mk_sampler(wgpu::FilterMode::Nearest),
        ];
        let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bri-ui screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bri-ui screen"),
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ubuf.as_entire_binding(),
            }],
        });
        let white = upload(device, queue, 1, 1, &[255, 255, 255, 255]);
        UiRenderer {
            tex_layout,
            screen_layout,
            pipeline_layout,
            shader,
            pipelines: HashMap::new(),
            samplers,
            textures: HashMap::new(),
            missing: Default::default(),
            white: Tex {
                view: white,
                size: (1, 1),
                groups: [None, None],
            },
            vbuf: None,
            ubuf,
            screen_group,
        }
    }

    /// Register (or replace) a host texture, e.g. a brick icon render target.
    /// DrawCmd::Image source rectangles for External keys use normalized UVs.
    pub fn set_external(&mut self, id: u64, view: wgpu::TextureView, size: (u32, u32)) {
        self.textures.insert(
            TexKey::External(id),
            Tex {
                view,
                size,
                groups: [None, None],
            },
        );
    }
    pub fn remove_external(&mut self, id: u64) {
        self.textures.remove(&TexKey::External(id));
    }
    /// Textures referenced by draw lists that could not be resolved.
    pub fn missing_textures(&self) -> impl Iterator<Item = &TexKey> {
        self.missing.iter()
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        let (layout, shader) = (&self.pipeline_layout, &self.shader);
        self.pipelines.entry(format).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bri-ui"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4],
                    })],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    fn ensure_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pack: &Pack,
        key: &TexKey,
    ) -> bool {
        if self.textures.contains_key(key) {
            return true;
        }
        if self.missing.contains(key) {
            return false;
        }
        match pack.pixels(key) {
            Some(p) => {
                let view = upload(device, queue, p.width, p.height, &p.rgba);
                self.textures.insert(
                    key.clone(),
                    Tex {
                        view,
                        size: (p.width, p.height),
                        groups: [None, None],
                    },
                );
                true
            }
            None => {
                self.missing.insert(key.clone());
                false
            }
        }
    }

    fn group(
        &mut self,
        device: &wgpu::Device,
        key: Option<&TexKey>,
        filter: Filter,
    ) -> Option<wgpu::BindGroup> {
        let fi = filter_index(filter);
        let tex = match key {
            Some(k) => self.textures.get_mut(k)?,
            None => &mut self.white,
        };
        if tex.groups[fi].is_none() {
            tex.groups[fi] = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bri-ui texture"),
                layout: &self.tex_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.samplers[fi]),
                    },
                ],
            }));
        }
        tex.groups[fi].clone()
    }

    /// Record the draw list into `target`. `size` is the target size in
    /// physical pixels; `scale` maps logical to physical pixels. `clear`
    /// clears the target first (otherwise the UI is composited over it).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: (u32, u32),
        scale: f32,
        pack: &Pack,
        dl: &DrawList,
        clear: Option<wgpu::Color>,
    ) {
        let _ = &self.screen_layout;
        queue.write_buffer(
            &self.ubuf,
            0,
            bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]),
        );
        let mut verts: Vec<Vertex> = Vec::with_capacity(dl.cmds.len() * 6);
        let mut batches: Vec<Batch> = Vec::new();
        let scissor_of = |c: &Rect| -> [u32; 4] {
            let x0 = ((c.x as f32 * scale).round().max(0.0) as u32).min(size.0);
            let y0 = ((c.y as f32 * scale).round().max(0.0) as u32).min(size.1);
            let x1 = ((c.right() as f32 * scale).round().max(0.0) as u32).min(size.0);
            let y1 = ((c.bottom() as f32 * scale).round().max(0.0) as u32).min(size.1);
            [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
        };
        for cmd in &dl.cmds {
            let (tex, filter, clip, dst, uv, color) = match cmd {
                DrawCmd::Fill { dst, color, clip } => (
                    None,
                    Filter::Nearest,
                    *clip,
                    [dst.x as f32, dst.y as f32, dst.w as f32, dst.h as f32],
                    [0.0, 0.0, 1.0, 1.0],
                    *color,
                ),
                DrawCmd::Image {
                    tex,
                    src,
                    dst,
                    tint,
                    filter,
                    clip,
                } => {
                    if !self.ensure_texture(device, queue, pack, tex) {
                        continue;
                    }
                    let (tw, th) = self.textures[tex].size;
                    let (tw, th) = if matches!(tex, TexKey::External(_)) {
                        (1.0, 1.0)
                    } else {
                        (tw as f32, th as f32)
                    };
                    let uv = [
                        src[0] / tw,
                        src[1] / th,
                        (src[0] + src[2]) / tw,
                        (src[1] + src[3]) / th,
                    ];
                    (Some(tex.clone()), *filter, *clip, *dst, uv, *tint)
                }
            };
            let sc = scissor_of(&clip);
            if sc[2] == 0 || sc[3] == 0 {
                continue;
            }
            let (x0, y0) = (dst[0] * scale, dst[1] * scale);
            let (x1, y1) = ((dst[0] + dst[2]) * scale, (dst[1] + dst[3]) * scale);
            let v = |x, y, u, w| Vertex {
                pos: [x, y],
                uv: [u, w],
                color,
            };
            let first = verts.len() as u32;
            verts.extend_from_slice(&[
                v(x0, y0, uv[0], uv[1]),
                v(x1, y0, uv[2], uv[1]),
                v(x1, y1, uv[2], uv[3]),
                v(x0, y0, uv[0], uv[1]),
                v(x1, y1, uv[2], uv[3]),
                v(x0, y1, uv[0], uv[3]),
            ]);
            match batches.last_mut() {
                Some(b) if b.tex == tex && b.filter == filter && b.scissor == sc => b.count += 6,
                _ => batches.push(Batch {
                    tex,
                    filter,
                    scissor: sc,
                    first,
                    count: 6,
                }),
            }
        }
        let bytes = bytemuck::cast_slice::<Vertex, u8>(&verts);
        let need = (bytes.len() as u64).max(64);
        if self.vbuf.as_ref().is_none_or(|(_, cap)| *cap < need) {
            let cap = need.next_power_of_two();
            self.vbuf = Some((
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("bri-ui vertices"),
                    size: cap,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                cap,
            ));
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.vbuf.as_ref().expect("allocated").0, 0, bytes);
        }
        let groups: Vec<Option<wgpu::BindGroup>> = batches
            .iter()
            .map(|b| self.group(device, b.tex.as_ref(), b.filter))
            .collect();
        let pipeline = self.pipeline(device, format).clone();
        let vbuf = self.vbuf.as_ref().expect("allocated").0.clone();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bri-ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if batches.is_empty() {
            return;
        }
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &self.screen_group, &[]);
        pass.set_vertex_buffer(0, vbuf.slice(..));
        for (b, g) in batches.iter().zip(groups) {
            let Some(g) = g else { continue };
            pass.set_bind_group(1, &g, &[]);
            pass.set_scissor_rect(b.scissor[0], b.scissor[1], b.scissor[2], b.scissor[3]);
            pass.draw(b.first..b.first + b.count, 0..1);
        }
    }
}

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bri-ui image"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        t.as_image_copy(),
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        size,
    );
    t.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Headless device for offscreen rendering (tests, gallery).
pub struct Headless {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
}

impl Headless {
    pub fn new() -> Result<Self> {
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .context("no wgpu adapter")?;
            let adapter_info = adapter.get_info();
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await?;
            Ok(Headless {
                device,
                queue,
                adapter_info,
            })
        })
    }

    /// Render a draw list offscreen and read back tightly packed RGBA8.
    pub fn render_rgba(
        &self,
        r: &mut UiRenderer,
        pack: &Pack,
        dl: &DrawList,
        size: (u32, u32),
        scale: f32,
        background: [f64; 4],
    ) -> Result<Vec<u8>> {
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let extent = wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        };
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bri-ui offscreen"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        let [cr, cg, cb, ca] = background;
        r.render(
            &self.device,
            &self.queue,
            &mut enc,
            &view,
            format,
            size,
            scale,
            pack,
            dl,
            Some(wgpu::Color {
                r: cr,
                g: cg,
                b: cb,
                a: ca,
            }),
        );
        let row = (size.0 * 4).div_ceil(256) * 256;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bri-ui readback"),
            size: row as u64 * size.1 as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(size.1),
                },
            },
            extent,
        );
        self.queue.submit([enc.finish()]);
        let (send, recv) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |res| {
                let _ = send.send(res);
            });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(60)),
        })?;
        recv.recv()??;
        let data = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|e| anyhow::anyhow!("map: {e:?}"))?;
        let mut out = Vec::with_capacity((size.0 * size.1 * 4) as usize);
        for y in 0..size.1 as usize {
            let s = y * row as usize;
            out.extend_from_slice(&data[s..s + size.0 as usize * 4]);
        }
        drop(data);
        readback.unmap();
        Ok(out)
    }
}
