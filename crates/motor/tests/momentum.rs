//! A body with momentum builds speed under a held move, slides when it is let go, and steers weakly in the air; a body without
//! it is steered straight to the move's speed as v20's players are.
use bri_motor::momentum::Momentum;
use bri_motor::player::{MoveInput, Player, PlayerTuning};
use glam::Vec3;
use rapier3d::prelude::*;

fn floor() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
    );
    bri_physics::detect_collisions(&mut world);
    world
}

fn heavy() -> PlayerTuning {
    PlayerTuning {
        momentum: Some(Momentum {
            acceleration: 81.92,
            friction: 8.0,
            stop_speed: 0.4,
            air_control: 0.35,
        }),
        ..Default::default()
    }
}

fn speed(p: &Player) -> f32 {
    let v = p.state().velocity;
    Vec3::new(v[0], 0.0, v[2]).length()
}

fn run(w: &mut PhysicsWorld, p: &mut Player, input: MoveInput, steps: usize) {
    for _ in 0..steps {
        p.step(w, input).unwrap();
    }
}

fn forward() -> MoveInput {
    MoveInput {
        forward: 1.0,
        ..Default::default()
    }
}

#[test]
fn a_body_with_momentum_takes_time_to_reach_speed_and_slides_when_let_go() {
    let mut w = floor();
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 0.0), heavy()).unwrap();
    run(&mut w, &mut p, MoveInput::default(), 30);
    run(&mut w, &mut p, forward(), 2);
    assert!(speed(&p) > 0.0 && speed(&p) < 3.0, "{}", speed(&p));
    run(&mut w, &mut p, forward(), 240);
    assert!((speed(&p) - 7.0).abs() < 0.1, "{}", speed(&p));
    run(&mut w, &mut p, MoveInput::default(), 4);
    assert!(speed(&p) > 0.5 && speed(&p) < 7.0, "{}", speed(&p));
    run(&mut w, &mut p, MoveInput::default(), 240);
    assert!(speed(&p) < 0.1, "{}", speed(&p));
}

#[test]
fn a_body_without_momentum_is_pushed_by_its_own_acceleration_alone() {
    let mut w = floor();
    let mut plain = Player::spawn(
        &mut w,
        1,
        Vec3::new(0.0, 0.05, 0.0),
        PlayerTuning::default(),
    )
    .unwrap();
    let mut heavy = Player::spawn(&mut w, 2, Vec3::new(50.0, 0.05, 0.0), heavy()).unwrap();
    run(&mut w, &mut plain, MoveInput::default(), 30);
    run(&mut w, &mut heavy, MoveInput::default(), 30);
    run(&mut w, &mut plain, forward(), 6);
    run(&mut w, &mut heavy, forward(), 6);
    // v20's run force is 48 a second and the momentum's 81.92, from the same standing start.
    let ratio = speed(&plain) / speed(&heavy);
    assert!((ratio - 48.0 / 81.92).abs() < 0.1, "{ratio}");
}
