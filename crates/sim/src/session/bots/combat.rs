//! Inventory tactics through ordinary weapon controls and native collision queries.
//! No item names decide abilities. Unknown scripts, portals and mounted firing
//! keep their existing executor until a truthful typed provider exists.
use super::tactics::{self, Aim, Capability, Context, Delivery, Family, Geometry, Intercept};
use super::*;
use bri_weapons::{Filter, Image, Query, State as ImageState, TargetId};

const PATH_TICKS: u32 = 256;
const SOLVES_PER_TICK: u32 = 2048;
const RAYS_PER_TICK: u32 = 544;
const CHEAP_RESERVE: u32 = 32;
const SWITCH_MARGIN: f32 = 0.15;

#[derive(Default)]
pub(super) struct State {
    movement: Option<Weapon>,
    intent: Option<Intent>,
    cursor: usize,
}
impl State {
    pub(super) fn intent(&self, tick: u64) -> Option<Intent> {
        self.intent.as_ref().filter(|i| i.tick == tick).cloned()
    }
    pub(super) fn movement_hint(&self) -> Option<Weapon> {
        self.movement
    }
}
/// One shared allowance, with a queue of observed fighters rather than owner
/// residues. A temporarily absent owner costs one turn; stale entries expire.
#[derive(Default)]
pub(super) struct Budget {
    tick: Option<u64>,
    owners: Vec<(OwnerId, u64)>,
    cursor: usize,
    turn: Option<OwnerId>,
    solves: u32,
    rays: u32,
}
impl Budget {
    pub(super) fn begin_tick(&mut self, tick: u64) {
        if self.tick == Some(tick) {
            return;
        }
        self.tick = Some(tick);
        self.owners
            .retain(|(_, seen)| tick.saturating_sub(*seen) <= 120);
        self.turn = if self.owners.is_empty() {
            None
        } else {
            self.cursor %= self.owners.len();
            let owner = self.owners[self.cursor].0;
            self.cursor = (self.cursor + 1) % self.owners.len();
            Some(owner)
        };
        self.solves = SOLVES_PER_TICK;
        self.rays = RAYS_PER_TICK;
    }
    fn register(&mut self, owner: OwnerId, tick: u64) -> bool {
        self.begin_tick(tick);
        if let Some(entry) = self.owners.iter_mut().find(|e| e.0 == owner) {
            entry.1 = tick;
        } else if self.owners.len() < MAX_BOTS {
            self.owners.push((owner, tick));
        }
        if self.turn.is_none() {
            self.turn = Some(owner);
        }
        self.turn == Some(owner)
    }
    fn ray(&mut self, expensive: bool, critical: bool) -> bool {
        let reserve = if !critical {
            CHEAP_RESERVE + PATH_TICKS
        } else if expensive {
            CHEAP_RESERVE
        } else {
            0
        };
        if self.rays <= reserve {
            return false;
        }
        self.rays -= 1;
        true
    }
}

#[derive(Clone, Copy)]
pub(super) struct Choice {
    pub(super) slot: usize,
    pub(super) weapon: Weapon,
    pub(super) direction: Vec3,
    pub(super) capability: Capability,
    pub(super) aim: Option<Aim>,
}
#[derive(Clone)]
pub(super) struct Intent {
    pub(super) choice: Choice,
    pub(super) seen: Seen,
    pub(super) tick: u64,
    pub(super) image: String,
    release_authorized: bool,
}
pub(super) enum Decision {
    Unsupported,
    Pending,
    Unsafe,
    /// Holding a charge is allowed, letting it fire is not yet authorized.
    Charging(Choice),
    Ready(Choice),
}

/// A conservative cycle estimate over authored trigger/timeout transitions.
/// Counts the waits of states reached between ordinary onFire visits. Runtime
/// state readiness remains authoritative; this estimate only ranks equipment.
fn cadence(image: &Image) -> u32 {
    let fire = image
        .states
        .iter()
        .position(|s| s.script.eq_ignore_ascii_case("onfire"));
    let Some(start) = fire else {
        return 1;
    };
    let mut at = start;
    let mut ticks = 0_u32;
    let mut visited = [false; 128];
    for _ in 0..image.states.len().min(visited.len()) {
        if at >= visited.len() || visited[at] {
            break;
        }
        visited[at] = true;
        let s = &image.states[at];
        ticks = ticks.saturating_add(s.ticks.max(1));
        let next = s.timeout.or(s.up).or(s.down);
        let Some(next) = next else {
            break;
        };
        if next == start {
            break;
        }
        at = next;
    }
    ticks.max(image.min_shot_ticks).max(1)
}

/// A release-only descriptor must not also launch through held-trigger,
/// timeout, ammo or loaded transitions. Every native onFire entry must be
/// reached via up; checking each finite authored edge proves this for all
/// current states, including recovery cycles, without speculative execution.
fn charge_release_only(image: &Image) -> bool {
    !image.charges()
        || image
            .states
            .first()
            .is_none_or(|s| !s.script.eq_ignore_ascii_case("onfire"))
            && image.states.iter().all(|s| {
                [s.timeout, s.down, s.ammo, s.no_ammo, s.loaded, s.not_loaded]
                    .into_iter()
                    .flatten()
                    .all(|to| {
                        image
                            .states
                            .get(to)
                            .is_none_or(|next| !next.script.eq_ignore_ascii_case("onfire"))
                    })
            })
}

fn capability(
    image: &Image,
    projectile: Option<&bri_weapons::ProjectileDef>,
    scale: f32,
) -> Option<Capability> {
    if !charge_release_only(image) {
        return None;
    }
    // A second firing state can be a stock mounted second hand even without
    // a serialized left_image. Single-launch geometry does not describe it.
    if image
        .states
        .iter()
        .any(|s| s.script.eq_ignore_ascii_case("onfireakimbo"))
    {
        return None;
    }
    if !image.melee {
        return tactics::native_capability(image, projectile, scale, cadence(image)).ok();
    }
    // Session hand frames currently have muzzle == eye, so native melee's
    // geometry correction is exactly one. Reject unresolved non-damage tools.
    let mut native = image.clone();
    native.melee = false;
    let mut cap = tactics::native_capability(&native, projectile, scale, cadence(image)).ok()?;
    if cap.direct_damage <= 0.0 && cap.splash_damage <= 0.0 {
        return None;
    }
    cap.family = Family::Melee;
    cap.near = image.bot.and_then(|b| b.near).unwrap_or(0.0);
    Some(cap)
}

fn movement_weapon(cap: Capability) -> Weapon {
    let (speed, fall) = match cap.delivery {
        Delivery::Projectile(f) => (f.speed, f.fall_per_tick * bri_weapons::TICK_HZ as f32),
        _ => (0.0, 0.0),
    };
    Weapon {
        melee: cap.family == Family::Melee,
        hold: cap.trigger.hold,
        charge: cap.trigger.charge_on_release,
        near: Some(cap.near.max(if cap.family == Family::Melee {
            0.0
        } else {
            (cap.splash_radius + 3.0).max(5.0)
        })),
        reach: cap.reach.min(match cap.delivery {
            Delivery::Projectile(f) => f.speed * PATH_TICKS.min(f.lifetime_ticks) as f32 / 120.0,
            _ => cap.reach,
        }),
        speed,
        fall,
        splash: cap.splash_radius,
    }
}

pub(super) fn choose(
    session: &Session,
    bot: OwnerId,
    seen: Seen,
    tick: u64,
    state: &mut State,
    budget: &mut Budget,
) -> Decision {
    state.movement = None;
    state.intent = None;
    let turn = budget.register(bot, tick);
    if seen.way.carry.is_some() || session.mounted(bot).is_some() {
        return Decision::Unsupported;
    }
    let Some(peer) = session.peers.get(&bot) else {
        return Decision::Unsafe;
    };
    let Some(target) = session.peers.get(&seen.owner).filter(|p| p.combat.alive) else {
        return Decision::Unsafe;
    };
    let Some(actor) = session.weapons.actor(ActorId(bot)) else {
        return Decision::Unsupported;
    };
    let origin = peer.player.eye();
    let velocity = Vec3::from(peer.player.state().velocity);
    let target_velocity = Vec3::from(target.player.state().velocity);
    let target_point = seen.eye - Vec3::Y * 0.5;
    let scale = peer.player.state().scale;
    let selected = actor.selected;
    let mut supported = false;
    let mut pending = false;
    let mut candidates = Vec::with_capacity(actor.inventory.len().min(tactics::MAX_CANDIDATES));
    let mut choices = Vec::with_capacity(candidates.capacity());
    // The held slot is first; remaining slots rotate when the global solver
    // allowance runs out, so late inventory entries cannot starve forever.
    let slots = selected.into_iter().chain(
        (0..actor.inventory.len())
            .map(|n| (n + state.cursor) % actor.inventory.len())
            .filter(|s| Some(*s) != selected),
    );
    for slot in slots.take(tactics::MAX_CANDIDATES) {
        if !turn && Some(slot) != selected {
            continue;
        }
        let Some(item) = actor.inventory[slot]
            .as_ref()
            .and_then(|i| session.weapons.pack.items.get(i))
        else {
            continue;
        };
        let Some(image) = session.weapons.pack.images.get(&item.image) else {
            continue;
        };
        let projectile = image
            .projectile
            .as_ref()
            .and_then(|p| session.weapons.pack.projectiles.get(p));
        let Some(cap) = capability(image, projectile, scale) else {
            continue;
        };
        supported = true;
        if state.movement.is_none() {
            state.movement = Some(movement_weapon(cap));
        }
        let distance = origin.distance(target_point);
        if distance < cap.near || distance > cap.reach {
            continue;
        }
        let ammo = session.weapons.ammo_on_equip(ActorId(bot), slot);
        let ready_rounds = ammo.as_ref().map(|a| {
            if a.supply == bri_weapons::Supply::Both
                && !matches!(a.reserve, bri_weapons::Reserve::Endless)
            {
                match a.reserve {
                    bri_weapons::Reserve::Rounds(n) => a.rounds.min(n),
                    _ => a.rounds,
                }
            } else {
                a.rounds
            }
        });
        if ready_rounds.is_some_and(|n| n < cap.rounds_per_attack) {
            continue;
        }
        let input = Intercept {
            muzzle: origin,
            target: target_point,
            target_velocity,
            shooter_velocity: velocity,
        };
        let mut solutions = [None, None];
        let curved = matches!(cap.delivery, Delivery::Projectile(f) if f.fall_per_tick > 0.0);
        if let Delivery::Projectile(f) = cap.delivery {
            let Ok(mut search) = tactics::InterceptSearch::new(f, input) else {
                continue;
            };
            let limit = PATH_TICKS.min(f.lifetime_ticks);
            while search.result().examined_segments < limit && budget.solves > 0 {
                let count = 16
                    .min(limit - search.result().examined_segments)
                    .min(budget.solves);
                budget.solves -= count;
                search.advance(count);
                if search.result().low.is_some() {
                    break;
                }
            }
            solutions[0] = search.result().low;
            if solutions[0].is_none() {
                pending |= search.result().examined_segments < limit;
                continue;
            }
            // High arcs are optional. Search only the bounded supported
            // horizon, on the fair planning turn, under the same budget.
            if curved && turn && budget.solves > 0 {
                let count = (limit - search.result().examined_segments).min(budget.solves);
                budget.solves -= count;
                search.advance(count);
                solutions[1] = search.result().high;
            }
        }
        let direction =
            solutions[0].map_or((target_point - origin).normalize_or_zero(), |a| a.direction);
        let choice = Choice {
            slot,
            weapon: movement_weapon(cap),
            direction,
            capability: cap,
            aim: solutions[0],
        };
        if curved && !turn {
            if Some(slot) == selected && image.charges() && safe_blast(session, bot, choice, origin)
            {
                state.intent = Some(Intent {
                    choice,
                    seen,
                    tick,
                    image: image.id.clone(),
                    release_authorized: false,
                });
                return Decision::Charging(choice);
            }
            pending = true;
            continue;
        }
        for aim in solutions.into_iter().take(if curved { 2 } else { 1 }) {
            if matches!(cap.delivery, Delivery::Projectile(_)) && aim.is_none() {
                continue;
            }
            let choice = Choice {
                aim,
                direction: aim.map_or(direction, |a| a.direction),
                ..choice
            };
            if !safe_blast(session, bot, choice, origin) {
                continue;
            }
            match clear_path(session, bot, seen.owner, choice, origin, budget, false) {
                None => {
                    pending = true;
                    break;
                }
                Some(false) => continue,
                Some(true) => {}
            }
            let impact = aim.map_or(target_point, |a| a.impact);
            let (self_clearance, ally_clearance) = clearances(
                session,
                bot,
                impact,
                aim.map_or(0.0, |a| a.time_seconds as f32),
            );
            let context = Context {
                distance,
                target_health: target.combat.health.max(1.0),
                hit_probability: 1.0,
                self_clearance,
                ally_clearance,
                blast_margin: 1.0,
                geometry: Geometry::Clear,
                aim,
                ready_rounds,
                opportunity_cost: 0.0,
                switch_seconds: if Some(slot) == selected {
                    0.0
                } else {
                    image.states.first().map_or(0.0, |s| s.ticks as f32 / 120.0)
                },
            };
            candidates.push(tactics::Candidate {
                slot: slot as u8,
                capability: cap,
                context,
            });
            choices.push(choice);
            break;
        }
    }
    if turn {
        state.cursor = state.cursor.wrapping_add(1);
    }
    match tactics::select(&candidates, selected.map(|s| s as u8), SWITCH_MARGIN) {
        Ok(Some(selection)) => {
            let choice = choices
                .into_iter()
                .find(|c| c.slot == usize::from(selection.slot))
                .expect("candidate choice");
            state.movement = Some(choice.weapon);
            let image = actor.inventory[choice.slot]
                .as_ref()
                .and_then(|i| session.weapons.pack.items.get(i))
                .map(|i| i.image.clone())
                .expect("validated item");
            state.intent = Some(Intent {
                choice,
                seen,
                tick,
                image,
                release_authorized: !session.spawn_protected(seen.owner),
            });
            if choice.capability.trigger.charge_on_release && session.spawn_protected(seen.owner) {
                Decision::Charging(choice)
            } else {
                Decision::Ready(choice)
            }
        }
        _ if !supported => Decision::Unsupported,
        _ if pending => Decision::Pending,
        _ => Decision::Unsafe,
    }
}

fn clearances(session: &Session, bot: OwnerId, impact: Vec3, seconds: f32) -> (f32, Option<f32>) {
    let mut own = 0.0;
    let mut ally: Option<f32> = None;
    for (owner, peer) in &session.peers {
        if !peer.combat.alive || (*owner != bot && !session.bot_allies(bot, *owner)) {
            continue;
        }
        let centre = Vec3::from(peer.player.state().feet)
            + Vec3::Y * peer.player.tuning().stand_height * 0.5;
        let velocity = Vec3::from(peer.player.state().velocity);
        let distance = (impact.distance(centre + velocity * seconds)
            - peer.player.tuning().stand_height * 0.5)
            .max(0.0);
        if *owner == bot {
            own = distance;
        } else {
            ally = Some(ally.map_or(distance, |d| d.min(distance)));
        }
    }
    (own, ally)
}
fn safe_blast(session: &Session, bot: OwnerId, choice: Choice, origin: Vec3) -> bool {
    if choice.capability.splash_radius == 0.0 {
        return true;
    }
    let Some(aim) = choice.aim else {
        return false;
    };
    let (own, ally) = clearances(session, bot, aim.impact, aim.time_seconds as f32);
    let safe = choice.capability.splash_radius + 1.0;
    origin.distance(aim.impact) > safe && own > safe && ally.is_none_or(|d| d > safe)
}

/// Exact free-flight segments match the native semi-implicit projectile step.
/// Allies get a swept motion envelope; intended-target collisions are followed
/// by a world probe so their present body cannot hide an obstacle on that chord.
fn clear_path(
    session: &Session,
    bot: OwnerId,
    enemy: OwnerId,
    choice: Choice,
    origin: Vec3,
    budget: &mut Budget,
    critical: bool,
) -> Option<bool> {
    let curved =
        matches!(choice.capability.delivery, Delivery::Projectile(f) if f.fall_per_tick > 0.0);
    let shapes = session.tutorial_shape_targets();
    let mut q = crate::weapon_query::WeaponQuery {
        simulation: &session.simulation,
        affect: &|_, _| true,
        affect_radius: &|_, _| true,
        ally: &|_, _| false,
        catch: &|_, _| false,
        responses: &session.events.projectile_responses,
        truncated_targets: 0,
        shapes: &shapes,
    };
    let (seconds, ticks) = choice.aim.map_or((0.0, 1), |a| {
        (a.time_seconds, if curved { a.flight_tick } else { 1 })
    });
    if ticks > PATH_TICKS {
        return Some(false);
    }
    let mut start = origin;
    for n in 1..=ticks {
        let time = if curved {
            (f64::from(n) / 120.0).min(seconds)
        } else {
            seconds
        };
        let end = match (choice.capability.delivery, choice.aim) {
            (Delivery::Projectile(f), Some(a)) => {
                tactics::flight_position(origin, a.launch_velocity, f.fall_per_tick, time)
                    .ok()?
                    .as_vec3()
            }
            _ => {
                origin
                    + choice.direction
                        * choice.capability.reach.min(
                            origin
                                .distance(session.peers.get(&enemy)?.player.eye() - Vec3::Y * 0.5)
                                + 0.5,
                        )
            }
        };
        if !budget.ray(curved, critical) {
            return None;
        }
        if q.passage(start, end).is_some() {
            return Some(false);
        }
        if !session.bot_fire_clear(bot, start, end, 0.0) {
            return Some(false);
        }
        // Future ally movement is conservatively enclosed about today's body.
        for (owner, peer) in &session.peers {
            if *owner == bot || !peer.combat.alive || !session.bot_allies(bot, *owner) {
                continue;
            }
            let centre = Vec3::from(peer.player.state().feet)
                + Vec3::Y * peer.player.tuning().stand_height * 0.5;
            let radius = peer.player.tuning().stand_height * 0.5
                + Vec3::from(peer.player.state().velocity).length() * time as f32
                + 0.1;
            let delta = end - start;
            let t =
                ((centre - start).dot(delta) / delta.length_squared().max(1e-9)).clamp(0.0, 1.0);
            if centre.distance(start + delta * t) < radius {
                return Some(false);
            }
        }
        let filter = Filter {
            projectile_age_ticks: if curved { Some(n - 1) } else { None },
            source: ActorId(bot),
            players: true,
            world_only: false,
        };
        if let Some(hit) = q.sweep(start, end, filter) {
            if hit.target != TargetId::Actor(ActorId(enemy)) {
                return Some(false);
            }
            if choice.capability.delivery == Delivery::Ray {
                return Some(true);
            }
            if !budget.ray(curved, critical) {
                return None;
            }
            if q.sweep(
                start,
                end,
                Filter {
                    players: false,
                    ..filter
                },
            )
            .is_some()
            {
                return Some(false);
            }
        } else if choice.capability.delivery == Delivery::Ray {
            // A ray may stop here only after its actual direction hits the
            // intended current body. A miss is not a bounded safe attack.
            return Some(false);
        }
        start = end;
    }
    Some(true)
}

/// Validate the actual executor aim after turn limits and error, immediately
/// before a press/release. A desired intercept alone is not authorization.
pub(super) fn validate_fire(
    session: &Session,
    bot: OwnerId,
    seen: Seen,
    mut choice: Choice,
    actual_direction: Vec3,
    budget: &mut Budget,
) -> bool {
    let Some(peer) = session.peers.get(&bot) else {
        return false;
    };
    if !actual_direction.is_finite() || actual_direction.length_squared() < 0.9 {
        return false;
    }
    choice.direction = actual_direction.normalize();
    let origin = peer.player.eye();
    if let (Delivery::Projectile(f), Some(mut aim)) = (choice.capability.delivery, choice.aim) {
        aim.direction = choice.direction;
        aim.launch_velocity =
            choice.direction * f.speed + Vec3::from(peer.player.state().velocity) * f.inherit;
        if !aim.launch_velocity.is_finite() || aim.launch_velocity.length() > 10_000.0 {
            return false;
        }
        let Ok(impact) = tactics::flight_position(
            origin,
            aim.launch_velocity,
            f.fall_per_tick,
            aim.time_seconds,
        ) else {
            return false;
        };
        aim.impact = impact.as_vec3();
        choice.aim = Some(aim);
    }
    if !safe_blast(session, bot, choice, origin) {
        return false;
    }
    // The planned endpoint must still intersect the observed target's motion
    // envelope after the real turn/error. Otherwise a missed shot could pass
    // the endpoint where this bounded validation stops.
    if let Some(aim) = choice.aim {
        let Some(target) = session.peers.get(&seen.owner) else {
            return false;
        };
        let travel = Vec3::from(target.player.state().velocity) * aim.time_seconds as f32;
        let (min, max) = target.player.world_bounds();
        let min = Vec3::from(min) + travel;
        let max = Vec3::from(max) + travel;
        if !aim.impact.cmpge(min).all() || !aim.impact.cmple(max).all() {
            return false;
        }
    }
    clear_path(session, bot, seen.owner, choice, origin, budget, true) == Some(true)
}

/// Called after player movement, collision synchronization and frame update.
/// Bots planned at tick n; Session's physics step advances the launch to n+1.
/// This does not reset the already shared per-step allowance.
pub(super) fn validate_intent(
    session: &Session,
    bot: OwnerId,
    intent: &Intent,
    actual_direction: Vec3,
    budget: &mut Budget,
) -> bool {
    if session.simulation.state().tick != intent.tick.saturating_add(1) {
        return false;
    }
    let Some(brain) = session.bots.brains.get(&bot) else {
        return false;
    };
    if brain.resting
        || !session.peers.get(&bot).is_some_and(|p| p.combat.alive)
        || !session
            .peers
            .get(&intent.seen.owner)
            .is_some_and(|p| p.combat.alive)
        || !session.bot_enemy(bot, &brain.kind, intent.seen.owner)
    {
        return false;
    }
    let Some(actor) = session.weapons.actor(ActorId(bot)) else {
        return false;
    };
    if actor.selected != Some(intent.choice.slot) {
        return false;
    }
    let Some((image, current)) = session.weapons.image_state(ActorId(bot), 0) else {
        return false;
    };
    if image.id != intent.image {
        return false;
    }
    let projectile = image
        .projectile
        .as_ref()
        .and_then(|p| session.weapons.pack.projectiles.get(p));
    if capability(image, projectile, actor.frame.scale) != Some(intent.choice.capability) {
        return false; // A package changed launch scale or native metadata after planning.
    }
    if let Some(ammo) = session.weapons.ammo(ActorId(bot)) {
        let usable = match (ammo.supply, ammo.reserve) {
            (bri_weapons::Supply::Both, bri_weapons::Reserve::Rounds(n)) => ammo.rounds.min(n),
            _ => ammo.rounds,
        };
        if usable < intent.choice.capability.rounds_per_attack {
            return false;
        }
    }
    // Charge holding and recovery cannot launch a shot. Only the actual
    // charged release needs a critical trajectory pass; this keeps a long
    // charge from exhausting the shared ray budget on every fighter tick.
    if image.charges() && (!intent.release_authorized || !image.fires_on_release(current)) {
        return true;
    }
    // Keep aim/movement and proven non-firing charge holds while immunity
    // runs out. Reject only an attack which could spend rounds on no damage.
    if session.spawn_protected(intent.seen.owner) {
        return false;
    }
    validate_fire(
        session,
        bot,
        intent.seen,
        intent.choice,
        actual_direction,
        budget,
    )
}

pub(super) struct TriggerDecision {
    pub(super) down: bool,
    pub(super) abort_charge: bool,
}
/// Current authored state, rather than a global tap phase, determines cadence.
pub(super) fn trigger(
    image: &Image,
    current: &ImageState,
    last_down: bool,
    on_target: bool,
    release_authorized: bool,
) -> TriggerDecision {
    if !on_target {
        return TriggerDecision {
            down: false,
            abort_charge: image.charges(),
        };
    }
    let down = if image.charges() {
        !image.fires_on_release(current) || !release_authorized
    } else if image.bot.unwrap_or_default().fire == bri_weapons::BotFire::Hold {
        true
    } else {
        !last_down && current.down.is_some()
    };
    TriggerDecision {
        down,
        abort_charge: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_budget_rotates_colliding_owner_residues_without_starvation() {
        let owners = [
            1, 17, 33, 49, 65, 81, 97, 113, 129, 145, 161, 177, 193, 209, 225, 241,
        ];
        let mut budget = Budget::default();
        let mut turns = BTreeMap::<OwnerId, usize>::new();
        for tick in 0..owners.len() as u64 * 3 {
            budget.begin_tick(tick);
            for owner in owners {
                if budget.register(owner, tick) {
                    *turns.entry(owner).or_default() += 1;
                }
            }
        }
        assert!(owners.iter().all(|o| turns.get(o).is_some_and(|n| *n >= 2)));
    }
    #[test]
    fn speculative_queries_cannot_spend_the_critical_firing_reserve() {
        let mut budget = Budget::default();
        budget.begin_tick(4);
        let mut planned = 0;
        while budget.ray(true, false) {
            planned += 1;
        }
        assert_eq!(planned, PATH_TICKS);
        let mut actual = 0;
        while budget.ray(true, true) {
            actual += 1;
        }
        assert_eq!(actual, PATH_TICKS);
        let mut cheap = 0;
        while budget.ray(false, true) {
            cheap += 1;
        }
        assert_eq!(
            cheap, CHEAP_RESERVE,
            "held ray firing retains its cheap allowance"
        );
        assert_eq!(budget.rays, 0);
        budget.begin_tick(4);
        assert_eq!(
            budget.rays, 0,
            "post-movement phase cannot reset the allowance"
        );
        budget.begin_tick(5);
        assert_eq!(budget.rays, RAYS_PER_TICK);
    }
    #[test]
    fn a_mixed_charge_graph_requires_an_explicit_launch_descriptor() {
        let pack = bri_weapons::testing::pack();
        let mut image = pack.images[bri_weapons::testing::SPEAR_IMAGE].clone();
        assert!(charge_release_only(&image));
        let mut on_mount = image.clone();
        on_mount.states[0].script = "onFire".into();
        assert!(on_mount.charges());
        assert!(!charge_release_only(&on_mount));
        let fire = image
            .states
            .iter()
            .position(|s| s.script.eq_ignore_ascii_case("onfire"))
            .unwrap();
        image.states[1].down = Some(fire);
        assert!(image.charges());
        assert!(!charge_release_only(&image));
        assert!(
            capability(
                &image,
                image
                    .projectile
                    .as_ref()
                    .and_then(|p| pack.projectiles.get(p)),
                1.0
            )
            .is_none()
        );
    }
    #[test]
    fn actual_postmovement_ray_frame_blocks_a_shooter_entering_a_raycast_wall() {
        use rapier3d::prelude::*;
        let mut world =
            bri_world::World::new("Live launch".into(), "fixture".into(), vec![[1.0; 4]]);
        let mut wall = bri_world::Brick::new(
            bri_world::ContentRef::Resolved(crate::testing::TALL.into()),
            [0.25, 1.5, 0.25],
            1,
        );
        wall.colliding = false; // Bodies can cross; the authored ray wall remains.
        world.bricks.insert(1, wall);
        world.next_brick_id = 2;
        let sim = crate::simulation::Simulation::new(
            world,
            crate::testing::definitions(),
            vec![ColliderBuilder::cuboid(20.0, 0.5, 20.0).translation(Vector::new(0.0, -0.5, 0.0))],
        )
        .unwrap();
        let mut session = Session::new(sim);
        session
            .set_weapon_pack(bri_weapons::testing::pack())
            .unwrap();
        let shooter = session
            .join("Shooter".into(), Vec3::new(-0.27, 0.05, 0.25), true)
            .unwrap();
        let enemy = session
            .join("Target".into(), Vec3::new(-0.27, 0.05, 10.0), false)
            .unwrap();
        let target = session.peers[&enemy].player.eye() - Vec3::Y * 0.5;
        let origin = session.peers[&shooter].player.eye();
        let image = &session.weapons.pack.images[bri_weapons::testing::GUN_IMAGE];
        let projectile = &session.weapons.pack.projectiles[bri_weapons::testing::GUN_PROJECTILE];
        let mut cap = capability(image, Some(projectile), 1.0).unwrap();
        cap.delivery = Delivery::Ray;
        let choice = Choice {
            slot: 0,
            weapon: movement_weapon(cap),
            capability: cap,
            direction: (target - origin).normalize(),
            aim: None,
        };
        let seen = Seen {
            owner: enemy,
            eye: session.peers[&enemy].player.eye(),
            feet: session.peers[&enemy].player.state().feet.into(),
            real: session.peers[&enemy].player.state().feet.into(),
            way: Way {
                aim: target,
                carry: None,
                length: origin.distance(target),
            },
        };
        let mut budget = Budget::default();
        budget.begin_tick(0);
        assert!(validate_fire(
            &session,
            shooter,
            seen,
            choice,
            choice.direction,
            &mut budget
        ));
        let mut crossed = false;
        for sequence in 1..=60 {
            session
                .movement(
                    shooter,
                    sequence,
                    MoveInput {
                        right: 1.0,
                        ..Default::default()
                    },
                )
                .unwrap();
            session.step().unwrap();
            let x = session.peers[&shooter].player.eye().x;
            if (0.0..0.5).contains(&x) {
                crossed = true;
                budget.begin_tick(sequence);
                assert!(
                    !validate_fire(
                        &session,
                        shooter,
                        seen,
                        choice,
                        choice.direction,
                        &mut budget
                    ),
                    "the old valid aim was reused after ordinary movement entered the ray wall"
                );
                break;
            }
        }
        assert!(
            crossed,
            "ordinary movement crossed the non-colliding ray wall"
        );
        budget.begin_tick(100);
        assert!(
            !validate_fire(&session, shooter, seen, choice, Vec3::X, &mut budget),
            "an actual ray miss cannot be truncated at the desired enemy distance"
        );
    }
    #[test]
    fn charge_hold_survives_a_validation_wait_and_unsafe_intent_aborts() {
        let image = bri_weapons::testing::pack().images[bri_weapons::testing::SPEAR_IMAGE].clone();
        let armed = image
            .states
            .iter()
            .find(|s| image.fires_on_release(s))
            .unwrap();
        assert!(trigger(&image, armed, true, true, false).down);
        assert!(!trigger(&image, armed, true, true, true).down);
        let abandoned = trigger(&image, armed, true, false, false);
        assert!(!abandoned.down && abandoned.abort_charge);
    }
}
