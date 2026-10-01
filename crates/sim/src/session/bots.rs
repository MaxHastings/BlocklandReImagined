//! Server-side bots placed by vehicle spawn bricks.
//!
//! v20 makes an `AIPlayer` for a player-type choice on a Vehicle Spawn brick
//! and gives it no brain. Bots that walk, find their way and fight are this
//! engine's mechanism; the kinds that exist, their names and how they play
//! come from Add-Ons (`bot_kind`). Without such an Add-On there are none.
//!
//! A bot is an ordinary session player without a connection: it has a body,
//! inventory, health, minigame membership and replicated pose, so every
//! gameplay rule applies to it unchanged and it costs no traffic of its own.
//! Its brain produces one movement input per tick: it strolls near its brick,
//! walks paths the walk grid (`crate::nav`) finds, turns its aim at a limited
//! rate, fires after a reaction delay with a little error, keeps the distance
//! its weapon wants, turns on whoever hurts it and searches where it last saw
//! an enemy. Bots follow the minigame of their spawn brick's owner and are
//! harmless outside minigames.
use super::*;
use crate::bot_kind::BotKind;
use crate::nav::{Body, Found, Ground, Nav, Search, Waypoint};
use bri_weapons::ActorId;

pub const MAX_BOTS: usize = 16;
const TICK: f32 = 1.0 / 120.0;
/// Farthest from its start a path may lead, across.
const SEARCH_BOUND: f32 = 72.0;
/// Ticks without progress before a bot plans again.
const STUCK_TICKS: u32 = 45;
/// Plans in a row that got stuck before a bot drops its goal.
const MAX_REPLANS: u32 = 3;
/// Ticks between aim error changes.
const ERROR_TICKS: u64 = 48;

#[derive(Default)]
pub(super) struct Bots {
    /// Kinds the enabled Add-Ons provide.
    kinds: Vec<BotKind>,
    by_brick: BTreeMap<BrickId, OwnerId>,
    brains: BTreeMap<OwnerId, Brain>,
    /// The walk grid, one per body size in use.
    navs: Vec<(Body, Nav)>,
    /// Who last hurt each bot, and when.
    hurt: BTreeMap<OwnerId, (OwnerId, u64)>,
}
struct Brain {
    brick: BrickId,
    kind: BotKind,
    home: Vec3,
    sequence: u64,
    rng: u64,
    /// Where it is going and how.
    goal: Option<Goal>,
    plan: Vec<Waypoint>,
    search: Option<Search>,
    /// The goal's plan is walked (or none exists): no new search until the
    /// goal changes or the bot gets stuck.
    settled: bool,
    next_wander: u64,
    last_position: Vec3,
    stuck: u32,
    replans: u32,
    /// Current aim, turned toward the wanted one at the kind's rate.
    yaw: f32,
    pitch: f32,
    target: Option<OwnerId>,
    /// Tick the current target was first seen.
    seen_since: u64,
    /// Where an enemy was last seen or heard, until when.
    memory: Option<(Vec3, u64)>,
    error: (f32, f32),
    next_error: u64,
    fire_down: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Goal {
    Wander(Vec3),
    Chase(Vec3),
    Search(Vec3),
    Home,
}
impl Goal {
    fn point(self, home: Vec3) -> Vec3 {
        match self {
            Self::Wander(p) | Self::Chase(p) | Self::Search(p) => p,
            Self::Home => home,
        }
    }
}
impl Brain {
    fn random(&mut self) -> f32 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }
    fn set_goal(&mut self, goal: Option<Goal>) {
        if self.goal != goal {
            self.goal = goal;
            self.plan.clear();
            self.search = None;
            self.replans = 0;
            self.stuck = 0;
            self.settled = false;
        }
    }
}
/// What the held weapon wants: how far it reaches and how its shots fly.
#[derive(Clone, Copy, Debug)]
struct Weapon {
    melee: bool,
    reach: f32,
    speed: f32,
    /// Downward acceleration of its projectile, units per second squared.
    fall: f32,
    /// Its explosion's radius, kept clear of.
    splash: f32,
}
impl Weapon {
    /// Closest and farthest it likes to fight from.
    fn band(&self) -> (f32, f32) {
        if self.melee {
            (0.0, (self.reach * 0.8).max(1.2))
        } else {
            let far = (self.reach * 0.7).clamp(6.0, 40.0);
            let near = (self.splash + 3.0).max(5.0);
            (near, far.max(near + 4.0))
        }
    }
}
impl Bots {
    pub(super) fn is_bot(&self, owner: OwnerId) -> bool {
        self.brains.contains_key(&owner)
    }
    /// The vehicle spawn brick that made this bot.
    pub(super) fn spawn_brick(&self, owner: OwnerId) -> Option<BrickId> {
        self.brains.get(&owner).map(|b| b.brick)
    }
    pub(super) fn home(&self, owner: OwnerId) -> Option<Vec3> {
        self.brains.get(&owner).map(|b| b.home)
    }
    fn kind(&self, id: &str) -> Option<&BotKind> {
        self.kinds.iter().find(|k| k.id == id)
    }
    /// A bot was hurt: it turns on whoever did it.
    pub(super) fn note_hurt(&mut self, bot: OwnerId, source: Option<OwnerId>, tick: u64) {
        if let Some(source) = source.filter(|s| *s != bot)
            && self.brains.contains_key(&bot)
        {
            self.hurt.insert(bot, (source, tick));
        }
    }
}

fn yaw_to(delta: Vec3) -> f32 {
    delta.x.atan2(-delta.z)
}
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}
/// Turn `from` toward `to` by at most `step` radians.
fn turn(from: f32, to: f32, step: f32) -> f32 {
    wrap(from + wrap(to - from).clamp(-step, step))
}

/// What one bot sees this tick.
struct Sight {
    target: Option<(OwnerId, Vec3, Vec3)>,
}

impl Session {
    pub fn is_bot(&self, owner: OwnerId) -> bool {
        self.bots.is_bot(owner)
    }
    /// Install the bot kinds the enabled Add-Ons provide. Bots whose kind
    /// is gone leave at the next reconcile.
    pub fn set_bot_kinds(&mut self, kinds: Vec<BotKind>) -> Result<()> {
        ensure!(
            kinds.len() <= crate::bot_kind::MAX_KINDS,
            "Too many bot kinds"
        );
        for kind in &kinds {
            kind.validate()?;
        }
        self.bots.kinds = kinds;
        for brain in self.bots.brains.values_mut() {
            if let Some(kind) = self.bots.kinds.iter().find(|k| k.id == brain.kind.id) {
                brain.kind = kind.clone();
            }
        }
        self.vehicles.scanned = false;
        Ok(())
    }
    /// Whether a spawn brick choice names a bot kind this server has.
    pub fn is_bot_kind(&self, id: &str) -> bool {
        self.bots.kind(id).is_some()
    }
    pub(super) fn bot_home(&self, owner: OwnerId) -> Option<Vec3> {
        self.bots.home(owner)
    }
    /// The owner of the spawn brick that placed this bot.
    pub(super) fn bot_brick_owner(&self, bot: OwnerId) -> Option<OwnerId> {
        let brick = self.bots.brains.get(&bot)?.brick;
        Some(self.simulation.state().bricks.get(&brick)?.owner)
    }
    /// A rider in a bot mount's first seat moves it in place of its brain
    /// (`setControlObject` on a mount with no controlling client).
    pub(super) fn drive_bot(&mut self, bot: OwnerId, input: MoveInput) -> Result<()> {
        let Some(brain) = self.bots.brains.get_mut(&bot) else {
            return Ok(());
        };
        brain.sequence += 1;
        let sequence = brain.sequence;
        self.movement(bot, sequence, input)
    }
    /// Reconcile bots with spawn bricks naming a bot kind.
    pub(super) fn reconcile_bot_brick(
        &mut self,
        brick_id: BrickId,
        wanted: Option<&str>,
    ) -> Result<()> {
        let current = self.bots.by_brick.get(&brick_id).copied();
        let kind = wanted.and_then(|id| self.bots.kind(id)).cloned();
        let same = current
            .and_then(|bot| self.bots.brains.get(&bot))
            .is_some_and(|b| kind.as_ref().is_some_and(|k| k.id == b.kind.id));
        if let Some(bot) = current.filter(|_| !same) {
            if self.peers.contains_key(&bot) {
                self.disconnect(bot)?;
                self.departed.remove(&bot);
            }
            self.bots.by_brick.remove(&brick_id);
            self.bots.brains.remove(&bot);
            self.bots.hurt.remove(&bot);
        }
        if same || self.bots.brains.len() >= MAX_BOTS {
            return Ok(());
        }
        let Some(kind) = kind else {
            return Ok(());
        };
        let Some(brick) = self.simulation.state().bricks.get(&brick_id) else {
            return Ok(());
        };
        let home = Vec3::from(brick.position) + Vec3::Y * 0.3;
        // A crowded spawn is retried on later ticks.
        if let Ok(bot) = self.join_inner(kind.name.clone(), home, false, true, None) {
            self.bots.by_brick.insert(brick_id, bot);
            self.bots.brains.insert(
                bot,
                Brain {
                    brick: brick_id,
                    kind,
                    home,
                    sequence: 0,
                    rng: 0x2545_F491_4F6C_DD1D ^ bot.wrapping_mul(0x9E37_79B9),
                    goal: None,
                    plan: Vec::new(),
                    search: None,
                    settled: false,
                    next_wander: 0,
                    last_position: home,
                    stuck: 0,
                    replans: 0,
                    yaw: 0.0,
                    pitch: 0.0,
                    target: None,
                    seen_since: 0,
                    memory: None,
                    error: (0.0, 0.0),
                    next_error: 0,
                    fire_down: false,
                },
            );
        }
        Ok(())
    }
    pub(super) fn bot_bricks(&self) -> Vec<BrickId> {
        self.bots.by_brick.keys().copied().collect()
    }
    /// Bots whose brick vanished; reconcile removes them.
    fn bot_bricks_pending(&self) -> Vec<BrickId> {
        self.bots
            .brains
            .values()
            .filter(|b| !self.simulation.state().bricks.contains_key(&b.brick))
            .map(|b| b.brick)
            .collect()
    }
    /// Minigame membership follows the spawn brick owner.
    fn sync_bot_minigames(&mut self) -> Result<()> {
        let bots: Vec<(OwnerId, BrickId)> = self
            .bots
            .brains
            .iter()
            .map(|(o, b)| (*o, b.brick))
            .collect();
        for (bot, brick) in bots {
            let Some(owner) = self.simulation.state().bricks.get(&brick).map(|b| b.owner) else {
                continue;
            };
            let wanted = self
                .peers
                .get(&owner)
                .and_then(|p| self.minigames.player(p.combat.player).ok())
                .and_then(|p| p.game);
            let Some(peer) = self.peers.get(&bot) else {
                continue;
            };
            let player = peer.combat.player;
            let current = self.minigames.player(player).ok().and_then(|p| p.game);
            if current != wanted {
                let effects = self
                    .minigames
                    .host_place(player, wanted)
                    .map_err(|e| anyhow::anyhow!("Bot minigame: {e}"))?;
                self.apply_minigame_effects(effects)?;
            }
        }
        Ok(())
    }
    /// Whether `bot` treats `other` as an enemy: anyone it may hurt, except
    /// bots of the same builder, who are on its side.
    fn bot_enemy(&self, bot: OwnerId, kind: &BotKind, other: OwnerId) -> bool {
        if other == bot || !self.peers.get(&other).is_some_and(|p| p.combat.alive) {
            return false;
        }
        if self.bots.is_bot(other)
            && (!kind.fights_bots || self.bot_brick_owner(other) == self.bot_brick_owner(bot))
        {
            return false;
        }
        self.can_damage_player(bot, other, false)
    }
    /// Whether `from` sees `to`: nothing solid between them.
    fn bot_sees(&self, from: Vec3, to: Vec3) -> bool {
        let delta = to - from;
        let distance = delta.length();
        distance > 0.1
            && self
                .simulation
                .target(
                    from,
                    delta / distance,
                    distance.min(Simulation::MAX_TARGET_DISTANCE),
                )
                .ok()
                .flatten()
                .is_none_or(|hit| hit.distance > distance - 0.5)
    }
    fn bot_sight(&self, bot: OwnerId, brain: &Brain, eye: Vec3) -> Sight {
        let kind = &brain.kind;
        let visible = |owner: OwnerId| -> Option<(Vec3, Vec3)> {
            let p = self.peers.get(&owner)?;
            let (at, feet) = (p.player.eye(), Vec3::from(p.player.state().feet));
            (at.distance(eye) < kind.sight
                && self.bot_enemy(bot, kind, owner)
                && self.bot_sees(eye, at))
            .then_some((at, feet))
        };
        // Keep fighting the same enemy while it stays in view.
        if let Some(current) = brain.target
            && let Some((at, feet)) = visible(current)
        {
            return Sight {
                target: Some((current, at, feet)),
            };
        }
        let mut best: Option<(OwnerId, Vec3, Vec3)> = None;
        let mut candidates: Vec<(f32, OwnerId)> = self
            .peers
            .iter()
            .filter(|(owner, p)| **owner != bot && p.combat.alive)
            .map(|(owner, p)| (p.player.eye().distance(eye), *owner))
            .filter(|(d, _)| *d < kind.sight)
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, owner) in candidates {
            if let Some((at, feet)) = visible(owner) {
                best = Some((owner, at, feet));
                break;
            }
        }
        Sight { target: best }
    }
    /// The held weapon's reach and flight.
    fn bot_weapon(&self, bot: OwnerId) -> Option<Weapon> {
        let (image, _) = self.weapons.image_state(ActorId(bot), 0)?;
        let projectile = image
            .projectile
            .as_ref()
            .and_then(|p| self.weapons.pack.projectiles.get(p));
        let Some(p) = projectile else {
            return Some(Weapon {
                melee: true,
                reach: 3.0,
                speed: 0.0,
                fall: 0.0,
                splash: 0.0,
            });
        };
        let reach = p.speed * p.lifetime_ticks as f32 * TICK;
        Some(Weapon {
            melee: image.melee || reach < 6.0,
            reach,
            speed: p.speed,
            fall: bri_weapons::runtime::fall_per_tick(p) * 120.0,
            splash: p.explosion.radius,
        })
    }
    /// One brain tick per bot: see, choose a goal, find the way, aim and
    /// pull the trigger.
    pub(super) fn step_bots(&mut self) -> Result<()> {
        if self.bots.brains.is_empty() {
            if !self.bots.navs.is_empty() {
                self.bots.navs.clear();
                self.simulation.track_collision_changes(false);
            }
            return Ok(());
        }
        self.simulation.track_collision_changes(true);
        let changes = self.simulation.take_collision_changes();
        for (body, nav) in &mut self.bots.navs {
            for (min, max) in &changes {
                nav.invalidate(*min, *max, body);
            }
            nav.begin_tick();
        }
        for brick in self.bot_bricks_pending() {
            self.reconcile_bot_brick(brick, None)?;
        }
        let tick = self.simulation.state().tick;
        if tick.is_multiple_of(30) {
            self.sync_bot_minigames()?;
        }
        // Bots share the grid's sampling budget; start with a different one
        // each tick so none waits behind the others.
        let mut bots: Vec<OwnerId> = self.bots.brains.keys().copied().collect();
        if !bots.is_empty() {
            let len = bots.len();
            bots.rotate_left(tick as usize % len);
        }
        for bot in bots {
            self.step_bot(bot, tick)?;
        }
        Ok(())
    }
    fn step_bot(&mut self, bot: OwnerId, tick: u64) -> Result<()> {
        let Some(peer) = self.peers.get(&bot) else {
            return Ok(());
        };
        if !peer.combat.alive {
            if tick >= peer.combat.respawn_tick + 120 {
                let _ = self.request_respawn(bot);
            }
            if let Some(brain) = self.bots.brains.get_mut(&bot) {
                brain.set_goal(None);
                brain.target = None;
                brain.memory = None;
            }
            return Ok(());
        }
        if self.riding.driver_of(bot).is_some() {
            return Ok(());
        }
        let state = peer.player.state().clone();
        let feet = Vec3::from(state.feet);
        let eye = peer.player.eye();
        let body = Body::of(peer.player.tuning(), state.scale);
        let brain = &self.bots.brains[&bot];
        let sight = self.bot_sight(bot, brain, eye);
        let weapon = self.bot_weapon(bot);
        let hurt = self.bots.hurt.remove(&bot);
        let hurt_by = hurt.and_then(|(source, _)| {
            let kind = &self.bots.brains[&bot].kind;
            let p = self.peers.get(&source)?;
            self.bot_enemy(bot, kind, source)
                .then(|| Vec3::from(p.player.state().feet))
        });
        let target_velocity = sight
            .target
            .and_then(|(owner, _, _)| self.peers.get(&owner))
            .map_or(Vec3::ZERO, |p| Vec3::from(p.player.state().velocity));

        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let kind = brain.kind.clone();
        let moved = feet.distance(brain.last_position);
        brain.last_position = feet;
        if brain.sequence == 0 {
            brain.yaw = state.yaw;
        }
        // Remember enemies seen, and where a hit came from.
        let memory_ticks = (kind.memory_seconds * 120.0) as u64;
        match sight.target {
            Some((owner, _, target_feet)) => {
                if brain.target != Some(owner) {
                    brain.target = Some(owner);
                    brain.seen_since = tick;
                }
                brain.memory = Some((target_feet, tick + memory_ticks));
            }
            None => {
                brain.target = None;
                if let Some(at) = hurt_by {
                    brain.memory = Some((at, tick + memory_ticks));
                }
            }
        }
        if brain.memory.is_some_and(|(_, until)| tick >= until) {
            brain.memory = None;
        }
        let away = flat(feet - brain.home).length();
        if away > kind.chase_radius {
            brain.memory = None;
            brain.target = None;
        }

        // Goal.
        let mut hold = false;
        let mut back_off = false;
        match (
            sight.target.filter(|_| away <= kind.chase_radius),
            brain.memory,
        ) {
            (Some((_, _, target_feet)), _) => {
                let (near, far) = weapon.map_or((2.0, 3.0), |w| w.band());
                let distance = flat(target_feet - feet).length();
                if distance > far || (target_feet.y - feet.y).abs() > body.step + 1.0 {
                    let moved_on = match brain.goal {
                        Some(Goal::Chase(p)) => p.distance(target_feet) > 2.5,
                        _ => true,
                    };
                    if moved_on {
                        brain.set_goal(Some(Goal::Chase(target_feet)));
                    }
                } else {
                    brain.set_goal(None);
                    hold = true;
                    back_off = distance < near;
                }
            }
            (None, Some((at, _))) => {
                if flat(at - feet).length() > 1.5 {
                    if brain.goal != Some(Goal::Search(at)) {
                        brain.set_goal(Some(Goal::Search(at)));
                    }
                } else {
                    // Got there: look around until it forgets.
                    brain.set_goal(None);
                    hold = true;
                }
            }
            (None, None) => match brain.goal {
                // The fight is over: back to its brick's surroundings.
                Some(Goal::Chase(_) | Goal::Search(_)) => brain.set_goal(None),
                _ if away > kind.wander_radius + 4.0 => brain.set_goal(Some(Goal::Home)),
                None if tick >= brain.next_wander => {
                    let angle = brain.random() * std::f32::consts::TAU;
                    let radius = brain.random() * kind.wander_radius;
                    let point = brain.home + Vec3::new(angle.sin(), 0.0, angle.cos()) * radius;
                    brain.set_goal(Some(Goal::Wander(point)));
                    brain.next_wander = tick + 240 + (brain.random() * 480.0) as u64;
                }
                _ => {}
            },
        }

        // Path.
        let home = brain.home;
        let mut wanted = None;
        if let Some(goal) = brain.goal {
            let point = goal.point(home);
            if brain.plan.is_empty() && brain.search.is_none() && !brain.settled {
                brain.search = Some(Search::new(feet, point, SEARCH_BOUND));
            }
            if brain.search.is_some() {
                let physics = &self.simulation.physics;
                let simulation = &self.simulation;
                let terrain = |o: Vec3, d: Vec3, r: f32| simulation.terrain_ray(o, d, r);
                let ground = Ground {
                    physics,
                    terrain: &terrain,
                };
                let at = match self.bots.navs.iter().position(|(b, _)| *b == body) {
                    Some(at) => at,
                    None => {
                        let mut nav = Nav::default();
                        nav.begin_tick();
                        self.bots.navs.push((body, nav));
                        self.bots.navs.len() - 1
                    }
                };
                let (bots_navs, brains) = (&mut self.bots.navs, &mut self.bots.brains);
                let nav = &mut bots_navs[at].1;
                let brain = brains.get_mut(&bot).unwrap();
                if let Some(found) = brain.search.as_mut().unwrap().step(nav, &ground, &body) {
                    brain.search = None;
                    match found {
                        Found::Path(path) | Found::Partial(path) if !path.is_empty() => {
                            brain.plan = path
                        }
                        // Already as close as it gets, or nowhere to stand.
                        _ => brain.plan.clear(),
                    }
                }
            }
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            while let Some(next) = brain.plan.first() {
                let d = next.feet - feet;
                if flat(d).length() < 0.4 && d.y.abs() < body.step + 0.5 {
                    brain.plan.remove(0);
                    brain.stuck = 0;
                } else {
                    break;
                }
            }
            if brain.plan.is_empty() && brain.search.is_none() {
                // Walked the whole plan: arrived, or as near as it goes.
                brain.settled = true;
                if matches!(brain.goal, Some(Goal::Wander(_) | Goal::Home)) {
                    brain.goal = None;
                }
            }
            wanted = brain.plan.first().copied();
        }
        let brain = self.bots.brains.get_mut(&bot).unwrap();

        // Aim: at the enemy, or where it walks.
        let mut aim_yaw = brain.yaw;
        let mut aim_pitch = 0.0;
        let mut fire = false;
        if let Some((_, target_eye, _)) = sight.target {
            let mut at = target_eye - Vec3::Y * 0.5;
            if let Some(w) = weapon.filter(|w| !w.melee && w.speed > 0.0) {
                let time = at.distance(eye) / w.speed;
                at += target_velocity * time;
                at.y += 0.5 * w.fall * time * time;
            }
            let delta = at - eye;
            if tick >= brain.next_error {
                let tracked = (tick - brain.seen_since) as f32 * TICK;
                let size = kind.aim_error_degrees.to_radians()
                    * (1.0 - (tracked / 2.0).min(1.0) * 2.0 / 3.0);
                brain.error = (
                    (brain.random() * 2.0 - 1.0) * size,
                    (brain.random() * 2.0 - 1.0) * size * 0.5,
                );
                brain.next_error = tick + ERROR_TICKS;
            }
            aim_yaw = wrap(yaw_to(delta) + brain.error.0);
            aim_pitch = (delta.y.atan2(flat(delta).length()) + brain.error.1).clamp(-1.5, 1.5);
            let reaction = (kind.reaction_seconds * 120.0) as u64;
            let in_reach = weapon.is_some_and(|w| delta.length() <= w.reach.max(1.0) * 1.1 + 0.5);
            fire = tick >= brain.seen_since + reaction
                && in_reach
                && wrap(aim_yaw - brain.yaw).abs() < 0.1
                && (aim_pitch - brain.pitch).abs() < 0.12;
        } else if let Some(next) = wanted {
            let d = flat(next.feet - feet);
            if d.length() > 0.05 {
                aim_yaw = yaw_to(d);
            }
        } else if hold {
            // Searching the spot: sweep the view.
            aim_yaw = wrap(brain.yaw + 0.8 * TICK * 2.0);
        }
        let step = kind.turn_degrees.to_radians() * TICK;
        brain.yaw = turn(brain.yaw, aim_yaw, step);
        brain.pitch += (aim_pitch - brain.pitch).clamp(-step, step);

        // Move along the plan, facing wherever it aims.
        let mut input = MoveInput {
            yaw: brain.yaw,
            pitch: brain.pitch.clamp(-1.5, 1.5),
            ..Default::default()
        };
        let forward = Vec3::new(brain.yaw.sin(), 0.0, -brain.yaw.cos());
        let right = Vec3::new(brain.yaw.cos(), 0.0, brain.yaw.sin());
        let mut direction = Vec3::ZERO;
        if let Some(next) = wanted {
            direction = flat(next.feet - feet).normalize_or_zero();
            input.jump = next.jump && flat(next.feet - feet).length() < 1.6 && state.grounded;
        } else if hold && sight.target.is_some() {
            // In its band: strafe so it is not a still target, and give
            // ground if too close.
            let side = if (tick / 90 + bot).is_multiple_of(2) {
                0.7
            } else {
                -0.7
            };
            direction = right * side;
            if back_off {
                direction -= forward;
            }
        }
        input.forward = direction.dot(forward).clamp(-1.0, 1.0);
        input.right = direction.dot(right).clamp(-1.0, 1.0);
        if let Some((_, target_eye, _)) = sight.target
            && target_eye.y - eye.y > 3.0
            && flat(target_eye - eye).length() < 20.0
            && wanted.is_none()
        {
            input.jet = true;
        }
        // Walking into something: hop, then plan again, then give up.
        let trying = input.forward != 0.0 || input.right != 0.0;
        if trying && moved < 0.01 {
            brain.stuck += 1;
        } else {
            brain.stuck = 0;
        }
        if brain.stuck > 20 && brain.stuck % 40 < 5 {
            input.jump = true;
        }
        let mut forget = false;
        if brain.stuck > STUCK_TICKS && wanted.is_some() {
            brain.stuck = 0;
            brain.replans += 1;
            brain.plan.clear();
            brain.search = None;
            brain.settled = false;
            // What blocked it may be newer than the grid: look again.
            forget = true;
            if brain.replans > MAX_REPLANS {
                brain.goal = None;
                brain.replans = 0;
                brain.next_wander = tick + 120;
            }
        }
        brain.sequence += 1;
        let sequence = brain.sequence;
        let fire_changed = fire != brain.fire_down;
        if forget && let Some((_, nav)) = self.bots.navs.iter_mut().find(|(b, _)| *b == body) {
            nav.invalidate(feet - Vec3::splat(1.0), feet + Vec3::splat(1.0), &body);
        }
        // Pulse the trigger so semi-automatic weapons keep firing.
        let pulse = fire && tick.is_multiple_of(40);
        brain.fire_down = fire && !pulse;
        self.movement(bot, sequence, input)?;
        if sight.target.is_some() {
            self.bot_arm(bot)?;
        }
        let direction = Vec3::new(
            input.yaw.sin() * input.pitch.cos(),
            input.pitch.sin(),
            -input.yaw.cos() * input.pitch.cos(),
        );
        if fire_changed || pulse {
            let down = fire && !pulse;
            if self.weapons.image_state(ActorId(bot), 0).is_some() || !down {
                // A bot's look reaches the host with its trigger.
                let _ = self.weapon_trigger(bot, down, direction, false);
                if down {
                    self.note_shot(bot);
                }
            }
        }
        Ok(())
    }
    /// Equip the first real weapon (not a building tool) in the inventory.
    fn bot_arm(&mut self, bot: OwnerId) -> Result<()> {
        let Some(actor) = self.weapons.actor(ActorId(bot)) else {
            return Ok(());
        };
        let weapon = actor.inventory.iter().position(|item| {
            item.as_deref()
                .is_some_and(|id| !bri_weapons::CORE_TOOLS.contains(&id))
        });
        if weapon.is_some() && actor.selected != weapon {
            let _ = self.equip_tool(bot, weapon);
        }
        Ok(())
    }
    /// Bot kinds for the Vehicle Spawn list, as (id, name).
    pub fn bot_choices(&self) -> Vec<(String, String)> {
        self.bots
            .kinds
            .iter()
            .map(|k| (k.id.clone(), k.name.clone()))
            .collect()
    }
}
