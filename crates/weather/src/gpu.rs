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
/// One camera's drops: the player's view, or another view of the same
/// weather (a mirror's) with its own camera and facing.
struct View {
    uniform: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    capacity: usize,
    stats: WeatherRenderStats,
}
pub struct WeatherRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_layout: wgpu::BindGroupLayout,
    texture_bind: wgpu::BindGroup,
    texture_scales: Vec<[f32; 2]>,
    /// The player's view first; others are made when first prepared.
    views: Vec<View>,
    max_instances: usize,
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
            source: wgpu::ShaderSource::Wgsl(bri_render::color::shader_source(include_str!("weather.wgsl")).into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weather"),
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {label:Some("weather alpha depth read"),layout:Some(&layout),vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs_main"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout {array_stride:80,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4]})]},primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},depth_stencil:Some(wgpu::DepthStencilState {format:depth,depth_write_enabled:Some(false),depth_compare:Some(bri_render::scene::DEPTH_NEARER),stencil:Default::default(),bias:Default::default()}),multisample:wgpu::MultisampleState {count:samples,..Default::default()},fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs_main"),compilation_options:wgpu::PipelineCompilationOptions {constants:&bri_render::color::output_constants(target),..Default::default()},targets:&[Some(wgpu::ColorTargetState {format:target,blend:Some(wgpu::BlendState::ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),multiview_mask:None,cache:None});
        let view = Self::view(device, &camera_layout, max_instances);
        Ok(Self {
            pipeline,
            camera_layout,
            texture_bind,
            texture_scales,
            views: vec![view],
            max_instances,
        })
    }
    fn view(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, capacity: usize) -> View {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weather camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather camera"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded weather instances"),
            size: (capacity * 80) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        View {
            uniform,
            camera_bind,
            buffer,
            capacity,
            stats: WeatherRenderStats::default(),
        }
    }
    /// The player's view.
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        frame: &WeatherFrame,
    ) -> Result<WeatherRenderStats> {
        self.write(queue, 0, view_projection, frame)
    }
    /// Another view of the same weather (a mirror's): view 1 and up, drawn
    /// by [`Self::render_view`], with `frame` snapshot for its own camera
    /// (`WeatherWorld::snapshot_from`).
    pub fn prepare_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: usize,
        view_projection: Mat4,
        frame: &WeatherFrame,
    ) -> Result<WeatherRenderStats> {
        ensure!(view >= 1, "View 0 is the player's");
        // Grown to what this view needs, not the player's full budget.
        let needed = frame
            .instances
            .len()
            .max(256)
            .next_power_of_two()
            .min(self.max_instances);
        // Views past the ones in use (an environment probe's after the
        // mirrors') leave the ones between empty until they are prepared.
        while self.views.len() < view {
            let empty = Self::view(device, &self.camera_layout, 256.min(self.max_instances));
            self.views.push(empty);
        }
        if view == self.views.len() {
            self.views.push(Self::view(device, &self.camera_layout, needed));
        } else if self.views[view].capacity < frame.instances.len() {
            self.views[view] = Self::view(device, &self.camera_layout, needed);
        }
        self.write(queue, view, view_projection, frame)
    }
    fn write(
        &mut self,
        queue: &wgpu::Queue,
        view: usize,
        view_projection: Mat4,
        frame: &WeatherFrame,
    ) -> Result<WeatherRenderStats> {
        ensure!(
            view_projection.is_finite() && frame.instances.len() <= self.views[view].capacity,
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
        let target = &mut self.views[view];
        queue.write_buffer(
            &target.uniform,
            0,
            bytemuck::cast_slice(&view_projection.to_cols_array()),
        );
        if !data.is_empty() {
            queue.write_buffer(&target.buffer, 0, bytemuck::cast_slice(&data));
        }
        target.stats = WeatherRenderStats {
            instances: data.len(),
            draw_calls: usize::from(!data.is_empty()),
            upload_bytes: data.len() * 80 + 64,
        };
        Ok(target.stats)
    }
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.render_view(pass, 0);
    }
    /// [`Self::render`] for a view [`Self::prepare_view`] prepared; nothing
    /// for one it did not.
    pub fn render_view(&self, pass: &mut wgpu::RenderPass<'_>, view: usize) {
        let Some(view) = self.views.get(view).filter(|v| v.stats.instances > 0) else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &view.camera_bind, &[]);
        pass.set_vertex_buffer(0, view.buffer.slice(..));
        pass.set_bind_group(1, &self.texture_bind, &[]);
        pass.draw(0..6, 0..view.stats.instances as u32);
    }
    /// The player's view.
    pub fn stats(&self) -> WeatherRenderStats {
        self.views[0].stats
    }
}
