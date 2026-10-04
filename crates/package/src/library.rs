//! The package library: every package installed under a content root,
//! whether it is enabled, what it says about itself, and what is wrong with
//! it. This is the mechanism behind the in-game Add-Ons screen; the screen
//! only shows it and asks it for changes.
//!
//! - **Enabled** packages are the ones `packages.json` lists: exactly what the
//!   host and the client load.
//! - **Disabled** packages keep their exact entry (side, role, directory) in
//!   `packages-disabled.json`, so turning one back on restores it unchanged.
//! - **Discovered** packages are directories under the root holding a
//!   `package.json` manifest that neither file lists yet (an Add-On someone
//!   imported or copied in). They start disabled.
//!
//! Changes are planned first ([`Library::plan`]) so a caller can show what
//! else must change (dependencies enabled, dependents disabled) and why a
//! change is refused, then applied ([`Library::apply`]) by rewriting both
//! files atomically. Base game packages (reserved ids) are always enabled.
//!
//! Design: `docs/architecture/mod-manager.md`.
use crate::API_LEVEL;
use crate::diag::{Diagnostic, Diagnostics, Severity};
use crate::id::{self, Requirement, Version};
use crate::packages::{
    MAX_PACKAGES, PACKAGES_FILE, PACKAGES_SCHEMA, PackageEntry, PackageSet, Side,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Entries for installed packages that are turned off.
pub const DISABLED_FILE: &str = "packages-disabled.json";
/// A mod package's own manifest.
pub const MANIFEST_FILE: &str = "package.json";
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
/// How deep under the content root discovery looks for manifests
/// (`packages/stresslab/stresslab-hud` is three levels).
pub const MAX_SCAN_DEPTH: usize = 3;
/// Directories discovery visits at most, so a huge tree stays cheap.
pub const MAX_SCAN_DIRS: usize = 4096;
/// Content kinds that only the server reads. A discovered package that
/// provides nothing else defaults to `server`.
pub const SERVER_KINDS: &[&str] = &[
    "behaviour",
    "script",
    "world",
    "entity",
    "archetype",
    "mode",
    "data",
];
/// Content kinds only clients draw. A discovered package that provides
/// nothing else defaults to `client`; any other package to `shared`.
pub const CLIENT_KINDS: &[&str] = &["model", "hud"];

/// What a package's manifest says about itself, read leniently for display.
/// Validation of the manifest is the package runtime's job; a manifest the
/// library cannot read at all is reported as `library.manifest`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub api: Option<u32>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub provenance: serde_json::Value,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    /// Add-Ons it works with when they are on, as `dependencies` when they
    /// are.
    #[serde(default)]
    pub optional_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub provides: Vec<Provided>,
    /// Add-Ons turned on with this one, after it, when installed: an
    /// import's host rules (which depend on it, so turning it off turns
    /// them off too), or an Add-On its scripts loaded themselves.
    #[serde(default)]
    pub companions: Vec<String>,
    /// Client code (`client.module`), which runs on each player's screen;
    /// [`CodeOwner`] says who decides whether it runs.
    #[serde(default)]
    pub client: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provided {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub id: String,
}

impl PackageInfo {
    /// `provenance.source` when the manifest names one (`original`,
    /// `blockland-addon`, ...).
    pub fn source(&self) -> Option<&str> {
        self.provenance.get("source").and_then(|s| s.as_str())
    }
    /// Whether it ships with the game as a bundled original:
    /// `tools/addon_bundle.py` marks each one's manifest
    /// `provenance.bundled` (crediting its authors), which no conversion the
    /// player makes carries. The classic Add-Ons folder never owns one.
    pub fn bundled(&self) -> bool {
        self.provenance.get("bundled").is_some_and(|b| !b.is_null())
    }
    /// The side it loads on ([`side_for_package`]).
    pub fn side(&self) -> Option<Side> {
        side_for_package(
            self.provides.iter().map(|p| p.kind.as_str()),
            CodeOwner::of(self.client.as_ref()),
        )
    }
    /// Provided kinds with how many of each, in first-seen order.
    pub fn kinds(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for p in &self.provides {
            match out.iter_mut().find(|(k, _)| *k == p.kind) {
                Some((_, n)) => *n += 1,
                None => out.push((p.kind.clone(), 1)),
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    /// The entry as `packages.json` (enabled) or `packages-disabled.json`
    /// lists it, or as discovery derived it.
    pub package: PackageEntry,
    pub enabled: bool,
    /// Base game package: always enabled, never disabled from the library.
    pub required: bool,
    /// Found by discovery; no list names it yet.
    pub discovered: bool,
    /// The package's manifest, when it has one. Base packages do not.
    pub info: Option<PackageInfo>,
    /// Problems with this package, worst first.
    pub problems: Vec<Diagnostic>,
}

impl LibraryEntry {
    pub fn id(&self) -> &str {
        &self.package.id
    }
    /// Display name: the manifest's, else the id.
    pub fn name(&self) -> &str {
        self.info
            .as_ref()
            .map(|i| i.name.as_str())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(&self.package.id)
    }
    pub fn has_errors(&self) -> bool {
        self.problems.iter().any(|d| d.severity == Severity::Error)
    }
    fn dependencies(&self) -> impl Iterator<Item = (&String, &String)> {
        self.info.iter().flat_map(|i| i.dependencies.iter())
    }
}

/// A requested change and everything it implies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub id: String,
    pub enable: bool,
    /// Other packages this change also turns on (dependencies, dependencies
    /// first) or off (dependents), in the order they change.
    pub also: Vec<String>,
    /// Why the change cannot be made. Empty when it can.
    pub refused: Vec<Diagnostic>,
}

impl Plan {
    pub fn allowed(&self) -> bool {
        self.refused.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    root: PathBuf,
    /// Enabled packages in load order, then disabled ones by name.
    pub entries: Vec<LibraryEntry>,
    /// Old Blockland add-ons dropped into [`DROP_DIR`], by name.
    pub legacy: Vec<LegacyAddOn>,
    /// Problems with the lists themselves.
    pub problems: Diagnostics,
}

/// The side an Add-On providing these content kinds loads on: `server` when
/// it provides only [`SERVER_KINDS`], `client` when only [`CLIENT_KINDS`],
/// otherwise `shared` (weapons, bricks and other gameplay data both need).
/// None when it mixes server and client kinds, which no side can load. The
/// Add-Ons screen and `bri-addon-check` both use this one rule.
pub fn side_for_kinds<'a>(kinds: impl IntoIterator<Item = &'a str>) -> Option<Side> {
    side_for_package(kinds, CodeOwner::None)
}

/// Who decides whether an Add-On's client code (`client` in its manifest)
/// runs in a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeOwner {
    /// It has no client code.
    None,
    /// The host: the code runs for everyone on a server that runs the
    /// Add-On, and for no one on a server that does not. Joiners download
    /// it. What everyone sees in the world (a ragdoll, a weapon's beam)
    /// looks the same for everyone this way. The default.
    Host,
    /// Each player, for their own screen only (`"personal": true` in the
    /// `client` section): it runs wherever that player plays and is never
    /// sent to anyone. For a HUD, a crosshair or a look only they see.
    Player,
}

impl CodeOwner {
    /// Read from a manifest's `client` section.
    pub fn of(client: Option<&serde_json::Value>) -> Self {
        match client {
            None => Self::None,
            Some(c) if c.get("personal").and_then(|p| p.as_bool()) == Some(true) => Self::Player,
            Some(_) => Self::Host,
        }
    }
}

/// [`side_for_kinds`] for a whole Add-On with its client code (`code`).
/// Client code the host decides on ([`CodeOwner::Host`]) makes an Add-On
/// `shared`, so the server sends it to joiners and a joiner runs exactly
/// the host's: on each player's screen, with no traffic of its own. A
/// personal one ([`CodeOwner::Player`]) that provides nothing else, or
/// only models and HUD panels, is `client`: each player's own choice.
/// Client code cannot ride on a `server` Add-On, which clients never load.
/// The release packagers (`tools/package_playtest.ps1`, `.sh`,
/// `package_mac.sh`) use the same rule.
pub fn side_for_package<'a>(
    kinds: impl IntoIterator<Item = &'a str>,
    code: CodeOwner,
) -> Option<Side> {
    let kinds: Vec<&str> = kinds.into_iter().collect();
    let has = |set: &[&str]| kinds.iter().any(|k| set.contains(k));
    let only = |set: &[&str]| kinds.iter().all(|k| set.contains(k));
    if kinds.is_empty() {
        Some(match code {
            CodeOwner::Player => Side::Client,
            CodeOwner::None | CodeOwner::Host => Side::Shared,
        })
    } else if has(SERVER_KINDS) && (has(CLIENT_KINDS) || code != CodeOwner::None) {
        None
    } else if only(SERVER_KINDS) {
        Some(Side::Server)
    } else if only(CLIENT_KINDS) {
        Some(match code {
            CodeOwner::Host => Side::Shared,
            CodeOwner::None | CodeOwner::Player => Side::Client,
        })
    } else {
        Some(Side::Shared)
    }
}

/// Bring each Add-On's side in `set` (not the base game's) in step with its
/// manifest under `root`: the manifest decides it ([`PackageInfo::side`]);
/// a list's copy is only a record of it, and may predate a change to the
/// rule or to the Add-On. An Add-On whose manifest cannot be read, or that
/// mixes sides, keeps its listed side, and its loading reports why.
pub fn follow_manifest_sides(root: &Path, set: &mut PackageSet) {
    for entry in set.packages.iter_mut().filter(|e| e.role.is_none()) {
        if let Some(side) = read_info(&root.join(&entry.dir).join(MANIFEST_FILE))
            .filter(|info| info.id == entry.id)
            .and_then(|info| info.side())
        {
            entry.side = side;
        }
    }
}

/// Keep each listed Add-On's companions (an import's host rules) in step
/// with it, whatever wrote the list: on right after it while it is on, off
/// while it is not. A list written before an Add-On's companions were
/// installed (a release that lacked them, an Import before its port had
/// rules) gains them on the next load; one listing a companion whose Add-On
/// is off loses it. A companion is found beside its Add-On, in the same
/// folder, by its manifest's id; one not installed (the player deleted it)
/// is left out, as [`Library::plan`] leaves it. Returns the ids turned on.
pub fn follow_companions(root: &Path, set: &mut PackageSet) -> Vec<String> {
    let mut beside = Beside::new(root);
    let mut turned_on = Vec::new();
    // Companions the listed Add-On at `i` names.
    let named = |set: &PackageSet, i: usize| -> Vec<String> {
        let entry = &set.packages[i];
        if entry.role.is_some() {
            return vec![];
        }
        read_info(&root.join(&entry.dir).join(MANIFEST_FILE))
            .filter(|info| info.id == entry.id)
            .map(|info| info.companions)
            .unwrap_or_default()
    };
    let mut owned: BTreeSet<String> = BTreeSet::new();
    let mut i = 0;
    while i < set.packages.len() {
        let mut at = i + 1;
        for companion in named(set, i) {
            owned.insert(companion.clone());
            if let Some(j) = set.packages.iter().position(|p| p.id == companion) {
                // Listed already: it loads after the Add-On it depends on.
                if j < i {
                    let entry = set.packages.remove(j);
                    i -= 1;
                    at -= 1;
                    set.packages.insert(at, entry);
                    at += 1;
                } else {
                    at = at.max(j + 1);
                }
                continue;
            }
            let Some(entry) = beside.entry(&set.packages[i].dir, &companion) else {
                continue;
            };
            set.packages.insert(at, entry);
            turned_on.push(companion);
            at += 1;
        }
        i += 1;
    }
    // A companion listed on while the Add-On naming it is installed but not
    // on goes off with it.
    let orphans: BTreeSet<String> = set
        .packages
        .iter()
        .filter(|p| p.role.is_none() && !owned.contains(&p.id))
        .filter(|p| beside.owner(&p.dir, &p.id).is_some())
        .map(|p| p.id.clone())
        .collect();
    set.packages.retain(|p| !orphans.contains(&p.id));
    turned_on
}

/// The installed Add-Ons of each folder holding a listed one, each folder
/// read once: where [`follow_companions`] finds companions and the Add-Ons
/// naming them.
struct Beside<'a> {
    root: &'a Path,
    folders: BTreeMap<PathBuf, Vec<(String, PackageInfo)>>,
}

impl<'a> Beside<'a> {
    fn new(root: &'a Path) -> Self {
        Self {
            root,
            folders: BTreeMap::new(),
        }
    }

    /// The Add-Ons installed in the folder holding `dir`, by folder name.
    fn folder(&mut self, dir: &str) -> &[(String, PackageInfo)] {
        let folder = Path::new(dir)
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        let root = self.root;
        self.folders.entry(folder).or_insert_with_key(|folder| {
            std::fs::read_dir(root.join(folder))
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().into_string().ok()?;
                    Some((name, read_info(&e.path().join(MANIFEST_FILE))?))
                })
                .collect()
        })
    }

    /// The installed Add-On `id` beside `dir`, as a list entry.
    fn entry(&mut self, dir: &str, id: &str) -> Option<PackageEntry> {
        let (name, info) = self.folder(dir).iter().find(|(_, info)| info.id == id)?;
        let folder = Path::new(dir).parent().unwrap_or(Path::new(""));
        let dir = if folder.as_os_str().is_empty() {
            name.clone()
        } else {
            format!("{}/{name}", folder.to_string_lossy().replace('\\', "/"))
        };
        Some(PackageEntry {
            id: info.id.clone(),
            version: info.version.clone(),
            side: info.side()?,
            dir,
            role: None,
        })
    }

    /// The installed Add-On beside `dir` that names `id` its companion.
    fn owner(&mut self, dir: &str, id: &str) -> Option<String> {
        self.folder(dir)
            .iter()
            .find(|(_, info)| info.companions.iter().any(|c| c == id))
            .map(|(_, info)| info.id.clone())
    }
}

/// Where players drop old Blockland add-on zips and folders, as in v20.
pub const DROP_DIR: &str = "Add-Ons";
/// Where importing one writes its package.
pub const IMPORT_DIR: &str = "addons";
/// Where a conversion is written before it is moved into [`IMPORT_DIR`]
/// ([`Library::install_staged`]), and where the copy it replaces waits
/// until it is in. Discovery never looks here (it skips `.` folders), nor
/// does anything that looks beside an Add-On for its host rules.
pub const STAGING_DIR: &str = ".addon-staging";
/// Legacy add-ons listed at most.
pub const MAX_LEGACY: usize = 1024;

/// An old Blockland add-on (zip or folder) waiting in [`DROP_DIR`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyAddOn {
    /// File stem, as v20 named it (`Weapon_Shotgun`).
    pub name: String,
    pub path: PathBuf,
    /// The installed package the player's conversion of it became, matched
    /// by the provenance the importer records (`Blockland Add-On <name>
    /// (...)`). Never a package the game ships ([`Self::included`]).
    pub imported_as: Option<String>,
    /// The package the game ships as this Add-On ([`PackageInfo::bundled`]):
    /// the dropped copy is not needed, so it is never converted, and the
    /// shipped one is never replaced or removed with it.
    pub included: Option<String>,
}

impl Library {
    /// Read both lists and discover unlisted packages under `root`.
    /// `packages.json` falls back to the base game's list and the installed
    /// default Add-Ons when absent, as the loaders do. A list that does not parse is an error: the library
    /// never rewrites a file it could not read.
    pub fn scan(root: &Path) -> Result<Self> {
        ensure!(root.is_dir(), "Missing content root {}", root.display());
        let enabled = PackageSet::load_root(root)?;
        let disabled_path = root.join(DISABLED_FILE);
        let disabled = if disabled_path.exists() {
            let mut disabled = PackageSet::load(&disabled_path)?;
            follow_manifest_sides(root, &mut disabled);
            // A companion its Add-On's list turned on is on, not off.
            let mut beside = Beside::new(root);
            disabled.packages.retain(|p| {
                !enabled.packages.iter().any(|e| e.id == p.id)
                    || beside.owner(&p.dir, &p.id).is_none()
            });
            disabled
        } else {
            PackageSet {
                schema_version: PACKAGES_SCHEMA,
                packages: vec![],
            }
        };
        let mut problems = Diagnostics::default();
        let mut entries = Vec::new();
        let mut ids = BTreeSet::new();
        for (package, on) in enabled
            .packages
            .iter()
            .map(|p| (p, true))
            .chain(disabled.packages.iter().map(|p| (p, false)))
        {
            // Listed off, but its folder is gone (an earlier version shipped
            // it, or the player deleted it): nothing to show or turn on. The
            // next change to the lists drops it.
            if !on && !root.join(&package.dir).is_dir() {
                continue;
            }
            if !ids.insert(package.id.clone()) {
                // Listed in both files: the enabled entry wins.
                problems.push(
                    Diagnostic::warning(
                        "library.listed_twice",
                        format!(
                            "`{}` is in both {PACKAGES_FILE} and {DISABLED_FILE}",
                            package.id
                        ),
                    )
                    .hint("it is treated as enabled; turning it off or on again tidies the lists"),
                );
                continue;
            }
            entries.push(entry(root, package.clone(), on, false));
        }
        let listed_dirs: BTreeSet<String> = entries
            .iter()
            .map(|e| e.package.dir.to_ascii_lowercase())
            .collect();
        let mut found = Vec::new();
        discover(root, root, 0, &listed_dirs, &mut 0, &mut found);
        for (dir, info) in found {
            if entries.len() >= MAX_PACKAGES {
                problems.push(Diagnostic::warning(
                    "library.too_many",
                    format!(
                        "more than {MAX_PACKAGES} add-ons are installed; the rest are not shown"
                    ),
                ));
                break;
            }
            let Some(info) = info else {
                entries.push(LibraryEntry {
                    package: PackageEntry {
                        id: dir.clone(),
                        version: "0.0.0".into(),
                        side: Side::Shared,
                        dir: dir.clone(),
                        role: None,
                    },
                    enabled: false,
                    required: false,
                    discovered: true,
                    info: None,
                    problems: vec![
                        Diagnostic::error("library.manifest", "package.json could not be read")
                            .at(format!("{dir}/{MANIFEST_FILE}")),
                    ],
                });
                continue;
            };
            if !ids.insert(info.id.clone()) {
                problems.push(
                    Diagnostic::warning(
                        "library.duplicate",
                        format!("`{}` is installed twice; {dir} is ignored", info.id),
                    )
                    .at(format!("{dir}/{MANIFEST_FILE}"))
                    .hint("delete one of the copies"),
                );
                continue;
            }
            let side = info.side();
            let package = PackageEntry {
                id: info.id.clone(),
                version: info.version.clone(),
                side: side.unwrap_or(Side::Shared),
                dir: dir.clone(),
                role: None,
            };
            let mut found = entry(root, package, false, true);
            if side.is_none() {
                found.problems.push(
                    Diagnostic::error(
                        "library.mixed_sides",
                        format!("`{}` has both server behaviour and client visuals or code", info.id),
                    )
                    .at(format!("{dir}/{MANIFEST_FILE}"))
                    .hint("split it into two Add-Ons: one for the server rules, one for models, HUD panels and client code, the second depending on the first"),
                );
            }
            entries.push(found);
        }
        let legacy = legacy(root, &entries);
        let mut library = Self {
            root: root.to_path_buf(),
            entries,
            legacy,
            problems,
        };
        library.check_dependencies();
        // Enabled in load order first, then everything else by name.
        let mut enabled_order: Vec<LibraryEntry> = Vec::new();
        let mut rest = Vec::new();
        for e in library.entries.drain(..) {
            if e.enabled {
                enabled_order.push(e);
            } else {
                rest.push(e);
            }
        }
        rest.sort_by_key(|e| e.name().to_ascii_lowercase());
        enabled_order.extend(rest);
        library.entries = enabled_order;
        Ok(library)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A fresh directory under [`IMPORT_DIR`] for importing `name`: its
    /// lower-case, `_`-joined name, numbered if taken. Content-root relative.
    pub fn import_dir(&self, name: &str) -> String {
        let stem = folder_stem(name);
        let mut dir = format!("{IMPORT_DIR}/{stem}");
        let mut n = 2;
        // A port's host rules go beside the import, in `<dir>-rules`.
        while self.root.join(&dir).exists() || self.root.join(format!("{dir}-rules")).exists() {
            dir = format!("{IMPORT_DIR}/{stem}-{n}");
            n += 1;
        }
        dir
    }

    /// Where the importer writes a new conversion of `name`
    /// ([`Self::install_staged`] moves it in): under [`STAGING_DIR`], with
    /// its host rules beside it in `<dir>-rules`. Content-root relative;
    /// whatever an earlier, interrupted conversion left there is deleted.
    pub fn staging_dir(&self, name: &str) -> Result<String> {
        let dir = format!("{STAGING_DIR}/{}", folder_stem(name));
        for d in [dir.clone(), format!("{dir}-rules")] {
            let path = self.root.join(d);
            if path.exists() {
                std::fs::remove_dir_all(&path)
                    .with_context(|| format!("Removing {}", path.display()))?;
            }
        }
        std::fs::create_dir_all(self.root.join(STAGING_DIR))
            .with_context(|| format!("Creating {}", self.root.join(STAGING_DIR).display()))?;
        Ok(dir)
    }

    /// Whether `id` ships with the game: a bundled original
    /// ([`PackageInfo::bundled`]) or the host rules that are part of one.
    /// Only the player's own conversions are the Add-Ons folder's to
    /// replace or remove.
    pub fn shipped(&self, id: &str) -> bool {
        let bundled = |id: &str| {
            self.get(id)
                .and_then(|e| e.info.as_ref())
                .is_some_and(PackageInfo::bundled)
        };
        bundled(id) || self.companion_of(id).is_some_and(bundled)
    }

    /// The folder of the player's conversion `id`, which this game made
    /// under [`IMPORT_DIR`] and so may replace or delete.
    fn conversion_dir(&self, id: &str) -> Result<String> {
        let entry = self
            .get(id)
            .with_context(|| format!("`{id}` is not installed"))?;
        ensure!(
            !self.shipped(id),
            "{} comes with the game, so it is not removed",
            entry.name()
        );
        let dir = entry.package.dir.clone();
        ensure!(
            dir.starts_with(&format!("{IMPORT_DIR}/"))
                && !dir
                    .split('/')
                    .any(|p| p.is_empty() || p == ".." || p == "."),
            "`{id}` was not converted by this game, so it is not removed"
        );
        Ok(dir)
    }

    /// The installed Add-On that names `id` its companion (an import whose
    /// host rules `id` is): `id` is part of it, on and off with it.
    pub fn companion_of(&self, id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| {
                e.info
                    .as_ref()
                    .is_some_and(|i| i.companions.iter().any(|c| c == id))
            })
            .map(|e| e.id())
    }

    pub fn get(&self, id: &str) -> Option<&LibraryEntry> {
        self.entries.iter().find(|e| e.package.id == id)
    }

    /// Enabled packages that depend on `id`, directly or through others.
    pub fn dependents(&self, id: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut frontier = vec![id.to_string()];
        while let Some(target) = frontier.pop() {
            for e in self.entries.iter().filter(|e| e.enabled) {
                if e.dependencies().any(|(dep, _)| *dep == target)
                    && e.package.id != id
                    && !out.contains(&e.package.id)
                {
                    out.push(e.package.id.clone());
                    frontier.push(e.package.id.clone());
                }
            }
        }
        out
    }

    /// Work out what turning `id` on or off involves, without changing
    /// anything.
    pub fn plan(&self, id: &str, enable: bool) -> Plan {
        let mut plan = Plan {
            id: id.to_string(),
            enable,
            also: vec![],
            refused: vec![],
        };
        let Some(target) = self.get(id) else {
            plan.refused.push(Diagnostic::error(
                "library.unknown",
                format!("`{id}` is not installed"),
            ));
            return plan;
        };
        // A companion (an import's host rules) is part of the Add-On naming
        // it: it turns on and off with it, never by itself.
        if let Some(owner) = self.companion_of(id) {
            plan.refused.push(
                Diagnostic::error(
                    "library.companion",
                    format!(
                        "{} is part of {} and turns on and off with it",
                        target.name(),
                        self.get(owner).map_or(owner, |e| e.name())
                    ),
                )
                .at(id.to_string()),
            );
            return plan;
        }
        if target.enabled == enable {
            return plan;
        }
        if !enable {
            if target.required {
                plan.refused.push(
                    Diagnostic::error(
                        "library.required",
                        format!("{} is part of the base game and stays on", target.name()),
                    )
                    .at(id.to_string()),
                );
            } else {
                plan.also = self.dependents(id);
            }
            return plan;
        }
        // Enabling: the package and every dependency it pulls in, deepest
        // first, must each be loadable.
        let mut order = Vec::new();
        let mut visiting = BTreeSet::new();
        self.collect(id, &mut order, &mut visiting, &mut plan.refused);
        // Then the companions of everything turning on, after it. One the
        // player deleted is skipped: the Add-On still works without it.
        let mut i = 0;
        while i < order.len() {
            let companions: Vec<String> = self
                .get(&order[i])
                .and_then(|e| e.info.as_ref())
                .map(|info| info.companions.clone())
                .unwrap_or_default();
            for companion in companions {
                if self.get(&companion).is_some() {
                    self.collect(&companion, &mut order, &mut visiting, &mut plan.refused);
                }
            }
            i += 1;
        }
        let turning_on: Vec<&LibraryEntry> = order
            .iter()
            .filter_map(|i| self.get(i))
            .filter(|e| !e.enabled)
            .collect();
        for e in &turning_on {
            if e.has_errors() {
                for d in e
                    .problems
                    .iter()
                    .filter(|d| d.severity == Severity::Error && d.code != "library.dependency")
                {
                    plan.refused.push(d.clone());
                }
            }
            if let Some(role) = &e.package.role
                && let Some(holder) = self
                    .entries
                    .iter()
                    .find(|o| o.enabled && o.package.role.as_deref() == Some(role))
            {
                plan.refused.push(
                    Diagnostic::error(
                        "library.role_conflict",
                        format!(
                            "{} and {} both fill the `{role}` role; turn {} off first",
                            e.name(),
                            holder.name(),
                            holder.name()
                        ),
                    )
                    .at(e.package.id.clone()),
                );
            }
        }
        plan.also = order.into_iter().filter(|i| i != id).collect();
        plan.also
            .retain(|i| self.get(i).is_some_and(|e| !e.enabled));
        plan
    }

    /// Depth-first dependency walk for enabling: pushes `id` after its
    /// dependencies. Missing or incompatible dependencies are refusals.
    fn collect(
        &self,
        id: &str,
        order: &mut Vec<String>,
        visiting: &mut BTreeSet<String>,
        refused: &mut Vec<Diagnostic>,
    ) {
        if order.iter().any(|o| o == id) || !visiting.insert(id.to_string()) {
            return;
        }
        let Some(e) = self.get(id) else {
            return;
        };
        for (dep, requirement) in e.dependencies() {
            match self.get(dep) {
                None => refused.push(
                    Diagnostic::error(
                        "library.dependency_missing",
                        format!(
                            "{} needs `{dep}` {requirement}, which is not installed",
                            e.name()
                        ),
                    )
                    .at(e.package.id.clone())
                    .hint(format!("install `{dep}` first")),
                ),
                Some(d) => {
                    let fits = match (
                        Requirement::parse(requirement),
                        Version::parse(&d.package.version),
                    ) {
                        (Ok(r), Ok(v)) => r.matches(v),
                        _ => false,
                    };
                    if !fits {
                        refused.push(
                            Diagnostic::error(
                                "library.dependency_version",
                                format!(
                                    "{} needs {} {requirement}; {} is installed",
                                    e.name(),
                                    d.name(),
                                    d.package.version
                                ),
                            )
                            .at(e.package.id.clone()),
                        );
                    } else {
                        self.collect(dep, order, visiting, refused);
                    }
                }
            }
        }
        order.push(id.to_string());
    }

    /// Apply an allowed plan: rewrite `packages.json` and
    /// `packages-disabled.json`, then rescan.
    pub fn apply(&mut self, plan: &Plan) -> Result<()> {
        if !plan.allowed() {
            bail!(
                "{}",
                plan.refused
                    .iter()
                    .map(|d| d.message.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        let changing: BTreeSet<&str> = plan
            .also
            .iter()
            .map(String::as_str)
            .chain([plan.id.as_str()])
            .collect();
        let mut on: Vec<PackageEntry> = self
            .entries
            .iter()
            .filter(|e| e.enabled && (plan.enable || !changing.contains(e.id())))
            .map(|e| e.package.clone())
            .collect();
        if plan.enable {
            // After everything already on, each package after the ones it
            // depends on: dependencies, the package, then its companions.
            let mut waiting: Vec<&LibraryEntry> = plan
                .also
                .iter()
                .chain([&plan.id])
                .filter_map(|id| self.get(id))
                .filter(|e| !e.enabled)
                .collect();
            while !waiting.is_empty() {
                let ready = waiting
                    .iter()
                    .position(|e| {
                        !e.dependencies()
                            .any(|(dep, _)| waiting.iter().any(|w| w.id() == dep))
                    })
                    // A cycle: keep the plan's order.
                    .unwrap_or(0);
                on.push(waiting.remove(ready).package.clone());
            }
        }
        let on_ids: BTreeSet<&str> = on.iter().map(|p| p.id.as_str()).collect();
        let off: Vec<PackageEntry> = self
            .entries
            .iter()
            .filter(|e| !on_ids.contains(e.id()))
            // Discovered packages stay unlisted until someone turns them on.
            .filter(|e| !e.discovered)
            .map(|e| e.package.clone())
            .collect();
        self.write_lists(on, off)
    }

    /// Delete the player's converted Add-On `id`, which must live under
    /// [`IMPORT_DIR`] and not ship with the game ([`Self::shipped`]): turned
    /// off first, with its port's companion host rules beside it
    /// (`<dir>-rules`) and whatever needs either, then both folders go. The
    /// lists drop it at their next change. What else was turned off.
    pub fn uninstall(&mut self, id: &str) -> Result<Vec<String>> {
        let dir = self.conversion_dir(id)?;
        let also = self.turn_off_with_rules(id, &dir)?;
        for dir in [format!("{dir}-rules"), dir] {
            let path = self.root.join(&dir);
            if path.is_dir() {
                std::fs::remove_dir_all(&path)
                    .with_context(|| format!("Removing {}", path.display()))?;
            }
        }
        *self = Self::scan(&self.root)?;
        Ok(also)
    }

    /// Turn the conversion `id` in `dir` off, with its host rules
    /// (`<id>-rules` in `<dir>-rules`) and whatever needs either. The rules
    /// are part of it ([`Self::companion_of`]), so they go with it here
    /// rather than being refused on their own. What else went off, the
    /// rules aside.
    fn turn_off_with_rules(&mut self, id: &str, dir: &str) -> Result<Vec<String>> {
        let rules = format!("{id}-rules");
        let rules_on = self
            .get(&rules)
            .is_some_and(|e| e.enabled && e.package.dir == format!("{dir}-rules"));
        let mut plan = if self
            .get(id)
            .is_some_and(|e| e.enabled && e.package.dir == dir)
        {
            self.plan(id, false)
        } else if rules_on {
            Plan {
                id: id.to_string(),
                enable: false,
                also: vec![],
                refused: vec![],
            }
        } else {
            return Ok(vec![]);
        };
        if rules_on {
            for other in std::iter::once(rules.clone()).chain(self.dependents(&rules)) {
                if other != id && !plan.also.contains(&other) {
                    plan.also.push(other);
                }
            }
        }
        self.apply(&plan)?;
        plan.also.retain(|a| *a != rules);
        Ok(plan.also)
    }

    /// Move the conversion of the dropped Add-On `name` that the importer
    /// wrote to `staged` ([`Self::staging_dir`], its host rules at
    /// `<staged>-rules`) into [`IMPORT_DIR`]. Its id and folder.
    ///
    /// With `replaces`, the player's earlier conversion of it, the new copy
    /// takes the old one's folder and its place in the lists: whether it is
    /// on does not change, so neither do the Add-Ons that need it. The old
    /// copy is deleted only once the new one is in; when anything fails it
    /// is left as it was. The staged copy is gone either way. A new copy
    /// that became a different Add-On (another id) replaces the old one as
    /// a removal does, then is turned on if the old one was.
    pub fn install_staged(
        &mut self,
        name: &str,
        staged: &str,
        replaces: Option<&str>,
    ) -> Result<(String, String)> {
        ensure!(
            staged.starts_with(&format!("{STAGING_DIR}/")) && !staged.contains(".."),
            "`{staged}` is not a staged conversion"
        );
        let result = self.install_staged_inner(name, staged, replaces);
        for dir in [staged.to_string(), format!("{staged}-rules")] {
            let path = self.root.join(dir);
            if path.exists() {
                let _ = std::fs::remove_dir_all(path);
            }
        }
        *self = Self::scan(&self.root)?;
        result
    }

    fn install_staged_inner(
        &mut self,
        name: &str,
        staged: &str,
        replaces: Option<&str>,
    ) -> Result<(String, String)> {
        let id = read_info(&self.root.join(staged).join(MANIFEST_FILE))
            .context("the conversion has no readable package.json")?
            .id;
        let old = replaces.filter(|old| self.get(old).is_some());
        if let Some(old) = old {
            let dir = self.conversion_dir(old)?;
            if old == id {
                self.swap_in(staged, &dir)?;
                self.relist(&dir)?;
                return Ok((id, dir));
            }
        }
        let was_on = old.is_some_and(|old| self.get(old).is_some_and(|e| e.enabled));
        if let Some(old) = old {
            self.uninstall(old)?;
        }
        let dir = self.import_dir(name);
        let moves = [
            (staged.to_string(), dir.clone()),
            (format!("{staged}-rules"), format!("{dir}-rules")),
        ];
        self.move_all(&moves)?;
        *self = Self::scan(&self.root)?;
        if was_on {
            let plan = self.plan(&id, true);
            if plan.allowed() {
                self.apply(&plan)?;
            }
        }
        Ok((id, dir))
    }

    /// Put the staged conversion and its host rules in place of the copy in
    /// `dir` and its rules: the old ones are moved aside, the new ones in,
    /// and only then is the old copy deleted. On failure every move is
    /// undone.
    fn swap_in(&self, staged: &str, dir: &str) -> Result<()> {
        let aside = [
            (dir.to_string(), format!("{staged}.old")),
            (format!("{dir}-rules"), format!("{staged}.old-rules")),
        ];
        for (_, to) in &aside {
            let path = self.root.join(to);
            if path.exists() {
                std::fs::remove_dir_all(&path)
                    .with_context(|| format!("Removing {}", path.display()))?;
            }
        }
        let moved_aside = self.move_all(&aside)?;
        let moves = [
            (staged.to_string(), dir.to_string()),
            (format!("{staged}-rules"), format!("{dir}-rules")),
        ];
        if let Err(error) = self.move_all(&moves) {
            self.undo(&moved_aside);
            return Err(error);
        }
        for (_, to) in &moved_aside {
            let path = self.root.join(to);
            std::fs::remove_dir_all(&path)
                .with_context(|| format!("Removing {}", path.display()))?;
        }
        Ok(())
    }

    /// Rename each existing `from` to `to`, all or none: what was moved.
    fn move_all(&self, moves: &[(String, String)]) -> Result<Vec<(String, String)>> {
        let mut done = vec![];
        for (from, to) in moves {
            let (source, target) = (self.root.join(from), self.root.join(to));
            if !source.exists() {
                continue;
            }
            let moved = match target.parent() {
                Some(parent) => std::fs::create_dir_all(parent),
                None => Ok(()),
            }
            .and_then(|()| std::fs::rename(&source, &target));
            if let Err(error) = moved {
                self.undo(&done);
                return Err(error).with_context(|| {
                    format!("Moving {} to {}", source.display(), target.display())
                });
            }
            done.push((from.clone(), to.clone()));
        }
        Ok(done)
    }

    fn undo(&self, moved: &[(String, String)]) {
        for (from, to) in moved.iter().rev() {
            if let Err(error) = std::fs::rename(self.root.join(to), self.root.join(from)) {
                // Nothing more can be done here; the copy is still at `to`.
                eprintln!("Could not move {to} back to {from}: {error}");
            }
        }
    }

    /// After the copy in `dir` was replaced: the lists' entries for it and
    /// its host rules follow the new copy's manifest (id, version and side),
    /// and one whose folder is gone goes. Which are on does not change; host
    /// rules only the new copy has turn on with it ([`follow_companions`]).
    fn relist(&mut self, dir: &str) -> Result<()> {
        *self = Self::scan(&self.root)?;
        let rules = format!("{dir}-rules");
        let root = self.root.clone();
        let refresh = |e: &LibraryEntry| -> Option<PackageEntry> {
            let mut p = e.package.clone();
            if p.dir != dir && p.dir != rules {
                return Some(p);
            }
            let info = read_info(&root.join(&p.dir).join(MANIFEST_FILE))?;
            if p.role.is_none()
                && let Some(side) = info.side()
            {
                p.side = side;
            }
            p.id = info.id;
            p.version = info.version;
            Some(p)
        };
        let on: Vec<PackageEntry> = self
            .entries
            .iter()
            .filter(|e| e.enabled)
            .filter_map(refresh)
            .collect();
        let off: Vec<PackageEntry> = self
            .entries
            .iter()
            .filter(|e| !e.enabled && !e.discovered)
            .filter_map(refresh)
            .collect();
        self.write_lists(on, off)
    }

    /// Write `packages.json` (`on`) and `packages-disabled.json` (`off`),
    /// then rescan.
    fn write_lists(&mut self, on: Vec<PackageEntry>, off: Vec<PackageEntry>) -> Result<()> {
        let enabled = PackageSet {
            schema_version: PACKAGES_SCHEMA,
            packages: on,
        };
        let disabled = PackageSet {
            schema_version: PACKAGES_SCHEMA,
            packages: off,
        };
        enabled.validate().into_result()?;
        disabled.validate().into_result()?;
        // The disabled list first: if the second write fails, a package is at
        // worst listed twice (and treated as enabled), never lost.
        write_atomic(&self.root.join(DISABLED_FILE), &disabled)?;
        write_atomic(&self.root.join(PACKAGES_FILE), &enabled)?;
        *self = Self::scan(&self.root)?;
        Ok(())
    }

    /// Mark enabled packages whose dependencies are not enabled and
    /// satisfied. They would not load, so the library says so up front.
    fn check_dependencies(&mut self) {
        let versions: BTreeMap<String, (bool, String)> = self
            .entries
            .iter()
            .map(|e| (e.package.id.clone(), (e.enabled, e.package.version.clone())))
            .collect();
        for e in &mut self.entries {
            let needs: Vec<(String, String)> = e
                .dependencies()
                .map(|(a, b)| (a.clone(), b.clone()))
                .collect();
            for (dep, requirement) in needs {
                let problem = match versions.get(&dep) {
                    None => Some(format!(
                        "needs `{dep}` {requirement}, which is not installed"
                    )),
                    Some((enabled, version)) => {
                        let fits = match (Requirement::parse(&requirement), Version::parse(version))
                        {
                            (Ok(r), Ok(v)) => r.matches(v),
                            _ => false,
                        };
                        if !fits {
                            Some(format!(
                                "needs `{dep}` {requirement}; {version} is installed"
                            ))
                        } else if e.enabled && !enabled {
                            Some(format!("needs `{dep}`, which is turned off"))
                        } else {
                            None
                        }
                    }
                };
                if let Some(message) = problem {
                    let severity_error = e.enabled;
                    let d = if severity_error {
                        Diagnostic::error("library.dependency", message)
                    } else {
                        Diagnostic::warning("library.dependency", message)
                    };
                    e.problems.push(d.at(e.package.id.clone()));
                }
            }
            e.problems.sort_by_key(|d| d.severity);
        }
    }
}

/// A dropped Add-On's folder name: lower-case, `_`-joined.
fn folder_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let stem: String = stem.trim_matches('_').chars().take(64).collect();
    if stem.is_empty() {
        "addon".into()
    } else {
        stem
    }
}

/// Build an entry for a listed or discovered package and read its manifest.
fn entry(root: &Path, package: PackageEntry, enabled: bool, discovered: bool) -> LibraryEntry {
    let required = id::is_reserved(&package.id);
    let mut problems = Vec::new();
    let dir = root.join(&package.dir);
    let mut info = None;
    if !dir.is_dir() {
        problems.push(
            Diagnostic::error(
                "library.missing_dir",
                format!("its folder `{}` is missing", package.dir),
            )
            .at(package.id.clone())
            .hint(if required {
                "rerun tools/bootstrap.py to rebuild the base game's content"
            } else {
                "reinstall it, or turn it off"
            }),
        );
    } else {
        let manifest = dir.join(MANIFEST_FILE);
        if manifest.is_file() {
            match read_info(&manifest) {
                Some(i) => {
                    if i.id != package.id {
                        problems.push(
                            Diagnostic::error(
                                "library.manifest_id",
                                format!("its package.json says it is `{}`", i.id),
                            )
                            .at(format!("{}/{MANIFEST_FILE}", package.dir)),
                        );
                    }
                    if i.version != package.version {
                        problems.push(
                            Diagnostic::warning(
                                "library.version",
                                format!(
                                    "the list says version {}, its package.json says {}",
                                    package.version, i.version
                                ),
                            )
                            .at(format!("{}/{MANIFEST_FILE}", package.dir)),
                        );
                    }
                    if let Some(api) = i.api
                        && api > API_LEVEL
                    {
                        problems.push(
                            Diagnostic::error(
                                "library.api",
                                format!("it needs a newer game (platform API {api}; this build has {API_LEVEL})"),
                            )
                            .at(format!("{}/{MANIFEST_FILE}", package.dir)),
                        );
                    }
                    info = Some(i);
                }
                None => problems.push(
                    Diagnostic::error("library.manifest", "its package.json could not be read")
                        .at(format!("{}/{MANIFEST_FILE}", package.dir)),
                ),
            }
        }
    }
    LibraryEntry {
        package,
        enabled: enabled || required,
        required,
        discovered,
        info,
        problems,
    }
}

/// The manifest of the package in `dir`, when it reads as one.
pub fn package_info(dir: &Path) -> Option<PackageInfo> {
    read_info(&dir.join(MANIFEST_FILE))
}

pub(crate) fn read_info(path: &Path) -> Option<PackageInfo> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    let info: PackageInfo = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (id::namespace_problem(&info.id).is_none() && Version::parse(&info.version).is_ok())
        .then_some(info)
}

/// Find directories holding a manifest that no list names. A directory with
/// a manifest is a package; discovery does not look inside it.
fn discover(
    root: &Path,
    dir: &Path,
    depth: usize,
    listed: &BTreeSet<String>,
    visited: &mut usize,
    out: &mut Vec<(String, Option<PackageInfo>)>,
) {
    if depth >= MAX_SCAN_DEPTH {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = read
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    children.sort();
    for child in children {
        *visited += 1;
        if *visited > MAX_SCAN_DIRS {
            return;
        }
        let Some(name) = child.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || (depth == 0 && name.eq_ignore_ascii_case(DROP_DIR)) {
            continue;
        }
        let Ok(rel) = child.strip_prefix(root) else {
            continue;
        };
        let rel: String = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if listed.contains(&rel.to_ascii_lowercase()) {
            continue;
        }
        let manifest = child.join(MANIFEST_FILE);
        if manifest.is_file() {
            out.push((rel, read_info(&manifest)));
        } else {
            discover(root, &child, depth + 1, listed, visited, out);
        }
    }
}

/// Zips and folders in [`DROP_DIR`], each matched to the package imported
/// from it, if any.
fn legacy(root: &Path, entries: &[LibraryEntry]) -> Vec<LegacyAddOn> {
    let Ok(read) = std::fs::read_dir(root.join(DROP_DIR)) else {
        return vec![];
    };
    let mut out: Vec<LegacyAddOn> = read
        .flatten()
        .filter_map(|e| {
            let kind = e.file_type().ok()?;
            let path = e.path();
            let zip = kind.is_file()
                && path
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("zip"));
            if !(zip || kind.is_dir()) {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            let prefix = format!("Blockland Add-On {name} (");
            let made_from = |bundled: bool| {
                entries
                    .iter()
                    .find(|p| {
                        p.info.as_ref().is_some_and(|i| {
                            i.bundled() == bundled
                                && i.source().is_some_and(|s| s.starts_with(&prefix))
                        })
                    })
                    .map(|p| p.package.id.clone())
            };
            Some(LegacyAddOn {
                imported_as: made_from(false),
                included: made_from(true),
                name,
                path,
            })
        })
        .take(MAX_LEGACY)
        .collect();
    out.sort_by_key(|l| l.name.to_ascii_lowercase());
    out
}

/// One entry per line, like the base list, written to a temporary file and
/// renamed over the target so a crash never leaves half a list.
pub(crate) fn write_atomic(path: &Path, set: &PackageSet) -> Result<()> {
    let mut text = format!(
        "{{\n  \"schema_version\": {},\n  \"packages\": [",
        set.schema_version
    );
    for (i, p) in set.packages.iter().enumerate() {
        text.push_str(if i == 0 { "\n    " } else { ",\n    " });
        text.push_str(&serde_json::to_string(p)?);
    }
    text.push_str(if set.packages.is_empty() {
        "]\n}\n"
    } else {
        "\n  ]\n}\n"
    });
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, text).with_context(|| format!("Writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("Replacing {}", path.display()))?;
    Ok(())
}

/// What players call the Add-On whose content directory is `dir` (its
/// folder, or `<folder>/assets`): the `name` in its manifest, else its id,
/// else `fallback` (the content-root-relative directory). A downloaded
/// Add-On lives under a hash in the download cache, so messages name it
/// this way rather than by folder.
pub fn add_on_label(dir: &Path, fallback: &str) -> String {
    let folder = if dir.ends_with("assets") {
        dir.parent().unwrap_or(dir)
    } else {
        dir
    };
    let manifest = std::fs::File::open(folder.join(MANIFEST_FILE))
        .ok()
        .and_then(|file| {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(
                &mut std::io::Read::take(file, MAX_MANIFEST_BYTES),
                &mut bytes,
            )
            .ok()?;
            serde_json::from_slice::<serde_json::Value>(&bytes).ok()
        });
    let field = |key: &str| {
        manifest
            .as_ref()
            .and_then(|m| m.get(key)?.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
            .map(str::to_owned)
    };
    field("name").or_else(|| field("id")).unwrap_or_else(|| {
        fallback
            .strip_suffix("/assets")
            .unwrap_or(fallback)
            .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Root(PathBuf);
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn root(name: &str) -> Root {
        let dir = std::env::temp_dir().join(format!("bri-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Root(dir)
    }
    fn manifest(root: &Path, dir: &str, id: &str, deps: serde_json::Value, kinds: &[&str]) {
        let path = root.join(dir);
        std::fs::create_dir_all(&path).unwrap();
        let provides: Vec<_> = kinds
            .iter()
            .enumerate()
            .map(|(i, k)| json!({ "kind": k, "id": format!("{id}:{k}/x{i}"), "file": "x.json" }))
            .collect();
        let m = json!({
            "schema_version": 1, "id": id, "version": "1.0.0", "api": 1,
            "name": format!("The {id}"), "license": "CC0-1.0",
            "provenance": { "source": "original" },
            "dependencies": deps, "capabilities": ["chat"], "provides": provides,
        });
        std::fs::write(path.join(MANIFEST_FILE), m.to_string()).unwrap();
    }
    fn list(root: &Path, packages: serde_json::Value) {
        std::fs::write(
            root.join(PACKAGES_FILE),
            json!({ "schema_version": 1, "packages": packages }).to_string(),
        )
        .unwrap();
    }
    fn ids(set: &PackageSet) -> Vec<&str> {
        set.packages.iter().map(|p| p.id.as_str()).collect()
    }

    /// Four imports with host rules beside them (made-up Add-Ons), as a
    /// release's Import or bundle leaves them: `addons/<x>` naming its
    /// companion, `addons/<x>-rules` depending on it.
    fn imports_with_rules(root: &Path, names: &[&str]) {
        for name in names {
            manifest(
                root,
                &format!("addons/{name}"),
                name,
                json!({}),
                &["weapons"],
            );
            let path = root.join("addons").join(name).join(MANIFEST_FILE);
            let mut m: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            m["companions"] = json!([format!("{name}-rules")]);
            std::fs::write(&path, m.to_string()).unwrap();
            manifest(
                root,
                &format!("addons/{name}-rules"),
                &format!("{name}-rules"),
                json!({ *name: "=1.0.0" }),
                &["behaviour"],
            );
        }
    }

    /// A list written without the host rules (a release that lacked them,
    /// Add-Ons turned on before their rules were installed) loads every on
    /// Add-On's rules right after it, whichever position it holds; one off
    /// keeps its rules off; a list turning rules on without their Add-On
    /// loses them; and turning one off in the game takes its rules with it.
    #[test]
    fn every_on_add_on_loads_its_companions_whatever_wrote_the_list() {
        let r = root("companions");
        imports_with_rules(
            &r.0,
            &["tool_alpha", "tool_beta", "tool_gamma", "tool_delta"],
        );
        let entry = |id: &str, side: &str| json!({ "id": id, "version": "1.0.0", "side": side, "dir": format!("addons/{id}") });
        // On: alpha, beta and gamma (gamma's rules listed before it, delta's
        // rules listed on without it); off: delta, and beta's rules.
        list(
            &r.0,
            json!([
                entry("tool_alpha", "shared"),
                entry("tool_gamma-rules", "server"),
                entry("tool_beta", "shared"),
                entry("tool_gamma", "shared"),
                entry("tool_delta-rules", "server"),
            ]),
        );
        std::fs::write(
            r.0.join(DISABLED_FILE),
            json!({ "schema_version": 1, "packages": [
                entry("tool_delta", "shared"), entry("tool_beta-rules", "server")
            ] })
            .to_string(),
        )
        .unwrap();
        let set = PackageSet::load_root(&r.0).unwrap();
        assert_eq!(
            ids(&set),
            [
                "tool_alpha",
                "tool_alpha-rules",
                "tool_beta",
                "tool_beta-rules",
                "tool_gamma",
                "tool_gamma-rules",
            ]
        );
        assert!(set.validate().is_empty());
        let rules = set
            .packages
            .iter()
            .find(|p| p.id == "tool_beta-rules")
            .unwrap();
        assert_eq!(rules.dir, "addons/tool_beta-rules");
        assert_eq!(rules.side, Side::Server);

        // The Add-Ons screen agrees, with nothing listed twice, and turning
        // beta off turns its rules off with it.
        let mut library = Library::scan(&r.0).unwrap();
        assert!(library.problems.is_empty(), "{:?}", library.problems);
        for id in ["tool_alpha-rules", "tool_beta-rules", "tool_gamma-rules"] {
            assert!(library.get(id).unwrap().enabled, "{id}");
        }
        for id in ["tool_delta", "tool_delta-rules"] {
            assert!(!library.get(id).unwrap().enabled, "{id}");
        }
        let plan = library.plan("tool_beta", false);
        assert_eq!(plan.also, ["tool_beta-rules"]);
        library.apply(&plan).unwrap();
        let set = PackageSet::load_root(&r.0).unwrap();
        assert_eq!(
            ids(&set),
            [
                "tool_alpha",
                "tool_alpha-rules",
                "tool_gamma",
                "tool_gamma-rules"
            ]
        );
        // And on again, with them.
        let mut library = Library::scan(&r.0).unwrap();
        let plan = library.plan("tool_delta", true);
        assert_eq!(plan.also, ["tool_delta-rules"]);
        library.apply(&plan).unwrap();
        let set = PackageSet::load_root(&r.0).unwrap();
        assert_eq!(&ids(&set)[4..], ["tool_delta", "tool_delta-rules"]);
    }

    /// A base package, three mods in a chain (hud → economy → world) and one
    /// with a missing dependency.
    fn fixture(name: &str) -> Root {
        let r = root(name);
        std::fs::create_dir_all(r.0.join("ui-pack-004")).unwrap();
        list(
            &r.0,
            json!([{ "id": "v20-ui", "version": "3.0.0", "side": "client", "dir": "ui-pack-004", "role": "ui_pack" }]),
        );
        manifest(
            &r.0,
            "packages/lab/world",
            "lab-world",
            json!({}),
            &["world", "script"],
        );
        manifest(
            &r.0,
            "packages/lab/economy",
            "lab-economy",
            json!({ "lab-world": "^1.0.0" }),
            &["behaviour"],
        );
        manifest(
            &r.0,
            "packages/lab/hud",
            "lab-hud",
            json!({ "lab-economy": "^1.0" }),
            &["hud"],
        );
        manifest(
            &r.0,
            "orphan",
            "orphan",
            json!({ "nowhere": "*" }),
            &["weapons"],
        );
        r
    }

    #[test]
    fn discovers_unlisted_packages_disabled_with_a_derived_side() {
        let r = fixture("discover");
        let lib = Library::scan(&r.0).unwrap();
        let base = lib.get("v20-ui").unwrap();
        assert!(base.enabled && base.required && base.problems.is_empty());
        let world = lib.get("lab-world").unwrap();
        assert!(!world.enabled && world.discovered);
        assert_eq!(world.package.dir, "packages/lab/world");
        assert_eq!(world.package.side, Side::Server);
        assert_eq!(lib.get("lab-hud").unwrap().package.side, Side::Client);
        assert_eq!(lib.get("orphan").unwrap().package.side, Side::Shared);
        assert_eq!(world.info.as_ref().unwrap().source(), Some("original"));
        // Disabled packages only warn about dependencies.
        let orphan = lib.get("orphan").unwrap();
        assert!(!orphan.has_errors());
        assert_eq!(orphan.problems[0].code, "library.dependency");
    }

    #[test]
    fn a_package_mixing_server_rules_and_client_visuals_is_refused() {
        let r = root("mixed");
        list(&r.0, json!([]));
        manifest(&r.0, "creeper", "creeper", json!({}), &["entity", "model"]);
        manifest(&r.0, "mob", "mob", json!({}), &["entity", "behaviour"]);
        let lib = Library::scan(&r.0).unwrap();
        let creeper = lib.get("creeper").unwrap();
        assert!(creeper.has_errors());
        assert_eq!(creeper.problems[0].code, "library.mixed_sides");
        assert_eq!(lib.get("mob").unwrap().package.side, Side::Server);
        assert!(!lib.plan("creeper", true).allowed());
    }

    #[test]
    fn the_host_decides_on_client_code_unless_it_is_personal() {
        let host = CodeOwner::of(Some(&json!({ "module": "client/main.wasm" })));
        let player = CodeOwner::of(Some(
            &json!({ "module": "client/main.wasm", "personal": true }),
        ));
        assert_eq!(host, CodeOwner::Host);
        assert_eq!(player, CodeOwner::Player);
        assert_eq!(CodeOwner::of(None), CodeOwner::None);
        // Code alone (a ragdoll): shared, so joiners get the host's.
        assert_eq!(side_for_package([], host), Some(Side::Shared));
        assert_eq!(side_for_package([], player), Some(Side::Client));
        // With models: the host's too, unless personal.
        assert_eq!(side_for_package(["model"], host), Some(Side::Shared));
        assert_eq!(side_for_package(["model"], player), Some(Side::Client));
        assert_eq!(
            side_for_package(["model"], CodeOwner::None),
            Some(Side::Client)
        );
        assert_eq!(side_for_package(["weapons"], player), Some(Side::Shared));
        // Clients never load a server package, so code cannot ride on one.
        assert_eq!(side_for_package(["behaviour"], host), None);
        assert_eq!(side_for_package(["behaviour"], player), None);
        assert_eq!(
            side_for_package(["behaviour"], CodeOwner::None),
            Some(Side::Server)
        );
    }

    #[test]
    fn a_listed_side_follows_the_add_ons_manifest() {
        let r = root("follow-side");
        std::fs::create_dir_all(r.0.join("ragdoll")).unwrap();
        std::fs::write(
            r.0.join("ragdoll").join(MANIFEST_FILE),
            json!({
                "schema_version": 1, "id": "ragdoll", "version": "1.0.0", "api": 1,
                "name": "Ragdoll", "license": "CC0-1.0",
                "client": { "module": "client/main.wasm" },
            })
            .to_string(),
        )
        .unwrap();
        // Listed as a player's own by an older game.
        list(
            &r.0,
            json!([
                { "id": "v20-ui", "version": "3.0.0", "side": "client", "dir": "ui", "role": "ui_pack" },
                { "id": "ragdoll", "version": "1.0.0", "side": "client", "dir": "ragdoll" },
                { "id": "gone", "version": "1.0.0", "side": "client", "dir": "gone" },
            ]),
        );
        let set = PackageSet::load_root(&r.0).unwrap();
        let sides: Vec<Side> = set.packages.iter().map(|p| p.side).collect();
        // The base game's and an unreadable Add-On's stay as listed.
        assert_eq!(sides, [Side::Client, Side::Shared, Side::Client]);
    }

    #[test]
    fn enabling_pulls_in_dependencies_first_and_disabling_takes_dependents() {
        let r = fixture("deps");
        let mut lib = Library::scan(&r.0).unwrap();
        let plan = lib.plan("lab-hud", true);
        assert!(plan.allowed(), "{:?}", plan.refused);
        assert_eq!(plan.also, ["lab-world", "lab-economy"]);
        lib.apply(&plan).unwrap();
        let on = PackageSet::load(&r.0.join(PACKAGES_FILE)).unwrap();
        assert_eq!(ids(&on), ["v20-ui", "lab-world", "lab-economy", "lab-hud"]);
        assert!(lib.get("lab-hud").unwrap().enabled);
        assert!(lib.entries.iter().all(|e| !e.enabled || !e.has_errors()));

        let plan = lib.plan("lab-world", false);
        assert!(plan.allowed());
        assert_eq!(plan.also, ["lab-economy", "lab-hud"]);
        lib.apply(&plan).unwrap();
        let on = PackageSet::load(&r.0.join(PACKAGES_FILE)).unwrap();
        assert_eq!(ids(&on), ["v20-ui"]);
        // The exact entries are kept for next time.
        let off = PackageSet::load(&r.0.join(DISABLED_FILE)).unwrap();
        assert!(
            off.packages
                .iter()
                .any(|p| p.id == "lab-world" && p.side == Side::Server)
        );
        let lib = Library::scan(&r.0).unwrap();
        let world = lib.get("lab-world").unwrap();
        assert!(!world.enabled && !world.discovered);
    }

    #[test]
    fn refusals_say_why() {
        let r = fixture("refuse");
        let lib = Library::scan(&r.0).unwrap();
        let plan = lib.plan("orphan", true);
        assert_eq!(plan.refused[0].code, "library.dependency_missing");
        assert!(plan.refused[0].message.contains("`nowhere`"));
        let plan = lib.plan("v20-ui", false);
        assert_eq!(plan.refused[0].code, "library.required");
        assert!(lib.plan("ghost", true).refused[0].code == "library.unknown");
        // A version the installed dependency does not satisfy.
        manifest(
            &r.0,
            "packages/lab/hud",
            "lab-hud",
            json!({ "lab-economy": "^2.0" }),
            &["hud"],
        );
        let lib = Library::scan(&r.0).unwrap();
        assert_eq!(
            lib.plan("lab-hud", true).refused[0].code,
            "library.dependency_version"
        );
        // A refused plan changes nothing on disk.
        let mut lib = lib;
        let before = std::fs::read(r.0.join(PACKAGES_FILE)).unwrap();
        assert!(lib.apply(&lib.plan("orphan", true)).is_err());
        assert_eq!(std::fs::read(r.0.join(PACKAGES_FILE)).unwrap(), before);
    }

    #[test]
    fn broken_and_newer_packages_are_reported_not_hidden() {
        let r = fixture("broken");
        std::fs::create_dir_all(r.0.join("junk")).unwrap();
        std::fs::write(r.0.join("junk").join(MANIFEST_FILE), "{ not json").unwrap();
        manifest(&r.0, "future", "future", json!({}), &["weapons"]);
        let path = r.0.join("future").join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\"api\":1", "\"api\":99");
        std::fs::write(&path, text).unwrap();
        list(
            &r.0,
            json!([
                { "id": "v20-ui", "version": "3.0.0", "side": "client", "dir": "ui-pack-004", "role": "ui_pack" },
                { "id": "gone", "version": "1.0.0", "side": "shared", "dir": "gone" }
            ]),
        );
        let lib = Library::scan(&r.0).unwrap();
        assert_eq!(
            lib.get("junk").unwrap().problems[0].code,
            "library.manifest"
        );
        assert_eq!(
            lib.get("gone").unwrap().problems[0].code,
            "library.missing_dir"
        );
        assert!(lib.get("gone").unwrap().enabled);
        let future = lib.get("future").unwrap();
        assert_eq!(future.problems[0].code, "library.api");
        assert_eq!(lib.plan("future", true).refused[0].code, "library.api");
        // A missing package can still be turned off, and then there is
        // nothing left to show.
        let mut lib = lib;
        lib.apply(&lib.plan("gone", false)).unwrap();
        assert!(lib.get("gone").is_none());
        let on = PackageSet::load(&r.0.join(PACKAGES_FILE)).unwrap();
        assert_eq!(ids(&on), ["v20-ui"]);
    }

    #[test]
    fn dropped_add_ons_are_listed_and_matched_to_their_import() {
        let r = fixture("legacy");
        let drop = r.0.join(DROP_DIR);
        std::fs::create_dir_all(drop.join("Vehicle_Jeep")).unwrap();
        std::fs::write(drop.join("Weapon_Shotgun.zip"), b"PK").unwrap();
        std::fs::write(drop.join("readme.txt"), b"hi").unwrap();
        manifest(
            &r.0,
            "addons/weapon_shotgun",
            "weapon_shotgun",
            json!({}),
            &["weapons"],
        );
        let path = r.0.join("addons/weapon_shotgun").join(MANIFEST_FILE);
        let mut m: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        m["provenance"]["source"] = json!("Blockland Add-On Weapon_Shotgun (zip), sha256 00");
        std::fs::write(&path, m.to_string()).unwrap();
        let lib = Library::scan(&r.0).unwrap();
        let legacy: Vec<_> = lib
            .legacy
            .iter()
            .map(|l| (l.name.as_str(), l.imported_as.as_deref()))
            .collect();
        assert_eq!(
            legacy,
            [
                ("Vehicle_Jeep", None),
                ("Weapon_Shotgun", Some("weapon_shotgun"))
            ]
        );
        // A dropped v20 folder is never mistaken for a package.
        assert!(lib.get("vehicle_jeep").is_none());
        assert_eq!(lib.import_dir("Weapon_Shotgun"), "addons/weapon_shotgun-2");
        assert_eq!(lib.import_dir("Vehicle Jeep!"), "addons/vehicle_jeep");
    }

    #[test]
    fn role_conflicts_are_refused() {
        let r = fixture("roles");
        std::fs::create_dir_all(r.0.join("ui-other")).unwrap();
        std::fs::write(
            r.0.join(DISABLED_FILE),
            json!({ "schema_version": 1, "packages": [
                { "id": "fancy-ui", "version": "1.0.0", "side": "client", "dir": "ui-other", "role": "ui_pack" }
            ]})
            .to_string(),
        )
        .unwrap();
        let lib = Library::scan(&r.0).unwrap();
        assert_eq!(
            lib.plan("fancy-ui", true).refused[0].code,
            "library.role_conflict"
        );
    }

    #[test]
    fn a_disabled_entry_whose_folder_is_gone_is_hidden_and_dropped() {
        let r = fixture("gone");
        std::fs::write(
            r.0.join(DISABLED_FILE),
            json!({ "schema_version": 1, "packages": [
                { "id": "lab-gone", "version": "1.0.0", "side": "shared", "dir": "packages/lab/gone" }
            ]})
            .to_string(),
        )
        .unwrap();
        let mut lib = Library::scan(&r.0).unwrap();
        assert!(lib.get("lab-gone").is_none());
        let plan = lib.plan("lab-world", true);
        lib.apply(&plan).unwrap();
        let off = PackageSet::load(&r.0.join(DISABLED_FILE)).unwrap();
        assert!(!ids(&off).contains(&"lab-gone"), "{:?}", ids(&off));
    }
}
