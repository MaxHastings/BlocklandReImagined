use super::*;
use bri_package_runtime::ops::BotIntent;

#[test]
fn a_follower_does_not_arrive_one_floor_above_its_leader() {
    let sim = crate::simulation::Simulation::new(
        bri_world::World::new("Two floors".into(), "fixture".into(), vec![[1.; 4]]),
        crate::testing::definitions(),
        vec![
            rapier3d::prelude::ColliderBuilder::cuboid(24., 0.5, 24.)
                .translation(Vec3::new(0., -0.5, 0.)),
            rapier3d::prelude::ColliderBuilder::cuboid(8., 0.5, 8.)
                .translation(Vec3::new(0., 7.5, 0.)),
        ],
    )
    .unwrap();
    let mut session = Session::new(sim);
    let below = Vec3::new(0., 0.05, 0.);
    let above = Vec3::new(0., 8.05, 0.);
    let leader = session.join("Leader below".into(), below, false).unwrap();
    let bot = session.join("Follower above".into(), above, false).unwrap();
    session
        .command(
            bot,
            1,
            Command::MiniGame(super::super::MiniGameRequest::Create {
                color: 0,
                settings: bri_minigames::Settings::default(),
            }),
        )
        .unwrap();
    let game = session.game_of(bot).unwrap();
    let player = session.peers[&bot].combat.player;
    let effects = session
        .minigames
        .execute(bri_minigames::Command::ForceRespawn { target: player })
        .unwrap();
    session.apply_minigame_effects(effects).unwrap();
    // Creating a mini-game respawns its owner. Establish the two-floor
    // fixture after that real lifecycle transition, rather than measuring
    // the spawn the mini-game correctly chose for its owner.
    session
        .peers
        .get_mut(&bot)
        .unwrap()
        .player
        .teleport(&mut session.simulation.physics, above, 0.)
        .unwrap();
    let follower_kind = BotKind {
        weapons: false,
        hold_seconds: 0.,
        ..BotKind::default()
    };
    session
        .bots
        .brains
        .insert(bot, Brain::new(None, follower_kind, above, bot, 0));
    session
        .bots
        .by_rules
        .insert(bot, ("fixture".into(), game.0));
    session
        .set_bot_intent(
            "fixture",
            bot,
            Some(BotIntent::Follow {
                leader,
                distance: 1.,
            }),
        )
        .unwrap();
    assert!(session.peers[&bot].player.state().feet[1] > 7.);
    assert!(session.peers[&leader].player.state().feet[1] < 1.);
    assert!(session.is_alive(bot) && session.is_alive(leader));
    let mut requested_lower_floor = false;
    for _ in 0..24 {
        session.step().unwrap();
        let brain = &session.bots.brains[&bot];
        // A genuinely unreachable synthetic platform can exhaust its route
        // inside this same tick. Observe the decision, rather than requiring
        // a transient Home goal to survive route execution.
        requested_lower_floor |= brain.behaviour == Behaviour::Return;
    }
    assert!(
        requested_lower_floor,
        "the follower treated horizontal proximity through a floor as arrival: {:?}, home {:?}, body {:?}, failures {:?}",
        session.bots.brains[&bot].behaviour,
        session.bots.brains[&bot].home,
        session.peers[&bot].player.state().feet,
        (session.bot_step_failures(), session.bot_thoughts())
    );
    assert_eq!(session.bot_step_failures().0, 0);
    // The leader comes onto the follower's reachable floor. An old route
    // failure or a search to the previous anchor must not freeze the follower.
    let now_reachable = above + Vec3::X * 4.;
    session
        .peers
        .get_mut(&leader)
        .unwrap()
        .player
        .teleport(&mut session.simulation.physics, now_reachable, 0.)
        .unwrap();
    for _ in 0..600 {
        session.step().unwrap();
    }
    let actual = Vec3::from(session.peers[&bot].player.state().feet);
    assert!(
        actual.distance(now_reachable) < 2.,
        "follower did not recover when leader moved to reachable floor: {actual:?}, {:?}, {:?}",
        session.bots.brains[&bot].goal,
        session.bots.brains[&bot].home_given_up
    );
}

#[test]
fn a_patrol_does_not_skip_a_point_on_another_floor() {
    let mut brain = Brain::new(None, BotKind::default(), Vec3::ZERO, 1, 0);
    brain.intent = Some(intents::Held {
        intent: BotIntent::Patrol {
            points: vec![[0., 8., 0.], [8., 8., 0.]],
            radius: 4.,
        },
        next: 0,
        until: u64::MAX,
    });
    assert_eq!(brain.patrol_point(Vec3::ZERO), Some(Vec3::new(0., 8., 0.)));
    assert_eq!(
        brain.patrol_point(Vec3::new(0., 8., 0.)),
        Some(Vec3::new(8., 8., 0.))
    );
}
