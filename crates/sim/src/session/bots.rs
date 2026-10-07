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
use crate::nav::{Body, Found, Ground, Mode, Nav, Search, Waypoint};
use behaviour::{Behaviour, Situation};
use bri_content::passage::{Way, carried_yaw};
use bri_package_runtime::ops::ObjectRef;
use bri_weapons::ActorId;

mod act;
mod arming;
mod behaviour;
pub(crate) mod cadence;
mod charged_control;
pub(super) use charged_control::FireAdmission;
mod claims;
mod combat_objectives;
mod contest;
mod cooldown;
mod explore;
mod extras;
mod fire;
use fire::{Hand, Shot};
#[path = "bots/combat.rs"]
mod hand_combat;
mod hearing;
mod interactions;
mod lifecycle;
mod looks;
mod objectives;
mod package_objectives;
mod perception;
pub use perception::BotNotice;
pub(super) use perception::Stimulus;
mod physical_objectives;
mod planning;
mod search_memory;
mod sight;
mod sightlines;
pub(super) use sightlines::{Subject as SightSubject, Urgency as SightUrgency};
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
    /// The leg of its route it is on: walk, swim, jet or drive (none: no
    /// route).
    pub leg: &'static str,
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
    /// The last glance or reaction and why (`perception`).
    pub noticed: Option<BotNotice>,
    /// Why it chose as it did (`surprise`): its drives and the last
    /// decision at each choice point, every term's contribution.
    pub surprise: BotSurpriseView,
    /// How teammates' intents moved its last choice (`team`).
    pub team: BotTeamView,
    /// What the act stage did (`act::Acted::line`).
    pub acted: String,
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
/// Whole ticks in `seconds` (none for less than one, or a negative time).
fn ticks(seconds: f32) -> u64 {
    (seconds * 120.0) as u64
}
/// Ticks before a bot's failing step is told again.
const BOT_FAILURE_EVERY: u64 = 60 * 120;
/// Seconds a rules bot waits past its game's respawn time, as a seeded range.
const RULES_RESPAWN: (f32, f32) = (0.3, 1.5);
/// Seconds a brick bot waits past its respawn time, as a seeded range.
const BRICK_RESPAWN: (f32, f32) = (0.75, 1.75);
/// How far off a weapon at full volume is heard as fighting to go and
/// look for (`Session::hear_fighting`)...
const FIGHT_HEARING: f32 = 128.0;
/// ...and within what share of its distance the spot is known.
const FIGHT_HEARD_ROUGHLY: f32 = 0.15;
/// Farthest from its start a path may lead, across.
const SEARCH_BOUND: f32 = 72.0;
/// How far ahead a step looks for a portal it would go through.
const PORTAL_REACH: f32 = 0.8;
/// The pitch of a bot looking at what it handles (an emote, a tool).
const LOOK_DOWN: f32 = -0.3;
/// How long the grid avoids a place a bot got nowhere walking into.
const AVOID_TICKS: u64 = 120 * 30;
/// Plans in a row that got stuck before a bot drops its goal.
const MAX_REPLANS: u32 = 3;
/// The mean seconds of one weave leg at an objective or in water.
const WEAVE_SECONDS: f32 = 0.75;
/// A goal that moves less than this keeps its route (`Brain::set_goal_near`).
const GOAL_SLACK: f32 = 0.2;
/// A ranged fighter strafes one way about this long before turning back.
/// It stands at a ledge or a wall until then, and turns away from an ally
/// at once. A melee fighter does not strafe: it closes to its band.
const STRAFE_SECONDS: f32 = 3.5;
/// How far a driver follows a fight from where it took the controls.
const DRIVE_CHASE_RADIUS: f32 = 96.0;
/// How often a strafe leg turns back rather than keeps on.
const TURN_BACK: f32 = 0.7;
/// Ticks between the aim error's seeded points; it eases between them.
const ERROR_TICKS: u64 = 48;
/// How much of the kind's aim error stays after a long track.
const ERROR_FLOOR: f32 = 1.0 / 3.0;
/// How far a bot's lead strays from the true intercept, as a fraction.
const LEAD_STRAY: f32 = 0.15;
/// Ticks between the lead's seeded points.
const LEAD_TICKS: u64 = 120;
/// Within this of where it began trying to get away, a bot has got
/// nowhere (`trapped`).
const PINNED: f32 = 2.0;
/// Seconds of trying and getting nowhere before it thinks of respawning
/// itself, and how many more before it is sure: its own unsticking (a
/// hop, plans again, a new goal) has had its turn by then.
const TRAPPED_AFTER: f32 = 10.0;
const TRAPPED_RAMP: f32 = 10.0;
/// Ticks without a try to get away after which it is not trapped.
const TRY_GAP: u64 = 120 * 10;
/// How sure it is that it is trapped, 0 to 1, from how long it has kept
/// trying to get away from one spot (`Brain::pinned`).
fn trapped(pinned: Option<(Vec3, u64, u64)>) -> f32 {
    pinned.map_or(0.0, |(_, since, last)| {
        ((last.saturating_sub(since) as f32 / 120.0 - TRAPPED_AFTER) / TRAPPED_RAMP).clamp(0.0, 1.0)
    })
}
/// How far either way a search sweeps its view (all round), and the
/// ticks between the points it looks to.
const SWEEP: f32 = std::f32::consts::PI;
const SWEEP_TICKS: u64 = 120;
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
    /// Blasts and sounds since bots last stepped (`perception`).
    stimuli: Vec<Stimulus>,
    /// Sounds players' weapons made since bots last stepped: who, where
    /// and how loud (`Session::hear_fighting`).
    noises: Vec<(OwnerId, Vec3, f32)>,
    /// The tick's shared sight-ray budget (`sightlines`).
    sightlines: std::sync::Mutex<sightlines::Sightlines>,
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
    /// A chase point whose best route ends beyond its reach (`reach`):
    /// that enemy is not worth chasing while they stay there.
    out_of_reach: Option<Vec3>,
    /// How far across and up it hurts from where it stands.
    reach: (f32, f32),
    /// Where the enemy it chases stands (their feet, not their eyes).
    chase_feet: Option<Vec3>,
    segment_anchor: Vec3,
    next_wander: u64,
    /// The walk leg's net progress (`route::Progress`): the one judge of
    /// whether it is stuck.
    progress: crate::route::Progress,
    replans: u32,
    /// What the act stage did last tick (`act::Acted`), for the readout.
    acted: act::Acted,
    /// The way it faced as it began sweeping the spot it searches.
    sweep_from: Option<f32>,
    /// Where it has been trying to get away from, since when, and when it
    /// last tried (`trapped`).
    pinned: Option<(Vec3, u64, u64)>,
    /// Where this life began, and whether it began by respawning itself:
    /// trapped again there, a respawn would only bring it back (`trapped`).
    life: Option<(Vec3, bool)>,
    /// Current aim, turned toward the wanted one at the kind's rate.
    yaw: f32,
    pitch: f32,
    target: Option<OwnerId>,
    /// Tick the current target was first seen.
    seen_since: u64,
    /// Its aim error now, radians (yaw, pitch): a seeded drift.
    error: (f32, f32),
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
    /// The kind its brick made, while a bite has turned it into another
    /// (`BotMelee::converts_below`); it comes back as this one.
    born: Option<BotKind>,
    next_interaction: u64,
    object_cursor: usize,
    push_contact: Option<(u64, u64)>,
    push_anchor: Option<(u64, Vec3)>,
    /// Headway of the chassis it drives: where it last got somewhere.
    vehicle_headway: crate::route::Headway<Vec3>,
    vehicle_since: Option<(u64, u64)>,
    /// The jet leg of its route it is flying, if any.
    jet_leg: Option<crate::route::JetLeg>,
    /// The gear its driving is in (`route::gear`).
    drive_gear: crate::route::Gear,
    /// Where it took a mount's controls (the vehicle and its feet then):
    /// the anchor of a driver's leash.
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
    /// Glances and reaction delays (`perception`).
    perception: perception::State,
    /// The small extra options' memory ([`extras`]).
    extras: extras::State,
    /// Where it has been and what it knows of enemies, for exploring.
    explore: explore::Explore,
}
/// A bot holding something with a tool that holds (the Gravity Gun, or
/// any tool whose trigger reaches and holds: `reach`, `hold`) carries it
/// out into open space and flings it with a swing of its aim.
#[derive(Clone, Copy, Debug)]
struct Carry {
    /// Where it is open to throw from (`Session::bot_open_spot`).
    to: Spot,
    since: u64,
    /// When the throwing swing began.
    swing: Option<u64>,
}
/// Where a carry goes to throw: still looking (from this candidate on, as
/// the shared ray budget allows), or found: a place, or here (open here,
/// nowhere near, or no way there).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Spot {
    Looking(usize),
    Found(Option<Vec3>),
}
impl Spot {
    fn place(self) -> Option<Vec3> {
        match self {
            Spot::Found(at) => at,
            Spot::Looking(_) => None,
        }
    }
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
/// How far a bot follows an enemy from a spot: past `radius` from `at` it
/// lets the chase and what it remembers go.
#[derive(Clone, Copy, Debug)]
struct Leash {
    at: Vec3,
    radius: f32,
}
impl Leash {
    fn away(&self, feet: Vec3) -> f32 {
        flat(feet - self.at).length()
    }
    fn holds(&self, feet: Vec3) -> bool {
        self.away(feet) <= self.radius
    }
}
impl Brain {
    /// Whether it is kept near a spot: a bot from a brick strolls round
    /// its brick and goes back to it. A rules bot has no spot of its own:
    /// it plays the whole map, as a player does, strolls from wherever it
    /// is and follows gunfire it hears (`hear_fighting`).
    fn tethered(&self) -> bool {
        self.brick.is_some()
    }
    /// Its chase leash. A driver keeps within `DRIVE_CHASE_RADIUS` of where
    /// it took the controls (a mount covers ground a walker does not); on
    /// foot a tethered bot keeps near its brick, and an untethered one has
    /// no bound (a leash to where it spawned had it drop a chase at the
    /// edge and wander back, then see them and chase again).
    fn leash(&self, driving: bool) -> Leash {
        let (at, radius) = match (driving, self.mount_anchor) {
            (true, Some((_, at))) => (at, DRIVE_CHASE_RADIUS),
            (true, None) => (self.leash, DRIVE_CHASE_RADIUS),
            (false, _) if !self.tethered() => (self.leash, f32::INFINITY),
            (false, _) => (self.leash, self.kind.chase_radius),
        };
        Leash { at, radius }
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
            out_of_reach: None,
            reach: (3.0, 2.0),
            chase_feet: None,
            segment_anchor: home,
            next_wander: 0,
            progress: crate::route::Progress::default(),
            replans: 0,
            acted: Default::default(),
            sweep_from: None,
            pinned: None,
            life: None,
            yaw: 0.0,
            pitch: 0.0,
            target: None,
            seen_since: 0,
            error: (0.0, 0.0),
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
            born: None,
            next_interaction: 0,
            object_cursor: 0,
            push_contact: None,
            push_anchor: None,
            vehicle_headway: Default::default(),
            vehicle_since: None,
            jet_leg: None,
            drive_gear: crate::route::Gear::Forward,
            mount_anchor: None,
            brick_team: None,
            named: String::new(),
            perception: Default::default(),
            surprise: surprise::Mind::new(bot),
            chase_offset: Vec3::ZERO,
            team: team::State::default(),
            extras: Default::default(),
            explore: Default::default(),
        }
    }
    fn random(&mut self) -> f32 {
        perception::draw(&mut self.rng)
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
            until = tick + ticks(leg(u, mean));
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
            self.progress.reset();
            self.settled = false;
            self.partial_route = false;
        }
    }
    /// Head for `goal`, unless it already heads for the same kind of goal
    /// within `GOAL_SLACK` of it: a point that drifts a little (an item, an
    /// objective's stand) does not restart its route every tick.
    fn set_goal_near(&mut self, goal: Goal) {
        let same = self.goal.is_some_and(|now| {
            std::mem::discriminant(&now) == std::mem::discriminant(&goal)
                && now.point(Vec3::ZERO).distance(goal.point(Vec3::ZERO)) < GOAL_SLACK
        });
        if !same {
            self.set_goal(Some(goal));
        }
    }
    /// The goal of going after an enemy: standing its ground in its band
    /// (`fight`), after the enemy in sight, or to where one was. Whether it
    /// stands, and whether it gives ground (closer than `near`).
    /// `reach`: how near its body counts as at a search point.
    fn pursue(
        &mut self,
        enemy: Option<Seen>,
        fight: bool,
        near: f32,
        feet: Vec3,
        reach: f32,
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
                // A goal is kept while the enemy stays near it, unless the
                // search to it came back empty well short of it: then the
                // goal follows the enemy wherever it now stands, so the bot
                // searches again rather than stand settled out of reach.
                let moved_on = match self.goal {
                    Some(Goal::Chase(p)) => {
                        p.distance(to) > 2.5
                            || self.settled
                                && self.plan.is_empty()
                                && flat(p - feet).length() > near
                    }
                    _ => true,
                };
                if moved_on {
                    self.set_goal(Some(Goal::Chase(to)));
                }
                (false, false)
            }
            (None, Some(knowledge)) => {
                // Settled short of the spot (no route, or one that ends
                // short of it walked to its end) is as near as it gets: on
                // to the next spot, not standing there.
                let failed = matches!(self.goal, Some(Goal::Search(_)))
                    && self.settled
                    && self.plan.is_empty();
                let next = self
                    .evidence_search
                    .next(knowledge, feet, tick, failed, reach);
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
        if !self.tethered() {
            self.home = feet;
        }
        if self.goal.is_none() && tick >= self.next_wander {
            let around = if self.tethered() { self.home } else { feet };
            let angle = self.random() * std::f32::consts::TAU;
            let radius = self.random() * self.kind.wander_radius;
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
/// a little over a body's height, so some of its shot lands on a target
/// that strafes and hops, and not every pellet (`fair_hit_rate`).
const SPREAD_BODY: f32 = 3.5;
/// The nearest a scattering weapon's band ends, however wide its spread.
const SPREAD_MIN_FAR: f32 = 3.0;
/// The share of a scattering weapon's spread kept clear of allies: its
/// pellets fall evenly across the cone, so the outer edge is thin.
const SPREAD_CLEAR: f32 = 0.6;
/// How far above the feet a splash weapon aims.
const FEET_AIM: f32 = 0.2;
/// The share of its reach a melee weapon swings from: inside it, so a
/// step back by the target does not leave the swing short.
const MELEE_BAND_SHARE: f32 = 0.8;
/// The farthest a melee weapon swings from however short it reaches: about
/// an arm's length, so a body never presses into its target to swing.
const MELEE_BAND_MIN: f32 = 1.2;
/// The share of its reach a ranged weapon fights from: inside it, so its
/// shots still land on a target backing off.
const RANGED_BAND_SHARE: f32 = 0.7;
/// The nearest and farthest a ranged weapon's band ends, whatever it
/// reaches: closer is a brawl, farther is past where a player picks a
/// target out to shoot at.
const RANGED_BAND_FAR: std::ops::RangeInclusive<f32> = 6.0..=40.0;
/// The share of its band's far end the near end may come out to, so a band
/// always has room to back into.
const RANGED_NEAR_SHARE: f32 = 0.75;
/// The least room between a ranged band's ends, wider for a single shot
/// than a scattering one (which needs to close in to land its spread).
const BAND_ROOM: f32 = 4.0;
const SPREAD_BAND_ROOM: f32 = 2.0;
/// Room kept past a weapon's own blast so a near miss does not catch its
/// holder, and the least standoff for any ranged weapon.
const BLAST_CLEARANCE: f32 = 3.0;
const MIN_STANDOFF: f32 = 5.0;
/// An attack that reaches less than this is a swing or a stab, fought up
/// close, whatever its image says.
const MELEE_REACH_LIMIT: f32 = 6.0;
/// How far a melee image reaches when neither its data nor a projectile
/// says.
const MELEE_DEFAULT_REACH: f32 = 3.0;
impl Weapon {
    /// The nearest a ranged weapon with this blast is fought from, when its
    /// data does not say.
    fn standoff(splash: f32) -> f32 {
        (splash + BLAST_CLEARANCE).max(MIN_STANDOFF)
    }
    /// Closest and farthest it likes to fight from.
    fn band(&self) -> (f32, f32) {
        if self.melee {
            (0.0, (self.reach * MELEE_BAND_SHARE).max(MELEE_BAND_MIN))
        } else {
            let mut far = (self.reach * RANGED_BAND_SHARE)
                .clamp(*RANGED_BAND_FAR.start(), *RANGED_BAND_FAR.end())
                .min(self.reach);
            // A scattering weapon lands its shot where its spread is still
            // about a body wide: farther, most of it flies past.
            if self.spread > 0.0 {
                far = far.min((SPREAD_BODY / self.spread.tan()).max(SPREAD_MIN_FAR));
            }
            let near = self
                .near
                .unwrap_or(Self::standoff(self.splash))
                .min(far * RANGED_NEAR_SHARE);
            let room = if self.spread > 0.0 {
                SPREAD_BAND_ROOM
            } else {
                BAND_ROOM
            };
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
            .filter(|b| b.tethered())
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
                    expires: tick.saturating_add(ticks(self.brains[&bot].kind.memory_seconds)),
                },
            );
        }
    }
}

fn yaw_to(delta: Vec3) -> f32 {
    delta.x.atan2(-delta.z)
}
/// Easing between seeded points pulls a drift toward zero: its spread is
/// sqrt(0.743) of one uniform draw's (the smoothstep's mean square). The
/// aim error is scaled back up to a uniform draw's spread, so `size` (the
/// kind's aim error, `perception`'s tracking lag) keeps its meaning.
const DRIFT_SPREAD: f32 = 1.08;
/// The aim error in radians (yaw, pitch) at `size`: a smooth seeded
/// drift within about it, so it never jumps and never settles.
fn aim_error(bot: OwnerId, tick: u64, size: f32) -> (f32, f32) {
    let salt = cadence::salt::AIM;
    let size = size * DRIFT_SPREAD;
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

/// How far below `at` the floor is, within a step up and a drop `body`
/// walks down: `None` where it would walk off into a fall.
fn floor_below(simulation: &crate::simulation::Simulation, at: Vec3, body: &Body) -> Option<f32> {
    super::admin_players::world_ray(
        simulation,
        at + Vec3::Y * 0.5,
        Vec3::NEG_Y,
        0.5 + body.step + body.drop,
    )
}
/// About how long a hop is in the air, in seconds.
const HOP_FLIGHT: f32 = 0.8;
/// A hop keeps the way it was moving: whether it comes down on floor
/// (a bot hopping at a deck's edge went off it).
fn hop_lands(simulation: &crate::simulation::Simulation, feet: Vec3, velocity: Vec3) -> bool {
    let drift = flat(velocity) * HOP_FLIGHT;
    super::admin_players::world_ray(simulation, feet + drift + Vec3::Y * 0.5, Vec3::NEG_Y, 1.5)
        .is_some()
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
/// What a bot has decided this tick, for [`Session::bot_act`] to carry out.
struct Act {
    input: MoveInput,
    wanted: Option<Waypoint>,
    behaviour: Behaviour,
    weapon: Option<Weapon>,
    bite: Option<crate::bot_kind::BotMelee>,
    native: hand_combat::Decision,
    native_choice: Option<hand_combat::Choice>,
    target: Option<Seen>,
    /// It means to attack now.
    fire: bool,
    grabbing: bool,
    gunning: bool,
    vehicle_weapon: bool,
    charged_ready: bool,
    mounted_charging: bool,
    objective: Option<objectives::View>,
    /// A goof's trigger (a spray can held, a tool clicked), which takes the
    /// trigger as an objective's tool does.
    goof_trigger: Option<bool>,
    callout: Option<String>,
}
/// What one bot sees this tick.
struct Sight {
    target: Option<Seen>,
}

impl Session {
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
        let tick = self.simulation.state().tick;
        for (body, nav) in &mut self.bots.navs {
            for (min, max) in &changes {
                nav.invalidate(*min, *max, body);
            }
            nav.begin_tick();
            nav.set_now(tick);
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
                        && brain.kind.weight("objective") > 0.0
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
        self.bots.stimuli.clear();
        self.hear_alerts(tick);
        self.hear_fighting(tick);
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
    /// A new game, round or team: what the bot knew of its old one goes.
    fn bot_note_context(&mut self, bot: OwnerId) {
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
    }
    /// A dead bot asks to come back once its wait is up, and starts its
    /// next life knowing nothing of this one.
    fn step_dead_bot(&mut self, bot: OwnerId, tick: u64) -> Result<()> {
        let respawn_tick = self.peers[&bot].combat.respawn_tick;
        // A brick's bot comes back about a second after it may; a rules
        // bot soon after its game lets it (Slayer's bot respawn time).
        // Each death adds its own seeded delay, so bots one blast killed
        // do not all come back on the same tick.
        let (lo, hi) = if self.bots.by_rules.contains_key(&bot) {
            RULES_RESPAWN
        } else {
            BRICK_RESPAWN
        };
        let delay = cadence::spread(bot, cadence::salt::RESPAWN, respawn_tick, lo, hi);
        let wait = ticks(delay);
        if tick >= respawn_tick + wait {
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
            brain.rehome = !brain.tethered();
            brain.leash = brain.home;
            self.bots.claims.release_owner(bot);
            self.bots.claims.forget(bot);
            brain.vehicle_since = None;
            brain.vehicle_headway.clear();
            brain.fire_down = false;
            brain.objective_tool = false;
            brain.surprise.new_life();
            brain.chase_offset = Vec3::ZERO;
            brain.pinned = None;
            // A life it ended itself begins where respawning brought it.
            brain.life = brain
                .life
                .filter(|(_, ended)| *ended)
                .map(|_| (Vec3::NAN, true));
        }
        Ok(())
    }
    /// A bot the rules hold still, or one a player rides or carries: it
    /// stands (or is moved), holding its fire, and thinks no further.
    /// Whether it was held.
    fn step_held_bot(&mut self, bot: OwnerId, tick: u64) -> Result<bool> {
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
            return Ok(true);
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
            return Ok(true);
        }
        Ok(false)
    }
    /// Its kind's own rules each tick: a swimmer out of water in a
    /// mini-game lasts only so long (the Shark's `hFishOutOfWater`), and its
    /// emote plays once each life (a zombie's arms out ahead). Whether it
    /// died of them.
    fn bot_kind_rules(
        &mut self,
        bot: OwnerId,
        wet: bool,
        feet: [f32; 3],
        tick: u64,
    ) -> Result<bool> {
        let gasping = self.bots.brains[&bot]
            .kind
            .out_of_water_seconds
            .filter(|_| !wet && self.game_of(bot).is_some());
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        brain.dry = if gasping.is_some() { brain.dry + 1 } else { 0 };
        if gasping.is_some_and(|seconds| brain.dry as f32 >= seconds * 120.0) {
            brain.dry = 0;
            self.kill(bot, None, combat::DamageKind::Suicide)?;
            return Ok(true);
        }
        // Its kind's emote, once each life (a zombie's arms out ahead).
        if !brain.posed {
            brain.posed = true;
            if let Some(name) = brain.kind.emote.clone() {
                self.emote_cue(
                    tick,
                    crate::presentation::CueKind::Emote { actor: bot, name },
                    feet,
                );
            }
        }
        Ok(false)
    }
    /// What it does with its hands and feet this tick, once its mind is
    /// made up: the trigger (tapped, held or let go to fire a charge), the
    /// move, a callout, a bite, an objective's tool and seat, and the
    /// weapon it takes out for the fight.
    fn bot_act(&mut self, bot: OwnerId, tick: u64, sequence: u64, act: Act) -> Result<()> {
        let Act {
            input,
            wanted,
            behaviour,
            weapon,
            bite,
            native,
            native_choice,
            target,
            fire,
            grabbing,
            gunning,
            vehicle_weapon,
            charged_ready,
            mounted_charging,
            objective,
            goof_trigger,
            callout,
        } = act;
        let brain = self.bots.brains.get_mut(&bot).unwrap();
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
        let objective_tool = objective.is_some_and(|view| view.trigger.is_some());
        let previous_objective_tool = brain.objective_tool;
        brain.objective_tool = objective_tool;
        if objective_tool {
            // Do not validate a manipulation trigger against yesterday's
            // hostile weapon intent at the post-movement launch gate.
            brain.native_combat_tick = None;
            desired_down = objective.unwrap().trigger.unwrap();
            cancel_hand_charge |= charging && last_down && !previous_objective_tool;
        } else if previous_objective_tool {
            desired_down = false;
            cancel_hand_charge |= charging && last_down;
        }
        if let Some(down) = goof_trigger {
            desired_down = down;
        }
        brain.acted.trigger = if goof_trigger.is_some() {
            Some(act::Trigger::Goof)
        } else if objective_tool {
            Some(act::Trigger::Objective)
        } else {
            (fire || desired_down).then_some(act::Trigger::Fight)
        };
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
        if let (Some(m), Some(seen)) = (bites, target) {
            self.bot_bite(bot, seen.owner, m, tick)?;
        }
        if behaviour == Behaviour::Objective {
            if let Some(view) = objective {
                // The step's tool slot as planned; one emptied since (the
                // item dropped, thrown or taken by the rules) fails the step,
                // which plans again, rather than the bot's whole turn.
                let filled = |slot: usize| {
                    self.weapons
                        .actor(ActorId(bot))
                        .is_some_and(|a| a.inventory.get(slot).is_some_and(Option::is_some))
                };
                if let Some(slot) = view.equip
                    && !filled(slot)
                {
                    self.bots
                        .brains
                        .get_mut(&bot)
                        .unwrap()
                        .objective
                        .fail(tick, "objective tool slot emptied");
                } else if let Some(slot) = view.equip
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
            || objective.is_some_and(|view| view.enemy.is_some()))
            && target.is_some()
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
                    let target = target.map(|s| s.owner);
                    self.surprise_fired(bot, native_choice, target, tick);
                }
            }
        }
        Ok(())
    }
    /// The way to its goal: swum straight in water; otherwise a route the
    /// planner searches toward it a slice at a time, waypoints taken as
    /// reached, a chase given up where its best route cannot reach the
    /// enemy (unless a door it may open stands across the way), and a near
    /// interaction point finished by the ordinary motor. The next waypoint.
    #[allow(clippy::too_many_arguments)]
    fn bot_path(
        &mut self,
        bot: OwnerId,
        feet: Vec3,
        body: &Body,
        costs: crate::route::Costs,
        swim: Option<f32>,
        state: &crate::player::PlayerState,
        tick: u64,
    ) -> Option<Waypoint> {
        let brain = self.bots.brains.get_mut(&bot).unwrap();
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
                    wanted = Some(Waypoint::walk(to));
                }
            }
        } else if let Some(goal) = brain.goal {
            let point = goal.point(home);
            if brain.plan.is_empty() && brain.search.is_none() && !brain.settled {
                brain.search = Some(Search::with(feet, point, SEARCH_BOUND, costs));
            }
            // Where a chase's route ends short of its enemy, and the enemy.
            let mut gave_up = None;
            if brain.search.is_some() {
                let physics = &self.simulation.physics;
                let simulation = &self.simulation;
                let terrain = |o: Vec3, d: Vec3, r: f32| simulation.terrain_ray(o, d, r);
                let waters = simulation.liquids();
                // Living bodies a takeoff must not climb into, nor a
                // pulled straight walk cut through (`nav::pull`).
                let (bodies, motions): (Vec<(Vec3, Vec3)>, Vec<Vec3>) = {
                    self.peers
                        .iter()
                        .filter(|(o, p)| **o != bot && p.combat.alive)
                        .map(|(_, p)| {
                            let at = Vec3::from(p.player.state().feet);
                            let half = p.player.tuning().width * 0.5;
                            (
                                (
                                    at - Vec3::new(half, 0.0, half),
                                    at + Vec3::new(half, p.player.tuning().stand_height, half),
                                ),
                                Vec3::from(p.player.state().velocity),
                            )
                        })
                        .unzip()
                };
                let ground = Ground {
                    physics,
                    terrain: &terrain,
                    passages: simulation.passages(),
                    waters: &waters,
                    bodies: &bodies,
                    motions: &motions,
                };
                let at = match self.bots.navs.iter().position(|(b, _)| *b == *body) {
                    Some(at) => at,
                    None => {
                        let mut nav = Nav::default();
                        nav.begin_tick();
                        self.bots.navs.push((*body, nav));
                        self.bots.navs.len() - 1
                    }
                };
                let (bots_navs, brains) = (&mut self.bots.navs, &mut self.bots.brains);
                let nav = &mut bots_navs[at].1;
                let brain = brains.get_mut(&bot).unwrap();
                if let Some(found) = brain.search.as_mut().unwrap().step(nav, &ground, body) {
                    brain.search = None;
                    match found {
                        Found::Path(path) if !path.is_empty() => {
                            brain.partial_route = false;
                            if matches!(brain.goal, Some(Goal::Chase(_))) {
                                brain.out_of_reach = None;
                            }
                            brain.plan = crate::nav::pull(&ground, body, feet, path);
                        }
                        Found::Partial(path) if !path.is_empty() => {
                            // The best route to an enemy ends where it cannot
                            // hurt them: chasing them there is worth nothing.
                            if let (Some(Goal::Chase(p)), Some(end)) = (brain.goal, path.last()) {
                                let (across, up) = brain.reach;
                                if let Some(at) = brain.chase_feet.filter(|_| !body.conservative)
                                    && (flat(at - end.feet).length() > across
                                        || at.y - end.feet.y > up)
                                {
                                    brain.out_of_reach = Some(p);
                                    gave_up = Some((end.feet, at));
                                }
                            }
                            brain.partial_route = true;
                            brain.segment_anchor = feet;
                            brain.plan = crate::nav::pull(&ground, body, feet, path);
                        }
                        // Already as close as it gets, or nowhere to stand.
                        _ => brain.plan.clear(),
                    }
                }
            }
            // A door it may open stands across the way on from where the
            // route ends: the enemy is in reach once it is clicked open.
            if let Some((end, at)) = gave_up
                && self.bot_opens_way(end, at)
            {
                self.bots.brains.get_mut(&bot).unwrap().out_of_reach = None;
            }
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            while let Some(next) = brain.plan.first() {
                let d = next.feet - feet;
                // A chassis is at a point once its side is: half its
                // footprint, never a pedestrian's tolerance.
                let near = if body.conservative {
                    (body.width * 0.5).max(1.2)
                } else {
                    0.4
                };
                // One it got nowhere toward, within a step of it (wedged
                // on a door jamb), it takes as reached, if more of the
                // route follows. (Not on the way to work a body or a brick:
                // pressing against those is the work.)
                let wedged = brain.progress.stalls() > 0
                    && brain.plan.len() > 1
                    && flat(d).length() < 1.0
                    && !matches!(brain.goal, Some(Goal::Objective(_) | Goal::Interact(_)));
                // One through an opening is reached by going through; one
                // swum to by being over it, at whatever depth; one flown to
                // by standing on it after the landing.
                let reached = match next.mode {
                    Mode::Walk => {
                        next.through.is_none()
                            && (flat(d).length() < near || wedged)
                            && d.y.abs() < body.step + 0.5
                    }
                    Mode::Swim => next.through.is_none() && flat(d).length() < near.max(0.6),
                    Mode::Jet { .. } => {
                        state.grounded && flat(d).length() < 2.0 && d.y.abs() < body.step + 0.5
                    }
                };
                if reached {
                    brain.plan.remove(0);
                    // Reaching a waypoint is getting somewhere.
                    brain.progress.reset();
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
            // Standing over a drop's landing, at the very edge or on a body
            // above it (a vehicle's roof, which a route from the floor
            // beneath starts under): heading for the landing itself just
            // wobbles there, so walk on toward the first of what comes
            // after it that is not underfoot until the drop carries it down.
            if let Some(next) = wanted.as_mut()
                && next.through.is_none()
                && state.grounded
                && next.feet.y < feet.y - body.step - 0.05
                && flat(next.feet - feet).length() < 0.6
            {
                let beyond = brain
                    .plan
                    .iter()
                    .skip(1)
                    .map(|w| w.feet)
                    .find(|at| flat(*at - feet).length() > 0.6)
                    .unwrap_or(point);
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
                wanted = Some(Waypoint::walk(point));
            }
        }
        wanted
    }
    fn step_bot(&mut self, bot: OwnerId, tick: u64) -> Result<()> {
        if self.bots.failing == Some(bot) {
            anyhow::bail!("this bot's steps were made to fail");
        }
        if self.bots.brains[&bot].objective.drive(self, bot).is_none() {
            self.promote_bot_seat(bot)?;
        }
        self.bot_note_context(bot);
        let Some(peer) = self.peers.get(&bot) else {
            return Ok(());
        };
        if !peer.combat.alive {
            return self.step_dead_bot(bot, tick);
        }
        if self.step_held_bot(bot, tick)? {
            return Ok(());
        }
        if !self.seated(bot) {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.vehicle_since = None;
            brain.vehicle_headway.clear();
            brain.mount_anchor = None;
        }
        let peer = &self.peers[&bot];
        let state = peer.player.state().clone();
        let own_feet = Vec3::from(state.feet);
        let eye = self
            .bot_weapon_origin(bot)
            .unwrap_or_else(|| peer.player.eye());
        // The legs its body can take now, costed from its own tuning: jets
        // when it can lift itself and its kind flies at all.
        let fly_weight = self.bots.brains[&bot].kind.weight("fly");
        // A kind that keeps to its water (`moves: swim`) swims there by
        // itself and takes no swim or jet legs.
        let walker = self.bots.brains[&bot].kind.moves != Moves::Swim;
        let costs = crate::route::Costs {
            swim: walker.then(|| crate::route::Swim::of(peer.player.tuning())),
            jets: (walker && !self.seated(bot))
                .then(|| crate::route::Jets::of(peer.player.tuning(), state.energy, fly_weight))
                .flatten(),
        };
        let driving = self.bot_vehicle_body(bot);
        let (feet, body) = driving.unwrap_or((own_feet, Body::of(peer.player.tuning(), 1.0)));
        if driving.is_some() {
            let mounted = self.mounted(bot).map(|(vehicle, _)| vehicle);
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            if brain.mount_anchor.map(|(v, _)| v) != mounted {
                brain.mount_anchor = mounted.map(|v| (v, feet));
            }
        }
        let mut leash = self.bots.brains[&bot].leash(driving.is_some());
        // A passenger goes where its driver takes it: however far that is
        // from its brick, it keeps its enemy (a gunner fights on).
        if driving.is_none() && self.seated(bot) {
            leash.at = feet;
        }
        // A swimmer in water: how tall it is, to keep it under.
        let swim = (self.bots.brains[&bot].kind.moves == Moves::Swim)
            .then(|| crate::water::body_height(&state, peer.player.tuning()) * state.scale)
            .filter(|height| {
                self.simulation
                    .liquid_at(state.feet, *height)
                    .is_some_and(|(_, covered)| covered >= SWIM_COVERAGE)
            });
        let height = crate::water::body_height(&state, peer.player.tuning()) * state.scale;
        let wet = self.simulation.liquid_at(state.feet, height).is_some();
        if self.bot_kind_rules(bot, wet, state.feet, tick)? {
            return Ok(());
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
        let Hand {
            native,
            native_choice,
            held,
            bite,
            weapon,
        } = self.bot_hand(bot, sight.target, tick)?;
        let (hurt_by, threat) = self.bot_evidence(bot, sight.target, feet, tick);
        // Holding something with its tool: carry it to open space to throw.
        let holding = self.held_by(bot).is_some();
        let grabbing = holding || self.is_reaching(bot);
        let interaction_enemy = sight
            .target
            .map(|s| Knowledge {
                subject: s.owner,
                at: s.real,
                observed: tick,
                expires: tick + ticks(self.bots.brains[&bot].kind.memory_seconds),
            })
            .or(self.bots.brains[&bot].memory)
            .or(hurt_by)
            .filter(|_| leash.holds(feet));
        let vehicle_weapon = self.bot_vehicle_weapon(bot).is_some();
        let can_retaliate = vehicle_weapon
            || self.bots.brains[&bot]
                .kind
                .melee
                .as_ref()
                .is_some_and(|melee| melee.damage > 0.0)
            || hand_combat::has_possible_attack(self, bot);
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
        // Nor is anything taken up before its planning turn in a new game
        // or round has said what that game asks (the body it would push at
        // an enemy may be the game's own ball).
        let unplanned = self.bots.brains[&bot].objective.unplanned();
        if unplanned {
            self.bots.claims.release_owner(bot);
        }
        let opportunity = if unplanned || objective.is_some_and(|view| view.resource.is_some()) {
            None
        } else {
            self.bot_interaction(bot, interaction_enemy, objective.is_none(), tick)
        };
        let objective_holding = objective
            .and_then(|view| view.held)
            .is_some_and(|target| self.held_by(bot) == Some(target));
        let objective_hold_control = objective.is_some_and(|view| {
            view.trigger.is_some() && matches!(view.resource, Some(claims::Resource::Body { .. }))
        });
        let Shot {
            crew_ready,
            attack_clear,
            harm,
            mounted_charging,
            charged_ready,
            target_velocity,
        } = self.bot_shot(bot, weapon, sight.target, eye, tick);

        // A known noncombat body/tool cannot resolve a threat by staring at
        // it. Keep its useful objective; unknown scripted attacks retain their
        // existing behavior rather than being silently classified as harmless.
        // At a body an opponent is also moving, staying engaged in the
        // contest keeps this side's intention live (`contest::engaged`).
        let contest_engaged = objective
            .and_then(|view| view.resource)
            .is_some_and(|resource| contest::engaged(self, bot, resource, feet));
        // How much going after the enemy in sight is worth (`bot_menace`):
        // nothing when it cannot hurt them; with nothing at stake, all of
        // it; more for one that hurt it; at play, all of it for one that
        // contests the body it works or that the play names, less for one
        // armed elsewhere and less still for one unarmed.
        let menace = self.bot_menace(
            objective.as_ref(),
            sight.target,
            threat,
            contest_engaged,
            can_retaliate,
        );
        let mounted = self.mounted(bot).map(|(vehicle, _)| vehicle);
        let allies = self.claim_allies(bot, tick);
        // A teammate has the body it works: it covers it (`contest::cover`).
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
        // It arms with nothing to fight with and an enemy about, or
        // upgrades in a game with nothing else at stake.
        let wants_arm = driving.is_none()
            && (!can_retaliate || objective.is_none())
            && self.bots.brains[&bot].kind.weight("arm") > 0.0
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
        // A spawn-protected target is watched; its reaction waits until it
        // can be hurt.
        let damageable = sight.target.is_none_or(|s| !self.spawn_protected(s.owner));
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
        // Going to find the game (`explore`): a place to look, when it
        // knows of no fight, has no objective, and enemies play.
        let memory = self.bots.brains[&bot].memory;
        self.bots
            .brains
            .get_mut(&bot)
            .unwrap()
            .explore
            .note(feet, memory, tick);
        let explore_to = (!self.bots.brains[&bot].tethered()
            && sight.target.is_none()
            && memory.is_none()
            && threat.is_none()
            && objective.is_none()
            && driving.is_none()
            && swim.is_none()
            && self.bot_enemies_about(bot, &self.bots.brains[&bot].kind))
        .then(|| self.bot_explore_target(bot, feet, eye, &body, &intents, tick))
        .flatten();
        // Whether it may respawn itself, trapped: not while it rides, nor
        // where a respawn it chose brought it (it would only come back),
        // and only as the game's rules let a player (`allow_suicide`).
        let respawn_worth = {
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            let (began, ended) = match brain.life {
                Some((at, ended)) if at.is_finite() => (at, ended),
                Some((_, ended)) => (feet, ended),
                None => (feet, false),
            };
            let ended = ended && flat(feet - began).length() <= 2.0 * PINNED;
            brain.life = Some((began, ended));
            let worth = trapped(brain.pinned);
            if worth > 0.0
                && !ended
                && self.mounted(bot).is_none()
                && self.package_policy("suicide", bot).is_ok()
            {
                worth
            } else {
                0.0
            }
        };
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
        } else if brain.carry.is_none() {
            brain.carry = Some(Carry {
                to: Spot::Looking(0),
                since: tick,
                swing: None,
            });
        }
        // Where to throw from, looked for over as many ticks as the shared
        // ray budget takes.
        if let Some(Spot::Looking(from)) = brain.carry.map(|c| c.to) {
            let to = self.bot_open_spot(bot, feet, from);
            if let Some(carry) = self.bots.brains.get_mut(&bot).unwrap().carry.as_mut() {
                carry.to = to;
            }
        }
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        if std::mem::take(&mut brain.rehome) {
            brain.home = feet;
            brain.leash = feet;
            leash = brain.leash(driving.is_some());
        }
        if brain.sequence == 0 {
            brain.yaw = state.yaw;
        }
        // Remember enemies seen, and where a hit came from.
        let memory_ticks = ticks(kind.memory_seconds);
        let mut warn = None;
        match sight.target {
            Some(seen) => {
                let fresh = brain.target != Some(seen.owner);
                if fresh {
                    brain.target = Some(seen.owner);
                    brain.seen_since = tick;
                }
                brain.perceive(seen.owner, seen.real, feet, tick, fresh, damageable);
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
        // A hit still interrupts at once; return fire waits its reaction.
        if let Some(k) = hurt_by {
            brain.hurt_by(&k, feet, tick);
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
        let away = leash.away(feet);
        if away > leash.radius {
            brain.memory = None;
            brain.evidence_search.clear();
            brain.target = None;
        }

        // Behaviour: the most urgent that applies.
        let enemy = sight.target.filter(|_| away <= leash.radius);
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
        let situation = Situation {
            holding: holding && !objective_hold_control,
            interaction: opportunity.map_or(0.0, |o| o.utility),
            objective: objective.is_some(),
            committed: objective.is_some_and(|view| view.committed),
            arm: arm.map_or(0.0, |(_, score)| score),
            // A swimmer reaches any depth, and a swing reaches round it
            // alike: only how far counts.
            spared: 1.0 - menace,
            enemy: enemy
                // Where it was out of reach, not across from there: one
                // that came down off a jump or a roof is in reach again.
                .filter(|seen| {
                    brain
                        .out_of_reach
                        .is_none_or(|p| p.distance(seen.real) > 2.5)
                })
                .map(|seen| {
                    if swim.is_some() || weapon.is_none_or(|w| w.melee) {
                        (seen.feet.distance(feet), 0.0)
                    } else {
                        (flat(seen.feet - feet).length(), seen.feet.y - feet.y)
                    }
                }),
            far,
            step: body.step,
            // A ranged weapon shoots up at what its band reaches; only a
            // body or melee hit must be level with its enemy.
            reach_up: weapon
                .filter(|w| !w.melee && far > 0.0)
                .map_or(0.0, |_| far),
            // Nor is searching where it cannot reach them.
            remembers: brain
                .memory
                .is_some_and(|m| brain.out_of_reach.is_none_or(|p| p.distance(m.at) > 2.5)),
            // Strayed past where it strolls; while it works a body or a
            // seat (an interaction, idle play's included) its leash is
            // the chase's, as for a fight.
            strayed: if brain.tethered() {
                let radius = if opportunity.is_some() {
                    kind.chase_radius
                } else {
                    kind.wander_radius
                };
                (away - radius) / STRAY
            } else {
                0.0
            },
            pursuing: brain.objective.pursuing(),
            explore: explore_to.is_some(),
            // Never a way out of a fight: not with an enemy in sight or
            // one that hurt it, nor while it carries something.
            trapped: if sight.target.is_none()
                && threat.is_none()
                && hurt_by.is_none()
                && !holding
                && !objective.is_some_and(|view| view.committed)
            {
                respawn_worth
            } else {
                0.0
            },
        };
        // Across its band; up, what a jump brings within its band.
        brain.reach = (situation.far.max(2.0), body.jump + 1.0 + situation.reach_up);
        brain.chase_feet = enemy.map(|seen| seen.feet);
        let mut scores = behaviour::scores(&situation, |b| kind.weight(b.name()));
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
        // Hurt by the one it already went after tells it nothing new: only
        // a hit from someone else re-opens the choice at once.
        let hurt_anew =
            hurt_now && hurt_by.is_some_and(|k| brain.choice_was.target != Some(k.subject));
        let interrupt = hurt_anew
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
            team::exit(&intents, feet, tall, &|at| {
                floor_below(&self.simulation, at, &body).is_some()
            }),
            carries,
            explore_to,
        );
        let plain_before = team::pressed(&kind.team, deficit, &mut scores);
        let claims = &self.bots.claims;
        brain.team.terms = team::side_terms(
            &kind.team,
            bot,
            gate,
            |b| claims.held_since(bot, b as u8, choices[b].target, tick),
            &mut scores,
            &choices,
            &intents,
            tall,
            &clear,
        );
        brain.team.allies = intents.len();
        // The plain pick, or a near one the surprise chooser takes.
        brain.surprise.gate = gate;
        let behaviour = surprise::behaviour(
            &mut brain.surprise,
            &kind.surprise,
            kind.hold(),
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
        let chosen = choices[behaviour as usize];
        self.bots.claims.publish(
            bot,
            claims::Intent {
                option: behaviour as u8,
                since: self
                    .bots
                    .claims
                    .held_since(bot, behaviour as u8, chosen.target, tick),
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
                // Boarding ends the walk to the seat and starts the drive:
                // the drive's progress counts from where it begins.
                self.bots.claims.leg(
                    bot,
                    mounted == Some(resource.vehicle()),
                    selected_objective.unwrap().point.distance(feet),
                    tick,
                );
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
                if let Some((at, _)) = arm {
                    brain.set_goal_near(Goal::Arm(at));
                }
                (false, false)
            }
            Behaviour::Interact => {
                if let Some(o) = opportunity {
                    brain.set_goal_near(Goal::Interact(o.point));
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
                        brain.pursue(intended, in_band, near, feet, body.width * 0.5, tick)
                    } else {
                        if step.waiting && !step.move_while_waiting {
                            brain.set_goal(None);
                        } else {
                            brain.set_goal_near(Goal::Objective(step.point));
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
                brain.set_goal(brain.carry.and_then(|c| c.to.place()).map(Goal::Carry));
                (false, false)
            }
            Behaviour::Fight => brain.pursue(enemy, true, near, feet, body.width * 0.5, tick),
            Behaviour::Chase | Behaviour::Search => {
                // Straight at them, or wide round a side (`surprise`).
                if let Some(seen) = enemy.filter(|_| behaviour == Behaviour::Chase) {
                    brain.chase_offset = surprise::route(
                        &mut brain.surprise,
                        &kind.surprise,
                        kind.hold(),
                        seen.real,
                        flanks,
                        gate,
                        tick,
                    );
                }
                brain.pursue(enemy, false, near, feet, body.width * 0.5, tick)
            }
            Behaviour::Return => {
                brain.set_goal(Some(Goal::Home));
                (false, false)
            }
            Behaviour::Respawn => {
                brain.set_goal(None);
                (false, false)
            }
            Behaviour::Explore => {
                // A route that ends short of the place is as far as it goes
                // that way: it looks elsewhere next.
                if brain.settled && matches!(brain.goal, Some(Goal::Wander(_))) {
                    brain.explore.to = None;
                    brain.set_goal(None);
                } else if let Some(to) = explore_to {
                    brain.set_goal_near(Goal::Wander(to));
                }
                (false, false)
            }
            Behaviour::Wander => {
                brain.wander(feet, tick, swim.is_some());
                (false, false)
            }
        };
        // Respawning: the command a player gives (Ctrl+K), with what it
        // costs them in this game.
        if behaviour == Behaviour::Respawn {
            if let Some((at, _)) = brain.life {
                brain.life = Some((at, true));
            }
            let sequence = self.peers.get(&bot).map_or(1, |p| p.last_sequence + 1);
            let _ = self.command(bot, sequence, Command::Suicide);
            return Ok(());
        }

        // Path.
        let home = brain.home;
        let mut wanted = self.bot_path(bot, feet, &body, costs, swim, &state, tick);
        // Standing on a body (a vehicle's roof, a crate, a head): the
        // world has no floor anywhere under its feet (its middle can stand
        // over a gap between bricks it stands on).
        let on_body = swim.is_none()
            && driving.is_none()
            && state.grounded
            && [
                (0.0, 0.0),
                (1.0, 1.0),
                (1.0, -1.0),
                (-1.0, 1.0),
                (-1.0, -1.0),
            ]
            .into_iter()
            .all(|(x, z)| {
                let at = own_feet + Vec3::new(x, 0.0, z) * (body.width * 0.4);
                super::admin_players::world_ray(
                    &self.simulation,
                    at + Vec3::Y * 0.1,
                    Vec3::NEG_Y,
                    0.4,
                )
                .is_none()
            });
        // The walk grid has no place for its feet there and no route: it
        // walks straight off toward wherever it is going, and plans again
        // once down. One going nowhere, or whose goal is close by below it
        // (under its feet or at the body's foot), goes nowhere walking at
        // it: it steps off to the nearest free floor instead, toward the
        // goal where that side is open.
        if wanted.is_none() && on_body && self.bots.brains[&bot].search.is_none() {
            let to = match self.bots.brains[&bot].goal.map(|g| g.point(home)) {
                Some(point) if flat(point - feet).length() > 1.5 => Some(point),
                Some(point) if point.y >= feet.y - body.step - 0.5 => None,
                point => self.bot_step_off(bot, feet, &body, point.unwrap_or(feet), f32::INFINITY),
            };
            if let Some(to) = to {
                wanted = Some(Waypoint::walk(to));
                self.bots.brains.get_mut(&bot).unwrap().settled = false;
            }
        }
        // The same when its route starts well below its feet, right by it:
        // it stands on something the walk grid does not see (a narrow wall
        // top, a ledge between cell centres), so it steps off to the
        // nearest free floor toward the route first.
        if let Some(next) = wanted
            && next.mode == Mode::Walk
            && next.through.is_none()
            && feet.y - next.feet.y > body.step + 0.5
            && flat(next.feet - feet).length() < body.width + 1.0
            && driving.is_none()
            && swim.is_none()
            && let Some(to) =
                self.bot_step_off(bot, feet, &body, next.feet, feet.y - body.step - 0.5)
        {
            wanted = Some(Waypoint::walk(to));
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
                **o != bot
                    && p.combat.alive
                    && flat(Vec3::from(p.player.state().feet) - feet).length() < 3.0
                    && !self.seated(**o)
                    && self.bot_allies(bot, **o)
            })
            .map(|(_, p)| Vec3::from(p.player.state().feet))
            .collect();
        let may_glance = perception::may_glance(behaviour) && sight.target.is_none();
        let glance = self.bot_glance(
            bot,
            tick,
            eye,
            may_glance
                && driving.is_none()
                && !self.seated(bot)
                && !situation.objective
                && !situation.holding,
            behaviour == Behaviour::Wander,
        );
        // Now and then something idle (`surprise`): goofing is an option
        // the chooser weighs against playing (`play_worth`).
        let idle = behaviour == Behaviour::Wander && objective.is_none();
        let play = self.play_worth(
            bot,
            behaviour,
            objective.as_ref(),
            selected_objective.is_some_and(|v| v.enemy.is_some()),
            sight.target,
            feet,
            tick,
        );
        // Idle play with a body (Interact) is already the bot's fun: a goof
        // would stand it still beside the ball it came to push.
        let natural = threat.is_none()
            && !matches!(behaviour, Behaviour::Interact | Behaviour::Carry)
            && !holding
            && driving.is_none()
            // A swimmer's idle hops and walks would take it out of its
            // water at the surface.
            && kind.moves != Moves::Swim
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
        pause.perched = on_body;
        (pause.pull, pause.copy) = self.team_mood_now(bot, (feet, eye), threat.is_some(), tick);
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let moment = brain
            .surprise
            .goof(&brain.kind.surprise, brain.kind.hold(), &pause, tick);
        let act = self.surprise_act(bot, moment, feet, eye, tick)?;
        let extra = self.bot_extras(
            bot,
            extras::Scene {
                behaviour,
                enemy_seen: sight.target.is_some(),
                hurt_by,
                holding: wanted.is_none(),
                natural,
                calm: !gate.urgent && !gate.carrying,
                carrying: gate.carrying,
                feet,
            },
            tick,
        )?;
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        // Carried there (or as near as it gets: settled, or no way there):
        // swing, after holding it up a moment.
        if let Some(carry) = brain.carry.as_mut()
            && carry.swing.is_none()
            && tick >= carry.since + LIFT_TICKS
            && match carry.to {
                Spot::Looking(_) => false,
                Spot::Found(to) => {
                    to.is_none_or(|to| flat(to - feet).length() < 0.6) || brain.settled
                }
            }
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
        // Aim: each that wants its eyes proposes a look; the highest has
        // them (`act::look`).
        let mut looks = vec![act::Look {
            by: act::Looker::Hold,
            yaw: brain.yaw,
            pitch: 0.0,
        }];
        let pitch_to = |delta: Vec3| delta.y.atan2(flat(delta).length()).clamp(-1.5, 1.5);
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
                    looks.push(act::Look {
                        by: act::Looker::Carry,
                        yaw: wrap(brain.yaw + 1.0),
                        pitch: 0.6,
                    });
                    step *= 2.0;
                    fire = tick < start + SWING_TICKS;
                    if !fire {
                        brain.next_grab = tick + REGRAB_TICKS;
                    }
                }
                None => {
                    let d = wanted.map_or(Vec3::ZERO, |next| {
                        flat(next.through.unwrap_or(next.feet) - feet)
                    });
                    looks.push(act::Look {
                        by: act::Looker::Carry,
                        yaw: if d.length() > 0.05 {
                            yaw_to(d)
                        } else {
                            brain.yaw
                        },
                        pitch: carry_pitch.unwrap_or(0.15),
                    });
                }
            }
        } else if behaviour == Behaviour::Objective
            && !gunning
            && selected_objective.is_none_or(|view| view.enemy.is_none())
        {
            let delta = selected_objective
                .as_ref()
                .map(|objective| objective.aim - eye);
            looks.push(act::Look {
                by: act::Looker::Objective,
                yaw: delta.map_or(brain.yaw, yaw_to),
                pitch: delta.map_or(0.0, pitch_to),
            });
        } else if let Some(seen) = sight.target {
            // A blast hurts all around where it lands: a splash weapon
            // aims at the feet, so a near miss still lands within it.
            let mut at = if weapon.is_some_and(|w| !w.melee && w.splash > 0.0) {
                seen.feet + Vec3::Y * FEET_AIM
            } else {
                seen.eye - Vec3::Y * 0.5
            };
            if let Some(w) = weapon.filter(|w| !w.melee && w.speed > 0.0) {
                let time = at.distance(eye) / w.speed;
                at += target_velocity * lead(bot, tick) * time;
                at.y += 0.5 * w.fall * time * time;
            }
            let delta = native_choice.map_or(at - eye, |c| c.direction);
            let tracked = tick.saturating_sub(brain.seen_since) as f32 * TICK;
            // How fast it moves across the bot's line of sight, the bot's
            // own motion included: tracking it lags (`perception`).
            let line = (at - eye).normalize_or_zero();
            let relative = target_velocity - Vec3::from(state.velocity);
            let across = (relative - line * relative.dot(line)).length();
            let size = kind.aim_error_degrees.to_radians()
                * (1.0 - (tracked / 2.0).min(1.0) * (1.0 - ERROR_FLOOR))
                * brain.perception.aim_scale(seen.owner, tracked)
                + perception::steady_error(&kind.perception, across, at.distance(eye));
            brain.error = aim_error(bot, tick, size);
            let aim_yaw = wrap(yaw_to(delta) + brain.error.0);
            let aim_pitch = (delta.y.atan2(flat(delta).length()) + brain.error.1).clamp(-1.5, 1.5);
            looks.push(act::Look {
                by: act::Looker::Target,
                yaw: aim_yaw,
                pitch: aim_pitch,
            });
            let reaction = ticks(kind.reaction_seconds);
            let in_reach = weapon.is_some_and(|w| at.distance(eye) <= w.reach.max(1.0) * 1.1 + 0.5);
            fire = enemy.is_some()
                && (driving.is_none() || vehicle_weapon)
                && brain
                    .perception
                    .acted(seen.owner, tick)
                    .unwrap_or(tick >= brain.seen_since + reaction)
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
            // A goof is not an attack.
            if tick < brain.next_grab || brain.surprise.flavour().is_some() {
                fire = false;
            }
        } else if let Some(at) = glance {
            // A glance turns the ordinary aim; the walk goes on.
            let delta = at - eye;
            looks.push(act::Look {
                by: act::Looker::Glance,
                yaw: yaw_to(delta),
                pitch: pitch_to(delta),
            });
        } else if let Some(next) = wanted {
            let d = flat(next.through.unwrap_or(next.feet) - feet);
            let mut yaw = if d.length() > 0.05 {
                yaw_to(d)
            } else {
                brain.yaw
            };
            // Strolling, the look drifts a little off the way.
            if idle {
                yaw = wrap(yaw + perception::drift(&kind.perception, bot, tick));
            }
            looks.push(act::Look {
                by: act::Looker::Route,
                yaw,
                pitch: 0.0,
            });
        } else if hold {
            // Searching the spot: it looks one way and back, all round the
            // way it came facing, pausing unevenly (a seeded drift eases in
            // and out of each point), not a turret's steady spin.
            let from = *brain.sweep_from.get_or_insert(brain.yaw);
            looks.push(act::Look {
                by: act::Looker::Sweep,
                yaw: wrap(
                    from + SWEEP * cadence::drift(bot, cadence::salt::SWEEP, 0, tick, SWEEP_TICKS),
                ),
                pitch: 0.0,
            });
        }
        if !hold || sight.target.is_some() || glance.is_some() {
            brain.sweep_from = None;
        }
        // A goof's gesture (waving a tool, looking about) does not keep its
        // eyes off someone it just noticed: the glance has the look.
        if let Some((yaw, pitch)) = act.aim.filter(|_| glance.is_none()).or(extra.aim) {
            looks.push(act::Look {
                by: act::Looker::Gesture,
                yaw,
                pitch,
            });
        }
        if act.look_down && glance.is_none() {
            looks.push(act::Look {
                by: act::Looker::Down,
                yaw: brain.yaw,
                pitch: LOOK_DOWN,
            });
        }
        let look = act::look(&looks);
        let (aim_yaw, aim_pitch) = (look.yaw, look.pitch);
        brain.acted.look = look.by;
        // Handling things (a carry's swing, an objective's or interaction's
        // controls) keeps the plain turn its controllers are built on, and
        // a startle does not stop it.
        let handling = matches!(
            behaviour,
            Behaviour::Carry | Behaviour::Objective | Behaviour::Interact
        );
        if let Some(seen) = sight.target {
            step *= brain.perception.turn_scale(seen.owner, tick);
        } else if !handling && brain.perception.startled(tick) {
            step = 0.0;
        }
        brain.yaw = if handling {
            turn(brain.yaw, aim_yaw, step)
        } else {
            brain
                .perception
                .turn(&kind.perception, brain.yaw, aim_yaw, step)
        };
        brain.pitch += (aim_pitch - brain.pitch).clamp(-step, step);

        // Move along the plan, facing wherever it aims.
        let mut input = MoveInput {
            yaw: brain.yaw,
            pitch: brain.pitch.clamp(-1.5, 1.5),
            ..Default::default()
        };
        let forward = Vec3::new(brain.yaw.sin(), 0.0, -brain.yaw.cos());
        let right = Vec3::new(brain.yaw.cos(), 0.0, brain.yaw.sin());
        // How it moves (`act`): each mover proposes, one order decides,
        // safety has the last word.
        let mut proposals: Vec<act::Proposal> = Vec::new();
        // Along its route, or through the opening it leads through.
        if let Some(next) = wanted {
            proposals.push(act::Proposal {
                jump: Some(next.jump && flat(next.feet - feet).length() < 1.6 && state.grounded),
                // Into a crawlspace: crouch on the way in (the body stays
                // down until it has room to stand).
                crouch: Some(next.crouch && flat(next.feet - feet).length() < 1.6),
                ..act::Proposal::walk(
                    act::Mover::Route,
                    if let Some(through) = next.through {
                        flat(through - feet).normalize_or_zero()
                    } else {
                        walk_direction.unwrap_or(Vec3::ZERO)
                    },
                )
            });
        }
        let routed = proposals.first().and_then(|p| p.walk).unwrap_or(Vec3::ZERO);
        if let Some(push) = pushing {
            proposals.push(act::Proposal::walk(act::Mover::Push, push));
        }
        // Its stance, from the walk the route or the push gives it.
        let given = pushing.unwrap_or(routed);
        let stance = match behaviour {
            // In its band at an objective, or a swimmer fighting (no floor
            // to probe, its water all round): weave so it is not a still
            // target, and give ground if too close.
            Behaviour::Objective | Behaviour::Fight
                if wanted.is_none()
                    && hold
                    && (behaviour == Behaviour::Objective || kind.moves == Moves::Swim) =>
            {
                let side = brain.strafe_leg(tick, WEAVE_SECONDS, &|_| true, false) * 0.7;
                let mut walk = right * side;
                if back_off {
                    walk -= forward;
                }
                Some(walk)
            }
            Behaviour::Fight if wanted.is_none() && hold => {
                let mut walk = given;
                let melee = weapon.is_none_or(|w| w.melee);
                let gap = enemy.map(|seen| flat(seen.feet - feet).length());
                if melee {
                    // A melee fighter closes to its band rather than
                    // circling its target; one on its head, or under it,
                    // steps off sideways (no swing reaches straight up or
                    // down its own body).
                    let stacked = enemy.is_some_and(|seen| {
                        flat(seen.feet - feet).length() < body.width
                            && (seen.feet.y - feet.y).abs() > body.step
                    });
                    if stacked {
                        walk = right;
                    } else if gap.is_some_and(|gap| gap > near.max(1.0) + 0.5) {
                        // Straight at it: not along its facing, which its
                        // aim error turns off the line (off a stair's edge).
                        walk = enemy
                            .map_or(forward, |seen| flat(seen.feet - feet).normalize_or(forward));
                    } else if let Some(gap) = gap {
                        // In its band it keeps its feet moving: in, back
                        // out and aside, on its own seeded beat.
                        let room = gap + FOOTWORK_BACK <= near.max(1.0);
                        walk = footwork(bot, tick, forward, right, room);
                    }
                } else {
                    // A ranged fighter strafes one way for a while, but
                    // not where the floor ends or a wall or an ally stands
                    // that way.
                    let floor = |side: f32| {
                        let under = floor_below(&self.simulation, feet + right * side * 0.9, &body);
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
                    let side = brain.strafe_leg(tick, STRAFE_SECONDS, &ground, parted);
                    if ground(side) {
                        walk = right * side * 0.7;
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
                        && floor_below(&self.simulation, feet + forward * 0.9, &body).is_some()
                    {
                        walk += forward * lean;
                    }
                }
                if back_off {
                    walk -= forward;
                }
                Some(walk)
            }
            _ => None,
        };
        if let Some(walk) = stance {
            proposals.push(act::Proposal::walk(act::Mover::Stance, walk));
        }
        // A jet leg flies itself: climb in the open, cross, land.
        match wanted.map(|w| (w.feet, w.mode)) {
            Some((to, Mode::Jet { apex, seconds })) if driving.is_none() => {
                let leg = brain
                    .jet_leg
                    .get_or_insert(crate::route::JetLeg::start(to, feet, tick));
                if leg.to != to {
                    *leg = crate::route::JetLeg::start(to, feet, tick);
                }
                let control = crate::route::jet(
                    leg,
                    feet,
                    Vec3::from(state.velocity),
                    state.grounded,
                    to,
                    apex,
                );
                // Replan from what happened.
                if leg.failed(feet, state.grounded, body.step, seconds, tick) {
                    brain.jet_leg = None;
                    brain.plan.clear();
                    brain.search = None;
                    brain.settled = false;
                    brain.replans += 1;
                } else {
                    proposals.push(act::Proposal {
                        jet: Some(control.jet),
                        jump: Some(control.jump),
                        crouch: Some(false),
                        ..act::Proposal::walk(act::Mover::Jet, control.direction)
                    });
                }
            }
            Some((to, Mode::Swim | Mode::Walk)) if swim.is_none() && wet && !state.grounded => {
                // Afloat: swim on toward it, rising where the way on (out
                // onto a higher bank) is higher.
                brain.jet_leg = None;
                if to.y > feet.y - 0.2 {
                    proposals.push(act::Proposal {
                        jump: Some(true),
                        ..act::Proposal::buttons(act::Mover::Swim)
                    });
                }
            }
            _ => brain.jet_leg = None,
        }
        if swim.is_some() {
            // Up and down as a swimmer does: jump rises, crouch dives.
            let to = match behaviour {
                Behaviour::Fight => enemy.map(|seen| seen.feet),
                _ => wanted.map(|w| w.feet),
            };
            if let Some(to) = to {
                proposals.push(act::Proposal {
                    jump: Some(to.y > feet.y + 0.4),
                    crouch: Some(to.y < feet.y - 0.4),
                    ..act::Proposal::buttons(act::Mover::Swim)
                });
            }
        }
        // A fight stands out of where a teammate's weapon will hit (`team`),
        // where there is floor to stand on (as the strafe checks): on a
        // deck the way out can be off its edge.
        if behaviour == Behaviour::Fight
            && let Some(out) = choices[Behaviour::Fight as usize]
                .place
                .filter(|out| *out != feet)
            && floor_below(&self.simulation, out, &body).is_some()
        {
            proposals.push(act::Proposal::walk(
                act::Mover::Team,
                flat(out - feet).normalize_or_zero(),
            ));
        }
        if let Some(walk) = act.direction {
            proposals.push(act::Proposal::walk(act::Mover::Goof, walk));
        }
        if let Some(walk) = extra.direction {
            proposals.push(act::Proposal::walk(act::Mover::Dodge, walk));
        }
        if extra.stand {
            proposals.push(act::Proposal::walk(act::Mover::Stand, Vec3::ZERO));
        }
        // Walking into something: hop, then plan again, then give up. One
        // judge says whether it is getting anywhere; held back at an edge
        // it is not (safety only takes a walk away, so a walk proposed is
        // one it tries). Getting somewhere is net progress across a window
        // (`route`), not moving at an instant: wobbling on a roof's edge or
        // between two spots is as stuck as standing against a wall. (Not
        // pressing against what it came to work: an item to pick up, a
        // brick or a body to use, at the goal itself.)
        let trying = proposals
            .iter()
            .max_by_key(|p| p.walk.is_some().then_some(p.mover))
            .and_then(|p| p.walk)
            .is_some_and(|walk| walk != Vec3::ZERO);
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        let at_work = matches!(
            brain.goal,
            Some(Goal::Objective(p) | Goal::Interact(p) | Goal::Arm(p))
                if flat(p - feet).length() < body.width + 1.0
        );
        let walking = driving.is_none()
            && trying
            && pushing.is_none()
            && !at_work
            && wanted.is_some_and(|w| matches!(w.mode, Mode::Walk | Mode::Swim));
        let stalled = walking
            && brain
                .progress
                .stalled(feet, self.peers[&bot].player.tuning().forward, 120.0, tick);
        if !walking {
            brain.progress.reset();
        }
        // Trapped: trying again and again to get somewhere beyond
        // `2 * PINNED` (walking at it, or wanting a goal it has no route
        // to), and still within `PINNED` of where it began. A stroll that
        // gets there leaves the spot, which clears it, as does no try for
        // `TRY_GAP`. An objective's step is the step's to give up (its own
        // deadline), not a trap.
        let far_goal = brain.goal.is_some_and(|g| {
            !matches!(g, Goal::Objective(_))
                && flat(g.point(brain.home) - feet).length() > 2.0 * PINNED
        });
        let tries = far_goal
            && driving.is_none()
            && pushing.is_none()
            && !at_work
            && (walking || wanted.is_none());
        brain.pinned = brain
            .pinned
            .filter(|(at, _, last)| flat(feet - *at).length() <= PINNED && tick < last + TRY_GAP);
        if tries {
            let (at, since, _) = brain.pinned.unwrap_or((feet, tick, tick));
            brain.pinned = Some((at, since, tick));
        }
        // A goof's, an extra's, an objective's (into a body) or the stall
        // judge's hop, which safety lets through where it comes down on
        // floor. The first window gone nowhere hops (off a body it stands
        // on, over what its shins catch); the next plans again.
        let into_body = selected_objective.is_some_and(|v| v.jump);
        let stall_hop = stalled && brain.progress.stalls() == 1;
        let controls = act::resolve(
            &mut proposals,
            act::Press {
                hop: act.jump || extra.jump || into_body,
                jump: stall_hop,
                crouch: act.crouch || extra.crouch,
                jet: extra.jet,
            },
        );
        (input.crouch, input.jet) = (controls.crouch, controls.jet);
        let (direction, vetoed, jump) = self.bot_safe_walk(
            bot,
            &body,
            feet,
            &state,
            controls,
            act::Ground {
                driving: driving.is_some(),
                swimming: swim.is_some() || wet,
                jet_leg: wanted.is_some_and(|w| matches!(w.mode, Mode::Jet { .. })),
                vehicle_detour: driving.is_none() && pushing.is_none(),
            },
            quarry,
            goal_at,
        );
        let brain = self.bots.brains.get_mut(&bot).unwrap();
        input.forward = direction.dot(forward).clamp(-1.0, 1.0);
        input.right = direction.dot(right).clamp(-1.0, 1.0);
        input.jump = jump;
        let mut forget = false;
        if stalled && brain.progress.stalls() > 1 && wanted.is_some() {
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
                brain.explore.to = None;
                if behaviour == Behaviour::Arm {
                    // No way to the item: pass it over.
                    brain.arming.pass_over(tick);
                }
                if let Some(carry) = brain.carry.as_mut() {
                    // No way to open space: throw from here.
                    carry.to = Spot::Found(None);
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
        brain.acted.walk = controls.by;
        brain.acted.held_back = vetoed;
        brain.acted.stalls = brain.progress.stalls();
        brain.acted.replans = brain.replans;
        brain.sequence += 1;
        let sequence = brain.sequence;
        if forget && let Some((_, nav)) = self.bots.navs.iter_mut().find(|(b, _)| *b == body) {
            nav.invalidate(feet - Vec3::splat(1.0), feet + Vec3::splat(1.0), &body);
            // What it walked toward and got nowhere: the grid avoids it a
            // while, for every body of this size (`Nav::avoid`).
            if let Some(next) = wanted.filter(|w| w.through.is_none()) {
                nav.avoid(next.feet, tick + AVOID_TICKS);
            }
        }
        self.bot_act(
            bot,
            tick,
            sequence,
            Act {
                input,
                wanted,
                behaviour,
                weapon,
                bite,
                native,
                native_choice,
                target: sight.target,
                fire,
                grabbing,
                gunning,
                vehicle_weapon,
                charged_ready,
                mounted_charging,
                objective: selected_objective,
                goof_trigger: act.trigger,
                callout,
            },
        )
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
    explore: Option<Vec3>,
) -> [team::Choice; Behaviour::COUNT] {
    use claims::{Resource, Target};
    let mut c = [team::Choice::default(); Behaviour::COUNT];
    let at = |b: Behaviour| b as usize;
    // Where it would go looking: teammates look elsewhere (`explore`).
    c[at(Behaviour::Explore)].place = explore;
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
            // Only a team's step crowds a teammate's: one's own progress
            // is not given way to an ally making the same.
            place: (v.shared).then_some(v.point),
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
