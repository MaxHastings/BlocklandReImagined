//! A map collider marked with the query types it blocks stops a walking body only when it lists `move`.
use bri_motor::player::{MoveInput, Player, PlayerTuning};
use bri_physics::blocks::collider_groups;
use glam::Vec3;
use rapier3d::prelude::*;

/// A floor and a wall 3 units ahead (-Z) that blocks the named query types (None: all of them).
fn world(blocks: Option<&[&str]>) -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(50.0, 0.5, 50.0).translation(Vector::new(0.0, -0.5, 0.0)),
    );
    let names: Option<Vec<String>> = blocks.map(|b| b.iter().map(|s| s.to_string()).collect());
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(10.0, 10.0, 0.5)
            .translation(Vector::new(0.0, 10.0, -3.5))
            .collision_groups(collider_groups(names.as_deref())),
    );
    bri_physics::detect_collisions(&mut world);
    world
}

/// How far toward the wall (-Z) a body gets walking for a while.
fn walk_into(blocks: Option<&[&str]>) -> f32 {
    let mut w = world(blocks);
    let mut p = Player::spawn(
        &mut w,
        1,
        Vec3::new(0.0, 0.05, 1.5),
        PlayerTuning::default(),
    )
    .unwrap();
    for _ in 0..360 {
        p.step(
            &mut w,
            MoveInput {
                forward: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    }
    p.state().feet[2]
}

#[test]
fn only_a_wall_that_lists_move_stops_a_walking_body() {
    assert!(walk_into(None) > -3.6, "an unmasked wall stops it");
    assert!(
        walk_into(Some(&["move", "sweep"])) > -3.6,
        "a wall that lists move stops it"
    );
    assert!(
        walk_into(Some(&["ray", "shot"])) < -4.0,
        "a wall that lists only rays and shots lets it through"
    );
}
