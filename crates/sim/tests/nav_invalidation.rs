//! A brick knocked out of a crowded chunk hands the bots' walk grid its own
//! box to forget, not the whole chunk's (v0.2.10 performance waste audit,
//! finding 10).
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, World, authority::Actor};
use rapier3d::prelude::*;

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]
        .into(),
    }
}

fn footprint(boxes: &[(glam::Vec3, glam::Vec3)]) -> f32 {
    boxes
        .iter()
        .map(|(min, max)| (max.x - min.x) * (max.z - min.z))
        .sum()
}

#[test]
fn removing_one_brick_forgets_its_own_box_not_the_chunk() {
    // 64 plates in one 8 x 8 x 8 chunk.
    let mut w = World::new("Test".into(), "map".into(), vec![[1.0; 4]]);
    let mut id = 1;
    for x in 0..8 {
        for z in 0..8 {
            let at = [0.5 + x as f32, 0.1, 0.25 + z as f32 * 0.5];
            w.bricks.insert(
                id,
                Brick::new(ContentRef::Resolved("plate".into()), at, 999),
            );
            id += 1;
        }
    }
    w.next_brick_id = id;
    let floor = ColliderBuilder::cuboid(10.0, 0.5, 10.0).translation(Vector::new(0.0, -0.5, 0.0));
    let mut sim = Simulation::new(w, definitions(), vec![floor]).unwrap();
    let actor = Actor {
        owner: 999,
        administrator: true,
        ..Default::default()
    };
    sim.track_collision_changes(true);
    sim.step().unwrap();
    assert!(
        sim.take_collision_changes().is_empty(),
        "nothing changed yet"
    );

    sim.remove(&actor, 28).unwrap();
    sim.step().unwrap();
    let changes = sim.take_collision_changes();
    assert!(!changes.is_empty(), "the removed brick is a change");
    let area = footprint(&changes);
    assert!(
        area <= 1.0,
        "one 1 x 0.5 plate forgot {area} units of footprint over {} boxes (the chunk is 64)",
        changes.len()
    );
    // Planting it back is the same box again.
    let brick = Brick::new(ContentRef::Resolved("plate".into()), [3.5, 0.1, 1.75], 999);
    let builder = bri_sim::simulation::Builder {
        actor: &actor,
        position: glam::Vec3::new(3.5, 1.0, 1.75),
        reach: 100.0,
    };
    sim.plant(&builder, brick).unwrap();
    sim.step().unwrap();
    let changes = sim.take_collision_changes();
    let area = footprint(&changes);
    assert!(!changes.is_empty() && area <= 1.0, "planting forgot {area}");
}
