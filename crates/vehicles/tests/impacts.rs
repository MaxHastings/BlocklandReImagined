//! Vehicle impacts against v20 (blocklandv20.exe `Vehicle::updatePos`
//! 0x56ecb1): a body collision plays the datablock's impact sound and
//! raises `onImpact`, and does no damage; see docs/audits/skis-v20.md.
use bri_vehicles::*;
use glam::Vec3;
use rapier3d::prelude::*;

fn world() -> (VehiclesWorld, PhysicsWorld) {
    let v = VehiclesWorld::new(
        Pack::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/vehicles-pack-011/vehicles.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(500., 0.5, 500.),
    );
    // A wall across the way, 20 ahead.
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., 5., -20.)),
        ColliderBuilder::cuboid(40., 5., 0.5),
    );
    (v, w)
}
fn spawn(v: &mut VehiclesWorld, w: &mut PhysicsWorld, name: &str, y: f32) {
    v.spawn(
        w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: format!("v20.vehicle.{name}"),
            transform: Transform {
                position: [0., y, 0.],
                ..Default::default()
            },
            spawn_id: None,
            respawn_ticks: None,
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
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

#[test]
fn stock_vehicles_hitting_a_wall_play_their_impact_sound_and_take_no_damage() {
    // (vehicle, the sound v20 plays for a hit this hard)
    let cases = [
        ("jeepvehicle", Some("fastImpactSound")),
        ("tankvehicle", Some("fastImpactSound")),
        ("flyingwheeledjeepvehicle", Some("fastImpactSound")),
        ("ballvehicle", Some("fastImpactSound")),
        ("skivehicle", Some("Impact1BSound")),
        // The Magic Carpet sets no impact sounds.
        ("magiccarpetvehicle", None),
    ];
    for (name, sound) in cases {
        let (mut v, mut w) = world();
        spawn(&mut v, &mut w, name, 3.);
        step(&mut v, &mut w, 180);
        v.set_velocity(&mut w, VehicleId(1), [0., 0., -30.])
            .unwrap();
        let intents = step(&mut v, &mut w, 180);
        let s = &v.snapshot(&w).vehicles[0];
        let sounds: Vec<_> = intents
            .iter()
            .filter_map(|i| match i {
                Intent::Audio { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        eprintln!("{name}: sounds {sounds:?}, damage {}", s.damage);
        // collDamageThresholdVel/collDamageMultiplier are never applied.
        assert_eq!(s.damage, 0., "{name}");
        assert!(!s.destroyed, "{name}");
        match sound {
            Some(sound) => assert!(sounds.contains(&sound), "{name}: {sounds:?}"),
            None => assert!(sounds.is_empty(), "{name}: {sounds:?}"),
        }
    }
}

#[test]
fn wheel_contact_follows_the_ground() {
    for name in [
        "jeepvehicle",
        "tankvehicle",
        "flyingwheeledjeepvehicle",
        "skivehicle",
    ] {
        let (mut v, mut w) = world();
        spawn(&mut v, &mut w, name, 3.);
        step(&mut v, &mut w, 180);
        let s = &v.snapshot(&w).vehicles[0];
        assert!(
            !s.wheel_contact.is_empty() && s.wheel_contact.iter().all(|c| *c),
            "{name} on the ground: {:?}",
            s.wheel_contact
        );
        let (mut v, mut w) = world();
        spawn(&mut v, &mut w, name, 200.);
        step(&mut v, &mut w, 2);
        let s = &v.snapshot(&w).vehicles[0];
        assert!(
            s.wheel_contact.iter().all(|c| !*c),
            "{name} in the air: {:?}",
            s.wheel_contact
        );
    }
}
