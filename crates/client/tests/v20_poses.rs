//! Held item poses pinned to v20's own datablocks, not to our lowered
//! presentation: each content test reads the literal `eyeOffset` and
//! `mountPoint` fields the importer keeps from the player's stock scripts
//! (the weapons pack's `definitions`; Torque axes: x right, y forward,
//! z up) and checks every stock image is placed by them.
//! `tools/audit_v20_poses.py` checks every field the same way. How
//! eye-offset images follow the arm runs on the synthetic item packs
//! (`support::item_fixture`) too.
#[macro_use]
mod support;

use anyhow::{Context, Result};
use bri_client::items::ItemAssets;
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;
use support::item_fixture::ItemFixture;

synthetic_and_content!(
    ItemFixture: first_person_eye_offset_images_move_with_the_arms_actions_as_they_do_in_the_hand
);

/// Torque x right, y forward, z up in the eye's frame, where native is
/// X right, Y up, -Z forward.
fn eye_space(right: f32, forward: f32, up: f32) -> Vec3 {
    Vec3::new(right, up, -forward)
}

/// Literal datablock fields of the player's generated weapons pack, with
/// inheritance, keyed by lower-case datablock name.
struct Raw(BTreeMap<String, bri_weapons::Definition>);
impl Raw {
    fn load(f: &ItemFixture) -> Result<Self> {
        let pack = bri_weapons::Pack::from_json(&std::fs::read(f.weapons.join("weapons.json"))?)?;
        Ok(Raw(pack
            .definitions
            .into_iter()
            .map(|d| (d.name.to_ascii_lowercase(), d))
            .collect()))
    }
    fn get(&self, name: &str, key: &str) -> Option<&str> {
        let mut at = self.0.get(name);
        for _ in 0..16 {
            let d = at?;
            if let Some(v) = d.fields.get(&key.to_ascii_lowercase()) {
                return Some(v.trim().trim_matches('"').trim());
            }
            at = d
                .parent
                .as_ref()
                .and_then(|p| self.0.get(&p.to_ascii_lowercase()));
        }
        None
    }
    fn vector(&self, name: &str, key: &str) -> Result<[f32; 3]> {
        let Some(v) = self.get(name, key) else {
            return Ok([0.; 3]);
        };
        let n: Vec<f32> = v
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .with_context(|| format!("{name}.{key} = {v:?}"))?;
        Ok([0, 1, 2].map(|i| n.get(i).copied().unwrap_or(0.)))
    }
    fn int(&self, name: &str, key: &str) -> Result<u32> {
        self.get(name, key).map_or(Ok(0), |v| {
            v.parse().with_context(|| format!("{name}.{key} = {v:?}"))
        })
    }
}

/// Every stock image in the presentation, with its datablock's name.
fn stock_images<'a>(assets: &'a ItemAssets, raw: &Raw) -> Vec<(&'a str, String)> {
    let images: Vec<_> = assets
        .presentation
        .images
        .keys()
        .filter_map(|id| {
            let name = id.strip_prefix("v20.image.")?;
            raw.0
                .contains_key(name)
                .then(|| (id.as_str(), name.to_owned()))
        })
        .collect();
    assert!(
        images.len() > 10,
        "the stock images have no datablocks: {images:?}"
    );
    images
}

#[test]
#[ignore = "requires generated v20 content"]
fn first_person_items_sit_where_their_v20_eye_offset_places_them() -> Result<()> {
    let f = ItemFixture::content()?;
    let assets = ItemAssets::load(&f.presentation, &f.weapons)?;
    let raw = Raw::load(&f)?;
    let mut checked = 0;
    for (id, name) in stock_images(&assets, &raw) {
        let [right, forward, up] = raw.vector(&name, "eyeOffset")?;
        if [right, forward, up] == [0.; 3] {
            continue;
        }
        let m = assets.mount_transform(id, true, Mat4::IDENTITY, |_| None)?;
        let at = m.w_axis.truncate();
        let want = eye_space(right, forward, up);
        assert!(
            at.distance(want) < 1e-4,
            "{id}: first person at {at}, its datablock's eyeOffset {want}"
        );
        checked += 1;
    }
    assert!(checked > 0, "no stock image has an eye offset");
    Ok(())
}

#[test]
#[ignore = "requires generated v20 content"]
fn hand_items_use_their_v20_mount_points_and_stay_in_the_hand_in_first_person() -> Result<()> {
    let f = ItemFixture::content()?;
    let assets = ItemAssets::load(&f.presentation, &f.weapons)?;
    let raw = Raw::load(&f)?;
    // An image with `eyeOffset = 0` (and no eye rotation) is held in the
    // hand in first person too, at its datablock's `mountPoint`.
    let hand = Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0));
    let mut checked = 0;
    for (id, name) in stock_images(&assets, &raw) {
        let image = &assets.presentation.images[id];
        if raw.vector(&name, "eyeOffset")? != [0.; 3] || image.eye_rotation_degrees != [0.; 3] {
            continue;
        }
        let asked = std::cell::Cell::new(None);
        assets.mount_transform(id, true, Mat4::IDENTITY, |n| {
            asked.set(Some(n));
            Some(hand)
        })?;
        assert_eq!(
            asked.get(),
            Some(raw.int(&name, "mountPoint")?),
            "{id}: mountPoint"
        );
        checked += 1;
    }
    assert!(checked > 0, "no stock image is held in the hand");
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
