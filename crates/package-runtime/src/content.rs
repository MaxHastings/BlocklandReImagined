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
}
impl Kind {
    pub const NAMES: [&str; 6] = ["behaviour", "script", "world", "entity", "model", "hud"];
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "behaviour" => Self::Behaviour,
            "script" => Self::Script,
            "world" => Self::World,
            "entity" => Self::Entity,
            "model" => Self::Model,
            "hud" => Self::Hud,
            _ => return None,
        })
    }
    pub fn side(self) -> Side {
        match self {
            Self::Behaviour | Self::Script | Self::World | Self::Entity => Side::Server,
            Self::Model | Self::Hud => Side::Client,
        }
    }
    /// Largest accepted file of this kind.
    pub fn max_bytes(self) -> usize {
        match self {
            Self::Script => 256 * 1024,
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
    /// `on_tick()` every `tick_interval` ticks, when set.
    #[serde(default)]
    pub tick_interval: Option<u32>,
}
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
    /// Replicated to every client. Private keys stay on the server.
    #[serde(default)]
    pub public: bool,
    /// Saved by the host and restored after a restart.
    #[serde(default = "yes")]
    pub persist: bool,
}
fn yes() -> bool {
    true
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
        Ok(())
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
    pub player: bool,
    pub key: String,
}
impl Binding {
    pub fn parse(bind: &str) -> Option<Self> {
        let id = bri_package::id::ContentId::parse(bind).ok()?;
        let (package, key) = (id.namespace, id.name);
        let player = match id.kind.as_str() {
            "player" => true,
            "global" => false,
            _ => return None,
        };
        identifier(&key).then_some(Self {
            package,
            player,
            key,
        })
    }
}
