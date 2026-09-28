//! Chaos over the wire: a real host (`bri_net::server`) on loopback and bot
//! clients connecting over QUIC, doing everything at once through the same
//! brains as [`crate::local`], joining and leaving as they go. Everything a
//! client receives is checked for NaN, nobody may be dropped unasked, and
//! the host must still answer a newcomer at the end.
use crate::{
    bots::{Bot, Catalog, Rng, View},
    fixture::Fixture,
    local::catalog,
    scan::ensure_finite,
};
use anyhow::{Context, Result, bail, ensure};
use bri_net::{
    client::{Client, ClientEvent},
    protocol::{MOVEMENT_REDUNDANCY, ResumeToken},
    server::{self, ServerHandle, ServerOptions},
};
use bri_package::environment::PackageRef;
use bri_sim::{player::MoveInput, session::Command};
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Options {
    pub seed: u64,
    pub seconds: u64,
    pub bots: usize,
    /// Bots that connect with the host's credential (load, clear, warp).
    pub administrators: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            seed: 1,
            seconds: 10,
            bots: 5,
            administrators: 2,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub ticks: u64,
    pub commands: u64,
    /// Commands the host refused, by reason (numbers stripped).
    pub rejected: BTreeMap<String, u64>,
    /// Commands the client refused to send (hostile aims, oversized requests).
    pub refused_locally: u64,
    pub joins: u64,
    pub leaves: u64,
    pub most_bricks: usize,
    pub most_vehicles: usize,
    pub most_projectiles: usize,
    pub checks: u64,
    /// Presentation cues clients received, by kind.
    pub cues: BTreeMap<String, u64>,
    /// Contained per-system failures the host reported when it stopped.
    pub server_step_errors: u64,
}

struct Remote {
    bot: Bot,
    client: Client,
    sequence: u64,
    recent: VecDeque<MoveInput>,
}

/// What a bot sees, from its client's replica.
fn view(client: &Client) -> View {
    let replica = &client.replica;
    let me = client.owner;
    View {
        me,
        feet: replica.poses.get(&me).map(|p| p.player.feet.into()),
        alive: replica.vitals.get(&me).is_none_or(|v| v.alive),
        mounted: replica
            .vehicles
            .values()
            .any(|v| v.occupants.contains(&Some(me))),
        bricks: replica
            .world
            .bricks
            .iter()
            .take(4096)
            .map(|(id, b)| {
                let definition = match &b.definition {
                    bri_world::ContentRef::Resolved(id) => id.clone(),
                    _ => String::new(),
                };
                (*id, b.position.into(), definition)
            })
            .collect(),
        vehicles: replica
            .vehicle_poses
            .values()
            .map(|p| p.position.into())
            .collect(),
        players: replica
            .poses
            .iter()
            .map(|(owner, p)| (*owner, p.player.feet.into()))
            .collect(),
    }
}

/// Everything a client draws from, as it holds it now.
pub fn check_replica(client: &Client, world: bool) -> Result<()> {
    let r = &client.replica;
    ensure_finite("replica weapons", &r.weapons)?;
    ensure_finite("replica poses", &r.poses)?;
    ensure_finite("replica vitals", &r.vitals)?;
    ensure_finite("replica vehicle poses", &r.vehicle_poses)?;
    ensure_finite("replica orbs", &r.orbs)?;
    ensure_finite("replica entities", &r.entities)?;
    ensure_finite("replica minigames", &r.minigames)?;
    ensure!(
        r.time_scale.is_finite(),
        "replica time scale {}",
        r.time_scale
    );
    for owner in r.poses.keys() {
        if let Some(pose) = r.interpolated(*owner, r.tick as f64 - 2.5) {
            ensure_finite("interpolated pose", &pose)?;
        }
    }
    if world {
        ensure_finite("replica world", &r.world)?;
    }
    Ok(())
}

pub struct NetChaos {
    server: ServerHandle,
    catalog: Catalog,
    packages: Vec<PackageRef>,
    remotes: Vec<Remote>,
    rng: Rng,
    names: u64,
    options: Options,
    pub report: Report,
    pub log: VecDeque<String>,
}

impl NetChaos {
    pub async fn start(fixture: Fixture, options: Options) -> Result<Self> {
        let catalog = catalog(&fixture);
        let packages = fixture.environment.packages.clone();
        let mut session = fixture.session;
        let mut settings = session.server_settings().clone();
        settings.bricks_per_second = 1000;
        session.set_server_settings(settings)?;
        let server = server::start(
            session,
            ServerOptions {
                bind: "127.0.0.1:0".parse()?,
                environment: fixture.environment,
                spawn_points: fixture.spawn_points,
                certificate: None,
                map_loader: None,
                autosave: None,
                packages: None,
            },
        )?;
        let mut chaos = Self {
            server,
            catalog,
            packages,
            remotes: Vec::new(),
            rng: Rng::new(options.seed),
            names: 0,
            options,
            report: Report::default(),
            log: VecDeque::new(),
        };
        for i in 0..chaos.options.bots {
            chaos.join(i < chaos.options.administrators).await?;
        }
        Ok(chaos)
    }

    fn note(&mut self, line: String) {
        if self.log.len() >= 64 {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    async fn connect(&self, name: String, administrator: bool) -> Result<Client> {
        let host: Option<ResumeToken> = administrator.then(|| self.server.host_token.clone());
        Client::connect_with_host(
            self.server.address,
            &self.server.certificate,
            name,
            self.packages.clone(),
            None,
            host,
        )
        .await
    }

    async fn join(&mut self, administrator: bool) -> Result<()> {
        self.names += 1;
        let client = self
            .connect(format!("Remote{}", self.names), administrator)
            .await
            .context("A bot could not join")?;
        let seed = self.rng.next_u64();
        self.note(format!("join {} admin={administrator}", client.owner));
        self.remotes.push(Remote {
            bot: Bot::new(client.owner, administrator, seed),
            client,
            sequence: 0,
            recent: VecDeque::new(),
        });
        self.report.joins += 1;
        Ok(())
    }

    /// One client tick for every bot: movement, maybe a command.
    async fn tick(&mut self) -> Result<()> {
        for remote in &mut self.remotes {
            let view = view(&remote.client);
            let input = remote.bot.movement(&view);
            remote.sequence += 1;
            remote.recent.push_back(input);
            if remote.recent.len() > MOVEMENT_REDUNDANCY {
                remote.recent.pop_front();
            }
            let recent: Vec<_> = remote.recent.iter().copied().collect();
            // A hostile input is refused before it is sent; a dropped
            // connection shows up in `drain`.
            let _ = remote.client.movement(remote.sequence, &recent, None);
            if let Some((command, aim)) = remote.bot.command(&view, &self.catalog) {
                let summary: String = format!("{command:?}").chars().take(160).collect();
                let line = format!("{} {summary} aim={aim:?}", remote.client.owner);
                if self.log.len() >= 64 {
                    self.log.pop_front();
                }
                self.log.push_back(line);
                self.report.commands += 1;
                if remote.client.request_with_aim(command, aim).await.is_err() {
                    self.report.refused_locally += 1;
                }
            }
        }
        self.report.ticks += 1;
        Ok(())
    }

    /// Take everything that has arrived. A client whose connection failed
    /// was dropped by the host: that is a failure unless an administrator
    /// kicked it.
    async fn drain(&mut self) -> Result<()> {
        let mut lost = Vec::new();
        for (i, remote) in self.remotes.iter_mut().enumerate() {
            loop {
                match tokio::time::timeout(Duration::ZERO, remote.client.receive()).await {
                    Err(_) => break,
                    Ok(Ok(ClientEvent::Reply {
                        result: Err(rejection),
                        ..
                    })) => {
                        let reason: String = format!("{rejection:?}")
                            .chars()
                            .filter(|c| !c.is_ascii_digit())
                            .take(80)
                            .collect();
                        *self.report.rejected.entry(reason).or_default() += 1;
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => {
                        lost.push((i, format!("{error:#}")));
                        break;
                    }
                }
            }
        }
        for remote in &mut self.remotes {
            let fired = remote.client.replica.weapons.fired().count();
            self.report.most_projectiles = self.report.most_projectiles.max(fired);
            let cues = remote.client.replica.take_cues();
            ensure_finite("replica cues", &cues)?;
            for cue in cues {
                let kind = format!("{:?}", cue.kind);
                let kind = kind.split([' ', '(', '{']).next().unwrap_or("").to_string();
                *self.report.cues.entry(kind).or_default() += 1;
            }
        }
        for (i, error) in lost.into_iter().rev() {
            let remote = self.remotes.remove(i);
            let owner = remote.client.owner;
            let kicked = error.contains("kick") || error.contains("Kick") || error.contains("ban");
            self.note(format!("lost {owner}: {error}"));
            ensure!(kicked, "client {owner} was dropped: {error}");
            self.report.leaves += 1;
        }
        Ok(())
    }

    async fn churn(&mut self) -> Result<()> {
        let players = self.options.administrators;
        if self.rng.chance(1.0 / 600.0) && self.remotes.len() > players {
            let index = players + self.rng.below(self.remotes.len() - players);
            let remote = self.remotes.remove(index);
            self.note(format!("leave {}", remote.client.owner));
            remote.client.close();
            self.report.leaves += 1;
        }
        if self.remotes.len() < self.options.bots && self.rng.chance(1.0 / 300.0) {
            self.join(false).await?;
        }
        Ok(())
    }

    fn check(&mut self, world: bool) -> Result<()> {
        for remote in &self.remotes {
            check_replica(&remote.client, world)?;
            let r = &remote.client.replica;
            self.report.most_bricks = self.report.most_bricks.max(r.world.bricks.len());
            self.report.most_vehicles = self.report.most_vehicles.max(r.vehicle_poses.len());
        }
        self.report.checks += 1;
        Ok(())
    }

    async fn play(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut tick = 0_u64;
        let mut checked = Instant::now();
        let mut world_checked = Instant::now();
        while start.elapsed() < Duration::from_secs(self.options.seconds) {
            // Every 120 Hz client tick that is due runs, as in the game.
            let due = (start.elapsed().as_secs_f64() * 120.0) as u64;
            while tick < due {
                tick += 1;
                self.churn().await?;
                self.tick().await?;
            }
            self.drain().await?;
            if checked.elapsed() > Duration::from_millis(250) {
                let world = world_checked.elapsed() > Duration::from_secs(2);
                self.check(world)?;
                checked = Instant::now();
                if world {
                    world_checked = Instant::now();
                }
            }
            tokio::time::sleep(Duration::from_millis(4)).await;
        }
        Ok(())
    }

    /// Run for `seconds`, prove the host still serves a newcomer, then stop
    /// it. Failures carry the seed and the last actions.
    pub async fn run(mut self) -> Result<Report> {
        let seed = self.options.seed;
        let outcome = match self.play().await {
            Ok(()) => self.finish().await,
            Err(error) => Err(error),
        };
        let Self {
            server,
            mut report,
            log,
            remotes,
            ..
        } = self;
        drop(remotes);
        let stopped = server.stop().await;
        let outcome = outcome.and_then(|()| {
            let host = stopped?;
            report.server_step_errors = host.step_errors;
            Ok(())
        });
        match outcome {
            Ok(()) => Ok(report),
            Err(error) => {
                let log: Vec<_> = log.into_iter().collect();
                bail!(
                    "net chaos seed {seed:#x} failed after {} ticks: {error:#}\nlast actions:\n  {}",
                    report.ticks,
                    log.join("\n  ")
                )
            }
        }
    }

    async fn finish(&mut self) -> Result<()> {
        let mut newcomer = self
            .connect("Newcomer".into(), false)
            .await
            .context("The host refused a newcomer after the soak")?;
        tokio::time::timeout(
            Duration::from_secs(10),
            newcomer.command(Command::Chat("still here?".into())),
        )
        .await
        .context("The host did not answer a newcomer after the soak")?
        .context("The host refused a newcomer's chat")?;
        check_replica(&newcomer, true)?;
        newcomer.close();
        for remote in self.remotes.drain(..) {
            remote.client.close();
        }
        Ok(())
    }
}
