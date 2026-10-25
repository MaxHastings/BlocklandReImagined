//! Becoming another body while playing: a brick's ChangeDatablock, an
//! Add-On's `set_archetype`, `/body` and a mini-game's rules all choose a
//! body here, and every one of them changes the player through the one
//! transition, [`Session::set_player_archetype`]. Nothing here names a
//! body: what a body may do (ride, use items, fit) is its archetype's data.
use super::*;
use crate::archetype::ArchetypeId;
use bri_weapons::ActorId;

/// Fewest ticks between two `/body` switches of one player.
const BODY_COMMAND_TICKS: u64 = 60;

/// A body someone chose for a player, kept across respawns
/// ([`Session::kept_body`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ChosenBody {
    pub(super) archetype: ArchetypeId,
    pub(super) by: Chooser,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Chooser {
    /// An Add-On's rule (`set_archetype`): kept everywhere, outranking the
    /// mini-game's player type.
    AddOn,
    /// The player, through a brick event or `/body`: kept outside
    /// mini-games and into any game that keeps chosen bodies (the default).
    Player,
}
/// Whether a body change must find room where the player stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Room {
    /// A bigger body that would reach into a brick or the map is refused,
    /// so a click or a command never wedges anyone into a ceiling.
    Required,
    /// An Add-On's rule decides; the motor resolves any overlap by moving.
    Forced,
}
/// What a body choice came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Chosen {
    /// The player is the new body now.
    Became,
    /// Not alive: the choice waits for the next spawn.
    Later,
    /// Not enough room here for the bigger body; nothing changed.
    NoRoom,
}

impl Session {
    /// Lay `laid` over `owner`'s body (`pushDatablock`): they move as it
    /// until it is lifted. A body of another shape, a dead one, or one
    /// already under it takes nothing.
    pub(super) fn lay_archetype(
        &mut self,
        owner: OwnerId,
        laid: crate::archetype::ArchetypeId,
    ) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("No such player")?;
        if !peer.combat.alive {
            return Ok(());
        }
        let current = peer.player.state().archetype;
        // `pushDatablock` takes only a datablock of the same shape.
        if self.archetypes.resolve(current).look.model != self.archetypes.resolve(laid).look.model {
            return Ok(());
        }
        let overlays = peer.overlays.get_or_insert_with(|| super::Overlays {
            base: current,
            laid: Vec::new(),
        });
        if overlays.base == laid || overlays.laid.contains(&laid) {
            return Ok(());
        }
        overlays.laid.push(laid);
        self.set_player_archetype(owner, laid)
    }
    /// Take `lifted` from under `owner`'s laid archetypes; the next one down
    /// (or their own body) is theirs again.
    pub(super) fn lift_archetype(
        &mut self,
        owner: OwnerId,
        lifted: crate::archetype::ArchetypeId,
    ) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("No such player")?;
        let Some(overlays) = peer.overlays.as_mut().filter(|_| peer.combat.alive) else {
            return Ok(());
        };
        let Some(at) = overlays.laid.iter().position(|a| *a == lifted) else {
            return Ok(());
        };
        overlays.laid.remove(at);
        let top = overlays.laid.last().copied().unwrap_or(overlays.base);
        if overlays.laid.is_empty() {
            peer.overlays = None;
        }
        if peer.player.state().archetype != top {
            self.set_player_archetype(owner, top)?;
        }
        Ok(())
    }
    /// Keep the archetype laid over `owner` for the image in their right
    /// hand ([`bri_weapons::Image::stance`]): lay the one it names for their
    /// body, lift the one the last image laid. Called as images mount and
    /// unmount, so a switch of tool never leaves a stance behind.
    pub(super) fn sync_stance(&mut self, owner: OwnerId) -> Result<()> {
        let Some(peer) = self.peers.get(&owner) else {
            return Ok(());
        };
        let base = peer
            .overlays
            .as_ref()
            .map_or(peer.player.state().archetype, |o| o.base);
        let body = self.archetypes.resolve(base).id.clone();
        let want = self
            .weapons
            .image_state(ActorId(owner), 0)
            .and_then(|(image, state)| state.stance.get(&body).or_else(|| image.stance.get(&body)))
            .and_then(|id| self.archetypes.find(id));
        let laid = peer.stance;
        if want == laid {
            return Ok(());
        }
        if let Some(old) = laid {
            self.lift_archetype(owner, old)?;
        }
        if let Some(new) = want {
            self.lay_archetype(owner, new)?;
        }
        self.peers.get_mut(&owner).context("No such player")?.stance = want;
        Ok(())
    }

    /// Choose `archetype` for `owner` and, when alive, make them it. The
    /// choice is kept for later spawns as [`Self::kept_body`] says. Under
    /// bodies an Add-On laid on top (`push_archetype`) it changes the one
    /// beneath.
    pub(super) fn choose_body(
        &mut self,
        owner: OwnerId,
        archetype: ArchetypeId,
        by: Chooser,
        room: Room,
    ) -> Result<Chosen> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        let alive = peer.combat.alive;
        if alive && room == Room::Required && peer.overlays.is_none() && !self.is_seated(owner) {
            let tuning = self.archetypes.tuning(archetype, peer.player.state().scale);
            if !peer.player.room_for(&self.simulation.physics, &tuning) {
                return Ok(Chosen::NoRoom);
            }
        }
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        peer.chosen_body = Some(ChosenBody { archetype, by });
        if !alive {
            return Ok(Chosen::Later);
        }
        match &mut peer.overlays {
            Some(overlays) => overlays.base = archetype,
            None => self.set_player_archetype(owner, archetype)?,
        }
        Ok(Chosen::Became)
    }
    /// Forget an Add-On's choice of body: the mini-game's player type
    /// decides at the next spawn.
    pub(super) fn forget_body(&mut self, owner: OwnerId) {
        if let Some(peer) = self.peers.get_mut(&owner) {
            peer.chosen_body = None;
        }
    }
    /// The body `owner` spawns as, if a choice outranks the mini-game's:
    /// an Add-On's always; the player's own outside mini-games, and inside
    /// the game they chose it in while that game keeps chosen bodies.
    pub(super) fn kept_body(&self, owner: OwnerId) -> Option<ArchetypeId> {
        let chosen = self.peers.get(&owner)?.chosen_body?;
        let kept = match chosen.by {
            Chooser::AddOn => true,
            Chooser::Player => self.keeps_chosen_body(owner),
        };
        kept.then_some(chosen.archetype)
    }
    /// Whether the player's mini-game (if any) keeps a body they chose.
    fn keeps_chosen_body(&self, owner: OwnerId) -> bool {
        self.game_of(owner).is_none_or(|g| {
            self.minigames
                .game(g)
                .is_ok_and(|g| g.settings.keep_chosen_body)
        })
    }
    /// A player's own choice ends in a game that sets its own bodies, so
    /// leaving that game does not bring it back.
    pub(super) fn drop_stale_body(&mut self, owner: OwnerId) {
        let keeps = self.keeps_chosen_body(owner);
        if let Some(peer) = self.peers.get_mut(&owner)
            && !keeps
            && matches!(
                peer.chosen_body,
                Some(ChosenBody {
                    by: Chooser::Player,
                    ..
                })
            )
        {
            peer.chosen_body = None;
        }
    }
    /// On a vehicle seat or another player's back.
    pub(super) fn is_seated(&self, owner: OwnerId) -> bool {
        self.mounted(owner).is_some() || self.riding.is_riding(owner)
    }
    /// The player's own choice of body (a brick event, `/body`), told why
    /// when refused.
    pub(super) fn player_chooses_body(
        &mut self,
        owner: OwnerId,
        archetype: ArchetypeId,
    ) -> Result<Chosen> {
        let chosen = self.choose_body(owner, archetype, Chooser::Player, Room::Required)?;
        if chosen == Chosen::NoRoom {
            let name = self.body_name(archetype);
            self.center_print(owner, format!("No room to become {name} here"));
        }
        Ok(chosen)
    }
    fn body_name(&self, archetype: ArchetypeId) -> String {
        let kind = self.archetypes.resolve(archetype);
        if kind.name.is_empty() {
            kind.id.clone()
        } else {
            kind.name.clone()
        }
    }
    /// The bodies `owner` may pick for themselves with `/body`: any
    /// choosable body outside a mini-game; inside one, its player type,
    /// their team's and the bodies it lists.
    pub(super) fn bodies_for(&self, owner: OwnerId) -> Vec<ArchetypeId> {
        let choosable = self.archetypes.choosable().map(|(id, _)| id);
        let Some(game) = self.game_of(owner) else {
            return choosable.collect();
        };
        let Ok(g) = self.minigames.game(game) else {
            return Vec::new();
        };
        let team = self
            .peers
            .get(&owner)
            .and_then(|p| self.minigames.player(p.combat.player).ok())
            .and_then(|p| p.team)
            .and_then(|t| g.teams.get(t))
            .and_then(|t| t.player_type.as_deref());
        let allowed: Vec<ArchetypeId> = std::iter::once(g.settings.player_type.as_str())
            .chain(team)
            .chain(g.settings.bodies.iter().map(String::as_str))
            .filter_map(|id| self.archetypes.find(id))
            .collect();
        choosable.filter(|id| allowed.contains(id)).collect()
    }
    /// The bodies the host's enabled Add-Ons have that this game leaves out
    /// (a game mode uses them, or they need another map), so `/body` says
    /// why rather than that no such body exists. Set after the Add-Ons are
    /// installed: a body this game has is never absent.
    pub fn set_absent_bodies(&mut self, bodies: Vec<AbsentBody>) {
        let archetypes = &self.archetypes;
        self.absent_bodies = bodies
            .into_iter()
            .filter(|b| archetypes.find(&b.id).is_none())
            .collect();
    }
    fn absent_body(&self, named: &str) -> Option<&AbsentBody> {
        let named = named.trim();
        self.absent_bodies
            .iter()
            .find(|b| b.id.eq_ignore_ascii_case(named) || b.name.eq_ignore_ascii_case(named))
    }
    /// `/body`: list the bodies one may pick; `/body <name>` becomes one.
    pub(super) fn body_command(&mut self, owner: OwnerId, named: &str) -> Result<()> {
        let bodies = self.bodies_for(owner);
        let names = || {
            bodies
                .iter()
                .map(|id| self.body_name(*id))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if named.trim().is_empty() {
            let mut text = if bodies.len() > 1 {
                format!("Bodies: {}. Type /body and a name.", names())
            } else {
                "This mini-game lets you pick no other body.".into()
            };
            let absent: Vec<&str> = self
                .absent_bodies
                .iter()
                .filter(|b| !b.name.is_empty())
                .map(|b| b.name.as_str())
                .collect();
            if !absent.is_empty() {
                text.push_str(&format!(
                    " Not in this game: {}; type /body and the name to see why.",
                    absent.join(", ")
                ));
            }
            self.private_chat(owner, text);
            return Ok(());
        }
        let Some(archetype) = self.archetypes.choice(named) else {
            let text = match self.absent_body(named) {
                Some(b) => format!("{} is not in this game. {}", b.label(), b.why),
                None => format!("No body is called {}.", named.trim()),
            };
            self.private_chat(owner, text);
            return Ok(());
        };
        if !bodies.contains(&archetype) {
            let name = self.body_name(archetype);
            self.private_chat(owner, format!("This mini-game does not let you be {name}."));
            return Ok(());
        }
        let tick = self.simulation.state().tick;
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        if peer
            .body_command_tick
            .is_some_and(|at| tick < at + BODY_COMMAND_TICKS)
        {
            self.private_chat(owner, "Wait a moment before changing body again.".into());
            return Ok(());
        }
        peer.body_command_tick = Some(tick);
        match self.player_chooses_body(owner, archetype)? {
            Chosen::Later if self.kept_body(owner) == Some(archetype) => {
                let name = self.body_name(archetype);
                self.private_chat(owner, format!("You will spawn as {name}."));
            }
            Chosen::Later => {
                self.private_chat(owner, "Pick a body once you have respawned.".into());
            }
            Chosen::Became | Chosen::NoRoom => {}
        }
        Ok(())
    }
}

/// A body an enabled Add-On of the host's has that this game left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsentBody {
    /// Its archetype id.
    pub id: String,
    /// Its shown name; empty for a body players cannot pick.
    pub name: String,
    /// Why this game left it out, in plain words.
    pub why: String,
}
impl AbsentBody {
    fn label(&self) -> &str {
        if self.name.is_empty() {
            &self.id
        } else {
            &self.name
        }
    }
}
