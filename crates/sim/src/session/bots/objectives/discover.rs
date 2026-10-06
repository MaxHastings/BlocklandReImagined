//! What a bot's game asks of it and how it could get there: the goals
//! its rules offer (the round's win, a score rising), the bricks whose
//! rows serve them, and the actions it may take, bounded by a budget.
use super::*;

impl Session {
    pub(super) fn rule_desired_state(&self, bot: OwnerId) -> Result<DesiredState, planning::Failure> {
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
    pub(super) fn score_desired_state(&self, bot: OwnerId) -> Option<DesiredState> {
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
    pub(super) fn score_of(
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
        let (sources, _) = self.objective_candidates(bot, owner);
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
        match self.objective_snapshot_with_budget(
            bot,
            tick,
            &Failed::default(),
            desired,
            &discovery,
            &mut budget,
        ) {
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
    pub(super) fn discover_desired_states(
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
    pub(super) fn objective_snapshot(
        &self,
        bot: OwnerId,
        tick: u64,
        failed: &Failed,
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
    pub(super) fn append_physical_actions(
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
    /// The sources an objective plan considers, scoring rows first and
    /// then the nearest, within the model's source and row bounds; and
    /// whether any were left out.
    pub(super) fn objective_candidates(
        &self,
        bot: OwnerId,
        owner: OwnerId,
    ) -> (Vec<BrickId>, bool) {
        let Some(indexed) = self.events.objective_sources.get(&owner) else {
            return (Vec::new(), false);
        };
        let Some(world) = self.events.world.as_ref() else {
            return (Vec::new(), false);
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
        let total = ranked.len();
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
        let truncated = out.len() < total;
        (out, truncated)
    }
    pub(super) fn objective_snapshot_with_budget(
        &self,
        bot: OwnerId,
        tick: u64,
        failed: &Failed,
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
        let (sources, truncated) = self.objective_candidates(bot, g.owner.account.0);
        budget.truncated |= truncated;
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
                budget.truncated = true;
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
                    budget.truncated = true;
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
                        budget.truncated = true;
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
                    budget.truncated = true;
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
}
