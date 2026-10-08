//! Administration DTOs and UI state. None of these values confer server authority.
use crate::api::RequestId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminRole {
    Player,
    Admin,
    SuperAdmin,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AdminFeature {
    Login,
    Kick,
    Ban,
    Unban,
    Spy,
    Wand,
    Maps,
    ClearBricks,
    HighlightBricks,
    HostOptions,
    AdminPassword,
    /// Make players Admin or Super Admin, or take it away (the host and
    /// Super Admins).
    Ranks,
    /// The Environment window: sun, sky, fog and the day/night cycle.
    Environment,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminPlayer {
    pub connection: u64,
    pub name: String,
    pub identity_label: String,
    pub role: AdminRole,
    pub owner: bool,
    pub local: bool,
    pub bot: bool,
    pub persistent_identity: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminBan {
    pub id: u64,
    pub administrator: String,
    pub name: String,
    pub identity_label: String,
    /// Absent when no original-compatible address display is supplied by the host.
    pub address: Option<String>,
    pub reason: String,
    pub remaining_minutes: Option<u64>,
}
/// A saved rank: the player gets it back when they rejoin with this key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminSavedRank {
    /// The player's verified key, as hex; it names the row.
    pub key: String,
    /// The name they had when the rank was given.
    pub name: String,
    pub role: AdminRole,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminBrickGroup {
    pub id: u64,
    pub name: String,
    pub identity_label: String,
    pub bricks: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminMap {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminOptions {
    pub name: String,
    pub port: u16,
    pub max_players: u16,
    pub brick_limit: u32,
    pub bricks_per_second: u32,
    pub max_chat_length: u32,
    pub physics_vehicles: u32,
    pub player_vehicles: u32,
    /// Bots the server runs at once (`$Pref::Server::MaxBots`, 1 to
    /// [`MOST_BOTS`]).
    #[serde(default = "default_max_bots")]
    pub max_bots: u32,
    pub random_brick_color: bool,
    pub chat_filter: bool,
    pub falling_damage: bool,
    pub public_domain_timeout_minutes: i32,
    pub too_far_distance: f32,
    pub per_player: AdminQuotas,
    pub lan: AdminQuotas,
    /// Running Add-Ons' server-wide settings changed from their defaults,
    /// by `namespace:key` (the Admin menu's Add-On Settings).
    #[serde(default)]
    pub addon_settings: BTreeMap<String, crate::api::MiniGameSettingValue>,
}
/// The most Max bots may be, as `bri_admin::MOST_BOTS`.
pub const MOST_BOTS: u32 = 32;
/// Max bots before the host changes it, as `bri_admin::DEFAULT_BOTS`.
pub const DEFAULT_BOTS: u32 = 16;
fn default_max_bots() -> u32 {
    DEFAULT_BOTS
}
/// v20's `$Pref::Server::*` defaults, as `bri_admin::ServerSettings::default`.
impl Default for AdminOptions {
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
            max_bots: DEFAULT_BOTS,
            random_brick_color: false,
            chat_filter: true,
            falling_damage: true,
            public_domain_timeout_minutes: -1,
            too_far_distance: 50.,
            per_player: AdminQuotas {
                schedules: 50,
                misc: 100,
                projectiles: 25,
                items: 25,
                environment: 100,
                players: 10,
                vehicles: 5,
            },
            lan: AdminQuotas {
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
/// The settings as v20's `$Pref::Server::` names (without the prefix) and
/// values, in serverConfigGui's order. Checkboxes are `1` or `0`.
pub fn option_pairs(o: &AdminOptions) -> Vec<(&'static str, String)> {
    let mut p = vec![
        ("Port", o.port.to_string()),
        ("BrickLimit", o.brick_limit.to_string()),
        ("MaxBricksPerSecond", o.bricks_per_second.to_string()),
        ("MaxChatLen", o.max_chat_length.to_string()),
        ("MaxPhysVehicles_Total", o.physics_vehicles.to_string()),
        ("MaxPlayerVehicles_Total", o.player_vehicles.to_string()),
        ("MaxBots", o.max_bots.to_string()),
        (
            "RandomBrickColor",
            u8::from(o.random_brick_color).to_string(),
        ),
        ("ETardFilter", u8::from(o.chat_filter).to_string()),
        ("FallingDamage", u8::from(o.falling_damage).to_string()),
        (
            "BrickPublicDomainTimeout",
            o.public_domain_timeout_minutes.to_string(),
        ),
        ("TooFarDistance", o.too_far_distance.to_string()),
    ];
    for (q, keys) in [
        (
            &o.per_player,
            [
                "Quota::Schedules",
                "Quota::Misc",
                "Quota::Projectile",
                "Quota::Item",
                "Quota::Environment",
                "Quota::Player",
                "Quota::Vehicle",
            ],
        ),
        (
            &o.lan,
            [
                "QuotaLAN::Schedules",
                "QuotaLAN::Misc",
                "QuotaLAN::Projectile",
                "QuotaLAN::Item",
                "QuotaLAN::Environment",
                "QuotaLAN::Player",
                "QuotaLAN::Vehicle",
            ],
        ),
    ] {
        for (k, n) in keys.into_iter().zip([
            q.schedules,
            q.misc,
            q.projectiles,
            q.items,
            q.environment,
            q.players,
            q.vehicles,
        ]) {
            p.push((k, n.to_string()));
        }
    }
    p
}
/// Set the setting named as in [`option_pairs`] from its text.
pub fn set_option(o: &mut AdminOptions, key: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    let number = || value.parse::<u32>().map_err(|_| format!("Invalid {key}"));
    let flag = || value.parse::<f64>().map_or(!value.is_empty(), |v| v != 0.0);
    match key {
        "Port" => {
            o.port = u16::try_from(number()?)
                .ok()
                .filter(|p| *p >= 1024)
                .ok_or("Port must be 1024–65535.")?
        }
        "BrickLimit" => o.brick_limit = number()?,
        "MaxBricksPerSecond" => o.bricks_per_second = number()?,
        "MaxChatLen" => o.max_chat_length = number()?,
        "MaxPhysVehicles_Total" => o.physics_vehicles = number()?,
        "MaxPlayerVehicles_Total" => o.player_vehicles = number()?,
        "MaxBots" => {
            o.max_bots = number()
                .ok()
                .filter(|n| (1..=MOST_BOTS).contains(n))
                .ok_or(format!("Max bots must be 1–{MOST_BOTS}."))?
        }
        "RandomBrickColor" => o.random_brick_color = flag(),
        "ETardFilter" => o.chat_filter = flag(),
        "FallingDamage" => o.falling_damage = flag(),
        "BrickPublicDomainTimeout" => {
            o.public_domain_timeout_minutes = value
                .parse()
                .ok()
                .filter(|m| *m >= -1)
                .ok_or("Invalid public-domain timeout")?
        }
        "TooFarDistance" => {
            o.too_far_distance = value
                .parse()
                .ok()
                .filter(|d: &f32| d.is_finite() && *d >= 0.)
                .ok_or("Invalid distance")?
        }
        _ => {
            let (q, field) = match key.split_once("::") {
                Some(("Quota", field)) => (&mut o.per_player, field),
                Some(("QuotaLAN", field)) => (&mut o.lan, field),
                _ => return Err(format!("Unknown setting {key}")),
            };
            let n = number()?;
            match field {
                "Schedules" => q.schedules = n,
                "Misc" => q.misc = n,
                "Projectile" => q.projectiles = n,
                "Item" => q.items = n,
                "Environment" => q.environment = n,
                "Player" => q.players = n,
                "Vehicle" => q.vehicles = n,
                _ => return Err(format!("Unknown setting {key}")),
            }
        }
    }
    Ok(())
}
/// The settings saved in `$Pref::Server::*`, over v20's defaults. A value
/// that does not parse keeps its default.
pub fn options_from_prefs(prefs: &crate::prefs::Prefs) -> AdminOptions {
    let mut o = AdminOptions::default();
    for (key, _) in option_pairs(&AdminOptions::default()) {
        if let Some(value) = prefs.get(&format!("$Pref::Server::{key}")) {
            let mut next = o.clone();
            if set_option(&mut next, key, value).is_ok() {
                o = next;
            }
        }
    }
    for name in prefs.keys() {
        if let Some((key, value)) = addon_pref(&name).zip(prefs.get(&name))
            && let Some(value) = addon_pref_value(value)
        {
            o.addon_settings.insert(key, value);
        }
    }
    o
}
/// Save the settings as `$Pref::Server::*`, as v20's serverConfigGui did.
/// Add-On settings save as `$Pref::Server::AddOn::<namespace>::<key>`.
pub fn options_to_prefs(o: &AdminOptions, prefs: &mut crate::prefs::Prefs) {
    for (key, value) in option_pairs(o) {
        prefs.set(&format!("$Pref::Server::{key}"), value);
    }
    for name in prefs.keys() {
        if addon_pref(&name).is_some() {
            prefs.reset(&name);
        }
    }
    for (key, value) in &o.addon_settings {
        let Some((namespace, key)) = key.split_once(':') else {
            continue;
        };
        use crate::api::MiniGameSettingValue as V;
        let value = match value {
            V::Bool(b) => serde_json::Value::Bool(*b),
            V::Int(n) => serde_json::Value::from(*n),
            V::Text(t) => serde_json::Value::String(t.clone()),
        };
        prefs.set(
            &format!("{ADDON_PREF}{namespace}::{key}"),
            value.to_string(),
        );
    }
}
const ADDON_PREF: &str = "$Pref::Server::AddOn::";
/// `namespace:key` of an Add-On setting's pref name.
fn addon_pref(name: &str) -> Option<String> {
    let rest = name
        .get(..ADDON_PREF.len())
        .filter(|p| p.eq_ignore_ascii_case(ADDON_PREF))
        .map(|_| &name[ADDON_PREF.len()..])?;
    let (namespace, key) = rest.split_once("::")?;
    Some(format!(
        "{}:{}",
        namespace.to_ascii_lowercase(),
        key.to_ascii_lowercase()
    ))
}
/// A saved Add-On setting value: `true`, `35` or a quoted text.
fn addon_pref_value(value: &str) -> Option<crate::api::MiniGameSettingValue> {
    use crate::api::MiniGameSettingValue as V;
    match serde_json::from_str(value).ok()? {
        serde_json::Value::Bool(b) => Some(V::Bool(b)),
        serde_json::Value::Number(n) => n.as_i64().map(V::Int),
        serde_json::Value::String(t) => Some(V::Text(t)),
        _ => None,
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminQuotas {
    pub schedules: u32,
    pub misc: u32,
    pub projectiles: u32,
    pub items: u32,
    pub environment: u32,
    pub players: u32,
    pub vehicles: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminSnapshot {
    pub revision: u64,
    pub role: AdminRole,
    pub local_host: bool,
    pub legacy_lan: bool,
    /// Host advertises only operations whose backend is actually installed.
    pub supported: BTreeSet<AdminFeature>,
    pub players: Vec<AdminPlayer>,
    pub options: Option<AdminOptions>,
}
/// Secret fields redact Debug; only an explicit request sends their content.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AdminSecret(pub String);
impl std::fmt::Debug for AdminSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminPasswordSlot {
    Join,
    Admin,
    SuperAdmin,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AdminAction {
    Refresh,
    Login {
        password: AdminSecret,
    },
    Kick {
        target: u64,
    },
    Ban {
        target: u64,
        minutes: Option<u32>,
        reason: String,
    },
    RequestBans,
    Unban {
        ban: u64,
    },
    Spy {
        target: u64,
    },
    Wand,
    RequestMaps,
    ChangeMap {
        map: String,
    },
    RequestBrickGroups,
    HighlightBrickGroup {
        group: u64,
    },
    ClearBrickGroup {
        group: u64,
    },
    ClearAllBricks,
    ConfigureHost {
        options: Box<AdminOptions>,
    },
    SetPassword {
        slot: AdminPasswordSlot,
        password: AdminSecret,
    },
    SetRole {
        target: u64,
        role: AdminRole,
    },
    RequestRanks,
    /// Take a saved rank off the list; anyone online keeps theirs until
    /// they leave.
    ForgetRank {
        key: String,
    },
    /// Replace the environment settings (the Environment window's Apply).
    SetEnvironment {
        settings: Box<bri_content::atmosphere::Settings>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AdminUpdate {
    State(AdminSnapshot),
    Bans {
        request: RequestId,
        revision: u64,
        rows: Vec<AdminBan>,
    },
    BrickGroups {
        request: RequestId,
        revision: u64,
        rows: Vec<AdminBrickGroup>,
    },
    Maps {
        request: RequestId,
        revision: u64,
        rows: Vec<AdminMap>,
    },
    Ranks {
        request: RequestId,
        revision: u64,
        rows: Vec<AdminSavedRank>,
    },
}
#[derive(Debug, Clone)]
pub struct AdminConfirmation {
    pub title: String,
    pub text: String,
    pub action: AdminAction,
}
#[derive(Debug, Clone, Default)]
pub struct AdminModel {
    pub snapshot: Option<AdminSnapshot>,
    pub bans: Vec<AdminBan>,
    pub groups: Vec<AdminBrickGroup>,
    pub maps: Vec<AdminMap>,
    pub saved_ranks: Vec<AdminSavedRank>,
    pub selected_player: Option<u64>,
    pub selected_rank: Option<String>,
    pub selected_ban: Option<u64>,
    pub selected_group: Option<u64>,
    pub selected_map: Option<String>,
    pub pending: BTreeMap<RequestId, AdminAction>,
    pub confirmation: Option<AdminConfirmation>,
    pub status: String,
    pub revision: u64,
}
fn text_ok(s: &str, max: usize) -> bool {
    s.len() <= max
        && !s
            .chars()
            .any(|c| c.is_control() || ('\u{e000}'..='\u{e0ff}').contains(&c))
}
fn unique(ids: impl Iterator<Item = u64>) -> bool {
    let mut seen = BTreeSet::new();
    ids.into_iter().all(|id| id > 0 && seen.insert(id))
}
impl AdminModel {
    pub fn is_admin(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|s| s.role != AdminRole::Player || s.local_host)
    }
    pub fn available(&self, f: AdminFeature) -> bool {
        self.snapshot.as_ref().is_some_and(|s| {
            s.supported.contains(&f)
                && match f {
                    AdminFeature::Login => true,
                    AdminFeature::HostOptions => s.local_host,
                    AdminFeature::AdminPassword | AdminFeature::Ranks => {
                        s.local_host || s.role == AdminRole::SuperAdmin
                    }
                    AdminFeature::Ban | AdminFeature::Unban => self.is_admin() && !s.legacy_lan,
                    _ => self.is_admin(),
                }
        })
    }
    pub fn player(&self, id: u64) -> Option<&AdminPlayer> {
        self.snapshot
            .as_ref()?
            .players
            .iter()
            .find(|p| p.connection == id)
    }
    pub fn allowed(&self, a: &AdminAction) -> bool {
        let (f, target) = match a {
            AdminAction::Refresh => return true,
            AdminAction::Login { .. } => (AdminFeature::Login, None),
            AdminAction::Kick { target } => (AdminFeature::Kick, Some(*target)),
            AdminAction::Ban { target, .. } => (AdminFeature::Ban, Some(*target)),
            AdminAction::Spy { target } => (AdminFeature::Spy, Some(*target)),
            AdminAction::RequestBans | AdminAction::Unban { .. } => (AdminFeature::Unban, None),
            AdminAction::Wand => (AdminFeature::Wand, None),
            AdminAction::RequestMaps | AdminAction::ChangeMap { .. } => (AdminFeature::Maps, None),
            AdminAction::RequestBrickGroups
            | AdminAction::ClearBrickGroup { .. }
            | AdminAction::ClearAllBricks => (AdminFeature::ClearBricks, None),
            AdminAction::HighlightBrickGroup { .. } => (AdminFeature::HighlightBricks, None),
            AdminAction::ConfigureHost { .. } => (AdminFeature::HostOptions, None),
            AdminAction::SetPassword {
                slot: AdminPasswordSlot::Admin,
                ..
            } => (AdminFeature::AdminPassword, None),
            AdminAction::SetPassword { .. } => (AdminFeature::HostOptions, None),
            AdminAction::SetRole { target, .. } => (AdminFeature::Ranks, Some(*target)),
            AdminAction::RequestRanks | AdminAction::ForgetRank { .. } => {
                (AdminFeature::Ranks, None)
            }
            AdminAction::SetEnvironment { settings } => {
                if settings.validate().is_err() {
                    return false;
                }
                (AdminFeature::Environment, None)
            }
        };
        if !self.available(f) {
            return false;
        }
        if let Some(id) = target {
            let Some(p) = self.player(id) else {
                return false;
            };
            if matches!(a, AdminAction::Kick { .. })
                && !p.bot
                && (p.owner || p.local || p.role == AdminRole::SuperAdmin)
            {
                return false;
            }
            // The host's rank is fixed, and a rank already held is no change.
            if let AdminAction::SetRole { role, .. } = a
                && (p.owner || p.local || p.bot || p.role == *role)
            {
                return false;
            }
            if matches!(a, AdminAction::Ban { .. })
                && (!p.persistent_identity || p.owner || p.local || p.role == AdminRole::SuperAdmin)
            {
                return false;
            }
        }
        match a {
            AdminAction::Unban { ban } => self.bans.iter().any(|b| b.id == *ban),
            AdminAction::ClearBrickGroup { group } | AdminAction::HighlightBrickGroup { group } => {
                self.groups.iter().any(|g| g.id == *group)
            }
            AdminAction::ChangeMap { map } => self.maps.iter().any(|m| m.id == *map),
            AdminAction::ForgetRank { key } => self.saved_ranks.iter().any(|r| r.key == *key),
            _ => true,
        }
    }
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn apply(&mut self, update: AdminUpdate) -> Result<(), String> {
        match update {
            AdminUpdate::State(s) => {
                if s.revision < self.revision {
                    return Err("Ignored stale administration state.".into());
                }
                if s.players.len() > 1024
                    || !unique(s.players.iter().map(|p| p.connection))
                    || s.players
                        .iter()
                        .any(|p| !text_ok(&p.name, 128) || !text_ok(&p.identity_label, 128))
                {
                    return Err("Invalid administration player list.".into());
                }
                let logged_in = s.role != AdminRole::Player || s.local_host;
                self.revision = s.revision;
                self.snapshot = Some(s);
                self.pending.retain(|_, a| {
                    !(matches!(a, AdminAction::Refresh)
                        || logged_in && matches!(a, AdminAction::Login { .. }))
                });
                if self
                    .selected_player
                    .is_some_and(|id| self.player(id).is_none())
                {
                    self.selected_player = None;
                }
                self.status.clear();
            }
            AdminUpdate::Bans {
                request,
                revision,
                rows,
            } => {
                if revision < self.revision
                    || !matches!(self.pending.get(&request), Some(AdminAction::RequestBans))
                {
                    return Err("Ignored stale ban list.".into());
                }
                if rows.len() > 4096
                    || !unique(rows.iter().map(|r| r.id))
                    || rows.iter().any(|r| {
                        !text_ok(&r.name, 128)
                            || !text_ok(&r.administrator, 128)
                            || !text_ok(&r.identity_label, 128)
                            || !text_ok(&r.reason, 512)
                            || r.address.as_ref().is_some_and(|s| !text_ok(s, 128))
                    })
                {
                    return Err("Invalid ban list.".into());
                }
                self.bans = rows;
                if self
                    .selected_ban
                    .is_some_and(|id| !self.bans.iter().any(|b| b.id == id))
                {
                    self.selected_ban = None;
                }
                self.pending.remove(&request);
                self.status.clear();
            }
            AdminUpdate::BrickGroups {
                request,
                revision,
                rows,
            } => {
                if revision < self.revision
                    || !matches!(
                        self.pending.get(&request),
                        Some(AdminAction::RequestBrickGroups)
                    )
                {
                    return Err("Ignored stale brick list.".into());
                }
                // Owner zero is the real unowned/imported brick group, not a
                // connection handle. It must remain selectable and clearable.
                if rows.len() > 4096
                    || rows.iter().map(|r| r.id).collect::<BTreeSet<_>>().len() != rows.len()
                    || rows
                        .iter()
                        .any(|r| !text_ok(&r.name, 128) || !text_ok(&r.identity_label, 128))
                {
                    return Err("Invalid brick list.".into());
                }
                self.groups = rows;
                self.pending.remove(&request);
                self.status.clear();
            }
            AdminUpdate::Maps {
                request,
                revision,
                rows,
            } => {
                if revision < self.revision
                    || !matches!(self.pending.get(&request), Some(AdminAction::RequestMaps))
                {
                    return Err("Ignored stale map list.".into());
                }
                let mut seen = BTreeSet::new();
                if rows.len() > 1024
                    || rows.iter().any(|r| {
                        !text_ok(&r.id, 128)
                            || r.id.is_empty()
                            || !seen.insert(&r.id)
                            || !text_ok(&r.name, 128)
                    })
                {
                    return Err("Invalid map list.".into());
                }
                self.maps = rows;
                self.pending.remove(&request);
                self.status.clear();
            }
            AdminUpdate::Ranks {
                request,
                revision,
                rows,
            } => {
                if revision < self.revision
                    || !matches!(self.pending.get(&request), Some(AdminAction::RequestRanks))
                {
                    return Err("Ignored stale rank list.".into());
                }
                let mut seen = BTreeSet::new();
                if rows.len() > 4096
                    || rows.iter().any(|r| {
                        r.key.len() != 64
                            || !r.key.bytes().all(|b| b.is_ascii_hexdigit())
                            || !seen.insert(&r.key)
                            || !text_ok(&r.name, 128)
                            || r.role == AdminRole::Player
                    })
                {
                    return Err("Invalid rank list.".into());
                }
                self.saved_ranks = rows;
                if self
                    .selected_rank
                    .as_ref()
                    .is_some_and(|key| !self.saved_ranks.iter().any(|r| r.key == *key))
                {
                    self.selected_rank = None;
                }
                self.pending.remove(&request);
                self.status.clear();
            }
        }
        if self
            .confirmation
            .as_ref()
            .is_some_and(|c| !self.allowed(&c.action))
        {
            self.confirmation = None;
            self.status = "The selected target or permission changed. Action canceled.".into();
        }
        Ok(())
    }
    pub fn result(&mut self, id: RequestId, result: &Result<(), String>) -> bool {
        let Some(action) = self.pending.get(&id).cloned() else {
            return false;
        };
        match result {
            Err(e) => {
                self.pending.remove(&id);
                self.status = format!("Rejected: {}", plain(e));
            }
            Ok(())
                if matches!(
                    action,
                    AdminAction::Refresh
                        | AdminAction::RequestBans
                        | AdminAction::RequestBrickGroups
                        | AdminAction::RequestMaps
                        | AdminAction::RequestRanks
                        | AdminAction::Login { .. }
                ) =>
            {
                self.status = "Waiting for authoritative state...".into();
            }
            Ok(()) => {
                self.pending.remove(&id);
                if let AdminAction::ForgetRank { key } = &action {
                    self.saved_ranks.retain(|row| row.key != *key);
                    if self.selected_rank.as_ref() == Some(key) {
                        self.selected_rank = None;
                    }
                }
                if let AdminAction::Unban { ban } = action {
                    // Only remove after the host's correlated acknowledgement.
                    self.bans.retain(|row| row.id != ban);
                    if self.selected_ban == Some(ban) {
                        self.selected_ban = None;
                    }
                }
                self.status = "Action accepted by the host.".into();
            }
        }
        true
    }
}
pub fn plain(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() && !('\u{e000}'..='\u{e0ff}').contains(c))
        .take(512)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::Prefs;

    #[test]
    fn server_prefs_round_trip_and_keep_defaults_for_bad_values() {
        let mut prefs = Prefs::default();
        assert_eq!(options_from_prefs(&prefs), AdminOptions::default());
        let mut o = AdminOptions {
            max_chat_length: 60,
            random_brick_color: true,
            ..Default::default()
        };
        o.lan.projectiles = 7;
        options_to_prefs(&o, &mut prefs);
        assert_eq!(prefs.get("$Pref::Server::QuotaLAN::Projectile"), Some("7"));
        assert_eq!(options_from_prefs(&prefs), o);
        prefs.set("$Pref::Server::Port", "80");
        prefs.set("$Pref::Server::MaxChatLen", "lots");
        let back = options_from_prefs(&prefs);
        assert_eq!((back.port, back.max_chat_length), (28000, 120));
    }

    #[test]
    fn addon_server_settings_save_as_prefs_and_drop_when_reset() {
        use crate::api::MiniGameSettingValue as V;
        let mut prefs = Prefs::default();
        let mut o = AdminOptions::default();
        o.addon_settings.insert("tier:tt_ammo".into(), V::Int(2));
        o.addon_settings
            .insert("tier:tt_display".into(), V::Bool(false));
        o.addon_settings
            .insert("tier:tt_name".into(), V::Text("1 \"two\"".into()));
        options_to_prefs(&o, &mut prefs);
        assert_eq!(prefs.get("$Pref::Server::AddOn::tier::tt_ammo"), Some("2"));
        assert_eq!(options_from_prefs(&prefs), o);
        o.addon_settings.remove("tier:tt_ammo");
        options_to_prefs(&o, &mut prefs);
        assert_eq!(prefs.get("$Pref::Server::AddOn::tier::tt_ammo"), None);
        assert_eq!(options_from_prefs(&prefs), o);
        prefs.set("$Pref::Server::AddOn::tier::tt_bad", "not json");
        assert_eq!(
            options_from_prefs(&prefs),
            o,
            "an unreadable value is left out"
        );
    }
}
