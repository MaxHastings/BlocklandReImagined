//! Chaos inside one process: bots drive a [`Session`] directly, tick by
//! tick, and everything the host would replicate is checked for NaN as it
//! goes. Deterministic for a seed, so a failure replays exactly.
use crate::{
    bots::{Bot, BrickKind, Catalog, Rng, View},
    fixture::Fixture,
    scan::ensure_finite,
};
use anyhow::{Result, bail};
use bri_sim::session::{Reply, Session};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct Options {
    pub seed: u64,
    pub ticks: u64,
    pub bots: usize,
    /// How many of the bots are administrators (they load builds, clear, warp).
    pub administrators: usize,
    /// Check replicated state every this many ticks.
    pub check_every: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            seed: 1,
            ticks: 1200,
            bots: 6,
            administrators: 2,
            check_every: 6,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub ticks: u64,
    pub commands: u64,
    pub rejected: u64,
    /// Why commands were refused, by message (numbers stripped).
    pub reasons: std::collections::BTreeMap<String, u64>,
    pub planted: u64,
    pub loads: u64,
    pub joins: u64,
    pub leaves: u64,
    /// Joins the host refused (a full server).
    pub refused_joins: u64,
    pub most_bricks: usize,
    pub most_vehicles: usize,
    pub most_projectiles: usize,
    pub mounted_ticks: u64,
    /// Contained per-system failures `Session::step` reported.
    pub step_errors: Vec<String>,
}

pub struct Chaos {
    pub session: Session,
    catalog: Catalog,
    spawns: Vec<glam::Vec3>,
    bots: Vec<Bot>,
    rng: Rng,
    names: u64,
    options: Options,
    sequences: std::collections::BTreeMap<u64, (u64, u64)>,
    /// The last actions, printed when something breaks.
    pub log: VecDeque<String>,
    pub report: Report,
}

/// Everything the host sends clients, the way the server loop gathers it.
pub fn check_replicated(session: &mut Session) -> Result<()> {
    ensure_finite("snapshot", &session.snapshot())?;
    ensure_finite("motion states", &session.motion_states())?;
    ensure_finite("vehicle poses", &session.vehicle_poses())?;
    ensure_finite("vehicle infos", &session.vehicle_infos())?;
    ensure_finite("weapon view", &session.weapon_view())?;
    ensure_finite("vitals", &session.vitals())?;
    ensure_finite("camera orbs", &session.camera_orbs())?;
    ensure_finite("entities", &session.package_entities())?;
    ensure_finite("minigames", &session.minigame_views())?;
    ensure_finite("cues", &session.take_cues())?;
    ensure_finite("notices", &session.take_private_notices())?;
    anyhow::ensure!(
        session.time_scale().is_finite(),
        "time scale is {}",
        session.time_scale()
    );
    Ok(())
}

pub fn view(session: &Session, owner: u64) -> View {
    let snapshot_players = session.motion_states();
    let feet = snapshot_players
        .iter()
        .find(|(p, _)| p.owner == owner)
        .map(|(p, _)| glam::Vec3::from(p.feet));
    let alive = session.vitals().get(&owner).is_none_or(|v| v.alive);
    View {
        me: owner,
        feet,
        alive,
        mounted: session.mounted(owner).is_some(),
        bricks: session
            .simulation()
            .state()
            .bricks
            .iter()
            .take(4096)
            .map(|(id, b)| {
                let definition = match &b.definition {
                    bri_world::ContentRef::Resolved(id) => id.clone(),
                    _ => String::new(),
                };
                (*id, glam::Vec3::from(b.position), definition)
            })
            .collect(),
        vehicles: session
            .vehicle_poses()
            .iter()
            .map(|p| glam::Vec3::from(p.position))
            .collect(),
        players: snapshot_players
            .iter()
            .map(|(p, _)| (p.owner, glam::Vec3::from(p.feet)))
            .collect(),
    }
}

/// What the bots may plant, spawn and load from a fixture.
pub fn catalog(fixture: &Fixture) -> Catalog {
    let definitions = &fixture.session.simulation().definitions;
    Catalog {
        bricks: fixture
            .bricks
            .iter()
            .filter_map(|id| {
                let mesh = &definitions.entries.get(id)?.mesh;
                Some(BrickKind {
                    id: id.clone(),
                    studs: mesh.footprint_studs.map(|v| v as i32),
                    plates: mesh.height_plates as i32,
                })
            })
            .collect(),
        vehicles: fixture.vehicles.clone(),
        saves: fixture.saves.clone(),
        palette: fixture.session.simulation().state().palette.len(),
        extent: fixture.extent,
    }
}

impl Chaos {
    pub fn new(fixture: Fixture, options: Options) -> Result<Self> {
        let catalog = catalog(&fixture);
        let mut chaos = Self {
            session: fixture.session,
            catalog,
            spawns: fixture.spawn_points,
            bots: Vec::new(),
            rng: Rng::new(options.seed),
            names: 0,
            sequences: Default::default(),
            options,
            log: VecDeque::new(),
            report: Report::default(),
        };
        let mut settings = chaos.session.server_settings().clone();
        // Bots build faster than a person; the rate limit is not under test.
        settings.bricks_per_second = 1000;
        chaos.session.set_server_settings(settings)?;
        for i in 0..chaos.options.bots {
            chaos.join(i < chaos.options.administrators)?;
        }
        Ok(chaos)
    }

    fn note(&mut self, line: String) {
        if self.log.len() >= 64 {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    fn join(&mut self, administrator: bool) -> Result<()> {
        self.names += 1;
        let spawn = self.spawns[self.rng.below(self.spawns.len())];
        let owner = match self
            .session
            .join(format!("Chaos{}", self.names), spawn, administrator)
        {
            Ok(owner) => owner,
            Err(error) => {
                self.report.refused_joins += 1;
                self.note(format!("join refused: {error:#}"));
                return Ok(());
            }
        };
        let seed = self.rng.next_u64();
        self.bots.push(Bot::new(owner, administrator, seed));
        self.report.joins += 1;
        self.note(format!("join {owner} admin={administrator}"));
        Ok(())
    }

    /// One server tick: every bot moves and maybe acts, players come and
    /// go, then the host steps.
    pub fn tick(&mut self) -> Result<()> {
        let tick = self.session.simulation().state().tick;
        if self.rng.chance(1.0 / 240.0) && self.bots.len() > self.options.administrators {
            let index = self.options.administrators
                + self
                    .rng
                    .below(self.bots.len() - self.options.administrators);
            let bot = self.bots.remove(index);
            self.note(format!("leave {}", bot.owner));
            self.session.disconnect(bot.owner)?;
            self.report.leaves += 1;
        }
        if self.bots.len() < self.options.bots && self.rng.chance(1.0 / 120.0) {
            self.join(false)?;
        }
        for i in 0..self.bots.len() {
            let owner = self.bots[i].owner;
            let view = view(&self.session, owner);
            let input = self.bots[i].movement(&view);
            let moves = &mut self.sequences.entry(owner).or_default().0;
            *moves += 1;
            let sequence = *moves;
            let _ = self.session.movement(owner, sequence, input);
            if view.mounted {
                self.report.mounted_ticks += 1;
            }
            if let Some((command, aim)) = self.bots[i].command(&view, &self.catalog) {
                let commands = &mut self.sequences.get_mut(&owner).unwrap().1;
                *commands += 1;
                let sequence = *commands;
                let summary = format!("{command:?}");
                let summary: String = summary.chars().take(160).collect();
                self.note(format!("t{tick} {owner} {summary} aim={aim:?}"));
                self.report.commands += 1;
                match self.session.command_with_aim(owner, sequence, command, aim) {
                    Ok(Reply::Planted(_)) => self.report.planted += 1,
                    Ok(Reply::Loaded { .. }) => self.report.loads += 1,
                    Ok(_) => {}
                    Err(error) => {
                        self.report.rejected += 1;
                        let reason: String = format!("{error:#}")
                            .chars()
                            .filter(|c| !c.is_ascii_digit())
                            .take(80)
                            .collect();
                        *self.report.reasons.entry(reason).or_default() += 1;
                    }
                }
            }
        }
        if let Err(error) = self.session.step() {
            let message = format!("t{tick}: {error:#}");
            self.note(format!("step error {message}"));
            if self.report.step_errors.len() < 32 {
                self.report.step_errors.push(message);
            }
        }
        // The host's own queues, drained as the server loop does.
        let _ = self.session.take_dirty();
        let _ = self.session.take_notices();
        for owner in self.session.take_admin_disconnects() {
            let _ = self.session.take_admin_disconnect_message(owner);
            self.bots.retain(|b| b.owner != owner);
        }
        self.report.ticks += 1;
        self.report.most_bricks = self
            .report
            .most_bricks
            .max(self.session.simulation().state().bricks.len());
        self.report.most_vehicles = self
            .report
            .most_vehicles
            .max(self.session.vehicle_poses().len());
        self.report.most_projectiles = self
            .report
            .most_projectiles
            .max(self.session.weapon_view().fired().count());
        if tick.is_multiple_of(self.options.check_every) {
            check_replicated(&mut self.session)?;
        }
        Ok(())
    }

    /// Run until `ticks` or the first failure, which is returned with the
    /// seed and the actions leading up to it. Panics are caught and reported
    /// the same way.
    pub fn run(mut self) -> Result<Report> {
        let seed = self.options.seed;
        for _ in 0..self.options.ticks {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.tick()));
            let failure = match outcome {
                Ok(Ok(())) => continue,
                Ok(Err(error)) => format!("{error:#}"),
                Err(panic) => format!(
                    "panic: {}",
                    panic
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| panic.downcast_ref::<&str>().copied())
                        .unwrap_or("<non-string panic>")
                ),
            };
            let log: Vec<_> = self.log.iter().cloned().collect();
            bail!(
                "chaos seed {seed:#x} failed at tick {}: {failure}\nlast actions:\n  {}",
                self.report.ticks,
                log.join("\n  ")
            );
        }
        Ok(self.report)
    }
}
