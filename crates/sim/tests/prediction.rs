use bri_sim::{
    player::{MoveInput, Player, PlayerTuning},
    prediction::Predictor,
};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::VecDeque;
fn scene() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert_collider(
        ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
        None,
    );
    world.detect_collisions(&(), &());
    world
}
#[test]
fn delayed_lossy_corrections_replay_inputs_without_replaying_other_physics() {
    let mut server_world = scene();
    let mut client_world = scene();
    let mut server = Player::spawn(
        &mut server_world,
        1,
        Vec3::new(0.0, 0.05, 0.0),
        PlayerTuning::default(),
    )
    .unwrap();
    let local = Player::spawn(
        &mut client_world,
        1,
        Vec3::new(0.0, 0.05, 0.0),
        PlayerTuning::default(),
    )
    .unwrap();
    let mut prediction = Predictor::new(local);
    let (other, _) = client_world.insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(20.0, 10.0, 20.0)),
        ColliderBuilder::ball(0.2),
    );
    let other_start = *client_world.bodies[other].position();
    let mut snapshots = VecDeque::new();
    let mut applied = MoveInput::default();
    let mut ack = 0;
    for tick in 1..=240_u64 {
        let input = MoveInput {
            forward: if tick < 120 { 1.0 } else { 0.0 },
            jump: (30..70).contains(&tick),
            jet: tick > 150,
            crouch: tick > 180,
            ..Default::default()
        };
        prediction.step(&mut client_world, tick, input).unwrap();
        // Deterministic 20% input loss and 100 ms snapshot delay.
        if !tick.is_multiple_of(5) || tick == 240 {
            applied = input;
            ack = tick;
        }
        server.step(&mut server_world, applied).unwrap();
        server_world.step();
        if tick.is_multiple_of(6) {
            snapshots.push_back((tick + 12, tick, ack, server.state().clone()));
        }
        while snapshots.front().is_some_and(|(due, _, _, _)| *due <= tick) {
            let (_, snapshot_tick, ack, state) = snapshots.pop_front().unwrap();
            let offset = prediction
                .reconcile(&mut client_world, snapshot_tick, ack, state)
                .unwrap();
            assert!(offset.is_finite());
        }
        assert!(prediction.pending_len() < 30);
        assert_eq!(*client_world.bodies[other].position(), other_start);
    }
    prediction
        .reconcile(&mut client_world, 240, 240, server.state().clone())
        .unwrap();
    assert_eq!(prediction.state(), server.state());
    assert_eq!(prediction.pending_len(), 0);
    let before = prediction.state().clone();
    let mut old = before.clone();
    old.feet[1] = 999.0;
    prediction
        .reconcile(&mut client_world, 180, 180, old)
        .unwrap();
    assert_eq!(prediction.state(), &before);
}
#[test]
fn prediction_history_and_correction_identity_are_bounded() {
    let mut world = scene();
    let player = Player::spawn(
        &mut world,
        1,
        Vec3::new(0.0, 0.05, 0.0),
        PlayerTuning::default(),
    )
    .unwrap();
    let mut prediction = Predictor::new(player);
    for sequence in 1..=240 {
        prediction
            .step(&mut world, sequence, MoveInput::default())
            .unwrap();
    }
    let before = prediction.state().clone();
    assert!(
        prediction
            .step(&mut world, 241, MoveInput::default())
            .is_err()
    );
    assert_eq!(prediction.state(), &before);
    let mut forged = before.clone();
    forged.owner = 2;
    assert!(prediction.reconcile(&mut world, 240, 240, forged).is_err());
    assert_eq!(prediction.pending_len(), 240);
    assert_eq!(prediction.state(), &before);
    assert!(
        prediction
            .reconcile(&mut world, 240, 241, before.clone())
            .is_err()
    );
    prediction.reconcile(&mut world, 240, 240, before).unwrap();
    prediction
        .step(&mut world, 241, MoveInput::default())
        .unwrap();
}
