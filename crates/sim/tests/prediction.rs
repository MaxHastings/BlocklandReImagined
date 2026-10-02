//! Client prediction against the real server session input queue, with
//! delayed and lossy delivery in both directions.
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    prediction::{CollisionMirror, INPUT_HISTORY, Predictor},
    session::Session,
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::VecDeque;

fn map() -> Vec<ColliderBuilder> {
    vec![
        ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
        // A wall and a step so collision response is part of the replay.
        ColliderBuilder::cuboid(0.5, 4.0, 6.0).translation(Vector::new(4.0, 4.0, -10.0)),
        ColliderBuilder::cuboid(3.0, 0.2, 2.0).translation(Vector::new(0.0, 0.2, -6.0)),
    ]
}
fn server() -> Session {
    Session::new(
        Simulation::new(
            World::new("Prediction".into(), "test".into(), vec![[1.0; 4]]),
            Definitions::default(),
            map(),
        )
        .unwrap(),
    )
}
fn input(tick: u64) -> MoveInput {
    MoveInput {
        forward: if tick < 200 { 1.0 } else { 0.0 },
        right: if (100..160).contains(&tick) { 1.0 } else { 0.0 },
        yaw: (tick as f32 * 0.004).sin() * 0.8,
        jump: (30..70).contains(&tick),
        jet: (220..320).contains(&tick),
        crouch: (280..330).contains(&tick),
        ..Default::default()
    }
}

#[test]
fn prediction_matches_server_under_delay_loss_and_redundancy() {
    let mut session = server();
    let owner = session
        .join("a".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let (initial, _) = session.motion_states().remove(0);
    let mirror = CollisionMirror::new(Definitions::default(), map(), vec![]);
    let mut prediction = Predictor::new(mirror, initial, Default::default()).unwrap();
    let mut to_server: VecDeque<(u64, u64, Vec<MoveInput>)> = VecDeque::new();
    let mut to_client = VecDeque::new();
    let mut worst = 0.0_f32;
    let mut acknowledged = false;
    for tick in 1..=600_u64 {
        // Like the real client, keep sending (idle) inputs every tick.
        if tick <= 581 {
            prediction.step(input(tick)).unwrap();
            // Deterministic 20% datagram loss; redundancy repeats six inputs.
            if !tick.is_multiple_of(5) {
                let recent: Vec<_> = prediction.recent(6).map(|(_, i)| *i).collect();
                to_server.push_back((tick + 6, prediction.sequence(), recent));
            }
        }
        while to_server.front().is_some_and(|(due, _, _)| *due <= tick) {
            let (_, newest, inputs) = to_server.pop_front().unwrap();
            let first = newest + 1 - inputs.len() as u64;
            for (i, input) in inputs.into_iter().enumerate() {
                session.movement(owner, first + i as u64, input).unwrap();
            }
        }
        session.step().unwrap();
        if tick.is_multiple_of(3) && !tick.is_multiple_of(7) {
            let (state, ack) = session.motion_states().remove(0);
            to_client.push_back((tick + 6, tick, ack, state));
        }
        while to_client.front().is_some_and(|(due, _, _, _)| *due <= tick) {
            let (_, server_tick, ack, state) = to_client.pop_front().unwrap();
            let first = !acknowledged;
            if let Some(offset) = prediction.reconcile(server_tick, ack, state).unwrap() {
                // Before any input arrives the server idles the player (it may
                // fall); the first acknowledgement absorbs that difference.
                if !first {
                    worst = worst.max(offset.length());
                }
                acknowledged |= ack > 0;
            }
        }
    }
    // Same motor, same collision, same input sequence: no visible corrections.
    assert!(worst < 1e-3, "prediction diverged by {worst}");
    let (state, ack) = session.motion_states().remove(0);
    assert_eq!(ack, 581);
    prediction.reconcile(601, ack, state.clone()).unwrap();
    assert_eq!(prediction.state(), &state);
    assert_eq!(prediction.pending_len(), 0);
    // The run exercised jumping, jetting and the raised step.
    assert!(Vec3::from(state.feet).distance(Vec3::ZERO) > 5.0);
}

#[test]
fn prediction_bumps_into_other_players_like_the_host() {
    let mut session = server();
    let a = session
        .join("a".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let b = session
        .join("b".into(), Vec3::new(0.0, 0.05, -3.0), false)
        .unwrap();
    let state = |session: &Session, owner| {
        session
            .motion_states()
            .into_iter()
            .find(|(s, _)| s.owner == owner)
            .unwrap()
            .0
    };
    let mirror = CollisionMirror::new(Definitions::default(), map(), vec![]);
    let mut prediction = Predictor::new(mirror, state(&session, a), Default::default()).unwrap();
    // Walk straight at the other player, in lockstep with the host.
    let walk = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    let mut worst = 0.0_f32;
    for tick in 1..=240_u64 {
        prediction.set_others([&state(&session, b)]).unwrap();
        let (sequence, _) = prediction.step(walk).unwrap();
        session.movement(a, sequence, walk).unwrap();
        session.movement(b, tick, MoveInput::default()).unwrap();
        session.step().unwrap();
        let host = Vec3::from(state(&session, a).feet);
        worst = worst.max(host.distance(Vec3::from(prediction.state().feet)));
    }
    // The host stopped the walker at the other body; so did the prediction.
    assert!(Vec3::from(state(&session, a).feet).z > -3.0);
    assert!(worst < 0.05, "prediction walked {worst} past the host");
}

#[test]
fn server_input_queue_ignores_duplicates_and_bounds_rate() {
    let mut session = server();
    let owner = session
        .join("a".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for sequence in 1..=10 {
        session.movement(owner, sequence, input(sequence)).unwrap();
        // Redundant resend of the same input is ignored, not an error.
        session.movement(owner, sequence, input(sequence)).unwrap();
    }
    session.step().unwrap();
    // Backlog above the target drains three inputs per tick.
    assert_eq!(session.motion_states()[0].1, 3);
    // The token bucket rejects sustained floods.
    let mut rejected = false;
    for sequence in 11..200 {
        rejected |= session
            .movement(owner, sequence, MoveInput::default())
            .is_err();
    }
    assert!(rejected);
}

#[test]
fn stale_and_forged_corrections_are_rejected_and_history_is_bounded() {
    let session = {
        let mut s = server();
        s.join("a".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        s
    };
    let (initial, _) = session.motion_states().remove(0);
    let mirror = CollisionMirror::new(Definitions::default(), map(), vec![]);
    let mut prediction = Predictor::new(mirror, initial.clone(), Default::default()).unwrap();
    for _ in 0..INPUT_HISTORY + 20 {
        prediction.step(MoveInput::default()).unwrap();
    }
    assert_eq!(prediction.pending_len(), INPUT_HISTORY);
    let mut forged = initial.clone();
    forged.owner += 1;
    assert!(prediction.reconcile(10, 1, forged).is_err());
    assert!(
        prediction
            .reconcile(10, prediction.sequence() + 1, initial.clone())
            .is_err()
    );
    assert!(
        prediction
            .reconcile(10, 5, initial.clone())
            .unwrap()
            .is_some()
    );
    // Older server ticks never rewind an applied correction.
    assert!(prediction.reconcile(9, 6, initial).unwrap().is_none());
}

/// A correction that moves the body no further than float noise keeps the
/// local motion, but takes everything else the host decided: a rope the
/// host tied to a player standing still reaches their own prediction (the
/// Grapple Rope's, which the client once dropped until they moved).
#[test]
fn a_correction_within_noise_still_takes_the_hosts_rope() {
    let session = {
        let mut s = server();
        s.join("a".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        s
    };
    let (initial, _) = session.motion_states().remove(0);
    let mirror = CollisionMirror::new(Definitions::default(), map(), vec![]);
    let mut prediction = Predictor::new(mirror, initial.clone(), Default::default()).unwrap();
    let mut roped = initial.clone();
    roped.tether = Some(bri_sim::player::Tether {
        anchor: [0.0, 8.0, -4.0],
        length: 9.0,
        target: 9.0,
        reel: 0.0,
        swing: 0.0,
        drift: [0.0; 3],
        keys: None,
        winding: 0,
        straight: false,
    });
    assert_eq!(
        prediction.reconcile(1, 0, roped.clone()).unwrap(),
        Some(Vec3::ZERO),
        "no visible correction"
    );
    assert_eq!(prediction.state().tether, roped.tether);
}

fn plate_definitions() -> Definitions {
    use bri_content::{
        brick::Brick as Mesh,
        collision::{CollisionBody, Part},
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
            "plate".into(),
            bri_sim::definitions::Definition {
                mesh: Mesh {
                    schema_version: 1,
                    id: "plate".into(),
                    footprint_studs: [2, 1],
                    height_plates: 1,
                    attachment_rows: vec!["bb".into()],
                    collision_boxes: vec![],
                    needs_external_collision: false,
                    coverage: None,
                    quads: vec![],
                },
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]
        .into(),
    }
}
/// `count` plates on a grid, keyed from 1.
fn plates(count: u64) -> bri_world::Bricks {
    (1..=count)
        .map(|id| {
            let (x, z) = ((id % 200) as f32, (id / 200) as f32);
            let position = [x - 100.0, 0.1, z * 0.5 - 50.0];
            let brick =
                bri_world::Brick::new(bri_world::ContentRef::Resolved("plate".into()), position, 1);
            (id, brick)
        })
        .collect()
}
#[test]
fn change_log_sync_matches_a_full_compare() {
    let mut bricks = plates(500);
    let mut full = CollisionMirror::new(plate_definitions(), map(), vec![]);
    let mut logged = CollisionMirror::new(plate_definitions(), map(), vec![]);
    assert!(full.sync(&bricks).unwrap());
    assert!(logged.sync(&bricks).unwrap());
    // Plant one, move one, remove one, as one replica revision.
    bricks.insert(900, bricks[&1].clone());
    bricks.get_mut(&900).unwrap().position[1] = 0.3;
    bricks.get_mut(&7).unwrap().position[1] = 0.5;
    bricks.remove(&8);
    assert!(full.sync(&bricks).unwrap());
    assert!(logged.sync_changes(&bricks, [900, 7, 8]).unwrap());
    // A log entry for an unchanged brick is harmless.
    assert!(!logged.sync_changes(&bricks, [9]).unwrap());
    assert!(!logged.sync(&bricks).unwrap(), "change log missed an edit");
    let count = |m: &CollisionMirror| m.physics().colliders.len();
    assert_eq!(count(&full), count(&logged));
    for (id, y) in [(900, 0.3), (7, 0.5)] {
        let hit = |m: &CollisionMirror| {
            let p = bricks[&id].position;
            m.physics()
                .query_pipeline()
                .cast_ray(
                    &Ray::new(Vector::new(p[0], 5.0, p[2]), Vector::new(0.0, -1.0, 0.0)),
                    10.0,
                    true,
                )
                .map(|(_, toi)| 5.0 - toi)
        };
        assert!(hit(&full).is_some_and(|top| (top - (y + 0.1)).abs() < 1e-4));
        assert_eq!(hit(&logged), hit(&full));
    }
}
/// `cargo test --release -p bri-sim --test prediction mirror_sync_timing -- --ignored --nocapture`
#[test]
#[ignore = "timing probe"]
fn mirror_sync_timing() {
    let mut bricks = plates(44_000);
    let mut full = CollisionMirror::new(plate_definitions(), map(), vec![]);
    let mut logged = CollisionMirror::new(plate_definitions(), map(), vec![]);
    full.sync(&bricks).unwrap();
    logged.sync(&bricks).unwrap();
    let (mut t_full, mut t_logged) = (Vec::new(), Vec::new());
    for plant in 0..20u64 {
        let id = 100_000 + plant;
        let mut brick = bricks[&(plant + 1)].clone();
        brick.position[1] = 0.3;
        bricks.insert(id, brick);
        let start = std::time::Instant::now();
        full.sync(&bricks).unwrap();
        t_full.push(start.elapsed().as_secs_f64() * 1e3);
        let start = std::time::Instant::now();
        logged.sync_changes(&bricks, [id]).unwrap();
        t_logged.push(start.elapsed().as_secs_f64() * 1e3);
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    println!(
        "44k bricks, one plant: full {:.3} ms, change log {:.3} ms (median of 20)",
        median(&mut t_full),
        median(&mut t_logged)
    );
}

/// A tool that takes the jet button: the prediction sends the press (the
/// host runs the tool's command from it) but its motor, like the host's,
/// never jets, and a correction replays exactly that.
#[test]
fn a_tool_that_takes_jet_is_predicted_without_jetting() {
    let session = {
        let mut s = server();
        s.join("a".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        s
    };
    let (initial, _) = session.motion_states().remove(0);
    let mirror = CollisionMirror::new(Definitions::default(), map(), vec![]);
    let mut prediction = Predictor::new(mirror, initial.clone(), Default::default()).unwrap();
    for _ in 0..30 {
        prediction.step(MoveInput::default()).unwrap();
    }
    let ground = prediction.state().feet[1];
    prediction.set_tool_jet(true);
    let jet = MoveInput {
        jet: true,
        ..Default::default()
    };
    for _ in 0..60 {
        prediction.step(jet).unwrap();
    }
    assert!((prediction.state().feet[1] - ground).abs() < 0.01);
    assert!(
        prediction.recent(6).all(|(_, input)| input.jet),
        "the press still reaches the host"
    );
    // A correction from the start replays the unjetted steps.
    prediction.reconcile(1, 0, initial).unwrap();
    assert!((prediction.state().feet[1] - ground).abs() < 0.05);
    prediction.set_tool_jet(false);
    for _ in 0..60 {
        prediction.step(jet).unwrap();
    }
    assert!(
        prediction.state().feet[1] > ground + 0.1,
        "without it, jet jets"
    );
}
