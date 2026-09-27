//! Read-only view of a v20 installation: loose files plus Add-On ZIP members
//! addressed as `Add-Ons/<zip base name>/<member>`, like Torque's resource
//! manager. Lookups are case-insensitive (the install comes from Windows).

use anyhow::{Context, Result, ensure};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

/// Portable path policy, including Windows alternate streams and aliases.
pub(crate) fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        && path.split('/').all(|p| {
            let stem = p.split('.').next().unwrap_or_default().to_ascii_uppercase();
            let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || (stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'));
            !p.is_empty() && p != "." && p != ".." && !p.ends_with([' ', '.']) && !device
        })
}

#[derive(Debug, Clone)]
enum Origin {
    Loose(PathBuf),
    Zip { archive: PathBuf, member: String },
}

pub struct Vfs {
    /// lower-case virtual path -> (original-case virtual path, origin)
    entries: BTreeMap<String, (String, Origin)>,
    pub warnings: Vec<String>,
    reads: RefCell<BTreeMap<String, serde_json::Value>>,
}

impl Vfs {
    pub fn open(root: &Path) -> Result<Self> {
        let mut vfs = Vfs {
            entries: BTreeMap::new(),
            warnings: Vec::new(),
            reads: RefCell::new(BTreeMap::new()),
        };
        vfs.walk(root, root)?;
        Ok(vfs)
    }

    fn walk(&mut self, root: &Path, dir: &Path) -> Result<()> {
        let mut items: Vec<_> = fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .collect::<Result<_, _>>()?;
        items.sort_by_key(|e| e.file_name());
        for e in items {
            let p = e.path();
            let ft = e.file_type()?;
            // Do not follow linked source directories/files outside the installation.
            if ft.is_symlink() {
                self.warnings
                    .push(format!("{}: linked source skipped", p.display()));
                continue;
            }
            if ft.is_dir() {
                self.walk(root, &p)?;
                continue;
            }
            let rel = p
                .strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/");
            let is_addon_zip = rel.to_ascii_lowercase().starts_with("add-ons/")
                && rel.to_ascii_lowercase().ends_with(".zip")
                && rel.matches('/').count() == 1;
            if is_addon_zip {
                self.index_zip(&p, &rel);
            }
            self.entries.insert(
                rel.to_ascii_lowercase(),
                (rel.clone(), Origin::Loose(p.clone())),
            );
        }
        Ok(())
    }

    fn index_zip(&mut self, archive: &Path, rel: &str) {
        let base = &rel[..rel.len() - 4];
        let file = match fs::File::open(archive) {
            Ok(f) => f,
            Err(e) => {
                self.warnings.push(format!("{rel}: {e}"));
                return;
            }
        };
        let mut zip = match zip::ZipArchive::new(file) {
            Ok(z) => z,
            Err(e) => {
                self.warnings
                    .push(format!("{rel}: not a readable ZIP ({e}); skipped"));
                return;
            }
        };
        for i in 0..zip.len() {
            let Ok(f) = zip.by_index_raw(i) else { continue };
            if f.is_dir() {
                continue;
            }
            let member = f.name().to_string();
            if !safe_relative(&member) || f.size() > MAX_ASSET_BYTES {
                self.warnings.push(format!(
                    "{rel}:{member}: unsafe path or oversized member; skipped"
                ));
                continue;
            }
            let virt = format!("{base}/{member}");
            // Loose files take precedence over archived ones, as in Torque.
            self.entries.entry(virt.to_ascii_lowercase()).or_insert((
                virt,
                Origin::Zip {
                    archive: archive.to_path_buf(),
                    member,
                },
            ));
        }
    }

    /// Original-case virtual path for a (case-insensitive) path, if present.
    pub fn resolve(&self, path: &str) -> Option<&str> {
        self.entries
            .get(&path.replace('\\', "/").to_ascii_lowercase())
            .map(|(p, _)| p.as_str())
    }

    /// Try `path` with each extension in order (`""` = as given).
    pub fn resolve_with_ext(&self, path: &str, exts: &[&str]) -> Option<String> {
        exts.iter()
            .find_map(|e| self.resolve(&format!("{path}{e}")).map(str::to_string))
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let (_, origin) = self
            .entries
            .get(&path.replace('\\', "/").to_ascii_lowercase())
            .with_context(|| format!("{path} not found in install"))?;
        let bytes = match origin {
            Origin::Loose(p) => {
                let f = fs::File::open(p)?;
                ensure!(
                    f.metadata()?.len() <= MAX_ASSET_BYTES,
                    "oversized source {path}"
                );
                let mut buf = Vec::new();
                f.take(MAX_ASSET_BYTES + 1).read_to_end(&mut buf)?;
                buf
            }
            Origin::Zip { archive, member } => {
                let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
                let f = zip.by_name(member)?;
                ensure!(
                    f.size() <= MAX_ASSET_BYTES,
                    "oversized archive source {path}"
                );
                let mut buf = Vec::new();
                f.take(MAX_ASSET_BYTES + 1).read_to_end(&mut buf)?;
                buf
            }
        };
        ensure!(
            bytes.len() as u64 <= MAX_ASSET_BYTES,
            "oversized source {path}"
        );
        self.reads.borrow_mut().insert(
            path.into(),
            serde_json::json!({
                "virtual_path": path, "source": self.describe(path),
                "sha256": crate::sha(&bytes), "bytes": bytes.len()
            }),
        );
        Ok(bytes)
    }

    pub fn source_records(&self) -> Vec<serde_json::Value> {
        self.reads.borrow().values().cloned().collect()
    }

    /// Human-readable origin for provenance.
    pub fn describe(&self, path: &str) -> String {
        match self.entries.get(&path.to_ascii_lowercase()) {
            Some((p, Origin::Loose(_))) => p.clone(),
            Some((_, Origin::Zip { archive, member })) => format!(
                "{}:{}",
                archive
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                member
            ),
            None => path.to_string(),
        }
    }

    /// All virtual paths (original case) under `prefix` (case-insensitive).
    pub fn list(&self, prefix: &str) -> Vec<String> {
        let low = prefix.to_ascii_lowercase();
        self.entries
            .range(low.clone()..)
            .take_while(|(k, _)| k.starts_with(&low))
            .map(|(_, (p, _))| p.clone())
            .collect()
    }
}
