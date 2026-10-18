//! Invented clips and IDs; no original/private content or aim law is encoded.
use anyhow::{Context, Result};
use bri_client::avatar::{
    ActionAnimation, AvatarAnimationInput, AvatarAssets, BodyKind, HeldToolPose,
};
use bri_content::{
    animation::{Layer, sample_layers},
    body::Role,
    shape::{Animation, NodeTrack},
};
use bri_vehicles::schema::{
    SEAT_READY_IMAGE_SLOTS, Seat, SeatHeldImage, SeatReadyContract, SeatReadyPolicy, SeatReadyPose,
};
use glam::{Mat4, Quat, Vec3};

fn policy(pose: SeatReadyPose, images: &[(u8, &str)]) -> SeatReadyPolicy {
    SeatReadyPolicy {
        contracts: vec![SeatReadyContract {
            pose,
            images: images
                .iter()
                .map(|(slot, id)| SeatHeldImage {
                    slot: *slot,
                    image: (*id).into(),
                })
                .collect(),
        }],
    }
}
fn seat() -> Seat {
    let mut seat = bri_vehicles::testing::car().seats.remove(0);
    seat.rider_pose = Some("owned_seat".into());
    seat.rider_ready_policy = Some(policy(SeatReadyPose::Both, &[(0, "test:image/rifle")]));
    seat
}
fn clip(name: &str, priority: i32, nodes: &[(&str, [f32; 3])]) -> Animation {
    Animation {
        name: name.into(),
        frames: 1,
        duration: 1.,
        looping: false,
        additive: false,
        parent_translations: false,
        priority,
        nodes: nodes
            .iter()
            .map(|(node, p)| NodeTrack {
                node: (*node).into(),
                translations: vec![*p],
                rotations: vec![],
                scales: vec![],
                scale_rotations: vec![],
            })
            .collect(),
        objects: vec![],
        rotation_interpolation: Default::default(),
        ground_translations: vec![],
        ground_rotations: vec![],
        triggers: vec![],
    }
}
fn fixture(owns_seat: bool, owns_both: bool) -> Result<AvatarAssets> {
    let mut assets = bri_client::testing::avatar::assets()?;
    for (name, priority, nodes) in [
        (
            "owned_seat",
            12,
            vec![("Mount0", [0.2, 0.3, -0.4]), ("Mount1", [-0.3, 0.2, -0.5])],
        ),
        (
            "armreadyboth",
            14,
            vec![
                ("Mount0", [0.8, -0.4, -0.1]),
                ("Mount1", [-0.7, -0.3, -0.2]),
            ],
        ),
        ("armreadyright", 14, vec![("Mount0", [0.6, 0.5, 0.2])]),
        ("armreadyleft", 14, vec![("Mount1", [-0.6, 0.5, 0.2])]),
        ("look", 16, vec![]),
        (
            "test_action",
            20,
            vec![("Mount0", [1.1, 0.7, -0.3]), ("Mount1", [-1.1, 0.7, -0.3])],
        ),
        ("test_gesture", 21, vec![("Mount1", [-1.4, 0.9, -0.6])]),
    ] {
        assets
            .rig
            .sequences
            .insert(name.into(), clip(name, priority, &nodes));
    }
    if !owns_seat {
        assets.rig.sequences.remove("owned_seat");
    }
    if !owns_both {
        assets.rig.sequences.remove("armreadyboth");
    }
    assets.rig.validate()?;
    Ok(assets)
}
fn player(scale: f32) -> Result<bri_sim::player::PlayerState> {
    Ok(serde_json::from_value(
        serde_json::json!({"owner":1,"feet":[3.,4.,5.],"velocity":[0.,0.,0.],"yaw":0.2,"pitch":0.,"grounded":true,"crouched":false,"jetting":false,"scale":scale}),
    )?)
}
fn input() -> AvatarAnimationInput {
    AvatarAnimationInput {
        sitting: true,
        mount_rotation: Some(Quat::from_rotation_y(0.6) * Quat::from_rotation_z(0.3)),
        seat_pose: Some("owned_seat".into()),
        held_tool_pose: HeldToolPose::Both,
        ..Default::default()
    }
}
fn hands(
    assets: &AvatarAssets,
    input: &AvatarAnimationInput,
    scale: f32,
    preserve: bool,
) -> Result<[Mat4; 2]> {
    let mut mesh = assets.mesh(assets.package.defaults.clone())?;
    let mut archetype = bri_sim::archetype::Archetypes::default()
        .resolve(Default::default())
        .clone();
    archetype.sockets.preserve_hand_pose = preserve;
    mesh.set_body_kind(BodyKind::of(&archetype, false));
    let player = player(scale)?;
    mesh.pose_with_animation(assets, &player, 0., input)?;
    mesh.pose_with_animation(assets, &player, 0.5, input)?;
    Ok([
        mesh.mount_node(assets, 0).context("right mount")?,
        mesh.mount_node(assets, 1).context("left mount")?,
    ])
}
fn close(a: [Mat4; 2], b: [Mat4; 2]) {
    for (a, b) in a.into_iter().zip(b) {
        assert!(a.abs_diff_eq(b, 1e-5), "{a:?} != {b:?}");
    }
}

#[test]
fn seat_policy_uses_native_slot_domain_and_exact_whole_image_set() {
    assert_eq!(SEAT_READY_IMAGE_SLOTS, bri_weapons::IMAGE_SLOTS);
    let mut s = seat();
    assert!(
        s.ready_matches(&[(0, "test:image/rifle")])
            .covers(SeatReadyPose::Both)
    );
    for images in [
        vec![],
        vec![(0, "test:image/other")],
        vec![(0, "test:image/rifle"), (1, "test:image/tool")],
        vec![(0, "test:image/rifle"), (2, "test:image/attachment")],
        vec![(0, "test:image/rifle"), (0, "test:image/rifle")],
        vec![(4, "test:image/rifle")],
    ] {
        assert!(!s.ready_matches(&images).covers(SeatReadyPose::Both));
    }
    s.rider_ready_policy = Some(policy(
        SeatReadyPose::Both,
        &[(0, "test:image/rifle"), (2, "test:image/attachment")],
    ));
    assert!(
        s.ready_matches(&[(2, "test:image/attachment"), (0, "test:image/rifle")])
            .covers(SeatReadyPose::Both)
    );
    assert!(
        !s.ready_matches(&[(0, "test:image/rifle")])
            .covers(SeatReadyPose::Both)
    );
    s.rider_pose = None;
    assert_eq!(
        s.ready_matches(&[(0, "test:image/rifle"), (2, "test:image/attachment")]),
        Default::default()
    );
}
#[test]
fn seat_policy_validation_rejects_invalid_and_ambiguous_contracts() -> Result<()> {
    let good = policy(SeatReadyPose::Both, &[(0, "test:image/rifle")]);
    let mut invalid = vec![
        SeatReadyPolicy { contracts: vec![] },
        policy(SeatReadyPose::Both, &[]),
        policy(SeatReadyPose::Both, &[(4, "x")]),
        policy(SeatReadyPose::Both, &[(0, "x"), (0, "y")]),
    ];
    for bad in [
        "".to_string(),
        "x".repeat(257),
        "bad\nid".into(),
        " padded".into(),
    ] {
        invalid.push(policy(SeatReadyPose::Both, &[(0, &bad)]));
    }
    let mut duplicate = good.clone();
    duplicate.contracts.push(duplicate.contracts[0].clone());
    invalid.push(duplicate);
    let mut order = policy(SeatReadyPose::Both, &[(0, "a"), (2, "b")]);
    let mut reversed = order.contracts[0].clone();
    reversed.images.reverse();
    order.contracts.push(reversed);
    invalid.push(order);
    invalid.push(SeatReadyPolicy {
        contracts: (0..17)
            .map(|i| SeatReadyContract {
                pose: SeatReadyPose::Both,
                images: vec![SeatHeldImage {
                    slot: 0,
                    image: format!("test:image/{i}"),
                }],
            })
            .collect(),
    });
    for p in invalid {
        assert!(!p.valid());
        assert_eq!(p.matches(&[(0, "test:image/rifle")]), Default::default());
    }
    let mut pack = bri_vehicles::testing::pack();
    pack.definitions[0].seats[0] = seat();
    pack.validate()?;
    let bytes = serde_json::to_vec(&pack)?;
    let loaded: bri_vehicles::Pack = serde_json::from_slice(&bytes)?;
    loaded.validate()?;
    pack.definitions[0].seats[0].rider_pose = None;
    assert!(pack.validate().is_err());
    let legacy = bri_vehicles::testing::car().seats.remove(0);
    assert!(
        serde_json::to_value(legacy)?
            .get("rider_ready_policy")
            .is_none()
    );
    assert!(
        serde_json::from_str::<SeatReadyPolicy>(r#"{"contracts":[{"pose":"none","images":[]}]}"#)
            .is_err()
    );
    Ok(())
}
#[test]
fn matched_owned_seat_keeps_both_hands_under_scale_tilt_turn_and_preservation_flags() -> Result<()>
{
    let assets = fixture(true, true)?;
    let mut i = input();
    i.seat_ready = seat().ready_matches(&[(0, "test:image/rifle")]);
    let native = sample_layers(
        &assets.rig.shape,
        &[Layer {
            animation: assets.rig.sequence("owned_seat").unwrap(),
            time: 0.5,
            weight: 1.,
        }],
    )?;
    for scale in [0.7, 1., 1.8] {
        for preserve in [false, true] {
            let observed = hands(&assets, &i, scale, preserve)?;
            let model = Mat4::from_scale_rotation_translation(
                Vec3::splat(scale),
                i.mount_rotation.unwrap(),
                Vec3::from(player(scale)?.feet),
            );
            let expected = ["Mount0", "Mount1"].map(|name| {
                model
                    * native.nodes[assets
                        .rig
                        .shape
                        .nodes
                        .iter()
                        .position(|n| n.name.eq_ignore_ascii_case(name))
                        .unwrap()]
            });
            close(observed, expected);
            let ordinary = hands(
                &assets,
                &AvatarAnimationInput {
                    seat_ready: Default::default(),
                    ..i.clone()
                },
                scale,
                preserve,
            )?;
            assert!(!observed[0].abs_diff_eq(ordinary[0], 1e-4));
            assert!(!observed[1].abs_diff_eq(ordinary[1], 1e-4));
        }
    }
    Ok(())
}
#[test]
fn absent_unmatched_standing_missing_clip_and_body_switch_keep_ordinary_layering() -> Result<()> {
    let assets = fixture(true, true)?;
    let mut i = input();
    let baseline = hands(&assets, &i, 1., false)?;
    let mut legacy = seat();
    legacy.rider_ready_policy = None;
    i.seat_ready = legacy.ready_matches(&[(0, "test:image/rifle")]);
    close(hands(&assets, &i, 1., false)?, baseline);
    legacy.rider_pose = None;
    assert_eq!(
        legacy.ready_matches(&[(0, "test:image/rifle")]),
        Default::default()
    );
    for images in [
        vec![(0, "unmatched")],
        vec![(0, "test:image/rifle"), (1, "tool")],
        vec![(0, "test:image/rifle"), (3, "extra")],
    ] {
        i.seat_ready = seat().ready_matches(&images);
        close(hands(&assets, &i, 1., false)?, baseline);
    }
    i.seat_ready = seat().ready_matches(&[(0, "test:image/rifle")]);
    i.sitting = false;
    i.mount_rotation = None;
    close(
        hands(&assets, &i, 1., false)?,
        hands(
            &assets,
            &AvatarAnimationInput {
                seat_ready: Default::default(),
                ..i.clone()
            },
            1.,
            false,
        )?,
    );
    i = input();
    i.seat_ready = seat().ready_matches(&[(0, "test:image/rifle")]);
    let switched = fixture(false, true)?;
    close(
        hands(&switched, &i, 1., false)?,
        hands(
            &switched,
            &AvatarAnimationInput {
                seat_ready: Default::default(),
                ..i.clone()
            },
            1.,
            false,
        )?,
    );
    i.seat_pose = None;
    close(
        hands(&assets, &i, 1., false)?,
        hands(
            &assets,
            &AvatarAnimationInput {
                seat_ready: Default::default(),
                ..i.clone()
            },
            1.,
            false,
        )?,
    );
    i.seat_pose = Some("missing".into());
    close(
        hands(&assets, &i, 1., false)?,
        hands(
            &assets,
            &AvatarAnimationInput {
                seat_ready: Default::default(),
                ..i.clone()
            },
            1.,
            false,
        )?,
    );
    Ok(())
}
#[test]
fn actual_both_to_right_fallback_requires_explicit_resolved_role_coverage() -> Result<()> {
    let assets = fixture(true, false)?;
    let mut i = input();
    i.seat_ready = seat().ready_matches(&[(0, "test:image/rifle")]);
    let ordinary = hands(
        &assets,
        &AvatarAnimationInput {
            seat_ready: Default::default(),
            ..i.clone()
        },
        1.,
        false,
    )?;
    close(hands(&assets, &i, 1., false)?, ordinary);
    let mut s = seat();
    s.rider_ready_policy = Some(policy(SeatReadyPose::Right, &[(0, "test:image/rifle")]));
    i.seat_ready = s.ready_matches(&[(0, "test:image/rifle")]);
    let owned = hands(&assets, &i, 1., false)?;
    let no_ready = hands(
        &assets,
        &AvatarAnimationInput {
            held_tool_pose: HeldToolPose::None,
            ..i.clone()
        },
        1.,
        false,
    )?;
    close(owned, no_ready);
    assert!(!owned[0].abs_diff_eq(ordinary[0], 1e-4));
    Ok(())
}
#[test]
fn authored_actions_gestures_look_and_seat_aim_remain_independent() -> Result<()> {
    let mut assets = fixture(true, true)?;
    let mut i = input();
    i.seat_ready = seat().ready_matches(&[(0, "test:image/rifle")]);
    i.action = Some(ActionAnimation {
        sequence: "test_action".into(),
        started_at: 0.,
    });
    i.gesture = Some(ActionAnimation {
        sequence: "test_gesture".into(),
        started_at: 0.,
    });
    close(
        hands(&assets, &i, 1., false)?,
        hands(
            &assets,
            &AvatarAnimationInput {
                seat_ready: Default::default(),
                ..i.clone()
            },
            1.,
            false,
        )?,
    );
    i.action = None;
    i.gesture = None;
    let neutral = hands(&assets, &i, 1., false)?;
    assets.rig.sequences.insert(
        "look".into(),
        clip(
            "look",
            16,
            &[("Mount0", [1.3, 0.1, -0.9]), ("Mount1", [-1.3, 0.1, -0.9])],
        ),
    );
    let look = hands(&assets, &i, 1., false)?;
    assert!(!look[0].abs_diff_eq(neutral[0], 1e-4));
    close(
        look,
        hands(
            &assets,
            &AvatarAnimationInput {
                seat_ready: Default::default(),
                ..i.clone()
            },
            1.,
            false,
        )?,
    );
    let name = Role::SeatAim.blockhead_clip();
    assets.rig.sequences.insert(
        name.into(),
        clip(
            name,
            17,
            &[("Mount0", [1.7, 0.2, -0.8]), ("Mount1", [-1.7, 0.2, -0.8])],
        ),
    );
    i.seat_aim = Some(0.5);
    let aimed = hands(&assets, &i, 1., false)?;
    assert!(!aimed[0].abs_diff_eq(look[0], 1e-4));
    close(
        aimed,
        hands(
            &assets,
            &AvatarAnimationInput {
                seat_ready: Default::default(),
                ..i.clone()
            },
            1.,
            false,
        )?,
    );
    Ok(())
}

#[test]
fn resolved_role_is_not_an_implied_union_or_a_missing_ready_layer() -> Result<()> {
    use bri_content::body::RoleClips;
    let mut assets = fixture(true, true)?;
    let roles = RoleClips::resolve(&Default::default(), |c| assets.rig.sequence(c).is_some());
    let images = [(0, "test:image/rifle")];
    let left = policy(SeatReadyPose::Left, &images).matches(&images);
    assert!(left.covers_resolved(Role::AimLeft, &roles));
    assert!(!left.covers_resolved(Role::AimRight, &roles));
    assert!(!left.covers_resolved(Role::AimBoth, &roles));
    assert!(!left.covers_resolved(Role::Idle, &roles));
    let mut singles = policy(SeatReadyPose::Right, &images);
    singles
        .contracts
        .push(policy(SeatReadyPose::Left, &images).contracts.remove(0));
    assert!(singles.valid());
    let singles = singles.matches(&images);
    assert!(singles.covers_resolved(Role::AimRight, &roles));
    assert!(singles.covers_resolved(Role::AimLeft, &roles));
    assert!(!singles.covers_resolved(Role::AimBoth, &roles));
    assets.rig.sequences.remove("armreadyright");
    assets.rig.sequences.remove("armreadyboth");
    let missing = RoleClips::resolve(&Default::default(), |c| assets.rig.sequence(c).is_some());
    assert!(!singles.covers_resolved(Role::AimRight, &missing));
    assert!(!singles.covers_resolved(Role::AimBoth, &missing));
    Ok(())
}
