//! Explicit local-content integration probe. Never creates a window or OS input.
//! Run: cargo test -p bri-client --test app_flow --release -- --ignored --nocapture
use anyhow::{Context, Result, bail, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
    settings,
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    screens::ScreenId,
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

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
            app.network_view().and_then(|v| v.tools.get(&v.owner).cloned()),
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

fn host(app: &mut App, name: &str) -> RequestId {
    app.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::SinglePlayer, game_mode: None,
        max_players: 1,
        server_name: name.into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
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

#[test]
#[ignore = "native Storm/Slopes maps, loopback host and offscreen GPU; no window or audio device"]
fn native_weather_map_settings_render_and_disconnect() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifact = workspace.join("artifacts/native-client-weather");
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let state = artifact.join(format!("state-{stamp}"));
    std::fs::create_dir_all(&state)?;
    let mut app = App::load(&workspace.join("content"), &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    let gpu = Headless::new()?;
    let mut ui_renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let mut reports = vec![];
    for (name, map, drops) in [
        (
            "storm",
            "v20/add-ons/map_slate_storm_revised/slatestormrevised.mis",
            5000,
        ),
        ("slopes", "v20/add-ons/map_slopes/slopes.mis", 500),
        ("bedroom", BEDROOM, 0),
    ] {
        action(
            &mut app,
            UiAction::HostGame {
                map: map.into(),
                mode: ServerMode::SinglePlayer, game_mode: None,
                max_players: 1,
                server_name: name.into(),
                password: String::new(),
                admin_password: String::new(),
                super_admin_password: String::new(),
            },
        )?;
        until(
            &mut app,
            "weather map and player",
            Duration::from_secs(45),
            |a| {
                a.network_view()
                    .is_some_and(|v| v.poses.contains_key(&v.owner))
            },
        )?;
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

#[test]
#[ignore = "requires converted native content, loopback QUIC and an offscreen GPU; no window"]
fn native_host_cancel_rehost_chat_compositor_disconnect_and_settings() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifact = workspace.join("artifacts/native-client-flow");
    let run_id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let state = artifact.join(format!("state-{}-{run_id}", std::process::id()));
    std::fs::create_dir_all(&state)?;
    let started = Instant::now();
    let mut app = App::load(&workspace.join("content"), &state, SIZE)?;

    // Keep stock generated binds while completing the first-run setup in memory.
    // Every application-side operation still crosses the same typed request queue.
    let initial_settings = app.ui.settings();
    app.ui
        .core
        .request(UiAction::SaveSettings(Box::new(initial_settings.clone())));
    pump(&mut app)?;
    assert_eq!(
        settings::load(&state.join("settings.json"))?,
        initial_settings
    );
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.update(0);

    let cancelled = host(&mut app, "Cancelled host must never enter");
    pump(&mut app)?;
    assert!(matches!(app.ui.core.conn, ConnectionState::Loading { .. }));
    app.ui.core.request(UiAction::CancelConnect);
    pump(&mut app)?;
    app.ui.update(0);
    assert!(matches!(app.ui.core.conn, ConnectionState::Idle));
    assert_eq!(app.ui.session_request(), None);

    // These three actions deliberately reach App::pump in one batch. Skipping
    // the stale host must not let its queued cancellation clear the new token.
    let queued_stale = host(&mut app, "Unpumped stale host");
    app.ui.core.request(UiAction::CancelConnect);
    let active = host(&mut app, "Native headless Bedroom");
    assert_ne!(active, cancelled);
    pump(&mut app)?;
    // A late callback carrying the cancelled token cannot poison the new loader.
    assert!(!app.ui.apply_session(
        cancelled,
        UiUpdate::Connection(ConnectionState::Failed {
            reason: "deliberately stale loader callback".into(),
        })
    ));
    until(
        &mut app,
        "Bedroom network host and player replica",
        Duration::from_secs(45),
        |a| {
            matches!(&a.ui.core.conn, ConnectionState::InGame { server_name, .. } if server_name == "Native headless Bedroom")
                && a.network_view()
                    .is_some_and(|v| v.poses.contains_key(&v.owner))
        },
    )?;
    let entered_ms = started.elapsed().as_millis();
    let (placements, foliage_load_ms) = app.foliage_placement();
    let foliage_placement = placements.to_vec();
    assert_eq!(placements.iter().map(|p| p.requested).sum::<u32>(), 41000);
    assert_eq!(placements.iter().map(|p| p.placed).sum::<u32>(), 41000);
    assert_eq!(app.ui.session_request(), Some(active));
    assert_eq!(app.ui.content.id(), ScreenId::Play);
    assert_eq!(app.ui.core.players.len(), 1);
    assert_eq!(app.network_view().unwrap().world.map_id, BEDROOM);

    let before = app.network_view().unwrap().poses[&app.network_view().unwrap().owner].clone();
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Forward,
        down: true,
    }));
    pump(&mut app)?;
    until(
        &mut app,
        "authoritative horizontal movement",
        Duration::from_secs(8),
        |a| {
            let view = a.network_view().unwrap();
            let pose = &view.poses[&view.owner];
            let dx = pose.player.feet[0] - before.player.feet[0];
            let dz = pose.player.feet[2] - before.player.feet[2];
            pose.tick > before.tick && dx * dx + dz * dz > 0.01
        },
    )?;
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Forward,
        down: false,
    }));
    pump(&mut app)?;
    let after = app.network_view().unwrap().poses[&app.network_view().unwrap().owner].clone();
    assert!(!app.controls.held(HeldControl::Forward));

    let chat = format!("Native loopback proof {run_id} <tag>");
    app.ui.core.request(UiAction::Chat {
        channel: ChatChannel::Say,
        text: chat.clone(),
    });
    pump(&mut app)?;
    until(
        &mut app,
        "server chat roundtrip and HUD delivery",
        Duration::from_secs(8),
        |a| {
            a.network_view()
                .is_some_and(|v| v.chat.iter().any(|line| line.text == chat))
                && a.ui
                    .core
                    .chat
                    .lines
                    .iter()
                    .any(|line| line.text.contains(&format!("{run_id} ‹tag›")))
        },
    )?;
    let authoritative_chat = app.network_view().unwrap().chat.clone();
    let hud_chat = app
        .ui
        .core
        .chat
        .lines
        .iter()
        .map(|l| l.text.clone())
        .collect::<Vec<_>>();
    assert_eq!(app.ui.session_request(), Some(active));

    let gpu = Headless::new().context("offscreen native-client-flow adapter")?;
    let mut ui_renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    until(
        &mut app,
        "player grounded before building probe",
        Duration::from_secs(8),
        |a| {
            let view = a.network_view().unwrap();
            view.poses[&view.owner].player.grounded
        },
    )?;
    // Local ghost deployment does not wait for an authoritative aim round trip.
    action(
        &mut app,
        UiAction::Game(GameAction::Look {
            yaw: std::f32::consts::PI,
            pitch: 1.0,
        }),
    )?;
    let print_brick = "v20/brick/brick2x2fprintdata";
    action(
        &mut app,
        UiAction::InstantUseBrick {
            brick: print_brick.into(),
        },
    )?;
    action(
        &mut app,
        UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down: true,
        }),
    )?;
    let ghost = app
        .building()
        .and_then(|b| b.ghost())
        .context("Brick Fire did not deploy ghost")?
        .clone();
    assert!(
        ghost.print.is_some(),
        "Printable ghost lost original default Letters/A"
    );
    assert!(
        app.network_view().unwrap().world.bricks.is_empty(),
        "Ghost must remain unplanted locally"
    );
    let ghost_frame = capture(&mut app, &gpu, &mut ui_renderer, true)?;
    assert_eq!(app.foliage_stats().sources, 41000);
    assert!(app.foliage_stats().draw_calls <= 2);
    let placement_before_gpu_restart = app.foliage_placement().0.to_vec();
    app.gpu_stopped();
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let _recreated_frame = capture(&mut app, &gpu, &mut ui_renderer, false)?;
    assert_eq!(app.foliage_stats().sources, 41000);
    assert_eq!(app.foliage_placement().0, placement_before_gpu_restart);
    image::save_buffer(
        artifact.join("bedroom-ghost.png"),
        &ghost_frame,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    action(&mut app, UiAction::Game(GameAction::PlantBrick))?;
    until(
        &mut app,
        "authoritative plant and material render snapshot",
        Duration::from_secs(8),
        |a| {
            a.network_view().unwrap().world.bricks.len() == 1
                && a.world_render_ready()
                && a.pending_requests() == 0
        },
    )?;
    let (&planted_id, planted) = app
        .network_view()
        .unwrap()
        .world
        .bricks
        .iter()
        .next()
        .unwrap();
    let planted = planted.clone();
    assert_eq!(planted.position, ghost.position);
    assert_eq!(planted.print, ghost.print);
    action(&mut app, UiAction::Game(GameAction::CancelBrick))?;

    // Cancel an in-flight inspection in the same action batch. Its server reply
    // may arrive, but cannot reopen a stale wrench dialog.
    app.ui.core.request(UiAction::UseTool { slot: 1 });
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Fire,
        down: true,
    }));
    app.ui
        .core
        .request(UiAction::CancelWrench { brick: planted_id });
    pump(&mut app)?;
    until(
        &mut app,
        "cancelled wrench reply drained",
        Duration::from_secs(5),
        |a| a.pending_requests() == 0,
    )?;
    assert!(
        !app.ui
            .stack()
            .iter()
            .any(|s| matches!(s, ScreenId::Wrench(_)))
    );

    action(&mut app, UiAction::UseTool { slot: 2 })?;
    until(
        &mut app,
        "authoritative printer slot replicated",
        Duration::from_secs(5),
        |a| {
            let view = a.network_view().unwrap();
            view.tools.get(&view.owner).is_some_and(|tools| {
                tools.selected == Some(2)
                    && tools.slots[2].as_deref() == Some("v20.weapon.printgun")
            })
        },
    )?;
    action(
        &mut app,
        UiAction::Game(GameAction::Look {
            yaw: 1.0,
            pitch: 0.0,
        }),
    )?;
    let away_yaw = app.controls.yaw;
    until(
        &mut app,
        "server deliberately aimed away from print brick",
        Duration::from_secs(5),
        |a| {
            let view = a.network_view().unwrap();
            (view.poses[&view.owner].player.yaw - away_yaw).abs() < 0.01
        },
    )?;
    // Turn toward the brick, click, and turn away in one UI drain. The movement
    // channel receives only the final aim; the click must retain its own aim.
    app.ui.core.request(UiAction::Game(GameAction::Look {
        yaw: -1.0,
        pitch: 0.0,
    }));
    app.ui.core.request(UiAction::Game(GameAction::Held {
        control: HeldControl::Fire,
        down: true,
    }));
    app.ui.core.request(UiAction::Game(GameAction::Look {
        yaw: 1.0,
        pitch: 0.0,
    }));
    pump(&mut app)?;
    until(
        &mut app,
        "authoritative printer opens original selector",
        Duration::from_secs(5),
        |a| a.ui.stack().contains(&ScreenId::PrintSelector),
    )?;
    action(
        &mut app,
        UiAction::Game(GameAction::Look {
            yaw: -1.0,
            pitch: 0.0,
        }),
    )?;
    let print_b = app
        .ui
        .core
        .prints
        .iter()
        .filter(|(aspect, _)| aspect.eq_ignore_ascii_case("Letters"))
        .flat_map(|(_, prints)| prints)
        .find(|print| print.name == "B")
        .context("Original Letters/B unavailable")?
        .id
        .clone();
    action(
        &mut app,
        UiAction::SetPrint {
            print: print_b.clone(),
        },
    )?;
    until(
        &mut app,
        "print edit replicated and rendered",
        Duration::from_secs(5),
        |a| {
            a.network_view().unwrap().world.bricks[&planted_id].print
                == Some(bri_world::ContentRef::Resolved(print_b.clone()))
                && a.world_render_ready()
                && a.pending_requests() == 0
        },
    )?;
    action(&mut app, UiAction::ClosePrintSelector)?;
    app.ui.core.pop(ScreenId::PrintSelector);
    app.ui.update(0);

    action(&mut app, UiAction::UseTool { slot: 1 })?;
    action(
        &mut app,
        UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down: true,
        }),
    )?;
    until(
        &mut app,
        "authoritative wrench opens properties",
        Duration::from_secs(5),
        |a| {
            a.ui.stack()
                .contains(&ScreenId::Wrench(WrenchVariant::Normal))
        },
    )?;
    action(&mut app, UiAction::RequestEvents { brick: planted_id })?;
    until(
        &mut app,
        "wrench events inspection",
        Duration::from_secs(5),
        |a| a.ui.stack().contains(&ScreenId::WrenchEvents),
    )?;
    action(
        &mut app,
        UiAction::SendEvents {
            brick: planted_id,
            rows: vec![EventRow::Editable(EventLine {
                enabled: true,
                delay_ms: 0,
                input: "onActivate".into(),
                target: "Self".into(),
                named_target: None,
                output: "setColor".into(),
                params: vec![ParamValue::PaintColor(1)],
            })],
        },
    )?;
    until(
        &mut app,
        "event edit and retained base wrench context",
        Duration::from_secs(5),
        |a| {
            a.network_view().unwrap().world.bricks[&planted_id]
                .events
                .len()
                == 1
                && a.pending_requests() == 0
        },
    )?;
    app.ui.core.pop(ScreenId::WrenchEvents);
    app.ui.update(0);
    let mut properties = app.ui.core.wrench.values(WrenchVariant::Normal);
    properties.name = "NativeHeadlessProof".into();
    properties.light = Some("v20/light/redlight".into());
    properties.emitter = Some("v20/emitter/playerjetemitter".into());
    action(
        &mut app,
        UiAction::SendWrench {
            brick: planted_id,
            variant: WrenchVariant::Normal,
            data: properties,
        },
    )?;
    until(
        &mut app,
        "wrench name edit after events return",
        Duration::from_secs(5),
        |a| {
            a.network_view().unwrap().world.bricks[&planted_id]
                .name
                .as_deref()
                == Some("NativeHeadlessProof")
                && a.pending_requests() == 0
                && a.world_render_ready()
                && a.effect_counts().0 == 2
                && a.effect_counts().2 > 0
        },
    )?;
    app.ui.core.pop(ScreenId::Wrench(WrenchVariant::Normal));
    ensure!(
        app.audio_requests()
            .get("brick.plant")
            .copied()
            .unwrap_or(0)
            >= 1,
        "No authoritative plant audio intent"
    );
    ensure!(
        app.audio_requests()
            .get("wrenchHitSound")
            .copied()
            .unwrap_or(0)
            >= 1,
        "No authoritative wrench audio intent"
    );
    ensure!(
        app.audio_stats().started > 0 && app.audio_stats().non_finite_samples == 0,
        "Silent-output mixer failed"
    );
    ensure!(
        app.audio_warnings().is_empty(),
        "Audio diagnostics: {:?}",
        app.audio_warnings()
    );
    app.ui.update(0);
    // No tick between captures: their changed pixels must come from the HUD pass.
    let scene = capture(&mut app, &gpu, &mut ui_renderer, false)?;
    let composite = capture(&mut app, &gpu, &mut ui_renderer, true)?;
    let missing = ui_renderer
        .missing_textures()
        .map(|k| format!("{k:?}"))
        .collect::<Vec<_>>();
    assert!(missing.is_empty(), "Missing UI textures: {missing:?}");
    let scene_colors = scene
        .chunks_exact(4)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    ensure!(
        scene_colors > 128,
        "Scene is suspiciously uniform: {scene_colors} colors"
    );
    let hud_pixels = scene
        .chunks_exact(4)
        .zip(composite.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    ensure!(
        hud_pixels > 100,
        "HUD compositor did not affect the rendered scene"
    );
    image::save_buffer(
        artifact.join("bedroom-scene.png"),
        &scene,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    image::save_buffer(
        artifact.join("bedroom-hud.png"),
        &composite,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;

    let view = app.network_view().unwrap();
    let eye = view.poses[&view.owner]
        .player
        .eye(&bri_sim::player::PlayerTuning::default());
    let floor_camera = app
        .building()
        .unwrap()
        .camera_position(eye, glam::Vec3::Y, 8.0)?;
    let floor_camera_distance = eye.distance(floor_camera);
    ensure!(
        (0.1..4.0).contains(&floor_camera_distance),
        "Original Bedroom floor failed camera volume sweep: {floor_camera_distance}"
    );
    action(
        &mut app,
        UiAction::Game(GameAction::ToggleFirstPerson { fast: false }),
    )?;
    let mut outfit = app.ui.settings().avatar;
    outfit.set("Hat", "1");
    outfit.set("Accent", "1");
    outfit.set("FaceName", "smileyPirate1");
    outfit.set("DecalName", "Mod-Suit");
    action(&mut app, UiAction::SetAvatar(outfit.clone()))?;
    until(
        &mut app,
        "original outfit replicated",
        Duration::from_secs(5),
        |a| {
            let view = a.network_view().unwrap();
            view.avatars
                .get(&view.owner)
                .is_some_and(|appearance| appearance.face == "smileyPirate1")
                && a.pending_requests() == 0
        },
    )?;
    let third_person = capture(&mut app, &gpu, &mut ui_renderer, true)?;
    let avatar = app
        .avatar_scene(app.network_view().unwrap().owner)
        .context("Third-person avatar is missing")?;
    ensure!(
        !avatar.vertices.is_empty()
            && avatar
                .images
                .iter()
                .any(|i| i.label.ends_with("smileypirate1"))
            && avatar.images.iter().any(|i| i.label.ends_with("mod-suit")),
        "Original avatar material bindings missing"
    );

    ensure!(
        third_person != composite,
        "Third-person camera did not change the view"
    );
    image::save_buffer(
        artifact.join("bedroom-third-person.png"),
        &third_person,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    action(
        &mut app,
        UiAction::PreviewAvatar {
            avatar: outfit,
            camera_rotation: [0.3, 0.0, 2.52],
            orbit_distance: 4.34,
        },
    )?;
    app.ui.core.push(ScreenId::Avatar);
    step(&mut app, Duration::from_millis(16))?;
    let avatar_ui = capture(&mut app, &gpu, &mut ui_renderer, true)?;
    image::save_buffer(
        artifact.join("original-avatar-ui.png"),
        &avatar_ui,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    assert_eq!(
        app.network_view().unwrap().avatars[&app.network_view().unwrap().owner].face,
        "smileyPirate1",
        "Preview published an uncommitted appearance"
    );
    app.ui.core.pop(ScreenId::Avatar);
    action(
        &mut app,
        UiAction::Game(GameAction::ToggleFirstPerson { fast: false }),
    )?;

    let before_save = app
        .network_view()
        .unwrap()
        .world
        .bricks
        .values()
        .next()
        .unwrap()
        .clone();
    assert!(app.network_view().unwrap().administrator);
    action(
        &mut app,
        UiAction::SaveBricks {
            name: "Native round trip.world.json".into(),
            description: "Original print and named events".into(),
            events: true,
            ownership: true,
            overwrite: false,
        },
    )?;
    until(
        &mut app,
        "local authoritative build save",
        Duration::from_secs(10),
        |a| {
            a.pending_requests() == 0
                && a.ui
                    .core
                    .save_files
                    .iter()
                    .any(|f| f.name == "Native round trip.world.json")
        },
    )?;
    let store = bri_client::saves::Store::new(&state, &app.content);
    let saved_build = store.load("Bedroom", "Native round trip.world.json")?;
    assert_eq!(
        saved_build.world.bricks.values().next().unwrap(),
        &before_save
    );
    app.ui.core.push(ScreenId::LoadBricks);
    step(&mut app, Duration::from_millis(16))?;
    until(
        &mut app,
        "native save list refreshed",
        Duration::from_secs(5),
        |a| a.pending_requests() == 0,
    )?;
    let save_ui = capture(&mut app, &gpu, &mut ui_renderer, true)?;
    image::save_buffer(
        artifact.join("native-load-dialog.png"),
        &save_ui,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    app.ui.core.pop(ScreenId::LoadBricks);

    action(&mut app, UiAction::Game(GameAction::UndoBrick))?;
    until(
        &mut app,
        "authoritative plant undo removes render/query brick",
        Duration::from_secs(5),
        |a| {
            a.network_view().unwrap().world.bricks.is_empty()
                && a.world_render_ready()
                && a.pending_requests() == 0
                && a.effect_counts().0 == 0
                && a.effect_counts().1 == 0
        },
    )?;

    action(
        &mut app,
        UiAction::LoadBricks {
            map: "Bedroom".into(),
            name: "Native round trip.world.json".into(),
            ownership: true,
        },
    )?;
    until(
        &mut app,
        "native build appended and rendered",
        Duration::from_secs(10),
        |a| {
            a.pending_requests() == 0
                && a.network_view().unwrap().world.bricks.len() == 1
                && a.world_render_ready()
                && a.effect_counts().0 == 2
                && a.effect_counts().2 > 0
        },
    )?;
    assert_eq!(
        app.network_view()
            .unwrap()
            .world
            .bricks
            .values()
            .next()
            .unwrap(),
        &before_save
    );
    action(
        &mut app,
        UiAction::SaveBricks {
            name: "Native round trip.world.json".into(),
            description: "Confirmed overwrite".into(),
            events: true,
            ownership: true,
            overwrite: true,
        },
    )?;
    until(
        &mut app,
        "native overwrite completed",
        Duration::from_secs(10),
        |a| a.pending_requests() == 0,
    )?;
    assert_eq!(
        store
            .load("Bedroom", "Native round trip.world.json")?
            .world
            .description,
        vec!["Confirmed overwrite"]
    );

    let mut saved = app.ui.settings();
    saved
        .prefs
        .insert("$pref::Input::MouseSensitivity".into(), "1.375".into());
    saved.avatar.lan_name = "Native probe persisted player".into();
    app.ui
        .core
        .request(UiAction::SaveSettings(Box::new(saved.clone())));
    pump(&mut app)?;
    assert_eq!(settings::load(&state.join("settings.json"))?, saved);
    // Reading may complete after a disconnect/rehost. Its old session token
    // must prevent an append into the new server even though both are local.
    app.ui.core.request(UiAction::LoadBricks {
        map: "Bedroom".into(),
        name: "Native round trip.world.json".into(),
        ownership: true,
    });
    app.ui.core.request(UiAction::Disconnect);
    let fresh = host(&mut app, "Stale load protection");
    pump(&mut app)?;
    until(
        &mut app,
        "new session without stale loaded bricks",
        Duration::from_secs(15),
        |a| {
            a.ui.session_request() == Some(fresh)
                && a.network_view().is_some()
                && a.pending_requests() == 0
        },
    )?;
    assert!(app.network_view().unwrap().world.bricks.is_empty());
    app.ui.core.request(UiAction::Disconnect);
    pump(&mut app)?;
    app.ui.update(0);
    assert!(matches!(app.ui.core.conn, ConnectionState::Idle));
    assert_eq!(app.ui.content.id(), ScreenId::MainMenu);
    assert!(app.network_view().is_none());
    assert_eq!(app.effect_counts(), (0, 0, 0, 0));
    assert!(app.foliage_placement().0.is_empty());
    assert_eq!(app.foliage_stats().sources, 0);
    assert!(app.ui.core.chat.lines.is_empty());
    assert!(app.ui.core.players.is_empty());
    assert_eq!(
        app.controls.movement(),
        bri_sim::player::MoveInput::default()
    );
    assert!(!app.ui.apply_session(
        active,
        UiUpdate::Chat {
            text: "stale disconnected chat".into()
        }
    ));
    app.gpu_stopped();
    drop(app);
    let reopened = App::load(&workspace.join("content"), &state, SIZE)?;
    assert_eq!(reopened.ui.settings(), saved);
    assert!(!reopened.ui.stack().contains(&ScreenId::DefaultControls));

    let mut report = serde_json::json!({
        "schema_version": 1, "test": "actual App -> Worker -> loopback QUIC host -> replica -> scene + HUD",
        "visible_window_created": false, "os_input_sent": false,
        "map": BEDROOM, "cancelled_request": cancelled, "active_request": active,
        "queued_stale_request": queued_stale, "batched_host_cancel_rehost": true,
        "entered_ms": entered_ms, "elapsed_ms": started.elapsed().as_millis(),
        "adapter": {"name": gpu.adapter_info.name, "backend": format!("{:?}", gpu.adapter_info.backend)},
        "viewport": [SIZE.0, SIZE.1], "missing_ui_textures": missing,
        "scene_unique_colors": scene_colors, "hud_changed_pixels": hud_pixels,
        "authoritative_pose_before": before, "authoritative_pose_after": after,
        "deployed_ghost": ghost, "authoritative_planted_brick": planted,
        "print_changed_to": print_b, "wrench_name_saved": "NativeHeadlessProof",
        "event_saved_then_base_wrench_saved": true, "cancelled_inspection_ignored": true, "plant_undo_replicated": true,
        "authoritative_chat": authoritative_chat, "hud_chat": hud_chat,
        "settings_reloaded": true, "cancelled_callbacks_rejected": true,
        "native_local_save_reload_preserves_print_events_owner": true,
        "confirmed_save_overwrite": true, "load_dialog_rendered": true,
        "pending_load_cannot_cross_rehost": true,
        "disconnect_clears_session": true, "state_directory": state,
        "click_aim_survives_same_batch_look_away": true,
        "third_person_rendered": true, "original_avatar_materials_rendered": true, "avatar_preview_rendered": true, "original_floor_camera_sweep_distance": floor_camera_distance,
        "limitations": "Integration evidence only. Action aim is captured independently of later look/movement; server position is not rewound. Original avatar outfit/materials/preview are covered; movement transitions, tools/emotes, interpolation and exact visual fidelity remain pending. No interactive fidelity acceptance, audible playback, complete audio, weapon or LAN discovery coverage."
    });
    report["wrench_light_emitter_replicated_rendered_saved_reloaded_removed"] = true.into();
    report["authoritative_audio_cues_mixed_without_device"] = true.into();
    report["native_bedroom_foliage_placement"] = serde_json::to_value(foliage_placement)?;
    report["native_bedroom_foliage_load_ms"] = foliage_load_ms.into();
    std::fs::write(
        artifact.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Native app flow passed; report: {}",
        artifact.join("report.json").display()
    );
    Ok(())
}
