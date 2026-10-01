//! Kitchen map models (palms; on the made-up root, its room's plant)
//! through the real client, as the host player and as a guest joining over
//! loopback. The ignored variant runs on the generated v20 content
//! (`-- --ignored`, BRI_CONTENT or content/). Never creates a window or OS
//! input.
use anyhow::{Result, bail, ensure};
use bri_client::{
    app::App,
    platform::{PlatformApp, RenderContext},
};
use bri_ui::{
    api::*,
    gpu::{Headless, UiRenderer},
    screens::ScreenId,
};
use std::time::{Duration, Instant};

#[macro_use]
mod support;
use support::{content_root::ContentRoot, wait};

synthetic_and_content!(ContentRoot: kitchen_palms_render_for_host_and_guest);

/// A free port for this test's host, so a game already hosting on 28000
/// (the player's own, say) does not break the test. Set under the GPU
/// turn, which every test here holding a port takes first.
fn test_port() -> u16 {
    let port = std::net::UdpSocket::bind("127.0.0.1:0")
        .and_then(|s| s.local_addr())
        .map(|a| a.port())
        .expect("a free UDP port");
    // SAFETY: set before this test's host or join starts.
    unsafe {
        std::env::set_var("BRI_TEST_HOST_PORT", port.to_string());
        std::env::set_var("BRI_TEST_DISCOVERY_PORT", "0");
    }
    port
}

const SIZE: (u32, u32) = (960, 720);
const KITCHEN: &str = "v20/add-ons/map_kitchen/kitchen.mis";

fn step(apps: &mut [&mut App], elapsed: Duration) -> Result<()> {
    for app in apps {
        app.tick(elapsed)?;
        app.ui.update(elapsed.as_millis() as u64);
        ensure!(app.pump()?.is_empty(), "Unexpected native window command");
        if let ConnectionState::Failed { reason } = &app.ui.core.conn {
            bail!("Native app connection failed: {reason}");
        }
    }
    Ok(())
}

fn until(apps: &mut [&mut App], what: &str, ready: impl Fn(&[&mut App]) -> bool) -> Result<()> {
    wait::until(apps, what, Duration::from_secs(120), step, |apps| {
        Ok(ready(apps))
    })
}

fn entered(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app.world_render_ready()
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}

fn capture(app: &mut App, gpu: &Headless, renderer: &mut UiRenderer) -> Result<Vec<u8>> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("map-shape capture"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    ensure!(
        app.render_scene(&mut RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target: &view,
            format,
            size: SIZE,
            ui_renderer: renderer,
        })?,
        "Entered session did not render a player camera"
    );
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("map-shape readback"),
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
    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((SIZE.0 * SIZE.1 * 4) as usize);
    for bytes in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&bytes[..SIZE.0 as usize * 4]);
    }
    Ok(pixels)
}

/// Green as a plant's leaves are.
fn leafy(p: &[u8]) -> bool {
    let [r, g, b] = [p[0], p[1], p[2]].map(i32::from);
    g > 60 && g > r + 30 && g > b + 30
}

fn distinct_colors(pixels: &[u8]) -> usize {
    pixels
        .chunks_exact(4)
        .map(|p| [p[0] >> 3, p[1] >> 3, p[2] >> 3])
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

fn kitchen_palms_render_for_host_and_guest(f: &ContentRoot) -> Result<()> {
    // Both players own a GPU from startup, as with a real window.
    let gpu = support::gpu::turn()?;
    let port = test_port();
    let artifact = f.out("native-map-shapes")?;
    let (host_state, guest_state) = (f.state()?, f.state()?);
    let mut host = App::load(&f.root, host_state.path(), SIZE)?;
    let mut guest = App::load(&f.root, guest_state.path(), SIZE)?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    for app in [&mut host, &mut guest] {
        app.gpu_ready(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm)?;
        app.ui.core.pop(ScreenId::DefaultControls);
        app.ui.update(0);
    }
    host.ui.core.request(UiAction::HostGame {
        map: KITCHEN.into(),
        mode: ServerMode::Lan,
        game_mode: None,
        max_players: 2,
        server_name: "Native headless Kitchen".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut [&mut host], "Kitchen host", |a| entered(a[0]))?;
    guest.ui.core.request(UiAction::JoinServer {
        address: format!("127.0.0.1:{port}"),
        password: String::new(),
    });
    until(&mut [&mut host, &mut guest], "Kitchen guest", |a| {
        entered(a[1]) && a[0].ui.core.players.len() == 2
    })?;
    assert_eq!(guest.network_view().unwrap().world.map_id, KITCHEN);
    // v20's spawnExplosion bursts 4-unit cloud particles at the new body,
    // tinted "0 1 0 1" at mid-life, so a player's first frames are green.
    // Let both bursts end (at most 0.8 s) before judging the frames.
    let settled = Instant::now();
    until(&mut [&mut host, &mut guest], "spawn bursts to end", |_| {
        settled.elapsed() > Duration::from_millis(1500)
    })?;

    for (who, app) in [("host", &mut host), ("guest", &mut guest)] {
        let pixels = capture(app, &gpu, &mut renderer)?;
        image::save_buffer(
            artifact.join(format!("kitchen-{who}.png")),
            &pixels,
            SIZE.0,
            SIZE.1,
            image::ColorType::Rgba8,
        )?;
        let colors = distinct_colors(&pixels);
        let leaves = pixels.chunks_exact(4).filter(|p| leafy(p)).count();
        println!("{who}: {colors} distinct colours, {leaves} leafy pixels");
        if f.content {
            ensure!(colors > 200, "{who} frame looks unrendered");
        } else {
            // The made-up room is plain, but its plant stands in view.
            ensure!(leaves > 500, "{who} does not see the room's plant");
        }
    }
    guest.ui.core.request(UiAction::Disconnect);
    host.ui.core.request(UiAction::Disconnect);
    step(&mut [&mut guest, &mut host], Duration::ZERO)?;
    Ok(())
}
