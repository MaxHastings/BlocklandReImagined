//! A bot's objective each tick: plan when it is time, keep or drop the
//! step it is on, and act on it.
use super::*;

impl Session {
    pub(in crate::session::bots) fn bot_objective(&mut self, bot: OwnerId, tick: u64) -> Option<View> {
        if self.bots.brains[&bot]
            .kind
            .weight("objective")
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
            // No plan among a model cut down to its bounds: say so, rather
            // than that there is none.
            let result = match result {
                Err(planning::Failure::NoPlan) if budget.truncated => {
                    Err(planning::Failure::ActionBudgetExceeded)
                }
                result => result,
            };
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
    pub(in crate::session::bots) fn bot_objective_act(&mut self, bot: OwnerId, tick: u64) -> Result<()> {
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
