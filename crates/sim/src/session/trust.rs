//! v20 trust lists: invitations, demotion, uploaded trust lists and the
//! `getTrustLevel` view each player's actor carries (`serverCmdTrust_Invite`,
//! `serverCmdAcceptTrustInvite`, `serverCmdTrust_Demote`,
//! `serverCmdTrustListUpload_*`, `SetMutualBrickGroupTrust`).
//!
//! A player's identity is their verified principal (v20's BL_ID). Trust is
//! mutual: both sides always hold the same level. LAN hosts trust everyone.
use super::*;
use bri_admin::Principal;
use bri_world::authority::{Trust, trust};

/// `StartInvitationTimeout`: one minute to answer.
const INVITE_TICKS: u64 = 60 * 120;
/// `serverCmdTrustListUpload_Line` cap.
pub const MAX_TRUST_LIST: usize = 1024;
/// Rejections within a minute that count as invite spam.
const REJECTION_WINDOW_TICKS: u64 = 60 * 120;

/// One saved trust list entry (`prefs-trustList.txt`: BL_ID and level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustEntry {
    pub principal: [u8; 32],
    pub level: u8,
}

/// How a player sees another in the player list (`secureClientCmd_ClientTrust`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerTrust {
    /// Stable identity for the viewer's saved trust list; `None` for bots and
    /// unverified connections.
    pub principal: Option<[u8; 32]>,
    pub level: TrustLevel,
    /// The viewer ignores this player's trust invites.
    pub ignoring: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    You,
    None,
    Build,
    Full,
    /// `$Server::LAN`: trust lists do not apply.
    Lan,
}

struct Invite {
    from: OwnerId,
    principal: Principal,
    level: u8,
    expires: u64,
}

#[derive(Default)]
pub(super) struct TrustBook {
    /// Mutual trust (`BrickGroup.Trust[]`); symmetric.
    levels: BTreeMap<Principal, BTreeMap<Principal, u8>>,
    /// Uploaded lists (`potentialTrust`).
    potential: BTreeMap<Principal, BTreeMap<Principal, u8>>,
    /// Pending invitation per invitee (`invitePendingBL_ID`).
    invites: BTreeMap<OwnerId, Invite>,
    /// `%client.Ignore[bl_id]`.
    ignoring: BTreeMap<OwnerId, BTreeSet<Principal>>,
    /// `lastTrustRejectionTime`, `trustRejectionCount` of inviters.
    rejections: BTreeMap<OwnerId, (u64, u32)>,
    /// Player list trust last sent to each viewer.
    published: BTreeMap<OwnerId, BTreeMap<OwnerId, PlayerTrust>>,
}

impl TrustBook {
    fn level(&self, a: Principal, b: Principal) -> u8 {
        self.levels
            .get(&a)
            .and_then(|m| m.get(&b))
            .copied()
            .unwrap_or(trust::NONE)
    }
    /// `SetMutualBrickGroupTrust`.
    fn set_mutual(&mut self, a: Principal, b: Principal, level: u8) {
        for (x, y) in [(a, b), (b, a)] {
            if level == trust::NONE {
                self.levels.entry(x).or_default().remove(&y);
            } else {
                self.levels.entry(x).or_default().insert(y, level);
            }
            self.potential.entry(x).or_default().insert(y, level);
        }
    }
}

impl Session {
    fn principal_of(&self, owner: OwnerId) -> Option<Principal> {
        self.peers
            .get(&owner)
            .map(|p| p.principal)
            .or_else(|| self.departed.get(&owner).map(|d| d.3))
            .flatten()
    }
    fn message_box(&mut self, owner: OwnerId, title: &str, text: String) {
        self.notify(
            owner,
            Notice::MessageBox {
                title: title.into(),
                text,
            },
        );
    }
    fn trust_peer(&self, owner: OwnerId) -> Result<(String, Principal)> {
        let peer = self.peers.get(&owner).context("Target client does not exist.")?;
        let principal = peer
            .principal
            .context("That player has no verified identity to trust.")?;
        Ok((peer.name.clone(), principal))
    }

    /// `serverCmdTrust_Invite`.
    pub(super) fn trust_invite(&mut self, owner: OwnerId, target: OwnerId, level: u8) -> Result<()> {
        const ERROR: &str = "Trust Invite Error";
        if self.lan_host {
            self.message_box(owner, ERROR, "Trust lists do not apply on a LAN.".into());
            return Ok(());
        }
        let (_, ours) = self.trust_peer(owner)?;
        let Ok((_, theirs)) = self.trust_peer(target) else {
            self.message_box(owner, ERROR, "Target client does not exist.".into());
            return Ok(());
        };
        if theirs == ours {
            self.message_box(owner, ERROR, "You already trust yourself.  I hope.".into());
            return Ok(());
        }
        if !matches!(level, trust::BUILD | trust::FULL) {
            self.message_box(owner, ERROR, "Invalid trust level specified.".into());
            return Ok(());
        }
        if self.trust.level(ours, theirs) >= level {
            self.message_box(
                owner,
                "Trust Invite",
                "You already trust this person at that level.".into(),
            );
            return Ok(());
        }
        if self
            .trust
            .ignoring
            .get(&target)
            .is_some_and(|i| i.contains(&ours))
        {
            self.message_box(owner, ERROR, "This person is ignoring your invites.".into());
            return Ok(());
        }
        let tick = self.simulation.state().tick;
        if let Some(pending) = self.trust.invites.get(&target).filter(|i| i.expires > tick) {
            let text = if pending.principal == ours {
                "This person hasn't responded to your first invite yet."
            } else {
                "This person is responding to another invite right now."
            };
            self.message_box(owner, ERROR, text.into());
            return Ok(());
        }
        self.trust.invites.insert(
            target,
            Invite {
                from: owner,
                principal: ours,
                level,
                expires: tick + INVITE_TICKS,
            },
        );
        let name = self.peers[&owner].name.clone();
        self.notify(
            target,
            Notice::TrustInvite {
                from: owner,
                name,
                principal: ours.0,
                level,
            },
        );
        Ok(())
    }

    fn take_invite(&mut self, owner: OwnerId, from: OwnerId) -> Option<Invite> {
        let tick = self.simulation.state().tick;
        let invite = self.trust.invites.remove(&owner)?;
        (invite.from == from && invite.expires > tick).then_some(invite)
    }

    /// `serverCmdAcceptTrustInvite`.
    pub(super) fn trust_accept(&mut self, owner: OwnerId, from: OwnerId) -> Result<()> {
        let Some(invite) = self.take_invite(owner, from) else {
            self.message_box(
                owner,
                "Trust Invitation Error",
                "That invitation is too old.".into(),
            );
            return Ok(());
        };
        let Ok((inviter, inviter_principal)) = self.trust_peer(invite.from) else {
            return Ok(());
        };
        if inviter_principal != invite.principal {
            return Ok(());
        }
        let (name, ours) = self.trust_peer(owner)?;
        self.trust.set_mutual(invite.principal, ours, invite.level);
        self.message_box(
            owner,
            "Trust Invitation Accepted",
            format!("You have accepted {inviter}'s trust invitation."),
        );
        self.message_box(
            invite.from,
            "Trust Invitation Accepted",
            format!("{name} has accepted your trust invitation."),
        );
        self.remember_trust(invite.from, ours, invite.level, &name);
        self.remember_trust(owner, invite.principal, invite.level, &inviter);
        self.refresh_trust();
        Ok(())
    }

    /// `serverCmdRejectTrustInvite`: three rejections within a minute kick a
    /// non-admin inviter for spam.
    pub(super) fn trust_reject(&mut self, owner: OwnerId, from: OwnerId) -> Result<()> {
        let Some(invite) = self.take_invite(owner, from) else {
            return Ok(());
        };
        if !self.peers.contains_key(&invite.from) {
            return Ok(());
        }
        let tick = self.simulation.state().tick;
        let entry = self.trust.rejections.entry(invite.from).or_insert((0, 0));
        if entry.1 == 0 || tick.saturating_sub(entry.0) < REJECTION_WINDOW_TICKS {
            if !self.peers[&invite.from].actor.administrator {
                *entry = (tick, entry.1 + 1);
                if entry.1 >= 3 {
                    let name = self.peers[&invite.from].name.clone();
                    self.system_chat(format!(
                        "\u{E003}{name}\u{E002} was kicked for spamming trust invites."
                    ));
                    self.admin_disconnects.push_back(invite.from);
                    return Ok(());
                }
            }
        } else {
            *entry = (0, 0);
        }
        let name = self.peers[&owner].name.clone();
        self.message_box(
            invite.from,
            "Trust Invite Rejected",
            format!("{name} has rejected your trust invitation."),
        );
        Ok(())
    }

    /// `serverCmdIgnoreTrustInvite`.
    pub(super) fn trust_ignore(&mut self, owner: OwnerId, from: OwnerId) -> Result<()> {
        let Some(invite) = self.take_invite(owner, from) else {
            return Ok(());
        };
        self.trust
            .ignoring
            .entry(owner)
            .or_default()
            .insert(invite.principal);
        if self.peers.contains_key(&invite.from) {
            let name = self.peers[&owner].name.clone();
            self.message_box(
                invite.from,
                "Trust Invite Rejected + Ignored",
                format!(
                    "{name} has rejected your trust invitation and will ignore any future invites from you."
                ),
            );
        }
        self.refresh_trust();
        Ok(())
    }

    /// `serverCmdUnIgnore`.
    pub(super) fn trust_unignore(&mut self, owner: OwnerId, target: OwnerId) -> Result<()> {
        let Some(principal) = self.principal_of(target) else {
            return Ok(());
        };
        if self
            .trust
            .ignoring
            .get_mut(&owner)
            .is_some_and(|i| i.remove(&principal))
        {
            let name = self.brick_owner_name(target);
            self.message_box(
                owner,
                "Ignore Removed",
                format!("You are no longer ignoring {name}"),
            );
            self.refresh_trust();
        }
        Ok(())
    }

    /// `serverCmdTrust_Demote`.
    pub(super) fn trust_demote(&mut self, owner: OwnerId, target: OwnerId, level: u8) -> Result<()> {
        let (name, ours) = self.trust_peer(owner)?;
        let theirs = self
            .principal_of(target)
            .context("That player has no verified identity")?;
        let target_name = self.brick_owner_name(target);
        if self.trust.level(ours, theirs) <= level {
            self.message_box(
                owner,
                "Trust",
                format!("{target_name} is already at or below that trust level."),
            );
            return Ok(());
        }
        let text = match level {
            trust::NONE => format!("{name} has removed you from their trust list."),
            trust::BUILD => format!("{name} has demoted you to build trust."),
            _ => {
                self.message_box(
                    owner,
                    "Trust Demote Error",
                    "Invalid trust level specified.".into(),
                );
                return Ok(());
            }
        };
        self.trust.set_mutual(ours, theirs, level);
        self.remember_trust(owner, theirs, level, &target_name);
        if self.peers.contains_key(&target) {
            self.message_box(target, "Trust", text);
            self.remember_trust(target, ours, level, &name);
        }
        self.refresh_trust();
        Ok(())
    }

    /// `InitializeTrustListUpload` .. `serverCmdTrustListUpload_Done`: the
    /// client's saved list replaces its previous upload, and entries both
    /// sides listed become mutual trust.
    pub(super) fn trust_list(&mut self, owner: OwnerId, list: Vec<TrustEntry>) -> Result<()> {
        ensure!(list.len() <= MAX_TRUST_LIST, "Trust list upload limit reached.");
        if self.lan_host {
            return Ok(());
        }
        let (_, ours) = self.trust_peer(owner)?;
        // Forget what this identity trusted before (both directions).
        if let Some(old) = self.trust.potential.remove(&ours) {
            for other in old.keys() {
                if let Some(m) = self.trust.levels.get_mut(other) {
                    m.remove(&ours);
                }
            }
        }
        self.trust.levels.remove(&ours);
        let mut potential = BTreeMap::new();
        for entry in list {
            if matches!(entry.level, trust::BUILD | trust::FULL) && entry.principal != ours.0 {
                potential.insert(Principal(entry.principal), entry.level);
            }
        }
        for (&other, &level) in &potential {
            let theirs = self
                .trust
                .potential
                .get(&other)
                .and_then(|m| m.get(&ours))
                .copied()
                .unwrap_or(trust::NONE);
            if theirs >= level {
                self.trust.set_mutual(ours, other, level);
            }
        }
        // `set_mutual` rewrote entries it touched; keep the full upload.
        self.trust.potential.entry(ours).or_default().extend(potential);
        self.refresh_trust();
        Ok(())
    }

    /// Tell a client to update its saved trust list (`updateClientTrustList`).
    fn remember_trust(&mut self, owner: OwnerId, principal: Principal, level: u8, name: &str) {
        self.notify(
            owner,
            Notice::TrustSaved {
                principal: principal.0,
                level,
                name: name.into(),
            },
        );
    }

    fn brick_owner_name(&self, owner: OwnerId) -> String {
        self.peers
            .get(&owner)
            .map(|p| p.name.clone())
            .or_else(|| self.departed.get(&owner).map(|d| d.0.clone()))
            .unwrap_or_default()
    }

    /// The trust every actor carries (`getTrustLevel`), recomputed when
    /// trust, players or the host mode change.
    pub(super) fn actor_trust(&self, owner: OwnerId) -> Trust {
        if self.lan_host {
            return Trust::Everyone;
        }
        let Some(ours) = self.principal_of(owner) else {
            return Trust::OwnerOnly;
        };
        let owners = self.peers.keys().chain(self.departed.keys()).copied();
        let levels: BTreeMap<OwnerId, u8> = owners
            .filter(|o| *o != owner)
            .filter_map(|o| {
                let theirs = self.principal_of(o)?;
                let level = if theirs == ours {
                    trust::YOU
                } else {
                    self.trust.level(ours, theirs)
                };
                (level > trust::NONE).then_some((o, level))
            })
            .collect();
        Trust::Levels(std::sync::Arc::new(levels))
    }

    /// Recompute actor trust and send changed player-list trust to viewers.
    pub(super) fn refresh_trust(&mut self) {
        let tick = self.simulation.state().tick;
        self.trust.invites.retain(|_, i| i.expires > tick);
        let owners: Vec<OwnerId> = self.peers.keys().copied().collect();
        for owner in &owners {
            let trust = self.actor_trust(*owner);
            if let Some(peer) = self.peers.get_mut(owner) {
                peer.actor.trust = trust;
            }
        }
        for viewer in &owners {
            if self.bots.is_bot(*viewer) {
                continue;
            }
            let rows: BTreeMap<OwnerId, PlayerTrust> = owners
                .iter()
                .map(|other| (*other, self.player_trust(*viewer, *other)))
                .collect();
            if self.trust.published.get(viewer) != Some(&rows) {
                self.trust.published.insert(*viewer, rows.clone());
                self.notify(*viewer, Notice::PlayerTrust(rows));
            }
        }
        self.trust.published.retain(|o, _| self.peers.contains_key(o));
    }

    fn player_trust(&self, viewer: OwnerId, other: OwnerId) -> PlayerTrust {
        let principal = self.principal_of(other);
        let level = if viewer == other {
            TrustLevel::You
        } else if self.lan_host {
            TrustLevel::Lan
        } else {
            match self.peers.get(&viewer).map(|p| p.actor.trust_level(other)) {
                Some(trust::YOU) => TrustLevel::You,
                Some(trust::FULL) => TrustLevel::Full,
                Some(trust::BUILD) => TrustLevel::Build,
                _ => TrustLevel::None,
            }
        };
        PlayerTrust {
            principal: principal.map(|p| p.0),
            level,
            ignoring: principal.is_some_and(|p| {
                self.trust
                    .ignoring
                    .get(&viewer)
                    .is_some_and(|i| i.contains(&p))
            }),
        }
    }

    /// Players join and leave: drop their pending invitations.
    pub(super) fn trust_disconnect(&mut self, owner: OwnerId) {
        self.trust.invites.remove(&owner);
        self.trust.invites.retain(|_, i| i.from != owner);
        self.trust.ignoring.remove(&owner);
        self.trust.rejections.remove(&owner);
        self.refresh_trust();
    }
}
