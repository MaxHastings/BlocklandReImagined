//! Default Add-Ons: the Add-Ons every copy of the game has on until the
//! player turns them off (today the Duplicator, the Stunt Plane and the
//! Mirror), and those it carries turned off for players to turn on
//! (`"enabled": false`, like the Ragdoll, the Gravity Gun and the New
//! Duplicator).
//!
//! One list, `packages/default-addons.json`, names them in load order. This
//! module, the release packagers and `tools/addon_bundle.py` all read it.
//! An entry is one of two kinds:
//!
//! - **Our own** (`path`): committed under the repository's `packages/<path>`.
//! - **A bundled original** (`original`): a classic community Add-On, such
//!   as Kaje and Ephialtes' Stunt Plane. Its files never enter the
//!   repository. `tools/addon_bundle.py` finds Maxwell's copy, checks it is
//!   one the list pins (`sha256`), imports it with its port and packs it into
//!   the private bundle the release builds carry, credited to its authors.
//!   An original no copy is pinned for yet, or one `withdrawn` after its
//!   author objected, is left out everywhere ([`list`]).
//!
//! A content root holds each at `addons/<id>`, where the game keeps Add-Ons:
//!
//! - A release gets them when it is packaged; its `packages.json` lists them.
//! - A source checkout's `content/` is generated, never committed, so the
//!   game installs our own when it starts ([`install_from_checkout`];
//!   `bri-client --check` and `bri-server` only with [`INSTALL_ENV`]=1): missing copies are copied in and copies
//!   that differ from the checkout's are replaced. The originals come from
//!   the bundle (`python tools/addon_bundle.py install`, which bootstrap
//!   runs when it finds them).
//!
//! A content root without a `packages.json` loads the base game and the
//! default Add-Ons installed in it ([`PackageSet::load_root`]): the list a
//! release ships. One carried turned off is installed but never listed: the
//! Add-Ons screen finds it under `addons/` and shows it off until the
//! player turns it on. Installing never writes that file, so a checkout keeps
//! following the base list as it changes. A root with its own list keeps the
//! player's choices: a default they turned off stays off.
use crate::library::{DISABLED_FILE, IMPORT_DIR, MANIFEST_FILE, read_info, write_atomic};
use crate::packages::{PACKAGES_FILE, PACKAGES_SCHEMA, PackageEntry, PackageSet};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The list's name in the repository's `packages/` folder.
pub const LIST_FILE: &str = "default-addons.json";
/// The list's schema: 2 added bundled originals.
pub const LIST_SCHEMA: u32 = 2;
const LIST: &str = include_str!("../../../packages/default-addons.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    schema_version: u32,
    addons: Vec<DefaultAddOn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultAddOn {
    /// Package id, as its `package.json` names it.
    pub id: String,
    /// Our own: its folder under the repository's `packages/`.
    #[serde(default)]
    pub path: Option<String>,
    /// A bundled original: the classic Add-On it is imported from.
    #[serde(default)]
    pub original: Option<Original>,
    /// False for one carried turned off: installed, never turned on for
    /// the player.
    #[serde(default = "starts_on")]
    pub enabled: bool,
}

/// A classic community Add-On the releases bundle (`tools/addon_bundle.py`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Original {
    /// Its folder or zip name, which is a Blockland Add-On's identity
    /// (`Vehicle_Stunt_Plane`). The package id is the importer's namespace
    /// for it, and its port is the one whose `entry.json` names it.
    pub addon: String,
    pub title: String,
    /// Who made it, as the credits and the Add-Ons screen name them.
    pub authors: Vec<String>,
    /// The version its import gets.
    pub version: String,
    /// The copies it may be bundled from: `source.sha256` of their import
    /// report (the zip's hash, or a folder's member hashes). Empty until
    /// one is pinned; until then it is not bundled.
    pub sha256: Vec<String>,
    /// Why it is no longer bundled (its author asked, say): set this one
    /// line to pull it from the next release.
    #[serde(default)]
    pub withdrawn: Option<String>,
}

fn starts_on() -> bool {
    true
}

impl DefaultAddOn {
    /// Where a content root holds it: `addons/<id>`.
    pub fn dir(&self) -> String {
        format!("{IMPORT_DIR}/{}", self.id)
    }
    /// Whether it ships: our own always; an original once a copy is pinned
    /// and unless it was withdrawn.
    pub fn ships(&self) -> bool {
        self.original
            .as_ref()
            .is_none_or(|o| !o.sha256.is_empty() && o.withdrawn.is_none())
    }
}

/// Every entry of the list as written, shipping or not, in load order.
pub fn listed() -> &'static [DefaultAddOn] {
    static PARSED: OnceLock<Vec<DefaultAddOn>> = OnceLock::new();
    PARSED.get_or_init(|| {
        let list: List = serde_json::from_str(LIST).expect("packages/default-addons.json is valid");
        assert_eq!(
            list.schema_version, LIST_SCHEMA,
            "packages/default-addons.json schema"
        );
        for addon in &list.addons {
            assert!(
                addon.path.is_some() != addon.original.is_some(),
                "{}: a default Add-On has a path (our own) or an original, not both",
                addon.id
            );
        }
        list.addons
    })
}

/// The default Add-Ons that ship, in load order.
pub fn list() -> &'static [DefaultAddOn] {
    static SHIPPING: OnceLock<Vec<DefaultAddOn>> = OnceLock::new();
    SHIPPING.get_or_init(|| listed().iter().filter(|a| a.ships()).cloned().collect())
}

/// Whether `id` is a default Add-On that starts turned on.
pub fn is_default(id: &str) -> bool {
    list().iter().any(|a| a.enabled && a.id == id)
}

/// `addon` as `packages.json` lists it when installed under `root`: its
/// manifest at `addons/<id>` names it. None when it is not installed.
pub fn installed_entry(root: &Path, addon: &DefaultAddOn) -> Option<PackageEntry> {
    installed_at(root, &addon.id).map(|(entry, _)| entry)
}

/// `addon` installed under `root`, then the companions its manifest names
/// installed beside it: a bundled original's host rules (`addons/<id>-rules`),
/// which Import turns on and off with it. Empty when it is not installed.
pub fn installed_with_companions(root: &Path, addon: &DefaultAddOn) -> Vec<PackageEntry> {
    let Some((entry, companions)) = installed_at(root, &addon.id) else {
        return vec![];
    };
    let mut out = vec![entry];
    out.extend(
        companions
            .iter()
            .filter_map(|id| installed_at(root, id).map(|(entry, _)| entry)),
    );
    out
}

/// The package `addons/<id>` holds when its manifest names `id`, and the
/// companions it names.
fn installed_at(root: &Path, id: &str) -> Option<(PackageEntry, Vec<String>)> {
    let dir = format!("{IMPORT_DIR}/{id}");
    let info = read_info(&root.join(&dir).join(MANIFEST_FILE))?;
    if info.id != id {
        return None;
    }
    let side = info.side()?;
    Some((
        PackageEntry {
            id: info.id,
            version: info.version,
            side,
            dir,
            role: None,
        },
        info.companions,
    ))
}

/// The default Add-Ons installed under `root` that start turned on, each
/// followed by its installed companions, in load order.
pub fn installed(root: &Path) -> Vec<PackageEntry> {
    list()
        .iter()
        .filter(|a| a.enabled)
        .flat_map(|a| installed_with_companions(root, a))
        .collect()
}

/// The repository's `packages/` folder when `content_root` is a source
/// checkout's `content/`: the folder beside it holds [`LIST_FILE`]. None
/// for a release, whose content was installed when it was packaged.
pub fn checkout_packages(content_root: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(content_root).ok()?;
    let packages = absolute.parent()?.join("packages");
    packages.join(LIST_FILE).is_file().then_some(packages)
}

/// What [`install`] changed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Copied in because they were missing or differed from the checkout's.
    pub copied: Vec<String>,
    /// Added to the content root's own lists, or brought in step there with
    /// the installed copy's version.
    pub listed: Vec<String>,
}

impl Installed {
    pub fn is_empty(&self) -> bool {
        self.copied.is_empty() && self.listed.is_empty()
    }
    /// Every Add-On changed either way, once each, in load order.
    pub fn ids(&self) -> Vec<&str> {
        list()
            .iter()
            .map(|a| a.id.as_str())
            .filter(|id| self.copied.iter().chain(&self.listed).any(|c| c == id))
            .collect()
    }
}

/// Set to 1, [`install_when_asked`] installs a checkout's default Add-Ons.
pub const INSTALL_ENV: &str = "BRI_INSTALL_DEFAULT_ADD_ONS";

/// [`install_from_checkout`] only when [`INSTALL_ENV`] is 1: what
/// `bri-client --check` and `bri-server` do, so validating or serving the
/// shared main checkout's content never writes to it. A release's content
/// already has its default Add-Ons (it is packaged with them).
pub fn install_when_asked(content_root: &Path) -> Result<Option<Installed>> {
    if std::env::var_os(INSTALL_ENV).is_some_and(|v| v == "1") {
        install_from_checkout(content_root)
    } else {
        Ok(None)
    }
}

/// [`install`] from the checkout `content_root` belongs to. Does nothing for
/// a content root outside a checkout (a release).
pub fn install_from_checkout(content_root: &Path) -> Result<Option<Installed>> {
    let Some(packages) = checkout_packages(content_root) else {
        return Ok(None);
    };
    install(content_root, &packages)
        .with_context(|| {
            format!(
                "Installing the default Add-Ons from {} into {}",
                packages.display(),
                content_root.display()
            )
        })
        .map(Some)
}

/// Make `root`'s default Add-Ons those in `packages` (a checkout's
/// `packages/`): copy each of our own into `addons/<id>` when it is missing
/// or differs. Bundled originals are never in a checkout; one installed
/// from the bundle is listed like the rest. When `root` has its own `packages.json`, a default that list
/// neither turns on nor off (`packages-disabled.json`) is turned on (unless
/// it is carried turned off), and a
/// listed one's entry follows the installed copy's version. A default the
/// player turned off stays off.
pub fn install(root: &Path, packages: &Path) -> Result<Installed> {
    ensure!(root.is_dir(), "Missing content root {}", root.display());
    let mut out = Installed::default();
    for addon in list() {
        let Some(path) = &addon.path else {
            continue;
        };
        let source = packages.join(path);
        ensure!(
            source.join(MANIFEST_FILE).is_file(),
            "The default Add-On `{}` is missing from {}",
            addon.id,
            source.display()
        );
        let target = root.join(addon.dir());
        if !same_tree(&source, &target)? {
            replace_tree(&source, &target, &addon.id)?;
            out.copied.push(addon.id.clone());
        }
    }
    out.listed = update_lists(root)?;
    Ok(out)
}

fn update_lists(root: &Path) -> Result<Vec<String>> {
    let enabled_path = root.join(PACKAGES_FILE);
    if !enabled_path.exists() {
        // The base game's list and the installed defaults already.
        return Ok(vec![]);
    }
    let mut enabled = PackageSet::load(&enabled_path)?;
    let disabled_path = root.join(DISABLED_FILE);
    let mut disabled = if disabled_path.exists() {
        PackageSet::load(&disabled_path)?
    } else {
        PackageSet {
            schema_version: PACKAGES_SCHEMA,
            packages: vec![],
        }
    };
    let (mut on, mut off) = (false, false);
    let mut changed = Vec::new();
    for addon in list() {
        // It, then its companions (an original's host rules), which go
        // where it goes: on after it when it is on, off when the player
        // turned it off.
        let mut group_on = addon.enabled;
        let mut after: Option<usize> = None;
        for (n, entry) in installed_with_companions(root, addon)
            .into_iter()
            .enumerate()
        {
            let position = |set: &PackageSet| set.packages.iter().position(|p| p.id == entry.id);
            let id = entry.id.clone();
            let moved = if let Some(i) = position(&enabled) {
                group_on |= n == 0;
                after = Some(i);
                let moved = follow(&mut enabled.packages[i], &entry);
                on |= moved;
                moved
            } else if let Some(i) = position(&disabled) {
                group_on &= n != 0;
                let moved = follow(&mut disabled.packages[i], &entry);
                off |= moved;
                moved
            } else if group_on {
                let at = after.map_or(enabled.packages.len(), |i| i + 1);
                enabled.packages.insert(at, entry);
                after = Some(at);
                on = true;
                true
            } else {
                // Off until the player turns it on; the library finds it.
                false
            };
            if moved {
                changed.push(id);
            }
        }
    }
    // The disabled list first, as the library writes them: a failure
    // between the two leaves a package listed twice, never lost.
    if off {
        disabled.validate().into_result()?;
        write_atomic(&disabled_path, &disabled)?;
    }
    if on {
        enabled.validate().into_result()?;
        write_atomic(&enabled_path, &enabled)?;
    }
    Ok(changed)
}

/// Bring a listed default's entry in step with the installed copy. False
/// when it already is, or lists the player's own copy elsewhere, which is
/// theirs to keep.
fn follow(listed: &mut PackageEntry, installed: &PackageEntry) -> bool {
    if listed.dir != installed.dir || listed == installed {
        return false;
    }
    *listed = installed.clone();
    true
}

/// Whether `copy` holds exactly `source`'s files, byte for byte.
fn same_tree(source: &Path, copy: &Path) -> Result<bool> {
    if !copy.is_dir() {
        return Ok(false);
    }
    let files = tree(source)?;
    // A copy holding a link or an odd entry is replaced, not followed.
    let Ok(copied) = tree(copy) else {
        return Ok(false);
    };
    if files != copied {
        return Ok(false);
    }
    for file in &files {
        if std::fs::read(source.join(file))? != std::fs::read(copy.join(file))? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Every file under `dir`, relative to it, sorted. Links are refused.
fn tree(dir: &Path) -> Result<Vec<PathBuf>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir).with_context(|| format!("Reading {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            let kind = std::fs::symlink_metadata(&path)?.file_type();
            if kind.is_dir() {
                walk(root, &path, out)?;
            } else if kind.is_file() {
                out.push(path.strip_prefix(root)?.to_path_buf());
            } else {
                bail!("{} is not a plain file or folder", path.display());
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    out.sort();
    Ok(out)
}

/// Replace `target` with a copy of `source`. The copy is made beside it
/// under a hidden name (the library never lists those) and swapped in, so
/// `target` is never half copied.
fn replace_tree(source: &Path, target: &Path, id: &str) -> Result<()> {
    let parent = target.parent().context("An Add-On folder has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
    let pid = std::process::id();
    let fresh = parent.join(format!(".{id}.installing-{pid}"));
    let old = parent.join(format!(".{id}.replaced-{pid}"));
    for leftover in [&fresh, &old] {
        let _ = std::fs::remove_dir_all(leftover);
    }
    let files = tree(source)?;
    for file in &files {
        let to = fresh.join(file);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(source.join(file), &to)
            .with_context(|| format!("Copying {}", source.join(file).display()))?;
    }
    let had_target = std::fs::symlink_metadata(target).is_ok();
    let swapped = (|| -> std::io::Result<()> {
        if had_target {
            rename(target, &old)?;
        }
        if let Err(error) = rename(&fresh, target) {
            if had_target {
                let _ = rename(&old, target);
            }
            return Err(error);
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&fresh);
    let _ = std::fs::remove_dir_all(&old);
    match swapped {
        Ok(()) => Ok(()),
        // Another copy of the game installed the same files meanwhile.
        Err(_) if same_tree(source, target)? => Ok(()),
        Err(error) => Err(error).with_context(|| format!("Installing {}", target.display())),
    }
}

/// A rename, retried briefly: on Windows a virus scanner or indexer can
/// hold a just-written file for a moment.
fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match std::fs::rename(from, to) {
            Err(error) if attempt < 5 && error.kind() == std::io::ErrorKind::PermissionDenied => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(40 * attempt));
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{self, Requirement, Version};
    use crate::library::Library;

    fn repo_packages() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages")
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bri-defaults-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ids(set: &[PackageEntry]) -> Vec<&str> {
        set.iter().map(|p| p.id.as_str()).collect()
    }

    /// Our own default Add-Ons, as `install` copies them: the list's
    /// entries with a `path`, in load order.
    fn ours() -> Vec<&'static str> {
        listed()
            .iter()
            .filter(|a| a.path.is_some())
            .map(|a| a.id.as_str())
            .collect()
    }

    /// What `tools/addon_bundle.py install` leaves for a bundled original:
    /// its import at `addons/<id>`. A stand-in with only a manifest.
    fn install_original(root: &Path, id: &str, version: &str) {
        let dir = root.join(IMPORT_DIR).join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(MANIFEST_FILE),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "version": "{version}", "api": 1,
                     "authors": ["Kaje", "Ephialtes"], "dependencies": {{ "v20-vehicles": "*" }},
                     "provides": [{{ "kind": "vehicles", "id": "{id}:vehicles/main", "file": "assets/vehicles.json" }}] }}"#
            ),
        )
        .unwrap();
    }

    /// An original whose port wrote host rules, as Import leaves it and
    /// `tools/addon_bundle.py install` copies it: the import at
    /// `addons/<id>` naming its companion, the rules at `addons/<id>-rules`.
    /// Stand-ins with only manifests.
    fn install_original_with_rules(root: &Path, id: &str) {
        install_original(root, id, "1.0.0");
        let manifest = root.join(IMPORT_DIR).join(id).join(MANIFEST_FILE);
        let mut info: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
        info["companions"] = serde_json::json!([format!("{id}-rules")]);
        std::fs::write(&manifest, serde_json::to_vec(&info).unwrap()).unwrap();
        let rules = root.join(IMPORT_DIR).join(format!("{id}-rules"));
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(
            rules.join(MANIFEST_FILE),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}-rules", "version": "1.0.0", "api": 1,
                     "dependencies": {{ "{id}": "=1.0.0" }}, "capabilities": ["player"],
                     "provides": [{{ "kind": "behaviour", "id": "{id}-rules:behaviour/behaviour", "file": "behaviour.json" }}] }}"#
            ),
        )
        .unwrap();
    }

    /// A bundled original that starts on starts with its port's host rules,
    /// right after it: without them its scripted behaviour (a hookshot's
    /// pull, a shovel's dig) never runs. The same in a root with no list, one
    /// whose list predates the rules, and not at all once the player turned
    /// the original off.
    #[test]
    fn an_original_starts_with_its_host_rules() {
        let root = scratch("rules");
        install_original_with_rules(&root, "tool_duplicator");
        let on = installed(&root);
        assert_eq!(ids(&on), ["tool_duplicator", "tool_duplicator-rules"]);
        assert_eq!(on[1].dir, "addons/tool_duplicator-rules");
        assert_eq!(on[1].side, crate::packages::Side::Server);
        let base = PackageSet::base().packages.len();
        let set = PackageSet::load_root(&root).unwrap();
        assert_eq!(
            ids(&set.packages[base..]),
            ["tool_duplicator", "tool_duplicator-rules"]
        );

        // A list written before the rules shipped gains them after it.
        let entry = |id: &str| {
            format!(
                r#"{{ "id": "{id}", "version": "1.0.0", "side": "shared", "dir": "addons/{id}" }}"#
            )
        };
        std::fs::write(
            root.join(PACKAGES_FILE),
            format!(
                r#"{{ "schema_version": 1, "packages": [{}, {}] }}"#,
                entry("tool_duplicator"),
                entry("brick_mirror")
            ),
        )
        .unwrap();
        install(&root, &repo_packages()).unwrap();
        let listed = PackageSet::load(&root.join(PACKAGES_FILE)).unwrap();
        assert_eq!(
            ids(&listed.packages),
            ["tool_duplicator", "tool_duplicator-rules", "brick_mirror"]
        );
        assert!(install(&root, &repo_packages()).unwrap().is_empty());

        // Turned off by the player: its rules stay off with it.
        std::fs::write(
            root.join(PACKAGES_FILE),
            r#"{ "schema_version": 1, "packages": [] }"#,
        )
        .unwrap();
        std::fs::write(
            root.join(DISABLED_FILE),
            format!(
                r#"{{ "schema_version": 1, "packages": [{}] }}"#,
                entry("tool_duplicator")
            ),
        )
        .unwrap();
        install(&root, &repo_packages()).unwrap();
        let listed = PackageSet::load(&root.join(PACKAGES_FILE)).unwrap();
        assert_eq!(ids(&listed.packages), ["brick_mirror"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_list_names_whole_add_ons_that_load_on_the_base_game() {
        let listed_ids: Vec<&str> = listed().iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            listed_ids[..3],
            ["tool_duplicator", "vehicle_stunt_plane", "brick_mirror"]
        );
        assert!(ours().contains(&"brick_mirror"));
        // On by default: the Duplicator, the Stunt Plane and the Mirror.
        let on: Vec<&str> = listed()
            .iter()
            .filter(|a| a.enabled)
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(
            on,
            ["tool_duplicator", "vehicle_stunt_plane", "brick_mirror"]
        );
        let mut available: Vec<(String, String)> = PackageSet::base()
            .packages
            .into_iter()
            .map(|p| (p.id, p.version))
            .collect();
        for addon in listed() {
            assert!(!id::is_reserved(&addon.id), "{}", addon.id);
            assert_eq!(
                listed().iter().filter(|a| a.id == addon.id).count(),
                1,
                "{} is listed twice",
                addon.id
            );
            match (&addon.path, &addon.original) {
                (Some(path), None) => {
                    assert!(crate::path::problem(path).is_none(), "{path}");
                    let manifest = repo_packages().join(path).join(MANIFEST_FILE);
                    let info =
                        read_info(&manifest).unwrap_or_else(|| panic!("{}", manifest.display()));
                    assert_eq!(info.id, addon.id);
                    assert!(
                        info.side().is_some(),
                        "{} mixes server and client content",
                        addon.id
                    );
                    available.push((info.id.clone(), info.version.clone()));
                }
                (None, Some(original)) => {
                    assert!(!original.title.is_empty() && !original.authors.is_empty());
                    assert!(Version::parse(&original.version).is_ok(), "{}", addon.id);
                    for sha in &original.sha256 {
                        assert!(
                            sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
                            "{}: {sha}",
                            addon.id
                        );
                    }
                    available.push((addon.id.clone(), original.version.clone()));
                }
                _ => panic!("{} needs a path or an original", addon.id),
            }
        }
        // Every dependency of our own is the base game or another default.
        for addon in listed() {
            let Some(path) = &addon.path else {
                continue;
            };
            let info = read_info(&repo_packages().join(path).join(MANIFEST_FILE)).unwrap();
            for (dependency, requirement) in &info.dependencies {
                let (_, version) = available
                    .iter()
                    .find(|(id, _)| id == dependency)
                    .unwrap_or_else(|| panic!("{} needs {dependency}", addon.id));
                assert!(
                    Requirement::parse(requirement)
                        .unwrap()
                        .matches(Version::parse(version).unwrap()),
                    "{} needs {dependency} {requirement}",
                    addon.id
                );
            }
        }
    }

    /// The source repository never holds another author's Add-On: nothing
    /// under `packages/` names a bundled original or was imported from a
    /// classic Add-On (`tools/addon_bundle.py` brings those into releases).
    #[test]
    fn no_original_is_committed() {
        fn manifests(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    manifests(&path, out);
                } else if path.file_name().is_some_and(|n| n == MANIFEST_FILE) {
                    out.push(path);
                }
            }
        }
        let mut found = Vec::new();
        manifests(&repo_packages(), &mut found);
        assert!(found.len() > 10);
        for manifest in found {
            let info = read_info(&manifest).unwrap_or_else(|| panic!("{}", manifest.display()));
            assert!(
                !listed()
                    .iter()
                    .any(|a| a.original.is_some() && a.id == info.id),
                "{} is a bundled original, committed",
                manifest.display()
            );
            assert!(
                !info
                    .source()
                    .is_some_and(|s| s.starts_with("Blockland Add-On ")),
                "{} was imported from a classic Add-On, committed",
                manifest.display()
            );
        }
    }

    #[test]
    fn an_original_ships_once_pinned_and_until_withdrawn() {
        let original = |sha256: &[&str], withdrawn: Option<&str>| DefaultAddOn {
            id: "weapon_example".into(),
            path: None,
            original: Some(Original {
                addon: "Weapon_Example".into(),
                title: "Example".into(),
                authors: vec!["Someone".into()],
                version: "1.0.0".into(),
                sha256: sha256.iter().map(|s| s.to_string()).collect(),
                withdrawn: withdrawn.map(Into::into),
            }),
            enabled: true,
        };
        let sha = "0".repeat(64);
        assert!(!original(&[], None).ships());
        assert!(original(&[&sha], None).ships());
        assert!(!original(&[&sha], Some("Its author asked")).ships());
        assert!(list().iter().all(DefaultAddOn::ships));
        assert!(listed().iter().filter(|a| a.ships()).eq(list().iter()));
    }

    /// Every showcase Add-On (`packages/showcase`) either ships, listed
    /// turned off for players to turn on, or is held back on purpose: one
    /// meant to ship cannot be left out of the releases, which package
    /// exactly this list.
    #[test]
    fn every_showcase_add_on_ships_turned_off_or_is_held_back() {
        const HELD_BACK: [&str; 0] = [];
        let mut found: Vec<(String, String)> = std::fs::read_dir(repo_packages().join("showcase"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|dir| dir.join(MANIFEST_FILE).is_file())
            .map(|dir| {
                let id = read_info(&dir.join(MANIFEST_FILE)).unwrap().id;
                (id, dir.file_name().unwrap().to_string_lossy().into_owned())
            })
            .collect();
        found.sort();
        assert!(!found.is_empty());
        for (id, folder) in &found {
            match list().iter().find(|a| &a.id == id) {
                Some(addon) => {
                    assert!(
                        !HELD_BACK.contains(&id.as_str()),
                        "{id} is both listed and held back"
                    );
                    assert!(!addon.enabled, "showcase Add-On {id} must ship turned off");
                    assert_eq!(addon.path.as_deref(), Some(&*format!("showcase/{folder}")));
                }
                None => assert!(
                    HELD_BACK.contains(&id.as_str()),
                    "showcase Add-On {id} is neither in packages/default-addons.json nor held back"
                ),
            }
        }
    }

    #[test]
    fn a_fresh_content_root_gets_them_on_without_a_package_list() {
        let root = scratch("fresh");
        let done = install(&root, &repo_packages()).unwrap();
        assert_eq!(done.copied, ours());
        assert!(done.listed.is_empty());
        assert!(
            !root.join(PACKAGES_FILE).exists(),
            "installing wrote a package list"
        );
        let base = PackageSet::base().packages;
        let defaults = |root: &Path| {
            let set = PackageSet::load_root(root).unwrap();
            assert_eq!(set.packages[..base.len()], base[..]);
            assert!(set.validate().is_empty());
            set.packages[base.len()..].to_vec()
        };
        // The originals come from the bundle, not the checkout.
        assert_eq!(ids(&defaults(&root)), ["brick_mirror"]);
        // Installed from the bundle, an original is on in its place.
        install_original(&root, "vehicle_stunt_plane", "1.0.0");
        let on = defaults(&root);
        assert_eq!(ids(&on), ["vehicle_stunt_plane", "brick_mirror"]);
        assert!(on.iter().all(|p| p.dir == format!("addons/{}", p.id)));
        assert_eq!(on[0].side, crate::packages::Side::Shared);
        // The Add-Ons screen shows them on, and the Ragdoll there to turn
        // on: drawn on each screen, but the host decides for everyone, so
        // it is shared and joiners download it.
        let library = Library::scan(&root).unwrap();
        for addon in list() {
            let Some(entry) = library.get(&addon.id) else {
                assert!(addon.original.is_some(), "{} is not installed", addon.id);
                continue;
            };
            assert_eq!(entry.enabled, addon.enabled, "{}", addon.id);
            assert_eq!(entry.discovered, !addon.enabled, "{}", addon.id);
        }
        let plane = library.get("vehicle_stunt_plane").unwrap();
        assert_eq!(plane.info.as_ref().unwrap().authors, ["Kaje", "Ephialtes"]);
        let ragdoll = library.get("ragdoll").unwrap();
        assert_eq!(ragdoll.package.side, crate::packages::Side::Shared);
        for id in [
            "ragdoll",
            "brick_portal",
            "gravity-gun-tool",
            "gravity-gun",
            "gravity-gun-fx",
            "blockhead_bot",
        ] {
            let entry = library.get(id).unwrap();
            assert!(entry.problems.is_empty(), "{id}: {:?}", entry.problems);
            assert!(!is_default(id), "{id} starts off");
        }
        // Turning on the Gravity Gun's effects brings its rule and tool.
        let plan = library.plan("gravity-gun-fx", true);
        assert_eq!(plan.also, ["gravity-gun-tool", "gravity-gun"], "{plan:?}");
        // A second start changes nothing, and leaves the original alone.
        assert!(install(&root, &repo_packages()).unwrap().is_empty());
        assert!(
            root.join("addons/vehicle_stunt_plane")
                .join(MANIFEST_FILE)
                .is_file()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_changed_copy_is_replaced_by_the_checkouts() {
        let root = scratch("changed");
        install(&root, &repo_packages()).unwrap();
        let manifest = root.join("addons/brick_mirror").join(MANIFEST_FILE);
        std::fs::write(&manifest, "{}").unwrap();
        std::fs::write(root.join("addons/brick_mirror/stray.txt"), "x").unwrap();
        let done = install(&root, &repo_packages()).unwrap();
        assert_eq!(done.copied, ["brick_mirror"]);
        assert_eq!(
            std::fs::read(&manifest).unwrap(),
            std::fs::read(repo_packages().join("brick_mirror").join(MANIFEST_FILE)).unwrap()
        );
        assert!(!root.join("addons/brick_mirror/stray.txt").exists());
        let hidden: Vec<_> = std::fs::read_dir(root.join("addons"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(hidden.is_empty(), "leftovers: {hidden:?}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_players_own_list_keeps_what_they_turned_off() {
        let root = scratch("own-list");
        let entry = |id: &str, version: &str| {
            format!(
                r#"{{ "id": "{id}", "version": "{version}", "side": "shared", "dir": "addons/{id}" }}"#
            )
        };
        std::fs::write(
            root.join(PACKAGES_FILE),
            r#"{ "schema_version": 1, "packages": [] }"#,
        )
        .unwrap();
        std::fs::write(
            root.join(DISABLED_FILE),
            format!(
                r#"{{ "schema_version": 1, "packages": [{}] }}"#,
                entry("vehicle_stunt_plane", "0.9.0")
            ),
        )
        .unwrap();
        // A newer import of the original, from the bundle.
        install_original(&root, "vehicle_stunt_plane", "1.0.0");
        let done = install(&root, &repo_packages()).unwrap();
        assert_eq!(done.listed, ["vehicle_stunt_plane", "brick_mirror"]);
        let on = PackageSet::load(&root.join(PACKAGES_FILE)).unwrap();
        assert_eq!(ids(&on.packages), ["brick_mirror"]);
        let off = PackageSet::load(&root.join(DISABLED_FILE)).unwrap();
        assert_eq!(ids(&off.packages), ["vehicle_stunt_plane"]);
        assert_eq!(off.packages[0].version, "1.0.0");
        assert!(install(&root, &repo_packages()).unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_a_checkouts_content_is_installed_into() {
        let root = scratch("checkout");
        std::fs::create_dir_all(root.join("checkout/content")).unwrap();
        std::fs::create_dir_all(root.join("checkout/packages")).unwrap();
        std::fs::write(root.join("checkout/packages").join(LIST_FILE), LIST).unwrap();
        std::fs::create_dir_all(root.join("release/content")).unwrap();
        assert_eq!(
            checkout_packages(&root.join("checkout/content")),
            Some(root.join("checkout/packages"))
        );
        assert_eq!(checkout_packages(&root.join("release/content")), None);
        assert_eq!(
            install_from_checkout(&root.join("release/content")).unwrap(),
            None
        );
        // The checkout's packages/ lacks the Add-Ons themselves.
        assert!(install_from_checkout(&root.join("checkout/content")).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
