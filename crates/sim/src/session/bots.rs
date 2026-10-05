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
use crate::bot_kind::{BotKind, MountAnchor, Moves};
use crate::nav::{Body, Found, Ground, Nav, Search, Waypoint};
use behaviour::{Behaviour, Situation};
use bri_content::passage::{Way, carried_yaw};
use bri_package_runtime::ops::ObjectRef;
use bri_weapons::ActorId;

mod arming;
mod behaviour;
pub(crate) mod cadence;
mod charged_control;
pub(super) use charged_control::FireAdmission;
mod claims;
mod combat_objectives;
mod contest;
#[path = "bots/combat.rs"]
mod hand_combat;
mod interactions;
mod objectives;
mod package_objectives;
mod physical_objectives;
mod planning;
mod search_memory;
mod surprise;
pub use surprise::{BotCandidate, BotDecision, BotDrive, BotSurpriseView};
mod tactics;
mod tuning;
pub use tuning::{BotReload, BotTuning};
mod team;
mod why;
pub use team::BotTeamView;

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
    /// Evidence-based search progress; never an unseen actor position.
    pub search_phase: &'static str,
    pub objective: Option<BrickId>,
    pub objective_diagnostic: Option<&'static str>,
    /// Current derived plan evidence, never commands or authored rule state.
    pub objective_detail: Option<BotObjectiveDetail>,
    /// Actual searches and unchanged failed-search reuse, for headless diagnostics.
    pub objective_searches: u64,
    pub objective_reused: u64,
    /// Why it chose as it did (`surprise`): its drives and the last
    /// decision at each choice point, every term's contribution.
    pub surprise: BotSurpriseView,
    /// How teammates' intents moved its last choice (`team`).
    pub team: BotTeamView,
}
#[derive(Clone, Debug)]
pub struct BotObjectiveDetail {
    pub desired: String,
    pub action: String,
    pub provider: &'static str,
    pub phase: &'static str,
    /// Proposed action IDs; bounded by the planner's depth limit.
    pub route: Vec<String>,
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
/// Ticks before a bot's failing step is told again.
const BOT_FAILURE_EVERY: u64 = 60 * 120;
/// Seconds a rules bot waits past its game's respawn time, as a seeded range.
const RULES_RESPAWN: (f32, f32) = (0.3, 1.5);
/// Seconds a brick bot waits past its respawn time, as a seeded range.
const BRICK_RESPAWN: (f32, f32) = (0.75, 1.75);
/// Farthest from its start a path may lead, across.
const SEARCH_BOUND: f32 = 72.0;
/// Ticks without progress before a bot plans again.
const STUCK_TICKS: u32 = 45;
/// How many ticks earlier than the 40th a stuck bot may hop, by its own
/// seeded phase: bots hop apart, and each still hops before it replans at
/// `STUCK_TICKS`.
const HOP_SPREAD: u64 = 5;
/// Within this of the point its objective takes it to, a bot is at its
/// post, and playing is worth what it watches there (`View::aim`).
const ON_OBJECTIVE: f32 = 6.0;
/// The action (an enemy, what a post watches) within this is worth all of
/// playing; past `ACTION_FAR` playing is worth `LULL_PLAY`, in between it
/// fades.
const ACTION_NEAR: f32 = 16.0;
const ACTION_FAR: f32 = 48.0;
/// Playing's worth in a lull (far from the action, waiting, nothing to
/// do), against a goof's `surprise::GOOF_SCORE`: a little more, so a lull
/// needs some boredom before a goof wins.
const LULL_PLAY: f32 = 0.55;
/// Playing's worth going back home, or with an enemy remembered or an
/// objective that wants one beaten but none in sight.
const RETURN_PLAY: f32 = 0.7;
const PRESSED_PLAY: f32 = 0.85;
/// Plans in a row that got stuck before a bot drops its goal.
const MAX_REPLANS: u32 = 3;
/// The mean seconds of one weave leg at an objective or in water.
const WEAVE_SECONDS: f32 = 0.75;
/// How often a strafe leg turns back rather than keeps on.
const TURN_BACK: f32 = 0.7;
/// Ticks between the aim error's seeded points; it eases between them.
const ERROR_TICKS: u64 = 48;
/// How much of the kind's aim error stays after a long track.
const ERROR_FLOOR: f32 = 0.5;
/// How far a bot's lead strays from the true intercept, as a fraction.
const LEAD_STRAY: f32 = 0.15;
/// Ticks between the lead's seeded points.
const LEAD_TICKS: u64 = 120;
/// Longest a bot carries what it holds toward open space before it throws
/// anyway.
const CARRY_TICKS: u64 = 720;
/// Shortest hold before the throw: it holds its catch up a moment.
const LIFT_TICKS: u64 = 90;
/// A roof closer than this over its eyes is one a carried catch must clear.
const CARRY_HEADROOM: f32 = 8.0;
/// How far a carried catch keeps off the floor and the roof.
const CARRY_CLEARANCE: f32 = 0.3;
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
/// Running and gunning, the farthest off its way (cosine) an enemy is shot
/// at: about 105 degrees, where sideways speed still holds.
const GUN_BEHIND_COS: f32 = -0.26;
/// A melee flyer farther across from its target than this climbs above
/// their feet before it moves over.
const FLY_OVER: f32 = 1.5;
/// Ticks pressing at a waypoint within a step without moving before it is
/// taken as reached.
const WEDGED_TICKS: u32 = 30;
/// Over how many units past the edge of its stroll a brick bot's urge to
/// walk home grows to full.
const STRAY: f32 = 4.0;
/// What a bot's last behaviour choice saw ([`Brain::choice_was`]).
#[derive(Clone, Copy, Debug, Default)]
struct ChoiceWas {
    objective: bool,
    committed: bool,
    holding: bool,
    target: Option<OwnerId>,
}
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
    /// When each bot's failed step was last told (`step_bots`).
    failed_told: BTreeMap<OwnerId, u64>,
    /// A bot whose every step fails, for tests of that isolation.
    failing: Option<OwnerId>,
    /// Geometry observed once per tick; admission still reads live occupancy.
    objects: Vec<bri_vehicles::VehicleSnapshot>,
    claims: claims::Claims,
    interaction_budget: usize,
    objective_budget_tick: u64,
    objective_budget_used: bool,
    objective_cursor: Option<OwnerId>,
    objective_candidate: Option<OwnerId>,
    combat_budget: hand_combat::Budget,
    /// Live dials (`/botset`) and where overrides are kept.
    tuning: tuning::Tuning,
    /// Wall time spent in `step_bots`, for the bot performance bar.
    pub(super) think_nanos: u64,
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
    /// Ticks in a row it pressed on without getting anywhere across, hops
    /// and all.
    wedged: u32,
    replans: u32,
    /// Current aim, turned toward the wanted one at the kind's rate.
    yaw: f32,
    pitch: f32,
    target: Option<OwnerId>,
    /// Tick the current target was first seen.
    seen_since: u64,
    /// Where an enemy was last seen or heard, until when.
    memory: Option<Knowledge>,
    evidence_search: search_memory::State,
    evidence_context: Option<(bri_minigames::GameId, u64, Option<bri_minigames::TeamId>)>,
    fire_down: bool,
    /// What it is doing ([`behaviour`]).
    behaviour: Behaviour,
    /// Tick it took up `behaviour`.
    behaviour_since: u64,
    /// What its last behaviour choice saw, to tell an interrupt.
    choice_was: ChoiceWas,
    /// The weapon it is going to pick up, with nothing to attack with
    /// (`arming`).
    arming: arming::Arming,
    /// The way a ranged fighter strafes (+1 right, -1 left), and the tick
    /// it turns back.
    strafe: (f32, u64),
    objective: objectives::State,
    combat: hand_combat::State,
    native_combat_tick: Option<u64>,
    /// The selected objective owns the ordinary hand trigger this tick.
    objective_tool: bool,
    /// Actual damage evidence can interrupt a noncombat goal. Merely seeing
    /// someone damageable does not make them more urgent than winning.
    objective_threat: Option<Knowledge>,
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
    /// A flight's progress (`BotFighting::fly_give_up_seconds`).
    fly: FlyProgress,
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
    /// Where it took a mount's controls (the vehicle and its feet then):
    /// the anchor of a driver's leash under `BotMounted::anchor`.
    mount_anchor: Option<(u64, Vec3)>,
    /// The spawn brick's Team choice last put into effect, with the game
    /// it was applied in; a new life starts without one, so it is applied
    /// again.
    brick_team: Option<(bri_minigames::GameId, u32)>,
    /// The name this brick bot last asked for (before a number makes it
    /// unique).
    named: String,
    /// Variation among its choices over time ([`surprise`]).
    surprise: surprise::Mind,
    /// Where a flanking chase aims, from the enemy (zero straight at them).
    chase_offset: Vec3,
    /// What teammates' intents did to its last choice ([`team`]).
    team: team::State,
}
/// How a flight is going: the nearest it came and when, and how long it
/// stays on the ground after giving one up.
#[derive(Clone, Copy, Debug, Default)]
struct FlyProgress {
    best: f32,
    since: u64,
    grounded_until: u64,
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
    /// Where a weapon lies that it goes to pick up.
    Arm(Vec3),
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
            | Self::Objective(p)
            | Self::Arm(p) => p,
            Self::Home => home,
        }
    }
}
impl Brain {
    /// Where its chase leash is measured from, and how long it is. A driver
    /// follows its kind's mounted pursuit policy; on foot, its brick's.
    fn pursuit(&self, driving: bool) -> (Vec3, f32) {
        let mounted = &self.kind.mounted;
        match (driving, mounted.anchor, self.mount_anchor) {
            (true, MountAnchor::Mount, Some((_, at))) => (at, mounted.chase_radius),
            (true, _, _) => (self.leash, mounted.chase_radius),
            (false, _, _) => (self.leash, self.kind.chase_radius),
        }
    }
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
            wedged: 0,
            replans: 0,
            yaw: 0.0,
            pitch: 0.0,
            target: None,
            seen_since: 0,
            memory: None,
            evidence_search: Default::default(),
            evidence_context: None,
            fire_down: false,
            behaviour: Behaviour::default(),
            behaviour_since: 0,
            choice_was: ChoiceWas::default(),
            arming: Default::default(),
            strafe: (1.0, 0),
            objective: objectives::State::default(),
            combat: hand_combat::State::default(),
            native_combat_tick: None,
            objective_tool: false,
            objective_threat: None,
            carry: None,
            next_grab: 0,
            next_bite: 0,
            posed: false,
            dry: 0,
            fly: FlyProgress::default(),
            born: None,
            next_interaction: 0,
            object_cursor: 0,
            push_contact: None,
            push_anchor: None,
            vehicle_stuck: 0,
            vehicle_anchor: None,
            vehicle_since: None,
            mount_anchor: None,
            brick_team: None,
            named: String::new(),
            surprise: surprise::Mind::new(bot),
            chase_offset: Vec3::ZERO,
            team: team::State::default(),
        }
    }
    fn random(&mut self) -> f32 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }
    /// The way it strafes or weaves this tick (+1 right, -1 left). Each leg
    /// lasts a seeded `mean` × [`leg`] seconds and then usually turns back,
    /// sometimes keeps on; it turns at once from a way that is not open
    /// (`open`), or when an ally stands that way and the other is open
    /// (`parted`). One pattern for the fight's strafe and every weave.
    fn strafe_leg(
        &mut self,
        tick: u64,
        mean: f32,
        open: &dyn Fn(f32) -> bool,
        parted: bool,
    ) -> f32 {
        let (mut side, mut until) = self.strafe;
        if tick >= until || parted {
            // The first leg goes the other way when it can; later legs
            // sometimes keep on.
            let turn = if until == 0 {
                parted || open(-side) || !open(side)
            } else {
                parted || !open(side) || open(-side) && self.random() < TURN_BACK
            };
            if turn {
                side = -side;
            }
            let u = self.random();
            until = tick + (leg(u, mean) * 120.0) as u64;
        }
        self.strafe = (side, until);
        side
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
    fn pursue(
        &mut self,
        enemy: Option<Seen>,
        fight: bool,
        near: f32,
        feet: Vec3,
        tick: u64,
    ) -> (bool, bool) {
        match (enemy, self.memory) {
            (Some(seen), _) if fight => {
                self.set_goal(None);
                (true, flat(seen.feet - feet).length() < near)
            }
            (Some(seen), _) => {
                // The chase heads for where it really stands, and the path
                // finds the way there.
                let to = seen.real + self.chase_offset;
                let moved_on = match self.goal {
                    Some(Goal::Chase(p)) => p.distance(to) > 2.5,
                    _ => true,
                };
                if moved_on {
                    self.set_goal(Some(Goal::Chase(to)));
                }
                (false, false)
            }
            (None, Some(knowledge)) => {
                let failed = matches!(self.goal, Some(Goal::Search(_)))
                    && self.settled
                    && self.plan.is_empty()
                    && !self.partial_route;
                let next = self.evidence_search.next(knowledge, feet, tick, failed);
                self.set_goal(next.map(Goal::Search));
                (next.is_none(), false)
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
    /// How far each of its projectiles may turn off the aim, in radians
    /// (`Shot::spread`): a scattering weapon closes in to land them.
    spread: f32,
}
/// Half the width a scattering weapon's spread may cover where it fights:
/// about a body's height, so most of its shot lands.
const SPREAD_BODY: f32 = 1.5;
/// The nearest a scattering weapon's band ends, however wide its spread.
const SPREAD_MIN_FAR: f32 = 3.0;
impl Weapon {
    /// Closest and farthest it likes to fight from.
    fn band(&self) -> (f32, f32) {
        if self.melee {
            (0.0, (self.reach * 0.8).max(1.2))
        } else {
            let mut far = (self.reach * 0.7).clamp(6.0, 40.0).min(self.reach);
            // A scattering weapon lands its shot where its spread is still
            // about a body wide: farther, most of it flies past.
            if self.spread > 0.0 {
                far = far.min((SPREAD_BODY / self.spread.tan()).max(SPREAD_MIN_FAR));
            }
            let near = self
                .near
                .unwrap_or((self.splash + 3.0).max(5.0))
                .min(far * 0.75);
            let room = if self.spread > 0.0 { 2.0 } else { 4.0 };
            (near, far.max(near + room).min(self.reach))
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
/// The aim error in radians (yaw, pitch): a smooth seeded drift, sized by
/// how long the bot has tracked its target, from the kind's whole error
/// down to `ERROR_FLOOR` of it. It never jumps and never settles.
fn aim_error(bot: OwnerId, tick: u64, tracked: f32, degrees: f32) -> (f32, f32) {
    let floor = 1.0 - ERROR_FLOOR;
    let size = degrees.to_radians() * (1.0 - (tracked / 2.0).clamp(0.0, 1.0) * floor);
    let salt = cadence::salt::AIM;
    (
        cadence::drift(bot, salt, 0, tick, ERROR_TICKS) * size,
        cadence::drift(bot, salt, 1, tick, ERROR_TICKS) * size * 0.5,
    )
}

/// How much of the target's velocity a bot leads by: about all of it,
/// drifting a little under and over, seeded per bot.
pub(super) fn lead(bot: OwnerId, tick: u64) -> f32 {
    1.0 + LEAD_STRAY * cadence::drift(bot, cadence::salt::LEAD, 0, tick, LEAD_TICKS)
}

/// A strafe leg's length, in seconds, from a uniform draw `u`: `mean`
/// × 0.5 to 1.5, so legs keep their mean and vary by over a quarter.
fn leg(u: f32, mean: f32) -> f32 {
    mean * (0.5 + u.clamp(0.0, 1.0))
}

/// Ticks in one melee footwork cycle: in, back out, aside.
const FOOTWORK_TICKS: u64 = 110;
/// About how far a step back carries a melee fighter, in units.
const FOOTWORK_BACK: f32 = 0.6;
/// A ranged fighter's lean in as it strafes: its most (a share of a
/// walk), the ticks its seeded drift takes to wander, and how far outside
/// its band's near edge it must be to lean in.
const LEAN: f32 = 0.35;
const LEAN_TICKS: u64 = 360;
const LEAN_ROOM: f32 = 2.0;

/// Where a melee fighter in its band steps this tick: in toward its target,
/// back out, a step aside, then a moment planted, each for a seeded share
/// of its own cycle, so no two bots step alike and none walks a circle.
/// Without `room` to give ground and stay in reach it plants instead of
/// stepping back, so its footwork never carries it out of its band (and
/// its fight into a chase and back).
fn footwork(bot: OwnerId, tick: u64, forward: Vec3, right: Vec3, room: bool) -> Vec3 {
    let salt = cadence::salt::FOOTWORK;
    let (k, f) = cadence::cycle(bot, salt, tick, FOOTWORK_TICKS);
    let lunge = cadence::spread(bot, salt, k, 0.15, 0.3);
    let back = lunge + cadence::spread(bot, salt, k ^ 1 << 32, 0.15, 0.25);
    let aside = back + cadence::spread(bot, salt, k ^ 3 << 32, 0.15, 0.25);
    if f < lunge {
        forward * 0.5
    } else if f < back {
        if room { -forward * 0.35 } else { Vec3::ZERO }
    } else if f < aside {
        let side = if cadence::spread(bot, salt, k ^ 2 << 32, 0.0, 1.0) < 0.5 {
            1.0
        } else {
            -1.0
        };
        right * side * 0.4
    } else {
        Vec3::ZERO
    }
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
        let name = self.brick_bot_name(&kind, brick_id);
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
        let joined = self.join_inner(name.clone(), home, false, true, None);
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
            let mut brain = Brain::new(Some(brick_id), kind, home, bot, crossed);
            brain.named = name;
            self.bots.brains.insert(bot, brain);
            self.weapons.set_bot(bri_weapons::ActorId(bot), true)?;
        }
        Ok(())
    }
    /// A spawn brick's bot comes back fresh at its brick (`spawnVehicle`,
    /// a mini-game reset): a new brain and a new life for the same player,
    /// so its mini-game membership and team stay as they were set. Only a
    /// missing bot, or one of another kind, is made again.
    pub(super) fn respawn_brick_bot(&mut self, brick_id: BrickId, kind: &str) -> Result<()> {
        let current = self.bots.by_brick.get(&brick_id).copied().filter(|bot| {
            self.peers.contains_key(bot)
                && self
                    .bots
                    .brains
                    .get(bot)
                    .is_some_and(|b| b.born.as_ref().unwrap_or(&b.kind).id == kind)
        });
        let (Some(bot), Some(brick), Some(kind)) = (
            current,
            self.simulation.state().bricks.get(&brick_id),
            self.bots.kind(kind).cloned(),
        ) else {
            self.reconcile_bot_brick(brick_id, None)?;
            return self.reconcile_bot_brick(brick_id, Some(kind));
        };
        let home = Vec3::from(brick.position) + Vec3::Y * 0.3;
        let crossed = self.crossings.count();
        self.bots.claims.release_owner(bot);
        self.bots.hurt.remove(&bot);
        let mut brain = Brain::new(Some(brick_id), kind, home, bot, crossed);
        if let Some(old) = self.bots.brains.get(&bot) {
            // Its movement sequence only moves forward.
            brain.sequence = old.sequence;
            brain.named = old.named.clone();
        }
        self.bots.brains.insert(bot, brain);
        let player = self.peers[&bot].combat.player;
        let effects = self
            .minigames
            .execute(bri_minigames::Command::ForceRespawn { target: player })
            .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
        self.apply_minigame_effects(effects)
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
        let release_can_fire = self
            .weapons
            .image_state(ActorId(bot), 0)
            .is_some_and(|(image, state)| charged_control::release_may_fire(image, state));
        self.release_trigger(bot)?;
        if release_can_fire {
            self.weapons.cancel_charge(ActorId(bot))
        } else {
            // A released shot's native Fire/cooldown must finish normally.
            Ok(true)
        }
    }
    pub(super) fn rest_rules_bot(&mut self, package: &str, bot: OwnerId, rest: bool) -> Result<()> {
        let own_kind = self.bots.brains.get(&bot).is_some_and(|brain| {
            let Some((owner, _)) = brain.kind.id.split_once(':') else {
                return false;
            };
            owner == package
                || self.packages.as_ref().is_some_and(|host| {
                    host.catalog.packages.get(owner).is_some_and(|provider| {
                        provider.manifest.companions.iter().any(|id| id == package)
                    })
                })
        });
        ensure!(
            self.bots.rules_package(bot) == Some(package) || own_kind,
            "Bot {bot} was not added by `{package}` and its kind is not owned by that package or its companion"
        );
        let brain = self.bots.brains.get_mut(&bot).context("No such bot")?;
        if rest && !brain.resting {
            brain.objective_threat = None;
            brain.set_goal(None);
            brain.target = None;
            brain.memory = None;
            brain.evidence_search.clear();
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
    /// The game a spawn brick's bot plays in: its builder's.
    fn spawn_brick_game(&self, brick: BrickId) -> Option<bri_minigames::GameId> {
        let owner = self.simulation.state().bricks.get(&brick)?.owner;
        let player = self.peers.get(&owner)?.combat.player;
        self.minigames.player(player).ok()?.game
    }
    /// What a spawn brick's bot is called: its kind, then the brick's name
    /// or else the team its Team choice names, so the Players list tells
    /// one brick's bot from another's ("Blockhead Bot (Red)").
    fn brick_bot_name(&self, kind: &BotKind, brick: BrickId) -> String {
        let Some(b) = self.simulation.state().bricks.get(&brick) else {
            return kind.name.clone();
        };
        let label = b
            .name
            .as_deref()
            .map(|n| n.trim().trim_start_matches('_').trim())
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                let team = bri_minigames::TeamId(b.vehicle.as_ref()?.team?);
                let game = self.minigames.game(self.spawn_brick_game(brick)?).ok()?;
                Some(game.teams.get(team)?.name.clone())
            });
        // The label is shortened, not the kind or the closing bracket.
        let room = MAX_PLAYER_NAME.saturating_sub(kind.name.chars().count() + 3);
        match label {
            Some(label) if room > 0 => {
                let label: String = label.chars().take(room).collect();
                format!("{} ({})", kind.name, label.trim_end())
            }
            _ => kind.name.clone(),
        }
    }
    /// A brick bot takes the name its brick now gives it, without the
    /// announcement a player's own rename makes.
    fn rename_brick_bot(&mut self, bot: OwnerId, wanted: String) -> Result<()> {
        let name = self.unique_name_except(&clean_player_name(&wanted), Some(bot));
        if let Some(brain) = self.bots.brains.get_mut(&bot) {
            brain.named = wanted;
        }
        let peer = self.peers.get(&bot).context("No such bot")?;
        if peer.name == name {
            return Ok(());
        }
        let player = peer.combat.player;
        self.admin.rename(bot, name.clone())?;
        let _ = self.minigames.rename(player, name.clone());
        self.peers.get_mut(&bot).context("No such bot")?.name = name;
        Ok(())
    }
    /// The spawn brick's Team choice puts its bot on that team of the game
    /// it plays in: when it joins, after each new life (a reset or a
    /// respawn) and whenever the choice or the game changes. In between,
    /// the game's own commands may move it. A team the game has not got
    /// (yet) is tried again on the next pass.
    fn apply_brick_team(
        &mut self,
        bot: OwnerId,
        game: Option<bri_minigames::GameId>,
        team: Option<u32>,
    ) -> Result<()> {
        let wanted = game.zip(team);
        if self
            .bots
            .brains
            .get(&bot)
            .is_none_or(|b| b.brick_team == wanted)
        {
            return Ok(());
        }
        if let Some((game, slot)) = wanted {
            let team = bri_minigames::TeamId(slot);
            let has = self
                .minigames
                .game(game)
                .is_ok_and(|g| g.teams.get(team).is_some());
            if !has {
                return Ok(());
            }
            let player = self.peers.get(&bot).context("No such bot")?.combat.player;
            if self.minigames.team_of(player) != Some(team) {
                let effects = self
                    .minigames
                    .assign_team(player, Some(team))
                    .map_err(|e| anyhow::anyhow!("Team rejected: {e}"))?;
                self.apply_minigame_effects(effects)?;
                // It appears where its team does, as a rules bot put on a
                // team does.
                let effects = self
                    .minigames
                    .execute(bri_minigames::Command::ForceRespawn { target: player })
                    .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
                self.apply_minigame_effects(effects)?;
            }
        }
        if let Some(brain) = self.bots.brains.get_mut(&bot) {
            brain.brick_team = wanted;
        }
        Ok(())
    }
    /// Minigame membership follows the spawn brick owner, and team the
    /// brick's Team choice.
    fn sync_bot_minigames(&mut self) -> Result<()> {
        let bots: Vec<(OwnerId, BrickId)> = self
            .bots
            .brains
            .iter()
            .filter_map(|(o, b)| Some((*o, b.brick?)))
            .collect();
        for (bot, brick) in bots {
            let Some((owner, team)) = self
                .simulation
                .state()
                .bricks
                .get(&brick)
                .map(|b| (b.owner, b.vehicle.as_ref().and_then(|v| v.team)))
            else {
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
            self.apply_brick_team(bot, wanted, team)?;
            let named = self.bots.brains.get(&bot).and_then(|b| {
                let name = self.brick_bot_name(b.born.as_ref().unwrap_or(&b.kind), brick);
                (name != b.named).then_some(name)
            });
            if let Some(name) = named {
                self.rename_brick_bot(bot, name)?;
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
                surprise: b.surprise.view(&b.kind.surprise),
                team: b.team.view(),
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
            && self.bot_team_relation(bot, other).is_none()
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
    /// Explicit same-game teams are the author's policy, including opposition.
    /// Unassigned creatures retain the original builder/species fallback.
    fn bot_team_relation(&self, bot: OwnerId, other: OwnerId) -> Option<bool> {
        let a = self
            .minigames
            .player(self.peers.get(&bot)?.combat.player)
            .ok()?;
        let b = self
            .minigames
            .player(self.peers.get(&other)?.combat.player)
            .ok()?;
        (a.game.is_some() && a.game == b.game && a.team.is_some() && b.team.is_some())
            .then(|| self.minigames.allied(a.id, b.id))
    }

    /// Whether two brick bots are on one side: one side (Bot_Hole's
    /// `hType`) never fights itself and fights every other; bots of no side
    /// side with their builder.
    fn bot_allies(&self, bot: OwnerId, other: OwnerId) -> bool {
        if let Some(allied) = self.bot_team_relation(bot, other) {
            return allied;
        }
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
        // Real injury takes priority over a previously visible bystander.
        // Resolve only through the same authoritative visibility test; when
        // the attacker is unseen during a retained objective, ordinary dated
        // hurt/search evidence must guide pursuit instead of a fresh passive
        // target. Never renew that evidence from an unseen live position.
        let tick = self.simulation.state().tick;
        let valid_threat = |k: &Knowledge| tick < k.expires && self.bot_enemy(bot, kind, k.subject);
        let threat = self
            .bots
            .hurt
            .get(&bot)
            .copied()
            .filter(valid_threat)
            .or(brain.objective_threat.filter(valid_threat));
        if let Some(threat) = threat {
            let target = visible(threat.subject);
            if target.is_some() || brain.objective.detail().is_some() {
                return Sight { target };
            }
        }
        // Keep fighting the same enemy while it stays in view.
        if let Some(seen) = brain.target.and_then(visible) {
            return Sight { target: Some(seen) };
        }
        // Through an opening, anyone may be in sight wherever they stand.
        let portals = !self.simulation.passages().list.is_empty();
        // Each enemy in view is its own option: the nearest, but each ally
        // already after one makes it farther (`team` overlap).
        let mut candidates: Vec<(f32, OwnerId)> = self
            .peers
            .iter()
            .filter(|(owner, p)| **owner != bot && p.combat.alive)
            .map(|(owner, p)| (p.player.eye().distance(eye), *owner))
            .filter(|(d, _)| portals || *d < kind.sight)
            .map(|(d, owner)| {
                (
                    d * (1.0 + kind.team.overlap() * self.team_crowd(bot, owner)),
                    owner,
                )
            })
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
        // Taking off for someone well above; once flying, until it is by
        // them, or it has gone too long without getting closer.
        let fighting = &brain.kind.fighting;
        let flying = brain.behaviour == Behaviour::Fly;
        let above = to.y - feet.y;
        let across = flat(to - feet).length();
        let landed = !grounded && above < 0.5 && across < 1.0;
        let floor = if flying {
            -fighting.fly_drop
        } else {
            fighting.fly_rise
        };
        let tick = self.simulation.state().tick;
        if across > AIR_CHASE || above < floor || landed || tick < brain.fly.grounded_until {
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
                spread: 0.0,
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
        // v20's `%spread`: each projectile turns by up to 5π·spread about
        // each axis.
        let spread = image
            .shot
            .as_ref()
            .filter(|s| s.spread > 0.0)
            .map_or(0.0, |s| (5.0 * std::f32::consts::PI * s.spread).min(1.4));
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
                spread,
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
                spread,
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
            spread,
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
                    !brain.resting
                        && self.riding.driver_of(**bot).is_none()
                        && !self.riding.is_riding(**bot)
                        && brain.objective.ready(tick)
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
        // One bot's failure is that bot's: it is told and the rest step.
        // Nothing a bot's step does can fail the whole session (its errors
        // are refused controls, commands or embodiment for that bot), so
        // none is passed up.
        for bot in bots {
            if let Err(error) = self.step_bot(bot, tick) {
                self.bot_step_failed(bot, tick, &error);
            }
        }
        self.bots
            .failed_told
            .retain(|b, _| self.bots.brains.contains_key(b));
        self.hear_alerts(tick);
        Ok(())
    }
    /// Tell that `bot`'s step failed: to the log and the host's admins as
    /// an Add-On problem (`packages::note`, told at most once a minute per
    /// code), and at most once a minute per bot however often it fails.
    /// The bot stays; it steps again next tick.
    fn bot_step_failed(&mut self, bot: OwnerId, tick: u64, error: &anyhow::Error) {
        let due = self
            .bots
            .failed_told
            .get(&bot)
            .is_none_or(|at| tick >= at.saturating_add(BOT_FAILURE_EVERY));
        if !due {
            return;
        }
        self.bots.failed_told.insert(bot, tick);
        let kind = self
            .bots
            .brains
            .get(&bot)
            .map_or("?", |b| b.kind.id.as_str());
        let message = format!("bot {bot} ({kind}) could not think this tick: {error:#}");
        match self.packages.as_mut() {
            Some(host) => super::packages::note(
                host,
                bri_package_runtime::Diagnostic::warning("bot.step", message)
                    .at(format!("bots/{kind}")),
            ),
            None => bri_console::warn(&message),
        }
    }
    /// Make every step of `bot` fail (None: none), to test that one bot's
    /// failure leaves the others thinking.
    #[doc(hidden)]
    pub fn fail_bot_steps(&mut self, bot: Option<OwnerId>) {
        self.bots.failing = bot;
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
        if self.bots.failing == Some(bot) {
            anyhow::bail!("this bot's steps were made to fail");
        }
        if self.bots.brains[&bot].objective.drive(self, bot).is_none() {
            self.promote_bot_seat(bot)?;
        }
        let context = self.game_of(bot).and_then(|game| {
            Some((
                game,
                self.minigames.game(game).ok()?.round,
                self.minigames
                    .player(self.peers.get(&bot)?.combat.player)
                    .ok()?
                    .team,
            ))
        });
        if let Some(brain) = self.bots.brains.get_mut(&bot)
            && brain.evidence_context != context
        {
            brain.evidence_context = context;
            brain.objective_threat = None;
            self.bots.hurt.remove(&bot);
            brain.memory = None;
            brain.target = None;
            brain.evidence_search.clear();
        }
        let Some(peer) = self.peers.get(&bot) else {
            return Ok(());
        };
        if !peer.combat.alive {
            // A brick's bot comes back about a second after it may; a rules
            // bot soon after its game lets it (Slayer's bot respawn time).
            // Each death adds its own seeded delay, so bots one blast killed
            // do not all come back on the same tick.
            let (lo, hi) = if self.bots.by_rules.contains_key(&bot) {
                RULES_RESPAWN
            } else {
                BRICK_RESPAWN
            };
            let delay = cadence::spread(
                bot,
                cadence::salt::RESPAWN,
                peer.combat.respawn_tick,
                lo,
                hi,
            );
            let wait = (delay * 120.0) as u64;
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
                brain.evidence_search.clear();
                brain.posed = false;
                brain.objective = objectives::State::default();
                brain.objective_threat = None;
                self.bots.hurt.remove(&bot);
                brain.dry = 0;
                brain.rehome = brain.brick.is_none();
                brain.leash = brain.home;
                self.bots.claims.release_owner(bot);
                self.bots.claims.forget(bot);
                brain.vehicle_since = None;
                brain.vehicle_stuck = 0;
                brain.vehicle_anchor = None;
                brain.fire_down = false;
                brain.objective_tool = false;
                brain.surprise.new_life();
                brain.chase_offset = Vec3::ZERO;
            }
            return Ok(());
        }
        // Held still by the rules: it stands, holding its fire.
        if self.bots.brains.get(&bot).is_some_and(|b| b.resting) {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.objective.suspend(tick);
            brain.native_combat_tick = None;
            brain.objective_tool = false;
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
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.objective.suspend(tick);
            brain.native_combat_tick = None;
            brain.objective_tool = false;
            let release = std::mem::take(&mut brain.fire_down);
            self.vehicles.set_fire(bot, false);
            if release && !self.abort_bot_hand_charge(bot)? {
                self.weapon_trigger(bot, false, Vec3::ZERO, false)?;
            }
            return Ok(());
        }
        if !self.seated(bot) {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.vehicle_since = None;
            brain.vehicle_anchor = None;
            brain.vehicle_stuck = 0;
            brain.mount_anchor = None;
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
        if driving.is_some() {
            let mounted = self.mounted(bot).map(|(vehicle, _)| vehicle);
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            if brain.mount_anchor.map(|(v, _)| v) != mounted {
                brain.mount_anchor = mounted.map(|v| (v, feet));
            }
        }
        let (mut leash, mut chase_radius) = self.bots.brains[&bot].pursuit(driving.is_some());
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
        self.surprise_settle(bot, tick);
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
        // An observed native grip is an ordinary held-tool sequence.
        // Finish its carry/release before considering another hand weapon;
        // switching would make the package drop the actual held participant.
        let hold_sequence =
            self.bot_weapon(bot).is_some_and(|w| w.hold) && self.held_by(bot).is_some();
        let native = if hold_sequence {
            hand_combat::Decision::Unsupported
        } else if let Some(seen) = sight.target {
            let mut combat = std::mem::take(&mut self.bots.brains.get_mut(&bot).unwrap().combat);
            let mut budget = std::mem::take(&mut self.bots.combat_budget);
            budget.begin_tick(tick);
            let mut mind = std::mem::take(&mut self.bots.brains.get_mut(&bot).unwrap().surprise);
            let decision =
                hand_combat::choose(self, bot, seen, tick, &mut combat, &mut budget, &mut mind);
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.combat = combat;
            brain.surprise = mind;
            self.bots.combat_budget = budget;
            decision
        } else {
            hand_combat::Decision::Unsupported
        };
        let native_choice = match &native {
            hand_combat::Decision::Ready(c) | hand_combat::Decision::Charging(c) => Some(*c),
            _ => None,
        };
        let native_gate = !hold_sequence
            && (!matches!(native, hand_combat::Decision::Unsupported)
                || (sight.target.is_none() && self.bots.brains[&bot].native_combat_tick.is_some()));
        self.bots.brains.get_mut(&bot).unwrap().native_combat_tick = native_gate.then_some(tick);
        // Fighting empty-handed with an enemy in sight (just respawned
        // mid-fight, say), it takes out its weapon now, so it keeps the band
        // of that weapon and does not drop into a chase for a tick.
        if matches!(native, hand_combat::Decision::Unsupported)
            && sight.target.is_some()
            && self.bots.brains[&bot].behaviour == Behaviour::Fight
            && !self.vehicles.weapon_seat(bot)
            && self.bot_weapon(bot).is_none()
        {
            self.bot_arm(bot)?;
        }
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
            spread: 0.0,
        }));
        let hurt_by = self.bots.hurt.remove(&bot).filter(|k| {
            tick < k.expires && self.bot_enemy(bot, &self.bots.brains[&bot].kind, k.subject)
        });
        if self.bots.brains[&bot].memory.is_some_and(|k| {
            tick >= k.expires || !self.bot_enemy(bot, &self.bots.brains[&bot].kind, k.subject)
        }) {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.memory = None;
            brain.evidence_search.clear();
        }
        let threat = hurt_by
            .or(self.bots.brains[&bot].objective_threat)
            .filter(|k| {
                tick < k.expires && self.bot_enemy(bot, &self.bots.brains[&bot].kind, k.subject)
            });
        self.bots.brains.get_mut(&bot).unwrap().objective_threat = threat;
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
            .filter(|_| flat(feet - leash).length() <= chase_radius);
        let vehicle_weapon = self.bot_vehicle_weapon(bot).is_some();
        let can_retaliate = vehicle_weapon
            || self.bots.brains[&bot]
                .kind
                .melee
                .as_ref()
                .is_some_and(|melee| melee.damage > 0.0)
            || hand_combat::has_possible_attack(self, bot);
        let retaliating =
            threat.is_some() && can_retaliate && flat(feet - leash).length() <= chase_radius;
        let mut pausing_delivery = false;
        let ranged_in_hand = held.is_some_and(|w| !w.melee);
        let objective = self.bot_objective(bot, tick).filter(|view| {
            // A carrier keeps delivering (it shoots back on the way); one
            // that can shoot back on the way does not stop for a hurt from
            // an enemy it sees.
            if view.committed
                || view.feet_only()
                    && ranged_in_hand
                    && threat.is_some_and(|k| sight.target.is_some_and(|s| s.owner == k.subject))
            {
                return true;
            }
            // Keep the selected action and its completion baseline, but let
            // ordinary chase/search resolve a dated real injury even outside
            // the weapon band. Objective's utility otherwise beats pursuit.
            // account_control suspends its approach clock during this pause.
            if view.enemy.is_none() && retaliating {
                pausing_delivery = true;
                return false;
            }
            // A different visible hostile is an ordinary combat interruption,
            // not execution of the retained intended-participant action.
            view.enemy
                .is_none_or(|target| sight.target.is_none_or(|seen| seen.owner == target))
        });
        // Objective reservations use the bot itself as their subject. Hand a
        // completed or invalidated action back before combat discovery sees
        // that reservation and mistakes its different subject for a failure.
        // Successful policy progress must not cool down the next action on
        // the same physical resource.
        if self
            .bots
            .claims
            .owner_claim(bot, tick)
            .is_some_and(|claim| {
                claim.subject == bot
                    && objective.and_then(|view| view.resource) != Some(claim.resource)
            })
        {
            self.bots.claims.release_owner(bot);
        }
        // A live objective reservation is not a combat opportunity. Both use
        // the same advisory leases, occupancy and native action admission.
        let opportunity =
            if pausing_delivery || objective.is_some_and(|view| view.resource.is_some()) {
                None
            } else {
                self.bot_interaction(bot, interaction_enemy, tick)
            };
        let objective_holding = objective
            .and_then(|view| view.held)
            .is_some_and(|target| self.held_by(bot) == Some(target));
        let objective_hold_control = objective.is_some_and(|view| {
            view.trigger.is_some() && matches!(view.resource, Some(claims::Resource::Body { .. }))
        });
        let crew_ready = self.bot_crew_ready(bot, tick);
        // A ranged shot that misses carries on to its reach, and goes as
        // far off as its aim errs now.
        let ranged = weapon.is_some_and(|w| !w.melee);
        let (yaw_error, pitch_error) = {
            let brain = &self.bots.brains[&bot];
            let tracked = tick.saturating_sub(brain.seen_since) as f32 * TICK;
            aim_error(bot, tick, tracked, brain.kind.aim_error_degrees)
        };
        let aim_off = yaw_error.hypot(pitch_error);
        let attack_clear = sight.target.is_none_or(|seen| {
            self.bot_fire_clear(
                bot,
                eye,
                seen.eye - Vec3::Y * 0.5,
                weapon.map_or(0.0, |w| w.splash),
                weapon
                    .filter(|_| ranged)
                    .map_or(0.0, |w| (w.reach - eye.distance(seen.eye)).max(0.0)),
                if ranged { aim_off } else { 0.0 },
            )
        });
        // Where its weapon will hit, for its side to keep out of (`team`):
        // the line the clear-fire check above holds fire for.
        let harm = sight.target.filter(|_| ranged).map(|seen| {
            let to = seen.eye - Vec3::Y * 0.5;
            let past = weapon.map_or(0.0, |w| (w.reach - eye.distance(seen.eye)).max(0.0));
            claims::Space {
                from: eye,
                to: to + (to - eye).normalize_or_zero() * past,
                radius: weapon.map_or(0.0, |w| w.splash).max(0.3),
                spread: aim_off.tan(),
            }
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

        // A known noncombat body/tool cannot resolve a threat by staring at
        // it. Keep its useful objective; unknown scripted attacks retain their
        // existing behavior rather than being silently classified as harmless.
        let peaceful_objective =
            objective.is_some_and(|view| view.enemy.is_none()) && threat.is_none();
        let objective_without_attack = peaceful_objective
            || objective.is_some_and(|view| view.enemy.is_none()) && !can_retaliate;
        let allies = self.claim_allies(bot, tick);
        // At a body an opponent is also moving, staying engaged in the
        // contest keeps this side's intention live (`contest::engaged`).
        let contest_engaged = objective
            .and_then(|view| view.resource)
            .is_some_and(|resource| contest::engaged(self, bot, resource, feet));
        // Where it would cover a body a teammate holds (`contest::cover`).
        let cover = objective.and_then(|view| {
            contest::cover(self, bot, view.resource?, feet, view.heading).map(|point| (point, view))
        });
        // Nothing to attack with (a driver has its chassis), an enemy about
        // and no peaceful objective: it arms itself from a weapon in sight
        // before it goes after anyone. Armed and in a game, a better weapon
        // in sight is an upgrade it may go for (`arming`).
        let enemy_known =
            sight.target.is_some() || threat.is_some() || self.bots.brains[&bot].memory.is_some();
        let enemy_distance = sight
            .target
            .map(|seen| seen.feet)
            .or(self.bots.brains[&bot].memory.map(|k| k.at))
            .map(|at| flat(at - feet).length());
        let wants_arm = driving.is_none()
            && !peaceful_objective
            && self.bots.brains[&bot]
                .kind
                .behaviours
                .get("arm")
                .is_none_or(|w| *w > 0.0)
            && if can_retaliate {
                self.game_of(bot).is_some()
            } else {
                enemy_known
            };
        let arm = if wants_arm {
            arming::arm_point(self, bot, feet, enemy_distance, can_retaliate, tick)
        } else {
            self.bots.brains.get_mut(&bot).unwrap().arming.clear();
            None
        };
        // What guards the surprise chooser: carrying an objective, urgency.
        let carrying = objective_holding || self.surprise_carrying(bot, objective.as_ref());
        let gate = self.surprise_gate(bot, feet, carrying, threat, tick);
        // Hurt just now, and it matters (low health, or from close by).
        let hurt_now = hurt_by.is_some() && gate.urgent;
        let flanks = sight.target.map_or([None, None], |seen| {
            self.surprise_flanks(bot, feet, seen.real)
        });
        // Teammates' intents, and what this bot exposes to them (`team`).
        let intents = self.team_intents(bot, tick);
        let seats = self.team_seats(bot, tick);
        let mount = self.mounted(bot).map(|(v, _)| v);
        let sightline = if driving.is_none() {
            self.team_sightline(
                bot,
                eye,
                sight
                    .target
                    .map(|s| s.real)
                    .or(self.bots.brains[&bot].memory.map(|k| k.at)),
            )
        } else {
            None
        };
        let carries = driving.and_then(|_| self.team_carries(bot, feet));
        let simulation = &self.simulation;
        let clear = move |a: Vec3, b: Vec3| {
            let d = b - a;
            super::admin_players::world_ray(simulation, a, d.normalize_or_zero(), d.length())
                .is_none()
        };
        let tall = self.peers[&bot].player.tuning().stand_height;
        let deficit = self.team_deficit(bot);
        // The target it went after died or left (not merely out of sight).
        let target_gone = self.bots.brains[&bot]
            .choice_was
            .target
            .is_some_and(|t| !self.peers.get(&t).is_some_and(|p| p.combat.alive));
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let kind = brain.kind.clone();
        // The grounded objective owns its hold controls, including ordinary
        // release of a different body caught by the real ray. Legacy carry
        // must not take over that recovery or start a combat swing.
        if !holding || objective_hold_control {
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
            (leash, chase_radius) = brain.pursuit(driving.is_some());
        }
        let moved = feet.distance(brain.last_position);
        // Across: a bot hopping against what blocks it moves up and down
        // but gets nowhere.
        let moved_across = flat(feet - brain.last_position).length();
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
                brain.evidence_search.observe(knowledge, feet);
                if cadence::beat(bot, cadence::salt::ALERT, tick, 30) || brain.seen_since == tick {
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
        let away = flat(feet - leash).length();
        if away > chase_radius {
            brain.memory = None;
            brain.evidence_search.clear();
            brain.target = None;
        }

        // Behaviour: the most urgent that applies.
        let enemy = sight.target.filter(|_| away <= chase_radius);
        // An enemy in sight it can shoot on the way to an objective that
        // needs only its feet (run and gun).
        let can_gun = enemy.is_some()
            && weapon.is_some_and(|w| !w.melee)
            && objective.is_some_and(|view| view.feet_only());
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
            holding: holding && !objective_hold_control,
            fly: !objective_without_attack
                && swim.is_none()
                && air.is_some_and(|a| !walks_up(a.to)),
            interaction: opportunity.map_or(0.0, |o| o.utility),
            objective: objective.is_some(),
            committed: objective.is_some_and(|view| view.committed),
            gunning: can_gun,
            arm: arm.map_or(0.0, |(_, score)| score),
            // A swimmer reaches any depth: only how far counts.
            enemy: enemy
                .filter(|_| !objective_without_attack)
                .map(|seen| match swim {
                    Some(_) => (seen.feet.distance(feet), 0.0),
                    None => (flat(seen.feet - feet).length(), seen.feet.y - feet.y),
                }),
            far,
            step: body.step,
            remembers: brain.memory.is_some(),
            strayed: if brain.brick.is_some() {
                (away - kind.wander_radius) / STRAY
            } else {
                0.0
            },
            pursuing: brain.objective.pursuing(),
        };
        let mut scores = behaviour::scores(&situation, |b| {
            kind.behaviours.get(b.name()).copied().unwrap_or(
                if matches!(b, Behaviour::Interact | Behaviour::Objective) {
                    0.0
                } else {
                    1.0
                },
            )
        });
        // What the choice must answer at once (`behaviour::Hold`): urgent
        // damage, an objective offered (one gone a moment is held, `paused`),
        // picked up or dropped, an enemy coming into sight, another target,
        // or the target dead or gone (one out of sight a moment is held). A choice no longer possible, or a must-do
        // one winning, the rule sees for itself.
        let target = sight.target.map(|seen| seen.owner);
        let retarget = brain
            .choice_was
            .target
            .is_some_and(|t| target.is_some_and(|now| now != t));
        let interrupt = hurt_now
            || situation.objective && !brain.choice_was.objective
            || situation.committed != brain.choice_was.committed
            || situation.holding != brain.choice_was.holding
            || retarget
            || target_gone
            || brain.behaviour == Behaviour::Wander
                && brain.choice_was.target.is_none()
                && target.is_some();
        brain.choice_was = ChoiceWas {
            objective: situation.objective,
            committed: situation.committed,
            holding: situation.holding,
            target,
        };
        // What each option would do, and what allies' intents add to it
        // (`team`): never while it carries an objective or is urgent.
        let choices = team_choices(
            opportunity,
            enemy,
            brain.memory,
            arm.map(|(at, _)| at),
            objective,
            feet,
            brain.home,
            team::exit(&intents, feet, tall),
            carries,
        );
        // Its objective is worth more as its team falls behind. (What it saw
        // work for a teammate the surprise chooser weighs: `team` copy.)
        scores[Behaviour::Objective as usize] *= 1.0 + kind.team.pressure * deficit;
        let plain_before = behaviour::best(&scores) as usize;
        let (current, current_since) = (brain.behaviour as usize, brain.behaviour_since);
        brain.team.terms = if gate.carrying || gate.urgent {
            Default::default()
        } else {
            team::adjust(
                &kind.team,
                bot,
                |b| if b == current { current_since } else { tick },
                &mut scores,
                &choices,
                &intents,
                tall,
                &clear,
            )
        };
        brain.team.allies = intents.len();
        // The plain pick, or a near one the surprise chooser takes.
        brain.surprise.gate = gate;
        let behaviour = surprise::behaviour(
            &mut brain.surprise,
            &kind.surprise,
            kind.hold,
            &scores,
            interrupt,
            // Between steps, or the objective gone a moment as one step
            // hands over to the next (a checkpoint reached), or the enemy
            // it chases out of sight a moment (heading for where it was
            // last seen): held, not dropped for a tick.
            brain.behaviour == Behaviour::Objective
                && (brain.objective.between_steps() || objective.is_none())
                || brain.behaviour == Behaviour::Chase
                    && target.is_none()
                    && brain.memory.is_some(),
            if can_retaliate {
                &behaviour::MUST_ARMED
            } else {
                &behaviour::MUST
            },
            gate,
            tick,
        );
        // A choice the terms changed is called out (`team`).
        let callout = (behaviour != brain.behaviour
            && behaviour as usize != plain_before
            && tick >= brain.team.next_callout)
            .then(|| {
                team::callout(
                    &kind.team,
                    &brain.team.terms,
                    plain_before,
                    behaviour as usize,
                )
            })
            .flatten()
            .map(str::to_owned);
        if behaviour != brain.behaviour {
            brain.behaviour_since = tick;
        }
        brain.behaviour = behaviour;
        // A flight that stops getting closer is given up for a while; one
        // within its band of them is where it means to be.
        if behaviour == Behaviour::Fly
            && let Some(air) = air
        {
            let gap = air.to.distance(feet);
            if brain.behaviour_since == tick || gap < brain.fly.best - 1.0 || gap <= far {
                brain.fly.best = gap;
                brain.fly.since = tick;
            } else {
                let give_up = (kind.fighting.fly_give_up_seconds * 120.0) as u64;
                if tick > brain.fly.since + give_up {
                    brain.fly.grounded_until = tick + give_up;
                }
            }
        }
        let chosen = choices[behaviour as usize];
        self.bots.claims.publish(
            bot,
            claims::Intent {
                option: behaviour as u8,
                since: brain.behaviour_since,
                place: chosen.place,
                target: chosen.target,
                seats,
                harm,
                mount,
                sight: sightline,
                flavour: brain.surprise.flavour().map(|f| f as u8),
                until: tick + 3,
            },
        );
        let mut selected_objective = objective.filter(|_| behaviour == Behaviour::Objective);
        let objective_resource = selected_objective
            .and_then(|view| view.resource)
            .filter(|_| {
                // After real sensor admission, the native grip already owns
                // exclusivity. A stationary delayed guard needs no advisory
                // approach lease; expiring that lease must not release an
                // otherwise valid physical hold. The provider still checks
                // identity, inventory, permission and occupancy every turn.
                // A delivered loose body likewise needs no lease while its
                // authored delay runs; an ally may take it up meanwhile.
                !selected_objective.is_some_and(|view| {
                    view.waiting
                        && if view.held.is_some() {
                            objective_holding
                        } else {
                            view.drive.is_none() && view.board.is_none()
                        }
                })
            });
        if behaviour != Behaviour::Interact
            && self
                .bots
                .claims
                .owner_claim(bot, tick)
                .is_some_and(|claim| {
                    objective_resource != Some(claim.resource) || claim.subject != bot
                })
        {
            self.bots.claims.release_owner(bot);
        }
        if let Some(resource) = objective_resource {
            if self.bots.claims.acquire(
                bot,
                bot,
                resource,
                selected_objective.unwrap().point.distance(feet),
                tick,
                |o| allies.contains(&o),
            ) {
                self.bots.claims.progress(
                    bot,
                    selected_objective.unwrap().point.distance(feet),
                    selected_objective.unwrap().physical_progress || contest_engaged,
                    tick,
                );
            } else if let Some((point, view)) = cover {
                // A teammate has the body: keep the objective and cover
                // behind it instead, ready to take it up when the claim ends.
                selected_objective = Some(objectives::View::locomotion(point, view.aim));
            } else {
                // Advisory contention blocks this proposed action, never the
                // authority/physics rules. It cannot grant tool or seat use.
                brain.objective.fail(tick, "objective resource claimed");
                brain.set_goal(None);
                brain.native_combat_tick = None;
                selected_objective = None;
            }
        }

        // Carrying an objective's delivery that needs only its feet (no
        // tool, trigger, body or seat), it shoots an enemy in sight on the
        // way, as a player runs and guns; the walk goes on. (On the way to
        // pick up, an enemy in its band is fought instead.)
        let gunning = behaviour == Behaviour::Objective
            && can_gun
            && selected_objective.is_some_and(|view| view.committed)
            && selected_objective.is_some_and(|view| view.feet_only());
        // Goal.
        brain.chase_offset = Vec3::ZERO;
        let (hold, back_off) = match behaviour {
            Behaviour::Arm => {
                if let Some((at, _)) = arm
                    && !matches!(brain.goal, Some(Goal::Arm(p)) if p.distance(at) < 0.2)
                {
                    brain.set_goal(Some(Goal::Arm(at)));
                }
                (false, false)
            }
            Behaviour::Interact => {
                if let Some(o) = opportunity
                    && !matches!(brain.goal, Some(Goal::Interact(p)) if p.distance(o.point) < 0.2)
                {
                    brain.set_goal(Some(Goal::Interact(o.point)));
                }
                (false, false)
            }
            Behaviour::Objective => {
                if let Some(step) = selected_objective.as_ref() {
                    if step.enemy.is_some() {
                        // The objective supplies the intended participant;
                        // ordinary bands and dated evidence execute combat.
                        if let Some(evidence) = step.enemy_evidence
                            && brain.memory.is_none_or(|memory| {
                                memory.subject != evidence.subject
                                    || memory.observed < evidence.observed
                            })
                        {
                            // This is a dated observation from the selected
                            // participant, never its current hidden transform.
                            brain.memory = Some(evidence);
                            brain.evidence_search.observe(evidence, feet);
                        }
                        let intended = enemy.filter(|seen| Some(seen.owner) == step.enemy);
                        let in_band = intended.is_some_and(|seen| {
                            flat(seen.feet - feet).length() <= far
                                && (seen.feet.y - feet.y).abs() <= body.step + 1.0
                        });
                        brain.pursue(intended, in_band, near, feet, tick)
                    } else {
                        if step.waiting && !step.move_while_waiting {
                            brain.set_goal(None);
                        } else if !matches!(brain.goal, Some(Goal::Objective(p)) if p.distance(step.point) < 0.2)
                        {
                            brain.set_goal(Some(Goal::Objective(step.point)));
                        }
                        (false, false)
                    }
                } else {
                    // Between steps, held by the hold rule: it stands where
                    // the last step finished until the next is planned.
                    brain.set_goal(None);
                    (false, false)
                }
            }
            Behaviour::Carry => {
                brain.set_goal(brain.carry.and_then(|c| c.to).map(Goal::Carry));
                (false, false)
            }
            Behaviour::Fight => brain.pursue(enemy, true, near, feet, tick),
            Behaviour::Chase | Behaviour::Search => {
                // Straight at them, or wide round a side (`surprise`).
                if let Some(seen) = enemy.filter(|_| behaviour == Behaviour::Chase) {
                    brain.chase_offset = surprise::route(
                        &mut brain.surprise,
                        &kind.surprise,
                        kind.hold,
                        seen.real,
                        flanks,
                        gate,
                        tick,
                    );
                }
                brain.pursue(enemy, false, near, feet, tick)
            }
            // Flying goes after them as walking would, so its path tells
            // when a walk leads up after all.
            Behaviour::Fly => {
                let fight = enemy.is_some_and(|seen| {
                    flat(seen.feet - feet).length() <= far
                        && (seen.feet.y - feet.y).abs() <= body.step + 1.0
                });
                brain.pursue(enemy, fight, near, feet, tick)
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
                // One it presses against without moving, within a step of
                // it (wedged on a door jamb), it takes as reached, if more
                // of the route follows. (Not on the way to work a body or a
                // brick: pressing against those is the work.)
                let wedged = brain.wedged > WEDGED_TICKS
                    && brain.plan.len() > 1
                    && flat(d).length() < 1.0
                    && !matches!(brain.goal, Some(Goal::Objective(_) | Goal::Interact(_)));
                if next.through.is_none()
                    && (flat(d).length() < if body.conservative { 1.2 } else { 0.4 } || wedged)
                    && d.y.abs() < body.step + 0.5
                {
                    brain.plan.remove(0);
                    brain.stuck = 0;
                    brain.wedged = 0;
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
            // Standing over a drop's landing, at the very edge: heading for
            // the landing itself just wobbles on the edge, so walk on
            // toward what comes after it until the drop carries it down.
            if let Some(next) = wanted.as_mut()
                && next.through.is_none()
                && state.grounded
                && next.feet.y < feet.y - body.step - 0.05
                && flat(next.feet - feet).length() < 0.6
            {
                let beyond = brain.plan.get(1).map_or(point, |w| w.feet);
                if flat(beyond - feet).length() > 0.6 {
                    next.through = Some(beyond);
                }
            }
            // A grid route ends at a cell, not necessarily at the authored
            // interaction point. Finish a nearby approach with the ordinary
            // motor; the action's physical reach decides when it succeeds.
            // Include the search's arrival radius, consumed-waypoint tolerance
            // and bounded target drift rather than stopping in the gap between
            // those tolerances and an unrelated fixed one-unit cutoff.
            if wanted.is_none()
                && brain.search.is_none()
                && matches!(goal, Goal::Interact(_) | Goal::Objective(_) | Goal::Arm(_))
                && flat(point - feet).length()
                    < crate::nav::ARRIVAL_RADIUS + if body.conservative { 1.2 } else { 0.4 } + 0.2
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
        // Standing on a body (a vehicle's roof, a crate) the walk grid has
        // no place for its feet and no route from there: it walks straight
        // off toward the enemy it is after, and plans again once down.
        if wanted.is_none()
            && swim.is_none()
            && driving.is_none()
            && state.grounded
            && let Some(Goal::Chase(point)) = self.bots.brains[&bot].goal
            && flat(point - feet).length() > 1.5
            && self.bots.brains[&bot].search.is_none()
            && super::admin_players::world_ray(
                &self.simulation,
                own_feet + Vec3::Y * 0.1,
                Vec3::NEG_Y,
                0.4,
            )
            .is_none()
        {
            wanted = Some(Waypoint {
                feet: point,
                jump: false,
                through: None,
                crouch: false,
            });
            self.bots.brains.get_mut(&bot).unwrap().settled = false;
        }
        let pushing = if behaviour == Behaviour::Interact {
            self.act_bot_interaction(bot, interaction_enemy.map(|k| k.at), tick)?
        } else {
            None
        };
        // The enemy it goes after is walked up to, not around.
        let quarry = sight
            .target
            .filter(|_| matches!(behaviour, Behaviour::Fight | Behaviour::Chase))
            .map(|seen| seen.owner);
        let goal_at = self.bots.brains[&bot].goal.map(|g| g.point(home));
        let walk_direction = wanted.map(|p| {
            self.bot_walk_direction(
                bot,
                flat(p.feet - feet).normalize_or_zero(),
                quarry,
                goal_at,
            )
        });
        // Allies close by, which a strafe does not walk into.
        let allies_near: Vec<Vec3> = self
            .peers
            .iter()
            .filter(|(o, p)| {
                **o != bot && p.combat.alive && !self.seated(**o) && self.bot_allies(bot, **o)
            })
            .map(|(_, p)| Vec3::from(p.player.state().feet))
            .filter(|at| flat(*at - feet).length() < 3.0)
            .collect();
        // Now and then something idle (`surprise`): goofing is an option
        // the chooser weighs against playing, with a small worth of its
        // own. Playing is worth what the bot is doing now: the action
        // close by (an enemy near, the objective's focus near once it is
        // at its post, on its way to it) wins easily; far from the action,
        // waiting, or with nothing to do, playing is worth little and a
        // goof wins now and then. Only what makes a goof impossible rules
        // it out: fighting or flying, hurt just now (urgent), holding
        // something, carrying the objective, or off its feet.
        let idle = behaviour == Behaviour::Wander && objective.is_none();
        let action = |d: f32| {
            let t = ((d - ACTION_NEAR) / (ACTION_FAR - ACTION_NEAR)).clamp(0.0, 1.0);
            1.0 + (LULL_PLAY - 1.0) * t
        };
        let mut play: f32 = if idle { LULL_PLAY } else { RETURN_PLAY };
        if let Some(view) = objective.as_ref() {
            play = play.max(if view.waiting {
                LULL_PLAY
            } else if flat(view.point - feet).length() > ON_OBJECTIVE {
                // On its way: on task.
                1.0
            } else {
                // At its post: worth what it watches.
                action(flat(view.aim - feet).length())
            });
        }
        if selected_objective.is_some_and(|v| v.enemy.is_some()) {
            play = play.max(PRESSED_PLAY);
        }
        if let Some(seen) = sight.target {
            play = play.max(action(flat(seen.feet - feet).length()));
        } else if self.bots.brains[&bot].memory.is_some() {
            play = play.max(PRESSED_PLAY);
        }
        let natural = threat.is_none()
            && !matches!(behaviour, Behaviour::Fight | Behaviour::Fly)
            && !holding
            && driving.is_none()
            && swim.is_none()
            && !self.seated(bot);
        let pause_gate = surprise::Gate {
            carrying: objective_holding || objective.as_ref().is_some_and(|v| v.committed),
            ..gate
        };
        // Carrying: the catch hangs off the floor, or it drags and trails
        // back into its holder's path, and under the roof, or it snags on
        // the roof's edge on the way out. The hold's point is raised or
        // lowered by how far the catch's body is off that band.
        let carry_pitch = holding
            .then(|| {
                let (_, distance, grip, _) = self.bot_hold_geometry(bot)?;
                let (low, high) = self.held_extent(bot)?;
                let ceiling =
                    super::admin_players::world_ray(&self.simulation, eye, Vec3::Y, CARRY_HEADROOM)
                        .map_or(f32::INFINITY, |up| eye.y + up);
                let lift = (feet.y + CARRY_CLEARANCE - low).max(0.0);
                let duck = (high - (ceiling - CARRY_CLEARANCE)).max(0.0);
                let wanted = grip.y + if duck > 0.0 { -duck } else { lift };
                Some(
                    ((wanted - eye.y) / distance.max(0.5))
                        .clamp(-1.0, 1.0)
                        .asin(),
                )
            })
            .flatten();
        let mut pause = self.surprise_pause(bot, natural, idle, pause_gate, eye);
        pause.play = play;
        (pause.pull, pause.copy) = self.team_mood(bot, feet, eye, tick);
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let moment = brain
            .surprise
            .goof(&brain.kind.surprise, brain.kind.hold, &pause, tick);
        let act = self.surprise_act(bot, moment, feet, eye, tick)?;
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

        // It runs on facing its way and shoots only an enemy ahead or to
        // the side: walking backwards is slow (`PlayerTuning::backward`).
        let gunning = gunning
            && sight.target.zip(wanted).is_none_or(|(seen, next)| {
                let way = flat(next.feet - feet).normalize_or_zero();
                let to = flat(seen.feet - feet).normalize_or_zero();
                way == Vec3::ZERO || way.dot(to) > GUN_BEHIND_COS
            });
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
                    aim_pitch = carry_pitch.unwrap_or(0.15);
                }
            }
        } else if behaviour == Behaviour::Objective
            && !gunning
            && selected_objective.is_none_or(|view| view.enemy.is_none())
        {
            if let Some(objective) = selected_objective.as_ref() {
                let delta = objective.aim - eye;
                aim_yaw = yaw_to(delta);
                aim_pitch = delta.y.atan2(flat(delta).length()).clamp(-1.5, 1.5);
            }
        } else if let Some(seen) = sight.target {
            let mut at = seen.eye - Vec3::Y * 0.5;
            if let Some(w) = weapon.filter(|w| !w.melee && w.speed > 0.0) {
                let time = at.distance(eye) / w.speed;
                at += target_velocity * lead(bot, tick) * time;
                at.y += 0.5 * w.fall * time * time;
            }
            let delta = native_choice.map_or(at - eye, |c| c.direction);
            let tracked = tick.saturating_sub(brain.seen_since) as f32 * TICK;
            let error = aim_error(bot, tick, tracked, kind.aim_error_degrees);
            aim_yaw = wrap(yaw_to(delta) + error.0);
            aim_pitch = (delta.y.atan2(flat(delta).length()) + error.1).clamp(-1.5, 1.5);
            let reaction = (kind.reaction_seconds * 120.0) as u64;
            let in_reach = weapon.is_some_and(|w| at.distance(eye) <= w.reach.max(1.0) * 1.1 + 0.5);
            fire = enemy.is_some()
                && (driving.is_none() || vehicle_weapon)
                && tick >= brain.seen_since + reaction
                && in_reach
                && attack_clear
                && crew_ready
                && wrap(aim_yaw - brain.yaw).abs() < 0.1
                && (aim_pitch - brain.pitch).abs() < 0.12
                // It fires in bursts with short pauses, as a player does;
                // a charge it winds up is let go when it is ready instead.
                && (weapon.is_some_and(|w| w.charge) || cadence::bursting(bot, tick));
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
        if let Some((yaw, pitch)) = act.aim {
            (aim_yaw, aim_pitch) = (yaw, pitch);
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
            // In its band at an objective, or a swimmer fighting (no floor
            // to probe, its water all round): weave so it is not a still
            // target, and give ground if too close.
            Behaviour::Objective | Behaviour::Fight
                if wanted.is_none()
                    && hold
                    && (behaviour == Behaviour::Objective || kind.moves == Moves::Swim) =>
            {
                let side = brain.strafe_leg(tick, WEAVE_SECONDS, &|_| true, false) * 0.7;
                direction = right * side;
                if back_off {
                    direction -= forward;
                }
            }
            Behaviour::Fight if wanted.is_none() && hold => {
                let melee = weapon.is_none_or(|w| w.melee);
                let gap = enemy.map(|seen| flat(seen.feet - feet).length());
                if melee {
                    // A melee fighter closes to its band rather than
                    // circling its target.
                    if gap.is_some_and(|gap| gap > near.max(1.0) + 0.5) {
                        direction = forward;
                    } else if let Some(gap) = gap {
                        // In its band it keeps its feet moving: in, back
                        // out and aside, on its own seeded beat.
                        let room = gap + FOOTWORK_BACK <= near.max(1.0);
                        direction = footwork(bot, tick, forward, right, room);
                    }
                } else {
                    // A ranged fighter strafes one way for a while, but
                    // not where the floor ends or a wall or an ally stands
                    // that way.
                    let floor = |side: f32| {
                        let probe = feet + right * side * 0.9 + Vec3::Y * 0.5;
                        let under = super::admin_players::world_ray(
                            &self.simulation,
                            probe,
                            Vec3::NEG_Y,
                            0.5 + body.step + body.drop,
                        );
                        let wall = super::admin_players::world_ray(
                            &self.simulation,
                            feet + Vec3::Y * 0.5,
                            right * side,
                            0.9,
                        );
                        under.is_some() && wall.is_none()
                    };
                    let ally = |side: f32| {
                        allies_near.iter().any(|at| {
                            let d = flat(*at - feet);
                            d.dot(right * side) > 0.0 && d.length() < 1.5
                        })
                    };
                    let ground = |side: f32| floor(side) && !ally(side);
                    // Each leg turns back the other way, unless only this
                    // way is open. One that reaches an edge stands there
                    // until the leg is up; one that meets an ally turns
                    // away from it at once.
                    let parted = ally(brain.strafe.0) && ground(-brain.strafe.0);
                    let side =
                        brain.strafe_leg(tick, kind.fighting.strafe_seconds, &ground, parted);
                    if ground(side) {
                        direction = right * side * 0.7;
                    }
                    // Now and then it presses in a little as it strafes (a
                    // slow seeded lean, never back toward an edge), so a
                    // strafe cut short and turned back is not a pace on
                    // the spot. Never inside its band's near edge.
                    let lean = cadence::drift(bot, cadence::salt::LEAN, 0, tick, LEAN_TICKS)
                        .max(0.0)
                        * LEAN;
                    if lean > 0.0
                        && gap.is_some_and(|gap| gap > near + LEAN_ROOM)
                        && super::admin_players::world_ray(
                            &self.simulation,
                            feet + forward * 0.9 + Vec3::Y * 0.5,
                            Vec3::NEG_Y,
                            0.5 + body.step + body.drop,
                        )
                        .is_some()
                    {
                        direction += forward * lean;
                    }
                }
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
                        // Still short of being over them, it climbs past
                        // their feet first, so it clears the edge they
                        // stand back from rather than pressing on its side.
                        let climb = if across > FLY_OVER { 0.5 } else { -0.6 };
                        if feet.y < air.to.y + climb
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
                        let ranged = weapon.is_some_and(|w| !w.melee);
                        // The live enemy supplies air.to. A close ranged
                        // flyer must regain its weapon's band, including
                        // when vertical separation selected Fly over Fight.
                        let close = ranged
                            && sight.target.is_some()
                            && (back_off || feet.distance(air.to) < near);
                        if close {
                            input.jet = false;
                            let side =
                                brain.strafe_leg(tick, WEAVE_SECONDS, &|_| true, false) * 0.7;
                            direction =
                                if across > 0.1 { -toward } else { -forward } + right * side;
                        } else {
                            // Clear a blocked roof/contact horizontally
                            // instead of spending lift against the ceiling.
                            let blocked = air.roofed || ranged && state.jump.ceiling;
                            input.jet =
                                (!ranged || !blocked) && (across > 1.0 || feet.y < air.to.y + 0.5);
                            direction = if blocked {
                                if across > 0.1 { -toward } else { forward }
                            } else if feet.y > air.to.y + 1.0 && across > 1.0 {
                                toward * (across / 3.0).min(1.0)
                            } else {
                                Vec3::ZERO
                            };
                        }
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
        // A fight stands out of where a teammate's weapon will hit (`team`).
        if behaviour == Behaviour::Fight
            && let Some(out) = choices[Behaviour::Fight as usize]
                .place
                .filter(|out| *out != feet)
        {
            direction = flat(out - feet).normalize_or_zero();
        }
        if let Some(to) = act.direction {
            direction = to;
        }
        input.jump |= act.jump;
        input.crouch |= act.crouch;
        input.forward = direction.dot(forward).clamp(-1.0, 1.0);
        input.right = direction.dot(right).clamp(-1.0, 1.0);
        // Walking into something: hop, then plan again, then give up.
        let trying = input.forward != 0.0 || input.right != 0.0;
        if driving.is_none() && trying && moved < 0.01 {
            brain.stuck += 1;
        } else {
            brain.stuck = 0;
        }
        if driving.is_none() && trying && moved_across < 0.01 {
            brain.wedged += 1;
        } else {
            brain.wedged = 0;
        }
        if driving.is_none()
            && pushing.is_none()
            && brain.stuck > 20
            && (u64::from(brain.stuck) + cadence::bot_phase(bot, cadence::salt::HOP) % HOP_SPREAD)
                % 40
                < 5
        {
            input.jump = true;
        }
        let mut forget = false;
        if brain.stuck > STUCK_TICKS && wanted.is_some() {
            brain.stuck = 0;
            brain.replans += 1;
            // Stuck: what it was doing is not working.
            let cfg = &brain.kind.surprise;
            let route = brain.surprise.chosen(surprise::Domain::Route);
            brain.surprise.outcome(
                cfg,
                surprise::Domain::Behaviour,
                behaviour as u32,
                false,
                tick,
            );
            if let Some(route) = route.filter(|_| behaviour == Behaviour::Chase) {
                brain
                    .surprise
                    .outcome(cfg, surprise::Domain::Route, route, false, tick);
            }
            brain.plan.clear();
            brain.search = None;
            brain.settled = false;
            // What blocked it may be newer than the grid: look again.
            forget = true;
            if brain.replans > MAX_REPLANS {
                brain.goal = None;
                if behaviour == Behaviour::Arm {
                    // No way to the item: pass it over.
                    brain.arming.pass_over(tick);
                }
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
        // Each bot taps on its own beat, so gunners do not fire in unison.
        let beat = cadence::beat(bot, cadence::salt::FIRE, tick, 40);
        let pulse = fire && !held_down && beat && bite.is_none();
        // A charged weapon is held until letting go fires it, then let go.
        let pulse = pulse
            || fire
                && charging
                && (if vehicle_weapon {
                    charged_ready || !mounted_charging && beat
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
                let tracking_charge = last_down
                    && native_choice.is_some()
                    && image.charges()
                    && charged_control::release_only(image);
                // The unchanged participant/equipment intent owns this live
                // wind-up even if range temporarily selected Return/Wander.
                // Objective tool controls below still cancel/preempt it.
                let decision = hand_combat::trigger(
                    image,
                    image_state,
                    last_down,
                    fire && native_choice.is_some() || tracking_charge,
                    fire && matches!(native, hand_combat::Decision::Ready(_)),
                );
                desired_down = decision.down;
                cancel_hand_charge = decision.abort_charge;
            } else {
                desired_down = false;
            }
        }
        let objective_tool = selected_objective.is_some_and(|view| view.trigger.is_some());
        let previous_objective_tool = brain.objective_tool;
        brain.objective_tool = objective_tool;
        if objective_tool {
            // Do not validate a manipulation trigger against yesterday's
            // hostile weapon intent at the post-movement launch gate.
            brain.native_combat_tick = None;
            desired_down = selected_objective.unwrap().trigger.unwrap();
            cancel_hand_charge |= charging && last_down && !previous_objective_tool;
        } else if previous_objective_tool {
            desired_down = false;
            cancel_hand_charge |= charging && last_down;
        }
        // Releasing a ready mounted charge is the firing control itself.
        // Preserve that intent before `fire` becomes the requested button
        // state; cancellation would otherwise erase the native charge before
        // the ordinary trigger-release executor could consume it.
        let mounted_release = fire
            && vehicle_weapon
            && charging
            && charged_ready
            && pulse
            && last_down
            && !objective_tool
            && !previous_objective_tool;
        let mut fire_changed = desired_down != last_down;
        fire = desired_down;

        brain.fire_down = desired_down;
        if cancel_hand_charge {
            self.abort_bot_hand_charge(bot)?;
        }
        if !fire && vehicle_weapon {
            self.vehicles.set_fire(bot, false);
            if !mounted_release && let Some(w) = &mut self.vehicles.world {
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
        if let Some(line) = callout {
            self.team_say(bot, line, tick)?;
        }
        if let (Some(m), Some(seen)) = (bites, sight.target) {
            self.bot_bite(bot, seen.owner, m, tick)?;
        }
        if behaviour == Behaviour::Objective {
            if let Some(view) = selected_objective {
                if let Some(slot) = view.equip
                    && self
                        .weapons
                        .actor(ActorId(bot))
                        .is_some_and(|a| a.selected != Some(slot))
                {
                    self.abort_bot_hand_charge(bot)?;
                    self.equip_tool(bot, Some(slot))?;
                    // The newly selected image has its own trigger state.
                    fire_changed = true;
                }
                if let Some((vehicle, seat)) = view.board {
                    self.try_bot_board(bot, vehicle, seat, tick)?;
                }
            }
            self.bot_objective_act(bot, tick)?;
        }
        if (behaviour != Behaviour::Objective
            || gunning
            || selected_objective.is_some_and(|view| view.enemy.is_some()))
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
                    let target = sight.target.map(|s| s.owner);
                    self.surprise_fired(bot, native_choice, target, tick);
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
    ) -> Option<FireAdmission> {
        let brain = self.bots.brains.get(&bot)?;
        let plan_tick = tick.checked_sub(1)?;
        if brain.native_combat_tick != Some(plan_tick) {
            return None;
        }
        let intent = brain.combat.intent(plan_tick);
        let mut budget = std::mem::take(&mut self.bots.combat_budget);
        let allowed = intent.as_ref().map_or(FireAdmission::Abort, |intent| {
            hand_combat::validate_intent(self, bot, intent, direction, &mut budget)
        });
        self.bots.combat_budget = budget;
        Some(allowed)
    }
    /// Replace a speculative release with a safe native hold, without a
    /// re-press (which itself would release/restart a charged image).
    pub(super) fn bot_hold_hand_charge(&mut self, bot: OwnerId) -> Result<()> {
        self.weapon_triggers.remove(&bot);
        if let Some(brain) = self.bots.brains.get_mut(&bot) {
            brain.fire_down = true;
        }
        self.weapons.trigger(ActorId(bot), true)
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
        if let Some((_, at)) = brain.mount_anchor.as_mut() {
            *at = carry.transform_point3(*at);
        }
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
        brain.objective_threat = None;
        brain.posed = false;
        brain.target = None;
        brain.memory = None;
        brain.evidence_search.clear();
        self.embody_bot(target, &kind)?;
        let max = self.max_health(target);
        self.peers.get_mut(&target).unwrap().combat.health = max;
        Ok(())
    }
    /// Equip the best weapon in the inventory by its data
    /// (`hand_combat::item_worth`), for a weapon the native chooser does not
    /// handle (a script fires it, or a seat holds the bot), unless it holds
    /// one already that is as good (the rules may have put it in its hand).
    /// With none that attacks by its data, the first real tool.
    fn bot_arm(&mut self, bot: OwnerId) -> Result<()> {
        // Holding or reaching for something with its tool: the tool stays.
        if self.held_by(bot).is_some() || self.is_reaching(bot) {
            return Ok(());
        }
        let Some(actor) = self.weapons.actor(ActorId(bot)) else {
            return Ok(());
        };
        let scale = self.peers.get(&bot).map_or(1.0, |p| p.player.state().scale);
        let worth = |slot: usize| {
            actor.inventory[slot]
                .as_deref()
                .map_or(0.0, |id| hand_combat::item_worth(self, id, scale))
        };
        let held = actor
            .selected
            .filter(|s| *s < actor.inventory.len())
            .map_or(0.0, worth);
        let best = (0..actor.inventory.len())
            .map(|slot| (slot, worth(slot)))
            .filter(|(_, w)| *w > 0.0)
            .fold(None::<(usize, f32)>, |best, (slot, w)| {
                if best.is_none_or(|(_, b)| w > b) {
                    Some((slot, w))
                } else {
                    best
                }
            });
        // Nothing that attacks by its data (a tool that grabs, say): the
        // first that is not a building tool, unless it holds one.
        let real = |item: &Option<String>| {
            item.as_deref()
                .is_some_and(|id| !bri_weapons::CORE_TOOLS.contains(&id))
        };
        let slot = match best {
            Some((slot, w)) if w > held => Some(slot),
            Some(_) => None,
            None if actor
                .selected
                .and_then(|s| actor.inventory.get(s))
                .is_some_and(real) =>
            {
                None
            }
            None => actor.inventory.iter().position(real),
        };
        if let Some(slot) = slot
            && actor.selected != Some(slot)
        {
            let _ = self.equip_tool(bot, Some(slot));
        }
        Ok(())
    }
    /// The bot kinds as rules see them (`bot_kinds()`).
    pub(super) fn bot_kind_id(&self, bot: OwnerId) -> Option<&str> {
        self.bots
            .brains
            .get(&bot)
            .map(|brain| brain.kind.id.as_str())
    }

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

#[cfg(test)]
mod strafe_tests {
    use super::*;

    #[test]
    fn strafe_legs_vary_about_their_mean() {
        for bot in [1, 7, 42] {
            let mut brain = Brain::new(None, BotKind::default(), Vec3::ZERO, bot, 0);
            let mean = 3.5;
            let (mut tick, mut legs, mut sides) = (0u64, Vec::new(), Vec::new());
            for _ in 0..400 {
                let side = brain.strafe_leg(tick, mean, &|_| true, false);
                let until = brain.strafe.1;
                legs.push((until - tick) as f32 / 120.0);
                sides.push(side);
                tick = until;
            }
            let n = legs.len() as f32;
            let m = legs.iter().sum::<f32>() / n;
            let sd = (legs.iter().map(|l| (l - m).powi(2)).sum::<f32>() / n).sqrt();
            assert!((m - mean).abs() < 0.25, "bot {bot} mean {m}");
            assert!(sd / m > 0.25, "bot {bot} cv {}", sd / m);
            let kept = sides.windows(2).filter(|w| w[0] == w[1]).count();
            assert!(kept > 40 && kept < 160, "bot {bot} kept on {kept} times");
        }
    }

    #[test]
    fn a_strafe_turns_at_once_from_a_closed_side() {
        let mut brain = Brain::new(None, BotKind::default(), Vec3::ZERO, 3, 0);
        let side = brain.strafe_leg(0, 3.5, &|_| true, false);
        let other = brain.strafe_leg(1, 3.5, &|s| s != side, true);
        assert_eq!(other, -side);
    }
}

#[cfg(test)]
mod footwork_tests {
    use super::*;

    #[test]
    fn melee_footwork_steps_in_out_aside_and_stands_unlike_its_neighbours() {
        let (forward, right) = (Vec3::NEG_Z, Vec3::X);
        let steps = |bot| {
            (0..1200)
                .map(|t| footwork(bot, t, forward, right, true))
                .collect::<Vec<_>>()
        };
        for bot in 1..=4 {
            let s = steps(bot);
            let into = s.iter().filter(|d| d.dot(forward) > 0.0).count();
            let out = s.iter().filter(|d| d.dot(forward) < 0.0).count();
            let aside = s.iter().filter(|d| d.dot(right).abs() > 0.0).count();
            let still = s.iter().filter(|d| **d == Vec3::ZERO).count();
            assert!(
                into > 150 && out > 150 && aside > 150 && still > 150,
                "bot {bot}: {into} {out} {aside} {still}"
            );
            // Net drift stays small: it does not walk off its band.
            let net: f32 = s.iter().map(|d| d.dot(forward)).sum::<f32>() / s.len() as f32;
            assert!(net.abs() < 0.2, "bot {bot} drifts {net}");
        }
        assert_ne!(steps(1), steps(2));
        // At the edge of its reach it never steps back out of it.
        for bot in 1..=4 {
            assert!((0..1200).all(|t| footwork(bot, t, forward, right, false).dot(forward) >= 0.0));
        }
    }
}

/// What each of a bot's options would do (`team::Choice`): where it would
/// stand, what it acts on and whose seat it takes. A fight stands where it
/// is, or at `exit`, the nearest place out of a teammate's line of fire.
#[allow(clippy::too_many_arguments)]
fn team_choices(
    opportunity: Option<interactions::Opportunity>,
    enemy: Option<Seen>,
    memory: Option<Knowledge>,
    arm: Option<Vec3>,
    objective: Option<objectives::View>,
    feet: Vec3,
    home: Vec3,
    exit: Option<Vec3>,
    carries: Option<(u64, Vec3)>,
) -> [team::Choice; 10] {
    use claims::{Resource, Target};
    let mut c = [team::Choice::default(); 10];
    let at = |b: Behaviour| b as usize;
    if let Some(o) = opportunity {
        c[at(Behaviour::Interact)] = team::Choice {
            place: Some(o.point),
            target: match o.resource {
                Resource::Body { vehicle } => Some(Target::Object(vehicle)),
                Resource::Seat { .. } => None,
            },
            seat: match o.resource {
                Resource::Seat { vehicle, .. } => Some(vehicle),
                Resource::Body { .. } => None,
            },
            ..Default::default()
        };
    }
    if let Some(seen) = enemy {
        let target = Some(Target::Player(seen.owner));
        c[at(Behaviour::Fight)] = team::Choice {
            place: Some(exit.unwrap_or(feet)),
            stand: true,
            target,
            ..Default::default()
        };
        c[at(Behaviour::Chase)].target = target;
        c[at(Behaviour::Fly)].target = target;
    }
    // A search goes to look where the enemy was: searchers crowd one spot,
    // but one coming to back up an ally on that enemy is not a pile-on.
    if let Some(k) = memory {
        c[at(Behaviour::Search)].place = Some(k.at);
    }
    c[at(Behaviour::Arm)].place = arm;
    c[at(Behaviour::Return)].place = Some(home);
    if let Some(v) = objective {
        c[at(Behaviour::Objective)] = team::Choice {
            place: Some(v.point),
            target: v
                .resource
                .map(|r| Target::Object(r.vehicle()))
                .or(v.enemy.map(Target::Player)),
            seat: v.board.map(|(vehicle, _)| vehicle),
            ..Default::default()
        };
    }
    for choice in &mut c {
        choice.carries = carries;
    }
    c
}
