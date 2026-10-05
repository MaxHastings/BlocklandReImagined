//! Native rule observations and semantic actions. Specialized physics, damage,
//! mini-games and package actions retain their own implementations.
use super::*;
use bri_events::{self as ev, Class, Dispatch, Entity, Slot, Trigger};
use bri_minigames as mg;
use bri_package_runtime::ops::ObjectRef;
use bri_vehicles::VehicleId;
use ev::rules::{Condition, Datum, Property, RuleOp, Subject};

const MAX_VARIABLES: usize = 8192;
const MAX_REGIONS: usize = bri_world::regions::MAX_OBSERVED_REGIONS;
const MAX_TRACE: usize = 128;
const MAX_PENDING_FACTS: usize = 256;
// Namespace, mini-game, round, subject class, subject identity, key.
pub(in crate::session) type StateKey = (u64, u64, u64, u8, u64, String);
#[derive(Default)]
pub(super) struct RuleState {
    variables: BTreeMap<StateKey, i64>,
    pending: Vec<(String, mg::GameId, Option<OwnerId>, Option<OwnerId>)>,
    occupants: BTreeSet<(BrickId, u8, u64)>,
    /// Each object's place at the last look and its relocation count then.
    previous: BTreeMap<(u8, u64), (Vec3, u64)>,
    trace: VecDeque<(BrickId, u16, String)>,
    traced: BTreeSet<BrickId>,
}

impl Session {
    fn rule_subject(&self, cx: &Trigger, target: Entity, subject: Subject) -> Option<Entity> {
        use super::events::{entity, id};
        match subject {
            Subject::SelfBrick => Some(Entity::brick(cx.source)),
            Subject::Target => Some(target),
            Subject::Player => cx
                .targets
                .get(&Slot::Player)
                .or_else(|| cx.targets.get(&Slot::Client))
                .copied(),
            Subject::Instigator => cx.targets.get(&Slot::Instigator).copied(),
            Subject::Object => cx.targets.get(&Slot::Object).copied(),
            Subject::MiniGame | Subject::Team => cx
                .targets
                .get(&Slot::MiniGame)
                .copied()
                .or_else(|| {
                    let player = cx
                        .targets
                        .get(&Slot::Player)
                        .or_else(|| cx.targets.get(&Slot::Instigator))
                        .or(cx.client.as_ref())?;
                    Some(entity(Class::MiniGame, self.game_of(player.id.index)?.0))
                })
                .map(|mut e| {
                    e.id = id(e.id.index);
                    e
                }),
        }
    }
    fn rule_actor(&self, cx: &Trigger) -> Option<OwnerId> {
        cx.targets
            .get(&Slot::Instigator)
            .or_else(|| cx.targets.get(&Slot::Player))
            .or(cx.client.as_ref())
            .map(|e| e.id.index)
    }
    pub(in crate::session) fn rule_key(
        &self,
        cx: &Trigger,
        target: Entity,
        subject: Subject,
        key: &str,
    ) -> Option<StateKey> {
        let namespace = self.simulation.state().bricks.get(&cx.source.index)?.owner;
        let e = self.rule_subject(cx, target, subject)?;
        let exists = match e.class {
            Class::Brick => self.simulation.state().bricks.contains_key(&e.id.index),
            Class::Player | Class::Client => self.peers.contains_key(&e.id.index),
            Class::MiniGame => self.minigames.game(mg::GameId(e.id.index)).is_ok(),
            Class::Vehicle => self.object_centre(ObjectRef::Vehicle(e.id.index)).is_some(),
            Class::Projectile => self.weapons.projectile(e.id.index).is_some(),
        };
        if !exists {
            return None;
        }
        let game = cx
            .targets
            .get(&Slot::MiniGame)
            .map(|e| mg::GameId(e.id.index))
            .or_else(|| self.rule_actor(cx).and_then(|p| self.game_of(p)));
        let round = game
            .and_then(|g| self.minigames.game(g).ok())
            .map_or(0, |g| g.round);
        let (class, identity) = match subject {
            Subject::Team => {
                let actor = self.rule_actor(cx)?;
                let player = self
                    .minigames
                    .player(self.peers.get(&actor)?.combat.player)
                    .ok()?;
                (4, u64::from(player.team?.0))
            }
            _ => (
                match e.class {
                    Class::Brick => 0,
                    Class::Player | Class::Client => 1,
                    Class::MiniGame => 2,
                    Class::Vehicle => 3,
                    Class::Projectile => 5,
                },
                e.id.index,
            ),
        };
        Some((
            namespace,
            game.map_or(0, |g| g.0),
            round,
            class,
            identity,
            key.to_string(),
        ))
    }
    pub(super) fn rule_query(&self, cx: &Trigger, target: Entity, c: &Condition) -> Option<Datum> {
        let entity = self.rule_subject(cx, target, c.subject);
        // A Team check that names a slot reads that team of the rule's
        // mini-game, whoever set it off.
        if let Some(slot) = c.team_slot() {
            let game = mg::GameId(entity?.id.index);
            let team = mg::TeamId(slot);
            return match c.property {
                Property::Exists => Some(Datum::Bool(
                    self.minigames
                        .game(game)
                        .is_ok_and(|g| g.teams.get(team).is_some()),
                )),
                Property::Score => self
                    .minigames
                    .team_score(game, team)
                    .ok()
                    .map(Datum::Number),
                _ => None,
            };
        }
        if c.property == Property::Exists && c.subject == Subject::Team {
            return Some(Datum::Bool(
                self.rule_actor(cx)
                    .and_then(|p| self.peers.get(&p))
                    .and_then(|p| self.minigames.player(p.combat.player).ok())
                    .is_some_and(|p| {
                        p.team.is_some()
                            && entity.is_some_and(|e| p.game == Some(mg::GameId(e.id.index)))
                    }),
            ));
        }
        if c.property == Property::Exists {
            return Some(Datum::Bool(entity.is_some_and(|e| match e.class {
                Class::Brick => self.simulation.state().bricks.contains_key(&e.id.index),
                Class::Player | Class::Client => self.peers.contains_key(&e.id.index),
                Class::MiniGame => self.minigames.game(mg::GameId(e.id.index)).is_ok(),
                Class::Vehicle => self.object_centre(ObjectRef::Vehicle(e.id.index)).is_some(),
                Class::Projectile => self.weapons.projectile(e.id.index).is_some(),
            })));
        }
        let e = entity?;
        if matches!(c.property, Property::Occupants | Property::Opponents) {
            if e.class != Class::Brick {
                return None;
            }
            let actor = self.rule_actor(cx);
            let team = actor
                .and_then(|p| self.peers.get(&p))
                .and_then(|p| self.minigames.player(p.combat.player).ok())
                .and_then(|p| p.team);
            let count = self
                .events
                .rules
                .occupants
                .iter()
                .filter(|(b, k, p)| {
                    if *b != e.id.index || *k != 0 {
                        return false;
                    }
                    if c.property == Property::Occupants {
                        return true;
                    }
                    if actor == Some(*p) {
                        return false;
                    }
                    let other = self
                        .peers
                        .get(p)
                        .and_then(|p| self.minigames.player(p.combat.player).ok())
                        .and_then(|p| p.team);
                    team.is_none() || other != team
                })
                .count();
            return Some(Datum::Number(count as i64));
        }
        if c.property == Property::Variable {
            let key = self.rule_key(cx, target, c.subject, &c.key)?;
            return Some(Datum::Number(
                *self.events.rules.variables.get(&key).unwrap_or(&0),
            ));
        }
        if c.subject == Subject::Team && c.property == Property::Score {
            let actor = self.rule_actor(cx)?;
            let player = self
                .minigames
                .player(self.peers.get(&actor)?.combat.player)
                .ok()?;
            let team = player.team?;
            let score = self.minigames.team_score(player.game?, team).ok()?;
            return Some(Datum::Number(score));
        }
        let compatible = match c.property {
            Property::IsInstigator | Property::Score | Property::Team => {
                matches!(e.class, Class::Player | Class::Client)
            }
            Property::Alive => matches!(e.class, Class::Player | Class::Client | Class::Vehicle),
            Property::RoundOver => e.class == Class::MiniGame && c.subject != Subject::Team,
            Property::Color => e.class == Class::Brick,
            Property::Kind | Property::SpawnedBy | Property::Speed => e.class == Class::Vehicle,
            _ => false,
        };
        if !compatible {
            return None;
        }
        match c.property {
            Property::IsInstigator => {
                Some(Datum::Bool(cx.targets.get(&Slot::Instigator).is_some_and(
                    |i| i.id == e.id && matches!(e.class, Class::Player | Class::Client),
                )))
            }
            Property::Alive => Some(Datum::Bool(match e.class {
                Class::Player | Class::Client => self.is_alive(e.id.index),
                Class::Vehicle => self.object_centre(ObjectRef::Vehicle(e.id.index)).is_some(),
                _ => true,
            })),
            Property::Score | Property::Team => {
                let player = self
                    .minigames
                    .player(self.peers.get(&e.id.index)?.combat.player)
                    .ok()?;
                Some(Datum::Number(if c.property == Property::Score {
                    player.score
                } else {
                    i64::from(player.team.map_or(0, |t| t.0))
                }))
            }
            Property::RoundOver => Some(Datum::Bool(
                self.minigames.game(mg::GameId(e.id.index)).ok()?.round_over,
            )),
            Property::Color => Some(Datum::Number(i64::from(
                self.simulation.state().bricks.get(&e.id.index)?.color,
            ))),
            Property::Kind => self
                .vehicle_infos()
                .into_iter()
                .find(|v| v.id == e.id.index)
                .map(|v| Datum::Text(v.definition)),
            Property::SpawnedBy => {
                let spawner = self.vehicle_spawn_brick(VehicleId(e.id.index))?;
                let brick = self.simulation.state().bricks.get(&spawner)?;
                let owner = self.simulation.state().bricks.get(&cx.source.index)?.owner;
                // Named references, like classic named targets, belong to the creator.
                // Missing/deleted/unnamed or foreign spawners cannot match, even !=.
                if brick.owner != owner {
                    return None;
                }
                brick.name.clone().map(Datum::Text)
            }
            Property::Speed => self
                .object_velocity(ObjectRef::Vehicle(e.id.index))
                .map(|v| Datum::Number(v.length().round() as i64)),
            _ => None,
        }
    }
    pub(super) fn rule_condition_value_label(
        &self,
        cx: &Trigger,
        target: Entity,
        c: &Condition,
        value: &Datum,
    ) -> String {
        if c.property == Property::Team
            && let Datum::Number(id) = value
        {
            let team = u32::try_from(*id).ok().map(mg::TeamId);
            // Use the condition's player, rather than the activating player,
            // so a Target from a different mini-game gets its own vocabulary.
            let game = self
                .rule_subject(cx, target, c.subject)
                .and_then(|e| self.game_of(e.id.index))
                .and_then(|id| self.minigames.game(id).ok());
            if let Some(name) = game.and_then(|g| team.and_then(|t| g.teams.get(t))) {
                return name.name.clone();
            }
            return if *id == 0 {
                "No team".into()
            } else {
                format!("Team {id}")
            };
        }
        value.label()
    }

    pub(super) fn rule_trace(&mut self, brick: BrickId, row: u16, text: String) {
        if !self.events.rules.traced.contains(&brick) {
            return;
        }
        let queue = &mut self.events.rules.trace;
        if queue.len() == MAX_TRACE {
            queue.pop_front();
        }
        queue.push_back((brick, row, text.chars().take(512).collect()));
    }
    pub(super) fn stop_rule_tracing(&mut self, owner: OwnerId) {
        let admin = self.is_administrator(owner);
        self.events.rules.traced.retain(|id| {
            !admin
                && self
                    .simulation
                    .state()
                    .bricks
                    .get(id)
                    .is_some_and(|b| b.owner != owner)
        });
        self.events
            .rules
            .trace
            .retain(|(id, _, _)| self.events.rules.traced.contains(id));
    }
    pub fn explain_rules(&mut self, owner: OwnerId, brick: BrickId) -> Result<()> {
        let source = self
            .simulation
            .state()
            .bricks
            .get(&brick)
            .context("Brick is gone")?;
        ensure!(
            source.owner == owner || self.is_administrator(owner),
            "Only the builder or an administrator may inspect rule traces"
        );
        ensure!(
            self.events.rules.traced.contains(&brick)
                || self.events.rules.traced.len() < MAX_REGIONS,
            "Trace subscription budget is full"
        );
        let region = self.simulation.brick_box(brick).map(|(min, max)| {
            source
                .rule_region
                .unwrap_or([(max.x - min.x).max(1.0), 4.0, (max.z - min.z).max(1.0)])
        });
        let count = self
            .events
            .rules
            .occupants
            .iter()
            .filter(|(b, _, _)| *b == brick)
            .count();
        let prefix = format!("[Events {brick}] ");
        self.notify(
            owner,
            Notice::Chat(format!("{prefix}{} saved rows", source.events.len())),
        );
        if let Some(size) = region {
            self.notify(
                owner,
                Notice::Chat(format!(
                    "{prefix}Region: {} wide, {} high, {} deep; {count} inside",
                    size[0], size[1], size[2]
                )),
            );
        }
        self.events.rules.traced.insert(brick);
        let lines: Vec<_> = self
            .events
            .rules
            .trace
            .iter()
            .filter(|(b, _, _)| *b == brick)
            .map(|(_, r, t)| format!("{prefix}Row {}: {t}", r + 1))
            .collect();
        if lines.is_empty() {
            self.notify(
                owner,
                Notice::Chat(format!(
                    "{prefix}Tracing is on. Try the event, then Refresh."
                )),
            );
        }
        let author = self.simulation.state().bricks.get(&brick).map(|b| b.owner);
        let bot_summaries: Vec<_> = self
            .bot_thoughts()
            .into_iter()
            .filter(|thought| {
                thought.objective == Some(brick)
                    || (thought.objective_diagnostic.is_some()
                        && self
                            .game_of(thought.bot)
                            .and_then(|g| self.minigames.game(g).ok())
                            .map(|g| g.owner.account.0)
                            == author)
            })
            .take(4)
            .map(|thought| {
                let name = self
                    .peers
                    .get(&thought.bot)
                    .map_or("NPC", |p| p.name.as_str());
                let status = thought
                    .objective_diagnostic
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        thought.objective_detail.as_ref().map_or_else(
                            || {
                                format!(
                                    "{}; objective brick {}",
                                    thought.behaviour,
                                    thought.objective.unwrap_or(brick)
                                )
                            },
                            |detail| {
                                format!(
                                    "{}; {} via {}: {}",
                                    detail.desired, detail.action, detail.provider, detail.phase
                                )
                            },
                        )
                    });
                format!("{prefix}[NPC {name}] {status}")
            })
            .collect();
        for summary in bot_summaries {
            self.notify(owner, Notice::Chat(summary));
        }
        for line in lines.into_iter().rev().take(12).rev() {
            self.notify(owner, Notice::Chat(line));
        }
        Ok(())
    }
    fn rule_game(&self, d: &Dispatch) -> Result<mg::GameId> {
        self.rule_game_context(&d.context, d.source, d.target)
    }
    pub(in crate::session) fn rule_game_context(
        &self,
        context: &Trigger,
        source_id: ev::Id,
        target: Entity,
    ) -> Result<mg::GameId> {
        let e = self
            .rule_subject(context, target, Subject::MiniGame)
            .context("This action needs a mini-game")?;
        let game = mg::GameId(e.id.index);
        let source = self
            .simulation
            .state()
            .bricks
            .get(&source_id.index)
            .context("Rule source is gone")?;
        let g = self
            .minigames
            .game(game)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        ensure!(
            g.owner.account.0 == source.owner,
            "Only the mini-game owner's rules may change match policy"
        );
        if matches!(target.class, Class::Player | Class::Client) {
            ensure!(
                self.game_of(target.id.index) == Some(game),
                "Target left this mini-game"
            );
        }
        Ok(game)
    }
    pub(super) fn apply_rule(&mut self, d: &Dispatch, op: &RuleOp) -> Result<()> {
        match op {
            RuleOp::Variable {
                scope,
                key,
                value,
                add,
            } => {
                let state_key = self
                    .rule_key(&d.context, d.target, *scope, key)
                    .context("Variable scope is absent")?;
                if matches!(scope, Subject::MiniGame | Subject::Team) {
                    self.rule_game(d)?;
                }
                ensure!(
                    self.events.rules.variables.contains_key(&state_key)
                        || self.events.rules.variables.len() < MAX_VARIABLES,
                    "Rule variable budget is full"
                );
                let before = *self.events.rules.variables.get(&state_key).unwrap_or(&0);
                let after = if *add {
                    before.checked_add(*value).context("Variable overflow")?
                } else {
                    *value
                };
                self.events.rules.variables.insert(state_key, after);
                if before != after {
                    self.fire_package_input(
                        if *scope == Subject::Target && d.target.class == Class::Brick {
                            d.target.id.index
                        } else {
                            d.source.index
                        },
                        "onRuleVariableChanged",
                        self.rule_actor(&d.context),
                        super::events::InputExtra {
                            game: d
                                .context
                                .targets
                                .get(&Slot::MiniGame)
                                .map(|e| mg::GameId(e.id.index)),
                            object: d.context.targets.get(&Slot::Object).map(|e| e.id.index),
                            ..Default::default()
                        },
                    );
                }
            }
            RuleOp::RegionSize(size) => {
                self.simulation
                    .mutate(d.target.id.index, |b| b.rule_region = Some(size.to_array()))?;
                self.dirty.insert(d.target.id.index);
            }
            RuleOp::Explain => {
                let owner = d
                    .client
                    .map(|e| e.id.index)
                    .context("Explain needs a player")?;
                self.explain_rules(owner, d.target.id.index)?;
            }
            RuleOp::AddScore(value) => {
                self.rule_game(d)?;
                let player = self
                    .peers
                    .get(&d.target.id.index)
                    .context("Player left")?
                    .combat
                    .player;
                let effects = self
                    .minigames
                    .event_score(player, *value, true)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            RuleOp::AddTeamScore(value) => {
                self.rule_game(d)?;
                let player = self
                    .peers
                    .get(&d.target.id.index)
                    .context("Player left")?
                    .combat
                    .player;
                ensure!(
                    self.minigames
                        .player(player)
                        .is_ok_and(|p| p.team.is_some()),
                    "Player has no team"
                );
                // Team score is the sum of canonical member scores, not a
                // parallel scoreboard. Award the toucher's real score.
                let effects = self
                    .minigames
                    .event_score(player, *value, true)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            RuleOp::TeamPoints { team, points } => {
                let game = self.rule_game(d)?;
                let effects = self
                    .minigames
                    .event_team_score(game, mg::TeamId(*team), i64::from(*points), true)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            RuleOp::TeamWin(team) => {
                let game = self.rule_game(d)?;
                let effects = self
                    .minigames
                    .end_round(game, vec![mg::TeamId(*team)], vec![])
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            RuleOp::WinRound | RuleOp::EndRound => {
                let game = self.rule_game(d)?;
                let (teams, players) = if matches!(op, RuleOp::WinRound) {
                    let p = self
                        .peers
                        .get(&d.target.id.index)
                        .context("Player left")?
                        .combat
                        .player;
                    let team = self.minigames.player(p).ok().and_then(|p| p.team);
                    (team.into_iter().collect(), vec![p])
                } else {
                    (vec![], vec![])
                };
                let effects = self
                    .minigames
                    .end_round(game, teams, players)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            RuleOp::SetTeam(team) => {
                self.rule_game(d)?;
                let p = self
                    .peers
                    .get(&d.target.id.index)
                    .context("Player left")?
                    .combat
                    .player;
                let effects = self
                    .minigames
                    .assign_team(p, Some(mg::TeamId(*team)))
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            RuleOp::ObjectVelocity(velocity) => {
                let object = ObjectRef::Vehicle(d.target.id.index);
                let source = self
                    .simulation
                    .state()
                    .bricks
                    .get(&d.source.index)
                    .context("Source is gone")?
                    .owner;
                ensure!(
                    self.may_move(source, object),
                    "Rule owner may not move this object"
                );
                let delta = *velocity - self.object_velocity(object).context("Object is gone")?;
                self.push_object(object, delta)?;
            }
            RuleOp::ResetObject => {
                let brick = self
                    .vehicle_spawn_brick(VehicleId(d.target.id.index))
                    .context("Object has no spawn brick")?;
                let owner = self
                    .simulation
                    .state()
                    .bricks
                    .get(&d.source.index)
                    .context("Source is gone")?
                    .owner;
                ensure!(
                    self.simulation
                        .state()
                        .bricks
                        .get(&brick)
                        .is_some_and(|b| self.may_edit_events_of(b.owner, owner)),
                    "The object's spawner is neither the rule owner's nor trusted to edit its events"
                );
                self.respawn_vehicle_brick(brick)?;
            }
        }
        Ok(())
    }
    pub(super) fn fire_rule_game_fact(
        &mut self,
        fact: &str,
        game: mg::GameId,
        player: Option<OwnerId>,
        killer: Option<OwnerId>,
    ) {
        if self.events.advancing {
            if self.events.rules.pending.len() < MAX_PENDING_FACTS {
                self.events
                    .rules
                    .pending
                    .push((fact.to_string(), game, player, killer));
            } else {
                super::events::note(
                    &mut self.events.diagnostics,
                    format!(
                        "MiniGame {} {fact}: deferred rule fact budget exceeded ({MAX_PENDING_FACTS} facts); fact skipped",
                        game.0,
                    ),
                );
            }
            return;
        }
        let Some(owner) = self.minigames.game(game).ok().map(|g| g.owner.account.0) else {
            return;
        };
        let bricks: Vec<_> = self
            .events
            .world
            .as_ref()
            .map(|w| w.listeners(fact))
            .unwrap_or_default()
            .into_iter()
            .map(|b| b.index)
            .filter(|b| {
                self.simulation
                    .state()
                    .bricks
                    .get(b)
                    .is_some_and(|b| b.owner == owner)
            })
            .collect();
        for brick in bricks {
            self.fire_input_with(
                brick,
                fact,
                player,
                super::events::InputExtra {
                    game: Some(game),
                    killer,
                    object: None,
                    ..Default::default()
                },
            );
        }
    }
    pub(super) fn rule_minigame_effect(&mut self, effect: &mg::Effect) {
        match effect {
            mg::Effect::Reset { game, .. } => {
                self.events.rules.variables.retain(|key, _| key.1 != game.0);
                self.fire_rule_game_fact("onRuleRoundStart", *game, None, None);
            }
            mg::Effect::RoundEnded { game, .. } => {
                self.fire_rule_game_fact("onRuleRoundEnd", *game, None, None)
            }
            mg::Effect::Spawn { player, .. } => {
                if let Some(owner) = self.owner_of(*player)
                    && let Some(game) = self.game_of(owner)
                {
                    self.fire_rule_game_fact("onRulePlayerSpawned", game, Some(owner), None);
                }
            }
            mg::Effect::Score { player, .. } => {
                if let Some(owner) = self.owner_of(*player)
                    && let Some(game) = self.game_of(owner)
                {
                    self.fire_rule_game_fact("onRuleScoreChanged", game, Some(owner), None);
                }
            }
            mg::Effect::TeamScore { game, .. } => {
                self.fire_rule_game_fact("onRuleScoreChanged", *game, None, None);
            }
            mg::Effect::Ended { game } => {
                self.events.rules.variables.retain(|key, _| key.1 != game.0)
            }
            _ => {}
        }
    }
    pub(super) fn step_rule_observations(&mut self) -> Result<()> {
        for (fact, game, player, killer) in std::mem::take(&mut self.events.rules.pending) {
            self.fire_rule_game_fact(&fact, game, player, killer);
        }
        let tick = self.simulation.state().tick;
        let Some(world) = self.events.world.as_ref() else {
            return Ok(());
        };
        let mut regions = BTreeSet::new();
        for fact in bri_world::regions::REGION_INPUTS {
            regions.extend(world.listeners(fact).into_iter().map(|b| b.index));
        }
        if regions.len() > MAX_REGIONS && tick.is_multiple_of(120) {
            super::events::note(
                &mut self.events.diagnostics,
                format!(
                    "Rule region budget exceeded: only the first {MAX_REGIONS} brick IDs are observed"
                ),
            );
        }
        let timers = if tick.is_multiple_of(120) {
            world.listeners("onRuleTimer")
        } else {
            vec![]
        };
        for brick in timers {
            let owner = self
                .simulation
                .state()
                .bricks
                .get(&brick.index)
                .map(|b| b.owner);
            let game = owner.and_then(|o| self.game_of(o));
            self.fire_input_with(
                brick.index,
                "onRuleTimer",
                None,
                super::events::InputExtra {
                    game,
                    ..Default::default()
                },
            );
        }
        // Reclaim dead identities even in pure switch/puzzle worlds. Once
        // per second is sufficient; this need not scan state every tick.
        if tick.is_multiple_of(120) {
            let bricks = &self.simulation.state().bricks;
            let peers = &self.peers;
            let vehicles = &self.vehicles.world;
            self.events.rules.variables.retain(|key, _| match key.3 {
                0 => bricks.contains_key(&key.4),
                1 => peers.contains_key(&key.4),
                3 => vehicles
                    .as_ref()
                    .is_some_and(|w| w.is_alive(VehicleId(key.4))),
                _ => true,
            });
            self.events
                .rules
                .traced
                .retain(|id| bricks.contains_key(id));
        }
        if regions.is_empty() {
            self.events.rules.previous.clear();
            self.events.rules.occupants.clear();
            return Ok(());
        }
        // (kind, id, position, credit, relocations): a changed relocation
        // count is a jump (a teleport, a respawn, a portal), which enters
        // only the region it lands in, not those on the line between.
        let mut objects: Vec<(u8, u64, Vec3, Option<u64>, u64)> = self
            .peers
            .iter()
            .filter(|(_, p)| p.combat.alive)
            .map(|(o, p)| {
                let feet = Vec3::from(p.player.state().feet);
                (0, *o, feet + Vec3::Y, Some(*o), p.player.relocations())
            })
            .collect();
        let drivers: BTreeMap<_, _> = self
            .vehicle_infos()
            .into_iter()
            .map(|v| {
                let driver = self
                    .vehicles
                    .world
                    .as_ref()
                    .and_then(|w| w.definition(&v.definition))
                    .and_then(|d| d.control_seat())
                    .and_then(|seat| v.occupants.get(seat).copied().flatten());
                (v.id, driver)
            })
            .collect();
        for v in self.vehicle_poses() {
            let driver = drivers.get(&v.id).copied().flatten();
            let by = self.mover_credit(ObjectRef::Vehicle(v.id)).or(driver);
            let relocations = self
                .vehicles
                .world
                .as_ref()
                .and_then(|w| w.relocations(VehicleId(v.id)))
                .unwrap_or_default();
            objects.push((1, v.id, Vec3::from(v.position), by, relocations));
        }
        let mut current = BTreeSet::new();
        let mut observations = vec![];
        for brick in regions.into_iter().take(MAX_REGIONS) {
            let Some(source) = self.simulation.state().bricks.get(&brick) else {
                continue;
            };
            let builder = source.owner;
            let builder_game = self.game_of(builder);
            let Some((min, max)) = self.simulation.brick_box(brick) else {
                continue;
            };
            let (lo, hi) = bri_world::regions::bounds(source.rule_region, (min, max));
            for (kind, id, position, by, relocations) in &objects {
                // Players share the builder's game, including None in free build.
                if *kind == 0 && self.game_of(*id) != builder_game {
                    continue;
                }
                // Objects remain owned-spawner observations. Uncredited objects
                // are eligible; in a match, credit from another game is excluded.
                // Free-build ownership does not depend on the mover's game.
                if *kind == 1
                    && builder_game.is_some()
                    && by.is_some_and(|p| self.game_of(p) != builder_game)
                {
                    continue;
                }
                // Objects from the builder's spawn bricks, or from those of
                // anyone who may edit the builder's events (an
                // administrator editing a loaded build).
                if *kind == 1
                    && self
                        .vehicle_spawn_brick(VehicleId(*id))
                        .and_then(|b| self.simulation.state().bricks.get(&b))
                        .is_none_or(|b| !self.may_edit_events_of(b.owner, builder))
                {
                    continue;
                }
                let key = (brick, *kind, *id);
                let inside = position.cmpge(lo).all() && position.cmple(hi).all();
                let was = self.events.rules.occupants.contains(&key);
                let prefix = if *kind == 0 { "onRegion" } else { "onObject" };
                if inside {
                    current.insert(key);
                    if !was {
                        observations.push((brick, format!("{prefix}Enter"), *by, *kind, *id));
                    } else if tick.is_multiple_of(120) {
                        observations.push((brick, format!("{prefix}Stay"), *by, *kind, *id));
                    }
                } else if was {
                    observations.push((brick, format!("{prefix}Leave"), *by, *kind, *id));
                } else if let Some((previous, then)) = self.events.rules.previous.get(&(*kind, *id))
                    && then == relocations
                    && segment_box(*previous, *position, lo, hi)
                {
                    observations.push((brick, format!("{prefix}Enter"), *by, *kind, *id));
                    observations.push((brick, format!("{prefix}Leave"), *by, *kind, *id));
                }
            }
        }
        self.events.rules.occupants = current;
        self.events.rules.previous = objects
            .iter()
            .map(|(k, id, pos, _, relocations)| ((*k, *id), (*pos, *relocations)))
            .collect();
        for (brick, fact, by, kind, object) in observations {
            let game = self
                .simulation
                .state()
                .bricks
                .get(&brick)
                .and_then(|b| self.game_of(b.owner));
            self.fire_input_with(
                brick,
                &fact,
                by,
                super::events::InputExtra {
                    game,
                    object: (kind == 1).then_some(object),
                    ..Default::default()
                },
            );
        }
        // Bound transient state even when builders delete/change objects.
        self.events
            .rules
            .traced
            .retain(|b| self.simulation.state().bricks.contains_key(b));
        Ok(())
    }
}
// Swept entry handles a fast racer crossing an entire checkpoint in one tick.
fn segment_box(start: Vec3, end: Vec3, min: Vec3, max: Vec3) -> bool {
    let delta = end - start;
    let mut near: f32 = 0.0;
    let mut far: f32 = 1.0;
    for axis in 0..3 {
        if delta[axis].abs() < 1e-6 {
            if start[axis] < min[axis] || start[axis] > max[axis] {
                return false;
            }
        } else {
            let a = (min[axis] - start[axis]) / delta[axis];
            let b = (max[axis] - start[axis]) / delta[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return false;
            }
        }
    }
    true
}

impl Session {
    /// Admin-only convenience authoring. These are ordinary editable brick
    /// programs, not hard-coded game-mode logic in Session::step.
    pub fn create_rule_lab(&mut self, owner: OwnerId, mode: &str) -> Result<Vec<BrickId>> {
        ensure!(
            self.is_administrator(owner),
            "Only an administrator may create a workshop example"
        );
        ensure!(
            [
                "puzzle", "race", "hill", "slayer", "soccer", "sandbox", "switch", "teamdoor",
                "addon"
            ]
            .contains(&mode),
            "Use /rulelab puzzle, race, hill, slayer, soccer, sandbox, switch, teamdoor or addon"
        );
        if self.game_of(owner).is_none() {
            let mut settings = mg::Settings {
                title: format!("Rule Workshop: {mode}"),
                points_kill_player: 0,
                points_kill_self: 0,
                points_die: 0,
                points_plant_brick: 0,
                points_break_brick: 0,
                ..self.minigames.catalog().defaults.clone()
            };
            // Match the ordinary MiniGame editor's available-item defaults.
            // Optional stock weapons can be disabled, and non-v20 hosts need
            // not supply the stock loadout just to create a region example.
            let mut missing_items = false;
            for item in &mut settings.loadout {
                if item
                    .as_ref()
                    .is_some_and(|id| !self.minigames.catalog().items.contains_key(id))
                {
                    *item = None;
                    missing_items = true;
                }
            }
            self.minigame_request(owner, MiniGameRequest::Create { color: 0, settings })?;
            if missing_items {
                self.private_chat(
                    owner,
                    "Some starting tools are unavailable. Choose equipment in MiniGame settings."
                        .into(),
                );
            }
        }
        let game = self.game_of(owner).context("Create a mini-game first")?;
        ensure!(
            self.minigames
                .game(game)
                .is_ok_and(|g| g.owner.account.0 == owner),
            "Leave the other host's mini-game before creating a lab"
        );
        let vehicle_bricks = self.tool_catalog.vehicle_bricks.clone();
        let ordinary = |id: &String, d: &crate::definitions::Definition| {
            d.special == crate::definitions::Special::None
                && d.bot.is_none()
                && d.link.is_none()
                && d.reflection.is_none()
                && !vehicle_bricks.contains(id)
        };
        let definition = self
            .simulation
            .definitions
            .entries
            .iter()
            .find(|(id, d)| {
                d.mesh.footprint_studs == [4, 4] && d.mesh.height_plates == 1 && ordinary(id, d)
            })
            .or_else(|| {
                self.simulation
                    .definitions
                    .entries
                    .iter()
                    .find(|(id, d)| ordinary(id, d))
            })
            .map(|(id, _)| id.clone())
            .context("No ordinary brick available")?;
        let feet = Vec3::from(self.peers[&owner].player.state().feet);
        let mut programs = lab_programs(mode);
        let mut spawn: Option<String> = None;
        if mode == "soccer" {
            spawn = self
                .vehicle_choices()
                .into_iter()
                .find(|(_, name)| {
                    name.to_ascii_lowercase().contains("steel")
                        && name.to_ascii_lowercase().contains("ball")
                })
                .map(|(id, _)| id);
            ensure!(
                spawn.is_some(),
                "Enable the Steel Ball Add-On before creating Soccer"
            );
        }
        if matches!(mode, "soccer" | "teamdoor") {
            let existing = &self
                .minigames
                .game(game)
                .map_err(|e| anyhow::anyhow!("{e}"))?
                .teams
                .list;
            if existing.len() < 2 {
                let teams = if existing.is_empty() {
                    vec![
                        mg::TeamSpec {
                            id: None,
                            name: "Blue".into(),
                            color: 0,
                        },
                        mg::TeamSpec {
                            id: None,
                            name: "Red".into(),
                            color: 1,
                        },
                    ]
                } else {
                    vec![
                        mg::TeamSpec {
                            id: Some(existing[0].id),
                            name: existing[0].name.clone(),
                            color: existing[0].color,
                        },
                        mg::TeamSpec {
                            id: None,
                            name: "Opponents".into(),
                            color: 1,
                        },
                    ]
                };
                let (_, effects) = self
                    .minigames
                    .set_teams(game, teams, false, false)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            let teams: Vec<_> = self
                .minigames
                .game(game)
                .map_err(|e| anyhow::anyhow!("{e}"))?
                .teams
                .list
                .iter()
                .map(|t| t.id)
                .collect();
            let effects = self
                .minigames
                .assign_team(self.peers[&owner].combat.player, Some(teams[0]))
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            self.apply_minigame_effects(effects)?;
            for (index, (_, rows)) in programs.iter_mut().enumerate() {
                for row in rows {
                    if mode == "teamdoor" {
                        for condition in &mut row.conditions {
                            if condition.property == Property::Team {
                                condition.value = Datum::Number(i64::from(teams[0].0));
                            }
                        }
                    } else {
                        // Each goal credits the team attacking it, by slot,
                        // whoever knocked the ball in.
                        let scorer = teams[1 - index].0;
                        if matches!(row.output.as_str(), "addTeamScore" | "winRound")
                            && let Some(ev::Value::Int(slot)) = row.params.first_mut()
                        {
                            *slot = i64::from(scorer);
                        }
                        for condition in &mut row.conditions {
                            if condition.team_slot().is_some() {
                                condition.key = scorer.to_string();
                            }
                        }
                    }
                }
            }
        }
        if mode == "soccer" {
            let definition = spawn.as_ref().unwrap();
            for (_, rows) in &mut programs {
                for row in rows {
                    row.conditions.push(Condition {
                        subject: Subject::Object,
                        property: Property::Kind,
                        key: String::new(),
                        compare: ev::rules::Compare::Equal,
                        value: Datum::Text(definition.clone()),
                    });
                }
            }
            for (_, rows) in &mut programs {
                for row in rows {
                    row.conditions.push(Condition {
                        subject: Subject::Object,
                        property: Property::SpawnedBy,
                        key: String::new(),
                        compare: ev::rules::Compare::Equal,
                        value: Datum::Text("lab_soccer_2".into()),
                    });
                }
            }
            programs.push(("Ball spawner".into(), vec![]));
            programs.push(("Practice ball spawner".into(), vec![]));
        }
        let mut bricks = vec![];
        let batch = self.simulation.state().next_brick_id;
        for (_, rows) in &mut programs {
            for row in rows {
                for condition in &mut row.conditions {
                    if condition.property == Property::SpawnedBy
                        && let Datum::Text(name) = &mut condition.value
                        && let Some(index) = name.strip_prefix(&format!("lab_{mode}_"))
                    {
                        *name = format!("lab_{mode}_{batch}_{index}");
                    }
                }
                if let ev::Target::Named(name) = &mut row.target
                    && let Some(index) = name.strip_prefix(&format!("lab_{mode}_"))
                {
                    *name = format!("lab_{mode}_{batch}_{index}");
                }
            }
        }
        for (index, (name, events)) in programs.into_iter().enumerate() {
            self.validate_event_rows(&events)?;
            let definition = if matches!(name.as_str(), "Gate panel" | "Door panel") {
                self.simulation
                    .definitions
                    .entries
                    .iter()
                    .find(|(id, d)| {
                        d.mesh.footprint_studs == [4, 1]
                            && d.mesh.height_plates >= 12
                            && matches!(d.collision.parts.as_slice(), [bri_content::collision::Part::Box { center, size }]
                                if Vec3::from(*center).length_squared() < 0.000001
                                && (Vec3::from(*size) - Vec3::new(2.0, d.mesh.height_plates as f32 * 0.2, 0.5)).length_squared() < 0.000001)
                            && ordinary(id, d)
                    })
                    .map(|(id, _)| id.clone())
                    .unwrap_or_else(|| definition.clone())
            } else if name.ends_with("ball spawner") || name == "Ball spawner" {
                self.tool_catalog
                    .vehicle_bricks
                    .iter()
                    .find(|id| self.simulation.definitions.entries.contains_key(*id))
                    .cloned()
                    .context("No vehicle spawn brick available")?
            } else {
                definition.clone()
            };
            let mesh = &self.simulation.definitions.entries[&definition].mesh;
            let lift = if matches!(name.as_str(), "Gate panel" | "Door panel") {
                mesh.height_plates as f32 * crate::grid::CELL[1] * 0.5
            } else {
                1.0
            };
            let mut at = feet + Vec3::new(6.0 + index as f32 * 7.0, lift, 0.0);
            if matches!(name.as_str(), "Gate panel" | "Door panel")
                && let Some(hit) =
                    self.simulation
                        .target(Vec3::new(at.x, feet.y + 2.0, at.z), -Vec3::Y, 8.0)?
                && hit.normal.y > 0.7
            {
                at.y = hit.position.y + lift;
            }
            let size = [
                mesh.footprint_studs[0] as f32,
                mesh.height_plates as f32,
                mesh.footprint_studs[1] as f32,
            ];
            let position = std::array::from_fn(|axis| {
                let cell = crate::grid::CELL[axis];
                let half = size[axis] * cell * 0.5;
                ((at[axis] - half) / cell).round() * cell + half
            });
            let mut brick = Brick::new(
                bri_world::ContentRef::Resolved(definition.clone()),
                position,
                owner,
            );
            brick.base_plate = true;
            brick.color = (index % self.simulation.state().palette.len().max(1)) as u8;
            brick.name = Some(format!("lab_{mode}_{batch}_{index}"));
            brick.events = events;
            if name.ends_with("ball spawner") || name == "Ball spawner" {
                brick.vehicle = spawn.clone().map(|v| {
                    Box::new(bri_world::VehicleSpawn {
                        vehicle: bri_world::ContentRef::Resolved(v),
                        recolor: true,
                    })
                });
            }
            bricks.push(brick);
        }
        let actor = self.peers[&owner].actor.clone();
        let ids = self.simulation.plant_group_floating(&actor, bricks)?;
        for id in &ids {
            self.dirty.insert(*id);
        }
        self.prepare_events();
        self.notify(
            owner,
            Notice::Chat(format!(
                "{mode} example placed 6 units east. Wrench the colored bricks to edit Events."
            )),
        );
        Ok(ids)
    }
}

/// Recipes use the public catalog and rule schema. No special-case runtime
/// names, package hooks or an engine-coded win condition.
pub fn lab_programs(mode: &str) -> Vec<(String, Vec<ev::Row>)> {
    use ev::rules::Compare;
    use ev::{Target, Value};
    fn row(
        fact: &str,
        target: Slot,
        action: &str,
        params: Vec<Value>,
        conditions: Vec<Condition>,
    ) -> ev::Row {
        ev::Row {
            conditions,
            preserved: None,
            enabled: true,
            input: fact.into(),
            delay_ms: 0,
            target: Target::Slot(target),
            output: action.into(),
            params,
        }
    }
    let variable = |scope, key: &str, value| Condition {
        subject: scope,
        property: Property::Variable,
        key: key.into(),
        compare: Compare::Equal,
        value: Datum::Number(value),
    };
    let score = |scope, value| Condition {
        subject: scope,
        property: Property::Score,
        key: String::new(),
        compare: Compare::AtLeast,
        value: Datum::Number(value),
    };
    let running = || Condition {
        subject: Subject::MiniGame,
        property: Property::RoundOver,
        key: String::new(),
        compare: Compare::Equal,
        value: Datum::Bool(false),
    };
    let exists = || ev::rules::default_condition();
    let state = |scope, key: &str, value| {
        vec![
            Value::Int(scope),
            Value::Text(key.into()),
            Value::Int(value),
        ]
    };
    match mode {
        "race" => (0..3)
            .map(|index| {
                let mut rows = vec![];
                for fact in ["onRegionEnter", "onObjectEnter"] {
                    let conditions = vec![
                        running(),
                        exists(),
                        variable(Subject::Player, "checkpoint", index),
                    ];
                    if index < 2 {
                        rows.push(row(
                            fact,
                            Slot::SelfBrick,
                            "setVariable",
                            state(1, "checkpoint", index + 1),
                            conditions,
                        ));
                    } else {
                        rows.push(row(
                            fact,
                            Slot::Player,
                            "addPlayerScore",
                            vec![Value::Int(1)],
                            conditions.clone(),
                        ));
                        rows.push(row(
                            fact,
                            Slot::Player,
                            "winRound",
                            vec![],
                            vec![
                                running(),
                                variable(Subject::Player, "checkpoint", 2),
                                score(Subject::Player, 3),
                            ],
                        ));
                        rows.push(row(
                            fact,
                            Slot::SelfBrick,
                            "setVariable",
                            state(1, "checkpoint", 0),
                            conditions,
                        ));
                    }
                }
                (format!("Checkpoint {}", index + 1), rows)
            })
            .collect(),
        "hill" => vec![(
            "Hill - score every second".into(),
            vec![
                row(
                    "onRegionStay",
                    Slot::Player,
                    "addPlayerScore",
                    vec![Value::Int(1)],
                    vec![
                        running(),
                        Condition {
                            subject: Subject::SelfBrick,
                            property: Property::Opponents,
                            key: String::new(),
                            compare: Compare::Equal,
                            value: Datum::Number(0),
                        },
                    ],
                ),
                row(
                    "onRegionStay",
                    Slot::Player,
                    "winRound",
                    vec![],
                    vec![running(), score(Subject::Player, 10)],
                ),
            ],
        )],
        "slayer" => vec![(
            "Deathmatch controller".into(),
            vec![
                row(
                    "onRulePlayerDied",
                    Slot::Instigator,
                    "addPlayerScore",
                    vec![Value::Int(1)],
                    vec![
                        running(),
                        exists(),
                        Condition {
                            subject: Subject::Player,
                            property: Property::IsInstigator,
                            key: String::new(),
                            compare: Compare::Equal,
                            value: Datum::Bool(false),
                        },
                    ],
                ),
                row(
                    "onRulePlayerDied",
                    Slot::Instigator,
                    "winRound",
                    vec![],
                    vec![running(), exists(), score(Subject::Instigator, 5)],
                ),
            ],
        )],
        // Goal 1 scores for the second team, goal 2 for the first, by slot
        // (the lab puts its game's own slots in): an own goal counts for
        // the attackers too. A ball scores once, however it bounces before
        // its reset (its Object variable), and the next ball is new.
        "soccer" => (0..2)
            .map(|index| {
                let scorer = 2 - index;
                let fresh = || variable(Subject::Object, "scored", 0);
                let mut reset = row("onObjectEnter", Slot::Object, "resetObject", vec![], vec![]);
                reset.delay_ms = 3000;
                let mut won = score(Subject::Team, 5);
                won.key = scorer.to_string();
                (
                    format!("Goal {}", index + 1),
                    vec![
                        row(
                            "onObjectEnter",
                            Slot::MiniGame,
                            "addTeamScore",
                            vec![Value::Int(scorer), Value::Int(1)],
                            vec![running(), fresh()],
                        ),
                        row(
                            "onObjectEnter",
                            Slot::MiniGame,
                            "winRound",
                            vec![Value::Int(scorer)],
                            vec![running(), fresh(), won],
                        ),
                        row(
                            "onObjectEnter",
                            Slot::SelfBrick,
                            "setVariable",
                            state(4, "scored", 1),
                            vec![],
                        ),
                        reset,
                    ],
                )
            })
            .collect(),
        "switch" => {
            let mut rows = vec![
                row(
                    "onActivate",
                    Slot::SelfBrick,
                    "cancelEvents",
                    vec![],
                    vec![],
                ),
                row(
                    "onActivate",
                    Slot::SelfBrick,
                    "setColor",
                    vec![Value::Color(1)],
                    vec![],
                ),
            ];
            for action in ["setColliding", "setRendering"] {
                let mut open = row(
                    "onActivate",
                    Slot::SelfBrick,
                    action,
                    vec![Value::Bool(false)],
                    vec![],
                );
                open.target = Target::Named("lab_switch_1".into());
                let mut close = open.clone();
                close.delay_ms = 2000;
                close.params = vec![Value::Bool(true)];
                rows.extend([open, close]);
            }
            vec![
                ("Classic switch - two-second door".into(), rows),
                ("Door panel".into(), vec![]),
            ]
        }
        "teamdoor" => {
            let allowed = || Condition {
                subject: Subject::Player,
                property: Property::Team,
                key: String::new(),
                compare: Compare::Equal,
                value: Datum::Number(1),
            };
            let mut programs = vec![];
            for (name, closed) in [("Open team door", false), ("Close team door", true)] {
                let rows = ["setColliding", "setRendering"]
                    .into_iter()
                    .map(|action| {
                        let mut r = row(
                            "onActivate",
                            Slot::SelfBrick,
                            action,
                            vec![Value::Bool(closed)],
                            vec![allowed()],
                        );
                        r.target = Target::Named("lab_teamdoor_2".into());
                        r
                    })
                    .collect();
                programs.push((name.into(), rows));
            }
            programs.push(("Gate panel".into(), vec![]));
            programs
        }
        "addon" => vec![(
            "Add-On three-way route switch".into(),
            vec![
                row("onActivate", Slot::SelfBrick, "cycleRoute", vec![], vec![]),
                row(
                    "onWorkshopRed",
                    Slot::SelfBrick,
                    "setColor",
                    vec![Value::Color(0)],
                    vec![],
                ),
                row(
                    "onWorkshopGreen",
                    Slot::SelfBrick,
                    "setColor",
                    vec![Value::Color(1)],
                    vec![],
                ),
                row(
                    "onWorkshopBlue",
                    Slot::SelfBrick,
                    "addVariable",
                    state(2, "blueSelections", 1),
                    vec![running()],
                ),
            ],
        )],
        "sandbox" => vec![
            (
                "Charged launcher".into(),
                vec![
                    row(
                        "onActivate",
                        Slot::SelfBrick,
                        "addVariable",
                        state(0, "count", 1),
                        vec![running()],
                    ),
                    row(
                        "onActivate",
                        Slot::Player,
                        "addVelocity",
                        vec![Value::Vector(Vec3::new(0., 12., 0.))],
                        vec![running(), variable(Subject::SelfBrick, "count", 3)],
                    ),
                    row(
                        "onActivate",
                        Slot::SelfBrick,
                        "setVariable",
                        state(0, "count", 0),
                        vec![running(), variable(Subject::SelfBrick, "count", 3)],
                    ),
                ],
            ),
            (
                "Third-visit bounce pad".into(),
                vec![
                    row(
                        "onRegionEnter",
                        Slot::SelfBrick,
                        "addVariable",
                        state(1, "visits", 1),
                        vec![running()],
                    ),
                    row(
                        "onRegionEnter",
                        Slot::Player,
                        "addVelocity",
                        vec![Value::Vector(Vec3::new(0., 12., 0.))],
                        vec![running(), variable(Subject::Player, "visits", 3)],
                    ),
                    row(
                        "onRegionEnter",
                        Slot::SelfBrick,
                        "setVariable",
                        state(1, "visits", 0),
                        vec![running(), variable(Subject::Player, "visits", 3)],
                    ),
                ],
            ),
            (
                "Five-second color pulse".into(),
                vec![
                    row(
                        "onRuleTimer",
                        Slot::SelfBrick,
                        "addVariable",
                        state(2, "seconds", 1),
                        vec![running()],
                    ),
                    row(
                        "onRuleTimer",
                        Slot::SelfBrick,
                        "setColor",
                        vec![Value::Color(1)],
                        vec![running(), variable(Subject::MiniGame, "seconds", 5)],
                    ),
                    row(
                        "onRuleTimer",
                        Slot::SelfBrick,
                        "setColor",
                        vec![Value::Color(0)],
                        vec![running(), variable(Subject::MiniGame, "seconds", 10)],
                    ),
                    row(
                        "onRuleTimer",
                        Slot::SelfBrick,
                        "setVariable",
                        state(2, "seconds", 0),
                        vec![running(), variable(Subject::MiniGame, "seconds", 10)],
                    ),
                ],
            ),
        ],
        _ => {
            let mut programs = vec![];
            for index in 0..3 {
                let mut rows = vec![
                    row(
                        "onActivate",
                        Slot::SelfBrick,
                        "setVariable",
                        state(2, "puzzleStep", index + 1),
                        vec![running(), variable(Subject::MiniGame, "puzzleStep", index)],
                    ),
                    row(
                        "onActivate",
                        Slot::SelfBrick,
                        "setColor",
                        vec![Value::Color(1)],
                        vec![
                            running(),
                            variable(Subject::MiniGame, "puzzleStep", index + 1),
                        ],
                    ),
                    row(
                        "onRuleRoundStart",
                        Slot::SelfBrick,
                        "setColor",
                        vec![Value::Color(0)],
                        vec![],
                    ),
                ];
                if index == 2 {
                    for action in ["setColliding", "setRendering"] {
                        let mut r = row(
                            "onActivate",
                            Slot::SelfBrick,
                            action,
                            vec![Value::Bool(false)],
                            vec![running(), variable(Subject::MiniGame, "puzzleStep", 3)],
                        );
                        r.target = Target::Named("lab_puzzle_3".into());
                        rows.push(r);
                    }
                }
                programs.push((format!("Puzzle switch {}", index + 1), rows));
            }
            programs.push((
                "Gate panel".into(),
                ["setColliding", "setRendering"]
                    .into_iter()
                    .map(|action| {
                        row(
                            "onRuleRoundStart",
                            Slot::SelfBrick,
                            action,
                            vec![Value::Bool(true)],
                            vec![],
                        )
                    })
                    .collect(),
            ));
            programs
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        definitions::{Definitions, Special},
        simulation::Simulation,
    };
    fn setup() -> (Session, OwnerId, OwnerId) {
        setup_packages(false)
    }
    fn setup_packages(addons: bool) -> (Session, OwnerId, OwnerId) {
        let pack = bri_vehicles::testing::pack_with(|d| {
            if d.id == bri_vehicles::testing::BALL {
                d.name = "Steel Ball".into();
            }
        });
        setup_vehicles(addons, pack)
    }
    fn setup_vehicles(addons: bool, pack: bri_vehicles::Pack) -> (Session, OwnerId, OwnerId) {
        setup_vehicle_weapons(addons, pack, None)
    }
    fn setup_vehicle_weapons(
        addons: bool,
        pack: bri_vehicles::Pack,
        weapons: Option<bri_weapons::Pack>,
    ) -> (Session, OwnerId, OwnerId) {
        let definitions = Definitions {
            entries: [
                (
                    "plate".into(),
                    crate::testing::definition("plate", [4, 4], 1, Special::None, false),
                ),
                (
                    "vehicle-plate".into(),
                    crate::testing::definition("vehicle-plate", [4, 4], 1, Special::None, false),
                ),
            ]
            .into(),
        };
        let simulation = Simulation::new(
            bri_world::World::new("Lab".into(), "lab".into(), vec![[1.0; 4], [0.0; 4]]),
            definitions,
            vec![
                rapier3d::prelude::ColliderBuilder::cuboid(100.0, 0.5, 100.0)
                    .translation(Vec3::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap();
        let mut s = Session::new(simulation);
        if let Some(weapons) = weapons {
            s.set_weapon_pack(weapons).unwrap();
        }
        s.set_lan_host(true);
        s.set_event_catalog(ev::testing::catalog_extended(), Vec::new())
            .unwrap();
        s.set_vehicle_pack(pack, Vec::new()).unwrap();
        s.tool_catalog.vehicle_bricks.insert("vehicle-plate".into());
        if addons {
            use bri_package::packages::{PackageEntry, PackageSet, Side};
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
            let set = PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: "rule-workshop-toys".into(),
                    version: "0.1.0".into(),
                    side: Side::Server,
                    dir: "rule-workshop-toys".into(),
                    role: None,
                }],
            };
            let catalog = bri_package_runtime::Catalog::load(&root, &set, true).unwrap();
            s.install_packages(std::sync::Arc::new(catalog), None)
                .unwrap();
        }
        let owner = s
            .join("Builder".into(), Vec3::new(0.0, 0.1, 0.0), true)
            .unwrap();
        let guest = s
            .join("Guest".into(), Vec3::new(-10.0, 0.1, 0.0), false)
            .unwrap();
        (s, owner, guest)
    }
    fn run(s: &mut Session, brick: BrickId, fact: &str, player: OwnerId) {
        s.start_event_tick(s.simulation.state().tick).unwrap();
        s.fire_input(brick, fact, Some(player));
        s.step_events(&BTreeSet::new()).unwrap();
    }
    fn join_game(s: &mut Session, owner: OwnerId, guest: OwnerId) {
        let game = s.game_of(owner).unwrap();
        s.minigame_request(guest, MiniGameRequest::Join { game: game.0 })
            .unwrap();
    }
    fn score(s: &Session, p: OwnerId) -> i64 {
        s.minigames.player(s.peers[&p].combat.player).unwrap().score
    }
    #[test]
    fn switch_recipe_uses_a_solid_upright_door_and_real_delayed_close() {
        let (mut s, owner, _) = setup();
        let mut panel =
            crate::testing::definition("upright-panel", [4, 1], 15, Special::None, false);
        panel.bot = None;
        s.simulation
            .definitions
            .entries
            .insert("upright-panel".into(), panel);
        s.simulation.definitions.entries.insert(
            "aaa-nonsolid-panel".into(),
            crate::testing::ramp("aaa-nonsolid-panel", [4, 1], 15),
        );
        for tick in 1..=60 {
            s.movement(owner, tick, crate::player::MoveInput::default())
                .unwrap();
            s.step().unwrap();
        }
        let ground = 0.0;
        let ids = s.create_rule_lab(owner, "switch").unwrap();
        s.step().unwrap(); // Normal event phase installs all named targets.
        let panel = ids[1];
        let definition = s
            .simulation
            .definitions
            .get(&s.simulation.state().bricks[&panel])
            .unwrap();
        assert_eq!(definition.mesh.footprint_studs, [4, 1]);
        assert!(definition.mesh.height_plates >= 12);
        assert!(matches!(
            definition.collision.parts.as_slice(),
            [bri_content::collision::Part::Box { .. }]
        ));
        let (lo, _) = s.simulation.brick_box(panel).unwrap();
        assert!(
            (lo.y - ground).abs() < 0.2,
            "panel reaches the creator foot plane: {lo:?}, {ground}"
        );
        assert!(s.simulation.state().bricks[&panel].colliding);
        s.events.rules.traced.insert(ids[0]);
        let cx = s
            .input_context(
                ids[0],
                "onActivate",
                Some(owner),
                super::super::events::InputExtra::default(),
                1,
            )
            .unwrap();
        assert_eq!(
            s.events
                .world
                .as_ref()
                .unwrap()
                .row_targets(cx.source, &cx, 2)
                .unwrap(),
            vec![ev::Entity::brick(super::super::events::id(panel))]
        );
        run(&mut s, ids[0], "onActivate", owner);
        assert!(
            !s.simulation.state().bricks[&panel].colliding,
            "real panel opened through authored event; diagnostics {:?}, events {:?}",
            s.take_event_diagnostics(),
            s.simulation.state().bricks[&ids[0]].events
        );
        for _ in 0..241 {
            s.simulation.step().unwrap();
        }
        s.start_event_tick(s.simulation.state().tick).unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(
            s.simulation.state().bricks[&panel].colliding,
            "real panel closed after the authored delay"
        );
    }
    #[test]
    fn workshop_never_uses_hole_bricks_for_ordinary_example_controls() {
        for mode in [
            "switch", "teamdoor", "puzzle", "race", "hill", "slayer", "soccer", "sandbox", "addon",
        ] {
            let (mut s, owner, _) = setup_packages(true);
            for (id, footprint, height) in [
                ("aaa-zombie-hole", [4, 4], 1),
                ("aaa-tall-bot-hole", [4, 1], 15),
            ] {
                let mut hole =
                    crate::testing::definition(id, footprint, height, Special::None, false);
                hole.bot = Some("zombie".into());
                s.simulation.definitions.entries.insert(id.into(), hole);
            }
            let ids = s.create_rule_lab(owner, mode).unwrap();
            for id in ids {
                let b = &s.simulation.state().bricks[&id];
                let d = s.simulation.definitions.get(b).unwrap();
                assert!(
                    d.bot.is_none(),
                    "{mode}: {} unexpectedly spawns a bot",
                    b.name.as_deref().unwrap()
                );
                assert!(d.link.is_none() && d.reflection.is_none());
                if b.vehicle.is_none() {
                    let bri_world::ContentRef::Resolved(id) = &b.definition else {
                        panic!()
                    };
                    assert!(!s.tool_catalog.vehicle_bricks.contains(id));
                }
            }
        }
    }

    #[test]
    fn workshop_example_uses_available_starting_equipment() {
        let (mut s, owner, _) = setup();
        let mut catalog = s.minigames.catalog().clone();
        catalog.defaults.respawn_ms = 17000;
        let defaults = catalog.defaults.loadout.clone();
        let unavailable = defaults.iter().flatten().next_back().unwrap().clone();
        assert!(catalog.items.remove(&unavailable).is_some());
        s.minigames.set_catalog(catalog.clone()).unwrap();
        let ids = s.create_rule_lab(owner, "hill").unwrap();
        assert!(!ids.is_empty());
        let settings = &s
            .minigames
            .game(s.game_of(owner).unwrap())
            .unwrap()
            .settings;
        assert_eq!(settings.respawn_ms, 17000, "server policy must remain");
        for (before, after) in defaults.iter().zip(&settings.loadout) {
            if before.as_ref() == Some(&unavailable) {
                assert_eq!(after, &None);
            } else {
                assert_eq!(after, before, "available equipment must remain");
            }
        }
        assert!(!s.minigames.catalog().items.contains_key(&unavailable));
        assert!(s.take_private_notices().iter().any(|(who, notice)| {
            *who == owner
                && matches!(notice, Notice::Chat(text) if text.contains("Choose equipment"))
        }));
    }
    #[test]
    fn timer_variable_change_keeps_match_context_without_an_actor() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "sandbox").unwrap();
        let brick = ids[2];
        let mut row = lab_programs("hill")[0].1[1].clone();
        row.input = "onRuleVariableChanged".into();
        row.target = ev::Target::Slot(Slot::MiniGame);
        row.output = "endRound".into();
        row.conditions = vec![Condition {
            subject: Subject::MiniGame,
            property: Property::Variable,
            key: "seconds".into(),
            compare: ev::rules::Compare::Equal,
            value: Datum::Number(2),
        }];
        s.simulation.mutate(brick, |b| b.events.push(row)).unwrap();
        s.dirty.insert(brick);
        for second in 1..=2 {
            for _ in 0..120 {
                s.simulation.step().unwrap();
            }
            s.start_event_tick(s.simulation.state().tick).unwrap();
            s.step_rule_observations().unwrap();
            s.step_events(&BTreeSet::new()).unwrap();
            s.step_events(&BTreeSet::new()).unwrap();
            assert_eq!(
                s.minigames.game(s.game_of(p).unwrap()).unwrap().round_over,
                second == 2
            );
        }
    }
    #[test]
    fn named_brick_state_change_fires_on_the_changed_brick() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "switch").unwrap();
        let mut write = lab_programs("puzzle")[0].1[0].clone();
        write.output = "setVariable".into();
        write.params = vec![
            ev::Value::Int(0),
            ev::Value::Text("clicks".into()),
            ev::Value::Int(3),
        ];
        write.conditions.clear();
        write.target =
            ev::Target::Named(s.simulation.state().bricks[&ids[1]].name.clone().unwrap());
        let mut reaction = lab_programs("puzzle")[0].1[1].clone();
        reaction.input = "onRuleVariableChanged".into();
        reaction.conditions = vec![Condition {
            subject: Subject::SelfBrick,
            property: Property::Variable,
            key: "clicks".into(),
            compare: ev::rules::Compare::Equal,
            value: Datum::Number(3),
        }];
        s.simulation
            .mutate(ids[0], |b| b.events = vec![write])
            .unwrap();
        s.simulation
            .mutate(ids[1], |b| {
                b.color = 0;
                b.events = vec![reaction];
            })
            .unwrap();
        s.dirty.extend(ids.iter().copied());
        run(&mut s, ids[0], "onActivate", p);
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(s.simulation.state().bricks[&ids[1]].color, 1);
    }
    #[test]
    fn deleted_brick_variables_are_reclaimed_without_active_regions() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "puzzle").unwrap();
        let mut write = lab_programs("puzzle")[0].1[0].clone();
        write.params[0] = ev::Value::Int(0);
        write.conditions.clear();
        s.simulation
            .mutate(ids[0], |b| b.events = vec![write])
            .unwrap();
        s.dirty.insert(ids[0]);
        run(&mut s, ids[0], "onActivate", p);
        assert!(!s.events.rules.variables.is_empty());
        let actor = s.peers[&p].actor.clone();
        s.simulation.remove(&actor, ids[0]).unwrap();
        for _ in 0..120 {
            s.simulation.step().unwrap();
        }
        s.step_rule_observations().unwrap();
        assert!(s.events.rules.variables.is_empty());
    }
    #[test]
    fn unrelated_identity_classes_cannot_supply_condition_values() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "puzzle").unwrap();
        let mut cx = Trigger::new(super::super::events::id(ids[0]), "onActivate", 1);
        cx.targets
            .insert(Slot::Player, super::super::events::entity(Class::Player, p));
        let target = Entity::brick(cx.source);
        for property in [
            Property::Score,
            Property::Team,
            Property::Kind,
            Property::Speed,
            Property::RoundOver,
        ] {
            let condition = Condition {
                subject: Subject::SelfBrick,
                property,
                key: String::new(),
                compare: ev::rules::Compare::NotEqual,
                value: Datum::Number(9),
            };
            assert_eq!(s.rule_query(&cx, target, &condition), None);
            assert!(!condition.matches(None));
        }
        let condition = Condition {
            subject: Subject::Team,
            property: Property::Exists,
            key: String::new(),
            compare: ev::rules::Compare::Equal,
            value: Datum::Bool(false),
        };
        assert_eq!(
            s.rule_query(&cx, target, &condition),
            Some(Datum::Bool(false))
        );
    }
    #[test]
    fn classic_switch_named_target_and_reactivation_delay_remain_familiar() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "switch").unwrap();
        run(&mut s, ids[0], "onActivate", p);
        assert!(!s.simulation.state().bricks[&ids[1]].colliding);
        for _ in 0..120 {
            s.simulation.step().unwrap();
        }
        run(&mut s, ids[0], "onActivate", p);
        for _ in 0..120 {
            s.simulation.step().unwrap();
        }
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(!s.simulation.state().bricks[&ids[1]].colliding);
        for _ in 0..120 {
            s.simulation.step().unwrap();
        }
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(s.simulation.state().bricks[&ids[1]].colliding);
    }
    #[test]
    fn team_door_opens_and_closes_for_blue_but_rejects_red() {
        let (mut s, p, q) = setup();
        let ids = s.create_rule_lab(p, "teamdoor").unwrap();
        join_game(&mut s, p, q);
        let game = s.game_of(p).unwrap();
        let red = s.minigames.game(game).unwrap().teams.list[1].id;
        let effects = s
            .minigames
            .assign_team(s.peers[&q].combat.player, Some(red))
            .unwrap();
        s.apply_minigame_effects(effects).unwrap();
        s.events.rules.traced.insert(ids[0]);
        run(&mut s, ids[0], "onActivate", q);
        assert!(
            s.events
                .rules
                .trace
                .iter()
                .any(|(_, _, text)| text.contains("Player Team = Blue (current: Red) - skipped")),
            "{:?}",
            s.events.rules.trace
        );
        assert!(s.simulation.state().bricks[&ids[2]].colliding);
        run(&mut s, ids[0], "onActivate", p);
        assert!(!s.simulation.state().bricks[&ids[2]].colliding);
        assert!(!s.simulation.state().bricks[&ids[2]].visible);
        run(&mut s, ids[1], "onActivate", q);
        assert!(!s.simulation.state().bricks[&ids[2]].colliding);
        run(&mut s, ids[1], "onActivate", p);
        assert!(s.simulation.state().bricks[&ids[2]].colliding);
        assert!(s.simulation.state().bricks[&ids[2]].visible);
    }
    #[test]
    fn delayed_team_door_checks_the_players_current_team() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "teamdoor").unwrap();
        let game = s.game_of(p).unwrap();
        let red = s.minigames.game(game).unwrap().teams.list[1].id;
        s.simulation
            .mutate(ids[0], |b| {
                for row in &mut b.events {
                    row.delay_ms = 1000;
                }
            })
            .unwrap();
        s.dirty.insert(ids[0]);
        run(&mut s, ids[0], "onActivate", p);
        let effects = s
            .minigames
            .assign_team(s.peers[&p].combat.player, Some(red))
            .unwrap();
        s.apply_minigame_effects(effects).unwrap();
        for _ in 0..120 {
            s.simulation.step().unwrap();
        }
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(
            s.simulation.state().bricks[&ids[2]].colliding,
            "Blue at activation, Red when due: door stays closed"
        );
    }
    #[test]
    fn credited_own_goal_resets_the_ball_without_awarding_points() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "soccer").unwrap();
        s.respawn_vehicle_brick(ids[2]).unwrap();
        let object = s.vehicle_infos()[0].id;
        let center = s.object_centre(ObjectRef::Vehicle(object)).unwrap();
        s.simulation.mutate(ids[1], |b| b.events.clear()).unwrap();
        s.simulation
            .mutate(ids[0], |b| {
                b.position = center.to_array();
                b.rule_region = Some([10.; 3]);
            })
            .unwrap();
        s.dirty.extend([ids[0], ids[1]]);
        s.credit(ObjectRef::Vehicle(object), p);
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(score(&s, p), 0);
        assert!(
            s.pending_events() > 0,
            "reset remains scheduled independently of who scored"
        );
    }
    #[test]
    fn native_projectile_push_credits_real_object_entry_but_zero_impulse_does_not() {
        use crate::player::MoveInput;
        for impulse in [1800.0, 0.0] {
            let pack = bri_vehicles::testing::pack_with(|d| {
                if d.id == bri_vehicles::testing::BALL {
                    d.name = "Steel Ball".into();
                }
            });
            let mut weapons = bri_weapons::testing::pack();
            let shot = weapons
                .projectiles
                .get_mut(bri_weapons::testing::GUN_PROJECTILE)
                .unwrap();
            shot.damage = 0.0;
            shot.impulse = impulse;
            shot.brick.direct = false;
            let (mut s, owner, _) = setup_vehicle_weapons(false, pack, Some(weapons));
            let ids = s.create_rule_lab(owner, "soccer").unwrap();
            // Author a single unambiguous shooting lane before simulation.
            s.simulation
                .mutate(ids[3], |b| b.position = [-30.0, 1.1, -30.0])
                .unwrap();
            s.respawn_vehicle_brick(ids[2]).unwrap();
            s.respawn_vehicle_brick(ids[3]).unwrap();
            let object = s
                .vehicle_infos()
                .into_iter()
                .find(|v| s.vehicle_spawn_brick(VehicleId(v.id)) == Some(ids[2]))
                .unwrap()
                .id;
            let spawn_name = s.simulation.state().bricks[&ids[2]].name.clone().unwrap();
            let conditions = vec![
                Condition {
                    subject: Subject::Object,
                    property: Property::SpawnedBy,
                    key: String::new(),
                    compare: ev::rules::Compare::Equal,
                    value: Datum::Text(spawn_name),
                },
                Condition {
                    subject: Subject::Instigator,
                    property: Property::Exists,
                    key: String::new(),
                    compare: ev::rules::Compare::Equal,
                    value: Datum::Bool(true),
                },
            ];
            let start = s.object_centre(ObjectRef::Vehicle(object)).unwrap();
            let goal = start + Vec3::X * 6.0;
            let mut win = lab_programs("soccer")[0].1[0].clone();
            win.output = "winRound".into();
            win.target = ev::Target::Slot(Slot::Instigator);
            win.params.clear();
            win.conditions = conditions;
            s.simulation
                .mutate(ids[0], |b| {
                    b.position = goal.to_array();
                    b.rule_region = Some([2.0, 12.0, 8.0]);
                    b.colliding = false;
                    b.events = vec![win];
                })
                .unwrap();
            s.simulation.mutate(ids[1], |b| b.events.clear()).unwrap();
            s.dirty.extend(ids.iter().copied());
            let slot = s.give_item(owner, bri_weapons::testing::GUN_ITEM).unwrap();
            s.equip_tool(owner, Some(slot)).unwrap();
            let mut sequence = 0;
            for _ in 0..60 {
                sequence += 1;
                s.movement(owner, sequence, MoveInput::default()).unwrap();
                s.step().unwrap();
            }
            let start = s.object_centre(ObjectRef::Vehicle(object)).unwrap();
            let eye = Vec3::from(s.peers[&owner].player.state().feet) + Vec3::Y * 2.4;
            let direction = (start - eye).normalize();
            let look = MoveInput {
                yaw: direction.x.atan2(-direction.z),
                pitch: direction.y.asin(),
                ..Default::default()
            };
            sequence += 1;
            s.movement(owner, sequence, look).unwrap();
            s.step().unwrap();
            s.command(owner, 1, Command::WeaponTrigger { down: true })
                .unwrap();
            for tick in 0..600 {
                if tick == 1 {
                    s.command(owner, 2, Command::WeaponTrigger { down: false })
                        .unwrap();
                }
                sequence += 1;
                s.movement(owner, sequence, look).unwrap();
                s.step().unwrap();
            }
            let end = s.object_centre(ObjectRef::Vehicle(object)).unwrap();
            if impulse > 0.0 {
                assert!(
                    end.x - start.x > 5.0,
                    "native projectile moved body: {start:?} -> {end:?}"
                );
                assert!(
                    s.round_results().any(|r| r.owners == vec![owner]),
                    "actual object entry must name its shooter as the canonical winner"
                );
            } else {
                assert!((end.x - start.x).abs() < 0.1);
                assert_eq!(s.mover_credit(ObjectRef::Vehicle(object)), None);
                assert_eq!(s.round_results().count(), 0);
            }
        }
    }

    #[test]
    fn object_region_credits_the_authored_driver_in_a_reordered_seat_layout() {
        let pack = bri_vehicles::testing::pack_with(|d| {
            if d.id == bri_vehicles::testing::BALL {
                d.name = "Steel Ball".into();
            } else if d.id == bri_vehicles::testing::CAR {
                d.seats.swap(0, 2);
            }
        });
        let (mut s, passenger, driver) = setup_vehicles(false, pack);
        let ids = s.create_rule_lab(passenger, "soccer").unwrap();
        join_game(&mut s, passenger, driver);
        s.simulation
            .mutate(ids[2], |b| {
                b.vehicle.as_mut().unwrap().vehicle =
                    bri_world::ContentRef::Resolved(bri_vehicles::testing::CAR.into());
            })
            .unwrap();
        s.respawn_vehicle_brick(ids[2]).unwrap();
        let object = VehicleId(s.vehicle_infos()[0].id);
        let world = s.vehicles.world.as_mut().unwrap();
        for (seat, owner) in [(0, passenger), (2, driver)] {
            let at = world
                .vehicle_snapshot(&s.simulation.physics, object)
                .unwrap()
                .seats[seat]
                .transform
                .position;
            world
                .mount(
                    &s.simulation.physics,
                    object,
                    seat,
                    bri_vehicles::Occupant {
                        id: bri_vehicles::OccupantId(owner),
                        owner: bri_vehicles::OwnerId(owner),
                        body: [1.0, 2.0],
                    },
                    at,
                )
                .unwrap();
        }
        let center = s.object_centre(ObjectRef::Vehicle(object.0)).unwrap();
        let row = goal_row(
            ev::Target::Slot(Slot::Instigator),
            "addPlayerScore",
            vec![ev::Value::Int(1)],
        );
        s.simulation
            .mutate(ids[0], |b| {
                b.position = center.to_array();
                b.rule_region = Some([10.0; 3]);
                b.events = vec![row];
            })
            .unwrap();
        s.simulation.mutate(ids[1], |b| b.events.clear()).unwrap();
        s.dirty.extend([ids[0], ids[1], ids[2]]);
        assert_eq!(s.mover_credit(ObjectRef::Vehicle(object.0)), None);
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(
            score(&s, driver),
            1,
            "the authored control seat supplies attribution"
        );
        assert_eq!(
            score(&s, passenger),
            0,
            "seat zero is a passenger in this layout"
        );
    }
    #[test]
    fn charge_is_shared_by_clickers_but_visit_bounce_is_per_player() {
        let (mut s, p, q) = setup();
        let ids = s.create_rule_lab(p, "sandbox").unwrap();
        join_game(&mut s, p, q);
        s.events.rules.traced.extend([ids[0], ids[1]]);
        run(&mut s, ids[0], "onActivate", p);
        run(&mut s, ids[0], "onActivate", q);
        assert!(
            !s.events
                .rules
                .trace
                .iter()
                .any(|(_, _, t)| t.starts_with("addVelocity") && t.ends_with("ran"))
        );
        run(&mut s, ids[0], "onActivate", p);
        assert_eq!(
            s.events
                .rules
                .trace
                .iter()
                .filter(|(_, _, t)| t.starts_with("addVelocity") && t.ends_with("ran"))
                .count(),
            1
        );
        s.events.rules.trace.clear();
        run(&mut s, ids[1], "onRegionEnter", p);
        run(&mut s, ids[1], "onRegionEnter", q);
        run(&mut s, ids[1], "onRegionEnter", p);
        assert!(
            !s.events
                .rules
                .trace
                .iter()
                .any(|(_, _, t)| t.starts_with("addVelocity") && t.ends_with("ran"))
        );
        run(&mut s, ids[1], "onRegionEnter", p);
        assert_eq!(
            s.events
                .rules
                .trace
                .iter()
                .filter(|(_, _, t)| t.starts_with("addVelocity") && t.ends_with("ran"))
                .count(),
            1
        );
    }
    #[test]
    fn game_timer_pulses_color_and_repeats_without_scoring_or_winning() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "sandbox").unwrap();
        s.simulation.mutate(ids[2], |b| b.color = 0).unwrap();
        for second in 1..=15 {
            run(&mut s, ids[2], "onRuleTimer", p);
            assert_eq!(
                s.simulation.state().bricks[&ids[2]].color,
                u8::from((5..10).contains(&second) || second == 15)
            );
        }
        assert_eq!(score(&s, p), 0);
        assert!(!s.minigames.game(s.game_of(p).unwrap()).unwrap().round_over);
    }
    #[test]
    fn puzzle_requires_three_distinct_switches_in_order_and_reset_recloses_gate() {
        let (mut s, p, q) = setup();
        let ids = s.create_rule_lab(p, "puzzle").unwrap();
        join_game(&mut s, p, q);
        let game = s.game_of(p).unwrap();
        run(&mut s, ids[2], "onActivate", q);
        assert!(s.simulation.state().bricks[&ids[3]].colliding);
        run(&mut s, ids[0], "onActivate", p);
        run(&mut s, ids[0], "onActivate", p);
        run(&mut s, ids[2], "onActivate", q);
        assert!(s.simulation.state().bricks[&ids[3]].colliding);
        run(&mut s, ids[1], "onActivate", q);
        run(&mut s, ids[2], "onActivate", p);
        assert!(!s.simulation.state().bricks[&ids[3]].colliding);
        assert!(!s.simulation.state().bricks[&ids[3]].visible);
        assert!(
            !s.minigames.game(game).unwrap().round_over,
            "puzzles unlock things without ending the match"
        );
        let effects = s
            .minigames
            .execute(mg::Command::Reset {
                game,
                authority: mg::EventAuthority::System,
            })
            .unwrap();
        s.apply_minigame_effects(effects).unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(s.simulation.state().bricks[&ids[3]].colliding);
        assert!(s.simulation.state().bricks[&ids[3]].visible);
        assert!(s.events.rules.variables.is_empty());
    }
    #[test]
    fn ordered_checkpoint_race_is_per_player_and_finishes_three_laps() {
        let (mut s, p, q) = setup();
        let ids = s.create_rule_lab(p, "race").unwrap();
        join_game(&mut s, p, q);
        run(&mut s, ids[2], "onRegionEnter", p);
        assert_eq!(score(&s, p), 0);
        for lap in 0..3 {
            run(&mut s, ids[0], "onRegionEnter", p);
            run(&mut s, ids[2], "onRegionEnter", q);
            assert_eq!(score(&s, q), 0);
            run(&mut s, ids[1], "onRegionEnter", p);
            run(&mut s, ids[2], "onRegionEnter", p);
            assert_eq!(score(&s, p), lap + 1);
        }
        assert!(s.minigames.game(s.game_of(p).unwrap()).unwrap().round_over);
    }
    #[test]
    fn hill_stops_scoring_after_win() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "hill").unwrap();
        for _ in 0..20 {
            run(&mut s, ids[0], "onRegionStay", p);
        }
        assert_eq!(score(&s, p), 10);
    }
    #[test]
    fn slayer_credits_killer_and_does_not_score_environment_or_self_death() {
        let (mut s, p, q) = setup();
        s.create_rule_lab(p, "slayer").unwrap();
        join_game(&mut s, p, q);
        let game = s.game_of(p).unwrap();
        s.fire_rule_game_fact("onRulePlayerDied", game, Some(q), None);
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(score(&s, q), 0);
        s.fire_rule_game_fact("onRulePlayerDied", game, Some(q), Some(q));
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(score(&s, q), 0);
        for _ in 0..5 {
            s.fire_rule_game_fact("onRulePlayerDied", game, Some(q), Some(p));
            s.step_events(&BTreeSet::new()).unwrap();
        }
        assert_eq!(score(&s, p), 5);
        assert!(s.minigames.game(game).unwrap().round_over);
    }
    #[test]
    fn delayed_rule_rechecks_conditions_after_round_end() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "sandbox").unwrap();
        let brick = ids[0];
        let mut row = lab_programs("puzzle")[0].1[0].clone();
        row.delay_ms = 1000;
        s.simulation
            .mutate(brick, |b| b.events = vec![row])
            .unwrap();
        s.dirty.insert(brick);
        s.fire_input(brick, "onActivate", Some(p));
        assert!(s.pending_events() > 0);
        let game = s.game_of(p).unwrap();
        let effects = s.minigames.end_round(game, vec![], vec![]).unwrap();
        s.apply_minigame_effects(effects).unwrap();
        for _ in 0..120 {
            s.simulation.step().unwrap();
        }
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(s.events.rules.variables.is_empty());
    }
    #[test]
    fn authored_conditions_and_region_dimensions_survive_save_and_copy() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "race").unwrap();
        let brick = ids[0];
        s.simulation
            .mutate(brick, |b| b.rule_region = Some([5.0, 6.0, 7.0]))
            .unwrap();
        let authored = &s.simulation.state().bricks[&brick];
        let packed = bri_world::packed::Packed::pack(std::iter::once((brick, Some(authored))));
        let decoded = packed.unpack(100).unwrap();
        assert_eq!(decoded[0].1.as_ref().unwrap(), authored);
        let extras = crate::blueprint::CopyExtras::of(0, authored).unwrap();
        let mut copied = Brick::new(authored.definition.clone(), [0.0, 0.0, 0.0], p);
        extras.put_on(&mut copied);
        assert_eq!(copied.events, authored.events);
        assert_eq!(copied.rule_region, authored.rule_region);
    }
    #[test]
    fn non_admin_cannot_scaffold_and_rule_authority_does_not_cross_games() {
        let (mut s, p, q) = setup();
        assert!(s.create_rule_lab(q, "race").is_err());
        let ids = s.create_rule_lab(p, "hill").unwrap();
        run(&mut s, ids[0], "onRegionStay", q);
        assert_eq!(score(&s, q), 0);
    }
    #[test]
    fn real_region_observation_blocks_contested_hill_and_resumes_when_clear() {
        let (mut s, p, q) = setup();
        let ids = s.create_rule_lab(p, "hill").unwrap();
        join_game(&mut s, p, q);
        let center = Vec3::from(s.simulation.state().bricks[&ids[0]].position);
        for actor in [p, q] {
            s.peers.get_mut(&actor).unwrap().player.place(
                &mut s.simulation.physics,
                center - Vec3::Y,
                0.0,
                Vec3::ZERO,
            );
        }
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        for _ in 0..120 {
            s.simulation.step().unwrap();
        }
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(score(&s, p), 0);
        assert_eq!(score(&s, q), 0);
        s.peers.get_mut(&q).unwrap().player.place(
            &mut s.simulation.physics,
            Vec3::new(-20.0, 0.1, 0.0),
            0.0,
            Vec3::ZERO,
        );
        for _ in 0..120 {
            s.simulation.step().unwrap();
        }
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(score(&s, p), 1);
    }
    #[test]
    fn free_build_region_observes_free_build_players_but_not_other_games() {
        let (mut s, p, q) = setup();
        let ids = s.create_rule_lab(p, "hill").unwrap();
        s.minigame_request(p, MiniGameRequest::Leave).unwrap();
        s.minigame_request(
            q,
            MiniGameRequest::Create {
                color: 0,
                settings: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(s.game_of(p), None);
        assert!(s.game_of(q).is_some());
        let mut enter = lab_programs("puzzle")[0].1[1].clone();
        enter.input = "onRegionEnter".into();
        enter.conditions.clear();
        s.simulation
            .mutate(ids[0], |b| {
                b.events = vec![enter];
                b.color = 0;
            })
            .unwrap();
        s.dirty.insert(ids[0]);
        s.step_events(&BTreeSet::new()).unwrap();
        let center = Vec3::from(s.simulation.state().bricks[&ids[0]].position);
        for (actor, position) in [(p, center + Vec3::X * 20.), (q, center - Vec3::Y)] {
            s.peers.get_mut(&actor).unwrap().player.place(
                &mut s.simulation.physics,
                position,
                0.,
                Vec3::ZERO,
            );
        }
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(!s.events.rules.occupants.contains(&(ids[0], 0, q)));
        assert_eq!(
            s.simulation.state().bricks[&ids[0]].color,
            0,
            "another game's player must not fire the free-build region's action"
        );
        s.peers.get_mut(&p).unwrap().player.place(
            &mut s.simulation.physics,
            center - Vec3::Y,
            0.,
            Vec3::ZERO,
        );
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(s.events.rules.occupants.contains(&(ids[0], 0, p)));
        assert_eq!(s.simulation.state().bricks[&ids[0]].color, 1);
        s.minigame_request(q, MiniGameRequest::Leave).unwrap();
        s.peers.get_mut(&q).unwrap().player.place(
            &mut s.simulation.physics,
            center - Vec3::Y,
            0.,
            Vec3::ZERO,
        );
        s.step_rule_observations().unwrap();
        assert!(s.events.rules.occupants.contains(&(ids[0], 0, q)));
    }

    #[test]
    fn free_build_object_observation_keeps_owned_spawners_regardless_of_mover_game() {
        let (mut s, p, q) = setup();
        // q is a stranger to p's bricks (a LAN host trusts everyone).
        s.set_lan_host(false);
        let ids = s.create_rule_lab(p, "soccer").unwrap();
        s.respawn_vehicle_brick(ids[2]).unwrap();
        s.minigame_request(p, MiniGameRequest::Leave).unwrap();
        s.minigame_request(
            q,
            MiniGameRequest::Create {
                color: 0,
                settings: Default::default(),
            },
        )
        .unwrap();
        // Leaving the match can respawn its ball; observe the live identity.
        let object = s.vehicle_infos()[0].id;
        let center = s.object_centre(ObjectRef::Vehicle(object)).unwrap();
        assert_eq!(s.game_of(p), None);
        assert!(s.game_of(q).is_some());
        s.credit(ObjectRef::Vehicle(object), q);
        s.simulation
            .mutate(ids[0], |b| {
                b.position = center.to_array();
                b.rule_region = Some([10.; 3]);
            })
            .unwrap();
        s.step_rule_observations().unwrap();
        assert!(s.events.rules.occupants.contains(&(ids[0], 1, object)));
        s.simulation.mutate(ids[2], |b| b.owner = q).unwrap();
        s.step_rule_observations().unwrap();
        assert!(
            !s.events.rules.occupants.contains(&(ids[0], 1, object)),
            "mover attribution never substitutes for the builder's owned spawner"
        );
    }

    #[test]
    fn deferred_game_fact_overflow_is_reported_and_diagnostics_remain_bounded() {
        let (mut s, p, _) = setup();
        s.create_rule_lab(p, "slayer").unwrap();
        let game = s.game_of(p).unwrap();
        s.events.advancing = true;
        for _ in 0..MAX_PENDING_FACTS {
            s.fire_rule_game_fact("onRuleScoreChanged", game, Some(p), None);
        }
        assert_eq!(s.events.rules.pending.len(), MAX_PENDING_FACTS);
        assert!(s.take_event_diagnostics().is_empty());
        for _ in 0..100 {
            s.fire_rule_game_fact("onRuleScoreChanged", game, Some(p), None);
        }
        assert_eq!(s.events.rules.pending.len(), MAX_PENDING_FACTS);
        let diagnostics = s.take_event_diagnostics();
        assert_eq!(diagnostics.len(), 64);
        assert!(diagnostics.iter().all(|d| d.contains("onRuleScoreChanged")
            && d.contains("256 facts")
            && d.contains("fact skipped")));
        s.events.advancing = false;
        s.step_rule_observations().unwrap();
        assert!(s.events.rules.pending.is_empty());
        s.events.advancing = true;
        s.fire_rule_game_fact("onRuleScoreChanged", game, Some(p), None);
        assert_eq!(s.events.rules.pending.len(), 1);
        assert!(s.take_event_diagnostics().is_empty());
    }

    #[test]
    fn shipped_addon_vocabulary_chains_into_guarded_core_rules() {
        let (mut s, p, _) = setup_packages(true);
        let ids = s.create_rule_lab(p, "addon").unwrap();
        assert_eq!(
            s.event_catalog()
                .unwrap()
                .output(Class::Brick, "cycleRoute")
                .unwrap()
                .package
                .as_deref(),
            Some("rule-workshop-toys")
        );
        for _ in 0..3 {
            run(&mut s, ids[0], "onActivate", p);
        }
        assert!(s.events.rules.variables.values().any(|v| *v == 1));
    }
    #[test]
    fn spawned_by_distinguishes_identical_balls_and_survives_respawn_without_cross_owner_matches() {
        let (mut s, p, q) = setup();
        let ids = s.create_rule_lab(p, "soccer").unwrap();
        assert_eq!(
            ids.len(),
            4,
            "match ball and practice ball have distinct named spawners"
        );
        for spawner in &ids[2..] {
            s.respawn_vehicle_brick(*spawner).unwrap();
        }
        let mut cx = Trigger::new(super::super::events::id(ids[1]), "onObjectEnter", 1);
        let target = Entity::brick(cx.source);
        let condition = Condition {
            subject: Subject::Object,
            property: Property::SpawnedBy,
            key: String::new(),
            compare: ev::rules::Compare::Equal,
            value: Datum::Text(
                s.simulation.state().bricks[&ids[2]]
                    .name
                    .clone()
                    .unwrap()
                    .to_uppercase(),
            ),
        };
        let ball = |s: &Session, brick| {
            s.vehicle_infos()
                .into_iter()
                .find(|v| s.vehicle_spawn_brick(VehicleId(v.id)) == Some(brick))
                .unwrap()
                .id
        };
        let original = ball(&s, ids[2]);
        cx.targets.insert(
            Slot::Object,
            super::super::events::entity(Class::Vehicle, original),
        );
        assert!(condition.matches(s.rule_query(&cx, target, &condition)));
        cx.targets.insert(
            Slot::Object,
            super::super::events::entity(Class::Vehicle, ball(&s, ids[3])),
        );
        assert!(
            !condition.matches(s.rule_query(&cx, target, &condition)),
            "same kind is not same spawner"
        );
        s.respawn_vehicle_brick(ids[2]).unwrap();
        cx.targets.insert(
            Slot::Object,
            super::super::events::entity(Class::Vehicle, original),
        );
        assert_eq!(
            s.rule_query(&cx, target, &condition),
            None,
            "delayed context never follows a replacement ball"
        );
        cx.targets.insert(
            Slot::Object,
            super::super::events::entity(Class::Vehicle, ball(&s, ids[2])),
        );
        assert!(
            condition.matches(s.rule_query(&cx, target, &condition)),
            "new ball keeps authored spawner identity"
        );
        s.simulation.mutate(ids[2], |b| b.owner = q).unwrap();
        assert_eq!(
            s.rule_query(&cx, target, &condition),
            None,
            "foreign names cannot match"
        );
        s.simulation
            .mutate(ids[2], |b| {
                b.owner = p;
                b.name = None;
            })
            .unwrap();
        assert_eq!(
            s.rule_query(&cx, target, &condition),
            None,
            "unnamed spawner has no named relationship"
        );
        let actor = s.peers[&p].actor.clone();
        s.simulation.remove(&actor, ids[2]).unwrap();
        assert_eq!(s.rule_query(&cx, target, &condition), None);
    }

    #[test]
    fn ball_goal_uses_real_scores_requires_credit_and_delayed_reset_replaces_object() {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "soccer").unwrap();
        s.respawn_vehicle_brick(ids[2]).unwrap();
        let object = s.vehicle_infos()[0].id;
        let center = s.object_centre(ObjectRef::Vehicle(object)).unwrap();
        s.simulation.mutate(ids[0], |b| b.events.clear()).unwrap();
        // A player's own points need a credited mover; the recipe's reset
        // follows.
        let reset = lab_programs("soccer")[1].1.last().unwrap().clone();
        assert_eq!(reset.output, "resetObject");
        s.simulation
            .mutate(ids[1], |b| {
                b.position = center.to_array();
                b.rule_region = Some([10.0; 3]);
                b.events = vec![
                    goal_row(
                        ev::Target::Slot(Slot::Instigator),
                        "addPlayerScore",
                        vec![ev::Value::Int(1)],
                    ),
                    reset,
                ];
            })
            .unwrap();
        s.dirty.extend([ids[1], ids[0]]);
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(score(&s, p), 0, "Natural motion has no scorer");
        s.simulation
            .mutate(ids[1], |b| b.position[0] += 50.0)
            .unwrap();
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        s.credit(ObjectRef::Vehicle(object), p);
        s.simulation
            .mutate(ids[1], |b| b.position = center.to_array())
            .unwrap();
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(score(&s, p), 1, "Awarded canonical MiniGame points");
        for _ in 0..360 {
            s.simulation.step().unwrap();
        }
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(!s.vehicle_infos().iter().any(|v| v.id == object));
        let mut cx = Trigger::new(super::super::events::id(ids[1]), "onObjectEnter", 1);
        cx.targets.insert(
            Slot::Object,
            super::super::events::entity(Class::Vehicle, object),
        );
        let condition = Condition {
            subject: Subject::Object,
            property: Property::Variable,
            key: "touches".into(),
            compare: ev::rules::Compare::Equal,
            value: Datum::Number(0),
        };
        assert_eq!(
            s.rule_query(&cx, Entity::brick(cx.source), &condition),
            None,
            "A reset object cannot supply even default-zero state"
        );
    }
    #[test]
    fn life_epoch_teleport_over_a_region_never_enters_it() {
        let (mut s, p, _q) = setup();
        let ids = s.create_rule_lab(p, "hill").unwrap();
        let mut enter = lab_programs("puzzle")[0].1[1].clone();
        enter.input = "onRegionEnter".into();
        enter.conditions.clear();
        s.simulation
            .mutate(ids[0], |b| {
                b.events = vec![enter];
                b.color = 0;
            })
            .unwrap();
        s.dirty.insert(ids[0]);
        s.step_events(&BTreeSet::new()).unwrap();
        let center = Vec3::from(s.simulation.state().bricks[&ids[0]].position);
        s.peers.get_mut(&p).unwrap().player.place(
            &mut s.simulation.physics,
            center - Vec3::Y + Vec3::X * 30.,
            0.,
            Vec3::ZERO,
        );
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        // A server teleport (instantRespawn, a reset, a teleport event) to the far side.
        s.peers
            .get_mut(&p)
            .unwrap()
            .player
            .teleport(
                &mut s.simulation.physics,
                center - Vec3::Y - Vec3::X * 30.,
                0.,
            )
            .unwrap();
        s.step_rule_observations().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert_eq!(
            s.simulation.state().bricks[&ids[0]].color,
            0,
            "a teleport that never entered the region fired onRegionEnter"
        );
    }
    #[test]
    fn life_epoch_rule_respawn_time_ends_with_its_minigame() {
        let (mut s, p, q) = setup();
        s.minigame_request(
            p,
            MiniGameRequest::Create {
                color: 0,
                settings: Default::default(),
            },
        )
        .unwrap();
        join_game(&mut s, p, q);
        // Slayer's team respawn time / teamkill penalty (`set_respawn_time`).
        s.peers.get_mut(&q).unwrap().respawn_ms = Some(60_000);
        s.minigame_request(q, MiniGameRequest::Leave).unwrap();
        assert_eq!(s.game_of(q), None);
        for _ in 0..400 {
            s.step().unwrap();
        }
        s.kill(q, None, super::super::combat::DamageKind::Suicide)
            .unwrap();
        let tick = s.simulation.state().tick;
        let wait = s.peers[&q].combat.respawn_tick - tick;
        assert!(wait <= 240, "free-build respawn waits {wait} ticks");
    }
    #[test]
    fn life_epoch_delayed_player_output_dies_with_its_life() {
        let (mut s, p, _q) = setup();
        let ids = s.create_rule_lab(p, "hill").unwrap();
        s.minigame_request(p, MiniGameRequest::Leave).unwrap();
        let row = ev::Row {
            conditions: vec![],
            preserved: None,
            enabled: true,
            input: "onActivate".into(),
            delay_ms: 5000,
            target: ev::Target::Slot(Slot::Player),
            output: "kill".into(),
            params: vec![],
        };
        s.simulation
            .mutate(ids[0], |b| b.events = vec![row])
            .unwrap();
        s.dirty.insert(ids[0]);
        for _ in 0..400 {
            s.step().unwrap();
        }
        s.start_event_tick(s.simulation.state().tick).unwrap();
        s.fire_input(ids[0], "onActivate", Some(p));
        s.step_events(&BTreeSet::new()).unwrap();
        // Dies some other way, then respawns.
        s.kill(p, None, super::super::combat::DamageKind::Suicide)
            .unwrap();
        for _ in 0..130 {
            s.step().unwrap();
        }
        s.request_respawn(p).unwrap();
        assert!(s.is_alive(p));
        for _ in 0..(6 * 120) {
            s.step().unwrap();
        }
        assert!(
            s.is_alive(p),
            "the old life's delayed kill killed the new body"
        );
    }
    #[test]
    fn swept_regions_detect_fast_passage_without_false_parallel_hit() {
        assert!(segment_box(
            Vec3::new(-5.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::splat(-1.0),
            Vec3::splat(1.0)
        ));
        assert!(!segment_box(
            Vec3::new(-5.0, 2.0, 0.0),
            Vec3::new(5.0, 2.0, 0.0),
            Vec3::splat(-1.0),
            Vec3::splat(1.0)
        ));
    }
    /// A goal on its own: the soccer lab's first goal brick, east of its
    /// ball spawn, with `rows` and a region that leaves the spawn outside.
    fn lone_goal(rows: Vec<ev::Row>) -> (Session, OwnerId, Vec<BrickId>, Vec3, Vec3) {
        let (mut s, p, ids, home, goal) = lone_goal_with(|events| *events = rows);
        s.simulation
            .mutate(ids[2], |b| b.name = Some("_ballspawn".into()))
            .unwrap();
        s.dirty.insert(ids[2]);
        s.step().unwrap();
        (s, p, ids, home, goal)
    }
    /// [`lone_goal`] keeping the lab's own goal rows, changed by `edit`.
    fn lone_goal_with(
        edit: impl FnOnce(&mut Vec<ev::Row>),
    ) -> (Session, OwnerId, Vec<BrickId>, Vec3, Vec3) {
        let (mut s, p, _) = setup();
        let ids = s.create_rule_lab(p, "soccer").unwrap();
        // The practice ball stays out of the way.
        s.simulation
            .mutate(ids[3], |b| b.position = [-30.0, 1.1, -30.0])
            .unwrap();
        s.respawn_vehicle_brick(ids[3]).unwrap();
        s.respawn_vehicle_brick(ids[2]).unwrap();
        let ball = ball_of(&s, ids[2]);
        let home = s.object_centre(ObjectRef::Vehicle(ball)).unwrap();
        let goal = home + Vec3::new(12.0, 0.0, 0.0);
        s.simulation
            .mutate(ids[0], |b| {
                b.position = goal.to_array();
                b.rule_region = Some([4.0, 12.0, 4.0]);
                b.colliding = false;
                edit(&mut b.events);
            })
            .unwrap();
        s.simulation.mutate(ids[1], |b| b.events.clear()).unwrap();
        s.dirty.extend(ids.iter().copied());
        for _ in 0..30 {
            s.step().unwrap();
        }
        let home = s
            .object_centre(ObjectRef::Vehicle(ball_of(&s, ids[2])))
            .unwrap();
        (s, p, ids, home, goal)
    }
    fn ball_of(s: &Session, spawner: BrickId) -> u64 {
        s.vehicle_infos()
            .into_iter()
            .find(|v| s.vehicle_spawn_brick(VehicleId(v.id)) == Some(spawner))
            .expect("the spawner's ball")
            .id
    }
    /// Moves `ball` to `at` as a credited kick would leave it, and runs a
    /// quarter second.
    fn place_ball(s: &mut Session, ball: u64, at: Vec3, by: OwnerId) {
        let transform = bri_vehicles::Transform {
            position: at.to_array(),
            rotation: [0.0, 0.0, 0.0, 1.0],
        };
        s.vehicles
            .world
            .as_mut()
            .unwrap()
            .set_transform(&mut s.simulation.physics, VehicleId(ball), &transform)
            .unwrap();
        s.credit(ObjectRef::Vehicle(ball), by);
        for _ in 0..30 {
            s.step().unwrap();
        }
    }
    /// Kicks `ball` into `at` from a few units west, the way a credited
    /// push does, and runs one second: it rolls in rather than jumping.
    fn kick_ball(s: &mut Session, ball: u64, at: Vec3, by: OwnerId) {
        let transform = bri_vehicles::Transform {
            position: (at - Vec3::X * 6.0).to_array(),
            rotation: [0.0, 0.0, 0.0, 1.0],
        };
        s.vehicles
            .world
            .as_mut()
            .unwrap()
            .set_transform(&mut s.simulation.physics, VehicleId(ball), &transform)
            .unwrap();
        s.step().unwrap();
        s.push_object(ObjectRef::Vehicle(ball), Vec3::X * 20.0)
            .unwrap();
        s.credit(ObjectRef::Vehicle(ball), by);
        for _ in 0..120 {
            s.step().unwrap();
        }
    }
    fn goal_row(target: ev::Target, output: &str, params: Vec<ev::Value>) -> ev::Row {
        ev::Row {
            conditions: vec![],
            preserved: None,
            enabled: true,
            input: "onObjectEnter".into(),
            delay_ms: 0,
            target,
            output: output.into(),
            params,
        }
    }
    fn score_row() -> ev::Row {
        goal_row(
            ev::Target::Slot(Slot::Instigator),
            "addPlayerScore",
            vec![ev::Value::Int(1)],
        )
    }
    /// Runs three goals with `reset` as the goal's reset row: each must
    /// score once, and bring a new ball back to its spawn brick at rest.
    fn assert_goals_rearm(reset: ev::Row) {
        let delay = reset.delay_ms;
        let (mut s, p, ids, home, goal) = lone_goal(vec![score_row(), reset]);
        for goals in 1..=3 {
            let ball = ball_of(&s, ids[2]);
            if goals == 2 {
                kick_ball(&mut s, ball, goal, p);
            } else {
                place_ball(&mut s, ball, goal, p);
            }
            assert_eq!(score(&s, p), goals, "entry {goals} with a {delay} ms reset");
            for _ in 0..(delay as usize * 120 / 1000 + 30) {
                s.step().unwrap();
            }
            let back = ball_of(&s, ids[2]);
            assert_ne!(back, ball, "a reset spawns a new ball");
            let at = s.object_centre(ObjectRef::Vehicle(back)).unwrap();
            assert!(
                (at - home).length() < 0.5,
                "the reset ball is at its spawn: {at:?} vs {home:?}"
            );
            let speed = s
                .object_velocity(ObjectRef::Vehicle(back))
                .unwrap()
                .length();
            assert!(speed < 0.5, "the reset ball is at rest: {speed}");
        }
    }
    /// Max (v0.2.3): after the ball went in, the goal never fired again.
    /// Each Object resetObject (now or after a delay) brings a new ball
    /// back to its spawn brick at rest, and each entry of it scores again.
    #[test]
    fn a_goal_fires_again_for_each_ball_its_reset_brings_back() {
        for delay in [0, 500] {
            let mut reset = goal_row(ev::Target::Slot(Slot::Object), "resetObject", vec![]);
            reset.delay_ms = delay;
            assert_goals_rearm(reset);
        }
    }
    /// The v20 way: the goal's row names the builder's ball spawn brick
    /// and respawns its vehicle.
    #[test]
    fn a_named_spawn_brick_respawn_vehicle_resets_the_ball_and_rearms_the_goal() {
        for delay in [0, 500] {
            let mut reset = goal_row(
                ev::Target::Named("_ballspawn".into()),
                "respawnVehicle",
                vec![],
            );
            reset.delay_ms = delay;
            assert_goals_rearm(reset);
        }
    }
    /// A ball that leaves the region and comes back enters it again.
    #[test]
    fn a_goal_fires_again_when_the_ball_leaves_and_comes_back() {
        let (mut s, p, ids, home, goal) = lone_goal(vec![score_row()]);
        let ball = ball_of(&s, ids[2]);
        place_ball(&mut s, ball, goal, p);
        assert_eq!(score(&s, p), 1);
        place_ball(&mut s, ball, goal, p);
        assert_eq!(score(&s, p), 1, "staying inside is not a new entry");
        place_ball(&mut s, ball, home, p);
        place_ball(&mut s, ball, goal, p);
        assert_eq!(score(&s, p), 2);
        place_ball(&mut s, ball, home, p);
        kick_ball(&mut s, ball, goal, p);
        assert_eq!(score(&s, p), 3, "a ball rolling back in enters again");
    }
    /// The goal's ball resets (a new ball) once it goes in: whether the
    /// goal's rows saw it.
    fn goal_reacts(s: &mut Session, ids: &[BrickId], goal: Vec3, by: OwnerId) -> bool {
        let ball = ball_of(s, ids[2]);
        place_ball(s, ball, goal, by);
        ball_of(s, ids[2]) != ball
    }
    fn reset_goal() -> Vec<ev::Row> {
        vec![goal_row(
            ev::Target::Slot(Slot::Object),
            "resetObject",
            vec![],
        )]
    }
    /// Max loaded someone else's build with its ownership kept: the goal
    /// is its builder's, the ball spawn the host's own. The host may edit
    /// the goal's events, so its rows see the host's ball.
    #[test]
    fn a_loaded_goal_sees_the_ball_of_an_administrator_who_may_edit_it() {
        let (mut s, p, ids, _, goal) = lone_goal(reset_goal());
        s.set_lan_host(false);
        let builder = 4242;
        s.simulation.mutate(ids[0], |b| b.owner = builder).unwrap();
        s.dirty.insert(ids[0]);
        s.step().unwrap();
        assert!(s.is_administrator(p));
        assert!(goal_reacts(&mut s, &ids, goal, p));
    }
    /// A stranger's ball leaves another builder's goal alone until the
    /// builder trusts them enough to edit its events.
    #[test]
    fn a_goal_sees_a_strangers_ball_only_once_they_may_edit_its_events() {
        let (mut s, _, ids, _, goal) = lone_goal(reset_goal());
        s.set_lan_host(false);
        let verified = |s: &mut Session, name: &str, key: u8| {
            s.join_verified(
                name.into(),
                Vec3::new(f32::from(key) * 3.0, 0.1, 8.0),
                false,
                Some(bri_admin::Principal([key; 32])),
            )
            .unwrap()
        };
        let ann = verified(&mut s, "Ann", 1);
        let bob = verified(&mut s, "Bob", 2);
        s.simulation.mutate(ids[0], |b| b.owner = ann).unwrap();
        s.simulation.mutate(ids[2], |b| b.owner = bob).unwrap();
        s.dirty.extend([ids[0], ids[2]]);
        s.respawn_vehicle_brick(ids[2]).unwrap();
        s.step().unwrap();
        assert!(!goal_reacts(&mut s, &ids, goal, bob), "Bob is a stranger");
        s.command(
            ann,
            1,
            Command::TrustInvite {
                target: bob,
                level: bri_world::authority::trust::BUILD,
            },
        )
        .unwrap();
        s.command(bob, 1, Command::AcceptTrust { from: ann })
            .unwrap();
        assert!(
            !goal_reacts(&mut s, &ids, goal, bob),
            "build trust is not enough"
        );
        s.command(
            ann,
            2,
            Command::TrustInvite {
                target: bob,
                level: bri_world::authority::trust::EVENTS,
            },
        )
        .unwrap();
        s.command(bob, 2, Command::AcceptTrust { from: ann })
            .unwrap();
        assert!(
            goal_reacts(&mut s, &ids, goal, bob),
            "Bob may edit Ann's events"
        );
    }
    /// Named targets stay per builder: another builder's spawn brick of
    /// the same name is not the goal's.
    #[test]
    fn a_goal_never_respawns_another_builders_named_spawn() {
        let reset = goal_row(
            ev::Target::Named("_ballspawn".into()),
            "respawnVehicle",
            vec![],
        );
        let (mut s, p, ids, _, goal) = lone_goal(vec![reset]);
        let guest = s.peers.keys().copied().find(|o| *o != p).unwrap();
        s.simulation.mutate(ids[2], |b| b.owner = guest).unwrap();
        s.dirty.insert(ids[2]);
        s.respawn_vehicle_brick(ids[2]).unwrap();
        s.step().unwrap();
        // The LAN host trusts everyone, so the goal sees the guest's ball;
        // the name still resolves only among the goal builder's bricks.
        assert!(!goal_reacts(&mut s, &ids, goal, p));
    }
    /// The shipped soccer goal credits the team attacking it by slot,
    /// whoever knocked the ball in: a defender's own goal counts for the
    /// attackers, and their own score is untouched. A ball scores once
    /// however it bounces before its reset; the next ball scores again. The
    /// win row, after the score row, sees the new score in the same firing.
    #[test]
    fn the_soccer_goal_credits_its_attackers_once_per_ball_even_for_an_own_goal() {
        let (mut s, p, ids, home, goal) = lone_goal_with(|rows| {
            for row in rows {
                for c in &mut row.conditions {
                    if c.property == Property::Score {
                        c.value = Datum::Number(2);
                    }
                }
            }
        });
        let game = s.game_of(p).unwrap();
        let teams: Vec<_> = s
            .minigames
            .game(game)
            .unwrap()
            .teams
            .list
            .iter()
            .map(|t| t.id)
            .collect();
        let defender = s.minigames.player(s.peers[&p].combat.player).unwrap().team;
        assert_eq!(
            defender,
            Some(teams[0]),
            "the lab puts its builder on the first team"
        );
        let attackers = |s: &Session| s.minigames.team_score(game, teams[1]).unwrap();
        let ball = ball_of(&s, ids[2]);
        place_ball(&mut s, ball, goal, p);
        assert_eq!(attackers(&s), 1, "an own goal counts for the attackers");
        assert_eq!(score(&s, p), 0, "and not for whoever knocked it in");
        assert_eq!(s.minigames.team_score(game, teams[0]).unwrap(), 0);
        // It bounces out and back in before its reset: no second point.
        place_ball(&mut s, ball, home, p);
        place_ball(&mut s, ball, goal, p);
        assert_eq!(attackers(&s), 1, "one ball, one goal");
        assert_eq!(s.round_results().count(), 0);
        for _ in 0..400 {
            s.step().unwrap();
        }
        let next = ball_of(&s, ids[2]);
        assert_ne!(next, ball);
        place_ball(&mut s, next, goal, p);
        assert_eq!(attackers(&s), 2);
        let result = s.round_results().last().expect("the second goal wins");
        assert_eq!(result.teams, vec![teams[1]]);
    }
}
