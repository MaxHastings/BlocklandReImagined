//! A round body turns aside at a wall's end where the box stops dead against it.
use bri_motor::player::{Body, MoveInput, Player, PlayerTuning};
use glam::Vec3;
use rapier3d::prelude::*;

/// A floor and a wall across x < 0, 3 units ahead (-Z), ending at x = 0.
fn world() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(50.0, 0.5, 50.0).translation(Vector::new(0.0, -0.5, 0.0)),
    );
    world.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(10.0, 10.0, 0.5).translation(Vector::new(-10.0, 10.0, -3.5)),
    );
    bri_physics::detect_collisions(&mut world);
    world
}

/// Where a body walking -Z for a while ends, starting `offset` to the right of the wall's end.
fn walk_past_the_end(body: Body, offset: f32) -> Vec3 {
    let mut w = world();
    let tuning = PlayerTuning {
        body,
        ..PlayerTuning::default()
    };
    let mut p = Player::spawn(&mut w, 1, Vec3::new(offset, 0.05, 1.5), tuning).unwrap();
    for _ in 0..480 {
        p.step(
            &mut w,
            MoveInput {
                forward: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    }
    Vec3::from(p.state().feet)
}

#[test]
fn a_cylinder_slides_round_a_wall_end_the_box_catches_on() {
    let width = PlayerTuning::default().width;
    // Its centre is a quarter of its radius clear of the wall's end: the
    // box's side still overlaps the wall's face, the cylinder's edge does not.
    let offset = width * 0.5 * 0.25;
    let boxed = walk_past_the_end(Body::Box, offset);
    assert!(boxed.z > -3.0, "the box is stopped by the face: {boxed}");
    let round = walk_past_the_end(Body::Cylinder, offset);
    assert!(round.z < -5.0, "the cylinder slides past the end: {round}");
    // Square on to the face, it stops as the box does.
    let square = walk_past_the_end(Body::Cylinder, -2.0);
    assert!(square.z > -3.0, "a wall's face still stops it: {square}");
}

/// Eight fixed solids as stairs going down toward -Z: each tread 0.96 long and
/// 0.64 lower than the last, every face in both windings (as a mesh of
/// separate brushes with two-sided faces is built), touching its neighbours.
fn stair_down() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    for i in 0..8 {
        let top = -0.64 * i as f32;
        let (z0, z1) = (-0.96 * i as f32 - 0.48, -0.96 * i as f32 + 0.48);
        let (x0, x1, bottom) = (-4.0, 4.0, top - 10.0);
        let corners = [
            [x0, bottom, z0],
            [x1, bottom, z0],
            [x1, top, z0],
            [x0, top, z0],
            [x0, bottom, z1],
            [x1, bottom, z1],
            [x1, top, z1],
            [x0, top, z1],
        ]
        .map(|c| Vector::new(c[0], c[1], c[2]));
        let quads = [
            [0, 1, 2, 3],
            [4, 7, 6, 5],
            [0, 4, 5, 1],
            [1, 5, 6, 2],
            [2, 6, 7, 3],
            [3, 7, 4, 0],
        ];
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        for q in quads {
            triangles.push([q[0], q[1], q[2]]);
            triangles.push([q[0], q[2], q[3]]);
        }
        let reversed: Vec<[u32; 3]> = triangles.iter().map(|t| [t[2], t[1], t[0]]).collect();
        triangles.extend(reversed);
        world.insert(
            RigidBodyBuilder::fixed(),
            ColliderBuilder::trimesh(corners.to_vec(), triangles).unwrap(),
        );
    }
    bri_physics::detect_collisions(&mut world);
    world
}

#[test]
fn a_walking_cylinder_goes_down_a_stair_of_touching_blocks_to_the_end() {
    let mut w = stair_down();
    let tuning = PlayerTuning {
        body: Body::Cylinder,
        follow_floor: true,
        step_height: 1.28,
        ..PlayerTuning::default()
    };
    let mut p = Player::spawn(&mut w, 1, Vec3::new(0.0, 0.05, 0.0), tuning).unwrap();
    for _ in 0..600 {
        p.step(
            &mut w,
            MoveInput {
                forward: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    }
    let feet = p.state().feet;
    assert!(
        feet[2] < -0.96 * 6.0 && feet[1] < -0.64 * 5.0,
        "stops on the stair: {feet:?}"
    );
}

#[test]
fn a_cylinder_can_move_out_of_shallow_wall_overlap_without_crossing_through_it() {
    let attempt = |yaw, mesh| {
        let mut w = bri_physics::new_world();
        w.insert(
            RigidBodyBuilder::fixed(),
            ColliderBuilder::cuboid(20., 0.5, 20.).translation(Vector::new(0., -0.5, 0.)),
        );
        let wall = if mesh {
            let vertices = [
                [0., 0., -4.],
                [0.16, 0., -4.],
                [0.16, 6., -4.],
                [0., 6., -4.],
                [0., 0., 4.],
                [0.16, 0., 4.],
                [0.16, 6., 4.],
                [0., 6., 4.],
            ]
            .map(Vector::from_array);
            let mut faces = Vec::new();
            for q in [
                [0, 1, 2, 3],
                [4, 7, 6, 5],
                [0, 4, 5, 1],
                [1, 5, 6, 2],
                [2, 6, 7, 3],
                [3, 7, 4, 0],
            ] {
                for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                    faces.push(t);
                    faces.push([t[2], t[1], t[0]]);
                }
            }
            ColliderBuilder::trimesh(vertices.to_vec(), faces).unwrap()
        } else {
            ColliderBuilder::cuboid(0.08, 3., 4.).translation(Vector::new(0.08, 3., 0.))
        };
        w.insert(RigidBodyBuilder::fixed(), wall);
        bri_physics::detect_collisions(&mut w);
        let tuning = PlayerTuning {
            body: Body::Cylinder,
            width: 1.714,
            stand_height: 3.634,
            unstick: None,
            ..Default::default()
        };
        // Authored slab spans X=0..0.16. The body's right edge reaches 0.16,
        // reproducing the observed overlap without a moving-door dependency.
        let mut p =
            Player::spawn_overlapping(&mut w, 1, Vec3::new(-0.697, 0.005, 0.), tuning).unwrap();
        for _ in 0..120 {
            p.step(
                &mut w,
                MoveInput {
                    forward: 1.,
                    yaw,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        Vec3::from(p.state().feet)
    };
    for mesh in [false, true] {
        let out = attempt(-std::f32::consts::FRAC_PI_2, mesh);
        assert!(
            out.x < -1.7,
            "ordinary movement must separate from contact (mesh={mesh}): {out:?}"
        );
        let into = attempt(std::f32::consts::FRAC_PI_2, mesh);
        assert!(
            into.x < -0.6,
            "moving into the slab must remain blocked (mesh={mesh}): {into:?}"
        );
    }
}
