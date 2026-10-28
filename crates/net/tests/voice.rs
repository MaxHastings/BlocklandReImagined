//! Proximity voice over real QUIC: a frame reaches the players within earshot
//! of the speaker, stamped with where the host says the speaker stands, and
//! nobody else; a flood is cut to the allowance.
mod common;
use anyhow::Result;
use bri_net::{
    client::Client,
    server::{self, ServerOptions},
    voice::{self, Heard, VoiceLink},
};
use glam::Vec3;
use std::time::{Duration, Instant};

const _: () = assert!(voice::RANGE < 95.0, "the far player is out of range");

/// A host running voice chat: its Add-On installed before players join,
/// as the game's defaults do. The package is empty — the engine owns the
/// relay; turning it off is what stops one.
fn voice_session() -> bri_sim::session::Session {
    use bri_package::packages::{PackageEntry, PackageSet, Side};
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "bri-net-voice-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let dir = root.join("voice_chat");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{"schema_version":1,"id":"voice_chat","version":"1.0.0","api":1,
            "name":"Voice Chat","license":"CC0-1.0","provides":[]}"#,
    )
    .unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "voice_chat".into(),
            version: "1.0.0".into(),
            side: Side::Shared,
            dir: "voice_chat".into(),
            role: None,
        }],
    };
    let catalog = bri_package_runtime::Catalog::load(&root, &set, true).unwrap();
    let mut session = common::session();
    session
        .install_packages(std::sync::Arc::new(catalog), None)
        .unwrap();
    session
}

/// Speaker at the origin, a listener 10 units away, another 95 away.
fn options() -> ServerOptions {
    ServerOptions {
        spawn_points: vec![
            Vec3::new(0.0, 0.05, 0.0),
            Vec3::new(10.0, 0.05, 0.0),
            Vec3::new(95.0, 0.05, 0.0),
        ],
        ..common::options()
    }
}

async fn connect(server: &server::ServerHandle, name: &str) -> Result<Client> {
    Client::connect(
        server.address,
        &server.certificate,
        name.into(),
        Vec::new(),
        None,
    )
    .await
}

/// Frames `link` hears over the next `wait`, stopping early at `want`.
async fn hear(link: &VoiceLink, want: usize, wait: Duration) -> Vec<Heard> {
    let mut heard = Vec::new();
    let end = Instant::now() + wait;
    while heard.len() < want && Instant::now() < end {
        link.take_heard(&mut heard);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    heard
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_without_the_voice_addon_relays_nothing() -> Result<()> {
    // The plain test session runs no Add-Ons: the host relays no voices.
    let server = server::start(common::session(), options())?;
    let speaker = connect(&server, "Speaker").await?;
    let near = connect(&server, "Near").await?;
    let (talk, listen) = (speaker.voice_link(), near.voice_link());
    talk.send(0, &[1; 163])?;
    assert!(
        hear(&listen, 1, Duration::from_millis(500))
            .await
            .is_empty(),
        "no voice without the Add-On"
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn voice_reaches_only_the_players_within_range() -> Result<()> {
    let server = server::start(voice_session(), options())?;
    let speaker = connect(&server, "Speaker").await?;
    let near = connect(&server, "Near").await?;
    let far = connect(&server, "Far").await?;
    let (talk, near_link, far_link) = (speaker.voice_link(), near.voice_link(), far.voice_link());
    let mut heard = Vec::new();
    for sequence in 0..5 {
        talk.send(sequence, &[sequence as u8 + 1; 163])?;
        tokio::time::sleep(Duration::from_millis(20)).await;
        near_link.take_heard(&mut heard);
    }
    heard.extend(hear(&near_link, 5 - heard.len(), Duration::from_secs(3)).await);
    assert_eq!(heard.len(), 5, "the player 10 units away hears every frame");
    for (i, frame) in heard.iter().enumerate() {
        assert_eq!(frame.speaker, speaker.owner);
        assert_eq!(frame.sequence, i as u16);
        assert_eq!(frame.payload, vec![i as u8 + 1; 163]);
        // The host's own view of the speaker, not anything the client said.
        assert!(frame.position[0].abs() < 1.0 && frame.position[2].abs() < 1.0);
    }
    assert!(
        hear(&far_link, 1, Duration::from_millis(300))
            .await
            .is_empty(),
        "a player {} units away is out of earshot",
        95
    );
    let mut own = Vec::new();
    talk.take_heard(&mut own);
    assert!(own.is_empty(), "nobody hears themselves");
    // Range works both ways: the far player's voice reaches nobody either.
    far_link.send(0, &[1; 163])?;
    assert!(
        hear(&near_link, 1, Duration::from_millis(300))
            .await
            .is_empty()
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_voice_flood_is_cut_to_the_allowance_and_nonsense_is_ignored() -> Result<()> {
    let server = server::start(voice_session(), options())?;
    let speaker = connect(&server, "Speaker").await?;
    let near = connect(&server, "Near").await?;
    let (talk, listen) = (speaker.voice_link(), near.voice_link());
    for sequence in 0..400u16 {
        talk.send(sequence, &[7; 163])?;
    }
    // Oversized and empty frames never leave the client.
    assert!(talk.send(0, &[]).is_err());
    assert!(talk.send(0, &vec![0; voice::MAX_PAYLOAD + 1]).is_err());
    let heard = hear(&listen, 400, Duration::from_millis(800)).await;
    assert!(!heard.is_empty(), "a burst gets through");
    assert!(
        heard.len() < 100,
        "{} of 400 frames passed a flood allowance",
        heard.len()
    );
    server.stop().await?;
    Ok(())
}

/// Exercise the actual encrypted transport with all 32 clients transmitting,
/// not only a 32-entry data structure. Drain every render interval as clients do.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thirty_two_simultaneous_speakers_preserve_identity_and_game_commands() -> Result<()> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut config = common::options();
    config.spawn_points = (0..32)
        .map(|i| Vec3::new((i % 8) as f32 * 2.0, 0.05, (i / 8) as f32 * 2.0))
        .collect();
    let server = server::start_with_limit(voice_session(), config, 32)?;
    let mut clients = Vec::new();
    for i in 0..32 {
        clients.push(connect(&server, &format!("Speaker{i}")).await?);
    }
    let links: Vec<_> = clients.iter().map(Client::voice_link).collect();
    let owners: BTreeSet<_> = clients.iter().map(|client| client.owner).collect();
    let pcm = std::array::from_fn(|i| {
        ((i as f32 * 0.173).sin() * 9000.0 + (i as f32 * 0.669).sin() * 3000.0) as i16
    });
    let payload = bri_audio::voice::Codec::encode(&pcm);
    assert!(bri_audio::voice::Codec::valid(&payload));
    let mut heard = vec![BTreeMap::<u64, usize>::new(); 32];
    let mut interval = tokio::time::interval(Duration::from_millis(20));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    for sequence in 0..60 {
        interval.tick().await;
        for link in &links {
            link.send(sequence, &payload)?;
        }
        for (index, link) in links.iter().enumerate() {
            let mut frames = Vec::new();
            link.take_heard(&mut frames);
            for frame in frames {
                assert_ne!(frame.speaker, clients[index].owner, "no self echo");
                assert!(owners.contains(&frame.speaker), "host-stamped identity");
                assert_eq!(frame.payload, payload);
                *heard[index].entry(frame.speaker).or_default() += 1;
            }
        }
        if sequence == 30 {
            // A reliable gameplay operation must still complete while voices
            // saturate the 32-player fanout. Its ordinary reply is authoritative.
            let reply = tokio::time::timeout(
                Duration::from_secs(3),
                clients[0].command(bri_sim::session::Command::Chat("Voice load control".into())),
            )
            .await??;
            assert!(
                matches!(reply, bri_sim::session::Reply::Accepted),
                "{reply:?}"
            );
        }
    }
    // Observe delivery rather than assert fragile wall-clock performance.
    for (index, heard) in heard.iter().enumerate() {
        assert_eq!(
            heard.len(),
            31,
            "listener {index} hears every remote speaker"
        );
        assert!(
            heard.values().all(|count| *count >= 30),
            "listener {index}: {heard:?}"
        );
    }
    eprintln!(
        "32-player voice: {} relayed packets observed; payload {} bytes; 60 frames each",
        heard
            .iter()
            .flat_map(|sources| sources.values())
            .sum::<usize>(),
        payload.len()
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn voice_recovers_over_loss_jitter_and_duplication_without_blocking_chat() -> Result<()> {
    use bri_net::impair::{ImpairedLink, Impairment};
    let server = server::start(voice_session(), options())?;
    let speaker = connect(&server, "Speaker").await?;
    let impaired = ImpairedLink::start(server.address, Impairment::BAD_WIFI, 42).await?;
    let mut listener = Client::connect(
        impaired.address,
        &server.certificate,
        "Listener".into(),
        vec![],
        None,
    )
    .await?;
    let talk = speaker.voice_link();
    let listen = listener.voice_link();
    let mut interval = tokio::time::interval(Duration::from_millis(20));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut heard = Vec::new();
    for sequence in 0..100 {
        interval.tick().await;
        talk.send(sequence, &[5; 60])?;
        listen.take_heard(&mut heard);
    }
    assert!(
        heard.len() >= 50,
        "received {} frames under simulated bad Wi-Fi",
        heard.len()
    );
    let reply = tokio::time::timeout(
        Duration::from_secs(5),
        listener.command(bri_sim::session::Command::Chat("Still playing".into())),
    )
    .await??;
    assert!(
        matches!(reply, bri_sim::session::Reply::Accepted),
        "{reply:?}"
    );
    server.stop().await?;
    Ok(())
}
