//! What an Add-On's weapon data names that is resolved somewhere else: the
//! sounds, effects, damage types and models its items, images and
//! projectiles use. The client checks each against what it actually loaded
//! (`bri_package::health`), so a name nothing provides is reported when the
//! game loads rather than doing nothing the first time it is used.
use crate::Pack;
use bri_package::health::Kind;

/// The Add-On namespace of a content id (`namespace:kind/name`); `None` for
/// the base game's ids (`v20.kind.name`).
pub fn add_on_of(id: &str) -> Option<&str> {
    id.split_once(':')
        .map(|(namespace, _)| namespace)
        .filter(|namespace| !namespace.is_empty())
}

/// One name an Add-On's weapon data uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference<'a> {
    /// The namespace of the item, image or projectile that names it.
    pub add_on: &'a str,
    pub kind: Kind,
    pub name: &'a str,
    /// What names it, for the report: `image x state Fire`.
    pub used_by: String,
}

impl Pack {
    /// Every sound, effect and damage type the Add-On items, images and
    /// projectiles of this (merged) pack name, the base game's own entries
    /// aside: content tests cover those. Images and projectiles are listed
    /// by `Pack::merge`, which drops what names a missing one.
    pub fn add_on_references(&self) -> Vec<Reference<'_>> {
        let mut out = Vec::new();
        for (id, image) in &self.images {
            let Some(add_on) = add_on_of(id) else {
                continue;
            };
            for state in &image.states {
                let used_by = || format!("image {id} state {}", state.name);
                if !state.sound.is_empty() {
                    out.push(Reference {
                        add_on,
                        kind: Kind::Sound,
                        name: &state.sound,
                        used_by: used_by(),
                    });
                }
                if !state.emitter.is_empty() {
                    out.push(Reference {
                        add_on,
                        kind: Kind::Effect,
                        name: &state.emitter,
                        used_by: used_by(),
                    });
                }
            }
        }
        for (id, projectile) in &self.projectiles {
            let Some(add_on) = add_on_of(id) else {
                continue;
            };
            let used_by = || format!("projectile {id}");
            for (kind, name) in [
                (Kind::Sound, projectile.sound.as_str()),
                (Kind::Effect, projectile.trail.as_str()),
                (Kind::Effect, projectile.bounce_effect.as_str()),
                (Kind::Effect, projectile.stick_effect.as_str()),
                (Kind::Effect, projectile.blood_effect.as_str()),
                (Kind::Explosion, projectile.explosion.effect.as_str()),
                (Kind::DamageType, projectile.damage_type.as_str()),
                (Kind::DamageType, projectile.radius_damage_type.as_str()),
            ] {
                // A bounce that only makes a sound (Explosive 1's and the
                // HE grenade's) has nothing to show; its sound is listed.
                if kind == Kind::Effect
                    && let Some(info) = self.sound_only_explosion(name)
                {
                    out.push(Reference {
                        add_on,
                        kind: Kind::Sound,
                        name: &info.sound,
                        used_by: format!("explosion {name}"),
                    });
                    continue;
                }
                if !name.trim().is_empty() {
                    out.push(Reference {
                        add_on,
                        kind,
                        name,
                        used_by: used_by(),
                    });
                }
            }
            // The explosion's own sound, as its projectile's Add-On uses it.
            let explosion = &projectile.explosion.effect;
            if let Some(info) = self
                .explosions
                .get(&crate::effect_symbol(explosion).to_ascii_lowercase())
                && !info.sound.is_empty()
            {
                out.push(Reference {
                    add_on,
                    kind: Kind::Sound,
                    name: &info.sound,
                    used_by: format!("explosion {explosion}"),
                });
            }
        }
        out
    }
    /// The explosion `name` names when it plays only a sound: no shape
    /// and no sizes to draw.
    fn sound_only_explosion(&self, name: &str) -> Option<&crate::ExplosionInfo> {
        self.explosions
            .get(&crate::effect_symbol(name).to_ascii_lowercase())
            .filter(|e| e.shape.is_empty() && e.sizes.is_empty() && !e.sound.is_empty())
    }
    /// Whether a `$DamageType::<name>` reference names a type the pack
    /// has, rather than falling back to `Default` ([`Pack::damage_type`]).
    pub fn has_damage_type(&self, reference: &str) -> bool {
        self.damage_types.contains_key(&damage_type_key(reference))
    }
}

/// The key of a `$DamageType::<name>` reference in [`Pack::damage_types`].
pub(crate) fn damage_type_key(reference: &str) -> String {
    let name = reference.trim();
    let name = match name.get(..13) {
        Some(prefix) if prefix.eq_ignore_ascii_case("$damagetype::") => &name[13..],
        _ => name,
    };
    name.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_add_on_image_names_its_sounds_and_effects_and_the_base_game_none() {
        let mut pack = crate::testing::pack();
        let mut base = pack.images.values().next().unwrap().clone();
        base.id = crate::native_id("image", "gunImage");
        base.states[0].sound = "gunShot".into();
        pack.images.insert(base.id.clone(), base);
        let mut image = pack.images.values().next().unwrap().clone();
        image.id = "sniper:image/rifle".into();
        image.states[0].sound = "SniperShot".into();
        image.states[0].emitter = "sniperSmokeEmitter".into();
        pack.images.insert(image.id.clone(), image.clone());
        let all = pack.add_on_references();
        assert!(
            !all.iter().any(|r| r.name == "gunShot"),
            "base entries are not listed"
        );
        let found: Vec<(Kind, &str)> = all
            .into_iter()
            .filter(|r| r.add_on == "sniper")
            .map(|r| (r.kind, r.name))
            .collect();
        assert_eq!(
            found,
            [
                (Kind::Sound, "SniperShot"),
                (Kind::Effect, "sniperSmokeEmitter")
            ]
        );
    }

    /// Max's v0.1.12 log: Explosive 1's frag bounce "shows nothing". It
    /// is an explosion that only plays a sound, so its sound is what to
    /// check.
    #[test]
    fn a_sound_only_bounce_is_checked_for_its_sound_not_an_effect() {
        let mut pack = crate::testing::pack();
        let mut round = pack.projectiles.values().next().unwrap().clone();
        round.id = "tier:projectile/frag".into();
        round.bounce_effect = "tierFragPortBounce".into();
        pack.projectiles.insert(round.id.clone(), round);
        let mut bounce = pack.explosions.values().next().unwrap().clone();
        bounce.name = "tierFragPortBounce".into();
        bounce.sound = "tier:sound/bounce".into();
        bounce.shape.clear();
        bounce.sizes.clear();
        pack.explosions.insert("tierfragportbounce".into(), bounce);
        let all = pack.add_on_references();
        assert!(
            !all.iter().any(|r| r.name == "tierFragPortBounce"),
            "{all:?}"
        );
        assert!(
            all.iter()
                .any(|r| r.kind == Kind::Sound && r.name == "tier:sound/bounce")
        );
    }

    #[test]
    fn an_unknown_damage_type_is_not_had_though_it_falls_back() {
        let pack = crate::testing::pack();
        let known = pack.damage_types.keys().next().unwrap().clone();
        assert!(pack.has_damage_type(&format!("$DamageType::{known}")));
        assert!(!pack.has_damage_type("$DamageType::NoSuchGun"));
        assert_eq!(add_on_of("sniper:image/rifle"), Some("sniper"));
        assert_eq!(add_on_of("v20.image.gunimage"), None);
    }
}
