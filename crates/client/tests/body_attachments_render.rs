//! Bounded normal-App render probe for the body contract: the Playground's
//! Runner (a box body from one Add-On) holds the Pulse Carbine (another
//! Add-On) at its own hand, sits in the Dune Buggy (a third) with its legs
//! folded, and climbs out standing. Runs on the made-up content root with
//! the repository's Playground Add-Ons staged for the test's length; the
//! ignored variant runs on the generated v20 content. Loopback QUIC and an
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
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[macro_use]
mod support;
use support::{content_root::ContentRoot, wait};

synthetic_and_content!(ContentRoot: the_runner_holds_the_carbine_sits_in_the_buggy_and_climbs_out);

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
        label: Some("body offscreen target"),
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
        label: Some("body readback"),
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
    support::gpu::wait(&gpu.device, "the frame")?;
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

/// The Playground's Add-Ons and what they need, copied into a hidden folder
/// of the content root for the test's length, dependencies first.
struct Staged {
    dir: PathBuf,
    entries: Vec<bri_package::packages::PackageEntry>,
}
const PLAYGROUND: &[&str] = &[
    "blockhead_bot",
    "playground/playground-runner",
    "playground/playground-runner-look",
    "playground/playground-rifle",
    "playground/playground-buggy",
    "playground/playground-mode",
];
impl Staged {
    fn install(content: &Path) -> Result<Self> {
        let folder = format!(".body-add-ons-{}", std::process::id());
        let dir = content.join(&folder);
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        let mut entries = Vec::new();
        for source in PLAYGROUND {
            let source = repo.join(source);
            let info: bri_package::library::PackageInfo =
                serde_json::from_slice(&std::fs::read(source.join("package.json"))?)?;
            copy_dir(&source, &dir.join(&info.id))?;
            entries.push(bri_package::packages::PackageEntry {
                id: info.id.clone(),
                version: info.version.clone(),
                side: info.side().unwrap_or(bri_package::packages::Side::Shared),
                dir: format!("{folder}/{}", info.id),
                role: None,
            });
        }
        Ok(Self { dir, entries })
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn mounted(app: &App) -> bool {
    app.network_view()
        .and_then(|v| v.vitals.get(&v.owner))
        .is_some_and(|v| v.mounted.is_some())
}

/// The held Carbine, drawn at the Runner's own right hand: beside the body
/// (not at its feet or its centre) and at hand height above the feet.
fn holds_at_its_hand(app: &App, what: &str) -> Result<glam::Vec3> {
    let owner = app.network_view().context("view")?.owner;
    let item = app
        .held_image_transform(0)
        .with_context(|| format!("{what}: no Carbine drawn in the right hand"))?;
    let body = app.avatar_body(owner).context("no body")?;
    let local = body.inverse().transform_point3(item.w_axis.truncate());
    eprintln!("{what}: carbine at {local:?} in the body's frame");
    ensure!(
        local.x.abs() > 0.2 && local.y > 0.3,
        "{what}: the Carbine is not at the Runner's hand: {local:?}"
    );
    Ok(local)
}

fn the_runner_holds_the_carbine_sits_in_the_buggy_and_climbs_out(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("body-attachments")?;
    let state_dir = f.state()?;
    let state = state_dir.path();
    let staged = Staged::install(&f.root)?;
    let mut set = bri_package::packages::PackageSet::load_root(&f.root)?;
    set.packages.retain(|p| p.role.is_some());
    set.packages.extend(staged.entries.iter().cloned());
    let mut app = App::load(&f.root, state, SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.apply_packages(&set).context("the Playground Add-Ons")?;
    let mode = app
        .ui
        .core
        .game_modes
        .iter()
        .find(|m| m.id == "playground-mode:mode/playground")
        .cloned()
        .context("the Playground mode is not listed")?;
    app.ui.core.request(UiAction::HostGame {
        map: f.map.0.clone(),
        mode: ServerMode::SinglePlayer,
        game_mode: Some(mode.id.clone()),
        max_players: 1,
        server_name: "Body attachments render".into(),
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
    // A mode with no rules of its own still runs: its mini-game, body and
    // loadout (this hosted a plain v20 game before).
    ensure!(
        app.network_view().is_some_and(|v| v.minigames.len() == 1),
        "the Playground's mini-game did not start"
    );
    // The mode's loadout puts the Carbine in the first tool slot.
    app.ui.core.request(UiAction::UseTool { slot: 0 });
    pump(&mut app)?;
    until(&mut app, "the Carbine in hand", |a| {
        a.held_image_transform(0).is_some()
    })?;
    let gpu = support::gpu::turn().context("offscreen body renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    app.ui
        .core
        .request(UiAction::Game(GameAction::ToggleFirstPerson {
            fast: false,
        }));
    pump(&mut app)?;
    run_for(&mut app, 1.0)?;
    let owner = app.network_view().context("view")?.owner;
    ensure!(
        app.avatar_is_stand_in(owner) == Some(true),
        "the Runner is not drawn as its own box body: {:?}, archetype {:?}",
        app.avatar_is_stand_in(owner),
        app.network_view()
            .and_then(|v| v.poses.get(&owner))
            .map(|p| p.player.archetype)
    );
    // Complete: its sockets are declared, and a box body names only the
    // animations it cannot play (it has no clips of its own).
    let report = app.avatar_body_report(owner).context("no body")?;
    eprintln!("Runner report: {report:#?}");
    ensure!(
        !report.iter().any(|l| l.contains("socket")),
        "the Runner lacks a socket: {report:#?}"
    );

    let standing = holds_at_its_hand(&app, "standing")?;
    save(
        &artifact.join("runner-standing.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;

    // A saved build: a vehicle spawn brick with the buggy five units ahead.
    let (player, _) = app.local_motion().context("local player")?;
    let feet = glam::Vec3::from(player.feet);
    let forward = glam::Vec3::new(player.yaw.sin(), 0.0, -player.yaw.cos());
    let spot = feet + forward * 5.0;
    let view = app.network_view().context("view")?;
    let map_id = view.world.map_id.clone();
    let mut world =
        bri_world::World::new("Buggy".into(), map_id.clone(), view.world.palette.clone());
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
        vehicle: bri_world::ContentRef::Resolved("playground-buggy:vehicle/buggy".into()),
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
    std::fs::write(folder.join("buggy.world.json"), serde_json::to_vec(&build)?)?;
    app.ui.core.request(UiAction::LoadBricks {
        map: f.map.1.clone(),
        name: "buggy.world.json".into(),
        // Loaded as the host's own bricks, so the buggy is part of the
        // mode's mini-game.
        ownership: false,
    });
    pump(&mut app)?;
    until(&mut app, "buggy spawn", |a| {
        a.network_view().is_some_and(|v| !v.vehicles.is_empty())
    })?;
    run_for(&mut app, 1.0)?;
    // Back off, then run at the buggy and jump into its seat.
    let buggy = *app
        .network_view()
        .and_then(|v| v.vehicles.keys().next())
        .context("no buggy")?;
    // Walk to its side, clear of the body that overhangs its wheels, jump
    // straight up and lean in to land on the seat, as a player would.
    let mut boarded = false;
    'tries: for _ in 0..10 {
        for _ in 0..100 {
            let (at, _, _) = app.drawn_vehicle(buggy).context("buggy not drawn")?;
            let (me, _) = app.local_motion().context("local player")?;
            let to = at + glam::Vec3::new(2.2, 0.0, 0.0) - glam::Vec3::from(me.feet);
            if glam::Vec2::new(to.x, to.z).length() < 0.4 {
                break;
            }
            look(&mut app, to.x.atan2(-to.z), 0.0)?;
            held(&mut app, HeldControl::Forward, true)?;
            run_for(&mut app, 0.05)?;
        }
        held(&mut app, HeldControl::Forward, false)?;
        run_for(&mut app, 0.3)?;
        look(&mut app, -std::f32::consts::FRAC_PI_2, 0.0)?;
        held(&mut app, HeldControl::Jump, true)?;
        run_for(&mut app, 0.1)?;
        held(&mut app, HeldControl::Jump, false)?;
        run_for(&mut app, 0.4)?;
        held(&mut app, HeldControl::Forward, true)?;
        for _ in 0..30 {
            run_for(&mut app, 0.05)?;
            boarded = mounted(&app);
            if boarded {
                break 'tries;
            }
        }
        held(&mut app, HeldControl::Forward, false)?;
        run_for(&mut app, 0.3)?;
    }
    held(&mut app, HeldControl::Forward, false)?;
    ensure!(
        boarded,
        "never boarded the buggy: {:?}",
        app.ui.core.center_print
    );
    run_for(&mut app, 1.0)?;
    // Seated, the body lowers to the seat (its `seat` socket) and still
    // holds the Carbine at its hand.
    let seated = holds_at_its_hand(&app, "seated")?;
    ensure!(
        app.avatar_action(owner).is_some_and(|(a, _)| a == "sit"),
        "not sitting in the seat: {:?}",
        app.avatar_action(owner)
    );
    ensure!(
        seated.y < standing.y - 0.2,
        "the seated Runner did not lower: standing {standing:?}, seated {seated:?}"
    );
    save(
        &artifact.join("runner-seated.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;

    // Climb out and step clear (or it lands back in the seat): it stands
    // again, the Carbine back at hand height.
    held(&mut app, HeldControl::Jet, true)?;
    run_for(&mut app, 0.2)?;
    held(&mut app, HeldControl::Jet, false)?;
    until(&mut app, "off the buggy", |a| !mounted(a))?;
    held(&mut app, HeldControl::Left, true)?;
    run_for(&mut app, 0.6)?;
    held(&mut app, HeldControl::Left, false)?;
    run_for(&mut app, 1.5)?;
    ensure!(!mounted(&app), "landed back in the seat");
    ensure!(
        app.avatar_action(owner).is_some_and(|(a, _)| a != "sit"),
        "still sitting after climbing out: {:?}",
        app.avatar_action(owner)
    );
    let out = holds_at_its_hand(&app, "climbed out")?;
    ensure!(
        (out.y - standing.y).abs() < 0.1,
        "after climbing out the Carbine is at {out:?}, not back at {standing:?}"
    );
    save(
        &artifact.join("runner-climbed-out.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    app.gpu_stopped();
    drop(staged);
    Ok(())
}
