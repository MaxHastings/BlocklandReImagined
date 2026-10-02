use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denial {
    /// A teammate or ally, with friendly fire off.
    Teammate,
    DifferentGame,
    Disabled,
    NotYours,
    NotInGame,
    StaleIdentity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(Denial),
    OutsideMinigames,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Brick,
    Vehicle,
    Bot,
    Item,
    Projectile,
    Other,
}
/// Explicit attribution is captured by the host at object creation. Round tokens
/// invalidate projectiles left over from reset; player generation invalidates reconnects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Membership {
    Owner,
    /// As [`Membership::Owner`], for a brick at this point: outside its
    /// game's [`MiniGame::region`] it is no game's.
    OwnerAt([f32; 3]),
    Outside,
    Explicit {
        game: GameId,
        round: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Player {
        player: PlayerId,
        life: LifeId,
    },
    Object {
        kind: ObjectKind,
        owner: Option<AccountId>,
        membership: Membership,
        spawn_brick: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentDamage {
    Falling,
    Impact,
    Water,
    Lava,
    Suicide,
    Script,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageSource {
    Player(PlayerId),
    Projectile {
        player: PlayerId,
        game: Option<GameId>,
        round: u64,
    },
    Vehicle {
        driver: PlayerId,
        game: Option<GameId>,
        round: u64,
    },
    Environment(EnvironmentDamage),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildAction {
    Build,
    Paint,
    Wand,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RespawnObject {
    Vehicle,
    Brick,
    Item,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnPoint {
    pub id: u64,
    pub owner: AccountId,
}
#[derive(Clone, Copy)]
struct Resolved {
    game: Option<GameId>,
    owner: Option<AccountId>,
    player: Option<PlayerId>,
    kind: ObjectKind,
    loose_item: bool,
}

impl MinigamesWorld {
    /// The game that claims bricks of builders in no game
    /// ([`MiniGame::claims_bricks`]): a server-owned one first, else the
    /// first made.
    fn claiming_game(&self) -> Option<GameId> {
        let claiming = || {
            self.games
                .values()
                .filter(|g| g.claims_bricks && g.settings.use_all_players_bricks)
        };
        claiming()
            .find(|g| g.is_server())
            .or_else(|| claiming().next())
            .map(|g| g.id)
    }
    fn resolve_target(&self, target: Target) -> Result<Resolved, Error> {
        match target {
            Target::Player { player, life } => {
                let p = self.player(player)?;
                if p.life != (LifeState::Alive { life }) {
                    return Err(Error::StaleLife);
                }
                Ok(Resolved {
                    game: p.game,
                    owner: Some(player.account),
                    player: Some(player),
                    kind: ObjectKind::Other,
                    loose_item: false,
                })
            }
            Target::Object {
                kind,
                owner,
                membership,
                spawn_brick,
            } => {
                let game = match membership {
                    Membership::Outside => None,
                    Membership::Explicit { game, round } => {
                        if self.game(game)?.round != round {
                            return Err(Error::StaleGame);
                        }
                        Some(game)
                    }
                    Membership::Owner | Membership::OwnerAt(_) => {
                        if self.mode == PolicyMode::LegacyLan
                            && matches!(
                                kind,
                                ObjectKind::Brick | ObjectKind::Vehicle | ObjectKind::Item
                            )
                        {
                            None
                        } else {
                            let game = owner.and_then(|a| {
                                match self.players.values().find(|p| p.id.account == a) {
                                    Some(p) => p.game.or_else(|| self.claiming_game()),
                                    // The world's bricks belong to a game
                                    // mode's mini-game.
                                    None if a == SERVER.account => self.server_game(),
                                    None => self.claiming_game(),
                                }
                            });
                            match (membership, game) {
                                (Membership::OwnerAt(at), Some(g))
                                    if self.games[&g].region.is_some_and(|r| !r.contains(at)) =>
                                {
                                    None
                                }
                                _ => game,
                            }
                        }
                    }
                };
                Ok(Resolved {
                    game,
                    owner,
                    player: None,
                    kind,
                    loose_item: kind == ObjectKind::Item && !spawn_brick,
                })
            }
        }
    }
    pub fn target_for_player(&self, player: PlayerId) -> Result<Target, Error> {
        let LifeState::Alive { life } = self.player(player)?.life else {
            return Err(Error::StaleLife);
        };
        Ok(Target::Player { player, life })
    }
    /// Captures source attribution for a projectile/vehicle without collapsing account/session identity.
    pub fn projectile_source(&self, player: PlayerId) -> Result<DamageSource, Error> {
        let game = self.player(player)?.game;
        Ok(DamageSource::Projectile {
            player,
            game,
            round: game.map_or(0, |g| self.games[&g].round),
        })
    }
    fn source_player(&self, source: DamageSource) -> Result<PlayerId, Error> {
        match source {
            DamageSource::Player(p) => {
                self.player(p)?;
                Ok(p)
            }
            DamageSource::Projectile {
                player,
                game,
                round,
            }
            | DamageSource::Vehicle {
                driver: player,
                game,
                round,
            } => {
                if self.player(player)?.game != game
                    || game.is_some_and(|g| self.games[&g].round != round)
                    || (game.is_none() && round != 0)
                {
                    return Err(Error::StaleGame);
                }
                Ok(player)
            }
            DamageSource::Environment(_) => Err(Error::InvalidEvent),
        }
    }
    pub fn can_use(&self, actor: PlayerId, target: Target) -> Decision {
        let Ok(p) = self.player(actor) else {
            return Decision::Deny(Denial::StaleIdentity);
        };
        let Ok(mut t) = self.resolve_target(target) else {
            return Decision::Deny(Denial::StaleIdentity);
        };
        if self.mode == PolicyMode::LegacyLan {
            return Decision::Allow;
        }
        if t.game != p.game && t.owner == Some(actor.account) && t.player.is_none() {
            t.game = p.game;
        }
        if t.game.is_none() && p.game.is_none() {
            return Decision::OutsideMinigames;
        }
        if t.game != p.game {
            return Decision::Deny(Denial::DifferentGame);
        }
        let g = &self.games[&p.game.expect("same valid game")];
        if t.loose_item {
            return Decision::Allow;
        }
        if g.settings.use_all_players_bricks {
            if g.settings.players_use_own_bricks && t.owner != Some(actor.account) {
                Decision::Deny(Denial::NotYours)
            } else {
                Decision::Allow
            }
        } else if t.player.is_some() || t.owner == Some(g.owner.account) {
            Decision::Allow
        } else {
            Decision::Deny(Denial::NotInGame)
        }
    }
    /// Source miniGameCanDamage tri-state; OutsideMinigames requires the host's sandbox policy.
    /// Environment/fall processing is separate in the original Armor callbacks.
    pub fn can_damage(&self, source: DamageSource, target: Target) -> Decision {
        let Ok(mut t) = self.resolve_target(target) else {
            return Decision::Deny(Denial::StaleIdentity);
        };
        if let DamageSource::Environment(kind) = source {
            if matches!(kind, EnvironmentDamage::Falling | EnvironmentDamage::Impact) {
                return match t.game {
                    Some(g) if !self.games[&g].settings.falling_damage => {
                        Decision::Deny(Denial::Disabled)
                    }
                    Some(_) => Decision::Allow,
                    None => Decision::OutsideMinigames,
                };
            }
            // Lava/water/script and suicide do not consult weaponDamage/selfDamage.
            return Decision::Allow;
        }
        let Ok(actor) = self.source_player(source) else {
            return Decision::Deny(Denial::StaleIdentity);
        };
        let p = &self.players[&actor];
        if t.game != p.game && t.owner == Some(actor.account) && t.player.is_none() {
            t.game = p.game;
        }
        if self.mode == PolicyMode::LegacyLan {
            let Some(id) = p.game else {
                return Decision::OutsideMinigames;
            };
            if t.player.is_some() && t.game != p.game {
                return Decision::Deny(Denial::DifferentGame);
            }
            if let Some(victim) = t.player
                && victim != actor
                && !self.games[&id].teams.friendly_fire
                && self.allied(actor, victim)
            {
                return Decision::Deny(Denial::Teammate);
            }
            let s = &self.games[&id].settings;
            let enabled = match t.kind {
                ObjectKind::Vehicle | ObjectKind::Bot => s.vehicle_damage,
                ObjectKind::Brick => s.brick_damage,
                _ => s.weapon_damage,
            };
            return if enabled {
                Decision::Allow
            } else {
                Decision::Deny(Denial::Disabled)
            };
        }
        if t.game.is_none() && p.game.is_none() {
            return Decision::OutsideMinigames;
        }
        if t.game != p.game {
            return Decision::Deny(Denial::DifferentGame);
        }
        let g = &self.games[&p.game.expect("same valid game")];
        let s = &g.settings;
        if let Some(victim) = t.player {
            if victim != actor && !g.teams.friendly_fire && self.allied(actor, victim) {
                return Decision::Deny(Denial::Teammate);
            }
            return if s.weapon_damage && (victim != actor || s.self_damage) {
                Decision::Allow
            } else {
                Decision::Deny(Denial::Disabled)
            };
        }
        let enabled = match t.kind {
            ObjectKind::Vehicle | ObjectKind::Bot => s.vehicle_damage,
            ObjectKind::Brick => s.brick_damage,
            _ => s.weapon_damage,
        };
        if !enabled {
            return Decision::Deny(Denial::Disabled);
        }
        if s.use_all_players_bricks || t.owner == Some(g.owner.account) {
            Decision::Allow
        } else {
            Decision::Deny(Denial::NotInGame)
        }
    }
    /// Radius callbacks additionally enforce selfDamage even in legacy LAN mode.
    pub fn can_radius_damage(&self, source: DamageSource, target: Target) -> Decision {
        if let (Ok(p), Target::Player { player, .. }) = (self.source_player(source), target)
            && p == player
            && let Some(g) = self.players[&p].game
            && !self.games[&g].settings.self_damage
        {
            return Decision::Deny(Denial::Disabled);
        }
        self.can_damage(source, target)
    }
    pub fn can_build(&self, player: PlayerId, action: BuildAction) -> Result<Decision, Error> {
        let p = self.player(player)?;
        let Some(game) = p.game else {
            return Ok(Decision::OutsideMinigames);
        };
        let s = &self.games[&game].settings;
        let enabled = match action {
            BuildAction::Build => s.enable_building,
            BuildAction::Paint => s.enable_painting,
            BuildAction::Wand => s.enable_wand,
        };
        Ok(if enabled {
            Decision::Allow
        } else {
            Decision::Deny(Denial::Disabled)
        })
    }
    pub fn respawn_delay(&self, game: Option<GameId>, kind: RespawnObject) -> Result<u64, Error> {
        let s = game
            .map(|id| self.game(id).map(|g| &g.settings))
            .transpose()?;
        let ms = match kind {
            RespawnObject::Vehicle => s.map_or(0, |s| s.vehicle_respawn_ms),
            RespawnObject::Brick => s.map_or(30000, |s| s.brick_respawn_ms),
            RespawnObject::Item => 4000,
        };
        Ok(ticks_for_ms(ms))
    }
    /// WheeledVehicleData destruction uses the damage source's minigame,
    /// clamps against authored burn time, then adds 100 ms. Flying/AIPlayer
    /// callbacks use the base delay instead; their host adapters select attribution.
    pub fn wheeled_destroy_respawn_delay(
        &self,
        source_game: Option<GameId>,
        burn_ms: u32,
    ) -> Result<u64, Error> {
        if burn_ms > 300000 {
            return Err(Error::InvalidSettings);
        }
        let configured = source_game
            .map(|id| self.game(id).map(|g| g.settings.vehicle_respawn_ms))
            .transpose()?
            .unwrap_or(0);
        Ok(ticks_for_ms(configured.max(burn_ms) + 100))
    }
    /// Host supplies actual spawn bricks. No candidates means use the map spawn.
    /// Sorting removes hash/physics iteration nondeterminism; each eligible brick has equal weight.
    pub fn pick_spawn(
        &self,
        player: PlayerId,
        points: &[SpawnPoint],
        random_word: u64,
    ) -> Result<Option<u64>, Error> {
        if points.len() > 65536 {
            return Err(Error::Capacity);
        }
        let p = self.player(player)?;
        let Some(id) = p.game else {
            return Ok(None);
        };
        let g = &self.games[&id];
        let s = &g.settings;
        if !s.use_spawn_bricks {
            return Ok(None);
        }
        let mut eligible: Vec<_> = points
            .iter()
            .filter(|point| {
                if !s.use_all_players_bricks {
                    point.owner == g.owner.account
                } else if s.players_use_own_bricks {
                    point.owner == player.account
                } else {
                    g.members.iter().any(|m| m.account == point.owner)
                }
            })
            .map(|p| p.id)
            .collect();
        eligible.sort_unstable();
        eligible.dedup();
        if eligible.is_empty() {
            return Ok(None);
        }
        let index = ((u128::from(random_word) * eligible.len() as u128) >> 64) as usize;
        Ok(Some(eligible[index]))
    }
}
