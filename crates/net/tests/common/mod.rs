//! The synthetic session and host options shared by the integration tests
//! that need no original game assets.
#![allow(dead_code)]
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_net::server::ServerOptions;
use bri_sim::{
    definitions::{Definition, Definitions},
    session::Session,
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;

pub fn session() -> Session {
    session_with(World::new(
        "Stress".into(),
        "fixture".into(),
        vec![[1.0; 4], [0.0; 4]],
    ))
}

/// The synthetic session over `world`, whose bricks use the `plate` definition.
pub fn session_with(world: World) -> Session {
    let mut session = Session::new(simulation_with(world));
    session
        .set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    session
}

/// The synthetic simulation over `world`: the `plate` definition on flat ground.
pub fn simulation_with(world: World) -> Simulation {
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
    let defs = Definitions {
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
            },
        )]
        .into(),
    };
    Simulation::new(
        world,
        defs,
        vec![ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )
    .unwrap()
}

pub fn options() -> ServerOptions {
    ServerOptions {
        bind: "127.0.0.1:0".parse().unwrap(),
        environment: bri_package::environment::Environment::empty(),
        spawn_points: (0..32)
            .map(|i| Vec3::new(-48.0 + 3.0 * i as f32, 0.05, 0.0))
            .collect(),
        certificate: None,
        map_loader: None,
        packages: None,
    }
}
