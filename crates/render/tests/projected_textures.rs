//! Projected textures: a floor quad is drawn into the colour and depth targets, then a
//! projection is landed on it from the depth buffer; the centre pixel is read back.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::dynamic_sprites::{Batch, DynamicSprites, SpriteVertex, State};
use bri_render::projected_textures::*;
use glam::{Mat4, Vec3};

const FLOOR: [u8; 3] = [100, 100, 100];

struct Case {
    texel: [u8; 4],
    blend: ProjectionBlend,
    /// The volume along the viewing axis: z must lie between `-far` and `-near`.
    near: f32,
    far: f32,
    gradient: Option<([u8; 8], Mat4)>,
    normal_fade: Option<Vec3>,
    /// A 2 x 1 picture (left, right) instead of the single texel, mapped to
    /// `s = centre` so the floor's middle lands at `s = centre`.
    wide: Option<[u8; 8]>,
    centre: f32,
    wrap: [bool; 2],
    /// The colour every pixel of the surface-info buffer holds (category in r, alpha in a).
    info: Option<[u8; 4]>,
    accept: u32,
    surface_alpha: bool,
    fog: Option<ViewFog>,
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let color_view = target.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let projection = bri_render::scene::perspective(1.2, 1.0, 0.1, 100.0);

    // Pass 1: the floor, a grey quad at z = -5.
    let mut sprites = DynamicSprites::new(&device, format, wgpu::TextureFormat::Depth32Float, 1);
    let white = sprites.add_image(&device, &queue, &[255; 4], 1, 1, false);
    let c = [
        f32::from(FLOOR[0]) / 255.0,
        f32::from(FLOOR[1]) / 255.0,
        f32::from(FLOOR[2]) / 255.0,
        1.0,
    ];
    let vertices: Vec<SpriteVertex> = [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)]
        .map(|(x, y)| SpriteVertex {
            position: [x, y, -5.0],
            color: c,
            uv: [0.0; 2],
            uv2: [0.0; 2],
        })
        .to_vec();
    let indices = [0u32, 1, 2, 0, 2, 3];
    let batches = [Batch {
        state: State::new(white),
        indices: &indices,
    }];
    sprites.prepare(&device, &queue, projection, &vertices, &batches);

    // Pass 2: the projection.
    let mut projectors = DepthProjectors::new(&device, &queue, format);
    let image = match &case.wide {
        Some(texels) => projectors.add_image(&device, &queue, texels, 2, 1, true),
        None => projectors.add_image(&device, &queue, &case.texel, 1, 1, true),
    };
    let info_view = case.info.map(|colour| {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("surface info"),
            size: wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let data: Vec<u8> = (0..32 * 32).flat_map(|_| colour).collect();
        queue.write_texture(
            texture.as_image_copy(),
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(128),
                rows_per_image: Some(32),
            },
            wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
        );
        texture.create_view(&Default::default())
    });
    let gradient = case.gradient.as_ref().map(|(texels, m)| {
        let mut rgba = Vec::new();
        for t in texels.chunks(4) {
            rgba.extend_from_slice(t);
        }
        (
            projectors.add_image(&device, &queue, &rgba, 1, 2, false),
            *m,
        )
    });
    // x in [-1, 1] -> s in [0, 1]; y in [-1, 1] -> t in [1, 0]; orthographic.
    let (s_scale, s_offset) = if case.wide.is_some() {
        (0.0, case.centre)
    } else {
        (0.5, 0.5)
    };
    let world_to_texture = Mat4::from_cols_array(&[
        s_scale, 0.0, 0.0, 0.0, //
        0.0, -0.5, 0.0, 0.0, //
        0.0, 0.0, 0.0, 0.0, //
        s_offset, 0.5, 0.0, 1.0,
    ]);
    let projection_set = Projection {
        image,
        world_to_texture,
        divide: false,
        planes: vec![[0.0, 0.0, -1.0, case.near], [0.0, 0.0, 1.0, -case.far]],
        blend: case.blend,
        gradient,
        normal_fade: case.normal_fade,
        bounds: Some((
            Vec3::new(-1.0, -1.0, -case.far),
            Vec3::new(1.0, 1.0, -case.near),
        )),
        wrap: case.wrap,
        accept: case.accept,
        surface_alpha: case.surface_alpha,
        draw_last: false,
    };
    projectors.prepare(
        &device,
        &queue,
        &View {
            view_projection: projection,
            camera: Vec3::ZERO,
            size: (32, 32),
            depth: &depth_view,
            surface_info: info_view.as_ref(),
            fog: case.fog,
        },
        &[projection_set],
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
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        projectors.render(&mut pass);
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

fn case(texel: [u8; 4], blend: ProjectionBlend) -> Case {
    Case {
        texel,
        blend,
        near: 4.0,
        far: 6.0,
        gradient: None,
        normal_fade: None,
        wide: None,
        centre: 0.5,
        wrap: [false, false],
        info: None,
        accept: 0,
        surface_alpha: false,
        fog: None,
    }
}

#[test]
fn projections_land_on_the_depth_buffer_inside_their_volume() -> Result<()> {
    // Alpha: a half-transparent red over the grey floor.
    near(
        centre(&case([255, 0, 0, 128], ProjectionBlend::Alpha))?,
        [178, 50, 50],
    );
    // Outside the volume (the floor is at depth 5, the volume ends at 4.5): unchanged.
    let mut outside = case([255, 0, 0, 255], ProjectionBlend::Alpha);
    outside.far = 4.5;
    outside.near = 4.0;
    near(centre(&outside)?, FLOOR);
    // Modulate: 2 x picture x floor, so a 128 grey picture leaves the floor unchanged
    // and (200, 100, 0) gives (157, 78, 0).
    near(
        centre(&case([128, 128, 128, 255], ProjectionBlend::Modulate2X))?,
        FLOOR,
    );
    near(
        centre(&case([200, 100, 0, 255], ProjectionBlend::Modulate2X))?,
        [157, 78, 0],
    );
    // Add.
    near(
        centre(&case([100, 0, 0, 255], ProjectionBlend::Add))?,
        [200, 100, 100],
    );
    Ok(())
}

#[test]
fn the_gradient_and_facing_fades_scale_the_projection() -> Result<()> {
    // The gradient's v is (-z - 4) / 2: the floor at z = -5 samples halfway between a
    // transparent and an opaque texel, so the added red is half strength.
    let to_v = Mat4::from_cols_array(&[
        0.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 0.0, //
        0.0, -0.5, 0.0, 0.0, //
        0.0, -2.0, 0.0, 1.0,
    ]);
    let mut graded = case([200, 0, 0, 255], ProjectionBlend::Add);
    graded.gradient = Some(([0, 0, 0, 0, 0, 0, 0, 255], to_v));
    near(centre(&graded)?, [200, 100, 100]);
    // Facing: the floor faces the camera at +Z. Projecting down -Z lights it fully;
    // projecting up +Z leaves it dark.
    let mut facing = case([100, 0, 0, 255], ProjectionBlend::Add);
    facing.normal_fade = Some(Vec3::NEG_Z);
    near(centre(&facing)?, [200, 100, 100]);
    let mut away = case([100, 0, 0, 255], ProjectionBlend::Add);
    away.normal_fade = Some(Vec3::Z);
    near(centre(&away)?, FLOOR);
    Ok(())
}

#[test]
fn addressing_info_buffer_and_fog_shape_the_projection() -> Result<()> {
    // A 2 x 1 picture, red then blue, with the floor's middle at s = 1.25: wrapping reads
    // the red texel (0.25), clamping the blue edge (1.0).
    let mut wrapped = case([0; 4], ProjectionBlend::Alpha);
    wrapped.wide = Some([255, 0, 0, 255, 0, 0, 255, 255]);
    wrapped.centre = 1.25;
    wrapped.wrap = [true, false];
    near(centre(&wrapped)?, [255, 0, 0]);
    wrapped.wrap = [false, false];
    near(centre(&wrapped)?, [0, 0, 255]);

    // The surface-info buffer: categories select, and its alpha scales (the base mask).
    let mut selected = case([255, 0, 0, 255], ProjectionBlend::Alpha);
    selected.info = Some([4, 0, 0, 255]);
    selected.accept = 4;
    near(centre(&selected)?, [255, 0, 0]);
    selected.accept = 1 | 2;
    near(centre(&selected)?, FLOOR);
    let mut masked = case([255, 0, 0, 255], ProjectionBlend::Alpha);
    masked.info = Some([255, 0, 0, 128]);
    masked.surface_alpha = true;
    near(centre(&masked)?, [178, 50, 50]);

    // Fog at depth 5 with range 0..10 is half strength: additive fades toward black,
    // alpha-blended toward the fog colour.
    let fog = |color| {
        Some(ViewFog {
            start: 0.0,
            end: 10.0,
            color,
        })
    };
    let mut added = case([100, 0, 0, 255], ProjectionBlend::Add);
    added.fog = fog([0.0, 1.0, 0.0]);
    near(centre(&added)?, [150, 100, 100]);
    let mut blended = case([255, 0, 0, 255], ProjectionBlend::Alpha);
    blended.fog = fog([0.0, 1.0, 0.0]);
    near(centre(&blended)?, [128, 128, 0]);
    Ok(())
}
