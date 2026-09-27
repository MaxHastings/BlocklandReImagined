//! Read-only virtual file system over a Blockland installation: loose files
//! plus `Add-Ons/<Name>.zip` members mounted at `Add-Ons/<Name>/...`, with
//! Windows-style case-insensitive lookup. Nothing is ever written back.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Container {
    Loose(PathBuf),
    Zip { archive: String, member: String },
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// Virtual path with original case, `/` separators.
    pub path: String,
    pub container: Container,
    /// `base`, the add-on directory name, or `root` for top-level files.
    pub package: String,
    pub size: u64,
}

impl Entry {
    pub fn container_label(&self) -> String {
        match &self.container {
            Container::Loose(_) => "loose".into(),
            Container::Zip { archive, .. } => format!("zip:{archive}"),
        }
    }
    /// Evidence label: `Add-Ons/X.zip!member` or the virtual path.
    pub fn evidence_label(&self) -> String {
        match &self.container {
            Container::Loose(_) => self.path.clone(),
            Container::Zip { archive, member } => format!("{archive}!{member}"),
        }
    }
    pub fn extension(&self) -> String {
        self.path
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    }
}

#[derive(Debug, Clone)]
pub struct Archive {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub members: usize,
}

pub struct Vfs {
    root: PathBuf,
    /// lower-case virtual path -> entry
    entries: BTreeMap<String, Entry>,
    pub archives: Vec<Archive>,
    /// Problems encountered while indexing (unreadable archives, duplicates).
    pub problems: Vec<String>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn package_of(path: &str) -> String {
    let mut parts = path.split('/');
    match parts.next() {
        Some(top) if top.eq_ignore_ascii_case("base") => "base".into(),
        Some(top) if top.eq_ignore_ascii_case("Add-Ons") => {
            parts.next().unwrap_or("Add-Ons").to_string()
        }
        _ => "root".into(),
    }
}

impl Vfs {
    /// Index `base/` and `Add-Ons/` (loose files and zip members).
    pub fn open(root: &Path) -> Result<Self, String> {
        if !root.join("base").is_dir() {
            return Err(format!(
                "{} does not look like a Blockland installation (no base/)",
                root.display()
            ));
        }
        let mut vfs = Vfs {
            root: root.to_path_buf(),
            entries: BTreeMap::new(),
            archives: Vec::new(),
            problems: Vec::new(),
        };
        let mut loose = Vec::new();
        for top in ["base", "Add-Ons"] {
            let dir = root.join(top);
            if dir.is_dir() {
                walk(&dir, &mut loose)
                    .map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
            }
        }
        // Top-level executable is indexed for engine-string evidence only.
        for f in ["blocklandv20.exe", "Blockland.exe"] {
            let p = root.join(f);
            if p.is_file() {
                loose.push(p);
            }
        }
        loose.sort();
        for p in loose {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            let is_addon_zip = rel.to_ascii_lowercase().starts_with("add-ons/")
                && rel.to_ascii_lowercase().ends_with(".zip")
                && rel.matches('/').count() == 1;
            if is_addon_zip {
                vfs.index_zip(&p, &rel);
                continue;
            }
            vfs.insert(Entry {
                package: package_of(&rel),
                path: rel,
                container: Container::Loose(p),
                size,
            });
        }
        Ok(vfs)
    }

    fn insert(&mut self, e: Entry) {
        let key = e.path.to_ascii_lowercase();
        if let Some(prev) = self.entries.get(&key) {
            // Loose files shadow archive members in Torque's resource manager.
            let prev_loose = matches!(prev.container, Container::Loose(_));
            let new_loose = matches!(e.container, Container::Loose(_));
            self.problems.push(format!(
                "duplicate virtual path {} ({} vs {})",
                e.path,
                prev.container_label(),
                e.container_label()
            ));
            if prev_loose || !new_loose {
                return;
            }
        }
        self.entries.insert(key, e);
    }

    fn index_zip(&mut self, path: &Path, rel: &str) {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.problems.push(format!("cannot read {rel}: {e}"));
                return;
            }
        };
        let stem = rel
            .rsplit('/')
            .next()
            .unwrap_or(rel)
            .trim_end_matches(".zip")
            .trim_end_matches(".ZIP")
            .to_string();
        let sha = sha256_hex(&bytes);
        let mut zip = match zip::ZipArchive::new(std::io::Cursor::new(bytes.as_slice())) {
            Ok(z) => z,
            Err(e) => {
                self.problems
                    .push(format!("cannot open archive {rel}: {e}"));
                return;
            }
        };
        let mut members = 0;
        for i in 0..zip.len() {
            let Ok(f) = zip.by_index(i) else {
                self.problems
                    .push(format!("unreadable member {i} in {rel}"));
                continue;
            };
            if f.is_dir() {
                continue;
            }
            let member = f.name().replace('\\', "/");
            let vpath = format!("Add-Ons/{stem}/{member}");
            members += 1;
            let size = f.size();
            drop(f);
            self.insert(Entry {
                path: vpath,
                container: Container::Zip {
                    archive: rel.to_string(),
                    member,
                },
                package: stem.clone(),
                size,
            });
        }
        self.archives.push(Archive {
            path: rel.to_string(),
            sha256: sha,
            bytes: path.metadata().map(|m| m.len()).unwrap_or(0),
            members,
        });
    }

    pub fn get(&self, virtual_path: &str) -> Option<&Entry> {
        self.entries
            .get(&virtual_path.replace('\\', "/").to_ascii_lowercase())
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    pub fn read(&self, e: &Entry) -> Result<Vec<u8>, String> {
        match &e.container {
            Container::Loose(p) => fs::read(p).map_err(|err| format!("{}: {err}", p.display())),
            Container::Zip { archive, member } => {
                let file = fs::File::open(self.root.join(archive))
                    .map_err(|err| format!("{archive}: {err}"))?;
                let mut z =
                    zip::ZipArchive::new(file).map_err(|err| format!("{archive}: {err}"))?;
                let mut f = z
                    .by_name(member)
                    .map_err(|err| format!("{archive}!{member}: {err}"))?;
                let mut v = Vec::with_capacity(f.size() as usize);
                f.read_to_end(&mut v)
                    .map_err(|err| format!("{archive}!{member}: {err}"))?;
                Ok(v)
            }
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            walk(&entry.path(), out)?;
        } else if ft.is_file() {
            out.push(entry.path());
        }
    }
    Ok(())
}

/// Normalise a Torque path reference relative to the script that contains it.
/// `~/x` -> `<script's top directory>/x`, `./x` -> `<script dir>/x`.
pub fn resolve_script_path(script_virtual_path: &str, raw: &str) -> String {
    let raw = raw.trim().replace('\\', "/");
    let script_dir = script_virtual_path
        .rsplit_once('/')
        .map(|(d, _)| d)
        .unwrap_or("");
    let joined = if let Some(rest) = raw.strip_prefix("~/") {
        let top = script_virtual_path.split('/').next().unwrap_or("");
        format!("{top}/{rest}")
    } else if raw.starts_with("./") || raw.starts_with("../") {
        format!("{script_dir}/{raw}")
    } else {
        raw.clone()
    };
    let mut parts: Vec<&str> = Vec::new();
    for p in joined.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::resolve_script_path as r;

    #[test]
    fn torque_path_forms() {
        assert_eq!(
            r(
                "base/client/scripts/allClientScripts-Vanilla.cs",
                "~/data/sound/error.wav"
            ),
            "base/data/sound/error.wav"
        );
        assert_eq!(
            r("Add-Ons/Weapon_Gun/server.cs", "./gunShot1.wav"),
            "Add-Ons/Weapon_Gun/gunShot1.wav"
        );
        assert_eq!(r("Add-Ons/X/a/b.cs", "../y.wav"), "Add-Ons/X/y.wav");
        assert_eq!(
            r(
                "Add-Ons/Sound_Synth4/server.cs",
                "base/data/sound/notes/Synth 4/Synth4_00.wav"
            ),
            "base/data/sound/notes/Synth 4/Synth4_00.wav"
        );
    }
}
