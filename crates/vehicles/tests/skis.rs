//! Skis against v20's `skiVehicle` (Item_Skis) and the WheeledVehicle
//! forces decoded from blocklandv20.exe; see docs/audits/skis-v20.md.
#[macro_use]
mod common;
use bri_vehicles::*;
use common::Fixture;
use glam::{Quat, Vec3};
use rapier3d::prelude::*;

/// A 4 km ground plane tilted `slope` radians down towards -Z, plus an
/// optional wall across the track at `wall_z`.
fn world(f: &Fixture, slope: f32, wall_z: Option<f32>) -> (VehiclesWorld, PhysicsWorld) {
    let v = f.vehicles();
    let mut w = bri_physics::new_world();
    let tilt = Quat::from_rotation_x(-slope);
    w.insert(
        RigidBodyBuilder::fixed().pose(Pose::from_parts(tilt * Vec3::new(0., -0.5, 0.), tilt)),
        ColliderBuilder::cuboid(2000., 0.5, 2000.),
    );
    if let Some(z) = wall_z {
        w.insert(
            RigidBodyBuilder::fixed().translation(Vec3::new(0., 2., z)),
            ColliderBuilder::cuboid(20., 4., 0.5),
        );
    }
    (v, w)
}
fn ride(f: &Fixture, v: &mut VehiclesWorld, w: &mut PhysicsWorld, position: Vec3, rotation: Quat) {
    v.spawn(
        w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: f.skis.into(),
            transform: Transform {
                position: position.to_array(),
                rotation: rotation.to_array(),
            },
            spawn_id: None,
            respawn_ticks: None,
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
    let seat = v.seat_position(w, VehicleId(1), 0).unwrap();
    v.mount(
        w,
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
}
fn drive(v: &mut VehiclesWorld, controls: Controls) {
    v.set_controls(OwnerId(10), OccupantId(20), controls)
        .unwrap();
}
fn step(v: &mut VehiclesWorld, w: &mut PhysicsWorld, n: usize) -> Vec<Intent> {
    let mut intents = vec![];
    for _ in 0..n {
        v.pre_step(w, &[]).unwrap();
        w.step();
        v.post_step(w).unwrap();
        intents.extend(v.drain_intents());
    }
    intents
}
fn state(v: &VehiclesWorld, w: &PhysicsWorld) -> Option<VehicleSnapshot> {
    v.snapshot(w).vehicles.into_iter().next()
}
/// The sound the skis' datablock plays for a hard hit.
fn hard_sound(f: &Fixture) -> String {
    f.definition(f.skis).authored["hardimpactsound"].clone()
}
fn wrecked(intents: &[Intent]) -> bool {
    intents
        .iter()
        .any(|i| matches!(i, Intent::TumbleRequested { .. }))
}

on_both! {
fn skis_push_to_top_speed_along_the_nose_on_flat_ground(f: &Fixture) {
    let (mut v, mut w) = world(f, 0., None);
    ride(f, &mut v, &mut w, Vec3::new(0., 1., 0.), Quat::IDENTITY);
    step(&mut v, &mut w, 120);
    drive(
        &mut v,
        Controls {
            throttle: 1.,
            ..Default::default()
        },
    );
    let intents = step(&mut v, &mut w, 5 * 120);
    let early = Vec3::from(state(&v, &w).unwrap().velocity);
    let intents = [intents, step(&mut v, &mut w, 15 * 120)].concat();
    let velocity = Vec3::from(state(&v, &w).unwrap().velocity);
    eprintln!("flat throttle: 5 s {early:?}, 20 s {velocity:?}");
    assert!(!wrecked(&intents));
    // The thrust on the mass, less the body friction on the part of the
    // weight the springs leave to the hull.
    let d = f.definition(f.skis);
    let free = d.thrust / d.mass * 5.;
    assert!((0.3 * free..free).contains(&-early.z), "{early:?}");
    // Thrust stops at `max_forward_vel`.
    let top = d.wheeled_flight.as_ref().unwrap().max_forward_vel;
    assert!((0.9 * top..top + 0.5).contains(&velocity.length()), "{velocity:?}");
    assert!(velocity.x.abs() < 0.5 && velocity.y.abs() < 0.5);
}
}

on_both! {
fn skis_slide_downhill_on_frictionless_tires(f: &Fixture) {
    let (mut v, mut w) = world(f, 25f32.to_radians(), None);
    let tilt = Quat::from_rotation_x(-25f32.to_radians());
    ride(f, &mut v, &mut w, tilt * Vec3::new(0., 1., 0.), tilt);
    let intents = step(&mut v, &mut w, 4 * 120);
    let s = state(&v, &w).unwrap();
    let velocity = Vec3::from(s.velocity);
    eprintln!("25 degree slope 4 s coasting: {velocity:?}");
    assert!(!wrecked(&intents));
    assert!(velocity.z < -10., "{velocity:?}");
}
}

on_both! {
fn ski_grip_turns_sideways_slide_into_the_nose_only_on_the_ground(f: &Fixture) {
    // Skis facing -Z moving sideways along +X: on the ground the sled
    // surface force (horizontal_surface_force × speed) kills the slide.
    let (mut v, mut w) = world(f, 0., None);
    ride(f, &mut v, &mut w, Vec3::new(0., 1., 0.), Quat::IDENTITY);
    step(&mut v, &mut w, 120);
    v.set_velocity(&mut w, VehicleId(1), [20., 0., -20.])
        .unwrap();
    step(&mut v, &mut w, 60);
    let ground = Vec3::from(state(&v, &w).unwrap().velocity);
    // High in the air nothing resists the slide.
    let (mut v, mut w) = world(f, 0., None);
    ride(f, &mut v, &mut w, Vec3::new(0., 200., 0.), Quat::IDENTITY);
    v.set_velocity(&mut w, VehicleId(1), [20., 0., -20.])
        .unwrap();
    step(&mut v, &mut w, 60);
    let air = Vec3::from(state(&v, &w).unwrap().velocity);
    eprintln!("sideways after 0.5 s: ground {ground:?} air {air:?}");
    assert!(ground.x.abs() < 4., "{ground:?}");
    assert!(air.x > 18., "{air:?}");
}
}

on_both! {
fn mouse_steering_turns_moving_skis_but_not_standing_ones(f: &Fixture) {
    let heading = |speed: f32| {
        let (mut v, mut w) = world(f, 0., None);
        ride(f, &mut v, &mut w, Vec3::new(0., 1., 0.), Quat::IDENTITY);
        step(&mut v, &mut w, 120);
        v.set_velocity(&mut w, VehicleId(1), [0., 0., -speed])
            .unwrap();
        drive(
            &mut v,
            Controls {
                look_delta: [f.definition(f.skis).max_steering, 0.],
                ..Default::default()
            },
        );
        step(&mut v, &mut w, 1);
        drive(&mut v, Controls::default());
        let intents = step(&mut v, &mut w, 120);
        assert!(!wrecked(&intents));
        let r = Quat::from_array(state(&v, &w).unwrap().transform.rotation);
        let forward = r * -Vec3::Z;
        forward.x.atan2(-forward.z).to_degrees()
    };
    let (standing, moving) = (heading(0.), heading(25.));
    eprintln!("heading after 1 s full right: standing {standing} moving {moving}");
    // No speed, no bite: the yaw torque scales with speed / max_forward_vel.
    assert!(standing.abs() < 2., "{standing}");
    assert!(moving > 20., "{moving}");
}
}

on_both! {
fn skis_wreck_when_the_body_lands_without_the_skis_under_it(f: &Fixture) {
    // Upside down, the body lands first with every ski in the air.
    let (mut v, mut w) = world(f, 0., None);
    ride(
        f,
        &mut v,
        &mut w,
        Vec3::new(0., 6., 0.),
        Quat::from_rotation_z(std::f32::consts::PI),
    );
    let intents = step(&mut v, &mut w, 240);
    assert!(wrecked(&intents));
    assert!(state(&v, &w).is_none());
}
}

on_both! {
fn a_hard_landing_on_the_skis_is_not_a_wreck(f: &Fixture) {
    let (mut v, mut w) = world(f, 0., None);
    ride(f, &mut v, &mut w, Vec3::new(0., 15., 0.), Quat::IDENTITY);
    let intents = step(&mut v, &mut w, 360);
    let puffs = intents
        .iter()
        .filter(|i| matches!(i, Intent::Fire(f) if f.projectile.contains("skiimpact")))
        .count();
    let sounds = intents
        .iter()
        .filter(|i| matches!(i, Intent::Audio { id, .. } if *id == hard_sound(f)))
        .count();
    eprintln!("15 m drop: puffs {puffs} hard sounds {sounds}");
    assert!(!wrecked(&intents));
    assert!(state(&v, &w).is_some());
}
}

on_both! {
fn skiing_into_a_wall_with_skis_on_the_ground_puffs_but_does_not_wreck(f: &Fixture) {
    let (mut v, mut w) = world(f, 0., Some(-30.));
    ride(f, &mut v, &mut w, Vec3::new(0., 1., 0.), Quat::IDENTITY);
    step(&mut v, &mut w, 120);
    v.set_velocity(&mut w, VehicleId(1), [0., 0., -25.])
        .unwrap();
    let intents = step(&mut v, &mut w, 240);
    let puffs = intents
        .iter()
        .filter(|i| matches!(i, Intent::Fire(f) if f.projectile.contains("skiimpact")))
        .count();
    let hard = intents
        .iter()
        .any(|i| matches!(i, Intent::Audio { id, .. } if *id == hard_sound(f)));
    eprintln!("wall at 25: puffs {puffs} hard sound {hard}");
    assert!(puffs >= 1);
    assert!(hard);
    assert!(!wrecked(&intents));
}
}

on_both! {
/// A client's prediction copy never wrecks, removes or respawns a vehicle:
/// the same upside-down landing that wrecks the host's skis leaves the
/// predicted skis in place, and the host's listing decides what happens.
fn a_prediction_copy_leaves_wrecking_to_the_host(f: &Fixture) {
    let (mut v, mut w) = world(f, 0., None);
    v.set_prediction(true);
    ride(
        f,
        &mut v,
        &mut w,
        Vec3::new(0., 6., 0.),
        Quat::from_rotation_z(std::f32::consts::PI),
    );
    let intents = step(&mut v, &mut w, 240);
    assert!(!wrecked(&intents));
    assert!(state(&v, &w).is_some(), "the predicted skis stay");
}
}
