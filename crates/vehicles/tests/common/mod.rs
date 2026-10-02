//! The vehicle catalog a test runs on, by role: the synthetic stand-ins
//! (`bri_vehicles::testing`) everywhere, and v20's converted vehicles where
//! that content is present (the push gate runs those with
//! `--include-ignored`).
#![allow(dead_code)]
use bri_vehicles::{testing, *};
use rapier3d::prelude::*;

pub struct Fixture {
    pub pack: Pack,
    /// A strafe-steered four-wheel car with passenger seats.
    pub car: &'static str,
    /// Wheeled, with an attached turret and its gunner in seat 2.
    pub tank: &'static str,
    /// Wheeled, with Blockland's wheeled flying forces.
    pub flying_car: &'static str,
    pub skis: &'static str,
    /// The `Flying` family.
    pub carpet: &'static str,
    pub horse: &'static str,
    pub rowboat: &'static str,
    /// A player-type gun that charges.
    pub cannon: &'static str,
    /// A standalone player-type gun turret.
    pub turret: &'static str,
    pub ball: &'static str,
    pub tumble: &'static str,
}

impl Fixture {
    pub fn synthetic() -> Self {
        Self {
            pack: testing::pack(),
            car: testing::CAR,
            tank: testing::TANK,
            flying_car: testing::FLYING_CAR,
            skis: testing::SKIS,
            carpet: testing::CARPET,
            horse: testing::HORSE,
            rowboat: testing::ROWBOAT,
            cannon: testing::CANNON,
            turret: testing::TURRET,
            ball: testing::BALL,
            tumble: testing::TUMBLE,
        }
    }
    /// v20's vehicles, converted by `tools/bootstrap.py`.
    pub fn content() -> Self {
        Self {
            pack: content_pack(),
            car: "v20.vehicle.jeepvehicle",
            tank: "v20.vehicle.tankvehicle",
            flying_car: "v20.vehicle.flyingwheeledjeepvehicle",
            skis: "v20.vehicle.skivehicle",
            carpet: "v20.vehicle.magiccarpetvehicle",
            horse: "v20.vehicle.horsearmor",
            rowboat: "v20.vehicle.rowboatarmor",
            cannon: "v20.vehicle.cannonturret",
            turret: "v20.vehicle.tankturretplayer",
            ball: "v20.vehicle.ballvehicle",
            tumble: "v20.vehicle.deathvehicle",
        }
    }
    pub fn definition(&self, id: &str) -> &Definition {
        self.pack
            .definitions
            .iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("no vehicle {id}"))
    }
    pub fn vehicles(&self) -> VehiclesWorld {
        VehiclesWorld::new(self.pack.clone()).unwrap()
    }
}

pub fn content_dir() -> std::path::PathBuf {
    bri_package::testing::pack_dir(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        "vehicles",
    )
}

pub fn content_pack() -> Pack {
    Pack::load(content_dir().join("vehicles.json")).unwrap()
}

/// A physics world with a flat floor `half` units each way around the origin.
pub fn floor(half: f32) -> PhysicsWorld {
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed().translation(glam::Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(half, 0.5, half),
    );
    w
}

/// Runs a test body on the synthetic catalog, and again on v20's converted
/// vehicles when that content is there (`--include-ignored`).
macro_rules! on_both {
    ($(#[$meta:meta])* fn $name:ident($f:ident: &Fixture) $body:block) => {
        $(#[$meta])*
        mod $name {
            #[allow(unused_imports)]
            use super::*;
            fn body($f: &Fixture) $body
            #[test]
            fn synthetic() {
                body(&Fixture::synthetic())
            }
            #[test]
            #[ignore = "requires generated v20 content"]
            fn content() {
                body(&Fixture::content())
            }
        }
    };
}
