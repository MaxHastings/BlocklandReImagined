//! One weapons pack built from every package that provides weapons: the
//! base game's pack first, then each other package in `packages.json` order.
//! Systems keep taking a single `Pack`; only loading changes.
use crate::{Pack, Resource, SoundDef};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The directory a sound's `file` is relative to, as [`resource_root`].
pub fn sound_root(root: &Path, sound: &SoundDef) -> PathBuf {
    match &sound.package {
        Some(dir) => root.parent().unwrap_or(root).join(dir),
        None => root.to_path_buf(),
    }
}

/// The directory a resource's `native_file` is relative to: its package's,
/// when the pack was merged, else the pack's own `root`. Base packages sit
/// directly under the content root, so a package directory resolves beside
/// `root`.
pub fn resource_root(root: &Path, resource: &Resource) -> PathBuf {
    match &resource.package {
        Some(dir) => root.parent().unwrap_or(root).join(dir),
        None => root.to_path_buf(),
    }
}

impl Pack {
    /// [`Pack::merge_with`] for packages that depend on none of each other.
    pub fn merge(self, parts: Vec<(String, Pack)>) -> (Pack, Vec<String>) {
        self.merge_with(parts, &BTreeMap::new())
    }

    /// Adds `parts`, each with the content-root-relative directory of its
    /// package, to this (the base) pack. Ids are namespaced, so a duplicate
    /// means two packages claim the same content: the first keeps it.
    /// References that no merged package satisfies drop only the item that
    /// needs them. Every skip is returned as a diagnostic; the result still
    /// needs `validate`.
    ///
    /// Damage types and explosions are still named as Torque named them
    /// (`$DamageType::gunHeadshot`), so two Add-Ons can declare one name
    /// differently. Each keeps its own: see [`scope_names`]. `depends_on`
    /// maps a package (a pack's `id`) to the packages it depends on, which
    /// decides when a later declaration replaces an earlier one as in v20.
    pub fn merge_with(
        mut self,
        parts: Vec<(String, Pack)>,
        depends_on: &BTreeMap<String, BTreeSet<String>>,
    ) -> (Pack, Vec<String>) {
        let mut notes = Vec::new();
        // Which package declared each damage type and explosion name.
        let mut owners: BTreeMap<(&'static str, String), String> = BTreeMap::new();
        for key in self.damage_types.keys() {
            owners.insert(("damage type", key.clone()), self.id.clone());
        }
        for key in self.explosions.keys() {
            owners.insert(("explosion", key.clone()), self.id.clone());
        }
        for (dir, mut part) in parts {
            scope_names(
                &mut self,
                &mut part,
                &mut owners,
                depends_on,
                &dir,
                &mut notes,
            );
            fn add<T>(
                into: &mut std::collections::BTreeMap<String, T>,
                from: std::collections::BTreeMap<String, T>,
                what: &str,
                dir: &str,
                notes: &mut Vec<String>,
            ) {
                for (key, value) in from {
                    match into.entry(key) {
                        std::collections::btree_map::Entry::Occupied(e) => notes.push(format!(
                            "{dir}: {what} {} is already declared; kept the earlier one",
                            e.key()
                        )),
                        std::collections::btree_map::Entry::Vacant(e) => {
                            e.insert(value);
                        }
                    }
                }
            }
            add(&mut self.items, part.items, "item", &dir, &mut notes);
            add(&mut self.images, part.images, "image", &dir, &mut notes);
            add(
                &mut self.projectiles,
                part.projectiles,
                "projectile",
                &dir,
                &mut notes,
            );
            // Keyed by Torque name: `scope_names` settled the clashes.
            add(
                &mut self.damage_types,
                part.damage_types,
                "damage type",
                &dir,
                &mut notes,
            );
            add(
                &mut self.explosions,
                part.explosions,
                "explosion",
                &dir,
                &mut notes,
            );
            let sounds = part
                .sounds
                .into_iter()
                .map(|(key, mut sound)| {
                    sound.package.get_or_insert_with(|| dir.clone());
                    (key, sound)
                })
                .collect();
            // Keyed by bare profile name, like damage types.
            add(&mut self.sounds, sounds, "sound", &dir, &mut notes);
            self.external_projectiles.extend(part.external_projectiles);
            merge_effects(&mut self.effects, part.effects, &dir, &mut notes);
            self.definitions.extend(part.definitions);
            self.resources
                .extend(part.resources.into_iter().map(|mut r| {
                    r.package.get_or_insert_with(|| dir.clone());
                    r
                }));
            self.diagnostics
                .extend(part.diagnostics.into_iter().map(|d| format!("{dir}: {d}")));
            self.id = format!("{}+{}", self.id, part.id);
        }
        // Drop what a missing package would have provided, innermost first.
        // A projectile a part took from a package it depends on is now
        // either here or missing.
        self.external_projectiles.clear();
        let projectiles = &self.projectiles;
        self.images.retain(|id, image| {
            let missing = image
                .projectile_refs()
                .find(|p| !projectiles.contains_key(*p));
            if let Some(p) = missing {
                notes.push(format!(
                    "image {id} dropped: projectile {p} is not provided"
                ));
            }
            missing.is_none()
        });
        for p in self.projectiles.values_mut() {
            if let Some(image) = &p.sport_image
                && !self.images.contains_key(image)
            {
                notes.push(format!(
                    "projectile {} loses sport image {image}: not provided",
                    p.id
                ));
                p.sport_image = None;
            }
        }
        let images = &self.images;
        self.items.retain(|id, item| {
            let keep = item.image.is_empty() || images.contains_key(&item.image);
            if !keep {
                notes.push(format!(
                    "item {id} dropped: image {} is not provided",
                    item.image
                ));
            }
            keep
        });
        (self, notes)
    }
}

/// Whether package `a` depends on `b`, directly or through others.
fn depends(depends_on: &BTreeMap<String, BTreeSet<String>>, a: &str, b: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut next = vec![a];
    while let Some(p) = next.pop() {
        for d in depends_on.get(p).into_iter().flatten() {
            if d == b {
                return true;
            }
            if seen.insert(d.as_str()) {
                next.push(d);
            }
        }
    }
    false
}

/// Settles `part`'s damage types and explosions whose names an earlier
/// package (or the base game) already declared differently, before they
/// are added:
///
/// - a package that depends on the earlier one re-declares the name on
///   purpose, and as in v20, where the Add-On loaded last sets the
///   datablock's fields, its declaration replaces the earlier one for
///   everyone;
/// - otherwise each keeps its own: the part's is kept as
///   `<package>:<name>`, and its own projectiles and guards name that.
///   Hooks still see the bare name (`DamageKind::hook_type`), and a
///   package's rules naming the bare name get the one of their own
///   package or one it depends on.
///
/// The same declaration twice is no clash.
fn scope_names(
    into: &mut Pack,
    part: &mut Pack,
    owners: &mut BTreeMap<(&'static str, String), String>,
    depends_on: &BTreeMap<String, BTreeSet<String>>,
    dir: &str,
    notes: &mut Vec<String>,
) {
    let package = part.id.clone();
    let mut renamed: BTreeMap<String, String> = BTreeMap::new();
    let types = std::mem::take(&mut part.damage_types);
    for (key, mut t) in types {
        let owner = owners.get(&("damage type", key.clone())).cloned();
        match (into.damage_types.get(&key), owner) {
            (Some(earlier), Some(owner)) if *earlier != t => {
                if depends(depends_on, &package, &owner) {
                    notes.push(format!(
                        "{dir}: damage type {key} replaces {owner}'s, as {package} depends on it"
                    ));
                    into.damage_types.insert(key.clone(), t);
                    owners.insert(("damage type", key), package.clone());
                } else {
                    let scoped = format!("{package}:{}", t.name);
                    t.name.clone_from(&scoped);
                    let scoped_key = scoped.to_ascii_lowercase();
                    notes.push(format!(
                        "{dir}: damage type {key} is also {owner}'s; {package}'s is kept as {scoped}"
                    ));
                    renamed.insert(key, scoped.clone());
                    owners.insert(("damage type", scoped_key.clone()), package.clone());
                    part.damage_types.insert(scoped_key, t);
                }
            }
            (Some(_), _) => {}
            (None, _) => {
                owners.insert(("damage type", key.clone()), package.clone());
                part.damage_types.insert(key, t);
            }
        }
    }
    let explosions = std::mem::take(&mut part.explosions);
    for (key, mut e) in explosions {
        let owner = owners.get(&("explosion", key.clone())).cloned();
        match (into.explosions.get(&key), owner) {
            (Some(earlier), Some(owner)) if *earlier != e => {
                if depends(depends_on, &package, &owner) {
                    notes.push(format!(
                        "{dir}: explosion {key} replaces {owner}'s, as {package} depends on it"
                    ));
                    into.explosions.insert(key.clone(), e);
                    owners.insert(("explosion", key), package.clone());
                } else {
                    e.name = format!("{package}:{}", e.name);
                    let scoped_key = e.name.to_ascii_lowercase();
                    notes.push(format!(
                        "{dir}: explosion {key} is also {owner}'s; {package}'s is kept as {}",
                        e.name
                    ));
                    owners.insert(("explosion", scoped_key.clone()), package.clone());
                    part.explosions.insert(scoped_key, e);
                }
            }
            (Some(_), _) => {}
            (None, _) => {
                owners.insert(("explosion", key.clone()), package.clone());
                part.explosions.insert(key, e);
            }
        }
    }
    if renamed.is_empty() {
        return;
    }
    // The part's own references to a renamed type, as `$DamageType::<name>`.
    let rename = |name: &mut String| {
        let bare = name.trim();
        let bare = match bare.get(..13) {
            Some(prefix) if prefix.eq_ignore_ascii_case("$damagetype::") => &bare[13..],
            _ => return,
        };
        if let Some(scoped) = renamed.get(&bare.to_ascii_lowercase()) {
            *name = format!("$DamageType::{scoped}");
        }
    };
    for p in part.projectiles.values_mut() {
        rename(&mut p.damage_type);
        rename(&mut p.radius_damage_type);
        if let Some(aura) = &mut p.aura {
            rename(&mut aura.damage_type);
        }
    }
    for image in part.images.values_mut() {
        if let Some(kill) = image.guard.as_mut().and_then(|g| g.reflect_kill.as_mut())
            && let Some(scoped) = renamed.get(&kill.to_ascii_lowercase())
        {
            kill.clone_from(scoped);
        }
    }
}

/// Add one package's effects; an id already present keeps the first
/// package's definition.
fn merge_effects(
    into: &mut crate::PackEffects,
    part: crate::PackEffects,
    dir: &str,
    notes: &mut Vec<String>,
) {
    fn add<T>(
        into: &mut Vec<T>,
        part: Vec<T>,
        id: impl Fn(&T) -> &str,
        dir: &str,
        notes: &mut Vec<String>,
    ) {
        for item in part {
            if into.iter().any(|x| id(x).eq_ignore_ascii_case(id(&item))) {
                notes.push(format!("{dir}: effect {} is already defined", id(&item)));
            } else {
                into.push(item);
            }
        }
    }
    add(&mut into.particles, part.particles, |p| &p.id, dir, notes);
    add(&mut into.emitters, part.emitters, |e| &e.id, dir, notes);
    add(&mut into.lights, part.lights, |l| &l.id, dir, notes);
    add(&mut into.explosions, part.explosions, |e| &e.id, dir, notes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    /// A package `id` with one gun round dealing `$DamageType::Headshot`,
    /// whose kill reads `message`.
    fn part(id: &str, message: &str) -> Pack {
        let base = testing::pack();
        let mut part = base.clone();
        part.id = id.into();
        part.items.clear();
        part.images.clear();
        part.projectiles.clear();
        part.damage_types.clear();
        part.explosions.clear();
        part.sounds.clear();
        part.effects = Default::default();
        part.definitions.clear();
        part.resources.clear();
        let mut round = base.projectiles[testing::GUN_PROJECTILE].clone();
        round.id = format!("{id}:projectile/round");
        round.damage_type = "$DamageType::Headshot".into();
        part.projectiles.insert(round.id.clone(), round);
        let mut t = base.damage_types.values().next().unwrap().clone();
        t.name = "Headshot".into();
        t.murder_message = message.into();
        part.damage_types.insert("headshot".into(), t);
        part
    }

    fn message(pack: &Pack, projectile: &str) -> String {
        let p = &pack.projectiles[projectile];
        pack.damage_type(&p.damage_type)
            .unwrap()
            .murder_message
            .clone()
    }

    /// Two Add-Ons that name a damage type alike each keep theirs, unless
    /// one depends on the other: then the later replaces it, as in v20.
    #[test]
    fn a_damage_type_two_add_ons_declare_is_each_ones_own_unless_one_depends_on_the_other() {
        let parts = || {
            vec![
                ("mwb/assets".to_owned(), part("mwb", "%2 shot %1 (mwb)")),
                ("kai/assets".to_owned(), part("kai", "%2 shot %1 (kai)")),
            ]
        };
        let (both, notes) = testing::pack().merge(parts());
        both.validate().unwrap();
        assert_eq!(message(&both, "mwb:projectile/round"), "%2 shot %1 (mwb)");
        assert_eq!(message(&both, "kai:projectile/round"), "%2 shot %1 (kai)");
        assert_eq!(
            both.projectiles["kai:projectile/round"].damage_type,
            "$DamageType::kai:Headshot"
        );
        assert!(
            notes
                .iter()
                .any(|n| n.contains("kai's is kept as kai:Headshot")),
            "{notes:?}"
        );

        let depends = BTreeMap::from([("kai".to_owned(), BTreeSet::from(["mwb".to_owned()]))]);
        let (replaced, notes) = testing::pack().merge_with(parts(), &depends);
        replaced.validate().unwrap();
        assert_eq!(
            message(&replaced, "mwb:projectile/round"),
            "%2 shot %1 (kai)"
        );
        assert_eq!(
            message(&replaced, "kai:projectile/round"),
            "%2 shot %1 (kai)"
        );
        assert!(
            notes.iter().any(|n| n.contains("replaces mwb's")),
            "{notes:?}"
        );

        // The same declaration twice is no clash.
        let (same, notes) = testing::pack().merge(vec![
            ("a/assets".to_owned(), part("a", "%2 shot %1")),
            ("b/assets".to_owned(), part("b", "%2 shot %1")),
        ]);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(
            same.projectiles["b:projectile/round"].damage_type,
            "$DamageType::Headshot"
        );
    }
}
