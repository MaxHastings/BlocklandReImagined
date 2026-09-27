//! `packages.json`: the packages a client or server loads, from where, and on
//! which side. The client and the dedicated server both read it from the
//! content root; the base game's own list ships as the default.
use crate::diag::{Diagnostic, Diagnostics};
use crate::id::{self, Version};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub const PACKAGES_FILE: &str = "packages.json";
pub const PACKAGES_SCHEMA: u32 = 1;
pub const MAX_PACKAGES: usize = 256;
pub const MAX_PACKAGES_BYTES: u64 = 1024 * 1024;

/// Where a package runs, which decides what a joining client must agree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Only the server loads it (reference worlds, lesson scripts). Never
    /// compared at join and never sent.
    Server,
    /// Both load it and simulate with it. Its hash must match to join.
    Shared,
    /// Presentation only (UI, sound, effects). A difference is reported to
    /// the joining player but does not refuse the join.
    Client,
}

impl Side {
    pub fn on_client(self) -> bool {
        self != Self::Server
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSet {
    pub schema_version: u32,
    /// In load order.
    pub packages: Vec<PackageEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageEntry {
    /// Package id; see [`id::content_namespace`] for the namespace its
    /// content uses.
    pub id: String,
    /// `major.minor.patch`.
    pub version: String,
    pub side: Side,
    /// Directory under the content root.
    pub dir: String,
    /// The engine system that reads this package, for packages the engine
    /// consumes directly (`weapons`, `map_bundle`, ...). At most one package
    /// fills each role. Packages without a role are loaded, hashed and agreed
    /// on, and are read by whichever system consumes their content kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// The base game's package list, used when the content root has no
/// `packages.json` of its own.
pub const BASE_PACKAGES: &str = include_str!("../base-packages.json");

impl PackageSet {
    pub fn base() -> Self {
        Self::parse(BASE_PACKAGES.as_bytes()).expect("the base package list is valid")
    }

    /// `root/packages.json` when present, otherwise [`Self::base`].
    pub fn load_root(root: &Path) -> Result<Self> {
        let path = root.join(PACKAGES_FILE);
        if path.exists() {
            Self::load(&path)
        } else {
            Ok(Self::base())
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let set: Self = serde_json::from_slice(bytes).context("Decoding packages.json")?;
        set.validate().into_result()?;
        Ok(set)
    }

    /// Read `packages.json` from a content root.
    pub fn load(path: &Path) -> Result<Self> {
        let meta = std::fs::symlink_metadata(path)
            .with_context(|| format!("Missing package list {}", path.display()))?;
        ensure!(
            meta.is_file() && meta.len() <= MAX_PACKAGES_BYTES,
            "Package list must be a regular file of at most {MAX_PACKAGES_BYTES} bytes: {}",
            path.display()
        );
        Self::parse(&std::fs::read(path)?).with_context(|| format!("Reading {}", path.display()))
    }

    pub fn validate(&self) -> Diagnostics {
        let mut out = Diagnostics::default();
        let here = PACKAGES_FILE;
        if self.schema_version != PACKAGES_SCHEMA {
            out.push(
                Diagnostic::error(
                    "packages.schema_version",
                    format!(
                        "schema_version {} is not supported; this game reads {PACKAGES_SCHEMA}",
                        self.schema_version
                    ),
                )
                .at(format!("{here}#/schema_version")),
            );
        }
        if self.packages.len() > MAX_PACKAGES {
            out.push(Diagnostic::error(
                "packages.too_many",
                format!(
                    "{} packages; the limit is {MAX_PACKAGES}",
                    self.packages.len()
                ),
            ));
        }
        let mut ids = BTreeSet::new();
        let mut dirs = BTreeSet::new();
        let mut roles = BTreeSet::new();
        for (index, package) in self.packages.iter().enumerate() {
            let at = |field: &str| format!("{here}#/packages/{index}/{field}");
            if let Some(problem) = id::namespace_problem(&package.id) {
                out.push(
                    Diagnostic::error(
                        "packages.id",
                        format!("package id `{}` {problem}", package.id),
                    )
                    .at(at("id")),
                );
            }
            if !ids.insert(package.id.as_str()) {
                out.push(
                    Diagnostic::error(
                        "packages.duplicate",
                        format!("package `{}` is listed twice", package.id),
                    )
                    .at(at("id")),
                );
            }
            if let Err(problem) = Version::parse(&package.version) {
                out.push(Diagnostic::error("packages.version", problem).at(at("version")));
            }
            if let Err(problem) = relative_dir(&package.dir) {
                out.push(Diagnostic::error("packages.dir", problem).at(at("dir")));
            } else if !dirs.insert(package.dir.to_ascii_lowercase()) {
                out.push(
                    Diagnostic::error(
                        "packages.duplicate_dir",
                        format!("directory `{}` is used by two packages", package.dir),
                    )
                    .at(at("dir")),
                );
            }
            if let Some(role) = &package.role
                && !roles.insert(role.as_str())
            {
                out.push(
                    Diagnostic::error(
                        "packages.role_conflict",
                        format!("role `{role}` is filled by more than one package"),
                    )
                    .at(at("role"))
                    .hint("the engine reads exactly one package per role; list only one"),
                );
            }
        }
        out
    }

    /// The package filling `role`.
    pub fn role(&self, role: &str) -> Result<&PackageEntry> {
        self.packages
            .iter()
            .find(|p| p.role.as_deref() == Some(role))
            .with_context(|| format!("No package provides the `{role}` role in packages.json"))
    }

    /// The directory of the package filling `role`, contained in `root`.
    pub fn role_dir(&self, root: &Path, role: &str) -> Result<PathBuf> {
        package_dir(root, self.role(role)?)
    }
}

/// A package's directory, resolved and confirmed to stay inside `root`.
pub fn package_dir(root: &Path, package: &PackageEntry) -> Result<PathBuf> {
    relative_dir(&package.dir).map_err(anyhow::Error::msg)?;
    let root = root
        .canonicalize()
        .with_context(|| format!("Missing content root {}", root.display()))?;
    let path = root.join(&package.dir).canonicalize().with_context(|| {
        format!(
            "Package `{}` is missing its directory {}",
            package.id, package.dir
        )
    })?;
    ensure!(
        path.starts_with(&root) && path.is_dir(),
        "Package `{}` directory must be a directory inside the content root",
        package.id
    );
    Ok(path)
}

fn relative_dir(dir: &str) -> std::result::Result<(), String> {
    let valid = !dir.is_empty()
        && dir.len() <= 160
        && !dir.contains(['\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        && dir
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && !s.ends_with([' ', '.']));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "`{dir}` must be a plain relative directory under the content root"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, dir: &str, role: Option<&str>) -> PackageEntry {
        PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side: Side::Shared,
            dir: dir.into(),
            role: role.map(Into::into),
        }
    }

    #[test]
    fn problems_are_named() {
        let set = PackageSet {
            schema_version: 1,
            packages: vec![
                entry("v20-weapons", "weapons-pack-009", Some("weapons")),
                entry("v20-weapons", "../escape", Some("weapons")),
                entry("Bad", "x", None),
            ],
        };
        let d = set.validate();
        assert_eq!(
            d.codes(),
            [
                "packages.duplicate",
                "packages.dir",
                "packages.role_conflict",
                "packages.id"
            ]
        );
    }

    #[test]
    fn base_list_is_valid_and_namespaced() {
        let base = PackageSet::base();
        assert_eq!(base.packages.len(), 18);
        for package in &base.packages {
            assert!(id::is_reserved(&package.id), "{}", package.id);
            assert_eq!(id::content_namespace(&package.id), id::BASE_NAMESPACE);
            assert!(package.role.is_some());
        }
    }

    #[test]
    fn roles_resolve() {
        let set = PackageSet {
            schema_version: 1,
            packages: vec![entry("v20-weapons", "weapons-pack-009", Some("weapons"))],
        };
        assert!(set.validate().is_empty());
        assert_eq!(set.role("weapons").unwrap().dir, "weapons-pack-009");
        assert!(set.role("audio").is_err());
    }
}
