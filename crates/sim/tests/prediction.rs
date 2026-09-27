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
    let owner = session.join("a".into(), Vec3::new(0.0, 0.05, 0.0), false).unwrap();
    let (initial, _) = session.motion_states().remove(0);
    let mirror = CollisionMirror::new(Definitions::default(), map(), vec![]);
    let mut prediction = Predictor::new(mirror, initial).unwrap();
    let mut to_server: VecDeque<(u64, u64, Vec<MoveInput>)> = VecDeque::new();
    let mut to_client = VecDeque::new();
    let mut worst = 0.0_f32;
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
            if let Some(offset) = prediction.reconcile(server_tick, ack, state).unwrap() {
                worst = worst.max(offset.length());
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
fn server_input_queue_ignores_duplicates_and_bounds_rate() {
    let mut session = server();
    let owner = session.join("a".into(), Vec3::new(0.0, 0.05, 0.0), false).unwrap();
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
        rejected |= session.movement(owner, sequence, MoveInput::default()).is_err();
    }
    assert!(rejected);
}

#[test]
fn stale_and_forged_corrections_are_rejected_and_history_is_bounded() {
    let session = {
        let mut s = server();
        s.join("a".into(), Vec3::new(0.0, 0.05, 0.0), false).unwrap();
        s
    };
    let (initial, _) = session.motion_states().remove(0);
    let mirror = CollisionMirror::new(Definitions::default(), map(), vec![]);
    let mut prediction = Predictor::new(mirror, initial.clone()).unwrap();
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
    assert!(prediction.reconcile(10, 5, initial.clone()).unwrap().is_some());
    // Older server ticks never rewind an applied correction.
    assert!(prediction.reconcile(9, 6, initial).unwrap().is_none());
}
