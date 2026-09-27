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
    session
}

fn options() -> ServerOptions {
    ServerOptions {
        bind: "127.0.0.1:0".parse().unwrap(),
        content_id: "fixture-v1".into(),
        spawn_points: (0..16)
            .map(|i| Vec3::new(-24.0 + 3.0 * i as f32, 0.05, 0.0))
            .collect(),
        certificate: None,
        map_loader: None,
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
    codec::write_small_request(&mut send, &JoinBegin { version: VERSION }).await?;
    let Message::Challenge { .. } =
        codec::decode(&codec::read_frame(&mut receive, codec::MAX_FRAME).await?)?
    else {
        anyhow::bail!("expected challenge")
    };
    let hello = Hello {
        version: VERSION,
        name: name.into(),
        content_id: "fixture-v1".into(),
        resume: None,
        host: None,
        identity: None,
    };
    codec::write_small_request(&mut send, &hello).await?;
    let welcome: Message = codec::decode(&codec::read_frame(&mut receive, codec::MAX_FRAME).await?)?;
    anyhow::ensure!(matches!(welcome, Message::Welcome { .. }), "expected welcome: {welcome:?}");
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
        "fixture-v1".into(),
        None,
    )
    .await
}

/// Round trip of one cheap reliable command.
async fn command_latency(client: &mut Client) -> Result<Duration> {
    let start = Instant::now();
    let reply = tokio::time::timeout(Duration::from_secs(30), client.command(Command::ToggleLight))
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
        hog.send.write_all(&(length as u32).to_le_bytes()).await?;
        hog.send.write_all(&[0x80]).await?;
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
