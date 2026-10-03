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
const APPROACH_TIMEOUT: u64 = 120 * 30;

#[derive(Clone, Debug)]
pub(super) struct Step {
    pub brick: BrickId,
    pub input: &'static str,
    pub point: Vec3,
    pub aim: Vec3,
    pub rows: Vec<ev::Row>,
    owner: OwnerId,
    name: Option<String>,
    pub game: bri_minigames::GameId,
    pub round: u64,
    pub team: Option<bri_minigames::TeamId>,
    observations: Vec<Observation>,
    pub delay: u64,
    pub deadline: u64,
    pub waiting: Option<u64>,
    pub observed_origin: u64,
    destination: Vec3,
    rearming: bool,
}

#[derive(Clone, Debug)]
struct Observation {
    context: Trigger,
    target: Entity,
    condition: Condition,
    before: Datum,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct View {
    pub point: Vec3,
    pub aim: Vec3,
    pub waiting: bool,
}
impl Step {
    fn view(&self) -> View {
        View {
            point: self.point,
            aim: self.aim,
            waiting: self.waiting.is_some(),
        }
    }
}
type Model = (Facts, Vec<Action>, BTreeMap<String, Step>);
type Groups = (Vec<EffectGroup>, u64, Vec<Observation>);
#[derive(Default)]
struct GroundingBudget {
    targets: usize,
    terms: usize,
    bytes: usize,
}
impl GroundingBudget {
    fn reserve(
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
    brick: BrickId,
    input: &'static str,
    rows: Vec<ev::Row>,
    owner: OwnerId,
    name: Option<String>,
    game: bri_minigames::GameId,
    round: u64,
    team: Option<bri_minigames::TeamId>,
    until: u64,
}
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub step: Option<Step>,
    next: u64,
    failed: Vec<FailedAction>,
    pub diagnostic: Option<&'static str>,
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
    pub(super) fn ready(&self, tick: u64) -> bool {
        self.step.is_none() && tick >= self.next
    }
    pub(super) fn fail(&mut self, tick: u64, reason: &'static str) {
        if let Some(step) = self.step.take() {
            self.failed
                .retain(|f| f.brick != step.brick || f.input != step.input);
            if self.failed.len() == 8 {
                self.failed.remove(0);
            }
            self.failed.push(FailedAction {
                brick: step.brick,
                input: step.input,
                rows: step.rows,
                owner: step.owner,
                name: step.name,
                game: step.game,
                round: step.round,
                team: step.team,
                until: tick.saturating_add(APPROACH_TIMEOUT * 2),
            });
        }
        self.next = tick.saturating_add(RETRY * 3);
        self.diagnostic = Some(reason);
    }
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
    fn objective_predicate(
        &self,
        cx: &Trigger,
        target: Entity,
        c: &Condition,
        facts: &mut Facts,
    ) -> Option<Predicate> {
        let key = match c.property {
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
            Property::Score if c.subject == Subject::Team => return None,
            _ => format!(
                "observation/{}/{}/{:?}/{:?}/{:?}",
                cx.source.index, target.id.index, c.subject, c.property, c.value
            ),
        };
        let observed = self.rule_query(cx, target, c)?;
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
                let key = format!("score/{bot}");
                facts.insert(key.clone(), FactValue::Number(player.score));
                // Canonical scoring saturates. Restrict this first projection
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

    fn objective_groups(
        &self,
        bot: OwnerId,
        program: &ev::BrickProgram,
        input: &str,
        facts: &mut Facts,
        budget: &mut GroundingBudget,
    ) -> Result<Groups, planning::Failure> {
        use planning::Failure as F;
        let world = self.events.world.as_ref().ok_or(F::Unsupported)?;
        let id = program.id.index;
        // Variable-changed inputs fire on the mutation's source/target brick.
        if program
            .rows
            .iter()
            .any(|r| r.enabled && r.input == "onRuleVariableChanged")
        {
            return Err(F::Unsupported);
        }
        let cx = self
            .input_context(
                id,
                input,
                Some(bot),
                super::super::events::InputExtra::default(),
                1,
            )
            .ok_or(F::Unsupported)?;
        let mut groups = Vec::new();
        let mut ordered = Vec::new();
        let mut observations = Vec::new();
        for (index, row) in program
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.enabled && r.input == input)
        {
            let targets = world
                .row_targets(cx.source, &cx, index as u16)
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
                let mut guards = Vec::new();
                for c in &row.conditions {
                    guards.push(
                        self.objective_predicate(&cx, target, c, facts)
                            .ok_or(F::Unsupported)?,
                    );
                }
                let (effects, subject, property, key, value) = match intent {
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
                            .objective_effect(bot, &cx, target, op, facts, &mut guards)
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
                    .rule_query(&cx, target, &condition)
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
        groups.extend(ordered.into_iter().map(|(_, _, group)| group));
        Ok((groups, delay, observations))
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
    fn objective_snapshot(
        &self,
        bot: OwnerId,
        tick: u64,
        failed: &[FailedAction],
    ) -> Result<Model, planning::Failure> {
        use planning::Failure as F;
        let game = self.game_of(bot).ok_or(F::NoPlan)?;
        let g = self.minigames.game(game).map_err(|_| F::NoPlan)?;
        if g.round_over {
            return Err(F::NoPlan);
        }
        let world = self.events.world.as_ref().ok_or(F::Unsupported)?;
        let sources = self
            .events
            .objective_sources
            .get(&g.owner.account.0)
            .ok_or(F::NoPlan)?;
        if sources.len() > SOURCES {
            return Err(F::ActionBudgetExceeded);
        }
        let mut row_count = 0usize;
        // Candidate sources are indexed by actual game creator. Decorative
        // named bricks do not consume the model's discovery budget.
        for id in sources {
            if let Some(program) = world.program(super::super::events::id(*id)) {
                row_count = row_count.saturating_add(program.rows.len());
                if row_count > ROWS {
                    return Err(F::ModelBudgetExceeded);
                }
            }
        }
        let peer = self.peers.get(&bot).ok_or(F::NoPlan)?;
        let feet = Vec3::from(peer.player.state().feet);
        let team = self
            .minigames
            .player(peer.combat.player)
            .map_err(|_| F::NoPlan)?
            .team;
        let mut facts = Facts::from([(win_key(bot), FactValue::Bool(false))]);
        let mut unsupported = false;
        let mut budget = GroundingBudget::default();
        let mut actions = Vec::new();
        let mut steps = BTreeMap::new();
        for id in sources {
            let Some(program) = world.program(super::super::events::id(*id)) else {
                continue;
            };
            if program.owner_scope != g.owner.account.0 {
                continue;
            }
            for input in ["onActivate", "onRegionEnter", "onBotTouch"] {
                if !program.rows.iter().any(|r| r.enabled && r.input == input) {
                    continue;
                }
                if failed.iter().any(|f| f.brick == *id && f.input == input) {
                    continue;
                }
                if actions.len() >= ACTIONS {
                    return Err(F::ActionBudgetExceeded);
                }
                budget.program(program)?; // Before program/parameter snapshots are cloned.
                let previous = facts.clone();
                let (groups, delay, observations) =
                    match self.objective_groups(bot, program, input, &mut facts, &mut budget) {
                        Ok(value) => value,
                        Err(F::ModelBudgetExceeded | F::FactBudgetExceeded) => {
                            return Err(F::ModelBudgetExceeded);
                        }
                        Err(_) => {
                            facts = previous;
                            unsupported = true;
                            continue;
                        }
                    };
                let (min, max) = self.simulation.brick_box(*id).ok_or(F::Unsupported)?;
                let aim = (min + max) * 0.5;
                let point = if input == "onRegionEnter" {
                    let (lo, hi) = bri_world::regions::bounds(
                        self.simulation.state().bricks[id].rule_region,
                        (min, max),
                    );
                    Vec3::new((lo.x + hi.x) * 0.5, feet.y, (lo.z + hi.z) * 0.5)
                } else if input == "onBotTouch" {
                    Vec3::new(aim.x, max.y, aim.z)
                } else {
                    let d = flat(feet - aim).normalize_or(Vec3::Z);
                    Vec3::new(aim.x + d.x * 1.2, feet.y, aim.z + d.z * 1.2)
                };
                let action_id = format!("{id}/{input}");
                actions.push(Action {
                    id: action_id.clone(),
                    cost: 1
                        + (feet.distance(point) * 10.0).min(100000.0) as u32
                        + (delay.min(100000) as u32),
                    preconditions: vec![],
                    effect_groups: groups,
                });
                let destination = point;
                let touching = if input == "onRegionEnter" {
                    let (lo, hi) = bri_world::regions::bounds(
                        self.simulation.state().bricks[id].rule_region,
                        (min, max),
                    );
                    let p = feet + Vec3::Y;
                    p.cmpge(lo).all() && p.cmple(hi).all()
                } else if input == "onBotTouch" {
                    let (lo, hi) = peer.player.world_bounds();
                    Vec3::from(lo).cmple(max).all() && Vec3::from(hi).cmpge(min).all()
                } else {
                    false
                };
                let point = if touching {
                    let (_, hi) = if input == "onRegionEnter" {
                        bri_world::regions::bounds(
                            self.simulation.state().bricks[id].rule_region,
                            (min, max),
                        )
                    } else {
                        (min, max)
                    };
                    Vec3::new(hi.x + 2.0, feet.y, point.z)
                } else {
                    point
                };
                steps.insert(
                    action_id,
                    Step {
                        brick: *id,
                        input,
                        point,
                        aim,
                        rows: program.rows.clone(),
                        owner: program.owner_scope,
                        name: program.name.clone(),
                        game,
                        round: g.round,
                        team,
                        observations,
                        delay,
                        deadline: tick + APPROACH_TIMEOUT,
                        waiting: None,
                        destination,
                        rearming: touching,
                        observed_origin: self
                            .events
                            .objective_inputs
                            .get(&(*id, bot, input.into()))
                            .map_or(0, |v| v.0),
                    },
                );
            }
        }
        if actions.is_empty() {
            return Err(if unsupported {
                F::Unsupported
            } else {
                F::NoPlan
            });
        }
        self.objective_reactions_clear(bot, game, &facts, &mut budget)?;
        Ok((facts, actions, steps))
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
        if tick >= state.next {
            state.failed.retain(|f| {
                tick < f.until
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
                    && self
                        .events
                        .world
                        .as_ref()
                        .and_then(|w| w.program(super::super::events::id(f.brick)))
                        .is_some_and(|p| {
                            p.owner_scope == f.owner && p.name == f.name && p.rows == f.rows
                        })
            });
        }
        if let Some(step) = state.step.as_mut() {
            if step.rearming
                && let Some((lo, hi)) = self.simulation.brick_box(step.brick)
            {
                let outside = if step.input == "onRegionEnter" {
                    let (lo, hi) = bri_world::regions::bounds(
                        self.simulation.state().bricks[&step.brick].rule_region,
                        (lo, hi),
                    );
                    let p = Vec3::from(self.peers[&bot].player.state().feet) + Vec3::Y;
                    !p.cmpge(lo).all() || !p.cmple(hi).all()
                } else {
                    let (p, q) = self.peers[&bot].player.world_bounds();
                    !Vec3::from(p).cmple(hi).all() || !Vec3::from(q).cmpge(lo).all()
                };
                if outside {
                    step.rearming = false;
                    step.point = step.destination;
                }
            }
            if step.waiting.is_none()
                && let Some((origin, when)) =
                    self.events
                        .objective_inputs
                        .get(&(step.brick, bot, step.input.into()))
                && *origin > step.observed_origin
            {
                step.waiting = Some(when + step.delay + 8);
                step.deadline = when + step.delay + 120 * 5;
            }
        }
        if let Some(step) = state.step.as_ref() {
            let same = self
                .simulation
                .state()
                .bricks
                .get(&step.brick)
                .is_some_and(|b| b.owner == step.owner)
                && self.game_of(bot) == Some(step.game)
                && self
                    .minigames
                    .game(step.game)
                    .is_ok_and(|g| g.round == step.round && !g.round_over)
                && self
                    .peers
                    .get(&bot)
                    .and_then(|p| self.minigames.player(p.combat.player).ok())
                    .is_some_and(|p| p.team == step.team)
                && self
                    .events
                    .world
                    .as_ref()
                    .and_then(|w| w.program(super::super::events::id(step.brick)))
                    .is_some_and(|p| {
                        p.owner_scope == step.owner
                            && p.name == step.name
                            && (tick % 30 != bot % 30 || p.rows == step.rows)
                    });
            if !same {
                state.step = None;
                state.next = tick + RETRY;
                state.diagnostic = Some("objective invalidated");
            } else if tick > step.deadline {
                state.fail(tick, "objective approach timed out");
            } else if step.waiting.is_none_or(|until| tick < until) {
                let out = state.step.as_ref().map(Step::view);
                self.bots.brains.get_mut(&bot)?.objective = state;
                return out;
            } else {
                let mut available = true;
                let mut progressed = false;
                for observation in &step.observations {
                    match self.rule_query(
                        &observation.context,
                        observation.target,
                        &observation.condition,
                    ) {
                        Some(value) => progressed |= value != observation.before,
                        None => available = false,
                    }
                }
                if available && progressed {
                    state.step = None;
                    state.failed.clear();
                    state.next = tick;
                } else {
                    state.fail(
                        tick,
                        if available {
                            "no observed objective progress"
                        } else {
                            "objective observation unavailable"
                        },
                    );
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
            let result = self.objective_snapshot(bot, tick, &state.failed).and_then(
                |(facts, actions, mut steps)| {
                    let route = planning::plan(
                        &facts,
                        &actions,
                        &Goal(vec![eq(win_key(bot), FactValue::Bool(true))]),
                        planning::Limits {
                            max_nodes: 256,
                            max_depth: 12,
                            max_actions: ACTIONS,
                            max_candidates: 4096,
                            max_facts: 128,
                            max_model_terms: 4096,
                            max_model_bytes: 65536,
                        },
                    )?;
                    let step = steps
                        .remove(route.first().ok_or(planning::Failure::NoPlan)?)
                        .ok_or(planning::Failure::Unsupported)?;

                    Ok(step)
                },
            );
            match result {
                Ok(step) => {
                    state.step = Some(step);
                    state.diagnostic = None;
                }
                Err(f) => {
                    state.diagnostic = Some(match f {
                        planning::Failure::NoPlan => "no grounded objective plan",
                        planning::Failure::Unsupported => "unsupported rule semantics",
                        _ => "objective planning budget/model limit",
                    })
                }
            }
        }
        let result = state.step.as_ref().map(Step::view);
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
        if self
            .events
            .world
            .as_ref()
            .and_then(|w| w.program(super::super::events::id(step.brick)))
            .is_none_or(|p| {
                p.owner_scope != step.owner || p.name != step.name || p.rows != step.rows
            })
        {
            self.bots
                .brains
                .get_mut(&bot)
                .unwrap()
                .objective
                .fail(tick, "objective source changed");
            return Ok(());
        }
        let target_brick = step.brick;
        let target_aim = step.aim;
        let input = step.input;
        let peer = &self.peers[&bot];
        let eye = peer.player.eye();
        let delta = target_aim - eye;
        if input == "onActivate" {
            if delta.length() > 5.0 {
                return Ok(());
            }
            let direction = Vec3::new(
                peer.player.state().yaw.sin() * peer.player.state().pitch.cos(),
                peer.player.state().pitch.sin(),
                -peer.player.state().yaw.cos() * peer.player.state().pitch.cos(),
            );
            if self
                .simulation
                .target_through(eye, direction, 5.0)?
                .and_then(|(hit, _)| hit.brick)
                != Some(target_brick)
            {
                return Ok(());
            }
            if self
                .weapons
                .actor(ActorId(bot))
                .is_some_and(|a| a.selected.is_some())
            {
                self.equip_tool(bot, None)?;
                return Ok(());
            }
            let sequence = self.peers[&bot].last_sequence.saturating_add(1);
            let activated = self.command(bot, sequence, Command::Activate)?;
            let sequence = self.peers[&bot].last_sequence.saturating_add(1);
            self.command(bot, sequence, Command::ActivateRelease)?;
            if !matches!(activated,Reply::Activated(Some(id)) if id==target_brick) {
                return Ok(());
            }
        } else {
            return Ok(());
        }
        if let Some(step) = self
            .bots
            .brains
            .get_mut(&bot)
            .and_then(|b| b.objective.step.as_mut())
            && let Some((origin, when)) =
                self.events
                    .objective_inputs
                    .get(&(step.brick, bot, step.input.into()))
            && *origin > step.observed_origin
        {
            step.waiting = Some(when + step.delay + 8);
            step.deadline = when + step.delay + 120 * 5;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let (_, actions, _) = s.objective_snapshot(bot, 0, &[]).unwrap();
        assert_eq!(
            actions.len(),
            1,
            "only actual creator/game candidates grounded"
        );
    }
}
