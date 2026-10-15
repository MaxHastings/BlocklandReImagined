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
        ally: &deny,
        catch: &no_catch,
        responses: &Default::default(),
        truncated_targets: 0,
        shapes: &[],
        rewound: None,
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

#[test]
fn shots_stop_at_script_moved_shapes_in_front_of_the_map() {
    use bri_sim::weapon_query::ShapeTarget;
    let sim = Simulation::new(
        World::new("Query".into(), "fixture".into(), vec![[1.; 4]]),
        Definitions::default(),
        vec![ColliderBuilder::cuboid(10., 4., 0.005).translation(Vector::new(0., 2., -8.))],
    )
    .unwrap();
    let deny = |_: ActorId, _: TargetId| false;
    let no_catch = |_: ActorId, _: ActorId| false;
    // A board a quarter turn about up, two units ahead of the shot.
    let board = ShapeTarget {
        id: 7,
        parts: vec![SharedShape::cuboid(0.1, 1., 1.)],
        rotation: glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        translation: Vec3::new(0., 1., -2.),
    };
    let shapes = [board];
    let responses = std::collections::BTreeMap::new();
    let q = |shapes| WeaponQuery {
        simulation: &sim,
        affect: &deny,
        affect_radius: &deny,
        ally: &deny,
        catch: &no_catch,
        responses: &responses,
        truncated_targets: 0,
        shapes,
        rewound: None,
    };
    let filter = Filter {
        source: ActorId(1),
        projectile_age_ticks: Some(1),
        players: true,
        world_only: false,
    };
    let from = Vec3::new(0., 1., 0.);
    let hit = q(&shapes)
        .sweep(from, from + Vec3::NEG_Z * 20., filter)
        .unwrap();
    assert_eq!(hit.target, TargetId::Shape(7));
    assert!((hit.position.z + 1.9).abs() < 0.001, "{:?}", hit.position);
    assert!(hit.normal.z > 0.99);
    // Beside it, or for world-only probes, the wall behind is what is hit.
    let past = q(&shapes)
        .sweep(
            from + Vec3::X * 2.,
            from + Vec3::X * 2. + Vec3::NEG_Z * 20.,
            filter,
        )
        .unwrap();
    assert_eq!(past.target, TargetId::Map(0));
    let probe = q(&shapes)
        .sweep(
            from,
            from + Vec3::NEG_Z * 20.,
            Filter {
                world_only: true,
                ..filter
            },
        )
        .unwrap();
    assert_eq!(probe.target, TargetId::Map(0));
}

/// Lag compensation: an aim ray or hitscan shot meets another body where
/// the shooter saw it (`bri_sim::rewind`); a flying projectile meets it
/// where it is.
#[test]
fn an_aimed_shot_meets_a_body_where_its_shooter_saw_it() {
    let mut sim = Simulation::new(
        World::new("Rewind".into(), "fixture".into(), vec![[1.; 4]]),
        Definitions::default(),
        vec![ColliderBuilder::cuboid(20., 0.5, 20.).translation(Vector::new(0., -0.5, 0.))],
    )
    .unwrap();
    let shooter = Player::spawn(
        &mut sim.physics,
        1,
        Vec3::new(0., 0.05, 0.),
        PlayerTuning::default(),
    )
    .unwrap();
    let runner = Player::spawn(
        &mut sim.physics,
        2,
        Vec3::new(0., 0.05, -8.),
        PlayerTuning::default(),
    )
    .unwrap();
    // The runner was 3 units to the right ten ticks ago, as the shooter's
    // screen still shows it.
    let mut history = bri_sim::rewind::History::default();
    history.record(90, [(2, Vec3::new(3., 0.05, -8.))]);
    history.record(100, [(2, Vec3::new(0., 0.05, -8.))]);
    let bodies = [(2, runner.collider(), Vec3::new(0., 0.05, -8.))];
    let rewound = history.rewound(100, [(1, 90)], &bodies);
    let deny = |_: ActorId, _: TargetId| false;
    let no_catch = |_: ActorId, _: ActorId| false;
    let responses = Default::default();
    let mut q = WeaponQuery {
        simulation: &sim,
        affect: &deny,
        affect_radius: &deny,
        ally: &deny,
        catch: &no_catch,
        responses: &responses,
        truncated_targets: 0,
        shapes: &[],
        rewound: Some(&rewound),
    };
    let aimed = Filter {
        source: ActorId(1),
        projectile_age_ticks: None,
        players: true,
        world_only: false,
    };
    let eye = shooter.eye();
    let seen = Vec3::new(3., eye.y, -8.);
    let at_seen = q.sweep(eye, eye + (seen - eye).normalize() * 20., aimed);
    assert_eq!(at_seen.map(|h| h.target), Some(TargetId::Actor(ActorId(2))));
    let at_now = q.sweep(eye, eye + Vec3::NEG_Z * 20., aimed);
    assert_ne!(at_now.map(|h| h.target), Some(TargetId::Actor(ActorId(2))));
    // A flying projectile lives in the host's present.
    let flying = Filter {
        projectile_age_ticks: Some(3),
        ..aimed
    };
    let now = q.sweep(eye, eye + Vec3::NEG_Z * 20., flying);
    assert_eq!(now.map(|h| h.target), Some(TargetId::Actor(ActorId(2))));
    // Another shooter who saw the present meets it where it is.
    let other = Filter {
        source: ActorId(3),
        ..aimed
    };
    // (From just past the first shooter's own body.)
    let ahead = eye + Vec3::NEG_Z * 1.5;
    let now = q.sweep(ahead, ahead + Vec3::NEG_Z * 20., other);
    assert_eq!(now.map(|h| h.target), Some(TargetId::Actor(ActorId(2))));
}
