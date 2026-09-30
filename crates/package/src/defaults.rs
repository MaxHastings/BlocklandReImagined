//! Default Add-Ons: the Add-Ons every copy of the game has on until the
//! player turns them off (today the Duplicator, the Stunt Plane and the
//! Mirror), and those it carries turned off for players to turn on
//! (`"enabled": false`, like the Ragdoll, the Gravity Gun and the Advanced
//! Duplicator).
//!
//! One list, `packages/default-addons.json`, names them in load order. This
//! module, the release packager (`tools/package_playtest.ps1`) and
//! `tools/default_addons.py` all read it. Each one is committed under the
//! repository's `packages/<path>` and a content root holds it at
//! `addons/<id>`, where the game keeps Add-Ons:
//!
//! - A release gets them when it is packaged; its `packages.json` lists them.
//! - A source checkout's `content/` is generated, never committed, so the
//!   game and the dedicated server install them when they start
//!   ([`install_from_checkout`]): missing copies are copied in and copies
//!   that differ from the checkout's are replaced.
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
    /// Its folder under the repository's `packages/`.
    pub path: String,
    /// How an imported one was converted from its original archive
    /// (`tools/default_addons.py`). The game does not read it.
    #[serde(default)]
    pub import: Option<serde_json::Value>,
    /// False for one carried turned off: installed, never turned on for
    /// the player.
    #[serde(default = "starts_on")]
    pub enabled: bool,
}

fn starts_on() -> bool {
    true
}

impl DefaultAddOn {
    /// Where a content root holds it: `addons/<id>`.
    pub fn dir(&self) -> String {
        format!("{IMPORT_DIR}/{}", self.id)
    }
}

/// The default Add-Ons, in load order.
pub fn list() -> &'static [DefaultAddOn] {
    static PARSED: OnceLock<Vec<DefaultAddOn>> = OnceLock::new();
    PARSED.get_or_init(|| {
        let list: List = serde_json::from_str(LIST).expect("packages/default-addons.json is valid");
        assert_eq!(
            list.schema_version, 1,
            "packages/default-addons.json schema"
        );
        list.addons
    })
}

/// Whether `id` is a default Add-On that starts turned on.
pub fn is_default(id: &str) -> bool {
    list().iter().any(|a| a.enabled && a.id == id)
}

/// `addon` as `packages.json` lists it when installed under `root`: its
/// manifest at `addons/<id>` names it. None when it is not installed.
pub fn installed_entry(root: &Path, addon: &DefaultAddOn) -> Option<PackageEntry> {
    let dir = addon.dir();
    let info = read_info(&root.join(&dir).join(MANIFEST_FILE))?;
    if info.id != addon.id {
        return None;
    }
    let side = info.side()?;
    Some(PackageEntry {
        id: info.id,
        version: info.version,
        side,
        dir,
        role: None,
    })
}

/// The default Add-Ons installed under `root` that start turned on, in
/// load order.
pub fn installed(root: &Path) -> Vec<PackageEntry> {
    list()
        .iter()
        .filter(|a| a.enabled)
        .filter_map(|a| installed_entry(root, a))
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
/// `packages/`): copy each into `addons/<id>` when it is missing or
/// differs. When `root` has its own `packages.json`, a default that list
/// neither turns on nor off (`packages-disabled.json`) is turned on (unless
/// it is carried turned off), and a
/// listed one's entry follows the installed copy's version. A default the
/// player turned off stays off.
pub fn install(root: &Path, packages: &Path) -> Result<Installed> {
    ensure!(root.is_dir(), "Missing content root {}", root.display());
    let mut out = Installed::default();
    for addon in list() {
        let source = packages.join(&addon.path);
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
        let Some(entry) = installed_entry(root, addon) else {
            continue;
        };
        let position = |set: &PackageSet| set.packages.iter().position(|p| p.id == addon.id);
        let moved = if let Some(i) = position(&enabled) {
            let moved = follow(&mut enabled.packages[i], &entry);
            on |= moved;
            moved
        } else if let Some(i) = position(&disabled) {
            let moved = follow(&mut disabled.packages[i], &entry);
            off |= moved;
            moved
        } else if addon.enabled {
            enabled.packages.push(entry);
            on = true;
            true
        } else {
            // Off until the player turns it on; the library finds it.
            false
        };
        if moved {
            changed.push(addon.id.clone());
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

    #[test]
    fn the_list_names_whole_add_ons_that_load_on_the_base_game() {
        let ids: Vec<&str> = list().iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "duplicator",
                "duplicator-tool",
                "vehicle_stunt_plane",
                "brick_mirror",
                "ragdoll",
                "brick_portal",
                "gravity-gun-tool",
                "gravity-gun",
                "gravity-gun-fx",
                "advanced-duplicator-tool",
                "advanced-duplicator",
                "blockhead_bot"
            ]
        );
        let mut available: Vec<(String, String)> = PackageSet::base()
            .packages
            .into_iter()
            .map(|p| (p.id, p.version))
            .collect();
        for addon in list() {
            assert!(!id::is_reserved(&addon.id), "{}", addon.id);
            assert!(
                crate::path::problem(&addon.path).is_none(),
                "{}",
                addon.path
            );
            let manifest = repo_packages().join(&addon.path).join(MANIFEST_FILE);
            let info = read_info(&manifest).unwrap_or_else(|| panic!("{}", manifest.display()));
            assert_eq!(info.id, addon.id);
            assert!(
                info.side().is_some(),
                "{} mixes server and client content",
                addon.id
            );
            available.push((info.id.clone(), info.version.clone()));
        }
        // Every dependency is the base game or another default.
        for addon in list() {
            let info = read_info(&repo_packages().join(&addon.path).join(MANIFEST_FILE)).unwrap();
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

    /// Every showcase Add-On (`packages/showcase`) either ships, listed
    /// turned off for players to turn on, or is held back on purpose: one
    /// meant to ship cannot be left out of the releases, which package
    /// exactly this list.
    #[test]
    fn every_showcase_add_on_ships_turned_off_or_is_held_back() {
        const HELD_BACK: [&str; 3] = ["steel-ball", "steel-ball-kit", "steel-ball-fx"];
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
                    assert_eq!(addon.path, format!("showcase/{folder}"));
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
        assert_eq!(
            done.copied,
            [
                "duplicator",
                "duplicator-tool",
                "vehicle_stunt_plane",
                "brick_mirror",
                "ragdoll",
                "brick_portal",
                "gravity-gun-tool",
                "gravity-gun",
                "gravity-gun-fx",
                "advanced-duplicator-tool",
                "advanced-duplicator",
                "blockhead_bot"
            ]
        );
        assert!(done.listed.is_empty());
        assert!(
            !root.join(PACKAGES_FILE).exists(),
            "installing wrote a package list"
        );
        let set = PackageSet::load_root(&root).unwrap();
        let base = PackageSet::base().packages;
        assert_eq!(set.packages[..base.len()], base[..]);
        let defaults = &set.packages[base.len()..];
        assert_eq!(
            ids(defaults),
            [
                "duplicator",
                "duplicator-tool",
                "vehicle_stunt_plane",
                "brick_mirror"
            ]
        );
        assert!(defaults.iter().all(|p| p.dir == format!("addons/{}", p.id)));
        assert_eq!(defaults[0].side, crate::packages::Side::Server);
        assert_eq!(defaults[1].side, crate::packages::Side::Shared);
        assert!(set.validate().is_empty());
        // The Add-Ons screen shows them on, and the Ragdoll there to turn
        // on: drawn on each screen, but the host decides for everyone, so
        // it is shared and joiners download it.
        let library = Library::scan(&root).unwrap();
        for addon in list() {
            let entry = library.get(&addon.id).unwrap();
            assert_eq!(entry.enabled, addon.enabled, "{}", addon.id);
            assert_eq!(entry.discovered, !addon.enabled, "{}", addon.id);
        }
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
        // A second start changes nothing.
        assert!(install(&root, &repo_packages()).unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_changed_copy_is_replaced_by_the_checkouts() {
        let root = scratch("changed");
        install(&root, &repo_packages()).unwrap();
        let script = root.join("addons/duplicator/duplicator.rhai");
        std::fs::write(&script, "// edited").unwrap();
        std::fs::write(root.join("addons/duplicator/stray.txt"), "x").unwrap();
        let done = install(&root, &repo_packages()).unwrap();
        assert_eq!(done.copied, ["duplicator"]);
        assert_eq!(
            std::fs::read(&script).unwrap(),
            std::fs::read(repo_packages().join("duplicator/duplicator/duplicator.rhai")).unwrap()
        );
        assert!(!root.join("addons/duplicator/stray.txt").exists());
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
        let done = install(&root, &repo_packages()).unwrap();
        assert_eq!(
            done.listed,
            [
                "duplicator",
                "duplicator-tool",
                "vehicle_stunt_plane",
                "brick_mirror"
            ]
        );
        let on = PackageSet::load(&root.join(PACKAGES_FILE)).unwrap();
        assert_eq!(
            ids(&on.packages),
            ["duplicator", "duplicator-tool", "brick_mirror"]
        );
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
