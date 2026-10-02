//! Rapier driven the way the game drives it, in random order: bricks
//! (fixed colliders) planted and removed with a collision refresh between
//! steps, vehicles (dynamic bodies) and ridden mounts (kinematic bodies)
//! spawned, teleported and removed. Rapier's debug consistency checks turn
//! any island bookkeeping slip into a panic; proptest shrinks it to the
//! shortest sequence. `PROPTEST_CASES` raises the case count.
use proptest::prelude::*;
use rapier3d::prelude::*;

#[derive(Clone, Debug)]
enum Op {
    Brick { x: i8, y: i8 },
    RemoveBrick(u8),
    Dynamic { x: i8, y: i8 },
    Kinematic { x: i8, y: i8 },
    RemoveBody(u8),
    Teleport { body: u8, x: i8, y: i8 },
    MoveKinematic { body: u8, x: i8, y: i8 },
    Detect,
    Step(u8),
}

fn op() -> impl Strategy<Value = Op> {
    let p = || -3i8..=3;
    prop_oneof![
        (p(), 0i8..=4).prop_map(|(x, y)| Op::Brick { x, y }),
        any::<u8>().prop_map(Op::RemoveBrick),
        (p(), 0i8..=4).prop_map(|(x, y)| Op::Dynamic { x, y }),
        (p(), 0i8..=4).prop_map(|(x, y)| Op::Kinematic { x, y }),
        any::<u8>().prop_map(Op::RemoveBody),
        (any::<u8>(), p(), 0i8..=4).prop_map(|(body, x, y)| Op::Teleport { body, x, y }),
        (any::<u8>(), p(), 0i8..=4).prop_map(|(body, x, y)| Op::MoveKinematic { body, x, y }),
        Just(Op::Detect),
        (1u8..=30).prop_map(Op::Step),
    ]
}

/// The game's refresh between steps (never Rapier's collision-only pass,
/// which left its persistent islands inconsistent: the chaos panics).
fn detect(w: &mut PhysicsWorld) {
    bri_physics::detect_collisions(w);
}

fn at(x: i8, y: i8) -> Vector {
    // Half-unit spacing so neighbours overlap and touch.
    Vector::new(f32::from(x) * 0.5, 0.5 + f32::from(y) * 0.5, 0.0)
}

fn run(ops: &[Op]) {
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(20.0, 0.5, 20.0),
    );
    let mut bricks: Vec<ColliderHandle> = Vec::new();
    let mut bodies: Vec<RigidBodyHandle> = Vec::new();
    let pick = |n: usize, i: u8| (n > 0).then(|| usize::from(i) % n);
    for op in ops {
        match *op {
            Op::Brick { x, y } => {
                bricks.push(w.insert_collider(
                    ColliderBuilder::cuboid(0.5, 0.25, 0.5).translation(at(x, y)),
                    None,
                ));
                detect(&mut w);
            }
            Op::RemoveBrick(i) => {
                if let Some(i) = pick(bricks.len(), i) {
                    w.remove_collider(bricks.swap_remove(i));
                    detect(&mut w);
                }
            }
            Op::Dynamic { x, y } => {
                bodies.push(
                    w.insert(
                        RigidBodyBuilder::dynamic().translation(at(x, y)),
                        ColliderBuilder::cuboid(0.4, 0.4, 0.4),
                    )
                    .0,
                );
            }
            Op::Kinematic { x, y } => {
                bodies.push(
                    w.insert(
                        RigidBodyBuilder::kinematic_position_based().translation(at(x, y)),
                        ColliderBuilder::cuboid(0.4, 0.4, 0.4),
                    )
                    .0,
                );
            }
            Op::RemoveBody(i) => {
                if let Some(i) = pick(bodies.len(), i) {
                    w.remove_body(bodies.swap_remove(i));
                }
            }
            Op::Teleport { body, x, y } => {
                if let Some(i) = pick(bodies.len(), body) {
                    w.bodies[bodies[i]].set_translation(at(x, y), true);
                }
            }
            Op::MoveKinematic { body, x, y } => {
                if let Some(i) = pick(bodies.len(), body) {
                    w.bodies[bodies[i]].set_next_kinematic_translation(at(x, y));
                }
            }
            Op::Detect => detect(&mut w),
            Op::Step(n) => {
                for _ in 0..n {
                    w.step();
                }
            }
        }
    }
    for _ in 0..3 {
        w.step();
    }
}

proptest! {
    #![proptest_config(bri_chaos::proptest_config(256, 0xf15))]
    #[test]
    fn rapier_islands_survive_the_games_update_patterns(ops in proptest::collection::vec(op(), 1..60)) {
        run(&ops);
    }
}

// Shortest sequences the fuzzer found against Rapier's own collision-only
// pass and against the workarounds tried before the microsecond step.
use Op::*;

#[test]
fn vehicle_spawned_onto_a_replaced_brick_then_moved() {
    run(&[
        Dynamic { x: -3, y: 0 },
        Brick { x: 0, y: 0 },
        Dynamic { x: 0, y: 0 },
        RemoveBrick(0),
        Brick { x: 0, y: 0 },
        Teleport {
            body: 12,
            x: 0,
            y: 4,
        },
        Step(5),
        Brick { x: -2, y: 0 },
        RemoveBrick(92),
        Dynamic { x: 0, y: 0 },
    ]);
}

#[test]
fn vehicles_removed_around_a_new_brick() {
    // A zero-length step here indexed past the end of Rapier's sleep scan,
    // in release builds too.
    run(&[
        Dynamic { x: 3, y: 0 },
        Dynamic { x: 0, y: 0 },
        Brick { x: 0, y: 0 },
        RemoveBody(68),
        Dynamic { x: 0, y: 0 },
        RemoveBody(0),
        Teleport {
            body: 0,
            x: 0,
            y: 0,
        },
    ]);
}

#[test]
fn a_stack_of_vehicles_on_a_mount_loses_its_brick() {
    run(&[
        Brick { x: 0, y: 0 },
        Dynamic { x: 0, y: 0 },
        Dynamic { x: -3, y: 2 },
        MoveKinematic {
            body: 0,
            x: 0,
            y: 0,
        },
        Dynamic { x: 0, y: 0 },
        Dynamic { x: -2, y: 2 },
        Dynamic { x: -1, y: 0 },
        Dynamic { x: 0, y: 0 },
        Dynamic { x: -2, y: 0 },
        Dynamic { x: 0, y: 0 },
        Step(1),
        RemoveBrick(0),
    ]);
}

#[test]
fn a_mount_spawned_touching_a_vehicle() {
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(10.0, 0.5, 10.0),
    );
    w.insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 1.0, 0.0)),
        ColliderBuilder::cuboid(0.5, 0.5, 0.5),
    );
    for _ in 0..3 {
        w.step();
    }
    w.insert(
        RigidBodyBuilder::kinematic_position_based().translation(Vector::new(0.0, 1.9, 0.0)),
        ColliderBuilder::cuboid(0.5, 0.5, 0.5),
    );
    detect(&mut w);
    for _ in 0..3 {
        w.step();
    }
}

#[test]
fn a_refresh_neither_moves_bodies_nor_drops_kinematic_targets() {
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(10.0, 0.5, 10.0),
    );
    let (falling, _) = w.insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 3.0, 0.0)),
        ColliderBuilder::cuboid(0.5, 0.5, 0.5),
    );
    let (mount, _) = w.insert(
        RigidBodyBuilder::kinematic_position_based().translation(Vector::new(4.0, 1.0, 0.0)),
        ColliderBuilder::cuboid(0.5, 0.5, 0.5),
    );
    // A resting body sunk into the floor (the solver would push it out) and
    // a spinning, moving one: a refresh must not simulate either of them.
    let (resting, _) = w.insert(
        RigidBodyBuilder::dynamic().translation(Vector::new(-3.0, 0.45, 0.0)),
        ColliderBuilder::cuboid(0.5, 0.5, 0.5),
    );
    w.bodies[falling].set_linvel(Vector::new(1.0, -2.0, 0.5), true);
    w.bodies[falling].set_angvel(Vector::new(0.2, 0.3, 0.4), true);
    w.bodies[mount].set_next_kinematic_translation(Vector::new(5.0, 1.0, 0.0));
    let before: Vec<_> = [falling, resting]
        .map(|b| {
            (
                *w.bodies[b].position(),
                w.bodies[b].linvel(),
                w.bodies[b].angvel(),
            )
        })
        .into();
    detect(&mut w);
    let after: Vec<_> = [falling, resting]
        .map(|b| {
            (
                *w.bodies[b].position(),
                w.bodies[b].linvel(),
                w.bodies[b].angvel(),
            )
        })
        .into();
    assert_eq!(before, after, "pose and velocities are bit-identical");
    assert_eq!(w.bodies[mount].translation().x, 4.0);
    // The mount still reaches the target it was given before the refresh.
    w.step();
    assert!((w.bodies[mount].translation().x - 5.0).abs() < 1e-4);
}
