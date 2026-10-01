//! Hardening: the package loader and the sandboxed script runtime, attacked
//! directly (no engine). Every test builds its own hostile package in a
//! temporary directory, so it replays on any machine without game content.
//!
//! Tests that expose a defect are `#[ignore = "finding H2-Fn: ..."]` and fail
//! when run with `--ignored`; the others record checks that held.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog, Diagnostic, Dynamic, PlayerKey, Store,
    ops::{CAPABILITIES, ObjectRef, Op, authorize},
    script::{Budget, Call, PlayerView, Runtime, Snapshot},
    state::{self, Namespace, check_value},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

// ---------------------------------------------------------------- fixtures

/// One package on disk: its manifest fields and files.
struct Spec {
    id: &'static str,
    side: Side,
    capabilities: Vec<&'static str>,
    /// (kind, content name, file, bytes)
    provides: Vec<(&'static str, String, String, Vec<u8>)>,
    /// Files written but not provided (or provided under another path).
    extra_files: Vec<(String, Vec<u8>)>,
}
impl Spec {
    fn server(id: &'static str, capabilities: Vec<&'static str>) -> Self {
        Self {
            id,
            side: Side::Server,
            capabilities,
            provides: Vec::new(),
            extra_files: Vec::new(),
        }
    }
    fn provide(
        mut self,
        kind: &'static str,
        name: &str,
        file: &str,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.provides
            .push((kind, name.into(), file.into(), bytes.into()));
        self
    }
    /// A behaviour (`behaviour.json`) and its script (`main.rhai`).
    fn behaviour(self, behaviour: serde_json::Value, script: &str) -> Self {
        self.provide(
            "behaviour",
            "main",
            "behaviour.json",
            serde_json::to_vec(&behaviour).unwrap(),
        )
        .provide("script", "main", "main.rhai", script)
    }
    fn write(&self, root: &Path) {
        let dir = root.join(self.id);
        std::fs::create_dir_all(&dir).unwrap();
        let provides: Vec<_> = self
            .provides
            .iter()
            .map(|(kind, name, file, _)| {
                json!({ "kind": kind, "id": format!("{}:{kind}/{name}", self.id), "file": file })
            })
            .collect();
        let manifest = json!({
            "schema_version": 1,
            "id": self.id,
            "version": "1.0.0",
            "api": 1,
            "name": self.id,
            "license": "CC0-1.0",
            "capabilities": self.capabilities,
            "provides": provides,
        });
        std::fs::write(
            dir.join("package.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        for (_, _, file, bytes) in &self.provides {
            let path = dir.join(file);
            if let Some(parent) = path.parent()
                && parent.starts_with(&dir)
            {
                std::fs::create_dir_all(parent).unwrap();
            }
            if path.starts_with(&dir) && !path.exists() {
                std::fs::write(path, bytes).unwrap();
            }
        }
        for (file, bytes) in &self.extra_files {
            std::fs::write(dir.join(file), bytes).unwrap();
        }
    }
    fn entry(&self) -> PackageEntry {
        PackageEntry {
            id: self.id.into(),
            version: "1.0.0".into(),
            side: self.side,
            dir: self.id.into(),
            role: None,
        }
    }
}

fn load(specs: &[Spec]) -> (tempfile::TempDir, Result<Catalog, Vec<Diagnostic>>) {
    let root = tempfile::tempdir().unwrap();
    for s in specs {
        s.write(root.path());
    }
    let set = PackageSet {
        schema_version: 1,
        packages: specs.iter().map(Spec::entry).collect(),
    };
    let catalog = Catalog::load(root.path(), &set, true);
    (root, catalog)
}
fn codes(problems: &[Diagnostic]) -> Vec<String> {
    problems.iter().map(|d| d.code.clone()).collect()
}
fn load_err(specs: &[Spec]) -> Vec<String> {
    match load(specs).1 {
        Ok(_) => Vec::new(),
        Err(problems) => codes(&problems),
    }
}

/// Compile one server package whose script is `script`, with the given
/// behaviour JSON (script path filled in).
fn runtime(script: &str, commands: serde_json::Value) -> (Catalog, Runtime) {
    let spec = Spec::server("probe", vec!["world.edit", "damage", "entity", "chat"]).behaviour(
        json!({ "schema_version": 1, "script": "main.rhai", "commands": commands,
                "state": { "global": { "g": { "default": 0 } },
                           "player": { "p": { "default": 0 } } } }),
        script,
    );
    let (_root, catalog) = load(&[spec]);
    let catalog = catalog.unwrap_or_else(|e| panic!("{e:#?}"));
    let runtime = Runtime::compile(&catalog).unwrap_or_else(|e| panic!("{e:#?}"));
    (catalog, runtime)
}
fn player(id: u64) -> PlayerView {
    PlayerView {
        id,
        key: PlayerKey::session(id),
        name: format!("P{id}"),
        position: [0.0; 3],
        alive: true,
        admin: false,
        ..Default::default()
    }
}
fn call(function: &str, budget: Budget) -> Call<'_> {
    Call {
        function,
        args: Vec::new(),
        budget,
        snapshot: Arc::new(Snapshot {
            players: vec![player(1), player(2)],
            ..Default::default()
        }),
        caller: Some(1),
        aim: None,
        entity: None,
        state: Namespace::default(),
        entity_vars: Default::default(),
        world: None,
    }
}
/// Run `function` from `script` and time it.
fn run(script: &str, function: &str, budget: Budget) -> (Result<Dynamic, Diagnostic>, Duration) {
    let (_, rt) = runtime(script, json!([]));
    let start = Instant::now();
    let result = rt.call("probe", call(function, budget)).map(|o| o.returned);
    (result, start.elapsed())
}
/// Generous wall-clock limit for one cut-off call, debug-build tolerant.
const CALL_LIMIT: Duration = Duration::from_secs(10);

// ------------------------------------------------- 1. script resource limits

/// An unbounded loop in a command is cut off by the operation budget with the
/// named `script.budget` error, in bounded time.
#[test]
fn unbounded_loop_is_cut_off_with_a_budget_error() {
    for budget in [Budget::Command, Budget::Think, Budget::Tick] {
        let (result, took) = run("fn f() { loop { } }", "f", budget);
        let e = result.unwrap_err();
        assert_eq!(e.code, "script.budget", "{e:?}");
        assert!(took < CALL_LIMIT, "{budget:?} took {took:?}");
    }
    // `while true` and a huge range behave the same.
    let (result, _) = run(
        "fn f() { let n = 0; for i in 0..9223372036854775807 { n += 1; } n }",
        "f",
        Budget::Command,
    );
    assert_eq!(result.unwrap_err().code, "script.budget");
}

/// Unbounded and mutual recursion stop at the call-depth limit with
/// `script.limit` instead of overflowing the native (2 MiB test-thread) stack.
#[test]
fn deep_recursion_stops_at_the_call_depth_limit() {
    let (result, _) = run("fn f() { g(1) } fn g(n) { g(n + 1) }", "f", Budget::Command);
    assert_eq!(result.unwrap_err().code, "script.limit");
    let (result, _) = run(
        "fn f() { a(0) } fn a(n) { b(n + 1) } fn b(n) { a(n + 1) }",
        "f",
        Budget::Command,
    );
    assert_eq!(result.unwrap_err().code, "script.limit");
    // Recursion with big locals at every level: still a named error.
    let (result, _) = run(
        "fn f() { g(0) } fn g(n) { let a = []; a.pad(1000, n); let s = \"x\"; s.pad(4000, 'y'); g(n + 1) + a.len() + s.len() }",
        "f",
        Budget::Command,
    );
    assert_eq!(result.unwrap_err().code, "script.limit");
}

/// Doubling a string, growing an array or a map without bound in one call
/// ends with `script.limit` (data too large), not an out-of-memory abort.
#[test]
fn oversized_strings_arrays_and_maps_are_refused() {
    for (what, script) in [
        ("string", "fn f() { let s = \"x\"; loop { s += s; } }"),
        ("array", "fn f() { let a = []; loop { a.push(1); } }"),
        ("array pad", "fn f() { let a = []; a.pad(10000000, 0); a }"),
        (
            "map",
            "fn f() { let m = #{}; let i = 0; loop { m[`k${i}`] = i; i += 1; } }",
        ),
        (
            "string pad",
            "fn f() { let s = \"\"; s.pad(1000000, 'x'); s }",
        ),
    ] {
        let (result, took) = run(script, "f", Budget::Command);
        let e = result.expect_err(what);
        // Either named cut-off is safe: whichever limit the call reaches first.
        assert!(
            matches!(e.code.as_str(), "script.limit" | "script.budget"),
            "{what}: {e:?}"
        );
        assert!(took < CALL_LIMIT, "{what} took {took:?}");
    }
}

/// Nested containers: 64 arrays of 60 000 elements each, or 1000 maps of
/// 1000 entries each, are refused as one oversized value (the limits are on
/// the whole value, not each container).
#[test]
fn nested_containers_count_toward_one_limit() {
    let (result, _) = run(
        "fn f() { let outer = []; for i in 0..64 { let a = []; a.pad(60000, i); outer.push(a); } outer.len() }",
        "f",
        Budget::Generate,
    );
    assert_eq!(result.unwrap_err().code, "script.limit");
}

/// Many distinct strings, each under the 4 KiB string limit, held in one
/// array: the call's live memory must stay bounded. Rhai sums string
/// lengths across a container against the 4 KiB string limit, so 60 000
/// distinct 4 KiB strings (~240 MiB) are refused with `script.limit`.
#[test]
fn many_large_strings_in_one_array_are_bounded() {
    let (result, took) = run(
        "fn f() { let s = \"\"; s.pad(4000, 'x'); let a = []; for i in 0..60000 { a.push(s + i); } a.len() }",
        "f",
        Budget::Generate,
    );
    // 60 000 strings of 4 000+ bytes: 240 MB of live data in one call.
    match result {
        Err(e) => assert_eq!(e.code, "script.limit"),
        Ok(n) => panic!(
            "a single call held {} strings of ~4 KB (~{} MiB) in {took:?}",
            n.as_int().unwrap(),
            n.as_int().unwrap() * 4000 / (1024 * 1024)
        ),
    }
}

/// Many small allocations (maps in an array) stay within the operation and
/// array budgets and finish or fail quickly.
#[test]
fn many_small_allocations_are_bounded() {
    let (result, took) = run(
        "fn f() { let a = []; for i in 0..50000 { a.push(#{ i: i }); } a.len() }",
        "f",
        Budget::Command,
    );
    assert!(took < CALL_LIMIT, "{took:?}");
    if let Err(e) = result {
        assert!(
            matches!(e.code.as_str(), "script.budget" | "script.limit"),
            "{e:?}"
        );
    }
}

/// The sandbox has no `eval`, no modules, no file or clock access; asking
/// for them is a compile or runtime error, never a capability.
#[test]
fn sandbox_denies_eval_modules_and_io() {
    for (what, script) in [
        ("eval", "fn f() { eval(\"1\") }"),
        ("import", "fn f() { import \"x\" as y; }"),
        ("timestamp", "fn f() { timestamp() }"),
        ("open", "fn f() { open_file(\"/etc/hostname\") }"),
        ("print to stdout", "fn f() { print_to_stdout(1) }"),
    ] {
        let spec = Spec::server("probe", vec![]).behaviour(
            json!({ "schema_version": 1, "script": "main.rhai" }),
            script,
        );
        let (_root, catalog) = load(&[spec]);
        let catalog = catalog.unwrap();
        match Runtime::compile(&catalog) {
            Err(problems) => assert!(!problems.is_empty(), "{what}"),
            Ok(rt) => {
                let e = rt.call("probe", call("f", Budget::Command)).unwrap_err();
                assert_eq!(e.code, "script.error", "{what}: {e:?}");
            }
        }
    }
}

/// A call may queue at most 1024 operations; the 1025th fails the whole call
/// (nothing is applied), and printed output is capped at 32 short lines.
#[test]
fn operations_and_output_per_call_are_capped() {
    let (result, _) = run(
        "fn f() { for i in 0..2000 { broadcast(\"hi\"); } }",
        "f",
        Budget::Command,
    );
    let e = result.unwrap_err();
    assert_eq!(e.code, "script.error");
    assert!(e.message.contains("1024"), "{e:?}");
    let (_, rt) = runtime(
        "fn f() { let s = \"\"; s.pad(4000, 'x'); for i in 0..1000 { print(s); } }",
        json!([]),
    );
    let out = rt.call("probe", call("f", Budget::Command)).unwrap();
    assert!(out.output.len() <= 32);
    assert!(out.output.iter().all(|l| l.chars().count() <= 256));
}

/// A deeply nested expression (10 000 parentheses) is a compile error, not a
/// parser stack overflow.
#[test]
fn deeply_nested_source_is_a_compile_error() {
    let expr = format!("{}1{}", "(".repeat(10_000), ")".repeat(10_000));
    let spec = Spec::server("probe", vec![]).behaviour(
        json!({ "schema_version": 1, "script": "main.rhai" }),
        &format!("fn f() {{ {expr} }}"),
    );
    let (_root, catalog) = load(&[spec]);
    let problems = Runtime::compile(&catalog.unwrap()).err().expect("refused");
    assert_eq!(codes(&problems), ["script.syntax"]);
}

/// Top-level statements (code that would run at load) are refused.
#[test]
fn top_level_code_is_refused() {
    let spec = Spec::server("probe", vec![]).behaviour(
        json!({ "schema_version": 1, "script": "main.rhai" }),
        "let x = 1; loop { } fn f() { }",
    );
    let (_root, catalog) = load(&[spec]);
    let problems = Runtime::compile(&catalog.unwrap()).err().expect("refused");
    assert!(codes(&problems).contains(&"script.top_level".to_string()));
}

// ---------------------------------------------------------- 2. state stores

/// State values are capped per value (4 KiB, depth 4, finite numbers), and a
/// call that writes a bad value fails as a whole.
#[test]
fn state_values_are_bounded_per_value() {
    assert!(check_value(&json!("x".repeat(5000))).is_err());
    assert!(check_value(&json!([[[[[1]]]]])).is_err());
    assert!(check_value(&json!([[[[1]]]])).is_ok());
    let (_, rt) = runtime(
        "fn f() { let s = \"\"; s.pad(4090, 'x'); set(\"g\", [s, s]); }",
        json!([]),
    );
    let e = rt.call("probe", call("f", Budget::Command)).unwrap_err();
    assert!(
        matches!(e.code.as_str(), "script.error" | "script.limit"),
        "{e:?}"
    );
    // add_player cannot overflow into a non-finite or wrapped number.
    let (_, rt) = runtime(
        "fn f() { add_player(1, \"p\", 9223372036854775807); add_player(1, \"p\", 1); }",
        json!([]),
    );
    assert!(rt.call("probe", call("f", Budget::Command)).is_err());
}

/// A script can write per-player state only for players in the snapshot; an
/// unknown or negative id is an error, so it cannot mint entries for
/// arbitrary keys.
#[test]
fn player_state_is_only_for_connected_players() {
    for script in [
        "fn f() { set_player(99, \"p\", 1); }",
        "fn f() { set_player(-1, \"p\", 1); }",
        "fn f() { set_player(\"principal:00\", \"p\", 1); }",
    ] {
        let (_, rt) = runtime(script, json!([]));
        assert!(
            rt.call("probe", call("f", Budget::Command)).is_err(),
            "{script}"
        );
    }
}

/// State admitted up to the engine's state budget still saves: the budget
/// (`MAX_STATE_BYTES`, counted by `stored_size` at every commit) is derived
/// from the store file's limit (finding H2-F1: 70 players at the per-value
/// and key-count limits made ~70 MiB of state, which could not be saved).
#[test]
fn grown_state_still_saves() {
    let value = json!("x".repeat(4000));
    check_value(&value).unwrap();
    let mut store = Store::default();
    let mut values = BTreeMap::new();
    for k in 0..15 {
        values.insert(format!("k{k}"), value.clone());
    }
    let per_player = state::stored_size(&values);
    assert!(per_player <= state::MAX_PLAYER_STATE_BYTES);
    let players = (state::MAX_STATE_BYTES - 1024) / (per_player + 80);
    for p in 0..players as u32 {
        let mut key = [0u8; 32];
        key[..4].copy_from_slice(&p.to_le_bytes());
        store
            .namespace_mut("probe")
            .players
            .insert(PlayerKey::principal(&key), values.clone());
    }
    assert!(store.stored_size() <= state::MAX_STATE_BYTES);
    let encoded = store.encode();
    assert!(
        encoded.is_ok(),
        "state within the admission budget cannot be saved: {:#}",
        encoded.unwrap_err()
    );
}

/// Store files are validated on load: a value past the limits or a wrong
/// schema is refused rather than handed to scripts.
#[test]
fn damaged_store_files_are_refused() {
    let mut store = Store::default();
    store
        .namespace_mut("probe")
        .global
        .insert("g".into(), json!([[[[[[1]]]]]]));
    let bytes = serde_json::to_vec(&json!({ "schema_version": 1, "store": store })).unwrap();
    assert!(Store::decode(&bytes).is_err());
    assert!(Store::decode(b"{\"schema_version\":2,\"store\":{\"namespaces\":{}}}").is_err());
    assert!(Store::decode(b"not json").is_err());
}

// ------------------------------------------------------------ 4. capabilities

/// Every operation a script can request is refused without its capability,
/// with the `op.capability` code, and allowed with it.
#[test]
fn every_operation_needs_its_declared_capability() {
    let ops = [
        Op::RemoveBrick { brick: 1 },
        Op::Explode {
            position: [0.0; 3],
            radius: 4.0,
            damage: 10.0,
            brick_radius: 2.0,
        },
        Op::Damage {
            target: ObjectRef::Player(1),
            amount: 5.0,
            by: Some(2),
            damage_type: None,
        },
        Op::Damage {
            target: ObjectRef::Vehicle(3),
            amount: 5.0,
            by: None,
            damage_type: Some("gunDirect".into()),
        },
        Op::Beam {
            from: [0.0; 3],
            to: [0.0, 0.0, 100.0],
            color: [1.0; 4],
            width: 0.1,
            seconds: 0.2,
            muzzle: Some(1),
        },
        Op::PlayThread {
            player: 1,
            thread: 3,
            sequence: "activate2".into(),
        },
        Op::SetFov {
            player: 1,
            fov: Some(30.0),
        },
        Op::SetFov {
            player: 1,
            fov: None,
        },
        Op::SetImageAmmo {
            player: 1,
            ammo: false,
        },
        Op::SetImageLoaded {
            player: 1,
            loaded: false,
        },
        Op::MountImage {
            player: 1,
            image: Some("probe:image/scope".into()),
        },
        Op::MountImage {
            player: 1,
            image: None,
        },
        Op::Teleport {
            player: 1,
            position: [1.0, 2.0, 3.0],
        },
        Op::Respawn { player: 1 },
        Op::RemoveBody { player: 1 },
        Op::SpawnEntity {
            kind: "probe:entity/x".into(),
            position: [0.0; 3],
            vars: Default::default(),
        },
        Op::RemoveEntity { entity: 1 },
        Op::Steer {
            entity: 1,
            direction: [1.0, 0.0],
            jump: false,
        },
        Op::Label {
            entity: 1,
            label: "a".into(),
        },
        Op::Tell {
            player: 1,
            text: "hi".into(),
        },
        Op::Broadcast { text: "hi".into() },
        Op::Fire {
            projectile: "probe:projectile/x".into(),
            position: [0.0; 3],
            velocity: [0.0, 0.0, 90.0],
            by: None,
        },
    ];
    for op in &ops {
        let needed = op.capability();
        assert!(CAPABILITIES.contains(&needed));
        let others: Vec<String> = CAPABILITIES
            .iter()
            .filter(|c| **c != needed)
            .map(|c| c.to_string())
            .collect();
        assert_eq!(
            authorize("probe", &others, op).unwrap_err().code,
            "op.capability",
            "{op:?}"
        );
        assert!(
            authorize("probe", &[needed.to_string()], op).is_ok(),
            "{op:?}"
        );
    }
    // Spawning another package's kind, or an unprefixed look-alike, is refused.
    for kind in ["other:entity/x", "probe-evil:entity/x", "probex:entity/x"] {
        let op = Op::SpawnEntity {
            kind: kind.into(),
            position: [0.0; 3],
            vars: Default::default(),
        };
        assert_eq!(
            authorize("probe", &["entity".into()], &op)
                .unwrap_err()
                .code,
            "op.foreign_entity",
            "{kind}"
        );
    }
}

/// Extreme parameters are refused by the shape check before any capability
/// or engine work: NaN/inf/huge positions, radii, damage and oversize text.
#[test]
fn extreme_operation_parameters_are_refused() {
    let all: Vec<String> = CAPABILITIES.iter().map(|c| c.to_string()).collect();
    let explode = |p: [f32; 3], r: f32, d: f32, b: f32| Op::Explode {
        position: p,
        radius: r,
        damage: d,
        brick_radius: b,
    };
    let bad = [
        explode([f32::NAN, 0.0, 0.0], 1.0, 1.0, 1.0),
        explode([0.0; 3], f32::INFINITY, 1.0, 1.0),
        explode([0.0; 3], 1e9, 1.0, 1.0),
        explode([0.0; 3], 1.0, 1e9, 1.0),
        explode([0.0; 3], 1.0, f32::NAN, 1.0),
        explode([0.0; 3], 1.0, 1.0, 1e9),
        explode([0.0; 3], -1.0, 1.0, 1.0),
        explode([2e6, 0.0, 0.0], 1.0, 1.0, 1.0),
        Op::Damage {
            target: ObjectRef::Player(1),
            amount: f32::NAN,
            by: None,
            damage_type: None,
        },
        Op::Damage {
            target: ObjectRef::Entity(1),
            amount: -5.0,
            by: None,
            damage_type: None,
        },
        Op::Damage {
            target: ObjectRef::Player(1),
            amount: 5.0,
            by: None,
            damage_type: Some("bad\ntype".into()),
        },
        Op::Beam {
            from: [0.0; 3],
            to: [0.0, 0.0, 5000.0],
            color: [1.0; 4],
            width: 0.1,
            seconds: 0.2,
            muzzle: None,
        },
        Op::Beam {
            from: [0.0; 3],
            to: [0.0, 0.0, 10.0],
            color: [2.0, 1.0, 1.0, 1.0],
            width: 0.1,
            seconds: 0.2,
            muzzle: None,
        },
        Op::Beam {
            from: [0.0; 3],
            to: [0.0, 0.0, 10.0],
            color: [1.0; 4],
            width: 0.0,
            seconds: 0.2,
            muzzle: None,
        },
        Op::Beam {
            from: [0.0; 3],
            to: [0.0, 0.0, 10.0],
            color: [1.0; 4],
            width: 0.1,
            seconds: 60.0,
            muzzle: None,
        },
        Op::PlayThread {
            player: 1,
            thread: 0,
            sequence: "activate".into(),
        },
        Op::PlayThread {
            player: 1,
            thread: 3,
            sequence: "no spaces".into(),
        },
        Op::SetFov {
            player: 1,
            fov: Some(1.0),
        },
        Op::SetFov {
            player: 1,
            fov: Some(f32::NAN),
        },
        Op::MountImage {
            player: 1,
            image: Some("probe:weapon/rifle".into()),
        },
        Op::Teleport {
            player: 1,
            position: [f32::NAN, 0.0, 0.0],
        },
        Op::Fire {
            projectile: "probe:projectile/x".into(),
            position: [0.0; 3],
            velocity: [0.0, 0.0, 1e5],
            by: None,
        },
        Op::Fire {
            projectile: "not a projectile".into(),
            position: [0.0; 3],
            velocity: [0.0; 3],
            by: None,
        },
        Op::Teleport {
            player: 1,
            position: [0.0, 2e6, 0.0],
        },
        Op::SpawnEntity {
            kind: "probe:entity/x".into(),
            position: [0.0, f32::INFINITY, 0.0],
            vars: Default::default(),
        },
        Op::SpawnEntity {
            kind: format!("probe:entity/{}", "x".repeat(200)),
            position: [0.0; 3],
            vars: Default::default(),
        },
        Op::Steer {
            entity: 1,
            direction: [f32::NAN, 0.0],
            jump: false,
        },
        Op::Steer {
            entity: 1,
            direction: [1e30, 0.0],
            jump: false,
        },
        Op::Label {
            entity: 1,
            label: "x".repeat(33),
        },
        Op::Label {
            entity: 1,
            label: "a\nb".into(),
        },
        Op::Tell {
            player: 1,
            text: "x".repeat(257),
        },
        Op::Broadcast { text: "  ".into() },
        Op::Broadcast {
            text: "a\u{7}b".into(),
        },
    ];
    assert!(bad.iter().any(|op| matches!(op, Op::Teleport { .. })));
    for op in &bad {
        assert_eq!(
            authorize("probe", &all, op).unwrap_err().code,
            "op.bounds",
            "{op:?}"
        );
    }
    // Scripts cannot even produce non-finite numbers for an operation.
    let (result, _) = run(
        "fn f() { explode(1.0/0.0, 0.0, 0.0, 1.0, 1.0, 1.0); }",
        "f",
        Budget::Command,
    );
    assert!(result.is_err());
}

/// Entity variables of entities the package does not own cannot be written
/// (the runtime only hands a call its own package's entities).
#[test]
fn entity_vars_of_foreign_entities_cannot_be_written() {
    let (_, rt) = runtime("fn f() { entity_set(42, \"x\", 1); }", json!([]));
    let e = rt.call("probe", call("f", Budget::Think)).unwrap_err();
    assert!(e.message.contains("does not belong"), "{e:?}");
}

// -------------------------------------------------- 6. hostile package files

fn script_pkg(file: &str) -> Spec {
    Spec::server("probe", vec![])
        .provide(
            "behaviour",
            "main",
            "behaviour.json",
            serde_json::to_vec(&json!({ "schema_version": 1, "script": file })).unwrap(),
        )
        .provide("script", "main", file, "fn f() { }")
}

/// `..` and absolute provide paths are refused with `package.file.path`.
#[test]
fn traversal_and_absolute_paths_are_refused() {
    for file in [
        "../outside.rhai",
        "a/../../outside.rhai",
        "/etc/hostname",
        "a\\b.rhai",
        "",
    ] {
        let problems = load_err(&[script_pkg(file)]);
        assert!(
            problems.contains(&"package.file.path".to_string()),
            "{file:?}: {problems:?}"
        );
    }
}

/// A provided file that is a symlink pointing outside the package directory
/// must be refused: packages read only their own files. On Linux the loader
/// follows the link and reads the outside file as the package's script.
#[cfg(unix)]
#[test]
fn symlinked_files_cannot_escape_the_package() {
    let root = tempfile::tempdir().unwrap();
    let secret = root.path().join("secret.rhai");
    std::fs::write(&secret, "fn f() { \"outside the package\" }").unwrap();
    let spec = Spec::server("probe", vec![])
        .provide(
            "behaviour",
            "main",
            "behaviour.json",
            serde_json::to_vec(&json!({ "schema_version": 1, "script": "main.rhai" })).unwrap(),
        )
        // Written by us below as a symlink, not by `write`.
        .provide("script", "main", "main.rhai", Vec::new());
    std::fs::create_dir_all(root.path().join("probe")).unwrap();
    std::os::unix::fs::symlink(&secret, root.path().join("probe/main.rhai")).unwrap();
    spec.write(root.path());
    let set = PackageSet {
        schema_version: 1,
        packages: vec![spec.entry()],
    };
    match Catalog::load(root.path(), &set, true) {
        Err(problems) => assert!(!problems.is_empty()),
        Ok(catalog) => panic!(
            "loaded a script from outside the package: {:?}",
            catalog.packages["probe"].script_source()
        ),
    }
}

/// Scripts over 256 KiB are refused with `package.file.too_large`.
#[test]
fn huge_script_files_are_refused() {
    let big = format!("fn f() {{ }}\n//{}", "x".repeat(300 * 1024));
    let spec = Spec::server("probe", vec![])
        .provide(
            "behaviour",
            "main",
            "behaviour.json",
            serde_json::to_vec(&json!({ "schema_version": 1, "script": "main.rhai" })).unwrap(),
        )
        .provide("script", "main", "main.rhai", big);
    let problems = load_err(&[spec]);
    assert!(
        problems.contains(&"package.file.too_large".to_string()),
        "{problems:?}"
    );
}

/// A script at the size limit made of thousands of functions compiles in
/// bounded time or is refused (Rhai's function limit), never hangs load.
#[test]
fn script_with_many_functions_loads_in_bounded_time() {
    let mut src = String::new();
    let mut i = 0;
    while src.len() < 250 * 1024 {
        src.push_str(&format!("fn f{i}(a) {{ a + {i} }}\n"));
        i += 1;
    }
    let spec = Spec::server("probe", vec![])
        .behaviour(json!({ "schema_version": 1, "script": "main.rhai" }), &src);
    let (_root, catalog) = load(&[spec]);
    let start = Instant::now();
    let _ = Runtime::compile(&catalog.unwrap());
    assert!(start.elapsed() < CALL_LIMIT, "{:?}", start.elapsed());
}

/// Manifests naming capabilities that do not exist are refused at load with
/// `manifest.capability`, listing the known ones.
#[test]
fn unknown_capabilities_are_refused() {
    for capability in ["admin", "fs", "world.edit.all", "Damage", ""] {
        let spec = Spec::server("probe", vec![capability]);
        let problems = load_err(&[spec]);
        assert_eq!(problems, ["manifest.capability"], "{capability:?}");
    }
}

/// A capability that was renamed is refused with its new name, not taken
/// as an alias: alpha keeps no backward compatibility.
#[test]
fn renamed_capabilities_name_their_new_name() {
    let problems = match load(&[Spec::server("probe", vec!["sound"])]).1 {
        Ok(_) => panic!("`sound` must be refused"),
        Err(problems) => problems,
    };
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, "manifest.capability");
    assert!(problems[0].message.contains("now called `effects`"), "{}", problems[0].message);
}

/// Content ids outside the package's namespace (claiming another package's
/// entity kind) are refused, so two packages cannot declare the same kind.
#[test]
fn packages_cannot_declare_another_packages_content() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_vec(&json!({
            "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
            "name": "probe", "license": "CC0-1.0",
            "provides": [{ "kind": "entity", "id": "victim:entity/x", "file": "x.json" }]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("x.json"), "{}").unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![Spec::server("probe", vec![]).entry()],
    };
    let problems = codes(&Catalog::load(root.path(), &set, true).unwrap_err());
    assert!(
        problems.contains(&"manifest.provide.namespace".to_string()),
        "{problems:?}"
    );
}

/// Two provides with the same content id in one package are a composition
/// conflict and must be reported at load. Today the second silently replaces
/// the first.
#[test]
fn duplicate_content_ids_are_reported() {
    let model = |c: f32| {
        serde_json::to_vec(&json!({ "schema_version": 1,
            "boxes": [{ "center": [0.0, 0.0, 0.0], "size": [1.0, 1.0, 1.0], "color": [c, c, c, 1.0] }] }))
        .unwrap()
    };
    let mut spec = Spec::server("probe", vec![])
        .provide("model", "m", "a.json", model(0.1))
        .provide("model", "m", "b.json", model(0.9));
    spec.side = Side::Client;
    let problems = load_err(&[spec]);
    assert!(
        !problems.is_empty(),
        "duplicate ids loaded without a diagnostic"
    );
}

/// Two client HUD panels binding the same key letter to different commands
/// are a composition conflict and must be reported at load.
#[test]
fn conflicting_hud_keys_are_reported() {
    let server = Spec::server("arcade", vec![]).behaviour(
        json!({ "schema_version": 1, "script": "main.rhai",
                "commands": [{ "name": "one" }, { "name": "two" }],
                "state": { "global": { "g": { "default": 0, "visible": "everyone" } } } }),
        "fn cmd_one(p) { } fn cmd_two(p) { }",
    );
    let hud = |id: &'static str, command: &str| {
        let mut s = Spec::server(id, vec![]).provide(
            "hud",
            "panel",
            "hud.json",
            serde_json::to_vec(&json!({
                "schema_version": 1, "slot": "hud.overlay", "anchor": "top_left",
                "title": "T", "background": [0.0, 0.0, 0.0, 1.0], "accent": [1.0, 1.0, 1.0, 1.0],
                "text": [1.0, 1.0, 1.0, 1.0],
                "rows": [{ "label": "G", "bind": "arcade:global/g" }],
                "keys": [{ "key": "F", "label": "Go", "package": "arcade", "command": command }]
            }))
            .unwrap(),
        );
        s.side = Side::Client;
        s
    };
    let problems = load_err(&[server, hud("hud-a", "one"), hud("hud-b", "two")]);
    assert!(
        !problems.is_empty(),
        "key F bound twice without a diagnostic"
    );
}

/// File names with `:` are refused, as package directories already are
/// (`relative_dir`): on Windows `main.rhai:x` is an alternate data stream of
/// `main.rhai`, so two provides could alias the same bytes.
#[cfg(unix)]
#[test]
fn colons_in_file_names_are_refused() {
    let problems = load_err(&[script_pkg("main.rhai:stream")]);
    assert!(
        problems.contains(&"package.file.path".to_string()),
        "{problems:?}"
    );
}

/// The weapon-era calls build the operations they name, check their
/// arguments in the script, and the world questions fail cleanly where the
/// engine offers no world (chunk generation).
#[test]
fn script_calls_build_their_operations_and_world_questions_need_a_world() {
    let (_, rt) = runtime(
        r#"fn f() {
            damage("vehicle:3", 5.0, (), "gunDirect");
            damage(2, 1.0, "player:1");
            beam([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
            beam([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], #{ color: [0.0, 1.0, 0.0], width: 0.5, seconds: 1.0, muzzle: 1 });
            mount_image(1, ());
            set_fov(1, 40);
            set_fov(1, ());
            play_thread(1, 3, "root");
        }"#,
        json!([]),
    );
    let ops = rt.call("probe", call("f", Budget::Command)).unwrap().ops;
    assert_eq!(
        ops[..2],
        [
            Op::Damage {
                target: ObjectRef::Vehicle(3),
                amount: 5.0,
                by: None,
                damage_type: Some("gunDirect".into()),
            },
            Op::Damage {
                target: ObjectRef::Player(2),
                amount: 1.0,
                by: Some(1),
                damage_type: None,
            },
        ]
    );
    assert!(matches!(ops[2], Op::Beam { width: 0.05, seconds: 0.1, muzzle: None, .. }));
    assert_eq!(
        ops[3],
        Op::Beam {
            from: [0.0; 3],
            to: [1.0, 0.0, 0.0],
            color: [0.0, 1.0, 0.0, 1.0],
            width: 0.5,
            seconds: 1.0,
            muzzle: Some(1),
        }
    );
    assert_eq!(ops[4], Op::MountImage { player: 1, image: None });
    assert_eq!(ops[5], Op::SetFov { player: 1, fov: Some(40.0) });
    assert_eq!(ops[6], Op::SetFov { player: 1, fov: None });
    assert_eq!(
        ops[7],
        Op::PlayThread { player: 1, thread: 3, sequence: "root".into() }
    );
    for (script, message) in [
        ("fn f() { raycast([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 5.0) }", "cannot be asked"),
        ("fn f() { can_damage(1, 2) }", "cannot be asked"),
        ("fn f() { raycast([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 5.0) }", "cannot be zero"),
        ("fn f() { raycast([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 5000.0) }", "0 to 2000"),
        ("fn f() { raycast([0.0, 0.0], [0.0, 1.0, 0.0], 5.0) }", "[x, y, z]"),
        ("fn f() { beam([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], #{ colour: 1 }) }", "no option `colour`"),
        ("fn f() { damage(\"vehicle:3\", 5.0, \"vehicle:4\") }", "expected a player"),
    ] {
        let (_, rt) = runtime(script, json!([]));
        let e = rt.call("probe", call("f", Budget::Command)).unwrap_err();
        assert!(e.message.contains(message), "{script}: {}", e.message);
    }
}

/// TorqueScript's colour escapes in a rule's string literals (`\c4`, and
/// `\cr`, `\cp`, `\co`) are the colour codes prints and chat draw by, in
/// plain and interpolated strings alike; a player's name that holds `\c4`
/// stays as typed.
#[test]
fn colour_escapes_in_string_literals_become_colour_codes() {
    let script = r#"
fn colours() {
    let name = players()[0].name;
    [`\c4Dup ${1 + 2}\c6!`, "\\c3x\\cr\\cp\\co", `\c9`, `\cx stays`, name]
}
"#;
    let (_, rt) = runtime(script, json!([]));
    let mut call = call("colours", Budget::Command);
    let mut named = player(1);
    named.name = r"Bob\c4".into();
    call.snapshot = Arc::new(Snapshot {
        players: vec![named],
        ..Default::default()
    });
    let returned = rt.call("probe", call).unwrap().returned;
    let texts: Vec<String> = returned
        .into_array()
        .unwrap()
        .into_iter()
        .map(|v| v.into_string().unwrap())
        .collect();
    assert_eq!(
        texts,
        [
            "\u{E004}Dup 3\u{E006}!",
            "\u{E003}x\u{E00A}\u{E00B}\u{E00C}",
            "\u{E009}",
            r"\cx stays",
            r"Bob\c4",
        ]
    );
}
