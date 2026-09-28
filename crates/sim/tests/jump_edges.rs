//! A jump tap on the host: one input with jump held, landing on any of the
//! 120 Hz steps inside a 32 ms motor tick.
use bri_sim::{
    definitions::Definitions, player::MoveInput, presentation::CueKind, session::Session,
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;

fn session() -> Session {
    let mut s = Session::new(
        Simulation::new(
            World::new("Flat".into(), "test".into(), vec![[1.0; 4]; 2]),
            Definitions {
                entries: Default::default(),
            },
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0))
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}

/// Join, run `idle` steps, tap jump once, and say whether the player jumped
/// and whether v20's `canJump` held when the tap arrived.
fn tap_after(idle: usize) -> (bool, bool) {
    let mut s = session();
    let owner = s
        .join("Jumper".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..idle {
        s.step().unwrap();
    }
    let p = s.snapshot().players[0].clone();
    let could = p.grounded && p.jump.delay == 0 && p.jump.since_contact == 0;
    s.take_cues();
    s.movement(
        owner,
        1,
        MoveInput {
            jump: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut jumped = false;
    for _ in 0..120 {
        s.step().unwrap();
        jumped |= s.take_cues().iter().any(|c| c.kind == CueKind::Jump);
    }
    (jumped, could)
}

/// Once a player stands where v20's `canJump` holds, a single tap jumps
/// whichever step of the 32 ms motor tick it lands on.
#[test]
fn a_tap_jumps_on_every_step_of_a_motor_tick_once_the_player_can_jump() {
    let mut tried = 0;
    for idle in 0..60 {
        let (jumped, could) = tap_after(idle);
        if could {
            tried += 1;
            assert!(jumped, "a tap after {idle} steps was eaten");
        }
    }
    // Every phase of the motor tick, several times over.
    assert!(tried >= 40, "{tried}");
}

/// v20 (`Player::updateMove`, `canJump` 0x5a2af8) counts a jumpable contact
/// only on the tick after the one that lands, so `grounded` turns true one
/// motor tick before a jump can start. A tap in that tick is a tap in the
/// air, as in v20; holding jump, as players do, jumps on the next tick.
#[test]
fn a_tap_in_the_landing_tick_is_a_tap_in_the_air_as_in_v20() {
    let landing: Vec<usize> = (0..60)
        .filter(|&idle| {
            let mut s = session();
            s.join("Jumper".into(), Vec3::new(0.0, 0.05, 0.0), false)
                .unwrap();
            for _ in 0..idle {
                s.step().unwrap();
            }
            let p = &s.snapshot().players[0];
            p.grounded && p.jump.since_contact != 0
        })
        .collect();
    assert!(!landing.is_empty());
    for idle in landing {
        assert_eq!(tap_after(idle), (false, false), "after {idle} steps");
    }
}
