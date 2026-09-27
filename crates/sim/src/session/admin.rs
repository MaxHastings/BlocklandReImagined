use super::Session;
use anyhow::{Context, Result, ensure};
use bri_admin::{
    Action, Administration, BanRecord, ConnectionId, DurableState, Effect, GameplayCommand, Origin,
    PasswordSlot, Principal, Request, Role, Secret, TrustedConnection,
};
use bri_world::{OwnerId, authority::Actor};
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
    LoginRejected { attempts: u8 },
    BrickGroups(Vec<AdminBrickGroup>),
    BanList {
        rows: Vec<BanRecord>,
        now_unix_seconds: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminSnapshot {
    pub revision: u64,
    pub role: Role,
    pub local_host: bool,
    pub legacy_lan: bool,
    pub supported: BTreeSet<AdminCapability>,
    pub players: Vec<AdminPlayer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminReply {
    pub snapshot: AdminSnapshot,
    pub data: AdminData,
}

#[derive(Debug)]
pub struct AdminCall {
    pub reply: AdminReply,
    /// Network adapter must close these authenticated peer connections before
    /// returning success for a kick or failed-password disconnect.
    pub disconnects: Vec<OwnerId>,
}

#[derive(Default)]
pub(super) struct AdminRuntime {
    authority: Administration,
    owner_to_connection: BTreeMap<OwnerId, ConnectionId>,
    connection_to_owner: BTreeMap<ConnectionId, OwnerId>,
    next_connection: u64,
    revision: u64,
    passwords: BTreeMap<PasswordSlot, Secret>,
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
        Ok(AdminSnapshot {
            revision: self.revision,
            role,
            local_host: self.authority.host_authority(Origin::Connection(id))?,
            legacy_lan: false,
            supported,
            players,
        })
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
                | Action::SetAdminPassword { .. }
                | Action::HostSetRole { .. }
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
        let mut changed = false;
        for effect in effects {
            match effect {
                Effect::Disconnect { target, .. } => {
                    if let Some(owner) = self.connection_to_owner.get(&target).copied() {
                        disconnects.push(owner);
                    }
                }
                Effect::RoleChanged { target, role } => {
                    let target_owner = *self
                        .connection_to_owner
                        .get(&target)
                        .context("Administration role target is no longer connected")?;
                    session
                        .peers
                        .get_mut(&target_owner)
                        .context("Administration target is not in the session")?
                        .actor
                        .administrator = role.is_admin();
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
                Effect::LoginRejected { attempts, .. } => {
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
                    let session_actor = Actor {
                        owner: actor_owner,
                        administrator: peer_actor.administrator,
                    };
                    match command {
                        GameplayCommand::RequestBrickGroups => {
                            data = AdminData::BrickGroups(brick_groups(session));
                        }
                        GameplayCommand::ClearAllBricks => {
                            let ids: Vec<_> =
                                session.simulation.state().bricks.keys().copied().collect();
                            preflight_removal(session, ids.len())?;
                            for brick in ids {
                                session.simulation.remove(&session_actor, brick)?;
                                session.dirty.insert(brick);
                            }
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
                            for brick in ids {
                                session.simulation.remove(&session_actor, brick)?;
                                session.dirty.insert(brick);
                            }
                            changed = true;
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
                Effect::Configure(_) => anyhow::bail!("Administration setting has no installed host adapter"),
            }
        }
        if changed {
            self.revision = self.revision.saturating_add(1);
        }
        let snapshot = self.snapshot(owner)?;
        Ok(AdminCall {
            reply: AdminReply { snapshot, data },
            disconnects,
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
    }

    pub fn restore_admin_state(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(self.peers.is_empty() && self.departed.is_empty(), "Admin state can only be restored before clients connect");
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
