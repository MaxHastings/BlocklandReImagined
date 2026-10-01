//! Deterministic vanilla minigame rules. The host owns transport, objects, and physics.
mod model;
mod persistence;
mod policy;
mod teams;
pub use model::*;
pub use persistence::Preset;
pub use policy::*;
use std::collections::{BTreeMap, BTreeSet};

pub struct MinigamesWorld {
    pub(crate) catalog: Catalog,
    pub(crate) mode: PolicyMode,
    pub(crate) clear_events: bool,
    pub(crate) tick: u64,
    pub(crate) next_session: u64,
    pub(crate) next_game: u64,
    pub(crate) next_life: u64,
    pub(crate) players: BTreeMap<PlayerId, PlayerState>,
    pub(crate) games: BTreeMap<GameId, MiniGame>,
}
impl MinigamesWorld {
    pub fn new(
        catalog: Catalog,
        mode: PolicyMode,
        clear_events_on_change: bool,
    ) -> Result<Self, Error> {
        catalog.validate()?;
        Ok(Self {
            catalog,
            mode,
            clear_events: clear_events_on_change,
            tick: 0,
            next_session: 1,
            next_game: 1,
            next_life: 1,
            players: BTreeMap::new(),
            games: BTreeMap::new(),
        })
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub fn player(&self, id: PlayerId) -> Result<&PlayerState, Error> {
        self.players.get(&id).ok_or(Error::StalePlayer)
    }
    pub fn game(&self, id: GameId) -> Result<&MiniGame, Error> {
        self.games.get(&id).ok_or(Error::StaleGame)
    }
    pub fn games(&self) -> impl Iterator<Item = &MiniGame> {
        self.games.values()
    }
    pub fn players(&self) -> impl Iterator<Item = &PlayerState> {
        self.players.values()
    }
    pub fn free_colors(&self) -> Vec<u8> {
        (0..10)
            .filter(|c| !self.games.values().any(|g| g.color == *c))
            .collect()
    }
    /// Host registration only. Account identity must already be authenticated.
    pub fn connect(
        &mut self,
        account: AccountId,
        name: String,
        admin: bool,
    ) -> Result<PlayerId, Error> {
        if self.players.len() >= MAX_PLAYERS
            || self.next_session == u64::MAX
            || self.next_life == u64::MAX
        {
            return Err(Error::Capacity);
        }
        if name.is_empty()
            || name.chars().count() > 64
            || name.chars().any(char::is_control)
            || self.players.keys().any(|p| p.account == account)
        {
            return Err(Error::InvalidSettings);
        }
        let id = PlayerId {
            account,
            session: self.next_session,
        };
        self.next_session += 1;
        let life = self.alloc_life();
        self.players.insert(
            id,
            PlayerState {
                id,
                name,
                admin,
                ready: false,
                game: None,
                score: 0,
                life: LifeState::Alive { life },
                invite: None,
                ignored_owners: BTreeSet::new(),
                last_join: None,
                team: None,
            },
        );
        Ok(id)
    }
    pub fn set_ready(&mut self, player: PlayerId, ready: bool) -> Result<(), Error> {
        self.players
            .get_mut(&player)
            .ok_or(Error::StalePlayer)?
            .ready = ready;
        Ok(())
    }
    pub fn set_admin(&mut self, player: PlayerId, admin: bool) -> Result<(), Error> {
        self.players
            .get_mut(&player)
            .ok_or(Error::StalePlayer)?
            .admin = admin;
        Ok(())
    }
    pub fn rename(&mut self, player: PlayerId, name: String) -> Result<(), Error> {
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err(Error::InvalidSettings);
        }
        self.players
            .get_mut(&player)
            .ok_or(Error::StalePlayer)?
            .name = name;
        Ok(())
    }
    pub fn disconnect(&mut self, player: PlayerId) -> Result<Vec<Effect>, Error> {
        self.player(player)?;
        if self.next_life > u64::MAX - MAX_PLAYERS as u64 {
            return Err(Error::Capacity);
        }
        let mut out = Vec::new();
        self.remove_member(player, &mut out)?;
        self.players.remove(&player);
        Ok(out)
    }
    fn alloc_life(&mut self) -> LifeId {
        let id = LifeId(self.next_life);
        self.next_life += 1;
        id
    }
    fn ready(&self, p: PlayerId) -> Result<(), Error> {
        if self.player(p)?.ready {
            Ok(())
        } else {
            Err(Error::NotReady)
        }
    }
    fn owned(&self, p: PlayerId) -> Result<GameId, Error> {
        let g = self.player(p)?.game.ok_or(Error::NotMember)?;
        if self.game(g)?.owner == p {
            Ok(g)
        } else {
            Err(Error::NotOwner)
        }
    }
    fn authorize_event(&self, id: GameId, authority: EventAuthority) -> Result<(), Error> {
        let game = self.game(id)?;
        match authority {
            EventAuthority::System => Ok(()),
            EventAuthority::Owner(p) => {
                self.player(p)?;
                if p == game.owner {
                    Ok(())
                } else {
                    Err(Error::NotOwner)
                }
            }
            EventAuthority::OwnerBrick {
                instigator,
                brick_owner,
            } => {
                if self.player(instigator)?.game != Some(id) {
                    Err(Error::NotMember)
                } else if brick_owner != game.owner.account {
                    Err(Error::NotOwner)
                } else {
                    Ok(())
                }
            }
        }
    }
    fn cleanup(&self, player: PlayerId, reset: bool, out: &mut Vec<Effect>) {
        let active = reset || self.mode != PolicyMode::LegacyLan;
        if active {
            out.push(Effect::Cleanup {
                player,
                clear_event_schedules: self.clear_events,
                reset_owned_vehicles: true,
                clear_spawned_objects: true,
            });
        }
    }
    fn score(&mut self, player: PlayerId, value: i64, out: &mut Vec<Effect>) {
        self.players
            .get_mut(&player)
            .expect("validated player")
            .score = value;
        out.push(Effect::Score { player, value });
    }
    fn add_score(&mut self, p: PlayerId, delta: i32, out: &mut Vec<Effect>) {
        let value = self.players[&p].score.saturating_add(i64::from(delta));
        self.score(p, value, out);
    }
    fn spawn(&mut self, player: PlayerId, reason: SpawnReason, out: &mut Vec<Effect>) {
        let life = self.alloc_life();
        let equipment = self.players[&player]
            .game
            .map(|id| self.games[&id].settings.equipment(&self.catalog));
        self.players
            .get_mut(&player)
            .expect("validated player")
            .life = LifeState::Alive { life };
        out.push(Effect::Spawn {
            player,
            life,
            reason,
            equipment,
        });
    }
    fn join_member(
        &mut self,
        player: PlayerId,
        id: GameId,
        out: &mut Vec<Effect>,
    ) -> Result<(), Error> {
        self.remove_member(player, out)?;
        let game = self.games.get_mut(&id).ok_or(Error::StaleGame)?;
        game.members.insert(player);
        let eject = game.owner == player || game.settings.use_all_players_bricks;
        let color = game.color;
        let p = self.players.get_mut(&player).expect("validated player");
        p.game = Some(id);
        p.invite = None;
        out.push(Effect::Invitation { player, game: None });
        out.push(Effect::Membership {
            player,
            game: Some(id),
            color: Some(color),
        });
        self.score(player, 0, out);
        self.cleanup(player, false, out);
        self.spawn(player, SpawnReason::Join, out);
        if eject {
            out.push(Effect::EjectVehicles {
                brick_owner: player.account,
            });
        }
        Ok(())
    }
    fn remove_member(&mut self, player: PlayerId, out: &mut Vec<Effect>) -> Result<(), Error> {
        let Some(id) = self.player(player)?.game else {
            return Ok(());
        };
        if self.games[&id].owner == player {
            self.end_game(id, out);
            return Ok(());
        }
        self.games
            .get_mut(&id)
            .expect("validated game")
            .members
            .remove(&player);
        self.clear_team(player, id, out);
        self.players
            .get_mut(&player)
            .expect("validated player")
            .game = None;
        out.push(Effect::Membership {
            player,
            game: None,
            color: None,
        });
        self.score(player, 0, out);
        self.cleanup(player, false, out);
        self.spawn(player, SpawnReason::Leave, out);
        out.push(Effect::EjectVehicles {
            brick_owner: player.account,
        });
        Ok(())
    }
    fn end_game(&mut self, id: GameId, out: &mut Vec<Effect>) {
        let game = self.games.remove(&id).expect("validated game");
        for p in &game.members {
            let alive = matches!(self.players[p].life, LifeState::Alive { .. });
            self.clear_team(*p, id, out);
            self.players.get_mut(p).expect("validated player").game = None;
            out.push(Effect::Membership {
                player: *p,
                game: None,
                color: None,
            });
            self.score(*p, 0, out);
            self.cleanup(*p, false, out);
            if *p == game.owner && alive {
                // Changing the life token invalidates delayed damage/death acknowledgements.
                let life = self.alloc_life();
                self.players.get_mut(p).expect("validated player").life = LifeState::Alive { life };
                out.push(Effect::RestoreOwner { player: *p, life });
            } else {
                self.spawn(*p, SpawnReason::End, out);
            }
            out.push(Effect::EjectVehicles {
                brick_owner: p.account,
            });
        }
        for p in self.players.values_mut() {
            if p.invite == Some(id) {
                p.invite = None;
                out.push(Effect::Invitation {
                    player: p.id,
                    game: None,
                });
            }
        }
        out.push(Effect::Ended { game: id });
    }
    /// Process a command from an authenticated actor. Effects have no hidden queue.
    pub fn execute(&mut self, command: Command) -> Result<Vec<Effect>, Error> {
        // A single command can respawn every member twice (end old + join new).
        if self.next_life > u64::MAX - (MAX_PLAYERS as u64 * 3) {
            return Err(Error::Capacity);
        }
        let mut out = Vec::new();
        if let Command::Create { actor, .. }
        | Command::Join { actor, .. }
        | Command::Leave { actor }
        | Command::Accept { actor, .. } = command
            && self.server_game().is_some()
        {
            self.player(actor)?;
            return Err(Error::ServerGame);
        }
        match command {
            Command::Create {
                actor,
                color,
                settings,
            } => {
                self.ready(actor)?;
                settings.validate(&self.catalog)?;
                if self.owned(actor).is_ok() {
                    return Err(Error::AlreadyOwner);
                }
                if color >= 10 || !self.free_colors().contains(&color) {
                    return Err(Error::ColorUnavailable);
                }
                if self.games.len() >= MAX_GAMES || self.next_game == u64::MAX {
                    return Err(Error::Capacity);
                }
                self.remove_member(actor, &mut out)?;
                let id = GameId(self.next_game);
                self.next_game += 1;
                self.games.insert(
                    id,
                    MiniGame {
                        id,
                        owner: actor,
                        color,
                        settings,
                        members: BTreeSet::new(),
                                round: 1,
                        last_reset: None,
                        ball_update_at: None,
                        teams: Teams::default(),
                        addon_settings: BTreeMap::new(),
                    },
                );
                out.push(Effect::Created { game: id });
                self.join_member(actor, id, &mut out)?;
            }
            Command::Configure { actor, settings } => {
                let id = self.owned(actor)?;
                settings.validate(&self.catalog)?;
                let old = self.games[&id].settings.clone();
                let members: Vec<_> = self.games[&id].members.iter().copied().collect();
                let slots = std::array::from_fn(|i| old.loadout[i] != settings.loadout[i]);
                let equipment = settings.equipment(&self.catalog);
                let game = self.games.get_mut(&id).expect("validated game");
                game.settings = settings.clone();
                game.ball_update_at = Some(self.tick.saturating_add(6));
                for p in members {
                    if old.respawn_ms != settings.respawn_ms
                        && let LifeState::Dead { life, ready_at } = self.players[&p].life
                    {
                        let died_at = ready_at - manual_respawn_ticks(old.respawn_ms);
                        let ready_at =
                            died_at.saturating_add(manual_respawn_ticks(settings.respawn_ms));
                        self.players.get_mut(&p).expect("validated player").life =
                            LifeState::Dead { life, ready_at };
                        out.push(Effect::RespawnDeadline {
                            player: p,
                            life,
                            ready_at,
                        });
                    }
                    if old.use_spawn_bricks != settings.use_spawn_bricks {
                        self.spawn(p, SpawnReason::SpawnSettingChanged, &mut out);
                    } else if matches!(self.players[&p].life, LifeState::Alive { .. }) {
                        out.push(Effect::ApplyEquipment {
                            player: p,
                            equipment: equipment.clone(),
                            changed_slots: slots,
                            change_player_type: old.player_type != settings.player_type,
                            cancel_building: old.enable_building && !settings.enable_building,
                            unmount_paint: old.enable_painting && !settings.enable_painting,
                        });
                    }
                }
                out.push(Effect::Configured { game: id });
            }
            Command::Join { actor, game } => {
                self.ready(actor)?;
                let g = self.game(game)?;
                if self.player(actor)?.game == Some(game) {
                    return Err(Error::AlreadyMember);
                }
                if g.settings.invite_only {
                    return Err(Error::InviteOnly);
                }
                if self
                    .player(actor)?
                    .last_join
                    .is_some_and(|t| self.tick.saturating_sub(t) < 600)
                {
                    return Err(Error::Cooldown);
                }
                self.players
                    .get_mut(&actor)
                    .expect("validated player")
                    .last_join = Some(self.tick);
                self.join_member(actor, game, &mut out)?;
            }
            Command::Leave { actor } => {
                self.player(actor)?.game.ok_or(Error::NotMember)?;
                self.remove_member(actor, &mut out)?;
            }
            Command::Invite { actor, target } => {
                let game = self.owned(actor)?;
                self.ready(target)?;
                let p = self.player(target)?;
                if p.game.is_some() {
                    return Err(Error::AlreadyMember);
                }
                if p.ignored_owners.contains(&actor.account) {
                    return Err(Error::Ignored);
                }
                if p.invite.is_some() {
                    return Err(Error::AlreadyInvited);
                }
                self.players
                    .get_mut(&target)
                    .expect("validated player")
                    .invite = Some(game);
                out.push(Effect::Invitation {
                    player: target,
                    game: Some(game),
                });
            }
            Command::Accept { actor, game } => {
                self.ready(actor)?;
                self.game(game)?;
                if self.player(actor)?.invite != Some(game) {
                    return Err(Error::NoInvitation);
                }
                self.join_member(actor, game, &mut out)?;
            }
            Command::Reject {
                actor,
                game,
                ignore_owner,
            } => {
                let owner = self.game(game)?.owner;
                let p = self.players.get_mut(&actor).ok_or(Error::StalePlayer)?;
                if p.invite != Some(game) {
                    return Err(Error::NoInvitation);
                }
                if ignore_owner && self.mode != PolicyMode::LegacyLan {
                    if p.ignored_owners.len() >= MAX_PLAYERS {
                        return Err(Error::Capacity);
                    }
                    p.ignored_owners.insert(owner.account);
                }
                p.invite = None;
                out.push(Effect::Invitation {
                    player: actor,
                    game: None,
                });
            }
            Command::Kick { actor, target } => {
                let id = self.owned(actor)?;
                if self.player(target)?.game != Some(id) {
                    return Err(Error::NotMember);
                }
                self.remove_member(target, &mut out)?;
            }
            Command::Reset { game, authority } => {
                self.authorize_event(game, authority)?;
                let g = self.game(game)?;
                if g.last_reset
                    .is_some_and(|t| self.tick.saturating_sub(t) < 600)
                {
                    return Err(Error::Cooldown);
                }
                if g.round == u64::MAX {
                    return Err(Error::Capacity);
                }
                let members: Vec<_> = g.members.iter().copied().collect();
                let owners = if g.settings.use_all_players_bricks {
                    members.iter().map(|p| p.account).collect()
                } else {
                    vec![g.owner.account]
                };
                let g = self.games.get_mut(&game).expect("validated game");
                g.last_reset = Some(self.tick);
                g.round += 1;
                out.push(Effect::ResetBricks {
                    owners,
                    respawn_vehicles: true,
                    reveal_items: true,
                });
                for p in members {
                    self.score(p, 0, &mut out);
                    self.cleanup(p, true, &mut out);
                    self.spawn(p, SpawnReason::Reset, &mut out);
                }
                out.push(Effect::Reset {
                    game,
                    round: self.games[&game].round,
                });
            }
            Command::RespawnAll { game, authority } => {
                self.authorize_event(game, authority)?;
                let members: Vec<_> = self.games[&game].members.iter().copied().collect();
                for p in members {
                    self.spawn(p, SpawnReason::RespawnAll, &mut out);
                }
            }
            Command::End { actor } => {
                let game = self.owned(actor)?;
                self.end_game(game, &mut out);
            }
            Command::Respawn { actor } => {
                let p = self.player(actor)?;
                let LifeState::Dead { ready_at, .. } = p.life else {
                    return Err(Error::StaleLife);
                };
                if self.tick < ready_at && !(p.game.is_none() && p.admin) {
                    return Err(Error::RespawnNotReady);
                }
                self.spawn(actor, SpawnReason::Respawn, &mut out);
            }
            Command::ForceRespawn { target } => {
                self.player(target)?;
                self.spawn(target, SpawnReason::Respawn, &mut out);
            }
            Command::Message {
                game,
                authority,
                kind,
                text,
            } => {
                self.authorize_event(game, authority)?;
                if text.chars().count() > 200
                    || text.contains('\0')
                    || matches!(kind, MessageKind::Center { seconds } | MessageKind::Bottom { seconds } if !(1..=10).contains(&seconds))
                {
                    return Err(Error::InvalidEvent);
                }
                let instigator = match authority {
                    EventAuthority::Owner(p) | EventAuthority::OwnerBrick { instigator: p, .. } => {
                        Some(p)
                    }
                    EventAuthority::System => None,
                };
                let text = if let Some(p) = instigator {
                    let text = text.replace("%1", &self.players[&p].name);
                    if kind == MessageKind::Chat {
                        text.replace("%2", &self.players[&p].score.to_string())
                    } else {
                        text
                    }
                } else {
                    text
                };
                out.push(Effect::Message {
                    recipients: self.games[&game].members.iter().copied().collect(),
                    kind,
                    text,
                });
            }
        }
        Ok(out)
    }
    /// Start a game mode's mini-game, owned by the server rather than a
    /// player ([`SERVER`]). The host then places every player in it with
    /// [`Self::host_place`]; while it runs, players cannot start, join or
    /// leave mini-games. At most one runs.
    pub fn host_create(&mut self, color: u8, settings: Settings) -> Result<GameId, Error> {
        settings.validate(&self.catalog)?;
        if self.games.values().any(MiniGame::is_server) {
            return Err(Error::ServerGame);
        }
        if color >= 10 || !self.free_colors().contains(&color) {
            return Err(Error::ColorUnavailable);
        }
        if self.games.len() >= MAX_GAMES || self.next_game == u64::MAX {
            return Err(Error::Capacity);
        }
        let id = GameId(self.next_game);
        self.next_game += 1;
        self.games.insert(
            id,
            MiniGame {
                id,
                owner: SERVER,
                color,
                settings,
                members: BTreeSet::new(),
                round: 1,
                last_reset: None,
                ball_update_at: None,
                teams: Teams::default(),
                addon_settings: BTreeMap::new(),
            },
        );
        Ok(id)
    }
    /// The game mode's mini-game, when the server runs one.
    pub fn server_game(&self) -> Option<GameId> {
        self.games.values().find(|g| g.is_server()).map(|g| g.id)
    }
    /// Trusted host placement: bots follow their spawn brick owner's game
    /// without invitations or join cooldowns.
    pub fn host_place(
        &mut self,
        player: PlayerId,
        game: Option<GameId>,
    ) -> Result<Vec<Effect>, Error> {
        self.player(player)?;
        let mut out = Vec::new();
        match game {
            Some(id) => {
                self.game(id)?;
                self.join_member(player, id, &mut out)?;
            }
            None => self.remove_member(player, &mut out)?,
        }
        Ok(out)
    }
    /// Trusted host moderation entry point. A network admin flag must not call this directly.
    pub fn moderate_end(&mut self, game: GameId) -> Result<Vec<Effect>, Error> {
        self.game(game)?;
        if self.next_life > u64::MAX - MAX_PLAYERS as u64 {
            return Err(Error::Capacity);
        }
        let mut out = Vec::new();
        self.end_game(game, &mut out);
        Ok(out)
    }
    /// Tick once alongside the 120 Hz world. Respawn eligibility does not auto-spawn.
    pub fn step(&mut self) -> Result<Vec<Effect>, Error> {
        self.tick = self.tick.checked_add(1).ok_or(Error::InvalidClock)?;
        let mut out = Vec::new();
        for g in self.games.values_mut() {
            if g.ball_update_at.is_some_and(|t| t <= self.tick) {
                g.ball_update_at = None;
                if let Some(image) = g.settings.equipment(&self.catalog).start_ball {
                    for p in &g.members {
                        if matches!(self.players[p].life, LifeState::Alive { .. }) {
                            out.push(Effect::StartBall {
                                player: *p,
                                image: image.clone(),
                                only_if_hands_empty: true,
                            });
                        }
                    }
                }
            }
        }
        Ok(out)
    }
    /// Host-confirmed death after permissions/health processing. Life tokens prevent duplicate scoring.
    pub fn died(
        &mut self,
        victim: PlayerId,
        life: LifeId,
        killer: Option<PlayerId>,
    ) -> Result<Vec<Effect>, Error> {
        let p = self.player(victim)?;
        if p.life != (LifeState::Alive { life }) {
            return Err(Error::StaleLife);
        }
        if let Some(k) = killer {
            self.player(k)?;
        }
        let game = p.game;
        // Cross-minigame attribution must be resolved before death, never award a foreign player.
        if killer.is_some_and(|k| self.players[&k].game != game) {
            return Err(Error::InvalidEvent);
        }
        let delay = game.map_or(1000, |g| self.games[&g].settings.respawn_ms);
        let ready_at = self
            .tick
            .checked_add(manual_respawn_ticks(delay))
            .ok_or(Error::InvalidClock)?;
        let mut out = Vec::new();
        if let Some(id) = game {
            let s = self.games[&id].settings.clone();
            match killer {
                Some(k) if k == victim => self.add_score(victim, s.points_kill_self, &mut out),
                Some(k) => {
                    self.add_score(k, s.points_kill_player, &mut out);
                    self.add_score(victim, s.points_die, &mut out);
                }
                None => self.add_score(victim, s.points_die, &mut out),
            }
        }
        self.players
            .get_mut(&victim)
            .expect("validated player")
            .life = LifeState::Dead { life, ready_at };
        out.push(Effect::Death {
            player: victim,
            life,
            ready_at,
        });
        Ok(out)
    }
    /// Call once following a successfully committed plant/break transaction, not on requests.
    pub fn brick_score(&mut self, player: PlayerId, planted: bool) -> Result<Vec<Effect>, Error> {
        let mut out = Vec::new();
        if let Some(g) = self.player(player)?.game {
            let s = &self.games[&g].settings;
            let delta = if planted {
                s.points_plant_brick
            } else {
                s.points_break_brick
            };
            self.add_score(player, delta, &mut out);
        }
        Ok(out)
    }
    /// `instantRespawn` event output: respawn now, alive or dead, skipping the
    /// respawn delay. The host validates event permission first.
    pub fn event_respawn(&mut self, player: PlayerId) -> Result<Vec<Effect>, Error> {
        self.player(player)?;
        let mut out = Vec::new();
        self.spawn(player, SpawnReason::Respawn, &mut out);
        Ok(out)
    }
    /// Native Client incScore/setScore event integration; host validates event ownership first.
    pub fn event_score(
        &mut self,
        player: PlayerId,
        value: i32,
        additive: bool,
    ) -> Result<Vec<Effect>, Error> {
        self.player(player)?;
        let mut out = Vec::new();
        if additive {
            self.add_score(player, value, &mut out);
        } else {
            self.score(player, value.into(), &mut out);
        }
        Ok(out)
    }
}
