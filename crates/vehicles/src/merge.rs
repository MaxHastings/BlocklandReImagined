//! One vehicles pack built from every package that provides vehicles: the
//! base game's pack first, then each other package in `packages.json` order.
use crate::schema::{Asset, Pack};
use bri_package::health::{Kind, Problem, package_folder};
use std::path::{Path, PathBuf};

/// The directory an asset's `path` is relative to: its package's, when the
/// pack was merged (content-root-relative), else the base pack's own root.
/// Base packs sit under the content root's `default-pack`, so a package
/// directory resolves against the content root, not beside the base root.
pub fn asset_root(content_root: &Path, base_root: &Path, asset: &Asset) -> PathBuf {
    match &asset.package {
        Some(dir) => content_root.join(dir),
        None => base_root.to_path_buf(),
    }
}

impl Pack {
    /// Adds `parts`, each with the content-root-relative directory of its
    /// package, to this (the base) pack. A duplicate vehicle id keeps the
    /// first. Asset paths stay relative to their own package, recorded in
    /// `Asset::package`. Verify each part's assets before merging it. A
    /// duplicate is returned as a problem of the later package's folder.
    pub fn merge(mut self, parts: Vec<(String, Pack)>) -> (Pack, Vec<Problem>) {
        let mut notes = Vec::new();
        for (dir, part) in parts {
            for d in part.definitions {
                if self.definitions.iter().any(|e| e.id == d.id) {
                    notes.push(Problem::new(
                        package_folder(&dir),
                        Kind::Vehicle,
                        d.id.clone(),
                        "is already declared by an earlier Add-On, which keeps it; this one's is ignored",
                    ));
                } else {
                    self.definitions.push(d);
                }
            }
            self.assets.extend(part.assets.into_iter().map(|mut a| {
                a.package.get_or_insert_with(|| dir.clone());
                a
            }));
            self.evidence.extend(part.evidence);
            self.unresolved
                .extend(part.unresolved.into_iter().map(|u| format!("{dir}: {u}")));
            for (k, v) in part.animation_aliases {
                self.animation_aliases.entry(k).or_insert(v);
            }
        }
        (self, notes)
    }
}
