//! The sun's lens flare over the world: a map's flare pictures strung along
//! the line from the sun's place on the screen through the screen's centre,
//! each added to the frame (`bri_content::environment::LensFlare`). Drawn in
//! a pass with a `depth` attachment and `samples` samples, which it neither
//! tests nor writes; whether the sun shows (in view, not blocked) is the
//! caller's to say.
use crate::color::{output_constants, shader_source};
use crate::scene::LensFlareScene;
use glam::Vec2;
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    place: [f32; 4],
    color: [f32; 4],
}

struct Drawn {
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    /// Hidden until `update` places it.
    shown: bool,
}

pub struct LensFlareRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    scene: Option<Arc<LensFlareScene>>,
    drawn: Vec<Drawn>,
}

impl LensFlareRenderer {
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lens flare"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
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
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lens flare"),
            source: wgpu::ShaderSource::Wgsl(shader_source(include_str!("lens_flare.wgsl")).into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lens flare"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let constants = output_constants(target);
        let additive = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lens flare"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
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
                    blend: Some(wgpu::BlendState {
                        color: additive,
                        alpha: additive,
                    }),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            layout,
            sampler,
            scene: None,
            drawn: Vec::new(),
        }
    }

    /// The map's flare (None: none). The same flare again costs nothing.
    pub fn set_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: Option<&Arc<LensFlareScene>>,
    ) {
        if self.scene.as_ref().map(Arc::as_ptr) == scene.map(Arc::as_ptr) {
            return;
        }
        self.scene = scene.cloned();
        self.drawn.clear();
        let Some(scene) = scene else { return };
        for (image, _) in &scene.elements {
            let size = wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&image.label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                texture.as_image_copy(),
                &image.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(image.width * 4),
                    rows_per_image: Some(image.height),
                },
                size,
            );
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lens flare element"),
                size: std::mem::size_of::<Uniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("lens flare element"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &texture.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.drawn.push(Drawn {
                uniform,
                bind,
                shown: false,
            });
        }
    }

    /// Place this frame's flare. `sun` is the sun's place on the screen in
    /// clip space (x right, y up), `strength` how much of it shows (0 hides
    /// it), `aspect` the frame's width over height.
    pub fn update(&mut self, queue: &wgpu::Queue, sun: Vec2, strength: f32, aspect: f32) {
        let Some(scene) = &self.scene else { return };
        for ((_, element), drawn) in scene.elements.iter().zip(&mut self.drawn) {
            drawn.shown = false;
            let level = element.brightness * strength;
            if level.is_nan() || level <= 0.0 || !sun.is_finite() {
                continue;
            }
            // 0 on the sun, 0.5 at the centre, 1 mirrored through it.
            let at = sun * (1.0 - 2.0 * element.position);
            let uniform = Uniform {
                place: [
                    at.x,
                    at.y,
                    element.radius / aspect.max(0.01),
                    element.radius,
                ],
                color: [element.tint[0], element.tint[1], element.tint[2], level],
            };
            queue.write_buffer(&drawn.uniform, 0, bytemuck::bytes_of(&uniform));
            drawn.shown = true;
        }
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        if !self.drawn.iter().any(|d| d.shown) {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        for drawn in self.drawn.iter().filter(|d| d.shown) {
            pass.set_bind_group(0, &drawn.bind, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}
