use bri_sim::{
    definitions::Definitions,
    player::{Player, PlayerTuning},
    simulation::Simulation,
    weapon_query::WeaponQuery,
};
use bri_weapons::{ActorId, Filter, Query, TargetId};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;

#[test]
fn swept_queries_share_player_map_collision_and_radius_occlusion() {
    let mut sim = Simulation::new(
        World::new("Query".into(), "fixture".into(), vec![[1.; 4]]),
        Definitions::default(),
        vec![
            ColliderBuilder::cuboid(10., 0.5, 10.).translation(Vector::new(0., -0.5, 0.)),
            ColliderBuilder::cuboid(10., 4., 0.005).translation(Vector::new(0., 2., -4.)),
        ],
    )
    .unwrap();
    let shooter = Player::spawn(
        &mut sim.physics,
        1,
        Vec3::new(0., 0.05, 0.),
        PlayerTuning::default(),
    )
    .unwrap();
    let target = Player::spawn(
        &mut sim.physics,
        2,
        Vec3::new(0., 0.05, -2.),
        PlayerTuning::default(),
    )
    .unwrap();
    let _behind = Player::spawn(
        &mut sim.physics,
        3,
        Vec3::new(0., 0.05, -6.),
        PlayerTuning::default(),
    )
    .unwrap();
    let deny = |_: ActorId, _: TargetId| false;
    let no_catch = |_: ActorId, _: ActorId| false;
    let mut q = WeaponQuery {
        simulation: &sim,
        affect: &deny,
        affect_radius: &deny,
        catch: &no_catch,
        responses: &Default::default(),
        truncated_targets: 0,
    };
    let filter = Filter {
        source: ActorId(1),
        projectile_age_ticks: Some(1),
        players: true,
        world_only: false,
    };
    let eye = shooter.eye();
    let hit = q.sweep(eye, eye + Vec3::NEG_Z * 10., filter).unwrap();
    assert_eq!(hit.target, TargetId::Actor(ActorId(2)));
    let map = q
        .sweep(
            eye,
            eye + Vec3::NEG_Z * 10.,
            Filter {
                players: false,
                ..filter
            },
        )
        .unwrap();
    assert_eq!(map.target, TargetId::Map(0));
    assert!((map.position.z + 3.995).abs() < 0.001);
    // Returning from outside the source shape may hit its owner.
    let returning = q
        .sweep(eye + Vec3::X * 2., eye - Vec3::X * 2., filter)
        .unwrap();
    assert_eq!(returning.target, TargetId::Actor(ActorId(1)));
    let targets = q.radius(eye, 10., 128);
    let near = targets
        .iter()
        .find(|n| n.target == TargetId::Actor(ActorId(2)))
        .unwrap();
    let far = targets
        .iter()
        .find(|n| n.target == TargetId::Actor(ActorId(3)))
        .unwrap();
    assert!(near.distance < (target.eye() - eye).length()); // closest bounds, not center
    assert!(q.visible(eye, near));
    assert!(!q.visible(eye, far));
    // An explosion on the near wall surface is visible to the near actor, but
    // the same thin wall still shields an actor on its far side.
    let impact = Vec3::new(0., eye.y, -3.995);
    assert!(q.visible(impact, near));
    assert!(!q.visible(impact, far));
    assert!(!q.can_affect(ActorId(1), near.target));
    assert_eq!(q.radius(eye, 10., 1).len(), 1);
    assert_eq!(q.truncated_targets, 2);
}
