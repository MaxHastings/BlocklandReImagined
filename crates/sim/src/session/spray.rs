//! `Player::SetTempColor`: colour spray paint recolours the band of the body
//! it hits, and `Player::burn` blackens the whole body. `ClearTempColor` runs
//! when the latest one ends, pops a spray's paint explosion at the player and
//! restores the avatar's own colours.
use super::*;
use bri_weapons::{ActorId, TargetId};

/// `SetTempColor(%rgba, 2000, ...)` at 120 ticks per second.
const TEMP_COLOR_TICKS: u64 = 240;
const PAINT_PROJECTILE: &str = "v20.projectile.bluepaintprojectile";
/// `setNodeColor("ALL", ...)`: every avatar colour slot.
const ALL_SLOTS: [&str; 13] = [
    "head",
    "torso",
    "hat",
    "accent",
    "pack",
    "secondpack",
    "hip",
    "rarm",
    "larm",
    "rhand",
    "lhand",
    "rleg",
    "lleg",
];

#[derive(Debug, Clone, Default)]
pub(super) struct TempColor {
    colors: BTreeMap<&'static str, [f32; 4]>,
    no_decal: bool,
    /// The spray's palette index; a burn has no explosion.
    paint: Option<u8>,
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

/// What Add-On rules put over a player for a while (`temp_look`): a colour
/// over every slot, and a face with worn parts' opacity. Each lasts its
/// own time, as v20's `SetTempColor` and a `setFaceName` reset schedule did.
#[derive(Debug, Clone, Default)]
pub(super) struct TempLook {
    color: Option<([f32; 4], u64)>,
    face: Option<(Option<String>, BTreeMap<String, f32>, u64)>,
}
impl TempLook {
    pub(super) fn apply(&self, appearance: &mut bri_content::avatar::Appearance) {
        if let Some((color, _)) = self.color {
            for slot in ALL_SLOTS {
                appearance.colors.insert(slot.into(), color);
            }
            appearance.decal = "AAA-None".into();
        }
        if let Some((face, alpha, _)) = &self.face {
            if let Some(face) = face {
                appearance.face = face.clone();
            }
            for (slot, a) in alpha {
                let worn = appearance
                    .parts
                    .get(slot)
                    .is_some_and(|p| !p.eq_ignore_ascii_case("none"));
                if let (true, Some(c)) = (worn, appearance.colors.get_mut(slot)) {
                    c[3] = *a;
                }
            }
        }
    }
    fn expire(&mut self, tick: u64) -> bool {
        if self.color.is_some_and(|(_, at)| at <= tick) {
            self.color = None;
        }
        if self.face.as_ref().is_some_and(|(_, _, at)| *at <= tick) {
            self.face = None;
        }
        self.color.is_none() && self.face.is_none()
    }
}

/// `SetTempColor`'s height bands above the feet, as avatar colour slots.
/// Packs, hats and accents only colour when worn. The chest band also
/// removes the decal.
fn band(height: f32, parts: &BTreeMap<String, String>) -> (Vec<&'static str>, bool) {
    let worn = |slot: &&str| parts.get(*slot).is_some_and(|p| !p.eq_ignore_ascii_case("none"));
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
        temp.paint = Some(paint);
        temp.clear_tick = tick + TEMP_COLOR_TICKS;
    }

    /// `Player::burn`: `SetTempColor("0 0 0 1", %time)` without a position
    /// colours every node black and removes the decal.
    pub(super) fn burn_player(&mut self, owner: OwnerId, seconds: f32) {
        let tick = self.simulation.state().tick;
        let Some(peer) = self.peers.get_mut(&owner) else {
            return;
        };
        let temp = peer.temp_color.get_or_insert_with(Default::default);
        for slot in ALL_SLOTS {
            temp.colors.insert(slot, [0.0, 0.0, 0.0, 1.0]);
        }
        temp.no_decal = true;
        temp.paint = None;
        temp.clear_tick = tick + (seconds.clamp(0.0, 300.0) * 120.0).ceil() as u64;
    }

    /// `Player::clearBurn`: a burn's black ends early; paint is left alone.
    pub(super) fn clear_burn(&mut self, owner: OwnerId) {
        if let Some(peer) = self.peers.get_mut(&owner)
            && peer.temp_color.as_ref().is_some_and(|t| t.no_decal)
        {
            peer.temp_color = None;
        }
    }

    /// `temp_look`: a colour replaces the last colour, a face or opacity
    /// the last face, each for `seconds`.
    pub(super) fn temp_look(
        &mut self,
        owner: OwnerId,
        look: bri_package_runtime::ops::TempLook,
        seconds: f32,
    ) {
        let tick = self.simulation.state().tick;
        let until = tick + (seconds.clamp(0.0, 60.0) * 120.0).ceil() as u64;
        let palette = &self.simulation.state().palette;
        let color = look.color.or_else(|| {
            look.paint
                .and_then(|p| palette.get(usize::from(p)).copied())
        });
        let Some(peer) = self.peers.get_mut(&owner) else {
            return;
        };
        let temp = peer.temp_look.get_or_insert_with(Default::default);
        if let Some(color) = color {
            temp.color = Some((color, until));
        }
        if look.face.is_some() || !look.alpha.is_empty() {
            temp.face = Some((look.face, look.alpha, until));
        }
    }

    pub(super) fn step_temp_colors(&mut self) {
        let tick = self.simulation.state().tick;
        for peer in self.peers.values_mut() {
            if peer.temp_look.as_mut().is_some_and(|l| l.expire(tick)) {
                peer.temp_look = None;
            }
        }
        for (&owner, peer) in &mut self.peers {
            let Some(paint) = peer
                .temp_color
                .take_if(|t| t.clear_tick <= tick)
                .and_then(|t| t.paint)
            else {
                continue;
            };
            let bounds = crate::player::item_bounds(&peer.player);
            let center = (Vec3::from(bounds.min) + Vec3::from(bounds.max)) * 0.5;
            self.cues.emit(
                tick,
                crate::presentation::CueKind::WeaponEffect {
                    source: TargetId::Actor(ActorId(owner)),
                    definition: bri_weapons::paint_effect("bluePaintExplosion", Some(paint)),
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
        let mut parts = BTreeMap::from([("pack".to_string(), "none".to_string()), ("hat".to_string(), "helmet".to_string())]);
        assert_eq!(band(0.3, &parts), (vec!["lleg", "rleg"], false));
        assert_eq!(band(0.8, &parts), (vec!["hip", "lhand", "rhand"], false));
        assert_eq!(band(1.5, &parts), (vec!["torso", "larm", "rarm"], true));
        assert_eq!(band(1.9, &parts), (vec![], false));
        parts.insert("secondpack".into(), "quiver".into());
        assert_eq!(band(1.9, &parts), (vec!["secondpack"], false));
        assert_eq!(band(2.2, &parts), (vec!["head"], false));
        assert_eq!(band(2.5, &parts), (vec!["hat"], false));
    }

    /// `temp_look`'s colour covers every slot and the decal; its face and
    /// opacity reach only parts worn; each ends at its own time.
    #[test]
    fn a_temp_look_colours_faces_and_fades_worn_parts_for_its_time() {
        let mut appearance = bri_content::avatar::Appearance {
            parts: BTreeMap::from([
                ("accent".to_string(), "visor".to_string()),
                ("hat".to_string(), "none".to_string()),
            ]),
            colors: BTreeMap::from([
                ("accent".to_string(), [0.2, 0.3, 0.4, 1.0]),
                ("hat".to_string(), [1.0; 4]),
            ]),
            face: "smiley".into(),
            decal: "Alyx".into(),
        };
        let mut look = TempLook {
            color: None,
            face: Some((
                Some("memeCats".into()),
                BTreeMap::from([("accent".into(), 0.7), ("hat".into(), 0.5)]),
                100,
            )),
        };
        let mut faced = appearance.clone();
        look.apply(&mut faced);
        assert_eq!(faced.face, "memeCats");
        assert_eq!(faced.colors["accent"], [0.2, 0.3, 0.4, 0.7]);
        assert_eq!(faced.colors["hat"], [1.0; 4], "no hat is worn");
        assert_eq!(faced.decal, "Alyx");
        look.color = Some(([0.9, 0.1, 0.1, 1.0], 50));
        let mut painted = appearance.clone();
        look.apply(&mut painted);
        assert_eq!(painted.colors.len(), ALL_SLOTS.len());
        assert_eq!(painted.colors["head"], [0.9, 0.1, 0.1, 1.0]);
        assert_eq!(painted.decal, "AAA-None");
        assert!(!look.expire(50), "the face lasts longer");
        look.apply(&mut appearance);
        assert_eq!(appearance.face, "memeCats");
        assert_eq!(appearance.decal, "Alyx");
        assert!(look.expire(100));
    }

    #[test]
    fn burning_blackens_every_slot_until_the_burn_ends() -> Result<()> {
        use rapier3d::prelude::*;
        let simulation = crate::simulation::Simulation::new(
            bri_world::World::new("Burn".into(), "test".into(), vec![[1.0; 4]]),
            crate::definitions::Definitions::default(),
            vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        )?;
        let mut s = Session::new(simulation);
        let colors: BTreeMap<_, _> = ALL_SLOTS.iter().map(|k| (*k, [0.5; 4])).collect();
        let none = |f: &str| serde_json::json!({"file": f, "sha256": "", "source": "", "width": 1, "height": 1});
        s.set_avatar_catalog(serde_json::from_value(serde_json::json!({
            "schema_version": 1, "id": "test", "rig": "rig.json", "rig_sha256": "",
            "parts": {"hat": ["none"], "accent": ["none"], "pack": ["none"],
                "secondpack": ["none"], "chest": ["chest"], "hip": ["pants"],
                "rarm": ["rarm"], "larm": ["larm"], "rhand": ["rhand"],
                "lhand": ["lhand"], "rleg": ["rshoe"], "lleg": ["lshoe"]},
            "accents_allowed": {}, "faces": ["smiley"], "decals": ["AAA-None", "Alyx"],
            "surfaces": {}, "textures": {"smiley": none("s.png"), "Alyx": none("a.png"),
                "AAA-None": none("n.png")},
            "defaults": {"parts": {}, "colors": colors, "face": "smiley", "decal": "Alyx"}
        }))?)?;
        let owner = s.join("Burning".into(), Vec3::new(0., 0.05, 0.), false)?;
        let own = s.avatars()[&owner].clone();
        s.burn_player(owner, 1.0);
        let burnt = &s.avatars()[&owner];
        assert!(burnt.colors.values().all(|c| *c == [0.0, 0.0, 0.0, 1.0]));
        assert_eq!(burnt.colors.len(), ALL_SLOTS.len());
        assert_eq!(burnt.decal, "AAA-None");
        for _ in 0..119 {
            s.step()?;
        }
        assert_ne!(s.avatars()[&owner], own);
        s.take_cues();
        s.step()?;
        s.step()?;
        assert_eq!(s.avatars()[&owner], own);
        // A burn's `ClearTempColor` has no paint projectile to explode.
        assert!(!s.take_cues().iter().any(|c| matches!(
            &c.kind,
            crate::presentation::CueKind::WeaponEffect { definition, .. }
                if definition.contains("PaintExplosion")
        )));
        Ok(())
    }
}
