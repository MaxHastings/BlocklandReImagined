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
//!
//! Add-On rules add bots too (`add_bot`, Slayer's Preferred Player Count):
//! those belong to a mini-game rather than a brick. They spawn where the
//! game's members do, roam from wherever they are rather than a brick, fight
//! whoever the game lets them hurt, rest while the rules hold them still
//! and leave with their game.
//!
//! Portals (linked bricks) are part of the world a bot knows: it sees and
//! aims through an opening at what stands beyond its partner, its paths
//! lead through openings where that is the way (`crate::nav`), and it
//! follows an enemy it watched go in. Its leash to its brick stretches the
//! way it walked, through openings included.
use super::*;
use crate::bot_kind::BotKind;
use crate::nav::{Body, Found, Ground, Nav, Search, Waypoint};
use bri_content::passage::{Way, carried_yaw};
use bri_package_runtime::ops::ObjectRef;
use bri_weapons::ActorId;

pub const MAX_BOTS: usize = bri_package_runtime::ops::MAX_BOTS;
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
    /// Bots a mini-game's rules added: by bot, the package and its game.
    by_rules: BTreeMap<OwnerId, (String, u64)>,
    brains: BTreeMap<OwnerId, Brain>,
    /// The walk grid, one per body size in use.
    navs: Vec<(Body, Nav)>,
    /// Who last hurt each bot, and when.
    hurt: BTreeMap<OwnerId, (OwnerId, u64)>,
}
struct Brain {
    /// The vehicle spawn brick that made it; `None` for a bot the rules
    /// added (`Bots::by_rules`).
    brick: Option<BrickId>,
    kind: BotKind,
    /// Where it strolls around: its brick, or for a rules bot wherever it
    /// last stood idle.
    home: Vec3,
    /// `home` as seen from where it stands: carried with it through every
    /// opening it goes through, so how far it strayed is how far it walked.
    leash: Vec3,
    /// The last trip through a portal (`Session::crossings`) it took
    /// account of.
    crossed: u64,
    /// The rules hold its brain still (`rest_bot`).
    resting: bool,
    /// A rules bot came back to life: where it stands next is its home.
    rehome: bool,
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
    fn new(brick: Option<BrickId>, kind: BotKind, home: Vec3, bot: OwnerId, crossed: u64) -> Self {
        Self {
            brick,
            kind,
            home,
            leash: home,
            crossed,
            resting: false,
            rehome: brick.is_none(),
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
        }
    }
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
        self.brains.get(&owner).and_then(|b| b.brick)
    }
    /// Where a brick's bot spawns: by its brick. A rules bot spawns where
    /// its game's members do.
    pub(super) fn home(&self, owner: OwnerId) -> Option<Vec3> {
        self.brains
            .get(&owner)
            .filter(|b| b.brick.is_some())
            .map(|b| b.home)
    }
    /// A bot a mini-game's rules added, and the package that added it: it
    /// plays as a member, so the rules' player hooks hear of it.
    pub(super) fn rules_package(&self, owner: OwnerId) -> Option<&str> {
        self.by_rules.get(&owner).map(|(p, _)| p.as_str())
    }
    /// A bot a spawn brick made: it is the brick's, not a member's, and
    /// the rules' player hooks leave it out.
    pub(super) fn is_brick_bot(&self, owner: OwnerId) -> bool {
        self.is_bot(owner) && !self.by_rules.contains_key(&owner)
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

/// An enemy a bot sees, and by which way.
#[derive(Clone, Copy)]
struct Seen {
    owner: OwnerId,
    /// Its eye and feet as seen along the way (through an opening, where
    /// they would stand were the partner's side right behind it).
    eye: Vec3,
    feet: Vec3,
    /// Where its feet really are.
    real: Vec3,
    way: Way,
}
/// What one bot sees this tick.
struct Sight {
    target: Option<Seen>,
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
        let brick = self.bots.brains.get(&bot)?.brick?;
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
            self.drop_bot(bot)?;
        }
        if same {
            return Ok(());
        }
        let Some(kind) = kind else {
            return Ok(());
        };
        let Some(brick) = self.simulation.state().bricks.get(&brick_id) else {
            return Ok(());
        };
        let (home, builder) = (Vec3::from(brick.position) + Vec3::Y * 0.3, brick.owner);
        // A refused bot is never silent: the brick's builder is told why,
        // as for a vehicle the server has no room for.
        if self.bots.brains.len() >= MAX_BOTS {
            self.notify(
                builder,
                Notice::Center {
                    text: format!("\u{E000}Server is limited to {MAX_BOTS} bots"),
                    seconds: 2.0,
                },
            );
            return Ok(());
        }
        let joined = self.join_inner(kind.name.clone(), home, false, true, None);
        if joined.is_err() {
            self.notify(
                builder,
                Notice::Center {
                    text: "\u{E000}Server is full".into(),
                    seconds: 2.0,
                },
            );
        }
        let crossed = self.crossings.count();
        if let Ok(bot) = joined {
            self.bots.by_brick.insert(brick_id, bot);
            self.bots
                .brains
                .insert(bot, Brain::new(Some(brick_id), kind, home, bot, crossed));
            self.weapons.set_bot(bri_weapons::ActorId(bot), true)?;
        }
        Ok(())
    }
    /// A bot leaves the server, whatever made it.
    fn drop_bot(&mut self, bot: OwnerId) -> Result<()> {
        if self.peers.contains_key(&bot) {
            self.disconnect(bot)?;
            self.departed.remove(&bot);
        }
        if let Some(brick) = self.bots.brains.remove(&bot).and_then(|b| b.brick) {
            self.bots.by_brick.remove(&brick);
        }
        self.bots.by_rules.remove(&bot);
        self.bots.hurt.remove(&bot);
        self.forget_player_state(bot);
        Ok(())
    }
    /// `add_bot`: a bot of `kind` joins `game` for `package`'s rules, on
    /// `team` when given (Slayer's `addBotToGame` and `addMember`).
    pub(super) fn add_rules_bot(
        &mut self,
        package: &str,
        game: u64,
        team: Option<u64>,
        kind: &str,
        name: &str,
    ) -> Result<()> {
        ensure!(
            self.bots.brains.len() < MAX_BOTS,
            "Server is limited to {MAX_BOTS} bots"
        );
        let kind = self
            .bots
            .kind(kind)
            .with_context(|| format!("No bot kind `{kind}`: its Add-On is not enabled"))?
            .clone();
        let game = bri_minigames::GameId(game);
        self.minigames
            .game(game)
            .map_err(|_| anyhow::anyhow!("No mini-game {}", game.0))?;
        let team = team
            .map(|t| u32::try_from(t).map(bri_minigames::TeamId))
            .transpose()
            .ok()
            .context("No such team")?;
        let drop = self.spawn_points.first().copied().unwrap_or(Vec3::Y);
        let bot = self.join_inner(name.to_owned(), drop, false, true, None)?;
        let crossed = self.crossings.count();
        self.bots
            .brains
            .insert(bot, Brain::new(None, kind, drop, bot, crossed));
        self.weapons.set_bot(bri_weapons::ActorId(bot), true)?;
        self.bots.by_rules.insert(bot, (package.to_owned(), game.0));
        let placed = (|| -> Result<()> {
            let player = self.peers[&bot].combat.player;
            let effects = self
                .minigames
                .host_place(player, Some(game))
                .map_err(|e| anyhow::anyhow!("Bot minigame: {e}"))?;
            self.apply_minigame_effects(effects)?;
            if let Some(team) = team {
                let effects = self
                    .minigames
                    .assign_team(player, Some(team))
                    .map_err(|e| anyhow::anyhow!("Team rejected: {e}"))?;
                self.apply_minigame_effects(effects)?;
                // It came in before it had a side: it appears where its
                // team does (`Slayer_TeamSO::addMember` spawns it again).
                let effects = self
                    .minigames
                    .execute(bri_minigames::Command::ForceRespawn { target: player })
                    .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
                self.apply_minigame_effects(effects)?;
            }
            Ok(())
        })();
        if placed.is_err() {
            self.drop_bot(bot)?;
        }
        placed
    }
    /// The bot, if `package`'s rules added it.
    fn own_bot(&self, package: &str, bot: OwnerId) -> Result<()> {
        ensure!(
            self.bots.rules_package(bot) == Some(package),
            "Bot {bot} is not one `{package}` added"
        );
        Ok(())
    }
    pub(super) fn remove_rules_bot(&mut self, package: &str, bot: OwnerId) -> Result<()> {
        self.own_bot(package, bot)?;
        self.drop_bot(bot)
    }
    pub(super) fn rules_bot_tool(
        &mut self,
        package: &str,
        bot: OwnerId,
        slot: Option<u8>,
    ) -> Result<()> {
        self.own_bot(package, bot)?;
        ensure!(self.is_alive(bot), "Only a living bot holds things");
        self.equip_tool(bot, slot.map(usize::from))
    }
    pub(super) fn rest_rules_bot(&mut self, package: &str, bot: OwnerId, rest: bool) -> Result<()> {
        self.own_bot(package, bot)?;
        let brain = self.bots.brains.get_mut(&bot).context("No such bot")?;
        if rest && !brain.resting {
            brain.set_goal(None);
            brain.target = None;
            brain.memory = None;
        }
        brain.resting = rest;
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
            .filter_map(|b| b.brick)
            .filter(|brick| !self.simulation.state().bricks.contains_key(brick))
            .collect()
    }
    /// Rules bots whose game ended, or who were put out of it: they leave
    /// with it (`Slayer_MiniGameSO::endGame` deletes its bots).
    fn rules_bots_gone(&self) -> Vec<OwnerId> {
        self.bots
            .by_rules
            .iter()
            .filter(|(bot, (_, game))| {
                self.peers
                    .get(bot)
                    .and_then(|p| self.minigames.player(p.combat.player).ok())
                    .and_then(|p| p.game)
                    != Some(bri_minigames::GameId(*game))
            })
            .map(|(bot, _)| *bot)
            .collect()
    }
    /// Minigame membership follows the spawn brick owner.
    fn sync_bot_minigames(&mut self) -> Result<()> {
        let bots: Vec<(OwnerId, BrickId)> = self
            .bots
            .brains
            .iter()
            .filter_map(|(o, b)| Some((*o, b.brick?)))
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
    /// bots of the same builder, who are on its side. A rules bot plays
    /// as a member, so its game alone says who is on its side (Slayer's
    /// `checkHoleBotTeams`).
    fn bot_enemy(&self, bot: OwnerId, kind: &BotKind, other: OwnerId) -> bool {
        if other == bot || !self.peers.get(&other).is_some_and(|p| p.combat.alive) {
            return false;
        }
        if self.bots.is_brick_bot(bot)
            && self.bots.is_brick_bot(other)
            && (!kind.fights_bots || self.bot_brick_owner(other) == self.bot_brick_owner(bot))
        {
            return false;
        }
        self.can_damage_player(bot, other, false)
    }
    fn bot_sight(&self, bot: OwnerId, brain: &Brain, eye: Vec3) -> Sight {
        let kind = &brain.kind;
        let visible = |owner: OwnerId| -> Option<Seen> {
            let p = self.peers.get(&owner)?;
            if !self.bot_enemy(bot, kind, owner) {
                return None;
            }
            let real = Vec3::from(p.player.state().feet);
            let way = self.simulation.sight(eye, p.player.eye(), kind.sight)?;
            Some(Seen {
                owner,
                eye: way.aim,
                feet: way.seen(real),
                real,
                way,
            })
        };
        // Keep fighting the same enemy while it stays in view.
        if let Some(seen) = brain.target.and_then(visible) {
            return Sight { target: Some(seen) };
        }
        // Through an opening, anyone may be in sight wherever they stand.
        let portals = !self.simulation.passages().list.is_empty();
        let mut candidates: Vec<(f32, OwnerId)> = self
            .peers
            .iter()
            .filter(|(owner, p)| **owner != bot && p.combat.alive)
            .map(|(owner, p)| (p.player.eye().distance(eye), *owner))
            .filter(|(d, _)| portals || *d < kind.sight)
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Sight {
            target: candidates.into_iter().find_map(|(_, owner)| visible(owner)),
        }
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
        for bot in self.rules_bots_gone() {
            self.drop_bot(bot)?;
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
            // A brick's bot comes back a second after it may; a rules bot
            // as soon as its game lets it (Slayer's bot respawn time).
            let wait = if self.bots.by_rules.contains_key(&bot) { 0 } else { 120 };
            if tick >= peer.combat.respawn_tick + wait {
                let _ = self.request_respawn(bot);
            }
            if let Some(brain) = self.bots.brains.get_mut(&bot) {
                brain.set_goal(None);
                brain.target = None;
                brain.memory = None;
                brain.rehome = brain.brick.is_none();
                brain.leash = brain.home;
            }
            return Ok(());
        }
        // Held still by the rules: it stands, holding its fire.
        if self.bots.brains.get(&bot).is_some_and(|b| b.resting) {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.sequence += 1;
            let sequence = brain.sequence;
            let input = MoveInput {
                yaw: brain.yaw,
                pitch: brain.pitch.clamp(-1.5, 1.5),
                ..Default::default()
            };
            let release = std::mem::take(&mut brain.fire_down);
            self.movement(bot, sequence, input)?;
            if release {
                let _ = self.weapon_trigger(bot, false, Vec3::ZERO, false);
            }
            return Ok(());
        }
        // Ridden by a player who steers it, or carried by one
        // (`mountObject`): its brain rests.
        if self.riding.driver_of(bot).is_some() || self.riding.is_riding(bot) {
            return Ok(());
        }
        let state = peer.player.state().clone();
        let feet = Vec3::from(state.feet);
        let eye = peer.player.eye();
        let body = Body::of(peer.player.tuning(), state.scale);
        self.bot_crossed(bot);
        let brain = &self.bots.brains[&bot];
        let sight = self.bot_sight(bot, brain, eye);
        // An enemy it was watching went in through an opening as it went
        // out of sight: it knows where that leads.
        let followed = brain
            .target
            .filter(|_| sight.target.is_none())
            .filter(|owner| {
                self.crossings
                    .last_of(ObjectRef::Player(*owner))
                    .is_some_and(|c| tick.saturating_sub(c.tick) <= 2)
            })
            .and_then(|owner| self.peers.get(&owner))
            .map(|p| Vec3::from(p.player.state().feet));
        let weapon = self.bot_weapon(bot);
        let hurt = self.bots.hurt.remove(&bot);
        let hurt_by = hurt.and_then(|(source, _)| {
            let kind = &self.bots.brains[&bot].kind;
            let p = self.peers.get(&source)?;
            self.bot_enemy(bot, kind, source)
                .then(|| Vec3::from(p.player.state().feet))
        });
        let target_velocity = sight.target.map_or(Vec3::ZERO, |seen| {
            self.peers.get(&seen.owner).map_or(Vec3::ZERO, |p| {
                seen.way.seen_vector(Vec3::from(p.player.state().velocity))
            })
        });

        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let kind = brain.kind.clone();
        if std::mem::take(&mut brain.rehome) {
            brain.home = feet;
            brain.leash = feet;
            brain.last_position = feet;
        }
        let moved = feet.distance(brain.last_position);
        brain.last_position = feet;
        if brain.sequence == 0 {
            brain.yaw = state.yaw;
        }
        // Remember enemies seen, and where a hit came from.
        let memory_ticks = (kind.memory_seconds * 120.0) as u64;
        match sight.target {
            Some(seen) => {
                if brain.target != Some(seen.owner) {
                    brain.target = Some(seen.owner);
                    brain.seen_since = tick;
                }
                brain.memory = Some((seen.real, tick + memory_ticks));
            }
            None => {
                brain.target = None;
                if let Some(at) = hurt_by.or(followed) {
                    brain.memory = Some((at, tick + memory_ticks));
                }
            }
        }
        if brain.memory.is_some_and(|(_, until)| tick >= until) {
            brain.memory = None;
        }
        let away = flat(feet - brain.leash).length();
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
            (Some(seen), _) => {
                let (near, far) = weapon.map_or((2.0, 3.0), |w| w.band());
                // How far it is the way it is seen; the chase heads for
                // where it really stands, and the path finds the way there.
                let distance = flat(seen.feet - feet).length();
                if distance > far || (seen.feet.y - feet.y).abs() > body.step + 1.0 {
                    let moved_on = match brain.goal {
                        Some(Goal::Chase(p)) => p.distance(seen.real) > 2.5,
                        _ => true,
                    };
                    if moved_on {
                        brain.set_goal(Some(Goal::Chase(seen.real)));
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
                // A rules bot has no brick to return to: it roams on from
                // wherever it is (Slayer's bots, `hReturnToSpawn` off).
                None if brain.brick.is_none() => {
                    brain.home = feet;
                    if tick >= brain.next_wander {
                        let angle = brain.random() * std::f32::consts::TAU;
                        let radius = brain.random() * kind.wander_radius;
                        let point = feet + Vec3::new(angle.sin(), 0.0, angle.cos()) * radius;
                        brain.set_goal(Some(Goal::Wander(point)));
                        brain.next_wander = tick + 240 + (brain.random() * 480.0) as u64;
                    }
                }
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
                    passages: simulation.passages(),
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
                // One through an opening is reached by going through.
                if next.through.is_none() && flat(d).length() < 0.4 && d.y.abs() < body.step + 0.5 {
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
        if let Some(seen) = sight.target {
            let mut at = seen.eye - Vec3::Y * 0.5;
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
            let d = flat(next.through.unwrap_or(next.feet) - feet);
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
            direction = flat(next.through.unwrap_or(next.feet) - feet).normalize_or_zero();
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
        if let Some(seen) = sight.target
            && seen.eye.y - eye.y > 3.0
            && flat(seen.eye - eye).length() < 20.0
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
    /// The bot went through an opening since it last looked: its heading,
    /// leash and plan go with it. A path leading through that opening
    /// walks on from where it let out; any other is planned again.
    fn bot_crossed(&mut self, bot: OwnerId) {
        let Some(brain) = self.bots.brains.get_mut(&bot) else {
            return;
        };
        let seen = std::mem::replace(&mut brain.crossed, self.crossings.count());
        let Some(carry) = self
            .crossings
            .since(seen)
            .filter(|c| c.object == ObjectRef::Player(bot))
            .map(|c| c.carry)
            .reduce(|before, then| then * before)
        else {
            return;
        };
        brain.yaw = carried_yaw(&carry, brain.yaw);
        brain.leash = carry.transform_point3(brain.leash);
        brain.last_position = carry.transform_point3(brain.last_position);
        brain.stuck = 0;
        match brain.plan.iter().take(2).position(|w| w.through.is_some()) {
            Some(at) => {
                brain.plan.drain(..at);
                brain.plan[0].through = None;
            }
            None => {
                brain.plan.clear();
                brain.search = None;
                brain.settled = false;
            }
        }
    }
    /// Equip the first real weapon (not a building tool) in the inventory,
    /// unless it holds one already (the rules may have put one in its
    /// hand).
    fn bot_arm(&mut self, bot: OwnerId) -> Result<()> {
        let Some(actor) = self.weapons.actor(ActorId(bot)) else {
            return Ok(());
        };
        let real = |item: &Option<String>| {
            item.as_deref()
                .is_some_and(|id| !bri_weapons::CORE_TOOLS.contains(&id))
        };
        if actor
            .selected
            .and_then(|s| actor.inventory.get(s))
            .is_some_and(real)
        {
            return Ok(());
        }
        let weapon = actor.inventory.iter().position(real);
        if weapon.is_some() && actor.selected != weapon {
            let _ = self.equip_tool(bot, weapon);
        }
        Ok(())
    }
    /// The bot kinds as rules see them (`bot_kinds()`).
    pub(super) fn bot_kind_views(&self) -> Vec<bri_package_runtime::script::BotKindView> {
        self.bots
            .kinds
            .iter()
            .map(|k| bri_package_runtime::script::BotKindView {
                id: k.id.clone(),
                name: k.name.clone(),
                first_names: k.first_names.clone(),
            })
            .collect()
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
