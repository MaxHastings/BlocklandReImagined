//! Item presentation packs for item rendering tests: the generated v20
//! packs, or a synthetic one written here (models built in code, textures
//! drawn in code, a weapons pack made from our own Bubble Blaster sample)
//! that `ItemAssets::load` reads through the same checks.
#![allow(dead_code)]

use super::files::{png, repo_root, scratch, write, write_json};
use bri_content::testing::{material, plain, rigid_shape};
use anyhow::{Context, Result};
use glam::Vec3;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub struct ItemFixture {
    /// The folder holding `presentation.json`.
    pub presentation: PathBuf,
    /// The folder holding `weapons.json`.
    pub weapons: PathBuf,
    /// Whether this is the generated v20 content.
    pub content: bool,
    /// Items with a model and an icon.
    pub modelled: Vec<String>,
    /// Items and the tint their presentation authors.
    pub tints: Vec<(String, [f32; 4])>,
    /// An image held in first person at its eye offset (the hammer).
    pub eye_offset_image: String,
    /// An image whose `eyeRotation` is written `eulerToMatrix(...)`, and a
    /// direction in its model's frame with where first person points it.
    pub euler_image: (String, Vec3, Vec3),
    /// Images on mount 0 and mount 1 with the same in-hand placement and
    /// no eye offset: first person holds them at the host's mounts.
    pub right_image: String,
    pub left_image: String,
    /// An item whose model has a `fire` sequence moving what everyone sees
    /// and a `muzzlePoint` node.
    pub gun_item: String,
    /// A model key the pack has that is not `gun_item`'s.
    pub other_model: String,
    /// A spray can model, and the one whose body is translucent.
    pub solid_can: String,
    pub clear_can: String,
    /// A static item drawn untinted whose model is not `gun_item`'s.
    pub other_item: String,
    /// An image whose `Fire` state plays a sequence moving what first
    /// person sees: (image, a node the sequence moves, the sequence).
    pub swing_image: (String, String, String),
    /// An image whose `Fire` sequence hides every object for a moment.
    pub throw_image: String,
    /// A projectile with a model.
    pub arrow: String,
    /// An image whose model has no `muzzlePoint`.
    pub no_muzzle_image: String,
    /// Where regenerated evidence (galleries, reports) goes.
    pub out: PathBuf,
    _scratch: Option<tempfile::TempDir>,
}

const NS: &str = "fixture";

impl ItemFixture {
    pub fn content() -> Result<Self> {
        let root = repo_root();
        let id = |s: &str| format!("v20.weapon.{s}");
        let image = |s: &str| format!("v20.image.{s}");
        Ok(Self {
            presentation: root.join("content/item-presentation-pack-010"),
            weapons: root.join("content/weapons-pack-009"),
            content: true,
            modelled: [
                "hammeritem",
                "wrenchitem",
                "printgun",
                "wanditem",
                "gunitem",
                "akimbogunitem",
            ]
            .map(id)
            .into(),
            tints: vec![
                (id("bluekeyitem"), [0., 0., 1., 1.]),
                (id("pushbroomitem"), [102. / 255., 50. / 255., 0., 1.]),
            ],
            eye_offset_image: image("hammerimage"),
            // Original Ski eyeRotation=eulerToMatrix("90 -90 0").
            // MatrixCreateFromEuler goes through QuatF(EulerF), giving
            // Rz*Rx*Ry: the ski's length (source +Y, native -Z) stands up
            // in first person instead of lying sideways.
            euler_image: (image("skiweaponimage"), -Vec3::Z, Vec3::Y),
            right_image: image("gunimage"),
            left_image: image("lefthandedgunimage"),
            gun_item: id("gunitem"),
            other_model: "base/data/shapes/wand.dts".into(),
            solid_can: "base/data/shapes/spraycan.dts".into(),
            clear_can: "base/data/shapes/transspraycan.dts".into(),
            other_item: id("bowitem"),
            swing_image: (image("hammerimage"), "FPhammer9999".into(), "Fire".into()),
            throw_image: image("spearimage"),
            arrow: "v20.projectile.arrowprojectile".into(),
            no_muzzle_image: image("brickimage"),
            out: root.join("artifacts"),
            _scratch: None,
        })
    }

    pub fn synthetic() -> Result<Self> {
        let dir = scratch("item-presentation-")?;
        let weapons = dir.path().join("weapons");
        let presentation = dir.path().join("items");
        write_synthetic(&weapons, &presentation)?;
        let id = |s: &str| format!("{NS}:weapon/{s}");
        let image = |s: &str| format!("{NS}:image/{s}");
        Ok(Self {
            presentation,
            weapons,
            content: false,
            modelled: ["hammer", "key", "gun", "ski"].map(id).into(),
            tints: vec![(id("key"), KEY_TINT)],
            eye_offset_image: image("hammer"),
            // eulerToMatrix("90 0 90") is Rz(90)*Rx(90)*Ry(0) in source
            // space (z up): source +X turns to source +Y (Rx leaves it,
            // Rz turns it), which is native -Z. Native +X is source +X.
            euler_image: (image("ski"), Vec3::X, -Vec3::Z),
            right_image: image("gun"),
            left_image: image("left_gun"),
            gun_item: id("gun"),
            other_model: model("key"),
            solid_can: model("spraycan"),
            clear_can: model("transspraycan"),
            other_item: id("hammer"),
            swing_image: (image("swing"), "head".into(), "swing".into()),
            throw_image: image("throw"),
            arrow: format!("{NS}:projectile/pellet"),
            no_muzzle_image: image("hammer"),
            out: PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("item-rendering-synthetic"),
            _scratch: Some(dir),
        })
    }
}

const KEY_TINT: [f32; 4] = [0.1, 0.3, 0.9, 1.];

/// A model key, as presentation keys are (lowercase source paths).
fn model(name: &str) -> String {
    format!("{NS}/shapes/{name}.dts")
}

/// One box of a model: the node it hangs from, its centre and half size
/// in that node's frame, and its material.
struct Part {
    node: usize,
    centre: [f32; 3],
    half: [f32; 3],
    material: usize,
}

/// A made-up model: named nodes (parent, rest translation), boxes, and
/// materials as (name, blend), one texture each.
struct Model {
    name: &'static str,
    nodes: Vec<(&'static str, Option<usize>, [f32; 3])>,
    parts: Vec<Part>,
    materials: Vec<(&'static str, &'static str)>,
    animations: Value,
}

impl Model {
    fn rest(&self, node: usize) -> Vec3 {
        let (_, parent, t) = self.nodes[node];
        Vec3::from(t) + parent.map_or(Vec3::ZERO, |p| self.rest(p))
    }
    /// The model's object box at rest.
    fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let (mut min, mut max) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
        for p in &self.parts {
            let c = self.rest(p.node) + Vec3::from(p.centre);
            min = min.min(c - Vec3::from(p.half));
            max = max.max(c + Vec3::from(p.half));
        }
        (min.to_array(), max.to_array())
    }
    /// The model as a native shape (`bri_content::testing::rigid_shape`'s
    /// boxes), with its animations.
    fn shape(&self) -> Result<Value> {
        let parts: Vec<_> = self
            .parts
            .iter()
            .map(|p| (p.node, p.centre, p.half, plain(p.material)))
            .collect();
        let mut shape = rigid_shape(
            self.name,
            &self.nodes,
            &parts,
            self.materials
                .iter()
                .map(|(name, blend)| material(name, blend))
                .collect(),
        );
        shape.animations = serde_json::from_value(self.animations.clone())?;
        Ok(serde_json::to_value(shape)?)
    }
}

fn models() -> Vec<Model> {
    let part = |node, centre, half, material| Part {
        node,
        centre,
        half,
        material,
    };
    let still = || json!([]);
    vec![
        Model {
            name: "hammer",
            nodes: vec![
                ("root", None, [0.; 3]),
                ("mountPoint", Some(0), [0., -0.1, 0.]),
            ],
            parts: vec![
                part(0, [0., 0., 0.], [0.03, 0.2, 0.03], 0),
                part(0, [0., 0.2, 0.], [0.06, 0.05, 0.12], 1),
            ],
            materials: vec![("handle", "opaque"), ("head", "opaque")],
            animations: still(),
        },
        Model {
            name: "key",
            nodes: vec![("root", None, [0.; 3])],
            parts: vec![part(0, [0., 0., 0.], [0.02, 0.1, 0.05], 0)],
            materials: vec![("key", "opaque")],
            animations: still(),
        },
        Model {
            name: "gun",
            nodes: vec![
                ("root", None, [0.; 3]),
                ("mountPoint", Some(0), [0., -0.05, 0.05]),
                ("barrel", Some(0), [0., 0.05, -0.1]),
                ("muzzlePoint", Some(2), [0., 0., -0.2]),
            ],
            parts: vec![
                part(0, [0., -0.05, 0.05], [0.03, 0.08, 0.04], 0),
                part(2, [0., 0., -0.05], [0.025, 0.025, 0.15], 0),
            ],
            materials: vec![("gun", "opaque")],
            // The barrel kicks back and returns.
            animations: json!([{
                "name": "fire", "frames": 3, "duration": 0.2, "looping": false,
                "additive": false, "priority": 0,
                "nodes": [{
                    "node": "barrel",
                    "rotations": [], "scales": [], "scale_rotations": [],
                    "translations": [[0., 0.05, -0.1], [0., 0.05, 0.], [0., 0.05, -0.1]],
                }],
                "objects": [], "ground_translations": [], "ground_rotations": [], "triggers": [],
            }]),
        },
        Model {
            name: "ski",
            nodes: vec![("root", None, [0.; 3])],
            parts: vec![part(0, [0., 0., 0.], [0.05, 0.01, 0.4], 0)],
            materials: vec![("ski", "opaque")],
            animations: still(),
        },
        Model {
            name: "bullet",
            nodes: vec![("root", None, [0.; 3])],
            parts: vec![part(0, [0., 0., 0.], [0.02, 0.02, 0.05], 0)],
            materials: vec![("bullet", "opaque")],
            animations: still(),
        },
        // A swung tool: its `swing` sequence carries the head forward.
        Model {
            name: "swinger",
            nodes: vec![
                ("root", None, [0.; 3]),
                ("mountPoint", Some(0), [0., -0.1, 0.]),
                ("head", Some(0), [0., 0.2, 0.]),
            ],
            parts: vec![
                part(0, [0., 0., 0.], [0.03, 0.2, 0.03], 0),
                part(2, [0., 0., 0.], [0.06, 0.05, 0.12], 0),
            ],
            materials: vec![("swinger", "opaque")],
            animations: json!([{
                "name": "swing", "frames": 3, "duration": 0.2, "looping": false,
                "additive": false, "priority": 0,
                "nodes": [{
                    "node": "head",
                    "rotations": [], "scales": [], "scale_rotations": [],
                    "translations": [[0., 0.2, 0.], [0., 0.2, -0.1], [0., 0.15, -0.2]],
                }],
                "objects": [], "ground_translations": [], "ground_rotations": [], "triggers": [],
            }]),
        },
        // A thrown weapon: its `throw` sequence hides both objects, then
        // shows them again.
        Model {
            name: "thrower",
            nodes: vec![("root", None, [0.; 3])],
            parts: vec![
                part(0, [0., 0., 0.], [0.02, 0.02, 0.4], 0),
                part(0, [0., 0., -0.45], [0.04, 0.04, 0.05], 0),
            ],
            materials: vec![("thrower", "opaque")],
            animations: json!([{
                "name": "throw", "frames": 3, "duration": 0.2, "looping": false,
                "additive": false, "priority": 0, "nodes": [],
                "objects": [
                    { "object": 0, "visibility": [0., 0., 1.], "frames": [0, 0, 0], "material_frames": [0, 0, 0] },
                    { "object": 1, "visibility": [0., 0., 1.], "frames": [0, 0, 0], "material_frames": [0, 0, 0] },
                ],
                "ground_translations": [], "ground_rotations": [], "triggers": [],
            }]),
        },
        // Two cans of one shape: a solid one, and one whose body is
        // translucent under a solid cap.
        Model {
            name: "spraycan",
            nodes: vec![("root", None, [0.; 3])],
            parts: vec![
                part(0, [0., 0., 0.], [0.06, 0.15, 0.06], 0),
                part(0, [0., 0.18, 0.], [0.03, 0.03, 0.03], 1),
            ],
            materials: vec![("body", "opaque"), ("cap", "opaque")],
            animations: still(),
        },
        Model {
            name: "transspraycan",
            nodes: vec![("root", None, [0.; 3])],
            parts: vec![
                part(0, [0., 0., 0.], [0.06, 0.15, 0.06], 0),
                part(0, [0., 0.18, 0.], [0.03, 0.03, 0.03], 1),
            ],
            materials: vec![("blank", "alpha"), ("cap", "opaque")],
            animations: still(),
        },
    ]
}

/// Our own sample weapons pack, the template for every item and image.
fn bubble() -> Result<Value> {
    let path = repo_root().join("packages/samples/sample-bubble-blaster/assets/weapons.json");
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn evidence() -> Value {
    json!({ "path": "synthetic", "sha256": "0".repeat(64), "line": 0 })
}

/// An item: (name, model, image, tint, icon colour).
type ItemSpec = (&'static str, &'static str, &'static str, [f32; 4], [u8; 3]);

/// An image: (name, model, mount point, offset, eye offset, raw eye
/// rotation degrees, eye rotation written as eulerToMatrix, fires).
type ImageSpec = (
    &'static str,
    &'static str,
    u32,
    [f32; 3],
    [f32; 3],
    [f32; 3],
    bool,
    bool,
);

fn write_synthetic(weapons_dir: &Path, items_dir: &Path) -> Result<()> {
    let base = bubble()?;
    let template_item = base["items"]
        .as_object()
        .and_then(|m| m.values().next())
        .context("sample has an item")?
        .clone();
    let template_image = base["images"]
        .as_object()
        .and_then(|m| m.values().next())
        .context("sample has an image")?
        .clone();
    let template_projectile = base["projectiles"]
        .as_object()
        .and_then(|m| m.values().next())
        .context("sample has a projectile")?
        .clone();
    let projectile_id = format!("{NS}:projectile/pellet");
    let images: [ImageSpec; 7] = [
        (
            "hammer",
            "hammer",
            0,
            [0., 0., 0.],
            [0.6, -0.2, -1.1],
            [0.; 3],
            false,
            false,
        ),
        (
            "gun",
            "gun",
            0,
            [0., 0.02, 0.],
            [0.; 3],
            [0.; 3],
            false,
            true,
        ),
        (
            "left_gun",
            "gun",
            1,
            [0., 0.02, 0.],
            [0.; 3],
            [0.; 3],
            false,
            true,
        ),
        (
            "ski",
            "ski",
            0,
            [0.; 3],
            [0.3, -0.4, -0.9],
            [90., 0., 90.],
            true,
            false,
        ),
        (
            "can", "spraycan", 0, [0.; 3], [0.; 3], [0.; 3], false, false,
        ),
        (
            "swing", "swinger", 0, [0.; 3], [0.; 3], [0.; 3], false, false,
        ),
        (
            "throw", "thrower", 0, [0.; 3], [0.; 3], [0.; 3], false, false,
        ),
    ];
    // Images whose `Fire` state plays a sequence of their model.
    let fire_sequences = [("swing", "swing"), ("throw", "throw")];
    let items: [ItemSpec; 4] = [
        ("hammer", "hammer", "hammer", [1.; 4], [120, 120, 130]),
        ("key", "key", "hammer", KEY_TINT, [40, 60, 200]),
        ("gun", "gun", "gun", [1.; 4], [60, 60, 60]),
        ("ski", "ski", "ski", [1.; 4], [200, 80, 40]),
    ];

    let mut pack = base.clone();
    pack["id"] = json!(format!("{NS}:weapons/main"));
    pack["items"] = json!({});
    pack["images"] = json!({});
    pack["projectiles"] = json!({});
    pack["definitions"] = json!([]);
    let mut projectile = template_projectile;
    projectile["id"] = json!(projectile_id);
    projectile["name"] = json!("fixturePelletProjectile");
    projectile["model"] = json!(model("bullet"));
    pack["projectiles"][&projectile_id] = projectile;
    for (name, shape, mount, offset, eye_offset, _, euler, fires) in images {
        let id = format!("{NS}:image/{name}");
        let image_name = format!("fixture_{name}Image");
        let mut image = template_image.clone();
        image["id"] = json!(id);
        image["name"] = json!(image_name);
        image["model"] = json!(model(shape));
        image["mount_point"] = json!(mount);
        image["offset"] = json!(offset);
        image["eye_offset"] = json!(eye_offset);
        image["source_rotation_degrees"] = json!([0., 0., 0.]);
        image["projectile"] = if fires {
            json!(projectile_id)
        } else {
            Value::Null
        };
        if let Some((_, sequence)) = fire_sequences.iter().find(|(n, _)| *n == name) {
            for state in image["states"].as_array_mut().context("image states")? {
                if state["name"] == "Fire" {
                    state["sequence"] = json!(sequence);
                }
            }
        }
        pack["images"][&id] = image;
        if euler {
            pack["definitions"].as_array_mut().unwrap().push(json!({
                "name": image_name, "class": "ShapeBaseImageData", "parent": null,
                "source": evidence(), "fields": { "eyerotation": "eulerToMatrix(\"90 0 90\")" },
            }));
        }
    }
    for (name, shape, image, _, _) in items {
        let id = format!("{NS}:weapon/{name}");
        let mut item = template_item.clone();
        item["id"] = json!(id);
        item["name"] = json!(format!("fixture_{name}Item"));
        item["ui_name"] = json!(name);
        item["image"] = json!(format!("{NS}:image/{image}"));
        item["model"] = json!(model(shape));
        item["icon"] = json!(format!("{NS}/icons/{name}"));
        pack["items"][&id] = item;
    }
    let weapons_sha = write_json(&weapons_dir.join("weapons.json"), &pack)?;

    // Models and their textures: each material's texture is clear (the
    // tint shows) with an opaque painted stripe (pigment over the tint).
    let mut model_entries = serde_json::Map::new();
    let mut textures = serde_json::Map::new();
    let mut bounds = std::collections::BTreeMap::new();
    for m in models() {
        let mut keys = vec![];
        for (i, (material, _)) in m.materials.iter().enumerate() {
            let key = format!("{NS}/shapes/{}_{material}.png", m.name);
            let shade = 60 + 50 * i as u8;
            let bytes = png(8, 8, |x, _| {
                if x == 3 || x == 4 {
                    [shade, shade / 2, 20, 255]
                } else {
                    [255, 255, 255, 0]
                }
            })?;
            let file = format!("textures/{}_{material}.png", m.name);
            let sha = write(&items_dir.join(&file), &bytes)?;
            textures.insert(
                key.clone(),
                json!({ "file": file, "sha256": sha, "width": 8, "height": 8, "source": "synthetic" }),
            );
            keys.push(key);
        }
        let file = format!("models/{}.json", m.name);
        let sha = write_json(&items_dir.join(&file), &m.shape()?)?;
        let (min, max) = m.bounds();
        bounds.insert(model(m.name), (min, max));
        model_entries.insert(
            model(m.name),
            json!({
                "file": file, "sha256": sha, "source": "synthetic", "source_sha256": "0".repeat(64),
                "textures": keys, "bounds_min": min, "bounds_max": max,
            }),
        );
    }
    let mut item_entries = serde_json::Map::new();
    let mut physics = serde_json::Map::new();
    for (name, shape, image, tint, colour) in items {
        let id = format!("{NS}:weapon/{name}");
        let icon = format!("{NS}/icons/{name}.png");
        let bytes = png(16, 16, |x, y| {
            let inside = (2..14).contains(&x) && (2..14).contains(&y);
            if inside {
                [colour[0], colour[1], colour[2], 255]
            } else {
                [0, 0, 0, 0]
            }
        })?;
        let file = format!("icons/{name}.png");
        let sha = write(&items_dir.join(&file), &bytes)?;
        textures.insert(
            icon.clone(),
            json!({ "file": file, "sha256": sha, "width": 16, "height": 16, "source": "synthetic" }),
        );
        item_entries.insert(
            id.clone(),
            json!({
                "model": model(shape), "image": format!("{NS}:image/{image}"),
                "tint": tint, "icon": icon, "evidence": evidence(),
            }),
        );
        let (min, max) = bounds[&model(shape)];
        physics.insert(id, json!({ "min": min, "max": max }));
    }
    let mut image_entries = serde_json::Map::new();
    for (name, shape, mount, offset, eye_offset, eye_rotation, _, _) in images {
        image_entries.insert(
            format!("{NS}:image/{name}"),
            json!({
                "model": model(shape), "mount_point": mount, "offset": offset,
                "eye_offset": eye_offset, "source_rotation_degrees": [0., 0., 0.],
                "eye_rotation_degrees": eye_rotation, "tint": [1., 1., 1., 1.],
                "evidence": evidence(),
            }),
        );
    }
    let physics_sha = write_json(
        &items_dir.join("item-physics.json"),
        &json!({ "schema_version": 1, "items": physics }),
    )?;
    write_json(
        &items_dir.join("presentation.json"),
        &json!({
            "schema_version": 2, "id": format!("{NS}:item-presentation/main"),
            "weapons_sha256": weapons_sha, "item_physics_sha256": physics_sha,
            "models": model_entries, "textures": textures, "items": item_entries,
            "images": image_entries,
            "projectiles": { projectile_id: { "model": model("bullet"), "tint": [0.9, 0.9, 0.2, 1.] } },
            "diagnostics": [],
        }),
    )?;
    Ok(())
}
