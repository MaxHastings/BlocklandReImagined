//! Experimental grounded rule objectives. Projection never executes game state;
//! movement, activation, event scheduling and MiniGame remain authoritative.
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

impl Session {
    /// The game and team whose canonical score a Team subject reads: the
    /// rule actor's, exactly as `rule_query` resolves it.
    fn objective_team(
        &self,
        cx: &Trigger,
    ) -> Option<(bri_minigames::GameId, bri_minigames::TeamId)> {
        let actor = cx
            .targets
            .get(&ev::Slot::Instigator)
            .or_else(|| cx.targets.get(&ev::Slot::Player))
            .or(cx.client.as_ref())?
            .id
            .index;
        let player = self
            .minigames
            .player(self.peers.get(&actor)?.combat.player)
            .ok()?;
        Some((player.game?, player.team?))
    }

    fn team_score(&self, game: bri_minigames::GameId, team: bri_minigames::TeamId) -> Option<i64> {
        self.minigames.team_score(game, team).ok()
    }

    pub(super) fn objective_fact_key(
        &self,
        cx: &Trigger,
        target: Entity,
        c: &Condition,
    ) -> Option<String> {
        Some(match c.property {
            Property::Variable => {
                format!("var/{:?}", self.rule_key(cx, target, c.subject, &c.key)?)
            }
            Property::Color => {
                let entity = match c.subject {
                    Subject::SelfBrick => cx.source,
                    Subject::Target => target.id,
                    _ => return None,
                };
                format!("brick/color/{}", entity.index)
            }
            Property::RoundOver => {
                let game = self.rule_game_context(cx, cx.source, target).ok()?;
                round_key(game, self.minigames.game(game).ok()?.round)
            }
            Property::Score if c.subject == Subject::Player => {
                format!("score/{}", cx.targets.get(&ev::Slot::Player)?.id.index)
            }
            Property::Score if c.subject == Subject::Instigator => {
                format!("score/{}", cx.targets.get(&ev::Slot::Instigator)?.id.index)
            }
            Property::Score if c.subject == Subject::Target => format!("score/{}", target.id.index),
            // Team score is the canonical sum of its members' scores (the
            // rule interpreter's own definition), keyed by game and team.
            Property::Score if c.subject == Subject::Team => {
                let (game, team) = self.objective_team(cx)?;
                team_score_key(game, team)
            }
            _ => format!(
                "observation/{:?}/{:?}",
                self.rule_key(cx, target, c.subject, &c.key)?,
                c.property
            ),
        })
    }

    fn objective_predicate(
        &self,
        cx: &Trigger,
        target: Entity,
        c: &Condition,
        facts: &mut Facts,
    ) -> Option<Predicate> {
        let key = self.objective_fact_key(cx, target, c)?;
        let observed = self.rule_query(cx, target, c)?;
        if matches!(c.property, Property::Kind | Property::SpawnedBy) {
            let key = format!("{key}/matches/{:?}/{:?}", c.compare, c.value);
            facts.insert(key.clone(), FactValue::Bool(c.matches(Some(observed))));
            return Some(eq(key, FactValue::Bool(true)));
        }
        let value = scalar(c.value.clone())?;
        facts.insert(key.clone(), scalar(observed)?);
        let compare = match c.compare {
            ev::rules::Compare::Equal => Compare::Equal,
            ev::rules::Compare::NotEqual => Compare::NotEqual,
            ev::rules::Compare::Less => Compare::Less,
            ev::rules::Compare::AtMost => Compare::AtMost,
            ev::rules::Compare::Greater => Compare::Greater,
            ev::rules::Compare::AtLeast => Compare::AtLeast,
        };
        Some(Predicate {
            key,
            compare,
            value,
        })
    }

    fn objective_effect(
        &self,
        bot: OwnerId,
        cx: &Trigger,
        target: Entity,
        op: &RuleOp,
        facts: &mut Facts,
        guards: &mut Vec<Predicate>,
    ) -> Option<Vec<Effect>> {
        let mut result = Vec::new();
        match op {
            RuleOp::Variable {
                scope,
                key,
                value,
                add,
            } => {
                if matches!(scope, Subject::MiniGame | Subject::Team) {
                    self.rule_game_context(cx, cx.source, target).ok()?;
                }
                let fact = format!("var/{:?}", self.rule_key(cx, target, *scope, key)?);
                let observed = self.rule_query(
                    cx,
                    target,
                    &Condition {
                        subject: *scope,
                        property: Property::Variable,
                        key: key.clone(),
                        compare: ev::rules::Compare::Equal,
                        value: Datum::Number(0),
                    },
                )?;
                facts.insert(fact.clone(), scalar(observed)?);
                result.push(if *add {
                    Effect::Add {
                        key: fact,
                        amount: *value,
                    }
                } else {
                    Effect::Set {
                        key: fact,
                        value: FactValue::Number(*value),
                    }
                });
            }
            RuleOp::AddScore(value) | RuleOp::AddTeamScore(value) => {
                self.rule_game_context(cx, cx.source, target).ok()?;
                if target.class != Class::Player || target.id.index != bot {
                    return None;
                }
                let player = self
                    .minigames
                    .player(self.peers.get(&bot)?.combat.player)
                    .ok()?;
                if matches!(op, RuleOp::AddTeamScore(_)) && player.team.is_none() {
                    return None;
                }
                let mut scores = vec![(format!("score/{bot}"), player.score)];
                // A team total is the sum of member scores: the same award
                // moves the member's team total by the same amount.
                if let (Some(game), Some(team)) = (player.game, player.team) {
                    scores.push((team_score_key(game, team), self.team_score(game, team)?));
                }
                for (key, current) in scores {
                    facts.insert(key.clone(), FactValue::Number(current));
                    // Canonical scoring saturates. Restrict this projection
                    // to arithmetic that cannot saturate, rather than predict wrong.
                    let (compare, bound) = if *value >= 0 {
                        (Compare::AtMost, i64::MAX - i64::from(*value))
                    } else {
                        (Compare::AtLeast, i64::MIN - i64::from(*value))
                    };
                    guards.push(Predicate {
                        key: key.clone(),
                        compare,
                        value: FactValue::Number(bound),
                    });
                    result.push(Effect::Add {
                        key,
                        amount: i64::from(*value),
                    });
                }
            }
            RuleOp::WinRound | RuleOp::EndRound => {
                let game = self.rule_game_context(cx, cx.source, target).ok()?;
                if target.class != Class::Player || target.id.index != bot {
                    return None;
                }
                let g = self.minigames.game(game).ok()?;
                let key = round_key(game, g.round);
                facts.insert(key.clone(), FactValue::Bool(g.round_over));
                guards.push(eq(key.clone(), FactValue::Bool(false)));
                result.push(Effect::Set {
                    key,
                    value: FactValue::Bool(true),
                });
                result.push(Effect::Set {
                    key: win_key(bot),
                    value: FactValue::Bool(matches!(op, RuleOp::WinRound)),
                });
            }
            _ => return None,
        }
        Some(result)
    }

    /// What an Add-On output's engine operations do to the facts a plan
    /// reads: a team's own points rise (Slayer's `IncScore`) or the round
    /// ends with its winners. Messages and sounds change nothing it plans
    /// on; any other operation has no known meaning here, so the output is
    /// unsupported rather than guessed. `None` inside when it does nothing
    /// a plan reads; with the team whose score it changes, if any.
    fn package_effects(
        &self,
        bot: OwnerId,
        ops: &[bri_package_runtime::ops::Op],
        facts: &mut Facts,
        guards: &mut Vec<Predicate>,
    ) -> Result<Option<PackageEffects>, planning::Failure> {
        use bri_package_runtime::ops::Op;
        use planning::Failure as F;
        let player = self
            .peers
            .get(&bot)
            .and_then(|p| self.minigames.player(p.combat.player).ok())
            .ok_or(F::Unsupported)?;
        let mut effects = Vec::new();
        let mut observed = None;
        for op in ops {
            match op {
                Op::SetTeamPoints(p) if p.add => {
                    let game = bri_minigames::GameId(p.game);
                    let team = u32::try_from(p.team)
                        .map(bri_minigames::TeamId)
                        .map_err(|_| F::Unsupported)?;
                    let key = team_score_key(game, team);
                    let current = self.team_score(game, team).ok_or(F::Unsupported)?;
                    facts.insert(key.clone(), FactValue::Number(current));
                    // As for rule scores: no projection where it saturates.
                    let (compare, bound) = if p.value >= 0 {
                        (Compare::AtMost, i64::MAX - p.value)
                    } else {
                        (Compare::AtLeast, i64::MIN - p.value)
                    };
                    guards.push(Predicate {
                        key: key.clone(),
                        compare,
                        value: FactValue::Number(bound),
                    });
                    effects.push(Effect::Add {
                        key,
                        amount: p.value,
                    });
                    observed.get_or_insert(team);
                }
                Op::EndRound(end) => {
                    let game = bri_minigames::GameId(end.game);
                    let g = self.minigames.game(game).map_err(|_| F::Unsupported)?;
                    let key = round_key(game, g.round);
                    facts.insert(key.clone(), FactValue::Bool(g.round_over));
                    guards.push(eq(key.clone(), FactValue::Bool(false)));
                    effects.push(Effect::Set {
                        key,
                        value: FactValue::Bool(true),
                    });
                    let won = end.players.contains(&bot)
                        || player
                            .team
                            .is_some_and(|t| end.teams.contains(&u64::from(t.0)));
                    effects.push(Effect::Set {
                        key: win_key(bot),
                        value: FactValue::Bool(won),
                    });
                }
                Op::Tell(_)
                | Op::TellMinigame(_)
                | Op::TellPlayers(_)
                | Op::Print(_)
                | Op::PrintMinigame(_)
                | Op::Broadcast(_)
                | Op::Sound(_) => {}
                _ => return Err(F::Unsupported),
            }
        }
        Ok((!effects.is_empty()).then_some((effects, observed)))
    }

    pub(super) fn project_objective_input(
        &self,
        bot: OwnerId,
        program: &ev::BrickProgram,
        cx: &Trigger,
        transitions: &[Transition],
        facts: &mut Facts,
        budget: &mut GroundingBudget,
    ) -> Result<Projection, planning::Failure> {
        use planning::Failure as F;
        let world = self.events.world.as_ref().ok_or(F::Unsupported)?;
        // Variable-changed inputs fire on the mutation's source/target brick.
        if program
            .rows
            .iter()
            .any(|r| r.enabled && r.input == "onRuleVariableChanged")
        {
            return Err(F::Unsupported);
        }
        if cx.source != program.id {
            return Err(F::Unsupported);
        }
        let mut groups = Vec::new();
        let mut ordered = Vec::new();
        let mut observations = Vec::new();
        let mut admission = Vec::new();
        let mut resets = Vec::new();
        for transition in transitions {
            let c = Condition {
                subject: Subject::Target,
                property: transition.property,
                key: transition.key.clone(),
                compare: ev::rules::Compare::Equal,
                value: match transition.after {
                    FactValue::Bool(v) => Datum::Bool(v),
                    FactValue::Number(v) => Datum::Number(v),
                },
            };
            let key = self
                .objective_fact_key(cx, transition.target, &c)
                .ok_or(F::Unsupported)?;
            let before = self
                .rule_query(cx, transition.target, &c)
                .ok_or(F::Unsupported)?;
            facts.insert(key.clone(), scalar(before.clone()).ok_or(F::Unsupported)?);
            budget.reserve(1, 1, key.len() + c.key.len())?;
            admission.push(Effect::Set {
                key,
                value: transition.after.clone(),
            });
            observations.push(Observation {
                context: cx.clone(),
                target: transition.target,
                condition: c,
                before,
            });
        }
        if !admission.is_empty() {
            groups.push(EffectGroup {
                guards: vec![],
                effects: admission,
            });
        }
        for (index, row) in program
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.enabled && r.input == cx.input)
        {
            let targets = world
                .row_targets(cx.source, cx, index as u16)
                .map_err(|_| F::Unsupported)?;
            // Resolve at most the event world's bounded fanout vector, then
            // reject before any per-target guards, facts or context clones.
            budget.reserve(targets.len(), 0, 0)?;
            let intent = world
                .row_intent(cx.source, index as u16)
                .ok_or(F::Unsupported)?;
            for target in targets {
                // All admitted targets must obey the same owner/operation
                // permission checked by the real executor.
                if target.class == Class::Brick
                    && self
                        .simulation
                        .state()
                        .bricks
                        .get(&target.id.index)
                        .is_none_or(|b| b.owner != program.owner_scope)
                {
                    return Err(F::Unsupported);
                }
                if row.conditions.iter().any(|c| {
                    matches!(c.property, Property::Kind | Property::SpawnedBy)
                        && !c.matches(self.rule_query(cx, target, c))
                }) {
                    continue;
                }
                let mut guards = Vec::new();
                for c in &row.conditions {
                    guards.push(
                        self.objective_predicate(cx, target, c, facts)
                            .ok_or(F::Unsupported)?,
                    );
                }
                let (effects, subject, property, key, value) = match intent {
                    // Resetting the captured object is a known authored effect:
                    // its spawner replaces it with a new incarnation once this
                    // row is due. It never counts as delivery or success; the
                    // physical provider grounds the replacement afresh.
                    Intent::Rule(RuleOp::ResetObject) => {
                        let spawner = self
                            .objective_reset(program, cx, target)
                            .ok_or(F::Unsupported)?;
                        let key = reset_key(spawner, target.id.index);
                        facts.insert(key.clone(), FactValue::Bool(false));
                        resets.push(target.id.index);
                        (
                            vec![Effect::Set {
                                key,
                                value: FactValue::Bool(true),
                            }],
                            Subject::Target,
                            Property::Exists,
                            String::new(),
                            Datum::Bool(true),
                        )
                    }
                    Intent::Rule(op) => {
                        if matches!(
                            op,
                            RuleOp::Variable {
                                scope: Subject::Target,
                                ..
                            }
                        ) && target.class == Class::Brick
                            && world.program(target.id).is_some_and(|p| {
                                p.rows
                                    .iter()
                                    .any(|r| r.enabled && r.input == "onRuleVariableChanged")
                            })
                        {
                            return Err(F::Unsupported);
                        }
                        let effects = self
                            .objective_effect(bot, cx, target, op, facts, &mut guards)
                            .ok_or(F::Unsupported)?;
                        let (subject, property, key, value) = match op {
                            RuleOp::Variable { scope, key, .. } => {
                                (*scope, Property::Variable, key.clone(), Datum::Number(0))
                            }
                            RuleOp::AddScore(_) | RuleOp::AddTeamScore(_) => (
                                Subject::Target,
                                Property::Score,
                                String::new(),
                                Datum::Number(0),
                            ),
                            RuleOp::WinRound | RuleOp::EndRound => (
                                Subject::MiniGame,
                                Property::RoundOver,
                                String::new(),
                                Datum::Bool(false),
                            ),
                            _ => return Err(F::Unsupported),
                        };
                        (effects, subject, property, key, value)
                    }
                    // An Add-On's output: what its own rules would do,
                    // read from a run that commits nothing.
                    Intent::Package(call) => {
                        let dispatch = ev::Dispatch {
                            context: cx.clone(),
                            source: cx.source,
                            target,
                            origin: cx.origin,
                            client: cx
                                .client
                                .or_else(|| cx.targets.get(&ev::Slot::Client).copied()),
                            input: cx.input.clone(),
                            row: index as u16,
                            output: row.output.clone(),
                            derived: match &row.target {
                                ev::Target::Derived(name) => Some(name.clone()),
                                _ => None,
                            },
                            scheduled_us: 0,
                            now_us: 0,
                            delay_ms: row.delay_ms,
                            intent: intent.clone(),
                        };
                        let ops = self
                            .package_output_ops(&dispatch, call)
                            .ok_or(F::Unsupported)?;
                        let Some((effects, team)) =
                            self.package_effects(bot, &ops, facts, &mut guards)?
                        else {
                            continue;
                        };
                        match team {
                            Some(team) => (
                                effects,
                                Subject::Team,
                                Property::Score,
                                team.0.to_string(),
                                Datum::Number(0),
                            ),
                            None => (
                                effects,
                                Subject::MiniGame,
                                Property::RoundOver,
                                String::new(),
                                Datum::Bool(false),
                            ),
                        }
                    }
                    Intent::Brick(ev::BrickOp::Color(value)) => {
                        let key = format!("brick/color/{}", target.id.index);
                        let color = self
                            .simulation
                            .state()
                            .bricks
                            .get(&target.id.index)
                            .ok_or(F::Unsupported)?
                            .color;
                        facts.insert(key.clone(), FactValue::Number(i64::from(color)));
                        (
                            vec![Effect::Set {
                                key,
                                value: FactValue::Number(i64::from(*value)),
                            }],
                            Subject::Target,
                            Property::Color,
                            String::new(),
                            Datum::Number(0),
                        )
                    }
                    Intent::Brick(
                        ev::BrickOp::ColorFx(_)
                        | ev::BrickOp::ShapeFx(_)
                        | ev::BrickOp::PlaySound(_),
                    ) => continue,
                    _ => return Err(F::Unsupported),
                };
                let condition = Condition {
                    subject,
                    property,
                    key,
                    value,
                    compare: ev::rules::Compare::Equal,
                };
                let before = self
                    .rule_query(cx, target, &condition)
                    .ok_or(F::Unsupported)?;
                if facts.len() > 128 {
                    return Err(F::FactBudgetExceeded);
                }
                let bytes = guards.iter().map(|g| g.key.len()).sum::<usize>()
                    + effects
                        .iter()
                        .map(|e| match e {
                            Effect::Set { key, .. } | Effect::Add { key, .. } => key.len(),
                        })
                        .sum::<usize>()
                    + condition.key.len()
                    + cx.input.len();
                budget.reserve(0, guards.len() + effects.len() + 1, bytes)?;
                observations.push(Observation {
                    context: cx.clone(),
                    target,
                    condition,
                    before,
                });
                ordered.push((row.delay_ms, index, EffectGroup { guards, effects }));
            }
        }
        ordered.sort_by_key(|(delay, index, _)| (*delay, *index));
        let delay = ordered
            .last()
            .map_or(0, |(d, _, _)| (u64::from(*d) * 120).div_ceil(1000));
        let admission_groups = groups.len();
        let mut group_delays = vec![0; admission_groups];
        for (delay, _, group) in ordered {
            group_delays.push((u64::from(delay) * 120).div_ceil(1000));
            groups.push(group);
        }
        Ok(Projection {
            groups,
            delay,
            group_delays,
            admission_groups,
            observations,
            resets,
        })
    }

    /// The spawner a ResetObject row would respawn, when its semantics are
    /// known: the target is exactly this input's captured object and the
    /// rule owner owns its spawner (the executor's own permission check).
    fn objective_reset(
        &self,
        program: &ev::BrickProgram,
        cx: &Trigger,
        target: Entity,
    ) -> Option<BrickId> {
        let captured = cx.targets.get(&ev::Slot::Object)?;
        if target.class != Class::Vehicle || captured.id != target.id {
            return None;
        }
        let spawner = self.vehicle_spawn_brick(bri_vehicles::VehicleId(target.id.index))?;
        self.simulation
            .state()
            .bricks
            .get(&spawner)
            .is_some_and(|b| b.owner == program.owner_scope)
            .then_some(spawner)
    }
    fn objective_reactions_clear(
        &self,
        bot: OwnerId,
        game: bri_minigames::GameId,
        facts: &Facts,
        budget: &mut GroundingBudget,
    ) -> Result<(), planning::Failure> {
        use planning::Failure as F;
        let owner = self
            .minigames
            .game(game)
            .map_err(|_| F::NoPlan)?
            .owner
            .account
            .0;
        let Some(sources) = self.events.objective_reactions.get(&owner) else {
            return Ok(());
        };
        if sources.len() > SOURCES {
            return Err(F::ModelBudgetExceeded);
        }
        let world = self.events.world.as_ref().ok_or(F::Unsupported)?;
        let mut rows = 0usize;
        for source in sources {
            let program = world
                .program(super::super::events::id(*source))
                .ok_or(F::Unsupported)?;
            rows = rows.saturating_add(program.rows.len());
            if rows > ROWS {
                return Err(F::ModelBudgetExceeded);
            }
            for (index, row) in program.rows.iter().enumerate().filter(|(_, r)| {
                r.enabled
                    && matches!(
                        r.input.as_str(),
                        "onRuleScoreChanged" | "onRuleTimer" | "onRuleRoundEnd"
                    )
            }) {
                let actor = (row.input == "onRuleScoreChanged").then_some(bot);
                let cx = self
                    .input_context(
                        *source,
                        &row.input,
                        actor,
                        super::super::events::InputExtra {
                            game: Some(game),
                            ..Default::default()
                        },
                        1,
                    )
                    .ok_or(F::Unsupported)?;
                let targets = world
                    .row_targets(cx.source, &cx, index as u16)
                    .map_err(|_| F::Unsupported)?;
                budget.reserve(targets.len(), 1, row.input.len())?;
                for target in targets {
                    match world.row_intent(cx.source, index as u16) {
                        Some(Intent::Brick(ev::BrickOp::PlaySound(_)))
                        | Some(Intent::Rule(RuleOp::Explain)) => {}
                        Some(Intent::Brick(ev::BrickOp::Color(_))) => {
                            if facts.contains_key(&format!("brick/color/{}", target.id.index)) {
                                return Err(F::Unsupported);
                            }
                        }
                        Some(Intent::Brick(ev::BrickOp::ColorFx(_) | ev::BrickOp::ShapeFx(_))) => {}
                        Some(Intent::Rule(RuleOp::Variable { scope, key, .. })) => {
                            let Some(key) = self.rule_key(&cx, target, *scope, key) else {
                                continue;
                            };
                            if facts.contains_key(&format!("var/{key:?}"))
                                || (*scope == Subject::Target
                                    && target.class == Class::Brick
                                    && world.program(target.id).is_some_and(|p| {
                                        p.rows.iter().any(|r| {
                                            r.enabled && r.input == "onRuleVariableChanged"
                                        })
                                    }))
                                || program
                                    .rows
                                    .iter()
                                    .any(|r| r.enabled && r.input == "onRuleVariableChanged")
                            {
                                return Err(F::Unsupported);
                            }
                        }
                        _ => return Err(F::Unsupported),
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn ground_causal_input(
        &self,
        bot: OwnerId,
        program: &ev::BrickProgram,
        context: Trigger,
        transitions: &[Transition],
        facts: &mut Facts,
        budget: &mut GroundingBudget,
    ) -> Result<CausalInput, planning::Failure> {
        budget.program(program)?;
        let projection =
            self.project_objective_input(bot, program, &context, transitions, facts, budget)?;
        Ok(CausalInput {
            source: SourceStamp {
                brick: program.id.index,
                rows: program.rows.clone(),
                owner: program.owner_scope,
                name: program.name.clone(),
            },
            context,
            projection,
        })
    }

    fn discover_rule_causes(
        &self,
        bot: OwnerId,
        sources: &[BrickId],
        facts: &mut Facts,
        budget: &mut GroundingBudget,
        unsupported: &mut bool,
    ) -> Result<Vec<CausalInput>, planning::Failure> {
        use planning::Failure as F;
        if sources.is_empty() {
            return Ok(Vec::new());
        }
        let world = self.events.world.as_ref().ok_or(F::Unsupported)?;
        let owner = self
            .game_of(bot)
            .and_then(|g| self.minigames.game(g).ok())
            .ok_or(F::NoPlan)?
            .owner
            .account
            .0;
        let mut causes = Vec::new();
        // These are the known native causal inputs at this baseline. Selecting
        // an available body/control method is deliberately a later operation.
        // Sources come most relevant first (`objective_candidates`): past
        // half the model's actions the rest are left out, keeping room for
        // bodies, enemies and Add-On goals.
        'sources: for id in sources {
            let Some(program) = world.program(super::super::events::id(*id)) else {
                continue;
            };
            if program.owner_scope != owner {
                continue;
            }
            for input in ["onActivate", "onRegionEnter", "onBotTouch"] {
                if !program.rows.iter().any(|r| r.enabled && r.input == input) {
                    continue;
                }
                if causes.len() >= ACTIONS / 2 {
                    break 'sources;
                }
                let previous = facts.clone();
                let cx = self
                    .input_context(
                        *id,
                        input,
                        Some(bot),
                        super::super::events::InputExtra::default(),
                        1,
                    )
                    .ok_or(F::Unsupported)?;
                let projection =
                    match self.ground_causal_input(bot, program, cx, &[], facts, budget) {
                        Ok(value) => value,
                        Err(F::ModelBudgetExceeded | F::FactBudgetExceeded) => {
                            return Err(F::ModelBudgetExceeded);
                        }
                        Err(_) => {
                            *facts = previous;
                            *unsupported = true;
                            continue;
                        }
                    };
                causes.push(projection);
            }
        }
        Ok(causes)
    }

    fn rule_desired_state(&self, bot: OwnerId) -> Result<DesiredState, planning::Failure> {
        let game = self.game_of(bot).ok_or(planning::Failure::NoPlan)?;
        let g = self
            .minigames
            .game(game)
            .map_err(|_| planning::Failure::NoPlan)?;
        if g.round_over {
            return Err(planning::Failure::NoPlan);
        }
        Ok(DesiredState {
            id: format!("rules/round/{}/{}/{bot}", game.0, g.round),
            predicates: Goal(vec![eq(win_key(bot), FactValue::Bool(true))]),
            completion: Completion::RoundWin {
                game,
                round: g.round,
                actor: bot,
            },
        })
    }

    /// Raise its team's score: tried when no plan reaches the win itself,
    /// as when an Add-On decides the win from a score its rules keep (a
    /// Slayer points limit). A team's score is the game's own measure of
    /// a side doing well; a lone player's points are not a goal of theirs.
    fn score_desired_state(&self, bot: OwnerId) -> Option<DesiredState> {
        let game = self.game_of(bot)?;
        let g = self.minigames.game(game).ok()?;
        if g.round_over {
            return None;
        }
        let team = self
            .minigames
            .player(self.peers.get(&bot)?.combat.player)
            .ok()?
            .team?;
        let from = self.team_score(game, team)?;
        let key = team_score_key(game, team);
        let team = Some(team);
        Some(DesiredState {
            id: format!("rules/score/{}/{}/{bot}", game.0, g.round),
            predicates: Goal(vec![Predicate {
                key,
                compare: Compare::AtLeast,
                value: FactValue::Number(from.saturating_add(1)),
            }]),
            completion: Completion::ScoreRise {
                game,
                round: g.round,
                team,
                actor: bot,
                from,
            },
        })
    }
    /// The team's canonical score, or the player's own without a team.
    fn score_of(
        &self,
        game: bri_minigames::GameId,
        team: Option<bri_minigames::TeamId>,
        actor: OwnerId,
    ) -> Option<i64> {
        match team {
            Some(team) => self.team_score(game, team),
            None => Some(
                self.minigames
                    .player(self.peers.get(&actor)?.combat.player)
                    .ok()?
                    .score,
            ),
        }
    }

    /// What `bot`'s objective planner sees now, for headless diagnostics
    /// (`soccer_probe`): the goals offered, the event sources it reads and
    /// their rows, the actions it grounds and how the plan comes out.
    #[doc(hidden)]
    pub fn bot_objective_report(&mut self, bot: OwnerId) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let Some(game) = self.game_of(bot) else {
            return "not in a mini-game".into();
        };
        let Ok(owner) = self.minigames.game(game).map(|g| g.owner.account.0) else {
            return "no game".into();
        };
        let indexed: Vec<_> = self
            .events
            .objective_sources
            .iter()
            .map(|(o, ids)| format!("{o:?}: {}", ids.len()))
            .collect();
        let _ = writeln!(out, "game owner {owner:?}; indexed sources by owner {indexed:?}");
        let sources = self.objective_candidates(bot, owner);
        let _ = writeln!(out, "{} candidate sources", sources.len());
        if let Some(world) = self.events.world.as_ref() {
            for id in sources.iter().take(12) {
                let Some(program) = world.program(super::super::events::id(*id)) else {
                    continue;
                };
                let _ = writeln!(out, "  brick {id:?} {:?}", program.name);
                for (row, r) in program.rows.iter().enumerate().take(8) {
                    let intent = u16::try_from(row)
                        .ok()
                        .and_then(|row| world.row_intent(super::super::events::id(*id), row));
                    let _ = writeln!(
                        out,
                        "    {} -> {:?}.{} {:?} intent {:?}",
                        r.input, r.target, r.output, r.params, intent
                    );
                }
            }
        }
        let mut budget = GroundingBudget::default();
        let discovery = match self.discover_desired_states(bot, &mut budget) {
            Ok(d) => d,
            Err(f) => {
                let _ = writeln!(out, "no goal offered: {f:?}");
                return out;
            }
        };
        let _ = writeln!(out, "goals offered (unsupported {}):", discovery.unsupported);
        for d in &discovery.candidates {
            let _ = writeln!(out, "  {d:?}");
        }
        let Some(desired) = discovery.candidates.first().cloned() else {
            return out;
        };
        let tick = self.simulation.state().tick;
        match self.objective_snapshot_with_budget(bot, tick, &[], desired, &discovery, &mut budget) {
            Ok((facts, desired, actions, _)) => {
                let _ = writeln!(out, "facts {facts:?}");
                for a in &actions {
                    let _ = writeln!(out, "  action {} cost {} effects {:?}", a.id, a.cost, a.effect_groups);
                }
                let _ = writeln!(
                    out,
                    "plan: {:?}",
                    planning::plan(&facts, &actions, &desired.predicates, limits())
                );
            }
            Err(f) => {
                let _ = writeln!(out, "no actions grounded: {f:?}");
            }
        }
        out
    }

    fn discover_desired_states(
        &mut self,
        bot: OwnerId,
        budget: &mut GroundingBudget,
    ) -> Result<DesiredDiscovery, planning::Failure> {
        budget.reserve(1, 1, 128)?;
        let mut desired = vec![self.rule_desired_state(bot)?];
        desired.extend(self.score_desired_state(bot));
        let offered = self.discover_package_objectives(bot, budget)?;
        desired.extend(offered.desired);
        Ok(DesiredDiscovery {
            candidates: desired,
            unsupported: offered.unsupported,
        })
    }

    #[cfg(test)]
    fn objective_snapshot(
        &self,
        bot: OwnerId,
        tick: u64,
        failed: &[FailedAction],
        desired: DesiredState,
    ) -> Result<Model, planning::Failure> {
        let candidates = DesiredDiscovery {
            candidates: vec![desired.clone()],
            unsupported: false,
        };
        self.objective_snapshot_with_budget(
            bot,
            tick,
            failed,
            desired,
            &candidates,
            &mut GroundingBudget::default(),
        )
    }

    fn append_physical_actions(
        &self,
        bot: OwnerId,
        sources: &[BrickId],
        facts: &mut Facts,
        budget: &mut GroundingBudget,
        unsupported: &mut bool,
    ) -> Result<Vec<GroundedAction>, planning::Failure> {
        use planning::Failure as F;
        let Some(world) = self.events.world.as_ref() else {
            return Ok(vec![]);
        };
        if !sources.iter().any(|id| {
            world
                .program(super::super::events::id(*id))
                .is_some_and(|p| {
                    p.rows
                        .iter()
                        .any(|r| r.enabled && r.input == "onObjectEnter")
                })
        }) {
            return Ok(vec![]);
        }
        let discovery =
            super::physical_objectives::discover(self, bot, budget).map_err(|e| match e {
                super::physical_objectives::Rejection::Budget => F::ModelBudgetExceeded,
                _ => F::Unsupported,
            })?;
        let mut actions = Vec::new();
        for source in sources {
            let Some(program) = world.program(super::super::events::id(*source)) else {
                continue;
            };
            if !program
                .rows
                .iter()
                .any(|r| r.enabled && r.input == "onObjectEnter")
            {
                continue;
            }
            let choices =
                super::physical_objectives::candidates(self, bot, *source, &discovery, budget)
                    .map_err(|e| match e {
                        super::physical_objectives::Rejection::Budget => F::ModelBudgetExceeded,
                        _ => F::Unsupported,
                    })?;
            for choice in choices {
                let context = self
                    .input_context(
                        *source,
                        "onObjectEnter",
                        Some(bot),
                        super::super::events::InputExtra {
                            object: Some(choice.goal.object.vehicle),
                            ..Default::default()
                        },
                        1,
                    )
                    .ok_or(F::Unsupported)?;
                let previous = facts.clone();
                let cause =
                    match self.ground_causal_input(bot, program, context, &[], facts, budget) {
                        Ok(cause) => cause,
                        Err(F::Unsupported) => {
                            // Report unknown authored semantics; never drop
                            // a goal region silently.
                            *facts = previous;
                            *unsupported = true;
                            continue;
                        }
                        Err(f) => return Err(f),
                    };
                if cause.projection.groups.is_empty() {
                    continue;
                }
                if actions.len() >= ACTIONS {
                    return Err(F::ActionBudgetExceeded);
                }
                let id = choice.key();
                budget.reserve(0, 1, id.len())?;
                actions.push(GroundedAction {
                    model: Action {
                        id,
                        cost: choice.cost,
                        preconditions: vec![],
                        effect_groups: cause.projection.groups.clone(),
                    },
                    cause: Cause::Rule(cause),
                    executor: Executor::Physical(choice),
                });
            }
        }
        Ok(actions)
    }

    /// The game creator's bricks with objective inputs this bot's model
    /// considers, at most `SOURCES` of them with at most `ROWS` rows between
    /// them: those whose rows change rules, the game or call an Add-On
    /// output (a goal's `IncScore`) first, then the rest nearest the bot.
    /// A big build (doors, lights and music bricks by the hundred) keeps the
    /// model to what is near and what scores, instead of giving up on it.
    fn objective_candidates(&self, bot: OwnerId, owner: OwnerId) -> Vec<BrickId> {
        let Some(indexed) = self.events.objective_sources.get(&owner) else {
            return Vec::new();
        };
        let Some(world) = self.events.world.as_ref() else {
            return Vec::new();
        };
        let feet = self
            .peers
            .get(&bot)
            .map_or(Vec3::ZERO, |p| Vec3::from(p.player.state().feet));
        let bricks = &self.simulation.state().bricks;
        let mut ranked: Vec<(bool, f32, BrickId, usize)> = indexed
            .iter()
            .filter_map(|id| {
                let program = world.program(super::super::events::id(*id))?;
                let scores = (0..program.rows.len()).any(|row| {
                    matches!(
                        u16::try_from(row)
                            .ok()
                            .and_then(|row| world.row_intent(super::super::events::id(*id), row)),
                        Some(Intent::Rule(_) | Intent::MiniGame(_) | Intent::Package(_))
                    )
                });
                let at = bricks
                    .get(id)
                    .map_or(Vec3::splat(f32::MAX), |b| Vec3::from(b.position));
                Some((!scores, at.distance(feet), *id, program.rows.len()))
            })
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut rows = 0usize;
        let mut out = Vec::new();
        for (_, _, id, count) in ranked {
            if out.len() >= SOURCES {
                break;
            }
            if rows + count > ROWS {
                continue;
            }
            rows += count;
            out.push(id);
        }
        out
    }

    fn objective_snapshot_with_budget(
        &self,
        bot: OwnerId,
        tick: u64,
        failed: &[FailedAction],
        desired: DesiredState,
        discovery: &DesiredDiscovery,
        budget: &mut GroundingBudget,
    ) -> Result<Model, planning::Failure> {
        use planning::Failure as F;
        let game = self.game_of(bot).ok_or(F::NoPlan)?;
        let g = self.minigames.game(game).map_err(|_| F::NoPlan)?;
        if g.round_over {
            return Err(F::NoPlan);
        }
        let sources = self.objective_candidates(bot, g.owner.account.0);
        let peer = self.peers.get(&bot).ok_or(F::NoPlan)?;
        let team = self
            .minigames
            .player(peer.combat.player)
            .map_err(|_| F::NoPlan)?
            .team;
        let mut facts = Facts::from([(win_key(bot), FactValue::Bool(false))]);
        let mut unsupported = discovery.unsupported;
        let desireds = &discovery.candidates;
        let mut actions = Vec::new();
        let mut steps = BTreeMap::new();
        let mut grounded_actions = Vec::new();
        let causes =
            self.discover_rule_causes(bot, &sources, &mut facts, budget, &mut unsupported)?;
        for cause in causes {
            let Some((executor, cost)) = BrickAction::ground(self, bot, &cause) else {
                unsupported = true;
                continue;
            };
            let action_id = format!("{}/{}", cause.source.brick, cause.context.input);
            if failed.iter().any(|f| f.action_id == action_id) {
                continue;
            }
            if grounded_actions.len() >= ACTIONS {
                break;
            }
            let model = Action {
                id: action_id.clone(),
                cost,
                preconditions: vec![],
                effect_groups: cause.projection.groups.clone(),
            };
            let grounded = GroundedAction {
                model,
                cause: Cause::Rule(cause),
                executor: Executor::Brick(executor),
            };
            grounded_actions.push(grounded);
        }
        for supplied in [
            self.append_physical_actions(bot, &sources, &mut facts, budget, &mut unsupported),
            self.append_enemy_actions(bot, &mut facts, budget),
        ] {
            let provided = match supplied {
                Ok(provided) => provided,
                Err(F::Unsupported) => {
                    unsupported = true;
                    continue;
                }
                Err(f) => return Err(f),
            };
            for grounded in provided {
                if failed.iter().any(|f| f.action_id == grounded.model.id) {
                    continue;
                }
                if grounded_actions.len() >= ACTIONS {
                    break;
                }
                grounded_actions.push(grounded);
            }
        }
        // Package and rule actions share this model's facts, prerequisites and
        // limits even when only one desired state is selected for this turn.
        for candidate in desireds {
            if let Completion::PackageCounter(stamp) = &candidate.completion {
                let provided = self.append_package_actions(bot, stamp, &mut facts, budget)?;
                for grounded in provided {
                    if failed.iter().any(|f| f.action_id == grounded.model.id) {
                        continue;
                    }
                    if grounded_actions.len() >= ACTIONS {
                        break;
                    }
                    grounded_actions.push(grounded);
                }
            }
        }
        // A retained goal may no longer be offered after its successful pickup.
        // Its validated owner binding remains the authoritative return journey.
        if !desireds.iter().any(|d| d == &desired)
            && let Completion::PackageCounter(stamp) = &desired.completion
        {
            for grounded in self.append_package_actions(bot, stamp, &mut facts, budget)? {
                if failed.iter().any(|f| f.action_id == grounded.model.id) {
                    continue;
                }
                if grounded_actions.len() >= ACTIONS {
                    break;
                }
                grounded_actions.push(grounded);
            }
        }
        for grounded in grounded_actions {
            let action_id = grounded.model.id.clone();
            let observed_origin = grounded.cause.admitted(self, bot, 0).map_or(0, |v| v.0);
            actions.push(grounded.model);
            steps.insert(
                action_id.clone(),
                Step {
                    action_id,
                    desired: desired.clone(),
                    cause: grounded.cause,
                    executor: grounded.executor,
                    phase: "approach",
                    game,
                    round: g.round,
                    team,
                    deadline: tick + APPROACH_TIMEOUT,
                    waiting: None,
                    observed_origin,
                },
            );
        }
        if actions.is_empty() {
            return Err(if unsupported {
                F::Unsupported
            } else {
                F::NoPlan
            });
        }
        self.objective_reactions_clear(bot, game, &facts, budget)?;
        Ok((facts, desired, actions, steps))
    }

    pub(super) fn bot_objective(&mut self, bot: OwnerId, tick: u64) -> Option<View> {
        if self.bots.brains[&bot]
            .kind
            .behaviours
            .get("objective")
            .copied()
            .unwrap_or(0.0)
            <= 0.0
        {
            return None;
        }
        let mut state = std::mem::take(&mut self.bots.brains.get_mut(&bot)?.objective);
        // Previous control ticks spent fighting, resting or ridden are not
        // evidence that approach failed. Waiting event due times remain real.
        state.account_control(
            tick,
            self.bots.brains[&bot].behaviour != Behaviour::Objective,
        );
        let context = self.game_of(bot).and_then(|game| {
            let g = self.minigames.game(game).ok()?;
            let team = self
                .peers
                .get(&bot)
                .and_then(|p| self.minigames.player(p.combat.player).ok())?
                .team;
            Some((game, g.round, team))
        });
        if state.desired_context != context {
            state.desired = None;
            state.desired_context = context;
            state.failed_desired.clear();
            // A new game, round or team is planned for at its next turn,
            // not after a retry wait left from before it.
            state.next = state.next.min(tick);
            state.unplanned = context.is_some();
        }
        if tick >= state.next {
            state.failed.retain(|f| {
                tick < f.until
                    && f.executor.validate(self, bot)
                    && f.cause.validate(self, bot, true)
                    && self.game_of(bot) == Some(f.game)
                    && self
                        .minigames
                        .game(f.game)
                        .is_ok_and(|g| g.round == f.round && !g.round_over)
                    && self
                        .peers
                        .get(&bot)
                        .and_then(|p| self.minigames.player(p.combat.player).ok())
                        .is_some_and(|p| p.team == f.team)
            });
        }
        if let Some(step) = state.step.as_mut() {
            step.executor.observe_controls(self, bot);
            if step.waiting.is_none()
                && matches!(&step.executor,Executor::Physical(action) if action.progressed())
            {
                step.deadline = tick.saturating_add(APPROACH_TIMEOUT);
            }
            // Getting closer to the step's point is progress too, whatever
            // provides the step: a long route keeps its deadline while it
            // advances, and only a stalled approach times out.
            if step.waiting.is_none()
                && let Some(point) = step.view(self, bot).map(|v| v.point)
                && let Some(peer) = self.peers.get(&bot)
            {
                let gap = point.distance(Vec3::from(peer.player.state().feet));
                if state.best.is_none_or(|best| gap < best - APPROACH_PROGRESS) {
                    state.best = Some(gap);
                    step.deadline = step.deadline.max(tick.saturating_add(APPROACH_TIMEOUT));
                }
            }
            if step.waiting.is_none()
                && let Some((_, when)) = step.admitted(self, bot)
            {
                step.wait_for(when);
            }
        }
        if let Some(outcome) = state
            .desired
            .as_ref()
            .and_then(|d| d.observed_completion(self, context.and_then(|c| c.2)))
        {
            let diagnostic = match &state.desired.as_ref()?.completion {
                Completion::PackageCounter(_) => "objective completed: declared package state",
                Completion::RoundWin { .. } if outcome => "objective completed: canonical winner",
                Completion::RoundWin { .. } => "objective ended without requested winner",
                Completion::ScoreRise { .. } if outcome => "objective completed: score rose",
                Completion::ScoreRise { .. } => "objective ended before the score rose",
            };
            state.step = None;
            state.desired = None;
            state.failed.clear();
            state.route.clear();
            // What it achieved may be offered again at once (a flag back on
            // its stand): look for the next objective as soon as the effects
            // of the completing event have landed, not after a retry's wait.
            state.next = tick + COMPLETION_SETTLE;
            state.paused = true;
            state.diagnostic = Some(diagnostic);
            self.bots.brains.get_mut(&bot)?.objective = state;
            return None;
        }
        let mut repair_hold = None;
        if let Some(step) = state.step.as_ref() {
            let same = step.validate(self, bot, tick % 30 == bot % 30);
            if !same {
                let diagnostic = step.invalidation_diagnostic(self, bot);
                // A body that no longer exists (scored and reset, destroyed)
                // leaves nothing to retry: ground its replacement at once.
                let replaced = step.object_gone(self);
                state.step = None;
                state.desired = None;
                state.next = if replaced { tick } else { tick + RETRY };
                state.paused = replaced;
                state.diagnostic = Some(diagnostic);
            } else if tick > step.deadline {
                state.fail(tick, "objective approach timed out");
            } else if step.progress(self, bot, tick).is_none() {
                let out = state
                    .step
                    .as_ref()
                    .and_then(|s| s.committed_view(self, bot));
                self.bots.brains.get_mut(&bot)?.objective = state;
                return out;
            } else {
                match step.progress(self, bot, tick).unwrap() {
                    Progress::Changed => {
                        // The authoritative effect changed the model. Keep a
                        // still-valid native grip for this repair turn while
                        // the fair grounding budget selects the next step.
                        // No completed rule/action is executed again.
                        if let Executor::Physical(action) = &step.executor
                            && matches!(
                                action.method,
                                super::physical_objectives::Method::Hold { .. }
                            )
                            && let Some(mut view) = step.view(self, bot)
                            && view
                                .held
                                .is_some_and(|target| self.held_by(bot) == Some(target))
                            && let Some((_, _, grip, _)) = self.bot_hold_geometry(bot)
                        {
                            view.point = self.peers[&bot].player.state().feet.into();
                            view.aim = grip;
                            view.waiting = true;
                            view.physical_progress = false;
                            repair_hold = Some(view);
                        }
                        state.paused = true;
                        state.step = None;
                        state.failed.clear();
                        state.next = tick;
                    }
                    Progress::Pending => state.fail(tick, "no observed objective progress"),
                    Progress::Unavailable => state.fail(tick, "objective observation unavailable"),
                }
            }
        }
        if self.bots.objective_budget_tick != tick {
            self.bots.objective_budget_tick = tick;
            self.bots.objective_budget_used = false;
        }
        if tick >= state.next
            && self.bots.objective_candidate == Some(bot)
            && !self.bots.objective_budget_used
        {
            self.bots.objective_budget_used = true;
            self.bots.objective_cursor = Some(bot);
            state.next = tick + RETRY;
            state.unplanned = false;
            let mut budget = GroundingBudget::default();
            // Another offered objective not yet found wanting: one that
            // cannot be planned hands the bot's next turn to it.
            let mut untried = false;
            let result = self
                .discover_desired_states(bot, &mut budget)
                .and_then(|discovery| {
                    let desireds = &discovery.candidates;
                    state.failed_desired.retain(|(d, until)| {
                        tick < *until && desireds.iter().any(|offered| offered == d)
                    });
                    if state.desired.as_ref().is_some_and(|d| match &d.completion {
                        Completion::PackageCounter(stamp) => !stamp.validate(self, bot),
                        Completion::RoundWin { .. } | Completion::ScoreRise { .. } => {
                            !desireds.iter().any(|offered| offered == d)
                        }
                    }) {
                        state.desired = None;
                    }
                    let desired = state
                        .desired
                        .clone()
                        .or_else(|| {
                            desireds
                                .iter()
                                .find(|d| {
                                    desireds.len() == 1
                                        || !state.failed_desired.iter().any(|(f, _)| f == *d)
                                })
                                .cloned()
                        })
                        .ok_or(planning::Failure::NoPlan)?;
                    untried = desireds.iter().any(|d| {
                        *d != desired && !state.failed_desired.iter().any(|(f, _)| f == d)
                    });
                    state.desired = Some(desired.clone());
                    self.objective_snapshot_with_budget(
                        bot,
                        tick,
                        &state.failed,
                        desired,
                        &discovery,
                        &mut budget,
                    )
                })
                .and_then(|(facts, desired, actions, mut steps)| {
                    let game = self.game_of(bot).ok_or(planning::Failure::NoPlan)?;
                    let round = self
                        .minigames
                        .game(game)
                        .map_err(|_| planning::Failure::NoPlan)?
                        .round;
                    let team = self
                        .peers
                        .get(&bot)
                        .and_then(|p| self.minigames.player(p.combat.player).ok())
                        .and_then(|p| p.team);
                    let context = (game, round, team);
                    if let Some(cached) = state.failed_search.as_ref()
                        && cached.matches(&facts, &actions, context, &desired)
                    {
                        state.reused = state.reused.saturating_add(1);
                        return Err(cached.failure);
                    }
                    state.searches = state.searches.saturating_add(1);
                    let route = planning::plan(&facts, &actions, &desired.predicates, limits());
                    let route = match route {
                        Ok(route) => {
                            state.failed_search = None;
                            route
                        }
                        Err(failure) => {
                            state.failed_search = Some(FailedSearch {
                                facts,
                                actions,
                                context,
                                desired: desired.clone(),
                                failure,
                            });
                            return Err(failure);
                        }
                    };
                    state.route = route.clone();
                    let step = steps
                        .remove(route.first().ok_or(planning::Failure::NoPlan)?)
                        .ok_or(planning::Failure::Unsupported)?;

                    Ok(step)
                });
            match result {
                Ok(step) => {
                    state.step = Some(step);
                    state.diagnostic = None;
                }
                Err(planning::Failure::NoPlan)
                    if state.desired.is_none() && state.diagnostic.is_some() =>
                {
                    // All declared desired candidates are cooling. Preserve the
                    // actual prior failure rather than relabeling it NoPlan.
                    state.paused = false;
                }
                Err(f) => {
                    let was_paused = std::mem::take(&mut state.paused);
                    if let Some(desired) = state.desired.take() {
                        state.failed_desired.retain(|(old, _)| old != &desired);
                        // Native rule goal plus the existing maximum eight offers.
                        if state.failed_desired.len() == 9 {
                            state.failed_desired.remove(0);
                        }
                        state.failed_desired.push((desired, tick + RETRY * 3));
                        if untried {
                            state.next = tick;
                            state.paused = was_paused;
                        }
                    }
                    state.route.clear();
                    state.diagnostic = Some(match f {
                        planning::Failure::NoPlan => "no grounded objective plan",
                        planning::Failure::Unsupported => "unsupported rule semantics",
                        planning::Failure::NodeBudgetExceeded => "objective node budget exceeded",
                        planning::Failure::ActionBudgetExceeded => {
                            "objective action budget exceeded"
                        }
                        planning::Failure::CandidateBudgetExceeded => {
                            "objective candidate budget exceeded"
                        }
                        planning::Failure::FactBudgetExceeded => "objective fact budget exceeded",
                        planning::Failure::ModelBudgetExceeded => {
                            "objective model/grounding budget exceeded"
                        }
                        planning::Failure::DepthLimitExceeded => "objective depth limit exceeded",
                        planning::Failure::InvalidActionModel => {
                            "invalid grounded objective action model"
                        }
                    });
                }
            }
        }
        if state.step.is_some() {
            state.paused = false;
        }
        if state.step.is_none() {
            state.best = None;
        }
        let result = state
            .step
            .as_ref()
            .and_then(|s| s.committed_view(self, bot))
            .or(repair_hold);
        self.bots.brains.get_mut(&bot)?.objective = state;
        result
    }

    pub(super) fn bot_objective_act(&mut self, bot: OwnerId, tick: u64) -> Result<()> {
        let Some(step) = self.bots.brains[&bot].objective.step.as_ref() else {
            return Ok(());
        };
        if step.waiting.is_some() {
            return Ok(());
        }
        if !step.validate(self, bot, true) {
            let diagnostic = step.invalidation_diagnostic(self, bot);
            self.bots
                .brains
                .get_mut(&bot)
                .unwrap()
                .objective
                .fail(tick, diagnostic);
            return Ok(());
        }
        let executor = step.executor.clone();
        let phase = executor.execute(self, bot)?;
        if let Some(step) = self
            .bots
            .brains
            .get_mut(&bot)
            .and_then(|b| b.objective.step.as_mut())
        {
            step.phase = phase;
        }
        let admitted = self.bots.brains[&bot]
            .objective
            .step
            .as_ref()
            .and_then(|s| s.admitted(self, bot));
        if let Some((_, when)) = admitted
            && let Some(step) = self
                .bots
                .brains
                .get_mut(&bot)
                .and_then(|b| b.objective.step.as_mut())
        {
            step.wait_for(when);
        }
        Ok(())
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

/// What an Add-On output does to a plan's facts, and the team whose score
/// it changes, if any.
type PackageEffects = (Vec<Effect>, Option<bri_minigames::TeamId>);
