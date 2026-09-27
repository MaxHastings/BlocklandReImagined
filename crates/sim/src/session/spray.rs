//! `Player::SetTempColor`: colour spray paint recolours the band of the body
//! it hits. `ClearTempColor` runs 2000 ms after the latest hit, pops the
//! paint's explosion at the player and restores the avatar's own colours.
use super::*;
use bri_weapons::{ActorId, TargetId};

/// `SetTempColor(%rgba, 2000, ...)` at 120 ticks per second.
const TEMP_COLOR_TICKS: u64 = 240;
const PAINT_PROJECTILE: &str = "v20.projectile.bluepaintprojectile";

#[derive(Debug, Clone, Default)]
pub(super) struct TempColor {
    colors: BTreeMap<&'static str, [f32; 4]>,
    no_decal: bool,
    paint: u8,
    clear_tick: u64,
}
impl TempColor {
    pub(super) fn apply(&self, appearance: &mut bri_content::avatar::Appearance) {
        for (slot, color) in &self.colors {
            appearance.colors.insert((*slot).into(), *color);
        }
        if self.no_decal {
            appearance.decal = "AAA-None".into();
        }
    }
}

/// `SetTempColor`'s height bands above the feet, as avatar colour slots.
/// Packs, hats and accents only colour when worn. The chest band also
/// removes the decal.
fn band(height: f32, parts: &BTreeMap<String, usize>) -> (Vec<&'static str>, bool) {
    let worn = |slot: &&str| parts.get(*slot).is_some_and(|i| *i > 0);
    if height < 0.63 {
        (vec!["lleg", "rleg"], false)
    } else if height < 1.04 {
        (vec!["hip", "lhand", "rhand"], false)
    } else if height < 1.72 {
        (vec!["torso", "larm", "rarm"], true)
    } else if height < 1.98 {
        (
            ["pack", "secondpack"].into_iter().filter(worn).collect(),
            false,
        )
    } else if height < 2.35 {
        (vec!["head"], false)
    } else {
        (["hat", "accent"].into_iter().filter(worn).collect(), false)
    }
}

impl Session {
    /// `paintProjectile::onCollision` on a player (`PlayerStandardArmor` is
    /// not `paintable`): no trust check, the paint colour at full alpha.
    pub(super) fn spray_player(&mut self, contact: &bri_weapons::ProjectileContact) {
        let (TargetId::Actor(target), Some(paint), PAINT_PROJECTILE) =
            (contact.target, contact.paint, contact.definition.as_str())
        else {
            return;
        };
        let Some(color) = self
            .simulation
            .state()
            .palette
            .get(usize::from(paint))
            .copied()
        else {
            return;
        };
        let tick = self.simulation.state().tick;
        let Some(peer) = self.peers.get_mut(&target.0) else {
            return;
        };
        let Some(avatar) = &peer.avatar else {
            return;
        };
        let height = contact.position.y - peer.player.state().feet[1];
        let (slots, no_decal) = band(height, &avatar.parts);
        let temp = peer.temp_color.get_or_insert_with(Default::default);
        for slot in slots {
            temp.colors
                .insert(slot, [color[0], color[1], color[2], 1.0]);
        }
        temp.no_decal |= no_decal;
        temp.paint = paint;
        temp.clear_tick = tick + TEMP_COLOR_TICKS;
    }

    pub(super) fn step_temp_colors(&mut self) {
        let tick = self.simulation.state().tick;
        for (&owner, peer) in &mut self.peers {
            let Some(temp) = peer.temp_color.take_if(|t| t.clear_tick <= tick) else {
                continue;
            };
            let bounds = peer.player.world_bounds();
            let center = (Vec3::from(bounds.min) + Vec3::from(bounds.max)) * 0.5;
            self.cues.emit(
                tick,
                crate::presentation::CueKind::WeaponEffect {
                    source: TargetId::Actor(ActorId(owner)),
                    definition: bri_weapons::paint_effect("bluePaintExplosion", Some(temp.paint)),
                    node: String::new(),
                    seconds: 0.0,
                    image: None,
                    hand: None,
                    direction: None,
                    scale: 2.0,
                },
                center.to_array(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_follow_set_temp_color_heights() {
        let mut parts = BTreeMap::from([("pack".to_string(), 0), ("hat".to_string(), 2)]);
        assert_eq!(band(0.3, &parts), (vec!["lleg", "rleg"], false));
        assert_eq!(band(0.8, &parts), (vec!["hip", "lhand", "rhand"], false));
        assert_eq!(band(1.5, &parts), (vec!["torso", "larm", "rarm"], true));
        assert_eq!(band(1.9, &parts), (vec![], false));
        parts.insert("secondpack".into(), 1);
        assert_eq!(band(1.9, &parts), (vec!["secondpack"], false));
        assert_eq!(band(2.2, &parts), (vec!["head"], false));
        assert_eq!(band(2.5, &parts), (vec!["hat"], false));
    }
}
