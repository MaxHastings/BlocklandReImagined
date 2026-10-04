//! Damaged and hostile saves: a real save (bricks, owners, events, lights)
//! with values swapped for extremes, keys dropped and lists cut or
//! repeated. Every save either refuses to decode or loads into a running
//! server, twice over itself, and the server keeps stepping with finite
//! state.
use bri_chaos::{
    fixture,
    local::{Chaos, Options, check_replicated},
    mutate::{apply, change},
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
                conditions: vec![],
                preserved: None,
                enabled: true,
                input: "onActivate".into(),
                delay_ms: 0,
                target: EventTarget::Slot(Slot::SelfBrick),
                output: "setColor".into(),
                params: vec![EventValue::Color(3)],
            },
            EventRow {
                conditions: vec![],
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
    #![proptest_config(bri_chaos::proptest_config(256, 0x5a7e))]

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
