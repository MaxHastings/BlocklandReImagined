//! Stress campaign: legal state against the limits of the formats that carry
//! it. Whatever players can build through the command path, a joining client
//! must be able to receive and the host must be able to save.
use anyhow::Result;
use bri_events::{Row, RowSelection, Slot, Target, Value};
use bri_net::{client::Client, server};
use bri_world::{Brick, ContentRef, World};
use std::time::Duration;

mod common;
use common as fixture;

/// One brick's event rows at sizes a player can send: `rows` rows whose
/// `setEventEnabled` selects `indices` row numbers (the world allows 256).
/// Indices come from a seeded generator so the rows do not compress away.
fn heavy_rows(rows: usize, indices: u16) -> Vec<Row> {
    let mut seed = 0x2545_f491_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed % 4096) as u16
    };
    (0..rows)
        .map(|_| Row {
            preserved: None,
            enabled: true,
            input: "onActivate".into(),
            delay_ms: 0,
            target: Target::Slot(Slot::SelfBrick),
            output: "setEventEnabled".into(),
            params: vec![
                Value::Rows(RowSelection::Indices(
                    (0..indices).map(|_| next()).collect(),
                )),
                Value::Bool(true),
            ],
        })
        .collect()
}

fn heavy_world(bricks: u64) -> World {
    let mut world = World::new("Heavy".into(), "fixture".into(), vec![[1.0; 4], [0.0; 4]]);
    for id in 1..=bricks {
        let mut brick = Brick::new(
            ContentRef::Resolved("plate".into()),
            [
                2.0 * (id % 32) as f32 - 23.5,
                0.1,
                20.25 + 0.5 * (id / 32) as f32,
            ],
            0,
        );
        brick.events = heavy_rows(1000, 256);
        world.bricks.insert(id, brick);
    }
    world.next_brick_id = bricks + 1;
    world
}

/// E14 (categories 6, 7). 48 bricks whose event rows are each within what
/// one `SetEvents` command may carry (checked below). A client must still be
/// able to join, and the world must still be savable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_world_of_legal_heavy_bricks_still_joins_and_saves() -> Result<()> {
    let rows = heavy_rows(1000, 256);
    let request = bri_net::codec::encode(&rows)?.len();
    assert!(
        rows.len() <= bri_world::MAX_EVENTS_PER_BRICK
            && request < bri_net::codec::PLAYER_MAX_REQUEST,
        "one brick's rows ({request} bytes) fit one player command"
    );
    let world = heavy_world(48);
    let brick = &world.bricks[&1];
    assert!(bri_net::codec::encode(brick)?.len() as u64 <= brick.stored_bound());
    world.validate()?;
    let saves = tempfile::tempdir()?;
    bri_world::persistence::save_new(&saves.path().join("heavy.json"), &world)?;
    let server = server::start(fixture::session_with(world), fixture::options())?;
    let joined = tokio::time::timeout(
        Duration::from_secs(60),
        Client::connect(
            server.address,
            &server.certificate,
            "Joiner".into(),
            Vec::new(),
            None,
        ),
    )
    .await?;
    let client = joined?;
    client.close();
    server.stop().await?;
    Ok(())
}

