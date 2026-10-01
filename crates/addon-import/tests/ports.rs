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

/// The stand-in Duplicator imported with its port, hosted on a flat floor
/// with 2x1 plates, and the host joined.
fn duplorcator_game(name: &str) -> (PathBuf, bri_sim::session::Session, u64) {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    use bri_sim::session::Session;
    use rapier3d::prelude::*;

    let dir = fresh(name);
    let root = dir.join("content");
    let out = root.join("addons/tool_duplicator");
    let report = import(&options(fixture("ports/Tool_Duplicator"), out.clone())).unwrap();
    let applied = &report.ports[0];
    assert!(applied.applied, "{:?}", applied.reason);
    // Read from the stand-in's own script, not the original's.
    assert_eq!(applied.values["reach"], "8");
    assert_eq!(applied.values["highlight_ms"], "2000");
    let rules = applied.rules.as_ref().expect("the port has host rules");
    assert_eq!(rules.id, "tool_duplicator-rules");
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    assert_eq!(
        pack.images["tool_duplicator:image/duplorcatorimage"]
            .command
            .as_deref(),
        Some("tool_duplicator-rules:fire")
    );

    // A flat floor and a 2x1 plate.
    let mesh = bri_content::brick::Brick {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = bri_content::collision::CollisionBody {
        id: "plate".into(),
        parts: vec![bri_content::collision::Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let definitions = bri_sim::definitions::Definitions {
        entries: [(
            "plate".into(),
            bri_sim::definitions::Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
            },
        )]
        .into(),
    };
    // White, blue, and the cyan the highlight looks for.
    let palette = vec![[1.0; 4], [0.2, 0.4, 1.0, 1.0], [0.0, 0.9, 0.9, 1.0]];
    let mut s = Session::new(
        bri_sim::simulation::Simulation::new(
            bri_world::World::new("Dup".into(), "dup".into(), palette),
            definitions,
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s.set_weapon_pack(pack).unwrap();
    let entry = |id: &str, side| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        dir: format!("addons/{id}"),
        role: None,
    };
    let set = PackageSet {
        schema_version: 1,
        packages: vec![
            entry("tool_duplicator", Side::Shared),
            entry("tool_duplicator-rules", Side::Server),
        ],
    };
    let catalog = bri_package_runtime::Catalog::load(&root, &set, true)
        .unwrap_or_else(|e| panic!("{e:#?}"));
    s.install_packages(std::sync::Arc::new(catalog), None).unwrap();

    let host = s.join("Host".into(), Vec3::new(0.0, 0.05, 2.0), true).unwrap();
    (dir, s, host)
}

/// The Duplorcator's port, on the stand-in Duplicator in a hosted game:
/// `/dup` gives the wand, a swing at the bottom of a build selects the
/// stack up from it with full trust, lights it, and shows the copy; the
/// plant key plants each brick of the copy that fits, and one Ctrl+Z takes
/// them all back.
#[test]
fn duplorcator_port_copies_lights_and_plants_brick_by_brick() {
    use bri_sim::session::{Command, Notice, PackageCommand, Reply, Session};

    let (dir, mut s, host) = duplorcator_game("duplorcator");
    let mut seq = 0u64;
    let mut cmd = |s: &mut Session, command: Command| {
        seq += 1;
        s.command(host, seq, command)
    };
    let plant = |s: &mut Session, position: [f32; 3], cmd: &mut dyn FnMut(&mut Session, Command) -> anyhow::Result<Reply>| {
        match cmd(
            s,
            Command::Plant {
                definition: "plate".into(),
                position,
                quarter_turns: 0,
                color: 1,
            },
        ) {
            Ok(Reply::Planted(id)) => id,
            other => panic!("plant at {position:?}: {other:?}"),
        }
    };
    // A plate with one half-on top, and a plate beside them, not joined.
    let base = plant(&mut s, [0.5, 0.1, 0.25], &mut cmd);
    plant(&mut s, [1.0, 0.3, 0.25], &mut cmd);
    plant(&mut s, [2.5, 0.1, 0.25], &mut cmd);
    let before = s.snapshot().world.bricks.len();

    // /dup puts the wand in hand.
    cmd(
        &mut s,
        Command::Package(PackageCommand {
            package: String::new(),
            command: "dup".into(),
            args: vec![],
        }),
    )
    .unwrap();
    s.step().unwrap();
    assert!(
        s.tool_inventories()[&host]
            .slots
            .iter()
            .any(|t| t.as_deref() == Some("tool_duplicator:weapon/duplorcatoritem"))
    );
    // Look down at the base plate's uncovered half and swing.
    for tick in 0..60u64 {
        s.movement(
            host,
            tick + 1,
            bri_sim::player::MoveInput {
                yaw: 0.142,
                pitch: -0.85,
                ..Default::default()
            },
        )
        .unwrap();
        if tick == 30 || tick == 31 {
            cmd(&mut s, Command::WeaponTrigger { down: tick == 30 }).unwrap();
        }
        s.step().unwrap();
    }
    let private = s.take_private_notices();
    let copy = s
        .blueprint(host)
        .unwrap_or_else(|| panic!("the swing copied the tower: {private:?}"))
        .clone();
    assert_eq!(copy.bricks.len(), 2);
    assert_eq!(copy.tool, "tool_duplicator:weapon/duplorcatoritem");
    // The copy keeps the bricks' own colour; the bricks glow cyan for now.
    assert!(copy.bricks.iter().all(|b| b.color == 1));
    let world = s.snapshot().world;
    assert_eq!((world.bricks[&base].color, world.bricks[&base].color_effect), (2, 3));
    let prints: Vec<String> = private
        .into_iter()
        .filter_map(|(_, n)| match n {
            Notice::Bottom { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert!(
        prints.iter().any(|t| t.contains("Duplication") && t.contains("2 bricks selected")),
        "{prints:?}"
    );
    // Two seconds later they have their own colour back.
    for _ in 0..250 {
        s.step().unwrap();
    }
    let world = s.snapshot().world;
    assert_eq!((world.bricks[&base].color, world.bricks[&base].color_effect), (1, 0));

    // Planted over the lone plate: the bottom plate would overlap it and is
    // skipped, the top one sits on it and plants, and the player hears how
    // many.
    let reply = cmd(
        &mut s,
        Command::PlaceBlueprint {
            position: [2.5, 0.0, 0.0],
            quarter_turns: 0,
            mirrored: false,
        },
    );
    assert!(matches!(reply, Ok(Reply::Planted(_))), "{reply:?}");
    assert_eq!(s.snapshot().world.bricks.len(), before + 1);
    s.step().unwrap();
    let prints: Vec<String> = s
        .take_private_notices()
        .into_iter()
        .filter_map(|(_, n)| match n {
            Notice::Center { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert!(
        prints.iter().any(|t| t.contains("1") && t.contains("/") && t.contains("duplicated successfully")),
        "{prints:?}"
    );
    // One undo takes the planted copy back.
    cmd(&mut s, Command::Tool(bri_sim::session::ToolAction::UndoBrick)).unwrap();
    assert_eq!(s.snapshot().world.bricks.len(), before);
    std::fs::remove_dir_all(dir).unwrap();
}

/// The Duplorcator's `/saveDup` and `/loadDup` on the port: a selection
/// saved by name loads back, a name nobody saved says so, and a v20
/// duplication file (bricks in a frame of their own, in its own colours)
/// loads onto the grid in this world's nearest colours, wand in hand.
#[test]
fn duplorcator_port_saves_and_loads_duplications() {
    use bri_sim::session::{
        Command, MemoryCopies, Notice, PackageArg, PackageCommand, Reply, Session,
    };
    use std::sync::Arc;

    let (dir, mut s, host) = duplorcator_game("duplorcator-saves");
    let store = Arc::new(MemoryCopies::default());
    s.set_copy_store(store.clone());
    let seq = std::cell::Cell::new(0u64);
    let cmd = |s: &mut Session, command: Command| {
        seq.set(seq.get() + 1);
        s.command(host, seq.get(), command)
    };
    let typed = |s: &mut Session, command: &str, name: Option<&str>| {
        let reply = cmd(
            s,
            Command::Package(PackageCommand {
                package: String::new(),
                command: command.into(),
                args: name
                    .map(|n| vec![PackageArg::String(n.into())])
                    .unwrap_or_default(),
            }),
        );
        assert!(reply.is_ok(), "/{command}: {reply:?}");
        // The store answers, and the Add-On hears it, over the next ticks.
        for _ in 0..3 {
            s.step().unwrap();
        }
        s.take_private_notices()
            .into_iter()
            .filter_map(|(_, n)| match n {
                Notice::Center { text, .. } | Notice::Bottom { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<String>>()
    };
    for position in [[0.5, 0.1, 0.25], [1.0, 0.3, 0.25]] {
        let reply = cmd(
            &mut s,
            Command::Plant {
                definition: "plate".into(),
                position,
                quarter_turns: 0,
                color: 1,
            },
        );
        assert!(matches!(reply, Ok(Reply::Planted(_))), "{reply:?}");
    }
    typed(&mut s, "dup", None);
    for tick in 0..40u64 {
        s.movement(
            host,
            tick + 1,
            bri_sim::player::MoveInput {
                yaw: 0.142,
                pitch: -0.85,
                ..Default::default()
            },
        )
        .unwrap();
        if tick == 30 || tick == 31 {
            cmd(&mut s, Command::WeaponTrigger { down: tick == 30 }).unwrap();
        }
        s.step().unwrap();
    }
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(2));

    let prints = typed(&mut s, "savedup", Some("My Tower"));
    assert!(
        prints.iter().any(|t| t.contains("successfully saved as") && t.contains("My Tower")),
        "{prints:?}"
    );
    let saved = store.saved("my tower").expect("the copy was kept");
    assert_eq!(saved.copy.bricks.len(), 2);
    assert_eq!(saved.saved_by, "Host");

    // A name nobody saved; then, a second on, the saved one.
    let prints = typed(&mut s, "loaddup", Some("Nothing"));
    assert!(
        prints.iter().any(|t| t.contains("Nothing") && t.contains("does not exist")),
        "{prints:?}"
    );
    for _ in 0..130 {
        s.step().unwrap();
    }
    s.take_private_notices();
    let prints = typed(&mut s, "loaddup", Some("my tower"));
    assert!(
        prints.iter().any(|t| t.contains("Loaded duplication") && t.contains("2<color:99AAAA>/\\c42")),
        "{prints:?}"
    );
    assert_eq!(s.blueprint(host).unwrap().bricks, saved.copy.bricks);

    // A v20 file: its first brick at its own origin, off this grid, in a
    // palette whose entry 5 is the cyan this world has as 2.
    let mut palette = vec![[0.5, 0.5, 0.5, 1.0]; 64];
    palette[5] = [0.0, 1.0, 1.0, 1.0];
    let loose = |position: [f32; 3]| {
        let mut b = bri_world::Brick::new(
            bri_world::ContentRef::Resolved("plate".into()),
            position,
            0,
        );
        b.color = 5;
        b
    };
    store.put_loose(
        "Old Bridge",
        vec![loose([0.0, 0.0, 0.0]), loose([0.5, 0.2, 0.0])],
        palette,
    );
    for _ in 0..130 {
        s.step().unwrap();
    }
    s.take_private_notices();
    let prints = typed(&mut s, "loaddup", Some("old bridge.bls"));
    assert!(
        prints.iter().any(|t| t.contains("2 bricks selected")),
        "{prints:?}"
    );
    let copy = s.blueprint(host).unwrap().clone();
    assert!(copy.bricks.iter().all(|b| b.color == 2));
    assert_eq!(copy.size, [3, 2, 1]);
    assert!(
        s.tool_inventories()[&host]
            .slots
            .iter()
            .any(|t| t.as_deref() == Some("tool_duplicator:weapon/duplorcatoritem"))
    );
    std::fs::remove_dir_all(dir).unwrap();
}
