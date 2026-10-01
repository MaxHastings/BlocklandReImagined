//! Headless probe: while the view turns, the local body and the held item
//! must hold still relative to the camera, whatever the frame timing and
//! wherever mouse motion lands between a frame's tick and its render. Runs
//! on the made-up content root; the ignored variant runs on the generated
//! v20 content (`--release -- --ignored`, BRI_CONTENT or content/).
//! Loopback QUIC and an offscreen GPU; never opens a window or moves the
//! mouse.
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
};
use glam::{Quat, Vec3};
use std::{
    thread,
    time::{Duration, Instant},
};

#[macro_use]
mod support;
use support::content_root::ContentRoot;

synthetic_and_content!(ContentRoot: body_and_held_item_hold_still_against_a_turning_camera);

const SIZE: (u32, u32) = (640, 480);
/// A steady mouse turn, in radians per second.
const TURN: f32 = 2.5;
const FRAMES: usize = 300;

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
fn game(app: &mut App, action: GameAction) -> Result<()> {
    app.ui.core.request(UiAction::Game(action));
    pump(app)
}
fn render(
    app: &mut App,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    target: &wgpu::TextureView,
) -> Result<()> {
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    ensure!(
        app.render_scene(&mut RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target,
            format: wgpu::TextureFormat::Rgba8Unorm,
            size: SIZE,
            ui_renderer: renderer,
        })?,
        "App did not render a session camera"
    );
    gpu.queue.submit([encoder.finish()]);
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    Ok(())
}

/// A small deterministic generator, so every run sees the same timing.
struct Lcg(u64);
impl Lcg {
    fn unit(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// One point tracked in the camera's frame. `step` is how far it moves
/// between frames; `jitter` is how far it lands from where its previous
/// motion carried it (constant velocity over the uneven frame times), which
/// smooth animation keeps small and a shake does not.
#[derive(Default)]
struct Track {
    step_mm: Vec<f32>,
    jitter_mm: Vec<f32>,
    history: Vec<(f32, Vec3)>,
}
impl Track {
    fn push(&mut self, seconds: f32, at: Vec3, measure: bool) {
        if measure && let [.., (t0, q0), (t1, q1)] = self.history[..] {
            let ahead = q1 + (q1 - q0) * ((seconds - t1) / (t1 - t0).max(1e-4));
            self.step_mm.push((at - q1).length() * 1000.0);
            self.jitter_mm.push((at - ahead).length() * 1000.0);
        }
        self.history.push((seconds, at));
    }
    fn json(&self) -> serde_json::Value {
        let summary = |v: &[f32]| serde_json::json!({"rms": rms(v), "p95": percentile(v, 0.95), "max": max(v)});
        serde_json::json!({"step_mm": summary(&self.step_mm), "jitter_mm": summary(&self.jitter_mm)})
    }
    fn line(&self) -> String {
        format!(
            "step rms {:7.3} | jitter rms {:7.3} p95 {:7.3} max {:7.3} mm",
            rms(&self.step_mm),
            rms(&self.jitter_mm),
            percentile(&self.jitter_mm, 0.95),
            max(&self.jitter_mm)
        )
    }
}
#[derive(Default)]
struct Stats {
    item: Track,
    body: Track,
    hand: Track,
    yaw_error_deg: Vec<f32>,
    frame_ms: Vec<f32>,
}
fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt()
}
fn percentile(v: &[f32], p: f32) -> f32 {
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s.get(((s.len() as f32 - 1.0) * p).round() as usize)
        .copied()
        .unwrap_or(0.0)
}
fn max(v: &[f32]) -> f32 {
    v.iter().copied().fold(0.0, f32::max)
}
impl Stats {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "frames": self.frame_ms.len(),
            "frame_ms": {"min": self.frame_ms.iter().copied().fold(f32::MAX, f32::min), "max": max(&self.frame_ms)},
            "held_item": self.item.json(),
            "body_origin": self.body.json(),
            "right_hand": self.hand.json(),
            "body_yaw_vs_camera_deg": {"max": max(&self.yaw_error_deg)},
        })
    }
}

/// Where a world point sits in the camera's frame (right, up, back).
fn in_camera(camera: (Vec3, f32, f32), point: Vec3) -> Vec3 {
    let (eye, yaw, pitch) = camera;
    let rotation = Quat::from_rotation_y(-yaw) * Quat::from_rotation_x(pitch);
    rotation.inverse() * (point - eye)
}

/// Turn steadily for `FRAMES` uneven frames. Each frame's mouse motion is
/// split at a random point: what arrived before the tick, and what arrived
/// while the tick ran, which the window loop hands over before rendering.
fn turn(
    app: &mut App,
    gpu: &Headless,
    renderer: &mut UiRenderer,
    target: &wgpu::TextureView,
    rng: &mut Lcg,
) -> Result<Stats> {
    let owner = app.network_view().context("in game")?.owner;
    let mut stats = Stats::default();
    let mut previous = Instant::now();
    let mut seconds = 0.0;
    let mut late = 0.0;
    for frame in 0..FRAMES {
        // 3 to 14 ms frames with an occasional 25 ms hitch.
        let wait = if frame % 37 == 36 {
            25.0
        } else {
            3.0 + rng.unit() * 11.0
        };
        thread::sleep(Duration::from_secs_f32(wait / 1000.0));
        let now = Instant::now();
        let dt = now.duration_since(previous);
        previous = now;
        let turned = TURN * dt.as_secs_f32();
        let split = rng.unit();
        // Motion that arrived after last frame's tick reaches the controls
        // before this tick; the rest arrives while this tick runs.
        game(
            app,
            GameAction::Look {
                yaw: late + turned * split,
                pitch: 0.0,
            },
        )?;
        step(app, dt)?;
        late = turned * (1.0 - split);
        // The window loop dispatches that input before the redraw.
        game(
            app,
            GameAction::Look {
                yaw: late,
                pitch: 0.0,
            },
        )?;
        late = 0.0;
        render(app, gpu, renderer, target)?;
        let camera = app.rendered_camera().context("rendered camera")?;
        let yaw = app.presented_local().context("local body")?.yaw;
        let point = |m: Option<glam::Mat4>, what: &str| -> Result<Vec3> {
            Ok(in_camera(
                camera,
                m.context(what.to_string())?.w_axis.truncate(),
            ))
        };
        let item = point(app.held_image_transform(0), "held item")?;
        let body = point(app.avatar_body(owner), "body")?;
        let hand = point(app.avatar_node(owner, "Mount0"), "hand node")?;
        let error = (yaw - camera.1 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        seconds += dt.as_secs_f32();
        let measure = frame >= 20;
        stats.item.push(seconds, item, measure);
        stats.body.push(seconds, body, measure);
        stats.hand.push(seconds, hand, measure);
        if measure {
            stats.yaw_error_deg.push(error.abs().to_degrees());
            stats.frame_ms.push(dt.as_secs_f32() * 1000.0);
        }
    }
    Ok(stats)
}

fn body_and_held_item_hold_still_against_a_turning_camera(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("view-jitter")?;
    let state_dir = f.state()?;
    let state = state_dir.path();
    let mut app = App::load(&f.root, state, SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: f.map.0.clone(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "View jitter".into(),
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
    until(&mut app, "player landing", |a| {
        a.local_motion()
            .is_some_and(|(p, _)| p.grounded && Vec3::from(p.velocity).length() < 0.001)
    })?;
    app.ui.core.request(UiAction::UseTool { slot: 0 });
    pump(&mut app)?;
    until(&mut app, "hammer in hand", |a| {
        a.network_view()
            .is_some_and(|v| v.tools[&v.owner].selected == Some(0))
            && a.held_image_transform(0).is_some()
    })?;
    if !f.content {
        // lavapipe crashes in the Unified lighting path here: see
        // `support::gpu::pin_classic_lighting` (an open renderer follow-up;
        // the content variant keeps Unified lighting).
        support::gpu::pin_classic_lighting(&mut app)?;
    }
    let gpu = support::gpu::turn().context("offscreen renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("view-jitter target"),
        size: wgpu::Extent3d {
            width: SIZE.0,
            height: SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target = texture.create_view(&Default::default());
    let mut rng = Lcg(0x5eed);
    let mut report = serde_json::json!({"adapter": gpu.adapter_info.name, "map": f.map.0,
        "turn_rad_per_s": TURN, "cases": {}});
    let mut cases = Vec::new();
    for (name, third, walk) in [
        ("first person, standing", false, false),
        ("first person, walking", false, true),
        ("third person, standing", true, false),
        ("third person, walking", true, true),
    ] {
        if third != app.controls.third_person {
            game(&mut app, GameAction::ToggleFirstPerson { fast: true })?;
        }
        game(
            &mut app,
            GameAction::Held {
                control: HeldControl::Forward,
                down: walk,
            },
        )?;
        if !walk {
            // A standing case measures a body at rest: the last case's walk
            // must have stopped and its run clip blended out to `root`
            // (a 0.25 s transition), or the torso still carries the hand.
            until(&mut app, "the body at rest", |a| {
                a.local_motion()
                    .is_some_and(|(p, _)| p.grounded && Vec3::from(p.velocity).length() < 0.001)
                    && a.network_view().and_then(|v| a.avatar_action(v.owner))
                        == Some(("root", false))
            })?;
        }
        let stats = turn(&mut app, &gpu, &mut renderer, &target, &mut rng)?;
        game(
            &mut app,
            GameAction::Held {
                control: HeldControl::Forward,
                down: false,
            },
        )?;
        eprintln!(
            "{name}: body yaw vs camera max {:.3} deg, frames {:.1}-{:.1} ms",
            max(&stats.yaw_error_deg),
            stats.frame_ms.iter().copied().fold(f32::MAX, f32::min),
            max(&stats.frame_ms)
        );
        eprintln!("  held item   {}", stats.item.line());
        eprintln!("  body origin {}", stats.body.line());
        eprintln!("  right hand  {}", stats.hand.line());
        report["cases"][name] = stats.json();
        cases.push((name, walk, stats));
    }
    std::fs::write(
        artifact.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    for (name, walk, stats) in &cases {
        // The body turns with the very yaw the camera was drawn with.
        ensure!(
            max(&stats.yaw_error_deg) < 1e-3,
            "{name}: body yaw trails the camera by up to {} deg",
            max(&stats.yaw_error_deg)
        );
        // The body holds still in the camera's frame; only float noise.
        ensure!(
            rms(&stats.body.jitter_mm) < 0.1,
            "{name}: body shakes {} mm rms against the camera",
            rms(&stats.body.jitter_mm)
        );
        // A first-person image rides the eye, so it holds still while
        // walking too. In third person it swings with the running arm.
        if !name.starts_with("third") || !walk {
            ensure!(
                rms(&stats.item.jitter_mm) < 0.1,
                "{name}: held item shakes {} mm rms against the camera",
                rms(&stats.item.jitter_mm)
            );
        }
    }
    Ok(())
}
