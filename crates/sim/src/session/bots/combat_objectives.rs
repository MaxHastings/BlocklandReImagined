//! Elimination is a causal action only when canonical native death and the full
//! authored owner inputs can project the actual desired round state. No hidden
//! enemy transforms or planner-written damage, scores or dispatches occur here.
use super::objectives::{self, Cause, GroundedAction, GroundingBudget, Transition};
use super::planning::{self, FactValue, Facts};
use super::*;
use bri_events::rules::{Condition, Property, Subject};
use bri_events::{Class, Entity};

#[derive(Clone, Debug)]
pub(super) struct Action {
    pub victim: OwnerId,
    pub life: bri_minigames::LifeId,
    pub evidence: Knowledge,
    pub actor_spawn: u64,
    pub game: bri_minigames::GameId,
    pub round: u64,
    pub team: Option<bri_minigames::TeamId>,
    pub planned: u64,
}

impl Action {
    pub(super) fn validate(&self, session: &Session, bot: OwnerId) -> bool {
        let Some(brain) = session.bots.brains.get(&bot) else {
            return false;
        };
        session.game_of(bot) == Some(self.game)
            && session.game_of(self.victim) == Some(self.game)
            && session
                .minigames
                .game(self.game)
                .is_ok_and(|g| g.round == self.round)
            && session.peers.get(&bot).is_some_and(|p| {
                p.combat.alive
                    && p.combat.spawn_tick == self.actor_spawn
                    && session
                        .minigames
                        .player(p.combat.player)
                        .is_ok_and(|p| p.team == self.team)
            })
            && session.peers.get(&self.victim).is_some_and(|p| {
                session.minigames.player(p.combat.player).is_ok_and(
                |p| matches!(p.life, bri_minigames::LifeState::Alive {life} if life == self.life),
            )
            })
            && session.bot_enemy(bot, &brain.kind, self.victim)
            && (brain.memory.is_some_and(|k| {
                k.subject == self.victim && session.simulation.state().tick < k.expires
            }) || session.simulation.state().tick < self.evidence.expires)
    }
    pub(super) fn view(&self, session: &Session, bot: OwnerId) -> Option<objectives::View> {
        if !self.validate(session, bot) {
            return None;
        }
        let evidence = session
            .bots
            .brains
            .get(&bot)?
            .memory
            .filter(|k| k.subject == self.victim)
            .unwrap_or(self.evidence);
        let mut view = objectives::View::locomotion(evidence.at, evidence.at + Vec3::Y);
        view.enemy = Some(self.victim);
        view.enemy_evidence = Some(evidence);
        Some(view)
    }
    /// A bounded observer records accepted canonical death with credited
    /// killer and exact life. Mere damage/absence/memory expiry is not admission.
    pub(super) fn admitted(&self, session: &Session, bot: OwnerId) -> Option<(u64, u64)> {
        session
            .death_results()
            .rev()
            .find(|r| {
                r.victim == self.victim
                    && r.life == self.life
                    && r.killer == Some(bot)
                    && r.game == Some(self.game.0)
                    && r.round == Some(self.round)
                    && r.tick >= self.planned
            })
            .map(|r| (r.tick, r.tick))
    }
}
impl Session {
    pub(super) fn append_enemy_actions(
        &self,
        bot: OwnerId,
        facts: &mut Facts,
        budget: &mut GroundingBudget,
    ) -> Result<Vec<GroundedAction>, planning::Failure> {
        use planning::Failure as F;
        let Some(game) = self.game_of(bot) else {
            return Ok(vec![]);
        };
        let g = self.minigames.game(game).map_err(|_| F::NoPlan)?;
        if g.round_over {
            return Ok(vec![]);
        }
        let Some(brain) = self.bots.brains.get(&bot) else {
            return Ok(vec![]);
        };
        if !hand_combat::has_possible_attack(self, bot)
            && brain.kind.melee.is_none()
            && self.bot_vehicle_weapon(bot).is_none()
        {
            return Ok(vec![]);
        }
        let tick = self.simulation.state().tick;
        let Some(evidence) = brain
            .memory
            .filter(|k| tick < k.expires && self.bot_enemy(bot, &brain.kind, k.subject))
        else {
            return Ok(vec![]);
        };
        let victim = evidence.subject;
        if self.game_of(victim) != Some(game) {
            return Ok(vec![]);
        }
        // Opaque death hooks may mutate scores/rules before scheduled rows run.
        // Reject this causal prediction; normal native combat remains available.
        if self
            .packages
            .as_ref()
            .is_some_and(|h| h.catalog.behaviours().any(|(_, b)| b.on_death))
        {
            return Err(F::Unsupported);
        }
        let peer = &self.peers[&bot];
        let player = self
            .minigames
            .player(peer.combat.player)
            .map_err(|_| F::NoPlan)?;
        let victim_peer = &self.peers[&victim];
        let victim_player = self
            .minigames
            .player(victim_peer.combat.player)
            .map_err(|_| F::NoPlan)?;
        let bri_minigames::LifeState::Alive { life } = victim_player.life else {
            return Ok(vec![]);
        };
        let Some(sources) = self.events.objective_deaths.get(&g.owner.account.0) else {
            return Ok(vec![]);
        };
        if sources.len() > 64 {
            return Err(F::ModelBudgetExceeded);
        }
        let world = self.events.world.as_ref().ok_or(F::Unsupported)?;
        let mut causes = Vec::new();
        let entity = |owner| Entity {
            class: Class::Player,
            id: super::super::events::id(owner),
        };
        let transitions = [
            Transition {
                target: entity(victim),
                property: Property::Alive,
                key: String::new(),
                after: FactValue::Bool(false),
            },
            Transition {
                target: entity(bot),
                property: Property::Score,
                key: String::new(),
                after: FactValue::Number(
                    player
                        .score
                        .saturating_add(i64::from(g.settings.points_kill_player)),
                ),
            },
            Transition {
                target: entity(victim),
                property: Property::Score,
                key: String::new(),
                after: FactValue::Number(
                    victim_player
                        .score
                        .saturating_add(i64::from(g.settings.points_die)),
                ),
            },
        ];
        for source in sources {
            let program = world
                .program(super::super::events::id(*source))
                .ok_or(F::Unsupported)?;
            let context = self
                .input_context(
                    *source,
                    "onRulePlayerDied",
                    Some(victim),
                    super::super::events::InputExtra {
                        game: Some(game),
                        killer: Some(bot),
                        ..Default::default()
                    },
                    1,
                )
                .ok_or(F::Unsupported)?;
            let cause = self.ground_causal_input(
                bot,
                program,
                context,
                if causes.is_empty() { &transitions } else { &[] },
                facts,
                budget,
            )?;
            causes.push(cause);
        }
        if causes.is_empty() {
            return Ok(vec![]);
        }
        let groups = objectives::project_rule_causes(&causes, budget)?;
        // A native death can be admitted only once for this exact living life.
        // The existing guaranteed transition sets the same canonical Alive key
        // false, so repeated projected death inputs cannot satisfy a counter.
        let alive_key = self
            .objective_fact_key(
                &causes[0].context,
                entity(victim),
                &Condition {
                    subject: Subject::Target,
                    property: Property::Alive,
                    key: String::new(),
                    compare: bri_events::rules::Compare::Equal,
                    value: bri_events::rules::Datum::Bool(true),
                },
            )
            .ok_or(F::Unsupported)?;
        budget.reserve(0, 1, alive_key.len())?;
        let actor_spawn = peer.combat.spawn_tick;
        let action = Action {
            victim,
            life,
            evidence,
            actor_spawn,
            game,
            round: g.round,
            team: player.team,
            planned: tick,
        };
        let distance = Vec3::from(peer.player.state().feet).distance(evidence.at);
        Ok(vec![GroundedAction {
            model: planning::Action {
                id: format!("enemy:{victim}:{}", life.0),
                cost: 1 + (distance * 10.0).min(100000.0) as u32,
                preconditions: vec![planning::Predicate {
                    key: alive_key,
                    compare: planning::Compare::Equal,
                    value: FactValue::Bool(true),
                }],
                effect_groups: groups,
            },
            cause: Cause::Rules(causes),
            executor: objectives::Executor::Enemy(action),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_events::{Row, Slot, Target, Value};

    #[test]
    fn grounded_death_cannot_count_the_same_living_victim_twice() {
        let world = bri_world::World::new("Death projection".into(), "test".into(), vec![[1.0; 4]]);
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
        s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())
            .unwrap();
        let victim = s
            .join("Author".into(), Vec3::new(0.0, 0.05, 0.0), true)
            .unwrap();
        let bot = s
            .join("Model probe".into(), Vec3::new(10.0, 0.05, 0.0), false)
            .unwrap();
        s.command(
            victim,
            1,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: Default::default(),
            }),
        )
        .unwrap();
        let game = s.game_of(victim).unwrap();
        s.command(
            bot,
            1,
            Command::MiniGame(MiniGameRequest::Join { game: game.0 }),
        )
        .unwrap();
        // Model-level evidence, not a gameplay success injection. Real sight,
        // search, firing and credited deaths have separate control fixtures.
        let mut brain = Brain::new(
            None,
            BotKind {
                id: "projection-probe".into(),
                name: "Projection probe".into(),
                melee: Some(crate::bot_kind::BotMelee {
                    damage: 1.0,
                    reach: 2.5,
                    seconds: 1.0,
                    action: None,
                    name: "Probe".into(),
                    converts_below: None,
                }),
                ..Default::default()
            },
            Vec3::ZERO,
            bot,
            0,
        );
        brain.memory = Some(Knowledge {
            subject: victim,
            at: Vec3::ZERO,
            observed: 0,
            expires: 1200,
        });
        s.bots.brains.insert(bot, brain);
        let mut brick = bri_world::Brick::new(
            bri_world::ContentRef::Resolved(crate::testing::PLATE.into()),
            [20.25, 0.1, 20.25],
            victim,
        );
        brick.events.push(Row {
            enabled: true,
            input: "onRulePlayerDied".into(),
            output: "addVariable".into(),
            target: Target::Slot(Slot::SelfBrick),
            params: vec![Value::Int(0), Value::Text("deaths".into()), Value::Int(1)],
            conditions: vec![],
            delay_ms: 0,
            preserved: None,
        });
        let actor = s.peers[&victim].actor.clone();
        let source = s
            .simulation
            .plant_group_floating(&actor, vec![brick])
            .unwrap()[0];
        s.sync_event_programs(&std::collections::BTreeSet::from([source]));
        let mut facts = Facts::new();
        let mut actions = s
            .append_enemy_actions(bot, &mut facts, &mut GroundingBudget::default())
            .unwrap();
        assert_eq!(actions.len(), 1);
        let action = actions.remove(0).model;
        let alive = &action.preconditions[0];
        assert_eq!(facts[&alive.key], FactValue::Bool(true));
        assert!(action.effect_groups.iter().flat_map(|g| &g.effects).any(|e|
            matches!(e, planning::Effect::Set {key, value: FactValue::Bool(false)} if key == &alive.key)
        ), "native death supplies Alive=false without an authored IF Alive");
        let counter = action
            .effect_groups
            .iter()
            .flat_map(|g| &g.effects)
            .find_map(|e| match e {
                planning::Effect::Add { key, amount: 1 } => Some(key.clone()),
                _ => None,
            })
            .unwrap();
        let goal = |count| {
            planning::Goal(vec![planning::Predicate {
                key: counter.clone(),
                compare: planning::Compare::AtLeast,
                value: FactValue::Number(count),
            }])
        };
        assert_eq!(
            planning::plan(
                &facts,
                std::slice::from_ref(&action),
                &goal(1),
                objectives::limits()
            )
            .unwrap(),
            vec![action.id.clone()]
        );
        assert_eq!(
            planning::plan(&facts, &[action], &goal(2), objectives::limits()),
            Err(planning::Failure::NoPlan)
        );
        assert!(s.is_alive(victim));
        assert_eq!(s.death_results().count(), 0);
        assert_eq!(s.round_results().count(), 0);
    }
}
