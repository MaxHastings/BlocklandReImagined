use bri_motor::player::{MoveInput, Placement, Player, PlayerTuning};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;

fn placement(vertical: Option<f32>, ceiling: bool) -> Placement {
    let mut physics = bri_physics::new_world();
    physics.insert(
        RigidBodyBuilder::fixed(),
        ColliderBuilder::cuboid(10.0, 2.0, 10.0).translation(Vector::new(0.0, -2.0, 0.0)),
    );
    if ceiling {
        physics.insert(
            RigidBodyBuilder::fixed(),
            ColliderBuilder::cuboid(10.0, 1.0, 10.0).translation(Vector::new(0.0, 4.0, 0.0)),
        );
    }
    bri_physics::detect_collisions(&mut physics);
    let mut unstick = json!({"seconds": 0, "reach": 0.968, "step": 0.484});
    if let Some(reach) = vertical {
        unstick["vertical_reach"] = json!(reach);
    }
    let tuning: PlayerTuning = serde_json::from_value(json!({
        "body": "cylinder", "width": 1.76, "stand_height": 4.0, "unstick": unstick,
    }))
    .unwrap();
    let feet = Vec3::new(0.0, -1.24, 0.0);
    let player = Player::spawn(&mut physics, 1, Vec3::new(20.0, 0.0, 0.0), tuning).unwrap();
    player.spot_near(&physics, feet)
}

#[test]
fn a_narrow_body_can_recover_from_a_floor_deeper_than_its_lateral_search() {
    assert!(matches!(placement(None, false), Placement::Stuck { .. }));
    match placement(Some(2.0), false) {
        Placement::Moved { to, .. } => {
            assert!(to.y >= 0.0 && to.y <= 0.76, "{to}");
            assert_eq!((to.x, to.z), (0.0, 0.0));
        }
        other => panic!("vertical floor recovery was refused: {other:?}"),
    }
}

#[test]
fn vertical_recovery_still_requires_room_for_the_whole_body() {
    assert!(matches!(
        placement(Some(2.0), true),
        Placement::Stuck { .. }
    ));
}

// The recovery probe must clear the supporting floor without inventing extra
// headroom. A floor and a ceiling exactly one body-height apart are a real
// authored passage, even when the motor rests slightly inside its floor.
#[test]
fn tight_headroom_does_not_make_a_fitting_body_embedded() {
    for merged in [false, true] {
        for (body, width) in [("box", 1.76), ("cylinder", 1.76), ("ball", 4.0)] {
            let mut physics = bri_physics::new_world();
            if merged {
                let mut positions = Vec::new();
                let mut triangles = Vec::new();
                for (half, centre) in [
                    (Vector::new(10.0, 2.0, 10.0), Vector::new(0.0, -2.0, 0.0)),
                    (Vector::new(10.0, 1.0, 10.0), Vector::new(0.0, 5.0, 0.0)),
                ] {
                    let (vertices, faces) = rapier3d::parry::shape::Cuboid::new(half).to_trimesh();
                    let base = positions.len() as u32;
                    positions.extend(vertices.into_iter().map(|p| p + centre));
                    triangles.extend(faces.into_iter().map(|t| t.map(|i| i + base)));
                }
                physics.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::trimesh(positions, triangles).unwrap(),
                );
            } else {
                physics.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::cuboid(10.0, 2.0, 10.0)
                        .translation(Vector::new(0.0, -2.0, 0.0)),
                );
                physics.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::cuboid(10.0, 1.0, 10.0)
                        .translation(Vector::new(0.0, 5.0, 0.0)),
                );
            }
            bri_physics::detect_collisions(&mut physics);
            let tuning: PlayerTuning = serde_json::from_value(json!({
                "body": body, "width": width, "stand_height": 4.0,
                "crouch_height": if body == "ball" { 4.0 } else { 1.0 },
                "unstick": {"seconds": 0, "reach": 0.968, "step": 0.484},
            }))
            .unwrap();
            let mut player =
                Player::spawn(&mut physics, 1, Vec3::new(30.0, 0.0, 0.0), tuning).unwrap();
            let feet = Vec3::new(0.0, -0.01, 0.0);
            assert!(
                matches!(player.spot_near(&physics, feet), Placement::Clear(p) if p == feet),
                "{body} has room below the ceiling; recovery must not add body height"
            );
            let above = player.spot_near(&physics, feet + Vec3::Y * 0.11);
            assert!(
                matches!(above, Placement::Stuck { .. }),
                "{body} merged={merged}: real ceiling penetration must still be refused: {above:?}"
            );
            assert!(
                matches!(
                    player.spot_near(&physics, feet - Vec3::Y * 0.11),
                    Placement::Stuck { .. }
                ),
                "{body}: real floor penetration must still be refused"
            );
            // Position setup models the motor's small settled floor contact;
            // subsequent travel uses normal movement and no recovery writes.
            player.teleport(&mut physics, feet, 0.0).unwrap();
            let relocations = player.relocations();
            if body != "ball" {
                let wider = PlayerTuning {
                    width: width + 0.1,
                    ..player.tuning().clone()
                };
                assert!(
                    player.room_for(&physics, &wider),
                    "a wider body still has the same fitting headroom"
                );
                player
                    .step(
                        &mut physics,
                        MoveInput {
                            crouch: true,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                player.step(&mut physics, MoveInput::default()).unwrap();
                assert!(
                    !player.state().crouched,
                    "the standing body has room to rise"
                );
            }
            for _ in 0..60 {
                player
                    .step(
                        &mut physics,
                        MoveInput {
                            forward: 1.0,
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
            assert_eq!(
                player.relocations(),
                relocations,
                "{body} merged={merged}: ordinary walking must not invoke recovery"
            );
            assert!(
                player.state().feet[2] < -1.0,
                "{body} merged={merged}: the fitting body must traverse the passage"
            );
        }
    }
}
