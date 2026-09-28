//! The environment: every package a peer actually loaded, with its content
//! hash. The server publishes its environment; a joining client sends its
//! own, and any difference is explained package by package.
use crate::API_LEVEL;
use crate::id::{self, Version};
use crate::packages::{PackageSet, Side, package_dir};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};

pub const ENVIRONMENT_SCHEMA: u32 = 1;
pub const MAX_PACKAGES: usize = crate::packages::MAX_PACKAGES;
/// Files per package directory, so hashing a hostile tree stays bounded.
pub const MAX_PACKAGE_FILES: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRef {
    pub id: String,
    pub version: String,
    pub side: Side,
    /// SHA-256 over the package's files: its identity.
    pub hash: String,
    /// Total bytes of the package's files.
    pub size: u64,
}

impl std::fmt::Display for PackageRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} ({})",
            self.id,
            self.version,
            &self.hash[..self.hash.len().min(12)]
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub schema_version: u32,
    /// The platform API level this build provides.
    pub api: u32,
    /// Loaded packages in load order.
    pub packages: Vec<PackageRef>,
}

impl Environment {
    /// No packages at all, for probes and tests that load no content.
    pub fn empty() -> Self {
        Self {
            schema_version: ENVIRONMENT_SCHEMA,
            api: API_LEVEL,
            packages: Vec::new(),
        }
    }

    /// Hash every package in `set` under `root`.
    pub fn load(root: &Path, set: &PackageSet) -> Result<Self> {
        let mut packages = Vec::with_capacity(set.packages.len());
        for entry in &set.packages {
            let dir = package_dir(root, entry)?;
            let (hash, size) =
                hash_dir(&dir).with_context(|| format!("Hashing package `{}`", entry.id))?;
            packages.push(PackageRef {
                id: entry.id.clone(),
                version: entry.version.clone(),
                side: entry.side,
                hash,
                size,
            });
        }
        Ok(Self {
            schema_version: ENVIRONMENT_SCHEMA,
            api: API_LEVEL,
            packages,
        })
    }

    /// The packages a client loads (shared and client side), which is what a
    /// client sends when joining.
    pub fn client_packages(&self) -> Vec<PackageRef> {
        self.packages
            .iter()
            .filter(|p| p.side.on_client())
            .cloned()
            .collect()
    }

    /// Checks for a package list received from the network.
    pub fn validate_refs(packages: &[PackageRef]) -> Result<()> {
        ensure!(packages.len() <= MAX_PACKAGES, "Too many packages");
        let mut ids = BTreeSet::new();
        for package in packages {
            ensure!(
                id::namespace_problem(&package.id).is_none(),
                "Invalid package id"
            );
            ensure!(
                Version::parse(&package.version).is_ok(),
                "Invalid package version"
            );
            ensure!(is_hash(&package.hash), "Invalid package hash");
            ensure!(ids.insert(&package.id), "Duplicate package {}", package.id);
        }
        Ok(())
    }

    /// One digest over the whole environment: equal environments have equal
    /// digests. For display and caching, never for explaining a mismatch.
    pub fn digest(&self) -> String {
        hex(&Sha256::digest(
            serde_json::to_vec(self).expect("environment serializes"),
        ))
    }

    /// Explain every difference between this (the server's) environment and
    /// the packages a client loaded. Server-only packages are never compared.
    pub fn compare(&self, client: &[PackageRef]) -> Vec<Mismatch> {
        let theirs: BTreeMap<&str, &PackageRef> =
            client.iter().map(|p| (p.id.as_str(), p)).collect();
        let ours: BTreeMap<&str, &PackageRef> = self
            .packages
            .iter()
            .filter(|p| p.side.on_client())
            .map(|p| (p.id.as_str(), p))
            .collect();
        let mut out = Vec::new();
        for (id, server) in &ours {
            match theirs.get(id) {
                None => out.push(Mismatch::Missing((*server).clone())),
                Some(client) if client.hash != server.hash || client.side != server.side => out
                    .push(Mismatch::Different {
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
    Different {
        server: PackageRef,
        client: PackageRef,
    },
    /// The client loaded a package the server does not run.
    Extra(PackageRef),
}

impl Mismatch {
    /// Whether this difference refuses the join. Only simulation content
    /// (`shared`) must agree; presentation (`client`) differences are
    /// reported and tolerated.
    pub fn blocks_join(&self) -> bool {
        match self {
            Self::Missing(p) | Self::Extra(p) => p.side == Side::Shared,
            Self::Different { server, client } => {
                server.side == Side::Shared || client.side == Side::Shared
            }
        }
    }
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

/// How a join refused for differing shared packages begins.
pub const REFUSAL: &str = "Your content does not match the server: ";

/// The refusal text for `blocking` mismatches: [`REFUSAL`] and
/// [`describe`]. [`parse_refusal`] reads it back.
pub fn refusal(blocking: &[Mismatch]) -> String {
    format!("{REFUSAL}{}", describe(blocking))
}

/// One package named in a refusal: its id, the server's version and the
/// joining player's version (`None` where that side lacks it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedPackage {
    pub id: String,
    pub server: Option<String>,
    pub client: Option<String>,
}

/// Read back a refusal written by [`refusal`], anywhere in `text` (error
/// chains put context in front of it). `None` when `text` is not one.
pub fn parse_refusal(text: &str) -> Option<Vec<RefusedPackage>> {
    let rest = &text[text.find(REFUSAL)? + REFUSAL.len()..];
    // `id version (hash)`, as `PackageRef` displays.
    fn package(text: &str) -> Option<(String, String)> {
        let mut words = text.trim().split(' ');
        let (id, version, hash) = (words.next()?, words.next()?, words.next()?);
        (hash.starts_with('(') && hash.ends_with(')') && words.next().is_none())
            .then(|| (id.to_string(), version.to_string()))
    }
    let mut out = Vec::new();
    for line in rest.trim_end().split("; ") {
        let row = if let Some(p) = line.strip_prefix("server has ") {
            if let Some(server) = p.strip_suffix(", you do not") {
                let (id, v) = package(server)?;
                RefusedPackage {
                    id,
                    server: Some(v),
                    client: None,
                }
            } else {
                let (server, client) = p.split_once(", you have ")?;
                let (id, sv) = package(server)?;
                let (_, cv) = package(client)?;
                RefusedPackage {
                    id,
                    server: Some(sv),
                    client: Some(cv),
                }
            }
        } else {
            let client = line
                .strip_prefix("you have ")?
                .strip_suffix(", the server does not")?;
            let (id, v) = package(client)?;
            RefusedPackage {
                id,
                server: None,
                client: Some(v),
            }
        };
        out.push(row);
    }
    (!out.is_empty()).then_some(out)
}

/// One line per mismatch, for a join rejection or a warning.
pub fn describe(mismatches: &[Mismatch]) -> String {
    mismatches
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn is_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Deterministic identity of a package directory: SHA-256 over each file's
/// relative path (forward slashes), length and SHA-256, in sorted path order.
/// Symlinks and junctions are refused rather than followed.
pub fn hash_dir(root: &Path) -> Result<(String, u64)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            ensure!(
                !kind.is_symlink(),
                "Packages may not contain links: {}",
                path.display()
            );
            if kind.is_dir() {
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("Package file names must be UTF-8")?
                    .replace('\\', "/");
                files.push((relative, path));
                ensure!(
                    files.len() <= MAX_PACKAGE_FILES,
                    "Package has more than {MAX_PACKAGE_FILES} files"
                );
            }
        }
    }
    files.sort();
    let mut total = Sha256::new();
    let mut size = 0u64;
    let mut buffer = vec![0u8; 1 << 16];
    for (relative, path) in files {
        let mut file = fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut length = 0u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            length += read as u64;
        }
        total.update(relative.as_bytes());
        total.update([0]);
        total.update(length.to_le_bytes());
        total.update(hasher.finalize());
        size += length;
    }
    Ok((hex(&total.finalize()), size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(id: &str, side: Side, hash: u8) -> PackageRef {
        PackageRef {
            id: id.into(),
            version: "1.0.0".into(),
            side,
            hash: format!("{hash:02x}").repeat(32),
            size: 1,
        }
    }

    #[test]
    fn mismatches_name_each_package_and_side_decides_refusal() {
        let server = Environment {
            schema_version: 1,
            api: API_LEVEL,
            packages: vec![
                package("v20-weapons", Side::Shared, 1),
                package("v20-ui", Side::Client, 2),
                package("v20-worlds", Side::Server, 3),
                package("v20-audio", Side::Client, 4),
            ],
        };
        let mut client = server.client_packages();
        assert_eq!(client.len(), 3);
        assert!(server.compare(&client).is_empty());
        Environment::validate_refs(&client).unwrap();

        client[0].hash = "aa".repeat(32);
        client[1].hash = "bb".repeat(32);
        client.remove(2);
        client.push(package("creeper", Side::Shared, 9));
        let found = server.compare(&client);
        let text: Vec<String> = found.iter().map(ToString::to_string).collect();
        assert_eq!(text.len(), 4, "{text:?}");
        assert!(
            text[0].starts_with("server has v20-audio 1.0.0"),
            "{text:?}"
        );
        assert!(text[0].ends_with("you do not"));
        assert!(text[1].contains("server has v20-ui") && text[1].contains("you have v20-ui"));
        assert!(text[2].contains("server has v20-weapons"));
        assert!(text[3].starts_with("you have creeper"));
        let blocking: Vec<bool> = found.iter().map(Mismatch::blocks_join).collect();
        assert_eq!(blocking, [false, false, true, true]);
    }

    #[test]
    fn directory_hash_is_order_and_content_sensitive() {
        let dir = std::env::temp_dir().join(format!("bri-package-hash-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a.json"), b"{}").unwrap();
        fs::write(dir.join("sub/b.bin"), b"123").unwrap();
        let (first, size) = hash_dir(&dir).unwrap();
        assert_eq!(size, 5);
        assert_eq!(hash_dir(&dir).unwrap().0, first);
        fs::write(dir.join("sub/b.bin"), b"124").unwrap();
        assert_ne!(hash_dir(&dir).unwrap().0, first);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refusals_read_back_package_by_package() {
        let r = |id: &str, version: &str, hash: char| PackageRef {
            id: id.into(),
            version: version.into(),
            side: Side::Shared,
            hash: hash.to_string().repeat(64),
            size: 1,
        };
        let text = format!(
            "Joining 127.0.0.1: {}",
            refusal(&[
                Mismatch::Missing(r("creeper", "1.0.0", 'a')),
                Mismatch::Different {
                    server: r("v20-weapons", "9.0.0", 'b'),
                    client: r("v20-weapons", "8.0.0", 'c')
                },
                Mismatch::Extra(r("zombies", "2.0.0", 'd')),
            ])
        );
        let rows = parse_refusal(&text).unwrap();
        let v = |s: &str| Some(s.to_string());
        assert_eq!(
            rows,
            [
                RefusedPackage {
                    id: "creeper".into(),
                    server: v("1.0.0"),
                    client: None
                },
                RefusedPackage {
                    id: "v20-weapons".into(),
                    server: v("9.0.0"),
                    client: v("8.0.0")
                },
                RefusedPackage {
                    id: "zombies".into(),
                    server: None,
                    client: v("2.0.0")
                },
            ]
        );
        assert!(parse_refusal("Connection timed out").is_none());
    }
}
