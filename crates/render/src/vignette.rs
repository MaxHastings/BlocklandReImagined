//! A full-screen vignette over the world (v21's Environment "Vignette"):
//! the colour weighs in toward the screen's edges, blended over the frame
//! or multiplied into it. One triangle, drawn only while one is set.
use crate::color::{output_constants, shader_source};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    color: [f32; 4],
    params: [f32; 4],
}

pub struct VignetteRenderer {
    over: wgpu::RenderPipeline,
    multiply: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    /// Multiply mode, while a vignette is set.
    active: Option<bool>,
}

impl VignetteRenderer {
    /// Drawn in a pass with a `depth` attachment and `samples` samples,
    /// which it neither tests nor writes.
    pub fn new(
        device: &wgpu::Device,
        target: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vignette"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vignette"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vignette"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vignette"),
            source: wgpu::ShaderSource::Wgsl(shader_source(include_str!("vignette.wgsl")).into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vignette"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let constants = output_constants(target);
        let pipeline = |blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("vignette"),
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
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let over = pipeline(wgpu::BlendState::ALPHA_BLENDING);
        let multiply = pipeline(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        });
        Self {
            over,
            multiply,
            uniform,
            bind,
            active: None,
        }
    }
    /// The vignette to draw (display-encoded colour, alpha its strength),
    /// or none; `aspect` is the frame's width over height.
    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        vignette: Option<([f32; 4], bool)>,
        aspect: f32,
    ) {
        self.active = None;
        let Some((color, multiply)) = vignette else {
            return;
        };
        if !color.iter().all(|c| c.is_finite()) || color[3] <= 0.0 {
            return;
        }
        let uniform = Uniform {
            color,
            params: [aspect.max(0.01), f32::from(u8::from(multiply)), 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        self.active = Some(multiply);
    }
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(multiply) = self.active else {
            return;
        };
        pass.set_pipeline(if multiply { &self.multiply } else { &self.over });
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}
