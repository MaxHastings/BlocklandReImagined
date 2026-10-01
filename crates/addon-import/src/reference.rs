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
            r.add_script(
                "base",
                &text,
                &format!("base/server/scripts/{name} (recovered)"),
            );
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
