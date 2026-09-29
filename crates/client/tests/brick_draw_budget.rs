//! Draw-count regression check for large builds: the largest stock save
//! (Golden Gate Bridge, 44,465 bricks on Slate) meshed through the client's
//! chunk path, then one frame recorded offscreen from fixed cameras with
//! Best shadows and 4x anti-aliasing. Counts, unlike frame times, do not move
//! with the load on the machine, so this runs in the gate. Frame times on
//! real saves: `large_build_perf.rs`.
//! Run: cargo test -p bri-client --test brick_draw_budget -- --ignored --nocapture
use anyhow::{Context, Result, ensure};
use bri_client::{content::ClientContent, world_chunks::ChunkedWorld};
use bri_render::{
    scene::{Camera, SceneRenderer, ShadowCasters},
    shadow::ShadowSettings,
};
use glam::Vec3;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Budgets with headroom over what the chunk path recorded on 2026-09-29
/// (44 world and at most 4 shadow draws, 46% of triangles drawn); a change
/// that multiplies draws or undoes face culling fails here.
const MAX_DRAWS: u32 = 150;
const MAX_SHADOW_DRAWS: u32 = 40;
const MAX_CULLED_SHARE: f64 = 0.6;

#[test]
#[ignore = "requires generated v20 content and a GPU adapter; no window"]
fn the_largest_stock_build_stays_within_its_draw_budget() -> Result<()> {
    let root = std::env::var_os("BRI_CONTENT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    );
    let content = ClientContent::load(&root)?;
    let entry = content
        .worlds
        .iter()
        .filter(|w| w.loadable)
        .max_by_key(|w| w.brick_count)
        .context("no stock world")?
        .clone();
    let loaded = content.paths.load_map(&entry.map_id, Some(&entry.id))?;
    let state = loaded.simulation.state();
    let world = Arc::new(bri_net::protocol::PublicWorld {
        name: entry.name.clone(),
        map_id: entry.map_id.clone(),
        palette: state.palette.clone(),
        bricks: bri_net::protocol::public_bricks(&state.bricks),
    });
    let meshes: BTreeMap<_, _> = loaded
        .simulation
        .definitions
        .entries
        .iter()
        .map(|(id, d)| (id.clone(), d.mesh.clone()))
        .collect();
    let materials = bri_client::materials::BrickMaterials::load(&content.paths.brick_materials)?;
    let palette = bri_client::world_chunks::BrickPalette::new(&materials)?;
    let mut chunked = ChunkedWorld::default();
    let chunks: Vec<_> = chunked
        .update(world.clone(), None, &meshes, &palette, Some(&materials), 8_000_000)?
        .into_iter()
        .filter_map(|(_, built)| built.map(|b| b.scene))
        .collect();
    // Triangles drawn against every face of every visible brick.
    let drawn: usize = chunks.iter().map(|c| c.indices.len() / 3).sum();
    let every_face: usize = world
        .bricks
        .values()
        .filter(|b| b.visible)
        .filter_map(|b| match &b.definition {
            bri_world::ContentRef::Resolved(d) => meshes.get(d),
            _ => None,
        })
        .map(|m| m.quads.len() * 2)
        .sum();
    let culled_share = drawn as f64 / every_face as f64;

    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let mut renderer = SceneRenderer::with_settings(&device, format, 4, Some(ShadowSettings::BEST));
    let gpu_palette = renderer.upload(&device, &queue, &palette.scene)?;
    renderer.reserve_chunks(&chunks.iter().collect::<Vec<_>>())?;
    let gpu_chunks = chunks
        .iter()
        .map(|c| renderer.upload_chunk(&device, &queue, c, &gpu_palette))
        .collect::<Result<Vec<_>>>()?;
    let scenes: Vec<_> = gpu_chunks.iter().collect();
    let (width, height) = (1280, 720);
    let target = |samples| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("budget target"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    let color = target(4);
    let depth = bri_render::scene::create_depth_samples(&device, width, height, 4)
        .create_view(&Default::default());
    let (min, max) = world.bricks.values().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), b| (lo.min(Vec3::from(b.position)), hi.max(Vec3::from(b.position))),
    );
    let center = (min + max) * 0.5;
    let extent = (max - min).length().max(20.0);
    let mut worst = bri_render::scene::RenderStats::default();
    for (name, eye) in [
        ("overview", center + Vec3::new(extent * 0.6, extent * 0.4, extent * 0.6)),
        ("near", center + Vec3::new(40.0, 25.0, 40.0)),
    ] {
        let camera = Camera::perspective(
            eye.to_array(),
            center.to_array(),
            width as f32 / height as f32,
            90f32.to_radians(),
            0.05,
            4000.0,
        );
        renderer.update_camera(&queue, &camera);
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render_shadows(
            &mut encoder,
            ShadowCasters {
                scenes: &scenes,
                instances: &[],
            },
            ShadowCasters {
                scenes: &[],
                instances: &[],
            },
        );
        renderer.render(&mut encoder, &color, &depth, &scenes, Some(wgpu::Color::BLACK));
        queue.submit([encoder.finish()]);
        let stats = renderer.stats();
        println!("{name}: {stats:?}");
        worst.draws = worst.draws.max(stats.draws);
        worst.shadow_draws = worst.shadow_draws.max(stats.shadow_draws);
    }
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    println!(
        "{}: {} bricks, {} chunks, {drawn} of {} triangles after culling ({:.0}%), blocks {:?}",
        entry.name,
        world.bricks.len(),
        chunks.len(),
        every_face,
        culled_share * 100.0,
        renderer.pool_usage()
    );
    ensure!(
        worst.draws <= MAX_DRAWS,
        "world pass recorded {} draws (budget {MAX_DRAWS})",
        worst.draws
    );
    ensure!(
        worst.shadow_draws <= MAX_SHADOW_DRAWS,
        "shadow passes recorded {} draws (budget {MAX_SHADOW_DRAWS})",
        worst.shadow_draws
    );
    ensure!(
        culled_share <= MAX_CULLED_SHARE,
        "covered faces are no longer culled: {:.0}% of triangles drawn",
        culled_share * 100.0
    );
    Ok(())
}

fn gpu() -> Result<(wgpu::Device, wgpu::Queue)> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::from_env()
        .unwrap_or(wgpu::Backends::PRIMARY & !wgpu::Backends::VULKAN);
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .context("no GPU adapter")?;
    Ok(pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor::default()),
    )?)
}
