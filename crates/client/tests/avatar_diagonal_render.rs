//! Offscreen capture of the Blockhead walking at 45 degrees.
//! Run with: cargo test -p bri-client --test avatar_diagonal_render --release -- --ignored --nocapture
//! Requires the converted avatar package (BRI_CONTENT overrides `content/`) and
//! an offscreen GPU; never opens a window.
use anyhow::{Context, Result, ensure};
use bri_client::avatar::{AvatarAnimationInput, AvatarAssets};
use bri_render::scene::{Camera, SceneRenderer, create_depth};
use bri_sim::player::PlayerState;
use bri_ui::gpu::Headless;
use glam::{Mat4, Quat, Vec3};
use std::{path::PathBuf, time::Duration};

const CELL: (u32, u32) = (200, 260);
const COLUMNS: u32 = 10;
const FRAMES: usize = 90;
const SHOWN: usize = 30;
const DT: f64 = 1.0 / 60.0;

fn content() -> PathBuf {
    std::env::var_os("BRI_CONTENT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

/// Holding forward and right: the body yaw follows the mouse every frame
/// while velocity runs toward the 45 degree move direction on 32 ms ticks.
/// `wobble` is a per-frame mouse wobble that changes turn direction
/// constantly; `turning` sweeps the mouse slowly left then right. `forward`
/// holds only forward with a still mouse: the plain run reference.
/// Returns the presented state and the latest simulated tick.
fn player(
    frame: usize,
    tick_state: &mut Option<PlayerState>,
    case: &str,
) -> (PlayerState, PlayerState) {
    let t = frame as f32 * DT as f32;
    let yaw = match case {
        "wobble" => 0.6 + 0.004 * (frame as f32 * 1.7).sin(),
        "turning" => 0.6 + 0.35 * (t * 2.0).sin(),
        _ => 0.6,
    };
    let tick = |frame: usize| (frame as f64 * DT / 0.032) as u64;
    let mut velocity = tick_state
        .as_ref()
        .map_or(Vec3::ZERO, |state| Vec3::from(state.velocity));
    let new_tick = frame == 0 || tick(frame) != tick(frame - 1);
    if new_tick {
        let facing = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
        let right = Vec3::new(yaw.cos(), 0.0, yaw.sin());
        let direction = if case == "forward" {
            facing
        } else {
            facing + right
        };
        let desired = direction.normalize() * 7.0;
        velocity = if frame == 0 {
            desired
        } else {
            velocity + (desired - velocity).clamp_length_max(48.0 * 0.032)
        };
    }
    let state = PlayerState {
        owner: 1,
        feet: (velocity * t).to_array(),
        velocity: velocity.to_array(),
        yaw,
        pitch: 0.0,
        head_yaw: 0.0,
        grounded: true,
        crouched: false,
        jetting: false,
        jump: Default::default(),
    };
    if new_tick {
        *tick_state = Some(state.clone());
    }
    (state, tick_state.clone().unwrap())
}

fn readback(gpu: &Headless, texture: &wgpu::Texture) -> Result<Vec<u8>> {
    let row = (CELL.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("avatar diagonal readback"),
        size: u64::from(row) * u64::from(CELL.1),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(CELL.1),
            },
        },
        wgpu::Extent3d {
            width: CELL.0,
            height: CELL.1,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    rx.recv_timeout(Duration::from_secs(5))??;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((CELL.0 * CELL.1 * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..CELL.0 as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}

#[test]
#[ignore = "requires original native avatar package and an offscreen GPU"]
fn diagonal_walk_keeps_a_continuous_leg_cycle() -> Result<()> {
    let prefix = std::env::var("BRI_CAPTURE_LABEL").unwrap_or_else(|_| "after".into());
    let reference = capture(&format!("{prefix}-forward"), "forward")?;
    for case in ["turning", "wobble"] {
        let steps = capture(&format!("{prefix}-{case}"), case)?;
        let worst = steps
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        println!("{prefix}-{case}: differs from a straight run by at most {worst:.2} deg/frame");
        if prefix == "after" {
            // A diagonal plays the same uninterrupted run cycle as a straight run.
            ensure!(
                worst < 0.5,
                "{case} diagonal left the run cycle ({worst:.2} deg)"
            );
        }
    }
    Ok(())
}

fn capture(label: &str, case: &str) -> Result<Vec<f32>> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/avatar-diagonal");
    std::fs::create_dir_all(&out)?;
    let assets = AvatarAssets::load(&content().join("avatar-pack-001"))?;
    let mut mesh = assets.mesh(assets.package.defaults.clone())?;
    let gpu = Headless::new().context("offscreen avatar renderer")?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = SceneRenderer::new(&gpu.device, format);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("avatar diagonal target"),
        size: wgpu::Extent3d {
            width: CELL.0,
            height: CELL.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = create_depth(&gpu.device, CELL.0, CELL.1);
    let rows = (SHOWN as u32).div_ceil(COLUMNS);
    let mut sheet = image::RgbaImage::new(CELL.0 * COLUMNS, CELL.1 * rows);
    let mut previous: Option<Vec3> = None;
    let mut steps = Vec::new();
    let mut tick_state = None;
    for frame in 0..FRAMES {
        let (p, tick) = player(frame, &mut tick_state, case);
        mesh.pose_with_animation(
            &assets,
            &p,
            frame as f64 * DT,
            &AvatarAnimationInput {
                tick_state: Some(tick),
                ..Default::default()
            },
        )?;
        let model = Mat4::from_rotation_translation(Quat::from_rotation_y(-p.yaw), p.feet.into());
        let leg = model.inverse()
            * mesh
                .world_node(&assets, "RightLeg")
                .context("Original RightLeg node")?;
        let direction = leg.y_axis.truncate().normalize();
        if let Some(previous) = previous {
            steps.push(previous.angle_between(direction).to_degrees());
        }
        previous = Some(direction);
        if frame >= FRAMES - SHOWN {
            let index = (frame - (FRAMES - SHOWN)) as u32;
            mesh.upload(&renderer, &gpu.device, &gpu.queue)?;
            let feet = Vec3::from(p.feet);
            let target = feet + Vec3::new(0.0, 1.3, 0.0);
            // Viewed from the walker's right side, slightly ahead.
            let side = Vec3::new(p.yaw.cos(), 0.0, p.yaw.sin());
            let ahead = Vec3::new(p.yaw.sin(), 0.0, -p.yaw.cos());
            let eye = target + (side * 0.9 + ahead * 0.35).normalize() * 6.0 + Vec3::Y * 0.6;
            let mut camera = Camera::perspective(
                eye.to_array(),
                target.to_array(),
                CELL.0 as f32 / CELL.1 as f32,
                35_f32.to_radians(),
                0.05,
                50.0,
            );
            camera.sun_direction = [0.721277, 0.57735, -0.57735, 0.0];
            camera.sun_color = [1.0, 1.0, 1.0, 0.0];
            camera.ambient = [0.5, 0.5, 0.5, 0.0];
            renderer.update_camera(&gpu.queue, &camera);
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            renderer.render(
                &mut encoder,
                &texture.create_view(&Default::default()),
                &depth.create_view(&Default::default()),
                &[mesh.gpu.as_ref().context("Uploaded avatar")?],
                Some(wgpu::Color {
                    r: 0.55,
                    g: 0.7,
                    b: 0.9,
                    a: 1.0,
                }),
            );
            gpu.queue.submit([encoder.finish()]);
            let pixels = readback(&gpu, &texture)?;
            let cell = image::RgbaImage::from_raw(CELL.0, CELL.1, pixels).context("Cell pixels")?;
            image::imageops::overlay(
                &mut sheet,
                &cell,
                i64::from(index % COLUMNS * CELL.0),
                i64::from(index / COLUMNS * CELL.1),
            );
        }
    }
    sheet.save(out.join(format!("{label}-sheet.png")))?;
    let largest = steps.iter().copied().fold(0.0_f32, f32::max);
    let mean = steps.iter().sum::<f32>() / steps.len() as f32;
    let report = serde_json::json!({
        "adapter": gpu.adapter_info.name,
        "frames": FRAMES,
        "fps": 60,
        "leg_step_degrees": { "largest": largest, "mean": mean, "all": steps },
    });
    std::fs::write(
        out.join(format!("{label}-report.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{label}: largest leg step {largest:.2} deg, mean {mean:.2} deg");
    Ok(steps)
}
