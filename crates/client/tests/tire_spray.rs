//! v20 tire emitters (`WheeledVehicle::advanceTime`, blocklandv20.exe
//! 0x571c60) for the stock vehicles; see docs/audits/skis-v20.md.
use bri_client::actor_effects::tire_sprays;
use bri_client::vehicles::VehicleFrame;
use glam::{Quat, Vec3};
use std::path::Path;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}
fn pack() -> bri_vehicles::Pack {
    bri_vehicles::Pack::load(root().join("content/vehicles-pack-012/vehicles.json")).unwrap()
}
fn frame(d: &bri_vehicles::Definition, speed: f32, contact: bool) -> VehicleFrame {
    VehicleFrame {
        position: Vec3::new(5., 0., 5.),
        rotation: Quat::IDENTITY,
        velocity: Vec3::new(0., 0., -speed),
        steering: 0.,
        wheel_suspension: d.wheels.iter().map(|w| w.rest_length).collect(),
        wheel_rotation: vec![0.; d.wheels.len()],
        wheel_contact: vec![contact; d.wheels.len()],
        turret_aim: [0.; 2],
    }
}

#[test]
#[ignore = "requires the converted vehicle and effects packs"]
fn stock_vehicles_spray_their_tire_emitter_by_speed_over_max_wheel_speed() {
    let pack = pack();
    let effects =
        bri_fx_runtime::EffectsPack::load(root().join("content/effects-runtime-pack-005")).unwrap();
    let emitters: Vec<_> = effects.emitter_ids().collect();
    let expected = [
        ("JeepVehicle", Some("v20/emitter/vehicletireemitter")),
        ("TankVehicle", Some("v20/emitter/vehicletireemitter")),
        (
            "FlyingWheeledJeepVehicle",
            Some("v20/emitter/vehicletireemitter"),
        ),
        ("skiVehicle", Some("v20/emitter/skiemitter")),
        // The Ball's `tireEmitter` is commented out; the Carpet has no wheels.
        ("BallVehicle", None),
        ("MagicCarpetVehicle", None),
        ("HorseArmor", None),
    ];
    for (name, emitter) in expected {
        let d = pack
            .definitions
            .iter()
            .find(|d| d.datablock == name)
            .unwrap();
        let sprays = tire_sprays(1, d, &frame(d, 10., true));
        match emitter {
            None => assert!(sprays.is_empty(), "{name}"),
            Some(emitter) => {
                assert!(
                    emitters.contains(&emitter),
                    "{name}: {emitter} not in the pack"
                );
                assert_eq!(sprays.len(), d.wheels.len(), "{name}");
                for (i, s) in sprays.iter().enumerate() {
                    assert_eq!(s.emitter, emitter);
                    assert_eq!(s.wheel, i);
                    // dt × speed / maxWheelSpeed of emitter time.
                    assert!((s.rate - 10. / d.max_speed).abs() < 1e-5, "{name}");
                    // At the bottom of the tire under its hub.
                    let w = &d.wheels[i];
                    let bottom = Vec3::new(5., 0., 5.) + Vec3::from(w.position)
                        - Vec3::Y * (w.rest_length + w.radius);
                    assert!(s.position.distance(bottom) < 1e-4, "{name}");
                }
                // Only above speed 1, and only on the ground.
                let slow = tire_sprays(1, d, &frame(d, 0.9, true));
                assert!(slow.iter().all(|s| s.rate == 0.), "{name}");
                let air = tire_sprays(1, d, &frame(d, 10., false));
                assert!(air.iter().all(|s| s.rate == 0.), "{name}");
            }
        }
    }
}
