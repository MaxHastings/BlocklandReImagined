//! A vehicle that names its acceleration and braking speeds up and stops at
//! those rates (measured on flat ground), and can be checked against a real
//! Add-On's numbers.
#[allow(unused_macros)]
mod common;
use bri_vehicles::{schema::DriveRates, *};
use glam::Vec3;
use rapier3d::prelude::*;

const DRIVER: OccupantId = OccupantId(20);

/// Halo's Warthog tag: 10.4 u/s^2 up to 34 u/s, braking at 40.8.
const TAG: DriveRates = DriveRates {
    acceleration: 10.38,
    deceleration: 40.8,
};

fn drive(definition: Definition) -> (VehiclesWorld, PhysicsWorld, String) {
    let id = definition.id.clone();
    let mut pack = bri_vehicles::testing::pack();
    pack.definitions.retain(|d| d.id != id);
    pack.definitions.push(definition);
    let mut v = VehiclesWorld::new(pack).unwrap();
    let mut w = common::floor(1000.);
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: id.clone(),
            transform: Transform {
                position: [0., 1.5, 0.],
                ..Default::default()
            },
            spawn_id: Some(SpawnId(4)),
            respawn_ticks: Some(600),
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
    let mut me = (v, w, id);
    step(&mut me.0, &mut me.1, 90, Controls::default(), false);
    let seat = me.0.snapshot(&me.1).vehicles[0].seats[0].transform.position;
    me.0.mount(
        &me.1,
        VehicleId(1),
        0,
        Occupant {
            id: DRIVER,
            owner: OwnerId(10),
            body: [1.25, 2.65],
        },
        seat,
    )
    .unwrap();
    me
}
fn step(v: &mut VehiclesWorld, w: &mut PhysicsWorld, n: usize, c: Controls, driven: bool) {
    if driven {
        v.set_controls(OwnerId(10), DRIVER, c).unwrap();
    }
    for _ in 0..n {
        v.pre_step(w, &[]).unwrap();
        w.step();
        v.post_step(w).unwrap();
    }
}
fn speed(v: &VehiclesWorld, w: &PhysicsWorld) -> f32 {
    let s = v.snapshot(w).vehicles[0].clone();
    let forward = glam::Quat::from_array(s.transform.rotation) * Vec3::NEG_Z;
    Vec3::from_array(s.velocity).dot(forward)
}
fn position(v: &VehiclesWorld, w: &PhysicsWorld) -> f32 {
    -v.snapshot(w).vehicles[0].transform.position[2]
}

/// Seconds from rest to `target` at full throttle.
fn time_to(definition: Definition, target: f32) -> f32 {
    let (mut v, mut w, _) = drive(definition);
    let throttle = Controls {
        throttle: 1.,
        ..Default::default()
    };
    for tick in 0..1200 {
        step(&mut v, &mut w, 1, throttle, true);
        if speed(&v, &w) >= target {
            return tick as f32 / 120.;
        }
    }
    f32::INFINITY
}
/// Distance a vehicle at `from` takes to stop with the brake held, and
/// the time that took.
fn stop_from(definition: Definition, from: f32) -> (f32, f32) {
    let (mut v, mut w, _) = drive(definition);
    let throttle = Controls {
        throttle: 1.,
        ..Default::default()
    };
    while speed(&v, &w) < from {
        step(&mut v, &mut w, 1, throttle, true);
    }
    let brake = Controls {
        brake: true,
        ..Default::default()
    };
    let start = position(&v, &w);
    for tick in 1..1200 {
        step(&mut v, &mut w, 1, brake, true);
        if speed(&v, &w) < 0.3 {
            return (position(&v, &w) - start, tick as f32 / 120.);
        }
    }
    (f32::INFINITY, f32::INFINITY)
}
/// A grippy four-wheel-drive car with the tag's rates.
fn tagged() -> Definition {
    let mut d = bri_vehicles::testing::car();
    d.id = "test:vehicle/tagged".into();
    d.max_speed = 34.;
    d.mass = 900.;
    d.engine_curve = vec![[0., 1.], [0.9, 1.], [1., 0.]];
    // The Warthog's wheels and tyres, carrying a mass of 900.
    for w in &mut d.wheels {
        w.powered = true;
        w.radius = 0.824;
        w.rest_length = 0.62;
        w.spring = 10300.;
        w.damping = 2600.;
        w.rebound_damping = 2400.;
        w.anti_sway = 6000.;
        w.tire = bri_vehicles::schema::Tire {
            static_friction: 3.,
            kinetic_friction: 2.25,
            lateral_grip: 0.6,
            lateral_force: 42000.,
            lateral_damping: 9000.,
            lateral_relaxation: 0.02,
            longitudinal_force: 40000.,
            longitudinal_damping: 5000.,
            longitudinal_relaxation: 0.02,
        };
    }
    d.drive = Some(TAG);
    d
}

#[test]
fn rates_become_forces_for_the_wheels_and_mass() {
    let mut d = tagged();
    let r = d.wheels[0].radius;
    d.resolve_drive();
    let expect = TAG.acceleration * d.mass * (1. + r * r) / r / 4.;
    assert!((d.engine_force - expect).abs() < 1e-2 * expect);
    // No powered wheel, no change.
    let mut still = tagged();
    still.wheels.iter_mut().for_each(|w| w.powered = false);
    still.engine_force = 123.;
    still.resolve_drive();
    assert_eq!(still.engine_force, 123.);
}
#[test]
fn it_reaches_speed_at_the_authored_acceleration() {
    // Flat out to 30 (under the 0.9 * 34 where the engine tapers): the
    // authored 10.38 u/s^2 takes 2.9 s, less a little drag.
    let seconds = time_to(tagged(), 30.);
    let ideal = 30. / TAG.acceleration;
    assert!(
        (seconds - ideal).abs() < 0.2 * ideal,
        "{seconds} s against {ideal} s"
    );
}
#[test]
fn it_brakes_at_the_authored_deceleration() {
    let (distance, seconds) = stop_from(tagged(), 28.);
    let (ideal_time, ideal_distance) = (28. / TAG.deceleration, 28. * 28. / (2. * TAG.deceleration));
    assert!(
        (seconds - ideal_time).abs() < 0.3 * ideal_time
            && (distance - ideal_distance).abs() < 0.3 * ideal_distance,
        "{seconds} s over {distance} against {ideal_time} s over {ideal_distance}"
    );
}

/// With `HALO_WARTHOG_VEHICLES` naming the private Add-On's vehicles.json,
/// the real Warthog's 0 to top speed time and braking distance, against the
/// tag it says it takes them from. Skipped when unset.
#[test]
fn the_private_warthog_meets_its_tag() {
    let Some(path) = std::env::var_os("HALO_WARTHOG_VEHICLES") else {
        return;
    };
    let pack: Pack = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut d = pack.definitions[0].clone();
    d.id = "test:vehicle/warthog".into();
    let rates = d.drive.expect("the Warthog authors its drive rates");
    // The Warthog tapers off near top speed: time the part before 0.9.
    let target = 0.85 * d.max_speed;
    let seconds = time_to(d.clone(), target);
    let ideal = target / rates.acceleration;
    println!("0 to {target:.1} u/s in {seconds:.2} s; the tag's rate gives {ideal:.2} s");
    let (distance, stop) = stop_from(d.clone(), 0.8 * d.max_speed);
    let ideal_distance = (0.8 * d.max_speed).powi(2) / (2. * rates.deceleration);
    println!(
        "braking from {:.1}: {distance:.1} units in {stop:.2} s; the tag's rate gives {ideal_distance:.1}",
        0.8 * d.max_speed
    );
    assert!(seconds.is_finite() && (seconds - ideal).abs() < 0.3 * ideal);
    assert!(distance.is_finite());
}
