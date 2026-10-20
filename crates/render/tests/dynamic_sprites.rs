//! Dynamic textured batches: how a texel, the vertex colour, the blend factors, the alpha test and the
//! fog combine, read back from the centre pixel of a cleared (0, 0, 1) target.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::dynamic_sprites::*;

type Edit = fn(&mut State);
/// A quad's depth, vertex colour and state edit.
type Quad = (f32, [f32; 4], Edit);

const BACKDROP: [u8; 3] = [0, 0, 255];

fn quad(z: f32, color: [f32; 4]) -> (Vec<SpriteVertex>, Vec<u32>) {
    let corners = [
        (-1., -1., 0., 1.),
        (1., -1., 1., 1.),
        (1., 1., 1., 0.),
        (-1., 1., 0., 0.),
    ];
    let vertices = corners
        .map(|(x, y, u, v)| SpriteVertex {
            position: [x, y, z],
            color,
            uv: [u, v],
            uv2: [u, v],
        })
        .to_vec();
    (vertices, vec![0, 1, 2, 0, 2, 3])
}

/// Draw the quads (each: z, vertex colour, state builder) in order; the centre pixel.
fn centre(texel: [u8; 4], quads: &[Quad]) -> Result<[u8; 3]> {
    pixel(texel, quads, 16, 16)
}

/// The same, reading the pixel at column `x`, row `y`.
fn pixel(texel: [u8; 4], quads: &[Quad], x: usize, y: usize) -> Result<[u8; 3]> {
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = common::color_target(&device, format, 32, 32);
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut sprites = DynamicSprites::new(&device, format, wgpu::TextureFormat::Depth32Float, 1);
    let image = sprites.add_image(&device, &queue, &texel, 1, 1, true);
    let mut vertices = Vec::new();
    let mut lists = Vec::new();
    for (z, color, edit) in quads {
        let (v, i) = quad(*z, *color);
        let base = vertices.len() as u32;
        vertices.extend(v);
        let mut state = State::new(image);
        edit(&mut state);
        lists.push((state, i.into_iter().map(|i| i + base).collect::<Vec<_>>()));
    }
    let batches: Vec<Batch> = lists
        .iter()
        .map(|(state, indices)| Batch {
            state: *state,
            indices,
        })
        .collect();
    // The eye is at the origin looking down -Z: the view matrix is the identity.
    let projection = bri_render::scene::perspective(1.2, 1.0, 0.1, 100.0);
    sprites.prepare(&device, &queue, projection, &vertices, &batches);
    let color_view = target.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dynamic sprites test"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(BACKDROP[0]) / 255.0,
                        g: f64::from(BACKDROP[1]) / 255.0,
                        b: f64::from(BACKDROP[2]) / 255.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        sprites.render(&mut pass);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: 256 * 32,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(32),
            },
        },
        wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely())?;
    let data = buffer.slice(..).get_mapped_range()?;
    let at = y * 256 + x * 4;
    Ok([data[at], data[at + 1], data[at + 2]])
}

fn near(got: [u8; 3], want: [u8; 3]) {
    for k in 0..3 {
        assert!(
            (i32::from(got[k]) - i32::from(want[k])).abs() <= 2,
            "got {got:?} want {want:?}"
        );
    }
}

#[test]
fn texel_vertex_blend_alpha_test_and_fog_combine_like_a_fixed_function_pipeline() -> Result<()> {
    let white = [1., 1., 1., 1.];
    // Opaque: texel x vertex.
    near(
        centre([255, 0, 0, 255], &[(-5., white, |_| {})])?,
        [255, 0, 0],
    );
    near(
        centre([255, 0, 0, 255], &[(-5., [0.5, 0.5, 0.5, 1.], |_| {})])?,
        [128, 0, 0],
    );
    // x2 and x4 combines clamp at 1.
    near(
        centre(
            [64, 64, 64, 255],
            &[(-5., [0.5, 0.5, 0.5, 1.], |s| s.color = Combine::Modulate2X)],
        )?,
        [64, 64, 64],
    );
    near(
        centre(
            [128, 128, 128, 255],
            &[(-5., [0.5, 0.5, 0.5, 1.], |s| s.color = Combine::Modulate4X)],
        )?,
        [255, 255, 255],
    );
    // Alpha blend and additive over the backdrop.
    near(
        centre(
            [255, 0, 0, 255],
            &[(-5., [1., 1., 1., 0.5], |s| {
                s.blend = Some((Factor::SrcAlpha, Factor::InvSrcAlpha));
            })],
        )?,
        [128, 0, 128],
    );
    near(
        centre(
            [255, 0, 0, 255],
            &[(-5., white, |s| {
                s.blend = Some((Factor::One, Factor::One));
            })],
        )?,
        [255, 0, 255],
    );
    // The frame-buffer "modulate" blend: source x destination + destination x source.
    near(
        centre(
            [128, 128, 128, 255],
            &[(-5., white, |s| {
                s.blend = Some((Factor::DstColor, Factor::SrcColor));
            })],
        )?,
        [0, 0, 255],
    );
    // Blend by the vertex alpha toward the vertex colour: red texel, vertex (0,1,0) alpha 0.5.
    near(
        centre(
            [255, 0, 0, 255],
            &[(-5., [0., 1., 0., 0.5], |s| {
                s.color = Combine::BlendByVertexAlpha;
            })],
        )?,
        [128, 128, 0],
    );
    // A constant colour replaces the vertex colour.
    near(
        centre(
            [255, 255, 255, 255],
            &[(-5., white, |s| s.constant = Some([0., 1., 0., 1.]))],
        )?,
        [0, 255, 0],
    );
    // Alpha test drops low alpha and keeps the rest.
    near(
        centre(
            [255, 0, 0, 255],
            &[(-5., [1., 1., 1., 0.3], |s| s.alpha_test = Some(0.5))],
        )?,
        BACKDROP,
    );
    near(
        centre(
            [255, 0, 0, 255],
            &[(-5., [1., 1., 1., 0.8], |s| s.alpha_test = Some(0.5))],
        )?,
        [255, 0, 0],
    );
    // Linear fog by view depth: depth 5 between 0 and 10 is halfway to the fog colour.
    near(
        centre(
            [255, 0, 0, 255],
            &[(-5., white, |s| {
                s.fog = Some(Fog {
                    start: 0.0,
                    end: 10.0,
                    color: [0., 1., 0.],
                });
            })],
        )?,
        [128, 128, 0],
    );
    // Depth: a nearer opaque quad hides a farther one; without the test the later quad wins.
    near(
        centre(
            [255, 0, 0, 255],
            &[(-4., white, |_| {}), (-6., [0., 1., 0., 1.], |_| {})],
        )?,
        [255, 0, 0],
    );
    near(
        centre(
            [255, 0, 0, 255],
            &[
                (-4., white, |_| {}),
                (-6., [0., 1., 0., 1.], |s| s.depth_test = false),
            ],
        )?,
        [0, 0, 0],
    );
    Ok(())
}

#[test]
fn line_batches_draw_the_indices_as_segments() -> Result<()> {
    // The quad's indices (0,1) (2,0) (2,3) read as a line list: the 2-0 segment is the
    // diagonal through the middle of the screen, and the quad is not filled.
    let white = [1., 1., 1., 1.];
    let lines: Edit = |s| s.topology = Topology::Lines;
    near(
        pixel([255, 0, 0, 255], &[(-5., white, lines)], 16, 15)?,
        [255, 0, 0],
    );
    near(
        pixel([255, 0, 0, 255], &[(-5., white, lines)], 16, 19)?,
        BACKDROP,
    );
    // As triangles the same indices fill that pixel.
    near(
        pixel([255, 0, 0, 255], &[(-5., white, |_| {})], 16, 19)?,
        [255, 0, 0],
    );
    Ok(())
}
