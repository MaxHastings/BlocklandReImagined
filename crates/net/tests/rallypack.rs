//! Main weapon-family smoke check using privately owned converted content.
use anyhow::{Context, Result, ensure};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    player::MoveInput,
    session::{Command, Session, ToolInventory},
};
use glam::Vec3;
use std::{path::PathBuf, sync::Arc};
mod common;

fn game(item: &str) -> Result<Session> {
    let root = PathBuf::from(
        std::env::var_os("BRI_RALLYPACK_TEST_ROOT").context("BRI_RALLYPACK_TEST_ROOT required")?,
    );
    let content = PathBuf::from(
        std::env::var_os("BRI_P90_CONTENT_ROOT").context("BRI_P90_CONTENT_ROOT required")?,
    );
    let installed = PackageSet::parse(&std::fs::read(content.join("packages.json"))?)?;
    let dir = &installed
        .packages
        .iter()
        .find(|p| p.id == "v20-weapons")
        .context("base weapons")?
        .dir;
    let base =
        bri_weapons::Pack::from_json(&std::fs::read(content.join(dir).join("weapons.json"))?)?;
    let pack = bri_weapons::Pack::from_json(&std::fs::read(
        root.join("weapon_rallypack/assets/weapons.json"),
    )?)?;
    let (pack, problems) = base.merge(vec![("weapon_rallypack/assets".into(), pack)]);
    ensure!(problems.is_empty(), "{problems:?}");
    let set = PackageSet {
        schema_version: 1,
        packages: vec![
            PackageEntry {
                id: "weapon_rallypack".into(),
                version: "0.1.0".into(),
                side: Side::Shared,
                dir: "weapon_rallypack".into(),
                role: None,
            },
            PackageEntry {
                id: "weapon_rallypack-rules".into(),
                version: "0.1.0".into(),
                side: Side::Server,
                dir: "weapon_rallypack-rules".into(),
                role: None,
            },
        ],
    };
    let catalog = Catalog::load(&root, &set, true).map_err(|d| anyhow::anyhow!("{d:?}"))?;
    let mut game = common::session();
    game.set_weapon_pack(pack)?;
    game.install_packages(Arc::new(catalog), None)?;
    game.set_spawn_loadout(ToolInventory {
        slots: vec![
            Some(format!("weapon_rallypack:weapon/{item}")),
            None,
            None,
            None,
            None,
        ],
        selected: None,
    })?;
    Ok(game)
}

#[test]
#[ignore = "requires privately converted Rallypack"]
fn principal_weapon_families_mount_fire_and_reload_without_script_errors() -> Result<()> {
    for item in [
        "r_assaultrifleitem",
        "r_shotgunitem",
        "r_sniperrifleitem",
        "r_pistolmagnumitem",
        "r_rocketlauncheritem",
        "r_bowitem",
        "r_m67item",
        "r_shieldpistolitem",
    ] {
        let mut game = game(item)?;
        let host = game.join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)?;
        for _ in 0..5 {
            game.step()?;
        }
        game.command(host, 1, Command::EquipTool { slot: Some(0) })?;
        for _ in 0..90 {
            game.step()?;
        }
        game.command(host, 2, Command::WeaponTrigger { down: true })?;
        let mut seq = 2;
        let mut fired = false;
        for _ in 0..120 {
            seq += 1;
            game.movement(host, seq, MoveInput::default())?;
            game.step()?;
            fired |= !game.weapon_view().projectiles.is_empty();
        }
        seq += 1;
        game.command(host, seq, Command::WeaponTrigger { down: false })?;
        for _ in 0..60 {
            game.step()?;
            fired |= !game.weapon_view().projectiles.is_empty();
        }
        ensure!(fired, "{item} never fired");
        seq += 1;
        game.command(host, seq, Command::ToggleLight)?;
        for _ in 0..1500 {
            game.step()?;
        }
        ensure!(
            game.package_diagnostics().is_empty(),
            "{item}: {:?}",
            game.package_diagnostics()
        );
        println!("{item}: mount/fire/reload OK");
    }
    Ok(())
}
