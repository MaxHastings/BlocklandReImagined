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
    /// The installed game's content was read (`--installed`): base
    /// datablocks are known from its brick catalog, weapons, sounds and
    /// effects.
    pub installed: bool,
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
}

/// The Add-Ons v20 shipped with (`docs/vanilla-reference-inventory.json`).
/// Only these are the base game's; any other Add-On, whatever its prefix
/// (`Weapon_Tier1`), is a community one the player imports themselves.
pub const VANILLA: &[&str] = &[
    "Brick_Arch",
    "Brick_Checkpoint",
    "Brick_Christmas_Tree",
    "Brick_Halloween",
    "Brick_Large_Cubes",
    "Brick_Teledoor",
    "Brick_Treasure_Chest",
    "Brick_V15",
    "Decal_Default",
    "Decal_Hoodie",
    "Decal_Jirue",
    "Decal_WORM",
    "Emote_Alarm",
    "Emote_Confusion",
    "Emote_Hate",
    "Emote_Love",
    "Face_Default",
    "Face_Jirue",
    "Face_Mythbusters",
    "Item_Key",
    "Item_Skis",
    "Item_Sports",
    "Light_Animated",
    "Light_Basic",
    "Map_Bedroom",
    "Map_BedroomDark",
    "Map_Construct",
    "Map_Destruct",
    "Map_Halloween_Slate",
    "Map_Kitchen",
    "Map_KitchenDark",
    "Map_Skylands",
    "Map_Slate",
    "Map_Slate_Desert",
    "Map_Slate_Sea_Revised",
    "Map_Slate_Storm_Revised",
    "Map_Slopes",
    "Map_Tutorial",
    "Particle_Basic",
    "Particle_FX_Cans",
    "Particle_Grass",
    "Particle_Player",
    "Particle_Tools",
    "Player_Fuel_Jet",
    "Player_Jump_Jet",
    "Player_Leap_Jet",
    "Player_No_Jet",
    "Player_Quake",
    "Print_1x2f_BLPRemote",
    "Print_1x2f_Default",
    "Print_2x2f_Default",
    "Print_2x2r_Default",
    "Print_2x2r_Monitor3",
    "Print_Letters_Default",
    "Projectile_GravityRocket",
    "Projectile_Pinball",
    "Projectile_Pong",
    "Projectile_Radio_Wave",
    "Script_Player_Persistence",
    "Sound_Beeps",
    "Sound_Phone",
    "Sound_Synth4",
    "System_ReturnToBlockland",
    "Vehicle_Ball",
    "Vehicle_Flying_Wheeled_Jeep",
    "Vehicle_Horse",
    "Vehicle_Jeep",
    "Vehicle_Magic_Carpet",
    "Vehicle_Pirate_Cannon",
    "Vehicle_Rowboat",
    "Vehicle_Tank",
    "Weapon_Bow",
    "Weapon_Gun",
    "Weapon_Guns_Akimbo",
    "Weapon_Horse_Ray",
    "Weapon_Push_Broom",
    "Weapon_Rocket_Launcher",
    "Weapon_Spear",
    "Weapon_Sword",
];

/// The base-game package that ships a vanilla Add-On's content
/// (`crates/package/base-packages.json`). `base` spans several packages.
/// `None` for an Add-On v20 did not ship.
pub fn base_package(addon: &str) -> Option<&'static str> {
    if !VANILLA.iter().any(|v| v.eq_ignore_ascii_case(addon)) {
        return None;
    }
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

    /// Base datablocks are known, from recovered core scripts or the
    /// installed game.
    pub fn knows_base(&self) -> bool {
        self.has_core || self.installed
    }

    /// Recovered core scripts only, with no reference install.
    pub fn core_only(core: &[std::path::PathBuf]) -> Result<Self> {
        let mut r = Self::default();
        r.add_core(core)?;
        Ok(r)
    }

    fn add_core(&mut self, core: &[std::path::PathBuf]) -> Result<()> {
        self.has_core |= !core.is_empty();
        for core in core {
            let text = std::fs::read_to_string(core)
                .with_context(|| format!("reading core script {}", core.display()))?;
            let name = core
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
            self.add_script(
                "base",
                &text,
                &format!("base/server/scripts/{name} (recovered)"),
            );
        }
        Ok(())
    }

    pub fn load(root: &Path, core: &[std::path::PathBuf]) -> Result<Self> {
        let mut r = Self {
            root: Some(root.display().to_string()),
            ..Self::default()
        };
        r.add_core(core)?;
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

/// A field value as script source: a quoted string.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The Add-On a v20 source path belongs to: `Add-Ons/Weapon_Gun/server.cs`
/// is `Weapon_Gun`, anything under `base/` is `base`.
fn addon_of_source(path: &str) -> String {
    let p = path.replace('\\', "/");
    let lower = p.to_ascii_lowercase();
    match lower.strip_prefix("add-ons/") {
        Some(_) => p["add-ons/".len()..]
            .split('/')
            .next()
            .unwrap_or("base")
            .to_owned(),
        None => "base".into(),
    }
}

impl Reference {
    /// Adds what the installed game provides, from its content root: the
    /// stock brick catalog's bricks (with their fields, so an Add-On brick
    /// can inherit from `brick1x1Data`), the weapons pack's datablocks, the
    /// sound profiles and the effects' particles and emitters. Packs the
    /// game does not have are skipped. Only names and declared fields are
    /// read; nothing of the base game is copied into an import.
    pub fn add_installed(&mut self, content: &Path) -> Result<()> {
        let set = bri_package::packages::PackageSet::load_root(content)
            .with_context(|| format!("reading the installed game at {}", content.display()))?;
        self.installed = true;
        let dir = |role: &str| set.role_dir(content, role).ok();
        if let Some(dir) = dir("brick_catalog") {
            self.add_installed_bricks(&dir.join("stock-catalog.json"))?;
        }
        if let Some(dir) = dir("weapons") {
            let path = dir.join("weapons.json");
            let pack: bri_weapons::Pack = serde_json::from_slice(
                &std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
            )
            .with_context(|| path.display().to_string())?;
            for d in pack.definitions {
                let addon = addon_of_source(&d.source.path);
                self.installed_datablock(
                    &addon,
                    &d.source.path,
                    d.class,
                    &d.name,
                    d.parent,
                    d.fields,
                );
            }
            for r in pack.resources {
                self.files.insert(r.path.to_ascii_lowercase());
            }
        }
        if let Some(dir) = dir("audio") {
            let path = dir.join("manifest.json");
            let manifest: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
            )?;
            for s in manifest["sounds"].as_array().into_iter().flatten() {
                let (Some(name), Some(addon)) = (s["name"].as_str(), s["package"].as_str()) else {
                    continue;
                };
                self.installed_datablock(
                    addon,
                    "",
                    "AudioProfile".into(),
                    name,
                    None,
                    BTreeMap::new(),
                );
            }
        }
        if let Some(dir) = dir("effects") {
            let path = dir.join("effects.json");
            let library: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
            )?;
            for (key, class) in [
                ("particles", "ParticleData"),
                ("emitters", "ParticleEmitterData"),
            ] {
                let entries: Vec<&serde_json::Value> = match &library[key] {
                    serde_json::Value::Array(a) => a.iter().collect(),
                    serde_json::Value::Object(m) => m.values().collect(),
                    _ => vec![],
                };
                for e in entries {
                    let Some(name) = e["name"]
                        .as_str()
                        .or_else(|| e["id"].as_str().and_then(|i| i.rsplit('/').next()))
                    else {
                        continue;
                    };
                    self.installed_datablock("base", "", class.into(), name, None, BTreeMap::new());
                }
            }
        }
        Ok(())
    }

    fn add_installed_bricks(&mut self, path: &Path) -> Result<()> {
        let catalog: bri_content::brick::Catalog = serde_json::from_slice(
            &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
        )
        .with_context(|| path.display().to_string())?;
        for b in catalog.bricks {
            let (Some(name), Some(mesh)) = (
                b.id.strip_prefix("v20/brick/"),
                b.mesh_id.strip_prefix("v20/"),
            ) else {
                continue;
            };
            self.files.insert(mesh.to_ascii_lowercase());
            let addon = addon_of_source(mesh);
            let mut fields = BTreeMap::new();
            fields.insert("brickfile".into(), quoted(mesh));
            fields.insert("uiname".into(), quoted(&b.display_name));
            fields.insert("category".into(), quoted(&b.category));
            fields.insert("subcategory".into(), quoted(&b.subcategory));
            if !b.icon_source.is_empty() {
                self.files
                    .insert(format!("{}.png", b.icon_source.to_ascii_lowercase()));
                fields.insert("iconname".into(), quoted(&b.icon_source));
            }
            if let Some(c) = &b.collision_source {
                fields.insert("collisionshapename".into(), quoted(c));
            }
            if let Some(a) = &b.print_aspect_ratio {
                fields.insert("printaspectratio".into(), quoted(a));
            }
            if let Some(k) = &b.special_kind {
                fields.insert("specialbricktype".into(), quoted(k));
            }
            fields.insert("orientationfix".into(), b.orientation_fix.to_string());
            fields.insert("cancover".into(), u8::from(b.can_cover).to_string());
            fields.insert(
                "indestructable".into(),
                u8::from(b.indestructible).to_string(),
            );
            fields.extend(b.other_properties);
            self.installed_datablock(&addon, mesh, "fxDTSBrickData".into(), name, None, fields);
        }
        Ok(())
    }

    fn installed_datablock(
        &mut self,
        addon: &str,
        path: &str,
        class: String,
        name: &str,
        parent: Option<String>,
        fields: BTreeMap<String, String>,
    ) {
        if name.is_empty() {
            return;
        }
        if addon != "base" {
            self.addons
                .entry(addon.to_ascii_lowercase())
                .or_insert_with(|| addon.to_owned());
        }
        // A reference install or core script already declaring it wins: it
        // is the source the installed content was converted from.
        self.datablocks
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| Owned {
                addon: addon.into(),
                path: format!("{path} (installed)"),
                sha256: String::new(),
                datablock: Datablock {
                    class,
                    name: name.into(),
                    parent,
                    fields,
                    line: 0,
                },
            });
    }
}
