//! The Flying Wheeled Jeep against blocklandv20.exe `WheeledVehicle::updateForces`:
//! thrust below `maxForwardVel`, lift from speed along the nose, control
//! surfaces that bite only above `stallSpeed`, and no jets.
#[macro_use]
mod common;
use bri_vehicles::*;
use common::Fixture;
use glam::{Quat, Vec3};
use rapier3d::prelude::*;

/// A mounted flying car at `height`, moving `speed` toward its nose.
fn flying_jeep(f: &Fixture, height: f32, speed: f32) -> (VehiclesWorld, PhysicsWorld) {
    let mut v = f.vehicles();
    let mut w = common::floor(2000.);
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: f.flying_car.into(),
            transform: Transform {
                position: [0., height, 0.],
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
    let (_, b) = w.bodies.iter_mut().find(|(_, b)| b.is_dynamic()).unwrap();
    b.set_linvel(Vec3::new(0., 0., -speed), true);
    (v, w)
}

fn fly(v: &mut VehiclesWorld, w: &mut PhysicsWorld, steps: usize, controls: Controls) {
    for _ in 0..steps {
        v.set_controls(OwnerId(10), OccupantId(20), controls)
            .unwrap();
        v.pre_step(w, &[]).unwrap();
        w.step();
        v.post_step(w).unwrap();
    }
}

fn state(v: &VehiclesWorld, w: &PhysicsWorld) -> (Vec3, Vec3, Quat, Vec3) {
    let s = &v.snapshot(w).vehicles[0];
    (
        Vec3::from_array(s.transform.position),
        Vec3::from_array(s.velocity),
        Quat::from_array(s.transform.rotation),
        Vec3::from_array(s.angular_velocity),
    )
}

fn throttle() -> Controls {
    Controls {
        throttle: 1.,
        ..Default::default()
    }
}

fn zero() -> Controls {
    Controls::default()
}

/// The flying car's top speed under thrust (`max_forward_vel`).
fn top_speed(f: &Fixture) -> f32 {
    f.definition(f.flying_car)
        .wheeled_flight
        .as_ref()
        .expect("flies")
        .max_forward_vel
}

on_both! {
fn takes_off_from_speed_and_climbs_on_mouse_down(f: &Fixture) {
    let (mut v, mut w) = flying_jeep(f, 3., 0.);
    fly(&mut v, &mut w, 720, throttle());
    let (p, vel, rot, _) = state(&v, &w);
    println!("runway {p} {vel}");
    // Thrust stops at `max_forward_vel`, where the lift (`lift` × speed,
    // capped at 4000) carries about the weight.
    let top = top_speed(f);
    let d = f.definition(f.flying_car);
    let lift = (d.lift * top).min(4000.);
    let weight = d.mass * VEHICLE_GRAVITY;
    assert!((lift - weight).abs() <= weight * 0.1, "lift {lift}, weight {weight}");
    assert!(
        vel.length() > top * 0.9 && vel.length() < top * 1.05,
        "speed {vel}"
    );
    assert!((rot * Vec3::Y).y > 0.97, "stays level on the runway");
    // Mouse down, with v20's default vehicle mouse invert, raises the nose.
    // Steering returns on every move without a mouse turn while the
    // throttle is held, so the pull must keep going.
    fly(
        &mut v,
        &mut w,
        240,
        Controls {
            look_delta: [0., -0.01],
            ..throttle()
        },
    );
    let (q, vel, _, _) = state(&v, &w);
    println!("climb {q} {vel}");
    assert!(q.y > p.y + 8., "pulled up from {p} to {q}");
}
}

on_both! {
fn holds_level_flight_at_full_speed(f: &Fixture) {
    let (mut v, mut w) = flying_jeep(f, 100., top_speed(f));
    fly(&mut v, &mut w, 240, throttle());
    let (p, vel, _, _) = state(&v, &w);
    println!("level {p} {vel}");
    assert!((p.y - 100.).abs() < 4., "level flight drifted to {p}");
    assert!(p.z < -top_speed(f) * 1.75, "flew forward, {p}");
}
}

on_both! {
fn mouse_turns_right_and_strafe_rolls_right(f: &Fixture) {
    let (mut v, mut w) = flying_jeep(f, 100., top_speed(f));
    fly(
        &mut v,
        &mut w,
        120,
        Controls {
            look_delta: [0.01, 0.],
            ..throttle()
        },
    );
    let (p, vel, rot, spin) = state(&v, &w);
    println!("turn {p} {vel} {spin} nose {}", rot * Vec3::NEG_Z);
    assert!((rot * Vec3::NEG_Z).x > 0.1, "mouse right turns right");
    let (mut v, mut w) = flying_jeep(f, 100., top_speed(f));
    fly(
        &mut v,
        &mut w,
        60,
        Controls {
            strafe: 1.,
            ..throttle()
        },
    );
    let (_, _, rot, _) = state(&v, &w);
    println!("roll up {}", rot * Vec3::Y);
    assert!((rot * Vec3::Y).x > 0.1, "D rolls the right side down");
}
}

on_both! {
fn controls_do_nothing_below_stall_speed(f: &Fixture) {
    let (mut v, mut w) = flying_jeep(f, 100., 0.);
    fly(
        &mut v,
        &mut w,
        12,
        Controls {
            look_delta: [0.08, 0.08],
            strafe: 1.,
            ..zero()
        },
    );
    fly(&mut v, &mut w, 108, zero());
    let (p, vel, rot, spin) = state(&v, &w);
    println!("stall {p} {vel} {spin}");
    assert!(spin.length() < 0.05, "stalled controls turned it, {spin}");
    assert!((rot * Vec3::Y).y > 0.99);
    assert!(p.y < 92., "falls, {p}");
}
}

on_both! {
fn glides_down_and_lands_on_its_wheels(f: &Fixture) {
    let (mut v, mut w) = flying_jeep(f, 12., 20.);
    fly(&mut v, &mut w, 480, zero());
    let s = &v.snapshot(&w).vehicles[0];
    let rot = Quat::from_array(s.transform.rotation);
    println!("landed {:?} {:?}", s.transform.position, s.wheel_suspension);
    assert!((rot * Vec3::Y).y > 0.98, "landed upright");
    assert!(s.transform.position[1] < 3., "on the ground");
}
}
