//! Endurance: the same few things done over and over, the way a server
//! that stays up for days does them. Players come and go, build and have
//! their builds undone, the host saves and reloads its world, and an
//! administrator changes the map, sixty times in a row. Afterwards nothing
//! may have been lost on the way (players, bricks, saved bytes) and the
//! heap must be where it was after the warm-up, not climbing with every
//! change of map.
//!
//! No network and no game content. A single cycle's work is small; the
//! test is the repetition. It is named `endurance_` so the tiered runner
//! (`tools/check.py soak`) can pick it out.
mod common;

use anyhow::{Result, ensure};
use bri_sim::session::{Command, Reply, Session, ToolAction};
use bri_world::{Brick, ContentRef, OwnerId, World, build};
use glam::Vec3;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicIsize, Ordering},
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

const CYCLES: usize = 60;
/// Cycles to run before the heap is measured: caches and pools fill once.
const WARM_UP: usize = 12;

/// A map of `count` plates; `salt` makes two maps differ in every brick.
fn map(name: &str, count: u64, salt: f32) -> World {
    let mut world = World::new(name.into(), "fixture".into(), vec![[1.0; 4], [0.0; 4]]);
    for id in 1..=count {
        let (x, z) = ((id % 60) as f32, (id / 60) as f32 * 0.5);
        let mut brick = Brick::new(
            ContentRef::Resolved("plate".into()),
            [-30.0 + x + salt, 0.1, 5.25 + z],
            1 + id % 4,
        );
        brick.color = (id % 2) as u8;
        world.bricks.insert(id, brick);
    }
    world.next_brick_id = count + 1;
    world
}

/// What a world is, for comparing: its bricks by id with place, owner and colour.
fn fingerprint(world: &World) -> Vec<(u64, [i32; 3], u64, u8)> {
    world
        .bricks
        .iter()
        .map(|(id, b)| {
            (
                *id,
                b.position.map(|v| (v * 1000.0).round() as i32),
                b.owner,
                b.color,
            )
        })
        .collect()
}

fn command(session: &mut Session, seq: &mut u64, owner: OwnerId, c: Command) -> Result<Reply> {
    *seq += 1;
    session.command(owner, *seq, c)
}

/// One cycle's hostile traffic from `owner`: plants that no honest client
/// sends, a replayed sequence number and a player who is not here. Each is
/// refused, none changes a brick, and the host keeps stepping.
fn hostile(session: &mut Session, seq: &mut u64, owner: OwnerId, gone: OwnerId) -> Result<()> {
    let plant = |definition: &str, position: [f32; 3], turns: u8, color: u8| Command::Plant {
        definition: definition.into(),
        position,
        quarter_turns: turns,
        color,
    };
    let bricks = session.simulation().state().bricks.len();
    let attacks = [
        plant("plate", [f32::NAN, 0.1, 0.0], 0, 0),
        plant("plate", [0.0, f32::INFINITY, 0.0], 0, 0),
        plant("plate", [1.0e30, 0.1, 1.0e30], 0, 0),
        plant("plate", [0.5, 0.1, -9.25], 0, 255),
        plant("plate", [0.5, 0.1, -9.25], 200, 0),
        plant("no/such/brick", [0.5, 0.1, -9.25], 0, 0),
        plant(&"x".repeat(5000), [0.5, 0.1, -9.25], 0, 0),
        Command::Chat("\u{0}".repeat(4096)),
    ];
    for (n, attack) in attacks.into_iter().enumerate() {
        match command(session, seq, owner, attack) {
            Err(_) => {}
            Ok(Reply::Planted(id)) => anyhow::bail!("hostile plant {n} was accepted as brick {id}"),
            Ok(_) => {}
        }
    }
    // A sequence number that was already used.
    let stale = session.command(owner, 1, plant("plate", [0.5, 0.1, -9.25], 0, 0));
    ensure!(
        !matches!(stale, Ok(Reply::Planted(_))),
        "a replayed sequence number planted a brick"
    );
    // Somebody who left, still sending.
    let ghost = session.command(gone, 1 << 40, plant("plate", [0.5, 0.1, -9.25], 0, 0));
    ensure!(
        !matches!(ghost, Ok(Reply::Planted(_))),
        "a player who left planted a brick"
    );
    ensure!(
        session.simulation().state().bricks.len() == bricks,
        "hostile commands changed the world"
    );
    session.step()?;
    Ok(())
}

#[test]
fn endurance_map_changes_joins_builds_and_saves_lose_nothing_and_leak_nothing() -> Result<()> {
    let maps = [map("Alpha", 1500, 0.0), map("Beta", 2200, 0.5)];
    let mut session = common::session_with(maps[0].clone());
    let admin = session.join("Admin".into(), Vec3::new(0.0, 0.05, 0.0), true)?;
    let mut guests: Vec<OwnerId> = Vec::new();
    for n in 0..3 {
        guests.push(session.join(
            format!("Guest{n}"),
            Vec3::new(4.0 * (n + 1) as f32, 0.05, 0.0),
            false,
        )?);
    }
    let mut seq = 0_u64;
    let mut ghost: OwnerId = 9_999;
    let mut heap = Vec::new();
    let mut saved_bytes = Vec::new();
    for cycle in 0..CYCLES {
        let current = &maps[cycle % 2];
        // Everyone plays a while: builds that the map change will discard,
        // and a build that is taken back again.
        for step in 0..40 {
            if step % 10 == 0 {
                for (n, guest) in guests.iter().enumerate() {
                    let reply = command(
                        &mut session,
                        &mut seq,
                        *guest,
                        Command::Plant {
                            definition: "plate".into(),
                            position: [-20.0 + 1.5 * (step / 10 * 3 + n) as f32, 0.1, -4.25],
                            quarter_turns: 0,
                            color: 0,
                        },
                    )?;
                    ensure!(
                        matches!(reply, Reply::Planted(_)),
                        "cycle {cycle}: {reply:?}"
                    );
                }
            }
            session.step()?;
        }
        // Whoever left last cycle is the ghost; before any has, the first
        // number nobody was ever given.
        hostile(&mut session, &mut seq, guests[1], ghost)?;
        command(
            &mut session,
            &mut seq,
            guests[0],
            Command::Tool(ToolAction::UndoBrick),
        )?;
        ensure!(
            session.simulation().state().bricks.len() as u64 == current.bricks.len() as u64 + 11,
            "cycle {cycle}: {} bricks after 12 plants and one undo on {}",
            session.simulation().state().bricks.len(),
            current.bricks.len()
        );
        // A guest leaves and a new one arrives, every cycle.
        let gone = guests.remove(cycle % guests.len());
        session.disconnect(gone)?;
        ghost = gone;
        guests.push(session.join(
            format!("Visitor{cycle}"),
            Vec3::new(-30.0 + 3.0 * (cycle % 10) as f32, 0.05, -14.0),
            false,
        )?);
        session.step()?;
        // The host saves its world and a later host reads it back whole.
        let captured = build::SavedBuild::capture(session.simulation().state(), true, true)?;
        let bytes = build::encode(&captured)?;
        let again = build::decode(&bytes)?;
        ensure!(
            fingerprint(&again.world) == fingerprint(&captured.world),
            "cycle {cycle}: the saved world came back different"
        );
        saved_bytes.push(bytes.len());
        drop((captured, again, bytes));
        // The administrator changes the map: a fresh session adopts the
        // players, and the old mission goes.
        let mut next = common::session_with(maps[(cycle + 1) % 2].clone());
        next.adopt(session, admin)?;
        session = next;
        session.step()?;
        let next_map = &maps[(cycle + 1) % 2];
        ensure!(
            fingerprint(session.simulation().state()) == fingerprint(next_map),
            "cycle {cycle}: the new map is not the map that was loaded"
        );
        let names = session.snapshot().names;
        ensure!(
            names.len() == 4 && names.values().any(|n| n == "Admin"),
            "cycle {cycle}: players after the change of map: {names:?}"
        );
        // Guests keep their numbers across maps; the visitor who arrived
        // this cycle is one of them.
        guests.retain(|g| names.contains_key(g));
        ensure!(guests.len() == 3, "cycle {cycle}: guests lost on the way");
        heap.push(live());
    }
    // The same map saves about the same bytes however many times it has been
    // through a cycle (cycles 12 and 58 are on the same map).
    let (a, b) = (saved_bytes[WARM_UP], saved_bytes[CYCLES - 2]);
    ensure!(
        a.abs_diff(b) <= 64,
        "saved size drifted: {a} then {b} bytes over {} cycles",
        CYCLES - 2 - WARM_UP
    );
    // Heap: after the warm-up, no growth beyond noise (1 MiB).
    let settled = heap[WARM_UP..].iter().copied().min().unwrap();
    let end = heap[CYCLES - 10..].iter().copied().max().unwrap();
    ensure!(
        end - settled < 1 << 20,
        "the heap grew {} bytes over {} map changes ({settled} to {end})",
        end - settled,
        CYCLES - WARM_UP
    );
    Ok(())
}
