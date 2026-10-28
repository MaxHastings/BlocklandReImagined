//! Voice chat session state: whether the host runs it at all (its Add-On
//! package), and whose voice an admin has muted.

use super::Session;
use bri_world::OwnerId;
use std::collections::BTreeSet;

/// The id of the voice chat's Add-On package. A host runs it when the player
/// has it turned on in Add-Ons, before any player joins; off, the host relays
/// no voices and clients hide the mute controls.
pub const VOICE_PACKAGE_ID: &str = "voice_chat";

impl Session {
    /// Whether this session runs voice chat at all (see [`VOICE_PACKAGE_ID`]).
    pub fn voice_chat(&self) -> bool {
        self.packages
            .as_ref()
            .is_some_and(|host| host.catalog.packages.contains_key(VOICE_PACKAGE_ID))
    }
    /// An admin muted `owner`'s voice: the host relays none of it, and no
    /// speech state of theirs reaches anyone.
    pub fn mute_voice(&mut self, owner: OwnerId) -> anyhow::Result<()> {
        let peer = self
            .peers
            .get(&owner)
            .ok_or_else(|| anyhow::anyhow!("Unknown voice owner"))?;
        if let Some(principal) = peer.principal {
            anyhow::ensure!(
                self.voice_muted_principals.contains(&principal)
                    || self.voice_muted_principals.len() < bri_admin::MAX_BANS,
                "Too many muted identities in this session"
            );
            self.voice_muted_principals.insert(principal);
            self.voice_muted.extend(
                self.peers
                    .iter()
                    .filter(|(_, peer)| peer.principal == Some(principal))
                    .map(|(owner, _)| *owner),
            );
        } else {
            self.voice_muted.insert(owner);
        }
        Ok(())
    }
    pub(super) fn restore_voice_mute(
        &mut self,
        owner: OwnerId,
        principal: Option<bri_admin::Principal>,
    ) {
        if principal.is_some_and(|principal| self.voice_muted_principals.contains(&principal)) {
            self.voice_muted.insert(owner);
        }
    }
    /// Unmute this verified identity, including its other active connections.
    pub fn unmute_voice(&mut self, owner: OwnerId) {
        self.voice_muted.remove(&owner);
        if let Some(principal) = self.peers.get(&owner).and_then(|peer| peer.principal) {
            self.voice_muted_principals.remove(&principal);
            self.voice_muted.retain(|owner| {
                self.peers
                    .get(owner)
                    .is_none_or(|peer| peer.principal != Some(principal))
            });
        }
    }
    /// Whether `owner`'s voice is muted by an admin right now.
    pub fn is_voice_muted(&self, owner: OwnerId) -> bool {
        self.voice_muted.contains(&owner)
    }
    /// The players an admin has muted right now, by owner id; the host
    /// broadcasts this set so every client can show it in the player list.
    pub fn voice_mute_set(&self) -> &BTreeSet<OwnerId> {
        &self.voice_muted
    }
}
