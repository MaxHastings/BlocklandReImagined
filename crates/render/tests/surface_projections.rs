//! Projected surfaces: a floor quad is drawn into the colour and depth targets, then the same
//! triangles are redrawn with a projected picture at a depth bias; the centre pixel is read back.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::dynamic_sprites::{Batch, DynamicSprites, SpriteVertex, State};
use bri_render::projected_textures::{ProjectionBlend, ViewFog};
use bri_render::surface_projections::*;
use glam::{Mat3, Mat4, Vec3};

const FLOOR: [u8; 3] = [100, 100, 100];

struct Case {
    texel: [u8; 4],
    blend: ProjectionBlend,
    /// The z of the redrawn triangles (the floor is at -5).
    z: f32,
    depth_bias: i32,
    uv: [f32; 2],
    fade: f32,
    vertex_fade: bool,
    mask: Option<[u8; 8]>,
    base: Option<([u8; 4], BaseBlend)>,
    normal_fade: Option<Vec3>,
    gradient: Option<([u8; 8], Mat4)>,
    fog: Option<ViewFog>,
    wide: Option<[u8; 8]>,
    wrap: bool,
    cube: Option<([[u8; 4]; 6], Mat3)>,
}

fn case(texel: [u8; 4], blend: ProjectionBlend) -> Case {
    Case {
        texel,
        blend,
        z: -5.0,
        depth_bias: 8,
        uv: [0.5, 0.5],
        fade: 1.0,
        vertex_fade: false,
        mask: None,
        base: None,
        normal_fade: None,
        gradient: None,
        fog: None,
        wide: None,
        wrap: false,
        cube: None,
    }
}

fn centre(case: &Case) -> Result<[u8; 3]> {
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
    let color_view = target.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let view_projection = bri_render::scene::perspective(1.2, 1.0, 0.1, 100.0);

    // The floor.
    let mut sprites = DynamicSprites::new(&device, format, wgpu::TextureFormat::Depth32Float, 1);
    let white = sprites.add_image(&device, &queue, &[255; 4], 1, 1, false);
    let c = [
        f32::from(FLOOR[0]) / 255.0,
        f32::from(FLOOR[1]) / 255.0,
        f32::from(FLOOR[2]) / 255.0,
        1.0,
    ];
    let corners = [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)];
    let floor: Vec<SpriteVertex> = corners
        .map(|(x, y)| SpriteVertex {
            position: [x, y, -5.0],
            color: c,
            uv: [0.0; 2],
            uv2: [0.0; 2],
        })
        .to_vec();
    let indices = [0u32, 1, 2, 0, 2, 3];
    sprites.prepare(
        &device,
        &queue,
        view_projection,
        &floor,
        &[Batch {
            state: State::new(white),
            indices: &indices,
        }],
    );

    // The projection of the same triangles.
    let mut surfaces = ProjectedSurfaces::new(
        &device,
        &queue,
        format,
        wgpu::TextureFormat::Depth32Float,
        1,
    );
    let image = match &case.wide {
        Some(t) => surfaces.add_image(&device, &queue, t, 2, 1, true),
        None => surfaces.add_image(&device, &queue, &case.texel, 1, 1, true),
    };
    let mask = case
        .mask
        .map(|m| surfaces.add_image(&device, &queue, &m, 2, 1, false));
    let base = case
        .base
        .map(|(t, b)| (surfaces.add_image(&device, &queue, &t, 1, 1, true), b));
    let gradient = case
        .gradient
        .as_ref()
        .map(|(t, m)| (surfaces.add_image(&device, &queue, t, 1, 2, false), *m));
    let reflection = case.cube.map(|(faces, direction)| Reflection {
        cube: surfaces.add_cube(
            &device,
            &queue,
            &faces.map(|texel| texel.repeat(16 * 16)),
            16,
            true,
        ),
        direction,
    });
    let vertices: Vec<SurfaceVertex> = corners
        .map(|(x, y)| SurfaceVertex {
            position: [x, y, case.z],
            normal: [0.0, 0.0, 1.0],
            uv: case.uv,
            fade: case.fade,
        })
        .to_vec();
    // x in [-1, 1] -> s in [0, 1] (or constant for the wide picture); y -> t in [1, 0].
    let (s_scale, s_offset) = if case.wide.is_some() {
        (0.0, 1.25)
    } else {
        (0.5, 0.5)
    };
    let world_to_texture = Mat4::from_cols_array(&[
        s_scale, 0.0, 0.0, 0.0, //
        0.0, -0.5, 0.0, 0.0, //
        0.0, 0.0, 0.0, 0.0, //
        s_offset, 0.5, 0.0, 1.0,
    ]);
    let projection = SurfaceProjection {
        image,
        world_to_texture,
        divide: false,
        blend: case.blend,
        wrap: [case.wrap, false],
        gradient,
        mask,
        base,
        reflection,
        normal_fade: case.normal_fade,
        vertex_fade: case.vertex_fade,
        two_sided: true,
        depth_bias: case.depth_bias,
        fog: case.fog,
    };
    surfaces.prepare(
        &device,
        &queue,
        view_projection,
        Vec3::ZERO,
        &vertices,
        &[SurfaceBatch {
            indices: &indices,
            projection,
        }],
    );

    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("floor"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("projection"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        surfaces.render(&mut pass);
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
    let at = 16 * 256 + 16 * 4;
    Ok([data[at], data[at + 1], data[at + 2]])
}

fn near(got: [u8; 3], want: [u8; 3]) {
    for k in 0..3 {
        assert!(
            (i32::from(got[k]) - i32::from(want[k])).abs() <= 3,
            "got {got:?} want {want:?}"
        );
    }
}

#[test]
fn the_redrawn_triangles_blend_the_picture_at_a_depth_bias() -> Result<()> {
    // Alpha, modulate and add over the grey floor, drawn on the very same triangles.
    near(
        centre(&case([255, 0, 0, 128], ProjectionBlend::Alpha))?,
        [178, 50, 50],
    );
    near(
        centre(&case([200, 100, 0, 255], ProjectionBlend::Modulate2X))?,
        [157, 78, 0],
    );
    near(
        centre(&case([100, 0, 0, 255], ProjectionBlend::Add))?,
        [200, 100, 100],
    );
    // Triangles behind the floor are hidden by the depth test.
    let mut behind = case([255, 0, 0, 255], ProjectionBlend::Alpha);
    behind.z = -5.5;
    near(centre(&behind)?, FLOOR);
    // In front they show.
    let mut ahead = case([255, 0, 0, 255], ProjectionBlend::Alpha);
    ahead.z = -4.5;
    near(centre(&ahead)?, [255, 0, 0]);
    Ok(())
}

#[test]
fn fades_masks_base_pictures_and_addressing_scale_the_projection() -> Result<()> {
    // The vertex fade.
    let mut faded = case([100, 0, 0, 255], ProjectionBlend::Add);
    faded.fade = 0.5;
    faded.vertex_fade = true;
    near(centre(&faded)?, [150, 100, 100]);
    // The surface mask: its alpha at the vertex uv (a 2 x 1 picture, transparent then opaque).
    let mut masked = case([100, 0, 0, 255], ProjectionBlend::Add);
    masked.mask = Some([0, 0, 0, 0, 0, 0, 0, 255]);
    masked.uv = [0.75, 0.5];
    near(centre(&masked)?, [200, 100, 100]);
    masked.uv = [0.25, 0.5];
    near(centre(&masked)?, FLOOR);
    // The surface's own picture first: twice white doubles the texel (clamped).
    let mut based = case([100, 50, 0, 255], ProjectionBlend::Alpha);
    based.base = Some(([255; 4], BaseBlend::Modulate2X));
    near(centre(&based)?, [200, 100, 0]);
    // Facing: the floor's normal is +Z, so a direction down -Z lights it, up -Z+ does not.
    let mut facing = case([100, 0, 0, 255], ProjectionBlend::Add);
    facing.normal_fade = Some(Vec3::NEG_Z);
    near(centre(&facing)?, [200, 100, 100]);
    facing.normal_fade = Some(Vec3::Z);
    near(centre(&facing)?, FLOOR);
    // Gradient: v = (-z - 4) / 2, so the floor sits halfway.
    let to_v = Mat4::from_cols_array(&[
        0.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 0.0, //
        0.0, -0.5, 0.0, 0.0, //
        0.0, -2.0, 0.0, 1.0,
    ]);
    let mut graded = case([200, 0, 0, 255], ProjectionBlend::Add);
    graded.gradient = Some(([0, 0, 0, 0, 0, 0, 0, 255], to_v));
    near(centre(&graded)?, [200, 100, 100]);
    // Addressing: red | blue picture with s = 1.25: repeat reads red, clamp the blue edge.
    let mut wide = case([0; 4], ProjectionBlend::Alpha);
    wide.wide = Some([255, 0, 0, 255, 0, 0, 255, 255]);
    wide.wrap = true;
    near(centre(&wide)?, [255, 0, 0]);
    wide.wrap = false;
    near(centre(&wide)?, [0, 0, 255]);
    // Fog at depth 5 over 0..10 halves an additive projection.
    let mut fogged = case([100, 0, 0, 255], ProjectionBlend::Add);
    fogged.fog = Some(ViewFog {
        start: 0.0,
        end: 10.0,
        color: [0.0, 1.0, 0.0],
    });
    near(centre(&fogged)?, [150, 100, 100]);
    Ok(())
}

#[test]
fn a_reflection_looks_the_view_ray_up_in_a_cube() -> Result<()> {
    fn centre_reflection(
        faces: &[[u8; 4]; 6],
        direction: Mat3,
        blend: ProjectionBlend,
    ) -> Result<[u8; 3]> {
        let mut c = case([0; 4], blend);
        c.cube = Some((*faces, direction));
        centre(&c)
    }
    // Faces +X red, -X green, +Y blue, -Y yellow, +Z magenta, -Z cyan.
    let colours: [[u8; 4]; 6] = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
        [255, 0, 255, 255],
        [0, 255, 255, 255],
    ];
    // The floor faces +Z and the ray runs along -Z, so it reflects straight back to +Z.
    let seen =
        |direction: Mat3, blend: ProjectionBlend| centre_reflection(&colours, direction, blend);
    near(seen(Mat3::IDENTITY, ProjectionBlend::Alpha)?, [255, 0, 255]);
    // Turning world +Z into the cube's -Y reads the yellow face.
    let z_to_minus_y = Mat3::from_cols(Vec3::X, Vec3::NEG_Z, Vec3::NEG_Y);
    near(seen(z_to_minus_y, ProjectionBlend::Alpha)?, [255, 255, 0]);
    // Modulated 2x over the grey floor: 2 x face x floor.
    near(
        seen(Mat3::IDENTITY, ProjectionBlend::Modulate2X)?,
        [200, 0, 200],
    );
    Ok(())
}
