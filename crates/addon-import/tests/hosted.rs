//! Imported Add-On packages in a hosted headless game: the dedicated server's
//! own setup (`bri_net::dedicated`) loads the base game plus imported packages
//! listed in `packages.json`, and players use their content.
use bri_addon_import::{Options, import};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_sim::session::{Command, Session};
use std::path::{Path, PathBuf};

const MAP: &str = "v20/add-ons/map_slate/slate.mis";

fn content_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

/// Imported packages live in a scratch folder inside the content root,
/// because package directories must be inside it; removed on drop.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn with_packages(dirs: &[(&str, &str)]) -> PackageSet {
    let mut set = PackageSet::base();
    for (id, dir) in dirs {
        set.packages.push(PackageEntry {
            id: (*id).into(),
            version: "1.0.0".into(),
            side: Side::Shared,
            dir: (*dir).into(),
            role: None,
        });
    }
    set
}

fn host(set: &PackageSet) -> (Session, glam::Vec3) {
    let root = content_root();
    let schema = bri_world::World::new("Hosted add-ons".into(), MAP.into(), vec![[1.0; 4]]);
    let host = bri_net::dedicated::load_packages(&root, set, schema).unwrap();
    let spawn = host.spawn_points[0];
    (host.session, spawn)
}

/// Gives `item` to a new player, equips it, clicks once and returns the
/// projectile definitions that appeared.
fn fire(session: &mut Session, spawn: glam::Vec3, item: &str) -> Vec<String> {
    let player = session.join("Tester".into(), spawn, false).unwrap();
    session.give_item(player, item).unwrap();
    let slot = session.tool_inventories()[&player]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(item))
        .expect("given item in the tool inventory");
    session
        .command(player, 1, Command::EquipTool { slot: Some(slot) })
        .unwrap();
    let mut seen = std::collections::BTreeSet::new();
    // A live client streams movement; idle shooters have their triggers dropped.
    for tick in 0..240u64 {
        session
            .movement(player, tick + 1, bri_sim::player::MoveInput::default())
            .unwrap();
        if tick == 120 || tick == 121 {
            session
                .command(player, tick, Command::WeaponTrigger { down: tick == 120 })
                .unwrap();
        }
        session.step().unwrap();
        // Joining plays the spawn effect, itself a projectile.
        for p in session.weapon_view().fired() {
            seen.insert((p.id, p.definition.clone()));
        }
    }
    session.disconnect(player).unwrap();
    seen.into_iter().map(|(_, d)| d).collect()
}

#[test]
#[ignore = "requires generated content (content/, see docs/content-regeneration.md)"]
fn imported_weapon_package_is_hosted_beside_vanilla() {
    let root = content_root();
    let scratch = Scratch(root.join(format!("_addon-hosted-{}", std::process::id())));
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Weapon_Synthetic_Blaster");
    import(&Options {
        input: fixture,
        out: scratch.0.join("blaster"),
        reference: None,
        core: vec![],
        version: "1.0.0".into(),
    })
    .unwrap();
    let dir = format!(
        "{}/blaster",
        scratch.0.file_name().unwrap().to_string_lossy()
    );
    let set = with_packages(&[("weapon_synthetic_blaster", &dir)]);
    let (_, spawn) = host(&set);
    // The imported brick loads into a hosted world too.
    let mut world = bri_world::World::new("Hosted add-ons".into(), MAP.into(), vec![[1.0; 4]]);
    let pad = bri_world::Brick::new(
        bri_world::ContentRef::Resolved(
            "weapon_synthetic_blaster:brick/brickblasterpaddata".into(),
        ),
        [
            spawn.x.round() + 4.25,
            (spawn.y / 0.2).round() * 0.2 + 0.3,
            spawn.z.round() + 0.25,
        ],
        0,
    );
    world.bricks.insert(1, pad);
    world.next_brick_id = 2;
    let mut session = bri_net::dedicated::load_packages(&root, &set, world)
        .unwrap()
        .session;
    assert!(
        session
            .simulation()
            .definitions
            .entries
            .contains_key("weapon_synthetic_blaster:brick/brickblasterpaddata")
    );
    assert_eq!(session.simulation().state().bricks.len(), 1);
    // Vanilla still works, and the imported weapon fires its own projectile.
    assert_eq!(
        fire(&mut session, spawn, "v20.weapon.gunitem"),
        ["v20.projectile.gunprojectile"]
    );
    assert_eq!(
        fire(
            &mut session,
            spawn,
            "weapon_synthetic_blaster:weapon/blasteritem"
        ),
        ["weapon_synthetic_blaster:projectile/blasterboltprojectile"]
    );
}

const ARCHIVE: &str = "C:/Users/Maxwell/Documents/_Blockland_Maxwell_1588_Archive/Addons";
const REFERENCE: &str = "E:/Downloads/B4v21Launcher/versions/Blockland v20";
const VEHICLE_SPAWN: &str = "v20/brick/brickvehiclespawndata";
const CAR: &str = "vehicle_blocko_car:vehicle/blockocarvehicle";

/// The acceptance scenario for multi-package loading: the real Sawn-off
/// Shotgun and Blocko Car packages hosted beside the base game.
#[test]
#[ignore = "requires generated content and Maxwell's Add-On archive with the v20 reference"]
fn community_shotgun_and_car_work_in_a_hosted_game() {
    let archive = std::env::var("BRI_ADDON_ARCHIVE").unwrap_or(ARCHIVE.into());
    let reference = std::env::var("BRI_V20_REFERENCE").unwrap_or(REFERENCE.into());
    if !Path::new(&archive).is_dir() || !Path::new(&reference).is_dir() {
        eprintln!("skipped: community archive or v20 reference install not on this machine");
        return;
    }
    let root = content_root();
    let scratch = Scratch(root.join(format!("_addon-accept-{}", std::process::id())));
    let name = scratch
        .0
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for (addon, dir) in [("Weapon_Shotgun", "shotgun"), ("Vehicle_Blocko_Car", "car")] {
        import(&Options {
            input: Path::new(&archive).join(format!("{addon}.zip")),
            out: scratch.0.join(dir),
            reference: Some(reference.clone().into()),
            core: vec![],
            version: "1.0.0".into(),
        })
        .unwrap();
    }
    let set = with_packages(&[
        ("weapon_shotgun", &format!("{name}/shotgun")),
        ("vehicle_blocko_car", &format!("{name}/car")),
    ]);
    // Place the car's spawn brick a few steps in front of a spawn point.
    let (_, spawn) = host(&set);
    let mut world = bri_world::World::new("Hosted add-ons".into(), MAP.into(), vec![[1.0; 4]]);
    let mut brick = bri_world::Brick::new(
        bri_world::ContentRef::Resolved(VEHICLE_SPAWN.into()),
        // On the stud and plate grid, sitting on the ground at the spawn.
        [
            spawn.x.round(),
            (spawn.y / 0.2).round() * 0.2 + 0.1,
            spawn.z.round() - 10.0,
        ],
        0,
    );
    brick.vehicle = Some(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved(CAR.into()),
        recolor: true,
    });
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let mut session = bri_net::dedicated::load_packages(&root, &set, world)
        .unwrap()
        .session;

    // The ported shotgun fires its three pellets, beside the vanilla gun.
    assert_eq!(
        fire(&mut session, spawn, "weapon_shotgun:weapon/shotgunitem"),
        ["weapon_shotgun:projectile/shotgunprojectile"; 3]
    );
    assert_eq!(
        fire(&mut session, spawn, "v20.weapon.gunitem"),
        ["v20.projectile.gunprojectile"]
    );

    // The car spawns on its brick; a player walks into it, mounts and drives.
    let driver = session.join("Driver".into(), spawn, false).unwrap();
    let mut sequence = 0;
    let mut feed = |s: &mut Session, input: bri_sim::player::MoveInput, ticks: usize| {
        for _ in 0..ticks {
            sequence += 1;
            s.movement(driver, sequence, input).unwrap();
            s.step().unwrap();
        }
    };
    feed(&mut session, Default::default(), 120);
    let infos = session.vehicle_infos();
    assert_eq!(infos.len(), 1, "the spawn brick produced the car");
    assert_eq!(infos[0].definition, CAR);
    for i in 0..80 {
        let input = bri_sim::player::MoveInput {
            forward: 1.0,
            jump: i % 3 == 0,
            ..Default::default()
        };
        feed(&mut session, input, 10);
        if session.mounted(driver).is_some() {
            break;
        }
    }
    assert!(
        session.mounted(driver).is_some(),
        "walked into the car and mounted"
    );
    let before = glam::Vec3::from(session.vehicle_poses()[0].position);
    let throttle = bri_sim::player::MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    feed(&mut session, throttle, 240);
    let after = glam::Vec3::from(session.vehicle_poses()[0].position);
    assert!(
        before.distance(after) > 5.0,
        "car drove {before} -> {after}"
    );
}
