//! Stress Lab acceptance over real QUIC: a loopback host and two clients
//! mine a generated world, meet a creeper, and keep their Bits across a
//! reconnect and a host restart. Package gameplay comes only from the
//! packages in `packages/stresslab`.
use anyhow::Result;
use bri_identity::ClientIdentity;
use bri_net::{
    client::Client,
    server::{self, ServerOptions},
};
use bri_sim::session::PackageArg;
use bri_stresslab::net::{DOWN, command, own, package, wait};
use glam::Vec3;

const ECONOMY: &str = "stresslab-economy";

fn options(spawns: Vec<Vec3>) -> ServerOptions {
    ServerOptions {
        bind: "127.0.0.1:0".parse().unwrap(),
        content_id: "stresslab".into(),
        spawn_points: spawns,
        certificate: None,
        map_loader: None,
    }
}
async fn join(
    server: &server::ServerHandle,
    name: &str,
    identity: &ClientIdentity,
    host: bool,
) -> Result<Client> {
    let host = host.then(|| server.host_token.clone());
    let client = Client::connect_with_identity(
        server.address,
        &server.certificate,
        name.into(),
        "stresslab".into(),
        None,
        host,
        identity,
    )
    .await?;
    Ok(client)
}
/// Mine straight down until `count` blocks are mined, waiting out the
/// command cooldown between swings.
async fn mine(client: &mut Client, count: i64) -> Result<()> {
    let start = own(client, ECONOMY, "mined").unwrap_or(0);
    for _ in 0..(count * 20) {
        let _ = command(client, package(ECONOMY, "mine", vec![]), DOWN).await;
        tokio::time::sleep(std::time::Duration::from_millis(160)).await;
        wait(client, 2, |_| true).await?;
        if own(client, ECONOMY, "mined").unwrap_or(0) >= start + count {
            return Ok(());
        }
    }
    anyhow::bail!(
        "mined {} of {count}",
        own(client, ECONOMY, "mined").unwrap_or(0) - start
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_clients_mine_meet_a_creeper_and_keep_their_bits_across_restart() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let alice_key = ClientIdentity::load_or_create(dir.path().join("alice.key"))?;
    let bob_key = ClientIdentity::load_or_create(dir.path().join("bob.key"))?;
    let (session, spawns) = bri_stresslab::fixture_session(None)?;
    let server = server::start(session, options(spawns.clone()))?;
    let mut alice = join(&server, "Alice", &alice_key, true).await?;
    let mut bob = join(&server, "Bob", &bob_key, false).await?;
    assert!(alice.administrator && !bob.administrator);

    // Both clients receive the same generated world and their own package state.
    wait(&mut alice, 5, |c| own(c, ECONOMY, "bits") == Some(0)).await?;
    wait(&mut bob, 5, |c| own(c, ECONOMY, "bits") == Some(0)).await?;
    assert!(alice.replica.world.bricks.len() > 5_000);

    // Mining replicates to both clients, and each sees the other's count.
    mine(&mut bob, 3).await?;
    let bob_id = bob.owner;
    wait(&mut alice, 5, |c| {
        c.replica.package_state.packages[ECONOMY]
            .players
            .get(&bob_id)
            .and_then(|p| p["mined"].as_i64())
            >= Some(3)
    })
    .await?;
    wait(&mut alice, 5, |_| true).await.ok();
    wait(&mut bob, 5, |_| true).await.ok();
    let (a, b) = (
        alice.replica.world.bricks.len(),
        bob.replica.world.bricks.len(),
    );
    for _ in 0..40 {
        if alice.replica.world.bricks == bob.replica.world.bricks {
            break;
        }
        tokio::select! {
            r = alice.receive() => { r?; }
            r = bob.receive() => { r?; }
        }
    }
    assert_eq!(
        alice.replica.world.bricks, bob.replica.world.bricks,
        "{a} vs {b} bricks"
    );

    // A client cannot invent currency: undeclared commands, wrong arguments
    // and other packages' admin commands are refused with their codes.
    let forged = command(
        &mut bob,
        package(ECONOMY, "give_bits", vec![PackageArg::Int(1_000_000)]),
        None,
    )
    .await
    .unwrap_err();
    assert!(forged.to_string().contains("command.unknown"), "{forged}");
    let wrong = command(
        &mut bob,
        package(ECONOMY, "sell", vec![PackageArg::Int(5)]),
        None,
    )
    .await
    .unwrap_err();
    assert!(wrong.to_string().contains("command.args"), "{wrong}");
    let admin = command(
        &mut bob,
        package("stresslab-creeper", "spawn", vec![]),
        None,
    )
    .await
    .unwrap_err();
    assert!(admin.to_string().contains("command.admin"), "{admin}");
    let ore = own(&bob, ECONOMY, "coal").unwrap() * 2
        + own(&bob, ECONOMY, "copper").unwrap() * 5
        + own(&bob, ECONOMY, "gold").unwrap() * 20;
    command(&mut bob, package(ECONOMY, "sell_all", vec![]), None).await?;
    wait(&mut bob, 5, |c| {
        own(c, ECONOMY, "coal") == Some(0) && own(c, ECONOMY, "copper") == Some(0)
    })
    .await?;
    let bits = own(&bob, ECONOMY, "bits").unwrap();
    assert_eq!(bits, ore);
    let mined = own(&bob, ECONOMY, "mined").unwrap();

    // The creeper: spawned by the administrator, replicated to both, it
    // walks to a player and explodes.
    command(
        &mut alice,
        package("stresslab-creeper", "spawn", vec![]),
        None,
    )
    .await?;
    wait(&mut bob, 5, |c| {
        c.replica
            .entities
            .values()
            .any(|e| e.model == "stresslab-creeper-model:model/creeper")
    })
    .await?;
    wait(&mut alice, 5, |c| !c.replica.entities.is_empty()).await?;
    let explosions = |c: &Client| {
        c.replica
            .package_state
            .packages
            .get("stresslab-creeper")
            .and_then(|n| n.global.get("explosions"))
            .and_then(|v| v.as_i64())
    };
    wait(&mut alice, 40, |c| explosions(c) == Some(1)).await?;
    wait(&mut alice, 5, |c| c.replica.entities.is_empty()).await?;
    let hurt = alice
        .replica
        .vitals
        .values()
        .any(|v| v.health < 100.0 || !v.alive);
    assert!(hurt, "the blast hurt someone: {:?}", alice.replica.vitals);

    // Reconnect: Bob's Bits follow his identity, not the connection.
    bob.close();
    drop(bob);
    wait(&mut alice, 5, |c| !c.replica.names.contains_key(&bob_id)).await?;
    let mut bob = join(&server, "Bob", &bob_key, false).await?;
    wait(&mut bob, 5, |c| own(c, ECONOMY, "bits").is_some()).await?;
    assert_eq!(own(&bob, ECONOMY, "bits"), Some(bits));
    assert_eq!(own(&bob, ECONOMY, "mined"), Some(mined));

    // Restart: the host saves package state and world edits; a new host
    // loads them, and Bob finds his Bits and the holes he dug.
    alice.close();
    bob.close();
    drop((alice, bob));
    let report = server.stop().await?;
    let save = report.packages.expect("package save");
    let removed = save.world.as_ref().unwrap().removed.clone();
    assert!(removed.len() >= 3);
    let bytes = save.encode()?;
    let save = bri_sim::session::PackageSave::decode(&bytes)?;
    let (session, spawns) = bri_stresslab::fixture_session(Some(save))?;
    let server = server::start(session, options(spawns))?;
    let mut bob = join(&server, "Bob", &bob_key, false).await?;
    wait(&mut bob, 5, |c| own(c, ECONOMY, "bits").is_some()).await?;
    assert_eq!(
        own(&bob, ECONOMY, "bits"),
        Some(bits),
        "Bits survived the restart"
    );
    assert_eq!(own(&bob, ECONOMY, "mined"), Some(mined));
    let voxel = |p: [f32; 3]| p.map(|c| ((c - 1.0) / 2.0).round() as i64);
    assert!(
        bob.replica
            .world
            .bricks
            .values()
            .all(|b| !removed.contains(&voxel(b.position))),
        "mined voxels stay mined"
    );
    bob.close();
    drop(bob);
    server.stop().await?;
    Ok(())
}
