//! Vehicle impacts against v20 (blocklandv20.exe `Vehicle::updatePos`
//! 0x56ecb1): a body collision plays the datablock's impact sound and
//! raises `onImpact`, and does no damage; see docs/audits/skis-v20.md.
#[macro_use]
mod common;
use bri_vehicles::*;
use common::Fixture;
use glam::Vec3;
use rapier3d::prelude::*;

fn world(f: &Fixture) -> (VehiclesWorld, PhysicsWorld) {
    let v = f.vehicles();
    let mut w = common::floor(500.);
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
            definition: name.into(),
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

on_both! {
fn stock_vehicles_hitting_a_wall_play_their_impact_sound_and_take_no_damage(f: &Fixture) {
    // Each plays its datablock's hard impact sound for a hit this hard; the
    // carpet sets no impact sounds and plays none.
    let cases = [f.car, f.tank, f.flying_car, f.ball, f.skis, f.carpet];
    for name in cases {
        let d = f.definition(name);
        let sound = d.authored.get("hardimpactsound").map(String::as_str);
        assert_eq!(
            sound.is_none(),
            name == f.carpet,
            "{name}: only the carpet is silent"
        );
        let (mut v, mut w) = world(f);
        spawn(&mut v, &mut w, name, 3.);
        step(&mut v, &mut w, 180);
        // Thrown at the wall: an empty vehicle's wheels are not turning,
        // and Torque's tyres skid a wheeled one to a stop within a few units
        // on the ground (v20's too), so it flies there over the ground.
        v.set_velocity(&mut w, VehicleId(1), [0., 8., -30.])
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
}

on_both! {
fn wheel_contact_follows_the_ground(f: &Fixture) {
    for name in [f.car, f.tank, f.flying_car, f.skis] {
        let (mut v, mut w) = world(f);
        spawn(&mut v, &mut w, name, 3.);
        step(&mut v, &mut w, 180);
        let s = &v.snapshot(&w).vehicles[0];
        assert!(
            !s.wheel_contact.is_empty() && s.wheel_contact.iter().all(|c| *c),
            "{name} on the ground: {:?}",
            s.wheel_contact
        );
        let (mut v, mut w) = world(f);
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
}
