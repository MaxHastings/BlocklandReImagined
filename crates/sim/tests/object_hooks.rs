//! `on_object` (docs/modding/rules.md, "Every object in the world"): an
//! Add-On hears every object come and go, whoever made it. Here `watcher`
//! hears the creatures and dropped items another Add-On, `kit`, makes and
//! removes, without either knowing the other.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    session::{Command, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

const KIT: &str = r#"
fn think(me) { }
fn cmd_cow(p) { spawn_entity("kit:entity/cow", 10.0, 0.0, 10.0); }
fn cmd_cull(p) { for e in entities() { remove_entity(e.id); } }
fn cmd_drop(p) { drop_item("kit:weapon/mine", 4.0, 0.2, 4.0); }
"#;

const WATCHER: &str = r#"
fn on_object(e) {
    let log = get("log");
    log.push(`${e.kind} ${e.object} ${e.definition} ${e.addon}`);
    set("log", log);
}
"#;

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn catalog() -> Arc<Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Root(std::env::temp_dir().join(format!(
        "bri-object-hooks-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let write = |dir: &str, file: &str, value: String| {
        std::fs::create_dir_all(root.0.join(dir)).unwrap();
        std::fs::write(root.0.join(dir).join(file), value).unwrap();
    };
    let provides = |id: &str, extra: Value| {
        let mut list = vec![
            json!({ "kind": "behaviour", "id": format!("{id}:behaviour/main"), "file": "behaviour.json" }),
            json!({ "kind": "script", "id": format!("{id}:script/main"), "file": "main.rhai" }),
        ];
        if let Value::Array(more) = extra {
            list.extend(more);
        }
        list
    };
    write(
        "kit",
        "package.json",
        json!({
            "schema_version": 1, "id": "kit", "version": "1.0.0", "api": 1,
            "name": "kit", "license": "CC0-1.0", "capabilities": ["entity", "player"],
            "dependencies": { "kitmodel": "^1.0.0" },
            "provides": provides("kit", json!([
                { "kind": "entity", "id": "kit:entity/cow", "file": "cow.json" }
            ]))
        })
        .to_string(),
    );
    write(
        "kit",
        "behaviour.json",
        json!({
            "schema_version": 1, "script": "main.rhai",
            "commands": [{ "name": "cow" }, { "name": "cull" }, { "name": "drop" }]
        })
        .to_string(),
    );
    write("kit", "main.rhai", KIT.into());
    write(
        "kit",
        "cow.json",
        json!({
            "schema_version": 1, "name": "cow", "model": "kitmodel:model/body",
            "think": "think", "think_interval": 30, "scale": 0.5, "health": 10.0, "max_alive": 4
        })
        .to_string(),
    );
    write(
        "kitmodel",
        "package.json",
        json!({
            "schema_version": 1, "id": "kitmodel", "version": "1.0.0", "api": 1,
            "name": "kitmodel", "license": "CC0-1.0",
            "provides": [{ "kind": "model", "id": "kitmodel:model/body", "file": "body.json" }]
        })
        .to_string(),
    );
    write(
        "kitmodel",
        "body.json",
        json!({ "schema_version": 1, "boxes": [
            { "center": [0.0, 0.5, 0.0], "size": [0.5, 1.0, 0.5], "color": [0.2, 0.8, 0.2, 1.0] }
        ] })
        .to_string(),
    );
    write(
        "watcher",
        "package.json",
        json!({
            "schema_version": 1, "id": "watcher", "version": "1.0.0", "api": 1,
            "name": "watcher", "license": "CC0-1.0",
            "provides": provides("watcher", json!([]))
        })
        .to_string(),
    );
    write(
        "watcher",
        "behaviour.json",
        json!({
            "schema_version": 1, "script": "main.rhai", "on_object": true,
            "state": { "global": { "log": { "default": [], "visible": "everyone" } } }
        })
        .to_string(),
    );
    write("watcher", "main.rhai", WATCHER.into());
    let entry = |id: &str| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side: if id == "kitmodel" { Side::Client } else { Side::Server },
        dir: id.into(),
        role: None,
    };
    let set = PackageSet {
        schema_version: 1,
        packages: vec![entry("kit"), entry("kitmodel"), entry("watcher")],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

fn weapons() -> bri_weapons::Pack {
    let pack = json!({
        "schema_version": 5,
        "id": "kit",
        "items": { "kit:weapon/mine": { "ui_name": "Kit Mine" } },
        "images": {},
        "projectiles": {}
    });
    bri_weapons::Pack::from_json(&serde_json::to_vec(&pack).unwrap()).unwrap()
}

struct Game {
    s: Session,
    p: OwnerId,
    seq: u64,
}
impl Game {
    fn new() -> Self {
        let mut s = Session::new(
            Simulation::new(
                World::new("Objects".into(), "objects".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![
                    ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                        .translation(Vector::new(0.0, -0.5, 0.0))
                        .user_data(u128::MAX),
                ],
            )
            .unwrap(),
        );
        s.set_weapon_pack(weapons()).unwrap();
        s.set_item_bounds(BTreeMap::new()).unwrap();
        s.install_packages(catalog(), None).unwrap();
        let p = s.join("P".into(), Vec3::new(0.0, 0.05, 0.0), true).unwrap();
        Self { s, p, seq: 0 }
    }
    fn kit(&mut self, command: &str) {
        self.seq += 1;
        self.s
            .command(
                self.p,
                self.seq,
                Command::Package(PackageCommand {
                    package: "kit".into(),
                    command: command.into(),
                    args: Vec::<PackageArg>::new(),
                }),
            )
            .unwrap();
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
    }
    fn log(&self) -> Vec<String> {
        self.s.package_state().packages["watcher"].global["log"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    }
}

#[test]
fn another_add_ons_creatures_and_drops_are_heard_coming_and_going() {
    let mut g = Game::new();
    g.steps(2);
    assert!(g.log().is_empty(), "{:?}", g.log());
    g.kit("cow");
    g.kit("drop");
    g.steps(2);
    let log = g.log();
    assert!(
        log.contains(&"created entity kit:entity/cow kit".to_owned()),
        "{log:?}"
    );
    assert!(
        log.contains(&"created drop kit:weapon/mine kit".to_owned()),
        "{log:?}"
    );
    g.kit("cull");
    g.steps(2);
    let log = g.log();
    assert!(
        log.contains(&"removed entity kit:entity/cow kit".to_owned()),
        "{log:?}"
    );
    // Each change is told once.
    assert_eq!(log.len(), 3, "{log:?}");
}
