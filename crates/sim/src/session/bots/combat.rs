//! Inventory tactics through ordinary weapon controls and native collision queries.
//! No item names decide abilities. Unknown scripts, portals and mounted firing
//! keep their existing executor until a truthful typed provider exists.
use super::tactics::{self, Aim, Capability, Context, Delivery, Family, Intercept};
use super::*;
use bri_weapons::{Filter, Image, Query, State as ImageState, TargetId};

const PATH_TICKS: u32 = 256;
const SOLVES_PER_TICK: u32 = 2048;
const RAYS_PER_TICK: u32 = 544;
const CHEAP_RESERVE: u32 = 32;

#[derive(Default)]
pub(super) struct State {
    movement: Option<Weapon>,
    intent: Option<Intent>,
    cursor: usize,
    /// The last tick the weapon in hand was one it could attack with.
    usable: u64,
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
    /// Whether `owner` has the planning turn this tick (every slot weighed).
    pub(super) fn has_turn(&self, owner: OwnerId, tick: u64) -> bool {
        self.tick == Some(tick) && self.turn == Some(owner)
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
    /// Aimed at a surface beside the target that its splash reaches
    /// (`surprise`), rather than at the target itself.
    pub(super) surface: bool,
    /// What the attack is worth ([`tactics::worth`]): its expected damage
    /// and the seconds it occupies. Nothing for a wind-up kept going.
    pub(super) dealt: f32,
    pub(super) seconds: f32,
}
#[derive(Clone)]
pub(super) struct Intent {
    pub(super) choice: Choice,
    pub(super) seen: Seen,
    pub(super) tick: u64,
    pub(super) image: String,
    release_authorized: bool,
    shooter_spawn: u64,
    target_spawn: u64,
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
    super::charged_control::release_only(image)
}

/// How much a splash aim (feet, or a surface beside the target) is
/// preferred over the body: a blast at the feet still lands when a dodging
/// body would have made the shot miss, so players aim rockets low.
const SPLASH_AIM: f32 = 1.5;

pub(super) fn capability(
    image: &Image,
    projectile: Option<&bri_weapons::ProjectileDef>,
    scale: f32,
) -> Option<Capability> {
    if super::super::tools::native_hammer(image) {
        if !scale.is_finite() || !(0.01..=100.0).contains(&scale) {
            return None;
        }
        return Some(Capability {
            family: Family::Melee,
            delivery: Delivery::Contact,
            trigger: tactics::Trigger::default(),
            reach: super::super::tools::TOOL_RANGE * scale,
            near: 0.0,
            direct_damage: super::super::tools::HAMMER_DAMAGE,
            splash_damage: 0.0,
            splash_radius: 0.0,
            arm_ticks: 0,
            cadence_ticks: cadence(image),
            rounds_per_attack: 1,
            push: (0.0, 0.0),
        });
    }
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
    let cap = tactics::native_capability(&native, projectile, scale, cadence(image)).ok()?;
    let mut cap = cap;
    cap.family = Family::Melee;
    cap.near = image.bot.and_then(|b| b.near).unwrap_or(0.0);
    Some(cap)
}

/// Cheap conservative availability for utility arbitration, not an aim or
/// safety decision. An unknown mechanism counts only if the bot can fire it.
pub(super) fn has_possible_attack(session: &Session, bot: OwnerId) -> bool {
    let Some(actor) = session.weapons.actor(ActorId(bot)) else {
        return false;
    };
    if actor.inventory.len() > tactics::MAX_CANDIDATES {
        return true;
    }
    let scale = session
        .peers
        .get(&bot)
        .map_or(1.0, |p| p.player.state().scale);
    for (slot, item) in actor.inventory.iter().enumerate() {
        let Some(item) = item else {
            continue;
        };
        let Some(image) = session
            .weapons
            .pack
            .items
            .get(item)
            .and_then(|i| session.weapons.pack.images.get(&i.image))
        else {
            return true;
        };
        let projectile = image
            .projectile
            .as_ref()
            .and_then(|id| session.weapons.pack.projectiles.get(id));
        if let Some(cap) = capability(image, projectile, scale) {
            // One that only pushes (a broom) still moves someone off what
            // they are after.
            if cap.direct_damage <= 0.0 && cap.splash_damage <= 0.0 && !cap.pushes() {
                continue;
            }
            let ammo = session.weapons.ammo_on_equip(ActorId(bot), slot);
            if ammo.as_ref().is_none_or(|a| {
                available_rounds(a) >= cap.rounds_per_attack
                    || matches!(a.reserve, bri_weapons::Reserve::Endless)
                    || matches!(a.reserve,bri_weapons::Reserve::Rounds(n) if n > 0)
            }) {
                return true;
            }
            continue;
        }
        // Only an explicit manipulation descriptor with no native attack
        // metadata identifies a noncombat tool. No IDs or command-name
        // guesses. Anything else is an attack only if the bot can use it
        // as one (`Session::image_weapon`, the reader it fires by).
        if !known_noncombat_manipulation(image) && session.image_weapon(image, scale).is_some() {
            return true;
        }
    }
    false
}

/// An item whose image has a native attack that does damage: one worth
/// picking up to fight with. Unlike `has_possible_attack`, an unknown
/// mechanism does not count.
pub(super) fn item_attacks(session: &Session, item: &str, scale: f32) -> bool {
    let pack = &session.weapons.pack;
    let Some(image) = pack.items.get(item).and_then(|i| pack.images.get(&i.image)) else {
        return false;
    };
    let projectile = image
        .projectile
        .as_ref()
        .and_then(|id| pack.projectiles.get(id));
    capability(image, projectile, scale)
        .is_some_and(|cap| cap.direct_damage > 0.0 || cap.splash_damage > 0.0 || cap.pushes())
}

/// An item whose attack pushes a player (a broom's shove).
pub(super) fn item_pushes(session: &Session, item: &str, scale: f32) -> bool {
    let pack = &session.weapons.pack;
    let Some(image) = pack.items.get(item).and_then(|i| pack.images.get(&i.image)) else {
        return false;
    };
    let projectile = image
        .projectile
        .as_ref()
        .and_then(|id| pack.projectiles.get(id));
    capability(image, projectile, scale).is_some_and(Capability::pushes)
}

/// An item that attacks from a distance (not a swing or a stab): one a
/// bot can fight with from where it sits.
pub(super) fn item_attacks_from_afar(session: &Session, item: &str, scale: f32) -> bool {
    let pack = &session.weapons.pack;
    let Some(image) = pack.items.get(item).and_then(|i| pack.images.get(&i.image)) else {
        return false;
    };
    let projectile = image
        .projectile
        .as_ref()
        .and_then(|id| pack.projectiles.get(id));
    capability(image, projectile, scale).is_some_and(|cap| {
        cap.family != Family::Melee && (cap.direct_damage > 0.0 || cap.splash_damage > 0.0)
    })
}

/// How much splash damage counts for, against direct damage, in an item's
/// worth: a blast hurts less the farther it lands.
const SPLASH_WORTH: f32 = 0.6;
/// The reach at and past which an item's reach adds nothing more.
const FULL_REACH: f32 = 48.0;
/// The worth of an item with an attack its data does not describe (a
/// script fires it): something to fight with, below any described one.
const UNKNOWN_WORTH: f32 = 1.0;

/// What an item is worth to fight with, from its data alone: the damage it
/// deals a second (splash counted at `SPLASH_WORTH`), times how much of
/// `FULL_REACH` it reaches (square-rooted, so reach matters less than
/// damage). 0 for a building tool or a tool known not to attack. The same
/// estimate ranks the inventory and the upgrades lying about.
pub(super) fn item_worth(session: &Session, item: &str, scale: f32) -> f32 {
    if session.weapons.building_tool(item) {
        return 0.0;
    }
    let pack = &session.weapons.pack;
    let Some(image) = pack.items.get(item).and_then(|i| pack.images.get(&i.image)) else {
        return 0.0;
    };
    let projectile = image
        .projectile
        .as_ref()
        .and_then(|id| pack.projectiles.get(id));
    let (damage, reach) = match capability(image, projectile, scale) {
        Some(cap) => (cap.damage(SPLASH_WORTH), cap.reach),
        None => {
            if known_noncombat_manipulation(image) {
                return 0.0;
            }
            // A script fires it: estimate from what its data does say.
            let shot = image.shot.as_ref();
            let ray = shot.and_then(|s| s.hitscan.as_ref());
            let count = shot.map_or(1, |s| s.projectiles.max(1)) as f32;
            let direct = ray
                .and_then(|r| r.damage)
                .or(projectile.map(|p| p.damage))
                .unwrap_or(0.0)
                .max(0.0);
            let splash = projectile.map_or(0.0, |p| p.explosion.damage.max(0.0));
            let reach =
                ray.map(|r| r.range * scale).or(projectile
                    .map(|p| p.speed * p.lifetime_ticks as f32 / bri_weapons::TICK_HZ as f32));
            let reach = image.bot.and_then(|b| b.reach).or(reach).unwrap_or(3.0);
            (count * direct + splash * SPLASH_WORTH, reach)
        }
    };
    let rate = bri_weapons::TICK_HZ as f32 / cadence(image).max(1) as f32;
    let worth = damage * rate * (reach / FULL_REACH).clamp(0.05, 1.0).sqrt();
    if worth.is_finite() && worth > 0.0 {
        worth
    } else {
        UNKNOWN_WORTH
    }
}

fn known_noncombat_manipulation(image: &bri_weapons::Image) -> bool {
    image.bot.and_then(|b| b.manipulation).is_some()
        && image.projectile.is_none()
        && !image.melee
        && image.shot.is_none()
        && image.volleys.is_empty()
        && image.last_shot.is_none()
        && image.state_shots.is_empty()
        && image.scripts.is_empty()
        && image.left_image.is_none()
        && image.cook.is_none()
}

fn available_rounds(a: &bri_weapons::runtime::AmmoView) -> u32 {
    if a.supply == bri_weapons::Supply::Both && !matches!(a.reserve, bri_weapons::Reserve::Endless)
    {
        match a.reserve {
            bri_weapons::Reserve::Rounds(n) => a.rounds.min(n),
            _ => a.rounds,
        }
    } else {
        a.rounds
    }
}

/// How a bot handles a weapon it fights with natively (`capability`):
/// its band, flight and blast, and `spread` (`fire::image_spread`). The one
/// reader of a natively modelled image (`Session::bot_weapon` uses it too).
pub(super) fn weapon_of(cap: Capability, spread: f32) -> Weapon {
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
            Weapon::standoff(cap.splash_radius)
        })),
        reach: cap.reach.min(match cap.delivery {
            Delivery::Projectile(f) => {
                f.speed * PATH_TICKS.min(f.lifetime_ticks) as f32 / bri_weapons::TICK_HZ as f32
            }
            _ => cap.reach,
        }),
        speed,
        fall,
        splash: cap.splash_radius,
        spread,
    }
}

/// The best shot `bot` has at `seen` from `origin` (its eye, or a spot it
/// weighs standing on).
#[allow(clippy::too_many_arguments)]
pub(super) fn choose(
    session: &Session,
    bot: OwnerId,
    seen: Seen,
    origin: Vec3,
    tick: u64,
    state: &mut State,
    budget: &mut Budget,
    mind: &mut super::surprise::Mind,
) -> Decision {
    // The guard as of the last tick: the gate is set later in the tick.
    let gate = mind.gate;
    state.movement = None;
    let previous = state.intent.take();
    let turn = budget.register(bot, tick);
    // Splash aims it may take instead of the body, by slot.
    let mut variants: Vec<(usize, u32, Choice, f32)> = Vec::new();
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
    let velocity = Vec3::from(peer.player.state().velocity);
    // A bot leads by about the target's velocity, a little under or over
    // as its own seeded drift goes, never a perfect intercept.
    let target_velocity = Vec3::from(target.player.state().velocity) * super::lead(bot, tick);
    let target_point = seen.aim;
    // Gathered when a candidate first needs them; its own body where it
    // would stand to shoot from `origin`.
    let shift = origin - peer.player.eye();
    let bodies = std::cell::LazyCell::new(|| {
        let mut bodies = Bodies::of(session, bot);
        if let Some(own) = bodies.own.as_mut() {
            own.centre += shift;
        }
        bodies
    });
    let scale = peer.player.state().scale;
    let selected = actor.selected;
    let mut supported = false;
    let mut pending = false;
    let mut charge_continuation = None;
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
        if (!turn || charge_continuation.is_some()) && Some(slot) != selected {
            // A live wind-up belongs to the unchanged participant/equipment.
            // A transient range/intercept miss may keep it tracking, but may
            // not hand its trigger sequence to another inventory choice.
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
        if Some(slot) == selected
            && image.charges()
            && actor.trigger_held()
            && previous.as_ref().is_some_and(|intent| {
                intent.seen.owner != seen.owner
                    || intent.shooter_spawn != peer.combat.spawn_tick
                    || intent.target_spawn != target.combat.spawn_tick
                    || intent.image != image.id
                    || intent.choice.slot != slot
            })
        {
            return Decision::Unsafe; // This is a different attack participant/equipment.
        }
        supported = true;
        if state.movement.is_none() {
            state.movement = Some(weapon_of(cap, super::fire::image_spread(image)));
        }
        let distance = origin.distance(target_point);
        let ammo = session.weapons.ammo_on_equip(ActorId(bot), slot);
        let ready_rounds = ammo.as_ref().map(available_rounds);
        if ready_rounds.is_some_and(|n| n < cap.rounds_per_attack) {
            continue;
        }
        if Some(slot) == selected
            && image.charges()
            && actor.trigger_held()
            && previous.as_ref().is_some_and(|intent| {
                intent.tick.saturating_add(1) == tick
                    && intent.image == image.id
                    && intent.choice.slot == slot
                    && intent.choice.capability == cap
                    && intent.seen.owner == seen.owner
                    && intent.shooter_spawn == peer.combat.spawn_tick
                    && intent.target_spawn == target.combat.spawn_tick
            })
        {
            // A transient missed intercept, blocked path or range boundary
            // is not loss of this live participant/equipment. Keep the
            // native wind-up while tracking; this intent cannot release.
            charge_continuation = Some(Choice {
                slot,
                weapon: weapon_of(cap, super::fire::image_spread(image)),
                capability: cap,
                direction: (target_point - origin).normalize_or_zero(),
                aim: None,
                surface: false,
                dealt: 0.0,
                seconds: 0.0,
            });
        }
        if distance < cap.near || distance > cap.reach {
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
            weapon: weapon_of(cap, super::fire::image_spread(image)),
            direction,
            capability: cap,
            aim: solutions[0],
            surface: false,
            dealt: 0.0,
            seconds: 0.0,
        };
        if curved && !turn {
            if Some(slot) == selected && image.charges() && safe_blast(&bodies, choice, origin) {
                state.intent = Some(Intent {
                    choice,
                    seen,
                    tick,
                    image: image.id.clone(),
                    release_authorized: false,
                    shooter_spawn: peer.combat.spawn_tick,
                    target_spawn: target.combat.spawn_tick,
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
            if !safe_blast(&bodies, choice, origin) {
                continue;
            }
            match clear_path(
                session, bot, seen.owner, choice, origin, &bodies, budget, false,
            ) {
                None => {
                    pending = true;
                    break;
                }
                Some(false) => continue,
                Some(true) => {}
            }
            let impact = aim.map_or(target_point, |a| a.impact);
            let (self_clearance, ally_clearance) =
                clearances(&bodies, impact, aim.map_or(0.0, |a| a.time_seconds as f32));
            let context = Context {
                distance,
                target_health: target.combat.health.max(1.0),
                hit_probability: 1.0,
                self_clearance,
                ally_clearance,
                blast_margin: 1.0,
                aim,
                ready_rounds,
                opportunity_cost: 0.0,
                push_harm: tactics::PUSH_WORTH,
                switch_seconds: if Some(slot) == selected {
                    0.0
                } else {
                    image
                        .states
                        .first()
                        .map_or(0.0, |s| s.ticks as f32 / bri_weapons::TICK_HZ as f32)
                },
            };
            candidates.push(tactics::Candidate {
                slot: slot as u8,
                capability: cap,
                context,
            });
            let (dealt, seconds) = tactics::worth(cap, context).unwrap_or_default();
            choices.push(Choice {
                dealt,
                seconds,
                ..choice
            });
            // A splash weapon may aim at the feet, or at a surface beside
            // the target, where its real blast still hurts.
            if cap.splash_radius > 0.0 && cap.splash_damage > 0.0 {
                let feet = seen.feet + Vec3::Y * 0.15;
                let centre = target_point;
                let surface =
                    session.surprise_surface(seen.owner, centre, origin, cap.splash_radius);
                for (aim, point, moving) in [
                    (super::surprise::AIM_FEET, Some(feet), target_velocity),
                    (super::surprise::AIM_SURFACE, surface, Vec3::ZERO),
                ] {
                    let Some(point) = point else { continue };
                    let solve = Solve {
                        slot,
                        origin,
                        velocity,
                        point,
                        moving,
                        surface: aim == super::surprise::AIM_SURFACE,
                        spread: super::fire::image_spread(image),
                    };
                    if let Some((c, score)) = variant(
                        session, bot, seen.owner, cap, solve, context, &bodies, budget,
                    ) {
                        variants.push((slot, aim, c, score * SPLASH_AIM));
                    }
                }
            }
            break;
        }
    }
    if turn {
        state.cursor = state.cursor.wrapping_add(1);
    }
    // The weapon in hand is held through a brief spell where it cannot
    // attack (the target inside its blast, an ally across the line, a
    // reload): the bot holds fire for the hold time instead of swapping
    // to another and back.
    if let Some(held) = selected {
        if candidates.iter().any(|c| usize::from(c.slot) == held) {
            state.usable = tick;
        } else if charge_continuation.is_none()
            && state.usable > 0
            && super::behaviour::paused_hold(
                state.usable,
                tick,
                session.bots.brains[&bot].kind.hold(),
            )
        {
            return Decision::Pending;
        }
    }
    match tactics::select(&candidates) {
        Ok(Some(selection)) => {
            use super::surprise::{AIM_TORSO, Choice, Domain};
            let kind = &session.bots.brains[&bot].kind;
            let (cfg, rule) = (&kind.surprise, kind.hold());
            let scores: Vec<(u32, f32)> = candidates
                .iter()
                .filter_map(|c| {
                    tactics::suitability(c.capability, c.context)
                        .ok()
                        .map(|s| (u32::from(c.slot), s))
                })
                .collect();
            // The hold rule picks among weapons on the planning turn, when
            // every slot is weighed; otherwise the held weapon while it
            // still works, else the best.
            let viable = |slot: u32| scores.iter().any(|(s, v)| *s == slot && *v > 0.0);
            let mut slot = mind
                .chosen(Domain::Weapon)
                .filter(|s| viable(*s))
                .map_or(usize::from(selection.slot), |s| s as usize);
            if turn && charge_continuation.is_none() {
                let ask = Choice {
                    domain: Domain::Weapon,
                    options: &scores,
                    interrupt: false,
                    paused: false,
                    must: &[],
                    fixed: &[],
                };
                slot = mind.pick(cfg, rule, ask, gate, tick) as usize;
            }
            let mut choice = choices
                .into_iter()
                .find(|c| c.slot == slot)
                .expect("candidate choice");
            let torso = scores
                .iter()
                .find(|(s, _)| *s as usize == slot)
                .map_or(0.0, |(_, score)| *score);
            let aims: Vec<(u32, f32)> = std::iter::once((AIM_TORSO, torso))
                .chain(variants.iter().filter(|v| v.0 == slot).map(|v| (v.1, v.3)))
                .collect();
            let ask = Choice {
                domain: Domain::Aim,
                options: &aims,
                interrupt: false,
                paused: false,
                must: &[],
                fixed: &[],
            };
            let aim = mind.pick(cfg, rule, ask, gate, tick);
            if let Some(v) = variants.iter().find(|v| v.0 == slot && v.1 == aim) {
                choice = v.2;
            }
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
                shooter_spawn: peer.combat.spawn_tick,
                target_spawn: target.combat.spawn_tick,
            });
            if choice.capability.trigger.charge_on_release && session.spawn_protected(seen.owner) {
                Decision::Charging(choice)
            } else {
                Decision::Ready(choice)
            }
        }
        _ if charge_continuation.is_some() => {
            let choice = charge_continuation.unwrap();
            state.movement = Some(choice.weapon);
            let image = actor.inventory[choice.slot]
                .as_ref()
                .and_then(|i| session.weapons.pack.items.get(i))
                .map(|i| i.image.clone())
                .expect("validated held item");
            state.intent = Some(Intent {
                choice,
                seen,
                tick,
                image,
                release_authorized: false,
                shooter_spawn: peer.combat.spawn_tick,
                target_spawn: target.combat.spawn_tick,
            });
            Decision::Charging(choice)
        }
        _ if !supported => Decision::Unsupported,
        _ if pending => Decision::Pending,
        _ => Decision::Unsafe,
    }
}

/// Where a splash aim goes ([`variant`]).
#[derive(Clone, Copy)]
struct Solve {
    slot: usize,
    origin: Vec3,
    velocity: Vec3,
    /// The point aimed at, and how it moves (the feet move with the body).
    point: Vec3,
    moving: Vec3,
    surface: bool,
    /// The weapon's spread (`fire::image_spread`).
    spread: f32,
}
/// A splash aim at `solve.point` instead of the body: a solved intercept
/// whose blast still reaches the body where it will be, safe and clear
/// like any shot, and scored by its splash alone.
#[allow(clippy::too_many_arguments)]
fn variant(
    session: &Session,
    bot: OwnerId,
    enemy: OwnerId,
    cap: Capability,
    solve: Solve,
    context: Context,
    bodies: &Bodies,
    budget: &mut Budget,
) -> Option<(Choice, f32)> {
    let Delivery::Projectile(f) = cap.delivery else {
        return None;
    };
    let mut search = tactics::InterceptSearch::new(
        f,
        Intercept {
            muzzle: solve.origin,
            target: solve.point,
            target_velocity: solve.moving,
            shooter_velocity: solve.velocity,
        },
    )
    .ok()?;
    let limit = PATH_TICKS.min(f.lifetime_ticks);
    while search.result().low.is_none()
        && search.result().examined_segments < limit
        && budget.solves > 0
    {
        let count = 16
            .min(limit - search.result().examined_segments)
            .min(budget.solves);
        budget.solves -= count;
        search.advance(count);
    }
    let aim = search.result().low?;
    let body = session.peers.get(&enemy)?;
    let centre = Vec3::from(body.player.state().feet)
        + Vec3::Y * body.player.tuning().stand_height * 0.5
        + Vec3::from(body.player.state().velocity) * aim.time_seconds as f32;
    if aim.impact.distance(centre) > cap.splash_radius * 0.9 {
        return None;
    }
    let choice = Choice {
        slot: solve.slot,
        weapon: weapon_of(cap, solve.spread),
        direction: aim.direction,
        capability: cap,
        aim: Some(aim),
        surface: solve.surface,
        dealt: 0.0,
        seconds: 0.0,
    };
    if !safe_blast(bodies, choice, solve.origin)
        || clear_path(
            session,
            bot,
            enemy,
            choice,
            solve.origin,
            bodies,
            budget,
            false,
        ) != Some(true)
    {
        return None;
    }
    let (self_clearance, ally_clearance) = clearances(bodies, aim.impact, aim.time_seconds as f32);
    let splash = Capability {
        direct_damage: 0.0,
        ..cap
    };
    let context = Context {
        aim: Some(aim),
        self_clearance,
        ally_clearance,
        ..context
    };
    let score = tactics::suitability(splash, context).ok()?;
    let (dealt, seconds) = tactics::worth(splash, context).ok()?;
    Some((
        Choice {
            dealt,
            seconds,
            ..choice
        },
        score,
    ))
}

/// A body a shot must spare, as one decision sees it ([`Bodies`]).
#[derive(Clone, Copy)]
struct Spared {
    centre: Vec3,
    /// Half the standing height.
    half: f32,
    velocity: Vec3,
    /// It rides the vehicle the shooter rides.
    shares_mount: bool,
}
/// The shooter's living body and its living allies'. Nothing moves while a
/// bot decides, so its side is looked up once a decision, not again for
/// every candidate and path segment (`bot_allies` is the dear part).
struct Bodies {
    own: Option<Spared>,
    allies: Vec<Spared>,
}
impl Bodies {
    fn of(session: &Session, bot: OwnerId) -> Self {
        let mount = session.mounted(bot).map(|(v, _)| v);
        let mut own = None;
        let mut allies = Vec::new();
        for (owner, peer) in &session.peers {
            if !peer.combat.alive || (*owner != bot && !session.bot_allies(bot, *owner)) {
                continue;
            }
            let half = peer.player.tuning().stand_height * 0.5;
            let spared = Spared {
                centre: Vec3::from(peer.player.state().feet) + Vec3::Y * half,
                half,
                velocity: Vec3::from(peer.player.state().velocity),
                shares_mount: mount.is_some() && mount == session.mounted(*owner).map(|(v, _)| v),
            };
            if *owner == bot {
                own = Some(spared);
            } else {
                allies.push(spared);
            }
        }
        Self { own, allies }
    }
}
fn clearances(bodies: &Bodies, impact: Vec3, seconds: f32) -> (f32, Option<f32>) {
    let distance =
        |b: &Spared| (impact.distance(b.centre + b.velocity * seconds) - b.half).max(0.0);
    let own = bodies.own.as_ref().map_or(0.0, distance);
    let ally = bodies
        .allies
        .iter()
        .map(distance)
        .reduce(|a: f32, b: f32| a.min(b));
    (own, ally)
}
fn safe_blast(bodies: &Bodies, choice: Choice, origin: Vec3) -> bool {
    if choice.capability.splash_radius == 0.0 {
        return true;
    }
    let Some(aim) = choice.aim else {
        return false;
    };
    let (own, ally) = clearances(bodies, aim.impact, aim.time_seconds as f32);
    let safe = choice.capability.splash_radius + 1.0;
    origin.distance(aim.impact) > safe && own > safe && ally.is_none_or(|d| d > safe)
}

/// Exact free-flight segments match the native semi-implicit projectile step.
/// Allies get a swept motion envelope; intended-target collisions are followed
/// by a world probe so their present body cannot hide an obstacle on that chord.
#[allow(clippy::too_many_arguments)]
fn clear_path(
    session: &Session,
    bot: OwnerId,
    enemy: OwnerId,
    choice: Choice,
    origin: Vec3,
    bodies: &Bodies,
    budget: &mut Budget,
    critical: bool,
) -> Option<bool> {
    if choice.capability.delivery == Delivery::Contact {
        if !budget.ray(false, critical) {
            return None;
        }
        // Hammer's native callback traces all bricks, including non-raycast
        // ones. WeaponQuery is a different ray and cannot authorize its swing.
        return Some(
            session
                .native_hammer_target(bot, choice.direction)
                .ok()
                .flatten()
                == Some(TargetId::Actor(ActorId(enemy))),
        );
    }
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
            (f64::from(n) / f64::from(bri_weapons::TICK_HZ)).min(seconds)
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
                            origin.distance({
                                let p = &session.peers.get(&enemy)?.player;
                                sightlines::aim_point(p.eye(), p.state().scale)
                            }) + 0.5,
                        )
            }
        };
        if !budget.ray(curved, critical) {
            return None;
        }
        if q.passage(start, end).is_some() {
            return Some(false);
        }
        let Some(space) = super::interactions::shot_space(start, end, 0.0, 0.0, 0.0) else {
            return Some(false);
        };
        let mut unmounted = bodies.allies.iter().filter(|a| !a.shares_mount);
        if unmounted.any(|a| space.holds(a.centre, a.half)) {
            return Some(false);
        }
        // Future ally movement is conservatively enclosed about today's body.
        for ally in &bodies.allies {
            let centre = ally.centre;
            let radius = ally.half + ally.velocity.length() * time as f32 + 0.1;
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
                // A surface shot that reaches its surface goes off there.
                let surface = choice.surface
                    && !matches!(hit.target, TargetId::Actor(_))
                    && choice
                        .aim
                        .is_some_and(|a| hit.position.distance(a.impact) < 0.75);
                return Some(surface);
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
    let bodies = Bodies::of(session, bot);
    if !safe_blast(&bodies, choice, origin) {
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
        if choice.surface {
            // A surface shot lands beside the body: its blast must reach it.
            if aim.impact.distance((min + max) * 0.5) > choice.capability.splash_radius {
                return false;
            }
        } else if !aim.impact.cmpge(min).all() || !aim.impact.cmple(max).all() {
            return false;
        }
    }
    clear_path(
        session, bot, seen.owner, choice, origin, &bodies, budget, true,
    ) == Some(true)
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
) -> FireAdmission {
    if session.simulation.state().tick != intent.tick.saturating_add(1) {
        return FireAdmission::Abort;
    }
    let Some(brain) = session.bots.brains.get(&bot) else {
        return FireAdmission::Abort;
    };
    if brain.resting
        || !session
            .peers
            .get(&bot)
            .is_some_and(|p| p.combat.alive && p.combat.spawn_tick == intent.shooter_spawn)
        || !session
            .peers
            .get(&intent.seen.owner)
            .is_some_and(|p| p.combat.alive && p.combat.spawn_tick == intent.target_spawn)
        || !session.bot_enemy(bot, &brain.kind, intent.seen.owner)
    {
        return FireAdmission::Abort;
    }
    let Some(actor) = session.weapons.actor(ActorId(bot)) else {
        return FireAdmission::Abort;
    };
    if actor.selected != Some(intent.choice.slot) {
        return FireAdmission::Abort;
    }
    let Some((image, current)) = session.weapons.image_state(ActorId(bot), 0) else {
        return FireAdmission::Abort;
    };
    if image.id != intent.image {
        return FireAdmission::Abort;
    }
    let projectile = image
        .projectile
        .as_ref()
        .and_then(|p| session.weapons.pack.projectiles.get(p));
    if capability(image, projectile, actor.frame.scale) != Some(intent.choice.capability) {
        return FireAdmission::Abort; // Launch metadata changed after planning.
    }
    if let Some(ammo) = session.weapons.ammo(ActorId(bot)) {
        let usable = match (ammo.supply, ammo.reserve) {
            (bri_weapons::Supply::Both, bri_weapons::Reserve::Rounds(n)) => ammo.rounds.min(n),
            _ => ammo.rounds,
        };
        if usable < intent.choice.capability.rounds_per_attack {
            return FireAdmission::Abort;
        }
    }
    // An unauthorized release stays held. A proven harmless recovery can
    // advance normally; indirect trigger-up paths need the same critical
    // trajectory pass as a direct release into Fire.
    if image.charges() && !intent.release_authorized {
        return FireAdmission::HoldCharge;
    }
    if image.charges() && !super::charged_control::release_may_fire(image, current) {
        return FireAdmission::Allow;
    }
    // Keep aim/movement and proven non-firing charge holds while immunity
    // runs out. Reject only an attack which could spend rounds on no damage.
    if !session.spawn_protected(intent.seen.owner)
        && validate_fire(
            session,
            bot,
            intent.seen,
            intent.choice,
            actual_direction,
            budget,
        )
    {
        FireAdmission::Allow
    } else if image.charges() && super::charged_control::release_only(image) && actor.trigger_held()
    {
        // Live participant/equipment remain valid. The actual direction or
        // shared validation allowance is temporarily unsuitable for release;
        // preserve its authored wind-up rather than remounting it.
        FireAdmission::HoldCharge
    } else {
        FireAdmission::Abort
    }
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
            weapon: weapon_of(cap, 0.0),
            capability: cap,
            direction: (target - origin).normalize(),
            aim: None,
            surface: false,
            dealt: 0.0,
            seconds: 0.0,
        };
        let seen = Seen {
            owner: enemy,
            eye: session.peers[&enemy].player.eye(),
            feet: session.peers[&enemy].player.state().feet.into(),
            aim: target,
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

    #[test]
    fn typed_hold_with_an_independent_native_script_attack_keeps_combat_fallback() {
        let pack = bri_weapons::Pack::from_json(include_bytes!(
            "../../../../../packages/showcase/gravity-gun-tool/assets/weapons.json"
        ))
        .unwrap();
        let mut image = pack.images.values().next().unwrap().clone();
        assert!(known_noncombat_manipulation(&image));
        image.id = "unfamiliar:image/hybrid".into();
        image.scripts.insert(
            "onsecondary".into(),
            bri_weapons::Script {
                arm: String::new(),
                fire: true,
                projectile: Some("unfamiliar:projectile/attack".into()),
                use_up: false,
            },
        );
        assert!(image.projectile.is_none());
        assert!(!known_noncombat_manipulation(&image));
        image.scripts.clear();
        image.bot = None;
        assert!(
            !known_noncombat_manipulation(&image),
            "an opaque command tool needs explicit capability semantics"
        );
    }
}
