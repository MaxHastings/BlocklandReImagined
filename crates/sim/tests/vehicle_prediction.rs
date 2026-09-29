//! A client predicting the vehicle it drives, against the host running the
//! same moves: the Flying Wheeled Jeep in the air, pitched up and down by a
//! scripted mouse. The host's poses reach the client late and only every few
//! ticks, as on a real connection.
use bri_sim::{
    definitions::Definitions,
    player::{MoveInput, PlayerState},
    prediction::{CollisionMirror, DriveSpawn, Predictor},
    session::driver_controls,
};
use bri_vehicles::*;
use glam::{Quat, Vec3};
use rapier3d::prelude::*;
use std::collections::VecDeque;

const JEEP: &str = "v20.vehicle.flyingwheeledjeepvehicle";
/// A round trip of 100 ms.
const DELAY: u64 = 12;
/// A pose every third tick.
const POSE_EVERY: u64 = 3;

fn pack() -> Pack {
    Pack::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/vehicles-pack-011/vehicles.json"
    ))
    .unwrap()
}
fn ground() -> ColliderBuilder {
    ColliderBuilder::cuboid(2000., 0.5, 2000.).translation(Vector::new(0., -0.5, 0.))
}
fn occupant() -> Occupant {
    Occupant {
        id: OccupantId(20),
        owner: OwnerId(10),
        body: [1.25, 2.65],
    }
}
fn spawn() -> Spawn {
    Spawn {
        scale: 1.,
        id: VehicleId(1),
        owner: OwnerId(10),
        definition: JEEP.into(),
        transform: Transform {
            position: [0., 120., 0.],
            ..Default::default()
        },
        spawn_id: None,
        respawn_ticks: None,
    }
}
/// The host: the jeep flying at 45 toward its nose, its driver seated.
fn host() -> (VehiclesWorld, PhysicsWorld) {
    let mut v = VehiclesWorld::new(pack()).unwrap();
    let mut w = bri_physics::new_world();
    w.insert(RigidBodyBuilder::fixed(), ground());
    v.spawn(&mut w, spawn()).unwrap();
    w.detect_collisions(&(), &());
    let seat = v.seat_position(&w, VehicleId(1), 0).unwrap();
    v.mount(&w, VehicleId(1), 0, occupant(), seat).unwrap();
    let (_, b) = w.bodies.iter_mut().find(|(_, b)| b.is_dynamic()).unwrap();
    b.set_linvel(Vec3::new(0., 0., -45.), true);
    v.drain_intents();
    (v, w)
}
fn motion(v: &VehiclesWorld, w: &PhysicsWorld) -> Motion {
    let s = &v.snapshot(w).vehicles[0];
    Motion {
        transform: s.transform.clone(),
        velocity: s.velocity,
        angular_velocity: s.angular_velocity,
        mouse_steering: s.mouse_steering,
        steering: s.steering,
        wheel_suspension: s.wheel_suspension.clone(),
        wheel_rotation: s.wheel_rotation.clone(),
        wheel_contact: s.wheel_contact.clone(),
    }
}
/// Full throttle; the mouse pushes the nose one way, then the other, then
/// rests, as a player pulling up into a loop and correcting would.
fn input(tick: u64) -> MoveInput {
    let pitch = match tick {
        0..=40 => -0.012 * tick as f32,
        41..=90 => -0.48 + 0.02 * (tick - 40) as f32,
        _ => 0.52,
    };
    MoveInput {
        forward: 1.0,
        pitch,
        yaw: 0.004 * tick as f32,
        ..Default::default()
    }
}
fn nose(t: &Transform) -> f32 {
    (Quat::from_array(t.rotation) * Vec3::NEG_Z).y.asin()
}

#[test]
fn a_driven_vehicle_answers_its_own_mouse_at_once_and_agrees_with_the_host() {
    let (mut v, mut w) = host();
    let mirror = CollisionMirror::new(Definitions::default(), vec![ground()], vec![]);
    let rider = PlayerState {
        owner: 10,
        feet: v.seat_position(&w, VehicleId(1), 0).unwrap(),
        velocity: [0.0; 3],
        yaw: 0.0,
        pitch: 0.0,
        head_yaw: 0.0,
        grounded: false,
        crouched: false,
        jetting: false,
        jump: Default::default(),
        archetype: Default::default(),
        scale: 1.0,
        energy: 100.0,
        tick: Default::default(),
    };
    let mut client = Predictor::new(mirror, rider, Default::default()).unwrap();
    client
        .drive(Some((
            pack(),
            DriveSpawn {
                spawn: spawn(),
                seat: 0,
                occupant: occupant(),
                prefs: (false, false),
            },
            motion(&v, &w),
        )))
        .unwrap();
    // What the host shows after each of the driver's inputs, and the poses
    // on their way to the client.
    let mut host_after = vec![motion(&v, &w).transform];
    let mut in_flight: VecDeque<(u64, u64, Motion)> = VecDeque::new();
    let mut last = input(0);
    let mut worst = 0.0_f32;
    for tick in 1..=150_u64 {
        let sequence = client.record(input(tick)).unwrap();
        assert_eq!(sequence, tick);
        // The host runs the same move this tick.
        let controls = driver_controls(&input(tick), (last.yaw, last.pitch), false, (false, false));
        last = input(tick);
        v.set_controls(OwnerId(10), OccupantId(20), controls).unwrap();
        v.pre_step(&mut w, &[]).unwrap();
        w.step();
        v.post_step(&mut w).unwrap();
        v.drain_intents();
        host_after.push(motion(&v, &w).transform);
        if tick % POSE_EVERY == 0 {
            in_flight.push_back((tick + DELAY, tick, motion(&v, &w)));
        }
        while in_flight.front().is_some_and(|(arrive, ..)| *arrive <= tick) {
            let (_, at, pose) = in_flight.pop_front().unwrap();
            client.drive_pose(at, at, &pose).unwrap();
        }
        let (_, _, predicted) = client.driven().unwrap();
        let host = &host_after[tick as usize];
        let apart = Vec3::from(predicted.position).distance(Vec3::from(host.position));
        let turned = Quat::from_array(predicted.rotation).angle_between(Quat::from_array(host.rotation));
        worst = worst.max(apart);
        assert!(
            apart < 0.05 && turned < 0.01,
            "tick {tick}: predicted {predicted:?} vs host {host:?}"
        );
    }
    // The mouse moved the nose while the host's poses were still a round
    // trip behind: the prediction answered on the tick of the input.
    let (_, _, now) = client.driven().unwrap();
    assert!(
        (nose(now) - nose(&host_after[150 - DELAY as usize])).abs() > 0.01,
        "the prediction should lead the delayed poses"
    );
    println!("worst prediction error {worst}");
}
