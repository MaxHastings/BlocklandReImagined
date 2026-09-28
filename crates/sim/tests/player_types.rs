//! v20 player datablocks on the shared player motor.
use bri_sim::{
    player::{MoveInput, Player, PlayerTuning},
    player_types::PlayerType,
};
use glam::Vec3;
use rapier3d::prelude::*;

fn scene() -> PhysicsWorld {
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
fn spawn(w: &mut PhysicsWorld, datablock: PlayerType) -> Player {
    let mut p = Player::spawn(w, 1, Vec3::new(0.0, 0.05, 0.0), PlayerTuning::default()).unwrap();
    p.set_archetype(w, datablock.archetype(), datablock.tuning(), 1.0)
        .unwrap();
    p.refill_energy();
    step(&mut p, w, MoveInput::default(), 60);
    assert!(p.state().grounded);
    p
}
const JET: MoveInput = MoveInput {
    forward: 0.0,
    right: 0.0,
    yaw: 0.0,
    pitch: 0.0,
    head_yaw: 0.0,
    jump: false,
    crouch: false,
    jet: true,
};

#[test]
fn only_jetting_datablocks_leave_the_ground() {
    for (datablock, flies) in [
        (PlayerType::Standard, true),
        (PlayerType::NoJet, false),
        (PlayerType::Quake, false),
        (PlayerType::Horse, false),
    ] {
        let mut w = scene();
        let mut p = spawn(&mut w, datablock);
        step(&mut p, &mut w, JET, 120);
        assert_eq!(p.state().feet[1] > 0.5, flies, "{datablock:?}");
    }
}

#[test]
fn fuel_jets_run_dry_and_recharge() {
    let mut w = scene();
    let mut p = spawn(&mut w, PlayerType::FuelJet);
    assert_eq!(p.state().energy, 100.0);
    // Drain 2 per 32 ms tick against a recharge of 0.8: about 2.7 seconds.
    step(&mut p, &mut w, JET, 240);
    assert!(p.state().energy < 30.0, "{}", p.state().energy);
    step(&mut p, &mut w, JET, 240);
    assert!(p.state().energy < 2.0 + 1.0);
    let peak = p.state().feet[1];
    step(&mut p, &mut w, JET, 480);
    assert!(p.state().feet[1] < peak, "an empty tank still climbs");
    step(&mut p, &mut w, MoveInput::default(), 600);
    assert!(p.state().energy > 99.0);
}

#[test]
fn quake_and_horse_run_at_their_own_speeds_in_their_own_boxes() {
    let run = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    for (datablock, speed, width) in [
        (PlayerType::Quake, 15.0, 1.25),
        (PlayerType::Horse, 12.0, 2.5),
    ] {
        let mut w = scene();
        let mut p = spawn(&mut w, datablock);
        step(&mut p, &mut w, run, 240);
        let v = Vec3::from(p.state().velocity);
        assert!((v.length() - speed).abs() < 0.05, "{datablock:?} {v}");
        let (min, max) = p.world_bounds();
        assert!((max[0] - min[0] - width).abs() < 0.001);
    }
}

#[test]
fn scale_grows_the_box_and_eye_but_not_the_speed() {
    let mut w = scene();
    let mut p = spawn(&mut w, PlayerType::Standard);
    p.set_archetype(
        &mut w,
        PlayerType::Standard.archetype(),
        PlayerType::Standard.tuning(),
        2.0,
    )
    .unwrap();
    step(
        &mut p,
        &mut w,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        240,
    );
    let (min, max) = p.world_bounds();
    assert!((max[1] - min[1] - 5.3).abs() < 0.001);
    assert!((p.eye().y - p.state().feet[1] - 2.0 * 2.156_496_5).abs() < 0.001);
    assert!((Vec3::from(p.state().velocity).length() - 7.0).abs() < 0.05);
}

#[test]
fn ball_shoot_player_can_only_jump() {
    let mut w = scene();
    let mut p = spawn(&mut w, PlayerType::BallShoot);
    step(
        &mut p,
        &mut w,
        MoveInput {
            forward: 1.0,
            right: 1.0,
            ..Default::default()
        },
        120,
    );
    let f = p.state().feet;
    assert!(f[0].abs() < 0.01 && f[2].abs() < 0.01);
    assert_eq!(PlayerType::from_id("v20.player.ballshootplayer"), None);
}

fn session() -> bri_sim::session::Session {
    use bri_sim::{definitions::Definitions, session::Session, simulation::Simulation};
    let mut s = Session::new(
        Simulation::new(
            bri_world::World::new("Types".into(), "test".into(), vec![[1.0; 4]]),
            Definitions::default(),
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}
fn datablock(s: &bri_sim::session::Session, owner: u64) -> PlayerType {
    let archetype = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap()
        .archetype;
    PlayerType::EVERY[usize::from(archetype.0)]
}

#[test]
fn mini_game_player_type_applies_on_spawn_and_on_update() {
    use bri_minigames::Settings;
    use bri_sim::session::{Command, MiniGameRequest};
    let mut s = session();
    let a = s
        .join("Owner".into(), Vec3::new(5.0, 0.05, 0.0), false)
        .unwrap();
    assert_eq!(datablock(&s, a), PlayerType::Standard);
    let settings = Settings {
        player_type: PlayerType::Quake.id().into(),
        ..Settings::default()
    };
    s.command(
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap();
    s.step().unwrap();
    assert_eq!(datablock(&s, a), PlayerType::Quake);
    let settings = Settings {
        player_type: PlayerType::Horse.id().into(),
        ..Settings::default()
    };
    s.command(
        a,
        2,
        Command::MiniGame(MiniGameRequest::Configure { settings }),
    )
    .unwrap();
    s.step().unwrap();
    assert_eq!(datablock(&s, a), PlayerType::Horse);
    assert_eq!(s.vitals()[&a].health, 250.0);
    s.command(a, 3, Command::MiniGame(MiniGameRequest::Leave))
        .unwrap();
    s.step().unwrap();
    assert_eq!(datablock(&s, a), PlayerType::Standard);
}

#[test]
fn the_host_eye_follows_the_crouch_thread_like_the_camera() {
    use bri_sim::crouch::{CROUCH_SECONDS, CrouchThread};
    let mut w = scene();
    let mut p = spawn(&mut w, PlayerType::Standard);
    let tuning = PlayerTuning::default();
    let crouch = MoveInput {
        crouch: true,
        ..Default::default()
    };
    // The client's view runs the same thread over the same steps, from the
    // crouch the motor reports (it changes on v20's 32 ms ticks).
    let mut view = CrouchThread::default();
    view.update(false, 0.0, CROUCH_SECONDS);
    let mut dipped = false;
    for tick in 0..16 {
        let input = if tick < 8 {
            crouch
        } else {
            MoveInput::default()
        };
        step(&mut p, &mut w, input, 1);
        view.update(p.state().crouched, bri_physics::FIXED_DT, CROUCH_SECONDS);
        let camera = tuning.eye_height(view.eye_fraction(CROUCH_SECONDS));
        let eye = p.eye().y - p.state().feet[1];
        assert!(
            (eye - camera).abs() < 1e-4,
            "tick {tick}: {eye} vs {camera}"
        );
        if p.state().crouched && !dipped {
            dipped = true;
            assert!(
                eye < tuning.stand_eye && eye > tuning.crouch_eye,
                "the eye dips rather than snapping: {eye}"
            );
        }
    }
    assert!(dipped);
}
