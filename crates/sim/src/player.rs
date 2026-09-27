//! The shared player motor, with the host's item-contact box.
pub use bri_motor::player::*;

/// The player's authoritative contact box as an item/ball overlap volume.
pub fn item_bounds(player: &Player) -> bri_weapons::ItemBounds {
    let (min, max) = player.world_bounds();
    bri_weapons::ItemBounds { min, max }
}
