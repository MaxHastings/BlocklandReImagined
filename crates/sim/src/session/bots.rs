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
use crate::bot_kind::{BotKind, Moves};
use crate::nav::{Body, Found, Ground, Nav, Search, Waypoint};
use behaviour::{Behaviour, Situation, choose};
use bri_content::passage::{Way, carried_yaw};
use bri_package_runtime::ops::ObjectRef;
use bri_weapons::ActorId;

mod behaviour;
mod claims;
#[path = "bots/combat.rs"]
mod hand_combat;
mod interactions;
mod objectives;
mod planning;
mod tactics;

/// Read-only brain evidence for headless diagnostics and playtest logs. This
/// is derived state, never an input that assigns decisions to a bot.
#[derive(Clone, Debug)]
pub struct BotThought {
    pub bot: OwnerId,
    pub behaviour: &'static str,
    pub visible: Option<OwnerId>,
    pub remembered: Option<BotEvidence>,
    pub task: Option<BotTask>,
    pub goal: Option<[f32; 3]>,
    pub next: Option<[f32; 3]>,
    pub path_steps: usize,
    pub searching: bool,
    pub objective: Option<BrickId>,
    pub objective_diagnostic: Option<&'static str>,
}
#[derive(Clone, Copy, Debug)]
pub struct BotEvidence {
    pub subject: OwnerId,
    pub position: [f32; 3],
    pub observed: u64,
    pub expires: u64,
}
#[derive(Clone, Copy, Debug)]
pub enum BotTask {
    Seat {
        vehicle: u64,
        seat: u8,
        subject: OwnerId,
        deadline: u64,
    },
    Push {
        vehicle: u64,
        subject: OwnerId,
        deadline: u64,
    },
}
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
/// Longest a bot carries what it holds toward open space before it throws
/// anyway.
const CARRY_TICKS: u64 = 720;
/// Shortest hold before the throw: it holds its catch up a moment.
const LIFT_TICKS: u64 = 90;
/// How long the throwing swing turns before it lets go.
const SWING_TICKS: u64 = 36;
/// After a throw, how long before it grabs again.
const REGRAB_TICKS: u64 = 120;
/// Farthest across an enemy above may be for a bot to fly to them.
const AIR_CHASE: f32 = 30.0;
/// Open space for a throw: sky this far up, room this far all round.
const OPEN_SKY: f32 = 16.0;
const OPEN_ROOM: f32 = 6.0;
/// How far round it the sky must be open too: where its catch swings.
const OPEN_SWING: f32 = 3.0;
/// The animation thread a hit with the body plays its action on, the arms'
/// (`playThread(2, activate2)`).
const MELEE_THREAD: u8 = 2;
/// Emotes a kind may strike: those that are only a look.
const BOT_EMOTES: [&str; 4] = ["hug", "love", "hate", "confusion"];
/// How much of a swimmer the water covers for it to swim rather than walk.
const SWIM_COVERAGE: f32 = 0.5;

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
    hurt: BTreeMap<OwnerId, Knowledge>,
    /// Warnings bots gave this tick (`BotKind::alerts_allies`), heard by
    /// their side once every bot has stepped.
    alerts: Vec<Alert>,
    /// Geometry observed once per tick; admission still reads live occupancy.
    objects: Vec<bri_vehicles::VehicleSnapshot>,
    claims: claims::Claims,
    interaction_budget: usize,
    objective_budget_tick: u64,
    objective_budget_used: bool,
    objective_cursor: Option<OwnerId>,
    objective_candidate: Option<OwnerId>,
    combat_budget: hand_combat::Budget,
}
/// A bot that saw an enemy, or was hurt, tells its side where.
struct Alert {
    from: OwnerId,
    /// Where the enemy was.
    knowledge: Knowledge,
    /// How far it is heard: the warner's sight.
    reach: f32,
}
/// Evidence keeps its subject and observation time when shared. No receiver
/// can extend it by relaying or look up an unseen subject's current transform.
#[derive(Clone, Copy, Debug)]
struct Knowledge {
    subject: OwnerId,
    at: Vec3,
    observed: u64,
    expires: u64,
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
    partial_route: bool,
    segment_anchor: Vec3,
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
    memory: Option<Knowledge>,
    error: (f32, f32),
    next_error: u64,
    fire_down: bool,
    /// What it is doing ([`behaviour`]).
    behaviour: Behaviour,
    objective: objectives::State,
    combat: hand_combat::State,
    native_combat_tick: Option<u64>,
    /// Carrying what it holds to throw it.
    carry: Option<Carry>,
    /// No grabbing before this tick (just threw).
    next_grab: u64,
    /// No hit with its body before this tick (`BotKind::melee`).
    next_bite: u64,
    /// Its kind's emote is struck for this life.
    posed: bool,
    /// Ticks in a row a swimmer spent out of water in a mini-game.
    dry: u32,
    /// The kind its brick made, while a bite has turned it into another
    /// (`BotMelee::converts_below`); it comes back as this one.
    born: Option<BotKind>,
    next_interaction: u64,
    object_cursor: usize,
    push_contact: Option<(u64, u64)>,
    push_anchor: Option<(u64, Vec3)>,
    vehicle_stuck: u32,
    vehicle_anchor: Option<Vec3>,
    vehicle_since: Option<(u64, u64)>,
}
/// An enemy up where a bot flies to them ([`Session::air_chase`]).
#[derive(Clone, Copy, Debug)]
struct AirChase {
    /// Where they stand.
    to: Vec3,
    /// Something just over the bot's head.
    roofed: bool,
}
/// A bot holding something with a tool that holds (the Gravity Gun, or
/// any tool whose trigger reaches and holds: `reach`, `hold`) carries it
/// out into open space and flings it with a swing of its aim.
#[derive(Clone, Copy, Debug)]
struct Carry {
    /// Where it is open; `None` when it is open here (or nowhere near).
    to: Option<Vec3>,
    since: u64,
    /// When the throwing swing began.
    swing: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Goal {
    Wander(Vec3),
    Chase(Vec3),
    Search(Vec3),
    /// Carrying what it holds to open space.
    Carry(Vec3),
    Interact(Vec3),
    Objective(Vec3),
    Home,
}
impl Goal {
    fn point(self, home: Vec3) -> Vec3 {
        match self {
            Self::Wander(p)
            | Self::Chase(p)
            | Self::Search(p)
            | Self::Carry(p)
            | Self::Interact(p)
            | Self::Objective(p) => p,
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
            partial_route: false,
            segment_anchor: home,
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
            behaviour: Behaviour::default(),
            objective: objectives::State::default(),
            combat: hand_combat::State::default(),
            native_combat_tick: None,
            carry: None,
            next_grab: 0,
            next_bite: 0,
            posed: false,
            dry: 0,
            born: None,
            next_interaction: 0,
            object_cursor: 0,
            push_contact: None,
            push_anchor: None,
            vehicle_stuck: 0,
            vehicle_anchor: None,
            vehicle_since: None,
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
            self.partial_route = false;
        }
    }
    /// The goal of going after an enemy: standing its ground in its band
    /// (`fight`), after the enemy in sight, or to where one was. Whether it
    /// stands, and whether it gives ground (closer than `near`).
    fn pursue(&mut self, enemy: Option<Seen>, fight: bool, near: f32, feet: Vec3) -> (bool, bool) {
        match (enemy, self.memory) {
            (Some(seen), _) if fight => {
                self.set_goal(None);
                (true, flat(seen.feet - feet).length() < near)
            }
            (Some(seen), _) => {
                // The chase heads for where it really stands, and the path
                // finds the way there.
                let moved_on = match self.goal {
                    Some(Goal::Chase(p)) => p.distance(seen.real) > 2.5,
                    _ => true,
                };
                if moved_on {
                    self.set_goal(Some(Goal::Chase(seen.real)));
                }
                (false, false)
            }
            (None, Some(Knowledge { at, .. })) => {
                if flat(at - feet).length() > 1.5 {
                    if self.goal != Some(Goal::Search(at)) {
                        self.set_goal(Some(Goal::Search(at)));
                    }
                    (false, false)
                } else {
                    // Got there: look around until it forgets.
                    self.set_goal(None);
                    (true, false)
                }
            }
            (None, None) => (false, false),
        }
    }
    /// The goal of strolling: somewhere near its brick, or for a rules bot
    /// (no brick to return to) near wherever it is (Slayer's bots,
    /// `hReturnToSpawn` off), now and then.
    fn wander(&mut self, feet: Vec3, tick: u64, swimming: bool) {
        if !matches!(self.goal, None | Some(Goal::Wander(_))) {
            // A fight or a carry is over.
            self.set_goal(None);
        }
        if self.brick.is_none() {
            self.home = feet;
        }
        if self.goal.is_none() && tick >= self.next_wander {
            let angle = self.random() * std::f32::consts::TAU;
            let radius = self.random() * self.kind.wander_radius;
            let around = if self.brick.is_none() {
                feet
            } else {
                self.home
            };
            // A swimmer roams up and down too (the water bounds it).
            let rise = if swimming {
                (self.random() * 2.0 - 1.0) * self.kind.wander_radius * 0.5
            } else {
                0.0
            };
            let point = around + Vec3::new(angle.sin() * radius, rise, angle.cos() * radius);
            self.set_goal(Some(Goal::Wander(point)));
            self.next_wander = tick + 240 + (self.random() * 480.0) as u64;
        }
    }
}
/// What the held weapon wants: how far it reaches and how its shots fly.
#[derive(Clone, Copy, Debug)]
struct Weapon {
    melee: bool,
    /// Its trigger is held down on target rather than tapped (`BotUse`).
    hold: bool,
    /// It fires on letting go after holding (`Image::charges`): held down
    /// while it charges, let go once letting go fires.
    charge: bool,
    /// Closest it is used from, when its data says (`BotUse::near`).
    near: Option<f32>,
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
            let far = (self.reach * 0.7).clamp(6.0, 40.0).min(self.reach);
            let near = self
                .near
                .unwrap_or((self.splash + 3.0).max(5.0))
                .min(far * 0.75);
            (near, far.max(near + 4.0).min(self.reach))
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
    pub(super) fn note_hurt(&mut self, bot: OwnerId, source: Option<(OwnerId, Vec3)>, tick: u64) {
        if let Some(source) = source.filter(|(s, _)| *s != bot)
            && self.brains.contains_key(&bot)
        {
            self.hurt.insert(
                bot,
                Knowledge {
                    subject: source.0,
                    at: source.1,
                    observed: tick,
                    expires: tick
                        .saturating_add((self.brains[&bot].kind.memory_seconds * 120.0) as u64),
                },
            );
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
            .is_some_and(|b| {
                let born = b.born.as_ref().unwrap_or(&b.kind);
                kind.as_ref().is_some_and(|k| k.id == born.id)
            });
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
            if let Err(e) = self.embody_bot(bot, &kind) {
                self.drop_bot(bot)?;
                self.notify(
                    builder,
                    Notice::Center {
                        text: format!("\u{E000}{e}"),
                        seconds: 3.0,
                    },
                );
                return Ok(());
            }
            self.bots.by_brick.insert(brick_id, bot);
            self.bots
                .brains
                .insert(bot, Brain::new(Some(brick_id), kind, home, bot, crossed));
            self.weapons.set_bot(bri_weapons::ActorId(bot), true)?;
        }
        Ok(())
    }
    /// A new bot takes its kind's body, and keeps it through respawns and
    /// mini-games as a body an Add-On chose does (`set_archetype`).
    fn embody_bot(&mut self, bot: OwnerId, kind: &BotKind) -> Result<()> {
        let body = match &kind.body {
            Some(body) => Some(self.archetypes.find(body).with_context(|| {
                format!("{}: its body {body} is in no enabled Add-On", kind.name)
            })?),
            None => None,
        };
        let pack = self.avatar_catalog.as_ref();
        let mut avatar = pack.map(|c| c.defaults.clone());
        if let (Some(look), Some(avatar)) = (&kind.look, avatar.as_mut()) {
            UniformParts {
                parts: look.parts.clone(),
                face: look.face.clone(),
                decal: look.decal.clone(),
            }
            .dress(avatar, pack);
            avatar.colors.extend(look.colors.clone());
        }
        let peer = self.peers.get_mut(&bot).context("No such bot")?;
        peer.avatar = avatar;
        peer.package_archetype = body;
        let body = body.unwrap_or_else(|| crate::player_types::PlayerType::Standard.archetype());
        self.set_player_archetype(bot, body)
    }
    /// Every bot kind's body is an archetype the enabled Add-Ons provide.
    pub(super) fn check_bot_bodies(&self) -> Result<()> {
        for kind in &self.bots.kinds {
            ensure!(
                kind.emote
                    .as_deref()
                    .is_none_or(|e| BOT_EMOTES.contains(&e)),
                "Bot {}: its emote is one of {}",
                kind.id,
                BOT_EMOTES.join(", ")
            );
            if let Some(body) = &kind.body {
                ensure!(
                    self.archetypes.find(body).is_some(),
                    "Bot {}: its body {body} is in no enabled Add-On",
                    kind.id
                );
            }
        }
        Ok(())
    }
    /// A bot leaves the server, whatever made it.
    fn drop_bot(&mut self, bot: OwnerId) -> Result<()> {
        self.bots.claims.release_owner(bot);
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
        if let Err(e) = self.embody_bot(bot, &kind) {
            self.drop_bot(bot)?;
            return Err(e);
        }
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
    /// Trigger release is a shot for a charged image. Abandoning hostile
    /// intent instead clears queued input and restarts the held image safely.
    fn abort_bot_hand_charge(&mut self, bot: OwnerId) -> Result<bool> {
        if !self
            .weapons
            .image_state(ActorId(bot), 0)
            .is_some_and(|(i, _)| i.charges())
        {
            return Ok(false);
        }
        self.release_trigger(bot)?;
        self.weapons.cancel_charge(ActorId(bot))
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
        if rest {
            self.bots.claims.release_owner(bot);
            if self.seated(bot) {
                self.dismount_vehicle(bot)?;
            }
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
    pub fn bot_thoughts(&self) -> Vec<BotThought> {
        let tick = self.simulation.state().tick;
        self.bots
            .brains
            .iter()
            .map(|(bot, b)| BotThought {
                bot: *bot,
                behaviour: b.behaviour.name(),
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
                goal: b.goal.map(|g| g.point(b.leash).to_array()),
                next: b.plan.first().map(|p| p.feet.to_array()),
                path_steps: b.plan.len(),
                searching: b.search.is_some(),
                objective: b.objective.step.as_ref().map(|s| s.brick),
                objective_diagnostic: b.objective.diagnostic,
            })
            .collect()
    }

    /// Whether `bot` treats `other` as an enemy: anyone it may hurt, except
    /// bots of the same builder, who are on its side. A rules bot plays
    /// as a member, so its game alone says who is on its side (Slayer's
    /// `checkHoleBotTeams`).
    fn bot_enemy(&self, bot: OwnerId, kind: &BotKind, other: OwnerId) -> bool {
        if other == bot
            || self.bot_allies(bot, other)
            || !self.peers.get(&other).is_some_and(|p| p.combat.alive)
        {
            return false;
        }
        if self.bots.is_brick_bot(bot)
            && self.bots.is_brick_bot(other)
            && (self.bot_allies(bot, other)
                || !kind.fights_bots
                    && kind.side.is_none()
                    && self
                        .bots
                        .brains
                        .get(&other)
                        .is_some_and(|b| b.kind.side.is_none()))
        {
            return false;
        }
        self.can_damage_player(bot, other, false)
    }
    /// Whether two brick bots are on one side: one side (Bot_Hole's
    /// `hType`) never fights itself and fights every other; bots of no side
    /// side with their builder.
    fn bot_allies(&self, bot: OwnerId, other: OwnerId) -> bool {
        if let (Some(a), Some(b)) = (self.peers.get(&bot), self.peers.get(&other))
            && bot != other
            && b.combat.alive
            && self.minigames.allied(a.combat.player, b.combat.player)
        {
            return true;
        }
        if !self.bots.is_brick_bot(bot) || !self.bots.is_brick_bot(other) {
            return false;
        }
        if self.game_of(bot) != self.game_of(other) {
            return false;
        }
        let side = |o: OwnerId| {
            self.bots
                .brains
                .get(&o)
                .and_then(|b| b.kind.side.as_deref())
        };
        match (side(bot), side(other)) {
            (None, None) => self.bot_brick_owner(other) == self.bot_brick_owner(bot),
            (mine, theirs) => mine == theirs,
        }
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
    /// Where a bot flies to reach its enemy (in sight, or last seen) well
    /// above it, and how the air between lies.
    fn air_chase(
        &self,
        brain: &Brain,
        sight: &Sight,
        feet: Vec3,
        eye: Vec3,
        grounded: bool,
    ) -> Option<AirChase> {
        let to = sight
            .target
            .map(|seen| seen.feet)
            .or(brain.memory.map(|k| k.at))?;
        // Taking off for someone well above; once up, until it is by them.
        let above = to.y - feet.y;
        let across = flat(to - feet).length();
        let landed = !grounded && above < 0.5 && across < 1.0;
        if across > AIR_CHASE || above < if grounded { 2.5 } else { -3.0 } || landed {
            return None;
        }
        let clear = |from: Vec3, d: Vec3, length: f32| {
            matches!(self.simulation.target(from, d, length), Ok(None))
        };
        Some(AirChase {
            to,
            roofed: !clear(eye, Vec3::Y, 3.0),
        })
    }
    /// Whether a body standing at `feet` has open sky above and room all
    /// round, to fling something.
    fn open_at(&self, feet: Vec3) -> bool {
        let clear = |from: Vec3, d: Vec3, length: f32| {
            matches!(self.simulation.target(from, d, length), Ok(None))
        };
        let chest = feet + Vec3::Y * 1.5;
        // Sky over it and over where its catch swings round it.
        clear(chest, Vec3::Y, OPEN_SKY)
            && (0..8).all(|i| {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let out = Vec3::new(a.sin(), 0.0, a.cos());
                clear(chest, out, OPEN_ROOM) && clear(chest + out * OPEN_SWING, Vec3::Y, OPEN_SKY)
            })
    }
    /// The nearest open place around `feet` to throw from: `None` when it
    /// is open here, or nowhere near.
    fn open_spot(&self, feet: Vec3) -> Option<Vec3> {
        if self.open_at(feet) {
            return None;
        }
        for ring in [6.0, 12.0, 18.0, 24.0] {
            for i in 0..12 {
                let a = i as f32 * std::f32::consts::TAU / 12.0;
                let p = feet + Vec3::new(a.sin(), 0.0, a.cos()) * ring;
                // The floor there, looked for from waist height so a roof
                // overhead is not taken for it.
                let stand = match self.simulation.target(p + Vec3::Y * 2.0, -Vec3::Y, 8.0) {
                    Ok(Some(hit)) if hit.normal.y > 0.7 => hit.position + Vec3::Y * 0.05,
                    Ok(Some(_)) => continue,
                    _ => p,
                };
                if self.open_at(stand) {
                    return Some(stand);
                }
            }
        }
        None
    }
    /// The held weapon's reach and flight.
    fn bot_vehicle_weapon(&self, bot: OwnerId) -> Option<Weapon> {
        if let Some((vehicle, seat)) = self.mounted(bot)
            && let Some(w) = &self.vehicles.world
            && w.weapon_available(bri_vehicles::VehicleId(vehicle))
            && let Some(d) = w.definition_of(bri_vehicles::VehicleId(vehicle))
            && d.weapon_seat() == Some(usize::from(seat))
            && let Some(gun) = &d.weapon
            && let Some(p) = self.weapons.pack.projectiles.get(&gun.projectile)
            && let Some(v) = self.bots.objects.iter().find(|v| v.id.0 == vehicle)
        {
            let speed = gun.speed * f32::from(gun.charge_steps.max(1)) * v.scale;
            return Some(Weapon {
                melee: false,
                hold: false,
                charge: gun.charge_ticks > 0,
                near: None,
                reach: speed * p.lifetime_ticks as f32 * TICK,
                speed,
                fall: bri_weapons::runtime::fall_per_tick(p) * 120.0,
                splash: p.explosion.radius,
            });
        }
        None
    }
    fn bot_weapon(&self, bot: OwnerId) -> Option<Weapon> {
        if let Some(gun) = self.bot_vehicle_weapon(bot) {
            return Some(gun);
        }
        let (image, _) = self.weapons.image_state(ActorId(bot), 0)?;
        let using = image.bot.unwrap_or_default();
        let hold = using.fire == bri_weapons::BotFire::Hold;
        if let Some(ray) = image.shot.as_ref().and_then(|s| s.hitscan.as_ref()) {
            return Some(Weapon {
                melee: false,
                hold,
                charge: image.charges(),
                near: using.near,
                reach: using.reach.unwrap_or(ray.range),
                speed: 0.0,
                fall: 0.0,
                splash: 0.0,
            });
        }
        let projectile = image
            .projectile
            .as_ref()
            .and_then(|p| self.weapons.pack.projectiles.get(p));
        let Some(p) = projectile else {
            let reach = using.reach.unwrap_or(3.0);
            return Some(Weapon {
                melee: reach < 6.0,
                hold,
                charge: image.charges(),
                near: using.near,
                reach,
                speed: 0.0,
                fall: 0.0,
                splash: 0.0,
            });
        };
        let reach = using
            .reach
            .unwrap_or(p.speed * p.lifetime_ticks as f32 * TICK);
        Some(Weapon {
            melee: image.melee || reach < 6.0,
            hold,
            charge: image.charges(),
            near: using.near,
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
        self.observe_bot_objects();
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
        self.bots.objective_candidate = objectives::next_turn(
            self.bots.objective_cursor,
            self.bots
                .brains
                .iter()
                .filter(|(bot, brain)| {
                    brain.objective.ready(tick)
                        && brain
                            .kind
                            .behaviours
                            .get("objective")
                            .copied()
                            .unwrap_or(0.0)
                            > 0.0
                        && self.peers.get(bot).is_some_and(|p| p.combat.alive)
                })
                .map(|(bot, _)| *bot),
        );
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
        self.hear_alerts(tick);
        Ok(())
    }
    /// Bots of a warner's side within its reach that have nothing better
    /// to go on remember where the enemy was, and go and look (Bot_Hole's
    /// `hAlertOtherBots`).
    fn hear_alerts(&mut self, tick: u64) {
        for alert in std::mem::take(&mut self.bots.alerts) {
            let Some(from) = self
                .peers
                .get(&alert.from)
                .map(|p| Vec3::from(p.player.state().feet))
            else {
                continue;
            };
            let heard: Vec<OwnerId> = self
                .bots
                .brains
                .iter()
                .filter(|(o, b)| {
                    **o != alert.from
                        && !b.resting
                        && b.target.is_none()
                        && b.memory
                            .is_none_or(|old| old.observed < alert.knowledge.observed)
                        && tick < alert.knowledge.expires
                        && self.bot_enemy(**o, &b.kind, alert.knowledge.subject)
                        && self.bot_allies(**o, alert.from)
                        && self.peers.get(o).is_some_and(|p| {
                            p.combat.alive
                                && Vec3::from(p.player.state().feet).distance(from) <= alert.reach
                        })
                })
                .map(|(o, _)| *o)
                .collect();
            for bot in heard {
                let brain = self.bots.brains.get_mut(&bot).unwrap();
                let expires = alert.knowledge.expires.min(
                    alert
                        .knowledge
                        .observed
                        .saturating_add((brain.kind.memory_seconds * 120.0) as u64),
                );
                if tick < expires {
                    brain.memory = Some(Knowledge {
                        expires,
                        ..alert.knowledge
                    });
                }
            }
        }
    }
    fn step_bot(&mut self, bot: OwnerId, tick: u64) -> Result<()> {
        self.promote_bot_seat(bot)?;
        let Some(peer) = self.peers.get(&bot) else {
            return Ok(());
        };
        if !peer.combat.alive {
            // A brick's bot comes back a second after it may; a rules bot
            // as soon as its game lets it (Slayer's bot respawn time).
            let wait = if self.bots.by_rules.contains_key(&bot) {
                0
            } else {
                120
            };
            if tick >= peer.combat.respawn_tick + wait {
                // A bot a bite turned comes back as its own kind.
                if let Some(born) = self.bots.brains.get_mut(&bot).and_then(|b| b.born.take()) {
                    self.bots.brains.get_mut(&bot).unwrap().kind = born.clone();
                    self.embody_bot(bot, &born)?;
                }
                let _ = self.request_respawn(bot);
            }
            if let Some(brain) = self.bots.brains.get_mut(&bot) {
                brain.set_goal(None);
                brain.target = None;
                brain.memory = None;
                brain.posed = false;
                brain.objective = objectives::State::default();
                brain.dry = 0;
                brain.rehome = brain.brick.is_none();
                brain.leash = brain.home;
                self.bots.claims.release_owner(bot);
                brain.vehicle_since = None;
                brain.vehicle_stuck = 0;
                brain.vehicle_anchor = None;
                brain.fire_down = false;
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
                jump: self.vehicles.is_mounted(bot),
                ..Default::default()
            };
            let release = std::mem::take(&mut brain.fire_down);
            self.movement(bot, sequence, input)?;
            self.vehicles.set_fire(bot, false);
            if release && !self.abort_bot_hand_charge(bot)? {
                let _ = self.weapon_trigger(bot, false, Vec3::ZERO, false);
            }
            return Ok(());
        }
        // Ridden by a player who steers it, or carried by one
        // (`mountObject`): its brain rests.
        if self.riding.driver_of(bot).is_some() || self.riding.is_riding(bot) {
            return Ok(());
        }
        if !self.seated(bot) {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.vehicle_since = None;
            brain.vehicle_anchor = None;
            brain.vehicle_stuck = 0;
        }
        let state = peer.player.state().clone();
        let own_feet = Vec3::from(state.feet);
        let eye = self
            .bot_weapon_origin(bot)
            .unwrap_or_else(|| peer.player.eye());
        let can_fly =
            peer.player.tuning().can_jet && state.energy >= peer.player.tuning().min_jet_energy;
        let driving = self.bot_vehicle_body(bot);
        let (feet, body) = driving.unwrap_or((own_feet, Body::of(peer.player.tuning(), 1.0)));
        // A swimmer in water: how tall it is, to keep it under.
        let swim = (self.bots.brains[&bot].kind.moves == Moves::Swim)
            .then(|| crate::water::body_height(&state, peer.player.tuning()) * state.scale)
            .filter(|height| {
                self.simulation
                    .liquid_at(state.feet, *height)
                    .is_some_and(|(_, covered)| covered >= SWIM_COVERAGE)
            });
        // A swimmer out of water in a mini-game lasts only so long (the
        // Shark's `hFishOutOfWater`).
        let height = crate::water::body_height(&state, peer.player.tuning()) * state.scale;
        let wet = self.simulation.liquid_at(state.feet, height).is_some();
        let gasping = self.bots.brains[&bot]
            .kind
            .out_of_water_seconds
            .filter(|_| !wet && self.game_of(bot).is_some());
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        brain.dry = if gasping.is_some() { brain.dry + 1 } else { 0 };
        if gasping.is_some_and(|seconds| brain.dry as f32 >= seconds * 120.0) {
            brain.dry = 0;
            return self.kill(bot, None, combat::DamageKind::Suicide);
        }
        // Its kind's emote, once each life (a zombie's arms out ahead).
        if !brain.posed {
            brain.posed = true;
            if let Some(name) = brain.kind.emote.clone() {
                self.emote_cue(
                    tick,
                    crate::presentation::CueKind::Emote { actor: bot, name },
                    state.feet,
                );
            }
        }
        self.bot_crossed(bot);
        let brain = &self.bots.brains[&bot];
        let sight = self.bot_sight(bot, brain, eye);
        // A crossing immediately after direct sight can carry that last
        // observation through the opening. It never reads the hidden body.
        let followed = brain
            .memory
            .filter(|k| {
                Some(k.subject) == brain.target
                    && sight.target.is_none()
                    && tick < k.expires
                    && self.bot_enemy(bot, &brain.kind, k.subject)
            })
            .and_then(|k| {
                self.crossings
                    .last_of(ObjectRef::Player(k.subject))
                    .filter(|c| tick.saturating_sub(c.tick) <= 2 && c.tick >= k.observed)
                    .map(|c| Knowledge {
                        at: c.carry.transform_point3(k.at),
                        ..k
                    })
            });
        // Inventory capabilities are grounded only in current sight. The
        // desired path uses a shared budget; actual firing is checked after
        // movement against the live launch frame in step_weapons.
        let native = if let Some(seen) = sight.target {
            let mut combat = std::mem::take(&mut self.bots.brains.get_mut(&bot).unwrap().combat);
            let mut budget = std::mem::take(&mut self.bots.combat_budget);
            budget.begin_tick(tick);
            let decision = hand_combat::choose(self, bot, seen, tick, &mut combat, &mut budget);
            self.bots.brains.get_mut(&bot).unwrap().combat = combat;
            self.bots.combat_budget = budget;
            decision
        } else {
            hand_combat::Decision::Unsupported
        };
        let native_choice = match &native {
            hand_combat::Decision::Ready(c) | hand_combat::Decision::Charging(c) => Some(*c),
            _ => None,
        };
        let native_gate = !matches!(native, hand_combat::Decision::Unsupported)
            || (sight.target.is_none() && self.bots.brains[&bot].native_combat_tick.is_some());
        self.bots.brains.get_mut(&bot).unwrap().native_combat_tick = native_gate.then_some(tick);
        // Empty-handed, a kind that hits with its body fights with that.
        let held = native_choice
            .map(|c| c.weapon)
            .or_else(|| {
                (!matches!(native, hand_combat::Decision::Unsupported))
                    .then(|| self.bots.brains[&bot].combat.movement_hint())
                    .flatten()
            })
            .or_else(|| self.bot_weapon(bot));
        let bite = held
            .is_none()
            .then(|| self.bots.brains[&bot].kind.melee.clone())
            .flatten();
        let weapon = held.or(bite.as_ref().map(|m| Weapon {
            melee: true,
            hold: false,
            charge: false,
            near: None,
            reach: m.reach,
            speed: 0.0,
            fall: 0.0,
            splash: 0.0,
        }));
        let hurt_by = self.bots.hurt.remove(&bot).filter(|k| {
            tick < k.expires && self.bot_enemy(bot, &self.bots.brains[&bot].kind, k.subject)
        });
        if self.bots.brains[&bot].memory.is_some_and(|k| {
            tick >= k.expires || !self.bot_enemy(bot, &self.bots.brains[&bot].kind, k.subject)
        }) {
            self.bots.brains.get_mut(&bot).unwrap().memory = None;
        }
        // Holding something with its tool: carry it to open space to throw.
        let holding = self.held_by(bot).is_some();
        let grabbing = holding || self.is_reaching(bot);
        let carry_to =
            (holding && self.bots.brains[&bot].carry.is_none()).then(|| self.open_spot(feet));
        let air = (can_fly && !self.seated(bot))
            .then(|| self.air_chase(&self.bots.brains[&bot], &sight, feet, eye, state.grounded))
            .flatten();
        let interaction_enemy = sight
            .target
            .map(|s| Knowledge {
                subject: s.owner,
                at: s.real,
                observed: tick,
                expires: tick + (self.bots.brains[&bot].kind.memory_seconds * 120.0) as u64,
            })
            .or(self.bots.brains[&bot].memory)
            .or(hurt_by)
            .filter(|_| {
                flat(feet - self.bots.brains[&bot].leash).length()
                    <= self.bots.brains[&bot].kind.chase_radius
            });
        let opportunity = self.bot_interaction(bot, interaction_enemy, tick);
        let objective = self.bot_objective(bot, tick);
        let vehicle_weapon = self.bot_vehicle_weapon(bot).is_some();
        let crew_ready = self.bot_crew_ready(bot, tick);
        let attack_clear = sight.target.is_none_or(|seen| {
            self.bot_fire_clear(
                bot,
                eye,
                seen.eye - Vec3::Y * 0.5,
                weapon.map_or(0.0, |w| w.splash),
            )
        });
        let mounted_charging = self
            .mounted(bot)
            .and_then(|(id, _)| self.bots.objects.iter().find(|v| v.id.0 == id))
            .is_some_and(|v| v.charge > 0);
        let charged_ready = self
            .mounted(bot)
            .and_then(|(id, _)| self.bots.objects.iter().find(|v| v.id.0 == id))
            .is_some_and(|v| {
                self.vehicles
                    .world
                    .as_ref()
                    .and_then(|w| w.definition(&v.definition))
                    .and_then(|d| d.weapon.as_ref())
                    .is_some_and(|g| g.charge_ticks > 0 && v.charge >= g.charge_steps)
            });
        let target_velocity = sight.target.map_or(Vec3::ZERO, |seen| {
            self.peers.get(&seen.owner).map_or(Vec3::ZERO, |p| {
                seen.way.seen_vector(Vec3::from(p.player.state().velocity))
            })
        });

        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let kind = brain.kind.clone();
        if !holding {
            brain.carry = None;
        } else if let Some(to) = carry_to {
            brain.carry = Some(Carry {
                to,
                since: tick,
                swing: None,
            });
        }
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
        let mut warn = None;
        match sight.target {
            Some(seen) => {
                if brain.target != Some(seen.owner) {
                    brain.target = Some(seen.owner);
                    brain.seen_since = tick;
                }
                let knowledge = Knowledge {
                    subject: seen.owner,
                    at: seen.real,
                    observed: tick,
                    expires: tick + memory_ticks,
                };
                brain.memory = Some(knowledge);
                if tick.is_multiple_of(30) || brain.seen_since == tick {
                    warn = Some(knowledge);
                }
            }
            None => {
                brain.target = None;
                if let Some(k) = hurt_by.or(followed) {
                    if brain.memory.is_none_or(|old| old.observed <= k.observed) {
                        brain.memory = Some(k);
                    }
                    warn = hurt_by;
                }
            }
        }
        if kind.alerts_allies
            && let Some(knowledge) = warn
        {
            self.bots.alerts.push(Alert {
                from: bot,
                knowledge,
                reach: kind.sight,
            });
        }
        let away = flat(feet - brain.leash).length();
        if away > kind.chase_radius {
            brain.memory = None;
            brain.target = None;
        }

        // Behaviour: the most urgent that applies.
        let enemy = sight.target.filter(|_| away <= kind.chase_radius);
        let (near, far) = if driving.is_some() && !vehicle_weapon {
            (0.0, 0.0)
        } else {
            weapon.map_or((2.0, 3.0), |w| w.band())
        };
        let walks_up = |to: Vec3| {
            brain
                .plan
                .last()
                .is_some_and(|w| w.feet.y > to.y - body.step - 0.5)
        };
        let situation = Situation {
            holding,
            fly: swim.is_none() && air.is_some_and(|a| !walks_up(a.to)),
            interaction: opportunity.map_or(0.0, |o| o.utility),
            objective: objective.is_some(),
            // A swimmer reaches any depth: only how far counts.
            enemy: enemy.map(|seen| match swim {
                Some(_) => (seen.feet.distance(feet), 0.0),
                None => (flat(seen.feet - feet).length(), seen.feet.y - feet.y),
            }),
            far,
            step: body.step,
            remembers: brain.memory.is_some(),
            strayed: brain.brick.is_some() && away > kind.wander_radius + 4.0,
            home: brain.goal != Some(Goal::Home),
        };
        let behaviour = choose(brain.behaviour, &situation, |b| {
            kind.behaviours.get(b.name()).copied().unwrap_or(
                if matches!(b, Behaviour::Interact | Behaviour::Objective) {
                    0.0
                } else {
                    1.0
                },
            )
        });
        brain.behaviour = behaviour;
        if behaviour != Behaviour::Interact {
            self.bots.claims.release_owner(bot);
        }

        // Goal.
        let (hold, back_off) = match behaviour {
            Behaviour::Interact => {
                if let Some(o) = opportunity
                    && !matches!(brain.goal, Some(Goal::Interact(p)) if p.distance(o.point) < 0.2)
                {
                    brain.set_goal(Some(Goal::Interact(o.point)));
                }
                (false, false)
            }
            Behaviour::Objective => {
                if let Some(step) = objective.as_ref() {
                    brain.set_goal(if step.waiting {
                        None
                    } else {
                        Some(Goal::Objective(step.point))
                    });
                }
                (false, false)
            }
            Behaviour::Carry => {
                brain.set_goal(brain.carry.and_then(|c| c.to).map(Goal::Carry));
                (false, false)
            }
            Behaviour::Fight => brain.pursue(enemy, true, near, feet),
            Behaviour::Chase | Behaviour::Search => brain.pursue(enemy, false, near, feet),
            // Flying goes after them as walking would, so its path tells
            // when a walk leads up after all.
            Behaviour::Fly => {
                let fight = enemy.is_some_and(|seen| {
                    flat(seen.feet - feet).length() <= far
                        && (seen.feet.y - feet.y).abs() <= body.step + 1.0
                });
                brain.pursue(enemy, fight, near, feet)
            }
            Behaviour::Return => {
                brain.set_goal(Some(Goal::Home));
                (false, false)
            }
            Behaviour::Wander => {
                brain.wander(feet, tick, swim.is_some());
                (false, false)
            }
        };

        // Path.
        let home = brain.home;
        let mut wanted = None;
        if let Some(height) = swim {
            // In water it swims straight there, keeping to the water.
            brain.plan.clear();
            brain.search = None;
            if let Some(goal) = brain.goal
                && let Some((water, _)) = self.simulation.liquid_at(state.feet, height)
            {
                let to = crate::water::swim_point(water, goal.point(home), height);
                if to.distance(feet) < 0.8 {
                    brain.settled = true;
                    if matches!(brain.goal, Some(Goal::Wander(_) | Goal::Home)) {
                        brain.goal = None;
                    }
                } else {
                    wanted = Some(Waypoint {
                        feet: to,
                        jump: false,
                        through: None,
                        crouch: false,
                    });
                }
            }
        } else if let Some(goal) = brain.goal {
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
                        Found::Path(path) if !path.is_empty() => {
                            brain.partial_route = false;
                            brain.plan = path;
                        }
                        Found::Partial(path) if !path.is_empty() => {
                            brain.partial_route = true;
                            brain.segment_anchor = feet;
                            brain.plan = path;
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
                if next.through.is_none()
                    && flat(d).length() < if body.conservative { 1.2 } else { 0.4 }
                    && d.y.abs() < body.step + 0.5
                {
                    brain.plan.remove(0);
                    brain.stuck = 0;
                } else {
                    break;
                }
            }
            if brain.plan.is_empty() && brain.search.is_none() {
                // Walked the whole plan: arrived, or as near as it goes.
                if matches!(brain.goal, Some(Goal::Objective(_))) && brain.partial_route {
                    brain.partial_route = false;
                    if feet.distance(brain.segment_anchor) > 0.5 {
                        brain.settled = false; // Continue a bounded, advancing segment.
                    } else {
                        brain.settled = true;
                        brain
                            .objective
                            .fail(tick, "objective navigation made no progress");
                    }
                } else {
                    brain.settled = true;
                }
                if matches!(brain.goal, Some(Goal::Wander(_) | Goal::Home)) {
                    brain.goal = None;
                }
            }
            wanted = brain.plan.first().copied();
            // A grid route ends at a cell, not necessarily at the authored
            // interaction point. Finish a nearby approach with the ordinary
            // motor; the action's physical reach decides when it succeeds.
            if wanted.is_none()
                && brain.search.is_none()
                && matches!(goal, Goal::Interact(_) | Goal::Objective(_))
                && flat(point - feet).length() < 1.0
                && flat(point - feet).length() > 0.1
                && (point.y - feet.y).abs() < body.step + 0.5
            {
                wanted = Some(Waypoint {
                    feet: point,
                    jump: false,
                    through: None,
                    crouch: false,
                });
            }
        }
        let pushing = if behaviour == Behaviour::Interact {
            self.act_bot_interaction(bot, interaction_enemy.map(|k| k.at), tick)?
        } else {
            None
        };
        let walk_direction =
            wanted.map(|p| self.bot_walk_direction(bot, flat(p.feet - feet).normalize_or_zero()));
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        // Carried there (or as near as it gets, or long enough): swing,
        // after holding it up a moment.
        if let Some(carry) = brain.carry.as_mut()
            && carry.swing.is_none()
            && tick >= carry.since + LIFT_TICKS
            && (carry.to.is_none_or(|to| flat(to - feet).length() < 0.6)
                || brain.settled
                || tick >= carry.since + CARRY_TICKS)
        {
            carry.swing = Some(tick);
        }
        if brain.carry.is_some_and(|c| c.swing.is_some()) {
            wanted = None;
        }

        // Aim: at the enemy, or where it walks.
        let mut aim_yaw = brain.yaw;
        let mut aim_pitch = 0.0;
        let mut fire = false;
        let mut step = kind.turn_degrees.to_radians() * TICK;
        if behaviour == Behaviour::Carry
            && let Some(carry) = brain.carry
        {
            // Hold on while carrying; the swing turns as fast as it can,
            // rising, and lets go while still turning, so it flings.
            fire = true;
            match carry.swing {
                Some(start) => {
                    aim_yaw = wrap(brain.yaw + 1.0);
                    aim_pitch = 0.6;
                    step *= 2.0;
                    fire = tick < start + SWING_TICKS;
                    if !fire {
                        brain.next_grab = tick + REGRAB_TICKS;
                    }
                }
                None => {
                    if let Some(next) = wanted {
                        let d = flat(next.through.unwrap_or(next.feet) - feet);
                        if d.length() > 0.05 {
                            aim_yaw = yaw_to(d);
                        }
                    }
                    aim_pitch = 0.15;
                }
            }
        } else if behaviour == Behaviour::Objective {
            if let Some(objective) = objective.as_ref() {
                let delta = objective.aim - eye;
                aim_yaw = yaw_to(delta);
                aim_pitch = delta.y.atan2(flat(delta).length()).clamp(-1.5, 1.5);
            }
        } else if let Some(seen) = sight.target {
            let mut at = seen.eye - Vec3::Y * 0.5;
            if let Some(w) = weapon.filter(|w| !w.melee && w.speed > 0.0) {
                let time = at.distance(eye) / w.speed;
                at += target_velocity * time;
                at.y += 0.5 * w.fall * time * time;
            }
            let delta = native_choice.map_or(at - eye, |c| c.direction);
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
            let in_reach = weapon.is_some_and(|w| at.distance(eye) <= w.reach.max(1.0) * 1.1 + 0.5);
            fire = enemy.is_some()
                && (driving.is_none() || vehicle_weapon)
                && tick >= brain.seen_since + reaction
                && in_reach
                && attack_clear
                && crew_ready
                && wrap(aim_yaw - brain.yaw).abs() < 0.1
                && (aim_pitch - brain.pitch).abs() < 0.12;
            // A tool reaching to hold keeps its trigger down while it
            // watches its target, until it catches; none just after a
            // throw.
            if grabbing && enemy.is_some() && driving.is_none() {
                fire = true;
            }
            if tick < brain.next_grab {
                fire = false;
            }
        } else if let Some(next) = wanted {
            let d = flat(next.through.unwrap_or(next.feet) - feet);
            if d.length() > 0.05 {
                aim_yaw = yaw_to(d);
            }
        } else if hold {
            // Searching the spot: sweep the view.
            aim_yaw = wrap(brain.yaw + 0.8 * TICK * 2.0);
        }
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
            direction = if let Some(through) = next.through {
                flat(through - feet).normalize_or_zero()
            } else {
                walk_direction.unwrap_or(Vec3::ZERO)
            };
            input.jump = next.jump && flat(next.feet - feet).length() < 1.6 && state.grounded;
            // Into a crawlspace: crouch on the way in (the body stays down
            // until it has room to stand).
            input.crouch = next.crouch && flat(next.feet - feet).length() < 1.6;
        }
        if let Some(push) = pushing {
            direction = push;
        }
        match behaviour {
            // In its band: strafe so it is not a still target, and give
            // ground if too close.
            Behaviour::Fight if wanted.is_none() && hold => {
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
            // Up where no walk leads: jet up and over to them, as a player
            // would, and come down by them. Jets lift hardest straight up
            // and lean into the move, so it climbs first and then steers
            // over, gliding down on them.
            Behaviour::Fly => {
                if let Some(air) = air {
                    let toward = flat(air.to - feet);
                    let across = toward.length();
                    let toward = toward.normalize_or_zero();
                    if !air.roofed && kind.moves != Moves::Swim && weapon.is_some_and(|w| w.melee) {
                        let velocity = Vec3::from(state.velocity);
                        let closing_speed = flat(velocity).dot(toward);
                        if feet.y < air.to.y - 0.6
                            || (!state.grounded && velocity.y < -1.0 && feet.y < air.to.y + 0.8)
                        {
                            // Recover the target's altitude before spending more
                            // thrust on horizontal speed.
                            input.jet = true;
                            direction = Vec3::ZERO;
                        } else {
                            // Crouched jets trade lift for flat forward thrust.
                            // Use them only with altitude in hand; near the target,
                            // release thrust and walk against excess closing speed.
                            input.crouch = true;
                            if across > 2.5 && closing_speed < 3.0 {
                                input.jet = true;
                                direction = toward;
                            } else {
                                input.jet = false;
                                let desired = (across * 0.8).clamp(0.35, 1.5);
                                direction = if closing_speed > desired {
                                    -toward
                                } else {
                                    toward
                                };
                            }
                        }
                    } else {
                        // Keep the original flight approach for ranged bots
                        // and first clear the space below a roof.
                        input.jet = across > 1.0 || feet.y < air.to.y + 0.5;
                        direction = if air.roofed {
                            if across > 0.1 { -toward } else { forward }
                        } else if feet.y > air.to.y + 1.0 && across > 1.0 {
                            toward * (across / 3.0).min(1.0)
                        } else {
                            Vec3::ZERO
                        };
                    }
                }
            }
            _ => {}
        }
        if swim.is_some() {
            // Up and down as a swimmer does: jump rises, crouch dives.
            let to = match behaviour {
                Behaviour::Fight => enemy.map(|seen| seen.feet),
                _ => wanted.map(|w| w.feet),
            };
            if let Some(to) = to {
                input.jump = to.y > feet.y + 0.4;
                input.crouch = to.y < feet.y - 0.4;
            }
        }
        input.forward = direction.dot(forward).clamp(-1.0, 1.0);
        input.right = direction.dot(right).clamp(-1.0, 1.0);
        // Walking into something: hop, then plan again, then give up.
        let trying = input.forward != 0.0 || input.right != 0.0;
        if driving.is_none() && trying && moved < 0.01 {
            brain.stuck += 1;
        } else {
            brain.stuck = 0;
        }
        if driving.is_none() && pushing.is_none() && brain.stuck > 20 && brain.stuck % 40 < 5 {
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
                if behaviour == Behaviour::Objective {
                    brain.objective.fail(tick, "objective navigation blocked");
                }
                brain.replans = 0;
                brain.next_wander = tick + 120;
                if let Some(c) = self.bots.claims.owner_claim(bot, tick) {
                    self.bots.claims.fail(bot, c.resource, tick);
                }
            }
        }
        brain.sequence += 1;
        let sequence = brain.sequence;
        if forget && let Some((_, nav)) = self.bots.navs.iter_mut().find(|(b, _)| *b == body) {
            nav.invalidate(feet - Vec3::splat(1.0), feet + Vec3::splat(1.0), &body);
        }
        // Tap the trigger so semi-automatic weapons keep firing; a tool
        // that holds (as its data says, or reaching or holding now) keeps
        // it down.
        let charging = weapon.is_some_and(|w| w.charge);
        let held_down = grabbing || charging || weapon.is_some_and(|w| w.hold);
        let pulse = fire && !held_down && tick.is_multiple_of(40) && bite.is_none();
        // A charged weapon is held until letting go fires it, then let go.
        let pulse = pulse
            || fire
                && charging
                && (if vehicle_weapon {
                    charged_ready || !mounted_charging && tick.is_multiple_of(40)
                } else {
                    self.weapons
                        .image_state(ActorId(bot), 0)
                        .is_some_and(|(image, state)| image.fires_on_release(state))
                });
        let bites = bite.as_ref().filter(|_| fire && tick >= brain.next_bite);
        if let Some(m) = bites {
            brain.next_bite = tick + (m.seconds * 120.0).round() as u64;
        }
        // A body's hit pulls no trigger. Native hand charge releases are
        // authorized only on its fair Ready turn and then validated postmove.
        let mut fire = fire && bite.is_none();
        let mut desired_down = fire && !pulse;
        let last_down = brain.fire_down;
        let mut cancel_hand_charge = !fire && charging && !vehicle_weapon && last_down;
        if !matches!(native, hand_combat::Decision::Unsupported) {
            if let Some((image, image_state)) = self.weapons.image_state(ActorId(bot), 0) {
                let decision = hand_combat::trigger(
                    image,
                    image_state,
                    last_down,
                    fire && native_choice.is_some(),
                    matches!(native, hand_combat::Decision::Ready(_)),
                );
                desired_down = decision.down;
                cancel_hand_charge |= decision.abort_charge;
            } else {
                desired_down = false;
            }
            fire = desired_down;
        }
        let fire_changed = desired_down != last_down;
        brain.fire_down = desired_down;
        if cancel_hand_charge {
            self.abort_bot_hand_charge(bot)?;
        }
        if !fire && vehicle_weapon {
            self.vehicles.set_fire(bot, false);
            if let Some(w) = &mut self.vehicles.world {
                w.cancel_weapon_charge(bri_vehicles::OccupantId(bot));
            }
        }
        // Preserve the brain's world aim before a seat converts look into
        // steering or a relative passenger angle.
        let direction = Vec3::new(
            input.yaw.sin() * input.pitch.cos(),
            input.pitch.sin(),
            -input.yaw.cos() * input.pitch.cos(),
        );
        let input = self.bot_seated_input(bot, input, wanted, behaviour, tick)?;
        self.movement(bot, sequence, input)?;
        if let (Some(m), Some(seen)) = (bites, sight.target) {
            self.bot_bite(bot, seen.owner, m, tick)?;
        }
        if behaviour == Behaviour::Objective {
            self.bot_objective_act(bot, tick)?;
        }
        if behaviour != Behaviour::Objective
            && sight.target.is_some()
            && !self.vehicles.weapon_seat(bot)
        {
            if let Some(choice) = native_choice {
                if self
                    .weapons
                    .actor(ActorId(bot))
                    .is_some_and(|a| a.selected != Some(choice.slot))
                {
                    self.abort_bot_hand_charge(bot)?;
                    self.equip_tool(bot, Some(choice.slot))?;
                    self.bots.brains.get_mut(&bot).unwrap().fire_down = false;
                    return Ok(());
                }
            } else if matches!(native, hand_combat::Decision::Unsupported) {
                self.bot_arm(bot)?;
            }
        }
        if fire_changed || (pulse && matches!(native, hand_combat::Decision::Unsupported)) {
            if self.vehicles.weapon_seat(bot) {
                let _ = self.command(
                    bot,
                    sequence,
                    Command::WeaponTrigger {
                        down: fire && !pulse,
                    },
                );
                return Ok(());
            }
            let down = desired_down;
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
    /// Supported inventory intent is checked at the actual post-movement
    /// launch frame. None preserves existing package/mounted executors.
    pub(super) fn bot_hand_fire_gate(
        &mut self,
        bot: OwnerId,
        direction: Vec3,
        tick: u64,
    ) -> Option<bool> {
        let brain = self.bots.brains.get(&bot)?;
        let plan_tick = tick.checked_sub(1)?;
        if brain.native_combat_tick != Some(plan_tick) {
            return None;
        }
        let intent = brain.combat.intent(plan_tick);
        let mut budget = std::mem::take(&mut self.bots.combat_budget);
        let allowed = intent.as_ref().is_some_and(|intent| {
            hand_combat::validate_intent(self, bot, intent, direction, &mut budget)
        });
        self.bots.combat_budget = budget;
        Some(allowed)
    }
    pub(super) fn bot_abort_unsafe_hand_fire(&mut self, bot: OwnerId) -> Result<()> {
        if let Some(brain) = self.bots.brains.get_mut(&bot) {
            brain.fire_down = false;
        }
        if !self.abort_bot_hand_charge(bot)? {
            self.weapons.trigger(ActorId(bot), false)?;
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
    /// A hit with the bot's body (`BotKind::melee`) on `target`, if it
    /// reaches them and the damage rules let it hurt them.
    fn bot_bite(
        &mut self,
        bot: OwnerId,
        target: OwnerId,
        melee: &crate::bot_kind::BotMelee,
        tick: u64,
    ) -> Result<()> {
        let (Some(me), Some(them)) = (self.peers.get(&bot), self.peers.get(&target)) else {
            return Ok(());
        };
        if !them.combat.alive {
            return Ok(());
        }
        let eye = me.player.eye();
        // The nearest point of their body, feet to head.
        let state = them.player.state();
        let feet = Vec3::from(state.feet);
        let height = crate::water::body_height(state, them.player.tuning()) * state.scale;
        let point = Vec3::new(feet.x, eye.y.clamp(feet.y, feet.y + height), feet.z);
        let width = them.player.tuning().width * state.scale * 0.5;
        if point.distance(eye) > melee.reach + width || !self.can_damage_player(bot, target, false)
        {
            return Ok(());
        }
        if let Some(action) = &melee.action {
            self.play_thread(tick, bot, MELEE_THREAD, action);
        }
        self.damage_player_at(
            target,
            melee.damage,
            combat::DamageKind::weapon(melee.name.clone(), true),
            Some(bot),
            Some(point),
        )?;
        // A brick's bot of another side, worn down far enough, turns into
        // one of its kind, whole again (`holeZombieInfect`); players never do.
        let Some(below) = melee.converts_below else {
            return Ok(());
        };
        let worn = self.is_alive(target)
            && self.bots.is_brick_bot(target)
            && self.peers[&target].combat.health <= self.max_health(target) * below;
        let kind = self.bots.brains[&bot].kind.clone();
        let other = self.bots.brains.get(&target).map(|b| b.kind.side.clone());
        if !worn || other.is_none() || other == Some(kind.side.clone()) {
            return Ok(());
        }
        let brain = self.bots.brains.get_mut(&target).unwrap();
        let born = std::mem::replace(&mut brain.kind, kind.clone());
        brain.born.get_or_insert(born);
        brain.posed = false;
        brain.target = None;
        brain.memory = None;
        self.embody_bot(target, &kind)?;
        let max = self.max_health(target);
        self.peers.get_mut(&target).unwrap().combat.health = max;
        Ok(())
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
