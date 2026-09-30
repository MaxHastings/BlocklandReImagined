//! Solid bricks share one collider per chunk; every hit and contact still
//! names its own brick, and edits rebuild only the chunk they touch.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    chunks::{CHUNK_SIZE, is_chunk},
    definitions::{Definition, Definitions},
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, World};
use rapier3d::prelude::*;

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "cube".into(),
        footprint_studs: [2, 2],
        height_plates: 3,
        attachment_rows: vec!["bb".into(), "bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "cube".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.6, 1.0],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
            "cube".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
            },
        )]
        .into(),
    }
}

fn brick(x: f32, colliding: bool) -> Brick {
    let mut brick = Brick::new(ContentRef::Resolved("cube".into()), [x, 0.3, 0.5], 1);
    brick.colliding = colliding;
    brick
}

/// Cast straight down at `x`; the brick the physics world says it hit.
fn brick_below(simulation: &Simulation, x: f32) -> Option<u64> {
    let ray = Ray::new(Vector::new(x, 5.0, 0.5), Vector::new(0.0, -1.0, 0.0));
    let (handle, hit) = simulation
        .physics
        .query_pipeline_with_filter(QueryFilter::default().exclude_sensors())
        .cast_ray_and_get_normal(&ray, 10.0, true)?;
    let tag = simulation.physics.colliders[handle].user_data;
    if is_chunk(tag) {
        simulation
            .chunks()
            .part_brick(tag, hit.subshape as usize)
    } else {
        u64::try_from(tag).ok()
    }
}

#[test]
fn solid_bricks_share_a_chunk_and_hits_name_their_brick() {
    let mut world = World::new("chunks".into(), "native".into(), vec![[1.0; 4]]);
    // Two solid bricks in one chunk, a non-colliding one, and a solid one
    // in the next chunk along x.
    world.bricks.insert(1, brick(0.5, true));
    world.bricks.insert(2, brick(2.5, true));
    world.bricks.insert(3, brick(4.5, false));
    world.bricks.insert(4, brick(CHUNK_SIZE + 0.5, true));
    world.next_brick_id = 5;
    let mut simulation = Simulation::new(world, definitions(), vec![]).unwrap();
    // Two chunk colliders and one sensor collider of its own.
    assert_eq!(simulation.chunks().len(), 2);
    assert_eq!(simulation.physics.colliders.len(), 3);
    assert_eq!(brick_below(&simulation, 0.5), Some(1));
    assert_eq!(brick_below(&simulation, 2.5), Some(2));
    assert_eq!(brick_below(&simulation, 4.5), None, "sensors are not solid");
    assert_eq!(brick_below(&simulation, CHUNK_SIZE + 0.5), Some(4));

    // Removing a brick rebuilds its chunk; the others keep their names.
    let admin = bri_world::authority::Actor {
        administrator: true,
        ..Default::default()
    };
    simulation.remove(&admin, 1).unwrap();
    assert_eq!(brick_below(&simulation, 0.5), None);
    assert_eq!(brick_below(&simulation, 2.5), Some(2));

    // A brick that stops colliding leaves its chunk for a sensor; one that
    // starts colliding joins its chunk.
    simulation.mutate(2, |b| b.colliding = false).unwrap();
    simulation.mutate(3, |b| b.colliding = true).unwrap();
    assert_eq!(brick_below(&simulation, 2.5), None);
    assert_eq!(brick_below(&simulation, 4.5), Some(3));
    assert_eq!(simulation.chunks().len(), 2);

    // Emptying a chunk takes its collider away.
    simulation.remove(&admin, 4).unwrap();
    assert_eq!(simulation.chunks().len(), 1);
    assert_eq!(brick_below(&simulation, CHUNK_SIZE + 0.5), None);
}
