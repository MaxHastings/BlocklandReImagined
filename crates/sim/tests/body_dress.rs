//! `set_body_dress`: an Add-On dresses a player's body in other textures and a
//! drawn size, which the host replicates with the player's avatar.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{definitions::Definitions, session::Session, simulation::Simulation};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::sync::Arc;

struct Root(std::path::PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn session(script: &str, name: &str) -> (Session, Root) {
    let root =
        Root(std::env::temp_dir().join(format!("bri-body-dress-{}-{name}", std::process::id())));
    let dir = root.0.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        json!({
            "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
            "name": "probe", "license": "CC0-1.0", "capabilities": ["player"],
            "provides": [
                { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
                { "kind": "script", "id": "probe:script/main", "file": "main.rhai" }
            ]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("behaviour.json"),
        json!({ "schema_version": 1, "script": "main.rhai", "on_spawn": true }).to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("main.rhai"), script).unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "probe".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "probe".into(),
            role: None,
        }],
    };
    let catalog = Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")));
    let ground = ColliderBuilder::cuboid(100.0, 0.5, 100.0)
        .translation(Vector::new(0.0, -0.5, 0.0))
        .user_data(u128::MAX);
    let mut s = Session::new(
        Simulation::new(
            World::new("Probe".into(), "probe".into(), vec![[1.0; 4]]),
            Definitions::default(),
            vec![ground],
        )
        .unwrap(),
    );
    let none = |f: &str| json!({"file": f, "sha256": "", "source": "", "width": 1, "height": 1});
    let colors: serde_json::Map<_, _> = [
        "head",
        "torso",
        "hat",
        "accent",
        "pack",
        "secondpack",
        "hip",
        "rarm",
        "larm",
        "rhand",
        "lhand",
        "rleg",
        "lleg",
    ]
    .iter()
    .map(|k| ((*k).to_owned(), json!([0.5, 0.5, 0.5, 1.0])))
    .collect();
    s.set_avatar_catalog(
        serde_json::from_value(json!({
            "schema_version": 1, "id": "test", "rig": "rig.json", "rig_sha256": "",
            "parts": {"hat": ["none"], "accent": ["none"], "pack": ["none"],
                "secondpack": ["none"], "chest": ["chest"], "hip": ["pants"],
                "rarm": ["rarm"], "larm": ["larm"], "rhand": ["rhand"],
                "lhand": ["lhand"], "rleg": ["rshoe"], "lleg": ["lshoe"]},
            "accents_allowed": {}, "faces": ["smiley"], "decals": ["AAA-None"],
            "surfaces": {}, "textures": {"smiley": none("s.png"), "AAA-None": none("n.png")},
            "defaults": {"parts": {}, "colors": colors, "face": "smiley", "decal": "AAA-None"}
        }))
        .unwrap(),
    )
    .unwrap();
    s.install_packages(catalog, None).unwrap();
    (s, root)
}

#[test]
fn a_spawn_hook_dresses_the_body_and_the_host_replicates_it() {
    let (mut s, _root) = session(
        "fn on_spawn(p) { set_body_dress(p, #{ skins: #{ face: \"face_3.png\", Shirt: \"shirt_1.png\" }, scale: 1.12 }); }",
        "dress",
    );
    let p = s
        .join("P".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..4 {
        s.step().unwrap();
    }
    let dress = s.avatars()[&p].dress.clone();
    assert_eq!(dress.skins["face"], "face_3.png");
    assert_eq!(dress.skins["Shirt"], "shirt_1.png");
    assert!((dress.scale - 1.12).abs() < 1e-6, "{}", dress.scale);
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
}

#[test]
fn a_dress_outside_its_limits_is_refused_and_a_body_is_undressed_by_unit() {
    let (mut s, _root) = session(
        "fn on_spawn(p) { set_body_dress(p, #{ scale: 9.0 }); }",
        "big",
    );
    let p = s
        .join("P".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..4 {
        s.step().unwrap();
    }
    assert!(s.avatars()[&p].dress.is_empty(), "too big: nothing applied");
    assert!(
        !s.package_diagnostics().is_empty(),
        "the refusal is reported"
    );
    let (mut s, _root) = session(
        "fn on_spawn(p) { set_body_dress(p, #{ scale: 1.2 }); set_body_dress(p, ()); }",
        "unit",
    );
    let p = s
        .join("P".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..4 {
        s.step().unwrap();
    }
    assert!(s.avatars()[&p].dress.is_empty());
}
