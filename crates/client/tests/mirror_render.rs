//! Bounded normal-App render probe for the default Mirror Add-On, in the
//! Bedroom (an interior) and on Slopes (sky, terrain, snow): a wall of
//! 1x4x5 Mirrors in front of the camera, and behind the camera (where only
//! the mirrors can show them) a red pillar, a horse and a brick emitter,
//! with the player standing between. Each view, straight on and at an
//! angle, is captured with Mirrors on High and Off into
//! artifacts/mirror-render/<map>-<view>-<high|off>.png.
//! Runs on the made-up content root (the Bedroom and Slopes there being
//! lit rooms); the ignored variant runs on the generated v20 content
//! (`-- --ignored`, BRI_CONTENT or content/). Loopback QUIC and an
//! offscreen GPU; never opens a window.
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

synthetic_and_content!(ContentRoot: the_mirror_shows_the_room_behind_the_camera);

const SIZE: (u32, u32) = (640, 480);
const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";
const SLOPES: &str = "v20/add-ons/map_slopes/slopes.mis";
const MIRROR: &str = "brick_mirror:brick/brickmirror1x4x5data";

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
            "{what} did not happen within {} s of game time: {:?}",
            GAME_BUDGET / 120,
            app.ui.core.conn,
        );
        ensure!(
            now.duration_since(last_progress) < STALL,
            "Stalled waiting for {what}: {:?}",
            app.ui.core.conn
        );
        thread::sleep(Duration::from_millis(10));
    }
}
/// The world mesh current, then a fixed number of frames (emitters get
/// going, the horse settles).
fn settle(app: &mut App) -> Result<()> {
    until(app, "world render caught up", |a| {
        a.world_render_ready() && a.pending_requests() == 0
    })?;
    let mut previous = Instant::now();
    for _ in 0..90 {
        thread::sleep(Duration::from_millis(10));
        let now = Instant::now();
        step(app, now.duration_since(previous))?;
        previous = now;
    }
    Ok(())
}
fn capture(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mirror offscreen target"),
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
        label: Some("mirror readback"),
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
    let drawn = Instant::now();
    support::gpu::wait(&gpu.device, "the mirror frame")?;
    eprintln!("the mirror frame drew in {:.1?}", drawn.elapsed());
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
fn request_ui(app: &mut App, action: UiAction) -> Result<()> {
    app.ui.core.request(action);
    pump(app)
}
/// What the game said, for a load that did not happen.
fn diagnose(app: &App) -> String {
    let lines = |texts: Vec<String>| texts.into_iter().rev().take(6).rev().collect::<Vec<_>>();
    format!(
        "pending requests {}, screens {:?}, chat {:?}, server chat {:?}",
        app.pending_requests(),
        std::iter::once(app.ui.content.id())
            .chain(app.ui.dialogs.iter().map(|d| d.id()))
            .collect::<Vec<_>>(),
        lines(
            app.ui
                .core
                .chat
                .lines
                .iter()
                .map(|l| l.text.clone())
                .collect()
        ),
        lines(
            app.network_view()
                .map(|v| v.chat.iter().map(|c| c.text.clone()).collect())
                .unwrap_or_default()
        ),
    )
}
fn set_mirrors(app: &mut App, level: &str) -> Result<()> {
    app.ui.apply(UiUpdate::SetPrefs(vec![(
        bri_ui::screens::options::REFLECTIONS.into(),
        level.into(),
    )]));
    let settings = app.ui.settings();
    app.ui
        .core
        .request(UiAction::SaveSettings(Box::new(settings)));
    pump(app)
}
fn red(pixel: &[u8]) -> bool {
    let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(i32::from);
    r > 80 && r > 2 * g && r > 2 * b
}

/// Host `map`, build the mirror wall and what stands behind the camera,
/// and capture each view with Mirrors on High and Off. Returns, per view,
/// the pixels the mirrors changed and the red pixels with and without them.
fn probe(
    f: &ContentRoot,
    map: &str,
    map_name: &str,
    artifact: &Path,
) -> Result<Vec<(String, usize, usize, usize)>> {
    let scratch_dir = f.state()?;
    let scratch = scratch_dir.path().to_path_buf();
    let result = (|| {
        let content = f.with_defaults(&scratch)?;
        let state = scratch.join("state");
        std::fs::create_dir_all(&state)?;
        let mut app = App::load(&content, &state, SIZE)?;
        app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
        app.ui.core.request(UiAction::HostGame {
            map: map.into(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 1,
            server_name: "Mirror render".into(),
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
        // The player faces the mirror wall eight units along -z; behind the
        // camera stand a red pillar (right), a burning brick (middle) and a
        // horse (left).
        let (player, _) = app.local_motion().context("local player")?;
        let feet = Vec3::from(player.feet);
        let floor = (feet.y / 0.2).ceil() * 0.2;
        let view = app.network_view().context("view")?;
        let map_id = view.world.map_id.clone();
        let loaded_content = bri_client::content::ClientContent::load(&content)?;
        let paths = &loaded_content.paths;
        let definitions = bri_sim::definitions::Definitions::load_with(
            &paths.brick_catalog,
            &paths.geometry,
            &paths.brick_extras,
        )?;
        // Load Bricks finds a save by the map's name as the save list shows it.
        let save_map =
            bri_client::saves::Store::new(&state, &loaded_content, None).map_name(&map_id);
        let snap = |x: f32, z: f32| Vec3::new((feet.x + x).round(), floor, (feet.z + z).round());
        // The mirrors' frames in the palette's whitest colour, so red in the
        // pictures is only the pillar.
        let white = view
            .world
            .palette
            .iter()
            .enumerate()
            .filter(|(_, c)| c[3] >= 0.99)
            .max_by(|(_, a), (_, b)| {
                a[..3]
                    .iter()
                    .copied()
                    .fold(1.0, f32::min)
                    .total_cmp(&b[..3].iter().copied().fold(1.0, f32::min))
            })
            .map_or(0, |(i, _)| i as u8);
        let mut world =
            bri_world::World::new("Mirror".into(), map_id.clone(), view.world.palette.clone());
        let mut next = 1;
        let mut add = |brick: bri_world::Brick| {
            world.bricks.insert(next, brick);
            next += 1;
        };
        let brick = |definition: &str, at: Vec3| {
            bri_world::Brick::new(
                bri_world::ContentRef::Resolved(definition.into()),
                at.to_array(),
                view.owner,
            )
        };
        // Each mirror on the build grid (the host drops bricks off it),
        // turned so its long side runs along x and its glass faces the camera.
        let [long, short, turns] = {
            let [x, z] = definitions.entries[MIRROR].mesh.footprint_studs;
            if x >= z { [x, z, 0] } else { [z, x, 1] }
        };
        let half_stud = |studs: u32| if studs % 2 == 1 { 0.25 } else { 0.0 };
        let wall = snap(0.0, -8.0) + Vec3::new(half_stud(long), 0.0, half_stud(short));
        for x in [-4.0, -2.0, 0.0, 2.0, 4.0] {
            // Five bricks (3 units) tall, standing on the floor.
            let mut mirror = brick(MIRROR, wall + Vec3::new(x, 1.5, 0.0));
            mirror.quarter_turns = turns as u8;
            mirror.color = white;
            add(mirror);
        }
        let pillar = snap(4.0, 5.0);
        for layer in 0..8 {
            let mut red = brick(&f.brick, pillar + Vec3::Y * (0.3 + 0.6 * layer as f32));
            red.color = 0;
            add(red);
        }
        let mut fire = brick(&f.brick, snap(0.0, 5.0) + Vec3::Y * 0.3);
        fire.emitter = Some(Box::new(bri_world::Emitter {
            asset: Some(bri_world::ContentRef::Resolved(f.emitter.clone())),
            direction: 0,
        }));
        add(fire);
        let mut spawn = brick(&f.vehicle_spawn, snap(-4.0, 5.0) + Vec3::Y * 0.1);
        spawn.vehicle = Some(Box::new(bri_world::VehicleSpawn {
            vehicle: bri_world::ContentRef::Resolved(bri_vehicles::testing::HORSE.into()),
            recolor: false,
            team: None,
        }));
        add(spawn);
        world.next_brick_id = next;
        let count = next - 1;
        let build = bri_world::build::SavedBuild::new(world);
        use sha2::Digest;
        let folder = state
            .join("saves")
            .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
        std::fs::create_dir_all(&folder)?;
        std::fs::write(
            folder.join("mirror.world.json"),
            serde_json::to_vec(&build)?,
        )?;
        eprintln!("{map_name}: loading {count} bricks as a save of {save_map:?}");
        app.ui.core.request(UiAction::LoadBricks {
            map: save_map.clone(),
            name: "mirror.world.json".into(),
            ownership: true,
        });
        pump(&mut app)?;
        let loaded = |a: &App| a.network_view().map_or(0, |v| v.world.bricks.len() as u64);
        let color_check = |a: &App| {
            a.ui.screen(bri_ui::screens::ScreenId::LoadBricksColor)
                .is_some()
        };
        let _ = until(&mut app, "the load to start", |a| {
            color_check(a) || loaded(a) > 0
        });
        if color_check(&app) {
            // The save's colours differ from the map's: keep them.
            eprintln!("{map_name}: the colour check opened; appending the save's colours");
            request_ui(&mut app, UiAction::LoadBricksColors(ColorLoad::Append))?;
            app.ui.core.pop(bri_ui::screens::ScreenId::LoadBricksColor);
        }
        let built = until(&mut app, "the build", |a| {
            loaded(a) >= count && a.pending_requests() == 0
        });
        ensure!(
            built.is_ok(),
            "the host built {} of the {count} bricks ({built:?}); {}",
            loaded(&app),
            diagnose(&app)
        );
        // The horse is wanted in the pictures but they are still worth
        // taking without it.
        if until(&mut app, "the horse", |a| {
            a.network_view().is_some_and(|v| !v.vehicles.is_empty())
        })
        .is_err()
        {
            eprintln!("{map_name}: WARNING the horse did not spawn");
        }
        let gpu = support::gpu::turn().context("offscreen mirror renderer")?;
        let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
        app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
        // Straight on at about eye height, a little in front of the player;
        // then from the right at an angle, so the mirrors look round to the
        // left of the room.
        let stand = Vec3::new(feet.x, floor, feet.z);
        let views = [
            ("straight", stand + Vec3::new(0.0, 1.6, -1.0), 0.0),
            ("angled", stand + Vec3::new(6.0, 1.6, -3.0), -0.876),
        ];
        let mut out = Vec::new();
        for (name, eye, yaw) in views {
            camera_at(&mut app, eye, yaw, 0.0)?;
            let mut shots = Vec::new();
            let mut stats = None;
            for (level_name, level) in [("high", "3"), ("off", "0")] {
                set_mirrors(&mut app, level)?;
                settle(&mut app)?;
                let pixels = capture(&mut app, &gpu, &mut renderer)?;
                save(
                    &artifact.join(format!("{map_name}-{name}-{level_name}.png").to_lowercase()),
                    &pixels,
                )?;
                shots.push(pixels);
                stats.get_or_insert_with(|| format!("{:?}", app.render_stats()));
            }
            let reds = |pixels: &[u8]| pixels.chunks_exact(4).filter(|p| red(p)).count();
            let changed = shots[0]
                .chunks_exact(4)
                .zip(shots[1].chunks_exact(4))
                .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 24))
                .count();
            eprintln!(
                "{map_name} {name}: mirrors changed {changed} px; red {} with, {} without; high: {}",
                reds(&shots[0]),
                reds(&shots[1]),
                stats.unwrap_or_default()
            );
            out.push((name.to_string(), changed, reds(&shots[0]), reds(&shots[1])));
        }
        app.gpu_stopped();
        Ok(out)
    })();
    drop(scratch_dir);
    result
}

fn the_mirror_shows_the_room_behind_the_camera(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("mirror-render")?;
    for (map, name) in [(BEDROOM, "Bedroom"), (SLOPES, "Slopes")] {
        for (view, changed, live, silver) in probe(f, map, name, &artifact)? {
            ensure!(
                changed > 20_000,
                "{name} {view}: the mirrors show nothing but silver ({changed} px differ)"
            );
            // Straight on, the red pillar behind the camera shows only in
            // the mirrors.
            if view == "straight" {
                ensure!(
                    live > silver + 200,
                    "{name}: the red pillar behind the camera is not in the mirror (red {live} vs {silver})"
                );
            }
        }
    }
    Ok(())
}
