//! Item_Sports balls through the host session with the generated v20 pack.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    session::{Command, Session},
    simulation::Simulation,
};
use bri_weapons::{ItemBounds, native_id};
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;
mod common;
use common::*;

fn session(item: &str) -> Session {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 2],
        height_plates: 1,
        attachment_rows: vec!["bb".into(), "bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.; 3],
            size: [1., 0.2, 1.],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let definitions = Definitions {
        entries: BTreeMap::from([(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
            },
        )]),
    };
    let mut world = World::new("Sports".into(), "test".into(), vec![[1.; 4]]);
    let mut b = Brick::new(ContentRef::Resolved("plate".into()), [0., 0.1, 0.], 77);
    b.item_spawn.item = Some(ContentRef::Resolved(native_id("weapon", item)));
    b.item_spawn.respawn_ms = 1000;
    world.bricks.insert(1, b);
    world.next_brick_id = 2;
    let simulation = Simulation::new(
        world,
        definitions,
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
    )
    .unwrap();
    let mut s = Session::new(simulation);
    let pack = weapon_pack();
    let bounds = pack
        .items
        .keys()
        .map(|id| {
            (
                id.clone(),
                ItemBounds {
                    min: [-0.3; 3],
                    max: [0.3; 3],
                },
            )
        })
        .collect();
    s.set_weapon_pack(pack).unwrap();
    s.set_item_bounds(bounds).unwrap();
    s
}

fn held(s: &Session, owner: u64) -> Option<String> {
    s.weapon_view()
        .images
        .get(&owner)
        .and_then(|i| i.iter().find(|i| i.hand == 0))
        .map(|i| i.image.clone())
}

fn step(s: &mut Session, owner: u64, n: usize) {
    for _ in 0..n {
        hold_still(s, owner);
        s.step().unwrap();
    }
}

#[test]
#[ignore = "requires generated native weapons-pack-007"]
fn walking_into_a_ball_mounts_it_and_fire_throws_it() {
    for item in [
        "basketballItem",
        "dodgeballItem",
        "footballItem",
        "soccerBallItem",
    ] {
        let mut s = session(item);
        let p = s
            .join("Player".into(), Vec3::new(0., 0.35, 0.), false)
            .unwrap();
        step(&mut s, p, 130);
        let image = held(&s, p).unwrap_or_else(|| panic!("{item} not picked up"));
        assert!(image.contains("ball"), "{item}: {image}");
        s.command(p, 1, Command::WeaponTrigger { down: true }).unwrap();
        step(&mut s, p, 100);
        s.command(p, 2, Command::WeaponTrigger { down: false })
            .unwrap();
        step(&mut s, p, 4);
        assert!(
            !s.weapon_view().projectiles.is_empty() || held(&s, p).is_none(),
            "{item} was not thrown: {:?}",
            held(&s, p)
        );
        assert!(held(&s, p).is_none(), "{item} still held");
    }
}
