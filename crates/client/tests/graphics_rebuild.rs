//! A graphics option that rebuilds the world's renderers (Shadow Quality
//! here) keeps the world as uploaded: no brick chunk or map is sent to the
//! GPU again, and the frame matches one drawn from a fresh upload. Sending
//! them all again took 0.5-1 s in one frame on a 190,000-brick build
//! (docs/progress/2026-10-11-frame-instrumentation.md).
//! Runs on the made-up content root; the ignored variant runs on the
//! generated v20 content (`--release -- --ignored`, BRI_CONTENT or
//! content/). Loopback QUIC and an offscreen GPU; never opens a window.
use anyhow::{Context, Result, ensure};
use bri_client::{
    app::App,
    frame_trace,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
};
use glam::Vec3;
use std::time::Duration;

#[macro_use]
mod support;
use support::{content_root::ContentRoot, wait};

synthetic_and_content!(ContentRoot: a_shadow_quality_change_keeps_the_world_uploaded);

const SIZE: (u32, u32) = (320, 240);
/// The frames the staged skylight field takes to draw (`bri-render`'s
/// `sky_exposure::DIRECTIONS`, one a frame), and one more.
const SETTLE_FRAMES: usize = 10;

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
    wait::until_done(app, what, step, ready)
}
fn save_settings(app: &mut App, prefs: &[(&str, &str)]) -> Result<()> {
    let mut settings = app.ui.settings();
    for (key, value) in prefs {
        settings.prefs.insert((*key).into(), (*value).into());
    }
    app.ui
        .core
        .request(UiAction::SaveSettings(Box::new(settings)));
    pump(app)
}
/// One frame as the platform draws it, and its pixels.
fn capture(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("graphics rebuild target"),
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
            size: SIZE,
            ui_renderer: renderer
        })?,
        "App did not render a session camera"
    );
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("graphics rebuild readback"),
        size: u64::from(row) * u64::from(SIZE.1),
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
                rows_per_image: Some(SIZE.1),
            },
        },
        extent,
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    support::gpu::wait(&gpu.device, "the graphics rebuild frame")?;
    rx.recv_timeout(Duration::from_secs(5))??;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((SIZE.0 * SIZE.1 * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..SIZE.0 as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}
/// Frames enough for the staged shadows and skylight to settle, with no
/// tick between them, so the camera and world hold still; the last one's
/// pixels.
fn settled(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    for _ in 0..SETTLE_FRAMES {
        capture(app, gpu, renderer)?;
    }
    capture(app, gpu, renderer)
}
fn red(pixel: &[u8]) -> bool {
    let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(i32::from);
    r > 60 && r > 2 * g && r > 2 * b
}

fn a_shadow_quality_change_keeps_the_world_uploaded(f: &ContentRoot) -> Result<()> {
    let artifact = f.out("graphics-rebuild")?;
    let state_dir = f.state()?;
    let state = state_dir.path();
    let mut app = App::load(&f.root, state, SIZE)?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    // Classic lighting draws at once; Unified waits for the map's bake.
    save_settings(
        &mut app,
        &[
            (bri_client::graphics::LIGHTING, "0"),
            ("$pref::ShadowQuality", "0"),
        ],
    )?;
    app.ui.core.request(UiAction::HostGame {
        map: f.open_map.0.clone(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Graphics rebuild".into(),
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
    // A saved build: a red block of bricks, 12 by 12 units and 6 layers
    // tall, ten units ahead of the player.
    let (player, _) = app.local_motion().context("local player")?;
    let feet = Vec3::from(player.feet);
    let forward = Vec3::new(player.yaw.sin(), 0.0, -player.yaw.cos());
    let floor = (feet.y / 0.2).ceil() * 0.2;
    let centre = feet + forward * 10.0;
    let view = app.network_view().context("view")?;
    let map_id = view.world.map_id.clone();
    let mut world =
        bri_world::World::new("Rebuild".into(), map_id.clone(), view.world.palette.clone());
    let mut next = 1;
    for layer in 0..6 {
        for i in 0..12 {
            for j in 0..12 {
                let mut brick = bri_world::Brick::new(
                    bri_world::ContentRef::Resolved(f.brick.clone()),
                    [
                        centre.x.round() + i as f32 - 5.5,
                        floor + 0.3 + 0.6 * layer as f32,
                        centre.z.round() + j as f32 - 5.5,
                    ],
                    view.owner,
                );
                brick.color = 0;
                world.bricks.insert(next, brick);
                next += 1;
            }
        }
    }
    world.next_brick_id = next;
    let bricks = world.bricks.len();
    let build = bri_world::build::SavedBuild::new(world);
    use sha2::Digest;
    let folder = state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("rebuild.world.json"),
        serde_json::to_vec(&build)?,
    )?;
    app.ui.core.request(UiAction::LoadBricks {
        map: f.open_map.1.clone(),
        name: "rebuild.world.json".into(),
        ownership: true,
    });
    pump(&mut app)?;
    until(&mut app, "the block of bricks", |a| {
        a.network_view()
            .is_some_and(|v| v.world.bricks.len() >= bricks)
    })?;
    until(&mut app, "world render caught up", |a| {
        a.world_render_ready() && a.pending_requests() == 0
    })?;

    let gpu = support::gpu::turn().context("offscreen renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    app.gpu_ready(&gpu.device, &gpu.queue, format)?;
    let before = settled(&mut app, &gpu, &mut renderer)?;
    image::save_buffer(
        artifact.join("before.png"),
        &before,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    let uploads = |app: &App| app.work_counters().uploads.unwrap_or_default();
    let loaded = uploads(&app);
    ensure!(
        loaded.palette_geometry > 0 && loaded.scenes > 0,
        "the world was not uploaded: {loaded:?}"
    );
    let shown = before.chunks_exact(4).filter(|p| red(p)).count();
    ensure!(shown > 500, "the bricks are not in view ({shown} px)");

    // Low shadows: the renderers are rebuilt, the world stays uploaded.
    save_settings(&mut app, &[("$pref::ShadowQuality", "3")])?;
    frame_trace::reset();
    capture(&mut app, &gpu, &mut renderer)?;
    let change = frame_trace::finish(Duration::ZERO, false);
    ensure!(
        change.spans.iter().any(|s| s.name == "graphics rebuild"),
        "Shadow Quality did not rebuild the renderers: {}",
        change.describe()
    );
    let after = settled(&mut app, &gpu, &mut renderer)?;
    let rebuilt = uploads(&app);
    image::save_buffer(
        artifact.join("kept.png"),
        &after,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    ensure!(
        !change.spans.iter().any(|s| s.name == "world upload"),
        "the map was uploaded again: {}",
        change.describe()
    );
    ensure!(
        rebuilt.palette_geometry == 0,
        "brick chunks were uploaded again ({rebuilt:?}): {}",
        change.describe()
    );

    // The same settings from a fresh upload draw the same frame.
    app.gpu_ready(&gpu.device, &gpu.queue, format)?;
    let fresh = settled(&mut app, &gpu, &mut renderer)?;
    image::save_buffer(
        artifact.join("fresh.png"),
        &fresh,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    ensure!(
        uploads(&app).palette_geometry > 0,
        "the fresh renderer uploaded no bricks"
    );
    app.gpu_stopped();
    // Chunks may draw in another order, so coplanar faces can settle a
    // pixel differently; anything more is a scene drawn wrong.
    let differing = after
        .chunks_exact(4)
        .zip(fresh.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
        .count();
    let kept_red = after.chunks_exact(4).filter(|p| red(p)).count();
    eprintln!(
        "bricks {shown} px before, {kept_red} px kept; {differing} px differ from a fresh upload"
    );
    ensure!(
        differing * 200 < (SIZE.0 * SIZE.1) as usize,
        "the kept world draws differently from a fresh upload ({differing} px)"
    );
    ensure!(
        kept_red > 500,
        "the kept bricks are not drawn ({kept_red} px)"
    );
    Ok(())
}
