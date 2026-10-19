//! Frames of the real client, offscreen: tools/capture.py drives this (docs/capture.md). It loads a content root as the game does,
//! hosts a map and mode on loopback, plays a list of steps (chat commands, waits, held controls, look, view, tool slots) and writes a
//! PNG of what the player's screen draws, HUD included, at each `shot` step. A tool for seeing and debugging, not a gate: it runs
//! only with BRI_CAPTURE set to the spec file, and otherwise passes at once. Never opens a window; needs a GPU adapter.
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
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[macro_use]
mod support;
use support::wait;

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
/// The local player's body (archetype id).
fn body(app: &App) -> Option<String> {
    let view = app.network_view()?;
    let pose = view.poses.get(&view.owner)?;
    Some(view.archetypes.resolve(pose.player.archetype).id.clone())
}
fn run_for(app: &mut App, seconds: f32) -> Result<()> {
    wait::run_one_for(app, Duration::from_secs_f32(seconds), step)
}
fn capture(
    app: &mut App,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    size: (u32, u32),
) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: size.0,
        height: size.1,
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
            size,
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
        size,
        ui.scale(),
        &ui.core.pack,
        &ui.draw(),
        None,
    );
    let row = (size.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hud readback"),
        size: u64::from(row) * u64::from(size.1),
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
                rows_per_image: Some(size.1),
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
    let mut pixels = Vec::with_capacity((size.0 * size.1 * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..size.0 as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}
fn save(path: &Path, data: &[u8], size: (u32, u32)) -> Result<()> {
    image::save_buffer(path, data, size.0, size.1, image::ColorType::Rgba8)?;
    Ok(())
}

fn control(name: &str) -> Result<HeldControl> {
    Ok(match name {
        "forward" => HeldControl::Forward,
        "backward" => HeldControl::Backward,
        "left" => HeldControl::Left,
        "right" => HeldControl::Right,
        "jump" => HeldControl::Jump,
        "crouch" => HeldControl::Crouch,
        "jet" => HeldControl::Jet,
        "fire" => HeldControl::Fire,
        "walk" => HeldControl::Walk,
        "zoom" => HeldControl::Zoom,
        "free_look" => HeldControl::FreeLook,
        other => anyhow::bail!(
            "unknown control {other:?} (forward backward left right jump crouch jet fire walk zoom free_look)"
        ),
    })
}

/// The steps of a spec, one JSON object each: {"command": name, "args": [..]}, {"wait": seconds}, {"until": "landed"},
/// {"hold": control, "down": bool}, {"look": {"yaw": radians, "pitch": radians}}, {"third_person": true}, {"use_tool": slot},
/// {"shot": name}, {"burst": {"name": n, "count": c, "every_ms": ms}} (steps the game `every_ms` and captures `count` times).
fn play(
    app: &mut App,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    steps: &[serde_json::Value],
    out: &Path,
    size: (u32, u32),
    written: &mut Vec<String>,
) -> Result<()> {
    for (i, step) in steps.iter().enumerate() {
        let what = format!("step {i} {step}");
        if let Some(name) = step.get("command").and_then(|v| v.as_str()) {
            let args: Vec<String> = step
                .get("args")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            app.ui.core.request(UiAction::ChatCommand {
                name: name.into(),
                args,
            });
            pump(app)?;
        } else if let Some(seconds) = step.get("wait").and_then(|v| v.as_f64()) {
            run_for(app, seconds as f32)?;
        } else if step.get("until").and_then(|v| v.as_str()) == Some("landed") {
            until(app, "the player standing", |a| {
                a.local_motion().is_some_and(|(p, _)| {
                    p.grounded && glam::Vec3::from(p.velocity).length() < 0.001
                })
            })?;
        } else if let Some(name) = step.get("hold").and_then(|v| v.as_str()) {
            let down = step.get("down").and_then(|v| v.as_bool()).unwrap_or(true);
            app.ui.core.request(UiAction::Game(GameAction::Held {
                control: control(name)?,
                down,
            }));
            pump(app)?;
        } else if let Some(look) = step.get("look") {
            let f = |k: &str| look.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            app.ui.core.request(UiAction::Game(GameAction::Look {
                yaw: f("yaw"),
                pitch: f("pitch"),
            }));
            pump(app)?;
        } else if step.get("third_person").is_some() {
            app.ui
                .core
                .request(UiAction::Game(GameAction::ToggleFirstPerson {
                    fast: false,
                }));
            pump(app)?;
        } else if let Some(slot) = step.get("use_tool").and_then(|v| v.as_u64()) {
            app.ui.core.request(UiAction::UseTool {
                slot: slot as usize,
            });
            pump(app)?;
        } else if let Some(name) = step.get("shot").and_then(|v| v.as_str()) {
            let path = out.join(format!("{name}.png"));
            save(&path, &capture(app, gpu, renderer, size)?, size)?;
            written.push(path.display().to_string());
        } else if let Some(burst) = step.get("burst") {
            let name = burst
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("burst");
            let count = burst.get("count").and_then(|v| v.as_u64()).unwrap_or(1);
            let every = burst.get("every_ms").and_then(|v| v.as_u64()).unwrap_or(12);
            for n in 0..count {
                step_game(app, every)?;
                let path = out.join(format!("{name}-{n:02}.png"));
                save(&path, &capture(app, gpu, renderer, size)?, size)?;
                written.push(path.display().to_string());
            }
        } else {
            anyhow::bail!("unknown {what}");
        }
    }
    Ok(())
}

fn step_game(app: &mut App, ms: u64) -> Result<()> {
    step(app, Duration::from_millis(ms))
}

#[test]
fn capture_frames() -> Result<()> {
    let Some(spec_path) = std::env::var_os("BRI_CAPTURE") else {
        return Ok(());
    };
    let spec: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&spec_path).context("the capture spec")?)?;
    let content = PathBuf::from(
        spec["content"]
            .as_str()
            .context("spec.content: the content root")?,
    );
    let out = PathBuf::from(
        spec["out"]
            .as_str()
            .context("spec.out: the folder for the frames")?,
    );
    let size = spec
        .get("size")
        .and_then(|s| s.as_array())
        .map_or((1280, 720), |s| {
            (
                s[0].as_u64().unwrap_or(1280) as u32,
                s[1].as_u64().unwrap_or(720) as u32,
            )
        });
    std::fs::create_dir_all(&out)?;
    let state = std::env::temp_dir().join(format!("bri-capture-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state)?;
    let mut app = App::load(&content, &state, size)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    let wanted = spec["map"].as_str().unwrap_or("").to_lowercase();
    let maps: Vec<String> = app.ui.core.maps.iter().map(|m| m.id.clone()).collect();
    let map = maps
        .iter()
        .find(|m| m.to_lowercase() == wanted)
        .or_else(|| maps.iter().find(|m| m.to_lowercase().contains(&wanted)))
        .with_context(|| format!("no map matches {wanted:?}; the maps are {maps:?}"))?
        .clone();
    let mode = match spec.get("mode").and_then(|m| m.as_str()) {
        Some(id) => Some(
            app.ui
                .core
                .game_modes
                .iter()
                .find(|m| m.id == id || m.id.contains(id))
                .map(|m| m.id.clone())
                .with_context(|| format!("no game mode matches {id:?}"))?,
        ),
        None => None,
    };
    println!("CAPTURE map {map} mode {mode:?}");
    app.ui.core.request(UiAction::HostGame {
        map,
        mode: ServerMode::SinglePlayer,
        game_mode: mode,
        max_players: 1,
        server_name: "capture".into(),
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
    let gpu = support::gpu::turn().context("offscreen renderer (no GPU adapter?)")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let steps = spec["steps"].as_array().context("spec.steps")?;
    let mut written = Vec::new();
    let result = play(
        &mut app,
        &gpu,
        &mut renderer,
        steps,
        &out,
        size,
        &mut written,
    );
    for path in &written {
        println!("CAPTURE frame {path}");
    }
    println!("CAPTURE done {} frames in {}", written.len(), out.display());
    result
}
