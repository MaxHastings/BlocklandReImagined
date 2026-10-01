//! The content kinds a package may provide, and their typed definitions.
//! Each kind declares which side needs it: server-only kinds (behaviour,
//! scripts, world providers) never leave the host.
use anyhow::{Result, ensure};
use bri_package::packages::Side;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Server behaviour: declared commands, state and hooks (JSON).
    Behaviour,
    /// Sandboxed server script source referenced by a behaviour.
    Script,
    /// A chunked world provider (JSON).
    World,
    /// An entity kind: name, model, body and think function (JSON). Server
    /// side: clients learn the model id from the replicated entity.
    Entity,
    /// A declarative box model (JSON).
    Model,
    /// A declarative HUD panel (JSON).
    Hud,
    /// A player archetype: movement, collision body, health and look
    /// (JSON). Server side: clients receive the host's archetype table with
    /// the checkpoint and predict from it.
    Archetype,
    /// A PNG image drawn on block faces. Client side: downloaded with the
    /// package like any file.
    Texture,
    /// A block: textures or flipbooks per face, and named states game rules
    /// switch between (JSON). Drawn on bricks whose `look` names it.
    Block,
    /// A weapons pack (`weapons.json`) merged onto the base game's by the
    /// engine's content loading (`content_identity::kind_providers`).
    Weapons,
    /// A vehicles pack (`vehicles.json`), merged the same way.
    Vehicles,
    /// A brick catalog (`brick-catalog/stock-catalog.json` with its meshes
    /// and collisions beside it), merged the same way.
    Bricks,
    /// Bot kinds a Vehicle Spawn brick can hold (`bots.json`): the name the
    /// list shows and how the engine's bot brain plays. Read by the engine
    /// like weapons and vehicles; players load it for the wrench list.
    Bots,
    /// A game mode the host can pick in Start Game: which Add-Ons run and
    /// on which map (JSON). Server side: only the host reads it.
    Mode,
}
impl Kind {
    pub const NAMES: [&str; 14] = [
        "behaviour",
        "script",
        "world",
        "entity",
        "model",
        "hud",
        "archetype",
        "texture",
        "block",
        "weapons",
        "vehicles",
        "bricks",
        "bots",
        "mode",
    ];
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "behaviour" => Self::Behaviour,
            "script" => Self::Script,
            "world" => Self::World,
            "entity" => Self::Entity,
            "model" => Self::Model,
            "hud" => Self::Hud,
            "archetype" => Self::Archetype,
            "texture" => Self::Texture,
            "block" => Self::Block,
            "weapons" => Self::Weapons,
            "vehicles" => Self::Vehicles,
            "bricks" => Self::Bricks,
            "bots" => Self::Bots,
            "mode" => Self::Mode,
            _ => return None,
        })
    }
    pub fn side(self) -> Side {
        match self {
            Self::Behaviour
            | Self::Script
            | Self::World
            | Self::Entity
            | Self::Archetype
            | Self::Mode => Side::Server,
            // Shared gameplay data is client-visible: clients load it too.
            Self::Model
            | Self::Hud
            | Self::Texture
            | Self::Block
            | Self::Weapons
            | Self::Vehicles
            | Self::Bricks
            | Self::Bots => Side::Client,
        }
    }
    /// Largest accepted file of this kind.
    pub fn max_bytes(self) -> usize {
        match self {
            Self::Script => 256 * 1024,
            Self::Weapons | Self::Vehicles | Self::Bricks => 32 * 1024 * 1024,
            _ => 128 * 1024,
        }
    }
}

fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 48
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
fn color(c: &[f32; 4]) -> bool {
    c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

/// Server behaviour: what clients may ask for, what state exists, and which
/// script functions the engine calls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Behaviour {
    pub schema_version: u32,
    /// Package-relative script file (also listed as a `script` provide).
    pub script: String,
    #[serde(default)]
    pub commands: Vec<CommandDef>,
    #[serde(default)]
    pub state: StateSchema,
    /// `on_join(player)` when a player joins.
    #[serde(default)]
    pub on_join: bool,
    /// `on_death(victim, killer)` after any player dies, however it
    /// happened; `killer` is the player credited, or `()`. Delivered at the
    /// start of the next tick.
    #[serde(default)]
    pub on_death: bool,
    /// `on_loadout(player)` after a player's items are set afresh: when
    /// they spawn or respawn, and when they join or leave a minigame. The
    /// place to hand out an Add-On's items. Delivered at the start of the
    /// next tick.
    #[serde(default)]
    pub on_loadout: bool,
    /// `on_spawn(player)` after a player comes to life: joining, respawning
    /// or `respawn`. Delivered at the start of the next tick, after
    /// `on_loadout`.
    #[serde(default)]
    pub on_spawn: bool,
    /// `on_leave(player)` as a player leaves the server, while their state
    /// is still readable.
    #[serde(default)]
    pub on_leave: bool,
    /// `on_damage(victim, attacker, amount, info)` before a player takes
    /// damage: return the amount to take instead (0 prevents it), or `()`
    /// to leave it. `attacker` is the player responsible, or `()`; `info`
    /// is `#{ kind, type, direct }`. Called as the damage happens, so it
    /// must be quick; damage its own operations cause is not filtered
    /// again.
    #[serde(default)]
    pub on_damage: bool,
    /// `on_entity_damage(entity, attacker, amount, info)` before one of
    /// this package's entities is hurt by a shot, a blast or `explode`:
    /// answered like `on_damage`. `info` is `#{ kind, type }`, `kind` being
    /// `weapon` or `package`.
    #[serde(default)]
    pub on_entity_damage: bool,
    /// `on_entity_death(entity, killer, info)` as one of this package's
    /// entities runs out of health, while it can still be read; it is
    /// removed right after. `killer` is the player responsible, or `()`.
    #[serde(default)]
    pub on_entity_death: bool,
    /// `on_pickup(player, item, info)` as a living player touches an item
    /// of this package (or one it depends on) lying in the world, before
    /// they pick it up, whether or not they have room: `false` leaves it,
    /// `"take"` uses it up without giving it (a spawn brick's item starts
    /// its respawn), `()` or `true` picks it up as usual. `info` is
    /// `#{ drop, spawner, data }`: the dropped item's id or the spawn
    /// brick's, and what `on_drop` kept with it. Called as it happens, so
    /// it must be quick.
    #[serde(default)]
    pub on_pickup: bool,
    /// `on_drop(player, item, slot)` as a player drops a tool of this
    /// package (or one it depends on). What it returns (a number, a map:
    /// a magazine's rounds) is kept with the dropped item and handed to
    /// `on_pickup` as `info.data`.
    #[serde(default)]
    pub on_drop: bool,
    /// `on_projectile_hit(hit)` after a projectile of this package's
    /// weapons (or one it depends on) strikes something. `hit` is
    /// `#{ projectile, by, kind, id, ref, x, y, z, nx, ny, nz, vx, vy,
    /// vz }`, `kind` being `player`, `vehicle`, `entity`, `brick` or `map`
    /// and `by` the shooter or `()`. Delivered at the start of the next
    /// tick.
    #[serde(default)]
    pub on_projectile_hit: bool,
    /// `on_activate(player)` as a living player clicks with nothing to
    /// fire (`serverCmdActivateStuff`, which v20 Add-Ons packaged as
    /// `Player::activateStuff`), before the engine's own activation: the
    /// arm's swing, flipping a vehicle, a brick's `onActivate`. Return
    /// `true` to take the click, so the engine does nothing more; anything
    /// else lets it carry on. Every package that declares it is asked, in
    /// load order, until one takes the click. Called as it happens, so it
    /// must be quick.
    #[serde(default)]
    pub on_activate: bool,
    /// `on_trigger(player, trigger, down)` as a living player with nothing
    /// in their hand presses (`down` true) or lets go of a trigger
    /// (v20's `Armor::onTrigger`). Trigger 0 is fire, the empty-hand click;
    /// its press comes before `on_activate`. Return `true` to take the
    /// press, so the engine does nothing more with it. Every package that
    /// declares it is asked, in load order, until one takes it.
    #[serde(default)]
    pub on_trigger: bool,
    /// `on_tick()` every `tick_interval` ticks, when set.
    #[serde(default)]
    pub tick_interval: Option<u32>,
    /// Engine decisions this package is asked about ([`POLICIES`]). For
    /// each, the engine calls `allow_<policy>(player)` before acting: `true`
    /// allows, `false` or a reason string refuses.
    #[serde(default)]
    pub policies: Vec<String>,
}
/// Decisions the engine owns the mechanism for and asks packages about.
pub const POLICIES: &[&str] = &[
    // A dead player asking to come back.
    "respawn", // Any command that builds (plant, paint, wand, wrench edits).
    "build",
    // Taking a tool, spray can or FX can into the hand, or putting it away
    // (`serverCmdUseTool`, `serverCmdUnUseTool`, `serverCmdUseSprayCan`,
    // `serverCmdUseFXCan`).
    "equip",
];
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDef {
    /// Command name; the script function is `cmd_<name>`.
    pub name: String,
    /// Argument types, in order: `int`, `float`, `string` or `bool`.
    #[serde(default)]
    pub args: Vec<ArgType>,
    /// When set, the engine resolves the caller's aim against bricks up to
    /// this distance and passes the hit to the script as `aim()`.
    #[serde(default)]
    pub aim_reach: Option<f32>,
    /// Minimum ticks between two uses by one player.
    #[serde(default)]
    pub cooldown_ticks: u32,
    /// Only administrators may send it.
    #[serde(default)]
    pub admin: bool,
    /// Dead players may send it too (a spectator vote, a class pick). By
    /// default only living players can (stress campaign W4: preconditions
    /// are declared, never left to each handler).
    #[serde(default)]
    pub while_dead: bool,
    /// Only an image runs it (a state, jet, light, cancel or wheel command
    /// of the held image): typed in chat or sent from a HUD it is refused,
    /// so players cannot type a gun's `/fire` or `/reload`.
    #[serde(default)]
    pub tool_only: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgType {
    Int,
    Float,
    String,
    Bool,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateSchema {
    /// Keys every player has, with defaults. Stored per durable player.
    #[serde(default)]
    pub player: BTreeMap<String, StateKey>,
    /// Server-wide keys.
    #[serde(default)]
    pub global: BTreeMap<String, StateKey>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateKey {
    pub default: serde_json::Value,
    /// Which clients receive the value. HUD panels may bind only visible
    /// keys.
    #[serde(default)]
    pub visible: Visible,
    /// Saved by the host and restored after a restart.
    #[serde(default = "yes")]
    pub persist: bool,
}
fn yes() -> bool {
    true
}
/// The audience of a state value: a player's secret (a hand of cards, a
/// unit's position under fog) is visible to that player alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visible {
    /// Stays on the server.
    #[default]
    Server,
    /// A player key sent only to the player it belongs to.
    Owner,
    /// Sent to every client.
    Everyone,
}
impl Behaviour {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "behaviour schema_version must be 1"
        );
        ensure!(self.commands.len() <= 64, "at most 64 commands");
        let mut names = std::collections::BTreeSet::new();
        for c in &self.commands {
            ensure!(
                identifier(&c.name),
                "command name `{}` must be lowercase a-z, 0-9, _",
                c.name
            );
            ensure!(names.insert(&c.name), "duplicate command `{}`", c.name);
            ensure!(
                c.args.len() <= 8,
                "command `{}` has more than 8 arguments",
                c.name
            );
            if let Some(reach) = c.aim_reach {
                ensure!(
                    reach.is_finite() && (0.0..=64.0).contains(&reach),
                    "aim_reach must be 0 to 64"
                );
            }
        }
        for (key, def) in self.state.player.iter().chain(&self.state.global) {
            ensure!(
                identifier(key),
                "state key `{key}` must be lowercase a-z, 0-9, _"
            );
            crate::state::check_value(&def.default)?;
        }
        for (i, policy) in self.policies.iter().enumerate() {
            ensure!(
                POLICIES.contains(&policy.as_str()),
                "unknown policy `{policy}`; known: {}",
                POLICIES.join(", ")
            );
            ensure!(
                !self.policies[..i].contains(policy),
                "policy `{policy}` listed twice"
            );
        }
        for (key, def) in &self.state.global {
            ensure!(
                def.visible != Visible::Owner,
                "server-wide key `{key}` has no owner; use \"everyone\" or \"server\""
            );
        }
        ensure!(
            self.state.player.len() + self.state.global.len() <= 256,
            "at most 256 state keys"
        );
        if let Some(interval) = self.tick_interval {
            ensure!(
                (1..=12_000).contains(&interval),
                "tick_interval must be 1 to 12000"
            );
        }
        Ok(())
    }
}

/// A game mode: a named choice in Start Game that says which Add-Ons run
/// and, optionally, on which map. Like a v21 gamemode, it is data: the
/// Add-Ons it names bring the rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameMode {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// A world an included Add-On provides (`package:world/name`) or a base
    /// game map id. None lets the host pick any base map.
    #[serde(default)]
    pub map: Option<String>,
    /// Package ids that run: the mode's own package or its dependencies,
    /// so turning the mode on turns them on.
    pub add_ons: Vec<String>,
    /// The mode's own mini-game, as v21 gamemodes had: the server runs it,
    /// every player is in it from joining, and nobody can start, join or
    /// leave another. None leaves mini-games to the players.
    #[serde(default)]
    pub minigame: Option<ModeMiniGame>,
}
/// A game mode's mini-game settings, the Mini-Game dialog's fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ModeMiniGame {
    /// Shown in the mini-game list; the mode's name when empty.
    pub title: String,
    /// Up to five items, by id (`v20.weapon.gunitem`, `package:weapon/name`).
    /// Add-Ons can hand out more in `on_loadout`.
    pub loadout: Vec<String>,
    /// The body every player gets (`v20.player.<datablock>` or a package
    /// archetype); the Standard Player when empty.
    pub player_type: String,
    pub respawn_seconds: f32,
    pub brick_respawn_seconds: f32,
    pub vehicle_respawn_seconds: f32,
    pub points_kill_player: i32,
    pub points_kill_self: i32,
    pub points_die: i32,
    pub points_break_brick: i32,
    pub points_plant_brick: i32,
    pub falling_damage: bool,
    pub weapon_damage: bool,
    pub self_damage: bool,
    pub vehicle_damage: bool,
    /// Whether weapons break the game's bricks (the world's own, and every
    /// player's with `use_all_players_bricks`).
    pub brick_damage: bool,
    pub building: bool,
    pub painting: bool,
    pub use_all_players_bricks: bool,
}
impl Default for ModeMiniGame {
    fn default() -> Self {
        Self {
            title: String::new(),
            loadout: Vec::new(),
            player_type: String::new(),
            respawn_seconds: 5.0,
            brick_respawn_seconds: 30.0,
            vehicle_respawn_seconds: 5.0,
            points_kill_player: 1,
            points_kill_self: -1,
            points_die: 0,
            points_break_brick: 0,
            points_plant_brick: 0,
            falling_damage: true,
            weapon_damage: true,
            self_damage: true,
            vehicle_damage: true,
            brick_damage: true,
            building: true,
            painting: true,
            use_all_players_bricks: false,
        }
    }
}
impl ModeMiniGame {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.title.chars().count() <= 35 && !self.title.chars().any(char::is_control),
            "minigame title must be at most 35 characters"
        );
        ensure!(
            self.loadout.len() <= 5,
            "a minigame loadout has at most 5 items"
        );
        for item in &self.loadout {
            ensure!(
                bri_package::id::is_content_ref(item, Some("weapon")),
                "minigame loadout item `{item}` is not an item id"
            );
        }
        ensure!(
            self.player_type.is_empty() || bri_package::id::is_content_ref(&self.player_type, None),
            "minigame player_type `{}` is not a player type id",
            self.player_type
        );
        ensure!(
            (1.0..=30.0).contains(&self.respawn_seconds),
            "respawn_seconds must be 1 to 30"
        );
        ensure!(
            (2.0..=300.0).contains(&self.brick_respawn_seconds),
            "brick_respawn_seconds must be 2 to 300"
        );
        ensure!(
            (0.0..=300.0).contains(&self.vehicle_respawn_seconds),
            "vehicle_respawn_seconds must be 0 to 300"
        );
        Ok(())
    }
}
impl GameMode {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "mode schema_version must be 1");
        ensure!(text(&self.name, 48), "name must be 1 to 48 characters");
        ensure!(
            self.description.len() <= 512 && !self.description.chars().any(char::is_control),
            "description must be at most 512 characters on one line"
        );
        ensure!(
            self.map.as_ref().is_none_or(|m| text(m, 160)),
            "map must be a world id or a map id"
        );
        ensure!(self.add_ons.len() <= 64, "at most 64 add_ons");
        if let Some(minigame) = &self.minigame {
            minigame.validate()?;
        }
        for id in &self.add_ons {
            ensure!(
                bri_package::id::namespace_problem(id).is_none(),
                "add_ons entry `{id}` is not a package id"
            );
        }
        Ok(())
    }
}

/// A world made of chunks the package generates on demand. The engine owns
/// streaming, collision, replication and persistence of edits; the package
/// owns what a chunk contains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkWorld {
    pub schema_version: u32,
    /// Script function `fn(cx, cz)` returning `[[x, y, z, material], ...]`
    /// in voxel coordinates.
    pub generate: String,
    /// Voxels along a chunk's X and Z.
    pub chunk_voxels: u32,
    /// World units per voxel edge.
    pub voxel_size: f32,
    /// Brick definition drawn for each voxel.
    pub voxel_brick: String,
    /// Chunks kept generated around each player, in chunks.
    pub view_chunks: u32,
    /// The world ends this many chunks from the origin.
    pub radius_chunks: u32,
    /// Palette entries a voxel may use, indexed by the script's `material`.
    pub materials: Vec<Material>,
    /// Seed when the server owner gives none.
    #[serde(default)]
    pub seed: i64,
    /// A base-game map whose sky, light and ground the world stands on,
    /// until packages can provide environments of their own.
    #[serde(default = "default_environment")]
    pub environment: String,
}
fn default_environment() -> String {
    "v20/add-ons/map_slate/slate.mis".into()
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    /// Namespaced id, reported to scripts as a brick's tag.
    pub id: String,
    pub name: String,
    pub color: [f32; 4],
    /// Engine-side protection: operations cannot remove it.
    #[serde(default)]
    pub indestructible: bool,
    /// A package block (`namespace:block/name`) drawn on this material's
    /// voxels in place of `color`.
    #[serde(default)]
    pub block: Option<String>,
}
impl ChunkWorld {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "world schema_version must be 1");
        ensure!(
            identifier(&self.generate),
            "generate must name a script function"
        );
        ensure!(
            (1..=32).contains(&self.chunk_voxels),
            "chunk_voxels must be 1 to 32"
        );
        ensure!(
            self.voxel_size.is_finite() && (0.25..=16.0).contains(&self.voxel_size),
            "voxel_size must be 0.25 to 16"
        );
        ensure!(text(&self.voxel_brick, 256), "voxel_brick is required");
        ensure!(
            (1..=8).contains(&self.view_chunks),
            "view_chunks must be 1 to 8"
        );
        ensure!(
            (1..=256).contains(&self.radius_chunks),
            "radius_chunks must be 1 to 256"
        );
        ensure!(
            !self.materials.is_empty() && self.materials.len() <= 64,
            "1 to 64 materials"
        );
        for m in &self.materials {
            ensure!(
                bri_package::id::ContentId::parse(&m.id).is_ok(),
                "material id `{}` is not namespace:kind/name",
                m.id
            );
            ensure!(
                text(&m.name, 64) && color(&m.color),
                "material `{}` needs a name and a 0..1 RGBA color",
                m.id
            );
            ensure!(
                m.block
                    .as_ref()
                    .is_none_or(|b| bri_package::id::ContentId::parse(b).is_ok()),
                "material `{}`: block must be namespace:block/name",
                m.id
            );
        }
        Ok(())
    }
    pub fn chunk_size(&self) -> f32 {
        self.chunk_voxels as f32 * self.voxel_size
    }
}

/// A non-player actor kind. Its body is an engine character body; its
/// behaviour is the package's `think(entity)` function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityKind {
    pub schema_version: u32,
    pub name: String,
    /// Model id (`namespace:model/name`) the clients draw.
    pub model: String,
    /// Script function `fn(entity)` run every `think_interval` ticks.
    pub think: String,
    pub think_interval: u32,
    /// Walking speed relative to a player.
    #[serde(default = "one")]
    pub speed: f32,
    /// Body scale relative to a player.
    #[serde(default = "one")]
    pub scale: f32,
    pub health: f32,
    /// Most live entities of this kind.
    pub max_alive: u32,
    /// The body's archetype (a package `archetype` id or v20's
    /// `v20.player.<datablock>`): how it moves and steers, whether a think
    /// or a player (`control`) drives it. Absent: a Blockhead's movement.
    #[serde(default)]
    pub archetype: Option<String>,
}
fn one() -> f32 {
    1.0
}
impl EntityKind {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "entity schema_version must be 1");
        ensure!(text(&self.name, 64), "entity name is required");
        ensure!(
            bri_package::id::ContentId::parse(&self.model).is_ok(),
            "model must be namespace:model/name"
        );
        ensure!(identifier(&self.think), "think must name a script function");
        ensure!(
            (1..=120).contains(&self.think_interval),
            "think_interval must be 1 to 120 ticks"
        );
        ensure!(
            self.speed.is_finite() && (0.0..=4.0).contains(&self.speed),
            "speed must be 0 to 4"
        );
        ensure!(
            self.scale.is_finite() && (0.2..=4.0).contains(&self.scale),
            "scale must be 0.2 to 4"
        );
        ensure!(
            self.health.is_finite() && self.health > 0.0 && self.health <= 100_000.0,
            "health must be positive"
        );
        ensure!(
            (1..=256).contains(&self.max_alive),
            "max_alive must be 1 to 256"
        );
        ensure!(
            self.archetype.as_ref().is_none_or(|a| text(a, 160)),
            "archetype must name an archetype"
        );
        Ok(())
    }
}

/// A player archetype: what a player is. It starts from `base` (an
/// archetype id; v20's standard player by default) and overrides only what
/// it names. `movement` takes any motor constant by name (`gravity`,
/// `forward`, `body`: `box` or `ball`, ...); the engine checks the result.
///
/// With `adjusts` it is no new archetype: it changes the named fields of one
/// of v20's own (`v20.player.playernojet`) for everyone while the Add-On is
/// on, as a v20 Add-On's `PlayerNoJet.maxStepHeight = 1.2;` did. Players of
/// other types are untouched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchetypeDef {
    pub schema_version: u32,
    /// Shown in menus; empty when players cannot pick it.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base: Option<String>,
    /// A v20 player type (`v20.player.<datablock>`) this changes in place.
    #[serde(default)]
    pub adjusts: Option<String>,
    #[serde(default)]
    pub movement: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub max_health: Option<f32>,
    #[serde(default)]
    pub energy_bar: Option<bool>,
    #[serde(default)]
    pub rideable: Option<bool>,
    #[serde(default)]
    pub can_ride: Option<bool>,
    /// Rider seats (`numMountPoints`, `mountNode`, `mountThread`), replacing
    /// the base's. A rideable archetype needs at least one to be mounted.
    #[serde(default)]
    pub mount_points: Option<Vec<MountPointDef>>,
    /// Model id the clients draw: a package model (`namespace:model/name`)
    /// or one of v20's shapes (`v20.shape.m` is the Blockhead).
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub camera_distance: Option<f32>,
}
/// One rider seat: the model node riders follow, its rest position from
/// the feet (facing -Z, at scale 1) and the rider's action (`root`, `sit`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountPointDef {
    pub node: String,
    pub position: [f32; 3],
    #[serde(default = "root_pose")]
    pub pose: String,
}
fn root_pose() -> String {
    "root".into()
}
impl ArchetypeDef {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "archetype schema_version must be 1"
        );
        ensure!(
            self.name.is_empty() || text(&self.name, 64),
            "archetype name must be at most 64 printable characters"
        );
        ensure!(
            self.movement.len() <= 64 && self.movement.keys().all(|k| identifier(k)),
            "movement names motor constants"
        );
        ensure!(
            self.model.as_ref().is_none_or(|m| text(m, 160)),
            "model must be a model id"
        );
        if let Some(stock) = &self.adjusts {
            ensure!(
                stock.starts_with("v20.player.") && text(stock, 160),
                "adjusts names one of v20's player types (v20.player.<datablock>)"
            );
            ensure!(
                self.base.is_none() && self.name.is_empty(),
                "an adjustment has no base or name of its own"
            );
        }
        Ok(())
    }
}

/// A PNG texture's size, read from its header: clients decode it, the
/// package loader only checks it is a PNG of a drawable size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
}
impl Texture {
    pub const MAX_EDGE: u32 = 1024;
    pub fn read(bytes: &[u8]) -> Result<Self> {
        const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        ensure!(
            bytes.len() >= 24 && bytes[..8] == SIGNATURE && &bytes[12..16] == b"IHDR",
            "a texture must be a PNG image"
        );
        let edge = |at: usize| {
            u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let (width, height) = (edge(16), edge(20));
        ensure!(
            (1..=Self::MAX_EDGE).contains(&width) && (1..=Self::MAX_EDGE).contains(&height),
            "a texture is 1 to {} pixels on each edge, not {width}x{height}",
            Self::MAX_EDGE
        );
        Ok(Self { width, height })
    }
}

/// One face of a block: a texture id, or a flipbook of texture ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FaceLook {
    Texture(String),
    Flipbook(Flipbook),
}
/// Frames shown in turn at `fps`; `once` holds the last frame instead of
/// looping (a crack that spreads, then stays).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flipbook {
    pub frames: Vec<String>,
    pub fps: f32,
    #[serde(default)]
    pub once: bool,
}
impl FaceLook {
    /// Every texture id this face uses.
    pub fn textures(&self) -> impl Iterator<Item = &String> {
        match self {
            Self::Texture(t) => std::slice::from_ref(t).iter(),
            Self::Flipbook(f) => f.frames.iter(),
        }
    }
    /// The texture shown `seconds` after the face began showing.
    pub fn frame(&self, seconds: f32) -> &str {
        match self {
            Self::Texture(t) => t,
            Self::Flipbook(f) => {
                let n = f.frames.len();
                let i = (seconds.max(0.0) * f.fps) as usize;
                let i = if f.once { i.min(n - 1) } else { i % n };
                &f.frames[i]
            }
        }
    }
    fn validate(&self) -> Result<()> {
        if let Self::Flipbook(f) = self {
            ensure!(
                (1..=64).contains(&f.frames.len()),
                "a flipbook has 1 to 64 frames"
            );
            ensure!(
                f.fps.is_finite() && (0.5..=60.0).contains(&f.fps),
                "a flipbook runs at 0.5 to 60 fps"
            );
        }
        for t in self.textures() {
            ensure!(
                bri_package::id::ContentId::parse(t).is_ok(),
                "`{t}` must be a texture id, namespace:texture/name"
            );
        }
        Ok(())
    }
}
/// Faces by name: `all`, `side` (the four walls), or one of `top`,
/// `bottom`, `north`, `south`, `east`, `west`. The most specific wins.
pub type Faces = BTreeMap<String, FaceLook>;
pub const FACE_NAMES: [&str; 8] = [
    "all", "side", "top", "bottom", "north", "south", "east", "west",
];
/// A block: what each face shows, and named states that replace some faces
/// (a dig tool sets `cracking`, the server's rules decide).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockDef {
    pub schema_version: u32,
    pub name: String,
    pub faces: Faces,
    #[serde(default)]
    pub states: BTreeMap<String, Faces>,
}
impl BlockDef {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "block schema_version must be 1");
        ensure!(text(&self.name, 64), "block name is required");
        ensure!(self.states.len() <= 32, "a block has at most 32 states");
        for (state, faces) in std::iter::once(("", &self.faces))
            .chain(self.states.iter().map(|(k, v)| (k.as_str(), v)))
        {
            ensure!(
                state.is_empty() || identifier(state),
                "state `{state}` must be an identifier"
            );
            for (face, look) in faces {
                ensure!(
                    FACE_NAMES.contains(&face.as_str()),
                    "unknown face `{face}`: use one of {FACE_NAMES:?}"
                );
                look.validate()?;
            }
        }
        for face in ["top", "bottom", "north", "south", "east", "west"] {
            ensure!(
                self.look(face, "").is_some(),
                "the block's own faces must cover `{face}` (use `all` or `side`)"
            );
        }
        Ok(())
    }
    /// What `face` shows in `state`: the state's most specific face, else
    /// the block's own.
    pub fn look(&self, face: &str, state: &str) -> Option<&FaceLook> {
        fn pick<'a>(faces: &'a Faces, face: &str) -> Option<&'a FaceLook> {
            let wall = matches!(face, "north" | "south" | "east" | "west");
            faces
                .get(face)
                .or_else(|| wall.then(|| faces.get("side")).flatten())
                .or_else(|| faces.get("all"))
        }
        self.states
            .get(state)
            .and_then(|faces| pick(faces, face))
            .or_else(|| pick(&self.faces, face))
    }
    /// Every texture id any face or state uses.
    pub fn textures(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.faces)
            .chain(self.states.values())
            .flat_map(|faces| faces.values().flat_map(FaceLook::textures))
    }
}

/// Boxes in the entity's frame: feet at the origin, facing -Z.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoxModel {
    pub schema_version: u32,
    pub boxes: Vec<ModelBox>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelBox {
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub color: [f32; 4],
    /// Colour while the entity's label equals this key, e.g. a flashing fuse.
    #[serde(default)]
    pub label_colors: BTreeMap<String, [f32; 4]>,
}
impl BoxModel {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "model schema_version must be 1");
        ensure!(
            !self.boxes.is_empty() && self.boxes.len() <= 64,
            "1 to 64 boxes"
        );
        for b in &self.boxes {
            ensure!(
                b.center.iter().all(|v| v.is_finite() && v.abs() <= 16.0)
                    && b.size
                        .iter()
                        .all(|v| v.is_finite() && *v > 0.0 && *v <= 16.0)
                    && color(&b.color)
                    && b.label_colors.values().all(color)
                    && b.label_colors.len() <= 8,
                "box needs a finite center, positive size up to 16 and 0..1 colors"
            );
        }
        Ok(())
    }
}

/// A HUD panel drawn by the client from replicated package state. It is
/// data: labels bound to state keys, never code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudPanel {
    pub schema_version: u32,
    /// Named HUD slot; today `hud.overlay`.
    pub slot: String,
    pub anchor: Anchor,
    pub title: String,
    pub background: [f32; 4],
    pub accent: [f32; 4],
    pub text: [f32; 4],
    pub rows: Vec<HudRow>,
    /// Keys that send a package command, shown as hints on the panel.
    #[serde(default)]
    pub keys: Vec<HudKey>,
    /// Show the panel only while the viewer holds one of these in hand: a
    /// weapons package's id (any of its images) or an image id. Empty: the
    /// panel always shows. An ammo counter lists its guns' package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holding: Vec<String>,
}
impl HudPanel {
    /// Whether the panel shows while the viewer holds `image` (`""` for
    /// empty hands).
    pub fn shows_holding(&self, image: &str) -> bool {
        self.holding.is_empty()
            || (!image.is_empty()
                && self.holding.iter().any(|h| {
                    h.eq_ignore_ascii_case(image)
                        || image
                            .split_once(':')
                            .is_some_and(|(package, _)| package.eq_ignore_ascii_case(h))
                }))
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudKey {
    /// One letter `A`-`Z`. The client refuses keys the base game already uses.
    pub key: String,
    pub label: String,
    pub package: String,
    /// A command the package's behaviour declares (with no arguments).
    pub command: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudRow {
    pub label: String,
    /// `package:player/key` for the viewing player's value, or
    /// `package:global/key`.
    pub bind: String,
    #[serde(default)]
    pub color: Option<[f32; 4]>,
}
pub const HUD_SLOTS: [&str; 1] = ["hud.overlay"];
impl HudPanel {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "hud schema_version must be 1");
        ensure!(
            HUD_SLOTS.contains(&self.slot.as_str()),
            "unknown HUD slot `{}`; known: {}",
            self.slot,
            HUD_SLOTS.join(", ")
        );
        ensure!(text(&self.title, 48), "title must be 1 to 48 characters");
        ensure!(
            color(&self.background) && color(&self.accent) && color(&self.text),
            "colors must be 0..1 RGBA"
        );
        ensure!(
            !self.rows.is_empty() && self.rows.len() <= 16,
            "1 to 16 rows"
        );
        for row in &self.rows {
            ensure!(text(&row.label, 32), "row label must be 1 to 32 characters");
            ensure!(
                Binding::parse(&row.bind).is_some(),
                "bind `{}` is not package:player/key or package:global/key",
                row.bind
            );
            ensure!(
                row.color.as_ref().is_none_or(color),
                "row color must be 0..1 RGBA"
            );
        }
        ensure!(self.keys.len() <= 8, "at most 8 keys");
        ensure!(self.holding.len() <= 32, "at most 32 holding entries");
        for h in &self.holding {
            ensure!(
                bri_package::id::namespace_problem(h).is_none()
                    || bri_package::id::ContentId::parse(h).is_ok_and(|id| id.kind == "image"),
                "holding `{h}` is neither a package id nor an image id"
            );
        }
        for k in &self.keys {
            ensure!(
                k.key.len() == 1 && k.key.chars().all(|c| c.is_ascii_uppercase()),
                "key `{}` must be one letter A-Z",
                k.key
            );
            ensure!(text(&k.label, 24), "key label must be 1 to 24 characters");
            ensure!(
                bri_package::id::namespace_problem(&k.package).is_none() && identifier(&k.command),
                "key needs a package id and a command name"
            );
        }
        Ok(())
    }
}
/// A parsed state binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub package: String,
    pub scope: Scope,
    pub key: String,
}
/// Whose value a binding shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `package:global/key`: the server-wide value.
    Global,
    /// `package:player/key`: the viewing player's own value.
    Player,
    /// `package:players/key`: every player's value, one line each (a
    /// scoreboard). The key must be visible to everyone.
    Players,
}
impl Binding {
    pub fn parse(bind: &str) -> Option<Self> {
        let id = bri_package::id::ContentId::parse(bind).ok()?;
        let (package, key) = (id.namespace, id.name);
        let scope = match id.kind.as_str() {
            "player" => Scope::Player,
            "players" => Scope::Players,
            "global" => Scope::Global,
            _ => return None,
        };
        identifier(&key).then_some(Self {
            package,
            scope,
            key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        bytes.extend(b"IHDR");
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes
    }

    #[test]
    fn a_panel_can_show_only_while_its_guns_are_held() {
        let mut panel: HudPanel = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "slot": "hud.overlay", "anchor": "bottom_right",
            "title": "AMMO", "background": [0, 0, 0, 0.5], "accent": [1, 1, 1, 1],
            "text": [1, 1, 1, 1],
            "rows": [{ "label": "Rounds", "bind": "guns-rules:player/ammo" }],
            "holding": ["guns", "tools:image/scope"]
        }))
        .unwrap();
        panel.validate().unwrap();
        assert!(panel.shows_holding("guns:image/rifle"), "any image of the package");
        assert!(panel.shows_holding("tools:image/scope"), "one named image");
        assert!(!panel.shows_holding("tools:image/hammer"));
        assert!(!panel.shows_holding("gunsmith:image/rifle"), "not a prefix match");
        assert!(!panel.shows_holding(""), "empty hands");
        panel.holding.clear();
        assert!(panel.shows_holding(""), "no list: always shown");
        panel.holding = vec!["Not An Id".into()];
        assert!(panel.validate().is_err());
        panel.holding = vec!["guns:weapon/rifle".into()];
        assert!(panel.validate().is_err(), "an item is not an image");
    }

    #[test]
    fn textures_are_pngs_of_a_drawable_size() {
        assert_eq!(
            Texture::read(&png(16, 32)).unwrap(),
            Texture {
                width: 16,
                height: 32
            }
        );
        assert!(Texture::read(&png(4096, 16)).is_err());
        assert!(Texture::read(&png(0, 16)).is_err());
        assert!(Texture::read(b"GIF89a not a png at all....").is_err());
    }

    #[test]
    fn a_block_covers_every_face_and_names_only_known_faces() {
        let block = |faces: serde_json::Value| -> Result<BlockDef> {
            let b: BlockDef = serde_json::from_value(serde_json::json!({
                "schema_version": 1, "name": "Test", "faces": faces
            }))?;
            b.validate()?;
            Ok(b)
        };
        assert!(block(serde_json::json!({ "all": "a:texture/x" })).is_ok());
        assert!(
            block(serde_json::json!({ "top": "a:texture/x", "side": "a:texture/y" })).is_err(),
            "the bottom is not covered"
        );
        assert!(
            block(serde_json::json!({ "all": "a:texture/x", "front": "a:texture/y" })).is_err()
        );
        assert!(
            block(serde_json::json!({ "all": { "frames": [], "fps": 4 } })).is_err(),
            "a flipbook needs frames"
        );
        assert!(
            block(serde_json::json!({ "all": { "frames": ["a:texture/x"], "fps": 900 } })).is_err()
        );
    }
}
