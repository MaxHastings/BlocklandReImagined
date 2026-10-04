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
    /// Teams named by an existing id keep their members (an id the game
    /// has not got makes that slot's team); teams left out are
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
                && (!TeamId::valid(id) || !kept.insert(id))
            {
                return Err(Error::StaleTeam);
            }
        }
        let orphans: Vec<_> = g
            .members
            .iter()
            .copied()
            .filter(|p| self.players[p].team.is_some_and(|t| !kept.contains(&t)))
            .collect();
        let teams = &mut self.games.get_mut(&game).expect("validated game").teams;
        let mut old: BTreeMap<TeamId, Team> = std::mem::take(&mut teams.list)
            .into_iter()
            .map(|t| (t.id, t))
            .collect();
        // New teams take the lowest slots no spec names; there are at most
        // MAX_TEAMS specs, so one is always free.
        let mut free = (1..=MAX_TEAMS as u32)
            .map(TeamId)
            .filter(|id| !kept.contains(id));
        let mut ids = Vec::with_capacity(specs.len());
        teams.list = specs
            .into_iter()
            .map(|spec| {
                let id = spec
                    .id
                    .unwrap_or_else(|| free.next().expect("a free team slot"));
                ids.push(id);
                Team {
                    id,
                    name: spec.name,
                    color: spec.color,
                    addon_settings: old
                        .remove(&id)
                        .map(|t| t.addon_settings)
                        .unwrap_or_default(),
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
    /// Whether `actor` may change a game's settings and teams (Slayer's
    /// `canEdit`): its owner, or an admin.
    pub fn can_edit(&self, actor: PlayerId, game: GameId) -> bool {
        self.games.get(&game).is_some_and(|g| g.owner == actor)
            || self.players.get(&actor).is_some_and(|p| p.admin)
    }
    /// Change Add-On settings of a game and its teams. The host has checked
    /// each value against its definition and who may change it; `None`
    /// puts a setting back to its default.
    pub fn set_addon_settings(
        &mut self,
        game: GameId,
        changes: Vec<SettingChange>,
    ) -> Result<Vec<Effect>, Error> {
        let g = self.game(game)?;
        if changes.len() > MAX_ADDON_SETTINGS {
            return Err(Error::Capacity);
        }
        for c in &changes {
            if c.key.is_empty() || c.key.len() > MAX_SETTING_KEY {
                return Err(Error::InvalidSettings);
            }
            if let Some(SettingValue::Text(t)) = &c.value
                && t.len() > MAX_SETTING_TEXT
            {
                return Err(Error::InvalidSettings);
            }
            if let Some(t) = c.team
                && g.teams.get(t).is_none()
            {
                return Err(Error::StaleTeam);
            }
        }
        let g = self.games.get_mut(&game).expect("validated game");
        let mut keys = BTreeSet::new();
        for c in changes {
            let map = match c.team {
                None => &mut g.addon_settings,
                Some(t) => {
                    &mut g
                        .teams
                        .list
                        .iter_mut()
                        .find(|team| team.id == t)
                        .expect("validated team")
                        .addon_settings
                }
            };
            let changed = match c.value {
                Some(v) => {
                    if !map.contains_key(&c.key) && map.len() >= MAX_ADDON_SETTINGS {
                        return Err(Error::Capacity);
                    }
                    map.insert(c.key.clone(), v.clone()) != Some(v)
                }
                None => map.remove(&c.key).is_some(),
            };
            if changed {
                keys.insert(c.key);
            }
        }
        Ok(if keys.is_empty() {
            Vec::new()
        } else {
            vec![Effect::AddOnSettings {
                game,
                keys: keys.into_iter().collect(),
            }]
        })
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
