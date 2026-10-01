//! The operations package behaviour may ask the engine to perform, and the
//! one place they are checked against a package's declared capabilities.
use bri_package::diag::Diagnostic;
use bri_package::setting::SettingValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The plant errors [`Op::PlantError`] shows: v20's `MsgPlantError_`
/// names, lower-case, as `on_place` reports them (`too_far`).
pub const PLANT_ERRORS: [&str; 7] = [
    "overlap", "float", "stuck", "buried", "too_far", "limit", "flood",
];
/// Most boxes one `show_shapes` set holds.
pub const MAX_SHAPES: usize = 64;
/// Longest label a shape carries, characters.
pub const MAX_SHAPE_LABEL: usize = 48;
/// Longest key naming a set of shapes, bytes.
pub const MAX_SHAPE_KEY: usize = 64;

/// A box [`Op::ShowShapes`] draws in the world for every player, unlit:
/// its faces in `color` seen from outside and `inside` seen from within
/// (alpha 0 draws no face), and `label` over its top centre like a
/// player's name, in `color` at full strength. Torque Add-Ons draw these with scaled `StaticShape`s
/// (the New Duplicator's selection box).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldShape {
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Straight RGBA, 0-255.
    pub color: [u8; 4],
    #[serde(default)]
    pub inside: [u8; 4],
    /// Outside colours of the faces across x, y and z, instead of `color`
    /// (`setNodeColor("out+X", …)`: a shaded cube).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sides: Option<[[u8; 4]; 3]>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}
impl WorldShape {
    /// The outside colour of the faces across each axis.
    pub fn outside(&self) -> [[u8; 4]; 3] {
        self.sides.unwrap_or([self.color; 3])
    }
}
impl WorldShape {
    /// Shape limits: a finite box no side longer than [`MAX_BOX_SPAN`]
    /// (and its frame), a short one-line label.
    pub fn check(&self) -> bool {
        let finite = |v: &[f32; 3]| v.iter().all(|x| x.is_finite() && x.abs() <= 1_000_000.0);
        finite(&self.min)
            && finite(&self.max)
            && (0..3).all(|a| {
                self.max[a] >= self.min[a] && self.max[a] - self.min[a] <= MAX_BOX_SPAN + 16.0
            })
            && self.label.chars().count() <= MAX_SHAPE_LABEL
            && !self.label.chars().any(char::is_control)
    }
}
/// A set of shapes' key: short, printable, no spaces.
pub fn shape_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_SHAPE_KEY && key.bytes().all(|b| b.is_ascii_graphic())
}

/// Variables an entity may be given when it is spawned.
pub const MAX_SPAWN_VARS: usize = 16;
/// Most players one `tell` to a list reaches (a server's most players).
pub const MAX_TELL_PLAYERS: usize = 256;
/// Fastest a script may set anything moving, units per second.
pub const MAX_PUSH_SPEED: f32 = 200.0;
/// Fastest projectile `fire` launches, units a second (the weapons
/// runtime's own limit).
pub const MAX_FIRE_SPEED: f32 = 10_000.0;
/// The mass scripts see for a player or entity body (Torque's player
/// `mass` is 90 as well).
pub const PLAYER_MASS: f32 = 90.0;
/// Farthest ahead of a player's eye a held object may float.
pub const MAX_HOLD_DISTANCE: f32 = 64.0;
/// Longest a tether's rope may be, and shortest, units (the player
/// motor's own limits).
pub const MAX_TETHER_LENGTH: f32 = 1000.0;
pub const MIN_TETHER_LENGTH: f32 = 1.0;
/// Fastest a tether reels, units a second.
pub const MAX_TETHER_REEL: f32 = 80.0;
/// Strongest push a tether's swing gives, units a second squared.
pub const MAX_TETHER_SWING: f32 = 60.0;
/// Strongest a hold may pull, in mass units times units per second
/// squared: what it gives a thing of mass `m` is at most `force / m`.
pub const MAX_HOLD_FORCE: f32 = 1.0e7;
/// Longest ray `raycast` casts, and longest `beam`, in units.
pub const MAX_RAY_RANGE: f32 = 2000.0;
/// Rays one script call may cast.
pub const MAX_RAYS_PER_CALL: usize = 64;
/// The field of view `set_fov` may give, degrees (Torque's player camera
/// `cameraMinFov` and `cameraMaxFov`).
pub const FOV_RANGE: std::ops::RangeInclusive<f32> = 5.0..=120.0;
/// The most `set_speed_scale` may ask for (the motor's own limit).
pub const MAX_SPEED_SCALE: f32 = 4.0;
/// The most rounds `give_ammo`, `set_reserve` or `set_rounds` may name (a
/// magazine's own reserve limit).
pub const MAX_AMMO_ROUNDS: u64 = 100_000;
/// Longest side of a box `copy_box` copies or `show_box` outlines, units
/// (2048 studs: the New Duplicator's largest admin box). What a box holds
/// is bounded by brick counts, not its size.
pub const MAX_BOX_SPAN: f32 = 1024.0;
/// Most bricks one copy may hold (`copy_build`, `copy_box`,
/// `load_copy`): the New Duplicator's limit for administrators. Big copies
/// are selected, planted, cut, painted and loaded a slice each tick.
pub const MAX_COPY_BRICKS: u32 = 1_000_000;
/// Most bricks one `paint_fill` may paint: v20's Fill Can lets
/// administrators fill 128000.
pub const MAX_FILL_BRICKS: usize = 128_000;
/// Widest gap `paint_fill`'s `reach` may jump, units.
pub const MAX_FILL_REACH: f32 = 4.0;
/// Longest a `temp_look` lasts, seconds.
pub const MAX_TEMP_LOOK_SECONDS: f32 = 60.0;
/// Widest `beam`, units, and longest it lasts, seconds.
pub const MAX_BEAM_WIDTH: f32 = 16.0;
pub const MAX_BEAM_SECONDS: f32 = 10.0;
/// Longest a `play_thread` may wait before it plays, seconds
/// (`%player.schedule(ms, "playThread", ...)`).
pub const MAX_THREAD_DELAY: f32 = 60.0;
/// Widest sphere `set_map_lights` covers, units, and brightest it makes a
/// light (times its recovered colour).
pub const MAX_LIGHT_RADIUS: f32 = 2000.0;
pub const MAX_LIGHT_TINT: f32 = 4.0;

/// Something in the world that moves: a player, a vehicle (any loose
/// physics body: cars, balls, tumbling bodies) or a package entity.
/// Scripts name one as `"player:3"`, `"vehicle:12"` or `"entity:7"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ObjectRef {
    Player(u64),
    Vehicle(u64),
    Entity(u64),
}
impl ObjectRef {
    pub fn parse(text: &str) -> Option<Self> {
        let (kind, id) = text.split_once(':')?;
        let id: u64 = id.parse().ok()?;
        match kind {
            "player" => Some(Self::Player(id)),
            "vehicle" => Some(Self::Vehicle(id)),
            "entity" => Some(Self::Entity(id)),
            _ => None,
        }
    }
    pub fn kind(self) -> &'static str {
        match self {
            Self::Player(_) => "player",
            Self::Vehicle(_) => "vehicle",
            Self::Entity(_) => "entity",
        }
    }
    pub fn id(self) -> u64 {
        match self {
            Self::Player(id) | Self::Vehicle(id) | Self::Entity(id) => id,
        }
    }
}
impl std::fmt::Display for ObjectRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind(), self.id())
    }
}

/// What a `paint_fill` or `paint_copy` paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillPaint {
    /// A palette colour (the colour spray cans).
    Color(u8),
    /// A colour effect, as the colour FX cans number them from 0 (none,
    /// pearl, chrome, glow, blink, swirl, rainbow).
    ColorEffect(u8),
    /// A shape effect, as the shape FX cans number them from 0 (none,
    /// undulo, water).
    ShapeEffect(u8),
}
impl FillPaint {
    /// Whether the effect is one the FX cans have.
    pub fn valid(self) -> bool {
        match self {
            Self::Color(_) => true,
            Self::ColorEffect(fx) => fx <= 6,
            Self::ShapeEffect(fx) => fx <= 2,
        }
    }
}

/// What a `paint_vehicle` paints.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VehiclePaint {
    /// A palette colour (the colour spray cans). A vehicle its spawn brick
    /// recolours takes it through the brick, which is painted too.
    Color(u8),
    /// Any colour, red, green and blue from 0 to 1, on the vehicle alone.
    Rgb([f32; 3]),
}

/// How `temp_look` changes a player for a while.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TempLook {
    /// Every colour slot this colour, and no decal (`SetTempColor` with
    /// no position).
    pub color: Option<[f32; 4]>,
    /// The same with a palette colour, by index (`getColorIDTable`).
    pub paint: Option<u8>,
    /// This face (a face decal's name, `setFaceName`).
    pub face: Option<String>,
    /// These slots keep their colour at this opacity, where the player
    /// wears that part (`setNodeColor` on a visor).
    pub alpha: BTreeMap<String, f32>,
}

/// Every capability a manifest may declare (with plain-language words in
/// `bri_package::capability`).
pub use bri_package::capability::CAPABILITIES;

/// What the body of a player under an [`Op::OrbitCamera`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrbitBody {
    /// It keeps its trigger: the body takes no moves, but a click is the
    /// player's empty-hand trigger for Add-Ons (`on_activate`), as
    /// Throwing's "Grabbed" camera struggles (`Observer::onTrigger`). Only
    /// a living player on their body or in another such orbit gets one,
    /// never around themselves, and `None` ends only this kind.
    #[default]
    Acts,
    /// It freezes (`setControlObject(%client.camera)`): the body takes no
    /// actions and the player's keys go to the rules (`on_observer`), as a
    /// spectator's do. A rule's `watch`: given from the body or another
    /// rules camera, dead or alive, around the player themselves too (a
    /// dead player's own is the corpse camera); `None` ends any rules
    /// camera.
    Frozen,
}
/// An orbit camera ([`Op::OrbitCamera`]): around player `target`, starting
/// `distance` whole units out, which the player's wheel zooms between `min`
/// and `max` (`setOrbitMode(%target, %transform, %min, %max, %cur)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Orbit {
    pub target: u64,
    pub min: u8,
    pub max: u8,
    pub distance: u8,
}
impl Orbit {
    /// Within [`ORBIT_DISTANCE`], `min <= distance <= max`.
    pub fn valid(&self) -> bool {
        ORBIT_DISTANCE.contains(&self.min)
            && ORBIT_DISTANCE.contains(&self.max)
            && self.min <= self.distance
            && self.distance <= self.max
    }
}

/// The camera [`Op::Camera`] gives.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraOp {
    Free,
    Point { at: [f32; 3], distance: f32 },
}
/// Nearest and farthest an orbit camera sits from its point.
pub const ORBIT_DISTANCES: std::ops::RangeInclusive<f32> = 0.5..=100.0;

/// Most knots a camera path holds (`PathCameraData.maxNodes`).
pub const MAX_PATH_KNOTS: usize = 20;
/// How a knot shapes the camera path through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnotKind {
    Normal,
    Kink,
    PositionOnly,
}
/// One knot of a camera path: where the camera is and looks, its speed to
/// the next knot in units per second, and how the path passes it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PathKnot {
    pub at: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
    pub kind: KnotKind,
    pub linear: bool,
    pub jump: bool,
}
/// What [`Op::SetGameRule`] changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GameRule {
    /// The server's default game, which players in none join (Slayer's
    /// Default Minigame), or no longer.
    Default(bool),
    /// A paint palette colour in place of its v20 colour, or none.
    PaintColor(Option<u8>),
    /// Bricks outside this box are not the game's, or none are.
    Region(Option<[[f32; 3]; 2]>),
    /// Scores carry over resets.
    KeepScores(bool),
    /// Whether leaving clears a member's event objects and schedules and
    /// respawns their vehicles.
    Cleanup { leave: bool },
    /// While it uses every player's bricks, it claims those of builders in
    /// no game.
    ClaimsBricks(bool),
    /// How far away its members' names show, or v20's own distance.
    NameDistance(Option<u32>),
    /// Change its own settings: the fields given, over what it has.
    Settings(serde_json::Value),
    /// End the game.
    End,
}
/// Longest JSON a mini-game's settings change may be.
pub const MAX_SETTINGS_JSON: usize = 4096;
/// Most keys one package keeps on the host (`set_host_data`).
pub const MAX_HOST_KEYS: usize = 64;
/// Most bytes one kept value takes, as JSON.
pub const MAX_HOST_VALUE: usize = 256 * 1024;
/// Whether `key` is one rules may keep a value under: an identifier.
pub fn valid_host_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 48
        && key.starts_with(|c: char| c.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
fn settings_json_ok(v: &serde_json::Value) -> bool {
    v.is_object() && serde_json::to_string(v).is_ok_and(|t| t.len() <= MAX_SETTINGS_JSON)
}
/// One team as [`Op::SetTeams`] asks for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamOp {
    pub id: Option<u64>,
    pub name: String,
    /// The team's paint palette index.
    pub color: u8,
}
/// Longest a dropped item may lie, seconds.
pub const MAX_DROP_SECONDS: u32 = 600;
/// Largest data a dropped item carries, bytes of JSON.
pub const MAX_DROP_DATA_BYTES: usize = 1024;
/// Most players one `end_round` names as winners.
pub const MAX_ROUND_WINNERS: usize = 256;
/// Most teams one mini-game may have, and the longest team name.
pub const MAX_TEAMS: usize = 64;
pub const MAX_TEAM_NAME: usize = 50;
/// Largest score `set_score` sets or adds.
pub const MAX_SCORE: i64 = 1_000_000_000;
/// The name a copy is saved under, from what a player typed: the file
/// name only (v20's `fileBase`, so a path or a `.bls` ending is dropped),
/// 1 to 64 letters, digits, spaces and `_ - ( ) .`, not starting with a
/// dot. `None` when nothing usable is left.
pub fn copy_name(typed: &str) -> Option<String> {
    let base = typed.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let base = base
        .strip_suffix(".bls")
        .or_else(|| base.strip_suffix(".BLS"))
        .unwrap_or(base)
        .trim();
    let ok = (1..=64).contains(&base.chars().count())
        && !base.starts_with('.')
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " _-().".contains(c));
    ok.then(|| base.to_string())
}

/// Which way a stack copy ([`Op::CopyBuild`]) goes from the clicked
/// brick: `up` takes what is built on it, else what it is built on;
/// `limited` keeps the stack on that side of the clicked brick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackReach {
    pub up: bool,
    pub limited: bool,
}
/// v20's trust levels a copy may ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyTrust {
    /// None: anyone's bricks.
    None,
    /// Build on their bricks.
    Build,
    /// Also paint and hammer them (v20's duplicators asked this).
    Full,
    /// Only their own (v20's "Self" trust, which no one gives another).
    Own,
}
/// The Add-On's rules for the bricks a copy takes and how it plants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyRule {
    /// The trust a player needs in a brick's owner to copy it.
    pub trust: CopyTrust,
    /// Public bricks (no owner) may be copied.
    pub public: bool,
    /// Administrators may copy any brick.
    pub admin: bool,
    /// Planting the copy plants each brick that fits and skips the rest,
    /// as v20's Duplorcator did, rather than all or nothing.
    pub partial: bool,
    /// The same trust in the owner of a brick's stack (who owns the bricks
    /// it was built on, v20's `stackBL_ID`) also lets a player copy it, and,
    /// with full trust, cut, paint or wrench it through the copy (the New
    /// Duplicator's trust checks).
    #[serde(default)]
    pub stack: bool,
}
impl Default for CopyRule {
    fn default() -> Self {
        Self {
            trust: CopyTrust::Build,
            public: true,
            admin: true,
            partial: false,
            stack: false,
        }
    }
}
/// How a copy ([`Op::CopyBuild`], [`Op::CopyBox`]) is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CopyHold {
    /// Held as a selection, not yet shown to place ([`Op::ShowCopy`]).
    pub hidden: bool,
    /// Added to the copy the player holds from this package, rather than
    /// replacing it (a duplicator's multi-select).
    pub add: bool,
}
/// What a player's mouse wheel picks ([`Op::SetScrollMode`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrollMode {
    None,
    Bricks,
    Paint,
    Tools,
}
impl ScrollMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "bricks" => Some(Self::Bricks),
            "paint" => Some(Self::Paint),
            "tools" => Some(Self::Tools),
            _ => None,
        }
    }
}
/// The mirror [`Op::MirrorCopy`] stands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirrorAxis {
    /// Across the world's x axis: east and west swap.
    X,
    /// Across the world's z axis: north and south swap.
    Z,
    /// Left and right as the player faces swap.
    View,
    /// Up and down: the copy turns upside down where it stands.
    Y,
}
impl MirrorAxis {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "x" => Some(Self::X),
            "z" => Some(Self::Z),
            "view" => Some(Self::View),
            "y" => Some(Self::Y),
            _ => None,
        }
    }
}
/// Where [`Op::Sound`] plays.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SoundAt {
    /// In the world, heard by everyone near it.
    Position([f32; 3]),
    /// At one player's ears only.
    Player(u64),
}
/// Most tool slots `set_tools` lists.
pub const MAX_TOOL_SLOTS: usize = 10;
/// The longest respawn time `set_respawn_time` sets (Slayer's 999 s).
pub const MAX_RESPAWN_MS: u32 = 999_999;
/// Longest bot name, in characters (a player name's limit).
pub const MAX_BOT_NAME_CHARS: usize = 23;
/// Bots one server runs at once, from spawn bricks and rules together.
pub const MAX_BOTS: usize = 16;
/// Mount points a body may have (`mountObject`'s node).
pub const MAX_MOUNT_POINTS: usize = 8;
/// Body scales `set_scale` allows.
pub const SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.2..=5.0;
/// How far out an Add-On's orbit camera may sit ([`Op::OrbitCamera`]), in
/// whole units.
pub const ORBIT_DISTANCE: std::ops::RangeInclusive<u8> = 1..=20;
/// Where a rule's `watch` sits: the corpse camera's distance
/// (`Observer::setMode("Corpse")`, 8 units).
pub const WATCH_DISTANCE: u8 = 8;
/// The avatar's part slots, each holding one of the avatar pack's choices
/// (`$pref::Avatar::Hat` and the rest).
pub const AVATAR_PARTS: [&str; 12] = [
    "hat",
    "accent",
    "pack",
    "secondpack",
    "chest",
    "hip",
    "rarm",
    "larm",
    "rhand",
    "lhand",
    "rleg",
    "lleg",
];
/// The avatar's colour slots, as `setNodeColor` names them.
pub const AVATAR_SLOTS: [&str; 13] = [
    "head",
    "torso",
    "hat",
    "accent",
    "pack",
    "secondpack",
    "hip",
    "rarm",
    "larm",
    "rhand",
    "lhand",
    "rleg",
    "lleg",
];
/// Longest key of a value kept on a brick (`set_brick_field`).
pub const MAX_BRICK_FIELD_KEY: usize = 32;
/// Whether `key` may name a value a package keeps on a brick.
pub fn is_brick_field_key(key: &str) -> bool {
    (1..=MAX_BRICK_FIELD_KEY).contains(&key.len())
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
/// Longest text a print may show.
pub const MAX_PRINT_CHARS: usize = 512;

/// One operation a script may ask for: a struct in the module of the
/// capability it needs (`ops/<capability>.rs`), listed once in `list.rs`.
pub trait ScriptOp: Into<Op> {
    /// The capability a package declares to use it.
    const CAPABILITY: &str;
    /// Its name in scripts and diagnostics.
    const NAME: &str;
    /// Shape limits, independent of who asks.
    fn bounded(&self) -> bool;
}

macro_rules! declare_ops {
    ($($op:ident = $module:ident,)*) => {
        /// An operation a script asked for (see [`ScriptOp`]).
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub enum Op {
            $($op($module::$op),)*
        }
        $(impl From<$module::$op> for Op {
            fn from(op: $module::$op) -> Self {
                Self::$op(op)
            }
        })*
        impl Op {
            pub fn capability(&self) -> &'static str {
                match self {
                    $(Self::$op(_) => <$module::$op as ScriptOp>::CAPABILITY,)*
                }
            }
            /// Its name in scripts and diagnostics.
            pub fn name(&self) -> &'static str {
                match self {
                    $(Self::$op(_) => <$module::$op as ScriptOp>::NAME,)*
                }
            }
            /// Shape limits, independent of who asks.
            fn bounded(&self) -> Result<(), String> {
                let ok = match self {
                    $(Self::$op(op) => op.bounded(),)*
                };
                if ok {
                    Ok(())
                } else {
                    Err(format!("{self:?} is outside the operation's limits"))
                }
            }
        }
    };
}
include!("list.rs");
for_each_op!(declare_ops);

/// Shape checks the operations share.
pub(crate) mod limits {
    use super::MAX_BOX_SPAN;
    /// Finite and within a million units of the origin.
    pub fn finite(v: &[f32]) -> bool {
        v.iter().all(|x| x.is_finite() && x.abs() <= 1_000_000.0)
    }
    /// A chat line: some text, at most 256 bytes, no control characters.
    pub fn chat(t: &str) -> bool {
        !t.trim().is_empty() && t.len() <= 256 && !t.chars().any(char::is_control)
    }
    /// An item (weapon) content reference.
    pub fn item(t: &str) -> bool {
        bri_package::id::is_content_ref(t, Some("weapon"))
    }
    /// A magazine's ammo type: 1 to 32 letters, digits, `.`, `_` or `-`.
    pub fn ammo_name(t: &str) -> bool {
        (1..=32).contains(&t.len())
            && t.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    }
    /// A box from `min` to `max`, each side at most `MAX_BOX_SPAN`.
    pub fn span(min: &[f32; 3], max: &[f32; 3]) -> bool {
        finite(min)
            && finite(max)
            && (0..3).all(|a| max[a] >= min[a] && max[a] - min[a] <= MAX_BOX_SPAN)
    }
    pub fn glam_length(v: &[f32; 3]) -> f32 {
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    }
}
use limits::*;

/// Check an operation a package asked for. This is the single capability
/// gate: every package operation passes through it before the engine acts.
/// Ownership checks that need live state (a package may steer only its own
/// entities) are the caller's second step and report through the same codes.
pub fn authorize(package: &str, capabilities: &[String], op: &Op) -> Result<(), Diagnostic> {
    op.bounded()
        .map_err(|m| Diagnostic::error("op.bounds", m).at(package))?;
    let needed = op.capability();
    if !capabilities.iter().any(|c| c == needed) {
        return Err(Diagnostic::error(
            "op.capability",
            format!("{} needs capability `{needed}`, which the package does not declare", op.name()),
        )
        .at(package)
        .hint(format!("add \"{needed}\" to capabilities in package.json; the server owner sees it when enabling the package")));
    }
    if let Op::SpawnEntity(SpawnEntity { kind, .. }) = op
        && kind.split(':').next() != Some(package)
    {
        return Err(Diagnostic::error(
            "op.foreign_entity",
            format!("cannot spawn `{kind}`: packages spawn only their own entity kinds"),
        )
        .at(package));
    }
    Ok(())
}
pub fn op_name(op: &Op) -> &'static str {
    op.name()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_kit_and_respawn_ops_stay_in_their_limits() {
        let parts = |slot: &str, part: &str| {
            Op::SetAvatarParts(SetAvatarParts {
                player: 1,
                parts: BTreeMap::from([(slot.to_owned(), part.to_owned())]),
                face: None,
                decal: None,
            })
        };
        assert!(parts("hat", "copHat").bounded().is_ok());
        assert!(
            parts("head", "copHat").bounded().is_err(),
            "the head is a colour, not a part"
        );
        assert!(parts("hat", "").bounded().is_err());
        assert!(parts("hat", &"x".repeat(65)).bounded().is_err());
        let tools = |tools: Vec<Option<String>>| Op::SetTools(SetTools { player: 1, tools });
        assert!(
            tools(vec![Some("v20.weapon.hammeritem".into()), None])
                .bounded()
                .is_ok()
        );
        assert!(tools(vec![None; MAX_TOOL_SLOTS + 1]).bounded().is_err());
        assert!(tools(vec![Some(String::new())]).bounded().is_err());
        let respawn = |ms| Op::SetRespawnTime(SetRespawnTime { player: 1, ms });
        assert!(respawn(None).bounded().is_ok());
        assert!(respawn(Some(MAX_RESPAWN_MS)).bounded().is_ok());
        assert!(respawn(Some(MAX_RESPAWN_MS + 1)).bounded().is_err());
        assert_eq!(respawn(None).capability(), "minigame");
        assert_eq!(parts("hat", "copHat").capability(), "player");
    }
}
