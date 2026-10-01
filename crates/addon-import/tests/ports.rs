//! Native ports (`crates/addon-import/ports`). Content-free: the stand-in
//! shotgun in `tests/fixtures/ports` is ours (CC0) and has the community
//! Sawn-off Shotgun's folder name and onFire shape, so the listed port
//! applies to it. The real Add-On runs in `import.rs` `real_community_samples`
//! where Maxwell's archive exists.
use bri_addon_import::{Options, import, import_with, porting, ports::Ports};
use bri_weapons::*;
use glam::Vec3;
use std::path::{Path, PathBuf};

fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bri-addon-ports-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn options(input: PathBuf, out: PathBuf) -> Options {
    Options {
        input,
        out,
        reference: None,
        core: vec![],
        version: "1.0.0".into(),
    }
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

/// One click of the imported weapon, aimed down -Z at rest: the velocities
/// of the projectiles it spawned and the recoil it asked for.
fn click(package: &Path, item: &str) -> (Vec<Vec3>, Vec<Vec3>) {
    let pack =
        Pack::from_json(&std::fs::read(package.join("assets/weapons.json")).unwrap()).unwrap();
    let mut world = WeaponsWorld::new(pack).unwrap();
    world.add_actor(ActorId(1), 5).unwrap();
    let slot = world.give(ActorId(1), item).unwrap();
    world.equip(ActorId(1), Some(slot)).unwrap();
    let (mut spawned, mut recoil) = (vec![], vec![]);
    for tick in 0..240 {
        if tick == 60 || tick == 61 {
            world.trigger(ActorId(1), tick == 60).unwrap();
        }
        for e in world.step(&mut Empty) {
            match e {
                Event::Spawned { velocity, .. } => spawned.push(velocity),
                Event::Recoil { velocity, .. } => recoil.push(velocity),
                _ => {}
            }
        }
    }
    (spawned, recoil)
}

const ITEM: &str = "weapon_shotgun:weapon/shotgunitem";

#[test]
fn builtin_ports_list_loads_and_names_its_tests() {
    let ports = Ports::builtin();
    assert!(!ports.list.ports.is_empty());
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for e in &ports.list.ports {
        assert!(!e.tests.is_empty(), "{} lists no test", e.addon);
        for t in &e.tests {
            // A port's own checks (`check-port`), or `path test_name`.
            let Some((file, name)) = t.split_once(' ') else {
                assert!(
                    root.join("crates/addon-import/ports").join(t).is_file(),
                    "{t} does not exist"
                );
                continue;
            };
            let text = std::fs::read_to_string(root.join(file)).unwrap();
            assert!(text.contains(&format!("fn {name}()")), "{t} does not exist");
        }
    }
}

/// v20's spread code for this copy: 5 shells, `%spread` 0.002, recoil 4.
#[test]
fn shotgun_port_fires_the_spread() {
    let dir = fresh("shotgun");
    let out = dir.join("package");
    let report = import(&options(fixture("ports/Weapon_Shotgun"), out.clone())).unwrap();

    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(
        (port.port.as_str(), port.status.as_str(), port.copy.as_str()),
        ("weapon_shotgun", "verified", "unlisted")
    );
    assert_eq!(port.values["projectiles"], "5");
    assert_eq!(port.values["spread"], "0.002");
    assert_eq!(port.values["recoil"], "4");
    let on_fire = &report.needs_behaviour[0];
    assert_eq!(on_fire.function, "shotgunImage::onFire");
    assert!(on_fire.port.as_ref().is_some_and(|p| p.applied));
    assert_eq!(report.summary.needs_behaviour_ported, 1);
    let md = std::fs::read_to_string(out.join("IMPORT-REPORT.md")).unwrap();
    assert!(
        md.contains("**Ported** by `weapon_shotgun` (verified)"),
        "{md}"
    );
    // The presentation still pins the patched weapons pack, or hosts and
    // players would refuse to load the Add-On.
    bri_addon_import::ports::check_pins(&out).unwrap();
    assert!(
        port.files_changed
            .iter()
            .any(|f| f == "assets/presentation.json")
    );

    // The recoil lands first (the pellets inherit it), then five pellets
    // turned by up to ±5π·0.002 rad about each axis, all different.
    let (pellets, recoil) = click(&out, ITEM);
    assert_eq!(recoil, [Vec3::Z * 4.0]);
    assert_eq!(pellets.len(), 5);
    let aim = Vec3::NEG_Z;
    let max = 3f32.sqrt() * 5.0 * std::f32::consts::PI * 0.002;
    for v in &pellets {
        assert!((v.length() - 76.0).abs() < 1e-3, "{v}");
        assert!(v.angle_between(aim) <= max + 1e-4, "{v}");
    }
    for (i, a) in pellets.iter().enumerate() {
        assert!(pellets[i + 1..].iter().all(|b| a.distance(*b) > 1e-3));
    }
    assert!(pellets.iter().any(|v| v.angle_between(aim) > 0.005));
    std::fs::remove_dir_all(dir).unwrap();
}

/// A copy whose script does not match keeps the one-pellet data import,
/// and the report names the port and what did not match.
#[test]
fn a_copy_that_does_not_match_is_named_not_patched() {
    let dir = fresh("mismatch");
    let copy = dir.join("Weapon_Shotgun");
    std::fs::create_dir_all(&copy).unwrap();
    for f in ["server.cs", "description.txt", "LICENSE.txt"] {
        let text = std::fs::read_to_string(fixture("ports/Weapon_Shotgun").join(f)).unwrap();
        let text = text.replace("%shellcount = 5;", "%shellcount = %this.shells;");
        std::fs::write(copy.join(f), text).unwrap();
    }
    let out = dir.join("package");
    let report = import(&options(copy, out.clone())).unwrap();
    let port = &report.ports[0];
    assert!(!port.applied);
    assert!(
        port.reason.as_deref().unwrap().contains("`projectiles`"),
        "{:?}",
        port.reason
    );
    assert!(port.files_changed.is_empty());
    assert!(
        report.needs_behaviour[0]
            .port
            .as_ref()
            .is_some_and(|p| !p.applied)
    );
    assert_eq!(report.summary.needs_behaviour_ported, 0);
    assert_eq!(click(&out, ITEM).0.len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

/// A port may add files and patch the manifest; Add-Ons without a listed
/// port are untouched.
#[test]
fn ports_add_files_and_leave_other_add_ons_alone() {
    let dir = fresh("custom");
    let ports_dir = dir.join("ports");
    std::fs::create_dir_all(ports_dir.join("blaster/files/docs")).unwrap();
    std::fs::write(
        ports_dir.join("ports.json"),
        r#"{ "schema_version": 1, "ports": [ {
            "addon": "Weapon_Synthetic_Blaster", "title": "Synthetic Blaster",
            "port": "blaster", "status": "partial",
            "covers": { "blasterImage::onFire": { "shots": "%i\\s*<\\s*(\\d+)" } } } ] }"#,
    )
    .unwrap();
    std::fs::write(
        ports_dir.join("blaster/port.json"),
        r#"{ "schema_version": 1, "patch": {
            "package.json": { "description": "Ported: {shots} bolts" },
            "assets/weapons.json": { "images": { "weapon_synthetic_blaster:image/blasterimage":
                { "shot": { "projectiles": "{shots}" } } } } } }"#,
    )
    .unwrap();
    std::fs::write(ports_dir.join("blaster/files/docs/PORT.md"), "notes").unwrap();
    let ports = Ports::from_dir(&ports_dir).unwrap();

    let out = dir.join("package");
    let report = import_with(
        &options(fixture("Weapon_Synthetic_Blaster"), out.clone()),
        &ports,
    )
    .unwrap();
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    assert_eq!(
        std::fs::read_to_string(out.join("docs/PORT.md")).unwrap(),
        "notes"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("package.json")).unwrap()).unwrap();
    assert_eq!(manifest["description"], "Ported: 3 bolts");
    assert!(report.package.files.iter().any(|f| f == "docs/PORT.md"));
    bri_addon_import::ports::check_pins(&out).unwrap();
    // `partial`: the other hooks still need behaviour, so gaps remain.
    assert_eq!(report.summary.verdict, "converted_with_gaps");
    assert_eq!(
        click(&out, "weapon_synthetic_blaster:weapon/blasteritem")
            .0
            .len(),
        3
    );

    // The built-in list has no port for the synthetic blaster.
    let plain = import(&options(
        fixture("Weapon_Synthetic_Blaster"),
        dir.join("plain"),
    ))
    .unwrap();
    assert!(plain.ports.is_empty());
    assert!(plain.needs_behaviour.iter().all(|b| b.port.is_none()));
    std::fs::remove_dir_all(dir).unwrap();
}

/// A port's host rules become a companion Add-On beside the import: only
/// the host loads it, it loads as the game reads any Add-On, and turning
/// the import on turns its rules on after it.
#[test]
fn port_rules_become_a_host_only_companion_turned_on_with_the_import() {
    let dir = fresh("rules");
    let ports_dir = dir.join("ports");
    std::fs::create_dir_all(ports_dir.join("blaster/rules")).unwrap();
    std::fs::write(
        ports_dir.join("ports.json"),
        r#"{ "schema_version": 1, "ports": [ {
            "addon": "Weapon_Synthetic_Blaster", "title": "Synthetic Blaster",
            "port": "blaster", "status": "partial",
            "covers": { "blasterImage::onFire": { "shots": "%i\\s*<\\s*(\\d+)" } } } ] }"#,
    )
    .unwrap();
    std::fs::write(
        ports_dir.join("blaster/port.json"),
        r#"{ "schema_version": 1, "rules": { "capabilities": ["chat"] }, "patch": {
            "assets/weapons.json": { "images": { "{namespace}:image/blasterimage":
                { "command": "{rules}:zap" } } } } }"#,
    )
    .unwrap();
    std::fs::write(
        ports_dir.join("blaster/rules/behaviour.json"),
        r#"{ "schema_version": 1, "script": "zap.rhai", "commands": [ { "name": "zap" } ] }"#,
    )
    .unwrap();
    std::fs::write(
        ports_dir.join("blaster/rules/zap.rhai"),
        "fn shots() { {{shots}} }\nfn cmd_zap(player) { tell(player, \"{{namespace}} fires \" + shots()); }\n",
    )
    .unwrap();
    let ports = Ports::from_dir(&ports_dir).unwrap();

    let root = dir.join("content");
    let out = root.join("addons/weapon_synthetic_blaster");
    let report = import_with(
        &options(fixture("Weapon_Synthetic_Blaster"), out.clone()),
        &ports,
    )
    .unwrap();
    let applied = &report.ports[0];
    assert!(applied.applied, "{:?}", applied.reason);
    let rules = applied.rules.as_ref().expect("the port has rules");
    assert_eq!(rules.id, "weapon_synthetic_blaster-rules");
    assert_eq!(rules.packages_json_entry["side"], "server");
    let rules_dir = root.join("addons/weapon_synthetic_blaster-rules");
    assert_eq!(
        std::fs::read_to_string(rules_dir.join("zap.rhai")).unwrap(),
        "fn shots() { 3 }\nfn cmd_zap(player) { tell(player, \"weapon_synthetic_blaster fires \" + shots()); }\n"
    );
    // The image fires the rules' command; the import names its rules.
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    assert_eq!(
        pack.images["weapon_synthetic_blaster:image/blasterimage"]
            .command
            .as_deref(),
        Some("weapon_synthetic_blaster-rules:zap")
    );
    bri_addon_import::ports::check_pins(&out).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("package.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["companions"],
        serde_json::json!(["weapon_synthetic_blaster-rules"])
    );

    // The library finds both; the rules are host-only, and turning the
    // import on turns its rules on after it.
    use bri_package::{library::Library, packages::Side};
    let mut library = Library::scan(&root).unwrap();
    let companion = library.get("weapon_synthetic_blaster-rules").unwrap();
    assert_eq!(companion.package.side, Side::Server);
    assert!(!companion.has_errors(), "{:?}", companion.problems);
    let plan = library.plan("weapon_synthetic_blaster", true);
    assert!(plan.allowed(), "{:?}", plan.refused);
    assert_eq!(plan.also, ["weapon_synthetic_blaster-rules"]);
    library.apply(&plan).unwrap();
    let on: Vec<&str> = library
        .entries
        .iter()
        .filter(|e| e.enabled && e.package.id.starts_with("weapon_synthetic"))
        .map(|e| e.id())
        .collect();
    assert_eq!(on, ["weapon_synthetic_blaster", "weapon_synthetic_blaster-rules"]);
    // Off again: the rules go with it.
    let plan = library.plan("weapon_synthetic_blaster", false);
    assert_eq!(plan.also, ["weapon_synthetic_blaster-rules"]);

    // The rules load as the game loads any host Add-On.
    let set = bri_package::packages::PackageSet {
        schema_version: 1,
        packages: ["weapon_synthetic_blaster", "weapon_synthetic_blaster-rules"]
            .iter()
            .map(|id| library.get(id).unwrap().package.clone())
            .collect(),
    };
    bri_package_runtime::Catalog::load(&root, &set, true)
        .unwrap_or_else(|e| panic!("{e:#?}"));

    // A second import may not land on the first one's rules.
    let again = import_with(
        &options(
            fixture("Weapon_Synthetic_Blaster"),
            root.join("addons/weapon_synthetic_blaster"),
        ),
        &ports,
    );
    assert!(again.is_err());
    assert_eq!(
        library.import_dir("Weapon_Synthetic_Blaster"),
        "addons/weapon_synthetic_blaster-2"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Hosted: the ported shotgun fires its five pellets through the session and
/// the recoil moves the shooter, on a flat synthetic ground.
#[test]
fn ported_shotgun_recoils_the_shooter_in_a_hosted_game() {
    use bri_sim::session::{Command, Session};
    use rapier3d::prelude::*;
    let dir = fresh("hosted");
    let out = dir.join("package");
    import(&options(fixture("ports/Weapon_Shotgun"), out.clone())).unwrap();
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    let mut session = Session::new(
        bri_sim::simulation::Simulation::new(
            bri_world::World::new("Ports".into(), "ports".into(), vec![[1.0; 4]]),
            bri_sim::definitions::Definitions {
                entries: Default::default(),
            },
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    let spawn = Vec3::new(0.0, 0.05, 0.0);
    session.set_spawn_points(vec![spawn]).unwrap();
    session.set_weapon_pack(pack).unwrap();
    let player = session.join("Tester".into(), spawn, false).unwrap();
    session.give_item(player, ITEM).unwrap();
    let slot = session.tool_inventories()[&player]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(ITEM))
        .unwrap();
    session
        .command(player, 1, Command::EquipTool { slot: Some(slot) })
        .unwrap();
    let speed = |s: &Session| {
        let p = s
            .snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == player)
            .unwrap();
        Vec3::from(p.velocity).length()
    };
    let mut pellets = std::collections::BTreeSet::new();
    let (mut before, mut after) = (0.0f32, 0.0f32);
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
        for p in session.weapon_view().fired() {
            if p.definition == "weapon_shotgun:projectile/shotgunprojectile" {
                pellets.insert(p.id);
            }
        }
        if (100..120).contains(&tick) {
            before = before.max(speed(&session));
        } else if tick >= 120 {
            after = after.max(speed(&session));
        }
    }
    assert_eq!(pellets.len(), 5);
    assert!(before < 0.5, "standing still before the shot: {before}");
    assert!(after > 3.0, "recoil of 4 moves the shooter: {after}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// `port` drafts the stand-in shotgun completely; `check-port` proves it and
/// prints a verified entry naming this copy's hash.
#[test]
fn port_command_drafts_a_spread_weapon_and_check_port_verifies_it() {
    let dir = fresh("scaffold");
    let work = dir.join("work");
    let s = porting::scaffold(&fixture("ports/Weapon_Shotgun"), &work, None, vec![]).unwrap();
    assert_eq!(s.drafted, ["shotgunImage::onFire"]);
    assert!(s.to_port.is_empty());
    assert_eq!(s.listed.as_deref(), Some("weapon_shotgun (verified)"));
    for f in [
        "AGENT.md",
        "original/server.cs",
        "imported/IMPORT-REPORT.md",
        "imported/assets/weapons.json",
        "port/port.json",
        "port/checks.json",
        "entry.json",
        "stubs.rhai",
        "work.json",
    ] {
        assert!(work.join(f).is_file(), "{f}");
    }
    // The plain import is left unported, for the patches to apply to.
    let plain: serde_json::Value =
        serde_json::from_slice(&std::fs::read(work.join("imported/assets/weapons.json")).unwrap())
            .unwrap();
    assert!(plain["images"]["weapon_shotgun:image/shotgunimage"]["shot"].is_null());
    let checks: porting::Checks =
        serde_json::from_slice(&std::fs::read(work.join("port/checks.json")).unwrap()).unwrap();
    assert_eq!(checks.checks[0].fire, ITEM);
    assert_eq!(checks.checks[0].projectiles, Some(5));
    assert_eq!(checks.checks[0].recoil, Some(4.0));

    let c = porting::check(&work).unwrap();
    assert!(c.passed(), "{:?} {:?}", c.reason, c.results);
    assert!(c.unported.is_empty());
    assert!(
        c.results[0].0.contains("the Add-On loads"),
        "{:?}",
        c.results
    );
    assert_eq!(c.entry.status, "verified");
    assert_eq!(
        c.entry.sha256,
        std::slice::from_ref(&c.report.source.sha256)
    );
    assert_eq!(c.entry.tests, ["weapon_shotgun/checks.json"]);
    let submitted: bri_addon_import::ports::Entry =
        serde_json::from_slice(&std::fs::read(work.join("submit.json")).unwrap()).unwrap();
    assert_eq!(submitted.status, "verified");
    // Checks catch a port that does not do what v20 does.
    let mut wrong = checks.clone();
    wrong.checks[0].projectiles = Some(4);
    std::fs::write(
        work.join("port/checks.json"),
        serde_json::to_vec(&wrong).unwrap(),
    )
    .unwrap();
    assert!(!porting::check(&work).unwrap().passed());
    std::fs::remove_dir_all(dir).unwrap();
}

/// A weapon with its own burst code is not drafted: the stubs quote it, and
/// check-port refuses until the porter says what the port covers. A hand
/// port of one function checks as partial.
#[test]
fn hand_ports_start_from_stubs_and_check_as_partial() {
    let dir = fresh("hand");
    let work = dir.join("work");
    let s = porting::scaffold(&fixture("Weapon_Synthetic_Blaster"), &work, None, vec![]).unwrap();
    assert!(s.drafted.is_empty());
    assert!(s.to_port.iter().any(|f| f == "blasterImage::onFire"));
    let stubs = std::fs::read_to_string(work.join("stubs.rhai")).unwrap();
    assert!(
        stubs.contains("messageClient(%obj.client, '', \"Blaster burst!\");"),
        "{stubs}"
    );
    assert!(stubs.contains("fn blasterimage__onfire()"));
    let agent = std::fs::read_to_string(work.join("AGENT.md")).unwrap();
    assert!(agent.contains("bri-import-addon check-port"));
    let err = porting::check(&work).unwrap_err().to_string();
    assert!(err.contains("covers no function"), "{err}");

    // What an agent writes after reading the stub.
    let mut entry: bri_addon_import::ports::Entry =
        serde_json::from_slice(&std::fs::read(work.join("entry.json")).unwrap()).unwrap();
    entry.covers.insert(
        "blasterImage::onFire".into(),
        [("bolts".to_string(), r"%i\s*<\s*(\d+)".to_string())].into(),
    );
    std::fs::write(work.join("entry.json"), serde_json::to_vec(&entry).unwrap()).unwrap();
    std::fs::write(
        work.join("port/port.json"),
        r#"{ "schema_version": 1, "patch": { "assets/weapons.json": { "images": {
            "weapon_synthetic_blaster:image/blasterimage": { "shot": { "projectiles": "{bolts}", "recoil": 2 } } } } } }"#,
    )
    .unwrap();
    let mut checks: porting::Checks =
        serde_json::from_slice(&std::fs::read(work.join("port/checks.json")).unwrap()).unwrap();
    assert_eq!(checks.checks.len(), 1);
    checks.checks[0].projectiles = Some(3);
    checks.checks[0].recoil = Some(2.0);
    std::fs::write(
        work.join("port/checks.json"),
        serde_json::to_vec(&checks).unwrap(),
    )
    .unwrap();
    let c = porting::check(&work).unwrap();
    assert!(c.passed(), "{:?} {:?}", c.reason, c.results);
    assert_eq!(c.entry.status, "partial");
    assert!(
        c.unported.iter().any(|f| f == "Armor::onCollision"),
        "{:?}",
        c.unported
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// The same flow through the shipped executable, as the release folder runs it.
#[test]
fn port_and_check_port_run_from_the_executable() {
    let dir = fresh("exe");
    let work = dir.join("work");
    let exe = env!("CARGO_BIN_EXE_bri-import-addon");
    let run =
        |args: &[&std::ffi::OsStr]| std::process::Command::new(exe).args(args).output().unwrap();
    let port = run(&[
        "port".as_ref(),
        fixture("ports/Weapon_Shotgun").as_os_str(),
        work.as_os_str(),
    ]);
    let text = String::from_utf8_lossy(&port.stdout);
    assert!(
        port.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&port.stderr)
    );
    assert!(text.contains("drafted: shotgunImage::onFire"), "{text}");
    let check = run(&["check-port".as_ref(), work.as_os_str()]);
    let text = String::from_utf8_lossy(&check.stdout);
    assert!(
        check.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(
        text.contains("PASS fire weapon_shotgun:weapon/shotgunitem"),
        "{text}"
    );
    assert!(text.contains("\"status\": \"verified\""), "{text}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The stand-in Player Throwing (`tests/fixtures/ports/Script_PlayerThrowing`,
/// CC0) imported into `root/addons`, its host rules beside it, both turned
/// on: the packages a host loads.
fn throwing_import(root: &Path) -> bri_package::packages::PackageSet {
    use bri_package::library::Library;
    let report = import(&options(
        fixture("ports/Script_PlayerThrowing"),
        root.join("addons/script_playerthrowing"),
    ))
    .unwrap();
    let applied = &report.ports[0];
    assert!(applied.applied, "{:?}", applied.reason);
    let mut library = Library::scan(root).unwrap();
    let plan = library.plan("script_playerthrowing", true);
    assert!(plan.allowed(), "{:?}", plan.refused);
    assert_eq!(plan.also, ["script_playerthrowing-rules"]);
    library.apply(&plan).unwrap();
    bri_package::packages::PackageSet {
        schema_version: 1,
        packages: ["script_playerthrowing", "script_playerthrowing-rules"]
            .iter()
            .map(|id| library.get(id).unwrap().package.clone())
            .collect(),
    }
}

/// The listed port reads this copy's own numbers (held scale, reach, look
/// limits, throw clamp, charge notches, the front check, the set-down
/// rule, both animations) into the host rules it writes beside the import.
#[test]
fn player_throwing_port_becomes_host_rules_with_this_copys_numbers() {
    let dir = fresh("throwing");
    let root = dir.join("content");
    let set = throwing_import(&root);
    let rules =
        std::fs::read_to_string(root.join("addons/script_playerthrowing-rules/throwing.rhai"))
            .unwrap();
    for line in [
        "fn node() { 0 }",
        "fn held_scale() { parse_float(\"0.75\") }",
        "fn reach() { parse_float(\"3\") }",
        "fn look_up() { parse_float(\"0.6\") }",
        "fn look_down() { parse_float(\"0.4\") }",
        "fn min_amount() { 1 }",
        "fn max_amount() { 30 }",
        "fn max_charge() { 10 }",
        "fn front_reach() { parse_float(\"2.5\") }",
        "fn down_look() { parse_float(\"-0.85\") }",
        "fn ground() { parse_float(\"0.3\") }",
        "fn held_sequence() { \"death1\" }",
        "fn holder_sequence() { \"armReadyBoth\" }",
        "fn orbit_distance() { parse_float(\"6\") }",
        "fn orbit_nearest() { parse_float(\"4\") }",
        "fn orbit_farthest() { parse_float(\"9\") }",
        "let half = parse_float(\"1.5708\") / 2.0;",
        "let z = parse_float(\"1\") * half.sin();",
    ] {
        assert!(rules.contains(line), "{line} missing from\n{rules}");
    }
    assert!(!rules.contains("{{"), "every value is filled");
    // The rules compile, with every hook and policy they name.
    let catalog =
        bri_package_runtime::Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    if let Err(problems) = bri_package_runtime::script::Runtime::compile(&catalog) {
        panic!("{problems:#?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

/// Hosted, on flat ground: in a minigame, an empty-hand click lifts the
/// player in front onto the hand (shrunk, limp, looking within the copy's
/// limits, bricks put away, their camera circling the holder); neither may
/// switch tools or take bricks in hand while held; the held player struggles free only after 3 s
/// and has their body's view back; after the 5 s grab timeout the next
/// grab, a held fire button and its release throw them at 2.5 times the
/// full charge.
#[test]
fn ported_player_throwing_grabs_throws_and_lets_go_in_a_hosted_game() {
    use bri_content::shape::{Node, Shape};
    use bri_sim::{
        player::MoveInput,
        session::{Command, ControlObject, MiniGameRequest, Notice, Session, shape_mount_points},
    };
    use rapier3d::prelude::*;
    use std::collections::BTreeMap;

    let dir = fresh("throwing-hosted");
    let root = dir.join("content");
    let set = throwing_import(&root);
    let catalog = std::sync::Arc::new(
        bri_package_runtime::Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")),
    );
    let mut s = Session::new(
        bri_sim::simulation::Simulation::new(
            bri_world::World::new("Ports".into(), "ports".into(), vec![[1.0; 4]]),
            bri_sim::definitions::Definitions {
                entries: Default::default(),
            },
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    let node = |name: &str, parent: Option<usize>, translation: [f32; 3]| Node {
        name: name.into(),
        parent,
        translation,
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    // A Blockhead's hands: mount0 right, mount1 left.
    let body = Shape {
        schema_version: 1,
        id: "v20.shape.m".into(),
        nodes: vec![
            node("root", None, [0.0; 3]),
            node("chest", Some(0), [0.0, 1.5, 0.0]),
            node("mount0", Some(1), [0.5, 0.2, -0.3]),
            node("mount1", Some(1), [-0.5, 0.2, -0.3]),
        ],
        objects: vec![],
        details: vec![],
        meshes: vec![],
        materials: vec![],
        animations: vec![],
    };
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s.set_body_mount_points("v20.shape.m", shape_mount_points(&body))
        .unwrap();
    s.install_packages(catalog, None).unwrap();

    let mut seq: BTreeMap<u64, u64> = BTreeMap::new();
    let mut moves: BTreeMap<u64, u64> = BTreeMap::new();
    let mut inputs: BTreeMap<u64, MoveInput> = BTreeMap::new();
    let mut cmd = |s: &mut Session, owner: u64, c: Command| {
        let n = seq.entry(owner).or_default();
        *n += 1;
        s.command(owner, *n, c)
    };
    let mut steps = |s: &mut Session, inputs: &BTreeMap<u64, MoveInput>, n: usize| {
        for _ in 0..n {
            for (owner, input) in inputs {
                let m = moves.entry(*owner).or_default();
                *m += 1;
                let _ = s.movement(*owner, *m, *input);
            }
            s.step().unwrap();
        }
    };
    let state = |s: &Session, owner: u64| {
        s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .map(|(p, _)| p)
            .unwrap()
    };
    let feet = |s: &Session, owner: u64| Vec3::from(state(s, owner).feet);

    let holder = s
        .join("Holder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let held = s
        .join("Held".into(), Vec3::new(0.0, 0.05, -1.5), false)
        .unwrap();
    inputs.insert(holder, MoveInput::default());
    inputs.insert(held, MoveInput::default());
    steps(&mut s, &inputs, 2);
    // A minigame where weapons hurt, both in it where they stand.
    s.set_spawn_points(vec![feet(&s, holder)]).unwrap();
    cmd(
        &mut s,
        holder,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings::default(),
        }),
    )
    .unwrap();
    let game = s.minigame_views()[0].id;
    s.set_spawn_points(vec![feet(&s, held)]).unwrap();
    cmd(
        &mut s,
        held,
        Command::MiniGame(MiniGameRequest::Join { game }),
    )
    .unwrap();
    steps(&mut s, &inputs, 330);
    let aim = |s: &mut Session, inputs: &mut BTreeMap<u64, MoveInput>| {
        let at = feet(s, held) + Vec3::Y * 1.3 - (feet(s, holder) + Vec3::Y * 2.1);
        let flat = Vec3::new(at.x, 0.0, at.z).length();
        let input = inputs.get_mut(&holder).unwrap();
        input.yaw = at.x.atan2(-at.z);
        input.pitch = at.y.atan2(flat);
    };
    aim(&mut s, &mut inputs);
    steps(&mut s, &inputs, 2);

    // The held player has bricks in hand.
    cmd(
        &mut s,
        held,
        Command::BrickHand(bri_sim::session::BrickHand {
            stocked: true,
            equipped: true,
            ghost: false,
        }),
    )
    .unwrap();
    s.take_private_notices();

    // Lifted onto the right hand.
    cmd(&mut s, holder, Command::Activate).unwrap();
    steps(&mut s, &inputs, 1);
    let vitals = s.vitals();
    let ride = vitals[&held].ride.expect("held");
    assert_eq!((ride.mount, ride.seat), (holder, 0));
    assert!((state(&s, held).scale - 0.75).abs() < 1e-4);
    assert_eq!(vitals[&held].look_limits, Some([0.4, 0.6]));
    // Their camera circles the holder, 6 units out, and stays there.
    assert_eq!(
        vitals[&held].control,
        ControlObject::Orbit {
            target: holder,
            min: 4,
            max: 9,
            distance: 6
        }
    );
    assert!(cmd(&mut s, held, Command::ControlPlayer).is_err());
    // Turned a quarter right on the hand (`setTransform`'s " 0 0 1 1.5708"),
    // and kept so while their mouse turns the camera.
    let turn = |s: &Session| {
        let d = state(s, held).yaw - state(s, holder).yaw;
        (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    };
    assert!(
        (turn(&s) - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
        "{}",
        turn(&s)
    );
    inputs.get_mut(&held).unwrap().yaw = 2.0;
    steps(&mut s, &inputs, 24);
    assert!(
        (turn(&s) - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
        "{}",
        turn(&s)
    );
    // Their bricks are put away (`unmountImage(0)`).
    assert!(
        s.take_private_notices()
            .iter()
            .any(|(o, n)| *o == held && matches!(n, Notice::PutAway))
    );
    // Neither switches tools (`PlayerThrowing_CanUseTools`).
    assert!(cmd(&mut s, held, Command::EquipTool { slot: None }).is_err());
    assert!(cmd(&mut s, holder, Command::EquipTool { slot: None }).is_err());
    // Nor take bricks in hand (`serverCmdUseInventory`): put back.
    cmd(
        &mut s,
        held,
        Command::BrickHand(bri_sim::session::BrickHand {
            stocked: true,
            equipped: true,
            ghost: false,
        }),
    )
    .unwrap();
    assert!(
        s.take_private_notices()
            .iter()
            .any(|(o, n)| *o == held && matches!(n, Notice::PutAway))
    );

    // Struggling: not before 3 s, then free, restored.
    steps(&mut s, &inputs, 120);
    cmd(&mut s, held, Command::Activate).unwrap();
    steps(&mut s, &inputs, 1);
    assert!(s.vitals()[&held].ride.is_some(), "too soon to escape");
    steps(&mut s, &inputs, 240);
    cmd(&mut s, held, Command::Activate).unwrap();
    steps(&mut s, &inputs, 1);
    let vitals = s.vitals();
    assert_eq!(vitals[&held].ride, None, "escaped");
    assert_eq!(vitals[&held].look_limits, None);
    assert_eq!(vitals[&held].control, ControlObject::Player);
    assert!((state(&s, held).scale - 1.0).abs() < 1e-4);
    assert!(cmd(&mut s, holder, Command::EquipTool { slot: None }).is_ok());

    // Grab again once the 5 s timeout has passed since letting go.
    steps(&mut s, &inputs, 300);
    aim(&mut s, &mut inputs);
    steps(&mut s, &inputs, 2);
    cmd(&mut s, holder, Command::Activate).unwrap();
    steps(&mut s, &inputs, 1);
    assert!(s.vitals()[&held].ride.is_none(), "grab timeout");
    steps(&mut s, &inputs, 300);
    aim(&mut s, &mut inputs);
    steps(&mut s, &inputs, 2);
    cmd(&mut s, holder, Command::Activate).unwrap();
    steps(&mut s, &inputs, 1);
    assert!(s.vitals()[&held].ride.is_some(), "grabbed again");

    // Look up, hold fire past the full charge, let go: thrown that way at
    // 2.5 x 11.
    inputs.get_mut(&holder).unwrap().pitch = 0.4;
    cmd(&mut s, holder, Command::Activate).unwrap();
    steps(&mut s, &inputs, 12 * 15);
    cmd(&mut s, holder, Command::ActivateRelease).unwrap();
    steps(&mut s, &inputs, 1);
    assert_eq!(s.vitals()[&held].ride, None, "thrown");
    let speed = Vec3::from(state(&s, held).velocity).length();
    assert!((speed - 27.5).abs() < 1.0, "thrown at {speed}");
    assert!(
        state(&s, held).velocity[1] > 5.0,
        "upward, where the holder looks"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
