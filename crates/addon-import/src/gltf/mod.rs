//! A glTF 2.0 model (`.glb`, or `.gltf` with its files) to a native
//! [`Shape`] and the PNGs its materials name: what Blender and most modelling
//! tools export, for an Add-On's own bodies, held items and vehicles.
//!
//! The authoring contract (names, scale, axes, clips) is
//! `docs/modding/gltf-models.md`. glTF is Y-up with the model facing +Z;
//! the native frame is Y-up facing -Z, so everything turns half a turn
//! about Y. Node scale, which native nodes do not carry, is pushed into
//! child translations, mesh vertices and inverse bind matrices. Animation
//! clips are sampled at a fixed rate; clips named after a body's animation
//! roles take the Blockhead's clip names, so a body needs no mapping.
mod brick;
mod export;
mod lod;
mod map;
mod map_environment;
mod map_terrain;
mod read;
mod write;
pub use export::{ExportReport, export_map, find_map};
#[doc(hidden)]
pub mod testing;

use anyhow::{Context, Result, bail, ensure};
use bri_console::Clamp;
use bri_content::shape::{
    Animation, Detail, Influence, Material, Mesh, Metal, Node, NodeTrack, Object, Primitive, Shape,
    Skin, Surface,
};
use glam::{Mat4, Quat, Vec3};
pub use read::Document;
use read::array;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What the model becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A player or bot body: a package with the model and an archetype.
    Body,
    /// A held item (weapon or tool): a model for `weapons.json`.
    Item,
    /// Anything else drawn from a package's own models: vehicles, wheels,
    /// projectiles, props.
    Prop,
    /// A whole scene as a map Add-On: its meshes, spawn points and lights
    /// (docs/modding/maps.md).
    Map,
    /// One brick for the brick menu, drawn in paint colour
    /// (docs/modding/bricks.md).
    Brick,
}
impl Kind {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "body" | "character" | "player" => Kind::Body,
            "item" | "weapon" | "tool" => Kind::Item,
            "prop" | "vehicle" | "model" => Kind::Prop,
            "map" | "level" => Kind::Map,
            "brick" => Kind::Brick,
            _ => bail!(
                "--kind is body, item, prop (vehicles, wheels and anything else), map or brick"
            ),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub input: PathBuf,
    pub out: PathBuf,
    pub kind: Kind,
    /// The model's file name and, for a body, its package id; from the
    /// input file name when unset.
    pub id: Option<String>,
    /// A body's name in menus; from the id when unset.
    pub name: Option<String>,
    /// Multiplies every length (`--scale 0.01` for a model made in
    /// centimetres).
    pub scale: Option<f32>,
    /// Scales the model to this height, feet to top, in game units.
    pub height: Option<f32>,
    /// Animation samples per second.
    pub fps: f32,
    /// Nodes to use for the body's sockets instead of the named empties.
    pub right_hand: Option<String>,
    pub left_hand: Option<String>,
    pub eye: Option<String>,
    /// Extra clips (not animation roles) that loop.
    pub loops: Vec<String>,
    pub version: String,
    /// The triangle budget of a body's distant detail; none when unset
    /// (or 0). Objects named `<name>_LOD1` make one whatever this says.
    pub lod: Option<usize>,
    /// How tall on screen, in pixels, a body draws its full detail; smaller
    /// than that it draws the distant one.
    pub lod_pixels: f32,
    /// A body's collision box, game units: its width (and depth) and its
    /// standing height. From the model when unset: its height, and the
    /// Blockhead's width for that height.
    pub box_width: Option<f32>,
    pub box_height: Option<f32>,
    /// A map's sky: a folder of six pictures (`front`, `right`, `back`,
    /// `left`, `up`, `down`, PNG or JPEG).
    pub sky: Option<PathBuf>,
    /// A map's heightmap terrain: a folder holding `height.png`, ground
    /// textures `layer0`..`layer7` with masks `weight0`..`weight7`, and an
    /// optional `detail` texture.
    pub terrain: Option<PathBuf>,
}
impl Options {
    pub fn new(input: impl Into<PathBuf>, out: impl Into<PathBuf>, kind: Kind) -> Self {
        Self {
            input: input.into(),
            out: out.into(),
            kind,
            id: None,
            name: None,
            scale: None,
            height: None,
            fps: 30.0,
            right_hand: None,
            left_hand: None,
            eye: None,
            loops: Vec::new(),
            version: "1.0.0".into(),
            lod: None,
            lod_pixels: LOD_PIXELS,
            box_width: None,
            box_height: None,
            sky: None,
            terrain: None,
        }
    }
}

/// What an import made and what it could not use.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub kind: Option<Kind>,
    pub source: String,
    /// The package id (a body) or the model's file name (item, prop).
    pub id: String,
    /// Files written, relative to the output folder.
    pub files: Vec<String>,
    /// The model id an archetype or `weapons.json` names.
    pub model: String,
    pub scale: f32,
    /// Feet-to-top height and widest horizontal extent, in game units.
    pub height: f32,
    pub width: f32,
    pub triangles: usize,
    pub vertices: usize,
    pub bones: usize,
    pub materials: Vec<String>,
    pub clips: Vec<Clip>,
    /// Socket name to the node it follows.
    pub sockets: BTreeMap<String, String>,
    /// Animation roles with no clip, and what the body does instead.
    pub missing_roles: Vec<String>,
    /// Nodes a `vehicles.json` or `weapons.json` refers to, with their
    /// rest position in the model's frame.
    pub points: Vec<Point>,
    /// Each `col_` mesh's corners in the model's frame: a vehicle's
    /// `collision_hulls`.
    pub hulls: Vec<Hull>,
    /// The distant detail, when the model has one.
    pub lod: Option<Lod>,
    /// A body's collision box and eye heights, as its archetype's
    /// `movement` gets them.
    pub movement: Option<Movement>,
    /// A map's teleporters, one line each: name, role, channel, radius.
    pub teleporters: Vec<String>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Movement {
    pub width: f64,
    pub stand_height: f64,
    pub crouch_height: f64,
    pub stand_eye: f64,
    pub crouch_eye: f64,
    pub sit_eye: f64,
}
impl Movement {
    /// The Blockhead's box and eyes (`PlayerTuning::default`) scaled to
    /// `height`, `width` wide and the standing eye at `eye`.
    fn new(width: f32, height: f32, eye: f32) -> Self {
        const BLOCKHEAD: (f32, f32, f32, f32, f32) =
            (2.65, 1.0, 2.156_496_5, 0.626_668_45, 1.635_348_9);
        let (tall, crouch, stand_eye, crouch_eye, sit_eye) = BLOCKHEAD;
        let eye = eye.clamped(height * 0.3, height);
        let r = |v: f32| (f64::from(v) * 1000.0).round() / 1000.0;
        Self {
            width: r(width),
            stand_height: r(height),
            crouch_height: r(height * crouch / tall),
            stand_eye: r(eye),
            crouch_eye: r(eye * crouch_eye / stand_eye),
            sit_eye: r(eye * sit_eye / stand_eye),
        }
    }
}
/// The Blockhead's width for its height (1.25 for 2.65).
const WIDTH_FOR_HEIGHT: f32 = 1.25 / 2.65;
#[derive(Debug, Clone, Serialize)]
pub struct Lod {
    pub triangles: usize,
    /// Drawn below this many pixels tall.
    pub pixels: f32,
    /// From the model's own `_LOD1` objects rather than generated.
    pub hand_made: bool,
}
/// Default [`Options::lod_pixels`]: a body under 100 pixels tall (about 20
/// units away for a Blockhead at 1080p) draws its distant detail.
pub const LOD_PIXELS: f32 = 100.0;
#[derive(Debug, Clone, Serialize)]
pub struct Hull {
    pub object: String,
    pub points: Vec<[f32; 3]>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Clip {
    pub source: String,
    pub name: String,
    pub role: Option<String>,
    pub frames: usize,
    pub duration: f32,
    pub looping: bool,
    pub additive: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Point {
    pub node: String,
    pub position: [f32; 3],
}

/// The animation roles of a body (`bri_content::body::Role` in v0.3.0's
/// body contract): the role's name, the Blockhead clip it is stored as,
/// whether it loops, and how it plays.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Play {
    /// The base pose: locomotion and whole-body actions.
    Absolute { looping: bool, priority: i32 },
    /// Layered over everything, relative to one frame (`0.5` the middle).
    Overlay { reference: f32 },
}
const LOOP: Play = Play::Absolute {
    looping: true,
    priority: 12,
};
const ONCE: Play = Play::Absolute {
    looping: false,
    priority: 12,
};
/// Native sequence priorities: armReady (14) over locomotion (12), death
/// (128) over everything.
const AIM: Play = Play::Absolute {
    looping: false,
    priority: 14,
};
const DEATH: Play = Play::Absolute {
    looping: false,
    priority: 128,
};
const LEVEL: Play = Play::Overlay { reference: 0.5 };
const FIRST: Play = Play::Overlay { reference: 0.0 };
#[rustfmt::skip]
const ROLES: [(&str, &str, Play, &str); 22] = [
    ("idle", "root", LOOP, "it stands in its bind pose"),
    ("run", "run", LOOP, "it plays its `idle`"),
    ("back", "back", LOOP, "it plays its `run` reversed"),
    ("side", "side", LOOP, "it plays its `run`"),
    ("side_right", "sideright", LOOP, "it plays its `side` reversed"),
    ("jump", "jump", ONCE, "it plays its `idle`"),
    ("fall", "fall", LOOP, "it plays its `jump`"),
    ("crouch", "crouch", ONCE, "it plays its `idle`"),
    ("crouch_run", "crouchrun", LOOP, "it plays its `crouch`"),
    ("crouch_back", "crouchback", LOOP, "it plays its `crouch`"),
    ("crouch_side", "crouchside", LOOP, "it plays its `crouch`"),
    ("crouch_side_right", "crouchsideright", LOOP, "it plays its `crouch_side` reversed"),
    ("aim_right", "armreadyright", AIM, "its arm does not raise a held item"),
    ("aim_left", "armreadyleft", AIM, "its arm does not raise a held item"),
    ("aim_both", "armreadyboth", AIM, "it plays its `aim_right`"),
    ("sit", "sit", ONCE, "it sits in its `idle`"),
    ("death", "death1", DEATH, "it falls in its `idle`"),
    ("swim", "swim", LOOP, "it swims in its locomotion pose"),
    ("look", "look", LEVEL, "it does not look up and down"),
    ("look_crouch", "lookcrouch", LEVEL, "it plays its `look`"),
    ("head_side", "headside", LEVEL, "its head does not turn in free look"),
    ("head_up", "headup", FIRST, "its head does not tilt under a pack"),
];
fn role(name: &str) -> Option<&'static (&'static str, &'static str, Play, &'static str)> {
    let lower = name
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-', '.'], "_");
    ROLES.iter().find(|(role, clip, ..)| {
        lower == *role || lower == *clip || lower.replace('_', "") == *clip
    })
}

/// The nodes each socket looks for, best first, when no `--right-hand`
/// (and so on) names one. Bone names cover Blender's, Mixamo's and the
/// common game rigs'.
const RIGHT_HAND: &[&str] = &[
    "socket_right_hand",
    "hand.r",
    "hand_r",
    "righthand",
    "mixamorig:righthand",
    "r_hand",
    "hand_right",
    "right_hand",
];
const LEFT_HAND: &[&str] = &[
    "socket_left_hand",
    "hand.l",
    "hand_l",
    "lefthand",
    "mixamorig:lefthand",
    "l_hand",
    "hand_left",
    "left_hand",
];
const EYE: &[&str] = &["socket_eye"];

/// glTF to native: half a turn about Y, then the import's scale.
fn point(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, v.y, -v.z)
}
fn turn(q: Quat) -> Quat {
    Quat::from_xyzw(-q.x, q.y, -q.z, q.w).normalize()
}
/// `C · m · C⁻¹` for an affine matrix.
fn conjugate(m: Mat4) -> Mat4 {
    let c = Mat4::from_scale(Vec3::new(-1.0, 1.0, -1.0));
    c * m * c
}

struct GNode {
    name: String,
    parent: Option<usize>,
    children: Vec<usize>,
    t: Vec3,
    r: Quat,
    s: Vec3,
    mesh: Option<usize>,
    skin: Option<usize>,
}

/// A node's uniform scale and whether its scale was uniform.
fn uniform(s: Vec3) -> (f32, bool) {
    let u = (s.x * s.y * s.z).abs().cbrt();
    let even = s.abs().max_element() <= s.abs().min_element() * 1.01 + 1e-6;
    (if u > 1e-6 { u } else { 1.0 }, even)
}

fn safe_name(name: &str) -> String {
    let mut out: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() { "model".into() } else { out }
}

/// The converted model and the PNGs its materials name.
pub struct Converted {
    pub shape: Shape,
    /// Material name to PNG bytes.
    pub textures: BTreeMap<String, Vec<u8>>,
    pub report: Report,
    /// The glTF material each of `shape.materials` was made from, in order
    /// (None for primitives with none).
    pub material_sources: Vec<Option<usize>>,
    /// A map's meshes named `water...`, left out of the model: each
    /// becomes a body of water.
    pub waters: Vec<WaterMesh>,
}

/// A map mesh named `water...`: its corners in its node's frame (native,
/// scaled) and the glTF material it is painted with.
#[derive(Clone, Debug)]
pub struct WaterMesh {
    pub name: String,
    pub node: usize,
    pub positions: Vec<[f32; 3]>,
    pub material: Option<usize>,
}

/// Convert and write the model as `opts.kind` says. `opts.out` must not
/// exist or be empty.
pub fn import(opts: &Options) -> Result<Report> {
    ensure!(
        opts.fps.is_finite() && (1.0..=120.0).contains(&opts.fps),
        "--fps is 1 to 120"
    );
    if opts.out.exists() {
        ensure!(
            std::fs::read_dir(&opts.out)?.next().is_none(),
            "{} is not empty: give a fresh output folder",
            opts.out.display()
        );
    }
    let stem = opts
        .input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let id = safe_name(opts.id.as_deref().unwrap_or(&stem));
    let doc = Document::open(&opts.input)?;
    let folder = opts.input.parent().map(Path::to_path_buf);
    let prefix = match opts.kind {
        Kind::Body => String::new(),
        _ => format!("{id}_"),
    };
    let mut converted = convert(&doc, folder.as_deref(), opts, &id, &prefix)?;
    converted.report.source = opts
        .input
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if matches!(opts.kind, Kind::Map | Kind::Brick) {
        let mut report = if opts.kind == Kind::Map {
            map::write(opts, &id, converted, &doc)?
        } else {
            brick::write(opts, &id, converted)?
        };
        report.files.push("IMPORT-REPORT.md".into());
        std::fs::write(opts.out.join("IMPORT-REPORT.md"), report.markdown())?;
        return Ok(report);
    }
    write(opts, &id, converted)
}

fn write(opts: &Options, id: &str, converted: Converted) -> Result<Report> {
    let Converted {
        shape,
        textures,
        mut report,
        ..
    } = converted;
    let out = &opts.out;
    let put = |rel: &str, bytes: &[u8], report: &mut Report| -> Result<()> {
        let path = out.join(rel);
        std::fs::create_dir_all(path.parent().context("output path")?)?;
        std::fs::write(path, bytes)?;
        report.files.push(rel.into());
        Ok(())
    };
    let shape_bytes = serde_json::to_vec(&shape)?;
    match opts.kind {
        Kind::Body => {
            let ns = crate::namespace_for(id)?;
            report.id = ns.clone();
            // Provided as `body` beside the archetype, so the Add-On is
            // shared: joiners download the body with its textures.
            let model = format!("{ns}:body/{id}");
            report.model = model.clone();
            let mut content = Vec::new();
            put(
                &format!("assets/{id}.shape.json"),
                &shape_bytes,
                &mut report,
            )?;
            for (name, png) in &textures {
                put(&format!("assets/{name}.png"), png, &mut report)?;
                content.push(json!({ "kind": "asset", "id": format!("{ns}:asset/{name}.png"), "file": format!("assets/{name}.png") }));
            }
            put(
                "assets/content.json",
                &serde_json::to_vec_pretty(&json!({ "schema_version": 1, "content": content }))?,
                &mut report,
            )?;
            let name = opts.name.clone().unwrap_or_else(|| title(id));
            let archetype = json!({
                "schema_version": 1,
                "name": name,
                "model": model,
                "max_health": 100,
                "movement": report.movement.map_or(json!({}), |m| json!(m)),
            });
            put(
                &format!("assets/archetypes/{id}.json"),
                &serde_json::to_vec_pretty(&archetype)?,
                &mut report,
            )?;
            let manifest = json!({
                "schema_version": 1,
                "id": ns,
                "version": opts.version,
                "api": 1,
                "name": name,
                "description": format!("{name}, a body imported from {} by bri-import-addon gltf.", report.source),
                "authors": [],
                "license": "proprietary",
                "provenance": { "source": format!("glTF model {}", report.source), "notes": "Imported by bri-import-addon gltf. Set the licence of the original model." },
                "dependencies": {},
                "capabilities": [],
                "provides": [
                    { "kind": "archetype", "id": format!("{ns}:archetype/{id}"), "file": format!("assets/archetypes/{id}.json") },
                    { "kind": "body", "id": model, "file": format!("assets/{id}.shape.json") },
                ],
            });
            put(
                "package.json",
                &serde_json::to_vec_pretty(&manifest)?,
                &mut report,
            )?;
        }
        Kind::Map | Kind::Brick => unreachable!("maps and bricks are written by their own modules"),
        Kind::Item | Kind::Prop => {
            report.id = id.into();
            report.model = format!("models/{id}.shape.json");
            put(
                &format!("models/{id}.shape.json"),
                &shape_bytes,
                &mut report,
            )?;
            for (name, png) in &textures {
                put(&format!("models/{name}.png"), png, &mut report)?;
            }
        }
    }
    report.files.push("IMPORT-REPORT.md".into());
    let markdown = report.markdown();
    std::fs::write(out.join("IMPORT-REPORT.md"), markdown)?;
    Ok(report)
}

fn title(id: &str) -> String {
    id.split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_ascii_uppercase().to_string() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The model in `doc` as a native shape. `prefix` goes before each
/// material's name (an item's PNGs share a folder with other models).
pub fn convert(
    doc: &Document,
    folder: Option<&Path>,
    opts: &Options,
    id: &str,
    prefix: &str,
) -> Result<Converted> {
    let mut report = Report {
        kind: Some(opts.kind),
        id: id.into(),
        ..Report::default()
    };
    let json = &doc.json;
    // Nodes, their parents and rest transforms.
    let mut nodes: Vec<GNode> = Vec::new();
    for (i, n) in array(json, "nodes").iter().enumerate() {
        let (t, r, s) = if let Some(m) = n["matrix"].as_array() {
            let m: Vec<f32> = m.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect();
            ensure!(m.len() == 16, "node {i} has a bad matrix");
            let (s, r, t) = Mat4::from_cols_slice(&m).to_scale_rotation_translation();
            (t, r, s)
        } else {
            let v = |key: &str, default: &[f32]| -> Vec<f32> {
                n[key]
                    .as_array()
                    .map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
                    .unwrap_or_else(|| default.to_vec())
            };
            let t = v("translation", &[0.0; 3]);
            let r = v("rotation", &[0.0, 0.0, 0.0, 1.0]);
            let s = v("scale", &[1.0; 3]);
            ensure!(
                t.len() == 3 && r.len() == 4 && s.len() == 3,
                "node {i} has a bad transform"
            );
            (
                Vec3::from_slice(&t),
                Quat::from_slice(&r).normalize(),
                Vec3::from_slice(&s),
            )
        };
        ensure!(
            t.is_finite() && r.is_finite() && s.is_finite(),
            "node {i} has a non-finite transform"
        );
        nodes.push(GNode {
            name: n["name"].as_str().unwrap_or_default().trim().to_string(),
            parent: None,
            children: n["children"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .map(|c| c as usize)
                .collect(),
            t,
            r,
            s,
            mesh: n["mesh"].as_u64().map(|m| m as usize),
            skin: n["skin"].as_u64().map(|s| s as usize),
        });
    }
    ensure!(
        !nodes.is_empty(),
        "the model has no nodes: export with Include > Visible Objects"
    );
    for i in 0..nodes.len() {
        for c in nodes[i].children.clone() {
            ensure!(c < nodes.len() && c != i, "node {i} has a bad child {c}");
            ensure!(nodes[c].parent.is_none(), "node {c} has two parents");
            nodes[c].parent = Some(i);
        }
    }
    // Only the default scene's nodes; parents before children.
    let scene = json["scene"].as_u64().unwrap_or(0) as usize;
    let roots: Vec<usize> = match array(json, "scenes").get(scene) {
        Some(s) => s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .map(|n| n as usize)
            .collect(),
        None => (0..nodes.len())
            .filter(|i| nodes[*i].parent.is_none())
            .collect(),
    };
    let mut order = Vec::new();
    let mut stack: Vec<usize> = roots.iter().rev().copied().collect();
    let mut seen = vec![false; nodes.len()];
    while let Some(n) = stack.pop() {
        ensure!(
            n < nodes.len(),
            "the scene names node {n}, which does not exist"
        );
        ensure!(!seen[n], "node {n} appears twice in the scene (a cycle)");
        seen[n] = true;
        order.push(n);
        stack.extend(nodes[n].children.iter().rev());
    }
    ensure!(
        order.len() <= 1024,
        "the model has {} nodes; at most 1024",
        order.len()
    );
    let mut new_index = vec![usize::MAX; nodes.len()];
    for (k, n) in order.iter().enumerate() {
        new_index[*n] = k;
    }
    // Uniform world scale of each node at rest, which native nodes don't
    // carry: children's translations and meshes take it instead.
    let mut sigma = vec![1.0f32; nodes.len()];
    for &n in &order {
        let (u, even) = uniform(nodes[n].s);
        let parent = nodes[n].parent.map_or(1.0, |p| sigma[p]);
        sigma[n] = parent * u;
        if !even && !nodes[n].children.is_empty() {
            report.warnings.push(format!(
                "node `{}` has a non-uniform scale {:?}; its children are placed with an even scale of {u:.3}. Apply its scale in Blender (Ctrl+A)",
                nodes[n].name,
                nodes[n].s.to_array()
            ));
        }
        if nodes[n].s.x * nodes[n].s.y * nodes[n].s.z < 0.0 {
            report.warnings.push(format!(
                "node `{}` is mirrored by a negative scale; apply the scale in Blender (Ctrl+A) so it draws the right way out",
                nodes[n].name
            ));
        }
    }
    let parent_sigma = |n: usize| nodes[n].parent.map_or(1.0, |p| sigma[p]);
    // Unique, non-empty node names: the engine finds nodes by name,
    // ignoring case.
    let mut names = Vec::new();
    let mut taken = BTreeSet::new();
    for &n in &order {
        let base = if nodes[n].name.is_empty() {
            format!("node{n}")
        } else {
            nodes[n]
                .name
                .chars()
                .filter(|c| !c.is_control())
                .take(64)
                .collect()
        };
        let mut name = base.clone();
        let mut k = 1;
        while !taken.insert(name.to_ascii_lowercase()) {
            name = format!("{base}_{k}");
            k += 1;
        }
        if name != base {
            report.warnings.push(format!(
                "two nodes are named `{base}`; the second is now `{name}`"
            ));
        }
        names.push(name);
    }
    let mut shape_nodes: Vec<Node> = order
        .iter()
        .enumerate()
        .map(|(k, &n)| Node {
            name: names[k].clone(),
            parent: nodes[n].parent.map(|p| new_index[p]),
            translation: point(nodes[n].t * parent_sigma(n)).to_array(),
            rotation: turn(nodes[n].r).to_array(),
        })
        .collect();

    // Materials, made on first use.
    let mut materials: Vec<Material> = Vec::new();
    let mut textures: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut material_index: BTreeMap<Option<usize>, usize> = BTreeMap::new();
    let mut material_sources: Vec<Option<usize>> = Vec::new();
    let mut material_names = BTreeSet::new();
    let mut material = |index: Option<usize>, report: &mut Report| -> Result<usize> {
        if let Some(i) = material_index.get(&index) {
            return Ok(*i);
        }
        let (m, png, maps) =
            make_material(doc, folder, index, prefix, &mut material_names, report)?;
        textures.insert(m.name.clone(), png);
        report.materials.push(m.name.clone());
        let at = materials.len();
        let name = m.name.clone();
        materials.push(m);
        material_sources.push(index);
        material_index.insert(index, at);
        // Each surface map is a material of its own, which the surface names.
        for (suffix, png) in maps.named() {
            let mut helper = materials[at].clone();
            helper.name = format!("{name}_{suffix}");
            while !material_names.insert(helper.name.clone()) {
                helper.name.push('_');
            }
            helper.blend = "opaque".into();
            helper.metal = None;
            helper.surface = None;
            textures.insert(helper.name.clone(), png);
            materials.push(helper);
            material_sources.push(None);
            let k = Some(materials.len() - 1);
            let surface = materials[at]
                .surface
                .as_mut()
                .context("surface maps without a surface")?;
            match suffix {
                "normal" => surface.normal = k,
                "metal_rough" => surface.metallic_roughness = k,
                "occlusion" => surface.occlusion = k,
                "detail" => surface.detail = k,
                "lightmap" => surface.lightmap = k,
                _ => surface.emissive_map = k,
            }
        }
        Ok(at)
    };

    // Meshes: one native object per node with a mesh.
    let mut visible: Vec<(Object, Mesh)> = Vec::new();
    let mut collision: Vec<(Object, Mesh)> = Vec::new();
    let mut waters: Vec<WaterMesh> = Vec::new();
    let mut warned = BTreeSet::new();
    for (k, &n) in order.iter().enumerate() {
        let Some(m) = nodes[n].mesh else { continue };
        let gmesh = array(json, "meshes")
            .get(m)
            .with_context(|| format!("node {n} names missing mesh {m}"))?;
        let skin = nodes[n]
            .skin
            .map(|s| {
                array(json, "skins")
                    .get(s)
                    .with_context(|| format!("node {n} names missing skin {s}"))
            })
            .transpose()?;
        let joints: Vec<usize> = skin
            .map(|s| {
                s["joints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .map(|j| j as usize)
                    .collect()
            })
            .unwrap_or_default();
        ensure!(
            joints.iter().all(|j| *j < nodes.len() && seen[*j]),
            "a skin names a joint outside the scene"
        );
        let is_collision = {
            let lower = names[k].to_ascii_lowercase();
            lower.starts_with("col_") || lower.starts_with("col-") || lower.starts_with("collision")
        };
        let is_water = opts.kind == Kind::Map && names[k].to_ascii_lowercase().starts_with("water");
        // Rigid meshes take the node's own (possibly uneven) scale and its
        // parents' even scale; skinned meshes are already in model space.
        let own_scale = nodes[n].s * parent_sigma(n);
        let mut mesh = Mesh {
            frame_vertices: 0,
            positions: Vec::new(),
            normals: Vec::new(),
            uv: Vec::new(),
            primitives: Vec::new(),
            skin: None,
            billboard: false,
            billboard_y: false,
            uv1: Vec::new(),
        };
        let mut influences: Vec<Influence> = Vec::new();
        let mut unweighted = 0;
        let mut dropped_weights = 0;
        if gmesh["weights"].is_array()
            || array(gmesh, "primitives")
                .iter()
                .any(|p| p["targets"].is_array())
        {
            report.warnings.push(format!(
                "mesh of `{}` has shape keys (morph targets); they are ignored",
                names[k]
            ));
        }
        for (pi, p) in array(gmesh, "primitives").iter().enumerate() {
            let mode = p["mode"].as_u64().unwrap_or(4);
            if !matches!(mode, 4..=6) {
                report.warnings.push(format!(
                    "`{}` primitive {pi} is points or lines; skipped",
                    names[k]
                ));
                continue;
            }
            let attr = &p["attributes"];
            let positions = doc.floats(
                attr["POSITION"]
                    .as_u64()
                    .with_context(|| format!("`{}` primitive {pi} has no positions", names[k]))?
                    as usize,
            )?;
            let count = positions.len();
            let normals = attr["NORMAL"]
                .as_u64()
                .map(|a| doc.floats(a as usize))
                .transpose()?;
            let uv = attr["TEXCOORD_0"]
                .as_u64()
                .map(|a| doc.floats(a as usize))
                .transpose()?;
            let uv1 = attr["TEXCOORD_1"]
                .as_u64()
                .map(|a| doc.floats(a as usize))
                .transpose()?;
            if attr.get("COLOR_0").is_some() && warned.insert("color") {
                report.warnings.push(
                    "vertex colours are ignored; bake them into the Base Color texture".into(),
                );
            }
            if uv.is_none() && p["material"].is_u64() {
                report.warnings.push(format!(
                    "`{}` primitive {pi} has no UVs; its texture draws as one colour",
                    names[k]
                ));
            }
            let indices: Vec<u32> = match p["indices"].as_u64() {
                Some(a) => doc.uints(a as usize)?.into_iter().map(|r| r[0]).collect(),
                None => (0..count as u32).collect(),
            };
            ensure!(
                indices.iter().all(|i| (*i as usize) < count),
                "`{}` primitive {pi} has an index past its vertices",
                names[k]
            );
            let mut triangles: Vec<[u32; 3]> = match mode {
                4 => indices
                    .chunks_exact(3)
                    .map(|t| [t[0], t[1], t[2]])
                    .collect(),
                5 => (2..indices.len())
                    .map(|i| {
                        if i % 2 == 0 {
                            [indices[i - 2], indices[i - 1], indices[i]]
                        } else {
                            [indices[i - 1], indices[i - 2], indices[i]]
                        }
                    })
                    .collect(),
                _ => (2..indices.len())
                    .map(|i| [indices[0], indices[i - 1], indices[i]])
                    .collect(),
            };
            triangles.retain(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
            let base = mesh.positions.len() as u32;
            let skinned = skin.is_some() && attr.get("JOINTS_0").is_some();
            let mirrored = !skinned && own_scale.x * own_scale.y * own_scale.z < 0.0;
            let place = |v: Vec3| {
                if skinned {
                    point(v)
                } else {
                    point(v * own_scale)
                }
            };
            let local: Vec<Vec3> = positions
                .iter()
                .map(|p| Vec3::new(p[0], p[1], p[2]))
                .collect();
            let smooth;
            let normals: Vec<Vec3> = match &normals {
                Some(n) => n.iter().map(|n| Vec3::new(n[0], n[1], n[2])).collect(),
                None => {
                    let mut acc = vec![Vec3::ZERO; count];
                    for t in &triangles {
                        let [a, b, c] = t.map(|i| local[i as usize]);
                        let face = (b - a).cross(c - a);
                        for i in t {
                            acc[*i as usize] += face;
                        }
                    }
                    smooth = acc;
                    smooth.clone()
                }
            };
            for (p, nrm) in local.iter().zip(&normals) {
                mesh.positions.push(place(*p).to_array());
                let nrm = if skinned { *nrm } else { *nrm / own_scale };
                mesh.normals
                    .push(point(nrm.normalize_or(Vec3::Y)).to_array());
            }
            match &uv {
                Some(uv) => mesh.uv.extend(uv.iter().map(|u| [u[0], u[1]])),
                None => mesh.uv.extend(std::iter::repeat_n([0.0, 0.0], count)),
            }
            // The second UV set rides along only once a primitive has one; the
            // vertices before it get (0, 0), and so do later ones without.
            if uv1.is_some() || !mesh.uv1.is_empty() {
                let filled = mesh.positions.len() - count;
                mesh.uv1.resize(filled, [0.0, 0.0]);
                match &uv1 {
                    Some(uv) => mesh.uv1.extend(uv.iter().map(|u| [u[0], u[1]])),
                    None => mesh.uv1.extend(std::iter::repeat_n([0.0, 0.0], count)),
                }
            }
            if skinned {
                let joint_rows =
                    doc.uints(attr["JOINTS_0"].as_u64().unwrap_or(u64::MAX) as usize)?;
                let weight_rows = doc.floats(
                    attr["WEIGHTS_0"]
                        .as_u64()
                        .with_context(|| format!("`{}` has joints but no weights", names[k]))?
                        as usize,
                )?;
                ensure!(
                    joint_rows.len() == count && weight_rows.len() == count,
                    "`{}` joints and weights do not match its vertices",
                    names[k]
                );
                if attr.get("JOINTS_1").is_some() {
                    dropped_weights += count;
                }
                for (v, (j, w)) in joint_rows.iter().zip(&weight_rows).enumerate() {
                    let mut pairs: Vec<(usize, f32)> = j
                        .iter()
                        .zip(w)
                        .filter(|(_, w)| **w > 1e-5)
                        .map(|(j, w)| (*j as usize, *w))
                        .collect();
                    ensure!(
                        pairs.iter().all(|(j, _)| *j < joints.len()),
                        "`{}` weights a joint its skin does not have",
                        names[k]
                    );
                    let total: f32 = pairs.iter().map(|(_, w)| w).sum();
                    if pairs.is_empty() || total <= 1e-5 {
                        unweighted += 1;
                        pairs = vec![(0, 1.0)];
                    }
                    let total: f32 = pairs.iter().map(|(_, w)| w).sum();
                    for (bone, weight) in pairs {
                        influences.push(Influence {
                            vertex: base as usize + v,
                            bone,
                            weight: weight / total,
                        });
                    }
                }
                mesh.skin.get_or_insert(Skin {
                    inverse_bind: Vec::new(),
                    nodes: Vec::new(),
                    influences: Vec::new(),
                });
            } else if skin.is_some() || mesh.skin.is_some() {
                bail!(
                    "`{}` mixes skinned and unskinned parts; skin every part of it",
                    names[k]
                );
            }
            // Collision is never drawn, so it needs no texture.
            let material = if is_collision || is_water {
                None
            } else {
                Some(material(
                    p["material"].as_u64().map(|m| m as usize),
                    &mut report,
                )?)
            };
            mesh.primitives.push(Primitive {
                material,
                triangles: triangles
                    .into_iter()
                    .map(|t| {
                        let t = t.map(|i| i + base);
                        if mirrored { [t[0], t[2], t[1]] } else { t }
                    })
                    .collect(),
            });
        }
        if mesh.positions.is_empty() {
            continue;
        }
        if unweighted > 0 {
            report.warnings.push(format!(
                "{unweighted} vertices of `{}` have no bone weights and follow its first bone; weight them in Blender (Weight Paint)",
                names[k]
            ));
        }
        if dropped_weights > 0 {
            report.warnings.push(format!(
                "`{}` has more than 4 bones on some vertices; only the first 4 are used (Weights > Limit Total in Blender)",
                names[k]
            ));
        }
        mesh.frame_vertices = mesh.positions.len();
        if let Some(skin_out) = &mut mesh.skin {
            let ibm = match skin.and_then(|s| s["inverseBindMatrices"].as_u64()) {
                Some(a) => doc
                    .floats(a as usize)?
                    .into_iter()
                    .map(|m| Mat4::from_cols_slice(&m))
                    .collect(),
                None => vec![Mat4::IDENTITY; joints.len()],
            };
            ensure!(
                ibm.len() >= joints.len(),
                "`{}`'s skin has fewer bind matrices than joints",
                names[k]
            );
            for (j, joint) in joints.iter().enumerate() {
                let even = Mat4::from_scale(Vec3::splat(sigma[*joint]));
                skin_out
                    .inverse_bind
                    .push(conjugate(even * ibm[j]).to_cols_array());
                skin_out.nodes.push(new_index[*joint]);
            }
            skin_out.influences = influences;
            report.bones = report.bones.max(joints.len());
        }
        report.triangles += mesh
            .primitives
            .iter()
            .map(|p| p.triangles.len())
            .sum::<usize>();
        report.vertices += mesh.frame_vertices;
        let object = Object {
            name: names[k].clone(),
            node: Some(k),
            meshes: Vec::new(),
            visibility: 1.0,
            frame: 0,
            material_frame: 0,
            live: None,
        };
        if is_water {
            waters.push(WaterMesh {
                name: names[k].clone(),
                node: k,
                positions: mesh.positions,
                material: array(gmesh, "primitives")
                    .first()
                    .and_then(|p| p["material"].as_u64())
                    .map(|m| m as usize),
            });
        } else if is_collision {
            collision.push((object, mesh));
        } else {
            visible.push((object, mesh));
        }
    }
    ensure!(!visible.is_empty(), "the model has no visible mesh");
    if opts.kind == Kind::Map && collision.is_empty() {
        // A map with no `col_` meshes is solid where it is drawn.
        collision = visible.clone();
    }
    let mut objects = Vec::new();
    let mut meshes = Vec::new();
    let mut details = Vec::new();
    for (name, group, collision) in [
        ("detail32", visible, false),
        ("collision-1", collision, true),
    ] {
        if group.is_empty() {
            continue;
        }
        details.push(Detail {
            name: name.into(),
            pixel_threshold: if collision { -1.0 } else { 32.0 },
            object_start: objects.len(),
            object_count: group.len(),
            mesh_offset: 0,
            collision,
        });
        for (mut object, mesh) in group {
            object.meshes = vec![meshes.len()];
            meshes.push(Some(mesh));
            objects.push(object);
        }
    }

    // Animation clips.
    let mut animations = Vec::new();
    let mut stored = BTreeSet::new();
    for (ai, a) in array(json, "animations").iter().enumerate() {
        let source = a["name"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("animation{ai}"));
        match sample_clip(doc, a, &source, &nodes, &new_index, &shape_nodes, &sigma, opts)? {
            Some((animation, clip, drift)) => {
                if !stored.insert(animation.name.clone()) {
                    report.warnings.push(format!(
                        "two clips are stored as `{}`; `{source}` is left out",
                        animation.name
                    ));
                    continue;
                }
                if let Some(d) = drift {
                    report.warnings.push(format!(
                        "`{source}` moves the model {d:.2} units over one loop; animate it in place (the game moves the body)"
                    ));
                }
                animations.push(animation);
                report.clips.push(clip);
            }
            None => report.warnings.push(format!("animation `{source}` moves nothing the importer keeps (shape-key or empty channels); skipped")),
        }
    }

    let mut shape = Shape {
        schema_version: 1,
        id: format!("gltf:{id}"),
        nodes: std::mem::take(&mut shape_nodes),
        objects,
        details,
        meshes,
        materials,
        animations,
    };
    distant_detail(&mut shape, opts, &mut report)?;
    let mut scale = opts.scale.unwrap_or(1.0);
    ensure!(scale.is_finite() && scale > 0.0, "--scale must be positive");
    if scale != 1.0 {
        rescale(&mut shape, scale);
    }
    let (min, max) = bounds(&shape)?;
    if let Some(height) = opts.height {
        ensure!(
            height.is_finite() && (0.05..=64.0).contains(&height),
            "--height is 0.05 to 64 units"
        );
        let measured = max.y - min.y;
        ensure!(
            measured > 1e-5,
            "the model is flat; it cannot be scaled to a height"
        );
        rescale(&mut shape, height / measured);
        scale *= height / measured;
    }
    let (min, max) = bounds(&shape)?;
    report.scale = scale;
    report.height = max.y - min.y.min(0.0);
    report.width = (max.x - min.x).max(max.z - min.z);
    if opts.kind == Kind::Body {
        if min.y < -0.05 * report.height || min.y > 0.05 * report.height {
            report.warnings.push(format!(
                "the feet are at {:.2}, not 0: put the origin between the feet at floor level",
                min.y
            ));
        }
        if report.height > 8.0 || report.height < 0.3 {
            report.warnings.push(format!(
                "the body is {:.2} units tall (a Blockhead is 2.65); pass --height to scale it",
                report.height
            ));
        }
        sockets(&mut shape, opts, report.height, &mut report)?;
        let height = opts.box_height.unwrap_or(report.height);
        ensure!(
            height.is_finite() && height > 0.1,
            "--box-height must be above 0.1"
        );
        let width = opts.box_width.unwrap_or(height * WIDTH_FOR_HEIGHT);
        ensure!(
            width.is_finite() && width > 0.05,
            "--box-width must be above 0.05"
        );
        // Where the eye is standing in the idle clip (a rig's rest pose need
        // not be standing), scaled with the box.
        let idle = shape
            .animations
            .iter()
            .find(|a| a.name == "root" && !a.additive);
        let pose = bri_content::animation::sample(&shape, idle, 0.0)?;
        let eye = shape
            .nodes
            .iter()
            .position(|n| n.name == "Eye")
            .map_or(report.height * 0.81, |e| pose.nodes[e].w_axis.y)
            * height
            / report.height;
        report.movement = Some(Movement::new(width, height, eye));
        for (role, _, _, cost) in ROLES {
            if !report.clips.iter().any(|c| c.role.as_deref() == Some(role)) {
                report
                    .missing_roles
                    .push(format!("no `{role}` clip: {cost}"));
            }
        }
    }
    if opts.kind == Kind::Item {
        level(&mut shape, "mountPoint");
        for name in ["mountPoint", "muzzlePoint"] {
            if !shape
                .nodes
                .iter()
                .any(|n| n.name.eq_ignore_ascii_case(name))
            {
                report.warnings.push(match name {
                    "mountPoint" => {
                        "no `mountPoint` empty: the hand holds the model at its origin".to_string()
                    }
                    _ => "no `muzzlePoint` empty: shots leave from where the item is held"
                        .to_string(),
                });
            }
        }
    }
    report.points = points(&shape, opts.kind);
    if opts.kind != Kind::Map {
        // A vehicle's hulls; a map's collision is the level itself.
        report.hulls = hulls(&shape)?;
    }
    shape
        .validate()
        .context("the converted model does not validate")?;
    Ok(Converted {
        shape,
        textures,
        report,
        material_sources,
        waters,
    })
}

/// Give a body a second, distant detail: each visible object's `_LOD1`
/// twin when the model has any (an object without one draws as it is), or
/// else its meshes simplified to the triangle budget. The twins leave the
/// full detail.
fn distant_detail(shape: &mut Shape, opts: &Options, report: &mut Report) -> Result<()> {
    let Some(full) = shape.details.iter().position(|d| !d.collision) else {
        return Ok(());
    };
    let d = shape.details[full].clone();
    let range = d.object_start..d.object_start + d.object_count;
    let twin_of = |name: &str| {
        let lower = name.to_ascii_lowercase();
        lower
            .strip_suffix("_lod1")
            .or_else(|| lower.strip_suffix(".lod1"))
            .map(str::to_owned)
    };
    let twins: Vec<usize> = range
        .clone()
        .filter(|i| twin_of(&shape.objects[*i].name).is_some())
        .collect();
    let mesh_of = |shape: &Shape, i: usize| shape.objects[i].meshes.first().copied();
    let count = |shape: &Shape, i: usize| {
        mesh_of(shape, i)
            .and_then(|m| shape.meshes[m].as_ref())
            .map_or(0, lod::triangles)
    };
    let mut low: BTreeMap<usize, usize> = BTreeMap::new();
    let hand_made = !twins.is_empty();
    if hand_made {
        for &t in &twins {
            report.triangles -= count(shape, t);
            let base = twin_of(&shape.objects[t].name).expect("a twin");
            let Some(original) = range
                .clone()
                .find(|i| !twins.contains(i) && shape.objects[*i].name.eq_ignore_ascii_case(&base))
            else {
                report.warnings.push(format!(
                    "`{}` names no object `{base}` to stand in for at a distance; left out",
                    shape.objects[t].name
                ));
                continue;
            };
            let Some(slot) = mesh_of(shape, t) else {
                continue;
            };
            let Some(mut mesh) = shape.meshes[slot].clone() else {
                continue;
            };
            // A rigid twin's vertices are in its own node's frame; move
            // them into the original's.
            if mesh.skin.is_none() {
                let to = |o: usize| {
                    shape.objects[o]
                        .node
                        .map_or(Mat4::IDENTITY, |n| world(shape, n))
                };
                let m = to(original).inverse() * to(t);
                for p in &mut mesh.positions {
                    *p = m.transform_point3(Vec3::from(*p)).to_array();
                }
                for n in &mut mesh.normals {
                    *n = m
                        .transform_vector3(Vec3::from(*n))
                        .normalize_or(Vec3::Y)
                        .to_array();
                }
            }
            shape.meshes[slot] = Some(mesh);
            low.insert(original, slot);
        }
    } else {
        if opts.kind != Kind::Body {
            return Ok(());
        }
        let total: usize = range.clone().map(|i| count(shape, i)).sum();
        let target = match opts.lod {
            None | Some(0) => return Ok(()),
            Some(t) => t,
        };
        if target >= total {
            return Ok(());
        }
        for i in range.clone() {
            let Some(mesh) = mesh_of(shape, i).and_then(|m| shape.meshes[m].as_ref()) else {
                continue;
            };
            let share = (lod::triangles(mesh) * target).div_ceil(total);
            let simple = lod::simplify(mesh, share);
            shape.meshes.push(Some(simple));
            low.insert(i, shape.meshes.len() - 1);
        }
    }
    // Rebuild the object list without the twins, each kept object with
    // its full mesh and its distant one.
    let mut objects = Vec::new();
    let mut details = Vec::new();
    let mut triangles = 0;
    for (di, detail) in shape.details.iter().enumerate() {
        let start = objects.len();
        for i in detail.object_start..detail.object_start + detail.object_count {
            if twins.contains(&i) {
                continue;
            }
            let mut object = shape.objects[i].clone();
            if di == full {
                let distant = low.get(&i).copied().or(object.meshes.first().copied());
                triangles += distant
                    .and_then(|m| shape.meshes[m].as_ref())
                    .map_or(0, lod::triangles);
                object.meshes = object
                    .meshes
                    .first()
                    .copied()
                    .into_iter()
                    .chain(distant)
                    .collect();
            }
            objects.push(object);
        }
        let count = objects.len() - start;
        if di == full {
            details.push(Detail {
                name: format!("detail{}", opts.lod_pixels.round() as i32),
                pixel_threshold: opts.lod_pixels,
                object_start: start,
                object_count: count,
                mesh_offset: 0,
                collision: false,
            });
            details.push(Detail {
                name: "detail0".into(),
                pixel_threshold: 0.0,
                object_start: start,
                object_count: count,
                mesh_offset: 1,
                collision: false,
            });
        } else {
            details.push(Detail {
                object_start: start,
                object_count: count,
                ..detail.clone()
            });
        }
    }
    shape.objects = objects;
    shape.details = details;
    report.lod = Some(Lod {
        triangles,
        pixels: opts.lod_pixels,
        hand_made,
    });
    Ok(())
}

/// Turn the rest rotation of a leaf node to the model's axes, so an empty's
/// rotation in Blender does not tilt what it holds.
fn level(shape: &mut Shape, name: &str) {
    let Some(i) = shape
        .nodes
        .iter()
        .position(|n| n.name.eq_ignore_ascii_case(name))
    else {
        return;
    };
    if shape.nodes.iter().any(|n| n.parent == Some(i)) {
        return;
    }
    let parent = shape.nodes[i].parent.map_or(Quat::IDENTITY, |p| {
        world(shape, p).to_scale_rotation_translation().1
    });
    shape.nodes[i].rotation = parent.inverse().normalize().to_array();
}

/// A node's rest transform in the model's frame.
fn world(shape: &Shape, mut node: usize) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    for _ in 0..shape.nodes.len() {
        let n = &shape.nodes[node];
        m = Mat4::from_rotation_translation(
            Quat::from_array(n.rotation),
            Vec3::from(n.translation),
        ) * m;
        match n.parent {
            Some(p) => node = p,
            None => break,
        }
    }
    m
}

fn rescale(shape: &mut Shape, k: f32) {
    for n in &mut shape.nodes {
        n.translation = (Vec3::from(n.translation) * k).to_array();
    }
    for mesh in shape.meshes.iter_mut().flatten() {
        for p in &mut mesh.positions {
            *p = (Vec3::from(*p) * k).to_array();
        }
        if let Some(skin) = &mut mesh.skin {
            for m in &mut skin.inverse_bind {
                m[12] *= k;
                m[13] *= k;
                m[14] *= k;
            }
        }
    }
    for a in &mut shape.animations {
        for t in &mut a.nodes {
            for v in &mut t.translations {
                *v = (Vec3::from(*v) * k).to_array();
            }
        }
    }
}

/// The model's visible geometry as players first see it: a body standing
/// in its idle clip's first frame (a rig's rest pose need not be a pose at
/// all), anything else at rest.
fn bounds(shape: &Shape) -> Result<(Vec3, Vec3)> {
    let idle = shape
        .animations
        .iter()
        .find(|a| a.name == "root" && !a.additive);
    let pose = bri_content::animation::sample(shape, idle, 0.0)?;
    let detail = shape
        .details
        .iter()
        .position(|d| !d.collision)
        .context("no visible detail")?;
    let triangles = bri_content::animation::triangles(shape, &pose, detail, |_| true)?;
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for t in &triangles {
        for v in &t.vertices {
            min = min.min(v.position);
            max = max.max(v.position);
        }
    }
    ensure!(min.x <= max.x, "the model has no triangles");
    Ok((min, max))
}

/// The body's `Mount0`, `Mount1` and `Eye` nodes: the Blockhead's socket
/// names, which a body that names no sockets is held, aimed and seen by.
/// Each is a new child of the node the socket follows, turned so that in
/// the rest pose its axes are the body's: what it holds points where the
/// body faces whichever way the bone or empty was turned.
fn sockets(shape: &mut Shape, opts: &Options, height: f32, report: &mut Report) -> Result<()> {
    let find = |shape: &Shape, wanted: &str| {
        let wanted = wanted.to_ascii_lowercase();
        shape.nodes.iter().position(|n| {
            let name = n.name.to_ascii_lowercase();
            // Blender's duplicate suffix: `socket_eye.001`.
            name == wanted
                || name.strip_prefix(&wanted).is_some_and(|rest| {
                    rest.len() == 4
                        && rest.starts_with('.')
                        && rest[1..].bytes().all(|b| b.is_ascii_digit())
                })
        })
    };
    for (socket, node_name, flag, names) in [
        ("right_hand", "Mount0", &opts.right_hand, RIGHT_HAND),
        ("left_hand", "Mount1", &opts.left_hand, LEFT_HAND),
        ("eye", "Eye", &opts.eye, EYE),
    ] {
        let follow = match flag {
            Some(name) => Some(find(shape, name).with_context(|| {
                format!(
                    "--{} names `{name}`, which the model does not have",
                    socket.replace('_', "-")
                )
            })?),
            None => names.iter().find_map(|n| find(shape, n)),
        };
        let existing = find(shape, node_name);
        let Some(follow) = follow else {
            if let Some(e) = existing {
                report
                    .sockets
                    .insert(socket.into(), shape.nodes[e].name.clone());
                continue;
            }
            if socket == "eye" {
                shape.nodes.push(Node {
                    name: "Eye".into(),
                    parent: None,
                    translation: [0.0, height * 0.92, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                });
                report
                    .sockets
                    .insert(socket.into(), "(the body, 92% up)".into());
                report.warnings.push(
                    "no `socket_eye` empty: the first-person eye is placed 92% up the body; add one between the eyes".into(),
                );
            } else {
                report.warnings.push(format!(
                    "no `socket_{socket}` empty or hand bone: items held in that hand are not drawn; add one at the palm"
                ));
            }
            continue;
        };
        if !shape.nodes[follow]
            .name
            .to_ascii_lowercase()
            .starts_with("socket_")
            && flag.is_none()
        {
            report.warnings.push(format!(
                "no `socket_{socket}` empty: the {} follows the `{}` bone's joint; add an empty at the palm for an exact grip",
                socket.replace('_', " "),
                shape.nodes[follow].name
            ));
        }
        if let Some(e) = existing {
            let renamed = format!("{}_model", shape.nodes[e].name);
            for a in &mut shape.animations {
                for t in &mut a.nodes {
                    if t.node.eq_ignore_ascii_case(&shape.nodes[e].name) {
                        t.node = renamed.clone();
                    }
                }
            }
            shape.nodes[e].name = renamed;
        }
        let rotation = world(shape, follow).to_scale_rotation_translation().1;
        shape.nodes.push(Node {
            name: node_name.into(),
            parent: Some(follow),
            translation: [0.0; 3],
            rotation: rotation.inverse().normalize().to_array(),
        });
        report
            .sockets
            .insert(socket.into(), shape.nodes[follow].name.clone());
    }
    Ok(())
}

/// Rest positions of the nodes other files refer to: an item's grip and
/// muzzle, a vehicle's seats (`mount0`...) and wheel hubs (`hub0`...).
fn points(shape: &Shape, kind: Kind) -> Vec<Point> {
    let wanted = |name: &str| {
        let n = name.to_ascii_lowercase();
        match kind {
            Kind::Body | Kind::Map | Kind::Brick => false,
            Kind::Item => matches!(
                n.as_str(),
                "mountpoint" | "muzzlepoint" | "ejectpoint" | "supportpoint"
            ),
            Kind::Prop => [
                "mount",
                "hub",
                "mountpoint",
                "muzzlepoint",
                "eye",
                "pivot",
                "turret",
            ]
            .iter()
            .any(|p| {
                n.strip_prefix(p)
                    .is_some_and(|rest| rest.is_empty() || rest.bytes().all(|b| b.is_ascii_digit()))
            }),
        }
    };
    shape
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| wanted(&n.name))
        .map(|(i, n)| {
            let p = world(shape, i).w_axis.truncate();
            Point {
                node: n.name.clone(),
                position: [round(p.x), round(p.y), round(p.z)],
            }
        })
        .collect()
}
/// The distinct corners of each collision object, posed at rest.
fn hulls(shape: &Shape) -> Result<Vec<Hull>> {
    let Some(detail) = shape.details.iter().position(|d| d.collision) else {
        return Ok(Vec::new());
    };
    let pose = bri_content::animation::sample(shape, None, 0.0)?;
    let triangles = bri_content::animation::triangles(shape, &pose, detail, |_| true)?;
    let mut out: Vec<Hull> = Vec::new();
    for t in &triangles {
        let name = &shape.objects[t.object].name;
        if out.last().is_none_or(|h| &h.object != name) {
            out.push(Hull {
                object: name.clone(),
                points: Vec::new(),
            });
        }
        let hull = out.last_mut().expect("pushed");
        for v in &t.vertices {
            let p = v.position.to_array().map(round);
            if !hull.points.contains(&p) {
                hull.points.push(p);
            }
        }
    }
    Ok(out)
}
fn round(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

/// One glTF animation as a native clip, with its report line and, for a
/// loop, how far its root drifts. `None` when nothing it animates is kept.
#[allow(clippy::too_many_arguments)]
fn sample_clip(
    doc: &Document,
    a: &Value,
    source: &str,
    nodes: &[GNode],
    new_index: &[usize],
    shape_nodes: &[Node],
    sigma: &[f32],
    opts: &Options,
) -> Result<Option<(Animation, Clip, Option<f32>)>> {
    struct Channel {
        node: usize,
        path: &'static str,
        times: Vec<f32>,
        values: Vec<Vec<f32>>,
        interpolation: String,
    }
    let mut channels = Vec::new();
    for c in array(a, "channels") {
        let Some(node) = c["target"]["node"].as_u64().map(|n| n as usize) else {
            continue;
        };
        if node >= nodes.len() || new_index[node] == usize::MAX {
            continue;
        }
        let path = match c["target"]["path"].as_str() {
            Some("translation") => "translation",
            Some("rotation") => "rotation",
            Some("scale") => "scale",
            _ => continue,
        };
        let sampler = array(a, "samplers")
            .get(c["sampler"].as_u64().unwrap_or(u64::MAX) as usize)
            .with_context(|| format!("`{source}` names a missing sampler"))?;
        let times: Vec<f32> = doc
            .floats(sampler["input"].as_u64().context("sampler without input")? as usize)?
            .into_iter()
            .map(|r| r[0])
            .collect();
        let values = doc.floats(
            sampler["output"]
                .as_u64()
                .context("sampler without output")? as usize,
        )?;
        let interpolation = sampler["interpolation"]
            .as_str()
            .unwrap_or("LINEAR")
            .to_string();
        let per = if interpolation == "CUBICSPLINE" { 3 } else { 1 };
        ensure!(
            !times.is_empty() && values.len() == times.len() * per,
            "`{source}` has a sampler whose times and values do not match"
        );
        channels.push(Channel {
            node,
            path,
            times,
            values,
            interpolation,
        });
    }
    if channels.is_empty() {
        return Ok(None);
    }
    let start = channels.iter().map(|c| c.times[0]).fold(f32::MAX, f32::min);
    let end = channels
        .iter()
        .map(|c| *c.times.last().unwrap())
        .fold(f32::MIN, f32::max);
    let duration = (end - start).max(0.0);
    let found = role(source);
    let lower = safe_name(source);
    let (name, play) = match found {
        Some((_, clip, play, _)) => (clip.to_string(), *play),
        None => (
            lower.clone(),
            Play::Absolute {
                looping: opts.loops.iter().any(|l| safe_name(l) == lower),
                priority: 12,
            },
        ),
    };
    let looping = matches!(play, Play::Absolute { looping: true, .. });
    let frames = if duration <= 1e-4 {
        1
    } else if looping {
        ((duration * opts.fps).round() as usize).max(1)
    } else {
        ((duration * opts.fps).round() as usize).max(1) + 1
    };
    ensure!(frames <= 100_000, "`{source}` is too long");
    let time = |i: usize| {
        if frames == 1 {
            start
        } else if looping {
            start + duration * i as f32 / frames as f32
        } else {
            start + duration * i as f32 / (frames - 1) as f32
        }
    };
    // Each animated node's local transform at every frame, rest where a
    // path is not animated.
    let animated: BTreeSet<usize> = channels.iter().map(|c| c.node).collect();
    let mut tracks = Vec::new();
    let mut root_drift = None;
    for node in animated {
        let g = &nodes[node];
        let mut t = vec![g.t; frames];
        let mut r = vec![g.r; frames];
        let mut s = vec![g.s; frames];
        let mut paths = BTreeSet::new();
        for c in channels.iter().filter(|c| c.node == node) {
            paths.insert(c.path);
            for i in 0..frames {
                let v = sample(
                    c.times.as_slice(),
                    &c.values,
                    &c.interpolation,
                    c.path == "rotation",
                    time(i),
                );
                match c.path {
                    "translation" => t[i] = Vec3::new(v[0], v[1], v[2]),
                    "rotation" => r[i] = Quat::from_xyzw(v[0], v[1], v[2], v[3]).normalize(),
                    _ => s[i] = Vec3::new(v[0], v[1], v[2]),
                }
            }
            if looping
                && c.path == "translation"
                && g.parent.is_none_or(|p| nodes[p].parent.is_none())
            {
                let first = sample(&c.times, &c.values, &c.interpolation, false, start);
                let last = sample(&c.times, &c.values, &c.interpolation, false, end);
                let d = Vec3::new(last[0] - first[0], 0.0, last[2] - first[2]).length()
                    * g.parent.map_or(1.0, |p| sigma[p]);
                if d > 0.05 {
                    root_drift = Some(root_drift.unwrap_or(0.0f32).max(d));
                }
            }
        }
        let k = new_index[node];
        let parent = g.parent.map_or(1.0, |p| sigma[p]);
        let nt: Vec<Vec3> = t.iter().map(|t| point(*t * parent)).collect();
        let nr: Vec<Quat> = r.iter().map(|r| turn(*r)).collect();
        let mut track = NodeTrack {
            node: shape_nodes[k].name.clone(),
            rotations: Vec::new(),
            translations: Vec::new(),
            scales: Vec::new(),
            scale_rotations: Vec::new(),
        };
        match play {
            // Every channel the clip has is an override, even one that holds
            // the rest pose: an aim clip keeping an arm still must stop the
            // run below it swinging that arm. Bones without a channel are
            // left to the layer below.
            Play::Absolute { .. } => {
                if paths.contains("translation") {
                    track.translations = nt.iter().map(|v| v.to_array()).collect();
                }
                if paths.contains("rotation") {
                    track.rotations = nr.iter().map(|q| q.to_array()).collect();
                }
                if paths.contains("scale") {
                    track.scales = s
                        .iter()
                        .map(|v| (*v / g.s.max(Vec3::splat(1e-6))).abs().to_array())
                        .collect();
                }
            }
            Play::Overlay { reference } => {
                let at = ((frames - 1) as f32 * reference).round() as usize;
                let (rt, rr) = (nt[at], nr[at]);
                let rot: Vec<Quat> = nr.iter().map(|q| (rr.inverse() * *q).normalize()).collect();
                let tr: Vec<Vec3> = nt.iter().map(|t| rr.inverse() * (*t - rt)).collect();
                if rot.iter().any(|q| q.dot(Quat::IDENTITY).abs() < 1.0 - 1e-6) {
                    track.rotations = rot.iter().map(|q| q.to_array()).collect();
                }
                if tr.iter().any(|v| v.length() > 1e-4) {
                    track.translations = tr.iter().map(|v| v.to_array()).collect();
                }
            }
        }
        if !(track.rotations.is_empty() && track.translations.is_empty() && track.scales.is_empty())
        {
            tracks.push(track);
        }
    }
    let (priority, additive) = match play {
        Play::Absolute { priority, .. } => (priority, false),
        Play::Overlay { .. } => (0, true),
    };
    let animation = Animation {
        name: name.clone(),
        frames,
        duration: if frames == 1 { 0.0 } else { duration },
        looping,
        additive,
        parent_translations: false,
        priority,
        nodes: tracks,
        objects: Vec::new(),
        rotation_interpolation: Default::default(),
        ground_translations: Vec::new(),
        ground_rotations: Vec::new(),
        triggers: Vec::new(),
    };
    let clip = Clip {
        source: source.into(),
        name,
        role: found.map(|r| r.0.to_string()),
        frames,
        duration: animation.duration,
        looping,
        additive,
    };
    Ok(Some((animation, clip, root_drift.filter(|_| looping))))
}

/// A sampler's value at `t`, clamped to its keys.
fn sample(
    times: &[f32],
    values: &[Vec<f32>],
    interpolation: &str,
    rotation: bool,
    t: f32,
) -> Vec<f32> {
    let cubic = interpolation == "CUBICSPLINE";
    let value = |k: usize| -> Vec<f32> {
        if cubic {
            values[k * 3 + 1].clone()
        } else {
            values[k].clone()
        }
    };
    if t <= times[0] {
        return value(0);
    }
    let last = times.len() - 1;
    if t >= times[last] {
        return value(last);
    }
    let k = times.partition_point(|x| *x <= t) - 1;
    let (t0, t1) = (times[k], times[k + 1]);
    let dt = t1 - t0;
    let u = if dt > 0.0 { (t - t0) / dt } else { 0.0 };
    let (a, b) = (value(k), value(k + 1));
    let out: Vec<f32> = match interpolation {
        "STEP" => a,
        "CUBICSPLINE" => {
            let (u2, u3) = (u * u, u * u * u);
            let out_tangent = &values[k * 3 + 2];
            let in_tangent = &values[(k + 1) * 3];
            (0..a.len())
                .map(|i| {
                    (2.0 * u3 - 3.0 * u2 + 1.0) * a[i]
                        + (u3 - 2.0 * u2 + u) * dt * out_tangent[i]
                        + (-2.0 * u3 + 3.0 * u2) * b[i]
                        + (u3 - u2) * dt * in_tangent[i]
                })
                .collect()
        }
        _ if rotation => {
            let qa = Quat::from_slice(&a).normalize();
            let qb = Quat::from_slice(&b).normalize();
            qa.slerp(qb, u).to_array().to_vec()
        }
        _ => a.iter().zip(&b).map(|(x, y)| x + (y - x) * u).collect(),
    };
    if rotation {
        Quat::from_slice(&out).normalize().to_array().to_vec()
    } else {
        out
    }
}

/// A material and its PNG: the base colour texture times the base colour
/// factor, or a swatch of the factor alone.
fn make_material(
    doc: &Document,
    folder: Option<&Path>,
    index: Option<usize>,
    prefix: &str,
    taken: &mut BTreeSet<String>,
    report: &mut Report,
) -> Result<(Material, Vec<u8>, SurfaceMaps)> {
    let null = Value::Null;
    let m = index.map_or(&null, |i| {
        array(&doc.json, "materials").get(i).unwrap_or(&null)
    });
    let source = m["name"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| match index {
            Some(i) => format!("material{i}"),
            None => "default".into(),
        });
    let base = format!("{prefix}{}", safe_name(&source));
    let mut name = base.clone();
    let mut k = 1;
    while !taken.insert(name.clone()) {
        name = format!("{base}_{k}");
        k += 1;
    }
    let pbr = &m["pbrMetallicRoughness"];
    let factor: Vec<f32> = pbr["baseColorFactor"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_f64().unwrap_or(1.0) as f32).collect())
        .unwrap_or_else(|| vec![1.0; 4]);
    ensure!(
        factor.len() == 4,
        "material `{source}` has a bad base colour"
    );
    // The factor is linear; the texture is sRGB.
    let srgb = |c: f32| {
        let c = c.clamped(0.0, 1.0);
        if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    };
    let tint = [
        srgb(factor[0]),
        srgb(factor[1]),
        srgb(factor[2]),
        factor[3].clamped(0.0, 1.0),
    ];
    let mut wrap = (true, true);
    let pixels = match pbr.get("baseColorTexture") {
        Some(info) => {
            let (mut rgba, sampler) = texture_image(doc, folder, info, &source, report)?;
            wrap = sampler;
            if tint != [1.0; 4] {
                for p in rgba.pixels_mut() {
                    for (channel, t) in p.0.iter_mut().zip(tint) {
                        *channel = (*channel as f32 * t).round() as u8;
                    }
                }
            }
            rgba
        }
        None => {
            image::RgbaImage::from_pixel(4, 4, image::Rgba(tint.map(|c| (c * 255.0).round() as u8)))
        }
    };
    // glTF ignores an OPAQUE material's alpha, and exporters leave masks and
    // specular maps in it; in a native shape an opaque texture with a
    // translucent texel is laid over the model's colour (v20's decal),
    // which ignores the world's light. MASK retains its alpha cutoff and depth writes.
    let mut pixels = pixels;
    let blend = match m["alphaMode"].as_str() {
        Some("BLEND") => "alpha",
        Some("MASK") => "opaque",
        _ => {
            for p in pixels.pixels_mut() {
                p.0[3] = 255;
            }
            "opaque"
        }
    };
    // Glow: emissiveFactor times KHR_materials_emissive_strength, linear.
    let strength = m["extensions"]["KHR_materials_emissive_strength"]["emissiveStrength"]
        .as_f64()
        .unwrap_or(1.0) as f32;
    let emissive: [f32; 3] = match m["emissiveFactor"].as_array() {
        Some(a) if a.len() == 3 => {
            [0, 1, 2].map(|c| (a[c].as_f64().unwrap_or(0.0) as f32 * strength).clamped(0.0, 1000.0))
        }
        _ => [0.0; 3],
    };
    let glow = emissive.iter().copied().fold(0.0, f32::max);
    let emissive_map = m.get("emissiveTexture").filter(|_| glow > 0.0);
    // A material that glows at half strength or more over its whole surface
    // draws self-lit, at its own colour whatever the world's light.
    let unlit = m["extensions"].get("KHR_materials_unlit").is_some()
        || (glow >= 0.5 && emissive_map.is_none());
    let mut maps = SurfaceMaps::default();
    let png_of = |info: &Value, report: &mut Report| -> Result<Vec<u8>> {
        let (rgba, _) = texture_image(doc, folder, info, &source, report)?;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
        Ok(png)
    };
    if !unlit {
        if let Some(info) = m.get("normalTexture") {
            maps.normal = Some(png_of(info, report)?);
        }
        if let Some(info) = pbr.get("metallicRoughnessTexture") {
            maps.metallic_roughness = Some(png_of(info, report)?);
        }
        if let Some(info) = m.get("occlusionTexture") {
            maps.occlusion = Some(png_of(info, report)?);
        }
        if let Some(info) = emissive_map {
            maps.emissive = Some(png_of(info, report)?);
        }
        // `extras.bri_detail`: a texture reference like glTF's own (its
        // `index`), plus `scale`, how many times it repeats across the
        // colour map.
        if m["extras"]["bri_detail"].is_object() {
            maps.detail = Some(png_of(&m["extras"]["bri_detail"], report)?);
        }
        // `extras.bri_lightmap`: a baked lightmap the second UV set reads,
        // as a texture reference (`index`) plus `strength`, its multiplier.
        // It takes the place of the sun and ambient light on the surface.
        if m["extras"]["bri_lightmap"].is_object() {
            maps.lightmap = Some(png_of(&m["extras"]["bri_lightmap"], report)?);
        }
    }
    let textured = maps.normal.is_some()
        || maps.metallic_roughness.is_some()
        || maps.occlusion.is_some()
        || maps.emissive.is_some()
        || maps.detail.is_some()
        || maps.lightmap.is_some();
    // glTF's default metallic factor is 1, but exporters write 0 for every
    // plain surface: without a metal map only a stated factor counts.
    let metallic = match pbr["metallicFactor"].as_f64() {
        Some(f) => f as f32,
        None if maps.metallic_roughness.is_some() => 1.0,
        None => 0.0,
    }
    .clamped(0.0, 1.0);
    let roughness = (pbr["roughnessFactor"].as_f64().unwrap_or(1.0) as f32).clamped(0.0, 1.0);
    // The full look when the material has maps or a glow of its own;
    // otherwise bare metal from a stated metallic factor of 0.5 or more.
    // The baked texture already carries the base colour, which for metal
    // is its reflectance.
    let surface = (!unlit && (textured || glow > 0.0)).then(|| Surface {
        normal: None,
        normal_scale: (m["normalTexture"]["scale"].as_f64().unwrap_or(1.0) as f32)
            .clamped(0.0, 8.0),
        metallic_roughness: None,
        metallic,
        roughness,
        occlusion: None,
        occlusion_strength: (m["occlusionTexture"]["strength"].as_f64().unwrap_or(1.0) as f32)
            .clamped(0.0, 1.0),
        emissive_map: None,
        emissive,
        reflection: None,
        detail: None,
        detail_scale: (m["extras"]["bri_detail"]["scale"].as_f64().unwrap_or(1.0) as f32)
            .clamped(0.01, 4096.0),
        lightmap: None,
        lightmap_strength: (m["extras"]["bri_lightmap"]["strength"]
            .as_f64()
            .unwrap_or(1.0) as f32)
            .clamped(0.0, 16.0),
    });
    let metal = pbr["metallicFactor"]
        .as_f64()
        .filter(|f| *f >= 0.5 && !unlit && surface.is_none())
        .map(|_| Metal {
            color: [1.0; 3],
            roughness,
            detail: None,
            detail_scale: 1.0,
            detail_strength: 1.0,
        });
    let uv_motion = if m["extras"]["bri_uv_motion"].is_null() {
        None
    } else {
        let motion: bri_content::shape::UvMotion =
            serde_json::from_value(m["extras"]["bri_uv_motion"].clone())
                .context("Material bri_uv_motion")?;
        ensure!(motion.valid(), "Invalid material UV motion");
        Some(motion)
    };
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    Ok((
        Material {
            name,
            wrap_u: wrap.0,
            wrap_v: wrap.1,
            blend: blend.into(),
            unlit,
            environment: false,
            mipmaps: true,
            detail_map: None,
            bump_map: None,
            reflectance_map: None,
            detail_scale: 1.0,
            reflectance: 0.0,
            paint_overlay: Some(false),
            alpha_cutoff: (m["alphaMode"].as_str() == Some("MASK"))
                .then(|| m["alphaCutoff"].as_f64().unwrap_or(0.5) as f32),
            uv_motion,
            metal,
            surface,
        },
        png,
        maps,
    ))
}

/// A material's texture (`info` a glTF textureInfo) as pixels, at most
/// 2048 on a side, with whether it repeats along u and v.
fn texture_image(
    doc: &Document,
    folder: Option<&Path>,
    info: &Value,
    source: &str,
    report: &mut Report,
) -> Result<(image::RgbaImage, (bool, bool))> {
    let t = info["index"]
        .as_u64()
        .with_context(|| format!("material `{source}` has a texture with no index"))?;
    if info["texCoord"].as_u64().unwrap_or(0) != 0 {
        report.warnings.push(format!(
            "material `{source}` uses a second UV map; the first is used"
        ));
    }
    if info["extensions"].get("KHR_texture_transform").is_some() {
        report.warnings.push(format!(
            "material `{source}` has a texture transform, which is ignored; apply it to the UVs"
        ));
    }
    let texture = array(&doc.json, "textures")
        .get(t as usize)
        .with_context(|| format!("material `{source}` names missing texture {t}"))?;
    let image = texture["source"].as_u64().with_context(|| {
        format!("material `{source}`'s texture is not PNG or JPEG (KTX2 and WebP are not read)")
    })? as usize;
    let mut wrap = (true, true);
    if let Some(s) = texture["sampler"]
        .as_u64()
        .and_then(|s| array(&doc.json, "samplers").get(s as usize))
    {
        wrap = (
            s["wrapS"].as_u64().unwrap_or(10497) != 33071,
            s["wrapT"].as_u64().unwrap_or(10497) != 33071,
        );
    }
    let bytes = doc.image(image, folder)?;
    let mut rgba = image::load_from_memory(&bytes)
        .with_context(|| format!("material `{source}`'s texture is not a PNG or JPEG"))?
        .to_rgba8();
    if rgba.width().max(rgba.height()) > 2048 {
        let k = 2048.0 / rgba.width().max(rgba.height()) as f32;
        let (w, h) = (
            ((rgba.width() as f32 * k) as u32).max(1),
            ((rgba.height() as f32 * k) as u32).max(1),
        );
        report.warnings.push(format!(
            "material `{source}`'s texture is {}x{}; scaled to {w}x{h}",
            rgba.width(),
            rgba.height()
        ));
        rgba = image::imageops::resize(&rgba, w, h, image::imageops::FilterType::Triangle);
    }
    Ok((rgba, wrap))
}

/// The textures of a material's [`Surface`], as PNGs: each becomes a
/// material of its own, named after the material with the suffix, whose
/// index the surface takes.
#[derive(Default)]
pub(crate) struct SurfaceMaps {
    pub normal: Option<Vec<u8>>,
    pub metallic_roughness: Option<Vec<u8>>,
    pub occlusion: Option<Vec<u8>>,
    pub emissive: Option<Vec<u8>>,
    pub detail: Option<Vec<u8>>,
    pub lightmap: Option<Vec<u8>>,
}
impl SurfaceMaps {
    /// Each map with the suffix its material takes.
    pub fn named(self) -> Vec<(&'static str, Vec<u8>)> {
        [
            ("normal", self.normal),
            ("metal_rough", self.metallic_roughness),
            ("occlusion", self.occlusion),
            ("glow", self.emissive),
            ("detail", self.detail),
            ("lightmap", self.lightmap),
        ]
        .into_iter()
        .filter_map(|(k, png)| Some((k, png?)))
        .collect()
    }
}

impl Report {
    pub fn markdown(&self) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let kind = match self.kind {
            Some(Kind::Body) => "body",
            Some(Kind::Item) => "item",
            Some(Kind::Map) => "map",
            Some(Kind::Brick) => "brick",
            _ => "model",
        };
        let _ = writeln!(s, "# {} ({kind}) from {}\n", self.id, self.source);
        let _ = writeln!(
            s,
            "{} triangles, {} vertices, {} bones; {:.2} units tall, {:.2} wide (scale {:.4}). A Blockhead is 2.65 tall and 1.25 wide.\n",
            self.triangles, self.vertices, self.bones, self.height, self.width, self.scale
        );
        let _ = writeln!(s, "Model: `{}`\n", self.model);
        if let Some(m) = &self.movement {
            let _ = writeln!(
                s,
                "Collision box: {} wide, {} tall ({} crouched); eye at {} (set with --box-width and --box-height, or in the archetype's `movement`).\n",
                m.width, m.stand_height, m.crouch_height, m.stand_eye
            );
        }
        if let Some(lod) = &self.lod {
            let _ = writeln!(
                s,
                "Distant detail: {} triangles ({}), drawn below {} pixels tall.\n",
                lod.triangles,
                if lod.hand_made {
                    "the `_LOD1` objects"
                } else {
                    "generated"
                },
                lod.pixels
            );
        }
        if !self.sockets.is_empty() {
            let _ = writeln!(s, "## Sockets\n");
            for (socket, node) in &self.sockets {
                let _ = writeln!(s, "- `{socket}` follows `{node}`");
            }
            let _ = writeln!(s);
        }
        if !self.clips.is_empty() {
            let _ = writeln!(s, "## Clips\n");
            for c in &self.clips {
                let how = if c.additive {
                    "overlay"
                } else if c.looping {
                    "loop"
                } else {
                    "once"
                };
                let role = c
                    .role
                    .as_deref()
                    .map(|r| format!(" (role `{r}`)"))
                    .unwrap_or_default();
                let _ = writeln!(
                    s,
                    "- `{}` → `{}`{role}: {} frames, {:.2} s, {how}",
                    c.source, c.name, c.frames, c.duration
                );
            }
            let _ = writeln!(s);
        }
        if !self.missing_roles.is_empty() {
            let _ = writeln!(s, "## Missing animations\n");
            for m in &self.missing_roles {
                let _ = writeln!(s, "- {m}");
            }
            let _ = writeln!(s);
        }
        if !self.points.is_empty() {
            let heading = if self.kind == Some(Kind::Map) {
                "## Spawn points"
            } else {
                "## Points (for weapons.json or vehicles.json)"
            };
            let _ = writeln!(s, "{heading}\n");
            for p in &self.points {
                let _ = writeln!(s, "- `{}` at {:?}", p.node, p.position);
            }
            let _ = writeln!(s);
        }
        if !self.teleporters.is_empty() {
            let _ = writeln!(s, "## Teleporters\n");
            for t in &self.teleporters {
                let _ = writeln!(s, "- {t}");
            }
            let _ = writeln!(s);
        }
        if !self.hulls.is_empty() {
            let _ = writeln!(
                s,
                "## Collision hulls (for vehicles.json `collision_hulls`)\n"
            );
            for h in &self.hulls {
                let _ = writeln!(
                    s,
                    "- `{}`: {} corners {}",
                    h.object,
                    h.points.len(),
                    serde_json::to_string(&h.points).unwrap_or_default()
                );
            }
            let _ = writeln!(s);
        }
        if !self.warnings.is_empty() {
            let _ = writeln!(s, "## Warnings\n");
            for w in &self.warnings {
                let _ = writeln!(s, "- {w}");
            }
            let _ = writeln!(s);
        }
        if !self.files.is_empty() {
            let _ = writeln!(s, "## Files\n");
            for f in &self.files {
                let _ = writeln!(s, "- {f}");
            }
        }
        s
    }
}

#[cfg(test)]
mod tests;
