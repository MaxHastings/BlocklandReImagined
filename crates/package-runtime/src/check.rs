//! Check one Add-On folder the way the game will read it, and say what it
//! is: which side runs it, what it provides, what it may do and what it
//! needs. Dependencies are looked up among the folder's siblings, so a
//! folder of Add-Ons checks as a set. Nothing is run.
use crate::{
    Catalog,
    content::Kind,
    manifest::{MANIFEST_FILE, Manifest},
    script::Runtime,
};
use bri_package::{
    capability,
    diag::{Diagnostic, Severity},
    packages::{PackageEntry, PackageSet, Side},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
};

/// Sibling folders read while looking for dependencies.
const MAX_SIBLINGS: usize = 4096;

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub ok: bool,
    /// The checked Add-On first, then the dependencies found beside it.
    pub add_ons: Vec<AddOnView>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddOnView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub folder: String,
    pub side: Side,
    pub provides: Vec<ProvideView>,
    /// What it may do, in the words players see.
    pub capabilities: Vec<CapabilityView>,
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvideView {
    pub kind: String,
    pub id: String,
    pub file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityView {
    pub name: String,
    pub meaning: String,
}

/// The side a package providing these kinds loads on, or None when it
/// mixes server behaviour with client visuals (which no side can load).
pub fn side_for<'a>(kinds: impl IntoIterator<Item = &'a str>) -> Option<Side> {
    let sides: Vec<Side> = kinds
        .into_iter()
        .filter_map(Kind::parse)
        .map(Kind::side)
        .collect();
    let all = |side: Side| !sides.is_empty() && sides.iter().all(|s| *s == side);
    if all(Side::Server) {
        Some(Side::Server)
    } else if all(Side::Client) {
        Some(Side::Client)
    } else if sides.is_empty() {
        Some(Side::Shared)
    } else {
        None
    }
}

fn manifest_id(dir: &Path) -> Option<String> {
    let bytes = std::fs::read(dir.join(MANIFEST_FILE)).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value.get("id")?.as_str().map(str::to_owned)
}

/// Check the Add-On in `folder`.
pub fn check(folder: &Path) -> Report {
    let mut diagnostics = Vec::new();
    let fail = |diagnostics: Vec<Diagnostic>| Report {
        ok: false,
        add_ons: vec![],
        diagnostics,
    };
    let (Some(parent), Some(name)) = (
        folder.parent().map(|p| {
            if p.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                p.to_path_buf()
            }
        }),
        folder.file_name().and_then(|n| n.to_str()),
    ) else {
        return fail(vec![
            Diagnostic::error(
                "check.folder",
                format!("{} is not an Add-On folder", folder.display()),
            )
            .hint("pass the folder that holds package.json"),
        ]);
    };
    let Some(target) = manifest_id(folder) else {
        return fail(vec![
            Diagnostic::error(
                "check.manifest",
                format!(
                    "{} has no readable {MANIFEST_FILE} with an id",
                    folder.display()
                ),
            )
            .hint("an Add-On is a folder with package.json; see docs/modding/README.md"),
        ]);
    };
    // Every sibling Add-On by id, for dependencies.
    let mut siblings = BTreeMap::new();
    siblings.insert(target.clone(), name.to_owned());
    if let Ok(dirs) = std::fs::read_dir(&parent) {
        for dir in dirs.flatten().take(MAX_SIBLINGS) {
            let Some(dir_name) = dir.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if dir_name != name
                && let Some(id) = manifest_id(&dir.path())
            {
                siblings.entry(id).or_insert(dir_name);
            }
        }
    }
    // The Add-On and everything it needs, in the order found.
    let mut found: Vec<(Manifest, String)> = Vec::new();
    let mut queue = VecDeque::from([target.clone()]);
    while let Some(id) = queue.pop_front() {
        if found.iter().any(|(m, _)| m.id == id) {
            continue;
        }
        let dir = siblings[&id].clone();
        let bytes = std::fs::read(parent.join(&dir).join(MANIFEST_FILE)).unwrap_or_default();
        match Manifest::parse(&bytes, &id) {
            Ok(manifest) => {
                for dependency in manifest.dependencies.keys() {
                    if bri_package::id::is_reserved(dependency) {
                        // The base game's own packages are always there.
                        continue;
                    } else if siblings.contains_key(dependency) {
                        queue.push_back(dependency.clone());
                    } else {
                        diagnostics.push(
                            Diagnostic::error(
                                "check.dependency.not_found",
                                format!("`{id}` needs `{dependency}`, which is not beside it"),
                            )
                            .at(format!("{id}/{MANIFEST_FILE}"))
                            .hint(format!(
                                "put the `{dependency}` folder next to `{dir}` to check them together"
                            )),
                        );
                    }
                }
                found.push((manifest, dir));
            }
            Err(mut problems) => {
                diagnostics.append(&mut problems);
                if id == target {
                    return fail(diagnostics);
                }
            }
        }
    }
    let mut set = PackageSet {
        schema_version: 1,
        packages: vec![],
    };
    let mut add_ons = Vec::new();
    for (manifest, dir) in &found {
        let side = side_for(manifest.provides.iter().map(|p| p.kind.as_str())).unwrap_or_else(|| {
            diagnostics.push(
                Diagnostic::error(
                    "check.mixed_sides",
                    format!("`{}` has both server behaviour and client visuals", manifest.id),
                )
                .at(format!("{}/{MANIFEST_FILE}", manifest.id))
                .hint("split it into two Add-Ons: one for the server rules, one for models and HUD panels, the second depending on the first"),
            );
            Side::Shared
        });
        set.packages.push(PackageEntry {
            id: manifest.id.clone(),
            version: manifest.version.clone(),
            side,
            dir: dir.clone(),
            role: None,
        });
        add_ons.push(AddOnView {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            folder: dir.clone(),
            side,
            provides: manifest
                .provides
                .iter()
                .map(|p| ProvideView {
                    kind: p.kind.clone(),
                    id: p.id.clone(),
                    file: p.file.clone(),
                })
                .collect(),
            capabilities: manifest
                .capabilities
                .iter()
                .map(|c| CapabilityView {
                    name: c.clone(),
                    meaning: capability::describe(c).unwrap_or("").into(),
                })
                .collect(),
            dependencies: manifest.dependencies.clone(),
        });
    }
    // As the host loads it (and compiles its scripts), then as a player does.
    match Catalog::load(&parent, &set, true) {
        Ok(catalog) => {
            if let Err(mut problems) = Runtime::compile(&catalog) {
                diagnostics.append(&mut problems);
            }
        }
        Err(mut problems) => diagnostics.append(&mut problems),
    }
    if let Err(mut problems) = Catalog::load(&parent, &set, false) {
        diagnostics.append(&mut problems);
    }
    // Missing dependencies are reported above, with where to put them.
    diagnostics.retain(|d| d.code != "set.dependency.missing");
    let mut seen = Vec::new();
    diagnostics.retain(|d| {
        let new = !seen.contains(d);
        if new {
            seen.push(d.clone());
        }
        new
    });
    Report {
        ok: !diagnostics.iter().any(|d| d.severity == Severity::Error),
        add_ons,
        diagnostics,
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, a) in self.add_ons.iter().enumerate() {
            if i == 1 {
                writeln!(f, "Checked with what it needs:")?;
            }
            writeln!(f, "{} ({} {}) in {}", a.name, a.id, a.version, a.folder)?;
            let runs = match a.side {
                Side::Server => "the host only; players never download it",
                Side::Client => "each player's game; other players don't need it",
                Side::Shared => "everyone; players download it when they join",
            };
            writeln!(f, "  Runs on: {runs}")?;
            for p in &a.provides {
                writeln!(f, "  Provides {} {} ({})", p.kind, p.id, p.file)?;
            }
            for c in &a.capabilities {
                writeln!(f, "  Can {}", c.meaning)?;
            }
            for (id, requirement) in &a.dependencies {
                writeln!(f, "  Needs {id} {requirement}")?;
            }
        }
        for d in &self.diagnostics {
            writeln!(f, "{d}")?;
        }
        if self.ok {
            write!(f, "OK: the game can load this Add-On.")
        } else {
            write!(f, "Not loadable: fix the errors above.")
        }
    }
}
