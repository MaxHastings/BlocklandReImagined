//! The operations package behaviour may ask the engine to perform, and the
//! one place they are checked against a package's declared capabilities.
use bri_package::diag::Diagnostic;
use bri_package::setting::SettingValue;
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
/// (512 studs).
pub const MAX_BOX_SPAN: f32 = 256.0;
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
    /// Copy the build at `brick` for `player` to place with `tool`: the
    /// brick and every brick joined to it that the player may build on,
    /// with `above_only` none below the brick. More than `limit` bricks is
    /// refused.
    CopyBuild {
        player: u64,
        brick: u64,
        limit: u32,
        above_only: bool,
        tool: String,
    },
    /// Copy every brick lying wholly inside the box from `min` to `max`
    /// (world units, grown out to the stud and plate grid) that `player`
    /// may build on, for them to place with `tool`. More than `limit`
    /// bricks is refused.
    CopyBox {
        player: u64,
        min: [f32; 3],
        max: [f32; 3],
        limit: u32,
        tool: String,
    },
    /// Mirror the copy `player` holds across `axis`. It shows and plants
    /// mirrored; mirroring it again the same way puts it back.
    MirrorCopy {
        player: u64,
        axis: MirrorAxis,
    },
    /// Remove the bricks `player`'s copy was taken from, as their hammer
    /// would (their full trust), as one step Ctrl+Z puts back as it was.
    CutCopy {
        player: u64,
    },
    /// Paint the bricks `player`'s copy was taken from in palette colour
    /// `color`, as their spray can would, as one step Ctrl+Z takes back.
    PaintCopy {
        player: u64,
        color: u8,
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
    /// Put an image in a worn slot (2 or 3) of a player, tinted with a
    /// palette colour (`mountImage(%image, 3)`: a flag on the back), or
    /// take it off with `None`.
    WearImage {
        player: u64,
        slot: u8,
        image: Option<String>,
        paint: Option<u8>,
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
    /// Keep a mini-game member from respawning until their mini-game resets
    /// or a rule lets them (Slayer's `setDead`: out of lives, between
    /// rounds). The client hides its respawn prompt while held.
    HoldRespawn {
        player: u64,
        held: bool,
    },
    /// Point a player's camera somewhere else while their body stays put
    /// (`setControlObject(camera)`): `Some(player)` orbits that player's
    /// body (the player themselves: their own body or corpse), `None`
    /// hands control back. A watching player's body takes no actions.
    Watch {
        player: u64,
        target: Option<u64>,
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
    /// (`setOrbitPointMode`); `watch(player, ())` hands control back.
    Camera {
        player: u64,
        camera: CameraOp,
    },
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
/// The mirror [`Op::MirrorCopy`] stands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirrorAxis {
    /// Across the world's x axis: east and west swap.
    X,
    /// Across the world's z axis: north and south swap.
    Z,
    /// Left and right as the player faces swap.
    View,
}
impl MirrorAxis {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "x" => Some(Self::X),
            "z" => Some(Self::Z),
            "view" => Some(Self::View),
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
            | Self::PaintFill { .. } => "world.edit",
            Self::Explode { .. }
            | Self::Damage { .. }
            | Self::Heal { .. }
            | Self::Fire { .. } => "damage",
            Self::SpawnEntity { .. }
            | Self::RemoveEntity { .. }
            | Self::Steer { .. }
            | Self::Label { .. } => "entity",
            Self::Tell { .. }
            | Self::Broadcast { .. }
            | Self::Print { .. }
            | Self::TellMinigame { .. }
            | Self::PrintMinigame { .. } => "chat",
            Self::Sound { .. }
            | Self::Beam { .. }
            | Self::PlayThread { .. }
            | Self::ShowBox { .. } => "effects",
            Self::CopyBuild { .. } | Self::CopyBox { .. } | Self::MirrorCopy { .. } => "build",
            Self::SetMapLights { .. } => "lighting",
            Self::SetTeams { .. }
            | Self::SetTeam { .. }
            | Self::SetScore { .. }
            | Self::ResetMinigame { .. }
            | Self::HoldRespawn { .. }
            | Self::EndRound { .. }
            | Self::SetSetting { .. }
            | Self::SetZonePeriod { .. } => "minigame",
            Self::SetBrickItem { .. } | Self::SetBrickColor { .. } => "world.edit",
            Self::FireBrickInput { .. } => "brick_events",
            Self::SetEnvironment { .. } => "environment",
            Self::Teleport { .. }
            | Self::Respawn { .. }
            | Self::SetArchetype { .. }
            | Self::Control { .. }
            | Self::GiveItem { .. }
            | Self::TakeItem { .. }
            | Self::DropItem { .. }
            | Self::RemoveDrop { .. }
            | Self::WearImage { .. }
            | Self::SetFov { .. }
            | Self::SetImageAmmo { .. }
            | Self::MountImage { .. }
            | Self::UnmountImage { .. }
            | Self::SetScale { .. }
            | Self::SetLookLimits { .. }
            | Self::Watch { .. }
            | Self::FollowPath { .. }
            | Self::Camera { .. }
            | Self::SetAvatarColors { .. } => "player",
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
            | Self::CutCopy { .. }
            | Self::PaintCopy { .. }
            | Self::UnmountImage { .. }
            | Self::HoldRespawn { .. }
            | Self::Watch { .. }
            | Self::UnmountObject { .. } => true,
            Self::MountObject {
                mount, rider, node, ..
            } => mount != rider && usize::from(*node) < MAX_MOUNT_POINTS,
            Self::SetScale { scale, .. } => scale.is_finite() && SCALE_RANGE.contains(scale),
            Self::SetLookLimits { limits, .. } => limits
                .is_none_or(|[down, up]| (0.0..=1.0).contains(&down) && (0.0..=1.0).contains(&up)),
            Self::PaintFill { limit, .. } => (1..=MAX_FILL_BRICKS as u32).contains(limit),
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
            Self::CopyBuild { limit, tool, .. } => (1..=10_000).contains(limit) && item(tool),
            Self::CopyBox {
                min,
                max,
                limit,
                tool,
                ..
            } => (1..=10_000).contains(limit) && item(tool) && span(min, max),
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
            Self::FireBrickInput { input, .. } => input.len() <= 64,
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
            Self::Print { text, seconds, .. } | Self::PrintMinigame { text, seconds, .. } => {
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
        Op::SetZonePeriod { .. } => "set_zone_period",
        Op::FireBrickInput { .. } => "fire_brick_input",
        Op::UnmountImage { .. } => "unmount_image",
        Op::MountObject { .. } => "mount_object",
        Op::UnmountObject { .. } => "unmount_object",
        Op::SetScale { .. } => "set_scale",
        Op::SetLookLimits { .. } => "set_look_limits",
        Op::HoldRespawn { .. } => "hold_respawn",
        Op::Watch { .. } => "watch",
        Op::FollowPath { .. } => "follow_path",
        Op::Camera {
            camera: CameraOp::Free,
            ..
        } => "free_camera",
        Op::Camera { .. } => "orbit_point",
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
        Op::TellMinigame { .. } => "tell_minigame",
        Op::PrintMinigame { bottom: false, .. } => "center_print_minigame",
        Op::PrintMinigame { bottom: true, .. } => "bottom_print_minigame",
        Op::CopyBuild { .. } => "copy_build",
        Op::CopyBox { .. } => "copy_box",
        Op::MirrorCopy { .. } => "mirror_copy",
        Op::CutCopy { .. } => "cut_copy",
        Op::PaintCopy { .. } => "paint_copy",
        Op::PaintFill { .. } => "paint_fill",
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
        Op::Reach { .. } => "reach",
        Op::SpawnVehicle { .. } => "spawn_vehicle",
        Op::RemoveVehicle { .. } => "remove_vehicle",
        Op::Fire { .. } => "fire",
        Op::Heal { .. } => "heal",
        Op::Print { bottom: false, .. } => "center_print",
        Op::Print { bottom: true, .. } => "bottom_print",
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
