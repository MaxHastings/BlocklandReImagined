//! `on_contact` hears two vehicles crash (`kind: "vehicles"`): which ran
//! into which, their drivers and how fast, once as the crash begins. A
//! bumper-car or demolition-derby Add-On scores it however it likes.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{definitions::Definitions, session::Session, simulation::Simulation};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

const BUGGY: &str = "playground-buggy:vehicle/buggy";
const DERBY: &str = r#"
fn on_contact(info) {
    if info.kind != "vehicles" { return; }
    let log = get("crashes");
    log.push(`${info.vehicle} hit ${info.target_vehicle} at ${info.speed.round()}`);
    set("crashes", log);
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
        "bri-vehicle-crashes-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("derby");
    std::fs::create_dir_all(&dir).unwrap();
    let write = |file: &str, value: String| std::fs::write(dir.join(file), value).unwrap();
    write(
        "package.json",
        json!({
            "schema_version": 1, "id": "derby", "version": "1.0.0", "api": 1,
            "name": "derby", "license": "CC0-1.0",
            "provides": [
                { "kind": "behaviour", "id": "derby:behaviour/main", "file": "behaviour.json" },
                { "kind": "script", "id": "derby:script/main", "file": "main.rhai" }
            ]
        })
        .to_string(),
    );
    write(
        "behaviour.json",
        json!({
            "schema_version": 1, "script": "main.rhai", "on_contact": true,
            "state": { "global": { "crashes": { "default": [], "visible": "everyone" } } }
        })
        .to_string(),
    );
    write("main.rhai", DERBY.into());
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "derby".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "derby".into(),
            role: None,
        }],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

fn crashes(s: &Session) -> Vec<String> {
    s.package_state().packages["derby"].global["crashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn two_vehicles_crashing_are_heard_once_as_the_crash_begins() {
    let mut s = Session::new(
        Simulation::new(
            World::new("Derby".into(), "derby".into(), vec![[1.0; 4]]),
            Definitions::default(),
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    let pack = bri_vehicles::Pack::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/playground/playground-buggy/assets/vehicles.json"),
    )
    .unwrap();
    s.set_vehicle_pack(pack, Vec::new()).unwrap();
    s.install_packages(catalog(), None).unwrap();
    let p = s
        .join("P".into(), Vec3::new(30.0, 0.05, 30.0), true)
        .unwrap();
    for _ in 0..10 {
        s.step().unwrap();
    }
    let still = s
        .spawn_vehicle_at(p, BUGGY, Vec3::new(0.0, 1.0, 0.0), 0.0, Vec3::ZERO)
        .unwrap();
    let rammer = s
        .spawn_vehicle_at(
            p,
            BUGGY,
            Vec3::new(0.0, 1.0, -6.0),
            0.0,
            Vec3::new(0.0, 0.0, 20.0),
        )
        .unwrap();
    for _ in 0..240 {
        s.step().unwrap();
    }
    let log = crashes(&s);
    assert!(
        !log.is_empty(),
        "no crash heard: {:?} {:?}",
        s.take_package_problems(),
        s.vehicle_poses()
    );
    assert!(
        log[0].starts_with(&format!("{} hit {} at ", rammer, still)),
        "{log:?}"
    );
    // Pushed along nose to tail, the two may part and meet again, but
    // never are they told twice for one touch.
    assert!(log.len() <= 3, "{log:?}");
}
