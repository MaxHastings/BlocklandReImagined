//! Native named avatar rig. Source constructors are resolved by the offline tool.
use crate::shape::{Animation, Shape};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rig {
    pub schema_version: u32,
    pub id: String,
    pub shape: Shape,
    /// Case-normalized gameplay aliases, not the possibly repeated embedded name.
    pub sequences: BTreeMap<String, Animation>,
    pub sources: Vec<Source>,
    pub omissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub alias: String,
    pub virtual_path: String,
    pub source_sha256: String,
    pub native_sha256: String,
    pub constructor_line: usize,
}

/// A player's avatar, as saved and sent. Parts are named by the part chosen
/// (`hat: "helmet"`, `accent: "visor"`), never by position in the pack's
/// lists, so adding or reordering parts keeps everyone's avatar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    pub parts: BTreeMap<String, String>,
    pub colors: BTreeMap<String, [f32; 4]>,
    pub face: String,
    pub decal: String,
}
impl Appearance {
    pub fn validate_bounds(&self) -> Result<()> {
        ensure!(
            self.parts.len() <= 12
                && self.colors.len() <= 13
                && self.face.len() <= 256
                && self.decal.len() <= 256
                && self.parts.iter().all(|(k, v)| {
                    k.len() <= 32 && !v.is_empty() && v.len() <= 64 && v.is_ascii()
                })
                && self.colors.iter().all(|(k, c)| k.len() <= 32
                    && c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))),
            "Invalid avatar appearance bounds"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Texture {
    pub file: String,
    pub sha256: String,
    pub source: String,
    pub width: u32,
    pub height: u32,
}

/// The avatar pack. Its file stores the default appearance the way v20's
/// prefs do, as positions in its own part lists; loading names them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "PackageFile", into = "PackageFile")]
pub struct Package {
    pub schema_version: u32,
    pub id: String,
    pub rig: String,
    pub rig_sha256: String,
    pub parts: BTreeMap<String, Vec<String>>,
    pub accents_allowed: BTreeMap<String, Vec<String>>,
    pub faces: Vec<String>,
    pub decals: Vec<String>,
    pub surfaces: BTreeMap<String, String>,
    pub textures: BTreeMap<String, Texture>,
    pub defaults: Appearance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageFile {
    schema_version: u32,
    id: String,
    rig: String,
    rig_sha256: String,
    parts: BTreeMap<String, Vec<String>>,
    accents_allowed: BTreeMap<String, Vec<String>>,
    faces: Vec<String>,
    decals: Vec<String>,
    surfaces: BTreeMap<String, String>,
    textures: BTreeMap<String, Texture>,
    defaults: PackDefaults,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackDefaults {
    parts: BTreeMap<String, usize>,
    colors: BTreeMap<String, [f32; 4]>,
    face: String,
    decal: String,
}
impl TryFrom<PackageFile> for Package {
    type Error = anyhow::Error;
    fn try_from(file: PackageFile) -> Result<Self> {
        let hat = file
            .defaults
            .parts
            .get("hat")
            .and_then(|i| file.parts.get("hat")?.get(*i))
            .map(|h| h.to_ascii_lowercase())
            .unwrap_or_default();
        let mut parts = BTreeMap::new();
        for (slot, index) in &file.defaults.parts {
            let choices = if slot == "accent" {
                file.accents_allowed.get(&hat)
            } else {
                file.parts.get(slot)
            };
            let name = match choices.and_then(|c| c.get(*index)) {
                Some(name) => name.to_ascii_lowercase(),
                None if slot == "accent" && *index == 0 => "none".into(),
                None => anyhow::bail!("Avatar pack default {slot}:{index} names no part"),
            };
            parts.insert(slot.clone(), name);
        }
        Ok(Self {
            schema_version: file.schema_version,
            id: file.id,
            rig: file.rig,
            rig_sha256: file.rig_sha256,
            parts: file.parts,
            accents_allowed: file.accents_allowed,
            faces: file.faces,
            decals: file.decals,
            surfaces: file.surfaces,
            textures: file.textures,
            defaults: Appearance {
                parts,
                colors: file.defaults.colors,
                face: file.defaults.face,
                decal: file.defaults.decal,
            },
        })
    }
}
impl From<Package> for PackageFile {
    fn from(package: Package) -> Self {
        let hat = package.defaults.parts.get("hat").cloned().unwrap_or_default();
        let parts = package
            .defaults
            .parts
            .iter()
            .map(|(slot, name)| {
                let choices = if slot == "accent" {
                    package.accents_allowed.get(&hat)
                } else {
                    package.parts.get(slot)
                };
                let index = choices
                    .and_then(|c| c.iter().position(|n| n.eq_ignore_ascii_case(name)))
                    .unwrap_or(0);
                (slot.clone(), index)
            })
            .collect();
        Self {
            schema_version: package.schema_version,
            id: package.id,
            rig: package.rig,
            rig_sha256: package.rig_sha256,
            parts: package.parts,
            accents_allowed: package.accents_allowed,
            faces: package.faces,
            decals: package.decals,
            surfaces: package.surfaces,
            textures: package.textures,
            defaults: PackDefaults {
                parts,
                colors: package.defaults.colors,
                face: package.defaults.face,
                decal: package.defaults.decal,
            },
        }
    }
}

pub struct Outfit {
    pub nodes: BTreeMap<String, [f32; 4]>,
    pub face: String,
    pub decal: String,
    pub head_up: bool,
}

impl Package {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && !self.id.is_empty(),
            "Invalid avatar package"
        );
        ensure!(
            crate::brick_materials::safe_relative(&self.rig),
            "Unsafe avatar rig path"
        );
        ensure!(
            !self.faces.is_empty() && !self.decals.is_empty() && self.textures.len() <= 1024,
            "Invalid avatar texture catalog"
        );
        for texture in self.textures.values() {
            ensure!(
                crate::brick_materials::safe_relative(&texture.file)
                    && texture.width > 0
                    && texture.height > 0
                    && texture.width <= 4096
                    && texture.height <= 4096,
                "Invalid avatar texture"
            );
        }
        for id in self
            .faces
            .iter()
            .chain(&self.decals)
            .chain(self.surfaces.values())
        {
            ensure!(self.textures.contains_key(id), "Unbound avatar image: {id}");
        }
        for slot in [
            "hat",
            "accent",
            "pack",
            "secondpack",
            "chest",
            "hip",
            "rarm",
            "larm",
            "rhand",
            "lhand",
            "rleg",
            "lleg",
        ] {
            ensure!(
                self.parts
                    .get(slot)
                    .is_some_and(|v| !v.is_empty() && v.len() <= 64),
                "Missing/invalid avatar slot {slot}"
            );
        }
        self.resolve(&self.defaults)?;
        Ok(())
    }

    /// Resolve chosen part names into visible named objects. Geometry is
    /// not duplicated into the network state. Unknown choices reject atomically.
    /// `appearance` with every choice this package does not have set back
    /// to the package default: unknown slots and colour names dropped,
    /// unknown parts, accents, faces and decals and unusable colours
    /// replaced. Returns what changed, for the player and the log. A player
    /// whose client knows an Add-On part the host lacks keeps the rest of
    /// their avatar instead of being refused.
    pub fn repaired(&self, appearance: &Appearance) -> (Appearance, Vec<String>) {
        const COLORS: [&str; 13] = [
            "head", "torso", "hat", "accent", "pack", "secondpack", "hip", "rarm", "larm",
            "rhand", "lhand", "rleg", "lleg",
        ];
        let mut changed = Vec::new();
        let mut fixed = Appearance {
            parts: BTreeMap::new(),
            colors: BTreeMap::new(),
            face: appearance.face.clone(),
            decal: appearance.decal.clone(),
        };
        let known = |slot: &str, name: &str| {
            self.parts
                .get(slot)
                .is_some_and(|choices| choices.iter().any(|c| c.eq_ignore_ascii_case(name)))
        };
        for (slot, name) in appearance.parts.iter().take(64) {
            if slot == "accent" || known(slot, name) {
                fixed.parts.insert(slot.clone(), name.clone());
            } else {
                changed.push(format!("{slot} {name}"));
            }
        }
        // An accent only fits the hat it belongs to.
        let hat = fixed
            .parts
            .get("hat")
            .or_else(|| self.defaults.parts.get("hat"))
            .map(|h| h.to_ascii_lowercase())
            .unwrap_or_default();
        if let Some(accent) = fixed.parts.get("accent").cloned() {
            let fits = accent.eq_ignore_ascii_case("none")
                || self
                    .accents_allowed
                    .get(&hat)
                    .is_some_and(|v| v.iter().any(|a| a.eq_ignore_ascii_case(&accent)));
            if !fits {
                fixed.parts.remove("accent");
                changed.push(format!("accent {accent}"));
            }
        }
        for (slot, color) in appearance.colors.iter().take(64) {
            if COLORS.contains(&slot.as_str())
                && color.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            {
                fixed.colors.insert(slot.clone(), *color);
            } else {
                changed.push(format!("{slot} colour"));
            }
        }
        let unique = |name: &str, choices: &[String]| {
            name.len() <= 256
                && choices
                    .iter()
                    .filter(|id| {
                        id.eq_ignore_ascii_case(name)
                            || (!name.contains('/')
                                && id
                                    .rsplit('/')
                                    .next()
                                    .is_some_and(|n| n.eq_ignore_ascii_case(name)))
                    })
                    .count()
                    == 1
        };
        if !unique(&fixed.face, &self.faces) {
            changed.push(format!("face {}", fixed.face));
            fixed.face = self.defaults.face.clone();
        }
        if !unique(&fixed.decal, &self.decals) {
            changed.push(format!("decal {}", fixed.decal));
            fixed.decal = self.defaults.decal.clone();
        }
        (fixed, changed)
    }
    pub fn resolve(&self, appearance: &Appearance) -> Result<Outfit> {
        appearance.validate_bounds()?;
        ensure!(
            appearance.parts.keys().all(|k| self.parts.contains_key(k))
                && appearance.colors.keys().all(|k| [
                    "head",
                    "torso",
                    "hat",
                    "accent",
                    "pack",
                    "secondpack",
                    "hip",
                    "rarm",
                    "larm",
                    "rhand",
                    "lhand",
                    "rleg",
                    "lleg"
                ]
                .contains(&k.as_str())),
            "Unknown avatar slot/color"
        );
        let part = |slot: &str| -> Result<String> {
            let choices = self
                .parts
                .get(slot)
                .ok_or_else(|| anyhow::anyhow!("Unknown avatar slot {slot}"))?;
            let name = match appearance
                .parts
                .get(slot)
                .or_else(|| self.defaults.parts.get(slot))
            {
                Some(name) => name.as_str(),
                None => choices.first().map_or("none", String::as_str),
            };
            Ok(choices
                .iter()
                .find(|choice| choice.eq_ignore_ascii_case(name))
                .ok_or_else(|| anyhow::anyhow!("Invalid avatar choice {slot}:{name}"))?
                .to_ascii_lowercase())
        };
        let color = |slot: &str| -> Result<[f32; 4]> {
            let mut c = *appearance
                .colors
                .get(slot)
                .or_else(|| self.defaults.colors.get(slot))
                .ok_or_else(|| anyhow::anyhow!("Missing avatar color {slot}"))?;
            ensure!(
                c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "Invalid avatar color {slot}"
            );
            if slot != "accent" {
                for channel in &mut c[..3] {
                    *channel = (*channel * 1000.0).floor() / 1000.0;
                }
                c[3] = 1.0;
            } else {
                c[3] = c[3].max(0.2);
            }
            Ok(c)
        };
        // Validate even currently hidden fields so selecting a skirt cannot
        // smuggle invalid leg colors/indices into replicated preferences.
        for slot in self.parts.keys().filter(|s| s.as_str() != "accent") {
            part(slot)?;
        }
        for slot in appearance.colors.keys() {
            color(slot)?;
        }
        let mut nodes = BTreeMap::new();
        nodes.insert("headskin".into(), color("head")?);
        for slot in [
            "hat",
            "pack",
            "secondpack",
            "chest",
            "hip",
            "rarm",
            "larm",
            "rhand",
            "lhand",
        ] {
            let node = part(slot)?;
            if node != "none" {
                nodes.insert(node, color(if slot == "chest" { "torso" } else { slot })?);
            }
        }
        let skirt = part("hip")? == "skirthip";
        for (slot, trim) in [("lleg", "skirttrimleft"), ("rleg", "skirttrimright")] {
            nodes.insert(if skirt { trim.into() } else { part(slot)? }, color(slot)?);
        }
        let hat = part("hat")?;
        let accent = appearance
            .parts
            .get("accent")
            .or_else(|| self.defaults.parts.get("accent"))
            .map_or("none", String::as_str);
        let selected = self
            .accents_allowed
            .get(&hat)
            .and_then(|v| v.iter().find(|a| a.eq_ignore_ascii_case(accent)));
        ensure!(
            accent.eq_ignore_ascii_case("none") || selected.is_some(),
            "Invalid accent for selected hat"
        );
        if let Some(node) = selected.filter(|s| !s.eq_ignore_ascii_case("none")) {
            nodes.insert(node.to_ascii_lowercase(), color("accent")?);
        }
        let image = |name: &str, choices: &[String]| -> Result<String> {
            let matches: Vec<_> = choices
                .iter()
                .filter(|id| {
                    id.eq_ignore_ascii_case(name)
                        || (!name.contains('/')
                            && id
                                .rsplit('/')
                                .next()
                                .is_some_and(|n| n.eq_ignore_ascii_case(name)))
                })
                .collect();
            ensure!(matches.len() == 1, "Unknown/ambiguous avatar image {name}");
            Ok(matches[0].clone())
        };
        Ok(Outfit {
            nodes,
            face: image(&appearance.face, &self.faces)?,
            decal: image(&appearance.decal, &self.decals)?,
            head_up: part("pack")? != "none" || part("secondpack")? != "none",
        })
    }
}

impl Rig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && !self.id.is_empty(),
            "Invalid avatar rig schema"
        );
        self.shape.validate()?;
        let names: BTreeSet<_> = self
            .shape
            .nodes
            .iter()
            .map(|n| n.name.to_ascii_lowercase())
            .collect();
        ensure!(
            names.len() == self.shape.nodes.len(),
            "Duplicate avatar node names"
        );
        ensure!(
            !self.sequences.is_empty() && self.sequences.len() <= 256,
            "Invalid sequence count"
        );
        for (name, clip) in &self.sequences {
            ensure!(
                !name.is_empty() && name == &name.to_ascii_lowercase(),
                "Invalid sequence alias"
            );
            clip.validate()?;
            let mut tracks = BTreeSet::new();
            for track in &clip.nodes {
                let key = track.node.to_ascii_lowercase();
                ensure!(
                    names.contains(&key) && tracks.insert(key),
                    "Unbound/duplicate avatar track in {name}: {}",
                    track.node
                );
            }
            let mut objects = BTreeSet::new();
            for track in &clip.objects {
                ensure!(
                    track.object < self.shape.objects.len() && objects.insert(track.object),
                    "Invalid avatar object track in {name}"
                );
            }
        }
        Ok(())
    }

    pub fn sequence(&self, name: &str) -> Option<&Animation> {
        self.sequences.get(&name.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package() -> Package {
        let slots = [
            ("hat", vec!["none", "helmet"]),
            ("accent", vec!["none", "visor"]),
            ("pack", vec!["none", "armor"]),
            ("secondpack", vec!["none"]),
            ("chest", vec!["chest"]),
            ("hip", vec!["pants", "skirthip"]),
            ("rarm", vec!["rarm"]),
            ("larm", vec!["larm"]),
            ("rhand", vec!["rhand"]),
            ("lhand", vec!["lhand"]),
            ("rleg", vec!["rshoe"]),
            ("lleg", vec!["lshoe"]),
        ];
        let parts: BTreeMap<String, Vec<String>> = slots
            .iter()
            .map(|(s, v)| (s.to_string(), v.iter().map(|c| c.to_string()).collect()))
            .collect();
        let colors = [
            "head", "torso", "hat", "accent", "pack", "secondpack", "hip", "rarm", "larm",
            "rhand", "lhand", "rleg", "lleg",
        ]
        .iter()
        .map(|s| (s.to_string(), [1.0, 1.0, 0.0, 1.0]))
        .collect();
        Package {
            schema_version: 1,
            id: "test".into(),
            rig: String::new(),
            rig_sha256: String::new(),
            parts,
            accents_allowed: BTreeMap::from([(
                "helmet".into(),
                vec!["none".into(), "visor".into()],
            )]),
            faces: vec!["faces/smiley".into()],
            decals: vec!["decals/aaa-none".into()],
            surfaces: BTreeMap::new(),
            textures: BTreeMap::new(),
            defaults: Appearance {
                parts: BTreeMap::from([("hat".into(), "none".into())]),
                colors,
                face: "smiley".into(),
                decal: "AAA-None".into(),
            },
        }
    }

    #[test]
    fn unknown_avatar_choices_fall_back_to_defaults_and_keep_the_rest() {
        let package = package();
        let wanted = Appearance {
            parts: BTreeMap::from([
                ("hat".into(), "Helmet".into()),
                ("pack".into(), "AddOnJetpack".into()),
                ("tail".into(), "long".into()),
                ("accent".into(), "visor".into()),
            ]),
            colors: BTreeMap::from([
                ("torso".into(), [0.5, 0.0, 0.0, 1.0]),
                ("wings".into(), [0.0; 4]),
            ]),
            face: "AddOnFace".into(),
            decal: "AAA-None".into(),
        };
        // Before: one unknown part refused the whole avatar.
        assert!(package.resolve(&wanted).is_err());
        let (fixed, changed) = package.repaired(&wanted);
        let outfit = package.resolve(&fixed).unwrap();
        assert_eq!(fixed.parts["hat"], "Helmet");
        assert_eq!(fixed.parts["accent"], "visor");
        assert!(!fixed.parts.contains_key("pack") && !fixed.parts.contains_key("tail"));
        assert_eq!(fixed.colors["torso"], [0.5, 0.0, 0.0, 1.0]);
        assert_eq!(fixed.face, "smiley");
        assert_eq!(outfit.face, "faces/smiley");
        assert_eq!(changed.len(), 4, "{changed:?}");
        // An accent that does not fit the hat goes back to none.
        let (fixed, changed) = package.repaired(&Appearance {
            parts: BTreeMap::from([("accent".into(), "visor".into())]),
            ..package.defaults.clone()
        });
        assert!(package.resolve(&fixed).is_ok() && changed == ["accent visor"]);
        // A valid avatar is untouched.
        let (same, changed) = package.repaired(&package.defaults);
        assert!(changed.is_empty());
        assert_eq!(same.face, package.defaults.face);
    }
}
