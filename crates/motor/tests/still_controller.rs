//! A body whose Add-On controller owns its running (the archetype's own
//! `acceleration` 0) stands still on level ground: nothing in the motor
//! lifts it off the floor, so it stays grounded and can jump.
use bri_motor::controller::{Controller, ControllerTick, MoveController, Probe};
use bri_motor::player::{MoveInput, Player, PlayerTuning};
use glam::Vec3;
use rapier3d::prelude::*;
use std::sync::Arc;

/// A controller like an Add-On's that owns the horizontal velocity and
/// leaves the vertical to the motor: no keys, no horizontal speed.
#[derive(Debug)]
struct OwnsRunning;
impl MoveController for OwnsRunning {
    fn id(&self) -> &str {
        "test:controller/owns-running"
    }
    fn step(&self, tick: &mut ControllerTick, _: &mut Probe) -> Result<(), String> {
        tick.velocity.x = tick.wish.x;
        tick.velocity.z = tick.wish.z;
        Ok(())
    }
}

fn floor() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(50.0, 0.5, 50.0).translation(Vector::new(0.0, -0.5, 0.0)),
    );
    bri_physics::detect_collisions(&mut world);
    world
}

fn stands_still(tuning: PlayerTuning) -> (f32, bool) {
    let mut world = floor();
    let mut player = Player::spawn(&mut world, 1, Vec3::new(0.0, 0.05, 0.0), tuning).unwrap();
    let mut lowest_grounded = true;
    // Thirty seconds at 120 Hz.
    for step in 0..3600 {
        player.step(&mut world, MoveInput::default()).unwrap();
        if step > 120 {
            lowest_grounded &= player.state().grounded;
        }
    }
    (player.state().feet[1], lowest_grounded)
}

#[test]
fn a_body_whose_controller_owns_its_running_stays_on_the_floor() {
    let tuning = PlayerTuning {
        acceleration: 0.0,
        air_control: 0.0,
        jump_speed: 0.0,
        drag: 0.001,
        gravity: 13.0,
        width: 1.648,
        stand_height: 2.884,
        controller: Some(Controller(Arc::new(OwnsRunning))),
        ..Default::default()
    };
    let (y, grounded) = stands_still(tuning);
    assert!(
        y.abs() < 0.02 && grounded,
        "feet at {y}, grounded throughout: {grounded}"
    );
}

#[test]
fn a_standard_body_stays_on_the_floor() {
    let (y, grounded) = stands_still(PlayerTuning::default());
    assert!(
        y.abs() < 0.02 && grounded,
        "feet at {y}, grounded throughout: {grounded}"
    );
}
