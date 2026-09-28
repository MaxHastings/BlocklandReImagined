//! Glass map shapes: a player hitting one faster than `minImpactSpeed`
//! smashes it (v20 `Armor::onImpact` -> `StaticShape::explode`).
use bri_sim::{
    definitions::Definitions, map::Breakable, presentation::CueKind, session::Session,
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;

/// Ground at y = 0 and a horizontal glass pane (collider 1) at y = 10.
fn session(indestructable: bool) -> Session {
    let mut s = Session::new(
        Simulation::new(
            World::new("Glass".into(), "test".into(), vec![[1.0; 4]; 2]),
            Definitions {
                entries: Default::default(),
            },
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
                ColliderBuilder::cuboid(20.0, 0.1, 20.0).translation(Vector::new(0.0, 10.0, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s.set_breakables(vec![Breakable {
        node: 7,
        datablock: "glassA".into(),
        explosion: Some("glassExplosion".into()),
        sound: Some("glassExplosionSound".into()),
        position: Vec3::new(0.0, 10.0, 0.0),
        center: Vec3::new(0.0, 10.5, 0.0),
        indestructable,
        colliders: 1..2,
    }])
    .unwrap();
    s
}

fn feet(s: &Session) -> Vec3 {
    Vec3::from(s.snapshot().players[0].feet)
}

/// Drop a player from `height` above the pane and run until it lands.
fn drop_onto_pane(s: &mut Session, height: f32) -> Vec<CueKind> {
    s.join("Faller".into(), Vec3::new(0.0, 10.1 + height, 0.0), true)
        .unwrap();
    let mut cues = Vec::new();
    for _ in 0..600 {
        s.step().unwrap();
        cues.extend(s.take_cues().into_iter().map(|c| c.kind));
        if s.snapshot().players[0].grounded {
            break;
        }
    }
    cues
}

#[test]
fn a_fast_fall_smashes_glass_and_the_player_drops_through() {
    let mut s = session(false);
    let cues = drop_onto_pane(&mut s, 60.0);
    // The impact itself still stops the player on the pane.
    assert!((feet(&s).y - 10.1).abs() < 0.2, "{}", feet(&s));
    assert_eq!(s.broken_shapes().into_iter().collect::<Vec<_>>(), [7]);
    assert!(cues.iter().any(|c| matches!(c,
        CueKind::WeaponEffect { definition, .. } if definition == "glassExplosion")));
    assert!(cues.iter().any(|c| matches!(c,
        CueKind::WeaponSound { profile } if profile == "glassExplosionSound")));
    // `schedule(100, setHidden, 1)`: then it no longer holds anyone up.
    for _ in 0..240 {
        s.step().unwrap();
    }
    assert!(feet(&s).y < 0.5, "{}", feet(&s));
    assert_eq!(s.broken_shapes().len(), 1, "it never repairs itself");
}

#[test]
fn a_slow_landing_does_not_break_glass() {
    let mut s = session(false);
    let cues = drop_onto_pane(&mut s, 3.0);
    for _ in 0..240 {
        s.step().unwrap();
    }
    assert!(s.broken_shapes().is_empty());
    assert!((feet(&s).y - 10.1).abs() < 0.2, "{}", feet(&s));
    assert!(
        !cues
            .iter()
            .any(|c| matches!(c, CueKind::WeaponSound { .. }))
    );
}

#[test]
fn indestructable_glass_holds() {
    let mut s = session(true);
    drop_onto_pane(&mut s, 60.0);
    for _ in 0..240 {
        s.step().unwrap();
    }
    assert!(s.broken_shapes().is_empty());
    assert!((feet(&s).y - 10.1).abs() < 0.2, "{}", feet(&s));
}

#[test]
fn client_mirrors_stop_colliding_with_smashed_shapes() {
    use bri_sim::prediction::BrokenShapes;
    let mut physics = bri_physics::new_world();
    let handles = vec![
        physics.insert_collider(ColliderBuilder::cuboid(1.0, 1.0, 1.0), None),
        physics.insert_collider(
            ColliderBuilder::cuboid(1.0, 1.0, 1.0).translation(Vector::new(0.0, 5.0, 0.0)),
            None,
        ),
    ];
    physics.detect_collisions(&(), &());
    let shape = Breakable {
        node: 3,
        datablock: "lightBulbA".into(),
        explosion: None,
        sound: None,
        position: Vec3::ZERO,
        center: Vec3::ZERO,
        indestructable: false,
        colliders: 1..2,
    };
    let mut broken = BrokenShapes::new(handles, &[shape]);
    let down = |physics: &PhysicsWorld| {
        physics
            .query_pipeline_with_filter(QueryFilter::default())
            .cast_ray(
                &Ray::new(Vector::new(0.0, 10.0, 0.0), Vector::NEG_Y),
                20.0,
                true,
            )
            .map(|(_, t)| t)
    };
    assert_eq!(down(&physics), Some(4.0));
    assert!(broken.apply(&mut physics, &[3].into()).unwrap());
    assert_eq!(down(&physics), Some(9.0), "the shape no longer blocks rays");
    assert!(!broken.apply(&mut physics, &[3].into()).unwrap());
    assert!(broken.apply(&mut physics, &Default::default()).unwrap());
    assert_eq!(down(&physics), Some(4.0));
}
