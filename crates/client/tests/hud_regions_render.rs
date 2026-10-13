//! Bounded normal-App render probe for HUD regions and body cameras: the
//! Playground's Runner shows its own textured HUD (a power meter strip, a
//! sprite-font counter and a reticle in place of the crosshair); `/trooper`
//! switches to the Trooper, whose camera profile narrows the view, bobs it
//! as it walks, trails its weapon behind turns and hangs the third-person
//! camera over a shoulder, while the stock HUD comes back; `/blockhead`
//! hands everything back to the stock camera and HUD. Loopback QUIC and an
//! offscreen GPU; never opens a window.
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_content::hud::Region;
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

synthetic_and_content!(ContentRoot: the_hud_and_camera_follow_the_body);

const SIZE: (u32, u32) = (640, 480);
const TROOPER: &str = "sample-trooper:archetype/trooper";
const RUNNER: &str = "playground-runner:archetype/runner";

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
/// Wait for `ready`, budgeted in the host's game time (as the body
/// attachments probe does), failing when nothing advances for `STALL`.
const GAME_BUDGET: u64 = 30 * 120;
const STALL: Duration = Duration::from_secs(90);
fn until(app: &mut App, what: &str, ready: impl Fn(&App) -> bool) -> Result<()> {
    // Catch up first: the host keeps ticking while the test captures a
    // frame (seconds on a software GPU), and those ticks are not the wait's.
    step(app, Duration::ZERO)?;
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
            "{what} did not happen within {} s of game time: {:?}; body {:?}; chat {:?}",
            GAME_BUDGET / 120,
            app.ui.core.conn,
            body(app),
            app.network_view()
                .map(|v| v.chat.iter().map(|c| c.text.clone()).collect::<Vec<_>>())
        );
        ensure!(
            now.duration_since(last_progress) < STALL,
            "Stalled waiting for {what}: {:?}",
            app.ui.core.conn
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn run_for(app: &mut App, seconds: f32) -> Result<()> {
    wait::run_one_for(app, Duration::from_secs_f32(seconds), step)
}
/// The local player's body (archetype id).
fn body(app: &App) -> Option<String> {
    let view = app.network_view()?;
    let pose = view.poses.get(&view.owner)?;
    Some(view.archetypes.resolve(pose.player.archetype).id.clone())
}
fn capture(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hud offscreen target"),
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
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hud readback"),
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
fn pixel(data: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * SIZE.0 + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2]]
}
fn near(a: [u8; 3], b: [u8; 3], within: i32) -> bool {
    a.iter()
        .zip(b)
        .all(|(a, b)| (i32::from(*a) - i32::from(b)).abs() <= within)
}
fn held(app: &mut App, control: HeldControl, down: bool) -> Result<()> {
    app.ui
        .core
        .request(UiAction::Game(GameAction::Held { control, down }));
    pump(app)
}
fn turn(app: &mut App, yaw: f32) -> Result<()> {
    app.ui
        .core
        .request(UiAction::Game(GameAction::Look { yaw, pitch: 0.0 }));
    pump(app)
}
fn command(app: &mut App, name: &str) -> Result<()> {
    app.ui.core.request(UiAction::ChatCommand {
        name: name.into(),
        args: Vec::new(),
    });
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
    "playground/playground-runner-hud",
    "samples/sample-trooper",
    "playground/playground-rifle",
    "playground/playground-buggy",
    "playground/playground-stomper-look",
    "playground/playground-stomper",
    "playground/playground-mode",
];
impl Staged {
    fn install(content: &Path) -> Result<Self> {
        let folder = format!(".hud-add-ons-{}", std::process::id());
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
fn claimed(app: &App) -> Vec<Region> {
    app.ui.core.hud_regions.iter().map(|r| r.region).collect()
}

fn the_hud_and_camera_follow_the_body(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("hud-regions")?;
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
        server_name: "HUD regions render".into(),
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
    until(&mut app, "the Runner landed", |a| {
        body(a).as_deref() == Some(RUNNER)
            && a.local_motion()
                .is_some_and(|(p, _)| p.grounded && glam::Vec3::from(p.velocity).length() < 0.001)
    })?;
    let gpu = support::gpu::turn().context("offscreen HUD renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    run_for(&mut app, 0.5)?;

    // The Runner's HUD: three regions, the crosshair among them.
    let regions = claimed(&app);
    ensure!(
        regions.contains(&Region::TopCenter)
            && regions.contains(&Region::TopLeft)
            && regions.contains(&Region::Crosshair),
        "the Runner's HUD regions: {regions:?}"
    );
    ensure!(
        app.ui.core.hud_claimed(Region::Crosshair),
        "the reticle replaces the stock crosshair"
    );
    let runner = capture(&mut app, &gpu, &mut renderer)?;
    save(&artifact.join("runner-hud.png"), &runner)?;
    // The power meter (top centre, 80 units square at 80, 12 in a box
    // 240 wide): full health lights its wedges blue, its rim is gold and
    // its hub white, all from the Add-On's own texture.
    let meter = (320, 52);
    let hub = pixel(&runner, meter.0, meter.1);
    let wedge = pixel(&runner, meter.0 + 25, meter.1 + 5);
    let rim = pixel(&runner, meter.0, meter.1 - 37);
    eprintln!("meter hub {hub:?} wedge {wedge:?} rim {rim:?}");
    ensure!(near(hub, [250, 250, 240], 24), "meter hub {hub:?}");
    ensure!(
        near(wedge, [70, 150, 255], 24),
        "full-health wedge {wedge:?}"
    );
    ensure!(near(rim, [230, 180, 40], 24), "meter rim {rim:?}");

    // Draw the Carbine: the Runner's ammo counter replaces the server's
    // ammo display, from the magazine the server sends its holder.
    app.ui.core.request(UiAction::UseTool { slot: 0 });
    until(&mut app, "the ammo counter", |a| {
        a.ui.core.hud_regions.iter().any(|r| {
            r.region == Region::Ammo
                && r.items
                    .iter()
                    .any(|i| matches!(i, bri_content::hud::Item::Image { .. }))
        })
    })?;
    run_for(&mut app, 0.5)?;
    ensure!(
        app.ui.core.bottom_print_ammo && app.ui.core.hud_claimed(Region::Ammo),
        "the ammo display is the bottom print, and the counter hides it"
    );
    let counter = app
        .ui
        .core
        .hud_regions
        .iter()
        .find(|r| r.region == Region::Ammo)
        .context("ammo region")?;
    let digits = counter
        .items
        .iter()
        .filter(|i| matches!(i, bri_content::hud::Item::Image { .. }))
        .count();
    // "24" in the magazine and "x" and the reserve's digits below it.
    ensure!(digits >= 4, "sprite digits drawn: {digits}");
    let armed = capture(&mut app, &gpu, &mut renderer)?;
    save(&artifact.join("runner-ammo.png"), &armed)?;

    // Become the Trooper: its camera owns the view, and the stock HUD is
    // back, for the Runner's HUD claimed only the Runner's body.
    let stock_fov = app.controls.fov();
    command(&mut app, "trooper")?;
    until(&mut app, "the Trooper", |a| {
        body(a).as_deref() == Some(TROOPER)
    })?;
    run_for(&mut app, 1.0)?;
    ensure!(
        claimed(&app).is_empty() && !app.ui.core.hud_claimed(Region::Crosshair),
        "the Runner's HUD stays after switching: {:?}",
        claimed(&app)
    );
    let fov = app.controls.fov();
    ensure!(
        (fov - stock_fov * 72.0 / 90.0).abs() < 0.5,
        "the Trooper's FOV is {fov}, from a stock {stock_fov}"
    );
    let trooper = capture(&mut app, &gpu, &mut renderer)?;
    save(&artifact.join("trooper-first-person.png"), &trooper)?;
    ensure!(
        pixel(&trooper, meter.0, meter.1) != hub
            || pixel(&trooper, meter.0 + 25, meter.1 + 5) != wedge,
        "the Runner's meter is still drawn"
    );
    // Walking bobs the view and turning trails the weapon.
    held(&mut app, HeldControl::Forward, true)?;
    let mut bobbed: f32 = 0.0;
    let mut trailed = false;
    for _ in 0..40 {
        turn(&mut app, 0.02)?;
        run_for(&mut app, 1.0 / 30.0)?;
        let yaw = app.controls.view_angles().0;
        bobbed = bobbed.max((app.controls.body_eye_offset(yaw).y - 0.08).abs());
        trailed |= app.controls.view_model_sway().is_some();
    }
    held(&mut app, HeldControl::Forward, false)?;
    eprintln!("Trooper bob {bobbed}, weapon trailed {trailed}");
    ensure!(bobbed > 0.005, "walking does not bob the Trooper's view");
    ensure!(trailed, "turning does not trail the Trooper's weapon");
    save(
        &artifact.join("trooper-walking.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    // Third person hangs over the right shoulder.
    app.ui
        .core
        .request(UiAction::Game(GameAction::ToggleFirstPerson { fast: true }));
    pump(&mut app)?;
    run_for(&mut app, 1.0)?;
    let yaw = app.controls.view_angles().0;
    let shoulder = app.controls.body_pivot_offset(yaw);
    ensure!(
        shoulder.length() > 0.8,
        "the Trooper's third person pivot: {shoulder:?}"
    );
    save(
        &artifact.join("trooper-third-person.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    app.ui
        .core
        .request(UiAction::Game(GameAction::ToggleFirstPerson { fast: true }));
    pump(&mut app)?;

    // Back to a Blockhead: the stock camera and the stock HUD, exactly.
    command(&mut app, "blockhead")?;
    until(&mut app, "the Blockhead", |a| {
        body(a).as_deref() == Some("v20.player.playerstandardarmor")
    })?;
    run_for(&mut app, 1.5)?;
    ensure!(
        app.controls.fov() == stock_fov,
        "the FOV did not come back: {} after {stock_fov}",
        app.controls.fov()
    );
    let yaw = app.controls.view_angles().0;
    ensure!(
        app.controls.body_eye_offset(yaw) == glam::Vec3::ZERO
            && app.controls.body_pivot_offset(yaw) == glam::Vec3::ZERO
            && app.controls.view_model_sway().is_none(),
        "the Trooper's camera stayed on the Blockhead"
    );
    ensure!(claimed(&app).is_empty(), "{:?}", claimed(&app));
    save(
        &artifact.join("blockhead-stock.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    app.gpu_stopped();
    drop(staged);
    Ok(())
}
