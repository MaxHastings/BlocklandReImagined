//! Native vehicle packs for vehicle rendering tests: the generated v20 pack,
//! or `bri_client::testing::vehicles`' made-up one (the engine's made-up
//! catalog with box models), which `VehicleAssets::load` reads through the
//! same checks.
#![allow(dead_code)]

use super::files::{repo_root, scratch};
use anyhow::{Context, Result};
use bri_vehicles::testing as vt;
use glam::Vec3;
use std::path::PathBuf;

pub struct VehicleFixture {
    /// The folder holding `vehicles.json`.
    pub dir: PathBuf,
    /// The weapons pack whose explosion debris the vehicle pack carries.
    pub weapons: bri_weapons::Pack,
    /// Whether this is the generated v20 content.
    pub content: bool,
    /// Vehicles drawn by the vehicle path (not the avatar horse rig), each
    /// with a camera distance that frames it.
    pub drawn: Vec<(String, f32)>,
    /// A wheeled car with seats.
    pub car: String,
    /// Where regenerated evidence goes.
    pub out: PathBuf,
    _scratch: Option<tempfile::TempDir>,
}

impl VehicleFixture {
    pub fn content() -> Result<Self> {
        let root = repo_root();
        let weapons = bri_weapons::Pack::from_json(&std::fs::read(
            bri_package::testing::pack_dir(&root.join("content"), "weapons").join("weapons.json"),
        )?)?;
        Ok(Self {
            dir: bri_package::testing::pack_dir(&root.join("content"), "vehicles"),
            weapons,
            content: true,
            drawn: [
                ("v20.vehicle.jeepvehicle", 12.0),
                ("v20.vehicle.tankvehicle", 14.0),
                ("v20.vehicle.magiccarpetvehicle", 9.0),
                ("v20.vehicle.rowboatarmor", 9.0),
                ("v20.vehicle.cannonturret", 7.0),
                ("v20.vehicle.ballvehicle", 6.0),
                ("v20.vehicle.flyingwheeledjeepvehicle", 12.0),
            ]
            .map(|(id, d)| (id.to_string(), d))
            .into(),
            car: "v20.vehicle.jeepvehicle".into(),
            out: root.join("artifacts/native-vehicles"),
            _scratch: None,
        })
    }

    pub fn synthetic() -> Result<Self> {
        let dir = scratch("vehicles-")?;
        let weapons = bri_client::testing::items::weapons_pack();
        let pack = bri_client::testing::vehicles::write_pack(dir.path())?;
        let drawn = [
            vt::CAR,
            vt::TANK,
            vt::CARPET,
            vt::ROWBOAT,
            vt::CANNON,
            vt::BALL,
            vt::FLYING_CAR,
        ]
        .into_iter()
        .map(|id| {
            let d = pack
                .definitions
                .iter()
                .find(|d| d.id == id)
                .context("synthetic vehicle")?;
            // Far enough back to see the whole body.
            let size = Vec3::from(d.bounds_max) - Vec3::from(d.bounds_min);
            Ok((id.to_string(), size.length() * 1.6 + 2.0))
        })
        .collect::<Result<_>>()?;
        Ok(Self {
            dir: dir.path().to_path_buf(),
            weapons,
            content: false,
            drawn,
            car: vt::CAR.into(),
            out: PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("native-vehicles-synthetic"),
            _scratch: Some(dir),
        })
    }
}
