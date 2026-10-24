//! A closing door stopped by a body must still reach its package's passage
//! policy. Physical motion/collision is real; the invented policy reverses a
//! blocked closing door and never toggles one already opening.
use super::*;
use bri_content::mover::{Glide, MoverDef, MoverKey};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use rapier3d::prelude::*;
use serde_json::json;

fn passage_session() -> (Session, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let addon = root.path().join("passage");
    std::fs::create_dir(&addon).unwrap();
    for (file, data) in [
        (
            "package.json",
            json!({"schema_version":1,"id":"passage","version":"1.0.0","api":1,
            "name":"Invented passage policy","license":"CC0-1.0","capabilities":["movers"],
            "provides":[{"kind":"behaviour","id":"passage:behaviour/main","file":"behaviour.json"},
                {"kind":"script","id":"passage:script/main","file":"main.rhai"}]}),
        ),
        (
            "behaviour.json",
            json!({"schema_version":1,"script":"main.rhai",
            "commands":[{"name":"enter","cooldown_ticks":0}],
            "state":{"global":{"requests":{"default":0,"visible":"everyone"}}}}),
        ),
    ] {
        std::fs::write(addon.join(file), data.to_string()).unwrap();
    }
    std::fs::write(
        addon.join("main.rhai"),
        r#"
        fn cmd_enter(p) {
            set("requests", get("requests") + 1);
            let m = mover("door");
            if !m.moving && (m.key == 0 || (m.blocked && m.to == 0)) {
                mover_goto(m.id, 1, 1.0);
            }
        }
    "#,
    )
    .unwrap();
    let catalog = bri_package_runtime::Catalog::load(
        root.path(),
        &PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "passage".into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: "passage".into(),
                role: None,
            }],
        },
        true,
    )
    .unwrap();
    let sim = Simulation::new(
        bri_world::World::new("Passage".into(), "fixture".into(), vec![[1.; 4]]),
        crate::testing::definitions(),
        vec![
            ColliderBuilder::cuboid(20., 0.5, 20.).translation(Vector::new(0., -0.5, 0.)),
            ColliderBuilder::compound(vec![(
                Pose::translation(1.5, 3., 0.),
                SharedShape::cuboid(1.5, 3., 0.1),
            )]),
        ],
    )
    .unwrap();
    let mut s = Session::new(sim);
    s.install_packages(Arc::new(catalog), None).unwrap();
    s.set_map_movers(vec![MapMover {
        node: 0,
        placement: glam::Mat4::IDENTITY,
        colliders: 1..2,
        def: MoverDef {
            id: "door".into(),
            keys: vec![
                MoverKey::default(),
                MoverKey {
                    translation: [0.; 3],
                    rotation: [0., -std::f32::consts::FRAC_PI_2, 0.],
                },
            ],
            glide: Glide::Smooth,
            encroach: Encroach::Stop,
            spin: None,
            collide: true,
            sounds: None,
            params: BTreeMap::from([("bots_open".into(), "passage:enter".into())]),
        },
    }])
    .unwrap();
    (s, root)
}

#[test]
fn blocked_closing_door_receives_a_passage_request_without_crowd_toggling() {
    let (mut s, _root) = passage_session();
    s.mover_goto("door", 1, 0.1).unwrap();
    for _ in 0..30 {
        s.step().unwrap();
    }
    assert_eq!(s.mover_views()[0].key, Some(1));
    let at = Vec3::new(1.5, 0.05, 0.5);
    s.set_spawn_points(vec![at]).unwrap();
    let first = s.join("First requester".into(), at, false).unwrap();
    let second = s
        .join("Second requester".into(), at + Vec3::Z * 0.3, false)
        .unwrap();
    s.mover_goto("door", 0, 1.).unwrap();
    for _ in 0..150 {
        s.step().unwrap();
    }
    let before = s.mover_views()[0].clone();
    assert!(
        before.blocked && !before.moving && before.key.is_none(),
        "{before:?}"
    );
    assert!(!s.movers_near_player(first, BOT_DOOR_REACH).is_empty());
    let requests = |s: &Session| {
        s.package_state().packages["passage"].global["requests"]
            .as_i64()
            .unwrap()
    };
    for _ in 0..8 {
        let tick = s.simulation.state().tick;
        s.bot_opens_door(first, tick);
        s.bot_opens_door(second, tick);
        s.step().unwrap();
    }
    assert_eq!(
        requests(&s),
        1,
        "blocked passage must request recovery once; a crowd must not stop/reverse its opening"
    );
    for _ in 0..150 {
        s.step().unwrap();
    }
    assert_eq!(
        s.mover_views()[0].key,
        Some(1),
        "passage reopened after physical closing obstruction"
    );
    assert!(s.take_package_problems().is_empty());
}

#[test]
fn a_route_avoids_the_open_leaf_after_cached_closed_door_samples() {
    use crate::nav::{Body, Found, Ground, Nav, Search};
    let (mut s, _root) = passage_session();
    s.simulation.track_collision_changes(true);
    let body = Body::of(&bri_motor::player::PlayerTuning::default(), 1.0);
    let mut nav = Nav::default();
    let from = Vec3::new(-2., 0., 1.5);
    let goal = Vec3::new(2., 0., 1.5);
    for open in [false, true] {
        if open {
            s.mover_goto("door", 1, 0.1).unwrap();
            for _ in 0..30 {
                s.step().unwrap();
            }
        }
        nav.invalidate_all(&s.simulation.take_collision_changes(), &body);
        let simulation = &s.simulation;
        let terrain = |_: Vec3, _: Vec3, _: f32| None;
        let solid =
            |handle: ColliderHandle, _: &Collider| !simulation.bot_passable().contains(&handle);
        let ground = Ground {
            physics: &simulation.physics,
            terrain: &terrain,
            passages: simulation.passages(),
            waters: &[],
            bodies: &[],
            motions: &[],
            solid: &solid,
        };
        let mut search = Search::new(from, goal, 16.);
        let mut found = None;
        for _ in 0..200 {
            nav.begin_tick();
            if let Some(result) = search.step(&mut nav, &ground, &body) {
                found = Some(result);
                break;
            }
        }
        let Some(Found::Path(path)) = found else {
            panic!("complete local route required: {found:?}");
        };
        // Independent physical clearance: the route's own passability filter
        // cannot excuse a leg intersecting the actual open door slab.
        let shape = SharedShape::cuboid(body.width * 0.5, body.height * 0.5, body.width * 0.5);
        let mut previous = from;
        for waypoint in path {
            let n = ((waypoint.feet - previous).length() / 0.1).ceil().max(1.) as u32;
            for i in 0..=n {
                let p = previous.lerp(waypoint.feet, i as f32 / n as f32);
                let pose = Pose::translation(p.x, p.y + body.height * 0.5, p.z);
                let gap = simulation.map_collider_distance(1, &pose, shape.as_ref(), 0.);
                assert!(
                    gap.is_none_or(|g| g >= -0.01),
                    "open={open}: route crosses actual slab at {p:?}, gap {gap:?}"
                );
            }
            previous = waypoint.feet;
        }
    }
}
