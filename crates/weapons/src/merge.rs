//! One weapons pack built from every package that provides weapons: the
//! base game's pack first, then each other package in `packages.json` order.
//! Systems keep taking a single `Pack`; only loading changes.
use crate::{Pack, Resource};
use std::path::{Path, PathBuf};

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
    /// needs them. Every skip is returned as a diagnostic; the result still
    /// needs `validate`.
    pub fn merge(mut self, parts: Vec<(String, Pack)>) -> (Pack, Vec<String>) {
        let mut notes = Vec::new();
        for (dir, part) in parts {
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
            // Still keyed by bare Torque name, so packages can collide here.
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
        self.images.retain(|id, image| match &image.projectile {
            Some(p) if !projectiles.contains(p) => {
                notes.push(format!(
                    "image {id} dropped: projectile {p} is not provided"
                ));
                false
            }
            _ => true,
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
            let keep = images.contains_key(&item.image);
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
