//! Bounded normal-App render probe for surfaces that sit close together,
//! seen from far away (Max, 2026-09-30: a red-framed mirror wall streaked
//! red across its glass, and a distant jeep with its rider looked wonky).
//! On Slopes: two walls of the default Mirror Add-On's 1x4x5 Mirrors, one
//! framed red and one white, and a stock jeep the player boards. Each wall
//! is pictured from 60, 90 and 120 units with Mirrors Off (plain silver
//! glass) and Medium; the jeep from 12 and 100 units. Any glass pixel that
//! shows the red frame through it fails the test (the mirror sits 1 mm over
//! its brick: forward depth lost that from about 30 units).
//! Runs on the made-up content root (its Slopes a lit room, its jeep a
//! made-up car); the ignored variant runs on the generated v20 content
//! (`-- --ignored`, BRI_CONTENT or content/), its pictures and report in
//! artifacts/distant-render/. Loopback QUIC and an offscreen GPU; never
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
use support::{content_root::ContentRoot, wait};

synthetic_and_content!(ContentRoot: distant_mirrors_and_riders_draw_cleanly);

const SIZE: (u32, u32) = (1280, 960);
const SLOPES: &str = "v20/add-ons/map_slopes/slopes.mis";
const MIRROR: &str = "brick_mirror:brick/brickmirror1x4x5data";
/// The stock jeep, which the made-up vehicle pack offers too.
const JEEP: &str = bri_vehicles::testing::CAR;

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
            "{what} did not happen within {} s of game time: {:?}; screens {:?}; observer {:?}; vitals {:?}",
            GAME_BUDGET / 120,
            app.ui.core.conn,
            app.ui.stack(),
            app.controls.observer(),
            app.network_view()
                .and_then(|v| v.vitals.get(&v.owner).cloned()),
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
        label: Some("distant offscreen target"),
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
        label: Some("distant readback"),
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

/// Run and jump at the jeep until the player boards it, in game time, and
/// fail unless they then sit on it, alive, as the shots expect. Returns the
/// seat.
fn board(app: &mut App) -> Result<u8> {
    let jeep = app
        .network_view()
        .and_then(|v| v.vehicles.keys().next().copied())
        .context("no jeep")?;
    // The jeep and seat the player rides, while alive.
    let riding = |a: &App| {
        a.network_view()
            .and_then(|v| v.vitals.get(&v.owner))
            .filter(|v| v.alive)
            .and_then(|v| v.mounted)
            .filter(|(vehicle, _)| *vehicle == jeep)
            .map(|(_, seat)| seat)
    };
    let held = |app: &mut App, control: HeldControl, down: bool| {
        request(app, GameAction::Held { control, down })
    };
    for attempt in 0..160 {
        if riding(app).is_some() {
            break;
        }
        let view = app.network_view().context("view")?;
        let Some(target) = view
            .vehicle_poses
            .get(&jeep)
            .map(|p| Vec3::from(p.position))
        else {
            run_for(app, 0.25)?;
            continue;
        };
        let (player, _) = app.local_motion().context("player")?;
        let to = target - Vec3::from(player.feet);
        let yaw = to.x.atan2(-to.z);
        let (current, _) = app.controls.view_angles();
        let turn = (yaw - current + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        request(
            app,
            GameAction::Look {
                yaw: turn,
                pitch: 0.0,
            },
        )?;
        held(app, HeldControl::Forward, true)?;
        held(app, HeldControl::Jump, attempt % 2 == 0)?;
        run_for(app, 0.25)?;
    }
    held(app, HeldControl::Forward, false)?;
    held(app, HeldControl::Jump, false)?;
    let vitals = |a: &App| {
        a.network_view()
            .and_then(|v| v.vitals.get(&v.owner).cloned())
    };
    let seat = riding(app).with_context(|| format!("never boarded the jeep: {:?}", vitals(app)))?;
    run_for(app, 1.0)?;
    ensure!(
        riding(app) == Some(seat),
        "left seat {seat} of the jeep once boarded: {:?}",
        vitals(app)
    );
    Ok(seat)
}
/// Let `seconds` of game time pass ([`wait::run_one_for`]).
fn run_for(app: &mut App, seconds: f32) -> Result<()> {
    wait::run_one_for(app, Duration::from_secs_f32(seconds), step)
}
/// Grey enough and mid-bright enough to be a mirror's silver.
fn grey(pixel: &[u8]) -> bool {
    let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(i32::from);
    r.max(g).max(b) - r.min(g).min(b) < 24 && (90..=215).contains(&((r + g + b) / 3))
}
/// Pixels where a red mirror wall's frame shows through its glass: glass
/// in the white-framed wall's picture, with every neighbour glass too (no
/// frame edge), but red in the red-framed wall's picture of the same spot.
/// Only the middle `box_size` pixels count, where the wall is. Returns
/// (fought, glass).
fn fights(red_wall: &[u8], white_wall: &[u8], box_size: [usize; 2]) -> (usize, usize) {
    let (w, h) = (SIZE.0 as usize, SIZE.1 as usize);
    let at = |pixels: &[u8], x: usize, y: usize| {
        let i = (y * w + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let (mut fought, mut glass) = (0, 0);
    let [bw, bh] = box_size.map(|v| v / 2);
    for y in (h / 2).saturating_sub(bh).max(1)..(h / 2 + bh).min(h - 1) {
        for x in (w / 2).saturating_sub(bw).max(1)..(w / 2 + bw).min(w - 1) {
            let inside = [(0, 0), (1, 0), (0, 1), (2, 1), (1, 2)]
                .iter()
                .all(|&(dx, dy)| grey(&at(white_wall, x + dx - 1, y + dy - 1)));
            if !inside {
                continue;
            }
            glass += 1;
            if red(&at(red_wall, x, y)) {
                fought += 1;
            }
        }
    }
    (fought, glass)
}

/// Two walls of Mirrors on Slopes, one framed red and one white, and a
/// jeep the player rides; pictures from 60 to 120 units.
fn probe(f: &ContentRoot, artifact: &Path) -> Result<Vec<String>> {
    let scratch_dir = f.state()?;
    let scratch = scratch_dir.path().to_path_buf();
    let result = (|| {
        let content = f.with_defaults(&scratch)?;
        let state = scratch.join("state");
        std::fs::create_dir_all(&state)?;
        let mut app = App::load(&content, &state, SIZE)?;
        app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
        app.ui.core.request(UiAction::HostGame {
            map: SLOPES.into(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 1,
            server_name: "Distant render".into(),
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
        let save_map =
            bri_client::saves::Store::new(&state, &loaded_content, None).map_name(&map_id);
        let snap = |x: f32, z: f32| Vec3::new((feet.x + x).round(), floor, (feet.z + z).round());
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
            bri_world::World::new("Distant".into(), map_id.clone(), view.world.palette.clone());
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
        let [long, short, turns] = {
            let [x, z] = definitions.entries[MIRROR].mesh.footprint_studs;
            if x >= z { [x, z, 0] } else { [z, x, 1] }
        };
        let half_stud = |studs: u32| if studs % 2 == 1 { 0.25 } else { 0.0 };
        // Two walls 16 wide and 12 tall, 24 apart, glass facing +z: the
        // red one left, the white one right.
        let walls = [(snap(-12.0, -12.0), 0u8), (snap(12.0, -12.0), white)].map(|(at, colour)| {
            (
                at + Vec3::new(half_stud(long), 0.0, half_stud(short)),
                colour,
            )
        });
        for (at, colour) in walls {
            for row in 0..4 {
                for column in 0..8 {
                    let x = -7.0 + 2.0 * column as f32;
                    let mut mirror = brick(MIRROR, at + Vec3::new(x, 1.5 + 3.0 * row as f32, 0.0));
                    mirror.quarter_turns = turns as u8;
                    mirror.color = colour;
                    add(mirror);
                }
            }
        }
        let mut spawn = brick(&f.vehicle_spawn, snap(0.0, 10.0) + Vec3::Y * 0.1);
        spawn.vehicle = Some(Box::new(bri_world::VehicleSpawn {
            vehicle: bri_world::ContentRef::Resolved(JEEP.into()),
            recolor: false,
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
            folder.join("distant.world.json"),
            serde_json::to_vec(&build)?,
        )?;
        app.ui.core.request(UiAction::LoadBricks {
            map: save_map.clone(),
            name: "distant.world.json".into(),
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
        until(&mut app, "the jeep", |a| {
            a.network_view().is_some_and(|v| !v.vehicles.is_empty())
        })?;
        let rider = board(&mut app)?;
        let mut notes = vec![format!("rider in seat {rider}")];
        let gpu = support::gpu::turn().context("offscreen distant renderer")?;
        let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
        app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
        // Each wall from 60, 90 and 120 units, 25 degrees from above,
        // straight on. Mirrors Off leaves plain silver glass, so any red in
        // it is the frame fighting through; Medium (the default) is saved
        // too, as a player sees it.
        let pitch = -25f32.to_radians();
        let back = Vec3::new(0.0, -pitch.sin(), pitch.cos());
        let mut failures = Vec::new();
        for distance in [60.0, 90.0, 120.0] {
            let mut shots = Vec::new();
            for (at, _) in walls {
                let centre = at + Vec3::Y * 6.0;
                camera_at(&mut app, centre + back * distance, 0.0, pitch)?;
                for (level_name, level) in [("off", "0"), ("medium", "2")] {
                    set_mirrors(&mut app, level)?;
                    settle(&mut app)?;
                    let pixels = capture(&mut app, &gpu, &mut renderer)?;
                    let colour = if shots.len() < 2 { "red" } else { "white" };
                    save(
                        &artifact.join(format!("wall-{colour}-{distance}-{level_name}.png")),
                        &pixels,
                    )?;
                    shots.push(pixels);
                }
            }
            // shots: red off, red medium, white off, white medium.
            // The walls' 16 x 12 units on screen (90 degree view), with a
            // margin.
            let per_unit = SIZE.0 as f32 * 0.5 / distance;
            let box_size = [18.0 * per_unit, 14.0 * per_unit].map(|v| v as usize);
            let (fought, glass) = fights(&shots[0], &shots[2], box_size);
            let note = format!("wall at {distance}: {fought} of {glass} glass px show the frame");
            eprintln!("{note}");
            if glass < 100 || fought * 200 > glass {
                failures.push(note.clone());
            }
            notes.push(note);
        }
        // The jeep, its rider and a nearby look for comparison: from above
        // as Max saw it, and from the side.
        set_mirrors(&mut app, "2")?;
        let jeep = app
            .network_view()
            .and_then(|v| {
                v.vehicle_poses
                    .values()
                    .next()
                    .map(|p| Vec3::from(p.position))
            })
            .context("jeep pose")?;
        for (name, distance, pitch_degrees, yaw) in [
            ("near-above", 12.0, -40.0f32, 0.0),
            ("above", 100.0, -40.0, 0.0),
            ("side", 100.0, -12.0, std::f32::consts::FRAC_PI_2),
        ] {
            let pitch = pitch_degrees.to_radians();
            let forward = Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            );
            camera_at(&mut app, jeep + Vec3::Y - forward * distance, yaw, pitch)?;
            settle(&mut app)?;
            let pixels = capture(&mut app, &gpu, &mut renderer)?;
            save(&artifact.join(format!("jeep-{name}.png")), &pixels)?;
        }
        app.gpu_stopped();
        ensure!(
            failures.is_empty(),
            "frames fight through the glass: {failures:?}"
        );
        Ok(notes)
    })();
    drop(scratch_dir);
    result
}

fn distant_mirrors_and_riders_draw_cleanly(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("distant-render")?;
    let notes = probe(f, &artifact)?;
    std::fs::write(artifact.join("report.txt"), notes.join("\n"))?;
    Ok(())
}

#[test]
fn a_frame_showing_through_glass_is_counted_and_its_edges_are_not() {
    let (w, h) = (SIZE.0 as usize, SIZE.1 as usize);
    let silver = [140u8, 145, 153, 255];
    let white_frame = [240u8, 240, 240, 255];
    let red_frame = [200u8, 20, 20, 255];
    // A block of glass in the middle, framed: white in one picture, red in
    // the other.
    let picture = |frame: [u8; 4]| {
        let mut pixels = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x.abs_diff(w / 2), y.abs_diff(h / 2));
                let colour = if dx < 20 && dy < 20 {
                    silver
                } else if dx < 24 && dy < 24 {
                    frame
                } else {
                    [0, 0, 0, 255]
                };
                pixels[(y * w + x) * 4..][..4].copy_from_slice(&colour);
            }
        }
        pixels
    };
    let (white, mut red_wall) = (picture(white_frame), picture(red_frame));
    let (fought, glass) = fights(&red_wall, &white, [100, 100]);
    assert_eq!(fought, 0);
    assert!(glass > 1000, "{glass}");
    // Three glass pixels lose to the frame.
    for x in [w / 2 - 5, w / 2, w / 2 + 5] {
        red_wall[(h / 2 * w + x) * 4..][..4].copy_from_slice(&red_frame);
    }
    assert_eq!(fights(&red_wall, &white, [100, 100]).0, 3);
}
