//! Jeep steering against blocklandv20.exe with the two steering prefs:
//! `Player::updateMove` (0x5b2e89) turns a held strafe key into
//! +-steeringStrafeSteeringRate (0.1) of move yaw per 32 ms tick,
//! `Vehicle::updateMove` (0x56b590) adds the move yaw to the steering
//! clamped to maxSteeringAngle (0.9785), and `WheeledVehicle::updateMove`
//! (0x570c4a) returns it on a move without yaw by
//! 1 - 0.9 x min(|throttle|, 10) / 10 per tick.
#[macro_use]
mod common;
use bri_vehicles::*;
use common::Fixture;
use rapier3d::prelude::PhysicsWorld;

/// 120 Hz steps per 32 ms source tick.
const PER_TICK: f32 = 96. / 25.;

/// The strafe-steered car, mounted.
fn jeep(f: &Fixture) -> (VehiclesWorld, PhysicsWorld) {
    vehicle(f, f.car)
}
fn vehicle(f: &Fixture, definition: &str) -> (VehiclesWorld, PhysicsWorld) {
    let mut v = f.vehicles();
    let mut w = common::floor(2000.);
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: definition.into(),
            transform: Transform {
                position: [0., 2., 0.],
                ..Default::default()
            },
            spawn_id: None,
            respawn_ticks: None,
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
    let seat = v.snapshot(&w).vehicles[0].seats[0].transform.position;
    v.mount(
        &w,
        VehicleId(1),
        0,
        Occupant {
            id: OccupantId(20),
            owner: OwnerId(10),
            body: [1.25, 2.65],
        },
        seat,
    )
    .unwrap();
    (v, w)
}

fn drive(v: &mut VehiclesWorld, w: &mut PhysicsWorld, ticks: f32, c: Controls) -> f32 {
    for _ in 0..(ticks * PER_TICK).round() as usize {
        v.set_controls(OwnerId(10), OccupantId(20), c).unwrap();
        v.pre_step(w, &[]).unwrap();
        w.step();
        v.post_step(w).unwrap();
    }
    v.snapshot(w).vehicles[0].steering
}

on_both! {
fn strafe_keys_steer_the_jeep_and_the_steering_returns_with_throttle(f: &Fixture) {
    let d = f.definition(f.car);
    let (rate, max) = (d.steering.strafe_rate, d.max_steering);
    assert!(d.strafe_steering && d.steering.auto_return);
    assert!(5. * rate < max && 25. * rate > max, "five ticks stay below the lock");
    let (mut v, mut w) = jeep(f);
    let right = Controls {
        strafe: 1.,
        // The mouse only looks around while the keys steer.
        look_delta: [0.05, 0.],
        ..Default::default()
    };
    // Five ticks of the strafe rate each.
    let s = drive(&mut v, &mut w, 5., right);
    assert!((s - 5. * rate).abs() < 0.01, "steering {s}");
    // Clamped at the steering angle.
    let s = drive(&mut v, &mut w, 20., right);
    assert!((s - max).abs() < 0.01, "steering {s}");
    // Released with the throttle released: it holds.
    let s = drive(&mut v, &mut w, 20., Controls::default());
    assert!((s - max).abs() < 0.01, "held steering {s}");
    // Released at full throttle: the return rate times the throttle's
    // share of the return's full speed, per tick.
    let gas = Controls {
        throttle: 1.,
        ..Default::default()
    };
    let s = drive(&mut v, &mut w, 10., gas);
    let share = 1f32.min(d.steering.auto_return_max_speed) / d.steering.auto_return_max_speed;
    let want = max * (1. - d.steering.auto_return_rate * share).powi(10);
    assert!(want < max * 0.9, "it returns noticeably");
    assert!((s - want).abs() < 0.02, "returned to {s}, want {want}");
}
}

on_both! {
fn steering_prefs_off_make_the_jeep_mouse_steered_and_hold_its_turn(f: &Fixture) {
    assert!(0.3 < f.definition(f.car).max_steering);
    let (mut v, mut w) = jeep(f);
    let off = |c: Controls| Controls {
        strafe_steering_off: true,
        auto_return_off: true,
        ..c
    };
    // Strafe steering off: the keys no longer steer, the mouse does.
    let s = drive(&mut v, &mut w, 5., off(Controls {
        strafe: 1.,
        ..Default::default()
    }));
    assert!(s.abs() < 1e-4, "keys steered {s}");
    let s = drive(&mut v, &mut w, 1. / PER_TICK, off(Controls {
        look_delta: [0.3, 0.],
        ..Default::default()
    }));
    assert!((s - 0.3).abs() < 0.01, "mouse steering {s}");
    // Auto-return off: full throttle leaves the turn where it is.
    let s = drive(&mut v, &mut w, 10., off(Controls {
        throttle: 1.,
        ..Default::default()
    }));
    assert!((s - 0.3).abs() < 0.01, "held steering {s}");
}
}

on_both! {
/// Max, v0.1.4: the Tank's mouse steering and its return fought. With
/// auto-return on, v20 returns the steering on a 32 ms move without yaw;
/// a mouse moving at the frame rate puts yaw in only some 120 Hz moves,
/// and the ones between must not count as "no yaw". The steering a steady
/// mouse builds matches a move-by-move mouse, and returns once it stops.
fn auto_return_waits_a_whole_move_before_fighting_the_mouse(f: &Fixture) {
    let steer = |every: usize| {
        let (mut v, mut w) = jeep(f);
        let mut steering = 0.;
        // 0.12 rad of mouse a v20 tick, arriving every `every` steps, at
        // full throttle with strafe steering off and auto-return on.
        let per_step = 0.12 / PER_TICK;
        for step in 0..24 {
            let c = Controls {
                throttle: 1.,
                strafe_steering_off: true,
                look_delta: [
                    if step % every == every - 1 {
                        per_step * every as f32
                    } else {
                        0.
                    },
                    0.,
                ],
                ..Default::default()
            };
            steering = drive(&mut v, &mut w, 1. / PER_TICK, c);
        }
        let released = drive(
            &mut v,
            &mut w,
            4.,
            Controls {
                throttle: 1.,
                strafe_steering_off: true,
                ..Default::default()
            },
        );
        (steering, released)
    };
    let (every_step, _) = steer(1);
    for every in [2, 3] {
        let (steering, released) = steer(every);
        assert!(
            (steering - every_step).abs() < 0.02,
            "a mouse every {every} steps steered {steering}, every step {every_step}"
        );
        assert!(released < steering * 0.8, "it returns once the mouse stops: {released}");
    }
}
}
