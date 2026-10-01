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
//! `BRI_OFF=i,j,...` switches those recovered lights off, as a broken bulb
//! or tube does (each "Light shape" line lists its lights); `BRI_BREAK=1`
//! breaks every bulb and tube by the client's rule. Each recovered light is
//! listed with its owning light shapes, and each light shape with the map
//! surfaces within 40 units of it, and within 8 units of it each triangle's
//! lightmap, Dynamic leftover and light shares, and the facing to its
//! lights. `BRI_DUMP_LEFT=1` saves each Dynamic sheet as `left-{image}.png`:
//! the decomposed light beside what is left with every light off.
//! `BRI_PIXELS=view:x,y;x,y` (pixels of the 1920x1080 `{view}-*.png`)
//! prints the map surface under each pixel and, for its lightmap texel and
//! the eight around it, how the bake split the light (each light's level,
//! facing, rays, weight and share) and what is drawn whole and broken.
//! `BRI_MAP=<map-substring>` draws the build on another map sharing its
//! interior (a Kitchen save on KitchenDark). `BRI_BRICK_LIGHTS=1` adds the
//! build's brick lights (the nearest 256 to each view, as the client).
//! `BRI_TERMS=1` prints, over the build's brick tops and sides, what each
//! part of the object lighting gives (ambient, sun, map lights, residual,
//! classic volume, brick lights) as the brightest channel's percentiles.
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

/// The build's brick lights as the client starts them (the effects runtime,
/// a moment after they switch on), as the shader's point lights.
fn brick_lights(
    pack: &std::path::Path,
    world: Arc<PublicWorld>,
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
) -> Result<Vec<bri_render::scene::PointLight>> {
    let pack = bri_fx_runtime::EffectsPack::load(pack)?;
    let limits = bri_fx_runtime::EffectsLimits { lights: 4096, ..Default::default() };
    let mut effects = bri_client::effects::WorldEffects::new(pack, limits)?;
    effects.sync(world, meshes)?;
    effects.advance(0.05, Vec3::ZERO, Vec3::ZERO, |_, _, _| Ok(true))?;
    let camera = bri_fx_runtime::Camera {
        view_projection: glam::Mat4::IDENTITY,
        position: Vec3::ZERO,
        right: Vec3::X,
        up: Vec3::Y,
    };
    let lights: Vec<_> = effects
        .world
        .snapshot(&camera)
        .lights
        .iter()
        .map(|l| bri_render::scene::PointLight {
            position_radius: l.position.extend(l.radius).to_array(),
            color: l.color.extend(0.0).to_array(),
        })
        .collect();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for l in &lights {
        *kinds
            .entry(format!("colour {:.2?} radius {:.1}", &l.color[..3], l.position_radius[3]))
            .or_default() += 1;
    }
    println!("Brick lights: {}", lights.len());
    for (kind, n) in kinds {
        println!("  {n} x {kind}");
    }
    Ok(lights)
}

/// BRI_TERMS: each part of the object lighting over the build's brick tops
/// (normal up) and sides (normal +X at the top's centre), as the shader
/// adds them in the Unified modes (map lights through their channels) and
/// Classic (the light volume), unshadowed by live casters. Printed as the
/// brightest channel's 10th, 50th and 90th percentiles and maximum.
fn light_terms(
    world: &World,
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
    scene: &SceneData,
    unified: Option<&MapLighting>,
    classic: Option<&LightVolume>,
    points: &[bri_render::scene::PointLight],
) {
    let bricks: Vec<&Brick> = world.bricks.values().collect();
    let step = (bricks.len() / 4000).max(1);
    let samples: Vec<Vec3> = bricks
        .iter()
        .step_by(step)
        .filter_map(|b| {
            let ContentRef::Resolved(id) = &b.definition else { return None };
            let height = meshes.get(id)?.height_plates as f32 * 0.2;
            Some(Vec3::from(b.position) + Vec3::Y * (height * 0.5 + 0.01))
        })
        .collect();
    let sun_toward = -Vec3::from(scene.sun_direction).normalize_or_zero();
    println!(
        "Terms over {} brick tops: ambient {:?}, sun colour {:?}",
        samples.len(),
        scene.ambient,
        scene.sun_color
    );
    for (face, normal) in [("top", Vec3::Y), ("side", Vec3::X)] {
        let mut terms: BTreeMap<&str, Vec<f32>> = BTreeMap::new();
        for &p in &samples {
            let mut add = |name, v: Vec3| terms.entry(name).or_default().push(v.max_element());
            add("1 ambient", Vec3::from(scene.ambient));
            add("2 sun (unshadowed)", Vec3::from(scene.sun_color) * normal.dot(sun_toward).max(0.0));
            if let Some(u) = unified {
                let v = &u.visibility;
                let at = p + normal * v.cell * 0.5;
                let cell = ((at - Vec3::from(v.origin)) / v.cell).floor();
                let inside = cell.cmpge(Vec3::ZERO).all()
                    && cell.cmplt(Vec3::new(v.dims[0] as f32, v.dims[1] as f32, v.dims[2] as f32)).all();
                let texel = inside.then(|| {
                    let c = cell.as_uvec3();
                    v.texels[(c.x + v.dims[0] * (c.y + v.dims[1] * c.z)) as usize]
                });
                let (mut seen, mut all) = (Vec3::ZERO, Vec3::ZERO);
                for l in &u.lights {
                    let delta = Vec3::from(l.position) - p;
                    let d = delta.length();
                    let cosine = normal.dot(delta) / d.max(1e-4);
                    if d >= l.outer || cosine <= 0.0 {
                        continue;
                    }
                    let falloff = ((l.outer - d) / (l.outer - l.inner).max(0.001)).clamp(0.0, 1.0);
                    let light = Vec3::from(l.color) * falloff * (0.5 + 0.5 * cosine);
                    all += light;
                    if let (Some(c), Some(t)) = (l.channel, texel) {
                        seen += light * f32::from(t[c as usize + 1]) / 255.0;
                    }
                }
                add("3 map lights (channels, seen)", seen);
                add("4 map lights (all, unshadowed)", all);
                add("5 residual volume", Vec3::from(u.residual.light(p.to_array(), normal.to_array())));
            }
            if let Some(c) = classic {
                add("6 classic volume", Vec3::from(c.light(p.to_array(), normal.to_array())));
            }
            let mut point = Vec3::ZERO;
            for l in points {
                let delta = Vec3::from_slice(&l.position_radius[..3]) - p;
                let d = delta.length();
                // As `vertex_point_illumination` (v20's GL attenuation).
                if d < l.position_radius[3] {
                    point += Vec3::from_slice(&l.color[..3]) * normal.dot(delta / d.max(1e-4)).max(0.0)
                        / (1.0 + 0.1 * d * d);
                }
            }
            add("7 brick lights", point);
        }
        println!("  {face}:");
        for (name, mut values) in terms {
            values.sort_by(f32::total_cmp);
            let at = |q: f32| values[((values.len() - 1) as f32 * q) as usize];
            println!(
                "    {name}: p10 {:.2}, p50 {:.2}, p90 {:.2}, max {:.2}",
                at(0.1),
                at(0.5),
                at(0.9),
                at(1.0)
            );
        }
    }
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

    let map_id = match std::env::var("BRI_MAP") {
        Ok(wanted) => bri_client::content::LOADABLE_MAPS
            .iter()
            .filter(|id| id.contains(wanted.as_str()))
            .min_by_key(|id| id.len())
            .with_context(|| format!("No loadable map matches {wanted:?}"))?
            .to_string(),
        Err(_) => entry.map_id.clone(),
    };
    println!("Drawing on {map_id}");
    let map = load_map_bundle(&paths.map_bundle, &map_id)?;
    let mut scene = map.scene;
    // BRI_PIXELS explains texels from the scene as the bake sees it, before
    // the leak cleanup and the Dynamic lightmaps change its images.
    let pixels = std::env::var("BRI_PIXELS").ok();
    let pristine = pixels.as_ref().map(|_| scene.clone());
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
    let light_shapes: Vec<(u32, Vec3)> = loaded
        .breakables
        .iter()
        .filter(|b| ["lightBulbA", "fluorescentLight"].iter().any(|n| b.datablock.eq_ignore_ascii_case(n)))
        .map(|b| (b.node, b.center))
        .collect();
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
        // Every recovered light and the light shapes it belongs to (the
        // client's rule), then the map surfaces within 40 units of each
        // light shape: what stays lit, or glows, when it breaks.
        for (i, (l, owners)) in u.lights.iter().zip(bri_render::map_lighting::fixture_owners(&u.lights, &light_shapes)).enumerate() {
            println!(
                "Light {i}: at {:?} colour {:?} inner {} reach {} channel {:?}, owned by nodes {:?}",
                l.position,
                l.color,
                l.inner,
                l.outer,
                l.channel,
                owners.iter().map(|(n, _)| *n).collect::<Vec<_>>()
            );
        }
        let owned: Vec<(usize, Vec<u32>)> = bri_render::map_lighting::fixture_owners(&u.lights, &light_shapes)
            .into_iter()
            .map(|o| o.into_iter().map(|(n, _)| n).collect())
            .enumerate()
            .collect();
        for &(node, centre) in &light_shapes {
            for (index, batch) in scene.batches.iter().enumerate() {
                let range = batch.indices.start as usize..batch.indices.end as usize;
                let near: Vec<f32> = scene.indices[range]
                    .chunks_exact(3)
                    .map(|t| t.iter().map(|&v| centre.distance(Vec3::from(scene.vertices[v as usize].position))).fold(f32::INFINITY, f32::min))
                    .filter(|&d| d <= 40.0)
                    .collect();
                if near.is_empty() {
                    continue;
                }
                let m = &scene.materials[batch.material];
                let lightmap = &scene.images[m.images[8]];
                let n = (lightmap.rgba.len() / 4).max(1) as f32;
                let mean: Vec<u32> = (0..3)
                    .map(|c| (lightmap.rgba.chunks_exact(4).map(|t| f32::from(t[c])).sum::<f32>() / n) as u32)
                    .collect();
                println!(
                    "  near node {node}: batch {index} '{}' {:?} {:?} lightmap image {} ({}x{}, mean {mean:?}){}: {} triangles, nearest {:.1}",
                    m.name,
                    m.kind,
                    m.alpha,
                    m.images[8],
                    lightmap.width,
                    lightmap.height,
                    if bri_render::scene::decomposed_lightmap(m.parameters) { " decomposed" } else { "" },
                    near.len(),
                    near.iter().copied().fold(f32::INFINITY, f32::min)
                );
                // Within 8 units (a lamp and its shade): per triangle, its
                // lightmap texel, the Dynamic leftover and light shares
                // there, and which of the shape's lights it faces.
                let Some(sheet) = u.dynamic.iter().find(|d| d.parts_image as usize == m.images[9]) else { continue };
                let range = batch.indices.start as usize..batch.indices.end as usize;
                for t in scene.indices[range].chunks_exact(3) {
                    let v: Vec<&bri_render::scene::SceneVertex> = t.iter().map(|&i| &scene.vertices[i as usize]).collect();
                    let at = v.iter().map(|v| Vec3::from(v.position)).sum::<Vec3>() / 3.0;
                    if v.iter().all(|v| centre.distance(Vec3::from(v.position)) > 8.0) {
                        continue;
                    }
                    let normal = v.iter().map(|v| Vec3::from(v.normal)).sum::<Vec3>().normalize_or_zero();
                    let uv = v.iter().fold([0.0f32; 2], |a, v| [a[0] + v.lightmap_uv[0] / 3.0, a[1] + v.lightmap_uv[1] / 3.0]);
                    let texel = |w: u32, h: u32| {
                        let x = ((uv[0] * w as f32) as u32).min(w - 1);
                        let y = ((uv[1] * h as f32) as u32).min(h - 1);
                        (y * w + x) as usize
                    };
                    let i = texel(lightmap.width, lightmap.height);
                    let j = texel(sheet.width, sheet.height);
                    let shares: Vec<String> = sheet
                        .lights
                        .iter()
                        .enumerate()
                        .map(|(c, &k)| (k, sheet.visibility[c / 4][j * 4 + c % 4]))
                        .filter(|&(_, s)| s > 0)
                        .map(|(k, s)| format!("{k}:{s}"))
                        .collect();
                    let facing: Vec<String> = owned
                        .iter()
                        .filter(|(_, o)| o.contains(&node))
                        .map(|&(k, _)| {
                            let l = &u.lights[k];
                            let delta = Vec3::from(l.position) - at;
                            format!("{k}:{}{:.1}", if normal.dot(delta) > 0.0 { "faces " } else { "away " }, delta.length())
                        })
                        .collect();
                    println!(
                        "    tri at {:.1?} normal {:.2?}: lightmap {:?} left {:?} shares [{}] owned lights [{}]",
                        at.to_array(),
                        normal.to_array(),
                        &lightmap.rgba[i * 4..i * 4 + 3],
                        &sheet.left[j * 4..j * 4 + 4],
                        shares.join(" "),
                        facing.join(" ")
                    );
                }
            }
        }
        // BRI_DUMP_LEFT=1: each Dynamic sheet as `left-{image}.png`, its
        // decomposed light beside what stays with every light off.
        if std::env::var("BRI_DUMP_LEFT").is_ok_and(|v| v == "1") {
            for d in &u.dynamic {
                let parts = &scene.images[d.parts_image as usize];
                let (w, h) = (d.width, d.height);
                let mut pixels = vec![255u8; (w * 2 * h * 4) as usize];
                for y in 0..h {
                    for x in 0..w {
                        let i = ((y * w + x) * 4) as usize;
                        let row = (y * w * 2) as usize;
                        let (a, b) = ((row + x as usize) * 4, (row + (w + x) as usize) * 4);
                        pixels[a..a + 3].copy_from_slice(parts.rgba.get(i..i + 3).unwrap_or(&[0; 3]));
                        pixels[b..b + 3].copy_from_slice(&d.left[i..i + 3]);
                    }
                }
                image::save_buffer(out.join(format!("left-{}.png", d.parts_image)), &pixels, w * 2, h, image::ColorType::Rgba8)?;
            }
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

    let brick_lights = if std::env::var("BRI_BRICK_LIGHTS").is_ok_and(|v| v == "1")
        || std::env::var("BRI_TERMS").is_ok_and(|v| v == "1")
    {
        brick_lights(&paths.effects_runtime, public.clone(), &meshes)?
    } else {
        vec![]
    };
    if std::env::var("BRI_TERMS").is_ok_and(|v| v == "1") {
        light_terms(&world, &meshes, &scene, unified.as_ref(), classic.as_ref(), &brick_lights);
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

    // BRI_PIXELS=view:x,y;x,y (pixels of the 1920x1080 `{view}-*.png`):
    // the map surface under each pixel, and for its lightmap texel and the
    // eight around it how the bake split the light (what the rays see, each
    // light's share) and what is left with the bulbs and tubes broken.
    if let (Some(spec), Some(pristine), Some(u)) = (&pixels, &pristine, &unified) {
        let (view, list) = spec.split_once(':').context("BRI_PIXELS=view:x,y;x,y")?;
        let (_, eye, look) = views.iter().find(|(n, _, _)| n == view).context("BRI_PIXELS names no view")?;
        let (width, height) = (1920.0f32, 1080.0f32);
        let forward = (*look - *eye).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward);
        let broken: Vec<bool> = bri_render::map_lighting::fixture_owners(&u.lights, &light_shapes)
            .iter()
            .map(|o| !o.is_empty())
            .collect();
        let bake = Bake::new(pristine).context("no lightmapped interior")?;
        for pixel in list.split(';') {
            let v: Vec<f32> = pixel.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            ensure!(v.len() == 2, "BRI_PIXELS: x,y pairs");
            // As the renderer's camera: 90 degrees across.
            let x = (v[0] + 0.5) / width * 2.0 - 1.0;
            let y = 1.0 - (v[1] + 0.5) / height * 2.0;
            let ray = (forward + right * x + up * y * (height / width)).normalize();
            let mut best: Option<(f32, usize, [u32; 3], [f32; 3])> = None;
            for (b, batch) in pristine.batches.iter().enumerate() {
                for t in pristine.indices[batch.indices.start as usize..batch.indices.end as usize].chunks_exact(3) {
                    let p = [0, 1, 2].map(|k| Vec3::from(pristine.vertices[t[k] as usize].position));
                    let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
                    let h = ray.cross(e2);
                    let det = e1.dot(h);
                    if det.abs() < 1e-9 {
                        continue;
                    }
                    let f = 1.0 / det;
                    let s = *eye - p[0];
                    let bu = f * s.dot(h);
                    let q = s.cross(e1);
                    let bv = f * ray.dot(q);
                    let d = f * e2.dot(q);
                    if bu < 0.0 || bv < 0.0 || bu + bv > 1.0 || d <= 0.05 || best.is_some_and(|b| b.0 <= d) {
                        continue;
                    }
                    best = Some((d, b, [t[0], t[1], t[2]], [1.0 - bu - bv, bu, bv]));
                }
            }
            let Some((d, b, tri, w)) = best else {
                println!("Pixel {pixel}: no surface");
                continue;
            };
            let m = &pristine.materials[pristine.batches[b].material];
            let at = *eye + ray * d;
            let uv = tri.iter().zip(w).fold([0.0f32; 2], |a, (&i, w)| {
                let t = pristine.vertices[i as usize].lightmap_uv;
                [a[0] + t[0] * w, a[1] + t[1] * w]
            });
            println!("Pixel {pixel}: batch {b} '{}' at {:.2?}, lightmap uv {:.4?}", m.name, at.to_array(), uv);
            if !bri_render::scene::decomposed_lightmap(m.parameters) {
                println!("  not a decomposed lightmap");
                continue;
            }
            let parts = &pristine.images[m.images[9]];
            let (pw, ph) = (parts.width as i64, parts.height as i64);
            let (cx, cy) = ((uv[0] * pw as f32) as i64, (uv[1] * ph as f32) as i64);
            let texels: Vec<(u32, u32)> = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (cx + dx, cy + dy)))
                .filter(|&(x, y)| x >= 0 && y >= 0 && x < pw && y < ph)
                .map(|(x, y)| (m.images[9] as u32, (y * pw + x) as u32))
                .collect();
            for line in bake.explain(&u.lights, &texels) {
                println!("  {line}");
            }
            // What the Dynamic lightmaps draw there, bulbs whole and broken.
            if let Some(sheet) = u.dynamic.iter().find(|d| d.parts_image as usize == m.images[9]) {
                for &(_, i) in &texels {
                    let i = i as usize;
                    let left = Vec3::new(sheet.left[i * 4] as f32, sheet.left[i * 4 + 1] as f32, sheet.left[i * 4 + 2] as f32);
                    let (mut whole, mut broke) = (left, left);
                    for (c, &k) in sheet.lights.iter().enumerate() {
                        let l = &u.lights[k as usize];
                        let distance = Vec3::from(l.position).distance(at);
                        let falloff = ((l.outer - distance) / (l.outer - l.inner).max(1e-3)).clamp(0.0, 1.0);
                        let given = Vec3::from(l.color) * falloff * 255.0 * f32::from(sheet.visibility[c / 4][i * 4 + c % 4]) / 255.0;
                        whole += given;
                        if !broken[k as usize] {
                            broke += given;
                        }
                    }
                    println!(
                        "  texel {}:{i} drawn: leftover {:.0?}, with the lights {:.0?}, with the bulbs broken {:.0?} (before the live sun and ambient)",
                        m.images[9],
                        left.to_array(),
                        whole.min(Vec3::splat(255.0)).to_array(),
                        broke.min(Vec3::splat(255.0)).to_array()
                    );
                }
            }
        }
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
    // As the client: the per-texel lightmaps in Dynamic, and in the Unified
    // modes on a map with bulbs or tubes.
    if let Some(u) = unified.as_ref().filter(|_| dynamic || !light_shapes.is_empty()) {
        let equipped = bri_render::map_lighting::DynamicSheet::equip(&u.dynamic, &mut scene);
        println!("Dynamic lightmaps: {} sheets, equipped {equipped}", u.dynamic.len());
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
                // BRI_OFF=i,j,...: those recovered lights switched off, as a
                // broken bulb or tube does (the "Light shape" lines name
                // each shape's lights).
                // BRI_BREAK=1: every bulb and tube broken, by the client's
                // rule (their lights off).
                let off: Vec<usize> = std::env::var("BRI_OFF")
                    .map(|text| text.split(',').filter_map(|x| x.trim().parse().ok()).collect())
                    .unwrap_or_default();
                let broken = std::env::var("BRI_BREAK").is_ok_and(|v| v == "1");
                if broken || !off.is_empty() {
                    let owners = bri_render::map_lighting::fixture_owners(&u.lights, &light_shapes);
                    let tints: Vec<Vec3> = (0..u.lights.len())
                        .map(|i| if off.contains(&i) || (broken && !owners[i].is_empty()) { Vec3::ZERO } else { Vec3::ONE })
                        .collect();
                    renderer.set_map_light_tints(&queue, &tints);
                }
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
            if std::env::var("BRI_BRICK_LIGHTS").is_ok_and(|v| v == "1") {
                let mut near = brick_lights.clone();
                near.sort_by(|a, b| {
                    let d = |l: &bri_render::scene::PointLight| {
                        Vec3::from_slice(&l.position_radius[..3]).distance_squared(*eye)
                    };
                    d(a).total_cmp(&d(b))
                });
                near.truncate(256);
                renderer.update_lights(&queue, &near)?;
            }
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
        "map": map_id, "world": if synthetic_count.is_some() { "synthetic".to_string() } else { entry.name.clone() },
        "bricks": world.bricks.len(), "chunks": chunks.len(), "mesh_ms": mesh_ms, "brick_shadows": brick_shadows,
        "classic_volume_bake_ms": classic_ms, "map_lighting_ms": unified_ms,
        "map_lights": unified.as_ref().map(|u| json!({"lights": u.lights, "report": u.report})),
        "frames": report,
    });
    std::fs::write(out.join("report.json"), serde_json::to_string_pretty(&report)?)?;
    Ok(())
}
