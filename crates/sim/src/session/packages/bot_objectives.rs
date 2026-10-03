//! Read-only, bounded package policy descriptions. This path never commits a
//! script outcome or performs its operations; ordinary pickup/zone hooks write.
use super::*;
use bri_package_runtime::bot_objectives::{
    self as description, CarryReturn, CounterBinding, CounterScope,
};

#[derive(Clone, Debug)]
pub(in crate::session) struct Offer {
    pub package: String,
    pub goal: CarryReturn,
    pub epoch: i64,
    pub baseline: i64,
}

#[derive(Default)]
pub(in crate::session) struct Discovery {
    pub offers: Vec<Offer>,
    pub unsupported: bool,
    pub budget_exhausted: bool,
}

impl Session {
    /// Called only on the common fair model-building turn. Even a rejected
    /// query consumes its package's existing server script-work allowance.
    pub(in crate::session) fn package_objective_offers(&mut self, bot: OwnerId) -> Discovery {
        let mut found = Discovery::default();
        if !self.package_participant(bot)
            || self.game_of(bot).is_none()
            || !self.peers.get(&bot).is_some_and(|p| p.combat.alive)
        {
            return found;
        }
        let providers: Vec<String> = self
            .packages
            .as_ref()
            .map(|h| {
                h.catalog
                    .behaviours()
                    .filter(|(_, b)| b.bot_objectives)
                    .take(description::MAX_OBJECTIVES + 1)
                    .map(|(p, _)| p.clone())
                    .collect()
            })
            .unwrap_or_default();
        if providers.len() > description::MAX_OBJECTIVES {
            found.budget_exhausted = true;
            return found;
        }
        if providers.is_empty() {
            return found;
        }
        let snapshot = Arc::new(self.package_snapshot());
        let tick = self.simulation.state().tick;
        let mut description_bytes = 0usize;
        for package in providers {
            let Some(host) = self.packages.as_mut() else {
                break;
            };
            if host.shares.work.available(&package, tick) <= 0 {
                found.budget_exhausted = true;
                continue;
            }
            let state = host.store.namespace(&package).cloned().unwrap_or_default();
            let before = state.clone();
            let vars = Arc::new(self.package_vars(&package));
            let world = super::super::script_world::ScriptWorld::new(self, &package);
            let call = Call {
                function: "bot_objectives",
                args: vec![Dynamic::from_int(bot as i64)],
                budget: Budget::Objective,
                snapshot: snapshot.clone(),
                caller: Some(bot),
                aim: None,
                entity: None,
                state,
                entity_vars: vars,
                world: Some(&world),
            };
            let start = std::time::Instant::now();
            let answer = self
                .packages
                .as_ref()
                .unwrap()
                .runtime
                .query(&package, call);
            let duration = start.elapsed();
            drop(world);
            let host = self.packages.as_mut().unwrap();
            *host.script_time.entry(package.clone()).or_default() += duration;
            self.charge_work(&package);
            let descriptors = answer.map_err(|d| d.message.clone()).and_then(|outcome| {
                description::read_only(&before, &outcome)
                    .and_then(|()| description::decode(&outcome.returned))
            });
            let descriptors = match descriptors {
                Ok(descriptors) => descriptors,
                Err(reason) => {
                    self.package_objective_note(&package, &reason);
                    found.unsupported = true;
                    continue;
                }
            };
            if found.offers.len() + descriptors.len() > description::MAX_OBJECTIVES {
                found.budget_exhausted = true;
                continue;
            }
            for described in descriptors {
                description_bytes =
                    description_bytes.saturating_add(described.validate().unwrap_or(usize::MAX));
                if description_bytes > 8192 {
                    found.budget_exhausted = true;
                    break;
                }
                let description::DesiredState::CarryReturn(goal) = described;
                let admitted = self.package_objective_binding_declared(&package, &goal.epoch)
                    && self.package_objective_binding_declared(&package, &goal.completion)
                    && self.packages.as_ref().is_some_and(|h| {
                        h.catalog
                            .behaviours()
                            .find(|(p, _)| **p == package)
                            .is_some_and(|(_, b)| {
                                b.on_pickup
                                    && b.state.player.contains_key(&goal.carriage.key)
                                    && item_hooks::owns(&h.catalog, &package, &goal.item)
                                    && goal.carriage.worn.as_ref().is_none_or(|w| {
                                        item_hooks::owns(&h.catalog, &package, &w.image)
                                    })
                            })
                    });
                let epoch = self.package_objective_counter(bot, &package, &goal.epoch, true);
                let baseline =
                    self.package_objective_counter(bot, &package, &goal.completion, false);
                if admitted
                    && let (Some(epoch), Some(baseline)) = (epoch, baseline)
                    && goal
                        .destinations
                        .iter()
                        .all(|id| self.package_objective_zone(bot, &package, *id).is_some())
                {
                    found.offers.push(Offer {
                        package: package.clone(),
                        goal,
                        epoch,
                        baseline,
                    });
                } else {
                    self.package_objective_note(&package, &format!("{} admission: declarations={}, initialized epoch={:?}, completion={:?}, zone identities={}",
                        goal.id, admitted, epoch, baseline, goal.destinations.iter().all(|id| self.package_objective_zone(bot, &package, *id).is_some())));
                    found.unsupported = true;
                }
            }
        }
        found
    }

    fn package_objective_note(&mut self, package: &str, reason: &str) {
        let message: String = reason.chars().take(256).collect();
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        if host.diagnostics.iter().any(|d| {
            d.code == "objective.query"
                && d.location.as_deref() == Some(package)
                && d.message == message
        }) {
            return;
        }
        note(
            host,
            Diagnostic::warning("objective.query", message).at(package),
        );
    }

    fn package_objective_binding_declared(&self, package: &str, binding: &CounterBinding) -> bool {
        self.packages
            .as_ref()
            .and_then(|h| h.catalog.behaviours().find(|(p, _)| *p == package))
            .is_some_and(|(_, b)| match binding.scope {
                CounterScope::Global => b.state.global.contains_key(&binding.key),
                CounterScope::Player => b.state.player.contains_key(&binding.key),
            })
    }

    pub(in crate::session) fn package_objective_counter(
        &self,
        bot: OwnerId,
        package: &str,
        binding: &CounterBinding,
        initialized: bool,
    ) -> Option<i64> {
        if !self.package_objective_binding_declared(package, binding) {
            return None;
        }
        let ns = self.packages.as_ref()?.store.namespace(package)?;
        let player = self.player_key(bot);
        if initialized {
            binding.read_initialized(ns, &player)
        } else {
            binding.read(ns, &player)
        }
    }

    pub(in crate::session) fn package_objective_carriage(
        &self,
        bot: OwnerId,
        package: &str,
        key: &str,
    ) -> Option<i64> {
        let host = self.packages.as_ref()?;
        if !host
            .catalog
            .behaviours()
            .find(|(p, _)| *p == package)?
            .1
            .state
            .player
            .contains_key(key)
        {
            return None;
        }
        let value = host
            .store
            .namespace(package)?
            .players
            .get(&self.player_key(bot))?
            .get(key)?;
        if value.is_null() {
            Some(0)
        } else {
            value.as_i64().filter(|n| *n >= 0)
        }
    }

    /// The exact same authored box and body overlap as step_zones; no waypoint
    /// geometry supplied by scripts and no definition-name interpretation.
    pub(in crate::session) fn package_objective_zone(
        &self,
        bot: OwnerId,
        package: &str,
        brick: BrickId,
    ) -> Option<(usize, Vec3, Vec3, bool)> {
        let game = self.game_of(bot)?;
        let view = self.brick_view(brick)?;
        if view.game != Some(game.0) {
            return None;
        }
        let host = self.packages.as_ref()?;
        let (_, b) = host.catalog.behaviours().find(|(p, _)| *p == package)?;
        let (index, zone) = b
            .zones
            .iter()
            .enumerate()
            .find(|(_, z)| z.bricks.contains(&view.kind))?;
        let lo = Vec3::from(view.min);
        let hi = Vec3::from(view.max) + Vec3::Y * zone.above;
        let (a, c) = self.peers.get(&bot)?.player.world_bounds();
        let inside = Vec3::from(a).cmple(hi).all() && Vec3::from(c).cmpge(lo).all();
        Some((index, lo, hi, inside))
    }
}
