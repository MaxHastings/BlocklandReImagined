//! Large-build frame-time benchmark on the normal App: hosts the save's map in
//! single player, loads a v20 `.bls` the way a dropped save converts, and
//! renders frames offscreen at 1440p with the player's own graphics settings.
//! Times come from the game's own instruments: the CPU spans the platform
//! loop measures (update, recording) and `GpuFrameTimer`'s timestamps.
//! Never opens a window or sends input.
//!
//! Run (release, real GPU):
//!   BRI_PERF_SAVE="…/saves/Kitchen/Badspot's Birth Day.bls" \
//!   BRI_PERF_SETTINGS="%LOCALAPPDATA%/BlocklandReImagined/settings.json" \
//!   cargo test -p bri-client --release --test large_build_perf -- --ignored --nocapture
//! Optional: BRI_PERF_OUT (report folder), BRI_PERF_SIZE ("2560x1440"),
//! BRI_PERF_FRAMES (per view), BRI_PERF_MAX_MS (fail above this p95 frame).
#[path = "support/sampler.rs"]
mod sampler;
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    content::ClientContent,
    perf::GpuFrameTimer,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{api::*, gpu::UiRenderer};
use glam::Vec3;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

fn pump(app: &mut App) -> Result<()> {
    ensure!(app.pump()?.is_empty(), "Unexpected native window command");
    Ok(())
}
fn step(app: &mut App, dt: Duration) -> Result<()> {
    app.tick(dt)?;
    app.ui.update(dt.as_millis() as u64);
    pump(app)
}
fn until(app: &mut App, what: &str, timeout: Duration, ready: impl Fn(&App) -> bool) -> Result<()> {
    let start = Instant::now();
    let mut previous = start;
    let mut previous_report = 0;
    loop {
        let now = Instant::now();
        step(app, now.duration_since(previous))?;
        previous = now;
        if ready(app) {
            return Ok(());
        }
        if start.elapsed().as_secs() / 10 != previous_report {
            previous_report = start.elapsed().as_secs() / 10;
            eprintln!(
                "{what}: {:.0} s, bricks {:?}, render ready {}, pending {}, chat {:?}",
                start.elapsed().as_secs_f32(),
                app.network_view().map(|v| v.world.bricks.len()),
                app.world_render_ready(),
                app.pending_requests(),
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
        }
        ensure!(
            start.elapsed() < timeout,
            "Timed out waiting for {what}: {:?}",
            app.ui.core.conn
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn request(app: &mut App, action: GameAction) -> Result<()> {
    app.ui.core.request(UiAction::Game(action));
    pump(app)
}
fn camera_at(app: &mut App, eye: Vec3, yaw: f32, pitch: f32) -> Result<()> {
    if app.controls.free_camera().is_none() {
        request(app, GameAction::DropCameraAtPlayer)?;
        until(app, "free camera", Duration::from_secs(20), |a| {
            a.controls.free_camera().is_some()
        })?;
    }
    app.controls.redrop_camera(eye);
    let (current_yaw, current_pitch) = app.controls.camera_angles();
    let delta = (yaw - current_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    request(
        app,
        GameAction::Look {
            yaw: delta,
            pitch: current_pitch - pitch,
        },
    )
}

fn stats(samples: &mut [f64]) -> serde_json::Value {
    if samples.is_empty() {
        return json!(null);
    }
    samples.sort_by(f64::total_cmp);
    let at = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
    json!({
        "mean_ms": samples.iter().sum::<f64>() / samples.len() as f64,
        "p50_ms": at(0.5), "p95_ms": at(0.95), "p99_ms": at(0.99), "max_ms": at(1.0),
    })
}
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    name: String,
}
/// The game's adapter choice: the high-performance GPU with timestamps.
fn gpu() -> Result<Gpu> {
    // The platform tries every primary backend except Vulkan first (DX12 on
    // Windows); WGPU_BACKEND overrides, as it does in the game.
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends =
        wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY & !wgpu::Backends::VULKAN);
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .context("no GPU")?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: bri_client_sandbox::gpu::timing_features(&adapter),
        ..Default::default()
    }))?;
    Ok(Gpu {
        device,
        queue,
        name: format!("{} ({:?})", info.name, info.backend),
    })
}

/// Frames as the platform loop runs them: update, record the scene and the
/// HUD, submit, then wait for the GPU as a synchronized present would.
fn frames(
    app: &mut App,
    gpu: &Gpu,
    ui: &mut UiRenderer,
    timer: &mut Option<GpuFrameTimer>,
    target: &wgpu::TextureView,
    size: (u32, u32),
    count: usize,
) -> Result<serde_json::Value> {
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let (mut update, mut record, mut gpu_ms, mut frame) = (vec![], vec![], vec![], vec![]);
    let mut previous = Instant::now();
    let mut last_gpu = None;
    for i in 0..count + 20 {
        let start = Instant::now();
        step(app, start.duration_since(previous))?;
        previous = start;
        let updated = Instant::now();
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        if let Some(t) = timer.as_mut() {
            t.begin(&mut encoder);
        }
        ensure!(
            app.render_scene(&mut RenderContext {
                device: &gpu.device,
                queue: &gpu.queue,
                encoder: &mut encoder,
                target,
                format,
                size,
                ui_renderer: ui,
            })?,
            "App did not render the world"
        );
        let hud = app.ui();
        ui.render(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            target,
            format,
            size,
            hud.scale(),
            &hud.core.pack,
            &hud.draw(),
            None,
        );
        if let Some(t) = timer.as_mut() {
            t.end(&mut encoder);
        }
        gpu.queue.submit([encoder.finish()]);
        let recorded = Instant::now();
        gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })?;
        let done = Instant::now();
        let measured = timer.as_mut().and_then(|t| t.collect(&gpu.device));
        if i >= 20 {
            update.push(ms(updated - start));
            record.push(ms(recorded - updated));
            frame.push(ms(done - start));
            if measured != last_gpu
                && let Some(g) = measured
            {
                gpu_ms.push(ms(g));
            }
        }
        last_gpu = measured;
    }
    Ok(json!({
        "render": app.render_stats(),
        "update": stats(&mut update),
        "record": stats(&mut record),
        "gpu": stats(&mut gpu_ms),
        "frame": stats(&mut frame),
    }))
}

#[test]
#[ignore = "real GPU, converted content and a v20 save (BRI_PERF_SAVE); no window"]
fn large_build_frame_times() -> Result<()> {
    let Some(save) = std::env::var_os("BRI_PERF_SAVE").map(PathBuf::from) else {
        eprintln!("BRI_PERF_SAVE is not set; skipping");
        return Ok(());
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let content_root =
        std::env::var_os("BRI_CONTENT").map_or_else(|| workspace.join("content"), PathBuf::from);
    let size = std::env::var("BRI_PERF_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((2560u32, 1440u32));
    let count: usize = std::env::var("BRI_PERF_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let name = save
        .file_stem()
        .context("save name")?
        .to_string_lossy()
        .to_string();
    let folder = save
        .parent()
        .and_then(Path::file_name)
        .context("save folder")?
        .to_string_lossy()
        .to_string();
    let map_id =
        bri_client::content::map_for_save_folder(&folder).context("save is not in a map folder")?;
    let out = std::env::var_os("BRI_PERF_OUT").map_or_else(
        || workspace.join("artifacts/large-build-perf"),
        PathBuf::from,
    );
    let state = out.join(format!("state-{}", std::process::id()));
    std::fs::create_dir_all(&state)?;
    if let Some(settings) = std::env::var_os("BRI_PERF_SETTINGS") {
        std::fs::copy(&settings, state.join("settings.json"))?;
    }

    // Convert as a dropped save converts, into the Load Bricks folder.
    let started = Instant::now();
    let content = ClientContent::load(&content_root)?;
    let converter = bri_client::old_saves::Converter::new(&content)?;
    let world = converter.convert(&std::fs::read(&save)?, &name, map_id)?;
    let bricks = world.bricks.len();
    let lights = world.bricks.values().filter(|b| b.light.is_some()).count();
    let emitters = world
        .bricks
        .values()
        .filter(|b| b.emitter.is_some())
        .count();
    drop(content);
    use sha2::Digest;
    let saves = state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&saves)?;
    std::fs::write(
        saves.join("bench.world.json"),
        serde_json::to_vec(&bri_world::build::SavedBuild::new(world))?,
    )?;
    let convert_ms = ms(started.elapsed());

    let mut app = App::load(&content_root, &state, size)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: map_id.into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Large build perf".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    pump(&mut app)?;
    until(&mut app, "in game", Duration::from_secs(180), |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner))
    })?;
    let loading = Instant::now();
    let map_name = app
        .ui
        .core
        .save_files
        .iter()
        .find(|f| f.name == "bench.world.json")
        .map(|f| f.map.clone())
        .unwrap_or(folder.clone());
    eprintln!("loading {bricks} bricks ({lights} lights, {emitters} emitters) under {map_name:?}");
    app.ui.core.request(UiAction::LoadBricks {
        map: map_name,
        name: "bench.world.json".into(),
        ownership: true,
    });
    pump(&mut app)?;
    // The save's colours differ from the map's: keep them (Append), as
    // Load Bricks' colour check offers.
    until(
        &mut app,
        "the colour check",
        Duration::from_secs(120),
        |a| {
            a.ui.screen(bri_ui::screens::ScreenId::LoadBricksColor)
                .is_some()
                || a.network_view().is_some_and(|v| !v.world.bricks.is_empty())
        },
    )?;
    if app
        .ui
        .screen(bri_ui::screens::ScreenId::LoadBricksColor)
        .is_some()
    {
        app.ui
            .core
            .request(UiAction::LoadBricksColors(bri_ui::api::ColorLoad::Append));
        pump(&mut app)?;
    }
    until(&mut app, "the build", Duration::from_secs(600), |a| {
        a.network_view()
            .is_some_and(|v| v.world.bricks.len() + 16 >= bricks)
            && a.world_render_ready()
            && a.pending_requests() == 0
    })?;
    let load_ms = ms(loading.elapsed());

    let gpu = gpu()?;
    let mut ui = UiRenderer::new(&gpu.device, &gpu.queue);
    let format = wgpu::TextureFormat::Bgra8Unorm;
    app.gpu_ready(&gpu.device, &gpu.queue, format)?;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("perf frame"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target = target.create_view(&Default::default());
    let mut timer = GpuFrameTimer::new(&gpu.device, &gpu.queue);

    // Views: where the player spawned, then outside the build looking at
    // its middle, then in its middle.
    let (min, max) = app
        .network_view()
        .context("view")?
        .world
        .bricks
        .values()
        .fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), b| {
                (
                    lo.min(Vec3::from(b.position)),
                    hi.max(Vec3::from(b.position)),
                )
            },
        );
    let center = (min + max) * 0.5;
    let extent = (max - min).max(Vec3::splat(20.0));
    let mut report = serde_json::Map::new();
    let profiling = std::env::var_os("BRI_PERF_PROFILE").is_some();
    // Past the first frames, which upload the map and every chunk.
    frames(&mut app, &gpu, &mut ui, &mut timer, &target, size, 10)?;
    let sampling = profiling.then(sampler::Sampler::start);
    report.insert(
        "spawn".into(),
        frames(&mut app, &gpu, &mut ui, &mut timer, &target, size, count)?,
    );
    if let Some(sampling) = sampling {
        let profile = sampling.finish();
        let text = profile.report(70);
        let stem = name.replace(|c: char| !c.is_alphanumeric(), "_");
        std::fs::create_dir_all(&out)?;
        std::fs::write(out.join(format!("{stem}-profile.txt")), &text)?;
        std::fs::write(out.join(format!("{stem}-profile.folded")), profile.folded())?;
        eprintln!("{text}");
    }
    let look = |from: Vec3, to: Vec3| {
        let d = (to - from).normalize();
        (d.x.atan2(-d.z), d.y.asin())
    };
    for (view, eye) in [
        (
            "overview",
            center + Vec3::new(extent.x * 0.7, extent.y * 0.6 + 10.0, extent.z * 0.7),
        ),
        ("inside", center + Vec3::new(0.0, 2.0, 0.0)),
    ] {
        let (yaw, pitch) = look(eye, center - Vec3::new(0.0, extent.y * 0.25, 0.0));
        camera_at(&mut app, eye, yaw, pitch)?;
        report.insert(
            view.into(),
            frames(&mut app, &gpu, &mut ui, &mut timer, &target, size, count)?,
        );
    }
    app.gpu_stopped();
    let result = json!({
        "save": name,
        "map": map_id,
        "bricks": bricks,
        "light_bricks": lights,
        "emitter_bricks": emitters,
        "adapter": gpu.name,
        "resolution": [size.0, size.1],
        "convert_ms": convert_ms,
        "load_ms": load_ms,
        "views": report,
    });
    let path = out.join(format!(
        "{}.json",
        name.replace(|c: char| !c.is_alphanumeric(), "_")
    ));
    std::fs::write(&path, serde_json::to_vec_pretty(&result)?)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    let _ = std::fs::remove_dir_all(&state);
    if let Some(limit) = std::env::var("BRI_PERF_MAX_MS")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
    {
        for (view, v) in &result["views"].as_object().context("views")?.clone() {
            let p95 = v["frame"]["p95_ms"].as_f64().unwrap_or(f64::MAX);
            ensure!(
                p95 <= limit,
                "{view}: p95 frame {p95:.2} ms is over {limit} ms"
            );
        }
    }
    Ok(())
}
