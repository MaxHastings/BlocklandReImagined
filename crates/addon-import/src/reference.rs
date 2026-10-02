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
    /// The installed game's own bricks, by lower-case datablock name.
    pub base_bricks: std::collections::BTreeSet<String>,
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
    /// The installed game's item textures, by their lower-case source path
    /// (`base/data/shapes/black50.png`): an imported model may draw with
    /// them by that key, as its players' games already have them.
    pub item_textures: BTreeSet<String>,
    /// Every definition of a lower-case datablock name, base first, then
    /// Add-Ons in load order: several Add-Ons may declare one name.
    pub declared: BTreeMap<String, Vec<Owned>>,
    /// Lower-case Add-On name to the Add-Ons it loads first
    /// (`ForceRequiredAddOn`, `LoadRequiredAddOn`), lower-case.
    pub requires: BTreeMap<String, Vec<String>>,
}

/// The Add-Ons a script loads before its own datablocks
/// (`ForceRequiredAddOn("Weapon_Package_Tier1")`), lower-case.
pub fn required_addons(text: &str) -> Vec<String> {
    static CALL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?i)\b(?:force|load)requiredaddon\s*\(\s*"([^"]+)"\s*\)"#)
            .expect("pattern")
    });
    CALL.captures_iter(&tscript::without_comments(text))
        .map(|c| c[1].to_ascii_lowercase())
        .collect()
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
            let mut datablock = d.clone();
            crate::resolve_file_fields(&mut datablock, path);
            let owned = Owned {
                addon: addon.into(),
                path: path.into(),
                sha256: script.sha256.clone(),
                datablock,
            };
            let key = d.name.to_ascii_lowercase();
            self.datablocks
                .entry(key.clone())
                .or_insert_with(|| owned.clone());
            self.declared.entry(key).or_default().push(owned);
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

    /// Each datablock name several Add-Ons declare, as it stood when
    /// `addon` loaded: Add-Ons load in name order, each after the ones it
    /// requires, and only those before `addon` or required by it
    /// (`required`) have run. Declaring a name again sets its fields on
    /// the one datablock, so later fields win and the rest stay. A name
    /// none of those declare keeps its first declaration.
    pub fn settle_for(&mut self, addon: &str, required: &[String]) {
        let me = addon.to_ascii_lowercase();
        let mut order: Vec<String> = vec!["base".into()];
        fn visit(
            a: &str,
            requires: &BTreeMap<String, Vec<String>>,
            order: &mut Vec<String>,
            depth: u32,
        ) {
            if depth > 32 || order.iter().any(|o| o == a) {
                return;
            }
            for d in requires.get(a).into_iter().flatten() {
                visit(d, requires, order, depth + 1);
            }
            if !order.iter().any(|o| o == a) {
                order.push(a.to_owned());
            }
        }
        for a in self.addons.keys().filter(|a| **a < me) {
            visit(a, &self.requires, &mut order, 0);
        }
        for d in required {
            visit(&d.to_ascii_lowercase(), &self.requires, &mut order, 0);
        }
        for (name, all) in &self.declared {
            if all.len() < 2 {
                continue;
            }
            let mut loaded: Vec<(usize, &Owned)> = all
                .iter()
                .filter_map(|o| {
                    let at = order
                        .iter()
                        .position(|a| a.eq_ignore_ascii_case(&o.addon))?;
                    Some((at, o))
                })
                .collect();
            loaded.sort_by_key(|(at, _)| *at);
            let Some(((_, first), rest)) = loaded.split_first() else {
                continue;
            };
            let mut settled = (*first).clone();
            for (_, o) in rest {
                if !o
                    .datablock
                    .class
                    .eq_ignore_ascii_case(&settled.datablock.class)
                {
                    continue;
                }
                settled.datablock.fields.extend(
                    o.datablock
                        .fields
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone())),
                );
                if o.datablock.parent.is_some() {
                    settled.datablock.parent = o.datablock.parent.clone();
                }
                settled.addon = o.addon.clone();
                settled.path = o.path.clone();
                settled.sha256 = o.sha256.clone();
            }
            self.datablocks.insert(name.clone(), settled);
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
            let path = format!("base/server/scripts/{name} (recovered)");
            self.add_script("base", &text, &path);
            if let Ok(script) = tscript::read(&text, &path) {
                for g in &script.globals {
                    if let Some(v) = bri_convert::catalog::constant_global(
                        &g.value,
                        "base/server/scripts",
                        &self.globals,
                    ) {
                        self.globals.insert(g.name.to_ascii_lowercase(), v);
                    }
                }
            }
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
            r.add_addon(&path);
        }
        Ok(r)
    }

    /// Reads the Add-On at `path` (a folder or zip) into the reference, as
    /// one of its install's Add-Ons. False when it is neither or unreadable.
    pub fn add_addon(&mut self, path: &Path) -> bool {
        let is_zip = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        if !is_zip && !path.is_dir() {
            return false;
        }
        let Ok(src) = source::read(path) else {
            return false;
        };
        self.addons
            .insert(src.name.to_ascii_lowercase(), src.name.clone());
        for (key, file) in &src.files {
            self.files.insert(key.clone());
            if key.ends_with(".cs") {
                let text = String::from_utf8_lossy(&file.bytes);
                self.requires
                    .entry(src.name.to_ascii_lowercase())
                    .or_default()
                    .extend(required_addons(&text));
                self.add_script(&src.name, &text, &file.path);
            }
        }
        true
    }

    /// Adds the Add-Ons `required` names, and the ones they require in
    /// turn, from `folder` (the one the imported copy is in, as v20's
    /// Add-Ons folder held them side by side) where the reference lacks
    /// them.
    pub fn add_beside(&mut self, folder: &Path, required: &[String]) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        let near: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
        let mut wanted: Vec<String> = required.iter().map(|a| a.to_ascii_lowercase()).collect();
        let mut tried = std::collections::BTreeSet::new();
        while let Some(a) = wanted.pop() {
            if self.addons.contains_key(&a) || !tried.insert(a.clone()) {
                continue;
            }
            let found = near.iter().find(|p| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_ascii_lowercase());
                name.as_deref() == Some(a.as_str()) || name == Some(format!("{a}.zip"))
            });
            if let Some(path) = found
                && self.add_addon(path)
            {
                wanted.extend(
                    self.requires
                        .get(&a)
                        .into_iter()
                        .flatten()
                        .map(|r| r.to_ascii_lowercase()),
                );
            }
        }
    }

    /// The base game's sound that plays `file` (`base/data/sound/clickMove.wav`
    /// is `clickMoveSound`): an Add-On's own profile of a base file plays
    /// the same sound, which the game already has by that name.
    pub fn base_sound(&self, file: &str) -> Option<&str> {
        self.datablocks
            .values()
            .filter(|o| o.addon == "base")
            .map(|o| &o.datablock)
            .filter(|d| d.class.eq_ignore_ascii_case("AudioProfile"))
            .find(|d| {
                d.fields
                    .get("filename")
                    .is_some_and(|f| crate::literal(f).eq_ignore_ascii_case(file))
            })
            .map(|d| d.name.as_str())
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
    /// The installed game's texture for a model material an Add-On does
    /// not carry: v20's stock materials (`blank`, `black50`, `gray75` and
    /// the rest in `base/data/shapes`) are what Add-On models lean on when
    /// they ship no copy, as Loz's Hookshot does. The key is the one
    /// players' games already load it by, so nothing is copied.
    /// The installed game's sound file at the v20 path `path`
    /// (`base/data/sound/vehicleExplosion.wav`, or a default Add-On's), in
    /// the spelling the game's sound bank finds it by: an Add-On's
    /// `AudioProfile` may play it without shipping a copy.
    pub fn stock_sound(&self, path: &str) -> Option<String> {
        let key = path.trim().replace('\\', "/").to_ascii_lowercase();
        ((key.ends_with(".wav") || key.ends_with(".ogg")) && self.files.contains(&key))
            .then_some(key)
    }
    pub fn base_texture(&self, material: &str) -> Option<String> {
        ["", ".png", ".jpg", ".jpeg"]
            .iter()
            .map(|ext| format!("base/data/shapes/{material}{ext}").to_ascii_lowercase())
            .find(|key| self.item_textures.contains(key))
    }

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
        if let Some(dir) = dir("item_presentation") {
            let path = dir.join("presentation.json");
            let manifest: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
            )
            .with_context(|| path.display().to_string())?;
            if let Some(textures) = manifest["textures"].as_object() {
                self.item_textures
                    .extend(textures.keys().map(|k| k.to_ascii_lowercase()));
            }
        }
        if let Some(dir) = dir("audio") {
            let path = dir.join("manifest.json");
            let manifest: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
            )?;
            // Its clips by the v20 paths they were read from, for Add-On
            // profiles that name one.
            for c in manifest["clips"].as_array().into_iter().flatten() {
                for source in c["sources"].as_array().into_iter().flatten() {
                    if let Some(path) = source["virtual_path"].as_str() {
                        self.files
                            .insert(path.replace('\\', "/").to_ascii_lowercase());
                    }
                }
            }
            for s in manifest["sounds"].as_array().into_iter().flatten() {
                let (Some(name), Some(addon)) = (s["name"].as_str(), s["package"].as_str()) else {
                    continue;
                };
                // Its clip's id carries the file it plays
                // (`v20/clip/base/data/sound/clickMove.wav`).
                let fields = s["clip"]
                    .as_str()
                    .and_then(|c| c.strip_prefix("v20/clip/"))
                    .map(|file| BTreeMap::from([("filename".to_owned(), quoted(file))]))
                    .unwrap_or_default();
                self.installed_datablock(addon, "", "AudioProfile".into(), name, None, fields);
            }
        }
        if let Some(dir) = dir("effects") {
            let path = dir.join("effects.json");
            let library: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
            )?;
            // The base game's particle textures, by the key particles name
            // them (`base/data/particles/dot`).
            if let Some(textures) = library["textures"].as_object() {
                self.files
                    .extend(textures.keys().map(|k| k.to_ascii_lowercase()));
            }
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
            self.base_bricks.insert(name.to_ascii_lowercase());
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
