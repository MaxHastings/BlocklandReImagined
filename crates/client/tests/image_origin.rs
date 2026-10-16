//! Invented fixtures for explicit origin; legacy numeric inference remains supported.
use anyhow::{Context, Result};
use bri_client::items::{ImagePresentation, ItemAssets};
use bri_weapons::{FirstPersonOrigin, Image};
use glam::{Mat4, Quat, Vec3};

#[test]
fn image_origin_serde_is_optional_typed_and_round_trips() -> Result<()> {
    let legacy: Image = serde_json::from_str("{}")?;
    assert_eq!(legacy.first_person_origin, None);
    assert!(
        serde_json::to_value(&legacy)?
            .get("first_person_origin")
            .is_none()
    );
    let base = serde_json::json!({
        "model":"", "mount_point":0, "offset":[0,0,0], "eye_offset":[0,0,0],
        "source_rotation_degrees":[0,0,0], "eye_rotation_degrees":[0,0,0],
        "tint":[1,1,1,1], "evidence":{"path":"", "sha256":"", "line":0}
    });
    let presentation: ImagePresentation = serde_json::from_value(base.clone())?;
    assert_eq!(presentation.first_person_origin, None);
    assert!(
        serde_json::to_value(&presentation)?
            .get("first_person_origin")
            .is_none()
    );
    for (name, origin) in [
        ("eye", FirstPersonOrigin::Eye),
        ("mount", FirstPersonOrigin::Mount),
    ] {
        let mut value = base.clone();
        value["first_person_origin"] = name.into();
        let p: ImagePresentation = serde_json::from_value(value.clone())?;
        assert_eq!(p.first_person_origin, Some(origin));
        assert_eq!(serde_json::to_value(p)?["first_person_origin"], name);
        let native: Image = serde_json::from_value(value)?;
        assert_eq!(native.first_person_origin, Some(origin));
        assert_eq!(serde_json::to_value(native)?["first_person_origin"], name);
    }
    for invalid in [
        serde_json::json!("camera"),
        serde_json::json!(true),
        serde_json::json!(7),
    ] {
        let mut value = base.clone();
        value["first_person_origin"] = invalid;
        assert!(serde_json::from_value::<Image>(value.clone()).is_err());
        assert!(serde_json::from_value::<ImagePresentation>(value).is_err());
    }
    Ok(())
}

#[test]
fn image_origin_native_addon_lowers_to_exact_identity_eye_without_host_or_correction() -> Result<()>
{
    let packs = bri_client::testing::items::write_packs()?;
    let dir = tempfile::tempdir()?;
    let id = "invented:image/held";
    let pack = serde_json::json!({"schema_version":bri_weapons::SCHEMA,"id":"invented",
        "images":{id:{"id":id,"first_person_origin":"eye","follow_arm":false,
            "offset":[0.1,0.2,0.3],"source_rotation_degrees":[20.,30.,40.]}}});
    std::fs::write(dir.path().join("weapons.json"), serde_json::to_vec(&pack)?)?;
    let mut assets = ItemAssets::load_with(
        &packs.presentation,
        &packs.weapons,
        &[("invented".into(), dir.path().to_path_buf())],
    )?;
    let image = assets
        .presentation
        .images
        .get_mut(id)
        .context("lowered image")?;
    assert_eq!(image.first_person_origin, Some(FirstPersonOrigin::Eye));
    assert!(!image.follow_arm);
    assert_eq!(image.eye_offset, [0.; 3]);
    // A correction lookup would fail; explicit eye placement must bypass it entirely.
    image.model = "invented:model/not-loaded".into();
    let eye = Mat4::from_rotation_translation(Quat::from_rotation_y(0.6), Vec3::new(10., 5., -3.));
    let placed = assets.mount_transform(id, true, eye, |_| {
        panic!("explicit eye must not query host hand")
    })?;
    assert_eq!(placed, eye);
    Ok(())
}

#[test]
fn image_origin_native_presentation_identity_must_agree() -> Result<()> {
    let packs = bri_client::testing::items::write_packs()?;
    let native_path = packs.weapons.join("weapons.json");
    let mut native: serde_json::Value = serde_json::from_slice(&std::fs::read(native_path)?)?;
    let id = native["images"]
        .as_object()
        .context("images")?
        .keys()
        .next()
        .context("image")?
        .clone();
    native["images"][&id]["first_person_origin"] = "eye".into();
    native["images"][&id]["eye_offset"] = serde_json::json!([0., 0., 0.]);
    native["images"][&id]["eye_rotation"] = serde_json::json!([0., 0., 0.]);
    native["images"][&id]["follow_arm"] = false.into();
    let digest = bri_content::testing::write_file(
        &packs.weapons,
        "weapons.json",
        &serde_json::to_vec(&native)?,
    )?;
    let manifest_path = packs.presentation.join("presentation.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    manifest["weapons_sha256"] = digest.into();
    manifest["images"][&id]["eye_offset"] = serde_json::json!([0., 0., 0.]);
    manifest["images"][&id]["eye_rotation_degrees"] = serde_json::json!([0., 0., 0.]);
    manifest["images"][&id]["follow_arm"] = false.into();
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest)?)?;
    let mismatch = ItemAssets::load_with(&packs.presentation, &packs.weapons, &[])
        .err()
        .context("must reject different origins")?;
    assert!(
        mismatch.to_string().contains("identity/transform mismatch"),
        "{mismatch:#}"
    );
    manifest["images"][&id]["first_person_origin"] = "eye".into();
    std::fs::write(manifest_path, serde_json::to_vec(&manifest)?)?;
    let mut assets = ItemAssets::load_with(&packs.presentation, &packs.weapons, &[])?;
    assert_eq!(
        assets.presentation.images[&id].first_person_origin,
        Some(FirstPersonOrigin::Eye)
    );
    assert!(!assets.presentation.images[&id].follow_arm);
    let eye = Mat4::from_rotation_translation(Quat::from_rotation_x(0.4), Vec3::new(3., 5., 7.));
    let action =
        Mat4::from_rotation_translation(Quat::from_rotation_z(0.2), Vec3::new(0.4, -0.1, 0.2));
    let placed = assets.moved_mount_transform(
        &id,
        true,
        eye,
        |_| panic!("explicit Eye never uses host hand"),
        |_| Some(action),
    )?;
    assert_eq!(
        placed, eye,
        "explicit origin preserves authored false arm policy"
    );
    let (legacy_id, legacy) = assets
        .presentation
        .images
        .iter_mut()
        .find(|(key, _)| *key != &id)
        .context("legacy image")?;
    assert_eq!(legacy.first_person_origin, None);
    assert!(
        legacy.follow_arm,
        "legacy base images still force stock arm following"
    );
    legacy.eye_offset = [0., 0., -0.3];
    legacy.eye_rotation_degrees = [0.; 3];
    let legacy_id = legacy_id.clone();
    let rest = assets.moved_mount_transform(&legacy_id, true, eye, |_| None, |_| None)?;
    let acted = assets.moved_mount_transform(&legacy_id, true, eye, |_| None, |_| Some(action))?;
    assert!(!acted.abs_diff_eq(rest, 1e-5));
    Ok(())
}

#[test]
fn image_origin_addon_presentation_conflict_is_reported_and_native_lowering_retained() -> Result<()>
{
    let packs = bri_client::testing::items::write_packs()?;
    let dir = tempfile::tempdir()?;
    let id = "invented:image/conflict";
    let native = serde_json::to_vec(&serde_json::json!({
        "schema_version":bri_weapons::SCHEMA,"id":"invented",
        "images":{id:{"id":id,"first_person_origin":"eye"}}
    }))?;
    let digest = bri_content::testing::write_file(dir.path(), "weapons.json", &native)?;
    let physics = bri_content::testing::write_file(
        dir.path(),
        "item-physics.json",
        br#"{"schema_version":1,"items":{}}"#,
    )?;
    let mut presentation = serde_json::json!({
        "schema_version":2,"id":"invented", "weapons_sha256":digest,
        "item_physics_sha256":physics,"models":{},"textures":{},"items":{},"projectiles":{},
        "images":{id:{"model":"","mount_point":0,"offset":[0,0,0],"eye_offset":[0,0,0],
            "first_person_origin":"mount","source_rotation_degrees":[0,0,0],
            "eye_rotation_degrees":[0,0,0],"tint":[1,1,1,1],
            "evidence":{"path":"","sha256":"","line":0}}},"diagnostics":[]
    });
    let path = dir.path().join("presentation.json");
    let extras = [("invented".into(), dir.path().to_path_buf())];
    std::fs::write(&path, serde_json::to_vec(&presentation)?)?;
    let rejected = ItemAssets::load_with(&packs.presentation, &packs.weapons, &extras)?;
    assert!(
        rejected
            .faults
            .iter()
            .any(|s| s.contains("origin mismatch")),
        "{:?}",
        rejected.faults
    );
    assert_eq!(
        rejected.presentation.images[id].first_person_origin,
        Some(FirstPersonOrigin::Eye)
    );
    presentation["images"][id]["first_person_origin"] = "eye".into();
    std::fs::write(path, serde_json::to_vec(&presentation)?)?;
    let accepted = ItemAssets::load_with(&packs.presentation, &packs.weapons, &extras)?;
    assert!(
        !accepted
            .faults
            .iter()
            .any(|s| s.contains("origin mismatch"))
    );
    assert_eq!(
        accepted.presentation.images[id].first_person_origin,
        Some(FirstPersonOrigin::Eye)
    );
    Ok(())
}
