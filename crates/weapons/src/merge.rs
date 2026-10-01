//! One weapons pack built from every package that provides weapons: the
//! base game's pack first, then each other package in `packages.json` order.
//! Systems keep taking a single `Pack`; only loading changes.
use crate::{Pack, Resource, SoundDef, add_on_of};
use bri_package::health::{Kind, Problem, package_folder};
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
    /// Adds `parts`, each with the content-root-relative directory of its
    /// package, to this (the base) pack. Ids are namespaced, so a duplicate
    /// means two packages claim the same content: the first keeps it.
    /// References that no merged package satisfies drop only the item that
    /// needs them. Every skip is returned as an Add-On problem, named by
    /// the package folder (`dir` less `/assets`) or the namespace of the
    /// id that lost something; the result still needs `validate`.
    pub fn merge(mut self, parts: Vec<(String, Pack)>) -> (Pack, Vec<Problem>) {
        let mut notes = Vec::new();
        for (dir, part) in parts {
            fn add<T>(
                into: &mut std::collections::BTreeMap<String, T>,
                from: std::collections::BTreeMap<String, T>,
                kind: Kind,
                dir: &str,
                notes: &mut Vec<Problem>,
            ) {
                for (key, value) in from {
                    match into.entry(key) {
                        std::collections::btree_map::Entry::Occupied(e) => notes.push(Problem::new(
                            package_folder(dir),
                            kind,
                            e.key().clone(),
                            "is already declared by an earlier Add-On, which keeps it; this one's is ignored",
                        )),
                        std::collections::btree_map::Entry::Vacant(e) => {
                            e.insert(value);
                        }
                    }
                }
            }
            add(&mut self.items, part.items, Kind::Item, &dir, &mut notes);
            add(&mut self.images, part.images, Kind::Image, &dir, &mut notes);
            add(
                &mut self.projectiles,
                part.projectiles,
                Kind::Projectile,
                &dir,
                &mut notes,
            );
            // Still keyed by bare Torque name, so packages can collide here.
            add(
                &mut self.damage_types,
                part.damage_types,
                Kind::DamageType,
                &dir,
                &mut notes,
            );
            add(
                &mut self.explosions,
                part.explosions,
                Kind::Explosion,
                &dir,
                &mut notes,
            );
            let sounds = part
                .sounds
                .into_iter()
                .map(|(key, mut sound)| {
                    if !sound.stock {
                        sound.package.get_or_insert_with(|| dir.clone());
                    }
                    (key, sound)
                })
                .collect();
            // Keyed by bare profile name, like damage types.
            add(&mut self.sounds, sounds, Kind::Sound, &dir, &mut notes);
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
        let projectiles: Vec<String> = self.projectiles.keys().cloned().collect();
        self.images.retain(|id, image| {
            let missing = image
                .projectile
                .iter()
                .chain(image.scripts.values().filter_map(|s| s.projectile.as_ref()))
                .find(|p| !projectiles.contains(p));
            if let Some(p) = missing {
                notes.push(
                    Problem::new(
                        add_on_of(id).unwrap_or(id),
                        Kind::Projectile,
                        p.clone(),
                        "no Add-On provides it, so the weapon is left out",
                    )
                    .used_by(format!("image {id}")),
                );
            }
            missing.is_none()
        });
        for p in self.projectiles.values_mut() {
            if let Some(image) = &p.sport_image
                && !self.images.contains_key(image)
            {
                notes.push(
                    Problem::new(
                        add_on_of(&p.id).unwrap_or(&p.id),
                        Kind::Image,
                        image.clone(),
                        "no Add-On provides it, so the ball is not held after a catch",
                    )
                    .used_by(format!("projectile {}", p.id)),
                );
                p.sport_image = None;
            }
        }
        let images = &self.images;
        self.items.retain(|id, item| {
            let keep = item.image.is_empty() || images.contains_key(&item.image);
            if !keep {
                notes.push(
                    Problem::new(
                        add_on_of(id).unwrap_or(id),
                        Kind::Image,
                        item.image.clone(),
                        "no Add-On provides it, so the item is left out",
                    )
                    .used_by(format!("item {id}")),
                );
            }
            keep
        });
        (self, notes)
    }
}

/// Add one package's effects; an id already present keeps the first
/// package's definition.
fn merge_effects(
    into: &mut crate::PackEffects,
    part: crate::PackEffects,
    dir: &str,
    notes: &mut Vec<Problem>,
) {
    fn add<T>(
        into: &mut Vec<T>,
        part: Vec<T>,
        id: impl Fn(&T) -> &str,
        dir: &str,
        notes: &mut Vec<Problem>,
    ) {
        for item in part {
            if into.iter().any(|x| id(x).eq_ignore_ascii_case(id(&item))) {
                notes.push(Problem::new(
                    package_folder(dir),
                    Kind::Effect,
                    id(&item),
                    "is already defined by an earlier Add-On, which keeps it; this one's is ignored",
                ));
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

