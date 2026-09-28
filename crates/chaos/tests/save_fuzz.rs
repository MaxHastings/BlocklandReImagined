//! Damaged and hostile saves: a real save (bricks, owners, events, lights)
//! with values swapped for extremes, keys dropped and lists cut or
//! repeated. Every save either refuses to decode or loads into a running
//! server, twice over itself, and the server keeps stepping with finite
//! state.
use bri_chaos::{
    fixture,
    local::{Chaos, Options, check_replicated},
};
use bri_events::Slot;
use bri_sim::session::{ActionAim, Command};
use bri_world::{EventRow, EventTarget, EventValue, build::SavedBuild};
use proptest::prelude::*;
use serde_json::Value;
use std::sync::OnceLock;

/// A save captured from a short bot run, with event rows added.
fn sample() -> &'static Value {
    static SAMPLE: OnceLock<Value> = OnceLock::new();
    SAMPLE.get_or_init(|| {
        let options = Options {
            seed: 0x5a5e,
            bots: 3,
            ..Options::default()
        };
        let mut chaos = Chaos::new(fixture::synthetic().unwrap(), options).unwrap();
        while chaos.session.simulation().state().bricks.len() < 24 {
            chaos.tick().unwrap();
            assert!(chaos.report.ticks < 4000, "the bots built nothing");
        }
        let mut world = chaos.session.simulation().state().clone();
        let rows = [
            EventRow {
                preserved: None,
                enabled: true,
                input: "onActivate".into(),
                delay_ms: 0,
                target: EventTarget::Slot(Slot::SelfBrick),
                output: "setColor".into(),
                params: vec![EventValue::Color(3)],
            },
            EventRow {
                preserved: None,
                enabled: true,
                input: "onActivate".into(),
                delay_ms: 33,
                target: EventTarget::Slot(Slot::SelfBrick),
                output: "fireRelay".into(),
                params: Vec::new(),
            },
        ];
        bri_world::update_bricks(&mut world.bricks, |brick| brick.events = rows.to_vec());
        let build = SavedBuild::capture(&world, true, true).unwrap();
        // Keep cases quick: the first few dozen bricks are plenty.
        let mut value = serde_json::to_value(&build).unwrap();
        if let Some(Value::Object(bricks)) = value.pointer_mut("/world/bricks") {
            let keep: Vec<_> = bricks.keys().take(32).cloned().collect();
            bricks.retain(|k, _| keep.contains(k));
        }
        value
    })
}

#[derive(Debug, Clone)]
enum Change {
    Number(usize, f64),
    Integer(usize, i64),
    Text(usize, String),
    Flip(usize),
    Drop(usize),
    Repeat(usize, usize),
    Cut(usize, usize),
}

fn change() -> impl Strategy<Value = Change> {
    let numbers = prop_oneof![
        Just(1e39),
        Just(-1e39),
        Just(3.4e38),
        Just(1e-45),
        Just(0.0),
        Just(-0.0),
        Just(-1.0),
        Just(0.5),
        Just(1e9),
        -1e6f64..1e6,
    ];
    let integers = prop_oneof![
        Just(-1i64),
        Just(0),
        Just(255),
        Just(256),
        Just(65536),
        Just(u32::MAX as i64 + 1),
        Just(i64::MAX),
        Just(i64::MIN),
    ];
    let texts = prop_oneof![
        Just(String::new()),
        Just("x".repeat(70_000)),
        Just("../../../etc/passwd".to_string()),
        Just("\u{0}\u{7}\u{202e}\u{fffd}".to_string()),
        Just("v20.brick.nonexistent".to_string()),
        Just("chaos/brick/1x1".to_string()),
        "[a-zA-Z0-9 ]{0,12}",
    ];
    prop_oneof![
        3 => (any::<usize>(), numbers).prop_map(|(at, v)| Change::Number(at, v)),
        3 => (any::<usize>(), integers).prop_map(|(at, v)| Change::Integer(at, v)),
        2 => (any::<usize>(), texts).prop_map(|(at, v)| Change::Text(at, v)),
        1 => any::<usize>().prop_map(Change::Flip),
        2 => any::<usize>().prop_map(Change::Drop),
        1 => (any::<usize>(), 1usize..300).prop_map(|(at, n)| Change::Repeat(at, n)),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(at, n)| Change::Cut(at, n)),
    ]
}

/// Every node's JSON pointer, parents before children.
fn pointers(value: &Value, here: String, out: &mut Vec<String>) {
    out.push(here.clone());
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                pointers(
                    v,
                    format!("{here}/{}", k.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                pointers(v, format!("{here}/{i}"), out);
            }
        }
        _ => {}
    }
}

fn apply(value: &mut Value, change: &Change) {
    let mut all = Vec::new();
    pointers(value, String::new(), &mut all);
    let of = |kind: fn(&Value) -> bool, at: usize, value: &Value| {
        let matching: Vec<_> = all
            .iter()
            .filter(|p| kind(value.pointer(p).unwrap()))
            .collect();
        (!matching.is_empty()).then(|| matching[at % matching.len()].clone())
    };
    match change {
        Change::Number(at, v) => {
            if let Some(p) =
                of(Value::is_f64, *at, value).or_else(|| of(Value::is_number, *at, value))
            {
                *value.pointer_mut(&p).unwrap() = serde_json::json!(v);
            }
        }
        Change::Integer(at, v) => {
            if let Some(p) = of(Value::is_number, *at, value) {
                *value.pointer_mut(&p).unwrap() = serde_json::json!(v);
            }
        }
        Change::Text(at, v) => {
            if let Some(p) = of(Value::is_string, *at, value) {
                *value.pointer_mut(&p).unwrap() = Value::String(v.clone());
            }
        }
        Change::Flip(at) => {
            if let Some(p) = of(Value::is_boolean, *at, value) {
                let slot = value.pointer_mut(&p).unwrap();
                *slot = Value::Bool(!slot.as_bool().unwrap());
            }
        }
        Change::Drop(at) => {
            // Any key of any object, or any element of any list.
            let p = &all[1 + at % (all.len() - 1)];
            let (parent, last) = p.rsplit_once('/').unwrap();
            match value.pointer_mut(parent) {
                Some(Value::Object(map)) => {
                    map.remove(&last.replace("~1", "/").replace("~0", "~"));
                }
                Some(Value::Array(items)) => {
                    items.remove(last.parse::<usize>().unwrap());
                }
                _ => {}
            }
        }
        Change::Repeat(at, n) => {
            if let Some(p) = of(|v| v.as_array().is_some_and(|a| !a.is_empty()), *at, value) {
                let items = value.pointer_mut(&p).unwrap().as_array_mut().unwrap();
                let first = items[0].clone();
                items.extend(std::iter::repeat_n(first, *n));
            }
        }
        Change::Cut(at, n) => {
            if let Some(p) = of(Value::is_array, *at, value) {
                let items = value.pointer_mut(&p).unwrap().as_array_mut().unwrap();
                let keep = n % (items.len() + 1);
                items.truncate(keep);
            }
        }
    }
}

/// Load a save the way an admin does, twice so it lands on itself, and keep
/// the server stepping.
fn load(build: SavedBuild) -> Result<(), TestCaseError> {
    let fixture = fixture::synthetic().unwrap();
    let spawn = fixture.spawn_points[0];
    let mut session = fixture.session;
    let admin = session.join("Admin".into(), spawn, true).unwrap();
    for (sequence, ownership) in [(1, true), (2, false)] {
        let command = Command::LoadBuild {
            build: Box::new(build.clone()),
            ownership,
        };
        let _ = session.command_with_aim(admin, sequence, command, None);
        for _ in 0..4 {
            session
                .step()
                .map_err(|e| TestCaseError::fail(format!("step failed: {e:#}")))?;
            check_replicated(&mut session).map_err(|e| TestCaseError::fail(format!("{e:#}")))?;
        }
    }
    let _ = session.command_with_aim(
        admin,
        3,
        Command::Activate,
        Some(ActionAim {
            yaw: 0.0,
            pitch: -1.2,
        }),
    );
    session
        .step()
        .map_err(|e| TestCaseError::fail(format!("step failed: {e:#}")))?;
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn damaged_saves_refuse_or_load_cleanly(changes in proptest::collection::vec(change(), 1..6)) {
        let mut value = sample().clone();
        for change in &changes {
            apply(&mut value, change);
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        // Both readers: the build file and the dedicated server's world file.
        let _ = bri_world::persistence::decode(&bytes);
        if let Ok(build) = bri_world::build::decode(&bytes) {
            load(build)?;
        }
    }

    #[test]
    fn garbage_bytes_never_panic_the_save_readers(bytes in proptest::collection::vec(any::<u8>(), 0..512), at in any::<usize>()) {
        let _ = bri_world::persistence::decode(&bytes);
        let _ = bri_world::build::decode(&bytes);
        // Truncated real saves.
        let whole = serde_json::to_vec(sample()).unwrap();
        let cut = &whole[..at % whole.len()];
        let _ = bri_world::persistence::decode(cut);
        let _ = bri_world::build::decode(cut);
    }
}

#[test]
fn the_undamaged_sample_loads() {
    let build: SavedBuild = serde_json::from_value(sample().clone()).unwrap();
    build.validate().unwrap();
    load(build).unwrap();
}
