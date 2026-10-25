//! Actual common/dedicated native registration; no original assets or game window.
mod common;
use anyhow::{Context, Result};
use bri_content::avatar::Package;
use bri_net::{
    content_identity, dedicated,
    host_setup::{self, MapSession},
};
use bri_package::packages::PackageSet;
use bri_sim::session::{AnimationStart, AnimationTarget};
use serde_json::json;
use std::sync::Arc;

const RAW: &str = "posefixture:asset/body";
const FITTED: &str = "posefixture:body/fitted";

fn root() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    bri_net::testing::write_root(root.path(), &[bri_net::testing::MAP])?;
    let dir = root.path().join("addons/posefixture");
    std::fs::create_dir_all(dir.join("assets/archetypes"))?;
    let shape = bri_content::testing::avatar::rig().shape;
    assert!(shape.animations.is_empty(), "bind-only fixture");
    std::fs::write(
        dir.join("assets/raw.shape.json"),
        serde_json::to_vec(&shape)?,
    )?;
    let mut alternate = shape.clone();
    alternate.nodes[0].translation[0] += 0.25;
    std::fs::write(
        dir.join("provided.shape.json"),
        serde_json::to_vec(&alternate)?,
    )?;
    std::fs::write(dir.join("fitted.shape.json"), serde_json::to_vec(&shape)?)?;
    std::fs::write(
        dir.join("assets/content.json"),
        json!({"content":[
            {"kind":"asset","id":RAW,"file":"assets/raw.shape.json"}
        ]})
        .to_string(),
    )?;
    let mut provides = vec![json!({"kind":"body","id":FITTED,"file":"fitted.shape.json"})];
    for (name, model) in [("raw", RAW), ("fitted", FITTED)] {
        let file = format!("assets/archetypes/{name}.json");
        std::fs::write(
            dir.join(&file),
            json!({"schema_version":1,"name":name,"model":model}).to_string(),
        )?;
        provides.push(
            json!({"kind":"archetype","id":format!("posefixture:archetype/{name}"),"file":file}),
        );
    }
    std::fs::write(
        dir.join("package.json"),
        json!({
            "schema_version":1,"id":"posefixture","version":"1.0.0","api":1,
            "name":"Native body fixture","license":"CC0-1.0","provides":provides
        })
        .to_string(),
    )?;
    let path = root.path().join("packages.json");
    let mut packages: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    packages["packages"].as_array_mut().unwrap().push(json!({
        "id":"posefixture","version":"1.0.0","side":"shared","dir":"addons/posefixture"
    }));
    std::fs::write(path, packages.to_string())?;
    Ok(root)
}

fn snapshot(
    root: &std::path::Path,
) -> Result<(
    bri_sim::session::AnimationAssets,
    bri_sim::session::BodyPoseBindings,
)> {
    let packages = PackageSet::load_root(root)?;
    let avatar_dir = packages.role_dir(root, "avatar")?;
    let avatar: Package = serde_json::from_slice(&std::fs::read(avatar_dir.join("avatar.json"))?)?;
    let weapons =
        content_identity::WeaponContent::load_with(&packages.role_dir(root, "weapons")?, &[])?;
    content_identity::body_pose_assets(
        root,
        &packages,
        &avatar_dir,
        &avatar,
        &weapons.pack,
        &packages.role_dir(root, "item_presentation")?,
        &[],
    )
}

#[test]
fn actual_common_and_dedicated_setup_keep_bind_only_and_provider_precedence() -> Result<()> {
    let root = root()?;
    let make_world = || {
        bri_world::World::new(
            "Body inputs".into(),
            bri_net::testing::MAP.into(),
            vec![[1.; 4]],
        )
    };
    let mut dedicated = dedicated::load(root.path(), make_world())?;
    let hosted = host_setup::hosted(
        dedicated.setup.add_ons.as_ref().map(|a| &a.server),
        bri_net::testing::MAP,
        None,
    )?;
    let (local, _) = dedicated.setup.session_with(
        &hosted,
        MapSession {
            simulation: common::simulation_with(make_world()),
            spawn_points: vec![glam::Vec3::new(0., 0.05, 0.)],
            breakables: vec![],
            teleporters: vec![],
            volumes: Vec::new(),
            movers: Vec::new(),
            tutorial: None,
        },
        None,
    )?;
    for name in [
        "v20.player.playerstandardarmor",
        "posefixture:archetype/raw",
        "posefixture:archetype/fitted",
    ] {
        let index = if name.starts_with("v20.") {
            Default::default()
        } else {
            dedicated
                .session
                .archetypes()
                .find(name)
                .context("native definition")?
        };
        let remote_info = dedicated.session.body_pose_asset_info(index)?;
        let local_info = local.body_pose_asset_info(index)?;
        assert_eq!(remote_info, local_info);
    }
    let raw = dedicated.session.body_pose_asset_info(
        dedicated
            .session
            .archetypes()
            .find("posefixture:archetype/raw")
            .unwrap(),
    )?;
    assert!(!raw.fitted, "an authored asset body is not fitted");
    let raw_bytes = std::fs::read(root.path().join("addons/posefixture/assets/raw.shape.json"))?;
    assert_eq!(raw.digest, bri_package::environment::hash_bytes(&raw_bytes));
    let other = std::fs::read(root.path().join("addons/posefixture/provided.shape.json"))?;
    assert_ne!(raw.digest, bri_package::environment::hash_bytes(&other));
    let fitted = dedicated.session.body_pose_asset_info(
        dedicated
            .session
            .archetypes()
            .find("posefixture:archetype/fitted")
            .unwrap(),
    )?;
    assert!(fitted.fitted && fitted.node_count > 0);
    assert!(fitted.roles.clip(bri_content::body::Role::Idle).is_none());
    assert!(
        fitted.fits.hands[0].is_some(),
        "bind-only native body still fits its actual limbs"
    );
    assert!(fitted.fits.seat.is_some());
    let before_info = dedicated.session.body_pose_asset_info(Default::default())?;
    let owner = dedicated
        .session
        .join("Native owner".into(), glam::Vec3::new(0., 1., 0.), true)?;
    let prepared = dedicated.session.prepare_animation_start(
        owner,
        AnimationStart {
            namespace: "native-body-test".into(),
            nonce: 1,
            target: AnimationTarget::Pawn(15),
            sequence: "activate2".into(),
            binding: bri_weapons::AnimationBinding {
                owner: dedicated.session.animation_owner(owner),
                ..Default::default()
            },
            rate: 1.,
            tween: 0.,
        },
    )?;
    let id = dedicated.session.commit_animation_start(prepared)?;
    let before = dedicated.session.animation_view();
    let mut bad = dedicated.setup.content.body_pose_bindings.clone();
    bad.reference_resource = Some("unknown:reference".into());
    assert!(
        dedicated
            .session
            .set_body_pose_assets(dedicated.setup.content.animation_assets.clone(), bad)
            .is_err()
    );
    assert_eq!(dedicated.session.animation_view(), before);
    assert_eq!(dedicated.session.animation_view().active[0].id, id);
    assert!(Arc::ptr_eq(
        &before_info,
        &dedicated.session.body_pose_asset_info(Default::default())?
    ));
    Ok(())
}

#[test]
fn optional_bad_selected_body_does_not_fall_back_and_existing_stock_checks_survive() -> Result<()> {
    let root = root()?;
    // Deliberately malformed kind/id declaration: ordinary Catalog admission
    // rejects this package. The selector must still not salvage an alternate
    // source in place of Avatar's selected asset entry.
    let path = root.path().join("addons/posefixture/package.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    manifest["provides"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"body","id":RAW,"file":"provided.shape.json"}));
    std::fs::write(path, manifest.to_string())?;
    let (before, selected) = snapshot(root.path())?;
    let raw_hash = bri_package::environment::hash_bytes(&std::fs::read(
        root.path().join("addons/posefixture/assets/raw.shape.json"),
    )?);
    assert_eq!(
        before
            .assets
            .iter()
            .find(|a| a.resource == RAW)
            .unwrap()
            .digest,
        raw_hash
    );
    assert!(
        !selected
            .bodies
            .iter()
            .find(|b| b.model == RAW)
            .unwrap()
            .fitted
    );
    std::fs::write(
        root.path().join("addons/posefixture/assets/raw.shape.json"),
        b"invalid native JSON",
    )?;
    let (assets, bindings) = snapshot(root.path())?;
    assert!(!bindings.bodies.iter().any(|b| b.model == RAW));
    assert!(
        !assets.assets.iter().any(|a| a.resource == RAW),
        "parseable alternate cannot override the actual failed client choice"
    );
    assert!(
        assets
            .assets
            .iter()
            .any(|a| a.resource == host_setup::BLOCKHEAD_MODEL)
    );
    assert!(bindings.bodies.iter().any(|b| b.model == FITTED));
    assets.validate()?;
    let packages = PackageSet::load_root(root.path())?;
    let avatar_dir = packages.role_dir(root.path(), "avatar")?;
    let mut avatar: Package =
        serde_json::from_slice(&std::fs::read(avatar_dir.join("avatar.json"))?)?;
    avatar.rig_sha256 = "incorrect".into();
    let weapons = content_identity::WeaponContent::load_with(
        &packages.role_dir(root.path(), "weapons")?,
        &[],
    )?;
    assert!(
        content_identity::body_pose_assets(
            root.path(),
            &packages,
            &avatar_dir,
            &avatar,
            &weapons.pack,
            &packages.role_dir(root.path(), "item_presentation")?,
            &[]
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn optional_bind_only_bodies_share_existing_catalog_budget() -> Result<()> {
    let root = root()?;
    let dir = root.path().join("addons/posefixture");
    let shape = bri_content::testing::rigid_shape("tiny", &[("root", None, [0.; 3])], &[], vec![]);
    std::fs::write(dir.join("tiny.shape.json"), serde_json::to_vec(&shape)?)?;
    let path = dir.join("package.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    let limit = bri_sim::session::MAX_ANIMATION_ASSETS;
    for i in 0..=limit {
        manifest["provides"].as_array_mut().unwrap().push(json!({"kind":"body","id":format!("posefixture:body/tiny{i}"),"file":"tiny.shape.json"}));
    }
    std::fs::write(path, manifest.to_string())?;
    let (assets, bindings) = snapshot(root.path())?;
    assert_eq!(assets.assets.len(), limit);
    assert!(bindings.bodies.len() <= assets.assets.len());
    assert!(
        assets
            .assets
            .iter()
            .any(|a| a.resource == host_setup::BLOCKHEAD_MODEL)
    );
    assets.validate()?;
    Ok(())
}
