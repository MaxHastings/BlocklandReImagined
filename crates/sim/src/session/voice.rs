//! Voice chat session state: whether the host runs it at all (its Add-On
//! package), and whose voice an admin has muted.

use super::Session;
use bri_world::OwnerId;
use std::collections::BTreeSet;

impl Session {
    /// Whether an enabled, authorized package supplies voice policy.
    pub fn voice_chat(&self) -> bool {
        self.packages.as_ref().is_some_and(|host| {
            host.catalog.behaviours().any(|(id, b)| {
                b.on_voice
                    && host.catalog.packages[id]
                        .manifest
                        .capabilities
                        .iter()
                        .any(|c| c == "voice")
            })
        })
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

/// Per-listener playback selected by authorized server packages.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoicePlayback {
    pub spatial: bool,
    pub range: f32,
    pub label: String,
}
impl VoicePlayback {
    pub fn valid(&self) -> bool {
        self.range.is_finite()
            && (1.0..=2000.0).contains(&self.range)
            && self.label.len() <= 48
            && !self.label.chars().any(char::is_control)
    }
}
/// One route returned by `voice_routes` (higher priority replaces lower).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceRoute {
    #[serde(default = "route_allowed")]
    pub hear: bool,
    pub listener: u64,
    pub spatial: bool,
    pub range: f32,
    pub label: String,
    pub priority: u8,
}

impl Session {
    pub fn voice_context(&self, owner: OwnerId) -> (Option<u64>, bool) {
        (
            self.game_of(owner).map(|g| g.0),
            self.peers.get(&owner).is_some_and(|p| p.combat.alive),
        )
    }
}

fn route_allowed() -> bool {
    true
}
