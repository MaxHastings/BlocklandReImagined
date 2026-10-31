//! A body whose Add-On controller owns its running (the archetype's own
//! `acceleration` 0) stands still on level ground: nothing in the motor
//! lifts it off the floor, so it stays grounded and can jump.
use bri_console::Clamp;
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

fn mesh_floor() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    let h = 72.0;
    let points = vec![
        Vector::new(-h, 0.0, -h),
        Vector::new(h, 0.0, -h),
        Vector::new(h, 0.0, h),
        Vector::new(-h, 0.0, h),
    ];
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::trimesh_with_flags(
            points,
            vec![[0, 2, 1], [0, 3, 2]],
            TriMeshFlags::FIX_INTERNAL_EDGES,
        )
        .unwrap(),
    );
    bri_physics::detect_collisions(&mut world);
    world
}

/// A shot that pushes a standing body very slightly down (a rifle round
/// fired from a little above, about 0.003 units/s into the floor) moves it
/// too little each tick for the sweep to see the floor. Nothing took that
/// speed off a body with no run force, so it sank through the floor,
/// whether a box or a map's mesh, and fell out of the world.
#[test]
fn a_body_whose_controller_owns_its_running_is_not_pushed_through_the_floor() {
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
    let pushes = [
        0.0001f32, 0.0026, 0.005, 0.0099, 0.0101, 0.0105, 0.011, 0.012, 0.015, 0.02, 0.0026, 0.05,
        0.0026, 0.1, 0.0005, 0.5, 0.0026,
    ];
    for mesh in [false, true] {
        let mut world = if mesh { mesh_floor() } else { floor() };
        let mut player =
            Player::spawn(&mut world, 1, Vec3::new(9.85, 0.05, -19.45), tuning.clone()).unwrap();
        for _ in 0..240 {
            player.step(&mut world, MoveInput::default()).unwrap();
        }
        for down in pushes {
            player.push(Vec3::new(0.0, -down, 0.0));
            for step in 0..1200 {
                player.step(&mut world, MoveInput::default()).unwrap();
                let state = player.state();
                assert!(
                    state.feet[1] > -0.001 && (step < 4 || state.grounded),
                    "pushed down {down} (mesh floor: {mesh}): feet at {:?} moving {:?}, \
                     grounded {} after {step} steps",
                    state.feet,
                    state.velocity,
                    state.grounded
                );
            }
        }
    }
}

#[test]
fn a_living_body_supports_another_until_it_becomes_a_corpse() {
    let mut world = floor();
    let tuning = PlayerTuning::default();
    let height = tuning.stand_height;
    let mut lower = Player::spawn(&mut world, 1, Vec3::new(0., 0.05, 0.), tuning.clone()).unwrap();
    let mut upper = Player::spawn(&mut world, 2, Vec3::new(0., height + 0.6, 0.), tuning).unwrap();
    for _ in 0..240 {
        lower.step(&mut world, MoveInput::default()).unwrap();
        upper.step(&mut world, MoveInput::default()).unwrap();
    }
    assert!(
        upper.state().grounded && upper.state().feet[1] > height - 0.1,
        "living body must support the upper body: {:?}",
        upper.state()
    );
    assert_eq!(
        upper.support_tag(),
        Some((1_u128 << 64) | 1),
        "the motor identifies the actual supporting actor"
    );
    upper.set_pass_group(&mut world, 1).unwrap();
    assert_eq!(
        upper.support_tag(),
        None,
        "changing collision eligibility invalidates the old contact"
    );
    for _ in 0..8 {
        upper.step(&mut world, MoveInput::default()).unwrap();
    }
    assert_eq!(upper.support_tag(), Some((1_u128 << 64) | 1));
    lower.set_corpse(&mut world, true);
    for _ in 0..240 {
        lower.step(&mut world, MoveInput::default()).unwrap();
        upper.step(&mut world, MoveInput::default()).unwrap();
    }
    assert!(
        upper.state().grounded && upper.state().feet[1] < 0.1,
        "death must revoke support: {:?}",
        upper.state()
    );
    assert_ne!(
        upper.support_tag(),
        Some((1_u128 << 64) | 1),
        "a corpse cannot remain the support"
    );
}

#[test]
fn ordinary_jump_can_land_on_a_living_body_without_jets() {
    let mut world = floor();
    let tuning = PlayerTuning::default();
    let height = tuning.stand_height;
    let mut lower = Player::spawn(&mut world, 1, Vec3::new(0., 0.05, 0.), tuning.clone()).unwrap();
    let mut upper = Player::spawn(&mut world, 2, Vec3::new(-2.0, 0.05, 0.), tuning).unwrap();
    for _ in 0..30 {
        lower.step(&mut world, MoveInput::default()).unwrap();
        upper.step(&mut world, MoveInput::default()).unwrap();
    }
    assert!(upper.state().grounded);
    let mut landed = false;
    for tick in 0..180 {
        lower.step(&mut world, MoveInput::default()).unwrap();
        let x = upper.state().feet[0];
        let vx = upper.state().velocity[0];
        upper
            .step(
                &mut world,
                MoveInput {
                    forward: (-x * 0.8 - vx * 0.6).clamped(-1.0, 1.0),
                    yaw: std::f32::consts::FRAC_PI_2,
                    jump: tick == 0,
                    ..Default::default()
                },
            )
            .unwrap();
        if upper.state().grounded && upper.state().feet[1] > height - 0.1 {
            landed = true;
            break;
        }
    }
    assert!(
        landed,
        "ordinary jump failed to land on the living support: {:?}",
        upper.state()
    );
}
