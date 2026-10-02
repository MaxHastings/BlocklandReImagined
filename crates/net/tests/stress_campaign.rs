//! Stress campaign experiments against the real QUIC host (see
//! docs/stress-lab/weakness-ledger.md). Each test is one replayable
//! experiment: the hostile or pathological traffic is written out in the test
//! body, and the assertion is the outcome the platform promises.
use anyhow::Result;
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_net::{
    client::Client,
    codec,
    protocol::{Hello, JoinBegin, Message, VERSION},
    server::{self, ServerOptions},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    session::{Command, Reply, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use std::time::{Duration, Instant};

fn session() -> Session {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let defs = Definitions {
        entries: [(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]
        .into(),
    };
    let mut session = Session::new(
        Simulation::new(
            World::new("Stress".into(), "fixture".into(), vec![[1.0; 4], [0.0; 4]]),
            defs,
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    session
        .set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    // The campaign's clients plant faster than v20's default plant rate.
    session
        .set_server_settings(bri_admin::ServerSettings {
            bricks_per_second: 100_000,
            ..Default::default()
        })
        .unwrap();
    session
}

fn options() -> ServerOptions {
    ServerOptions {
        bind: "127.0.0.1:0".parse().unwrap(),
        environment: bri_package::environment::Environment::empty(),
        spawn_points: (0..32)
            .map(|i| Vec3::new(-48.0 + 3.0 * i as f32, 0.05, 0.0))
            .collect(),
        certificate: None,
        map_loader: None,
        packages: None,
    }
}

fn endpoint(certificate: &[u8]) -> Result<quinn::Endpoint> {
    endpoint_from(certificate, "127.0.0.1:0")
}
fn endpoint_from(certificate: &[u8], bind: &str) -> Result<quinn::Endpoint> {
    let mut roots = quinn::rustls::RootCertStore::empty();
    roots.add(certificate.to_vec().into())?;
    let mut config = quinn::ClientConfig::with_root_certificates(std::sync::Arc::new(roots))?;
    config.transport_config(std::sync::Arc::new(server::transport()));
    let mut endpoint = quinn::Endpoint::client(bind.parse()?)?;
    endpoint.set_default_client_config(config);
    Ok(endpoint)
}

/// A joined peer driven at the frame level, so a test can send what the
/// shipped client never would.
struct RawPeer {
    _endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    send: quinn::SendStream,
    _receive: quinn::RecvStream,
}
async fn raw_join(server: &server::ServerHandle, name: &str) -> Result<RawPeer> {
    let endpoint = endpoint(&server.certificate)?;
    let connection = endpoint.connect(server.address, "blockland.local")?.await?;
    let (mut send, mut receive) = connection.open_bi().await?;
    codec::write_small_request(&mut send, &JoinBegin::join()).await?;
    let Message::Challenge { .. } =
        codec::decode(&codec::read_frame(&mut receive, codec::MAX_FRAME).await?)?
    else {
        anyhow::bail!("expected challenge")
    };
    let hello = Hello {
        version: VERSION,
        name: name.into(),
        clan: Default::default(),
        packages: Vec::new(),
        resume: None,
        host: None,
        identity: None,
        accept_differences: false,
    };
    codec::write_small_request(&mut send, &hello).await?;
    let welcome: Message =
        codec::decode(&codec::read_frame(&mut receive, codec::MAX_FRAME).await?)?;
    anyhow::ensure!(
        matches!(welcome, Message::Welcome { .. }),
        "expected welcome: {welcome:?}"
    );
    Ok(RawPeer {
        _endpoint: endpoint,
        connection,
        send,
        _receive: receive,
    })
}

async fn join(server: &server::ServerHandle, name: &str) -> Result<Client> {
    Client::connect(
        server.address,
        &server.certificate,
        name.into(),
        Vec::new(),
        None,
    )
    .await
}

/// Round trip of one cheap reliable command.
async fn command_latency(client: &mut Client) -> Result<Duration> {
    let start = Instant::now();
    let reply = tokio::time::timeout(
        Duration::from_secs(30),
        client.command(Command::ToggleLight),
    )
    .await??;
    assert!(matches!(reply, Reply::Accepted), "{reply:?}");
    Ok(start.elapsed())
}

/// E1 (resource pressure / admission). Connections that complete the QUIC
/// handshake but never start the join hold the host's shared admission
/// permits. A handful of such idle connections from one address must not
/// keep a real player out.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn idle_handshakes_from_one_address_cannot_lock_out_real_players() -> Result<()> {
    let server = server::start(session(), options())?;
    // Replay: 96 QUIC connections from one address (127.0.0.2, so the real
    // player on 127.0.0.1 is another source); none opens a stream.
    let mut idle = Vec::new();
    for _ in 0..96 {
        let endpoint = endpoint_from(&server.certificate, "127.0.0.2:0")?;
        if let Ok(Ok(connection)) = tokio::time::timeout(
            Duration::from_secs(2),
            endpoint.connect(server.address, "blockland.local")?,
        )
        .await
        {
            idle.push((endpoint, connection));
        }
    }
    let started = Instant::now();
    let joined = tokio::time::timeout(Duration::from_secs(5), join(&server, "Real")).await;
    assert!(
        matches!(joined, Ok(Ok(_))),
        "a real player was locked out by {} idle connections for {:?}: {:?}",
        idle.len(),
        started.elapsed(),
        joined.map(|r| r.map(|_| ()))
    );
    drop(idle);
    server.stop().await?;
    Ok(())
}

/// E2 (resource pressure / fairness). Every peer's command bodies draw on one
/// shared byte budget, reserved from the declared frame length before the
/// body arrives. Guests declaring maximum-size frames and trickling them must
/// not stall every other player's commands.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn guests_reserving_huge_request_frames_cannot_stall_other_players() -> Result<()> {
    let server = server::start(session(), options())?;
    let mut victim = join(&server, "Victim").await?;
    let baseline = command_latency(&mut victim).await?;
    // Replay: two joined guests each declare a MAX_REQUEST body, and two
    // more the largest player body; each sends 1 byte of it.
    let mut hogs = Vec::new();
    for (name, length) in [
        ("HogA", codec::MAX_REQUEST),
        ("HogB", codec::MAX_REQUEST),
        ("HogC", codec::PLAYER_MAX_REQUEST),
        ("HogD", codec::PLAYER_MAX_REQUEST),
    ] {
        let mut hog = raw_join(&server, name).await?;
        // The host may close a guest's bulk declaration before the byte lands.
        let _ = hog.send.write_all(&(length as u32).to_le_bytes()).await;
        let _ = hog.send.write_all(&[0x80]).await;
        hogs.push(hog);
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let during = command_latency(&mut victim).await?;
    assert!(
        during < Duration::from_secs(1),
        "victim command stalled {during:?} (baseline {baseline:?}) while guests held the shared request budget"
    );
    for hog in &hogs {
        hog.connection.close(0_u32.into(), b"done");
    }
    victim.close();
    server.stop().await?;
    Ok(())
}

/// E3 (resource pressure / fairness). A joined peer writes tiny commands as
/// fast as its stream allows and never reads a reply. Session rate limits
/// reject most of them, but each still crosses the host's shared event queue.
/// Other players' command latency must stay interactive.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_command_flood_does_not_starve_other_players() -> Result<()> {
    use bri_net::protocol::Request;
    let server = server::start(session(), options())?;
    let mut victim = join(&server, "Victim").await?;
    let baseline = command_latency(&mut victim).await?;
    let mut flooder = raw_join(&server, "Flooder").await?;
    // Replay: Request { sequence: 1.., command: ToggleLight } back to back.
    let flood = tokio::spawn(async move {
        let mut sent = 0_u64;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            sent += 1;
            let request = Request::new(sent, Command::ToggleLight, None);
            if codec::write_request(&mut flooder.send, &request, codec::PLAYER_MAX_REQUEST)
                .await
                .is_err()
            {
                break;
            }
        }
        (sent, flooder)
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut worst = Duration::ZERO;
    for _ in 0..10 {
        worst = worst.max(command_latency(&mut victim).await?);
    }
    let (sent, _flooder) = flood.await?;
    assert!(
        worst < Duration::from_millis(500),
        "victim worst latency {worst:?} (baseline {baseline:?}) during a flood of {sent} commands"
    );
    victim.close();
    server.stop().await?;
    Ok(())
}

/// E4 (resource pressure / many editors). 24 clients plant bricks at the
/// session's full per-player action rate at once. The host must keep its
/// tick rate and every client's replica must converge on the same world.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_clients_building_at_once_converge_without_dropping_ticks() -> Result<()> {
    let server = server::start(session(), options())?;
    let mut clients = Vec::new();
    for n in 0..24 {
        clients.push(join(&server, &format!("Builder{n}")).await?);
    }
    // Replay: client n plants a plate at x = 2n - 23.5, z = 20.25 + 0.5k (k < 40),
    // pipelined without waiting for replies.
    let started = Instant::now();
    let mut tasks = Vec::new();
    for (n, mut client) in clients.into_iter().enumerate() {
        tasks.push(tokio::spawn(async move {
            let mut sequences = Vec::new();
            for k in 0..40 {
                sequences.push(
                    client
                        .request(Command::Plant {
                            definition: "plate".into(),
                            position: [2.0 * n as f32 - 23.5, 0.1, 20.25 + 0.5 * k as f32],
                            quarter_turns: 0,
                            color: 0,
                        })
                        .await?,
                );
            }
            let mut planted = 0;
            let mut replies = 0;
            tokio::time::timeout(Duration::from_secs(30), async {
                while replies < sequences.len() {
                    if let bri_net::client::ClientEvent::Reply { result, .. } =
                        client.receive().await?
                    {
                        replies += 1;
                        match result {
                            Ok(Reply::Planted(_)) => planted += 1,
                            other => eprintln!("E4 client {n} reply {replies}: {other:?}"),
                        }
                    }
                }
                anyhow::Ok(())
            })
            .await??;
            anyhow::Ok((planted, client))
        }));
    }
    let mut total = 0;
    let mut clients = Vec::new();
    for task in tasks {
        let (planted, client) = task.await??;
        total += planted;
        clients.push(client);
    }
    let elapsed = started.elapsed();
    assert_eq!(total, 24 * 40, "every non-overlapping plant succeeds");
    // Every replica converges on the host's world.
    for client in &mut clients {
        tokio::time::timeout(Duration::from_secs(10), async {
            while client.replica.world.bricks.len() < total {
                client.receive().await?;
            }
            anyhow::Ok(())
        })
        .await??;
    }
    for client in &clients {
        client.close();
    }
    let report = server.stop().await?;
    eprintln!(
        "E4: {total} plants from 24 clients in {elapsed:?}; ticks {} dropped {}",
        report.ticks, report.dropped_ticks
    );
    assert_eq!(report.final_world.bricks.len(), total);
    // Unoptimized builds are too slow to hold 120 Hz through the burst; the
    // release run (E4 in the ledger) must hold it exactly.
    assert!(
        report.dropped_ticks == 0 || cfg!(debug_assertions),
        "host fell behind its tick rate"
    );
    Ok(())
}
