//! The v0.3.0 crossover soak: four players and sixteen mixed bots
//! (Blockhead, Runner, Commando, Trooper and Stomper bodies) on a large
//! build, every Playground pack loaded, the Playground carbine leaving
//! impact decals on the build, all through a link with 150 ms round trips.
//!
//! The host is built as a real one is (`dedicated::load_packages`); the four
//! players are real QUIC clients through the impaired link. The host joins
//! with its own Add-Ons; the three friends join with empty download caches.
//! Each client predicts its own body as the game does
//! (`bri_sim::prediction::Predictor`) and counts the corrections the host's
//! poses make. It reports the host's tick time, each client's per-tick
//! work (the headless stand-in for frame time: receiving, replicating and
//! predicting), prediction corrections and join times.
//!
//! - `BRI_CROSSOVER_SECONDS`: how long it plays (default 30; the release
//!   gate's full run is 1800).
//! - `BRI_CROSSOVER_BRICKS`: the build's size (default 20000).
//!
//! Numbers go to stderr as `[crossover]` lines.
mod gate;

use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_net::{
    client::{Client, ClientEvent},
    impair::{ImpairedLink, Impairment},
    packages::PackageShelf,
    protocol::MOVEMENT_REDUNDANCY,
    server::{self, ServerOptions},
};
use bri_package::{packages::Side, sync::Cache};
use bri_progress::Progress;
use bri_sim::{
    player::MoveInput,
    prediction::{CollisionMirror, Predictor},
    presentation::CueKind,
    session::{ActionAim, Command, MiniGameRequest, Reply},
};
use bri_world::{Brick, ContentRef, OwnerId, VehicleSpawn};
use gate::*;
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

const PLAYERS: usize = 4;
const CARBINE: &str = "playground-rifle:weapon/carbine";
/// The bot kinds the sixteen spawn bricks hold, in turn.
const KINDS: [&str; 5] = [
    "bot.blockhead",
    "playground-stomper:bot/stomper",
    "crossover-bots:bot/runner",
    "crossover-bots:bot/trooper",
    "crossover-bots:bot/commando",
];
const SPAWN_BRICKS: usize = 16;
/// A correction the eye can see: the predicted body put back this far.
const VISIBLE: f32 = 0.25;

/// Bot kinds wearing the Runner's, the Trooper's and the Commando's bodies:
/// an Add-On made at test time, its `bots.json` beside its manifest.
fn crossover_bots(dir: &Path) -> Result<AddOn> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "id": "crossover-bots",
            "version": "1.0.0",
            "api": 1,
            "name": "Crossover Bots",
            "description": "Made by the crossover soak: bots in other Add-Ons' bodies.",
            "authors": ["Blockland ReImagined"],
            "license": "CC0-1.0",
            "provenance": { "source": "original" },
            "provides": [
                { "kind": "bots", "id": "crossover-bots:bots/main", "file": "bots.json" }
            ]
        }))?,
    )?;
    let kind = |name: &str, body: &str| {
        serde_json::json!({
            "id": format!("crossover-bots:bot/{}", name.to_lowercase()),
            "name": name,
            "body": body,
        })
    };
    std::fs::write(
        dir.join("bots.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1,
            "bots": [
                kind("Runner", "playground-runner:archetype/runner"),
                kind("Trooper", "sample-trooper:archetype/trooper"),
                kind("Commando", "sample-commando:archetype/commando"),
            ]
        }))?,
    )?;
    Ok(AddOn::new(
        "crossover-bots",
        dir.to_path_buf(),
        Side::Shared,
    ))
}

/// The open room with a large build at its far end (walls of 2x2 bricks a
/// metre apart, layer on layer) and a ring of sixteen bot spawn bricks.
fn crossover_world(bricks: u64) -> bri_world::World {
    let mut world = world("Crossover");
    let mut id = 0;
    let (per_row, rows) = (120u64, 13u64);
    for i in 0..bricks {
        let (x, row, layer) = (i % per_row, i / per_row % rows, i / (per_row * rows));
        id += 1;
        let mut brick = Brick::new(
            ContentRef::Resolved("fixture/brick/2x2".into()),
            [
                -60.0 + x as f32,
                0.3 + 0.6 * layer as f32,
                -66.0 + 2.0 * row as f32,
            ],
            1 + i % 8,
        );
        brick.color = (i % 16) as u8;
        world.bricks.insert(id, brick);
    }
    for i in 0..SPAWN_BRICKS {
        let angle = i as f32 / SPAWN_BRICKS as f32 * std::f32::consts::TAU;
        id += 1;
        let mut brick = Brick::new(
            ContentRef::Resolved(bri_sim::testing::VEHICLE_SPAWN.into()),
            [
                (24.0 * angle.cos()).round(),
                0.1,
                (24.0 * angle.sin()).round(),
            ],
            1,
        );
        brick.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(KINDS[i % KINDS.len()].into()),
            recolor: false,
            team: None,
        }));
        world.bricks.insert(id, brick);
    }
    world.next_brick_id = id + 1;
    world
}

/// The room as the prediction collides with it: its floor and walls.
fn room() -> Vec<ColliderBuilder> {
    let (half, height) = (HALF, 40.0);
    vec![
        ColliderBuilder::cuboid(half, 0.5, half).translation(Vector::new(0.0, -0.5, 0.0)),
        ColliderBuilder::cuboid(0.5, height, half).translation(Vector::new(
            half + 0.5,
            height,
            0.0,
        )),
        ColliderBuilder::cuboid(0.5, height, half).translation(Vector::new(
            -half - 0.5,
            height,
            0.0,
        )),
        ColliderBuilder::cuboid(half, height, 0.5).translation(Vector::new(
            0.0,
            height,
            half + 0.5,
        )),
        ColliderBuilder::cuboid(half, height, 0.5).translation(Vector::new(
            0.0,
            height,
            -half - 0.5,
        )),
    ]
}

/// One of the four players: a real client predicting its own body.
struct Player {
    name: String,
    client: Client,
    predictor: Option<Predictor>,
    definitions: bri_sim::definitions::Definitions,
    recent: VecDeque<MoveInput>,
    sequence: u64,
    spawn_tick: u64,
    pending: BTreeMap<u64, Instant>,
    answered: u64,
    worst_reply: Duration,
    poses: u64,
    corrections: u64,
    visible: u64,
    worst_correction: f32,
    decals: u64,
    shots: u64,
    work: Duration,
    worst_work: Duration,
    ticks: u64,
    joined_in: Duration,
    downloaded: Vec<(String, u64)>,
    carbine: Option<usize>,
}
impl Player {
    fn own_pose(&self) -> Option<&bri_net::protocol::Pose> {
        self.client.replica.poses.get(&self.client.owner)
    }
    /// One 120 Hz tick of input: predicted, then sent with redundancy.
    fn step(&mut self, tick: u64, index: usize) -> Result<()> {
        let Some(predictor) = &mut self.predictor else {
            return Ok(());
        };
        // Round and round near the middle, each player its own way, a hop
        // now and then; facing the build while it shoots.
        let phase = tick as f32 / 240.0 + index as f32 * 1.7;
        let input = MoveInput {
            forward: 1.0,
            right: (phase * 0.7).sin() * 0.5,
            yaw: if (tick / 120).is_multiple_of(3) {
                0.0
            } else {
                phase.sin() * std::f32::consts::PI
            },
            pitch: 0.0,
            jump: tick % 360 < 6,
            ..Default::default()
        };
        let (sequence, _) = predictor.step(input)?;
        self.sequence = sequence;
        self.recent.push_back(input);
        if self.recent.len() > MOVEMENT_REDUNDANCY {
            self.recent.pop_front();
        }
        let recent: Vec<_> = self.recent.iter().copied().collect();
        self.client.movement(sequence, &recent, None, None)
    }
    /// Ask the host for `command`; its reply is counted when it comes.
    async fn ask(&mut self, command: Command, aim: Option<ActionAim>) -> Result<()> {
        let sequence = self.client.request_with_aim(command, aim).await?;
        self.pending.insert(sequence, Instant::now());
        Ok(())
    }
    /// Everything that has arrived: replies, poses (reconciled), cues.
    async fn drain(&mut self) -> Result<()> {
        let start = Instant::now();
        while let Ok(event) = tokio::time::timeout(Duration::ZERO, self.client.receive()).await {
            if let ClientEvent::Reply { sequence, result } = event? {
                if let Some(sent) = self.pending.remove(&sequence) {
                    self.worst_reply = self.worst_reply.max(sent.elapsed());
                    self.answered += 1;
                }
                // A refused shot (no carbine in hand yet, dead) is play,
                // not a failure.
                let _ = result;
            }
        }
        for cue in self.client.replica.take_cues() {
            if matches!(cue.kind, CueKind::Decal { .. }) {
                self.decals += 1;
            }
        }
        self.reconcile()?;
        let took = start.elapsed();
        self.work += took;
        self.worst_work = self.worst_work.max(took);
        Ok(())
    }
    fn reconcile(&mut self) -> Result<()> {
        let Some(pose) = self.own_pose().cloned() else {
            return Ok(());
        };
        let Some(predictor) = &mut self.predictor else {
            // The first pose: predict from it, colliding with the build.
            let mirror = CollisionMirror::new(self.definitions.clone(), room(), Vec::new());
            let mut predictor = Predictor::new(
                mirror,
                pose.player.clone(),
                (*self.client.replica.archetypes).clone(),
            )?;
            predictor.sync_world(&self.client.replica.world.bricks)?;
            predictor.continue_after(pose.acknowledged_input);
            predictor.teleport(pose.tick, pose.acknowledged_input, pose.player.clone())?;
            self.sequence = pose.acknowledged_input;
            self.spawn_tick = pose.spawn_tick;
            self.predictor = Some(predictor);
            return Ok(());
        };
        if pose.spawn_tick != self.spawn_tick || pose.passage_vehicle.is_some() {
            // A new body (a respawn): taken as it is, not a correction.
            self.spawn_tick = pose.spawn_tick;
            if pose.acknowledged_input <= predictor.sequence() {
                predictor.teleport(pose.tick, pose.acknowledged_input, pose.player.clone())?;
            }
            return Ok(());
        }
        if pose.acknowledged_input > predictor.sequence() {
            return Ok(());
        }
        if let Some(offset) = predictor.reconcile(
            pose.tick,
            pose.acknowledged_input,
            pose.player.clone(),
            pose.passage_frame,
        )? {
            self.poses += 1;
            let length = offset.length();
            if length > 0.001 {
                self.corrections += 1;
            }
            if length > VISIBLE {
                self.visible += 1;
            }
            self.worst_correction = self.worst_correction.max(length);
        }
        Ok(())
    }
}

#[test]
fn soak_crossover_four_players_and_sixteen_mixed_bots() -> Result<()> {
    let seconds = env("BRI_CROSSOVER_SECONDS", 30);
    let bricks = env("BRI_CROSSOVER_BRICKS", 20_000);
    let made = bri_content::testing::ScratchDir::new("crossover-bots")?;
    let mut add_ons = playground();
    for (id, dir) in [
        ("sample-commando", "samples/sample-commando"),
        ("sample-commando-rifle", "samples/sample-commando-rifle"),
        ("sample-commando-look", "samples/sample-commando-look"),
    ] {
        add_ons.push(repository_add_on(id, dir));
    }
    add_ons.push(crossover_bots(&made.path().join("crossover-bots"))?);
    let root = Root::new("crossover", &add_ons)?;
    drop(made);
    let host = root.host(crossover_world(bricks))?;
    let definitions = host.session.simulation().definitions.clone();
    let base = root.base_packages(&host);
    let all = host.environment.packages.clone();
    let shelf = PackageShelf::new(root.path(), &root.set, &host.environment)?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;
    let server = {
        let _in = rt.enter();
        server::start(
            host.session,
            bri_net::server::ServerOptions {
                bind: "127.0.0.1:0".parse()?,
                environment: host.environment.clone(),
                spawn_points: host.spawn_points.clone(),
                certificate: None,
                map_loader: Some(host.setup.clone()),
                packages: Some(Arc::new(shelf)),
            } as ServerOptions,
        )?
    };
    // 150 ms there and back.
    let impairment = Impairment {
        latency: Duration::from_millis(70),
        jitter: Duration::from_millis(10),
        ..Impairment::NONE
    };
    let link = rt.block_on(ImpairedLink::start(server.address, impairment, 0xc055))?;
    let identity_dir = tempfile::tempdir()?;
    let caches = tempfile::tempdir()?;
    let mut players = Vec::new();
    for index in 0..PLAYERS {
        let name = if index == 0 {
            "Host".to_string()
        } else {
            format!("Friend {index}")
        };
        let identity = bri_identity::ClientIdentity::load_or_create(
            identity_dir.path().join(format!("{index}.identity")),
        )?;
        let cache = Cache::open(&caches.path().join(index.to_string()))?;
        let (have, token) = if index == 0 {
            (all.clone(), Some(server.host_token.clone()))
        } else {
            (base.clone(), None)
        };
        let start = Instant::now();
        let (client, downloaded) = rt.block_on(fetching_join(
            link.address,
            &server.certificate,
            &name,
            have,
            token,
            &identity,
            &cache,
            Progress::default(),
        ))?;
        players.push(Player {
            name,
            client,
            predictor: None,
            definitions: definitions.clone(),
            recent: VecDeque::new(),
            sequence: 0,
            spawn_tick: 0,
            pending: BTreeMap::new(),
            answered: 0,
            worst_reply: Duration::ZERO,
            poses: 0,
            corrections: 0,
            visible: 0,
            worst_correction: 0.0,
            decals: 0,
            shots: 0,
            work: Duration::ZERO,
            worst_work: Duration::ZERO,
            ticks: 0,
            joined_in: start.elapsed(),
            downloaded,
            carbine: None,
        });
        // In the world before the next joins.
        let player = players.last_mut().unwrap();
        rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(120), async {
                while player.own_pose().is_none() {
                    player.client.receive().await?;
                }
                Result::<()>::Ok(())
            })
            .await
            .context("no pose after joining")?
        })?;
        player.joined_in = start.elapsed();
    }
    // The host starts a game with the carbine; everyone joins it.
    let settings = bri_minigames::Settings {
        title: "Crossover".into(),
        enable_building: true,
        loadout: [
            Some(bri_weapons::HAMMER.into()),
            Some(bri_weapons::WRENCH.into()),
            Some(bri_weapons::PRINTER.into()),
            Some(CARBINE.into()),
            None,
        ],
        ..Default::default()
    };
    rt.block_on(players[0].ask(
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
        None,
    ))?;
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
    for player in players.iter_mut().skip(1) {
        rt.block_on(player.ask(Command::MiniGame(MiniGameRequest::Join { game }), None))?;
    }

    // Play.
    let start = Instant::now();
    let mut tick = 0u64;
    let mut perf = Vec::new();
    let mut next_perf_tick = 120;
    let mut bots_seen = BTreeSet::new();
    rt.block_on(async {
        while start.elapsed() < Duration::from_secs(seconds) {
            let due = (start.elapsed().as_secs_f64() * 120.0) as u64;
            while tick < due {
                tick += 1;
                for (index, player) in players.iter_mut().enumerate() {
                    let began = Instant::now();
                    player.step(tick, index)?;
                    player.work += began.elapsed();
                    player.ticks += 1;
                    // The carbine out, then a shot at the build every two
                    // seconds (each player at its own moment).
                    if (tick + index as u64 * 60).is_multiple_of(240) {
                        let owner = player.client.owner;
                        if player.carbine.is_none() {
                            player.carbine =
                                player.client.replica.tools.get(&owner).and_then(|t| {
                                    t.slots.iter().position(|s| s.as_deref() == Some(CARBINE))
                                });
                            if let Some(slot) = player.carbine {
                                player
                                    .ask(Command::EquipTool { slot: Some(slot) }, None)
                                    .await?;
                            }
                        } else if let Some(pose) = player.own_pose() {
                            let eye = Vec3::from(pose.player.feet) + Vec3::Y * 2.2;
                            let at = Vec3::new(eye.x.clamped(-55.0, 55.0), 1.5, -42.0) - eye;
                            let aim = ActionAim {
                                yaw: at.x.atan2(-at.z),
                                pitch: at.y.atan2(Vec3::new(at.x, 0.0, at.z).length()),
                            };
                            player
                                .ask(Command::WeaponTrigger { down: true }, Some(aim))
                                .await?;
                            player
                                .ask(Command::WeaponTrigger { down: false }, Some(aim))
                                .await?;
                            player.shots += 1;
                        }
                    }
                    // A chat line a minute, so every command is answered.
                    if (tick + index as u64 * 30).is_multiple_of(7200) {
                        player
                            .ask(Command::Chat(format!("{} {tick}", player.name)), None)
                            .await?;
                    }
                }
            }
            for player in &mut players {
                player.drain().await?;
            }
            let replica = &players[0].client.replica;
            let humans: BTreeSet<OwnerId> = players.iter().map(|p| p.client.owner).collect();
            for owner in replica.names.keys().filter(|o| !humans.contains(o)) {
                bots_seen.insert(*owner);
            }
            // A catch-up batch can cross the second without ending exactly
            // on its tick. Sample the crossed boundary, once per batch.
            if tick >= next_perf_tick {
                perf.push(server.perf.lock().unwrap().clone());
                next_perf_tick = (tick / 120 + 1) * 120;
            }
            tokio::time::sleep(Duration::from_millis(4)).await;
        }
        // The last replies.
        let settle = Instant::now();
        while settle.elapsed() < Duration::from_secs(10)
            && players.iter().any(|p| !p.pending.is_empty())
        {
            for player in &mut players {
                player.drain().await?;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Result::<()>::Ok(())
    })?;

    // What the host and every player saw.
    let replica = &players[0].client.replica;
    let humans: BTreeSet<OwnerId> = players.iter().map(|p| p.client.owner).collect();
    let bots: Vec<OwnerId> = replica
        .names
        .keys()
        .copied()
        .filter(|o| !humans.contains(o))
        .collect();
    let mut bodies: BTreeMap<String, usize> = BTreeMap::new();
    for bot in &bots {
        let body = replica
            .poses
            .get(bot)
            .map(|p| replica.archetypes.resolve(p.player.archetype).id.clone())
            .unwrap_or_else(|| "(no pose)".into());
        *bodies.entry(body).or_default() += 1;
    }
    let windows: Vec<_> = perf
        .iter()
        .skip(5)
        .filter(|p| p.ticks_per_second > 0.0)
        .collect();
    ensure!(
        !windows.is_empty(),
        "no host performance windows were sampled"
    );
    let mean = |f: &dyn Fn(&bri_net::server::ServerPerf) -> f32| {
        windows.iter().map(|p| f(p)).sum::<f32>() / windows.len().max(1) as f32
    };
    let tick_mean = mean(&|p| p.tick_ms_mean);
    let tick_worst = windows.iter().map(|p| p.tick_ms_max).fold(0.0, f32::max);
    let rate = mean(&|p| p.ticks_per_second);
    let slowest = windows
        .iter()
        .map(|p| p.ticks_per_second)
        .fold(f32::MAX, f32::min);
    eprintln!(
        "[crossover] {seconds} s, {} bricks, {} players, {} bots ({} seen in all), link 150 ms round trip",
        replica.world.bricks.len(),
        PLAYERS,
        bots.len(),
        bots_seen.len()
    );
    eprintln!("[crossover] bot bodies: {bodies:?}");
    eprintln!(
        "[crossover] host tick: mean {tick_mean:.2} ms, worst {tick_worst:.2} ms, \
         {rate:.1} ticks/s (slowest second {slowest:.1})"
    );
    for player in &players {
        let per_tick = player.work.as_secs_f64() * 1000.0 / player.ticks.max(1) as f64;
        let behind = player
            .sequence
            .saturating_sub(player.own_pose().map_or(0, |p| p.acknowledged_input));
        eprintln!(
            "[crossover] {}: joined in {:.2} s (downloaded {} packs, {:.1} MB); client work \
             {per_tick:.3} ms a tick, worst drain {:.1} ms; {} poses, {} corrected \
             ({:.2}%), {} visible (> {VISIBLE}), worst {:.3}; {} shots, {} decals seen; \
             worst reply {:?}; moves {} ahead of the host",
            player.name,
            player.joined_in.as_secs_f64(),
            player.downloaded.len(),
            player.downloaded.iter().map(|(_, b)| *b).sum::<u64>() as f64 / 1e6,
            player.worst_work.as_secs_f64() * 1000.0,
            player.poses,
            player.corrections,
            100.0 * player.corrections as f64 / player.poses.max(1) as f64,
            player.visible,
            player.worst_correction,
            player.shots,
            player.decals,
            player.worst_reply,
            behind,
        );
    }
    eprintln!(
        "[crossover] link: {} datagrams forwarded",
        link.stats.forwarded.load(Ordering::Relaxed)
    );

    // The bars.
    ensure!(
        bots.len() >= SPAWN_BRICKS,
        "{} bots in the game, wanted {SPAWN_BRICKS}",
        bots.len()
    );
    for body in [
        "playground-stomper:archetype/stomper",
        "playground-runner:archetype/runner",
        "sample-trooper:archetype/trooper",
        "sample-commando:archetype/commando",
    ] {
        ensure!(bodies.contains_key(body), "no bot wears {body}: {bodies:?}");
    }
    ensure!(
        rate >= 110.0,
        "the host kept {rate:.1} ticks a second, not 120"
    );
    for player in &players {
        ensure!(
            player.pending.is_empty(),
            "{}: {} commands never answered",
            player.name,
            player.pending.len()
        );
        ensure!(player.poses > 0, "{}: no poses reconciled", player.name);
        let behind = player
            .sequence
            .saturating_sub(player.own_pose().map_or(0, |p| p.acknowledged_input));
        ensure!(
            behind < 120,
            "{}: the host acknowledged moves {behind} behind",
            player.name
        );
        ensure!(
            player.client.replica.names.len() >= PLAYERS + SPAWN_BRICKS,
            "{} sees {} players and bots",
            player.name,
            player.client.replica.names.len()
        );
    }
    ensure!(
        players.iter().map(|p| p.decals).sum::<u64>() > 0,
        "no impact decal reached any player"
    );
    for player in players.iter().skip(1) {
        ensure!(
            !player.downloaded.is_empty(),
            "{} downloaded nothing into an empty cache",
            player.name
        );
    }
    let _ = Reply::Accepted;
    drop(players);
    rt.block_on(server.stop())?;
    drop(link);
    Ok(())
}
