//! The showcase Add-Ons over real QUIC: one player grabs and lifts a Steel
//! Ball with the Gravity Gun, and another player's replica sees the hold,
//! the gun's beam state and the drop.
mod common;

use anyhow::Result;
use bri_net::{client::Client, server};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    player::MoveInput,
    session::{ActionAim, Command, PackageCommand, Reply},
};
use glam::Vec3;
use std::{path::PathBuf, sync::Arc, time::Duration};

const BALL: &str = "steel-ball-kit:vehicle/steelball";

fn showcase() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase")
}

fn session() -> bri_sim::session::Session {
    let mut session = common::session();
    let path = showcase().join("gravity-gun-tool/assets/weapons.json");
    session
        .set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap())
        .unwrap();
    session
        .set_vehicle_pack(
            bri_vehicles::Pack::load(showcase().join("steel-ball-kit/assets/vehicles.json"))
                .unwrap(),
            Vec::new(),
        )
        .unwrap();
    let packages = [
        ("gravity-gun", Side::Server),
        ("gravity-gun-tool", Side::Shared),
        ("steel-ball", Side::Server),
        ("steel-ball-kit", Side::Shared),
    ]
    .into_iter()
    .map(|(id, side)| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side,
        dir: id.into(),
        role: None,
    })
    .collect();
    let catalog = Catalog::load(
        &showcase(),
        &PackageSet {
            schema_version: 1,
            packages,
        },
        true,
    )
    .unwrap();
    session.install_packages(Arc::new(catalog), None).unwrap();
    session
}

async fn wait(client: &mut Client, predicate: impl Fn(&Client) -> bool) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !predicate(client) {
            client.receive().await?;
        }
        Result::<()>::Ok(())
    })
    .await?
}

fn beam(client: &Client, player: u64) -> Vec<f64> {
    client
        .replica
        .package_state
        .packages
        .get("gravity-gun")
        .and_then(|ns| ns.players.get(&player))
        .and_then(|m| m.get("beam"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
        .unwrap_or_default()
}

async fn gun(client: &mut Client, command: &str) -> Result<Reply> {
    let sequence = client
        .request_with_aim(
            Command::Package(PackageCommand {
                package: "gravity-gun".into(),
                command: command.into(),
                args: vec![],
            }),
            Some(ActionAim {
                yaw: 0.0,
                pitch: -0.2,
            }),
        )
        .await?;
    let mut reply = None;
    tokio::time::timeout(Duration::from_secs(10), async {
        while reply.is_none() {
            if let bri_net::client::ClientEvent::Reply {
                sequence: s,
                result,
            } = client.receive().await?
                && s == sequence
            {
                reply = Some(result);
            }
        }
        Result::<()>::Ok(())
    })
    .await??;
    reply
        .unwrap()
        .map_err(|rejection| anyhow::anyhow!("{rejection:?}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_player_sees_the_gravity_gun_lift_and_drop_a_steel_ball() -> Result<()> {
    let mut game = session();
    let ball = game.spawn_vehicle_at(0, BALL, Vec3::new(0.0, 1.3, -7.0), 0.0, Vec3::ZERO)?;
    let mut options = common::options();
    options.spawn_points = vec![Vec3::new(0.0, 0.05, 0.0), Vec3::new(6.0, 0.05, 0.0)];
    let server = server::start(game, options)?;
    let connect = |name: &str| {
        Client::connect(
            server.address,
            &server.certificate,
            name.into(),
            Vec::new(),
            None,
        )
    };
    let mut thrower = connect("Thrower").await?;
    let mut watcher = connect("Watcher").await?;
    let thrower_id = thrower.owner;
    wait(&mut thrower, |c| {
        c.replica
            .poses
            .get(&c.owner)
            .is_some_and(|p| p.player.grounded)
    })
    .await?;
    let start = {
        wait(&mut watcher, |c| {
            c.replica.vehicle_poses.contains_key(&ball)
        })
        .await?;
        Vec3::from(watcher.replica.vehicle_poses[&ball].position)
    };
    // Look at the ball, then grab it: the watcher sees the beam state
    // name it.
    thrower.movement(
        1,
        &[MoveInput {
            pitch: -0.2,
            ..Default::default()
        }],
        None,
        None,
    )?;
    // `/gravitygun` puts the gun in hand; a hold ends when it is put away.
    assert_eq!(gun(&mut thrower, "gravitygun").await?, Reply::Accepted);
    assert_eq!(gun(&mut thrower, "grab").await?, Reply::Accepted);
    wait(&mut watcher, |c| {
        beam(c, thrower_id).get(..2) == Some(&[1.0, ball as f64][..])
    })
    .await?;
    // Held, the ball follows the thrower's view up.
    thrower.movement(
        2,
        &[MoveInput {
            pitch: 0.3,
            ..Default::default()
        }],
        None,
        None,
    )?;
    wait(&mut watcher, |c| {
        c.replica
            .vehicle_poses
            .get(&ball)
            .is_some_and(|p| p.position[1] > start.y + 1.5)
    })
    .await?;
    // Let go: the watcher sees the beam go out and the ball fall.
    assert_eq!(gun(&mut thrower, "release").await?, Reply::Accepted);
    wait(&mut watcher, |c| {
        beam(c, thrower_id).get(..3) == Some(&[0.0, 0.0, 0.0][..])
    })
    .await?;
    wait(&mut watcher, |c| {
        c.replica
            .vehicle_poses
            .get(&ball)
            .is_some_and(|p| p.position[1] < start.y + 0.5)
    })
    .await?;
    thrower.close();
    watcher.close();
    server.stop().await?;
    Ok(())
}
