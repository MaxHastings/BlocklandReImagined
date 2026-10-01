//! An Add-On's own wrench event inputs (`brick_inputs`, v20's
//! `registerInputEvent`): they join the host's catalog whichever is set up
//! first, builders' rows on them run when the rules fire them, and a
//! package fires only its own.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, PackageArg, PackageCommand, Reply, Session},
    simulation::Simulation,
};
use bri_world::{EventRow, EventTarget, EventValue, OwnerId, World, authority::Edit};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

const SCRIPT: &str = r#"
fn cmd_ping(p, brick) { fire_brick_input(brick, "onPing", p); }
fn cmd_steal(p, brick) { fire_brick_input(brick, "onActivate", p); }
"#;

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn add_ons(name: &str, inputs: serde_json::Value) -> (Root, Arc<Catalog>) {
    let root = Root(std::env::temp_dir().join(format!(
        "bri-brick-inputs-{}-{name}",
        std::process::id()
    )));
    let dir = root.0.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
        "name": "probe", "license": "CC0-1.0",
        "capabilities": ["brick_events"],
        "provides": [
            { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "probe:script/main", "file": "main.rhai" }
        ]
    });
    let behaviour = json!({
        "schema_version": 1,
        "script": "main.rhai",
        "brick_inputs": inputs,
        "commands": [
            { "name": "ping", "args": ["int"] },
            { "name": "steal", "args": ["int"] }
        ]
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(dir.join("main.rhai"), SCRIPT).unwrap();
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
    let catalog = Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));
    (root, Arc::new(catalog))
}

fn session() -> Session {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 2],
        height_plates: 3,
        attachment_rows: vec!["bb".into(); 6],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.6, 1.0],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let definitions = Definitions {
        entries: [(
            "plate".into(),
            Definition {
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
    let mut s = Session::new(
        Simulation::new(
            World::new("Inputs".into(), "inputs".into(), vec![[1.0; 4], [0.0; 4]]),
            definitions,
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}

fn steps(s: &mut Session, players: &[OwnerId], n: u64) {
    for _ in 0..n {
        for &p in players {
            let seq = 1_000_000 + s.simulation().state().tick;
            s.movement(p, seq, MoveInput::default()).unwrap();
        }
        s.step().unwrap();
    }
}

fn run(s: &mut Session, owner: OwnerId, seq: u64, command: &str, brick: u64) -> anyhow::Result<()> {
    s.command(
        owner,
        seq,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: command.into(),
            args: vec![PackageArg::Int(brick as i64)],
        }),
    )
    .map(drop)
}

#[test]
fn rows_on_an_add_ons_input_run_when_its_rules_fire_it() {
    let ping = json!([{ "name": "onPing", "targets": ["Player", "Client", "MiniGame"] }]);
    // Packages before the event catalog and after it come to the same list.
    for packages_first in [true, false] {
        let (_root, add_ons) = add_ons(&format!("fire-{packages_first}"), ping.clone());
        let mut s = session();
        if packages_first {
            s.install_packages(add_ons.clone(), None).unwrap();
        }
        s.set_event_catalog(bri_events::testing::catalog(), Vec::new())
            .unwrap();
        if !packages_first {
            s.install_packages(add_ons.clone(), None).unwrap();
        }
        let input = s.event_catalog().unwrap().input("onPing").expect("listed");
        assert_eq!(input.id, "probe:onPing");
        assert_eq!(
            input.targets.iter().map(|(slot, _)| slot.as_str()).collect::<Vec<_>>(),
            ["Self", "Player", "Client", "MiniGame"]
        );
        assert_eq!(s.package_brick_inputs().len(), 1, "sent to players");

        let builder = s.join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), false).unwrap();
        steps(&mut s, &[builder], 10);
        let Reply::Planted(brick) = s
            .command(
                builder,
                1,
                Command::Plant {
                    definition: "plate".into(),
                    position: [5.0, 0.3, -3.0],
                    quarter_turns: 0,
                    color: 0,
                },
            )
            .unwrap()
        else {
            panic!("expected a plant")
        };
        let row = EventRow {
            preserved: None,
            enabled: true,
            input: "onPing".into(),
            delay_ms: 0,
            target: EventTarget::Slot(bri_events::Slot::SelfBrick),
            output: "setColor".into(),
            params: vec![EventValue::Color(1)],
        };
        s.edit_brick(builder, brick, Edit::Events(vec![row])).unwrap();
        run(&mut s, builder, 2, "ping", brick).unwrap();
        steps(&mut s, &[builder], 2);
        assert_eq!(s.simulation().state().bricks[&brick].color, 1, "the row ran");
        assert!(
            s.package_diagnostics().is_empty(),
            "{:?}",
            s.package_diagnostics()
        );

        // The engine's own inputs are not the package's to fire.
        run(&mut s, builder, 3, "steal", brick).unwrap();
        steps(&mut s, &[builder], 2);
        assert!(
            s.package_diagnostics()
                .iter()
                .any(|d| d.message.contains("not one of `probe`'s brick_inputs")),
            "{:?}",
            s.package_diagnostics()
        );
    }
}

#[test]
fn an_input_the_engine_already_has_is_refused() {
    let (_root, add_ons) = add_ons(
        "taken",
        json!([{ "name": "onActivate", "targets": ["Player"] }]),
    );
    let mut s = session();
    s.set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    let error = s.install_packages(add_ons, None).unwrap_err();
    assert!(format!("{error:#}").contains("already taken"), "{error:#}");
    // The host keeps its own catalog.
    assert!(s.event_catalog().unwrap().input("onActivate").is_some());
}
