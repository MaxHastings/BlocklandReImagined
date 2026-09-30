//! The player motor's rope (`Tether`): a body swings on it like a pendulum,
//! it goes slack when the body comes closer, reels in and out, the movement
//! keys pump a swing, and prediction runs it exactly as the host does.
use bri_sim::player::{MoveInput, Player, PlayerTuning, Tether};
use glam::Vec3;
use rapier3d::prelude::*;

/// A world with a floor far below, so a swing never touches it.
fn open_air() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert_collider(
        ColliderBuilder::cuboid(500.0, 0.5, 500.0).translation(Vector::new(0.0, -200.5, 0.0)),
        None,
    );
    world.detect_collisions(&(), &());
    world
}
fn floor() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert_collider(
        ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
        None,
    );
    world.detect_collisions(&(), &());
    world
}
fn step(p: &mut Player, w: &mut PhysicsWorld, input: MoveInput, n: usize) {
    for _ in 0..n {
        p.step(w, input).unwrap();
        w.step();
    }
}
fn rope(anchor: Vec3, length: f32) -> Tether {
    Tether {
        anchor: anchor.to_array(),
        length,
        target: length,
        reel: 24.0,
        swing: 7.0,
    }
}
fn grip(p: &Player) -> Vec3 {
    Tether::grip(Vec3::from(p.state().feet), p.tuning())
}
fn stretch(p: &Player) -> f32 {
    let t = p.state().tether.unwrap();
    grip(p).distance(Vec3::from(t.anchor)) - t.length
}

#[test]
fn a_rope_swings_its_player_like_a_pendulum() {
    let mut world = open_air();
    let tuning = PlayerTuning::default();
    let anchor = Vec3::new(0.0, 60.0, 0.0);
    let length = 20.0;
    // Held out level with the anchor, the rope straight, then let go.
    let feet = anchor + Vec3::X * length - Vec3::Y * tuning.stand_height * 0.85;
    let mut p = Player::spawn(&mut world, 1, feet, tuning.clone()).unwrap();
    p.set_tether(Some(rope(anchor, length))).unwrap();
    let mut lowest = f32::MAX;
    let mut fastest = 0.0_f32;
    let mut far_side = f32::MAX;
    let mut worst_stretch = 0.0_f32;
    for _ in 0..(120 * 3) {
        step(&mut p, &mut world, MoveInput::default(), 1);
        let g = grip(&p);
        lowest = lowest.min(g.y);
        fastest = fastest.max(Vec3::from(p.state().velocity).length());
        far_side = far_side.min(g.x);
        worst_stretch = worst_stretch.max(stretch(&p));
    }
    // The rope never lets it fall past its length (one 32 ms tick of
    // swinging at most stretches it a little before it pulls back in).
    assert!(worst_stretch < 0.6, "stretched {worst_stretch}");
    // It swung down through the bottom, a rope's length below the anchor...
    assert!((anchor.y - length - lowest).abs() < 0.8, "lowest {lowest}");
    // ...about as fast as falling that far (v20's air drag takes a little)...
    let free_fall = (2.0 * tuning.gravity * length).sqrt();
    assert!(fastest > free_fall * 0.8 && fastest < free_fall * 1.1, "{fastest} vs {free_fall}");
    // ...and up the other side, most of the way to level.
    assert!(far_side < -length * 0.75, "reached {far_side}");
    assert!(p.state().tether.is_some());
}

#[test]
fn a_slack_rope_lets_its_player_fall_until_it_pulls_tight() {
    let mut world = open_air();
    let tuning = PlayerTuning::default();
    let anchor = Vec3::new(0.0, 60.0, 0.0);
    let feet = anchor - Vec3::Y * (tuning.stand_height * 0.85 + 2.0);
    let mut p = Player::spawn(&mut world, 1, feet, tuning).unwrap();
    p.set_tether(Some(rope(anchor, 12.0))).unwrap();
    // Ten units of slack: it falls freely at first.
    step(&mut p, &mut world, MoveInput::default(), 36);
    assert!(p.state().velocity[1] < -5.0, "{:?}", p.state().velocity);
    // Then the rope catches it and holds it hanging a rope's length down.
    step(&mut p, &mut world, MoveInput::default(), 600);
    assert!(stretch(&p).abs() < 0.3, "stretch {}", stretch(&p));
    assert!(Vec3::from(p.state().velocity).length() < 1.0);
}

#[test]
fn reeling_in_lifts_a_player_off_the_ground_and_reeling_out_lowers_them() {
    let mut world = floor();
    let tuning = PlayerTuning::default();
    let mut p = Player::spawn(&mut world, 1, Vec3::new(0.0, 0.05, 0.0), tuning.clone()).unwrap();
    step(&mut p, &mut world, MoveInput::default(), 60);
    assert!(p.state().grounded);
    let anchor = grip(&p) + Vec3::new(3.0, 20.0, 0.0);
    let length = grip(&p).distance(anchor);
    p.set_tether(Some(rope(anchor, length))).unwrap();
    let mut tether = p.state().tether.unwrap();
    tether.target = 4.0;
    p.set_tether(Some(tether)).unwrap();
    step(&mut p, &mut world, MoveInput::default(), 240);
    let t = p.state().tether.unwrap();
    assert_eq!(t.length, 4.0);
    assert!(!p.state().grounded);
    assert!(grip(&p).distance(anchor) < 4.5, "{}", grip(&p).distance(anchor));
    // Paid out again, the player comes back down to the floor.
    let mut tether = t;
    tether.target = 40.0;
    p.set_tether(Some(tether)).unwrap();
    step(&mut p, &mut world, MoveInput::default(), 480);
    assert!(p.state().grounded, "{:?}", p.state());
    assert_eq!(p.state().tether.unwrap().length, 40.0);
}

#[test]
fn a_rope_is_a_leash_on_the_ground() {
    let mut world = floor();
    let tuning = PlayerTuning::default();
    let mut p = Player::spawn(&mut world, 1, Vec3::new(0.0, 0.05, 0.0), tuning).unwrap();
    step(&mut p, &mut world, MoveInput::default(), 60);
    let anchor = grip(&p);
    p.set_tether(Some(rope(anchor, 6.0))).unwrap();
    let forward = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    step(&mut p, &mut world, forward, 600);
    let flat = Vec3::from(p.state().feet) - (anchor - Vec3::Y * anchor.y);
    assert!(flat.length() < 6.5, "walked {} from the anchor", flat.length());
}

#[test]
fn the_movement_keys_pump_a_swing_up_from_hanging_still() {
    let mut world = open_air();
    let tuning = PlayerTuning::default();
    let anchor = Vec3::new(0.0, 60.0, 0.0);
    let feet = anchor - Vec3::Y * (tuning.stand_height * 0.85 + 15.0);
    let mut p = Player::spawn(&mut world, 1, feet, tuning).unwrap();
    p.set_tether(Some(rope(anchor, 15.0))).unwrap();
    step(&mut p, &mut world, MoveInput::default(), 120);
    assert!(Vec3::from(p.state().velocity).length() < 0.5);
    // Pumping in time with the swing, as on a playground swing: push the
    // way it is already going (facing -z, forward pushes toward -z).
    let mut highest = f32::MIN;
    for _ in 0..(120 * 8) {
        let going = p.state().velocity[2];
        let input = MoveInput {
            forward: if going <= 0.0 { 1.0 } else { -1.0 },
            ..Default::default()
        };
        step(&mut p, &mut world, input, 1);
        highest = highest.max(grip(&p).y);
    }
    // Up past level with the halfway point of the rope.
    assert!(highest > anchor.y - 7.5, "rose to {highest}");
    assert!(stretch(&p) < 0.6);
}

#[test]
fn a_rope_breaks_when_its_player_is_carried_far_past_it_and_on_teleport() {
    let mut world = open_air();
    let tuning = PlayerTuning::default();
    let anchor = Vec3::new(0.0, 60.0, 0.0);
    let feet = anchor - Vec3::Y * (tuning.stand_height * 0.85 + 5.0);
    let mut p = Player::spawn(&mut world, 1, feet, tuning).unwrap();
    p.set_tether(Some(rope(anchor, 5.0))).unwrap();
    // A blast flings them.
    p.push(Vec3::new(0.0, 0.0, 200.0));
    step(&mut p, &mut world, MoveInput::default(), 12);
    assert!(p.state().tether.is_some(), "a rope holds one hard shove");
    p.set_tether(Some(rope(anchor, 5.0))).unwrap();
    p.teleport(&mut world, feet + Vec3::X * 40.0, 0.0).unwrap();
    assert!(p.state().tether.is_none(), "a teleport cuts the rope");
    p.set_tether(Some(rope(anchor, 5.0))).unwrap();
    let mut state = p.state().clone();
    state.feet = (feet + Vec3::X * 40.0).to_array();
    let tuning = p.tuning().clone();
    p.restore(&mut world, state, tuning).unwrap();
    step(&mut p, &mut world, MoveInput::default(), 4);
    assert!(p.state().tether.is_none(), "stretched past breaking");
    assert!(p.set_tether(Some(rope(anchor, 0.2))).is_err());
    assert!(p.set_tether(Some(rope(Vec3::splat(f32::NAN), 5.0))).is_err());
}

#[test]
fn a_restored_swing_replays_exactly_as_the_host_ran_it() {
    let anchor = Vec3::new(0.0, 60.0, 0.0);
    let tuning = PlayerTuning::default();
    let feet = anchor + Vec3::new(12.0, -8.0, 3.0);
    let mut host_world = open_air();
    let mut host = Player::spawn(&mut host_world, 1, feet, tuning.clone()).unwrap();
    host.set_tether(Some(rope(anchor, 16.0))).unwrap();
    let inputs: Vec<MoveInput> = (0..600)
        .map(|i| MoveInput {
            forward: ((i / 50) % 3) as f32 - 1.0,
            right: ((i / 70) % 2) as f32,
            yaw: (i as f32 * 0.01).sin(),
            ..Default::default()
        })
        .collect();
    step(&mut host, &mut host_world, inputs[0], 1);
    for input in &inputs[1..200] {
        step(&mut host, &mut host_world, *input, 1);
    }
    // The client picks up from the host's state mid-swing and runs the
    // same inputs.
    let mut client_world = open_air();
    let mut client = Player::attach(&mut client_world, host.state().clone(), tuning).unwrap();
    for input in &inputs[200..] {
        step(&mut host, &mut host_world, *input, 1);
        step(&mut client, &mut client_world, *input, 1);
    }
    assert_eq!(host.state(), client.state());
    assert!(host.state().tether.is_some());
}
