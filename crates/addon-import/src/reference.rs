//! What the base game already provides: datablocks, damage types and files
//! of a read-only v20 reference install, so an Add-On's references to them
//! become dependencies instead of unknowns.
use crate::source;
use anyhow::{Context, Result};
use bri_convert::tscript::{self, Datablock};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone)]
pub struct Owned {
    /// Add-On folder name, or `base` for the core scripts.
    pub addon: String,
    pub path: String,
    pub sha256: String,
    pub datablock: Datablock,
}

#[derive(Debug, Default)]
pub struct Reference {
    pub root: Option<String>,
    /// Recovered core scripts were given, so base datablocks are known.
    pub has_core: bool,
    /// Lower-case datablock name.
    pub datablocks: BTreeMap<String, Owned>,
    /// Lower-case `$DamageType::` names.
    pub damage_types: BTreeSet<String>,
    /// Lower-case function names (`gunimage::onfire`) to their Add-On.
    pub functions: BTreeMap<String, String>,
    /// Lower-case virtual paths (`add-ons/weapon_gun/bullet.dts`, `base/data/...`).
    pub files: BTreeSet<String>,
    /// Lower-case Add-On name to its spelling.
    pub addons: BTreeMap<String, String>,
    /// Globals the core scripts set to constants (`$backslot` → `4`), by
    /// lower-case name with its `$`: Add-On fields name them
    /// (`mountPoint = $BackSlot;`).
    pub globals: bri_convert::catalog::Globals,
}

/// The base-game package that ships a vanilla Add-On's content
/// (`crates/package/base-packages.json`). `base` spans several packages.
pub fn base_package(addon: &str) -> Option<&'static str> {
    let a = addon.to_ascii_lowercase();
    Some(match a.split('_').next().unwrap_or("") {
        "weapon" | "item" | "projectile" | "emote" => "v20-weapons",
        "vehicle" => "v20-vehicles",
        "brick" | "print" => "v20-bricks",
        "map" => "v20-map-bundle",
        "sound" | "music" => "v20-audio",
        "particle" => "v20-effects",
        _ => return None,
    })
}

impl Reference {
    pub fn add_script(&mut self, addon: &str, text: &str, path: &str) {
        let Ok(script) = tscript::read(text, path) else {
            return;
        };
        for d in &script.datablocks {
            self.datablocks
                .entry(d.name.to_ascii_lowercase())
                .or_insert_with(|| {
                    let mut datablock = d.clone();
                    crate::resolve_file_fields(&mut datablock, path);
                    Owned {
                        addon: addon.into(),
                        path: path.into(),
                        sha256: script.sha256.clone(),
                        datablock,
                    }
                });
        }
        for f in &script.functions {
            self.functions
                .insert(f.qualified().to_ascii_lowercase(), addon.into());
        }
        if let Ok(types) = bri_weapons_import::damage_types(text) {
            self.damage_types
                .extend(types.into_iter().map(|t| t.name.to_ascii_lowercase()));
        }
    }

    pub fn load(root: &Path, core: &[std::path::PathBuf]) -> Result<Self> {
        let mut r = Self {
            root: Some(root.display().to_string()),
            has_core: !core.is_empty(),
            ..Self::default()
        };
        for core in core {
            let text = std::fs::read_to_string(core)
                .with_context(|| format!("reading core script {}", core.display()))?;
            let name = core
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
            let path = format!("base/server/scripts/{name} (recovered)");
            r.add_script("base", &text, &path);
            if let Ok(script) = tscript::read(&text, &path) {
                for g in &script.globals {
                    if let Some(v) =
                        bri_convert::catalog::constant_global(&g.value, "base/server/scripts", &r.globals)
                    {
                        r.globals.insert(g.name.to_ascii_lowercase(), v);
                    }
                }
            }
        }
        let mut stack = vec![root.join("base")];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(rel) = p.strip_prefix(root) {
                    r.files.insert(
                        rel.to_string_lossy()
                            .replace('\\', "/")
                            .to_ascii_lowercase(),
                    );
                }
            }
        }
        let mut addons: Vec<_> = std::fs::read_dir(root.join("Add-Ons"))
            .context("reference install has no Add-Ons folder")?
            .flatten()
            .map(|e| e.path())
            .collect();
        addons.sort();
        for path in addons {
            let is_zip = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
            if !is_zip && !path.is_dir() {
                continue;
            }
            let Ok(src) = source::read(&path) else {
                continue;
            };
            r.addons
                .insert(src.name.to_ascii_lowercase(), src.name.clone());
            for (key, file) in &src.files {
                r.files.insert(key.clone());
                if key.ends_with(".cs") {
                    r.add_script(&src.name, &String::from_utf8_lossy(&file.bytes), &file.path);
                }
            }
        }
        Ok(r)
    }

    /// Finds a file with Torque's implicit extensions (`iconName`, textures).
    pub fn has_file(&self, path: &str) -> Option<String> {
        let p = path.to_ascii_lowercase();
        ["", ".png", ".jpg", ".dts", ".wav", ".ogg", ".blb"]
            .iter()
            .map(|e| format!("{p}{e}"))
            .find(|c| self.files.contains(c))
    }

    /// The Add-On a root-relative path belongs to (`add-ons/weapon_gun/x` → `Weapon_Gun`).
    pub fn addon_of(&self, path: &str) -> Option<String> {
        let p = path.to_ascii_lowercase();
        if p.starts_with("base/") {
            return Some("base".into());
        }
        let rest = p.strip_prefix("add-ons/")?;
        let name = rest.split('/').next()?;
        Some(
            self.addons
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.to_owned()),
        )
    }
}
