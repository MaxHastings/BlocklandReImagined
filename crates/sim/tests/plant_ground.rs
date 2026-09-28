//! Planting and chain-kill share one "rests on the map" rule: a brick is
//! accepted on the map floor exactly when chain-kill would call it ground.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    simulation::{Builder, Simulation},
};
use bri_world::{Brick, ContentRef, World, authority::Actor};
use glam::Vec3;
use rapier3d::prelude::*;

fn simulation() -> Simulation {
    let mesh = Mesh {
        schema_version: 1,
        id: "brick".into(),
        footprint_studs: [2, 2],
        height_plates: 3,
        attachment_rows: vec!["bb".into(); 6],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "brick".into(),
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
    let definitions = Definitions {
        entries: [(
            "brick".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
            },
        )]
        .into(),
    };
    Simulation::new(
        World::new("Ground".into(), "test".into(), vec![[1.0; 4]]),
        definitions,
        vec![ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )
    .unwrap()
}

fn plant(sim: &mut Simulation, bottom: f32) -> anyhow::Result<u64> {
    let actor = Actor {
        owner: 1,
        administrator: true,
        trust: Default::default(),
    };
    sim.plant(
        &Builder {
            actor: &actor,
            position: Vec3::ZERO,
            reach: 50.0,
        },
        Brick::new(
            ContentRef::Resolved("brick".into()),
            [0.0, bottom + 0.3, -4.0],
            1,
        ),
    )
}

#[test]
fn a_brick_one_plate_above_the_floor_is_floating() {
    let mut sim = simulation();
    // One plate of air under it: chain-kill would not call this ground, so
    // planting refuses it instead of accepting a brick nothing holds up.
    let err = plant(&mut sim, 0.2).unwrap_err();
    assert!(err.to_string().contains("float"), "{err}");
    plant(&mut sim, 0.0).unwrap();
}
