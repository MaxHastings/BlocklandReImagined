//! Admin Change Map (`serverCmdChangeMap`): the host builds a fresh session
//! for the new map, which adopts the players, administration, trust and chat
//! of the old one. Everything the old mission held (bricks, vehicles, bots,
//! items, minigames and events) is cleaned up with it.
use super::*;

impl Session {
    /// Maps the Admin menu may change to (`serverCmdGetMapList`).
    pub fn set_map_list(&mut self, maps: Vec<MapListing>) -> Result<()> {
        ensure!(
            maps.len() <= 1024
                && maps.iter().all(|m| {
                    !m.id.is_empty()
                        && m.id.len() <= 128
                        && m.name.len() <= 128
                        && !m.name.chars().any(char::is_control)
                }),
            "Invalid map list"
        );
        self.admin.maps_available = !maps.is_empty();
        self.map_list = maps;
        Ok(())
    }
    pub fn spawn_points(&self) -> &[Vec3] {
        &self.spawn_points
    }
    pub(super) fn request_map_change(&mut self, admin: OwnerId, map: String) -> Result<()> {
        ensure!(
            self.map_list.iter().any(|m| m.id == map),
            "That map is not available on this server"
        );
        ensure!(self.map_change.is_none(), "The map is already changing");
        self.map_change = Some((admin, map));
        Ok(())
    }
    /// A requested map change for the host to load: (administrator, map id).
    pub fn take_map_change(&mut self) -> Option<(OwnerId, String)> {
        self.map_change.take()
    }
    /// Take over `old`'s players and server state. `self` is a freshly
    /// configured session for the new map with nobody connected yet.
    pub fn adopt(&mut self, mut old: Session, admin: OwnerId) -> Result<()> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty(),
            "The new map already has players"
        );
        let tick = old.simulation.state().tick;
        self.simulation.set_tick(tick);
        self.weapons.tick = tick;
        // Bots stay with the old mission.
        let bots: Vec<OwnerId> = old.peers.keys().copied().filter(|o| old.bots.is_bot(*o)).collect();
        for bot in bots {
            old.admin_disconnect(bot);
        }
        self.admin = std::mem::take(&mut old.admin);
        self.admin.maps_available = !self.map_list.is_empty();
        self.admin_disconnects = std::mem::take(&mut old.admin_disconnects);
        self.admin_disconnect_messages = std::mem::take(&mut old.admin_disconnect_messages);
        self.trust = std::mem::take(&mut old.trust);
        self.trust.forget_published();
        self.chat = std::mem::take(&mut old.chat);
        self.next_chat = old.next_chat;
        self.cues = std::mem::take(&mut old.cues);
        self.private_notices = std::mem::take(&mut old.private_notices);
        self.notices = std::mem::take(&mut old.notices);
        self.lan_host = old.lan_host;
        self.time_scale = old.time_scale;
        // Players keep their owner numbers across maps, and so does the
        // record of who each number is.
        for (owner, record) in &old.simulation.state().owners {
            if self.simulation.state().owners.get(owner) != Some(record) {
                self.simulation.claim_owner(*owner, record.clone()).ok();
            }
        }
        self.next_owner = self.next_owner.max(old.next_owner);
        self.departed = std::mem::take(&mut old.departed);
        // Their steering prefs came once, when they joined.
        self.vehicles.steering = std::mem::take(&mut old.vehicles.steering);
        let players: Vec<(OwnerId, Peer)> = std::mem::take(&mut old.peers)
            .into_iter()
            .filter(|(owner, _)| !old.bots.is_bot(*owner))
            .collect();
        let mut spawn = 0;
        for (owner, peer) in players {
            let mut placed = None;
            for _ in 0..self.spawn_points.len().max(1) {
                let point = self.spawn_points.get(spawn).copied().unwrap_or(Vec3::ZERO);
                spawn = (spawn + 1) % self.spawn_points.len().max(1);
                if let Ok(player) = Player::spawn(
                    &mut self.simulation.physics,
                    owner,
                    point,
                    PlayerTuning::default(),
                ) {
                    placed = Some(player);
                    break;
                }
            }
            // Every point taken (by bricks or the players placed before):
            // place the body anyway, as a join does.
            let arrived = (|| {
                let player = match placed {
                    Some(player) => player,
                    None => Player::spawn_overlapping(
                        &mut self.simulation.physics,
                        owner,
                        self.spawn_points.first().copied().unwrap_or(Vec3::ZERO),
                        PlayerTuning::default(),
                    )?,
                };
                self.spawn_inventory(owner)?;
                let combat = self.combat_connect(owner, &peer.name, peer.actor.administrator)?;
                anyhow::Ok((player, combat))
            })();
            // One player the new map cannot take is let go with the reason;
            // the map change goes ahead for everyone else.
            let (player, combat) = match arrived {
                Ok(arrived) => arrived,
                Err(error) => {
                    eprintln!(
                        "Map change could not place {} ({owner}): {error:#}",
                        peer.name
                    );
                    self.admin.disconnect(owner);
                    self.admin_disconnects.push_back(owner);
                    self.admin_disconnect_messages
                        .insert(owner, format!("The new map could not place you: {error:#}"));
                    continue;
                }
            };
            let tick = self.simulation.state().tick;
            self.peers.insert(
                owner,
                Peer {
                    player,
                    actor: Actor {
                        owner,
                        administrator: peer.actor.administrator,
                        ..Default::default()
                    },
                    combat,
                    special: Default::default(),
                    control: ControlObject::Player,
                    camera: None,
                    last_drop_tick: None,
                    tutorial: Default::default(),
                    input: MoveInput::default(),
                    inputs: VecDeque::new(),
                    seated_pace: SeatedPace::default(),
                    last_input_tick: tick,
                    window_tick: tick,
                    actions: 0,
                    chats: 0,
                    inspection: None,
                    talk_stops: VecDeque::new(),
                    ..peer
                },
            );
        }
        let name = self.peers.get(&admin).map_or_else(String::new, |p| p.name.clone());
        let map = self.simulation.state().name.clone();
        self.system_chat(format!("\u{E003}{name} \u{E000}changed the map to {map}"));
        self.refresh_trust();
        // A map load is a start: settings read only then take the host's.
        self.start_settings();
        Ok(())
    }
}

impl Session {
    /// The host could not load the requested map.
    pub fn map_change_failed(&mut self, admin: OwnerId, reason: &str) {
        self.notify(
            admin,
            Notice::MessageBox {
                title: "Change Map".into(),
                text: format!("The map could not be loaded.\n\n{reason}"),
            },
        );
    }
}
