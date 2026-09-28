//! Headless knocked-out-brick debris probe. Loads a real map and saved build,
//! then replays the client's per-frame debris work (query mirror sync,
//! hidden-brick ghosts, `BrickDebris` cues and physics, debris GPU models and
//! an offscreen draw) around a rocket blast, a Destructo Wand chain and a
//! mass kill past the body cap. Reports each frame's cost by stage and the
//! worst frames. It never opens a window or reads input.
//!
//! Usage: debris_probe <content-root> <report.json> [world-name-substring]
use anyhow::{Context, Result, ensure};
use bri_client::brick_debris::{BrickDebris, DebrisModels};
use bri_client::building::Building;
use bri_client::content::ClientContent;
use bri_client::network::WorldChanges;
use bri_net::protocol::{PublicWorld, public_bricks};
use bri_render::scene::{Camera, SceneRenderer, create_depth};
use bri_sim::presentation::{Cue, CueKind};
use bri_world::BrickId;
use glam::Vec3;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const DT: f32 = 1.0 / 60.0;
/// Frames per scenario: the last body lives out its 3 s solid and 2 s fade.
const FRAMES: usize = 60 * 8;

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// One scheduled kill: on `frame`, these bricks die from this blast.
struct Kill {
    frame: usize,
    bricks: Vec<BrickId>,
    origin: Vec3,
    force: f32,
    radius: f32,
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: SceneRenderer,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
    adapter: String,
}

fn gpu() -> Result<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .context("No headless GPU adapter")?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("debris probe"),
        required_limits: adapter.limits(),
        ..Default::default()
    }))?;
    let (width, height) = (1280, 720);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("debris target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, width, height).create_view(&Default::default());
    let renderer = SceneRenderer::new(&device, format);
    Ok(Gpu {
        device,
        queue,
        renderer,
        view,
        depth,
        adapter: format!("{} ({:?})", info.name, info.backend),
    })
}

/// The `n` bricks nearest `center`, nearest first.
fn nearest(world: &PublicWorld, center: Vec3, n: usize, skip: &BTreeSet<BrickId>) -> Vec<BrickId> {
    let mut all: Vec<(f32, BrickId)> = world
        .bricks
        .iter()
        .filter(|(id, b)| b.visible && !skip.contains(id))
        .map(|(id, b)| (Vec3::from(b.position).distance_squared(center), *id))
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    all.into_iter().take(n).map(|(_, id)| id).collect()
}

#[allow(clippy::too_many_arguments)] // probe inputs
fn scenario(
    name: &str,
    kills: &[Kill],
    world: &Arc<PublicWorld>,
    building: &mut Building,
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
    materials: &bri_client::materials::BrickMaterials,
    palette: &bri_client::world_chunks::BrickPalette,
    gpu: &mut Gpu,
    wand_out: bool,
) -> Result<serde_json::Value> {
    // The client uploads the brick palette once per map, before any debris.
    let gpu_palette = gpu
        .renderer
        .upload(&gpu.device, &gpu.queue, &palette.scene)?;
    // Every scenario starts from the same intact world.
    building.sync_world(world)?;
    let mut debris = BrickDebris::new();
    let mut models = DebrisModels::default();
    let mut current = (**world).clone();
    let mut cue_id = 1_000_000u64;
    let focus = kills.first().map(|k| k.origin).unwrap_or(Vec3::ZERO);
    let camera = Camera::perspective(
        (focus + Vec3::new(12.0, 8.0, 12.0)).to_array(),
        focus.to_array(),
        16.0 / 9.0,
        90f32.to_radians(),
        0.05,
        4000.0,
    );
    gpu.renderer.update_camera(&gpu.queue, &camera);
    let stages = [
        "world_sync",
        "hidden_ghosts",
        "cues",
        "advance",
        "models_upload",
        "draw",
    ];
    let mut frames: Vec<[f64; 6]> = Vec::new();
    let mut bodies = Vec::new();
    let mut hidden_dirty = true;
    for frame in 0..FRAMES {
        let mut t = [0.0f64; 6];
        let mut cues = Vec::new();
        let mut changed = BTreeSet::new();
        for kill in kills.iter().filter(|k| k.frame == frame) {
            for id in &kill.bricks {
                // The cue captures the brick's look before it goes.
                let Some(b) = current.bricks.get(id).cloned() else {
                    continue;
                };
                cue_id += 1;
                cues.push(Cue {
                    id: cue_id,
                    tick: frame as u64,
                    position: b.position,
                    kind: CueKind::BrickKill {
                        brick: *id,
                        definition: b.definition.clone(),
                        quarter_turns: b.quarter_turns,
                        color: b.color,
                        color_effect: b.color_effect,
                        shape_effect: b.shape_effect,
                        print: b.print.clone(),
                        origin: kill.origin.to_array(),
                        force: kill.force,
                        radius: kill.radius,
                    },
                });
                current.bricks.remove(id);
                changed.insert(*id);
            }
        }
        // The replica's world update arrives with the cues (killBrick).
        if !changed.is_empty() {
            let s = Instant::now();
            building.sync_world_changes(
                &current,
                Some(&WorldChanges {
                    bricks: changed.clone(),
                    palette: false,
                }),
            )?;
            debris.sync_world(&current);
            t[0] = ms(s.elapsed());
            hidden_dirty = true;
        }
        let s = Instant::now();
        let thrown = debris.cues(&cues, building)?;
        t[2] = ms(s.elapsed());
        if thrown > 0 {
            hidden_dirty = true;
        }
        // With a brick tool, wand or hammer out, the client reveals hidden
        // bricks and rebuilds that ghost scene whenever the world changes.
        if wand_out && hidden_dirty {
            let s = Instant::now();
            let hidden = PublicWorld {
                name: "Non-rendering bricks".into(),
                map_id: current.map_id.clone(),
                palette: current.palette.clone(),
                bricks: current
                    .bricks
                    .iter()
                    .filter(|(id, b)| !b.visible && !debris.is_dead(**id))
                    .map(|(id, b)| {
                        let mut b = b.clone();
                        b.visible = true;
                        (*id, b)
                    })
                    .collect(),
            };
            if !hidden.bricks.is_empty() {
                let data = bri_client::world_scene::build_world_scene_materials(
                    &hidden,
                    meshes,
                    1_000_000,
                    Some(materials),
                )?;
                if !data.indices.is_empty() {
                    let _ = gpu.renderer.upload(&gpu.device, &gpu.queue, &data)?;
                }
            }
            t[1] = ms(s.elapsed());
            hidden_dirty = false;
        }
        let s = Instant::now();
        debris.advance(DT, building)?;
        t[3] = ms(s.elapsed());
        let s = Instant::now();
        models.upload(
            &debris,
            &gpu.renderer,
            &gpu.device,
            &gpu.queue,
            meshes,
            palette,
            &gpu_palette,
            materials,
            &current.palette,
        )?;
        t[4] = ms(s.elapsed());
        let s = Instant::now();
        let draws = models.draws();
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        gpu.renderer.render_with_instances(
            &mut encoder,
            &gpu.view,
            &gpu.depth,
            &[],
            &draws,
            Some(wgpu::Color::BLACK),
        );
        gpu.queue.submit([encoder.finish()]);
        gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })?;
        t[5] = ms(s.elapsed());
        frames.push(t);
        bodies.push(debris.len());
    }
    ensure!(debris.is_empty(), "{name}: debris outlived its fade");
    let totals: Vec<f64> = frames.iter().map(|f| f.iter().sum()).collect();
    let mut order: Vec<usize> = (0..totals.len()).collect();
    order.sort_by(|a, b| totals[*b].total_cmp(&totals[*a]));
    let worst: Vec<_> = order
        .iter()
        .take(5)
        .map(|&i| {
            let mut stage = serde_json::Map::new();
            for (s, v) in stages.iter().zip(frames[i]) {
                stage.insert((*s).into(), json!((v * 1000.0).round() / 1000.0));
            }
            json!({ "frame": i, "bodies": bodies[i], "total_ms": totals[i], "stages": stage })
        })
        .collect();
    let mut stage_max = serde_json::Map::new();
    let mut stage_sum = serde_json::Map::new();
    for (i, s) in stages.iter().enumerate() {
        stage_max.insert(
            (*s).into(),
            json!(frames.iter().map(|f| f[i]).fold(0.0, f64::max)),
        );
        stage_sum.insert((*s).into(), json!(frames.iter().map(|f| f[i]).sum::<f64>()));
    }
    let mut sorted = totals.clone();
    sorted.sort_by(f64::total_cmp);
    let summary = json!({
        "kills": kills.iter().map(|k| k.bricks.len()).sum::<usize>(),
        "peak_bodies": bodies.iter().max(),
        "worst_frame_ms": sorted.last(),
        "p99_ms": sorted[(sorted.len() - 1) * 99 / 100],
        "p50_ms": sorted[sorted.len() / 2],
        "stage_max_ms": stage_max,
        "stage_total_ms": stage_sum,
        "worst_frames": worst,
        "diagnostics": format!("{:?}", debris.diagnostics),
        "models": format!("{:?}", models.diagnostics),
    });
    println!("{name}: {}", serde_json::to_string(&summary)?);
    Ok(summary)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        (2..=3).contains(&args.len()),
        "Usage: debris_probe <content-root> <report.json> [world-name-substring]"
    );
    let root = PathBuf::from(&args[0]);
    let report_path = PathBuf::from(&args[1]);
    let wanted = args.get(2).map_or("Golden Gate", String::as_str);
    let content = ClientContent::load(&root)?;
    let entry = content
        .worlds
        .iter()
        .filter(|w| w.loadable && w.name.contains(wanted))
        .max_by_key(|w| w.brick_count)
        .with_context(|| {
            let names: Vec<_> = content
                .worlds
                .iter()
                .filter(|w| w.loadable)
                .map(|w| &w.name)
                .collect();
            format!("No loadable reference world matches {wanted:?}; have {names:?}")
        })?
        .clone();
    println!(
        "World {} ({} bricks) on {}",
        entry.name, entry.brick_count, entry.map_id
    );
    let paths = content.paths.clone();
    let loaded = paths.load_map(&entry.map_id, Some(&entry.id))?;
    let state = loaded.simulation.state();
    let world = Arc::new(PublicWorld {
        name: state.name.clone(),
        map_id: state.map_id.clone(),
        palette: state.palette.clone(),
        bricks: public_bricks(&state.bricks),
    });
    let definitions = loaded.simulation.definitions.clone();
    let meshes: BTreeMap<_, _> = definitions
        .entries
        .iter()
        .map(|(id, def)| (id.clone(), def.mesh.clone()))
        .collect();
    let materials = bri_client::materials::BrickMaterials::load(&paths.brick_materials)?;
    let palette = bri_client::world_chunks::BrickPalette::new(&materials)?;
    let mut building = Building::new(definitions, loaded.query_colliders.clone())?;
    building.set_breakables(&loaded.breakables);
    building.attach_terrain(loaded.terrain.clone());
    let mut gpu = gpu()?;

    // The blast centre: the brick nearest the middle of the build's bounds.
    let (min, max) = world.bricks.values().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), b| {
            let p = Vec3::from(b.position);
            (lo.min(p), hi.max(p))
        },
    );
    let middle = (min + max) * 0.5;
    let centre = nearest(&world, middle, 1, &BTreeSet::new())
        .first()
        .map(|id| Vec3::from(world.bricks[id].position))
        .context("empty world")?;
    let none = BTreeSet::new();

    // Rocket: v20's rocket brick explosion kills the dozen bricks around it.
    let rocket = [Kill {
        frame: 10,
        bricks: nearest(&world, centre, 12, &none),
        origin: centre,
        force: 30.0,
        radius: 3.0,
    }];
    // Destructo Wand chain: one brick at a time, a couple of frames apart,
    // spreading out from the first.
    let chain_order = nearest(&world, centre, 40, &none);
    let wand: Vec<Kill> = chain_order
        .iter()
        .enumerate()
        .map(|(i, id)| Kill {
            frame: 10 + i * 2,
            bricks: vec![*id],
            origin: Vec3::from(world.bricks[id].position) - Vec3::Y * 0.5,
            force: 12.0,
            radius: 0.0,
        })
        .collect();
    // Near the cap: 128 bricks in one blast, then two more blasts that
    // evict the oldest bodies.
    let mass_order = nearest(&world, centre, 128 + 64, &none);
    let mass = [
        Kill {
            frame: 10,
            bricks: mass_order[..128].to_vec(),
            origin: centre,
            force: 40.0,
            radius: 8.0,
        },
        Kill {
            frame: 40,
            bricks: mass_order[128..160].to_vec(),
            origin: centre,
            force: 40.0,
            radius: 8.0,
        },
        Kill {
            frame: 70,
            bricks: mass_order[160..].to_vec(),
            origin: centre,
            force: 40.0,
            radius: 8.0,
        },
    ];
    let mut report = serde_json::Map::new();
    report.insert(
        "world".into(),
        json!({ "name": entry.name, "map": entry.map_id, "bricks": entry.brick_count }),
    );
    report.insert("adapter".into(), json!(gpu.adapter));
    // A warm-up pass so one-off first-use costs show separately.
    for (name, kills, wand_out) in [
        ("rocket_cold", &rocket[..], false),
        ("rocket", &rocket[..], false),
        ("wand_chain", &wand[..], true),
        ("cap_128", &mass[..], false),
    ] {
        let result = scenario(
            name,
            kills,
            &world,
            &mut building,
            &meshes,
            &materials,
            &palette,
            &mut gpu,
            wand_out,
        )?;
        report.insert(name.into(), result);
    }
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
