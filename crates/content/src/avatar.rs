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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    pub parts: BTreeMap<String, usize>,
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
                && self.parts.iter().all(|(k, v)| k.len() <= 32 && *v < 64)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Resolve stock indices and names into visible named objects. Geometry is
    /// not duplicated into the network state. Unknown choices reject atomically.
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
            let index = appearance
                .parts
                .get(slot)
                .or_else(|| self.defaults.parts.get(slot))
                .copied()
                .unwrap_or(0);
            Ok(self
                .parts
                .get(slot)
                .and_then(|p| p.get(index))
                .ok_or_else(|| anyhow::anyhow!("Invalid avatar choice {slot}:{index}"))?
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
            .copied()
            .unwrap_or(0);
        let selected = self.accents_allowed.get(&hat).and_then(|v| v.get(accent));
        ensure!(
            accent == 0 || selected.is_some(),
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
