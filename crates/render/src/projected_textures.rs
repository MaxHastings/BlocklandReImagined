//! Projected textures: a picture projected onto whatever geometry the depth buffer already
//! holds, inside a volume, the way a flashlight, a decal or a blob shadow lands on a
//! scene. A full-screen pass per projection reconstructs each pixel's world position
//! from the depth buffer, maps it into the projection's texture space and, inside the
//! volume, blends the picture over the colour target.
//!
//! The pass reads depth as a texture, so it runs in its own render pass with the colour
//! target only (loaded, not cleared). Depth is the world's reversed 0..1 range (near is
//! 1, the cleared far value is 0, which projects nothing). The depth texture must be
//! single-sampled.
//!
//! Colour is display-encoded (see `color`) like every other pass.
//!
//! Optional fades multiply a projection's strength: a gradient picture sampled along a
//! matrix-derived coordinate (fade with distance along the projection), and a surface
//! facing factor `max(0, -n . direction)` from the pixel's screen-space normal.
use crate::color::{output_constants, shader_source};
use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProjectorImage(pub usize);

/// How the projection joins what is already on the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProjectionBlend {
    /// 2 x picture x target, with 0.5 as the neutral value; fades pull the picture toward
    /// 0.5.
    Modulate2X,
    /// Picture over target by the picture's alpha; fades multiply the alpha.
    Alpha,
    /// Picture added to the target; fades multiply the colour.
    Add,
}

#[derive(Clone, Debug)]
pub struct Projection {
    pub image: ProjectorImage,
    /// World position to texture space `(s, t, q)`; the picture coordinate is `(s, t)` when
    /// `divide` is false and `(s, t) / q` when it is true (perspective).
    pub world_to_texture: Mat4,
    pub divide: bool,
    /// The volume: a pixel is inside when `n . p - w >= 0` for every plane (up to 6).
    pub planes: Vec<[f32; 4]>,
    pub blend: ProjectionBlend,
    /// A picture and a matrix whose `y` output is its `v` coordinate (`u` is 0); its alpha
    /// is the fade.
    pub gradient: Option<(ProjectorImage, Mat4)>,
    /// Fade by how squarely the surface faces against this direction.
    pub normal_fade: Option<Vec3>,
    /// A world box around the volume, used to limit the pass to the screen area it covers.
    pub bounds: Option<(Vec3, Vec3)>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    inverse_view_projection: [[f32; 4]; 4],
    view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    size: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ProjectionUniform {
    world_to_texture: [[f32; 4]; 4],
    gradient: [[f32; 4]; 4],
    planes: [[f32; 4]; 6],
    /// plane count, divide, blend (0 modulate, 1 alpha, 2 add), has gradient
    modes: [f32; 4],
    /// xyz direction, w 1 when the normal fade is on
    forward: [f32; 4],
}

struct Image {
    view: wgpu::TextureView,
}

struct Prepared {
    blend: ProjectionBlend,
    bind: wgpu::BindGroup,
    /// x, y, width, height in pixels.
    scissor: [u32; 4],
    _uniform: wgpu::Buffer,
}

pub struct DepthProjectors {
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    globals_layout: wgpu::BindGroupLayout,
    projection_layout: wgpu::BindGroupLayout,
    target: wgpu::TextureFormat,
    sampler: wgpu::Sampler,
    globals_buffer: wgpu::Buffer,
    images: Vec<Image>,
    blank: wgpu::TextureView,
    pipelines: std::collections::HashMap<ProjectionBlend, wgpu::RenderPipeline>,
    globals_bind: Option<wgpu::BindGroup>,
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

fn blend_code(blend: ProjectionBlend) -> f32 {
    match blend {
        ProjectionBlend::Modulate2X => 0.0,
        ProjectionBlend::Alpha => 1.0,
        ProjectionBlend::Add => 2.0,
    }
}

/// The camera and targets a frame of projections is drawn against.
pub struct View<'a> {
    /// The camera's view-projection (reversed depth).
    pub view_projection: Mat4,
    pub camera: Vec3,
    /// The colour target's size in pixels.
    pub size: (u32, u32),
    /// The scene's depth, single-sampled.
    pub depth: &'a wgpu::TextureView,
}

impl DepthProjectors {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target: wgpu::TextureFormat) -> Self {
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
        let float_texture = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("projected textures globals"),
            entries: &[
                entry(0, uniform),
                entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let projection_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("projected textures projection"),
            entries: &[
                entry(0, uniform),
                entry(1, float_texture),
                entry(2, float_texture),
                entry(
                    3,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("projected textures"),
            bind_group_layouts: &[Some(&globals_layout), Some(&projection_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("projected textures"),
            source: wgpu::ShaderSource::Wgsl(
                shader_source(include_str!("projected_textures.wgsl")).into(),
            ),
        });
        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("projected textures globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let blank = Self::texture(device, queue, &[255; 4], 1, 1, false);
        Self {
            shader,
            pipeline_layout,
            globals_layout,
            projection_layout,
            target,
            sampler,
            globals_buffer,
            images: Vec::new(),
            blank,
            pipelines: Default::default(),
            globals_bind: None,
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
            label: Some("projected texture image"),
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
    ) -> ProjectorImage {
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        self.images.push(Image {
            view: Self::texture(device, queue, rgba, width, height, srgb),
        });
        ProjectorImage(self.images.len() - 1)
    }

    fn pipeline(&mut self, device: &wgpu::Device, blend: ProjectionBlend) {
        if self.pipelines.contains_key(&blend) {
            return;
        }
        let constants = output_constants(self.target);
        let (src, dst) = factors(blend);
        let component = wgpu::BlendComponent {
            src_factor: src,
            dst_factor: dst,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("projected textures"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
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
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(blend, pipeline);
    }

    /// Everything the pass needs this frame. `view_projection` is the camera's (reversed
    /// depth), `size` the colour target's size in pixels and `depth` the scene's depth.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &View<'_>,
        projections: &[Projection],
    ) {
        let (view_projection, camera, size, depth) =
            (view.view_projection, view.camera, view.size, view.depth);
        let globals = Globals {
            inverse_view_projection: view_projection.inverse().to_cols_array_2d(),
            view_projection: view_projection.to_cols_array_2d(),
            camera: [camera.x, camera.y, camera.z, 1.0],
            size: [size.0 as f32, size.1 as f32, 0.0, 0.0],
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        self.globals_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("projected textures globals"),
            layout: &self.globals_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        }));
        self.prepared.clear();
        for p in projections {
            self.pipeline(device, p.blend);
            let mut planes = [[0.0f32; 4]; 6];
            for (slot, plane) in planes.iter_mut().zip(&p.planes) {
                *slot = *plane;
            }
            let (gradient_matrix, gradient_view) = match &p.gradient {
                Some((image, m)) => (*m, &self.images[image.0].view),
                None => (Mat4::IDENTITY, &self.blank),
            };
            let forward = p.normal_fade.unwrap_or(Vec3::ZERO);
            let uniform_data = ProjectionUniform {
                world_to_texture: p.world_to_texture.to_cols_array_2d(),
                gradient: gradient_matrix.to_cols_array_2d(),
                planes,
                modes: [
                    p.planes.len().min(6) as f32,
                    if p.divide { 1.0 } else { 0.0 },
                    blend_code(p.blend),
                    if p.gradient.is_some() { 1.0 } else { 0.0 },
                ],
                forward: [
                    forward.x,
                    forward.y,
                    forward.z,
                    if p.normal_fade.is_some() { 1.0 } else { 0.0 },
                ],
            };
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("projected texture"),
                size: std::mem::size_of::<ProjectionUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&uniform_data));
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("projected texture"),
                layout: &self.projection_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.images[p.image.0].view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(gradient_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.prepared.push(Prepared {
                blend: p.blend,
                bind,
                scissor: scissor(view_projection, size, p.bounds),
                _uniform: uniform,
            });
        }
    }

    /// Draw every prepared projection into a pass over the colour target.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(globals) = &self.globals_bind else {
            return;
        };
        pass.set_bind_group(0, globals, &[]);
        for p in &self.prepared {
            if p.scissor[2] == 0 || p.scissor[3] == 0 {
                continue;
            }
            pass.set_pipeline(&self.pipelines[&p.blend]);
            pass.set_bind_group(1, &p.bind, &[]);
            pass.set_scissor_rect(p.scissor[0], p.scissor[1], p.scissor[2], p.scissor[3]);
            pass.draw(0..3, 0..1);
        }
    }
}

/// The screen rectangle a world box covers; the whole target when it reaches behind the
/// camera or no box is given.
fn scissor(view_projection: Mat4, size: (u32, u32), bounds: Option<(Vec3, Vec3)>) -> [u32; 4] {
    let full = [0, 0, size.0, size.1];
    let Some((min, max)) = bounds else {
        return full;
    };
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        let clip = view_projection * corner.extend(1.0);
        if clip.w <= 1e-4 {
            return full;
        }
        let ndc = [clip.x / clip.w, clip.y / clip.w];
        for k in 0..2 {
            lo[k] = lo[k].min(ndc[k]);
            hi[k] = hi[k].max(ndc[k]);
        }
    }
    let to_x = |n: f32| {
        ((n * 0.5 + 0.5) * size.0 as f32)
            .max(0.0)
            .min(size.0 as f32)
    };
    let to_y = |n: f32| {
        ((0.5 - n * 0.5) * size.1 as f32)
            .max(0.0)
            .min(size.1 as f32)
    };
    let (x0, x1) = (to_x(lo[0]).floor(), to_x(hi[0]).ceil());
    let (y0, y1) = (to_y(hi[1]).floor(), to_y(lo[1]).ceil());
    [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32]
}
