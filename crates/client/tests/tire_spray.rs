//! v20 tire emitters (`WheeledVehicle::advanceTime`, blocklandv20.exe
//! 0x571c60); see docs/audits/skis-v20.md. Runs on the made-up vehicle
//! catalog (`bri_vehicles::testing`) and, ignored, on the stock vehicles.
#[macro_use]
mod support;

use anyhow::{Context, Result};
use bri_client::actor_effects::tire_sprays;
use bri_client::vehicles::VehicleFrame;
use bri_fx_runtime::EffectsPack;
use glam::{Quat, Vec3};
use std::sync::Arc;
use support::files::repo_root;

/// Vehicles and the effects pack their tyre emitters play from.
struct Fixture {
    pack: bri_vehicles::Pack,
    effects: Arc<EffectsPack>,
    /// Each vehicle's datablock name and the emitter its tyres spray, if any.
    expected: Vec<(String, Option<String>)>,
}

impl Fixture {
    fn content() -> Result<Self> {
        let root = repo_root();
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
        Ok(Self {
            pack: bri_vehicles::Pack::load(root.join("content/vehicles-pack-012/vehicles.json"))?,
            effects: EffectsPack::load(root.join("content/effects-runtime-pack-005"))?,
            expected: expected
                .map(|(n, e)| (n.to_string(), e.map(String::from)))
                .into(),
        })
    }

    /// The made-up catalog: the wheeled vehicles and the skis spray their
    /// emitters, which the effects pack carries; the ball, carpet and horse
    /// have none.
    fn synthetic() -> Result<Self> {
        use bri_vehicles::testing as vt;
        let pack = vt::pack();
        let emitter = |name: &str| format!("v20/emitter/{}", name.to_ascii_lowercase());
        let datablock = |id: &str| -> Result<String> {
            Ok(pack
                .definitions
                .iter()
                .find(|d| d.id == id)
                .context("synthetic vehicle")?
                .datablock
                .clone())
        };
        let mut expected = vec![];
        for id in [vt::CAR, vt::TANK, vt::FLYING_CAR] {
            expected.push((datablock(id)?, Some(emitter(vt::TIRE_EMITTER))));
        }
        expected.push((datablock(vt::SKIS)?, Some(emitter(vt::SKI_EMITTER))));
        for id in [vt::BALL, vt::CARPET, vt::HORSE] {
            expected.push((datablock(id)?, None));
        }
        let effects = bri_fx_runtime::testing::pack(|library| {
            for name in [vt::TIRE_EMITTER, vt::SKI_EMITTER] {
                library.emitters.push(bri_fx_runtime::testing::emitter(
                    &emitter(name),
                    name,
                    &["particle"],
                ));
            }
        });
        Ok(Self {
            pack,
            effects,
            expected,
        })
    }
}

synthetic_and_content!(Fixture: vehicles_spray_their_tire_emitter_by_speed_over_max_wheel_speed);

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

fn vehicles_spray_their_tire_emitter_by_speed_over_max_wheel_speed(f: &Fixture) -> Result<()> {
    let pack = &f.pack;
    let emitters: Vec<_> = f.effects.emitter_ids().collect();
    for (name, emitter) in &f.expected {
        let emitter = emitter.as_deref();
        let d = pack
            .definitions
            .iter()
            .find(|d| d.datablock == *name)
            .context("vehicle")?;
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
    Ok(())
}
