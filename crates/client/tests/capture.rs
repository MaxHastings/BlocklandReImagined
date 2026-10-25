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
fn draw_view(
    app: &mut App,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    size: (u32, u32),
    view: &wgpu::TextureView,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<()> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    ensure!(
        app.render_scene(&mut RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder,
            target: view,
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
        encoder,
        view,
        format,
        size,
        ui.scale(),
        &ui.core.pack,
        &ui.draw(),
        None,
    );
    Ok(())
}

/// Synchronized offscreen frames, without image copies/encoding. A diagnostic,
/// not interactive FPS: the GPU wait deliberately serializes every frame.
fn measure(
    app: &mut App,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    size: (u32, u32),
    count: usize,
    yaw_per_frame: f32,
) -> Result<serde_json::Value> {
    ensure!(
        (30..=240).contains(&count),
        "measure needs 30 to 240 frames"
    );
    ensure!(
        yaw_per_frame.is_finite() && yaw_per_frame.abs() <= 0.05,
        "invalid measurement yaw step"
    );
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bounded offscreen measurement"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut update = Vec::new();
    let mut record = Vec::new();
    let mut total = Vec::new();
    let mut passes = std::collections::BTreeMap::<&str, Vec<f64>>::new();
    let mut previous = Instant::now();
    app.time_gpu_passes(true);
    let result = (|| {
        for i in 0..count + 20 {
            let start = Instant::now();
            if yaw_per_frame != 0. {
                app.ui.core.request(UiAction::Game(GameAction::Look {
                    yaw: yaw_per_frame,
                    pitch: 0.,
                }));
            }
            step(app, start.duration_since(previous))?;
            previous = start;
            let updated = Instant::now();
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            draw_view(app, gpu, renderer, size, &view, &mut encoder)?;
            gpu.queue.submit([encoder.finish()]);
            let recorded = Instant::now();
            support::gpu::wait(&gpu.device, "offscreen measured frame")?;
            if i >= 20 {
                let ms = |d: Duration| d.as_secs_f64() * 1000.;
                update.push(ms(updated - start));
                record.push(ms(recorded - updated));
                total.push(ms(start.elapsed()));
                for &(name, time) in app.gpu_pass_times() {
                    passes.entry(name).or_default().push(f64::from(time));
                }
            }
        }
        let stats = |mut samples: Vec<f64>| {
            samples.sort_by(f64::total_cmp);
            if samples.is_empty() {
                return serde_json::Value::Null;
            }
            serde_json::json!({
                "p50_ms": samples[samples.len()/2],
                "p95_ms": samples[(samples.len()*95/100).min(samples.len()-1)],
                "samples": samples.len(),
            })
        };
        let passes: std::collections::BTreeMap<_, _> = passes
            .into_iter()
            .map(|(name, samples)| (name, stats(samples)))
            .collect();
        Ok(serde_json::json!({
            "update": stats(update), "record_and_submit": stats(record),
            "synchronized_frame": stats(total), "gpu_passes": passes,
            "render": app.render_stats(),
            "yaw_per_frame": yaw_per_frame,
        }))
    })();
    app.time_gpu_passes(false);
    result
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
    draw_view(app, gpu, renderer, size, &view, &mut encoder)?;
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

/// The player or bot a step names: the first other player whose name contains `name` (case-insensitive); an empty name picks the
/// nearest living one. Its owner, and the point `height` of its body's height above its feet.
fn target(app: &App, name: &str, height: f32) -> Option<(bri_world::OwnerId, glam::Vec3)> {
    let view = app.network_view()?;
    let me = glam::Vec3::from(view.poses.get(&view.owner)?.player.feet);
    let wanted = name.to_lowercase();
    let mut found: Vec<_> = view
        .poses
        .iter()
        .filter(|(owner, _)| **owner != view.owner)
        .filter(|(owner, _)| {
            let named = view
                .names
                .get(owner)
                .is_some_and(|n| n.to_lowercase().contains(&wanted));
            let living = view.vitals.get(owner).is_some_and(|v| v.alive);
            named && (!wanted.is_empty() || living)
        })
        .map(|(owner, pose)| {
            let feet = glam::Vec3::from(pose.player.feet);
            let tall = view
                .archetypes
                .resolve(pose.player.archetype)
                .movement
                .stand_height
                * pose.player.scale;
            (*owner, feet + glam::Vec3::Y * tall * height)
        })
        .collect();
    found.sort_by(|a, b| a.1.distance(me).total_cmp(&b.1.distance(me)));
    found.into_iter().next()
}
/// `angle` turned into (-PI, PI].
fn wrapped(mut angle: f32) -> f32 {
    while angle > std::f32::consts::PI {
        angle -= std::f32::consts::TAU;
    }
    while angle <= -std::f32::consts::PI {
        angle += std::f32::consts::TAU;
    }
    angle
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
/// {"key": name, "seconds": 0.1} and {"mouse": "left", "seconds": 0.1} (a press through the interface's own input path, so the profile's binds
/// decide: the Use key opens a door), {"aim": [x, z]} (turn to face a point), {"goto": [x, z]} (walk there), {"aim_at": {"name": part of a
/// player's or bot's name, "height": 0.7}} (turn the crosshair onto it; fire with `hold`) and {"until": "dead", "name": ..} (wait for it to die),
/// {"measure": 120} records bounded synchronized frame timings without image readback.
/// {"panel": {"control": id, "lists": {id: row}}} (a press or pick in the open Add-On panel), {"report": true} (prints every body's place and health), {"shot": name}, {"burst": {"name": n, "count": c, "every_ms": ms}} (steps the game `every_ms` and captures `count` times).
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
        } else if step.get("until").and_then(|v| v.as_str()) == Some("alive") {
            until(app, "the deployed player alive", |a| {
                a.network_view().is_some_and(|v| {
                    v.poses.contains_key(&v.owner)
                        && v.vitals.get(&v.owner).is_some_and(|life| life.alive)
                })
            })?;
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
        } else if let Some(name) = step.get("key").and_then(|v| v.as_str()) {
            // A key press through the interface's own input path (the profile's binds, the panels, the chat box), held `seconds` (default 0.1).
            let key = bri_ui::input::Key::from_torque(name).with_context(|| {
                format!("unknown key {name:?} (Torque ActionMap names: r, f, 2, lshift, f3 ...)")
            })?;
            let hold = step.get("seconds").and_then(|v| v.as_f64()).unwrap_or(0.1) as f32;
            let mods = bri_ui::input::Modifiers::default();
            app.ui.handle_input(bri_ui::input::InputEvent::KeyDown {
                key,
                mods,
                repeat: false,
            });
            pump(app)?;
            run_for(app, hold)?;
            app.ui
                .handle_input(bri_ui::input::InputEvent::KeyUp { key, mods });
            pump(app)?;
        } else if let Some(name) = step.get("mouse").and_then(|v| v.as_str()) {
            // A mouse button press through the interface's own input path (the profile's mouse binds), held `seconds` (default 0.1).
            let button = match name {
                "left" => bri_ui::input::MouseButton::Left,
                "middle" => bri_ui::input::MouseButton::Middle,
                "right" => bri_ui::input::MouseButton::Right,
                other => anyhow::bail!("unknown mouse button {other:?} (left, middle, right)"),
            };
            let hold = step.get("seconds").and_then(|v| v.as_f64()).unwrap_or(0.1) as f32;
            let (x, y) = (0.0, 0.0);
            app.ui
                .handle_input(bri_ui::input::InputEvent::MouseDown { button, x, y });
            pump(app)?;
            run_for(app, hold)?;
            app.ui
                .handle_input(bri_ui::input::InputEvent::MouseUp { button, x, y });
            pump(app)?;
        } else if let Some(aim) = step.get("aim").and_then(|v| v.as_array()) {
            // Turn to face a point (the look action, repeated until within a hair of it); nothing is held. Prints the yaw it ended at.
            let (tx, tz) = (
                aim.first().and_then(|v| v.as_f64()).context("aim x")? as f32,
                aim.get(1).and_then(|v| v.as_f64()).context("aim z")? as f32,
            );
            for _ in 0..20 {
                let Some((p, _)) = app.local_motion() else {
                    break;
                };
                let want = (tx - p.feet[0]).atan2(-(tz - p.feet[2]));
                let mut delta = want - p.yaw;
                while delta > std::f32::consts::PI {
                    delta -= std::f32::consts::TAU;
                }
                while delta < -std::f32::consts::PI {
                    delta += std::f32::consts::TAU;
                }
                if delta.abs() < 0.005 {
                    break;
                }
                app.ui.core.request(UiAction::Game(GameAction::Look {
                    yaw: delta,
                    pitch: 0.0,
                }));
                pump(app)?;
                run_for(app, 0.05)?;
            }
            let at = app.local_motion().map(|(p, _)| (p.feet, p.yaw));
            println!("CAPTURE aim {tx} {tz}: at {at:.3?}");
        } else if let Some(goto) = step.get("goto").and_then(|v| v.as_array()) {
            // Walk to a point by the controls alone: turn toward it (the look action) and hold forward until within `within`
            // (default 0.8), or `timeout` seconds (default 30) pass. Reports where it ended.
            let (tx, tz) = (
                goto.first().and_then(|v| v.as_f64()).context("goto x")? as f32,
                goto.get(1).and_then(|v| v.as_f64()).context("goto z")? as f32,
            );
            let within = step.get("within").and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
            let timeout = step.get("timeout").and_then(|v| v.as_f64()).unwrap_or(30.0) as f32;
            app.ui.core.request(UiAction::Game(GameAction::Held {
                control: HeldControl::Forward,
                down: true,
            }));
            let mut spent = 0.0f32;
            let mut arrived = false;
            while spent < timeout {
                let Some((p, _)) = app.local_motion() else {
                    break;
                };
                let (dx, dz) = (tx - p.feet[0], tz - p.feet[2]);
                if (dx * dx + dz * dz).sqrt() <= within {
                    arrived = true;
                    break;
                }
                let want = dx.atan2(-dz);
                let mut delta = want - p.yaw;
                while delta > std::f32::consts::PI {
                    delta -= std::f32::consts::TAU;
                }
                while delta < -std::f32::consts::PI {
                    delta += std::f32::consts::TAU;
                }
                app.ui.core.request(UiAction::Game(GameAction::Look {
                    yaw: delta,
                    pitch: 0.0,
                }));
                pump(app)?;
                run_for(app, 0.05)?;
                spent += 0.05;
            }
            app.ui.core.request(UiAction::Game(GameAction::Held {
                control: HeldControl::Forward,
                down: false,
            }));
            pump(app)?;
            let at = app.local_motion().map(|(p, _)| p.feet);
            println!(
                "CAPTURE goto {tx} {tz}: {} after {spent:.2}s at {at:.2?}",
                if arrived { "arrived" } else { "did not arrive" }
            );
        } else if let Some(aim) = step.get("aim_at") {
            // Turn the view onto a player or bot, the look action repeated until the crosshair is on it; nothing is held and nothing is
            // damaged: fire with `hold`. {"aim_at": {"name": "Gd", "height": 0.7}}: `name` part of its name (empty: the nearest living one),
            // `height` how far up its body, as a share of its height (default 0.7, the chest).
            let name = aim.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let height = aim.get("height").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32;
            let mut aimed = None;
            for _ in 0..40 {
                let (Some((owner, point)), Some((p, Some(eye)))) =
                    (target(app, name, height), app.local_motion())
                else {
                    break;
                };
                let to = point - eye;
                let yaw = wrapped(to.x.atan2(-to.z) - p.yaw);
                let pitch = to.y.atan2(to.x.hypot(to.z)) - p.pitch;
                aimed = Some((owner, to.length(), yaw.abs().max(pitch.abs())));
                if yaw.abs() < 0.005 && pitch.abs() < 0.005 {
                    break;
                }
                app.ui
                    .core
                    .request(UiAction::Game(GameAction::Look { yaw, pitch: -pitch }));
                pump(app)?;
                run_for(app, 0.05)?;
            }
            let (owner, distance, off) = aimed.with_context(|| {
                format!(
                    "no player to aim at matches {name:?}; the others are {:?}",
                    app.network_view().map(|v| v
                        .names
                        .iter()
                        .filter(|(o, _)| **o != v.owner)
                        .map(|(_, n)| n.clone())
                        .collect::<Vec<_>>())
                )
            })?;
            println!("CAPTURE aim_at {owner} {distance:.2} away, {off:.3} rad off");
        } else if step.get("until").and_then(|v| v.as_str()) == Some("dead") {
            // Wait until the named player or bot lies dead (shot, by `hold fire` after `aim_at`): {"until": "dead", "name": "Gd"}.
            let name = step.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let (owner, _) = target(app, name, 0.0)
                .with_context(|| format!("no player to wait on matches {name:?}"))?;
            until(app, "the target dead", |a| {
                a.network_view()
                    .is_some_and(|v| v.vitals.get(&owner).is_some_and(|life| !life.alive))
            })?;
            println!("CAPTURE dead {owner}");
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
            if let Some(stats) = app.render_stats() {
                println!("CAPTURE render {name} {}", serde_json::to_string(&stats)?);
            }
            written.push(path.display().to_string());
        } else if let Some(measurement) = step.get("measure") {
            let count = measurement
                .as_u64()
                .or_else(|| measurement["frames"].as_u64())
                .context("measure needs a frame count")?;
            let yaw = measurement["yaw_per_frame"].as_f64().unwrap_or(0.) as f32;
            let report = measure(app, gpu, renderer, size, usize::try_from(count)?, yaw)?;
            println!("CAPTURE measure {}", serde_json::to_string(&report)?);
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
                if let Some(stats) = app.render_stats() {
                    println!(
                        "CAPTURE render {name}-{n:02} {}",
                        serde_json::to_string(&stats)?
                    );
                }
                written.push(path.display().to_string());
            }
        } else if let Some(panel) = step.get("panel") {
            // What a player does in the open Add-On panel: {"panel": {"control": "deploy", "lists": {"team": 1}}}.
            let open = app
                .ui
                .core
                .panel
                .clone()
                .context("a panel step with no panel open")?;
            let lists = panel
                .get("lists")
                .and_then(|l| l.as_object())
                .map(|l| {
                    l.iter()
                        .map(|(k, v)| (k.clone(), v.as_u64().map(|n| n as u16)))
                        .collect()
                })
                .unwrap_or_default();
            app.ui.core.request(UiAction::PanelEvent {
                package: open.package,
                panel: open.id,
                control: panel
                    .get("control")
                    .and_then(|c| c.as_str())
                    .unwrap_or("close")
                    .into(),
                fields: Vec::new(),
                lists,
                drop: None,
            });
            pump(app)?;
        } else if let Some(expected) = step.get("expect_player") {
            let view = app
                .network_view()
                .context("expected player without a session")?;
            let pose = view
                .poses
                .get(&view.owner)
                .context("expected player without a body")?;
            ensure!(
                view.vitals.get(&view.owner).is_some_and(|v| v.alive),
                "expected a living player at capture viewpoint; feet {:?}",
                pose.player.feet
            );
            let feet: [f32; 3] = serde_json::from_value(expected["feet"].clone())?;
            let within = expected["within"].as_f64().unwrap_or(2.0) as f32;
            ensure!(
                within.is_finite() && within > 0. && feet.iter().all(|v| v.is_finite()),
                "invalid expected viewpoint"
            );
            ensure!(
                glam::Vec3::from(feet).distance(glam::Vec3::from(pose.player.feet)) <= within,
                "capture viewpoint {:?} differs from expected {:?} by more than {within}",
                pose.player.feet,
                feet
            );
        } else if step.get("report").is_some() {
            if let Some(view) = app.network_view() {
                for (id, pose) in &view.poses {
                    let name = view.names.get(id).cloned().unwrap_or_default();
                    let vitals = view.vitals.get(id);
                    println!(
                        "CAPTURE report tick {} {name:?} feet {:.2?} alive {:?} health {:?}",
                        view.tick,
                        pose.player.feet,
                        vitals.map(|v| v.alive),
                        vitals.map(|v| v.health),
                    );
                }
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
    if let Some(settings) = spec.get("settings_file").and_then(|s| s.as_str()) {
        std::fs::copy(settings, state.join("settings.json"))?;
    }
    let mut app = App::load(&content, &state, size)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    if let Some(quality) = spec.get("quality").and_then(|s| s.as_str()) {
        let quality = match quality {
            "Low" => bri_client::quality::Quality::Low,
            "Medium" => bri_client::quality::Quality::Medium,
            "High" => bri_client::quality::Quality::High,
            _ => anyhow::bail!("Unknown diagnostic quality {quality}"),
        };
        let mut settings = app.ui.settings();
        settings.prefs.extend(quality.prefs());
        app.ui
            .core
            .request(UiAction::SaveSettings(Box::new(settings)));
        pump(&mut app)?;
    }
    if let Some(name) = spec.get("menu_shot").and_then(|s| s.as_str()) {
        let gpu = support::gpu::turn()?;
        let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
        app.ui.update(16);
        let pixels = gpu.render_rgba(
            &mut renderer,
            &app.ui.core.pack,
            &app.ui.draw(),
            size,
            app.ui.scale(),
            [0.0, 0.0, 0.0, 1.0],
        )?;
        image::save_buffer(
            out.join(format!("{name}.png")),
            &pixels,
            size.0,
            size.1,
            image::ColorType::Rgba8,
        )?;
    }
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
