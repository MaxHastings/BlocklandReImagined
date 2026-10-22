use bri_motor::player::{Placement, Player, PlayerTuning};
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
