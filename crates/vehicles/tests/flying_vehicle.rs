//! FlyingVehicle (the Magic Carpet) against blocklandv20.exe
//! `FlyingVehicle::updateForces` (0x568770, stock Torque): its forces act
//! along its own axes, so it goes where its nose points. Also the schema 5
//! pack upgrade that types the wheeled flying and steering fields.
#[macro_use]
mod common;
use bri_vehicles::*;
use common::Fixture;
use glam::{Quat, Vec3};
use rapier3d::prelude::*;

#[test]
#[ignore = "requires generated v20 content"]
fn a_schema_5_pack_loads_with_typed_flight_and_steering() {
    let p = common::content_pack();
    assert_eq!(p.schema_version, schema::SCHEMA_VERSION);
    let d = |name: &str| {
        p.definitions
            .iter()
            .find(|d| d.datablock == name)
            .unwrap()
            .clone()
    };
    let jeep = d("FlyingWheeledJeepVehicle");
    assert_eq!(jeep.family, Family::Wheeled);
    let f = jeep.wheeled_flight.as_ref().expect("flies");
    assert_eq!(
        (
            f.max_forward_vel,
            f.max_reverse_vel,
            f.horizontal_surface_force,
            f.vertical_surface_force,
            f.stall_speed,
            f.sled
        ),
        (40., 40., 130., 130., 10., false)
    );
    assert!(d("JeepVehicle").wheeled_flight.is_none());
    assert!(d("skiVehicle").wheeled_flight.as_ref().unwrap().sled);
    assert!(d("MagicCarpetVehicle").wheeled_flight.is_none());
    let s = &d("JeepVehicle").steering;
    assert_eq!(
        (
            s.strafe_rate,
            s.auto_return,
            s.auto_return_rate,
            s.auto_return_max_speed
        ),
        (0.1, true, 0.9, 10.)
    );
    assert!(p.definitions.iter().all(|d| d.threads.is_empty()));
}

/// A mounted carpet high above the floor with its nose `pitch` radians
/// up, moving `speed` along the nose.
fn carpet(f: &Fixture, pitch: f32, speed: f32) -> (VehiclesWorld, PhysicsWorld) {
    let mut v = f.vehicles();
    let mut w = common::floor(2000.);
    let rotation = Quat::from_rotation_x(pitch);
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: f.carpet.into(),
            transform: Transform {
                position: [0., 100., 0.],
                rotation: rotation.to_array(),
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
    b.set_linvel(rotation * Vec3::new(0., 0., -speed), true);
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

fn state(v: &VehiclesWorld, w: &PhysicsWorld) -> (Vec3, Vec3, Vec3) {
    let s = &v.snapshot(w).vehicles[0];
    (
        Vec3::from(s.transform.position),
        Vec3::from(s.velocity),
        Quat::from_array(s.transform.rotation) * Vec3::NEG_Z,
    )
}

on_both! {
fn the_carpet_climbs_and_dives_where_its_nose_points(f: &Fixture) {
    let forward = Controls {
        throttle: 1.,
        ..Default::default()
    };
    let mut heights = vec![];
    for pitch in [0.4, 0., -0.4] {
        let (mut v, mut w) = carpet(f, pitch, 20.);
        fly(&mut v, &mut w, 120, forward);
        let (p, velocity, nose) = state(&v, &w);
        // The thrust and the lift both turn with the carpet: its path
        // follows the nose rather than staying level.
        assert!(
            velocity.normalize().dot(nose) > 0.8,
            "pitch {pitch}: flies along its nose, velocity {velocity} nose {nose}"
        );
        heights.push(p.y);
    }
    eprintln!("heights after 1 s nose up, level, nose down: {heights:?}");
    assert!(
        heights[0] > heights[1] + 5. && heights[1] > heights[2] + 5.,
        "nose up climbs and nose down dives: {heights:?}"
    );
}
}

on_both! {
fn mouse_pitch_tips_the_carpets_nose_about_its_own_wing(f: &Fixture) {
    // Rolled on its side, pitching still turns the nose about the carpet's
    // own wing, not the world's horizontal.
    for roll in [0., 0.6] {
        let (mut v, mut w) = carpet(f, 0., 30.);
        let (_, b) = w.bodies.iter_mut().find(|(_, b)| b.is_dynamic()).unwrap();
        b.set_rotation(Quat::from_rotation_z(roll), true);
        let (_, _, before) = state(&v, &w);
        let wing = Quat::from_rotation_z(roll) * Vec3::X;
        // Mouse down raises the nose with v20's default vehicle mouse invert.
        fly(
            &mut v,
            &mut w,
            30,
            Controls {
                look_delta: [0., -0.02],
                ..Default::default()
            },
        );
        let (_, _, after) = state(&v, &w);
        let turn = before.cross(after);
        eprintln!("roll {roll}: nose turned about {turn}");
        assert!(
            turn.length() > 0.02 && turn.normalize().dot(wing) > 0.9,
            "roll {roll}: nose turned about {turn}, wing {wing}"
        );
    }
}
}
