use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    item_spawners::{ContactIndex, ItemSpawners, facing, placement},
    session::{Command, Session},
    simulation::Simulation,
};
use bri_weapons::{CORE_TOOLS, ItemBounds};
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;
fn mesh() -> Mesh {
    Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 2],
        height_plates: 1,
        attachment_rows: vec!["bb".into(), "bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    }
}
fn definitions() -> Definitions {
    let mesh = mesh();
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
    Definitions {
        entries: BTreeMap::from([(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                requires_behavior_adapter: false,
            },
        )]),
    }
}
fn bounds() -> BTreeMap<String, ItemBounds> {
    CORE_TOOLS
        .into_iter()
        .map(|id| {
            (
                id.into(),
                ItemBounds {
                    min: [-0.1; 3],
                    max: [0.1; 3],
                },
            )
        })
        .collect()
}
fn brick() -> Brick {
    let mut b = Brick::new(ContentRef::Resolved("plate".into()), [0., 0.1, 0.], 77);
    b.item_spawn.item = Some(ContentRef::Resolved(CORE_TOOLS[3].into()));
    b.item_spawn.respawn_ms = 1000;
    b
}
fn session() -> Session {
    let mut world = World::new("Items".into(), "test".into(), vec![[1.; 4]]);
    world.bricks.insert(1, brick());
    world.next_brick_id = 2;
    let simulation = Simulation::new(
        world,
        definitions(),
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
    )
    .unwrap();
    let mut s = Session::new(simulation);
    s.set_item_bounds(bounds()).unwrap();
    s
}
#[test]
fn static_pickup_is_contact_driven_not_builder_trust_and_duplicates_do_not_restart_timer() {
    let mut s = session();
    let far = s
        .join("Far".into(), Vec3::new(5., 0.35, 0.), false)
        .unwrap();
    let player = s
        .join("Visitor".into(), Vec3::new(0., 0.35, 0.), false)
        .unwrap();
    assert_ne!(player, 77);
    s.step().unwrap();
    assert_eq!(
        s.tool_inventories()[&player].slots[3].as_deref(),
        Some(CORE_TOOLS[3])
    );
    assert!(s.tool_inventories()[&far].slots[3].is_none());
    let timer = s.weapon_view().static_items[0].available_at;
    assert_eq!(timer, 121);
    for _ in 0..125 {
        s.step().unwrap();
    }
    assert_eq!(s.weapon_view().static_items[0].available_at, timer);
    assert!(s.tool_inventories()[&player].slots[4].is_none());
    assert!(s.set_item_bounds(bounds()).is_err());
    // After the original owner leaves, another overlapping player can consume
    // the respawned item; an occupied full/duplicate inventory did not consume it.
    s.disconnect(player).unwrap();
    let next = s
        .join("Next".into(), Vec3::new(0., 0.35, 0.), false)
        .unwrap();
    s.step().unwrap();
    assert_eq!(
        s.tool_inventories()[&next].slots[3].as_deref(),
        Some(CORE_TOOLS[3])
    );
    assert!(s.weapon_view().static_items[0].available_at > timer);
}
#[test]
fn drop_uses_current_host_pose_and_another_player_can_collect_immediately() {
    let mut s = session();
    let a = s
        .join("Thrower".into(), Vec3::new(8., 1., 8.), false)
        .unwrap();
    s.command(a, 1, Command::EquipTool { slot: Some(0) })
        .unwrap();
    s.command(a, 2, Command::DropTool { slot: 0 }).unwrap();
    let drop = s.weapon_view().drops[0].clone();
    assert_eq!(drop.position, Vec3::new(8., 2.5, 7.));
    assert_eq!(drop.velocity, Vec3::NEG_Z * 20.);
    assert_eq!((drop.pickup_after, drop.expires), (58, 1200));
    assert!(s.tool_inventories()[&a].slots[0].is_none());
    assert_eq!(s.tool_inventories()[&a].selected, None);
    // Recipient starts with no Hammer; dropping it from afar frees the slot.
    let b = s
        .join("Recipient".into(), Vec3::new(8., 2.4, 6.3), false)
        .unwrap();
    s.command(b, 1, Command::DropTool { slot: 0 }).unwrap();
    s.step().unwrap();
    assert_eq!(
        s.tool_inventories()[&b].slots[0].as_deref(),
        Some(CORE_TOOLS[0])
    );
    assert!(s.weapon_view().drops.iter().all(|d| d.id != drop.id));
    assert!(s.command(a, 2, Command::DropTool { slot: 1 }).is_err());
    assert!(s.command(a, 3, Command::DropTool { slot: 5 }).is_err());
}
#[test]
fn placement_preserves_asymmetric_pivot_world_axes_and_reconciliation_clock() {
    let shape = ItemBounds {
        min: [-0.1, -0.2, -0.8],
        max: [0.3, 0.6, 0.2],
    };
    let mut b = brick();
    b.position = [10., 2., -3.];
    let mut mesh = mesh();
    mesh.footprint_studs = [4, 2];
    b.quarter_turns = 1;
    for direction in 2..=5 {
        b.item_spawn.direction = direction;
        for (selector, axis) in [
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::NEG_Z,
            Vec3::X,
            Vec3::Z,
            Vec3::NEG_X,
        ]
        .into_iter()
        .enumerate()
        {
            b.item_spawn.position = selector as u8;
            let pos = placement(&b, &mesh, shape).unwrap();
            let world = shape.transformed(pos, facing(direction));
            let center = (Vec3::from(world.min) + Vec3::from(world.max)) * 0.5;
            let half = (Vec3::from(world.max) - Vec3::from(world.min)) * 0.5;
            let expected = Vec3::from(b.position) + axis * (Vec3::new(0.5, 0.1, 1.) + half);
            assert!((center - expected).length() < 0.00001);
        }
    }
    let mut s = ItemSpawners::new(bounds());
    let mut b = brick();
    s.reconcile(1, Some(&b), &definitions(), 7).unwrap();
    s.picked_up(1, 7, 120).unwrap();
    b.item_spawn.respawn_ms = 3000;
    b.item_spawn.direction = 3;
    s.reconcile(1, Some(&b), &definitions(), 10).unwrap();
    assert_eq!(s.items[&1].available_at, 127);
    s.reconcile(1, None, &definitions(), 11).unwrap();
    assert!(s.items.is_empty());
    assert!(
        s.contacts(ItemBounds {
            min: [-2.; 3],
            max: [2.; 3]
        })
        .is_empty()
    );
}
#[test]
fn broadphase_includes_boundary_contacts_negative_cells_and_large_shapes() {
    let mut index = ContactIndex::default();
    index.insert(
        1,
        ItemBounds {
            min: [-8.; 3],
            max: [0.; 3],
        },
    );
    index.insert(
        2,
        ItemBounds {
            min: [-1000.; 3],
            max: [1000.; 3],
        },
    );
    assert_eq!(
        index.query(ItemBounds {
            min: [0.; 3],
            max: [0.1; 3]
        }),
        [1, 2].into()
    );
    index.insert(
        1,
        ItemBounds {
            min: [20.; 3],
            max: [21.; 3],
        },
    );
    assert_eq!(
        index.query(ItemBounds {
            min: [-0.1; 3],
            max: [0.; 3]
        }),
        [2].into()
    );
    index.remove(2);
    assert!(
        index
            .query(ItemBounds {
                min: [-1.; 3],
                max: [1.; 3]
            })
            .is_empty()
    );
}

#[test]
fn item_catalog_and_capacity_are_preflighted_before_world_mutation() {
    let spawners = ItemSpawners::new(bounds());
    let mut world = World::new("Capacity".into(), "test".into(), vec![[1.; 4]]);
    for id in 1..=bri_sim::item_spawners::MAX_STATIC_ITEMS as u64 {
        world.bricks.insert(id, brick());
    }
    let properties = bri_world::authority::WrenchProperties {
        name: None,
        light: None,
        emitter: None,
        emitter_direction: 0,
        item_spawn: brick().item_spawn,
        raycast: true,
        colliding: true,
        visible: true,
    };
    let edit = bri_world::authority::Edit::Properties(properties.clone());
    assert!(spawners.validate_edit(&world, 1, &edit).is_ok());
    assert!(spawners.validate_edit(&world, 5000, &edit).is_err());
    assert!(
        spawners
            .validate_append(&world, &[(5000, brick())].into())
            .is_err()
    );
    let mut unknown = properties;
    unknown.item_spawn.item = Some(ContentRef::Resolved("v20.weapon.missing".into()));
    assert!(
        spawners
            .validate_edit(&world, 1, &bri_world::authority::Edit::Properties(unknown))
            .is_err()
    );
    let mut empty = brick();
    empty.item_spawn.item = None;
    assert!(
        spawners
            .validate_append(&world, &[(5000, empty)].into())
            .is_ok()
    );
    assert_eq!(world.bricks.len(), bri_sim::item_spawners::MAX_STATIC_ITEMS);
}
