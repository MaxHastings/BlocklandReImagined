//! Bounded normal-App render probe for original held tools.
//! Run with: cargo test -p bri-client --test app_item_render --release -- --ignored --nocapture
//! Requires converted v20 content, loopback QUIC and an offscreen GPU; never opens a window.
use anyhow::{Context, Result, ensure};
use bri_client::{app::App, platform::{PlatformApp, RenderContext}};
use bri_ui::{api::*, gpu::{Headless, UiRenderer}};
use std::{path::Path, thread, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

const SIZE: (u32, u32) = (640, 480);
const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";

fn pump(app: &mut App) -> Result<()> {
    ensure!(app.pump()?.is_empty(), "Unexpected native window command in offscreen test");
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
        if ready(app) { return Ok(()); }
        ensure!(start.elapsed() < Duration::from_secs(45), "Timed out waiting for {what}: {:?}", app.ui.core.conn);
        thread::sleep(Duration::from_millis(10));
    }
}
fn capture(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("native held-tool offscreen target"), size: extent, mip_level_count: 1,
        sample_count: 1, dimension: wgpu::TextureDimension::D2, format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    ensure!(app.render_scene(&mut RenderContext { device: &gpu.device, queue: &gpu.queue,
        encoder: &mut encoder, target: &view, format, size: SIZE, ui_renderer: renderer })?,
        "App did not render a session camera");
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("held-tool readback"),
        size: u64::from(row) * u64::from(SIZE.1), usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false });
    encoder.copy_texture_to_buffer(texture.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buffer,
        layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(SIZE.1) } }, extent);
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
    gpu.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(30)) })?;
    rx.recv_timeout(Duration::from_secs(5))??;
    let mapped = buffer.slice(..).get_mapped_range().map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((SIZE.0 * SIZE.1 * 4) as usize);
    for line in mapped.chunks_exact(row as usize) { pixels.extend_from_slice(&line[..SIZE.0 as usize * 4]); }
    drop(mapped); buffer.unmap(); Ok(pixels)
}
fn save(path: &Path, data: &[u8]) -> Result<()> {
    image::save_buffer(path, data, SIZE.0, SIZE.1, image::ColorType::Rgba8)?; Ok(())
}
fn changed_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4).zip(b.chunks_exact(4)).filter(|(x,y)| x != y).count()
}
fn write_diff(path: &Path, a: &[u8], b: &[u8]) -> Result<()> {
    let diff = a.chunks_exact(4).zip(b.chunks_exact(4)).flat_map(|(x,y)| {
        let d = (0..3).map(|i| x[i].abs_diff(y[i]).saturating_mul(3)).collect::<Vec<_>>();
        [d[0],d[1],d[2],255]
    }).collect::<Vec<_>>();
    save(path, &diff)
}

#[test]
#[ignore = "requires converted native v20 content, loopback QUIC and offscreen GPU; no window/audio device"]
fn native_core_tools_render_from_eye_and_original_mounts() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifact = workspace.join("artifacts/native-world-items");
    std::fs::create_dir_all(&artifact)?;
    let state = artifact.join(format!("state-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()));
    std::fs::create_dir_all(&state)?;
    let mut app = App::load(&workspace.join("content"), &state, SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame { map: BEDROOM.into(), mode: ServerMode::SinglePlayer,
        max_players: 1, server_name: "Native held item render".into(), password: String::new(),
        admin_password: String::new(), super_admin_password: String::new() });
    pump(&mut app)?;
    until(&mut app, "Bedroom host/player", |a| matches!(a.ui.core.conn, ConnectionState::InGame { .. })
        && a.network_view().is_some_and(|v| v.poses.contains_key(&v.owner)))?;
    let gpu = Headless::new().context("offscreen native held-item renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;

    let angles = [("front", std::f32::consts::PI, 0.08), ("quarter", std::f32::consts::PI * 0.5, 0.28), ("down", std::f32::consts::PI, -0.35)];
    let mut report = serde_json::json!({"adapter":gpu.adapter_info.name,"size":SIZE,"map":BEDROOM,"visible_window":false,"audio_device":false,"cases":[]});
    for (angle, yaw, pitch) in angles {
        // GameAction::Look carries mouse deltas, with positive screen Y looking down.
        let (current_yaw, current_pitch) = app.controls.view_angles();
        let yaw_delta = (yaw - current_yaw + std::f32::consts::PI).rem_euclid(2.0 * std::f32::consts::PI) - std::f32::consts::PI;
        app.ui.core.request(UiAction::Game(GameAction::Look { yaw: yaw_delta, pitch: current_pitch - pitch })); pump(&mut app)?;
        ensure!((app.controls.view_angles().0-yaw).abs()<0.02 || (app.controls.view_angles().0-yaw).abs()>6.26,
            "local view yaw did not reach static test angle");
        step(&mut app, Duration::from_millis(16))?;
        let baseline = capture(&mut app, &gpu, &mut renderer)?;
        save(&artifact.join(format!("none-{angle}.png")), &baseline)?;
        for (slot, label) in [(0,"hammer"),(1,"wrench"),(2,"printer")] {
            app.ui.core.request(UiAction::UseTool { slot }); pump(&mut app)?;
            until(&mut app, "selected core tool", |a| a.network_view().is_some_and(|v| {
                let t = &v.tools[&v.owner]; t.selected == Some(slot) && t.slots[slot].is_some()
            }))?;
            step(&mut app, Duration::from_millis(16))?;
            let frame = capture(&mut app, &gpu, &mut renderer)?;
            let diff = changed_pixels(&baseline, &frame);
            ensure!(diff > 20, "{label} at {angle} changed too few visible pixels ({diff}); item may be offscreen");
            ensure!(app.world_item_stats().visible_instances >= 1, "{label} at {angle} produced no mounted world-item instance");
            let name = format!("{label}-{angle}"); save(&artifact.join(format!("{name}.png")), &frame)?;
            write_diff(&artifact.join(format!("{name}-diff.png")), &baseline, &frame)?;
            report["cases"].as_array_mut().unwrap().push(serde_json::json!({"tool":label,"view":angle,"changed_pixels":diff,
                "visible_instances":app.world_item_stats().visible_instances,"models":app.world_item_stats().cached_models,
                "geometry_slots":app.world_item_stats().geometry_slots,"pose_samples":app.world_item_stats().pose_samples}));
        }
        app.ui.core.request(UiAction::UnUseTool); pump(&mut app)?;
        until(&mut app, "tool deselection", |a| a.network_view().is_some_and(|v| v.tools[&v.owner].selected.is_none()))?;
        step(&mut app, Duration::from_millis(16))?;
        ensure!(app.world_item_stats().visible_instances == 0, "deselected core tool left a mounted instance");
        ensure!(changed_pixels(&baseline, &capture(&mut app, &gpu, &mut renderer)?) < 20,
            "deselection did not restore baseline first-person render");
    }

    // Third-person view puts the local player back into the real App scene and
    // uses the authored Mount0 hand transform for the same selected tool.
    app.ui.core.request(UiAction::Game(GameAction::Held { control: HeldControl::Zoom, down: true }));
    app.ui.core.request(UiAction::Game(GameAction::SetZoomFov { fov: 12.0 }));
    app.ui.core.request(UiAction::Game(GameAction::ToggleFirstPerson { fast: false })); pump(&mut app)?;
    app.ui.core.request(UiAction::UnUseTool); pump(&mut app)?;
    until(&mut app, "third-person empty hands", |a| a.network_view().is_some_and(|v| v.tools[&v.owner].selected.is_none()))?;
    step(&mut app, Duration::from_millis(16))?;
    let third_baseline = capture(&mut app, &gpu, &mut renderer)?;
    app.ui.core.request(UiAction::UseTool { slot: 1 }); pump(&mut app)?;
    until(&mut app, "third-person wrench", |a| a.network_view().is_some_and(|v| v.tools[&v.owner].selected==Some(1)))?;
    step(&mut app, Duration::from_millis(16))?;
    let third = capture(&mut app, &gpu, &mut renderer)?;
    let third_diff = changed_pixels(&third_baseline, &third);
    ensure!(third_diff > 20, "third-person wrench changed too few visible pixels ({third_diff})");
    ensure!(app.world_item_stats().visible_instances >= 1, "third-person wrench missing from world projection");
    save(&artifact.join("wrench-third-person.png"), &third)?;
    save(&artifact.join("wrench-third-person-none.png"), &third_baseline)?;
    write_diff(&artifact.join("wrench-third-person-diff.png"), &third_baseline, &third)?;
    app.gpu_stopped();
    app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
    let reset = capture(&mut app, &gpu, &mut renderer)?;
    ensure!(changed_pixels(&third, &reset) == 0, "GPU recreation changed static third-person item render");
    save(&artifact.join("wrench-third-person-reset.png"), &reset)?;
    report["third_person_wrench_changed_pixels"] = third_diff.into();
    report["third_person_visible_instances"] = app.world_item_stats().visible_instances.into();
    report["third_person_models"] = app.world_item_stats().cached_models.into();
    report["third_person_geometry_slots"] = app.world_item_stats().geometry_slots.into();
    report["gpu_reset_equal_pixels"] = (SIZE.0 * SIZE.1).into();
    std::fs::write(artifact.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    app.gpu_stopped();
    Ok(())
}
