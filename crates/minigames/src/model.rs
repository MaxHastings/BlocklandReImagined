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
}
impl Catalog {
    pub fn minimal_vanilla() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
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
        if self.title.trim().is_empty()
            || self.title.chars().count() > 35
            || self.title.chars().any(char::is_control)
            || !(1000..=30000).contains(&self.respawn_ms)
            || self.vehicle_respawn_ms > 300000
            || !(2000..=300000).contains(&self.brick_respawn_ms)
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
}
impl MiniGame {
    /// A game mode's mini-game, owned by the server ([`SERVER`]).
    pub fn is_server(&self) -> bool {
        self.owner == SERVER
    }
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
    /// The server runs a game mode's mini-game: players stay in it and
    /// cannot start, join or leave another.
    ServerGame,
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
