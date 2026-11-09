//! Explicit offscreen selection-to-first-live-frame timing. No window/input;
//! BRI_LOAD_CONTENT, BRI_LOAD_MAP and BRI_LOAD_OUT name external inputs/output.
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::{ConnectionState, ServerMode, UiAction},
    gpu::{Headless, UiRenderer},
    screens::ScreenId,
};
use serde_json::json;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

// Match the normal client's initial framebuffer and allocation strategy.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
const SIZE: (u32, u32) = (1280, 720);
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const HANG: Duration = Duration::from_secs(120);

fn draw(
    app: &mut App,
    gpu: &Headless,
    ui: &mut UiRenderer,
    target: &wgpu::TextureView,
) -> Result<bool> {
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let drawn = app.render_scene(&mut RenderContext {
        device: &gpu.device,
        queue: &gpu.queue,
        encoder: &mut encoder,
        target,
        format: FORMAT,
        size: SIZE,
        ui_renderer: ui,
    })?;
    gpu.queue.submit([encoder.finish()]);
    gpu.device.poll(wgpu::PollType::Poll)?;
    Ok(drawn)
}

fn step(app: &mut App, elapsed: Duration) -> Result<()> {
    app.tick(elapsed)?;
    app.ui.update(elapsed.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "unexpected native window command");
    if let ConnectionState::Failed { reason } = &app.ui.core.conn {
        anyhow::bail!("load failed: {reason}");
    }
    Ok(())
}

fn main() -> Result<()> {
    bri_net::allocator::tune();
    let root = PathBuf::from(std::env::var_os("BRI_LOAD_CONTENT").context("set BRI_LOAD_CONTENT")?);
    let out = PathBuf::from(std::env::var_os("BRI_LOAD_OUT").context("set BRI_LOAD_OUT")?);
    let map = std::env::var("BRI_LOAD_MAP").context("set BRI_LOAD_MAP")?;
    // This standalone probe sets ports before any server/runtime starts.
    unsafe {
        std::env::set_var("BRI_TEST_HOST_PORT", "0");
        std::env::set_var("BRI_TEST_DISCOVERY_PORT", "0");
    }
    std::fs::create_dir_all(&out)?;
    let state = out.join(format!("state-{}", std::process::id()));
    ensure!(
        !state.exists(),
        "state must be fresh for a first-process measurement"
    );
    let opened = Instant::now();
    let mut app = App::load(&root, &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    let startup_ms = opened.elapsed().as_secs_f64() * 1000.;
    let gpu_started = Instant::now();
    let gpu = bri_client::platform::offscreen_gpu()?;
    let mut ui = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, FORMAT)?;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("map-load first-frame probe"),
        size: wgpu::Extent3d {
            width: SIZE.0,
            height: SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target = target.create_view(&Default::default());
    draw(&mut app, &gpu, &mut ui, &target)?;
    let gpu_init_ms = gpu_started.elapsed().as_secs_f64() * 1000.;
    let mut runs = Vec::new();
    for index in 0..3 {
        bri_console::log::clear();
        let started = Instant::now();
        let mut previous = started;
        app.ui.core.request(UiAction::HostGame {
            map: map.clone(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 4,
            server_name: "Offscreen load timing".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        });
        let mut entered_ms = None;
        loop {
            ensure!(
                started.elapsed() < HANG,
                "map load did not finish: {:?}",
                app.ui.core.conn
            );
            let now = Instant::now();
            step(&mut app, now - previous)?;
            previous = now;
            let entered = matches!(app.ui.core.conn, ConnectionState::InGame { .. });
            if entered {
                entered_ms.get_or_insert_with(|| started.elapsed().as_secs_f64() * 1000.);
            }
            let rendered = draw(&mut app, &gpu, &mut ui, &target)?;
            if entered && rendered {
                gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let first_frame_ms = started.elapsed().as_secs_f64() * 1000.;
        ensure!(
            app.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner)),
            "no live player pose"
        );
        let phases: Vec<_> = bri_console::log::lines()
            .into_iter()
            .filter(|l| l.text.starts_with("Map preparation:") || l.text.starts_with("Loading "))
            .map(|l| l.text)
            .collect();
        runs.push(json!({"index":index,"entered_ms":entered_ms,"first_frame_ms":first_frame_ms,"phases":phases}));
        app.ui.core.request(UiAction::Disconnect);
        let leaving = Instant::now();
        while app.network_view().is_some() {
            ensure!(leaving.elapsed() < HANG, "disconnect did not finish");
            step(&mut app, Duration::from_millis(1))?;
            std::thread::sleep(Duration::from_millis(1));
        }
        // Dispatch the queued disconnect even if the view was already absent.
        step(&mut app, Duration::ZERO)?;
    }
    let report = json!({"map":map,"content":root,"startup_ms":startup_ms,"gpu_init_ms":gpu_init_ms,"size":[SIZE.0,SIZE.1],
        "gpu":format!("{:?}",gpu.adapter_info),"runs":runs,
        "scope":"Fresh process, warm filesystem allowed; default private app state. Timing starts at typed map selection and ends after InGame plus a completed offscreen live-player frame. No window, OS input, saves or interactive acceptance."});
    std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    eprintln!("LOAD_TIMING {report}");
    Ok(())
}
