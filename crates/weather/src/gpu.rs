use crate::{WeatherFrame, WeatherPack};
use anyhow::{Result, ensure};
use glam::Mat4;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    position: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
}
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct WeatherRenderStats {
    pub instances: usize,
    pub draw_calls: usize,
    pub upload_bytes: usize,
}
pub struct WeatherRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    texture_bind: wgpu::BindGroup,
    texture_scales: Vec<[f32; 2]>,
    buffer: wgpu::Buffer,
    capacity: usize,
    stats: WeatherRenderStats,
}
impl WeatherRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pack: &WeatherPack,
        target: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        samples: u32,
        max_instances: usize,
    ) -> Result<Self> {
        ensure!(
            max_instances > 0 && max_instances <= 131072 && matches!(samples, 1 | 2 | 4 | 8 | 16),
            "Invalid weather GPU limits"
        );
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("weather camera"),
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
            label: Some("weather atlas"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weather camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("original weather atlas linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let width = pack.textures.iter().map(|t| t.width).max().unwrap_or(1);
        let height = pack.textures.iter().map(|t| t.height).max().unwrap_or(1);
        let layers = pack.textures.len().max(1) as u32;
        ensure!(
            width <= device.limits().max_texture_dimension_2d
                && height <= device.limits().max_texture_dimension_2d
                && layers <= device.limits().max_texture_array_layers
                && u64::from(width) * u64::from(height) * u64::from(layers) * 4 <= 128 << 20,
            "Weather texture array exceeds budget/device limits"
        );
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("weather original atlas array"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut texture_scales = Vec::new();
        for (layer, image) in pack.textures.iter().enumerate() {
            // No resizing: source texels occupy a rectangle in an otherwise transparent layer.
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &atlas,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &image.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(image.width * 4),
                    rows_per_image: Some(image.height),
                },
                wgpu::Extent3d {
                    width: image.width,
                    height: image.height,
                    depth_or_array_layers: 1,
                },
            );
            texture_scales.push([
                image.width as f32 / width as f32,
                image.height as f32 / height as f32,
            ]);
        }
        let view = atlas.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let texture_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather atlas array"),
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
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("native rain snow and splash atlases"),
            source: wgpu::ShaderSource::Wgsl(include_str!("weather.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weather"),
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {label:Some("weather alpha depth read"),layout:Some(&layout),vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs_main"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout {array_stride:80,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4]})]},primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},depth_stencil:Some(wgpu::DepthStencilState {format:depth,depth_write_enabled:Some(false),depth_compare:Some(wgpu::CompareFunction::LessEqual),stencil:Default::default(),bias:Default::default()}),multisample:wgpu::MultisampleState {count:samples,..Default::default()},fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs_main"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState {format:target,blend:Some(wgpu::BlendState::ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),multiview_mask:None,cache:None});
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded weather instances"),
            size: (max_instances * 80) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            pipeline,
            uniform,
            camera_bind,
            texture_bind,
            texture_scales,
            buffer,
            capacity: max_instances,
            stats: WeatherRenderStats::default(),
        })
    }
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        frame: &WeatherFrame,
    ) -> Result<WeatherRenderStats> {
        ensure!(
            view_projection.is_finite() && frame.instances.len() <= self.capacity,
            "Invalid weather render frame"
        );
        let mut data = Vec::with_capacity(frame.instances.len());
        for p in &frame.instances {
            ensure!(
                (p.texture as usize) < self.texture_scales.len()
                    && p.position.is_finite()
                    && p.right.is_finite()
                    && p.up.is_finite()
                    && p.uv.iter().all(|v| v.is_finite() && (0.0..=1.).contains(v))
                    && p.color.iter().all(|v| v.is_finite()),
                "Invalid weather instance"
            );
            data.push(Instance {
                position: p.position.extend(p.texture as f32).to_array(),
                right: p
                    .right
                    .extend(self.texture_scales[p.texture as usize][0])
                    .to_array(),
                up: p
                    .up
                    .extend(self.texture_scales[p.texture as usize][1])
                    .to_array(),
                uv: p.uv,
                color: p.color,
            });
        }
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::cast_slice(&view_projection.to_cols_array()),
        );
        if !data.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&data));
        }
        self.stats = WeatherRenderStats {
            instances: data.len(),
            draw_calls: usize::from(!data.is_empty()),
            upload_bytes: data.len() * 80 + 64,
        };
        Ok(self.stats)
    }
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.set_bind_group(1, &self.texture_bind, &[]);
        if self.stats.instances > 0 {
            pass.draw(0..6, 0..self.stats.instances as u32);
        }
    }
    pub fn stats(&self) -> WeatherRenderStats {
        self.stats
    }
}
