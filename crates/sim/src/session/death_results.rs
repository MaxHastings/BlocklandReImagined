//! Bounded observations of accepted canonical deaths, never planning commands.
use super::*;

/// An accepted MiniGame life transition. Identity includes the life so a later
/// respawn of the same participant cannot satisfy an earlier action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathResult {
    pub victim: OwnerId,
    pub life: bri_minigames::LifeId,
    pub killer: Option<OwnerId>,
    pub game: Option<u64>,
    pub round: Option<u64>,
    pub tick: u64,
}

/// Health a player actually lost to one hit, after protection, crouching
/// and the like, and at most what it had left.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageResult {
    pub victim: OwnerId,
    pub source: Option<OwnerId>,
    pub amount: f32,
    /// The projectile that did it, when one did.
    pub projectile: Option<String>,
    pub tick: u64,
}

/// How many recent observations of each kind are kept.
const RETAINED: usize = 64;

impl Session {
    /// Recent actual damage to players; not replicated or saved.
    pub fn damage_results(&self) -> impl DoubleEndedIterator<Item = &DamageResult> {
        self.damage_results.iter()
    }

    /// How many damage results have ever been recorded, so a reader can
    /// tell none fell out of the bounded history unread.
    pub fn damage_recorded(&self) -> u64 {
        self.damage_recorded
    }

    pub(super) fn observe_damage_result(&mut self, result: DamageResult) {
        self.damage_recorded += 1;
        if self.damage_results.len() == RETAINED {
            self.damage_results.pop_front();
        }
        self.damage_results.push_back(result);
    }

    /// Recent actual life transitions; not replicated or saved.
    pub fn death_results(&self) -> impl DoubleEndedIterator<Item = &DeathResult> {
        self.death_results.iter()
    }

    pub(super) fn observe_death_result(&mut self, result: DeathResult) {
        if self.death_results.len() == RETAINED {
            self.death_results.pop_front();
        }
        self.death_results.push_back(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_deaths_are_exact_life_observations_not_repeated_damage() {
        let simulation = Simulation::new(
            World::new("deaths".into(), "fixture".into(), vec![[1.0; 4]]),
            crate::definitions::Definitions::default(),
            vec![
                rapier3d::prelude::ColliderBuilder::cuboid(30.0, 0.5, 30.0)
                    .translation(Vec3::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap();
        let mut session = Session::new(simulation);
        let owner = session
            .join("Participant".into(), Vec3::new(0.0, 0.05, 0.0), true)
            .unwrap();
        session
            .minigame_request(
                owner,
                MiniGameRequest::Create {
                    color: 0,
                    settings: bri_minigames::Settings::default(),
                },
            )
            .unwrap();
        let game = session.game_of(owner).unwrap();
        let round = session.minigames.game(game).unwrap().round;
        let bri_minigames::LifeState::Alive { life } = session
            .minigames
            .player(session.peers[&owner].combat.player)
            .unwrap()
            .life
        else {
            panic!("new participant must have an active life");
        };
        assert_eq!(session.death_results().count(), 0);
        session
            .kill(
                owner,
                Some(owner),
                super::super::combat::DamageKind::Suicide,
            )
            .unwrap();
        let result = session.death_results().next_back().unwrap();
        assert_eq!(result.victim, owner);
        assert_eq!(result.life, life);
        assert_eq!(result.killer, Some(owner));
        assert_eq!(result.game, Some(game.0));
        assert_eq!(result.round, Some(round));
        session
            .kill(
                owner,
                Some(owner),
                super::super::combat::DamageKind::Suicide,
            )
            .unwrap();
        assert_eq!(session.death_results().count(), 1);
        for _ in 0..66 {
            let effects = session
                .minigames
                .execute(bri_minigames::Command::ForceRespawn {
                    target: session.peers[&owner].combat.player,
                })
                .unwrap();
            session.apply_minigame_effects(effects).unwrap();
            session
                .kill(owner, None, super::super::combat::DamageKind::Event)
                .unwrap();
        }
        assert_eq!(session.death_results().count(), 64);
        assert!(session.death_results().all(|r| r.killer.is_none()));
        assert_ne!(session.death_results().next_back().unwrap().life, life);
    }

    #[test]
    fn damage_results_record_health_actually_lost() {
        let simulation = Simulation::new(
            World::new("damage".into(), "fixture".into(), vec![[1.0; 4]]),
            crate::definitions::Definitions::default(),
            vec![
                rapier3d::prelude::ColliderBuilder::cuboid(30.0, 0.5, 30.0)
                    .translation(Vec3::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap();
        let mut session = Session::new(simulation);
        let owner = session
            .join("Participant".into(), Vec3::new(0.0, 0.05, 0.0), true)
            .unwrap();
        let health = session.peers[&owner].combat.health;
        let hit = |s: &mut Session, amount: f32| {
            s.damage_player_at(
                owner,
                amount,
                super::super::combat::DamageKind::Event,
                Some(owner),
                None,
            )
            .unwrap();
        };
        hit(&mut session, 10.0);
        hit(&mut session, health * 2.0);
        // Dead: a further hit takes nothing and is not recorded.
        hit(&mut session, 10.0);
        let lost: Vec<f32> = session.damage_results().map(|r| r.amount).collect();
        assert_eq!(lost, vec![10.0, health - 10.0]);
        assert_eq!(session.damage_recorded(), 2);
        assert!(
            session
                .damage_results()
                .all(|r| r.victim == owner && r.source == Some(owner) && r.projectile.is_none())
        );
    }
}
