//! A body whose archetype climbs takes hold of a ladder it faces and
//! pushes toward, runs up and down it at the volume's speed with no
//! gravity, and lets go when it jumps, looks away, leaves the volume or
//! comes down onto the floor. A body without the ability ignores ladders.
use bri_motor::climb::{self, Climb};
use bri_motor::player::{MoveInput, Player, PlayerTuning};
use glam::Vec3;
use rapier3d::prelude::*;

/// A floor, a wall 3 units ahead (-Z) rising 20 units, and a ladder
/// volume in front of it from the floor to 12 units up, facing the wall.
fn world() -> PhysicsWorld {
    world_with(0.8, false)
}

/// [`world`] with a volume `half_depth` deep each side of z -1.8 (the body is
/// 1.76 wide, so a volume it hangs inside must be deeper than that), that lets
/// the climber strafe when `strafe`.
fn world_with(half_depth: f32, strafe: bool) -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(50.0, 0.5, 50.0).translation(Vector::new(0.0, -0.5, 0.0)),
    );
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(10.0, 10.0, 0.5).translation(Vector::new(0.0, 10.0, -3.5)),
    );
    world.insert_collider(
        climb::collider(
            Vec3::new(0.0, 6.0, -1.8),
            Vec3::new(1.5, 6.0, half_depth),
            0.0,
            8.0,
            strafe,
        ),
        None,
    );
    bri_physics::detect_collisions(&mut world);
    world
}

fn climber() -> PlayerTuning {
    PlayerTuning {
        climb: Some(Climb::default()),
        ..Default::default()
    }
}

fn run(world: &mut PhysicsWorld, player: &mut Player, input: MoveInput, steps: usize) {
    for _ in 0..steps {
        player.step(world, input).unwrap();
    }
}

fn push(forward: f32) -> MoveInput {
    MoveInput {
        forward,
        ..Default::default()
    }
}

#[test]
fn walking_into_a_ladder_it_faces_takes_hold_and_runs_up_at_the_ladders_speed() {
    let mut w = world();
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 1.5), climber()).unwrap();
    run(&mut w, &mut p, MoveInput::default(), 30);
    assert!(!p.state().posture.climbing);
    // Forward carries the middle into the volume (z of -1 or less).
    run(&mut w, &mut p, push(1.0), 120);
    assert!(p.state().posture.climbing, "{:?}", p.state());
    let start = p.state().feet[1];
    // Half a second of climbing at 8 a second.
    run(&mut w, &mut p, push(1.0), 60);
    let rise = p.state().feet[1] - start;
    assert!((rise - 4.0).abs() < 0.4, "rose {rise}");
    assert_eq!(p.state().yaw, 0.0, "faces the rungs");
}

#[test]
fn a_climber_hangs_without_falling_and_comes_down_to_the_floor() {
    let mut w = world();
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, -1.4), climber()).unwrap();
    run(&mut w, &mut p, push(1.0), 90);
    assert!(p.state().posture.climbing);
    let up = p.state().feet[1];
    assert!(up > 3.0, "{up}");
    run(&mut w, &mut p, MoveInput::default(), 120);
    assert!(p.state().posture.climbing, "no gravity pulls it off");
    assert!((p.state().feet[1] - up).abs() < 0.05, "hangs still");
    run(&mut w, &mut p, push(-1.0), 400);
    let s = p.state();
    assert!(!s.posture.climbing, "stepped off onto the floor: {s:?}");
    assert!(s.grounded && s.feet[1] < 0.1);
}

#[test]
fn leaving_the_top_of_the_volume_lets_go_with_the_speed_it_had() {
    let mut w = world();
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, -1.4), climber()).unwrap();
    run(&mut w, &mut p, push(1.0), 90);
    assert!(p.state().posture.climbing);
    // Up until the middle leaves the box (12 above the floor, less half
    // the body).
    let mut was_climbing_at = 0.0;
    for _ in 0..400 {
        p.step(&mut w, push(1.0)).unwrap();
        if p.state().posture.climbing {
            was_climbing_at = p.state().feet[1];
        } else {
            break;
        }
    }
    assert!(!p.state().posture.climbing, "never let go");
    assert!(
        was_climbing_at > 9.5 && was_climbing_at < 11.0,
        "let go at {was_climbing_at}"
    );
    // It leaves still rising, as Unreal's ladder exit keeps the velocity.
    assert!(p.state().velocity[1] > 4.0, "{:?}", p.state().velocity);
}

#[test]
fn a_hop_off_the_rungs_while_looking_away_does_not_take_hold_again() {
    let mut w = world();
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, -1.4), climber()).unwrap();
    run(&mut w, &mut p, push(1.0), 90);
    assert!(p.state().posture.climbing);
    let away = MoveInput {
        yaw: std::f32::consts::FRAC_PI_2,
        jump: true,
        ..Default::default()
    };
    run(&mut w, &mut p, away, 8);
    assert!(!p.state().posture.climbing);
    assert!(p.state().velocity[1] > 5.0 || p.state().feet[1] > 4.0);
}

#[test]
fn a_body_that_cannot_climb_walks_through_a_ladder_as_air() {
    let mut w = world();
    let mut p = Player::spawn(
        &mut w,
        1,
        Vec3::new(0.0, 0.05, 1.5),
        PlayerTuning::default(),
    )
    .unwrap();
    run(&mut w, &mut p, push(1.0), 240);
    assert!(!p.state().posture.climbing);
    assert!(p.state().feet[1] < 0.5, "still on the floor");
}

#[test]
fn a_climber_not_facing_the_ladder_does_not_take_hold() {
    let mut w = world();
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 1.5), climber()).unwrap();
    // Walking sideways across the front of the volume, facing +X.
    let sideways = MoveInput {
        right: 1.0,
        yaw: std::f32::consts::FRAC_PI_2,
        ..Default::default()
    };
    run(&mut w, &mut p, sideways, 60);
    assert!(!p.state().posture.climbing);
}

#[test]
fn a_climber_with_a_wall_pull_hangs_against_the_wall_it_faces() {
    let tuning = PlayerTuning {
        climb: Some(Climb {
            wall_pull: 4.0,
            ..Climb::default()
        }),
        ..Default::default()
    };
    // Held in front of the volume (z -0.6 is its near edge, z -3.0 its far
    // side), it ends against the wall face at z -3.0, half a body width out.
    let mut w = world_with(1.2, false);
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 1.5), tuning.clone()).unwrap();
    run(&mut w, &mut p, push(1.0), 120);
    assert!(p.state().posture.climbing);
    run(&mut w, &mut p, push(1.0), 120);
    let z = p.state().feet[2];
    let against = -3.0 + tuning.width * 0.5;
    assert!((z - against).abs() < 0.15, "z {z}, wall at {against}");
    // Without a pull it stays where it took hold.
    let mut w = world_with(1.2, false);
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 1.5), climber()).unwrap();
    run(&mut w, &mut p, push(1.0), 240);
    assert!(p.state().feet[2] > against + 1.0, "{}", p.state().feet[2]);
}

#[test]
fn a_wall_pull_stops_with_the_far_side_of_the_body_at_the_far_side_of_the_volume() {
    // A volume no deeper than the body would have its middle drawn to the
    // far plane and its disc hanging over whatever lies behind: the pull ends
    // a body width short of it instead, so the climber hangs inside.
    let tuning = PlayerTuning {
        climb: Some(Climb {
            wall_pull: 4.0,
            ..Climb::default()
        }),
        ..Default::default()
    };
    let mut w = world_with(0.8, false);
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 1.5), tuning.clone()).unwrap();
    run(&mut w, &mut p, push(1.0), 120);
    run(&mut w, &mut p, push(1.0), 120);
    let far = -1.8 - 0.8;
    let z = p.state().feet[2];
    assert!(
        z > far + tuning.width * 0.5 - 0.1,
        "z {z}: its far side is past the volume's at {far}"
    );
}

#[test]
fn a_diagonal_on_the_rungs_is_no_faster_than_straight_up() {
    let straight = |input: MoveInput| {
        let mut w = world_with(1.2, true);
        let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 1.5), climber()).unwrap();
        run(&mut w, &mut p, push(1.0), 120);
        assert!(p.state().posture.climbing);
        let before = Vec3::from(p.state().feet);
        run(&mut w, &mut p, input, 20);
        assert!(p.state().posture.climbing);
        (Vec3::from(p.state().feet) - before).length()
    };
    let up = straight(push(1.0));
    let diagonal = straight(MoveInput {
        forward: 1.0,
        right: 1.0,
        ..Default::default()
    });
    assert!(up > 0.1, "{up}");
    assert!(diagonal <= up * 1.05, "diagonal {diagonal}, straight {up}");
}
