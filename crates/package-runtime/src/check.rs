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
    pub optional_dependencies: BTreeMap<String, String>,
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
                // An optional one beside it is checked with it; a missing
                // one is fine.
                for dependency in manifest.optional_dependencies.keys() {
                    if !bri_package::id::is_reserved(dependency)
                        && siblings.contains_key(dependency)
                    {
                        queue.push_back(dependency.clone());
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
            optional_dependencies: manifest.optional_dependencies.clone(),
        });
    }
    // As the host loads it (and compiles its scripts), then as a player does.
    match Catalog::load(&parent, &set, true) {
        Ok(catalog) => match Runtime::compile(&catalog) {
            Ok(runtime) => diagnostics.extend(runtime.unknown_calls()),
            Err(mut problems) => diagnostics.append(&mut problems),
        },
        Err(mut problems) => diagnostics.append(&mut problems),
    }
    if let Err(mut problems) = Catalog::load(&parent, &set, false) {
        diagnostics.append(&mut problems);
    }
    // Weapons are merged by the engine's content loading rather than the
    // catalog, from `assets/weapons.json` whether or not `provides` lists it.
    // Every sound the set declares, lower case, and whether it loops.
    let mut sounds = BTreeMap::new();
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
        sounds.extend(
            pack.sounds
                .iter()
                .map(|(key, def)| (key.to_ascii_lowercase(), def.looping)),
        );
        diagnostics.extend(crate::timing::weapon_timing(
            &pack,
            &format!("{}/assets/weapons.json", manifest.id),
        ));
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
                        .hint("an item's own model is a native model whose materials each name a PNG beside it"),
                );
            }
        }
        // Each sound file it ships, through the game's own decoder: a file
        // the game cannot decode loads with the Add-On but plays silence.
        // (A `stock` sound is the installed game's copy, not the Add-On's.)
        for (profile, def) in pack.sounds.iter().filter(|(_, d)| !d.stock) {
            let loaded =
                bri_package::path::inside(&bri_weapons::sound_root(&assets, def), &def.file)
                    .and_then(|path| {
                        bri_audio::bank::SoundAsset::decoded_file(profile, &path, 1.0, 2.0)
                    });
            if let Err(e) = loaded {
                diagnostics.push(
                    Diagnostic::error(
                        "check.sound",
                        format!("sound {profile} will not play: {e}"),
                    )
                    .at(format!("{}/assets/{}", manifest.id, def.file))
                    .hint("the game plays PCM WAV and Ogg Vorbis files of up to 30 seconds and 8 MiB; re-export it as 16-bit PCM WAV"),
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
    // Each vehicle's engine sound, against the sounds the set declares.
    for (manifest, dir) in &found {
        for provided in manifest.provides.iter().filter(|p| p.kind == "vehicles") {
            let Ok(bytes) = std::fs::read(parent.join(dir).join(&provided.file)) else {
                continue;
            };
            diagnostics.extend(engine_sound_problems(
                &bytes,
                &sounds,
                &format!("{}/{}", manifest.id, provided.file),
            ));
        }
    }
    // Each map, the way the game will load it: scene, models, textures,
    // spawn points and lights.
    for (manifest, dir) in &found {
        for provided in manifest.provides.iter().filter(|p| p.kind == "map") {
            let bundle = parent.join(dir).join(&provided.file);
            let at = format!("{}/{}", manifest.id, provided.file);
            diagnostics.extend(map_problems(&bundle, &provided.id, &at));
        }
    }
    // The state each behaviour declares, against the limits it will meet.
    for (manifest, dir) in &found {
        for provided in manifest.provides.iter().filter(|p| p.kind == "behaviour") {
            let Ok(bytes) = std::fs::read(parent.join(dir).join(&provided.file)) else {
                continue;
            };
            if let Ok(behaviour) = serde_json::from_slice::<crate::content::Behaviour>(&bytes) {
                diagnostics.extend(crate::timing::state_budget(&manifest.id, &behaviour));
            }
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

/// What is wrong with the engine sounds a vehicles pack (`bytes`, at
/// `at`) names: an Add-On sound (a namespaced key) that no weapons pack in
/// the set declares, or one that does not loop. A key without a namespace
/// is one of the game's own sounds.
fn engine_sound_problems(
    bytes: &[u8],
    sounds: &BTreeMap<String, bool>,
    at: &str,
) -> Vec<Diagnostic> {
    let Ok(pack) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return vec![];
    };
    let definitions = pack.get("definitions").and_then(|d| d.as_array());
    let mut problems = Vec::new();
    for d in definitions.into_iter().flatten() {
        let Some(sound) = d
            .get("engine_sound")
            .and_then(|e| e.get("sound"))
            .and_then(|s| s.as_str())
        else {
            continue;
        };
        let vehicle = d.get("id").and_then(|i| i.as_str()).unwrap_or("?");
        match sounds.get(&sound.to_ascii_lowercase()) {
            Some(true) => {}
            Some(false) => problems.push(
                Diagnostic::warning(
                    "check.vehicles.engine_sound",
                    format!("{vehicle}'s engine sound {sound} plays once instead of looping"),
                )
                .at(at.to_owned())
                .hint("declare it with \"looping\": true in the weapons pack's sounds"),
            ),
            None if sound.contains(':') => problems.push(
                Diagnostic::error(
                    "check.vehicles.engine_sound",
                    format!("{vehicle}'s engine sound {sound} is not declared"),
                )
                .at(at.to_owned())
                .hint("add it to the sounds of the Add-On's assets/weapons.json, with \"looping\": true"),
            ),
            None => {}
        }
    }
    problems
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
    for name in materials
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("name")?.as_str())
    {
        if !folder.join(format!("{name}.png")).is_file() {
            return Some(format!(
                "material {name} of {model} has no {name}.png beside it"
            ));
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
            for (id, requirement) in &a.optional_dependencies {
                writeln!(f, "  Uses {id} {requirement} when it is on")?;
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

/// What keeps the map in `bundle` (a map Add-On's `bundle.json`, `at` for
/// messages) from loading or playing as its author expects. The index
/// itself is checked with the rest of the Add-On (`content.map`); this
/// follows what it names.
fn map_problems(bundle: &Path, id: &str, at: &str) -> Vec<Diagnostic> {
    use bri_content::scene::{Kind, Scene};
    const GUIDE: &str =
        "see docs/modding/maps.md, or reimport the map with bri-import-addon gltf --kind map";
    let mut out = Vec::new();
    let Some(root) = bundle.parent() else {
        return out;
    };
    let Ok(index) = std::fs::read(bundle)
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).map_err(|e| e.to_string()))
    else {
        return out;
    };
    // Bundle files sit flat beside bundle.json.
    let file = |name: &str| {
        (!name.is_empty() && !name.contains(['/', '\\']) && name != "..")
            .then(|| root.join(name))
            .filter(|p| p.is_file())
    };
    let map = index["maps"]
        .as_array()
        .and_then(|m| m.iter().find(|m| m["id"].as_str() == Some(id)));
    let Some(scene_file) = map.and_then(|m| m["file"].as_str()) else {
        return out;
    };
    let Some(scene_path) = file(scene_file) else {
        out.push(
            Diagnostic::error(
                "check.map",
                format!("the map's scene {scene_file} is not beside bundle.json"),
            )
            .at(at)
            .hint(GUIDE),
        );
        return out;
    };
    let scene: Scene = match std::fs::read(&scene_path)
        .map_err(anyhow::Error::from)
        .and_then(|b| Ok(serde_json::from_slice(&b)?))
    {
        Ok(scene) => scene,
        Err(e) => {
            out.push(
                Diagnostic::error(
                    "check.map",
                    format!("scene {scene_file} does not read: {e:#}"),
                )
                .at(at)
                .hint(GUIDE),
            );
            return out;
        }
    };
    let scene_at = format!("{at} → {scene_file}");
    if scene.id != id {
        out.push(
            Diagnostic::error(
                "check.map",
                format!(
                    "scene {scene_file} is map {}, but bundle.json says {id}",
                    scene.id
                ),
            )
            .at(&scene_at),
        );
    }
    if !scene.nodes.iter().any(|n| matches!(n.kind, Kind::Spawn)) {
        out.push(
            Diagnostic::error("check.map", "the map has no spawn point, so players have nowhere to start")
                .at(&scene_at)
                .hint("in Blender, add an Empty named spawn where players start (spawn.001 and so on for more), then reimport"),
        );
    }
    match scene.lights() {
        Ok(lights) => {
            let animated: Vec<_> = lights
                .iter()
                .filter(|l| l.kind != "omni")
                .map(|l| format!("{} ({})", scene.nodes[l.node].name, l.kind))
                .collect();
            if !animated.is_empty() {
                out.push(
                    Diagnostic::warning(
                        "check.map.light",
                        format!("{} keep their type, but draw as steady omni lights for now", animated.join(", ")),
                    )
                    .at(&scene_at)
                    .hint("nothing to change: they play as their type once the engine supports map light animation and spot cones"),
                );
            }
        }
        Err(e) => out.push(
            Diagnostic::error("check.map.light", format!("{e:#}"))
                .at(&scene_at)
                .hint("a light's bri_light_type is omni, spot, strobe, pulse, pulse2, flicker or runway, and its radius is more than bri_inner; see docs/modding/maps.md"),
        ),
    }
    // Every model the scene places, its collision, and every texture.
    let assets = index["assets"].as_object();
    let textures = index["textures"].as_object();
    for node in scene
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::StaticModel))
    {
        let Some(asset) = node.asset.as_deref() else {
            out.push(
                Diagnostic::error(
                    "check.map",
                    format!("model node {} names no model", node.name),
                )
                .at(&scene_at),
            );
            continue;
        };
        let Some(model_file) = assets.and_then(|a| a.get(asset)).and_then(|f| f.as_str()) else {
            out.push(
                Diagnostic::error(
                    "check.map",
                    format!("model {asset} is not in bundle.json's assets"),
                )
                .at(at)
                .hint(GUIDE),
            );
            continue;
        };
        let shape = file(model_file)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<bri_content::shape::Shape>(&b).ok());
        let Some(shape) = shape else {
            out.push(
                Diagnostic::error(
                    "check.map",
                    format!("model {model_file} is missing or does not read"),
                )
                .at(at)
                .hint(GUIDE),
            );
            continue;
        };
        if !shape.details.iter().any(|d| d.collision) {
            out.push(
                Diagnostic::warning("check.map", format!("model {model_file} has no collision: players fall through it"))
                    .at(at)
                    .hint("name the meshes players stand on col_..., or leave out col_ meshes so everything drawn is solid, then reimport"),
            );
        }
    }
    for binding in index["bindings"].as_array().into_iter().flatten() {
        let Some(texture) = binding["texture"].as_str() else {
            continue;
        };
        let found = textures
            .and_then(|t| t.get(texture))
            .and_then(|f| f.as_str())
            .and_then(file);
        if found.is_none() {
            out.push(
                Diagnostic::error(
                    "check.map",
                    format!("texture {texture} is missing beside bundle.json"),
                )
                .at(at)
                .hint(GUIDE),
            );
        }
    }
    out
}

#[cfg(test)]
mod map_tests {
    use super::map_problems;

    fn write(dir: &std::path::Path, scene: serde_json::Value) -> std::path::PathBuf {
        std::fs::write(
            dir.join("m.scene.json"),
            serde_json::to_vec(&scene).unwrap(),
        )
        .unwrap();
        let bundle = dir.join("bundle.json");
        std::fs::write(
            &bundle,
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "maps": [{ "id": "a:map/m", "name": "M", "file": "m.scene.json" }],
                "assets": { "a:map/m/model": "m.shape.json" },
                "textures": {},
                "bindings": [{ "asset": "a:map/m/model", "shape_material": 0, "skin": "", "texture": "wood.png" }],
            }))
            .unwrap(),
        )
        .unwrap();
        bundle
    }
    fn node(kind: &str, asset: Option<&str>, properties: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "name": kind, "parent": null, "kind": kind,
            "transform": [1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, 0.0,0.0,0.0,1.0],
            "asset": asset, "properties": properties })
    }

    #[test]
    fn map_problems_name_what_to_fix() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = write(
            dir.path(),
            serde_json::json!({ "schema_version": 1, "id": "a:map/m", "name": "M", "nodes": [
                node("static_model", Some("a:map/m/model"), serde_json::json!({})),
                node("light", None, serde_json::json!({ "type": "disco" })),
            ] }),
        );
        let found = map_problems(&bundle, "a:map/m", "a/map/bundle.json");
        let text: Vec<String> = found
            .iter()
            .map(|d| format!("{} {}", d.message, d.hint.clone().unwrap_or_default()))
            .collect();
        let has = |s: &str| text.iter().any(|t| t.contains(s));
        assert!(
            has("no spawn point") && has("Empty named spawn"),
            "{text:#?}"
        );
        assert!(has("disco"), "{text:#?}");
        assert!(has("m.shape.json is missing"), "{text:#?}");
        assert!(has("texture wood.png is missing"), "{text:#?}");
    }
}
