//! The server environment: what a client must agree with to join.
//!
//! The server publishes one [`Environment`] (its lockfile): the platform API
//! level, the base-game content identity, and every enabled package with its
//! version and data-archive hash, in load order. A joining client installs
//! whichever of those archives it lacks, then reports the set it activated;
//! any difference is explained package by package.
use crate::diag::{Diagnostic, Diagnostics};
use crate::id::{self, Requirement, Version};
use crate::manifest::{Manifest, SlotMode};
use crate::{API_LEVEL, archive};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ENVIRONMENT_SCHEMA: u32 = 1;
pub const MAX_PACKAGES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRef {
    pub id: String,
    pub version: String,
    /// SHA-256 of the data archive: the package's identity.
    pub hash: String,
    /// Data archive size in bytes.
    pub size: u64,
}

impl std::fmt::Display for PackageRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} ({})", self.id, self.version, &self.hash[..self.hash.len().min(12)])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub schema_version: u32,
    pub api: u32,
    /// The base game's content identity (vanilla packs).
    pub base_content: String,
    /// Enabled packages in load order (dependencies first).
    pub packages: Vec<PackageRef>,
}

impl Environment {
    pub fn vanilla(base_content: impl Into<String>) -> Self {
        Self {
            schema_version: ENVIRONMENT_SCHEMA,
            api: API_LEVEL,
            base_content: base_content.into(),
            packages: Vec::new(),
        }
    }

    /// Checks for an environment received from the network.
    pub fn validate_bounds(&self, max_archive_bytes: u64) -> Result<()> {
        ensure!(self.schema_version == ENVIRONMENT_SCHEMA, "Unsupported environment schema");
        ensure!(self.packages.len() <= MAX_PACKAGES, "Too many packages");
        ensure!(
            !self.base_content.is_empty() && self.base_content.len() <= 128,
            "Invalid base content identity"
        );
        let mut ids = BTreeSet::new();
        let mut hashes = BTreeSet::new();
        for package in &self.packages {
            ensure!(
                id::namespace_problem(&package.id).is_none() && !id::is_reserved(&package.id),
                "Invalid package id"
            );
            ensure!(Version::parse(&package.version).is_ok(), "Invalid package version");
            ensure!(archive::is_hash(&package.hash), "Invalid package hash");
            ensure!(package.size <= max_archive_bytes, "Package {} is too large", package.id);
            ensure!(ids.insert(&package.id), "Duplicate package {}", package.id);
            ensure!(hashes.insert(&package.hash), "Duplicate package hash");
        }
        Ok(())
    }

    /// Total bytes of every package archive.
    pub fn total_bytes(&self) -> u64 {
        self.packages.iter().map(|p| p.size).sum()
    }

    /// The lockfile digest: equal environments have equal digests.
    pub fn digest(&self) -> String {
        archive::hash_bytes(&serde_json::to_vec(self).expect("environment serializes"))
    }

    /// Explain every difference between this (the server's) package set and
    /// a client's. Empty when they agree.
    pub fn compare(&self, client: &[PackageRef]) -> Vec<Mismatch> {
        let theirs: BTreeMap<&str, &PackageRef> = client.iter().map(|p| (p.id.as_str(), p)).collect();
        let ours: BTreeMap<&str, &PackageRef> = self.packages.iter().map(|p| (p.id.as_str(), p)).collect();
        let mut out = Vec::new();
        for (id, server) in &ours {
            match theirs.get(id) {
                None => out.push(Mismatch::Missing((*server).clone())),
                Some(client) if client.hash != server.hash => out.push(Mismatch::Different {
                    server: (*server).clone(),
                    client: (*client).clone(),
                }),
                Some(_) => {}
            }
        }
        for (id, client) in &theirs {
            if !ours.contains_key(id) {
                out.push(Mismatch::Extra((*client).clone()));
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Mismatch {
    /// The server has it; the client does not.
    Missing(PackageRef),
    /// Both have the package, with different content.
    Different { server: PackageRef, client: PackageRef },
    /// The client activated a package the server does not run.
    Extra(PackageRef),
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(p) => write!(f, "server has {p}, you do not"),
            Self::Different { server, client } => {
                write!(f, "server has {server}, you have {client}")
            }
            Self::Extra(p) => write!(f, "you have {p}, the server does not"),
        }
    }
}

/// One package offered to [`resolve`].
#[derive(Debug, Clone)]
pub struct Candidate {
    pub manifest: Manifest,
    pub data: archive::Built,
}

/// Order the enabled packages so dependencies load first, and refuse sets
/// that cannot run together: duplicate ids, missing or wrong-version
/// dependencies, cycles, a newer API level than this build, or two packages
/// replacing the same exclusive slot.
pub fn resolve(base_content: &str, candidates: &[Candidate]) -> (Option<Environment>, Diagnostics) {
    let mut out = Diagnostics::default();
    let mut by_id: BTreeMap<&str, &Candidate> = BTreeMap::new();
    for candidate in candidates {
        let id = candidate.manifest.id.as_str();
        if let Some(first) = by_id.insert(id, candidate) {
            out.push(
                Diagnostic::error(
                    "environment.duplicate_package",
                    format!(
                        "package `{id}` is enabled twice ({} and {})",
                        first.manifest.version, candidate.manifest.version
                    ),
                )
                .hint("enable only one version of a package"),
            );
        }
        if candidate.manifest.api > API_LEVEL {
            out.push(Diagnostic::error(
                "environment.api",
                format!(
                    "`{id}` needs platform API level {}; this server provides {API_LEVEL}",
                    candidate.manifest.api
                ),
            ));
        }
    }
    for candidate in by_id.values() {
        let manifest = &candidate.manifest;
        for (dependency, text) in &manifest.dependencies {
            let Ok(requirement) = Requirement::parse(text) else { continue };
            match by_id.get(dependency.as_str()) {
                None => out.push(
                    Diagnostic::error(
                        "environment.dependency_missing",
                        format!("`{}` needs `{dependency}`, which is not enabled", manifest.id),
                    )
                    .hint(format!("enable `{dependency}` too, or disable `{}`", manifest.id)),
                ),
                Some(found) => {
                    let version = found.manifest.version();
                    if !version.is_some_and(|v| requirement.matches(v)) {
                        out.push(Diagnostic::error(
                            "environment.dependency_version",
                            format!(
                                "`{}` needs `{dependency}` {text}, but {} is enabled",
                                manifest.id, found.manifest.version
                            ),
                        ));
                    }
                }
            }
        }
    }
    // Exclusive slots.
    let mut replacing: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for candidate in by_id.values() {
        for claim in &candidate.manifest.slots {
            if claim.mode == SlotMode::Replace {
                replacing.entry(&claim.slot).or_default().push(&candidate.manifest.id);
            }
        }
    }
    for (slot, packages) in &replacing {
        if packages.len() > 1 {
            out.push(
                Diagnostic::error(
                    "environment.slot_conflict",
                    format!("slot `{slot}` is replaced by more than one package: {}", packages.join(", ")),
                )
                .hint("an exclusive slot takes one package; disable all but one"),
            );
        }
    }
    // Deterministic topological order: dependencies first, ties by id.
    let mut order: Vec<&str> = Vec::new();
    let mut state: BTreeMap<&str, u8> = BTreeMap::new(); // 1 visiting, 2 done
    fn visit<'a>(
        id: &'a str,
        by_id: &BTreeMap<&'a str, &'a Candidate>,
        state: &mut BTreeMap<&'a str, u8>,
        order: &mut Vec<&'a str>,
        path: &mut Vec<&'a str>,
        out: &mut Diagnostics,
    ) {
        match state.get(id) {
            Some(2) => return,
            Some(1) => {
                path.push(id);
                out.push(Diagnostic::error(
                    "environment.dependency_cycle",
                    format!("dependency cycle: {}", path.join(" -> ")),
                ));
                path.pop();
                return;
            }
            _ => {}
        }
        let Some(candidate) = by_id.get(id) else { return };
        state.insert(id, 1);
        path.push(id);
        for dependency in candidate.manifest.dependencies.keys() {
            if let Some((key, _)) = by_id.get_key_value(dependency.as_str()) {
                visit(key, by_id, state, order, path, out);
            }
        }
        path.pop();
        state.insert(id, 2);
        order.push(id);
    }
    for id in by_id.keys() {
        let mut path = Vec::new();
        visit(id, &by_id, &mut state, &mut order, &mut path, &mut out);
    }
    if out.has_errors() {
        return (None, out);
    }
    let packages = order
        .into_iter()
        .map(|id| {
            let candidate = by_id[id];
            PackageRef {
                id: id.into(),
                version: candidate.manifest.version.clone(),
                hash: candidate.data.hash.clone(),
                size: candidate.data.size,
            }
        })
        .collect();
    (
        Some(Environment {
            schema_version: ENVIRONMENT_SCHEMA,
            api: API_LEVEL,
            base_content: base_content.into(),
            packages,
        }),
        out,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::SlotClaim;

    fn candidate(id: &str, version: &str, deps: &[(&str, &str)]) -> Candidate {
        Candidate {
            manifest: Manifest {
                schema_version: 1,
                id: id.into(),
                version: version.into(),
                api: 1,
                name: id.into(),
                description: String::new(),
                authors: vec![],
                license: "CC0-1.0".into(),
                provenance: Default::default(),
                dependencies: deps.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
                capabilities: vec![],
                provides: vec![],
                slots: vec![],
            },
            data: archive::Built {
                hash: archive::hash_bytes(format!("{id}{version}").as_bytes()),
                size: 10,
                files: 1,
            },
        }
    }

    #[test]
    fn dependencies_load_first_and_order_is_deterministic() {
        let (env, d) = resolve(
            "base",
            &[
                candidate("zombies", "1.0.0", &[("creatures", ">=2")]),
                candidate("creatures", "2.1.0", &[]),
                candidate("alpha", "1.0.0", &[]),
            ],
        );
        assert!(d.is_empty(), "{d:?}");
        let ids: Vec<_> = env.unwrap().packages.into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["alpha", "creatures", "zombies"]);
    }

    #[test]
    fn broken_sets_are_explained() {
        let codes = |set: &[Candidate]| resolve("base", set).1.codes().into_iter().map(String::from).collect::<Vec<_>>();
        assert_eq!(
            codes(&[candidate("zombies", "1.0.0", &[("creatures", ">=2")])]),
            ["environment.dependency_missing"]
        );
        assert_eq!(
            codes(&[
                candidate("zombies", "1.0.0", &[("creatures", ">=2")]),
                candidate("creatures", "1.5.0", &[]),
            ]),
            ["environment.dependency_version"]
        );
        assert_eq!(
            codes(&[candidate("a", "1.0.0", &[("b", "*")]), candidate("b", "1.0.0", &[("a", "*")])]),
            ["environment.dependency_cycle"]
        );
        assert_eq!(
            codes(&[candidate("a", "1.0.0", &[]), candidate("a", "2.0.0", &[])]),
            ["environment.duplicate_package"]
        );
        let mut one = candidate("ctf", "1.0.0", &[]);
        let mut two = candidate("tdm", "1.0.0", &[]);
        for c in [&mut one, &mut two] {
            c.manifest.slots.push(SlotClaim {
                slot: "game.mode".into(),
                mode: SlotMode::Replace,
                id: format!("{}:behaviour/mode", c.manifest.id),
            });
        }
        assert_eq!(codes(&[one, two]), ["environment.slot_conflict"]);
    }

    #[test]
    fn mismatches_name_each_package() {
        let (env, _) = resolve("base", &[candidate("a", "1.0.0", &[]), candidate("b", "1.0.0", &[])]);
        let env = env.unwrap();
        env.validate_bounds(u64::MAX).unwrap();
        let mut client = env.packages.clone();
        assert!(env.compare(&client).is_empty());
        client[0].hash = archive::hash_bytes(b"other");
        client[0].version = "0.9.0".into();
        client.remove(1);
        client.push(candidate("c", "1.0.0", &[]).manifest_ref());
        let text: Vec<String> = env.compare(&client).iter().map(ToString::to_string).collect();
        assert_eq!(text.len(), 3);
        assert!(text[0].starts_with("server has a 1.0.0"), "{text:?}");
        assert!(text[0].contains("you have a 0.9.0"));
        assert!(text[1].starts_with("server has b 1.0.0"));
        assert!(text[2].starts_with("you have c 1.0.0"));
    }

    impl Candidate {
        fn manifest_ref(&self) -> PackageRef {
            PackageRef {
                id: self.manifest.id.clone(),
                version: self.manifest.version.clone(),
                hash: self.data.hash.clone(),
                size: self.data.size,
            }
        }
    }
}
