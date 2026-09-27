//! Instanced billboard adapter for a host-owned wgpu device and render pass.
use crate::{BlendMode, Camera, EffectsPack, FrameEffects};
use anyhow::{Result, ensure};
use std::ops::Range;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuParticle {
    position_size: [f32; 4],
    color: [f32; 4],
    axis_spin: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuCamera {
    view_projection: [[f32; 4]; 4],
    right: [f32; 4],
    up: [f32; 4],
    position: [f32; 4],
}
struct Run {
    texture: usize,
    blend: BlendMode,
    depth_test: bool,
    instances: Range<u32>,
}
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct RenderStats {
    pub instances: usize,
    pub draw_calls: usize,
    pub uploaded_bytes: usize,
}
pub struct EffectsRenderer {
    alpha: wgpu::RenderPipeline,
    additive: wgpu::RenderPipeline,
    additive_color: wgpu::RenderPipeline,
    flare_alpha: wgpu::RenderPipeline,
    flare_additive: wgpu::RenderPipeline,
    flare_additive_color: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    images: Vec<wgpu::BindGroup>,
    instances: wgpu::Buffer,
    capacity: usize,
    runs: Vec<Run>,
    stats: RenderStats,
}
impl EffectsRenderer {
    /// Target and depth formats/sample count must match the host render pass.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pack: &EffectsPack,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
        sample_count: u32,
        max_instances: usize,
    ) -> Result<Self> {
        ensure!(
            max_instances > 0
                && max_instances <= 1_004_096
                && matches!(sample_count, 1 | 2 | 4 | 8 | 16),
            "Invalid effects GPU limits"
        );
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effects camera"),
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
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("original effect texture"),
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
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("effects camera"),
            size: std::mem::size_of::<GpuCamera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effects camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("effects linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut images = Vec::new();
        for image in &pack.textures {
            ensure!(
                image.width <= device.limits().max_texture_dimension_2d
                    && image.height <= device.limits().max_texture_dimension_2d,
                "Effect texture exceeds device limit"
            );
            let size = wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&image.id),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            let view = texture.create_view(&Default::default());
            images.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&image.id),
                layout: &texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            }));
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("native effects billboards"),
            source: wgpu::ShaderSource::Wgsl(include_str!("particles.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("effects"),
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = |blend: wgpu::BlendState, depth_test: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("native effects"),layout:Some(&layout),vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs_main"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout {array_stride:std::mem::size_of::<GpuParticle>() as u64,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4]})]},
            primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},depth_stencil:Some(wgpu::DepthStencilState {format:depth_format,depth_write_enabled:Some(false),depth_compare:Some(if depth_test {wgpu::CompareFunction::LessEqual}else{wgpu::CompareFunction::Always}),stencil:Default::default(),bias:Default::default()}),multisample:wgpu::MultisampleState {count:sample_count,..Default::default()},
            fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs_main"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState {format:color_format,blend:Some(blend),write_mask:wgpu::ColorWrites::ALL})]}),multiview_mask:None,cache:None,
        })
        };
        let additive = |src_factor| wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let alpha = pipeline(wgpu::BlendState::ALPHA_BLENDING, true);
        let additive_color = pipeline(additive(wgpu::BlendFactor::One), true);
        let flare_alpha = pipeline(wgpu::BlendState::ALPHA_BLENDING, false);
        let flare_additive_color = pipeline(additive(wgpu::BlendFactor::One), false);
        let flare_additive = pipeline(additive(wgpu::BlendFactor::SrcAlpha), false);
        let additive = pipeline(additive(wgpu::BlendFactor::SrcAlpha), true);
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded effect instances"),
            size: (max_instances * std::mem::size_of::<GpuParticle>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            alpha,
            additive,
            additive_color,
            flare_alpha,
            flare_additive,
            flare_additive_color,
            camera,
            camera_bind,
            images,
            instances,
            capacity: max_instances,
            runs: Vec::new(),
            stats: RenderStats::default(),
        })
    }
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        camera: &Camera,
        frame: &FrameEffects,
    ) -> Result<RenderStats> {
        ensure!(
            frame.particles.len() <= self.capacity,
            "Effects frame exceeds GPU instance budget"
        );
        ensure!(
            camera.view_projection.is_finite()
                && camera.position.is_finite()
                && camera.right.is_normalized()
                && camera.up.is_normalized(),
            "Invalid effects camera"
        );
        let mut data = Vec::with_capacity(frame.particles.len());
        let mut runs: Vec<Run> = Vec::new();
        for (i, p) in frame.particles.iter().enumerate() {
            ensure!(
                (p.texture as usize) < self.images.len()
                    && p.position.is_finite()
                    && p.color.is_finite()
                    && p.size.is_finite()
                    && p.size >= 0.
                    && p.axis.is_finite()
                    && p.spin.is_finite(),
                "Invalid effects instance"
            );
            data.push(GpuParticle {
                position_size: p.position.extend(p.size).to_array(),
                color: p.color.to_array(),
                axis_spin: p.axis.extend(p.spin).to_array(),
            });
            if let Some(run) = runs.last_mut()
                && run.texture == p.texture as usize
                && run.blend == p.blend
                && run.depth_test == p.depth_test
            {
                run.instances.end += 1;
            } else {
                runs.push(Run {
                    texture: p.texture as usize,
                    blend: p.blend,
                    depth_test: p.depth_test,
                    instances: i as u32..i as u32 + 1,
                });
            }
        }
        let uniform = GpuCamera {
            view_projection: camera.view_projection.to_cols_array_2d(),
            right: camera.right.extend(0.).to_array(),
            up: camera.up.extend(0.).to_array(),
            position: camera.position.extend(0.).to_array(),
        };
        queue.write_buffer(&self.camera, 0, bytemuck::bytes_of(&uniform));
        if !data.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&data));
        }
        self.stats = RenderStats {
            instances: data.len(),
            draw_calls: runs.len(),
            uploaded_bytes: std::mem::size_of_val(data.as_slice())
                + std::mem::size_of::<GpuCamera>(),
        };
        self.runs = runs;
        Ok(self.stats)
    }
    /// Call after opaque scene geometry in the same depth-tested pass, or a load/load pass.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, &self.camera_bind, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        for run in &self.runs {
            pass.set_pipeline(match (run.blend, run.depth_test) {
                (BlendMode::Alpha, true) => &self.alpha,
                (BlendMode::Additive, true) => &self.additive,
                (BlendMode::AdditiveColor, true) => &self.additive_color,
                (BlendMode::Alpha, false) => &self.flare_alpha,
                (BlendMode::Additive, false) => &self.flare_additive,
                (BlendMode::AdditiveColor, false) => &self.flare_additive_color,
            });
            pass.set_bind_group(1, &self.images[run.texture], &[]);
            pass.draw(0..6, run.instances.clone());
        }
    }
    pub fn stats(&self) -> RenderStats {
        self.stats
    }
}
