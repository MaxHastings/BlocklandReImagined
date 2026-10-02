//! Administrator world commands from the Admin menu and chat
//! (`ServerCmdHilightBrickGroup`, `RealBrickCount`, `CancelAllEvents`,
//! `ClearBots`).
use super::*;

impl Session {
    /// `/brickCount` (anyone) and `/realBrickCount` (admins): the server's
    /// bricks, told to whoever asked.
    pub(super) fn brick_count(&mut self, asker: OwnerId) {
        let count = self.simulation.state().bricks.len();
        let text = if count == 1 {
            "1 brick".to_string()
        } else {
            format!("{count} bricks")
        };
        self.notify(asker, Notice::Chat(text));
    }
    /// `/tripOut` (`serverCmdTripOut`, allGameScripts.cs:4733): an
    /// administrator's joke that sets every brick to the Rainbow colour
    /// effect and the Undulo shape effect. Others are ignored, and nothing
    /// is said, as in v20.
    pub(super) fn trip_out(&mut self, owner: OwnerId) -> Result<()> {
        if !self.is_administrator(owner) {
            return Ok(());
        }
        let ids: Vec<_> = self.simulation.state().bricks.keys().copied().collect();
        self.simulation.mutate_many(&ids, |b| {
            b.color_effect = 6;
            b.shape_effect = 1;
        })?;
        self.dirty.extend(ids);
        Ok(())
    }
    /// `/clearBricks` (`ServerCmdClearBricks`): a player deletes all of
    /// their own bricks, indestructible ones too, at most once every five
    /// seconds. Nothing happens when they have none.
    pub(super) fn clear_own_bricks(&mut self, owner: OwnerId) -> Result<()> {
        let tick = self.simulation.state().tick;
        let name = self
            .peers
            .get(&owner)
            .context("Unknown connection")?
            .name
            .clone();
        if self
            .cleared_bricks_at
            .get(&owner)
            .is_some_and(|at| tick.saturating_sub(*at) < 5 * bri_world::TICKS_PER_SECOND)
        {
            return Ok(());
        }
        let ids: Vec<_> = self
            .simulation
            .state()
            .bricks
            .iter()
            .filter_map(|(&id, b)| (b.owner == owner).then_some(id))
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        self.cleared_bricks_at.insert(owner, tick);
        // The brick group is theirs, so the engine deletes all of it.
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        self.simulation.remove_many(&engine, &ids)?;
        for id in &ids {
            self.events.respawns.remove(id);
        }
        self.dirty.extend(ids);
        self.system_message(
            Some(MessageTag::ClearBricks),
            format!("\u{E003}{name}\u{E002} cleared \u{E003}{name}\u{E002}'s bricks"),
        );
        Ok(())
    }
    /// `/cancelEvents` (`serverCmdCancelEvents`, allGameScripts.cs:4958):
    /// a player cancels their bricks' pending events and removes what their
    /// events spawned, at most once every five seconds. Not while in someone
    /// else's minigame, and on LAN servers only administrators.
    pub(super) fn cancel_own_events(&mut self, owner: OwnerId) -> Result<()> {
        let tick = self.simulation.state().tick;
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        let administrator = peer.actor.administrator;
        let player = peer.combat.player;
        if let Some(game) = self.game_of(owner)
            && self.minigames.game(game).is_ok_and(|g| g.owner != player)
        {
            self.notify(
                owner,
                Notice::Chat("CancelEvents is not allowed while in a minigame.".into()),
            );
            return Ok(());
        }
        if self.lan_host && !administrator {
            return Ok(());
        }
        let wait = 5 * bri_world::TICKS_PER_SECOND;
        if let Some(elapsed) = self
            .cancelled_events_at
            .get(&owner)
            .map(|at| tick.saturating_sub(*at))
            .filter(|elapsed| *elapsed < wait)
        {
            let seconds = (wait - elapsed).div_ceil(bri_world::TICKS_PER_SECOND);
            self.notify(
                owner,
                Notice::Chat(format!("You must wait {seconds} seconds.")),
            );
            return Ok(());
        }
        self.cancelled_events_at.insert(owner, tick);
        self.notify(
            owner,
            Notice::Chat("Deleting all events and event-spawned objects...".into()),
        );
        self.cancel_owner_events(owner);
        self.reset_owned_vehicles(owner);
        self.clear_event_projectiles(owner);
        Ok(())
    }
    /// `/cancelAllEvents`: drop every scheduled event row.
    pub(super) fn admin_cancel_all_events(&mut self, admin: OwnerId) {
        let name = self
            .peers
            .get(&admin)
            .map_or_else(String::new, |p| p.name.clone());
        self.system_chat(format!("\u{E003}{name}\u{E000} canceled all events."));
        self.cancel_all_events();
    }
    /// `/clearBots`: remove every bot, and every player-type mount (horse,
    /// boat, cannon, turret) no rider controls, as v20's `ServerCmdClearBots`
    /// deletes every player object no client controls. A spawn brick keeps
    /// its setting and brings its bot or mount back when respawned.
    pub(super) fn admin_clear_bots(&mut self, admin: OwnerId) -> Result<()> {
        let bricks = self.bot_bricks();
        for brick in &bricks {
            self.reconcile_bot_brick(*brick, None)?;
        }
        let mounts = self.uncontrolled_mounts();
        for mount in &mounts {
            self.clear_mount(*mount)?;
        }
        let name = self
            .peers
            .get(&admin)
            .map_or_else(String::new, |p| p.name.clone());
        self.system_chat(format!(
            "\u{E003}{name}\u{E000} cleared all bots ({}).",
            bricks.len() + mounts.len()
        ));
        Ok(())
    }
}
