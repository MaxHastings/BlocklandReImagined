//! `on_volume` (docs/modding/rules.md, "Map volumes"): the rules hear a
//! player's body centre go into and out of a map volume, once each.
use bri_content::scene::Volume;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    session::{Command, PackageArg, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::{Mat4, Vec3};
use rapier3d::prelude::*;
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const RULES: &str = r#"
fn on_volume(e) {
    let log = get("log");
    log.push(`${e.kind} ${e.player} ${e.id} ${e.properties.place}`);
    set("log", log);
}
fn cmd_go(p, x) {
    teleport(p, x, 0.2, 0.0);
    let r = map_region_at(x, 0.2, 0.0);
    set("region", if r == () { "missing" } else { `${r.number}:${r.properties.kind}` });
}
"#;

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn catalog() -> Arc<Catalog> {
    let root = Root(std::env::temp_dir().join(format!("bri-volume-hooks-{}", std::process::id())));
    std::fs::create_dir_all(root.0.join("zone")).unwrap();
    let write =
        |file: &str, value: String| std::fs::write(root.0.join("zone").join(file), value).unwrap();
    write(
        "package.json",
        json!({
            "schema_version": 1, "id": "zone", "version": "1.0.0", "api": 1,
            "name": "zone", "license": "CC0-1.0", "capabilities": ["player"],
            "provides": [
                { "kind": "behaviour", "id": "zone:behaviour/main", "file": "behaviour.json" },
                { "kind": "script", "id": "zone:script/main", "file": "main.rhai" }
            ]
        })
        .to_string(),
    );
    write(
        "behaviour.json",
        json!({
            "schema_version": 1, "script": "main.rhai", "on_volume": true,
            "commands": [{ "name": "go", "args": ["float"] }],
            "state": { "global": {
                "log": { "default": [], "visible": "everyone" },
                "region": { "default": "", "visible": "everyone" }
            } }
        })
        .to_string(),
    );
    write("main.rhai", RULES.into());
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "zone".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "zone".into(),
            role: None,
        }],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

fn volume(id: &str, centre: [f32; 3]) -> Volume {
    Volume {
        node: 0,
        id: id.into(),
        priority: 0,
        center: centre,
        half: [2.0, 5.0, 5.0],
        to_local: Mat4::from_translation(-Vec3::from(centre)).to_cols_array(),
        properties: BTreeMap::from([("place".to_owned(), "roof".to_owned())]),
        world: (
            [centre[0] - 2.0, centre[1] - 5.0, centre[2] - 5.0],
            [centre[0] + 2.0, centre[1] + 5.0, centre[2] + 5.0],
        ),
        aabb: false,
    }
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
                World::new("Zones".into(), "zones".into(), vec![[1.0; 4]]),
                Definitions::default(),
                vec![
                    ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                        .translation(Vector::new(0.0, -0.5, 0.0))
                        .user_data(u128::MAX),
                ],
            )
            .unwrap(),
        );
        s.install_packages(catalog(), None).unwrap();
        s.set_volumes(vec![volume("pad", [20.0, 0.0, 0.0])])
            .unwrap();
        let p = s.join("P".into(), Vec3::new(0.0, 0.05, 0.0), true).unwrap();
        Self { s, p, seq: 0 }
    }
    fn go(&mut self, x: f64) {
        self.seq += 1;
        self.s
            .command(
                self.p,
                self.seq,
                Command::Package(PackageCommand {
                    package: "zone".into(),
                    command: "go".into(),
                    args: vec![PackageArg::Float(x)],
                }),
            )
            .unwrap();
        for _ in 0..3 {
            self.s.step().unwrap();
        }
    }
    fn log(&self) -> Vec<String> {
        self.s.package_state().packages["zone"].global["log"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    }
}

#[test]
fn entering_and_leaving_a_volume_is_heard_once_each() {
    let mut g = Game::new();
    g.go(0.0);
    assert!(g.log().is_empty(), "{:?}", g.log());
    g.go(20.5);
    let want = format!("enter {} pad roof", g.p);
    assert_eq!(g.log(), vec![want.clone()]);
    // Standing still tells nothing more.
    g.go(21.0);
    assert_eq!(g.log().len(), 1, "{:?}", g.log());
    g.go(0.0);
    assert_eq!(g.log(), vec![want, format!("leave {} pad roof", g.p)]);
}

#[test]
fn package_region_query_tracks_both_sides_boundary_and_map_replacement() {
    use bri_content::regions::{Region, RegionMap};
    let mut game = Game::new();
    game.go(1.0);
    let observed = |game: &Game| game.s.package_state().packages["zone"].global["region"].clone();
    assert_eq!(observed(&game), "missing");
    game.s
        .set_map_regions(Some(RegionMap {
            planes: vec![[1.0, 0.0, 0.0, 0.0]],
            front: vec![-1],
            back: vec![-1],
            front_region: vec![1],
            back_region: vec![0],
            regions: ["west", "east"]
                .into_iter()
                .map(|kind| Region {
                    properties: BTreeMap::from([("kind".into(), kind.into())]),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }))
        .unwrap();
    for (x, expected) in [
        (1.0, "1:east"),
        (-1.0, "0:west"),
        (0.0, "1:east"),
        (-5.0, "0:west"),
    ] {
        game.go(x);
        assert_eq!(observed(&game), expected);
    }
    assert!(
        game.s
            .simulation()
            .map_region_at([f32::NAN, 0.0, 0.0])
            .is_none()
    );
    game.s.set_map_regions(None).unwrap();
    game.go(1.0);
    assert_eq!(observed(&game), "missing");
}
