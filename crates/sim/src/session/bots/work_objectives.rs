//! Places a package offers to work or guard (`work` and `guard` in
//! `bot_objectives`), grounded into the shared planner. A `work` offer is
//! a place to stand and a rules command to hold until a declared counter
//! rises; a `guard` offer is a place to stand and fight from. Controls are
//! ordinary walking plus the package's own command, sent as a player's key
//! sends it, so the rules need no bot branch.
use super::objectives::{
    self, Cause, Completion, DesiredState, GroundedAction, GroundingBudget, Progress,
};
use super::planning::{self, Compare, Effect, EffectGroup, FactValue, Facts, Goal, Predicate};
use super::*;
use crate::session::packages::bot_objectives::{GuardOffer, WorkOffer};
use crate::session::{Command, PackageArg, PackageCommand};
use bri_package_runtime::bot_objectives::CounterBinding;

/// Seconds past a job's own time before a hold that moved nothing is
/// given up on.
const WORK_MARGIN_SECONDS: f32 = 10.0;
/// Share of a job's time after which the bot is past stopping for a fight.
const COMMITTED_SHARE: f32 = 0.8;
/// How much less a guard post is worth than work of the same weight.
const GUARD_SHARE: f32 = 0.25;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkTask {
    pub command_package: String,
    pub command: String,
    pub hold: bool,
    /// How long the rules need it held, in ticks.
    pub ticks: u64,
    pub open: Option<CounterBinding>,
    pub completion: CounterBinding,
    pub baseline: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Task {
    Work(WorkTask),
    Guard,
}

/// One offer as one bot found it, valid while the bot's life, game, round
/// and team stand and the Add-On's state allows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlaceStamp {
    pub package: String,
    pub actor: OwnerId,
    pub spawn_tick: u64,
    pub game: bri_minigames::GameId,
    pub round: u64,
    pub team: Option<bri_minigames::TeamId>,
    pub id: String,
    pub at: [u32; 3],
    pub radius: u32,
    pub claimants: u8,
    pub weight: u32,
    pub task: Task,
}

/// What a bot has sent for a place and has still to let go of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Held {
    pub place: u64,
    pub package: String,
    pub command: String,
}

impl PlaceStamp {
    pub(super) fn at(&self) -> Vec3 {
        Vec3::from(self.at.map(f32::from_bits))
    }
    pub(super) fn radius(&self) -> f32 {
        f32::from_bits(self.radius)
    }
    fn weight(&self) -> f32 {
        f32::from_bits(self.weight)
    }
    /// A stable key for the offer, shared by every bot it is offered to.
    pub(super) fn key(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in self
            .package
            .bytes()
            .chain(std::iter::once(0xff))
            .chain(self.id.bytes())
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }
    fn fact_key(&self) -> String {
        format!("place:{}:{}:{}", self.package, self.actor, self.id)
    }
    fn context_valid(&self, session: &Session, bot: OwnerId) -> bool {
        self.actor == bot
            && session.game_of(bot) == Some(self.game)
            && session
                .minigames
                .game(self.game)
                .is_ok_and(|g| g.round == self.round && !g.round_over)
            && session.peers.get(&bot).is_some_and(|p| {
                p.combat.alive
                    && p.combat.spawn_tick == self.spawn_tick
                    && session
                        .minigames
                        .player(p.combat.player)
                        .is_ok_and(|p| p.team == self.team)
            })
    }
    pub(super) fn validate(&self, session: &Session, bot: OwnerId) -> bool {
        if !self.context_valid(session, bot) {
            return false;
        }
        match &self.task {
            Task::Guard => true,
            Task::Work(work) => {
                session
                    .package_objective_counter(bot, &self.package, &work.completion, false)
                    .is_some()
                    && work.open.as_ref().is_none_or(|open| {
                        session.package_objective_truthy(bot, &self.package, open) == Some(true)
                    })
            }
        }
    }
    /// A work offer is done when its counter has risen; a guard post is
    /// never done, only left.
    pub(super) fn completion(&self, session: &Session, bot: OwnerId) -> Option<bool> {
        if !self.context_valid(session, bot) {
            return None;
        }
        match &self.task {
            Task::Guard => Some(false),
            Task::Work(work) => session
                .package_objective_counter(bot, &self.package, &work.completion, false)
                .map(|n| n > work.baseline),
        }
    }
    pub(super) fn is_guard(&self) -> bool {
        matches!(self.task, Task::Guard)
    }
    fn is_work(&self) -> bool {
        matches!(self.task, Task::Work(_))
    }
    /// What the offer is worth to a bot `distance` away: the rules'
    /// weight, falling with the way to go and, for work, the time it
    /// takes. A guard post is worth less than work of the same weight.
    fn utility(&self, distance: f32) -> f32 {
        let near = 1.0 / (1.0 + distance);
        match &self.task {
            Task::Work(work) => {
                self.weight() * near / (1.0 + work.ticks as f32 * TICK / WORK_TIME_UNIT)
            }
            Task::Guard => self.weight() * GUARD_SHARE * near,
        }
    }
    fn bytes(&self) -> usize {
        self.package.len()
            + self.id.len()
            + match &self.task {
                Task::Guard => 0,
                Task::Work(w) => w.command.len() + w.command_package.len() + 64,
            }
    }
}

/// Seconds that halve a job's appeal.
const WORK_TIME_UNIT: f32 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Action {
    pub stamp: PlaceStamp,
    /// Since when it has stood within reach of the place.
    arrived: Option<u64>,
}

impl Action {
    pub(super) fn validate(&self, session: &Session, bot: OwnerId) -> bool {
        self.stamp.validate(session, bot)
    }
    fn in_reach(&self, session: &Session, bot: OwnerId) -> bool {
        session.peers.get(&bot).is_some_and(|p| {
            Vec3::from(p.player.state().feet).distance(self.stamp.at()) <= self.stamp.radius()
        })
    }
    pub(super) fn advance(&mut self, session: &Session, bot: OwnerId) {
        if self.in_reach(session, bot) {
            let tick = session.simulation.state().tick;
            self.arrived.get_or_insert(tick);
        } else {
            self.arrived = None;
        }
    }
    /// The step's deadline while it stands at the place: a job gets its
    /// own time and a margin; a guard post has none.
    pub(super) fn deadline(&self, tick: u64) -> Option<u64> {
        let arrived = self.arrived?;
        Some(match &self.stamp.task {
            Task::Work(work) => arrived + work.ticks + (WORK_MARGIN_SECONDS / TICK).round() as u64,
            Task::Guard => tick.saturating_add(objectives::APPROACH_TIMEOUT),
        })
    }
    /// Past most of the job: stopping now would waste the time put in.
    pub(super) fn committed(&self, session: &Session) -> bool {
        let Task::Work(work) = &self.stamp.task else {
            return false;
        };
        let tick = session.simulation.state().tick;
        self.arrived
            .is_some_and(|at| (tick - at) as f32 >= work.ticks as f32 * COMMITTED_SHARE)
    }
    pub(super) fn view(&self, session: &Session, bot: OwnerId) -> Option<objectives::View> {
        if !self.validate(session, bot) {
            return None;
        }
        let at = self.stamp.at();
        Some(objectives::View::locomotion(at, at + Vec3::Y))
    }
    pub(super) fn observe(&self, session: &Session, bot: OwnerId) -> Progress {
        if self.stamp.completion(session, bot) == Some(true) {
            return Progress::Changed;
        }
        if !self.validate(session, bot) {
            return Progress::Unavailable;
        }
        Progress::Pending
    }
    /// Walk to the place (the route does that), then send the command as a
    /// player's key would, held while it stands there.
    pub(super) fn execute(&self, session: &mut Session, bot: OwnerId) -> Result<&'static str> {
        let Task::Work(work) = &self.stamp.task else {
            return Ok(if self.in_reach(session, bot) {
                "guarding"
            } else {
                "approach"
            });
        };
        if !self.in_reach(session, bot) {
            session.release_work(bot);
            return Ok("approach");
        }
        let place = self.stamp.key();
        if session.bots.brains[&bot]
            .objective
            .held()
            .is_some_and(|h| h.place == place)
        {
            return Ok("working");
        }
        if session.send_work(bot, &work.command_package, &work.command, work.hold, true) {
            if work.hold {
                session
                    .bots
                    .brains
                    .get_mut(&bot)
                    .unwrap()
                    .objective
                    .hold(Held {
                        place,
                        package: work.command_package.clone(),
                        command: work.command.clone(),
                    });
            }
            Ok("working")
        } else {
            let tick = session.simulation.state().tick;
            session
                .bots
                .brains
                .get_mut(&bot)
                .unwrap()
                .objective
                .fail(tick, "work command refused");
            Ok("refused")
        }
    }
}

impl Session {
    /// Send a place's command as the bot's own key: `down` presses a held
    /// command and lets go of it again; a command sent once has no arguments.
    /// Whether the command was accepted.
    pub(super) fn send_work(
        &mut self,
        bot: OwnerId,
        package: &str,
        command: &str,
        hold: bool,
        down: bool,
    ) -> bool {
        let sequence = self.peers[&bot].last_sequence.saturating_add(1);
        self.command(
            bot,
            sequence,
            Command::Package(PackageCommand {
                package: package.into(),
                command: command.into(),
                args: if hold {
                    vec![PackageArg::Bool(down)]
                } else {
                    Vec::new()
                },
            }),
        )
        .is_ok()
    }
    /// Let go of a held command.
    pub(super) fn release_work(&mut self, bot: OwnerId) {
        let Some(held) = self
            .bots
            .brains
            .get_mut(&bot)
            .and_then(|b| b.objective.let_go())
        else {
            return;
        };
        self.send_work(bot, &held.package, &held.command, true, false);
    }
    /// Each tick: a bot that holds a command or a place keeps both only while
    /// its step is that place's work and it is working; otherwise it lets go.
    pub(super) fn settle_places(&mut self, bot: OwnerId, tick: u64) {
        let working = self.bots.brains.get(&bot).and_then(|brain| {
            let step = brain.objective.step.as_ref()?;
            let objectives::Executor::Place(action) = &step.executor else {
                return None;
            };
            Some((
                action.stamp.key(),
                brain.behaviour == Behaviour::Objective,
                action.stamp.is_work(),
            ))
        });
        if let Some((place, _, _)) = working {
            self.bots.claims.hold_place(bot, place, tick);
        } else if self.bots.claims.held_place(bot, tick).is_some() {
            self.bots.claims.release_place(bot);
        }
        let holding = self
            .bots
            .brains
            .get(&bot)
            .and_then(|b| b.objective.held().map(|h| h.place));
        if let Some(place) = holding
            && !working.is_some_and(|(key, objective, _)| key == place && objective)
        {
            self.release_work(bot);
        }
    }

    /// The package's work and guard offers as desired states, best first.
    pub(super) fn place_desired_states(
        &self,
        bot: OwnerId,
        works: Vec<WorkOffer>,
        guards: Vec<GuardOffer>,
        (game, round, team, spawn_tick, feet): (
            bri_minigames::GameId,
            u64,
            Option<bri_minigames::TeamId>,
            u64,
            Vec3,
        ),
    ) -> Vec<DesiredState> {
        let tick = self.simulation.state().tick;
        let mut offered: Vec<(f32, PlaceStamp)> = Vec::new();
        let stamp = |package: String,
                     id: String,
                     at: [f32; 3],
                     radius: f32,
                     claimants: u8,
                     weight: f32,
                     task: Task| {
            PlaceStamp {
                package,
                actor: bot,
                spawn_tick,
                game,
                round,
                team,
                id,
                at: [at[0].to_bits(), at[1].to_bits(), at[2].to_bits()],
                radius: radius.to_bits(),
                claimants,
                weight: weight.to_bits(),
                task,
            }
        };
        for offer in works {
            let goal = offer.goal;
            let ticks = (goal.seconds / TICK).ceil() as u64;
            let command_package = goal
                .command
                .package
                .clone()
                .unwrap_or_else(|| offer.package.clone());
            let place = stamp(
                offer.package,
                goal.id,
                goal.at,
                goal.radius,
                goal.claimants,
                goal.weight,
                Task::Work(WorkTask {
                    command_package,
                    command: goal.command.name,
                    hold: goal.command.hold,
                    ticks,
                    open: goal.open,
                    completion: goal.completion,
                    baseline: offer.baseline,
                }),
            );
            // Not offered while the Add-On says it cannot be worked now.
            if !place.validate(self, bot) {
                continue;
            }
            offered.push((place.utility(place.at().distance(feet)), place));
        }
        for offer in guards {
            let goal = offer.goal;
            let place = stamp(
                offer.package,
                goal.id,
                goal.at,
                goal.radius,
                goal.claimants,
                goal.weight,
                Task::Guard,
            );
            offered.push((place.utility(place.at().distance(feet)), place));
        }
        // A place already held by as many as it takes is left to them, and
        // the bot takes the next best (or a guard post).
        offered.retain(|(_, place)| {
            !self
                .bots
                .claims
                .place_full(place.key(), place.claimants, bot, tick)
        });
        offered.sort_by(|a, b| b.0.total_cmp(&a.0));
        offered
            .into_iter()
            .map(|(_, place)| {
                let predicate = match place.task {
                    Task::Guard => Predicate {
                        key: place.fact_key(),
                        compare: Compare::Equal,
                        value: FactValue::Bool(true),
                    },
                    Task::Work(ref work) => Predicate {
                        key: place.fact_key(),
                        compare: Compare::AtLeast,
                        value: FactValue::Number(work.baseline.saturating_add(1)),
                    },
                };
                DesiredState {
                    id: format!("{}:{}", place.package, place.id),
                    predicates: Goal(vec![predicate]),
                    completion: Completion::Place(Box::new(place)),
                }
            })
            .collect()
    }

    pub(super) fn append_place_actions(
        &self,
        bot: OwnerId,
        stamp: &PlaceStamp,
        facts: &mut Facts,
        budget: &mut GroundingBudget,
    ) -> Result<Vec<GroundedAction>, planning::Failure> {
        if !stamp.validate(self, bot) {
            return Err(planning::Failure::NoPlan);
        }
        let (now, after, label) = match &stamp.task {
            Task::Guard => (FactValue::Bool(false), FactValue::Bool(true), "guard"),
            Task::Work(work) => {
                let now = self
                    .package_objective_counter(bot, &stamp.package, &work.completion, false)
                    .ok_or(planning::Failure::Unsupported)?;
                let after = work
                    .baseline
                    .checked_add(1)
                    .ok_or(planning::Failure::Unsupported)?;
                (FactValue::Number(now), FactValue::Number(after), "work")
            }
        };
        facts.insert(stamp.fact_key(), now);
        budget.reserve(1, 4, stamp.bytes() * 2)?;
        let feet = Vec3::from(self.peers[&bot].player.state().feet);
        let model = planning::Action {
            id: format!("place:{}:{}:{label}", stamp.package, stamp.id),
            cost: 1 + (feet.distance(stamp.at()) * 10.0).min(100000.0) as u32,
            preconditions: vec![],
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: vec![Effect::Set {
                    key: stamp.fact_key(),
                    value: after,
                }],
            }],
        };
        Ok(vec![GroundedAction {
            model,
            cause: Cause::Place(stamp.clone()),
            executor: objectives::Executor::Place(Action {
                stamp: stamp.clone(),
                arrived: None,
            }),
        }])
    }
}
