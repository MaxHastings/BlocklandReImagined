//! Native drop motion: low airborne speed is not evidence of support.
use bri_weapons::*;
use glam::{Quat, Vec3};
use std::collections::BTreeMap;

#[derive(Default)]
struct Scene {
    floor: Option<f32>,
    coverage: Option<f32>,
}
impl Scene {
    fn floor_hit(&self, start: Vec3, end: Vec3, extent: f32) -> Option<Hit> {
        let floor = self.floor? + extent;
        if start.y >= floor && end.y < floor {
            let fraction = (start.y - floor) / (start.y - end.y);
            Some(Hit {
                target: TargetId::Map(1),
                position: start.lerp(end, fraction),
                normal: Vec3::Y,
                fraction,
                color: None,
            })
        } else {
            None
        }
    }
}
impl Query for Scene {
    fn sweep(&mut self, start: Vec3, end: Vec3, filter: Filter) -> Option<Hit> {
        assert!(filter.world_only && !filter.players);
        self.floor_hit(start, end, 0.)
    }
    fn sweep_box(
        &mut self,
        start: Vec3,
        end: Vec3,
        half: Vec3,
        rotation: Quat,
        filter: Filter,
    ) -> Option<Hit> {
        assert!(filter.world_only && !filter.players);
        self.floor_hit(start, end, ItemBounds::lowest(half, rotation))
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        vec![]
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        true
    }
    fn liquid(&mut self, _: Vec3, _: f32) -> Option<Liquid> {
        self.coverage.map(|coverage| Liquid {
            coverage,
            density: 1.,
            viscosity: 0.,
        })
    }
}
fn world(boxed: bool) -> WeaponsWorld {
    let mut world = WeaponsWorld::new(testing::pack()).unwrap();
    if boxed {
        world.set_item_bounds(BTreeMap::from([(
            CORE_TOOLS[0].into(),
            ItemBounds::FALLBACK,
        )]));
    }
    world
}

#[test]
fn ordinary_upward_tool_throw_descends_after_apex() {
    let mut w = world(true);
    let actor = ActorId(1);
    w.add_actor(actor, 5).unwrap();
    w.give(actor, CORE_TOOLS[0]).unwrap();
    w.set_frame(
        actor,
        Frame {
            direction: Vec3::Y,
            ..Default::default()
        },
    )
    .unwrap();
    w.drop_item(actor, 0).unwrap();
    let mut peak = f32::NEG_INFINITY;
    for _ in 0..180 {
        w.step(&mut Scene::default());
        peak = peak.max(w.drops().next().unwrap().position.y);
    }
    let drop = w.drops().next().unwrap();
    assert!(drop.velocity.y < -9., "airborne apex: {drop:?}");
    assert!(drop.position.y < peak - 2., "must descend: {drop:?}");
}

#[test]
fn exact_and_near_zero_airborne_drops_receive_gravity_with_or_without_bounds() {
    for boxed in [false, true] {
        for speed in [0., -0.0001, 0.0001] {
            let mut w = world(boxed);
            w.spawn_drop(CORE_TOOLS[0], Vec3::Y * 10., Vec3::Y * speed)
                .unwrap();
            w.step(&mut Scene::default());
            let drop = w.drops().next().unwrap();
            assert!(drop.velocity.y < -0.16, "{boxed}/{speed}: {drop:?}");
            assert!(drop.position.y < 10.);
        }
    }
}

#[test]
fn supported_rest_is_stable_and_support_removal_resumes_falling() {
    for boxed in [false, true] {
        let mut w = world(boxed);
        let position = Vec3::Y * (if boxed { 0.25 } else { 0. } + 0.002);
        w.spawn_drop(CORE_TOOLS[0], position, Vec3::ZERO).unwrap();
        let mut scene = Scene {
            floor: Some(0.),
            ..Default::default()
        };
        for _ in 0..120 {
            w.step(&mut scene);
            let drop = w.drops().next().unwrap();
            assert_eq!(drop.position, position);
            assert_eq!(drop.velocity, Vec3::ZERO);
        }
        scene.floor = None;
        for _ in 0..60 {
            w.step(&mut scene);
        }
        let drop = w.drops().next().unwrap();
        assert!(drop.position.y < position.y - 2.);
        assert!(drop.velocity.y < -9.);
    }
}

#[test]
fn buoyancy_can_lift_a_stationary_item_off_support() {
    let mut w = world(true);
    let position = Vec3::Y * 0.252;
    w.spawn_drop(CORE_TOOLS[0], position, Vec3::ZERO).unwrap();
    let mut scene = Scene {
        floor: Some(0.),
        coverage: Some(1.),
    };
    w.step(&mut scene);
    let drop = w.drops().next().unwrap();
    assert!(drop.velocity.y > 0.6);
    assert!(drop.position.y > position.y);
}

#[test]
fn moving_drop_keeps_bounce_friction_pickup_and_timeout() {
    let mut w = world(true);
    w.spawn_drop(
        CORE_TOOLS[0],
        Vec3::new(0., 0.3, 0.),
        Vec3::new(10., -10., 0.),
    )
    .unwrap();
    w.step(&mut Scene {
        floor: Some(0.),
        ..Default::default()
    });
    let drop = w.drops().next().unwrap();
    assert!((drop.velocity.x - 0.8).abs() < 1e-5);
    assert!((drop.velocity.y - (10. + 20. / 120.) * 0.2).abs() < 1e-5);
    assert!((drop.position.y - 0.252).abs() < 1e-5);
    let id = drop.id;
    let mut floor = Scene {
        floor: Some(0.),
        ..Default::default()
    };
    for _ in 0..180 {
        w.step(&mut floor);
    }
    let resting = w.drops().next().unwrap().position;
    assert_eq!(w.drops().next().unwrap().velocity, Vec3::ZERO);
    for _ in 0..30 {
        w.step(&mut floor);
        assert_eq!(w.drops().next().unwrap().position, resting);
    }
    let actor = ActorId(8);
    w.add_actor(actor, 5).unwrap();
    assert_eq!(w.pickup(actor, id).unwrap(), 0);
    assert!(w.drops().next().is_none());
    w.spawn_drop(CORE_TOOLS[0], Vec3::Y * 10., Vec3::ZERO)
        .unwrap();
    for _ in 0..1200 {
        w.step(&mut Scene::default());
    }
    assert!(w.drops().next().is_none());
}

#[test]
fn a_lingering_item_stays_where_it_was_dropped_and_survives_a_save() {
    let mut pack = testing::pack();
    let item = pack
        .items
        .values()
        .find(|i| i.can_drop)
        .map(|i| i.id.clone())
        .unwrap();
    pack.items.get_mut(&item).unwrap().lingers = true;
    let mut w = WeaponsWorld::new(pack.clone()).unwrap();
    let actor = ActorId(1);
    w.add_actor(actor, 5).unwrap();
    w.give(actor, &item).unwrap();
    let lingering = w.drop_item(actor, 0).unwrap();
    let mut ground = Scene {
        floor: Some(-100.),
        ..Default::default()
    };
    for _ in 0..runtime::MAX_DROP_TICKS + 10 {
        w.step(&mut ground);
    }
    assert!(w.drops().any(|d| d.id == lingering), "it pops like v20's");
    let bytes = serde_json::to_vec(&w.save()).unwrap();
    let mut restored = WeaponsWorld::restore(pack, &bytes).unwrap();
    assert!(restored.drops().any(|d| d.id == lingering));
    restored.pickup(actor, lingering).unwrap();
}
