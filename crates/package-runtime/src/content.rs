//! The content kinds a package may provide, and their typed definitions.
//! Each kind declares which side needs it: server-only kinds (behaviour,
//! scripts, world providers) never leave the host.
use anyhow::{Result, ensure};
use bri_package::packages::Side;
pub use bri_package::setting::{SettingDef, SettingItems};
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
    /// the checkpoint and predict from it. Being data, a shared package may
    /// carry it too (an imported Add-On's player types).
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
    /// Keys players can bind to packages' commands in Options → Controls
    /// (`binds.json`). Client side, like HUD panels.
    Binds,
}
impl Kind {
    pub const NAMES: [&str; 15] = [
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
        "binds",
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
            "binds" => Self::Binds,
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
            | Self::Bots
            | Self::Binds => Side::Client,
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
    /// `on_path_node(player, knot)` as a camera path this package gave the
    /// player (`follow_path`) reaches each knot, from 0
    /// (`PathCameraData::onNode`).
    #[serde(default)]
    pub on_path_node: bool,
    /// `on_observer(player, button)` as a spectator presses a key: a dead
    /// player whose respawn a rule holds, or one under a rules camera
    /// (`watch`, `follow_path`, `free_camera`, `orbit_point`). `button` is
    /// `"fire"`, `"jump"`, `"jet"` or `"light"` (`Observer::onTrigger`'s
    /// triggers 0, 2 and 4, and `serverCmdLight`). Return `true` to take
    /// it; every package that declares it is asked, in load order, until
    /// one does. Called as it happens, so it must be quick.
    #[serde(default)]
    pub on_observer: bool,
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
    /// `on_minigame(event)` after something happens to a mini-game:
    /// `event` is `#{ kind, game, player, team }`, `kind` being `created`,
    /// `configured`, `reset`, `ended`, `joined`, `left`, `team` (a member's
    /// team changed; `team` is the new one or `()`) or `teams` (the game's
    /// team list changed). Delivered at the start of the next tick.
    #[serde(default)]
    pub on_minigame: bool,
    /// `on_pick_spawn(player)` as a player is about to (re)spawn: return a
    /// brick id to appear on that brick, `[x, y, z]` to appear there, or
    /// `()` to leave the choice to the engine (spawn bricks, then the map).
    /// The first package answering decides. Called as it happens, so it
    /// must be quick (Slayer's team spawns).
    #[serde(default)]
    pub on_pick_spawn: bool,
    /// Touch zones over bricks (Torque triggers a brick made with
    /// `createTrigger`): `on_zone(player, brick, event)` as a living player
    /// enters (`"enter"`), stays in (`"tick"`, when `ticks` is set) or
    /// leaves (`"leave"`) the box over a brick of one of `bricks`.
    #[serde(default)]
    pub zones: Vec<ZoneDef>,
    /// `on_copy(player, info)` after this package's `copy_build` or
    /// `copy_box` for `player`: `info` is `#{ bricks, limit_reached,
    /// refused, error, message }`, `error` being `()` or why nothing was
    /// copied (`trust`, `public`, `empty`, `invalid`) and `message` the
    /// engine's words for it. Declaring it keeps the engine's own message
    /// from the player. Delivered at the start of the next tick.
    #[serde(default)]
    pub on_copy: bool,
    /// `on_place(player, info)` after `player` plants (or fails to plant)
    /// a copy this package gave them: `info` is `#{ planted, bricks,
    /// error, message }`, `error` being `()` or the plant failure
    /// (`overlap`, `float`, `buried`, `stuck`, `too_far`, `limit`,
    /// `forbidden`, `other`). Declaring it keeps the engine's own message from the
    /// player. Delivered at the start of the next tick.
    #[serde(default)]
    pub on_place: bool,
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
    /// `on_event_row(player, brick, row)` for each row of events a player
    /// sends from the wrench, before they are kept: return `false` or a
    /// reason to leave that row out (Slayer's Restrict Output Events, as
    /// its `serverCmdAddEvent`), anything else to keep it. `row` is
    /// `#{ index, input, target, class, output, package }`.
    #[serde(default)]
    pub on_event_row: bool,
    /// `on_trigger(player, trigger, down)` as a living player with nothing
    /// in their hand presses (`down` true) or lets go of a trigger
    /// (v20's `Armor::onTrigger`). Trigger 0 is fire, the empty-hand click;
    /// its press comes before `on_activate`. Return `true` to take the
    /// press, so the engine does nothing more with it. Every package that
    /// declares it is asked, in load order, until one takes it.
    #[serde(default)]
    pub on_trigger: bool,
    /// `on_drop_key(player)` as a living player with nothing in their hand
    /// presses the Drop Tool key (v20's `serverCmdDropTool` while
    /// `currTool` is -1, which Capture the Flag packaged to drop a carried
    /// flag). Return `true` to take the key. Every package that declares
    /// it is asked, in load order, until one takes it.
    #[serde(default)]
    pub on_drop_key: bool,
    /// `on_tick()` every `tick_interval` ticks, when set.
    #[serde(default)]
    pub tick_interval: Option<u32>,
    /// Engine decisions this package is asked about ([`POLICIES`]). For
    /// each, the engine calls `allow_<policy>(player)` before acting: `true`
    /// allows, `false` or a reason string refuses.
    #[serde(default)]
    pub policies: Vec<String>,
    /// Settings a host edits in the Mini-Game window's Add-On Settings and
    /// these rules read with `setting(game, key)` (Slayer's preferences).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingDef>,
    /// Choices added to a list setting of an Add-On this one depends on
    /// (a game mode joining Slayer's mode picker).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setting_items: Vec<SettingItems>,
    /// Wrench event inputs these rules fire with `fire_brick_input`
    /// (`registerInputEvent`; Slayer_CTF's `onFlagPickedUp`). Builders wire
    /// them to outputs on their bricks like the engine's own inputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub brick_inputs: Vec<BrickInputDef>,
    /// Wrench event outputs these rules carry out (`registerOutputEvent`;
    /// Slayer's `setTeamControl`): builders pick them like the engine's
    /// own, and a row that runs one calls
    /// `on_brick_output(output, target, params, info)`. Needs the
    /// `brick_events` capability.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub brick_outputs: Vec<BrickOutputDef>,
    /// Wrench event targets these rules resolve (`registerEventTarget`;
    /// Slayer's `Team(Client)`): every input with the target's `from` slot
    /// offers it, and its rows run this behaviour's `brick_outputs` of the
    /// target's class, `on_brick_output` getting the `from` entity and the
    /// target's name in `info.target`. Needs the `brick_events` capability.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub brick_targets: Vec<BrickTargetDef>,
    /// When set, a player's undo (Ctrl+Z) of a step this package's copy
    /// ops made (a plant, paint, wrench, cut or fill) that changed more
    /// bricks than this is held the first time: `on_copy` hears `action`
    /// `"undo"` with the `bricks` it would change, and the next undo goes
    /// ahead. Any other undo between starts over.
    #[serde(default)]
    pub undo_confirm_over: Option<u32>,
    /// `on_minigame_request(player, action, info)` before the engine acts
    /// on a player's mini-game request: `action` is `create`, `join`,
    /// `leave`, `edit` (the Mini-Game window's settings), `reset`,
    /// `respawn_all`, `end`, `invite`, `kick` or `ignore` (ignoring an
    /// invitation); `info` is `#{ game, target }`, `game` the game acted on
    /// (the player's own, or the one they join) and `target` the player
    /// invited or kicked. Return `()` to leave it to the engine (a game's
    /// owner runs it), `true` to let the player do it to that game though
    /// they do not own it (Slayer's Edit and Reset Rights; a join skips
    /// invite-only and the join wait), `false` or a reason to refuse, or
    /// `#{ title, text }` to refuse in a message box. A refused `ignore`
    /// still turns the invitation down, without ignoring the owner. Every
    /// package that declares it is asked, in load order; the first refusal
    /// stands. Called as it happens, so it must be quick.
    #[serde(default)]
    pub on_minigame_request: bool,
    /// `on_chat(player, info)` as a player sends a chat line, after the
    /// engine's flood and mute checks: `info` is `#{ text, team }`, `team`
    /// true for team chat. Return `()` to send it as usual, `false` or a
    /// reason to drop it (the reason goes to the sender), or `#{ line, to }`
    /// to send `line` as written (the sender's name included) to the players
    /// in `to` instead (Slayer's Team Display Mode and dead talking). The
    /// first package answering decides. Called as it happens, so it must be
    /// quick.
    #[serde(default)]
    pub on_chat: bool,
    /// `on_death_message(victim, killer, info)` as the engine is about to
    /// tell a mini-game a player died: `info` is `#{ type, suicide, bot }`.
    /// Return `()` for the engine's line, `false` to send none, or a map of
    /// what to change: `victim` and `killer` (the names as shown, colour
    /// codes allowed), `suffix` (text after the line, Slayer's
    /// `(Killing Spree | 5)`) and `to` (who hears it). Called as it
    /// happens, so it must be quick.
    #[serde(default)]
    pub on_death_message: bool,
    /// Brick kinds (`namespace:brick/name`, `v20/brick/<datablock>`, or
    /// `*` for every brick) whose changes these rules hear as
    /// `on_brick(event, brick, player)`: `event` is `planted` (by a
    /// player), `loaded` (from a build), `painted`, `named` or `removed`;
    /// `player` who did it, or `()`. Delivered at the start of the next
    /// tick, except `removed`, which comes while the brick can still be
    /// read (Slayer's `slayerPrepareBrick`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_brick: Vec<String>,
    /// `on_ride(player, info)` as a player is about to board a vehicle the
    /// mini-game rules let them use: `info` is `#{ vehicle, owner,
    /// spawn_brick }`. Return `()` or `true` to let them, `false` or a
    /// reason to keep them off (the reason is printed in the middle of
    /// their screen; Slayer's Team Vehicle spawns). Called as it happens,
    /// so it must be quick.
    #[serde(default)]
    pub on_ride: bool,
    /// How mini-games' own settings start and how far they may go while
    /// these rules run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minigame_settings: Option<MinigameSettingsDef>,
    /// Least ticks between two of these rules' commands from one player,
    /// whichever they are (Slayer's `isSpamming`): on top of each
    /// command's own `cooldown_ticks`.
    #[serde(default)]
    pub command_cooldown_ticks: u32,
}
/// A mini-game's own settings as these rules start them and bound them
/// ([`Behaviour::minigame_settings`]); what is left out stays v20's.
/// Seconds throughout, as the Mini-Game window shows them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinigameSettingsDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Longest title, in characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub respawn: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub respawn_range: Option<[u32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle_respawn: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle_respawn_range: Option<[u32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brick_respawn: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brick_respawn_range: Option<[u32; 2]>,
    /// A brick respawn time of -1 keeps knocked-out bricks out until the
    /// next reset.
    #[serde(default)]
    pub brick_never: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_wand: Option<bool>,
    /// The five start tools, `""` for an empty slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loadout: Option<[String; 5]>,
}
impl MinigameSettingsDef {
    pub fn validate(&self) -> Result<()> {
        let range = |r: Option<[u32; 2]>, floor: u32, what: &str| -> Result<()> {
            if let Some([lo, hi]) = r {
                ensure!(
                    floor <= lo && lo <= hi && hi <= 999,
                    "minigame_settings: {what} runs {floor} to 999 seconds, low to high"
                );
            }
            Ok(())
        };
        range(self.respawn_range, 1, "respawn_range")?;
        range(self.vehicle_respawn_range, 0, "vehicle_respawn_range")?;
        range(self.brick_respawn_range, 0, "brick_respawn_range")?;
        if let Some(n) = self.title_length {
            ensure!((1..=256).contains(&n), "minigame_settings: title_length is 1 to 256");
        }
        if let Some(t) = &self.title {
            ensure!(
                !t.trim().is_empty()
                    && t.chars().count() <= self.title_length.unwrap_or(35) as usize
                    && !t.chars().any(char::is_control),
                "minigame_settings: title must fit its length"
            );
        }
        for (what, v) in [
            ("respawn", self.respawn),
            ("vehicle_respawn", self.vehicle_respawn),
            ("brick_respawn", self.brick_respawn),
        ] {
            ensure!(v.is_none_or(|v| v <= 999), "minigame_settings: {what} is at most 999");
        }
        for item in self.loadout.iter().flatten() {
            ensure!(
                item.is_empty() || bri_package::id::is_content_ref(item, None),
                "minigame_settings: loadout item `{item}` is not a content id"
            );
        }
        Ok(())
    }
}
/// Most wrench event inputs one behaviour declares.
pub const MAX_BRICK_INPUTS: usize = 32;
/// Targets an Add-On's input may offer besides `Self`, the brick, with the
/// class each one is (`registerInputEvent`'s target list). `OwnerPlayer`
/// and `OwnerClient` are the brick owner's, while they are on the server;
/// `Player(Killer)` and `Client(Killer)` whoever killed the player an input
/// is about (`fire_game_input`'s `killer`).
pub const BRICK_INPUT_TARGETS: [(&str, &str); 7] = [
    ("Player", "Player"),
    ("Client", "GameConnection"),
    ("MiniGame", "MiniGame"),
    ("OwnerPlayer", "Player"),
    ("OwnerClient", "GameConnection"),
    ("Player(Killer)", "Player"),
    ("Client(Killer)", "GameConnection"),
];
/// A wrench event input: its name as builders pick it, and the targets its
/// rows may aim at besides the brick itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrickInputDef {
    pub name: String,
    #[serde(default)]
    pub targets: Vec<String>,
    /// One of the engine's inputs this one may follow
    /// (`onPlayerTouch`, `onActivate`): when a player sets that input off on
    /// a brick with rows on this one, the rules' `on_brick_input` decides
    /// whether this one runs too (Slayer's `onPlayerTouch(Team1)`). Needs
    /// the `brick_events` capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follows: Option<String>,
}
impl BrickInputDef {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (3..=64).contains(&self.name.len())
                && self.name.starts_with("on")
                && self
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_()".contains(c)),
            "brick input `{}`: a name like onFlagPickedUp, 3 to 64 letters, digits, _ or ()",
            self.name
        );
        if let Some(follows) = &self.follows {
            ensure!(
                (3..=64).contains(&follows.len())
                    && follows.starts_with("on")
                    && follows.chars().all(|c| c.is_ascii_alphanumeric()),
                "brick input `{}`: follows `{follows}`, which is not an input name like onPlayerTouch",
                self.name
            );
        }
        for (i, t) in self.targets.iter().enumerate() {
            ensure!(
                BRICK_INPUT_TARGETS.iter().any(|(slot, _)| slot == t),
                "brick input `{}`: target `{t}` is not one of {}",
                self.name,
                BRICK_INPUT_TARGETS.map(|(s, _)| s).join(", ")
            );
            ensure!(
                !self.targets[..i].contains(t),
                "brick input `{}`: target `{t}` listed twice",
                self.name
            );
        }
        Ok(())
    }
}
/// Most wrench event targets one behaviour declares.
pub const MAX_BRICK_TARGETS: usize = 8;
/// Classes of the engine's own targets, which an Add-On target's class is
/// not.
const NATIVE_CLASSES: [&str; 6] = [
    "fxDTSBrick",
    "Player",
    "GameConnection",
    "MiniGame",
    "Projectile",
    "Vehicle",
];
/// A wrench event target an Add-On adds to the inputs: its name as
/// builders pick it (`Team(Client)`), the class of thing it stands for
/// (`Slayer_TeamSO`), which only this behaviour's outputs act on, and the
/// slot it is found from: `Self`, the brick, or one of
/// [`BRICK_INPUT_TARGETS`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrickTargetDef {
    pub name: String,
    pub class: String,
    pub from: String,
}
impl BrickTargetDef {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=64).contains(&self.name.len())
                && self
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_()".contains(c))
                && self.name != "Self"
                && !BRICK_INPUT_TARGETS
                    .iter()
                    .chain(&[("Projectile", ""), ("Bot", ""), ("Driver", ""), ("Ball", "")])
                    .any(|(slot, _)| slot.eq_ignore_ascii_case(&self.name)),
            "brick target `{}`: a name like Team(Client), 1 to 64 letters, digits, _ or (), \
             not one of the engine's own",
            self.name
        );
        ensure!(
            (1..=64).contains(&self.class.len())
                && self
                    .class
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !NATIVE_CLASSES
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(&self.class)),
            "brick target `{}`: class `{}` must be the Add-On's own, like Slayer_TeamSO",
            self.name,
            self.class
        );
        ensure!(
            self.from == "Self" || BRICK_INPUT_TARGETS.iter().any(|(slot, _)| *slot == self.from),
            "brick target `{}`: from `{}` is not Self or one of {}",
            self.name,
            self.from,
            BRICK_INPUT_TARGETS.map(|(s, _)| s).join(", ")
        );
        Ok(())
    }
}
/// Most wrench event outputs one behaviour declares, and parameters one
/// output takes (the wrench's four).
pub const MAX_BRICK_OUTPUTS: usize = 32;
pub const MAX_OUTPUT_PARAMS: usize = 4;
/// The classes an Add-On's output may act on: what a row's target is.
pub const BRICK_OUTPUT_CLASSES: [&str; 4] = ["fxDTSBrick", "Player", "GameConnection", "MiniGame"];
/// A wrench event output: its name, the kind of thing it acts on, and the
/// parameters a builder fills in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrickOutputDef {
    pub name: String,
    /// One of [`BRICK_OUTPUT_CLASSES`], or the class of one of the
    /// behaviour's `brick_targets`.
    pub class: String,
    #[serde(default)]
    pub params: Vec<OutputParam>,
}
/// One field of an output row, as the wrench shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutputParam {
    /// A whole number from `min` to `max`.
    Int { min: i64, max: i64, default: i64 },
    /// A number from `min` to `max` in steps of `step`.
    Float {
        min: f32,
        max: f32,
        step: f32,
        default: f32,
    },
    Bool,
    /// Text of at most `max_length` characters in a box `width` wide.
    String { max_length: u32, width: i32 },
    /// A colour of the server's palette.
    PaintColor { default: u8 },
    /// One of named choices, each with the number the rules receive.
    List { items: Vec<(String, i64)> },
    /// A vector at most `max_length` long.
    Vector { max_length: f32 },
}
impl OutputParam {
    fn valid(&self) -> bool {
        match self {
            Self::Int { min, max, default } => {
                min <= default && default <= max && *min >= -1_000_000 && *max <= 1_000_000
            }
            Self::Float {
                min,
                max,
                step,
                default,
            } => {
                [min, max, step, default].iter().all(|v| v.is_finite())
                    && min <= default
                    && default <= max
                    && *step > 0.
                    && *step <= 10_000.
            }
            Self::Bool | Self::PaintColor { .. } => true,
            Self::String { max_length, width } => *max_length <= 4096 && (0..=1024).contains(width),
            Self::List { items } => {
                !items.is_empty()
                    && items.len() <= 256
                    && items.iter().all(|(s, _)| !s.is_empty() && s.len() <= 128)
            }
            Self::Vector { max_length } => {
                max_length.is_finite() && *max_length > 0. && *max_length <= 10_000.
            }
        }
    }
}
impl BrickOutputDef {
    /// Checks the output; `classes` are the behaviour's own target classes
    /// it may act on besides [`BRICK_OUTPUT_CLASSES`].
    pub fn validate(&self, classes: &[&str]) -> Result<()> {
        ensure!(
            (1..=64).contains(&self.name.len())
                && self.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "brick output `{}`: a name like setTeamControl, 1 to 64 letters, digits or _",
            self.name
        );
        ensure!(
            BRICK_OUTPUT_CLASSES.contains(&self.class.as_str())
                || classes.contains(&self.class.as_str()),
            "brick output `{}`: class `{}` is not one of {} or a brick_targets class",
            self.name,
            self.class,
            BRICK_OUTPUT_CLASSES.join(", ")
        );
        ensure!(
            self.params.len() <= MAX_OUTPUT_PARAMS,
            "brick output `{}`: at most {MAX_OUTPUT_PARAMS} params",
            self.name
        );
        for p in &self.params {
            ensure!(
                p.valid(),
                "brick output `{}`: parameter {p:?} is out of range",
                self.name
            );
        }
        Ok(())
    }
}
/// Most touch zones one behaviour declares, and brick kinds one zone names.
pub const MAX_ZONES: usize = 16;
pub const MAX_ZONE_KINDS: usize = 16;
/// A touch zone: the box over every brick of some kinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneDef {
    /// Brick definitions (`namespace:brick/name` of this package or one it
    /// depends on, or `v20/brick/<datablock>`).
    pub bricks: Vec<String>,
    /// How far the box reaches above the brick's top, units. v20's
    /// `createTrigger` reached 0.2 above it.
    #[serde(default = "ZoneDef::default_above")]
    pub above: f32,
    /// How often the zone is checked, milliseconds: a Torque trigger's
    /// `tickPeriodMS` (100 unless the trigger's datablock says otherwise),
    /// 10 to 10000, rounded up to whole ticks.
    #[serde(default = "ZoneDef::default_period")]
    pub period_ms: u32,
    /// Also call `on_zone(player, brick, "tick")` for players staying in.
    #[serde(default)]
    pub ticks: bool,
}
impl ZoneDef {
    fn default_above() -> f32 {
        0.2
    }
    fn default_period() -> u32 {
        100
    }
    /// The check period in ticks (120 a second), at least one.
    pub fn period_ticks(&self) -> u32 {
        Self::ticks_of(self.period_ms)
    }
    /// `period_ms` in ticks, at least one.
    pub fn ticks_of(period_ms: u32) -> u32 {
        (period_ms * 120).div_ceil(1000).max(1)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.bricks.is_empty() && self.bricks.len() <= MAX_ZONE_KINDS,
            "a zone names 1 to {MAX_ZONE_KINDS} brick kinds"
        );
        for b in &self.bricks {
            ensure!(
                bri_package::id::is_content_ref(b, Some("brick")) || b.starts_with("v20/brick/"),
                "zone brick `{b}` is not a brick id"
            );
        }
        ensure!(
            self.above.is_finite() && (0.0..=64.0).contains(&self.above),
            "a zone's `above` must be 0 to 64"
        );
        ensure!(
            (10..=10_000).contains(&self.period_ms),
            "a zone's period_ms must be 10 to 10000"
        );
        Ok(())
    }
}
/// Farthest a command's `aim_reach` looks: the New Duplicator selected
/// bricks up to 1000 units away.
pub const MAX_AIM_REACH: f32 = 1000.0;
/// Decisions the engine owns the mechanism for and asks packages about.
pub const POLICIES: &[&str] = &[
    // A dead player asking to come back.
    "respawn", // Any command that builds (plant, paint, wand, wrench edits).
    "build",
    // Taking a tool, spray can or FX can into the hand, or putting it away
    // (`serverCmdUseTool`, `serverCmdUnUseTool`, `serverCmdUseSprayCan`,
    // `serverCmdUseFXCan`).
    "equip",
    // The light key (`serverCmdLight`).
    "light",
    // `/suicide` and its key (`serverCmdSuicide`).
    "suicide",
    // An administrator's F7 and F8 (`serverCmdDropPlayerAtCamera`,
    // `serverCmdDropCameraAtPlayer`).
    "admin_camera",
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
    /// this distance (at most [`MAX_AIM_REACH`]) and passes the hit to the
    /// script as `aim()`.
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
    /// A server-wide map from mini-game id (as text) to that game's value:
    /// a game's entry saves with a build saved by whoever runs it and comes
    /// back under the game the build loads into (Slayer's fly-through path,
    /// kept beside its saved mini-game config). The rules remove a game's
    /// entry when it ends, as they keep it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub per_minigame: bool,
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
        ensure!(self.zones.len() <= MAX_ZONES, "at most {MAX_ZONES} zones");
        for z in &self.zones {
            z.validate()?;
        }
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
                    reach.is_finite() && (0.0..=MAX_AIM_REACH).contains(&reach),
                    "aim_reach must be 0 to {MAX_AIM_REACH}"
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
        if let Some(m) = &self.minigame_settings {
            m.validate()?;
        }
        ensure!(self.on_brick.len() <= 64, "on_brick lists at most 64 brick kinds");
        for kind in &self.on_brick {
            ensure!(
                kind == "*"
                    || bri_package::id::is_content_ref(kind, Some("brick"))
                    || kind.starts_with("v20/brick/"),
                "on_brick: `{kind}` is not a brick kind or *"
            );
        }
        ensure!(
            self.command_cooldown_ticks <= 120 * 60,
            "command_cooldown_ticks is at most a minute"
        );
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
        ensure!(
            self.settings.len() <= bri_package::setting::MAX_SETTINGS,
            "at most {} settings",
            bri_package::setting::MAX_SETTINGS
        );
        ensure!(self.setting_items.len() <= 16, "at most 16 setting_items");
        let mut keys = std::collections::BTreeSet::new();
        for def in &self.settings {
            def.validate().map_err(anyhow::Error::msg)?;
            ensure!(keys.insert(&def.key), "setting `{}` declared twice", def.key);
        }
        for def in &self.settings {
            if let Some(when) = &def.shown_when {
                ensure!(
                    when.setting.contains(':') || keys.contains(&when.setting),
                    "setting `{}`: shown_when names `{}`, which is not one of these settings",
                    def.key,
                    when.setting
                );
            }
        }
        for more in &self.setting_items {
            more.validate().map_err(anyhow::Error::msg)?;
        }
        ensure!(
            self.brick_inputs.len() <= MAX_BRICK_INPUTS,
            "at most {MAX_BRICK_INPUTS} brick_inputs"
        );
        for (i, input) in self.brick_inputs.iter().enumerate() {
            input.validate()?;
            ensure!(
                !self.brick_inputs[..i]
                    .iter()
                    .any(|o| o.name.eq_ignore_ascii_case(&input.name)),
                "brick input `{}` declared twice",
                input.name
            );
        }
        ensure!(
            self.brick_outputs.len() <= MAX_BRICK_OUTPUTS,
            "at most {MAX_BRICK_OUTPUTS} brick_outputs"
        );
        ensure!(
            self.brick_targets.len() <= MAX_BRICK_TARGETS,
            "at most {MAX_BRICK_TARGETS} brick_targets"
        );
        for (i, target) in self.brick_targets.iter().enumerate() {
            target.validate()?;
            ensure!(
                !self.brick_targets[..i]
                    .iter()
                    .any(|o| o.name.eq_ignore_ascii_case(&target.name)),
                "brick target `{}` declared twice",
                target.name
            );
        }
        let classes: Vec<&str> = self.brick_targets.iter().map(|t| t.class.as_str()).collect();
        for (i, output) in self.brick_outputs.iter().enumerate() {
            output.validate(&classes)?;
            ensure!(
                !self.brick_outputs[..i].iter().any(|o| o.class == output.class
                    && o.name.eq_ignore_ascii_case(&output.name)),
                "brick output `{}` declared twice for {}",
                output.name,
                output.class
            );
        }
        for (key, def) in &self.state.global {
            ensure!(
                def.visible != Visible::Owner,
                "server-wide key `{key}` has no owner; use \"everyone\" or \"server\""
            );
            ensure!(
                !def.per_minigame || def.default.as_object().is_some_and(|m| m.is_empty()),
                "per-mini-game key `{key}` is a map by mini-game id: its default is {{}}"
            );
        }
        for (key, def) in &self.state.player {
            ensure!(
                !def.per_minigame,
                "player key `{key}` cannot be per mini-game; make it server-wide"
            );
        }
        ensure!(
            self.state.player.len() + self.state.global.len() <= 256,
            "at most 256 state keys"
        );
        if let Some(over) = self.undo_confirm_over {
            ensure!(
                (1..=1_000_000).contains(&over),
                "undo_confirm_over must be 1 to 1000000"
            );
        }
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
    /// `thirdPersonOnly`.
    #[serde(default)]
    pub third_person_only: Option<bool>,
    /// Whether it fires and uses items; `false` for a body that only
    /// waits (`PlayerData::onTrigger` doing nothing).
    #[serde(default)]
    pub uses_items: Option<bool>,
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
/// Keys a player can bind to packages' commands, listed in Options →
/// Controls under `division` (`binds.json`). Data only: a key sends its
/// command to the host as a typed command would.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binds {
    pub schema_version: u32,
    /// The Controls heading the binds go under.
    pub division: String,
    pub binds: Vec<BindDef>,
}
/// Most binds one file may offer.
pub const MAX_BINDS: usize = 32;
impl Binds {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "binds schema_version must be 1");
        ensure!(
            text(&self.division, 64),
            "division must be 1 to 64 characters"
        );
        ensure!(
            !self.binds.is_empty() && self.binds.len() <= MAX_BINDS,
            "1 to {MAX_BINDS} binds"
        );
        for (i, bind) in self.binds.iter().enumerate() {
            ensure!(
                text(&bind.name, 64),
                "bind names are 1 to 64 characters"
            );
            ensure!(
                !self.binds[..i].iter().any(|b| b.name == bind.name),
                "bind `{}` listed twice",
                bind.name
            );
            ensure!(
                bri_package::id::namespace_problem(&bind.package).is_none()
                    && identifier(&bind.command),
                "bind `{}` must name a package and one of its commands",
                bind.name
            );
            for key in bind.key.iter().chain(&bind.mac_key) {
                ensure!(
                    !key.trim().is_empty()
                        && key.len() <= 32
                        && key.chars().all(|c| c.is_ascii_graphic() || c == ' '),
                    "bind `{}` has a bad key `{key}`",
                    bind.name
                );
            }
        }
        Ok(())
    }
}
/// A key a player can bind to a package's command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindDef {
    /// What Controls calls it.
    pub name: String,
    /// The package whose behaviour declares the command.
    pub package: String,
    /// The command it sends.
    pub command: String,
    /// Default key, as Controls writes it (`ctrl c`, `shift-ctrl x`,
    /// `lcontrol`); none leaves it unbound until the player picks one.
    #[serde(default)]
    pub key: Option<String>,
    /// The default key on a Mac, when it differs (`cmd c`).
    #[serde(default)]
    pub mac_key: Option<String>,
    /// Sent with `true` as the key goes down and `false` as it comes up,
    /// to a command taking one `bool`; else sent once as it goes down, to
    /// a command with no arguments.
    #[serde(default)]
    pub hold: bool,
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
    #[test]
    fn a_brick_target_is_the_add_ons_own_and_found_from_a_slot() {
        let target = |name: &str, class: &str, from: &str| {
            BrickTargetDef {
                name: name.into(),
                class: class.into(),
                from: from.into(),
            }
            .validate()
        };
        assert!(target("Team(Client)", "Slayer_TeamSO", "Client").is_ok());
        assert!(target("Team(Brick)", "Slayer_TeamSO", "Self").is_ok());
        assert!(target("Client", "Slayer_TeamSO", "Self").is_err(), "a slot's name");
        assert!(target("Team(Client)", "GameConnection", "Client").is_err(), "a native class");
        assert!(target("Team(Client)", "Slayer_TeamSO", "Driver").is_err(), "not a base");
        assert!(target("Team Client", "Slayer_TeamSO", "Client").is_err());
        let output = BrickOutputDef {
            name: "IncScore".into(),
            class: "Slayer_TeamSO".into(),
            params: vec![],
        };
        assert!(output.validate(&["Slayer_TeamSO"]).is_ok());
        assert!(output.validate(&[]).is_err(), "only a class of its own targets");
    }

    #[test]
    fn a_brick_input_follows_an_input_by_name_and_may_aim_at_the_killer() {
        let input = |follows: Option<&str>, targets: &[&str]| {
            BrickInputDef {
                name: "onPlayerTouch(Team1)".into(),
                targets: targets.iter().map(|t| t.to_string()).collect(),
                follows: follows.map(Into::into),
            }
            .validate()
        };
        assert!(input(Some("onPlayerTouch"), &["Player", "Client"]).is_ok());
        assert!(input(None, &["Client", "Player(Killer)", "Client(Killer)"]).is_ok());
        assert!(input(Some("PlayerTouch"), &[]).is_err(), "not an input name");
        assert!(input(Some("onTouch(Team1)"), &[]).is_err(), "one of the engine's, plain");
    }
}
