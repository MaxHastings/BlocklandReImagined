//! Hardening over the real transport: the QUIC host must reject or ignore
//! frames, datagrams and credentials a normal client never sends, keep the
//! authoritative state unchanged, and keep serving everyone else.
//!
//! Synthetic sessions only (no original game assets). Each test lists its
//! exact request sequence. The findings these tests first confirmed are
//! recorded in docs/stress-lab/weakness-ledger.md.
use anyhow::{Context, Result};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_identity::ClientIdentity;
use bri_net::{
    client::{Client, ClientEvent},
    codec,
    protocol::{
        Hello, IdentityProof, JoinBegin, MAX_DATAGRAM, MOVEMENT_REDUNDANCY, Message, Movement,
        Request, ResumeToken, VERSION, identity_transcript,
    },
    server::{self, ServerHandle, ServerOptions},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde::Serialize;
use sha2::Digest;
use std::{net::SocketAddr, time::Duration};

// ---------------------------------------------------------------- fixtures

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
            World::new(
                "Hardening".into(),
                "fixture".into(),
                vec![[1.0; 4], [0.0; 4]],
            ),
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

const SPAWNS: [Vec3; 3] = [
    Vec3::new(0.0, 0.05, 0.0),
    Vec3::new(3.0, 0.05, 0.0),
    Vec3::new(-3.0, 0.05, 0.0),
];

fn options() -> ServerOptions {
    ServerOptions {
        bind: "127.0.0.1:0".parse().unwrap(),
        environment: bri_package::environment::Environment::empty(),
        spawn_points: SPAWNS.to_vec(),
        certificate: None,
        map_loader: None,
        autosave: None,
        packages: None,
    }
}

fn shared_package(id: &str) -> bri_package::environment::PackageRef {
    bri_package::environment::PackageRef {
        id: id.into(),
        version: "1.0.0".into(),
        side: bri_package::packages::Side::Shared,
        hash: "cd".repeat(32),
        size: 1,
    }
}

fn hello(name: &str) -> Hello {
    Hello {
        version: VERSION,
        name: name.into(),
        packages: Vec::new(),
        resume: None,
        host: None,
        identity: None,
    }
}

async fn connect(server: &ServerHandle, name: &str) -> Result<Client> {
    Client::connect(
        server.address,
        &server.certificate,
        name.into(),
        Vec::new(),
        None,
    )
    .await
}

/// A raw QUIC connection to the host, bound to `local` (a loopback address),
/// that has sent `JoinBegin` and read the identity challenge.
struct Raw {
    _endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    send: quinn::SendStream,
    receive: quinn::RecvStream,
    nonce: [u8; 32],
}
async fn raw_endpoint(certificate: &[u8], local: &str) -> Result<quinn::Endpoint> {
    let mut roots = quinn::rustls::RootCertStore::empty();
    roots.add(certificate.to_vec().into())?;
    let mut config = quinn::ClientConfig::with_root_certificates(std::sync::Arc::new(roots))?;
    config.transport_config(std::sync::Arc::new(server::transport()));
    let mut endpoint = quinn::Endpoint::client(local.parse()?)?;
    endpoint.set_default_client_config(config);
    Ok(endpoint)
}
async fn raw_challenge(
    address: SocketAddr,
    certificate: &[u8],
    begin: u32,
) -> Result<(Raw, Message)> {
    let endpoint = raw_endpoint(certificate, "127.0.0.1:0").await?;
    let connection = endpoint.connect(address, "blockland.local")?.await?;
    let (mut send, mut receive) = connection.open_bi().await?;
    codec::write_small_request(&mut send, &JoinBegin { version: begin, purpose: bri_net::protocol::Purpose::Join }).await?;
    let first =
        codec::decode::<Message>(&codec::read_frame(&mut receive, codec::MAX_FRAME).await?)?;
    let nonce = match &first {
        Message::Challenge { nonce, .. } => *nonce,
        _ => [0; 32],
    };
    Ok((
        Raw {
            _endpoint: endpoint,
            connection,
            send,
            receive,
            nonce,
        },
        first,
    ))
}
/// Complete a raw handshake with `make(nonce)` as the Hello; returns the
/// connection and the host's answer (Welcome or Rejected).
async fn raw_join(
    server: &ServerHandle,
    make: impl FnOnce(&[u8; 32]) -> Hello,
) -> Result<(Raw, Message)> {
    let (mut raw, _) = raw_challenge(server.address, &server.certificate, VERSION).await?;
    let hello = make(&raw.nonce);
    codec::write_small_request(&mut raw.send, &hello).await?;
    let answer = tokio::time::timeout(
        Duration::from_secs(10),
        codec::read_frame(&mut raw.receive, codec::MAX_FRAME),
    )
    .await??;
    Ok((raw, codec::decode::<Message>(&answer)?))
}
/// Why the join was refused: the reason, or the differing packages.
fn rejected(message: &Message) -> Option<String> {
    match message {
        Message::Rejected(reason) => Some(reason.clone()),
        Message::PackagesDiffer(differences) => Some(format!(
            "content differs: {}",
            bri_package::environment::describe(differences)
        )),
        _ => None,
    }
}
fn signed(hello: &mut Hello, key: &ClientIdentity, nonce: &[u8; 32], certificate: &[u8]) {
    let fingerprint: [u8; 32] = sha2::Sha256::digest(certificate).into();
    let transcript = identity_transcript(hello, nonce, &fingerprint).unwrap();
    hello.identity = Some(IdentityProof {
        public_key: *key.public_key(),
        signature: key.sign(&transcript).unwrap().to_vec(),
    });
}
/// Keep reading a raw joined connection's reliable stream so the host never
/// sees a stalled reader.
fn drain(mut receive: quinn::RecvStream) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while codec::read_frame(&mut receive, codec::MAX_FRAME)
            .await
            .is_ok()
        {}
    })
}
/// Process events for `duration`, keeping the replica current.
async fn pump(client: &mut Client, duration: Duration) -> Result<()> {
    let _ = tokio::time::timeout(duration, async {
        loop {
            client.receive().await?;
        }
        #[allow(unreachable_code)]
        anyhow::Ok(())
    })
    .await;
    Ok(())
}
/// Send a command and wait up to `limit` for its reply.
async fn command_within(client: &mut Client, command: Command, limit: Duration) -> Result<bool> {
    let sequence = client.request(command).await?;
    Ok(tokio::time::timeout(limit, async {
        loop {
            if let ClientEvent::Reply { sequence: s, .. } = client.receive().await?
                && s == sequence
            {
                return anyhow::Ok(());
            }
        }
    })
    .await
    .is_ok_and(|r| r.is_ok()))
}
fn resume_hello(
    nonce: &[u8; 32],
    token: ResumeToken,
    key: Option<&ClientIdentity>,
    certificate: &[u8],
) -> Hello {
    let mut h = Hello {
        resume: Some(token),
        ..hello("Ann")
    };
    if let Some(key) = key {
        signed(&mut h, key, nonce, certificate);
    }
    h
}
fn key(dir: &tempfile::TempDir, name: &str) -> ClientIdentity {
    ClientIdentity::load_or_create(dir.path().join(name)).unwrap()
}

// ------------------------------------------------------- codec (offline)

/// Inputs: empty, garbage, truncated zstd, zstd of valid MessagePack plus a
/// trailing byte, a frame one byte over MAX_FRAME, a zstd bomb expanding
/// past MAX_DECODED, oversized and garbage datagrams, a Request and a Hello
/// carrying forged extra fields.
#[test]
fn codec_rejects_garbage_truncated_trailing_oversized_and_forged_frames() {
    assert!(codec::decode::<Message>(&[]).is_err());
    assert!(codec::decode::<Message>(&[0xff; 64]).is_err());
    let valid = codec::encode(&Message::Rejected("x".into())).unwrap();
    assert!(codec::decode::<Message>(&valid).is_ok());
    assert!(codec::decode::<Message>(&valid[..valid.len() - 3]).is_err());
    let mut raw = codec::encode_request(&Message::Rejected("x".into()), codec::MAX_HELLO).unwrap();
    raw.push(0xc0);
    let trailing = zstd::stream::encode_all(raw.as_slice(), 1).unwrap();
    assert!(codec::decode::<Message>(&trailing).is_err());
    assert!(codec::decode::<Message>(&vec![0; codec::MAX_FRAME + 1]).is_err());
    // A tiny frame that expands past the decoded budget.
    let bomb = {
        use std::io::Write;
        let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 19).unwrap();
        let chunk = vec![0u8; 1 << 20];
        for _ in 0..(codec::MAX_DECODED >> 20) + 1 {
            encoder.write_all(&chunk).unwrap();
        }
        encoder.finish().unwrap()
    };
    assert!(bomb.len() < 1 << 20);
    assert!(codec::decode::<Message>(&bomb).is_err());

    assert!(codec::decode_datagram::<Movement>(&vec![0x90; MAX_DATAGRAM + 1]).is_err());
    assert!(codec::decode_datagram::<Movement>(&[0xde, 0xad, 0xbe, 0xef]).is_err());
    assert!(codec::decode_datagram::<Movement>(&[]).is_err());

    #[derive(Serialize)]
    struct ForgedRequest {
        sequence: u64,
        command: Command,
        aim: Option<bri_sim::session::ActionAim>,
        owner: u64,
        administrator: bool,
    }
    let forged = codec::encode_request(
        &ForgedRequest {
            sequence: 1,
            command: Command::Chat("hi".into()),
            aim: None,
            owner: 1,
            administrator: true,
        },
        codec::MAX_HELLO,
    )
    .unwrap();
    assert!(codec::decode_datagram::<Request>(&forged).is_err());
    #[derive(Serialize)]
    struct ForgedHello {
        version: u32,
        name: String,
        packages: Vec<bri_package::environment::PackageRef>,
        resume: Option<ResumeToken>,
        host: Option<ResumeToken>,
        identity: Option<IdentityProof>,
        administrator: bool,
    }
    let forged = codec::encode_request(
        &ForgedHello {
            version: VERSION,
            name: "x".into(),
            packages: Vec::new(),
            resume: None,
            host: None,
            identity: None,
            administrator: true,
        },
        codec::MAX_HELLO,
    )
    .unwrap();
    assert!(codec::decode_datagram::<Hello>(&forged).is_err());
}

/// Movement datagrams: empty inputs, MOVEMENT_REDUNDANCY + 1 inputs, newest
/// below the input count, a wrong version; newest = u64::MAX never wraps.
#[test]
fn movement_and_hello_shapes_are_validated() {
    let input = MoveInput::default();
    let movement = |newest, n, version| Movement {
        camera: None,
        version,
        newest,
        inputs: vec![input; n],
    };
    assert!(movement(10, 0, VERSION).validate().is_err());
    assert!(
        movement(10, MOVEMENT_REDUNDANCY + 1, VERSION)
            .validate()
            .is_err()
    );
    assert!(movement(2, 3, VERSION).validate().is_err());
    assert!(movement(10, 1, VERSION + 1).validate().is_err());
    let max = movement(u64::MAX, MOVEMENT_REDUNDANCY, VERSION);
    max.validate().unwrap();
    let sequences: Vec<u64> = max.sequenced().map(|(s, _)| s).collect();
    assert_eq!(sequences.len(), MOVEMENT_REDUNDANCY);
    assert!(sequences.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(*sequences.last().unwrap(), u64::MAX);

    for name in [
        String::new(),
        "  ".into(),
        "x".repeat(49),
        "a\nb".into(),
        "\u{7}".into(),
    ] {
        assert!(hello(&name).validate_bounds().is_err(), "{name:?}");
    }
    let mut h = hello("ok");
    h.packages = vec![shared_package("bad id!")];
    assert!(h.validate_bounds().is_err());
    let mut h = hello("ok");
    h.version = VERSION + 1;
    assert!(h.validate_bounds().is_err());
    let mut h = hello("ok");
    h.identity = Some(IdentityProof {
        public_key: [1; 32],
        signature: vec![0; 10],
    });
    assert!(h.validate_bounds().is_err());
    hello("x".repeat(48).as_str()).validate_bounds().unwrap();
}

// ------------------------------------------------- live host credentials

/// Raw handshakes: JoinBegin{VERSION+1}; Hello with an unknown resume
/// token; with a random host token; names "", 49 bytes, "a\nb"; a wrong
/// content id; a wrong Hello version; a 10-byte identity signature; a
/// signature over another nonce. Control: a clean Client joins.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forged_or_malformed_hellos_are_rejected() -> Result<()> {
    let server = server::start(session(), options())?;
    // The host refuses the version before any challenge. (It closes right
    // after writing the reason, so the reason itself may not arrive.)
    match raw_challenge(server.address, &server.certificate, VERSION + 1).await {
        Ok((_raw, first)) => {
            assert!(
                rejected(&first).is_some_and(|r| r.contains("version")),
                "{first:?}"
            )
        }
        Err(error) => eprintln!("wrong JoinBegin version: connection closed ({error:#})"),
    }

    type Forge = Box<dyn FnOnce(&[u8; 32]) -> Hello>;
    let cases: Vec<(&str, Forge)> = vec![
        (
            "Invalid resume credential",
            Box::new(|_| Hello {
                resume: Some(ResumeToken([7; 32])),
                ..hello("Forger")
            }),
        ),
        (
            "Invalid host credential",
            Box::new(|_| Hello {
                host: Some(ResumeToken([9; 32])),
                ..hello("Pretender")
            }),
        ),
        ("Invalid player name", Box::new(|_| hello(""))),
        ("Invalid player name", Box::new(|_| hello(&"x".repeat(49)))),
        ("Invalid player name", Box::new(|_| hello("a\nb"))),
        (
            "content",
            Box::new(|_| Hello {
                packages: vec![shared_package("other-content")],
                ..hello("Mismatch")
            }),
        ),
        (
            "version",
            Box::new(|_| Hello {
                version: VERSION + 1,
                ..hello("Old")
            }),
        ),
        (
            "signature",
            Box::new(|_| Hello {
                identity: Some(IdentityProof {
                    public_key: [3; 32],
                    signature: vec![0; 10],
                }),
                ..hello("Short sig")
            }),
        ),
    ];
    for (expected, make) in cases {
        let (_raw, answer) = raw_join(&server, make).await?;
        let reason = rejected(&answer).with_context(|| format!("accepted: expected {expected}"))?;
        assert!(
            reason.to_lowercase().contains(&expected.to_lowercase()),
            "{reason:?} (expected {expected})"
        );
    }
    // A proof signed over a different challenge.
    let dir = tempfile::tempdir()?;
    let identity = key(&dir, "replay.key");
    let certificate = server.certificate.clone();
    let (_raw, answer) = raw_join(&server, |nonce| {
        let mut other = *nonce;
        other[0] ^= 1;
        let mut h = hello("Replay");
        signed(&mut h, &identity, &other, &certificate);
        h
    })
    .await?;
    assert!(
        rejected(&answer).is_some_and(|r| r.contains("proof failed")),
        "{answer:?}"
    );

    let control = connect(&server, "Control").await?;
    assert!(!control.administrator);
    drop(control);
    let report = server.stop().await?;
    assert_eq!(report.joins, 1);
    Ok(())
}

/// Requests: A joins with key A (gets token T). While A is connected:
/// resume T with key A. A disconnects. Resume T with key B; resume T with
/// no key. A second fresh join with key A while A is connected (by design
/// allowed, but never an administrator). Control: resume T with key A.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_tickets_are_bound_to_identity_and_one_live_connection() -> Result<()> {
    let server = server::start(session(), options())?;
    let dir = tempfile::tempdir()?;
    let (key_a, key_b) = (key(&dir, "a.key"), key(&dir, "b.key"));
    let first = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Ann".into(),
        Vec::new(),
        None,
        None,
        &key_a,
    )
    .await?;
    let token = first.resume.clone();
    let owner = first.owner;
    let certificate = server.certificate.clone();
    let (_raw, answer) = raw_join(&server, |nonce| {
        resume_hello(nonce, token.clone(), Some(&key_a), &certificate)
    })
    .await?;
    assert!(
        rejected(&answer).is_some_and(|r| r.contains("still connected")),
        "{answer:?}"
    );
    // Same key, fresh join: a distinct, unprivileged owner.
    let twin = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Ann twin".into(),
        Vec::new(),
        None,
        None,
        &key_a,
    )
    .await?;
    assert_ne!(twin.owner, owner);
    assert!(!twin.administrator);
    drop(twin);
    drop(first);
    // Wait until the host has processed both departures.
    tokio::time::timeout(Duration::from_secs(5), async {
        while server.players.load(std::sync::atomic::Ordering::Relaxed) != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    let (_raw, answer) = raw_join(&server, |nonce| {
        resume_hello(nonce, token.clone(), Some(&key_b), &certificate)
    })
    .await?;
    assert!(
        rejected(&answer).is_some_and(|r| r.contains("identity")),
        "{answer:?}"
    );
    let (_raw, answer) = raw_join(&server, |nonce| {
        resume_hello(nonce, token.clone(), None, &certificate)
    })
    .await?;
    assert!(
        rejected(&answer).is_some_and(|r| r.contains("identity")),
        "{answer:?}"
    );
    let (_raw, answer) = raw_join(&server, |nonce| {
        resume_hello(nonce, token.clone(), Some(&key_a), &certificate)
    })
    .await?;
    match answer {
        Message::Welcome {
            owner: resumed,
            administrator,
            ..
        } => {
            assert_eq!(resumed, owner);
            assert!(!administrator);
        }
        other => panic!("control resume failed: {other:?}"),
    }
    server.stop().await?;
    Ok(())
}

// --------------------------------------------- live hostile traffic

/// A raw joined attacker sends datagrams: NaN / inf / out-of-range inputs,
/// seven inputs, zero inputs, newest below the count, a wrong version,
/// garbage bytes, then newest = u64::MAX with forward input and afterwards
/// newest = 10. An observer watches the attacker's replicated pose.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hostile_movement_datagrams_keep_the_attacker_finite_and_in_place() -> Result<()> {
    let server = server::start(session(), options())?;
    let mut observer = connect(&server, "Observer").await?;
    let (raw, answer) = raw_join(&server, |_| hello("Attacker")).await?;
    let Message::Welcome { owner, .. } = answer else {
        panic!("{answer:?}")
    };
    let _drain = drain(raw.receive);
    pump(&mut observer, Duration::from_millis(500)).await?;
    let start = Vec3::from(observer.replica.poses[&owner].player.feet);
    let bad = |f: fn(&mut MoveInput)| {
        let mut input = MoveInput::default();
        f(&mut input);
        input
    };
    let hostile_inputs = [
        bad(|i| i.forward = f32::NAN),
        bad(|i| i.right = f32::INFINITY),
        bad(|i| i.yaw = f32::NAN),
        bad(|i| i.pitch = 10.0),
        bad(|i| i.head_yaw = f32::NEG_INFINITY),
        bad(|i| i.forward = 1e30),
    ];
    let mut datagrams: Vec<Vec<u8>> = Vec::new();
    for (n, input) in hostile_inputs.iter().enumerate() {
        let movement = Movement {
            camera: None,
            version: VERSION,
            newest: 100 + n as u64,
            inputs: vec![*input; MOVEMENT_REDUNDANCY],
        };
        datagrams.push(codec::encode_datagram(&movement)?);
    }
    for movement in [
        Movement {
            camera: None,
            version: VERSION,
            newest: 200,
            inputs: vec![MoveInput::default(); MOVEMENT_REDUNDANCY + 1],
        },
        Movement {
            camera: None,
            version: VERSION,
            newest: 201,
            inputs: vec![],
        },
        Movement {
            camera: None,
            version: VERSION,
            newest: 1,
            inputs: vec![MoveInput::default(); 3],
        },
        Movement {
            camera: None,
            version: VERSION + 1,
            newest: 202,
            inputs: vec![MoveInput::default()],
        },
    ] {
        datagrams.push(codec::encode_request(&movement, MAX_DATAGRAM)?);
    }
    datagrams.push(vec![0xff; 64]);
    datagrams.push(Vec::new());
    for bytes in &datagrams {
        let _ = raw.connection.send_datagram(bytes.clone().into());
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    pump(&mut observer, Duration::from_millis(500)).await?;
    let pose = &observer.replica.poses[&owner];
    assert_eq!(pose.acknowledged_input, 0, "a hostile input was simulated");
    let feet = Vec3::from(pose.player.feet);
    assert!(feet.is_finite() && Vec3::from(pose.player.velocity).is_finite());
    assert!(feet.distance(start) < 0.1, "moved {feet} from {start}");

    // A saturated sequence: accepted once, then later sequences are stale.
    let forward = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    let saturated = Movement {
        camera: None,
        version: VERSION,
        newest: u64::MAX,
        inputs: vec![forward; MOVEMENT_REDUNDANCY],
    };
    raw.connection
        .send_datagram(codec::encode_datagram(&saturated)?.into())?;
    for newest in 10..40 {
        let later = Movement {
            camera: None,
            version: VERSION,
            newest,
            inputs: vec![forward; MOVEMENT_REDUNDANCY],
        };
        let _ = raw
            .connection
            .send_datagram(codec::encode_datagram(&later)?.into());
        tokio::time::sleep(Duration::from_millis(8)).await;
    }
    pump(&mut observer, Duration::from_millis(1500)).await?;
    let pose = &observer.replica.poses[&owner];
    let feet = Vec3::from(pose.player.feet);
    assert!(feet.is_finite() && Vec3::from(pose.player.velocity).is_finite());
    assert!(
        feet.distance(start) < 2.0,
        "six inputs moved {feet} from {start}"
    );
    // The host is still serving the observer.
    assert!(
        command_within(
            &mut observer,
            Command::Chat("still here".into()),
            Duration::from_secs(3)
        )
        .await?
    );
    server.stop().await?;
    Ok(())
}

/// Requests: a raw joined attacker writes (1) a Request frame carrying
/// forged owner/administrator fields, then on a second connection (2) a
/// length prefix followed by garbage bytes, then on a third (3) a zero
/// length prefix. A victim chats afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forged_or_garbage_request_frames_close_only_the_sender() -> Result<()> {
    let server = server::start(session(), options())?;
    let mut victim = connect(&server, "Victim").await?;
    #[derive(Serialize)]
    struct ForgedRequest {
        sequence: u64,
        command: Command,
        aim: Option<bri_sim::session::ActionAim>,
        owner: u64,
        administrator: bool,
    }
    let forged = codec::encode_request(
        &ForgedRequest {
            sequence: 1,
            command: Command::Chat("forged".into()),
            aim: None,
            owner: victim.owner,
            administrator: true,
        },
        codec::MAX_HELLO,
    )?;
    let mut garbage = 64_u32.to_le_bytes().to_vec();
    garbage.extend([0xc1; 64]);
    let payloads: Vec<Vec<u8>> = vec![
        {
            let mut frame = (forged.len() as u32).to_le_bytes().to_vec();
            frame.extend(&forged);
            frame
        },
        garbage,
        0_u32.to_le_bytes().to_vec(),
    ];
    for (n, payload) in payloads.into_iter().enumerate() {
        let (mut raw, answer) = raw_join(&server, |_| hello(&format!("Attacker {n}"))).await?;
        assert!(matches!(answer, Message::Welcome { .. }), "{answer:?}");
        raw.send.write_all(&payload).await?;
        let closed = tokio::time::timeout(Duration::from_secs(3), raw.connection.closed()).await;
        assert!(closed.is_ok(), "payload {n} did not close the sender");
    }
    assert!(
        command_within(
            &mut victim,
            Command::Chat("hello".into()),
            Duration::from_secs(3)
        )
        .await?
    );
    pump(&mut victim, Duration::from_millis(200)).await?;
    let chat: Vec<_> = victim.replica.chat.iter().map(|c| c.text.clone()).collect();
    assert_eq!(chat, vec!["hello".to_string()], "{chat:?}");
    server.stop().await?;
    Ok(())
}

/// Requests: two raw joined attackers each write a length prefix of
/// MAX_REQUEST (64 MiB) and then stall without a body; two more write a
/// PLAYER_MAX_REQUEST prefix and stall. A victim then sends Chat and must
/// get its reply promptly.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_oversized_request_bodies_do_not_starve_other_players() -> Result<()> {
    let server = server::start(session(), options())?;
    let mut victim = connect(&server, "Victim").await?;
    let mut attackers = Vec::new();
    for (n, length) in [
        codec::MAX_REQUEST,
        codec::MAX_REQUEST,
        codec::PLAYER_MAX_REQUEST,
        codec::PLAYER_MAX_REQUEST,
    ]
    .into_iter()
    .enumerate()
    {
        let (mut raw, answer) = raw_join(&server, |_| hello(&format!("Staller {n}"))).await?;
        assert!(matches!(answer, Message::Welcome { .. }));
        // One write: the host closes a player that announces a bulk-sized
        // request as soon as it reads the prefix, so a second write could
        // race that close and fail.
        let mut stall = (length as u32).to_le_bytes().to_vec();
        stall.extend([0x92; 16]);
        raw.send.write_all(&stall).await?;
        attackers.push(raw);
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        command_within(
            &mut victim,
            Command::Chat("unblocked".into()),
            Duration::from_secs(3)
        )
        .await?,
        "stalled attacker bodies starved the victim's commands"
    );
    drop(attackers);
    server.stop().await?;
    Ok(())
}

/// Requests: an attacker bound to 127.0.0.2 opens 80 QUIC connections and
/// never sends JoinBegin; a legitimate client from 127.0.0.1 then joins.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_pre_join_connections_from_one_source_cannot_lock_others_out() -> Result<()> {
    let server = server::start(session(), options())?;
    let Ok(endpoint) = raw_endpoint(&server.certificate, "127.0.0.2:0").await else {
        eprintln!("127.0.0.2 unavailable; skipping");
        return Ok(());
    };
    let mut held = Vec::new();
    for _ in 0..80 {
        if let Ok(Ok(connection)) = tokio::time::timeout(
            Duration::from_millis(500),
            endpoint.connect(server.address, "blockland.local")?,
        )
        .await
        {
            held.push(connection);
        }
    }
    eprintln!("attacker holds {} pre-join connections", held.len());
    let joined = tokio::time::timeout(Duration::from_secs(5), connect(&server, "Legit")).await;
    assert!(
        matches!(joined, Ok(Ok(_))),
        "a legitimate join was locked out by idle connections"
    );
    drop(held);
    server.stop().await?;
    Ok(())
}

/// Requests: guest Login("wrong") x3 (each answered with its attempt
/// count), a fourth Login("wrong"); the host must close the connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn fourth_failed_login_closes_the_connection_end_to_end() -> Result<()> {
    use bri_admin::{Action, Request as AdminRequest, Secret};
    let server = server::start(session(), options())?;
    // Password login needs a durable identity (failed guesses follow it).
    let dir = tempfile::tempdir()?;
    let identity =
        bri_identity::ClientIdentity::load_or_create(dir.path().join("client.identity"))?;
    let mut guest = Client::connect_with_identity(
        server.address,
        &server.certificate,
        "Guesser".into(),
        Vec::new(),
        None,
        None,
        &identity,
    )
    .await?;
    let wrong = || {
        Command::Admin(AdminRequest::new(Action::Login {
            password: Secret::new("wrong".into()).unwrap(),
        }))
    };
    for _ in 0..3 {
        guest.command(wrong()).await?;
    }
    guest.request(wrong()).await?;
    let closed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if guest.receive().await.is_err() {
                return;
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "the locked guesser stayed connected");
    server.stop().await?;
    Ok(())
}
