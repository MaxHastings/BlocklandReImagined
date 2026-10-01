//! Teams within a mini-game. v20's minigames had none: Add-Ons such as
//! Slayer added them, so the engine keeps the mechanism (who is on which
//! team, who counts as an ally, friendly fire) and an Add-On decides the
//! policy (how many teams, who joins which, what a team wins).
use crate::*;

impl MinigamesWorld {
    /// The team a player plays for, if their game has teams.
    pub fn team_of(&self, player: PlayerId) -> Option<TeamId> {
        self.players.get(&player).and_then(|p| p.team)
    }
    /// Whether two players are teammates or allies in the same game.
    pub fn allied(&self, a: PlayerId, b: PlayerId) -> bool {
        let (Some(pa), Some(pb)) = (self.players.get(&a), self.players.get(&b)) else {
            return false;
        };
        match (pa.game, pa.team, pb.team) {
            (Some(g), Some(ta), Some(tb)) if pb.game == Some(g) => {
                self.games[&g].teams.allied(ta, tb)
            }
            _ => false,
        }
    }
    /// Set a game's teams and team rules, as its Add-On's policy asks.
    /// Teams named by an existing id keep their members; teams left out are
    /// removed and their members are left with no team. Returns each
    /// spec's team id, in order.
    pub fn set_teams(
        &mut self,
        game: GameId,
        specs: Vec<TeamSpec>,
        friendly_fire: bool,
        ally_same_color: bool,
    ) -> Result<(Vec<TeamId>, Vec<Effect>), Error> {
        let g = self.game(game)?;
        if specs.len() > MAX_TEAMS {
            return Err(Error::Capacity);
        }
        let mut kept = BTreeSet::new();
        for spec in &specs {
            if !valid_team_name(&spec.name) {
                return Err(Error::InvalidSettings);
            }
            if let Some(id) = spec.id
                && (g.teams.get(id).is_none() || !kept.insert(id))
            {
                return Err(Error::StaleTeam);
            }
        }
        let new_count = specs.iter().filter(|s| s.id.is_none()).count() as u32;
        if g.teams.next.checked_add(new_count).is_none() {
            return Err(Error::Capacity);
        }
        let orphans: Vec<_> = g
            .members
            .iter()
            .copied()
            .filter(|p| self.players[p].team.is_some_and(|t| !kept.contains(&t)))
            .collect();
        let teams = &mut self.games.get_mut(&game).expect("validated game").teams;
        let mut ids = Vec::with_capacity(specs.len());
        teams.list = specs
            .into_iter()
            .map(|spec| {
                let id = spec.id.unwrap_or_else(|| {
                    let id = TeamId(teams.next);
                    teams.next += 1;
                    id
                });
                ids.push(id);
                Team {
                    id,
                    name: spec.name,
                    color: spec.color,
                }
            })
            .collect();
        teams.friendly_fire = friendly_fire;
        teams.ally_same_color = ally_same_color;
        let mut out = Vec::new();
        for p in orphans {
            self.clear_team(p, game, &mut out);
        }
        out.push(Effect::TeamsConfigured { game });
        Ok((ids, out))
    }
    /// Put a member of a game on one of its teams, or on none. The host or
    /// the Add-On decides whether they respawn for it.
    pub fn assign_team(
        &mut self,
        player: PlayerId,
        team: Option<TeamId>,
    ) -> Result<Vec<Effect>, Error> {
        let game = self.player(player)?.game.ok_or(Error::NotMember)?;
        if let Some(t) = team
            && self.games[&game].teams.get(t).is_none()
        {
            return Err(Error::StaleTeam);
        }
        let mut out = Vec::new();
        let p = self.players.get_mut(&player).expect("validated player");
        if p.team != team {
            p.team = team;
            out.push(Effect::TeamChanged { player, game, team });
        }
        Ok(out)
    }
    pub(crate) fn clear_team(&mut self, player: PlayerId, game: GameId, out: &mut Vec<Effect>) {
        let p = self.players.get_mut(&player).expect("validated player");
        if p.team.take().is_some() {
            out.push(Effect::TeamChanged {
                player,
                game,
                team: None,
            });
        }
    }
}
