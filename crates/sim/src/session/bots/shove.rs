//! What a push would do (`docs/plans/v0.2.6-design-ledge-pushes.md`): an
//! attack is worth its predicted effect, not a flat number.
//!
//! A push sends its target off at the velocity the engine would give it
//! (the weapon's impulse along the shot and up, over the body's mass), and
//! the target's own motor is flown from there over the real floor
//! (`reach::shove_landing`) to where it comes down and the hardest blow it
//! takes on the way. That blow is worth the falling damage the game would
//! deal for it (`Session::fall_harm`, the same rule, read-only). A shove
//! into nothing, onto a step, or in a game with falling damage off is
//! worth nothing; one that kills is worth the target's health. Where the
//! target contests a body the bot's objective wants, knocking it off is
//! worth [`contest::CONTEST_SHOVE_WORTH`] at least (the larger of the two,
//! never their sum).
use super::*;
use crate::nav::Ground;

/// Where a shove is predicted to put its target: where it comes down and
/// about how many seconds that takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Landing {
    pub at: Vec3,
    pub seconds: f32,
}

/// A shove's worth, in health: the larger of the harm where it lands and
/// what knocking the target off a contested body is worth (never their
/// sum), at most the health it has.
fn worth(fall: f32, contest: f32, health: f32) -> f32 {
    fall.max(contest).min(health.max(0.0))
}

/// Whether a target standing at `feet` came down where a push was
/// predicted to put it: within its body's `width` of `at`.
pub(super) fn landed_as_predicted(feet: Vec3, at: Vec3, width: f32) -> bool {
    flat(feet - at).length() <= width
}

impl Session {
    /// What `bot` pushing `target` along `direction` with a push of
    /// `push` (along, up) is worth, in the target's health, and where it is
    /// predicted to land. The flight spends `allowance` (motor ticks and
    /// colliders, a share of the shot chooser's budget); `None` for the
    /// landing when it does not come down within it.
    pub(super) fn bot_shove(
        &self,
        bot: OwnerId,
        target: OwnerId,
        direction: Vec3,
        push: (f32, f32),
        allowance: &mut u32,
    ) -> (f32, Option<Landing>) {
        let Some(peer) = self.peers.get(&target).filter(|p| p.combat.alive) else {
            return (0.0, None);
        };
        let state = peer.player.state();
        let feet = Vec3::from(state.feet);
        let health = peer.combat.health;
        // `Player::AddVelocity(impulse / mass)`, the impulse along the shot
        // and up (`weapons` runtime, `Event::Impulse`).
        let velocity = Vec3::from(state.velocity)
            + (direction.normalize_or_zero() * push.0 + Vec3::Y * push.1)
                / crate::session::combat::PLAYER_MASS;
        // The tuning a player keeps has its scale in it already.
        let tuning = peer.player.tuning().clone();
        let simulation = &self.simulation;
        let terrain = |o: Vec3, d: Vec3, r: f32| simulation.terrain_ray(o, d, r);
        let waters = simulation.liquids();
        let ground = Ground {
            physics: &simulation.physics,
            terrain: &terrain,
            passages: simulation.passages(),
            waters: &waters,
            bodies: &[],
            motions: &[],
        };
        let landed = crate::reach::shove_landing(&tuning, feet, velocity, &ground, allowance);
        let fall = landed.map_or(0.0, |(_, impact)| self.fall_harm(target, impact));
        let harm = worth(fall, contest::shove_worth(self, bot, target), health);
        let landing = landed.map(|(at, _)| {
            let across = Vec3::new(velocity.x, 0.0, velocity.z).length();
            Landing {
                at,
                seconds: if across > 0.0 {
                    flat(at - feet).length() / across
                } else {
                    0.0
                },
            }
        });
        (harm, landing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A target standing at the +x edge of a deck (top at y = 0, ending at
    /// x = 1), above a floor `drop` below it (none for `None`).
    fn deck(drop: Option<f32>) -> (Session, OwnerId, OwnerId) {
        deck_armed(drop, false)
    }

    /// [`deck`], with the test weapons loaded when `armed`.
    fn deck_armed(drop: Option<f32>, armed: bool) -> (Session, OwnerId, OwnerId) {
        use rapier3d::prelude::*;
        let world = bri_world::World::new("Shove".into(), "fixture".into(), vec![[1.0; 4]]);
        let mut colliders = vec![
            ColliderBuilder::cuboid(10.0, 0.5, 10.0).translation(Vector::new(-9.0, -0.5, 0.0)),
        ];
        if let Some(drop) = drop {
            colliders.push(
                ColliderBuilder::cuboid(400.0, 0.5, 400.0).translation(Vector::new(
                    0.0,
                    -drop - 0.5,
                    0.0,
                )),
            );
        }
        let sim =
            crate::simulation::Simulation::new(world, crate::testing::definitions(), colliders)
                .unwrap();
        let mut s = Session::new(sim);
        if armed {
            s.set_weapon_pack(bri_weapons::testing::pack()).unwrap();
        }
        let bot = s
            .join("Shover".into(), Vec3::new(-2.0, 0.05, 0.0), false)
            .unwrap();
        let target = s
            .join("Target".into(), Vec3::new(0.3, 0.05, 0.0), false)
            .unwrap();
        (s, bot, target)
    }

    /// A shove along `direction` that changes the target's speed by 8
    /// units a second along it and 4 up (a body pushed flat stays on its
    /// feet: the landing reads it as down where it stands), and what it is
    /// worth.
    fn shove(
        s: &Session,
        bot: OwnerId,
        target: OwnerId,
        direction: Vec3,
    ) -> (f32, Option<Landing>) {
        let push = (
            8.0 * crate::session::combat::PLAYER_MASS,
            4.0 * crate::session::combat::PLAYER_MASS,
        );
        let mut allowance = 4096;
        s.bot_shove(bot, target, direction, push, &mut allowance)
    }

    #[test]
    fn a_shove_off_a_drop_that_hurts_is_worth_the_harm_and_on_the_flat_nothing() {
        let (s, bot, target) = deck(Some(30.0));
        let (over, landing) = shove(&s, bot, target, Vec3::X);
        assert!(over > 0.0, "off the edge, onto the floor far below: {over}");
        assert!(landing.is_some_and(|l| l.at.y < -20.0), "{landing:?}");
        let (flat, landing) = shove(&s, bot, target, Vec3::NEG_X);
        assert_eq!(flat, 0.0, "along the deck");
        assert!(landing.is_some_and(|l| l.at.y > -1.0), "{landing:?}");
    }

    #[test]
    fn a_shove_is_worth_nothing_where_falls_do_not_hurt_or_it_never_lands() {
        let (mut s, bot, target) = deck(Some(30.0));
        s.admin.settings.falling_damage = false;
        assert_eq!(shove(&s, bot, target, Vec3::X).0, 0.0, "falling damage off");
        let (s, bot, target) = deck(None);
        assert_eq!(shove(&s, bot, target, Vec3::X), (0.0, None), "into nothing");
    }

    #[test]
    fn a_scaled_target_is_flown_at_its_own_size_once() {
        let (mut s, bot, target) = deck(Some(30.0));
        s.set_player_scale(target, 2.0).unwrap();
        let (_, landing) = shove(&s, bot, target, Vec3::X);
        // The same shove flown with the tuning the player keeps, which
        // has its scale in it already.
        let peer = &s.peers[&target];
        let velocity = Vec3::from(peer.player.state().velocity) + Vec3::new(8.0, 4.0, 0.0);
        let simulation = &s.simulation;
        let terrain = |o: Vec3, d: Vec3, r: f32| simulation.terrain_ray(o, d, r);
        let waters = simulation.liquids();
        let ground = Ground {
            physics: &simulation.physics,
            terrain: &terrain,
            passages: simulation.passages(),
            waters: &waters,
            bodies: &[],
            motions: &[],
        };
        let mut allowance = 4096;
        let flown = crate::reach::shove_landing(
            peer.player.tuning(),
            Vec3::from(peer.player.state().feet),
            velocity,
            &ground,
            &mut allowance,
        );
        assert_eq!(landing.map(|l| l.at), flown.map(|(at, _)| at));
    }

    #[test]
    fn a_contested_shove_is_worth_the_larger_never_the_sum() {
        assert_eq!(
            worth(0.0, contest::CONTEST_SHOVE_WORTH, 100.0),
            contest::CONTEST_SHOVE_WORTH
        );
        assert_eq!(worth(40.0, contest::CONTEST_SHOVE_WORTH, 100.0), 40.0);
        assert_eq!(worth(140.0, 0.0, 100.0), 100.0, "at most its health");
        assert_eq!(worth(0.0, 0.0, 100.0), 0.0, "no contest, no harm");
    }

    #[test]
    fn a_push_works_when_its_target_comes_down_where_predicted() {
        let at = Vec3::new(5.0, -30.0, 0.0);
        assert!(landed_as_predicted(Vec3::new(5.5, -30.0, 0.3), at, 1.25));
        assert!(!landed_as_predicted(Vec3::new(1.0, 0.0, 0.0), at, 1.25));
    }

    /// A push-only weapon (a push broom: no damage of its own) at an enemy
    /// on the edge of a deck above a drop that hurts: the push's landing is
    /// its worth, so it is planned and the fire gate lets it go.
    #[test]
    fn a_push_only_weapon_at_an_enemy_by_a_drop_is_worth_firing_and_fires() {
        let (mut s, bot, target) = deck_armed(Some(24.0), true);
        super::super::harm::one_game(&mut s, bot, target);
        let slot = s.give_item(bot, bri_weapons::testing::BROOM_ITEM).unwrap();
        s.equip_tool(bot, Some(slot)).unwrap();
        let mut kind = crate::bot_kind::BotKind {
            id: "pusher".into(),
            ..Default::default()
        };
        kind.surprise.strength = 0.0;
        s.bots
            .brains
            .insert(bot, Brain::new(None, kind, Vec3::ZERO, bot, 0));
        let p = &s.peers[&target].player;
        let (eye, feet) = (p.eye(), Vec3::from(p.state().feet));
        let from = s.peers[&bot].player.eye();
        let seen = Seen {
            owner: target,
            eye,
            feet,
            aim: sightlines::aim_point(eye, p.state().scale),
            real: feet,
            way: bri_content::passage::Way {
                aim: eye,
                carry: None,
                length: from.distance(eye),
            },
        };
        let mut budget = hand_combat::Budget::default();
        budget.begin_tick(10);
        let mut state = hand_combat::State::default();
        let mut mind = s.bots.brains[&bot].surprise.clone();
        let decision =
            hand_combat::choose(&s, bot, seen, from, 10, &mut state, &mut budget, &mut mind);
        let hand_combat::Decision::Ready(choice) = decision else {
            panic!("a push off the deck is planned");
        };
        assert_eq!(choice.capability.direct_damage, 0.0, "it only pushes");
        assert!(
            choice.harm.push > 0.0 && choice.dealt > 0.0,
            "{:?}",
            choice.harm
        );
        assert!(hand_combat::validate_fire(
            &s,
            bot,
            seen,
            choice,
            choice.direction,
            &mut budget
        ));
    }
}
