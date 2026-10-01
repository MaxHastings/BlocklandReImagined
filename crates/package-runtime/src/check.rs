//! Check one Add-On folder the way the game will read it, and say what it
//! is: which side runs it, what it provides, what it may do and what it
//! needs. Dependencies are looked up among the folder's siblings, so a
//! folder of Add-Ons checks as a set. Nothing is run.
use crate::{
    Catalog,
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

/// The side an Add-On providing these kinds loads on, or None when it
/// mixes server behaviour with client visuals (which no side can load). The
/// same rule the Add-Ons screen uses.
pub fn side_for<'a>(kinds: impl IntoIterator<Item = &'a str>) -> Option<Side> {
    bri_package::library::side_for_kinds(kinds)
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
        let side = bri_package::library::side_for_package(
            manifest.provides.iter().map(|p| p.kind.as_str()),
            bri_package::library::CodeOwner::of(manifest.client.as_ref()),
        )
        .unwrap_or_else(|| {
            diagnostics.push(
                Diagnostic::error(
                    "check.mixed_sides",
                    format!("`{}` has both server behaviour and client visuals or code", manifest.id),
                )
                .at(format!("{}/{MANIFEST_FILE}", manifest.id))
                .hint("split it into two Add-Ons: one for the server rules, one for models, HUD panels and client code, the second depending on the first"),
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
    // Weapons are merged by the engine's content loading rather than the
    // catalog, from `assets/weapons.json` whether or not `provides` lists it.
    for (manifest, dir) in &found {
        let file = parent.join(dir).join("assets/weapons.json");
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let pack = match bri_weapons::Pack::from_json(&bytes) {
            Ok(pack) => pack,
            Err(e) => {
                diagnostics.push(
                    Diagnostic::error("check.weapons", format!("{e:#}"))
                        .at(format!("{}/assets/weapons.json", manifest.id))
                        .hint("compare it with packages/samples/sample-bubble-blaster/assets/weapons.json"),
                );
                continue;
            }
        };
        // The Add-On's own models (`*.shape.json` beside weapons.json) and
        // the PNG each of their materials names.
        let assets = parent.join(dir).join("assets");
        let models: std::collections::BTreeSet<&str> = pack
            .items
            .values()
            .map(|i| i.model.as_str())
            .chain(pack.images.values().map(|i| i.model.as_str()))
            .chain(pack.projectiles.values().map(|p| p.model.as_str()))
            .collect();
        let own = |m: &str| m.to_ascii_lowercase().ends_with(".shape.json");
        for model in models.iter().filter(|m| own(m)) {
            if let Some(problem) = own_model_problem(&assets, model) {
                diagnostics.push(
                    Diagnostic::warning("check.weapons.model", problem)
                        .at(format!("{}/assets/{model}", manifest.id))
                        .hint("an item's own model is a native model whose materials each name a PNG beside it; tools/make_trench_assets.py writes one"),
                );
            }
        }
        // Presentation is how items look. Without it the game still loads
        // the Add-On: items use their own models, or the stock models and
        // icons they name, and one the base game lacks shows no model and
        // its first letter.
        let borrows = models.iter().any(|m| !m.is_empty() && !own(m));
        if let Some(problem) = presentation_problem(&assets, &bytes).filter(|_| borrows) {
            diagnostics.push(
                Diagnostic::warning("check.weapons.presentation", problem)
                    .at(format!("{}/assets/presentation.json", manifest.id))
                    .hint("items then use the stock models and icons they name; run bri-import-addon to convert the Add-On's own art"),
            );
        }
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

/// Why an item's own model `model` (relative to `assets`) will not draw,
/// if it will not: missing, unreadable, or a material without its PNG.
fn own_model_problem(assets: &Path, model: &str) -> Option<String> {
    let file = assets.join(model);
    if model.contains("..") || !file.is_file() {
        return Some(format!("model {model} is not in the Add-On"));
    }
    let value: serde_json::Value = match std::fs::read(&file).map(|b| serde_json::from_slice(&b)) {
        Ok(Ok(value)) => value,
        Ok(Err(e)) => return Some(format!("model {model} does not read: {e}")),
        Err(e) => return Some(format!("model {model} does not read: {e}")),
    };
    let folder = file.parent().unwrap_or(assets);
    let materials = value.get("materials").and_then(|m| m.as_array());
    for name in materials.into_iter().flatten().filter_map(|m| m.get("name")?.as_str()) {
        if !folder.join(format!("{name}.png")).is_file() {
            return Some(format!("material {name} of {model} has no {name}.png beside it"));
        }
    }
    None
}

/// Why the item presentation beside a weapons pack will not be used, if
/// it will not: missing, unreadable, or written for other `weapons` bytes.
fn presentation_problem(assets: &Path, weapons: &[u8]) -> Option<String> {
    use sha2::Digest;
    let Ok(bytes) = std::fs::read(assets.join("presentation.json")) else {
        return Some("weapons.json has no presentation.json beside it".into());
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(e) => return Some(format!("presentation.json does not read: {e}")),
    };
    let expected = format!("{:x}", sha2::Sha256::digest(weapons));
    (value.get("weapons_sha256").and_then(|v| v.as_str()) != Some(expected.as_str()))
        .then(|| "presentation.json was made for a different weapons.json".into())
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
