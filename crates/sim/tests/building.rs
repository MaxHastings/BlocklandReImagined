use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    ghost,
    grid::Bounds,
    simulation::{Builder, Simulation},
};
use bri_world::{
    Action, Brick, ContentRef, Event, Input, Target, World,
    authority::{Actor, Edit},
};
use glam::Vec3;
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
                requires_behavior_adapter: false,
            },
        )]
        .into(),
    }
}
fn brick(y: f32) -> Brick {
    Brick::new(ContentRef::Resolved("plate".into()), [0.5, y, 0.25], 999)
}
fn world() -> World {
    World::new("Test".into(), "map".into(), vec![[1.0; 4]])
}
fn floor() -> ColliderBuilder {
    ColliderBuilder::cuboid(10.0, 0.5, 10.0).translation(Vector::new(0.0, -0.5, 0.0))
}
fn actor(owner: u64) -> Actor {
    Actor {
        owner,
        administrator: false,
    }
}

#[test]
fn placement_is_atomic_and_checks_grid_support_ownership_and_reach() {
    let mut sim = Simulation::new(world(), definitions(), vec![floor()]).unwrap();
    let owner = actor(1);
    let builder = Builder {
        actor: &owner,
        position: Vec3::Y,
        reach: 50.0,
    };
    let id = sim.plant(&builder, brick(0.1)).unwrap();
    assert_eq!(sim.state().bricks[&id].owner, 1);
    for invalid in [brick(0.1), brick(0.2), brick(2.1), brick(-0.1)] {
        let before = sim.state().clone();
        assert!(sim.plant(&builder, invalid).is_err());
        assert_eq!(*sim.state(), before);
    }
    let other = actor(2);
    let other_builder = Builder {
        actor: &other,
        position: Vec3::Y,
        reach: 50.0,
    };
    assert!(
        sim.plant(&other_builder, brick(0.3))
            .unwrap_err()
            .to_string()
            .contains("permission")
    );
    let far = Builder {
        actor: &owner,
        position: Vec3::splat(100.0),
        reach: 1.0,
    };
    assert!(sim.plant(&far, brick(0.3)).is_err());
    sim.plant(&builder, brick(0.3)).unwrap();
    assert!(sim.remove(&other, id).is_err());
    sim.remove(&owner, id).unwrap();
    // Removal must release the occupancy index as well as its collider.
    sim.plant(&builder, brick(0.1)).unwrap();
}

#[test]
fn targeting_flags_and_delayed_collision_changes_reach_the_solver() {
    let mut sim = Simulation::new(world(), definitions(), vec![floor()]).unwrap();
    let owner = actor(1);
    let builder = Builder {
        actor: &owner,
        position: Vec3::Y,
        reach: 50.0,
    };
    let mut b = brick(0.1);
    b.events.push(Event {
        enabled: true,
        input: Input::Activate,
        delay_ms: 10,
        target: Target::ThisBrick,
        action: Action::Colliding(false),
    });
    let id = sim.plant(&builder, b).unwrap();
    let eye = Vec3::new(0.5, 2.0, 0.25);
    sim.edit(&owner, id, Edit::Action(Action::Visible(false)))
        .unwrap();
    assert_eq!(
        sim.target(eye, -Vec3::Y, 3.0).unwrap().unwrap().brick,
        Some(id)
    );
    let (body, _) = sim.physics.insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(0.5, 1.0, 0.25)),
        ColliderBuilder::ball(0.1),
    );
    for _ in 0..240 {
        sim.step().unwrap();
    }
    assert!((sim.physics.bodies[body].translation().y - 0.3).abs() < 0.015);
    assert_eq!(sim.activate(eye, -Vec3::Y).unwrap(), Some(id));
    sim.step().unwrap();
    sim.step().unwrap();
    assert!(sim.state().bricks[&id].colliding);
    sim.step().unwrap();
    assert!(!sim.state().bricks[&id].colliding);
    for _ in 0..240 {
        sim.step().unwrap();
    }
    assert!((sim.physics.bodies[body].translation().y - 0.1).abs() < 0.015);
    assert_eq!(
        sim.target(eye, -Vec3::Y, 3.0).unwrap().unwrap().brick,
        Some(id)
    );
    sim.edit(&owner, id, Edit::Action(Action::Raycast(false)))
        .unwrap();
    assert_eq!(sim.target(eye, -Vec3::Y, 3.0).unwrap().unwrap().brick, None);
}

#[test]
fn native_triangle_map_support_embedding_and_target_occlusion() {
    // Two triangles represent a floor, matching the map adapter's shape class.
    let mesh = ColliderBuilder::trimesh(
        vec![
            Vector::new(-10.0, 0.0, -10.0),
            Vector::new(-10.0, 0.0, 10.0),
            Vector::new(10.0, 0.0, 10.0),
            Vector::new(10.0, 0.0, -10.0),
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    )
    .unwrap();
    let wall = ColliderBuilder::cuboid(2.0, 2.0, 0.1).translation(Vector::new(0.0, 1.0, -1.0));
    let mut sim = Simulation::new(world(), definitions(), vec![mesh, wall]).unwrap();
    let owner = actor(1);
    let builder = Builder {
        actor: &owner,
        position: Vec3::Y,
        reach: 50.0,
    };
    sim.plant(&builder, brick(0.1)).unwrap();
    let hit = sim
        .target(Vec3::new(0.5, 0.1, -2.0), Vec3::Z, 5.0)
        .unwrap()
        .unwrap();
    assert_eq!(hit.brick, None);
    assert!(hit.distance < 1.0);
    let mut buried = brick(-0.1);
    buried.position[0] = 2.0;
    assert!(sim.plant(&builder, buried).is_err());
}

#[test]
fn camera_relative_shifts_and_rotations_preserve_the_build_grid() {
    let defs = definitions();
    let mesh = &defs.entries["plate"].mesh;
    for facing in [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z] {
        for direction in [-1, 1] {
            let mut b = brick(0.1);
            let original = b.clone();
            for _ in 0..4 {
                ghost::rotate(&mut b, mesh, facing, direction);
                Bounds::new(&b, mesh).unwrap();
            }
            assert_eq!(b, original);
        }
        let mut b = brick(0.1);
        let start = Vec3::from(b.position);
        ghost::shift(&mut b, mesh, facing, 1, 1, 1, false);
        assert!(
            (Vec3::from(b.position)
                - (start + (facing + Vec3::Y.cross(facing)) * 0.5 + Vec3::Y * 0.2))
                .length()
                < 0.00001
        );
        Bounds::new(&b, mesh).unwrap();
    }
    let mut b = brick(0.1);
    ghost::shift(&mut b, mesh, Vec3::X, 1, 0, 0, true);
    assert_eq!(b.position[0], 1.5);
    ghost::rotate(&mut b, mesh, Vec3::X, 1);
    let x = b.position[0];
    ghost::shift(&mut b, mesh, Vec3::X, 1, 0, 0, true);
    assert_eq!(b.position[0], x + 0.5);
}

#[test]
fn planting_cannot_trap_a_dynamic_or_kinematic_entity() {
    for body in [
        RigidBodyBuilder::dynamic(),
        RigidBodyBuilder::kinematic_position_based(),
    ] {
        let mut sim = Simulation::new(world(), definitions(), vec![floor()]).unwrap();
        sim.physics.insert(
            body.translation(Vector::new(0.5, 0.3, 0.25)),
            ColliderBuilder::ball(0.3),
        );
        sim.physics.detect_collisions(&(), &());
        let owner = actor(1);
        let builder = Builder {
            actor: &owner,
            position: Vec3::Y,
            reach: 50.0,
        };
        let before = sim.state().clone();
        assert_eq!(
            sim.plant(&builder, brick(0.1))
                .unwrap_err()
                .downcast_ref::<bri_sim::simulation::PlantFailure>(),
            Some(&bri_sim::simulation::PlantFailure::Stuck)
        );
        assert_eq!(*sim.state(), before);
        assert!(sim.target(Vec3::Y, Vec3::splat(f32::MAX), 10.0).is_err());
    }
}

#[test]
fn authored_hull_below_grid_is_allowed_only_at_the_map_floor() {
    let mut defs = definitions();
    let d = defs.entries.get_mut("plate").unwrap();
    d.collision.parts = vec![Part::Box {
        center: [0.0, -0.015, 0.0],
        size: [1.0, 0.2, 0.5],
    }];
    d.shape = bri_physics::content::collider(&d.collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let mut sim = Simulation::new(world(), defs, vec![floor()]).unwrap();
    let owner = actor(1);
    let builder = Builder {
        actor: &owner,
        position: Vec3::Y,
        reach: 50.0,
    };
    let id = sim.plant(&builder, brick(0.1)).unwrap();
    sim.remove(&owner, id).unwrap();
    assert!(sim.plant(&builder, brick(-0.1)).is_err());
    // A kinematic body at the same floor cannot consume the map-only allowance.
    sim.physics.insert(
        RigidBodyBuilder::kinematic_position_based().translation(Vector::new(0.5, -0.5, 0.25)),
        ColliderBuilder::cuboid(1.0, 0.5, 1.0),
    );
    sim.physics.detect_collisions(&(), &());
    assert!(sim.plant(&builder, brick(0.1)).is_err());
}
