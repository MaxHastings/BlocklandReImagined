//! A brick whose relay loop spawns an explosion on every hop, as playtesters
//! built on the a20 host. On a20 each `spawnExplosion` became a stationary
//! live rocket: the loop filled the host's 1024-projectile budget, nothing
//! exploded, nobody's weapon could fire, and the host fell behind. The host
//! and its player must keep running, explosions must explode, and a loop at
//! v20's 33 ms relay floor must run at v20's speed.
mod common;
use anyhow::{Context, Result};
use bri_net::{client::Client, server};
use bri_sim::presentation::CueKind;
use bri_world::{Brick, ContentRef, EventRow, EventTarget, EventValue, World};
use std::{path::Path, time::Duration};

fn row(input: &str, output: &str, delay_ms: u32, params: Vec<EventValue>) -> EventRow {
    EventRow {
        preserved: None,
        enabled: true,
        input: input.into(),
        delay_ms,
        target: EventTarget::Slot(bri_events::Slot::SelfBrick),
        output: output.into(),
        params,
    }
}

/// `onActivate -> fireRelay`, then `onRelay -> spawnExplosion` and
/// `onRelay -> fireRelay`, relaying after `delay_ms`: an endless loop.
fn explosion_loop(delay_ms: u32) -> Vec<EventRow> {
    vec![
        row("onActivate", "fireRelay", delay_ms, vec![]),
        row(
            "onRelay",
            "spawnExplosion",
            0,
            vec![
                EventValue::Datablock(Some("v20.projectile.rocketlauncherprojectile".into())),
                EventValue::Float(1.0),
            ],
        ),
        row("onRelay", "fireRelay", delay_ms, vec![]),
    ]
}

/// Longest the host may go without sending the player anything. Only a
/// hang guard: a debug build under a heavily loaded gate was measured going
/// 16 s between messages, so this is far from any working host.
const STALL: Duration = Duration::from_secs(300);

/// Ten seconds of host time with the loop running: (explosions the player
/// saw, most live loop rockets it saw, the host's report).
async fn run_loop(delay_ms: u32) -> Result<(usize, usize, server::ServerReport)> {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut world = World::new("Storm".into(), "fixture".into(), vec![[1.0; 4]]);
    let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, -3.25], 1);
    brick.events = explosion_loop(delay_ms);
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let mut game = common::session_with(world);
    game.set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(
        content.join("weapons-pack-009/weapons.json"),
    )?)?)?;
    game.set_event_catalog(
        bri_events::Catalog::load(content.join("events-pack-002/catalog.json"))?,
        Vec::<String>::new(),
    )?;
    game.fire_brick_input(1, "onActivate", None);
    let server = server::start(game, common::options())?;
    let mut player = Client::connect(
        server.address,
        &server.certificate,
        "Watcher".into(),
        Vec::new(),
        None,
    )
    .await?;
    let mut explosions = 0;
    let mut most_projectiles = 0;
    let start = player.replica.tick;
    // Run for 1200 host ticks, however long a busy machine takes to simulate
    // them: a debug build under the gate's parallel load needs minutes. Only
    // a host that stops sending altogether fails here; it sends at least a
    // heartbeat every few ticks, so minutes without a message are a stall.
    while player.replica.tick < start + 1200 {
        tokio::time::timeout(STALL, player.receive())
            .await
            .context("the host stopped sending updates")?
            .context("player connection failed")?;
        let fresh = player.replica.take_cues();
        explosions += fresh
            .iter()
            .filter(|c| matches!(&c.kind, CueKind::WeaponEffect { definition, .. } if definition == "rocketExplosion"))
            .count();
        // The player's own join `spawnProjectile` is not the loop's.
        let rockets = player
            .replica
            .weapons
            .projectiles
            .iter()
            .filter(|p| p.definition.contains("rocketlauncher"))
            .count();
        most_projectiles = most_projectiles.max(rockets);
    }
    player.close();
    Ok((explosions, most_projectiles, server.stop().await?))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the native events and weapons packs; headless QUIC only"]
async fn zero_delay_explosion_loop_is_capped_and_never_stops_the_host() -> Result<()> {
    let (explosions, projectiles, report) = run_loop(0).await?;
    eprintln!(
        "zero delay: {explosions} explosions, {projectiles} projectiles, {} step errors, {} dropped ticks",
        report.step_errors, report.dropped_ticks
    );
    assert_eq!(report.step_errors, 0);
    assert_eq!(projectiles, 0, "explosions must not linger as projectiles");
    assert!(explosions > 1000, "the loop keeps exploding: {explosions}");
    assert!(
        explosions <= 1200 * bri_weapons::MAX_EXPLOSIONS_PER_TICK,
        "per-tick explosion cap: {explosions}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the native events and weapons packs; headless QUIC only"]
async fn relay_loop_at_v20s_33_ms_explodes_thirty_times_a_second() -> Result<()> {
    let (explosions, projectiles, report) = run_loop(33).await?;
    eprintln!("33 ms: {explosions} explosions, {projectiles} projectiles");
    assert_eq!(report.step_errors, 0);
    assert_eq!(projectiles, 0);
    // 33 ms rounds up to the next 120 Hz tick: 30 hops a second.
    assert!(
        (270..=310).contains(&explosions),
        "ten seconds at 33 ms: {explosions}"
    );
    Ok(())
}
