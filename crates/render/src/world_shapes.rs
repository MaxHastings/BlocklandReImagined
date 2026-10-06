//! Translucent boxes Add-Ons draw in the world (`show_shapes`), unlit and
//! unfogged: each face in one colour seen from outside and another seen
//! from inside, as Torque Add-Ons draw scaled `StaticShape`s whose faces
//! point out or in (the New Duplicator's selection box). The set is
//! uploaded once when it changes; each frame only the camera is written.
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShapeVertex {
    pub position: [f32; 3],
    /// The face's outward normal.
    pub normal: [f32; 3],
    /// Display-encoded RGBA seen from outside, and from inside.
    pub outside: [f32; 4],
    pub inside: [f32; 4],
}

/// The six faces of the box from `min` to `max` as triangle-list vertices:
/// those across each axis `outside[axis]` seen from outside, all `inside`
/// from within, straight RGBA from 0 to 1. Faces with neither are left out.
pub fn box_faces(
    min: Vec3,
    max: Vec3,
    outside: [[f32; 4]; 3],
    inside: [f32; 4],
    out: &mut Vec<ShapeVertex>,
) {
    for axis in 0..3 {
        let outside = outside[axis];
        if outside[3] <= 0.0 && inside[3] <= 0.0 {
            continue;
        }
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for (side, at) in [(-1.0, min[axis]), (1.0, max[axis])] {
            let corner = |a: bool, b: bool| {
                let mut p = Vec3::ZERO;
                p[axis] = at;
                p[u] = if a { max[u] } else { min[u] };
                p[v] = if b { max[v] } else { min[v] };
                p.to_array()
            };
            let mut normal = [0.0; 3];
            normal[axis] = side;
            let quad = [
                corner(false, false),
                corner(true, false),
                corner(true, true),
                corner(false, true),
            ];
            for i in [0, 1, 2, 0, 2, 3] {
                out.push(ShapeVertex {
                    position: quad[i],
                    normal,
                    outside,
                    inside,
                });
            }
        }
    }
}

pub struct ShapeRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    buffer: Option<wgpu::Buffer>,
    vertices: u32,
}

impl ShapeRenderer {
    pub fn new(
        device: &wgpu::Device,
        target: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world shapes camera"),
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
            label: Some("world shapes camera"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world shapes camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world shapes"),
            source: wgpu::ShaderSource::Wgsl(
                crate::color::shader_source(include_str!("world_shapes.wgsl")).into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world shapes"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world shapes"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ShapeVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x4
                    ],
                })],
            },
            // Both sides: the vertex shader picks the colour for the side
            // the eye is on.
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
    /// Replace the drawn faces (triangles).
    pub fn set_faces(&mut self, device: &wgpu::Device, vertices: &[ShapeVertex]) -> Result<()> {
        use crate::BufferInit;
        ensure!(
            vertices.len().is_multiple_of(3) && u32::try_from(vertices.len()).is_ok(),
            "Invalid world shape faces"
        );
        self.vertices = vertices.len() as u32;
        self.buffer = (!vertices.is_empty()).then(|| {
            device.buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("world shapes"),
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
    pub fn prepare(&self, queue: &wgpu::Queue, view_projection: Mat4, eye: Vec3) {
        if !self.is_empty() {
            let mut camera = [0.0_f32; 20];
            camera[..16].copy_from_slice(&view_projection.to_cols_array());
            camera[16..19].copy_from_slice(&eye.to_array());
            queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&camera));
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
    fn box_faces_are_six_outward_quads() {
        let mut out = vec![];
        let (min, max) = (Vec3::new(0.0, 1.0, 2.0), Vec3::new(1.0, 3.0, 5.0));
        box_faces(
            min,
            max,
            [[0.0, 0.0, 0.0, 0.35]; 3],
            [0.0, 0.0, 0.0, 0.6],
            &mut out,
        );
        assert_eq!(out.len(), 36);
        let centre = (min + max) / 2.0;
        for v in &out {
            let p = Vec3::from(v.position);
            let n = Vec3::from(v.normal);
            assert_eq!(n.length(), 1.0);
            // On the box, and the normal points away from its centre.
            assert!(p.cmpge(min).all() && p.cmple(max).all());
            assert!(n.dot(p - centre) > 0.0);
        }
        let mut none = vec![];
        box_faces(min, max, [[1.0, 1.0, 1.0, 0.0]; 3], [0.0; 4], &mut none);
        assert!(none.is_empty());
        // A shaded cube: only the faces across y drawn here.
        let mut shaded = vec![];
        let clear = [0.0; 4];
        box_faces(min, max, [clear, [1.0; 4], clear], clear, &mut shaded);
        assert_eq!(shaded.len(), 12);
        assert!(shaded.iter().all(|v| v.normal[1] != 0.0));
    }
}
