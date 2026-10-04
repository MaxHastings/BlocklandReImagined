//! Bounded observations of canonical round effects, never objective commands.
use super::*;
use bri_minigames as mg;

const RETAINED: usize = 64;

/// The actual result emitted by MiniGame rules. Empty winner lists denote a
/// round ended without named winners; the planner's projection is not a result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundResult {
    pub game: u64,
    pub round: u64,
    pub tick: u64,
    pub players: Vec<mg::PlayerId>,
    pub owners: Vec<OwnerId>,
    pub teams: Vec<mg::TeamId>,
}

impl Session {
    /// Read-only recent canonical outcomes for diagnostics/headless acceptance.
    /// This is not replicated, saved, or a way to announce a winner.
    pub fn round_results(&self) -> impl DoubleEndedIterator<Item = &RoundResult> {
        self.round_results.iter()
    }

    pub(super) fn observe_round_result(&mut self, effect: &mg::Effect) {
        let mg::Effect::RoundEnded {
            game,
            teams,
            players,
        } = effect
        else {
            return;
        };
        let Ok(state) = self.minigames.game(*game) else {
            return;
        };
        let result = RoundResult {
            game: game.0,
            round: state.round,
            tick: self.simulation.state().tick,
            players: players.clone(),
            owners: players.iter().filter_map(|p| self.owner_of(*p)).collect(),
            teams: teams.clone(),
        };
        if self.round_results.len() == RETAINED {
            self.round_results.pop_front();
        }
        self.round_results.push_back(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes_only_follow_canonical_round_effects_and_retention_is_bounded() {
        let simulation = Simulation::new(
            World::new("outcomes".into(), "fixture".into(), vec![[1.0; 4]]),
            crate::definitions::Definitions::default(),
            vec![
                rapier3d::prelude::ColliderBuilder::cuboid(30.0, 0.5, 30.0)
                    .translation(Vec3::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap();
        let mut s = Session::new(simulation);
        let owner = s
            .join("Participant".into(), Vec3::new(0.0, 0.05, 0.0), true)
            .unwrap();
        s.minigame_request(
            owner,
            MiniGameRequest::Create {
                color: 0,
                settings: mg::Settings::default(),
            },
        )
        .unwrap();
        let game = s.game_of(owner).unwrap();
        let player = s.peers[&owner].combat.player;
        assert_eq!(s.round_results().count(), 0);
        let effects = s.minigames.end_round(game, vec![], vec![player]).unwrap();
        s.apply_minigame_effects(effects).unwrap();
        let result = s.round_results().next_back().unwrap();
        assert_eq!(result.game, game.0);
        assert_eq!(result.owners, vec![owner]);
        assert_eq!(result.players, vec![player]);
        assert!(result.teams.is_empty());
        for _ in 0..RETAINED + 2 {
            for _ in 0..600 {
                s.minigames.step().unwrap();
            }
            let effects = s
                .minigames
                .execute(mg::Command::Reset {
                    game,
                    authority: mg::EventAuthority::Owner(player),
                })
                .unwrap();
            s.apply_minigame_effects(effects).unwrap();
            let effects = s.minigames.end_round(game, vec![], vec![]).unwrap();
            s.apply_minigame_effects(effects).unwrap();
        }
        assert_eq!(s.round_results().count(), RETAINED);
        assert!(
            s.round_results()
                .all(|r| r.owners.is_empty() && r.players.is_empty())
        );
    }
}
