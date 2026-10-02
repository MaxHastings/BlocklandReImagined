//! Headless brick-load benchmark: how fast a save reaches a server and its
//! players. No window, no input. For each workload it measures
//!
//! 1. the host's Load Bricks: `.bls` conversion, reading the converted save,
//!    and the authority publishing it (offline, CPU time, deterministic);
//! 2. the same load over loopback QUIC to the host and an already connected
//!    player (wall time and bytes on the wire);
//! 3. a player joining the finished world: time until the client holds the
//!    world, bytes received, and the client-side work before it can play
//!    (query mirror, prediction mirror and, for reference, chunk meshes,
//!    which the rendering lane owns).
//!
//! Usage: brick_load_bench <content-root> <bench-dir> <report.json> [runs] [synthetic-bricks]
//! `bench-dir` holds `birthday.bls` (Kitchen) and `christmas.bls` (Slate).
use anyhow::{Context, Result, ensure};
use bri_client::content::ClientContent;
use bri_net::{
    client::Client,
    codec,
    protocol::{Request, public_bricks},
    server::{self, ServerOptions},
};
use bri_sim::session::{Command, Reply, Session};
use bri_world::{Brick, ContentRef, World, build::SavedBuild};
use glam::Vec3;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const KITCHEN: &str = "v20/add-ons/map_kitchen/kitchen.mis";
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Process CPU time (all threads), so a busy machine inflates wall time but
/// not this.
fn cpu() -> Duration {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::FILETIME, System::Threading::*};
        let mut t = [FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        }; 4];
        let [a, b, c, d] = &mut t;
        GetProcessTimes(GetCurrentProcess(), a, b, c, d);
        let f = |t: &FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        Duration::from_nanos((f(&t[2]) + f(&t[3])) * 100)
    }
    #[cfg(not(windows))]
    Duration::ZERO
}

/// Wall and CPU time of a closure.
fn timed<T>(f: impl FnOnce() -> Result<T>) -> Result<(T, f64, f64)> {
    let (w, c) = (Instant::now(), cpu());
    let out = f()?;
    Ok((out, ms(w.elapsed()), ms(cpu() - c)))
}

struct Setup {
    content: ClientContent,
    weapons: bri_net::content_identity::WeaponContent,
    item_bounds: BTreeMap<String, bri_weapons::ItemBounds>,
    vehicles: bri_vehicles::Pack,
    meshes: Arc<BTreeMap<String, bri_content::brick::Brick>>,
    materials: Arc<bri_client::materials::BrickMaterials>,
    palette: Arc<bri_client::world_chunks::BrickPalette>,
}
impl Setup {
    fn session(&self, map: &str) -> Result<(Session, Vec<Vec3>, bri_client::content::LoadedMap)> {
        let mut loaded = self.content.load_map(map, None)?;
        let simulation = std::mem::replace(
            &mut loaded.simulation,
            bri_sim::simulation::Simulation::new(
                World::new("placeholder".into(), map.into(), vec![[1.0; 4]]),
                bri_sim::definitions::Definitions {
                    entries: BTreeMap::new(),
                },
                vec![],
            )?,
        );
        let mut session = Session::new(simulation);
        session.set_weapon_pack(self.weapons.pack.clone())?;
        session.set_item_bounds(self.item_bounds.clone())?;
        session.set_vehicle_pack(self.vehicles.clone(), self.content.paths.bot_kinds()?)?;
        session.set_event_catalog(
            self.content.events.clone(),
            self.content
                .event_sounds
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>(),
        )?;
        session.set_spawn_points(loaded.spawn_points.clone())?;
        let spawns = loaded.spawn_points.clone();
        Ok((session, spawns, loaded))
    }
}

struct Workload {
    name: &'static str,
    map: &'static str,
    build: SavedBuild,
    /// Conversion from `.bls`, when the workload has a source file.
    convert: Option<Value>,
}

/// Load `build` into a fresh offline session and step until the load ends,
/// encoding each update's bricks as the host would. CPU-timed stages.
fn offline_load(setup: &Setup, workload: &Workload) -> Result<(Value, Session, Vec<Vec3>)> {
    let (mut session, spawns, _) = setup.session(workload.map)?;
    let host = spawns
        .iter()
        .find_map(|p| session.join("Host".into(), *p, true).ok())
        .context("No spawn for the host")?;
    // Baseline step cost with nothing loading.
    for _ in 0..24 {
        session.step()?;
    }
    let (_, idle_wall, idle_cpu) = timed(|| {
        for _ in 0..120 {
            session.step()?;
        }
        Ok(())
    })?;
    let _ = session.take_dirty();
    let build = workload.build.clone();
    let (reply, accept_wall, accept_cpu) = timed(|| {
        session.command(
            host,
            1,
            Command::LoadBuild {
                build: Box::new(build),
                ownership: false,
            },
        )
    })?;
    let Reply::Loaded { bricks: total } = reply else {
        anyhow::bail!("Unexpected load reply")
    };
    let mut ticks = 0u64;
    let mut step_cpu = 0.0;
    let mut max_step = 0.0f64;
    let mut encode_cpu = 0.0;
    let mut update_bytes = 0usize;
    let mut updates = 0usize;
    let mut max_update = 0usize;
    let started = Instant::now();
    while session.build_loading() {
        let (_, wall, c) = timed(|| session.step())?;
        step_cpu += c;
        max_step = max_step.max(wall);
        ticks += 1;
        if session
            .simulation()
            .state()
            .tick
            .is_multiple_of(bri_net::protocol::UPDATE_INTERVAL)
        {
            let dirty = session.take_dirty();
            if !dirty.is_empty() {
                let state = session.simulation().state();
                let bricks: BTreeMap<_, _> = dirty
                    .into_iter()
                    .map(|id| {
                        (
                            id,
                            state.bricks.get(&id).map(bri_net::protocol::public_brick),
                        )
                    })
                    .collect();
                let (bytes, _, c) = timed(|| {
                    codec::encode(&bri_net::protocol::Message::WorldChunk(
                        bricks
                            .into_iter()
                            .filter_map(|(id, b)| Some((id, b?)))
                            .collect(),
                    ))
                })?;
                encode_cpu += c;
                update_bytes += bytes.len();
                max_update = max_update.max(bytes.len());
                updates += 1;
            }
        }
        ensure!(ticks < 120 * 3600, "Load did not finish");
    }
    let created = session.simulation().state().bricks.len();
    let (_, after_wall, _) = timed(|| {
        for _ in 0..120 {
            session.step()?;
        }
        Ok(())
    })?;
    Ok((
        json!({
            "requested": total,
            "created": created,
            "accept_wall_ms": accept_wall,
            "accept_cpu_ms": accept_cpu,
            "load_ticks": ticks,
            "load_game_seconds": ticks as f64 / 120.0,
            "load_offline_wall_ms": ms(started.elapsed()),
            "load_step_cpu_ms": step_cpu,
            "idle_step_cpu_ms_per_tick": idle_cpu / 120.0,
            "idle_step_wall_ms_per_tick": idle_wall / 120.0,
            "loaded_step_wall_ms_per_tick": after_wall / 120.0,
            "max_step_ms": max_step,
            "update_encode_cpu_ms": encode_cpu,
            "update_bytes": update_bytes,
            "updates": updates,
            "max_update_bytes": max_update,
        }),
        session,
        spawns,
    ))
}

/// Where a load's per-brick time goes: the host's slice steps done by hand
/// on a bare simulation, each timed.
fn placement_profile(setup: &Setup, workload: &Workload) -> Result<Value> {
    let mut loaded = setup.content.load_map(workload.map, None)?;
    let sim = &mut loaded.simulation;
    let build = workload.build.clone();
    let mapping = bri_world::build::LoadMapping::new(sim.state(), &build, 1, false, 2)?;
    let actor = bri_world::authority::Actor {
        owner: 1,
        administrator: true,
        ..Default::default()
    };
    let mut t = [std::time::Duration::ZERO; 7];
    let bricks = &build.world.bricks;
    let mut next = 0;
    let mut placed = 0usize;
    while next <= bricks.keys().next_back().copied().unwrap_or(0) {
        let clock = Instant::now();
        let mut slice = Vec::with_capacity(256);
        for (id, brick) in bricks.range(next..).take(256) {
            slice.push(brick.clone());
            next = id + 1;
        }
        if slice.is_empty() {
            break;
        }
        t[0] += clock.elapsed();
        let clock = Instant::now();
        let slice: Vec<_> = slice
            .into_iter()
            .filter_map(|b| mapping.brick(b).ok())
            .collect();
        t[1] += clock.elapsed();
        let clock = Instant::now();
        let slice: Vec<_> = slice.into_iter().filter(|b| sim.fits_grid(b)).collect();
        t[2] += clock.elapsed();
        let clock = Instant::now();
        let slice = sim.drop_overlapping(slice)?;
        t[3] += clock.elapsed();
        let clock = Instant::now();
        let plan = bri_world::build::LoadPlan::batch(sim.state(), &mapping.palette, slice, 2)?;
        t[4] += clock.elapsed();
        let clock = Instant::now();
        placed += sim.load_build_unrefreshed(&actor, plan)?.len();
        t[5] += clock.elapsed();
        if placed % 1024 < 256 {
            let clock = Instant::now();
            sim.step()?;
            t[6] += clock.elapsed();
        }
    }
    let names = [
        "take",
        "validate_map",
        "fits_grid",
        "drop_overlapping",
        "plan",
        "insert",
        "physics_step",
    ];
    Ok(json!({
        "placed": placed,
        "ms": names.iter().zip(t).map(|(n, d)| (n.to_string(), json!(ms(d)))).collect::<serde_json::Map<_, _>>(),
    }))
}

fn options(spawns: Vec<Vec3>) -> ServerOptions {
    ServerOptions {
        bind: "127.0.0.1:0".parse().expect("address"),
        environment: bri_package::environment::Environment::empty(),
        spawn_points: spawns,
        certificate: None,
        map_loader: None,
        packages: None,
    }
}

/// Receive until the replica holds `count` bricks; the time it got there,
/// the first brick's time and how many brick-changing updates arrived.
async fn receive_until(
    client: &mut Client,
    count: usize,
    start: Instant,
    limit: Duration,
) -> Result<(f64, Option<f64>, usize)> {
    let before = client.replica.world.bricks.len();
    let mut first = None;
    let mut world_updates = 0;
    tokio::time::timeout(limit, async {
        while client.replica.world.bricks.len() < count {
            if let bri_net::client::ClientEvent::Updated { world_changed, .. } =
                client.receive().await?
                && world_changed
            {
                world_updates += 1;
                if first.is_none() && client.replica.world.bricks.len() > before {
                    first = Some(ms(start.elapsed()));
                }
            }
        }
        Result::<()>::Ok(())
    })
    .await
    .context("Replication timed out")??;
    Ok((ms(start.elapsed()), first, world_updates))
}

/// Load over QUIC with the host and one watching player connected.
async fn live_load(setup: &Setup, workload: &Workload, created: usize) -> Result<Value> {
    let (session, spawns, _) = setup.session(workload.map)?;
    let server = server::start(session, options(spawns))?;
    let mut host = Client::connect_with_host(
        server.address,
        &server.certificate,
        "Host".into(),
        Vec::new(),
        None,
        Some(server.host_token.clone()),
    )
    .await?;
    let mut watcher = Client::connect(
        server.address,
        &server.certificate,
        "Watcher".into(),
        Vec::new(),
        None,
    )
    .await?;
    let base = host.replica.world.bricks.len();
    let request = codec::frame_request(
        &Request::new(
            1,
            Command::LoadBuild {
                build: Box::new(workload.build.clone()),
                ownership: false,
            },
            None,
        ),
        codec::MAX_REQUEST,
        codec::MAX_BULK_DECODED,
    )
    .map(|(bytes, _)| bytes);
    let request_bytes = match &request {
        Ok(bytes) => json!(bytes.len()),
        Err(error) => json!(format!("{error:#}")),
    };
    if request.is_err() {
        drop((host, watcher));
        let _ = server.stop().await;
        return Ok(
            json!({"request_bytes": request_bytes, "failed": "request exceeds the command limit"}),
        );
    }
    let (host_before, watch_before) = (host.link_probe().sample(), watcher.link_probe().sample());
    let start = Instant::now();
    let target = base + created;
    let watch = tokio::spawn(async move {
        let result = receive_until(&mut watcher, target, start, Duration::from_secs(600)).await;
        (watcher, result)
    });
    let sequence = host
        .request(Command::LoadBuild {
            build: Box::new(workload.build.clone()),
            ownership: false,
        })
        .await?;
    let sent_ms = ms(start.elapsed());
    let accepted = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            if let bri_net::client::ClientEvent::Reply {
                sequence: s,
                result,
            } = host.receive().await?
            {
                ensure!(s == sequence, "Unexpected reply");
                result.map_err(anyhow::Error::msg)?;
                return Result::<f64>::Ok(ms(start.elapsed()));
            }
        }
    })
    .await??;
    let (host_done, host_first, host_updates) =
        receive_until(&mut host, target, start, Duration::from_secs(600)).await?;
    let (watcher, watched) = watch.await?;
    let (watch_done, watch_first, watch_updates) = watched?;
    let (host_after, watch_after) = (host.link_probe().sample(), watcher.link_probe().sample());
    ensure!(
        host.replica.world == watcher.replica.world,
        "Host and watcher replicas differ"
    );
    // A player joining the finished world.
    let join_start = Instant::now();
    let join_cpu = cpu();
    let late = Client::connect(
        server.address,
        &server.certificate,
        "Late".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut late = late;
    let join_ms = ms(join_start.elapsed());
    let near_bricks = late.replica.world.bricks.len();
    // What the client builds before the player can move: the nearby world.
    let near_world = Arc::new(late.replica.world.clone());
    late.await_world().await?;
    let join_complete_ms = ms(join_start.elapsed());
    let join_cpu_ms = ms(cpu() - join_cpu);
    let join_sample = late.link_probe().sample();
    ensure!(
        late.replica.world.bricks == host.replica.world.bricks,
        "Late join differs from the host's replica"
    );
    let world = near_world;
    drop((host, watcher, late));
    let _ = server.stop().await;
    Ok(json!({
        "request_bytes": request_bytes,
        "request_sent_ms": sent_ms,
        "accepted_ms": accepted,
        "host_first_brick_ms": host_first,
        "host_complete_ms": host_done,
        "host_world_updates": host_updates,
        "watcher_first_brick_ms": watch_first,
        "watcher_complete_ms": watch_done,
        "watcher_world_updates": watch_updates,
        "host_sent_bytes": host_after.sent_bytes - host_before.sent_bytes,
        "host_received_bytes": host_after.received_bytes - host_before.received_bytes,
        "watcher_received_bytes": watch_after.received_bytes - watch_before.received_bytes,
        "join_ms": join_ms,
        "join_complete_ms": join_complete_ms,
        "join_near_bricks": near_bricks,
        "join_cpu_ms": join_cpu_ms,
        "join_received_bytes": join_sample.received_bytes,
        "join_client": client_stages(setup, workload.map, world)?,
    }))
}

/// What the client does with a joined world before it can play.
fn client_stages(
    setup: &Setup,
    map: &str,
    world: Arc<bri_net::protocol::PublicWorld>,
) -> Result<Value> {
    let (session, _, loaded) = setup.session(map)?;
    let definitions = session.simulation().definitions.clone();
    let waters = session.simulation().waters.clone();
    drop(session);
    let (mut building, _, _) = timed(|| {
        bri_client::building::Building::new(definitions.clone(), loaded.query_colliders.clone())
    })?;
    let (_, query_wall, query_cpu) = timed(|| building.sync_world(&world))?;
    let mut mirror = bri_sim::prediction::CollisionMirror::new(
        definitions,
        loaded.query_colliders.clone(),
        waters,
    );
    let (_, mirror_wall, mirror_cpu) = timed(|| mirror.sync(&world.bricks))?;
    let mut chunked = bri_client::world_chunks::ChunkedWorld::default();
    let (changes, mesh_wall, mesh_cpu) = timed(|| {
        chunked.update(
            world.clone(),
            None,
            &setup.meshes,
            &setup.palette,
            Some(&setup.materials),
            usize::MAX / 4,
        )
    })?;
    Ok(json!({
        "query_mirror_wall_ms": query_wall,
        "query_mirror_cpu_ms": query_cpu,
        "prediction_mirror_wall_ms": mirror_wall,
        "prediction_mirror_cpu_ms": mirror_cpu,
        "chunk_meshes_wall_ms": mesh_wall,
        "chunk_meshes_cpu_ms": mesh_cpu,
        "chunks": changes.len(),
    }))
}

/// A player joining a server that already holds `session`'s world.
async fn join_only(setup: &Setup, map: &str, session: Session, spawns: Vec<Vec3>) -> Result<Value> {
    let count = session.simulation().state().bricks.len();
    let server = server::start(session, options(spawns))?;
    let join_start = Instant::now();
    let join_cpu = cpu();
    let late = Client::connect(
        server.address,
        &server.certificate,
        "Late".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut late = late;
    let join_ms = ms(join_start.elapsed());
    let near_bricks = late.replica.world.bricks.len();
    // What the client builds before the player can move: the nearby world.
    let near_world = Arc::new(late.replica.world.clone());
    late.await_world().await?;
    let join_complete_ms = ms(join_start.elapsed());
    let join_cpu_ms = ms(cpu() - join_cpu);
    let sample = late.link_probe().sample();
    ensure!(
        late.replica.world.bricks.len() == count,
        "Join is missing bricks"
    );
    let world = near_world;
    drop(late);
    let _ = server.stop().await;
    Ok(json!({
        "join_ms": join_ms,
        "join_complete_ms": join_complete_ms,
        "join_near_bricks": near_bricks,
        "join_cpu_ms": join_cpu_ms,
        "join_received_bytes": sample.received_bytes,
        "join_client": client_stages(setup, map, world)?,
    }))
}

/// Rows of a v20 save: its bricks' definitions by frequency, reused to
/// build a large synthetic world with a realistic mix.
fn synthetic(setup: &Setup, like: &World, count: usize, spawns: &[Vec3]) -> Result<World> {
    // Real bricks from the save (their grid alignment and turn), each moved
    // into its own 4x4-stud, 3-plate cell so nothing overlaps.
    let mut table: Vec<&Brick> = Vec::new();
    for brick in like.bricks.values() {
        if let ContentRef::Resolved(id) = &brick.definition
            && let Some(mesh) = setup.meshes.get(id)
            && mesh.footprint_studs[0] <= 4
            && mesh.footprint_studs[1] <= 4
            && mesh.height_plates <= 3
            && brick.events.is_empty()
        {
            table.push(brick);
        }
    }
    ensure!(!table.is_empty(), "No small bricks to build with");
    let mut world = World::new("Synthetic".into(), SLATE.into(), like.palette.clone());
    let snap = |v: f32, step: f32| (v / step).round() * step;
    let origin = Vec3::new(
        snap(spawns[0].x + 40.0, 2.0),
        snap(spawns[0].y + 2.0, 0.2),
        snap(spawns[0].z - 40.0, 2.0),
    );
    let side = 100usize;
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for i in 0..count {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let (x, z, y) = (i % side, (i / side) % side, i / (side * side));
        let template = table[(seed >> 16) as usize % table.len()];
        let ContentRef::Resolved(id) = &template.definition else {
            unreachable!()
        };
        let height = setup.meshes[id].height_plates as f32 * 0.2;
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

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}
/// Median, min and max of every numeric field across runs.
fn summarize(runs: &[Value]) -> Value {
    fn walk(runs: &[&Value]) -> Value {
        match runs[0] {
            Value::Object(first) => {
                let mut out = serde_json::Map::new();
                for key in first.keys() {
                    let items: Vec<&Value> = runs.iter().filter_map(|r| r.get(key)).collect();
                    if items.len() == runs.len() {
                        out.insert(key.clone(), walk(&items));
                    }
                }
                Value::Object(out)
            }
            Value::Number(_) => {
                let mut v: Vec<f64> = runs.iter().filter_map(|r| r.as_f64()).collect();
                if v.len() != runs.len() {
                    return runs[0].clone();
                }
                let (lo, hi) = (
                    v.iter().copied().fold(f64::INFINITY, f64::min),
                    v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                );
                let m = median(&mut v);
                if lo == hi {
                    json!(m)
                } else {
                    json!({"median": m, "min": lo, "max": hi})
                }
            }
            other => other.clone(),
        }
    }
    walk(&runs.iter().collect::<Vec<_>>())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        (3..=5).contains(&args.len()),
        "Usage: brick_load_bench <content-root> <bench-dir> <report.json> [runs] [synthetic-bricks]"
    );
    let root = PathBuf::from(&args[0]);
    let bench = PathBuf::from(&args[1]);
    let report_path = PathBuf::from(&args[2]);
    let runs: usize = args.get(3).map_or(Ok(3), |s| s.parse())?;
    let synthetic_count: usize = args.get(4).map_or(Ok(1_000_000), |s| s.parse())?;
    let only = std::env::var("BRI_BENCH_ONLY").ok();

    let content = ClientContent::load(&root)?;
    let weapons = content.paths.weapon_content()?;
    let item_bounds = content.paths.item_physics(&weapons)?.bounds;
    let vehicles = content.paths.vehicle_pack()?;
    let materials = Arc::new(bri_client::materials::BrickMaterials::load(
        &content.paths.brick_materials,
    )?);
    let palette = Arc::new(bri_client::world_chunks::BrickPalette::new(&materials)?);
    let converter = bri_client::old_saves::Converter::new(&content)?;
    let mut setup = Setup {
        content,
        weapons,
        item_bounds,
        vehicles,
        meshes: Arc::new(BTreeMap::new()),
        materials,
        palette,
    };
    let (probe, _, _) = setup.session(SLATE)?;
    setup.meshes = Arc::new(
        probe
            .simulation()
            .definitions
            .entries
            .iter()
            .map(|(id, d)| (id.clone(), d.mesh.clone()))
            .collect(),
    );
    drop(probe);

    let mut workloads = Vec::new();
    let mut birthday = None;
    for (name, file, map) in [
        ("birthday", "birthday.bls", KITCHEN),
        ("christmas", "christmas.bls", SLATE),
    ] {
        let bytes = std::fs::read(bench.join(file))?;
        let mut samples = Vec::new();
        let mut world = None;
        for _ in 0..runs {
            let (w, wall, c) = timed(|| converter.convert(&bytes, name, map))?;
            let saved = bri_world::build::encode(&SavedBuild::new(w.clone()))?;
            let (_, decode_wall, decode_cpu) = timed(|| bri_world::build::decode(&saved))?;
            samples.push(json!({
                "bls_bytes": bytes.len(),
                "convert_wall_ms": wall,
                "convert_cpu_ms": c,
                "cached_bytes": saved.len(),
                "read_cached_wall_ms": decode_wall,
                "read_cached_cpu_ms": decode_cpu,
            }));
            world = Some(w);
        }
        let world = world.context("No runs")?;
        println!("{name}: {} bricks", world.bricks.len());
        if name == "birthday" {
            birthday = Some(world.clone());
        }
        workloads.push(Workload {
            name,
            map,
            build: SavedBuild::new(world),
            convert: Some(summarize(&samples)),
        });
    }
    let synthetic_world = synthetic(
        &setup,
        birthday.as_ref().context("birthday")?,
        synthetic_count,
        &setup.session(SLATE)?.1,
    )?;
    // Reading it back as Load Bricks reads a saved build from disk.
    let saved = bri_world::build::encode(&SavedBuild::new(synthetic_world.clone()))?;
    let (_, read_wall, read_cpu) = timed(|| bri_world::build::decode(&saved))?;
    let read = json!({
        "cached_bytes": saved.len(),
        "read_cached_wall_ms": read_wall,
        "read_cached_cpu_ms": read_cpu,
    });
    drop(saved);
    workloads.push(Workload {
        name: "synthetic",
        map: SLATE,
        build: SavedBuild::new(synthetic_world),
        convert: Some(read),
    });

    let mut report = serde_json::Map::new();
    for workload in &workloads {
        if only.as_deref().is_some_and(|o| o != workload.name) {
            continue;
        }
        let bricks = workload.build.world.bricks.len();
        println!("== {} ({bricks} bricks)", workload.name);
        if std::env::var_os("BRI_BENCH_PROFILE").is_some() {
            println!("  profile {}", placement_profile(&setup, workload)?);
            continue;
        }
        let mut offline = Vec::new();
        let mut live = Vec::new();
        let mut joins = Vec::new();
        for run in 0..runs {
            let (o, session, spawns) = offline_load(&setup, workload)?;
            let created = o["created"].as_u64().unwrap_or(0) as usize;
            println!("  run {run}: offline {o}");
            offline.push(o);
            let fits = codec::frame_request(
                &Request::new(
                    1,
                    Command::LoadBuild {
                        build: Box::new(workload.build.clone()),
                        ownership: false,
                    },
                    None,
                ),
                codec::MAX_REQUEST,
                codec::MAX_BULK_DECODED,
            );
            if workload.name == "synthetic"
                && let Err(error) = &fits
            {
                // Too large for a Load Bricks request: measure a join of the
                // world as a host would hold it.
                let j = join_only(&setup, workload.map, session, spawns).await?;
                println!("  run {run}: join {j}");
                joins.push(json!({"request": format!("{error:#}"), "join": j}));
            } else {
                drop(session);
                let l = live_load(&setup, workload, created).await?;
                println!("  run {run}: live {l}");
                live.push(l);
            }
            // Leave the machine a breather between heavy runs.
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let public = public_bricks(&workload.build.world.bricks).len();
        report.insert(
            workload.name.into(),
            json!({
                "bricks": bricks,
                "public_bricks": public,
                "convert": workload.convert,
                "offline": summarize(&offline),
                "live": if live.is_empty() { Value::Null } else { summarize(&live) },
                "join": if joins.is_empty() { Value::Null } else { summarize(&joins) },
                "runs": runs,
            }),
        );
    }
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &report_path,
        serde_json::to_vec_pretty(&Value::Object(report))?,
    )?;
    println!("Report: {}", report_path.display());
    Ok(())
}
