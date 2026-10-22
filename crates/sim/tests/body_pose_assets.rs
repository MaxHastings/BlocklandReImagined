//! Retained native inputs from real Session package definitions; no live pose sampling.
use anyhow::{Context, Result};
use bri_content::{avatar::Rig, body::Role};
use bri_package::packages::PackageSet;
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    session::{
        AnimationAsset, AnimationAssets, AnimationStart, AnimationTarget, BodyPoseBinding,
        BodyPoseBindings, Session,
    },
    simulation::Simulation,
};
use glam::Vec3;
use serde_json::json;
use std::sync::Arc;

const MODEL: &str = "bodyfixture:body/native";
const STOCK: &str = "v20.shape.m";

fn asset(resource: &str, rig: bool, bytes: Vec<u8>) -> AnimationAsset {
    AnimationAsset {
        resource: resource.into(),
        digest: bri_package::environment::hash_bytes(&bytes),
        rig,
        library_clips: Vec::new(),
        bytes,
    }
}

fn fixture() -> Result<(
    tempfile::TempDir,
    Session,
    AnimationAssets,
    BodyPoseBindings,
    PackageSet,
)> {
    let root = tempfile::tempdir()?;
    let dir = root.path().join("bodyfixture");
    std::fs::create_dir_all(&dir)?;
    let rig = bri_content::testing::avatar::rig();
    let mut shape = rig.shape.clone();
    shape.animations = rig.sequences.values().cloned().collect();
    std::fs::write(dir.join("body.shape.json"), serde_json::to_vec(&shape)?)?;
    let mut provides = vec![json!({"kind":"body","id":MODEL,"file":"body.shape.json"})];
    for (name, policy) in [
        (
            "first",
            json!({"sockets":{"right_hand":{"node":"Mount0","offset":[0.01,0.,0.]}}}),
        ),
        (
            "second",
            json!({"animations":{"aim_right":"run"},"sockets":{"right_hand":{"node":"missing"}}}),
        ),
        ("authored", json!({"sockets":{"preserve_hand_pose":true}})),
    ] {
        let mut definition = json!({"schema_version":1,"name":name,"model":MODEL});
        for (key, value) in policy.as_object().unwrap() {
            definition[key] = value.clone();
        }
        let file = format!("{name}.json");
        std::fs::write(dir.join(&file), definition.to_string())?;
        provides.push(
            json!({"kind":"archetype","id":format!("bodyfixture:archetype/{name}"),"file":file}),
        );
    }
    std::fs::write(
        dir.join("package.json"),
        json!({
            "schema_version":1,"id":"bodyfixture","version":"1.0.0","api":1,
            "name":"Body inputs","license":"CC0-1.0","provides":provides
        })
        .to_string(),
    )?;
    let packages: PackageSet = serde_json::from_value(json!({"schema_version":1,"packages":[
        {"id":"bodyfixture","version":"1.0.0","side":"shared","dir":"bodyfixture"}
    ]}))?;
    let mut session = Session::new(Simulation::new(
        bri_world::World::new("Body inputs".into(), "test".into(), vec![[1.; 4]]),
        Definitions::default(),
        vec![],
    )?);
    session.install_packages(
        Arc::new(
            Catalog::load(root.path(), &packages, true).map_err(|e| anyhow::anyhow!("{e:#?}"))?,
        ),
        None,
    )?;
    let assets = AnimationAssets {
        assets: vec![
            asset(STOCK, true, serde_json::to_vec(&rig)?),
            asset(MODEL, false, serde_json::to_vec(&shape)?),
        ],
        images: Default::default(),
    };
    let bindings = BodyPoseBindings {
        reference_resource: Some(STOCK.into()),
        bodies: vec![
            BodyPoseBinding {
                model: STOCK.into(),
                resource: STOCK.into(),
                fitted: false,
            },
            BodyPoseBinding {
                model: MODEL.into(),
                resource: MODEL.into(),
                fitted: true,
            },
        ],
    };
    session.set_body_pose_assets(assets.clone(), bindings.clone())?;
    Ok((root, session, assets, bindings, packages))
}

#[test]
fn actual_definitions_sharing_a_rig_keep_roles_sockets_and_authored_hands_distinct() -> Result<()> {
    let (_root, mut session, _, _, _packages) = fixture()?;
    let first = session
        .archetypes()
        .find("bodyfixture:archetype/first")
        .context("first")?;
    let second = session
        .archetypes()
        .find("bodyfixture:archetype/second")
        .context("second")?;
    let authored = session
        .archetypes()
        .find("bodyfixture:archetype/authored")
        .context("authored")?;
    let one = session.body_pose_asset_info(first)?;
    let two = session.body_pose_asset_info(second)?;
    assert_eq!(one.digest, two.digest);
    assert!(
        !one.definition.sockets.preserve_hand_pose && !two.definition.sockets.preserve_hand_pose
    );
    assert_eq!(one.roles.clip(Role::AimRight).unwrap().0, "armreadyright");
    assert_eq!(two.roles.clip(Role::AimRight).unwrap().0, "run");
    assert!(one.hands[0].is_some() && two.hands[0].is_none());
    assert!(one.fits.hands[0].is_some() && two.fits.hands[0].is_none());
    let keep = session.body_pose_asset_info(authored)?;
    assert_eq!(keep.fits.first_person, [true; 2]);
    assert!(keep.fits.hands.iter().all(Option::is_none));
    assert!(Arc::ptr_eq(&one, &session.body_pose_asset_info(first)?));
    // Change the real definition through the existing host tuning operation.
    let owner = session.join("Body tuner".into(), Vec3::new(0., 1., 0.), true)?;
    session.tune_archetype(
        owner,
        "bodyfixture:archetype/first",
        &[("sockets.right_hand.offset.0".into(), 0.19)],
    )?;
    let changed = session.body_pose_asset_info(first)?;
    assert!(!Arc::ptr_eq(&one, &changed));
    assert_eq!(changed.digest, one.digest);
    assert_eq!(changed.definition.sockets.hand(0).unwrap().offset[0], 0.19);
    assert!(changed.fits.hands[0].is_some());
    assert!(
        one.definition.sockets.hand(0).unwrap().offset[0] == 0.01,
        "old immutable receipt remains intact"
    );
    Ok(())
}

#[test]
fn failed_registration_preserves_catalog_ticket_active_identity_and_resolved_entries() -> Result<()>
{
    let (_root, mut session, assets, bindings, _) = fixture()?;
    let info = session.body_pose_asset_info(Default::default())?;
    let owner = session.join("Body owner".into(), Vec3::new(0., 1., 0.), true)?;
    let prepared = session.prepare_animation_start(
        owner,
        AnimationStart {
            namespace: "body-input-test".into(),
            nonce: 1,
            target: AnimationTarget::Pawn(15),
            sequence: "activate2".into(),
            binding: bri_weapons::AnimationBinding {
                owner: session.animation_owner(owner),
                ..Default::default()
            },
            rate: 1.,
            tween: 0.,
        },
    )?;
    let mut unknown = bindings.clone();
    unknown.reference_resource = Some("missing:resource".into());
    assert!(
        session
            .set_body_pose_assets(assets.clone(), unknown)
            .is_err()
    );
    // Failure did not even invalidate a ticket prepared against the old revision.
    let id = session.commit_animation_start(prepared)?;
    let before = session.animation_view();
    let mut invalid = vec![];
    let mut damaged = assets.clone();
    damaged.assets[0].bytes.push(b' ');
    invalid.push((damaged, bindings.clone()));
    let mut bad_hierarchy = assets.clone();
    let mut rig: Rig = serde_json::from_slice(&bad_hierarchy.assets[0].bytes)?;
    rig.shape.nodes[0].parent = Some(0);
    bad_hierarchy.assets[0] = asset(STOCK, true, serde_json::to_vec(&rig)?);
    invalid.push((bad_hierarchy, bindings.clone()));
    let mut bind_only = assets.clone();
    let mut shape: bri_content::shape::Shape = serde_json::from_slice(&bind_only.assets[1].bytes)?;
    shape.animations.clear();
    shape.nodes[0].parent = Some(0);
    bind_only.assets[1] = asset(MODEL, false, serde_json::to_vec(&shape)?);
    invalid.push((bind_only, bindings.clone()));
    let mut duplicate = bindings.clone();
    duplicate.bodies.push(duplicate.bodies[0].clone());
    invalid.push((assets.clone(), duplicate));
    let mut excess = assets.clone();
    excess.assets.resize(257, excess.assets[0].clone());
    invalid.push((excess, bindings.clone()));
    for (incoming, bindings) in invalid {
        assert!(session.set_body_pose_assets(incoming, bindings).is_err());
        assert_eq!(session.animation_view(), before);
        assert_eq!(session.animation_view().active[0].id, id);
        assert!(Arc::ptr_eq(
            &info,
            &session.body_pose_asset_info(Default::default())?
        ));
    }
    let mut absent = bindings;
    absent.reference_resource = None;
    session.set_body_pose_assets(assets, absent)?;
    let first = session
        .archetypes()
        .find("bodyfixture:archetype/first")
        .unwrap();
    let without_reference = session.body_pose_asset_info(first)?;
    assert!(without_reference.reference_resource.is_none());
    assert!(without_reference.reference_digest.is_none());
    assert!(without_reference.fits.seat.is_none());
    assert!(
        without_reference.fits.hands[0].is_some(),
        "missing reference does not suppress independent hand fits"
    );
    assert_eq!(session.animation_view().active[0].id, id);
    assert!(
        serde_json::from_str::<BodyPoseBindings>(
            r#"{"reference_resource":"first","reference_resource":"second","bodies":[]}"#
        )
        .is_err()
    );
    Ok(())
}
