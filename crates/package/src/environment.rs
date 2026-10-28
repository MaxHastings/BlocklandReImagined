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
    /// Files whose length and modification time match the last hash taken
    /// of them are not read again ([`remember_file_hashes_in`]).
    pub fn load(root: &Path, set: &PackageSet) -> Result<Self> {
        Ok(Self::load_counting(root, set)?.0)
    }

    /// [`Self::load`], also returning how many bytes it had to read.
    pub fn load_counting(root: &Path, set: &PackageSet) -> Result<(Self, u64)> {
        let mut cache = HASH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let (file, hashes) = cache.kept();
        hashes.read = 0;
        let mut packages = Vec::with_capacity(set.packages.len());
        for entry in &set.packages {
            let dir = package_dir(root, entry)?;
            let (hash, size) = hash_dir_with(&dir, Some(&mut *hashes))
                .with_context(|| format!("Hashing package `{}`", entry.id))?;
            packages.push(PackageRef {
                id: entry.id.clone(),
                version: entry.version.clone(),
                side: entry.side,
                hash,
                size,
            });
        }
        let read = hashes.read;
        if let Some(file) = &file {
            // A cache that cannot be written only costs the next run a read.
            let _ = hashes.save(file);
        }
        Ok((
            Self {
                schema_version: ENVIRONMENT_SCHEMA,
                api: API_LEVEL,
                packages,
            },
            read,
        ))
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

/// SHA-256 identity of immutable native resource bytes.
pub fn hash_bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn is_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Files the operating system drops into folders a person browses: thumbnail
/// caches, folder settings and macOS metadata. They are never content, so a
/// package's identity ignores them rather than refusing a join over them.
pub fn is_os_litter(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "thumbs.db" | "ehthumbs.db" | "ehthumbs_vista.db" | "desktop.ini" | ".ds_store"
    ) || name.starts_with("._")
}

/// Deterministic identity of a package directory: SHA-256 over each file's
/// relative path (forward slashes), length and SHA-256, in sorted path order.
/// Symlinks and junctions are refused rather than followed. Reads every
/// file: what checks a download uses.
pub fn hash_dir(root: &Path) -> Result<(String, u64)> {
    hash_dir_with(root, None)
}

fn hash_dir_with(root: &Path, mut known: Option<&mut FileHashes>) -> Result<(String, u64)> {
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
            } else if entry.file_name().to_str().is_some_and(is_os_litter) {
                continue;
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
        let (length, digest) = match known.as_deref_mut() {
            Some(known) => known.hash(&path, &mut buffer)?,
            None => hash_file(&path, &mut buffer)?,
        };
        total.update(relative.as_bytes());
        total.update([0]);
        total.update(length.to_le_bytes());
        total.update(digest);
        size += length;
    }
    Ok((hex(&total.finalize()), size))
}

fn hash_file(path: &Path, buffer: &mut [u8]) -> Result<(u64, [u8; 32])> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut length = 0u64;
    loop {
        let read = file.read(buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        length += read as u64;
    }
    Ok((length, hasher.finalize().into()))
}

/// Schema of the file-hash cache [`remember_file_hashes_in`] keeps. A cache
/// of another schema is ignored and rewritten.
const FILE_HASHES_SCHEMA: u32 = 1;
/// A file changed this recently may change again within its timestamp's
/// resolution without its length or time moving, so its hash is not kept.
const SETTLED: std::time::Duration = std::time::Duration::from_secs(5);

/// Each package file's SHA-256 with the length and modification time it
/// had when read. A file whose length and time are unchanged is not read
/// again: hashing all content on every Host and Join read about 560 MB,
/// which took 24-27 s from a cold disk.
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileHashes {
    schema_version: u32,
    /// Absolute path -> (length, modified in ns since the epoch, SHA-256 hex).
    files: BTreeMap<String, (u64, u128, String)>,
    #[serde(skip)]
    seen: BTreeSet<String>,
    #[serde(skip)]
    changed: bool,
    #[serde(skip)]
    read: u64,
}

impl FileHashes {
    fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|c| c.schema_version == FILE_HASHES_SCHEMA)
            .unwrap_or(Self {
                schema_version: FILE_HASHES_SCHEMA,
                ..Self::default()
            })
    }

    fn hash(&mut self, path: &Path, buffer: &mut [u8]) -> Result<(u64, [u8; 32])> {
        let key = path.to_string_lossy().into_owned();
        let meta = fs::metadata(path)?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
        self.seen.insert(key.clone());
        if let (Some(modified), Some((length, at, digest))) = (modified, self.files.get(&key))
            && *length == meta.len()
            && *at == modified.as_nanos()
            && let Some(digest) = unhex(digest)
        {
            return Ok((*length, digest));
        }
        let (length, digest) = hash_file(path, buffer)?;
        self.read += length;
        let settled = modified.filter(|m| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .is_ok_and(|now| now.saturating_sub(*m) >= SETTLED)
        });
        // Kept only when the file did not change while it was read.
        let after = fs::metadata(path)?;
        match settled {
            Some(m) if after.len() == length && after.modified().ok() == meta.modified().ok() => {
                self.files.insert(key, (length, m.as_nanos(), hex(&digest)));
            }
            _ => {
                self.files.remove(&key);
            }
        }
        self.changed = true;
        Ok((length, digest))
    }

    fn save(&mut self, path: &Path) -> Result<()> {
        let before = self.files.len();
        self.files.retain(|k, _| self.seen.contains(k));
        if !self.changed && self.files.len() == before {
            return Ok(());
        }
        let bytes = serde_json::to_vec(self)?;
        let temp = path.with_extension("tmp");
        fs::write(&temp, bytes)?;
        fs::rename(&temp, path)?;
        self.changed = false;
        Ok(())
    }
}

fn unhex(text: &str) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    if text.len() != 64 {
        return None;
    }
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

struct HashCache {
    file: Option<std::path::PathBuf>,
    hashes: Option<FileHashes>,
}

impl HashCache {
    /// The kept hashes, read from their file on first use, and that file.
    fn kept(&mut self) -> (Option<std::path::PathBuf>, &mut FileHashes) {
        let file = self.file.clone();
        let hashes = self.hashes.get_or_insert_with(|| match &file {
            Some(file) => FileHashes::load(file),
            None => FileHashes {
                schema_version: FILE_HASHES_SCHEMA,
                ..FileHashes::default()
            },
        });
        (file, hashes)
    }
}

/// Each file's length and SHA-256, through the hashes [`Environment::load`]
/// keeps: a file unchanged since it was last hashed is not read. What a
/// host lists for joiners' downloads; a joiner checking what it downloaded
/// reads every byte ([`hash_dir`]).
pub fn kept_file_hashes(paths: &[std::path::PathBuf]) -> Result<Vec<(u64, [u8; 32])>> {
    let mut cache = HASH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let (file, hashes) = cache.kept();
    let mut buffer = vec![0u8; 1 << 16];
    let found = paths
        .iter()
        .map(|path| hashes.hash(path, &mut buffer))
        .collect::<Result<Vec<_>>>();
    if let Some(file) = &file {
        // A cache that cannot be written only costs the next run a read.
        let _ = hashes.save(file);
    }
    found
}

static HASH_CACHE: std::sync::Mutex<HashCache> = std::sync::Mutex::new(HashCache {
    file: None,
    hashes: None,
});

/// Keep package file hashes in `file` across runs, so [`Environment::load`]
/// reads only files that changed since the last time. Without it the hashes
/// are still kept for the rest of this process.
pub fn remember_file_hashes_in(file: std::path::PathBuf) {
    let mut cache = HASH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.file = Some(file);
    cache.hashes = None;
}

/// A separate cache for each installed content root. Saving one installation
/// prunes files it did not see; sharing its cache with another installation
/// would therefore evict the other's verified hashes on every switch.
/// Canonical roots make relative/absolute spellings of one install agree.
pub fn content_hash_cache_file(root: &Path, state: &Path) -> Result<std::path::PathBuf> {
    let canonical = root.canonicalize()?;
    let key = hash_bytes(canonical.as_os_str().as_encoded_bytes());
    Ok(state.join(format!("package-hashes-{key}.json")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_bytes_have_standard_sha256_identity() {
        assert_eq!(
            hash_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hash_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    fn aged(path: &Path, bytes: &[u8], seconds_ago: u64) {
        fs::write(path, bytes).unwrap();
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds_ago);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    #[test]
    fn switching_installations_keeps_each_verified_cache_and_rereads_edits() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let state = std::env::temp_dir().join(format!("bri-install-hashes-{unique}"));
        let a = state.join("release");
        let b = state.join("preview");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        aged(&a.join("asset.bin"), b"alpha", 60);
        aged(&b.join("asset.bin"), b"bravo", 60);
        let a_cache = content_hash_cache_file(&a, &state).unwrap();
        let b_cache = content_hash_cache_file(&b, &state).unwrap();
        assert_ne!(a_cache, b_cache);
        assert_eq!(
            a_cache,
            content_hash_cache_file(&a.join("."), &state).unwrap()
        );

        // Simulate separate processes, including saving/pruning the other root.
        for (root, cache) in [(&a, &a_cache), (&b, &b_cache)] {
            let mut hashes = FileHashes::load(cache);
            assert_eq!(
                hash_dir_with(root, Some(&mut hashes)).unwrap(),
                hash_dir(root).unwrap()
            );
            assert_eq!(hashes.read, 5);
            hashes.save(cache).unwrap();
        }
        let mut hashes = FileHashes::load(&a_cache);
        let original = hash_dir_with(&a, Some(&mut hashes)).unwrap();
        assert_eq!(original, hash_dir(&a).unwrap());
        assert_eq!(hashes.read, 0, "the preview must not evict the release");
        aged(&a.join("asset.bin"), b"other", 30);
        let edited = hash_dir_with(&a, Some(&mut hashes)).unwrap();
        assert_ne!(edited, original);
        assert_eq!(edited, hash_dir(&a).unwrap());
        assert_eq!(hashes.read, 5, "an edit must still invalidate the hash");
        fs::remove_dir_all(state).unwrap();
    }

    #[test]
    fn kept_file_hashes_skip_unchanged_files_and_reread_changed_ones() {
        let root = std::env::temp_dir().join(format!("bri-hashes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("pack/sub")).unwrap();
        aged(&root.join("pack/a.json"), b"alpha", 60);
        aged(&root.join("pack/sub/b.bin"), &[7u8; 5000], 60);
        let pack = root.join("pack");
        let cache_file = root.join("hashes.json");
        let mut hashes = FileHashes::load(&cache_file);
        let first = hash_dir_with(&pack, Some(&mut hashes)).unwrap();
        assert_eq!(
            first,
            hash_dir(&pack).unwrap(),
            "same identity as a full read"
        );
        assert_eq!(hashes.read, 5005);
        hashes.save(&cache_file).unwrap();

        // A later run reads nothing for the same files.
        let mut hashes = FileHashes::load(&cache_file);
        assert_eq!(hash_dir_with(&pack, Some(&mut hashes)).unwrap(), first);
        assert_eq!(hashes.read, 0);

        // A changed file is read again, even at the same length.
        aged(&root.join("pack/a.json"), b"bravo", 30);
        let second = hash_dir_with(&pack, Some(&mut hashes)).unwrap();
        assert_eq!(second, hash_dir(&pack).unwrap());
        assert_ne!(second, first);
        assert_eq!(hashes.read, 5);

        // A file written just now is hashed but not kept: it may change
        // again inside its timestamp's resolution.
        fs::write(root.join("pack/a.json"), b"charl").unwrap();
        hashes.read = 0;
        assert_eq!(
            hash_dir_with(&pack, Some(&mut hashes)).unwrap(),
            hash_dir(&pack).unwrap()
        );
        assert_eq!(hashes.read, 5);
        hashes.read = 0;
        hash_dir_with(&pack, Some(&mut hashes)).unwrap();
        assert_eq!(hashes.read, 5);

        // A deleted file leaves the cache when it is next saved.
        fs::remove_file(root.join("pack/sub/b.bin")).unwrap();
        hashes.seen.clear();
        hash_dir_with(&pack, Some(&mut hashes)).unwrap();
        hashes.save(&cache_file).unwrap();
        assert!(
            !FileHashes::load(&cache_file)
                .files
                .keys()
                .any(|k| k.ends_with("b.bin"))
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// What a host lists for downloads comes from the kept hashes: a file
    /// whose length and time are unchanged is not read again.
    #[test]
    fn kept_file_hashes_answer_unchanged_files_without_reading_them() {
        let root = std::env::temp_dir().join(format!("bri-kept-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("texture.png");
        aged(&path, b"first", 60);
        let first = kept_file_hashes(std::slice::from_ref(&path)).unwrap();
        assert_eq!(first[0].0, 5);
        // Same length and time, other bytes: only a read would notice.
        let at = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, b"other").unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(at)
            .unwrap();
        assert_eq!(
            kept_file_hashes(std::slice::from_ref(&path)).unwrap(),
            first
        );
        let _ = fs::remove_dir_all(&root);
    }

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
        // Browsing the folder must not change its identity.
        fs::write(dir.join("Thumbs.db"), b"cache").unwrap();
        fs::write(dir.join("sub/desktop.ini"), b"[.ShellClassInfo]").unwrap();
        fs::write(dir.join("sub/.DS_Store"), b"mac").unwrap();
        assert_eq!(hash_dir(&dir).unwrap(), (first.clone(), size));
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
