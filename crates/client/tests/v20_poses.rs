//! Held item poses pinned to v20's own datablocks, not to our packs: each
//! expectation below is copied from the stock script that defines the image
//! (Torque axes: x right, y forward, z up). `tools/audit_v20_poses.py`
//! checks every field of every stock image the same way. Those pinned
//! tables need the generated packs; how eye-offset images follow the arm
//! runs on the synthetic item packs (`support::item_fixture`) too.
#[macro_use]
mod support;

use anyhow::Result;
use bri_client::items::ItemAssets;
use glam::{Mat4, Vec3};
use support::item_fixture::ItemFixture;

fn assets() -> Result<ItemAssets> {
    let f = ItemFixture::content()?;
    ItemAssets::load(&f.presentation, &f.weapons)
}

synthetic_and_content!(
    ItemFixture: first_person_eye_offset_images_move_with_the_arms_actions_as_they_do_in_the_hand
);

/// Torque x right, y forward, z up in the eye's frame, where native is
/// X right, Y up, -Z forward.
fn eye_space(right: f32, forward: f32, up: f32) -> Vec3 {
    Vec3::new(right, up, -forward)
}

#[test]
#[ignore = "requires generated v20 content"]
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
        assert!(
            at.distance(want) < 1e-4,
            "{id}: first person at {at}, v20 {want}"
        );
        // The signs a player sees: to the right, ahead, below the eye.
        assert!(at.x > 0.0 && at.z < 0.0 && at.y < 0.0, "{id}: {at}");
    }
    Ok(())
}

#[test]
#[ignore = "requires generated v20 content"]
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

fn first_person_eye_offset_images_move_with_the_arms_actions_as_they_do_in_the_hand(
    f: &ItemFixture,
) -> Result<()> {
    let assets = ItemAssets::load(&f.presentation, &f.weapons)?;
    let eye = Mat4::from_rotation_translation(
        glam::Quat::from_rotation_y(0.4) * glam::Quat::from_rotation_x(-0.3),
        Vec3::new(1.0, 2.0, 3.0),
    );
    let hand = Mat4::from_rotation_translation(
        glam::Quat::from_rotation_x(-1.2),
        Vec3::new(0.3, 1.1, -0.2),
    );
    // A brick shift: the arm pushes out and turns a little.
    let action = Mat4::from_rotation_translation(
        glam::Quat::from_rotation_z(0.2),
        Vec3::new(0.05, -0.1, 0.25),
    );
    // Images held at an eye offset in first person.
    let offset: Vec<String> = if f.content {
        [
            "v20.image.brickimage",
            "v20.image.hammerimage",
            "v20.image.bluespraycanimage",
        ]
        .map(String::from)
        .into()
    } else {
        vec![f.eye_offset_image.clone(), f.euler_image.0.clone()]
    };
    for id in &offset {
        assert_ne!(assets.presentation.images[id].eye_offset, [0.; 3], "{id}");
        let still = assets.mount_transform(id, true, eye, |_| None)?;
        let unmoved = assets.moved_mount_transform(id, true, eye, |_| Some(hand), |_| None)?;
        assert!(
            unmoved.abs_diff_eq(still, 1e-5),
            "{id}: no action, no motion"
        );
        let moved =
            assets.moved_mount_transform(id, true, eye, |_| Some(hand), |_| Some(action))?;
        assert!(!moved.abs_diff_eq(still, 1e-3), "{id}: the action moves it");
        // The same motion, in the image's own frame, as the hand gives it.
        let in_hand = assets.mount_transform(id, false, eye, |_| Some(hand))?;
        let in_hand_moved = assets.mount_transform(id, false, eye, |_| Some(hand * action))?;
        assert!(
            (still.inverse() * moved).abs_diff_eq(in_hand.inverse() * in_hand_moved, 1e-4),
            "{id}"
        );
    }
    // A gun has no eye offset: first person keeps it in the hand, which
    // the action has already moved.
    assert_eq!(
        assets.presentation.images[&f.right_image].eye_offset,
        [0.; 3]
    );
    let gun = assets.moved_mount_transform(
        &f.right_image,
        true,
        eye,
        |_| Some(hand * action),
        |_| Some(action),
    )?;
    let in_hand = assets.mount_transform(&f.right_image, false, eye, |_| Some(hand * action))?;
    assert!(gun.abs_diff_eq(in_hand, 1e-5));
    Ok(())
}
