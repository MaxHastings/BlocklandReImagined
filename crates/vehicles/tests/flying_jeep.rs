//! The Flying Wheeled Jeep against blocklandv20.exe `WheeledVehicle::updateForces`:
//! thrust below `maxForwardVel`, lift from speed along the nose, control
//! surfaces that bite only above `stallSpeed`, and no jets.
use bri_vehicles::*;
use glam::{Quat, Vec3};
use rapier3d::prelude::*;

/// A mounted Flying Wheeled Jeep at `height`, moving `speed` toward its nose.
fn flying_jeep(height: f32, speed: f32) -> (VehiclesWorld, PhysicsWorld) {
    let pack = Pack::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/vehicles-pack-012/vehicles.json"
    ))
    .unwrap();
    let mut v = VehiclesWorld::new(pack).unwrap();
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(2000., 0.5, 2000.),
    );
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: "v20.vehicle.flyingwheeledjeepvehicle".into(),
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

#[test]
fn takes_off_from_speed_and_climbs_on_mouse_down() {
    let (mut v, mut w) = flying_jeep(3., 0.);
    fly(&mut v, &mut w, 720, throttle());
    let (p, vel, rot, _) = state(&v, &w);
    println!("runway {p} {vel}");
    // Thrust stops at maxForwardVel 40, where lift (100 × speed, capped at
    // 4000) carries the jeep's 200 × 20 weight.
    assert!(vel.length() > 36. && vel.length() < 42., "speed {vel}");
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

#[test]
fn holds_level_flight_at_full_speed() {
    let (mut v, mut w) = flying_jeep(100., 40.);
    fly(&mut v, &mut w, 240, throttle());
    let (p, vel, _, _) = state(&v, &w);
    println!("level {p} {vel}");
    assert!((p.y - 100.).abs() < 4., "level flight drifted to {p}");
    assert!(p.z < -70., "flew forward, {p}");
}

#[test]
fn mouse_turns_right_and_strafe_rolls_right() {
    let (mut v, mut w) = flying_jeep(100., 40.);
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
    let (mut v, mut w) = flying_jeep(100., 40.);
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

#[test]
fn controls_do_nothing_below_stall_speed() {
    let (mut v, mut w) = flying_jeep(100., 0.);
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

#[test]
fn glides_down_and_lands_on_its_wheels() {
    let (mut v, mut w) = flying_jeep(12., 20.);
    fly(&mut v, &mut w, 480, zero());
    let s = &v.snapshot(&w).vehicles[0];
    let rot = Quat::from_array(s.transform.rotation);
    println!("landed {:?} {:?}", s.transform.position, s.wheel_suspension);
    assert!((rot * Vec3::Y).y > 0.98, "landed upright");
    assert!(s.transform.position[1] < 3., "on the ground");
}
