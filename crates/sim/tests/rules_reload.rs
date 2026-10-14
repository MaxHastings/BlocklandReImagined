//! `/reloadrules` (docs/modding/README.md, "Edit while you play"): the host
//! swaps in an Add-On's edited rules script without hosting again. State
//! carries on, and a script with a mistake keeps the old one running.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    session::{Command, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::sync::Arc;

#[test]
fn edited_rules_reload_in_place_and_keep_their_state() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("counter");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        json!({
            "schema_version": 1, "id": "counter", "version": "1.0.0", "api": 1,
            "name": "Counter", "license": "CC0-1.0",
            "provides": [
                { "kind": "behaviour", "id": "counter:behaviour/main", "file": "behaviour.json" },
                { "kind": "script", "id": "counter:script/main", "file": "main.rhai" }
            ]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("behaviour.json"),
        json!({
            "schema_version": 1, "script": "main.rhai",
            "commands": [{ "name": "go" }],
            "state": { "global": { "count": { "default": 0, "visible": "everyone" } } }
        })
        .to_string(),
    )
    .unwrap();
    let script = dir.join("main.rhai");
    std::fs::write(
        &script,
        r#"fn cmd_go(p) { set("count", get("count") + 1); }"#,
    )
    .unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "counter".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "counter".into(),
            role: None,
        }],
    };
    let catalog = Catalog::load(root.path(), &set, true).unwrap_or_else(|e| panic!("{e:#?}"));

    let mut s = Session::new(
        Simulation::new(
            World::new("Reload".into(), "reload".into(), vec![[1.0; 4]]),
            Definitions::default(),
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                    .translation(Vector::new(0.0, -0.5, 0.0))
                    .user_data(u128::MAX),
            ],
        )
        .unwrap(),
    );
    s.install_packages(Arc::new(catalog), None).unwrap();
    let host = s
        .join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let mut seq = 0;
    let mut send = |s: &mut Session, package: &str, command: &str| {
        seq += 1;
        s.command(
            host,
            seq,
            Command::Package(PackageCommand {
                package: package.into(),
                command: command.into(),
                args: vec![],
            }),
        )
        .unwrap();
    };
    let count = |s: &Session| s.package_state().packages["counter"].global["count"].clone();

    send(&mut s, "counter", "go");
    assert_eq!(count(&s), json!(1));

    // A mistake is reported and the old rules keep running.
    std::fs::write(&script, r#"fn cmd_go(p) { set("count", "#).unwrap();
    assert!(s.reload_rules().is_err());
    send(&mut s, "counter", "go");
    assert_eq!(count(&s), json!(2));

    // Fixed, the host reloads by typing /reloadrules; the count carries on.
    std::fs::write(
        &script,
        r#"fn cmd_go(p) { set("count", get("count") + 10); }"#,
    )
    .unwrap();
    send(&mut s, "", "reloadrules");
    send(&mut s, "counter", "go");
    assert_eq!(count(&s), json!(12));
    assert_eq!(s.reload_rules(), Ok(Vec::new()), "nothing changed since");
}
