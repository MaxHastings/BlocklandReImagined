//! The content-addressed package cache.
//!
//! ```text
//! <root>/objects/<hash>.bripkg   verified data archives
//! <root>/unpacked/<hash>/        their files, for the game to read
//! <root>/tmp/                    downloads and unpacks in progress
//! ```
//! A hash names exactly one archive, so a package shared by many servers is
//! downloaded once, and nothing is ever trusted by name: an archive enters
//! `objects/` only after its bytes hash to its name, and is unpacked only
//! after every file matches the archive's table. Entries appear by atomic
//! rename, so a crash mid-download leaves nothing half-installed.
use crate::archive::{self, Limits, Side};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
    limits: Limits,
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Store {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        Self::with_limits(root, Limits::default())
    }

    pub fn with_limits(root: impl Into<PathBuf>, limits: Limits) -> Result<Self> {
        let root = root.into();
        for dir in ["objects", "unpacked", "tmp"] {
            std::fs::create_dir_all(root.join(dir))
                .with_context(|| format!("Could not create package cache {}", root.display()))?;
        }
        Ok(Self { root, limits })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn archive_path(&self, hash: &str) -> PathBuf {
        self.root.join("objects").join(format!("{hash}.bripkg"))
    }

    /// The unpacked files of an installed package.
    pub fn dir(&self, hash: &str) -> Option<PathBuf> {
        if !archive::is_hash(hash) {
            return None;
        }
        let dir = self.root.join("unpacked").join(hash);
        dir.is_dir().then_some(dir)
    }

    pub fn has(&self, hash: &str) -> bool {
        self.dir(hash).is_some()
    }

    fn temp(&self, label: &str) -> PathBuf {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        self.root
            .join("tmp")
            .join(format!("{label}-{}-{n}", std::process::id()))
    }

    /// Start receiving the archive `hash` of `size` bytes.
    pub fn begin(&self, hash: &str, size: u64) -> Result<Download> {
        ensure!(archive::is_hash(hash), "Invalid package hash");
        ensure!(size <= self.max_archive_bytes(), "Package archive is larger than the limit");
        let path = self.temp("download");
        let file = std::fs::File::create_new(&path)?;
        Ok(Download {
            store: self.clone(),
            expected: hash.into(),
            size,
            received: 0,
            hash: Sha256::new(),
            file: Some(std::io::BufWriter::new(file)),
            path,
        })
    }

    /// Largest archive the limits allow: table plus contents.
    pub fn max_archive_bytes(&self) -> u64 {
        let table = 12 + self.limits.max_files as u64 * (2 + self.limits.max_path as u64 + 40);
        table + self.limits.max_total_bytes
    }

    /// Install a complete archive already on disk whose bytes must hash to
    /// `hash`.
    pub fn insert_file(&self, archive: &Path, hash: &str) -> Result<PathBuf> {
        let size = std::fs::metadata(archive)?.len();
        let mut download = self.begin(hash, size)?;
        let mut reader = std::fs::File::open(archive)?;
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let n = std::io::Read::read(&mut reader, &mut buffer)?;
            if n == 0 {
                break;
            }
            download.write(&buffer[..n])?;
        }
        download.finish()
    }

    /// Pack a package directory's data side and install it, returning the
    /// data archive's hash. For servers and local tools; clients only ever
    /// install downloads.
    pub fn insert_dir(&self, package: &Path) -> Result<archive::Built> {
        let (entries, diagnostics) = archive::collect(package, &self.limits);
        diagnostics.into_result()?;
        let temp = self.temp("build");
        let built = {
            let mut file = std::io::BufWriter::new(std::fs::File::create_new(&temp)?);
            let built = archive::write(&entries, Side::Client, &mut file)?;
            file.flush()?;
            built
        };
        let installed = self.insert_file(&temp, &built.hash);
        let _ = std::fs::remove_file(&temp);
        installed?;
        Ok(built)
    }

    /// Delete installed packages not in `keep`. Returns how many were removed.
    pub fn prune(&self, keep: &[String]) -> Result<usize> {
        let mut removed = 0;
        for entry in std::fs::read_dir(self.root.join("unpacked"))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if archive::is_hash(&name) && !keep.contains(&name) {
                std::fs::remove_dir_all(entry.path())?;
                let _ = std::fs::remove_file(self.archive_path(&name));
                removed += 1;
            }
        }
        Ok(removed)
    }
}

/// An archive being received. Bytes are hashed as they arrive; nothing is
/// installed unless the total size and hash match exactly.
pub struct Download {
    store: Store,
    expected: String,
    size: u64,
    received: u64,
    hash: Sha256,
    file: Option<std::io::BufWriter<std::fs::File>>,
    path: PathBuf,
}

impl Download {
    pub fn received(&self) -> u64 {
        self.received
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            self.received + bytes.len() as u64 <= self.size,
            "Package sent more bytes than announced"
        );
        self.hash.update(bytes);
        self.file.as_mut().context("Download already finished")?.write_all(bytes)?;
        self.received += bytes.len() as u64;
        Ok(())
    }

    /// Verify and install. Returns the unpacked directory.
    pub fn finish(mut self) -> Result<PathBuf> {
        ensure!(self.received == self.size, "Package download is incomplete");
        let mut file = self.file.take().context("Download already finished")?;
        file.flush()?;
        drop(file);
        let actual = archive::hex(&std::mem::take(&mut self.hash).finalize());
        ensure!(
            actual == self.expected,
            "Package hash mismatch: expected {}, received {actual}",
            self.expected
        );
        let store = &self.store;
        let object = store.archive_path(&self.expected);
        if !object.exists() {
            rename_or_exists(&self.path, &object)?;
        }
        if let Some(dir) = store.dir(&self.expected) {
            return Ok(dir);
        }
        let staging = store.temp("unpack");
        let result = archive::unpack(&object, &staging, &store.limits);
        if let Err(error) = result {
            let _ = std::fs::remove_dir_all(&staging);
            // A verified hash with a bad table means the server published a
            // malformed archive; do not keep it.
            let _ = std::fs::remove_file(&object);
            return Err(error.context("Package archive is malformed"));
        }
        let dest = store.root.join("unpacked").join(&self.expected);
        if std::fs::rename(&staging, &dest).is_err() {
            // Someone else installed it first.
            let _ = std::fs::remove_dir_all(&staging);
            ensure!(dest.is_dir(), "Could not install package {}", self.expected);
        }
        Ok(dest)
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

fn rename_or_exists(from: &Path, to: &Path) -> Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) if to.exists() => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_install_only_when_the_hash_matches() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("pkg");
        std::fs::create_dir_all(package.join("assets")).unwrap();
        std::fs::write(package.join("package.json"), b"{}").unwrap();
        std::fs::write(package.join("assets").join("a.png"), b"png").unwrap();
        let server = Store::open(dir.path().join("server")).unwrap();
        let built = server.insert_dir(&package).unwrap();
        let bytes = std::fs::read(server.archive_path(&built.hash)).unwrap();

        let client = Store::open(dir.path().join("client")).unwrap();
        assert!(!client.has(&built.hash));
        // Wrong bytes under the right name: refused, nothing installed.
        let mut bad = client.begin(&built.hash, bytes.len() as u64).unwrap();
        let mut tampered = bytes.clone();
        tampered[20] ^= 0xff;
        bad.write(&tampered).unwrap();
        assert!(bad.finish().unwrap_err().to_string().contains("hash mismatch"));
        assert!(!client.has(&built.hash));
        // Too many bytes: refused before they are written.
        let mut long = client.begin(&built.hash, 4).unwrap();
        assert!(long.write(&bytes).is_err());
        drop(long);
        // Chunked, correct download installs and unpacks.
        let mut good = client.begin(&built.hash, bytes.len() as u64).unwrap();
        for chunk in bytes.chunks(7) {
            good.write(chunk).unwrap();
        }
        let installed = good.finish().unwrap();
        assert_eq!(std::fs::read(installed.join("assets").join("a.png")).unwrap(), b"png");
        assert!(client.has(&built.hash));
        // No temporary files are left behind.
        assert_eq!(std::fs::read_dir(client.root().join("tmp")).unwrap().count(), 0);
        assert_eq!(client.prune(&[]).unwrap(), 1);
        assert!(!client.has(&built.hash));
    }
}
