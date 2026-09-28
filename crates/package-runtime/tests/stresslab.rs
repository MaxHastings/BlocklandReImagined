//! The Stress Lab packages load, compile and behave through the runtime
//! alone, without the engine.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog, Dynamic, PlayerKey,
    ops::{Op, authorize},
    script::{Aim, Budget, Call, EntityView, PlayerView, Runtime, Snapshot, entity_map, voxels},
    state::Namespace,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/stresslab")
}
fn set() -> PackageSet {
    let mut packages = vec![PackageEntry {
        id: "v20-bricks".into(),
        version: "4.0.0".into(),
        side: Side::Shared,
        dir: "unused".into(),
        role: Some("brick_catalog".into()),
    }];
    for (id, side) in [
        ("stresslab-world", Side::Server),
        ("stresslab-creeper", Side::Server),
        ("stresslab-creeper-model", Side::Client),
        ("stresslab-economy", Side::Server),
        ("stresslab-hud", Side::Client),
    ] {
        packages.push(PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side,
            dir: id.into(),
            role: None,
        });
    }
    PackageSet {
        schema_version: 1,
        packages,
    }
}
fn player(id: u64) -> PlayerView {
    PlayerView {
        id,
        key: PlayerKey::session(id),
        name: format!("P{id}"),
        position: [0.0; 3],
        alive: true,
        admin: true,
    }
}
fn call<'a>(function: &'a str, args: Vec<Dynamic>, snapshot: &Arc<Snapshot>) -> Call<'a> {
    Call {
        function,
        args,
        budget: Budget::Command,
        snapshot: snapshot.clone(),
        caller: None,
        aim: None,
        entity: None,
        state: Namespace::default(),
        entity_vars: Default::default(),
    }
}

#[test]
fn stress_lab_packages_load_on_server_and_client() {
    let server = Catalog::load(&root(), &set(), true).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(server.packages.len(), 5);
    assert!(server.world().is_some());
    assert!(server.entity("stresslab-creeper:entity/creeper").is_some());
    // A client loads only client-side packages and never sees scripts.
    let client = Catalog::load(&root(), &set(), false).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(
        client.packages.keys().collect::<Vec<_>>(),
        ["stresslab-creeper-model", "stresslab-hud"]
    );
    assert!(client.packages.values().all(|p| p.behaviour.is_none()));
    Runtime::compile(&server).unwrap_or_else(|e| panic!("{e:#?}"));
}

#[test]
fn world_generation_is_deterministic_and_bounded() {
    let catalog = Catalog::load(&root(), &set(), true).unwrap();
    let mut runtime = Runtime::compile(&catalog).unwrap();
    let (_, _, world) = catalog.world().unwrap();
    let snapshot = Arc::new(Snapshot {
        seed: world.seed,
        ..Default::default()
    });
    let mut generate = |cx: i64, cz: i64| {
        let mut c = call(&world.generate, vec![cx.into(), cz.into()], &snapshot);
        c.budget = Budget::Generate;
        let out = runtime
            .call("stresslab-world", c)
            .unwrap_or_else(|e| panic!("{e}"));
        voxels(&out.returned, world.materials.len(), 8 * 8 * 64).unwrap()
    };
    let a = generate(0, 0);
    assert_eq!(a, generate(0, 0));
    assert_ne!(a, generate(1, 0));
    assert!(
        a.iter()
            .all(|v| (0..8).contains(&v[0]) && (0..8).contains(&v[2]))
    );
    assert!(
        a.iter().filter(|v| v[1] == 0).all(|v| v[3] == 6),
        "bedrock floor"
    );
    let many: Vec<_> = (0..6).flat_map(|c| generate(c, -c)).collect();
    for ore in [3, 4, 5] {
        assert!(
            many.iter().any(|v| v[3] == ore),
            "material {ore} never generated"
        );
    }
}

#[test]
fn mining_economy_runs_on_server_state_only() {
    let catalog = Catalog::load(&root(), &set(), true).unwrap();
    let mut runtime = Runtime::compile(&catalog).unwrap();
    let snapshot = Arc::new(Snapshot {
        players: vec![player(5)],
        ..Default::default()
    });
    let key = PlayerKey::session(5);
    let mut state = Namespace::default();
    let defaults = catalog.packages["stresslab-economy"]
        .behaviour
        .as_ref()
        .unwrap()
        .state
        .clone();
    state.players.insert(
        key.clone(),
        defaults
            .player
            .iter()
            .map(|(k, d)| (k.clone(), d.default.clone()))
            .collect(),
    );
    state.global = defaults
        .global
        .iter()
        .map(|(k, d)| (k.clone(), d.default.clone()))
        .collect();
    let mut mine = |state: &mut Namespace, tag: &str| {
        let mut c = call("cmd_mine", vec![5_i64.into()], &snapshot);
        c.caller = Some(5);
        c.aim = Some(Aim {
            brick: Some(77),
            tag: Some(tag.into()),
            look: None,
            position: [0.0; 3],
            distance: 2.0,
        });
        c.state = state.clone();
        let out = runtime
            .call("stresslab-economy", c)
            .unwrap_or_else(|e| panic!("{e}"));
        *state = out.state;
        out.ops
    };
    assert_eq!(
        mine(&mut state, "stresslab-world:material/copper"),
        [Op::RemoveBrick { brick: 77 }]
    );
    mine(&mut state, "stresslab-world:material/copper");
    mine(&mut state, "stresslab-world:material/stone");
    assert_eq!(state.players[&key]["copper"], 2);
    assert_eq!(state.players[&key]["mined"], 3);
    // Bedrock is not mineable: no removal is asked for.
    assert!(
        !mine(&mut state, "stresslab-world:material/bedrock")
            .iter()
            .any(|o| matches!(o, Op::RemoveBrick { .. }))
    );
    let mut c = call("cmd_sell_all", vec![5_i64.into()], &snapshot);
    c.state = state.clone();
    let out = runtime.call("stresslab-economy", c).unwrap();
    assert_eq!(out.state.players[&key]["bits"], 10);
    assert_eq!(out.state.players[&key]["copper"], 0);
    assert_eq!(out.state.global["sold"], 2);
}

#[test]
fn creeper_chases_then_fuses_then_explodes() {
    let catalog = Catalog::load(&root(), &set(), true).unwrap();
    let mut runtime = Runtime::compile(&catalog).unwrap();
    let mut vars = BTreeMap::from([(9_u64, BTreeMap::new())]);
    let mut state = Namespace::default();
    state.global.insert("explosions".into(), 0.into());
    let mut think = |x: f32,
                     vars: &mut BTreeMap<u64, BTreeMap<String, serde_json::Value>>,
                     state: &mut Namespace| {
        let me = EntityView {
            id: 9,
            kind: "stresslab-creeper:entity/creeper".into(),
            position: [x, 0.0, 0.0],
            yaw: 0.0,
            label: String::new(),
            health: 20.0,
            speed: 3.0,
        };
        let snapshot = Arc::new(Snapshot {
            players: vec![player(1)],
            entities: vec![me.clone()],
            ..Default::default()
        });
        let mut c = call("think", vec![entity_map(&me)], &snapshot);
        c.entity = Some(9);
        c.budget = Budget::Think;
        c.entity_vars = Arc::new(vars.clone());
        c.state = state.clone();
        let out = runtime
            .call("stresslab-creeper", c)
            .unwrap_or_else(|e| panic!("{e}"));
        vars.extend(out.entity_vars);
        *state = out.state;
        out.ops
    };
    let far = think(10.0, &mut vars, &mut state);
    assert!(
        far.iter()
            .any(|o| matches!(o, Op::Steer { direction, .. } if direction[0] < 0.0)),
        "{far:?}"
    );
    let exploded = (0..20).any(|_| {
        let ops = think(1.0, &mut vars, &mut state);
        let boom = ops.iter().any(|o| matches!(o, Op::Explode { .. }));
        assert!(
            !boom
                || ops
                    .iter()
                    .any(|o| matches!(o, Op::RemoveEntity { entity: 9 }))
        );
        boom
    });
    assert!(exploded);
    assert_eq!(state.global["explosions"], 1);
}

#[test]
fn one_capability_gate_checks_every_operation() {
    let op = Op::Explode {
        position: [0.0; 3],
        radius: 3.0,
        damage: 10.0,
        brick_radius: 0.0,
    };
    let denied = authorize(
        "stresslab-economy",
        &["world.edit".into(), "chat".into()],
        &op,
    )
    .unwrap_err();
    assert_eq!(denied.code, "op.capability");
    assert!(authorize("stresslab-creeper", &["damage".into()], &op).is_ok());
    let foreign = Op::SpawnEntity {
        kind: "other:entity/x".into(),
        position: [0.0; 3],
        vars: Default::default(),
    };
    assert_eq!(
        authorize("stresslab-creeper", &["entity".into()], &foreign)
            .unwrap_err()
            .code,
        "op.foreign_entity"
    );
    let huge = Op::Explode {
        position: [0.0; 3],
        radius: 500.0,
        damage: 10.0,
        brick_radius: 0.0,
    };
    assert_eq!(
        authorize("stresslab-creeper", &["damage".into()], &huge)
            .unwrap_err()
            .code,
        "op.bounds"
    );
}

#[test]
fn runaway_scripts_are_stopped_and_change_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = dir.path().join("loop");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(
        pkg.join("package.json"),
        r#"{"schema_version":1,"id":"loop","version":"1.0.0","api":1,"name":"Loop","license":"CC0-1.0",
            "provides":[{"kind":"behaviour","id":"loop:behaviour/b","file":"b.json"},{"kind":"script","id":"loop:script/s","file":"s.rhai"}]}"#,
    )
    .unwrap();
    std::fs::write(
        pkg.join("b.json"),
        r#"{"schema_version":1,"script":"s.rhai","commands":[{"name":"spin"},{"name":"big"}],"state":{"global":{"n":{"default":0}}}}"#,
    )
    .unwrap();
    std::fs::write(
        pkg.join("s.rhai"),
        "fn cmd_spin(p) { set(\"n\", 1); loop { } }\nfn cmd_big(p) { let s = \"x\"; loop { s += s; } }\n",
    )
    .unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "loop".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "loop".into(),
            role: None,
        }],
    };
    let catalog = Catalog::load(dir.path(), &set, true).unwrap();
    let mut runtime = Runtime::compile(&catalog).unwrap();
    let snapshot = Arc::new(Snapshot::default());
    let spin = runtime
        .call("loop", call("cmd_spin", vec![1_i64.into()], &snapshot))
        .unwrap_err();
    assert_eq!(spin.code, "script.budget", "{spin}");
    let big = runtime
        .call("loop", call("cmd_big", vec![1_i64.into()], &snapshot))
        .unwrap_err();
    assert_eq!(big.code, "script.limit", "{big}");
}

#[test]
fn broken_packages_report_every_problem_with_codes() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = dir.path().join("bad");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(
        pkg.join("package.json"),
        r#"{"schema_version":1,"id":"bad","version":"1.0","api":9,"name":"Bad","license":"",
            "capabilities":["root"],
            "provides":[{"kind":"hud","id":"other:hud/x","file":"../escape.json"},{"kind":"widget","id":"bad:widget/w","file":"w.json"}]}"#,
    )
    .unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "bad".into(),
            version: "1.0.0".into(),
            side: Side::Client,
            dir: "bad".into(),
            role: None,
        }],
    };
    let problems = Catalog::load(dir.path(), &set, true).unwrap_err();
    let codes: Vec<&str> = problems.iter().map(|d| d.code.as_str()).collect();
    for code in [
        "manifest.version",
        "manifest.api",
        "manifest.license",
        "manifest.capability",
        "manifest.provide.namespace",
        "manifest.provide.unknown_kind",
    ] {
        assert!(codes.contains(&code), "{code} missing from {codes:?}");
    }
    assert!(
        problems
            .iter()
            .all(|d| d.location.as_deref().is_some_and(|l| l.starts_with("bad/"))),
        "{problems:#?}"
    );
}
