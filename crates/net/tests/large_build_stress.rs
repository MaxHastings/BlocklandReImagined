//! Large builds, headless: what 10k, 100k (and, on request, 1M) bricks cost
//! to save, load, host, step and edit, and that they come back exactly.
//!
//! The sizes default to 10k and 100k so the gate runs it on every candidate.
//! `BRI_STRESS_BRICKS=10000,100000,1000000` picks others (a million bricks
//! wants a release build and a few GB; the scheduled run is
//! `cargo test --release -p bri-net --test large_build_stress -- --nocapture`).
//! The table of times and bytes goes to stderr.
//!
//! This is the engine side only: it says nothing about frame rate, which
//! `bri-client`'s `large_build_perf` measures on a real GPU. Times are the
//! calling thread's CPU time (a loaded machine does not stretch them) and
//! the bars are ratios between sizes plus a generous per-brick memory cap,
//! so they hold on a slow runner and still fail when something goes
//! quadratic or a copy of the world leaks.
mod common;

use anyhow::{Result, ensure};
use bri_sim::{
    cpu_time,
    session::{Command, Reply, ToolAction},
};
use bri_world::{Brick, ContentRef, World, build};
use glam::Vec3;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicIsize, Ordering},
    time::Duration,
};

/// Counts live heap bytes, every thread included.
struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        LIVE.fetch_add(size as isize - layout.size() as isize, Ordering::Relaxed);
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

/// Heap bytes per brick above which something keeps a copy it should not.
/// The 100k census measured about a tenth of this for the host alone.
const MAX_HEAP_PER_BRICK: f64 = 4096.0;

fn sizes() -> Vec<u64> {
    std::env::var("BRI_STRESS_BRICKS")
        .ok()
        .map(|text| {
            text.split(',')
                .map(|n| n.trim().parse().expect("BRI_STRESS_BRICKS: a count"))
                .collect()
        })
        .unwrap_or_else(|| vec![10_000, 100_000])
}

/// `count` plates on a grid with several owners, as a loaded build has them.
fn big_world(count: u64) -> World {
    let mut world = World::new("Stress".into(), "fixture".into(), vec![[1.0; 4], [0.0; 4]]);
    for id in 1..=count {
        let (x, z) = ((id % 180) as f32, (id / 180 % 360) as f32 * 0.5);
        let layer = (id / (180 * 360)) as f32;
        let mut brick = Brick::new(
            ContentRef::Resolved("plate".into()),
            [-90.0 + x, 0.1 + 0.2 * layer, 5.25 + z],
            1 + id % 8,
        );
        brick.color = (id % 2) as u8;
        world.bricks.insert(id, brick);
    }
    world.next_brick_id = count + 1;
    world
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = cpu_time::thread();
    let out = f();
    (out, cpu_time::thread().saturating_sub(start))
}

#[derive(Debug)]
struct Row {
    bricks: u64,
    save: Duration,
    load: Duration,
    host: Duration,
    heap_per_brick: f64,
    first: Duration,
    tick: Duration,
    edit: Duration,
}

fn measure(count: u64) -> Result<Row> {
    // Room under the engine's brick limit for the edits below.
    let count = count.min(bri_world::MAX_BRICKS as u64 - 400);
    let world = big_world(count);

    // Save and load a build file.
    let (bytes, save) = timed(|| -> Result<Vec<u8>> {
        let saved = build::SavedBuild::capture(&world, true, true)?;
        build::encode(&saved)
    });
    let bytes = bytes?;
    let (decoded, load) = timed(|| build::decode(&bytes));
    let decoded = decoded?;
    ensure!(
        decoded.world.bricks.len() as u64 == count,
        "{count} bricks saved, {} loaded",
        decoded.world.bricks.len()
    );
    // Every brick, not a sample: ids, places, owners and colours.
    for (id, brick) in &world.bricks {
        let back = decoded.world.bricks.get(id);
        ensure!(
            back.is_some_and(|b| b.position == brick.position
                && b.owner == brick.owner
                && b.color == brick.color),
            "brick {id} came back different"
        );
    }
    drop(decoded);
    drop(bytes);

    // Host it.
    let before = live();
    let (mut session, host) = timed(|| common::session_with(world));
    let heap_per_brick = (live() - before) as f64 / count as f64;
    let owner = session.join("Stress".into(), Vec3::new(0.0, 0.05, 0.0), true)?;
    // The first ticks build what is near the player (colliders, chunks); that
    // one-off is reported as `first`, and the steady tick is timed after it.
    let (warm, first) = timed(|| -> Result<()> {
        for _ in 0..30 {
            session.step()?;
        }
        Ok(())
    });
    warm?;
    let (stepped, tick) = timed(|| -> Result<()> {
        for _ in 0..120 {
            session.step()?;
        }
        Ok(())
    });
    stepped?;
    ensure!(
        session.simulation().state().bricks.len() as u64 == count,
        "stepping changed the brick count"
    );

    // The server's default brick limit (256000) stops players planting past
    // it; saves load past it, so a bigger world needs the limit raised.
    if count >= 250_000 {
        let settings = bri_admin::ServerSettings {
            brick_limit: 2_000_000,
            ..Default::default()
        };
        session.set_server_settings(settings)?;
    }
    // Edit: plant 200 bricks into a clear corner, then undo them all. The
    // host allows 60 actions per second per player, so two ticks pass
    // between commands; only the commands are timed.
    let revision = session.simulation().state().revision;
    let mut edit = Duration::ZERO;
    let mut seq = 0;
    let mut command = |session: &mut bri_sim::session::Session, c: Command| -> Result<Reply> {
        seq += 1;
        let (reply, took) = timed(|| session.command(owner, seq, c));
        edit += took;
        for _ in 0..2 {
            session.step()?;
        }
        reply
    };
    for i in 0..200_u32 {
        let reply = command(
            &mut session,
            Command::Plant {
                definition: "plate".into(),
                position: [
                    -20.0 + 1.5 * (i % 27) as f32,
                    0.1,
                    -4.25 - 0.5 * (i / 27) as f32,
                ],
                quarter_turns: 0,
                color: 0,
            },
        )?;
        ensure!(matches!(reply, Reply::Planted(_)), "plant {i}: {reply:?}");
    }
    for _ in 0..200 {
        command(&mut session, Command::Tool(ToolAction::UndoBrick))?;
    }
    ensure!(
        session.simulation().state().bricks.len() as u64 == count,
        "400 edits left {} bricks, not {count}",
        session.simulation().state().bricks.len()
    );
    ensure!(
        session.simulation().state().revision > revision,
        "edits did not move the world's revision"
    );
    Ok(Row {
        bricks: count,
        save,
        load,
        host,
        heap_per_brick,
        first: first / 30,
        tick: tick / 120,
        edit: edit / 400,
    })
}

#[test]
fn stress_large_builds_save_load_host_and_edit_without_blowing_up() -> Result<()> {
    let mut rows = Vec::new();
    for count in sizes() {
        let row = measure(count)?;
        eprintln!(
            "{:>8} bricks: save {:>7.0?} load {:>7.0?} host {:>7.0?} | {:>5.0} B/brick on the host | first ticks {:>7.2?}, then {:>7.2?}; edit {:>7.2?}",
            row.bricks,
            row.save,
            row.load,
            row.host,
            row.heap_per_brick,
            row.first,
            row.tick,
            row.edit
        );
        ensure!(
            row.heap_per_brick <= MAX_HEAP_PER_BRICK,
            "{} bricks cost {:.0} bytes each on the host",
            row.bricks,
            row.heap_per_brick
        );
        rows.push(row);
    }
    let (small, large) = (rows.first().unwrap(), rows.last().unwrap());
    if large.bricks >= small.bricks * 5 {
        let growth = large.bricks as f64 / small.bricks as f64;
        // Thread CPU time on Windows moves in ~15.6 ms steps, so a release
        // build's small timings read 0; floor at two steps so a ratio never
        // divides by a reading the clock could not resolve.
        let ratio = |a: Duration, b: Duration| b.as_secs_f64() / a.as_secs_f64().max(0.032);
        // Saving, loading and hosting grow with the build; three times the
        // growth means something is worse than linear.
        for (what, a, b) in [
            ("save", small.save, large.save),
            ("load", small.load, large.load),
            ("host", small.host, large.host),
            ("first ticks", small.first, large.first),
        ] {
            ensure!(
                ratio(a, b) <= growth * 3.0,
                "{what} grew {:.1}x for {growth:.0}x the bricks ({a:?} to {b:?})",
                ratio(a, b)
            );
        }
        // A tick and an edit should not depend on how much else is built.
        for (what, a, b) in [
            ("tick", small.tick, large.tick),
            ("edit", small.edit, large.edit),
        ] {
            ensure!(
                b <= a * 8 + Duration::from_millis(5),
                "a {what} costs {b:?} at {} bricks and {a:?} at {}",
                large.bricks,
                small.bricks
            );
        }
    }
    Ok(())
}
