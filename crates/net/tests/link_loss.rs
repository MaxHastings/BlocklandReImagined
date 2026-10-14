//! The packet loss the net graph shows (Ctrl+N) is the transport's count of
//! this client's packets declared lost. Jitter must not read as loss, and
//! real loss must read near its rate.
mod common;
use anyhow::Result;
use bri_net::{
    client::Client,
    impair::{ImpairedLink, Impairment},
    server,
};
use bri_sim::player::MoveInput;
use std::time::Duration;

/// Percent of the client's packets declared lost while it sends input as
/// the game does (two inputs a frame at 60 fps) over `impairment`.
async fn shown_loss(impairment: Impairment) -> Result<f64> {
    let world = bri_world::World::new("Loss".into(), "fixture".into(), vec![[1.0; 4], [0.0; 4]]);
    let server = server::start(common::session_with(world), common::options())?;
    let link = ImpairedLink::start(server.address, impairment, 7).await?;
    let mut client = Client::connect(
        link.address,
        &server.certificate,
        "Player".into(),
        Vec::new(),
        None,
    )
    .await?;
    let probe = client.link_probe();
    let mut frame = tokio::time::interval(Duration::from_micros(16_667));
    let started = tokio::time::Instant::now();
    let (measure_at, end) = (
        started + Duration::from_secs(2),
        started + Duration::from_secs(8),
    );
    let (mut sequence, mut history, mut first) = (0, std::collections::VecDeque::new(), None);
    loop {
        tokio::select! {
            _ = frame.tick() => {
                let now = tokio::time::Instant::now();
                if now >= end {
                    break;
                }
                if first.is_none() && now >= measure_at {
                    first = Some(probe.sample());
                }
                for _ in 0..2 {
                    sequence += 1;
                    history.push_back(MoveInput::default());
                    if history.len() > bri_net::protocol::MOVEMENT_REDUNDANCY {
                        history.pop_front();
                    }
                }
                let inputs: Vec<_> = history.iter().copied().collect();
                client.movement(sequence, &inputs, None, None)?;
            }
            event = client.receive() => { event?; }
        }
    }
    let (first, last) = (first.unwrap(), probe.sample());
    let sent = last.sent_packets - first.sent_packets;
    let lost = last.lost_packets - first.lost_packets;
    Ok(lost as f64 * 100.0 / sent as f64)
}

/// 60 ms each way with 40 ms of jitter and nothing dropped showed 10% loss
/// with QUIC's default loss timing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jitter_alone_is_not_shown_as_loss() -> Result<()> {
    let jitter = Impairment {
        latency: Duration::from_millis(60),
        jitter: Duration::from_millis(40),
        ..Impairment::NONE
    };
    let shown = shown_loss(jitter).await?;
    assert!(
        shown < 2.0,
        "a jittery link without loss shows {shown:.1}% loss"
    );
    Ok(())
}

/// 5% loss each way showed 13% with QUIC's default loss timing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_loss_is_shown_near_its_rate() -> Result<()> {
    let shown = shown_loss(Impairment::BAD_WIFI).await?;
    assert!(
        (1.5..9.0).contains(&shown),
        "5% loss each way shows {shown:.1}% loss"
    );
    Ok(())
}
