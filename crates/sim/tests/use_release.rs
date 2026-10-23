//! Ordinary rule-driven Use press/release on a loose objective item.
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
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Game {
    s: Session,
    owner: u64,
    sequence: u64,
    _root: Root,
}
impl Game {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = Root(std::env::temp_dir().join(format!(
            "bri-use-release-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        let mut entries = Vec::new();
        for id in ["first", "second"] {
            let folder = root.0.join(id);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("package.json"),json!({
                "schema_version":1,"id":id,"version":"1.0.0","api":1,"name":id,"license":"CC0-1.0","capabilities":["player"],
                "provides":[{"kind":"behaviour","id":format!("{id}:behaviour/main"),"file":"behaviour.json"},{"kind":"script","id":format!("{id}:script/main"),"file":"main.rhai"}]
            }).to_string()).unwrap();
            std::fs::write(folder.join("behaviour.json"),json!({
                "schema_version":1,"script":"main.rhai","on_use_object":id=="first","on_use_release":true,
                "commands": [{"name":"prepare","args":[]},{"name":"remove","args":[]}],
                "state":{"global":{"target":{"default":0,"visible":"everyone"},"released":{"default":[],"visible":"everyone"}}}
            }).to_string()).unwrap();
            std::fs::write(folder.join("main.rhai"),r#"
                fn cmd_prepare(p) { drop_item("first:weapon/case",0.0,2.0,-2.0); }
                fn cmd_remove(p) { remove_drop(get("target")); }
                fn on_use_object(p,target) { set("target",target.id); true }
                fn on_use_release(p,target) { let log=get("released"); log.push(target.ref); set("released",log); true }
            "#).unwrap();
            entries.push(PackageEntry {
                id: id.into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: id.into(),
                role: None,
            });
        }
        let catalog = Arc::new(
            Catalog::load(
                &root.0,
                &PackageSet {
                    schema_version: 1,
                    packages: entries,
                },
                true,
            )
            .unwrap(),
        );
        let mut s = Session::new(
            Simulation::new(
                World::new("Use".into(), "use".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![
                    ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                ],
            )
            .unwrap(),
        );
        let pack=bri_weapons::Pack::from_json(&serde_json::to_vec(&json!({"schema_version":5,"id":"first","items":{"first:weapon/case":{"ui_name":"Case","image":"first:image/case"}},"images":{"first:image/case":{"states":[{"name":"Ready"}]}},"projectiles":{},"resources":[]})).unwrap()).unwrap();
        s.set_weapon_pack(pack).unwrap();
        s.set_item_bounds(
            [(
                "first:weapon/case".into(),
                bri_weapons::ItemBounds {
                    min: [-0.3, -1.0, -0.3],
                    max: [0.3, 1.0, 0.3],
                },
            )]
            .into(),
        )
        .unwrap();
        s.install_packages(catalog, None).unwrap();
        let owner = s.join("P".into(), Vec3::new(0.0, 0.05, 0.0), true).unwrap();
        let mut game = Self {
            s,
            owner,
            sequence: 0,
            _root: root,
        };
        game.send(Command::Package(PackageCommand {
            package: "first".into(),
            command: "prepare".into(),
            args: vec![],
        }));
        game
    }
    fn send(&mut self, command: Command) {
        self.sequence += 1;
        self.s.command(self.owner, self.sequence, command).unwrap();
    }
    fn released(&self, id: &str) -> serde_json::Value {
        self.s.package_state().packages[id].global["released"].clone()
    }
    fn target(&self) -> i64 {
        self.s.package_state().packages["first"].global["target"]
            .as_i64()
            .unwrap()
    }
}
#[test]
fn release_reaches_all_declaring_packages_once_with_the_original_removed_target() {
    let mut g = Game::new();
    g.send(Command::ActivateRelease);
    assert_eq!(g.released("first"), json!([]));
    g.send(Command::Activate);
    let target = g.target();
    assert!(target > 0, "ordinary Use must select the loose item");
    g.send(Command::Package(PackageCommand {
        package: "first".into(),
        command: "remove".into(),
        args: vec![],
    }));
    g.send(Command::ActivateRelease);
    g.send(Command::ActivateRelease);
    for id in ["first", "second"] {
        assert_eq!(
            g.released(id),
            json!([format!("drop:{target}")]),
            "{id} must receive the same target exactly once"
        );
    }
    assert!(
        g.s.package_diagnostics().is_empty(),
        "{:?}",
        g.s.package_diagnostics()
    );
}
#[test]
fn releasing_after_death_still_finishes_the_taken_use() {
    let mut g = Game::new();
    g.send(Command::Activate);
    let target = g.target();
    assert!(target > 0);
    g.send(Command::Suicide);
    g.send(Command::ActivateRelease);
    for id in ["first", "second"] {
        assert_eq!(g.released(id), json!([format!("drop:{target}")]));
    }
}
