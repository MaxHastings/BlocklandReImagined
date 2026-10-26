//! Bot kinds Add-Ons provide (`assets/bots.json`).
//!
//! v20 has no brain for the player objects a Vehicle Spawn brick makes:
//! they stand where they spawn until ridden. Bots that walk, find their way
//! and fight are the engine's bot mechanism; which ones exist, what the
//! spawn list calls them and how they play is each Add-On's data. The base
//! game provides none.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

mod composure;
mod surprise;
mod team;
pub mod tuning;
pub use composure::{Blow, BotComposure};
pub use surprise::{BotSurprise, FLAVOURS};
pub use team::{BotTeam, CALLOUTS, TERMS};

pub const SCHEMA_VERSION: u32 = 1;
/// Bot kinds one server knows, over every Add-On.
pub const MAX_KINDS: usize = 64;
/// Most first names one kind lists.
pub const MAX_FIRST_NAMES: usize = 256;
/// The behaviours a kind's `behaviours` may weigh, in the brain's urgency
/// order (`session::bots::behaviour::Behaviour`).
pub const BEHAVIOURS: [&str; 12] = [
    "carry",
    "interact",
    "flee",
    "fight",
    "arm",
    "chase",
    "search",
    "return",
    "objective",
    "explore",
    "respawn",
    "wander",
];
/// Route legs a kind's `behaviours` may also weigh: `fly` scales how
/// readily its routes take a jet leg (0: never), as `crate::route::Jets`
/// costs them (`docs/architecture/bots.md`, Routes).
pub const LEG_WEIGHTS: [&str; 1] = ["fly"];

/// One bot kind: its spawn list entry and how its brain plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotKind {
    /// Stable id a spawn brick stores.
    pub id: String,
    /// What the Vehicle Spawn list shows, and the bot's player name.
    pub name: String,
    /// How far, in world units, it sees other players.
    pub sight: f32,
    /// How far from its spawn brick it strolls when nothing is going on.
    pub wander_radius: f32,
    /// Whether a brick bot returns home when idle. Combat pursuit is separate.
    pub return_home: bool,
    /// How far idle interactions may take it from home before returning.
    /// Also the pursuit radius for an explicit follow intent; ordinary
    /// combat is limited by perception and evidence, not distance to spawn.
    pub chase_radius: f32,
    /// Seconds from first seeing an enemy to its first shot.
    pub reaction_seconds: f32,
    /// Degrees a second its aim turns.
    pub turn_degrees: f32,
    /// Aim error in degrees when a fight starts; it narrows to a third of
    /// this while the bot keeps its enemy in sight.
    pub aim_error_degrees: f32,
    /// Seconds it remembers where it last saw, or was hurt by, an enemy.
    pub memory_seconds: f32,
    /// Whether it fights bots on the other side as well as players.
    pub fights_bots: bool,
    /// First names rules may call its bots by (Slayer names its bots
    /// "Bot " and a random first name).
    pub first_names: Vec<String>,
    /// The body it plays in: an archetype an Add-On provides
    /// (`namespace:archetype/name`: its model, speeds and health). None
    /// for the standard player.
    pub body: Option<String>,
    /// How it hurts what it reaches with its own body while its hands are
    /// empty (Bot_Hole's `hMelee`): a zombie's swipe, a shark's bite.
    pub melee: Option<BotMelee>,
    /// Where it gets about.
    pub moves: Moves,
    /// Bot_Hole's `hType`: bots of one side never fight each other and
    /// fight everyone else, players and other bots, whoever built them.
    /// None: a builder's bots are one side (`fights_bots`).
    pub side: Option<String>,
    /// How a Blockhead body looks, over the avatar pack's defaults.
    pub look: Option<BotLook>,
    /// An emote it strikes as it spawns and as it starts a fight (`hug`
    /// holds the arms out ahead, as `playThread(1, armReadyBoth)` did).
    pub emote: Option<String>,
    /// A swimmer in a mini-game dies after this long out of water.
    pub out_of_water_seconds: Option<f32>,
    /// When it first sees an enemy, or is hurt, bots of its side within its
    /// sight that have nothing better to go on go and look where the enemy
    /// was (Bot_Hole's `hAlertOtherBots`).
    pub alerts_allies: bool,
    /// Weights on its behaviours' scores by name (`carry`, `interact`,
    /// `fight`, `chase`, `search`, `return`, `wander`), and on its route's
    /// legs (`fly`: how readily it jets; 0 never). Ordinary
    /// behaviours default to 1; environmental interactions and objectives default to 0:
    /// 0 turns one off (a guard that never gives chase), more puts it ahead
    /// of others (`docs/architecture/bots.md`).
    pub behaviours: std::collections::BTreeMap<String, f32>,
    /// How far from itself, in world units, it looks for loose bodies an
    /// authored object-entry objective can use.
    pub objective_radius: f32,
    /// How its choices vary and change over time (`surprise`); its
    /// `strength` 0 is the plain brain.
    pub surprise: BotSurprise,
    /// How teammates' intents, the mood about it and its team's score
    /// weigh in its choices (`team`).
    pub team: BotTeam,
    /// What it notices: brief glances and how long it takes to react
    /// (`session::bots::perception`).
    pub perception: BotPerception,
    /// Seconds a choice is held before another may take over (`BotHold`).
    pub hold_seconds: f32,
    /// Whether it fights with hand weapons. False: it never takes up,
    /// draws or fires one, even one the rules put in its hand, and fights
    /// with its body (`melee`, `contact`, `ability`) or a vehicle gun it
    /// crews.
    pub weapons: bool,
    /// Which of the brain's fighting tactics it uses (all, by default).
    pub tactics: BotTactics,
    /// Which seats of a vehicle it takes (all, by default).
    pub vehicles: BotVehicles,
    /// Hurts whoever it touches, whatever it is doing: a creature that
    /// harms by bumping into you. None: touching is harmless.
    pub contact: Option<BotContact>,
    /// An attack its Add-On defines (a fuse that ends in a blast, a
    /// spit, a roar): the brain decides when, the Add-On what happens
    /// (`on_bot_ability`).
    pub ability: Option<BotAbility>,
    /// Below this share of its health, with an enemy about, it runs away
    /// from them (the `flee` behaviour); 0 never.
    pub flee_below: f32,
    /// How steady it stays under fire (`composure`): none unless its Add-On
    /// gives it an events table.
    pub composure: BotComposure,
}
/// What a minimal-profile bot costs as a share of a full Blockhead bot:
/// measured at 21 to 24 percent (`creature_think_time_16` in
/// `crates/chaos/tests/bot_gauntlet.rs`, release), so four count as one
/// against the bot limit.
pub const MINIMAL_COST: f32 = 0.25;

/// The tactics the shared brain may use in a fight
/// (`docs/architecture/bots.md`). Off, the brain leaves the tactic out
/// and spends nothing on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotTactics {
    /// Weighs places to fight from (where it is seen, where its shot is
    /// best: "Where to stand"). Off, it fights where it stands.
    pub cover: bool,
    /// Gives ground: steps back out of a fight or to a place farther from
    /// its enemy. Off, it never backs off.
    pub retreat: bool,
    /// Comes at an enemy round a side as well as straight (`surprise`).
    pub flank: bool,
    /// Hops or steps out of the way of a shot or a chassis coming at it.
    pub dodge: bool,
}
impl BotTactics {
    /// None at all.
    pub fn none() -> Self {
        Self {
            cover: false,
            retreat: false,
            flank: false,
            dodge: false,
        }
    }
}
impl Default for BotTactics {
    fn default() -> Self {
        Self {
            cover: true,
            retreat: true,
            flank: true,
            dodge: true,
        }
    }
}
/// The seats it takes, by what the seat does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotVehicles {
    /// A seat with the controls.
    pub drive: bool,
    /// A seat with the vehicle's gun.
    pub gun: bool,
    /// Any other seat, riding along.
    pub ride: bool,
}
impl Default for BotVehicles {
    fn default() -> Self {
        Self {
            drive: true,
            gun: true,
            ride: true,
        }
    }
}
impl BotVehicles {
    /// It takes no seat at all.
    pub fn none(&self) -> bool {
        !self.drive && !self.gun && !self.ride
    }
}
/// Harm on touch: an enemy whose body touches its body loses `damage`
/// every `seconds` while they touch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BotContact {
    pub damage: f32,
    #[serde(default = "melee_seconds")]
    pub seconds: f32,
    /// What the kill feed and `on_damage` hooks call it.
    #[serde(default = "contact_name")]
    pub name: String,
}
fn contact_name() -> String {
    "Touch".into()
}
/// An attack its Add-On defines. The brain walks into `range` of an enemy
/// it sees and, its reaction passed, starts it: a `fuse_seconds` wind-up
/// (its body plays `action` and wears `emote`, `sound` repeats every
/// `sound_seconds`) that is called off when the enemy gets `cancel_range`
/// away, then the Add-On's `on_bot_ability(bot, kind, ability, target)`
/// hooks run. What the ability does (a blast, a spit, a heal) is
/// entirely theirs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotAbility {
    /// Its name, passed to the hook.
    pub name: String,
    /// How near an enemy must be to start it, in world units.
    pub range: f32,
    /// Seconds of wind-up before the hook runs (0 at once).
    pub fuse_seconds: f32,
    /// The enemy this far away during the wind-up calls it off.
    pub cancel_range: f32,
    /// Seconds before it may start again.
    pub cooldown_seconds: f32,
    /// Whether it must see its enemy to start (false: one it remembers
    /// near will do).
    pub sight: bool,
    /// An action its body's model plays as the wind-up starts.
    pub action: Option<String>,
    /// An image its body wears during the wind-up (`emote` slot).
    pub emote: Option<String>,
    /// A sound played at it during the wind-up...
    pub sound: Option<String>,
    /// ...every this many seconds.
    pub sound_seconds: f32,
    /// It stands still during the wind-up rather than keep on at its
    /// enemy.
    pub stand: bool,
}
impl Default for BotAbility {
    fn default() -> Self {
        Self {
            name: "Ability".into(),
            range: 2.5,
            fuse_seconds: 0.0,
            cancel_range: 6.0,
            cooldown_seconds: 2.0,
            sight: true,
            action: None,
            emote: None,
            sound: None,
            sound_seconds: 0.5,
            stand: false,
        }
    }
}
/// What a bot notices, from engine data only: a blast's radius, a sound's
/// volume, someone staring at it, something moving fast; and how its
/// reaction to a new target scales with what it was doing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotPerception {
    /// Scales how far every source is noticed (a blast's radius, a sound's
    /// volume, a stare or fast motion within part of its sight); 0 turns
    /// glances off.
    pub salience: f32,
    /// How long a glance holds, give or take a quarter.
    pub glance_seconds: f32,
    /// Seconds after a glance before the next.
    pub cooldown_seconds: f32,
    /// How human its noticing and aim are, 0 to 4 (1 shipped): it scales
    /// the reaction delay, the starting aim error, the view-cone delay and
    /// slower turn, the warning delay, the turn's overshoot, the idle drift
    /// and the aim error that remains however long it tracks, together. 0
    /// keeps exactly `reaction_seconds`, the plain narrowing of
    /// `aim_error_degrees` and the plain linear turn.
    pub strength: f32,
    /// While strolling or playing about, `reaction_seconds` and the starting
    /// aim error are scaled by this (already fighting or hunting, they are
    /// the kind's plain numbers)...
    pub relaxed_scale: f32,
    /// ...and by this more for a target outside its view cone, toward which
    /// it also turns this many times slower until it has reacted.
    pub away_scale: f32,
    /// How wide its view cone is, in degrees.
    pub view_degrees: f32,
}
impl Default for BotPerception {
    fn default() -> Self {
        Self {
            salience: 1.0,
            glance_seconds: 0.8,
            cooldown_seconds: 6.0,
            strength: 1.0,
            relaxed_scale: 1.8,
            away_scale: 1.5,
            view_degrees: 180.0,
        }
    }
}
/// How a bot holds a choice (which behaviour, weapon, aim, route): the
/// one rule against flip-flopping (`docs/architecture/bots.md`, "Holding
/// a choice"). A choice is held at least `seconds`, and after that another
/// takes over only by scoring more than [`HOLD_MARGIN`] (a share) above it. An
/// interrupt (urgent damage, an objective picked up or dropped, the target
/// lost or dead, a choice no longer possible, a must-do behaviour) takes
/// over at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotHold {
    pub seconds: f32,
}
impl Default for BotHold {
    fn default() -> Self {
        Self { seconds: 0.5 }
    }
}
/// The share over a held choice's score another needs to take over.
pub const HOLD_MARGIN: f32 = 0.1;

/// A bot's own avatar: parts by name in each slot, paint by slot, face
/// and decal by name, each only where the server's avatar pack has it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotLook {
    pub parts: std::collections::BTreeMap<String, String>,
    pub colors: std::collections::BTreeMap<String, [f32; 4]>,
    pub face: Option<String>,
    pub decal: Option<String>,
}
/// An attack a bot makes with its body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BotMelee {
    /// Health taken per hit.
    pub damage: f32,
    /// How far from its eye a hit lands, in world units.
    #[serde(default = "melee_reach")]
    pub reach: f32,
    /// Seconds between hits.
    #[serde(default = "melee_seconds")]
    pub seconds: f32,
    /// The action its model plays with each hit (`activate2` swings both
    /// arms on the Blockhead); none plays nothing.
    #[serde(default)]
    pub action: Option<String>,
    /// What the kill feed and `on_damage` hooks call the hit.
    #[serde(default = "melee_name")]
    pub name: String,
    /// A bot of another side already at or under this share of health
    /// converts on contact before damage (a zombie's bite turning a bot).
    #[serde(default)]
    pub converts_below: Option<f32>,
}
impl BotMelee {
    pub(crate) fn with_damage(damage: f32) -> Self {
        Self {
            damage,
            reach: melee_reach(),
            seconds: melee_seconds(),
            action: None,
            name: melee_name(),
            converts_below: None,
        }
    }
}
fn melee_reach() -> f32 {
    2.5
}
fn melee_seconds() -> f32 {
    1.0
}
fn melee_name() -> String {
    "Bite".into()
}
/// How a bot gets about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Moves {
    /// Walks the brick world and jets as a player does.
    #[default]
    Walk,
    /// Swims: in water it heads straight for its goal at any depth and
    /// keeps to the water; out of it, it walks back to its home.
    Swim,
}
impl Default for BotKind {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            sight: 80.0,
            wander_radius: 12.0,
            return_home: true,
            chase_radius: 48.0,
            reaction_seconds: 0.35,
            turn_degrees: 300.0,
            aim_error_degrees: 5.0,
            memory_seconds: 8.0,
            fights_bots: true,
            first_names: Vec::new(),
            body: None,
            melee: None,
            moves: Moves::Walk,
            side: None,
            look: None,
            emote: None,
            out_of_water_seconds: None,
            alerts_allies: false,
            behaviours: Default::default(),
            objective_radius: 24.0,
            perception: BotPerception::default(),
            surprise: BotSurprise::default(),
            team: BotTeam::default(),
            hold_seconds: BotHold::default().seconds,
            weapons: true,
            tactics: BotTactics::default(),
            vehicles: BotVehicles::default(),
            contact: None,
            ability: None,
            flee_below: 0.0,
            composure: BotComposure::default(),
        }
    }
}
impl BotKind {
    /// The hold its choices follow.
    pub fn hold(&self) -> BotHold {
        BotHold {
            seconds: self.hold_seconds,
        }
    }
    /// Its weight on behaviour or leg `name` (`behaviours`): 1 unless it
    /// says otherwise, but 0 for the opt-in ones (interact, objective).
    /// Interact is off for a kind that takes no seat and has no tool use
    /// to offer (`vehicles` all off).
    pub fn weight(&self, name: &str) -> f32 {
        self.behaviours
            .get(name)
            .copied()
            .unwrap_or(if matches!(name, "interact" | "objective") {
                0.0
            } else {
                1.0
            })
    }
    /// Whether it attacks with its body at all: a hit (`melee`), a touch
    /// (`contact`) or its ability.
    pub fn body_attack(&self) -> bool {
        self.melee.as_ref().is_some_and(|m| m.damage > 0.0)
            || self.contact.as_ref().is_some_and(|c| c.damage > 0.0)
            || self.ability.is_some()
    }
    /// What one bot of this kind costs the server, as a share of a full
    /// Blockhead bot, for the bot limit (`Session::bot_limit`): a minimal
    /// profile (no hand weapons, tactics, seats, teamwork, surprise or
    /// human perception) is [`MINIMAL_COST`], anything more a whole bot.
    pub fn cost(&self) -> f32 {
        let minimal = !self.weapons
            && self.tactics == BotTactics::none()
            && self.vehicles.none()
            && self.team.teamwork == 0.0
            && self.surprise.strength == 0.0
            && self.perception.strength == 0.0;
        if minimal { MINIMAL_COST } else { 1.0 }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.trim().is_empty()
                && self.id.len() <= 96
                && self
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c)),
            "Bot id `{}` must be 1-96 letters, digits or ._:/-",
            self.id
        );
        ensure!(
            !self.name.trim().is_empty()
                && self.name.chars().count() <= 32
                && !self.name.chars().any(char::is_control),
            "Bot `{}` needs a name of 1-32 characters",
            self.id
        );
        ensure!(
            self.first_names.len() <= MAX_FIRST_NAMES
                && self.first_names.iter().all(|n| {
                    !n.trim().is_empty()
                        && n.chars().count() <= 16
                        && !n.chars().any(char::is_control)
                }),
            "Bot `{}`: first_names are at most {MAX_FIRST_NAMES} names of 1-16 characters",
            self.id
        );
        ensure!(
            self.body
                .as_ref()
                .is_none_or(|b| !b.trim().is_empty() && b.len() <= 128),
            "Bot `{}`: body must name an archetype",
            self.id
        );
        if let Some(m) = &self.melee {
            for (name, value, min, max) in [
                ("damage", m.damage, 0.0, 1000.0),
                ("reach", m.reach, 0.5, 16.0),
                ("seconds", m.seconds, 0.05, 30.0),
            ] {
                ensure!(
                    value.is_finite() && (min..=max).contains(&value),
                    "Bot `{}`: melee {name} must be {min} to {max}",
                    self.id
                );
            }
            ensure!(
                !m.name.trim().is_empty()
                    && m.name.chars().count() <= 32
                    && m.action.as_ref().is_none_or(|a| {
                        !a.is_empty()
                            && a.len() <= 32
                            && a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    }),
                "Bot `{}`: melee needs a name of 1-32 characters and an action of letters, digits or _",
                self.id
            );
        }
        let short =
            |s: &str| !s.trim().is_empty() && s.len() <= 64 && !s.chars().any(char::is_control);
        ensure!(
            self.side.as_deref().is_none_or(short)
                && self.emote.as_deref().is_none_or(short)
                && self.look.as_ref().is_none_or(|l| {
                    l.parts.len() <= 16
                        && l.colors.len() <= 16
                        && l.parts.iter().all(|(k, v)| short(k) && short(v))
                        && l.colors.iter().all(|(k, c)| {
                            short(k) && c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                        })
                        && l.face.as_deref().is_none_or(short)
                        && l.decal.as_deref().is_none_or(short)
                }),
            "Bot `{}`: side, emote and look names are 1-64 characters, colours 0 to 1",
            self.id
        );
        ensure!(
            self.melee
                .as_ref()
                .and_then(|m| m.converts_below)
                .is_none_or(|c| c.is_finite() && (0.0..=1.0).contains(&c))
                && self
                    .out_of_water_seconds
                    .is_none_or(|s| s.is_finite() && (0.0..=600.0).contains(&s)),
            "Bot `{}`: converts_below is 0 to 1 and out_of_water_seconds 0 to 600",
            self.id
        );
        ensure!(
            self.behaviours.len() <= BEHAVIOURS.len() + LEG_WEIGHTS.len()
                && self.behaviours.iter().all(|(name, weight)| {
                    (BEHAVIOURS.contains(&name.as_str()) || LEG_WEIGHTS.contains(&name.as_str()))
                        && weight.is_finite()
                        && (0.0..=10.0).contains(weight)
                }),
            "Bot `{}`: behaviours weighs {}, {} by 0 to 10",
            self.id,
            BEHAVIOURS.join(", "),
            LEG_WEIGHTS.join(", ")
        );
        if let Some(c) = &self.contact {
            ensure!(
                c.damage.is_finite()
                    && (0.0..=1000.0).contains(&c.damage)
                    && c.seconds.is_finite()
                    && (0.05..=30.0).contains(&c.seconds)
                    && !c.name.trim().is_empty()
                    && c.name.chars().count() <= 32,
                "Bot `{}`: contact damage is 0 to 1000 every 0.05 to 30 seconds, with a name of 1-32 characters",
                self.id
            );
        }
        if let Some(a) = &self.ability {
            let word = |s: &str| {
                !s.is_empty()
                    && s.len() <= 96
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c))
            };
            for (name, value, min, max) in [
                ("range", a.range, 0.5, 64.0),
                ("fuse_seconds", a.fuse_seconds, 0.0, 30.0),
                ("cancel_range", a.cancel_range, 0.5, 128.0),
                ("cooldown_seconds", a.cooldown_seconds, 0.0, 600.0),
                ("sound_seconds", a.sound_seconds, 0.05, 30.0),
            ] {
                ensure!(
                    value.is_finite() && (min..=max).contains(&value),
                    "Bot `{}`: ability {name} must be {min} to {max}",
                    self.id
                );
            }
            ensure!(
                !a.name.trim().is_empty()
                    && a.name.chars().count() <= 32
                    && !a.name.chars().any(char::is_control)
                    && a.cancel_range >= a.range
                    && a.action.as_ref().is_none_or(|x| {
                        !x.is_empty()
                            && x.len() <= 32
                            && x.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    })
                    && a.emote.as_deref().is_none_or(word)
                    && a.sound.as_deref().is_none_or(word),
                "Bot `{}`: an ability needs a name of 1-32 characters, a cancel_range no shorter than its range, an action of letters, digits or _, and emote and sound ids",
                self.id
            );
        }
        let ranges = [
            ("flee_below", self.flee_below, 0.0, 1.0),
            ("sight", self.sight, 0.0, 400.0),
            ("wander_radius", self.wander_radius, 0.0, 64.0),
            ("chase_radius", self.chase_radius, 0.0, 256.0),
            ("reaction_seconds", self.reaction_seconds, 0.0, 5.0),
            ("turn_degrees", self.turn_degrees, 10.0, 3600.0),
            ("aim_error_degrees", self.aim_error_degrees, 0.0, 45.0),
            ("memory_seconds", self.memory_seconds, 0.0, 60.0),
            ("objective_radius", self.objective_radius, 1.0, 128.0),
            ("hold_seconds", self.hold_seconds, 0.0, 5.0),
            ("perception.salience", self.perception.salience, 0.0, 8.0),
            (
                "perception.glance_seconds",
                self.perception.glance_seconds,
                0.05,
                5.0,
            ),
            (
                "perception.cooldown_seconds",
                self.perception.cooldown_seconds,
                0.0,
                120.0,
            ),
            ("perception.strength", self.perception.strength, 0.0, 4.0),
            (
                "perception.relaxed_scale",
                self.perception.relaxed_scale,
                0.0,
                8.0,
            ),
            (
                "perception.away_scale",
                self.perception.away_scale,
                1.0,
                8.0,
            ),
            (
                "perception.view_degrees",
                self.perception.view_degrees,
                10.0,
                360.0,
            ),
        ];
        for (name, value, min, max) in ranges {
            ensure!(
                value.is_finite() && (min..=max).contains(&value),
                "Bot `{}`: {name} must be {min} to {max}",
                self.id
            );
        }
        self.surprise
            .validate()
            .with_context(|| format!("Bot `{}`", self.id))?;
        self.team
            .validate()
            .with_context(|| format!("Bot `{}`", self.id))?;
        self.composure
            .validate()
            .with_context(|| format!("Bot `{}`", self.id))?;
        Ok(())
    }
}

/// A `bots.json` file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BotPack {
    pub schema_version: u32,
    pub bots: Vec<BotKind>,
}
impl BotPack {
    /// Reads a `bots.json`. A `//` outside a string starts a comment that
    /// runs to the end of its line, so tunables can say what they do.
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        let raw: serde_json::Value =
            serde_json::from_slice(&strip_comments(bytes)).context("bots.json")?;
        for bot in raw
            .get("bots")
            .and_then(|b| b.as_array())
            .into_iter()
            .flatten()
        {
            for (path, _) in tuning::numbers(bot) {
                ensure!(
                    tuning::settable(&path),
                    "bots.json: `{path}` is fixed in code; a kind sets {}",
                    tuning::SETTABLE.join(", ")
                );
            }
        }
        let pack: Self = serde_json::from_value(raw).context("bots.json")?;
        ensure!(
            pack.schema_version == SCHEMA_VERSION,
            "bots.json schema_version must be {SCHEMA_VERSION}"
        );
        ensure!(pack.bots.len() <= MAX_KINDS, "Too many bot kinds");
        for bot in &pack.bots {
            bot.validate()?;
        }
        Ok(pack)
    }
    /// Every provider's kinds in order; a later id replaces an earlier one.
    pub fn merge(packs: impl IntoIterator<Item = BotPack>) -> Result<Vec<BotKind>> {
        let mut out: Vec<BotKind> = Vec::new();
        for pack in packs {
            for bot in pack.bots {
                match out.iter_mut().find(|b| b.id == bot.id) {
                    Some(slot) => *slot = bot,
                    None => out.push(bot),
                }
            }
        }
        ensure!(out.len() <= MAX_KINDS, "Too many bot kinds");
        Ok(out)
    }
}

/// `bytes` with every `//` comment outside a JSON string blanked out
/// (spaces keep error positions where they were).
fn strip_comments(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let (mut string, mut escaped, mut comment) = (false, false, false);
    for i in 0..out.len() {
        let c = out[i];
        if comment {
            if c == b'\n' {
                comment = false;
            } else {
                out[i] = b' ';
            }
        } else if string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                string = false;
            }
        } else if c == b'"' {
            string = true;
        } else if c == b'/' && out.get(i + 1) == Some(&b'/') {
            comment = true;
            out[i] = b' ';
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Comments explain tunables; a `//` inside a string is text.
    #[test]
    fn comments_are_skipped_but_not_inside_strings() {
        let pack = BotPack::from_json(
            br#"{"schema_version":1, // the format
            "bots":[{"id":"bot.a","name":"A // B", // its name
            "surprise":{"strength":0.25}}]}"#,
        )
        .unwrap();
        assert_eq!(pack.bots[0].name, "A // B");
        assert_eq!(pack.bots[0].surprise.strength, 0.25);
        // On by default.
        assert_eq!(BotKind::default().surprise.strength, 0.6);
        for bad in [
            r#""surprise":{"strength":2}"#,
            r#""surprise":{"band":-0.1}"#,
            r#""surprise":{"flavours":{"teleport":1}}"#,
            r#""surprise":{"loud":1}"#,
            r#""team":{"teamwork":1.5}"#,
            r#""team":{"callouts":{"shout":"Hi"}}"#,
        ] {
            let json = format!(r#"{{"schema_version":1,"bots":[{{"id":"x","name":"X",{bad}}}]}}"#);
            assert!(BotPack::from_json(json.as_bytes()).is_err(), "{bad}");
        }
    }
    #[test]
    fn packs_validate_and_later_ids_replace_earlier_ones() {
        let a = BotPack::from_json(br#"{"schema_version":1,"bots":[{"id":"bot.a","name":"A"}]}"#)
            .unwrap();
        let b = BotPack::from_json(
            br#"{"schema_version":1,"bots":[{"id":"bot.a","name":"A2","sight":20},{"id":"bot.b","name":"B"}]}"#,
        )
        .unwrap();
        let merged = BotPack::merge([a, b]).unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].name, "A2");
        assert_eq!(merged[0].sight, 20.0);
        assert!(
            BotPack::from_json(br#"{"schema_version":1,"bots":[{"id":"x","name":""}]}"#).is_err()
        );
        assert!(
            BotPack::from_json(
                br#"{"schema_version":1,"bots":[{"id":"x","name":"X","sight":-1}]}"#
            )
            .is_err()
        );
        assert!(
            BotPack::from_json(br#"{"schema_version":1,"bots":[{"id":"x","name":"X","speed":2}]}"#)
                .is_err()
        );
    }
    #[test]
    fn a_kind_sets_only_what_it_is_and_the_main_dials() {
        let pack = BotPack::from_json(
            br#"{"schema_version":1,"bots":[{"id":"x","name":"X","sight":40,"behaviours":{"chase":0.5},"surprise":{"strength":0.2,"flavours":{"spray":0}},"team":{"teamwork":1,"mood":0.5,"pressure":0.2},"perception":{"strength":2},"hold_seconds":1}]}"#,
        )
        .unwrap();
        let read = &pack.bots[0];
        assert_eq!(read.perception.strength, 2.0);
        assert_eq!(read.hold().seconds, 1.0);
        assert_eq!(read.team.mood, 0.5);
        assert_eq!(read.surprise.flavour_weight("spray"), 0.0);
        // Everything else keeps its fixed value.
        for fixed in [
            r#""hold":{"seconds":1}"#,
            r#""fighting":{"strafe_seconds":2}"#,
            r#""perception":{"glance_seconds":1}"#,
            r#""team":{"mood_cap":4}"#,
            r#""mounted":{"chase_radius":10}"#,
        ] {
            let json =
                format!(r#"{{"schema_version":1,"bots":[{{"id":"x","name":"X",{fixed}}}]}}"#);
            assert!(BotPack::from_json(json.as_bytes()).is_err(), "{fixed}");
        }
        // How quick and how long it remembers are the kind's own.
        let quick = BotPack::from_json(
            br#"{"schema_version":1,"bots":[{"id":"x","name":"X","reaction_seconds":0.1,"memory_seconds":2}]}"#,
        )
        .unwrap();
        assert_eq!(quick.bots[0].reaction_seconds, 0.1);
        assert_eq!(quick.bots[0].memory_seconds, 2.0);
        assert!(tuning::with_dial(read, "team.mood_cap", 1.0).is_err());
        assert!(
            tuning::dials(read)
                .iter()
                .all(|(path, _)| tuning::settable(path))
        );
    }
    #[test]
    fn the_shipped_kind_keeps_the_fixed_values() {
        // The Blockhead's bots.json sets only what SETTABLE allows, so its
        // other numbers are the code's.
        let pack = BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap();
        let shipped = &pack.bots[0];
        let fixed = BotKind::default();
        assert_eq!(shipped.perception, fixed.perception);
        assert_eq!(shipped.surprise.strength, fixed.surprise.strength);
        assert_eq!(shipped.team.teamwork, fixed.team.teamwork);
    }
    #[test]
    fn body_melee_and_swimming_are_read_and_limited() {
        let pack = BotPack::from_json(
            br#"{"schema_version":1,"bots":[{"id":"bot.fish","name":"Fish","body":"fish:archetype/fish","moves":"swim","melee":{"damage":25,"reach":3,"seconds":0.8,"action":"activate2"}}]}"#,
        )
        .unwrap();
        let fish = &pack.bots[0];
        assert_eq!(fish.moves, Moves::Swim);
        assert_eq!(fish.body.as_deref(), Some("fish:archetype/fish"));
        let melee = fish.melee.as_ref().unwrap();
        assert_eq!((melee.damage, melee.reach, melee.seconds), (25.0, 3.0, 0.8));
        assert_eq!(melee.name, "Bite");
        assert!(
            fish.weapons,
            "a kind fights with hand weapons unless it says not"
        );
        let unarmed = BotPack::from_json(
            br#"{"schema_version":1,"bots":[{"id":"x","name":"X","weapons":false}]}"#,
        )
        .unwrap();
        assert!(!unarmed.bots[0].weapons);
        assert_eq!(BotKind::default().moves, Moves::Walk);
        for bad in [
            r#""melee":{"damage":-1}"#,
            r#""melee":{"damage":5,"seconds":0}"#,
            r#""melee":{"damage":5,"action":"a b"}"#,
            r#""moves":"fly""#,
            r#""body":" ""#,
        ] {
            let json = format!(r#"{{"schema_version":1,"bots":[{{"id":"x","name":"X",{bad}}}]}}"#);
            assert!(BotPack::from_json(json.as_bytes()).is_err(), "{bad}");
        }
    }
}
