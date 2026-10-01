//! Skis against v20's `skiVehicle` (Item_Skis) and the WheeledVehicle
//! forces decoded from blocklandv20.exe; see docs/audits/skis-v20.md.
use bri_vehicles::*;
use glam::{Quat, Vec3};
use rapier3d::prelude::*;

fn pack() -> Pack {
    Pack::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/vehicles-pack-012/vehicles.json"
    ))
    .unwrap()
}
/// A 4 km ground plane tilted `slope` radians down towards -Z, plus an
/// optional wall across the track at `wall_z`.
fn world(slope: f32, wall_z: Option<f32>) -> (VehiclesWorld, PhysicsWorld) {
    let v = VehiclesWorld::new(pack()).unwrap();
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
fn ride(v: &mut VehiclesWorld, w: &mut PhysicsWorld, position: Vec3, rotation: Quat) {
    v.spawn(
        w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: "v20.vehicle.skivehicle".into(),
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
fn wrecked(intents: &[Intent]) -> bool {
    intents
        .iter()
        .any(|i| matches!(i, Intent::TumbleRequested { .. }))
}

#[test]
fn skis_push_to_forty_along_the_nose_on_flat_ground() {
    let (mut v, mut w) = world(0., None);
    ride(&mut v, &mut w, Vec3::new(0., 1., 0.), Quat::IDENTITY);
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
    // forwardThrust 500 on 90 kg less bodyFriction 0.21 on the part of
    // the weight the springs leave to the hull: about 3.2 m/s^2.
    assert!((12.0..20.).contains(&-early.z), "{early:?}");
    // Thrust stops at maxForwardVel 40.
    assert!((36.0..40.5).contains(&velocity.length()), "{velocity:?}");
    assert!(velocity.x.abs() < 0.5 && velocity.y.abs() < 0.5);
}

#[test]
fn skis_slide_downhill_on_frictionless_tires() {
    let (mut v, mut w) = world(25f32.to_radians(), None);
    let tilt = Quat::from_rotation_x(-25f32.to_radians());
    ride(&mut v, &mut w, tilt * Vec3::new(0., 1., 0.), tilt);
    let intents = step(&mut v, &mut w, 4 * 120);
    let s = state(&v, &w).unwrap();
    let velocity = Vec3::from(s.velocity);
    eprintln!("25 degree slope 4 s coasting: {velocity:?}");
    assert!(!wrecked(&intents));
    assert!(velocity.z < -10., "{velocity:?}");
}

#[test]
fn ski_grip_turns_sideways_slide_into_the_nose_only_on_the_ground() {
    // Skis facing -Z moving sideways along +X: on the ground the sled
    // surface force (horizontalSurfaceForce 50 × speed) kills the slide.
    let (mut v, mut w) = world(0., None);
    ride(&mut v, &mut w, Vec3::new(0., 1., 0.), Quat::IDENTITY);
    step(&mut v, &mut w, 120);
    v.set_velocity(&mut w, VehicleId(1), [20., 0., -20.])
        .unwrap();
    step(&mut v, &mut w, 60);
    let ground = Vec3::from(state(&v, &w).unwrap().velocity);
    // High in the air nothing resists the slide.
    let (mut v, mut w) = world(0., None);
    ride(&mut v, &mut w, Vec3::new(0., 200., 0.), Quat::IDENTITY);
    v.set_velocity(&mut w, VehicleId(1), [20., 0., -20.])
        .unwrap();
    step(&mut v, &mut w, 60);
    let air = Vec3::from(state(&v, &w).unwrap().velocity);
    eprintln!("sideways after 0.5 s: ground {ground:?} air {air:?}");
    assert!(ground.x.abs() < 4., "{ground:?}");
    assert!(air.x > 18., "{air:?}");
}

#[test]
fn mouse_steering_turns_moving_skis_but_not_standing_ones() {
    let heading = |speed: f32| {
        let (mut v, mut w) = world(0., None);
        ride(&mut v, &mut w, Vec3::new(0., 1., 0.), Quat::IDENTITY);
        step(&mut v, &mut w, 120);
        v.set_velocity(&mut w, VehicleId(1), [0., 0., -speed])
            .unwrap();
        drive(
            &mut v,
            Controls {
                look_delta: [0.885, 0.],
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
    // No speed, no bite: the yaw torque scales with speed / maxForwardVel.
    assert!(standing.abs() < 2., "{standing}");
    assert!(moving > 20., "{moving}");
}

#[test]
fn skis_wreck_when_the_body_lands_without_the_skis_under_it() {
    // Upside down, the body lands first with every ski in the air.
    let (mut v, mut w) = world(0., None);
    ride(
        &mut v,
        &mut w,
        Vec3::new(0., 6., 0.),
        Quat::from_rotation_z(std::f32::consts::PI),
    );
    let intents = step(&mut v, &mut w, 240);
    assert!(wrecked(&intents));
    assert!(state(&v, &w).is_none());
}

#[test]
fn a_hard_landing_on_the_skis_is_not_a_wreck() {
    let (mut v, mut w) = world(0., None);
    ride(&mut v, &mut w, Vec3::new(0., 15., 0.), Quat::IDENTITY);
    let intents = step(&mut v, &mut w, 360);
    let puffs = intents
        .iter()
        .filter(|i| matches!(i, Intent::Fire(f) if f.projectile.contains("skiimpact")))
        .count();
    let sounds = intents
        .iter()
        .filter(|i| matches!(i, Intent::Audio { id, .. } if id == "Impact1BSound"))
        .count();
    eprintln!("15 m drop: puffs {puffs} hard sounds {sounds}");
    assert!(!wrecked(&intents));
    assert!(state(&v, &w).is_some());
}

#[test]
fn skiing_into_a_wall_with_skis_on_the_ground_puffs_but_does_not_wreck() {
    let (mut v, mut w) = world(0., Some(-30.));
    ride(&mut v, &mut w, Vec3::new(0., 1., 0.), Quat::IDENTITY);
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
        .any(|i| matches!(i, Intent::Audio { id, .. } if id == "Impact1BSound"));
    eprintln!("wall at 25: puffs {puffs} hard sound {hard}");
    assert!(puffs >= 1);
    assert!(hard);
    assert!(!wrecked(&intents));
}

/// A client's prediction copy never wrecks, removes or respawns a vehicle:
/// the same upside-down landing that wrecks the host's skis leaves the
/// predicted skis in place, and the host's listing decides what happens.
#[test]
fn a_prediction_copy_leaves_wrecking_to_the_host() {
    let (mut v, mut w) = world(0., None);
    v.set_prediction(true);
    ride(
        &mut v,
        &mut w,
        Vec3::new(0., 6., 0.),
        Quat::from_rotation_z(std::f32::consts::PI),
    );
    let intents = step(&mut v, &mut w, 240);
    assert!(!wrecked(&intents));
    assert!(state(&v, &w).is_some(), "the predicted skis stay");
}
