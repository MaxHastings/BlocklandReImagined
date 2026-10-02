use crate::{Camera, CullStats, FoliageField, FoliagePack, Image};
use anyhow::{Result, ensure};
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuPlant {
    position_width: [f32; 4],
    shape: [f32; 4],
    sway: [f32; 4],
    light: [f32; 4],
    top: [f32; 4],
    bottom: [f32; 4],
    fade: [f32; 4],
    alpha: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    vp: [f32; 16],
    position: [f32; 4],
    right: [f32; 4],
    time_fog: [f32; 4],
    illumination: [f32; 4],
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct RenderStats {
    pub sources: usize,
    pub visible: usize,
    pub upload_bytes: usize,
    pub resident_instance_bytes: usize,
    pub draw_calls: usize,
    pub phase_rebases: u64,
    pub culling: Vec<CullStats>,
}
struct Run {
    texture: usize,
    range: std::ops::Range<u32>,
}
#[derive(Clone, Copy)]
pub struct RenderConfig {
    pub target: wgpu::TextureFormat,
    pub depth: wgpu::TextureFormat,
    pub samples: u32,
}
pub struct FoliageRenderer {
    illumination: [f32; 3],
    plant_buffer: wgpu::Buffer,
    original_plants: Vec<GpuPlant>,
    time_origin: f64,
    pipeline: wgpu::RenderPipeline,
    camera_layout: wgpu::BindGroupLayout,
    textures: Vec<wgpu::BindGroup>,
    /// The player's view first; others (a mirror's) made when first prepared.
    views: Vec<View>,
    fields: Vec<FoliageField>,
    offsets: Vec<u32>,
    stats: RenderStats,
}
/// One camera's visible plants.
struct View {
    uniform: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    indices: wgpu::Buffer,
    runs: Vec<Run>,
}
impl FoliageRenderer {
    /// Takes already placed fields; device, queue, target and render pass belong to the host.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pack: &FoliagePack,
        images: &[Image],
        fields: Vec<FoliageField>,
        config: RenderConfig,
    ) -> Result<Self> {
        let RenderConfig {
            target,
            depth,
            samples,
        } = config;
        pack.validate()?;
        ensure!(
            images.len() == pack.textures.len() && matches!(samples, 1 | 2 | 4 | 8 | 16),
            "invalid foliage GPU content/settings"
        );
        let total: usize = fields.iter().map(|f| f.plants().len()).sum();
        ensure!(
            total <= 500000 && fields.len() <= 128,
            "foliage GPU instance bound exceeded"
        );
        let mut data = vec![];
        let mut offsets = vec![];
        for field in &fields {
            let d = field.definition();
            ensure!(
                pack.definitions.iter().any(|p| p.id == d.id
                    && serde_json::to_value(p).ok() == serde_json::to_value(d).ok()),
                "field definition differs from pack"
            );
            offsets.push(data.len() as u32);
            for p in field.plants() {
                data.push(GpuPlant {
                    position_width: p.position.extend(p.width).to_array(),
                    shape: [
                        p.height,
                        p.angle,
                        p.flip as u8 as f32,
                        d.billboard as u8 as f32,
                    ],
                    sway: [
                        d.sway_magnitude[0],
                        d.sway_magnitude[1],
                        p.sway_phase,
                        p.sway_rate,
                    ],
                    light: [
                        p.light_phase,
                        719. / d.light_seconds,
                        d.luminance[0],
                        d.luminance[1],
                    ],
                    top: d.color_top,
                    bottom: d.color_bottom,
                    fade: [d.closest, d.distance, d.fade_near, d.fade_far],
                    alpha: [
                        d.ground_alpha,
                        d.alpha_cutoff,
                        d.sway as u8 as f32,
                        d.light as u8 as f32,
                    ],
                });
            }
        }
        let resident_instance_bytes = data.len() * 128;
        ensure!(
            resident_instance_bytes.max(128)
                <= device.limits().max_storage_buffer_binding_size as usize,
            "foliage storage exceeds device limit"
        );
        if data.is_empty() {
            data.push(bytemuck::Zeroable::zeroed());
        }
        let plant_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("immutable native foliage instances"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("foliage camera and immutable plants"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("foliage texture"),
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("original foliage atlas linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        ensure!(
            images.iter().map(|i| i.rgba.len() as u64).sum::<u64>() <= 128 << 20,
            "foliage images exceed128MiB budget"
        );
        let mut textures = Vec::new();
        for (index, image) in images.iter().enumerate() {
            ensure!(
                image.width > 0
                    && image.height > 0
                    && image.width <= device.limits().max_texture_dimension_2d
                    && image.height <= device.limits().max_texture_dimension_2d
                    && image.rgba.len() as u64
                        == u64::from(image.width) * u64::from(image.height) * 4,
                "invalid foliage GPU image"
            );
            let size = wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            };
            // Mipmapped against shimmer, keeping the alpha-tested coverage of
            // the strictest definition that uses this image.
            let cutoff = pack
                .definitions
                .iter()
                .filter(|d| d.texture == index)
                .map(|d| d.alpha_cutoff)
                .fold(0.5f32, f32::max);
            let levels = bri_render::mipmap::chain_preserving_coverage(
                image.width,
                image.height,
                &image.rgba,
                true,
                cutoff,
            );
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("original foliage RGBA"),
                size,
                mip_level_count: levels.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, (width, height, rgba)) in levels.iter().enumerate() {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    rgba.as_ref(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width * 4),
                        rows_per_image: Some(*height),
                    },
                    wgpu::Extent3d {
                        width: *width,
                        height: *height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            let view = texture.create_view(&Default::default());
            textures.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
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
            label: Some("native foliage sway light fade mask"),
            source: wgpu::ShaderSource::Wgsl(
                bri_render::color::shader_source(include_str!("foliage.wgsl")).into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("two-sided masked blended foliage depth write"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 4,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0=>Uint32],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(true),
                depth_compare: Some(bri_render::scene::DEPTH_NEARER),
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
                    constants: &bri_render::color::output_constants(target),
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let mut renderer = Self {
            illumination: [1.0; 3],
            plant_buffer,
            original_plants: data,
            time_origin: 0.,
            pipeline,
            camera_layout,
            textures,
            views: vec![],
            fields,
            offsets,
            stats: RenderStats {
                sources: total,
                resident_instance_bytes,
                ..Default::default()
            },
        };
        renderer.add_view(device);
        Ok(renderer)
    }
    fn add_view(&mut self, device: &wgpu::Device) {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("foliage camera"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.camera_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.plant_buffer.as_entire_binding(),
                },
            ],
        });
        let indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("visible foliage indices only"),
            size: (self.stats.sources.max(1) * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.views.push(View {
            uniform,
            camera_bind,
            indices,
            runs: vec![],
        });
    }
    /// Environment light shared by the player's view, mirrors and probes.
    pub fn set_illumination(&mut self, illumination: [f32; 3]) -> Result<()> {
        ensure!(
            illumination.iter().all(|v| v.is_finite() && *v >= 0.0),
            "Invalid foliage illumination"
        );
        self.illumination = illumination;
        Ok(())
    }
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        camera: &Camera,
        seconds: f32,
        fog_start: f32,
        fog_end: f32,
    ) -> Result<RenderStats> {
        self.prepare_elapsed(queue, camera, f64::from(seconds), fog_start, fog_end)
    }

    /// Rebase per-instance phases every ten minutes. Shader time stays small,
    /// while authored differing rates retain continuity across long sessions.
    pub fn prepare_elapsed(
        &mut self,
        queue: &wgpu::Queue,
        camera: &Camera,
        seconds: f64,
        fog_start: f32,
        fog_end: f32,
    ) -> Result<RenderStats> {
        camera.validate()?;
        ensure!(
            seconds.is_finite()
                && (0. ..=1.0e12).contains(&seconds)
                && fog_start.is_finite()
                && fog_end.is_finite()
                && fog_end > fog_start,
            "invalid foliage time/fog"
        );
        let origin = (seconds / 600.).floor() * 600.;
        let mut rebase_bytes = 0;
        if origin != self.time_origin {
            let mut data = self.original_plants.clone();
            for plant in &mut data {
                plant.sway[2] = phase_at(plant.sway[2], plant.sway[3], origin);
                plant.light[0] = phase_at(plant.light[0], plant.light[1], origin);
            }
            queue.write_buffer(&self.plant_buffer, 0, bytemuck::cast_slice(&data));
            rebase_bytes = data.len() * std::mem::size_of::<GpuPlant>();
            self.time_origin = origin;
            self.stats.phase_rebases += 1;
        }
        let (visible, culling) = self.write(queue, 0, camera, seconds, fog_start, fog_end)?;
        self.stats.visible = visible;
        self.stats.upload_bytes = std::mem::size_of::<Uniform>() + visible * 4 + rebase_bytes;
        self.stats.draw_calls = self.views[0].runs.len();
        self.stats.culling = culling;
        Ok(self.stats.clone())
    }
    /// Another view of the same plants (a mirror's): view 1 and up, drawn by
    /// [`Self::render_view`], at the `seconds` the player's view last used.
    /// Returns the plants it sees.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: usize,
        camera: &Camera,
        seconds: f64,
        fog_start: f32,
        fog_end: f32,
    ) -> Result<usize> {
        camera.validate()?;
        ensure!(view >= 1, "View 0 is the player's");
        // Views past the ones in use (an environment probe's after the
        // mirrors') leave the ones between empty until they are prepared.
        while self.views.len() <= view {
            self.add_view(device);
        }
        Ok(self
            .write(queue, view, camera, seconds, fog_start, fog_end)?
            .0)
    }
    /// Cull for `camera` and upload view `view`'s visible plants and uniform.
    fn write(
        &mut self,
        queue: &wgpu::Queue,
        view: usize,
        camera: &Camera,
        seconds: f64,
        fog_start: f32,
        fog_end: f32,
    ) -> Result<(usize, Vec<CullStats>)> {
        let mut indices = vec![];
        let mut visible = vec![];
        let mut runs = vec![];
        let mut culling = vec![];
        for (i, field) in self.fields.iter().enumerate() {
            let stats = field.visible(camera, &mut visible)?;
            let start = indices.len() as u32;
            indices.extend(visible.iter().map(|n| n + self.offsets[i]));
            if indices.len() > start as usize {
                runs.push(Run {
                    texture: field.definition().texture,
                    range: start..indices.len() as u32,
                });
            }
            culling.push(stats);
        }
        let uniform = Uniform {
            vp: camera.view_projection.to_cols_array(),
            position: camera.position.extend(1.).to_array(),
            right: camera.right.extend(0.).to_array(),
            time_fog: [(seconds - self.time_origin) as f32, fog_start, fog_end, 0.],
            illumination: [
                self.illumination[0],
                self.illumination[1],
                self.illumination[2],
                0.,
            ],
        };
        let target = &mut self.views[view];
        queue.write_buffer(&target.uniform, 0, bytemuck::bytes_of(&uniform));
        if !indices.is_empty() {
            queue.write_buffer(&target.indices, 0, bytemuck::cast_slice(&indices));
        }
        target.runs = runs;
        Ok((indices.len(), culling))
    }
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.render_view(pass, 0);
    }
    /// [`Self::render`] for a view [`Self::prepare_view`] prepared; nothing
    /// for one it did not.
    pub fn render_view(&self, pass: &mut wgpu::RenderPass<'_>, view: usize) {
        let Some(view) = self.views.get(view).filter(|v| !v.runs.is_empty()) else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &view.camera_bind, &[]);
        pass.set_vertex_buffer(0, view.indices.slice(..));
        for run in &view.runs {
            pass.set_bind_group(1, &self.textures[run.texture], &[]);
            pass.draw(0..6, run.range.clone());
        }
    }
    pub fn stats(&self) -> &RenderStats {
        &self.stats
    }
}

fn phase_at(initial: f32, rate: f32, seconds: f64) -> f32 {
    (f64::from(initial) + f64::from(rate) * seconds).rem_euclid(720.) as f32
}

#[cfg(test)]
mod phase_tests {
    use super::*;
    #[test]
    fn rebase_preserves_distinct_authored_rates_after_days() {
        for rate in [0.13, 7.19, 143.8, 719.] {
            for seconds in [599.9_f64, 600.1, 86400.125, 604800.375] {
                let origin = (seconds / 600.).floor() * 600.;
                let rebased = phase_at(phase_at(113.5, rate, origin), rate, seconds - origin);
                let absolute = phase_at(113.5, rate, seconds);
                let error = (rebased - absolute).abs();
                assert!(error.min((720. - error).abs()) < 0.001);
            }
        }
    }
}
