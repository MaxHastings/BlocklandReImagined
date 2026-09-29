//! The standalone game: one `BlocklandReImagined.exe` that carries a packaged
//! release (the release folder's zip) appended to this launcher.
//!
//! The file ends with a footer: the zip's SHA-256 (32 bytes), its length
//! (u64, little-endian) and [`MAGIC`]. A code signature, when there is one,
//! follows the footer; [`find_payload`] reads the PE header to skip it.
//!
//! On start the launcher installs the payload into `<root>/Game`, where root
//! is the per-user data folder the game already keeps settings, saves and
//! logs in (`%LOCALAPPDATA%\BlocklandReImagined`). The game there is ordinary:
//! its Add-Ons, imports and package choices live in `Game/content`, so they
//! stay with the player, never beside the exe. A newer exe replaces the base
//! files and carries every file the player added across ([`install`]).
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

pub const MAGIC: &[u8; 8] = b"BRISFX01";
pub const FOOTER: u64 = 32 + 8 + 8;
/// The installed game, under the per-user data folder.
pub const GAME_DIR: &str = "Game";
/// Which payload `Game` holds and its base files, written last.
pub const MARKER: &str = ".standalone.json";
pub const CLIENT: &str = "bri-client.exe";
const PACKAGES: &str = "content/packages.json";
const DISABLED: &str = "content/packages-disabled.json";

/// The message when an upgrade finds the old game still running.
pub const CLOSE_GAME: &str = "Blockland ReImagined is still running from an older version. Close it, then start the new one again.";

/// The per-user data folder: `BRI_STANDALONE_ROOT` (tests), else the
/// folder the game keeps its state in.
pub fn default_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("BRI_STANDALONE_ROOT").filter(|r| !r.is_empty()) {
        return Ok(PathBuf::from(root));
    }
    let local = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is missing")?;
    Ok(PathBuf::from(local).join("BlocklandReImagined"))
}

/// Where the payload sits inside the exe.
#[derive(Debug, Clone)]
pub struct Payload {
    pub file: PathBuf,
    pub offset: u64,
    pub len: u64,
    pub sha256: [u8; 32],
}

impl Payload {
    pub fn id(&self) -> String {
        self.sha256.iter().map(|b| format!("{b:02x}")).collect()
    }
    fn open(&self) -> Result<Slice> {
        let file =
            File::open(&self.file).with_context(|| format!("opening {}", self.file.display()))?;
        Ok(Slice {
            file,
            start: self.offset,
            len: self.len,
            pos: 0,
        })
    }
}

/// The payload region of the exe as its own seekable file.
struct Slice {
    file: File,
    start: u64,
    len: u64,
    pos: u64,
}

impl Read for Slice {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.len.saturating_sub(self.pos);
        let n = (buf.len() as u64).min(left) as usize;
        if n == 0 {
            return Ok(0);
        }
        self.file.seek(SeekFrom::Start(self.start + self.pos))?;
        let read = self.file.read(&mut buf[..n])?;
        self.pos += read as u64;
        Ok(read)
    }
}

impl Seek for Slice {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before the payload",
            ));
        }
        self.pos = pos as u64;
        Ok(self.pos)
    }
}

fn read_at(file: &mut File, at: u64, buf: &mut [u8]) -> io::Result<()> {
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(buf)
}

/// Where an Authenticode signature starts, if the exe carries one.
fn signature_offset(file: &mut File, len: u64) -> Option<u64> {
    let mut dos = [0u8; 64];
    read_at(file, 0, &mut dos).ok()?;
    if &dos[..2] != b"MZ" {
        return None;
    }
    let pe = u32::from_le_bytes(dos[60..64].try_into().ok()?) as u64;
    let mut head = [0u8; 26];
    read_at(file, pe, &mut head).ok()?;
    if &head[..4] != b"PE\0\0" {
        return None;
    }
    let optional = pe + 24;
    let directories = match u16::from_le_bytes([head[24], head[25]]) {
        0x10b => optional + 96,
        0x20b => optional + 112,
        _ => return None,
    };
    let mut security = [0u8; 8];
    read_at(file, directories + 4 * 8, &mut security).ok()?;
    let offset = u32::from_le_bytes(security[..4].try_into().ok()?) as u64;
    let size = u32::from_le_bytes(security[4..].try_into().ok()?) as u64;
    (size > 0 && offset > 0 && offset + size <= len).then_some(offset)
}

/// Find the game appended to `exe`.
pub fn find_payload(exe: &Path) -> Result<Payload> {
    let mut file = File::open(exe).with_context(|| format!("opening {}", exe.display()))?;
    let len = file.metadata()?.len();
    let end = signature_offset(&mut file, len).unwrap_or(len);
    // A signature is aligned to 8 bytes, so up to 7 bytes of padding may
    // follow the footer.
    for pad in 0..8u64 {
        let Some(at) = end.checked_sub(FOOTER + pad) else {
            break;
        };
        let mut footer = [0u8; FOOTER as usize];
        read_at(&mut file, at, &mut footer)?;
        if &footer[40..] != MAGIC {
            continue;
        }
        let size = u64::from_le_bytes(footer[32..40].try_into()?);
        let offset = at
            .checked_sub(size)
            .context("the game inside this exe is truncated")?;
        return Ok(Payload {
            file: exe.to_path_buf(),
            offset,
            len: size,
            sha256: footer[..32].try_into()?,
        });
    }
    bail!("This exe carries no game; download BlocklandReImagined.exe again.")
}

/// The installed payload, from `Game/.standalone.json`.
#[derive(Debug, Default)]
struct Marker {
    payload: String,
    files: HashSet<String>,
}

fn read_marker(game: &Path) -> Option<Marker> {
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(game.join(MARKER)).ok()?).ok()?;
    Some(Marker {
        payload: value.get("payload")?.as_str()?.to_string(),
        files: value
            .get("files")?
            .as_array()?
            .iter()
            .filter_map(|f| f.as_str().map(String::from))
            .collect(),
    })
}

/// Only one launcher installs at a time.
struct Lock(PathBuf);

impl Lock {
    fn acquire(path: PathBuf) -> Result<Self> {
        let start = Instant::now();
        loop {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self(path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    let age = fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| SystemTime::now().duration_since(t).ok());
                    if age.is_some_and(|a| a > Duration::from_secs(600)) {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    ensure!(
                        start.elapsed() < Duration::from_secs(180),
                        "another launcher is still unpacking the game ({})",
                        path.display()
                    );
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => return Err(e).with_context(|| format!("creating {}", path.display())),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Install the payload into `root/Game` unless it is there already, and
/// return that folder.
pub fn install(payload: &Payload, root: &Path) -> Result<PathBuf> {
    fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    let game = root.join(GAME_DIR);
    let id = payload.id();
    let current = |game: &Path| {
        read_marker(game).is_some_and(|m| m.payload == id) && game.join(CLIENT).is_file()
    };
    if current(&game) {
        return Ok(game);
    }
    let _lock = Lock::acquire(root.join("Game.lock"))?;
    if current(&game) {
        return Ok(game);
    }
    let pid = std::process::id();
    // Unpacking left over from a launcher that did not finish.
    for entry in fs::read_dir(root)?.flatten() {
        if entry.file_name().to_string_lossy().starts_with("Game.new-") {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    let staging = root.join(format!("Game.new-{pid}"));
    let files = match extract(payload, &staging) {
        Ok(files) => files,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    let old = root.join(format!("Game.old-{pid}"));
    let upgrading = game.exists();
    if upgrading && fs::rename(&game, &old).is_err() {
        let _ = fs::remove_dir_all(&staging);
        bail!(CLOSE_GAME);
    }
    let finish = || -> Result<()> {
        if upgrading {
            let base = read_marker(&old).map(|m| m.files).unwrap_or_default();
            carry_player_files(&old, &staging, &base)?;
            merge_packages(&old, &staging)?;
        }
        let mut listed: Vec<_> = files.iter().cloned().collect();
        listed.sort();
        let marker = serde_json::json!({ "schema_version": 1, "payload": id, "files": listed });
        fs::write(staging.join(MARKER), serde_json::to_vec_pretty(&marker)?)?;
        fs::rename(&staging, &game)
            .with_context(|| format!("moving the game into {}", game.display()))
    };
    if let Err(e) = finish() {
        // Put the old game back rather than leave the player without one;
        // anything already carried across stays in the staging folder.
        if upgrading && !game.exists() {
            let _ = fs::rename(&old, &game);
        }
        return Err(e.context(format!("updating the game in {}", game.display())));
    }
    if upgrading {
        let _ = fs::remove_dir_all(&old);
    }
    Ok(game)
}

/// Unpack the payload into `dir` after checking its hash. The zip holds one
/// top-level folder (the release folder), which is dropped. Returns the
/// installed files' relative paths.
fn extract(payload: &Payload, dir: &Path) -> Result<HashSet<String>> {
    let mut hasher = Sha256::new();
    io::copy(&mut payload.open()?, &mut hasher)?;
    ensure!(
        hasher.finalize().as_slice() == payload.sha256,
        "The game inside this exe is damaged; download BlocklandReImagined.exe again."
    );
    let mut zip =
        zip::ZipArchive::new(payload.open()?).context("reading the game inside this exe")?;
    let mut top: Option<std::ffi::OsString> = None;
    let mut files = HashSet::new();
    fs::create_dir_all(dir)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry
            .enclosed_name()
            .with_context(|| format!("unsafe path in the game: {}", entry.name()))?;
        let mut parts = name.components();
        let first = parts
            .next()
            .context("empty path in the game")?
            .as_os_str()
            .to_owned();
        match &top {
            None => top = Some(first),
            Some(t) => ensure!(
                *t == first,
                "the game inside this exe has more than one top folder"
            ),
        }
        let relative: PathBuf = parts.collect();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let path = dir.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&path).with_context(|| format!("writing {}", path.display()))?;
        io::copy(&mut entry, &mut out)?;
        files.insert(slash(&relative));
    }
    ensure!(
        files.contains(CLIENT),
        "the game inside this exe has no {CLIENT}"
    );
    Ok(files)
}

fn slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            walk(&entry.path(), out)?;
        } else if kind.is_file() {
            out.push(entry.path());
        }
    }
    Ok(())
}

/// Move what the player added (Add-Ons, imports, logs, package choices)
/// from the old game to the new one. Files the old version shipped (`base`)
/// stay behind, as do paths the new version ships itself.
fn carry_player_files(old: &Path, new: &Path, base: &HashSet<String>) -> Result<()> {
    let mut found = Vec::new();
    walk(old, &mut found)?;
    for path in found {
        let relative = slash(path.strip_prefix(old)?);
        if relative == MARKER || relative == PACKAGES || base.contains(&relative) {
            continue;
        }
        let target = new.join(&relative);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::rename(&path, &target).is_err() {
            fs::copy(&path, &target).with_context(|| format!("keeping {relative}"))?;
        }
    }
    Ok(())
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn ids(list: &serde_json::Value) -> HashSet<String> {
    list.get("packages")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
        .filter_map(|p| p.get("id").and_then(|i| i.as_str()).map(String::from))
        .collect()
}

/// The new `packages.json`: the version's own list, less the optional
/// packages the player turned off, plus the Add-Ons they turned on.
fn merge_packages(old: &Path, new: &Path) -> Result<()> {
    let Some(before) = read_json(&old.join(PACKAGES)) else {
        return Ok(());
    };
    let target = new.join(PACKAGES);
    let mut list = read_json(&target).context("the new game has no content/packages.json")?;
    let enabled_before = ids(&before);
    let disabled_before = read_json(&new.join(DISABLED))
        .map(|d| ids(&d))
        .unwrap_or_default();
    let shipped = ids(&list);
    let Some(packages) = list.get_mut("packages").and_then(|p| p.as_array_mut()) else {
        return Ok(());
    };
    packages.retain(|p| {
        let id = p.get("id").and_then(|i| i.as_str()).unwrap_or_default();
        p.get("role").is_some() || enabled_before.contains(id) || !disabled_before.contains(id)
    });
    for package in before
        .get("packages")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
    {
        let (Some(id), Some(dir)) = (
            package.get("id").and_then(|i| i.as_str()),
            package.get("dir").and_then(|d| d.as_str()),
        ) else {
            continue;
        };
        if !shipped.contains(id) && new.join("content").join(dir).is_dir() {
            packages.push(package.clone());
        }
    }
    let enabled = ids(&list);
    fs::write(&target, serde_json::to_vec_pretty(&list)?)?;
    // A package is on or off, never both.
    if let Some(mut disabled) = read_json(&new.join(DISABLED)) {
        if let Some(entries) = disabled.get_mut("packages").and_then(|p| p.as_array_mut()) {
            entries.retain(|p| {
                !p.get("id")
                    .and_then(|i| i.as_str())
                    .is_some_and(|id| enabled.contains(id))
            });
        }
        fs::write(new.join(DISABLED), serde_json::to_vec_pretty(&disabled)?)?;
    }
    Ok(())
}

/// Append `zip` to `stub` as a standalone exe at `out` (what the packaging
/// script does; kept here for tests).
pub fn assemble(stub: &Path, zip: &Path, out: &Path) -> Result<()> {
    let mut bytes = fs::read(stub)?;
    let payload = fs::read(zip)?;
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&Sha256::digest(&payload));
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(MAGIC);
    fs::write(out, bytes)?;
    Ok(())
}
