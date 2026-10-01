//! A made-up avatar package: a boxy figure with the node names the
//! engine looks up (`Eye`, `Mount0`, `Mount1`, `Head`, the arms and
//! legs), one choice or more in every outfit slot, and every sequence
//! alias the client plays. Its hats and packs hang from nodes beside
//! the head and back (children of the root), not under them, so a body
//! posed part by part must carry them along.
//!
//! [`write`] lays the package out in a folder; `bri_client::testing::avatar::assets()`
//! loads it through the client's `AvatarAssets`.
use super::{Paint, cuboid, material, plain, png, sha256, write_file};
use crate::avatar::Rig;
use crate::shape::{Animation, Detail, Influence, Mesh, Node, NodeTrack, Object, Shape, Skin};
use anyhow::Result;
use glam::{Mat4, Quat, Vec3};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

/// The pack's id, and the rig file `avatar.json` names.
pub const ID: &str = "test:avatar/boxhead";
pub const RIG_FILE: &str = "rig.json";

/// Choices of each outfit slot, the first the default. Hats and packs
/// have more than one real choice; `cap` allows no visor.
pub const PARTS: [(&str, &[&str]); 12] = [
    ("hat", &["none", "helmet", "cap"]),
    ("accent", &["none", "visor", "plume"]),
    ("pack", &["none", "armor", "tank"]),
    ("secondpack", &["none", "cape"]),
    ("chest", &["chest", "femchest"]),
    ("hip", &["pants", "skirthip"]),
    ("rarm", &["rarm"]),
    ("larm", &["larm"]),
    ("rhand", &["rhand", "rhook"]),
    ("lhand", &["lhand"]),
    ("rleg", &["rshoe", "rpeg"]),
    ("lleg", &["lshoe"]),
];
/// Accents each hat allows.
pub const ACCENTS: [(&str, &[&str]); 2] =
    [("helmet", &["none", "visor"]), ("cap", &["none", "plume"])];
pub const FACES: [&str; 2] = ["faces/smile", "faces/frown"];
pub const DECALS: [&str; 2] = ["decals/plain", "decals/stripes"];
/// Shape materials other than `face` and `decal`, and their textures.
/// `glass` (the visor) is alpha blended.
pub const SURFACES: [(&str, &str); 3] = [
    ("skin", "body/skin"),
    ("gear", "body/gear"),
    ("glass", "body/glass"),
];
/// The default colour of each paint slot. The accent's alpha is kept
/// (above the engine's floor); every other slot is drawn opaque.
pub const COLORS: [(&str, [f32; 4]); 13] = [
    ("head", [0.9, 0.8, 0.3, 1.0]),
    ("torso", [0.2, 0.4, 0.7, 1.0]),
    ("hat", [0.6, 0.1, 0.1, 1.0]),
    ("accent", [0.8, 0.8, 0.9, 0.6]),
    ("pack", [0.3, 0.3, 0.3, 1.0]),
    ("secondpack", [0.5, 0.0, 0.5, 1.0]),
    ("hip", [0.1, 0.2, 0.4, 1.0]),
    ("rarm", [0.2, 0.4, 0.7, 1.0]),
    ("larm", [0.2, 0.4, 0.7, 1.0]),
    ("rhand", [0.9, 0.8, 0.3, 1.0]),
    ("lhand", [0.9, 0.8, 0.3, 1.0]),
    ("rleg", [0.1, 0.1, 0.1, 1.0]),
    ("lleg", [0.1, 0.1, 0.1, 1.0]),
];

/// Nodes: name, parent, translation from the parent.
const NODES: [(&str, Option<&str>, [f32; 3]); 19] = [
    ("Body", None, [0.0, 0.0, 0.0]),
    ("Hip", Some("Body"), [0.0, 0.9, 0.0]),
    ("Torso", Some("Hip"), [0.0, 0.25, 0.0]),
    ("Head", Some("Torso"), [0.0, 0.95, 0.0]),
    ("Eye", Some("Head"), [0.0, 0.3, -0.1]),
    ("RightArm", Some("Torso"), [0.62, 0.8, 0.0]),
    ("RightHand", Some("RightArm"), [0.05, -0.75, -0.15]),
    ("Mount0", Some("RightHand"), [0.0, -0.05, -0.2]),
    ("LeftArm", Some("Torso"), [-0.62, 0.8, 0.0]),
    ("LeftHand", Some("LeftArm"), [-0.05, -0.75, -0.15]),
    ("Mount1", Some("LeftHand"), [0.0, -0.05, -0.2]),
    ("RightLeg", Some("Hip"), [0.25, -0.05, 0.0]),
    ("LeftLeg", Some("Hip"), [-0.25, -0.05, 0.0]),
    ("RSki", Some("RightLeg"), [0.0, -0.85, 0.0]),
    ("LSki", Some("LeftLeg"), [0.0, -0.85, 0.0]),
    // Beside the head and back, not under them.
    ("HatSpot", Some("Body"), [0.0, 2.78, 0.0]),
    ("BackSpot", Some("Body"), [0.0, 1.65, 0.35]),
    ("Mount2", Some("HatSpot"), [0.0, 0.1, 0.0]),
    ("Mount3", Some("BackSpot"), [0.0, 0.0, 0.2]),
];

// Shape material indices.
const FACE: usize = 0;
const DECAL: usize = 1;
const SKIN: usize = 2;
const GEAR: usize = 3;
const GLASS: usize = 4;
/// Rigid parts: object name, node, box centre and half size in the
/// node's frame, paint. `pants` is skinned, built separately.
/// Object name, node, box centre and half size in the node's frame,
/// paint.
type RigidPart = (&'static str, &'static str, [f32; 3], [f32; 3], Paint);
const RIGID: [RigidPart; 23] = [
    (
        "headskin",
        "Head",
        [0.0, 0.3, 0.0],
        [0.3, 0.3, 0.3],
        Paint {
            front: FACE,
            rest: SKIN,
        },
    ),
    (
        "chest",
        "Torso",
        [0.0, 0.45, 0.0],
        [0.5, 0.45, 0.25],
        Paint {
            front: DECAL,
            rest: SKIN,
        },
    ),
    (
        "femchest",
        "Torso",
        [0.0, 0.45, -0.02],
        [0.48, 0.45, 0.27],
        Paint {
            front: DECAL,
            rest: SKIN,
        },
    ),
    (
        "skirthip",
        "Hip",
        [0.0, -0.05, 0.0],
        [0.55, 0.2, 0.3],
        plain(SKIN),
    ),
    (
        "skirttrimleft",
        "LeftLeg",
        [0.0, -0.45, 0.0],
        [0.27, 0.35, 0.3],
        plain(SKIN),
    ),
    (
        "skirttrimright",
        "RightLeg",
        [0.0, -0.45, 0.0],
        [0.27, 0.35, 0.3],
        plain(SKIN),
    ),
    (
        "rarm",
        "RightArm",
        [0.0, -0.35, 0.0],
        [0.13, 0.4, 0.16],
        plain(SKIN),
    ),
    (
        "larm",
        "LeftArm",
        [0.0, -0.35, 0.0],
        [0.13, 0.4, 0.16],
        plain(SKIN),
    ),
    (
        "rhand",
        "RightHand",
        [0.0, 0.0, 0.0],
        [0.12, 0.12, 0.12],
        plain(SKIN),
    ),
    (
        "rhook",
        "RightHand",
        [0.0, -0.05, 0.0],
        [0.04, 0.15, 0.08],
        plain(GEAR),
    ),
    (
        "lhand",
        "LeftHand",
        [0.0, 0.0, 0.0],
        [0.12, 0.12, 0.12],
        plain(SKIN),
    ),
    (
        "rshoe",
        "RightLeg",
        [0.0, -0.45, -0.05],
        [0.22, 0.4, 0.3],
        plain(SKIN),
    ),
    (
        "rpeg",
        "RightLeg",
        [0.0, -0.45, 0.0],
        [0.06, 0.4, 0.06],
        plain(GEAR),
    ),
    (
        "lshoe",
        "LeftLeg",
        [0.0, -0.45, -0.05],
        [0.22, 0.4, 0.3],
        plain(SKIN),
    ),
    (
        "rski",
        "RSki",
        [0.0, 0.0, -0.3],
        [0.1, 0.02, 0.9],
        plain(GEAR),
    ),
    (
        "lski",
        "LSki",
        [0.0, 0.0, -0.3],
        [0.1, 0.02, 0.9],
        plain(GEAR),
    ),
    (
        "helmet",
        "HatSpot",
        [0.0, 0.0, 0.0],
        [0.35, 0.12, 0.35],
        plain(GEAR),
    ),
    (
        "visor",
        "HatSpot",
        [0.0, -0.2, -0.36],
        [0.3, 0.08, 0.02],
        plain(GLASS),
    ),
    (
        "cap",
        "HatSpot",
        [0.0, -0.02, 0.0],
        [0.32, 0.08, 0.32],
        plain(GEAR),
    ),
    (
        "plume",
        "HatSpot",
        [0.0, 0.25, 0.0],
        [0.03, 0.18, 0.12],
        plain(GEAR),
    ),
    (
        "armor",
        "BackSpot",
        [0.0, -0.1, 0.05],
        [0.42, 0.45, 0.12],
        plain(GEAR),
    ),
    (
        "tank",
        "BackSpot",
        [0.0, -0.2, 0.08],
        [0.2, 0.5, 0.15],
        plain(GEAR),
    ),
    (
        "cape",
        "BackSpot",
        [0.0, -0.6, 0.12],
        [0.5, 0.6, 0.02],
        plain(SKIN),
    ),
];

fn node_index(name: &str) -> usize {
    NODES
        .iter()
        .position(|(n, _, _)| *n == name)
        .unwrap_or_else(|| panic!("fixture node {name}"))
}

/// Each node's bind-pose transform in the shape's frame.
fn bind_world() -> Vec<Mat4> {
    let mut world: Vec<Mat4> = Vec::with_capacity(NODES.len());
    for (_, parent, t) in NODES {
        let local = Mat4::from_translation(Vec3::from(t));
        world.push(match parent {
            Some(p) => world[node_index(p)] * local,
            None => local,
        });
    }
    world
}

/// `pants`: one box in the shape's frame, its top on `Hip` and each
/// lower corner on the leg on its side.
fn pants(world: &[Mat4]) -> Mesh {
    let hip = world[node_index("Hip")].w_axis.truncate();
    let mut mesh = cuboid(
        hip + Vec3::new(0.0, -0.1, 0.0),
        Vec3::new(0.5, 0.25, 0.26),
        plain(SKIN),
    );
    let bones = ["Hip", "RightLeg", "LeftLeg"].map(node_index);
    let influences = mesh
        .positions
        .iter()
        .enumerate()
        .map(|(vertex, p)| {
            let bone = if p[1] > hip.y - 0.1 {
                0
            } else if p[0] > 0.0 {
                1
            } else {
                2
            };
            Influence {
                vertex,
                bone,
                weight: 1.0,
            }
        })
        .collect();
    mesh.skin = Some(Skin {
        inverse_bind: bones
            .iter()
            .map(|b| world[*b].inverse().to_cols_array())
            .collect(),
        nodes: bones.to_vec(),
        influences,
    });
    mesh
}

fn q(axis: Vec3, angle: f32) -> [f32; 4] {
    Quat::from_axis_angle(axis, angle).to_array()
}
fn rx(angle: f32) -> [f32; 4] {
    q(Vec3::X, angle)
}
fn ry(angle: f32) -> [f32; 4] {
    q(Vec3::Y, angle)
}
fn rz(angle: f32) -> [f32; 4] {
    q(Vec3::Z, angle)
}
/// The bind translation of `node`, for absolute tracks that move it.
fn at(node: &str) -> Vec3 {
    Vec3::from(NODES[node_index(node)].2)
}

struct Clip {
    frames: usize,
    duration: f32,
    looping: bool,
    additive: bool,
    priority: i32,
    nodes: Vec<NodeTrack>,
}
fn rotations(node: &str, keys: &[[f32; 4]]) -> NodeTrack {
    NodeTrack {
        node: node.into(),
        rotations: keys.to_vec(),
        translations: Vec::new(),
        scales: Vec::new(),
        scale_rotations: Vec::new(),
    }
}
fn moves(node: &str, keys: &[[f32; 4]], offsets: &[Vec3]) -> NodeTrack {
    NodeTrack {
        translations: offsets.iter().map(|o| o.to_array()).collect(),
        ..rotations(node, keys)
    }
}
fn absolute(frames: usize, duration: f32, looping: bool, nodes: Vec<NodeTrack>) -> Clip {
    Clip {
        frames,
        duration,
        looping,
        additive: false,
        priority: 12,
        nodes,
    }
}
fn additive(frames: usize, duration: f32, nodes: Vec<NodeTrack>) -> Clip {
    Clip {
        frames,
        duration,
        looping: false,
        additive: true,
        priority: 16,
        nodes,
    }
}
const I: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// A walk cycle: legs swing `swing` about X (or Z for `side`), arms
/// against them, the hip at `hip` above the feet.
fn stride(swing: f32, about: fn(f32) -> [f32; 4], hip: f32, duration: f32) -> Clip {
    let legs = |s: f32| [about(s), I, about(-s), I];
    let hip_at = at("Hip").with_y(hip);
    absolute(
        4,
        duration,
        true,
        vec![
            rotations("RightLeg", &legs(swing)),
            rotations("LeftLeg", &legs(-swing)),
            rotations("RightArm", &legs(-swing * 0.7)),
            rotations("LeftArm", &legs(swing * 0.7)),
            moves("Hip", &[I; 4], &[hip_at; 4]),
        ],
    )
}

/// Every sequence alias the client plays, invented.
fn sequences() -> BTreeMap<String, Clip> {
    const CROUCHED: f32 = 0.6;
    let standing = at("Hip").y;
    let hip = |y: f32| at("Hip").with_y(y);
    let mut clips = BTreeMap::new();
    let mut add = |name: &str, clip: Clip| {
        clips.insert(name.to_string(), clip);
    };
    add(
        "root",
        absolute(
            1,
            1.0,
            true,
            ["RightLeg", "LeftLeg", "RightArm", "LeftArm"]
                .iter()
                .map(|n| rotations(n, &[I]))
                .chain([moves("Hip", &[I], &[hip(standing)])])
                .collect(),
        ),
    );
    add("run", stride(0.6, rx, standing, 0.8));
    add("back", stride(-0.5, rx, standing, 0.9));
    add("side", stride(0.4, rz, standing, 0.7));
    add("crouchrun", stride(0.4, rx, CROUCHED, 1.0));
    add("crouchback", stride(-0.35, rx, CROUCHED, 1.1));
    add("crouchside", stride(0.3, rz, CROUCHED, 1.0));
    add(
        "crouch",
        Clip {
            priority: 13,
            ..absolute(
                2,
                0.3,
                false,
                vec![
                    moves("Hip", &[I, I], &[hip(standing), hip(CROUCHED)]),
                    rotations("RightLeg", &[I, rx(0.5)]),
                    rotations("LeftLeg", &[I, rx(0.5)]),
                ],
            )
        },
    );
    add(
        "fall",
        absolute(
            1,
            1.0,
            true,
            vec![
                rotations("RightArm", &[rz(0.9)]),
                rotations("LeftArm", &[rz(-0.9)]),
            ],
        ),
    );
    // Additive, unlike the other movement clips.
    add(
        "jump",
        Clip {
            priority: 12,
            ..additive(
                2,
                0.4,
                vec![
                    rotations("RightLeg", &[I, rx(0.3)]),
                    rotations("LeftLeg", &[I, rx(-0.2)]),
                ],
            )
        },
    );
    add(
        "sit",
        absolute(
            1,
            1.0,
            true,
            vec![
                moves("Hip", &[I], &[hip(0.45)]),
                rotations("RightLeg", &[rx(1.5)]),
                rotations("LeftLeg", &[rx(1.5)]),
            ],
        ),
    );
    add(
        "death1",
        Clip {
            priority: 128,
            ..absolute(
                3,
                1.0,
                false,
                vec![
                    moves(
                        "Body",
                        &[I, rx(0.8), rx(1.5)],
                        &[
                            Vec3::ZERO,
                            Vec3::new(0.0, 0.0, 0.3),
                            Vec3::new(0.0, 0.25, 0.7),
                        ],
                    ),
                    rotations("Head", &[I, rx(0.3), rx(0.5)]),
                    rotations("RightArm", &[I, rz(0.6), rz(1.2)]),
                    rotations("LeftArm", &[I, rz(-0.6), rz(-1.2)]),
                ],
            )
        },
    );
    // Held-arm poses: absolute, over locomotion's arms.
    let raise = |arms: &[&str]| Clip {
        priority: 14,
        ..absolute(
            2,
            0.2,
            false,
            arms.iter()
                .flat_map(|arm| {
                    [
                        rotations(arm, &[I, rx(1.4)]),
                        rotations(&arm.replace("Arm", "Hand"), &[I, rx(0.1)]),
                    ]
                })
                .collect(),
        )
    };
    add("armreadyright", raise(&["RightArm"]));
    add("armreadyleft", raise(&["LeftArm"]));
    add("armreadyboth", raise(&["RightArm", "LeftArm"]));
    // Overlays.
    add(
        "look",
        additive(
            3,
            1.0,
            vec![
                rotations("Head", &[rx(0.7), I, rx(-0.6)]),
                rotations("RightArm", &[rx(0.6), I, rx(-0.5)]),
                rotations("LeftArm", &[rx(0.6), I, rx(-0.5)]),
            ],
        ),
    );
    add(
        "headside",
        additive(3, 1.0, vec![rotations("Head", &[ry(1.2), I, ry(-1.2)])]),
    );
    add(
        "headup",
        additive(2, 0.2, vec![rotations("Head", &[I, rx(0.25)])]),
    );
    // Actions: additive swings of the right arm. `armattack` also
    // turns the hip; `wrench` names the right leg without moving it.
    add(
        "armattack",
        additive(
            3,
            0.3,
            vec![
                rotations("RightArm", &[I, rx(0.9), I]),
                rotations("Hip", &[I, ry(0.15), I]),
            ],
        ),
    );
    add(
        "wrench",
        additive(
            3,
            0.5,
            vec![
                rotations("RightArm", &[I, rz(0.5), I]),
                rotations("RightLeg", &[I, I, I]),
            ],
        ),
    );
    // Builder and chat gestures (thread 3).
    let nudge = |offset: Vec3| {
        additive(
            3,
            0.4,
            vec![moves(
                "RightArm",
                &[I; 3],
                &[Vec3::ZERO, offset, Vec3::ZERO],
            )],
        )
    };
    add("shiftaway", nudge(Vec3::new(0.0, 0.0, -0.2)));
    add("shiftto", nudge(Vec3::new(0.0, 0.0, 0.2)));
    add("shiftleft", nudge(Vec3::new(-0.15, 0.0, 0.0)));
    add("shiftright", nudge(Vec3::new(0.15, 0.0, 0.0)));
    add("shiftup", nudge(Vec3::new(0.0, 0.15, 0.0)));
    add("shiftdown", nudge(Vec3::new(0.0, -0.15, 0.0)));
    let twist = |angle: f32| additive(3, 0.4, vec![rotations("RightHand", &[I, rz(angle), I])]);
    add("rotcw", twist(-0.8));
    add("rotccw", twist(0.8));
    add(
        "plant",
        additive(3, 0.35, vec![rotations("RightArm", &[I, rx(-0.6), I])]),
    );
    add(
        "undo",
        additive(3, 0.35, vec![rotations("RightArm", &[I, rz(-0.7), I])]),
    );
    add(
        "activate",
        additive(3, 0.3, vec![rotations("RightArm", &[I, rx(1.0), I])]),
    );
    add(
        "activate2",
        additive(3, 0.3, vec![rotations("RightArm", &[I, rx(1.2), I])]),
    );
    add(
        "talk",
        additive(3, 0.25, vec![rotations("Head", &[I, rx(0.12), I])]),
    );
    clips
}

/// The rig: the shape (nodes, part objects, one visible detail) and
/// its sequences.
pub fn rig() -> Rig {
    let world = bind_world();
    let mut objects = Vec::new();
    let mut meshes = Vec::new();
    for (name, node, centre, half, paint) in RIGID {
        objects.push(Object {
            name: name.into(),
            node: Some(node_index(node)),
            meshes: vec![meshes.len()],
            visibility: 1.0,
            frame: 0,
            material_frame: 0,
        });
        meshes.push(Some(cuboid(Vec3::from(centre), Vec3::from(half), paint)));
    }
    objects.push(Object {
        name: "pants".into(),
        node: None,
        meshes: vec![meshes.len()],
        visibility: 1.0,
        frame: 0,
        material_frame: 0,
    });
    meshes.push(Some(pants(&world)));
    let shape = Shape {
        schema_version: 1,
        id: format!("{ID}/shape"),
        nodes: NODES
            .iter()
            .map(|(name, parent, t)| Node {
                name: (*name).into(),
                parent: parent.map(node_index),
                translation: *t,
                rotation: I,
            })
            .collect(),
        details: vec![Detail {
            name: "detail1".into(),
            pixel_threshold: 0.0,
            object_start: 0,
            object_count: objects.len(),
            mesh_offset: 0,
            collision: false,
        }],
        objects,
        meshes,
        materials: vec![
            material("face", "opaque"),
            material("decal", "opaque"),
            material("skin", "opaque"),
            material("gear", "opaque"),
            material("glass", "alpha"),
        ],
        animations: Vec::new(),
    };
    let sequences = sequences()
        .into_iter()
        .map(|(name, clip)| {
            let animation = Animation {
                name: name.clone(),
                frames: clip.frames,
                duration: clip.duration,
                looping: clip.looping,
                additive: clip.additive,
                priority: clip.priority,
                nodes: clip.nodes,
                objects: Vec::new(),
                ground_translations: Vec::new(),
                ground_rotations: Vec::new(),
                triggers: Vec::new(),
            };
            (name, animation)
        })
        .collect();
    Rig {
        schema_version: 1,
        id: ID.into(),
        shape,
        sequences,
        sources: Vec::new(),
        omissions: Vec::new(),
    }
}

/// A small texture for `id`, drawn in code: a colour from its name
/// with a darker border.
fn texture(id: &str) -> Result<Vec<u8>> {
    let seed = sha256(id.as_bytes());
    let tint: Vec<u8> = (0..3)
        .map(|i| u8::from_str_radix(&seed[i * 2..i * 2 + 2], 16).unwrap_or(128))
        .collect();
    let alpha = if id == "body/glass" { 128 } else { 255 };
    png(8, 8, |x, y| {
        let edge = x == 0 || y == 0 || x == 7 || y == 7;
        let shade = |c: u8| if edge { c / 2 } else { c };
        [shade(tint[0]), shade(tint[1]), shade(tint[2]), alpha]
    })
}

/// Write the pack into `dir`: `avatar.json`, the rig and its textures,
/// each named with its SHA-256 the way the loader checks.
pub fn write(dir: &Path) -> Result<()> {
    let rig_sha = write_file(dir, RIG_FILE, &serde_json::to_vec(&rig())?)?;
    let mut textures = serde_json::Map::new();
    let ids = FACES
        .iter()
        .chain(&DECALS)
        .copied()
        .chain(SURFACES.iter().map(|(_, id)| *id));
    for id in ids {
        let file = format!("textures/{id}.png");
        let sha = write_file(dir, &file, &texture(id)?)?;
        textures.insert(
            id.into(),
            json!({"file": file, "sha256": sha, "source": "made up", "width": 8, "height": 8}),
        );
    }
    let listed = |pairs: &[(&str, &[&str])]| -> serde_json::Map<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), json!(v)))
            .collect()
    };
    let package = json!({
        "schema_version": 1,
        "id": ID,
        "rig": RIG_FILE,
        "rig_sha256": rig_sha,
        "parts": listed(&PARTS),
        "accents_allowed": listed(&ACCENTS),
        "faces": FACES,
        "decals": DECALS,
        "surfaces": SURFACES.iter().map(|(k, v)| ((*k).to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
        "textures": textures,
        "defaults": {
            "parts": PARTS.iter().map(|(slot, _)| ((*slot).to_string(), json!(0))).collect::<serde_json::Map<_, _>>(),
            "colors": COLORS.iter().map(|(k, c)| ((*k).to_string(), json!(c))).collect::<serde_json::Map<_, _>>(),
            "face": "smile",
            "decal": "plain",
        },
    });
    write_file(dir, "avatar.json", &serde_json::to_vec_pretty(&package)?)?;
    Ok(())
}
