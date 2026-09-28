//! Found by the network soak: once bricks covered every spawn point, the
//! host refused every join ("Player spawn is obstructed") and a map change
//! failed. v20 places the new body whatever is there, as respawns here do.
use bri_chaos::{fixture, local::check_replicated};
use bri_sim::session::{Command, Reply};
use bri_world::{Brick, ContentRef, World, build::SavedBuild};
use glam::Vec3;

#[test]
fn players_still_join_when_every_spawn_point_is_built_over() {
    let fixture = fixture::synthetic().unwrap();
    let spawns = fixture.spawn_points.clone();
    let mut session = fixture.session;
    let admin = session
        .join("Admin".into(), Vec3::new(20.0, 0.05, 20.0), true)
        .unwrap();
    // Four baseplates over the whole spawn grid (the last point is inside
    // the pillar already).
    let mut world = World::new("Cover".into(), "chaos/map".into(), vec![[1.0; 4]]);
    for (i, (x, z)) in [(-4.0, -4.0), (4.0, -4.0), (-4.0, 4.0), (4.0, 4.0)]
        .into_iter()
        .enumerate()
    {
        world.bricks.insert(
            i as u64 + 1,
            Brick::new(
                ContentRef::Resolved(fixture::BASEPLATE.into()),
                [x, 0.1, z],
                1,
            ),
        );
    }
    world.next_brick_id = 5;
    let reply = session
        .command(
            admin,
            1,
            Command::LoadBuild {
                build: Box::new(SavedBuild::new(world)),
                ownership: false,
            },
        )
        .unwrap();
    assert!(matches!(reply, Reply::Loaded { .. }), "{reply:?}");
    session.step().unwrap();
    let tuning = bri_sim::player::PlayerTuning::default();
    for spawn in &spawns {
        assert!(
            !bri_sim::player::Player::clear(&session.simulation().physics, *spawn, &tuning),
            "spawn {spawn} is still clear"
        );
    }
    for n in 0..3 {
        session.join(format!("Late{n}"), spawns[n], false).unwrap();
    }
    for _ in 0..120 {
        session.step().unwrap();
    }
    check_replicated(&mut session).unwrap();
}
