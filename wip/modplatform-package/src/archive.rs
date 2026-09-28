//! The package archive: one deterministic file per package side, named by the
//! SHA-256 of its bytes.
//!
//! A package directory splits into two sides. Everything outside `server/` is
//! client data: it is packed into the *data archive*, whose hash is the
//! package's identity on the wire, and is downloaded by joining clients.
//! `server/` holds server behaviour and never leaves the server. Only data
//! file types may appear on the client side; code never reaches a client.
//!
//! Layout (little endian):
//! ```text
//! b"BRIPKG\0\x01"                      magic and format version
//! u32 entry count
//! per entry, sorted by path:  u16 path length, path (UTF-8, '/'), u64 size, [u8; 32] sha256
//! the entries' bytes, in the same order
//! ```
//! The same directory always produces byte-identical archives, so equal
//! content has equal hashes on every machine.
use crate::diag::{Diagnostic, Diagnostics};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 8] = b"BRIPKG\0\x01";
pub const SERVER_DIR: &str = "server";

/// File types allowed in client data. Anything else is refused, so an
/// unexpected type is a precise error rather than a silent download.
pub const DATA_EXTENSIONS: &[&str] = &[
    "json", "txt", "md", "csv", "png", "jpg", "jpeg", "tga", "dds", "bmp", "ogg", "wav", "glb",
    "gltf", "bin", "blb", "bls", "ttf", "otf",
];
/// Types allowed under `server/`.
pub const SERVER_EXTENSIONS: &[&str] = &["luau", "json", "txt", "md"];
/// Types that can run code somewhere. Named so the error says why.
pub const CODE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "com", "scr", "msi", "bat", "cmd", "ps1", "psm1", "vbs", "js", "mjs",
    "jar", "py", "sh", "so", "dylib", "lua", "luau", "wasm", "cs", "gui", "lnk", "hta", "reg",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Downloaded by clients.
    Client,
    /// Stays on the server.
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_files: usize,
    pub max_file_bytes: u64,
    /// Per side.
    pub max_total_bytes: u64,
    pub max_path: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 2048,
            max_file_bytes: 32 * 1024 * 1024,
            max_total_bytes: 128 * 1024 * 1024,
            max_path: 160,
        }
    }
}

/// One file of a package directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Package-relative, '/'-separated.
    pub path: String,
    pub side: Side,
    pub size: u64,
    pub source: PathBuf,
}

/// Why a package-relative path is unsafe or unportable, or None.
pub fn path_problem(path: &str, limits: &Limits) -> Option<String> {
    if path.is_empty() || path.len() > limits.max_path {
        return Some(format!("path must be 1 to {} characters", limits.max_path));
    }
    if path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return Some("path must be relative, use '/', and contain no ':'".into());
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Some("path has an empty, '.' or '..' segment".into());
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Some(format!(
                "`{segment}` may contain only letters, digits, '.', '_' and '-' (no spaces)"
            ));
        }
        if segment.ends_with('.') {
            return Some(format!("`{segment}` ends with '.', which Windows drops"));
        }
        let stem = segment.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && stem.as_bytes()[3].is_ascii_digit());
        if reserved {
            return Some(format!("`{segment}` is a reserved Windows device name"));
        }
    }
    None
}

pub fn extension(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub fn side_of(path: &str) -> Side {
    if path.split('/').next() == Some(SERVER_DIR) && path.contains('/') {
        Side::Server
    } else {
        Side::Client
    }
}

/// The problem with storing `path` on its side, or None.
fn type_problem(path: &str, side: Side) -> Option<Diagnostic> {
    let ext = extension(path);
    let allowed = match side {
        Side::Client => DATA_EXTENSIONS,
        Side::Server => SERVER_EXTENSIONS,
    };
    if allowed.contains(&ext.as_str()) {
        return None;
    }
    if side == Side::Client && CODE_EXTENSIONS.contains(&ext.as_str()) {
        let hint = if ext == "luau" {
            "server behaviour belongs under server/; it runs on the server and is never downloaded"
        } else {
            "remove it; clients only ever receive data"
        };
        return Some(
            Diagnostic::error(
                "file.code_in_client_data",
                format!("`.{ext}` files can run code; packages never send code to clients"),
            )
            .at(path)
            .hint(hint),
        );
    }
    Some(
        Diagnostic::error(
            "file.type_not_allowed",
            format!(
                "`.{ext}` is not an allowed {} file type",
                if side == Side::Client { "client data" } else { "server" }
            ),
        )
        .at(path)
        .hint(format!("allowed: {}", allowed.join(", "))),
    )
}

/// Walk a package directory. Symlinks, unsafe paths, disallowed types and
/// anything over the limits are errors; dot-files are skipped.
pub fn collect(dir: &Path, limits: &Limits) -> (Vec<Entry>, Diagnostics) {
    let mut out = Diagnostics::default();
    let mut entries = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((path, relative)) = stack.pop() {
        let listing = match std::fs::read_dir(&path) {
            Ok(listing) => listing,
            Err(error) => {
                out.push(
                    Diagnostic::error("file.unreadable", format!("cannot list directory: {error}"))
                        .at(if relative.is_empty() { ".".into() } else { relative.clone() }),
                );
                continue;
            }
        };
        let mut children: Vec<_> = listing.filter_map(|e| e.ok()).collect();
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let name = child.file_name().to_string_lossy().into_owned();
            let child_relative = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            if name.starts_with('.') {
                out.push(
                    Diagnostic::info("file.hidden_skipped", "hidden file or directory is not packaged")
                        .at(&child_relative),
                );
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(child.path()) else {
                out.push(Diagnostic::error("file.unreadable", "cannot read metadata").at(&child_relative));
                continue;
            };
            if meta.file_type().is_symlink() {
                out.push(
                    Diagnostic::error("file.symlink", "symbolic links and junctions are not allowed")
                        .at(&child_relative)
                        .hint("copy the file into the package instead"),
                );
                continue;
            }
            if let Some(problem) = path_problem(&child_relative, limits) {
                out.push(
                    Diagnostic::error("file.path", problem)
                        .at(&child_relative)
                        .hint("rename it using lowercase letters, digits, '_' and '-'"),
                );
                continue;
            }
            if meta.is_dir() {
                stack.push((child.path(), child_relative));
                continue;
            }
            let side = side_of(&child_relative);
            if let Some(problem) = type_problem(&child_relative, side) {
                out.push(problem);
                continue;
            }
            if meta.len() > limits.max_file_bytes {
                out.push(
                    Diagnostic::error(
                        "file.too_large",
                        format!("{} bytes; the per-file limit is {}", meta.len(), limits.max_file_bytes),
                    )
                    .at(&child_relative),
                );
                continue;
            }
            entries.push(Entry {
                path: child_relative,
                side,
                size: meta.len(),
                source: child.path(),
            });
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    // Windows and many archive tools fold case: two paths that differ only in
    // case would overwrite each other on a client.
    let mut folded: BTreeMap<String, &str> = BTreeMap::new();
    for entry in &entries {
        if let Some(first) = folded.insert(entry.path.to_ascii_lowercase(), &entry.path) {
            out.push(
                Diagnostic::error(
                    "file.case_collision",
                    format!("`{}` and `{first}` differ only in letter case", entry.path),
                )
                .at(&entry.path),
            );
        }
    }
    for side in [Side::Client, Side::Server] {
        let files: Vec<_> = entries.iter().filter(|e| e.side == side).collect();
        let total: u64 = files.iter().map(|e| e.size).sum();
        let label = if side == Side::Client { "client data" } else { "server" };
        if files.len() > limits.max_files {
            out.push(Diagnostic::error(
                "package.too_many_files",
                format!("{} {label} files; the limit is {}", files.len(), limits.max_files),
            ));
        }
        if total > limits.max_total_bytes {
            out.push(Diagnostic::error(
                "package.too_large",
                format!("{total} bytes of {label}; the limit is {}", limits.max_total_bytes),
            ));
        }
    }
    (entries, out)
}

/// A built archive: its bytes' hash and size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Built {
    pub hash: String,
    pub size: u64,
    pub files: usize,
}

/// Write the archive of `entries` on `side` into `out`, hashing as it goes.
pub fn write(entries: &[Entry], side: Side, out: &mut impl Write) -> Result<Built> {
    let mut entries: Vec<&Entry> = entries.iter().filter(|e| e.side == side).collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut hashing = HashingWriter {
        inner: out,
        hash: Sha256::new(),
        size: 0,
    };
    // Hash each file first so the header can carry it; files are re-read for
    // the body and must not change in between.
    let mut digests = Vec::with_capacity(entries.len());
    for entry in &entries {
        let bytes = std::fs::read(&entry.source)
            .with_context(|| format!("Could not read {}", entry.path))?;
        ensure!(bytes.len() as u64 == entry.size, "{} changed while packaging", entry.path);
        digests.push(<[u8; 32]>::from(Sha256::digest(&bytes)));
    }
    hashing.write_all(MAGIC)?;
    hashing.write_all(&(entries.len() as u32).to_le_bytes())?;
    for (entry, digest) in entries.iter().zip(&digests) {
        hashing.write_all(&(entry.path.len() as u16).to_le_bytes())?;
        hashing.write_all(entry.path.as_bytes())?;
        hashing.write_all(&entry.size.to_le_bytes())?;
        hashing.write_all(digest)?;
    }
    for (entry, digest) in entries.iter().zip(&digests) {
        let bytes = std::fs::read(&entry.source)?;
        ensure!(
            <[u8; 32]>::from(Sha256::digest(&bytes)) == *digest,
            "{} changed while packaging",
            entry.path
        );
        hashing.write_all(&bytes)?;
    }
    Ok(Built {
        hash: hex(&hashing.hash.finalize()),
        size: hashing.size,
        files: entries.len(),
    })
}

/// Build the archive for `side` in memory.
pub fn build(entries: &[Entry], side: Side) -> Result<(Vec<u8>, Built)> {
    let mut bytes = Vec::new();
    let built = write(entries, side, &mut bytes)?;
    Ok((bytes, built))
}

struct HashingWriter<'a, W: Write> {
    inner: &'a mut W,
    hash: Sha256,
    size: u64,
}
impl<W: Write> Write for HashingWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(bytes)?;
        self.hash.update(&bytes[..n]);
        self.size += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}

/// True for a lowercase 64-digit SHA-256 hex string.
pub fn is_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// An archive's table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableEntry {
    pub path: String,
    pub size: u64,
    pub sha256: [u8; 32],
}

/// Read and check the header of an untrusted archive. Every path is
/// re-validated, sizes are bounded and the data must be exactly as long as
/// the table says.
pub fn read_table(reader: &mut impl Read, archive_size: u64, limits: &Limits) -> Result<Vec<TableEntry>> {
    let mut magic = [0; 8];
    reader.read_exact(&mut magic).context("Archive is truncated")?;
    ensure!(&magic == MAGIC, "Not a package archive (bad magic)");
    let mut count = [0; 4];
    reader.read_exact(&mut count)?;
    let count = u32::from_le_bytes(count) as usize;
    ensure!(count <= limits.max_files, "Archive lists {count} files; the limit is {}", limits.max_files);
    let mut table = Vec::with_capacity(count);
    let mut header = 12u64;
    let mut total = 0u64;
    let mut previous: Option<String> = None;
    for _ in 0..count {
        let mut len = [0; 2];
        reader.read_exact(&mut len)?;
        let len = u16::from_le_bytes(len) as usize;
        ensure!(len <= limits.max_path, "Archive path too long");
        let mut path = vec![0; len];
        reader.read_exact(&mut path)?;
        let path = String::from_utf8(path).context("Archive path is not UTF-8")?;
        if let Some(problem) = path_problem(&path, limits) {
            anyhow::bail!("Unsafe archive path `{path}`: {problem}");
        }
        ensure!(
            previous.as_ref().is_none_or(|p| p.to_ascii_lowercase() < path.to_ascii_lowercase()),
            "Archive paths are not sorted and unique"
        );
        let mut size = [0; 8];
        reader.read_exact(&mut size)?;
        let size = u64::from_le_bytes(size);
        ensure!(size <= limits.max_file_bytes, "Archive file `{path}` is too large");
        let mut sha256 = [0; 32];
        reader.read_exact(&mut sha256)?;
        total = total.checked_add(size).context("Archive size overflow")?;
        ensure!(total <= limits.max_total_bytes, "Archive contents exceed the size limit");
        header += 2 + len as u64 + 8 + 32;
        previous = Some(path.clone());
        table.push(TableEntry { path, size, sha256 });
    }
    ensure!(header + total == archive_size, "Archive length does not match its table");
    Ok(table)
}

/// Unpack a verified archive file into `dest`, checking every file's hash.
/// `dest` must not exist; the caller renames it into place.
pub fn unpack(archive: &Path, dest: &Path, limits: &Limits) -> Result<Vec<TableEntry>> {
    let size = std::fs::metadata(archive)?.len();
    let mut reader = std::io::BufReader::new(std::fs::File::open(archive)?);
    let table = read_table(&mut reader, size, limits)?;
    std::fs::create_dir_all(dest)?;
    for entry in &table {
        let target = dest.join(entry.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create_new(&target)
            .with_context(|| format!("Could not create {}", entry.path))?;
        let mut hash = Sha256::new();
        let mut remaining = entry.size;
        let mut buffer = vec![0; 64 * 1024];
        while remaining > 0 {
            let n = remaining.min(buffer.len() as u64) as usize;
            reader.read_exact(&mut buffer[..n])?;
            hash.update(&buffer[..n]);
            file.write_all(&buffer[..n])?;
            remaining -= n as u64;
        }
        ensure!(
            <[u8; 32]>::from(hash.finalize()) == entry.sha256,
            "File `{}` does not match its recorded hash",
            entry.path
        );
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(root: &Path, path: &str, bytes: &[u8]) {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
    }

    #[test]
    fn archives_are_deterministic_split_by_side_and_round_trip() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        // Same content written in a different order.
        write_file(a.path(), "package.json", b"{}");
        write_file(a.path(), "assets/b.png", b"png");
        write_file(a.path(), "server/main.luau", b"print(1)");
        write_file(b.path(), "server/main.luau", b"print(1)");
        write_file(b.path(), "assets/b.png", b"png");
        write_file(b.path(), "package.json", b"{}");
        let limits = Limits::default();
        let (ea, da) = collect(a.path(), &limits);
        let (eb, _) = collect(b.path(), &limits);
        assert!(!da.has_errors(), "{da:?}");
        let (bytes_a, built_a) = build(&ea, Side::Client).unwrap();
        let (bytes_b, built_b) = build(&eb, Side::Client).unwrap();
        assert_eq!(bytes_a, bytes_b);
        assert_eq!(built_a, built_b);
        assert_eq!(built_a.files, 2, "server/ stays out of the data archive");
        assert_eq!(hash_bytes(&bytes_a), built_a.hash);
        let (_, server) = build(&ea, Side::Server).unwrap();
        assert_eq!(server.files, 1);

        let archive = a.path().join("out.bripkg");
        std::fs::write(&archive, &bytes_a).unwrap();
        let dest = b.path().join("unpacked");
        let table = unpack(&archive, &dest, &limits).unwrap();
        assert_eq!(table.len(), 2);
        assert_eq!(std::fs::read(dest.join("assets").join("b.png")).unwrap(), b"png");
        assert!(!dest.join("server").exists());
    }

    #[test]
    fn unsafe_paths_types_and_links_are_refused() {
        let limits = Limits::default();
        for bad in ["../x.png", "a/../x.png", "/x.png", "a\\b.png", "c:x.png", "a b.png", "con.png",
            "LPT1.txt", "x.png.", "a//b.png"] {
            assert!(path_problem(bad, &limits).is_some(), "{bad}");
        }
        assert!(path_problem("assets/sounds/hiss-2.ogg", &limits).is_none());
        assert!(path_problem("console.png", &limits).is_none());

        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "payload.exe", b"MZ");
        write_file(dir.path(), "brain.luau", b"--");
        write_file(dir.path(), "notes.xyz", b"?");
        write_file(dir.path(), "server/model.png", b"png");
        // Windows folds case on disk, so case collisions are exercised
        // through a crafted archive table in `tampered_archives_fail_to_unpack`.
        write_file(dir.path(), ".git/config", b"x");
        let (_, diags) = collect(dir.path(), &limits);
        let mut codes = diags.codes();
        codes.sort();
        assert_eq!(
            codes,
            [
                "file.code_in_client_data",
                "file.code_in_client_data",
                "file.hidden_skipped",
                "file.type_not_allowed",
                "file.type_not_allowed",
            ]
        );
    }

    #[test]
    fn oversized_packages_and_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.bin", &[0; 100]);
        write_file(dir.path(), "b.bin", &[0; 100]);
        let limits = Limits {
            max_file_bytes: 150,
            max_total_bytes: 150,
            ..Limits::default()
        };
        let (_, diags) = collect(dir.path(), &limits);
        assert_eq!(diags.codes(), ["package.too_large"]);
        let limits = Limits {
            max_file_bytes: 50,
            ..Limits::default()
        };
        let (_, diags) = collect(dir.path(), &limits);
        assert_eq!(diags.codes(), ["file.too_large", "file.too_large"]);
    }

    #[test]
    fn tampered_archives_fail_to_unpack() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "src/a.json", b"{\"a\":1}");
        let limits = Limits::default();
        let (entries, _) = collect(&dir.path().join("src"), &limits);
        let (mut bytes, _) = build(&entries, Side::Client).unwrap();
        let archive = dir.path().join("x.bripkg");
        // Flip a data byte: the table hash no longer matches.
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(&archive, &bytes).unwrap();
        let error = unpack(&archive, &dir.path().join("out1"), &limits).unwrap_err();
        assert!(error.to_string().contains("does not match its recorded hash"), "{error}");
        // Truncate: the table no longer accounts for the length.
        bytes.pop();
        std::fs::write(&archive, &bytes).unwrap();
        assert!(unpack(&archive, &dir.path().join("out2"), &limits).is_err());
        // A hostile path in the table.
        let mut evil = MAGIC.to_vec();
        evil.extend(1u32.to_le_bytes());
        evil.extend(5u16.to_le_bytes());
        evil.extend(b"../x1");
        evil.extend(0u64.to_le_bytes());
        evil.extend([0; 32]);
        std::fs::write(&archive, &evil).unwrap();
        let error = unpack(&archive, &dir.path().join("out3"), &limits).unwrap_err();
        assert!(error.to_string().contains("Unsafe archive path"), "{error}");
        // Two paths that one Windows file would receive.
        let mut folded = MAGIC.to_vec();
        folded.extend(2u32.to_le_bytes());
        for path in [b"A.png", b"a.png"] {
            folded.extend(5u16.to_le_bytes());
            folded.extend(path);
            folded.extend(0u64.to_le_bytes());
            folded.extend(<[u8; 32]>::from(Sha256::digest(b"")));
        }
        std::fs::write(&archive, &folded).unwrap();
        let error = unpack(&archive, &dir.path().join("out4"), &limits).unwrap_err();
        assert!(error.to_string().contains("not sorted and unique"), "{error}");
    }
}
