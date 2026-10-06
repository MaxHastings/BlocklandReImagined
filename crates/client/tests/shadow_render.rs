//! Bounded normal-App render probe for sun shadows in the Bedroom: a player
//! on a brick roof shades the roof only (the room's ceiling above once let
//! the shadow fall through to the floor below as well), and a spawned horse
//! casts like every other vehicle (see side-on-roof.png).
//! It draws Classic lighting, where players and vehicles cast sun shadows
//! everywhere as in v20. Unified shades them from the sun with the map's own
//! walls, so under the Bedroom's ceiling they cast lamp shadows instead
//! (bri-render's map_lamps_cast_live_shadows_in_unified_modes_by_shadow_quality);
//! and Unified starts as Classic until the map's bake
//! arrives, so a capture here once caught either mode.
//! Runs on the made-up content root (its Bedroom a lit room); the ignored
//! variant runs on the generated v20 content (`--release -- --ignored`,
//! BRI_CONTENT or content/). Loopback QUIC and an offscreen GPU; never
//! opens a window.
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
};
use glam::Vec3;
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[macro_use]
mod support;
use support::content_root::ContentRoot;

synthetic_and_content!(ContentRoot: a_player_on_a_roof_shades_the_roof_not_the_floor_below);

const SIZE: (u32, u32) = (640, 480);
/// Pillar layers of 2x2 bricks (0.6 units each) under the player.
const LAYERS: usize = 14;
/// Roof bricks along each side (one unit each) on top of the pillar.
const ROOF: usize = 12;

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
/// Wait for `ready` within a game-time budget, failing early when the host
/// stops advancing (see horse_riding_render.rs).
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
            "{what} did not happen within {} s of game time: {:?}; player {:?}",
            GAME_BUDGET / 120,
            app.ui.core.conn,
            app.local_motion().map(|(p, _)| p.feet),
        );
        ensure!(
            now.duration_since(last_progress) < STALL,
            "Stalled waiting for {what}: {:?}",
            app.ui.core.conn
        );
        thread::sleep(Duration::from_millis(10));
    }
}
/// Let the view catch up before a capture: the world mesh must show the
/// latest world (the roof's chunks mesh off-thread, and a loaded PC once
/// captured before they had), then a fixed number of frames pass for the
/// camera and shadows. Frames, not wall time, so a slow PC waits longer.
fn settle(app: &mut App) -> Result<()> {
    until(app, "world render caught up", |a| {
        a.world_render_ready() && a.pending_requests() == 0
    })?;
    let mut previous = Instant::now();
    for _ in 0..SETTLE_FRAMES {
        thread::sleep(Duration::from_millis(10));
        let now = Instant::now();
        step(app, now.duration_since(previous))?;
        previous = now;
    }
    Ok(())
}
/// Frames `settle` runs once the world render is current.
const SETTLE_FRAMES: usize = 60;
fn capture(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow offscreen target"),
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
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shadow readback"),
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
    support::gpu::wait(&gpu.device, "the shadow frame")?;
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
fn request(app: &mut App, action: GameAction) -> Result<()> {
    app.ui.core.request(UiAction::Game(action));
    pump(app)
}
/// Put the free camera at `eye`, looking along `yaw` and `pitch`.
fn camera_at(app: &mut App, eye: Vec3, yaw: f32, pitch: f32) -> Result<()> {
    if app.controls.free_camera().is_none() {
        request(app, GameAction::DropCameraAtPlayer)?;
        until(app, "free camera", |a| a.controls.free_camera().is_some())?;
    }
    app.controls.redrop_camera(eye);
    let (current_yaw, current_pitch) = app.controls.camera_angles();
    let delta = (yaw - current_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    request(
        app,
        GameAction::Look {
            yaw: delta,
            pitch: current_pitch - pitch,
        },
    )
}
/// Drop the player so its feet land at `feet`, then wait for it to settle.
fn drop_player(app: &mut App, feet: Vec3) -> Result<()> {
    camera_at(app, feet + Vec3::Y * 2.5, 0.0, 0.0)?;
    request(app, GameAction::DropPlayerAtCamera)?;
    until(app, "player drop", |a| {
        a.local_motion().is_some_and(|(p, _)| {
            p.grounded
                && Vec3::from(p.velocity).length() < 0.001
                && Vec3::from(p.feet).distance(feet) < 2.0
        })
    })
}
fn red(pixel: &[u8]) -> bool {
    let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(i32::from);
    r > 60 && r > 2 * g && r > 2 * b
}

fn a_player_on_a_roof_shades_the_roof_not_the_floor_below(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("shadow-render")?;
    let state_dir = f.state()?;
    let state = state_dir.path();
    let mut app = App::load(&f.root, state, SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    let mut settings = app.ui.settings();
    settings
        .prefs
        .insert(bri_client::graphics::LIGHTING.into(), "0".into());
    app.ui
        .core
        .request(UiAction::SaveSettings(Box::new(settings)));
    pump(&mut app)?;
    app.ui.core.request(UiAction::HostGame {
        map: f.open_map.0.clone(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Shadow render".into(),
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
            .is_some_and(|(p, _)| p.grounded && Vec3::from(p.velocity).length() < 0.001)
    })?;
    // A saved build: a red pillar of 2x2 bricks (2x2 units, 8.4 tall) eight
    // units ahead under a red 12x12 unit roof, wide enough to catch the
    // player's whole shadow, and a horse spawn beside the roof.
    let (player, _) = app.local_motion().context("local player")?;
    let feet = Vec3::from(player.feet);
    let forward = Vec3::new(player.yaw.sin(), 0.0, -player.yaw.cos());
    let right = Vec3::new(player.yaw.cos(), 0.0, player.yaw.sin());
    let floor = (feet.y / 0.2).ceil() * 0.2;
    let snap = |p: Vec3| Vec3::new(p.x.round(), floor, p.z.round());
    let pillar = snap(feet + forward * 8.0);
    let horse = snap(pillar + right * 10.0);
    let view = app.network_view().context("view")?;
    let map_id = view.world.map_id.clone();
    let mut world =
        bri_world::World::new("Shadow".into(), map_id.clone(), view.world.palette.clone());
    let mut next = 1;
    let mut red_brick = |x: f32, layer: usize, z: f32| {
        let mut brick = bri_world::Brick::new(
            bri_world::ContentRef::Resolved(f.brick.clone()),
            [x, floor + 0.3 + 0.6 * layer as f32, z],
            view.owner,
        );
        brick.color = 0;
        world.bricks.insert(next, brick);
        next += 1;
    };
    for layer in 0..LAYERS {
        for (dx, dz) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
            red_brick(pillar.x + dx, layer, pillar.z + dz);
        }
    }
    for i in 0..ROOF {
        for j in 0..ROOF {
            let offset = |k: usize| k as f32 - ROOF as f32 / 2.0 + 0.5;
            red_brick(pillar.x + offset(i), LAYERS, pillar.z + offset(j));
        }
    }
    let mut spawn = bri_world::Brick::new(
        bri_world::ContentRef::Resolved(f.vehicle_spawn.clone()),
        [horse.x, floor + 0.1, horse.z],
        view.owner,
    );
    spawn.vehicle = Some(Box::new(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved(bri_vehicles::testing::HORSE.into()),
        recolor: false,
        team: None,
    }));
    world.bricks.insert(next, spawn);
    world.next_brick_id = next + 1;
    let build = bri_world::build::SavedBuild::new(world);
    use sha2::Digest;
    let folder = state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("shadow.world.json"),
        serde_json::to_vec(&build)?,
    )?;
    app.ui.core.request(UiAction::LoadBricks {
        map: f.open_map.1.clone(),
        name: "shadow.world.json".into(),
        ownership: true,
    });
    pump(&mut app)?;
    until(&mut app, "roof and horse", |a| {
        a.network_view().is_some_and(|v| {
            !v.vehicles.is_empty() && v.world.bricks.len() > 4 * LAYERS + ROOF * ROOF
        })
    })?;
    let gpu = support::gpu::turn().context("offscreen shadow renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let top = pillar + Vec3::Y * (0.6 * (LAYERS + 1) as f32);
    drop_player(&mut app, top)?;
    settle(&mut app)?;
    ensure!(
        app.local_motion()
            .is_some_and(|(p, _)| (p.feet[1] - top.y).abs() < 0.3),
        "player did not land on the roof: {:?}",
        app.local_motion().map(|(p, _)| p.feet)
    );
    // Straight down over the roof, high enough to see the floor round it;
    // then a raking view from the side that also shows the horse.
    let overhead = (top + Vec3::Y * 22.0, player.yaw, -1.55);
    let side = (
        pillar - forward * 16.0 + right * 6.0 + Vec3::Y * 12.0,
        player.yaw - 0.35,
        -0.5,
    );
    let mut shots = Vec::new();
    for (name, (eye, yaw, pitch)) in [("overhead", overhead), ("side", side)] {
        camera_at(&mut app, eye, yaw, pitch)?;
        settle(&mut app)?;
        let pixels = capture(&mut app, &gpu, &mut renderer)?;
        save(&artifact.join(format!("{name}-on-roof.png")), &pixels)?;
        shots.push(pixels);
    }
    // The same views with the player far away: whatever changed is the
    // player and its shadow.
    drop_player(&mut app, feet - forward * 20.0)?;
    let mut empty = Vec::new();
    for (name, (eye, yaw, pitch)) in [("overhead", overhead), ("side", side)] {
        camera_at(&mut app, eye, yaw, pitch)?;
        settle(&mut app)?;
        let pixels = capture(&mut app, &gpu, &mut renderer)?;
        save(&artifact.join(format!("{name}-empty.png")), &pixels)?;
        empty.push(pixels);
    }
    app.gpu_stopped();
    // Overhead, the roof is the only red, and it catches the player's whole
    // shadow: any other pixel the player darkens is floor its shadow reached
    // through the roof.
    let (with, without) = (&shots[0], &empty[0]);
    let luma = |p: &[u8]| i32::from(p[0]) + i32::from(p[1]) + i32::from(p[2]);
    // The roof seen straight down is a square: inside its outline is roof,
    // whatever stands on it (the player's own darker parts are not floor).
    let width = SIZE.0 as usize;
    let outline = without
        .chunks_exact(4)
        .enumerate()
        .filter(|(_, p)| red(p))
        .fold(None, |r: Option<[usize; 4]>, (i, _)| {
            let (x, y) = (i % width, i / width);
            Some(r.map_or([x, y, x, y], |[x0, y0, x1, y1]| {
                [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]
            }))
        });
    let inside = |i: usize| {
        let (x, y) = (i % width, i / width);
        outline.is_some_and(|[x0, y0, x1, y1]| (x0..=x1).contains(&x) && (y0..=y1).contains(&y))
    };
    let (mut roof, mut shaded, mut leaked) = (0, 0, 0);
    let mut mask = Vec::with_capacity(with.len());
    for (i, (a, b)) in with
        .chunks_exact(4)
        .zip(without.chunks_exact(4))
        .enumerate()
    {
        let on_roof = red(b) || inside(i);
        let darker = luma(b) - luma(a) > 45;
        roof += usize::from(on_roof);
        shaded += usize::from(darker && on_roof);
        leaked += usize::from(darker && !on_roof);
        mask.extend_from_slice(&match (darker, on_roof) {
            (true, true) => [0, 255, 0, 255],
            (true, false) => [255, 0, 255, 255],
            _ => [a[0] / 3, a[1] / 3, a[2] / 3, 255],
        });
    }
    save(&artifact.join("overhead-diff.png"), &mask)?;
    eprintln!("roof {roof} px; player darkened {shaded} on it and {leaked} elsewhere");
    ensure!(roof > 2000, "roof not in the overhead view ({roof} px)");
    ensure!(shaded > 50, "the player shades the roof ({shaded} px)");
    ensure!(
        leaked < 40,
        "the player's shadow also darkened {leaked} floor pixels through the roof"
    );
    Ok(())
}
