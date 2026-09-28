//! Package distribution. A server lists a package's files; a client fetches
//! the files it lacks by content hash into a cache and installs the package
//! only after every byte and the package hash check out. Data only: a server
//! never sends native code, and nothing downloaded is executed.
//!
//! The cache is content addressed at two levels: files are stored once by
//! SHA-256 (`objects/`), so an asset two packages share downloads once, and
//! installed packages are directories named by package hash (`packages/`).
//!
//! Nothing in the cache is trusted merely because it exists. An installed
//! package carries a seal (`packages/<hash>.seal`) recording each file's
//! size and modification time; when the directory no longer matches its seal
//! it is re-hashed, and removed if it no longer is the package it claims to
//! be. Objects are checked by size before reuse and by hash as they are
//! installed, and a damaged one is deleted so the next fetch replaces it.
//! Every writer stages under its own unique name, so concurrent fetches into
//! one cache never share a partial file.
use crate::environment::{MAX_PACKAGE_FILES, PackageRef, hash_dir, is_hash};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// A name no other writer in any process uses, for staging files.
fn unique(stem: &str, kind: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{stem}.{}-{}.{kind}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub const LISTING_SCHEMA: u32 = 1;
/// Default bound on everything a client's cache holds. Least recently used
/// packages are evicted past it; see [`Cache::prune`].
pub const CACHE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Objects and staging younger than this may belong to a fetch in progress
/// and are never pruned.
pub const IN_FLIGHT: std::time::Duration = std::time::Duration::from_secs(3600);
/// Largest single file a server may send.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Largest package a server may send.
pub const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Longest relative file path: the package path rule's.
pub const MAX_PATH: usize = crate::path::MAX_PATH;
/// File types that are native code or that Windows runs when opened. A
/// package that contains one is refused outright, whatever it is named.
pub const CODE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "drv", "ocx", "cpl", "scr", "com", "msi", "msp", "so", "dylib", "bat",
    "cmd", "ps1", "psm1", "psd1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "lnk", "url",
    "reg", "jar", "sh", "app", "pif", "appx", "msix",
];
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    /// Relative path with forward slashes.
    pub path: String,
    pub size: u64,
    /// SHA-256 of the file's bytes: the object it is stored and fetched as.
    pub sha256: String,
}

/// Every file of one package. Its entries determine the package hash, so a
/// client checks the listing against the hash it was promised before it
/// fetches a single file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listing {
    pub schema_version: u32,
    pub package: PackageRef,
    /// Sorted by path.
    pub files: Vec<FileEntry>,
}

/// Why a path cannot be part of a downloadable package, or None: the
/// package path rule ([`crate::path::problem`]), and no code.
pub fn path_problem(path: &str) -> Option<String> {
    if let Some(problem) = crate::path::problem(path) {
        return Some(problem);
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if let Some((_, extension)) = name.rsplit_once('.')
        && CODE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
    {
        return Some(format!(
            "`.{extension}` files are code; servers send data, never code"
        ));
    }
    None
}

/// The package hash of a set of files, by the same rule as
/// [`hash_dir`](crate::environment::hash_dir).
pub fn package_hash(files: &[FileEntry]) -> Result<String> {
    let mut total = Sha256::new();
    for file in files {
        total.update(file.path.as_bytes());
        total.update([0]);
        total.update(file.size.to_le_bytes());
        total.update(unhex(&file.sha256)?);
    }
    Ok(hex(&total.finalize()))
}

impl Listing {
    /// List `dir`, which must be the package `package` describes.
    pub fn of(dir: &Path, package: &PackageRef) -> Result<Self> {
        let mut files = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(current) = pending.pop() {
            for entry in fs::read_dir(&current)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                let path = entry.path();
                ensure!(!kind.is_symlink(), "Packages may not contain links");
                if kind.is_dir() {
                    pending.push(path);
                    continue;
                }
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(crate::environment::is_os_litter)
                {
                    continue;
                }
                let relative = path
                    .strip_prefix(dir)?
                    .to_str()
                    .context("Package file names must be UTF-8")?
                    .replace('\\', "/");
                let (sha256, size) = hash_file(&path)?;
                files.push(FileEntry {
                    path: relative,
                    size,
                    sha256,
                });
                ensure!(
                    files.len() <= MAX_PACKAGE_FILES,
                    "Package has more than {MAX_PACKAGE_FILES} files"
                );
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let listing = Self {
            schema_version: LISTING_SCHEMA,
            package: package.clone(),
            files,
        };
        listing
            .validate(package)
            .with_context(|| format!("Package `{}` cannot be sent to clients", package.id))?;
        Ok(listing)
    }

    /// Everything a client checks before fetching: the listing is for the
    /// package it asked for, every path is safe to create on Windows, no
    /// file is code, sizes are within budget and add up, and the entries
    /// hash to the promised package hash.
    pub fn validate(&self, expected: &PackageRef) -> Result<()> {
        ensure!(
            self.schema_version == LISTING_SCHEMA,
            "Unsupported listing schema"
        );
        ensure!(
            &self.package == expected,
            "Listing is for {}, not {}",
            self.package,
            expected
        );
        ensure!(
            self.files.len() <= MAX_PACKAGE_FILES,
            "Package has more than {MAX_PACKAGE_FILES} files"
        );
        let mut folded = BTreeSet::new();
        let mut total = 0_u64;
        for (index, file) in self.files.iter().enumerate() {
            if let Some(problem) = path_problem(&file.path) {
                bail!("{}: {problem}", file.path);
            }
            ensure!(is_hash(&file.sha256), "{}: invalid file hash", file.path);
            ensure!(
                file.size <= MAX_FILE_BYTES,
                "{}: file exceeds {MAX_FILE_BYTES} bytes",
                file.path
            );
            if index > 0 {
                ensure!(
                    self.files[index - 1].path < file.path,
                    "Listing is not in strict path order"
                );
            }
            // Windows paths are case-insensitive: two entries that differ
            // only by case would overwrite each other.
            ensure!(
                folded.insert(file.path.to_lowercase()),
                "{}: another file has the same name ignoring case",
                file.path
            );
            total = total
                .checked_add(file.size)
                .context("Package size overflows")?;
        }
        // A file may not also be a directory of another file.
        for file in &self.files {
            let lower = file.path.to_lowercase();
            let mut prefix = String::new();
            for segment in lower
                .split('/')
                .rev()
                .skip(1)
                .collect::<Vec<_>>()
                .iter()
                .rev()
            {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(segment);
                ensure!(
                    !folded.contains(&prefix),
                    "{}: `{prefix}` is both a file and a directory",
                    file.path
                );
            }
        }
        ensure!(
            total <= MAX_PACKAGE_BYTES,
            "Package exceeds {MAX_PACKAGE_BYTES} bytes"
        );
        ensure!(
            total == expected.size,
            "Listing size does not match the package"
        );
        ensure!(
            package_hash(&self.files)? == expected.hash,
            "Listing does not hash to the package it claims to be"
        );
        Ok(())
    }
}

/// A client's download cache (see the module docs).
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn open(root: &Path) -> Result<Self> {
        for dir in ["objects", "packages", "incoming"] {
            fs::create_dir_all(root.join(dir)).with_context(|| {
                format!("Could not create the package cache {}", root.display())
            })?;
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    fn object(&self, sha256: &str) -> PathBuf {
        self.root.join("objects").join(sha256)
    }

    fn package_dir(&self, package: &PackageRef) -> PathBuf {
        self.root.join("packages").join(&package.hash)
    }

    fn seal_path(&self, package: &PackageRef) -> PathBuf {
        self.root
            .join("packages")
            .join(format!("{}.seal", package.hash))
    }

    /// The installed directory for `package`, if this cache holds it intact.
    /// A directory that changed since it was sealed is hashed again; if it is
    /// no longer the package, it is removed so a fetch installs it afresh.
    pub fn installed(&self, package: &PackageRef) -> Option<PathBuf> {
        if !is_hash(&package.hash) {
            return None;
        }
        let dir = self.package_dir(package);
        if !dir.is_dir() {
            return None;
        }
        let seal = self.seal_path(package);
        let current = seal_of(&dir).ok()?;
        if fs::read_to_string(&seal).ok().as_deref() == Some(current.as_str()) {
            touch(&seal);
            return Some(dir);
        }
        match hash_dir(&dir) {
            Ok((hash, size)) if hash == package.hash && size == package.size => {
                let _ = bri_files::replace(&seal, current.as_bytes());
                Some(dir)
            }
            _ => {
                // Move aside first so a concurrent reader never sees a
                // half-removed package under its identity.
                let doomed = self
                    .root
                    .join("incoming")
                    .join(unique(&package.hash, "damaged"));
                if fs::rename(&dir, &doomed).is_ok() {
                    let _ = fs::remove_dir_all(&doomed);
                }
                let _ = fs::remove_file(&seal);
                None
            }
        }
    }

    /// Files of `listing` the cache does not hold yet, each object once.
    pub fn missing<'a>(&self, listing: &'a Listing) -> Vec<&'a FileEntry> {
        let mut seen = BTreeSet::new();
        listing
            .files
            .iter()
            .filter(|f| {
                seen.insert(&f.sha256)
                    && fs::metadata(self.object(&f.sha256))
                        .map_or(true, |m| !m.is_file() || m.len() != f.size)
            })
            .collect()
    }

    /// Start receiving the object `file` names. Nothing is visible in the
    /// cache until [`ObjectWriter::finish`] has checked size and hash.
    pub fn receive(&self, file: &FileEntry) -> Result<ObjectWriter> {
        ensure!(is_hash(&file.sha256), "Invalid object hash");
        ensure!(file.size <= MAX_FILE_BYTES, "Object exceeds budget");
        let partial = self
            .root
            .join("incoming")
            .join(unique(&file.sha256, "partial"));
        Ok(ObjectWriter {
            target: self.object(&file.sha256),
            file: fs::File::create(&partial)?,
            partial,
            expected: file.clone(),
            hasher: Sha256::new(),
            written: 0,
        })
    }

    /// Bound the cache to `max_bytes`. Packages in `keep` stay; others go
    /// least recently used first. Objects are only a download store (every
    /// installed package holds its own files), so settled ones are removed,
    /// as is staging left by crashed fetches. Anything touched within
    /// [`IN_FLIGHT`] may belong to a running fetch and stays.
    pub fn prune(&self, max_bytes: u64, keep: &[PackageRef]) -> Result<Pruned> {
        let now = std::time::SystemTime::now();
        let settled = |path: &Path| {
            fs::metadata(path)
                .and_then(|m| m.modified())
                .is_ok_and(|t| now.duration_since(t).unwrap_or_default() > IN_FLIGHT)
        };
        let mut pruned = Pruned::default();
        for dir in ["objects", "incoming"] {
            for entry in fs::read_dir(self.root.join(dir))? {
                let path = entry?.path();
                if settled(&path) {
                    pruned.bytes += size_of(&path);
                    let _ = fs::remove_dir_all(&path).or_else(|_| fs::remove_file(&path));
                }
            }
        }
        let keep: BTreeSet<&str> = keep.iter().map(|p| p.hash.as_str()).collect();
        let mut packages = Vec::new();
        let mut total = 0;
        for entry in fs::read_dir(self.root.join("packages"))? {
            let path = entry?.path();
            let Some(hash) = path
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|n| is_hash(n))
            else {
                continue;
            };
            let size = size_of(&path);
            total += size;
            let used = fs::metadata(path.with_extension("seal"))
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            if !keep.contains(hash) {
                packages.push((used, size, path.clone()));
            }
        }
        for dir in ["objects", "incoming"] {
            total += size_of(&self.root.join(dir));
        }
        packages.sort();
        for (_, size, path) in packages {
            if total <= max_bytes {
                break;
            }
            let _ = fs::remove_file(path.with_extension("seal"));
            if fs::remove_dir_all(&path).is_ok() {
                total -= size;
                pruned.bytes += size;
                pruned.packages += 1;
            }
        }
        pruned.remaining = total;
        Ok(pruned)
    }

    /// Materialize a package whose objects are all present, verify the
    /// result against the package hash, then publish it atomically.
    pub fn install(&self, listing: &Listing) -> Result<PathBuf> {
        listing.validate(&listing.package)?;
        if let Some(target) = self.installed(&listing.package) {
            return Ok(target);
        }
        let target = self.package_dir(&listing.package);
        let staging = self
            .root
            .join("incoming")
            .join(unique(&listing.package.hash, "package"));
        let result = (|| {
            for file in &listing.files {
                let destination = staging.join(&file.path);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                let source = self.object(&file.sha256);
                ensure!(source.is_file(), "{}: object not downloaded", file.path);
                fs::copy(&source, &destination)
                    .with_context(|| format!("{}: could not install", file.path))?;
                // Check the copy, not the object: the object may change
                // under us, the copy is what gets published.
                if hash_file(&destination)? != (file.sha256.clone(), file.size) {
                    let _ = fs::remove_file(&source);
                    bail!(
                        "{}: the cached copy was damaged and has been discarded; fetch again",
                        file.path
                    );
                }
            }
            fs::create_dir_all(&staging)?;
            // Independent check with the loader's own hash: whatever the
            // cache held, only the promised package is published.
            let (hash, size) = hash_dir(&staging)?;
            ensure!(
                hash == listing.package.hash && size == listing.package.size,
                "Installed files do not match package {}",
                listing.package
            );
            let seal = seal_of(&staging)?;
            match fs::rename(&staging, &target) {
                Ok(()) => {}
                // Another install of the same package won the race.
                Err(_) if self.installed(&listing.package).is_some() => {
                    let _ = fs::remove_dir_all(&staging);
                    return Ok(target.clone());
                }
                Err(error) => return Err(error.into()),
            }
            // Renaming keeps modification times, so the seal still holds.
            bri_files::replace(&self.seal_path(&listing.package), seal.as_bytes())?;
            Ok(target.clone())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }
}

/// What [`Cache::prune`] removed and what the cache holds afterwards.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pruned {
    pub packages: usize,
    pub bytes: u64,
    pub remaining: u64,
}

fn touch(path: &Path) {
    if let Ok(file) = fs::File::options().append(true).open(path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
}

/// Bytes under `path`, a file or a directory; unreadable parts count as 0.
fn size_of(path: &Path) -> u64 {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::read_dir(path)
            .map(|entries| entries.flatten().map(|e| size_of(&e.path())).sum())
            .unwrap_or(0),
        Ok(meta) => meta.len(),
        Err(_) => 0,
    }
}

/// One object being received. Dropping it without `finish` discards it.
pub struct ObjectWriter {
    target: PathBuf,
    partial: PathBuf,
    file: fs::File,
    expected: FileEntry,
    hasher: Sha256,
    written: u64,
}

impl ObjectWriter {
    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let written = self.written + bytes.len() as u64;
        ensure!(
            written <= self.expected.size,
            "{}: more bytes than the listing promised",
            self.expected.path
        );
        self.file.write_all(bytes)?;
        self.hasher.update(bytes);
        self.written = written;
        Ok(())
    }
    pub fn written(&self) -> u64 {
        self.written
    }
    pub fn remaining(&self) -> u64 {
        self.expected.size - self.written
    }
    pub fn finish(mut self) -> Result<()> {
        ensure!(
            self.written == self.expected.size,
            "{}: download ended early",
            self.expected.path
        );
        let hash = hex(&std::mem::take(&mut self.hasher).finalize());
        ensure!(
            hash == self.expected.sha256,
            "{}: downloaded bytes do not match their hash",
            self.expected.path
        );
        self.file.sync_all()?;
        match fs::rename(&self.partial, &self.target) {
            Ok(()) => Ok(()),
            // Another download of the same object won the race: same bytes.
            Err(_) if self.target.is_file() => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

impl Drop for ObjectWriter {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.partial);
    }
}

/// Read `length` bytes of `file` from `offset`, for serving an object.
pub fn read_range(path: &Path, offset: u64, length: usize) -> Result<Vec<u8>> {
    use std::io::Seek;
    let mut file = fs::File::open(path)?;
    file.seek(std::io::SeekFrom::Start(offset))?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Each file's relative path, size and modification time, one per line in
/// path order: cheap to recompute, and changed by any ordinary edit.
fn seal_of(dir: &Path) -> Result<String> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let modified = meta
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let relative = entry
                .path()
                .strip_prefix(dir)?
                .to_string_lossy()
                .replace('\\', "/");
            files.push(format!("{relative}\t{}\t{modified}", meta.len()));
            ensure!(files.len() <= MAX_PACKAGE_FILES, "Too many files");
        }
    }
    files.sort();
    Ok(files.join("\n"))
}

fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    let mut length = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        length += read as u64;
    }
    Ok((hex(&hasher.finalize()), length))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Result<[u8; 32]> {
    ensure!(is_hash(text), "Invalid hash");
    let mut out = [0; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packages::Side;

    fn package_in(dir: &Path, files: &[(&str, &[u8])]) -> PackageRef {
        for (path, bytes) in files {
            let path = dir.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let (hash, size) = hash_dir(dir).unwrap();
        PackageRef {
            id: "creeper".into(),
            version: "1.0.0".into(),
            side: Side::Shared,
            hash,
            size,
        }
    }

    fn fetch_all(cache: &Cache, source: &Path, listing: &Listing) {
        for file in cache.missing(listing) {
            let mut writer = cache.receive(file).unwrap();
            writer
                .write(&fs::read(source.join(&file.path)).unwrap())
                .unwrap();
            writer.finish().unwrap();
        }
    }

    #[test]
    fn listing_hash_matches_the_loader_and_install_round_trips() {
        let server = tempdir("server");
        let package = package_in(
            &server,
            &[
                ("package.json", b"{}"),
                ("models/creeper.glb", b"mesh"),
                ("sounds/hiss.wav", b"hiss"),
            ],
        );
        let listing = Listing::of(&server, &package).unwrap();
        assert_eq!(package_hash(&listing.files).unwrap(), package.hash);
        let cache_root = tempdir("cache");
        let cache = Cache::open(&cache_root).unwrap();
        assert!(cache.installed(&package).is_none());
        assert_eq!(cache.missing(&listing).len(), 3);
        fetch_all(&cache, &server, &listing);
        assert!(cache.missing(&listing).is_empty(), "objects are reused");
        let installed = cache.install(&listing).unwrap();
        assert_eq!(hash_dir(&installed).unwrap().0, package.hash);
        assert_eq!(cache.installed(&package), Some(installed));
        let _ = fs::remove_dir_all(server);
        let _ = fs::remove_dir_all(cache_root);
    }

    #[test]
    fn prune_evicts_least_recently_used_packages_but_never_kept_or_in_flight_ones() {
        let cache_root = tempdir("cache-prune");
        let cache = Cache::open(&cache_root).unwrap();
        let old = std::time::SystemTime::now() - 2 * IN_FLIGHT;
        let mut packages = Vec::new();
        for (i, name) in ["a", "b", "c"].into_iter().enumerate() {
            let server = tempdir(&format!("server-prune-{name}"));
            let package = package_in(&server, &[("blob.bin", &[i as u8; 1000])]);
            let listing = Listing::of(&server, &package).unwrap();
            fetch_all(&cache, &server, &listing);
            cache.install(&listing).unwrap();
            let _ = fs::remove_dir_all(server);
            packages.push(package);
        }
        // Last used: c longest ago, then a, then b.
        for (package, age) in packages.iter().zip([2, 3, 1]) {
            let seal = cache.seal_path(package);
            let file = fs::File::options().append(true).open(seal).unwrap();
            file.set_modified(old + std::time::Duration::from_secs(age))
                .unwrap();
        }
        // 3000 bytes of packages plus 3000 of objects still in flight: one
        // eviction fits the budget, and it is the least recently used.
        let pruned = cache.prune(5000, &[]).unwrap();
        assert_eq!((pruned.packages, pruned.remaining), (1, 5000), "{pruned:?}");
        assert!(!cache.seal_path(&packages[2]).exists());
        assert!(cache.seal_path(&packages[0]).exists());
        // Kept packages survive any budget.
        let pruned = cache.prune(0, &packages[1..2]).unwrap();
        assert_eq!(pruned.packages, 1, "{pruned:?}");
        assert!(cache.installed(&packages[1]).is_some(), "kept");
        assert!(cache.installed(&packages[0]).is_none());
        assert_eq!(fs::read_dir(cache_root.join("objects")).unwrap().count(), 3);
        for object in fs::read_dir(cache_root.join("objects")).unwrap() {
            let file = fs::File::options()
                .append(true)
                .open(object.unwrap().path())
                .unwrap();
            file.set_modified(old).unwrap();
        }
        let pruned = cache.prune(u64::MAX, &[]).unwrap();
        assert_eq!(
            (pruned.packages, pruned.bytes),
            (0, 3000),
            "settled objects go"
        );
        assert_eq!(pruned.remaining, 1000);
        let _ = fs::remove_dir_all(cache_root);
    }

    #[test]
    fn damage_at_rest_is_discarded_rather_than_trusted() {
        let server = tempdir("server-rest");
        let package = package_in(&server, &[("a.json", b"real"), ("b.bin", b"data")]);
        let listing = Listing::of(&server, &package).unwrap();
        let cache_root = tempdir("cache-rest");
        let cache = Cache::open(&cache_root).unwrap();
        fetch_all(&cache, &server, &listing);
        // Same size, different bytes: only the hash can tell.
        let object = cache.object(&listing.files[0].sha256);
        fs::write(&object, b"fake").unwrap();
        let error = cache.install(&listing).unwrap_err().to_string();
        assert!(error.contains("damaged"), "{error}");
        assert!(!object.exists(), "the damaged object is discarded");
        assert_eq!(cache.missing(&listing).len(), 1);
        fetch_all(&cache, &server, &listing);
        let installed = cache.install(&listing).unwrap();
        // An edit inside the installed package breaks its seal; the package
        // is re-hashed, found wrong and removed rather than returned.
        fs::write(installed.join("b.bin"), b"DATA").unwrap();
        assert_eq!(cache.installed(&package), None);
        assert!(!installed.exists());
        let again = cache.install(&listing).unwrap();
        assert_eq!(hash_dir(&again).unwrap().0, package.hash);
        let _ = fs::remove_dir_all(server);
        let _ = fs::remove_dir_all(cache_root);
    }

    #[test]
    fn corrupt_short_and_long_downloads_never_reach_the_cache() {
        let server = tempdir("server-bad");
        let package = package_in(&server, &[("a.json", b"real")]);
        let listing = Listing::of(&server, &package).unwrap();
        let cache_root = tempdir("cache-bad");
        let cache = Cache::open(&cache_root).unwrap();
        let file = &listing.files[0];
        let mut corrupt = cache.receive(file).unwrap();
        corrupt.write(b"fake").unwrap();
        assert!(
            corrupt
                .finish()
                .unwrap_err()
                .to_string()
                .contains("do not match")
        );
        let mut short = cache.receive(file).unwrap();
        short.write(b"re").unwrap();
        assert!(
            short
                .finish()
                .unwrap_err()
                .to_string()
                .contains("ended early")
        );
        let mut long = cache.receive(file).unwrap();
        assert!(long.write(b"really").is_err());
        drop(long);
        // An interrupted download leaves nothing behind.
        let interrupted = cache.receive(file).unwrap();
        drop(interrupted);
        assert_eq!(cache.missing(&listing).len(), 1);
        assert_eq!(
            fs::read_dir(cache_root.join("incoming")).unwrap().count(),
            0
        );
        assert!(cache.install(&listing).is_err());
        let _ = fs::remove_dir_all(server);
        let _ = fs::remove_dir_all(cache_root);
    }

    #[test]
    fn hostile_listings_are_refused_before_any_fetch() {
        let server = tempdir("server-hostile");
        let package = package_in(&server, &[("a.json", b"1"), ("b.json", b"2")]);
        let good = Listing::of(&server, &package).unwrap();
        let entry = |path: &str| FileEntry {
            path: path.into(),
            size: 1,
            sha256: "00".repeat(32),
        };
        for path in [
            "../evil.json",
            "/abs.json",
            "C:/x.json",
            "a\\b.json",
            "dir/./x.json",
            "trailing.",
            "nul.json",
            "COM1",
            "run.exe",
            "lib.DLL",
            "script.ps1",
            "tool.bat",
            "x\u{0}.json",
            "",
        ] {
            assert!(path_problem(path).is_some(), "{path:?} accepted");
        }
        assert!(path_problem("models/creeper.glb").is_none());
        // Every tampering changes the hash or breaks a rule.
        let mut renamed = good.clone();
        renamed.files[0].path = "A.json".into();
        assert!(renamed.validate(&package).is_err());
        let mut case_twins = good.clone();
        case_twins.files[1] = FileEntry {
            path: "a.jsoN".into(),
            ..case_twins.files[1].clone()
        };
        assert!(case_twins.validate(&package).is_err());
        let mut code = good.clone();
        code.files.push(entry("z.exe"));
        assert!(
            code.validate(&package)
                .unwrap_err()
                .to_string()
                .contains("code")
        );
        let mut swapped = good.clone();
        swapped.package.id = "zombies".into();
        assert!(swapped.validate(&package).is_err());
        let mut bomb = good.clone();
        bomb.files[0].size = MAX_FILE_BYTES + 1;
        assert!(bomb.validate(&package).is_err());
        let mut file_and_dir = good.clone();
        file_and_dir.files.push(entry("b.json/inner.json"));
        assert!(file_and_dir.validate(&package).is_err());
        good.validate(&package).unwrap();
        let _ = fs::remove_dir_all(server);
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bri-sync-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
