//! What a host keeps of itself to come back from a crash or a restart
//! (`Session::recovery_snapshot`), and a restarted host giving the
//! mini-game back to the player who ran it (`Session::hold_minigame`).
use bri_admin::Principal;
use bri_minigames::Settings;
use bri_sim::{
    definitions::Definitions,
    session::{Command, MiniGameRequest, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;

fn session() -> Session {
    let mut s = Session::new(
        Simulation::new(
            World::new("Recovery".into(), "test".into(), vec![[1.0; 4]]),
            Definitions::default(),
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}
const SPAWN: Vec3 = Vec3::new(0.0, 0.05, 0.0);

#[test]
fn a_restarted_host_gives_the_minigame_back_to_whoever_ran_it() {
    let runner = Principal([7; 32]);
    let mut before = session();
    assert_eq!(before.recovery_snapshot().1, None);
    let a = before
        .join_verified("Alpha".into(), SPAWN, false, Some(runner))
        .unwrap();
    before
        .command(
            a,
            1,
            Command::MiniGame(MiniGameRequest::Create {
                color: 3,
                settings: Settings::default(),
            }),
        )
        .unwrap();
    let (_, kept) = before.recovery_snapshot();
    let kept = kept.expect("the running mini-game is kept");
    assert_eq!(kept["owner"], "07".repeat(32));
    assert_eq!(kept["color"], 3);

    let mut after = session();
    after.hold_minigame(kept.clone());
    // Held until its player comes back, and kept by the next save.
    assert_eq!(after.recovery_snapshot().1, Some(kept));
    let other = after
        .join_verified("Bravo".into(), SPAWN, false, Some(Principal([9; 32])))
        .unwrap();
    assert!(after.minigame_views().is_empty());
    let back = after
        .join_verified("Alpha".into(), SPAWN, false, Some(runner))
        .unwrap();
    let games = after.minigame_views();
    assert_eq!(games.len(), 1);
    assert_eq!((games[0].owner, games[0].color), (back, 3));
    assert!(!games[0].members.contains(&other));
}
