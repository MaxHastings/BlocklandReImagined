//! What the host sends in typical scenes, over real QUIC on loopback, and
//! byte budgets that fail when a change makes the network heavier. The
//! numbers and their breakdown are in `docs/audits/network-bandwidth.md`.
//!
//! Print the table: cargo test -p bri-net --test bandwidth -- --nocapture
mod common;
use anyhow::Result;
use bri_net::{
    client::Client,
    server::{self, ServerHandle},
    traffic::{Kind, TrafficSample},
};
use bri_sim::{player::MoveInput, session::Command};
use bri_world::{Brick, ContentRef, World};
use std::time::Duration;

/// The client's prediction rate and a 60 fps frame: two inputs a frame.
const FRAME: Duration = Duration::from_micros(16_667);
const INPUTS_PER_FRAME: u64 = 2;
const ROCKET: &str = "bandwidth:weapon/rocketitem";
const ROCKET_IMAGE: &str = "bandwidth:image/rocketimage";
const ROCKET_PROJECTILE: &str = "bandwidth:projectile/rocket";

/// A rocket launcher without shapes or effects: holding the trigger fires
/// one rocket every half second that knocks out bricks within 3 units.
fn rocket_pack() -> bri_weapons::Pack {
    let state = |name: &str, ticks, script: &str| bri_weapons::State {
        name: name.into(),
        ticks,
        wait: true,
        allow_change: true,
        script: script.into(),
        ..Default::default()
    };
    let states = vec![
        bri_weapons::State {
            timeout: Some(1),
            ..state("Activate", 0, "")
        },
        bri_weapons::State {
            down: Some(2),
            ..state("Ready", 0, "")
        },
        bri_weapons::State {
            timeout: Some(3),
            ..state("Fire", 60, "onFire")
        },
        bri_weapons::State {
            timeout: Some(1),
            ..state("Reload", 0, "")
        },
    ];
    let image = bri_weapons::Image {
        id: ROCKET_IMAGE.into(),
        name: "rocketLauncherImage".into(),
        model: String::new(),
        projectile: Some(ROCKET_PROJECTILE.into()),
        mount_point: 0,
        offset: [0.; 3],
        eye_offset: [0.; 3],
        source_rotation_degrees: [0.; 3],
        correct_muzzle: false,
        melee: false,
        color: [1.; 4],
        color_shift: false,
        arm_ready: true,
        casing: String::new(),
        min_shot_ticks: 0,
        command: Default::default(),
        commands: Default::default(),
        shot: None,
        eye_rotation: [0.0; 3],
        zoom: None,
        bot: None,
        crosshair: true,
        follow_arm: false,
        hide_nodes: Vec::new(),
        both_arms: false,
        paint_tint: false,
        left_image: None,
        magazine: None,
        volleys: vec![],
        last_shot: None,
        state_shots: Default::default(),
        cook: None,
        guard: None,
        rope: None,
        light: None,
        paint_picker: false,
        scripts: Default::default(),
        states,
    };
    let item = bri_weapons::Item {
        id: ROCKET.into(),
        name: "rocketLauncherItem".into(),
        ui_name: "Rocket L.".into(),
        image: ROCKET_IMAGE.into(),
        model: String::new(),
        icon: String::new(),
        can_drop: true,
        sport: false,
        hidden: false,
        label: String::new(),
        rotate: false,
        ..Default::default()
    };
    let projectile = bri_weapons::ProjectileDef {
        id: ROCKET_PROJECTILE.into(),
        name: "rocketLauncherProjectile".into(),
        model: String::new(),
        speed: 40.,
        inherit: 0.,
        gravity: 0.,
        lifetime_ticks: 480,
        fade_ticks: 0,
        arm_ticks: 0,
        ballistic: false,
        elasticity: 0.,
        friction: 0.,
        damage: 0.,
        damage_type: String::new(),
        radius_damage_type: String::new(),
        impulse: 0.,
        vertical: 0.,
        explode_player: true,
        explode_death: true,
        collide_players: true,
        explosion: bri_weapons::Explosion {
            effect: String::new(),
            damage: 0.,
            radius: 3.,
            impulse: 0.,
            impulse_radius: 0.,
            impulse_vertical: 0.,
            burn_seconds: 0.,
        },
        brick: bri_weapons::BrickImpact {
            radius: 3.,
            direct: true,
            force: 20.,
            max_volume: 1000.,
            max_floating_volume: 1000.,
        },
        bounce_effect: String::new(),
        stick_effect: String::new(),
        blood_effect: String::new(),
        bounce_angle: 0.,
        min_stick_speed: 0.,
        trail: String::new(),
        sound: String::new(),
        light_radius: 0.,
        light_color: [0.; 3],
        sport_image: None,
        rest_speed: 0.,
        max_bounces: 0,
        children: Vec::new(),
        aura: None,
        slow: None,
        fixed_damage: false,
    };
    let pack = bri_weapons::Pack {
        effects: Default::default(),
        schema_version: bri_weapons::SCHEMA,
        id: "bandwidth.rockets".into(),
        items: [(ROCKET.to_string(), item)].into(),
        images: [(ROCKET_IMAGE.to_string(), image)].into(),
        projectiles: [(ROCKET_PROJECTILE.to_string(), projectile)].into(),
        external_projectiles: Default::default(),
        damage_types: Default::default(),
        explosions: Default::default(),
        sounds: Default::default(),
        definitions: vec![],
        resources: vec![],
        diagnostics: vec![],
        bindings: vec![],
    };
    pack.validate().unwrap();
    pack
}

/// A LAN host (anyone's bricks can be blown up, as v20) with rocket launchers
/// in every spawn loadout, over `world`.
fn host(world: World) -> Result<ServerHandle> {
    host_with(world, common::options())
}
fn host_with(world: World, options: server::ServerOptions) -> Result<ServerHandle> {
    let mut session = common::session_with(world);
    session.set_lan_host(true);
    session.set_weapon_pack(rocket_pack())?;
    session.set_spawn_loadout(bri_sim::session::ToolInventory {
        slots: [Some(ROCKET.to_string()), None, None, None, None].into(),
        selected: None,
    })?;
    server::start(session, options)
}

fn empty_world() -> World {
    World::new(
        "Bandwidth".into(),
        "fixture".into(),
        vec![[1.0; 4], [0.0; 4]],
    )
}

/// A wall of plates 16 wide and 40 high, 6 units in front of the first spawn
/// point (which faces -Z), for rockets to knock bricks out of.
fn wall_world() -> World {
    let mut world = empty_world();
    let mut id = 1;
    for column in 0..16 {
        for row in 0..40 {
            let position = [-55.5 + column as f32, 0.1 + 0.2 * row as f32, -6.25];
            world.bricks.insert(
                id,
                Brick::new(ContentRef::Resolved("plate".into()), position, 1),
            );
            id += 1;
        }
    }
    world.next_brick_id = id;
    world
}

/// A flat build of `count` plates, for the join download.
fn big_world(count: u64) -> World {
    let mut world = empty_world();
    for id in 1..=count {
        let (x, z) = ((id % 180) as f32, (id / 180 % 360) as f32 * 0.5);
        let layer = (id / (180 * 360)) as f32;
        let mut brick = Brick::new(
            ContentRef::Resolved("plate".into()),
            [-90.0 + x, 0.1 + 0.2 * layer, 5.25 + z],
            1 + id % 8,
        );
        brick.color = (id % 2) as u8;
        world.bricks.insert(id, brick);
    }
    world.next_brick_id = count + 1;
    world
}

async fn join(server: &ServerHandle, players: usize) -> Result<Vec<Client>> {
    let mut clients = Vec::new();
    for i in 0..players {
        clients.push(
            Client::connect(
                server.address,
                &server.certificate,
                format!("Player{i}"),
                Vec::new(),
                None,
            )
            .await?,
        );
    }
    Ok(clients)
}

/// What each player does every frame.
#[derive(Clone, Copy, PartialEq)]
enum Act {
    /// Stands still, still sending input like a real client.
    Idle,
    /// Runs in a circle.
    Walk,
    /// Runs in a circle and plants five bricks a second.
    Build,
    /// Holds the trigger of a rocket launcher at `yaw`, `pitch` for the
    /// measured window.
    Shoot { yaw: f32, pitch: f32 },
}

/// Bytes per second one scene cost.
struct Report {
    name: &'static str,
    players: usize,
    seconds: f64,
    /// What the host sent, by kind, summed over every player.
    host: TrafficSample,
    /// QUIC bytes each player received and sent (packet overhead included).
    received: Vec<u64>,
    sent: Vec<u64>,
}
impl Report {
    fn rate(&self, bytes: u64) -> f64 {
        bytes as f64 / self.seconds
    }
    /// Host payload per second across every player.
    fn host_rate(&self) -> f64 {
        self.rate(self.host.total())
    }
    /// Mean QUIC bytes per second one player received.
    fn download(&self) -> f64 {
        self.rate(self.received.iter().sum::<u64>()) / self.players as f64
    }
    fn upload(&self) -> f64 {
        self.rate(self.sent.iter().sum::<u64>()) / self.players as f64
    }
    fn print(&self) {
        eprintln!(
            "{:<28} {:>2} players: host sends {:>9.0} B/s payload; each player receives {:>8.0} B/s, sends {:>7.0} B/s on the wire",
            self.name,
            self.players,
            self.host_rate(),
            self.download(),
            self.upload()
        );
        for kind in Kind::ALL {
            let bytes = self.host.bytes(kind);
            if bytes > 0 {
                eprintln!(
                    "{:>34} {:>9.0} B/s in {:>6.0} msg/s ({:.0} B each)",
                    kind.name(),
                    self.rate(bytes),
                    self.rate(self.host.messages(kind)),
                    bytes as f64 / self.host.messages(kind) as f64
                );
            }
        }
    }
}

/// Run every player's frames for `warmup` then `window`, measuring the window.
async fn drive(
    name: &'static str,
    server: &ServerHandle,
    clients: Vec<Client>,
    acts: impl Fn(usize) -> Act,
    warmup: Duration,
    window: Duration,
) -> Result<(Report, Vec<Client>)> {
    let players = clients.len();
    let mut tasks = Vec::new();
    for (index, mut client) in clients.into_iter().enumerate() {
        let act = acts(index);
        tasks.push(tokio::spawn(async move {
            let probe = client.link_probe();
            let mut frame = tokio::time::interval(FRAME);
            frame.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let started = tokio::time::Instant::now();
            let (measure_at, end) = (started + warmup, started + warmup + window);
            let mut first = None;
            let mut sequence = 0_u64;
            let mut history = std::collections::VecDeque::new();
            let mut frames = 0_u64;
            let mut shooting = false;
            if matches!(act, Act::Shoot { .. }) {
                client.request(Command::EquipTool { slot: Some(0) }).await?;
            }
            loop {
                tokio::select! {
                    _ = frame.tick() => {
                        let now = tokio::time::Instant::now();
                        if now >= end {
                            break;
                        }
                        if first.is_none() && now >= measure_at {
                            first = Some(probe.sample());
                        }
                        frames += 1;
                        let t = frames as f32 / 60.0;
                        let input = match act {
                            Act::Idle => MoveInput::default(),
                            Act::Walk | Act::Build => MoveInput {
                                forward: 1.0,
                                yaw: (t * 0.8 + index as f32).sin() * 3.0,
                                ..Default::default()
                            },
                            // Move the aim between shots so each rocket finds bricks.
                            Act::Shoot { yaw, pitch } => {
                                let shot = (t * 2.0) as usize;
                                MoveInput {
                                    yaw: yaw + [-0.7, 0.7, 0.0][shot % 3],
                                    pitch: pitch + [-0.15, 0.45][shot / 3 % 2],
                                    ..Default::default()
                                }
                            }
                        };
                        for _ in 0..INPUTS_PER_FRAME {
                            sequence += 1;
                            history.push_back(input);
                            if history.len() > bri_net::protocol::MOVEMENT_REDUNDANCY {
                                history.pop_front();
                            }
                        }
                        let inputs: Vec<_> = history.iter().copied().collect();
                        client.movement(sequence, &inputs, None, None)?;
                        // The ghost brick follows the builder's aim, reported at
                        // the client's 10 Hz.
                        if act == Act::Build && frames.is_multiple_of(6) {
                            let k = frames / 6;
                            client.request(Command::GhostBrick(Some(bri_sim::session::GhostBrick {
                                definition: "plate".into(),
                                position: [-60.0 + 15.0 * index as f32 + (k % 20) as f32 * 0.5, 0.1, 20.25],
                                quarter_turns: (k % 4) as u8,
                                color: 0,
                                print: None,
                            }))).await?;
                        }
                        if act == Act::Build && frames.is_multiple_of(12) {
                            let k = frames / 12;
                            let position = [
                                -60.0 + 15.0 * index as f32 + (k % 10) as f32,
                                0.1,
                                10.25 + (k / 10) as f32 * 0.5,
                            ];
                            client.request(Command::Plant {
                                definition: "plate".into(),
                                position,
                                quarter_turns: 0,
                                color: (k % 2) as u8,
                            }).await?;
                        }
                        let firing = matches!(act, Act::Shoot { .. }) && first.is_some()
                            && now < measure_at + window.mul_f32(0.6);
                        if firing != shooting {
                            shooting = firing;
                            client.request(Command::WeaponTrigger { down: firing }).await?;
                        }
                    }
                    event = client.receive() => { event?; }
                }
            }
            let last = probe.sample();
            let first = first.unwrap_or(last);
            anyhow::Ok((
                client,
                last.received_bytes - first.received_bytes,
                last.sent_bytes - first.sent_bytes,
            ))
        }));
    }
    tokio::time::sleep(warmup).await;
    let before = server.traffic.sample();
    tokio::time::sleep(window).await;
    let host = server.traffic.sample().since(&before);
    let mut report = Report {
        name,
        players,
        seconds: window.as_secs_f64(),
        host,
        received: Vec::new(),
        sent: Vec::new(),
    };
    let mut clients = Vec::new();
    for task in tasks {
        let (client, received, sent) = task.await??;
        report.received.push(received);
        report.sent.push(sent);
        clients.push(client);
    }
    report.print();
    Ok((report, clients))
}

const WARMUP: Duration = Duration::from_secs(1);
const WINDOW: Duration = Duration::from_secs(4);

async fn scene(
    name: &'static str,
    world: World,
    players: usize,
    acts: impl Fn(usize) -> Act,
) -> Result<(Report, Vec<Client>, ServerHandle)> {
    let server = host(world)?;
    let clients = join(&server, players).await?;
    let (report, clients) = drive(name, &server, clients, acts, WARMUP, WINDOW).await?;
    Ok((report, clients, server))
}

/// `players` running in circles, each spawned `spacing` units from the
/// next on a grid eight wide: 64 apart is players spread over a big map,
/// 3 apart a crowd at spawn.
async fn crowd(
    name: &'static str,
    players: usize,
    spacing: f32,
) -> Result<(Report, Vec<Client>, ServerHandle)> {
    let mut options = common::options();
    options.spawn_points = (0..players)
        .map(|i| {
            glam::Vec3::new(
                (i % 8) as f32 * spacing - 3.5 * spacing,
                0.05,
                (i / 8) as f32 * spacing,
            )
        })
        .collect();
    let server = host_with(empty_world(), options)?;
    let clients = join(&server, players).await?;
    let (report, clients) =
        drive(name, &server, clients, |_| Act::Walk, WARMUP, WINDOW).await?;
    Ok((report, clients, server))
}

async fn finish(clients: Vec<Client>, server: ServerHandle) -> Result<()> {
    for client in &clients {
        client.close();
    }
    server.stop().await?;
    Ok(())
}

/// 32 players running, spread over a map and crowded at spawn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "prints the crowd rows of the audit table; run with --ignored --nocapture"]
async fn crowd_bandwidth_table() -> Result<()> {
    for (name, spacing) in [("32 running, spread out", 64.0), ("32 running, crowded", 3.0)] {
        let (_, clients, server) = crowd(name, 32, spacing).await?;
        finish(clients, server).await?;
    }
    Ok(())
}

/// Every scene, printed as the audit's table. Not a budget: see the tests
/// below for those.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "prints the audit table; run with --ignored --nocapture"]
async fn bandwidth_table() -> Result<()> {
    for players in [1, 8] {
        let (_, clients, server) =
            scene("idle freebuild", empty_world(), players, |_| Act::Idle).await?;
        finish(clients, server).await?;
    }
    let (_, clients, server) = scene("8 players running", empty_world(), 8, |_| Act::Walk).await?;
    finish(clients, server).await?;
    let (_, clients, server) =
        scene("8 players building", empty_world(), 8, |_| Act::Build).await?;
    eprintln!(
        "  bricks planted: {}",
        clients[0].replica.world.bricks.len()
    );
    finish(clients, server).await?;
    let (_, clients, server) = explosion().await?;
    eprintln!("  bricks knocked out: {}", knocked_out(&clients[0]));
    finish(clients, server).await?;
    let (_, clients, server) = rocket_fight().await?;
    finish(clients, server).await?;
    // Vehicles need the converted vehicle pack; their datagram is priced
    // here instead. A parked vehicle settles like a still player.
    let jeep = bri_net::protocol::Datagram::Vehicle(bri_sim::session::VehiclePose {
        id: 1000,
        tick: 1_000_000,
        position: [12.5, 1.25, -40.0],
        rotation: [0.01, 0.7, 0.01, 0.7],
        velocity: [8.0, 0.1, -3.0],
        steering: 0.2,
        wheel_suspension: vec![0.1; 4],
        wheel_rotation: vec![1.5; 4],
        wheel_contact: vec![true; 4],
        wheel_tire: vec![Default::default(); 4],
        turret_aim: [0.0; 2],
        jetting: false,
        angular_velocity: [0.1, 0.4, 0.0],
        mouse_steering: [0.3, 0.0],
        driver_input: 123_456,
        driver_steering: (false, false),
        steering_quiet: 0,
        actor: None,
    });
    let size = bri_net::codec::encode_datagram_item(&jeep)?.len();
    eprintln!(
        "a driving four-wheel vehicle: {size} B per pose, {} B/s to each player at 40 Hz",
        size * 40
    );
    for bricks in [10_000, 100_000] {
        let server = host(big_world(bricks))?;
        let before = server.traffic.sample();
        let clients = join(&server, 1).await?;
        let sent = server.traffic.sample().since(&before);
        eprintln!(
            "join download of {bricks} bricks: {} bytes ({:.1} B per brick)",
            sent.bytes(Kind::World),
            sent.bytes(Kind::World) as f64 / bricks as f64
        );
        finish(clients, server).await?;
    }
    Ok(())
}

/// Eight players each hold down a rocket launcher, aimed apart.
async fn rocket_fight() -> Result<(Report, Vec<Client>, ServerHandle)> {
    scene("8 players firing rockets", empty_world(), 8, |i| {
        Act::Shoot {
            yaw: -1.2 + 0.3 * i as f32,
            pitch: 0.2,
        }
    })
    .await
}

/// One player fires rockets into a wall of 640 plates while seven watch.
async fn explosion() -> Result<(Report, Vec<Client>, ServerHandle)> {
    // Aim from the first spawn point's eye at the middle of the wall.
    let (eye, target) = (
        glam::Vec3::new(-48.0, 2.3, 0.0),
        glam::Vec3::new(-48.0, 2.5, -6.0),
    );
    let d = target - eye;
    let aim = Act::Shoot {
        yaw: d.x.atan2(-d.z),
        pitch: d.y.atan2(glam::Vec2::new(d.x, d.z).length()),
    };
    scene("explosion, 7 watching", wall_world(), 8, move |i| {
        if i == 0 { aim } else { Act::Idle }
    })
    .await
}

fn knocked_out(client: &Client) -> usize {
    client
        .replica
        .world
        .bricks
        .values()
        .filter(|b| !b.visible)
        .count()
}

// Budgets: about twice what each scene measured when they were set (see the
// audit), and far below what it cost before. A failure means a change made
// the network heavier; measure with the table above and either fix it or
// raise the budget in the audit with the reason.

/// Eight players standing still, as a real client sends input.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_idle_server_stays_under_its_byte_budget() -> Result<()> {
    let (report, clients, server) =
        scene("idle freebuild", empty_world(), 8, |_| Act::Idle).await?;
    finish(clients, server).await?;
    // 653 KB/s before the audit; 14 KB/s after.
    assert!(
        report.host_rate() < 30_000.0,
        "idle host sends {:.0} B/s",
        report.host_rate()
    );
    // Still players' poses go out for a few intervals, then once a second.
    let poses = report.rate(report.host.messages(Kind::Pose));
    assert!(poses < 300.0, "idle host sends {poses:.0} poses/s");
    Ok(())
}

/// 32 players running spread over a map: poses go out by distance.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_spread_out_crowd_stays_under_its_byte_budget() -> Result<()> {
    let (report, clients, server) = crowd("32 running, spread out", 32, 64.0).await?;
    finish(clients, server).await?;
    // 2.1 MB/s when every player got every other at 40 Hz; 0.41 MB/s after.
    assert!(
        report.host_rate() < 800_000.0,
        "spread out crowd host sends {:.0} B/s",
        report.host_rate()
    );
    Ok(())
}

/// 32 players running around one spawn: the closest keep the full rate.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_crowd_at_spawn_stays_under_its_byte_budget() -> Result<()> {
    let (report, clients, server) = crowd("32 running, crowded", 32, 3.0).await?;
    finish(clients, server).await?;
    // 2.0 MB/s before ranking by distance and compact poses; 0.99 MB/s after.
    assert!(
        report.host_rate() < 1_400_000.0,
        "crowd host sends {:.0} B/s",
        report.host_rate()
    );
    Ok(())
}

/// Rockets knocking hundreds of bricks out of a wall while seven watch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_explosion_stays_under_its_byte_budget() -> Result<()> {
    let (report, clients, server) = explosion().await?;
    let knocked = knocked_out(&clients[0]);
    finish(clients, server).await?;
    assert!(knocked >= 64, "the scene knocked out only {knocked} bricks");
    // 701 KB/s before the audit; 31 KB/s after.
    assert!(
        report.host_rate() < 70_000.0,
        "explosion host sends {:.0} B/s",
        report.host_rate()
    );
    Ok(())
}

/// Projectiles in flight cost nothing: clients coast them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_rocket_fight_stays_under_its_byte_budget() -> Result<()> {
    let (report, clients, server) = rocket_fight().await?;
    finish(clients, server).await?;
    let updates = report.rate(report.host.bytes(Kind::Update));
    // 221 KB/s of world updates before the audit; 13 KB/s after.
    assert!(
        updates < 30_000.0,
        "rocket fight world updates {updates:.0} B/s"
    );
    Ok(())
}

/// A shot's sound reaches the shooter by the time its projectile does, with
/// updates skipped while nothing changes (the late-join test in `loopback.rs`
/// checks the same with v20's gun).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shot_sound_arrives_with_its_projectile() -> Result<()> {
    let mut pack = rocket_pack();
    for state in &mut pack
        .images
        .get_mut(ROCKET_IMAGE)
        .expect("rocket image")
        .states
    {
        if state.name == "Fire" {
            state.sound = "gunShot1Sound".into();
        }
    }
    let mut session = common::session_with(empty_world());
    session.set_lan_host(true);
    session.set_weapon_pack(pack)?;
    session.set_spawn_loadout(bri_sim::session::ToolInventory {
        slots: [Some(ROCKET.to_string()), None, None, None, None].into(),
        selected: None,
    })?;
    let server = server::start(session, common::options())?;
    for round in 0..10 {
        let mut client = Client::connect(
            server.address,
            &server.certificate,
            format!("Shooter {round}"),
            Vec::new(),
            None,
        )
        .await?;
        let shooter = client.owner;
        client.command(Command::EquipTool { slot: Some(0) }).await?;
        wait_for(&mut client, |c| {
            c.replica
                .weapons
                .images
                .get(&shooter)
                .is_some_and(|images| images.iter().any(|i| i.state == "Ready"))
        })
        .await?;
        let first = client.replica.poses[&shooter].acknowledged_input + 1;
        client.movement(first, &[MoveInput::default()], None, None)?;
        client
            .command(Command::WeaponTrigger { down: true })
            .await?;
        client
            .command(Command::WeaponTrigger { down: false })
            .await?;
        wait_for(&mut client, |c| {
            c.replica
                .weapons
                .projectiles
                .iter()
                .any(|p| p.source.0 == shooter)
        })
        .await?;
        let cues = client.replica.take_cues();
        assert!(
            cues.iter().any(|c| matches!(&c.kind,
                bri_sim::presentation::CueKind::WeaponSound { profile } if profile == "gunShot1Sound")),
            "round {round}: {:?}",
            cues.iter().map(|c| &c.kind).collect::<Vec<_>>()
        );
        client.close();
    }
    server.stop().await?;
    Ok(())
}

async fn wait_for(client: &mut Client, predicate: impl Fn(&Client) -> bool) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !predicate(client) {
            client.receive().await?;
        }
        Result::<()>::Ok(())
    })
    .await?
}

/// v20's spawn projectile lives a tick or two, so it is gone before the next
/// update after a join: the joiner's checkpoint holds it and no update ever
/// did. The next update must remove it, or the joiner flies it on forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_joiner_drops_a_projectile_that_ended_before_the_next_update() -> Result<()> {
    let mut pack = rocket_pack();
    let mut spawn = pack.projectiles[ROCKET_PROJECTILE].clone();
    spawn.id = bri_sim::session::SPAWN_PROJECTILE.into();
    spawn.name = "spawnProjectile".into();
    spawn.lifetime_ticks = 2;
    spawn.brick.radius = 0.;
    spawn.brick.direct = false;
    pack.projectiles.insert(spawn.id.clone(), spawn);
    pack.validate()?;
    let mut session = common::session_with(empty_world());
    session.set_weapon_pack(pack)?;
    let server = server::start(session, common::options())?;
    for round in 0..5 {
        let mut client = Client::connect(
            server.address,
            &server.certificate,
            format!("Joiner {round}"),
            Vec::new(),
            None,
        )
        .await?;
        let joined = client.replica.tick;
        wait_for(&mut client, |c| c.replica.tick > joined + 24).await?;
        assert!(
            client.replica.weapons.projectiles.is_empty(),
            "round {round}: still flying {:?}",
            client
                .replica
                .weapons
                .projectiles
                .iter()
                .map(|p| (&p.definition, p.age))
                .collect::<Vec<_>>()
        );
        client.close();
    }
    server.stop().await?;
    Ok(())
}
