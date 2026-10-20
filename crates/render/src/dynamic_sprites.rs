//! Dynamic textured triangle batches: the caller builds the vertices every
//! frame (a camera-facing or axis-locked quad per particle, a strip for a
//! beam) and states how they are drawn, the way a fixed-function pipeline is
//! told: one texture, a per-vertex colour, how texture and colour combine, a
//! blend state, depth test and write flags, an optional alpha test and linear
//! distance fog. Nothing here knows where the vertices came from.
//!
//! Depth is the world's reversed 0..1 range (near is 1), so the test is
//! greater-or-equal.
//!
//! Colour is display-encoded (see `color`): the texture is decoded and
//! re-encoded like every other pass, combined with the vertex colour in display
//! space, blended by the stated factors, and written through `output_color`.
use crate::color::{output_constants, shader_source};
use glam::Mat4;
use std::collections::HashMap;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteVertex {
    pub position: [f32; 3],
    /// Display-encoded RGBA, 0..1.
    pub color: [f32; 4],
    pub uv: [f32; 2],
    /// A second texture coordinate, read when the batch blends two frames.
    pub uv2: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Factor {
    Zero,
    One,
    SrcColor,
    InvSrcColor,
    SrcAlpha,
    InvSrcAlpha,
    DstColor,
    InvDstColor,
}
impl Factor {
    fn wgpu(self) -> wgpu::BlendFactor {
        match self {
            Factor::Zero => wgpu::BlendFactor::Zero,
            Factor::One => wgpu::BlendFactor::One,
            Factor::SrcColor => wgpu::BlendFactor::Src,
            Factor::InvSrcColor => wgpu::BlendFactor::OneMinusSrc,
            Factor::SrcAlpha => wgpu::BlendFactor::SrcAlpha,
            Factor::InvSrcAlpha => wgpu::BlendFactor::OneMinusSrcAlpha,
            Factor::DstColor => wgpu::BlendFactor::Dst,
            Factor::InvDstColor => wgpu::BlendFactor::OneMinusDst,
        }
    }
}

/// How the texel and the vertex colour make the fragment's colour or alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Combine {
    /// texel x vertex.
    Modulate,
    /// texel x vertex x 2.
    Modulate2X,
    /// texel x vertex x 4.
    Modulate4X,
    /// The texel alone.
    Texture,
    /// The vertex alone.
    Vertex,
    /// texel x a + vertex x (1 - a), a being the vertex colour's own alpha
    /// (when a constant replaces the vertex colour, the constant is what
    /// the texel fades toward).
    BlendByVertexAlpha,
}
impl Combine {
    fn code(self) -> f32 {
        match self {
            Combine::Modulate => 0.0,
            Combine::Modulate2X => 1.0,
            Combine::Modulate4X => 2.0,
            Combine::Texture => 3.0,
            Combine::Vertex => 4.0,
            Combine::BlendByVertexAlpha => 5.0,
        }
    }
}

/// How the texel of the second coordinate joins the first (`Batch::frames`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Frames {
    /// One coordinate only.
    Single,
    /// mix(second, first, vertex alpha): the vertex alpha is the share of the
    /// first frame, and is spent there (the fragment's alpha is then 1).
    ByVertexAlpha,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    pub start: f32,
    pub end: f32,
    /// What the fragment fades toward (display-encoded). Additive blends fade
    /// toward black, so a distant sprite adds nothing.
    pub color: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub usize);

/// How a projected picture joins the colour a sprite already has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionMode {
    /// picture x colour.
    Modulate,
    /// picture x colour x 2.
    Modulate2X,
    /// picture x its own alpha + colour x (1 - that alpha).
    ByPictureAlpha,
}
impl ProjectionMode {
    fn code(self) -> f32 {
        match self {
            ProjectionMode::Modulate => 0.0,
            ProjectionMode::Modulate2X => 1.0,
            ProjectionMode::ByPictureAlpha => 2.0,
        }
    }
}

/// A picture projected onto a batch's sprites through their world positions, joined to
/// the colour after the sprite's own texture and vertex colour. Alpha is untouched.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteProjection {
    pub image: ImageId,
    /// World position to texture space `(s, t, q)`.
    pub world_to_texture: Mat4,
    /// Divide `(s, t)` by `q` (perspective).
    pub divide: bool,
    pub mode: ProjectionMode,
}

/// How a batch's indices join into primitives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Topology {
    /// Three indices per triangle.
    #[default]
    Triangles,
    /// Two indices per independent line segment.
    Lines,
}
impl Topology {
    fn wgpu(self) -> wgpu::PrimitiveTopology {
        match self {
            Topology::Triangles => wgpu::PrimitiveTopology::TriangleList,
            Topology::Lines => wgpu::PrimitiveTopology::LineList,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub image: ImageId,
    /// None draws the fragment as it is (no blending).
    pub blend: Option<(Factor, Factor)>,
    pub color: Combine,
    pub alpha: Combine,
    pub frames: Frames,
    /// Replaces the vertex colour (a constant "factor" colour), when set.
    pub constant: Option<[f32; 4]>,
    pub depth_test: bool,
    pub depth_write: bool,
    /// Fragments with alpha below this are dropped.
    pub alpha_test: Option<f32>,
    pub clamp_u: bool,
    pub clamp_v: bool,
    pub two_sided: bool,
    pub fog: Option<Fog>,
    pub topology: Topology,
    /// Up to four projected pictures.
    pub projections: [Option<SpriteProjection>; 4],
}
impl State {
    pub fn new(image: ImageId) -> Self {
        Self {
            image,
            blend: None,
            color: Combine::Modulate,
            alpha: Combine::Modulate,
            frames: Frames::Single,
            constant: None,
            depth_test: true,
            depth_write: true,
            alpha_test: None,
            clamp_u: false,
            clamp_v: false,
            two_sided: true,
            fog: None,
            topology: Topology::Triangles,
            projections: [None; 4],
        }
    }
}

/// One draw: indices into this frame's vertices (`prepare`'s `vertices`).
pub struct Batch<'a> {
    pub state: State,
    pub indices: &'a [u32],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_projection: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BatchUniform {
    /// colour combine, alpha combine, frames, alpha test (< 0 off)
    modes: [f32; 4],
    constant: [f32; 4],
    /// has constant, fog start, fog end, fog enabled
    fog: [f32; 4],
    fog_color: [f32; 4],
    projection_matrices: [[[f32; 4]; 4]; 4],
    /// enabled, divide, mode, 0
    projection_modes: [[f32; 4]; 4],
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PipelineKey {
    blend: Option<(Factor, Factor)>,
    depth_test: bool,
    depth_write: bool,
    two_sided: bool,
    topology: Topology,
}

struct Image {
    view: wgpu::TextureView,
}

struct Prepared {
    key: PipelineKey,
    bind: wgpu::BindGroup,
    start: u32,
    count: u32,
    /// Keeps the per-batch uniform alive.
    _uniform: wgpu::Buffer,
}

pub struct DynamicSprites {
    target: wgpu::TextureFormat,
    depth: wgpu::TextureFormat,
    samples: u32,
    shader: wgpu::ShaderModule,
    batch_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    globals_buffer: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    samplers: [wgpu::Sampler; 4],
    images: Vec<Image>,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    vertices: wgpu::Buffer,
    vertex_capacity: u64,
    indices: wgpu::Buffer,
    index_capacity: u64,
    prepared: Vec<Prepared>,
}

impl DynamicSprites {
    pub fn new(
        device: &wgpu::Device,
        target: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty,
            count: None,
        };
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dynamic sprites globals"),
            entries: &[entry(0, uniform)],
        });
        let batch_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dynamic sprites batch"),
            entries: &[
                entry(0, uniform),
                entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    3,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    4,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    5,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    6,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    7,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dynamic sprites"),
            bind_group_layouts: &[Some(&globals_layout), Some(&batch_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dynamic sprites"),
            source: wgpu::ShaderSource::Wgsl(
                shader_source(include_str!("dynamic_sprites.wgsl")).into(),
            ),
        });
        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dynamic sprites globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dynamic sprites globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let sampler = |u: bool, v: bool| {
            let mode = |clamp| {
                if clamp {
                    wgpu::AddressMode::ClampToEdge
                } else {
                    wgpu::AddressMode::Repeat
                }
            };
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: mode(u),
                address_mode_v: mode(v),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        let samplers = [
            sampler(false, false),
            sampler(true, false),
            sampler(false, true),
            sampler(true, true),
        ];
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 4096,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            target,
            depth,
            samples,
            shader,
            batch_layout,
            pipeline_layout,
            globals_buffer,
            globals_bind,
            samplers,
            images: Vec::new(),
            pipelines: HashMap::new(),
            vertices: buffer("dynamic sprite vertices", wgpu::BufferUsages::VERTEX),
            vertex_capacity: 4096,
            indices: buffer("dynamic sprite indices", wgpu::BufferUsages::INDEX),
            index_capacity: 4096,
            prepared: Vec::new(),
        }
    }

    /// Upload a picture; `srgb` pictures are decoded like every other texture.
    pub fn add_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        srgb: bool,
    ) -> ImageId {
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dynamic sprite image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        self.images.push(Image {
            view: texture.create_view(&Default::default()),
        });
        ImageId(self.images.len() - 1)
    }

    fn pipeline(&mut self, device: &wgpu::Device, key: &PipelineKey) {
        if self.pipelines.contains_key(key) {
            return;
        }
        let constants = output_constants(self.target);
        let blend = key.blend.map(|(src, dst)| {
            let component = wgpu::BlendComponent {
                src_factor: src.wgpu(),
                dst_factor: dst.wgpu(),
                operation: wgpu::BlendOperation::Add,
            };
            wgpu::BlendState {
                color: component,
                alpha: component,
            }
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2, 3 => Float32x2];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dynamic sprites"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SpriteVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: key.topology.wgpu(),
                cull_mode: if key.two_sided {
                    None
                } else {
                    Some(wgpu::Face::Back)
                },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: self.depth,
                depth_write_enabled: Some(key.depth_write),
                depth_compare: Some(if key.depth_test {
                    wgpu::CompareFunction::GreaterEqual
                } else {
                    wgpu::CompareFunction::Always
                }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: self.samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.target,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key.clone(), pipeline);
    }

    /// The view of projection slot `slot`'s picture, or any picture when it is unused.
    fn projection_view(&self, state: &State, slot: usize) -> &wgpu::TextureView {
        match state.projections[slot] {
            Some(p) => &self.images[p.image.0].view,
            None => &self.images[state.image.0].view,
        }
    }

    /// Upload this frame's geometry. `batches` index into `vertices`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        vertices: &[SpriteVertex],
        batches: &[Batch<'_>],
    ) {
        queue.write_buffer(
            &self.globals_buffer,
            0,
            bytemuck::bytes_of(&Globals {
                view_projection: view_projection.to_cols_array_2d(),
            }),
        );
        self.prepared.clear();
        let index_total: usize = batches.iter().map(|b| b.indices.len()).sum();
        if vertices.is_empty() || index_total == 0 {
            return;
        }
        let vertex_bytes = std::mem::size_of_val(vertices) as u64;
        if vertex_bytes > self.vertex_capacity {
            self.vertex_capacity = vertex_bytes.next_power_of_two();
            self.vertices = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dynamic sprite vertices"),
                size: self.vertex_capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        let index_bytes = (index_total * 4) as u64;
        if index_bytes > self.index_capacity {
            self.index_capacity = index_bytes.next_power_of_two();
            self.indices = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dynamic sprite indices"),
                size: self.index_capacity,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(vertices));
        let mut all = Vec::with_capacity(index_total);
        for batch in batches {
            let start = all.len() as u32;
            all.extend_from_slice(batch.indices);
            let state = batch.state;
            let key = PipelineKey {
                blend: state.blend,
                depth_test: state.depth_test,
                depth_write: state.depth_write,
                two_sided: state.two_sided,
                topology: state.topology,
            };
            self.pipeline(device, &key);
            let fog = state.fog;
            let uniform_data = BatchUniform {
                modes: [
                    state.color.code(),
                    state.alpha.code(),
                    match state.frames {
                        Frames::Single => 0.0,
                        Frames::ByVertexAlpha => 1.0,
                    },
                    state.alpha_test.unwrap_or(-1.0),
                ],
                constant: state.constant.unwrap_or([0.0; 4]),
                fog: [
                    if state.constant.is_some() { 1.0 } else { 0.0 },
                    fog.map_or(0.0, |f| f.start),
                    fog.map_or(1.0, |f| f.end),
                    if fog.is_some() { 1.0 } else { 0.0 },
                ],
                fog_color: fog.map_or([0.0; 4], |f| [f.color[0], f.color[1], f.color[2], 0.0]),
                projection_matrices: state.projections.map(|p| {
                    p.map_or(Mat4::IDENTITY, |p| p.world_to_texture)
                        .to_cols_array_2d()
                }),
                projection_modes: state.projections.map(|p| {
                    p.map_or([0.0; 4], |p| {
                        [1.0, if p.divide { 1.0 } else { 0.0 }, p.mode.code(), 0.0]
                    })
                }),
            };
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dynamic sprites batch"),
                size: std::mem::size_of::<BatchUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&uniform_data));
            let sampler =
                &self.samplers[usize::from(state.clamp_u) + 2 * usize::from(state.clamp_v)];
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dynamic sprites batch"),
                layout: &self.batch_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &self.images[state.image.0].view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(
                            self.projection_view(&state, 0),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(
                            self.projection_view(&state, 1),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(
                            self.projection_view(&state, 2),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(
                            self.projection_view(&state, 3),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::Sampler(&self.samplers[3]),
                    },
                ],
            });
            self.prepared.push(Prepared {
                key,
                bind,
                start,
                count: batch.indices.len() as u32,
                _uniform: uniform,
            });
        }
        queue.write_buffer(&self.indices, 0, bytemuck::cast_slice(&all));
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.prepared.is_empty() {
            return;
        }
        pass.set_bind_group(0, &self.globals_bind, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for batch in &self.prepared {
            if batch.count == 0 {
                continue;
            }
            pass.set_pipeline(&self.pipelines[&batch.key]);
            pass.set_bind_group(1, &batch.bind, &[]);
            pass.draw_indexed(batch.start..batch.start + batch.count, 0, 0..1);
        }
    }
}
