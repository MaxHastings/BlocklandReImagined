//! Overnight QA matrix against a packaged build's content: a LAN host and a
//! second headless client visit every stock map under every game mode the
//! enabled Add-Ons offer. On each map the guest plants and hammers a brick,
//! the host saves and reloads, then the host changes map. Never creates a
//! window or OS input; the host binds UDP 28000/28050.
//!
//! BRI_CONTENT_ROOT=<build>/content BRI_QA_OUT=<dir> \
//!   cargo test -p bri-client --test night_qa --release -- --ignored --nocapture
use anyhow::{Context, Result, bail, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    models::admin::AdminAction,
    screens::ScreenId,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const SIZE: (u32, u32) = (960, 720);
const BRICK: &str = "v20/brick/brick2x4data";
/// How far below the horizon the guest aims to build and hammer.
const DOWN: f32 = 1.0;

/// The QA runs need their environment; without it (the push gate, a plain
/// `cargo test --include-ignored`) they skip instead of failing.
fn qa_env(vars: &[&str]) -> bool {
    let missing: Vec<_> = vars
        .iter()
        .filter(|v| std::env::var_os(v).is_none())
        .collect();
    if !missing.is_empty() {
        eprintln!("night QA needs {missing:?}; skipped");
    }
    missing.is_empty()
}

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    let commands = app.pump()?;
    ensure!(
        commands.is_empty(),
        "Unexpected window command {commands:?}"
    );
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("Connection failed: {reason}");
    }
    Ok(())
}

pub struct Pair {
    pub host: Box<App>,
    pub guest: Box<App>,
    previous: Instant,
}

impl Pair {
    fn until(
        &mut self,
        what: &str,
        timeout: Duration,
        ready: impl Fn(&App, &App) -> bool,
    ) -> Result<()> {
        let start = Instant::now();
        loop {
            let now = Instant::now();
            let elapsed = now
                .duration_since(self.previous)
                .min(Duration::from_millis(100));
            self.previous = now;
            step(&mut self.host, elapsed).context("host")?;
            step(&mut self.guest, elapsed).context("guest")?;
            if ready(&self.host, &self.guest) {
                return Ok(());
            }
            ensure!(start.elapsed() < timeout, "Timed out waiting for {what}");
            thread::sleep(Duration::from_millis(8));
        }
    }
    fn settle(&mut self, time: Duration) -> Result<()> {
        let start = Instant::now();
        self.until("settle", time + Duration::from_secs(1), |_, _| {
            start.elapsed() >= time
        })
    }
}

fn request(app: &mut App, action: UiAction) -> Result<()> {
    app.ui.core.request(action);
    let commands = app.pump()?;
    ensure!(
        commands.is_empty(),
        "Unexpected window command {commands:?}"
    );
    app.ui.update(0);
    Ok(())
}

fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}

fn bricks(app: &App) -> usize {
    app.network_view().map_or(0, |v| v.world.bricks.len())
}

/// One of our 2x4 bricks stands at `spot` (worlds may also generate bricks).
fn brick_at(app: &App, spot: [f32; 3]) -> bool {
    app.network_view().is_some_and(|v| {
        v.world.bricks.values().any(|b| {
            b.position == spot
                && matches!(&b.definition, bri_world::ContentRef::Resolved(d) if d == BRICK)
        })
    })
}

fn grounded(app: &App) -> bool {
    app.network_view()
        .is_some_and(|v| v.poses.get(&v.owner).is_some_and(|p| p.player.grounded))
}

/// Hosting pauses while this file exists, so another test run can use the ports.
fn pause_file() -> Option<PathBuf> {
    std::env::var_os("BRI_QA_OUT").map(|o| PathBuf::from(o).join("PAUSE"))
}

fn paused() -> bool {
    pause_file().is_some_and(|p| p.exists())
}

/// Wait until UDP 28000 is free so a concurrent test run cannot collide.
fn wait_for_port() -> Result<()> {
    while paused() {
        thread::sleep(Duration::from_secs(5));
    }
    let start = Instant::now();
    loop {
        if std::net::UdpSocket::bind("0.0.0.0:28000").is_ok() {
            return Ok(());
        }
        ensure!(
            start.elapsed() < Duration::from_secs(1800),
            "UDP 28000 stayed busy"
        );
        thread::sleep(Duration::from_secs(5));
    }
}

pub fn capture(
    app: &mut App,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    scene: bool,
) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("night qa capture"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let drew = scene
        && app.render_scene(&mut bri_client::platform::RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target: &view,
            format,
            size: SIZE,
            ui_renderer: renderer,
        })?;
    renderer.render(
        &gpu.device,
        &gpu.queue,
        &mut encoder,
        &view,
        format,
        SIZE,
        app.ui.scale(),
        &app.ui.core.pack,
        &app.ui.draw(),
        (!drew).then_some(wgpu::Color::BLACK),
    );
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("night qa readback"),
        size: u64::from(row) * u64::from(SIZE.1),
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
                rows_per_image: Some(SIZE.1),
            },
        },
        extent,
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    rx.recv_timeout(Duration::from_secs(5))??;
    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((SIZE.0 * SIZE.1 * 4) as usize);
    for bytes in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&bytes[..SIZE.0 as usize * 4]);
    }
    drop(mapped);
    readback.unmap();
    Ok(pixels)
}

fn save_png(path: &Path, pixels: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    image::save_buffer(path, pixels, SIZE.0, SIZE.1, image::ColorType::Rgba8)?;
    Ok(())
}

#[derive(Serialize, Default)]
struct MapReport {
    mode: String,
    map: String,
    reached_by: String,
    steps: Vec<String>,
    error: Option<String>,
    seconds: f32,
    warnings: Vec<String>,
    errors: Vec<String>,
    /// Lines logged five or more times on this map.
    spam: BTreeMap<String, usize>,
}

fn console_since(report: &mut MapReport) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for line in bri_console::log::lines() {
        *counts.entry(line.text.clone()).or_default() += 1;
        match line.level {
            bri_console::Level::Warning if !report.warnings.contains(&line.text) => {
                report.warnings.push(line.text)
            }
            bri_console::Level::Error if !report.errors.contains(&line.text) => {
                report.errors.push(line.text)
            }
            _ => {}
        }
    }
    report.spam = counts.into_iter().filter(|(_, n)| *n >= 5).collect();
    bri_console::log::clear();
}

/// Look is a mouse delta; turn it into an absolute aim (`down` radians
/// below the horizon).
fn aim(app: &mut App, yaw: f32, down: f32) -> Result<()> {
    for _ in 0..3 {
        let scale = app.controls.fov() / 90.0;
        let turn = (yaw - app.controls.yaw + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let tilt = down + app.controls.pitch;
        if turn.abs() < 1e-3 && tilt.abs() < 1e-3 {
            break;
        }
        request(
            app,
            UiAction::Game(GameAction::Look {
                yaw: turn / scale,
                pitch: tilt / scale,
            }),
        )?;
    }
    ensure!(
        (app.controls.pitch + down).abs() < 0.01,
        "aim missed: controls yaw {} pitch {}, wanted {yaw} {}",
        app.controls.yaw,
        app.controls.pitch,
        -down
    );
    Ok(())
}

/// The guest aims at the floor and plants one brick, trying four headings
/// until the ghost deploys. Returns the heading used.
pub fn plant(pair: &mut Pair, start: f32) -> Result<f32> {
    let before = bricks(&pair.guest);
    let mut used = None;
    for quarter in 0..4 {
        let yaw = start + quarter as f32 * std::f32::consts::FRAC_PI_2;
        aim(&mut pair.guest, yaw, DOWN)?;
        // The predicted player takes the new look on the next ticks.
        pair.settle(Duration::from_millis(150))?;
        request(
            &mut pair.guest,
            UiAction::InstantUseBrick {
                brick: BRICK.into(),
            },
        )?;
        for down in [true, false] {
            request(
                &mut pair.guest,
                UiAction::Game(GameAction::Held {
                    control: HeldControl::Fire,
                    down,
                }),
            )?;
            pair.settle(Duration::from_millis(100))?;
        }
        // A ghost on another player's feet is refused as Stuck (as in v20);
        // Stress Lab's generated spawns stand players two units apart.
        let ghost = pair
            .guest
            .building()
            .and_then(|b| b.ghost())
            .map(|g| g.position);
        let on_someone = ghost.is_some_and(|g| {
            pair.guest.network_view().is_some_and(|v| {
                v.poses.iter().any(|(owner, p)| {
                    *owner != v.owner
                        && (p.player.feet[0] - g[0]).abs() < 1.5
                        && (p.player.feet[2] - g[2]).abs() < 1.5
                })
            })
        });
        if ghost.is_some() && !on_someone {
            used = Some(yaw);
            break;
        }
    }
    let yaw = used.with_context(|| {
        format!(
            "Brick fire did not deploy a ghost in any direction; images {:?}, equipment {:?}, target {:?}, presented {:?}, controls ({}, {}), stack {:?}, console {:?}",
            pair.guest.network_view().and_then(|v| v.weapons.images.get(&v.owner).map(|i| i.iter().map(|i| format!("{i:?}")).collect::<Vec<_>>())),
            pair.guest.building().map(|b| b.equipment().clone()),
            pair.guest.building().zip(pair.guest.presented_local()).map(|(b, p)| {
                b.target(b.archetypes().eye(p), p.forward(), 15.0).map(|h| h.map(|h| h.position)).map_err(|e| e.to_string())
            }),
            pair.guest.presented_local().map(|p| (p.feet, p.yaw, p.pitch)),
            pair.guest.controls.yaw,
            pair.guest.controls.pitch,
            pair.guest.ui.stack(),
            bri_console::log::lines().iter().rev().take(5).map(|l| l.text.clone()).collect::<Vec<_>>()
        )
    })?;
    let spot = pair
        .guest
        .building()
        .and_then(|b| b.ghost())
        .map(|g| g.position)
        .unwrap();
    request(&mut pair.guest, UiAction::Game(GameAction::PlantBrick))?;
    let _ = before;
    // The plant-error icon hides after 800 ms; remember any it showed.
    let shown = std::cell::RefCell::new(None);
    pair.until("planted brick on both", Duration::from_secs(10), |h, g| {
        if let Some((error, _)) = &g.ui.core.plant_error {
            *shown.borrow_mut() = Some(format!("{error:?}"));
        }
        brick_at(h, spot) && brick_at(g, spot) && g.pending_requests() == 0
    })
    .with_context(|| {
        let chat: Vec<_> = pair
            .guest
            .ui
            .core
            .chat
            .lines
            .iter()
            .rev()
            .take(3)
            .map(|l| l.text.clone())
            .collect();
        format!(
            "guest chat tail {chat:?}, plant error shown {:?}, ghost was {spot:?}, players' feet {:?}, nearest host bricks {:?}",
            shown.borrow(),
            pair.host.network_view().map(|v| v.poses.values().map(|p| p.player.feet).collect::<Vec<_>>()),
            pair.host.network_view().map(|v| {
                let mut near: Vec<_> = v
                    .world
                    .bricks
                    .values()
                    .filter(|b| (b.position[0] - spot[0]).abs() < 1.5 && (b.position[2] - spot[2]).abs() < 1.5 && (b.position[1] - spot[1]).abs() < 1.5)
                    .map(|b| (b.position, format!("{:?}", b.definition)))
                    .collect();
                near.truncate(4);
                near
            })
        )
    })?;
    request(&mut pair.guest, UiAction::Game(GameAction::CancelBrick))?;
    Ok(yaw)
}

/// The guest hammers the brick it aims at until one brick is gone.
pub fn hammer(pair: &mut Pair, yaw: f32) -> Result<()> {
    let before = bricks(&pair.host);
    request(&mut pair.guest, UiAction::UseTool { slot: 0 })?;
    aim(&mut pair.guest, yaw, DOWN)?;
    pair.settle(Duration::from_millis(150))?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        request(
            &mut pair.guest,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: true,
            }),
        )?;
        pair.settle(Duration::from_millis(250))?;
        request(
            &mut pair.guest,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: false,
            }),
        )?;
        pair.settle(Duration::from_millis(250))?;
        if bricks(&pair.host) < before && bricks(&pair.guest) < before {
            return Ok(());
        }
    }
    let view = pair.guest.network_view().context("guest view")?;
    let pose = view
        .poses
        .get(&view.owner)
        .map(|p| (p.player.feet, p.player.yaw, p.player.pitch));
    let tools = view
        .tools
        .get(&view.owner)
        .map(|t| (t.selected, t.slots.clone()));
    let bricks: Vec<_> = view.world.bricks.values().map(|b| b.position).collect();
    let chat: Vec<_> = pair
        .guest
        .ui
        .core
        .chat
        .lines
        .iter()
        .rev()
        .take(4)
        .map(|l| l.text.clone())
        .collect();
    bail!(
        "Hammer did not remove the brick ({before} bricks remain); guest pose {pose:?}, tools {tools:?}, bricks at {bricks:?}, ghost {:?}, chat {chat:?}",
        pair.guest
            .building()
            .and_then(|b| b.ghost())
            .map(|g| g.position)
    )
}

fn save_and_reload(pair: &mut Pair, name: &str, yaw: f32, steps: &mut Vec<String>) -> Result<()> {
    let count = bricks(&pair.host);
    request(
        &mut pair.host,
        UiAction::SaveBricks {
            name: format!("{name}.world.json"),
            description: "night qa".into(),
            events: true,
            ownership: true,
            overwrite: true,
        },
    )?;
    let file = format!("{name}.world.json");
    pair.until("save listed", Duration::from_secs(15), |h, _| {
        h.pending_requests() == 0 && h.ui.core.save_files.iter().any(|f| f.name == file)
    })?;
    let map = pair
        .host
        .ui
        .core
        .save_files
        .iter()
        .find(|f| f.name == file)
        .map(|f| f.map.clone())
        .context("save row")?;
    steps.push(format!("saved {count} bricks as {map}/{file}"));
    // Clear by hammering, then load back.
    hammer(pair, yaw)?;
    steps.push("hammered".into());
    request(
        &mut pair.host,
        UiAction::LoadBricks {
            map: map.clone(),
            name: file.clone(),
            ownership: true,
        },
    )?;
    let ours: Vec<[f32; 3]> = pair
        .host
        .network_view()
        .map(|v| {
            v.world
                .bricks
                .values()
                .filter(
                    |b| matches!(&b.definition, bri_world::ContentRef::Resolved(d) if d == BRICK),
                )
                .map(|b| b.position)
                .collect()
        })
        .unwrap_or_default();
    let _ = ours;
    pair.until(
        "loaded bricks replicated",
        Duration::from_secs(30),
        |h, g| h.pending_requests() == 0 && bricks(h) >= count && bricks(g) == bricks(h),
    )
    .with_context(|| {
        format!(
            "host {} guest {} want {count}",
            bricks(&pair.host),
            bricks(&pair.guest)
        )
    })?;
    pair.settle(Duration::from_secs(1))?;
    let (h, g) = (bricks(&pair.host), bricks(&pair.guest));
    ensure!(
        h == count && g == count,
        "reload left host {h} and guest {g} bricks; saved {count}"
    );
    steps.push(format!("reloaded {count} bricks"));
    Ok(())
}

/// Everything a visit checks once both players are in the game on `map`.
fn visit(
    pair: &mut Pair,
    report: &mut MapReport,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    out: &Path,
) -> Result<()> {
    pair.until("both players standing", Duration::from_secs(30), |h, g| {
        grounded(h) && grounded(g)
    })
    .or_else(|e| {
        report.steps.push(format!("not grounded: {e}"));
        Ok::<_, anyhow::Error>(())
    })?;
    pair.until("guest render ready", Duration::from_secs(30), |_, g| {
        g.world_render_ready()
    })?;
    let name = format!(
        "{}-{}",
        report.mode.replace([':', '/'], "_"),
        report
            .map
            .rsplit('/')
            .next()
            .unwrap_or("map")
            .trim_end_matches(".mis")
    );
    let frame = capture(&mut pair.guest, gpu, renderer, true)?;
    save_png(&out.join("maps").join(format!("{name}.png")), &frame)?;
    let yaw = plant(pair, 0.0)?;
    report.steps.push(format!("planted facing {yaw:.2}"));
    save_and_reload(pair, &name, yaw, &mut report.steps)?;
    // The guest can still build after the reload.
    hammer(pair, yaw)?;
    plant(pair, yaw)?;
    report
        .steps
        .push("hammered and replanted after reload".into());
    Ok(())
}

fn host_game(pair: &mut Pair, map: &str, mode: Option<&str>) -> Result<()> {
    wait_for_port()?;
    request(
        &mut pair.host,
        UiAction::HostGame {
            map: map.into(),
            mode: ServerMode::Lan,
            game_mode: mode.map(Into::into),
            max_players: 8,
            server_name: "Night QA".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    pair.until("host in game", Duration::from_secs(180), |h, _| in_game(h))?;
    request(
        &mut pair.guest,
        UiAction::JoinServer {
            address: "127.0.0.1".into(),
            password: String::new(),
        },
    )?;
    pair.until("guest in game", Duration::from_secs(180), |h, g| {
        in_game(g) && h.ui.core.players.len() == 2
    })?;
    Ok(())
}

fn leave(pair: &mut Pair) {
    let _ = request(&mut pair.guest, UiAction::Disconnect);
    let _ = request(&mut pair.host, UiAction::Disconnect);
    let _ = pair.settle(Duration::from_secs(2));
    for app in [&mut pair.host, &mut pair.guest] {
        app.ui.core.pop(ScreenId::MessageBox);
    }
}

fn load(content: &Path, out: &Path, name: &str) -> Result<Box<App>> {
    let state = out.join(format!("state-{name}"));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state)?;
    let mut app = App::load(content, &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.core.settings.avatar.lan_name = name.into();
    Ok(app)
}

pub fn setup(name: &str) -> Result<(PathBuf, PathBuf, Pair, Headless, UiRenderer)> {
    let content = PathBuf::from(std::env::var_os("BRI_CONTENT_ROOT").context("BRI_CONTENT_ROOT")?);
    let out = PathBuf::from(std::env::var_os("BRI_QA_OUT").context("BRI_QA_OUT")?).join(name);
    std::fs::create_dir_all(&out)?;
    let mut host = load(&content, &out, "QaHost")?;
    let mut guest = load(&content, &out, "QaGuest")?;
    let gpu = Headless::new().context("offscreen adapter")?;
    let renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    host.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    guest.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    Ok((
        content,
        out,
        Pair {
            host,
            guest,
            previous: Instant::now(),
        },
        gpu,
        renderer,
    ))
}

#[test]
#[ignore = "packaged content, loopback UDP 28000/28050 and an offscreen GPU; no window"]
fn every_map_every_mode_two_players_build_save_reload_change_map() -> Result<()> {
    if std::env::var_os("BRI_CONTENT_ROOT").is_none() || std::env::var_os("BRI_QA_OUT").is_none() {
        eprintln!("night QA needs BRI_CONTENT_ROOT and BRI_QA_OUT; skipped");
        return Ok(());
    }
    let (_content, out, mut pair, gpu, mut renderer) = setup("matrix")?;
    let maps: Vec<String> = bri_client::content::LOADABLE_MAPS
        .iter()
        .map(|m| m.to_string())
        .collect();
    let modes: Vec<(Option<String>, Option<String>)> = std::iter::once((None, None))
        .chain(
            pair.host
                .ui
                .core
                .game_modes
                .iter()
                .map(|m| (Some(m.id.clone()), m.map.clone())),
        )
        .collect();
    println!("modes: {modes:?}");
    let only = std::env::var("BRI_QA_MAPS").ok();
    let mut reports = Vec::new();
    bri_console::log::clear();
    for (mode, own_map) in &modes {
        let mode_name = mode.clone().unwrap_or_else(|| "Custom".into());
        let list: Vec<String> = match own_map {
            Some(m) => vec![m.clone()],
            None => maps
                .iter()
                .filter(|m| {
                    only.as_ref()
                        .is_none_or(|o| o.split(',').any(|x| m.contains(x)))
                })
                .cloned()
                .collect(),
        };
        let mut hosting = false;
        for map in &list {
            let start = Instant::now();
            let mut report = MapReport {
                mode: mode_name.clone(),
                map: map.clone(),
                ..Default::default()
            };
            if hosting && paused() {
                println!("paused: releasing the ports");
                leave(&mut pair);
                hosting = false;
            }
            let result = (|| -> Result<()> {
                if hosting {
                    report.reached_by = "Change Map".into();
                    // Opening Change Map asks the host for its list.
                    pair.host.ui.core.push(ScreenId::AdminMaps);
                    pair.host.ui.update(0);
                    pair.until("map list", Duration::from_secs(10), |h, _| {
                        h.ui.core.admin.maps.iter().any(|m| m.id == *map)
                    })?;
                    pair.host.ui.core.pop(ScreenId::AdminMaps);
                    request(
                        &mut pair.host,
                        UiAction::Admin(AdminAction::ChangeMap { map: map.clone() }),
                    )?;
                    pair.until("both on the new map", Duration::from_secs(180), |h, g| {
                        [h, g].iter().all(|a| {
                            a.network_view().is_some_and(|v| v.world.map_id == *map)
                                && a.scene_map() == Some(map.as_str())
                                && in_game(a)
                                && a.ui.core.players.len() == 2
                        })
                    })?;
                } else {
                    report.reached_by = "Host".into();
                    host_game(&mut pair, map, mode.as_deref())?;
                    hosting = true;
                }
                report.steps.push("both in game".into());
                visit(&mut pair, &mut report, &gpu, &mut renderer, &out)
            })();
            if let Err(e) = result {
                report.error = Some(format!("{e:#}"));
                let frame = capture(&mut pair.guest, &gpu, &mut renderer, true);
                if let Ok(frame) = frame {
                    let _ = save_png(
                        &out.join("failures").join(format!(
                            "{}-{}.png",
                            reports.len(),
                            mode_name.replace([':', '/'], "_")
                        )),
                        &frame,
                    );
                }
                leave(&mut pair);
                hosting = false;
            }
            report.seconds = start.elapsed().as_secs_f32();
            console_since(&mut report);
            println!(
                "[{}] {} {} via {}: {} ({:.0} s, {} warnings, {} errors)",
                reports.len(),
                report.mode,
                report.map,
                report.reached_by,
                report.error.as_deref().unwrap_or("ok"),
                report.seconds,
                report.warnings.len(),
                report.errors.len()
            );
            reports.push(report);
            std::fs::write(
                out.join("report.json"),
                serde_json::to_vec_pretty(&reports)?,
            )?;
        }
        leave(&mut pair);
    }
    let failed = reports.iter().filter(|r| r.error.is_some()).count();
    println!(
        "{failed} of {} visits failed; report {}",
        reports.len(),
        out.join("report.json").display()
    );
    Ok(())
}

/// Visible texts and commands of the top screen, for reviewing strings.
fn describe(app: &App) -> String {
    let mut out = format!("stack {:?}\n", app.ui.stack());
    let top = app.ui.top_id();
    if let Some(screen) = app.ui.screen(top) {
        let v = screen.view();
        for n in v.walk() {
            if !v.is_shown(n) {
                continue;
            }
            let node = v.node(n);
            let text = v.text_of(n);
            let name = node.ctrl.name.clone().unwrap_or_default();
            let command = node.ctrl.command.clone().unwrap_or_default();
            if text.trim().is_empty() && command.is_empty() && name.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "{} [{name}] {:?} cmd={command:?}\n",
                node.ctrl.class,
                text.trim()
            ));
            for (item, _) in node.state.items.iter().take(40) {
                out.push_str(&format!("    item {item:?}\n"));
            }
        }
    }
    out
}

struct Shots {
    dir: PathBuf,
    n: usize,
}

impl Shots {
    fn take(
        &mut self,
        app: &mut App,
        gpu: &Headless,
        renderer: &mut UiRenderer,
        name: &str,
    ) -> Result<()> {
        self.n += 1;
        let file = format!("{:02}-{name}", self.n);
        let scene = app.network_view().is_some() && app.world_render_ready();
        let frame = capture(app, gpu, renderer, scene)?;
        save_png(&self.dir.join(format!("{file}.png")), &frame)?;
        std::fs::write(self.dir.join(format!("{file}.txt")), describe(app))?;
        println!("shot {file}: {:?}", app.ui.stack());
        Ok(())
    }
}

fn click(app: &mut App, name: &str) -> Result<()> {
    let top = app.ui.top_id();
    let (x, y) = app
        .ui
        .control_center(top, name)
        .with_context(|| format!("No control {name:?} on {top:?}"))?;
    use bri_ui::input::{InputEvent, MouseButton};
    app.ui.handle_input(InputEvent::MouseMove { x, y });
    app.ui.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x,
        y,
    });
    app.ui.handle_input(InputEvent::MouseUp {
        button: MouseButton::Left,
        x,
        y,
    });
    let commands = app.pump()?;
    ensure!(
        commands.is_empty(),
        "Unexpected window command {commands:?}"
    );
    app.ui.update(0);
    Ok(())
}

fn run_for(app: &mut App, ms: u64) -> Result<()> {
    for _ in 0..(ms / 16).max(1) {
        step(app, Duration::from_millis(16))?;
    }
    Ok(())
}

/// Like `run_for`, but a failed connection is a state to look at, not an error.
fn run_loose(app: &mut App, ms: u64) -> Result<()> {
    for _ in 0..(ms / 16).max(1) {
        app.tick(Duration::from_millis(16))?;
        app.ui.update(16);
        app.pump()?;
        thread::sleep(Duration::from_millis(4));
    }
    Ok(())
}

#[test]
#[ignore = "packaged content, loopback UDP 28000/28050 and an offscreen GPU; no window"]
fn new_player_screens() -> Result<()> {
    if std::env::var_os("BRI_CONTENT_ROOT").is_none() || std::env::var_os("BRI_QA_OUT").is_none() {
        eprintln!("night QA needs BRI_CONTENT_ROOT and BRI_QA_OUT; skipped");
        return Ok(());
    }
    let content = PathBuf::from(std::env::var_os("BRI_CONTENT_ROOT").context("BRI_CONTENT_ROOT")?);
    let out =
        PathBuf::from(std::env::var_os("BRI_QA_OUT").context("BRI_QA_OUT")?).join("new-player");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;
    let gpu = Headless::new().context("offscreen adapter")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    let mut shots = Shots {
        dir: out.join("shots"),
        n: 0,
    };
    // A first run: an empty state folder.
    let mut app = App::load(&content, &out.join("state-new"), SIZE)?;
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    run_for(&mut app, 100)?;
    shots.take(&mut app, &gpu, &mut renderer, "first-run")?;
    app.ui.core.pop(ScreenId::DefaultControls);
    run_for(&mut app, 100)?;
    shots.take(&mut app, &gpu, &mut renderer, "main-menu")?;
    for (screen, name) in [
        (ScreenId::Options, "options"),
        (ScreenId::StartMission, "start-game"),
        (ScreenId::GameModes, "game-modes"),
        (ScreenId::AddOns, "add-ons"),
        (ScreenId::JoinServer, "join-server"),
        (ScreenId::ManualJoin, "connect-to-ip"),
        (ScreenId::Avatar, "avatar"),
        (ScreenId::About, "about"),
        (ScreenId::Console, "console"),
    ] {
        app.ui.core.push(screen);
        run_for(&mut app, 200)?;
        shots.take(&mut app, &gpu, &mut renderer, name)?;
        if screen == ScreenId::Options {
            let panes: Vec<String> = {
                let v = app.ui.screen(ScreenId::Options).unwrap().view();
                v.walk()
                    .filter(|&n| v.is_shown(n))
                    .filter_map(|n| v.node(n).ctrl.command.clone())
                    .filter(|c| c.starts_with("optionsDlg.setPane("))
                    .collect()
            };
            for pane in panes {
                if click(&mut app, &pane).is_ok() {
                    run_for(&mut app, 100)?;
                    let label = pane
                        .trim_start_matches("optionsDlg.setPane(")
                        .trim_end_matches(");")
                        .to_lowercase();
                    shots.take(&mut app, &gpu, &mut renderer, &format!("options-{label}"))?;
                }
            }
        }
        app.ui.core.pop(screen);
        run_for(&mut app, 50)?;
    }
    // Joining an address nobody answers.
    request(
        &mut app,
        UiAction::JoinServer {
            address: "127.0.0.1:28999".into(),
            password: String::new(),
        },
    )?;
    run_loose(&mut app, 300)?;
    shots.take(&mut app, &gpu, &mut renderer, "connecting")?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(40)
        && !matches!(app.ui.core.conn, ConnectionState::Failed { .. })
    {
        run_loose(&mut app, 100)?;
    }
    run_loose(&mut app, 200)?;
    shots.take(&mut app, &gpu, &mut renderer, "no-answer")?;
    app.ui.core.pop(ScreenId::MessageBox);
    let _ = request(&mut app, UiAction::CancelConnect);
    run_loose(&mut app, 100)?;

    // Host a LAN game, then a guest joins by address and the host leaves.
    wait_for_port()?;
    request(
        &mut app,
        UiAction::HostGame {
            map: "v20/add-ons/map_slate/slate.mis".into(),
            mode: ServerMode::Lan,
            game_mode: None,
            max_players: 8,
            server_name: "New player".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    run_for(&mut app, 200)?;
    shots.take(&mut app, &gpu, &mut renderer, "host-loading")?;
    let start = Instant::now();
    while !(in_game(&app) && app.world_render_ready()) {
        ensure!(
            start.elapsed() < Duration::from_secs(120),
            "host never in game"
        );
        run_for(&mut app, 50)?;
    }
    run_for(&mut app, 1500)?;
    shots.take(&mut app, &gpu, &mut renderer, "in-game")?;
    for (screen, name) in [
        (ScreenId::EscapeMenu, "escape-menu"),
        (ScreenId::PlayerList, "player-list"),
        (ScreenId::BrickSelector, "brick-selector"),
        (ScreenId::MiniGames, "minigames"),
        (ScreenId::SaveBricks, "save-bricks"),
        (ScreenId::LoadBricks, "load-bricks"),
        (ScreenId::Admin, "admin"),
    ] {
        app.ui.core.push(screen);
        run_for(&mut app, 300)?;
        shots.take(&mut app, &gpu, &mut renderer, name)?;
        app.ui.core.pop(screen);
        run_for(&mut app, 50)?;
    }
    let mut guest = App::load(&content, &out.join("state-guest"), SIZE)?;
    guest.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    guest.ui.core.pop(ScreenId::DefaultControls);
    guest.ui.core.push(ScreenId::JoinServer);
    guest.ui.core.push(ScreenId::ManualJoin);
    run_for(&mut guest, 160)?;
    shots.take(&mut guest, &gpu, &mut renderer, "guest-connect-to-ip")?;
    request(
        &mut guest,
        UiAction::JoinServer {
            address: "127.0.0.1".into(),
            password: String::new(),
        },
    )?;
    let start = Instant::now();
    let mut loading_shot = false;
    while !(in_game(&guest) && guest.world_render_ready()) {
        ensure!(
            start.elapsed() < Duration::from_secs(120),
            "guest never in game"
        );
        step(&mut app, Duration::from_millis(16))?;
        step(&mut guest, Duration::from_millis(16))?;
        if !loading_shot && matches!(guest.ui.core.conn, ConnectionState::Loading { .. }) {
            shots.take(&mut guest, &gpu, &mut renderer, "guest-loading")?;
            loading_shot = true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..90 {
        step(&mut app, Duration::from_millis(16))?;
        step(&mut guest, Duration::from_millis(16))?;
    }
    shots.take(&mut guest, &gpu, &mut renderer, "guest-in-game")?;
    shots.take(&mut app, &gpu, &mut renderer, "host-sees-guest")?;
    // The host quits: what does the guest read?
    request(&mut app, UiAction::Disconnect)?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) && in_game(&guest) {
        run_loose(&mut app, 16)?;
        run_loose(&mut guest, 16)?;
    }
    run_loose(&mut guest, 300)?;
    shots.take(&mut guest, &gpu, &mut renderer, "guest-host-left")?;
    run_loose(&mut app, 100)?;
    shots.take(&mut app, &gpu, &mut renderer, "host-after-leave")?;
    std::fs::write(
        out.join("console.txt"),
        bri_console::log::lines()
            .iter()
            .map(|l| format!("{:?} {}\n", l.level, l.text))
            .collect::<String>(),
    )?;
    Ok(())
}

/// Regression: once the server shows the grey brick in hand, a click must
/// still place the ghost (it went to the brick image's trigger instead).
#[test]
#[ignore = "packaged or generated content, loopback UDP and an offscreen GPU; no window"]
fn click_places_the_ghost_after_the_brick_is_in_hand() -> Result<()> {
    let content = std::env::var_os("BRI_CONTENT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    let state = std::env::temp_dir().join(format!("bri-brick-hand-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let mut app = App::load(&content, &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    request(
        &mut app,
        UiAction::HostGame {
            map: "v20/add-ons/map_slate/slate.mis".into(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 1,
            server_name: "Brick hand".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    let start = Instant::now();
    while !(in_game(&app) && grounded(&app)) {
        ensure!(start.elapsed() < Duration::from_secs(120), "never in game");
        run_for(&mut app, 50)?;
    }
    aim(&mut app, 0.0, DOWN)?;
    request(
        &mut app,
        UiAction::InstantUseBrick {
            brick: BRICK.into(),
        },
    )?;
    let holds = |a: &App| {
        a.network_view().is_some_and(|v| {
            v.weapons.images.get(&v.owner).is_some_and(|i| {
                i.iter()
                    .any(|i| i.hand == 0 && i.image == "v20.image.brickimage")
            })
        })
    };
    let start = Instant::now();
    while !holds(&app) {
        ensure!(
            start.elapsed() < Duration::from_secs(10),
            "brick never in hand"
        );
        run_for(&mut app, 16)?;
    }
    run_for(&mut app, 100)?;
    request(
        &mut app,
        UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down: true,
        }),
    )?;
    let ghost = app.building().and_then(|b| b.ghost()).is_some();
    let _ = request(&mut app, UiAction::Disconnect);
    let _ = std::fs::remove_dir_all(&state);
    ensure!(ghost, "A click with the brick in hand placed no ghost");
    Ok(())
}

/// Placing the ghost fires `brickImage` as in v20: `brickTrailEmitter`
/// streams from the brick in hand and `brickDeployExplosion` puffs where the
/// ghost lands, in first and third person alike. Prints what it saw.
#[test]
#[ignore = "packaged or generated content, loopback UDP and an offscreen GPU; no window"]
fn placing_the_ghost_shows_the_brick_trail_and_puff() -> Result<()> {
    let content = std::env::var_os("BRI_CONTENT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    let state = std::env::temp_dir().join(format!("bri-brick-puff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let mut app = App::load(&content, &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    request(
        &mut app,
        UiAction::HostGame {
            map: "v20/add-ons/map_slate/slate.mis".into(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 1,
            server_name: "Brick puff".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    let start = Instant::now();
    while !(in_game(&app) && grounded(&app)) {
        ensure!(start.elapsed() < Duration::from_secs(120), "never in game");
        run_for(&mut app, 50)?;
    }
    aim(&mut app, 0.0, DOWN)?;
    request(
        &mut app,
        UiAction::InstantUseBrick {
            brick: BRICK.into(),
        },
    )?;
    let holds = |a: &App| {
        a.network_view().is_some_and(|v| {
            v.weapons.images.get(&v.owner).is_some_and(|i| {
                i.iter()
                    .any(|i| i.hand == 0 && i.image == "v20.image.brickimage")
            })
        })
    };
    let start = Instant::now();
    while !holds(&app) {
        ensure!(
            start.elapsed() < Duration::from_secs(10),
            "brick never in hand"
        );
        run_for(&mut app, 16)?;
    }
    // The host steps on the wall clock: give it real time, frame by frame.
    let live = |app: &mut App, ms: u64| -> Result<()> {
        for _ in 0..(ms / 16).max(1) {
            step(app, Duration::from_millis(16))?;
            thread::sleep(Duration::from_millis(16));
        }
        Ok(())
    };
    let mut failures = Vec::new();
    for third in [false, true] {
        if app.controls.third_person_view() != third {
            request(
                &mut app,
                UiAction::Game(GameAction::ToggleFirstPerson { fast: true }),
            )?;
        }
        ensure!(
            app.controls.third_person_view() == third,
            "view never switched"
        );
        let view = if third {
            "third person"
        } else {
            "first person"
        };
        live(&mut app, 600)?;
        let before = app.weapon_effect_diagnostics().clone();
        request(
            &mut app,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: true,
            }),
        )?;
        live(&mut app, 16)?;
        request(
            &mut app,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: false,
            }),
        )?;
        let mut most = (0, 0);
        let mut last = String::new();
        for frame in 0..60 {
            live(&mut app, 16)?;
            let (sources, particles) = app.weapon_effect_counts();
            most = (most.0.max(sources), most.1.max(particles));
            // What the server shows and what reached the effects, as it changes.
            let trace = app.network_view().map_or_else(String::new, |v| {
                let states: Vec<_> = v.weapons.images.get(&v.owner).into_iter().flatten()
                    .map(|i| format!("{}:{}", i.image, i.state)).collect();
                let shots: Vec<_> = v.weapons.projectiles.iter().map(|p| p.definition.clone()).collect();
                format!(
                    "images {states:?} projectiles {shots:?} backlog {:?} accepted {} sources {sources} particles {particles} ghost {:?} held brick {:?} pending {} print {:?} screens {:?}",
                    app.weapon_effect_backlog(),
                    app.weapon_effect_diagnostics().accepted_cues,
                    app.building().and_then(|b| b.ghost()).map(|g| g.position),
                    app.building().map(|b| b.held_brick()),
                    app.pending_requests(),
                    app.ui.core.bottom_print.as_ref().map(|p| &p.0),
                    app.ui.stack(),
                )
            });
            if trace != last {
                println!("  {view} frame {frame}: {trace}");
                last = trace;
            }
        }
        let after = app.weapon_effect_diagnostics();
        let accepted = after.accepted_cues - before.accepted_cues;
        let missing: Vec<_> = after.messages.difference(&before.messages).collect();
        println!(
            "{view}: accepted cues {accepted}, most sources {}, most particles {}, \
             missing bindings +{}, missing poses +{}, capacity +{}, new messages {missing:?}",
            most.0,
            most.1,
            after.missing_bindings - before.missing_bindings,
            after.missing_poses - before.missing_poses,
            after.capacity_rejections - before.capacity_rejections,
        );
        if accepted < 2
            || most.1 == 0
            || after.missing_poses != before.missing_poses
            || after.missing_bindings != before.missing_bindings
        {
            failures.push(view);
        }
    }
    let _ = request(&mut app, UiAction::Disconnect);
    let _ = std::fs::remove_dir_all(&state);
    ensure!(
        failures.is_empty(),
        "Brick deploy effects missing in {failures:?}"
    );
    Ok(())
}

/// Import a v20 weapon through the Add-Ons screen's import path and play a
/// brick pack imported with `bri-import-addon`: turn both on, host, plant an
/// imported brick, and fire the imported weapon from a mini-game loadout.
/// BRI_IMPORT_ROOT is a scratch content root with `Add-Ons/Weapon_Shotgun.zip`
/// dropped in and `addons/brick_fence` imported; BRI_IMPORTER the shipped
/// `bri-import-addon.exe`.
#[test]
#[ignore = "scratch content with v20 add-ons, loopback UDP and an offscreen GPU; no window"]
fn imported_v20_add_ons_play() -> Result<()> {
    if !qa_env(&["BRI_IMPORT_ROOT", "BRI_IMPORTER", "BRI_QA_OUT"]) {
        return Ok(());
    }
    let root = PathBuf::from(std::env::var_os("BRI_IMPORT_ROOT").context("BRI_IMPORT_ROOT")?);
    let importer = PathBuf::from(std::env::var_os("BRI_IMPORTER").context("BRI_IMPORTER")?);
    let out = PathBuf::from(std::env::var_os("BRI_QA_OUT").context("BRI_QA_OUT")?).join("import");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;
    let before = bri_client::add_ons::view(&root);
    let rows = |v: &AddOnsView| {
        v.rows
            .iter()
            .map(|r| {
                format!(
                    "{} | {} | {} | enabled {}",
                    r.id, r.name, r.category, r.enabled
                )
            })
            .collect::<Vec<_>>()
    };
    println!("before: {:#?}\nnotice {:?}", rows(&before), before.notice);
    ensure!(
        before.rows.iter().any(|r| r.id == "legacy:Weapon_Shotgun"),
        "The dropped zip is not listed as converting"
    );
    // The Add-Ons folder's worker, which opening Add-Ons starts.
    let notes = bri_client::add_ons::start_sync(&root, &importer)?;
    let notice = loop {
        let note = notes.recv_timeout(Duration::from_secs(300))?;
        println!("sync: {note:?}");
        if note.finished {
            break note.notice;
        }
    };
    let after = bri_client::add_ons::view(&root);
    println!("after: {:#?}\nnotice {:?}", rows(&after), after.notice);
    let weapon = after
        .rows
        .iter()
        .find(|r| r.name.to_lowercase().contains("shotgun") && !r.id.starts_with("legacy:"))
        .with_context(|| format!("No imported shotgun row after {notice:?}"))?
        .id
        .clone();
    for id in [weapon.as_str(), "brick_fence"] {
        let view = bri_client::add_ons::set_enabled(&root, id, true)?;
        println!("enable {id}: {:?}", view.notice);
    }
    std::fs::write(
        out.join("rows.txt"),
        rows(&bri_client::add_ons::view(&root)).join("\n"),
    )?;

    let mut app = App::load(&root, &out.join("state"), SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    let gpu = Headless::new().context("offscreen adapter")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    request(
        &mut app,
        UiAction::HostGame {
            map: "v20/add-ons/map_slate/slate.mis".into(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 1,
            server_name: "Imports".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    let start = Instant::now();
    while !(in_game(&app) && grounded(&app) && app.world_render_ready()) {
        ensure!(start.elapsed() < Duration::from_secs(180), "never in game");
        run_for(&mut app, 50)?;
    }
    let fence = app
        .ui
        .core
        .bricks
        .iter()
        .find(|b| b.id.starts_with("brick_fence:"))
        .map(|b| b.id.clone())
        .context("No imported fence brick in the brick selector")?;
    // Plant an imported brick.
    let mut ghost = false;
    for quarter in 0..4 {
        aim(&mut app, quarter as f32 * std::f32::consts::FRAC_PI_2, DOWN)?;
        run_for(&mut app, 150)?;
        request(
            &mut app,
            UiAction::InstantUseBrick {
                brick: fence.clone(),
            },
        )?;
        for down in [true, false] {
            request(
                &mut app,
                UiAction::Game(GameAction::Held {
                    control: HeldControl::Fire,
                    down,
                }),
            )?;
            run_for(&mut app, 100)?;
        }
        ghost = app.building().and_then(|b| b.ghost()).is_some();
        if ghost {
            break;
        }
    }
    if !ghost {
        request(
            &mut app,
            UiAction::InstantUseBrick {
                brick: BRICK.into(),
            },
        )?;
        run_for(&mut app, 100)?;
        println!(
            "stock brick equips as {:?}",
            app.building().map(|b| b.equipment().clone())
        );
        request(
            &mut app,
            UiAction::InstantUseBrick {
                brick: fence.clone(),
            },
        )?;
        println!(
            "fence right after request {:?}",
            app.building().map(|b| b.equipment().clone())
        );
        run_for(&mut app, 100)?;
        println!(
            "fence after 100 ms {:?}",
            app.building().map(|b| b.equipment().clone())
        );
    }
    ensure!(
        ghost,
        "No ghost for {fence}; definition {:?}, equipment {:?}, stack {:?}, console {:?}",
        app.building().map(|b| b.definition_half_extents(&fence)),
        app.building().map(|b| b.equipment().clone()),
        app.ui.stack(),
        bri_console::log::lines()
            .iter()
            .rev()
            .take(6)
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
    );
    request(&mut app, UiAction::Game(GameAction::PlantBrick))?;
    let start = Instant::now();
    while bricks(&app) == 0 {
        ensure!(
            start.elapsed() < Duration::from_secs(10),
            "{fence} was not planted"
        );
        run_for(&mut app, 50)?;
    }
    request(&mut app, UiAction::Game(GameAction::CancelBrick))?;
    let planted = app
        .network_view()
        .unwrap()
        .world
        .bricks
        .values()
        .next()
        .unwrap()
        .definition
        .clone();
    println!("planted {planted:?}");
    // Fire the imported weapon from a mini-game loadout.
    let item = app
        .ui
        .core
        .datablocks
        .get("ItemData")
        .into_iter()
        .flatten()
        .map(|c| c.id.clone())
        .find(|id| id.to_lowercase().contains("shotgun"))
        .with_context(|| "No shotgun among the mini-game items".to_string())?;
    let mut rules = MiniGameRules {
        title: "Imports".into(),
        ..Default::default()
    };
    rules.loadout[3] = Some(item.clone());
    request(&mut app, UiAction::CreateMiniGame { color: 1, rules })?;
    let start = Instant::now();
    loop {
        run_for(&mut app, 50)?;
        let has = app.network_view().is_some_and(|v| {
            v.tools
                .get(&v.owner)
                .is_some_and(|t| t.slots[3].as_deref() == Some(item.as_str()))
        });
        if has {
            break;
        }
        ensure!(
            start.elapsed() < Duration::from_secs(15),
            "{item} never reached slot 4"
        );
    }
    request(&mut app, UiAction::UseTool { slot: 3 })?;
    run_for(&mut app, 800)?;
    aim(&mut app, 0.0, 0.1)?;
    let mut fired = 0;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(6) {
        request(
            &mut app,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: true,
            }),
        )?;
        run_for(&mut app, 100)?;
        fired = fired.max(
            app.network_view()
                .map_or(0, |v| v.weapons.projectiles.len()),
        );
        request(
            &mut app,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: false,
            }),
        )?;
        run_for(&mut app, 300)?;
    }
    let images: Vec<String> = app
        .network_view()
        .and_then(|v| {
            v.weapons
                .images
                .get(&v.owner)
                .map(|i| i.iter().map(|i| i.image.clone()).collect())
        })
        .unwrap_or_default();
    let frame = capture(&mut app, &gpu, &mut renderer, true)?;
    save_png(&out.join("imported-shotgun.png"), &frame)?;
    std::fs::write(
        out.join("console.txt"),
        bri_console::log::lines()
            .iter()
            .map(|l| format!("{:?} {}\n", l.level, l.text))
            .collect::<String>(),
    )?;
    println!("holding {images:?}; most projectiles in flight {fired}");
    ensure!(in_game(&app), "left the game while firing");
    ensure!(fired > 0, "{item} fired no projectiles; holding {images:?}");
    let _ = request(&mut app, UiAction::Disconnect);
    Ok(())
}

#[derive(Serialize, Default, Clone)]
struct SoakMinute {
    minute: u32,
    bricks: usize,
    players: Vec<usize>,
    connected: Vec<bool>,
    /// Wall milliseconds for one `step` of each app: average and worst.
    step_ms_avg: Vec<f64>,
    step_ms_max: Vec<f64>,
    /// Authoritative ticks the host view advanced this minute (120 Hz = 7200).
    server_ticks: u64,
    vehicles: usize,
    projectiles_seen: usize,
    planted: u32,
    hammered: u32,
    shots: u32,
    chats: u32,
    driver_mounted_s: f32,
    warnings: usize,
    errors: usize,
    events: Vec<String>,
}

struct Soaker {
    apps: Vec<App>,
    names: Vec<String>,
    previous: Instant,
    step_ms: Vec<Vec<f64>>,
}

impl Soaker {
    fn tick(&mut self) -> Vec<String> {
        let now = Instant::now();
        let elapsed = now
            .duration_since(self.previous)
            .min(Duration::from_millis(100));
        self.previous = now;
        let mut problems = Vec::new();
        for (i, app) in self.apps.iter_mut().enumerate() {
            let t = Instant::now();
            let result = (|| -> Result<()> {
                app.tick(elapsed)?;
                app.ui.update(elapsed.as_millis() as u64);
                app.pump()?;
                Ok(())
            })();
            self.step_ms[i].push(t.elapsed().as_secs_f64() * 1000.0);
            if let Err(e) = result {
                problems.push(format!("{} step: {e:#}", self.names[i]));
            }
        }
        problems
    }
    fn run(&mut self, time: Duration, events: &mut Vec<String>) {
        let start = Instant::now();
        while start.elapsed() < time {
            events.extend(self.tick());
            thread::sleep(Duration::from_millis(8));
        }
    }
}

fn held(app: &mut App, control: HeldControl, down: bool) -> Result<()> {
    request(app, UiAction::Game(GameAction::Held { control, down }))
}

/// A one-brick save with a jeep spawn five units ahead of `app`'s player.
fn jeep_save(host_state: &Path, app: &App) -> Result<(String, String)> {
    let player = app.presented_local().context("driver")?.clone();
    let feet = glam::Vec3::from(player.feet);
    let forward = glam::Vec3::new(player.yaw.sin(), 0.0, -player.yaw.cos());
    let spot = feet + forward * 6.0;
    let view = app.network_view().context("view")?;
    let map_id = view.world.map_id.clone();
    let mut world =
        bri_world::World::new("Jeep".into(), map_id.clone(), view.world.palette.clone());
    let mut brick = bri_world::Brick::new(
        bri_world::ContentRef::Resolved("v20/brick/brickvehiclespawndata".into()),
        [
            (spot.x * 2.0).round() / 2.0,
            (feet.y / 0.2).ceil() * 0.2 + 0.1,
            (spot.z * 2.0).round() / 2.0,
        ],
        view.owner,
    );
    brick.vehicle = Some(Box::new(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved("v20.vehicle.jeepvehicle".into()),
        recolor: false,
        team: None,
    }));
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let build = bri_world::build::SavedBuild::new(world);
    use sha2::Digest;
    let folder = host_state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("soak-jeep.world.json"),
        serde_json::to_vec(&build)?,
    )?;
    Ok(("Slate".into(), "soak-jeep.world.json".into()))
}

fn mounted(app: &App) -> bool {
    app.network_view()
        .and_then(|v| v.vitals.get(&v.owner))
        .is_some_and(|v| v.mounted.is_some())
}

/// Host plus three headless guests on Slate for BRI_SOAK_SECONDS (default
/// an hour): a builder, a gunner in a mini-game, a jeep driver, chat from
/// everyone, and a save, clear and reload every BRI_SOAK_SAVE_SECONDS
/// (default 15 minutes). Writes soak.json every minute.
#[test]
#[ignore = "packaged content, loopback UDP 28000/28050; an hour long; no window"]
fn soak_four_players_build_drive_fire_chat_and_save() -> Result<()> {
    if !qa_env(&["BRI_CONTENT_ROOT", "BRI_QA_OUT"]) {
        return Ok(());
    }
    let content = PathBuf::from(std::env::var_os("BRI_CONTENT_ROOT").context("BRI_CONTENT_ROOT")?);
    let out = PathBuf::from(std::env::var_os("BRI_QA_OUT").context("BRI_QA_OUT")?).join("soak");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;
    let seconds: u64 = std::env::var("BRI_SOAK_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600);
    let save_every: u64 = std::env::var("BRI_SOAK_SAVE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(900);
    let names: Vec<String> = ["SoakHost", "Builder", "Gunner", "Driver"]
        .map(String::from)
        .to_vec();
    let mut apps = Vec::new();
    for name in &names {
        apps.push(*load(&content, &out, name)?);
    }
    std::fs::write(out.join("pid.txt"), std::process::id().to_string())?;
    let host_state = out.join("state-SoakHost");
    let mut s = Soaker {
        apps,
        names: names.clone(),
        previous: Instant::now(),
        step_ms: vec![Vec::new(); 4],
    };
    let mut events = Vec::new();
    wait_for_port()?;
    request(
        &mut s.apps[0],
        UiAction::HostGame {
            map: "v20/add-ons/map_slate/slate.mis".into(),
            mode: ServerMode::Lan,
            game_mode: None,
            max_players: 8,
            server_name: "Night soak".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    let wait =
        |s: &mut Soaker, what: &str, secs: u64, ready: &dyn Fn(&[App]) -> bool| -> Result<()> {
            let start = Instant::now();
            let mut ev = Vec::new();
            while !ready(&s.apps) {
                ensure!(
                    start.elapsed() < Duration::from_secs(secs),
                    "Timed out waiting for {what}: {ev:?}"
                );
                ev.extend(s.tick());
                thread::sleep(Duration::from_millis(8));
            }
            Ok(())
        };
    wait(&mut s, "host", 180, &|a| in_game(&a[0]))?;
    for i in 1..4 {
        request(
            &mut s.apps[i],
            UiAction::JoinServer {
                address: "127.0.0.1".into(),
                password: String::new(),
            },
        )?;
    }
    wait(&mut s, "guests", 300, &|a| {
        a.iter().all(in_game) && a[0].ui.core.players.len() == 4
    })?;
    wait(&mut s, "standing", 60, &|a| a.iter().all(grounded))?;
    // Gunner: a mini-game with the gun; the host owns it, the gunner joins.
    let mut rules = MiniGameRules {
        title: "Soak DM".into(),
        respawn_seconds: 2,
        ..Default::default()
    };
    rules.use_all_players_bricks = true;
    request(&mut s.apps[0], UiAction::CreateMiniGame { color: 2, rules })?;
    wait(&mut s, "mini-game listed", 20, &|a| {
        !a[2].ui.core.minigames.games.is_empty()
    })?;
    let game = s.apps[2].ui.core.minigames.games[0].id;
    request(&mut s.apps[2], UiAction::JoinMiniGame { game })?;
    s.run(Duration::from_secs(2), &mut events);
    // Driver: load a jeep spawn in front of them.
    let (map, file) = jeep_save(&host_state, &s.apps[3])?;
    request(
        &mut s.apps[0],
        UiAction::LoadBricks {
            map,
            name: file,
            ownership: true,
        },
    )?;
    wait(&mut s, "jeep", 30, &|a| {
        a[3].network_view().is_some_and(|v| !v.vehicles.is_empty())
    })?;
    let driver_yaw = s.apps[3].controls.yaw;

    let start = Instant::now();
    let mut minute = SoakMinute::default();
    let mut minutes: Vec<SoakMinute> = Vec::new();
    let mut last_tick = s.apps[0].network_view().map_or(0, |v| v.tick);
    let mut next_minute = Duration::from_secs(60);
    let mut next_save = Duration::from_secs(save_every);
    let mut builder_yaw = 0.0f32;
    let mut step = 0u64;
    bri_console::log::clear();
    while start.elapsed() < Duration::from_secs(seconds) {
        step += 1;
        let problems = s.tick();
        minute.events.extend(problems);
        let t = start.elapsed();
        // Builder: a brick every ~2 s, a hammer every 5th, walking a circle.
        if step.is_multiple_of(240) {
            let b = &mut s.apps[1];
            builder_yaw += 0.7;
            let result = (|| -> Result<bool> {
                aim(b, builder_yaw, DOWN)?;
                request(
                    b,
                    UiAction::InstantUseBrick {
                        brick: BRICK.into(),
                    },
                )?;
                held(b, HeldControl::Fire, true)?;
                held(b, HeldControl::Fire, false)?;
                let ghost = b.building().and_then(|g| g.ghost()).is_some();
                if ghost {
                    request(b, UiAction::Game(GameAction::PlantBrick))?;
                    request(b, UiAction::Game(GameAction::CancelBrick))?;
                }
                Ok(ghost)
            })();
            match result {
                Ok(true) => minute.planted += 1,
                Ok(false) => {}
                Err(e) => minute.events.push(format!("builder: {e:#}")),
            }
        }
        if step % 1200 == 600 {
            let b = &mut s.apps[1];
            let _ = request(b, UiAction::UseTool { slot: 0 })
                .and_then(|_| held(b, HeldControl::Fire, true));
            minute.hammered += 1;
        }
        if step % 1200 == 660 {
            let _ = held(&mut s.apps[1], HeldControl::Fire, false);
            let _ = request(&mut s.apps[1], UiAction::UnUseTool);
            // Walk a little so the next bricks land elsewhere.
            let _ = held(&mut s.apps[1], HeldControl::Forward, true);
        }
        if step % 1200 == 720 {
            let _ = held(&mut s.apps[1], HeldControl::Forward, false);
        }
        // Gunner: equip the gun (slot 3) and fire about once a second.
        if step.is_multiple_of(120) {
            let g = &mut s.apps[2];
            let yaw = (step as f32 * 0.013).sin() * 3.0;
            let r = aim(g, yaw, 0.05)
                .and_then(|_| request(g, UiAction::UseTool { slot: 3 }))
                .and_then(|_| held(g, HeldControl::Fire, true));
            if let Err(e) = r {
                minute.events.push(format!("gunner: {e:#}"));
            }
            minute.shots += 1;
        }
        if step % 120 == 30 {
            let _ = held(&mut s.apps[2], HeldControl::Fire, false);
        }
        // Driver: board the jeep and drive in weaving circles.
        if step.is_multiple_of(60) {
            minute.projectiles_seen = minute.projectiles_seen.max(
                s.apps[0]
                    .network_view()
                    .map_or(0, |v| v.weapons.projectiles.len()),
            );
            let d = &mut s.apps[3];
            if mounted(d) {
                minute.driver_mounted_s += 0.5;
                let left = (step / 360).is_multiple_of(2);
                let _ = held(d, HeldControl::Forward, true);
                let _ = held(d, HeldControl::Left, left);
                let _ = held(d, HeldControl::Right, !left);
            } else {
                let _ = held(d, HeldControl::Left, false);
                let _ = held(d, HeldControl::Right, false);
                // Head for the nearest vehicle and jump at it.
                let target = d.network_view().and_then(|v| {
                    let me = glam::Vec3::from(v.poses.get(&v.owner)?.player.feet);
                    v.vehicle_poses
                        .values()
                        .map(|x| glam::Vec3::from(x.position))
                        .min_by(|a, b| a.distance(me).total_cmp(&b.distance(me)))
                        .map(|p| (p - me, me))
                });
                if let Some((to, _)) = target {
                    let yaw = to.x.atan2(-to.z);
                    let _ = aim(d, yaw, 0.0);
                } else {
                    let _ = aim(d, driver_yaw, 0.0);
                }
                let _ = held(d, HeldControl::Forward, true);
                // Walk into it, like a player; jump only if stuck on top.
                let on_top = d.network_view().is_some_and(|v| {
                    v.poses.get(&v.owner).is_some_and(|p| {
                        v.vehicle_poses.values().any(|x| {
                            let dx = p.player.feet[0] - x.position[0];
                            let dz = p.player.feet[2] - x.position[2];
                            dx * dx + dz * dz < 9.0 && p.player.feet[1] > x.position[1] + 1.5
                        })
                    })
                });
                let _ = held(d, HeldControl::Crouch, false);
                // Pressed against the side: hop in, as the horse test does.
                let _ = held(d, HeldControl::Jump, !on_top && step.is_multiple_of(240));
                if on_top {
                    // Step off backwards.
                    let _ = held(d, HeldControl::Forward, false);
                    let _ = held(d, HeldControl::Backward, true);
                } else {
                    let _ = held(d, HeldControl::Backward, false);
                }
            }
        }
        // Chat from everyone.
        if step.is_multiple_of(2400) {
            for (i, app) in s.apps.iter_mut().enumerate() {
                let text = format!(
                    "{} at {:.0} s",
                    ["hi", "brb", "nice build", "gg"][i],
                    t.as_secs_f32()
                );
                if request(
                    app,
                    UiAction::Chat {
                        channel: ChatChannel::Say,
                        text,
                    },
                )
                .is_ok()
                {
                    minute.chats += 1;
                }
            }
        }
        // Save, clear and reload.
        if t >= next_save {
            next_save += Duration::from_secs(save_every);
            let result = soak_save_reload(&mut s, t);
            minute.events.push(match result {
                Ok(line) => line,
                Err(e) => format!("SAVE/RELOAD FAILED: {e:#}"),
            });
        }
        if t >= next_minute {
            next_minute += Duration::from_secs(60);
            let host = &s.apps[0];
            let tick = host.network_view().map_or(0, |v| v.tick);
            minute.minute = minutes.len() as u32 + 1;
            minute.server_ticks = tick.saturating_sub(last_tick);
            last_tick = tick;
            minute.bricks = bricks(host);
            minute.vehicles = host.network_view().map_or(0, |v| v.vehicles.len());
            if let Some(v) = s.apps[3].network_view() {
                let me = v
                    .poses
                    .get(&v.owner)
                    .map(|p| glam::Vec3::from(p.player.feet));
                let jeep = v
                    .vehicle_poses
                    .values()
                    .next()
                    .map(|p| glam::Vec3::from(p.position));
                minute.events.push(format!(
                    "driver at {me:?}, jeep at {jeep:?}, {} vehicle poses, mounted {}",
                    v.vehicle_poses.len(),
                    mounted(&s.apps[3])
                ));
            }
            minute.players = s.apps.iter().map(|a| a.ui.core.players.len()).collect();
            minute.connected = s.apps.iter().map(in_game).collect();
            for (i, ms) in s.step_ms.iter_mut().enumerate() {
                let _ = i;
                let avg = if ms.is_empty() {
                    0.0
                } else {
                    ms.iter().sum::<f64>() / ms.len() as f64
                };
                minute.step_ms_avg.push((avg * 100.0).round() / 100.0);
                minute
                    .step_ms_max
                    .push(ms.iter().cloned().fold(0.0, f64::max).round());
                ms.clear();
            }
            let lines = bri_console::log::lines();
            minute.warnings = lines
                .iter()
                .filter(|l| l.level == bri_console::Level::Warning)
                .count();
            minute.errors = lines
                .iter()
                .filter(|l| l.level == bri_console::Level::Error)
                .count();
            for l in lines
                .iter()
                .filter(|l| l.level != bri_console::Level::Normal)
            {
                let text = format!("console: {}", l.text);
                if !minute.events.contains(&text) && minute.events.len() < 40 {
                    minute.events.push(text);
                }
            }
            bri_console::log::clear();
            println!(
                "minute {} bricks {} ticks {} vehicles {} step avg {:?} max {:?} connected {:?} planted {} shots {} mounted {:.0}s events {}",
                minute.minute,
                minute.bricks,
                minute.server_ticks,
                minute.vehicles,
                minute.step_ms_avg,
                minute.step_ms_max,
                minute.connected,
                minute.planted,
                minute.shots,
                minute.driver_mounted_s,
                minute.events.len()
            );
            for e in &minute.events {
                println!("    {e}");
            }
            minutes.push(std::mem::take(&mut minute));
            std::fs::write(out.join("soak.json"), serde_json::to_vec_pretty(&minutes)?)?;
            // A dropped guest rejoins, like a player would.
            for i in 1..4 {
                if !in_game(&s.apps[i]) {
                    s.apps[i].ui.core.pop(ScreenId::MessageBox);
                    let _ = request(
                        &mut s.apps[i],
                        UiAction::JoinServer {
                            address: "127.0.0.1".into(),
                            password: String::new(),
                        },
                    );
                }
            }
        }
        thread::sleep(Duration::from_millis(8));
    }
    // A last save and reload, then everyone leaves.
    let line = soak_save_reload(&mut s, start.elapsed());
    println!("final: {line:?}");
    for i in (0..4).rev() {
        let _ = request(&mut s.apps[i], UiAction::Disconnect);
    }
    s.run(Duration::from_secs(2), &mut events);
    std::fs::write(out.join("soak.json"), serde_json::to_vec_pretty(&minutes)?)?;
    line?;
    Ok(())
}

fn soak_save_reload(s: &mut Soaker, t: Duration) -> Result<String> {
    let name = format!("soak-{}.world.json", t.as_secs());
    let count = bricks(&s.apps[0]);
    let started = Instant::now();
    request(
        &mut s.apps[0],
        UiAction::SaveBricks {
            name: name.clone(),
            description: "soak".into(),
            events: true,
            ownership: true,
            overwrite: true,
        },
    )?;
    let mut ev = Vec::new();
    let until = |s: &mut Soaker,
                 what: &str,
                 ev: &mut Vec<String>,
                 ready: &dyn Fn(&[App]) -> bool|
     -> Result<()> {
        let start = Instant::now();
        while !ready(&s.apps) {
            ensure!(
                start.elapsed() < Duration::from_secs(60),
                "Timed out waiting for {what}"
            );
            ev.extend(s.tick());
            thread::sleep(Duration::from_millis(8));
        }
        Ok(())
    };
    until(s, "save", &mut ev, &|a| {
        a[0].pending_requests() == 0 && a[0].ui.core.save_files.iter().any(|f| f.name == name)
    })?;
    let saved = started.elapsed();
    let map = s.apps[0]
        .ui
        .core
        .save_files
        .iter()
        .find(|f| f.name == name)
        .map(|f| f.map.clone())
        .context("row")?;
    let stored = s.apps[0]
        .ui
        .core
        .save_files
        .iter()
        .find(|f| f.name == name)
        .and_then(|f| f.brick_count);
    request(&mut s.apps[0], UiAction::Admin(AdminAction::ClearAllBricks))?;
    until(s, "clear", &mut ev, &|a| bricks(&a[0]) == 0)?;
    let cleared = started.elapsed();
    request(
        &mut s.apps[0],
        UiAction::LoadBricks {
            map,
            name: name.clone(),
            ownership: true,
        },
    )?;
    until(s, "reload on every player", &mut ev, &|a| {
        a.iter().filter(|x| in_game(x)).all(|x| bricks(x) == count)
    })?;
    Ok(format!(
        "save/clear/reload of {count} bricks (file says {stored:?}): saved {:.1} s, cleared {:.1} s, reloaded everywhere {:.1} s; {} step problems",
        saved.as_secs_f32(),
        cleared.as_secs_f32(),
        started.elapsed().as_secs_f32(),
        ev.len()
    ))
}

/// Stress Lab Strata alone: plant a brick, save, reload (single player).
#[test]
#[ignore = "packaged content and a loopback server; no window"]
fn stress_lab_single_player_build_save_reload() -> Result<()> {
    if !qa_env(&["BRI_CONTENT_ROOT", "BRI_QA_OUT"]) {
        return Ok(());
    }
    let content = PathBuf::from(std::env::var_os("BRI_CONTENT_ROOT").context("BRI_CONTENT_ROOT")?);
    let out = PathBuf::from(std::env::var_os("BRI_QA_OUT").context("BRI_QA_OUT")?).join("strata");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;
    let mut app = load(&content, &out, "Miner")?;
    let mode = app
        .ui
        .core
        .game_modes
        .first()
        .context("no game mode")?
        .clone();
    request(
        &mut app,
        UiAction::HostGame {
            map: mode.map.clone().unwrap_or_default(),
            mode: ServerMode::SinglePlayer,
            game_mode: Some(mode.id.clone()),
            max_players: 1,
            server_name: "Strata".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    let start = Instant::now();
    while !(in_game(&app) && grounded(&app)) {
        ensure!(start.elapsed() < Duration::from_secs(180), "never in game");
        run_for(&mut app, 50)?;
    }
    run_for(&mut app, 2000)?;
    let before = bricks(&app);
    println!("strata bricks at start {before}");
    let mut outcome = Vec::new();
    for quarter in 0..4 {
        aim(&mut app, quarter as f32 * std::f32::consts::FRAC_PI_2, DOWN)?;
        run_for(&mut app, 150)?;
        request(
            &mut app,
            UiAction::InstantUseBrick {
                brick: BRICK.into(),
            },
        )?;
        request(
            &mut app,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: true,
            }),
        )?;
        request(
            &mut app,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: false,
            }),
        )?;
        let ghost = app.building().and_then(|b| b.ghost()).map(|g| g.position);
        request(&mut app, UiAction::Game(GameAction::PlantBrick))?;
        run_for(&mut app, 600)?;
        outcome.push(format!(
            "quarter {quarter}: ghost {ghost:?}, bricks {}, plant error {:?}, chat {:?}",
            bricks(&app),
            app.ui.core.plant_error,
            app.ui
                .core
                .chat
                .lines
                .iter()
                .rev()
                .take(2)
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
        ));
        if bricks(&app) > before {
            break;
        }
    }
    for o in &outcome {
        println!("{o}");
    }
    let count = bricks(&app);
    request(
        &mut app,
        UiAction::SaveBricks {
            name: "strata.world.json".into(),
            description: "".into(),
            events: true,
            ownership: true,
            overwrite: true,
        },
    )?;
    let start = Instant::now();
    while !(app.pending_requests() == 0
        && app
            .ui
            .core
            .save_files
            .iter()
            .any(|f| f.name == "strata.world.json"))
    {
        ensure!(start.elapsed() < Duration::from_secs(30), "no save");
        run_for(&mut app, 50)?;
    }
    let row = app
        .ui
        .core
        .save_files
        .iter()
        .find(|f| f.name == "strata.world.json")
        .unwrap()
        .clone();
    println!(
        "saved {count} live bricks; file {:?} bricks under {:?}",
        row.brick_count, row.map
    );
    request(
        &mut app,
        UiAction::LoadBricks {
            map: row.map.clone(),
            name: row.name.clone(),
            ownership: true,
        },
    )?;
    run_for(&mut app, 8000)?;
    println!(
        "after reload into the same world: {} bricks (was {count})",
        bricks(&app)
    );
    Ok(())
}

/// Probe: plant on a map in single player and print what the player is told.
/// BRI_QA_PROBE_MAP picks the map (default The Slopes).
#[test]
#[ignore = "packaged content and a loopback server; no window"]
fn plant_probe_single_player() -> Result<()> {
    if !qa_env(&["BRI_CONTENT_ROOT", "BRI_QA_OUT"]) {
        return Ok(());
    }
    let content = PathBuf::from(std::env::var_os("BRI_CONTENT_ROOT").context("BRI_CONTENT_ROOT")?);
    let out = PathBuf::from(std::env::var_os("BRI_QA_OUT").context("BRI_QA_OUT")?).join("probe");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;
    let mut map =
        std::env::var("BRI_QA_PROBE_MAP").unwrap_or("v20/add-ons/map_slopes/slopes.mis".into());
    let mut app = load(&content, &out, "Prober")?;
    // BRI_QA_PROBE_MODE=1 plays the first game mode on its own world.
    let game_mode = std::env::var("BRI_QA_PROBE_MODE")
        .ok()
        .and_then(|_| app.ui.core.game_modes.first().cloned());
    if let Some(m) = &game_mode {
        map = m.map.clone().unwrap_or(map);
    }
    request(
        &mut app,
        UiAction::HostGame {
            map: map.clone(),
            mode: ServerMode::SinglePlayer,
            game_mode: game_mode.map(|m| m.id),
            max_players: 1,
            server_name: "Probe".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    let start = Instant::now();
    while !(in_game(&app) && grounded(&app)) {
        ensure!(start.elapsed() < Duration::from_secs(180), "never in game");
        run_for(&mut app, 50)?;
    }
    run_for(&mut app, 1000)?;
    let me = app.presented_local().map(|p| p.feet);
    println!("player at {me:?}");
    for quarter in 0..4 {
        aim(&mut app, quarter as f32 * std::f32::consts::FRAC_PI_2, DOWN)?;
        run_for(&mut app, 150)?;
        request(
            &mut app,
            UiAction::InstantUseBrick {
                brick: BRICK.into(),
            },
        )?;
        request(
            &mut app,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: true,
            }),
        )?;
        request(
            &mut app,
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down: false,
            }),
        )?;
        let ghost = app.building().and_then(|b| b.ghost()).map(|g| g.position);
        let before = bricks(&app);
        let target = app.building().zip(app.presented_local()).map(|(b, p)| {
            b.target(b.archetypes().eye(p), p.forward(), 15.0)
                .ok()
                .flatten()
                .map(|h| (h.position, h.normal, h.brick))
        });
        println!("  target {target:?}");
        bri_console::log::clear();
        app.ui.core.request(UiAction::Game(GameAction::PlantBrick));
        let commands = app.pump()?;
        let _ = commands;
        let mut seen = Vec::new();
        for _ in 0..40 {
            app.tick(Duration::from_millis(16))?;
            app.ui.update(16);
            app.pump()?;
            let line = format!(
                "plant_error {:?}, stack {:?}, pending {}",
                app.ui.core.plant_error.map(|p| p.0),
                app.ui.stack(),
                app.pending_requests()
            );
            if seen.last() != Some(&line) {
                seen.push(line);
            }
            thread::sleep(Duration::from_millis(16));
        }
        println!(
            "quarter {quarter}: ghost {ghost:?} bricks {before} -> {}; {seen:?}; console {:?}; chat {:?}",
            bricks(&app),
            bri_console::log::lines()
                .iter()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>(),
            app.ui
                .core
                .chat
                .lines
                .iter()
                .rev()
                .take(2)
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
        );
        request(&mut app, UiAction::Game(GameAction::CancelBrick))?;
    }
    Ok(())
}
