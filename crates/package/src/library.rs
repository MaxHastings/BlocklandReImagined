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
pub const SERVER_KINDS: &[&str] = &["behaviour", "script", "world", "entity", "mode"];
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
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub provides: Vec<Provided>,
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
    /// Problems with the lists themselves.
    pub problems: Diagnostics,
}

impl Library {
    /// Read both lists and discover unlisted packages under `root`.
    /// `packages.json` falls back to the base game's list when absent, as
    /// the loaders do. A list that does not parse is an error: the library
    /// never rewrites a file it could not read.
    pub fn scan(root: &Path) -> Result<Self> {
        ensure!(root.is_dir(), "Missing content root {}", root.display());
        let enabled = PackageSet::load_root(root)?;
        let disabled_path = root.join(DISABLED_FILE);
        let disabled = if disabled_path.exists() {
            PackageSet::load(&disabled_path)?
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
                        "more than {MAX_PACKAGES} packages are installed; the rest are not shown"
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
            let only = |kinds: &[&str]| {
                !info.provides.is_empty()
                    && info
                        .provides
                        .iter()
                        .all(|p| kinds.contains(&p.kind.as_str()))
            };
            let (server_only, client_only) = (only(SERVER_KINDS), only(CLIENT_KINDS));
            let has = |kinds: &[&str]| {
                info.provides
                    .iter()
                    .any(|p| kinds.contains(&p.kind.as_str()))
            };
            let package = PackageEntry {
                id: info.id.clone(),
                version: info.version.clone(),
                side: if server_only {
                    Side::Server
                } else if client_only {
                    Side::Client
                } else {
                    Side::Shared
                },
                dir: dir.clone(),
                role: None,
            };
            let mut found = entry(root, package, false, true);
            if has(SERVER_KINDS) && has(CLIENT_KINDS) {
                found.problems.push(
                    Diagnostic::error(
                        "library.mixed_sides",
                        format!("`{}` has both server behaviour and client visuals", info.id),
                    )
                    .at(format!("{dir}/{MANIFEST_FILE}"))
                    .hint("split it into two Add-Ons: one for the server rules, one for models and HUD panels, the second depending on the first"),
                );
            }
            entries.push(found);
        }
        let mut library = Self {
            root: root.to_path_buf(),
            entries,
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
            // Dependencies first, then the package, after everything already on.
            for id in plan.also.iter().chain([&plan.id]) {
                if let Some(e) = self.get(id)
                    && !e.enabled
                {
                    on.push(e.package.clone());
                }
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

fn read_info(path: &Path) -> Option<PackageInfo> {
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
        if name.starts_with('.') {
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

/// One entry per line, like the base list, written to a temporary file and
/// renamed over the target so a crash never leaves half a list.
fn write_atomic(path: &Path, set: &PackageSet) -> Result<()> {
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

    /// A base package, three mods in a chain (hud → economy → world) and one
    /// with a missing dependency.
    fn fixture(name: &str) -> Root {
        let r = root(name);
        std::fs::create_dir_all(r.0.join("ui-pack-003")).unwrap();
        list(
            &r.0,
            json!([{ "id": "v20-ui", "version": "3.0.0", "side": "client", "dir": "ui-pack-003", "role": "ui_pack" }]),
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
                { "id": "v20-ui", "version": "3.0.0", "side": "client", "dir": "ui-pack-003", "role": "ui_pack" },
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
        // A missing package can still be turned off.
        let mut lib = lib;
        lib.apply(&lib.plan("gone", false)).unwrap();
        assert!(!lib.get("gone").unwrap().enabled);
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
}
