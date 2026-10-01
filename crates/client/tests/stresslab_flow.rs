//! Stress Lab through the real client, offscreen: host the package world from
//! Start Game, see the miner HUD, mine with its key, meet a creeper, save.
//! Runs with the repo's Stress Lab packages over the made-up content root;
//! the ignored variant runs over the generated v20 content (`--release --
//! --ignored`, BRI_CONTENT or content/, or BRI_STRESSLAB_CONTENT for a
//! packaged release's root). Never creates a window, audio device or OS
//! input.
use anyhow::{Context, Result, bail, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    screens::ScreenId,
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[macro_use]
mod support;
use support::content_root::ContentRoot;

synthetic_and_content!(ContentRoot: stress_lab_hosts_shows_the_miner_hud_mines_and_meets_a_creeper);

const SIZE: (u32, u32) = (960, 720);
const WORLD: &str = "stresslab-world:world/strata";

fn pump(app: &mut App) -> Result<()> {
    ensure!(
        app.pump()?.is_empty(),
        "Unexpected native window command in headless test"
    );
    Ok(())
}

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    pump(app)?;
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        bail!("Native app connection failed: {reason}");
    }
    Ok(())
}

fn until(app: &mut App, what: &str, timeout: Duration, ready: impl Fn(&App) -> bool) -> Result<()> {
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
            start.elapsed() < timeout,
            "Timed out waiting for {what}; screens {:?}; tools {:?}; state {:?}; world bricks {:?}; pending {}; ghost {:?}; dialogs {:?}",
            app.ui.stack(),
            app.network_view()
                .and_then(|v| v.tools.get(&v.owner).cloned()),
            app.ui.core.conn,
            app.network_view().map(|v| v.world.bricks.len()),
            app.pending_requests(),
            app.building().and_then(|b| b.ghost()),
            app.ui.screen(ScreenId::MessageBox).map(|s| s
                .view()
                .walk()
                .map(|n| s.view().text_of(n))
                .collect::<Vec<_>>())
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn action(app: &mut App, action: UiAction) -> Result<()> {
    app.ui.core.request(action);
    pump(app)?;
    app.ui.update(0);
    Ok(())
}

fn capture(
    app: &mut App,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    with_ui: bool,
) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("native app-flow compositor"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    ensure!(
        app.render_scene(&mut RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target: &target_view,
            format,
            size: SIZE,
            ui_renderer: renderer,
        })?,
        "Entered session did not render an authoritative player camera"
    );
    if with_ui {
        renderer.render(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &target_view,
            format,
            SIZE,
            app.ui.scale(),
            &app.ui.core.pack,
            &app.ui.draw(),
            None,
        );
    }
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("native app-flow readback"),
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
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
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

fn package_set() -> bri_package::packages::PackageSet {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    let mut packages = vec![PackageEntry {
        id: "v20-bricks".into(),
        version: "4.0.0".into(),
        side: Side::Shared,
        dir: "unused".into(),
        role: Some("brick_catalog".into()),
    }];
    for (id, side) in [
        ("stresslab-world", Side::Server),
        ("stresslab-creeper", Side::Server),
        ("stresslab-creeper-model", Side::Client),
        ("stresslab-economy", Side::Server),
        ("stresslab-hud", Side::Client),
        ("stresslab-mode", Side::Server),
    ] {
        packages.push(PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side,
            dir: id.into(),
            role: None,
        });
    }
    PackageSet {
        schema_version: 1,
        packages,
    }
}

fn own(app: &App, key: &str) -> Option<i64> {
    let v = app.network_view()?;
    v.package_state
        .packages
        .get("stresslab-economy")?
        .players
        .get(&v.owner)?
        .get(key)?
        .as_i64()
}

fn stress_lab_hosts_shows_the_miner_hud_mines_and_meets_a_creeper(f: &ContentRoot) -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifact = f.out("stresslab-client")?;
    let state_dir = f.state()?;
    let state = state_dir.path();
    // BRI_STRESSLAB_CONTENT checks a packaged release's content root, whose
    // packages.json enables the Stress Lab; otherwise this checkout's
    // packages are enabled over the content root.
    let packaged = std::env::var_os("BRI_STRESSLAB_CONTENT")
        .filter(|_| f.content)
        .map(std::path::PathBuf::from);
    let mut app = App::load(packaged.as_deref().unwrap_or(&f.root), state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    if packaged.is_none() {
        app.enable_packages(&workspace.join("packages/stresslab"), &package_set())?;
    }
    ensure!(
        app.content.maps.iter().any(|m| m.id == WORLD),
        "Start Game lists the package world"
    );
    let gpu = support::gpu::turn()?;
    let mut ui_renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    action(
        &mut app,
        UiAction::HostGame {
            map: WORLD.into(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 1,
            server_name: "Stress Lab".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    until(
        &mut app,
        "the package world and the player",
        Duration::from_secs(60),
        |a| {
            a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner) && v.world.bricks.len() > 5_000)
                && own(a, "bits") == Some(0)
        },
    )?;
    for _ in 0..40 {
        step(&mut app, Duration::from_millis(16))?;
    }
    // The miner HUD is on screen with the server's values.
    let panel = app
        .ui
        .core
        .package_panels
        .first()
        .context("miner panel")?
        .clone();
    ensure!(panel.title == "STRESS LAB MINER", "{panel:?}");
    ensure!(
        panel.rows[0] == ("Bits".to_string(), "0".to_string(), panel.rows[0].2),
        "{panel:?}"
    );
    let keys: Vec<char> = app.ui.core.package_keys.iter().map(|k| k.key).collect();
    let free: String = ('a'..='z')
        .filter(|c| {
            app.ui
                .core
                .binds
                .command_for_key(
                    bri_ui::input::Key::Letter(*c),
                    bri_ui::input::Modifiers::NONE,
                )
                .is_none()
        })
        .collect();
    eprintln!("letters the base game leaves free: {free}");
    // Mine: look down and use the panel's key when the base game leaves it free.
    action(
        &mut app,
        UiAction::Game(GameAction::Look {
            yaw: 0.,
            pitch: 1.5,
        }),
    )?;
    for _ in 0..10 {
        step(&mut app, Duration::from_millis(16))?;
    }
    for _ in 0..6 {
        if keys.contains(&'h') {
            app.ui.handle_input(bri_ui::input::InputEvent::KeyDown {
                key: bri_ui::input::Key::Letter('h'),
                mods: bri_ui::input::Modifiers::NONE,
                repeat: false,
            });
            app.ui.handle_input(bri_ui::input::InputEvent::KeyUp {
                key: bri_ui::input::Key::Letter('h'),
                mods: bri_ui::input::Modifiers::NONE,
            });
        } else {
            action(
                &mut app,
                UiAction::Game(GameAction::Package {
                    package: "stresslab-economy".into(),
                    command: "mine".into(),
                    pressed: None,
                }),
            )?;
        }
        for _ in 0..16 {
            step(&mut app, Duration::from_millis(16))?;
        }
    }
    until(&mut app, "mining", Duration::from_secs(10), |a| {
        own(a, "mined").unwrap_or(0) > 0
    })?;
    for _ in 0..10 {
        step(&mut app, Duration::from_millis(16))?;
    }
    let mined = own(&app, "mined").unwrap_or(0);
    let panel = app.ui.core.package_panels[0].clone();
    ensure!(
        panel
            .rows
            .iter()
            .any(|r| r.0 == "Blocks mined" && r.1 == mined.to_string()),
        "{panel:?}"
    );
    // A creeper, drawn from its package model.
    action(
        &mut app,
        UiAction::Game(GameAction::Package {
            package: "stresslab-creeper".into(),
            command: "spawn".into(),
            pressed: None,
        }),
    )?;
    until(&mut app, "the creeper", Duration::from_secs(10), |a| {
        a.network_view().is_some_and(|v| !v.entities.is_empty())
    })?;
    action(
        &mut app,
        UiAction::Game(GameAction::Look {
            yaw: 0.,
            pitch: -1.2,
        }),
    )?;
    for _ in 0..30 {
        step(&mut app, Duration::from_millis(16))?;
    }
    let frame = capture(&mut app, &gpu, &mut ui_renderer, true)?;
    image::save_buffer(
        artifact.join("stresslab-hud.png"),
        &frame,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    // The panel's accent bar is drawn at the top right.
    let pixel = |x: u32, y: u32| &frame[((y * SIZE.0 + x) * 4) as usize..][..4];
    let accent_found = (SIZE.0 - 200..SIZE.0 - 10).any(|x| {
        (8..30).any(|y| {
            let p = pixel(x, y);
            p[0] > 200 && p[1] > 140 && p[2] < 90
        })
    });
    ensure!(accent_found, "the miner panel's accent colour is on screen");
    action(&mut app, UiAction::Disconnect)?;
    for _ in 0..100 {
        step(&mut app, Duration::from_millis(16))?;
        if state
            .join("packages")
            .read_dir()
            .is_ok_and(|mut d| d.next().is_some())
        {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let saved = state
        .join("packages")
        .join("stresslab-world-world-strata.save.json");
    ensure!(
        saved.is_file(),
        "the host saved the package world to {}",
        saved.display()
    );
    std::fs::write(
        artifact.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "hosted": WORLD, "mined": mined, "panel": panel.title, "keys": keys,
            "free_letters": free, "save": saved, "visible_window": false, "audio_device": false, "adapter": gpu.adapter_info.name,
        }))?,
    )?;
    app.gpu_stopped();
    Ok(())
}
