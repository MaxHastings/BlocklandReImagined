//! Screen-space ambient occlusion: a full-screen pass over the finished
//! world that darkens creases, wall feet and ledge undersides from the depth
//! buffer alone (`ambient_occlusion.wgsl`). It multiplies the frame, so it
//! runs between the world and the particles, UI and sky effects drawn after
//! it, and only while it is wanted: Classic lighting never draws it.
use crate::color::{output_constants, shader_source};
use crate::scene::{GpuInstances, GpuScene, SceneRenderer, WorldPass};
use std::cell::RefCell;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    view_projection: [f32; 16],
    inverse: [f32; 16],
    eye: [f32; 4],
    params: [f32; 4],
    size: [f32; 4],
    atmosphere: [f32; 4],
}

/// How far a crease reaches (world units), the darkest it makes a fully
/// enclosed pixel (share of its light removed), and the eye distances it
/// fades out over.
pub const RADIUS: f32 = 0.9;
pub const STRENGTH: f32 = 0.4;
pub const FADE: (f32, f32) = (40.0, 90.0);

type CachedMask = ((u32, u32), wgpu::TextureView);

pub struct AmbientOcclusion {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    samples: u32,
    mask: RefCell<Option<CachedMask>>,
}

impl AmbientOcclusion {
    /// Drawn into a colour target of `target` format with `samples` samples;
    /// it needs no depth attachment: the depth is read as a texture.
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat, samples: u32) -> Self {
        let multisampled = samples > 1;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ambient occlusion"),
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
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled,
                    },
                    count: None,
                },
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ambient occlusion"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let texture = if multisampled {
            "texture_depth_multisampled_2d"
        } else {
            "texture_depth_2d"
        };
        let source = include_str!("ambient_occlusion.wgsl")
            .replace("DEPTH_TEXTURE", texture)
            .replace(
                "MASK_TEXTURE",
                if multisampled {
                    "texture_multisampled_2d<f32>"
                } else {
                    "texture_2d<f32>"
                },
            );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ambient occlusion"),
            source: wgpu::ShaderSource::Wgsl(shader_source(&source).into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ambient occlusion"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let constants = output_constants(target);
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ambient occlusion"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    // Colour times the shade; alpha stays.
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Dst,
                            dst_factor: wgpu::BlendFactor::Zero,
                            operation: wgpu::BlendOperation::Add,
                        },
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
        Self {
            pipeline,
            layout,
            uniform,
            samples,
            mask: RefCell::new(None),
        }
    }
    /// Darken `color` (`size` pixels, drawn with `depth`, a view of a
    /// `TEXTURE_BINDING` depth texture of the same samples) by the
    /// occlusion seen from the camera `view_projection` and `eye`, fading
    /// out under the camera's fog (`atmosphere`, and `below`, its
    /// `fog_color.w`) so fogged creases keep the fog's colour.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        size: (u32, u32),
        view_projection: [f32; 16],
        eye: [f32; 3],
        (atmosphere, below): ([f32; 4], f32),
        (renderer, scenes, instances): (
            &SceneRenderer,
            &[&GpuScene],
            &[(&GpuScene, &GpuInstances)],
        ),
    ) {
        let matrix = glam::Mat4::from_cols_array(&view_projection);
        let inverse = matrix.inverse();
        if !inverse.is_finite() || size.0 == 0 || size.1 == 0 {
            return;
        }
        let mut cached = self.mask.borrow_mut();
        if cached.as_ref().is_none_or(|(old, _)| *old != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("occlusion exclusion mask"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.samples,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            *cached = Some((size, texture.create_view(&Default::default())));
        }
        let mask = &cached.as_ref().expect("allocated above").1;
        renderer.render_occlusion_mask(
            encoder,
            WorldPass {
                view: 0,
                color: mask,
                resolve: None,
                depth,
                viewport: None,
                clear: Some(wgpu::Color::BLACK),
                after_opaque: None,
                after_all: None,
            },
            scenes,
            instances,
        );
        let uniform = Uniform {
            view_projection,
            inverse: inverse.to_cols_array(),
            eye: [eye[0], eye[1], eye[2], 1.0],
            params: [RADIUS, STRENGTH, FADE.0, FADE.1],
            size: [size.0 as f32, size.1 as f32, below, 0.0],
            atmosphere,
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ambient occlusion"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(mask),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ambient occlusion"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
}
