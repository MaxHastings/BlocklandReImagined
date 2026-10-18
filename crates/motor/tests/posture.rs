//! A body whose archetype can lie prone and lean: rules lay it down and
//! get it up, it crawls at its prone speeds in its low box, and its eye
//! slides sideways as it leans, never leaving the body.
use bri_motor::player::{Lean, MoveInput, Player, PlayerTuning, Prone};
use glam::Vec3;
use rapier3d::prelude::*;

fn world(ceiling: Option<f32>) -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(50.0, 0.5, 50.0).translation(Vector::new(0.0, -0.5, 0.0)),
    );
    if let Some(y) = ceiling {
        world.insert(
            RigidBodyBuilder::fixed(),
            // Over everything 5 units ahead (-Z) and on.
            ColliderBuilder::cuboid(50.0, 0.5, 25.0).translation(Vector::new(0.0, y + 0.5, -30.0)),
        );
    }
    bri_physics::detect_collisions(&mut world);
    world
}

fn soldier() -> PlayerTuning {
    PlayerTuning {
        prone: Some(Prone {
            height: 0.5,
            eye: 0.35,
            forward: 1.0,
            backward: 0.5,
            sideways: 0.5,
        }),
        lean: Some(Lean {
            distance: 0.5,
            angle: 0.3,
            body_angle: None,
            seconds: 0.25,
        }),
        ..Default::default()
    }
}

fn settle(world: &mut PhysicsWorld, player: &mut Player, input: MoveInput, steps: usize) {
    for _ in 0..steps {
        player.step(world, input).unwrap();
    }
}

#[test]
fn a_prone_body_crawls_low_and_gets_up_only_with_room() {
    let mut w = world(None);
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 0.0), soldier()).unwrap();
    settle(&mut w, &mut p, MoveInput::default(), 60);
    assert!(p.set_prone(&mut w, true));
    let forward = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    settle(&mut w, &mut p, forward, 240);
    let s = p.state();
    assert!(s.posture.prone && s.crouched, "{s:?}");
    let (min, max) = p.world_bounds();
    assert!((max[1] - min[1] - 0.5).abs() < 1e-3, "the prone box");
    let speed = Vec3::from(s.velocity).length();
    assert!((0.8..1.2).contains(&speed), "crawling at 1: {speed}");
    assert!(
        (p.eye().y - s.feet[1] - 0.35).abs() < 0.05,
        "eye {}",
        p.eye()
    );
    // Up: crouched, then standing once the crouch key is let go.
    assert!(p.set_prone(&mut w, false));
    settle(&mut w, &mut p, MoveInput::default(), 120);
    assert!(!p.state().posture.prone && !p.state().crouched);

    // Crawl under a 0.8-high ceiling: no room to rise to the 1-high crouch.
    let mut w = world(Some(0.8));
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 0.0), soldier()).unwrap();
    settle(&mut w, &mut p, MoveInput::default(), 30);
    assert!(p.set_prone(&mut w, true));
    settle(&mut w, &mut p, forward, 120 * 9);
    assert!(
        p.state().feet[2] < -6.0,
        "under the ceiling: {:?}",
        p.state().feet
    );
    assert!(!p.set_prone(&mut w, false), "no room to get up");
    assert!(p.state().posture.prone);
}

#[test]
fn a_lean_eases_the_eye_sideways_within_the_body() {
    let mut w = world(None);
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 0.0), soldier()).unwrap();
    settle(&mut w, &mut p, MoveInput::default(), 60);
    let upright = p.eye();
    assert!(p.set_lean(1.0));
    settle(&mut w, &mut p, MoveInput::default(), 6);
    let partway = p.state().posture.lean;
    assert!(partway > 0.0 && partway < 1.0, "easing: {partway}");
    settle(&mut w, &mut p, MoveInput::default(), 60);
    // Facing -Z (yaw 0), right is +X.
    let moved = p.eye() - upright;
    assert!(
        (moved.x - 0.5).abs() < 1e-3 && moved.z.abs() < 1e-3,
        "{moved}"
    );
    assert!(moved.x <= p.tuning().width * 0.5);
    assert!((p.state().lean_tilt(p.tuning()) - 0.3).abs() < 1e-4);
    // The body tilts with the view unless the lean sets its own angle.
    assert!((p.state().lean_body_tilt(p.tuning()) - 0.3).abs() < 1e-4);
    let mut tuning = p.tuning().clone();
    tuning.lean.as_mut().unwrap().body_angle = Some(0.0);
    assert!(
        (p.state().lean_tilt(&tuning) - 0.3).abs() < 1e-4,
        "the view still rolls"
    );
    assert_eq!(
        p.state().lean_body_tilt(&tuning),
        0.0,
        "the body stays upright"
    );
    tuning.lean.as_mut().unwrap().body_angle = Some(0.1);
    assert!((p.state().lean_body_tilt(&tuning) - 0.1).abs() < 1e-4);
    // A body without a lean cannot.
    let mut q = Player::spawn(
        &mut w,
        2,
        Vec3::new(5.0, 0.05, 0.0),
        PlayerTuning::default(),
    )
    .unwrap();
    assert!(!q.set_lean(1.0) && !q.set_prone(&mut w, true));
}

#[test]
fn mounted_eye_is_shared_by_interactions_and_clears_on_dismount() {
    let mut w = world(None);
    let mut p = Player::spawn(&mut w, 1, Vec3::new(3., 4., 5.), soldier()).unwrap();
    let standing = p.eye();
    p.set_seated(true);
    p.set_mounted_eye(Some(Vec3::new(3.25, 5.1, 4.8)));
    assert!(p.eye().distance(Vec3::new(3.25, 5.1, 4.8)) < 1e-5);
    p.set_mounted_eye(Some(Vec3::NAN));
    assert!(p.eye().is_finite());
    p.set_mounted_eye(Some(Vec3::new(3.25, 5.1, 4.8)));
    p.set_seated(false);
    assert!(p.eye().distance(standing) < 1e-5);
}
