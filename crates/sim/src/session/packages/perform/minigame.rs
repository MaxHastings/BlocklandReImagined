//! What the engine does for the `minigame` operations
//! (`bri_package_runtime::ops::minigame`).
use super::game_hooks::patched_settings;
use super::*;
use bri_minigames as mg;
use bri_package_runtime::ops::GameRule;

impl Perform for ops::ReportColumn {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::ReportColumn { game, change } = self;
        session.package_report_column(game, change)
    }
}
impl Perform for ops::RestoreMinigame {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::RestoreMinigame { game, snapshot } = self;
        session.restore_minigame_snapshot(package, bri_minigames::GameId(game), snapshot)
    }
}
impl Perform for ops::ReviveBricks {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::ReviveBricks { game } = self;
        session.revive_game_bricks(bri_minigames::GameId(game))
    }
}
impl Perform for ops::SetSetting {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SetSetting {
            game,
            team,
            key,
            value,
        } = self;
        session.package_set_setting(package, game, team, key, value)
    }
}
impl Perform for ops::SetZonePeriod {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SetZonePeriod { zone, period_ms } = self;
        session.package_set_zone_period(package, zone, period_ms)
    }
}
impl Perform for ops::SetRespawnTime {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        session.peers.get(&self.player).context("No such player")?;
        Ok(())
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetRespawnTime { player, ms } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        peer.respawn_ms = ms;
        Ok(())
    }
}

/// A mini-game operation that only changes the mini-games: what it does,
/// as a function of them and of who is in the game. `require` runs it on
/// a copy ([`Rule::check`]), so the refusal it hears is the one the
/// operation itself would give, and `perform` runs it for real and then
/// applies the effects to the session.
trait Rule: Clone {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        peers: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>>;
    fn check(&self, session: &Session) -> Result<()> {
        let mut copy = session.minigames.clone();
        self.clone().rule(&mut copy, &session.peers).map(drop)
    }
    fn perform(self, session: &mut Session) -> Result<()> {
        let effects = self.rule(&mut session.minigames, &session.peers)?;
        session.apply_minigame_effects(effects)
    }
}
macro_rules! rule_ops {
    ($($op:ident),* $(,)?) => {$(
        impl Perform for ops::$op {
            fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
                Rule::check(self, session)
            }
            fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
                Rule::perform(self, session)
            }
        }
    )*};
}
rule_ops!(
    SetTeams,
    SetTeam,
    SetScore,
    SetTeamPoints,
    ResetMinigame,
    EndRound,
    SetGameRule,
    CreateMinigame,
    PlaceMember,
    HoldRespawn,
);

/// The mini-game player of a player in the session.
fn member(peers: &BTreeMap<OwnerId, Peer>, owner: u64) -> Result<mg::PlayerId> {
    Ok(peers.get(&owner).context("No such player")?.combat.player)
}
fn team_id(team: u64) -> Result<mg::TeamId> {
    u32::try_from(team)
        .map(mg::TeamId)
        .ok()
        .context("No such team")
}

impl Rule for ops::SetTeams {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        _: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::SetTeams {
            game,
            teams,
            friendly_fire,
            ally_same_color,
        } = self;
        let specs = teams
            .into_iter()
            .map(|t| {
                Ok(mg::TeamSpec {
                    id: t.id.map(team_id).transpose()?,
                    name: t.name,
                    color: t.color,
                    player_type: t.body,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(games
            .set_teams(mg::GameId(game), specs, friendly_fire, ally_same_color)
            .map_err(|e| anyhow::anyhow!("Teams rejected: {e}"))?
            .1)
    }
}
impl Rule for ops::SetTeam {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        peers: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::SetTeam { player, team } = self;
        let target = member(peers, player)?;
        let team = team.map(team_id).transpose()?;
        games
            .assign_team(target, team)
            .map_err(|e| anyhow::anyhow!("Team rejected: {e}"))
    }
}
impl Rule for ops::SetScore {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        peers: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::SetScore { player, value, add } = self;
        let target = member(peers, player)?;
        let value = i32::try_from(value).context("Score out of range")?;
        games
            .event_score(target, value, add)
            .map_err(|e| anyhow::anyhow!("Score rejected: {e}"))
    }
}
impl Rule for ops::SetTeamPoints {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        _: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::SetTeamPoints {
            game,
            team,
            value,
            add,
        } = self;
        games
            .event_team_score(mg::GameId(game), team_id(team)?, value, add)
            .map_err(|e| anyhow::anyhow!("Team points rejected: {e}"))
    }
}
impl Rule for ops::ResetMinigame {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        _: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::ResetMinigame { game } = self;
        games
            .execute(mg::Command::Reset {
                game: mg::GameId(game),
                authority: mg::EventAuthority::System,
            })
            .map_err(|e| anyhow::anyhow!("Reset rejected: {e}"))
    }
}
impl Rule for ops::EndRound {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        peers: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::EndRound {
            game,
            teams,
            players,
        } = self;
        let teams = teams.into_iter().map(team_id).collect::<Result<Vec<_>>>()?;
        let players = players
            .into_iter()
            .map(|p| member(peers, p))
            .collect::<Result<Vec<_>>>()?;
        games
            .end_round(mg::GameId(game), teams, players)
            .map_err(|e| anyhow::anyhow!("Round end rejected: {e}"))
    }
}
impl Rule for ops::SetGameRule {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        _: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::SetGameRule { game, rule } = self;
        Ok({
            let game = mg::GameId(game);
            let rejected = |e: mg::Error| anyhow::anyhow!("Mini-game rule rejected: {e}");
            match rule {
                GameRule::Default(on) => {
                    let now = games.default_game();
                    let next = match (on, now) {
                        (true, _) => Some(game),
                        (false, Some(g)) if g == game => None,
                        (false, other) => other,
                    };
                    games.set_default_game(next).map_err(rejected)?
                }
                GameRule::PaintColor(paint) => {
                    games.set_paint_color(game, paint).map_err(rejected)?
                }
                GameRule::Region(region) => {
                    games
                        .set_region(game, region.map(|[min, max]| mg::Region { min, max }))
                        .map_err(rejected)?;
                    Vec::new()
                }
                GameRule::NameDistance(d) => {
                    games.set_name_distance(game, d).map_err(rejected)?;
                    Vec::new()
                }
                GameRule::KeepScores(keep) => {
                    games.set_keep_scores(game, keep).map_err(rejected)?;
                    Vec::new()
                }
                GameRule::Cleanup { leave } => {
                    games
                        .set_cleanup(game, mg::CleanupRules { leave })
                        .map_err(rejected)?;
                    Vec::new()
                }
                GameRule::ClaimsBricks(on) => {
                    games.set_claims_bricks(game, on).map_err(rejected)?;
                    Vec::new()
                }
                GameRule::Settings(patch) => {
                    let current = &games.game(game).map_err(rejected)?.settings;
                    let settings = patched_settings(current, &patch)?;
                    games.host_configure(game, settings).map_err(rejected)?
                }
                GameRule::End => games.host_end(game).map_err(rejected)?,
            }
        })
    }
}
impl Rule for ops::CreateMinigame {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        peers: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::CreateMinigame {
            owner,
            settings,
            paint,
        } = self;
        let effects = {
            let defaults = games.catalog().defaults.clone();
            let settings = patched_settings(&defaults, &settings)?;
            let color = *games
                .free_colors()
                .first()
                .context("Every mini-game colour is taken")?;
            let (game, effects) = match owner {
                Some(owner) => {
                    let actor = member(peers, owner)?;
                    let effects = games
                        .execute(mg::Command::Create {
                            actor,
                            color,
                            settings,
                        })
                        .map_err(|e| anyhow::anyhow!("Mini-game not made: {e}"))?;
                    let game = games
                        .player(actor)
                        .ok()
                        .and_then(|p| p.game)
                        .context("No mini-game was made")?;
                    (game, effects)
                }
                None => {
                    let game = games
                        .host_create_shared(color, settings)
                        .map_err(|e| anyhow::anyhow!("Mini-game not made: {e}"))?;
                    (game, vec![mg::Effect::Created { game }])
                }
            };
            let mut effects = effects;
            if let Some(paint) = paint {
                effects.extend(
                    games
                        .set_paint_color(game, Some(paint))
                        .map_err(|e| anyhow::anyhow!("Mini-game colour: {e}"))?,
                );
            }
            effects
        };
        Ok(effects)
    }
}
impl Rule for ops::PlaceMember {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        peers: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::PlaceMember { player, game } = self;
        let target = member(peers, player)?;
        games
            .host_place(target, game.map(mg::GameId))
            .map_err(|e| anyhow::anyhow!("Placing rejected: {e}"))
    }
}
impl Rule for ops::HoldRespawn {
    fn rule(
        self,
        games: &mut mg::MinigamesWorld,
        peers: &BTreeMap<OwnerId, Peer>,
    ) -> Result<Vec<mg::Effect>> {
        let ops::HoldRespawn { player, held } = self;
        let target = member(peers, player)?;
        games
            .hold_respawn(target, held)
            .map_err(|e| anyhow::anyhow!("Respawn hold rejected: {e}"))?;
        Ok(Vec::new())
    }
}
