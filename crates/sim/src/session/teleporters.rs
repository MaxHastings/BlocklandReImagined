//! Map teleporters (`Scene::teleporters`): a living player who steps within
//! an entrance's radius comes out at an exit of the entrance's channel,
//! facing the exit's way and still moving as they were relative to where
//! they faced. Several exits of one channel take turns. A player standing
//! in an entrance goes through once; they step out of it before it takes
//! them again.
use super::*;
use bri_content::scene::{MAX_TELEPORTERS, Teleporter};
use glam::Quat;

#[derive(Default)]
pub(super) struct MapTeleporters {
    teleporters: Vec<Teleporter>,
    /// The entrance each player stood in last tick, if any.
    inside: BTreeMap<OwnerId, usize>,
    /// How many trips each channel has sent, to take its exits in turn.
    trips: BTreeMap<String, usize>,
    /// The sound each traveller hears (`set_teleport_sound`).
    sound: Option<String>,
}
impl MapTeleporters {
    pub(super) fn set_sound(&mut self, sound: Option<String>) {
        self.sound = sound;
    }
}

/// The entrance `feet` stands in: within its radius across, and from its
/// radius below it to its radius plus `height` above it.
fn entrance_at(teleporters: &[Teleporter], feet: Vec3, height: f32) -> Option<usize> {
    teleporters.iter().position(|t| {
        let at = Vec3::from(t.position);
        let across = (feet - at).with_y(0.0).length();
        let up = feet.y - at.y;
        t.entrance && across <= t.radius && up >= -t.radius && up <= t.radius + height
    })
}

/// The `turn`th exit of `channel`, cycling.
fn exit_of(teleporters: &[Teleporter], channel: &str, turn: usize) -> Option<usize> {
    let exits: Vec<usize> = (0..teleporters.len())
        .filter(|&i| !teleporters[i].entrance && teleporters[i].channel == channel)
        .collect();
    (!exits.is_empty()).then(|| exits[turn % exits.len()])
}

impl Session {
    /// Install the map's teleporters (from `NativeMap`).
    pub fn set_teleporters(&mut self, teleporters: Vec<Teleporter>) -> Result<()> {
        ensure!(
            teleporters.len() <= MAX_TELEPORTERS,
            "Too many map teleporters"
        );
        ensure!(
            teleporters
                .iter()
                .all(|t| Vec3::from(t.position).is_finite()
                    && t.yaw.is_finite()
                    && t.radius.is_finite()
                    && t.radius > 0.0),
            "Invalid map teleporter"
        );
        self.map_teleporters = MapTeleporters {
            teleporters,
            ..Default::default()
        };
        Ok(())
    }
    /// Send every living player who stepped into an entrance this tick to
    /// its exit.
    pub(super) fn map_teleports(&mut self) -> Result<()> {
        if self.map_teleporters.teleporters.is_empty() {
            return Ok(());
        }
        let owners: Vec<OwnerId> = self.peers.keys().copied().collect();
        for owner in owners {
            let peer = &self.peers[&owner];
            let state = peer.player.state();
            let standing = if peer.combat.alive {
                entrance_at(
                    &self.map_teleporters.teleporters,
                    Vec3::from(state.feet),
                    peer.player.tuning().stand_height,
                )
            } else {
                None
            };
            let was = match standing {
                Some(entrance) => self.map_teleporters.inside.insert(owner, entrance),
                None => self.map_teleporters.inside.remove(&owner),
            };
            let Some(entrance) = standing.filter(|e| was != Some(*e)) else {
                continue;
            };
            let channel = self.map_teleporters.teleporters[entrance].channel.clone();
            let turn = self
                .map_teleporters
                .trips
                .entry(channel.clone())
                .or_default();
            let Some(exit) = exit_of(&self.map_teleporters.teleporters, &channel, *turn) else {
                continue;
            };
            *turn += 1;
            let exit = self.map_teleporters.teleporters[exit].clone();
            // Keep the motion relative to the facing: turn it with the player.
            let turned = exit.yaw - state.yaw;
            let velocity = Vec3::from(state.velocity);
            let new_velocity = Quat::from_rotation_y(-turned) * velocity;
            let peer = self.peers.get_mut(&owner).unwrap();
            peer.player.teleport(
                &mut self.simulation.physics,
                Vec3::from(exit.position),
                exit.yaw,
            )?;
            peer.player.push(new_velocity);
            peer.discard_inputs();
            if let Some(sound) = self.map_teleporters.sound.clone() {
                self.notify(owner, super::Notice::Sound(sound));
            }
            let peer = self.peers.get_mut(&owner).unwrap();
            // Landing in another entrance takes them through only once
            // they step out of it.
            match entrance_at(
                &self.map_teleporters.teleporters,
                Vec3::from(exit.position),
                peer.player.tuning().stand_height,
            ) {
                Some(next) => self.map_teleporters.inside.insert(owner, next),
                None => self.map_teleporters.inside.remove(&owner),
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn teleporter(entrance: bool, channel: &str, position: [f32; 3]) -> Teleporter {
        Teleporter {
            node: 0,
            entrance,
            channel: channel.into(),
            position,
            yaw: 0.0,
            radius: 1.0,
        }
    }

    #[test]
    fn a_player_is_in_an_entrance_only_inside_its_radius_and_height() {
        let all = [
            teleporter(false, "a", [0.0, 0.0, 0.0]),
            teleporter(true, "a", [10.0, 0.0, 0.0]),
        ];
        // Exits never take a player.
        assert_eq!(entrance_at(&all, Vec3::ZERO, 2.5), None);
        assert_eq!(entrance_at(&all, Vec3::new(10.5, 0.2, 0.5), 2.5), Some(1));
        assert_eq!(entrance_at(&all, Vec3::new(11.2, 0.0, 0.0), 2.5), None);
        assert_eq!(entrance_at(&all, Vec3::new(10.0, 3.4, 0.0), 2.5), Some(1));
        assert_eq!(entrance_at(&all, Vec3::new(10.0, 3.6, 0.0), 2.5), None);
        assert_eq!(entrance_at(&all, Vec3::new(10.0, -1.1, 0.0), 2.5), None);
    }

    #[test]
    fn a_channels_exits_take_turns_and_other_channels_are_ignored() {
        let all = [
            teleporter(true, "a", [0.0; 3]),
            teleporter(false, "b", [1.0; 3]),
            teleporter(false, "a", [2.0; 3]),
            teleporter(false, "a", [3.0; 3]),
        ];
        assert_eq!(exit_of(&all, "a", 0), Some(2));
        assert_eq!(exit_of(&all, "a", 1), Some(3));
        assert_eq!(exit_of(&all, "a", 2), Some(2));
        assert_eq!(exit_of(&all, "c", 0), None);
    }
}
