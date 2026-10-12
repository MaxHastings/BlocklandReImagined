//! An Add-On movement controller (`packages/samples/sample-wall-kick`) on the
//! authoritative session and a predicting client: the Wall Kicker kicks off
//! a wall, the client predicts the kick under 150 ms of latency each way and
//! datagram loss, a client that cannot run the controller is corrected to
//! the host's kick, and a Blockhead beside it moves exactly as it would on
//! a server without the Add-On. The Diver and the Trooper
//! (`sample-dive`, `sample-trooper`) are predicted the same way.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    archetype::Archetypes,
    definitions::Definitions,
    player::{MoveInput, PlayerState},
    prediction::{CollisionMirror, Predictor},
    session::{Command, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::VecDeque, path::PathBuf, sync::Arc};

const KICKER: &str = "sample-wall-kick:archetype/kicker";
const DIVER: &str = "sample-dive:archetype/diver";
const TROOPER: &str = "sample-trooper:archetype/trooper";
/// 150 ms one way, in 120 Hz steps.
const LATENCY: u64 = 18;
/// Before inputs arrive the host idles each body (every join and body
/// change steps the session), so its 32 ms ticks fall at other steps than
/// the client's; the first acknowledgements absorb that. Corrections count
/// from here, before the first jump (input 20) and every controller move.
const SETTLED: u64 = 15;

fn map() -> Vec<ColliderBuilder> {
    vec![
        ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
        // A tall wall whose face is at x = 3.5.
        ColliderBuilder::cuboid(0.5, 10.0, 8.0).translation(Vector::new(4.0, 10.0, 0.0)),
    ]
}
fn add_on() -> Arc<Catalog> {
    let set = PackageSet {
        schema_version: 1,
        packages: ["sample-wall-kick", "sample-dive", "sample-trooper"]
            .into_iter()
            .map(|id| PackageEntry {
                id: id.into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: id.into(),
                role: None,
            })
            .collect(),
    };
    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples");
    Arc::new(Catalog::load(&samples, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}
fn session(with_add_on: bool) -> Session {
    let mut s = Session::new(
        Simulation::new(
            World::new("Wall".into(), "test".into(), vec![[1.0; 4]]),
            Definitions::default(),
            map(),
        )
        .unwrap(),
    );
    if with_add_on {
        s.install_packages(add_on(), None).unwrap();
    }
    s
}
fn state(s: &Session, owner: u64) -> (PlayerState, u64) {
    s.motion_states()
        .into_iter()
        .find(|(p, _)| p.owner == owner)
        .unwrap()
}
/// Take a body through its Add-On's own command.
fn become_body(s: &mut Session, owner: u64, body: Body) {
    let (package, command, archetype) = match body {
        Body::Kicker => ("sample-wall-kick", "wallkick", KICKER),
        Body::Diver => ("sample-dive", "diver", DIVER),
        Body::Trooper => ("sample-trooper", "trooper", TROOPER),
    };
    s.command(
        owner,
        1,
        Command::Package(PackageCommand {
            package: package.into(),
            command: command.into(),
            args: vec![],
        }),
    )
    .unwrap();
    s.step().unwrap();
    assert!(
        s.package_diagnostics().is_empty(),
        "{:#?}",
        s.package_diagnostics()
    );
    let (p, _) = state(s, owner);
    assert_eq!(s.archetypes().resolve(p.archetype).id, archetype);
}
fn become_kicker(s: &mut Session, owner: u64) {
    become_body(s, owner, Body::Kicker);
}
#[derive(Clone, Copy, PartialEq)]
enum Body {
    Kicker,
    Diver,
    Trooper,
}
/// The client's copy of the host's table, as the checkpoint carries it:
/// serialised (modules travel, compiled controllers do not), then bound.
fn received(s: &Session, bind: bool) -> Archetypes {
    let json = serde_json::to_vec(s.archetypes()).unwrap();
    let mut table: Archetypes = serde_json::from_slice(&json).unwrap();
    table.validate().unwrap();
    assert!(
        table.iter().all(|(_, a)| a.movement.controller.is_none()),
        "compiled controllers never travel"
    );
    if bind {
        assert!(bri_sim::archetype::bind_controllers(&mut table).is_empty());
    }
    table
}

/// Run at the wall facing +x, jump, and press jump again at the wall.
/// The yaw sways a little so the run is not axis-aligned.
fn input(tick: u64) -> MoveInput {
    MoveInput {
        forward: if tick < 150 { 1.0 } else { 0.0 },
        yaw: std::f32::consts::FRAC_PI_2 + (tick as f32 * 0.01).sin() * 0.05,
        jump: (20..26).contains(&tick) || (60..64).contains(&tick),
        ..Default::default()
    }
}

/// Server-only run of `input` from the spawn; the highest point and the
/// lowest x velocity reached, and whether a kick was announced.
fn run_alone(kicker: bool) -> (f32, f32, bool) {
    let mut s = session(true);
    let owner = s
        .join("a".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    if kicker {
        become_kicker(&mut s, owner);
    }
    let (mut top, mut back, mut announced) = (0.0_f32, 0.0_f32, false);
    for tick in 1..=240 {
        s.movement(owner, tick, input(tick)).unwrap();
        s.step().unwrap();
        let (p, _) = state(&s, owner);
        top = top.max(p.feet[1]);
        back = back.min(p.velocity[0]);
        announced |= p.controller.is_some_and(|c| c.intent == 1);
    }
    (top, back, announced)
}

#[test]
fn the_wall_kicker_kicks_off_a_wall_and_the_blockhead_does_not() {
    let (blockhead_top, blockhead_back, blockhead_kick) = run_alone(false);
    let (kicker_top, kicker_back, kicker_kick) = run_alone(true);
    assert!(!blockhead_kick && kicker_kick);
    // The Blockhead's second press at the wall does nothing: it slides
    // down. The kicker springs up and away from the wall.
    assert!(blockhead_back > -1.0, "{blockhead_back}");
    assert!(
        kicker_back < -5.0,
        "kicked away from the wall: {kicker_back}"
    );
    assert!(
        kicker_top > blockhead_top + 1.0,
        "kicked higher than one jump: {kicker_top} vs {blockhead_top}"
    );
}

/// Real networking's shape: inputs and poses each 150 ms late, a fifth of
/// input datagrams lost (each carries the last six inputs), poses every
/// third step with some skipped. Same controller bytes on both sides, so
/// the client's prediction of the kick needs no visible correction.
#[test]
fn a_kick_is_predicted_under_150_ms_of_latency() {
    let [run] = networked([Client {
        body: Body::Kicker,
        at: Vec3::new(0.0, 0.05, 0.0),
        binds: true,
    }]);
    assert!(run.announced, "the host's kicker kicked");
    assert!(run.worst < 1e-3, "prediction diverged by {}", run.worst);
    assert!(run.converged);
}

/// A client that cannot run the controller (it would not compile there,
/// or it is a different version) predicts the stock motor: at the wall it
/// slides where the host kicked. The host's poses correct it, and once the
/// last input is acknowledged it is exactly where the host put it.
#[test]
fn a_client_without_the_controller_is_corrected_to_the_hosts_kick() {
    let [run] = networked([Client {
        body: Body::Kicker,
        at: Vec3::new(0.0, 0.05, 0.0),
        binds: false,
    }]);
    assert!(run.announced);
    assert!(
        run.worst > 0.5,
        "the host corrected a missed kick: {}",
        run.worst
    );
    assert!(run.converged);
}

/// Two clients on one server, each with a different Add-On's controller:
/// a Wall Kicker at the wall and a Diver in the open, both predicted under
/// the same latency and loss without a visible correction.
#[test]
fn two_clients_predict_two_different_controllers_at_once() {
    let [kicker, diver] = networked([
        Client {
            body: Body::Kicker,
            at: Vec3::new(0.0, 0.05, 0.0),
            binds: true,
        },
        Client {
            body: Body::Diver,
            at: Vec3::new(-30.0, 0.05, 20.0),
            binds: true,
        },
    ]);
    for run in [&kicker, &diver] {
        assert!(run.announced);
        assert!(run.worst < 1e-3, "prediction diverged by {}", run.worst);
        assert!(run.converged);
    }
    // The dive threw the Diver along its run at twice its running speed;
    // the kick sent the Kicker off the wall slower than that.
    assert!(diver.fastest > 12.0, "{}", diver.fastest);
    assert!(kicker.fastest < 12.0, "{}", kicker.fastest);
}

/// The Trooper, a heavy shooter's body, runs slower than a Blockhead,
/// jumps higher, and steers in the air; a client predicts all of it.
#[test]
fn the_trooper_is_slower_and_floatier_and_is_predicted() {
    let [trooper] = networked([Client {
        body: Body::Trooper,
        at: Vec3::new(-30.0, 0.05, -20.0),
        binds: true,
    }]);
    assert!(trooper.announced, "it steered in the air");
    assert!(
        trooper.worst < 1e-3,
        "prediction diverged by {}",
        trooper.worst
    );
    assert!(trooper.converged);
    // A Blockhead runs at 7 and rises 12²/(2·20) = 3.6.
    assert!(trooper.fastest < 7.0, "{}", trooper.fastest);
    assert!(trooper.highest > 4.5, "{}", trooper.highest);
}

struct Client {
    body: Body,
    at: Vec3,
    /// Whether the client can compile the host's controllers.
    binds: bool,
}
struct Run {
    /// The largest correction once the host has acknowledged the first
    /// [`SETTLED`] inputs.
    worst: f32,
    /// The host's body announced its controller's move (intent 1 or 2).
    announced: bool,
    /// The client ended exactly where the host did.
    converged: bool,
    /// The host's body's fastest speed across the ground.
    fastest: f32,
    /// The host's body's highest rise above where it joined.
    highest: f32,
}

/// One host, one predicting client per entry, each sending `input` with
/// the Diver pressing crouch where the Kicker presses jump again, and the
/// Trooper jumping once and strafing right in the air.
fn networked<const N: usize>(clients: [Client; N]) -> [Run; N] {
    let mut s = session(true);
    let owners: Vec<u64> = clients
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let owner = s.join(format!("p{i}"), c.at, false).unwrap();
            become_body(&mut s, owner, c.body);
            owner
        })
        .collect();
    let input = |body: Body, tick: u64| {
        let mut i = input(tick);
        if body == Body::Diver {
            i.jump = (20..26).contains(&tick);
            i.crouch = (40..44).contains(&tick);
        }
        if body == Body::Trooper {
            i.jump = (20..26).contains(&tick);
            i.right = if (30..80).contains(&tick) { 1.0 } else { 0.0 };
        }
        i
    };
    let mut predictors: Vec<Predictor> = clients
        .iter()
        .zip(&owners)
        .map(|(c, owner)| {
            let mirror = CollisionMirror::new(Definitions::default(), map(), vec![]);
            Predictor::new(mirror, state(&s, *owner).0, received(&s, c.binds)).unwrap()
        })
        .collect();
    let mut runs: Vec<Run> = clients
        .iter()
        .map(|_| Run {
            worst: 0.0,
            announced: false,
            converged: false,
            fastest: 0.0,
            highest: 0.0,
        })
        .collect();
    let mut to_server: Vec<VecDeque<(u64, u64, Vec<MoveInput>)>> = vec![VecDeque::new(); N];
    let mut to_client: Vec<VecDeque<(u64, u64, u64, PlayerState)>> = vec![VecDeque::new(); N];
    // Not a multiple of five: the last input is not among the lost ones.
    let last_input = 201;
    for tick in 1..=last_input + 3 * LATENCY {
        for (i, client) in predictors.iter_mut().enumerate() {
            if tick <= last_input {
                client.step(input(clients[i].body, tick)).unwrap();
                if !tick.is_multiple_of(5) {
                    let recent: Vec<_> = client.recent(6).map(|(_, i)| *i).collect();
                    to_server[i].push_back((tick + LATENCY, client.sequence(), recent));
                }
            }
            while to_server[i].front().is_some_and(|(due, ..)| *due <= tick) {
                let (_, newest, inputs) = to_server[i].pop_front().unwrap();
                let first = newest + 1 - inputs.len() as u64;
                for (n, input) in inputs.into_iter().enumerate() {
                    s.movement(owners[i], first + n as u64, input).unwrap();
                }
            }
        }
        s.step().unwrap();
        for (i, client) in predictors.iter_mut().enumerate() {
            let (p, ack) = state(&s, owners[i]);
            runs[i].announced |= p.controller.is_some_and(|c| c.intent != 0);
            let across = Vec3::from(p.velocity).with_y(0.0).length();
            runs[i].fastest = runs[i].fastest.max(across);
            runs[i].highest = runs[i].highest.max(p.feet[1] - clients[i].at.y);
            // Each client also bumps into the other bodies where the host
            // last showed them, as the real client does.
            let others: Vec<PlayerState> = owners
                .iter()
                .filter(|o| **o != owners[i])
                .map(|o| state(&s, *o).0)
                .collect();
            client.set_others(others.iter()).unwrap();
            if tick.is_multiple_of(3) && !tick.is_multiple_of(7) {
                to_client[i].push_back((tick + LATENCY, tick, ack, p));
            }
            while to_client[i].front().is_some_and(|(due, ..)| *due <= tick) {
                let (_, server_tick, ack, p) = to_client[i].pop_front().unwrap();
                if let Some(offset) = client
                    .reconcile(server_tick, ack, p, Default::default())
                    .unwrap()
                    && ack >= SETTLED
                {
                    runs[i].worst = runs[i].worst.max(offset.length());
                }
            }
        }
    }
    for (i, client) in predictors.iter_mut().enumerate() {
        let (host, ack) = state(&s, owners[i]);
        assert_eq!(ack, last_input);
        client
            .reconcile(u64::MAX, ack, host.clone(), Default::default())
            .unwrap();
        runs[i].converged = client.state() == &host;
    }
    runs.try_into().unwrap_or_else(|_| unreachable!())
}

/// One server, two bodies: a Blockhead and a Wall Kicker running the same
/// inputs side by side. The Blockhead's every state is exactly what it is
/// on a server without the Add-On.
#[test]
fn a_blockhead_beside_a_kicker_moves_exactly_as_without_the_add_on() {
    let mut plain = session(false);
    let alone = plain
        .join("a".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let mut mixed = session(true);
    let blockhead = mixed
        .join("a".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    // The kicker runs at the wall a little further along it.
    let kicker = mixed
        .join("b".into(), Vec3::new(0.0, 0.05, 5.0), false)
        .unwrap();
    become_kicker(&mut mixed, kicker);
    plain.step().unwrap();
    let mut differed = false;
    for tick in 1..=240 {
        plain.movement(alone, tick, input(tick)).unwrap();
        mixed.movement(blockhead, tick, input(tick)).unwrap();
        mixed.movement(kicker, tick, input(tick)).unwrap();
        plain.step().unwrap();
        mixed.step().unwrap();
        let (a, _) = state(&plain, alone);
        let (b, _) = state(&mixed, blockhead);
        let (k, _) = state(&mixed, kicker);
        assert_eq!(a.feet, b.feet, "tick {tick}");
        assert_eq!(a.velocity, b.velocity, "tick {tick}");
        assert_eq!(b.controller, None);
        differed |= k.feet[1] > a.feet[1] + 0.5;
    }
    assert!(differed, "the kicker beside it did kick");
}
