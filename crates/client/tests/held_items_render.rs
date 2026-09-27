//! Bounded normal-App render probe for held balls and the Akimbo Guns.
//! Run with: cargo test -p bri-client --test held_items_render --release -- --ignored --nocapture
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
fn held(app: &App, hand: u8) -> Option<String> {
    let view = app.network_view()?;
    view.weapons
        .images
        .get(&view.owner)?
        .iter()
        .find(|i| i.hand == hand)
        .map(|i| i.image.clone())
}

#[test]
#[ignore = "requires converted native v20 content, loopback QUIC and offscreen GPU; no window/audio device"]
fn start_ball_is_held_and_thrown_and_akimbo_raises_both_arms() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifact = workspace.join("artifacts/held-items");
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
        server_name: "Held item render".into(),
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
    let rules = MiniGameRules {
        loadout: [
            Some("v20.weapon.basketballitem".into()),
            Some("v20.weapon.akimbogunitem".into()),
            None,
            None,
            None,
        ],
        ..MiniGameRules::default()
    };
    app.ui
        .core
        .request(UiAction::CreateMiniGame { color: 0, rules });
    pump(&mut app)?;
    until(&mut app, "start ball in hand", |a| {
        held(a, 0).is_some_and(|i| i.contains("basketball"))
    })?;
    until(&mut app, "player landing", |a| {
        a.local_motion()
            .is_some_and(|(p, _)| p.grounded && glam::Vec3::from(p.velocity).length() < 0.001)
    })?;
    run_for(&mut app, 0.5)?;
    let gpu = Headless::new().context("offscreen held-item renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let mut report = serde_json::json!({"adapter": gpu.adapter_info.name, "map": BEDROOM});
    save(
        &artifact.join("ball-first-person.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    app.ui
        .core
        .request(UiAction::Game(GameAction::ToggleFirstPerson {
            fast: false,
        }));
    pump(&mut app)?;
    run_for(&mut app, 0.5)?;
    save(
        &artifact.join("ball-third-person.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    report["ball_visible_instances"] = app.world_item_stats().visible_instances.into();
    ensure!(
        app.world_item_stats().visible_instances >= 1,
        "held basketball has no mounted world-item instance"
    );

    // Hold fire to wind up the shot, then release to throw.
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Fire,
        down: true,
    }));
    pump(&mut app)?;
    run_for(&mut app, 0.6)?;
    save(
        &artifact.join("ball-shooting.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Fire,
        down: false,
    }));
    pump(&mut app)?;
    until(&mut app, "thrown basketball", |a| {
        a.network_view().is_some_and(|v| {
            v.weapons
                .projectiles
                .iter()
                .any(|p| p.definition.contains("basketball"))
        })
    })?;
    ensure!(held(&app, 0).is_none(), "the thrown ball is still in hand");
    run_for(&mut app, 0.15)?;
    save(
        &artifact.join("ball-thrown.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;

    app.ui.core.request(UiAction::UseTool { slot: 1 });
    pump(&mut app)?;
    until(&mut app, "akimbo guns", |a| {
        held(a, 1).is_some_and(|i| i.contains("lefthandedgun"))
    })?;
    run_for(&mut app, 0.5)?;
    save(
        &artifact.join("akimbo-third-person.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    report["akimbo_visible_instances"] = app.world_item_stats().visible_instances.into();
    ensure!(
        app.world_item_stats().visible_instances >= 2,
        "akimbo guns are not both drawn"
    );
    // Fire the akimbo pair: casings eject from both guns.
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Fire,
        down: true,
    }));
    pump(&mut app)?;
    run_for(&mut app, 0.2)?;
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Fire,
        down: false,
    }));
    pump(&mut app)?;
    run_for(&mut app, 0.25)?;
    report["shell_casings"] = app.weapon_shell_count().into();
    save(
        &artifact.join("akimbo-fired.png"),
        &capture(&mut app, &gpu, &mut renderer)?,
    )?;
    ensure!(
        app.weapon_shell_count() >= 2,
        "fired akimbo guns ejected no casings"
    );

    let dark = capture(&mut app, &gpu, &mut renderer)?;
    app.ui.core.request(UiAction::Game(GameAction::UseLight));
    pump(&mut app)?;
    until(&mut app, "player light", |a| {
        a.network_view()
            .is_some_and(|v| v.vitals.get(&v.owner).is_some_and(|v| v.light))
    })?;
    run_for(&mut app, 0.2)?;
    let lit = capture(&mut app, &gpu, &mut renderer)?;
    save(&artifact.join("light-third-person.png"), &lit)?;
    save(&artifact.join("light-off-third-person.png"), &dark)?;
    std::fs::write(
        artifact.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    app.gpu_stopped();
    Ok(())
}
