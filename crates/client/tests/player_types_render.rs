//! Bounded normal-App render probe for v20 player types.
//! Run with: cargo test -p bri-client --test player_types_render --release -- --ignored --nocapture
//! Requires converted v20 content, loopback QUIC and an offscreen GPU; never opens a window.
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
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SIZE: (u32, u32) = (640, 480);
const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";

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
fn until(app: &mut App, what: &str, ready: impl Fn(&App) -> bool) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    loop {
        let now = Instant::now();
        step(app, now.duration_since(previous))?;
        previous = now;
        if ready(app) {
            return Ok(());
        }
        ensure!(
            start.elapsed() < Duration::from_secs(45),
            "Timed out waiting for {what}: {:?}",
            app.ui.core.conn
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn run_for(app: &mut App, seconds: f32) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    while start.elapsed().as_secs_f32() < seconds {
        thread::sleep(Duration::from_millis(10));
        let now = Instant::now();
        step(app, now.duration_since(previous))?;
        previous = now;
    }
    Ok(())
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
#[test]
#[ignore = "requires converted native v20 content, loopback QUIC and offscreen GPU; no window/audio device"]
fn horse_players_draw_as_horses_and_fuel_jets_show_energy() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifact = workspace.join("artifacts/player-types");
    std::fs::create_dir_all(&artifact)?;
    let state = artifact.join(format!(
        "state-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    std::fs::create_dir_all(&state)?;
    let mut app = App::load(&workspace.join("content"), &state, SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::SinglePlayer,
        max_players: 1,
        server_name: "Player type render".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    pump(&mut app)?;
    until(&mut app, "Bedroom host/player", |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner))
    })?;
    let gpu = Headless::new().context("offscreen player-type renderer")?;
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
        &artifact.join("standard.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    let datablock = |a: &App| a.local_motion().map(|(p, _)| p.datablock);
    for (id, label) in [
        ("v20.player.horsearmor", "horse"),
        ("v20.player.playerfueljet", "fueljet"),
    ] {
        let rules = MiniGameRules {
            player_type: id.into(),
            ..MiniGameRules::default()
        };
        let action = if label == "horse" {
            UiAction::CreateMiniGame { color: 0, rules }
        } else {
            let game = app
                .ui
                .core
                .minigames
                .active_game
                .context("minigame to configure")?;
            UiAction::ConfigureMiniGame { game, rules }
        };
        app.ui.core.request(action);
        pump(&mut app)?;
        until(&mut app, label, |a| {
            datablock(a).is_some_and(|d| d.id() == id)
        })?;
        run_for(&mut app, 1.0)?;
        save(
            &artifact.join(format!("{label}.png")),
            &capture(&mut app, &gpu, &mut renderer)?,
        )?;
    }
    // The Horse's 10 energy carried over; rechargeRate refills the tank.
    run_for(&mut app, 3.0)?;
    ensure!(
        app.ui.core.energy.is_some_and(|e| e > 0.9),
        "Fuel-Jet energy bar missing: {:?}",
        app.ui.core.energy
    );
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Jet,
        down: true,
    }));
    pump(&mut app)?;
    run_for(&mut app, 1.5)?;
    let drained = app.ui.core.energy;
    save(
        &artifact.join("fueljet-jetting.png"),
        &capture_hud(&mut app, &gpu, &mut renderer, true)?,
    )?;
    ensure!(
        drained.is_some_and(|e| e < 0.8),
        "jetting did not drain the energy bar: {drained:?}"
    );
    app.gpu_stopped();
    Ok(())
}
