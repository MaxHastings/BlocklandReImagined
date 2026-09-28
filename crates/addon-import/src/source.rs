//! Reads one Add-On (a zip or a folder) into virtual paths
//! `Add-Ons/<Name>/<member>`, the way Blockland mounts it. Bounded, read-only.
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};

pub const MAX_MEMBERS: usize = 4096;
pub const MAX_MEMBER_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct File {
    /// Virtual path with the source's spelling.
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub origin: String,
    pub format: &'static str,
    pub sha256: String,
    /// Keyed by lower-case virtual path; Torque resolves paths case-insensitively.
    pub files: BTreeMap<String, File>,
    /// Members refused while reading, with why.
    pub refused: Vec<(String, String)>,
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Source {
    pub fn dir(&self) -> String {
        format!("Add-Ons/{}", self.name)
    }
    pub fn get(&self, virtual_path: &str) -> Option<&File> {
        self.files.get(&virtual_path.to_ascii_lowercase())
    }
    /// The member name inside the Add-On (`shotgun.dts`).
    pub fn member<'a>(&self, file: &'a File) -> &'a str {
        &file.path[self.dir().len() + 1..]
    }
}

fn safe(member: &str) -> Result<String, String> {
    let m = member.replace('\\', "/");
    if m.starts_with('/') || m.contains(':') || m.split('/').any(|p| p == "..") {
        return Err("path escapes the Add-On".into());
    }
    Ok(m)
}

pub fn read(path: &Path) -> Result<Source> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Add-On path has no name")?
        .to_owned();
    let mut members: Vec<(String, Vec<u8>)> = vec![];
    let mut refused = vec![];
    let mut total = 0usize;
    let (format, sha256) = if path.is_dir() {
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    refused.push((entry.path().display().to_string(), "symlink".into()));
                } else if kind.is_dir() {
                    stack.push(entry.path());
                } else {
                    let rel = entry
                        .path()
                        .strip_prefix(path)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    ensure!(
                        entry.metadata()?.len() <= MAX_MEMBER_BYTES,
                        "Oversized member {rel}"
                    );
                    let bytes = std::fs::read(entry.path())?;
                    total += bytes.len();
                    members.push((rel, bytes));
                }
            }
            ensure!(members.len() <= MAX_MEMBERS, "Member budget exceeded");
        }
        members.sort();
        let mut h = Sha256::new();
        for (m, b) in &members {
            h.update(m.as_bytes());
            h.update(hash(b).as_bytes());
        }
        ("folder", format!("{:x}", h.finalize()))
    } else if bri_convert::archive::is_rar(path)? {
        // Some community ".zip" files are RAR archives; read through 7z.
        for (member, data) in bri_convert::archive::rar_assets(path)? {
            total += data.len();
            members.push((member, data));
        }
        ensure!(members.len() <= MAX_MEMBERS, "Member budget exceeded");
        ("rar", hash(&std::fs::read(path)?))
    } else {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let digest = hash(&bytes);
        let mut zip =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a zip archive")?;
        ensure!(zip.len() <= MAX_MEMBERS, "Member budget exceeded");
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let member = entry.name().to_owned();
            ensure!(
                entry.size() <= MAX_MEMBER_BYTES,
                "Oversized member {member}"
            );
            let mut data = vec![];
            entry
                .by_ref()
                .take(MAX_MEMBER_BYTES + 1)
                .read_to_end(&mut data)?;
            ensure!(
                data.len() as u64 <= MAX_MEMBER_BYTES,
                "Oversized member {member}"
            );
            total += data.len();
            members.push((member, data));
        }
        ("zip", digest)
    };
    ensure!(total <= MAX_TOTAL_BYTES, "Aggregate byte budget exceeded");
    // Some zips wrap everything in one folder named after the Add-On.
    let wrapper = format!("{}/", name.to_ascii_lowercase());
    let wrapped = !members.is_empty()
        && members
            .iter()
            .all(|(m, _)| m.to_ascii_lowercase().starts_with(&wrapper));
    let mut files = BTreeMap::new();
    for (member, bytes) in members {
        let member = if wrapped {
            member[wrapper.len()..].to_owned()
        } else {
            member
        };
        match safe(&member) {
            Ok(m) => {
                let path = format!("Add-Ons/{name}/{m}");
                if files
                    .insert(
                        path.to_ascii_lowercase(),
                        File {
                            path: path.clone(),
                            bytes,
                        },
                    )
                    .is_some()
                {
                    refused.push((path, "duplicate path differing only in case".into()));
                }
            }
            Err(why) => refused.push((member, why)),
        }
    }
    Ok(Source {
        name,
        origin: path.display().to_string(),
        format,
        sha256,
        files,
        refused,
    })
}

/// Resolves a script path the way Torque does relative to `script`:
/// `./x` is the script's folder, `~/x` is `base/x`, otherwise root-relative.
pub fn resolve(script: &str, value: &str) -> String {
    let v = value.trim().replace('\\', "/");
    if let Some(rest) = v.strip_prefix("./") {
        let dir = script.rsplit_once('/').map_or("", |(d, _)| d);
        format!("{dir}/{rest}")
    } else if let Some(rest) = v.strip_prefix("~/") {
        format!("base/{rest}")
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn torque_paths() {
        assert_eq!(resolve("Add-Ons/A/b/c.cs", "./x.dts"), "Add-Ons/A/b/x.dts");
        assert_eq!(resolve("Add-Ons/A/c.cs", "~/data/x"), "base/data/x");
        assert_eq!(
            resolve("Add-Ons/A/c.cs", "Add-ons/B/y.blb"),
            "Add-ons/B/y.blb"
        );
        assert!(safe("../x").is_err() && safe("a/b").is_ok());
    }
}
