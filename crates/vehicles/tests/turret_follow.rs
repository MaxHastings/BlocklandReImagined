//! A weapon with aim limits swings onto its gunner's aim instead of
//! snapping to it, and shoots where it points.
#[allow(unused_macros)]
mod common;
use bri_vehicles::{aim::AimLimits, *};
use common::Fixture;
use glam::Vec3;
use rapier3d::prelude::*;

/// Halo's Warthog: 180 degrees a second, 600 degrees a second squared.
const LIMITS: AimLimits = AimLimits {
    max_velocity: std::f32::consts::PI,
    max_acceleration: 10.471_976,
};
const GUNNER: OccupantId = OccupantId(22);

/// A tank whose turret follows its gunner within `limits`, gunner aboard.
fn tank(limits: Option<AimLimits>) -> (VehiclesWorld, PhysicsWorld) {
    let mut f = Fixture::synthetic();
    for d in &mut f.pack.definitions {
        if d.id == f.tank {
            d.weapon.as_mut().unwrap().aim_limits = limits;
            d.look_pitch = [-0.261_799_4, 0.610_865_2];
        }
    }
    let mut v = f.vehicles();
    let mut w = common::floor(500.);
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: f.tank.into(),
            transform: Transform {
                position: [0., 3., 0.],
                ..Default::default()
            },
            spawn_id: Some(SpawnId(4)),
            respawn_ticks: Some(600),
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
    step(&mut v, &mut w, 30);
    let seat = v.snapshot(&w).vehicles[0].seats[2].transform.position;
    v.mount(
        &w,
        VehicleId(1),
        2,
        Occupant {
            id: GUNNER,
            owner: OwnerId(10),
            body: [1.25, 2.65],
        },
        seat,
    )
    .unwrap();
    v.drain_intents();
    (v, w)
}
fn step(v: &mut VehiclesWorld, w: &mut PhysicsWorld, n: usize) {
    for _ in 0..n {
        v.pre_step(w, &[]).unwrap();
        w.step();
        v.post_step(w).unwrap();
    }
}
fn aim_at(v: &mut VehiclesWorld, yaw: f32, pitch: f32, fire: bool) {
    v.set_controls(
        OwnerId(10),
        GUNNER,
        Controls {
            aim_yaw: yaw,
            aim_pitch: pitch,
            fire,
            ..Default::default()
        },
    )
    .unwrap();
}
fn barrel(v: &VehiclesWorld, w: &PhysicsWorld) -> [f32; 2] {
    v.vehicle_snapshot(w, VehicleId(1)).unwrap().turret_aim
}

#[test]
fn the_barrel_lags_its_gunner_and_arrives() {
    let (mut v, mut w) = tank(Some(LIMITS));
    aim_at(&mut v, 2.0, 0.3, false);
    step(&mut v, &mut w, 60);
    let half_second = barrel(&v, &w);
    // A barrel at the speed limit from the first tick would be at 1.57.
    assert!(half_second[0] > 0.05 && half_second[0] < 1.2, "{half_second:?}");
    step(&mut v, &mut w, 600);
    let there = barrel(&v, &w);
    assert!((there[0] - 2.0).abs() < 1e-3 && (there[1] - 0.3).abs() < 1e-3, "{there:?}");
}
#[test]
fn without_limits_the_barrel_is_on_the_aim_at_once() {
    let (mut v, mut w) = tank(None);
    aim_at(&mut v, 2.0, 0.3, false);
    step(&mut v, &mut w, 2);
    let aim = barrel(&v, &w);
    assert!((aim[0] - 2.0).abs() < 1e-6 && (aim[1] - 0.3).abs() < 1e-6);
}
#[test]
fn a_new_gunner_takes_the_barrel_where_it_points() {
    let (mut v, mut w) = tank(Some(LIMITS));
    aim_at(&mut v, 2.0, 0.0, false);
    step(&mut v, &mut w, 20);
    let moving = barrel(&v, &w);
    // The turret keeps swinging toward the aim its gunner left it with.
    v.dismount(&w, OwnerId(10), GUNNER, false).unwrap();
    step(&mut v, &mut w, 600);
    let left = barrel(&v, &w);
    assert!(left[0] > moving[0] && (left[0] - 2.0).abs() < 1e-3, "{moving:?} {left:?}");
}
#[test]
fn shots_leave_along_the_barrel_not_the_look() {
    let (mut v, mut w) = tank(Some(LIMITS));
    aim_at(&mut v, 1.5, 0.0, true);
    step(&mut v, &mut w, 2);
    let shots: Vec<_> = v
        .drain_intents()
        .into_iter()
        .filter_map(|i| match i {
            Intent::Fire(f) => Some(f),
            _ => None,
        })
        .collect();
    assert_eq!(shots.len(), 1);
    // The look is 86 degrees round; the barrel has hardly left the nose.
    let direction = Vec3::from_array(shots[0].direction);
    assert!(direction.dot(Vec3::NEG_Z) > 0.95, "{direction:?}");
}
#[test]
fn the_gunners_seat_follows_the_barrel() {
    let (mut v, mut w) = tank(Some(LIMITS));
    aim_at(&mut v, 3.0, 0.0, false);
    let seat = |v: &VehiclesWorld, w: &PhysicsWorld| {
        Vec3::from_array(v.snapshot(w).vehicles[0].seats[2].transform.position)
    };
    let start = seat(&v, &w);
    step(&mut v, &mut w, 12);
    let early = seat(&v, &w);
    step(&mut v, &mut w, 600);
    let late = seat(&v, &w);
    // Turning about the mount, the seat moves little at first and far by
    // the end: it is carried with the barrel, not placed at the aim.
    assert!((early - start).length() < (late - start).length() * 0.5);
}
