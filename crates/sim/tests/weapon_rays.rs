//! Projectiles obey a brick's Ray Casting setting, like tools and clicks;
//! Colliding only decides whether bodies pass through it.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    simulation::{Builder, Simulation},
    weapon_query::WeaponQuery,
};
use bri_weapons::{ActorId, Filter, Query, TargetId};
use bri_world::{Brick, ContentRef, World, authority::Actor};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;

fn simulation() -> (Simulation, u64) {
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
                reflection: None,
            },
        )]
        .into(),
    };
    let mut sim = Simulation::new(
        World::new("Rays".into(), "test".into(), vec![[1.0; 4]]),
        definitions,
        vec![ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )
    .unwrap();
    let actor = Actor {
        owner: 1,
        administrator: true,
        trust: Default::default(),
    };
    let id = sim
        .plant(
            &Builder {
                actor: &actor,
                position: Vec3::ZERO,
                reach: 50.0,
            },
            Brick::new(ContentRef::Resolved("brick".into()), [0.0, 0.3, -4.0], 1),
        )
        .unwrap();
    (sim, id)
}

fn shot_hits(sim: &Simulation, brick: u64) -> bool {
    let never = |_: ActorId, _: TargetId| false;
    let never_catch = |_: ActorId, _: ActorId| false;
    let responses = BTreeMap::new();
    let mut query = WeaponQuery {
        simulation: sim,
        affect: &never,
        affect_radius: &never,
        catch: &never_catch,
        responses: &responses,
        truncated_targets: 0,
        shapes: &[],
    };
    let filter = Filter {
        projectile_age_ticks: Some(10),
        source: ActorId(99),
        players: true,
        world_only: false,
    };
    query
        .sweep(Vec3::new(0.0, 0.3, 0.0), Vec3::new(0.0, 0.3, -8.0), filter)
        .is_some_and(|hit| hit.target == TargetId::Brick(brick))
}

#[test]
fn projectiles_follow_ray_casting_not_colliding() {
    let (mut sim, id) = simulation();
    assert!(shot_hits(&sim, id), "a normal brick stops the shot");
    sim.mutate(id, |b| b.raycast = false).unwrap();
    assert!(!shot_hits(&sim, id), "Ray Casting off lets shots through");
    sim.mutate(id, |b| {
        b.raycast = true;
        b.colliding = false;
    })
    .unwrap();
    assert!(shot_hits(&sim, id), "Colliding off still stops shots");
}
