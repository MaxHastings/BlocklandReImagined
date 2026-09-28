//! Net soak: two real QUIC clients play through an impaired link (latency,
//! jitter, 5% loss, duplication and reordering both ways) while moving every
//! tick and chatting. Nothing may disconnect, every command must be answered,
//! both replicas must agree, and movement must keep being acknowledged.
//! `BRI_SOAK_SECONDS` lengthens the run (default 10), `BRI_SOAK_SEED` replays
//! one exact impairment pattern.
use anyhow::{Context, Result, ensure};
use bri_net::{
    client::{Client, ClientEvent},
    impair::{ImpairedLink, Impairment},
    protocol::MOVEMENT_REDUNDANCY,
    server::{self, ServerOptions},
};
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Command, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    collections::VecDeque,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

fn session() -> Result<Session> {
    Ok(Session::new(Simulation::new(
        World::new("Soak".into(), "fixture".into(), vec![[1.0; 4]]),
        Definitions::default(),
        vec![ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )?))
}

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// One simulated player: sends a tick's input every 1/120 s with redundancy,
/// chats once a second, and tracks what came back.
struct Player {
    client: Client,
    sequence: u64,
    recent: VecDeque<MoveInput>,
    pending: Vec<(u64, Instant)>,
    answered: u64,
    worst_reply: Duration,
}
impl Player {
    fn step(&mut self, tick: u64) -> Result<()> {
        let angle = tick as f32 * 0.01;
        let input = MoveInput {
            forward: angle.sin(),
            right: angle.cos() * 0.5,
            yaw: (angle % std::f32::consts::TAU) - std::f32::consts::PI,
            ..Default::default()
        };
        self.sequence += 1;
        self.recent.push_back(input);
        if self.recent.len() > MOVEMENT_REDUNDANCY {
            self.recent.pop_front();
        }
        let recent: Vec<_> = self.recent.iter().copied().collect();
        self.client.movement(self.sequence, &recent)
    }
    async fn chat(&mut self, text: String) -> Result<()> {
        let sequence = self.client.request(Command::Chat(text)).await?;
        self.pending.push((sequence, Instant::now()));
        Ok(())
    }
    /// Handle everything that has already arrived.
    async fn drain(&mut self) -> Result<()> {
        // A zero timeout still polls once: take what is ready, never wait.
        while let Ok(event) = tokio::time::timeout(Duration::ZERO, self.client.receive()).await
        {
            if let ClientEvent::Reply { sequence, result } = event? {
                result.map_err(anyhow::Error::msg)?;
                let index = self
                    .pending
                    .iter()
                    .position(|(s, _)| *s == sequence)
                    .context("Reply to an unknown request")?;
                let (_, sent) = self.pending.remove(index);
                self.worst_reply = self.worst_reply.max(sent.elapsed());
                self.answered += 1;
            }
        }
        Ok(())
    }
    fn acknowledged(&self) -> u64 {
        self.client
            .replica
            .poses
            .get(&self.client.owner)
            .map_or(0, |p| p.acknowledged_input)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_players_stay_consistent_through_a_lossy_jittery_link() -> Result<()> {
    let seconds = env("BRI_SOAK_SECONDS", 10);
    let seed = env("BRI_SOAK_SEED", 0x5eed);
    let server = server::start(
        session()?,
        ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment: bri_package::environment::Environment::empty(),
            spawn_points: vec![Vec3::new(0.0, 0.05, 0.0), Vec3::new(3.0, 0.05, 0.0)],
            certificate: None,
            map_loader: None,
            autosave: None,
        },
    )?;
    let link = ImpairedLink::start(server.address, Impairment::BAD_WIFI, seed).await?;
    let mut players = Vec::new();
    for name in ["Alpha", "Bravo"] {
        let client =
            Client::connect(link.address, &server.certificate, name.into(), Vec::new(), None)
                .await?;
        players.push(Player {
            client,
            sequence: 0,
            recent: VecDeque::new(),
            pending: Vec::new(),
            answered: 0,
            worst_reply: Duration::ZERO,
        });
    }
    // Like the real client: every 120 Hz tick that is due runs, however
    // coarse the wakeups, so input rate matches the prediction clock.
    let start = Instant::now();
    let mut tick = 0_u64;
    let mut chats = 0_u64;
    while start.elapsed() < Duration::from_secs(seconds) {
        let due = (start.elapsed().as_secs_f64() * 120.0) as u64;
        while tick < due {
            tick += 1;
            for player in &mut players {
                player.step(tick)?;
            }
            if tick.is_multiple_of(120) {
                for (i, player) in players.iter_mut().enumerate() {
                    player.chat(format!("soak {i} {tick}")).await?;
                }
                chats += 1;
            }
        }
        for player in &mut players {
            player.drain().await?;
        }
        tokio::time::sleep(Duration::from_millis(4)).await;
    }
    let expected = seconds * 120;
    ensure!(tick + 12 >= expected, "Soak loop ran {tick} of {expected} ticks");
    // Let the last replies and replication arrive.
    let settle = Instant::now();
    let all_chat = |p: &Player| p.client.replica.chat.len() as u64 >= 2 * chats.min(50);
    while settle.elapsed() < Duration::from_secs(10)
        && players.iter().any(|p| !p.pending.is_empty() || !all_chat(p))
    {
        for player in &mut players {
            player.drain().await?;
        }
        tokio::time::sleep(Duration::from_millis(4)).await;
    }
    for player in &players {
        ensure!(
            player.pending.is_empty() && player.answered == chats,
            "{} of {chats} chat commands answered",
            player.answered
        );
        ensure!(
            player.client.replica.names.len() == 2,
            "Each client sees both players"
        );
        // Redundant inputs absorb the loss: acknowledgement trails what was
        // sent by at most the link's delay, never by lost inputs piling up.
        let behind = player.sequence - player.acknowledged();
        ensure!(behind < 120, "Movement acknowledgement fell {behind} inputs behind");
        // Random loss is not congestion: with BBR a reply costs about one
        // retransmission (Cubic took 3-8 s on this link, collapsing its window).
        ensure!(
            player.worst_reply < Duration::from_millis(2500),
            "A command took {:?} through a playable link",
            player.worst_reply
        );
        eprintln!(
            "{}: {} inputs, acknowledged {}, worst reply {:?}, rtt {:?}",
            player.client.replica.names[&player.client.owner],
            player.sequence,
            player.acknowledged(),
            player.worst_reply,
            player.client.rtt()
        );
    }
    // The host keeps publishing every 50 ms, so replicas are compared on
    // what both have fully received: the whole chat history.
    let (a, b) = (&players[0].client.replica, &players[1].client.replica);
    ensure!(
        a.chat.iter().map(|c| &c.text).eq(b.chat.iter().map(|c| &c.text)),
        "Replicas disagree on chat"
    );
    ensure!(a.cursor.abs_diff(b.cursor) <= 40, "One replica fell far behind");
    let dropped = link.stats.dropped.load(Ordering::Relaxed);
    let forwarded = link.stats.forwarded.load(Ordering::Relaxed);
    eprintln!("link: {forwarded} forwarded, {dropped} dropped, seed {seed:#x}");
    ensure!(dropped > 0, "The link must actually impair");
    drop(players);
    server.stop().await?;
    Ok(())
}
