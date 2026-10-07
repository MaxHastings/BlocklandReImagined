//! A few lines saying why a bot does what it does, from its readout
//! (`BotThought`): its choice, the best candidates, the biggest terms on
//! them and what it last noticed. The host's performance overlay (F3)
//! shows them for the bot the player looks at; they never go on the wire.
use super::*;

/// Lines in a readout.
pub const WHY_LINES: usize = 5;

impl BotThought {
    /// The readout at `tick`, at most [`WHY_LINES`] lines.
    pub fn why(&self, tick: u64) -> Vec<String> {
        let decision = self
            .surprise
            .decisions
            .iter()
            .find(|d| d.domain == "behaviour");
        let mut choice = format!("Doing {}", self.behaviour);
        if let Some(flavour) = self.surprise.interrupt {
            choice.push_str(&format!(", goofing ({flavour})"));
        }
        if let Some(d) = decision {
            if d.chosen != d.plain {
                choice.push_str(&format!(" over plain {}", d.plain));
            }
            choice.push_str(&format!(" [{}]", d.reason));
        }
        let mut out = vec![choice];
        let mut candidates: Vec<&BotCandidate> =
            decision.map_or(Vec::new(), |d| d.candidates.iter().collect());
        candidates.sort_by(|a, b| b.adjusted.total_cmp(&a.adjusted));
        if !candidates.is_empty() {
            let top: Vec<String> = candidates
                .iter()
                .take(3)
                .map(|c| format!("{} {:.2}", c.option, c.adjusted))
                .collect();
            out.push(format!("Top: {}", top.join(", ")));
        }
        // The biggest terms on the candidates: drift, boredom, lost
        // effectiveness, then the hold in force.
        let mut terms: Vec<(f32, String)> = Vec::new();
        for c in &candidates {
            terms.push((c.drift.abs(), format!("drift {:+.2} {}", c.drift, c.option)));
            terms.push((c.boredom, format!("boredom {:.2} {}", c.boredom, c.option)));
            terms.push((
                1.0 - c.effectiveness,
                format!("effect {:.2} {}", c.effectiveness, c.option),
            ));
        }
        terms.retain(|(size, _)| *size > 0.005);
        terms.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut parts: Vec<String> = terms.into_iter().take(3).map(|(_, t)| t).collect();
        if let Some(gate) = self.surprise.gate {
            parts.push(format!("hold: {gate}"));
        } else if decision.is_some_and(|d| d.reason == "committed") {
            parts.push("hold: committed".into());
        }
        if self.surprise.strength <= 0.0 {
            parts.push("surprise off".into());
        }
        if !parts.is_empty() {
            out.push(format!("Terms: {}", parts.join(", ")));
        }
        let mut noticed = match (self.visible, &self.remembered) {
            (Some(seen), _) => format!("Sees player {seen}"),
            (None, Some(e)) => format!(
                "Remembers player {} {:.1} s ago at ({:.0}, {:.0}, {:.0})",
                e.subject,
                tick.saturating_sub(e.observed) as f32 / 120.0,
                e.position[0],
                e.position[1],
                e.position[2]
            ),
            (None, None) => "Noticed nothing".to_string(),
        };
        // A glance or reaction still under way (`perception`).
        if let Some(n) = self.noticed.filter(|n| tick < n.until) {
            noticed.push_str(&format!("; {}", n.why));
        }
        out.push(noticed);
        out.push(self.acted.clone());
        out.truncate(WHY_LINES);
        out
    }
}

impl Session {
    pub fn bot_thoughts(&self) -> Vec<BotThought> {
        let tick = self.simulation.state().tick;
        self.bots
            .brains
            .iter()
            .map(|(bot, b)| BotThought {
                bot: *bot,
                behaviour: b.behaviour.name(),
                leg: if self.bot_vehicle_body(*bot).is_some() && !b.plan.is_empty() {
                    "drive"
                } else {
                    b.plan
                        .first()
                        .map_or("none", |w| crate::route::leg_name(w.mode))
                },
                visible: b.target,
                remembered: b.memory.map(|k| BotEvidence {
                    subject: k.subject,
                    position: k.at.to_array(),
                    observed: k.observed,
                    expires: k.expires,
                }),
                task: self
                    .bots
                    .claims
                    .owner_claim(*bot, tick)
                    .map(|c| match c.resource {
                        claims::Resource::Seat { vehicle, seat } => BotTask::Seat {
                            vehicle,
                            seat,
                            subject: c.subject,
                            deadline: c.deadline,
                        },
                        claims::Resource::Body { vehicle } => BotTask::Push {
                            vehicle,
                            subject: c.subject,
                            deadline: c.deadline,
                        },
                    }),
                // Standing still for a goof it wants to go nowhere: its
                // route waits for it.
                goal: (!b.surprise.standing())
                    .then(|| b.goal.map(|g| g.point(b.leash).to_array()))
                    .flatten(),
                next: (!b.surprise.standing())
                    .then(|| b.plan.first().map(|p| p.feet.to_array()))
                    .flatten(),
                path_steps: b.plan.len(),
                searching: b.search.is_some(),
                search_phase: b.evidence_search.phase(),
                objective: b.objective.source(),
                objective_diagnostic: b.objective.diagnostic,
                objective_detail: b.objective.detail().map(|detail| BotObjectiveDetail {
                    desired: detail.desired.to_owned(),
                    action: detail.action.to_owned(),
                    provider: detail.provider,
                    phase: if detail.phase == "waiting" {
                        "waiting"
                    } else if b.resting || b.behaviour != Behaviour::Objective {
                        "paused"
                    } else {
                        detail.phase
                    },
                    route: detail.route.to_vec(),
                }),
                objective_searches: b.objective.searches,
                objective_reused: b.objective.reused,
                noticed: b.perception.why,
                surprise: b.surprise.view(&b.kind.surprise),
                team: b.team.view(),
                acted: b.acted.line(),
                planned: b.combat.intent(tick).map(|i| BotPlannedHarm {
                    enemy: i.choice.harm.enemy,
                    ally: i.choice.harm.ally,
                    own: i.choice.harm.own,
                    push: i.choice.harm.push,
                    kills_ally: i.choice.harm.kills_ally,
                }),
            })
            .collect()
    }
    /// CPU time this session's ticking thread has spent thinking for its
    /// bots (`step_bots`), in nanoseconds ([`crate::cpu_time`]): a
    /// diagnostic for the perf bar, never game state.
    pub fn bot_think_nanos(&self) -> u64 {
        self.bots.think_nanos
    }
    /// Every bot's readout ([`BotThought::why`]), for the host's overlay.
    pub fn bot_why(&self) -> Vec<(OwnerId, Vec<String>)> {
        let tick = self.simulation.state().tick;
        self.bot_thoughts()
            .into_iter()
            .map(|t| (t.bot, t.why(tick)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(option: &str, adjusted: f32, drift: f32, boredom: f32) -> BotCandidate {
        BotCandidate {
            option: option.into(),
            score: adjusted,
            adjusted,
            drift,
            boredom,
            effectiveness: 1.0,
        }
    }

    #[test]
    fn the_readout_names_the_choice_its_rivals_terms_and_what_it_noticed() {
        let mut t = BotThought {
            bot: 7,
            behaviour: "chase",
            leg: "walk",
            visible: None,
            remembered: Some(BotEvidence {
                subject: 3,
                position: [10.0, 0.0, -4.0],
                observed: 1000,
                expires: 2000,
            }),
            task: None,
            goal: None,
            next: None,
            path_steps: 0,
            searching: false,
            search_phase: "",
            objective: None,
            objective_diagnostic: None,
            objective_detail: None,
            objective_searches: 0,
            objective_reused: 0,
            noticed: None,
            team: Default::default(),
            acted: "Acts: walk route, look route, trigger none".into(),
            planned: None,
            surprise: BotSurpriseView {
                strength: 0.6,
                gate: None,
                interrupt: None,
                drives: Vec::new(),
                decisions: vec![BotDecision {
                    domain: "behaviour",
                    tick: 1200,
                    plain: "fight".into(),
                    chosen: "chase".into(),
                    varied: true,
                    reason: "committed",
                    candidates: vec![
                        candidate("wander", 0.1, 0.0, 0.0),
                        candidate("fight", 0.8, -0.3, 0.05),
                        candidate("search", 0.2, 0.0, 0.0),
                        candidate("chase", 0.75, 0.1, 0.4),
                    ],
                }],
            },
        };
        assert_eq!(
            t.why(1240),
            [
                "Doing chase over plain fight [committed]",
                "Top: fight 0.80, chase 0.75, search 0.20",
                "Terms: boredom 0.40 chase, drift -0.30 fight, drift +0.10 chase, hold: committed",
                "Remembers player 3 2.0 s ago at (10, 0, -4)",
                "Acts: walk route, look route, trigger none",
            ]
        );
        t.visible = Some(3);
        t.noticed = Some(super::super::BotNotice {
            why: "reacting: relaxed",
            since: 1200,
            until: 1300,
        });
        t.surprise.strength = 0.0;
        t.surprise.interrupt = Some("hop");
        t.surprise.decisions.clear();
        assert_eq!(
            t.why(1240),
            [
                "Doing chase, goofing (hop)",
                "Terms: surprise off",
                "Sees player 3; reacting: relaxed",
                "Acts: walk route, look route, trigger none",
            ]
        );
        // Once over, it is not shown.
        assert_eq!(t.why(1300)[2], "Sees player 3");
    }
}
