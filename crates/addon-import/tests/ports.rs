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
/// A copy of `from` with every text file's lines ended `\r\n`, as a
/// Windows checkout with `core.autocrlf` (or a copy saved on Windows) has.
fn crlf_copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let path = e.path();
        let dest = to.join(e.file_name());
        if path.is_dir() {
            crlf_copy(&path, &dest);
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let text = ["cs", "rhai", "json"]
            .iter()
            .any(|x| path.extension().is_some_and(|e| e == *x));
        let bytes = match String::from_utf8(bytes) {
            Ok(t) if text => t.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes(),
            Ok(t) => t.into_bytes(),
            Err(e) => e.into_bytes(),
        };
        std::fs::write(dest, bytes).unwrap();
    }
}

#[test]
fn a_copy_and_ports_with_windows_line_endings_import_as_with_unix_ones() {
    let dir = fresh("crlf");
    let ports_dir = dir.join("ports");
    crlf_copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("ports"), &ports_dir);
    let crlf_ports = Ports::from_dir(&ports_dir).unwrap();
    let copy = dir.join("Gamemode_Slayer");
    crlf_copy(&fixture("ports/Gamemode_Slayer"), &copy);
    let lf = dir.join("lf/gamemode_slayer");
    let crlf = dir.join("crlf/gamemode_slayer");
    let report = import_with(&options(fixture("ports/Gamemode_Slayer"), lf.clone()), &Ports::builtin()).unwrap();
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    let report = import_with(&options(copy, crlf.clone()), &crlf_ports).unwrap();
    assert!(report.ports[0].applied, "{:?}", report.ports[0].reason);
    for file in ["slayer.rhai", "behaviour.json"] {
        let read = |out: &Path| std::fs::read_to_string(out.with_file_name("gamemode_slayer-rules").join(file)).unwrap();
        let (lf, crlf) = (read(&lf), read(&crlf));
        assert!(!crlf.contains('\r'), "{file} keeps Windows line endings");
        assert_eq!(lf, crlf, "{file}");
    }
}

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
            // The loop's `slayerSound`, renamed at run time, is what the
            // port's datablocks.cs declares.
            let voices = report.datablocks.iter().find(|d| d.name == "slayerSound").unwrap();
            assert_eq!(voices.status, "consumed", "{voices:?}");
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
    // The team kit and uniform, from the copy's team preferences.
    assert_eq!(setting(&behaviour, "team_uniform")["default"], 3);
    assert_eq!(setting(&behaviour, "team_equip_0")["default"], "v20.weapon.hammeritem");
    assert_eq!(setting(&behaviour, "team_equip_3")["default"], "");
    assert_eq!(setting(&behaviour, "team_player_type")["default"], "v20.player.playerstandardarmor");
    assert_eq!(setting(&behaviour, "uni_hat")["default"], 1);
    assert_eq!(setting(&behaviour, "uni_head_color")["default"], "0.5 0.25 0 1");
    assert_eq!(setting(&behaviour, "allow_custom_faces")["default"], false);
    assert!(slayer.contains(r#"let skins = ["0.9 0.8 0.6 1", "0.9 0.8 0.6 1", "0.4 0.3 0.2 1""#));
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
    // The flag's light (`flagHasLight`, `flagLightRadius`), white so it
    // takes the carrier's team colour as the flag does.
    assert_eq!(
        image.light,
        Some(bri_weapons::ImageLight { radius: 12.0, color: [1.0; 3] })
    );
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
    // The player's side: Slayer's keys open screens, its help pages come
    // from the copy's own `.hfl` (the stand-in's), the first one welcoming,
    // and its holiday splash shows its pictures, each an image of the copy.
    let client = bri_package_runtime::Catalog::load(&root, &set, false).unwrap_or_else(|e| panic!("{e:#?}"));
    let slayer = &client.packages["gamemode_slayer"];
    let binds: Vec<_> = slayer.binds.values().flat_map(|b| &b.binds).collect();
    let screens: Vec<_> = binds.iter().filter_map(|b| b.screen.as_deref()).collect();
    assert_eq!(screens, ["minigame_addons", "options"]);
    let pages: Vec<_> = slayer.help.values().flat_map(|h| &h.pages).collect();
    assert_eq!(pages.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), ["Slayer", "Slayer Guide"]);
    assert!(pages[0].welcome && !pages[1].welcome);
    assert!(pages[1].text.contains("<font:arial bold:24>Stand-in Guide"), "{}", pages[1].text);
    let splash = slayer.splashes.values().next().expect("splash");
    assert_eq!((splash.from, splash.to), ([12, 20], [12, 31]));
    for file in splash.layers.iter().map(|l| &l.image).chain(&splash.falling.as_ref().unwrap().images) {
        let asset = slayer.assets.iter().find(|a| &a.file == file).unwrap_or_else(|| panic!("no {file}"));
        assert_eq!(asset.kind, bri_package_runtime::content::Kind::Image);
        assert!(!asset.bytes.is_empty(), "{file}");
    }
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
            distance: 6,
            body: bri_sim::session::OrbitBody::Acts,
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
    // A spawn brick, of a build no one here owns, whose stand-in plane
    // (crates/vehicles/tests/fixtures) takes its colour.
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
    pad.vehicle = Some(Box::new(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved(
            "test_plane:vehicle/standinplane".into(),
        ),
        recolor: true,
    }));
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
        .join("../vehicles/tests/fixtures/stand-in-plane/assets/vehicles.json");
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

/// The stand-in Duplicator imported with its port, hosted on a flat floor
/// with 2x1 plates, and the host joined.
fn duplorcator_game(name: &str) -> (PathBuf, bri_sim::session::Session, u64) {
    let (dir, s, host, report) = duplicator_game(name, "Tool_Duplicator", "tool_duplicator");
    let applied = &report.ports[0];
    // Read from the stand-in's own script, not the original's.
    assert_eq!(applied.values["reach"], "8");
    assert_eq!(applied.values["highlight_ms"], "2000");
    let pack = Pack::from_json(
        &std::fs::read(dir.join("content/addons/tool_duplicator/assets/weapons.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        pack.images["tool_duplicator:image/duplorcatorimage"]
            .command
            .as_deref(),
        Some("tool_duplicator-rules:fire")
    );
    (dir, s, host)
}

/// A stand-in duplicator Add-On (`fixture` under the port fixtures)
/// imported with its port as `id`, hosted on a flat floor with 2x1 plates,
/// and the host (an admin) joined.
fn duplicator_game(
    name: &str,
    fixture_name: &str,
    id: &str,
) -> (PathBuf, bri_sim::session::Session, u64, bri_addon_import::report::Report) {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    use bri_sim::session::Session;
    use rapier3d::prelude::*;

    let dir = fresh(name);
    let root = dir.join("content");
    let out = root.join(format!("addons/{id}"));
    let report = import(&options(fixture(&format!("ports/{fixture_name}")), out.clone())).unwrap();
    let applied = &report.ports[0];
    assert!(applied.applied, "{:?}", applied.reason);
    let rules = applied.rules.as_ref().expect("the port has host rules");
    assert_eq!(rules.id, format!("{id}-rules"));
    let pack = Pack::from_json(&std::fs::read(out.join("assets/weapons.json")).unwrap()).unwrap();
    // A flat floor and a 2x1 plate.
    let mesh = bri_content::brick::Brick {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        // A plain brick, as v20's `BRICK` geometry is.
        collision_boxes: vec![bri_content::brick::CollisionBox {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
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
            entry(id, Side::Shared),
            entry(&format!("{id}-rules"), Side::Server),
        ],
    };
    let catalog = bri_package_runtime::Catalog::load(&root, &set, true)
        .unwrap_or_else(|e| panic!("{e:#?}"));
    s.install_packages(std::sync::Arc::new(catalog), None).unwrap();

    let host = s.join("Host".into(), Vec3::new(0.0, 0.05, 2.0), true).unwrap();
    (dir, s, host, report)
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
            flipped: false,
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

    // One brick over the admin limit: it asks first, and yes loads what
    // fits.
    let tower: Vec<_> = (0..5001)
        .map(|i| loose([0.0, 0.2 * i as f32, 0.0]))
        .collect();
    store.put_loose("Huge", tower, vec![[0.0, 1.0, 1.0, 1.0]; 64]);
    for _ in 0..130 {
        s.step().unwrap();
    }
    s.take_private_notices();
    cmd(
        &mut s,
        Command::Package(PackageCommand {
            package: String::new(),
            command: "loaddup".into(),
            args: vec![PackageArg::String("huge".into())],
        }),
    )
    .unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let asked = s.take_private_notices().into_iter().find_map(|(_, n)| match n {
        Notice::Question {
            package, command, ..
        } => Some((package, command)),
        _ => None,
    });
    let (package, command) = asked.expect("the load asked first");
    assert_eq!(s.blueprint(host).unwrap().bricks.len(), 2, "nothing loaded yet");
    let prints = {
        cmd(
            &mut s,
            Command::Package(PackageCommand {
                package,
                command,
                args: vec![],
            }),
        )
        .unwrap();
        for _ in 0..3 {
            s.step().unwrap();
        }
        s.take_private_notices()
    };
    assert_eq!(s.blueprint(host).unwrap().bricks.len(), 5000);
    assert!(
        prints.iter().any(|(_, n)| matches!(n, Notice::Center { text, .. } if text.contains("5000<color:99AAAA>/\\c45001"))),
        "{prints:?}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// The rest of the Duplorcator on the port: drawing the wand with nothing
/// selected says Normal Mode; a copy of over 50 bricks (its "Max Flood
/// Bypass") waits the admin's planting timeout, a second, after a plant,
/// with the flood error and how long; the cancel key lets the selection
/// go; and with client loading off there is no cached duplication.
#[test]
fn duplorcator_port_waits_after_big_plants_and_cancels() {
    use bri_sim::session::{
        Command, MemoryCopies, Notice, PackageArg, PackageCommand, Reply, Session,
    };
    use bri_sim::simulation::PlantFailure;
    use std::sync::Arc;

    let (dir, mut s, host) = duplorcator_game("duplorcator-waits");
    let store = Arc::new(MemoryCopies::default());
    s.set_copy_store(store.clone());
    let seq = std::cell::Cell::new(0u64);
    let cmd = |s: &mut Session, command: Command| {
        seq.set(seq.get() + 1);
        s.command(host, seq.get(), command)
    };
    let notices = |s: &mut Session| -> Vec<Notice> {
        for _ in 0..3 {
            s.step().unwrap();
        }
        s.take_private_notices().into_iter().map(|(_, n)| n).collect()
    };
    let typed = |s: &mut Session, command: &str, arg: Option<&str>| {
        let reply = cmd(
            s,
            Command::Package(PackageCommand {
                package: String::new(),
                command: command.into(),
                args: arg
                    .map(|n| vec![PackageArg::String(n.into())])
                    .unwrap_or_default(),
            }),
        );
        assert!(reply.is_ok(), "/{command}: {reply:?}");
    };
    let text = |n: &Notice| match n {
        Notice::Center { text, .. } | Notice::Bottom { text, .. } => Some(text.clone()),
        _ => None,
    };

    typed(&mut s, "dup", None);
    let drawn = notices(&mut s);
    assert!(
        drawn.iter().filter_map(text).any(|t| t.contains("Normal") && t.contains("No bricks selected")),
        "{drawn:?}"
    );

    // A tower of 60 plates loaded as a duplication.
    let tower: Vec<_> = (0..60)
        .map(|i| {
            bri_world::Brick::new(
                bri_world::ContentRef::Resolved("plate".into()),
                [0.0, 0.2 * i as f32, 0.0],
                0,
            )
        })
        .collect();
    store.put_loose("Tower", tower, vec![[1.0; 4]; 64]);
    typed(&mut s, "loaddup", Some("tower"));
    notices(&mut s);
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(60));

    let place = |s: &mut Session, x: f32| {
        cmd(
            s,
            Command::PlaceBlueprint {
                position: [x, 0.0, 0.0],
                quarter_turns: 0,
                mirrored: false,
                flipped: false,
            },
        )
    };
    let reply = place(&mut s, 3.0);
    assert!(matches!(reply, Ok(Reply::Planted(_))), "{reply:?}");
    notices(&mut s);
    let bricks = s.snapshot().world.bricks.len();
    let _ = place(&mut s, 6.0);
    let refused = notices(&mut s);
    assert_eq!(s.snapshot().world.bricks.len(), bricks, "too soon");
    assert!(
        refused.iter().any(|n| matches!(n, Notice::PlantError(PlantFailure::Limit))),
        "the flood error: {refused:?}"
    );
    assert!(
        refused.iter().filter_map(text).any(|t| t.contains("You must wait") && t.contains("1") && t.contains("before planting again")),
        "{refused:?}"
    );
    for _ in 0..120 {
        s.step().unwrap();
    }
    let reply = place(&mut s, 6.0);
    assert!(matches!(reply, Ok(Reply::Planted(_))), "a second on: {reply:?}");
    assert_eq!(s.snapshot().world.bricks.len(), bricks + 60);

    // The cancel key lets the selection go.
    cmd(&mut s, Command::CancelBrick).unwrap();
    let canceled = notices(&mut s);
    assert!(s.blueprint(host).is_none(), "the selection is gone");
    assert!(
        canceled.iter().filter_map(text).any(|t| t.contains("Normal") && t.contains("No bricks selected")),
        "{canceled:?}"
    );

    typed(&mut s, "reloaddup", None);
    let reloaded = notices(&mut s);
    assert!(
        reloaded.iter().filter_map(text).any(|t| t == "You have no cached duplication"),
        "{reloaded:?}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// What the host told the player since last asked: centre and bottom
/// prints and chat lines.
fn told(s: &mut bri_sim::session::Session) -> Vec<String> {
    use bri_sim::session::Notice;
    s.take_private_notices()
        .into_iter()
        .filter_map(|(_, n)| match n {
            Notice::Center { text, .. } | Notice::Bottom { text, .. } | Notice::Chat(text) => {
                Some(text)
            }
            _ => None,
        })
        .collect()
}

/// A command the player's client sends: a typed one (no package), or a key
/// the held duplicator takes.
fn send(
    s: &mut bri_sim::session::Session,
    host: u64,
    seq: &std::cell::Cell<u64>,
    command: bri_sim::session::Command,
) -> anyhow::Result<bri_sim::session::Reply> {
    seq.set(seq.get() + 1);
    s.command(host, seq.get(), command)
}

fn typed(command: &str, args: &[&str]) -> bri_sim::session::Command {
    bri_sim::session::Command::Package(bri_sim::session::PackageCommand {
        package: String::new(),
        command: command.into(),
        args: args
            .iter()
            .map(|a| bri_sim::session::PackageArg::String((*a).into()))
            .collect(),
    })
}

fn nd_key(command: &str, args: Vec<bri_sim::session::PackageArg>) -> bri_sim::session::Command {
    bri_sim::session::Command::Package(bri_sim::session::PackageCommand {
        package: "tool_newduplicator-rules".into(),
        command: command.into(),
        args,
    })
}

/// The New Duplicator's shapes `key` that every player sees for `owner`.
fn nd_shapes(
    s: &bri_sim::session::Session,
    owner: u64,
    key: &str,
) -> Vec<bri_package_runtime::ops::WorldShape> {
    let end = format!("/{owner}/{key}");
    s.world_shapes()
        .into_iter()
        .find(|(k, _)| k.ends_with(&end))
        .map_or_else(Vec::new, |(_, set)| set.to_vec())
}
/// The corners of `owner`'s selection box, from its outer faces (0.01
/// outside it).
fn nd_box(s: &bri_sim::session::Session, owner: u64) -> Option<([f32; 3], [f32; 3])> {
    let outer = nd_shapes(s, owner, "box").into_iter().find(|x| x.color == [0, 0, 0, 89])?;
    let snap = |v: f32| (v * 1e4).round() / 1e4;
    Some((outer.min.map(|v| snap(v + 0.01)), outer.max.map(|v| snap(v - 0.01))))
}

/// The stand-in New Duplicator hosted, with a plate, a half-on plate on it
/// and a lone plate beside them; the duplicator in hand and the host
/// looking down at the bottom plate's uncovered half.
fn new_duplicator_game(
    name: &str,
) -> (
    PathBuf,
    bri_sim::session::Session,
    u64,
    std::cell::Cell<u64>,
    u64,
) {
    use bri_sim::session::{Command, Reply};

    let (dir, mut s, host, report) =
        duplicator_game(name, "Tool_NewDuplicator", "tool_newduplicator");
    let values = &report.ports[0].values;
    // Read from the stand-in's own script, not the original's.
    assert_eq!(values["reach"], "12");
    assert_eq!(values["max_bricks_player"], "3");
    assert_eq!(values["super_studs"], "4");
    assert_eq!(values["super_plates"], "10");
    let pack = Pack::from_json(
        &std::fs::read(dir.join("content/addons/tool_newduplicator/assets/weapons.json")).unwrap(),
    )
    .unwrap();
    // Lying in the world it loops the sequence its `ND_Item::onAdd` plays.
    assert_eq!(pack.items["tool_newduplicator:weapon/nd_item"].idle, "spin");
    for image in ["nd_image", "nd_image_box", "nd_image_blue"] {
        let image = &pack.images[&format!("tool_newduplicator:image/{image}")];
        assert_eq!(image.command.as_deref(), Some("tool_newduplicator-rules:fire"));
        // Unloaded while a job runs, it spins (`stateSpinThread`).
        let ready = &image.states[1];
        let spin = ready.not_loaded.map(|i| &image.states[i]).unwrap();
        assert_eq!((ready.spin, spin.spin), (bri_weapons::Spin::Stop, bri_weapons::Spin::FullSpeed));
        assert_eq!(spin.loaded, Some(1));
        assert_eq!(
            image.commands.unmount.as_deref(),
            Some("tool_newduplicator-rules:unmount")
        );
    }
    let seq = std::cell::Cell::new(0u64);
    let mut base = 0;
    for (i, position) in [[0.5, 0.1, 0.25], [1.0, 0.3, 0.25], [2.5, 0.1, 0.25]]
        .into_iter()
        .enumerate()
    {
        let reply = send(
            &mut s,
            host,
            &seq,
            Command::Plant {
                definition: "plate".into(),
                position,
                quarter_turns: 0,
                color: 1,
            },
        );
        match reply {
            Ok(Reply::Planted(id)) if i == 0 => base = id,
            Ok(Reply::Planted(_)) => {}
            other => panic!("plant at {position:?}: {other:?}"),
        }
    }
    // /d, the shortest of its names, puts it in hand.
    send(&mut s, host, &seq, typed("d", &[])).unwrap();
    for tick in 0..30u64 {
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
        s.step().unwrap();
    }
    (dir, s, host, seq, base)
}

/// One tick of the host looking down at the bottom plate's uncovered half.
fn look(s: &mut bri_sim::session::Session, host: u64) {
    let sequence = s.snapshot().world.tick + 1000;
    s.movement(
        host,
        sequence,
        bri_sim::player::MoveInput {
            yaw: 0.142,
            pitch: -0.85,
            ..Default::default()
        },
    )
    .unwrap();
    s.step().unwrap();
}

/// Swing the duplicator where the host looks, once a newly mounted image
/// is ready.
fn swing(s: &mut bri_sim::session::Session, host: u64, seq: &std::cell::Cell<u64>) {
    use bri_sim::session::Command;
    for _ in 0..20 {
        look(s, host);
    }
    for down in [true, false] {
        send(s, host, seq, Command::WeaponTrigger { down }).unwrap();
        look(s, host);
    }
    for _ in 0..30 {
        look(s, host);
    }
}

/// The New Duplicator's port on the stand-in in a hosted game: taking it
/// out starts stack mode; a click selects the stack up from a brick, lights
/// it in its own colours and holds it; planting it says what fit and what
/// was blocked and goes to plant mode; cancel goes back. [Light] gives box
/// mode, where a click boxes a brick, the brick keys grow the box and
/// planting selects what lies in it.
#[test]
fn new_duplicator_port_selects_stacks_and_boxes_and_plants() {
    use bri_sim::session::{Command, Notice, PackageArg, Reply, ToolAction};

    let (dir, mut s, host, seq, base) = new_duplicator_game("new-duplicator");
    assert!(
        s.tool_inventories()[&host]
            .slots
            .iter()
            .any(|t| t.as_deref() == Some("tool_newduplicator:weapon/nd_item"))
    );
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains("Selection Mode") && t.contains("Select stack up")),
        "taking it out starts stack mode: {prints:?}"
    );
    // Ctrl+V means nothing in stack mode (`NewDuplicatorMode::onPaste`).
    send(&mut s, host, &seq, nd_key("ndpaste", vec![])).unwrap();
    s.step().unwrap();
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains("Paste can not be used in your current duplicator mode.")),
        "{prints:?}"
    );

    swing(&mut s, host, &seq);
    if s.blueprint(host).is_none() {
        panic!("the click selected the stack: {:?}", told(&mut s));
    }
    let copy = s.blueprint(host).unwrap().clone();
    assert_eq!(copy.bricks.len(), 2);
    assert_eq!(copy.tool, "tool_newduplicator:weapon/nd_item");
    // Everyone sees gold edges round the selection.
    let highlight = nd_shapes(&s, host, "highlight");
    assert_eq!(highlight.len(), 12, "{highlight:#?}");
    assert!(highlight.iter().all(|x| x.color == [255, 214, 0, 252]));
    let world = s.snapshot().world;
    assert_eq!(
        (world.bricks[&base].color, world.bricks[&base].color_effect),
        (1, 3),
        "the selection glows in its own colour"
    );
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"Selected \c32\c6 Bricks!")),
        "{prints:?}"
    );
    assert!(
        prints.iter().any(|t| t.contains(r"Selection Mode (\c32\c6 Bricks)")
            && t.contains("<just:right>")
            && t.contains("[Plant Brick]: Duplicate")),
        "{prints:?}"
    );

    // Held as a selection: no ghost until a brick key takes it up.
    assert!(
        !s.take_private_notices()
            .into_iter()
            .any(|(_, n)| matches!(n, Notice::Blueprint(Some(_)))),
        "the selection is not a ghost yet"
    );
    send(&mut s, host, &seq, nd_key("plant", vec![])).unwrap();
    s.step().unwrap();
    assert!(
        s.take_private_notices()
            .into_iter()
            .any(|(_, n)| matches!(n, Notice::Blueprint(Some(_)))),
        "[Plant Brick] takes the selection up as a ghost"
    );
    // Everyone sees a blue box round the ghost, following it as its
    // player turns and moves it (`spawnGhostBricks`, `getGhostWorldBox`),
    // gone once it is put away.
    let pose = bri_sim::session::CopyPose {
        anchor: [copy.origin[0] + 1.0, copy.origin[1], copy.origin[2]],
        quarter_turns: 1,
        mirrored: true,
        flipped: false,
    };
    send(&mut s, host, &seq, Command::CopyPose(Some(pose))).unwrap();
    s.step().unwrap();
    s.step().unwrap();
    let highlight = nd_shapes(&s, host, "highlight");
    assert_eq!(highlight.len(), 12, "{highlight:#?}");
    assert!(
        highlight.iter().all(|x| x.color == [51, 51, 255, 252]),
        "{highlight:#?}"
    );
    let (min, max) = copy.ghost_box(pose.anchor, 1, (false, true));
    let middle =
        |a: [f32; 3], b: [f32; 3]| std::array::from_fn::<f32, 3, _>(|i| (a[i] + b[i]) * 0.5);
    let low = highlight.iter().fold([f32::MAX; 3], |m, x| {
        std::array::from_fn(|i| m[i].min(x.min[i]))
    });
    let high = highlight.iter().fold([f32::MIN; 3], |m, x| {
        std::array::from_fn(|i| m[i].max(x.max[i]))
    });
    let (got, want) = (middle(low, high), middle(min, max));
    assert!(
        (0..3).all(|i| (got[i] - want[i]).abs() < 1e-3),
        "{got:?} vs {want:?}"
    );
    // Turned a quarter, the 2x1 copy's box lies along z.
    assert!(high[2] - low[2] > high[0] - low[0], "{low:?}..{high:?}");
    send(&mut s, host, &seq, Command::CopyPose(None)).unwrap();
    s.step().unwrap();
    s.step().unwrap();
    assert!(nd_shapes(&s, host, "highlight").is_empty());

    // Planted over the lone plate: the bottom plate is blocked, the top
    // one plants on it.
    let before = s.snapshot().world.bricks.len();
    let reply = send(
        &mut s,
        host,
        &seq,
        Command::PlaceBlueprint {
            position: [2.5, 0.0, 0.0],
            quarter_turns: 0,
            mirrored: false,
            flipped: false,
        },
    );
    assert!(matches!(reply, Ok(Reply::Planted(_))), "{reply:?}");
    assert_eq!(s.snapshot().world.bricks.len(), before + 1);
    for _ in 0..3 {
        s.step().unwrap();
    }
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"Planted \c31\c6 / \c32\c6 Brick!")
            && t.contains(r"\c31\c6 blocked.")),
        "{prints:?}"
    );
    assert!(
        prints.iter().any(|t| t.contains(r"Plant Mode (\c32\c6 Bricks)")
            && t.contains(r"Size: \c33\c6 x \c31\c6 x \c32\c6 Plates")),
        "{prints:?}"
    );
    send(&mut s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
    assert_eq!(s.snapshot().world.bricks.len(), before);

    // Cancel leaves plant mode, letting the selection go.
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    assert!(s.blueprint(host).is_none());
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains("Click Brick: Select stack up")),
        "{prints:?}"
    );

    // [Light]: box mode. A click boxes the brick it hits.
    send(&mut s, host, &seq, Command::ToggleLight).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert!(told(&mut s).iter().any(|t| t.contains(r"Type: \c3Box")));
    swing(&mut s, host, &seq);
    let outline = nd_box(&s, host).expect("the click put a box round the brick");
    assert_eq!(outline, ([0.0, 0.0, 0.0], [1.0, 0.2, 0.5]));
    // Everyone sees it: see-through faces, twelve edges, two corner
    // cubes and its name.
    let shapes = nd_shapes(&s, host, "box");
    assert_eq!(shapes.len(), 17, "{shapes:#?}");
    let name = s.names()[&host].clone();
    assert!(
        shapes
            .iter()
            .any(|x| x.label.starts_with(&name) && x.label.ends_with(" Selection Box") && x.color[..3] == [255, 214, 0]),
        "{shapes:#?}"
    );
    assert!(shapes.iter().any(|x| x.inside == [0, 0, 0, 153]));
    // The brick keys move its top corner: a brick (3 plates) up, then a
    // super shift of the stand-in's 10 plates down, which the 8-unit box
    // allows.
    send(
        &mut s,
        host,
        &seq,
        nd_key(
            "shift",
            vec![
                PackageArg::Int(0),
                PackageArg::Int(0),
                PackageArg::Int(3),
                PackageArg::Bool(false),
            ],
        ),
    )
    .unwrap();
    s.step().unwrap();
    let grown = nd_box(&s, host).expect("the box grew");
    assert!((grown.1[1] - 0.8).abs() < 1e-4, "{grown:?}");
    // Plant selects what lies wholly in the box: the bottom plate, not the
    // one half over its edge.
    send(&mut s, host, &seq, nd_key("plant", vec![])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(1));
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains("Press [Cancel Brick] to adjust the box.")),
        "{prints:?}"
    );
    // Cancel lets the selection go and keeps the box.
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    assert!(s.blueprint(host).is_none());
    assert!(told(&mut s).iter().any(|t| t.contains("Selection canceled!")));
    // [Prev Seat]: not limited, the box takes what reaches into it too.
    send(&mut s, host, &seq, Command::SwitchSeat(-1)).unwrap();
    s.step().unwrap();
    assert!(told(&mut s).iter().any(|t| t.contains(r"Limited: \c0No")));
    for _ in 0..60 {
        s.step().unwrap();
    }
    send(&mut s, host, &seq, nd_key("plant", vec![])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(2));
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    assert!(nd_box(&s, host).is_none());
    assert!(nd_shapes(&s, host, "highlight").is_empty());
    let diagnostics = s.package_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The New Duplicator's port: /Cut cuts the originals away and goes to
/// plant mode, where /MirrorX mirrors the ghost and /MirrorZ turns it
/// upside down, a click puts the selection against what it hits, and
/// /SaveDup and /LoadDup keep it by name.
#[test]
fn new_duplicator_port_mirrors_cuts_saves_and_loads() {
    use bri_sim::session::{Command, MemoryCopies, Notice};
    use std::sync::Arc;

    let (dir, mut s, host, seq, base) = new_duplicator_game("new-duplicator-saves");
    let store = Arc::new(MemoryCopies::default());
    s.set_copy_store(store.clone());
    swing(&mut s, host, &seq);
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(2));
    told(&mut s);

    // Mirrors only a ghost in plant mode.
    send(&mut s, host, &seq, typed("mx", &[])).unwrap();
    s.step().unwrap();
    assert!(
        told(&mut s)
            .iter()
            .any(|t| t.contains("can only be used in plant mode"))
    );

    // /Cut: the stack is gone, the selection stays to plant.
    let before = s.snapshot().world.bricks.len();
    send(&mut s, host, &seq, typed("cut", &[])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert_eq!(s.snapshot().world.bricks.len(), before - 2);
    assert!(!s.snapshot().world.bricks.contains_key(&base));
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"Cut \c32\c6 Bricks!")),
        "{prints:?}"
    );
    assert!(prints.iter().any(|t| t.contains("Plant Mode")), "{prints:?}");
    send(&mut s, host, &seq, typed("mx", &[])).unwrap();
    send(&mut s, host, &seq, typed("mz", &[])).unwrap();
    s.step().unwrap();
    let notices = s.take_private_notices();
    assert!(
        notices
            .iter()
            .any(|(_, n)| matches!(n, Notice::MirrorCopy { .. }))
    );
    assert!(notices.iter().any(|(_, n)| matches!(n, Notice::FlipCopy)));

    // In plant mode a click puts the selection against what it hits.
    swing(&mut s, host, &seq);
    assert!(
        s.take_private_notices()
            .into_iter()
            .any(|(_, n)| matches!(n, Notice::MoveCopy { .. }))
    );

    // Saved by name (the host is an admin), then loaded back.
    for _ in 0..130 {
        s.step().unwrap();
    }
    send(&mut s, host, &seq, typed("savedup", &["Tower"])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"wrote \c32\c6 Bricks!")),
        "{prints:?}"
    );
    assert_eq!(store.saved("tower").expect("kept").copy.bricks.len(), 2);
    send(&mut s, host, &seq, typed("savedup", &["Bad/Name"])).unwrap();
    s.step().unwrap();
    assert!(told(&mut s).iter().any(|t| t.contains("Bad save name")));

    for _ in 0..130 {
        s.step().unwrap();
    }
    send(&mut s, host, &seq, typed("ld", &["Nothing"])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert!(told(&mut s).iter().any(|t| t.contains("does not exist")));
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    assert!(s.blueprint(host).is_none());
    for _ in 0..130 {
        s.step().unwrap();
    }
    told(&mut s);
    send(&mut s, host, &seq, typed("loaddup", &["tower"])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"got \c32\c6 Bricks!")),
        "{prints:?}"
    );
    assert!(prints.iter().any(|t| t.contains("Plant Mode")), "{prints:?}");
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(2));
    let diagnostics = s.package_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// [`swing`] with Ctrl held, the original's multiselect key: its bind
/// sends `/NdMultiSelect` as Ctrl goes down and again as it comes up.
fn swing_multi(s: &mut bri_sim::session::Session, host: u64, seq: &std::cell::Cell<u64>) {
    use bri_sim::session::PackageArg;
    send(s, host, seq, nd_key("ndmultiselect", vec![PackageArg::Bool(true)])).unwrap();
    swing(s, host, seq);
    send(s, host, seq, nd_key("ndmultiselect", vec![PackageArg::Bool(false)])).unwrap();
}

/// The answer to the New Duplicator's question, as the player's OK sends it.
fn answer(
    s: &mut bri_sim::session::Session,
    host: u64,
    seq: &std::cell::Cell<u64>,
    asked: &str,
) {
    use bri_sim::session::Notice;
    let question = s.take_private_notices().into_iter().find_map(|(_, n)| match n {
        Notice::Question { package, command, .. } => Some((package, command)),
        _ => None,
    });
    let (package, command) = question.expect("the port asked first");
    assert_eq!(command, asked);
    send(
        s,
        host,
        seq,
        bri_sim::session::Command::Package(bri_sim::session::PackageCommand {
            package,
            command,
            args: vec![],
        }),
    )
    .unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
}

/// The New Duplicator's port closes its gaps: Ctrl adds to a selection; a
/// paint can then paints it (one Ctrl+Z puts it back); /FillWrench sets
/// every brick's settings; /ForcePlant plants a ghost in mid air; box mode
/// from a selection boxes it, Ctrl grows and moves the box, and /SuperCut
/// and /FillBricks, each asked first, cut the box out and fill it.
#[test]
fn new_duplicator_port_paints_wrenches_supercuts_fills_and_force_plants() {
    use bri_sim::session::{Command, Notice, PackageArg, Reply, ToolAction, WrenchFill};

    let (dir, mut s, host, seq, base) = new_duplicator_game("new-duplicator-gaps");
    swing(&mut s, host, &seq);
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(2));
    assert!(
        s.take_private_notices()
            .into_iter()
            .any(|(_, n)| matches!(n, Notice::TakePaint(true))),
        "with a selection the paint cans pick its fill colour"
    );

    // Down from the bottom plate is that plate alone; Ctrl adds it to the
    // selection, which keeps both.
    send(&mut s, host, &seq, Command::SwitchSeat(1)).unwrap();
    swing_multi(&mut s, host, &seq);
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(2));
    swing(&mut s, host, &seq);
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(1), "without Ctrl it is replaced");
    send(&mut s, host, &seq, Command::SwitchSeat(1)).unwrap();
    swing(&mut s, host, &seq);
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(2));
    told(&mut s);

    // A colour can: fill colour mode, and [Plant Brick] paints.
    send(
        &mut s,
        host,
        &seq,
        nd_key("paint", vec![PackageArg::Bool(false), PackageArg::Int(2)]),
    )
    .unwrap();
    s.step().unwrap();
    assert!(told(&mut s).iter().any(|t| t.contains("Paint Mode")));
    send(&mut s, host, &seq, nd_key("plant", vec![])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let world = s.snapshot().world;
    assert_eq!(
        (world.bricks[&base].color, world.bricks[&base].color_effect),
        (2, 0),
        "painted, and no longer glowing"
    );
    assert!(told(&mut s).iter().any(|t| t.contains(r"Painted \c32\c6 Bricks!")));
    send(&mut s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
    assert_eq!(s.snapshot().world.bricks[&base].color, 1);
    // An FX can: the Undulo shape effect.
    send(
        &mut s,
        host,
        &seq,
        nd_key("paint", vec![PackageArg::Bool(true), PackageArg::Int(8)]),
    )
    .unwrap();
    send(&mut s, host, &seq, nd_key("plant", vec![])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert_eq!(s.snapshot().world.bricks[&base].shape_effect, 1);
    // Cancel goes back to selecting, the tools box back.
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    assert!(
        s.take_private_notices()
            .into_iter()
            .any(|(_, n)| matches!(n, Notice::ScrollMode(_)))
    );
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(2));

    // /FillWrench opens the wrench on the selection; what is ticked goes
    // on every brick.
    send(&mut s, host, &seq, typed("fw", &[])).unwrap();
    s.step().unwrap();
    assert!(
        s.take_private_notices()
            .into_iter()
            .any(|(_, n)| matches!(n, Notice::WrenchCopy { bricks: 2 }))
    );
    let fill = WrenchFill {
        name: Some(Some("tower".into())),
        raycast: Some(false),
        ..Default::default()
    };
    send(&mut s, host, &seq, Command::WrenchCopy(fill)).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let world = s.snapshot().world;
    assert_eq!(world.bricks[&base].name.as_deref(), Some("tower"));
    assert!(!world.bricks[&base].raycast);
    assert!(told(&mut s).iter().any(|t| t.contains(r"Applied changes to \c32\c6 Bricks!")));
    send(&mut s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
    assert_eq!(s.snapshot().world.bricks[&base].name, None);

    // Plant mode; /ForcePlant plants the ghost where it floats.
    send(&mut s, host, &seq, nd_key("plant", vec![])).unwrap();
    send(&mut s, host, &seq, typed("fp", &[])).unwrap();
    s.step().unwrap();
    assert!(
        s.take_private_notices()
            .into_iter()
            .any(|(_, n)| matches!(n, Notice::PlantCopy))
    );
    let before = s.snapshot().world.bricks.len();
    let reply = send(
        &mut s,
        host,
        &seq,
        Command::PlaceBlueprint {
            position: [5.0, 2.0, 5.0],
            quarter_turns: 0,
            mirrored: false,
            flipped: true,
        },
    );
    assert!(matches!(reply, Ok(Reply::Planted(_))), "{reply:?}");
    assert_eq!(s.snapshot().world.bricks.len(), before + 2);
    send(&mut s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
    assert_eq!(s.snapshot().world.bricks.len(), before);
    send(&mut s, host, &seq, typed("tfp", &[])).unwrap();
    s.step().unwrap();
    assert!(told(&mut s).iter().any(|t| t.contains("Force Plant has been enabled")));

    // Back to selecting, the stack again, then box mode boxes it.
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    swing(&mut s, host, &seq);
    send(&mut s, host, &seq, Command::ToggleLight).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let boxed = nd_box(&s, host).expect("a box round the selection");
    assert_eq!(boxed, ([0.0, 0.0, 0.0], [1.5, 0.4, 0.5]));
    assert!(s.blueprint(host).is_none(), "the selection became the box");
    // /SuperCut, asked first, cuts everything reaching into the box.
    let before = s.snapshot().world.bricks.len();
    send(&mut s, host, &seq, typed("sc", &[])).unwrap();
    s.step().unwrap();
    answer(&mut s, host, &seq, "ndconfirmsupercut");
    assert_eq!(s.snapshot().world.bricks.len(), before - 2);
    assert!(told(&mut s).iter().any(|t| t.contains(r"Deleted \c32\c6 Bricks!")));
    send(&mut s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
    assert_eq!(s.snapshot().world.bricks.len(), before);
    // /FillBricks: a supercut, then the box filled with plain plates.
    send(&mut s, host, &seq, typed("fb", &[])).unwrap();
    s.step().unwrap();
    answer(&mut s, host, &seq, "ndconfirmfillbricks");
    for _ in 0..3 {
        s.step().unwrap();
    }
    let prints = told(&mut s);
    assert!(prints.iter().any(|t| t.contains(r"Filled in \c32\c6 bricks")), "{prints:?}");
    // The box is 3 studs by 2 plates: two 2x1 plates fill it, the stud
    // left over takes no plate this catalog has.
    assert_eq!(s.snapshot().world.bricks.len(), before - 2 + 2);
    // Ctrl held, the brick keys move the whole box.
    told(&mut s);
    send(&mut s, host, &seq, nd_key("ndmultiselect", vec![PackageArg::Bool(true)])).unwrap();
    for _ in 0..5 {
        look(&mut s, host);
    }
    send(
        &mut s,
        host,
        &seq,
        nd_key(
            "shift",
            vec![
                PackageArg::Int(0),
                PackageArg::Int(0),
                PackageArg::Int(1),
                PackageArg::Bool(false),
            ],
        ),
    )
    .unwrap();
    s.step().unwrap();
    let moved = nd_box(&s, host).expect("the box moved");
    assert!((moved.0[1] - 0.2).abs() < 1e-4 && (moved.1[1] - 0.6).abs() < 1e-4, "{moved:?}");
    let diagnostics = s.package_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The New Duplicator's port: [Prev Seat] in plant mode turns the ghost
/// about its start brick; /PlantAs plants into another player's group,
/// whose bricks one undo (asked first, over the stand-in's 2) takes back;
/// /MirErrors tells of the last mirrored plant; /SaveDup asks before it
/// overwrites; /AllDups lists the saves; and a player who is no admin
/// waits between plants.
#[test]
fn new_duplicator_port_pivots_plants_as_waits_and_lists() {
    use bri_sim::session::{Command, MemoryCopies, Notice, Reply, ToolAction};
    use std::sync::Arc;

    let (dir, mut s, host, seq, _) = new_duplicator_game("new-duplicator-plant-as");
    let store = Arc::new(MemoryCopies::default());
    s.set_copy_store(store.clone());
    let guest = s.join("Guest".into(), Vec3::new(4.0, 0.05, 4.0), false).unwrap();
    // A third plate on the top one: the stack up from the bottom is 3.
    let reply = send(
        &mut s,
        host,
        &seq,
        Command::Plant {
            definition: "plate".into(),
            position: [1.0, 0.5, 0.25],
            quarter_turns: 0,
            color: 1,
        },
    );
    assert!(matches!(reply, Ok(Reply::Planted(_))), "{reply:?}");
    swing(&mut s, host, &seq);
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(3));
    send(&mut s, host, &seq, nd_key("plant", vec![])).unwrap();
    s.step().unwrap();
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"Pivot: \c3Whole Selection\c6 [Prev Seat]")),
        "{prints:?}"
    );

    // [Prev Seat]: the start brick is the pivot.
    send(&mut s, host, &seq, Command::SwitchSeat(-1)).unwrap();
    s.step().unwrap();
    let notices = s.take_private_notices();
    assert!(
        notices
            .iter()
            .any(|(_, n)| matches!(n, Notice::PivotCopy { whole: false })),
        "{notices:?}"
    );
    assert!(
        notices.iter().any(|(_, n)| matches!(n, Notice::Bottom { text, .. } if text.contains(r"Pivot: \c3Start Brick"))),
        "{notices:?}"
    );

    // /PlantAs: nobody by that name, then the guest (the host is an admin
    // and the stand-in lets admins past trust).
    send(&mut s, host, &seq, typed("pa", &["Nobody"])).unwrap();
    s.step().unwrap();
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r#"No brick group was found for "\c3Nobody\c6""#)),
        "{prints:?}"
    );
    send(&mut s, host, &seq, typed("pa", &["gue"])).unwrap();
    s.step().unwrap();
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"planted in \c3Guest\c6's group!")),
        "{prints:?}"
    );
    assert!(
        prints.iter().any(|t| t.contains(r"Planting as: \c3Guest")),
        "{prints:?}"
    );
    let before = s.snapshot().world.bricks.len();
    let reply = send(
        &mut s,
        host,
        &seq,
        Command::PlaceBlueprint {
            position: [5.0, 0.0, 0.0],
            quarter_turns: 0,
            mirrored: true,
            flipped: false,
        },
    );
    assert!(matches!(reply, Ok(Reply::Planted(_))), "{reply:?}");
    let world = s.snapshot().world;
    assert_eq!(world.bricks.len(), before + 3);
    assert_eq!(world.bricks.values().filter(|b| b.owner == guest).count(), 3);
    for _ in 0..3 {
        s.step().unwrap();
    }
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"Planted \c33\c6 / \c33\c6 Bricks!")),
        "{prints:?}"
    );
    assert!(!prints.iter().any(|t| t.contains("probably mirrored incorrectly")));
    // A plain plate mirrors exactly.
    send(&mut s, host, &seq, typed("me", &[])).unwrap();
    s.step().unwrap();
    assert!(
        told(&mut s)
            .iter()
            .any(|t| t.contains("There were no mirror errors in your last plant attempt."))
    );
    // Over 2 bricks, the first undo only asks.
    send(&mut s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
    s.step().unwrap();
    assert_eq!(s.snapshot().world.bricks.len(), before + 3);
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"Next undo will affect \c33\c6 bricks. Press undo again to continue.")),
        "{prints:?}"
    );
    send(&mut s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
    s.step().unwrap();
    assert_eq!(s.snapshot().world.bricks.len(), before);
    send(&mut s, host, &seq, typed("pa", &[])).unwrap();
    s.step().unwrap();
    assert!(
        told(&mut s)
            .iter()
            .any(|t| t.contains("Bricks will be planted in your own group!"))
    );

    // /SaveDup: a second save by the same name asks first.
    for name in ["Tower", "Tower"] {
        send(&mut s, host, &seq, typed("savedup", &[name])).unwrap();
        for _ in 0..3 {
            s.step().unwrap();
        }
    }
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r#"Save "\c3Tower\c6" already exists. Repeat the command to overwrite."#)),
        "{prints:?}"
    );
    send(&mut s, host, &seq, typed("sd", &["Tower"])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"Finished saving selection, wrote \c33\c6 Bricks!")),
        "{prints:?}"
    );
    assert_eq!(store.saved("tower").expect("kept").copy.bricks.len(), 3);

    // /AllDups, with and without a filter.
    send(&mut s, host, &seq, typed("ad", &[])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"\c31\c6 saved duplication is available:")),
        "{prints:?}"
    );
    assert!(prints.iter().any(|t| t.contains(r" - \c3Tower")), "{prints:?}");
    send(&mut s, host, &seq, typed("alldups", &["zzz"])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r#"No saved duplications are available for filter "\c3zzz\c6"."#)),
        "{prints:?}"
    );
    send(&mut s, host, &seq, typed("ad", &["a/b"])).unwrap();
    s.step().unwrap();
    assert!(told(&mut s).iter().any(|t| t.contains("Bad pattern")));

    // The guest loads it and plants; the next plant waits 2 seconds.
    let gseq = std::cell::Cell::new(0u64);
    send(&mut s, guest, &gseq, typed("ld", &["tower"])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert_eq!(s.blueprint(guest).map(|b| b.bricks.len()), Some(3));
    told(&mut s);
    let before = s.snapshot().world.bricks.len();
    let place = |s: &mut bri_sim::session::Session, x: f32| {
        send(
            s,
            guest,
            &gseq,
            Command::PlaceBlueprint {
                position: [x, 0.0, 4.0],
                quarter_turns: 0,
                mirrored: false,
                flipped: false,
            },
        )
    };
    assert!(matches!(place(&mut s, 6.0), Ok(Reply::Planted(_))));
    assert_eq!(s.snapshot().world.bricks.len(), before + 3);
    s.step().unwrap();
    place(&mut s, 8.0).unwrap();
    s.step().unwrap();
    assert_eq!(s.snapshot().world.bricks.len(), before + 3);
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"You need to wait\c3 2\c6 seconds before planting again!")),
        "{prints:?}"
    );
    for _ in 0..(2 * bri_world::TICKS_PER_SECOND) {
        s.step().unwrap();
    }
    assert!(matches!(place(&mut s, 8.0), Ok(Reply::Planted(_))));
    assert_eq!(s.snapshot().world.bricks.len(), before + 6);
    let diagnostics = s.package_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The New Duplicator's port: its keys (Ctrl C copies, Ctrl V plants,
/// Ctrl X cuts) come with it as a binds file the Controls list shows
/// under "New Duplicator"; /DupVersion and /DupClients name its version;
/// /ClearDups puts every duplicator away, for admins only.
#[test]
fn new_duplicator_port_keys_and_admin_commands() {
    use bri_sim::session::{Command, Notice};

    let (dir, mut s, host, seq, base) = new_duplicator_game("new-duplicator-keys");
    // The keys: a client-side binds file the port adds and provides.
    let addon = dir.join("content/addons/tool_newduplicator");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(addon.join("package.json")).unwrap()).unwrap();
    let provided = manifest["provides"].as_array().unwrap();
    let binds = provided
        .iter()
        .find(|p| p["kind"] == "binds")
        .expect("the binds file is provided");
    assert_eq!(binds["file"], "binds.json");
    let file: serde_json::Value =
        serde_json::from_slice(&std::fs::read(addon.join("binds.json")).unwrap()).unwrap();
    assert_eq!(file["division"], "New Duplicator");
    let keys: Vec<_> = file["binds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["command"].as_str().unwrap(), b["key"].as_str().unwrap_or("")))
        .collect();
    assert!(keys.contains(&("ndcopy", "ctrl c")), "{keys:?}");
    assert!(keys.contains(&("ndmultiselect", "lcontrol")), "{keys:?}");
    assert!(keys.contains(&("fillbricks", "shift-ctrl v")), "{keys:?}");
    assert!(
        file["binds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["package"] == "tool_newduplicator-rules")
    );

    // Ctrl C in stack mode: the selection, held as a ghost to plant.
    swing(&mut s, host, &seq);
    assert_eq!(s.blueprint(host).map(|b| b.bricks.len()), Some(2));
    told(&mut s);
    send(&mut s, host, &seq, nd_key("ndcopy", vec![])).unwrap();
    s.step().unwrap();
    let notices = s.take_private_notices();
    assert!(notices.iter().any(|(_, n)| matches!(n, Notice::Blueprint(Some(_)))));
    assert!(
        notices.iter().any(|(_, n)| matches!(n, Notice::Bottom { text, .. } if text.contains("Plant Mode"))),
        "{notices:?}"
    );
    // Ctrl V plants it.
    send(&mut s, host, &seq, nd_key("ndpaste", vec![])).unwrap();
    s.step().unwrap();
    assert!(
        s.take_private_notices()
            .into_iter()
            .any(|(_, n)| matches!(n, Notice::PlantCopy))
    );
    // Back to selecting; Ctrl X cuts.
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    swing(&mut s, host, &seq);
    let before = s.snapshot().world.bricks.len();
    send(&mut s, host, &seq, nd_key("ndcut", vec![])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert_eq!(s.snapshot().world.bricks.len(), before - 2);
    assert!(!s.snapshot().world.bricks.contains_key(&base));
    told(&mut s);

    // /DupVersion and /DupClients: the stand-in's version.
    send(&mut s, host, &seq, typed("dv", &[])).unwrap();
    s.step().unwrap();
    let prints = told(&mut s);
    assert!(prints.iter().any(|t| t.contains("Blockland version: ")), "{prints:?}");
    assert!(
        prints.iter().any(|t| t.contains(r"New duplicator version: \c39.9.1")),
        "{prints:?}"
    );
    let guest = s.join("Guest".into(), Vec3::new(4.0, 0.05, 4.0), false).unwrap();
    send(&mut s, host, &seq, typed("dupclients", &[])).unwrap();
    s.step().unwrap();
    let prints = told(&mut s);
    for name in ["Host", "Guest"] {
        assert!(
            prints.iter().any(|t| t.contains(&format!(r"\c3{name}\c6 has \c39.9.1"))),
            "{prints:?}"
        );
    }

    // /ClearDups: refused to the guest; the host's puts every one away.
    let gseq = std::cell::Cell::new(0u64);
    send(&mut s, guest, &gseq, typed("cleardups", &[])).unwrap();
    s.step().unwrap();
    assert!(told(&mut s).iter().any(|t| t.contains("admin only")));
    assert!(s.blueprint(host).is_some());
    send(&mut s, host, &seq, typed("cleardups", &[])).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert!(s.blueprint(host).is_none());
    let diagnostics = s.package_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}


/// A plant bigger than a tick's copy work goes on over the next ticks
/// behind the original's progress line (`NDM_PlantCopyProgress`), and
/// [Cancel Brick] stops it, keeping what went in as one undo.
#[test]
fn new_duplicator_port_shows_progress_and_cancels_a_big_plant() {
    use bri_sim::session::{Command, Reply, ToolAction};

    let (dir, mut s, host, seq, _) = new_duplicator_game("new-duplicator-progress");
    swing(&mut s, host, &seq);
    assert_eq!(s.blueprint(host).unwrap().bricks.len(), 2);
    send(&mut s, host, &seq, nd_key("plant", vec![])).unwrap();
    s.step().unwrap();
    told(&mut s);
    // One brick's planting a tick.
    s.set_copy_work(32);
    for _ in 0..3 {
        s.step().unwrap();
    }
    let before = s.snapshot().world.bricks.len();
    let reply = send(
        &mut s,
        host,
        &seq,
        Command::PlaceBlueprint {
            position: [-2.5, 0.0, -2.0],
            quarter_turns: 0,
            mirrored: false,
            flipped: false,
        },
    );
    assert!(matches!(reply, Ok(Reply::Accepted)), "{reply:?}");
    assert!(s.copy_working(host));
    // The first brick went in at once; [Cancel Brick] stops the second.
    assert_eq!(s.snapshot().world.bricks.len(), before + 1);
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    assert!(!s.copy_working(host));
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains(r"Planting... (\c350%\c6, \c30\c6 failed)")
            && t.contains("[Cancel Brick]: Cancel planting")),
        "{prints:?}"
    );
    assert!(
        prints.iter().any(|t| t.contains("Planting canceled!")),
        "{prints:?}"
    );
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert_eq!(s.snapshot().world.bricks.len(), before + 1);
    // What went in is one undo.
    send(&mut s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert_eq!(s.snapshot().world.bricks.len(), before);
    std::fs::remove_dir_all(dir).unwrap();
}

/// The New Duplicator's port: /SuperCut and /FillBricks go a slice a tick
/// behind the original's progress line, [Cancel Brick] stops a supercut
/// part way, and an undo of either goes over ticks too.
#[test]
fn new_duplicator_port_supercuts_and_fills_over_ticks() {
    use bri_sim::session::{Command, ToolAction};

    let (dir, mut s, host, seq, _) = new_duplicator_game("new-duplicator-box-jobs");
    // A tower on the stack, so the box holds more than a tick's work.
    for k in 0..8 {
        for _ in 0..10 {
            s.step().unwrap();
        }
        let position = [1.0, 0.5 + 0.2 * k as f32, 0.25];
        let plant = Command::Plant {
            definition: "plate".into(),
            position,
            quarter_turns: 0,
            color: 1,
        };
        send(&mut s, host, &seq, plant).unwrap();
    }
    swing(&mut s, host, &seq);
    send(&mut s, host, &seq, Command::ToggleLight).unwrap();
    for _ in 0..3 {
        s.step().unwrap();
    }
    told(&mut s);
    let before = s.snapshot().world.bricks.len();
    let finish = |s: &mut bri_sim::session::Session| {
        let mut ticks = 0;
        while s.copy_working(host) {
            s.step().unwrap();
            ticks += 1;
            assert!(ticks < 1000, "the job never finished");
        }
        // Its report comes at the start of the next tick.
        s.step().unwrap();
        ticks
    };
    // About a brick a tick.
    s.set_copy_work(32);
    for _ in 0..3 {
        s.step().unwrap();
    }
    send(&mut s, host, &seq, typed("sc", &[])).unwrap();
    s.step().unwrap();
    answer(&mut s, host, &seq, "ndconfirmsupercut");
    assert!(s.copy_working(host), "{:?} {}", told(&mut s), s.snapshot().world.bricks.len());
    // While it works the duplicator spins (`setImageLoaded(0, false)`).
    let held = |s: &bri_sim::session::Session| s.weapon_view().images[&host][0].state.clone();
    let mut spun = false;
    for _ in 0..4 {
        s.step().unwrap();
        spun |= held(&s) == "Spin";
    }
    assert!(spun && s.copy_working(host), "{}", held(&s));
    assert!(finish(&mut s) > 0);
    let mut ticks = 0;
    while held(&s) != "Ready" {
        s.step().unwrap();
        ticks += 1;
        assert!(ticks < 10, "it stops spinning when the job ends: {}", held(&s));
    }
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains("Supercut in progress... (")
            && t.contains("[Cancel Brick]: Cancel supercut")),
        "{prints:?}"
    );
    assert!(prints.iter().any(|t| t.contains(r"Deleted \c310\c6 Bricks!")), "{prints:?}");
    assert_eq!(s.snapshot().world.bricks.len(), before - 10);
    // Its undo (asked twice, over the stand-in's 2) puts them all back,
    // over ticks.
    let undo = |s: &mut bri_sim::session::Session| {
        send(s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
        send(s, host, &seq, Command::Tool(ToolAction::UndoBrick)).unwrap();
        assert!(s.copy_working(host));
    };
    undo(&mut s);
    finish(&mut s);
    assert_eq!(s.snapshot().world.bricks.len(), before);

    // Cancelled at once: what was cut stays cut, as one undo step.
    send(&mut s, host, &seq, typed("sc", &[])).unwrap();
    s.step().unwrap();
    answer(&mut s, host, &seq, "ndconfirmsupercut");
    assert!(s.copy_working(host));
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    assert!(!s.copy_working(host));
    assert!(told(&mut s).iter().any(|t| t.contains("Supercut canceled!")));
    let left = s.snapshot().world.bricks.len();
    assert!(left < before && left > before - 10, "{left} of {before}");
    undo(&mut s);
    finish(&mut s);
    assert_eq!(s.snapshot().world.bricks.len(), before);

    // /FillBricks: the supercut, then the fill, each over ticks.
    send(&mut s, host, &seq, typed("fb", &[])).unwrap();
    s.step().unwrap();
    answer(&mut s, host, &seq, "ndconfirmfillbricks");
    // The fill starts as the supercut's report comes.
    finish(&mut s);
    assert!(s.copy_working(host));
    finish(&mut s);
    let prints = told(&mut s);
    assert!(prints.iter().any(|t| t.contains("Filling in bricks... (")), "{prints:?}");
    // Ten plates fill the box the ten were cut from.
    assert!(prints.iter().any(|t| t.contains(r"Filled in \c310\c6 bricks")), "{prints:?}");
    assert_eq!(s.snapshot().world.bricks.len(), before);
    let diagnostics = s.package_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// What the host heard: its prints, and its sounds.
fn heard(s: &mut bri_sim::session::Session, host: u64) -> (Vec<String>, Vec<String>) {
    use bri_sim::session::Notice;
    let (mut prints, mut sounds) = (Vec::new(), Vec::new());
    for (owner, notice) in s.take_private_notices() {
        match notice {
            Notice::Center { text, .. } | Notice::Bottom { text, .. } | Notice::Chat(text)
                if owner == host =>
            {
                prints.push(text)
            }
            Notice::Sound(profile) if owner == host => sounds.push(profile),
            _ => {}
        }
    }
    (prints, sounds)
}

/// A click of the duplicator where the host looks, past the select wait,
/// ending the tick it lands: a job it starts is still running.
fn nd_click(s: &mut bri_sim::session::Session, host: u64, seq: &std::cell::Cell<u64>) {
    use bri_sim::session::Command;
    for _ in 0..60 {
        look(s, host);
    }
    send(s, host, seq, Command::WeaponTrigger { down: true }).unwrap();
    look(s, host);
    send(s, host, seq, Command::WeaponTrigger { down: false }).unwrap();
}

/// The New Duplicator's port: each job cancels its own way (a selection
/// says so and drops what it found, a cut goes back to selecting), the
/// menu sounds play at a job's start and end, a stack selection shows its
/// queue, and putting the duplicator away mid-job stops it with nothing
/// said. A selection glows until it is let go.
#[test]
fn new_duplicator_port_cancels_each_job_its_own_way_and_glows_until_let_go() {
    use bri_sim::session::{Command, Reply};

    let (dir, mut s, host, seq, base) = new_duplicator_game("new-duplicator-cancels");
    // A tower on the stack, more than a tick's work to select.
    for k in 0..12 {
        for _ in 0..10 {
            s.step().unwrap();
        }
        let plant = Command::Plant {
            definition: "plate".into(),
            position: [1.0, 0.5 + 0.2 * k as f32, 0.25],
            quarter_turns: 0,
            color: 1,
        };
        assert!(matches!(send(&mut s, host, &seq, plant), Ok(Reply::Planted(_))));
    }
    s.set_copy_work(32);
    for _ in 0..3 {
        s.step().unwrap();
    }
    heard(&mut s, host);

    // A stack selection, cancelled part way.
    nd_click(&mut s, host, &seq);
    // Its first progress report comes at the next tick's start.
    s.step().unwrap();
    assert!(s.copy_working(host), "the selection runs over ticks");
    let (prints, sounds) = heard(&mut s, host);
    assert!(sounds.iter().any(|x| x == "uploadStartSound"), "{sounds:?}");
    assert!(
        prints.iter().any(|t| t.contains("Selecting... (") && t.contains(r"\c6 in Queue)")
            && t.contains("[Cancel Brick]: Cancel selection")),
        "{prints:?}"
    );
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    assert!(!s.copy_working(host));
    let (prints, _) = heard(&mut s, host);
    assert!(prints.iter().any(|t| t.contains("Selection canceled!")), "{prints:?}");
    assert!(s.blueprint(host).is_none());

    // Selected whole: the end sound, and a glow that lasts.
    nd_click(&mut s, host, &seq);
    let mut ticks = 0;
    while s.copy_working(host) {
        s.step().unwrap();
        ticks += 1;
        assert!(ticks < 2000, "the selection never finished");
    }
    s.step().unwrap();
    let (prints, sounds) = heard(&mut s, host);
    assert!(sounds.iter().any(|x| x == "uploadEndSound"), "{sounds:?} {prints:?}");
    assert_eq!(s.blueprint(host).unwrap().bricks.len(), 14);
    // The bottom plate, by where it is (an undone cut puts it back as a
    // new brick).
    let at = s.snapshot().world.bricks[&base].position;
    let glow = |s: &bri_sim::session::Session| {
        let world = s.snapshot().world;
        let brick = world.bricks.values().find(|b| b.position == at).expect("the bottom plate");
        (brick.color, brick.color_effect)
    };
    assert_eq!(glow(&s), (1, 3));
    for _ in 0..(20 * 120) {
        s.step().unwrap();
    }
    assert_eq!(glow(&s), (1, 3), "lit until let go");

    // A cut, cancelled: back to selecting, the selection gone, the cut
    // part kept as one undo.
    let before = s.snapshot().world.bricks.len();
    send(&mut s, host, &seq, typed("cut", &[])).unwrap();
    assert!(s.copy_working(host));
    s.step().unwrap();
    send(&mut s, host, &seq, Command::CancelBrick).unwrap();
    s.step().unwrap();
    assert!(!s.copy_working(host));
    let (prints, _) = heard(&mut s, host);
    assert!(
        prints.iter().any(|t| t.contains("Selection Mode")),
        "back to selecting: {prints:?}"
    );
    assert!(s.blueprint(host).is_none());
    let left = s.snapshot().world.bricks.len();
    assert!(left < before, "{left} of {before}");
    send(&mut s, host, &seq, Command::Tool(bri_sim::session::ToolAction::UndoBrick)).unwrap();
    while s.copy_working(host) {
        s.step().unwrap();
    }
    s.step().unwrap();
    assert_eq!(s.snapshot().world.bricks.len(), before);
    assert_eq!(glow(&s), (1, 0), "a cancelled cut lets its selection go");

    // Put away mid-selection: stopped, nothing said, nothing lit.
    nd_click(&mut s, host, &seq);
    assert!(s.copy_working(host));
    heard(&mut s, host);
    send(&mut s, host, &seq, Command::EquipTool { slot: None }).unwrap();
    s.step().unwrap();
    assert!(!s.copy_working(host));
    let (prints, _) = heard(&mut s, host);
    assert!(!prints.iter().any(|t| t.contains("canceled")), "{prints:?}");
    for _ in 0..3 {
        s.step().unwrap();
    }
    assert_eq!(glow(&s), (1, 0));

    let diagnostics = s.package_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The New Duplicator's port: a box-mode click on a brick the player may
/// not select is refused with the original's message and error sound
/// (`ndTrustCheckMessage`), and makes no box.
#[test]
fn new_duplicator_port_refuses_a_box_corner_without_trust() {
    use bri_sim::session::Command;

    let (dir, mut s, _host, _seq, _) = new_duplicator_game("new-duplicator-box-trust");
    // Standing on the host's lone plate, looking straight down at it.
    let guest = s.join("Guest".into(), Vec3::new(2.5, 0.25, 0.25), false).unwrap();
    let gseq = std::cell::Cell::new(0u64);
    send(&mut s, guest, &gseq, typed("d", &[])).unwrap();
    let down = |s: &mut bri_sim::session::Session, n: u64| {
        s.movement(
            guest,
            s.snapshot().world.tick + n + 5000,
            bri_sim::player::MoveInput {
                pitch: -1.55,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
    };
    for n in 0..60 {
        down(&mut s, n);
    }
    send(&mut s, guest, &gseq, Command::ToggleLight).unwrap();
    for n in 0..60 {
        down(&mut s, n);
    }
    heard(&mut s, guest);
    for pressed in [true, false] {
        send(&mut s, guest, &gseq, Command::WeaponTrigger { down: pressed }).unwrap();
        down(&mut s, 0);
    }
    for n in 0..5 {
        down(&mut s, n);
    }
    let (prints, sounds) = heard(&mut s, guest);
    assert!(
        prints.iter().any(|t| t.contains("You don't have enough trust to do that!")),
        "{prints:?}"
    );
    assert!(sounds.iter().any(|x| x == "errorSound"), "{sounds:?}");
    assert!(nd_box(&s, guest).is_none(), "no box");
    let diagnostics = s.package_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The New Duplicator's port mirrors the player's own ghost brick outside
/// plant mode (`FxDtsBrick::ndMirrorGhost`), and says the original's line
/// with no ghost out.
#[test]
fn new_duplicator_port_mirrors_a_ghost_brick() {
    use bri_sim::session::{BrickHand, Command, GhostBrick, Notice};

    let (dir, mut s, host, seq, _) = new_duplicator_game("new-duplicator-ghost-mirror");
    let hand = |ghost| {
        Command::BrickHand(BrickHand {
            stocked: true,
            equipped: ghost,
            ghost,
        })
    };
    // Bricks in hand put the duplicator away; no ghost yet.
    send(&mut s, host, &seq, hand(false)).unwrap();
    s.step().unwrap();
    s.take_private_notices();
    send(&mut s, host, &seq, typed("mx", &[])).unwrap();
    s.step().unwrap();
    let prints = told(&mut s);
    assert!(
        prints.iter().any(|t| t.contains("The mirror command can only be used in plant mode or with a ghost brick.")),
        "{prints:?}"
    );
    send(&mut s, host, &seq, hand(true)).unwrap();
    let ghost = GhostBrick {
        definition: "plate".into(),
        position: [4.5, 0.1, 4.25],
        quarter_turns: 0,
        color: 0,
        print: None,
    };
    send(&mut s, host, &seq, Command::GhostBrick(Some(ghost))).unwrap();
    s.step().unwrap();
    s.take_private_notices();
    send(&mut s, host, &seq, typed("mx", &[])).unwrap();
    s.step().unwrap();
    let notices: Vec<Notice> = s
        .take_private_notices()
        .into_iter()
        .filter(|(o, _)| *o == host)
        .map(|(_, n)| n)
        .collect();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::MirrorGhost { definition, .. } if definition == "plate")),
        "{notices:?}"
    );
    let diagnostics = s.package_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    std::fs::remove_dir_all(dir).unwrap();
}
