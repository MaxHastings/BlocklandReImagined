//! Random wrench event programs on a field of bricks: every output in the
//! catalog with legal and edge parameters, zero and short delays, relays
//! that feed each other and loops, fired over and over while the host
//! steps. However the programs are wired, the host must keep stepping,
//! keep each tick's work bounded and replicate only finite state.
//!
//! `BRI_CONTENT` adds a run over the real v20 event catalog.
use bri_chaos::{bots::Rng, fixture, local::check_replicated};
use bri_events::{Catalog, Param, RowSelection, Slot, testing};
use bri_sim::session::{Command, Session};
use bri_world::{Brick, ContentRef, EventRow, EventTarget, EventValue, World, build::SavedBuild};
use proptest::prelude::*;
use std::time::{Duration, Instant};

const HOST: u64 = 1;
/// Bricks in the field: a 6 x 4 grid of plates.
const FIELD: u64 = 24;
/// Ticks each case runs (five seconds of play).
const TICKS: u32 = 160;
/// No tick may take longer than this, whatever the programs do: one host
/// tick (32 ms) in release builds, where the worst seeds take about 6 ms,
/// and far more in unoptimised debug builds. A chain whose per-hop cost
/// grows with the queue takes far longer than either.
const TICK_BUDGET: Duration = if cfg!(debug_assertions) {
    Duration::from_millis(500)
} else {
    Duration::from_millis(32)
};

fn value(param: &Param, rng: &mut Rng, palette: usize) -> EventValue {
    match param {
        Param::Int { min, max, .. } => EventValue::Int(match rng.below(4) {
            0 => *min,
            1 => *max,
            _ => min + (rng.next_u64() % (max - min + 1).max(1) as u64) as i64,
        }),
        Param::Float { min, max, step, .. } => {
            let steps = ((max - min) / step).floor().max(0.0) as u64;
            EventValue::Float(min + step * (rng.next_u64() % (steps + 1)) as f32)
        }
        Param::Bool => EventValue::Bool(rng.chance(0.5)),
        Param::String { max_length, .. } => EventValue::Text(
            ["", "x", "\u{202e}rtl", "<color:ff0000>hi"][rng.below(4)]
                .chars()
                .take(*max_length as usize)
                .collect(),
        ),
        Param::Datablock { .. } => EventValue::Datablock(None),
        Param::Vector { max_length } => {
            let scale = if rng.chance(0.3) {
                *max_length
            } else {
                rng.range(0.0, *max_length)
            };
            let v = glam::Vec3::new(
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
            )
            .normalize_or_zero()
                * scale;
            EventValue::Vector(v)
        }
        Param::PaintColor { .. } => EventValue::Color(rng.below(palette) as u8),
        Param::IntList { .. } => EventValue::Rows(if rng.chance(0.5) {
            RowSelection::All
        } else {
            RowSelection::Indices((0..rng.below(4)).map(|_| rng.below(8) as u16).collect())
        }),
        Param::List { items } => EventValue::Int(rng.pick(items).map_or(0, |(_, n)| *n)),
    }
}

fn program(catalog: &Catalog, rng: &mut Rng, palette: usize) -> Vec<EventRow> {
    let mut rows = Vec::new();
    for _ in 0..rng.below(7) {
        let Some(input) = rng.pick(&catalog.inputs) else {
            break;
        };
        // A target the input offers, and an output that target has.
        let Some((slot, class)) = rng.pick(&input.targets).cloned() else {
            continue;
        };
        let outputs: Vec<_> = catalog
            .outputs
            .iter()
            .filter(|o| o.class_name.eq_ignore_ascii_case(&class))
            .collect();
        let Some(output) = rng.pick(&outputs) else {
            continue;
        };
        let target = match Slot::parse(&slot) {
            Some(slot) if !rng.chance(0.1) => EventTarget::Slot(slot),
            // Named bricks, most of them missing.
            _ => EventTarget::Named(format!("b{}", rng.below(8))),
        };
        rows.push(EventRow {
            preserved: None,
            enabled: !rng.chance(0.1),
            input: input.name.clone(),
            delay_ms: [0, 0, 0, 33, 100, 1000][rng.below(6)],
            target,
            output: output.name.clone(),
            params: output
                .params
                .iter()
                .map(|p| value(p, rng, palette))
                .collect(),
        });
    }
    rows
}

fn run(mut session: Session, catalog: &Catalog, seed: u64) -> Result<(), TestCaseError> {
    let fail = |what: String| TestCaseError::fail(format!("seed {seed}: {what}"));
    let mut rng = Rng::new(seed);
    let palette = session.simulation().state().palette.len();
    // The programs arrive the way whole builds do: an administrator's Load
    // Bricks, which keeps their zero delays.
    let mut world = World::new("Events".into(), "chaos/map".into(), vec![[1.0; 4]; palette]);
    for i in 0..FIELD {
        let x = (i % 6) as f32 * 0.5 - 1.25;
        let z = (i / 6) as f32 * 0.5 + 2.25;
        let mut brick = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            [x, 0.1, z],
            HOST,
        );
        if i < 8 {
            brick.name = Some(format!("b{i}"));
        }
        brick.events = program(catalog, &mut rng, palette);
        world.bricks.insert(i + 1, brick);
    }
    world.next_brick_id = FIELD + 1;
    let loaded = session.command(
        HOST,
        1,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: true,
        },
    );
    prop_assert!(loaded.is_ok(), "seed {seed}: load refused: {loaded:?}");
    for _ in 0..90 {
        session
            .step()
            .map_err(|e| fail(format!("loading: {e:#}")))?;
    }
    let bricks: Vec<u64> = session
        .simulation()
        .state()
        .bricks
        .keys()
        .copied()
        .collect();
    prop_assert_eq!(
        bricks.len() as u64,
        FIELD,
        "seed {}: the field loaded",
        seed
    );
    let inputs: Vec<String> = catalog.inputs.iter().map(|i| i.name.clone()).collect();
    let mut slowest = Duration::ZERO;
    for tick in 0..TICKS {
        for _ in 0..rng.below(4) {
            let brick = bricks[rng.below(bricks.len())];
            let input = &inputs[rng.below(inputs.len())];
            let player = rng.chance(0.7).then_some(HOST);
            session.fire_brick_input(brick, input, player);
        }
        let started = Instant::now();
        session
            .step()
            .map_err(|e| fail(format!("tick {tick}: {e:#}")))?;
        slowest = slowest.max(started.elapsed());
        let _ = session.take_event_diagnostics();
        let _ = session.take_cues();
        let _ = session.take_dirty();
    }
    prop_assert!(
        slowest <= TICK_BUDGET,
        "seed {seed}: a tick took {slowest:?} with {} events pending",
        session.pending_events()
    );
    check_replicated(&mut session).map_err(|e| fail(format!("{e:#}")))?;
    Ok(())
}

fn synthetic() -> Session {
    let fixture = fixture::synthetic().unwrap();
    let mut session = fixture.session;
    session
        .set_event_catalog(testing::catalog(), Vec::new())
        .unwrap();
    assert_eq!(
        session
            .join("Host".into(), fixture.spawn_points[0], true)
            .unwrap(),
        HOST
    );
    session
}

/// How many cases to run: `name` when set, else a smoke count for the gate
/// or the full soak under `BRI_BENCH`.
fn smoke(name: &str, gate: u64, bench: u64) -> u64 {
    let soak = std::env::var_os("BRI_BENCH").is_some();
    bri_chaos::env(name, if soak { bench } else { gate })
}

proptest! {
    // A looping case takes seconds in a debug build, so the gate runs a
    // smoke case or two; `BRI_BENCH` (or `BRI_CHAOS_CASES`) soaks.
    #![proptest_config(ProptestConfig { cases: smoke("BRI_CHAOS_CASES", 2, 8) as u32, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn random_event_programs_keep_the_host_stepping(seed in any::<u64>()) {
        run(synthetic(), &testing::catalog(), seed)?;
    }
}

#[test]
#[ignore = "needs generated content; set BRI_CONTENT"]
fn random_v20_event_programs_keep_the_host_stepping() {
    let Some(root) = fixture::content_root() else {
        eprintln!("skipped: BRI_CONTENT is not set");
        return;
    };
    for seed in 0..smoke("BRI_CHAOS_SEEDS", 2, 16) {
        let fixture = fixture::content(&root, "v20/add-ons/map_slate/slate.mis").unwrap();
        let mut session = fixture.session;
        let catalog = session
            .event_catalog()
            .expect("the content installs the v20 events")
            .clone();
        assert_eq!(
            session
                .join("Host".into(), fixture.spawn_points[0], true)
                .unwrap(),
            HOST
        );
        run(session, &catalog, seed).unwrap();
    }
}
