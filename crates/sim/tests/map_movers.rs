//! Map movers (doors): a package starts one and the session eases it
//! along its keys with its collision, holds it when a body stands in its
//! way, and tells clients its state so their copies of the world agree.
use bri_content::mover::{Encroach, Glide, MoverDef, MoverKey, MoverState};
use bri_sim::{
    definitions::Definitions, map::MapMover, player::MoveInput, prediction::CollisionMirror,
    session::Session, simulation::Simulation,
};
use bri_world::World;
use glam::{Mat4, Vec3};
use rapier3d::prelude::*;
use std::collections::BTreeMap;

/// A wall slab 3 wide, 6 tall and 0.2 thick hinged at the origin, lying
/// along +X (a door shut across a doorway), swinging 90 degrees to lie
/// along +Z.
fn door_collider() -> ColliderBuilder {
    ColliderBuilder::compound(vec![(
        Pose::translation(1.5, 3.0, 0.0),
        SharedShape::cuboid(1.5, 3.0, 0.1),
    )])
    .position(Pose::IDENTITY)
}

fn door() -> MapMover {
    MapMover {
        node: 5,
        def: MoverDef {
            id: "door".into(),
            keys: vec![
                MoverKey::default(),
                MoverKey {
                    translation: [0.0; 3],
                    rotation: [0.0, -std::f32::consts::FRAC_PI_2, 0.0],
                },
            ],
            glide: Glide::Smooth,
            encroach: Encroach::Stop,
            spin: None,
            collide: true,
            params: BTreeMap::from([("kind".to_string(), "door".to_string())]),
        },
        placement: Mat4::IDENTITY,
        colliders: 1..2,
    }
}

fn session() -> Session {
    let mut s = Session::new(
        Simulation::new(
            World::new("Doors".into(), "test".into(), vec![[1.0; 4]; 2]),
            Definitions {
                entries: Default::default(),
            },
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
                door_collider(),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(1.5, 0.05, 4.0)]).unwrap();
    s.set_map_movers(vec![door()]).unwrap();
    s
}

fn walk(s: &mut Session, owner: u64, sequence: &mut u64, forward: f32, ticks: usize) {
    for _ in 0..ticks {
        *sequence += 1;
        s.movement(
            owner,
            *sequence,
            MoveInput {
                forward,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
    }
}

fn feet(s: &Session) -> Vec3 {
    Vec3::from(s.snapshot().players[0].feet)
}

#[test]
fn a_shut_door_stops_a_player_and_an_open_one_lets_them_through() {
    let mut s = session();
    let owner = s
        .join("Walker".into(), Vec3::new(1.5, 0.05, 4.0), true)
        .unwrap();
    let mut sequence = 0;
    walk(&mut s, owner, &mut sequence, 1.0, 240);
    assert!(feet(&s).z > 0.4, "the shut door stops them: {:?}", feet(&s));
    walk(&mut s, owner, &mut sequence, -1.0, 120);
    // Open it; it is not yet open after half a second.
    assert!(s.mover_goto("door", 1, 1.5).unwrap());
    let views = s.mover_views();
    assert!(views[0].moving && views[0].key.is_none());
    for _ in 0..60 {
        s.step().unwrap();
    }
    let mid = s.mover_views();
    assert!(mid[0].moving && mid[0].progress > 0.3 && mid[0].progress < 0.7);
    for _ in 0..150 {
        s.step().unwrap();
    }
    let open = s.mover_views();
    assert_eq!(open[0].key, Some(1));
    assert!(!open[0].moving && !open[0].blocked);
    walk(&mut s, owner, &mut sequence, 1.0, 300);
    assert!(feet(&s).z < -1.0, "through the open door: {:?}", feet(&s));
}

#[test]
fn a_door_held_by_a_player_in_its_way_waits_and_the_script_can_reverse_it() {
    let mut s = session();
    // Standing where the door sweeps through.
    let owner = s
        .join("Stander".into(), Vec3::new(1.5, 0.05, 1.5), true)
        .unwrap();
    let _ = owner;
    assert!(s.mover_goto("door", 1, 1.0).unwrap());
    for _ in 0..200 {
        s.step().unwrap();
    }
    let v = &s.mover_views()[0];
    assert!(v.blocked && !v.moving, "{v:?}");
    assert!(v.progress < 0.6, "stopped part way: {}", v.progress);
    assert!(s.map_mover_states()[0].blocked);
    // Back the way it came; nothing is in the way of that.
    assert!(s.mover_goto("door", 0, 1.0).unwrap());
    for _ in 0..200 {
        s.step().unwrap();
    }
    let v = &s.mover_views()[0];
    assert_eq!(v.key, Some(0), "{v:?}");
    assert!(!v.blocked);
}

#[test]
fn a_player_touching_a_door_is_near_it_and_one_across_the_room_is_not() {
    let mut s = session();
    let near = s
        .join("Near".into(), Vec3::new(1.5, 0.05, 0.8), true)
        .unwrap();
    let far = s
        .join("Far".into(), Vec3::new(30.0, 0.05, 30.0), true)
        .unwrap();
    s.step().unwrap();
    assert_eq!(s.movers_near_player(near, 0.5), vec!["door".to_string()]);
    assert!(s.movers_near_player(far, 0.5).is_empty());
    assert!(s.mover_goto("door", 7, 1.0).is_err());
    assert!(s.mover_goto("nothing", 0, 1.0).is_err());
}

#[test]
fn a_clients_copy_of_the_world_puts_the_door_where_the_hosts_states_do() {
    let mut s = session();
    assert!(s.mover_goto("door", 1, 1.0).unwrap());
    for _ in 0..130 {
        s.step().unwrap();
    }
    let states = s.map_mover_states();
    assert_eq!(states.len(), 1);
    let tick = s.snapshot().world.tick;
    let mut mirror = CollisionMirror::new(
        Definitions {
            entries: Default::default(),
        },
        vec![
            ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
            door_collider(),
        ],
        Vec::new(),
    );
    mirror.set_movers(&[door()]);
    // Shut, a ray along the doorway hits the slab; open, it does not.
    let ray = Ray::new(Vector::new(1.5, 1.0, 4.0), Vector::new(0.0, 0.0, -1.0));
    let hit = |mirror: &CollisionMirror| {
        mirror
            .physics()
            .query_pipeline_with_filter(QueryFilter::default())
            .cast_ray(&ray, 10.0, true)
            .is_some_and(|(_, toi)| toi < 8.0 && toi > 0.0 && (4.0 - toi).abs() < 0.2)
    };
    assert!(hit(&mirror), "a shut door is in the way");
    assert!(mirror.set_map_movers(&states, tick));
    assert!(!hit(&mirror), "the open door is out of the way");
    assert!(!mirror.set_map_movers(&states, tick), "no change, no move");
    assert_eq!(
        states[0],
        MoverState {
            moving: false,
            ..states[0]
        }
    );
}
