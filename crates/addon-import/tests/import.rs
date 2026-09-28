//! Importer tests. The synthetic Add-On in `tests/fixtures` is ours (CC0) and
//! runs everywhere. The real community samples are not redistributable, so
//! `real_community_samples` runs only where Maxwell's archive and the v20
//! reference install exist, and says so when it skips.
use bri_addon_import::{Options, import, report::Report};
use bri_weapons::*;
use glam::Vec3;
use std::path::{Path, PathBuf};

fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bri-addon-import-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("package")
}

struct Empty;
impl Query for Empty {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        vec![]
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
}

/// Loads the imported weapons pack into the weapons runtime, equips `item`
/// and pulls the trigger; returns the projectile definitions spawned.
fn fire(package: &Path, item: &str) -> Vec<String> {
    let pack =
        Pack::from_json(&std::fs::read(package.join("assets/weapons.json")).unwrap()).unwrap();
    let mut world = WeaponsWorld::new(pack).unwrap();
    world.add_actor(ActorId(1), 5).unwrap();
    let slot = world.give(ActorId(1), item).unwrap();
    world.equip(ActorId(1), Some(slot)).unwrap();
    let mut spawned = vec![];
    for tick in 0..240 {
        // One click: press, then release on the next tick.
        if tick == 60 || tick == 61 {
            world.trigger(ActorId(1), tick == 60).unwrap();
        }
        for e in world.step(&mut Empty) {
            if let Event::Spawned { definition, .. } = e {
                spawned.push(definition);
            }
        }
    }
    spawned
}

fn json(report: &Report) -> serde_json::Value {
    serde_json::to_value(report).unwrap()
}

#[test]
fn synthetic_addon_imports_with_report() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Weapon_Synthetic_Blaster");
    let out = fresh("synthetic");
    let report = import(&Options {
        input: fixture,
        out: out.clone(),
        reference: None,
        core: vec![],
        version: "1.0.0".into(),
    })
    .unwrap();
    let r = json(&report);

    // Provenance and package identity.
    assert_eq!(report.source.title, "Synthetic Blaster");
    assert_eq!(report.source.licence_status, "known");
    assert_eq!(report.package.id, "weapon_synthetic_blaster");
    assert_eq!(report.package.packages_json_entry["side"], "shared");

    // Ids in the platform grammar, in the Add-On's own namespace.
    let ids: Vec<&str> = report.ids.iter().map(|i| i.id.as_str()).collect();
    for id in [
        "weapon_synthetic_blaster:weapon/blasteritem",
        "weapon_synthetic_blaster:image/blasterimage",
        "weapon_synthetic_blaster:projectile/blasterboltprojectile",
        "weapon_synthetic_blaster:explosion/blasterexplosion",
        "weapon_synthetic_blaster:damage_type/syntheticblaster",
        "weapon_synthetic_blaster:brick/brickblasterpaddata",
        "weapon_synthetic_blaster:brick_geometry/bricks/pad.blb",
    ] {
        assert!(ids.contains(&id), "missing {id} in {ids:?}");
        bri_package::id::ContentId::parse(id).unwrap();
    }

    // Datablocks recognised and converted.
    let status = |name: &str| {
        report
            .datablocks
            .iter()
            .find(|d| d.name == name)
            .map(|d| (d.recognised_as.clone(), d.status.clone()))
            .unwrap()
    };
    assert_eq!(status("blasterItem"), ("weapon".into(), "converted".into()));
    assert_eq!(
        status("blasterImage"),
        ("weapon_image".into(), "converted_with_gaps".into())
    );
    assert_eq!(
        status("brickBlasterPadData"),
        ("brick".into(), "converted".into())
    );
    assert_eq!(status("blasterChargeSound").1, "recognised_only");
    assert_eq!(status("blasterExplosion").1, "converted_with_gaps");

    // The dependency is named even without a reference install.
    let dep = &report.dependencies[0];
    assert_eq!(
        (dep.addon.as_str(), dep.status.as_str()),
        ("Weapon_Gun", "missing")
    );
    assert_eq!(dep.source.as_ref().unwrap().line, 4);

    // Behaviour: the custom onFire, the global override; the empty onMount is not listed.
    let fire_fn = report
        .needs_behaviour
        .iter()
        .find(|b| b.function == "blasterImage::onFire")
        .unwrap();
    assert_eq!(fire_fn.hook.kind, "image_state_script");
    assert_eq!(fire_fn.source.line, 78);
    let ops: Vec<&str> = fire_fn.operations.iter().map(|o| o.op.as_str()).collect();
    for op in [
        "spawn_projectile",
        "set_velocity",
        "random",
        "send_chat",
        "read_aim",
    ] {
        assert!(ops.contains(&op), "{op} missing from {ops:?}");
    }
    // Capabilities are the package runtime's; what it lacks is listed as missing.
    assert_eq!(fire_fn.capabilities, ["chat"]);
    for missing in ["players.move", "projectiles.spawn", "random.seeded"] {
        assert!(fire_fn.missing_capabilities.contains(&missing.to_string()));
    }
    assert!(fire_fn.hook.runtime_hook.is_none());
    assert!(
        fire_fn
            .entity_state
            .contains(&"%obj.lastBlasterShot".to_string())
    );
    let over = report
        .needs_behaviour
        .iter()
        .find(|b| b.function == "Armor::onCollision")
        .unwrap();
    assert_eq!(over.hook.kind, "global_override");
    assert!(over.blockers.iter().any(|b| b.contains("eval")));
    assert!(
        !report
            .needs_behaviour
            .iter()
            .any(|b| b.function == "blasterImage::onMount")
    );

    // Unsupported and ambiguous findings point at their source.
    let find = |list: &str, what: &str| {
        r[list]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["what"].as_str().unwrap().contains(what))
    };
    assert!(find("unsupported", "GunItem.uiName"));
    assert!(find(
        "ambiguous",
        "exec Add-Ons/Weapon_Synthetic_Blaster/missing.cs"
    ));
    assert!(find("ambiguous", "blasterExplosion.emitter[0]"));
    assert!(find("ambiguous", "blasterItem.shapefile"));
    assert_eq!(report.summary.verdict, "converted_with_gaps");

    // The package directory and its manifest.
    for f in [
        "package.json",
        "import-report.json",
        "IMPORT-REPORT.md",
        "assets/weapons.json",
        "assets/bricks.json",
    ] {
        assert!(out.join(f).is_file(), "{f} not written");
    }
    // package.json is the package runtime's manifest and the package loads
    // through its loader; the imported content is declared in content.json.
    let manifest = bri_package_runtime::manifest::Manifest::parse(
        &std::fs::read(out.join("package.json")).unwrap(),
        "weapon_synthetic_blaster",
    )
    .unwrap();
    assert_eq!(manifest.license, "CC0-1.0");
    let entry: bri_package::packages::PackageEntry =
        serde_json::from_value(report.package.packages_json_entry.clone()).unwrap();
    bri_package_runtime::Package::load(&out, &entry).unwrap();
    let content: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("assets/content.json")).unwrap()).unwrap();
    assert!(content["content"].as_array().unwrap().len() >= 7);

    // Loads in the weapons runtime and fires from data: one bolt per shot,
    // because the burst lives in the onFire behaviour the report lists.
    let shots = fire(&out, "weapon_synthetic_blaster:weapon/blasteritem");
    assert_eq!(
        shots,
        ["weapon_synthetic_blaster:projectile/blasterboltprojectile"]
    );
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
}

#[test]
fn refuses_to_overwrite_or_write_inside_the_source() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Weapon_Synthetic_Blaster");
    let options = |out: PathBuf| Options {
        input: fixture.clone(),
        out,
        reference: None,
        core: vec![],
        version: "1.0.0".into(),
    };
    assert!(import(&options(fixture.join("nested-output"))).is_err());
    let existing = fresh("existing");
    std::fs::create_dir_all(&existing).unwrap();
    assert!(import(&options(existing.clone())).is_err());
    std::fs::remove_dir_all(existing.parent().unwrap()).unwrap();
}

const ARCHIVE: &str = "C:/Users/Maxwell/Documents/_Blockland_Maxwell_1588_Archive/Addons";
const REFERENCE: &str = "E:/Downloads/B4v21Launcher/versions/Blockland v20";

#[test]
fn real_community_samples() {
    let archive = std::env::var("BRI_ADDON_ARCHIVE").unwrap_or(ARCHIVE.into());
    let reference = std::env::var("BRI_V20_REFERENCE").unwrap_or(REFERENCE.into());
    if !Path::new(&archive).is_dir() || !Path::new(&reference).is_dir() {
        eprintln!("skipped: community archive or v20 reference install not on this machine");
        return;
    }
    let run = |name: &str| {
        let out = fresh(name);
        let report = import(&Options {
            input: Path::new(&archive).join(format!("{name}.zip")),
            out: out.clone(),
            reference: Some(reference.clone().into()),
            core: vec![],
            version: "1.0.0".into(),
        })
        .unwrap();
        (report, out)
    };

    // A weapon with a custom projectile script.
    let (shotgun, out) = run("Weapon_Shotgun");
    assert_eq!(shotgun.source.authors, ["Ephialtes"]);
    assert_eq!(shotgun.source.licence_status, "unknown");
    let gun = shotgun
        .dependencies
        .iter()
        .find(|d| d.addon == "Weapon_Gun")
        .unwrap();
    assert_eq!(
        (gun.status.as_str(), gun.package.as_deref()),
        ("reference", Some("v20-weapons"))
    );
    assert!(gun.uses.iter().any(|u| u == "gunExplosion"));
    let on_fire = &shotgun.needs_behaviour[0];
    assert_eq!(on_fire.function, "shotgunImage::onFire");
    assert!(
        on_fire
            .missing_capabilities
            .contains(&"projectiles.spawn".to_string())
    );
    assert_eq!(shotgun.summary.assets_failed, 0);
    // Data alone fires one pellet; the three-pellet spread is the listed behaviour.
    assert_eq!(
        fire(&out, "weapon_shotgun:weapon/shotgunitem"),
        ["weapon_shotgun:projectile/shotgunprojectile"]
    );
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();

    // A vehicle that leans on the Jeep for effects and inherited explosions.
    let (car, out) = run("Vehicle_Blocko_Car");
    assert!(
        car.ids
            .iter()
            .any(|i| i.id == "vehicle_blocko_car:vehicle/blockocarvehicle")
    );
    assert!(
        car.dependencies
            .iter()
            .any(|d| d.addon == "Vehicle_Jeep" && d.status == "reference")
    );
    assert!(car.needs_behaviour.is_empty());
    drive(&out);
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();

    // A bot whose AI framework is another Add-On that is not installed.
    let (zombie, out) = run("Bot_Zombie");
    let hole = zombie
        .dependencies
        .iter()
        .find(|d| d.addon == "Bot_Hole")
        .unwrap();
    assert_eq!(hole.status, "missing");
    let kinds: Vec<_> = zombie
        .needs_behaviour
        .iter()
        .map(|b| b.hook.kind.as_str())
        .collect();
    assert!(kinds.contains(&"global_override") && kinds.contains(&"framework_callback"));
    let infect = zombie
        .needs_behaviour
        .iter()
        .find(|b| b.function == "holeZombieInfect")
        .unwrap();
    assert!(infect.blockers.iter().any(|b| b.contains("eval")));
    assert!(
        zombie
            .datablocks
            .iter()
            .any(|d| d.recognised_as == "bot" && d.status == "recognised_only")
    );
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
}

/// Spawns the imported car in the vehicles runtime on a flat floor, seats a
/// driver and checks that throttle moves it forward.
fn drive(package: &Path) {
    use bri_vehicles::*;
    use rapier3d::prelude::*;
    let pack = Pack::load(package.join("assets/vehicles.json")).unwrap();
    let mut v = VehiclesWorld::new(pack).unwrap();
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(500., 0.5, 500.),
    );
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: "vehicle_blocko_car:vehicle/blockocarvehicle".into(),
            transform: Transform {
                position: [0., 2., 0.],
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
        },
        seat,
    )
    .unwrap();
    let step = |v: &mut VehiclesWorld, w: &mut PhysicsWorld, n| {
        for _ in 0..n {
            v.pre_step(w, &[]).unwrap();
            w.step();
            v.post_step(w).unwrap();
        }
    };
    step(&mut v, &mut w, 240);
    v.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            throttle: 1.,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut v, &mut w, 300);
    let s = v.snapshot(&w);
    assert!(
        s.vehicles[0].transform.position[2] < -3.,
        "did not drive: {:?}",
        s.vehicles[0].transform.position
    );
}
