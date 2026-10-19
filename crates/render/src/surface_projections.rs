//! Projected surfaces: a picture projected onto given triangles by redrawing them. The
//! caller supplies the receiving geometry (the same triangles the scene drew, clipped or
//! culled as it likes) with a per-vertex normal, surface texture coordinate and fade, and
//! says how the picture maps onto world space; the pass redraws the triangles over the
//! scene at a depth bias, writing no depth, blending the picture over the colour target.
//!
//! Compared with [`crate::projected_textures`] (which works from the depth buffer alone),
//! this keeps what only the triangles know: which surface a pixel belongs to, its own
//! texture coordinate (for the surface's alpha mask and base picture), the authored
//! normal and a per-vertex fade.
//!
//! The picture's coordinates are `world_to_texture * position`: `(s, t)` directly, or
//! divided by the third component when `divide` is set (perspective). Fades multiply a
//! projection's strength: the vertex fade, a gradient picture sampled along a matrix
//! coordinate, the surface mask's alpha and the facing of the interpolated normal against
//! a direction. Blends follow [`ProjectionBlend`].
//!
//! Depth is the world's reversed 0..1 range; the pass tests greater-or-equal after adding
//! `depth_bias` units.
use crate::color::{output_constants, shader_source};
use crate::projected_textures::{ProjectionBlend, ViewFog};
use glam::{Mat4, Vec3};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceImage(pub usize);

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SurfaceVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// The surface's own texture coordinate (for the mask and the base picture).
    pub uv: [f32; 2],
    /// The vertex's share of the projection (1 everywhere for an unfaded one).
    pub fade: f32,
}

/// How the surface's own picture joins the projected one before the frame-buffer blend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseBlend {
    /// 2 x picture x base.
    Modulate2X,
    /// picture by its own alpha over the base.
    ByPictureAlpha,
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceProjection {
    pub image: SurfaceImage,
    pub world_to_texture: Mat4,
    pub divide: bool,
    pub blend: ProjectionBlend,
    /// Repeat (true) or extend the edge (false) past 0..1 on `s` and `t`.
    pub wrap: [bool; 2],
    pub gradient: Option<(SurfaceImage, Mat4)>,
    /// Scale by the alpha of this picture sampled at the vertex coordinate.
    pub mask: Option<SurfaceImage>,
    /// Join the surface's own picture first.
    pub base: Option<(SurfaceImage, BaseBlend)>,
    /// Multiply by `max(0, -n . direction)` of the interpolated normal.
    pub normal_fade: Option<Vec3>,
    /// Multiply by the vertex fade.
    pub vertex_fade: bool,
    pub two_sided: bool,
    /// Depth units added to the triangles (the original's depth bias).
    pub depth_bias: i32,
    pub fog: Option<ViewFog>,
}

pub struct SurfaceBatch<'a> {
    pub indices: &'a [u32],
    pub projection: SurfaceProjection,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BatchUniform {
    world_to_texture: [[f32; 4]; 4],
    gradient: [[f32; 4]; 4],
    /// divide, blend (0 modulate, 1 alpha, 2 add), has gradient, has mask
    modes: [f32; 4],
    /// xyz direction, w normal fade on
    forward: [f32; 4],
    /// vertex fade, base mode (0 none, 1 modulate 2x, 2 by picture alpha), fog on, 0
    extra: [f32; 4],
    /// fog start, fog end, 0, 0
    fog: [f32; 4],
    fog_color: [f32; 4],
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PipelineKey {
    blend: ProjectionBlend,
    two_sided: bool,
    depth_bias: i32,
}

struct Prepared {
    key: PipelineKey,
    bind: wgpu::BindGroup,
    start: u32,
    count: u32,
    _uniform: wgpu::Buffer,
}

pub struct ProjectedSurfaces {
    target: wgpu::TextureFormat,
    depth: wgpu::TextureFormat,
    samples: u32,
    shader: wgpu::ShaderModule,
    batch_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    globals_buffer: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    /// repeat/clamp combinations for the projected picture
    samplers: [wgpu::Sampler; 4],
    repeat: wgpu::Sampler,
    clamp: wgpu::Sampler,
    images: Vec<wgpu::TextureView>,
    blank: wgpu::TextureView,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    vertices: wgpu::Buffer,
    vertex_capacity: u64,
    indices: wgpu::Buffer,
    index_capacity: u64,
    prepared: Vec<Prepared>,
}

fn factors(blend: ProjectionBlend) -> (wgpu::BlendFactor, wgpu::BlendFactor) {
    match blend {
        ProjectionBlend::Modulate2X => (wgpu::BlendFactor::Dst, wgpu::BlendFactor::Src),
        ProjectionBlend::Alpha => (
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
        ),
        ProjectionBlend::Add => (wgpu::BlendFactor::One, wgpu::BlendFactor::One),
    }
}

impl ProjectedSurfaces {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
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
        let texture = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let sampler = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("projected surfaces globals"),
            entries: &[entry(0, uniform)],
        });
        let batch_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("projected surfaces batch"),
            entries: &[
                entry(0, uniform),
                entry(1, texture),
                entry(2, texture),
                entry(3, texture),
                entry(4, texture),
                entry(5, sampler),
                entry(6, sampler),
                entry(7, sampler),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("projected surfaces"),
            bind_group_layouts: &[Some(&globals_layout), Some(&batch_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("projected surfaces"),
            source: wgpu::ShaderSource::Wgsl(
                shader_source(include_str!("surface_projections.wgsl")).into(),
            ),
        });
        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("projected surfaces globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("projected surfaces globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let make = |u: bool, v: bool| {
            let mode = |wrap| {
                if wrap {
                    wgpu::AddressMode::Repeat
                } else {
                    wgpu::AddressMode::ClampToEdge
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
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 4096,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let blank = Self::texture(device, queue, &[255; 4], 1, 1, false);
        Self {
            target,
            depth,
            samples,
            shader,
            batch_layout,
            pipeline_layout,
            globals_buffer,
            globals_bind,
            samplers: [
                make(false, false),
                make(true, false),
                make(false, true),
                make(true, true),
            ],
            repeat: make(true, true),
            clamp: make(false, false),
            images: Vec::new(),
            blank,
            pipelines: HashMap::new(),
            vertices: buffer("projected surface vertices", wgpu::BufferUsages::VERTEX),
            vertex_capacity: 4096,
            indices: buffer("projected surface indices", wgpu::BufferUsages::INDEX),
            index_capacity: 4096,
            prepared: Vec::new(),
        }
    }

    fn texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        srgb: bool,
    ) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("projected surface image"),
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
        texture.create_view(&Default::default())
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
    ) -> SurfaceImage {
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        self.images
            .push(Self::texture(device, queue, rgba, width, height, srgb));
        SurfaceImage(self.images.len() - 1)
    }

    fn pipeline(&mut self, device: &wgpu::Device, key: &PipelineKey) {
        if self.pipelines.contains_key(key) {
            return;
        }
        let constants = output_constants(self.target);
        let (src, dst) = factors(key.blend);
        let component = wgpu::BlendComponent {
            src_factor: src,
            dst_factor: dst,
            operation: wgpu::BlendOperation::Add,
        };
        let attributes =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("projected surfaces"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SurfaceVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: if key.two_sided {
                    None
                } else {
                    Some(wgpu::Face::Back)
                },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: self.depth,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: key.depth_bias,
                    slope_scale: 1.0,
                    clamp: 0.0,
                },
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
                    blend: Some(wgpu::BlendState {
                        color: component,
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key.clone(), pipeline);
    }

    fn view(&self, image: Option<SurfaceImage>) -> &wgpu::TextureView {
        image.map_or(&self.blank, |i| &self.images[i.0])
    }

    /// Upload this frame's geometry. `batches` index into `vertices`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        camera: Vec3,
        vertices: &[SurfaceVertex],
        batches: &[SurfaceBatch<'_>],
    ) {
        queue.write_buffer(
            &self.globals_buffer,
            0,
            bytemuck::bytes_of(&Globals {
                view_projection: view_projection.to_cols_array_2d(),
                camera: [camera.x, camera.y, camera.z, 1.0],
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
                label: Some("projected surface vertices"),
                size: self.vertex_capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        let index_bytes = (index_total * 4) as u64;
        if index_bytes > self.index_capacity {
            self.index_capacity = index_bytes.next_power_of_two();
            self.indices = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("projected surface indices"),
                size: self.index_capacity,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(vertices));
        let mut all = Vec::with_capacity(index_total);
        for batch in batches {
            let p = &batch.projection;
            let start = all.len() as u32;
            all.extend_from_slice(batch.indices);
            let key = PipelineKey {
                blend: p.blend,
                two_sided: p.two_sided,
                depth_bias: p.depth_bias,
            };
            self.pipeline(device, &key);
            let (gradient_matrix, gradient_image) = match p.gradient {
                Some((i, m)) => (m, Some(i)),
                None => (Mat4::IDENTITY, None),
            };
            let forward = p.normal_fade.unwrap_or(Vec3::ZERO);
            let uniform_data = BatchUniform {
                world_to_texture: p.world_to_texture.to_cols_array_2d(),
                gradient: gradient_matrix.to_cols_array_2d(),
                modes: [
                    if p.divide { 1.0 } else { 0.0 },
                    match p.blend {
                        ProjectionBlend::Modulate2X => 0.0,
                        ProjectionBlend::Alpha => 1.0,
                        ProjectionBlend::Add => 2.0,
                    },
                    if p.gradient.is_some() { 1.0 } else { 0.0 },
                    if p.mask.is_some() { 1.0 } else { 0.0 },
                ],
                forward: [
                    forward.x,
                    forward.y,
                    forward.z,
                    if p.normal_fade.is_some() { 1.0 } else { 0.0 },
                ],
                extra: [
                    if p.vertex_fade { 1.0 } else { 0.0 },
                    match p.base {
                        None => 0.0,
                        Some((_, BaseBlend::Modulate2X)) => 1.0,
                        Some((_, BaseBlend::ByPictureAlpha)) => 2.0,
                    },
                    if p.fog.is_some() { 1.0 } else { 0.0 },
                    0.0,
                ],
                fog: p.fog.map_or([0.0; 4], |f| [f.start, f.end, 0.0, 0.0]),
                fog_color: p
                    .fog
                    .map_or([0.0; 4], |f| [f.color[0], f.color[1], f.color[2], 1.0]),
            };
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("projected surface batch"),
                size: std::mem::size_of::<BatchUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&uniform_data));
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("projected surface batch"),
                layout: &self.batch_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(self.view(Some(p.image))),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(self.view(gradient_image)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(self.view(p.mask)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(
                            self.view(p.base.map(|b| b.0)),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(
                            &self.samplers[usize::from(p.wrap[0]) + 2 * usize::from(p.wrap[1])],
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&self.clamp),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::Sampler(&self.repeat),
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

    /// Draw into a pass over the colour target and the scene's depth (loaded, not written).
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
