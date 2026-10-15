//! The v0.3.0 all-features soak: bots, a portal pair (two linked bricks of
//! one name), vehicles on spawn bricks and a scripted Add-On all running in
//! one mini-game while two real QUIC clients play: the host's own client
//! and a friend who joins with an empty Add-On cache and downloads what the
//! host runs. Mirrors the crossover soak (`crossover_soak.rs`).
//!
//! It passes only if nothing panicked or failed on the host (no failed
//! step, no host failure, no Add-On or script problem), both clients stayed
//! connected and in step with the host (movement acknowledged, every
//! command answered, the host's bricks the same on each client, every bot
//! and vehicle seen), and the host kept its 120 ticks a second.
//!
//! Two runs:
//! - `all_features_soak_short`: 20 s on shipped Add-Ons only (the
//!   Playground, the Portal and Mirror bricks, the Rule Workshop's scripted
//!   switch) in the made-up room; content-free, so CI runs it too.
//! - `soak_all_features_with_classic_add_ons` (ignored; the gate runs it
//!   with `--include-ignored`): three minutes. With generated content
//!   (`BRI_CONTENT`) holding the ported classic Add-Ons ([`CLASSIC`]) in
//!   its `addons/`, it hosts on that content, on Slate, with them on;
//!   without it, it says so and soaks the shipped set instead.
//!
//! `BRI_ALL_FEATURES_SECONDS` sets how long either plays. Numbers go to
//! stderr as `[all-features]` lines.
mod gate;

use anyhow::{Context, Result, ensure};
use bri_net::{
    client::{Client, ClientEvent},
    dedicated::Dedicated,
    packages::PackageShelf,
    protocol::MOVEMENT_REDUNDANCY,
    server::{self, ServerOptions},
};
use bri_package::{
    environment::PackageRef,
    packages::{PackageEntry, PackageSet, Side},
    sync::Cache,
};
use bri_progress::Progress;
use bri_sim::{
    player::MoveInput,
    session::{Command, MiniGameRequest},
};
use bri_world::{Brick, ContentRef, OwnerId, OwnerRecord, VehicleSpawn, World};
use gate::*;
use glam::Vec3;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

/// Ported classic Add-Ons the long soak turns on when the content has them:
/// a vehicle, two bot kinds, a thrown weapon and a tool, beside the shipped
/// Portal bricks, Blockhead bots and the Playground's buggy and rifle.
const CLASSIC: [&str; 5] = [
    "vehicle_stunt_plane",
    "bot_hole",
    "bot_zombie",
    "weapon_hegrenade",
    "tool_duplicator",
];
const SHIPPED_ON_CONTENT: [&str; 4] = [
    "brick_portal",
    "blockhead_bot",
    "playground-buggy",
    "playground-rifle",
];
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";
const OWNER: OwnerId = 1;
const PORTAL_NAME: &str = "soak";
const BOT_SPAWNS: usize = 10;
const MOST_VEHICLES: usize = 4;

/// Where the host runs: its content root (kept alive while it runs), the
/// package list, and what a joining player already has.
struct Host {
    _scratch: Option<Root>,
    root: PathBuf,
    set: PackageSet,
    label: String,
}
impl Host {
    fn load(&self, world: World) -> Result<Dedicated> {
        bri_net::dedicated::load_packages(&self.root, &self.set, world)
    }
    /// The base game's packages the host runs: what a fresh player has.
    fn base_packages(&self, host: &Dedicated) -> Vec<PackageRef> {
        let base: Vec<&str> = self
            .set
            .packages
            .iter()
            .filter(|p| p.role.is_some())
            .map(|p| p.id.as_str())
            .collect();
        host.environment
            .packages
            .iter()
            .filter(|p| base.contains(&p.id.as_str()))
            .cloned()
            .collect()
    }
}

/// The made-up room with the shipped Add-Ons: the Playground, the Portal
/// and Mirror bricks and the Rule Workshop's scripted switch.
fn shipped() -> Result<Host> {
    let mut add_ons = playground();
    for (id, dir) in [
        ("brick_portal", "brick_portal"),
        ("brick_mirror", "brick_mirror"),
        ("rule-workshop-toys", "rule-workshop-toys"),
    ] {
        add_ons.push(repository_add_on(id, dir));
    }
    let root = Root::new("all-features", &add_ons)?;
    Ok(Host {
        root: root.path().to_path_buf(),
        set: root.set.clone(),
        _scratch: Some(root),
        label: "shipped Add-Ons, made-up room".into(),
    })
}

/// The generated content at `BRI_CONTENT` with the classic Add-Ons on, or
/// why not.
fn classic() -> std::result::Result<Result<Host>, String> {
    let Some(root) = std::env::var_os("BRI_CONTENT").map(PathBuf::from) else {
        return Err("BRI_CONTENT is not set".into());
    };
    let missing: Vec<&str> = CLASSIC
        .iter()
        .chain(&SHIPPED_ON_CONTENT)
        .copied()
        .filter(|id| !root.join("addons").join(id).join("package.json").is_file())
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "{} has no {} under addons/",
            root.display(),
            missing.join(", ")
        ));
    }
    Ok((|| {
        let mut set = PackageSet::load_root(&root)?;
        for id in CLASSIC.iter().chain(&SHIPPED_ON_CONTENT) {
            if set.packages.iter().any(|p| p.id == *id) {
                continue;
            }
            let dir = root.join("addons").join(id);
            let info = bri_package::library::package_info(&dir)
                .with_context(|| format!("{} has no readable package.json", dir.display()))?;
            set.packages.push(PackageEntry {
                id: id.to_string(),
                version: info.version.clone(),
                side: info.side().unwrap_or(Side::Shared),
                dir: format!("addons/{id}"),
                role: None,
            });
        }
        bri_package::library::follow_manifest_sides(&root, &mut set);
        bri_package::library::follow_companions(&root, &mut set);
        Ok(Host {
            label: format!("classic Add-Ons {CLASSIC:?} on {}", root.display()),
            root,
            set,
            _scratch: None,
        })
    })())
}

/// A brick's logical size (x, y, z) from its definition's mesh.
fn size(host: &Dedicated, id: &str) -> Option<Vec3> {
    let mesh = &host.session.simulation().definitions.entries.get(id)?.mesh;
    Some(Vec3::new(
        mesh.footprint_studs[0] as f32 * 0.5,
        mesh.height_plates as f32 * 0.2,
        mesh.footprint_studs[1] as f32 * 0.5,
    ))
}

/// `id` placed with its bottom on the floor at `at`, on the brick grid.
fn placed(host: &Dedicated, id: &str, at: Vec3) -> Option<Brick> {
    let size = size(host, id)?;
    let cells = [0.5, 0.2, 0.5];
    let bottom = Vec3::new(at.x, at.y + size.y * 0.5, at.z);
    let snapped = Vec3::from_array(std::array::from_fn(|axis| {
        ((bottom[axis] - size[axis] * 0.5) / cells[axis]).round() * cells[axis] + size[axis] * 0.5
    }));
    Some(Brick::new(
        ContentRef::Resolved(id.into()),
        snapped.into(),
        OWNER,
    ))
}

/// The world: a ring of bot and vehicle spawn bricks around the spawn,
/// a portal pair across it, and a low wall to fight around.
struct Layout {
    world: World,
    bot_kinds: Vec<String>,
    vehicle_kinds: Vec<String>,
    portal: String,
}
fn layout(probe: &Dedicated, map: &str) -> Result<Layout> {
    let s = &probe.session;
    let definitions = &s.simulation().definitions.entries;
    let origin = probe.spawn_points.first().copied().unwrap_or(Vec3::ZERO);
    let floor = Vec3::new(origin.x, origin.y - 0.05, origin.z);
    let plate = [
        bri_sim::testing::VEHICLE_SPAWN,
        "v20/brick/brickvehiclespawndata",
    ]
    .into_iter()
    .find(|id| definitions.contains_key(*id))
    .context("no vehicle spawn brick")?;
    let portal = definitions
        .iter()
        .filter(|(id, d)| id.starts_with("brick_portal:") && d.link.is_some())
        .min_by_key(|(_, d)| d.mesh.footprint_studs[0] * d.mesh.height_plates)
        .map(|(id, _)| id.clone())
        .context("no Portal brick (brick_portal) among the bricks")?;
    let bot_kinds: Vec<String> = s.bot_choices().into_iter().map(|(id, _)| id).collect();
    ensure!(!bot_kinds.is_empty(), "no bot kind is enabled");
    let mut vehicle_kinds: Vec<String> = s
        .vehicle_choices()
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| !bot_kinds.contains(id))
        .collect();
    // The Add-Ons' vehicles first, then whatever else the content has.
    vehicle_kinds.sort_by_key(|id| !(id.contains("stunt") || id.contains("buggy")));
    vehicle_kinds.truncate(MOST_VEHICLES);
    ensure!(!vehicle_kinds.is_empty(), "no vehicle is enabled");

    let mut world = World::new(
        "All features".into(),
        map.into(),
        vec![[0.6, 0.6, 0.6, 1.0]; 16],
    );
    world
        .owners
        .insert(OWNER, OwnerRecord::new([7; 32], "Soak builder".into()));
    let mut bricks = Vec::new();
    let spawns = BOT_SPAWNS + vehicle_kinds.len();
    for i in 0..spawns {
        let angle = i as f32 / spawns as f32 * std::f32::consts::TAU;
        let at = floor + Vec3::new(26.0 * angle.cos(), 0.0, 26.0 * angle.sin());
        let mut brick = placed(probe, plate, at).context("no spawn plate size")?;
        let kind = if i % 4 == 3 && i / 4 < vehicle_kinds.len() {
            vehicle_kinds[i / 4].clone()
        } else {
            bot_kinds[i % bot_kinds.len()].clone()
        };
        brick.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(kind),
            recolor: false,
            team: None,
        }));
        bricks.push(brick);
    }
    // Every vehicle kind on a spawn brick: those the ring did not get one.
    let placed_vehicles: BTreeSet<String> = bricks
        .iter()
        .filter_map(|b| match &b.vehicle.as_ref()?.vehicle {
            ContentRef::Resolved(id) if vehicle_kinds.contains(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    for (i, kind) in vehicle_kinds
        .iter()
        .filter(|k| !placed_vehicles.contains(*k))
        .enumerate()
    {
        let at = floor + Vec3::new(-12.0 + 8.0 * i as f32, 0.0, 14.0);
        let mut brick = placed(probe, plate, at).context("no spawn plate size")?;
        brick.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(kind.clone()),
            recolor: false,
            team: None,
        }));
        bricks.push(brick);
    }
    // The portal pair, facing each other across the ring.
    for x in [-10.0, 10.0] {
        let mut brick =
            placed(probe, &portal, floor + Vec3::new(x, 0.0, -8.0)).context("no portal size")?;
        brick.name = Some(PORTAL_NAME.into());
        bricks.push(brick);
    }
    // A low wall to fight around, of whichever plain brick there is.
    if let Some(wall) = ["fixture/brick/2x2", "v20/brick/brick2x2data"]
        .into_iter()
        .find(|id| definitions.contains_key(*id))
    {
        let height = size(probe, wall).map_or(1.2, |s| s.y);
        for i in 0..40 {
            let at = floor + Vec3::new(-10.0 + (i % 20) as f32, height * (i / 20) as f32, 6.0);
            if let Some(mut brick) = placed(probe, wall, at) {
                brick.color = (i % 16) as u8;
                bricks.push(brick);
            }
        }
    }
    for (i, brick) in bricks.into_iter().enumerate() {
        world.bricks.insert(i as u64 + 1, brick);
        world.next_brick_id = i as u64 + 2;
    }
    Ok(Layout {
        world,
        bot_kinds,
        vehicle_kinds,
        portal,
    })
}

/// One connected player: moves every tick with redundancy, and counts the
/// replies to what it asks.
struct Player {
    name: String,
    client: Client,
    sequence: u64,
    recent: VecDeque<MoveInput>,
    pending: BTreeMap<u64, Instant>,
    answered: u64,
    worst_reply: Duration,
    downloaded: Vec<(String, u64)>,
}
impl Player {
    fn step(&mut self, tick: u64, index: usize) -> Result<()> {
        let phase = tick as f32 / 240.0 + index as f32 * 1.7;
        let input = MoveInput {
            forward: 1.0,
            right: (phase * 0.7).sin() * 0.5,
            yaw: phase.sin() * std::f32::consts::PI,
            jump: tick % 360 < 6,
            ..Default::default()
        };
        self.sequence += 1;
        self.recent.push_back(input);
        if self.recent.len() > MOVEMENT_REDUNDANCY {
            self.recent.pop_front();
        }
        let recent: Vec<_> = self.recent.iter().copied().collect();
        self.client.movement(self.sequence, &recent, None, None)
    }
    async fn ask(&mut self, command: Command) -> Result<()> {
        let sequence = self.client.request(command).await?;
        self.pending.insert(sequence, Instant::now());
        Ok(())
    }
    /// Everything that has arrived; a lost connection is an error here.
    async fn drain(&mut self) -> Result<()> {
        while let Ok(event) = tokio::time::timeout(Duration::ZERO, self.client.receive()).await {
            if let ClientEvent::Reply { sequence, .. } = event?
                && let Some(sent) = self.pending.remove(&sequence)
            {
                self.worst_reply = self.worst_reply.max(sent.elapsed());
                self.answered += 1;
            }
        }
        Ok(())
    }
    fn acknowledged(&self) -> u64 {
        self.client
            .replica
            .poses
            .get(&self.client.owner)
            .map_or(0, |p| p.acknowledged_input)
    }
}

fn soak(host: Host, map: &str, seconds: u64) -> Result<()> {
    eprintln!("[all-features] {}, {seconds} s", host.label);
    // Loaded once to learn what the content offers, then again with the
    // world built from it.
    let mut probe = gate::world("probe");
    probe.map_id = map.into();
    let probe = host.load(probe)?;
    let Layout {
        world,
        bot_kinds,
        vehicle_kinds,
        portal,
    } = layout(&probe, map)?;
    drop(probe);
    let mut dedicated = host.load(world)?;
    // The two portals pair up: after a step their openings lead somewhere.
    dedicated.session.step()?;
    let passages = dedicated.session.simulation().passages();
    ensure!(
        !passages.list.is_empty(),
        "the portal pair is not linked ({} shut openings)",
        passages.closed.len()
    );
    let problems: Vec<String> = dedicated
        .session
        .package_diagnostics()
        .iter()
        .map(ToString::to_string)
        .collect();
    ensure!(problems.is_empty(), "Add-On problems on load: {problems:?}");
    let base = host.base_packages(&dedicated);
    let all = dedicated.environment.packages.clone();
    let shelf = PackageShelf::new(&host.root, &host.set, &dedicated.environment)?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;
    let server = {
        let _in = rt.enter();
        server::start(
            dedicated.session,
            ServerOptions {
                bind: "127.0.0.1:0".parse()?,
                environment: dedicated.environment.clone(),
                spawn_points: dedicated.spawn_points.clone(),
                certificate: None,
                map_loader: Some(dedicated.setup.clone()),
                packages: Some(std::sync::Arc::new(shelf)),
            },
        )?
    };
    let identity_dir = tempfile::tempdir()?;
    let caches = tempfile::tempdir()?;
    let mut players = Vec::new();
    for (index, name) in ["Host", "Friend"].into_iter().enumerate() {
        let identity = bri_identity::ClientIdentity::load_or_create(
            identity_dir.path().join(format!("{index}.identity")),
        )?;
        let cache = Cache::open(&caches.path().join(index.to_string()))?;
        let (have, token) = if index == 0 {
            (all.clone(), Some(server.host_token.clone()))
        } else {
            (base.clone(), None)
        };
        let (client, downloaded) = rt.block_on(fetching_join(
            server.address,
            &server.certificate,
            name,
            have,
            token,
            &identity,
            &cache,
            Progress::default(),
        ))?;
        let mut player = Player {
            name: name.into(),
            client,
            sequence: 0,
            recent: VecDeque::new(),
            pending: BTreeMap::new(),
            answered: 0,
            worst_reply: Duration::ZERO,
            downloaded,
        };
        rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(120), async {
                while !player
                    .client
                    .replica
                    .poses
                    .contains_key(&player.client.owner)
                {
                    player.client.receive().await?;
                }
                Result::<()>::Ok(())
            })
            .await
            .context("no pose after joining")?
        })?;
        players.push(player);
    }
    // One mini-game for everyone, with the Playground's rifle (and the
    // classic grenade and duplicator when they are on).
    let mut loadout: [Option<String>; 5] = Default::default();
    let tools = host.set.packages.iter().any(|p| p.id == "weapon_hegrenade");
    loadout[0] = Some("playground-rifle:weapon/carbine".into());
    if tools {
        loadout[1] = Some("weapon_hegrenade:weapon/hegrenadeitem".into());
    }
    let settings = bri_minigames::Settings {
        title: "All features".into(),
        enable_building: true,
        loadout,
        ..Default::default()
    };
    rt.block_on(players[0].ask(Command::MiniGame(MiniGameRequest::Create {
        color: 0,
        settings,
    })))?;
    let game = rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                players[0].drain().await?;
                if let Some(game) = players[0].client.replica.minigames.first() {
                    return Result::<u64>::Ok(game.id);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .context("no mini-game")?
    })?;
    rt.block_on(players[1].ask(Command::MiniGame(MiniGameRequest::Join { game })))?;

    // Play.
    let start = Instant::now();
    let mut tick = 0u64;
    let mut perf = Vec::new();
    let mut next_perf_tick = 120;
    let mut most_vehicles = 0;
    rt.block_on(async {
        while start.elapsed() < Duration::from_secs(seconds) {
            let due = (start.elapsed().as_secs_f64() * 120.0) as u64;
            while tick < due {
                tick += 1;
                for (index, player) in players.iter_mut().enumerate() {
                    player.step(tick, index)?;
                    if (tick + index as u64 * 60).is_multiple_of(600) {
                        let line = format!("{} {tick}", player.name);
                        player.ask(Command::Chat(line)).await?;
                    }
                }
            }
            for player in &mut players {
                player.drain().await?;
            }
            most_vehicles = most_vehicles.max(players[1].client.replica.vehicles.len());
            if tick >= next_perf_tick {
                perf.push(server.perf.lock().unwrap().clone());
                next_perf_tick = (tick / 120 + 1) * 120;
            }
            tokio::time::sleep(Duration::from_millis(4)).await;
        }
        // The last replies, and the last bricks.
        let settle = Instant::now();
        while settle.elapsed() < Duration::from_secs(3)
            || (settle.elapsed() < Duration::from_secs(15)
                && players.iter().any(|p| !p.pending.is_empty()))
        {
            for player in &mut players {
                player.drain().await?;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Result::<()>::Ok(())
    })?;
    let problems: Vec<String> = server
        .package_problems
        .lock()
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect();
    let connected = server.players.load(std::sync::atomic::Ordering::Relaxed);
    // Each client's view at the end: (name, bricks, players and bots seen,
    // moves the host has yet to acknowledge, unanswered commands, tick).
    type Seen = (String, BTreeMap<u64, Brick>, usize, u64, u64, u64);
    let clients: Vec<Seen> = players
        .iter()
        .map(|p| {
            let replica = &p.client.replica;
            (
                p.name.clone(),
                replica
                    .world
                    .bricks
                    .iter()
                    .map(|(id, b)| (*id, b.clone()))
                    .collect(),
                replica.names.len(),
                p.sequence.saturating_sub(p.acknowledged()),
                p.pending.len() as u64,
                replica.tick,
            )
        })
        .collect();
    for player in &players {
        eprintln!(
            "[all-features] {}: downloaded {} packs; {} commands answered, worst reply {:?}; \
             sees {} players and bots, {} vehicles",
            player.name,
            player.downloaded.len(),
            player.answered,
            player.worst_reply,
            player.client.replica.names.len(),
            player.client.replica.vehicles.len(),
        );
    }
    drop(players);
    let report = rt.block_on(server.stop())?;

    let windows: Vec<_> = perf
        .iter()
        .skip(5)
        .filter(|p| p.ticks_per_second > 0.0)
        .collect();
    let mean = |f: &dyn Fn(&bri_net::server::ServerPerf) -> f32| {
        windows.iter().map(|p| f(p)).sum::<f32>() / windows.len().max(1) as f32
    };
    let tick_mean = mean(&|p| p.tick_ms_mean);
    let tick_worst = windows.iter().map(|p| p.tick_ms_max).fold(0.0, f32::max);
    let rate = mean(&|p| p.ticks_per_second);
    let host_bricks = &report.final_world.bricks;
    eprintln!(
        "[all-features] host: {} ticks, {} step errors, tick mean {tick_mean:.2} ms, worst \
         {tick_worst:.2} ms, {rate:.1} ticks/s; {} bricks; bots {bot_kinds:?}; vehicles \
         {vehicle_kinds:?} (most seen at once {most_vehicles}); portal {portal}",
        report.ticks,
        report.step_errors,
        host_bricks.len(),
    );

    // The bars.
    ensure!(
        report.failure.is_none(),
        "host failed: {:?}",
        report.failure
    );
    ensure!(
        report.step_errors == 0,
        "{} host steps failed",
        report.step_errors
    );
    ensure!(
        problems.is_empty(),
        "Add-On or script problems: {problems:?}"
    );
    let diagnostics: Vec<String> = report
        .package_diagnostics
        .iter()
        .map(ToString::to_string)
        .collect();
    ensure!(
        diagnostics.is_empty(),
        "Add-On diagnostics: {diagnostics:?}"
    );
    ensure!(connected == 2, "{connected} players still connected, not 2");
    ensure!(
        rate >= 110.0,
        "the host kept {rate:.1} ticks a second, not 120"
    );
    ensure!(
        most_vehicles >= vehicle_kinds.len(),
        "the friend saw at most {most_vehicles} vehicles of {}",
        vehicle_kinds.len()
    );
    for (name, bricks, seen, behind, pending, replica_tick) in clients {
        ensure!(pending == 0, "{name}: {pending} commands never answered");
        ensure!(
            behind < 120,
            "{name}: the host acknowledged moves {behind} behind"
        );
        ensure!(
            seen >= 2 + BOT_SPAWNS,
            "{name} sees {seen} players and bots, wanted at least {}",
            2 + BOT_SPAWNS
        );
        ensure!(
            replica_tick + 240 >= report.ticks,
            "{name} is at tick {replica_tick}, the host ended at {}",
            report.ticks
        );
        // Every brick the client holds is the host's, where the host has it.
        let differ: Vec<u64> = bricks
            .iter()
            .filter(|(id, b)| {
                host_bricks.get(*id).is_some_and(|h| {
                    h.definition != b.definition
                        || Vec3::from(h.position).distance(Vec3::from(b.position)) > 1e-3
                })
            })
            .map(|(id, _)| *id)
            .collect();
        ensure!(
            differ.is_empty(),
            "{name}: bricks differ from the host's: {differ:?}"
        );
        // Bricks knocked out in the last moments may not have reached the
        // client before the host stopped; more than a few is a desync.
        let host_ids: BTreeSet<u64> = host_bricks.keys().copied().collect();
        let ids: BTreeSet<u64> = bricks.keys().copied().collect();
        let apart = host_ids.symmetric_difference(&ids).count();
        ensure!(
            apart <= 2 + host_ids.len() / 50,
            "{name}: {apart} bricks are on only one of client and host ({} on the host)",
            host_ids.len()
        );
    }
    drop(host);
    Ok(())
}

#[test]
fn all_features_soak_short() -> Result<()> {
    soak(shipped()?, MAP, env("BRI_ALL_FEATURES_SECONDS", 20))
}

#[test]
#[ignore = "three minutes; the gate runs it with --include-ignored"]
fn soak_all_features_with_classic_add_ons() -> Result<()> {
    let seconds = env("BRI_ALL_FEATURES_SECONDS", 180);
    match classic() {
        Ok(host) => soak(host?, SLATE, seconds),
        Err(why) => {
            eprintln!("skipped classic Add-Ons: {why}; soaking the shipped Add-Ons instead");
            soak(shipped()?, MAP, seconds)
        }
    }
}
