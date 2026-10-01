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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    RemoveBrick {
        brick: u64,
    },
    /// Add a world-owned brick of a known shape: an arena, a gate, a board.
    /// The colour is matched to the nearest colour of the world's palette.
    PlaceBrick {
        shape: String,
        position: [f32; 3],
        color: [f32; 4],
    },
    /// Plant a brick of kind `kind` (a brick catalog id) into build
    /// `owner` (a brick's `owner`, 0 for the world's own), palette colour `color`, `turns`
    /// clockwise quarter turns, centred at `position` (snapped to the stud
    /// and plate grid). Where it does not fit (another brick, a player,
    /// the map) nothing is planted and nothing is reported, as v20's
    /// `fxDTSBrick::plant` returned an error the script checked. A brick
    /// split, merged or piled by a rule (Trench Digging's dirt) stays its
    /// builder's.
    PlantBrick {
        kind: String,
        position: [f32; 3],
        turns: u8,
        color: u8,
        owner: u64,
    },
    /// Put a voxel of the generated world's `material` (its id) at voxel
    /// coordinates `position`: dirt thrown back into a trench. It becomes
    /// part of the world, saved with its edits, and is refused where
    /// something is in the way (a brick, a player, a vehicle).
    PlaceVoxel {
        position: [i64; 3],
        material: String,
    },
    /// Colour a player's avatar over their own colours, per avatar slot
    /// (`torso`, `larm`, `rleg`, ...): a team's uniform. An empty map
    /// gives them their own colours back. Kept across respawns.
    SetAvatarColors {
        player: u64,
        colors: BTreeMap<String, [f32; 4]>,
    },
    /// Dress a player's avatar in parts over their own choices, per part
    /// slot (`hat: "copHat"`, `pack: "none"`), and a face and decal: a
    /// team's full uniform (Slayer's `hideAllNodes` and `unHideNode`). A
    /// part, face or decal the server's avatar pack lacks is left as theirs.
    /// No parts, face or decal gives them their own back. Kept across
    /// respawns.
    SetAvatarParts {
        player: u64,
        parts: BTreeMap<String, String>,
        face: Option<String>,
        decal: Option<String>,
    },
    /// Damage players within `radius` (falling off linearly) and destroy
    /// bricks within `brick_radius`.
    Explode {
        position: [f32; 3],
        radius: f32,
        damage: f32,
        brick_radius: f32,
        /// How it looks and sounds: an explosion of the weapons pack by
        /// name (`rocketExplosion`, an imported Add-On's own); the rocket's
        /// when `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        explosion: Option<String>,
    },
    /// Damage a player, vehicle or entity (`%obj.damage`). `by` is the
    /// player credited; `damage_type` names a weapons pack damage type (its
    /// kill message, vehicle scale and whether it is a direct hit), or the
    /// package itself when `None`. Scripts decide who may be hurt; they ask
    /// the minigame rules with `can_damage`.
    Damage {
        target: ObjectRef,
        amount: f32,
        by: Option<u64>,
        damage_type: Option<String>,
    },
    /// Move a living player, keeping their facing.
    Teleport {
        player: u64,
        position: [f32; 3],
    },
    /// Give a player a new life at a spawn point, alive or dead.
    Respawn {
        player: u64,
    },
    /// Make a player this archetype (a package's `archetype` id or v20's
    /// `v20.player.<datablock>`), now and at every respawn. An empty id
    /// hands the choice back to the mini-game's player type.
    SetArchetype {
        player: u64,
        archetype: String,
    },
    /// Lay an archetype over a living player's own for a while (Kai's
    /// `pushDatablock`: a machine gunner walking slowly as they fire). The
    /// player moves as the newest one laid on, keeping the damage they
    /// have taken; `set_archetype` meanwhile changes the one underneath.
    /// Refused quietly for a body of another model, or one already laid
    /// on. All are lifted when the player dies.
    PushArchetype {
        player: u64,
        archetype: String,
    },
    /// Lift an archetype [`Op::PushArchetype`] laid on (`popDatablock`);
    /// nothing when it is not on.
    PopArchetype {
        player: u64,
        archetype: String,
    },
    /// Show a block brick in one of its block's named states (`""` for the
    /// block's own faces): a dig tool cracks it, a switch lights it.
    SetBlockState {
        brick: u64,
        state: String,
    },
    /// Hand a player's movement input to one of the package's entities
    /// (`entity`), or back to the player's own body (`None`). The avatar
    /// stands where it was while the entity is driven.
    Control {
        player: u64,
        entity: Option<u64>,
    },
    /// `vars` are the entity's first package-local variables, so what a
    /// package creates is addressable from its first think (an owner, a
    /// team, a home).
    SpawnEntity {
        kind: String,
        position: [f32; 3],
        vars: BTreeMap<String, serde_json::Value>,
    },
    RemoveEntity {
        entity: u64,
    },
    /// Walk direction on the ground plane (normalised by the engine), jump.
    Steer {
        entity: u64,
        direction: [f32; 2],
        jump: bool,
    },
    /// A short replicated label clients may present (model colours).
    Label {
        entity: u64,
        label: String,
    },
    Tell {
        player: u64,
        text: String,
    },
    Broadcast {
        text: String,
    },
    /// A chat line to every member of a mini-game (`MiniGameSO::messageAll`).
    /// One line of the package's chat share, however many members;
    /// `except` leaves one member out (`messageAllExcept`).
    TellMinigame {
        game: u64,
        text: String,
        except: Option<u64>,
    },
    /// A center or bottom print to every member of a mini-game
    /// (`centerPrintAll`, `bottomPrintAll`): one print of the share.
    PrintMinigame {
        game: u64,
        text: String,
        seconds: f32,
        bottom: bool,
    },
    /// Copy the stack at `brick` for `player` to place with `tool`, as
    /// v20's duplicators select one (`reach`, `rule`), cut short at
    /// `limit` bricks.
    CopyBuild {
        player: u64,
        brick: u64,
        limit: u32,
        reach: StackReach,
        rule: CopyRule,
        tool: String,
        hold: CopyHold,
    },
    /// Copy every brick lying wholly inside the box from `min` to `max`
    /// (world units, grown out to the stud and plate grid; not `limited`,
    /// every brick reaching into it) that `rule` lets `player` take, for
    /// them to place with `tool`, cut short at `limit` bricks.
    CopyBox {
        player: u64,
        min: [f32; 3],
        max: [f32; 3],
        limited: bool,
        limit: u32,
        rule: CopyRule,
        tool: String,
        hold: CopyHold,
    },
    /// Light the bricks `player`'s copy was taken from in the palette
    /// colour nearest `color` (RGBA), glowing, for `seconds`, then give
    /// them their own colours back, as v20's duplicators showed a
    /// selection. Everyone sees it. A negative `seconds` keeps them lit
    /// until the copy is let go or lit again; 0 puts them out now.
    HighlightCopy {
        player: u64,
        /// `None` lights them in their own colours (only the glow, as the
        /// New Duplicator did).
        color: Option<[f32; 4]>,
        seconds: f32,
    },
    /// Keep the copy `player` holds on the host under `name` (see
    /// [`copy_name`]). One saved under that name before is replaced, or,
    /// without `overwrite`, kept, and the save reports `exists`. The
    /// package's `on_copy` hears how it went (`action` `"save"`).
    SaveCopy {
        player: u64,
        name: String,
        overwrite: bool,
    },
    /// The names copies are saved under on the host that contain `filter`
    /// (any case; every one when empty), in order: the package's `on_copy`
    /// hears them (`action` `"list"`, `names`).
    ListCopies {
        player: u64,
        filter: String,
    },
    /// Give `player` the copy saved under `name` to place with `tool`, at
    /// most `limit` bricks of it (the first ones saved), replacing any copy
    /// they hold (or, with `whole`, nothing when it holds more). Saved
    /// copies are the host's: copies saved with any
    /// duplicator, and v20 duplication files in the host's saves. The
    /// package's `on_copy` hears how it went (`action` `"load"`).
    LoadCopy {
        player: u64,
        name: String,
        limit: u32,
        tool: String,
        partial: bool,
        /// Take nothing, and report `limit`, when the copy holds more.
        whole: bool,
    },
    /// Mirror the copy `player` holds across `axis`. It shows and plants
    /// mirrored; mirroring it again the same way puts it back.
    MirrorCopy {
        player: u64,
        axis: MirrorAxis,
    },
    /// Mirror `player`'s ghost brick (the brick in their hand, where it
    /// would plant) across `axis` where it stands: it becomes its mirror
    /// image, itself turned or its twin. A brick with no exact image in
    /// that mirror stays as it is and the player is told `asymmetric`.
    MirrorGhost {
        player: u64,
        axis: MirrorAxis,
        asymmetric: String,
    },
    /// Move the copy `player` holds against the surface at `point` whose
    /// outward `normal` is given, as a ghost brick is put where it is
    /// aimed: its box's middle sits half its size out along the normal,
    /// on the grid.
    MoveCopy {
        player: u64,
        point: [f32; 3],
        normal: [f32; 3],
    },
    /// Take away the copy `player` holds, as if they had never copied.
    DropCopy {
        player: u64,
    },
    /// Give `player` the copy they hold as a selection
    /// ([`CopyHold::hidden`]) to place, where it was taken.
    ShowCopy {
        player: u64,
    },
    /// Keep the copy `player` holds as a selection only: the ghost they
    /// place it with goes, the copy and the bricks it came from stay.
    HideCopy {
        player: u64,
    },
    /// Move the copy `player` places as their brick shift keys would:
    /// `offset` is studs away from and to the left of their facing and
    /// plates up, `super_shift` moves by the copy's own size.
    ShiftCopy {
        player: u64,
        offset: [i32; 3],
        super_shift: bool,
    },
    /// Turn the copy `player` places a quarter turn as their rotate keys
    /// would: 1 clockwise seen from above, -1 the other way.
    RotateCopy {
        player: u64,
        direction: i8,
    },
    /// Plant the copy `player` places where it stands, as their plant key
    /// would; with `float`, bricks with nothing under them plant this once
    /// as if they stood on the ground (v20's force plant).
    PlantCopy {
        player: u64,
        float: bool,
    },
    /// Let every plant of the copy `player` holds float, or not; with
    /// `admin_only`, only while they are an administrator (a plant that
    /// finds them not one does not float, and `on_place` says so with
    /// `float_refused`).
    FloatCopy {
        player: u64,
        float: bool,
        #[serde(default)]
        admin_only: bool,
    },
    /// After each plant of a copy, `player`'s next copy plant waits this
    /// long; one sooner is refused and `on_place` hears `error` `wait`,
    /// with the seconds left in `wait`. 0 lets them plant at once.
    PlantWait {
        player: u64,
        seconds: f32,
    },
    /// Stop `player`'s copy work that is going on over several ticks (a big
    /// selection, plant, cut, paint, wrench, undo or load). What it did so
    /// far stays done, as one step of their undo; the Add-On's `on_copy`
    /// (or `on_place`) hears it with `error` `canceled` (`canceled` true).
    CancelCopy {
        player: u64,
    },
    /// What the copy `player` places turns about and is put against a
    /// clicked surface by: the whole copy (`whole`), else the brick it was
    /// taken from first (the clicked brick of a stack).
    PivotCopy {
        player: u64,
        whole: bool,
    },
    /// Plant `player`'s copies into another player's brick group: `target`
    /// names them (a player's name or part of it, or a BL_ID); empty plants
    /// into their own again. Each plant needs build trust with that group,
    /// or `admin` and an administrator. The package's `on_copy` hears the
    /// group chosen (`action` `"plant_as"`, `name`, or `error` `missing`
    /// or `trust`).
    PlantAs {
        player: u64,
        target: String,
        admin: bool,
    },
    /// Remove the bricks `player`'s copy was taken from, as their hammer
    /// would (their full trust), as one step Ctrl+Z puts back as it was:
    /// all or none, or with `each` every brick they may cut, the rest
    /// counted (`on_copy`, `action` `"cut"`, `refused`).
    CutCopy {
        player: u64,
        #[serde(default)]
        each: bool,
    },
    /// Paint the bricks `player`'s copy was taken from with `paint`, as
    /// their spray or FX can would, as one step Ctrl+Z takes back. With
    /// `each`, every brick they may paint is painted and the rest are
    /// counted (`on_copy`, `action` `"paint"`); else all or none.
    PaintCopy {
        player: u64,
        paint: FillPaint,
        each: bool,
    },
    /// Open `player`'s wrench on every brick their copy was taken from: the
    /// settings they tick apply to each brick they may change, as one step
    /// Ctrl+Z takes back (`on_copy`, `action` `"wrench"`).
    WrenchCopy {
        player: u64,
    },
    /// Remove every brick reaching into the box from `min` to `max` that
    /// `player` may hammer, and put plain bricks back over the parts that
    /// stuck out of it (v20's New Duplicator's supercut), as one step
    /// Ctrl+Z takes back (`on_copy`, `action` `"supercut"`). A copy job.
    SuperCut {
        player: u64,
        min: [f32; 3],
        max: [f32; 3],
    },
    /// Fill the empty room in the box from `min` to `max` with the fewest
    /// plain bricks of palette colour `color`, as `player`'s own, as one
    /// step Ctrl+Z takes back (`on_copy`, `action` `"fill"`). A copy job,
    /// stopping at the server's brick limit.
    FillBox {
        player: u64,
        min: [f32; 3],
        max: [f32; 3],
        color: u8,
    },
    /// Let the held image take `player`'s paint and FX cans (its
    /// `commands.paint`) instead of the can coming out, or stop.
    TakePaint {
        player: u64,
        take: bool,
    },
    /// Switch what `player`'s mouse wheel and number keys pick
    /// (`clientCmdSetScrollMode`), without changing what is in hand.
    ScrollMode {
        player: u64,
        mode: ScrollMode,
    },
    /// Paint `brick` and every brick of its colour joined to it as
    /// `player`'s spray cans would paint each one (their full trust; a fill
    /// flows around bricks it may not paint), as one step Ctrl+Z takes
    /// back. Bricks join through shared faces, or with `reach` through any
    /// overlap of a brick's box grown by `reach` (sideways, up and down),
    /// as v20's `containerBoxSearch` fills found them.
    PaintFill {
        player: u64,
        brick: u64,
        paint: FillPaint,
        limit: u32,
        reach: Option<[f32; 2]>,
        /// More than `limit` bricks: paint the first `limit` and stop, as
        /// v20 did, instead of refusing the fill.
        stop_at_limit: bool,
        /// Centre-printed, for these seconds, when the limit stops a fill.
        limit_message: Option<(String, f32)>,
        /// How long a refusal ("does not trust you enough") shows, seconds.
        refusal_seconds: Option<f32>,
        /// When the limit stops a fill, the player's plant-limit error
        /// (`MsgPlantError_Limit`) shows too.
        limit_error: bool,
    },
    /// Paint a vehicle as `player` (their full trust from its spawn
    /// brick's build, the minigame's paint rule), as one step Ctrl+Z takes
    /// back. Its riders take the colour for `riders_seconds`.
    PaintVehicle {
        player: u64,
        vehicle: u64,
        paint: VehiclePaint,
        riders_seconds: Option<f32>,
        /// How long a refusal ("does not trust you enough") shows, seconds.
        refusal_seconds: Option<f32>,
    },
    /// For `seconds`, a player looks different (`SetTempColor`,
    /// `setFaceName`, `setNodeColor`), then as they were.
    TempLook {
        player: u64,
        look: TempLook,
        seconds: f32,
    },
    /// Outline a box for one player while `tool` is in their hand (a
    /// selection, a zone being marked); `None` takes it away.
    ShowBox {
        player: u64,
        area: Option<([f32; 3], [f32; 3])>,
        tool: String,
    },
    /// Draw the set of boxes named `key` for every player (joiners too),
    /// replacing the set drawn under that key; none takes it away. A set
    /// with an `owner` goes when that player leaves.
    ShowShapes {
        owner: Option<u64>,
        key: String,
        shapes: Vec<WorldShape>,
    },
    /// Put an item in a player's tool list (unless they carry it) and,
    /// with `equip`, in their hand.
    GiveItem {
        player: u64,
        item: String,
        equip: bool,
    },
    /// Put a whole tool list in a living player's hands, slot by slot
    /// (`forceEquip`, a team's start tools): `None` empties a slot, slots
    /// past the list are emptied, and items the server lacks leave theirs
    /// empty. What they held is put away.
    SetTools {
        player: u64,
        tools: Vec<Option<String>>,
    },
    /// Take one `item` out of a player's tool list (`%obj.tool[%slot] =
    /// 0`): the held slot if it holds one, else the first that does. A held
    /// item is put away.
    TakeItem {
        player: u64,
        item: String,
    },
    /// Put an item of this package (or one it depends on) in the world as
    /// a pickup at `position`, moving at `velocity`, that pops after ten
    /// seconds like a dropped tool. `data` travels with it to `on_pickup`
    /// as `info.data`, as what `on_drop` keeps does (a dead player's
    /// ammo in the bag they leave).
    DropItem {
        item: String,
        position: [f32; 3],
        velocity: [f32; 3],
        /// A palette colour tinting it (a team's flag).
        #[serde(default)]
        paint: Option<u8>,
        /// Kept with it: `on_pickup` sees it as `info.data`, `drops()` too.
        #[serde(default)]
        data: Option<serde_json::Value>,
        /// Seconds until it pops (1 to [`MAX_DROP_SECONDS`]); `None` is
        /// v20's ten.
        #[serde(default)]
        seconds: Option<u32>,
    },
    /// Take back an item this package put in the world with `drop_item`.
    RemoveDrop {
        drop: u64,
    },
    /// Float `text` over an item this package put in the world, in palette
    /// colour `color` (`setShapeName` with `setShapeNameColor`: a dropped
    /// flag's countdown), or take it away with `None`.
    NameDrop {
        drop: u64,
        text: Option<String>,
        color: u8,
    },
    /// Change an object's velocity by `velocity` (units per second). `by`
    /// is the player credited when what it hits is hurt or broken.
    Push {
        target: ObjectRef,
        velocity: [f32; 3],
        by: Option<u64>,
    },
    /// Knock a player off their feet into a tumble, flying at `velocity`;
    /// for `seconds` (0.1 to 60) when given, else until it settles.
    Tumble {
        player: u64,
        velocity: [f32; 3],
        by: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seconds: Option<f32>,
    },
    /// Keep `target` floating `distance` ahead of `player`'s eye, where
    /// they look, until let go. The engine pulls it there every tick; heavy
    /// things lag. A player holds one thing at a time.
    ///
    /// `at` is the point on the object it is held by (world space, now),
    /// else its middle. `force` limits how hard it pulls (the engine's
    /// default otherwise). With `turn`, the object keeps the turn it had
    /// relative to the holder's heading, so it swings round with them.
    Hold {
        player: u64,
        target: ObjectRef,
        distance: f32,
        at: Option<[f32; 3]>,
        force: Option<f32>,
        turn: bool,
    },
    /// Carry what `player` holds `distance` from their eye from now on
    /// (reeling it in or out).
    HoldDistance {
        player: u64,
        distance: f32,
    },
    /// Let go of what `player` holds, and stop reaching.
    LetGo {
        player: u64,
    },
    /// Tie `player` to `anchor` with a rope `length` long (`None`: exactly
    /// as long as it spans now): they move freely within it and swing on
    /// it (the player motor's `Tether`). `brick`
    /// ties it to that brick, and the rope breaks when the brick goes;
    /// `object` ties it to that spot on a player, vehicle or entity, which
    /// carries the anchor along as it moves and turns, and the rope breaks
    /// when it goes. `reel` is how fast `TetherLength` changes it and
    /// `swing` how hard the movement keys push a hanging player (the
    /// engine's defaults otherwise). `keys` (`[shortest, longest]`) lets
    /// the player's jump and crouch keys reel it in and out between those.
    /// With `straight`, reeling in draws the player straight along it.
    /// A player has one rope; a new one replaces it.
    Tether {
        player: u64,
        anchor: [f32; 3],
        length: Option<f32>,
        brick: Option<u64>,
        reel: Option<f32>,
        swing: Option<f32>,
        #[serde(default)]
        object: Option<ObjectRef>,
        #[serde(default)]
        keys: Option<[f32; 2]>,
        #[serde(default)]
        straight: bool,
    },
    /// Reel `player`'s rope toward `length`.
    TetherLength {
        player: u64,
        length: f32,
    },
    /// Cut `player`'s rope. With `keep` (0 to 1), the player keeps only
    /// that fraction of their speed relative to what the rope was tied to,
    /// as a rope's grip slows them as it lets go.
    Untether {
        player: u64,
        #[serde(default)]
        keep: Option<f32>,
    },
    /// Keep reaching for something to hold: every tick, while `player`
    /// holds nothing, the engine looks where they look, up to `distance`,
    /// and holds the first thing it meets that they may move, by the spot
    /// it met, as far off as it was (at least `near`), as [`Op::Hold`]
    /// with `force` and `turn` would. Reaching ends once it holds
    /// something, on `let_go`, or when the player dies. The script sees
    /// the catch with `held` (a gun whose trigger stays down catches what
    /// comes in range, with no second click).
    Reach {
        player: u64,
        distance: f32,
        near: f32,
        force: Option<f32>,
        turn: bool,
    },
    /// Spawn a vehicle definition (`namespace:vehicle/name`) of this package
    /// or one it depends on, turned `yaw` radians and moving at `velocity`.
    /// `owner` is the player it belongs to (their trust and minigame rules
    /// apply), or the world.
    SpawnVehicle {
        definition: String,
        position: [f32; 3],
        yaw: f32,
        velocity: [f32; 3],
        owner: Option<u64>,
    },
    /// Remove a vehicle this package spawned.
    RemoveVehicle {
        vehicle: u64,
    },
    /// Launch a projectile of this package's weapons, or a dependency's,
    /// from `position` at `velocity`: a creature's gun, a trap, a fireball.
    /// With `by` it is that player's shot, hurting whom their shots may;
    /// without, the package's own, which hurts any living player.
    Fire {
        projectile: String,
        position: [f32; 3],
        velocity: [f32; 3],
        by: Option<u64>,
    },
    /// A projectile's explosion on a living player (`%obj.spawnExplosion`),
    /// `scale` times its size (0.1 to 10): an emote, a crit's burst. It
    /// hurts and pushes as the explosion would.
    SpawnExplosion {
        player: u64,
        projectile: String,
        scale: f32,
    },
    /// Give a living player health, up to their archetype's most.
    Heal {
        player: u64,
        amount: f32,
    },
    /// Text in the middle of the screen (`centerPrint`), or above the
    /// bottom edge (`bottomPrint`), for `seconds`: one player's, or
    /// everyone's when `player` is `None`. Empty text clears it.
    /// `hide_bar` hides the bottom print's bar (`bottomPrint`'s third
    /// argument).
    Print {
        player: Option<u64>,
        text: String,
        seconds: f32,
        bottom: bool,
        #[serde(default)]
        hide_bar: bool,
    },
    /// One player's plant-error icon and sound, as v20's
    /// `messageClient(%client, 'MsgPlantError_…')`: one of
    /// [`PLANT_ERRORS`]. `flood` (planting too soon) shows what the
    /// engine's own plant rate shows when a player plants too fast.
    PlantError {
        player: u64,
        error: String,
    },
    /// Tell one player something in a message box they close with OK
    /// (v20's `MessageBoxOK` from the server).
    MessageBox {
        player: u64,
        title: String,
        text: String,
    },
    /// Ask one player a yes or no question (v20's `MessageBoxYesNo` from
    /// the server): yes sends the package's own `command`, which takes no
    /// arguments, as if they had typed it; no does nothing.
    Ask {
        player: u64,
        title: String,
        text: String,
        command: String,
    },
    /// Show `player` a score report in its own window (Slayer's End of
    /// Round Report), with the columns Add-Ons changed for their game; or
    /// close it with `None`.
    ShowReport {
        player: u64,
        report: Option<Box<crate::report::Report>>,
    },
    /// Change a column of the reports a game's players are shown, from now
    /// on: retitle and fill it, add it, or take it out (`title: None`).
    /// Capture the Flag's Flag Pick-ups in place of Slayer's Kills.
    ReportColumn {
        game: u64,
        change: crate::report::ColumnChange,
    },
    /// Play a sound profile (an Add-On weapons pack's `sounds`, or v20's):
    /// at `position` for everyone near, or at one player's ears.
    Sound {
        profile: String,
        at: SoundAt,
    },
    /// A straight beam from `from` to `to` for `seconds`, fading out: a
    /// tracer, a laser, a bolt. With `muzzle`, clients start it at that
    /// player's held muzzle as they draw it. Presentation only.
    Beam {
        from: [f32; 3],
        to: [f32; 3],
        color: [f32; 4],
        width: f32,
        seconds: f32,
        muzzle: Option<u64>,
    },
    /// Play an animation on one of a player's four script threads
    /// (`playThread`): 0 and 1 the body, 2 the arms with what they hold, 3 a
    /// gesture; `root` stops it. `after` seconds later when above 0, as
    /// `%player.schedule(ms, "playThread", ...)` did.
    PlayThread {
        player: u64,
        thread: u8,
        sequence: String,
        after: f32,
    },
    /// Every map light within `radius` of `position` shines at `tint` times
    /// its recovered colour (0 switches it off, 1 is as the map was lit),
    /// for every player, until the map changes.
    SetMapLights {
        position: [f32; 3],
        radius: f32,
        tint: [f32; 3],
    },
    /// Change the live environment (sun, light, fog, sky, day/night) for
    /// every player until the map changes: `changes` sets what it sets,
    /// then each of `unset` (names from `bri_content::atmosphere::KEYS`)
    /// goes back to the map's own.
    SetEnvironment {
        changes: Box<bri_content::atmosphere::Settings>,
        unset: Vec<String>,
    },
    /// Set a player's field of view (`setControlCameraFov`), or hand it back
    /// to their own setting with `None`.
    SetFov {
        player: u64,
        fov: Option<f32>,
    },
    /// Move a player's body at this share of its running, crouching and
    /// swimming speeds (0 to 4; 1 is its archetype's own) until changed or
    /// they respawn.
    SetSpeedScale {
        player: u64,
        scale: f32,
    },
    /// Add rounds of `ammo` to a player's reserve (an ammo box), up to the
    /// most its magazines carry.
    GiveAmmo {
        player: u64,
        ammo: String,
        rounds: u64,
    },
    /// Set a player's reserve of `ammo`; `None` never runs out.
    SetReserve {
        player: u64,
        ammo: String,
        rounds: Option<u64>,
    },
    /// Set the rounds in a player's magazine of `item`, up to its size.
    SetRounds {
        player: u64,
        item: String,
        rounds: u64,
    },
    /// Start reloading the gun in a player's hand, as the light key does.
    Reload {
        player: u64,
    },
    /// Whether the image in a player's hand has ammo (`setImageAmmo`), which
    /// its states' `ammo` transitions read.
    SetImageAmmo {
        player: u64,
        ammo: bool,
    },
    /// Whether the image in a player's hand is loaded (`setImageLoaded`),
    /// which its states' `loaded` and `not_loaded` transitions read: a tool
    /// that spins while it works.
    SetImageLoaded {
        player: u64,
        loaded: bool,
    },
    /// Put another image in a player's hand, keeping their tool slot
    /// (`mountImage`): a scope, a second fire mode. `None` puts back the
    /// selected tool's own image.
    MountImage {
        player: u64,
        image: Option<String>,
    },
    /// Mount an image on a player's body in the emote slot
    /// (`%player.emote(%image)`), replacing the emote, pain or flames there:
    /// every client plays it, and its states run their commands for the
    /// wearer (a heal over time). `None` empties the slot.
    Emote {
        player: u64,
        image: Option<String>,
        /// `%skipSpam`: without it an image counts toward the player's
        /// emote spam check (more than five quick emotes are dropped), as
        /// the stock emotes do.
        #[serde(default)]
        skip_spam: bool,
    },
    /// Put an image in a worn slot (2 or 3) of a player, tinted with a
    /// palette colour (`mountImage(%image, 3)`: a flag on the back), or
    /// take it off with `None`. With `keep`, no other package replaces or
    /// takes off that image while it is worn (Slayer CTF's
    /// `Player::mountImage` and `unMountImage` overrides, which guard the
    /// flag).
    WearImage {
        player: u64,
        slot: u8,
        image: Option<String>,
        paint: Option<u8>,
        #[serde(default)]
        keep: bool,
    },
    /// Set a mini-game's teams and team rules, as Slayer's team list does:
    /// a team with an `id` keeps it and its members, one without is new,
    /// and teams left out are removed (their members are left on none).
    SetTeams {
        game: u64,
        teams: Vec<TeamOp>,
        friendly_fire: bool,
        ally_same_color: bool,
    },
    /// Put a member of a mini-game on one of its teams, or on none.
    SetTeam {
        player: u64,
        team: Option<u64>,
    },
    /// Set a player's mini-game score, or with `add` change it by `value`
    /// (`incScore`).
    SetScore {
        player: u64,
        value: i64,
        add: bool,
    },
    /// Reset a mini-game (`MiniGameSO::reset`): every member respawns with
    /// a score of 0 and the game's bricks come back.
    ResetMinigame {
        game: u64,
    },
    /// End a mini-game's round (Slayer's `endRound`), won by these teams
    /// and players, or by nobody. Every rule hears `on_minigame` with
    /// `kind == "round_end"`; the round stays over until a reset.
    EndRound {
        game: u64,
        teams: Vec<u64>,
        players: Vec<u64>,
    },
    /// Change an Add-On setting of a mini-game, or of one of its teams
    /// (`Slayer_MiniGameSO::setPref`): `key` is the package's own or
    /// `namespace:key`; `None` puts it back to its default.
    SetSetting {
        game: u64,
        team: Option<u64>,
        key: String,
        value: Option<SettingValue>,
    },
    /// The item a brick holds out to be picked up (`setItem`): an item of
    /// this package, a dependency's or v20's, or `None` for none.
    SetBrickItem {
        brick: u64,
        item: Option<String>,
    },
    /// Repaint a brick in palette colour `color` (`fxDTSBrick::setColor`
    /// from a game's script: a capture point taking its holder's colour).
    /// Nothing to undo; the brick must be the world's, a mini-game's or one
    /// the calling player has full trust on.
    SetBrickColor {
        brick: u64,
        color: u8,
    },
    /// Keep `value` on a brick as this package's `key`, or clear it with
    /// `None`: a v20 script's dynamic field on a brick (Slayer's
    /// `isLocked[color]`). Every package reads it with `brick_field`; it
    /// goes with the brick. Keys are 1 to [`MAX_BRICK_FIELD_KEY`] letters,
    /// digits or `_`.
    SetBrickField {
        brick: u64,
        key: String,
        value: Option<serde_json::Value>,
    },
    /// How often one of this package's zones (`behaviour.zones`, by index)
    /// is checked from now on, 10 to 10000 ms, as a script setting
    /// `TriggerData.tickPeriodMS` did (Slayer's capture point Tick Time).
    SetZonePeriod {
        zone: u32,
        period_ms: u32,
    },
    /// Fire one of this package's wrench event inputs on a brick
    /// (`processInputEvent`): the rows its builder wired to it run, as
    /// theirs. `player` fills the Player, Client and MiniGame targets.
    FireBrickInput {
        brick: u64,
        input: String,
        player: Option<u64>,
    },
    /// Fire one of this package's inputs on every brick of mini-game `game`
    /// wired to it (Slayer's `processMultiSourceInputEvent`:
    /// `onMinigameDeath`, `onMinigameRoundStart`). `player` fills the
    /// Player and Client targets, `killer` the `Player(Killer)` and
    /// `Client(Killer)` ones, and the game the MiniGame target.
    FireGameInput {
        game: u64,
        input: String,
        player: Option<u64>,
        killer: Option<u64>,
    },
    /// Empty a player's hand (`unMountImage(0)`): the tool they held is put
    /// away, still in its slot.
    UnmountImage {
        player: u64,
    },
    /// Seat player `rider` on player `mount`'s mount point `node`
    /// (`%mount.mountObject(%rider, %node)`; a Blockhead's `Mount<node>`):
    /// carried with it and drawn on that node as it animates. With
    /// `can_dismount` false the rider cannot get off by jumping
    /// (`canDismount = 0`). Riders a rule seats stay on through the mount
    /// changing body while the new one has the node. `turn` (radians,
    /// clockwise seen from above) turns the rider's body on the mount
    /// point, as a `setTransform` on a mounted player sets its `mRot.z`.
    MountObject {
        mount: u64,
        rider: u64,
        node: u8,
        can_dismount: bool,
        turn: f32,
    },
    /// Take `rider` off the player they ride, where they are, moving as
    /// the mount moved (`unMountObject`).
    UnmountObject {
        rider: u64,
    },
    /// A player's body scale (`setScale`, `setPlayerScale`); a new body
    /// is full size again.
    SetScale {
        player: u64,
        scale: f32,
    },
    /// Bound how far a player's arms and head follow their look
    /// (`setLookLimits(%up, %down)`), as `[down, up]` positions from 0
    /// (all the way up) to 1, or `None` for the whole range. A new body
    /// looks freely again.
    SetLookLimits {
        player: u64,
        limits: Option<[f32; 2]>,
    },
    /// Keep a mini-game member from respawning until their mini-game resets
    /// or a rule lets them (Slayer's `setDead`: out of lives, between
    /// rounds). The client hides its respawn prompt while held.
    HoldRespawn {
        player: u64,
        held: bool,
    },
    /// How long a mini-game member waits to respawn after dying, in ms,
    /// in place of their mini-game's time (`setRespawnTime`; Slayer's team
    /// Respawn Time), or `None` for the mini-game's again. Kept until they
    /// leave the mini-game.
    SetRespawnTime {
        player: u64,
        ms: Option<u32>,
    },
    /// Put a bot in a mini-game (Slayer's `addBotToGame`): a player body
    /// without a connection, of a bot `kind` an enabled Add-On provides
    /// (`bot_kinds()`), playing with the engine's bot brain. It joins
    /// `game`, and `team` when given, and spawns as a member does; the
    /// rules hear it join as any member. It is this package's until
    /// `RemoveBot` or its game ends, and counts toward the server's bot
    /// limit.
    AddBot {
        game: u64,
        team: Option<u64>,
        kind: String,
        name: String,
    },
    /// Take away a bot this package added.
    RemoveBot {
        bot: u64,
    },
    /// Stop or restart the brain of a bot this package added (Slayer's
    /// `stopHoleLoop` and `resetHoleLoop`): a resting bot stands still and
    /// holds its fire.
    RestBot {
        bot: u64,
        rest: bool,
    },
    /// Put a tool slot in the hand of a bot this package added, or put its
    /// tools away with `None` (`AiPlayer::setWeapon`, Slayer's
    /// `useRandomTool`). Its brain fights with what it holds, arming
    /// itself only when its hand holds no weapon.
    BotTool {
        bot: u64,
        slot: Option<u8>,
    },
    /// Fly a player's camera along knots (`setControlObject(pathCamera)`),
    /// their body standing still, or with `None` hand control back. The
    /// package's `on_path_node` hears each knot reached.
    FollowPath {
        player: u64,
        knots: Option<Vec<PathKnot>>,
    },
    /// Give a player a free camera from where their camera is
    /// (`Camera::setMode("Observer")`), or an orbit around a point
    /// (`setOrbitPointMode`); a frozen [`Op::OrbitCamera`]'s `None` hands
    /// control back.
    Camera {
        player: u64,
        camera: CameraOp,
    },
    /// Give a player an orbit camera around a player's body (v20's
    /// `setOrbitMode` and `setControlObject(camera)`), or (`None`) their
    /// body back from the kind of camera `body` names. One seam for both
    /// of v20's uses: Throwing's held player, whose click still acts, and
    /// a rule's `watch`, whose body freezes and whose keys go to the rules.
    OrbitCamera {
        player: u64,
        body: OrbitBody,
        orbit: Option<Orbit>,
    },
}
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
/// What a player's mouse wheel picks ([`Op::ScrollMode`]).
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
impl Op {
    pub fn capability(&self) -> &'static str {
        match self {
            Self::RemoveBrick { .. }
            | Self::PlaceBrick { .. }
            | Self::PlantBrick { .. }
            | Self::PlaceVoxel { .. }
            | Self::SetBlockState { .. }
            | Self::CutCopy { .. }
            | Self::PaintCopy { .. }
            | Self::WrenchCopy { .. }
            | Self::SuperCut { .. }
            | Self::FillBox { .. }
            | Self::PaintFill { .. }
            | Self::PaintVehicle { .. } => "world.edit",
            Self::TempLook { .. } => "player",
            Self::Explode { .. }
            | Self::Damage { .. }
            | Self::Heal { .. }
            | Self::Fire { .. }
            | Self::SpawnExplosion { .. } => {
                "damage"
            }
            Self::SpawnEntity { .. }
            | Self::RemoveEntity { .. }
            | Self::Steer { .. }
            | Self::Label { .. } => "entity",
            Self::Tell { .. }
            | Self::Broadcast { .. }
            | Self::Print { .. }
            | Self::PlantError { .. }
            | Self::ShowReport { .. }
            | Self::TellMinigame { .. }
            | Self::PrintMinigame { .. }
            | Self::Ask { .. }
            | Self::MessageBox { .. } => "chat",
            Self::Sound { .. }
            | Self::Beam { .. }
            | Self::PlayThread { .. }
            | Self::ShowBox { .. }
            | Self::ShowShapes { .. } => "effects",
            Self::CopyBuild { .. }
            | Self::CopyBox { .. }
            | Self::SaveCopy { .. }
            | Self::LoadCopy { .. }
            | Self::MirrorCopy { .. }
            | Self::MirrorGhost { .. }
            | Self::MoveCopy { .. }
            | Self::DropCopy { .. }
            | Self::ShowCopy { .. }
            | Self::HideCopy { .. }
            | Self::ShiftCopy { .. }
            | Self::RotateCopy { .. }
            | Self::PlantCopy { .. }
            | Self::FloatCopy { .. }
            | Self::PlantWait { .. }
            | Self::CancelCopy { .. }
            | Self::PivotCopy { .. }
            | Self::PlantAs { .. }
            | Self::ListCopies { .. }
            | Self::TakePaint { .. }
            | Self::HighlightCopy { .. } => "build",
            Self::SetMapLights { .. } => "lighting",
            Self::SetTeams { .. }
            | Self::SetTeam { .. }
            | Self::SetScore { .. }
            | Self::ResetMinigame { .. }
            | Self::HoldRespawn { .. }
            | Self::SetRespawnTime { .. }
            | Self::EndRound { .. }
            | Self::SetSetting { .. }
            | Self::SetZonePeriod { .. }
            | Self::ReportColumn { .. } => "minigame",
            Self::AddBot { .. }
            | Self::RemoveBot { .. }
            | Self::RestBot { .. }
            | Self::BotTool { .. } => "bots",
            Self::SetBrickItem { .. } | Self::SetBrickColor { .. } => "world.edit",
            Self::FireBrickInput { .. }
            | Self::FireGameInput { .. }
            | Self::SetBrickField { .. } => "brick_events",
            Self::SetEnvironment { .. } => "environment",
            Self::Teleport { .. }
            | Self::Respawn { .. }
            | Self::SetArchetype { .. }
            | Self::PushArchetype { .. }
            | Self::PopArchetype { .. }
            | Self::Control { .. }
            | Self::GiveItem { .. }
            | Self::SetTools { .. }
            | Self::TakeItem { .. }
            | Self::DropItem { .. }
            | Self::RemoveDrop { .. }
            | Self::NameDrop { .. }
            | Self::WearImage { .. }
            | Self::SetFov { .. }
            | Self::SetSpeedScale { .. }
            | Self::GiveAmmo { .. }
            | Self::SetReserve { .. }
            | Self::SetRounds { .. }
            | Self::Reload { .. }
            | Self::SetImageAmmo { .. }
            | Self::SetImageLoaded { .. }
            | Self::MountImage { .. }
            | Self::Emote { .. }
            | Self::UnmountImage { .. }
            | Self::SetScale { .. }
            | Self::SetLookLimits { .. }
            | Self::FollowPath { .. }
            | Self::Camera { .. }
            | Self::OrbitCamera { .. }
            | Self::SetAvatarColors { .. }
            | Self::SetAvatarParts { .. }
            | Self::ScrollMode { .. } => "player",
            Self::MountObject { .. } | Self::UnmountObject { .. } => "physics",
            Self::Push { .. }
            | Self::Tumble { .. }
            | Self::Hold { .. }
            | Self::HoldDistance { .. }
            | Self::LetGo { .. }
            | Self::Tether { .. }
            | Self::TetherLength { .. }
            | Self::Untether { .. }
            | Self::Reach { .. }
            | Self::SpawnVehicle { .. }
            | Self::RemoveVehicle { .. } => "physics",
        }
    }
    /// Shape limits, independent of who asks.
    fn bounded(&self) -> Result<(), String> {
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite() && x.abs() <= 1_000_000.0);
        let chat =
            |t: &str| !t.trim().is_empty() && t.len() <= 256 && !t.chars().any(char::is_control);
        let item = |t: &str| bri_package::id::is_content_ref(t, Some("weapon"));
        // A magazine's ammo type: 1 to 32 letters, digits, `.`, `_` or `-`.
        let ammo_name = |t: &str| {
            (1..=32).contains(&t.len())
                && t.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        };
        // A box from `min` to `max`, each side at most `MAX_BOX_SPAN`.
        let span = |min: &[f32; 3], max: &[f32; 3]| {
            finite(min)
                && finite(max)
                && (0..3).all(|a| max[a] >= min[a] && max[a] - min[a] <= MAX_BOX_SPAN)
        };
        let ok = match self {
            Self::RemoveBrick { .. }
            | Self::RemoveEntity { .. }
            | Self::Respawn { .. }
            | Self::Control { .. }
            | Self::SetImageAmmo { .. }
            | Self::SetImageLoaded { .. }
            | Self::MirrorCopy { .. }
            | Self::DropCopy { .. }
            | Self::ShowCopy { .. }
            | Self::HideCopy { .. }
            | Self::PlantCopy { .. }
            | Self::FloatCopy { .. }
            | Self::PivotCopy { .. }
            | Self::CancelCopy { .. }
            | Self::TakePaint { .. }
            | Self::ScrollMode { .. }
            | Self::CutCopy { .. }
            | Self::WrenchCopy { .. }
            | Self::UnmountImage { .. }
            | Self::HoldRespawn { .. }
            | Self::RemoveBot { .. }
            | Self::RestBot { .. }
            | Self::UnmountObject { .. } => true,
            Self::MirrorGhost { asymmetric, .. } => chat(asymmetric),
            Self::BotTool { slot, .. } => slot.is_none_or(|s| usize::from(s) < MAX_TOOL_SLOTS),
            Self::AddBot { kind, name, .. } => {
                !kind.is_empty()
                    && kind.len() <= 96
                    && kind.chars().all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c))
                    && !name.trim().is_empty()
                    && name.chars().count() <= MAX_BOT_NAME_CHARS
                    && !name.chars().any(char::is_control)
            }
            Self::MountObject {
                mount,
                rider,
                node,
                turn,
                ..
            } => mount != rider && usize::from(*node) < MAX_MOUNT_POINTS && turn.is_finite(),
            Self::SetScale { scale, .. } => scale.is_finite() && SCALE_RANGE.contains(scale),
            Self::OrbitCamera {
                player,
                body,
                orbit,
            } => orbit
                .is_none_or(|o| o.valid() && (o.target != *player || *body == OrbitBody::Frozen)),
            Self::SetTools { tools, .. } => {
                tools.len() <= MAX_TOOL_SLOTS
                    && tools
                        .iter()
                        .flatten()
                        .all(|id| !id.is_empty() && id.len() <= 160 && id.is_ascii())
            }
            Self::SetRespawnTime { ms, .. } => ms.is_none_or(|ms| ms <= MAX_RESPAWN_MS),
            Self::SetLookLimits { limits, .. } => limits
                .is_none_or(|[down, up]| (0.0..=1.0).contains(&down) && (0.0..=1.0).contains(&up)),
            Self::PaintFill {
                paint,
                limit,
                reach,
                limit_message,
                refusal_seconds,
                ..
            } => {
                (1..=MAX_FILL_BRICKS as u32).contains(limit)
                    && refusal_seconds.is_none_or(|s| (0.0..=30.0).contains(&s))
                    && paint.valid()
                    && reach.is_none_or(|r| r.iter().all(|v| (0.0..=MAX_FILL_REACH).contains(v)))
                    && limit_message.as_ref().is_none_or(|(text, seconds)| {
                        text.chars().count() <= MAX_PRINT_CHARS && (0.0..=30.0).contains(seconds)
                    })
            }
            Self::PaintVehicle {
                paint,
                riders_seconds,
                refusal_seconds,
                ..
            } => {
                refusal_seconds.is_none_or(|s| (0.0..=30.0).contains(&s))
                    && riders_seconds.is_none_or(|s| (0.0..=MAX_TEMP_LOOK_SECONDS).contains(&s))
                    && match paint {
                        VehiclePaint::Color(_) => true,
                        VehiclePaint::Rgb(c) => c.iter().all(|v| (0.0..=1.0).contains(v)),
                    }
            }
            Self::TempLook { look, seconds, .. } => {
                (0.0..=MAX_TEMP_LOOK_SECONDS).contains(seconds)
                    && look
                        .color
                        .is_none_or(|c| c.iter().all(|v| (0.0..=1.0).contains(v)))
                    && look.face.as_ref().is_none_or(|f| {
                        !f.is_empty() && f.len() <= 64 && f.chars().all(|c| c.is_ascii_graphic())
                    })
                    && look.alpha.len() <= AVATAR_SLOTS.len()
                    && look.alpha.iter().all(|(slot, a)| {
                        AVATAR_SLOTS.contains(&slot.as_str()) && (0.0..=1.0).contains(a)
                    })
            }
            Self::PaintCopy { paint, .. } => paint.valid(),
            Self::ShiftCopy { offset, .. } => {
                (-1..=1).contains(&offset[0])
                    && (-1..=1).contains(&offset[1])
                    && (-3..=3).contains(&offset[2])
            }
            Self::RotateCopy { direction, .. } => matches!(direction, -1 | 1),
            Self::SuperCut { min, max, .. } | Self::FillBox { min, max, .. } => span(min, max),
            Self::Teleport { position, .. } => finite(position),
            Self::PlantBrick {
                kind,
                position,
                turns,
                ..
            } => {
                (bri_package::id::is_content_ref(kind, Some("brick"))
                    || kind
                        .strip_prefix("v20/brick/")
                        .is_some_and(|n| !n.is_empty() && n.len() <= 128))
                    && finite(position)
                    && *turns < 4
            }
            Self::PlaceVoxel { position, material } => {
                position.iter().all(|c| c.abs() <= 1_000_000)
                    && bri_package::id::ContentId::parse(material).is_ok()
            }
            Self::SetAvatarColors { colors, .. } => {
                colors.len() <= AVATAR_SLOTS.len()
                    && colors.iter().all(|(slot, c)| {
                        AVATAR_SLOTS.contains(&slot.as_str())
                            && c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                    })
            }
            Self::SetAvatarParts {
                parts, face, decal, ..
            } => {
                let name = |n: &str| !n.is_empty() && n.len() <= 64 && n.is_ascii();
                parts.len() <= AVATAR_PARTS.len()
                    && parts
                        .iter()
                        .all(|(slot, part)| AVATAR_PARTS.contains(&slot.as_str()) && name(part))
                    && face.iter().chain(decal).all(|n| n.len() <= 256 && n.is_ascii())
            }
            Self::SetBlockState { state, .. } => {
                state.len() <= 64 && !state.chars().any(char::is_control)
            }
            Self::SetArchetype { archetype, .. } => {
                archetype.len() <= 160 && !archetype.chars().any(char::is_control)
            }
            Self::PushArchetype { archetype, .. } | Self::PopArchetype { archetype, .. } => {
                !archetype.is_empty()
                    && archetype.len() <= 160
                    && !archetype.chars().any(char::is_control)
            }
            Self::PlaceBrick {
                shape,
                position,
                color,
            } => {
                !shape.is_empty()
                    && shape.len() <= 128
                    && finite(position)
                    && color.iter().all(|c| (0.0..=1.0).contains(c))
            }
            Self::Explode {
                position,
                radius,
                damage,
                brick_radius,
                explosion,
            } => {
                explosion.as_deref().is_none_or(|e| {
                    (1..=64).contains(&e.len())
                        && e.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                }) && finite(position)
                    && (0.0..=32.0).contains(radius)
                    && (0.0..=1000.0).contains(damage)
                    && (0.0..=16.0).contains(brick_radius)
            }
            Self::Damage {
                amount,
                damage_type,
                ..
            } => {
                amount.is_finite()
                    && (0.0..=1000.0).contains(amount)
                    && damage_type.as_deref().is_none_or(|t| {
                        !t.is_empty() && t.len() <= 64 && !t.chars().any(char::is_control)
                    })
            }
            Self::Beam {
                from,
                to,
                color,
                width,
                seconds,
                ..
            } => {
                let span = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
                finite(from)
                    && finite(to)
                    && glam_length(&span) <= MAX_RAY_RANGE
                    && color.iter().all(|c| (0.0..=1.0).contains(c))
                    && width.is_finite()
                    && *width > 0.0
                    && *width <= MAX_BEAM_WIDTH
                    && seconds.is_finite()
                    && *seconds > 0.0
                    && *seconds <= MAX_BEAM_SECONDS
            }
            Self::PlayThread {
                thread,
                sequence,
                after,
                ..
            } => {
                *thread <= 3
                    && after.is_finite()
                    && (0.0..=MAX_THREAD_DELAY).contains(after)
                    && !sequence.is_empty()
                    && sequence.len() <= 64
                    && sequence
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }
            Self::SetFov { fov, .. } => fov.is_none_or(|f| FOV_RANGE.contains(&f)),
            Self::SetSpeedScale { scale, .. } => {
                scale.is_finite() && (0.0..=MAX_SPEED_SCALE).contains(scale)
            }
            Self::GiveAmmo { ammo, rounds, .. } => {
                ammo_name(ammo) && (1..=MAX_AMMO_ROUNDS).contains(rounds)
            }
            Self::SetReserve { ammo, rounds, .. } => {
                ammo_name(ammo) && rounds.is_none_or(|r| r <= MAX_AMMO_ROUNDS)
            }
            Self::SetRounds {
                item: id, rounds, ..
            } => item(id) && *rounds <= MAX_AMMO_ROUNDS,
            Self::Reload { .. } => true,
            Self::SetMapLights {
                position,
                radius,
                tint,
            } => {
                finite(position)
                    && radius.is_finite()
                    && (0.0..=MAX_LIGHT_RADIUS).contains(radius)
                    && tint.iter().all(|t| t.is_finite() && (0.0..=MAX_LIGHT_TINT).contains(t))
            }
            Self::SetEnvironment { changes, unset } => {
                changes.validate().is_ok()
                    && unset.len() <= bri_content::atmosphere::KEYS.len()
                    && unset
                        .iter()
                        .all(|k| bri_content::atmosphere::KEYS.contains(&k.as_str()))
            }
            Self::MountImage { image, .. } | Self::Emote { image, .. } => image
                .as_deref()
                .is_none_or(|i| bri_package::id::is_content_ref(i, Some("image"))),
            Self::WearImage { slot, image, .. } => {
                (2..=3).contains(slot)
                    && image
                        .as_deref()
                        .is_none_or(|i| bri_package::id::is_content_ref(i, Some("image")))
            }
            Self::RemoveDrop { .. } => true,
            Self::SpawnEntity {
                kind,
                position,
                vars,
            } => {
                kind.len() <= 128
                    && finite(position)
                    && vars.len() <= MAX_SPAWN_VARS
                    && vars.values().all(|v| crate::state::check_value(v).is_ok())
            }
            Self::Steer { direction, .. } => finite(direction),
            Self::Label { label, .. } => label.len() <= 32 && !label.chars().any(char::is_control),
            Self::Tell { text, .. }
            | Self::Broadcast { text }
            | Self::TellMinigame { text, .. } => chat(text),
            Self::CopyBuild { limit, tool, .. } => (1..=MAX_COPY_BRICKS).contains(limit) && item(tool),
            Self::CopyBox {
                min,
                max,
                limit,
                tool,
                ..
            } => (1..=MAX_COPY_BRICKS).contains(limit) && item(tool) && span(min, max),
            Self::SaveCopy { name, .. } => copy_name(name).as_deref() == Some(name.as_str()),
            Self::ListCopies { filter, .. } => {
                filter.is_empty() || copy_name(filter).as_deref() == Some(filter.as_str())
            }
            Self::PlantWait { seconds, .. } => (0.0..=60.0).contains(seconds),
            Self::PlantAs { target, .. } => {
                target.len() <= 64 && !target.chars().any(char::is_control)
            }
            Self::MoveCopy { point, normal, .. } => {
                finite(point) && point.iter().all(|v| v.abs() <= 1_000_000.0) && finite(normal)
            }
            Self::LoadCopy {
                name, limit, tool, ..
            } => {
                copy_name(name).as_deref() == Some(name.as_str())
                    && (1..=MAX_COPY_BRICKS).contains(limit)
                    && item(tool)
            }
            Self::HighlightCopy { color, seconds, .. } => {
                color
                    .iter()
                    .flatten()
                    .all(|c| (0.0..=1.0).contains(c))
                    && *seconds <= 60.0
            }
            Self::ShowBox { area, tool, .. } => match area {
                Some((min, max)) => item(tool) && span(min, max),
                None => tool.is_empty(),
            },
            Self::GiveItem { item: id, .. } | Self::TakeItem { item: id, .. } => item(id),
            Self::DropItem {
                item: id,
                position,
                velocity,
                data,
                seconds,
                ..
            } => {
                seconds.is_none_or(|s| (1..=MAX_DROP_SECONDS).contains(&s))
                    && data.as_ref().is_none_or(|d| {
                        serde_json::to_vec(d).is_ok_and(|b| b.len() <= MAX_DROP_DATA_BYTES)
                    })
                    && item(id)
                    && finite(position)
                    && finite(velocity)
                    && glam_length(velocity) <= MAX_PUSH_SPEED
                    && data
                        .as_ref()
                        .is_none_or(|d| crate::state::check_value(d).is_ok())
            }
            Self::Push { velocity, .. } => finite(velocity) && glam_length(velocity) <= MAX_PUSH_SPEED,
            Self::Tumble {
                velocity, seconds, ..
            } => {
                finite(velocity)
                    && glam_length(velocity) <= MAX_PUSH_SPEED
                    && seconds.is_none_or(|s| (0.1..=60.0).contains(&s))
            }
            Self::Hold {
                distance,
                at,
                force,
                ..
            } => {
                distance.is_finite()
                    && (0.5..=MAX_HOLD_DISTANCE).contains(distance)
                    && at.as_ref().is_none_or(|a| finite(a))
                    && force.is_none_or(|f| f.is_finite() && f > 0.0 && f <= MAX_HOLD_FORCE)
            }
            Self::HoldDistance { distance, .. } => {
                distance.is_finite() && (0.5..=MAX_HOLD_DISTANCE).contains(distance)
            }
            Self::Tether {
                anchor,
                length,
                brick,
                reel,
                swing,
                object,
                keys,
                ..
            } => {
                let span = MIN_TETHER_LENGTH..=MAX_TETHER_LENGTH;
                finite(anchor)
                    && length.is_none_or(|l| span.contains(&l))
                    && reel.is_none_or(|r| (0.0..=MAX_TETHER_REEL).contains(&r))
                    && swing.is_none_or(|s| (0.0..=MAX_TETHER_SWING).contains(&s))
                    && !(brick.is_some() && object.is_some())
                    && keys.is_none_or(|[short, long]| {
                        span.contains(&short) && span.contains(&long) && short <= long
                    })
            }
            Self::TetherLength { length, .. } => {
                (MIN_TETHER_LENGTH..=MAX_TETHER_LENGTH).contains(length)
            }
            Self::Reach {
                distance,
                near,
                force,
                ..
            } => {
                distance.is_finite()
                    && near.is_finite()
                    && (0.5..=MAX_HOLD_DISTANCE).contains(near)
                    && (*near..=MAX_HOLD_DISTANCE).contains(distance)
                    && force.is_none_or(|f| f.is_finite() && f > 0.0 && f <= MAX_HOLD_FORCE)
            }
            Self::LetGo { .. } | Self::Untether { .. } | Self::RemoveVehicle { .. } => true,
            Self::SetTeams { teams, .. } => {
                teams.len() <= MAX_TEAMS
                    && teams.iter().all(|t| {
                        !t.name.trim().is_empty()
                            && t.name.chars().count() <= MAX_TEAM_NAME
                            && !t.name.chars().any(char::is_control)
                    })
            }
            Self::SetTeam { .. } | Self::ResetMinigame { .. } => true,
            Self::EndRound { teams, players, .. } => {
                teams.len() <= MAX_TEAMS && players.len() <= MAX_ROUND_WINNERS
            }
            Self::SetSetting { key, value, .. } => {
                bri_package::setting::is_setting_ref(key)
                    && !matches!(value, Some(SettingValue::Text(t)) if t.len() > bri_package::setting::MAX_TEXT)
            }
            Self::SetScore { value, .. } => value.abs() <= MAX_SCORE,
            Self::SetBrickItem { item: id, .. } => id.as_deref().is_none_or(item),
            Self::SetBrickColor { .. } => true,
            Self::SetBrickField { key, .. } => is_brick_field_key(key),
            Self::NameDrop { text, .. } => text.as_deref().is_none_or(|t| {
                t.chars().count() <= bri_weapons::MAX_DROP_NAME && !t.chars().any(char::is_control)
            }),
            Self::FollowPath { knots, .. } => knots.as_ref().is_none_or(|k| {
                (1..=MAX_PATH_KNOTS).contains(&k.len())
                    && k.iter().all(|k| {
                        finite(&k.at)
                            && k.yaw.is_finite()
                            && k.pitch.is_finite()
                            && k.pitch.abs() <= std::f32::consts::FRAC_PI_2
                            && (0.1..=1000.0).contains(&k.speed)
                    })
            }),
            Self::Camera { camera, .. } => match camera {
                CameraOp::Free => true,
                CameraOp::Point { at, distance } => {
                    finite(at) && ORBIT_DISTANCES.contains(distance)
                }
            },
            Self::SetZonePeriod { zone, period_ms } => {
                (*zone as usize) < crate::content::MAX_ZONES && (10..=10_000).contains(period_ms)
            }
            Self::FireBrickInput { input, .. } | Self::FireGameInput { input, .. } => {
                input.len() <= 64
            }
            Self::Fire {
                projectile,
                position,
                velocity,
                ..
            } => {
                bri_package::id::is_content_ref(projectile, Some("projectile"))
                    && finite(position)
                    && finite(velocity)
                    && glam_length(velocity) <= MAX_FIRE_SPEED
            }
            Self::SpawnExplosion {
                projectile, scale, ..
            } => {
                bri_package::id::is_content_ref(projectile, Some("projectile"))
                    && (0.1..=10.0).contains(scale)
            }
            Self::Heal { amount, .. } => amount.is_finite() && (0.0..=100_000.0).contains(amount),
            Self::MessageBox { title, text, .. } => {
                title.chars().count() <= 64
                    && text.chars().count() <= MAX_PRINT_CHARS
                    && ![title, text].iter().any(|t| t.chars().any(|c| c.is_control() && c != '\n'))
            }
            Self::Ask {
                title,
                text,
                command,
                ..
            } => {
                title.chars().count() <= 64
                    && text.chars().count() <= MAX_PRINT_CHARS
                    && ![title, text].iter().any(|t| t.chars().any(|c| c.is_control() && c != '\n'))
                    && !command.is_empty()
                    && command.len() <= 64
                    && command.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }
            Self::Print { text, seconds, .. } | Self::PrintMinigame { text, seconds, .. } => {
                text.chars().count() <= MAX_PRINT_CHARS
                    && !text.chars().any(|c| c.is_control() && c != '\n')
                    && seconds.is_finite()
                    && (0.0..=600.0).contains(seconds)
            }
            Self::PlantError { error, .. } => PLANT_ERRORS.contains(&error.as_str()),
            Self::ShowShapes { key, shapes, .. } => {
                shape_key(key) && shapes.len() <= MAX_SHAPES && shapes.iter().all(WorldShape::check)
            }
            Self::ShowReport { report, .. } => report.as_ref().is_none_or(|r| r.is_bounded()),
            Self::ReportColumn { change, .. } => change.is_bounded(),
            Self::Sound { profile, at } => {
                !profile.is_empty()
                    && profile.len() <= 128
                    && !profile.chars().any(char::is_control)
                    && match at {
                        SoundAt::Position(p) => finite(p),
                        SoundAt::Player(_) => true,
                    }
            }
            Self::SpawnVehicle {
                definition,
                position,
                yaw,
                velocity,
                ..
            } => {
                bri_package::id::is_content_ref(definition, Some("vehicle"))
                    && finite(position)
                    && yaw.is_finite()
                    && finite(velocity)
                    && glam_length(velocity) <= MAX_PUSH_SPEED
            }
        };
        if ok {
            Ok(())
        } else {
            Err(format!("{self:?} is outside the operation's limits"))
        }
    }
}

fn glam_length(v: &[f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

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
            format!("{} needs capability `{needed}`, which the package does not declare", op_name(op)),
        )
        .at(package)
        .hint(format!("add \"{needed}\" to capabilities in package.json; the server owner sees it when enabling the package")));
    }
    if let Op::SpawnEntity { kind, .. } = op
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
    match op {
        Op::RemoveBrick { .. } => "remove_brick",
        Op::PlaceBrick { .. } => "place_brick",
        Op::PlantBrick { .. } => "plant_brick",
        Op::PlaceVoxel { .. } => "place_voxel",
        Op::SetAvatarColors { .. } => "set_avatar_colors",
        Op::SetAvatarParts { .. } => "set_avatar_parts",
        Op::Explode { .. } => "explode",
        Op::Damage { .. } => "damage",
        Op::Beam { .. } => "beam",
        Op::PlayThread { .. } => "play_thread",
        Op::SetFov { .. } => "set_fov",
        Op::SetSpeedScale { .. } => "set_speed_scale",
        Op::GiveAmmo { .. } => "give_ammo",
        Op::SetReserve { .. } => "set_reserve",
        Op::SetRounds { .. } => "set_rounds",
        Op::Reload { .. } => "reload",
        Op::SetMapLights { .. } => "set_map_lights",
        Op::SetEnvironment { .. } => "set_environment",
        Op::SetImageAmmo { .. } => "set_image_ammo",
        Op::SetImageLoaded { .. } => "set_image_loaded",
        Op::MountImage { .. } => "mount_image",
        Op::Emote { .. } => "emote",
        Op::SetTeams { .. } => "set_teams",
        Op::SetTeam { .. } => "set_team",
        Op::SetScore { add: false, .. } => "set_score",
        Op::SetScore { add: true, .. } => "add_score",
        Op::ResetMinigame { .. } => "reset_minigame",
        Op::EndRound { .. } => "end_round",
        Op::SetSetting { team: None, .. } => "set_setting",
        Op::SetSetting { team: Some(_), .. } => "set_team_setting",
        Op::SetBrickItem { .. } => "set_brick_item",
        Op::SetBrickColor { .. } => "set_brick_color",
        Op::SetBrickField { .. } => "set_brick_field",
        Op::NameDrop { .. } => "name_drop",
        Op::SetZonePeriod { .. } => "set_zone_period",
        Op::FireBrickInput { .. } => "fire_brick_input",
        Op::FireGameInput { .. } => "fire_game_input",
        Op::UnmountImage { .. } => "unmount_image",
        Op::MountObject { .. } => "mount_object",
        Op::UnmountObject { .. } => "unmount_object",
        Op::SetScale { .. } => "set_scale",
        Op::SetLookLimits { .. } => "set_look_limits",
        Op::HoldRespawn { .. } => "hold_respawn",
        Op::SetTools { .. } => "set_tools",
        Op::SetRespawnTime { .. } => "set_respawn_time",
        Op::AddBot { .. } => "add_bot",
        Op::RemoveBot { .. } => "remove_bot",
        Op::RestBot { .. } => "rest_bot",
        Op::BotTool { .. } => "bot_tool",
        Op::FollowPath { .. } => "follow_path",
        Op::Camera {
            camera: CameraOp::Free,
            ..
        } => "free_camera",
        Op::Camera { .. } => "orbit_point",
        Op::OrbitCamera {
            body: OrbitBody::Frozen,
            ..
        } => "watch",
        Op::OrbitCamera { .. } => "orbit_camera",
        Op::SpawnEntity { .. } => "spawn_entity",
        Op::RemoveEntity { .. } => "remove_entity",
        Op::Steer { .. } => "steer",
        Op::Label { .. } => "label",
        Op::Tell { .. } => "tell",
        Op::Teleport { .. } => "teleport",
        Op::Respawn { .. } => "respawn",
        Op::SetArchetype { .. } => "set_archetype",
        Op::PushArchetype { .. } => "push_archetype",
        Op::PopArchetype { .. } => "pop_archetype",
        Op::Control { .. } => "control",
        Op::SetBlockState { .. } => "set_block_state",
        Op::Broadcast { .. } => "broadcast",
        Op::TellMinigame { .. } => "tell_minigame",
        Op::PrintMinigame { bottom: false, .. } => "center_print_minigame",
        Op::PrintMinigame { bottom: true, .. } => "bottom_print_minigame",
        Op::CopyBuild { .. } => "copy_build",
        Op::CopyBox { .. } => "copy_box",
        Op::MirrorCopy { .. } => "mirror_copy",
        Op::MirrorGhost { .. } => "mirror_ghost",
        Op::MoveCopy { .. } => "move_copy",
        Op::DropCopy { .. } => "drop_copy",
        Op::ShowCopy { .. } => "show_copy",
        Op::HideCopy { .. } => "hide_copy",
        Op::ShiftCopy { .. } => "shift_copy",
        Op::RotateCopy { .. } => "rotate_copy",
        Op::PlantCopy { .. } => "plant_copy",
        Op::FloatCopy { .. } => "float_copy",
        Op::PlantWait { .. } => "plant_wait",
        Op::CancelCopy { .. } => "cancel_copy",
        Op::PivotCopy { .. } => "pivot_copy",
        Op::PlantAs { .. } => "plant_as",
        Op::ListCopies { .. } => "list_copies",
        Op::WrenchCopy { .. } => "wrench_copy",
        Op::SuperCut { .. } => "super_cut",
        Op::FillBox { .. } => "fill_box",
        Op::TakePaint { .. } => "take_paint",
        Op::ScrollMode { .. } => "scroll_mode",
        Op::CutCopy { .. } => "cut_copy",
        Op::PaintCopy { .. } => "paint_copy",
        Op::PaintFill { .. } => "paint_fill",
        Op::HighlightCopy { .. } => "highlight_copy",
        Op::SaveCopy { .. } => "save_copy",
        Op::LoadCopy { .. } => "load_copy",
        Op::PaintVehicle { .. } => "paint_vehicle",
        Op::TempLook { .. } => "temp_look",
        Op::ShowBox { area: Some(_), .. } => "show_box",
        Op::ShowBox { area: None, .. } => "hide_box",
        Op::GiveItem { .. } => "give_item",
        Op::TakeItem { .. } => "take_item",
        Op::DropItem { .. } => "drop_item",
        Op::RemoveDrop { .. } => "remove_drop",
        Op::WearImage { .. } => "mount_image",
        Op::Push { .. } => "push",
        Op::Tumble { .. } => "tumble",
        Op::Hold { .. } => "hold",
        Op::HoldDistance { .. } => "hold_distance",
        Op::LetGo { .. } => "let_go",
        Op::Tether { .. } => "tether",
        Op::TetherLength { .. } => "tether_length",
        Op::Untether { .. } => "untether",
        Op::Reach { .. } => "reach",
        Op::SpawnVehicle { .. } => "spawn_vehicle",
        Op::RemoveVehicle { .. } => "remove_vehicle",
        Op::Fire { .. } => "fire",
        Op::SpawnExplosion { .. } => "spawn_explosion",
        Op::Heal { .. } => "heal",
        Op::ShowReport { report: Some(_), .. } => "show_report",
        Op::ShowReport { report: None, .. } => "hide_report",
        Op::ReportColumn { .. } => "report_column",
        Op::Print { bottom: false, .. } => "center_print",
        Op::Print { bottom: true, .. } => "bottom_print",
        Op::PlantError { .. } => "plant_error",
        Op::ShowShapes { shapes, .. } if shapes.is_empty() => "hide_shapes",
        Op::ShowShapes { .. } => "show_shapes",
        Op::Ask { .. } => "ask",
        Op::MessageBox { .. } => "message_box",
        Op::Sound {
            at: SoundAt::Position(_),
            ..
        } => "sound_at",
        Op::Sound {
            at: SoundAt::Player(_),
            ..
        } => "play_sound",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_kit_and_respawn_ops_stay_in_their_limits() {
        let parts = |slot: &str, part: &str| Op::SetAvatarParts {
            player: 1,
            parts: BTreeMap::from([(slot.to_owned(), part.to_owned())]),
            face: None,
            decal: None,
        };
        assert!(parts("hat", "copHat").bounded().is_ok());
        assert!(parts("head", "copHat").bounded().is_err(), "the head is a colour, not a part");
        assert!(parts("hat", "").bounded().is_err());
        assert!(parts("hat", &"x".repeat(65)).bounded().is_err());
        let tools = |tools: Vec<Option<String>>| Op::SetTools { player: 1, tools };
        assert!(tools(vec![Some("v20.weapon.hammeritem".into()), None]).bounded().is_ok());
        assert!(tools(vec![None; MAX_TOOL_SLOTS + 1]).bounded().is_err());
        assert!(tools(vec![Some(String::new())]).bounded().is_err());
        let respawn = |ms| Op::SetRespawnTime { player: 1, ms };
        assert!(respawn(None).bounded().is_ok());
        assert!(respawn(Some(MAX_RESPAWN_MS)).bounded().is_ok());
        assert!(respawn(Some(MAX_RESPAWN_MS + 1)).bounded().is_err());
        assert_eq!(respawn(None).capability(), "minigame");
        assert_eq!(parts("hat", "copHat").capability(), "player");
    }
}
