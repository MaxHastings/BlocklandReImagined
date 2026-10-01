//! The sandboxed server script runtime (a Rhai prototype, not a commitment).
//!
//! A script is a set of functions. The engine calls them with a read-only
//! snapshot of the game; a script reads that snapshot, edits its own
//! package's state and entity variables, and asks for operations. Nothing a
//! script does touches the game directly: the engine commits the state and
//! applies the operations afterwards, through [`crate::ops::authorize`].
//!
//! Sandbox: no file, network, clock, module or `eval` access; a fixed
//! operation budget per call; bounded strings, arrays, maps, call depth and
//! operation count. A failing or over-budget call changes nothing.
use crate::manifest::location;
use crate::ops::{FillPaint, ObjectRef, Op, SoundAt, TempLook, VehiclePaint, WorldShape};
use crate::state::{Namespace, PlayerKey, check_value};
use bri_package::diag::Diagnostic;
use rhai::{AST, Array, Dynamic, Engine, EvalAltResult, Map};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

mod bots;
mod games;
pub use bots::BotKindView;
pub use games::{BrickView, DropView, MAX_BRICKS_LISTED, MinigameView, TeamView};

/// Operation budgets per kind of call.
#[derive(Debug, Clone, Copy)]
pub enum Budget {
    Command,
    Think,
    Tick,
    Generate,
}
impl Budget {
    pub fn operations(self) -> u64 {
        match self {
            Self::Command => 200_000,
            Self::Think => 100_000,
            Self::Tick => 400_000,
            // One chunk is generated per tick, so its budget fits a tick.
            Self::Generate => 400_000,
        }
    }
}
const MAX_OPS_PER_CALL: usize = 1024;
const MAX_OUTPUT_LINES: usize = 32;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerView {
    pub id: u64,
    pub key: PlayerKey,
    pub name: String,
    pub position: [f32; 3],
    pub alive: bool,
    pub admin: bool,
    /// Where the player sees from, and the unit direction they look.
    #[serde(default)]
    pub eye: [f32; 3],
    #[serde(default)]
    pub look: [f32; 3],
    /// Their control object's transform (`getControlObject().getTransform()`):
    /// a free or path camera's eye and heading while one has control, else
    /// the body's eye and heading. Yaw and pitch in radians.
    #[serde(default)]
    pub camera: [f32; 3],
    #[serde(default)]
    pub camera_yaw: f32,
    #[serde(default)]
    pub camera_pitch: f32,
    #[serde(default)]
    pub velocity: [f32; 3],
    /// The item in their hand (`namespace:weapon/name`), or empty.
    #[serde(default)]
    pub item: String,
    /// The minigame they play in, if any.
    #[serde(default)]
    pub minigame: Option<u64>,
    #[serde(default)]
    pub health: f32,
    #[serde(default)]
    pub max_health: f32,
    /// What they are: an archetype id (`package:archetype/name`, or
    /// `v20.player.<datablock>`).
    #[serde(default)]
    pub archetype: String,
    #[serde(default)]
    pub crouched: bool,
    /// Seated on a vehicle or riding another player.
    #[serde(default)]
    pub mounted: bool,
    /// Body scale (1 for a normal body).
    #[serde(default)]
    pub scale: f32,
    /// The middle of their body (`getWorldBoxCenter`).
    #[serde(default)]
    pub center: [f32; 3],
    /// Their selected tool slot (`currTool`), from 0.
    #[serde(default)]
    pub slot: Option<u64>,
    /// The image in their hand (`getMountedImage(0)`) and the name of the
    /// state it is in (`getImageState(0)`), or empty.
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub image_state: String,
    /// The palette index of the colour their spray can last picked.
    #[serde(default)]
    pub paint: u8,
    /// The FX can they last picked (`serverCmdUseFXCan`'s index, from 0),
    /// or `None` when it was a colour can.
    #[serde(default)]
    pub fx_can: Option<u8>,
    /// Whether they may paint now: their minigame's painting rule
    /// (`enablePainting`), or true outside minigames.
    #[serde(default)]
    pub may_paint: bool,
    /// Where the image in their hand fires from (`getMuzzlePoint(0)`), or
    /// the eye when they hold nothing.
    #[serde(default)]
    pub muzzle: [f32; 3],
    /// Each tool slot's item id, empty for an empty slot (`%obj.tool[%i]`).
    #[serde(default)]
    pub tools: Vec<String>,
    /// The team of their mini-game they play for, if it has teams.
    #[serde(default)]
    pub team: Option<u64>,
    /// Their mini-game score (0 outside mini-games).
    #[serde(default)]
    pub score: i64,
    /// A bot (an `AIPlayer`), not a connected player.
    #[serde(default)]
    pub bot: bool,
    /// A bot's spawn brick's owner (`%bot.spawnBrick.client`), if any.
    #[serde(default)]
    pub bot_owner: Option<u64>,
    /// The package whose rules added this bot (`add_bot`), if a rule did
    /// rather than a spawn brick.
    #[serde(default)]
    pub spawner: Option<String>,
    /// The player this one rides (`getObjectMount`), and on which of its
    /// mount points.
    #[serde(default)]
    pub riding: Option<(u64, u8)>,
    /// Copy work of theirs goes on over the next ticks (a big selection,
    /// plant, cut, paint, wrench, undo or load; `cancel_copy` stops it).
    #[serde(default)]
    pub copy_working: bool,
    /// The copy they hold from a duplicator (`copy_build`, `load_copy`):
    /// the Add-On that took it and its bricks.
    #[serde(default)]
    pub copy: Option<(String, u64)>,
    /// They hold bricks with a ghost brick out, where it would plant.
    #[serde(default)]
    pub ghost: bool,
}
/// Live questions a script may ask the engine during a call. They read the
/// world as it is when the call runs: a call's own operations apply after it
/// returns, so a brick it removes still stops its rays.
pub trait World {
    /// The first thing a ray meets within `range` of `from` along the unit
    /// `direction`, passing through the body of the player `ignore`.
    fn raycast(
        &self,
        from: [f32; 3],
        direction: [f32; 3],
        range: f32,
        ignore: Option<u64>,
    ) -> Option<RayHit>;
    /// Whether the minigame and trust rules let player `by` hurt `target`
    /// (`minigameCanDamage`).
    fn can_damage(&self, by: u64, target: ObjectRef) -> bool;
    /// The box brick `brick` fills (its grid cells), lowest corner first.
    fn brick_box(&self, brick: u64) -> Option<([f32; 3], [f32; 3])>;
    /// The generated world's voxel that `brick` is: its voxel coordinates
    /// and material id.
    fn voxel(&self, brick: u64) -> Option<([i64; 3], String)>;
    /// Whether a voxel could be placed at voxel coordinates `position`
    /// now: inside the world, its chunk generated, and nothing in the way.
    fn can_place_voxel(&self, position: [i64; 3]) -> bool;
    /// Up to `limit` bricks of definition `kind`, lowest id first.
    fn bricks_of(&self, _kind: &str, _limit: usize) -> Vec<BrickView> {
        Vec::new()
    }
    fn brick(&self, _brick: u64) -> Option<BrickView> {
        None
    }
    /// The value a package keeps on `brick` as `key` (`set_brick_field`):
    /// the calling package's own key, or `namespace:key` for another's.
    fn brick_field(&self, _brick: u64, _key: &str) -> Option<serde_json::Value> {
        None
    }
    /// The world's paint palette, RGBA from 0 to 1, by colour index.
    fn palette(&self) -> Vec<[f32; 4]> {
        Vec::new()
    }
    /// The server's avatar pack choices, in the pack's order (v20's list
    /// positions): each part slot's (`hat`, `pack`, ...), `face` and
    /// `decal`, and `accents.<hat>` for the accents each hat wears
    /// (`$accentsAllowed`). Empty when the server has no avatar pack.
    fn avatar_choices(&self) -> BTreeMap<String, Vec<String>> {
        BTreeMap::new()
    }
    /// The items the calling package put in the world with `drop_item`
    /// that still lie there.
    fn drops(&self) -> Vec<DropView> {
        Vec::new()
    }
    /// Bricks whose box overlaps the box from `min` to `max`, at most
    /// `limit` of them (`InitContainerBoxSearch`).
    fn bricks_in(&self, _min: [f32; 3], _max: [f32; 3], _limit: usize) -> Vec<u64> {
        vec![]
    }
    /// Whether a rule acting for `caller` may remove brick `brick`, or
    /// plant into its build (`miniGameCanDamage` with the trust rules).
    fn can_edit(&self, _caller: Option<u64>, _brick: u64) -> bool {
        false
    }
    /// Whether a copy by `player` under `rule` would take brick `brick`
    /// (the trust a copy checks, [`crate::ops::CopyRule`]).
    fn may_copy(&self, _player: u64, _brick: u64, _rule: crate::ops::CopyRule) -> bool {
        false
    }
    /// Whether a brick of `kind` turned `turns` quarter turns would fit
    /// centred at `position` now (snapped to the grid as `plant_brick`
    /// does): nothing in the way, inside the world.
    fn can_plant(&self, _kind: &str, _position: [f32; 3], _turns: u8) -> bool {
        false
    }
    /// Which part of player `player` a hit at `point` strikes
    /// (`getDamageLocation`): `"head"`, `"torso"` or `"legs"`, or `None`
    /// for no living player.
    fn hit_region(&self, _player: u64, _point: [f32; 3]) -> Option<&'static str> {
        None
    }
    /// An Add-On setting of mini-game `game` (or of its team `team`):
    /// `key` is the calling package's own or `namespace:key`. Its value,
    /// or its default when nobody changed it; an error names what is
    /// wrong (no such game, team or setting).
    fn setting(
        &self,
        _game: u64,
        _team: Option<u64>,
        _key: &str,
    ) -> Result<bri_package::setting::SettingValue, String> {
        Err("this host has no Add-On settings".into())
    }
}
/// Most bricks one `bricks_in` returns.
pub const MAX_BRICKS_IN: usize = 1024;
/// What a ray met.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RayTarget {
    Object(ObjectRef),
    Brick(u64),
    /// The map, its terrain or a map shape.
    Map,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub target: RayTarget,
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub distance: f32,
    /// The part of a player the ray struck (`"head"`, `"torso"` or
    /// `"legs"`), `None` for anything else.
    pub region: Option<&'static str>,
}
/// A loose physics body or other movable thing, as scripts see it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectView {
    pub object: ObjectRef,
    /// A vehicle's definition, an entity's kind; empty for players.
    pub definition: String,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub mass: f32,
    /// Radius of a sphere around it, units.
    pub radius: f32,
    /// The player it belongs to (a vehicle's spawner, the player
    /// themselves), when there is one.
    pub owner: Option<u64>,
    /// The package that spawned it (`spawn_vehicle`), or empty.
    #[serde(default)]
    pub package: String,
}
/// What a player holds (`hold`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldView {
    pub player: u64,
    pub object: ObjectRef,
    pub distance: f32,
}
/// A player's rope (`tether`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TetherView {
    pub player: u64,
    pub anchor: [f32; 3],
    /// Its length now, and the length it reels toward.
    pub length: f32,
    pub target: f32,
    /// The brick it is tied to, if any.
    pub brick: Option<u64>,
    /// The player, vehicle or entity it is tied to, if any.
    #[serde(default)]
    pub object: Option<ObjectRef>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityView {
    pub id: u64,
    pub kind: String,
    pub position: [f32; 3],
    pub yaw: f32,
    pub label: String,
    pub health: f32,
    /// Horizontal speed, units per second.
    pub speed: f32,
}
/// What the caller is aiming at, resolved by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aim {
    pub brick: Option<u64>,
    /// The brick's provider tag (for generated voxels, the material id).
    pub tag: Option<String>,
    /// The brick's block and its state, when it shows a block.
    #[serde(default)]
    pub look: Option<(String, String)>,
    pub position: [f32; 3],
    /// The face it met points this way (zero when it met only an object).
    #[serde(default)]
    pub normal: [f32; 3],
    pub distance: f32,
    /// The movable object the aim met before any brick.
    #[serde(default)]
    pub object: Option<AimObject>,
}
/// A movable object an aim met, and whether the caller may move it under
/// the minigame and trust rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AimObject {
    pub object: ObjectRef,
    pub position: [f32; 3],
    pub distance: f32,
    pub movable: bool,
}
/// Read-only game facts for one tick.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub tick: u64,
    /// The host's game version, as players see it on the main menu
    /// (`game_version()`).
    pub game_version: String,
    pub seed: i64,
    /// The live environment settings (`environment()`).
    pub environment: bri_content::atmosphere::Settings,
    pub players: Vec<PlayerView>,
    /// Bots: player bodies without a connection. [`player`](Self::player)
    /// finds them; `players()` leaves them out.
    pub bots: Vec<PlayerView>,
    /// The bot kinds enabled Add-Ons provide (`bot_kinds()`).
    pub bot_kinds: Vec<bots::BotKindView>,
    pub entities: Vec<EntityView>,
    /// Vehicles and other loose physics bodies, and bots (players without
    /// a connection, `object: player`, `definition` their kind, `owner`
    /// their spawn brick's). Players and entities are in their own lists,
    /// and in [`object`](Self::object)'s answers.
    pub objects: Vec<ObjectView>,
    pub holds: Vec<HoldView>,
    /// Every mini-game, with its members and teams.
    pub minigames: Vec<MinigameView>,
    pub tethers: Vec<TetherView>,
}
impl Snapshot {
    /// A connected player or a bot.
    pub fn player(&self, id: u64) -> Option<&PlayerView> {
        self.players.iter().chain(&self.bots).find(|p| p.id == id)
    }
    /// Any movable object by reference, players and entities included.
    pub fn object(&self, object: ObjectRef) -> Option<ObjectView> {
        match object {
            ObjectRef::Vehicle(_) => self.objects.iter().find(|o| o.object == object).cloned(),
            ObjectRef::Player(id) => self
                .players
                .iter()
                .find(|p| p.id == id)
                .map(|p| ObjectView {
                    object,
                    definition: String::new(),
                    position: p.position,
                    velocity: p.velocity,
                    mass: crate::ops::PLAYER_MASS,
                    radius: 1.3,
                    owner: Some(id),
                    package: String::new(),
                })
                // A bot: a player body among the objects.
                .or_else(|| self.objects.iter().find(|o| o.object == object).cloned()),
            ObjectRef::Entity(id) => {
                self.entities
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| ObjectView {
                        object,
                        definition: e.kind.clone(),
                        position: e.position,
                        velocity: [0.0; 3],
                        mass: crate::ops::PLAYER_MASS,
                        radius: 1.3,
                        owner: None,
                        package: String::new(),
                    })
            }
        }
    }
}

/// One call's inputs.
pub struct Call<'a> {
    pub function: &'a str,
    pub args: Vec<Dynamic>,
    pub budget: Budget,
    pub snapshot: Arc<Snapshot>,
    pub caller: Option<u64>,
    pub aim: Option<Aim>,
    /// The entity a `think` call is for.
    pub entity: Option<u64>,
    pub state: Namespace,
    /// Package-local variables of the package's entities, shared by every
    /// call in a tick; a call's writes come back in its [`Outcome`].
    pub entity_vars: Arc<EntityVars>,
    /// The live world `raycast` and `can_damage` ask; without one they fail.
    pub world: Option<&'a dyn World>,
}
/// Each entity's package-local variables.
pub type EntityVars = BTreeMap<u64, BTreeMap<String, serde_json::Value>>;
/// One call's results. Only produced when the call succeeded.
#[derive(Debug)]
pub struct Outcome {
    pub returned: Dynamic,
    pub ops: Vec<Op>,
    pub state: Namespace,
    /// The complete variables of each entity the call wrote to.
    pub entity_vars: EntityVars,
    pub output: Vec<String>,
}

struct Invocation {
    snapshot: Arc<Snapshot>,
    caller: Option<u64>,
    aim: Option<Aim>,
    entity: Option<u64>,
    state: Namespace,
    entity_vars: Arc<EntityVars>,
    /// Entities this call wrote, with all their variables.
    written: EntityVars,
    ops: Vec<Op>,
    output: Vec<String>,
    /// The call's [`World`], valid only while the call runs (see
    /// [`Runtime::call`]).
    ///
    /// Why a raw pointer: script functions are registered once as
    /// `'static` closures and reach the running call through this
    /// thread-local, and Rhai's per-call channels (`CallFnOptions` tags,
    /// `this_ptr`) carry only `'static` `Dynamic` values. The world borrows
    /// the session for the call, so it cannot be `'static`; making it so
    /// would mean copying the physics world per call, or running scripts
    /// on another thread. The pointer is set and cleared in `Runtime::call`
    /// only, and read only through `with_world`.
    world: Option<*const (dyn World + 'static)>,
    rays: usize,
}
thread_local! {
    static CURRENT: RefCell<Option<Invocation>> = const { RefCell::new(None) };
    /// Script operations the running call may use.
    static LIMIT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
/// A point as a script reads it, `[x, y, z]`.
fn point3(p: [f32; 3]) -> Dynamic {
    Dynamic::from_array(p.iter().map(|v| Dynamic::from_float(f64::from(*v))).collect())
}
/// The running call's world.
fn with_world<T>(f: impl FnOnce(&dyn World, &mut Invocation) -> Fallible<T>) -> Fallible<T> {
    with(|i| {
        let Some(world) = i.world else {
            return fail("the world cannot be asked here");
        };
        // SAFETY: `Runtime::call` stores this pointer from a reference that
        // outlives the call and takes the invocation back out before it
        // returns, so it is only reached while the reference is live.
        let world = unsafe { &*world };
        f(world, i)
    })
}
type Fallible<T> = Result<T, Box<EvalAltResult>>;
fn fail<T>(message: impl Into<String>) -> Fallible<T> {
    Err(message.into().into())
}
/// `#{ color: n }`, `#{ color_effect: n }` or `#{ shape_effect: n }`, for
/// `paint_fill` and `paint_copy`.
fn fill_paint(what: &str, paint: &Map) -> Fallible<FillPaint> {
    let index = |v: &Dynamic, key: &str| {
        v.as_int()
            .ok()
            .and_then(|i| u8::try_from(i).ok())
            .ok_or_else(|| format!("{key} is a number, 0 to 255"))
    };
    let mut chosen = None;
    for (key, value) in paint {
        let p = match key.as_str() {
            "color" => FillPaint::Color(index(value, "color")?),
            "color_effect" => FillPaint::ColorEffect(index(value, "color_effect")?),
            "shape_effect" => FillPaint::ShapeEffect(index(value, "shape_effect")?),
            other => {
                return fail(format!(
                    "{what} paints color, color_effect or shape_effect, not `{other}`"
                ));
            }
        };
        if chosen.replace(p).is_some() {
            return fail(format!("{what} paints one of color, color_effect or shape_effect"));
        }
    }
    chosen.ok_or_else(|| format!("{what} needs #{{ color: n }} or an effect").into())
}
fn with<T>(f: impl FnOnce(&mut Invocation) -> Fallible<T>) -> Fallible<T> {
    CURRENT.with(|c| match c.borrow_mut().as_mut() {
        Some(invocation) => f(invocation),
        None => fail("no script call is active"),
    })
}
fn number(value: &Dynamic) -> Fallible<f64> {
    if let Ok(i) = value.as_int() {
        Ok(i as f64)
    } else if let Ok(f) = value.as_float() {
        Ok(f)
    } else {
        fail(format!("expected a number, got {}", value.type_name()))
    }
}
fn float(value: &Dynamic) -> Fallible<f32> {
    let v = number(value)?;
    if v.is_finite() {
        Ok(v as f32)
    } else {
        fail("number is not finite")
    }
}
fn id(value: &Dynamic) -> Fallible<u64> {
    match value.as_int() {
        Ok(i) if i >= 0 => Ok(i as u64),
        _ => fail(format!("expected an id, got {}", value.type_name())),
    }
}
fn push(op: Op) -> Fallible<()> {
    with(|i| {
        if i.ops.len() >= MAX_OPS_PER_CALL {
            return fail(format!(
                "more than {MAX_OPS_PER_CALL} operations in one call"
            ));
        }
        i.ops.push(op);
        Ok(())
    })
}
fn to_json(value: &Dynamic) -> Fallible<serde_json::Value> {
    let json: serde_json::Value = rhai::serde::from_dynamic(value)?;
    check_value(&json).map_err(|e| e.to_string())?;
    Ok(json)
}
fn to_dynamic(value: &serde_json::Value) -> Dynamic {
    rhai::serde::to_dynamic(value).unwrap_or(Dynamic::UNIT)
}
fn map(entries: impl IntoIterator<Item = (&'static str, Dynamic)>) -> Dynamic {
    let mut m = Map::new();
    for (k, v) in entries {
        m.insert(k.into(), v);
    }
    Dynamic::from_map(m)
}
fn position(p: [f32; 3]) -> [(&'static str, Dynamic); 3] {
    [
        ("x", Dynamic::from_float(p[0] as f64)),
        ("y", Dynamic::from_float(p[1] as f64)),
        ("z", Dynamic::from_float(p[2] as f64)),
    ]
}
fn float_entry(key: &'static str, v: f32) -> (&'static str, Dynamic) {
    (key, Dynamic::from_float(v as f64))
}
fn player_map(p: &PlayerView) -> Dynamic {
    let [x, y, z] = position(p.position);
    map([
        ("id", Dynamic::from_int(p.id as i64)),
        ("name", p.name.clone().into()),
        x,
        y,
        z,
        ("alive", p.alive.into()),
        ("admin", p.admin.into()),
        ("copy_working", p.copy_working.into()),
        ("ghost", p.ghost.into()),
        (
            "copy",
            p.copy.as_ref().map_or(Dynamic::UNIT, |(package, bricks)| {
                map([
                    ("addon", package.clone().into()),
                    ("bricks", Dynamic::from_int(*bricks as i64)),
                ])
            }),
        ),
        float_entry("ex", p.eye[0]),
        float_entry("ey", p.eye[1]),
        float_entry("ez", p.eye[2]),
        float_entry("lx", p.look[0]),
        float_entry("ly", p.look[1]),
        float_entry("lz", p.look[2]),
        (
            "camera",
            map([
                (
                    "at",
                    Dynamic::from_array(
                        p.camera
                            .iter()
                            .map(|v| Dynamic::from_float(f64::from(*v)))
                            .collect(),
                    ),
                ),
                float_entry("yaw", p.camera_yaw),
                float_entry("pitch", p.camera_pitch),
            ]),
        ),
        float_entry("vx", p.velocity[0]),
        float_entry("vy", p.velocity[1]),
        float_entry("vz", p.velocity[2]),
        ("item", p.item.clone().into()),
        (
            "minigame",
            p.minigame
                .map_or(Dynamic::UNIT, |g| Dynamic::from_int(g as i64)),
        ),
        float_entry("health", p.health),
        float_entry("max_health", p.max_health),
        ("archetype", p.archetype.clone().into()),
        ("crouched", p.crouched.into()),
        ("mounted", p.mounted.into()),
        float_entry("scale", p.scale),
        float_entry("cx", p.center[0]),
        float_entry("cy", p.center[1]),
        float_entry("cz", p.center[2]),
        (
            "slot",
            p.slot.map_or(Dynamic::UNIT, |s| Dynamic::from_int(s as i64)),
        ),
        ("image", p.image.clone().into()),
        ("image_state", p.image_state.clone().into()),
        ("paint", Dynamic::from_int(i64::from(p.paint))),
        (
            "fx_can",
            p.fx_can
                .map_or(Dynamic::UNIT, |c| Dynamic::from_int(i64::from(c))),
        ),
        ("may_paint", p.may_paint.into()),
        float_entry("mx", p.muzzle[0]),
        float_entry("my", p.muzzle[1]),
        float_entry("mz", p.muzzle[2]),
        (
            "tools",
            Dynamic::from_array(p.tools.iter().map(|t| t.clone().into()).collect()),
        ),
        (
            "team",
            p.team.map_or(Dynamic::UNIT, |t| Dynamic::from_int(t as i64)),
        ),
        ("score", Dynamic::from_int(p.score)),
        ("bot", p.bot.into()),
        (
            "bot_owner",
            p.bot_owner
                .map_or(Dynamic::UNIT, |o| Dynamic::from_int(o as i64)),
        ),
        (
            "spawner",
            p.spawner.clone().map_or(Dynamic::UNIT, Dynamic::from),
        ),
        (
            "riding",
            p.riding
                .map_or(Dynamic::UNIT, |(m, _)| Dynamic::from_int(m as i64)),
        ),
        (
            "seat",
            p.riding
                .map_or(Dynamic::UNIT, |(_, s)| Dynamic::from_int(i64::from(s))),
        ),
    ])
}
fn object_map(o: &ObjectView) -> Dynamic {
    let [x, y, z] = position(o.position);
    let speed = (o.velocity[0].powi(2) + o.velocity[1].powi(2) + o.velocity[2].powi(2)).sqrt();
    map([
        ("ref", o.object.to_string().into()),
        ("kind", o.object.kind().into()),
        ("id", Dynamic::from_int(o.object.id() as i64)),
        ("definition", o.definition.clone().into()),
        x,
        y,
        z,
        float_entry("vx", o.velocity[0]),
        float_entry("vy", o.velocity[1]),
        float_entry("vz", o.velocity[2]),
        float_entry("speed", speed),
        float_entry("mass", o.mass),
        float_entry("radius", o.radius),
        (
            "owner",
            o.owner
                .map_or(Dynamic::UNIT, |owner| Dynamic::from_int(owner as i64)),
        ),
        ("spawner", o.package.clone().into()),
    ])
}
/// `[r, g, b]` or `[r, g, b, a]`, each 0 to 1.
fn color<const N: usize>(value: Dynamic, what: &str) -> Fallible<[f32; N]> {
    let list = value
        .into_typed_array::<Dynamic>()
        .map_err(|_| format!("{what} is a list of {N} numbers from 0 to 1"))?;
    let list = list.iter().map(float).collect::<Fallible<Vec<f32>>>()?;
    <[f32; N]>::try_from(list)
        .map_err(|_| format!("{what} is a list of {N} numbers from 0 to 1").into())
}
fn color_value(c: &[f32]) -> Dynamic {
    Dynamic::from_array(c.iter().map(|v| Dynamic::from_float(f64::from(*v))).collect())
}
/// The set environment settings as a script reads them; unset ones are
/// absent (the map's own).
fn environment_map(e: &bri_content::atmosphere::Settings, tick: u64) -> Dynamic {
    let mut m = Map::new();
    let mut put = |k: &str, v: Dynamic| {
        m.insert(k.into(), v);
    };
    if let Some(d) = &e.day_cycle {
        put("day_length", Dynamic::from_float(f64::from(d.length_seconds)));
        put("time_of_day", Dynamic::from_float(d.time_at(tick)));
    }
    for (k, v) in [("sun_azimuth", e.sun_azimuth), ("sun_elevation", e.sun_elevation)]
        .into_iter()
        .chain([
            ("visible_distance", e.visible_distance),
            ("fog_distance", e.fog_distance),
        ])
    {
        if let Some(v) = v {
            put(k, Dynamic::from_float(f64::from(v)));
        }
    }
    for (k, c) in [
        ("direct_light", e.direct_light),
        ("ambient_light", e.ambient_light),
        ("shadow_color", e.shadow_color),
        ("fog_color", e.fog_color),
        ("sky_color", e.sky_color),
    ] {
        if let Some(c) = c {
            put(k, color_value(&c));
        }
    }
    if let Some(f) = &e.sun_flare {
        put("sun_flare_color", color_value(&f.color));
        put("sun_flare_size", Dynamic::from_float(f64::from(f.size)));
    }
    if let Some(v) = &e.vignette {
        put("vignette_color", color_value(&v.color));
        put("vignette_multiply", v.multiply.into());
    }
    Dynamic::from_map(m)
}
/// `set_environment(#{ ... })`: each key sets one setting, `()` puts it
/// back to the map's own (see docs/modding/README.md, "Environment").
fn set_environment(options: Map) -> Fallible<()> {
    use bri_content::atmosphere::{DEFAULT_DAY_LENGTH, DayCycle, Settings, SunFlare, Vignette};
    let (current, tick) = with(|i| Ok((i.snapshot.environment.clone(), i.snapshot.tick)))?;
    let mut changes = Settings::default();
    let mut unset = Vec::new();
    let mut day_length = None;
    let mut time_of_day = None;
    let mut day_cycle_off = false;
    let mut flare = current.sun_flare;
    let mut flare_set = false;
    let mut vignette = current.vignette;
    let mut vignette_set = false;
    let mut remove = |k: &str| unset.push(k.to_owned());
    for (key, value) in options {
        let clear = value.is_unit();
        match key.as_str() {
            "day_length" if clear => day_cycle_off = true,
            "day_length" => day_length = Some(float(&value)?),
            "time_of_day" if !clear => time_of_day = Some(float(&value)?),
            "time_of_day" => {}
            "day_cycle" => {
                if !value.as_bool().map_err(|_| "day_cycle is true or false")? {
                    day_cycle_off = true;
                } else if current.day_cycle.is_none() {
                    day_length.get_or_insert(DEFAULT_DAY_LENGTH);
                }
            }
            "sun_azimuth" | "sun_elevation" | "visible_distance" | "fog_distance" if clear => {
                remove(key.as_str())
            }
            "sun_azimuth" => changes.sun_azimuth = Some(float(&value)?),
            "sun_elevation" => changes.sun_elevation = Some(float(&value)?),
            "visible_distance" => changes.visible_distance = Some(float(&value)?),
            "fog_distance" => changes.fog_distance = Some(float(&value)?),
            "direct_light" | "ambient_light" | "shadow_color" | "fog_color" | "sky_color"
                if clear =>
            {
                remove(key.as_str())
            }
            "direct_light" => changes.direct_light = Some(color(value, "direct_light")?),
            "ambient_light" => changes.ambient_light = Some(color(value, "ambient_light")?),
            "shadow_color" => changes.shadow_color = Some(color(value, "shadow_color")?),
            "fog_color" => changes.fog_color = Some(color(value, "fog_color")?),
            "sky_color" => changes.sky_color = Some(color(value, "sky_color")?),
            "sun_flare_color" | "sun_flare_size" if clear => {
                flare = None;
                flare_set = true;
            }
            "sun_flare_color" => {
                flare.get_or_insert_with(SunFlare::default).color =
                    color(value, "sun_flare_color")?;
                flare_set = true;
            }
            "sun_flare_size" => {
                flare.get_or_insert_with(SunFlare::default).size = float(&value)?;
                flare_set = true;
            }
            "vignette_color" if clear => {
                vignette = None;
                vignette_set = true;
            }
            "vignette_color" => {
                let color = color(value, "vignette_color")?;
                vignette
                    .get_or_insert(Vignette {
                        color,
                        multiply: false,
                    })
                    .color = color;
                vignette_set = true;
            }
            "vignette_multiply" => {
                let multiply = value.as_bool().map_err(|_| "vignette_multiply is true or false")?;
                let Some(v) = &mut vignette else {
                    return fail("set vignette_color before vignette_multiply");
                };
                v.multiply = multiply;
                vignette_set = true;
            }
            other => {
                return fail(format!(
                    "set_environment has no setting `{other}` (day_cycle, day_length, time_of_day, \
                     sun_azimuth, sun_elevation, direct_light, ambient_light, shadow_color, \
                     sun_flare_color, sun_flare_size, visible_distance, fog_distance, fog_color, \
                     sky_color, vignette_color, vignette_multiply)"
                ));
            }
        }
    }
    if flare_set {
        match flare {
            Some(f) => changes.sun_flare = Some(f),
            None => unset.push("sun_flare".into()),
        }
    }
    if vignette_set {
        match vignette {
            Some(v) => changes.vignette = Some(v),
            None => unset.push("vignette".into()),
        }
    }
    if day_cycle_off {
        unset.push("day_cycle".into());
    } else if day_length.is_some() || time_of_day.is_some() {
        let running = current.day_cycle;
        let Some(length) = day_length.or(running.map(|d| d.length_seconds)) else {
            return fail("time_of_day needs a day cycle: set day_length too");
        };
        let time = time_of_day
            .or(running.map(|d| d.time_at(tick) as f32))
            .unwrap_or(0.5);
        changes.day_cycle = Some(DayCycle {
            length_seconds: length,
            time: time.rem_euclid(1.0),
            anchor_tick: 0,
        });
    }
    changes
        .validate()
        .map_err(|e| format!("set_environment: {e}"))?;
    push(Op::SetEnvironment {
        changes: Box::new(changes),
        unset,
    })
}
fn object_ref(value: &Dynamic) -> Fallible<ObjectRef> {
    let text = value.clone().into_string().map_err(|_| {
        format!(
            "expected an object like \"vehicle:3\", got {}",
            value.type_name()
        )
    })?;
    ObjectRef::parse(&text)
        .ok_or_else(|| format!("`{text}` is not an object like \"vehicle:3\"").into())
}
/// `[x, y, z]`.
fn path_knot(value: &Dynamic) -> Fallible<crate::ops::PathKnot> {
    let knot = value
        .read_lock::<rhai::Map>()
        .ok_or("a knot is #{ at, yaw, pitch, speed, type, path, jump }")?;
    let field = |key: &str| knot.get(key).cloned().unwrap_or(Dynamic::UNIT);
    let text = |key: &str, default: &str| -> Fallible<String> {
        let v = field(key);
        if v.is_unit() {
            Ok(default.to_owned())
        } else {
            v.into_string()
                .map(|s| s.to_ascii_lowercase())
                .map_err(|_| format!("a knot's {key} is a string").into())
        }
    };
    let at = field("at")
        .try_cast::<Array>()
        .ok_or("a knot's at is [x, y, z]")?;
    let number = |key: &str, default: f32| -> Fallible<f32> {
        let v = field(key);
        if v.is_unit() { Ok(default) } else { float(&v) }
    };
    Ok(crate::ops::PathKnot {
        at: vector(&at)?,
        yaw: number("yaw", 0.0)?,
        pitch: number("pitch", 0.0)?,
        speed: number("speed", 7.0)?,
        kind: match text("type", "normal")?.as_str() {
            "normal" => crate::ops::KnotKind::Normal,
            "kink" => crate::ops::KnotKind::Kink,
            "position" | "position only" => crate::ops::KnotKind::PositionOnly,
            other => return fail(format!("a knot's type is normal, kink or position, not {other}")),
        },
        linear: match text("path", "spline")?.as_str() {
            "spline" => false,
            "linear" => true,
            other => return fail(format!("a knot's path is spline or linear, not {other}")),
        },
        jump: field("jump").as_bool().unwrap_or(false),
    })
}
fn vector(value: &Array) -> Fallible<[f32; 3]> {
    match value.as_slice() {
        [x, y, z] => Ok([float(x)?, float(y)?, float(z)?]),
        _ => fail("a point or direction is [x, y, z]"),
    }
}
/// The player a set of shapes goes with, or `()` for none.
fn shape_owner(value: &Dynamic) -> Fallible<Option<u64>> {
    if value.is_unit() { Ok(None) } else { id(value).map(Some) }
}
/// A shape map: `#{min, max, color, inside, sides, label}`, colours RGBA
/// from 0 to 1 (all but `min` and `max` optional; `sides` is the outside
/// colours across x, y and z).
fn world_shape(value: Dynamic) -> Fallible<WorldShape> {
    let Some(map) = value.try_cast::<Map>() else {
        return fail("a shape is #{min, max, color, inside, sides, label}");
    };
    for key in map.keys() {
        if !["min", "max", "color", "inside", "sides", "label"].contains(&key.as_str()) {
            return fail(format!(
                "a shape has no `{key}` (min, max, color, inside, sides, label)"
            ));
        }
    }
    let point = |key: &str| -> Fallible<[f32; 3]> {
        match map.get(key).and_then(|v| v.clone().try_cast::<Array>()) {
            Some(v) => vector(&v),
            None => fail(format!("a shape's {key} is [x, y, z]")),
        }
    };
    let byte = |v: Dynamic, what: &str| -> Fallible<[u8; 4]> {
        let c = color::<4>(v, what)?;
        Ok(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
    };
    let rgba = |key: &str| -> Fallible<[u8; 4]> {
        match map.get(key).filter(|v| !v.is_unit()) {
            None => Ok([0; 4]),
            Some(v) => byte(v.clone(), &format!("a shape's {key}")),
        }
    };
    let sides = match map.get("sides").filter(|v| !v.is_unit()) {
        None => None,
        Some(v) => {
            let what = "a shape's sides are three colours, across x, y and z";
            let list = v.clone().into_typed_array::<Dynamic>().map_err(|_| what)?;
            let list = list.into_iter().map(|c| byte(c, what)).collect::<Fallible<Vec<_>>>()?;
            Some(<[[u8; 4]; 3]>::try_from(list).map_err(|_| what)?)
        }
    };
    let label = match map.get("label").filter(|v| !v.is_unit()) {
        None => String::new(),
        Some(v) => v.clone().into_string().map_err(|_| "a shape's label is text")?,
    };
    Ok(WorldShape {
        min: point("min")?,
        max: point("max")?,
        color: rgba("color")?,
        inside: rgba("inside")?,
        sides,
        label,
    })
}
/// A player by id, or an object like `"vehicle:3"`.
fn target(value: &Dynamic) -> Fallible<ObjectRef> {
    if value.is_string() {
        object_ref(value)
    } else {
        Ok(ObjectRef::Player(id(value)?))
    }
}
/// A player to credit or ignore: an id, `"player:3"`, or `()` for none.
fn player_or_none(value: &Dynamic) -> Fallible<Option<u64>> {
    match target(value) {
        _ if value.is_unit() => Ok(None),
        Ok(ObjectRef::Player(p)) => Ok(Some(p)),
        Ok(other) => fail(format!("expected a player, got {other}")),
        Err(e) => Err(e),
    }
}
fn ray_map(hit: &RayHit) -> Dynamic {
    let [x, y, z] = position(hit.position);
    let (kind, id, reference) = match hit.target {
        RayTarget::Object(o) => (
            o.kind(),
            Dynamic::from_int(o.id() as i64),
            Dynamic::from(o.to_string()),
        ),
        RayTarget::Brick(b) => ("brick", Dynamic::from_int(b as i64), Dynamic::UNIT),
        RayTarget::Map => ("map", Dynamic::UNIT, Dynamic::UNIT),
    };
    let mut entries = vec![
        ("kind", kind.into()),
        ("id", id),
        ("ref", reference),
        x,
        y,
        z,
        float_entry("nx", hit.normal[0]),
        float_entry("ny", hit.normal[1]),
        float_entry("nz", hit.normal[2]),
        float_entry("distance", hit.distance),
    ];
    if let Some(region) = hit.region {
        entries.push(("region", region.into()));
    }
    map(entries)
}
fn credit(value: &Dynamic) -> Fallible<Option<u64>> {
    if value.is_unit() {
        Ok(None)
    } else {
        Ok(Some(id(value)?))
    }
}
pub fn entity_map(e: &EntityView) -> Dynamic {
    let [x, y, z] = position(e.position);
    map([
        ("id", Dynamic::from_int(e.id as i64)),
        ("kind", e.kind.clone().into()),
        x,
        y,
        z,
        ("yaw", Dynamic::from_float(e.yaw as f64)),
        ("label", e.label.clone().into()),
        ("health", Dynamic::from_float(e.health as f64)),
        ("speed", Dynamic::from_float(e.speed as f64)),
    ])
}
fn player_key(i: &Invocation, player: &Dynamic) -> Fallible<PlayerKey> {
    let player = id(player)?;
    // Bots keep state too while they play (a mini-game's rules track
    // their members alike).
    i.snapshot
        .player(player)
        .map(|p| p.key.clone())
        .ok_or_else(|| format!("no player {player}").into())
}

fn register_api(engine: &mut Engine) {
    engine.register_fn("tick", || with(|i| Ok(i.snapshot.tick as i64)));
    engine.register_fn("seed", || with(|i| Ok(i.snapshot.seed)));
    engine.register_fn("caller", || {
        with(|i| {
            Ok(i.caller
                .map_or(Dynamic::UNIT, |c| Dynamic::from_int(c as i64)))
        })
    });
    engine.register_fn("game_version", || with(|i| Ok(i.snapshot.game_version.clone())));
    engine.register_fn("players", || {
        with(|i| Ok(i.snapshot.players.iter().map(player_map).collect::<Array>()))
    });
    engine.register_fn("player", |player: Dynamic| {
        with(|i| {
            let player = id(&player)?;
            Ok(i.snapshot.player(player).map_or(Dynamic::UNIT, player_map))
        })
    });
    engine.register_fn("bots", || {
        with(|i| Ok(i.snapshot.bots.iter().map(player_map).collect::<Array>()))
    });
    engine.register_fn("entities", || {
        with(|i| {
            Ok(i.snapshot
                .entities
                .iter()
                .map(entity_map)
                .collect::<Array>())
        })
    });
    engine.register_fn("me", || {
        with(|i| {
            let Some(me) = i.entity else {
                return Ok(Dynamic::UNIT);
            };
            Ok(i.snapshot
                .entities
                .iter()
                .find(|e| e.id == me)
                .map_or(Dynamic::UNIT, entity_map))
        })
    });
    engine.register_fn("aim", || {
        with(|i| {
            Ok(i.aim.as_ref().map_or(Dynamic::UNIT, |a| {
                let [x, y, z] = position(a.position);
                map([
                    (
                        "brick",
                        a.brick
                            .map_or(Dynamic::UNIT, |b| Dynamic::from_int(b as i64)),
                    ),
                    ("tag", a.tag.clone().map_or(Dynamic::UNIT, Dynamic::from)),
                    (
                        "block",
                        a.look
                            .as_ref()
                            .map_or(Dynamic::UNIT, |(b, _)| Dynamic::from(b.clone())),
                    ),
                    (
                        "state",
                        a.look
                            .as_ref()
                            .map_or(Dynamic::UNIT, |(_, s)| Dynamic::from(s.clone())),
                    ),
                    x,
                    y,
                    z,
                    float_entry("nx", a.normal[0]),
                    float_entry("ny", a.normal[1]),
                    float_entry("nz", a.normal[2]),
                    ("distance", Dynamic::from_float(a.distance as f64)),
                    (
                        "object",
                        a.object
                            .as_ref()
                            .map_or(Dynamic::UNIT, |o| Dynamic::from(o.object.to_string())),
                    ),
                    (
                        "movable",
                        a.object.as_ref().is_some_and(|o| o.movable).into(),
                    ),
                    (
                        "object_distance",
                        a.object
                            .as_ref()
                            .map_or(Dynamic::UNIT, |o| Dynamic::from_float(o.distance as f64)),
                    ),
                ])
            }))
        })
    });
    // Package state: global keys, then per-player keys.
    engine.register_fn("get", |key: &str| {
        with(|i| Ok(i.state.global.get(key).map_or(Dynamic::UNIT, to_dynamic)))
    });
    engine.register_fn("set", |key: &str, value: Dynamic| {
        with(|i| {
            i.state.global.insert(key.into(), to_json(&value)?);
            Ok(())
        })
    });
    engine.register_fn("get_player", |player: Dynamic, key: &str| {
        with(|i| {
            let k = player_key(i, &player)?;
            Ok(i.state
                .players
                .get(&k)
                .and_then(|m| m.get(key))
                .map_or(Dynamic::UNIT, to_dynamic))
        })
    });
    engine.register_fn(
        "set_player",
        |player: Dynamic, key: &str, value: Dynamic| {
            with(|i| {
                let k = player_key(i, &player)?;
                let v = to_json(&value)?;
                i.state.players.entry(k).or_default().insert(key.into(), v);
                Ok(())
            })
        },
    );
    engine.register_fn(
        "add_player",
        |player: Dynamic, key: &str, amount: Dynamic| {
            with(|i| {
                let k = player_key(i, &player)?;
                let values = i.state.players.entry(k).or_default();
                let current = values
                    .get(key)
                    .cloned()
                    .unwrap_or(serde_json::Value::from(0));
                let next = match (current.as_i64(), amount.as_int()) {
                    (Some(a), Ok(b)) => {
                        serde_json::Value::from(a.checked_add(b).ok_or("state number overflow")?)
                    }
                    _ => {
                        let sum = current.as_f64().unwrap_or(0.0) + number(&amount)?;
                        serde_json::Number::from_f64(sum)
                            .map(serde_json::Value::Number)
                            .ok_or("state number is not finite")?
                    }
                };
                values.insert(key.into(), next.clone());
                Ok(to_dynamic(&next))
            })
        },
    );
    // Package-local entity variables.
    engine.register_fn("entity_get", |entity: Dynamic, key: &str| {
        with(|i| {
            let e = id(&entity)?;
            Ok(i.written
                .get(&e)
                .or_else(|| i.entity_vars.get(&e))
                .and_then(|m| m.get(key))
                .map_or(Dynamic::UNIT, to_dynamic))
        })
    });
    engine.register_fn(
        "entity_set",
        |entity: Dynamic, key: &str, value: Dynamic| {
            with(|i| {
                let e = id(&entity)?;
                if !i.written.contains_key(&e) {
                    let Some(vars) = i.entity_vars.get(&e) else {
                        return fail(format!("entity {e} does not belong to this package"));
                    };
                    let vars = vars.clone();
                    i.written.insert(e, vars);
                }
                let v = to_json(&value)?;
                i.written
                    .get_mut(&e)
                    .expect("inserted above")
                    .insert(key.into(), v);
                Ok(())
            })
        },
    );
    // Pure helpers.
    engine.register_fn("noise", |seed: i64, x: Dynamic, z: Dynamic| {
        Ok::<_, Box<EvalAltResult>>(crate::noise::value2(seed, number(&x)?, number(&z)?))
    });
    engine.register_fn("hash3", |seed: i64, x: i64, y: i64, z: i64| {
        crate::noise::hash3(seed, x, y, z)
    });
    // Operations.
    engine.register_fn("remove_brick", |brick: Dynamic| {
        push(Op::RemoveBrick { brick: id(&brick)? })
    });
    engine.register_fn(
        "place_voxel",
        |x: i64, y: i64, z: i64, material: &str| {
            push(Op::PlaceVoxel {
                position: [x, y, z],
                material: material.into(),
            })
        },
    );
    // A brick into build `owner` (a brick's owner): plant_brick(kind,
    // [x, y, z], turns, color, owner).
    engine.register_fn(
        "plant_brick",
        |kind: &str, position: Array, turns: i64, color: i64, owner: i64| {
            let position = vector(&position)?;
            let Ok(color) = u8::try_from(color) else {
                return fail("plant_brick's colour is a palette index, 0 to 255");
            };
            push(Op::PlantBrick {
                kind: kind.into(),
                position,
                turns: u8::try_from(turns.rem_euclid(4)).expect("0..4"),
                color,
                owner: u64::try_from(owner).or_else(|_| fail("an owner is 0 or more"))?,
            })
        },
    );
    engine.register_fn(
        "place_brick",
        |shape: &str, x: Dynamic, y: Dynamic, z: Dynamic, r: Dynamic, g: Dynamic, b: Dynamic| {
            push(Op::PlaceBrick {
                shape: shape.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                color: [float(&r)?, float(&g)?, float(&b)?, 1.0],
            })
        },
    );
    engine.register_fn(
        "explode",
        |x: Dynamic,
         y: Dynamic,
         z: Dynamic,
         radius: Dynamic,
         damage: Dynamic,
         brick_radius: Dynamic| {
            push(Op::Explode {
                position: [float(&x)?, float(&y)?, float(&z)?],
                radius: float(&radius)?,
                damage: float(&damage)?,
                brick_radius: float(&brick_radius)?,
            })
        },
    );
    engine.register_fn(
        "fire",
        |projectile: &str,
         x: Dynamic,
         y: Dynamic,
         z: Dynamic,
         vx: Dynamic,
         vy: Dynamic,
         vz: Dynamic| { fire_op(projectile, [x, y, z], [vx, vy, vz], Dynamic::UNIT) },
    );
    engine.register_fn(
        "fire",
        |projectile: &str,
         x: Dynamic,
         y: Dynamic,
         z: Dynamic,
         vx: Dynamic,
         vy: Dynamic,
         vz: Dynamic,
         by: Dynamic| { fire_op(projectile, [x, y, z], [vx, vy, vz], by) },
    );
    engine.register_fn("damage", |target: Dynamic, amount: Dynamic| {
        damage_op(&target, &amount, &Dynamic::UNIT, None)
    });
    // `()` credits nobody, as `on_death` passes `()` for no killer.
    engine.register_fn("damage", |target: Dynamic, amount: Dynamic, by: Dynamic| {
        damage_op(&target, &amount, &by, None)
    });
    engine.register_fn(
        "damage",
        |target: Dynamic, amount: Dynamic, by: Dynamic, damage_type: &str| {
            damage_op(&target, &amount, &by, Some(damage_type.into()))
        },
    );
    engine.register_fn(
        "teleport",
        |player: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic| {
            push(Op::Teleport {
                player: id(&player)?,
                position: [float(&x)?, float(&y)?, float(&z)?],
            })
        },
    );
    engine.register_fn("respawn", |player: Dynamic| {
        push(Op::Respawn {
            player: id(&player)?,
        })
    });
    engine.register_fn("set_archetype", |player: Dynamic, archetype: &str| {
        push(Op::SetArchetype {
            player: id(&player)?,
            archetype: archetype.into(),
        })
    });
    engine.register_fn("set_block_state", |brick: Dynamic, state: &str| {
        push(Op::SetBlockState {
            brick: id(&brick)?,
            state: state.into(),
        })
    });
    engine.register_fn("control", |player: Dynamic, entity: Dynamic| {
        push(Op::Control {
            player: id(&player)?,
            entity: Some(id(&entity)?),
        })
    });
    engine.register_fn("release", |player: Dynamic| {
        push(Op::Control {
            player: id(&player)?,
            entity: None,
        })
    });
    engine.register_fn(
        "spawn_entity",
        |kind: &str, x: Dynamic, y: Dynamic, z: Dynamic| {
            push(Op::SpawnEntity {
                kind: kind.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                vars: BTreeMap::new(),
            })
        },
    );
    engine.register_fn(
        "spawn_entity",
        |kind: &str, x: Dynamic, y: Dynamic, z: Dynamic, vars: Map| {
            let vars = vars
                .into_iter()
                .map(|(k, v)| Ok((k.to_string(), to_json(&v)?)))
                .collect::<Fallible<_>>()?;
            push(Op::SpawnEntity {
                kind: kind.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                vars,
            })
        },
    );
    engine.register_fn("remove_entity", |entity: Dynamic| {
        push(Op::RemoveEntity {
            entity: id(&entity)?,
        })
    });
    engine.register_fn(
        "steer",
        |entity: Dynamic, dx: Dynamic, dz: Dynamic, jump: bool| {
            push(Op::Steer {
                entity: id(&entity)?,
                direction: [float(&dx)?, float(&dz)?],
                jump,
            })
        },
    );
    engine.register_fn("label", |entity: Dynamic, label: &str| {
        push(Op::Label {
            entity: id(&entity)?,
            label: label.into(),
        })
    });
    engine.register_fn("tell", |player: Dynamic, text: &str| {
        push(Op::Tell {
            player: id(&player)?,
            text: text.into(),
        })
    });
    engine.register_fn("broadcast", |text: &str| {
        push(Op::Broadcast { text: text.into() })
    });
    // copy_build(player, brick, limit, "up" | "down", tool[, options]),
    // copy_box(player, min, max, limit, tool[, options]); options are
    // `copy_rule`'s.
    fn copy_build(
        player: Dynamic,
        brick: Dynamic,
        limit: i64,
        reach: &str,
        tool: &str,
        options: Map,
    ) -> Result<(), Box<EvalAltResult>> {
        let (rule, limited, hold) = copy_rule(&options)?;
        let limited = limited.unwrap_or(false);
        push(Op::CopyBuild {
            player: id(&player)?,
            brick: id(&brick)?,
            limit: u32::try_from(limit).map_err(|_| "limit must be 1 to 1000000")?,
            reach: crate::ops::StackReach {
                up: match reach {
                    "up" => true,
                    "down" => false,
                    _ => return Err("a stack goes \"up\" or \"down\"".into()),
                },
                limited,
            },
            rule,
            tool: tool.into(),
            hold,
        })
    }
    engine.register_fn(
        "copy_build",
        |player: Dynamic, brick: Dynamic, limit: i64, reach: &str, tool: &str| {
            copy_build(player, brick, limit, reach, tool, Map::new())
        },
    );
    engine.register_fn("copy_build", copy_build);
    fn copy_box(
        player: Dynamic,
        min: Array,
        max: Array,
        limit: i64,
        tool: &str,
        options: Map,
    ) -> Result<(), Box<EvalAltResult>> {
        let (rule, limited, hold) = copy_rule(&options)?;
        push(Op::CopyBox {
            player: id(&player)?,
            min: vector(&min)?,
            max: vector(&max)?,
            // A box takes what lies wholly inside it unless told otherwise.
            limited: limited.unwrap_or(true),
            limit: u32::try_from(limit).map_err(|_| "limit must be 1 to 1000000")?,
            rule,
            tool: tool.into(),
            hold,
        })
    }
    engine.register_fn(
        "copy_box",
        |player: Dynamic, min: Array, max: Array, limit: i64, tool: &str| {
            copy_box(player, min, max, limit, tool, Map::new())
        },
    );
    engine.register_fn("copy_box", copy_box);
    engine.register_fn("copy_name", |typed: &str| -> Dynamic {
        crate::ops::copy_name(typed).map_or(Dynamic::UNIT, Dynamic::from)
    });
    fn saved_name(name: &str) -> Result<String, Box<EvalAltResult>> {
        crate::ops::copy_name(name)
            .filter(|n| n == name)
            .ok_or_else(|| format!("`{name}` is not a copy name (see copy_name)").into())
    }
    engine.register_fn("save_copy", |player: Dynamic, name: &str| {
        push(Op::SaveCopy {
            player: id(&player)?,
            name: saved_name(name)?,
            overwrite: true,
        })
    });
    // save_copy(player, name, #{ overwrite: false }): keep one saved
    // before, and hear `exists`.
    engine.register_fn("save_copy", |player: Dynamic, name: &str, options: Map| {
        let mut overwrite = true;
        for (key, value) in &options {
            match key.as_str() {
                "overwrite" => {
                    overwrite = value
                        .as_bool()
                        .map_err(|_| "save option `overwrite` is true or false")?
                }
                other => return Err(format!("unknown save option `{other}`").into()),
            }
        }
        push(Op::SaveCopy {
            player: id(&player)?,
            name: saved_name(name)?,
            overwrite,
        })
    });
    engine.register_fn("list_copies", |player: Dynamic, filter: &str| {
        let filter = filter.trim();
        if !filter.is_empty() {
            saved_name(filter)?;
        }
        push(Op::ListCopies {
            player: id(&player)?,
            filter: filter.into(),
        })
    });
    fn load_copy(
        player: Dynamic,
        name: &str,
        limit: i64,
        tool: &str,
        options: Map,
    ) -> Result<(), Box<EvalAltResult>> {
        let mut partial = false;
        let mut whole = false;
        for (key, value) in &options {
            let flag = || {
                value
                    .as_bool()
                    .map_err(|_| format!("load option `{key}` is true or false"))
            };
            match key.as_str() {
                "partial" => partial = flag()?,
                "whole" => whole = flag()?,
                other => return Err(format!("unknown load option `{other}`").into()),
            }
        }
        push(Op::LoadCopy {
            player: id(&player)?,
            name: saved_name(name)?,
            limit: u32::try_from(limit).map_err(|_| "limit must be 1 to 1000000")?,
            tool: tool.into(),
            partial,
            whole,
        })
    }
    engine.register_fn(
        "load_copy",
        |player: Dynamic, name: &str, limit: i64, tool: &str| {
            load_copy(player, name, limit, tool, Map::new())
        },
    );
    engine.register_fn("load_copy", load_copy);
    engine.register_fn(
        "highlight_copy",
        |player: Dynamic, color: Dynamic, seconds: Dynamic| {
            // `()` keeps each brick's own colour and only makes it glow.
            let color = if color.is_unit() {
                None
            } else {
                let color: Array = color
                    .try_cast()
                    .ok_or("highlight_copy's colour is [r, g, b(, a)] or ()")?;
                let rgb = vector(&color[..color.len().min(3)].to_vec())?;
                let alpha = match color.get(3) {
                    Some(a) => float(a)?,
                    None => 1.0,
                };
                Some([rgb[0], rgb[1], rgb[2], alpha])
            };
            push(Op::HighlightCopy {
                player: id(&player)?,
                color,
                seconds: float(&seconds)?,
            })
        },
    );
    engine.register_fn("mirror_copy", |player: Dynamic, axis: &str| {
        push(Op::MirrorCopy {
            player: id(&player)?,
            axis: crate::ops::MirrorAxis::parse(axis)
                .ok_or("mirror_copy's axis is \"x\", \"z\", \"view\" or \"y\"")?,
        })
    });
    // mirror_ghost(player, axis, asymmetric): the player's ghost brick
    // mirrored where it stands; `asymmetric` is what they are told when it
    // has no exact mirror image.
    engine.register_fn("mirror_ghost", |player: Dynamic, axis: &str, asymmetric: &str| {
        push(Op::MirrorGhost {
            player: id(&player)?,
            axis: crate::ops::MirrorAxis::parse(axis)
                .ok_or("mirror_ghost's axis is \"x\", \"z\", \"view\" or \"y\"")?,
            asymmetric: asymmetric.into(),
        })
    });
    engine.register_fn(
        "move_copy",
        |player: Dynamic, point: Array, normal: Array| {
            push(Op::MoveCopy {
                player: id(&player)?,
                point: vector(&point)?,
                normal: vector(&normal)?,
            })
        },
    );
    engine.register_fn("drop_copy", |player: Dynamic| {
        push(Op::DropCopy {
            player: id(&player)?,
        })
    });
    for (name, show) in [("show_copy", true), ("hide_copy", false)] {
        engine.register_fn(name, move |player: Dynamic| {
            let player = id(&player)?;
            push(if show {
                Op::ShowCopy { player }
            } else {
                Op::HideCopy { player }
            })
        });
    }
    engine.register_fn(
        "shift_copy",
        |player: Dynamic, x: i64, y: i64, z: i64, super_shift: bool| {
            let step = |v: i64| i32::try_from(v).map_err(|_| "a shift is a few studs or plates");
            push(Op::ShiftCopy {
                player: id(&player)?,
                offset: [step(x)?, step(y)?, step(z)?],
                super_shift,
            })
        },
    );
    engine.register_fn("rotate_copy", |player: Dynamic, direction: i64| {
        push(Op::RotateCopy {
            player: id(&player)?,
            direction: if direction < 0 { -1 } else { 1 },
        })
    });
    fn plant_copy(player: Dynamic, options: Map) -> Result<(), Box<EvalAltResult>> {
        let mut float = false;
        for (key, value) in &options {
            match key.as_str() {
                "float" => {
                    float = value
                        .as_bool()
                        .map_err(|_| "plant option `float` is true or false")?
                }
                other => return Err(format!("unknown plant option `{other}`").into()),
            }
        }
        push(Op::PlantCopy {
            player: id(&player)?,
            float,
        })
    }
    engine.register_fn("plant_copy", |player: Dynamic| plant_copy(player, Map::new()));
    engine.register_fn("plant_copy", plant_copy);
    engine.register_fn("float_copy", |player: Dynamic, float: bool| {
        push(Op::FloatCopy {
            player: id(&player)?,
            float,
            admin_only: false,
        })
    });
    // float_copy(player, float, #{ admin_only: true }): floating only
    // while the player is an administrator, checked at each plant.
    engine.register_fn("float_copy", |player: Dynamic, float: bool, options: Map| {
        let mut admin_only = false;
        for (key, value) in &options {
            match key.as_str() {
                "admin_only" => {
                    admin_only = value
                        .as_bool()
                        .map_err(|_| "float option `admin_only` is true or false")?
                }
                other => return Err(format!("unknown float option `{other}`").into()),
            }
        }
        push(Op::FloatCopy {
            player: id(&player)?,
            float,
            admin_only,
        })
    });
    engine.register_fn("wrench_copy", |player: Dynamic| {
        push(Op::WrenchCopy {
            player: id(&player)?,
        })
    });
    engine.register_fn("super_cut", |player: Dynamic, min: Array, max: Array| {
        push(Op::SuperCut {
            player: id(&player)?,
            min: vector(&min)?,
            max: vector(&max)?,
        })
    });
    engine.register_fn(
        "fill_box",
        |player: Dynamic, min: Array, max: Array, color: i64| {
            push(Op::FillBox {
                player: id(&player)?,
                min: vector(&min)?,
                max: vector(&max)?,
                color: u8::try_from(color).map_err(|_| "color is a palette index, 0 to 255")?,
            })
        },
    );
    engine.register_fn("plant_wait", |player: Dynamic, seconds: Dynamic| {
        push(Op::PlantWait {
            player: id(&player)?,
            seconds: float(&seconds)?,
        })
    });
    engine.register_fn("cancel_copy", |player: Dynamic| {
        push(Op::CancelCopy {
            player: id(&player)?,
        })
    });
    engine.register_fn("pivot_copy", |player: Dynamic, pivot: &str| {
        let whole = match pivot {
            "whole" => true,
            "start" => false,
            _ => return Err("pivot_copy is \"whole\" or \"start\"".into()),
        };
        push(Op::PivotCopy {
            player: id(&player)?,
            whole,
        })
    });
    engine.register_fn("plant_as", |player: Dynamic, target: &str, admin: bool| {
        push(Op::PlantAs {
            player: id(&player)?,
            target: target.trim().into(),
            admin,
        })
    });
    engine.register_fn("take_paint", |player: Dynamic, take: bool| {
        push(Op::TakePaint {
            player: id(&player)?,
            take,
        })
    });
    engine.register_fn("scroll_mode", |player: Dynamic, mode: &str| {
        push(Op::ScrollMode {
            player: id(&player)?,
            mode: crate::ops::ScrollMode::parse(mode)
                .ok_or("scroll_mode is \"none\", \"bricks\", \"paint\" or \"tools\"")?,
        })
    });
    engine.register_fn("cut_copy", |player: Dynamic| {
        push(Op::CutCopy {
            player: id(&player)?,
            each: false,
        })
    });
    // cut_copy(player, #{ each: true }): each brick the player may cut,
    // the rest counted for `on_copy`.
    engine.register_fn("cut_copy", |player: Dynamic, options: Map| {
        let mut each = false;
        for (key, value) in &options {
            match key.as_str() {
                "each" => {
                    each = value
                        .as_bool()
                        .map_err(|_| "cut option `each` is true or false")?
                }
                other => return Err(format!("unknown cut option `{other}`").into()),
            }
        }
        push(Op::CutCopy {
            player: id(&player)?,
            each,
        })
    });
    engine.register_fn("paint_copy", |player: Dynamic, color: i64| {
        push(Op::PaintCopy {
            player: id(&player)?,
            paint: FillPaint::Color(
                u8::try_from(color).map_err(|_| "color is a palette index, 0 to 255")?,
            ),
            each: false,
        })
    });
    // paint_copy(player, #{ color | color_effect | shape_effect: n }): each
    // brick the player may paint, the rest counted for `on_copy`.
    engine.register_fn("paint_copy", |player: Dynamic, paint: Map| {
        push(Op::PaintCopy {
            player: id(&player)?,
            paint: fill_paint("paint_copy", &paint)?,
            each: true,
        })
    });
    // paint_fill(player, brick, paint, options): paint is #{ color: n },
    // #{ color_effect: n } or #{ shape_effect: n }; options holds limit
    // (required) and may hold reach: [sideways, vertical], stop_at_limit
    // limit_message: [text, seconds], limit_error (the plant-limit error
    // when the limit stops it) and refusal_seconds.
    engine.register_fn(
        "paint_fill",
        |player: Dynamic, brick: Dynamic, paint: Map, options: Map| {
            let paint = fill_paint("paint_fill", &paint)?;
            let (mut limit, mut reach, mut stop_at_limit, mut limit_message) =
                (None, None, false, None);
            let (mut refusal_seconds, mut limit_error) = (None, false);
            for (key, value) in options {
                match key.as_str() {
                    "limit" => {
                        limit = Some(
                            value
                                .as_int()
                                .ok()
                                .and_then(|l| u32::try_from(l).ok())
                                .ok_or("limit is a count of bricks")?,
                        )
                    }
                    "reach" => {
                        let r = value.try_cast::<Array>().ok_or("reach is [sideways, vertical]")?;
                        let [side, up] = r.as_slice() else {
                            return fail("reach is [sideways, vertical]");
                        };
                        reach = Some([float(side)?, float(up)?]);
                    }
                    "stop_at_limit" => {
                        stop_at_limit = value.as_bool().map_err(|_| "stop_at_limit is true or false")?
                    }
                    "limit_message" => {
                        let m = value.try_cast::<Array>().ok_or("limit_message is [text, seconds]")?;
                        let [text, seconds] = m.as_slice() else {
                            return fail("limit_message is [text, seconds]");
                        };
                        limit_message = Some((text.to_string(), float(seconds)?));
                    }
                    "limit_error" => {
                        limit_error = value.as_bool().map_err(|_| "limit_error is true or false")?
                    }
                    "refusal_seconds" => refusal_seconds = Some(float(&value)?),
                    other => {
                        return fail(format!(
                            "paint_fill has no option `{other}` (limit, reach, stop_at_limit, limit_message, limit_error, refusal_seconds)"
                        ));
                    }
                }
            }
            push(Op::PaintFill {
                player: id(&player)?,
                brick: id(&brick)?,
                paint,
                limit: limit.ok_or("paint_fill needs a limit")?,
                reach,
                stop_at_limit,
                limit_message,
                refusal_seconds,
                limit_error,
            })
        },
    );
    // paint_vehicle(player, vehicle, paint, options): paint is #{ color: n }
    // (a palette colour) or #{ rgb: [r, g, b] }; options may hold
    // riders_seconds and refusal_seconds.
    engine.register_fn(
        "paint_vehicle",
        |player: Dynamic, vehicle: Dynamic, paint: Map, options: Map| {
            let vehicle = match object_ref(&vehicle) {
                Ok(ObjectRef::Vehicle(v)) => v,
                Ok(other) => return fail(format!("{other} is not a vehicle")),
                Err(_) => id(&vehicle)?,
            };
            let mut chosen = None;
            for (key, value) in paint {
                let p = match key.as_str() {
                    "color" => VehiclePaint::Color(
                        value
                            .as_int()
                            .ok()
                            .and_then(|i| u8::try_from(i).ok())
                            .ok_or("color is a palette index, 0 to 255")?,
                    ),
                    "rgb" => VehiclePaint::Rgb(color(value, "rgb")?),
                    other => {
                        return fail(format!("paint_vehicle paints color or rgb, not `{other}`"));
                    }
                };
                if chosen.replace(p).is_some() {
                    return fail("paint_vehicle paints one of color or rgb");
                }
            }
            let paint = chosen.ok_or("paint_vehicle needs #{ color: n } or #{ rgb: [r, g, b] }")?;
            let (mut riders_seconds, mut refusal_seconds) = (None, None);
            for (key, value) in options {
                match key.as_str() {
                    "riders_seconds" => riders_seconds = Some(float(&value)?),
                    "refusal_seconds" => refusal_seconds = Some(float(&value)?),
                    other => {
                        return fail(format!(
                            "paint_vehicle has no option `{other}` (riders_seconds, refusal_seconds)"
                        ));
                    }
                }
            }
            push(Op::PaintVehicle {
                player: id(&player)?,
                vehicle,
                paint,
                riders_seconds,
                refusal_seconds,
            })
        },
    );
    engine.register_fn(
        "show_box",
        |player: Dynamic, min: Array, max: Array, tool: &str| {
            push(Op::ShowBox {
                player: id(&player)?,
                area: Some((vector(&min)?, vector(&max)?)),
                tool: tool.into(),
            })
        },
    );
    engine.register_fn("hide_box", |player: Dynamic| {
        push(Op::ShowBox {
            player: id(&player)?,
            area: None,
            tool: String::new(),
        })
    });
    engine.register_fn(
        "show_shapes",
        |owner: Dynamic, key: &str, shapes: Array| {
            push(Op::ShowShapes {
                owner: shape_owner(&owner)?,
                key: key.into(),
                shapes: shapes.into_iter().map(world_shape).collect::<Fallible<_>>()?,
            })
        },
    );
    engine.register_fn("hide_shapes", |owner: Dynamic, key: &str| {
        push(Op::ShowShapes {
            owner: shape_owner(&owner)?,
            key: key.into(),
            shapes: Vec::new(),
        })
    });
    engine.register_fn("give_item", |player: Dynamic, item: &str, equip: bool| {
        push(Op::GiveItem {
            player: id(&player)?,
            item: item.into(),
            equip,
        })
    });
    // Every tool slot at once (`forceEquip`): an item id or () per slot.
    engine.register_fn("set_tools", |player: Dynamic, tools: Array| {
        let tools = tools
            .into_iter()
            .map(|t| {
                if t.is_unit() || t.to_string().is_empty() {
                    Ok(None)
                } else {
                    t.into_string().map(Some).map_err(|_| "a tool is an item id or ()")
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        push(Op::SetTools {
            player: id(&player)?,
            tools,
        })
    });
    engine.register_fn("take_item", |player: Dynamic, item: &str| {
        push(Op::TakeItem {
            player: id(&player)?,
            item: item.into(),
        })
    });
    engine.register_fn(
        "drop_item",
        |item: &str, x: Dynamic, y: Dynamic, z: Dynamic| {
            push(Op::DropItem {
                item: item.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                velocity: [0.0; 3],
                paint: None,
                data: None,
                seconds: None,
            })
        },
    );
    engine.register_fn(
        "drop_item",
        |item: &str, x: Dynamic, y: Dynamic, z: Dynamic, vx: Dynamic, vy: Dynamic, vz: Dynamic| {
            push(Op::DropItem {
                item: item.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                velocity: [float(&vx)?, float(&vy)?, float(&vz)?],
                paint: None,
                data: None,
                seconds: None,
            })
        },
    );
    engine.register_fn("heal", |player: Dynamic, amount: Dynamic| {
        push(Op::Heal {
            player: id(&player)?,
            amount: float(&amount)?,
        })
    });
    // `()` as the player prints to everyone.
    for (name, bottom) in [("center_print", false), ("bottom_print", true)] {
        engine.register_fn(
            name,
            move |player: Dynamic, text: &str, seconds: Dynamic| {
                push(Op::Print {
                    player: if player.is_unit() {
                        None
                    } else {
                        Some(id(&player)?)
                    },
                    text: text.into(),
                    seconds: float(&seconds)?,
                    bottom,
                    hide_bar: false,
                })
            },
        );
    }
    engine.register_fn(
        "bottom_print",
        |player: Dynamic, text: &str, seconds: Dynamic, hide_bar: bool| {
            push(Op::Print {
                player: if player.is_unit() {
                    None
                } else {
                    Some(id(&player)?)
                },
                text: text.into(),
                seconds: float(&seconds)?,
                bottom: true,
                hide_bar,
            })
        },
    );
    // Every member of a mini-game, counted once.
    engine.register_fn("tell_minigame", |game: Dynamic, text: &str| {
        push(Op::TellMinigame {
            game: id(&game)?,
            text: text.into(),
            except: None,
        })
    });
    engine.register_fn(
        "tell_minigame",
        |game: Dynamic, text: &str, except: Dynamic| {
            push(Op::TellMinigame {
                game: id(&game)?,
                text: text.into(),
                except: Some(id(&except)?),
            })
        },
    );
    for (name, bottom) in [
        ("center_print_minigame", false),
        ("bottom_print_minigame", true),
    ] {
        engine.register_fn(
            name,
            move |game: Dynamic, text: &str, seconds: Dynamic| {
                push(Op::PrintMinigame {
                    game: id(&game)?,
                    text: text.into(),
                    seconds: float(&seconds)?,
                    bottom,
                })
            },
        );
    }
    engine.register_fn("message_box", |player: Dynamic, title: &str, text: &str| {
        push(Op::MessageBox {
            player: id(&player)?,
            title: title.into(),
            text: text.into(),
        })
    });
    engine.register_fn(
        "ask",
        |player: Dynamic, title: &str, text: &str, command: &str| {
            push(Op::Ask {
                player: id(&player)?,
                title: title.into(),
                text: text.into(),
                command: command.into(),
            })
        },
    );
    engine.register_fn("plant_error", |player: Dynamic, error: &str| {
        push(Op::PlantError {
            player: id(&player)?,
            error: error.to_ascii_lowercase(),
        })
    });
    engine.register_fn("play_sound", |player: Dynamic, profile: &str| {
        push(Op::Sound {
            profile: profile.into(),
            at: SoundAt::Player(id(&player)?),
        })
    });
    engine.register_fn(
        "sound_at",
        |profile: &str, x: Dynamic, y: Dynamic, z: Dynamic| {
            push(Op::Sound {
                profile: profile.into(),
                at: SoundAt::Position([float(&x)?, float(&y)?, float(&z)?]),
            })
        },
    );
    register_physics(engine);
    register_queries(engine);
    register_presentation(engine);
    games::register(engine);
    bots::register(engine);
}

/// A copy's options map: `trust` ("build" or "full"), `public_bricks`, `admin`,
/// `partial` (bools, see [`crate::ops::CopyRule`]) and `limited`: a stack
/// keeps to its side of the clicked brick (default false); a box takes only
/// what lies wholly inside it (default true). Unnamed options keep their
/// defaults.
type CopyOptions = (crate::ops::CopyRule, Option<bool>, crate::ops::CopyHold);
fn copy_rule(options: &Map) -> Result<CopyOptions, Box<EvalAltResult>> {
    let mut rule = crate::ops::CopyRule::default();
    let mut limited = None;
    let mut hold = crate::ops::CopyHold::default();
    for (key, value) in options {
        let flag = || {
            value
                .as_bool()
                .map_err(|_| format!("copy option `{key}` is true or false"))
        };
        match key.as_str() {
            "trust" => {
                rule.trust = match value.clone().into_string().as_deref() {
                    Ok("build") => crate::ops::CopyTrust::Build,
                    Ok("full") => crate::ops::CopyTrust::Full,
                    _ => return Err("copy option `trust` is \"build\" or \"full\"".into()),
                }
            }
            "public_bricks" => rule.public = flag()?,
            "admin" => rule.admin = flag()?,
            "partial" => rule.partial = flag()?,
            "stack" => rule.stack = flag()?,
            "limited" => limited = Some(flag()?),
            "hidden" => hold.hidden = flag()?,
            "add" => hold.add = flag()?,
            other => return Err(format!("unknown copy option `{other}`").into()),
        }
    }
    Ok((rule, limited, hold))
}

fn damage_op(
    target_value: &Dynamic,
    amount: &Dynamic,
    by: &Dynamic,
    damage_type: Option<String>,
) -> Fallible<()> {
    push(Op::Damage {
        target: target(target_value)?,
        amount: float(amount)?,
        by: player_or_none(by)?,
        damage_type,
    })
}

/// Questions for the live world: rays and the damage rules.
fn register_queries(engine: &mut Engine) {
    fn raycast(from: Array, direction: Array, range: Dynamic, ignore: Dynamic) -> Fallible<Dynamic> {
        let from = vector(&from)?;
        let direction = vector(&direction)?;
        let range = float(&range)?;
        let ignore = player_or_none(&ignore)?;
        let length = (direction[0].powi(2) + direction[1].powi(2) + direction[2].powi(2)).sqrt();
        if !length.is_finite() || length <= 1e-6 {
            return fail("a ray's direction cannot be zero");
        }
        if range <= 0.0 || range > crate::ops::MAX_RAY_RANGE {
            return fail(format!(
                "a ray reaches 0 to {} units",
                crate::ops::MAX_RAY_RANGE
            ));
        }
        if from.iter().any(|c| c.abs() > 1_000_000.0) {
            return fail("a ray starts inside the world's bounds");
        }
        let direction = direction.map(|c| c / length);
        with_world(|world, i| {
            if i.rays >= crate::ops::MAX_RAYS_PER_CALL {
                return fail(format!(
                    "more than {} rays in one call",
                    crate::ops::MAX_RAYS_PER_CALL
                ));
            }
            i.rays += 1;
            Ok(world
                .raycast(from, direction, range, ignore)
                .map_or(Dynamic::UNIT, |hit| ray_map(&hit)))
        })
    }
    engine.register_fn(
        "raycast",
        |from: Array, direction: Array, range: Dynamic| {
            raycast(from, direction, range, Dynamic::UNIT)
        },
    );
    engine.register_fn("raycast", raycast);
    engine.register_fn("can_damage", |by: Dynamic, target_value: Dynamic| {
        let Some(by) = player_or_none(&by)? else {
            return fail("can_damage asks about a player");
        };
        let target = target(&target_value)?;
        with_world(|world, _| Ok(world.can_damage(by, target)))
    });
    // The generated world's voxel a brick is, #{ x, y, z, material } in
    // voxel coordinates, or () for any other brick.
    engine.register_fn("voxel", |brick: Dynamic| {
        let brick = id(&brick)?;
        with_world(|world, _| {
            Ok(world
                .voxel(brick)
                .map_or(Dynamic::UNIT, |([x, y, z], material)| {
                    map([
                        ("x", Dynamic::from_int(x)),
                        ("y", Dynamic::from_int(y)),
                        ("z", Dynamic::from_int(z)),
                        ("material", material.into()),
                    ])
                }))
        })
    });
    engine.register_fn("can_place_voxel", |x: i64, y: i64, z: i64| {
        with_world(|world, _| Ok(world.can_place_voxel([x, y, z])))
    });
    // The part of a player a hit at a point strikes, "head", "torso" or
    // "legs" (`getDamageLocation`), or () for no living player.
    engine.register_fn(
        "hit_region",
        |player: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic| {
            let player = id(&player)?;
            let point = [float(&x)?, float(&y)?, float(&z)?];
            with_world(|world, _| {
                Ok(world
                    .hit_region(player, point)
                    .map_or(Dynamic::UNIT, Dynamic::from))
            })
        },
    );
    // The bricks overlapping a box, as ids: up to 1024.
    engine.register_fn("bricks_in", |min: Array, max: Array| {
        let min = vector(&min)?;
        let max = vector(&max)?;
        if (0..3).any(|a| max[a] < min[a] || max[a] - min[a] > crate::ops::MAX_BOX_SPAN) {
            return fail("bricks_in needs a box no more than 256 units a side, min before max");
        }
        with_world(|world, _| {
            Ok(Dynamic::from_array(
                world
                    .bricks_in(min, max, MAX_BRICKS_IN)
                    .into_iter()
                    .map(|b| Dynamic::from_int(b as i64))
                    .collect(),
            ))
        })
    });
    // Whether this call's player may change a brick: their own trust, or
    // inside a minigame a build it plays with.
    engine.register_fn("can_edit", |brick: Dynamic| {
        let brick = id(&brick)?;
        with_world(|world, i| Ok(world.can_edit(i.caller, brick)))
    });
    // may_copy(player, brick, options): whether a copy by the player with
    // these copy options (`copy_build`'s) would take the brick: the New
    // Duplicator's `ndTrustCheckSelect` for a box corner.
    engine.register_fn("may_copy", |player: Dynamic, brick: Dynamic, options: Map| {
        let (player, brick) = (id(&player)?, id(&brick)?);
        let (rule, _, _) = copy_rule(&options)?;
        with_world(|world, _| Ok(world.may_copy(player, brick, rule)))
    });
    engine.register_fn(
        "can_plant",
        |kind: &str, position: Array, turns: i64| {
            let position = vector(&position)?;
            let turns = u8::try_from(turns.rem_euclid(4)).expect("0..4");
            with_world(|world, _| Ok(world.can_plant(kind, position, turns)))
        },
    );
    // The box a brick fills, #{ min: [x, y, z], max: [x, y, z] } in world
    // units, or () when there is no such brick.
    engine.register_fn("brick_box", |brick: Dynamic| {
        let brick = id(&brick)?;
        with_world(|world, _| {
            Ok(world.brick_box(brick).map_or(Dynamic::UNIT, |(min, max)| {
                let point = |p: [f32; 3]| {
                    Dynamic::from_array(
                        p.iter()
                            .map(|v| Dynamic::from_float(f64::from(*v)))
                            .collect(),
                    )
                };
                map([("min", point(min)), ("max", point(max))])
            }))
        })
    });
}

/// The `effects` operations, and the player view and image operations.
fn register_presentation(engine: &mut Engine) {
    fn beam(from: Array, to: Array, options: Map) -> Fallible<()> {
        let mut color = [1.0, 0.9, 0.6, 1.0];
        let mut width = 0.05;
        let mut seconds = 0.1;
        let mut muzzle = None;
        for (key, value) in options {
            match key.as_str() {
                "color" => {
                    let c = value
                        .into_typed_array::<Dynamic>()
                        .map_err(|_| "color is [r, g, b] or [r, g, b, a]")?;
                    let c = c.iter().map(float).collect::<Fallible<Vec<f32>>>()?;
                    color = match c[..] {
                        [r, g, b] => [r, g, b, 1.0],
                        [r, g, b, a] => [r, g, b, a],
                        _ => return fail("color is [r, g, b] or [r, g, b, a]"),
                    };
                }
                "width" => width = float(&value)?,
                "seconds" => seconds = float(&value)?,
                "muzzle" => muzzle = player_or_none(&value)?,
                other => {
                    return fail(format!(
                        "beam has no option `{other}` (color, width, seconds, muzzle)"
                    ));
                }
            }
        }
        push(Op::Beam {
            from: vector(&from)?,
            to: vector(&to)?,
            color,
            width,
            seconds,
            muzzle,
        })
    }
    engine.register_fn("beam", |from: Array, to: Array| beam(from, to, Map::new()));
    engine.register_fn("beam", beam);
    engine.register_fn(
        "play_thread",
        |player: Dynamic, thread: i64, sequence: &str| {
            push(Op::PlayThread {
                player: id(&player)?,
                thread: u8::try_from(thread).map_err(|_| "thread is 2 or 3")?,
                sequence: sequence.into(),
            })
        },
    );
    // Every map light within `radius` of `at`: `on` (true), `color`
    // ([1.0, 1.0, 1.0], times the recovered colour) and `brightness` (1.0);
    // an empty map puts them back as the map was lit.
    fn set_map_lights(at: Array, radius: Dynamic, options: Map) -> Fallible<()> {
        let mut on = true;
        let mut color = [1.0f32; 3];
        let mut brightness = 1.0f32;
        for (key, value) in options {
            match key.as_str() {
                "on" => on = value.as_bool().map_err(|_| "on is true or false")?,
                "color" => {
                    let c = value
                        .into_typed_array::<Dynamic>()
                        .map_err(|_| "color is [r, g, b]")?;
                    let c = c.iter().map(float).collect::<Fallible<Vec<f32>>>()?;
                    color = match c[..] {
                        [r, g, b] => [r, g, b],
                        _ => return fail("color is [r, g, b]"),
                    };
                }
                "brightness" => brightness = float(&value)?,
                other => {
                    return fail(format!(
                        "set_map_lights has no option `{other}` (on, color, brightness)"
                    ));
                }
            }
        }
        let scale = if on { brightness } else { 0.0 };
        push(Op::SetMapLights {
            position: vector(&at)?,
            radius: float(&radius)?,
            tint: color.map(|c| c * scale),
        })
    }
    engine.register_fn("set_map_lights", set_map_lights);
    // A uniform over the avatar's own colours: #{ torso: [r, g, b], ... }
    // per colour slot, or () for the player's own colours again.
    engine.register_fn("set_avatar_colors", |player: Dynamic, colors: Dynamic| {
        let mut out = BTreeMap::new();
        if !colors.is_unit() {
            let Some(colors) = colors.try_cast::<Map>() else {
                return fail("set_avatar_colors takes #{ slot: [r, g, b], ... } or ()");
            };
            for (slot, c) in colors {
                let c = c
                    .try_cast::<Array>()
                    .ok_or("a colour is [r, g, b] or [r, g, b, a]")?;
                let c = match c.as_slice() {
                    [r, g, b] => [float(r)?, float(g)?, float(b)?, 1.0],
                    [r, g, b, a] => [float(r)?, float(g)?, float(b)?, float(a)?],
                    _ => return fail("a colour is [r, g, b] or [r, g, b, a]"),
                };
                if !crate::ops::AVATAR_SLOTS.contains(&slot.as_str()) {
                    return fail(format!(
                        "`{slot}` is not an avatar colour slot ({})",
                        crate::ops::AVATAR_SLOTS.join(", ")
                    ));
                }
                out.insert(slot.to_string(), c);
            }
        }
        push(Op::SetAvatarColors {
            player: id(&player)?,
            colors: out,
        })
    });
    // A uniform's parts over the avatar's own choices: #{ hat: "copHat",
    // pack: "none", face: "smiley", decal: "AAA-None" }, or () for the
    // player's own again.
    engine.register_fn("set_avatar_parts", |player: Dynamic, look: Dynamic| {
        let (mut parts, mut face, mut decal) = (BTreeMap::new(), None, None);
        if !look.is_unit() {
            let Some(look) = look.try_cast::<Map>() else {
                return fail("set_avatar_parts takes #{ slot: part, face: name, decal: name } or ()");
            };
            for (slot, name) in look {
                let name = name.into_string().map_err(|_| "a part, face or decal is its name")?;
                match slot.as_str() {
                    "face" => face = Some(name),
                    "decal" => decal = Some(name),
                    s if crate::ops::AVATAR_PARTS.contains(&s) => {
                        parts.insert(s.to_owned(), name);
                    }
                    s => {
                        return fail(format!(
                            "`{s}` is not an avatar part slot ({}), face or decal",
                            crate::ops::AVATAR_PARTS.join(", ")
                        ));
                    }
                }
            }
        }
        push(Op::SetAvatarParts {
            player: id(&player)?,
            parts,
            face,
            decal,
        })
    });
    // temp_look(player, look, seconds): for a while every colour slot
    // #{ color: [r, g, b, a] } or palette colour #{ paint: n } (and no
    // decal), a face #{ face: "name" },
    // worn parts at an opacity #{ alpha: #{ accent: 0.7 } }.
    engine.register_fn(
        "temp_look",
        |player: Dynamic, look: Map, seconds: Dynamic| {
            let mut out = TempLook::default();
            for (key, value) in look {
                match key.as_str() {
                    "color" => {
                        let c = value
                            .try_cast::<Array>()
                            .ok_or("color is [r, g, b] or [r, g, b, a]")?;
                        out.color = Some(match c.as_slice() {
                            [r, g, b] => [float(r)?, float(g)?, float(b)?, 1.0],
                            [r, g, b, a] => [float(r)?, float(g)?, float(b)?, float(a)?],
                            _ => return fail("color is [r, g, b] or [r, g, b, a]"),
                        });
                    }
                    "paint" => {
                        out.paint = Some(
                            value
                                .as_int()
                                .ok()
                                .and_then(|i| u8::try_from(i).ok())
                                .ok_or("paint is a palette index, 0 to 255")?,
                        )
                    }
                    "face" => out.face = Some(value.to_string()),
                    "alpha" => {
                        let slots = value
                            .try_cast::<Map>()
                            .ok_or("alpha is #{ slot: opacity }")?;
                        for (slot, a) in slots {
                            if !crate::ops::AVATAR_SLOTS.contains(&slot.as_str()) {
                                return fail(format!("`{slot}` is not an avatar colour slot"));
                            }
                            out.alpha.insert(slot.to_string(), float(&a)?);
                        }
                    }
                    other => {
                        return fail(format!(
                            "temp_look has no `{other}` (color, paint, face, alpha)"
                        ));
                    }
                }
            }
            push(Op::TempLook {
                player: id(&player)?,
                look: out,
                seconds: float(&seconds)?,
            })
        },
    );
    engine.register_fn("set_environment", set_environment);
    engine.register_fn("reset_environment", || {
        push(Op::SetEnvironment {
            changes: Box::default(),
            unset: bri_content::atmosphere::KEYS.map(String::from).to_vec(),
        })
    });
    engine.register_fn("environment", || {
        with(|i| {
            let tick = i.snapshot.tick;
            Ok(environment_map(&i.snapshot.environment, tick))
        })
    });
    engine.register_fn("set_fov", |player: Dynamic, fov: Dynamic| {
        push(Op::SetFov {
            player: id(&player)?,
            fov: if fov.is_unit() {
                None
            } else {
                Some(float(&fov)?)
            },
        })
    });
    engine.register_fn("set_image_ammo", |player: Dynamic, ammo: bool| {
        push(Op::SetImageAmmo {
            player: id(&player)?,
            ammo,
        })
    });
    engine.register_fn("set_image_loaded", |player: Dynamic, loaded: bool| {
        push(Op::SetImageLoaded {
            player: id(&player)?,
            loaded,
        })
    });
    engine.register_fn("unmount_image", |player: Dynamic| {
        push(Op::UnmountImage {
            player: id(&player)?,
        })
    });
    engine.register_fn("set_scale", |player: Dynamic, scale: Dynamic| {
        push(Op::SetScale {
            player: id(&player)?,
            scale: float(&scale)?,
        })
    });
    engine.register_fn(
        "set_look_limits",
        |player: Dynamic, up: Dynamic, down: Dynamic| {
            push(Op::SetLookLimits {
                player: id(&player)?,
                limits: Some([float(&down)?, float(&up)?]),
            })
        },
    );
    // How long `player` waits to respawn, in ms, or () for their
    // mini-game's time.
    engine.register_fn("set_respawn_time", |player: Dynamic, ms: Dynamic| {
        let ms = if ms.is_unit() {
            None
        } else {
            Some(
                ms.as_int()
                    .ok()
                    .and_then(|ms| u32::try_from(ms).ok())
                    .filter(|ms| *ms <= crate::ops::MAX_RESPAWN_MS)
                    .ok_or("a respawn time is 0 to 999999 ms, or ()")?,
            )
        };
        push(Op::SetRespawnTime {
            player: id(&player)?,
            ms,
        })
    });
    engine.register_fn("hold_respawn", |player: Dynamic, held: bool| {
        push(Op::HoldRespawn {
            player: id(&player)?,
            held,
        })
    });
    // A rule's `watch`: the frozen orbit camera (`orbit_camera` below)
    // around `target` at the corpse camera's distance, which the wheel
    // does not zoom; `watch(p, ())` hands back any rules camera.
    engine.register_fn("watch", |player: Dynamic, target: Dynamic| {
        let at = crate::ops::WATCH_DISTANCE;
        push(Op::OrbitCamera {
            player: id(&player)?,
            body: crate::ops::OrbitBody::Frozen,
            orbit: Some(crate::ops::Orbit {
                target: id(&target)?,
                min: at,
                max: at,
                distance: at,
            }),
        })
    });
    engine.register_fn("watch", |player: Dynamic, _: ()| {
        push(Op::OrbitCamera {
            player: id(&player)?,
            body: crate::ops::OrbitBody::Frozen,
            orbit: None,
        })
    });
    // A camera path (`PathCamera`): `knots` is an array of
    // `#{ at: [x, y, z], yaw, pitch, speed, type, path, jump }`, `type`
    // "normal", "kink" or "position", `path` "spline" or "linear".
    engine.register_fn("follow_path", |player: Dynamic, knots: Array| {
        push(Op::FollowPath {
            player: id(&player)?,
            knots: Some(knots.iter().map(path_knot).collect::<Fallible<_>>()?),
        })
    });
    engine.register_fn("follow_path", |player: Dynamic, _: ()| {
        push(Op::FollowPath {
            player: id(&player)?,
            knots: None,
        })
    });
    // A spectator's free camera from where their camera is, and an orbit
    // `distance` units out around a point; `watch(p, ())` ends either.
    engine.register_fn("free_camera", |player: Dynamic| {
        push(Op::Camera {
            player: id(&player)?,
            camera: crate::ops::CameraOp::Free,
        })
    });
    engine.register_fn(
        "orbit_point",
        |player: Dynamic, at: Array, distance: Dynamic| {
            push(Op::Camera {
                player: id(&player)?,
                camera: crate::ops::CameraOp::Point {
                    at: vector(&at)?,
                    distance: float(&distance)?,
                },
            })
        },
    );
    engine.register_fn("set_look_limits", |player: Dynamic, _: ()| {
        push(Op::SetLookLimits {
            player: id(&player)?,
            limits: None,
        })
    });
    /// `"acts"` (the default) or `"frozen"`: see [`crate::ops::OrbitBody`].
    fn orbit_body(body: &str) -> Fallible<crate::ops::OrbitBody> {
        match body {
            "acts" => Ok(crate::ops::OrbitBody::Acts),
            "frozen" => Ok(crate::ops::OrbitBody::Frozen),
            _ => fail(format!(
                "an orbit camera's body \"acts\" or is \"frozen\", not \"{body}\""
            )),
        }
    }
    fn orbit_camera(
        player: Dynamic,
        target: Dynamic,
        min: Dynamic,
        max: Dynamic,
        distance: Dynamic,
        body: crate::ops::OrbitBody,
    ) -> Fallible<()> {
        let range = crate::ops::ORBIT_DISTANCE;
        let units = |v: &Dynamic| -> Fallible<u8> {
            let v = float(v)?;
            if !(f32::from(*range.start())..=f32::from(*range.end())).contains(&v) {
                return fail(format!(
                    "an orbit camera sits {} to {} units out",
                    range.start(),
                    range.end()
                ));
            }
            Ok(v.round() as u8)
        };
        let orbit = crate::ops::Orbit {
            target: id(&target)?,
            min: units(&min)?,
            max: units(&max)?,
            distance: units(&distance)?,
        };
        if !orbit.valid() {
            return fail("an orbit camera starts between its nearest and farthest");
        }
        push(Op::OrbitCamera {
            player: id(&player)?,
            body,
            orbit: Some(orbit),
        })
    }
    // `orbit_camera(p, target[, nearest, farthest], distance[, body])`,
    // `body` "acts" (the default: the click still reaches `on_activate`)
    // or "frozen" (`watch`'s: keys go to `on_observer`);
    // `orbit_camera(p, ()[, body])` ends that kind.
    engine.register_fn(
        "orbit_camera",
        |player: Dynamic, target: Dynamic, distance: Dynamic| {
            let body = crate::ops::OrbitBody::Acts;
            orbit_camera(player, target, distance.clone(), distance.clone(), distance, body)
        },
    );
    engine.register_fn(
        "orbit_camera",
        |player: Dynamic, target: Dynamic, min: Dynamic, max: Dynamic, distance: Dynamic| {
            let body = crate::ops::OrbitBody::Acts;
            orbit_camera(player, target, min, max, distance, body)
        },
    );
    engine.register_fn(
        "orbit_camera",
        |player: Dynamic,
         target: Dynamic,
         min: Dynamic,
         max: Dynamic,
         distance: Dynamic,
         body: rhai::ImmutableString| {
            orbit_camera(player, target, min, max, distance, orbit_body(&body)?)
        },
    );
    engine.register_fn("orbit_camera", |player: Dynamic, _: ()| {
        push(Op::OrbitCamera {
            player: id(&player)?,
            body: crate::ops::OrbitBody::Acts,
            orbit: None,
        })
    });
    engine.register_fn(
        "orbit_camera",
        |player: Dynamic, _: (), body: rhai::ImmutableString| {
            push(Op::OrbitCamera {
                player: id(&player)?,
                body: orbit_body(&body)?,
                orbit: None,
            })
        },
    );
    engine.register_fn("mount_image", |player: Dynamic, image: Dynamic| {
        push(Op::MountImage {
            player: id(&player)?,
            image: if image.is_unit() {
                None
            } else {
                Some(
                    image
                        .into_string()
                        .map_err(|_| "an image is a string like \"pkg:image/scope\", or ()")?,
                )
            },
        })
    });
}

fn fire_op(
    projectile: &str,
    at: [Dynamic; 3],
    velocity: [Dynamic; 3],
    by: Dynamic,
) -> Fallible<()> {
    let [x, y, z] = at;
    let [vx, vy, vz] = velocity;
    push(Op::Fire {
        projectile: projectile.into(),
        position: [float(&x)?, float(&y)?, float(&z)?],
        velocity: [float(&vx)?, float(&vy)?, float(&vz)?],
        by: credit(&by)?,
    })
}
fn push_op(target: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic, by: Dynamic) -> Fallible<()> {
    push(Op::Push {
        target: object_ref(&target)?,
        velocity: [float(&x)?, float(&y)?, float(&z)?],
        by: credit(&by)?,
    })
}
fn tumble_op(player: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic, by: Dynamic) -> Fallible<()> {
    let player = match object_ref(&player) {
        Ok(ObjectRef::Player(p)) => p,
        Ok(other) => return fail(format!("only players tumble, not {other}")),
        Err(_) => id(&player)?,
    };
    push(Op::Tumble {
        player,
        velocity: [float(&x)?, float(&y)?, float(&z)?],
        by: credit(&by)?,
    })
}

fn tether_op(player: Dynamic, anchor: Array, length: Dynamic, options: rhai::Map) -> Fallible<()> {
    for key in options.keys() {
        if !matches!(key.as_str(), "brick" | "object" | "reel" | "swing" | "keys" | "straight") {
            return fail(format!(
                "tether has no option `{key}` (brick, object, reel, swing, keys, straight)"
            ));
        }
    }
    let a = anchor.iter().map(float).collect::<Fallible<Vec<f32>>>()?;
    let [x, y, z] = a[..] else {
        return fail("tether's anchor is [x, y, z]");
    };
    let option = |key: &str| options.get(key).filter(|v| !v.is_unit());
    let brick = match option("brick") {
        None => None,
        Some(b) => Some(id(b)?),
    };
    let object = option("object").map(object_ref).transpose()?;
    let keys = match option("keys") {
        None => None,
        Some(k) => {
            let Some(k) = k.clone().try_cast::<Array>() else {
                return fail("tether's keys are [shortest, longest]");
            };
            let k = k.iter().map(float).collect::<Fallible<Vec<f32>>>()?;
            let [short, long] = k[..] else {
                return fail("tether's keys are [shortest, longest]");
            };
            Some([short, long])
        }
    };
    push(Op::Tether {
        player: id(&player)?,
        anchor: [x, y, z],
        length: if length.is_unit() {
            None
        } else {
            Some(float(&length)?)
        },
        brick,
        reel: option("reel").map(float).transpose()?,
        swing: option("swing").map(float).transpose()?,
        object,
        keys,
        straight: match option("straight").map(Dynamic::as_bool) {
            None => false,
            Some(Ok(straight)) => straight,
            Some(Err(_)) => return fail("tether's straight is true or false"),
        },
    })
}

/// Movable objects: reading them, and the `physics` operations.
fn register_physics(engine: &mut Engine) {
    engine.register_fn("object", |object: Dynamic| {
        with(|i| {
            let object = object_ref(&object)?;
            Ok(i.snapshot
                .object(object)
                .map_or(Dynamic::UNIT, |o| object_map(&o)))
        })
    });
    engine.register_fn("objects", || {
        with(|i| Ok(i.snapshot.objects.iter().map(object_map).collect::<Array>()))
    });
    engine.register_fn(
        "objects_near",
        |x: Dynamic, y: Dynamic, z: Dynamic, radius: Dynamic| {
            with(|i| {
                let centre = [float(&x)?, float(&y)?, float(&z)?];
                let radius = float(&radius)?;
                let near = |p: [f32; 3]| {
                    (p[0] - centre[0]).powi(2)
                        + (p[1] - centre[1]).powi(2)
                        + (p[2] - centre[2]).powi(2)
                        <= radius * radius
                };
                let players = i
                    .snapshot
                    .players
                    .iter()
                    .filter(|p| p.alive)
                    .map(|p| ObjectRef::Player(p.id));
                let entities = i.snapshot.entities.iter().map(|e| ObjectRef::Entity(e.id));
                let vehicles = i.snapshot.objects.iter().map(|o| o.object);
                Ok(players
                    .chain(entities)
                    .chain(vehicles)
                    .filter_map(|o| i.snapshot.object(o))
                    .filter(|o| near(o.position))
                    .map(|o| object_map(&o))
                    .collect::<Array>())
            })
        },
    );
    engine.register_fn("held", |player: Dynamic| {
        with(|i| {
            let player = id(&player)?;
            Ok(i.snapshot
                .holds
                .iter()
                .find(|h| h.player == player)
                .map_or(Dynamic::UNIT, |h| Dynamic::from(h.object.to_string())))
        })
    });
    engine.register_fn(
        "push",
        |target: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic| {
            push_op(target, x, y, z, Dynamic::UNIT)
        },
    );
    engine.register_fn("push", push_op);
    engine.register_fn(
        "tumble",
        |player: Dynamic, x: Dynamic, y: Dynamic, z: Dynamic| {
            tumble_op(player, x, y, z, Dynamic::UNIT)
        },
    );
    engine.register_fn("tumble", tumble_op);
    engine.register_fn(
        "hold",
        |player: Dynamic, target: Dynamic, distance: Dynamic| {
            push(Op::Hold {
                player: id(&player)?,
                target: object_ref(&target)?,
                distance: float(&distance)?,
                at: None,
                force: None,
                turn: false,
            })
        },
    );
    // `hold(player, ref, distance, #{ at: [x, y, z], force: f, turn: true })`:
    // every option may be left out.
    engine.register_fn(
        "hold",
        |player: Dynamic, target: Dynamic, distance: Dynamic, options: rhai::Map| {
            for key in options.keys() {
                if !matches!(key.as_str(), "at" | "force" | "turn") {
                    return fail(format!("hold has no option `{key}` (at, force, turn)"));
                }
            }
            let at = match options.get("at") {
                None => None,
                Some(value) if value.is_unit() => None,
                Some(value) => {
                    let Some(a) = value.clone().try_cast::<Array>() else {
                        return fail("hold's `at` is [x, y, z]");
                    };
                    let v = a.iter().map(float).collect::<Fallible<Vec<f32>>>()?;
                    let [x, y, z] = v[..] else {
                        return fail("hold's `at` is [x, y, z]");
                    };
                    Some([x, y, z])
                }
            };
            let force = options.get("force").map(float).transpose()?;
            let turn = match options.get("turn") {
                None => false,
                Some(value) => match value.as_bool() {
                    Ok(b) => b,
                    Err(_) => return fail("hold's `turn` is true or false"),
                },
            };
            push(Op::Hold {
                player: id(&player)?,
                target: object_ref(&target)?,
                distance: float(&distance)?,
                at,
                force,
                turn,
            })
        },
    );
    engine.register_fn("hold_distance", |player: Dynamic, distance: Dynamic| {
        push(Op::HoldDistance {
            player: id(&player)?,
            distance: float(&distance)?,
        })
    });
    // `reach(player, distance, #{ near: d, force: f, turn: true })`: hold
    // the first thing that comes where they look within `distance`
    // (`Op::Reach`); every option may be left out.
    engine.register_fn(
        "reach",
        |player: Dynamic, distance: Dynamic, options: rhai::Map| {
            for key in options.keys() {
                if !matches!(key.as_str(), "near" | "force" | "turn") {
                    return fail(format!("reach has no option `{key}` (near, force, turn)"));
                }
            }
            let near = options.get("near").map(float).transpose()?.unwrap_or(0.5);
            let force = options.get("force").map(float).transpose()?;
            let turn = match options.get("turn") {
                None => false,
                Some(value) => match value.as_bool() {
                    Ok(b) => b,
                    Err(_) => return fail("reach's `turn` is true or false"),
                },
            };
            push(Op::Reach {
                player: id(&player)?,
                distance: float(&distance)?,
                near,
                force,
                turn,
            })
        },
    );
    // How far off what `player` holds is carried, or () when nothing is held.
    engine.register_fn("held_distance", |player: Dynamic| {
        with(|i| {
            let player = id(&player)?;
            Ok(i.snapshot
                .holds
                .iter()
                .find(|h| h.player == player)
                .map_or(Dynamic::UNIT, |h| Dynamic::from_float(f64::from(h.distance))))
        })
    });
    engine.register_fn("let_go", |player: Dynamic| {
        push(Op::LetGo {
            player: id(&player)?,
        })
    });
    engine.register_fn("tethered", |player: Dynamic| {
        with(|i| {
            let player = id(&player)?;
            Ok(i.snapshot
                .tethers
                .iter()
                .find(|t| t.player == player)
                .map_or(Dynamic::UNIT, |t| {
                    let mut map = rhai::Map::new();
                    let [x, y, z] = t.anchor;
                    map.insert("x".into(), Dynamic::from_float(x.into()));
                    map.insert("y".into(), Dynamic::from_float(y.into()));
                    map.insert("z".into(), Dynamic::from_float(z.into()));
                    map.insert("length".into(), Dynamic::from_float(t.length.into()));
                    map.insert("target".into(), Dynamic::from_float(t.target.into()));
                    map.insert(
                        "brick".into(),
                        t.brick.map_or(Dynamic::UNIT, |b| Dynamic::from_int(b as i64)),
                    );
                    map.insert(
                        "object".into(),
                        t.object
                            .map_or(Dynamic::UNIT, |o| Dynamic::from(o.to_string())),
                    );
                    Dynamic::from_map(map)
                }))
        })
    });
    engine.register_fn(
        "tether",
        |player: Dynamic, anchor: Array, length: Dynamic| {
            tether_op(player, anchor, length, rhai::Map::new())
        },
    );
    // `tether(player, [x, y, z], length, #{ brick: id, object: ref, reel: r,
    // swing: s, keys: [shortest, longest], straight: true })`:
    // every option may be left out; a length of `()` is as long as the
    // rope spans now.
    engine.register_fn("tether", tether_op);
    engine.register_fn("tether_length", |player: Dynamic, length: Dynamic| {
        push(Op::TetherLength {
            player: id(&player)?,
            length: float(&length)?,
        })
    });
    engine.register_fn("untether", |player: Dynamic| {
        push(Op::Untether {
            player: id(&player)?,
            keep: None,
        })
    });
    // `untether(player, #{ keep: k })`: let go, keeping only `k` (0 to 1) of
    // their speed relative to what the rope was tied to.
    engine.register_fn("untether", |player: Dynamic, options: rhai::Map| {
        let keep = match options.get("keep") {
            Some(k) => {
                let k = float(k)?;
                if !(0.0..=1.0).contains(&k) {
                    return Err("untether keep must be between 0 and 1".into());
                }
                Some(k)
            }
            None => None,
        };
        push(Op::Untether {
            player: id(&player)?,
            keep,
        })
    });
    fn mount_object(
        mount: Dynamic,
        rider: Dynamic,
        node: i64,
        can_dismount: bool,
        turn: Dynamic,
    ) -> Fallible<()> {
        push(Op::MountObject {
            mount: id(&mount)?,
            rider: id(&rider)?,
            node: u8::try_from(node)
                .ok()
                .filter(|n| usize::from(*n) < crate::ops::MAX_MOUNT_POINTS)
                .ok_or("a mount point is 0 to 7")?,
            can_dismount,
            turn: float(&turn)?.to_radians(),
        })
    }
    engine.register_fn(
        "mount_object",
        |mount: Dynamic, rider: Dynamic, node: i64, can_dismount: bool| {
            mount_object(mount, rider, node, can_dismount, Dynamic::from_float(0.0))
        },
    );
    engine.register_fn("mount_object", mount_object);
    engine.register_fn("unmount_object", |rider: Dynamic| {
        push(Op::UnmountObject { rider: id(&rider)? })
    });
    engine.register_fn(
        "spawn_vehicle",
        |definition: &str,
         x: Dynamic,
         y: Dynamic,
         z: Dynamic,
         yaw: Dynamic,
         velocity: Array,
         owner: Dynamic| {
            let v = velocity.iter().map(float).collect::<Fallible<Vec<f32>>>()?;
            let [vx, vy, vz] = v[..] else {
                return fail("velocity is [x, y, z]");
            };
            push(Op::SpawnVehicle {
                definition: definition.into(),
                position: [float(&x)?, float(&y)?, float(&z)?],
                yaw: float(&yaw)?,
                velocity: [vx, vy, vz],
                owner: credit(&owner)?,
            })
        },
    );
    engine.register_fn("remove_vehicle", |vehicle: Dynamic| {
        let vehicle = match object_ref(&vehicle) {
            Ok(ObjectRef::Vehicle(v)) => v,
            Ok(other) => return fail(format!("{other} is not a vehicle")),
            Err(_) => id(&vehicle)?,
        };
        push(Op::RemoveVehicle { vehicle })
    });
}

/// Whether `source` compiles as a package script, with the line of the
/// first problem: for tools that write scripts (an Add-On port's rules)
/// to refuse one the game would not load.
pub fn check_syntax(source: &str) -> Result<(), String> {
    sandbox().compile(source).map(drop).map_err(|e| match e.1.line() {
        Some(line) => format!("line {line}: {}", e.0),
        None => e.0.to_string(),
    })
}

fn sandbox() -> Engine {
    use rhai::packages::{
        BasicArrayPackage, BasicMapPackage, BasicMathPackage, BasicStringPackage, CorePackage,
        LogicPackage, MoreStringPackage, Package,
    };
    let mut engine = Engine::new_raw();
    CorePackage::new().register_into_engine(&mut engine);
    LogicPackage::new().register_into_engine(&mut engine);
    BasicMathPackage::new().register_into_engine(&mut engine);
    BasicArrayPackage::new().register_into_engine(&mut engine);
    BasicMapPackage::new().register_into_engine(&mut engine);
    BasicStringPackage::new().register_into_engine(&mut engine);
    MoreStringPackage::new().register_into_engine(&mut engine);
    engine.disable_symbol("eval");
    engine.set_max_call_levels(32);
    engine.set_max_expr_depths(64, 32);
    engine.set_max_string_size(4096);
    engine.set_max_array_size(65_536);
    engine.set_max_map_size(1024);
    engine.set_max_variables(256);
    engine.set_max_functions(256);
    engine.on_print(|text| {
        CURRENT.with(|c| {
            if let Some(i) = c.borrow_mut().as_mut()
                && i.output.len() < MAX_OUTPUT_LINES
            {
                i.output.push(text.chars().take(256).collect());
            }
        })
    });
    engine.on_debug(|_, _, _| {});
    // Each call's budget is enforced here, so `call` needs no `&mut`: the
    // engine may run while the world it asks is borrowed.
    engine.set_max_operations(Budget::Tick.operations().max(Budget::Generate.operations()) + 1);
    engine.on_progress(|operations| {
        OPERATIONS.with(|o| o.set(operations));
        (operations > LIMIT.with(std::cell::Cell::get)).then_some(Dynamic::UNIT)
    });
    register_api(&mut engine);
    engine
}

thread_local! {
    /// Operations the running call has used so far.
    static OPERATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Compiled scripts for every package with behaviour.
pub struct Runtime {
    engine: Engine,
    scripts: BTreeMap<String, Arc<AST>>,
    /// Script file per package, for diagnostics.
    sources: BTreeMap<String, String>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            engine: sandbox(),
            scripts: BTreeMap::new(),
            sources: BTreeMap::new(),
        }
    }
}
impl Runtime {
    /// Compile each package's script and check that the functions its
    /// content names exist with the right arity.
    pub fn compile(set: &crate::Catalog) -> Result<Self, Vec<Diagnostic>> {
        let mut runtime = Self::default();
        let mut problems = Vec::new();
        for (id, package) in &set.packages {
            let Some(behaviour) = &package.behaviour else {
                continue;
            };
            let Some(source) = package.script_source() else {
                continue;
            };
            let ast = match runtime.engine.compile(source) {
                Ok(ast) => ast,
                Err(e) => {
                    let at = match e.1.line() {
                        Some(line) => format!("{}:{line}", location(id, &behaviour.script)),
                        None => location(id, &behaviour.script),
                    };
                    problems.push(Diagnostic::error("script.syntax", e.0.to_string()).at(at));
                    continue;
                }
            };
            if !ast.statements().is_empty() {
                problems.push(
                    Diagnostic::error("script.top_level", "scripts may only define functions")
                        .at(location(id, &behaviour.script))
                        .hint("move top-level statements into a function; keep constants in behaviour state"),
                );
            }
            let has = |name: &str, arity: usize| {
                ast.iter_functions()
                    .any(|f| f.name == name && f.params.len() == arity)
            };
            let mut need = |name: String, arity: usize, why: &str| {
                if !has(&name, arity) {
                    problems.push(
                        Diagnostic::error(
                            "script.missing_function",
                            format!("{why} needs `fn {name}` with {arity} parameter(s)"),
                        )
                        .at(location(id, &behaviour.script)),
                    );
                }
            };
            for c in &behaviour.commands {
                need(
                    format!("cmd_{}", c.name),
                    1 + c.args.len(),
                    &format!("command `{}`", c.name),
                );
            }
            if behaviour.on_join {
                need("on_join".into(), 1, "on_join");
            }
            if behaviour.on_loadout {
                need("on_loadout".into(), 1, "on_loadout");
            }
            if behaviour.on_death {
                need("on_death".into(), 2, "on_death");
            }
            if behaviour.on_spawn {
                need("on_spawn".into(), 1, "on_spawn");
            }
            if behaviour.on_leave {
                need("on_leave".into(), 1, "on_leave");
            }
            if behaviour.on_path_node {
                need("on_path_node".into(), 2, "on_path_node");
            }
            if behaviour.on_observer {
                need("on_observer".into(), 2, "on_observer");
            }
            if behaviour.on_damage {
                need("on_damage".into(), 4, "on_damage");
            }
            if behaviour.on_entity_damage {
                need("on_entity_damage".into(), 4, "on_entity_damage");
            }
            if behaviour.on_entity_death {
                need("on_entity_death".into(), 3, "on_entity_death");
            }
            if behaviour.on_pickup {
                need("on_pickup".into(), 3, "on_pickup");
            }
            if behaviour.on_drop {
                need("on_drop".into(), 3, "on_drop");
            }
            if behaviour.on_projectile_hit {
                need("on_projectile_hit".into(), 1, "on_projectile_hit");
            }
            if behaviour.on_minigame {
                need("on_minigame".into(), 1, "on_minigame");
            }
            if behaviour.on_pick_spawn {
                need("on_pick_spawn".into(), 1, "on_pick_spawn");
            }
            if !behaviour.zones.is_empty() {
                need("on_zone".into(), 3, "zones");
            }
            if !behaviour.brick_outputs.is_empty() {
                need("on_brick_output".into(), 4, "brick_outputs");
            }
            let follows = behaviour.brick_inputs.iter().any(|i| i.follows.is_some());
            if follows {
                need("on_brick_input".into(), 3, "brick_inputs that follow an input");
            }
            if behaviour.on_copy {
                need("on_copy".into(), 2, "on_copy");
            }
            if behaviour.on_place {
                need("on_place".into(), 2, "on_place");
            }
            if behaviour.on_copy_ghost {
                need("on_copy_ghost".into(), 2, "on_copy_ghost");
            }
            if behaviour.on_activate {
                need("on_activate".into(), 1, "on_activate");
            }
            if behaviour.on_event_row {
                need("on_event_row".into(), 3, "on_event_row");
            }
            if behaviour.on_trigger {
                need("on_trigger".into(), 3, "on_trigger");
            }
            if behaviour.on_drop_key {
                need("on_drop_key".into(), 1, "on_drop_key");
            }
            for policy in &behaviour.policies {
                need(format!("allow_{policy}"), 1, &format!("policy `{policy}`"));
            }
            if behaviour.tick_interval.is_some() {
                need("on_tick".into(), 0, "tick_interval");
            }
            for w in package.worlds.values() {
                need(w.generate.clone(), 2, "the world provider");
            }
            for e in package.entities.values() {
                need(e.think.clone(), 1, &format!("entity `{}`", e.name));
            }
            let adds_events = follows
                || !behaviour.brick_outputs.is_empty()
                || !behaviour.brick_targets.is_empty();
            if adds_events && !package.manifest.capabilities.iter().any(|c| c == "brick_events")
            {
                problems.push(
                    Diagnostic::error(
                        "behaviour.brick_outputs",
                        "brick_outputs, brick_targets and inputs that follow another need the \
                         `brick_events` capability",
                    )
                    .at(location(id, &behaviour.script))
                    .hint("add \"brick_events\" to the manifest's capabilities"),
                );
            }
            runtime.scripts.insert(id.clone(), Arc::new(ast));
            runtime.sources.insert(id.clone(), behaviour.script.clone());
        }
        if problems.is_empty() {
            Ok(runtime)
        } else {
            Err(problems)
        }
    }
    pub fn has_script(&self, package: &str) -> bool {
        self.scripts.contains_key(package)
    }
    /// Script operations the last [`call`](Self::call) used, whether it
    /// succeeded or not: what the engine charges to the caller's share.
    pub fn last_operations(&self) -> u64 {
        OPERATIONS.with(std::cell::Cell::get)
    }
    /// Run one function. On error nothing of the call is kept.
    pub fn call(&self, package: &str, call: Call<'_>) -> Result<Outcome, Diagnostic> {
        let ast =
            self.scripts.get(package).cloned().ok_or_else(|| {
                Diagnostic::error("script.none", "package has no script").at(package)
            })?;
        let previous_limit = LIMIT.with(|l| l.replace(call.budget.operations()));
        OPERATIONS.with(|o| o.set(0));
        // The world reference outlives this function; the pointer is taken
        // back out below, before `call.world`'s borrow ends, and is never
        // reached after that (see `with_world`).
        let world = call.world.map(|w| {
            let w: *const (dyn World + '_) = w;
            // SAFETY: only the trait object's lifetime bound changes.
            unsafe { std::mem::transmute::<*const (dyn World + '_), *const (dyn World + 'static)>(w) }
        });
        let previous = CURRENT.with(|c| {
            c.borrow_mut().replace(Invocation {
                snapshot: call.snapshot,
                caller: call.caller,
                aim: call.aim,
                entity: call.entity,
                state: call.state,
                entity_vars: call.entity_vars,
                written: BTreeMap::new(),
                ops: Vec::new(),
                output: Vec::new(),
                world,
                rays: 0,
            })
        });
        let options = rhai::CallFnOptions::new()
            .eval_ast(false)
            .rewind_scope(true);
        let mut scope = rhai::Scope::new();
        let result = self.engine.call_fn_with_options::<Dynamic>(
            options,
            &mut scope,
            &ast,
            call.function,
            call.args,
        );
        let invocation = CURRENT
            .with(|c| std::mem::replace(&mut *c.borrow_mut(), previous))
            .expect("set above");
        LIMIT.with(|l| l.set(previous_limit));
        match result {
            Ok(returned) => Ok(Outcome {
                returned,
                ops: invocation.ops,
                state: invocation.state,
                entity_vars: invocation.written,
                output: invocation.output,
            }),
            Err(e) => {
                let code = match *e {
                    EvalAltResult::ErrorTooManyOperations(_)
                    | EvalAltResult::ErrorTerminated(..) => "script.budget",
                    EvalAltResult::ErrorDataTooLarge(..) | EvalAltResult::ErrorStackOverflow(_) => {
                        "script.limit"
                    }
                    _ => "script.error",
                };
                let position = e.position();
                let script = self
                    .sources
                    .get(package)
                    .map_or(String::new(), Clone::clone);
                let mut problem =
                    Diagnostic::error(code, format!("{}: {}", call.function, e)).at(match position
                        .line()
                    {
                        Some(line) => format!("{}:{line}", location(package, &script)),
                        None => location(package, &script),
                    });
                if code == "script.budget" {
                    problem = problem.hint(format!(
                        "the call exceeded {} script operations",
                        call.budget.operations()
                    ));
                }
                Err(problem)
            }
        }
    }
}

/// Convert a script's `[[x, y, z, m], ...]` into voxels, bounded.
pub fn voxels(value: &Dynamic, materials: usize, limit: usize) -> Result<Vec<[i64; 4]>, String> {
    let array = value.read_lock::<Array>().ok_or_else(|| {
        format!(
            "expected an array of [x, y, z, material], got {}",
            value.type_name()
        )
    })?;
    if array.len() > limit {
        return Err(format!(
            "{} voxels is more than the chunk limit {limit}",
            array.len()
        ));
    }
    let mut out = Vec::with_capacity(array.len());
    for entry in array.iter() {
        let v = entry
            .read_lock::<Array>()
            .filter(|v| v.len() == 4)
            .ok_or("each voxel is [x, y, z, material]")?;
        let mut item = [0_i64; 4];
        for (slot, value) in item.iter_mut().zip(v.iter()) {
            *slot = value
                .as_int()
                .map_err(|_| "voxel coordinates and material are integers")?;
        }
        if item[3] < 0 || item[3] as usize >= materials {
            return Err(format!("material {} is not declared", item[3]));
        }
        if item.iter().take(3).any(|c| c.unsigned_abs() > 100_000) {
            return Err("voxel coordinate out of range".into());
        }
        out.push(item);
    }
    Ok(out)
}
