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
        ..Default::default()
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

    // The dependency is named even without a reference install: an
    // Add-On v20 shipped is the game's own base package.
    let dep = &report.dependencies[0];
    assert_eq!(
        (
            dep.addon.as_str(),
            dep.status.as_str(),
            dep.package.as_deref()
        ),
        ("Weapon_Gun", "base", Some("v20-weapons"))
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
    // Hiding Weapon_Gun's item runs only when the player had it off, and
    // turning this on turns Weapon_Gun on with it: noted, not a gap.
    assert!(!find("unsupported", "GunItem.uiName"));
    assert!(find("ambiguous", "GunItem.uiName"));
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
        ..Default::default()
    };
    assert!(import(&options(fixture.join("nested-output"))).is_err());
    let existing = fresh("existing");
    std::fs::create_dir_all(&existing).unwrap();
    assert!(import(&options(existing.clone())).is_err());
    std::fs::remove_dir_all(existing.parent().unwrap()).unwrap();
}

const REFERENCE: &str = "E:/Downloads/B4v21Launcher/versions/Blockland v20";

/// Maxwell's Steam copy of Blockland, whose Add-Ons folder holds the
/// community Butterfly Knife and HE Grenade.
const STEAM_ADDONS: &str = "S:/SteamLibrary/steamapps/common/Blockland/Add-Ons";

/// The listed knife and grenade ports apply to the copies a player has, read
/// their names from those scripts and pass their checks.
#[test]
fn real_steam_knife_and_grenade_ports() {
    let addons = std::env::var("BRI_STEAM_ADDONS").unwrap_or(STEAM_ADDONS.into());
    let reference = std::env::var("BRI_V20_REFERENCE").unwrap_or(REFERENCE.into());
    if !Path::new(&addons).is_dir() || !Path::new(&reference).is_dir() {
        eprintln!("skipped: Steam Add-Ons or v20 reference install not on this machine");
        return;
    }
    let ports = [
        ("Weapon_ButterflyKnife", "jab", "butterflyknifeprojectile"),
        ("Weapon_HEGrenade", "bounce_sound", "hegrenadeBounceSound"),
    ];
    for (name, value, expected) in ports {
        let out = fresh(name);
        let report = import(&Options {
            input: Path::new(&addons).join(format!("{name}.zip")),
            out: out.clone(),
            reference: Some(reference.clone().into()),
            ..Default::default()
        })
        .unwrap();
        let port = &report.ports[0];
        eprintln!(
            "{name} sha256 {} port {:?} values {:?}",
            report.source.sha256, port.reason, port.values
        );
        assert!(port.applied, "{:?}", port.reason);
        assert!(port.values[value].eq_ignore_ascii_case(expected));
        let checks: bri_addon_import::porting::Checks = serde_json::from_slice(
            &std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("ports")
                    .join(&port.port)
                    .join("checks.json"),
            )
            .unwrap(),
        )
        .unwrap();
        for (line, ok) in bri_addon_import::porting::run_checks(&out, &checks).unwrap() {
            assert!(ok, "{line}");
        }
        std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
    }
}

#[test]
fn real_community_samples() {
    // Only folders named for this run: the test reads nothing of the
    // machine's on its own. `required_framework_stays_missing` covers the
    // bot case without them.
    let (Ok(archive), Ok(reference)) = (
        std::env::var("BRI_ADDON_ARCHIVE"),
        std::env::var("BRI_V20_REFERENCE"),
    ) else {
        eprintln!("skipped: set BRI_ADDON_ARCHIVE and BRI_V20_REFERENCE to run it");
        return;
    };
    if !Path::new(&archive).is_dir() || !Path::new(&reference).is_dir() {
        eprintln!("skipped: {archive} or {reference} is not a folder");
        return;
    }
    let run = |name: &str| {
        let out = fresh(name);
        let report = import(&Options {
            input: Path::new(&archive).join(format!("{name}.zip")),
            out: out.clone(),
            reference: Some(reference.clone().into()),
            ..Default::default()
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
    // The listed port (crates/addon-import/ports) turns the onFire burst
    // into the image's shot data, read from this copy's script: three pellets.
    let port = &shotgun.ports[0];
    eprintln!(
        "Weapon_Shotgun sha256 {} port {:?} values {:?}",
        shotgun.source.sha256, port.reason, port.values
    );
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(port.values["projectiles"], "3");
    assert!(on_fire.port.as_ref().is_some_and(|p| p.applied));
    assert_eq!(
        fire(&out, "weapon_shotgun:weapon/shotgunitem"),
        ["weapon_shotgun:projectile/shotgunprojectile"; 3]
    );
    let checks: bri_addon_import::porting::Checks =
        serde_json::from_slice(include_bytes!("../ports/weapon_shotgun/checks.json")).unwrap();
    for (line, ok) in bri_addon_import::porting::run_checks(&out, &checks).unwrap() {
        assert!(ok, "{line}");
    }
    // Merged with the base game's pack, when this checkout has generated content.
    let vanilla =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/weapons-pack-009/weapons.json");
    if vanilla.is_file() {
        let base = Pack::from_json(&std::fs::read(&vanilla).unwrap()).unwrap();
        let shotgun =
            Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
        let (merged, notes) = base.merge(vec![("weapon_shotgun/assets".into(), shotgun)]);
        merged.validate().unwrap();
        assert!(merged.items.contains_key("v20.weapon.gunitem"));
        assert!(
            merged
                .items
                .contains_key("weapon_shotgun:weapon/shotgunitem")
        );
        assert!(notes.is_empty(), "{notes:?}");
    }
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

    // A WheeledVehicle with Blockland's flying fields, three wheels and a
    // propeller its scripts switch by speed.
    let (plane, out) = run("Vehicle_Stunt_Plane");
    assert_eq!(plane.summary.assets_failed, 0, "{:?}", plane.assets);
    // Its contrail images wait 10000 s, past the weapons pack's image
    // limits; they become the plane's trails instead, and the weapons stay.
    let unsupported: Vec<_> = plane.unsupported.iter().map(|u| u.what.as_str()).collect();
    assert!(
        !unsupported.iter().any(|u| u.contains("ontrail"))
            && !unsupported.contains(&"weapon lowering"),
        "{unsupported:?}"
    );
    let status = |name: &str| {
        plane
            .datablocks
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case(name))
            .map(|d| d.status.as_str())
    };
    assert_eq!(status("ContrailImage1"), Some("consumed"));
    assert_eq!(status("contrailEmitter"), Some("converted"));
    assert_eq!(status("contrailParticle"), Some("converted"));
    let pack = bri_vehicles::Pack::load(out.join("assets/vehicles.json")).unwrap();
    let d = &pack.definitions[0];
    assert_eq!(d.family, bri_vehicles::Family::Wheeled);
    let f = d.wheeled_flight.as_ref().expect("flies");
    assert_eq!(
        (f.max_forward_vel, f.stall_speed, f.sled),
        (40., 10., false)
    );
    assert!(!d.strafe_steering && d.steering.auto_return);
    // WheeledVehicleData::onAdd with three wheels steers wheel 0 alone and
    // drives the other two. The plane's hub0 is its front-left wheel (hub2
    // is the tail wheel), so in v20 too only that wheel turns.
    let wheels: Vec<_> = d.wheels.iter().map(|w| (w.steering, w.powered)).collect();
    assert_eq!(wheels, [(1., false), (0., true), (0., true)]);
    assert!(d.wheels[0].position[0] < 0. && d.wheels[0].position[2] < d.wheels[2].position[2]);
    // contrailCheck mounts contrailImage1/2 at the wing tips (mount3,
    // mount4) from minContrailSpeed 30; their FireA state runs
    // ContrailEmitter.
    let trails: Vec<_> = d
        .trails
        .iter()
        .map(|t| {
            (
                t.node.as_str(),
                t.emitter.as_str(),
                t.min_speed,
                t.max_speed,
            )
        })
        .collect();
    let emitter = "vehicle_stunt_plane:emitter/contrailemitter";
    assert_eq!(
        trails,
        [
            ("mount3", emitter, Some(30.), None),
            ("mount4", emitter, Some(30.), None)
        ]
    );
    // At the wing tips, 4.5 either side.
    let x: Vec<f32> = d.trails.iter().map(|t| t.transform.position[0]).collect();
    assert!(
        (x[0] - 4.5).abs() < 0.01 && (x[1] + 4.5).abs() < 0.01,
        "{x:?}"
    );
    let e = &d.effects.emitters[0];
    assert_eq!(
        (e.id.as_str(), e.period, e.speed, e.particles.as_slice()),
        (
            emitter,
            0.001,
            0.,
            &["vehicle_stunt_plane:particle/contrailparticle".to_owned()][..]
        )
    );
    let p = &d.effects.particles[0];
    assert_eq!(
        (p.texture.as_str(), p.lifetime, p.inherited_velocity),
        ("base/data/particles/cloud", 0.5, 0.)
    );
    let threads: Vec<_> = d
        .threads
        .iter()
        .map(|t| (t.slot, t.sequence.as_str(), t.min_speed, t.max_speed))
        .collect();
    assert_eq!(
        threads,
        [
            (0, "propslow", None, Some(5.)),
            (0, "propfast", Some(5.), None)
        ]
    );
    fly(&out, &d.id);
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

/// A bot that runs on another Add-On's AI framework (`Bot_Zombie` on
/// `Bot_Hole`), with that one absent from the reference install: it names
/// none of its datablocks, but calls its functions, so it is still missing,
/// not unused. Synthetic files in a temp folder only.
#[test]
fn required_framework_stays_missing() {
    let root = fresh("framework-source");
    let reference = root.with_file_name("reference");
    std::fs::create_dir_all(reference.join("Add-Ons")).unwrap();
    std::fs::create_dir_all(reference.join("base")).unwrap();
    let source = root.with_file_name("Bot_Synthetic_Zombie");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("description.txt"),
        "Title: Synthetic Zombie\nAuthor: tests",
    )
    .unwrap();
    std::fs::write(
        source.join("server.cs"),
        r#"
ForceRequiredAddOn("Bot_Synthetic_Hole");
function ZombieArmor::onBotLoop(%this, %obj)
{
   holeSyntheticWander(%obj);
}
"#,
    )
    .unwrap();
    let out = fresh("framework");
    let report = import(&Options {
        input: source,
        out: out.clone(),
        reference: Some(reference),
        ..Default::default()
    })
    .unwrap();
    let hole = report
        .dependencies
        .iter()
        .find(|d| d.addon == "Bot_Synthetic_Hole")
        .unwrap();
    assert_eq!(hole.status, "missing");
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

/// Spawns the imported car in the vehicles runtime on a flat floor, seats a
/// driver and checks that throttle moves it forward.
fn drive(package: &Path) {
    let (mut v, mut w) = seated(
        package,
        "vehicle_blocko_car:vehicle/blockocarvehicle",
        2.,
        0.,
    );
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

/// The imported plane launched at 45 high in the air under full throttle
/// holds its height on lift, where a car would fall about 40 in two seconds.
fn fly(package: &Path, id: &str) {
    let (mut v, mut w) = seated(package, id, 45., 45.);
    v.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            throttle: 1.,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut v, &mut w, 240);
    let p = v.snapshot(&w).vehicles[0].transform.position;
    assert!(p[1] > 38. && p[2] < -60., "did not fly: {p:?}");
}

use bri_vehicles::{Controls, OccupantId, OwnerId, VehiclesWorld};
use rapier3d::prelude::PhysicsWorld;

fn step(v: &mut VehiclesWorld, w: &mut PhysicsWorld, n: usize) {
    for _ in 0..n {
        v.pre_step(w, &[]).unwrap();
        w.step();
        v.post_step(w).unwrap();
    }
}

/// The package's vehicle `id` at `height`, moving `speed` toward its nose,
/// over a flat floor with a driver seated.
fn seated(package: &Path, id: &str, height: f32, speed: f32) -> (VehiclesWorld, PhysicsWorld) {
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
            definition: id.into(),
            transform: Transform {
                position: [0., height, 0.],
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
    let (_, b) = w.bodies.iter_mut().find(|(_, b)| b.is_dynamic()).unwrap();
    b.set_linvel(Vec3::new(0., 0., -speed), true);
    (v, w)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// Step a of the multi-package proposal: two packages' weapons merge into
/// one pack that the unchanged weapons runtime loads and fires from.
#[test]
fn imported_weapon_packs_merge_into_one_runtime_pack() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Weapon_Synthetic_Blaster");
    let first = fresh("merge-a");
    let second_src = first.parent().unwrap().join("Weapon_Second_Blaster");
    copy_dir(&fixture, &second_src);
    let second = first.parent().unwrap().join("second");
    for (input, out) in [(fixture, first.clone()), (second_src, second.clone())] {
        import(&Options {
            input,
            out,
            ..Default::default()
        })
        .unwrap();
    }
    let load =
        |p: &Path| Pack::from_json(&std::fs::read(p.join("assets/weapons.json")).unwrap()).unwrap();
    let (merged, notes) = load(&first).merge(vec![("second/assets".into(), load(&second))]);
    merged.validate().unwrap();
    assert!(
        merged
            .items
            .contains_key("weapon_synthetic_blaster:weapon/blasteritem")
    );
    assert!(
        merged
            .items
            .contains_key("weapon_second_blaster:weapon/blasteritem")
    );
    // Explosions and damage types are still keyed by bare Torque name.
    assert!(
        notes
            .iter()
            .any(|n| n.contains("explosion blasterexplosion is already declared")),
        "{notes:?}"
    );
    assert!(
        merged
            .resources
            .iter()
            .any(|r| r.package.as_deref() == Some("second/assets"))
    );
    assert!(merged.resources.iter().any(|r| r.package.is_none()));

    let mut world = WeaponsWorld::new(merged.clone()).unwrap();
    world.add_actor(ActorId(1), 5).unwrap();
    let slot = world
        .give(ActorId(1), "weapon_second_blaster:weapon/blasteritem")
        .unwrap();
    world.equip(ActorId(1), Some(slot)).unwrap();
    let mut spawned = vec![];
    for tick in 0..240 {
        if tick == 60 || tick == 61 {
            world.trigger(ActorId(1), tick == 60).unwrap();
        }
        for e in world.step(&mut Empty) {
            if let Event::Spawned { definition, .. } = e {
                spawned.push(definition);
            }
        }
    }
    assert_eq!(
        spawned,
        ["weapon_second_blaster:projectile/blasterboltprojectile"]
    );

    // A package whose projectile nobody provides loses only that weapon.
    let mut orphan = load(&second);
    orphan.projectiles.clear();
    orphan.id = "orphan".into();
    for item in orphan.items.values_mut() {
        item.id = item.id.replace("weapon_second_blaster", "orphan");
        item.image = item.image.replace("weapon_second_blaster", "orphan");
    }
    orphan.items = orphan
        .items
        .into_values()
        .map(|i| (i.id.clone(), i))
        .collect();
    orphan.images = orphan
        .images
        .into_values()
        .map(|mut i| {
            i.id = i.id.replace("weapon_second_blaster", "orphan");
            i.projectile = Some("orphan:projectile/missing".into());
            (i.id.clone(), i)
        })
        .collect();
    let (merged, notes) = merged.merge(vec![("orphan".into(), orphan)]);
    merged.validate().unwrap();
    assert!(!merged.items.keys().any(|k| k.starts_with("orphan:")));
    assert!(
        merged
            .items
            .contains_key("weapon_second_blaster:weapon/blasteritem")
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("image orphan:image/blasterimage dropped")),
        "{notes:?}"
    );
    std::fs::remove_dir_all(first.parent().unwrap()).unwrap();
}

/// A tiny mono 8-bit WAV: our own bytes, not a v20 sound.
fn wav() -> Vec<u8> {
    let samples = [128u8; 64];
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&8000u32.to_le_bytes());
    b.extend_from_slice(&8000u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    b.extend_from_slice(&samples);
    b
}

/// A gun Add-On written here, shaped like the ones modders port: its own
/// sound, muzzle emitter (with values the engine's `onAdd` corrects), an
/// explosion with a burst, light and debris, a kill icon it forgot to
/// ship, a hidden item without a `uiName` and an ammo box nobody holds.
#[test]
fn a_gun_add_on_brings_its_sounds_effects_debris_and_odd_items() {
    let root = fresh("kit-source");
    let source = root.with_file_name("Weapon_Synthetic_Kit");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("description.txt"),
        "Title: Synthetic Kit\nAuthor: Blockland ReImagined tests\nWritten for the importer tests.",
    )
    .unwrap();
    std::fs::write(source.join("server.cs"), "exec(\"./kit.cs\");\n").unwrap();
    std::fs::write(source.join("fire.wav"), wav()).unwrap();
    let mut spark = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 200, 80, 255]))
        .write_to(&mut spark, image::ImageFormat::Png)
        .unwrap();
    std::fs::write(source.join("spark.png"), spark.into_inner()).unwrap();
    std::fs::write(
        source.join("kit.cs"),
        r#"
datablock AudioDescription(kitClose2d) { volume = 0.5; isLooping = false; is3D = false; };
datablock AudioProfile(kitFireSound) { filename = "./fire.wav"; description = kitClose2d; preload = true; };
datablock ParticleData(kitSparkParticle) { textureName = "base/data/particles/cloud"; lifetimeMS = 200; };
datablock ParticleEmitterData(kitFlashEmitter)
{
   ejectionPeriodMS = 5; periodVarianceMS = 5; thetaMin = 0; thetaMax = 200;
   particles = "kitSparkParticle";
};
datablock ParticleData(kitGlowParticle) { textureName = "./spark"; lifetimeMS = 300; };
datablock ParticleEmitterData(kitGlowEmitter) { ejectionPeriodMS = 10; particles = "kitGlowParticle"; };
datablock ParticleData(kitTrailParticle) { textureName = "base/data/particles/cloud"; lifetimeMS = 400; };
datablock ParticleEmitterData(kitTrailEmitter) { ejectionPeriodMS = 20; particles = "kitTrailParticle"; };
datablock ParticleData(kitStrayParticle) { textureName = "base/data/particles/cloud"; lifetimeMS = 400; };
datablock ParticleEmitterData(kitStrayEmitter) { ejectionPeriodMS = 20; particles = "kitStrayParticle"; };
datablock DebrisData(kitShellDebris) { shapeFile = "./shell.dts"; lifetime = 2; numBounces = 3; emitters = "kitTrailEmitter"; };
datablock ExplosionData(kitBoomExplosion)
{
   lifetimeMS = 300; soundProfile = kitFireSound;
   emitter[0] = kitFlashEmitter; emitter[1] = kitGlowEmitter; emitter[4] = kitStrayEmitter;
   particleEmitter = kitFlashEmitter; particleDensity = 12; particleRadius = 0.5;
   lightStartRadius = 3; lightEndRadius = 0; lightStartColor = "1 0.5 0";
   debris = kitShellDebris; debrisNum = 2;
};
AddDamageType("KitRound", '<bitmap:add-ons/Weapon_Synthetic_Kit/ci_round> %1', '%2 <bitmap:add-ons/Weapon_Synthetic_Kit/ci_round> %1', 0.5, 1);
datablock ProjectileData(kitRoundProjectile)
{
   directDamage = 5; directDamageType = $DamageType::KitRound;
   explosion = kitBoomExplosion; particleEmitter = kitFlashEmitter;
   muzzleVelocity = 90; lifetime = 2000;
};
datablock ItemData(kitGunItem) { shapeFile = "./gun.dts"; uiName = "Kit Gun"; image = kitGunImage; canDrop = true; };
datablock ItemData(kitHiddenItem) { shapeFile = "./gun.dts"; image = kitScopeImage; };
datablock ItemData(kitAmmoItem) { shapeFile = "./ammo.dts"; uiName = "Kit Ammo"; };
datablock ShapeBaseImageData(kitGunImage)
{
   shapeFile = "./gun.dts"; item = kitGunItem; projectile = kitRoundProjectile;
   stateName[0] = "Activate"; stateTimeoutValue[0] = 0.1; stateTransitionOnTimeout[0] = "Ready";
   stateName[1] = "Ready"; stateTransitionOnTriggerDown[1] = "Fire";
   stateName[2] = "Fire"; stateFire[2] = true; stateSound[2] = kitFireSound;
   stateEmitter[2] = kitFlashEmitter; stateEmitterTime[2] = 0.05;
   stateTimeoutValue[2] = 0.2; stateTransitionOnTimeout[2] = "Ready";
};
datablock ShapeBaseImageData(kitScopeImage) { shapeFile = "./gun.dts"; stateName[0] = "Scoped"; };
"#,
    )
    .unwrap();
    let out = fresh("kit");
    let report = import(&Options {
        input: source.clone(),
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    let ns = "weapon_synthetic_kit";
    let id = |kind: &str, name: &str| format!("{ns}:{kind}/{name}");

    // Its sound plays: the state names it by id, its description's volume,
    // and not being 3D makes it the holder's alone.
    let fire = id("sound", "kitfiresound");
    let sound = pack
        .sound(&fire)
        .expect("the Add-On's sound is in its pack");
    assert_eq!(
        (sound.volume, sound.local, sound.looping),
        (0.5, true, false)
    );
    assert!(out.join("assets").join(&sound.file).is_file());
    let gun = &pack.images[&id("image", "kitgunimage")];
    assert_eq!(gun.states[2].sound, fire);
    assert_eq!(pack.explosions["kitboomexplosion"].sound, fire);

    // Its emitter, corrected as the engine's onAdd would, drawn by the
    // state and trailing the round.
    let flash = id("emitter", "kitflashemitter");
    let emitter = pack
        .effects
        .emitters
        .iter()
        .find(|e| e.id == flash)
        .unwrap();
    assert!(emitter.period_variance < emitter.period);
    assert_eq!(emitter.theta_degrees, [0.0, 180.0]);
    assert_eq!(
        pack.effects.particles[0].id,
        id("particle", "kitsparkparticle")
    );
    assert_eq!(gun.states[2].emitter, flash);
    assert_eq!(
        pack.projectiles[&id("projectile", "kitroundprojectile")].trail,
        flash
    );
    // Its explosion: emitter, burst and fading light, found by its name.
    let boom = &pack.effects.explosions[0];
    assert_eq!(boom.id, id("explosion", "kitboomexplosion"));
    assert_eq!(bri_weapons::effect_symbol(&boom.id), "kitboomexplosion");
    let glow = id("emitter", "kitglowemitter");
    assert_eq!(
        (boom.lifetime, boom.emitters.clone()),
        (0.3, vec![flash.clone(), glow.clone()])
    );
    // A particle drawing the Add-On's own texture names it as the item
    // presentation lists it, and the presentation carries the image.
    let glow_particle = pack
        .effects
        .particles
        .iter()
        .find(|p| p.id == id("particle", "kitglowparticle"))
        .unwrap();
    assert!(
        glow_particle.texture.ends_with("/spark.png")
            && !glow_particle.texture.starts_with("base/"),
        "{}",
        glow_particle.texture
    );
    let presentation: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("assets/presentation.json")).unwrap())
            .unwrap();
    assert!(
        presentation["textures"][&glow_particle.texture]["file"].is_string(),
        "{}",
        presentation["textures"]
    );
    assert_eq!(boom.burst, Some((flash.clone(), 12, 0.5)));
    assert!(boom.light.is_some());
    // And its debris.
    let debris = bri_weapons::debris::explosion_debris(&pack);
    assert!(debris.contains_key("kitboomexplosion"), "{debris:?}");
    // Its pieces trail the Add-On's own emitter, converted for them.
    let trail = id("emitter", "kittrailemitter");
    assert_eq!(debris["kitboomexplosion"].emitters, vec![trail.clone()]);
    assert!(pack.effects.emitters.iter().any(|e| e.id == trail));
    let status = |name: &str| {
        report
            .datablocks
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.status.clone())
            .unwrap_or_default()
    };
    assert_eq!(status("kitTrailEmitter"), "converted");
    // An emitter only in a fifth explosion slot was refused as v20 loaded
    // it, and so was its particle.
    assert!(!boom.emitters.iter().any(|e| e.contains("kitstray")));
    assert_eq!(
        (status("kitStrayEmitter"), status("kitStrayParticle")),
        ("consumed".to_owned(), "consumed".to_owned())
    );

    // The kill icon it forgot to ship leaves its messages, not its kills.
    let round = &pack.damage_types["kitround"];
    assert_eq!(
        (
            round.suicide_message.as_str(),
            round.murder_message.as_str()
        ),
        ("%1", "%2 %1")
    );

    // A hidden item is left out, its image kept; an ammo box is an item
    // nobody holds.
    assert!(!pack.items.contains_key(&id("weapon", "kithiddenitem")));
    assert!(pack.images.contains_key(&id("image", "kitscopeimage")));
    assert!(
        report
            .ambiguous
            .iter()
            .any(|f| f.what == "item kitHiddenItem")
    );
    assert_eq!(pack.items[&id("weapon", "kitammoitem")].image, "");
    assert_eq!(pack.items[&id("weapon", "kitammoitem")].ui_name, "Kit Ammo");
    assert_eq!(pack.items[&id("weapon", "kitgunitem")].ui_name, "Kit Gun");
    // Its sound description is read into the sound, not left over.
    let description = report.datablocks.iter().find(|d| d.name == "kitClose2d").unwrap();
    assert_eq!(description.status, "consumed", "{description:?}");
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

/// An emitter Add-On written here: an emitter with a `uiName` is one
/// players put on bricks, so it is converted with its name though nothing
/// else uses it; one without a name that nothing uses is left out, as v20
/// never drew it.
#[test]
fn a_named_emitter_is_offered_for_bricks() {
    let root = fresh("glow-source");
    let source = root.with_file_name("Emote_Synthetic_Glow");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("description.txt"),
        "Title: Synthetic Glow\nAuthor: Blockland ReImagined tests\nWritten for the importer tests.",
    )
    .unwrap();
    std::fs::write(
        source.join("server.cs"),
        r#"
datablock ParticleData(glowParticle) { textureName = "base/data/particles/cloud"; lifetimeMS = 500; };
datablock ParticleEmitterData(glowEmitter)
{
   ejectionPeriodMS = 35; ejectionOffset = 1.8; particles = "glowParticle";
   uiName = "Emote - Synthetic Glow";
};
datablock ParticleEmitterData(spareEmitter) { ejectionPeriodMS = 35; particles = "glowParticle"; };
"#,
    )
    .unwrap();
    let out = fresh("glow");
    let report = import(&Options {
        input: source.clone(),
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    let names: Vec<_> = pack
        .effects
        .emitters
        .iter()
        .map(|e| (e.id.as_str(), e.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [(
            "emote_synthetic_glow:emitter/glowemitter",
            "Emote - Synthetic Glow"
        )]
    );
    let status = |name: &str| {
        report
            .datablocks
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.status.clone())
            .unwrap()
    };
    assert_eq!(status("glowEmitter"), "converted");
    assert_eq!(status("spareEmitter"), "consumed");
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

/// Torque's load rules on a synthetic Add-On: a sound whose path its script
/// built at load (`%path @ "x.wav"`, the other Add-On's folder only when
/// `isFile` finds it) plays the Add-On's own file; a particle only an unused
/// emitter names was never drawn; a subfolder's description is not game data.
#[test]
fn built_paths_idle_particles_and_folder_descriptions() {
    let root = fresh("load-rules-source");
    let source = root.with_file_name("Weapon_Synthetic_Club");
    std::fs::create_dir_all(source.join("sounds")).unwrap();
    std::fs::create_dir_all(source.join("extra")).unwrap();
    std::fs::write(
        source.join("description.txt"),
        "Title: Synthetic Club\nAuthor: tests",
    )
    .unwrap();
    std::fs::write(source.join("extra/Description.txt"), "Title: an older part").unwrap();
    std::fs::write(source.join("sounds/swing.wav"), wav()).unwrap();
    std::fs::write(
        source.join("server.cs"),
        r#"
if(isFile("Add-Ons/Weapon_Other_Club/description.txt"))
   %path = "Add-Ons/Weapon_Other_Club/";
else
   %path = "./sounds/";
datablock AudioProfile(clubSwingSound) { filename = %path @ "swing.wav"; description = AudioClosest3d; preload = true; };
datablock ParticleData(smokeParticle) { textureName = "base/data/particles/cloud"; lifetimeMS = 500; };
datablock ParticleEmitterData(smokeEmitter) { ejectionPeriodMS = 35; particles = "smokeParticle"; };
datablock ProjectileData(clubProjectile) { directDamage = 5; muzzleVelocity = 50; lifetime = 100; };
datablock ItemData(clubItem) { shapeFile = "./club.dts"; uiName = "Club"; image = clubImage; };
datablock ShapeBaseImageData(clubImage)
{
   shapeFile = "./club.dts"; item = clubItem; projectile = clubProjectile;
   stateName[0] = "Activate"; stateTimeoutValue[0] = 0.1; stateTransitionOnTimeout[0] = "Ready";
   stateName[1] = "Ready"; stateTransitionOnTriggerDown[1] = "Fire";
   stateName[2] = "Fire"; stateFire[2] = true; stateSound[2] = clubSwingSound;
   stateTimeoutValue[2] = 0.2; stateTransitionOnTimeout[2] = "Ready";
};
"#,
    )
    .unwrap();
    let out = fresh("load-rules");
    let report = import(&Options {
        input: source.clone(),
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let status = |name: &str| {
        report
            .datablocks
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.status.clone())
            .unwrap()
    };
    assert_ne!(status("clubSwingSound"), "recognised_only");
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    assert!(
        pack.sounds
            .contains_key("weapon_synthetic_club:sound/clubswingsound"),
        "{:?}",
        pack.sounds.keys().collect::<Vec<_>>()
    );
    assert_eq!(status("smokeEmitter"), "consumed");
    assert_eq!(status("smokeParticle"), "consumed");
    let folder = report
        .assets
        .iter()
        .find(|a| a.source.ends_with("extra/Description.txt"))
        .unwrap();
    assert_eq!(folder.status, "skipped");
    assert!(
        !report
            .unsupported
            .iter()
            .any(|u| u.what.to_ascii_lowercase().contains("description")),
        "{:?}",
        report.unsupported
    );
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

/// Kaje's Sniper Rifle and Conan's Sniper Rifle Updated, from the copies on
/// Maxwell's PC (read only): the listed ports apply and each fires one round
/// a click, straight. `BRI_SNIPER_RIFLES` names other folders to look in.
#[test]
fn real_sniper_rifles() {
    let folders = std::env::var("BRI_SNIPER_RIFLES").unwrap_or(
        "S:/SteamLibrary/steamapps/common/Blockland/Add-Ons;\
         C:/Users/Maxwell/Desktop/Games/.research/sniper/glass-343"
            .into(),
    );
    let ports = [
        (
            "Weapon_Sniper_Rifle",
            "weapon_sniper_rifle",
            &include_bytes!("../ports/weapon_sniper_rifle/checks.json")[..],
        ),
        (
            "Weapon_Sniper_Rifle_Updated",
            "weapon_sniper_rifle_updated",
            &include_bytes!("../ports/weapon_sniper_rifle_updated/checks.json")[..],
        ),
    ];
    let mut found = 0;
    for (addon, port, checks) in ports {
        let copy = folders.split(';').flat_map(|f| {
            let f = Path::new(f);
            [f.join(format!("{addon}.zip")), f.join(addon)]
        });
        let Some(input) = copy.into_iter().find(|p| p.exists()) else {
            eprintln!("skipped {addon}: no copy on this machine");
            continue;
        };
        found += 1;
        let out = fresh(addon);
        let reference = Path::new(REFERENCE).is_dir().then(|| REFERENCE.into());
        let report = import(&Options {
            input,
            out: out.clone(),
            reference,
            core: vec![],
            installed: None,
            version: "1.0.0".into(),
        })
        .unwrap();
        let applied = &report.ports[0];
        eprintln!(
            "{addon} sha256 {} port {:?} values {:?}",
            report.source.sha256, applied.reason, applied.values
        );
        assert_eq!(applied.port, port);
        assert!(applied.applied, "{:?}", applied.reason);
        // Its hash is listed (ports.json `sha256`), so it is a known copy.
        assert_eq!(applied.copy, "listed");
        assert_eq!(
            report.summary.needs_behaviour,
            report.summary.needs_behaviour_ported
        );
        let checks: bri_addon_import::porting::Checks = serde_json::from_slice(checks).unwrap();
        for (line, ok) in bri_addon_import::porting::run_checks(&out, &checks).unwrap() {
            eprintln!("{line}");
            assert!(ok, "{line}");
        }
        std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
    }
    eprintln!("{found} of 2 sniper rifles found");
}

/// An Add-On that requires another community Add-On by name (Tier 2 needs
/// Tier 1) depends on the package importing that one makes; one requiring
/// a vanilla Add-On depends on the base game's package.
#[test]
fn a_required_community_add_on_becomes_a_dependency_on_its_import() {
    let root = fresh("requires").parent().unwrap().to_path_buf();
    let install = root.join("Blockland");
    let core = install.join("Add-Ons/Weapon_Core_Kit");
    std::fs::create_dir_all(&core).unwrap();
    std::fs::write(
        core.join("server.cs"),
        "datablock ProjectileData(coreRound) { muzzleVelocity = 90; };\n",
    )
    .unwrap();
    std::fs::create_dir_all(install.join("base")).unwrap();
    let addon = root.join("Weapon_Core_Extra");
    std::fs::create_dir_all(&addon).unwrap();
    std::fs::write(
        addon.join("server.cs"),
        "ForceRequiredAddOn(\"Weapon_Core_Kit\");\nForceRequiredAddOn(\"Weapon_Gun\");\n",
    )
    .unwrap();
    let report = import(&Options {
        input: addon,
        out: root.join("package"),
        reference: Some(install),
        ..Default::default()
    })
    .unwrap();
    let deps: Vec<_> = report
        .dependencies
        .iter()
        .map(|d| (d.addon.as_str(), d.status.as_str(), d.package.as_deref()))
        .collect();
    assert_eq!(
        deps,
        [
            ("Weapon_Core_Kit", "reference", Some("weapon_core_kit")),
            // Not installed and nothing of it named: v20 ran the same.
            ("Weapon_Gun", "unused", None),
        ]
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("package/package.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["dependencies"],
        serde_json::json!({ "weapon_core_kit": "*" })
    );
    std::fs::remove_dir_all(&root).unwrap();
}

/// Two Add-Ons in the reference declare the same gun image. In v20 the one
/// that loaded last before this Add-On holds: Add-Ons load in name order,
/// each after the ones it requires, and a later declaration sets its
/// fields on the same datablock. So a skin built on its required pack's gun
/// gets that pack's states (`onReload` here), merged over an earlier
/// pack's (`Ready`'s sound is kept), and never a pack that loads after it.
#[test]
fn a_name_two_add_ons_declare_is_the_one_loaded_last_before_this_one() {
    let root = fresh("shared-name");
    let reference = root.with_file_name("reference");
    let gun = |dir: &str, states: &str| {
        let d = reference.join("Add-Ons").join(dir);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("server.cs"),
            format!(
                "datablock ItemData(sharedGunItem) {{ uiName = \"{dir}\"; image = sharedGunImage; }};\n\
                 datablock ShapeBaseImageData(sharedGunImage)\n{{\n   item = sharedGunItem;\n{states}}};\n"
            ),
        )
        .unwrap();
    };
    // Loads first by name, with its own Ready sound and a third state.
    gun(
        "Weapon_Aaa_Other",
        "   stateName[0] = \"Ready\"; stateSound[0] = otherSound;\n   stateName[2] = \"Extra\";\n",
    );
    // The pack the skin requires: its reload state names a script.
    gun(
        "Weapon_Mmm_Host",
        "   stateName[0] = \"Ready\"; stateTransitionOnTriggerDown[0] = \"Reload\";\n   stateName[1] = \"Reload\"; stateScript[1] = \"onReload\";\n",
    );
    // Loads after the skin and is not required: never seen by it.
    gun(
        "Weapon_Zzz_Later",
        "   stateName[1] = \"Reload\"; stateScript[1] = \"onLater\";\n",
    );
    std::fs::create_dir_all(reference.join("base")).unwrap();
    let source = root.with_file_name("Weapon_Nnn_Skin");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("description.txt"), "Title: Skin\nAuthor: tests").unwrap();
    std::fs::write(
        source.join("server.cs"),
        r#"
ForceRequiredAddOn("Weapon_Mmm_Host");
datablock ItemData(skinGunItem : sharedGunItem) { uiName = "Skin"; image = skinGunImage; };
datablock ShapeBaseImageData(skinGunImage : sharedGunImage) { item = skinGunItem; };
function skinGunImage::onReload(%this, %obj, %slot)
{
   %obj.playThread(2, shiftUp);
}
"#,
    )
    .unwrap();
    let out = fresh("shared-name-out");
    let report = import(&Options {
        input: source.clone(),
        out: out.clone(),
        reference: Some(reference.clone()),
        ..Default::default()
    })
    .unwrap();
    let reload = report
        .needs_behaviour
        .iter()
        .find(|b| b.function == "skinGunImage::onReload")
        .unwrap();
    assert_eq!(reload.hook.kind, "image_state_script", "{:?}", reload.hook);
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    let image = &pack.images["weapon_nnn_skin:image/skingunimage"];
    let names: Vec<&str> = image.states.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Ready", "Reload", "Extra"]);
    assert_eq!(image.states[1].script, "onReload");
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(&reference);
    let _ = std::fs::remove_dir_all(&source);
}

/// A sound an Add-On downloads from a website when it runs is not in the
/// copy and not a gap in its port: the report says it needs that download,
/// apart from a sound file the copy simply lacks.
#[test]
fn a_sound_the_add_on_downloads_is_reported_as_external() {
    let root = fresh("jingle-source");
    let source = root.with_file_name("Script_Synthetic_Jingle");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("server.cs"), "exec(\"./jingle.cs\");\n").unwrap();
    std::fs::write(
        source.join("jingle.cs"),
        r#"
%music = "config/client/temp/jingle.ogg";
if(!isFile(%music))
    connectToUrl("http://example.invalid/jingle.ogg", "GET", %music);
datablock AudioProfile(JingleMusic) { fileName = "config/client/temp/jingle.ogg"; description = AudioClosest3d; preload = true; };
datablock AudioProfile(JingleBell) { fileName = "./bell.wav"; description = AudioClosest3d; preload = true; };
"#,
    )
    .unwrap();
    let out = fresh("jingle");
    let report = import(&Options {
        input: source.clone(),
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let status = |name: &str| {
        let d = report.datablocks.iter().find(|d| d.name == name).unwrap();
        (d.status.clone(), d.notes.join("; "))
    };
    let (music, notes) = status("JingleMusic");
    assert_eq!(music, "external", "{notes}");
    assert!(notes.contains("downloaded from an external site, not in the copy"), "{notes}");
    assert_eq!(status("JingleBell").0, "recognised_only");
    assert_eq!(report.summary.datablocks_external, 1);
    assert_eq!(report.summary.datablocks_recognised_only, 1);
    assert!(report.markdown().contains("Datablocks needing a download (not in the copy) | 1"));
    std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
