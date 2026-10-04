//! Sprites in a mirror: each live mirror draws the effects from its own
//! reflected eye, so a sprite only the mirror can show (behind the viewer)
//! appears in it on the side it stands, facing the mirror's eye.
use anyhow::Result;
use bri_content::effects::Library;
use bri_fx_runtime::{gpu::EffectsRenderer, pack::TextureImage, *};
use bri_render::{
    reflection::{Mirror, ReflectionSettings, Reflections},
    scene::{Camera as SceneCamera, SceneData, SceneRenderer, WorldPass, create_depth_samples},
};
use glam::{Mat4, Vec3, Vec4};
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

/// Red pixels in the left and right halves of the frame.
fn red_halves(pixels: &[u8]) -> [usize; 2] {
    let mut out = [0; 2];
    for (i, p) in pixels.chunks_exact(4).enumerate() {
        if p[0] > 150 && p[1] < 60 && p[2] < 60 {
            out[usize::from(i as u32 % SIZE >= SIZE / 2)] += 1;
        }
    }
    out
}

/// The viewer at z = 4 faces a 3x3 mirror in the plane z = 0; a red sprite
/// stands behind the viewer, right of centre, where only the mirror shows it.
fn frame(mirrored: bool) -> Result<Vec<u8>> {
    let (device, queue) = pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance.request_adapter(&Default::default()).await?;
        anyhow::Ok(adapter.request_device(&Default::default()).await?)
    })?;
    let mut renderer = SceneRenderer::with_samples(&device, FORMAT, 1);
    let scene = renderer.upload(&device, &queue, &SceneData::default())?;
    let camera = SceneCamera::perspective([0.0, 0.0, 4.0], [0.0; 3], 1.0, 1.0, 0.05, 100.0);
    renderer.update_camera(&queue, &camera);
    let mirror = Mirror {
        corners: [
            Vec3::new(-1.5, -1.5, 0.0),
            Vec3::new(1.5, -1.5, 0.0),
            Vec3::new(1.5, 1.5, 0.0),
            Vec3::new(-1.5, 1.5, 0.0),
        ],
        tint: [1.0; 3],
        strength: 1.0,
        looks: bri_render::reflection::Looks::Reflect,
        fallback: bri_render::reflection::SILVER,
        recess: 0.0,
    };
    let settings = if mirrored {
        ReflectionSettings::MEDIUM
    } else {
        ReflectionSettings::OFF
    };
    let mut reflections = Reflections::new(&device, FORMAT, 1, settings);
    reflections.prepare(
        &device,
        &queue,
        &mut renderer,
        &camera,
        (SIZE, SIZE),
        &[mirror],
    )?;
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
            position: Vec3::new(1.2, 0.2, 6.0),
            size: 1.0,
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
        view_projection: Mat4::from_cols_array(&camera.view_projection),
        position: Vec3::new(0.0, 0.0, 4.0),
        right: Vec3::X,
        up: Vec3::Y,
    };
    sprites.prepare(&queue, &player, &sprite())?;
    for (i, plane) in reflections.plan().planes.iter().enumerate() {
        let view = Camera {
            view_projection: plane.view_projection,
            position: plane.eye,
            right: plane.reflect_direction(Vec3::X),
            up: plane.reflect_direction(Vec3::Y),
        };
        sprites.prepare_view(&device, &queue, 1 + i, &view, &sprite())?;
    }
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mirror sprites"),
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
    let clear = wgpu::Color {
        r: 0.0,
        g: 0.0,
        b: 0.3,
        a: 1.0,
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    let late = |pass: &mut wgpu::RenderPass<'_>, view: usize| sprites.render_view(pass, view);
    reflections.render(&renderer, &mut encoder, &[&scene], &[], clear, &late);
    let surfaces = |pass: &mut wgpu::RenderPass<'_>| reflections.draw_surfaces(pass, 0);
    let own = |pass: &mut wgpu::RenderPass<'_>| sprites.render(pass);
    renderer.render_world(
        &mut encoder,
        WorldPass {
            view: 0,
            color: &color,
            resolve: None,
            depth: &depth,
            viewport: None,
            clear: Some(clear),
            after_opaque: Some(&surfaces),
            after_all: Some(&own),
        },
        &[&scene],
        &[],
    );
    let row = (SIZE * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mirror sprites readback"),
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
fn a_mirror_shows_a_sprite_behind_the_viewer_on_its_own_side() -> Result<()> {
    let [left, right] = red_halves(&frame(true)?);
    assert!(left == 0 && right > 20, "red {left} {right}");
    // Without the mirror's view the sprite is out of sight.
    assert_eq!(red_halves(&frame(false)?), [0, 0]);
    Ok(())
}
