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
//! `BRI_LIGHT_AT=x,y,z[;x,y,z]` also prints what each recovered light gives
//! those points: falloff, visibility channel and the volume's verdict.
//! The map's lightmap leak cleanup applies as in the client (`BRI_LEAKS=0`
//! renders the lightmaps as baked); each changed lightmap is saved as
//! `leaks-{image}.png` with the changed texels in red.
//! Shadow checks: `BRI_TOWER=x,y,z,width,height` adds a brick-like tower
//! (kept like bricks) and `BRI_PLAYER=x,y,z` a player-sized box (moving,
//! like players), standing there; `BRI_LAMPS=0` turns lamp shadows off and
//! `BRI_SUN=0` the sun, to see which light casts what; `BRI_LIGHT_SCALE=k`
//! scales every light, to see shadows where full light saturates.
//! `BRI_DYNAMIC=1` renders the Dynamic mode alone (`{view}-dynamic.png`,
//! with its GPU times), for comparison with a run without it.
use anyhow::{Context, Result, ensure};
use bri_client::content::ClientContent;
use bri_net::protocol::PublicWorld;
use bri_render::{
    light_volume::LightVolume,
    map_lighting::{Bake, MapLighting},
    scene::{
        Camera, GpuInstances, GpuScene, Material, MeshBatch, SceneData, SceneRenderer, SceneTransform, SceneVertex,
        ShadowCasters,
    },
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

/// An axis-aligned white box (a stand-in brick tower or player).
fn cuboid(min: Vec3, max: Vec3) -> SceneData {
    let mut data = SceneData::default();
    data.materials.push(Material::vertex_lit("white", 0));
    for axis in 0..3 {
        for side in [0usize, 1] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let mut normal = Vec3::ZERO;
            normal[axis] = if side == 1 { 1.0 } else { -1.0 };
            let corner = |a: usize, b: usize| {
                let mut p = min;
                p[axis] = if side == 1 { max[axis] } else { min[axis] };
                p[u] = if a == 1 { max[u] } else { min[u] };
                p[v] = if b == 1 { max[v] } else { min[v] };
                p
            };
            let quad = [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)];
            // Counter-clockwise seen from outside.
            let order: [usize; 4] = if side == 1 { [0, 1, 2, 3] } else { [0, 3, 2, 1] };
            let base = data.vertices.len() as u32;
            data.vertices.extend(order.map(|k| SceneVertex {
                position: quad[k].to_array(),
                normal: normal.to_array(),
                uv: [0.0; 2],
                lightmap_uv: [0.0; 2],
                color: [1.0; 4],
                fx: [0.0; 4],
            }));
            data.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    data.batches.push(MeshBatch {
        indices: 0..data.indices.len() as u32,
        material: 0,
        center: ((min + max) * 0.5).to_array(),
    });
    data
}

/// `name=a,b,c...` numbers from the environment.
fn env_numbers(name: &str, count: usize) -> Result<Option<Vec<f32>>> {
    let Ok(text) = std::env::var(name) else { return Ok(None) };
    let v: Vec<f32> = text.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    ensure!(v.len() == count, "{name} wants {count} numbers");
    Ok(Some(v))
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
    let mut scene = map.scene;
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
        // The lightmap leak cleanup, as the client applies it (BRI_LEAKS=0
        // leaves the lightmaps as baked, to compare). Each changed lightmap
        // is also saved as leaks-{image}.png: the cleaned lightmap with the
        // changed texels in red, for review on this machine only.
        let before = scene.images.clone();
        if std::env::var("BRI_LEAKS").map_or(true, |v| v != "0") {
            let changed = bri_render::map_lighting::TexelFix::apply(&u.leaks, &mut scene.images);
            for image in changed {
                let fixes = u.leaks.iter().filter(|f| f.image as usize == image).count();
                let label = &scene.images[image].label;
                println!("Leak cleanup: image {image} ({label}): {fixes} texels");
                let mut pixels = scene.images[image].rgba.clone();
                for (i, (a, b)) in before[image].rgba.chunks_exact(4).zip(scene.images[image].rgba.chunks_exact(4)).enumerate() {
                    if a != b {
                        pixels[i * 4..i * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
                    } else {
                        pixels[i * 4 + 3] = 255;
                    }
                }
                let (w, h) = (scene.images[image].width, scene.images[image].height);
                image::save_buffer(out.join(format!("leaks-{image}.png")), &pixels, w, h, image::ColorType::Rgba8)?;
            }
        }
        // Each light bulb and tube and the recovered lights within 32 units
        // of its centre (the client gives a light to its nearest shapes
        // within LIGHT_SHAPE_REACH, 24 units).
        for b in loaded.breakables.iter().filter(|b| {
            ["lightBulbA", "fluorescentLight"].iter().any(|n| b.datablock.eq_ignore_ascii_case(n))
        }) {
            let near: Vec<String> = u
                .lights
                .iter()
                .enumerate()
                .map(|(i, l)| (i, b.center.distance(Vec3::from(l.position))))
                .filter(|&(_, d)| d <= 32.0)
                .map(|(i, d)| format!("light {i} at {d:.1}"))
                .collect();
            println!("Light shape {} node {} at {:?}: {}", b.datablock, b.node, b.center, near.join(", "));
        }
        // BRI_LIGHT_AT=x,y,z[;x,y,z...]: what each recovered light gives a
        // point (native Y-up) as the shader reads it: its falloff there and
        // its visibility channel in the volume cell holding the point (the
        // shader samples half a cell off the surface, filtered).
        if let Ok(points) = std::env::var("BRI_LIGHT_AT") {
            for point in points.split(';') {
                let p: Vec<f32> = point.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                ensure!(p.len() == 3, "BRI_LIGHT_AT wants x,y,z");
                let at = Vec3::new(p[0], p[1], p[2]);
                let v = &u.visibility;
                let cell = ((at - Vec3::from(v.origin)) / v.cell).floor();
                let inside = cell.cmpge(Vec3::ZERO).all()
                    && cell.cmplt(Vec3::new(v.dims[0] as f32, v.dims[1] as f32, v.dims[2] as f32)).all();
                let texel = inside.then(|| {
                    let c = cell.as_uvec3();
                    v.texels[(c.x + v.dims[0] * (c.y + v.dims[1] * c.z)) as usize]
                });
                println!("Light at {at:?}: volume cell {} units, texel {texel:?}", v.cell);
                for (i, l) in u.lights.iter().enumerate() {
                    let d = at.distance(Vec3::from(l.position));
                    if d >= l.outer {
                        continue;
                    }
                    let falloff = ((l.outer - d) / (l.outer - l.inner).max(0.001)).clamp(0.0, 1.0);
                    let seen = match (l.channel, texel) {
                        (Some(c), Some(t)) => format!("{}", t[c as usize + 1]),
                        (None, _) => "no channel (residual only, never casts)".into(),
                        (_, None) => "outside volume".into(),
                    };
                    println!(
                        "  light {i} at {:?} colour {:?} reach {}: distance {d:.1}, falloff {falloff:.2}, channel {:?}, seen {seen}",
                        l.position, l.color, l.outer, l.channel
                    );
                }
            }
        }
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
    // BRI_LAMPS=0: no lamp shadows (the sun's alone); BRI_SUN=0 below: no
    // sun (the lamps' alone).
    let lamps = std::env::var("BRI_LAMPS").map_or(true, |v| v != "0");
    // BRI_DYNAMIC=1: the Dynamic lighting mode alone (its light cubes would
    // change the other modes), to compare with a run without it.
    let dynamic = std::env::var("BRI_DYNAMIC").is_ok_and(|v| v == "1");
    let settings = ShadowSettings {
        lamps: if lamps { ShadowSettings::BEST.lamps } else { 0 },
        light_cubes: dynamic,
        ..ShadowSettings::BEST
    };
    if let Some(u) = &unified {
        let changed = bri_render::map_lighting::DynamicSheet::apply(u.dynamic.clone(), &mut scene.images);
        println!("Dynamic lightmaps: {} images", changed.len());
    }
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
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
    // Stand-ins, as the client draws them: BRI_TOWER=x,y,z,width,height a
    // brick tower (a kept, static chunk) standing on x,y,z; BRI_PLAYER=x,y,z
    // a player-sized box (a moving instance) standing there.
    let tower = match env_numbers("BRI_TOWER", 5)? {
        Some(t) => {
            let half = Vec3::new(t[3] * 0.5, 0.0, t[3] * 0.5);
            let foot = Vec3::new(t[0], t[1], t[2]);
            let data = cuboid(foot - half, foot + half + Vec3::Y * t[4]);
            let palette = renderer.upload(&device, &queue, &data)?;
            Some(renderer.upload_chunk(&device, &queue, &data, &palette)?)
        }
        None => None,
    };
    let player = match env_numbers("BRI_PLAYER", 3)? {
        Some(p) => {
            let body = renderer.upload(&device, &queue, &cuboid(Vec3::new(-0.5, 0.0, -0.3), Vec3::new(0.5, 2.6, 0.3)))?;
            let mut instances = GpuInstances::new(&device, 1)?;
            instances.update(
                &queue,
                &[SceneTransform {
                    transform: glam::Mat4::from_translation(Vec3::new(p[0], p[1], p[2])),
                    tint: [1.0; 4],
                }],
            )?;
            Some((body, instances))
        }
        None => None,
    };
    let models: Vec<(&GpuScene, &GpuInstances)> = player.iter().map(|(b, i)| (b, i)).collect();
    let no_sun = std::env::var("BRI_SUN").is_ok_and(|v| v == "0");
    let light_scale: f32 = std::env::var("BRI_LIGHT_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let mut scenes = vec![&gpu_map];
    scenes.extend(gpu_world.iter());
    scenes.extend(tower.iter());
    // Brick Shadows on unless BRI_BRICK_SHADOWS=0 (the client's default is
    // off: bricks then only stop other casters' shadows).
    let brick_shadows = std::env::var("BRI_BRICK_SHADOWS").map_or(true, |v| v != "0");
    let mut report = serde_json::Map::new();
    // Classic runs again last: the first views after upload run on a GPU
    // still settling its clocks and caches, which alone moved the median by
    // more than any mode.
    let modes: &[(u8, &str)] = if dynamic {
        &[(3, "dynamic")]
    } else {
        &[(0, "classic"), (1, "unified"), (2, "shine"), (0, "classic-again")]
    };
    for &(mode, label) in modes {
        match (mode, &unified) {
            (0, _) => {
                renderer.set_light_volume(&device, &queue, classic.as_ref())?;
                renderer.set_map_lighting(&device, &queue, None, false)?;
            }
            (_, Some(u)) => {
                // BRI_LIGHT_SCALE=k: every light (sun, ambient, map lights,
                // residual) times k, to see detail where the full light
                // saturates.
                let mut u = u.clone();
                if light_scale != 1.0 {
                    for l in &mut u.lights {
                        l.color = l.color.map(|c| c * light_scale);
                    }
                    for t in u.residual.texels.iter_mut().chain(&mut u.residual_all.texels) {
                        for c in &mut t[..3] {
                            *c = (*c as f32 * light_scale).round().clamp(0.0, 255.0) as u8;
                        }
                    }
                }
                let residual = if mode == 3 { &u.residual_all } else { &u.residual };
                renderer.set_light_volume(&device, &queue, Some(residual))?;
                renderer.set_map_lighting(&device, &queue, Some(&u), mode == 3)?;
            }
            (_, None) => {
                renderer.set_light_volume(&device, &queue, None)?;
                renderer.set_map_lighting(&device, &queue, None, false)?;
            }
        }
        let mut views_out = serde_json::Map::new();
        for (name, eye, look) in &views {
            for t in &mut terrain {
                t.update(&queue, *eye, 4000.0)?;
            }
            let terrain_draws: Vec<_> =
                terrain.iter().flat_map(GpuTerrain::draws).chain(models.iter().copied()).collect();
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
            if no_sun {
                camera.sun_color = [0.0; 4];
            }
            for c in 0..3 {
                camera.sun_color[c] *= light_scale;
                camera.ambient[c] *= light_scale;
            }
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
                // As the client: the map shades objects in the Unified modes.
                let map: &[&GpuScene] = if mode != 0 { &scenes[..1] } else { &[] };
                renderer.render_shadows_with_map(
                    &mut encoder,
                    ShadowCasters { scenes: if brick_shadows { &scenes[1..] } else { &[] }, instances: &models },
                    ShadowCasters { scenes: if brick_shadows { &[] } else { &scenes[1..] }, instances: &[] },
                    map,
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
