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
async fn voice_reaches_only_the_players_within_range() -> Result<()> {
    let server = server::start(common::session(), options())?;
    let speaker = connect(&server, "Speaker").await?;
    let near = connect(&server, "Near").await?;
    let far = connect(&server, "Far").await?;
    let (talk, near_link, far_link) = (speaker.voice_link(), near.voice_link(), far.voice_link());
    for sequence in 0..5 {
        talk.send(sequence, &[sequence as u8 + 1; 163])?;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let heard = hear(&near_link, 5, Duration::from_secs(3)).await;
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
    let server = server::start(common::session(), options())?;
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
