//! Offscreen Start Game mission list with the actual client UI. No window or input.
//! Set BRI_CONTENT_ROOT to a packaged build's content directory to check a package.
use anyhow::{Context, Result, ensure};
use bri_client::app::App;
use bri_ui::{
    gpu::{Headless, UiRenderer},
    screens::ScreenId,
};
use std::{path::PathBuf, time::Duration};

const SIZE: (u32, u32) = (960, 720);

#[test]
#[ignore = "requires converted native content and an offscreen GPU; no window"]
fn start_game_lists_and_draws_every_loadable_map() -> Result<()> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let content = std::env::var_os("BRI_CONTENT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("content"));
    let state = std::env::temp_dir().join(format!("bri-start-list-{}", std::process::id()));
    let mut app = App::load(&content, &state, SIZE)?;
    app.ui.core.push(ScreenId::StartMission);
    app.ui.update(0);
    let screen = app
        .ui
        .screen(ScreenId::StartMission)
        .context("Start Game screen")?;
    let view = screen.view();
    let list = view.id("SM_missionList").context("mission list")?;
    let items = &view.node(list).state.items;
    let names: Vec<_> = items.iter().map(|(name, _)| name.clone()).collect();
    ensure!(
        names.len() == bri_client::content::LOADABLE_MAPS.len(),
        "Mission list has {} maps: {names:?}",
        names.len()
    );
    // Every row must lie inside the list control, or it is clipped away.
    let rect = view.node(list).rect;
    let rows = names.len() as i32 * view.node(list).state.row_height;
    ensure!(
        rect.h >= rows,
        "Mission list is {} px tall but its {} rows need {rows} px",
        rect.h,
        names.len()
    );

    let gpu = Headless::new()?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("start game list"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render(
        &gpu.device,
        &gpu.queue,
        &mut encoder,
        &target_view,
        format,
        SIZE,
        app.ui.scale(),
        &app.ui.core.pack,
        &app.ui.draw(),
        Some(wgpu::Color::BLACK),
    );
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("start game readback"),
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
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    rx.recv_timeout(Duration::from_secs(5))??;
    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((SIZE.0 * SIZE.1 * 4) as usize);
    for bytes in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&bytes[..SIZE.0 as usize * 4]);
    }
    drop(mapped);
    readback.unmap();
    let output = workspace.join("artifacts/start-game-list");
    std::fs::create_dir_all(&output)?;
    image::save_buffer(
        output.join("start-game.png"),
        &pixels,
        SIZE.0,
        SIZE.1,
        image::ColorType::Rgba8,
    )?;
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "content": content.display().to_string(),
            "maps": names,
            "list_height": rect.h,
            "row_height": view.node(list).state.row_height,
        }))?,
    )?;
    let _ = std::fs::remove_dir_all(&state);
    Ok(())
}
