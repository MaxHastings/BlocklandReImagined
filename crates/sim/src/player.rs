//! The shared player motor, with the host's item-contact box.
pub use bri_motor::player::*;

/// The player's authoritative contact box as an item/ball overlap volume.
pub fn item_bounds(player: &Player) -> bri_weapons::ItemBounds {
    let (min, max) = player.world_bounds();
    bri_weapons::ItemBounds { min, max }
}

/// Torque's `PlayerData` defaults for `getDamageLocation`: a point in the
/// top `1 - HEAD` of the body's box is the head, one below `TORSO` of it
/// the legs, anything between the torso. v20's armours leave them unset.
pub const HEAD_FRACTION: f32 = 0.85;
pub const TORSO_FRACTION: f32 = 0.55;

/// Which part of `player` a hit at `point` struck, `getDamageLocation`'s
/// first word: `"head"`, `"torso"` or `"legs"`, by the point's height in
/// the body's box as it stands now (a crouched body's box is shorter).
/// Points above or below the box count as the nearest part.
pub fn hit_region(player: &Player, point: [f32; 3]) -> &'static str {
    let (min, max) = player.world_bounds();
    region_in(min[1], max[1], point[1])
}

/// [`hit_region`] for a box from `bottom` to `top`.
pub fn region_in(bottom: f32, top: f32, y: f32) -> &'static str {
    let height = top - bottom;
    let up = if height > 0.0 {
        (y - bottom) / height
    } else {
        0.0
    };
    if up > HEAD_FRACTION {
        "head"
    } else if up > TORSO_FRACTION {
        "torso"
    } else {
        "legs"
    }
}

#[cfg(test)]
mod region_tests {
    use super::region_in;

    #[test]
    fn bands_follow_torque_defaults() {
        // A 2.65 unit tall standing Blockhead with its feet at 10.
        let (bottom, top) = (10.0, 12.65);
        assert_eq!(region_in(bottom, top, 12.6), "head");
        assert_eq!(region_in(bottom, top, 10.0 + 2.65 * 0.86), "head");
        assert_eq!(region_in(bottom, top, 10.0 + 2.65 * 0.84), "torso");
        assert_eq!(region_in(bottom, top, 10.0 + 2.65 * 0.56), "torso");
        assert_eq!(region_in(bottom, top, 10.0 + 2.65 * 0.54), "legs");
        assert_eq!(region_in(bottom, top, 9.0), "legs", "below the feet");
        assert_eq!(region_in(bottom, top, 20.0), "head", "above the head");
        assert_eq!(
            region_in(bottom, bottom, bottom),
            "legs",
            "a flat box, as Torque's zero height"
        );
    }
}
