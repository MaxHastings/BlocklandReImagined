//! Headless engine performance probe. Loads a real map and saved build, adds
//! simulated players, Blockhead bots in a running minigame and parked
//! vehicles, then measures the authoritative tick, replication encoding,
//! client mesh/collision rebuilds and offscreen GPU frames. It never opens a
//! window or reads input; the GPU pass renders into an offscreen texture.
//!
//! Usage: perf_probe <content-root> <report.json> [world-name-substring]
use anyhow::{Context, Result, ensure};
use bri_client::content::ClientContent;
use bri_net::protocol::{Checkpoint, Datagram, Pose, PublicWorld, public_brick};
use bri_render::{
    scene::{Camera, SceneRenderer, create_depth},
    scene_loader::load_map_bundle,
};
use bri_sim::{
    player::MoveInput,
    session::{Command, MiniGameRequest, Session, ToolCatalog},
};
use bri_world::{Brick, ContentRef, VehicleSpawn};
use glam::Vec3;
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const SPAWN_BRICK: &str = "v20/brick/brickvehiclespawndata";
const BOTS: usize = 16;
const HUMANS: usize = 8;
const VEHICLES: [&str; 4] = [
    "v20.vehicle.jeepvehicle",
    "v20.vehicle.tankvehicle",
    "v20.vehicle.horsearmor",
    "v20.vehicle.jeepvehicle",
];
const TICKS: usize = 120 * 20;

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
fn percentiles(samples: &mut [f64]) -> serde_json::Value {
    samples.sort_by(f64::total_cmp);
    let at = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
    json!({
        "samples": samples.len(),
        "mean_ms": samples.iter().sum::<f64>() / samples.len() as f64,
        "p50_ms": at(0.5), "p95_ms": at(0.95), "p99_ms": at(0.99), "max_ms": at(1.0),
    })
}
/// Deterministic xorshift so runs are comparable.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        (2..=3).contains(&args.len()),
        "Usage: perf_probe <content-root> <report.json> [world-name-substring]"
    );
    let root = PathBuf::from(&args[0]);
    let report_path = PathBuf::from(&args[1]);
    let wanted = args.get(2).map_or("Golden Gate", String::as_str);

    let started = Instant::now();
    let content = ClientContent::load(&root)?;
    let entry = content
        .worlds
        .iter()
        .filter(|w| w.loadable && w.name.contains(wanted))
        .max_by_key(|w| w.brick_count)
        .with_context(|| format!("No loadable reference world matches {wanted:?}"))?
        .clone();
    println!(
        "World {} ({} bricks) on {}",
        entry.name, entry.brick_count, entry.map_id
    );
    let paths = content.paths.clone();

    // ---- Authoritative load ----------------------------------------------
    let load_start = Instant::now();
    let loaded = paths.load_map(&entry.map_id, Some(&entry.id))?;
    let server_load_ms = ms(load_start.elapsed());
    let spawn = loaded.spawn_points[0];
    // Spawn bricks for bots and vehicles arrive as a build loaded by the first
    // (host) player, so the bots follow that player's minigame. This measures
    // load, not placement validation.
    let height = loaded.simulation.definitions.entries[SPAWN_BRICK]
        .mesh
        .height_plates as f32
        * 0.2;
    let base = Vec3::new(
        (spawn.x * 2.0).round() / 2.0,
        (spawn.y / 0.2).round() * 0.2 + height * 0.5,
        (spawn.z * 2.0).round() / 2.0,
    );
    let state = loaded.simulation.state();
    let mut spawners = bri_world::World::new(
        "Probe spawners".into(),
        state.map_id.clone(),
        state.palette.clone(),
    );
    for i in 0..BOTS + VEHICLES.len() {
        let (x, z) = ((i % 5) as f32 * 8.0 - 16.0, (i / 5) as f32 * 8.0 + 6.0);
        let mut brick = Brick::new(
            ContentRef::Resolved(SPAWN_BRICK.into()),
            [base.x + x, base.y, base.z - z],
            0,
        );
        let kind = if i < BOTS {
            "bot.blockhead"
        } else {
            VEHICLES[i - BOTS]
        };
        brick.vehicle = Some(VehicleSpawn {
            vehicle: ContentRef::Resolved(kind.into()),
            recolor: false,
        });
        spawners.bricks.insert(i as u64 + 1, brick);
    }
    spawners.next_brick_id = (BOTS + VEHICLES.len()) as u64 + 1;
    let definitions = loaded.simulation.definitions.clone();
    let waters = loaded.simulation.waters.clone();
    let simulation = loaded.simulation;

    let weapons = bri_net::content_identity::WeaponContent::load(&paths.weapons)?;
    let item_physics =
        bri_net::content_identity::ItemPhysicsContent::load(&paths.item_presentation, &weapons)?;
    let mut session = Session::new(simulation);
    let mut catalog = ToolCatalog {
        vehicles: VEHICLES
            .iter()
            .map(|v| v.to_string())
            .chain(["bot.blockhead".to_string()])
            .collect(),
        vehicle_bricks: [SPAWN_BRICK.to_string()].into(),
        ..Default::default()
    };
    catalog.install_items(weapons.item_choices.iter().map(|(id, _)| id.clone()))?;
    session.set_tool_catalog(catalog)?;
    session.set_weapon_pack(weapons.pack.clone())?;
    session.set_item_bounds(item_physics.bounds)?;
    session.set_vehicle_pack(bri_vehicles::Pack::load(
        paths.vehicles.join("vehicles.json"),
    )?)?;
    session.set_spawn_points(loaded.spawn_points.clone())?;
    let mut humans = Vec::new();
    for i in 0..HUMANS {
        let owner = loaded
            .spawn_points
            .iter()
            .find_map(|point| session.join(format!("Player {i}"), *point, i == 0).ok())
            .context("No unobstructed spawn point for a simulated player")?;
        humans.push(owner);
    }
    session.command(
        humans[0],
        1,
        Command::LoadBuild {
            build: Box::new(bri_world::build::SavedBuild::new(spawners)),
            ownership: false,
        },
    )?;
    let mut sequence = [0u64; HUMANS];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut inputs: Vec<MoveInput> = vec![MoveInput::default(); HUMANS];
    let mut feed = |session: &mut Session, rng: &mut Rng, tick: usize| -> Result<()> {
        for (i, owner) in humans.iter().enumerate() {
            // New intent twice a second: walk, strafe, jump and jet.
            if tick % 60 == i % 60 {
                inputs[i] = MoveInput {
                    forward: rng.next() * 2.0 - 1.0,
                    right: rng.next() * 2.0 - 1.0,
                    yaw: rng.next() * std::f32::consts::TAU - std::f32::consts::PI,
                    pitch: 0.0,
                    head_yaw: 0.0,
                    jump: rng.next() < 0.2,
                    crouch: false,
                    jet: rng.next() < 0.3,
                };
            }
            sequence[i] += 1;
            session.movement(*owner, sequence[i], inputs[i])?;
        }
        Ok(())
    };
    // Settle bots and vehicles before the minigame starts.
    for tick in 0..120 {
        feed(&mut session, &mut rng, tick)?;
        session.step()?;
    }
    session.command(
        humans[0],
        2,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Default::default(),
        }),
    )?;
    let game = session
        .minigame_views()
        .first()
        .map(|m| m.id)
        .context("Minigame was not created")?;
    for (i, owner) in humans.iter().enumerate().skip(1) {
        let _ = session.command(
            *owner,
            i as u64 + 10,
            Command::MiniGame(MiniGameRequest::Join { game }),
        );
    }
    let bots = session
        .names()
        .keys()
        .filter(|o| session.is_bot(**o))
        .count();
    let vehicles = session.vehicle_infos().len();
    println!("{bots} bots, {vehicles} vehicles, {HUMANS} players");

    let mut step_ms = Vec::with_capacity(TICKS);
    let mut replication_ms = Vec::new();
    let mut pose_bytes = 0usize;
    let mut pose_datagrams = 0usize;
    let mut max_pose = 0usize;
    let mut delta_bytes = 0usize;
    for tick in 0..TICKS {
        feed(&mut session, &mut rng, tick)?;
        let t = Instant::now();
        session.step()?;
        step_ms.push(ms(t.elapsed()));
        let now = session.simulation().state().tick;
        if now.is_multiple_of(bri_net::protocol::POSE_INTERVAL) {
            let t = Instant::now();
            for (player, acknowledged_input) in session.motion_states() {
                let bytes = bri_net::codec::encode_datagram(&Datagram::Pose(Pose {
                    tick: now,
                    acknowledged_input,
                    player,
                }))?;
                max_pose = max_pose.max(bytes.len());
                pose_bytes += bytes.len();
                pose_datagrams += 1;
            }
            for pose in session.vehicle_poses() {
                pose_bytes += bri_net::codec::encode_datagram(&Datagram::Vehicle(pose))?.len();
                pose_datagrams += 1;
            }
            replication_ms.push(ms(t.elapsed()));
        }
        if now.is_multiple_of(6) {
            let dirty = session.take_dirty();
            let bricks: BTreeMap<_, _> = dirty
                .iter()
                .map(|id| {
                    (
                        *id,
                        session
                            .simulation()
                            .state()
                            .bricks
                            .get(id)
                            .map(public_brick),
                    )
                })
                .collect();
            delta_bytes += bri_net::codec::encode(&(
                bricks,
                session.vitals(),
                session.weapon_view(),
                session.take_cues(),
            ))?
            .len();
        }
    }
    let seconds = TICKS as f64 / 120.0;
    let over_budget = step_ms.iter().filter(|v| **v > 1000.0 / 120.0).count();

    // Late join: an O(1) snapshot on the authority loop, then the world is
    // chunked and compressed off it and reassembled by the client.
    let t = Instant::now();
    let (checkpoint, bricks) = Checkpoint::from_session(&session, 0);
    let checkpoint_build_ms = ms(t.elapsed());
    let t = Instant::now();
    let frames = bri_net::protocol::WorldTransfer {
        head: bri_net::protocol::Message::MapChanged(checkpoint),
        bricks,
    }
    .encode()?;
    let checkpoint_encode_ms = ms(t.elapsed());
    let checkpoint_bytes: usize = frames.iter().map(Vec::len).sum();
    let t = Instant::now();
    let bri_net::protocol::Message::MapChanged(head) = bri_net::codec::decode(&frames[0])? else {
        anyhow::bail!("Expected a checkpoint")
    };
    let mut assembly = bri_net::protocol::WorldAssembly::new(head)?;
    for frame in &frames[1..] {
        let bri_net::protocol::Message::WorldChunk(chunk) = bri_net::codec::decode(frame)? else {
            anyhow::bail!("Expected a world chunk")
        };
        assembly.add(chunk)?;
    }
    let decoded = assembly.finish()?;
    let checkpoint_decode_ms = ms(t.elapsed());
    let world = Arc::new(decoded.world);

    // ---- Client: brick mesh and collision rebuilds -------------------------
    let meshes: BTreeMap<_, _> = definitions
        .entries
        .iter()
        .map(|(id, def)| (id.clone(), def.mesh.clone()))
        .collect();
    let materials = bri_client::materials::BrickMaterials::load(&paths.brick_materials)?;
    let palette = bri_client::world_chunks::BrickPalette::new(&materials)?;
    let mut mesh_ms = Vec::new();
    let mut chunked = None;
    for _ in 0..3 {
        let t = Instant::now();
        let mut state = bri_client::world_chunks::ChunkedWorld::default();
        let changes = state.update(
            world.clone(),
            None,
            &meshes,
            &palette,
            Some(&materials),
            4_000_000,
        )?;
        mesh_ms.push(ms(t.elapsed()));
        chunked = Some((state, changes));
    }
    let (mut chunked, world_chunks) = chunked.unwrap();
    let world_chunks: Vec<_> = world_chunks
        .into_iter()
        .filter_map(|(key, scene)| Some((key, scene?)))
        .collect();
    let mut mirror = bri_sim::prediction::CollisionMirror::new(
        definitions.clone(),
        loaded.query_colliders.clone(),
        waters,
    );
    let t = Instant::now();
    mirror.sync(&world.bricks)?;
    let mirror_full_ms = ms(t.elapsed());
    let mut building =
        bri_client::building::Building::new(definitions.clone(), loaded.query_colliders.clone())?;
    let t = Instant::now();
    building.sync_world(&world)?;
    let building_full_ms = ms(t.elapsed());
    // One planted brick: the client diffs the replica, rebuilds the touched
    // chunk and re-syncs the collision mirror.
    let mut one_more: PublicWorld = (*world).clone();
    let mut planted = one_more
        .bricks
        .values()
        .next()
        .context("empty world")?
        .clone();
    planted.position[1] += 50.0;
    one_more.bricks.insert(u64::MAX - 1, planted);
    let t = Instant::now();
    mirror.sync_changes(&one_more.bricks, [u64::MAX - 1])?;
    let mirror_one_ms = ms(t.elapsed());
    let t = Instant::now();
    building.sync_world_changes(
        &one_more,
        Some(&bri_client::network::WorldChanges {
            bricks: [u64::MAX - 1].into(),
            palette: false,
        }),
    )?;
    let building_one_ms = ms(t.elapsed());
    // The previous path rebuilt the whole world; it is also the render reference.
    let reference = Instant::now();
    let full_world = bri_client::world_scene::build_world_scene_materials(
        &world,
        &meshes,
        4_000_000,
        Some(&materials),
    )?;
    let full_rebuild_reference_ms = ms(reference.elapsed());
    let same = Arc::new((*world).clone());
    let t = Instant::now();
    let unchanged = chunked.update(same, None, &meshes, &palette, Some(&materials), 4_000_000)?;
    ensure!(unchanged.is_empty(), "Unchanged replica rebuilt chunks");
    let diff_only_ms = ms(t.elapsed());
    let one_more = Arc::new(one_more);
    let t = Instant::now();
    let one_brick_changes = chunked.update(
        one_more,
        Some(&bri_client::network::WorldChanges {
            bricks: [u64::MAX - 1].into(),
            palette: false,
        }),
        &meshes,
        &palette,
        Some(&materials),
        4_000_000,
    )?;
    let one_brick_mesh_ms = ms(t.elapsed());
    let (one_brick_key, one_brick_chunk) = one_brick_changes
        .into_iter()
        .find_map(|(key, scene)| Some((key, scene?)))
        .context("Planted brick rebuilt no chunk")?;
    let one_brick_chunk_bricks = chunked.chunk_bricks(one_brick_key);
    let largest_chunk_bricks = world_chunks
        .iter()
        .map(|(key, _)| chunked.chunk_bricks(*key))
        .max()
        .unwrap_or(0);

    // ---- GPU: offscreen frames ---------------------------------------------
    let map_scene = load_map_bundle(&paths.map_bundle, &entry.map_id)?.scene;
    let snapshots = report_path.with_extension("snapshots");
    let gpu = gpu_frames(
        &snapshots,
        &full_world,
        &map_scene,
        &palette,
        &world_chunks,
        &one_brick_chunk,
        &world,
        spawn,
    )
    .unwrap_or_else(|e| json!({ "error": format!("{e:#}") }));

    let report = json!({
        "world": { "name": entry.name, "map": entry.map_id, "bricks": entry.brick_count },
        "load": { "total_ms": ms(started.elapsed()), "server_map_and_world_ms": server_load_ms },
        "population": { "players": HUMANS, "bots": bots, "vehicles": vehicles },
        "server_tick": {
            "budget_ms": 1000.0 / 120.0,
            "ticks_over_budget": over_budget,
            "step": percentiles(&mut step_ms),
        },
        "replication": {
            "pose_encode_per_interval": percentiles(&mut replication_ms),
            "pose_datagrams_per_second_per_peer": pose_datagrams as f64 / seconds,
            "pose_bytes_per_second_per_peer": pose_bytes as f64 / seconds,
            "largest_pose_datagram_bytes": max_pose,
            "reliable_delta_bytes_per_second": delta_bytes as f64 / seconds,
            "checkpoint_build_ms": checkpoint_build_ms,
            "checkpoint_encode_ms": checkpoint_encode_ms,
            "checkpoint_decode_ms": checkpoint_decode_ms,
            "checkpoint_compressed_bytes": checkpoint_bytes,
        },
        "client": {
            "world_mesh_build": percentiles(&mut mesh_ms),
            "world_triangles": chunked.triangles(),
            "world_vertex_bytes": world_chunks.iter().map(|(_, c)| c.vertices.len()).sum::<usize>() * std::mem::size_of::<bri_render::scene::SceneVertex>(),
            "world_chunks": world_chunks.len(),
            "world_batches": world_chunks.iter().map(|(_, c)| c.batches.len()).sum::<usize>(),
            "one_brick_rebuild_ms": one_brick_mesh_ms,
            "one_brick_full_rebuild_reference_ms": full_rebuild_reference_ms,
            "unchanged_replica_diff_ms": diff_only_ms,
            "one_brick_chunk_bricks": one_brick_chunk_bricks,
            "largest_chunk_bricks": largest_chunk_bricks,
            "one_brick_chunk_vertex_bytes": one_brick_chunk.vertices.len() * std::mem::size_of::<bri_render::scene::SceneVertex>(),
            "collision_mirror_full_ms": mirror_full_ms,
            "collision_mirror_one_brick_ms": mirror_one_ms,
            "building_query_full_ms": building_full_ms,
            "building_query_one_brick_ms": building_one_ms,
            "map_triangles": map_scene.indices.len() / 3,
            "map_batches": map_scene.batches.len(),
            "map_images": map_scene.images.len(),
        },
        "gpu": gpu,
    });
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Read an Rgba8 render target back to tightly packed pixels.
fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
) -> Result<Vec<u8>> {
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

#[allow(clippy::too_many_arguments)] // offscreen harness inputs
fn gpu_frames(
    snapshots: &std::path::Path,
    reference_world: &bri_render::scene::SceneData,
    map: &bri_render::scene::SceneData,
    palette: &bri_client::world_chunks::BrickPalette,
    chunks: &[(
        bri_client::world_chunks::ChunkKey,
        bri_render::scene::SceneData,
    )],
    planted: &bri_render::scene::SceneData,
    public: &PublicWorld,
    spawn: Vec3,
) -> Result<serde_json::Value> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .context("No headless GPU adapter")?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("perf probe"),
        required_limits: adapter.limits(),
        ..Default::default()
    }))?;
    let (width, height) = (1920, 1080);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("perf target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, width, height).create_view(&Default::default());
    let mut renderer = SceneRenderer::new(&device, format);
    let t = Instant::now();
    let gpu_map = renderer.upload(&device, &queue, map)?;
    let gpu_palette = renderer.upload(&device, &queue, &palette.scene)?;
    let gpu_world = chunks
        .iter()
        .map(|(_, chunk)| renderer.upload_chunk(&device, chunk, &gpu_palette))
        .collect::<Result<Vec<_>>>()?;
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    let upload_ms = ms(t.elapsed());
    let t = Instant::now();
    let _planted = renderer.upload_chunk(&device, planted, &gpu_palette)?;
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    let one_chunk_upload_ms = ms(t.elapsed());
    let mut scenes = vec![&gpu_map];
    scenes.extend(gpu_world.iter());
    let gpu_reference = renderer.upload(&device, &queue, reference_world)?;
    std::fs::create_dir_all(snapshots)?;

    let (min, max) = public.bricks.values().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), b| {
            (
                lo.min(Vec3::from(b.position)),
                hi.max(Vec3::from(b.position)),
            )
        },
    );
    let center = (min + max) * 0.5;
    let extent = (max - min).length().max(20.0);
    let views = [
        ("spawn_eye", spawn + Vec3::Y * 2.4, center),
        (
            "overview",
            center + Vec3::new(extent * 0.6, extent * 0.4, extent * 0.6),
            center,
        ),
        // Inside the shadow distance, looking down across the build.
        ("near", center + Vec3::new(40.0, 25.0, 40.0), center),
    ];
    let mut out = serde_json::Map::new();
    for &(name, eye, look) in &views {
        let mut camera = Camera::perspective(
            eye.to_array(),
            look.to_array(),
            width as f32 / height as f32,
            90f32.to_radians(),
            0.05,
            4000.0,
        );
        camera.apply_environment(map);
        renderer.update_camera(&queue, &camera);
        let mut frames = Vec::new();
        for i in 0..70 {
            let t = Instant::now();
            let mut encoder = device.create_command_encoder(&Default::default());
            renderer.render(
                &mut encoder,
                &view,
                &depth,
                &scenes,
                Some(wgpu::Color::BLACK),
            );
            queue.submit([encoder.finish()]);
            device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })?;
            if i >= 10 {
                frames.push(ms(t.elapsed()));
            }
        }
        // Chunked (culled) and whole-world renders must match; coplanar
        // faces may resolve differently, so report the fraction that differs.
        let chunked = read_back(&device, &queue, &target)?;
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render(
            &mut encoder,
            &view,
            &depth,
            &[&gpu_map, &gpu_reference],
            Some(wgpu::Color::BLACK),
        );
        queue.submit([encoder.finish()]);
        let reference = read_back(&device, &queue, &target)?;
        let differing = chunked
            .chunks_exact(4)
            .zip(reference.chunks_exact(4))
            .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
            .count();
        for (label, pixels) in [("chunked", &chunked), ("reference", &reference)] {
            image::save_buffer(
                snapshots.join(format!("{name}-{label}.png")),
                pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )?;
        }
        let mut stats = percentiles(&mut frames);
        stats["pixels_differing_from_whole_world"] =
            json!(differing as f64 / (width * height) as f64);
        out.insert(name.into(), stats);
    }
    let variants = quality_variants(
        &device,
        &queue,
        snapshots,
        map,
        palette,
        chunks,
        &views,
        (width, height),
    )?;
    Ok(json!({
        "adapter": format!("{} ({:?})", info.name, info.backend),
        "resolution": [width, height],
        "upload_map_and_world_ms": upload_ms,
        "one_brick_chunk_upload_ms": one_chunk_upload_ms,
        "frames": out,
        "quality_variants": variants,
    }))
}

/// Render each view with each graphics option set the client offers, saving
/// `{view}-{variant}.png` and frame times, as the client composes its passes.
#[allow(clippy::too_many_arguments)] // offscreen harness inputs
fn quality_variants(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    snapshots: &std::path::Path,
    map: &bri_render::scene::SceneData,
    palette: &bri_client::world_chunks::BrickPalette,
    chunks: &[(
        bri_client::world_chunks::ChunkKey,
        bri_render::scene::SceneData,
    )],
    views: &[(&str, Vec3, Vec3)],
    (width, height): (u32, u32),
) -> Result<serde_json::Value> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let texture = |samples: u32, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("variant target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let resolved = texture(
        1,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let resolved_view = resolved.create_view(&Default::default());
    let mut out = serde_json::Map::new();
    use bri_render::shadow::ShadowSettings;
    for (variant, samples, shadows) in [
        ("no-msaa", 1, None),
        ("msaa4", 4, None),
        ("msaa4-shadows-low", 4, Some(ShadowSettings::LOW)),
        ("msaa4-shadows-best", 4, Some(ShadowSettings::BEST)),
    ] {
        let mut renderer = SceneRenderer::with_settings(device, format, samples, shadows);
        let gpu_map = renderer.upload(device, queue, map)?;
        let gpu_palette = renderer.upload(device, queue, &palette.scene)?;
        let gpu_world = chunks
            .iter()
            .map(|(_, chunk)| renderer.upload_chunk(device, chunk, &gpu_palette))
            .collect::<Result<Vec<_>>>()?;
        let mut scenes = vec![&gpu_map];
        scenes.extend(gpu_world.iter());
        let multisampled = (samples > 1).then(|| {
            texture(samples, wgpu::TextureUsages::RENDER_ATTACHMENT)
                .create_view(&Default::default())
        });
        let depth = bri_render::scene::create_depth_samples(device, width, height, samples)
            .create_view(&Default::default());
        let mut results = serde_json::Map::new();
        for &(name, eye, look) in views {
            let mut camera = Camera::perspective(
                eye.to_array(),
                look.to_array(),
                width as f32 / height as f32,
                90f32.to_radians(),
                0.05,
                4000.0,
            );
            camera.apply_environment(map);
            renderer.update_camera(queue, &camera);
            let mut frames = Vec::new();
            for i in 0..40 {
                let t = Instant::now();
                let mut encoder = device.create_command_encoder(&Default::default());
                renderer.render_shadows(
                    &mut encoder,
                    bri_render::scene::ShadowCasters {
                        scenes: &scenes[1..],
                        instances: &[],
                    },
                    bri_render::scene::ShadowCasters {
                        scenes: &scenes[..1],
                        instances: &[],
                    },
                );
                renderer.render(
                    &mut encoder,
                    multisampled.as_ref().unwrap_or(&resolved_view),
                    &depth,
                    &scenes,
                    Some(wgpu::Color::BLACK),
                );
                if let Some(color) = &multisampled {
                    // The client resolves in its last world pass.
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("variant resolve"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: color,
                            depth_slice: None,
                            resolve_target: Some(&resolved_view),
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Discard,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                }
                queue.submit([encoder.finish()]);
                device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })?;
                if i >= 10 {
                    frames.push(ms(t.elapsed()));
                }
            }
            let pixels = read_back(device, queue, &resolved)?;
            image::save_buffer(
                snapshots.join(format!("{name}-{variant}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )?;
            let mut stats = percentiles(&mut frames);
            if shadows.is_some() {
                // The caster passes alone, to separate them from receiver cost.
                let mut casters = Vec::new();
                for _ in 0..20 {
                    let t = Instant::now();
                    let mut encoder = device.create_command_encoder(&Default::default());
                    renderer.render_shadows(
                        &mut encoder,
                        bri_render::scene::ShadowCasters {
                            scenes: &scenes[1..],
                            instances: &[],
                        },
                        bri_render::scene::ShadowCasters {
                            scenes: &scenes[..1],
                            instances: &[],
                        },
                    );
                    queue.submit([encoder.finish()]);
                    device.poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: None,
                    })?;
                    casters.push(ms(t.elapsed()));
                }
                stats["shadow_casters_only"] = percentiles(&mut casters);
            }
            results.insert(name.into(), stats);
        }
        out.insert(variant.into(), results.into());
    }
    Ok(out.into())
}
