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
    pub random_brick_color: bool,
    pub chat_filter: bool,
    pub falling_damage: bool,
    pub public_domain_timeout_minutes: i32,
    pub too_far_distance: f32,
    pub per_player: AdminQuotas,
    pub lan: AdminQuotas,
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
    pub selected_player: Option<u64>,
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
                    AdminFeature::AdminPassword => s.local_host || s.role == AdminRole::SuperAdmin,
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
                        | AdminAction::Login { .. }
                ) =>
            {
                self.status = "Waiting for authoritative state...".into();
            }
            Ok(()) => {
                self.pending.remove(&id);
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
