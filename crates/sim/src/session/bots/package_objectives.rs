//! Typed package desired state, grounded into the shared planner. Controls are
//! ordinary walking; the package's real pickup/zone policy alone scores returns.
use super::objectives::{
    self, Cause, Completion, DesiredState, GroundedAction, GroundingBudget, Progress,
};
use super::planning::{self, Compare, Effect, EffectGroup, FactValue, Facts, Goal, Predicate};
use super::*;
use bri_package_runtime::bot_objectives::{Carriage, CounterBinding, PickupSource};

#[derive(Default)]
pub(super) struct Discovery {
    pub desired: Vec<DesiredState>,
    pub unsupported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Stamp {
    pub package: String,
    pub actor: OwnerId,
    pub spawn_tick: u64,
    pub game: bri_minigames::GameId,
    pub round: u64,
    pub team: Option<bri_minigames::TeamId>,
    pub source: PickupSource,
    pub source_brick: BrickStamp,
    pub item: String,
    pub epoch: CounterBinding,
    pub observed_epoch: i64,
    pub carriage: Carriage,
    pub destinations: Vec<Destination>,
    pub completion: CounterBinding,
    pub baseline: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BrickStamp {
    id: BrickId,
    kind: String,
    owner: OwnerId,
    color: u8,
    bounds: ([u32; 3], [u32; 3]),
}
impl BrickStamp {
    fn capture(session: &Session, id: BrickId) -> Option<Self> {
        let view = session.brick_view(id)?;
        Some(Self {
            id,
            kind: view.kind,
            owner: view.owner,
            color: view.color,
            bounds: (view.min.map(f32::to_bits), view.max.map(f32::to_bits)),
        })
    }
    fn valid(&self, session: &Session) -> bool {
        Self::capture(session, self.id).as_ref() == Some(self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Destination {
    brick: BrickStamp,
    zone: usize,
    bounds: ([u32; 3], [u32; 3]),
}
impl Destination {
    fn valid(&self, session: &Session, bot: OwnerId, package: &str) -> bool {
        self.brick.valid(session)
            && session
                .package_objective_zone(bot, package, self.brick.id)
                .is_some_and(|(zone, lo, hi, _)| {
                    zone == self.zone
                        && (
                            lo.to_array().map(f32::to_bits),
                            hi.to_array().map(f32::to_bits),
                        ) == self.bounds
                })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Action {
    Pickup {
        stamp: Stamp,
    },
    Visit {
        stamp: Stamp,
        destination: BrickId,
        zone: usize,
        rearming: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Carried {
    Empty,
    ThisSource,
    Other,
}

impl Stamp {
    pub(super) fn source(&self) -> Option<BrickId> {
        Some(self.source.spawner())
    }
    fn context_valid(&self, session: &Session, bot: OwnerId) -> bool {
        self.actor == bot
            && session.game_of(bot) == Some(self.game)
            && session
                .minigames
                .game(self.game)
                .is_ok_and(|g| g.round == self.round)
            && session.peers.get(&bot).is_some_and(|p| {
                p.combat.alive
                    && p.combat.spawn_tick == self.spawn_tick
                    && session
                        .minigames
                        .player(p.combat.player)
                        .is_ok_and(|p| p.team == self.team)
            })
    }
    pub(super) fn validate(&self, session: &Session, bot: OwnerId) -> bool {
        self.context_valid(session, bot)
            && self.source_brick.valid(session)
            && session.package_objective_counter(bot, &self.package, &self.epoch, true)
                == Some(self.observed_epoch)
    }
    pub(super) fn completion(&self, session: &Session, bot: OwnerId) -> Option<bool> {
        // Real completion can atomically recreate its source (and epoch).
        // Observe the preserved actual counter before incarnation rejection.
        if !self.context_valid(session, bot) {
            return None;
        }
        session
            .package_objective_counter(bot, &self.package, &self.completion, false)
            .map(|n| n > self.baseline)
    }
    fn carried(&self, session: &Session, bot: OwnerId) -> Option<Carried> {
        let n = session.package_objective_carriage(bot, &self.package, &self.carriage.key)?;
        if n == 0 {
            return Some(Carried::Empty);
        }
        if n != self.source.spawner() as i64 {
            return Some(Carried::Other);
        }
        if self.carriage.worn.as_ref().is_some_and(|w| {
            session
                .weapons
                .image_state(bri_weapons::ActorId(bot), w.slot)
                .is_none_or(|(image, _)| image.id != w.image)
        }) {
            return None;
        }
        Some(Carried::ThisSource)
    }
    fn pickup_point(&self, session: &Session) -> Option<Vec3> {
        let source = session.brick_view(self.source.spawner())?;
        if source.game != Some(self.game.0) {
            return None;
        }
        let tick = session.simulation.state().tick;
        match self.source {
            PickupSource::Brick { brick } => {
                let item = session.item_spawners.items.get(&brick)?;
                if item.item != self.item || source.item != self.item || tick < item.available_at {
                    return None;
                }
                let at = Vec3::from(item.position);
                Some(Vec3::new(at.x, source.max[1], at.z))
            }
            PickupSource::Drop { drop, .. } => {
                let d = session.weapons.drops().find(|d| d.id == drop)?;
                if d.item != self.item || tick < d.pickup_after || tick >= d.expires {
                    return None;
                }
                Some(d.position)
            }
        }
    }
    fn bytes(&self) -> usize {
        self.package.len()
            + self.item.len()
            + self.carriage.key.len()
            + self.source_brick.kind.len()
            + self
                .destinations
                .iter()
                .map(|d| d.brick.kind.len() + 80)
                .sum::<usize>()
            + self.carriage.worn.as_ref().map_or(0, |w| w.image.len())
            + [&self.epoch, &self.completion]
                .iter()
                .map(|b| b.key.len() + b.path.iter().map(String::len).sum::<usize>())
                .sum::<usize>()
    }
    fn counter_key(&self) -> String {
        format!(
            "package:{}:{}:{:?}:{}:{:?}",
            self.package,
            self.actor,
            self.completion.scope,
            self.completion.key,
            self.completion.path
        )
    }
    fn carriage_key(&self) -> String {
        format!(
            "package:{}:{}:carriage:{}:{}",
            self.package,
            self.actor,
            self.carriage.key,
            self.source.spawner()
        )
    }
}
impl Action {
    pub(super) fn validate(&self, session: &Session, bot: OwnerId) -> bool {
        let stamp = self.stamp();
        if !stamp.validate(session, bot) {
            return false;
        }
        match self {
            // The successful hook consumes the source before this check. Real
            // carriage is therefore evidence, not a missing-source failure.
            Self::Pickup { .. } => {
                stamp.carried(session, bot) == Some(Carried::ThisSource)
                    || stamp.carried(session, bot) == Some(Carried::Empty)
                        && stamp.pickup_point(session).is_some()
            }
            Self::Visit {
                destination, zone, ..
            } => {
                stamp.carried(session, bot) == Some(Carried::ThisSource)
                    && stamp
                        .destinations
                        .iter()
                        .find(|d| d.brick.id == *destination && d.zone == *zone)
                        .is_some_and(|d| d.valid(session, bot, &stamp.package))
            }
        }
    }
    pub(super) fn stamp(&self) -> &Stamp {
        match self {
            Self::Pickup { stamp } | Self::Visit { stamp, .. } => stamp,
        }
    }
    pub(super) fn advance(&mut self, session: &Session, bot: OwnerId) {
        if let Self::Visit {
            stamp,
            destination,
            rearming,
            ..
        } = self
            && *rearming
            && session
                .package_objective_zone(bot, &stamp.package, *destination)
                .is_some_and(|(_, _, _, inside)| !inside)
        {
            *rearming = false;
        }
    }
    pub(super) fn view(&self, session: &Session, bot: OwnerId) -> Option<objectives::View> {
        if !self.validate(session, bot) {
            return None;
        }
        let feet = Vec3::from(session.peers.get(&bot)?.player.state().feet);
        let point = match self {
            Self::Pickup { stamp } => stamp.pickup_point(session).unwrap_or(feet),
            Self::Visit {
                stamp,
                destination,
                rearming,
                ..
            } => {
                let (_, lo, hi, _) =
                    session.package_objective_zone(bot, &stamp.package, *destination)?;
                let p = Vec3::new(
                    (lo.x + hi.x) * 0.5,
                    session.simulation.brick_box(*destination)?.1.y,
                    (lo.z + hi.z) * 0.5,
                );
                if *rearming {
                    Vec3::new(
                        hi.x + session.peers.get(&bot)?.player.tuning().width + 0.5,
                        feet.y,
                        p.z,
                    )
                } else {
                    p
                }
            }
        };
        Some(objectives::View::locomotion(point, point + Vec3::Y))
    }
    pub(super) fn observe(&self, session: &Session, bot: OwnerId) -> Progress {
        if self.stamp().completion(session, bot) == Some(true) {
            return Progress::Changed;
        }
        if !self.validate(session, bot) {
            return Progress::Unavailable;
        }
        match self {
            Self::Pickup { stamp } if stamp.carried(session, bot) == Some(Carried::ThisSource) => {
                Progress::Changed
            }
            _ => Progress::Pending,
        }
    }
}
impl Session {
    pub(super) fn discover_package_objectives(
        &mut self,
        bot: OwnerId,
        budget: &mut GroundingBudget,
    ) -> Result<Discovery, planning::Failure> {
        let Some(game) = self.game_of(bot) else {
            return Ok(Discovery::default());
        };
        let g = self
            .minigames
            .game(game)
            .map_err(|_| planning::Failure::NoPlan)?;
        if g.round_over {
            return Ok(Discovery::default());
        }
        let round = g.round;
        let peer = self.peers.get(&bot).ok_or(planning::Failure::NoPlan)?;
        let spawn_tick = peer.combat.spawn_tick;
        let team = self
            .minigames
            .player(peer.combat.player)
            .map_err(|_| planning::Failure::NoPlan)?
            .team;
        // Charge a bounded query envelope before invoking scripts or cloning
        // their accepted descriptions. Their own VM also caps all allocations.
        budget.reserve(8, 256, 8192)?;
        let discovered = self.package_objective_offers(bot);
        if discovered.budget_exhausted {
            return Err(planning::Failure::ModelBudgetExceeded);
        }
        let mut result = Vec::new();
        for offer in discovered.offers {
            let Some(source_brick) = BrickStamp::capture(self, offer.goal.source.spawner()) else {
                continue;
            };
            let destinations: Vec<_> = offer
                .goal
                .destinations
                .iter()
                .filter_map(|id| {
                    let brick = BrickStamp::capture(self, *id)?;
                    let (zone, lo, hi, _) =
                        self.package_objective_zone(bot, &offer.package, *id)?;
                    Some(Destination {
                        brick,
                        zone,
                        bounds: (
                            lo.to_array().map(f32::to_bits),
                            hi.to_array().map(f32::to_bits),
                        ),
                    })
                })
                .collect();
            let stamp = Stamp {
                package: offer.package,
                actor: bot,
                spawn_tick,
                game,
                round,
                team,
                source: offer.goal.source,
                source_brick,
                item: offer.goal.item,
                epoch: offer.goal.epoch,
                observed_epoch: offer.epoch,
                carriage: offer.goal.carriage,
                destinations,
                completion: offer.goal.completion,
                baseline: offer.baseline,
            };
            let Some(after) = stamp.baseline.checked_add(1) else {
                continue;
            };
            if stamp.carried(self, bot) == Some(Carried::Other) {
                continue;
            }
            result.push(DesiredState {
                id: format!("{}:{}", stamp.package, offer.goal.id),
                predicates: Goal(vec![Predicate {
                    key: stamp.counter_key(),
                    compare: Compare::AtLeast,
                    value: FactValue::Number(after),
                }]),
                completion: Completion::PackageCounter(Box::new(stamp)),
            });
        }
        Ok(Discovery {
            desired: result,
            unsupported: discovered.unsupported,
        })
    }
    pub(super) fn append_package_actions(
        &self,
        bot: OwnerId,
        stamp: &Stamp,
        facts: &mut Facts,
        budget: &mut GroundingBudget,
    ) -> Result<Vec<GroundedAction>, planning::Failure> {
        if !stamp.validate(self, bot) {
            return Err(planning::Failure::NoPlan);
        }
        let carried = stamp
            .carried(self, bot)
            .ok_or(planning::Failure::Unsupported)?;
        if carried == Carried::Other {
            return Err(planning::Failure::NoPlan);
        }
        let now = self
            .package_objective_counter(bot, &stamp.package, &stamp.completion, false)
            .ok_or(planning::Failure::Unsupported)?;
        let after = stamp
            .baseline
            .checked_add(1)
            .ok_or(planning::Failure::Unsupported)?;
        let carriage_key = stamp.carriage_key();
        let counter_key = stamp.counter_key();
        facts.insert(
            carriage_key.clone(),
            FactValue::Bool(carried == Carried::ThisSource),
        );
        facts.insert(counter_key.clone(), FactValue::Number(now));
        let mut result = Vec::new();
        if carried == Carried::Empty
            && let Some(point) = stamp.pickup_point(self)
        {
            budget.reserve(1, 4, stamp.bytes() * 2 + carriage_key.len())?;
            result.push(self.package_model_action(
                bot,
                Action::Pickup {
                    stamp: stamp.clone(),
                },
                point,
                vec![Predicate {
                    key: carriage_key.clone(),
                    compare: Compare::Equal,
                    value: FactValue::Bool(false),
                }],
                Effect::Set {
                    key: carriage_key.clone(),
                    value: FactValue::Bool(true),
                },
                "pickup",
            ));
        }
        // Destinations are read from the preserved owner description on the
        // stamp, never rediscovered by scanning all world bricks each tick.
        for destination_stamp in &stamp.destinations {
            if !destination_stamp.valid(self, bot, &stamp.package) {
                continue;
            }
            let destination = destination_stamp.brick.id;
            let Some((zone, lo, hi, inside)) =
                self.package_objective_zone(bot, &stamp.package, destination)
            else {
                continue;
            };
            budget.reserve(
                1,
                4,
                stamp.bytes() * 2 + counter_key.len() + carriage_key.len(),
            )?;
            let point = (lo + hi) * 0.5;
            result.push(self.package_model_action(
                bot,
                Action::Visit {
                    stamp: stamp.clone(),
                    destination,
                    zone,
                    rearming: inside,
                },
                point,
                vec![Predicate {
                    key: carriage_key.clone(),
                    compare: Compare::Equal,
                    value: FactValue::Bool(true),
                }],
                Effect::Set {
                    key: counter_key.clone(),
                    value: FactValue::Number(after),
                },
                &format!("visit:{destination}"),
            ));
        }
        Ok(result)
    }
    fn package_model_action(
        &self,
        bot: OwnerId,
        executor: Action,
        point: Vec3,
        preconditions: Vec<Predicate>,
        effect: Effect,
        label: &str,
    ) -> GroundedAction {
        let stamp = executor.stamp();
        let feet = Vec3::from(self.peers[&bot].player.state().feet);
        let model = planning::Action {
            id: format!(
                "package:{}:{}:{}:{label}",
                stamp.package,
                stamp.source.spawner(),
                stamp.observed_epoch
            ),
            cost: 1 + (feet.distance(point) * 10.0).min(100000.0) as u32,
            preconditions,
            effect_groups: vec![EffectGroup {
                guards: vec![],
                effects: vec![effect],
            }],
        };
        GroundedAction {
            model,
            cause: Cause::Package(stamp.clone()),
            executor: objectives::Executor::Package(executor),
        }
    }
}
