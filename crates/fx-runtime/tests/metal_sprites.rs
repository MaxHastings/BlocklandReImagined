//! Sprites in metal: the environment probe's faces draw the effects from
//! the probe's eye, as a mirror's view does, so a ball of bare metal
//! reflects smoke and sparks behind the viewer that only it can show.
use anyhow::Result;
use bri_content::effects::Library;
use bri_fx_runtime::{gpu::EffectsRenderer, pack::TextureImage, *};
use bri_render::{
    environment_probe::EnvironmentProbe,
    scene::{
        Camera as SceneCamera, Material, MaterialKind, MeshBatch, SceneData, SceneImage,
        SceneRenderer, SceneVertex, WorldPass, create_depth_samples,
    },
};
use glam::{Vec3, Vec4};
use std::collections::BTreeMap;

const SIZE: u32 = 128;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn pack() -> std::sync::Arc<EffectsPack> {
    EffectsPack::from_parts(
        Library {
            schema_version: 1,
            lights: Vec::new(),
            particles: Vec::new(),
            emitters: Vec::new(),
            textures: BTreeMap::from([("pixel".into(), "pixel.png".into())]),
        },
        Manifest {
            schema_version: 1,
            library_sha256: String::new(),
            textures: BTreeMap::new(),
            emitter_alpha: BTreeMap::new(),
            bindings: Vec::new(),
            composites: Vec::new(),
            unresolved: Vec::new(),
        },
        vec![TextureImage {
            id: "pixel".into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }],
    )
    .unwrap()
}

/// A mirror-smooth metal ball of radius 1 at the origin, in a dark room.
fn ball() -> SceneData {
    let mut data = SceneData::default();
    data.images.push(SceneImage {
        label: "flat".into(),
        width: 1,
        height: 1,
        rgba: vec![128, 255, 128, 128],
        srgb: false,
    });
    let (rings, segments) = (32u32, 64u32);
    for ring in 0..=rings {
        let theta = ring as f32 / rings as f32 * std::f32::consts::PI;
        for segment in 0..=segments {
            let phi = segment as f32 / segments as f32 * std::f32::consts::TAU;
            let n = Vec3::new(theta.sin() * phi.cos(), theta.cos(), -theta.sin() * phi.sin());
            data.vertices.push(SceneVertex {
                position: n.to_array(),
                normal: n.to_array(),
                uv: [segment as f32 / segments as f32, ring as f32 / rings as f32],
                lightmap_uv: [0.0; 2],
                color: [1.0; 4],
                fx: [0.0; 4],
            });
        }
    }
    let row = segments + 1;
    for ring in 0..rings {
        for segment in 0..segments {
            let a = ring * row + segment;
            let b = a + row;
            data.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let mut material = Material::vertex_lit("metal", 0);
    material.kind = MaterialKind::Metal;
    material.images[1] = 1;
    material.parameters = Some([[0.03, 1.0, 1.0, 0.0], [0.97; 4], [0.0; 4], [0.0; 4]]);
    data.materials.push(material);
    data.batches.push(MeshBatch {
        indices: 0..data.indices.len() as u32,
        material: 0,
        center: [0.0; 3],
    });
    data.sun_color = [0.0; 3];
    data.ambient = [0.0; 3];
    data.clear_color = [0.0, 0.0, 0.0, 1.0];
    data
}

/// Red pixels in the frame.
fn red(pixels: &[u8]) -> usize {
    pixels
        .chunks_exact(4)
        .filter(|p| p[0] > 120 && p[1] < 60 && p[2] < 60)
        .count()
}

/// The viewer at z = 3.2 looks at the ball; a big red sprite hangs behind
/// the viewer, where only the ball can show it. `sprites_in_probe` draws
/// the effects into the probe's faces.
fn frame(sprites_in_probe: bool) -> Result<Vec<u8>> {
    let (device, queue) = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance.request_adapter(&Default::default()).await?;
        anyhow::Ok(adapter.request_device(&Default::default()).await?)
    })?;
    let data = ball();
    let mut renderer = SceneRenderer::with_samples(&device, FORMAT, 1);
    let scene = renderer.upload(&device, &queue, &data)?;
    let mut camera = SceneCamera::perspective([0.0, 0.0, 3.2], [0.0; 3], 1.0, 0.7, 0.05, 100.0);
    camera.apply_environment(&data);
    renderer.update_camera(&queue, &camera);
    let mut probe = EnvironmentProbe::new(&device, &renderer, FORMAT, 1);
    probe.prepare(&device, &queue, &mut renderer, &camera, Some(Vec3::ZERO), 32.0);
    let mut sprites = EffectsRenderer::new(
        &device,
        &queue,
        &pack(),
        FORMAT,
        bri_render::scene::DEPTH_FORMAT,
        1,
        16,
    )?;
    let sprite = || FrameEffects {
        particles: vec![ParticleInstance {
            position: Vec3::new(0.0, 0.0, 8.0),
            size: 12.0,
            color: Vec4::new(1.0, 0.0, 0.0, 1.0),
            spin: 0.0,
            axis: Vec3::ZERO,
            texture: 0,
            blend: BlendMode::Alpha,
            depth_test: true,
        }],
        lights: Vec::new(),
    };
    let player = Camera {
        view_projection: glam::Mat4::from_cols_array(&camera.view_projection),
        position: Vec3::new(0.0, 0.0, 3.2),
        right: Vec3::X,
        up: Vec3::Y,
    };
    sprites.prepare(&queue, &player, &sprite())?;
    let faces = probe.face_views();
    assert_eq!(faces.len(), 6, "a new probe draws every face");
    if sprites_in_probe {
        // Views past the mirrors' (none here) are made on demand.
        for face in &faces {
            let view = Camera {
                view_projection: face.view_projection,
                position: face.eye,
                right: face.right,
                up: face.up,
            };
            sprites.prepare_view(&device, &queue, face.view, &view, &sprite())?;
        }
    }
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("metal sprites"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color = target.create_view(&Default::default());
    let depth = create_depth_samples(&device, SIZE, SIZE, 1).create_view(&Default::default());
    let clear = wgpu::Color::BLACK;
    let mut encoder = device.create_command_encoder(&Default::default());
    let late = |pass: &mut wgpu::RenderPass<'_>, view: usize| sprites.render_view(pass, view);
    probe.render(&renderer, &mut encoder, &[&scene], &[], clear, &|_, _| {}, &late);
    renderer.render_world(
        &mut encoder,
        WorldPass {
            view: 0,
            color: &color,
            resolve: None,
            depth: &depth,
            viewport: None,
            clear: Some(clear),
            after_opaque: None,
            after_all: None,
        },
        &[&scene],
        &[],
    );
    let row = (SIZE * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("metal sprites readback"),
        size: u64::from(row * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::from_secs(30)),
    })?;
    rx.recv_timeout(std::time::Duration::from_secs(30))??;
    let mapped = readback.slice(..).get_mapped_range()?;
    Ok(mapped
        .chunks_exact(row as usize)
        .flat_map(|r| r[..SIZE as usize * 4].iter().copied())
        .collect())
}

#[test]
fn metal_reflects_a_sprite_behind_the_viewer() -> Result<()> {
    let shown = red(&frame(true)?);
    assert!(shown > 200, "{shown} red pixels in the ball");
    // Without the probe's views of the effects the ball shows only black.
    assert_eq!(red(&frame(false)?), 0);
    Ok(())
}
