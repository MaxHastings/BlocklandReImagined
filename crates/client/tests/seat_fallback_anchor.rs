//! Invented seat/body points exercise native frames; no private source point.
use anyhow::{Context, Result};
use bri_client::{
    avatar::{AvatarAnimationInput, AvatarAssets},
    vehicles::{ClientVehicles, VehicleAssets},
};
use bri_content::shape::{Animation, NodeTrack};
use bri_sim::{
    player::PlayerState,
    session::{VehicleInfo, VehiclePose},
};
use glam::{Mat4, Quat, Vec3};
use std::collections::BTreeMap;

fn player(scale: f32) -> Result<PlayerState> {
    Ok(serde_json::from_value(serde_json::json!({
        "owner":1,"feet":[3.,4.,5.],"velocity":[0.,0.,0.],"yaw":0.2,"pitch":0.,
        "grounded":true,"crouched":false,"jetting":false,"scale":scale
    }))?)
}
fn clip(name: &str, node: &str, at: Vec3) -> Animation {
    Animation {
        name: name.into(),
        frames: 1,
        duration: 1.,
        looping: false,
        additive: false,
        priority: 12,
        nodes: vec![NodeTrack {
            node: node.into(),
            translations: vec![at.to_array()],
            rotations: vec![],
            scales: vec![],
            scale_rotations: vec![],
        }],
        objects: vec![],
        ground_translations: vec![],
        ground_rotations: vec![],
        triggers: vec![],
    }
}
fn body(height: f32, owns: bool) -> Result<(AvatarAssets, String)> {
    let mut assets = bri_client::testing::avatar::assets()?;
    let bind = bri_content::animation::sample(&assets.rig.shape, None, 0.)?;
    let parents: Vec<_> = assets.rig.shape.nodes.iter().map(|n| n.parent).collect();
    let hip = bri_content::hold::hip(&parents, &bind.nodes).context("fixture hip")?;
    let node = assets.rig.shape.nodes[hip].name.clone();
    assets
        .rig
        .sequences
        .insert("sit".into(), clip("sit", &node, Vec3::new(0., height, 0.3)));
    if owns {
        assets.rig.sequences.insert(
            "invented_owned_seat".into(),
            clip(
                "invented_owned_seat",
                &node,
                Vec3::new(0., height + 0.4, -0.6),
            ),
        );
    }
    assets.rig.validate()?;
    Ok((assets, node))
}
fn input(rotation: Quat, target: Option<Vec3>) -> AvatarAnimationInput {
    AvatarAnimationInput {
        sitting: true,
        mount_rotation: Some(rotation),
        seat_pose: Some("invented_owned_seat".into()),
        seat_fallback_hip: target,
        ..Default::default()
    }
}
fn pose_hip(
    assets: &AvatarAssets,
    node: &str,
    p: &PlayerState,
    i: &AvatarAnimationInput,
) -> Result<Vec3> {
    let mut mesh = assets.mesh(assets.package.defaults.clone())?;
    for t in [0., 1.] {
        mesh.pose_with_animation(assets, p, t, i)?;
    }
    Ok(mesh
        .world_node(assets, node)
        .context("posed hip")?
        .w_axis
        .truncate())
}
#[test]
fn explicit_world_target_survives_distinct_body_hips_scales_and_passenger_turns() -> Result<()> {
    let seat_rotation = Quat::from_rotation_z(0.3) * Quat::from_rotation_y(0.7);
    let origin = Vec3::new(3., 4., 5.);
    let hip_point = Vec3::new(0.2, 0.7, -0.4);
    for height in [0.6, 1.4] {
        let (assets, node) = body(height, false)?;
        for vehicle_scale in [0.7, 2.3] {
            let target = origin + seat_rotation * (hip_point * vehicle_scale);
            assert!(target.distance(origin) > 0.1);
            for body_scale in [0.5, 1., 1.8] {
                for passenger_turn in [-0.8, 0., 1.1] {
                    let rotation = seat_rotation * Quat::from_rotation_y(passenger_turn);
                    let actual = pose_hip(
                        &assets,
                        &node,
                        &player(body_scale)?,
                        &input(rotation, Some(target)),
                    )?;
                    assert!(actual.abs_diff_eq(target, 1e-5), "{actual} != {target}");
                }
            }
        }
    }
    Ok(())
}
#[test]
fn owned_clip_ignores_anchor_and_absent_or_invalid_policy_keeps_legacy() -> Result<()> {
    let rotation = Quat::from_rotation_y(0.8) * Quat::from_rotation_z(0.3);
    let p = player(1.7)?;
    for owns in [false, true] {
        let (assets, node) = body(1.2, owns)?;
        let baseline = pose_hip(&assets, &node, &p, &input(rotation, None))?;
        for invalid in [Vec3::NAN, Vec3::splat(f32::INFINITY)] {
            assert!(
                pose_hip(&assets, &node, &p, &input(rotation, Some(invalid)))?
                    .abs_diff_eq(baseline, 1e-5)
            );
        }
        if owns {
            assert!(
                pose_hip(
                    &assets,
                    &node,
                    &p,
                    &input(rotation, Some(Vec3::new(5., 6., 7.)))
                )?
                .abs_diff_eq(baseline, 1e-5)
            );
        } else {
            assert!(baseline.abs_diff_eq(Vec3::from(p.feet), 1e-5));
        }
        // Removing the selected clip context also removes the exemption.
        let ordinary = AvatarAnimationInput {
            seat_pose: None,
            ..input(rotation, None)
        };
        let stale = AvatarAnimationInput {
            seat_fallback_hip: Some(Vec3::splat(20.)),
            ..ordinary.clone()
        };
        assert!(
            pose_hip(&assets, &node, &p, &ordinary)?
                .abs_diff_eq(pose_hip(&assets, &node, &p, &stale)?, 1e-5)
        );
        let dead = AvatarAnimationInput {
            dead: true,
            ..input(rotation, None)
        };
        let stale = AvatarAnimationInput {
            seat_fallback_hip: Some(Vec3::splat(20.)),
            ..dead.clone()
        };
        assert!(
            pose_hip(&assets, &node, &p, &dead)?
                .abs_diff_eq(pose_hip(&assets, &node, &p, &stale)?, 1e-5)
        );
    }
    Ok(())
}

fn vehicle() -> Result<(
    bri_client::testing::ScratchDir,
    VehicleAssets,
    ClientVehicles,
    VehicleInfo,
)> {
    let dir = bri_client::testing::ScratchDir::new("seat-fallback")?;
    let mut pack = bri_client::testing::vehicles::write_pack(dir.path())?;
    let d = pack
        .definitions
        .iter_mut()
        .find(|d| d.id == bri_vehicles::testing::CAR)
        .context("car")?;
    d.seats[0].rider_pose = Some("invented_owned_seat".into());
    d.seats[0].rider_fallback_hip = Some([0.2, 0.7, -0.4]);
    d.seats[0].transform.position = [0.5, 0.8, -1.1];
    d.seats[0].transform.rotation =
        (Quat::from_rotation_y(0.6) * Quat::from_rotation_z(0.2)).to_array();
    pack.validate()?;
    std::fs::write(dir.path().join("vehicles.json"), serde_json::to_vec(&pack)?)?;
    let assets = VehicleAssets::load(dir.path())?;
    let info = VehicleInfo {
        id: 1,
        definition: bri_vehicles::testing::CAR.into(),
        color: None,
        occupants: vec![],
        destroyed: false,
        turret_broken: false,
        scale: 2.3,
        lights: false,
    };
    let pose: VehiclePose = serde_json::from_value(serde_json::json!({
        "id":1,"tick":1,"position":[3.,4.,5.],
        "rotation":Quat::from_rotation_z(0.3).to_array(),"velocity":[0.,0.,0.],
        "steering":0.,"wheel_suspension":[],"wheel_rotation":[],"wheel_contact":[],
        "wheel_tire":[],"turret_aim":[0.,0.],"jetting":false,"angular_velocity":[0.,0.,0.],
        "mouse_steering":[0.,0.],"driver_input":0,"driver_steering":[false,false],"steering_quiet":0
    }))?;
    let mut vehicles = ClientVehicles::default();
    vehicles.update(
        &BTreeMap::from([(1, info.clone())]),
        &BTreeMap::from([(1, pose)]),
        None,
        None,
        &Default::default(),
    );
    Ok((dir, assets, vehicles, info))
}
#[test]
fn real_vehicle_helper_uses_static_seat_and_full_animated_mount_frame_once() -> Result<()> {
    let (_dir, assets, vehicles, mut info) = vehicle()?;
    let local = Vec3::new(0.2, 0.7, -0.4);
    let (origin, rotation) = vehicles.seat_transform(&assets, &info, 0).context("seat")?;
    let target = vehicles
        .seat_fallback_hip(&assets, &info, 0, None)
        .context("hip")?;
    assert!(target.abs_diff_eq(origin + rotation * (local * info.scale), 1e-5));
    // The supplied animated node already includes vehicle and authored node scale.
    let animated = Mat4::from_scale_rotation_translation(
        Vec3::new(2.3, 1.9, 2.7),
        Quat::from_rotation_x(0.4) * Quat::from_rotation_y(0.9),
        Vec3::new(-3., 7., 2.),
    );
    let target = vehicles
        .seat_fallback_hip(&assets, &info, 0, Some(animated))
        .context("animated hip")?;
    assert!(target.abs_diff_eq(animated.transform_point3(local), 1e-5));
    info.scale = 0.8;
    assert_eq!(
        vehicles.seat_fallback_hip(&assets, &info, 0, Some(animated)),
        Some(target)
    );
    for invalid in [Mat4::ZERO, Mat4::NAN, Mat4::from_scale(Vec3::ZERO)] {
        assert!(
            vehicles
                .seat_fallback_hip(&assets, &info, 0, Some(invalid))
                .is_none()
        );
    }
    info.scale = f32::NAN;
    assert!(
        vehicles
            .seat_fallback_hip(&assets, &info, 0, None)
            .is_none()
    );
    info.destroyed = true;
    assert!(
        vehicles
            .seat_fallback_hip(&assets, &info, 0, Some(animated))
            .is_none()
    );
    info.destroyed = false;
    info.turret_broken = true;
    assert!(
        vehicles
            .seat_fallback_hip(&assets, &info, 0, Some(animated))
            .is_none()
    );
    Ok(())
}
#[test]
fn schema_rejects_nonfinite_out_of_bounds_and_unselected_anchors() -> Result<()> {
    let mut pack = bri_vehicles::testing::pack();
    let seat = &mut pack.definitions[0].seats[0];
    assert!(
        serde_json::to_value(&*seat)?
            .get("rider_fallback_hip")
            .is_none()
    );
    seat.rider_pose = Some("invented_owned_seat".into());
    seat.rider_fallback_hip = Some([0.2, 0.7, -0.4]);
    pack.validate()?;
    let loaded: bri_vehicles::Pack = serde_json::from_slice(&serde_json::to_vec(&pack)?)?;
    loaded.validate()?;
    for bad in [[f32::NAN, 0., 0.], [0., f32::INFINITY, 0.], [0., 0., 64.1]] {
        pack.definitions[0].seats[0].rider_fallback_hip = Some(bad);
        assert!(pack.validate().is_err());
    }
    pack.definitions[0].seats[0].rider_fallback_hip = Some([0.; 3]);
    pack.definitions[0].seats[0].rider_pose = None;
    assert!(pack.validate().is_err());
    Ok(())
}
