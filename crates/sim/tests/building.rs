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
use bri_world::{Brick, ContentRef, World, authority::Actor};
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
        ..Default::default()
    }
}

#[test]
fn liquid_volumes_do_not_block_sight_but_still_remain_selectable() {
    let mut defs = definitions();
    let mut water = defs.entries["plate"].clone();
    water.special = bri_sim::definitions::Special::Water;
    defs.entries.insert("water".into(), water);
    let mut w = world();
    let mut liquid = brick(0.1);
    liquid.definition = ContentRef::Resolved("water".into());
    w.bricks.insert(1, liquid);
    w.next_brick_id = 2;
    let from = Vec3::new(-2.0, 0.1, 0.25);
    let to = Vec3::new(3.0, 0.1, 0.25);
    let sim = Simulation::new(w.clone(), defs.clone(), vec![]).unwrap();
    assert_eq!(
        sim.target(from, Vec3::X, 5.0).unwrap().unwrap().brick,
        Some(1)
    );
    assert!(sim.sight(from, to, 6.0).is_some());
    let mut wall = brick(0.1);
    wall.position[0] = 2.0;
    w.bricks.insert(2, wall);
    w.next_brick_id = 3;
    let sim = Simulation::new(w, defs, vec![]).unwrap();
    assert!(
        sim.sight(from, to, 6.0).is_none(),
        "solid surfaces still occlude"
    );
    assert_eq!(
        sim.target_bricks_always(from, Vec3::X, 5.0)
            .unwrap()
            .unwrap()
            .brick,
        Some(1)
    );
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
    let b = brick(0.1);
    let id = sim.plant(&builder, b).unwrap();
    let eye = Vec3::new(0.5, 2.0, 0.25);
    sim.mutate(id, |b| b.visible = false).unwrap();
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
    sim.mutate(id, |b| b.colliding = false).unwrap();
    assert!(!sim.state().bricks[&id].colliding);
    for _ in 0..240 {
        sim.step().unwrap();
    }
    assert!((sim.physics.bodies[body].translation().y - 0.1).abs() < 0.015);
    assert_eq!(
        sim.target(eye, -Vec3::Y, 3.0).unwrap().unwrap().brick,
        Some(id)
    );
    sim.mutate(id, |b| b.raycast = false).unwrap();
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

#[test]
fn long_targeting_rays_match_a_brute_force_search_of_every_brick() {
    // A dense 60 x 60 unit build with a few bricks that do not raycast.
    let mut w = world();
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 33) as i32
    };
    for id in 1..=3000u64 {
        let mut b = brick(0.1 + (random() % 40) as f32 * 0.2);
        b.position[0] = 0.5 + (random() % 60 - 30) as f32;
        b.position[2] = 0.25 + (random() % 120 - 60) as f32 * 0.5;
        b.raycast = id % 7 != 0;
        w.bricks.insert(id, b);
    }
    w.next_brick_id = 3001;
    let sim = Simulation::new(w, definitions(), vec![floor()]).unwrap();
    let defs = definitions();
    let mut hits = 0;
    for i in 0..2000 {
        let origin = Vec3::new(
            (random() % 100 - 50) as f32 + 0.37,
            (random() % 40) as f32 * 0.25 + 0.13,
            (random() % 100 - 50) as f32 - 0.21,
        );
        let direction = Vec3::new(
            (random() % 200 - 100) as f32,
            (random() % 200 - 100) as f32,
            (random() % 200 - 100) as f32,
        )
        .try_normalize()
        .unwrap_or(Vec3::X);
        let all = i % 2 == 0;
        let hit = if all {
            sim.target_bricks_always(origin, direction, 149.0)
        } else {
            sim.target(origin, direction, 149.0)
        }
        .unwrap();
        let mut brute: Option<(u64, f32)> = None;
        for (id, b) in &sim.state().bricks {
            if !all && !b.raycast {
                continue;
            }
            let inverse = b.transform().inverse();
            if let Some((d, _)) = bri_physics::content::raycast(
                &defs.get(b).unwrap().collision,
                Vector::from_array(inverse.transform_point3(origin).to_array()),
                Vector::from_array(inverse.transform_vector3(direction).to_array()),
                149.0,
            ) && brute.is_none_or(|(_, best)| d < best)
            {
                brute = Some((*id, d));
            }
        }
        match (hit, brute) {
            (Some(hit), Some((id, d))) if hit.brick.is_some() => {
                hits += 1;
                assert!(
                    (hit.distance - d).abs() < 1e-4,
                    "ray {i}: {} vs {d}",
                    hit.distance
                );
                assert!(hit.brick == Some(id) || (hit.distance - d).abs() < 1e-5);
            }
            (Some(hit), brute) => {
                assert!(
                    hit.brick.is_none(),
                    "ray {i}: brick hit without a brute-force hit"
                );
                assert!(
                    brute.is_none_or(|(_, d)| d >= hit.distance - 1e-4),
                    "ray {i}: missed a closer brick"
                );
            }
            (None, brute) => assert!(brute.is_none(), "ray {i}: missed brick {brute:?}"),
        }
    }
    assert!(hits > 200, "only {hits} rays hit bricks");
}

#[test]
fn support_cannot_admit_a_hanging_or_copied_brick_inside_authored_map_geometry() {
    use bri_sim::simulation::PlantFailure;
    // Closed authored floor volume, kept as triangles by the actual interior
    // adapter. The brick below merely touches its top boundary and then lies
    // inside; surface contact alone cannot establish its solid occupancy.
    let vertices = [
        [-10.0, -2.0, -10.0],
        [-10.0, -2.0, 10.0],
        [10.0, -2.0, 10.0],
        [10.0, -2.0, -10.0],
        [-10.0, 0.0, -10.0],
        [-10.0, 0.0, 10.0],
        [10.0, 0.0, 10.0],
        [10.0, 0.0, -10.0],
    ];
    let triangles = [
        [4, 5, 6],
        [4, 6, 7],
        [0, 2, 1],
        [0, 3, 2],
        [0, 4, 7],
        [0, 7, 3],
        [1, 2, 6],
        [1, 6, 5],
        [0, 1, 5],
        [0, 5, 4],
        [3, 7, 6],
        [3, 6, 2],
    ];
    let detail = bri_content::interior::Detail {
        minimum_pixels: 0,
        materials: vec![],
        surfaces: vec![],
        lightmaps: vec![],
        collision_triangles: triangles.map(|t| t.map(|i| vertices[i])).into(),
        convex_hulls: vec![vertices.to_vec()],
        ambient: [0; 4],
        alarm_ambient: [0; 4],
        has_alarm: false,
    };
    for (name, offset, rotation) in [
        ("Slate atrium", Vec3::ZERO, 0.0),
        ("Renamed copper workshop", Vec3::new(12.0, 4.0, -7.5), 0.73),
    ] {
        let transform =
            glam::Mat4::from_rotation_translation(glam::Quat::from_rotation_y(rotation), offset);
        let mesh = bri_physics::content::interior_collider(&detail, transform).unwrap();
        let mut layout = world();
        layout.name = name.into();
        let mut sim = Simulation::new(layout, definitions(), vec![mesh]).unwrap();
        let owner = actor(1);
        let builder = Builder {
            actor: &owner,
            position: offset + Vec3::Y,
            reach: 50.0,
        };
        let placed = |height| {
            let mut b = brick(height);
            b.position = (Vec3::from(b.position) + offset).to_array();
            b
        };
        sim.plant(&builder, placed(0.1)).unwrap();
        let before = sim.state().clone();
        let error = sim.plant(&builder, placed(-0.1)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<PlantFailure>(),
            Some(&PlantFailure::Buried),
            "support must not hide solid map occupancy in {name}: {error}"
        );
        assert_eq!(*sim.state(), before, "rejection must be atomic");
        let error = sim
            .plant_group(&owner, vec![placed(-0.1), placed(-0.3)])
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<PlantFailure>(),
            Some(&PlantFailure::Buried),
            "a connected copied group must obey the same map occupancy rule: {error}"
        );
        assert_eq!(*sim.state(), before, "copied rejection must be atomic");
        sim.plant_group(&owner, vec![placed(0.3), placed(0.5)])
            .unwrap();
    }
}
#[test]
fn open_disconnected_map_surfaces_do_not_fill_the_space_between_them() {
    // One native-adapter collider contains two upward-facing disconnected
    // panels. Its broad AABB includes this free middle gap, but it is not
    // a closed solid and the nearest upper panel edge cannot fill the gap.
    let vertices = [
        [-10., 0., -10.],
        [-10., 0., 10.],
        [-8., 0., 10.],
        [-8., 0., -10.],
        [2., 2., -10.],
        [2., 2., 10.],
        [4., 2., 10.],
        [4., 2., -10.],
    ];
    let triangles = [[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]];
    let detail = bri_content::interior::Detail {
        minimum_pixels: 0,
        materials: vec![],
        surfaces: vec![],
        lightmaps: vec![],
        collision_triangles: triangles.map(|t| t.map(|i| vertices[i])).into(),
        convex_hulls: vec![],
        ambient: [0; 4],
        alarm_ambient: [0; 4],
        has_alarm: false,
    };
    let mesh = bri_physics::content::interior_collider(&detail, glam::Mat4::IDENTITY).unwrap();
    let mut layout = world();
    let mut support = brick(0.9);
    support.owner = 1;
    layout.bricks.insert(1, support);
    layout.next_brick_id = 2;
    let mut sim = Simulation::new(layout, definitions(), vec![mesh]).unwrap();
    let owner = actor(1);
    let builder = Builder {
        actor: &owner,
        position: Vec3::new(0.5, 1.1, 0.25),
        reach: 50.,
    };
    let before = sim.state().bricks.len();
    sim.plant(&builder, brick(1.1))
        .expect("A supported brick in the open gap must remain plantable");
    assert_eq!(sim.state().bricks.len(), before + 1);
}
#[test]
fn copied_group_internal_connectors_cannot_cross_a_native_map_floor() {
    let detail = bri_content::interior::Detail {
        minimum_pixels: 0,
        materials: vec![],
        surfaces: vec![],
        lightmaps: vec![],
        collision_triangles: vec![
            [[-10., 0., -10.], [-10., 0., 10.], [10., 0., 10.]],
            [[-10., 0., -10.], [10., 0., 10.], [10., 0., -10.]],
        ],
        convex_hulls: vec![],
        ambient: [0; 4],
        alarm_ambient: [0; 4],
        has_alarm: false,
    };
    let mesh = bri_physics::content::interior_collider(&detail, glam::Mat4::IDENTITY).unwrap();
    let mut layout = world();
    let mut anchor = brick(0.5);
    anchor.owner = 1;
    layout.bricks.insert(1, anchor);
    layout.next_brick_id = 2;
    let mut sim = Simulation::new(layout, definitions(), vec![mesh]).unwrap();
    let owner = actor(1);
    let before = sim.state().clone();
    sim.plant_group(&owner, vec![brick(0.3), brick(0.1), brick(-0.1)])
        .expect_err("A copied column must not connect internally through the map floor");
    assert_eq!(*sim.state(), before, "Copied rejection must be atomic");
}

#[test]
fn floor_dip_root_still_accepts_stacking_but_not_support_through_its_floor() {
    use bri_sim::simulation::PlantFailure;
    // The root's cell centre is exactly on the allowed floor-dip boundary.
    // This is valid support from above, but cannot bridge that floor downward.
    let raised_floor =
        ColliderBuilder::cuboid(10.0, 0.5, 10.0).translation(Vector::new(0.0, -0.4, 0.0));
    let mut sim = Simulation::new(world(), definitions(), vec![raised_floor]).unwrap();
    let owner = actor(1);
    let builder = Builder {
        actor: &owner,
        position: Vec3::Y,
        reach: 50.0,
    };
    sim.plant(&builder, brick(0.1)).unwrap();
    sim.plant(&builder, brick(0.3))
        .expect("a floor-dipped root still supports a plate above");
    let before = sim.state().clone();
    let error = sim.plant(&builder, brick(-0.1)).unwrap_err();
    assert_eq!(
        error.downcast_ref::<PlantFailure>(),
        Some(&PlantFailure::Buried)
    );
    assert_eq!(*sim.state(), before);
}

#[test]
fn copied_group_keeps_a_clear_attachment_beside_a_partial_map_floor() {
    // The plate spans two stud cells. Only the first connector is obstructed;
    // the second is clear, so normal copied hanging support remains valid.
    let detail = bri_content::interior::Detail {
        minimum_pixels: 0,
        materials: vec![],
        surfaces: vec![],
        lightmaps: vec![],
        collision_triangles: vec![
            [[0., 0., -10.], [0., 0., 10.], [0.5, 0., 10.]],
            [[0., 0., -10.], [0.5, 0., 10.], [0.5, 0., -10.]],
        ],
        convex_hulls: vec![],
        ambient: [0; 4],
        alarm_ambient: [0; 4],
        has_alarm: false,
    };
    let mesh = bri_physics::content::interior_collider(&detail, glam::Mat4::IDENTITY).unwrap();
    let mut layout = world();
    let mut anchor = brick(0.5);
    anchor.owner = 1;
    layout.bricks.insert(1, anchor);
    layout.next_brick_id = 2;
    let mut sim = Simulation::new(layout, definitions(), vec![mesh]).unwrap();
    let owner = actor(1);
    let before = sim.state().bricks.len();
    let ids = sim
        .plant_group(&owner, vec![brick(0.3), brick(0.1), brick(-0.1)])
        .expect("An unblocked matching connector must preserve normal copied support");
    assert_eq!(ids.len(), 3);
    assert_eq!(sim.state().bricks.len(), before + 3);
}

#[test]
fn pathological_map_blocked_attachment_query_refuses_with_a_limit_before_publication() {
    let mut defs = definitions();
    let mut wide = defs.entries["plate"].clone();
    wide.mesh.id = "renamed-wide-attachment".into();
    wide.mesh.footprint_studs = [64, 64];
    wide.mesh.attachment_rows = vec!["b".repeat(64); 64];
    wide.collision = CollisionBody {
        id: wide.mesh.id.clone(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [32.0, 0.2, 32.0],
        }],
    };
    wide.shape = bri_physics::content::collider(&wide.collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    defs.entries.insert("renamed-wide-attachment".into(), wide);
    let mut w = world();
    let root = Brick::new(
        ContentRef::Resolved("renamed-wide-attachment".into()),
        [16.0, 0.1, 16.0],
        1,
    );
    w.bricks.insert(1, root);
    w.next_brick_id = 2;
    let triangles = ColliderBuilder::trimesh(
        vec![
            Vector::new(-1.0, 0.0, -1.0),
            Vector::new(-1.0, 0.0, 33.0),
            Vector::new(33.0, 0.0, 33.0),
            Vector::new(33.0, 0.0, -1.0),
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    )
    .unwrap();
    let mut sim = Simulation::new(w, defs, vec![triangles]).unwrap();
    let owner = actor(1);
    let builder = Builder {
        actor: &owner,
        position: Vec3::new(16.0, 1.0, 16.0),
        reach: 50.0,
    };
    let before = sim.state().clone();
    let hanging = Brick::new(
        ContentRef::Resolved("renamed-wide-attachment".into()),
        [16.0, -0.1, 16.0],
        1,
    );
    let error = sim.plant(&builder, hanging).unwrap_err();
    assert_eq!(
        error.downcast_ref::<bri_sim::simulation::PlantFailure>(),
        Some(&bri_sim::simulation::PlantFailure::Limit)
    );
    assert_eq!(*sim.state(), before);
}

#[test]
fn ordinary_large_footprints_keep_floor_samples_independent_of_query_overhead() {
    for width in [16, 64] {
        let id = format!("renamed-footprint-{width}");
        let mut defs = bri_sim::testing::definitions();
        defs.entries.insert(
            id.clone(),
            bri_sim::testing::definition(
                &id,
                [width, width],
                1,
                bri_sim::definitions::Special::None,
                false,
            ),
        );
        let mut sim = Simulation::new(world(), defs, vec![floor()]).unwrap();
        let owner = actor(1);
        let plate = Brick::new(ContentRef::Resolved(id.clone()), [0.0, 1.1, 0.0], 1);
        let ids = sim.plant_group_floating(&owner, vec![plate]).unwrap();
        assert_eq!(ids.len(), 1);
        assert!(sim.state().bricks[&ids[0]].base_plate);
        assert_eq!(sim.state().bricks[&ids[0]].position, [0.0, 1.1, 0.0]);
        // The limited floor covers only the middle of the 64x64 footprint,
        // so this legitimate root needs more than 256 misses before its hit.
        let builder = Builder {
            actor: &owner,
            position: Vec3::Y,
            reach: 50.0,
        };
        let grounded = sim
            .plant(
                &builder,
                Brick::new(ContentRef::Resolved(id), [0.0, 0.1, 0.0], 1),
            )
            .unwrap();
        assert_eq!(sim.state().bricks[&grounded].position, [0.0, 0.1, 0.0]);
    }
}

/// v20's `fxDTSBrick::plant` (0x53ec40) refuses a brick for the map only
/// where an interior crosses its centre lines or stands more than 0.1 above
/// its bottom, and never asks about players, bots or static shapes. Bricks
/// that clip a corner, a tree or someone standing there plant; Max saw
/// these refused as buried or stuck.
#[test]
fn only_what_v20_refuses_blocks_a_plant() {
    use bri_sim::{map::MapSurface, simulation::PlantFailure};
    let owner = actor(1);
    let builder = Builder {
        actor: &owner,
        position: Vec3::Y,
        reach: 50.0,
    };
    let plant = |extra: Vec<ColliderBuilder>, body: Option<u128>, at: f32| {
        let mut map = vec![floor()];
        map.extend(extra);
        let mut sim = Simulation::new(world(), definitions(), map).unwrap();
        if let Some(tag) = body {
            // A standing player's box with its feet on the floor, half a
            // unit into the plate's far end.
            sim.physics.insert(
                RigidBodyBuilder::kinematic_position_based()
                    .translation(Vector::new(1.125, 1.325, 0.25)),
                ColliderBuilder::cuboid(0.625, 1.325, 0.625).user_data(tag),
            );
            sim.physics.detect_collisions(&(), &());
        }
        sim.plant(&builder, brick(at))
            .map(|_| ())
            .map_err(|e| *e.downcast_ref::<PlantFailure>().unwrap())
    };
    // The plate spans x 0 to 1 and z 0 to 0.5 on the floor.
    // A wall whose face, x + z = 1.4, cuts only the far corner.
    let corner = ColliderBuilder::cuboid(1.0, 2.0, 5.0).position(Pose::from_parts(
        Vector::new(
            0.7 + std::f32::consts::FRAC_1_SQRT_2,
            0.0,
            0.7 + std::f32::consts::FRAC_1_SQRT_2,
        ),
        glam::Quat::from_rotation_y(-std::f32::consts::FRAC_PI_4),
    ));
    assert_eq!(plant(vec![corner], None, 0.1), Ok(()), "a clipped corner");
    // A tree trunk through the middle of the plate.
    let trunk = ColliderBuilder::cuboid(0.05, 1.0, 0.05)
        .translation(Vector::new(0.5, 0.5, 0.25))
        .user_data(MapSurface::Static as u128);
    assert_eq!(plant(vec![trunk], None, 0.1), Ok(()), "a static shape");
    for (who, tag) in [
        ("a player", (1_u128 << 64) | 7),
        ("a bot", bri_sim::session::ENTITY_TAG | 7),
    ] {
        assert_eq!(plant(vec![], Some(tag), 0.1), Ok(()), "{who}");
    }
    // Still refused: an interior wall across the plate's centre line, a
    // plate sunk more than 0.1 into the floor, and a vehicle.
    let wall = ColliderBuilder::cuboid(0.5, 2.0, 5.0).translation(Vector::new(1.4, 0.0, 0.0));
    assert_eq!(plant(vec![wall], None, 0.1), Err(PlantFailure::Buried));
    assert_eq!(plant(vec![], None, -0.1), Err(PlantFailure::Buried));
    assert_eq!(
        // A vehicle's collider tag.
        plant(vec![], Some((2_u128 << 64) | 7), 0.1),
        Err(PlantFailure::Stuck)
    );
}
