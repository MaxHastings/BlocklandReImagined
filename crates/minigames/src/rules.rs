//! What the host's Add-On rules change about how a game runs: the server's
//! default game, a server-owned game that is not a game mode's, a game's
//! paint colour, region, scores across resets and cleanup. v20 had none of
//! these; Slayer added them. The engine keeps the mechanism, the Add-On
//! decides when to use it.
use crate::*;

impl MinigamesWorld {
    /// The game players who are in none join (Slayer's Default Minigame).
    pub fn default_game(&self) -> Option<GameId> {
        self.default_game
    }
    /// Make `game` the default, or have none. Every ready player in no
    /// game joins it at once, as new players do when they first spawn
    /// ([`Self::join_default`]).
    pub fn set_default_game(&mut self, game: Option<GameId>) -> Result<Vec<Effect>, Error> {
        if let Some(id) = game {
            self.game(id)?;
        }
        self.default_game = game;
        let mut out = Vec::new();
        if let Some(id) = game {
            let free: Vec<_> = self
                .players
                .values()
                .filter(|p| p.ready && p.game.is_none())
                .map(|p| p.id)
                .collect();
            for p in free {
                self.join_member(p, id, &mut out)?;
            }
            out.push(Effect::Configured { game: id });
        }
        Ok(out)
    }
    /// Put `player`, in no game, in the default game, if there is one.
    pub fn join_default(&mut self, player: PlayerId) -> Result<Vec<Effect>, Error> {
        let mut out = Vec::new();
        if let Some(id) = self.default_game
            && self.player(player)?.game.is_none()
            && self.games.contains_key(&id)
        {
            self.join_member(player, id, &mut out)?;
        }
        Ok(out)
    }
    /// A game the server owns that players come to and leave as they like
    /// (a host's game made at server start), unlike a game mode's.
    pub fn host_create_shared(&mut self, color: u8, settings: Settings) -> Result<GameId, Error> {
        settings.validate(&self.catalog)?;
        if self.server_game().is_some() {
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
        let mut game = MiniGame::new(id, SERVER, color, settings);
        game.shared = true;
        self.games.insert(id, game);
        Ok(id)
    }
    /// Give `game` a paint palette colour, or its v20 colour again with
    /// `None`. No two games share one.
    pub fn set_paint_color(
        &mut self,
        game: GameId,
        paint: Option<u8>,
    ) -> Result<Vec<Effect>, Error> {
        self.game(game)?;
        if let Some(c) = paint
            && (c >= 64
                || self
                    .games
                    .values()
                    .any(|g| g.id != game && g.paint_color == Some(c)))
        {
            return Err(Error::ColorUnavailable);
        }
        self.games
            .get_mut(&game)
            .expect("validated game")
            .paint_color = paint;
        Ok(vec![Effect::Configured { game }])
    }
    /// Bound `game` to a box: bricks outside it are not the game's.
    pub fn set_region(&mut self, game: GameId, region: Option<Region>) -> Result<(), Error> {
        if region.is_some_and(|r| !r.valid()) {
            return Err(Error::InvalidSettings);
        }
        self.games.get_mut(&game).ok_or(Error::StaleGame)?.region = region;
        Ok(())
    }
    /// Whether scores carry over `game`'s resets.
    pub fn set_keep_scores(&mut self, game: GameId, keep: bool) -> Result<(), Error> {
        self.games
            .get_mut(&game)
            .ok_or(Error::StaleGame)?
            .keep_scores = keep;
        Ok(())
    }
    pub fn set_cleanup(&mut self, game: GameId, cleanup: CleanupRules) -> Result<(), Error> {
        self.games.get_mut(&game).ok_or(Error::StaleGame)?.cleanup = cleanup;
        Ok(())
    }
    pub fn set_name_distance(&mut self, game: GameId, distance: Option<u32>) -> Result<(), Error> {
        if distance.is_some_and(|d| d > crate::model::MAX_NAME_DISTANCE) {
            return Err(Error::InvalidSettings);
        }
        self.games
            .get_mut(&game)
            .ok_or(Error::StaleGame)?
            .name_distance = distance;
        Ok(())
    }
    pub fn set_claims_bricks(&mut self, game: GameId, claims: bool) -> Result<(), Error> {
        self.games
            .get_mut(&game)
            .ok_or(Error::StaleGame)?
            .claims_bricks = claims;
        Ok(())
    }
    /// Change `game`'s own settings for the host's rules, whoever owns it
    /// (a config loaded into it, Slayer's `setPref` on a v20 setting).
    pub fn host_configure(
        &mut self,
        game: GameId,
        settings: Settings,
    ) -> Result<Vec<Effect>, Error> {
        self.game(game)?;
        let mut out = Vec::new();
        self.configure(game, settings, &mut out)?;
        Ok(out)
    }
    /// End `game` for the host's rules.
    pub fn host_end(&mut self, game: GameId) -> Result<Vec<Effect>, Error> {
        self.moderate_end(game)
    }
}
