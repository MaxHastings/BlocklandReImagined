//! `on_climb_end`: the rules hear that a climb they started with
//! `start_climb` is over and the body is on the ledge. The motor ends the
//! climb (the feet clear the ledge, or its seconds run out); the engine
//! only notices, so a game without the hook never pays for it.
use super::game_hooks::declaring;
use super::*;

impl Session {
    /// Tell `on_climb_end` hooks which players finished a climb since the
    /// last tick. A player who left the game is told to nobody.
    pub(in crate::session) fn deliver_climb_ends(&mut self) {
        let ended: Vec<OwnerId> = self
            .peers
            .iter()
            .filter(|(_, p)| p.climbing_ledge && p.player.state().posture.mantle.is_none())
            .map(|(&owner, _)| owner)
            .collect();
        for owner in &ended {
            if let Some(peer) = self.peers.get_mut(owner) {
                peer.climbing_ledge = false;
            }
        }
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        let hooks = declaring(host, |b| b.on_climb_end);
        for owner in ended {
            for package in &hooks {
                let _ = self.run_package(
                    package,
                    "on_climb_end",
                    vec![Dynamic::from_int(owner as i64)],
                    Budget::Command,
                    None,
                    None,
                    None,
                );
                self.charge_work(package);
            }
        }
    }
}
