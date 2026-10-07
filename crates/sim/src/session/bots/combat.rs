//! Inventory tactics through ordinary weapon controls and native collision queries.
//! No item names decide abilities. Unknown scripts, portals and mounted firing
//! keep their existing executor until a truthful typed provider exists.
use super::harm::{self, Bodies, Chord, Shape, Strike};
use super::tactics::{self, Aim, Capability, Context, Delivery, Family, Harm, Intercept};
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
    /// What a push from a slot does to a target, as last predicted on a
    /// planning turn (`shove`): kept between turns, as a flight is dear.
    shove: Option<(OwnerId, usize, f32, Option<super::shove::Landing>)>,
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
    /// The planning fighter's fair share of what is left this tick, for
    /// weighing other places to stand (`spots`): the allowance over the
    /// fighters it is shared by, above the reserve the rays keep for
    /// launches. What it spends is taken back with [`Self::charge`].
    pub(super) fn share(&self) -> Budget {
        let fighters = self.owners.len().max(1) as u32;
        let reserve = CHEAP_RESERVE + PATH_TICKS;
        Budget {
            tick: self.tick,
            owners: self.owners.clone(),
            cursor: self.cursor,
            turn: self.turn,
            solves: self.solves.min(SOLVES_PER_TICK / fighters),
            rays: self
                .rays
                .min(reserve + (RAYS_PER_TICK - reserve) / fighters),
        }
    }
    /// Takes what `share` spent of the `(solves, rays)` it was given.
    pub(super) fn charge(&mut self, given: (u32, u32), share: &Budget) {
        self.solves = self.solves.saturating_sub(given.0 - share.solves);
        self.rays = self.rays.saturating_sub(given.1 - share.rays);
    }
    /// What a share was given, for [`Self::charge`].
    pub(super) fn allowance(&self) -> (u32, u32) {
        (self.solves, self.rays)
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
    /// Where its push is predicted to put the target (`shove`).
    pub(super) landing: Option<super::shove::Landing>,
    /// What it would do to each side (`harm`).
    pub(super) harm: Harm,
}
#[derive(Clone)]
pub(super) struct Intent {
    pub(super) choice: Choice,
    pub(super) seen: Seen,
    pub(super) tick: u64,
    pub(super) image: String,
    /// What the planned shot sweeps (`harm::Shape`): the trigger holds it
    /// while a body steps in.
    pub(super) shape: Shape,
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
/// The share of its blast radius a splash aim's burst may land from the
/// body's middle: inside the edge, where the blast still hurts.
const SPLASH_REACH: f32 = 0.9;
/// How far above the feet a splash aim at the feet lands.
const FEET_AIM: f32 = 0.15;

pub(super) fn capability(
    image: &Image,
    projectile: Option<&bri_weapons::ProjectileDef>,
    scale: f32,
    projectiles: &std::collections::BTreeMap<String, bri_weapons::ProjectileDef>,
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
            danger: 0.0,
            arm_ticks: 0,
            cadence_ticks: cadence(image),
            rounds_per_attack: 1,
            push: (0.0, 0.0),
        });
    }
    // Its `onFire` runs something else in place of launching (a host
    // building tool other than the hammer's swing, skis, a key, a swap):
    // its projectile, if it has one, never flies.
    if image.on_fire.is_some() {
        return None;
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
        return tactics::native_capability(image, projectile, scale, cadence(image), projectiles)
            .ok();
    }
    // Session hand frames currently have muzzle == eye, so native melee's
    // geometry correction is exactly one. Reject unresolved non-damage tools.
    let mut native = image.clone();
    native.melee = false;
    let cap =
        tactics::native_capability(&native, projectile, scale, cadence(image), projectiles).ok()?;
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
        match attack_of(session, image, scale) {
            Some(Some(cap)) => {
                let ammo = session.weapons.ammo_on_equip(ActorId(bot), slot);
                if ammo.as_ref().is_none_or(|a| {
                    available_rounds(a) >= cap.rounds_per_attack
                        || matches!(a.reserve, bri_weapons::Reserve::Endless)
                        || matches!(a.reserve,bri_weapons::Reserve::Rounds(n) if n > 0)
                }) {
                    return true;
                }
            }
            Some(None) => return true,
            None => {}
        }
    }
    false
}

/// What `image` could hurt or shove someone with in a bot's hands, read
/// cautiously: `Some(Some(cap))` for an attack the bot reads natively that
/// does damage or pushes (a broom moves someone off what they are after),
/// `Some(None)` for a mechanism it reads only by its states but can fire,
/// which counts as one, and None for nothing. The one reader for both
/// "has it anything to fight with" and "could its idle click hurt".
pub(super) fn attack_of(
    session: &Session,
    image: &Image,
    scale: f32,
) -> Option<Option<Capability>> {
    let projectile = image
        .projectile
        .as_ref()
        .and_then(|id| session.weapons.pack.projectiles.get(id));
    if let Some(cap) = capability(image, projectile, scale, &session.weapons.pack.projectiles) {
        return (cap.direct_damage > 0.0 || cap.splash_damage > 0.0 || cap.pushes())
            .then_some(Some(cap));
    }
    // What its data says: a building tool that destroys or flings (the
    // wands) counts, since a fling can hurt through the fall; one that
    // inspects or prints, skis, a key, a swap, or anything that paints
    // hurts nobody.
    match &image.on_fire {
        Some(bri_weapons::OnFire::Tool(
            bri_weapons::HostTool::Destroy | bri_weapons::HostTool::AdminDestroy,
        )) => return Some(None),
        Some(_) => return None,
        None if super::super::tools::image_paints(image) => return None,
        None => {}
    }
    // Only an explicit manipulation descriptor with no native attack
    // metadata identifies a noncombat tool. No IDs or command-name
    // guesses. Anything else is an attack only if the bot can use it as
    // one (`Session::image_weapon`, the reader it fires by).
    (!known_noncombat_manipulation(image) && session.image_weapon(image, scale).is_some())
        .then_some(None)
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
    capability(image, projectile, scale, &session.weapons.pack.projectiles)
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
    capability(image, projectile, scale, &session.weapons.pack.projectiles)
        .is_some_and(Capability::pushes)
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
    capability(image, projectile, scale, &session.weapons.pack.projectiles).is_some_and(|cap| {
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
    let (damage, reach) =
        match capability(image, projectile, scale, &session.weapons.pack.projectiles) {
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
                let reach = ray.map(|r| r.range * scale).or(projectile
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
    let (speed, fall) = cap.delivery.flight().map_or((0.0, 0.0), |f| {
        (f.speed, f.fall_per_tick * bri_weapons::TICK_HZ as f32)
    });
    Weapon {
        melee: cap.family == Family::Melee,
        hold: cap.trigger.hold,
        charge: cap.trigger.charge_on_release,
        near: Some(cap.near.max(if cap.family == Family::Melee {
            0.0
        } else {
            Weapon::standoff(cap.splash_radius)
        })),
        reach: cap.reach.min(cap.delivery.flight().map_or(cap.reach, |f| {
            f.speed * PATH_TICKS.min(f.lifetime_ticks) as f32 / bri_weapons::TICK_HZ as f32
        })),
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
    let mut variants: Vec<(usize, u32, Choice, f32, Shape)> = Vec::new();
    if seen.way.carry.is_some() {
        return Decision::Unsupported;
    }
    // A gunner fights with its seat's gun, planned as any shot.
    if let Some(gun) = mounted_gun(session, bot) {
        return choose_mounted(session, bot, seen, gun, tick, state, budget);
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
    let bodies = std::cell::LazyCell::new(|| Bodies::of(session, bot, shift));
    let own_health = peer.combat.health;
    let mut shapes: Vec<Shape> = Vec::new();
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
        let Some(cap) = capability(image, projectile, scale, &session.weapons.pack.projectiles)
        else {
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
                landing: None,
                harm: Harm::default(),
            });
        }
        if distance < cap.near || distance > cap.reach {
            continue;
        }
        // A timed throw goes off where it comes to rest, so it is thrown to
        // land at the feet rather than into the body it would bounce off.
        let input = Intercept {
            muzzle: origin,
            target: if matches!(cap.delivery, Delivery::Timed { .. }) {
                seen.feet + Vec3::Y * FEET_AIM
            } else {
                target_point
            },
            target_velocity,
            shooter_velocity: velocity,
        };
        let mut solutions = [None, None];
        let curved = cap.delivery.flight().is_some_and(|f| f.fall_per_tick > 0.0);
        if let Some(f) = cap.delivery.flight() {
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
            landing: None,
            harm: Harm::default(),
        };
        if curved && !turn {
            // A wind-up is kept between planning turns only while what it
            // would do stays a shot its side takes.
            let mut chords = Vec::new();
            if Some(slot) == selected
                && image.charges()
                && clear_path(
                    session,
                    bot,
                    seen.owner,
                    choice,
                    origin,
                    budget,
                    false,
                    &mut chords,
                ) == Some(true)
            {
                let (harm, shape) = assess(
                    session,
                    bot,
                    hand_strike(session, bot, image, cap, None),
                    &chords,
                    origin,
                    &bodies,
                );
                if tactics::harm_allows(harm, own_health) {
                    let choice = Choice { harm, ..choice };
                    state.intent = Some(Intent {
                        choice,
                        seen,
                        tick,
                        image: image.id.clone(),
                        shape,
                        release_authorized: false,
                        shooter_spawn: peer.combat.spawn_tick,
                        target_spawn: target.combat.spawn_tick,
                    });
                    return Decision::Charging(choice);
                }
            }
            pending = true;
            continue;
        }
        for aim in solutions.into_iter().take(if curved { 2 } else { 1 }) {
            if cap.delivery.flight().is_some() && aim.is_none() {
                continue;
            }
            // A timed throw is judged where it goes off, not where it first
            // lands.
            let timed = matches!(cap.delivery, Delivery::Timed { .. });
            // Its way: a timed throw's from its burst, bounces and all;
            // anything else's from the chords that check it reaches its
            // target.
            let mut chords = Vec::new();
            let aim = match aim.filter(|_| timed) {
                Some(thrown) => match timed_aim(
                    session, bot, seen.owner, slot, cap, origin, thrown, budget, false,
                ) {
                    None => {
                        pending = true;
                        break;
                    }
                    Some(None) => continue,
                    Some(Some((burst, path))) => {
                        chords = path;
                        Some(burst)
                    }
                },
                None => aim,
            };
            let choice = Choice {
                aim,
                direction: aim.map_or(direction, |a| a.direction),
                ..choice
            };
            if !timed {
                match clear_path(
                    session,
                    bot,
                    seen.owner,
                    choice,
                    origin,
                    budget,
                    false,
                    &mut chords,
                ) {
                    None => {
                        pending = true;
                        break;
                    }
                    Some(false) => continue,
                    Some(true) => {}
                }
            }
            // A swing strikes its target: the check above found it so.
            let contact = (cap.delivery == Delivery::Contact).then_some(Some(seen.owner));
            let strike = hand_strike(session, bot, image, cap, contact);
            let (harm, shape) = assess(session, bot, strike, &chords, origin, &bodies);
            // What its push would do to them (`shove`): flown on the
            // planning turn, on a share of the solver budget no longer
            // than a shot's path, and kept until the next.
            let (push_harm, landing) = if !cap.pushes() {
                (0.0, None)
            } else if turn {
                let mut allowance = budget.solves.min(PATH_TICKS);
                let spent = allowance;
                let (harm, landing) =
                    session.bot_shove(bot, seen.owner, choice.direction, cap.push, &mut allowance);
                budget.solves -= spent - allowance;
                state.shove = Some((seen.owner, slot, harm, landing));
                (harm, landing)
            } else {
                state
                    .shove
                    .filter(|(owner, s, _, _)| *owner == seen.owner && *s == slot)
                    .map_or((0.0, None), |(_, _, harm, landing)| (harm, landing))
            };
            let context = Context {
                distance,
                target_health: target.combat.health.max(1.0),
                hit_probability: 1.0,
                harm,
                own_health,
                aim,
                ready_rounds,
                opportunity_cost: 0.0,
                push_harm,
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
                landing,
                harm,
                ..choice
            });
            shapes.push(shape);
            // A splash weapon may aim at the feet, or at a surface beside
            // the target, where its real blast still hurts.
            if cap.splash_radius > 0.0 && cap.splash_damage > 0.0 {
                let feet = seen.feet + Vec3::Y * FEET_AIM;
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
                    if let Some((c, shape, score)) = variant(
                        session, bot, seen.owner, image, cap, solve, context, &bodies, budget,
                    ) {
                        variants.push((slot, aim, c, score * SPLASH_AIM, shape));
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
            let at = choices
                .iter()
                .position(|c| c.slot == slot)
                .expect("candidate choice");
            let (mut choice, mut shape) = (choices[at], shapes.swap_remove(at));
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
                (choice, shape) = (v.2, v.4.clone());
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
                shape,
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
                shape: Shape::default(),
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

/// What a timed throw does ([`Delivery::Timed`]): where it goes off and the
/// tick, if it does.
#[derive(Clone, Debug, PartialEq)]
struct Burst {
    position: Vec3,
    ticks: u32,
    /// Its way there, chord by chord (`harm`).
    path: Vec<Chord>,
}

/// How many ticks of a timed throw's arc one ray covers: the most whose
/// sag off the straight chord stays under the thinnest brick, a plate, so
/// no brick can slip between ray and arc. Over `k` ticks the arc falls
/// `fall_per_tick` (a speed lost each tick) for `k / HZ` seconds, so it
/// sags `fall_per_tick * k^2 / (8 * HZ)`.
fn chord_ticks(fall_per_tick: f32) -> u32 {
    let plate = bri_content::brick::PLATE;
    if fall_per_tick <= 0.0 {
        return PATH_TICKS;
    }
    let hz = bri_weapons::TICK_HZ as f32;
    ((8.0 * plate * hz / fall_per_tick).sqrt() as u32).clamp(1, PATH_TICKS)
}

/// The cooked fuse a timed throw from `image` carries if released now:
/// what is left since it was lit, or for an unlit one the whole fuse less
/// the ticks its states take from lighting to firing.
fn fuse_left(session: &Session, bot: OwnerId, image: &Image, fuse: u32) -> Option<u32> {
    let actor = session.weapons.actor(ActorId(bot))?;
    if let Some(lit) = actor.fuse_lit(&image.id) {
        let burned = session.simulation.state().tick.saturating_sub(lit);
        return Some(fuse.saturating_sub(burned.min(u64::from(u32::MAX)) as u32));
    }
    let cook = image.cook.as_ref()?;
    let mut at = image
        .states
        .iter()
        .position(|s| s.script.eq_ignore_ascii_case(&cook.script))?;
    let mut ticks = 0_u32;
    for _ in 0..image.states.len() {
        let state = &image.states[at];
        if state.script.eq_ignore_ascii_case("onfire") {
            return Some(fuse.saturating_sub(ticks));
        }
        ticks = ticks.saturating_add(state.ticks);
        at = state.timeout.or(state.up).or(state.down)?;
    }
    None
}

/// The projectile a slot's image throws.
fn thrown(
    session: &Session,
    bot: OwnerId,
    slot: usize,
) -> Option<(&Image, &bri_weapons::ProjectileDef)> {
    let actor = session.weapons.actor(ActorId(bot))?;
    let item = actor.inventory.get(slot)?.as_ref()?;
    let image = session
        .weapons
        .pack
        .images
        .get(&session.weapons.pack.items.get(item)?.image)?;
    let projectile = session
        .weapons
        .pack
        .projectiles
        .get(image.projectile.as_ref()?)?;
    Some((image, projectile))
}

/// Follow a timed throw from `origin` at `launch` until it goes off, with
/// the host's own rules: the clock first (`expires`), then at each hit
/// whether it bursts (`hit_bursts`), sticks or bounces (`rebound`). A
/// stuck throw, or one whose bounce can no longer lift it off the ground
/// for a tick, waits where it is for its clock. None when the shared ray
/// budget runs out (try again next turn); Some(None) when it never goes
/// off where a bot can follow (it fades, or flies through a portal).
#[allow(clippy::too_many_arguments)]
fn burst(
    session: &Session,
    bot: OwnerId,
    def: &bri_weapons::ProjectileDef,
    fall_per_tick: f32,
    origin: Vec3,
    launch: Vec3,
    fuse: Option<u32>,
    budget: &mut Budget,
    critical: bool,
) -> Option<Option<Burst>> {
    use bri_weapons::runtime::{Rebound, expires, hit_bursts, rebound};
    // A body at rest goes off when its clock says.
    let waits = |at: Vec3, age: u32, path: Vec<Chord>| -> Option<Option<Burst>> {
        let fuse = fuse.filter(|f| *f >= age && *f < def.lifetime_ticks);
        Some(match fuse {
            Some(ticks) => Some(Burst {
                position: at,
                ticks,
                path,
            }),
            None if def.explode_death => Some(Burst {
                position: at,
                ticks: def.lifetime_ticks.max(age),
                path,
            }),
            None => None,
        })
    };
    let hz = bri_weapons::TICK_HZ as f32;
    let mut path: Vec<Chord> = Vec::new();
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
    let chord = chord_ticks(fall_per_tick);
    let at = |start: Vec3, velocity: Vec3, ticks: u32| -> Option<Vec3> {
        tactics::flight_position(
            start,
            velocity,
            fall_per_tick,
            f64::from(ticks) / f64::from(bri_weapons::TICK_HZ),
        )
        .ok()
        .map(|p| p.as_vec3())
    };
    // The arc being followed: where it left from, at what velocity, and the
    // ticks flown on it so far; and the throw's age and bounces.
    let (mut start, mut velocity, mut flown) = (origin, launch, 0_u32);
    let (mut age, mut bounces) = (0_u32, 0_u32);
    while age < tactics::MAX_LIFETIME_TICKS {
        // Up to one chord of ticks, stopping where the clock runs out.
        let mut ticks = 0;
        let mut expiry = None;
        while ticks < chord {
            if let Some(bursts) = expires(def, age + ticks + 1, fuse) {
                expiry = Some(bursts);
                break;
            }
            ticks += 1;
        }
        if ticks > 0 {
            let from = at(start, velocity, flown)?;
            let to = at(start, velocity, flown + ticks)?;
            if !budget.ray(true, critical) {
                return None;
            }
            if q.passage(from, to).is_some() {
                return Some(None);
            }
            let filter = |age: u32| Filter {
                projectile_age_ticks: Some(age),
                source: ActorId(bot),
                players: def.collide_players,
                world_only: false,
            };
            // The chord touched something: find the tick it hits, one ray a
            // tick, as the host flies it. The arc may pass what the chord
            // touched.
            let mut struck = None;
            if q.sweep(from, to, filter(age + 1)).is_some() {
                for into in 1..=ticks {
                    let a = at(start, velocity, flown + into - 1)?;
                    let b = at(start, velocity, flown + into)?;
                    if !budget.ray(true, critical) {
                        return None;
                    }
                    if let Some(hit) = q.sweep(a, b, filter(age + into)) {
                        struck = Some((into, hit));
                        break;
                    }
                }
            }
            if let Some((into, hit)) = struck {
                let hit_age = age + into;
                path.push(Chord {
                    from,
                    to: hit.position,
                    seconds: hit_age as f32 / hz,
                });
                let normal = hit.normal.normalize_or_zero();
                if normal == Vec3::ZERO {
                    return Some(None);
                }
                if hit_bursts(def, hit_age, matches!(hit.target, TargetId::Actor(_))) {
                    return Some(Some(Burst {
                        position: hit.position,
                        ticks: hit_age,
                        path,
                    }));
                }
                let moving = velocity + Vec3::NEG_Y * fall_per_tick * (flown + into) as f32;
                match rebound(def, moving, normal, bounces) {
                    Rebound::Stuck => return waits(hit.position, hit_age, path),
                    Rebound::Bounced { bursts: true, .. } => {
                        return Some(Some(Burst {
                            position: hit.position,
                            ticks: hit_age,
                            path,
                        }));
                    }
                    Rebound::Bounced { velocity: off, .. } => {
                        bounces += 1;
                        // Its bounce cannot lift it for even a tick: it
                        // rolls to rest where it is.
                        if normal.y > 0.0 && off.y <= fall_per_tick {
                            return waits(hit.position, hit_age, path);
                        }
                        // It flies the rest of that tick off the surface, as
                        // the host's step does.
                        let rest_of_tick = off * (1.0 - hit.fraction) / bri_weapons::TICK_HZ as f32;
                        (start, velocity, flown, age) = (
                            hit.position + normal * 0.001 + rest_of_tick,
                            off,
                            0,
                            hit_age,
                        );
                        continue;
                    }
                }
            }
            path.push(Chord {
                from,
                to,
                seconds: (age + ticks) as f32 / hz,
            });
        }
        flown += ticks;
        age += ticks;
        match expiry {
            Some(true) => {
                return Some(Some(Burst {
                    position: at(start, velocity, flown)?,
                    ticks: age + 1,
                    path,
                }));
            }
            Some(false) => return Some(None),
            None => {}
        }
    }
    Some(None)
}

/// A timed throw at `aim` from `origin`: the aim moved to where it goes
/// off, if that is within its blast of `enemy` where the enemy will be by
/// then, and its way there. None when the ray budget ran out.
#[allow(clippy::too_many_arguments)]
fn timed_aim(
    session: &Session,
    bot: OwnerId,
    enemy: OwnerId,
    slot: usize,
    cap: Capability,
    origin: Vec3,
    aim: Aim,
    budget: &mut Budget,
    critical: bool,
) -> Option<Option<(Aim, Vec<Chord>)>> {
    let Delivery::Timed { flight, fuse } = cap.delivery else {
        return Some(None);
    };
    let Some((image, def)) = thrown(session, bot, slot) else {
        return Some(None);
    };
    let fuse = match fuse {
        Some(f) => match fuse_left(session, bot, image, f) {
            Some(left) => Some(left),
            None => return Some(None),
        },
        None => None,
    };
    let Some(burst) = burst(
        session,
        bot,
        def,
        flight.fall_per_tick,
        origin,
        aim.launch_velocity,
        fuse,
        budget,
        critical,
    )?
    else {
        return Some(None);
    };
    let seconds = f64::from(burst.ticks) / f64::from(bri_weapons::TICK_HZ);
    let Some(body) = session.peers.get(&enemy) else {
        return Some(None);
    };
    let centre = Vec3::from(body.player.state().feet)
        + Vec3::Y * body.player.tuning().stand_height * 0.5
        + Vec3::from(body.player.state().velocity) * seconds as f32;
    if burst.position.distance(centre) > cap.splash_radius * SPLASH_REACH {
        return Some(None);
    }
    Some(Some((
        Aim {
            impact: burst.position,
            time_seconds: seconds,
            flight_tick: burst.ticks,
            ..aim
        },
        burst.path,
    )))
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
    image: &Image,
    cap: Capability,
    solve: Solve,
    context: Context,
    bodies: &Bodies,
    budget: &mut Budget,
) -> Option<(Choice, Shape, f32)> {
    let f = cap.delivery.flight()?;
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
    let timed = matches!(cap.delivery, Delivery::Timed { .. });
    let mut chords = Vec::new();
    let aim = if timed {
        let (aim, path) = timed_aim(
            session,
            bot,
            enemy,
            solve.slot,
            cap,
            solve.origin,
            aim,
            budget,
            false,
        )??;
        chords = path;
        aim
    } else {
        aim
    };
    let body = session.peers.get(&enemy)?;
    let centre = Vec3::from(body.player.state().feet)
        + Vec3::Y * body.player.tuning().stand_height * 0.5
        + Vec3::from(body.player.state().velocity) * aim.time_seconds as f32;
    if !timed && aim.impact.distance(centre) > cap.splash_radius * SPLASH_REACH {
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
        landing: None,
        harm: Harm::default(),
    };
    if !timed
        && clear_path(
            session,
            bot,
            enemy,
            choice,
            solve.origin,
            budget,
            false,
            &mut chords,
        ) != Some(true)
    {
        return None;
    }
    // Scored by its splash alone: it is aimed beside the body.
    let splash = Capability {
        direct_damage: 0.0,
        ..cap
    };
    let strike = hand_strike(session, bot, image, splash, None);
    let (harm, shape) = assess(session, bot, strike, &chords, solve.origin, bodies);
    let context = Context {
        aim: Some(aim),
        harm,
        ..context
    };
    let score = tactics::suitability(splash, context).ok()?;
    let (dealt, seconds) = tactics::worth(splash, context).ok()?;
    Some((
        Choice {
            dealt,
            seconds,
            harm,
            ..choice
        },
        shape,
        score,
    ))
}

/// The gun of the seat a bot rides in, as an attack: its capability, the
/// projectile it fires, where it fires from, the vehicle's motion and size.
struct MountedGun {
    cap: Capability,
    projectile: String,
    muzzle: Vec3,
    velocity: Vec3,
    scale: f32,
}

/// The gun `bot`'s seat fires, when it sits in a vehicle's gun seat whose
/// gun is ready to be used (`Session::bot_vehicle_weapon` reads the same
/// gun for its movement).
fn mounted_gun(session: &Session, bot: OwnerId) -> Option<MountedGun> {
    let (vehicle, seat) = session.mounted(bot)?;
    let world = session.vehicles.world.as_ref()?;
    let id = bri_vehicles::VehicleId(vehicle);
    let d = world.definition_of(id)?;
    if !world.weapon_available(id) || d.weapon_seat() != Some(usize::from(seat)) {
        return None;
    }
    let gun = d.weapon.as_ref()?;
    let p = session.weapons.pack.projectiles.get(&gun.projectile)?;
    let v = session.bots.objects.iter().find(|v| v.id.0 == vehicle)?;
    let speed = gun.speed * f32::from(gun.charge_steps.max(1)) * v.scale;
    let cap = tactics::vehicle_capability(
        p,
        speed,
        v.scale,
        u32::try_from(gun.cooldown_ticks.max(1)).unwrap_or(u32::MAX),
        gun.charge_ticks > 0,
        &session.weapons.pack.projectiles,
    )
    .ok()?;
    Some(MountedGun {
        cap,
        projectile: gun.projectile.clone(),
        muzzle: session.bot_weapon_origin(bot)?,
        velocity: Vec3::from(v.velocity),
        scale: v.scale,
    })
}

/// A gunner's shot at `seen` with its seat's `gun`: solved, checked and
/// priced (`harm::shot_harm`, its crew spared) as a hand weapon's is, and
/// ready only when its side takes the trade.
fn choose_mounted(
    session: &Session,
    bot: OwnerId,
    seen: Seen,
    gun: MountedGun,
    tick: u64,
    state: &mut State,
    budget: &mut Budget,
) -> Decision {
    let Some(peer) = session.peers.get(&bot) else {
        return Decision::Unsafe;
    };
    let Some(target) = session.peers.get(&seen.owner).filter(|p| p.combat.alive) else {
        return Decision::Unsafe;
    };
    let cap = gun.cap;
    let origin = gun.muzzle;
    state.movement = Some(weapon_of(cap, 0.0));
    let distance = origin.distance(seen.aim);
    if distance < cap.near || distance > cap.reach {
        return Decision::Pending;
    }
    let Some(flight) = cap.delivery.flight() else {
        return Decision::Unsupported;
    };
    let input = Intercept {
        muzzle: origin,
        target: seen.aim,
        target_velocity: Vec3::from(target.player.state().velocity) * super::lead(bot, tick),
        shooter_velocity: gun.velocity,
    };
    let Ok(mut search) = tactics::InterceptSearch::new(flight, input) else {
        return Decision::Unsupported;
    };
    let limit = PATH_TICKS.min(flight.lifetime_ticks);
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
    let Some(aim) = search.result().low else {
        return Decision::Pending;
    };
    let choice = Choice {
        slot: usize::MAX,
        weapon: weapon_of(cap, 0.0),
        capability: cap,
        direction: aim.direction,
        aim: Some(aim),
        surface: false,
        dealt: 0.0,
        seconds: 0.0,
        landing: None,
        harm: Harm::default(),
    };
    let mut chords = Vec::new();
    match clear_path(
        session,
        bot,
        seen.owner,
        choice,
        origin,
        budget,
        false,
        &mut chords,
    ) {
        None => return Decision::Pending,
        Some(false) => return Decision::Unsafe,
        Some(true) => {}
    }
    let bodies = Bodies::of(session, bot, Vec3::ZERO);
    let def = session.weapons.pack.projectiles.get(&gun.projectile);
    let strike = Strike::of(cap, def, 1, 0.0, gun.scale);
    let (harm, shape) = assess(session, bot, strike, &chords, origin, &bodies);
    let context = Context {
        distance,
        target_health: target.combat.health.max(1.0),
        hit_probability: 1.0,
        harm,
        own_health: peer.combat.health,
        aim: Some(aim),
        ready_rounds: None,
        opportunity_cost: 0.0,
        push_harm: 0.0,
        switch_seconds: 0.0,
    };
    let Ok((dealt, seconds)) = tactics::worth(cap, context) else {
        return Decision::Unsafe;
    };
    let choice = Choice {
        dealt,
        seconds,
        harm,
        ..choice
    };
    state.intent = Some(Intent {
        choice,
        seen,
        tick,
        image: gun.projectile,
        shape,
        release_authorized: true,
        shooter_spawn: peer.combat.spawn_tick,
        target_spawn: target.combat.spawn_tick,
    });
    Decision::Ready(choice)
}

/// What a hand weapon's attack `cap` strikes with: the projectile its
/// `image` launches, that many pellets at its spread, at `bot`'s scale.
/// `contact`: a swing's struck body.
fn hand_strike(
    session: &Session,
    bot: OwnerId,
    image: &Image,
    cap: Capability,
    contact: Option<Option<OwnerId>>,
) -> Strike {
    let def = image
        .projectile
        .as_ref()
        .and_then(|p| session.weapons.pack.projectiles.get(p));
    let pellets = image.shot.as_ref().map_or(1, |s| s.projectiles);
    let scale = session
        .peers
        .get(&bot)
        .map_or(1.0, |p| p.player.state().scale);
    Strike {
        contact,
        ..Strike::of(cap, def, pellets, super::fire::image_spread(image), scale)
    }
}

/// What `strike` along `chords` from `origin` does to each side and what
/// it sweeps (`harm`), its way turned by the bot's aim error too, so the
/// shot it will really fire.
fn assess(
    session: &Session,
    bot: OwnerId,
    strike: Strike,
    chords: &[Chord],
    origin: Vec3,
    bodies: &Bodies,
) -> (Harm, Shape) {
    let error = session
        .bots
        .brains
        .get(&bot)
        .map_or((0.0, 0.0), |b| b.error);
    // Its enemies take what the aimed shot does (the hit chance prices its
    // miss); its side the worse of the aimed shot and the one its aim error
    // sends off.
    let (mut harm, mut shape) = harm::shot_harm(bodies, origin, chords, &strike);
    if error != (0.0, 0.0) {
        let (off, way) = harm::shot_harm(
            bodies,
            origin,
            &harm::turned(origin, chords, error),
            &strike,
        );
        harm.ally = harm.ally.max(off.ally);
        harm.own = harm.own.max(off.own);
        harm.kills_ally |= off.kills_ally;
        for owner in way.priced {
            if !shape.priced.contains(&owner) {
                shape.priced.push(owner);
            }
        }
    }
    (harm, shape)
}

/// Exact free-flight segments match the native semi-implicit projectile step.
/// Intended-target collisions are followed by a world probe so their present
/// body cannot hide an obstacle on that chord. The chords it checks are
/// pushed to `chords` for the harm check (`harm::shot_harm`).
#[allow(clippy::too_many_arguments)]
fn clear_path(
    session: &Session,
    bot: OwnerId,
    enemy: OwnerId,
    choice: Choice,
    origin: Vec3,
    budget: &mut Budget,
    critical: bool,
    chords: &mut Vec<Chord>,
) -> Option<bool> {
    if choice.capability.delivery == Delivery::Contact {
        if !budget.ray(false, critical) {
            return None;
        }
        chords.push(Chord {
            from: origin,
            to: origin + choice.direction * choice.capability.reach,
            seconds: 0.0,
        });
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
    let curved = choice
        .capability
        .delivery
        .flight()
        .is_some_and(|f| f.fall_per_tick > 0.0);
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
        let end = match (choice.capability.delivery.flight(), choice.aim) {
            (Some(f), Some(a)) => {
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
        chords.push(Chord {
            from: start,
            to: end,
            seconds: time as f32,
        });
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
    let timed = matches!(choice.capability.delivery, Delivery::Timed { .. });
    let mut chords = Vec::new();
    if let (Some(f), Some(mut aim)) = (choice.capability.delivery.flight(), choice.aim) {
        aim.direction = choice.direction;
        aim.launch_velocity =
            choice.direction * f.speed + Vec3::from(peer.player.state().velocity) * f.inherit;
        if !aim.launch_velocity.is_finite() || aim.launch_velocity.length() > 10_000.0 {
            return false;
        }
        if timed {
            // Where the real throw goes off, from the real direction.
            match timed_aim(
                session,
                bot,
                seen.owner,
                choice.slot,
                choice.capability,
                origin,
                aim,
                budget,
                true,
            ) {
                Some(Some((burst, path))) => {
                    aim = burst;
                    chords = path;
                }
                _ => return false,
            }
        } else {
            let Ok(impact) = tactics::flight_position(
                origin,
                aim.launch_velocity,
                f.fall_per_tick,
                aim.time_seconds,
            ) else {
                return false;
            };
            aim.impact = impact.as_vec3();
        }
        choice.aim = Some(aim);
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
        if choice.surface || timed {
            // A surface shot lands beside the body, a timed throw goes off
            // near it: its blast must reach it.
            if aim.impact.distance((min + max) * 0.5) > choice.capability.splash_radius {
                return false;
            }
        } else if !aim.impact.cmpge(min).all() || !aim.impact.cmple(max).all() {
            return false;
        }
    }
    if !timed
        && clear_path(
            session,
            bot,
            seen.owner,
            choice,
            origin,
            budget,
            true,
            &mut chords,
        ) != Some(true)
    {
        return false;
    }
    // What the shot really does, aim error and all, judged as it was
    // planned: never a teammate's death or its own, and no more harm to its
    // side than the harm to its enemies it was taken for.
    let Some(image) = session
        .weapons
        .actor(ActorId(bot))
        .and_then(|a| a.inventory.get(choice.slot).cloned().flatten())
        .and_then(|i| session.weapons.pack.items.get(&i))
        .and_then(|i| session.weapons.pack.images.get(&i.image))
    else {
        return false;
    };
    let contact = (choice.capability.delivery == Delivery::Contact).then_some(Some(seen.owner));
    let bodies = Bodies::of(session, bot, Vec3::ZERO);
    let strike = hand_strike(session, bot, image, choice.capability, contact);
    let (harm, _) = assess(session, bot, strike, &chords, origin, &bodies);
    tactics::harm_allows(harm, peer.combat.health) && harm.ally + harm.own < choice.harm.enemy
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
    if capability(
        image,
        projectile,
        actor.frame.scale,
        &session.weapons.pack.projectiles,
    ) != Some(intent.choice.capability)
    {
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
    /// The stock tools read by their data: the hammer's swing and the
    /// wands' destroy and fling count as attacks; the wrench, the printer
    /// and the spray can hurt nobody, whatever projectile they carry.
    #[test]
    fn stock_tools_read_as_their_data_says() {
        use bri_weapons::testing as t;
        let world = bri_world::World::new("Tools".into(), "fixture".into(), vec![[1.0; 4]]);
        let sim = crate::simulation::Simulation::new(world, crate::testing::definitions(), vec![])
            .unwrap();
        let mut session = Session::new(sim);
        let mut pack = t::pack();
        // The stock wrench carries a damaging projectile it never launches.
        pack.images.get_mut(t::WRENCH_IMAGE).unwrap().projectile = Some(t::GUN_PROJECTILE.into());
        session.set_weapon_pack(pack).unwrap();
        let read = |image: &str| attack_of(&session, &session.weapons.pack.images[image], 1.0);
        assert!(matches!(read(t::HAMMER_IMAGE), Some(Some(_))));
        assert_eq!(read(t::WAND_IMAGE), Some(None));
        assert_eq!(read(t::ADMIN_WAND_IMAGE), Some(None));
        for harmless in [t::WRENCH_IMAGE, t::PRINTER_IMAGE, t::SPRAY_CAN_IMAGE] {
            assert_eq!(read(harmless), None, "{harmless}");
        }
        let wrench = &session.weapons.pack.images[t::WRENCH_IMAGE];
        let projectile = &session.weapons.pack.projectiles[t::GUN_PROJECTILE];
        assert!(
            capability(
                wrench,
                Some(projectile),
                1.0,
                &session.weapons.pack.projectiles
            )
            .is_none(),
            "the wrench is no weapon"
        );
    }
    /// What a bot could hurt someone with is read cautiously: a native
    /// attack by its capability, and a weapon it reads only by its states
    /// (ported scripts, as an Add-On knife) as one too, so neither the fight
    /// code nor an idle goof takes it for harmless.
    #[test]
    fn a_scripted_weapon_counts_as_one_that_can_hurt() {
        let world = bri_world::World::new("Weapons".into(), "fixture".into(), vec![[1.0; 4]]);
        let sim = crate::simulation::Simulation::new(world, crate::testing::definitions(), vec![])
            .unwrap();
        let mut session = Session::new(sim);
        let mut pack = bri_weapons::testing::pack();
        let mut knife = pack.images[bri_weapons::testing::SWORD_IMAGE].clone();
        knife.id = "stranger:image/knife".into();
        knife.scripts = serde_json::from_value(serde_json::json!({
            "onfire": {"arm": "spearthrow", "fire": true}
        }))
        .unwrap();
        pack.images.insert(knife.id.clone(), knife.clone());
        session.set_weapon_pack(pack).unwrap();
        let gun = &session.weapons.pack.images[bri_weapons::testing::GUN_IMAGE];
        assert!(matches!(attack_of(&session, gun, 1.0), Some(Some(_))));
        let knife = &session.weapons.pack.images[&knife.id];
        assert!(capability(knife, None, 1.0, &session.weapons.pack.projectiles).is_none());
        assert_eq!(attack_of(&session, knife, 1.0), Some(None));
    }
    /// A timed throw spends one ray per chord, and a chord sags no more than
    /// a plate off the real arc: pinned for a known arc in open air.
    #[test]
    fn a_timed_throw_spends_one_ray_a_chord() {
        use rapier3d::prelude::*;
        let def = bri_weapons::ProjectileDef {
            id: "t:p/high".into(),
            speed: 10.0,
            gravity: 1.0,
            ballistic: true,
            lifetime_ticks: 360,
            arm_ticks: 360,
            explode_death: true,
            ..Default::default()
        };
        let fall = bri_weapons::runtime::fall_per_tick(&def);
        let chord = chord_ticks(fall);
        let sag = |k: u32| {
            let at = |t: u32| {
                tactics::flight_position(Vec3::ZERO, Vec3::X * 10.0, fall, f64::from(t) / 120.0)
                    .unwrap()
                    .as_vec3()
            };
            let middle = at(k / 2);
            let chord_middle = at(0).lerp(at(k), (k / 2) as f32 / k as f32);
            (middle - chord_middle).length()
        };
        assert!(sag(chord) <= bri_content::brick::PLATE, "chord {chord}");
        assert!(
            sag(chord + 2) > bri_content::brick::PLATE,
            "chord {chord} is the longest"
        );
        let mut pack = bri_weapons::testing::pack();
        pack.projectiles.insert(def.id.clone(), def.clone());
        let world = bri_world::World::new("Sky".into(), "fixture".into(), vec![[1.0; 4]]);
        let sim = crate::simulation::Simulation::new(
            world,
            crate::testing::definitions(),
            vec![ColliderBuilder::cuboid(1.0, 0.5, 1.0).translation(Vector::new(0.0, -0.5, 0.0))],
        )
        .unwrap();
        let mut session = Session::new(sim);
        session.set_weapon_pack(pack).unwrap();
        let thrower = session
            .join("Thrower".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        let mut budget = Budget::default();
        budget.begin_tick(0);
        let before = budget.rays;
        let planned = burst(
            &session,
            thrower,
            &def,
            fall,
            Vec3::new(100.0, 5000.0, 0.0),
            Vec3::X * 10.0,
            None,
            &mut budget,
            true,
        )
        .unwrap()
        .unwrap();
        assert_eq!(planned.ticks, def.lifetime_ticks);
        // It flies 359 ticks before its life ends on the 360th.
        assert_eq!(
            before - budget.rays,
            (def.lifetime_ticks - 1).div_ceil(chord)
        );
    }

    /// A timed throw's planned burst ([`burst`]) against the host flying the
    /// same projectile over the same floor, for each way it goes off: an
    /// armed hit, its last allowed bounce, its cooked fuse, the end of its
    /// life, stuck where it struck, and rolled to rest. The tick agrees
    /// exactly; the place to within a hair, or for a body the plan rests at
    /// its last bounce, to within how far it can still roll.
    #[test]
    fn a_planned_burst_matches_the_host_flying_it() {
        use rapier3d::prelude::*;
        let grenade = |id: &str| bri_weapons::ProjectileDef {
            id: id.into(),
            name: "An unfamiliar canister".into(),
            speed: 12.0,
            gravity: 1.0,
            ballistic: true,
            elasticity: 0.5,
            friction: 0.2,
            lifetime_ticks: 600,
            arm_ticks: 600,
            explode_death: true,
            explosion: bri_weapons::Explosion {
                damage: 50.0,
                radius: 3.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let cases = [
            (
                "armed",
                bri_weapons::ProjectileDef {
                    arm_ticks: 20,
                    ..grenade("t:p/armed")
                },
                None,
            ),
            (
                "last bounce",
                bri_weapons::ProjectileDef {
                    max_bounces: 2,
                    ..grenade("t:p/bounce")
                },
                None,
            ),
            ("fuse", grenade("t:p/fuse"), Some(70)),
            (
                "end of life",
                bri_weapons::ProjectileDef {
                    lifetime_ticks: 150,
                    ..grenade("t:p/life")
                },
                None,
            ),
            (
                "stuck",
                bri_weapons::ProjectileDef {
                    min_stick_speed: 1.0,
                    bounce_angle: 180.0,
                    lifetime_ticks: 200,
                    ..grenade("t:p/stick")
                },
                None,
            ),
            (
                "rolled",
                bri_weapons::ProjectileDef {
                    elasticity: 0.2,
                    friction: 0.6,
                    lifetime_ticks: 400,
                    ..grenade("t:p/roll")
                },
                None,
            ),
        ];
        for (case, def, fuse) in cases {
            let mut pack = bri_weapons::testing::pack();
            pack.projectiles.insert(def.id.clone(), def.clone());
            let world = bri_world::World::new("Throw".into(), "fixture".into(), vec![[1.0; 4]]);
            let sim = crate::simulation::Simulation::new(
                world,
                crate::testing::definitions(),
                vec![
                    ColliderBuilder::cuboid(40.0, 0.5, 40.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                ],
            )
            .unwrap();
            let mut session = Session::new(sim);
            session.set_weapon_pack(pack).unwrap();
            let thrower = session
                .join("Thrower".into(), Vec3::new(-20.0, 0.05, 0.0), false)
                .unwrap();
            let origin = Vec3::new(0.0, 2.0, 0.0);
            let launch = Vec3::new(8.0, 6.0, 1.0);
            let fall = bri_weapons::runtime::fall_per_tick(&def);
            let mut budget = Budget::default();
            budget.begin_tick(0);
            let planned = burst(
                &session,
                thrower,
                &def,
                fall,
                origin,
                launch,
                fuse,
                &mut budget,
                true,
            )
            .expect("the critical budget covers one throw")
            .unwrap_or_else(|| panic!("{case}: planned no burst"));
            let id = session
                .weapons
                .spawn(&def.id, ActorId(thrower), origin, launch, 1.0)
                .unwrap();
            if let Some(fuse) = fuse {
                session.weapons.light_fuse(id, fuse);
            }
            let shapes = session.tutorial_shape_targets();
            let mut flown = None;
            for tick in 1..=def.lifetime_ticks + 1 {
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
                let blast = session
                    .weapons
                    .step(&mut q)
                    .into_iter()
                    .find_map(|e| match e {
                        bri_weapons::Event::Blast { position, .. } => Some(position),
                        _ => None,
                    });
                if let Some(position) = blast {
                    flown = Some((tick, position));
                    break;
                }
            }
            let (ticks, position) =
                flown.unwrap_or_else(|| panic!("{case}: the host never burst it"));
            assert_eq!(planned.ticks, ticks, "{case}: tick");
            let roll = if case == "rolled" {
                // It rests at its last bounce in the plan; the host rolls it
                // on, slowed by friction, until its clock runs out.
                launch.length() * def.elasticity
            } else {
                0.01
            };
            assert!(
                planned.position.distance(position) <= roll,
                "{case}: planned {:?}, host {:?}",
                planned.position,
                position
            );
        }
    }
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
                1.0,
                &pack.projectiles
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
        super::super::harm::one_game(&mut session, shooter, enemy);
        let slot = session
            .give_item(shooter, bri_weapons::testing::GUN_ITEM)
            .unwrap();
        let target = session.peers[&enemy].player.eye() - Vec3::Y * 0.5;
        let origin = session.peers[&shooter].player.eye();
        let image = &session.weapons.pack.images[bri_weapons::testing::GUN_IMAGE];
        let projectile = &session.weapons.pack.projectiles[bri_weapons::testing::GUN_PROJECTILE];
        let mut cap = capability(
            image,
            Some(projectile),
            1.0,
            &session.weapons.pack.projectiles,
        )
        .unwrap();
        cap.delivery = Delivery::Ray;
        let choice = Choice {
            slot,
            weapon: weapon_of(cap, 0.0),
            capability: cap,
            direction: (target - origin).normalize(),
            aim: None,
            surface: false,
            dealt: 0.0,
            seconds: 0.0,
            landing: None,
            harm: Harm {
                enemy: 10.0,
                ..Default::default()
            },
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
