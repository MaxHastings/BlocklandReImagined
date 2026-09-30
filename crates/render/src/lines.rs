//! Opaque one-pixel world lines, unlit and unfogged: v20's outlines of
//! non-rendering bricks (`fxDTSBrick::renderObject` draws GL line loops in
//! the brick's paint colour). The line set is uploaded once when it
//! changes; each frame only the camera is written.
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    pub position: [f32; 3],
    /// Display-encoded colour, like brick paint.
    pub color: [f32; 3],
}

/// The 12 edges of the box from `min` to `max`, as 24 line-list vertices.
pub fn box_edges(min: Vec3, max: Vec3, color: [f32; 3], out: &mut Vec<LineVertex>) {
    let corner = |i: usize| {
        Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        )
    };
    for a in 0..8 {
        for bit in [1, 2, 4] {
            if a & bit == 0 {
                for i in [a, a | bit] {
                    out.push(LineVertex {
                        position: corner(i).to_array(),
                        color,
                    });
                }
            }
        }
    }
}

pub struct LineRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    buffer: Option<wgpu::Buffer>,
    vertices: u32,
}

impl LineRenderer {
    pub fn new(
        device: &wgpu::Device,
        target: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("line camera"),
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
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("line camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("line camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world lines"),
            source: wgpu::ShaderSource::Wgsl(
                crate::color::shader_source(include_str!("lines.wgsl")).into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world lines"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world lines"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(false),
                depth_compare: Some(crate::scene::DEPTH_NEARER),
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
                    constants: &crate::color::output_constants(target),
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            uniform,
            camera_bind,
            buffer: None,
            vertices: 0,
        }
    }
    /// Replace the drawn lines (pairs of vertices).
    pub fn set_lines(&mut self, device: &wgpu::Device, vertices: &[LineVertex]) -> Result<()> {
        use wgpu::util::DeviceExt;
        ensure!(
            vertices.len().is_multiple_of(2) && u32::try_from(vertices.len()).is_ok(),
            "Invalid world line list"
        );
        self.vertices = vertices.len() as u32;
        self.buffer = (!vertices.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("world lines"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        Ok(())
    }
    pub fn clear(&mut self) {
        self.buffer = None;
        self.vertices = 0;
    }
    pub fn is_empty(&self) -> bool {
        self.vertices == 0
    }
    pub fn prepare(&self, queue: &wgpu::Queue, view_projection: Mat4) {
        if !self.is_empty() {
            queue.write_buffer(
                &self.uniform,
                0,
                bytemuck::cast_slice(&view_projection.to_cols_array()),
            );
        }
    }
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(buffer) = &self.buffer else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..self.vertices, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_edges_are_the_twelve_axis_edges() {
        let mut out = vec![];
        box_edges(
            Vec3::ZERO,
            Vec3::new(1.0, 2.0, 3.0),
            [1.0, 0.0, 0.0],
            &mut out,
        );
        assert_eq!(out.len(), 24);
        let mut per_axis = [0; 3];
        for pair in out.chunks_exact(2) {
            let d = Vec3::from(pair[1].position) - Vec3::from(pair[0].position);
            let axis = (0..3).find(|a| d[*a] != 0.0).unwrap();
            assert_eq!(d.abs().to_array().iter().filter(|v| **v != 0.0).count(), 1);
            assert!(d[axis] > 0.0);
            per_axis[axis] += 1;
        }
        assert_eq!(per_axis, [4, 4, 4]);
    }
}
