//! Ray and shape queries from anywhere, in any direction, including from
//! inside bricks, map walls and each other: where the a16 casing crash came
//! from (a ray starting inside a brick reported a zero normal). Every hit a
//! query reports must be usable as is: finite, with a unit normal.
use bri_chaos::fixture::{self, BRICK, PLATE, STONE, TALL, WATER};
use bri_sim::{simulation::Simulation, weapon_query::WeaponQuery};
use bri_weapons::{ActorId, Filter, Query};
use bri_world::{Brick, ContentRef};
use glam::{Quat, Vec3};
use proptest::prelude::*;
use std::collections::BTreeMap;

fn brick(definition: &str, position: [f32; 3], quarter_turns: u8) -> Brick {
    let mut b = Brick::new(ContentRef::Resolved(definition.into()), position, 1);
    b.quarter_turns = quarter_turns;
    b
}

fn simulation() -> Simulation {
    let mut sensor = brick(PLATE, [3.25, 0.1, 3.25], 0);
    sensor.colliding = false;
    let mut hidden = brick(PLATE, [3.75, 0.1, 3.25], 0);
    hidden.raycast = false;
    fixture::synthetic_simulation(&[
        brick(PLATE, [0.25, 0.1, 0.25], 0),
        brick(BRICK, [1.0, 0.3, 0.5], 0),
        brick(BRICK, [1.0, 0.9, 0.5], 1),
        brick(TALL, [2.25, 1.5, 0.25], 0),
        brick(WATER, [-2.0, 0.3, -2.0], 0),
        brick(STONE, [-4.0, 0.3, 0.0], 0),
        sensor,
        hidden,
    ])
    .unwrap()
}

thread_local! {
    static SIMULATION: Simulation = simulation();
}

/// A point near the bricks and walls: inside them often.
fn point() -> impl Strategy<Value = Vec3> {
    prop_oneof![
        4 => (-6.0f32..6.0, -1.0f32..4.0, -6.0f32..6.0).prop_map(|(x, y, z)| Vec3::new(x, y, z)),
        // Exactly on faces, edges and centres of the bricks above.
        2 => (0usize..6, 0usize..3).prop_map(|(i, j)| {
            let centres = [[0.25, 0.1, 0.25], [1.0, 0.3, 0.5], [2.25, 1.5, 0.25], [-4.0, 0.3, 0.0], [12.0, 4.0, 0.0], [0.0, 0.0, 0.0]];
            let c = Vec3::from(centres[i]);
            [c, c + Vec3::new(0.25, 0.1, 0.0), c - Vec3::Y * 0.1][j]
        }),
        1 => prop_oneof![Just(f32::NAN), Just(f32::INFINITY), Just(1e30f32), Just(-0.0f32)]
            .prop_map(|v| Vec3::new(v, 1.0, 0.0)),
    ]
}

fn direction() -> impl Strategy<Value = Vec3> {
    prop_oneof![
        6 => (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0).prop_map(|(x, y, z)| Vec3::new(x, y, z)),
        2 => prop_oneof![Just(Vec3::NEG_Y), Just(Vec3::Y), Just(Vec3::X), Just(Vec3::NEG_Z)],
        1 => Just(Vec3::ZERO),
        1 => Just(Vec3::splat(1e-20)),
        1 => Just(Vec3::new(f32::NAN, 0.0, 1.0)),
    ]
}

fn usable(position: Vec3, normal: Vec3) -> Result<(), TestCaseError> {
    prop_assert!(position.is_finite(), "hit position {position}");
    prop_assert!(
        normal.is_finite() && (normal.length() - 1.0).abs() < 0.01,
        "hit normal {normal} is not a unit vector"
    );
    Ok(())
}

proptest! {
    #![proptest_config(bri_chaos::proptest_config(2000, 0x9e0))]

    #[test]
    fn targeting_rays_report_usable_hits(origin in point(), direction in direction(), reach in prop_oneof![0.0f32..200.0, Just(0.0), Just(f32::NAN)]) {
        SIMULATION.with(|s| -> Result<(), TestCaseError> {
            for all in [false, true] {
                let result = if all {
                    s.target_bricks_always(origin, direction, reach)
                } else {
                    s.target(origin, direction, reach)
                };
                if let Ok(Some(hit)) = result {
                    usable(hit.position, hit.normal)?;
                    prop_assert!(hit.distance.is_finite() && hit.distance >= 0.0 && hit.distance <= reach);
                }
            }
            Ok(())
        })?;
    }

    #[test]
    fn weapon_sweeps_report_usable_hits(start in point(), delta in direction(), length in 0.0f32..50.0,
        age in prop_oneof![Just(None), (0u32..10).prop_map(Some)], half in (0.0f32..1.0, 0.0f32..1.0, 0.0f32..1.0), turn in -3.2f32..3.2) {
        SIMULATION.with(|s| -> Result<(), TestCaseError> {
            let responses = BTreeMap::new();
            let yes = |_: ActorId, _: bri_weapons::TargetId| true;
            let catch = |_: ActorId, _: ActorId| false;
            let mut q = WeaponQuery {
                simulation: s,
                affect: &yes,
                affect_radius: &yes,
                catch: &catch,
                responses: &responses,
                truncated_targets: 0,
                shapes: &[],
            };
            let filter = Filter { projectile_age_ticks: age, source: ActorId(1), players: true, world_only: false };
            let end = start + delta * length;
            if let Some(hit) = q.sweep(start, end, filter) {
                usable(hit.position, hit.normal)?;
                prop_assert!((0.0..=1.0).contains(&hit.fraction), "fraction {}", hit.fraction);
            }
            let half = Vec3::new(half.0, half.1, half.2);
            if let Some(hit) = q.sweep_box(start, end, half, Quat::from_rotation_y(turn), filter) {
                usable(hit.position, hit.normal)?;
                prop_assert!((0.0..=1.0).contains(&hit.fraction), "fraction {}", hit.fraction);
            }
            let _ = q.radius(start, length, 16);
            Ok(())
        })?;
    }

    #[test]
    fn brick_collision_shapes_build_or_refuse(parts in proptest::collection::vec(
        ((-2.0f32..2.0, -2.0f32..2.0, -2.0f32..2.0), prop_oneof![
            4 => (0.0f32..3.0, 0.0f32..3.0, 0.0f32..3.0),
            1 => Just((0.0, 0.0, 0.0)),
            1 => Just((f32::NAN, 1.0, 1.0)),
            1 => Just((-1.0, 1.0, 1.0)),
            1 => Just((1e30, 1e30, 1e30)),
        ]), 0..6)) {
        let body = bri_content::collision::CollisionBody {
            id: "fuzz".into(),
            parts: parts
                .into_iter()
                .map(|((x, y, z), (w, h, d))| bri_content::collision::Part::Box { center: [x, y, z], size: [w, h, d] })
                .collect(),
        };
        if let Ok(builder) = bri_physics::content::collider(&body) {
            let collider = builder.build();
            let aabb = collider.compute_aabb();
            prop_assert!(aabb.mins.is_finite() && aabb.maxs.is_finite(), "{aabb:?}");
        }
    }
}
