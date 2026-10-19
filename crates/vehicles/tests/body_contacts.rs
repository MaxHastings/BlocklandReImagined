//! A vehicle's hull scraping fixed ground or walls ([`BodyContacts`]): a hull sliding along a wall
//! at speed raises `BodyContact` at the wall; one at rest and one without the table raise none; a
//! continuing scrape is paced by `interval`.
#[allow(unused_macros)]
mod common;
use bri_vehicles::schema::{BodyContacts, ContactEffect};
use bri_vehicles::*;
use common::Fixture;
use glam::Vec3;
use rapier3d::prelude::*;

fn contacts() -> BodyContacts {
    BodyContacts {
        min_slip: 3.0,
        interval: 0.5,
        sound_interval: 0.5,
        effects: vec![ContactEffect {
            surface: "*".into(),
            effect: "t:sparks".into(),
            sound: Some("t:scrape".into()),
        }],
    }
}

fn world(with: Option<BodyContacts>) -> (VehiclesWorld, PhysicsWorld) {
    let mut f = Fixture::synthetic();
    let car = f.car;
    for d in f.pack.definitions.iter_mut().filter(|d| d.id == car) {
        d.body_contacts = with.clone();
    }
    let mut v = f.vehicles();
    let mut w = common::floor(500.);
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., 5., -20.)),
        ColliderBuilder::cuboid(40., 5., 0.5),
    );
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: car.into(),
            transform: Transform {
                position: [0., 3., 0.],
                ..Default::default()
            },
            spawn_id: None,
            respawn_ticks: None,
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
    (v, w)
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
fn scrapes(intents: &[Intent]) -> Vec<(f32, [f32; 3])> {
    intents
        .iter()
        .filter_map(|i| match i {
            Intent::BodyContact { slip, point, .. } => Some((*slip, *point)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_hull_sliding_along_a_wall_scrapes_at_the_wall() {
    let (mut v, mut w) = world(Some(contacts()));
    step(&mut v, &mut w, 180);
    // Thrown into the wall and along it, over the ground.
    v.set_velocity(&mut w, VehicleId(1), [12., 8., -30.])
        .unwrap();
    let intents = step(&mut v, &mut w, 240);
    let hits = scrapes(&intents);
    eprintln!("scrapes {hits:?}");
    assert!(!hits.is_empty(), "no scrape against the wall");
    for (slip, point) in &hits {
        assert!(*slip >= 3.0, "slip {slip}");
        assert!(
            (point[2] + 19.5).abs() < 1.5,
            "scrape at {point:?}, not the wall"
        );
    }
    // Paced by the interval (0.5 s = 60 ticks), not once a tick.
    assert!(hits.len() <= 240 / 60 + 1, "{} scrapes in 2 s", hits.len());
}

#[test]
fn a_resting_hull_and_a_vehicle_without_the_table_make_none() {
    let (mut v, mut w) = world(Some(contacts()));
    assert!(scrapes(&step(&mut v, &mut w, 360)).is_empty());
    let (mut v, mut w) = world(None);
    step(&mut v, &mut w, 180);
    v.set_velocity(&mut w, VehicleId(1), [12., 8., -30.])
        .unwrap();
    assert!(scrapes(&step(&mut v, &mut w, 240)).is_empty());
}
