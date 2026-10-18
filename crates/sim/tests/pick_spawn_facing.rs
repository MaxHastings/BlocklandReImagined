//! `on_pick_spawn` may return `[x, y, z, yaw]`: the player appears there
//! facing that way (radians, the motor's yaw), and `[x, y, z]` still faces
//! down -Z as before.
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
        Root(std::env::temp_dir().join(format!("bri-pick-spawn-{}-{name}", std::process::id())));
    let dir = root.0.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        json!({
            "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
            "name": "probe", "license": "CC0-1.0", "capabilities": [],
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
        json!({ "schema_version": 1, "script": "main.rhai", "on_pick_spawn": true }).to_string(),
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
    s.install_packages(catalog, None).unwrap();
    (s, root)
}

fn motion(s: &Session, owner: u64) -> bri_sim::player::PlayerState {
    s.motion_states()
        .into_iter()
        .find(|(p, _)| p.owner == owner)
        .unwrap()
        .0
}

#[test]
fn a_fourth_number_is_the_way_the_player_faces_when_they_spawn() {
    let (mut s, _root) = session("fn on_pick_spawn(p) { [10.0, 0.05, -20.0, 1.25] }", "four");
    let p = s
        .join("P".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let state = motion(&s, p);
    assert!(
        (Vec3::from(state.feet) - Vec3::new(10.0, 0.05, -20.0)).length() < 0.01,
        "{:?}",
        state.feet
    );
    assert!((state.yaw - 1.25).abs() < 1e-4, "yaw {}", state.yaw);
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
}

#[test]
fn three_numbers_still_face_the_default_way() {
    let (mut s, _root) = session("fn on_pick_spawn(p) { [10.0, 0.05, -20.0] }", "three");
    let p = s
        .join("P".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let state = motion(&s, p);
    assert!(
        (Vec3::from(state.feet) - Vec3::new(10.0, 0.05, -20.0)).length() < 0.01,
        "{:?}",
        state.feet
    );
    assert!(state.yaw.abs() < 1e-4, "yaw {}", state.yaw);
}
