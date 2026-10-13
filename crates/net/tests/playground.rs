//! The Mixed Add-On Playground (`packages/playground`) over real QUIC: a
//! clean client downloads exactly the Add-Ons it needs, two players see
//! each other as the Runner with the Pulse Carbine, one drives the Dune
//! Buggy through a brick wall while the other watches, and a player who
//! joins afterwards finds the buggy, its driver and the broken wall as they
//! are. The headless half is `crates/sim/tests/playground.rs`.
mod common;

use anyhow::{Context, Result};
use bri_net::{
    client::{Client, HostPin},
    packages::PackageShelf,
    server,
};
use bri_package::{
    environment::Environment,
    packages::{PackageEntry, PackageSet, Side},
    sync::Cache,
};
use bri_package_runtime::Catalog;
use bri_progress::Progress;
use bri_sim::{
    player::MoveInput,
    session::{Command, SeatSince, ToolCatalog},
};
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use std::{path::PathBuf, sync::Arc, time::Duration};

const MODE: &str = "playground-mode:mode/playground";
const RUNNER: &str = "playground-runner:archetype/runner";
const CARBINE: &str = "playground-rifle:weapon/carbine";
const CARBINE_IMAGE: &str = "playground-rifle:image/carbine";
const BUGGY: &str = "playground-buggy:vehicle/buggy";
const SPAWN: Vec3 = Vec3::new(0.0, 0.05, 8.0);
const WALL_Z: f32 = -16.25;

fn packages() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages")
}

/// The Add-Ons the mode runs, as a host starting it would load them.
fn set() -> PackageSet {
    PackageSet {
        schema_version: 1,
        packages: [
            (
                "playground-runner",
                "playground/playground-runner",
                Side::Server,
            ),
            (
                "playground-runner-look",
                "playground/playground-runner-look",
                Side::Client,
            ),
            (
                "playground-rifle",
                "playground/playground-rifle",
                Side::Shared,
            ),
            (
                "playground-buggy",
                "playground/playground-buggy",
                Side::Shared,
            ),
            ("blockhead_bot", "blockhead_bot", Side::Shared),
            (
                "playground-mode",
                "playground/playground-mode",
                Side::Server,
            ),
        ]
        .into_iter()
        .map(|(id, dir, side)| PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side,
            dir: dir.into(),
            role: None,
        })
        .collect(),
    }
}

/// The host's game: the mode's Add-Ons, a wall of plates 12 wide and 8 high
/// across the buggy's path, and the buggy parked at the origin.
fn session() -> (bri_sim::session::Session, u64) {
    let mut world = World::new("Playground".into(), "playground".into(), vec![[1.0; 4]]);
    for row in 0..8 {
        for column in 0..12 {
            let id = world.next_brick_id;
            world.bricks.insert(
                id,
                Brick::new(
                    ContentRef::Resolved("plate".into()),
                    [column as f32 - 5.5, 0.1 + 0.2 * row as f32, WALL_Z],
                    0,
                ),
            );
            world.next_brick_id += 1;
        }
    }
    let mut session = common::session_with(world);
    // The carbine's weapons and the buggy's gun, merged as a host merges
    // every package's.
    let load = |dir: &str| {
        let path = packages().join(dir).join("weapons.json");
        bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
    };
    let (weapons, problems) = load("playground/playground-rifle/assets").merge(vec![(
        "playground/playground-buggy/assets".into(),
        load("playground/playground-buggy/assets"),
    )]);
    assert!(problems.is_empty(), "{problems:?}");
    session.set_weapon_pack(weapons).unwrap();
    let bots = packages().join("blockhead_bot/assets/bots.json");
    session
        .set_vehicle_pack(
            bri_vehicles::Pack::load(
                packages().join("playground/playground-buggy/assets/vehicles.json"),
            )
            .unwrap(),
            bri_sim::bot_kind::BotPack::from_json(&std::fs::read(bots).unwrap())
                .unwrap()
                .bots,
        )
        .unwrap();
    session
        .set_tool_catalog(ToolCatalog {
            items: [CARBINE.to_string()].into(),
            vehicles: [BUGGY.to_string()].into(),
            ..Default::default()
        })
        .unwrap();
    let mut catalog = Catalog::load(&packages(), &set(), true).unwrap_or_else(|e| panic!("{e:#?}"));
    catalog.mode = catalog
        .for_mode(MODE)
        .unwrap_or_else(|e| panic!("{e:#?}"))
        .mode;
    session.install_packages(Arc::new(catalog), None).unwrap();
    let buggy = session
        .spawn_vehicle_at(0, BUGGY, Vec3::new(0.0, 1.0, 0.0), 0.0, Vec3::ZERO)
        .unwrap();
    (session, buggy)
}

async fn wait(client: &mut Client, predicate: impl Fn(&Client) -> bool) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !predicate(client) {
            client.receive().await?;
        }
        Result::<()>::Ok(())
    })
    .await?
}

/// Lets `client` catch up to server tick `tick`: a player who was not
/// reading has a backlog of older state to get through first.
async fn catch_up(client: &mut Client, tick: u64) -> Result<()> {
    wait(client, |c| c.replica.tick >= tick).await
}

/// Holds `input` for `ticks` of `client`'s own inputs, sent in redundant
/// batches, each acknowledged before the next. Like a real client, it tags
/// its moves with the seat it knows it sits in (`seat`, the last report).
async fn hold(
    client: &mut Client,
    seat: &mut Option<SeatSince>,
    input: MoveInput,
    ticks: usize,
) -> Result<()> {
    let mut left = ticks;
    while left > 0 {
        let n = left.min(bri_net::protocol::MOVEMENT_REDUNDANCY);
        let first = client.replica.poses[&client.owner].acknowledged_input + 1;
        let newest = first + n as u64 - 1;
        *seat = SeatSince::follow(*seat, client.replica.seat_of(client.owner), first);
        client.movement(newest, &vec![input; n], None, *seat)?;
        wait(client, |c| {
            c.replica.poses[&c.owner].acknowledged_input >= newest
        })
        .await?;
        left -= n;
    }
    Ok(())
}

fn feet(client: &Client, owner: u64) -> Vec3 {
    Vec3::from(client.replica.poses[&owner].player.feet)
}

/// Runs at the vehicle from behind, hopping, until the replica says
/// `client` sits in it (v20 boards from above).
/// Returns the seat and the look it boarded with: a driver who then turns
/// the mouse steers.
async fn board(
    client: &mut Client,
    seat: &mut Option<SeatSince>,
    vehicle: u64,
) -> Result<(u8, f32)> {
    for _ in 0..20 {
        let at = Vec3::from(client.replica.vehicle_poses[&vehicle].position);
        let to = at - feet(client, client.owner);
        let yaw = to.x.atan2(-to.z);
        let back = MoveInput {
            forward: -1.0,
            yaw,
            ..Default::default()
        };
        hold(client, seat, back, 20).await?;
        for i in 0..30 {
            let run = MoveInput {
                forward: 1.0,
                jump: i < 3,
                yaw,
                ..Default::default()
            };
            hold(client, seat, run, 1).await?;
            if let Some((v, seat)) = client.replica.vitals[&client.owner].mounted
                && v == vehicle
            {
                return Ok((seat, yaw));
            }
        }
    }
    anyhow::bail!("never boarded")
}

fn standing(client: &Client) -> usize {
    client
        .replica
        .world
        .bricks
        .values()
        .filter(|b| b.visible && b.colliding && b.raycast)
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_players_and_a_late_joiner_share_the_playground() -> Result<()> {
    let (game, buggy) = session();
    let root = packages();
    let environment = Environment::load(&root, &set())?;
    let shelf = PackageShelf::new(&root, &set(), &environment)?;
    let mut options = common::options();
    options.spawn_points = vec![SPAWN];
    options.environment = environment.clone();
    options.packages = Some(Arc::new(shelf));
    let server = server::start(game, options)?;
    let identity_dir = tempfile::tempdir()?;
    let identity =
        bri_identity::ClientIdentity::load_or_create(identity_dir.path().join("client.identity"))?;
    // Every player starts with an empty Add-On cache.
    let join = async |name: &str| {
        let cache_dir = tempfile::tempdir()?;
        let cache = Cache::open(cache_dir.path())?;
        let (client, fetched, _) = Client::connect_fetching(
            server.address,
            HostPin::from(&server.certificate[..]),
            name.into(),
            Vec::new(),
            None,
            &identity,
            &cache,
            Progress::default(),
            |fetched, _| Ok(fetched.iter().map(|f| f.package.clone()).collect()),
        )
        .await?;
        let mut ids: Vec<String> = fetched.iter().map(|f| f.package.id.clone()).collect();
        ids.sort();
        anyhow::Ok((client, ids, cache_dir))
    };

    // A clean client downloads the look, the carbine, the buggy and the
    // bot: what its screen and prediction need. The Runner's rules and the
    // mode stay on the host.
    let (mut driver, ids, _driver_cache) = join("Driver").await?;
    assert_eq!(
        ids,
        [
            "blockhead_bot",
            "playground-buggy",
            "playground-rifle",
            "playground-runner-look"
        ]
    );
    let (mut watcher, _, _watcher_cache) = join("Watcher").await?;
    let (d, w) = (driver.owner, watcher.owner);
    wait(&mut driver, |c| {
        c.replica
            .poses
            .get(&c.owner)
            .is_some_and(|p| p.player.grounded)
            && c.replica.vehicle_poses.contains_key(&buggy)
    })
    .await?;
    wait(&mut watcher, |c| {
        c.replica.poses.contains_key(&d) && c.replica.vehicles.contains_key(&buggy)
    })
    .await?;

    // Each sees the other as the Runner, and has the carbine in their own
    // inventory (inventories are private; others see what is in hand).
    let armed = |c: &Client| {
        c.replica
            .tools
            .get(&c.owner)
            .is_some_and(|t| t.slots.iter().any(|s| s.as_deref() == Some(CARBINE)))
    };
    wait(&mut driver, |c| {
        armed(c) && c.replica.poses.contains_key(&w)
    })
    .await?;
    wait(&mut watcher, armed).await?;
    for (client, other) in [(&watcher, d), (&driver, w)] {
        let body = client
            .replica
            .archetypes
            .resolve(client.replica.poses[&other].player.archetype);
        assert_eq!(body.id, RUNNER);
    }

    // The driver takes the carbine out; the watcher sees it in hand.
    let slot = driver.replica.tools[&d]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(CARBINE))
        .unwrap();
    driver
        .request(Command::EquipTool { slot: Some(slot) })
        .await?;
    let in_hand = |c: &Client| {
        c.replica
            .weapons
            .images
            .get(&d)
            .is_some_and(|images| images.iter().any(|i| i.image == CARBINE_IMAGE))
    };
    wait(&mut watcher, in_hand).await?;

    // In from behind, then full throttle at the wall.
    let mut seat_report = None;
    let (seat, yaw) = board(&mut driver, &mut seat_report, buggy).await?;
    assert_eq!(seat, 0, "the driver's seat");
    wait(&mut watcher, |c| {
        c.replica.vehicles[&buggy].occupants.first() == Some(&Some(d))
    })
    .await?;
    let built = standing(&watcher);
    assert_eq!(built, 12 * 8);
    // The mouse steers: look at the middle of the wall and keep looking at
    // it, so the buggy turns to face it from wherever it was boarded.
    let target = Vec3::new(0.0, 0.0, WALL_Z - 6.0);
    let mut yaw = yaw;
    for _ in 0..30 {
        let at = Vec3::from(driver.replica.vehicle_poses[&buggy].position);
        if at.z < WALL_Z - 2.0 {
            break;
        }
        let to = target - at;
        yaw = to.x.atan2(-to.z);
        let throttle = MoveInput {
            forward: 1.0,
            yaw,
            ..Default::default()
        };
        hold(&mut driver, &mut seat_report, throttle, 30).await?;
        // A real client reads every datagram as it comes; keep the
        // watcher reading so its socket does not overflow and drop them.
        catch_up(&mut watcher, driver.replica.tick).await?;
        if standing(&driver) + 12 < built {
            break;
        }
    }
    let brake = MoveInput {
        yaw,
        ..Default::default()
    };
    // Brake until it stands still, so everyone can be compared at rest.
    for _ in 0..20 {
        hold(&mut driver, &mut seat_report, brake, 30).await?;
        catch_up(&mut watcher, driver.replica.tick).await?;
        let speed = Vec3::from(driver.replica.vehicle_poses[&buggy].velocity).length();
        if speed < 0.2 {
            break;
        }
    }
    let last = driver.replica.vehicle_poses[&buggy].tick;
    wait(&mut watcher, |c| {
        c.replica.vehicle_poses[&buggy].tick >= last
    })
    .await?;
    // Real-time driving is not exact, so check what both players agree on:
    // the buggy reached the wall, smashed through part of it, and every
    // client sees the same buggy and the same bricks.
    let resting = Vec3::from(watcher.replica.vehicle_poses[&buggy].position);
    assert!(
        resting.z < WALL_Z + 3.0,
        "the watcher saw it reach the wall: {resting}"
    );
    // Brick deltas travel apart from vehicle poses, so give the watcher
    // until it sees the same bricks out as the driver.
    let left = standing(&driver);
    assert!(left < built, "the driver saw bricks knocked out: {left}");
    wait(&mut watcher, |c| standing(c) == left)
        .await
        .with_context(|| format!("the watcher never saw {left} bricks standing"))?;
    let seen = Vec3::from(driver.replica.vehicle_poses[&buggy].position);
    assert!(
        seen.distance(resting) < 0.5,
        "both see it at {seen} and {resting}"
    );
    assert_eq!(
        watcher.replica.vitals[&d].mounted,
        Some((buggy, 0)),
        "still driving"
    );
    // Someone joining now finds it all as it is: the buggy at the
    // wall with the Runner at the wheel, and the same bricks out.
    let (mut late, _, _late_cache) = join("Late").await?;
    wait(&mut late, |c| {
        c.world_complete()
            && c.replica.vehicle_poses.contains_key(&buggy)
            && c.replica.poses.contains_key(&d)
    })
    .await?;
    catch_up(&mut late, watcher.replica.tick).await?;
    let there = Vec3::from(late.replica.vehicle_poses[&buggy].position);
    assert!(
        there.distance(resting) < 0.5,
        "the late joiner sees it at {there}"
    );
    assert_eq!(late.replica.vitals[&d].mounted, Some((buggy, 0)));
    assert_eq!(
        late.replica
            .archetypes
            .resolve(late.replica.poses[&d].player.archetype)
            .id,
        RUNNER
    );
    assert!(in_hand(&late), "the late joiner sees the carbine in hand");
    wait(&mut late, |c| standing(c) == left).await?;
    drop((driver, watcher, late));
    server.stop().await?;
    Ok(())
}

/// What `client` sees `owner` as: their body's archetype id.
fn body_of(client: &Client, owner: u64) -> Option<String> {
    let pose = client.replica.poses.get(&owner)?;
    Some(
        client
            .replica
            .archetypes
            .resolve(pose.player.archetype)
            .id
            .clone(),
    )
}

/// `/body <named>`, typed in chat.
async fn become_body(client: &mut Client, named: &str) -> Result<()> {
    client
        .request(Command::Package(bri_sim::session::PackageCommand {
            package: String::new(),
            command: "body".into(),
            args: named
                .split_whitespace()
                .map(|w| bri_sim::session::PackageArg::String(w.into()))
                .collect(),
        }))
        .await?;
    Ok(())
}

/// v0.3.0 W4 over real QUIC, mid-match in the Playground mode, which lets
/// players pick the Blockhead as well as its Runner: a Runner holding the
/// Carbine becomes a Blockhead and the other player sees it still holding
/// the Carbine; seated at the buggy's wheel it becomes a Runner again and
/// stays seated; it climbs out a Runner. A late joiner sees the body,
/// the seat and the Carbine as they are.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn players_switch_bodies_mid_match_armed_and_seated() -> Result<()> {
    const STANDARD: &str = "v20.player.playerstandardarmor";
    let (game, buggy) = session();
    let root = packages();
    let environment = Environment::load(&root, &set())?;
    let shelf = PackageShelf::new(&root, &set(), &environment)?;
    let mut options = common::options();
    options.spawn_points = vec![SPAWN];
    options.environment = environment.clone();
    options.packages = Some(Arc::new(shelf));
    let server = server::start(game, options)?;
    let identity_dir = tempfile::tempdir()?;
    let identity =
        bri_identity::ClientIdentity::load_or_create(identity_dir.path().join("client.identity"))?;
    let join = async |name: &str| {
        let cache_dir = tempfile::tempdir()?;
        let cache = Cache::open(cache_dir.path())?;
        let (client, _, _) = Client::connect_fetching(
            server.address,
            HostPin::from(&server.certificate[..]),
            name.into(),
            Vec::new(),
            None,
            &identity,
            &cache,
            Progress::default(),
            |fetched, _| Ok(fetched.iter().map(|f| f.package.clone()).collect()),
        )
        .await?;
        anyhow::Ok((client, cache_dir))
    };
    let (mut ann, _ann_cache) = join("Ann").await?;
    let (mut bo, _bo_cache) = join("Bo").await?;
    let (a, b) = (ann.owner, bo.owner);
    let armed = |c: &Client| {
        c.replica
            .tools
            .get(&c.owner)
            .is_some_and(|t| t.slots.iter().any(|s| s.as_deref() == Some(CARBINE)))
    };
    wait(&mut ann, |c| {
        armed(c)
            && c.replica.vehicle_poses.contains_key(&buggy)
            && c.replica
                .poses
                .get(&c.owner)
                .is_some_and(|p| p.player.grounded)
    })
    .await?;
    wait(&mut bo, |c| {
        armed(c) && body_of(c, a).as_deref() == Some(RUNNER)
    })
    .await?;
    let slot = ann.replica.tools[&a]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(CARBINE))
        .unwrap();
    ann.request(Command::EquipTool { slot: Some(slot) }).await?;
    let in_hand = |c: &Client| {
        c.replica
            .weapons
            .images
            .get(&a)
            .is_some_and(|images| images.iter().any(|i| i.image == CARBINE_IMAGE))
    };
    wait(&mut bo, in_hand).await?;

    // A Runner holding the Carbine becomes a Blockhead holding it.
    become_body(&mut ann, "Standard Player").await?;
    wait(&mut ann, |c| body_of(c, a).as_deref() == Some(STANDARD)).await?;
    wait(&mut bo, |c| {
        body_of(c, a).as_deref() == Some(STANDARD) && in_hand(c)
    })
    .await?;
    assert_eq!(
        body_of(&bo, b).as_deref(),
        Some(RUNNER),
        "Bo is still a Runner"
    );

    // At the wheel, back to a Runner, still seated.
    let mut seat_report = None;
    let (seat, yaw) = board(&mut ann, &mut seat_report, buggy).await?;
    assert_eq!(seat, 0);
    // `/body` takes one change a half second; idle a moment first.
    hold(
        &mut ann,
        &mut seat_report,
        MoveInput {
            yaw,
            ..Default::default()
        },
        60,
    )
    .await?;
    become_body(&mut ann, "Runner").await?;
    wait(&mut bo, |c| {
        body_of(c, a).as_deref() == Some(RUNNER)
            && c.replica.vehicles[&buggy].occupants.first() == Some(&Some(a))
    })
    .await?;
    assert_eq!(ann.replica.vitals[&a].mounted.map(|m| m.0), Some(buggy));

    // Jet climbs out, a Runner.
    let jet = MoveInput {
        jet: true,
        yaw,
        ..Default::default()
    };
    for _ in 0..10 {
        // One press a batch: the press that gets off makes the host drop
        // the rest of its batch, still tagged with the seat.
        hold(&mut ann, &mut seat_report, jet, 1).await?;
        hold(
            &mut ann,
            &mut seat_report,
            MoveInput {
                yaw,
                ..Default::default()
            },
            30,
        )
        .await?;
        if ann.replica.vitals[&a].mounted.is_none() {
            break;
        }
    }
    wait(&mut bo, |c| {
        c.replica.vitals[&a].mounted.is_none() && body_of(c, a).as_deref() == Some(RUNNER)
    })
    .await?;

    let (mut late, _late_cache) = join("Late").await?;
    wait(&mut late, |c| {
        c.world_complete() && body_of(c, a).as_deref() == Some(RUNNER) && in_hand(c)
    })
    .await?;
    assert_eq!(late.replica.vitals[&a].mounted, None);
    drop((ann, bo, late));
    server.stop().await?;
    Ok(())
}
