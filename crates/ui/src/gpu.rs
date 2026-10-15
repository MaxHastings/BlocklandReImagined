//! wgpu renderer for a [`DrawList`].
//!
//! - Positions are logical pixels × `scale` (UI scaling).
//! - [`Filter::Nearest`] art (bitmap glyphs, 1:1 pixel art) is sampled
//!   "sharp": each texel covers exactly its share of the screen and only the
//!   pixels a texel edge crosses blend with the neighbour. At whole scales this
//!   is plain nearest sampling; at 125% or 150% it keeps strokes even instead
//!   of alternating one and two pixels wide.
//! - Blending is non-premultiplied alpha in the original display (gamma)
//!   space, like Torque's fixed-function GUI. Render into a **non-sRGB**
//!   view (`Rgba8Unorm`/`Bgra8Unorm`). With an sRGB swapchain, create the
//!   surface with an extra non-sRGB `view_formats` entry and render the UI
//!   through that view.
//! - Clip rectangles become scissor rectangles.
//! - A fractional scale above 1 (UI Size 125%, 150%, 250%) draws the UI at
//!   the next whole scale into an offscreen texture and shrinks it onto the
//!   target with exact area weights, as macOS draws its scaled resolutions.
//!   Every logical pixel then covers the same 1.25 or 1.5 screen pixels, so
//!   one-pixel borders, glyph strokes and stretched art look the same
//!   wherever they land, and the result is the whole-scale UI, made smaller.
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
    /// 1.0 for [`Filter::Nearest`] quads (sharp sampling), else 0.0.
    sharp: f32,
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
    @location(2) @interpolate(flat) sharp: f32,
};

@vertex
fn vs(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>, @location(3) sharp: f32) -> VOut {
    var o: VOut;
    let ndc = vec2<f32>(pos.x / screen.size.x * 2.0 - 1.0, 1.0 - pos.y / screen.size.y * 2.0);
    o.pos = vec4<f32>(ndc, 0.0, 1.0);
    o.uv = uv;
    o.color = color;
    o.sharp = sharp;
    return o;
}

fn texel(i: vec2<f32>, size: vec2<f32>) -> vec4<f32> {
    let c = textureLoad(tex, vec2<i32>(clamp(i, vec2<f32>(0.0), size - 1.0)), 0);
    return vec4<f32>(c.rgb * c.a, c.a);
}

// Nearest-looking magnification at any scale: inside a texel the sample sits
// on its centre; only where a texel edge crosses the pixel does it blend with
// the neighbour, by the share of the pixel each covers. Blends premultiplied,
// so transparent texels do not darken glyph edges.
fn sharp_sample(p: vec2<f32>, w: vec2<f32>, size: vec2<f32>) -> vec4<f32> {
    let fl = floor(p);
    let g = p - fl - 0.5;
    let wc = clamp(w, vec2<f32>(1e-6), vec2<f32>(1.0));
    let k = 0.5 / wc - 0.5;
    let gw = g / wc;
    let b = fl + gw - clamp(gw, -k, k);
    let i = floor(b);
    let t = b - i;
    let top = mix(texel(i, size), texel(i + vec2<f32>(1.0, 0.0), size), t.x);
    let bottom = mix(texel(i + vec2<f32>(0.0, 1.0), size), texel(i + vec2<f32>(1.0, 1.0), size), t.x);
    let c = mix(top, bottom, t.y);
    return select(vec4<f32>(0.0), vec4<f32>(c.rgb / c.a, c.a), c.a > 0.0);
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(tex));
    let p = i.uv * size;
    // Derivatives and the filtered sample stay in uniform control flow.
    let w = fwidth(p);
    let filtered = textureSample(tex, samp, i.uv);
    var c = filtered;
    if i.sharp > 0.5 {
        c = sharp_sample(p, w, size);
    }
    return c * i.color;
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
    downscale: Option<Downscale>,
}

/// The offscreen whole-scale UI and the pass that shrinks it onto the target.
struct Downscale {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    ubuf: wgpu::Buffer,
    /// The offscreen texture, its size and the bind group reading it.
    target: Option<((u32, u32), wgpu::TextureView, wgpu::BindGroup)>,
}

/// Format of the offscreen UI: non-sRGB, so blending stays in display space.
const OFFSCREEN: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Shrinks the premultiplied offscreen UI by `ratio` (whole scale over the
/// requested scale, between 1 and 2): each target pixel averages the
/// offscreen texels its footprint covers, weighted by the area covered. A
/// footprint at most two texels wide touches at most three per axis.
const DOWNSCALE_SHADER: &str = r#"
struct Params { ratio: f32, pad0: f32, size: vec2<f32> };
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var src: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

fn weights(start: f32, ratio: f32) -> vec3<f32> {
    let i = floor(start);
    let end = start + ratio;
    var w: vec3<f32>;
    for (var k = 0; k < 3; k++) {
        let a = i + f32(k);
        w[k] = max(min(end, a + 1.0) - max(start, a), 0.0) / ratio;
    }
    return w;
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let start = floor(pos.xy) * params.ratio;
    let base = floor(start);
    let wx = weights(start.x, params.ratio);
    let wy = weights(start.y, params.ratio);
    let last = params.size - 1.0;
    var c = vec4<f32>(0.0);
    for (var y = 0; y < 3; y++) {
        for (var x = 0; x < 3; x++) {
            let w = wx[x] * wy[y];
            if w > 0.0 {
                let t = min(base + vec2<f32>(f32(x), f32(y)), last);
                c += w * textureLoad(src, vec2<i32>(t), 0);
            }
        }
    }
    return c;
}
"#;

impl Downscale {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bri-ui downscale"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bri-ui downscale"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bri-ui downscale"),
            source: wgpu::ShaderSource::Wgsl(DOWNSCALE_SHADER.into()),
        });
        let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bri-ui downscale"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Downscale {
            layout,
            pipeline_layout,
            shader,
            pipelines: HashMap::new(),
            ubuf,
            target: None,
        }
    }

    /// The offscreen texture of `size`, made or remade as needed.
    fn target(&mut self, device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
        if self.target.as_ref().is_none_or(|(s, ..)| *s != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bri-ui whole scale"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: OFFSCREEN,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bri-ui downscale"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.ubuf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                ],
            });
            self.target = Some((size, view, group));
        }
        self.target.as_ref().expect("made above").1.clone()
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        let (layout, shader) = (&self.pipeline_layout, &self.shader);
        self.pipelines.entry(format).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bri-ui downscale"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
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
                        // The offscreen UI is premultiplied (drawn over
                        // transparent black with ordinary alpha blending).
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }
}

/// The whole scale a fractional `scale` above 1 is drawn at before it is
/// shrunk, or `None` to draw straight at `scale`.
fn whole_scale(scale: f32) -> Option<f32> {
    (scale.is_finite() && scale > 1.0 && scale.fract() > 1e-4).then(|| scale.ceil())
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
            downscale: None,
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
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4, 3 => Float32],
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
        let Some(whole) = whole_scale(scale) else {
            self.draw(
                device, queue, encoder, target, format, size, scale, pack, dl, clear,
            );
            return;
        };
        let ratio = whole / scale;
        let inner = (
            ((size.0 as f32 * ratio).ceil() as u32).max(1),
            ((size.1 as f32 * ratio).ceil() as u32).max(1),
        );
        let downscale = self.downscale.get_or_insert_with(|| Downscale::new(device));
        let offscreen = downscale.target(device, inner);
        queue.write_buffer(
            &downscale.ubuf,
            0,
            bytemuck::cast_slice(&[ratio, 0.0, inner.0 as f32, inner.1 as f32]),
        );
        self.draw(
            device,
            queue,
            encoder,
            &offscreen,
            OFFSCREEN,
            inner,
            whole,
            pack,
            dl,
            Some(wgpu::Color::TRANSPARENT),
        );
        let downscale = self.downscale.as_mut().expect("made above");
        let pipeline = downscale.pipeline(device, format).clone();
        let group = downscale.target.as_ref().expect("made above").2.clone();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bri-ui downscale"),
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// [`Self::render`] straight at `scale`.
    #[allow(clippy::too_many_arguments)]
    fn draw(
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
            let sharp = if filter == Filter::Nearest { 1.0 } else { 0.0 };
            let v = |x, y, u, w| Vertex {
                pos: [x, y],
                uv: [u, w],
                color,
                sharp,
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
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
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
        self.render_rgba_with_timeout(
            r,
            pack,
            dl,
            size,
            scale,
            background,
            std::time::Duration::from_secs(60),
        )
    }

    /// Render and read back with a caller-selected budget for slow software GPUs.
    #[allow(clippy::too_many_arguments)] // mirrors render_rgba and adds only its wait budget
    pub fn render_rgba_with_timeout(
        &self,
        r: &mut UiRenderer,
        pack: &Pack,
        dl: &DrawList,
        size: (u32, u32),
        scale: f32,
        background: [f64; 4],
        timeout: std::time::Duration,
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
            timeout: Some(timeout),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{DrawList, Filter};
    use crate::pack::{Pack, Pixels};

    /// One-logical-pixel fill lines at a fractional scale each cover exactly
    /// 1.25 or 1.5 screen pixels wherever they fall (drawn at the next whole
    /// scale and shrunk by area), instead of rounding to one or two pixels.
    /// Translucent fills blend over the background as they do when drawn
    /// straight.
    #[test]
    fn fractional_scale_lines_cover_their_exact_share() {
        let gpu = Headless::new().unwrap();
        let pack = Pack::from_parts(Default::default(), Default::default());
        let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
        for scale in [1.25f32, 1.5, 2.5] {
            let width = (16.0 * scale) as u32;
            let mut dl = DrawList::new(Rect::new(0, 0, 16, 8));
            for x in (0..16).step_by(2) {
                dl.fill(Rect::new(x, 0, 1, 4), crate::geom::WHITE);
            }
            dl.fill(Rect::new(0, 4, 16, 4), [255, 0, 0, 128]);
            let out = gpu
                .render_rgba(
                    &mut renderer,
                    &pack,
                    &dl,
                    (width, (8.0 * scale) as u32),
                    scale,
                    [0.0, 0.0, 1.0, 1.0],
                )
                .unwrap();
            for x in 0..width {
                let covered: f32 = (0..16)
                    .step_by(2)
                    .map(|j| {
                        let (a, b) = (j as f32 * scale, (j + 1) as f32 * scale);
                        (b.min(x as f32 + 1.0) - a.max(x as f32)).max(0.0)
                    })
                    .sum();
                let want = (covered * 255.0).round() as i32;
                let got = out[x as usize * 4] as i32;
                assert!(
                    (got - want).abs() <= 3,
                    "scale {scale} pixel {x}: got {got}, want {want}"
                );
            }
            let row = ((6.0 * scale) as u32 * width) as usize * 4;
            let px = &out[row + 4..row + 8];
            assert!(
                (px[0] as i32 - 128).abs() <= 2 && px[1] == 0 && (px[2] as i32 - 127).abs() <= 2,
                "scale {scale}: translucent fill {px:?}"
            );
        }
    }

    /// A one-texel stripe pattern drawn at fractional UI scales covers each
    /// screen pixel by exactly its share of opaque texels, so a 1-texel stroke
    /// is 1.25 or 1.5 pixels everywhere instead of alternating 1 and 2. Whole
    /// scales stay exact nearest sampling, and transparent texels do not
    /// darken the opaque ones they blend with.
    #[test]
    fn nearest_art_keeps_even_strokes_at_fractional_scales() {
        let gpu = Headless::new().unwrap();
        let pack = Pack::from_parts(Default::default(), Default::default());
        let key = TexKey::Image("stripes".into());
        let rgba: Vec<u8> = (0..8)
            .flat_map(|i| {
                if i % 2 == 0 {
                    [255, 255, 255, 255]
                } else {
                    [0, 0, 0, 0]
                }
            })
            .collect();
        pack.insert_pixels(
            key.clone(),
            Pixels {
                width: 8,
                height: 1,
                rgba,
            },
        );
        let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
        for scale in [1.0f32, 1.25, 1.5, 2.0, 2.5, 3.0] {
            let width = (8.0 * scale) as u32;
            let size = (width, 4);
            let mut dl = DrawList::new(Rect::new(0, 0, 8, 4));
            dl.image(
                key.clone(),
                [0.0, 0.0, 8.0, 1.0],
                [0.0, 0.0, 8.0, 1.0],
                crate::geom::WHITE,
                Filter::Nearest,
            );
            let out = gpu
                .render_rgba(&mut renderer, &pack, &dl, size, scale, [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            for x in 0..width {
                // Share of pixel [x, x+1) covered by even (opaque) texels.
                let covered: f32 = (0..8)
                    .step_by(2)
                    .map(|j| {
                        let (a, b) = (j as f32 * scale, (j + 1) as f32 * scale);
                        (b.min(x as f32 + 1.0) - a.max(x as f32)).max(0.0)
                    })
                    .sum();
                let want = (covered * 255.0).round() as i32;
                let got = out[x as usize * 4] as i32;
                assert!(
                    (got - want).abs() <= 3,
                    "scale {scale} pixel {x}: got {got}, want {want}"
                );
                if scale.fract() == 0.0 {
                    assert!(got == 0 || got == 255, "scale {scale} pixel {x}: {got}");
                }
            }
        }
    }
}
