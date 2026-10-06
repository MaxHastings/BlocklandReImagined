//! What a brick's rows would do, as a plan reads it: the facts its
//! conditions read, the effects its outputs have, an Add-On output's
//! included, and the rows that react to a score or a round.
use super::*;

impl Session {
    /// The game and team whose canonical score a Team subject reads: the
    /// rule actor's, exactly as `rule_query` resolves it.
    pub(super) fn objective_team(
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
    pub(super) fn team_score(&self, game: bri_minigames::GameId, team: bri_minigames::TeamId) -> Option<i64> {
        self.minigames.team_score(game, team).ok()
    }
    pub(in crate::session::bots) fn objective_fact_key(
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
    pub(super) fn objective_predicate(
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
    pub(super) fn objective_effect(
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
    pub(super) fn package_effects(
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
    pub(in crate::session::bots) fn project_objective_input(
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
            // A print count is a display, like a sound: nothing a plan
            // reads, unless a step can wrap into a target's own overflow
            // rows, which a plan would then have to follow.
            if let Some(may_wrap) = world.row_print_may_wrap(cx.source, index as u16) {
                let wraps_into_rows = may_wrap
                    && targets.iter().any(|t| {
                        world.program(t.id).is_some_and(|p| {
                            p.rows.iter().any(|r| {
                                r.enabled
                                    && matches!(
                                        r.input.as_str(),
                                        "onPrintCountOverFlow" | "onPrintCountUnderFlow"
                                    )
                            })
                        })
                    });
                if wraps_into_rows {
                    return Err(F::Unsupported);
                }
                continue;
            }
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
                    intent if changes_nothing_planned(intent) => continue,
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
    pub(super) fn objective_reset(
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
    pub(super) fn objective_reactions_clear(
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
                        Some(intent) if changes_nothing_planned(intent) => {}
                        Some(Intent::Brick(ev::BrickOp::Color(_))) => {
                            if facts.contains_key(&format!("brick/color/{}", target.id.index)) {
                                return Err(F::Unsupported);
                            }
                        }
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
    pub(in crate::session::bots) fn ground_causal_input(
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
    pub(super) fn discover_rule_causes(
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
                    budget.truncated = true;
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
}
