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
//! Stacked saves: `BRI_PERF_SAVE="a.bls;b.bls;c.bls" BRI_PERF_MAP=Bedroom`
//! loads them all onto one map. Each view reports GPU ms per world pass
//! (`gpu_passes`) beside the frame's whole GPU time.
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
    // Look turns by its amount times FOV / 90, as the mouse does.
    let scale = 90.0 / app.controls.fov();
    request(
        app,
        GameAction::Look {
            yaw: delta * scale,
            pitch: (current_pitch - pitch) * scale,
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
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
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

/// Hollow towers of 1x1 bricks (8x8 studs, 20 bricks tall, one colour each,
/// about one in twenty translucent) on a grid centred on the origin.
fn synthetic_world(count: usize, map_id: &str, palette: Vec<[f32; 4]>) -> bri_world::World {
    const SIDE: usize = 8;
    const LAYERS: usize = 20;
    let (clear, solid): (Vec<usize>, Vec<usize>) =
        (0..palette.len()).partition(|&i| palette[i][3] < 1.0);
    let mut world = bri_world::World::new(format!("Synthetic {count}"), map_id.into(), palette);
    let ring: Vec<(usize, usize)> = (0..SIDE)
        .flat_map(|i| (0..SIDE).map(move |j| (i, j)))
        .filter(|&(i, j)| i == 0 || j == 0 || i == SIDE - 1 || j == SIDE - 1)
        .collect();
    let towers = count.div_ceil(ring.len() * LAYERS);
    let grid = (towers as f32).sqrt().ceil() as usize;
    let spacing = 6.0;
    let origin = -(grid as f32) * spacing * 0.5;
    let mut id = 1u64;
    'city: for t in 0..towers {
        let (tx, tz) = (t % grid, t / grid);
        let color = if t % 20 == 7 && !clear.is_empty() {
            clear[t / 20 % clear.len()]
        } else {
            solid[t % solid.len().max(1)]
        };
        for layer in 0..LAYERS {
            for &(i, j) in &ring {
                if id as usize > count {
                    break 'city;
                }
                let mut brick = bri_world::Brick::new(
                    bri_world::ContentRef::Resolved("v20/brick/brick1x1data".into()),
                    [
                        origin + tx as f32 * spacing + 0.25 + 0.5 * i as f32,
                        0.3 + 0.6 * layer as f32,
                        origin + tz as f32 * spacing + 0.25 + 0.5 * j as f32,
                    ],
                    0,
                );
                brick.color = color as u8;
                world.bricks.insert(id, brick);
                id += 1;
            }
        }
    }
    world.next_brick_id = id;
    world
}

const PART_BRICKS: usize = 50_000;

/// Load one saved part and wait until the world holds `total` bricks.
fn load_part(
    app: &mut App,
    folder: &str,
    name: &str,
    first: bool,
    total: usize,
    stacked: bool,
) -> Result<Duration> {
    let start = Instant::now();
    app.ui.core.request(UiAction::LoadBricks {
        map: folder.into(),
        name: name.into(),
        ownership: true,
    });
    pump(app)?;
    if first {
        // The save's colours differ from the map's: keep them (Append), as
        // Load Bricks' colour check offers.
        until(app, "the colour check", Duration::from_secs(300), |a| {
            a.ui.screen(bri_ui::screens::ScreenId::LoadBricksColor)
                .is_some()
                || a.network_view().is_some_and(|v| !v.world.bricks.is_empty())
        })?;
        if app
            .ui
            .screen(bri_ui::screens::ScreenId::LoadBricksColor)
            .is_some()
        {
            app.ui
                .core
                .request(UiAction::LoadBricksColors(bri_ui::api::ColorLoad::Append));
            pump(app)?;
            app.ui.core.pop(bri_ui::screens::ScreenId::LoadBricksColor);
        }
    }
    // Stacked saves may overlap, and the host refuses overlapping bricks:
    // there the part is done once the count stops growing for 20 s, and
    // the load took until its last brick arrived, not those 20 s.
    let last = std::cell::Cell::new((0usize, Instant::now()));
    let settled = std::cell::Cell::new(false);
    until(app, name, Duration::from_secs(900), |a| {
        let count = a.network_view().map_or(0, |v| v.world.bricks.len());
        if count != last.get().0 {
            last.set((count, Instant::now()));
        }
        settled.set(stacked && last.get().1.elapsed() > Duration::from_secs(20));
        (count + 16 >= total || settled.get())
            && a.world_render_ready()
            && a.pending_requests() == 0
    })?;
    Ok(if settled.get() {
        last.get().1 - start
    } else {
        start.elapsed()
    })
}

/// What a ghost change costs: the old path rebuilt the ghost's scene, with
/// its surface textures and their mip chains, and uploaded it; the cached
/// ghost only writes its one transform.
fn ghost_cost(content: &ClientContent, map_id: &str, gpu: &Gpu) -> Result<serde_json::Value> {
    use bri_render::scene::{GpuInstances, SceneRenderer, SceneTransform};
    let loaded = content.paths.load_map(map_id, None)?;
    let meshes: std::collections::BTreeMap<String, _> = loaded
        .simulation
        .definitions
        .entries
        .iter()
        .map(|(id, d)| (id.clone(), d.mesh.clone()))
        .collect();
    let materials = bri_client::materials::BrickMaterials::load(&content.paths.brick_materials)?;
    let renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Bgra8Unorm);
    let world = bri_net::protocol::PublicWorld {
        name: "Ghost".into(),
        map_id: map_id.into(),
        palette: loaded.simulation.state().palette.clone(),
        bricks: bri_world::Bricks::unit(
            0,
            bri_world::Brick::new(
                bri_world::ContentRef::Resolved("v20/brick/brick1x1data".into()),
                [0.25, 0.3, 0.25],
                0,
            ),
        ),
    };
    let wait = || {
        gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
    };
    let (mut rebuild, mut moved) = (vec![], vec![]);
    for _ in 0..30 {
        let t = Instant::now();
        let data = bri_client::world_scene::build_world_scene_materials(
            &world,
            &meshes,
            100_000,
            Some(&materials),
        )?;
        let _gpu = renderer.upload(&gpu.device, &gpu.queue, &data)?;
        gpu.queue.submit([]);
        wait()?;
        rebuild.push(ms(t.elapsed()));
    }
    let mut instances = GpuInstances::new(&gpu.device, 1)?;
    for i in 0..30 {
        let t = Instant::now();
        instances.update(
            &gpu.queue,
            &[SceneTransform {
                transform: glam::Mat4::from_translation(Vec3::X * i as f32),
                tint: [1.0; 4],
            }],
        )?;
        gpu.queue.submit([]);
        wait()?;
        moved.push(ms(t.elapsed()));
    }
    Ok(json!({ "rebuild": stats(&mut rebuild), "move": stats(&mut moved) }))
}

/// Save the last rendered frame (BGRA) as `path` (RGBA PNG).
fn screenshot(gpu: &Gpu, target: &wgpu::Texture, path: &Path) -> Result<()> {
    let (width, height) = (target.width(), target.height());
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("perf screenshot"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        target.size(),
    );
    gpu.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        for bgra in line[..width as usize * 4].chunks_exact(4) {
            pixels.extend_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
        }
    }
    image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)?;
    Ok(())
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
    // GPU ms per world pass, in frame order.
    let mut passes: Vec<(&'static str, Vec<f64>)> = Vec::new();
    app.time_gpu_passes(true);
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
                for &(pass, time) in app.gpu_pass_times() {
                    match passes.iter_mut().find(|(p, _)| *p == pass) {
                        Some((_, times)) => times.push(f64::from(time)),
                        None => passes.push((pass, vec![f64::from(time)])),
                    }
                }
            }
        }
        last_gpu = measured;
    }
    let passes: serde_json::Map<String, serde_json::Value> = passes
        .iter_mut()
        .map(|(pass, times)| ((*pass).to_string(), stats(times)))
        .collect();
    Ok(json!({
        "render": app.render_stats(),
        "entities": app.entity_counts(),
        "gpu_passes": passes,
        "update": stats(&mut update),
        "record": stats(&mut record),
        "gpu": stats(&mut gpu_ms),
        "frame": stats(&mut frame),
    }))
}

#[test]
#[ignore = "real GPU, converted content and a v20 save (BRI_PERF_SAVE); no window"]
fn large_build_frame_times() -> Result<()> {
    // BRI_PERF_SYNTHETIC=<bricks> builds a city of brick towers on Slate
    // instead of loading a save.
    let synthetic: Option<usize> = std::env::var("BRI_PERF_SYNTHETIC")
        .ok()
        .and_then(|s| s.parse().ok());
    // BRI_PERF_SAVE may list several saves separated by `;`: they load
    // together (the first save's colours), as a player stacks saves.
    let stacked: Vec<PathBuf> = match (synthetic, std::env::var("BRI_PERF_SAVE")) {
        (Some(n), _) => vec![PathBuf::from(format!("Slate/Synthetic {n}.bls"))],
        (None, Ok(list)) => list
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .collect(),
        (None, Err(_)) => {
            eprintln!("BRI_PERF_SAVE is not set; skipping");
            return Ok(());
        }
    };
    let save = stacked
        .first()
        .context("BRI_PERF_SAVE lists no save")?
        .clone();
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
    // BRI_PERF_MAP (a save folder name such as "Bedroom") loads every
    // save onto that map instead of the first save's own.
    let folder = match std::env::var("BRI_PERF_MAP") {
        Ok(map) => map,
        Err(_) => save
            .parent()
            .and_then(Path::file_name)
            .context("save folder")?
            .to_string_lossy()
            .to_string(),
    };
    let name = if stacked.len() > 1 {
        format!("{name} and {} more on {folder}", stacked.len() - 1)
    } else {
        name
    };
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
        if let Some(n) = synthetic.or((stacked.len() > 1).then_some(2_000_000)) {
            // This copy only: a server limit that admits the whole city.
            let path = state.join("settings.json");
            let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            value["settings"]["prefs"]["$Pref::Server::BrickLimit"] = json!(n.to_string());
            std::fs::write(&path, serde_json::to_vec(&value)?)?;
        }
    }

    // Convert as a dropped save converts, into the Load Bricks folder.
    let started = Instant::now();
    let content = ClientContent::load(&content_root)?;
    let converter = bri_client::old_saves::Converter::new(&content)?;
    let world = match synthetic {
        Some(n) => {
            // Any stock world's palette: v20's default colours.
            let stock = content
                .worlds
                .iter()
                .find(|w| w.loadable)
                .context("no stock world")?;
            let palette = content
                .paths
                .load_map(&stock.map_id, Some(&stock.id))?
                .simulation
                .state()
                .palette
                .clone();
            synthetic_world(n, map_id, palette)
        }
        None => {
            let mut world = converter.convert(&std::fs::read(&save)?, &name, map_id)?;
            for more in &stacked[1..] {
                let part = converter.convert(&std::fs::read(more)?, &name, map_id)?;
                for brick in part.bricks.values() {
                    world.bricks.insert(world.next_brick_id, brick.clone());
                    world.next_brick_id += 1;
                }
            }
            world
        }
    };
    let bricks = world.bricks.len();
    let lights = world.bricks.values().filter(|b| b.light.is_some()).count();
    let emitters = world
        .bricks
        .values()
        .filter(|b| b.emitter.is_some())
        .count();
    use sha2::Digest;
    let saves = state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&saves)?;
    // A native build file holds at most MAX_BUILD_BYTES (about 100k bricks
    // of JSON), so a larger world loads as parts, one after another.
    let mut parts = Vec::new();
    let ids: Vec<u64> = world.bricks.keys().copied().collect();
    for (n, slice) in ids.chunks(PART_BRICKS).enumerate() {
        let mut part = bri_world::World::new(
            world.name.clone(),
            world.map_id.clone(),
            world.palette.clone(),
        );
        for id in slice {
            part.bricks.insert(*id, world.bricks[id].clone());
        }
        part.next_brick_id = slice.last().map_or(1, |id| id + 1);
        let file = format!("bench-{n}.world.json");
        std::fs::write(
            saves.join(&file),
            serde_json::to_vec(&bri_world::build::SavedBuild::new(part))?,
        )?;
        parts.push(file);
    }
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
    eprintln!(
        "loading {bricks} bricks ({lights} lights, {emitters} emitters) in {} parts",
        parts.len()
    );
    let mut loaded = 0;
    let mut loading = Duration::ZERO;
    for (n, part) in parts.iter().enumerate() {
        let expected = (bricks - loaded).min(PART_BRICKS);
        loaded += expected;
        loading += load_part(&mut app, &folder, part, n == 0, loaded, stacked.len() > 1)?;
    }
    let load_ms = ms(loading);
    let placed = app.network_view().map_or(0, |v| v.world.bricks.len());

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
    let target_texture = target;
    let target = target_texture.create_view(&Default::default());
    let mut timer = GpuFrameTimer::new(&gpu.device, &gpu.queue);

    // Views: where the player spawned, then outside the build looking at
    // its middle, then in its middle.
    // Its middle 90% on each axis, so a few far-flung bricks of a stacked
    // save neither move the middle nor push the overview out into the fog.
    let (min, max) = {
        let view = app.network_view().context("view")?;
        let mut axes: [Vec<f32>; 3] = Default::default();
        for b in view.world.bricks.values() {
            for (axis, v) in axes.iter_mut().zip(b.position) {
                axis.push(v);
            }
        }
        let at = |axis: &mut Vec<f32>, p: f32| {
            axis.sort_by(f32::total_cmp);
            axis.get(((axis.len().max(1) - 1) as f32 * p) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let [x, y, z] = &mut axes;
        (
            Vec3::new(at(x, 0.05), at(y, 0.0), at(z, 0.05)),
            Vec3::new(at(x, 0.95), at(y, 0.95), at(z, 0.95)),
        )
    };
    let center = (min + max) * 0.5;
    let extent = (max - min).max(Vec3::splat(20.0));
    let mut report = serde_json::Map::new();
    // BRI_PERF_PROFILE=<view> (or 1 for the spawn view) samples that view.
    let profile_view = std::env::var("BRI_PERF_PROFILE")
        .ok()
        .map(|v| if v == "1" { "spawn".to_string() } else { v });
    let stem = name.replace(|c: char| !c.is_alphanumeric(), "_");
    let write_profile = |view: &str, profile: sampler::Profile| -> Result<()> {
        let text = profile.report(70);
        std::fs::create_dir_all(&out)?;
        std::fs::write(out.join(format!("{stem}-{view}-profile.txt")), &text)?;
        std::fs::write(
            out.join(format!("{stem}-{view}-profile.folded")),
            profile.folded(),
        )?;
        eprintln!("{text}");
        Ok(())
    };
    // Past the first frames, which upload the map and every chunk.
    frames(&mut app, &gpu, &mut ui, &mut timer, &target, size, 10)?;
    let sampling = (profile_view.as_deref() == Some("spawn")).then(sampler::Sampler::start);
    report.insert(
        "spawn".into(),
        frames(&mut app, &gpu, &mut ui, &mut timer, &target, size, count)?,
    );
    if let Some(sampling) = sampling {
        write_profile("spawn", sampling.finish())?;
    }
    screenshot(
        &gpu,
        &target_texture,
        &out.join(format!("{stem}-spawn.png")),
    )?;
    let look = |from: Vec3, to: Vec3| {
        let d = (to - from).normalize();
        (d.x.atan2(-d.z), d.y.asin())
    };
    for (view, eye) in [
        (
            "overview",
            // Above one corner of the build, near enough to stay inside the
            // map's fog.
            center
                + Vec3::new(1.0, 0.7, 1.0).normalize()
                    * (Vec3::new(extent.x, 0.0, extent.z).length() * 0.6).clamp(30.0, 150.0),
        ),
        ("inside", center + Vec3::new(0.0, 2.0, 0.0)),
    ] {
        let (yaw, pitch) = look(eye, center - Vec3::new(0.0, extent.y * 0.25, 0.0));
        camera_at(&mut app, eye, yaw, pitch)?;
        // Settle first, so the profile holds only steady frames.
        frames(&mut app, &gpu, &mut ui, &mut timer, &target, size, 10)?;
        let sampling = (profile_view.as_deref() == Some(view)).then(sampler::Sampler::start);
        report.insert(
            view.into(),
            frames(&mut app, &gpu, &mut ui, &mut timer, &target, size, count)?,
        );
        if let Some(sampling) = sampling {
            write_profile(view, sampling.finish())?;
        }
        screenshot(
            &gpu,
            &target_texture,
            &out.join(format!("{stem}-{view}.png")),
        )?;
    }
    app.gpu_stopped();
    let ghost = ghost_cost(&content, map_id, &gpu)?;
    let result = json!({
        "ghost_change": ghost,
        "save": name,
        "map": map_id,
        "bricks": bricks,
        "bricks_placed": placed,
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
