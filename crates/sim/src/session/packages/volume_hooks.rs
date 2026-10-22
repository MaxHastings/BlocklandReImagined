//! `on_volume`: the rules hear a player walk into or out of a map volume
//! (`Scene::volumes`), a place's name, a capture zone, a hazard. The engine
//! compares who is inside each volume with the last tick, so a game without
//! the hook never pays for it and no volume has a meaning of its own.
use super::game_hooks::declaring;
use super::*;
use bri_package_runtime::rhai::Map;

/// Most changes told in one tick; the rest wait for the next, since what
/// the engine remembers only moves by what it told.
const MAX_A_TICK: usize = 256;

/// Which volumes (indexes into the map's list) each player's body centre
/// was in at the end of the last tick the hook looked.
#[derive(Default)]
pub(super) struct VolumeWatch {
    inside: BTreeMap<OwnerId, BTreeSet<usize>>,
}

impl Session {
    /// Tell `on_volume` hooks who entered or left a volume since the last
    /// tick. A dead or departed player leaves every volume they were in.
    pub(in crate::session) fn deliver_volume_changes(&mut self) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        let hooks = declaring(host, |b| b.on_volume);
        if hooks.is_empty() || self.map_volumes.is_empty() {
            return;
        }
        let mut now: BTreeMap<OwnerId, BTreeSet<usize>> = BTreeMap::new();
        for (&owner, peer) in &self.peers {
            if !peer.combat.alive {
                continue;
            }
            let state = peer.player.state();
            let height = crate::water::body_height(state, peer.player.tuning()) * state.scale;
            let centre = [state.feet[0], state.feet[1] + height * 0.5, state.feet[2]];
            let held: BTreeSet<usize> = self
                .map_volumes
                .iter()
                .enumerate()
                .filter(|(_, v)| v.contains(centre))
                .map(|(i, _)| i)
                .collect();
            if !held.is_empty() {
                now.insert(owner, held);
            }
        }
        let host = self.packages.as_mut().expect("checked");
        let before = &mut host.volume_watch.inside;
        let mut events: Vec<(&'static str, OwnerId, usize)> = Vec::new();
        for (&owner, held) in &now {
            let was = before.get(&owner);
            for &i in held {
                if !was.is_some_and(|w| w.contains(&i)) {
                    events.push(("enter", owner, i));
                }
            }
        }
        for (&owner, was) in before.iter() {
            for &i in was {
                if !now.get(&owner).is_some_and(|n| n.contains(&i)) {
                    events.push(("leave", owner, i));
                }
            }
        }
        // Only what is told is remembered: the rest is told next tick.
        events.truncate(MAX_A_TICK);
        for &(kind, owner, i) in &events {
            let set = before.entry(owner).or_default();
            if kind == "enter" {
                set.insert(i);
            } else {
                set.remove(&i);
            }
        }
        before.retain(|_, set| !set.is_empty());
        for (kind, owner, i) in events {
            let Some(volume) = self.map_volumes.get(i) else {
                continue;
            };
            let mut properties = Map::new();
            for (k, v) in &volume.properties {
                properties.insert(k.as_str().into(), v.clone().into());
            }
            let mut map = Map::new();
            map.insert("kind".into(), kind.into());
            map.insert("player".into(), Dynamic::from_int(owner as i64));
            map.insert("id".into(), volume.id.clone().into());
            map.insert(
                "priority".into(),
                Dynamic::from_int(i64::from(volume.priority)),
            );
            map.insert("properties".into(), properties.into());
            for package in &hooks {
                let _ = self.run_package(
                    package,
                    "on_volume",
                    vec![Dynamic::from_map(map.clone())],
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
