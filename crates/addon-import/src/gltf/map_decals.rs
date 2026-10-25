//! A map Add-On's projector pictures from `--projector-pictures FILE.json` (docs/modding/maps.md): a [`bri_content::decals::DecalSet`] already in the native frame,
//! with each decal's `picture` a PNG beside the file, and the projectors its glTF nodes place (`bri_projector`). The pictures and the set
//! are copied into the map's bundle and listed in its `decals` table.
use super::map_scatter::Put;
use super::{Document, Options, Report};
use anyhow::{Context, Result, ensure};
use bri_content::decals::{Decal, DecalSet};
use bri_content::shape::Shape;
use bri_effects_render::projector_runtime::{
    MakeOptions, Placement, ProjectorBlend, ProjectorReceivers, ProjectorSpec, make_decal,
};
use glam::{Mat4, Vec3};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

/// A node's custom property `bri_projector`: a picture thrown from the node along its forward axis (glTF's -Z, the way a light or camera looks; +Z in the shape's native frame, which turns glTF half a turn about y) onto the map's surfaces
/// inside a volume. Made the way a game's projector is made at run time (`bri_effects_render::projector_runtime::make_decal`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Projector {
    /// A PNG beside the glTF.
    picture: String,
    /// The cone's full opening in degrees; 0 (the default) is a box as wide as `width`.
    #[serde(default)]
    fov_degrees: f32,
    /// How far along the throw it reaches, in the model's units.
    reach: f32,
    /// The footprint's width at the node, in the model's units.
    width: f32,
    /// Turn of the picture about the throw, degrees.
    #[serde(default)]
    roll_degrees: f32,
    #[serde(default)]
    spin_degrees_per_second: f32,
    #[serde(default)]
    blend: Blend,
    /// The map's walls, floors and meshes take it (the default).
    #[serde(default = "yes")]
    surfaces: bool,
    /// The terrain takes it (the default).
    #[serde(default = "yes")]
    terrain: bool,
    /// Faces turned away from the node take it.
    #[serde(default)]
    backfaces: bool,
    /// Surfaces nearly parallel to the throw take it (the default).
    #[serde(default = "yes")]
    grazing_surfaces: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Blend {
    #[default]
    Modulate2x,
    Alpha,
    Add,
}

/// Decimals a throw direction is kept to. A node pointed straight down has no yaw of its own, and f32 noise in its quaternion
/// (a few millionths) would pick one at random; kept to this many places it is exactly vertical and the yaw is the same every import.
const DIRECTION_DECIMALS: i32 = 5;

fn clean(component: f32) -> f32 {
    let step = 10f32.powi(DIRECTION_DECIMALS);
    (component * step).round() / step
}

/// A PNG's width and height from its header.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let header = bytes.get(..24)?;
    if !header.starts_with(b"\x89PNG\r\n\x1a\n") || &header[12..16] != b"IHDR" {
        return None;
    }
    let word = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().expect("four bytes"));
    Some((word(16), word(20)))
}

/// The decals the glTF's `bri_projector` nodes make, each with the file name of its picture (read from `folder`).
fn node_decals(
    doc: &Document,
    shape: &Shape,
    globals: &[Mat4],
    folder: Option<&Path>,
    report: &mut Report,
) -> Result<Vec<(Decal, String)>> {
    let mut out = Vec::new();
    for (k, node) in shape.nodes.iter().enumerate() {
        let Some(json) = doc.json["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|n| n["name"].as_str().map(str::trim) == Some(node.name.as_str()))
            .and_then(|n| n["extras"].get("bri_projector"))
        else {
            continue;
        };
        let name = &node.name;
        let p: Projector = serde_json::from_value(json.clone())
            .with_context(|| format!("`{name}`'s bri_projector"))?;
        let scale = report.scale;
        ensure!(
            p.reach > 0.0
                && p.width > 0.0
                && p.fov_degrees >= 0.0
                && p.fov_degrees < 180.0
                && [
                    p.reach,
                    p.width,
                    p.fov_degrees,
                    p.roll_degrees,
                    p.spin_degrees_per_second
                ]
                .iter()
                .all(|v| v.is_finite())
                && !p.picture.is_empty()
                && !p.picture.contains(['/', '\\']),
            "`{name}`'s bri_projector needs a picture file name, a reach and a width above 0 and a field of view below 180"
        );
        let file = folder.map(|f| f.join(&p.picture)).with_context(|| {
            format!("`{name}`'s bri_projector has no folder to find its picture in")
        })?;
        let bytes = std::fs::read(&file)
            .with_context(|| format!("`{name}`'s bri_projector picture {}", file.display()))?;
        let size = png_size(&bytes).with_context(|| {
            format!(
                "`{name}`'s bri_projector picture {} is not a PNG",
                p.picture
            )
        })?;
        let (_, rotation, position) = globals[k].to_scale_rotation_translation();
        let spec = ProjectorSpec {
            picture: p.picture.clone(),
            fov_degrees: p.fov_degrees,
            reach: p.reach * scale,
            width: p.width * scale,
            spin_degrees_per_second: p.spin_degrees_per_second,
            receivers: ProjectorReceivers {
                map_surfaces: p.surfaces,
                terrain: p.terrain,
                backfaces: p.backfaces,
                grazing_surfaces: p.grazing_surfaces,
            },
            blend: match p.blend {
                Blend::Modulate2x => ProjectorBlend::Modulate2x,
                Blend::Alpha => ProjectorBlend::Alpha,
                Blend::Add => ProjectorBlend::Add,
            },
        };
        let placement = Placement {
            position: position.to_array(),
            direction: (rotation * Vec3::Z).to_array().map(clean),
            roll_degrees: p.roll_degrees,
        };
        let decal = make_decal(
            &spec,
            &placement,
            size,
            &p.picture,
            MakeOptions {
                bake_spin: true,
                widen_for_turn: true,
            },
        )
        .with_context(|| format!("`{name}`'s bri_projector picture has no size"))?;
        out.push((decal, p.picture));
    }
    Ok(out)
}

/// The decals file's name below `map/` and the pictures it needs listed as textures.
pub(super) struct Built {
    pub file: Option<String>,
    pub textures: BTreeMap<String, String>,
}

pub(super) fn decals(
    opts: &Options,
    id: &str,
    doc: &Document,
    shape: &Shape,
    globals: &[Mat4],
    put: Put<'_>,
    report: &mut Report,
) -> Result<Built> {
    let mut built = Built {
        file: None,
        textures: BTreeMap::new(),
    };
    let mut set = DecalSet::default();
    let mut dir = Path::new(".");
    if let Some(path) = &opts.projector_pictures {
        let text =
            std::fs::read(path).with_context(|| format!("Reading decals {}", path.display()))?;
        set = serde_json::from_slice(&text).context("Reading the map's decals")?;
        set.validate()?;
        ensure!(
            set.decals.is_empty(),
            "--projector-pictures carries pictures only; a game's projectors attach at run time and are not baked into the map (a glTF node's bri_projector bakes one)"
        );
        dir = path.parent().unwrap_or(Path::new("."));
    }
    let placed = node_decals(doc, shape, globals, opts.input.parent(), report)?;
    if set.pictures.is_empty() && placed.is_empty() {
        return Ok(built);
    }
    let mut copied = BTreeMap::new();
    for (_, picture) in &placed {
        if copied.contains_key(picture) {
            continue;
        }
        let bytes = std::fs::read(opts.input.parent().unwrap_or(Path::new(".")).join(picture))
            .with_context(|| format!("Reading decal picture {picture}"))?;
        let name = format!("{id}_decal_{picture}");
        put(&format!("map/{name}"), &bytes, report)?;
        built.textures.insert(name.clone(), name.clone());
        copied.insert(picture.clone(), name);
    }
    set.decals
        .extend(placed.into_iter().map(|(mut decal, picture)| {
            decal.picture = copied[&picture].clone();
            decal
        }));
    for picture in &mut set.pictures {
        let name = match copied.get(&picture.file) {
            Some(name) => name.clone(),
            None => {
                let bytes = std::fs::read(dir.join(&picture.file))
                    .with_context(|| format!("Reading decal picture {}", picture.file))?;
                ensure!(
                    bytes.starts_with(b"\x89PNG"),
                    "Decal picture {} is not a PNG",
                    picture.file
                );
                let name = format!("{id}_decal_{}", picture.file);
                put(&format!("map/{name}"), &bytes, report)?;
                built.textures.insert(name.clone(), name.clone());
                copied.insert(picture.file.clone(), name.clone());
                name
            }
        };
        picture.file = name;
    }
    report
        .warnings
        .extend(set.notes.iter().map(|n| format!("decals: {n}")));
    let file = format!("{id}.decals.json");
    put(&format!("map/{file}"), &serde_json::to_vec(&set)?, report)?;
    built.file = Some(file);
    Ok(built)
}
