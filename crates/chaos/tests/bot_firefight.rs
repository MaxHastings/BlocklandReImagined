//! Regression investigation for the v0.2.0 Bedroom mixed-bot crash report.
//! Real content is required; no interactive client or renderer is launched.
use anyhow::{Result, ensure};
use bri_chaos::{fixture, scan::ensure_finite};
use bri_sim::session::{Command, MiniGameRequest};
use bri_world::{Brick, ContentRef, VehicleSpawn, World};
use glam::Vec3;

#[test]
#[ignore = "requires generated v20 content"]
fn bedroom_mixed_firefight_survives_spawn_loadout_changes() -> Result<()> {
    let root = fixture::content_root().expect("set BRI_CONTENT to generated content");
    // Also run without a loadout change: the reported edit may be coincidental.
    for change in [false, true] {
        let mut packages = bri_package::packages::PackageSet::load_root(&root)?;
        for id in [
            "bot_hole",
            "blockhead_bot",
            "bot_zombie",
            "gravity-gun",
            "gravity-gun-tool",
        ] {
            if !packages.packages.iter().any(|p| p.id == id) {
                packages.packages.push(bri_package::packages::PackageEntry {
                    id: id.into(),
                    version: "1.0.0".into(),
                    side: bri_package::packages::Side::Shared,
                    dir: format!("addons/{id}"),
                    role: None,
                });
            }
        }
        bri_package::library::follow_manifest_sides(&root, &mut packages);
        bri_package::library::follow_companions(&root, &mut packages);
        let dedicated = bri_net::dedicated::load_packages(
            &root,
            &packages,
            World::new(
                "Mixed firefight".into(),
                "v20/add-ons/map_bedroom/bedroom.mis".into(),
                vec![[1.0; 4]],
            ),
        )?;
        let origin = dedicated.spawn_points[0];
        let mut s = dedicated.session;
        let owner = 1;
        let choices = s.bot_choices();
        let head = choices
            .iter()
            .find(|(_, name)| name == "Blockhead Bot")
            .map(|(id, _)| id.clone())
            .expect("Blockhead Bot enabled");
        let zombie = choices
            .iter()
            .find(|(_, name)| name == "Zombie")
            .map(|(id, _)| id.clone())
            .expect("Zombie enabled");
        let plate = "v20/brick/brickvehiclespawndata".to_owned();
        let mesh = &s.simulation().definitions.entries[&plate].mesh;
        let size = [
            mesh.footprint_studs[0] as f32 * 0.5,
            mesh.height_plates as f32 * 0.2,
            mesh.footprint_studs[1] as f32 * 0.5,
        ];
        let cells = [0.5, 0.2, 0.5];
        let anchor = Vec3::from_array(std::array::from_fn(|axis| {
            ((origin[axis] - size[axis] * 0.5) / cells[axis]).round() * cells[axis]
                + size[axis] * 0.5
        }));
        let mut world = World::new(
            "Mixed firefight".into(),
            "v20/add-ons/map_bedroom/bedroom.mis".into(),
            vec![[1.0; 4]],
        );
        for side in 0..2 {
            for i in 0..8 {
                let id = (side * 8 + i + 1) as u64;
                let mut b = Brick::new(
                    ContentRef::Resolved(plate.clone()),
                    (anchor
                        + Vec3::new(
                            (i % 4) as f32 * (size[0] + 1.0),
                            0.0,
                            side as f32 * (size[2] * 2.0 + 8.0) + (i / 4) as f32 * (size[2] + 1.0),
                        ))
                    .into(),
                    owner,
                );
                b.vehicle = Some(Box::new(VehicleSpawn {
                    vehicle: ContentRef::Resolved(if side == 0 {
                        head.clone()
                    } else {
                        zombie.clone()
                    }),
                    recolor: false,
                    team: None,
                }));
                world.bricks.insert(id, b);
            }
        }
        world.next_brick_id = 17;
        world.owners.insert(
            owner,
            bri_world::OwnerRecord::new([1; 32], "Firefight host".into()),
        );
        s = bri_net::dedicated::load_packages(&root, &packages, world)?.session;
        let owner = s.join_verified(
            "Firefight host".into(),
            origin,
            true,
            Some(bri_admin::Principal([1; 32])),
        )?;
        for _ in 0..30 {
            s.step()?;
        }
        ensure!(
            s.names().keys().filter(|o| s.is_bot(**o)).count() == 16,
            "all sixteen bots spawned; bricks={} bots={} notices={:?}",
            s.simulation().state().bricks.len(),
            s.names().keys().filter(|o| s.is_bot(**o)).count(),
            s.take_notices()
        );
        let mut settings = bri_minigames::Settings {
            loadout: [Some("v20.weapon.gunitem".into()), None, None, None, None],
            ..Default::default()
        };
        s.command(
            owner,
            2,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: settings.clone(),
            }),
        )?;
        let mut fired = std::collections::BTreeSet::new();
        for tick in 0..120 * 45 {
            if change && tick == 120 * 8 {
                settings.loadout[0] = Some("gravity-gun-tool:weapon/gravitygun".into());
                s.command(
                    owner,
                    3,
                    Command::MiniGame(MiniGameRequest::Configure {
                        settings: settings.clone(),
                    }),
                )?;
            }
            s.step()?;
            fired.extend(s.weapon_view().fired().map(|shot| shot.id));
            if tick % 6 == 0 {
                ensure_finite("firefight snapshot", &s.snapshot())?;
                ensure_finite("firefight motion", &s.motion_states())?;
            }
        }
        ensure!(!fired.is_empty(), "bots actually fired; change={change}");
        eprintln!(
            "change={change}: {} distinct projectiles observed across 45 seconds",
            fired.len()
        );
    }
    Ok(())
}
