//! Video rehearsal: walks the beats of the v0.2.4 showcase video on the
//! normal App, offscreen, with the player's own settings, and records what a
//! camera would see: a screenshot per moment, every frame's time (update,
//! record and the GPU wait a synchronized present makes) and the console's
//! warnings and errors. Never opens a window or sends OS input.
//!
//! Run (release, real GPU):
//!   BRI_REHEARSAL_OUT=<folder> \
//!   BRI_REHEARSAL_SETTINGS="%LOCALAPPDATA%/BlocklandReImagined/settings.json" \
//!   BRI_REHEARSAL_SAVES="%LOCALAPPDATA%/BlocklandReImagined/saves" \
//!   cargo test -p bri-client --release --test video_rehearsal -- --ignored --nocapture
//! Optional: BRI_REHEARSAL_SIZE ("1920x1080"), BRI_CONTENT (content root).
use anyhow::{Context, Result, bail, ensure};
use bri_console::Clamp;
use bri_client::{
    app::App,
    perf::GpuFrameTimer,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{api::*, gpu::UiRenderer, screens::ScreenId};
use glam::Vec3;
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";
const BRICK: &str = "v20/brick/brick2x4data";
/// A frame slower than this is a visible hitch on a 60 Hz recording.
const HITCH_MS: f64 = 50.0;

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[derive(Serialize, Default, Clone)]
struct Stats {
    frames: usize,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
}
fn stats(samples: &[f64]) -> Stats {
    if samples.is_empty() {
        return Stats::default();
    }
    let mut s = samples.to_vec();
    s.sort_by(f64::total_cmp);
    let at = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
    Stats {
        frames: s.len(),
        mean_ms: s.iter().sum::<f64>() / s.len() as f64,
        p50_ms: at(0.5),
        p95_ms: at(0.95),
        p99_ms: at(0.99),
        max_ms: at(1.0),
    }
}

/// One beat of the script: how its frames went and what it left behind.
#[derive(Serialize, Default)]
struct Beat {
    name: String,
    seconds: f64,
    frame: Stats,
    update: Stats,
    gpu: Stats,
    /// Frames over [`HITCH_MS`]: (seconds into the beat, ms, of which
    /// update ms).
    hitches: Vec<(f64, f64, f64)>,
    shots: Vec<String>,
    notes: Vec<String>,
    warnings: Vec<String>,
    errors: Vec<String>,
    #[serde(skip)]
    frames: Vec<f64>,
    #[serde(skip)]
    updates: Vec<f64>,
    #[serde(skip)]
    gpus: Vec<f64>,
    #[serde(skip)]
    started: Option<Instant>,
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    name: String,
}
/// The game's adapter choice: the high-performance GPU with timestamps.
fn gpu() -> Result<Gpu> {
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

struct Rehearsal {
    app: Box<App>,
    gpu: Gpu,
    ui: UiRenderer,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    timer: Option<GpuFrameTimer>,
    size: (u32, u32),
    out: PathBuf,
    state: PathBuf,
    shot: usize,
    beat: Beat,
    beats: Vec<Beat>,
    previous: Instant,
    last_gpu: Option<Duration>,
    /// Other players' Apps (headless guests), stepped with every frame.
    others: Vec<Box<App>>,
}

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

impl Rehearsal {
    fn new(name: &str) -> Result<Option<Self>> {
        let Some(out) = std::env::var_os("BRI_REHEARSAL_OUT") else {
            eprintln!("BRI_REHEARSAL_OUT is not set; skipping");
            return Ok(None);
        };
        let out = PathBuf::from(out).join(name);
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out)?;
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let content = std::env::var_os("BRI_CONTENT")
            .map_or_else(|| workspace.join("content"), PathBuf::from);
        let size = std::env::var("BRI_REHEARSAL_SIZE")
            .ok()
            .and_then(|s| {
                let (w, h) = s.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            })
            .unwrap_or((1920u32, 1080u32));
        let state = out.join("state");
        std::fs::create_dir_all(&state)?;
        if let Some(settings) = std::env::var_os("BRI_REHEARSAL_SETTINGS") {
            std::fs::copy(&settings, state.join("settings.json"))?;
        }
        prepare_add_ons(&content)?;
        let mut beat = Beat {
            name: "startup".into(),
            started: Some(Instant::now()),
            ..Default::default()
        };
        let started = Instant::now();
        let mut app = App::load(&content, &state, size)?;
        beat.notes
            .push(format!("App::load {:.0} ms", ms(started.elapsed())));
        let gpu = gpu()?;
        let ui = UiRenderer::new(&gpu.device, &gpu.queue);
        let opened = Instant::now();
        app.gpu_ready(&gpu.device, &gpu.queue, FORMAT)?;
        beat.notes
            .push(format!("gpu_ready {:.0} ms", ms(opened.elapsed())));
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rehearsal frame"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let timer = GpuFrameTimer::new(&gpu.device, &gpu.queue);
        println!("adapter {}", gpu.name);
        bri_console::log::clear();
        Ok(Some(Self {
            app,
            gpu,
            ui,
            target,
            view,
            timer,
            size,
            out,
            state,
            shot: 0,
            beat,
            beats: Vec::new(),
            previous: Instant::now(),
            last_gpu: None,
            others: Vec::new(),
        }))
    }

    /// One frame as the platform loop runs it: update with the wall time
    /// since the last, record the scene and the HUD, submit, then wait for
    /// the GPU as a synchronized present would.
    fn frame(&mut self) -> Result<()> {
        let start = Instant::now();
        let dt = start.duration_since(self.previous).min(Duration::from_millis(250));
        self.previous = start;
        self.app.tick(dt)?;
        self.app.ui.update(dt.as_millis() as u64);
        let commands = self.app.pump()?;
        ensure!(commands.is_empty(), "Unexpected window command {commands:?}");
        if let ConnectionState::Failed { reason } = &self.app.ui.core.conn {
            bail!("Connection failed: {reason}");
        }
        for other in &mut self.others {
            other.tick(dt)?;
            other.ui.update(dt.as_millis() as u64);
            other.pump()?;
        }
        let updated = Instant::now();
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        if let Some(t) = self.timer.as_mut() {
            t.begin(&mut encoder);
        }
        let drew = self.app.render_scene(&mut RenderContext {
            device: &self.gpu.device,
            queue: &self.gpu.queue,
            encoder: &mut encoder,
            target: &self.view,
            format: FORMAT,
            size: self.size,
            ui_renderer: &mut self.ui,
        })?;
        let hud = self.app.ui();
        self.ui.render(
            &self.gpu.device,
            &self.gpu.queue,
            &mut encoder,
            &self.view,
            FORMAT,
            self.size,
            hud.scale(),
            &hud.core.pack,
            &hud.draw(),
            (!drew).then_some(wgpu::Color::BLACK),
        );
        if let Some(t) = self.timer.as_mut() {
            t.end(&mut encoder);
        }
        self.gpu.queue.submit([encoder.finish()]);
        self.gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })?;
        let done = Instant::now();
        let measured = self.timer.as_mut().and_then(|t| t.collect(&self.gpu.device));
        if measured != self.last_gpu
            && let Some(g) = measured
        {
            self.beat.gpus.push(ms(g));
        }
        self.last_gpu = measured;
        let frame = ms(done - start);
        self.beat.frames.push(frame);
        self.beat.updates.push(ms(updated - start));
        if frame > HITCH_MS {
            let at = self
                .beat
                .started
                .map_or(0.0, |s| s.elapsed().as_secs_f64());
            self.beat.hitches.push((at, frame, ms(updated - start)));
        }
        Ok(())
    }

    fn run(&mut self, seconds: f32) -> Result<()> {
        let start = Instant::now();
        while start.elapsed().as_secs_f32() < seconds {
            self.frame()?;
        }
        Ok(())
    }

    fn until(
        &mut self,
        what: &str,
        timeout: Duration,
        ready: impl Fn(&App) -> bool,
    ) -> Result<Duration> {
        let start = Instant::now();
        loop {
            self.frame()?;
            if ready(&self.app) {
                return Ok(start.elapsed());
            }
            ensure!(
                start.elapsed() < timeout,
                "Timed out waiting for {what}: {:?}, {}",
                self.app.ui.core.conn,
                describe(&self.app)
            );
        }
    }

    fn request(&mut self, action: UiAction) -> Result<()> {
        self.app.ui.core.request(action);
        let commands = self.app.pump()?;
        ensure!(commands.is_empty(), "Unexpected window command {commands:?}");
        self.app.ui.update(0);
        Ok(())
    }
    /// A request from another player's App.
    fn other(&mut self, i: usize, action: UiAction) -> Result<()> {
        let other = &mut self.others[i];
        other.ui.core.request(action);
        other.pump()?;
        other.ui.update(0);
        Ok(())
    }
    fn game(&mut self, action: GameAction) -> Result<()> {
        self.request(UiAction::Game(action))
    }
    fn held(&mut self, control: HeldControl, down: bool) -> Result<()> {
        self.game(GameAction::Held { control, down })
    }
    /// Press and let go, a few frames apart.
    fn press(&mut self, control: HeldControl) -> Result<()> {
        self.held(control, true)?;
        self.run(0.1)?;
        self.held(control, false)?;
        self.run(0.1)
    }
    fn chat_command(&mut self, name: &str, args: &[&str]) -> Result<()> {
        self.request(UiAction::ChatCommand {
            name: name.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
        })
    }

    /// Mouse look to an absolute heading and pitch (`down` radians below
    /// the horizon).
    fn aim(&mut self, yaw: f32, down: f32) -> Result<()> {
        for _ in 0..3 {
            let scale = self.app.controls.fov() / 90.0;
            let turn = (yaw - self.app.controls.yaw + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let tilt = down + self.app.controls.pitch;
            if turn.abs() < 1e-3 && tilt.abs() < 1e-3 {
                break;
            }
            self.game(GameAction::Look {
                yaw: turn / scale,
                pitch: tilt / scale,
            })?;
        }
        Ok(())
    }
    /// Turn slowly through `radians` over `seconds`, as a camera pan.
    fn pan(&mut self, radians: f32, seconds: f32) -> Result<()> {
        let start = Instant::now();
        let mut turned = 0.0;
        while start.elapsed().as_secs_f32() < seconds {
            let want = radians * (start.elapsed().as_secs_f32() / seconds).min(1.0);
            let scale = self.app.controls.fov() / 90.0;
            self.game(GameAction::Look {
                yaw: (want - turned) / scale,
                pitch: 0.0,
            })?;
            turned = want;
            self.frame()?;
        }
        Ok(())
    }

    fn shot(&mut self, name: &str) -> Result<()> {
        self.frame()?;
        self.shot += 1;
        let file = format!("{:02}-{name}.png", self.shot);
        let (width, height) = self.size;
        let row = (width * 4).div_ceil(256) * 256;
        let buffer = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rehearsal shot"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            self.target.size(),
        );
        self.gpu.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.gpu.device.poll(wgpu::PollType::Wait {
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
        drop(mapped);
        image::save_buffer(
            self.out.join(&file),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )?;
        self.beat.shots.push(file);
        // The readback is not a frame the player would see.
        self.previous = Instant::now();
        Ok(())
    }

    fn note(&mut self, text: impl Into<String>) {
        let text = text.into();
        println!("  {text}");
        self.beat.notes.push(text);
    }

    /// Close the current beat and start the next.
    fn beat(&mut self, name: &str) {
        let mut done = std::mem::replace(
            &mut self.beat,
            Beat {
                name: name.into(),
                started: Some(Instant::now()),
                ..Default::default()
            },
        );
        done.seconds = done.started.map_or(0.0, |s| s.elapsed().as_secs_f64());
        done.frame = stats(&done.frames);
        done.update = stats(&done.updates);
        done.gpu = stats(&done.gpus);
        for line in bri_console::log::lines() {
            let list = match line.level {
                bri_console::Level::Warning => &mut done.warnings,
                bri_console::Level::Error => &mut done.errors,
                _ => continue,
            };
            if !list.contains(&line.text) {
                list.push(line.text);
            }
        }
        bri_console::log::clear();
        println!(
            "[{}] {:.1} s, frame p50 {:.1} p95 {:.1} max {:.1} ms, {} hitches, {} warnings, {} errors",
            done.name,
            done.seconds,
            done.frame.p50_ms,
            done.frame.p95_ms,
            done.frame.max_ms,
            done.hitches.len(),
            done.warnings.len(),
            done.errors.len()
        );
        self.beats.push(done);
        let _ = self.write();
    }

    fn write(&self) -> Result<()> {
        std::fs::write(
            self.out.join("report.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "adapter": self.gpu.name,
                "resolution": [self.size.0, self.size.1],
                "beats": self.beats,
            }))?,
        )?;
        Ok(())
    }

    /// Copy saves from BRI_REHEARSAL_SAVES (a v20-style saves folder) into
    /// this run's saves folder, where Load Bricks finds them.
    fn copy_saves(&self, saves: &[&str]) -> Result<()> {
        let Some(from) = std::env::var_os("BRI_REHEARSAL_SAVES") else {
            bail!("BRI_REHEARSAL_SAVES is not set");
        };
        for save in saves {
            let source = PathBuf::from(&from).join(save);
            let target = self.state.join("saves").join(save);
            std::fs::create_dir_all(target.parent().context("save folder")?)?;
            std::fs::copy(&source, &target)
                .with_context(|| format!("copying {}", source.display()))?;
        }
        Ok(())
    }

    /// Load Bricks the save whose name contains `name`, keeping its colours,
    /// and wait until its bricks are built and drawn. Returns the bricks.
    fn load_save(&mut self, name: &str) -> Result<usize> {
        let start = Instant::now();
        let wanted = name.to_lowercase();
        let info = loop {
            self.request(UiAction::RequestSaveList { map: None })?;
            self.run(0.5)?;
            if let Some(info) = self
                .app
                .ui
                .core
                .save_files
                .iter()
                .find(|f| f.name.to_lowercase().contains(&wanted))
            {
                break info.clone();
            }
            ensure!(
                start.elapsed() < Duration::from_secs(300),
                "No save named {name} in {:?}",
                self.app
                    .ui
                    .core
                    .save_files
                    .iter()
                    .map(|f| &f.name)
                    .collect::<Vec<_>>()
            );
        };
        self.note(format!(
            "save {} on {} ({:?} bricks) listed after {:.1} s",
            info.name,
            info.map,
            info.brick_count,
            start.elapsed().as_secs_f32()
        ));
        let before = self.bricks();
        let loading = Instant::now();
        self.request(UiAction::LoadBricks {
            map: info.map.clone(),
            name: info.name.clone(),
            ownership: true,
        })?;
        let mut last = (before, Instant::now());
        let expected = info.brick_count.map(|n| n as usize);
        loop {
            self.frame()?;
            if self.app.ui.screen(ScreenId::LoadBricksColor).is_some() {
                self.request(UiAction::LoadBricksColors(ColorLoad::Append))?;
                self.app.ui.core.pop(ScreenId::LoadBricksColor);
            }
            let count = self.bricks();
            if count != last.0 {
                last = (count, Instant::now());
            }
            let all = expected.is_some_and(|n| count + 16 >= before + n);
            let settled = count > before && last.1.elapsed() > Duration::from_secs(5);
            if (all || settled) && self.app.world_render_ready() && self.app.pending_requests() == 0
            {
                break;
            }
            ensure!(
                loading.elapsed() < Duration::from_secs(900),
                "Loading {name} stalled at {count} bricks: {}",
                describe(&self.app)
            );
        }
        let loaded = self.bricks() - before;
        self.note(format!(
            "loaded {loaded} bricks in {:.1} s",
            loading.elapsed().as_secs_f32()
        ));
        Ok(loaded)
    }

    /// The middle 90% of the world's bricks on each axis: (min, max).
    fn build_bounds(&self) -> Option<(Vec3, Vec3)> {
        let view = self.app.network_view()?;
        if view.world.bricks.is_empty() {
            return None;
        }
        let mut axes: [Vec<f32>; 3] = Default::default();
        for b in view.world.bricks.values() {
            for (axis, v) in axes.iter_mut().zip(b.position) {
                axis.push(v);
            }
        }
        let at = |axis: &mut Vec<f32>, p: f32| {
            axis.sort_by(f32::total_cmp);
            axis[((axis.len() - 1) as f32 * p) as usize]
        };
        let [x, y, z] = &mut axes;
        Some((
            Vec3::new(at(x, 0.05), at(y, 0.0), at(z, 0.05)),
            Vec3::new(at(x, 0.95), at(y, 0.95), at(z, 0.95)),
        ))
    }
    /// The admin camera (F8) placed at `eye`, looking at `target`.
    fn camera_at(&mut self, eye: Vec3, target: Vec3) -> Result<()> {
        if self.app.controls.free_camera().is_none() {
            self.game(GameAction::DropCameraAtPlayer)?;
            self.until("free camera", Duration::from_secs(20), |a| {
                a.controls.free_camera().is_some()
            })?;
        }
        self.app.controls.redrop_camera(eye);
        let d = (target - eye).normalize_or_zero();
        let (yaw, pitch) = (d.x.atan2(-d.z), d.y.asin());
        let (current_yaw, current_pitch) = self.app.controls.camera_angles();
        let delta = (yaw - current_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let scale = 90.0 / self.app.controls.fov();
        self.game(GameAction::Look {
            yaw: delta * scale,
            pitch: (current_pitch - pitch) * scale,
        })?;
        self.run(0.5)
    }
    /// The admin camera above one corner of the build, looking at it.
    fn overview(&mut self) -> Result<()> {
        let Some((min, max)) = self.build_bounds() else {
            return Ok(());
        };
        let center = (min + max) * 0.5;
        let extent = (max - min).max(Vec3::splat(20.0));
        let eye = center
            + Vec3::new(1.0, 0.7, 1.0).normalize()
                * (Vec3::new(extent.x, 0.0, extent.z).length() * 0.6).clamped(20.0, 150.0);
        self.camera_at(eye, center - Vec3::new(0.0, extent.y * 0.25, 0.0))
    }
    /// Back to the player's own eyes.
    fn camera_back(&mut self) -> Result<()> {
        if self.app.controls.free_camera().is_some() {
            // `/ret`: control of the body again, where it stood.
            self.chat_command("ret", &[])?;
            self.until("back in the body", Duration::from_secs(10), |a| {
                a.controls.free_camera().is_none()
            })?;
        }
        Ok(())
    }

    /// Look from the eye at `point`.
    fn aim_at(&mut self, point: Vec3) -> Result<()> {
        let Some(me) = self.app.presented_local() else {
            return Ok(());
        };
        let eye = Vec3::from(me.feet) + Vec3::Y * if me.crouched { 0.63 } else { 2.16 };
        let d = (point - eye).normalize_or_zero();
        self.aim(d.x.atan2(-d.z), -d.y.asin())?;
        self.run(0.2)
    }
    fn brick_position(&self, brick: u64) -> Option<Vec3> {
        let view = self.app.network_view()?;
        view.world.bricks.get(&brick).map(|b| Vec3::from(b.position))
    }
    /// Hit `brick` with the wrench, as a player does, and send `data` from
    /// the dialog it opens. Returns whether the dialog opened.
    fn wrench(&mut self, brick: u64, variant: WrenchVariant, data: WrenchData) -> Result<bool> {
        let Some(slot) = self
            .slots()
            .iter()
            .position(|s| s.as_deref().is_some_and(|s| s.contains("wrench")))
        else {
            self.note("no wrench in the tools");
            return Ok(false);
        };
        let Some(at) = self.brick_position(brick) else {
            return Ok(false);
        };
        self.request(UiAction::UseTool { slot })?;
        self.run(0.4)?;
        self.aim_at(at)?;
        self.press(HeldControl::Fire)?;
        let start = Instant::now();
        while !self.app.ui.stack().iter().any(|s| matches!(s, ScreenId::Wrench(_))) {
            if start.elapsed() > Duration::from_secs(3) {
                self.note(format!("the wrench opened nothing on brick {brick}"));
                self.request(UiAction::UnUseTool)?;
                return Ok(false);
            }
            self.frame()?;
        }
        self.request(UiAction::SendWrench {
            brick,
            variant,
            data,
        })?;
        self.run(0.3)?;
        if self.app.ui.screen(ScreenId::MessageBox).is_some() {
            self.note(format!("wrench refused: {}", describe(&self.app)));
            self.app.ui.core.pop(ScreenId::MessageBox);
        }
        for v in [
            WrenchVariant::Normal,
            WrenchVariant::Sound,
            WrenchVariant::VehicleSpawn,
        ] {
            self.app.ui.core.pop(ScreenId::Wrench(v));
        }
        self.request(UiAction::UnUseTool)?;
        self.run(0.2)?;
        Ok(true)
    }

    /// The id of a datablock-menu entry whose name (else id) contains `want`.
    fn choice(&self, list: &str, want: &str) -> Option<String> {
        let want = want.to_lowercase();
        self.app.ui.core.datablocks.get(list).and_then(|c| {
            c.iter()
                .find(|c| c.name.to_lowercase() == want)
                .or_else(|| c.iter().find(|c| c.name.to_lowercase().contains(&want)))
                .or_else(|| c.iter().find(|c| c.id.to_lowercase().contains(&want)))
                .map(|c| c.id.clone())
        })
    }

    /// Plant a vehicle spawn brick on the floor ahead and pick `vehicle`
    /// (a name in its list) on it. Returns the brick.
    fn spawn_vehicle(&mut self, vehicle: &str) -> Result<u64> {
        let id = self
            .choice("Vehicle", vehicle)
            .with_context(|| format!("No {vehicle} in the vehicle list"))?;
        let yaw = self.app.controls.yaw;
        let before: std::collections::BTreeSet<u64> = self
            .app
            .network_view()
            .map(|v| v.world.bricks.keys().copied().collect())
            .unwrap_or_default();
        let mut planted = false;
        'search: for down in [0.35, 0.5, 0.7] {
            for eighth in 0..8 {
                self.aim(yaw + eighth as f32 * std::f32::consts::FRAC_PI_4, down)?;
                if self.plant("v20/brick/brickvehiclespawndata", (0, 0, 0))? {
                    planted = true;
                    break 'search;
                }
            }
        }
        ensure!(planted, "No vehicle spawn brick planted for {vehicle}");
        self.game(GameAction::CancelBrick)?;
        let brick = self
            .app
            .network_view()
            .and_then(|v| v.world.bricks.keys().copied().find(|b| !before.contains(b)))
            .context("the new spawn brick")?;
        let vehicles = self.vehicle_count();
        self.wrench(
            brick,
            WrenchVariant::VehicleSpawn,
            WrenchData {
                vehicle: Some(id.clone()),
                rendering: true,
                colliding: true,
                raycasting: true,
                item_dir: 2,
                item_respawn_ms: 4000,
                ..Default::default()
            },
        )?;
        let start = Instant::now();
        while self.vehicle_count() == vehicles && start.elapsed() < Duration::from_secs(5) {
            self.frame()?;
        }
        self.note(format!(
            "{vehicle} ({id}) on brick {brick}: vehicles {} -> {}",
            vehicles,
            self.vehicle_count()
        ));
        Ok(brick)
    }
    fn vehicle_count(&self) -> usize {
        self.app
            .network_view()
            .map_or(0, |v| v.vehicles.values().filter(|v| !v.destroyed).count())
    }
    /// The newest vehicle whose definition contains `name`, and where it is.
    fn vehicle(&self, name: &str) -> Option<(u64, Vec3)> {
        let view = self.app.network_view()?;
        let info = view
            .vehicles
            .values()
            .filter(|v| !v.destroyed && v.definition.to_lowercase().contains(name))
            .max_by_key(|v| v.id)?;
        let pose = view.vehicle_poses.get(&info.id)?;
        Some((info.id, Vec3::from(pose.position)))
    }
    fn mounted(&self) -> Option<u64> {
        let view = self.app.network_view()?;
        view.vitals.get(&view.owner)?.mounted.map(|(id, _)| id)
    }
    /// Run at `to` (jumping close by when `jump`) until within `within` or
    /// mounted, for at most `seconds`.
    fn run_to(&mut self, to: Vec3, within: f32, jump: bool, seconds: f32) -> Result<bool> {
        let start = Instant::now();
        let mut forward = false;
        let mut reached = false;
        while start.elapsed().as_secs_f32() < seconds {
            let Some(feet) = self.feet() else { break };
            let d = to - feet;
            let flat = Vec3::new(d.x, 0.0, d.z);
            if flat.length() < within || (jump && self.mounted().is_some()) {
                reached = true;
                break;
            }
            self.aim(d.x.atan2(-d.z), 0.1)?;
            if !forward {
                self.held(HeldControl::Forward, true)?;
                forward = true;
            }
            if jump && flat.length() < 2.5 {
                self.held(HeldControl::Jump, true)?;
            }
            self.frame()?;
        }
        if forward {
            self.held(HeldControl::Forward, false)?;
        }
        self.held(HeldControl::Jump, false)?;
        Ok(reached)
    }
    /// Get on the newest vehicle named `name`.
    fn mount(&mut self, name: &str) -> Result<bool> {
        let Some((id, at)) = self.vehicle(name) else {
            self.note(format!("no {name} to get on"));
            return Ok(false);
        };
        self.run_to(at, 0.3, true, 8.0)?;
        self.run(0.5)?;
        let on = self.mounted() == Some(id);
        self.note(format!("{name}: mounted {on}"));
        Ok(on)
    }
    /// Hold forward, turning `turn` radians a second, for `seconds`.
    fn drive(&mut self, seconds: f32, turn: f32) -> Result<()> {
        self.held(HeldControl::Forward, true)?;
        let start = Instant::now();
        let mut last = Instant::now();
        while start.elapsed().as_secs_f32() < seconds {
            let scale = self.app.controls.fov() / 90.0;
            let dt = last.elapsed().as_secs_f32();
            last = Instant::now();
            self.game(GameAction::Look {
                yaw: turn * dt / scale,
                pitch: 0.0,
            })?;
            self.frame()?;
        }
        self.held(HeldControl::Forward, false)
    }
    /// Make a mini-game whose loadout is the items named, in order.
    fn minigame(&mut self, items: &[&str]) -> Result<()> {
        let mut rules = MiniGameRules {
            title: "Rehearsal".into(),
            brick_damage: true,
            ..Default::default()
        };
        for (slot, name) in items.iter().enumerate().take(5) {
            match self.choice("ItemData", name) {
                Some(id) => rules.loadout[slot] = Some(id),
                None => self.note(format!("no item {name}")),
            }
        }
        self.request(UiAction::CreateMiniGame { color: 1, rules })?;
        self.run(2.0)?;
        let slots = self.slots();
        self.note(format!("mini-game loadout {slots:?}"));
        Ok(())
    }
    fn slots(&self) -> Vec<Option<String>> {
        self.app
            .network_view()
            .and_then(|v| v.tools.get(&v.owner).map(|t| t.slots.to_vec()))
            .unwrap_or_default()
    }
    /// Take out tool `slot` and fire it `shots` times, holding `hold` s.
    fn fire(&mut self, slot: usize, shots: usize, hold: f32) -> Result<()> {
        self.request(UiAction::UseTool { slot })?;
        self.run(0.6)?;
        for _ in 0..shots {
            self.held(HeldControl::Fire, true)?;
            self.run(hold)?;
            self.held(HeldControl::Fire, false)?;
            self.run(0.6)?;
        }
        Ok(())
    }
    fn chat_lines(&self, n: usize) -> Vec<String> {
        self.app
            .ui
            .core
            .chat
            .lines
            .iter()
            .rev()
            .take(n)
            .map(|l| l.text.clone())
            .collect()
    }

    /// Press and let go of a key, as the keyboard does.
    fn key(&mut self, key: bri_ui::input::Key) -> Result<()> {
        use bri_ui::input::{InputEvent, Modifiers};
        let mods = Modifiers::NONE;
        self.app.ui.handle_input(InputEvent::KeyDown {
            key,
            mods,
            repeat: false,
        });
        self.frame()?;
        self.app.ui.handle_input(InputEvent::KeyUp { key, mods });
        self.frame()
    }
    /// Change map as the admin's Change Map does, and wait to be standing
    /// in the new one, drawn.
    fn change_map(&mut self, map: &str) -> Result<()> {
        self.request(UiAction::Admin(
            bri_ui::models::admin::AdminAction::ChangeMap { map: map.into() },
        ))?;
        let map = map.to_string();
        let took = self.until("the new map", Duration::from_secs(300), |a| {
            a.network_view().is_some_and(|v| v.world.map_id == map)
                && a.scene_map() == Some(map.as_str())
                && in_game(a)
                && grounded(a)
                && a.world_render_ready()
        })?;
        self.note(format!("changed map to {map} in {:.0} ms", ms(took)));
        Ok(())
    }
    /// Run the day and night: a whole day in `length` seconds from `time`.
    fn day_cycle(&mut self, length: f32, time: f32) -> Result<()> {
        let mut settings = self
            .app
            .network_view()
            .map(|v| v.environment.clone())
            .unwrap_or_default();
        settings.day_cycle = Some(bri_content::atmosphere::DayCycle {
            length_seconds: length,
            time,
            anchor_tick: 0,
        });
        self.request(UiAction::Admin(
            bri_ui::models::admin::AdminAction::SetEnvironment {
                settings: Box::new(settings),
            },
        ))
    }
    /// Shots every `every` seconds for `seconds`.
    fn watch(&mut self, name: &str, seconds: f32, every: f32) -> Result<()> {
        let start = Instant::now();
        let mut n = 0;
        while start.elapsed().as_secs_f32() < seconds {
            self.run(every)?;
            self.shot(&format!("{name}-{n:02}"))?;
            n += 1;
        }
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        self.beat("end");
        self.beats.pop();
        self.write()?;
        let _ = self.request(UiAction::Disconnect);
        let _ = std::fs::remove_dir_all(&self.state);
        println!("report {}", self.out.join("report.json").display());
        Ok(())
    }

    fn host(&mut self, map: &str, name: &str) -> Result<()> {
        self.request(UiAction::HostGame {
            map: map.into(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 1,
            server_name: name.into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        })?;
        let took = self.until("in game", Duration::from_secs(300), |a| {
            in_game(a) && grounded(a) && a.world_render_ready()
        })?;
        self.note(format!("hosted {map} in {:.0} ms", ms(took)));
        // Aim from the eye, as first person does; the player's own
        // settings may start in third person.
        if self.app.controls.third_person {
            self.game(GameAction::ToggleFirstPerson { fast: true })?;
            self.run(0.5)?;
        }
        Ok(())
    }

    fn feet(&self) -> Option<Vec3> {
        self.app.presented_local().map(|p| Vec3::from(p.feet))
    }
    fn bricks(&self) -> usize {
        self.app
            .network_view()
            .map_or(0, |v| v.world.bricks.len())
    }

    /// Hold a brick, aim at the floor ahead and plant it. Returns whether a
    /// brick was added.
    fn plant(&mut self, brick: &str, shift: (i32, i32, i32)) -> Result<bool> {
        let before = self.bricks();
        self.request(UiAction::InstantUseBrick {
            brick: brick.into(),
        })?;
        self.run(0.1)?;
        if self.app.building().and_then(|b| b.ghost()).is_none() {
            self.press(HeldControl::Fire)?;
        }
        if self.app.building().and_then(|b| b.ghost()).is_none() {
            return Ok(false);
        }
        let (x, y, z) = shift;
        if (x, y, z) != (0, 0, 0) {
            self.game(GameAction::ShiftBrick { x, y, z })?;
            self.run(0.05)?;
        }
        self.game(GameAction::PlantBrick)?;
        let start = Instant::now();
        while self.bricks() == before && start.elapsed() < Duration::from_secs(3) {
            self.frame()?;
        }
        Ok(self.bricks() > before)
    }
}

/// Add-Ons the script uses that a release ships turned off.
const TURN_ON: &[&str] = &[
    "blockhead_bot",
    "ragdoll",
    "brick_portal",
    "gravity-gun",
    "gravity-gun-tool",
    "gravity-gun-fx",
    "steel-ball",
    "steel-ball-kit",
    "steel-ball-fx",
];

/// Convert the classic Add-Ons dropped in the content's Add-Ons folder, as
/// opening Add-Ons does (with the release's own bri-import-addon), and turn
/// on the script's Add-Ons and every converted one. Only when the content
/// is a packaged build (BRI_CONTENT with the importer beside it).
fn prepare_add_ons(content: &Path) -> Result<()> {
    let Some(game) = content.parent() else {
        return Ok(());
    };
    let importer = game.join("bri-import-addon.exe");
    if !importer.is_file() {
        return Ok(());
    }
    let notes = bri_client::add_ons::start_sync(content, &importer, false)?;
    loop {
        let note = notes.recv_timeout(Duration::from_secs(600))?;
        if note.finished {
            println!("Add-On sync: {:?}", note.notice);
            break;
        }
    }
    let view = bri_client::add_ons::view(content);
    let converted: Vec<String> = view
        .rows
        .iter()
        .filter(|r| !r.enabled && !r.id.starts_with("legacy:"))
        .filter(|r| {
            let n = r.name.to_lowercase();
            n.contains("sniper") || n.contains("grenade") || n.contains("duplicator")
        })
        .map(|r| r.id.clone())
        .collect();
    for id in TURN_ON.iter().map(|s| s.to_string()).chain(converted) {
        match bri_client::add_ons::set_enabled(content, &id, true) {
            Ok(_) => println!("enabled {id}"),
            Err(e) => println!("could not enable {id}: {e:#}"),
        }
    }
    Ok(())
}

/// The stack and the top screen's visible texts.
fn describe(app: &App) -> String {
    let mut out = format!("stack {:?}\n", app.ui.stack());
    if let Some(screen) = app.ui.screen(app.ui.top_id()) {
        let v = screen.view();
        for n in v.walk() {
            let text = v.text_of(n);
            if v.is_shown(n) && !text.trim().is_empty() {
                out.push_str(&format!("  {:?}\n", text.trim()));
            }
        }
    }
    out
}

fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}
fn grounded(app: &App) -> bool {
    app.network_view()
        .is_some_and(|v| v.poses.get(&v.owner).is_some_and(|p| p.player.grounded))
}

/// Opening, Bedroom and building: startup to the main menu, hosting
/// Bedroom, a look around, bricks, brick search, spray paint and FX cans,
/// printer, hammer, wrench light/emitter/item/music and the avatar editor.
#[test]
#[ignore = "real GPU and generated content (BRI_REHEARSAL_OUT); no window"]
fn bedroom_building() -> Result<()> {
    let Some(mut r) = Rehearsal::new("bedroom")? else {
        return Ok(());
    };
    r.app.ui.core.pop(ScreenId::DefaultControls);
    r.run(2.0)?;
    r.shot("main-menu")?;
    r.beat("host-bedroom");
    r.request(UiAction::RefreshHostColorsets)?;
    r.app.ui.core.push(ScreenId::StartMission);
    r.run(0.5)?;
    r.shot("start-game")?;
    r.app.ui.core.pop(ScreenId::StartMission);
    r.host(BEDROOM, "Bedroom")?;
    r.shot("spawn")?;
    r.beat("look-around");
    r.pan(std::f32::consts::TAU, 8.0)?;
    r.shot("after-look")?;

    r.beat("build");
    let yaw = r.app.controls.yaw;
    r.aim(yaw, 0.6)?;
    let mut planted = usize::from(r.plant(BRICK, (0, 0, 0))?);
    // A little wall: the ghost stays where it was planted, so shift it.
    for (i, shift) in [(0, 0, 3), (0, 4, -3), (0, 0, 3), (0, -8, -3), (0, 0, 3), (0, 0, 3)]
        .into_iter()
        .enumerate()
    {
        let before = r.bricks();
        let (x, y, z) = shift;
        r.game(GameAction::ShiftBrick { x, y, z })?;
        r.run(0.1)?;
        r.game(GameAction::PlantBrick)?;
        r.run(0.4)?;
        if r.bricks() > before {
            planted += 1;
        } else {
            r.note(format!("plant {i} after shift {shift:?} added no brick"));
        }
    }
    r.game(GameAction::CancelBrick)?;
    r.note(format!("planted {planted} bricks"));
    r.shot("built")?;

    r.beat("brick-search");
    r.app.ui.core.push(ScreenId::BrickSelector);
    r.run(0.5)?;
    r.shot("selector")?;
    r.app.ui.core.selector.search = "arch".into();
    r.app.ui.core.pop(ScreenId::BrickSelector);
    r.app.ui.core.push(ScreenId::BrickSelector);
    r.run(0.5)?;
    r.shot("search-arch")?;
    r.app.ui.core.selector.search.clear();
    r.app.ui.core.pop(ScreenId::BrickSelector);
    r.run(0.2)?;

    r.beat("spray-paint");
    r.aim(yaw, 0.6)?;
    for (n, color) in [0u32, 3, 6].into_iter().enumerate() {
        r.request(UiAction::UseSprayCan { color })?;
        r.run(0.3)?;
        r.press(HeldControl::Fire)?;
        r.run(0.4)?;
        if n == 0 {
            r.shot("spray-can")?;
        }
    }
    for (fx, name) in [(1u32, "pearl"), (2, "chrome"), (3, "glow"), (6, "rainbow")] {
        r.request(UiAction::UseFxCan { fx })?;
        r.run(0.3)?;
        r.press(HeldControl::Fire)?;
        r.run(0.6)?;
        r.shot(&format!("fx-{name}"))?;
    }
    r.request(UiAction::UnUseTool)?;
    r.run(0.3)?;

    r.beat("tools");
    let slots: Vec<Option<String>> = r
        .app
        .network_view()
        .and_then(|v| v.tools.get(&v.owner).map(|t| t.slots.to_vec()))
        .unwrap_or_default();
    r.note(format!("tool slots {slots:?}"));
    for (slot, item) in slots.iter().enumerate() {
        let Some(item) = item.clone() else { continue };
        r.request(UiAction::UseTool { slot })?;
        r.run(0.6)?;
        r.press(HeldControl::Fire)?;
        r.run(0.8)?;
        let short = item.rsplit(['.', '/', ':']).next().unwrap_or("tool").to_string();
        r.shot(&format!("tool-{short}"))?;
        if r.app.ui.screen(ScreenId::PrintSelector).is_some() {
            r.note("printer opened the print selector");
            r.app.ui.core.pop(ScreenId::PrintSelector);
        }
        for variant in [
            WrenchVariant::Normal,
            WrenchVariant::Sound,
            WrenchVariant::VehicleSpawn,
        ] {
            if r.app.ui.screen(ScreenId::Wrench(variant)).is_some() {
                r.note(format!("{short} opened the {variant:?} wrench"));
                r.shot("wrench-dialog")?;
                r.app.ui.core.pop(ScreenId::Wrench(variant));
            }
        }
    }
    r.request(UiAction::UnUseTool)?;
    r.run(0.3)?;

    r.beat("wrench-light-emitter-item-music");
    let categories: Vec<(String, usize)> = r
        .app
        .ui
        .core
        .datablocks
        .iter()
        .map(|(k, v)| (k.clone(), v.len()))
        .collect();
    r.note(format!("datablock lists {categories:?}"));
    let pick = |r: &Rehearsal, list: &str, want: &str| -> Option<String> {
        r.app.ui.core.datablocks.get(list).and_then(|c| {
            c.iter()
                .find(|c| c.id.to_lowercase().contains(want) || c.name.to_lowercase().contains(want))
                .map(|c| c.id.clone())
        })
    };
    let light = pick(&r, "FxLightData", "red").or_else(|| pick(&r, "FxLightData", "light"));
    let emitter = pick(&r, "ParticleEmitterData", "fire")
        .or_else(|| pick(&r, "ParticleEmitterData", "emitter"));
    let item = pick(&r, "ItemData", "sword");
    let music = pick(&r, "Music", "rock").or_else(|| pick(&r, "Music", ""));
    r.note(format!("light {light:?} emitter {emitter:?} item {item:?} music {music:?}"));
    let ids: Vec<u64> = r
        .app
        .network_view()
        .map(|v| v.world.bricks.keys().copied().collect())
        .unwrap_or_default();
    if let [a, b, c, ..] = ids[..] {
        let mut data = WrenchData {
            rendering: true,
            colliding: true,
            raycasting: true,
            item_dir: 2,
            item_respawn_ms: 4000,
            ..Default::default()
        };
        data.light = light;
        r.wrench(a, WrenchVariant::Normal, data.clone())?;
        r.run(1.0)?;
        r.shot("light")?;
        let mut data = WrenchData {
            emitter,
            ..data
        };
        data.light = None;
        r.wrench(b, WrenchVariant::Normal, data.clone())?;
        r.run(1.5)?;
        r.shot("emitter")?;
        let data = WrenchData {
            emitter: None,
            item,
            item_pos: 0,
            item_dir: 2,
            ..data
        };
        r.wrench(c, WrenchVariant::Normal, data.clone())?;
        r.run(1.0)?;
        r.shot("item")?;
        let data = WrenchData {
            item: None,
            sound: music,
            ..data
        };
        // Music goes on a music brick, whose wrench is the sound one.
        let before = ids.len();
        r.aim(yaw + 0.5, 0.6)?;
        r.plant("v20/brick/brickmusicdata", (0, 0, 0))?;
        r.game(GameAction::CancelBrick)?;
        let music_brick = r.app.network_view().and_then(|v| {
            v.world
                .bricks
                .keys()
                .copied()
                .find(|b| !ids.contains(b))
        });
        r.note(format!("music brick {music_brick:?} ({before} bricks before)"));
        if let Some(d) = music_brick {
            r.wrench(d, WrenchVariant::Sound, data)?;
            r.run(2.0)?;
            let music = r.app.network_view().and_then(|v| {
                v.world.bricks.get(&d).map(|b| format!("{:?}", b.sound))
            });
            r.note(format!("music brick plays {music:?}"));
            r.shot("music")?;
        }
        let lit = r
            .app
            .network_view()
            .map(|v| {
                let w = &v.world.bricks;
                (
                    w.get(&a).is_some_and(|b| b.light.is_some()),
                    w.get(&b).is_some_and(|b| b.emitter.is_some()),
                    w.get(&c).is_some_and(|b| b.item_spawn.item.is_some()),
                )
            })
            .unwrap_or_default();
        r.note(format!("light, emitter, item set: {lit:?}"));
    } else {
        r.note(format!("only {} bricks to wrench", ids.len()));
    }

    r.beat("duplicator");
    r.chat_command("dup", &[])?;
    r.run(1.0)?;
    let slots = r.slots();
    r.note(format!("tools after /dup {slots:?}"));
    if let Some(slot) = slots
        .iter()
        .position(|s| s.as_deref().is_some_and(|s| s.to_lowercase().contains("dup")))
    {
        r.request(UiAction::UseTool { slot })?;
        r.run(0.6)?;
        r.aim(yaw, 0.6)?;
        r.press(HeldControl::Fire)?;
        r.run(1.0)?;
        r.shot("dup-selected")?;
        // The first brick key takes the selection up as a ghost; then move
        // it clear of the original and plant.
        r.game(GameAction::ShiftBrick { x: 0, y: 0, z: 0 })?;
        r.run(0.5)?;
        for (n, shift) in [(10, 0, 0), (0, 0, 15), (0, 10, 0)].into_iter().enumerate() {
            let before = r.bricks();
            let (x, y, z) = shift;
            r.game(GameAction::ShiftBrick { x, y, z })?;
            r.run(0.2)?;
            r.game(GameAction::PlantBrick)?;
            r.run(3.0)?;
            r.note(format!("duplicate {n}: bricks {before} -> {}", r.bricks()));
            r.shot(&format!("dup-placed-{n}"))?;
        }
        r.game(GameAction::CancelBrick)?;
        r.request(UiAction::UnUseTool)?;
        r.run(0.3)?;
    }
    r.note(format!("chat {:?}", r.chat_lines(6)));

    r.beat("avatar-randomize");
    r.app.ui.core.push(ScreenId::Avatar);
    r.run(1.0)?;
    r.shot("avatar")?;
    for n in 0..4 {
        let top = r.app.ui.top_id();
        let Some((x, y)) = r
            .app
            .ui
            .control_center(top, "Randomize!")
        else {
            r.note("no Randomize control on the avatar screen");
            break;
        };
        use bri_ui::input::{InputEvent, MouseButton};
        r.app.ui.handle_input(InputEvent::MouseMove { x, y });
        r.app.ui.handle_input(InputEvent::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        });
        r.app.ui.handle_input(InputEvent::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        });
        r.run(0.8)?;
        r.shot(&format!("randomize-{n}"))?;
    }
    r.app.ui.core.pop(ScreenId::Avatar);
    r.run(1.0)?;
    r.shot("after-avatar")?;
    r.finish()
}

/// How long the scene's pipelines take to compile on this machine's GPU
/// (the wait between launch and the first frame of a map).
#[test]
#[ignore = "real GPU (BRI_REHEARSAL_OUT); no window"]
fn scene_pipeline_compile_time() -> Result<()> {
    if std::env::var_os("BRI_REHEARSAL_OUT").is_none() {
        return Ok(());
    }
    let gpu = gpu()?;
    let start = Instant::now();
    let _scene = bri_render::scene::SceneRenderer::new(&gpu.device, FORMAT);
    println!(
        "{}: scene pipelines {:.0} ms",
        gpu.name,
        ms(start.elapsed())
    );
    Ok(())
}

/// ACM City and the action beats: the save loading in Bedroom, the Jeep,
/// Horse and Tank, a mini-game with the rocket launcher on bricks, a bot
/// fought with sword, hammer, spear and rocket, the sniper rifle and HE
/// grenade, the Gravity Gun, ragdolls and the admin camera (F8).
#[test]
#[ignore = "real GPU and generated content (BRI_REHEARSAL_OUT); no window"]
fn city_vehicles_weapons() -> Result<()> {
    let Some(mut r) = Rehearsal::new("city")? else {
        return Ok(());
    };
    r.app.ui.core.pop(ScreenId::DefaultControls);
    r.host(BEDROOM, "City")?;
    r.beat("load-acm-city");
    let bricks = r.load_save("ACM City")?;
    r.note(format!("ACM City {bricks} bricks"));
    r.overview()?;
    r.shot("acm-city")?;
    r.camera_back()?;
    r.beat("walk-city");
    r.pan(1.5, 3.0)?;
    let yaw = r.app.controls.yaw;
    r.held(HeldControl::Forward, true)?;
    r.run(3.0)?;
    r.held(HeldControl::Forward, false)?;
    r.shot("walked")?;

    for (vehicle, name, seconds) in [
        ("Jeep", "jeep", 6.0),
        ("Horse", "horse", 4.0),
        ("Tank", "tank", 4.0),
    ] {
        r.beat(&format!("vehicle-{name}"));
        r.aim(yaw, 0.0)?;
        r.spawn_vehicle(vehicle)?;
        r.run(1.0)?;
        r.shot(&format!("{name}-spawned"))?;
        if r.mount(name)? {
            r.drive(seconds, 0.4)?;
            r.shot(&format!("{name}-driving"))?;
            if name == "tank" {
                r.press(HeldControl::Fire)?;
                r.run(1.5)?;
                r.shot("tank-fired")?;
            }
            r.press(HeldControl::Jet)?;
            r.run(1.0)?;
            r.note(format!("after dismount mounted {:?}", r.mounted()));
        }
    }

    r.beat("minigame-rockets");
    r.minigame(&["rocket", "sword", "hammer", "spear", "sniper"])?;
    let slots = r.slots();
    let slot_of = |want: &str| {
        slots
            .iter()
            .position(|s| s.as_deref().is_some_and(|s| s.to_lowercase().contains(want)))
    };
    // Rockets at the city's bricks.
    if let Some(rocket) = slot_of("rocket") {
        r.aim(yaw + std::f32::consts::PI, -0.05)?;
        r.fire(rocket, 3, 0.1)?;
        r.run(1.0)?;
        r.shot("rockets")?;
    }
    r.beat("bot-fight");
    r.aim(yaw, 0.0)?;
    r.spawn_vehicle("Blockhead Bot")?;
    r.run(4.0)?;
    r.shot("bot")?;
    for weapon in ["sword", "hammer", "spear", "rocket"] {
        if let Some(slot) = slot_of(weapon) {
            r.fire(slot, 2, 0.1)?;
            r.shot(&format!("bot-{weapon}"))?;
        }
    }
    r.run(3.0)?;
    r.shot("bot-after")?;
    r.beat("sniper");
    if let Some(slot) = slot_of("sniper") {
        r.request(UiAction::UseTool { slot })?;
        r.run(0.6)?;
        r.held(HeldControl::Zoom, true)?;
        r.run(1.0)?;
        r.shot("sniper-zoom")?;
        r.press(HeldControl::Fire)?;
        r.run(0.5)?;
        r.held(HeldControl::Zoom, false)?;
        r.run(0.5)?;
    }
    r.beat("he-grenade");
    let game = r
        .app
        .network_view()
        .and_then(|v| v.minigames.first().map(|g| g.id));
    if let Some(game) = game {
        r.request(UiAction::EndMiniGame {
            game: bri_ui::api::MiniGameId(game),
        })?;
        r.run(1.0)?;
    }
    r.minigame(&["grenade", "rocket"])?;
    let slots = r.slots();
    if let Some(slot) = slots
        .iter()
        .position(|s| s.as_deref().is_some_and(|s| s.to_lowercase().contains("grenade")))
    {
        r.aim(yaw, -0.2)?;
        r.fire(slot, 1, 0.8)?;
        r.run(3.0)?;
        r.shot("he-grenade")?;
    }
    r.beat("gravity-gun-ragdoll");
    let game = r
        .app
        .network_view()
        .and_then(|v| v.minigames.first().map(|g| g.id));
    if let Some(game) = game {
        r.request(UiAction::EndMiniGame {
            game: bri_ui::api::MiniGameId(game),
        })?;
        r.run(1.0)?;
    }
    let slots = r.slots();
    r.note(format!("tools outside the mini-game {slots:?}"));
    r.aim(yaw, 0.0)?;
    r.spawn_vehicle("Steel Ball")?;
    r.run(1.0)?;
    if let Some(slot) = slots
        .iter()
        .position(|s| s.as_deref().is_some_and(|s| s.contains("gravity")))
    {
        r.request(UiAction::UseTool { slot })?;
        r.run(0.6)?;
        r.aim(yaw, 0.3)?;
        r.held(HeldControl::Fire, true)?;
        r.run(1.5)?;
        r.shot("gravity-grab")?;
        r.pan(1.0, 1.0)?;
        r.held(HeldControl::Fire, false)?;
        r.run(2.0)?;
        r.shot("gravity-throw")?;
    }
    r.beat("admin-camera");
    r.game(GameAction::DropCameraAtPlayer)?;
    r.run(0.5)?;
    r.held(HeldControl::Backward, true)?;
    r.run(1.5)?;
    r.held(HeldControl::Backward, false)?;
    r.held(HeldControl::Jump, true)?;
    r.run(1.0)?;
    r.held(HeldControl::Jump, false)?;
    r.pan(1.0, 2.0)?;
    r.shot("admin-camera")?;
    r.camera_back()?;
    r.run(1.0)?;
    r.shot("back-to-player")?;
    r.note(format!("chat {:?}", r.chat_lines(8)));
    r.finish()
}

/// Pong and the event editor, MoTE Mansion's garden and the Kitchen from
/// day into night, the Slopes with skis and the Stunt Plane, Skylands, and
/// the big Badspot Block Party save with the F3 overlay.
#[test]
#[ignore = "real GPU and generated content (BRI_REHEARSAL_OUT); no window"]
fn maps_lighting_big_build() -> Result<()> {
    let Some(mut r) = Rehearsal::new("maps")? else {
        return Ok(());
    };
    r.app.ui.core.pop(ScreenId::DefaultControls);
    r.copy_saves(&["Slate/Badspot's Block Party1.bls"])?;
    r.host(BEDROOM, "Maps")?;

    r.beat("pong");
    r.load_save("Pong")?;
    r.overview()?;
    r.shot("pong")?;
    r.camera_back()?;
    let evented = r.app.network_view().and_then(|v| {
        v.world
            .bricks
            .iter()
            .find(|(_, b)| !b.events.is_empty())
            .map(|(id, _)| *id)
    });
    if let Some(brick) = evented {
        // The Events button of the brick's wrench dialog.
        if let (Some(slot), Some(at)) = (
            r.slots()
                .iter()
                .position(|s| s.as_deref().is_some_and(|s| s.contains("wrench"))),
            r.brick_position(brick),
        ) {
            r.request(UiAction::UseTool { slot })?;
            r.run(0.4)?;
            r.aim_at(at)?;
            r.press(HeldControl::Fire)?;
            r.run(1.0)?;
        }
        r.request(UiAction::RequestEvents { brick })?;
        r.run(1.0)?;
        r.note(format!("event editor open: {:?}", r.app.ui.stack()));
        r.shot("event-editor")?;
        r.app.ui.core.pop(ScreenId::WrenchEvents);
        r.run(0.3)?;
    }
    r.request(UiAction::Admin(
        bri_ui::models::admin::AdminAction::ClearAllBricks,
    ))
    .ok();
    r.run(2.0)?;

    r.beat("mansion-day-night");
    r.load_save("Mansion")?;
    r.overview()?;
    r.shot("mansion-day")?;
    r.day_cycle(60.0, 0.6)?;
    r.watch("mansion", 30.0, 3.0)?;
    r.camera_back()?;

    r.beat("kitchen-day-night");
    r.change_map("v20/add-ons/map_kitchen/kitchen.mis")?;
    r.day_cycle(60.0, 0.6)?;
    r.watch("kitchen", 30.0, 3.0)?;
    r.game(GameAction::UseLight)?;
    r.run(1.0)?;
    r.shot("kitchen-player-light")?;

    r.beat("slopes");
    r.change_map("v20/add-ons/map_slopes/slopes.mis")?;
    r.shot("slopes")?;
    r.minigame(&["ski"])?;
    if let Some(slot) = r
        .slots()
        .iter()
        .position(|s| s.as_deref().is_some_and(|s| s.to_lowercase().contains("ski")))
    {
        r.fire(slot, 1, 0.1)?;
        r.held(HeldControl::Forward, true)?;
        r.watch("skiing", 6.0, 2.0)?;
        r.held(HeldControl::Forward, false)?;
    }
    let game = r
        .app
        .network_view()
        .and_then(|v| v.minigames.first().map(|g| g.id));
    if let Some(game) = game {
        r.request(UiAction::EndMiniGame {
            game: bri_ui::api::MiniGameId(game),
        })?;
        r.run(1.0)?;
    }
    r.beat("stunt-plane");
    r.spawn_vehicle("Stunt Plane")?;
    r.run(1.0)?;
    if r.mount("plane")? {
        r.held(HeldControl::Forward, true)?;
        r.watch("flying", 8.0, 2.0)?;
        r.held(HeldControl::Forward, false)?;
    }

    r.beat("skylands");
    r.change_map("v20/add-ons/map_skylands/skylands.mis")?;
    r.shot("skylands")?;
    r.pan(std::f32::consts::TAU, 8.0)?;
    r.shot("skylands-around")?;

    r.beat("badspot-block-party");
    r.change_map("v20/add-ons/map_slate/slate.mis")?;
    let bricks = r.load_save("Block Party1")?;
    r.note(format!("Badspot's Block Party {bricks} bricks"));
    r.shot("block-party")?;
    r.beat("badspot-overview");
    r.overview()?;
    r.run(6.0)?;
    r.shot("block-party-overview")?;
    r.beat("f3-overlay");
    r.key(bri_ui::input::Key::F(3))?;
    r.pan(1.0, 4.0)?;
    r.shot("f3")?;
    r.finish()
}

/// The credits: three players on one LAN game (two headless guests join the
/// rehearsing host), the player list, emotes (/love, /hug, /hate, /alarm,
/// /confusion, /wtf), jumping, a hammer swing and spray paint, all seen
/// from the host.
#[test]
#[ignore = "real GPU, generated content and UDP 28000 (BRI_REHEARSAL_OUT); no window"]
fn credits_three_players() -> Result<()> {
    let Some(mut r) = Rehearsal::new("credits")? else {
        return Ok(());
    };
    r.app.ui.core.pop(ScreenId::DefaultControls);
    let content = r.app.content.paths.root.clone();
    for name in ["Guest One", "Guest Two"] {
        let state = r.out.join(format!("state-{}", name.replace(' ', "-")));
        std::fs::create_dir_all(&state)?;
        let mut guest = App::load(&content, &state, (640, 480))?;
        guest.ui.core.pop(ScreenId::DefaultControls);
        guest.ui.core.settings.avatar.lan_name = name.into();
        r.others.push(guest);
    }
    r.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::Lan,
        game_mode: None,
        max_players: 8,
        server_name: "Credits".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    })?;
    r.until("host in game", Duration::from_secs(300), |a| {
        in_game(a) && grounded(a) && a.world_render_ready()
    })?;
    r.beat("players-join");
    for i in 0..r.others.len() {
        r.other(
            i,
            UiAction::JoinServer {
                address: "127.0.0.1".into(),
                password: String::new(),
            },
        )?;
        let want = i + 2;
        r.until("a guest joins", Duration::from_secs(300), |a| {
            a.ui.core.players.len() == want
        })?;
        r.run(1.0)?;
        r.shot(&format!("joined-{i}"))?;
    }
    let others_in = r.others.iter().all(|g| in_game(g));
    r.note(format!("guests in game {others_in}"));
    r.run(3.0)?;
    r.app.ui.core.push(ScreenId::PlayerList);
    r.run(0.8)?;
    r.shot("player-list")?;
    r.app.ui.core.pop(ScreenId::PlayerList);
    // Turn round to face the guests, who spawn near the host.
    r.pan(std::f32::consts::PI, 1.5)?;
    r.shot("facing-guests")?;
    r.beat("emotes");
    for (i, emote) in [(0, "love"), (1, "hug"), (0, "hate"), (1, "alarm"), (0, "confusion"), (1, "wtf")] {
        r.other(
            i,
            UiAction::ChatCommand {
                name: emote.into(),
                args: vec![],
            },
        )?;
        r.run(0.6)?;
        r.shot(&format!("emote-{emote}"))?;
    }
    r.chat_command("hug", &[])?;
    r.run(0.8)?;
    r.shot("host-hug")?;
    r.beat("jumping-and-tools");
    for i in 0..r.others.len() {
        r.other(i, UiAction::Game(GameAction::Held { control: HeldControl::Jump, down: true }))?;
    }
    r.run(1.5)?;
    r.shot("guests-jumping")?;
    for i in 0..r.others.len() {
        r.other(i, UiAction::Game(GameAction::Held { control: HeldControl::Jump, down: false }))?;
        r.other(i, UiAction::UseSprayCan { color: 3 + i as u32 })?;
        r.other(i, UiAction::Game(GameAction::Held { control: HeldControl::Fire, down: true }))?;
    }
    r.run(1.0)?;
    r.shot("guests-spray")?;
    for i in 0..r.others.len() {
        r.other(i, UiAction::Game(GameAction::Held { control: HeldControl::Fire, down: false }))?;
        r.other(i, UiAction::UseTool { slot: 0 })?;
        r.other(i, UiAction::Game(GameAction::Held { control: HeldControl::Fire, down: true }))?;
    }
    r.run(1.2)?;
    r.shot("guests-hammer")?;
    for i in 0..r.others.len() {
        r.other(i, UiAction::Game(GameAction::Held { control: HeldControl::Fire, down: false }))?;
    }
    r.run(2.0)?;
    r.note(format!("chat {:?}", r.chat_lines(8)));
    for i in 0..r.others.len() {
        let _ = r.other(i, UiAction::Disconnect);
    }
    r.run(1.0)?;
    r.finish()
}
