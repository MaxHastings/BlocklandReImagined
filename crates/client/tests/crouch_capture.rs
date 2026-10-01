//! Offscreen capture of v20's crouch-thread "humping" quirk: a side-view
//! contact sheet plus the sampled hip heights, on the made-up avatar
//! (`bri_client::testing::avatar`) and, ignored, on the original rig. No
//! window.
#[macro_use]
mod support;

use anyhow::{Context, Result};
use bri_client::avatar::AvatarMesh;
use bri_render::scene::{Camera, SceneRenderer, create_depth};
use bri_sim::player::PlayerState;
use bri_ui::gpu::Headless;
use support::{avatar_fixture::AvatarFixture, gpu};

synthetic_and_content!(AvatarFixture: recrouching_while_rising_snaps_the_rig_to_standing);

const TILE: (u32, u32) = (240, 360);

fn player(crouched: bool) -> PlayerState {
    PlayerState {
        owner: 1,
        feet: [0.0; 3],
        velocity: [0.0; 3],
        yaw: 0.0,
        pitch: 0.0,
        head_yaw: 0.0,
        grounded: true,
        crouched,
        jetting: false,
        jump: Default::default(),
        archetype: Default::default(),
        scale: 1.0,
        energy: 100.0,
        tick: Default::default(),
        tether: None,
    }
}

fn render(gpu: &Headless, renderer: &mut SceneRenderer, mesh: &AvatarMesh) -> Result<Vec<u8>> {
    let (width, height) = TILE;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("crouch capture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let depth = create_depth(&gpu.device, width, height).create_view(&Default::default());
    // Front three-quarter view from the right, framing the whole body.
    let camera = Camera::perspective(
        [4.6, 2.0, -3.4],
        [0.0, 1.0, -0.5],
        width as f32 / height as f32,
        45f32.to_radians(),
        0.05,
        100.0,
    );
    renderer.update_camera(&gpu.queue, &camera);
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("crouch readback"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let scene = mesh.gpu.as_ref().context("Uploaded avatar")?;
    renderer.render(
        &mut encoder,
        &view,
        &depth,
        &[scene],
        Some(wgpu::Color {
            r: 0.55,
            g: 0.7,
            b: 0.85,
            a: 1.0,
        }),
    );
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..width as usize * 4]);
    }
    Ok(pixels)
}

fn recrouching_while_rising_snaps_the_rig_to_standing(f: &AvatarFixture) -> Result<()> {
    let assets = &f.assets;
    let mut mesh = assets.mesh(assets.package.defaults.clone())?;
    let gpu = gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);

    // Stand, crouch for 0.3 s, release for 0.1 s, then tap crouch again.
    let frames = 60 * 7 / 10;
    let crouched_at = |frame: u32| (6..24).contains(&frame) || frame >= 30;
    let captured = [5, 9, 23, 26, 29, 30, 33, 41];
    let mut hips = Vec::new();
    let mut tiles = Vec::new();
    for frame in 0..frames {
        let state = player(crouched_at(frame));
        mesh.pose(assets, &state, f64::from(frame) / 60.0)?;
        let hip = mesh.world_node(assets, "Hip").context("Hip")?.w_axis.y;
        hips.push((frame, state.crouched, hip));
        if captured.contains(&frame) {
            mesh.upload(&renderer, &gpu.device, &gpu.queue)?;
            tiles.push(render(&gpu, &mut renderer, &mesh)?);
        }
    }
    // The pose sinks, rises part way, then snaps to standing on the re-press.
    let at = |frame: u32| hips[frame as usize].2;
    assert!(at(9) < at(5) && at(23) < at(9));
    assert!(at(29) > at(23) && at(29) < at(5) - 0.05);
    assert!(at(30) > at(29) + 0.1, "re-crouch did not snap: {hips:?}");
    assert!(at(41) < at(30));

    let out = f.out("torque-quirks")?;
    let (width, height) = TILE;
    let sheet_width = width * tiles.len() as u32;
    let mut sheet = vec![0u8; (sheet_width * height * 4) as usize];
    for (i, tile) in tiles.iter().enumerate() {
        for y in 0..height as usize {
            let src = &tile[y * width as usize * 4..(y + 1) * width as usize * 4];
            let start = (y * sheet_width as usize + i * width as usize) * 4;
            sheet[start..start + src.len()].copy_from_slice(src);
        }
    }
    image::save_buffer(
        out.join("crouch-tap.png"),
        &sheet,
        sheet_width,
        height,
        image::ColorType::Rgba8,
    )?;
    let log: String = hips
        .iter()
        .map(|(frame, crouched, hip)| format!("{frame}\t{crouched}\t{hip:.4}\n"))
        .collect();
    std::fs::write(
        out.join("crouch-tap-hip.tsv"),
        format!("frame\tcrouch_held\thip_y\n{log}"),
    )?;
    Ok(())
}
