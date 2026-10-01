//! A vehicle pack may leave a weapon's sound and effect out, as the chaos
//! vehicles do. Firing must then ask for no sound or effect at all: clients
//! drop the connection over a cue that names none.
use bri_chaos::fixture;
use bri_vehicles::*;
use rapier3d::prelude::*;

#[test]
fn a_weapon_without_sound_or_effect_fires_without_cues() {
    let (pack, _) = fixture::synthetic_vehicles().unwrap();
    let mut v = VehiclesWorld::new(pack).unwrap();
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed().translation(glam::Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(500., 0.5, 500.),
    );
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: bri_vehicles::testing::TURRET.into(),
            transform: Transform {
                position: [0., 0.1, 0.],
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
    v.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            fire: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut intents = Vec::new();
    for _ in 0..40 {
        v.pre_step(&mut w, &[]).unwrap();
        w.step();
        v.post_step(&mut w).unwrap();
        intents.extend(v.drain_intents());
    }
    assert!(intents.iter().any(|i| matches!(i, Intent::Fire(_))));
    for intent in &intents {
        match intent {
            Intent::Audio { id, .. } | Intent::Effect { id, .. } => {
                assert!(!id.is_empty(), "{intent:?}")
            }
            _ => {}
        }
    }
}
