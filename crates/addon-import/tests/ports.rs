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
        ..Default::default()
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
    assert_eq!(
        on,
        ["weapon_synthetic_blaster", "weapon_synthetic_blaster-rules"]
    );
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
    bri_package_runtime::Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));

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
    let s = porting::scaffold(&fixture("ports/Weapon_Shotgun"), &work, None, vec![], None).unwrap();
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
    let s = porting::scaffold(&fixture("Weapon_Synthetic_Blaster"), &work, None, vec![], None).unwrap();
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

/// Slayer and its Capture the Flag mode: both ports apply to stand-ins with
/// the originals' folder names and function shapes (`tests/fixtures/ports`,
/// CC0), CTF's run-time flag datablocks import from the port's
/// `datablocks.cs` with this copy's numbers, and both host-rules companions
/// load as the game loads them.
#[test]
fn slayer_ports_apply_with_their_rules() {
    let dir = fresh("slayer");
    let root = dir.join("content");
    let ports = Ports::builtin();
    let mut ids = vec![];
    for (addon, ns) in [
        ("Gamemode_Slayer", "gamemode_slayer"),
        ("Gamemode_Slayer_CTF", "gamemode_slayer_ctf"),
    ] {
        let out = root.join("addons").join(ns);
        let report = import_with(&options(fixture(&format!("ports/{addon}")), out), &ports).unwrap();
        let applied = &report.ports[0];
        assert!(applied.applied, "{addon}: {:?}", applied.reason);
        assert_eq!(applied.status, "partial");
        if ns == "gamemode_slayer" {
            // The capture point trigger's callbacks are the rules' zone.
            let trigger = report.datablocks.iter().find(|d| d.name == "Slayer_CPTriggerData").unwrap();
            assert_eq!(trigger.status, "consumed", "{trigger:?}");
            // So are the path cameras': the fly-through and the spectators'
            // auto camera are `follow_path` in the rules.
            for name in ["Slayer_PathCamData", "Slayer_SpectatePathCamData"] {
                let camera = report.datablocks.iter().find(|d| d.name == name).unwrap();
                assert_eq!(camera.status, "consumed", "{camera:?}");
            }
        }
        ids.push(ns.to_owned());
        ids.push(applied.rules.as_ref().expect("rules").id.clone());
    }
    let slayer = std::fs::read_to_string(root.join("addons/gamemode_slayer-rules/slayer.rhai")).unwrap();
    // Its countdown voices and buzzer, made at run time in the original,
    // are declared by the port and play by id; a voice whose file this copy
    // lacks stays out.
    let sounds = Pack::from_json(&std::fs::read(root.join("addons/gamemode_slayer/assets/weapons.json")).unwrap())
        .unwrap()
        .sounds;
    for id in ["slayer_begin_sound", "slayer_1_seconds_sound"] {
        assert!(sounds.contains_key(&format!("gamemode_slayer:sound/{id}")), "{id}: {:?}", sounds.keys());
    }
    assert!(!sounds.contains_key("gamemode_slayer:sound/slayer_2_seconds_sound"));
    assert!(slayer.contains("`gamemode_slayer:sound/slayer_${left}_seconds_sound`"));
    assert!(slayer.contains("\"gamemode_slayer:brick/brickslyrspawnpointdata\""));
    // Capture points: the bricks convert, their bars are this copy's
    // lengths, and their trigger is a zone ticking at its preference.
    let content = std::fs::read_to_string(root.join("addons/gamemode_slayer/assets/content.json")).unwrap();
    for brick in ["brickslyrcpdata", "brickslyrlrgcpdata"] {
        assert!(content.contains(&format!("gamemode_slayer:brick/{brick}")), "no {brick}");
    }
    assert!(slayer.contains("{\n        3\n    }") && slayer.contains("{\n        5\n    }"));
    let rules: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("addons/gamemode_slayer-rules/behaviour.json")).unwrap(),
    )
    .unwrap();
    let zone = &rules["zones"][0];
    assert_eq!((zone["period_ms"].as_u64(), zone["ticks"].as_bool()), (Some(100), Some(true)));
    // Settings at this copy's defaults, its game modes in the mode list.
    let read = |path: &str| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(root.join(path)).unwrap()).unwrap()
    };
    let setting = |b: &serde_json::Value, key: &str| -> serde_json::Value {
        b["settings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["key"] == key)
            .unwrap_or_else(|| panic!("no setting {key}"))
            .clone()
    };
    let behaviour = read("addons/gamemode_slayer-rules/behaviour.json");
    assert_eq!(setting(&behaviour, "time_between_rounds")["default"], 6);
    // The fly-through and spectating, at this copy's numbers.
    assert_eq!(setting(&behaviour, "fly_play_on_reset")["default"], true);
    assert_eq!(setting(&behaviour, "fly_during_countdown")["default"], false);
    assert_eq!(setting(&behaviour, "spectate_auto_cam")["default"], true);
    assert_eq!(setting(&behaviour, "team_only_dead_cam")["default"], false);
    for line in [
        "fn fly_max_knots() { 6 }",
        "fn fly_default_speed() { 9 }",
        "ms_ticks(3000)",
        "ms_ticks(500)",
        "orbit_point(p, [b.x, b.y + 1.5, b.z], 6)",
        "set_fov(p, 100.to_float())",
        "ms_ticks(2000)",
    ] {
        assert!(slayer.contains(line), "slayer.rhai lacks `{line}`");
    }
    // The wrench outputs, with this copy's parameters and words.
    let output = |name: &str| {
        behaviour["brick_outputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == name)
            .unwrap_or_else(|| panic!("no {name} output"))
            .clone()
    };
    assert_eq!(behaviour["brick_outputs"].as_array().unwrap().len(), 20);
    assert_eq!(
        output("BottomPrintAll")["params"],
        serde_json::json!([
            { "type": "string", "max_length": 150, "width": 120 },
            { "type": "int", "min": 1, "max": 8, "default": 2 },
            { "type": "bool" }
        ])
    );
    assert_eq!(output("IncScore")["class"], "Slayer_TeamSO");
    assert_eq!(
        behaviour["brick_targets"],
        serde_json::json!([
            { "name": "Team(Client)", "class": "Slayer_TeamSO", "from": "Client" },
            { "name": "Team(Brick)", "class": "Slayer_TeamSO", "from": "Self" }
        ])
    );
    assert_eq!(
        output("setTeamControlLocked")["params"],
        serde_json::json!([
            { "type": "list", "items": [["Mine", 0], ["Colour", 1], ["Every", 2]] },
            { "type": "paint_color", "default": 1 },
            { "type": "bool" }
        ])
    );
    assert_eq!(
        output("addLives")["params"],
        serde_json::json!([{ "type": "int", "min": 0, "max": 50, "default": 2 }])
    );
    assert_eq!(output("StartFlyThrough")["params"], serde_json::json!([]));
    // The team inputs follow the engine's, the mini-game inputs run on every
    // brick of the game, and Restrict Output Events at this copy's levels.
    let inputs = behaviour["brick_inputs"].as_array().unwrap();
    assert_eq!(inputs.len(), 10 + 12 + 5);
    let input = |name: &str| inputs.iter().find(|i| i["name"] == name).unwrap_or_else(|| panic!("no {name}"));
    assert_eq!(input("onActivate(Team6)")["follows"], "onActivate");
    assert_eq!(input("onPlayerTouch(Team1)")["follows"], "onPlayerTouch");
    assert_eq!(
        input("onMinigameDeath")["targets"],
        serde_json::json!(["Client", "Player(Killer)", "Client(Killer)", "MiniGame"])
    );
    assert_eq!(behaviour["on_event_row"], true);
    assert_eq!(setting(&behaviour, "restrict_output_events")["default"], false);
    for line in [
        "\"minigame:bottomprintall\": 2,",
        "\"minigame:win\": -1,",
        "\"minigame:startflythrough\": 3,",
    ] {
        assert!(slayer.contains(line), "slayer.rhai lacks `{line}`");
    }
    assert_eq!(setting(&behaviour, "clear_stats")["default"], false);
    for line in ["Locked for now.", "\"Extended by\"", "\"Time now\""] {
        assert!(slayer.contains(line), "slayer.rhai lacks `{line}`");
    }
    assert_eq!(setting(&behaviour, "auto_sort")["default"], true);
    assert_eq!(setting(&behaviour, "team_lives")["default"], -1);
    let mode = setting(&behaviour, "mode");
    assert_eq!(mode["default"], "Slayer_Deathmatch");
    assert_eq!(
        mode["items"],
        serde_json::json!([
            { "value": "Slayer_Deathmatch", "name": "Free for All" },
            { "value": "Slayer_TeamDeathmatch", "name": "Teams" }
        ])
    );

    // This copy's numbers, read from its scripts.
    let ctf = root.join("addons/gamemode_slayer_ctf");
    let rules = std::fs::read_to_string(root.join("addons/gamemode_slayer_ctf-rules/ctf.rhai")).unwrap();
    for line in [
        "fn flag_slot() { 2 }",
        "fn pickup_guard_ticks() { (250 * 120 + 999) / 1000 }",
        "let ahead = 1.5;",
        "let fling = 4;",
        "setting(game, \"gamemode_slayer-rules:mode\") == ctf_mode()",
    ] {
        assert!(rules.contains(line), "ctf.rhai lacks `{line}`");
    }
    let behaviour = read("addons/gamemode_slayer_ctf-rules/behaviour.json");
    assert_eq!(behaviour["zones"][0]["period_ms"], 100);
    assert_eq!(setting(&behaviour, "flag_returns_to_win")["default"], 3);
    assert_eq!(setting(&behaviour, "points_flag")["default"], 25);
    assert_eq!(setting(&behaviour, "dropped_flag_respawn")["default"], 7);
    assert_eq!(setting(&behaviour, "manual_flag_drop")["default"], true);
    // Capture the Flag joins Slayer's modes, so its rules need Slayer's.
    assert_eq!(
        behaviour["setting_items"],
        serde_json::json!([{ "setting": "gamemode_slayer-rules:mode",
                             "items": [{ "value": "Slayer_CTF", "name": "Flag Game" }] }])
    );
    let manifest = read("addons/gamemode_slayer_ctf-rules/package.json");
    assert_eq!(manifest["dependencies"]["gamemode_slayer-rules"], "*");

    // The flag item and image the original makes at run time, one of each,
    // taking the colour of their brick or carrier.
    let pack = Pack::from_json(&std::fs::read(ctf.join("assets/weapons.json")).unwrap()).unwrap();
    let image = &pack.images["gamemode_slayer_ctf:image/slyrctf_flagimage"];
    assert!(image.paint_tint);
    assert_eq!(image.mount_point, 4);
    // "0.1 -0.2 -0.3" in Torque's Z-up axes.
    assert_eq!(image.offset, [0.1, -0.3, 0.2]);
    let item = &pack.items["gamemode_slayer_ctf:weapon/slyrctf_flagitem"];
    assert_eq!(item.ui_name, "Stand-in Flag");
    assert_eq!(item.image, "gamemode_slayer_ctf:image/slyrctf_flagimage");
    assert!(!item.can_drop);
    // `slyrCTF_FlagItem::onAdd` plays the flag model's idle thread.
    assert_eq!(item.idle, "wave");
    let content = std::fs::read_to_string(ctf.join("assets/content.json")).unwrap();
    for brick in ["brickslyrctfflagdata", "brickslyrctfflagreturndata"] {
        assert!(
            content.contains(&format!("gamemode_slayer_ctf:brick/{brick}")),
            "no {brick}"
        );
    }

    // Both companions load as the game loads any host Add-On.
    use bri_package::library::Library;
    let library = Library::scan(&root).unwrap();
    let set = bri_package::packages::PackageSet {
        schema_version: 1,
        packages: ids
            .iter()
            .map(|id| {
                let e = library.get(id).unwrap_or_else(|| panic!("{id} not found"));
                assert!(!e.has_errors(), "{id}: {:?}", e.problems);
                e.package.clone()
            })
            .collect(),
    };
    bri_package_runtime::Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    std::fs::remove_dir_all(dir).unwrap();
}

/// A 2x1 plate, for a hosted game with bricks.
fn plate() -> bri_sim::definitions::Definitions {
    use bri_content::{
        brick::{Brick as Mesh, Face, Quad, Surface, Vertex},
        collision::{CollisionBody, Part},
    };
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![Quad {
            face: Face::Top,
            surface: Surface::Ramp,
            vertices: [
                [-0.5, 0.1, -0.25],
                [0.5, 0.1, -0.25],
                [0.5, 0.1, 0.25],
                [-0.5, 0.1, 0.25],
            ]
            .map(|position| Vertex {
                position,
                normal: [0.0, 1.0, 0.0],
                uv: [0.0; 2],
            }),
            colors: None,
        }],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    bri_sim::definitions::Definitions {
        entries: [(
            "plate".to_string(),
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
    }
}

/// Hosted: the stand-in Fill Can, imported with the listed port, fires its
/// projectile at a row of plates and its host rules fill them as v20 did:
/// the colour picked, an FX can picked while it stays in hand, an
/// administrator's limit (3 in this copy) with its message, and one undo.
#[test]
fn fill_can_port_rules_fill_what_v20_filled() {
    use bri_package::{library::Library, packages::PackageSet};
    use bri_sim::session::{Command, Notice, PackageCommand, Reply, Session, ToolAction};
    use bri_world::{BrickId, OwnerId};
    use rapier3d::prelude::*;
    const NS: &str = "tool_fill_can";
    const IMAGE: &str = "tool_fill_can:image/fillcanimage";
    let dir = fresh("fill-can");
    let root = dir.join("content");
    let out = root.join(format!("addons/{NS}"));
    let report = import(&options(fixture("ports/Tool_Fill_Can"), out.clone())).unwrap();
    let applied = &report.ports[0];
    assert!(applied.applied, "{:?}", applied.reason);
    assert_eq!(applied.values["admin_limit"], "3");
    let rules = std::fs::read_to_string(root.join(format!("addons/{NS}-rules/fill.rhai"))).unwrap();
    assert!(rules.contains("[0.6 / 2.0, 0.3 / 2.0]"), "{rules}");
    let library = Library::scan(&root).unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: [NS.to_string(), format!("{NS}-rules")]
            .iter()
            .map(|id| library.get(id).unwrap().package.clone())
            .collect(),
    };
    let catalog = std::sync::Arc::new(
        bri_package_runtime::Catalog::load(&root, &set, true).unwrap_or_else(|e| panic!("{e:#?}")),
    );
    let mut pack =
        Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    assert!(pack.images[IMAGE].paint_picker);
    // Stand-ins for the stock spray cans (base game content).
    for can in
        std::iter::once(bri_sim::session::SPRAY_CAN_IMAGE).chain(bri_sim::session::FX_CAN_IMAGES)
    {
        let mut image = pack.images[IMAGE].clone();
        image.id = can.into();
        image.projectile = None;
        image.paint_picker = false;
        pack.images.insert(image.id.clone(), image);
    }
    const RED: u8 = 2;
    const BLUE: u8 = 1;
    // A spawn brick, of a build no one here owns, whose committed stunt
    // plane takes its colour.
    let mut world = bri_world::World::new(
        "Fill".into(),
        "fill".into(),
        vec![[1.0; 4], [0.2, 0.4, 1.0, 1.0], [0.9, 0.1, 0.1, 1.0]],
    );
    let mut pad = bri_world::Brick::new(
        bri_world::ContentRef::Resolved("plate".into()),
        [-1.0, 0.1, 10.25],
        4242,
    );
    pad.color = RED;
    pad.vehicle = Some(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved(
            "vehicle_stunt_plane:vehicle/stuntplanevehicle".into(),
        ),
        recolor: true,
    });
    world.bricks.insert(1, pad);
    world.next_brick_id = 2;
    let mut s = Session::new(
        bri_sim::simulation::Simulation::new(
            world,
            plate(),
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    let spawn = Vec3::new(0.5, 0.05, 4.0);
    s.set_spawn_points(vec![spawn]).unwrap();
    s.set_weapon_pack(pack).unwrap();
    let plane = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/imported/vehicle_stunt_plane/assets/vehicles.json");
    s.set_vehicle_pack(bri_vehicles::Pack::load(plane).unwrap(), Vec::new())
        .unwrap();
    s.install_packages(catalog, None).unwrap();
    // One sequence for every message, so each player's only rises.
    let seq = std::cell::Cell::new(0u64);
    let next = || {
        seq.set(seq.get() + 1);
        seq.get()
    };
    let cmd = |s: &mut Session, owner: OwnerId, command: Command| s.command(owner, next(), command);
    let steps = |s: &mut Session, n: usize| {
        for _ in 0..n {
            s.step().unwrap();
        }
    };
    // Five red plates in a row on the ground, one end facing whoever
    // planted them, at `x`.
    let plant_row = |s: &mut Session, owner: OwnerId, x: f32| -> Vec<BrickId> {
        (0..5)
            .map(|i| {
                steps(s, 121);
                match cmd(
                    s,
                    owner,
                    Command::Plant {
                        definition: "plate".into(),
                        position: [x, 0.1, 0.25 - 0.5 * i as f32],
                        quarter_turns: 0,
                        color: RED,
                    },
                ) {
                    Ok(Reply::Planted(id)) => id,
                    other => panic!("plant {i}: {other:?}"),
                }
            })
            .collect()
    };
    let colors = |s: &Session, row: &[BrickId]| -> Vec<(u8, u8)> {
        let world = s.snapshot().world;
        row.iter()
            .map(|id| (world.bricks[id].color, world.bricks[id].color_effect))
            .collect()
    };
    // Spray once at `target`.
    let spray_at = |s: &mut Session, owner: OwnerId, target: Vec3| {
        let feet = s
            .snapshot()
            .players
            .iter()
            .find(|p| p.owner == owner)
            .unwrap()
            .feet;
        let d = (target - (Vec3::from(feet) + Vec3::Y * 2.156)).normalize();
        let input = bri_sim::player::MoveInput {
            yaw: d.x.atan2(-d.z),
            pitch: d.y.asin(),
            ..Default::default()
        };
        for i in 0..90u64 {
            s.movement(owner, next(), input).unwrap();
            if i == 30 || i == 31 {
                s.command(owner, next(), Command::WeaponTrigger { down: i == 30 })
                    .unwrap();
            }
            s.step().unwrap();
        }
    };
    // Spray once at the nearest plate of the row at `x`.
    let spray =
        |s: &mut Session, owner: OwnerId, x: f32| spray_at(s, owner, Vec3::new(x, 0.2, 0.25));
    let held = |s: &Session, owner: OwnerId| {
        s.weapon_view().images[&owner]
            .iter()
            .any(|i| i.image == IMAGE && i.hand == 0)
    };
    let fillcan = Command::Package(PackageCommand {
        package: String::new(),
        command: "fillcan".into(),
        args: vec![],
    });

    // A player picks blue, takes the Fill Can out and sprays: every red
    // plate joined to the one hit turns blue. One Ctrl+Z takes it back.
    let painter = s.join("Painter".into(), spawn, false).unwrap();
    let row = plant_row(&mut s, painter, 0.5);
    cmd(&mut s, painter, Command::UseSprayCan { color: BLUE }).unwrap();
    cmd(&mut s, painter, fillcan.clone()).unwrap();
    steps(&mut s, 30);
    assert!(held(&s, painter));
    spray(&mut s, painter, 0.5);
    assert_eq!(colors(&s, &row), vec![(BLUE, 0); 5]);
    assert!(matches!(
        cmd(&mut s, painter, Command::Tool(ToolAction::UndoBrick)),
        Ok(Reply::Undone(Some(_)))
    ));
    assert_eq!(colors(&s, &row), vec![(RED, 0); 5]);

    // Picking an FX can with the Fill Can out keeps it in hand, and the
    // next spray gives the red plates that effect.
    cmd(&mut s, painter, Command::UseFxCan { fx: 3 }).unwrap();
    steps(&mut s, 30);
    assert!(held(&s, painter), "the Fill Can stays out");
    spray(&mut s, painter, 0.5);
    assert_eq!(colors(&s, &row), vec![(RED, 3); 5]);

    // An administrator's fill stops at this copy's limit, and says so.
    let admin = s
        .join("Admin".into(), Vec3::new(-2.5, 0.05, 4.0), true)
        .unwrap();
    let theirs = plant_row(&mut s, admin, -2.5);
    cmd(&mut s, admin, Command::UseSprayCan { color: BLUE }).unwrap();
    cmd(&mut s, admin, fillcan).unwrap();
    steps(&mut s, 30);
    s.take_private_notices();
    spray(&mut s, admin, -2.5);
    let painted = colors(&s, &theirs)
        .iter()
        .filter(|(c, _)| *c == BLUE)
        .count();
    assert_eq!(painted, 3);
    let told: Vec<Notice> = s
        .take_private_notices()
        .into_iter()
        .filter(|(o, _)| *o == admin)
        .map(|(_, n)| n)
        .filter(|n| matches!(n, Notice::Center { .. } | Notice::PlantError(_)))
        .collect();
    assert_eq!(
        told,
        [
            Notice::PlantError(bri_sim::simulation::PlantFailure::Limit),
            Notice::Center {
                text: "\u{E003}Reached Fill Can Brick Limit (500)".to_string(),
                seconds: 4.0
            }
        ]
    );

    // A spray at the plane paints it, through the brick that recolours
    // it; the painter, not trusted by its build, is refused for this
    // copy's time.
    let plane = || {
        let v = s.vehicle_poses();
        assert_eq!(v.len(), 1, "the plane is on its spawn");
        Vec3::from(v[0].position)
    };
    let at = plane() + Vec3::Y * 0.8;
    let paint = |s: &Session| s.vehicle_infos()[0].color;
    let red = paint(&s);
    cmd(&mut s, painter, Command::UseSprayCan { color: BLUE }).unwrap();
    s.take_private_notices();
    spray_at(&mut s, painter, at);
    assert_eq!(paint(&s), red);
    let refused: Vec<Notice> = s
        .take_private_notices()
        .into_iter()
        .filter(|(o, n)| *o == painter && matches!(n, Notice::Center { .. }))
        .map(|(_, n)| n)
        .collect();
    assert_eq!(
        refused,
        [Notice::Center {
            text: "BL_ID: 4242 does not trust you enough to do that.".into(),
            seconds: 2.0
        }]
    );
    spray_at(&mut s, admin, at);
    let [r, g, b, _] = s.simulation().state().palette[usize::from(BLUE)];
    assert_eq!(paint(&s), Some([r, g, b, 1.0]));
    assert_eq!(s.snapshot().world.bricks[&1].color, BLUE);
    // With an FX can it takes a colour of its own; its brick stays blue.
    cmd(&mut s, admin, Command::UseFxCan { fx: 1 }).unwrap();
    steps(&mut s, 30);
    spray_at(&mut s, admin, at);
    assert_ne!(paint(&s), Some([r, g, b, 1.0]));
    assert_eq!(s.snapshot().world.bricks[&1].color, BLUE);
   std::fs::remove_dir_all(dir).unwrap();
}

/// Kaje's Sniper Rifle: `onFire` kicks the arm with `shiftAway` (read from
/// the copy's script) and fires one round as any weapon does. The import
/// ships nothing of the original; the stand-in carries its shape.
#[test]
fn sniper_rifle_port_kicks_the_arm() {
    let dir = fresh("sniper");
    let out = dir.join("package");
    let report = import(&options(fixture("ports/Weapon_Sniper_Rifle"), out.clone())).unwrap();
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(
        (port.port.as_str(), port.status.as_str()),
        ("weapon_sniper_rifle", "verified")
    );
    assert!(port.values["fire_arm"].eq_ignore_ascii_case("shiftaway"));
    assert_eq!(report.summary.needs_behaviour_ported, 1);
    bri_addon_import::ports::check_pins(&out).unwrap();
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    let image = &pack.images["weapon_sniper_rifle:image/sniperrifleimage"];
    let on_fire = &image.scripts["onfire"];
    assert!(on_fire.arm.eq_ignore_ascii_case("shiftaway") && on_fire.fire);
    assert!(image.hide_nodes.is_empty() && !image.both_arms);

    // One round straight down the aim, and the arm kicks with it.
    let mut world = WeaponsWorld::new(pack).unwrap();
    world.add_actor(ActorId(1), 5).unwrap();
    let slot = world
        .give(ActorId(1), "weapon_sniper_rifle:weapon/sniperrifleitem")
        .unwrap();
    world.equip(ActorId(1), Some(slot)).unwrap();
    let (mut rounds, mut kicks) = (vec![], vec![]);
    for tick in 0..240 {
        if tick == 60 || tick == 61 {
            world.trigger(ActorId(1), tick == 60).unwrap();
        }
        for e in world.step(&mut Empty) {
            match e {
                Event::Spawned { velocity, .. } => rounds.push(velocity),
                Event::Animation { sequence, .. } => kicks.push(sequence),
                _ => {}
            }
        }
    }
    assert_eq!(rounds.len(), 1);
    assert!(rounds[0].angle_between(Vec3::NEG_Z) < 1e-4);
    assert!(
        kicks.iter().any(|k| k.eq_ignore_ascii_case("shiftaway")),
        "{kicks:?}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Conan's Sniper Rifle Updated draws its own hands: held, it hides the
/// Blockhead's hands and hooks and raises both arms, and its shot plays
/// `plant`. All three of its image callbacks are covered.
#[test]
fn sniper_rifle_updated_port_draws_its_own_hands() {
    let dir = fresh("sniper-updated");
    let out = dir.join("package");
    let report = import(&options(
        fixture("ports/Weapon_Sniper_Rifle_Updated"),
        out.clone(),
    ))
    .unwrap();
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(port.port, "weapon_sniper_rifle_updated");
    assert_eq!(report.summary.needs_behaviour, 3);
    assert_eq!(report.summary.needs_behaviour_ported, 3);
    bri_addon_import::ports::check_pins(&out).unwrap();
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    let image = &pack.images["weapon_sniper_rifle_updated:image/sniperrifleanimatedimage"];
    let on_fire = &image.scripts["onfire"];
    assert!(on_fire.arm.eq_ignore_ascii_case("plant") && on_fire.fire);
    assert_eq!(image.hide_nodes, ["lhand", "rhand", "lhook", "rhook"]);
    assert!(image.both_arms);

    // A copy that hides other nodes is a different script: no port.
    let copy = dir.join("Weapon_Sniper_Rifle_Updated");
    std::fs::create_dir_all(&copy).unwrap();
    for f in [
        "server.cs",
        "Weapon_Sniper Rifle.cs",
        "description.txt",
        "LICENSE.txt",
    ] {
        let text = std::fs::read_to_string(fixture("ports/Weapon_Sniper_Rifle_Updated").join(f))
            .unwrap()
            .replace("hideNode(\"rhook\")", "hideNode(\"rarm\")");
        std::fs::write(copy.join(f), text).unwrap();
    }
    let other = import(&options(copy, dir.join("other"))).unwrap();
    assert!(!other.ports[0].applied);
    assert!(
        other.ports[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("`rhook`")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Trench Digging's port on a stand-in with its folder name: the four
/// images run the host rules' commands, the shovel and dirt swing the arm
/// from data, the dirt shot is the rules' to fire, and the rules read the
/// dig reach from this copy.
#[test]
fn trench_digging_port_writes_its_rules() {
    let dir = fresh("trench");
    let root = dir.join("content");
    let out = root.join("addons/gamemode_trenchdigging");
    let report = import(&options(
        fixture("ports/Gamemode_TrenchDigging"),
        out.clone(),
    ))
    .unwrap();
    let port = &report.ports[0];
    assert!(port.applied, "{:?}", port.reason);
    assert_eq!(port.port, "gamemode_trenchdigging");
    assert_eq!(port.values["reach"], "10");
    let rules = port.rules.as_ref().expect("the port has rules");
    assert_eq!(rules.id, "gamemode_trenchdigging-rules");

    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    let image = |name: &str| &pack.images[&format!("gamemode_trenchdigging:image/{name}")];
    for (name, command) in [
        ("trenchshovelimage", "dig"),
        ("adminshovelimage", "dig"),
        ("trenchdirtimage", "place"),
        ("admindirtimage", "place"),
    ] {
        assert_eq!(
            image(name).command.as_deref(),
            Some(format!("gamemode_trenchdigging-rules:{command}").as_str()),
            "{name}"
        );
    }
    assert!(image("trenchdirtimage").projectile.is_none());
    assert!(image("admindirtimage").projectile.is_none());
    // onPreFire's swing: armattack on PreFire, root 200 ms in (12 + 12
    // ticks), the shot itself 12 ticks after PreFire starts as before.
    for name in ["trenchshovelimage", "trenchdirtimage"] {
        let states = &image(name).states;
        let arm: Vec<(&str, u32, &str)> = states
            .iter()
            .filter(|s| !s.arm.is_empty())
            .map(|s| (s.name.as_str(), s.ticks, s.arm.as_str()))
            .collect();
        assert_eq!(
            arm,
            [("PreFire", 12, "armattack"), ("FireArmRest", 24, "root")],
            "{name}"
        );
        let fire = states.iter().position(|s| s.name == "Fire").unwrap();
        assert_eq!(states[fire].script, "onFire");
        assert_eq!(
            states[fire].timeout,
            states.iter().position(|s| s.name == "FireArmRest")
        );
        assert_eq!(
            states[states.len() - 1].timeout,
            states.iter().position(|s| s.name == "CheckFire")
        );
    }
    bri_addon_import::ports::check_pins(&out).unwrap();

    // The rules name this import's ids and read its reach.
    let rules_dir = root.join("addons/gamemode_trenchdigging-rules");
    let script = std::fs::read_to_string(rules_dir.join("trench.rhai")).unwrap();
    assert!(script.contains("\"gamemode_trenchdigging:brick/\""));
    assert!(!script.contains("{{"));
    let behaviour: serde_json::Value =
        serde_json::from_slice(&std::fs::read(rules_dir.join("behaviour.json")).unwrap()).unwrap();
    assert_eq!(behaviour["commands"][0]["aim_reach"], 10);
    // server.cs's PlayerNoJet.maxStepHeight = 1.2, as an adjustment the
    // rules provide.
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(rules_dir.join("package.json")).unwrap()).unwrap();
    assert!(
        manifest["provides"].as_array().unwrap().iter().any(|p| p["kind"] == "archetype"
            && p["id"] == "gamemode_trenchdigging-rules:archetype/playernojet"
            && p["file"] == "archetypes/playernojet.json"),
        "{manifest}"
    );
    let adjust: serde_json::Value = serde_json::from_slice(
        &std::fs::read(rules_dir.join("archetypes/playernojet.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(adjust["adjusts"], "v20.player.playernojet");
    assert_eq!(adjust["movement"]["step_height"], 1.2);
    std::fs::remove_dir_all(dir).unwrap();
}

fn ported(name: &str, addon: &str) -> (PathBuf, PathBuf, bri_addon_import::report::Report) {
    let dir = fresh(name);
    let out = dir.join("package");
    let report = import(&options(fixture(&format!("ports/{addon}")), out.clone())).unwrap();
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    assert!(report.needs_behaviour.iter().all(|n| n.port.is_some()));
    bri_addon_import::ports::check_pins(&out).unwrap();
    (dir, out, report)
}

fn holder(package: &Path, items: &[&str]) -> WeaponsWorld {
    let pack =
        Pack::from_json(&std::fs::read(package.join("assets/weapons.json")).unwrap()).unwrap();
    let mut world = WeaponsWorld::new(pack).unwrap();
    world.add_actor(ActorId(1), 5).unwrap();
    for (slot, item) in items.iter().enumerate() {
        world.give_at(ActorId(1), slot, item).unwrap();
    }
    world.equip(ActorId(1), Some(0)).unwrap();
    world
}

fn run(world: &mut WeaponsWorld, ticks: usize) -> Vec<Event> {
    (0..ticks).flat_map(|_| world.step(&mut Empty)).collect()
}

fn arm(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Animation {
                thread: 2,
                sequence,
                ..
            } => Some(sequence.clone()),
            _ => None,
        })
        .collect()
}

fn launched(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Spawned { definition, .. } => Some(definition.clone()),
            _ => None,
        })
        .collect()
}

/// The stand-in's v20 states and scripts: a click lets go during Charge
/// (0.5 s), so `onFiretwo` (its later definition) jabs with
/// `butterflyknifeProjectile`; held past Charge, letting go runs `onFire`,
/// `spearThrow` then `Parent::onFire` with the image's
/// `butterflyknifekillProjectile`. `onCharge` raises the arm with
/// `spearReady` and `onStopFire` lowers it with `root`.
#[test]
fn butterfly_knife_port_jabs_and_stabs() {
    let (dir, out, report) = ported("butterfly-knife", "Weapon_ButterflyKnife");
    assert_eq!(report.ports[0].values["jab"], "butterflyknifeProjectile");
    let knife = "weapon_butterflyknife:weapon/butterflyknifeitem";
    let mut w = holder(&out, &[knife]);
    run(&mut w, 60);
    w.trigger(ActorId(1), true).unwrap();
    let mut events = run(&mut w, 6);
    w.trigger(ActorId(1), false).unwrap();
    events.extend(run(&mut w, 120));
    assert_eq!(
        launched(&events),
        ["weapon_butterflyknife:projectile/butterflyknifeprojectile"]
    );
    assert_eq!(
        arm(&events),
        ["spearReady", "root"],
        "the jab swings no arm in v20"
    );

    w.trigger(ActorId(1), true).unwrap();
    let mut events = run(&mut w, 59);
    assert!(launched(&events).is_empty(), "still charging");
    events.extend(run(&mut w, 30));
    w.trigger(ActorId(1), false).unwrap();
    events.extend(run(&mut w, 60));
    assert_eq!(
        launched(&events),
        ["weapon_butterflyknife:projectile/butterflyknifekillprojectile"]
    );
    assert_eq!(arm(&events), ["spearReady", "spearThrow"]);
    let pack = &w.pack;
    assert_eq!(
        pack.projectiles["weapon_butterflyknife:projectile/butterflyknifeprojectile"].damage,
        20.0
    );
    assert_eq!(
        pack.projectiles["weapon_butterflyknife:projectile/butterflyknifekillprojectile"].damage,
        80.0
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// The stand-in's v20 states and scripts: the first press goes to Pindrop,
/// which ejects the pin (`stateEjectShell`) and fires nothing. The second,
/// held through Charge (0.5 s, `onCharge`: `spearReady`) and let go, runs
/// `onFire`: `spearThrow`, `Parent::onFire`, then the grenade leaves its
/// tool slot and the hand (`serverCmdUnUseTool`). Another grenade stays.
/// The thrown one plays `hegrenadeBounceSound` when it bounces.
#[test]
fn he_grenade_port_pulls_the_pin_then_throws_it_away() {
    let (dir, out, report) = ported("he-grenade", "Weapon_HEGrenade");
    assert_eq!(
        report.ports[0].values["bounce_sound"],
        "hegrenadeBounceSound"
    );
    let grenade = "weapon_hegrenade:weapon/hegrenadeitem";
    let mut w = holder(&out, &[grenade, grenade]);
    run(&mut w, 30);
    w.trigger(ActorId(1), true).unwrap();
    let mut events = run(&mut w, 2);
    w.trigger(ActorId(1), false).unwrap();
    events.extend(run(&mut w, 60));
    assert!(
        events.iter().any(|e| matches!(e, Event::Shell { .. })),
        "the pin flies off"
    );
    assert!(launched(&events).is_empty(), "a click only pulls the pin");

    w.trigger(ActorId(1), true).unwrap();
    let mut events = run(&mut w, 70);
    w.trigger(ActorId(1), false).unwrap();
    events.extend(run(&mut w, 4));
    assert_eq!(
        launched(&events),
        ["weapon_hegrenade:projectile/hegrenadeprojectile"]
    );
    assert_eq!(arm(&events), ["spearReady", "spearThrow"]);
    let a = w.actor(ActorId(1)).unwrap();
    assert_eq!(a.inventory[0], None, "the thrown grenade left the tools");
    assert_eq!(a.inventory[1].as_deref(), Some(grenade));
    assert!(w.image_state(ActorId(1), 0).is_none(), "the hand is empty");

    let p = &w.pack.projectiles["weapon_hegrenade:projectile/hegrenadeprojectile"];
    let bounce = &w.pack.explosions[&p.bounce_effect.to_ascii_lowercase()];
    assert_eq!(bounce.sound, "weapon_hegrenade:sound/hegrenadebouncesound");
    assert!(w.pack.sounds.contains_key(&bounce.sound));
    std::fs::remove_dir_all(dir).unwrap();
}
