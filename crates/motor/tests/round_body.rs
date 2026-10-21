//! A round body turns aside at a wall's end where the box stops dead against it.
use bri_motor::player::{Body, MoveInput, Player, PlayerTuning};
use glam::Vec3;
use rapier3d::prelude::*;

/// A floor and a wall across x < 0, 3 units ahead (-Z), ending at x = 0.
fn world() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(50.0, 0.5, 50.0).translation(Vector::new(0.0, -0.5, 0.0)),
    );
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(10.0, 10.0, 0.5).translation(Vector::new(-10.0, 10.0, -3.5)),
    );
    bri_physics::detect_collisions(&mut world);
    world
}

/// Where a body walking -Z for a while ends, starting `offset` to the right of the wall's end.
fn walk_past_the_end(body: Body, offset: f32) -> Vec3 {
    let mut w = world();
    let tuning = PlayerTuning {
        body,
        ..PlayerTuning::default()
    };
    let mut p = Player::spawn(&mut w, 1, Vec3::new(offset, 0.05, 1.5), tuning).unwrap();
    for _ in 0..480 {
        p.step(
            &mut w,
            MoveInput {
                forward: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    }
    Vec3::from(p.state().feet)
}

#[test]
fn a_cylinder_slides_round_a_wall_end_the_box_catches_on() {
    let width = PlayerTuning::default().width;
    // Its centre is a quarter of its radius clear of the wall's end: the
    // box's side still overlaps the wall's face, the cylinder's edge does not.
    let offset = width * 0.5 * 0.25;
    let boxed = walk_past_the_end(Body::Box, offset);
    assert!(boxed.z > -3.0, "the box is stopped by the face: {boxed}");
    let round = walk_past_the_end(Body::Cylinder, offset);
    assert!(round.z < -5.0, "the cylinder slides past the end: {round}");
    // Square on to the face, it stops as the box does.
    let square = walk_past_the_end(Body::Cylinder, -2.0);
    assert!(square.z > -3.0, "a wall's face still stops it: {square}");
}
