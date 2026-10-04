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
    /// Layer of the effects texture array, the texture's own size, and the
    /// sprite's blend mode (0 alpha, 1 additive, 2 additive colour).
    image: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuCamera {
    view_projection: [[f32; 4]; 4],
    right: [f32; 4],
    up: [f32; 4],
    position: [f32; 4],
    /// The scene's fog (`Camera::atmosphere` and `fog_color` of bri-render).
    atmosphere: [f32; 4],
    fog_color: [f32; 4],
}
/// Consecutive sprites that share a pipeline draw together. Every texture
/// is a layer of one array and every blend mode is one premultiplied blend,
/// so only flares (drawn over everything) split a run.
struct Run {
    depth_test: bool,
    instances: Range<u32>,
}
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct RenderStats {
    pub instances: usize,
    pub draw_calls: usize,
    pub uploaded_bytes: usize,
}
/// One camera's sprites: the player's view, or another view of the same
/// world (a mirror's) with its own camera, culling and order.
struct View {
    camera: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
    runs: Vec<Run>,
    stats: RenderStats,
}
/// The most sprite instances one renderer accepts: one world at its largest
/// [`crate::EffectsLimits`] (a million particles and 4096 light flares).
pub const MAX_INSTANCES: usize = 1_004_096;
pub struct EffectsRenderer {
    sprites: wgpu::RenderPipeline,
    flares: wgpu::RenderPipeline,
    camera_layout: wgpu::BindGroupLayout,
    images: wgpu::BindGroup,
    /// Per texture: its layer and size, as the instance data carries them.
    layers: Vec<[f32; 4]>,
    /// The player's view first; others are made when first prepared.
    views: Vec<View>,
    max_instances: usize,
    /// This frame's instance data, kept to reuse its allocation.
    data: Vec<GpuParticle>,
    /// The scene's fog: atmosphere and colour, as [`Self::set_fog`] gave them.
    fog: [[f32; 4]; 2],
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
                && max_instances <= MAX_INSTANCES
                && matches!(sample_count, 1 | 2 | 4 | 8 | 16),
            "Invalid effects GPU limits"
        );
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effects camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
            label: Some("effects linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Every original texture is one layer of an array as large as the
        // largest, copied to the layer's corner without resampling; the
        // shader samples only that corner, clamped as the texture alone was.
        let width = pack
            .textures
            .iter()
            .map(|i| i.width)
            .max()
            .unwrap_or(1)
            .max(1);
        let height = pack
            .textures
            .iter()
            .map(|i| i.height)
            .max()
            .unwrap_or(1)
            .max(1);
        let count = pack.textures.len().max(1) as u32;
        ensure!(
            width <= device.limits().max_texture_dimension_2d
                && height <= device.limits().max_texture_dimension_2d
                && count <= device.limits().max_texture_array_layers,
            "Effect textures exceed device limits"
        );
        let array = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("original effect textures"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: count,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut layers = Vec::with_capacity(pack.textures.len());
        for (layer, image) in pack.textures.iter().enumerate() {
            ensure!(
                image.width > 0
                    && image.height > 0
                    && image.rgba.len() == (image.width * image.height * 4) as usize,
                "Invalid effect texture {}",
                image.id
            );
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &array,
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
            layers.push([layer as f32, image.width as f32, image.height as f32, 0.]);
        }
        let view = array.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let images = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("original effect textures"),
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
            label: Some("native effects billboards"),
            source: wgpu::ShaderSource::Wgsl(
                bri_render::color::shader_source(include_str!("particles.wgsl")).into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("effects"),
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = |blend: wgpu::BlendState, depth_test: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("native effects"),layout:Some(&layout),vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs_main"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout {array_stride:std::mem::size_of::<GpuParticle>() as u64,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4]})]},
            primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},depth_stencil:Some(wgpu::DepthStencilState {format:depth_format,depth_write_enabled:Some(false),depth_compare:Some(if depth_test {bri_render::scene::DEPTH_NEARER}else{wgpu::CompareFunction::Always}),stencil:Default::default(),bias:Default::default()}),multisample:wgpu::MultisampleState {count:sample_count,..Default::default()},
            fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs_main"),compilation_options:wgpu::PipelineCompilationOptions {constants:&bri_render::color::output_constants(color_format),..Default::default()},targets:&[Some(wgpu::ColorTargetState {format:color_format,blend:Some(blend),write_mask:wgpu::ColorWrites::ALL})]}),multiview_mask:None,cache:None,
        })
        };
        // Premultiplied blending (One, OneMinusSrcAlpha) draws all three
        // original modes: the shader writes (rgb * a, a) for alpha blending,
        // (rgb * a, 0) for additive and (rgb, 0) for additive colour, the
        // same colours the separate blend states produced.
        let premultiplied = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let sprites = pipeline(premultiplied, true);
        let flares = pipeline(premultiplied, false);
        let view = Self::view(device, &camera_layout, max_instances);
        Ok(Self {
            sprites,
            flares,
            camera_layout,
            images,
            layers,
            views: vec![view],
            max_instances,
            data: Vec::new(),
            fog: [[0.0; 4]; 2],
        })
    }
    fn view(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, capacity: usize) -> View {
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("effects camera"),
            size: std::mem::size_of::<GpuCamera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effects camera"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded effect instances"),
            size: (capacity * std::mem::size_of::<GpuParticle>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        View {
            camera,
            camera_bind,
            instances,
            capacity,
            runs: Vec::new(),
            stats: RenderStats::default(),
        }
    }
    /// Fog sprites as the scene is fogged: bri-render's `Camera::atmosphere`
    /// and `fog_color`. Every view prepared afterwards uses it.
    pub fn set_fog(&mut self, atmosphere: [f32; 4], fog_color: [f32; 4]) {
        self.fog = [atmosphere, fog_color];
    }
    /// The player's view: `camera` and the sprites it sees.
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        camera: &Camera,
        frame: &FrameEffects,
    ) -> Result<RenderStats> {
        self.write(queue, 0, camera, frame)
    }
    /// Another view of the same effects (a mirror's): view 1 and up, drawn
    /// by [`Self::render_view`]. Its sprites come from a snapshot for its
    /// own camera, so they face it and sort for it.
    pub fn prepare_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: usize,
        camera: &Camera,
        frame: &FrameEffects,
    ) -> Result<RenderStats> {
        ensure!(view >= 1, "View 0 is the player's");
        // Grown to what this view needs, not the player's full budget.
        let needed = frame
            .particles
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
            self.views
                .push(Self::view(device, &self.camera_layout, needed));
        } else if self.views[view].capacity < frame.particles.len() {
            self.views[view] = Self::view(device, &self.camera_layout, needed);
        }
        self.write(queue, view, camera, frame)
    }
    fn write(
        &mut self,
        queue: &wgpu::Queue,
        view: usize,
        camera: &Camera,
        frame: &FrameEffects,
    ) -> Result<RenderStats> {
        ensure!(
            frame.particles.len() <= self.views[view].capacity,
            "Effects frame exceeds GPU instance budget"
        );
        ensure!(
            camera.view_projection.is_finite()
                && camera.position.is_finite()
                && camera.right.is_normalized()
                && camera.up.is_normalized(),
            "Invalid effects camera"
        );
        let mut data = std::mem::take(&mut self.data);
        data.clear();
        let mut runs: Vec<Run> = Vec::new();
        for (i, p) in frame.particles.iter().enumerate() {
            ensure!(
                (p.texture as usize) < self.layers.len()
                    && p.position.is_finite()
                    && p.color.is_finite()
                    && p.size.is_finite()
                    && p.size >= 0.
                    && p.axis.is_finite()
                    && p.spin.is_finite(),
                "Invalid effects instance {i}: texture={} layers={} position={:?} color={:?} size={} axis={:?} spin={}",
                p.texture,
                self.layers.len(),
                p.position,
                p.color,
                p.size,
                p.axis,
                p.spin
            );
            let mut image = self.layers[p.texture as usize];
            image[3] = match p.blend {
                BlendMode::Alpha => 0.,
                BlendMode::Additive => 1.,
                BlendMode::AdditiveColor => 2.,
            };
            data.push(GpuParticle {
                position_size: p.position.extend(p.size).to_array(),
                color: p.color.to_array(),
                axis_spin: p.axis.extend(p.spin).to_array(),
                image,
            });
            if let Some(run) = runs.last_mut()
                && run.depth_test == p.depth_test
            {
                run.instances.end += 1;
            } else {
                runs.push(Run {
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
            atmosphere: self.fog[0],
            fog_color: self.fog[1],
        };
        let target = &mut self.views[view];
        queue.write_buffer(&target.camera, 0, bytemuck::bytes_of(&uniform));
        if !data.is_empty() {
            queue.write_buffer(&target.instances, 0, bytemuck::cast_slice(&data));
        }
        target.stats = RenderStats {
            instances: data.len(),
            draw_calls: runs.len(),
            uploaded_bytes: std::mem::size_of_val(data.as_slice())
                + std::mem::size_of::<GpuCamera>(),
        };
        target.runs = runs;
        self.data = data;
        Ok(target.stats)
    }
    /// Call after opaque scene geometry in the same depth-tested pass, or a load/load pass.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.render_view(pass, 0);
    }
    /// [`Self::render`] for a view [`Self::prepare_view`] prepared; nothing
    /// for one it did not.
    pub fn render_view(&self, pass: &mut wgpu::RenderPass<'_>, view: usize) {
        let Some(view) = self.views.get(view) else {
            return;
        };
        pass.set_bind_group(0, &view.camera_bind, &[]);
        pass.set_bind_group(1, &self.images, &[]);
        pass.set_vertex_buffer(0, view.instances.slice(..));
        for run in &view.runs {
            pass.set_pipeline(if run.depth_test {
                &self.sprites
            } else {
                &self.flares
            });
            pass.draw(0..6, run.instances.clone());
        }
    }
    /// The most sprites one view draws; a frame must be cut to this first.
    pub fn max_instances(&self) -> usize {
        self.max_instances
    }
    /// The player's view.
    pub fn stats(&self) -> RenderStats {
        self.views[0].stats
    }
}
