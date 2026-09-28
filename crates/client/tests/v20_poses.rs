//! Held item poses pinned to v20's own datablocks, not to our packs: each
//! expectation below is copied from the stock script that defines the image
//! (Torque axes: x right, y forward, z up). `tools/audit_v20_poses.py`
//! checks every field of every stock image the same way.
use anyhow::Result;
use bri_client::items::ItemAssets;
use glam::{Mat4, Vec3};
use std::path::Path;

fn assets() -> Result<ItemAssets> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ItemAssets::load(
        &root.join("item-presentation-pack-010"),
        &root.join("weapons-pack-009"),
    )
}

/// Torque x right, y forward, z up in the eye's frame, where native is
/// X right, Y up, -Z forward.
fn eye_space(right: f32, forward: f32, up: f32) -> Vec3 {
    Vec3::new(right, up, -forward)
}

#[test]
#[ignore = "requires generated item-presentation-pack-010 and weapons-pack-009; CPU only"]
fn first_person_items_sit_right_forward_and_below_the_eye_as_v20_places_them() -> Result<()> {
    let assets = assets()?;
    // `eyeOffset` of each image, from its v20 datablock.
    let table = [
        ("v20.image.hammerimage", (0.7, 1.2, -0.15)),
        ("v20.image.wrenchimage", (0.7, 1.2, -0.15)),
        ("v20.image.printgunimage", (0.7, 1.2, -0.55)),
        ("v20.image.swordimage", (0.7, 1.2, -0.25)),
        ("v20.image.wandimage", (0.7, 1.2, -0.25)),
        ("v20.image.adminwandimage", (0.7, 1.2, -0.25)),
        ("v20.image.bluespraycanimage", (0.7, 1.0, -0.6)),
        ("v20.image.brickimage", (0.7, 1.2, -0.8)),
        ("v20.image.skiweaponimage", (0.7, 1.2, -0.15)),
    ];
    let no_mount = |_| None;
    for (id, (right, forward, up)) in table {
        let m = assets.mount_transform(id, true, Mat4::IDENTITY, no_mount)?;
        let at = m.w_axis.truncate();
        let want = eye_space(right, forward, up);
        assert!(at.distance(want) < 1e-4, "{id}: first person at {at}, v20 {want}");
        // The signs a player sees: to the right, ahead, below the eye.
        assert!(at.x > 0.0 && at.z < 0.0 && at.y < 0.0, "{id}: {at}");
    }
    Ok(())
}

#[test]
#[ignore = "requires generated item-presentation-pack-010 and weapons-pack-009; CPU only"]
fn hand_items_use_v20s_mount_points_and_the_gun_stays_in_the_hand_in_first_person() -> Result<()> {
    let assets = assets()?;
    // A gun has `eyeOffset = 0` in v20, so first person keeps it in the hand.
    let hand = Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0));
    for (id, mount) in [
        ("v20.image.gunimage", 0),
        ("v20.image.rocketlauncherimage", 0),
        ("v20.image.bowimage", 0),
        ("v20.image.spearimage", 0),
        ("v20.image.lefthandedgunimage", 1),
        ("v20.image.basketballimage", 8),
        ("v20.image.soccerballimage", 8),
    ] {
        let asked = std::cell::Cell::new(None);
        assets.mount_transform(id, true, Mat4::IDENTITY, |n| {
            asked.set(Some(n));
            Some(hand)
        })?;
        assert_eq!(asked.get(), Some(mount), "{id}: mountPoint");
    }
    Ok(())
}
