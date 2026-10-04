//! Bounded normal-App render probe for riding a spawned horse. Runs on the
//! made-up content root (its horse rig); the ignored variant runs on the
//! generated v20 content (`--release -- --ignored`, BRI_CONTENT or
//! content/). Loopback QUIC and an offscreen GPU; never opens a window.
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[macro_use]
mod support;
use support::{content_root::ContentRoot, wait};

synthetic_and_content!(ContentRoot: riding_a_horse_holds_the_rider_still_on_its_animated_back);

const SIZE: (u32, u32) = (640, 480);

fn pump(app: &mut App) -> Result<()> {
    ensure!(
        app.pump()?.is_empty(),
        "Unexpected native window command in offscreen test"
    );
    Ok(())
}
fn step(app: &mut App, dt: Duration) -> Result<()> {
    app.tick(dt)?;
    app.ui.update(dt.as_millis() as u64);
    pump(app)
}
/// Wait for `ready`, budgeted in the host's game time rather than wall time:
/// fail after `GAME_BUDGET` of server ticks, or when nothing advances (no
/// server tick, no connection change) for `STALL` of wall time. A loaded
/// machine (a cold build, the gate running other suites) runs the host more
/// slowly; a wall-clock deadline turned that into spurious failures.
const GAME_BUDGET: u64 = 30 * 120;
const STALL: Duration = Duration::from_secs(90);
fn until(app: &mut App, what: &str, ready: impl Fn(&App) -> bool) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    let tick = |a: &App| a.network_view().map(|v| v.tick);
    let first_tick = tick(app);
    let mut last_progress = start;
    let mut last_seen = (tick(app), format!("{:?}", app.ui.core.conn));
    loop {
        let now = Instant::now();
        step(app, now.duration_since(previous))?;
        previous = now;
        if ready(app) {
            eprintln!(
                "{what}: {:.1} s wall, {} game ticks",
                start.elapsed().as_secs_f32(),
                tick(app)
                    .zip(first_tick)
                    .map_or(0, |(t, f)| t.saturating_sub(f))
            );
            return Ok(());
        }
        let seen = (tick(app), format!("{:?}", app.ui.core.conn));
        if seen != last_seen {
            last_seen = seen;
            last_progress = now;
        }
        let game = tick(app)
            .zip(first_tick)
            .map_or(0, |(t, f)| t.saturating_sub(f));
        ensure!(
            game < GAME_BUDGET,
            "{what} did not happen within {} s of game time: {:?}; bricks (position, vehicle) {:?}; player {:?}; chat {:?}",
            GAME_BUDGET / 120,
            app.ui.core.conn,
            app.network_view().map(|v| v
                .world
                .bricks
                .values()
                .map(|b| (b.position, b.vehicle.is_some()))
                .collect::<Vec<_>>()),
            app.local_motion().map(|(p, _)| p.feet),
            app.network_view()
                .map(|v| v.chat.iter().map(|c| c.text.clone()).collect::<Vec<_>>())
        );
        ensure!(
            now.duration_since(last_progress) < STALL,
            "Stalled waiting for {what} (no host progress for {} s): {:?}",
            STALL.as_secs(),
            app.ui.core.conn
        );
        thread::sleep(Duration::from_millis(10));
    }
}
/// Let `seconds` of game time pass ([`wait::run_one_for`]).
fn run_for(app: &mut App, seconds: f32) -> Result<()> {
    wait::run_one_for(app, Duration::from_secs_f32(seconds), step)
}
fn capture(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    capture_hud(app, gpu, renderer, false)
}
fn capture_hud(
    app: &mut App,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    hud: bool,
) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("held-item offscreen target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    ensure!(
        app.render_scene(&mut RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target: &view,
            format,
            size: SIZE,
            ui_renderer: renderer
        })?,
        "App did not render a session camera"
    );
    if hud {
        app.ui.update(0);
        let ui = app.ui();
        renderer.render(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &view,
            format,
            SIZE,
            ui.scale(),
            &ui.core.pack,
            &ui.draw(),
            None,
        );
    }
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("held-item readback"),
        size: u64::from(row) * u64::from(SIZE.1),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE.1),
            },
        },
        extent,
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
    let mut pixels = Vec::with_capacity((SIZE.0 * SIZE.1 * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..SIZE.0 as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}
fn save(path: &Path, data: &[u8]) -> Result<()> {
    image::save_buffer(path, data, SIZE.0, SIZE.1, image::ColorType::Rgba8)?;
    Ok(())
}
fn look(app: &mut App, yaw: f32, pitch: f32) -> Result<()> {
    let (current_yaw, current_pitch) = app.controls.view_angles();
    let delta = (yaw - current_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    app.ui.core.request(UiAction::Game(GameAction::Look {
        yaw: delta,
        pitch: current_pitch - pitch,
    }));
    pump(app)
}
fn held(app: &mut App, control: HeldControl, down: bool) -> Result<()> {
    app.ui
        .core
        .request(UiAction::Game(GameAction::Held { control, down }));
    pump(app)
}

fn riding_a_horse_holds_the_rider_still_on_its_animated_back(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("horse-riding")?;
    let state_dir = f.state()?;
    let state = state_dir.path();
    let mut app = App::load(&f.root, state, SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: f.map.0.clone(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Horse riding render".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    pump(&mut app)?;
    until(&mut app, "host/player", |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner))
    })?;
    until(&mut app, "player landing", |a| {
        a.local_motion()
            .is_some_and(|(p, _)| p.grounded && glam::Vec3::from(p.velocity).length() < 0.001)
    })?;
    // A saved build: a vehicle spawn brick with a horse five units ahead.
    let (player, _) = app.local_motion().context("local player")?;
    let feet = glam::Vec3::from(player.feet);
    let forward = glam::Vec3::new(player.yaw.sin(), 0.0, -player.yaw.cos());
    let spot = feet + forward * 5.0;
    let view = app.network_view().context("view")?;
    let map_id = view.world.map_id.clone();
    let mut world =
        bri_world::World::new("Horse".into(), map_id.clone(), view.world.palette.clone());
    let mut brick = bri_world::Brick::new(
        bri_world::ContentRef::Resolved(f.vehicle_spawn.clone()),
        [
            (spot.x * 2.0).round() / 2.0,
            (feet.y / 0.2).ceil() * 0.2 + 0.1,
            (spot.z * 2.0).round() / 2.0,
        ],
        view.owner,
    );
    brick.vehicle = Some(Box::new(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved(bri_vehicles::testing::HORSE.into()),
        recolor: false,
        team: None,
    }));
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let build = bri_world::build::SavedBuild::new(world);
    use sha2::Digest;
    let folder = state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&folder)?;
    std::fs::write(folder.join("horse.world.json"), serde_json::to_vec(&build)?)?;
    app.ui.core.request(UiAction::LoadBricks {
        // The save list names maps by their display name.
        map: f.map.1.clone(),
        name: "horse.world.json".into(),
        ownership: true,
    });
    pump(&mut app)?;
    until(&mut app, "horse spawn", |a| {
        a.network_view().is_some_and(|v| !v.vehicles.is_empty())
    })?;
    run_for(&mut app, 1.0)?;
    // Run at the horse and jump onto its back.
    look(&mut app, player.yaw, 0.0)?;
    held(&mut app, HeldControl::Forward, true)?;
    let mut mounted = false;
    for _ in 0..20 {
        held(&mut app, HeldControl::Jump, true)?;
        run_for(&mut app, 0.2)?;
        held(&mut app, HeldControl::Jump, false)?;
        run_for(&mut app, 0.3)?;
        mounted = app
            .network_view()
            .and_then(|v| v.vitals.get(&v.owner))
            .is_some_and(|v| v.mounted.is_some());
        if mounted {
            break;
        }
    }
    held(&mut app, HeldControl::Forward, false)?;
    ensure!(mounted, "never boarded the horse");
    let gpu = support::gpu::turn().context("offscreen horse renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    app.ui
        .core
        .request(UiAction::Game(GameAction::ToggleFirstPerson {
            fast: false,
        }));
    pump(&mut app)?;
    run_for(&mut app, 1.0)?;
    save(
        &artifact.join("mounted-idle.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    held(&mut app, HeldControl::Forward, true)?;
    run_for(&mut app, 0.8)?;
    save(
        &artifact.join("mounted-running.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    let (yaw, pitch) = app.controls.view_angles();
    look(&mut app, yaw + 1.0, pitch)?;
    run_for(&mut app, 0.3)?;
    save(
        &artifact.join("mounted-turning.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    held(&mut app, HeldControl::Jump, true)?;
    run_for(&mut app, 0.25)?;
    save(
        &artifact.join("mounted-jumping.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    held(&mut app, HeldControl::Jump, false)?;
    held(&mut app, HeldControl::Forward, false)?;
    app.gpu_stopped();
    Ok(())
}
