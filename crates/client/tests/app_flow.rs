//! Explicit local-content integration probes. The weather probe runs on the
//! made-up content root too; the ignored variants run on the generated v20
//! content (`--release -- --ignored --nocapture`, BRI_CONTENT or
//! content/). Never creates a window or OS input.
use anyhow::{Result, bail, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    screens::ScreenId,
};
use std::time::Duration;

#[macro_use]
mod support;
use support::{
    content_root::{ContentRoot, RAIN},
    wait,
};

synthetic_and_content!(ContentRoot: native_weather_map_settings_render_and_disconnect);

const SIZE: (u32, u32) = (960, 720);
const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";

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

/// How much game time a wait may take before it counts as a hang. The waits
/// end on what they wait for; this bound only catches a hang, so it sits far
/// past what the slowest step takes ([`wait::until`]).
const HANG: Duration = Duration::from_secs(300);

/// Step the app until `ready` holds, with `HANG` of game time to spare.
fn until(app: &mut App, what: &str, ready: impl Fn(&App) -> bool) -> Result<()> {
    wait::until_one(app, what, HANG, step, ready).map_err(|error| {
        anyhow::anyhow!(
            "{error:#}; screens {:?}; tools {:?}; world bricks {:?}; pending {}; ghost {:?}; dialogs {:?}",
            app.ui.stack(),
            app.network_view()
                .and_then(|v| v.tools.get(&v.owner).cloned()),
            app.network_view().map(|v| v.world.bricks.len()),
            app.pending_requests(),
            app.building().and_then(|b| b.ghost()),
            app.ui.screen(ScreenId::MessageBox).map(|s| s
                .view()
                .walk()
                .map(|n| s.view().text_of(n))
                .collect::<Vec<_>>())
        )
    })
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
    // Wait for the copy itself, not a wall-clock deadline: a loaded machine
    // is slow, not wrong. The map callback has run once the wait returns.
    gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
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

/// Rain on its map, the drops counted and drawn, turned off and on in the
/// settings and cleared on disconnect, and none on a map without weather:
/// v20's Storm and Slopes (with their authored drop counts) and Bedroom, or
/// the made-up root's rainy map ([`RAIN`]) and its first room.
fn native_weather_map_settings_render_and_disconnect(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("native-client-weather")?;
    let state_dir = f.state()?;
    let state = state_dir.path();
    let mut app = App::load(&f.root, state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    let gpu = support::gpu::turn()?;
    let mut ui_renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let mut reports = vec![];
    let maps = if f.content {
        vec![
            (
                "storm",
                "v20/add-ons/map_slate_storm_revised/slatestormrevised.mis",
                5000,
            ),
            ("slopes", "v20/add-ons/map_slopes/slopes.mis", 500),
            ("bedroom", BEDROOM, 0),
        ]
    } else {
        vec![("rain", RAIN.0, RAIN.1 as usize), ("dry", f.map.0.as_str(), 0)]
    };
    for (name, map, drops) in maps {
        action(
            &mut app,
            UiAction::HostGame {
                map: map.into(),
                mode: ServerMode::SinglePlayer,
                game_mode: None,
                max_players: 1,
                server_name: name.into(),
                password: String::new(),
                admin_password: String::new(),
                super_admin_password: String::new(),
            },
        )?;
        until(&mut app, "weather map and player", |a| {
            a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner))
        })?;
        action(
            &mut app,
            UiAction::Game(GameAction::Look {
                yaw: 0.,
                pitch: 0.4,
            }),
        )?;
        for _ in 0..50 {
            step(&mut app, Duration::from_millis(32))?;
        }
        assert_eq!(app.weather_counts().0, drops);
        let diagnostics = app.weather_diagnostics();
        assert_eq!(diagnostics.invalid_hits, 0);
        if drops > 0 {
            assert!(diagnostics.collision_queries >= drops as u64);
        }
        let wet = capture(&mut app, &gpu, &mut ui_renderer, false)?;
        image::save_buffer(
            artifact.join(format!("{name}.png")),
            &wet,
            SIZE.0,
            SIZE.1,
            image::ColorType::Rgba8,
        )?;
        let mut saved = app.ui.settings();
        saved
            .prefs
            .insert("$pref::precipitationOn".into(), "0".into());
        action(&mut app, UiAction::SaveSettings(Box::new(saved.clone())))?;
        assert_eq!(app.weather_counts(), (0, 0));
        // No simulation tick between these renders: only precipitation changed.
        let dry = capture(&mut app, &gpu, &mut ui_renderer, false)?;
        let changed = wet
            .chunks_exact(4)
            .zip(dry.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        if drops > 0 {
            ensure!(
                changed > 10,
                "{name}: weather did not affect rendered pixels ({changed})"
            );
        } else {
            assert_eq!(changed, 0);
        }
        saved
            .prefs
            .insert("$pref::precipitationOn".into(), "1".into());
        action(&mut app, UiAction::SaveSettings(Box::new(saved)))?;
        for _ in 0..4 {
            step(&mut app, Duration::from_millis(32))?;
        }
        assert_eq!(app.weather_counts().0, drops);
        action(&mut app, UiAction::Disconnect)?;
        assert_eq!(app.weather_counts(), (0, 0));
        reports.push(serde_json::json!({"map":map,"authored_drops":drops,"changed_pixels":changed,"diagnostics":diagnostics}));
    }
    std::fs::write(
        artifact.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "maps":reports,"visible_window":false,"audio_device":false,"adapter":gpu.adapter_info.name,
            "settings_off_on":true,"disconnect_clears":true
        }))?,
    )?;
    app.gpu_stopped();
    Ok(())
}
