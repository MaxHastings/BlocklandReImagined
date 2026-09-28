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
    pub host: App,
    pub guest: App,
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

/// The guest aims at the floor in front of itself and plants one brick.
pub fn plant(pair: &mut Pair, yaw: f32) -> Result<()> {
    let before = bricks(&pair.guest);
    request(
        &mut pair.guest,
        UiAction::Game(GameAction::Look { yaw, pitch: 1.0 }),
    )?;
    request(
        &mut pair.guest,
        UiAction::InstantUseBrick {
            brick: BRICK.into(),
        },
    )?;
    request(
        &mut pair.guest,
        UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down: true,
        }),
    )?;
    request(
        &mut pair.guest,
        UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down: false,
        }),
    )?;
    ensure!(
        pair.guest.building().and_then(|b| b.ghost()).is_some(),
        "Brick fire did not deploy a ghost"
    );
    request(&mut pair.guest, UiAction::Game(GameAction::PlantBrick))?;
    pair.until("planted brick on both", Duration::from_secs(10), |h, g| {
        bricks(h) == before + 1 && bricks(g) == before + 1 && g.pending_requests() == 0
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
        format!("guest chat tail {chat:?}")
    })?;
    request(&mut pair.guest, UiAction::Game(GameAction::CancelBrick))?;
    Ok(())
}

/// The guest hammers the brick it aims at until one brick is gone.
pub fn hammer(pair: &mut Pair, yaw: f32) -> Result<()> {
    let before = bricks(&pair.host);
    request(&mut pair.guest, UiAction::UseTool { slot: 0 })?;
    request(
        &mut pair.guest,
        UiAction::Game(GameAction::Look { yaw, pitch: 1.0 }),
    )?;
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
    bail!("Hammer did not remove the brick ({before} bricks remain)")
}

fn save_and_reload(pair: &mut Pair, name: &str, steps: &mut Vec<String>) -> Result<()> {
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
    hammer(pair, std::f32::consts::PI)?;
    steps.push("hammered".into());
    request(
        &mut pair.host,
        UiAction::LoadBricks {
            map: map.clone(),
            name: file.clone(),
            ownership: true,
        },
    )?;
    pair.until(
        "loaded bricks replicated",
        Duration::from_secs(20),
        |h, g| h.pending_requests() == 0 && bricks(h) == count && bricks(g) == count,
    )
    .with_context(|| {
        format!(
            "host {} guest {} want {count}",
            bricks(&pair.host),
            bricks(&pair.guest)
        )
    })?;
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
    let yaw = std::f32::consts::PI;
    plant(pair, yaw)?;
    report.steps.push("planted".into());
    save_and_reload(pair, &name, &mut report.steps)?;
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
        for screen in [ScreenId::MessageBox] {
            app.ui.core.pop(screen);
        }
    }
}

fn load(content: &Path, out: &Path, name: &str) -> Result<App> {
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
                    request(&mut pair.host, UiAction::Admin(AdminAction::RequestMaps))?;
                    pair.until("map list", Duration::from_secs(10), |h, _| {
                        h.ui.core.admin.maps.iter().any(|m| m.id == *map)
                    })?;
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
