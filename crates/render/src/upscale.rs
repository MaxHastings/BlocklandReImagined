//! Render Scale: the world drawn at fewer pixels than the window, then
//! stretched over it with bilinear filtering before the interface draws at
//! the window's own size. The copy keeps the colour as stored: the texture
//! has the frame's format, so sampling and writing it encode alike.

/// The world's size for a window of `size` at `percent` (1 to 100) of its
/// width and height, never below one pixel.
pub fn scaled_size(size: (u32, u32), percent: u32) -> (u32, u32) {
    let percent = percent.clamp(1, 100);
    let scale = |v: u32| ((u64::from(v) * u64::from(percent) + 50) / 100).max(1) as u32;
    (scale(size.0), scale(size.1))
}

pub struct Upscale {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// The world's texture, its view, the bind group reading it and its size.
    target: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup, (u32, u32))>,
}

impl Upscale {
    /// Copies into a target of `format`.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render scale"),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render scale"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render scale"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render scale"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("render scale"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            format,
            pipeline,
            layout,
            sampler,
            target: None,
        }
    }
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    /// The size the world last drew at.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.target.as_ref().map(|t| t.3)
    }
    /// The texture the world draws into, `size` pixels; made again only
    /// when the size changes.
    pub fn target(&mut self, device: &wgpu::Device, size: (u32, u32)) -> &wgpu::TextureView {
        if self.target.as_ref().is_none_or(|t| t.3 != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("render scale world"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("render scale"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.target = Some((texture, view, bind, size));
        }
        &self.target.as_ref().unwrap().1
    }
    /// Stretch the world's texture over all of `output`.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        let Some((_, _, bind, _)) = &self.target else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render scale"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

const SHADER: &str = r"
@group(0) @binding(0) var world:texture_2d<f32>;
@group(0) @binding(1) var world_sampler:sampler;
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Out {
    let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));
    return Out(vec4<f32>(uv.x*2.0-1.0,1.0-uv.y*2.0,0.0,1.0),uv);
}
@fragment fn fs_main(in:Out)->@location(0) vec4<f32> {
    return textureSample(world,world_sampler,in.uv);
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_size_rounds_and_never_reaches_zero() {
        assert_eq!(scaled_size((3024, 1964), 100), (3024, 1964));
        assert_eq!(scaled_size((3024, 1964), 50), (1512, 982));
        assert_eq!(scaled_size((1920, 1080), 85), (1632, 918));
        assert_eq!(scaled_size((1920, 1080), 70), (1344, 756));
        assert_eq!(scaled_size((1, 1), 50), (1, 1));
        // Out of range asks for the nearest the setting allows.
        assert_eq!(scaled_size((1000, 500), 250), (1000, 500));
        assert_eq!(scaled_size((1000, 500), 0), (10, 5));
    }
}
