//! Brick item respawn ghosts through the real App, in single player and on a
//! LAN host with a joined guest: plant a brick, wrench a gun onto it, walk
//! into the gun, and watch it stay as a faded ghost until it respawns.
//! Timing is judged in sim ticks: the ghost is inspected under v20's longest
//! respawn, then the wrench restocks it and the ordinary 8 s is waited out.
//! v20: `ItemData::onPickup` -> `Item::Respawn` -> `fadeOut` / `fadeIn`.
//! Never opens a window or sends OS input.
//! Run: cargo test -p bri-client --test item_ghost --release -- --ignored --nocapture --test-threads=1
use anyhow::{Context, Result, bail, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
    world_items::{ItemIdentity, RESPAWN_GHOST_ALPHA},
};
use bri_sim::item_spawners::StaticItem;
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    screens::ScreenId,
};
use glam::Vec3;
use std::{
    f32::consts::{PI, TAU},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SIZE: (u32, u32) = (640, 480);
const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";
const BRICK: &str = "v20/brick/brick2x2data";
const GUN: &str = "v20.weapon.gunitem";
/// The ordinary respawn the test waits out, judged in sim ticks.
const RESPAWN_MS: u32 = 8000;
/// v20's longest item respawn (`$Game::Item::MaxRespawnTime`): holds the
/// first ghost while it is inspected, however slow the machine.
const HELD_RESPAWN_MS: u32 = 300_000;
/// Third-person captures face this far beside the item.
const THIRD_PERSON_TURN: f32 = 0.3;

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("Connection failed: {reason}");
    }
    Ok(())
}
/// The newest server tick every app has seen; None until all are in game.
fn seen_tick(apps: &[&mut App]) -> Option<u64> {
    apps.iter()
        .map(|a| a.network_view().map(|v| v.tick))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .min()
}
/// A wait that stops advancing for this many times its budget of wall time
/// has a stopped game, not a slow one.
const STALLED: u32 = 20;
/// Step every app until `ready` holds. `apps[0]` is the acting player.
/// `budget` is game time: once every app is in game it counts the server
/// ticks all of them have seen, so a loaded machine that slows the game and
/// the clients stretches the wait with them. Before that (loading, joining)
/// it is wall time.
fn until(
    apps: &mut [&mut App],
    what: &str,
    budget: Duration,
    ready: impl Fn(&[&mut App]) -> bool,
) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    let mut first_tick = None;
    loop {
        let now = Instant::now();
        for app in apps.iter_mut() {
            step(app, now.duration_since(previous))?;
        }
        previous = now;
        if ready(apps) {
            return Ok(());
        }
        let tick = seen_tick(apps);
        first_tick = first_tick.or(tick);
        let spent = match (first_tick, tick) {
            (Some(first), Some(tick)) => tick - first >= ticks(budget.as_millis() as u32),
            _ => start.elapsed() >= budget,
        };
        ensure!(!spent, "Timed out waiting for {what}");
        ensure!(
            start.elapsed() < budget * STALLED + Duration::from_secs(60),
            "Timed out waiting for {what}: the game stopped advancing"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
/// Let `time` of game time pass (server ticks every app has seen).
fn run_for(apps: &mut [&mut App], time: Duration) -> Result<()> {
    let start = seen_tick(apps).context("waiting in game time out of game")?;
    let end = start + ticks(time.as_millis() as u32);
    until(apps, "time", time + Duration::from_secs(5), |a| {
        seen_tick(a).is_some_and(|t| t >= end)
    })
}
fn act(app: &mut App, action: UiAction) -> Result<()> {
    app.ui.core.request(action);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    app.ui.update(0);
    Ok(())
}
fn hold(app: &mut App, control: HeldControl, down: bool) -> Result<()> {
    act(app, UiAction::Game(GameAction::Held { control, down }))
}
fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}
fn settled(app: &App) -> bool {
    app.local_motion()
        .is_some_and(|(p, _)| p.grounded && Vec3::from(p.velocity).length() < 0.001)
}
fn feet(app: &App) -> Vec3 {
    let v = app.network_view().unwrap();
    Vec3::from(v.poses[&v.owner].player.feet)
}
fn guns(app: &App) -> usize {
    let v = app.network_view().unwrap();
    v.tools.get(&v.owner).map_or(0, |t| {
        t.slots.iter().filter(|s| s.as_deref() == Some(GUN)).count()
    })
}
fn spawned(app: &App, brick: u64) -> Option<StaticItem> {
    app.network_view()?
        .weapons
        .static_items
        .iter()
        .find(|i| i.brick == brick)
        .cloned()
}
/// Replicated state: the item is waiting to respawn.
fn ghosted(app: &App, brick: u64) -> bool {
    let tick = app.network_view().map_or(0, |v| v.tick);
    spawned(app, brick).is_some_and(|i| i.available_at > tick)
}
/// Local presentation: the alpha this client draws the brick's item with.
fn drawn_alpha(app: &App, brick: u64) -> Option<f32> {
    app.world_item_instances()
        .find(|(id, _)| *id == ItemIdentity::Static(brick))
        .map(|(_, t)| t.tint[3])
}
fn ticks(ms: u32) -> u64 {
    (u64::from(ms) * 120).div_ceil(1000)
}
fn flat_distance(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, 0., a.z - b.z).length()
}
/// Turn to face `target`. Look deltas are mouse input, scaled by the
/// current FOV (`getMouseAdjustAmount`), so converge on the angle.
fn look_at(app: &mut App, target: Vec3) -> Result<()> {
    look_beside(app, target, 0.)
}
/// Face `turn` radians to the side of `target`.
fn look_beside(app: &mut App, target: Vec3, turn: f32) -> Result<()> {
    let (state, eye) = app.local_motion().context("local motion")?;
    let eye = eye.unwrap_or(Vec3::from(state.feet) + Vec3::Y * 2.);
    let d = target - eye;
    let yaw = d.x.atan2(-d.z) + turn;
    let pitch = d.y.atan2(Vec3::new(d.x, 0., d.z).length());
    face(app, (yaw, pitch))
}
/// Set the view angles exactly (to capture the same frame twice).
fn face(app: &mut App, (yaw, pitch): (f32, f32)) -> Result<()> {
    for _ in 0..40 {
        let (current_yaw, current_pitch) = app.controls.view_angles();
        let delta = (yaw - current_yaw + PI).rem_euclid(TAU) - PI;
        if delta.abs() < 1e-5 && (pitch - current_pitch).abs() < 1e-5 {
            return Ok(());
        }
        act(
            app,
            UiAction::Game(GameAction::Look {
                yaw: delta,
                pitch: current_pitch - pitch,
            }),
        )?;
    }
    let (current_yaw, _) = app.controls.view_angles();
    ensure!(
        ((yaw - current_yaw + PI).rem_euclid(TAU) - PI).abs() < 0.001,
        "could not face yaw {yaw}"
    );
    Ok(())
}

struct Camera<'a> {
    gpu: &'a Headless,
    renderer: UiRenderer,
    artifact: PathBuf,
}
impl Camera<'_> {
    fn capture(&mut self, app: &mut App, name: &str) -> Result<Vec<u8>> {
        eprintln!(
            "capture {name}: feet {} angles {:?} third {}",
            feet(app),
            app.controls.view_angles(),
            app.controls.third_person
        );
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let gpu = self.gpu;
        let extent = wgpu::Extent3d {
            width: SIZE.0,
            height: SIZE.1,
            depth_or_array_layers: 1,
        };
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("item ghost offscreen target"),
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
                ui_renderer: &mut self.renderer,
            })?,
            "App did not render a session camera"
        );
        let row = (SIZE.0 * 4).div_ceil(256) * 256;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("item ghost readback"),
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
        image::save_buffer(
            self.artifact.join(format!("{name}.png")),
            &pixels,
            SIZE.0,
            SIZE.1,
            image::ColorType::Rgba8,
        )?;
        Ok(pixels)
    }
}
/// Pixels that differ perceptibly.
fn changed_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 8))
        .count()
}
fn set_third_person(apps: &mut [&mut App], on: bool) -> Result<()> {
    if apps[0].controls.third_person != on {
        act(
            apps[0],
            UiAction::Game(GameAction::ToggleFirstPerson { fast: true }),
        )?;
    }
    ensure!(
        apps[0].controls.third_person == on,
        "camera mode did not change"
    );
    // Wait for the camera slide to finish: aim and captures come from it.
    let end = if on { 1. } else { 0. };
    until(apps, "camera slide", Duration::from_secs(10), |a| {
        a[0].controls.camera_pos() == end
    })
}
/// The actor backs out from the brick along the item's side `out`, then
/// faces the item and stands still.
fn back_off(apps: &mut [&mut App], item: Vec3, out: Vec3) -> Result<()> {
    look_at(apps[0], item - out * 2.)?;
    hold(apps[0], HeldControl::Backward, true)?;
    until(
        apps,
        "backing away from the item",
        Duration::from_secs(5),
        |a| flat_distance(feet(a[0]), item) > 2.,
    )?;
    hold(apps[0], HeldControl::Backward, false)?;
    until(apps, "standing still", Duration::from_secs(5), |a| {
        settled(a[0])
    })?;
    look_at(apps[0], item)?;
    run_for(apps, Duration::from_millis(100))
}
/// Walk at the item until `done`, or for `limit` of game time.
fn walk_into(
    apps: &mut [&mut App],
    item: Vec3,
    limit: Duration,
    done: impl Fn(&[&mut App]) -> bool,
) -> Result<bool> {
    look_at(apps[0], item)?;
    hold(apps[0], HeldControl::Forward, true)?;
    let end = seen_tick(apps).context("walking out of game")? + ticks(limit.as_millis() as u32);
    let result = until(
        apps,
        "walking into the item",
        limit + Duration::from_secs(1),
        |a| done(a) || seen_tick(a).is_some_and(|t| t >= end),
    );
    hold(apps[0], HeldControl::Forward, false)?;
    result?;
    Ok(done(apps))
}

/// Swing the wrench at `brick` and Send its dialog with a gun on `side`.
fn wrench_gun(
    apps: &mut [&mut App],
    brick: u64,
    placed: Vec3,
    side: u8,
    respawn_ms: u32,
) -> Result<()> {
    look_at(apps[0], placed)?;
    // Swing only once the server holds this aim.
    until(apps, "server aim", Duration::from_secs(5), |a| {
        let v = a[0].network_view().unwrap();
        let p = &v.poses[&v.owner].player;
        let (yaw, pitch) = a[0].controls.view_angles();
        ((p.yaw - yaw + PI).rem_euclid(TAU) - PI).abs() < 0.01 && (p.pitch - pitch).abs() < 0.01
    })?;
    act(apps[0], UiAction::UseTool { slot: 1 })?;
    hold(apps[0], HeldControl::Fire, true)?;
    until(apps, "wrench dialog", Duration::from_secs(5), |a| {
        a[0].ui
            .stack()
            .contains(&ScreenId::Wrench(WrenchVariant::Normal))
    })
    .with_context(|| {
        let v = apps[0].network_view().unwrap();
        format!(
            "brick {placed} feet {} stack {:?} images {:?} audio {:?} angles {:?} eye {:?}",
            feet(apps[0]),
            apps[0].ui.stack(),
            v.weapons.images.get(&v.owner),
            apps[0].audio_requests(),
            apps[0].controls.view_angles(),
            apps[0].local_motion().map(|m| m.1),
        )
    })?;
    hold(apps[0], HeldControl::Fire, false)?;
    let mut data = apps[0].ui.core.wrench.values(WrenchVariant::Normal);
    data.item = Some(GUN.into());
    data.item_pos = side;
    data.item_dir = 2;
    data.item_respawn_ms = respawn_ms;
    act(
        apps[0],
        UiAction::SendWrench {
            brick,
            variant: WrenchVariant::Normal,
            data,
        },
    )?;
    until(apps, "wrench applied", Duration::from_secs(8), |a| {
        a[0].network_view().unwrap().world.bricks[&brick]
            .item_spawn
            .respawn_ms
            == respawn_ms
    })?;
    apps[0].ui.core.pop(ScreenId::Wrench(WrenchVariant::Normal));
    act(apps[0], UiAction::UnUseTool)
}

/// Plant a 2x2 brick ahead of `apps[0]`, wrench a gun onto its side facing
/// the player, pick the gun up by walking into it, then follow the ghost
/// until it respawns and can be picked up again. Every app must see the
/// ghost and its return.
fn ghost_cycle(
    apps: &mut [&mut App],
    camera: &mut Camera,
    label: &str,
) -> Result<serde_json::Value> {
    until(apps, "actor landing", Duration::from_secs(30), |a| {
        settled(a[0])
    })?;
    // A LAN host's Windows Firewall question is outside this test: close it
    // unanswered, as a player would before building.
    for app in apps.iter_mut() {
        while app.ui.stack().contains(&ScreenId::MessageBox) {
            app.ui.core.pop(ScreenId::MessageBox);
            app.ui.update(0);
        }
    }
    let bricks_before: Vec<u64> = apps[0]
        .network_view()
        .unwrap()
        .world
        .bricks
        .keys()
        .copied()
        .collect();
    act(
        apps[0],
        UiAction::Game(GameAction::Look {
            yaw: PI,
            pitch: 1.0,
        }),
    )?;
    act(
        apps[0],
        UiAction::InstantUseBrick {
            brick: BRICK.into(),
        },
    )?;
    hold(apps[0], HeldControl::Fire, true)?;
    hold(apps[0], HeldControl::Fire, false)?;
    ensure!(
        apps[0].building().and_then(|b| b.ghost()).is_some(),
        "Brick Fire did not deploy a ghost brick"
    );
    act(apps[0], UiAction::Game(GameAction::PlantBrick))?;
    until(apps, "planted brick", Duration::from_secs(8), |a| {
        a.iter()
            .all(|app| app.network_view().unwrap().world.bricks.len() == bricks_before.len() + 1)
    })?;
    act(apps[0], UiAction::Game(GameAction::CancelBrick))?;
    let (brick, placed) = apps[0]
        .network_view()
        .unwrap()
        .world
        .bricks
        .iter()
        .find(|(id, _)| !bricks_before.contains(id))
        .map(|(id, b)| (*id, Vec3::from(b.position)))
        .unwrap();
    let start = feet(apps[0]);
    ensure!(
        (placed.y - start.y).abs() < 1.5 && flat_distance(placed, start) < 4.,
        "brick {placed} landed out of reach of the player at {start}"
    );

    // Wrench it: the gun goes on the brick side facing the player. The first
    // wait is v20's longest, so the ghost can be inspected at any machine
    // speed; the wrench then restocks it and sets the ordinary 8 s.
    let toward = start - placed;
    let (side, out) = [
        (2u8, Vec3::NEG_Z),
        (3, Vec3::X),
        (4, Vec3::Z),
        (5, Vec3::NEG_X),
    ]
    .into_iter()
    .max_by(|a, b| a.1.dot(toward).total_cmp(&b.1.dot(toward)))
    .unwrap();
    let guns_before = guns(apps[0]);
    wrench_gun(apps, brick, placed, side, HELD_RESPAWN_MS)?;
    until(
        apps,
        "wrenched item on every client",
        Duration::from_secs(8),
        |a| a.iter().all(|app| spawned(app, brick).is_some()),
    )?;
    let item = Vec3::from(spawned(apps[0], brick).unwrap().position);
    // Pick it up by walking into it; a gun wrenched against the player's
    // side is taken as soon as it appears, like v20's contact pickup.
    let touched_at_once = guns(apps[0]) > guns_before;
    let picked = touched_at_once
        || walk_into(apps, item, Duration::from_secs(5), |a| {
            guns(a[0]) > guns_before
        })?;
    ensure!(picked, "walking into the item did not pick it up");
    let picked_tick = apps[0].network_view().unwrap().tick;
    let held = spawned(apps[0], brick).unwrap().available_at;
    ensure!(
        held > picked_tick + ticks(HELD_RESPAWN_MS) - 120,
        "the pickup did not start the brick's respawn wait"
    );
    until(apps, "ghost on every client", Duration::from_secs(5), |a| {
        a.iter().all(|app| {
            ghosted(app, brick)
                && drawn_alpha(app, brick) == Some(RESPAWN_GHOST_ALPHA)
                && app.world_item_stats().cooling_down >= 1
        })
    })?;

    back_off(apps, item, out)?;
    set_third_person(apps, false)?;
    let first_view = apps[0].controls.view_angles();
    let ghost_first = camera.capture(apps[0], &format!("{label}-ghost-first-person"))?;
    // The chase camera looks past the player; turn so the body hides nothing.
    set_third_person(apps, true)?;
    look_beside(apps[0], item, THIRD_PERSON_TURN)?;
    run_for(apps, Duration::from_millis(100))?;
    let third_view = apps[0].controls.view_angles();
    let ghost_third = camera.capture(apps[0], &format!("{label}-ghost-third-person"))?;
    set_third_person(apps, false)?;
    ensure!(ghosted(apps[0], brick), "the held ghost returned early");

    // The wrench's Send replaces the faded Item with a solid, available one
    // (`fxDTSBrick::setItem`) and sets the ordinary respawn time.
    wrench_gun(apps, brick, placed, side, RESPAWN_MS)?;
    until(
        apps,
        "restocked item on every client",
        Duration::from_secs(8),
        |a| {
            a.iter().all(|app| {
                !ghosted(app, brick)
                    && drawn_alpha(app, brick) == Some(1.)
                    && app.world_item_stats().cooling_down == 0
            })
        },
    )?;
    // Let the wrench's hit sparks burn out before the matching frames.
    run_for(apps, Duration::from_secs(3))?;
    face(apps[0], first_view)?;
    run_for(apps, Duration::from_millis(100))?;
    let full_first = camera.capture(apps[0], &format!("{label}-solid-first-person"))?;
    set_third_person(apps, true)?;
    face(apps[0], third_view)?;
    run_for(apps, Duration::from_millis(100))?;
    let full_third = camera.capture(apps[0], &format!("{label}-solid-third-person"))?;
    look_at(apps[0], item)?;
    set_third_person(apps, false)?;
    let first_diff = changed_pixels(&ghost_first, &full_first);
    let third_diff = changed_pixels(&ghost_third, &full_third);
    ensure!(
        first_diff > 20,
        "first person: ghost and solid item look alike ({first_diff} px)"
    );
    ensure!(
        third_diff > 20,
        "third person: ghost and solid item look alike ({third_diff} px)"
    );

    // Throw the first gun away from the brick: the last test needs two free
    // slots (five tools, `maxTools`).
    look_at(apps[0], item + out * 6.)?;
    let slot = {
        let v = apps[0].network_view().unwrap();
        v.tools[&v.owner]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(GUN))
            .context("no gun to drop")?
    };
    let carried = guns(apps[0]);
    act(apps[0], UiAction::UseTool { slot })?;
    until(apps, "gun in hand", Duration::from_secs(5), |a| {
        let v = a[0].network_view().unwrap();
        v.tools[&v.owner].selected == Some(slot)
    })?;
    act(apps[0], UiAction::Game(GameAction::DropTool))?;
    until(apps, "gun thrown", Duration::from_secs(5), |a| {
        guns(a[0]) < carried
    })?;
    // Available again: the next touch picks it up and fades it for the
    // brick's 8 s (960 ticks at 120 Hz).
    let guns_before = guns(apps[0]);
    let picked = walk_into(apps, item, Duration::from_secs(5), |a| {
        guns(a[0]) > guns_before
    })?;
    ensure!(picked, "the restocked item could not be picked up");
    let seen = apps[0].network_view().unwrap().tick;
    let second = spawned(apps[0], brick).unwrap().available_at;
    let wait = second.saturating_sub(seen);
    ensure!(
        (ticks(RESPAWN_MS) - 120..=ticks(RESPAWN_MS)).contains(&wait),
        "respawn {wait} ticks after the pickup was seen, not {}",
        ticks(RESPAWN_MS)
    );
    let wall = Instant::now();
    until(apps, "second ghost", Duration::from_secs(5), |a| {
        a.iter()
            .all(|app| ghosted(app, brick) && drawn_alpha(app, brick) == Some(RESPAWN_GHOST_ALPHA))
    })?;
    // `canPickup = 0`: the player stays in contact with the ghost and takes
    // nothing until `fadeIn`, then takes it at once. Judged in sim ticks.
    let holding = guns(apps[0]);
    let taken = std::cell::Cell::new(None);
    until(
        apps,
        "standing in the ghost until it returns",
        Duration::from_millis(u64::from(RESPAWN_MS) * 4 + 30_000),
        |a| {
            let tick = a[0].network_view().unwrap().tick;
            if guns(a[0]) > holding && taken.get().is_none() {
                taken.set(Some(tick));
            }
            taken.get().is_some()
        },
    )?;
    let taken = taken.get().unwrap();
    ensure!(
        taken >= second,
        "took the ghost at tick {taken}, before its respawn at {second}"
    );
    ensure!(
        taken - second <= 120,
        "took the returned item {} ticks late",
        taken - second
    );
    let respawn_wall_seconds = wall.elapsed().as_secs_f32();
    let ghost_contact = flat_distance(feet(apps[0]), item);
    Ok(serde_json::json!({
        "brick": brick, "item_side": side, "item": item.to_array(),
        "picked_tick": picked_tick, "held_available_at": held,
        "second_seen_tick": seen, "second_available_at": second,
        "second_taken_tick": taken,
        "respawn_wall_seconds": respawn_wall_seconds,
        "ghost_contact_distance": ghost_contact,
        "picked_on_appearing": touched_at_once,
        "first_person_changed_pixels": first_diff,
        "third_person_changed_pixels": third_diff,
        "clients": apps.len(),
    }))
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn load(artifact: &Path, name: &str) -> Result<App> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let state = artifact.join(format!("state-{name}-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&state)?;
    let mut app = App::load(&workspace().join("content"), &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.core.settings.avatar.lan_name = name.into();
    Ok(app)
}
fn host(app: &mut App, mode: ServerMode) -> Result<()> {
    act(
        app,
        UiAction::HostGame {
            map: BEDROOM.into(),
            mode,
            game_mode: None,
            max_players: 4,
            server_name: "Item ghost probe".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    until(&mut [app], "host in game", Duration::from_secs(90), |a| {
        in_game(a[0])
    })
}

#[test]
#[ignore = "converted native v20 content, loopback QUIC and an offscreen GPU; no window"]
fn single_player_pickup_leaves_a_ghost_until_the_item_respawns() -> Result<()> {
    let artifact = workspace().join("artifacts/item-ghost");
    std::fs::create_dir_all(&artifact)?;
    let mut app = load(&artifact, "Solo")?;
    let gpu = Headless::new().context("offscreen adapter")?;
    let mut camera = Camera {
        gpu: &gpu,
        renderer: UiRenderer::new(&gpu.device, &gpu.queue),
        artifact: artifact.clone(),
    };
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    host(&mut app, ServerMode::SinglePlayer)?;
    let report = ghost_cycle(&mut [&mut app], &mut camera, "single-player")?;
    std::fs::write(
        artifact.join("single-player.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    app.gpu_stopped();
    Ok(())
}

#[test]
#[ignore = "converted native v20 content, loopback UDP and an offscreen GPU; no window"]
fn lan_host_and_guest_both_see_each_others_pickup_ghosts() -> Result<()> {
    let port = std::net::UdpSocket::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    // SAFETY: set before any host or join starts; tests run one at a time.
    unsafe {
        std::env::set_var("BRI_TEST_HOST_PORT", port.to_string());
        std::env::set_var("BRI_TEST_DISCOVERY_PORT", "0");
    }
    let artifact = workspace().join("artifacts/item-ghost");
    std::fs::create_dir_all(&artifact)?;
    let mut host_app = load(&artifact, "Hosty")?;
    let mut guest = load(&artifact, "Guesty")?;
    let gpu = Headless::new().context("offscreen adapter")?;
    let mut camera = Camera {
        gpu: &gpu,
        renderer: UiRenderer::new(&gpu.device, &gpu.queue),
        artifact: artifact.clone(),
    };
    host_app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    guest.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    host(&mut host_app, ServerMode::Lan)?;
    act(
        &mut guest,
        UiAction::JoinServer {
            address: format!("127.0.0.1:{port}"),
            password: String::new(),
        },
    )?;
    until(
        &mut [&mut host_app, &mut guest],
        "guest in game",
        Duration::from_secs(90),
        |a| in_game(a[1]) && a.iter().all(|app| app.ui.core.players.len() == 2),
    )?;
    // The joined guest's pickup, seen by the host; then the host's own,
    // seen by the guest.
    let guest_report = ghost_cycle(&mut [&mut guest, &mut host_app], &mut camera, "lan-guest")?;
    let host_report = ghost_cycle(&mut [&mut host_app, &mut guest], &mut camera, "lan-host")?;
    let report = serde_json::json!({"guest_pickup": guest_report, "host_pickup": host_report});
    std::fs::write(
        artifact.join("lan.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    host_app.gpu_stopped();
    guest.gpu_stopped();
    Ok(())
}
