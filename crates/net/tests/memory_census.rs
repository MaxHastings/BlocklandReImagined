//! Heap bytes per brick for each copy of a world a listen host keeps: the
//! authority's world, its collision and spatial index, the host player's own
//! replica, and that replica's prediction collision mirror. Run:
//! cargo test --release -p bri-net --test memory_census -- --ignored --nocapture
mod common;

use anyhow::Result;
use bri_net::{client::Client, server};
use bri_world::{Brick, ContentRef, World};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

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

/// `count` plates on a grid, as a loaded build would have them.
fn build(count: u64) -> World {
    let mut world = World::new("Census".into(), "fixture".into(), vec![[1.0; 4], [0.0; 4]]);
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

/// Heap bytes per brick of each copy.
struct Census {
    world: f64,
    host: f64,
    replica: f64,
    mirror: f64,
}

async fn census(count: u64) -> Result<Census> {
    let per = |bytes: isize| bytes as f64 / count as f64;
    let start = live();
    let world = build(count);
    let world_bytes = live() - start;
    let before = live();
    let session = common::session_with(world);
    let definitions = session.simulation().definitions.clone();
    let host = live() - before;
    let handle = server::start(session, common::options())?;
    let before = live();
    let client = Client::connect(
        handle.address,
        &handle.certificate,
        "Census".into(),
        Vec::new(),
        None,
    )
    .await?;
    let replica = live() - before;
    assert_eq!(client.replica.world.bricks.len() as u64, count);
    let before = live();
    let mut mirror = bri_sim::prediction::CollisionMirror::new(definitions, Vec::new(), Vec::new());
    mirror.sync(&client.replica.world.bricks)?;
    let mirror_bytes = live() - before;
    client.close();
    handle.stop().await?;
    drop(mirror);
    Ok(Census {
        world: per(world_bytes),
        // The session holds the world it was given; count what it adds.
        host: per(host),
        replica: per(replica),
        mirror: per(mirror_bytes),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "prints the census; run with --ignored --nocapture"]
async fn memory_census_table() -> Result<()> {
    eprintln!("Brick is {} bytes inline", std::mem::size_of::<Brick>());
    for count in [10_000, 100_000] {
        let c = census(count).await?;
        eprintln!(
            "{count:>7} bricks: world {:.0} B/brick, host adds {:.0}, host player's replica {:.0}, its collision mirror {:.0}; total {:.0}",
            c.world,
            c.host,
            c.replica,
            c.mirror,
            c.world + c.host + c.replica + c.mirror
        );
    }
    Ok(())
}
