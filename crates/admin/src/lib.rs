//! Native administration foundation. The host supplies authenticated connections;
//! request bytes never select the acting connection, role, or host authority.
use bri_console::Clamp;
use bri_package::setting::{self, SettingValue};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{Read, Write},
};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_CONNECTIONS: usize = 1024;
pub const MAX_BANS: usize = 4096;
pub const MAX_AUTO_ROLES: usize = 4096;
pub const MAX_SAVE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_REQUEST_BYTES: usize = 4096;
/// Identities whose failed password guesses are remembered across reconnects.
pub const MAX_LOGIN_STRIKES: usize = 4096;
/// How long an identity's failed guesses count against it after the last one.
pub const LOGIN_STRIKE_SECONDS: u64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConnectionId(pub u64);
/// Host-verified native identity, never a legacy BL_ID or an unverified client claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Principal(pub [u8; 32]);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BanId(pub u64);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    #[default]
    Player,
    Admin,
    SuperAdmin,
}
impl Role {
    pub fn is_admin(self) -> bool {
        self != Self::Player
    }
}

/// Not serializable: created only by the transport/local-host adapter.
#[derive(Clone, Copy, Debug)]
pub enum Origin {
    Connection(ConnectionId),
    HostConsole,
}
/// Registration is a trusted host API, never a network message.
#[derive(Clone, Debug)]
pub struct TrustedConnection {
    pub id: ConnectionId,
    pub display_name: String,
    pub principal: Option<Principal>,
    pub is_owner: bool,
    pub is_local: bool,
    pub is_bot: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct PlayerRow {
    pub connection: ConnectionId,
    pub display_name: String,
    pub role: Role,
    pub is_owner: bool,
    pub is_local: bool,
    pub is_bot: bool,
    pub durable_identity_available: bool,
}
#[derive(Clone, Debug)]
struct Session {
    trusted: TrustedConnection,
    role: Role,
    failed_logins: u8,
    locked: bool,
}

/// Secret request values deliberately redact Debug and are never part of snapshots.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);
impl Secret {
    pub fn new(value: String) -> Result<Self, Error> {
        validate_text(&value, 256)?;
        Ok(Self(value))
    }
    pub fn validate(&self) -> Result<(), Error> {
        validate_text(&self.0, 256)
    }
    /// Only the host credential verifier or password storage adapter should call this.
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BanDuration {
    Minutes(u32),
    Forever,
}
impl BanDuration {
    fn deadline(self, now: u64) -> Result<Option<u64>, Error> {
        match self {
            Self::Forever => Ok(None),
            Self::Minutes(n) if (1..=525_600_000).contains(&n) => now
                .checked_add(u64::from(n) * 60)
                .map(Some)
                .ok_or(Error::InvalidTime),
            _ => Err(Error::InvalidTime),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BanRecord {
    pub id: BanId,
    pub principal: Principal,
    pub victim_name: String,
    pub issued_by: String,
    pub reason: String,
    pub created_unix_seconds: u64,
    pub expires_unix_seconds: Option<u64>,
}
impl BanRecord {
    pub fn active(&self, now: u64) -> bool {
        self.expires_unix_seconds.is_none_or(|end| now < end)
    }
}

/// Native resource IDs resolve through host catalogs; never execute source paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
pub enum Action {
    Login {
        password: Secret,
    },
    Kick {
        target: ConnectionId,
    },
    Ban {
        target: ConnectionId,
        duration: BanDuration,
        reason: String,
    },
    Unban {
        ban: BanId,
    },
    RequestBanList,
    /// The saved ranks (v20's auto-admin lists), for the host and Super Admins.
    RequestAutoRoles,
    RequestBrickGroups,
    RequestMaps,
    Spy {
        target: ConnectionId,
    },
    Fetch {
        target: ConnectionId,
    },
    Find {
        target: ConnectionId,
    },
    ReturnToPreviousPosition,
    DropPlayerAtCamera,
    DropCameraAtPlayer,
    RealBrickCount,
    ResetVehicles,
    CancelAllEvents,
    DestructoWand,
    ChangeMap {
        map: String,
    },
    ClearAllBricks,
    ClearBrickGroup {
        group: u64,
    },
    HighlightBrickGroup {
        group: u64,
    },
    ClearVehicles,
    ClearBots,
    Warp,
    TimeScale {
        scale: f32,
    },
    /// The Environment window's Apply: the whole live environment, each
    /// setting unset to keep the map's own.
    SetEnvironment {
        settings: bri_content::atmosphere::Settings,
    },
    SetAdminPassword {
        password: Secret,
    },
    /// Make a connected player Admin or Super Admin, or take the rank away.
    /// The host and Super Admins may; the rank is also saved in the host's
    /// auto-admin list under the player's verified key (v20's
    /// `$Pref::Server::AutoAdminList`), so it returns when they rejoin.
    HostSetRole {
        target: ConnectionId,
        role: Role,
    },
    HostSetAutoRole {
        principal: Principal,
        role: Role,
    },
    HostSetPassword {
        slot: PasswordSlot,
        password: Secret,
    },
    HostConfigure {
        settings: ServerSettings,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PasswordSlot {
    Join,
    Admin,
    SuperAdmin,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub action: Action,
}
impl Request {
    pub fn new(action: Action) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            action,
        }
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(Error::Budget);
        }
        let value: Self = serde_json::from_slice(bytes)?;
        if value.schema_version != SCHEMA_VERSION {
            return Err(Error::Schema);
        }
        value.action.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::Schema);
        }
        self.action.validate()
    }
}
impl Action {
    fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Login { password }
            | Self::SetAdminPassword { password }
            | Self::HostSetPassword { password, .. } => validate_text(password.expose(), 256),
            Self::Ban {
                reason, duration, ..
            } => {
                validate_text(reason, 512)?;
                duration.deadline(0)?;
                Ok(())
            }
            Self::ChangeMap { map } => {
                validate_text(map, 128)?;
                if map.is_empty()
                    || !map
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
                    || map.split('/').any(|part| matches!(part, "" | "." | ".."))
                {
                    Err(Error::InvalidValue)
                } else {
                    Ok(())
                }
            }
            Self::TimeScale { scale } if !scale.is_finite() => Err(Error::InvalidValue),
            Self::SetEnvironment { settings } => {
                settings.validate().map_err(|_| Error::InvalidValue)
            }
            Self::HostSetAutoRole { principal, .. } => validate_principal(*principal),
            Self::HostConfigure { settings } => settings.validate(),
            _ => Ok(()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quotas {
    pub schedules: u32,
    pub misc: u32,
    pub projectiles: u32,
    pub items: u32,
    pub environment: u32,
    pub players: u32,
    pub vehicles: u32,
}
impl Quotas {
    fn validate(&self) -> Result<(), Error> {
        if [
            self.schedules,
            self.misc,
            self.projectiles,
            self.items,
            self.environment,
            self.players,
            self.vehicles,
        ]
        .into_iter()
        .any(|x| x > 1_000_000)
        {
            Err(Error::Budget)
        } else {
            Ok(())
        }
    }
}
/// Source defaults, with explicit native safety bounds; application/restart is host-owned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerSettings {
    pub name: String,
    pub port: u16,
    pub max_players: u16,
    pub brick_limit: u32,
    pub bricks_per_second: u32,
    pub max_chat_length: u32,
    pub physics_vehicles: u32,
    pub player_vehicles: u32,
    pub random_brick_color: bool,
    pub chat_filter: bool,
    pub falling_damage: bool,
    pub public_domain_timeout_minutes: i32,
    pub too_far_distance: f32,
    pub wrench_events_admin_only: bool,
    pub per_player: Quotas,
    pub lan: Quotas,
    /// Running Add-Ons' server-wide settings changed from their defaults,
    /// by `namespace:key` (RTB's `$Pref::Server::*` preferences). Values of
    /// Add-Ons not running now are kept for when they run again.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub addon_settings: BTreeMap<String, SettingValue>,
}
/// Most server-wide Add-On setting values the host keeps.
pub const MAX_ADDON_SETTINGS: usize = 512;
impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            name: "Blockland Server".into(),
            port: 28000,
            max_players: 8,
            brick_limit: 256000,
            bricks_per_second: 10,
            max_chat_length: 120,
            physics_vehicles: 10,
            player_vehicles: 150,
            random_brick_color: false,
            chat_filter: true,
            falling_damage: true,
            public_domain_timeout_minutes: -1,
            too_far_distance: 50.,
            wrench_events_admin_only: false,
            per_player: Quotas {
                schedules: 50,
                misc: 100,
                projectiles: 25,
                items: 25,
                environment: 100,
                players: 10,
                vehicles: 5,
            },
            lan: Quotas {
                schedules: 300,
                misc: 300,
                projectiles: 50,
                items: 50,
                environment: 500,
                players: 64,
                vehicles: 20,
            },
            addon_settings: BTreeMap::new(),
        }
    }
}
impl ServerSettings {
    pub fn validate(&self) -> Result<(), Error> {
        validate_text(&self.name, 128)?;
        if self.name.is_empty()
            || self.port < 1024
            || self.max_players == 0
            || usize::from(self.max_players) > MAX_CONNECTIONS
            || self.brick_limit > 100_000_000
            || self.bricks_per_second > 100_000
            || self.max_chat_length > 4096
            || self.physics_vehicles > 1_000_000
            || self.player_vehicles > 1_000_000
            || self.public_domain_timeout_minutes < -1
            || !self.too_far_distance.is_finite()
            || !(0.0..=1_000_000.).contains(&self.too_far_distance)
        {
            return Err(Error::InvalidValue);
        }
        // Each value's own definition is checked by the running Add-Ons.
        if self.addon_settings.len() > MAX_ADDON_SETTINGS
            || self.addon_settings.iter().any(|(key, value)| {
                !key.contains(':')
                    || !setting::is_setting_ref(key)
                    || value.as_text().is_some_and(|t| {
                        t.chars().count() > setting::MAX_TEXT || t.chars().any(char::is_control)
                    })
            })
        {
            return Err(Error::InvalidValue);
        }
        self.per_player.validate()?;
        self.lan.validate()
    }
}

/// An authorized effect still requires host execution and gameplay validation.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Disconnect {
        target: ConnectionId,
        reason: DisconnectReason,
    },
    RoleChanged {
        target: ConnectionId,
        role: Role,
    },
    LoginRejected {
        attempts: u8,
        disconnect: bool,
    },
    LoginIgnored,
    BansChanged,
    BanList(Vec<BanRecord>),
    AutoRoleList(Vec<AutoRole>),
    AutoRolesChanged,
    PasswordChange {
        slot: PasswordSlot,
        password: Secret,
    },
    Configure(ServerSettings),
    Gameplay {
        actor: Option<ConnectionId>,
        command: GameplayCommand,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum GameplayCommand {
    Spy(ConnectionId),
    Fetch(ConnectionId),
    Find(ConnectionId),
    ReturnToPreviousPosition,
    DropPlayerAtCamera,
    DropCameraAtPlayer,
    RealBrickCount,
    ResetVehicles,
    CancelAllEvents,
    DestructoWand,
    ChangeMap(String),
    ClearAllBricks,
    ClearBrickGroup(u64),
    HighlightBrickGroup(u64),
    ClearVehicles,
    ClearBots,
    Warp,
    TimeScale(f32),
    SetEnvironment(Box<bri_content::atmosphere::Settings>),
    RequestBrickGroups,
    RequestMaps,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisconnectReason {
    Kicked,
    Banned(BanId),
    FailedPasswords,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown or expired transport connection")]
    UnknownConnection,
    #[error("administrator permission required")]
    Denied,
    #[error("host-local authority required; no stock remote command was verified")]
    HostOnly,
    #[error("protected owner, local client or Super Admin")]
    Protected,
    #[error("host-verified persistent identity unavailable")]
    IdentityUnavailable,
    #[error("connection is banned")]
    Banned,
    #[error("unknown stable ban ID")]
    UnknownBan,
    #[error("input/resource budget exceeded")]
    Budget,
    #[error("invalid native value")]
    InvalidValue,
    #[error("invalid or overflowing time")]
    InvalidTime,
    #[error("duplicate or reused identity")]
    Duplicate,
    #[error("unsupported schema")]
    Schema,
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}
fn validate_text(value: &str, max: usize) -> Result<(), Error> {
    if value.len() > max {
        return Err(Error::Budget);
    }
    if value.chars().any(char::is_control) {
        return Err(Error::InvalidValue);
    }
    Ok(())
}
fn validate_principal(p: Principal) -> Result<(), Error> {
    if p.0 == [0; 32] {
        Err(Error::InvalidValue)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoRole {
    pub principal: Principal,
    pub role: Role,
    /// The name the player had when the rank was given, for people reading
    /// the saved list. Joining matches the key, never the name.
    #[serde(default)]
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableState {
    pub schema_version: u32,
    pub next_ban_id: u64,
    pub bans: Vec<BanRecord>,
    pub auto_roles: Vec<AutoRole>,
}
impl Default for DurableState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            next_ban_id: 1,
            bans: Vec::new(),
            auto_roles: Vec::new(),
        }
    }
}
impl DurableState {
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::Schema);
        }
        if self.bans.len() > MAX_BANS || self.auto_roles.len() > MAX_AUTO_ROLES {
            return Err(Error::Budget);
        }
        if self.next_ban_id == 0 {
            return Err(Error::InvalidValue);
        }
        let mut ids = BTreeSet::new();
        let mut principals = BTreeSet::new();
        for b in &self.bans {
            validate_principal(b.principal)?;
            if b.id.0 == 0 || b.id.0 >= self.next_ban_id {
                return Err(Error::InvalidValue);
            }
            if !ids.insert(b.id) || !principals.insert(b.principal) {
                return Err(Error::Duplicate);
            }
            validate_text(&b.victim_name, 128)?;
            validate_text(&b.issued_by, 128)?;
            validate_text(&b.reason, 512)?;
            if b.expires_unix_seconds.is_some_and(|t| {
                t <= b.created_unix_seconds || t - b.created_unix_seconds > 525_600_000 * 60
            }) {
                return Err(Error::InvalidTime);
            }
        }
        principals.clear();
        for a in &self.auto_roles {
            validate_principal(a.principal)?;
            if a.role == Role::Player {
                return Err(Error::InvalidValue);
            }
            validate_text(&a.name, 128)?;
            if !principals.insert(a.principal) {
                return Err(Error::Duplicate);
            }
        }
        Ok(())
    }
    pub fn read(reader: impl Read) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        reader
            .take(MAX_SAVE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_SAVE_BYTES {
            return Err(Error::Budget);
        }
        let state: Self = serde_json::from_slice(&bytes)?;
        state.validate()?;
        Ok(state)
    }
    /// Host writes these validated bytes to its atomic-save staging path. No paths are accepted from clients.
    pub fn write(&self, mut writer: impl Write) -> Result<(), Error> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_SAVE_BYTES {
            return Err(Error::Budget);
        }
        writer.write_all(&bytes)?;
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct Administration {
    sessions: BTreeMap<ConnectionId, Session>,
    /// Failed guesses per durable identity (count, unix seconds of the last),
    /// so reconnecting never buys fresh guesses.
    login_strikes: BTreeMap<Principal, (u8, u64)>,
    durable: DurableState,
    last_connection_id: u64,
}
impl Administration {
    pub fn durable(&self) -> &DurableState {
        &self.durable
    }
    /// Atomic in-memory restore. Does not silently change roles of already connected clients.
    pub fn restore(&mut self, reader: impl Read) -> Result<(), Error> {
        let state = DurableState::read(reader)?;
        self.durable = state;
        Ok(())
    }
    /// IDs must increase for this service lifetime; delayed packets cannot target a replacement client.
    pub fn connect(&mut self, c: TrustedConnection, now: u64) -> Result<Role, Error> {
        if c.id.0 <= self.last_connection_id {
            return Err(Error::Duplicate);
        }
        if self.sessions.len() >= MAX_CONNECTIONS {
            return Err(Error::Budget);
        }
        validate_text(&c.display_name, 128)?;
        if c.display_name.is_empty() {
            return Err(Error::InvalidValue);
        }
        if let Some(p) = c.principal {
            validate_principal(p)?;
            if !c.is_owner && !c.is_local && self.is_banned(p, now) {
                return Err(Error::Banned);
            }
        }
        let failed_logins = c
            .principal
            .and_then(|p| self.login_strikes.get(&p))
            .filter(|(_, last)| now.saturating_sub(*last) < LOGIN_STRIKE_SECONDS)
            .map_or(0, |(count, _)| *count);
        let role = if c.is_owner || c.is_local {
            Role::SuperAdmin
        } else {
            c.principal
                .and_then(|p| {
                    self.durable
                        .auto_roles
                        .iter()
                        .find(|a| a.principal == p)
                        .map(|a| a.role)
                })
                .unwrap_or_default()
        };
        self.last_connection_id = c.id.0;
        self.sessions.insert(
            c.id,
            Session {
                trusted: c,
                role,
                failed_logins,
                locked: false,
            },
        );
        Ok(role)
    }
    pub fn disconnect(&mut self, id: ConnectionId) {
        self.sessions.remove(&id);
    }
    /// A connected player chose a new display name.
    pub fn rename(&mut self, id: ConnectionId, display_name: String) -> Result<(), Error> {
        validate_text(&display_name, 128)?;
        if display_name.is_empty() {
            return Err(Error::InvalidValue);
        }
        let session = self.sessions.get_mut(&id).ok_or(Error::UnknownConnection)?;
        session.trusted.display_name = display_name;
        Ok(())
    }
    pub fn is_banned(&self, p: Principal, now: u64) -> bool {
        self.durable
            .bans
            .iter()
            .any(|b| b.principal == p && b.active(now))
    }
    pub fn rows(&self) -> Vec<PlayerRow> {
        self.sessions
            .values()
            .map(|s| PlayerRow {
                connection: s.trusted.id,
                display_name: s.trusted.display_name.clone(),
                role: s.role,
                is_owner: s.trusted.is_owner,
                is_local: s.trusted.is_local,
                is_bot: s.trusted.is_bot,
                durable_identity_available: s.trusted.principal.is_some(),
            })
            .collect()
    }
    fn actor(&self, origin: Origin) -> Result<Option<&Session>, Error> {
        match origin {
            Origin::HostConsole => Ok(None),
            Origin::Connection(id) => self
                .sessions
                .get(&id)
                .filter(|s| !s.locked)
                .map(Some)
                .ok_or(Error::UnknownConnection),
        }
    }
    pub fn role(&self, id: ConnectionId) -> Option<Role> {
        self.sessions.get(&id).map(|s| s.role)
    }
    pub fn host_authority(&self, origin: Origin) -> Result<bool, Error> {
        Ok(self
            .actor(origin)?
            .is_none_or(|s| s.trusted.is_owner || s.trusted.is_local))
    }
    pub fn permission(&self, origin: Origin, action: &Action) -> Result<(), Error> {
        let actor = self.actor(origin)?;
        match action {
            Action::Login { .. } => {
                if actor.is_some() {
                    Ok(())
                } else {
                    Err(Error::Denied)
                }
            }
            Action::HostSetRole { .. }
            | Action::HostSetAutoRole { .. }
            | Action::RequestAutoRoles => {
                if self.host_authority(origin)? || actor.is_some_and(|s| s.role == Role::SuperAdmin)
                {
                    Ok(())
                } else {
                    Err(Error::Denied)
                }
            }
            Action::HostSetPassword { .. } | Action::HostConfigure { .. } => {
                if self.host_authority(origin)? {
                    Ok(())
                } else {
                    Err(Error::HostOnly)
                }
            }
            Action::SetAdminPassword { .. } => {
                if actor.is_none_or(|s| {
                    s.role == Role::SuperAdmin || s.trusted.is_owner || s.trusted.is_local
                }) {
                    Ok(())
                } else {
                    Err(Error::Denied)
                }
            }
            _ => {
                if actor
                    .is_none_or(|s| s.role.is_admin() || s.trusted.is_owner || s.trusted.is_local)
                {
                    Ok(())
                } else {
                    Err(Error::Denied)
                }
            }
        }
    }
    fn target(&self, id: ConnectionId) -> Result<&Session, Error> {
        self.sessions.get(&id).ok_or(Error::UnknownConnection)
    }
    fn protect_target(&self, id: ConnectionId, ban: bool) -> Result<(), Error> {
        let t = self.target(id)?;
        if (ban || !t.trusted.is_bot)
            && (t.trusted.is_owner || t.trusted.is_local || t.role == Role::SuperAdmin)
        {
            Err(Error::Protected)
        } else {
            Ok(())
        }
    }
    /// `verify_password` is a trusted host verifier; return only Admin/SA on success.
    /// It must prefer SA if both configured passwords match. Empty passwords never call it.
    pub fn handle(
        &mut self,
        origin: Origin,
        request: Request,
        now: u64,
        verify_password: impl FnOnce(&str) -> Option<Role>,
    ) -> Result<Vec<Effect>, Error> {
        request.validate()?;
        self.permission(origin, &request.action)?;
        let actor = match origin {
            Origin::Connection(id) => Some(id),
            Origin::HostConsole => None,
        };
        let actor_name = self
            .actor(origin)?
            .map_or("CONSOLE", |s| s.trusted.display_name.as_str())
            .to_owned();
        let command = match request.action {
            Action::Login { password } => {
                let s = self
                    .sessions
                    .get_mut(&actor.ok_or(Error::Denied)?)
                    .ok_or(Error::UnknownConnection)?;
                if password.expose().is_empty() {
                    return Ok(vec![Effect::LoginIgnored]);
                }
                // Guess budgets follow the durable identity; an anonymous
                // connection could reconnect for fresh guesses forever.
                let principal = s.trusted.principal.ok_or(Error::Denied)?;
                // An identity that used up its guesses is refused without
                // checking the password until its strikes expire.
                let verified = if s.failed_logins > 3 {
                    None
                } else {
                    verify_password(password.expose())
                };
                if let Some(role @ (Role::Admin | Role::SuperAdmin)) = verified {
                    s.role = role;
                    self.login_strikes.remove(&principal);
                    return Ok(vec![Effect::RoleChanged {
                        target: s.trusted.id,
                        role,
                    }]);
                }
                s.failed_logins = s.failed_logins.saturating_add(1);
                s.locked = s.failed_logins > 3;
                if !self.login_strikes.contains_key(&principal)
                    && self.login_strikes.len() >= MAX_LOGIN_STRIKES
                    && let Some(stale) = self
                        .login_strikes
                        .iter()
                        .min_by_key(|(_, (_, last))| *last)
                        .map(|(p, _)| *p)
                {
                    self.login_strikes.remove(&stale);
                }
                self.login_strikes.insert(principal, (s.failed_logins, now));
                let mut out = vec![Effect::LoginRejected {
                    attempts: s.failed_logins,
                    disconnect: s.locked,
                }];
                if s.locked {
                    out.push(Effect::Disconnect {
                        target: s.trusted.id,
                        reason: DisconnectReason::FailedPasswords,
                    });
                }
                return Ok(out);
            }
            Action::Kick { target } => {
                self.protect_target(target, false)?;
                return Ok(vec![Effect::Disconnect {
                    target,
                    reason: DisconnectReason::Kicked,
                }]);
            }
            Action::Ban {
                target,
                duration,
                reason,
            } => {
                self.protect_target(target, true)?;
                let victim = self.target(target)?;
                let principal = victim.trusted.principal.ok_or(Error::IdentityUnavailable)?;
                // A principal can have multiple connections; protect every matching connection.
                for s in self
                    .sessions
                    .values()
                    .filter(|s| s.trusted.principal == Some(principal))
                {
                    self.protect_target(s.trusted.id, true)?;
                }
                let id = BanId(self.durable.next_ban_id);
                let next = id.0.checked_add(1).ok_or(Error::Budget)?;
                let record = BanRecord {
                    id,
                    principal,
                    victim_name: victim.trusted.display_name.clone(),
                    issued_by: actor_name,
                    reason,
                    created_unix_seconds: now,
                    expires_unix_seconds: duration.deadline(now)?,
                };
                let retained = self
                    .durable
                    .bans
                    .iter()
                    .filter(|b| b.active(now) && b.principal != principal)
                    .count();
                if retained >= MAX_BANS {
                    return Err(Error::Budget);
                }
                self.durable
                    .bans
                    .retain(|b| b.active(now) && b.principal != principal);
                self.durable.bans.push(record);
                self.durable.next_ban_id = next;
                let mut out = vec![Effect::BansChanged];
                out.extend(
                    self.sessions
                        .values()
                        .filter(|s| s.trusted.principal == Some(principal))
                        .map(|s| Effect::Disconnect {
                            target: s.trusted.id,
                            reason: DisconnectReason::Banned(id),
                        }),
                );
                return Ok(out);
            }
            Action::Unban { ban } => {
                let i = self
                    .durable
                    .bans
                    .iter()
                    .position(|b| b.id == ban)
                    .ok_or(Error::UnknownBan)?;
                self.durable.bans.remove(i);
                return Ok(vec![Effect::BansChanged]);
            }
            Action::RequestBanList => {
                return Ok(vec![Effect::BanList(
                    self.durable
                        .bans
                        .iter()
                        .filter(|b| b.active(now))
                        .cloned()
                        .collect(),
                )]);
            }
            Action::HostSetRole { target, role } => {
                let s = self
                    .sessions
                    .get_mut(&target)
                    .ok_or(Error::UnknownConnection)?;
                if s.trusted.is_owner || s.trusted.is_local || s.trusted.is_bot {
                    return Err(Error::Protected);
                }
                let saved = s
                    .trusted
                    .principal
                    .map(|p| (p, s.trusted.display_name.clone()));
                let mut out = vec![Effect::RoleChanged { target, role }];
                // Without a verified key the rank lasts for this visit only.
                if let Some((principal, name)) = saved {
                    self.set_auto_role(principal, role, name)?;
                    out.push(Effect::AutoRolesChanged);
                }
                if let Some(s) = self.sessions.get_mut(&target) {
                    s.role = role;
                }
                return Ok(out);
            }
            Action::RequestAutoRoles => {
                return Ok(vec![Effect::AutoRoleList(self.durable.auto_roles.clone())]);
            }
            Action::HostSetAutoRole { principal, role } => {
                let name = self
                    .durable
                    .auto_roles
                    .iter()
                    .find(|a| a.principal == principal)
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                self.set_auto_role(principal, role, name)?;
                return Ok(vec![Effect::AutoRolesChanged]);
            }
            Action::SetAdminPassword { password } => {
                return Ok(vec![Effect::PasswordChange {
                    slot: PasswordSlot::Admin,
                    password,
                }]);
            }
            Action::HostSetPassword { slot, password } => {
                return Ok(vec![Effect::PasswordChange { slot, password }]);
            }
            Action::HostConfigure { settings } => return Ok(vec![Effect::Configure(settings)]),
            Action::Spy { target } => {
                self.target(target)?;
                GameplayCommand::Spy(target)
            }
            Action::Fetch { target } => {
                self.target(target)?;
                GameplayCommand::Fetch(target)
            }
            Action::Find { target } => {
                self.target(target)?;
                GameplayCommand::Find(target)
            }
            Action::ReturnToPreviousPosition => GameplayCommand::ReturnToPreviousPosition,
            Action::DropPlayerAtCamera => GameplayCommand::DropPlayerAtCamera,
            Action::DropCameraAtPlayer => GameplayCommand::DropCameraAtPlayer,
            Action::RealBrickCount => GameplayCommand::RealBrickCount,
            Action::ResetVehicles => GameplayCommand::ResetVehicles,
            Action::CancelAllEvents => GameplayCommand::CancelAllEvents,
            Action::DestructoWand => GameplayCommand::DestructoWand,
            Action::ChangeMap { map } => GameplayCommand::ChangeMap(map),
            Action::ClearAllBricks => GameplayCommand::ClearAllBricks,
            Action::ClearBrickGroup { group } => GameplayCommand::ClearBrickGroup(group),
            Action::HighlightBrickGroup { group } => GameplayCommand::HighlightBrickGroup(group),
            Action::ClearVehicles => GameplayCommand::ClearVehicles,
            Action::ClearBots => GameplayCommand::ClearBots,
            Action::Warp => GameplayCommand::Warp,
            Action::TimeScale { scale } => GameplayCommand::TimeScale(scale.clamped(0.2, 2.0)),
            Action::SetEnvironment { settings } => {
                GameplayCommand::SetEnvironment(Box::new(settings))
            }
            Action::RequestBrickGroups => GameplayCommand::RequestBrickGroups,
            Action::RequestMaps => GameplayCommand::RequestMaps,
        };
        Ok(vec![Effect::Gameplay { actor, command }])
    }
}

impl Administration {
    /// Save (or, for `Player`, forget) the rank `principal` gets on joining.
    fn set_auto_role(
        &mut self,
        principal: Principal,
        role: Role,
        name: String,
    ) -> Result<(), Error> {
        let at = self
            .durable
            .auto_roles
            .iter()
            .position(|a| a.principal == principal);
        if at.is_none() && role != Role::Player && self.durable.auto_roles.len() >= MAX_AUTO_ROLES {
            return Err(Error::Budget);
        }
        if let Some(i) = at {
            self.durable.auto_roles.remove(i);
        }
        if role != Role::Player {
            self.durable.auto_roles.push(AutoRole {
                principal,
                role,
                name,
            });
        }
        Ok(())
    }
}

/// Stock minigame commands check ownership, not administration. Host override is
/// deliberately not implicit here; the minigame subsystem applies its own rules.
pub fn owns_minigame(actor: ConnectionId, owner: ConnectionId) -> bool {
    actor == owner
}
