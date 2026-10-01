//! Where the player's classic Blockland Add-Ons are: the folders the game
//! reads old Add-Ons (zips and folders, as v20 and v21 keep them) from, to
//! import them on the player's own machine. Nothing here copies or ships
//! them; Import Add-On converts the player's own copy (`bri-addon-import`)
//! and a host sends what it converted to the players who join it, as v20
//! hosts sent their Add-Ons.
//!
//! The folders, first match by name wins:
//! 1. the content root's own [`DROP_DIR`] (drop a zip there, as in v20);
//! 2. folders the player added ([`FOLDERS_FILE`]);
//! 3. Blockland installed through Steam (v21), found from Steam's library
//!    list on Windows, macOS and Linux;
//! 4. the v20 install the game's content was generated from
//!    (`_regeneration/v20-path.txt`, written by `tools/bootstrap.py`).
//!
//! The first two belong to the content root. The last two are the machine's,
//! and are looked at only when a [`Discovery`] asks for them: the game passes
//! [`Discovery::machine`], and anything else (tests, tools) sees only what
//! its root holds.
//!
//! Each install is also the importer's `--reference`: its `base/` and other
//! Add-Ons satisfy the datablocks an Add-On builds on (Tier 2 on Tier 1).
use crate::library::DROP_DIR;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The folders the player added, under the content root.
pub const FOLDERS_FILE: &str = "classic-folders.json";
/// Folders the player may add.
pub const MAX_FOLDERS: usize = 16;
/// Steam libraries read from one `libraryfolders.vdf`.
const MAX_LIBRARIES: usize = 32;
/// The largest Steam library list read.
const MAX_VDF_BYTES: u64 = 256 * 1024;

/// Where a classic folder came from, as players are told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// The game's own Add-Ons drop folder.
    Drop,
    /// A folder the player added.
    Added,
    /// Blockland from Steam.
    Steam,
    /// The v20 install the game was set up from.
    V20,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Drop => "the game's Add-Ons folder",
            Self::Added => "a folder you added",
            Self::Steam => "Blockland on Steam",
            Self::V20 => "Blockland v20",
        }
    }
}

/// One folder of classic Add-Ons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub origin: Origin,
    /// The folder holding the Add-On zips and folders.
    pub add_ons: PathBuf,
    /// The Blockland install around it (its `base/` and `Add-Ons/`), when
    /// there is one: the importer's reference for what an Add-On builds on.
    pub install: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderList {
    schema_version: u32,
    #[serde(default)]
    folders: Vec<PathBuf>,
}

/// Where to look for classic Add-Ons beyond the content root's own folders.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovery {
    /// Steam installs whose libraries may hold Blockland.
    pub steam: Vec<PathBuf>,
    /// The v20 install the content root remembers.
    pub v20: bool,
}

impl Discovery {
    /// Only the content root's drop folder and the folders the player added.
    pub fn root_only() -> Self {
        Self::default()
    }
    /// This machine's Blockland: Steam wherever it is installed, and the
    /// remembered v20 install.
    pub fn machine() -> Self {
        Self {
            steam: steam_roots(),
            v20: true,
        }
    }
}

/// Every classic Add-On folder for the content root `root` that `discovery`
/// reaches, in the order their Add-Ons are matched by name.
pub fn folders(root: &Path, discovery: &Discovery) -> Vec<Folder> {
    let mut out = vec![Folder {
        origin: Origin::Drop,
        add_ons: root.join(DROP_DIR),
        install: None,
    }];
    for folder in added(root) {
        out.push(Folder {
            origin: Origin::Added,
            install: install_of(&folder),
            add_ons: add_ons_of(&folder),
        });
    }
    for steam_root in &discovery.steam {
        for library in steam_libraries(steam_root) {
            let install = library.join("steamapps").join("common").join("Blockland");
            if install.join(DROP_DIR).is_dir() {
                out.push(Folder {
                    origin: Origin::Steam,
                    add_ons: install.join(DROP_DIR),
                    install: Some(install),
                });
            }
        }
    }
    if discovery.v20
        && let Some(v20) = remembered_v20(root)
        && v20.join(DROP_DIR).is_dir()
    {
        out.push(Folder {
            origin: Origin::V20,
            add_ons: v20.join(DROP_DIR),
            install: Some(v20),
        });
    }
    // One entry per folder: the same install found twice (an added folder
    // that is also Steam's) keeps its first origin.
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|f| seen.insert(key(&f.add_ons)));
    out
}

/// The folders the player added, as saved.
pub fn added(root: &Path) -> Vec<PathBuf> {
    std::fs::read(root.join(FOLDERS_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<FolderList>(&bytes).ok())
        .map(|l| l.folders.into_iter().take(MAX_FOLDERS).collect())
        .unwrap_or_default()
}

/// Add `folder` (a Blockland install or an Add-Ons folder) to the folders
/// the game reads classic Add-Ons from.
pub fn add(root: &Path, folder: &Path) -> Result<()> {
    ensure!(folder.is_dir(), "{} is not a folder", folder.display());
    let folder = folder
        .canonicalize()
        .unwrap_or_else(|_| folder.to_path_buf());
    let mut list = added(root);
    if list.iter().any(|f| key(f) == key(&folder)) {
        return Ok(());
    }
    ensure!(
        list.len() < MAX_FOLDERS,
        "At most {MAX_FOLDERS} folders can be added"
    );
    list.push(folder);
    save(root, list)
}

/// Stop reading classic Add-Ons from `folder`. Nothing in it is touched.
pub fn remove(root: &Path, folder: &Path) -> Result<()> {
    let mut list = added(root);
    list.retain(|f| key(f) != key(folder));
    save(root, list)
}

fn save(root: &Path, folders: Vec<PathBuf>) -> Result<()> {
    let text = serde_json::to_string_pretty(&FolderList {
        schema_version: 1,
        folders,
    })?;
    let path = root.join(FOLDERS_FILE);
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, text + "\n").with_context(|| format!("Writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("Replacing {}", path.display()))
}

/// A folder named for an install holds its Add-Ons in `Add-Ons/`; any
/// other folder is taken to be an Add-Ons folder itself.
fn add_ons_of(folder: &Path) -> PathBuf {
    let inner = folder.join(DROP_DIR);
    if inner.is_dir() {
        inner
    } else {
        folder.to_path_buf()
    }
}

fn install_of(folder: &Path) -> Option<PathBuf> {
    if folder.join(DROP_DIR).is_dir() {
        return Some(folder.to_path_buf());
    }
    let parent = folder.parent()?;
    (folder
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(DROP_DIR))
        && parent.join("base").is_dir())
    .then(|| parent.to_path_buf())
}

fn key(path: &Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = path.to_string_lossy().replace('\\', "/");
    if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text
    }
}

fn remembered_v20(root: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(root.join("_regeneration").join("v20-path.txt")).ok()?;
    let path = PathBuf::from(text.trim());
    (!text.trim().is_empty()).then_some(path)
}

/// Where Steam may be installed on this machine.
pub fn steam_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(path) = std::env::var_os("BRI_STEAM") {
        out.push(PathBuf::from(path));
    }
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(dir) = std::env::var_os(var) {
                out.push(PathBuf::from(dir).join("Steam"));
            }
        }
        out.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        if cfg!(target_os = "macos") {
            out.push(home.join("Library/Application Support/Steam"));
        } else {
            out.push(home.join(".steam/steam"));
            out.push(home.join(".local/share/Steam"));
        }
    }
    out
}

/// The Steam libraries `steam_root` knows: itself and each `"path"` in its
/// `steamapps/libraryfolders.vdf`.
pub fn steam_libraries(steam_root: &Path) -> Vec<PathBuf> {
    let mut out = vec![steam_root.to_path_buf()];
    let vdf = steam_root.join("steamapps").join("libraryfolders.vdf");
    let text = std::fs::File::open(&vdf).ok().and_then(|file| {
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_VDF_BYTES), &mut bytes)
            .ok()?;
        String::from_utf8(bytes).ok()
    });
    for line in text.iter().flat_map(|t| t.lines()) {
        let fields: Vec<&str> = line.split('"').collect();
        // `"path"		"S:\\SteamLibrary"` splits to ["", path, gap, value, ""].
        if fields.len() >= 5 && fields[1].eq_ignore_ascii_case("path") {
            out.push(PathBuf::from(fields[3].replace("\\\\", "\\")));
        }
        if out.len() > MAX_LIBRARIES {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bri-classic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn steam_libraries_v20_and_added_folders_are_found_once_each() {
        let t = temp("folders");
        let root = t.join("content");
        std::fs::create_dir_all(root.join(DROP_DIR)).unwrap();
        // Steam in one place, Blockland in a second library.
        let steam = t.join("Steam");
        let library = t.join("SteamLibrary");
        let blockland = library.join("steamapps/common/Blockland");
        std::fs::create_dir_all(steam.join("steamapps")).unwrap();
        std::fs::create_dir_all(blockland.join(DROP_DIR)).unwrap();
        let escaped = library.to_string_lossy().replace('\\', "\\\\");
        std::fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{escaped}\"\n\t}}\n}}\n"
            ),
        )
        .unwrap();
        // The v20 install bootstrap remembered.
        let v20 = t.join("Blockland v20");
        std::fs::create_dir_all(v20.join(DROP_DIR)).unwrap();
        std::fs::create_dir_all(v20.join("base")).unwrap();
        std::fs::create_dir_all(root.join("_regeneration")).unwrap();
        std::fs::write(
            root.join("_regeneration/v20-path.txt"),
            format!("{}\n", v20.display()),
        )
        .unwrap();
        // The player adds an Add-Ons folder by itself, and Steam's again.
        let mine = t.join("My Add-Ons");
        std::fs::create_dir_all(&mine).unwrap();
        add(&root, &mine).unwrap();
        add(&root, &blockland).unwrap();
        add(&root, &mine).unwrap();
        let discovery = Discovery {
            steam: vec![steam.clone()],
            v20: true,
        };
        let found: Vec<(Origin, bool)> = folders(&root, &discovery)
            .iter()
            .map(|f| (f.origin, f.install.is_some()))
            .collect();
        assert_eq!(
            found,
            [
                (Origin::Drop, false),
                (Origin::Added, false),
                (Origin::Added, true),
                (Origin::V20, true),
            ],
            "Steam's install was added by hand first"
        );
        remove(&root, &blockland).unwrap();
        let found = folders(&root, &discovery);
        assert_eq!(found[2].origin, Origin::Steam);
        assert_eq!(found[2].install.as_deref(), Some(blockland.as_path()));
        assert_eq!(found[2].add_ons, blockland.join(DROP_DIR));
        // Without discovery only the root's own folders count.
        let own: Vec<Origin> = folders(&root, &Discovery::root_only())
            .iter()
            .map(|f| f.origin)
            .collect();
        assert_eq!(own, [Origin::Drop, Origin::Added]);
        let _ = std::fs::remove_dir_all(&t);
    }
}
