pub use bri_package::setting::SettingValue;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: u32 = 1;
pub const TICKS_PER_SECOND: u64 = 120;
pub const MAX_PLAYERS: usize = 1024;
pub const MAX_GAMES: usize = 10;
pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
pub const STANDARD_PLAYER: &str = "v20.player.playerstandardarmor";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AccountId(pub u64);
/// A connection generation, allocated by the authoritative world; never a BL_ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId {
    pub account: AccountId,
    pub session: u64,
}
/// The server itself as a mini-game owner: a game mode's mini-game, which
/// every player is in and nobody owns. Its account is the world's (brick
/// owner 0), so the world's own bricks, such as a generated map, are the
/// game's bricks.
pub const SERVER: PlayerId = PlayerId {
    account: AccountId(0),
    session: 0,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GameId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LifeId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lives {
    Unlimited,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyMode {
    Internet,
    LegacyLan,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema_version: u32,
    pub player_types: BTreeSet<String>,
    /// Item IDs are shared content IDs. A sports item grants an image, not a tool.
    pub items: BTreeMap<String, Option<String>>,
    /// What a game's own settings may be and start as: v20's, or what the
    /// host's Add-On rules ask for (Slayer's Title of 50 characters and
    /// respawn times up to 999 seconds).
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub defaults: Settings,
}
/// A brick respawn time that never comes: a knocked-out brick stays out
/// until the game resets (Slayer's Respawn Time: Brick of -1).
pub const NEVER: u32 = u32::MAX;
/// Bounds of a game's own settings ([`Catalog::limits`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Limits {
    /// Longest title, in characters.
    pub title: u32,
    /// Each respawn time's least and most, in milliseconds.
    pub respawn_ms: [u32; 2],
    pub vehicle_respawn_ms: [u32; 2],
    pub brick_respawn_ms: [u32; 2],
    /// Whether a brick respawn time may be [`NEVER`].
    pub brick_never: bool,
}
impl Default for Limits {
    /// v20's Mini-Game window.
    fn default() -> Self {
        Self {
            title: 35,
            respawn_ms: [1000, 30000],
            vehicle_respawn_ms: [0, 300000],
            brick_respawn_ms: [2000, 300000],
            brick_never: false,
        }
    }
}
impl Limits {
    /// The widest of two sets of bounds: every running Add-On's together.
    pub fn widen(self, other: Self) -> Self {
        let span = |a: [u32; 2], b: [u32; 2]| [a[0].min(b[0]), a[1].max(b[1])];
        Self {
            title: self.title.max(other.title),
            respawn_ms: span(self.respawn_ms, other.respawn_ms),
            vehicle_respawn_ms: span(self.vehicle_respawn_ms, other.vehicle_respawn_ms),
            brick_respawn_ms: span(self.brick_respawn_ms, other.brick_respawn_ms),
            brick_never: self.brick_never || other.brick_never,
        }
    }
    pub(crate) fn valid(&self) -> bool {
        let ok = |r: [u32; 2], floor: u32| r[0] >= floor && r[0] <= r[1] && r[1] <= 999_000;
        (1..=256).contains(&self.title)
            && ok(self.respawn_ms, 1000)
            && ok(self.vehicle_respawn_ms, 0)
            && ok(self.brick_respawn_ms, 0)
    }
}
impl Catalog {
    pub fn minimal_vanilla() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            limits: Limits::default(),
            defaults: Settings::default(),
            player_types: [STANDARD_PLAYER.into()].into(),
            items: [
                "hammeritem",
                "wrenchitem",
                "printgun",
                "gunitem",
                "rocketlauncheritem",
            ]
            .into_iter()
            .map(|n| (format!("v20.weapon.{n}"), None))
            .collect(),
        }
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.schema_version != SCHEMA_VERSION
            || !self.player_types.contains(STANDARD_PLAYER)
            || self.player_types.len() > 1024
            || self.items.len() > 4096
            || self
                .player_types
                .iter()
                .chain(self.items.keys())
                .any(|s| !valid_content_id(s))
            || self.items.values().flatten().any(|s| !valid_content_id(s))
            || !self.limits.valid()
        {
            return Err(Error::InvalidCatalog);
        }
        Ok(())
    }
}
/// v20's `v20.<kind>.<name>` ids, or a package's `namespace:kind/name`.
fn valid_content_id(s: &str) -> bool {
    bri_package::id::is_content_ref(s, None)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub title: String,
    pub invite_only: bool,
    pub use_all_players_bricks: bool,
    pub players_use_own_bricks: bool,
    pub use_spawn_bricks: bool,
    pub points_break_brick: i32,
    pub points_plant_brick: i32,
    pub points_kill_player: i32,
    pub points_kill_self: i32,
    pub points_die: i32,
    pub respawn_ms: u32,
    pub vehicle_respawn_ms: u32,
    pub brick_respawn_ms: u32,
    pub falling_damage: bool,
    pub weapon_damage: bool,
    pub self_damage: bool,
    pub vehicle_damage: bool,
    pub brick_damage: bool,
    pub enable_wand: bool,
    pub enable_building: bool,
    pub enable_painting: bool,
    pub player_type: String,
    pub loadout: [Option<String>; 5],
    pub lives: Lives,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            title: "Default Mini-Game".into(),
            invite_only: false,
            use_all_players_bricks: false,
            players_use_own_bricks: false,
            use_spawn_bricks: true,
            points_break_brick: 0,
            points_plant_brick: 0,
            points_kill_player: 1,
            points_kill_self: -1,
            points_die: 0,
            respawn_ms: 1000,
            vehicle_respawn_ms: 5000,
            brick_respawn_ms: 30000,
            falling_damage: true,
            weapon_damage: true,
            self_damage: true,
            vehicle_damage: true,
            brick_damage: true,
            enable_wand: false,
            enable_building: true,
            enable_painting: true,
            player_type: STANDARD_PLAYER.into(),
            lives: Lives::Unlimited,
            loadout: [
                "hammeritem",
                "wrenchitem",
                "printgun",
                "gunitem",
                "rocketlauncheritem",
            ]
            .map(|s| Some(format!("v20.weapon.{s}"))),
        }
    }
}
impl Settings {
    /// UI adapters may clamp legacy seconds before constructing this typed form.
    /// Invalid content never silently falls back to a different item/player type.
    pub fn validate(&self, catalog: &Catalog) -> Result<(), Error> {
        let l = &catalog.limits;
        let within = |ms: u32, r: [u32; 2]| (r[0]..=r[1]).contains(&ms);
        if self.title.trim().is_empty()
            || self.title.chars().count() > l.title as usize
            || self.title.chars().any(char::is_control)
            || !within(self.respawn_ms, l.respawn_ms)
            || !within(self.vehicle_respawn_ms, l.vehicle_respawn_ms)
            || !(within(self.brick_respawn_ms, l.brick_respawn_ms)
                || (l.brick_never && self.brick_respawn_ms == NEVER))
        {
            return Err(Error::InvalidSettings);
        }
        if !catalog.player_types.contains(&self.player_type)
            || self
                .loadout
                .iter()
                .flatten()
                .any(|s| !catalog.items.contains_key(s))
        {
            return Err(Error::UnknownContent);
        }
        Ok(())
    }
    pub fn equipment(&self, catalog: &Catalog) -> Equipment {
        let tools = self
            .loadout
            .clone()
            .map(|item| item.filter(|id| catalog.items.get(id).is_some_and(Option::is_none)));
        let start_ball = self.loadout[0]
            .as_ref()
            .and_then(|id| catalog.items.get(id))
            .cloned()
            .flatten();
        Equipment {
            player_type: self.player_type.clone(),
            tools,
            start_ball,
            building: self.enable_building,
            painting: self.enable_painting,
            wand: self.enable_wand,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Equipment {
    pub player_type: String,
    pub tools: [Option<String>; 5],
    pub start_ball: Option<String>,
    pub building: bool,
    pub painting: bool,
    pub wand: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifeState {
    Alive { life: LifeId },
    Dead { life: LifeId, ready_at: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: PlayerId,
    pub name: String,
    pub admin: bool,
    pub ready: bool,
    pub game: Option<GameId>,
    pub score: i64,
    pub life: LifeState,
    pub invite: Option<GameId>,
    pub ignored_owners: BTreeSet<AccountId>,
    pub last_join: Option<u64>,
    /// The team of their mini-game they play for, if it has teams.
    #[serde(default)]
    pub team: Option<TeamId>,
    /// Clicking does not respawn them ([`MinigamesWorld::hold_respawn`]).
    #[serde(default)]
    pub respawn_held: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGame {
    pub id: GameId,
    pub owner: PlayerId,
    pub color: u8,
    pub settings: Settings,
    pub members: BTreeSet<PlayerId>,
    pub round: u64,
    pub last_reset: Option<u64>,
    pub ball_update_at: Option<u64>,
    /// The game's teams, which an Add-On sets up (v20's minigames had none;
    /// Slayer added them). Empty: every member plays for themself.
    #[serde(default)]
    pub teams: Teams,
    /// Add-On settings changed from their defaults, by `namespace:key`
    /// (see [`bri_package::setting`]). The host checks each against its
    /// definition; a setting not here has its default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub addon_settings: BTreeMap<String, SettingValue>,
    /// A rule ended this round (Slayer's `endRound`): play waits for the
    /// next reset, which clears it.
    #[serde(default)]
    pub round_over: bool,
    /// Scores carry over a reset (Slayer's Clear Scores on Reset off).
    #[serde(default)]
    pub keep_scores: bool,
    /// What leaving clears for a member: the host's Add-On rules may keep
    /// it (Slayer's `removeMember`).
    #[serde(default)]
    pub cleanup: CleanupRules,
    /// A paint palette colour the host's rules gave the game in place of
    /// its v20 colour (Slayer's Color, any of 64).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint_color: Option<u8>,
    /// Bricks outside this box are not the game's: nobody in it uses or
    /// damages them (Slayer's Region Boundary bricks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    /// While it uses every player's bricks, it also claims the bricks of
    /// builders in no mini-game (Slayer's `getMinigameFromObject`).
    #[serde(default)]
    pub claims_bricks: bool,
    /// A server-owned game that does not take everyone (a host's game made
    /// at server start, Slayer's Auto Start With Server): players come and
    /// go as in a player's game.
    #[serde(default)]
    pub shared: bool,
    /// How far away its members' names show over their heads
    /// (`setShapeNameDistance`; Slayer's Name Distance), or v20's own.
    #[serde(default)]
    pub name_distance: Option<u32>,
}
/// Farthest a name may show (v20's default shape name distance).
pub const MAX_NAME_DISTANCE: u32 = 8192;
/// A box in world units, lowest corner first.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub min: [f32; 3],
    pub max: [f32; 3],
}
impl Eq for Region {}
impl Region {
    pub fn contains(&self, p: [f32; 3]) -> bool {
        (0..3).all(|i| self.min[i] <= p[i] && p[i] <= self.max[i])
    }
    pub fn valid(&self) -> bool {
        (0..3).all(|i| {
            self.min[i].is_finite() && self.max[i].is_finite() && self.min[i] <= self.max[i]
        })
    }
}
/// See [`MiniGame::cleanup`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CleanupRules {
    /// Leaving the game (or its end) clears the member's event schedules
    /// and objects and respawns their vehicles, as v20's `removeMember`.
    pub leave: bool,
}
impl Default for CleanupRules {
    fn default() -> Self {
        Self { leave: true }
    }
}
impl MiniGame {
    /// A game mode's mini-game, owned by the server ([`SERVER`]).
    pub fn is_server(&self) -> bool {
        self.owner == SERVER
    }
    /// The server's only mini-game, which every player is in (a game
    /// mode's), as opposed to a [`MiniGame::shared`] one.
    pub fn is_exclusive(&self) -> bool {
        self.is_server() && !self.shared
    }
    pub(crate) fn new(id: GameId, owner: PlayerId, color: u8, settings: Settings) -> Self {
        Self {
            id,
            owner,
            color,
            settings,
            members: BTreeSet::new(),
            round: 1,
            last_reset: None,
            ball_update_at: None,
            teams: Teams::default(),
            addon_settings: BTreeMap::new(),
            round_over: false,
            keep_scores: false,
            cleanup: CleanupRules::default(),
            paint_color: None,
            region: None,
            claims_bricks: false,
            shared: false,
            name_distance: None,
        }
    }
}
/// Most teams one mini-game has (Slayer's team list has no fixed cap; its
/// GUI offers colours from the 64-colour palette).
pub const MAX_TEAMS: usize = 64;
/// Longest team name, in characters.
pub const MAX_TEAM_NAME: usize = 50;
/// A team of one mini-game. Ids are kept while the team exists, so a
/// renamed or recoloured team keeps its members.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TeamId(pub u32);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Team {
    pub id: TeamId,
    pub name: String,
    /// Index into the server's paint palette: the team's colour, which also
    /// claims the bricks painted it (team spawns, flags).
    pub color: u8,
    /// Add-On team settings changed from their defaults, by
    /// `namespace:key`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub addon_settings: BTreeMap<String, SettingValue>,
}
/// Most Add-On settings one game (or one team) holds apart from defaults.
pub const MAX_ADDON_SETTINGS: usize = 512;
/// Longest `namespace:key`.
pub const MAX_SETTING_KEY: usize = 96;
/// Longest text setting, in bytes.
pub const MAX_SETTING_TEXT: usize = 1024;
/// One Add-On setting to change: the game's own (`team` None) or a team's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingChange {
    pub team: Option<TeamId>,
    pub key: String,
    /// `None` puts it back to its default.
    pub value: Option<SettingValue>,
}
/// One team as an Add-On asks for it: an existing `id` keeps that team and
/// its members, `None` makes a new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamSpec {
    pub id: Option<TeamId>,
    pub name: String,
    pub color: u8,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Teams {
    pub list: Vec<Team>,
    /// Whether teammates (and allies) may hurt each other.
    pub friendly_fire: bool,
    /// Teams of the same colour are allies (Slayer's `allySameColors`).
    pub ally_same_color: bool,
    pub(crate) next: u32,
}
impl Teams {
    pub fn get(&self, id: TeamId) -> Option<&Team> {
        self.list.iter().find(|t| t.id == id)
    }
    /// Same team, or allied by colour.
    pub fn allied(&self, a: TeamId, b: TeamId) -> bool {
        a == b
            || (self.ally_same_color
                && matches!((self.get(a), self.get(b)), (Some(x), Some(y)) if x.color == y.color))
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        let mut ids = BTreeSet::new();
        if self.list.len() > MAX_TEAMS {
            return Err(Error::Capacity);
        }
        for t in &self.list {
            if !ids.insert(t.id)
                || t.id.0 >= self.next
                || !valid_team_name(&t.name)
                || !valid_addon_settings(&t.addon_settings)
            {
                return Err(Error::InvalidSettings);
            }
        }
        Ok(())
    }
}
pub(crate) fn valid_addon_settings(map: &BTreeMap<String, SettingValue>) -> bool {
    map.len() <= MAX_ADDON_SETTINGS
        && map.iter().all(|(k, v)| {
            !k.is_empty()
                && k.len() <= MAX_SETTING_KEY
                && !matches!(v, SettingValue::Text(t) if t.len() > MAX_SETTING_TEXT)
        })
}
pub(crate) fn valid_team_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= MAX_TEAM_NAME
        && !name.chars().any(char::is_control)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    StalePlayer,
    StaleGame,
    StaleLife,
    NotReady,
    NotOwner,
    NotMember,
    AlreadyMember,
    AlreadyOwner,
    InviteOnly,
    NoInvitation,
    Ignored,
    AlreadyInvited,
    Cooldown,
    ColorUnavailable,
    InvalidSettings,
    UnknownContent,
    InvalidCatalog,
    Capacity,
    InvalidEvent,
    InvalidSnapshot,
    InvalidClock,
    RespawnNotReady,
    /// The player's game holds their respawn (out of lives, round over).
    RespawnHeld,
    /// The round already ended; it waits for a reset.
    RoundOver,
    /// The server runs a game mode's mini-game: players stay in it and
    /// cannot start, join or leave another.
    ServerGame,
    /// No such team in that mini-game.
    StaleTeam,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnReason {
    Join,
    Leave,
    Reset,
    RespawnAll,
    Respawn,
    SpawnSettingChanged,
    End,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Chat,
    Center { seconds: u8 },
    Bottom { seconds: u8 },
}
/// All side effects are returned to the host, which applies them in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Created {
        game: GameId,
    },
    Ended {
        game: GameId,
    },
    Configured {
        game: GameId,
    },
    Membership {
        player: PlayerId,
        game: Option<GameId>,
        color: Option<u8>,
    },
    Invitation {
        player: PlayerId,
        game: Option<GameId>,
    },
    Score {
        player: PlayerId,
        value: i64,
    },
    /// A member's team changed (`None`: no team, as on leaving the game).
    TeamChanged {
        player: PlayerId,
        game: GameId,
        team: Option<TeamId>,
    },
    /// The game's team list or team rules changed.
    TeamsConfigured {
        game: GameId,
    },
    /// Add-On settings of the game or its teams changed: their
    /// `namespace:key`s.
    AddOnSettings {
        game: GameId,
        keys: Vec<String>,
    },
    /// A rule ended the round, won by these teams and players (none: a
    /// round nobody won).
    RoundEnded {
        game: GameId,
        teams: Vec<TeamId>,
        players: Vec<PlayerId>,
    },
    Cleanup {
        player: PlayerId,
        clear_event_schedules: bool,
        reset_owned_vehicles: bool,
        clear_spawned_objects: bool,
    },
    EjectVehicles {
        brick_owner: AccountId,
    },
    Spawn {
        player: PlayerId,
        life: LifeId,
        reason: SpawnReason,
        equipment: Option<Equipment>,
    },
    /// Living owner remains in place when ending; host heals and restores outside gear.
    RestoreOwner {
        player: PlayerId,
        life: LifeId,
    },
    ApplyEquipment {
        player: PlayerId,
        equipment: Equipment,
        changed_slots: [bool; 5],
        change_player_type: bool,
        cancel_building: bool,
        unmount_paint: bool,
    },
    StartBall {
        player: PlayerId,
        image: String,
        only_if_hands_empty: bool,
    },
    Death {
        player: PlayerId,
        life: LifeId,
        ready_at: u64,
    },
    RespawnDeadline {
        player: PlayerId,
        life: LifeId,
        ready_at: u64,
    },
    ResetBricks {
        owners: Vec<AccountId>,
        respawn_vehicles: bool,
        reveal_items: bool,
    },
    Reset {
        game: GameId,
        round: u64,
    },
    Message {
        recipients: Vec<PlayerId>,
        kind: MessageKind,
        text: String,
    },
}
/// Caller context must come from the host's validated event target, never packet fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventAuthority {
    Owner(PlayerId),
    OwnerBrick {
        instigator: PlayerId,
        brick_owner: AccountId,
    },
    System,
}
#[derive(Clone, Debug)]
pub enum Command {
    Create {
        actor: PlayerId,
        color: u8,
        settings: Settings,
    },
    Configure {
        actor: PlayerId,
        settings: Settings,
    },
    Join {
        actor: PlayerId,
        game: GameId,
    },
    Leave {
        actor: PlayerId,
    },
    Invite {
        actor: PlayerId,
        target: PlayerId,
    },
    Accept {
        actor: PlayerId,
        game: GameId,
    },
    Reject {
        actor: PlayerId,
        game: GameId,
        ignore_owner: bool,
    },
    Kick {
        actor: PlayerId,
        target: PlayerId,
    },
    Reset {
        game: GameId,
        authority: EventAuthority,
    },
    RespawnAll {
        game: GameId,
        authority: EventAuthority,
    },
    End {
        actor: PlayerId,
    },
    Respawn {
        actor: PlayerId,
    },
    /// The host gives `target` a new life now, alive or dead: a package's
    /// round reset, or `serverCmdDropPlayerAtCamera` on a dead administrator
    /// (`spawnPlayer` without the respawn wait). Only the host constructs it.
    ForceRespawn {
        target: PlayerId,
    },
    Message {
        game: GameId,
        authority: EventAuthority,
        kind: MessageKind,
        text: String,
    },
    /// `actor` runs `game` for its owner: the host's Add-On rules let them
    /// (Slayer's Edit and Reset Rights). The host checked that; the owner's
    /// own commands are [`Command::Configure`] and the rest.
    Manage {
        actor: PlayerId,
        game: GameId,
        action: Manage,
    },
}
/// What [`Command::Manage`] does to a game.
#[derive(Clone, Debug)]
pub enum Manage {
    Configure(Settings),
    Invite(PlayerId),
    Kick(PlayerId),
    /// A new round at once, without the five seconds the owner waits
    /// between resets.
    Reset,
    RespawnAll,
    End,
}
pub const COLORS: [[u8; 3]; 10] = [
    [255, 0, 0],
    [255, 128, 0],
    [255, 255, 0],
    [0, 255, 0],
    [0, 128, 0],
    [0, 255, 255],
    [0, 128, 128],
    [0, 128, 255],
    [255, 128, 255],
    [0, 0, 0],
];

/// Inclusive scheduled timers round up. Manual death respawn uses strict > below.
pub const fn ticks_for_ms(ms: u32) -> u64 {
    (ms as u64 * TICKS_PER_SECOND).div_ceil(1000)
}
pub const fn manual_respawn_ticks(ms: u32) -> u64 {
    ms as u64 * TICKS_PER_SECOND / 1000 + 1
}
