//! The operations package behaviour may ask the engine to perform, and the
//! one place they are checked against a package's declared capabilities.
use bri_package::diag::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
/// Longest side of a box `copy_box` copies or `show_box` outlines, units
/// (2048 studs: the New Duplicator's largest admin box). What a box holds
/// is bounded by brick counts, not its size.
pub const MAX_BOX_SPAN: f32 = 1024.0;
/// Most bricks one `paint_fill` may paint.
pub const MAX_FILL_BRICKS: usize = 10_000;
/// Widest `beam`, units, and longest it lasts, seconds.
pub const MAX_BEAM_WIDTH: f32 = 16.0;
pub const MAX_BEAM_SECONDS: f32 = 10.0;
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
    /// Damage players within `radius` (falling off linearly) and destroy
    /// bricks within `brick_radius`.
    Explode {
        position: [f32; 3],
        radius: f32,
        damage: f32,
        brick_radius: f32,
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
    /// selection. Everyone sees it.
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
    /// Let every plant of the copy `player` holds float, or not.
    FloatCopy {
        player: u64,
        float: bool,
    },
    /// After each plant of a copy, `player`'s next copy plant waits this
    /// long; one sooner is refused and `on_place` hears `error` `wait`,
    /// with the seconds left in `wait`. 0 lets them plant at once.
    PlantWait {
        player: u64,
        seconds: f32,
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
    /// would (their full trust), as one step Ctrl+Z puts back as it was.
    CutCopy {
        player: u64,
    },
    /// Paint the bricks `player`'s copy was taken from with `paint`, as
    /// their spray or FX can would, as one step Ctrl+Z takes back. With
    /// `each`, every brick they may paint is painted and the rest are
    /// counted (`on_copy`, `action` `"paint"`); else all or none.
    PaintCopy {
        player: u64,
        paint: CopyPaint,
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
    /// Ctrl+Z takes back (`on_copy`, `action` `"supercut"`).
    SuperCut {
        player: u64,
        min: [f32; 3],
        max: [f32; 3],
    },
    /// Fill the empty room in the box from `min` to `max` with the fewest
    /// plain bricks of palette colour `color`, as `player`'s own, as one
    /// step Ctrl+Z takes back (`on_copy`, `action` `"fill"`).
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
    /// Paint `brick` and every brick of its colour joined to it through
    /// shared faces in palette colour `color`, as `player`'s spray can
    /// would paint each one (their full trust; a fill flows around bricks
    /// it may not paint), as one step Ctrl+Z takes back. More than `limit`
    /// bricks is refused.
    PaintFill {
        player: u64,
        brick: u64,
        color: u8,
        limit: u32,
    },
    /// Outline a box for one player while `tool` is in their hand (a
    /// selection, a zone being marked); `None` takes it away.
    ShowBox {
        player: u64,
        area: Option<([f32; 3], [f32; 3])>,
        tool: String,
    },
    /// Put an item in a player's tool list (unless they carry it) and,
    /// with `equip`, in their hand.
    GiveItem {
        player: u64,
        item: String,
        equip: bool,
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
    /// seconds like a dropped tool.
    DropItem {
        item: String,
        position: [f32; 3],
        velocity: [f32; 3],
    },
    /// Change an object's velocity by `velocity` (units per second). `by`
    /// is the player credited when what it hits is hurt or broken.
    Push {
        target: ObjectRef,
        velocity: [f32; 3],
        by: Option<u64>,
    },
    /// Knock a player off their feet into a tumble, flying at `velocity`.
    Tumble {
        player: u64,
        velocity: [f32; 3],
        by: Option<u64>,
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
    /// Give a living player health, up to their archetype's most.
    Heal {
        player: u64,
        amount: f32,
    },
    /// Text in the middle of the screen (`centerPrint`), or above the
    /// bottom edge (`bottomPrint`), for `seconds`: one player's, or
    /// everyone's when `player` is `None`. Empty text clears it.
    Print {
        player: Option<u64>,
        text: String,
        seconds: f32,
        bottom: bool,
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
    /// Play an animation on a player's body (`playThread`): thread 2 the
    /// arms with what they hold, thread 3 a gesture; `root` stops it.
    PlayThread {
        player: u64,
        thread: u8,
        sequence: String,
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
    /// Whether the image in a player's hand has ammo (`setImageAmmo`), which
    /// its states' `ammo` transitions read.
    SetImageAmmo {
        player: u64,
        ammo: bool,
    },
    /// Put another image in a player's hand, keeping their tool slot
    /// (`mountImage`): a scope, a second fire mode. `None` puts back the
    /// selected tool's own image.
    MountImage {
        player: u64,
        image: Option<String>,
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
    /// changing body while the new one has the node.
    MountObject {
        mount: u64,
        rider: u64,
        node: u8,
        can_dismount: bool,
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
}
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
    /// Build on their bricks.
    Build,
    /// Also paint and hammer them (v20's duplicators asked this).
    Full,
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
}
impl Default for CopyRule {
    fn default() -> Self {
        Self {
            trust: CopyTrust::Build,
            public: true,
            admin: true,
            partial: false,
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
/// What [`Op::PaintCopy`] puts on bricks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyPaint {
    /// A palette colour (the spray cans).
    Color(u8),
    /// A colour effect, 0 to 6: none, pearl, chrome, glow, blink, swirl,
    /// rainbow (the colour FX cans).
    ColorEffect(u8),
    /// A shape effect, 0 to 2: none, undulo, water (the shape FX cans).
    ShapeEffect(u8),
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
/// Mount points a body may have (`mountObject`'s node).
pub const MAX_MOUNT_POINTS: usize = 8;
/// Body scales `set_scale` allows.
pub const SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.2..=5.0;
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
            | Self::PaintFill { .. } => "world.edit",
            Self::Explode { .. }
            | Self::Damage { .. }
            | Self::Heal { .. }
            | Self::Fire { .. } => "damage",
            Self::SpawnEntity { .. }
            | Self::RemoveEntity { .. }
            | Self::Steer { .. }
            | Self::Label { .. } => "entity",
            Self::Tell { .. } | Self::Broadcast { .. } | Self::Print { .. } | Self::Ask { .. } => {
                "chat"
            }
            Self::Sound { .. }
            | Self::Beam { .. }
            | Self::PlayThread { .. }
            | Self::ShowBox { .. } => "effects",
            Self::CopyBuild { .. }
            | Self::CopyBox { .. }
            | Self::SaveCopy { .. }
            | Self::LoadCopy { .. }
            | Self::MirrorCopy { .. }
            | Self::MoveCopy { .. }
            | Self::DropCopy { .. }
            | Self::ShowCopy { .. }
            | Self::HideCopy { .. }
            | Self::ShiftCopy { .. }
            | Self::RotateCopy { .. }
            | Self::PlantCopy { .. }
            | Self::FloatCopy { .. }
            | Self::PlantWait { .. }
            | Self::PivotCopy { .. }
            | Self::PlantAs { .. }
            | Self::ListCopies { .. }
            | Self::TakePaint { .. }
            | Self::HighlightCopy { .. } => "build",
            Self::SetMapLights { .. } => "lighting",
            Self::SetEnvironment { .. } => "environment",
            Self::Teleport { .. }
            | Self::Respawn { .. }
            | Self::SetArchetype { .. }
            | Self::Control { .. }
            | Self::GiveItem { .. }
            | Self::TakeItem { .. }
            | Self::DropItem { .. }
            | Self::SetFov { .. }
            | Self::SetImageAmmo { .. }
            | Self::MountImage { .. }
            | Self::UnmountImage { .. }
            | Self::SetScale { .. }
            | Self::SetLookLimits { .. }
            | Self::SetAvatarColors { .. }
            | Self::ScrollMode { .. } => "player",
            Self::MountObject { .. } | Self::UnmountObject { .. } => "physics",
            Self::Push { .. }
            | Self::Tumble { .. }
            | Self::Hold { .. }
            | Self::HoldDistance { .. }
            | Self::LetGo { .. }
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
            | Self::MirrorCopy { .. }
            | Self::DropCopy { .. }
            | Self::ShowCopy { .. }
            | Self::HideCopy { .. }
            | Self::PlantCopy { .. }
            | Self::FloatCopy { .. }
            | Self::PivotCopy { .. }
            | Self::TakePaint { .. }
            | Self::ScrollMode { .. }
            | Self::CutCopy { .. }
            | Self::WrenchCopy { .. }
            | Self::UnmountImage { .. }
            | Self::UnmountObject { .. } => true,
            Self::MountObject {
                mount, rider, node, ..
            } => mount != rider && usize::from(*node) < MAX_MOUNT_POINTS,
            Self::SetScale { scale, .. } => scale.is_finite() && SCALE_RANGE.contains(scale),
            Self::SetLookLimits { limits, .. } => limits
                .is_none_or(|[down, up]| (0.0..=1.0).contains(&down) && (0.0..=1.0).contains(&up)),
            Self::PaintFill { limit, .. } => (1..=MAX_FILL_BRICKS as u32).contains(limit),
            Self::PaintCopy { paint, .. } => match paint {
                CopyPaint::Color(_) => true,
                CopyPaint::ColorEffect(fx) => *fx <= 6,
                CopyPaint::ShapeEffect(fx) => *fx <= 2,
            },
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
            Self::SetBlockState { state, .. } => {
                state.len() <= 64 && !state.chars().any(char::is_control)
            }
            Self::SetArchetype { archetype, .. } => {
                archetype.len() <= 160 && !archetype.chars().any(char::is_control)
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
            } => {
                finite(position)
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
                thread, sequence, ..
            } => {
                (2..=3).contains(thread)
                    && !sequence.is_empty()
                    && sequence.len() <= 64
                    && sequence
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }
            Self::SetFov { fov, .. } => fov.is_none_or(|f| FOV_RANGE.contains(&f)),
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
            Self::MountImage { image, .. } => image
                .as_deref()
                .is_none_or(|i| bri_package::id::is_content_ref(i, Some("image"))),
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
            Self::Tell { text, .. } | Self::Broadcast { text } => chat(text),
            Self::CopyBuild { limit, tool, .. } => (1..=10_000).contains(limit) && item(tool),
            Self::CopyBox {
                min,
                max,
                limit,
                tool,
                ..
            } => (1..=10_000).contains(limit) && item(tool) && span(min, max),
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
                    && (1..=10_000).contains(limit)
                    && item(tool)
            }
            Self::HighlightCopy { color, seconds, .. } => {
                color
                    .iter()
                    .flatten()
                    .all(|c| (0.0..=1.0).contains(c))
                    && (0.0..=60.0).contains(seconds)
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
            } => {
                item(id)
                    && finite(position)
                    && finite(velocity)
                    && glam_length(velocity) <= MAX_PUSH_SPEED
            }
            Self::Push { velocity, .. } | Self::Tumble { velocity, .. } => {
                finite(velocity) && glam_length(velocity) <= MAX_PUSH_SPEED
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
            Self::LetGo { .. } | Self::RemoveVehicle { .. } => true,
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
            Self::Heal { amount, .. } => amount.is_finite() && (0.0..=100_000.0).contains(amount),
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
            Self::Print { text, seconds, .. } => {
                text.chars().count() <= MAX_PRINT_CHARS
                    && !text.chars().any(|c| c.is_control() && c != '\n')
                    && seconds.is_finite()
                    && (0.0..=600.0).contains(seconds)
            }
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
        Op::Explode { .. } => "explode",
        Op::Damage { .. } => "damage",
        Op::Beam { .. } => "beam",
        Op::PlayThread { .. } => "play_thread",
        Op::SetFov { .. } => "set_fov",
        Op::SetMapLights { .. } => "set_map_lights",
        Op::SetEnvironment { .. } => "set_environment",
        Op::SetImageAmmo { .. } => "set_image_ammo",
        Op::MountImage { .. } => "mount_image",
        Op::UnmountImage { .. } => "unmount_image",
        Op::MountObject { .. } => "mount_object",
        Op::UnmountObject { .. } => "unmount_object",
        Op::SetScale { .. } => "set_scale",
        Op::SetLookLimits { .. } => "set_look_limits",
        Op::SpawnEntity { .. } => "spawn_entity",
        Op::RemoveEntity { .. } => "remove_entity",
        Op::Steer { .. } => "steer",
        Op::Label { .. } => "label",
        Op::Tell { .. } => "tell",
        Op::Teleport { .. } => "teleport",
        Op::Respawn { .. } => "respawn",
        Op::SetArchetype { .. } => "set_archetype",
        Op::Control { .. } => "control",
        Op::SetBlockState { .. } => "set_block_state",
        Op::Broadcast { .. } => "broadcast",
        Op::CopyBuild { .. } => "copy_build",
        Op::CopyBox { .. } => "copy_box",
        Op::MirrorCopy { .. } => "mirror_copy",
        Op::MoveCopy { .. } => "move_copy",
        Op::DropCopy { .. } => "drop_copy",
        Op::ShowCopy { .. } => "show_copy",
        Op::HideCopy { .. } => "hide_copy",
        Op::ShiftCopy { .. } => "shift_copy",
        Op::RotateCopy { .. } => "rotate_copy",
        Op::PlantCopy { .. } => "plant_copy",
        Op::FloatCopy { .. } => "float_copy",
        Op::PlantWait { .. } => "plant_wait",
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
        Op::ShowBox { area: Some(_), .. } => "show_box",
        Op::ShowBox { area: None, .. } => "hide_box",
        Op::GiveItem { .. } => "give_item",
        Op::TakeItem { .. } => "take_item",
        Op::DropItem { .. } => "drop_item",
        Op::Push { .. } => "push",
        Op::Tumble { .. } => "tumble",
        Op::Hold { .. } => "hold",
        Op::HoldDistance { .. } => "hold_distance",
        Op::LetGo { .. } => "let_go",
        Op::Reach { .. } => "reach",
        Op::SpawnVehicle { .. } => "spawn_vehicle",
        Op::RemoveVehicle { .. } => "remove_vehicle",
        Op::Fire { .. } => "fire",
        Op::Heal { .. } => "heal",
        Op::Print { bottom: false, .. } => "center_print",
        Op::Print { bottom: true, .. } => "bottom_print",
        Op::Ask { .. } => "ask",
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
