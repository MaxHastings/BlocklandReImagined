//! Experimental grounded rule objectives. Projection never executes game state;
//! movement, activation, event scheduling and MiniGame remain authoritative.
//!
//! What the game asks and how to get there is found in `discover`, what a
//! brick's rows would do in `project`, and each tick's plan and step in
//! `turn`; this file holds what they share.
mod discover;
mod project;
mod turn;
use super::planning::{
    self, Action, Compare, Effect, EffectGroup, FactValue, Facts, Goal, Predicate,
};
use super::*;
use bri_events::{self as ev, Class, Entity, Intent, Trigger};
use ev::rules::{Condition, Datum, Property, RuleOp, Subject};

const SOURCES: usize = 64;
const ROWS: usize = 256;
const ACTIONS: usize = 32;
const RETRY: u64 = 120;
/// Ticks after an objective completes before it looks for the next one:
/// the completing event's own effects (a flag put back) land first.
const COMPLETION_SETTLE: u64 = 12;
const APPROACH_TIMEOUT: u64 = 120 * 30;
/// Closer than this to the step's point since the best so far counts as
/// approach progress, which extends the approach deadline.
const APPROACH_PROGRESS: f32 = 0.5;

/// Policy supplies desired state independently of the controls which can cause it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DesiredState {
    pub id: String,
    pub predicates: Goal,
    pub completion: Completion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Completion {
    PackageCounter(Box<super::package_objectives::Stamp>),
    RoundWin {
        game: bri_minigames::GameId,
        round: u64,
        actor: OwnerId,
    },
    /// The actor's team's score (its own, without a team) above `from`
    /// this round: what scoring is for when the plan cannot see the win.
    ScoreRise {
        game: bri_minigames::GameId,
        round: u64,
        team: Option<bri_minigames::TeamId>,
        actor: OwnerId,
        from: i64,
    },
}

/// Outputs that change nothing a plan reads: effects, sounds, messages,
/// explanations and putting everyone back at a spawn. A row of these
/// neither hides a goal nor counts toward one.
fn changes_nothing_planned(intent: &Intent) -> bool {
    matches!(
        intent,
        Intent::Brick(ev::BrickOp::ColorFx(_) | ev::BrickOp::ShapeFx(_) | ev::BrickOp::PlaySound(_))
            | Intent::Client(ev::ClientOp::Message { .. } | ev::ClientOp::PlaySound(_))
            | Intent::MiniGame(ev::MiniGameOp::Message { .. } | ev::MiniGameOp::RespawnAll)
            | Intent::Rule(RuleOp::Explain)
    )
}

impl DesiredState {
    fn observed_completion(
        &self,
        session: &Session,
        team: Option<bri_minigames::TeamId>,
    ) -> Option<bool> {
        match &self.completion {
            Completion::RoundWin { game, round, actor } => session
                .round_results()
                .rev()
                .find(|r| r.game == game.0 && r.round == *round)
                .map(|r| r.owners.contains(actor) || team.is_some_and(|t| r.teams.contains(&t))),
            Completion::PackageCounter(stamp) => stamp
                .completion(session, stamp.actor)
                .filter(|complete| *complete),
            Completion::ScoreRise {
                game,
                round,
                team,
                actor,
                from,
            } => {
                let g = session.minigames.game(*game).ok()?;
                if g.round != *round || g.round_over {
                    return Some(false);
                }
                (session.score_of(*game, *team, *actor)? > *from).then_some(true)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SourceStamp {
    pub brick: BrickId,
    pub rows: Vec<ev::Row>,
    pub owner: OwnerId,
    pub name: Option<String>,
}

/// A causal input has rule meaning, context and observations, but no control
/// method. Multiple providers may supply different grounded ways to cause it.
#[derive(Clone, Debug)]
pub(super) struct CausalInput {
    pub source: SourceStamp,
    pub context: Trigger,
    pub projection: Projection,
}

/// Admission and canonical progress belong to the mechanism that owns them.
/// Packages need no invented event input to participate in the same lifecycle.
#[derive(Clone, Debug)]
pub(super) enum Cause {
    Rule(CausalInput),
    Rules(Vec<CausalInput>),
    Package(super::package_objectives::Stamp),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Progress {
    Pending,
    Changed,
    Unavailable,
}

impl Cause {
    fn rules(&self) -> Option<&[CausalInput]> {
        match self {
            Self::Rule(cause) => Some(std::slice::from_ref(cause)),
            Self::Rules(causes) => Some(causes),
            Self::Package(_) => None,
        }
    }
    fn source(&self) -> Option<BrickId> {
        match self {
            Self::Package(stamp) => stamp.source(),
            _ => self.rules()?.first().map(|c| c.source.brick),
        }
    }
    fn validate(&self, session: &Session, bot: OwnerId, full: bool) -> bool {
        if let Self::Package(stamp) = self {
            return stamp.validate(session, bot);
        }
        self.rules().is_some_and(|causes| {
            !causes.is_empty()
                && causes.iter().all(|cause| {
                    let stamp = &cause.source;
                    session
                        .simulation
                        .state()
                        .bricks
                        .get(&stamp.brick)
                        .is_some_and(|b| b.owner == stamp.owner)
                        && session
                            .events
                            .world
                            .as_ref()
                            .and_then(|w| w.program(super::super::events::id(stamp.brick)))
                            .is_some_and(|p| {
                                p.owner_scope == stamp.owner
                                    && p.name == stamp.name
                                    && (!full || p.rows == stamp.rows)
                            })
                })
        })
    }
    fn admitted(&self, session: &Session, bot: OwnerId, after: u64) -> Option<(u64, u64)> {
        let Self::Rule(cause) = self else {
            return None;
        };
        let object = cause
            .context
            .targets
            .get(&ev::Slot::Object)
            .map(|e| e.id.index);
        session
            .events
            .objective_inputs
            .get(&(cause.source.brick, bot, cause.context.input.clone(), object))
            .copied()
            .filter(|(origin, _)| *origin > after)
    }
    /// The authored rows replace this captured object once they are due.
    fn resets(&self, object: u64) -> bool {
        self.rules()
            .is_some_and(|causes| causes.iter().any(|c| c.projection.resets.contains(&object)))
    }
    fn delay(&self) -> u64 {
        self.rules().map_or(0, |causes| {
            causes.iter().map(|c| c.projection.delay).max().unwrap_or(0)
        })
    }
    fn observe(&self, session: &Session) -> Progress {
        let Some(causes) = self.rules() else {
            return Progress::Pending;
        };
        let mut changed = false;
        for observation in causes.iter().flat_map(|c| &c.projection.observations) {
            let Some(value) = session.rule_query(
                &observation.context,
                observation.target,
                &observation.condition,
            ) else {
                return Progress::Unavailable;
            };
            changed |= value != observation.before;
        }
        if changed {
            Progress::Changed
        } else {
            Progress::Pending
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Projection {
    pub groups: Vec<EffectGroup>,
    pub delay: u64,
    // Parallel timing for the grouped interpreter; prefixes precede scheduled rows.
    group_delays: Vec<u64>,
    admission_groups: usize,
    observations: Vec<Observation>,
    /// Captured objects a projected ResetObject row replaces once due.
    resets: Vec<u64>,
}

/// Guaranteed native changes preceding admission, never projected state writes.
/// For example native death changes Alive and real member score before rows.
#[derive(Clone, Debug)]
pub(super) struct Transition {
    pub target: Entity,
    pub property: Property,
    pub key: String,
    pub after: FactValue,
}

#[derive(Clone, Debug)]
struct Observation {
    context: Trigger,
    target: Entity,
    condition: Condition,
    before: Datum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrickMethod {
    Activate,
    Region,
    Touch,
}
impl BrickMethod {
    fn from_input(input: &str) -> Option<Self> {
        match input {
            "onActivate" => Some(Self::Activate),
            "onRegionEnter" => Some(Self::Region),
            "onBotTouch" => Some(Self::Touch),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct BrickAction {
    source: BrickId,
    method: BrickMethod,
    point: Vec3,
    aim: Vec3,
    destination: Vec3,
    rearming: bool,
    bounds: (Vec3, Vec3),
    region: Option<[f32; 3]>,
}

/// One provider-owned lifecycle, extended with typed providers after the Brick
/// baseline passes. No input strings choose an executor in the shared scheduler.
#[derive(Clone, Debug)]
pub(super) enum Executor {
    Brick(BrickAction),
    Package(super::package_objectives::Action),
    Physical(super::physical_objectives::Choice),
    Enemy(super::combat_objectives::Action),
}

#[derive(Clone, Debug)]
pub(super) struct GroundedAction {
    pub model: Action,
    pub cause: Cause,
    pub executor: Executor,
}

pub(super) fn limits() -> planning::Limits {
    planning::Limits {
        max_nodes: 256,
        max_depth: 12,
        max_actions: ACTIONS,
        max_candidates: 4096,
        max_facts: 128,
        max_model_terms: 4096,
        max_model_bytes: 65536,
    }
}

#[derive(Clone, Debug)]
pub(super) struct Step {
    pub action_id: String,
    pub desired: DesiredState,
    pub cause: Cause,
    pub executor: Executor,
    pub phase: &'static str,
    pub game: bri_minigames::GameId,
    pub round: u64,
    pub team: Option<bri_minigames::TeamId>,
    pub deadline: u64,
    pub waiting: Option<u64>,
    pub observed_origin: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct View {
    pub point: Vec3,
    pub aim: Vec3,
    pub waiting: bool,
    pub equip: Option<usize>,
    pub trigger: Option<bool>,
    pub resource: Option<claims::Resource>,
    pub board: Option<(u64, u8)>,
    pub held: Option<ObjectRef>,
    pub drive: Option<(u64, u8)>,
    pub move_while_waiting: bool,
    pub enemy: Option<OwnerId>,
    pub enemy_evidence: Option<Knowledge>,
    pub physical_progress: bool,
    /// The way a loose body is being delivered (`Directive::heading`).
    pub heading: Vec3,
    /// It carries what the objective delivers (a declared carriage it
    /// picked up, a body it holds): stopping would put that at risk.
    pub committed: bool,
    /// The step is planned for its mini-game team, so a teammate doing it
    /// does it for the team; without one it is the bot's own progress
    /// (a racer's checkpoint), which nobody else's arrival advances.
    pub shared: bool,
}
impl View {
    /// The step needs only its feet: no tool, trigger, body, seat or enemy
    /// of its own, so its aim is free for other things on the way.
    pub(super) fn feet_only(&self) -> bool {
        self.enemy.is_none()
            && self.trigger.is_none()
            && self.equip.is_none()
            && self.held.is_none()
            && self.drive.is_none()
            && self.board.is_none()
            && self.resource.is_none()
    }
    pub(super) fn locomotion(point: Vec3, aim: Vec3) -> Self {
        Self {
            point,
            aim,
            waiting: false,
            equip: None,
            trigger: None,
            resource: None,
            board: None,
            held: None,
            drive: None,
            move_while_waiting: false,
            enemy: None,
            enemy_evidence: None,
            physical_progress: false,
            heading: Vec3::ZERO,
            committed: false,
            shared: false,
        }
    }
}
impl Step {
    fn invalidation_diagnostic(&self, session: &Session, bot: OwnerId) -> &'static str {
        if let Executor::Physical(action) = &self.executor
            && let Err(reason) = action.directive(session, bot)
        {
            return reason.diagnostic();
        }
        "objective source or capability changed"
    }
    pub(super) fn source(&self) -> Option<BrickId> {
        self.cause.source()
    }
    fn validate(&self, session: &Session, bot: OwnerId, full: bool) -> bool {
        self.cause.validate(session, bot, full)
            && (self.executor.validate(session, bot) || self.expected_reset(session))
            && session.game_of(bot) == Some(self.game)
            && session
                .minigames
                .game(self.game)
                .is_ok_and(|g| g.round == self.round && !g.round_over)
            && session
                .peers
                .get(&bot)
                .and_then(|p| session.minigames.player(p.combat.player).ok())
                .is_some_and(|p| p.team == self.team)
    }
    /// After real admission, the projected ResetObject replacing the
    /// captured body is the authored outcome, not an invalidated step.
    /// Its observation (the old incarnation no longer exists) is checked with
    /// every other effect once the due time passes.
    fn object_gone(&self, session: &Session) -> bool {
        matches!(&self.executor, Executor::Physical(action)
            if session
                .object_centre(ObjectRef::Vehicle(action.goal.object.vehicle))
                .is_none())
    }
    fn expected_reset(&self, session: &Session) -> bool {
        let Executor::Physical(action) = &self.executor else {
            return false;
        };
        let object = action.goal.object.vehicle;
        self.waiting.is_some()
            && self.cause.resets(object)
            && session.object_centre(ObjectRef::Vehicle(object)).is_none()
    }
    fn view(&self, session: &Session, bot: OwnerId) -> Option<View> {
        let mut view = if self.waiting.is_some() && matches!(self.executor, Executor::Enemy(_)) {
            let peer = session.peers.get(&bot)?;
            View::locomotion(peer.player.state().feet.into(), peer.player.eye())
        } else {
            self.executor.view(session, bot)?
        };
        view.waiting = self.waiting.is_some();
        view.shared = self.team.is_some();
        if view.waiting
            && matches!(&self.executor,Executor::Physical(action)
            if matches!(action.method,super::physical_objectives::Method::Hammer{..}))
        {
            // The native grip must keep holding through due-time guards, but
            // a completed swing must not keep hitting the delivered body.
            view.trigger = Some(false);
        }
        Some(view)
    }
    /// [`Self::view`] with whether it carries what the step delivers.
    fn committed_view(&self, session: &Session, bot: OwnerId) -> Option<View> {
        let view = self.view(session, bot)?;
        let committed = match &self.executor {
            Executor::Package(action) => action.carrying(session, bot),
            _ => view
                .held
                .is_some_and(|held| session.held_by(bot) == Some(held)),
        };
        Some(View { committed, ..view })
    }
    fn progress(&self, session: &Session, bot: OwnerId, tick: u64) -> Option<Progress> {
        match &self.executor {
            Executor::Package(action) => match action.observe(session, bot) {
                Progress::Pending => None,
                progress => Some(progress),
            },
            Executor::Brick(_) | Executor::Physical(_) | Executor::Enemy(_) => self
                .waiting
                .filter(|until| tick >= *until)
                .map(|_| self.cause.observe(session)),
        }
    }
    fn admitted(&self, session: &Session, bot: OwnerId) -> Option<(u64, u64)> {
        match &self.executor {
            Executor::Enemy(action) => {
                let (origin, death_tick) = action.admitted(session, bot)?;
                let mut admitted = death_tick;
                for cause in self.cause.rules()? {
                    let victim = cause.context.targets.get(&ev::Slot::Player)?.id.index;
                    let (_, when) = session
                        .events
                        .objective_inputs
                        .get(&(
                            cause.source.brick,
                            victim,
                            cause.context.input.clone(),
                            None,
                        ))
                        .copied()
                        .filter(|(_, when)| *when >= death_tick)?;
                    admitted = admitted.max(when);
                }
                Some((origin, admitted))
            }
            _ => self.cause.admitted(session, bot, self.observed_origin),
        }
    }
    fn wait_for(&mut self, when: u64) {
        self.phase = "waiting";
        self.waiting = Some(when + self.cause.delay() + 8);
        self.deadline = when + self.cause.delay() + 120 * 5;
    }
}
type Model = (Facts, DesiredState, Vec<Action>, BTreeMap<String, Step>);
struct DesiredDiscovery {
    candidates: Vec<DesiredState>,
    unsupported: bool,
}
#[derive(Default)]
pub(super) struct GroundingBudget {
    targets: usize,
    terms: usize,
    bytes: usize,
}
impl GroundingBudget {
    pub(super) fn reserve(
        &mut self,
        targets: usize,
        terms: usize,
        bytes: usize,
    ) -> Result<(), planning::Failure> {
        self.targets = self.targets.saturating_add(targets);
        self.terms = self.terms.saturating_add(terms);
        self.bytes = self.bytes.saturating_add(bytes);
        if self.targets > 128 || self.terms > 4096 || self.bytes > 65536 {
            return Err(planning::Failure::ModelBudgetExceeded);
        }
        Ok(())
    }
    fn program(&mut self, program: &ev::BrickProgram) -> Result<(), planning::Failure> {
        self.reserve(0, 0, program.name.as_ref().map_or(0, String::len))?;
        for r in &program.rows {
            let mut bytes = r.input.len().saturating_add(r.output.len());
            if let ev::Target::Named(v) | ev::Target::Derived(v) = &r.target {
                bytes = bytes.saturating_add(v.len());
            }
            for c in &r.conditions {
                bytes = bytes.saturating_add(c.key.len());
                if let Datum::Text(v) = &c.value {
                    bytes = bytes.saturating_add(v.len());
                }
            }
            for p in &r.params {
                if let ev::Value::Text(v) | ev::Value::Datablock(Some(v)) = p {
                    bytes = bytes.saturating_add(v.len());
                }
                if let ev::Value::Rows(ev::RowSelection::Indices(indices)) = p {
                    self.reserve(0, indices.len(), 0)?;
                }
            }
            if let Some(p) = &r.preserved {
                bytes = bytes
                    .saturating_add(p.original.len())
                    .saturating_add(p.diagnostic.len());
            }
            self.reserve(0, 1 + r.params.len() + r.conditions.len(), bytes)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct FailedAction {
    action_id: String,
    cause: Cause,
    executor: Executor,
    game: bri_minigames::GameId,
    round: u64,
    team: Option<bri_minigames::TeamId>,
    until: u64,
}

/// One bounded negative result, checked against freshly grounded facts/actions.
/// A complete NoPlan/depth proof is independent of positive action costs;
/// exhausted work budgets depend on queue order and require identical costs.
#[derive(Clone, Debug)]
struct FailedSearch {
    facts: Facts,
    actions: Vec<Action>,
    context: (bri_minigames::GameId, u64, Option<bri_minigames::TeamId>),
    desired: DesiredState,
    failure: planning::Failure,
}
impl FailedSearch {
    fn matches(
        &self,
        facts: &Facts,
        actions: &[Action],
        context: (bri_minigames::GameId, u64, Option<bri_minigames::TeamId>),
        desired: &DesiredState,
    ) -> bool {
        self.desired == *desired
            && self.context == context
            && self.facts == *facts
            && self.actions.len() == actions.len()
            && self.actions.iter().zip(actions).all(|(a, b)| {
                a.id == b.id
                    && a.preconditions == b.preconditions
                    && a.effect_groups == b.effect_groups
                    && (a.cost == b.cost
                        || (b.cost > 0
                            && matches!(
                                self.failure,
                                planning::Failure::NoPlan | planning::Failure::DepthLimitExceeded
                            )))
            })
    }
}
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub step: Option<Step>,
    next: u64,
    failed: Vec<FailedAction>,
    failed_search: Option<FailedSearch>,
    pub route: Vec<String>,
    desired: Option<DesiredState>,
    desired_context: Option<(bri_minigames::GameId, u64, Option<bri_minigames::TeamId>)>,
    failed_desired: Vec<(DesiredState, u64)>,
    last_tick: Option<u64>,
    pub(super) searches: u64,
    pub(super) reused: u64,
    pub diagnostic: Option<&'static str>,
    /// Between steps: a step finished (or the objective was completed, or
    /// its body replaced) and the next is not planned yet. The hold rule
    /// holds the objective through this (`behaviour::Ask::paused`).
    paused: bool,
    /// The nearest the current step's approach has come to its point.
    best: Option<f32>,
    /// In a game, round or team it has not had a planning turn for yet:
    /// it does not know yet what that game asks of it.
    unplanned: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Detail<'a> {
    pub desired: &'a str,
    pub action: &'a str,
    pub provider: &'static str,
    pub phase: &'static str,
    pub route: &'a [String],
}

pub(super) fn next_turn(
    last: Option<OwnerId>,
    ready: impl Iterator<Item = OwnerId>,
) -> Option<OwnerId> {
    let mut first = None;
    for bot in ready {
        if first.is_none() {
            first = Some(bot);
        }
        if last.is_none_or(|v| bot > v) {
            return Some(bot);
        }
    }
    first
}
impl State {
    pub(super) fn source(&self) -> Option<BrickId> {
        self.step.as_ref().and_then(Step::source)
    }
    pub(super) fn drive(&self, session: &Session, bot: OwnerId) -> Option<(u64, u8)> {
        let step = self.step.as_ref()?;
        let drive = step.view(session, bot)?.drive?;
        step.validate(session, bot, true).then_some(drive)
    }
    pub(super) fn detail(&self) -> Option<Detail<'_>> {
        let step = self.step.as_ref()?;
        Some(Detail {
            desired: &step.desired.id,
            action: &step.action_id,
            provider: step.executor.provider(),
            phase: step.phase,
            route: &self.route,
        })
    }

    /// Rest/rider control skips the entire brain tick. Account for that pause
    /// here; retaining the old Objective behavior must not consume approach
    /// time on resumption. Authored event waiting deadlines are never paused.
    pub(super) fn suspend(&mut self, tick: u64) {
        self.account_control(tick, true);
    }

    fn account_control(&mut self, tick: u64, suspended: bool) {
        let elapsed = self
            .last_tick
            .replace(tick)
            .map_or(0, |old| tick.saturating_sub(old));
        if suspended
            && let Some(step) = self.step.as_mut()
            && step.waiting.is_none()
        {
            step.deadline = step.deadline.saturating_add(elapsed);
        }
    }
    /// A step it was after failed (timed out, made no progress) and is
    /// cooling down before it is tried again: the objective is still
    /// offered, so it does not walk all the way home meanwhile.
    /// Between steps (see `paused`).
    pub(super) fn between_steps(&self) -> bool {
        self.step.is_none() && self.paused
    }
    pub(super) fn pursuing(&self) -> bool {
        self.step.is_none() && !self.failed.is_empty()
    }
    /// Its game's objectives are not known to it yet: its planning turn
    /// in this game, round and team is still to come.
    pub(super) fn unplanned(&self) -> bool {
        self.unplanned
    }
    pub(super) fn ready(&self, tick: u64) -> bool {
        self.step.is_none() && tick >= self.next
    }
    pub(super) fn fail(&mut self, tick: u64, reason: &'static str) {
        self.paused = false;
        if let Some(step) = self.step.take() {
            self.failed.retain(|f| f.action_id != step.action_id);
            if self.failed.len() == 8 {
                self.failed.remove(0);
            }
            self.failed.push(FailedAction {
                action_id: step.action_id,
                cause: step.cause,
                executor: step.executor,
                game: step.game,
                round: step.round,
                team: step.team,
                until: tick.saturating_add(APPROACH_TIMEOUT * 2),
            });
        }
        self.route.clear();
        self.next = tick.saturating_add(RETRY * 3);
        self.diagnostic = Some(reason);
    }
}
/// Combine native dispatches in the scheduler's causal order, preserving the
/// one rule interpreter. Native changes precede every scheduled authored group.
pub(super) fn project_rule_causes(
    causes: &[CausalInput],
    budget: &mut GroundingBudget,
) -> Result<Vec<EffectGroup>, planning::Failure> {
    let count = causes.iter().map(|c| c.projection.groups.len()).sum();
    budget.reserve(0, count, 0)?;
    let mut prefix = Vec::new();
    let mut scheduled = Vec::new();
    for (source, cause) in causes.iter().enumerate() {
        for (index, group) in cause.projection.groups.iter().enumerate() {
            if index < cause.projection.admission_groups {
                prefix.push(group.clone());
            } else {
                scheduled.push((
                    cause.projection.group_delays[index],
                    source,
                    index,
                    group.clone(),
                ));
            }
        }
    }
    scheduled.sort_by_key(|(delay, source, index, _)| (*delay, *source, *index));
    prefix.extend(scheduled.into_iter().map(|(_, _, _, group)| group));
    Ok(prefix)
}

fn eq(key: String, value: FactValue) -> Predicate {
    Predicate {
        key,
        compare: Compare::Equal,
        value,
    }
}
fn round_key(game: bri_minigames::GameId, round: u64) -> String {
    format!("round/{}/{round}/over", game.0)
}
fn team_score_key(game: bri_minigames::GameId, team: bri_minigames::TeamId) -> String {
    format!("score/team/{}/{}", game.0, team.0)
}
fn reset_key(spawner: BrickId, object: u64) -> String {
    format!("object/{spawner}/{object}/reset")
}
fn win_key(bot: OwnerId) -> String {
    format!("wins/{bot}")
}
fn scalar(d: Datum) -> Option<FactValue> {
    match d {
        Datum::Bool(v) => Some(FactValue::Bool(v)),
        Datum::Number(v) => Some(FactValue::Number(v)),
        Datum::Text(_) => None,
    }
}

impl BrickAction {
    fn ground(session: &Session, bot: OwnerId, cause: &CausalInput) -> Option<(Self, u32)> {
        let method = BrickMethod::from_input(&cause.context.input)?;
        let brick = cause.source.brick;
        let (min, max) = session.simulation.brick_box(brick)?;
        let aim = (min + max) * 0.5;
        let peer = session.peers.get(&bot)?;
        let feet = Vec3::from(peer.player.state().feet);
        let bounds = bri_world::regions::bounds(
            session.simulation.state().bricks[&brick].rule_region,
            (min, max),
        );
        let point = match method {
            BrickMethod::Region => Vec3::new(
                (bounds.0.x + bounds.1.x) * 0.5,
                feet.y,
                (bounds.0.z + bounds.1.z) * 0.5,
            ),
            BrickMethod::Touch => Vec3::new(aim.x, max.y, aim.z),
            BrickMethod::Activate => {
                let d = flat(feet - aim).normalize_or(Vec3::Z);
                Vec3::new(aim.x + d.x * 1.2, feet.y, aim.z + d.z * 1.2)
            }
        };
        let cost = 1
            + (feet.distance(point) * 10.0).min(100000.0) as u32
            + cause.projection.delay.min(100000) as u32;
        let touching = match method {
            BrickMethod::Region => {
                let p = feet + Vec3::Y;
                p.cmpge(bounds.0).all() && p.cmple(bounds.1).all()
            }
            BrickMethod::Touch => {
                let (lo, hi) = peer.player.world_bounds();
                Vec3::from(lo).cmple(max).all() && Vec3::from(hi).cmpge(min).all()
            }
            BrickMethod::Activate => false,
        };
        let destination = point;
        let point = if touching {
            let hi = if method == BrickMethod::Region {
                bounds.1
            } else {
                max
            };
            Vec3::new(hi.x + 2.0, feet.y, point.z)
        } else {
            point
        };
        Some((
            Self {
                source: brick,
                method,
                point,
                aim,
                destination,
                rearming: touching,
                bounds: (min, max),
                region: session.simulation.state().bricks[&brick].rule_region,
            },
            cost,
        ))
    }
}

impl Executor {
    fn view(&self, _session: &Session, _bot: OwnerId) -> Option<View> {
        match self {
            Self::Brick(action) => Some(View::locomotion(action.point, action.aim)),
            Self::Package(action) => action.view(_session, _bot),
            Self::Enemy(action) => action.view(_session, _bot),
            Self::Physical(action) => action.directive(_session, _bot).ok().map(|d| View {
                point: d.point,
                aim: d.aim,
                resource: Some(d.resource),
                equip: d.equip,
                trigger: matches!(
                    action.method,
                    super::physical_objectives::Method::Hold { .. }
                        | super::physical_objectives::Method::Hammer { .. }
                )
                .then_some(d.trigger),
                board: d.board,
                held: d.held,
                drive: d.drive,
                physical_progress: d.physical_progress,
                heading: d.heading,
                move_while_waiting: matches!(
                    action.method,
                    super::physical_objectives::Method::Hold { .. }
                ),
                ..View::default()
            }),
        }
    }
    fn validate(&self, session: &Session, bot: OwnerId) -> bool {
        let action = match self {
            Self::Brick(action) => action,
            Self::Package(action) => return action.validate(session, bot),
            Self::Physical(action) => return action.directive(session, bot).is_ok(),
            Self::Enemy(action) => {
                return action.admitted(session, bot).is_some() || action.validate(session, bot);
            }
        };
        let brick = action.source;
        session.peers.get(&bot).is_some_and(|p| p.combat.alive)
            && session.simulation.brick_box(brick) == Some(action.bounds)
            && session
                .simulation
                .state()
                .bricks
                .get(&brick)
                .is_some_and(|b| b.rule_region == action.region)
    }
    fn provider(&self) -> &'static str {
        match self {
            Self::Brick(_) => "native brick input",
            Self::Package(_) => "declared package pickup/zone",
            Self::Enemy(_) => "native combat/death rules",
            Self::Physical(action) => match action.method {
                super::physical_objectives::Method::Push => "native physical contact",
                super::physical_objectives::Method::Hammer { .. } => "native hammer",
                super::physical_objectives::Method::Hold { .. } => "declared physical hold",
                super::physical_objectives::Method::Drive { .. } => "native control seat",
            },
        }
    }
    fn observe_controls(&mut self, session: &Session, bot: OwnerId) {
        let action = match self {
            Self::Brick(action) => action,
            Self::Package(action) => {
                action.advance(session, bot);
                return;
            }
            Self::Physical(action) => {
                action.advance(session, bot);
                return;
            }
            Self::Enemy(_) => return,
        };
        let brick = action.source;
        if !action.rearming {
            return;
        }
        let Some((lo, hi)) = session.simulation.brick_box(brick) else {
            return;
        };
        let peer = &session.peers[&bot];
        let outside = match action.method {
            BrickMethod::Region => {
                let (lo, hi) = bri_world::regions::bounds(
                    session.simulation.state().bricks[&brick].rule_region,
                    (lo, hi),
                );
                let p = Vec3::from(peer.player.state().feet) + Vec3::Y;
                !p.cmpge(lo).all() || !p.cmple(hi).all()
            }
            BrickMethod::Touch => {
                let (p, q) = peer.player.world_bounds();
                !Vec3::from(p).cmple(hi).all() || !Vec3::from(q).cmpge(lo).all()
            }
            BrickMethod::Activate => true,
        };
        if outside {
            action.rearming = false;
            action.point = action.destination;
        }
    }

    fn execute(&self, session: &mut Session, bot: OwnerId) -> Result<&'static str> {
        if let Self::Physical(action) = self {
            let phase = action.execute(session, bot)?;
            // Rearm still uses the selected method's ordinary controls. Keep
            // its diagnostic phase until a real region exit is observed.
            return Ok(if action.rearming() { "rearm" } else { phase });
        }
        let Self::Brick(action) = self else {
            return Ok("approach");
        };
        let brick = action.source;
        if action.method != BrickMethod::Activate {
            return Ok("approach");
        }
        let peer = &session.peers[&bot];
        let eye = peer.player.eye();
        if action.aim.distance(eye) > 5.0 {
            return Ok("approach");
        }
        let state = peer.player.state();
        let direction = Vec3::new(
            state.yaw.sin() * state.pitch.cos(),
            state.pitch.sin(),
            -state.yaw.cos() * state.pitch.cos(),
        );
        if session
            .simulation
            .target_through(eye, direction, 5.0)?
            .and_then(|(hit, _)| hit.brick)
            != Some(brick)
        {
            return Ok("approach");
        }
        if session
            .weapons
            .actor(ActorId(bot))
            .is_some_and(|a| a.selected.is_some())
        {
            session.equip_tool(bot, None)?;
            return Ok("preparing");
        }
        let sequence = session.peers[&bot].last_sequence.saturating_add(1);
        session.command(bot, sequence, Command::Activate)?;
        let sequence = session.peers[&bot].last_sequence.saturating_add(1);
        session.command(bot, sequence, Command::ActivateRelease)?;
        Ok("admission")
    }
}

/// What an Add-On output does to a plan's facts, and the team whose score
/// it changes, if any.
type PackageEffects = (Vec<Effect>, Option<bri_minigames::TeamId>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compound_native_inputs_preserve_prefix_and_global_scheduled_order() {
        let group = |guards, effects| EffectGroup { guards, effects };
        let set = |key: &str, n| Effect::Set {
            key: key.into(),
            value: FactValue::Number(n),
        };
        let input = |brick, groups, group_delays, admission_groups| CausalInput {
            source: SourceStamp {
                brick,
                rows: vec![],
                owner: 7,
                name: None,
            },
            context: Trigger::new(
                super::super::super::events::id(brick),
                "onRulePlayerDied",
                1,
            ),
            projection: Projection {
                groups,
                group_delays,
                admission_groups,
                delay: 60,
                observations: vec![],
                resets: vec![],
            },
        };
        let a = input(
            1,
            vec![
                group(vec![], vec![set("native", 1)]),
                group(
                    vec![eq("earlier".into(), FactValue::Number(1))],
                    vec![set("desired", 1)],
                ),
            ],
            vec![0, 60],
            1,
        );
        let b = input(
            2,
            vec![group(
                vec![eq("native".into(), FactValue::Number(1))],
                vec![set("earlier", 1)],
            )],
            vec![0],
            0,
        );
        let groups = project_rule_causes(&[a, b], &mut GroundingBudget::default()).unwrap();
        let action = Action {
            id: "actual compound native dispatch".into(),
            cost: 1,
            preconditions: vec![],
            effect_groups: groups,
        };
        let facts = Facts::from([
            ("native".into(), FactValue::Number(0)),
            ("earlier".into(), FactValue::Number(0)),
            ("desired".into(), FactValue::Number(0)),
        ]);
        let desired = Goal(vec![eq("desired".into(), FactValue::Number(1))]);
        assert_eq!(
            planning::plan(&facts, &[action], &desired, limits()).unwrap(),
            vec!["actual compound native dispatch"]
        );
    }

    #[test]
    fn failed_search_reuse_checks_current_facts_semantics_context_and_order_sensitive_costs() {
        let facts = Facts::from([("latch".into(), FactValue::Number(0))]);
        let actions = vec![Action {
            id: "unfamiliar-source/input".into(),
            cost: 10,
            preconditions: vec![],
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: vec![Effect::Set {
                    key: "latch".into(),
                    value: FactValue::Number(1),
                }],
            }],
        }];
        let context = (bri_minigames::GameId(1), 2, None);
        let desired = DesiredState {
            id: "known-goal".into(),
            predicates: Goal(vec![]),
            completion: Completion::RoundWin {
                game: context.0,
                round: context.1,
                actor: 17,
            },
        };
        let mut cache = FailedSearch {
            facts: facts.clone(),
            actions: actions.clone(),
            context,
            desired: desired.clone(),
            failure: planning::Failure::NoPlan,
        };
        assert!(cache.matches(&facts, &actions, context, &desired));
        let mut different_goal = desired.clone();
        different_goal.id = "other-goal".into();
        assert!(!cache.matches(&facts, &actions, context, &different_goal));
        different_goal = desired.clone();
        different_goal.completion = Completion::RoundWin {
            game: context.0,
            round: context.1,
            actor: 18,
        };
        assert!(!cache.matches(&facts, &actions, context, &different_goal));
        let changed_facts = Facts::from([("latch".into(), FactValue::Number(1))]);
        assert!(!cache.matches(&changed_facts, &actions, context, &desired));
        assert!(!cache.matches(&facts, &actions, (context.0, 3, None), &desired));
        assert!(!cache.matches(
            &facts,
            &actions,
            (bri_minigames::GameId(2), 2, None),
            &desired
        ));
        assert!(!cache.matches(
            &facts,
            &actions,
            (context.0, 2, Some(bri_minigames::TeamId(1))),
            &desired
        ));
        let mut changed = actions.clone();
        changed[0].id = "replacement/input".into();
        assert!(!cache.matches(&facts, &changed, context, &desired));
        changed = actions.clone();
        changed[0].effect_groups[0].effects.clear();
        assert!(!cache.matches(&facts, &changed, context, &desired));
        changed = actions.clone();
        changed[0].cost = 30; // Wander changes distance; a complete NoPlan remains a proof.
        assert!(cache.matches(&facts, &changed, context, &desired));
        cache.failure = planning::Failure::NodeBudgetExceeded;
        assert!(!cache.matches(&facts, &changed, context, &desired));
        assert!(cache.matches(&facts, &actions, context, &desired));
    }
    #[test]
    fn round_robin_does_not_starve_same_modulo_owner_ids() {
        let bots = [1, 121, 241];
        let mut cursor = None;
        let mut sequence = Vec::new();
        for _ in 0..6 {
            cursor = next_turn(cursor, bots.into_iter());
            sequence.push(cursor.unwrap());
        }
        assert_eq!(sequence, vec![1, 121, 241, 1, 121, 241]);
        assert_eq!(next_turn(Some(121), [1, 241].into_iter()), Some(241));
    }
    #[test]
    fn physical_context_uses_actual_actor_and_real_quota_client_without_fake_connection() {
        let world = bri_world::World::new("Context".into(), "test".into(), vec![[1.0; 4]]);
        let simulation = crate::simulation::Simulation::new(
            world,
            crate::testing::definitions(),
            vec![
                rapier3d::prelude::ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                    .translation(Vec3::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap();
        let mut s = Session::new(simulation);
        s.set_lan_host(true);
        s.set_event_catalog(ev::testing::catalog(), Vec::<String>::new())
            .unwrap();
        let owner = s
            .join("Author".into(), Vec3::new(0.0, 0.05, 0.0), true)
            .unwrap();
        let bot = s
            .join("Physical NPC".into(), Vec3::new(10.0, 0.05, 0.0), false)
            .unwrap();
        s.bots.brains.insert(
            bot,
            Brain::new(
                None,
                BotKind {
                    id: "probe".into(),
                    name: "Probe".into(),
                    ..Default::default()
                },
                Vec3::ZERO,
                bot,
                0,
            ),
        );
        s.command(
            owner,
            1,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: Default::default(),
            }),
        )
        .unwrap();
        let game = s.game_of(owner).unwrap();
        s.command(
            bot,
            1,
            Command::MiniGame(MiniGameRequest::Join { game: game.0 }),
        )
        .unwrap();
        let actor = s.peers[&owner].actor.clone();
        let source = s
            .simulation
            .plant_group_floating(
                &actor,
                vec![Brick::new(
                    bri_world::ContentRef::Resolved(crate::testing::PLATE.into()),
                    [20.25, 0.1, 20.25],
                    owner,
                )],
            )
            .unwrap()[0];
        for input in ["onActivate", "onRegionEnter"] {
            let cx = s
                .input_context(
                    source,
                    input,
                    Some(bot),
                    super::super::super::events::InputExtra::default(),
                    1,
                )
                .unwrap();
            assert_eq!(cx.targets[&ev::Slot::Player].id.index, bot);
            assert_eq!(cx.targets[&ev::Slot::Instigator].id.index, bot);
            assert_eq!(cx.targets[&ev::Slot::MiniGame].id.index, game.0);
            assert!(!cx.targets.contains_key(&ev::Slot::Client));
            assert_eq!(
                cx.client.unwrap().id.index,
                owner,
                "real captured LAN account"
            );
            let target = cx.targets[&ev::Slot::Player];
            assert_eq!(s.rule_game_context(&cx, cx.source, target).unwrap(), game);
            s.simulation.mutate(source, |b| b.owner = bot).unwrap();
            assert!(
                s.rule_game_context(&cx, cx.source, target).is_err(),
                "foreign creator cannot control match"
            );
            s.simulation.mutate(source, |b| b.owner = owner).unwrap();
        }
        // A genuinely separate creator's unsupported reaction cannot disable
        // this actor's otherwise grounded match (not a global world abort).
        let other = s
            .join("Other creator".into(), Vec3::new(40.0, 0.05, 0.0), false)
            .unwrap();
        s.command(
            other,
            1,
            Command::MiniGame(MiniGameRequest::Create {
                color: 1,
                settings: Default::default(),
            }),
        )
        .unwrap();
        assert_ne!(s.game_of(other), s.game_of(bot));
        let foreign_actor = s.peers[&other].actor.clone();
        let mut foreign_brick = Brick::new(
            bri_world::ContentRef::Resolved(crate::testing::PLATE.into()),
            [40.25, 0.1, 40.25],
            other,
        );
        foreign_brick.events.push(ev::Row {
            enabled: true,
            input: "onRuleTimer".into(),
            output: "setColliding".into(),
            target: ev::Target::Slot(ev::Slot::SelfBrick),
            params: vec![ev::Value::Bool(false)],
            conditions: vec![],
            delay_ms: 0,
            preserved: None,
        });
        let foreign = s
            .simulation
            .plant_group_floating(&foreign_actor, vec![foreign_brick])
            .unwrap()[0];
        s.simulation
            .mutate(source, |b| {
                b.events.push(ev::Row {
                    enabled: true,
                    input: "onActivate".into(),
                    output: "winRound".into(),
                    target: ev::Target::Slot(ev::Slot::Player),
                    params: vec![],
                    conditions: vec![],
                    delay_ms: 0,
                    preserved: None,
                })
            })
            .unwrap();
        s.sync_event_programs(&BTreeSet::from([source, foreign]));
        let (_, _, actions, _) = s
            .objective_snapshot(bot, 0, &[], s.rule_desired_state(bot).unwrap())
            .unwrap();
        assert_eq!(
            actions.len(),
            1,
            "only actual creator/game candidates grounded"
        );

        // The same grouped interpreter accepts exact native admission previews.
        // Future Alive=false must be a prefix effect, not a fabricated initial
        // fact: an ordinary activation cannot satisfy this guarded row.
        s.simulation
            .mutate(source, |b| {
                b.events[0].conditions.push(Condition {
                    subject: Subject::Player,
                    property: Property::Alive,
                    key: String::new(),
                    compare: ev::rules::Compare::Equal,
                    value: Datum::Bool(false),
                })
            })
            .unwrap();
        s.sync_event_programs(&BTreeSet::from([source]));
        let program = s
            .events
            .world
            .as_ref()
            .unwrap()
            .program(super::super::super::events::id(source))
            .unwrap();
        let cx = s
            .input_context(
                source,
                "onActivate",
                Some(bot),
                super::super::super::events::InputExtra::default(),
                1,
            )
            .unwrap();
        let target = cx.targets[&ev::Slot::Player];
        let transition = Transition {
            target,
            property: Property::Alive,
            key: String::new(),
            after: FactValue::Bool(false),
        };
        let mut base = Facts::from([(win_key(bot), FactValue::Bool(false))]);
        let plain = s
            .ground_causal_input(
                bot,
                program,
                cx.clone(),
                &[],
                &mut base,
                &mut GroundingBudget::default(),
            )
            .unwrap();
        let desired = s.rule_desired_state(bot).unwrap();
        let action = Action {
            id: "ordinary-input".into(),
            cost: 1,
            preconditions: vec![],
            effect_groups: plain.projection.groups,
        };
        assert_eq!(
            planning::plan(&base, &[action], &desired.predicates, limits()),
            Err(planning::Failure::NoPlan)
        );
        let preview = s
            .ground_causal_input(
                bot,
                program,
                cx.clone(),
                &[transition],
                &mut base,
                &mut GroundingBudget::default(),
            )
            .unwrap();
        let key = s
            .objective_fact_key(
                &cx,
                target,
                &Condition {
                    subject: Subject::Target,
                    property: Property::Alive,
                    key: String::new(),
                    compare: ev::rules::Compare::Equal,
                    value: Datum::Bool(false),
                },
            )
            .unwrap();
        assert_eq!(
            base[&key],
            FactValue::Bool(true),
            "base remains authoritative before admission"
        );
        assert_eq!(
            s.objective_fact_key(&cx, target, &program.rows[0].conditions[0])
                .as_ref(),
            Some(&key),
            "Player and Target aliases share a fact"
        );
        let action = Action {
            id: "native-admission-preview".into(),
            cost: 1,
            preconditions: vec![],
            effect_groups: preview.projection.groups,
        };
        assert_eq!(
            planning::plan(&base, &[action], &desired.predicates, limits()).unwrap(),
            vec!["native-admission-preview"]
        );
        assert!(s.is_alive(bot), "projection cannot execute native death");
        assert_eq!(s.vitals()[&bot].score, 0);
        assert_eq!(
            s.round_results().count(),
            0,
            "prediction cannot announce a winner"
        );
    }
}
