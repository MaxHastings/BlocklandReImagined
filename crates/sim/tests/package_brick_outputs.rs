//! An Add-On's own wrench event outputs (`brick_outputs`, v20's
//! `registerOutputEvent`): builders' rows run them through the rules'
//! `on_brick_output`, which can answer with one of its own inputs to run
//! next on the brick (Slayer's `checkTeam`), limited to a range of rows.
//! Its own targets (`brick_targets`, v20's `registerEventTarget`, Slayer's
//! `Team(Client)`) join the inputs with their base slot and run its outputs
//! on the base entity.
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
fn on_brick_output(output, target, params, info) {
    if output == "greet" {
        let log = get("log");
        log.push(`${output}:${target}:${params[0]}:${info.class}:${info.target}:${info.base}`);
        set("log", log);
        return;
    }
    if output == "relay" {
        fire_brick_input(target, "onNo", info.client);
        return;
    }
    let log = get("log");
    log.push(`${output}:${target}:${params[0]}:${info.class}:${info.client}:${info.owner}:${info.row}`);
    set("log", log);
    if output == "check" {
        let input = if params[0] { "onYes" } else { "onNo" };
        return #{ input: input, rows: [1, 3] };
    }
    ()
}
"#;

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn add_ons(name: &str, capabilities: serde_json::Value) -> (Root, Arc<Catalog>) {
    let root =
        Root(std::env::temp_dir().join(format!("bri-brick-outputs-{}-{name}", std::process::id())));
    let dir = root.0.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
        "name": "probe", "license": "CC0-1.0",
        "capabilities": capabilities,
        "provides": [
            { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "probe:script/main", "file": "main.rhai" }
        ]
    });
    let behaviour = json!({
        "schema_version": 1,
        "script": "main.rhai",
        "brick_inputs": [
            { "name": "onPing", "targets": ["Player", "Client"] },
            { "name": "onYes", "targets": ["Client", "OwnerClient"] },
            { "name": "onNo", "targets": ["Client"] }
        ],
        "brick_outputs": [
            { "name": "tally", "class": "GameConnection",
              "params": [{ "type": "int", "min": 0, "max": 9, "default": 1 }] },
            { "name": "check", "class": "fxDTSBrick", "params": [{ "type": "bool" }] },
            { "name": "relay", "class": "fxDTSBrick" },
            { "name": "greet", "class": "Probe_Pal",
              "params": [{ "type": "int", "min": 0, "max": 9, "default": 1 }] }
        ],
        "brick_targets": [
            { "name": "Pal(Client)", "class": "Probe_Pal", "from": "Client" },
            { "name": "Pal(Brick)", "class": "Probe_Pal", "from": "Self" }
        ],
        "commands": [{ "name": "ping", "args": ["int"] }],
        "state": { "global": { "log": { "default": [], "visible": "everyone" } } }
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
                bot: None,
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

fn row(input: &str, slot: bri_events::Slot, output: &str, params: Vec<EventValue>) -> EventRow {
    EventRow {
        preserved: None,
        enabled: true,
        input: input.into(),
        delay_ms: 0,
        target: EventTarget::Slot(slot),
        output: output.into(),
        params,
    }
}

#[test]
fn an_add_ons_outputs_run_its_rules_and_chain_its_own_inputs() {
    use bri_events::Slot;
    let (_root, add_ons) = add_ons("chain", json!(["brick_events"]));
    let mut s = session();
    s.set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    s.install_packages(add_ons, None).unwrap();
    let catalog = s.event_catalog().unwrap();
    let check = catalog
        .output(bri_events::Class::Brick, "check")
        .expect("listed");
    assert_eq!(check.package.as_deref(), Some("probe"));
    assert!(catalog.output(bri_events::Class::Client, "tally").is_some());
    assert_eq!(s.package_brick_outputs().len(), 4, "sent to players");
    let yes = catalog.input("onYes").unwrap();
    assert!(yes.targets.iter().any(|(slot, _)| slot == "OwnerClient"));

    let builder = s
        .join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), false)
        .unwrap();
    let visitor = s
        .join("Visitor".into(), Vec3::new(-5.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, &[builder, visitor], 10);
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
    let rows = vec![
        row(
            "onPing",
            Slot::SelfBrick,
            "check",
            vec![EventValue::Bool(true)],
        ),
        row(
            "onYes",
            Slot::OwnerClient,
            "tally",
            vec![EventValue::Int(2)],
        ),
        row("onYes", Slot::Client, "tally", vec![EventValue::Int(3)]),
        row(
            "onYes",
            Slot::SelfBrick,
            "setColor",
            vec![EventValue::Color(1)],
        ),
        // Past the range `check` answered with, so it stays as it is.
        row(
            "onYes",
            Slot::SelfBrick,
            "setColliding",
            vec![EventValue::Bool(false)],
        ),
        row(
            "onNo",
            Slot::SelfBrick,
            "setColor",
            vec![EventValue::Color(0)],
        ),
    ];
    s.edit_brick(builder, brick, Edit::Events(rows)).unwrap();
    run(&mut s, visitor, 2, "ping", brick).unwrap();
    steps(&mut s, &[builder, visitor], 2);
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
    let log: Vec<String> =
        serde_json::from_value(s.package_state().packages["probe"].global["log"].clone()).unwrap();
    assert_eq!(
        log,
        [
            format!("check:{brick}:true:fxDTSBrick:{visitor}:{builder}:0"),
            format!("tally:{builder}:2:GameConnection:{visitor}:{builder}:1"),
            format!("tally:{visitor}:3:GameConnection:{visitor}:{builder}:2"),
        ]
    );
    let state = &s.simulation().state().bricks[&brick];
    assert_eq!(state.color, 1, "rows in the range ran");
    assert!(state.colliding, "the row past the range did not");
}

#[test]
fn outputs_need_the_brick_events_capability() {
    let (_root, add_ons) = add_ons("no-capability", json!([]));
    let mut s = session();
    let error = format!("{:#}", s.install_packages(add_ons, None).unwrap_err());
    assert!(error.contains("brick_events"), "{error}");
}

#[test]
fn an_input_the_rules_fire_from_an_output_runs_after_the_phase() {
    use bri_events::Slot;
    let (_root, add_ons) = add_ons("relay", json!(["brick_events"]));
    let mut s = session();
    s.set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    s.install_packages(add_ons, None).unwrap();
    let builder = s
        .join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), false)
        .unwrap();
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
    let rows = vec![
        row("onPing", Slot::SelfBrick, "relay", vec![]),
        row(
            "onNo",
            Slot::SelfBrick,
            "setColor",
            vec![EventValue::Color(1)],
        ),
    ];
    s.edit_brick(builder, brick, Edit::Events(rows)).unwrap();
    run(&mut s, builder, 2, "ping", brick).unwrap();
    steps(&mut s, &[builder], 3);
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
    assert_eq!(s.simulation().state().bricks[&brick].color, 1, "onNo ran");
}

#[test]
fn an_add_ons_targets_run_its_outputs_on_their_base() {
    use bri_events::Slot;
    let (_root, add_ons) = add_ons("targets", json!(["brick_events"]));
    let mut s = session();
    s.set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    s.install_packages(add_ons, None).unwrap();
    let catalog = s.event_catalog().unwrap();
    let targets = |input: &str| catalog.input(input).unwrap().targets.clone();
    let pal = ("Pal(Client)".to_string(), "Probe_Pal".to_string());
    assert!(targets("onActivate").contains(&pal), "a native input");
    assert!(targets("onPing").contains(&pal), "an Add-On's input");
    assert!(!targets("onRelay").contains(&pal), "onRelay has no client");
    assert!(
        targets("onRelay").contains(&("Pal(Brick)".into(), "Probe_Pal".into())),
        "every input has the brick"
    );
    assert_eq!(s.package_brick_events().targets.len(), 2, "sent to players");

    let builder = s
        .join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), false)
        .unwrap();
    let visitor = s
        .join("Visitor".into(), Vec3::new(-5.0, 0.05, 0.0), false)
        .unwrap();
    steps(&mut s, &[builder, visitor], 10);
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
    let derived = |input: &str, target: &str, output: &str, n: i64| EventRow {
        target: EventTarget::Derived(target.into()),
        ..row(input, Slot::SelfBrick, output, vec![EventValue::Int(n)])
    };
    // Only the target's own class's outputs, and only where it is offered.
    for refused in [
        derived("onPing", "Pal(Client)", "tally", 1),
        derived("onRelay", "Pal(Client)", "greet", 1),
        derived("onPing", "Pal(Nobody)", "greet", 1),
    ] {
        assert!(
            s.edit_brick(builder, brick, Edit::Events(vec![refused.clone()]))
                .is_err(),
            "{refused:?}"
        );
    }
    let rows = vec![
        derived("onPing", "Pal(Client)", "greet", 4),
        derived("onPing", "Pal(Brick)", "greet", 5),
    ];
    s.edit_brick(builder, brick, Edit::Events(rows)).unwrap();
    run(&mut s, visitor, 2, "ping", brick).unwrap();
    steps(&mut s, &[builder, visitor], 2);
    assert!(
        s.package_diagnostics().is_empty(),
        "{:?}",
        s.package_diagnostics()
    );
    let log: Vec<String> =
        serde_json::from_value(s.package_state().packages["probe"].global["log"].clone()).unwrap();
    assert_eq!(
        log,
        [
            format!("greet:{visitor}:4:Probe_Pal:Pal(Client):GameConnection"),
            format!("greet:{brick}:5:Probe_Pal:Pal(Brick):fxDTSBrick"),
        ]
    );
}
