//! Offscreen comparison of the lighting modes (`$pref::Video::Lighting`):
//! loads a map and a saved build (or a synthetic one of any size), bakes the
//! map's light volume and recovered lights as the client does, then renders
//! the same views in Classic, Unified and Unified+Shine with Best shadows and
//! brick shadows on, saving `{view}-{mode}.png` and GPU frame times. It never
//! opens a window or reads input.
//!
//! Usage: lighting_probe <content-root> <out-dir> <world-name-substring>
//!        lighting_probe <content-root> <out-dir> synthetic:<map-substring>:<bricks>
//! Optional views follow as `name=ex,ey,ez,tx,ty,tz` (native Y-up).
use anyhow::{Context, Result, ensure};
use bri_client::content::ClientContent;
use bri_net::protocol::PublicWorld;
use bri_render::{
    light_volume::LightVolume,
    map_lighting::{Bake, MapLighting},
    scene::{Camera, SceneRenderer, ShadowCasters},
    scene_loader::load_map_bundle,
    shadow::ShadowSettings,
    terrain_scene::GpuTerrain,
};
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Instant};

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Rows of real bricks from `like`, each in its own 4x4-stud, 3-plate cell
/// of a 100 x 100 column grid starting beside `at` (as `brick_load_bench`).
fn synthetic(
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
    like: &World,
    count: usize,
    at: Vec3,
) -> Result<World> {
    let table: Vec<&Brick> = like
        .bricks
        .values()
        .filter(|b| {
            matches!(&b.definition, ContentRef::Resolved(id) if meshes.get(id).is_some_and(|m| {
                m.footprint_studs[0] <= 4 && m.footprint_studs[1] <= 4 && m.height_plates <= 3
            })) && b.events.is_empty()
        })
        .collect();
    ensure!(!table.is_empty(), "No small bricks to build with");
    let mut world = World::new("Synthetic".into(), like.map_id.clone(), like.palette.clone());
    let snap = |v: f32, step: f32| (v / step).round() * step;
    let origin = Vec3::new(snap(at.x - 100.0, 2.0), snap(at.y, 0.2), snap(at.z + 100.0, 2.0));
    let side = 100usize;
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for i in 0..count {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let (x, z, y) = (i % side, (i / side) % side, i / (side * side));
        let template = table[(seed >> 16) as usize % table.len()];
        let ContentRef::Resolved(id) = &template.definition else { unreachable!() };
        let height = meshes[id].height_plates as f32 * 0.2;
        let [tx, ty, tz] = template.position;
        let bottom = ty - height * 0.5;
        let mut brick = template.clone();
        brick.owner = 0;
        brick.source_records.clear();
        brick.position = [
            origin.x + x as f32 * 2.0 + (tx - snap(tx, 2.0)),
            origin.y + (y * 3) as f32 * 0.2 + (bottom - snap(bottom, 0.2)) + height * 0.5,
            origin.z - z as f32 * 2.0 + (tz - snap(tz, 2.0)),
        ];
        brick.color = ((seed >> 40) % like.palette.len() as u64) as u8;
        world.bricks.insert(i as u64 + 1, brick);
    }
    world.next_brick_id = count as u64 + 1;
    world.validate()?;
    Ok(world)
}

fn read_back(device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::Texture) -> Result<Vec<u8>> {
    let (width, height) = (target.width(), target.height());
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe readback"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..width as usize * 4]);
    }
    Ok(pixels)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 3,
        "Usage: lighting_probe <content-root> <out-dir> <world-substring | synthetic:<map>:<bricks>> [name=ex,ey,ez,tx,ty,tz ...]"
    );
    let root = PathBuf::from(&args[0]);
    let out = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&out)?;
    let content = ClientContent::load(&root)?;
    let paths = content.paths.clone();
    let (entry, synthetic_count) = match args[2].strip_prefix("synthetic:") {
        Some(rest) => {
            let (map, count) = rest.split_once(':').context("synthetic:<map>:<bricks>")?;
            let entry = content
                .worlds
                .iter()
                .filter(|w| w.loadable && w.map_id.contains(map))
                .max_by_key(|w| w.brick_count)
                .context("No world on that map to take bricks from")?;
            (entry.clone(), Some(count.parse::<usize>()?))
        }
        None => (
            content
                .worlds
                .iter()
                .filter(|w| w.loadable && w.name.contains(args[2].as_str()))
                .max_by_key(|w| w.brick_count)
                .with_context(|| format!("No loadable world matches {:?}", args[2]))?
                .clone(),
            None,
        ),
    };
    let loaded = paths.load_map(&entry.map_id, Some(&entry.id))?;
    let definitions = loaded.simulation.definitions.clone();
    let meshes: BTreeMap<_, _> = definitions
        .entries
        .iter()
        .map(|(id, def)| (id.clone(), def.mesh.clone()))
        .collect();
    let spawn = loaded.spawn_points[0];
    let mut world = loaded.simulation.state().clone();
    if let Some(count) = synthetic_count {
        world = synthetic(&meshes, &world, count, spawn)?;
    }
    println!(
        "{} on {}: {} bricks",
        if synthetic_count.is_some() { "Synthetic" } else { entry.name.as_str() },
        entry.map_id,
        world.bricks.len()
    );
    let public = Arc::new(PublicWorld {
        name: world.name.clone(),
        map_id: world.map_id.clone(),
        palette: world.palette.clone(),
        bricks: world.bricks.iter().map(|(k, b)| (*k, bri_net::protocol::public_brick(b))).collect(),
    });
    let materials = bri_client::materials::BrickMaterials::load(&paths.brick_materials)?;
    let palette = bri_client::world_chunks::BrickPalette::new(&materials)?;
    let mut chunked = bri_client::world_chunks::ChunkedWorld::default();
    let t = Instant::now();
    let chunks: Vec<_> = chunked
        .update(public.clone(), None, &meshes, &palette, Some(&materials), 64_000_000)?
        .into_iter()
        .filter_map(|(_, scene)| Some(scene?.scene))
        .collect();
    let mesh_ms = ms(t.elapsed());

    let map = load_map_bundle(&paths.map_bundle, &entry.map_id)?;
    let scene = map.scene;
    let cache = out.join("cache");
    std::fs::create_dir_all(&cache)?;
    let t = Instant::now();
    let classic = LightVolume::bake(&scene, 2.0, 1_000_000);
    let classic_ms = ms(t.elapsed());
    let t = Instant::now();
    let unified: Option<MapLighting> = Bake::new(&scene).map(|bake| {
        let key = bake.key();
        let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
        let file = cache.join(format!("{hex}.maplighting"));
        if let Some(stored) = std::fs::read(&file).ok().and_then(|b| MapLighting::from_bytes(&b, key)) {
            return stored;
        }
        let lit = bake.bake(2.0, 1_000_000, 2.0, 2_000_000);
        let _ = std::fs::write(&file, lit.to_bytes(key));
        lit
    });
    let unified_ms = ms(t.elapsed());
    if let Some(u) = &unified {
        println!("Map lighting: {} lights, report {:?}", u.lights.len(), u.report);
    }

    // Views: given, or from spawn toward the build's centre and a closer one.
    let (lo, hi) = world.bricks.values().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), b| (lo.min(Vec3::from(b.position)), hi.max(Vec3::from(b.position))),
    );
    let centre = if world.bricks.is_empty() { spawn } else { (lo + hi) * 0.5 };
    let mut views: Vec<(String, Vec3, Vec3)> = args[3..]
        .iter()
        .filter_map(|a| {
            let (name, v) = a.split_once('=')?;
            let v: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            (v.len() == 6).then(|| (name.to_string(), Vec3::new(v[0], v[1], v[2]), Vec3::new(v[3], v[4], v[5])))
        })
        .collect();
    if views.is_empty() {
        views.push(("spawn".into(), spawn + Vec3::Y * 2.4, centre));
        let extent = (hi - lo).length().clamp(20.0, 120.0);
        views.push(("overview".into(), centre + Vec3::new(extent * 0.5, extent * 0.35, extent * 0.5), centre));
    }

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .context("No headless GPU adapter")?;
    let info = adapter.get_info();
    // GPU time for each frame's passes, where the adapter can stamp inside
    // encoders; wall-clock times move with the load on the machine.
    let stamps = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    let timed = adapter.features().contains(stamps);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("lighting probe"),
        required_limits: adapter.limits(),
        required_features: if timed { stamps } else { wgpu::Features::empty() },
        ..Default::default()
    }))?;
    let queries = timed.then(|| {
        device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame stamps"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        })
    });
    let resolved = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("stamps"),
        size: 16,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readable = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("stamps read"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let period = f64::from(queue.get_timestamp_period());
    let (width, height) = (1920u32, 1080u32);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("probe target"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let depth = bri_render::scene::create_depth(&device, width, height).create_view(&Default::default());
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::BEST));
    let gpu_map = renderer.upload(&device, &queue, &scene)?;
    let gpu_palette = renderer.upload(&device, &queue, &palette.scene)?;
    renderer.reserve_chunks(&chunks.iter().collect::<Vec<_>>())?;
    let gpu_world = chunks
        .iter()
        .map(|chunk| renderer.upload_chunk(&device, &queue, chunk, &gpu_palette))
        .collect::<Result<Vec<_>>>()?;
    let mut terrain = map
        .terrain
        .into_iter()
        .map(|t| GpuTerrain::upload(&renderer, &device, &queue, t.into(), 4000.0))
        .collect::<Result<Vec<_>>>()?;
    let mut scenes = vec![&gpu_map];
    scenes.extend(gpu_world.iter());
    // Brick Shadows on unless BRI_BRICK_SHADOWS=0 (the client's default is
    // off: bricks then only stop other casters' shadows).
    let brick_shadows = std::env::var("BRI_BRICK_SHADOWS").map_or(true, |v| v != "0");
    let mut report = serde_json::Map::new();
    // Classic runs again last: the first views after upload run on a GPU
    // still settling its clocks and caches, which alone moved the median by
    // more than any mode.
    for (mode, label) in [(0u8, "classic"), (1, "unified"), (2, "shine"), (0, "classic-again")] {
        match (mode, &unified) {
            (0, _) => {
                renderer.set_light_volume(&device, &queue, classic.as_ref())?;
                renderer.set_map_lighting(&device, &queue, None)?;
            }
            (_, Some(u)) => {
                renderer.set_light_volume(&device, &queue, Some(&u.residual))?;
                renderer.set_map_lighting(&device, &queue, Some(u))?;
            }
            (_, None) => {
                renderer.set_light_volume(&device, &queue, None)?;
                renderer.set_map_lighting(&device, &queue, None)?;
            }
        }
        let mut views_out = serde_json::Map::new();
        for (name, eye, look) in &views {
            for t in &mut terrain {
                t.update(&queue, *eye, 4000.0)?;
            }
            let terrain_draws: Vec<_> = terrain.iter().flat_map(GpuTerrain::draws).collect();
            let mut camera = Camera::perspective(
                eye.to_array(),
                look.to_array(),
                width as f32 / height as f32,
                2.0 * ((45f32.to_radians()).tan() / (width as f32 / height as f32)).atan(),
                0.05,
                4000.0,
            );
            camera.apply_environment(&scene);
            camera.ambient[3] = f32::from(mode);
            let mut frames = Vec::new();
            let mut gpu = Vec::new();
            for i in 0..80 {
                let t = Instant::now();
                // Every frame, as the client does (lamp faces kept between
                // frames redraw only when stale).
                renderer.update_camera(&queue, &camera);
                let mut encoder = device.create_command_encoder(&Default::default());
                if let Some(q) = &queries {
                    encoder.write_timestamp(q, 0);
                }
                renderer.render_shadows(
                    &mut encoder,
                    ShadowCasters { scenes: if brick_shadows { &scenes[1..] } else { &[] }, instances: &[] },
                    ShadowCasters { scenes: if brick_shadows { &[] } else { &scenes[1..] }, instances: &[] },
                );
                renderer.render_with_instances(
                    &mut encoder,
                    &view,
                    &depth,
                    &scenes,
                    &terrain_draws,
                    Some(wgpu::Color::BLACK),
                );
                if let Some(q) = &queries {
                    encoder.write_timestamp(q, 1);
                    encoder.resolve_query_set(q, 0..2, &resolved, 0);
                    encoder.copy_buffer_to_buffer(&resolved, 0, &readable, 0, 16);
                }
                queue.submit([encoder.finish()]);
                device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None })?;
                if i >= 10 {
                    frames.push(ms(t.elapsed()));
                    if queries.is_some() {
                        readable.slice(..).map_async(wgpu::MapMode::Read, |_| {});
                        device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None })?;
                        let ticks: Vec<u64> = readable
                            .slice(..)
                            .get_mapped_range()?
                            .chunks_exact(8)
                            .map(|c| u64::from_le_bytes(c.try_into().expect("8 bytes")))
                            .collect();
                        readable.unmap();
                        gpu.push(ticks[1].wrapping_sub(ticks[0]) as f64 * period / 1e6);
                    }
                }
            }
            frames.sort_by(f64::total_cmp);
            gpu.sort_by(f64::total_cmp);
            let gpu_p50 = gpu.get(gpu.len() / 2).copied();
            let gpu_min = gpu.first().copied();
            let pixels = read_back(&device, &queue, &target)?;
            let file = out.join(format!("{name}-{label}.png"));
            image::save_buffer(&file, &pixels, width, height, image::ColorType::Rgba8)?;
            println!(
                "{label} {name}: wall p50 {:.2} ms (min {:.2}); GPU p50 {:.2} ms (min {:.2}) -> {}",
                frames[frames.len() / 2],
                frames[0],
                gpu_p50.unwrap_or(f64::NAN),
                gpu_min.unwrap_or(f64::NAN),
                file.display()
            );
            views_out.insert(
                name.clone(),
                json!({"p50_ms": frames[frames.len() / 2], "min_ms": frames[0], "p95_ms": frames[frames.len() * 95 / 100],
                    "gpu_p50_ms": gpu_p50, "gpu_min_ms": gpu_min, "eye": eye.to_array(), "look": look.to_array()}),
            );
        }
        report.insert(label.into(), views_out.into());
    }
    let report = json!({
        "adapter": format!("{} ({:?})", info.name, info.backend),
        "map": entry.map_id, "world": if synthetic_count.is_some() { "synthetic".to_string() } else { entry.name.clone() },
        "bricks": world.bricks.len(), "chunks": chunks.len(), "mesh_ms": mesh_ms, "brick_shadows": brick_shadows,
        "classic_volume_bake_ms": classic_ms, "map_lighting_ms": unified_ms,
        "map_lights": unified.as_ref().map(|u| json!({"lights": u.lights, "report": u.report})),
        "frames": report,
    });
    std::fs::write(out.join("report.json"), serde_json::to_string_pretty(&report)?)?;
    Ok(())
}
