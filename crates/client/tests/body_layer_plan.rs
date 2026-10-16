//! Matching inputs recorded against the legacy Avatar layer block before its
//! mechanical delegation. Invented clips, native AvatarMesh and native sampler.
use anyhow::{Context, Result, ensure};
use bri_client::avatar::{ActionAnimation, AvatarAnimationInput, AvatarAssets, HeldToolPose};
use bri_content::{
    animation::{Channels, Layer, Pose, sample_layers_with_transition},
    shape::{Animation, NodeTrack},
};
use bri_vehicles::schema::{SeatHeldImage, SeatReadyContract, SeatReadyPolicy, SeatReadyPose};
use glam::Quat;

const SEAT: &str = "plan_seat";
fn clip(
    name: &str,
    priority: i32,
    additive: bool,
    node: &str,
    at: Option<[f32; 3]>,
    turn: Option<Quat>,
) -> Animation {
    Animation {
        name: name.into(),
        frames: 1,
        duration: 1.,
        looping: false,
        additive,
        priority,
        nodes: vec![NodeTrack {
            node: node.into(),
            translations: at.into_iter().collect(),
            rotations: turn.into_iter().map(|q| q.to_array()).collect(),
            scales: vec![],
            scale_rotations: vec![],
        }],
        objects: vec![],
        rotation_interpolation: Default::default(),
        ground_translations: vec![],
        ground_rotations: vec![],
        triggers: vec![],
    }
}
fn assets(base_additive: bool, look_additive: bool) -> Result<AvatarAssets> {
    let mut assets = bri_client::testing::avatar::assets()?;
    assets.rig.sequences.remove("headside");
    assets.rig.sequences.remove("headup");
    for c in [
        clip("root", 12, false, "Mount0", Some([0.1, 0.2, -0.3]), None),
        clip(
            SEAT,
            12,
            base_additive,
            "Mount0",
            Some([0.4, 0.3, -0.5]),
            None,
        ),
        clip(
            "armreadyright",
            14,
            false,
            "Mount0",
            Some([0.7, 0.8, -0.9]),
            None,
        ),
        clip("plan_body", 1, false, "Hip", Some([0.1, 0.5, 0.]), None),
        clip(
            "plan_action",
            20,
            false,
            "Mount0",
            None,
            Some(Quat::from_rotation_z(0.5)),
        ),
        clip(
            "plan_gesture",
            0,
            true,
            "Mount0",
            Some([0.1, 0., -0.05]),
            None,
        ),
    ] {
        assets.rig.sequences.insert(c.name.clone(), c);
    }
    let mut look = clip("look", 16, look_additive, "Mount0", None, None);
    look.frames = 2;
    look.duration = 2.;
    look.nodes[0].rotations = vec![
        Quat::IDENTITY.to_array(),
        Quat::from_rotation_y(0.6).to_array(),
    ];
    assets.rig.sequences.insert("look".into(), look);
    let mut aim = clip("seataim", 17, false, "Mount0", None, None);
    aim.frames = 2;
    aim.duration = 4.;
    aim.nodes[0].rotations = vec![
        Quat::IDENTITY.to_array(),
        Quat::from_rotation_x(0.8).to_array(),
    ];
    assets.rig.sequences.insert("seataim".into(), aim);
    assets.rig.validate()?;
    Ok(assets)
}
fn player(pitch: f32) -> Result<bri_sim::player::PlayerState> {
    Ok(serde_json::from_value(
        serde_json::json!({"owner":1,"feet":[1.,2.,3.],"velocity":[0.,0.,0.],"yaw":0.,"pitch":pitch,"grounded":true,"crouched":false,"jetting":false,"scale":1.3}),
    )?)
}
fn action(sequence: &str) -> Option<ActionAnimation> {
    Some(ActionAnimation {
        sequence: sequence.into(),
        started_at: 0.,
    })
}
fn input() -> AvatarAnimationInput {
    let mut seat = bri_vehicles::testing::car().seats.remove(0);
    seat.rider_pose = Some(SEAT.into());
    seat.rider_ready_policy = Some(SeatReadyPolicy {
        contracts: vec![SeatReadyContract {
            pose: SeatReadyPose::Right,
            images: vec![SeatHeldImage {
                slot: 0,
                image: "test:image".into(),
            }],
        }],
    });
    AvatarAnimationInput {
        sitting: true,
        mount_rotation: Some(Quat::from_rotation_z(0.2)),
        seat_pose: Some(SEAT.into()),
        seat_ready: seat.ready_matches(&[(0, "test:image")]),
        held_tool_pose: HeldToolPose::Right,
        body: [action("plan_body"), action("MiSsInG_Plan_Action")],
        action: action("plan_action"),
        gesture: action("plan_gesture"),
        ..Default::default()
    }
}
fn layers<'a>(
    assets: &'a AvatarAssets,
    names: &[&str],
    look_time: f32,
    time: f32,
) -> Vec<Layer<'a>> {
    names
        .iter()
        .map(|name| {
            let animation = assets.rig.sequence(name).unwrap();
            Layer {
                animation,
                time: if *name == "look" {
                    look_time
                } else if *name == "armreadyright" {
                    animation.duration
                } else {
                    time
                },
                weight: 1.,
            }
        })
        .collect()
}
fn compare(
    assets: &AvatarAssets,
    mesh: &bri_client::avatar::AvatarMesh,
    acted: &Pose,
    unacted: &Pose,
) -> Result<()> {
    for name in ["Hip", "Mount0", "Mount1", "Eye"] {
        let i = assets
            .rig
            .shape
            .nodes
            .iter()
            .position(|n| n.name == name)
            .context("node")?;
        ensure!(
            mesh.world_node(assets, name)
                .context("world node")?
                .abs_diff_eq(mesh.body_transform() * acted.nodes[i], 1e-5),
            "acted node {name}"
        );
    }
    let hand = assets
        .rig
        .shape
        .nodes
        .iter()
        .position(|n| n.name == "Mount0")
        .unwrap();
    let delta = unacted.nodes[hand].inverse() * acted.nodes[hand];
    ensure!(
        mesh.mount_action(assets, 0)
            .context("actual acted/unacted delta")?
            .abs_diff_eq(delta, 1e-5),
        "unacted transition boundary changed"
    );
    Ok(())
}
#[test]
fn legacy_order_masks_and_both_transition_boundaries_match_native_samples() -> Result<()> {
    for base_additive in [false, true] {
        for look_additive in [false, true] {
            let assets = assets(base_additive, look_additive)?;
            let p = player(0.)?;
            let mut input = input();
            let mut mesh = assets.mesh(assets.package.defaults.clone())?;
            mesh.wants_unacted = true;
            for t in [0., 0.1] {
                mesh.pose_with_animation(&assets, &p, t, &input)?;
            }
            let (acted_names, unacted_names, at, unacted_at) = if !base_additive {
                (
                    vec!["plan_body", SEAT, "plan_action", "look", "plan_gesture"],
                    vec![SEAT, "look"],
                    2,
                    1,
                )
            } else if look_additive {
                (
                    vec!["plan_body", "plan_action", SEAT, "look", "plan_gesture"],
                    vec![SEAT, "look"],
                    2,
                    0,
                )
            } else {
                // Look is appended after absolute actions despite its lower priority.
                (
                    vec!["plan_body", "plan_action", "look", SEAT, "plan_gesture"],
                    vec!["look", SEAT],
                    3,
                    1,
                )
            };
            let acted_layers = layers(&assets, &acted_names, 1., 0.1);
            let unacted_layers = layers(&assets, &unacted_names, 1., 0.1);
            let (acted, source) =
                sample_layers_with_transition(&assets.rig.shape, &acted_layers, at, None)?;
            let (unacted, _) = sample_layers_with_transition(
                &assets.rig.shape,
                &unacted_layers,
                unacted_at,
                None,
            )?;
            compare(&assets, &mesh, &acted, &unacted)?;
            ensure!(mesh.skipped_clips() == ["missing_plan_action"]);
            // Leave the selected seat clip and transition to ordinary root. The
            // low-priority body action changes acted boundary2 versus unacted1.
            input.sitting = false;
            input.seat_pose = None;
            mesh.pose_with_animation(&assets, &p, 0.2, &input)?;
            mesh.pose_with_animation(&assets, &p, 0.3, &input)?;
            let next = layers(
                &assets,
                &[
                    "plan_body",
                    "root",
                    "armreadyright",
                    "plan_action",
                    "look",
                    "plan_gesture",
                ],
                1.,
                0.1,
            );
            let next_unacted = layers(&assets, &["root", "armreadyright", "look"], 1., 0.1);
            let from: Option<(&Channels, f32)> = Some((&source, 0.6));
            let (expected, _) = sample_layers_with_transition(&assets.rig.shape, &next, 2, from)?;
            let (expected_unacted, _) =
                sample_layers_with_transition(&assets.rig.shape, &next_unacted, 1, from)?;
            compare(&assets, &mesh, &expected, &expected_unacted)?;
            ensure!(
                mesh.action().1,
                "fixture lost its existing .25-second transition"
            );
        }
    }
    Ok(())
}
#[test]
fn legacy_noncentral_clamped_look_and_selected_duration_are_preserved() -> Result<()> {
    let assets = assets(false, true)?;
    for (pitch, seat_aim, selected, expected_time) in [
        (1.2, None, "look", 0.4),
        (-0.6, None, "look", 0.7),
        (1.2, Some(0.75), "seataim", 3.),
        (-0.6, Some(2.), "seataim", 4.),
        (1.2, Some(f32::NAN), "look", 0.4),
    ] {
        let p = player(pitch)?;
        let input = AvatarAnimationInput {
            sitting: true,
            mount_rotation: Some(Quat::IDENTITY),
            seat_pose: Some(SEAT.into()),
            look_limits: Some([0.2, 0.35]),
            seat_aim,
            ..Default::default()
        };
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        for t in [0., 1.] {
            mesh.pose_with_animation(&assets, &p, t, &input)?;
        }
        let stack = [
            Layer {
                animation: assets.rig.sequence(SEAT).unwrap(),
                time: 1.,
                weight: 1.,
            },
            Layer {
                animation: assets.rig.sequence(selected).unwrap(),
                time: expected_time,
                weight: 1.,
            },
        ];
        let (expected, _) = sample_layers_with_transition(&assets.rig.shape, &stack, 1, None)?;
        let i = assets
            .rig
            .shape
            .nodes
            .iter()
            .position(|n| n.name == "Mount0")
            .unwrap();
        let actual = mesh.world_node(&assets, "Mount0").unwrap();
        ensure!(
            actual.abs_diff_eq(mesh.body_transform() * expected.nodes[i], 1e-5),
            "{selected} at {expected_time}"
        );
    }
    Ok(())
}
