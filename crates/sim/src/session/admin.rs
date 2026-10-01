use super::{ControlObject, Session};
use anyhow::{Context, Result, ensure};
use bri_admin::{
    Action, Administration, BanRecord, ConnectionId, DurableState, Effect, GameplayCommand, Origin,
    PasswordSlot, Principal, Request, Role, Secret, TrustedConnection,
};
use bri_world::OwnerId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminCapability {
    Login,
    Kick,
    Ban,
    Unban,
    ClearBricks,
    AdminPassword,
    /// Brick Management "Hilight".
    HighlightBricks,
    /// `/realBrickCount`, `/cancelAllEvents`, `/clearBots`.
    WorldCommands,
    DestructoWand,
    Spy,
    /// `/fetch`, `/find`, `/warp`.
    Teleport,
    /// `/resetVehicles`, `/clearVehicles`.
    Vehicles,
    /// `/timeScale`.
    TimeScale,
    /// The Admin Menu's Environment window.
    Environment,
    /// Admin menu Change Map.
    ChangeMap,
    /// Server settings (brick limit, plant rate, chat length, reach).
    HostOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminPlayer {
    pub connection: u64,
    pub name: String,
    pub identity_label: String,
    pub role: Role,
    pub owner: bool,
    pub local: bool,
    pub bot: bool,
    pub persistent_identity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminBrickGroup {
    pub id: u64,
    pub name: String,
    pub identity_label: String,
    pub bricks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AdminData {
    None,
    LoginRejected {
        attempts: u8,
    },
    BrickGroups(Vec<AdminBrickGroup>),
    BanList {
        rows: Vec<BanRecord>,
        now_unix_seconds: u64,
    },
    /// `serverCmdGetMapList`.
    Maps(Vec<MapListing>),
    /// The saved ranks players get back when they rejoin.
    AutoRoles(Vec<bri_admin::AutoRole>),
}

/// A map the host can change to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapListing {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminSnapshot {
    pub revision: u64,
    pub role: Role,
    pub local_host: bool,
    pub legacy_lan: bool,
    pub supported: BTreeSet<AdminCapability>,
    pub players: Vec<AdminPlayer>,
    /// The server settings, for whoever may change them.
    pub options: Option<bri_admin::ServerSettings>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminReply {
    pub snapshot: AdminSnapshot,
    pub data: AdminData,
}

#[derive(Debug)]
pub struct AdminCall {
    pub reply: AdminReply,
}

/// The close message a disconnected player sees (v20 showed the kick or ban
/// reason and how long a ban lasts).
pub fn disconnect_message(
    reason: &bri_admin::DisconnectReason,
    durable: &DurableState,
    now: u64,
) -> String {
    match reason {
        bri_admin::DisconnectReason::Kicked => "You were kicked from the server by an admin.".into(),
        bri_admin::DisconnectReason::FailedPasswords => {
            "You were disconnected after too many wrong admin passwords.".into()
        }
        bri_admin::DisconnectReason::Banned(id) => {
            let ban = durable.bans.iter().find(|b| b.id == *id);
            let length = match ban.and_then(|b| b.expires_unix_seconds) {
                None if ban.is_some() => " permanently".to_string(),
                None => String::new(),
                Some(end) => {
                    let minutes = end.saturating_sub(now).div_ceil(60).max(1);
                    match minutes {
                        1 => " for 1 minute".into(),
                        m if m < 120 => format!(" for {m} minutes"),
                        m if m < 48 * 60 => format!(" for {} hours", m.div_ceil(60)),
                        m => format!(" for {} days", m.div_ceil(24 * 60)),
                    }
                }
            };
            let reason: String = ban
                .map(|b| b.reason.chars().filter(|c| !c.is_control()).take(120).collect())
                .unwrap_or_default();
            let reason = reason.trim();
            if reason.is_empty() {
                format!("You were banned from this server{length}.")
            } else {
                format!("You were banned from this server{length}. Reason: {reason}")
            }
        }
    }
}

#[derive(Default)]
pub(super) struct AdminRuntime {
    authority: Administration,
    owner_to_connection: BTreeMap<OwnerId, ConnectionId>,
    connection_to_owner: BTreeMap<ConnectionId, OwnerId>,
    next_connection: u64,
    revision: u64,
    passwords: BTreeMap<PasswordSlot, Secret>,
    /// The host installed a map list, so Change Map works.
    pub(super) maps_available: bool,
    /// Server settings the host applies (v20's `$Pref::Server::*`).
    pub(super) settings: bri_admin::ServerSettings,
}

impl AdminRuntime {
    pub(super) fn set_passwords(&mut self, admin: Secret, super_admin: Secret) {
        for (slot, password) in [
            (PasswordSlot::Admin, admin),
            (PasswordSlot::SuperAdmin, super_admin),
        ] {
            if password.expose().is_empty() {
                self.passwords.remove(&slot);
            } else {
                self.passwords.insert(slot, password);
            }
        }
    }
    pub(super) fn restore(&mut self, bytes: &[u8]) -> Result<()> {
        self.authority.restore(bytes)?;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }
    pub(super) fn durable(&self) -> &DurableState {
        self.authority.durable()
    }
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn connect(
        &mut self,
        owner: OwnerId,
        name: String,
        trusted_host: bool,
        is_bot: bool,
        principal: Option<Principal>,
        now_unix_seconds: u64,
    ) -> Result<Role> {
        let value = self
            .next_connection
            .checked_add(1)
            .context("Administration connection IDs exhausted")?;
        let id = ConnectionId(value);
        let role = self.authority.connect(
            TrustedConnection {
                id,
                display_name: name,
                principal,
                is_owner: trusted_host,
                is_local: trusted_host,
                is_bot,
            },
            now_unix_seconds,
        )?;
        self.next_connection = value;
        self.owner_to_connection.insert(owner, id);
        self.connection_to_owner.insert(id, owner);
        self.revision = self.revision.saturating_add(1);
        Ok(role)
    }

    pub(super) fn rename(&mut self, owner: OwnerId, name: String) -> Result<()> {
        let id = *self
            .owner_to_connection
            .get(&owner)
            .context("Unknown administration connection")?;
        self.authority.rename(id, name)?;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub(super) fn disconnect(&mut self, owner: OwnerId) {
        if let Some(id) = self.owner_to_connection.remove(&owner) {
            self.connection_to_owner.remove(&id);
            self.authority.disconnect(id);
            self.revision = self.revision.saturating_add(1);
        }
    }

    pub(super) fn snapshot(&self, owner: OwnerId) -> Result<AdminSnapshot> {
        let id = *self
            .owner_to_connection
            .get(&owner)
            .context("Unknown administration connection")?;
        let role = self
            .authority
            .role(id)
            .context("Unknown administration role")?;
        let players = self
            .authority
            .rows()
            .into_iter()
            .map(|row| AdminPlayer {
                connection: row.connection.0,
                name: row.display_name,
                identity_label: if row.durable_identity_available {
                    "Verified local-key pseudonym".to_owned()
                } else {
                    "Unavailable".to_owned()
                },
                role: row.role,
                owner: row.is_owner,
                local: row.is_local,
                bot: row.is_bot,
                persistent_identity: row.durable_identity_available,
            })
            .collect();
        let rows = self.authority.rows();
        let mut supported: BTreeSet<AdminCapability> = [
            AdminCapability::Login,
            AdminCapability::Kick,
            AdminCapability::ClearBricks,
        ]
        .into_iter()
        .collect();
        if role.is_admin() {
            supported.insert(AdminCapability::Unban);
            supported.insert(AdminCapability::HighlightBricks);
            supported.insert(AdminCapability::WorldCommands);
            supported.insert(AdminCapability::DestructoWand);
            supported.insert(AdminCapability::Spy);
            supported.insert(AdminCapability::Teleport);
            supported.insert(AdminCapability::Vehicles);
            supported.insert(AdminCapability::TimeScale);
            supported.insert(AdminCapability::Environment);
            if self.maps_available {
                supported.insert(AdminCapability::ChangeMap);
            }
            if rows.iter().any(|row| {
                row.durable_identity_available
                    && !row.is_owner
                    && !row.is_local
                    && row.role != Role::SuperAdmin
            }) {
                supported.insert(AdminCapability::Ban);
            }
        }
        if role == Role::SuperAdmin || self.authority.host_authority(Origin::Connection(id))? {
            supported.insert(AdminCapability::AdminPassword);
        }
        let host = self.authority.host_authority(Origin::Connection(id))?;
        if host {
            supported.insert(AdminCapability::HostOptions);
        }
        Ok(AdminSnapshot {
            revision: self.revision,
            role,
            local_host: host,
            legacy_lan: false,
            supported,
            players,
            options: host.then(|| self.settings.clone()),
        })
    }

    fn target_owner(&self, target: ConnectionId) -> Result<OwnerId> {
        self.connection_to_owner
            .get(&target)
            .copied()
            .context("That player is no longer connected")
    }

    fn request(
        &mut self,
        session: &mut Session,
        owner: OwnerId,
        request: Request,
        now: u64,
        persist: &mut impl FnMut(&DurableState) -> Result<()>,
    ) -> Result<AdminCall> {
        let id = *self
            .owner_to_connection
            .get(&owner)
            .context("Unknown administration connection")?;
        let origin = Origin::Connection(id);
        request.validate()?;
        let supported = matches!(
            &request.action,
            Action::Login { .. }
                | Action::Kick { .. }
                | Action::Ban { .. }
                | Action::Unban { .. }
                | Action::RequestBanList
                | Action::ClearAllBricks
                | Action::ClearBrickGroup { .. }
                | Action::RequestBrickGroups
                | Action::HighlightBrickGroup { .. }
                | Action::RealBrickCount
                | Action::CancelAllEvents
                | Action::ClearBots
                | Action::DestructoWand
                | Action::Spy { .. }
                | Action::DropCameraAtPlayer
                | Action::Fetch { .. }
                | Action::Find { .. }
                | Action::Warp
                | Action::ResetVehicles
                | Action::ClearVehicles
                | Action::TimeScale { .. }
                | Action::SetEnvironment { .. }
                | Action::RequestMaps
                | Action::ChangeMap { .. }
                | Action::SetAdminPassword { .. }
                | Action::HostSetRole { .. }
                | Action::HostSetAutoRole { .. }
                | Action::RequestAutoRoles
                | Action::HostConfigure { .. }
                | Action::HostSetPassword {
                    slot: PasswordSlot::Admin | PasswordSlot::SuperAdmin,
                    ..
                }
        );
        if !supported {
            self.authority.permission(origin, &request.action)?;
            anyhow::bail!("Administration action has no installed host adapter");
        }
        if matches!(
            &request.action,
            Action::HostSetPassword {
                slot: PasswordSlot::Join,
                ..
            }
        ) {
            self.authority.permission(origin, &request.action)?;
            anyhow::bail!("Join password is not connected to transport admission");
        }
        // v20's `MsgAdminForce` lines need names and the ban terms before
        // the request is consumed.
        let actor_name = session
            .peers
            .get(&owner)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let prior_role = self.authority.role(id);
        let prior_target_role = match &request.action {
            Action::HostSetRole { target, .. } => self.authority.role(*target),
            _ => None,
        };
        let ban_line = match &request.action {
            Action::Ban {
                target,
                duration,
                reason,
            } => self
                .connection_to_owner
                .get(target)
                .and_then(|victim| session.peers.get(victim))
                .map(|victim| {
                    let label: String = victim
                        .principal
                        .map(|p| p.0[..4].iter().map(|b| format!("{b:02x}")).collect())
                        .unwrap_or_default();
                    let (a, v) = (&actor_name, &victim.name);
                    match duration {
                        bri_admin::BanDuration::Forever => format!(
                            "\u{E003}{a}\u{E002} permanently banned \u{E003}{v}\u{E002} (ID: {label}) - \u{E002}\"{reason}\""
                        ),
                        bri_admin::BanDuration::Minutes(m) => format!(
                            "\u{E003}{a}\u{E002} banned \u{E003}{v}\u{E002} (ID: {label}) for {m} minutes - \u{E002}\"{reason}\""
                        ),
                    }
                }),
            _ => None,
        };
        let passwords = &self.passwords;
        let mut candidate = self.authority.clone();
        let effects = candidate.handle(origin, request, now, |attempt| {
            [PasswordSlot::SuperAdmin, PasswordSlot::Admin]
                .into_iter()
                .find_map(|slot| {
                    passwords
                        .get(&slot)
                        .filter(|configured| secret_eq(configured.expose(), attempt))
                        .map(|_| match slot {
                            PasswordSlot::SuperAdmin => Role::SuperAdmin,
                            PasswordSlot::Admin => Role::Admin,
                            PasswordSlot::Join => unreachable!(),
                        })
                })
        })?;
        if effects
            .iter()
            .any(|effect| matches!(effect, Effect::BansChanged | Effect::AutoRolesChanged))
        {
            persist(candidate.durable())?;
        }
        self.authority = candidate;
        let mut data = AdminData::None;
        let mut disconnects = Vec::new();
        let mut disconnect_messages = BTreeMap::new();
        let mut changed = false;
        if let Some(line) = ban_line {
            session.admin_announce(line);
        }
        for effect in effects {
            match effect {
                Effect::Disconnect { target, reason } => {
                    if let Some(owner) = self.connection_to_owner.get(&target).copied() {
                        // `serverCmdKick` (the LAN form: no BL_ID to show).
                        if reason == bri_admin::DisconnectReason::Kicked {
                            let name = session.peers.get(&owner).map(|p| p.name.clone());
                            session.admin_announce(format!(
                                "\u{E003}{actor_name}\u{E002} kicked \u{E003}{}",
                                name.unwrap_or_default()
                            ));
                        }
                        disconnects.push(owner);
                        disconnect_messages.insert(
                            owner,
                            disconnect_message(&reason, self.authority.durable(), now),
                        );
                    }
                }
                Effect::RoleChanged { target, role } => {
                    let target_owner = *self
                        .connection_to_owner
                        .get(&target)
                        .context("Administration role target is no longer connected")?;
                    // `serverCmdSAD`: announced only when the level rises.
                    if target != id && prior_target_role != Some(role) {
                        // Ranks given from the Admin menu or `/admin`.
                        let name = session
                            .peers
                            .get(&target_owner)
                            .map(|p| p.name.clone())
                            .unwrap_or_default();
                        session.admin_announce(match role {
                            Role::SuperAdmin => format!(
                                "\u{E003}{actor_name}\u{E002} made \u{E003}{name}\u{E002} Super Admin"
                            ),
                            Role::Admin => format!(
                                "\u{E003}{actor_name}\u{E002} made \u{E003}{name}\u{E002} Admin"
                            ),
                            Role::Player => format!(
                                "\u{E003}{actor_name}\u{E002} removed \u{E003}{name}\u{E002}'s admin"
                            ),
                        });
                    } else if target == id && prior_role != Some(role) {
                        let how = match role {
                            bri_admin::Role::SuperAdmin => Some("Super Admin (Password)"),
                            bri_admin::Role::Admin => Some("Admin (Password)"),
                            _ => None,
                        };
                        if let Some(how) = how {
                            session.admin_announce(format!(
                                "\u{E002}{actor_name} has become {how}"
                            ));
                        }
                    }
                    session.set_role(target_owner, role.is_admin())?;
                    changed = true;
                }
                Effect::PasswordChange { slot, password } => {
                    if password.expose().is_empty() {
                        self.passwords.remove(&slot);
                    } else {
                        self.passwords.insert(slot, password);
                    }
                    changed = true;
                }
                Effect::LoginRejected {
                    attempts,
                    disconnect,
                } => {
                    if disconnect {
                        session.admin_announce(format!(
                            "\u{E003}{actor_name}\u{E002} failed to guess the admin password."
                        ));
                    }
                    data = AdminData::LoginRejected { attempts };
                }
                Effect::Gameplay { actor, command } => {
                    let actor = actor.context("Administration gameplay request needs a player")?;
                    let actor_owner = *self
                        .connection_to_owner
                        .get(&actor)
                        .context("Administration actor disconnected")?;
                    let peer_actor = &session
                        .peers
                        .get(&actor_owner)
                        .context("Administration actor is not in the session")?
                        .actor;
                    let session_actor = peer_actor.clone();
                    match command {
                        GameplayCommand::RequestBrickGroups => {
                            data = AdminData::BrickGroups(brick_groups(session));
                        }
                        GameplayCommand::ClearAllBricks => {
                            let ids: Vec<_> =
                                session.simulation.state().bricks.keys().copied().collect();
                            preflight_removal(session, ids.len())?;
                            // `ServerCmdClearAllBricks`.
                            if !ids.is_empty() {
                                let name = session.peers[&actor_owner].name.clone();
                                session.system_message(
                                    Some(super::MessageTag::ClearBricks),
                                    format!("\u{E003}{name}\u{E000} cleared all bricks."),
                                );
                            }
                            session.simulation.remove_many(&session_actor, &ids)?;
                            session.dirty.extend(ids);
                            changed = true;
                        }
                        GameplayCommand::ClearBrickGroup(group) => {
                            let group: OwnerId = group;
                            let ids: Vec<_> = session
                                .simulation
                                .state()
                                .bricks
                                .iter()
                                .filter_map(|(&brick, b)| (b.owner == group).then_some(brick))
                                .collect();
                            ensure!(!ids.is_empty(), "Unknown brick group");
                            preflight_removal(session, ids.len())?;
                            // `ServerCmdClearBrickGroup`: the LAN host's own
                            // group is just "the bricks".
                            let name = session.peers[&actor_owner].name.clone();
                            let text = if group == actor_owner && session.lan_host {
                                format!("\u{E003}{name}\u{E002} cleared the bricks")
                            } else {
                                let owner = brick_groups(session)
                                    .into_iter()
                                    .find(|g| g.id == group)
                                    .map_or_else(String::new, |g| g.name);
                                format!(
                                    "\u{E003}{name}\u{E002} cleared \u{E003}{owner}\u{E002}'s bricks"
                                )
                            };
                            session.system_message(Some(super::MessageTag::ClearBricks), text);
                            session.simulation.remove_many(&session_actor, &ids)?;
                            session.dirty.extend(ids);
                            changed = true;
                        }
                        GameplayCommand::HighlightBrickGroup(group) => {
                            session.highlight_brick_group(group)?;
                        }
                        GameplayCommand::RealBrickCount => session.brick_count(actor_owner),
                        GameplayCommand::CancelAllEvents => {
                            session.admin_cancel_all_events(actor_owner)
                        }
                        GameplayCommand::ClearBots => session.admin_clear_bots(actor_owner)?,
                        GameplayCommand::DestructoWand => {
                            session.use_admin_wand(actor_owner)?;
                        }
                        GameplayCommand::Spy(target) => {
                            let target = *self
                                .connection_to_owner
                                .get(&target)
                                .context("That player has no body to spy on")?;
                            session.set_control(actor_owner, ControlObject::Spy(target))?;
                        }
                        GameplayCommand::Fetch(target) => {
                            let victim = self.target_owner(target)?;
                            session.admin_fetch(actor_owner, victim)?;
                        }
                        GameplayCommand::Find(target) => {
                            let victim = self.target_owner(target)?;
                            session.admin_find(actor_owner, victim)?;
                        }
                        GameplayCommand::Warp => session.admin_warp(actor_owner)?,
                        GameplayCommand::RequestMaps => {
                            data = AdminData::Maps(session.map_list.clone());
                        }
                        GameplayCommand::ChangeMap(map) => {
                            session.request_map_change(actor_owner, map)?;
                        }
                        GameplayCommand::ResetVehicles => {
                            session.admin_reset_vehicles(actor_owner)?
                        }
                        GameplayCommand::ClearVehicles => {
                            session.admin_clear_vehicles(actor_owner)?
                        }
                        GameplayCommand::TimeScale(scale) => {
                            session.admin_time_scale(actor_owner, scale)?
                        }
                        GameplayCommand::SetEnvironment(settings) => {
                            session.set_environment(*settings)?;
                        }
                        GameplayCommand::DropCameraAtPlayer => {
                            session.drop_camera_at_player(actor_owner)?;
                        }
                        other => {
                            anyhow::bail!("Administration action is not implemented: {other:?}")
                        }
                    }
                }
                Effect::LoginIgnored => {}
                Effect::BanList(rows) => {
                    data = AdminData::BanList {
                        rows,
                        now_unix_seconds: now,
                    }
                }
                Effect::BansChanged | Effect::AutoRolesChanged => changed = true,
                Effect::AutoRoleList(rows) => data = AdminData::AutoRoles(rows),
                Effect::Configure(settings) => {
                    // Applied: brick limit and plant rate (planting), max
                    // chat length and TooFarDistance. The rest are kept and
                    // shown as set.
                    settings.validate()?;
                    session.check_server_addon_settings(&settings.addon_settings)?;
                    self.settings = settings;
                    changed = true;
                }
            }
        }
        if changed {
            self.revision = self.revision.saturating_add(1);
        }
        // Committed effects are published before the reply is built: the
        // reply can fail (a sender who just locked itself out has no
        // snapshot) and must not take the disconnects with it.
        session.admin_disconnects.extend(disconnects);
        session.admin_disconnect_messages.extend(disconnect_messages);
        let snapshot = self.snapshot(owner)?;
        Ok(AdminCall {
            reply: AdminReply { snapshot, data },
        })
    }
}

fn preflight_removal(session: &Session, count: usize) -> Result<()> {
    session
        .simulation
        .state()
        .revision
        .checked_add(u64::try_from(count)?)
        .context("World revision exhausted")?;
    Ok(())
}

fn brick_groups(session: &Session) -> Vec<AdminBrickGroup> {
    let mut counts = BTreeMap::<OwnerId, u64>::new();
    for brick in session.simulation.state().bricks.values() {
        *counts.entry(brick.owner).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(owner, bricks)| {
            let player = session.peers.get(&owner);
            AdminBrickGroup {
                id: owner,
                name: if owner == 0 {
                    "Unowned Bricks".to_owned()
                } else {
                    player.map_or_else(|| format!("Former player {owner}"), |p| p.name.clone())
                },
                identity_label: "Unavailable".to_owned(),
                bricks,
            }
        })
        .collect()
}

fn secret_eq(left: &str, right: &str) -> bool {
    let a = left.as_bytes();
    let b = right.as_bytes();
    let mut difference = a.len() ^ b.len();
    let max = a.len().max(b.len());
    for i in 0..max {
        difference |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    difference == 0
}

impl Session {
    pub(super) fn admin_request(
        &mut self,
        owner: OwnerId,
        request: Request,
        now: u64,
        persist: &mut impl FnMut(&DurableState) -> Result<()>,
    ) -> Result<AdminCall> {
        let mut runtime = std::mem::take(&mut self.admin);
        let result = runtime.request(self, owner, request, now, persist);
        self.admin = runtime;
        result
    }
}

impl Session {
    pub(super) fn admin_connect(
        &mut self,
        owner: OwnerId,
        name: String,
        trusted_host: bool,
        is_bot: bool,
        principal: Option<Principal>,
    ) -> Result<Role> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.admin
            .connect(owner, name, trusted_host, is_bot, principal, now)
            .map_err(|error| {
                // Tell a banned player how long is left and why, as v20 did.
                let durable = self.admin.durable();
                let ban = principal
                    .filter(|_| matches!(error.downcast_ref(), Some(bri_admin::Error::Banned)))
                    .and_then(|p| durable.bans.iter().find(|b| b.principal == p && b.active(now)));
                match ban {
                    Some(ban) => anyhow::anyhow!(
                        "{}",
                        disconnect_message(&bri_admin::DisconnectReason::Banned(ban.id), durable, now)
                            .replacen("You were banned", "You are banned", 1)
                    ),
                    None => error,
                }
            })
    }

    pub fn restore_admin_state(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty(),
            "Admin state can only be restored before clients connect"
        );
        self.admin.restore(bytes)
    }

    pub fn admin_durable_state(&self) -> &DurableState {
        self.admin.durable()
    }

    pub(super) fn admin_disconnect(&mut self, owner: OwnerId) {
        self.admin.disconnect(owner);
    }

    pub fn admin_state(&self, owner: OwnerId) -> Result<AdminSnapshot> {
        self.admin.snapshot(owner)
    }

    pub fn admin_revision(&self) -> u64 {
        self.admin.revision()
    }
}
