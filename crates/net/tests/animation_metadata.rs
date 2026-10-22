//! Real host setup registration and common bounded native metadata loading.
mod common;
use anyhow::{Context, Result};
use bri_content::avatar::{Package, Rig};
use bri_net::{
    content_identity, dedicated,
    host_setup::{self, MapSession},
};
use bri_package::packages::PackageSet;
use bri_sim::session::{AnimationStart, AnimationTarget, Session};
use bri_weapons::AnimationBinding;
use bri_world::World;
use serde_json::json;

fn admit(session: &mut Session, sequence: &str) -> Result<bri_sim::session::ActiveAnimation> {
    let owner = session.join(
        "Native metadata".into(),
        glam::Vec3::new(0.0, 0.05, 0.0),
        true,
    )?;
    let prepared = session.prepare_animation_start(
        owner,
        AnimationStart {
            namespace: "metadata-test".into(),
            nonce: 1,
            target: AnimationTarget::Pawn(15),
            sequence: sequence.into(),
            binding: AnimationBinding {
                owner: session.animation_owner(owner),
                ..Default::default()
            },
            rate: 1.0,
            tween: 0.0,
        },
    )?;
    session.commit_animation_start(prepared)?;
    session
        .animation_view()
        .active
        .into_iter()
        .next()
        .context("Host has no registered native clock")
}

#[test]
fn dedicated_and_common_local_host_setup_register_the_same_verified_native_clip() -> Result<()> {
    let root = tempfile::tempdir()?;
    bri_net::testing::write_root(root.path(), &[bri_net::testing::MAP])?;
    let packages = PackageSet::load_root(root.path())?;
    let avatar_dir = packages.role_dir(root.path(), "avatar")?;
    let avatar: Package = serde_json::from_slice(&std::fs::read(avatar_dir.join("avatar.json"))?)?;
    let bytes = std::fs::read(avatar_dir.join(&avatar.rig))?;
    let rig: Rig = serde_json::from_slice(&bytes)?;
    let (sequence, clip) = rig
        .sequences
        .iter()
        .find(|(_, clip)| !clip.looping && clip.duration > 0.0)
        .context("No finite fixture sequence")?;
    let make_world = || {
        World::new(
            "Native metadata".into(),
            bri_net::testing::MAP.into(),
            vec![[1.0; 4]],
        )
    };
    let mut dedicated = dedicated::load(root.path(), make_world())?;
    let from_dedicated = admit(&mut dedicated.session, sequence)?;
    let hosted = host_setup::hosted(None, bri_net::testing::MAP, None)?;
    let map = MapSession {
        simulation: common::simulation_with(make_world()),
        spawn_points: vec![glam::Vec3::new(0., 0.05, 0.)],
        breakables: vec![],
        teleporters: vec![],
        volumes: Vec::new(),
        movers: Vec::new(),
        tutorial: None,
    };
    let (mut local, _) = dedicated.setup.session_with(&hosted, map, None)?;
    let from_local = admit(&mut local, sequence)?;
    assert_ne!(from_local.id.session, from_dedicated.id.session);
    assert_eq!(from_local.clip, from_dedicated.clip);
    assert_eq!(
        from_local.clip.digest,
        bri_package::environment::hash_bytes(&bytes)
    );
    assert_eq!(from_local.clip.resource, host_setup::BLOCKHEAD_MODEL);
    assert_eq!(from_local.clip.duration, f64::from(clip.duration));
    Ok(())
}

#[test]
fn loader_checks_actual_bytes_and_current_body_cannot_select_another_models_clip() -> Result<()> {
    let root = tempfile::tempdir()?;
    bri_net::testing::write_root(root.path(), &[bri_net::testing::MAP])?;
    let packages = PackageSet::load_root(root.path())?;
    let avatar_dir = packages.role_dir(root.path(), "avatar")?;
    let avatar: Package = serde_json::from_slice(&std::fs::read(avatar_dir.join("avatar.json"))?)?;
    let weapons_dir = packages.role_dir(root.path(), "weapons")?;
    let weapons = content_identity::WeaponContent::load_with(&weapons_dir, &[])?;
    let presentation = packages.role_dir(root.path(), "item_presentation")?;
    let catalog = content_identity::animation_assets(
        root.path(),
        &packages,
        &avatar_dir,
        &avatar,
        &weapons.pack,
        &presentation,
        &[],
    )?;
    catalog.validate()?;
    let mut damaged = catalog.clone();
    damaged.assets[0].bytes.push(b' ');
    assert!(damaged.validate().is_err());
    let mut wrong = avatar.clone();
    wrong.rig_sha256 = "0".repeat(64);
    assert!(
        content_identity::animation_assets(
            root.path(),
            &packages,
            &avatar_dir,
            &wrong,
            &weapons.pack,
            &presentation,
            &[]
        )
        .is_err()
    );
    let mut host = dedicated::load(
        root.path(),
        World::new(
            "Metadata".into(),
            bri_net::testing::MAP.into(),
            vec![[1.; 4]],
        ),
    )?;
    let owner = host
        .session
        .join("Owner".into(), glam::Vec3::new(0., 0.05, 0.), true)?;
    let before = host.session.animation_view();
    let request: AnimationStart = serde_json::from_value(json!({
        "namespace":"test","nonce":1,"target":{"Pawn":15},"sequence":"unbound_foreign_clip",
        "binding":{"owner":host.session.animation_owner(owner),"item":null,"image":null},"rate":1.,"tween":0.
    }))?;
    assert!(
        host.session
            .prepare_animation_start(owner, request)
            .is_err()
    );
    assert!(host.session.set_animation_assets(damaged).is_err());
    assert_eq!(host.session.animation_view(), before);
    Ok(())
}

#[test]
fn ambiguous_optional_image_clock_does_not_discard_stock_metadata_or_break_hosting() -> Result<()> {
    let root = tempfile::tempdir()?;
    let avatar_dir = root.path().join("avatar");
    bri_content::testing::avatar::write(&avatar_dir)?;
    let avatar_dir = avatar_dir.canonicalize()?;
    let avatar: Package = serde_json::from_slice(&std::fs::read(avatar_dir.join("avatar.json"))?)?;
    let packages = PackageSet {
        schema_version: 1,
        packages: vec![],
    };
    let pack = bri_weapons::Pack::from_json(br#"{"schema_version":5,"id":"metadata","items":{},"images":{"metadata:image/shared":{"model":"actual_model","states":[{"name":"Ready"}]}},"projectiles":{}}"#)?;
    let first = root.path().join("first");
    let second = root.path().join("second");
    for (dir, duration) in [(&first, 1.0), (&second, 2.0)] {
        std::fs::create_dir_all(dir)?;
        let shape = json!({"schema_version":1,"id":"invented","nodes":[],"objects":[],"details":[],"meshes":[],"materials":[],
            "animations":[{"name":"cycle","frames":1,"duration":duration,"looping":false,"additive":false,"priority":0,"nodes":[],"objects":[],"ground_translations":[],"ground_rotations":[],"triggers":[]}]});
        std::fs::write(dir.join("model.json"), shape.to_string())?;
        std::fs::write(dir.join("presentation.json"),json!({"models":{"actual_model":{"file":"model.json"}},"images":{"metadata:image/shared":{"model":"actual_model"}}}).to_string())?;
    }
    let first = first.canonicalize()?;
    let second = second.canonicalize()?;
    let catalog = content_identity::animation_assets(
        root.path(),
        &packages,
        &avatar_dir,
        &avatar,
        &pack,
        &first,
        &[("second".into(), second)],
    )?;
    assert!(!catalog.images.contains_key("metadata:image/shared"));
    assert!(
        catalog
            .assets
            .iter()
            .any(|a| a.resource == host_setup::BLOCKHEAD_MODEL)
    );
    catalog.validate()?;
    let mut session = common::session();
    session.set_weapon_pack(pack)?;
    session.set_animation_assets(catalog)?;
    let rig: Rig = serde_json::from_slice(&std::fs::read(avatar_dir.join(&avatar.rig))?)?;
    let sequence = rig
        .sequences
        .iter()
        .find(|(_, clip)| !clip.looping && clip.duration > 0.)
        .unwrap()
        .0;
    admit(&mut session, sequence)?;
    // More optional image resources than the clock catalog can admit must not
    // remove the verified stock rig or make legacy hosting fail.
    let mut pack_images = serde_json::Map::new();
    let mut presentation_images = serde_json::Map::new();
    for index in 0..257 {
        let id = format!("metadata:image/extra{index}");
        pack_images.insert(
            id.clone(),
            json!({"model":"actual_model","states":[{"name":"Ready"}]}),
        );
        presentation_images.insert(id, json!({"model":"actual_model"}));
    }
    std::fs::write(
        first.join("presentation.json"),
        json!({"models":{"actual_model":{"file":"model.json"}},"images":presentation_images})
            .to_string(),
    )?;
    let extra_pack = bri_weapons::Pack::from_json(&serde_json::to_vec(
        &json!({"schema_version":5,"id":"many-images","items":{},"images":pack_images,"projectiles":{}}),
    )?)?;
    let limited = content_identity::animation_assets(
        root.path(),
        &packages,
        &avatar_dir,
        &avatar,
        &extra_pack,
        &first,
        &[],
    )?;
    assert!(
        limited
            .assets
            .iter()
            .any(|a| a.resource == host_setup::BLOCKHEAD_MODEL)
    );
    assert!(!limited.images.is_empty() && limited.images.len() < extra_pack.images.len());
    limited.validate()?;
    let mut limited_host = common::session();
    limited_host.set_weapon_pack(extra_pack)?;
    limited_host.set_animation_assets(limited)?;
    admit(&mut limited_host, sequence)?;
    Ok(())
}

fn optional_setup(
    root: &std::path::Path,
    extras: &[(String, std::path::PathBuf)],
) -> Result<host_setup::HostSetup> {
    let host = dedicated::load(
        root,
        World::new(
            "Optional metadata".into(),
            bri_net::testing::MAP.into(),
            vec![[1.; 4]],
        ),
    )?;
    let mut setup = (*host.setup).clone();
    let image_pack = bri_weapons::Pack::from_json(br#"{"schema_version":5,"id":"optional","items":{},"images":{"metadata:image/optional":{"model":"Actual_Model","states":[{"name":"Ready"}]}},"projectiles":{}}"#)?;
    setup.content.weapon_pack.images.extend(image_pack.images);
    let packages = PackageSet::load_root(root)?;
    setup.content.animation_assets = content_identity::animation_assets(
        root,
        &packages,
        &packages.role_dir(root, "avatar")?,
        &setup.content.avatar_catalog,
        &setup.content.weapon_pack,
        &packages.role_dir(root, "item_presentation")?,
        extras,
    )?;
    Ok(setup)
}

fn assert_stock_host(setup: &host_setup::HostSetup, root: &std::path::Path) -> Result<()> {
    let hosted = host_setup::hosted(None, bri_net::testing::MAP, None)?;
    let map = MapSession {
        simulation: common::simulation_with(World::new(
            "Optional metadata".into(),
            bri_net::testing::MAP.into(),
            vec![[1.; 4]],
        )),
        spawn_points: vec![glam::Vec3::new(0., 0.05, 0.)],
        breakables: vec![],
        teleporters: vec![],
        volumes: Vec::new(),
        movers: Vec::new(),
        tutorial: None,
    };
    let (mut session, _) = setup.session_with(&hosted, map, None)?;
    let packages = PackageSet::load_root(root)?;
    let bytes = std::fs::read(
        packages
            .role_dir(root, "avatar")?
            .join(&setup.content.avatar_catalog.rig),
    )?;
    let rig: Rig = serde_json::from_slice(&bytes)?;
    let sequence = rig
        .sequences
        .iter()
        .find(|(_, c)| !c.looping && c.duration > 0.)
        .unwrap()
        .0;
    let active = admit(&mut session, sequence)?;
    assert_eq!(
        active.clip.digest,
        bri_package::environment::hash_bytes(&bytes)
    );
    Ok(())
}

#[test]
fn malformed_optional_extra_presentation_keeps_verified_stock_hosting() -> Result<()> {
    let root = tempfile::tempdir()?;
    bri_net::testing::write_root(root.path(), &[bri_net::testing::MAP])?;
    let extra = root.path().join("optional");
    std::fs::create_dir_all(&extra)?;
    std::fs::write(
        extra.join("presentation.json"),
        b"{ malformed optional JSON",
    )?;
    let setup = optional_setup(root.path(), &[("optional".into(), extra.canonicalize()?)])?;
    assert!(
        !setup
            .content
            .animation_assets
            .images
            .contains_key("metadata:image/optional")
    );
    assert_stock_host(&setup, root.path())
}

#[test]
fn optional_extra_clock_requires_actual_image_model_identity() -> Result<()> {
    let root = tempfile::tempdir()?;
    bri_net::testing::write_root(root.path(), &[bri_net::testing::MAP])?;
    let extra = root.path().join("optional");
    std::fs::create_dir_all(&extra)?;
    let extra = extra.canonicalize()?;
    let shape = json!({"schema_version":1,"id":"invented","nodes":[],"objects":[],"details":[],"meshes":[],"materials":[],
        "animations":[{"name":"cycle","frames":1,"duration":1.,"looping":false,"additive":false,"priority":0,"nodes":[],"objects":[],"ground_translations":[],"ground_rotations":[],"triggers":[]}]});
    std::fs::write(extra.join("model.json"), shape.to_string())?;
    for (model, supported) in [("another_model", false), ("actual_model", true)] {
        let mut models = serde_json::Map::new();
        models.insert(model.into(), json!({"file":"model.json"}));
        std::fs::write(
            extra.join("presentation.json"),
            json!({"models":models,"images":{"metadata:image/optional":{"model":model}}})
                .to_string(),
        )?;
        let setup = optional_setup(root.path(), &[("optional".into(), extra.clone())])?;
        assert_eq!(
            setup
                .content
                .animation_assets
                .images
                .contains_key("metadata:image/optional"),
            supported,
            "model {model}"
        );
        assert_eq!(
            setup
                .content
                .animation_assets
                .assets
                .iter()
                .any(|a| a.resource == "metadata:image/optional"),
            supported
        );
        assert_stock_host(&setup, root.path())?;
    }
    Ok(())
}

#[test]
fn malformed_base_presentation_still_fails_existing_mandatory_validation() -> Result<()> {
    let root = tempfile::tempdir()?;
    bri_net::testing::write_root(root.path(), &[bri_net::testing::MAP])?;
    let packages = PackageSet::load_root(root.path())?;
    let weapons = content_identity::WeaponContent::load_with(
        &packages.role_dir(root.path(), "weapons")?,
        &[],
    )?;
    std::fs::write(
        packages
            .role_dir(root.path(), "item_presentation")?
            .join("presentation.json"),
        b"{ malformed mandatory JSON",
    )?;
    assert!(
        content_identity::ItemPhysicsContent::load_with(
            &packages.role_dir(root.path(), "item_presentation")?,
            &weapons,
            &[],
        )
        .is_err()
    );
    assert!(
        dedicated::load(
            root.path(),
            World::new(
                "Mandatory metadata".into(),
                bri_net::testing::MAP.into(),
                vec![[1.; 4]]
            )
        )
        .is_err()
    );
    Ok(())
}
