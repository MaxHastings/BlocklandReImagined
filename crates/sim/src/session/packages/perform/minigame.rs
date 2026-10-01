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
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetRespawnTime { player, ms } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        peer.respawn_ms = ms;
        Ok(())
    }
}
impl Perform for ops::SetTeams {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetTeams {
            game,
            teams,
            friendly_fire,
            ally_same_color,
        } = self;
        let effects = {
            let specs = teams
                .into_iter()
                .map(|t| {
                    Ok(mg::TeamSpec {
                        id: t
                            .id
                            .map(|id| u32::try_from(id).map(mg::TeamId))
                            .transpose()
                            .ok()
                            .context("No such team")?,
                        name: t.name,
                        color: t.color,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            session
                .minigames
                .set_teams(mg::GameId(game), specs, friendly_fire, ally_same_color)
                .map_err(|e| anyhow::anyhow!("Teams rejected: {e}"))?
                .1
        };
        session.apply_minigame_effects(effects)
    }
}
impl Perform for ops::SetTeam {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetTeam { player, team } = self;
        let effects = {
            let target = session.minigame_player(player)?;
            let team = team
                .map(|t| u32::try_from(t).map(mg::TeamId))
                .transpose()
                .ok()
                .context("No such team")?;
            session
                .minigames
                .assign_team(target, team)
                .map_err(|e| anyhow::anyhow!("Team rejected: {e}"))?
        };
        session.apply_minigame_effects(effects)
    }
}
impl Perform for ops::SetScore {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetScore { player, value, add } = self;
        let effects = {
            let target = session.minigame_player(player)?;
            let value = i32::try_from(value).context("Score out of range")?;
            session
                .minigames
                .event_score(target, value, add)
                .map_err(|e| anyhow::anyhow!("Score rejected: {e}"))?
        };
        session.apply_minigame_effects(effects)
    }
}
impl Perform for ops::ResetMinigame {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::ResetMinigame { game } = self;
        let effects = {
            session
                .minigames
                .execute(mg::Command::Reset {
                    game: mg::GameId(game),
                    authority: mg::EventAuthority::System,
                })
                .map_err(|e| anyhow::anyhow!("Reset rejected: {e}"))?
        };
        session.apply_minigame_effects(effects)
    }
}
impl Perform for ops::EndRound {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::EndRound {
            game,
            teams,
            players,
        } = self;
        let effects = {
            let teams = teams
                .into_iter()
                .map(|t| u32::try_from(t).map(mg::TeamId))
                .collect::<Result<Vec<_>, _>>()
                .ok()
                .context("No such team")?;
            let players = players
                .into_iter()
                .map(|p| session.minigame_player(p))
                .collect::<Result<Vec<_>>>()?;
            session
                .minigames
                .end_round(mg::GameId(game), teams, players)
                .map_err(|e| anyhow::anyhow!("Round end rejected: {e}"))?
        };
        session.apply_minigame_effects(effects)
    }
}
impl Perform for ops::SetGameRule {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetGameRule { game, rule } = self;
        let effects = {
            let game = mg::GameId(game);
            let rejected = |e: mg::Error| anyhow::anyhow!("Mini-game rule rejected: {e}");
            match rule {
                GameRule::Default(on) => {
                    let now = session.minigames.default_game();
                    let next = match (on, now) {
                        (true, _) => Some(game),
                        (false, Some(g)) if g == game => None,
                        (false, other) => other,
                    };
                    session.minigames.set_default_game(next).map_err(rejected)?
                }
                GameRule::PaintColor(paint) => session
                    .minigames
                    .set_paint_color(game, paint)
                    .map_err(rejected)?,
                GameRule::Region(region) => {
                    session
                        .minigames
                        .set_region(game, region.map(|[min, max]| mg::Region { min, max }))
                        .map_err(rejected)?;
                    Vec::new()
                }
                GameRule::NameDistance(d) => {
                    session
                        .minigames
                        .set_name_distance(game, d)
                        .map_err(rejected)?;
                    Vec::new()
                }
                GameRule::KeepScores(keep) => {
                    session
                        .minigames
                        .set_keep_scores(game, keep)
                        .map_err(rejected)?;
                    Vec::new()
                }
                GameRule::Cleanup { leave } => {
                    session
                        .minigames
                        .set_cleanup(game, mg::CleanupRules { leave })
                        .map_err(rejected)?;
                    Vec::new()
                }
                GameRule::ClaimsBricks(on) => {
                    session
                        .minigames
                        .set_claims_bricks(game, on)
                        .map_err(rejected)?;
                    Vec::new()
                }
                GameRule::Settings(patch) => {
                    let current = &session.minigames.game(game).map_err(rejected)?.settings;
                    let settings = patched_settings(current, &patch)?;
                    session
                        .minigames
                        .host_configure(game, settings)
                        .map_err(rejected)?
                }
                GameRule::End => session.minigames.host_end(game).map_err(rejected)?,
            }
        };
        session.apply_minigame_effects(effects)
    }
}
impl Perform for ops::CreateMinigame {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::CreateMinigame {
            owner,
            settings,
            paint,
        } = self;
        let effects = {
            let defaults = session.minigames.catalog().defaults.clone();
            let settings = patched_settings(&defaults, &settings)?;
            let color = *session
                .minigames
                .free_colors()
                .first()
                .context("Every mini-game colour is taken")?;
            let (game, effects) = match owner {
                Some(owner) => {
                    let actor = session.minigame_player(owner)?;
                    let effects = session
                        .minigames
                        .execute(mg::Command::Create {
                            actor,
                            color,
                            settings,
                        })
                        .map_err(|e| anyhow::anyhow!("Mini-game not made: {e}"))?;
                    let game = session
                        .minigames
                        .player(actor)
                        .ok()
                        .and_then(|p| p.game)
                        .context("No mini-game was made")?;
                    (game, effects)
                }
                None => {
                    let game = session
                        .minigames
                        .host_create_shared(color, settings)
                        .map_err(|e| anyhow::anyhow!("Mini-game not made: {e}"))?;
                    (game, vec![mg::Effect::Created { game }])
                }
            };
            let mut effects = effects;
            if let Some(paint) = paint {
                effects.extend(
                    session
                        .minigames
                        .set_paint_color(game, Some(paint))
                        .map_err(|e| anyhow::anyhow!("Mini-game colour: {e}"))?,
                );
            }
            effects
        };
        session.apply_minigame_effects(effects)
    }
}
impl Perform for ops::PlaceMember {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::PlaceMember { player, game } = self;
        let effects = {
            let target = session.minigame_player(player)?;
            session
                .minigames
                .host_place(target, game.map(mg::GameId))
                .map_err(|e| anyhow::anyhow!("Placing rejected: {e}"))?
        };
        session.apply_minigame_effects(effects)
    }
}
impl Perform for ops::HoldRespawn {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::HoldRespawn { player, held } = self;
        let effects = {
            let target = session.minigame_player(player)?;
            session
                .minigames
                .hold_respawn(target, held)
                .map_err(|e| anyhow::anyhow!("Respawn hold rejected: {e}"))?;
            Vec::new()
        };
        session.apply_minigame_effects(effects)
    }
}
