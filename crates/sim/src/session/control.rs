//! Control objects (`GameConnection::setControlObject`): a connection's moves
//! drive exactly one thing at a time. The body walks, or drives its vehicle
//! seat while mounted; an admin camera leaves the body standing still.
use super::{MoveInput, Peer, Session};
use anyhow::{Context, Result, ensure};
use bri_world::OwnerId;
use serde::{Deserialize, Serialize};

/// What a connection's moves steer. Replicated in [`super::Vitals`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlObject {
    /// The player's own body, or the vehicle seat it occupies.
    #[default]
    Player,
    /// Admin free camera (`dropCameraAtPlayer`). It carries no world state,
    /// so each client flies its own camera; the server only parks the body.
    Camera,
    /// Admin `spy`: an orbit camera around another player.
    Spy(OwnerId),
    /// `Corpse` camera after death: orbits the player's own body until
    /// respawn, so the corpse takes no more input.
    Corpse,
}

impl Peer {
    /// The motor input for this tick: controls only while the body is in
    /// control. Otherwise it idles and keeps its own aim, so neither a corpse
    /// nor a camera operator turns the blockhead.
    pub(super) fn body_input(&self, input: MoveInput) -> MoveInput {
        if self.control == ControlObject::Player {
            input
        } else {
            let state = self.player.state();
            MoveInput {
                yaw: state.yaw,
                pitch: state.pitch,
                ..Default::default()
            }
        }
    }
}

impl Session {
    pub fn control(&self, owner: OwnerId) -> Option<ControlObject> {
        self.peers.get(&owner).map(|p| p.control)
    }
    /// Grant an admin camera; callers own the administrator check.
    pub(super) fn set_control(&mut self, owner: OwnerId, control: ControlObject) -> Result<()> {
        if let ControlObject::Spy(target) = control {
            ensure!(target != owner, "You cannot spy on yourself");
            ensure!(self.peers.contains_key(&target), "Unknown player");
        }
        self.peers
            .get_mut(&owner)
            .context("Unknown connection")?
            .control = control;
        Ok(())
    }
    /// `setControlObject(player)`, or back to the corpse camera while dead.
    pub(super) fn return_to_body(&mut self, owner: OwnerId) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        peer.control = if peer.combat.alive {
            ControlObject::Player
        } else {
            ControlObject::Corpse
        };
        Ok(())
    }
    /// Spies watching a departing player return to their own bodies.
    pub(super) fn release_spies(&mut self, target: OwnerId) {
        let spies: Vec<_> = self
            .peers
            .iter()
            .filter(|(_, p)| p.control == ControlObject::Spy(target))
            .map(|(owner, _)| *owner)
            .collect();
        for spy in spies {
            let _ = self.return_to_body(spy);
        }
    }
}
