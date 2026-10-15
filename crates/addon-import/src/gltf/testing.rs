//! Writing small glTF models for tests: [`Gltf`] builds a `.glb` from nodes,
//! meshes and clips, and [`character`] is an original rigged figure laid out
//! the way the authoring guide asks (sockets, role clips, a scaled
//! armature, textures), so tests need no third-party model.
pub use super::write::{Channel, Gltf};
use glam::{Mat4, Quat, Vec3};
use serde_json::json;

/// A box of the figure: centre, size and its `(bone, weight)` pairs.
type Part<'a> = (Vec3, Vec3, &'a [(u32, f32)]);

/// A box's 24 corners (four per face, so faces keep flat normals), their
/// normals, UVs and 12 triangles wound counter-clockwise from outside.
pub fn cuboid(center: Vec3, size: Vec3) -> (Vec<Vec3>, Vec<Vec3>, Vec<[f32; 2]>, Vec<u32>) {
    let (mut p, mut n, mut uv, mut idx) = (vec![], vec![], vec![], vec![]);
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let mut normal = Vec3::ZERO;
            normal[axis] = sign;
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let base = p.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut corner = normal;
                corner[u] = a;
                corner[v] = b;
                p.push(center + corner * size * 0.5);
                n.push(normal);
                uv.push([(a + 1.0) * 0.5, (b + 1.0) * 0.5]);
            }
            if sign > 0.0 {
                idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            } else {
                idx.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
            }
        }
    }
    (p, n, uv, idx)
}

/// A box of `n` by `n` quads a side bulged toward an ellipsoid, each side
/// its own vertices (a UV seam along every edge), as [`cuboid`].
pub fn rounded_cuboid(
    center: Vec3,
    size: Vec3,
    n: u32,
) -> (Vec<Vec3>, Vec<Vec3>, Vec<[f32; 2]>, Vec<u32>) {
    let (mut p, mut nrm, mut uv, mut idx) = (vec![], vec![], vec![], vec![]);
    let at = |i: u32| -1.0 + 2.0 * i as f32 / n as f32;
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let base = p.len() as u32;
            for j in 0..=n {
                for i in 0..=n {
                    let mut cube = Vec3::ZERO;
                    cube[axis] = sign;
                    cube[u] = at(i);
                    cube[v] = at(j);
                    let round = cube.lerp(cube.normalize() * 1.2, 0.5);
                    p.push(center + round * size * 0.5);
                    nrm.push((round / size).normalize());
                    uv.push([i as f32 / n as f32, j as f32 / n as f32]);
                }
            }
            for j in 0..n {
                for i in 0..n {
                    let a = base + j * (n + 1) + i;
                    let (b, c, d) = (a + 1, a + n + 2, a + n + 1);
                    if sign > 0.0 {
                        idx.extend([a, b, c, a, c, d]);
                    } else {
                        idx.extend([a, c, b, a, d, c]);
                    }
                }
            }
        }
    }
    (p, nrm, uv, idx)
}

/// The figure's bones: name, parent (index into this list), translation
/// and rotation in the parent's frame. The armature above them is scaled
/// by [`ARMATURE_SCALE`], as models made at another scale are.
pub const BONES: [(&str, Option<usize>, [f32; 3]); 5] = [
    ("hips", None, [0.0, 2.0, 0.0]),
    ("spine", Some(0), [0.0, 1.0, 0.0]),
    ("head", Some(1), [0.0, 1.0, 0.0]),
    ("arm.R", Some(1), [-0.8, 0.8, 0.0]),
    ("hand.L", Some(1), [0.8, 0.8, 0.0]),
];
pub const ARMATURE_SCALE: f32 = 0.5;

/// Node indices of the figure's interesting parts.
pub struct Figure {
    pub glb: Vec<u8>,
    pub armature: usize,
    pub bones: Vec<usize>,
    pub hand_socket: usize,
    pub nose: Vec3,
}

/// An original rigged figure, facing +Z as Blender exports: a skinned body
/// (hips, spine, head, two arms), a rigid crown on the head, a right-hand
/// socket empty turned at an angle, textures, and the clips `Idle`, `Run`,
/// `Back`, `Side`, `Crouch`, `Look`, `head_side`, `aim_right` and `Stomp`.
/// About 2.25 units tall.
pub fn character(with_eye: bool) -> Figure {
    detailed_character(with_eye, 1)
}
/// [`character`] with each box of its skinned body a rounded box of
/// `subdivisions` squared quads a side (12 by 12 makes about 8,600
/// triangles), as heavy as a detailed game character.
pub fn detailed_character(with_eye: bool, subdivisions: u32) -> Figure {
    let mut g = Gltf::new();
    let armature = g.node(
        json!({ "name": "Armature", "scale": [ARMATURE_SCALE, ARMATURE_SCALE, ARMATURE_SCALE] }),
        None,
    );
    let mut bones = Vec::new();
    for (name, parent, t) in BONES {
        let p = parent.map_or(armature, |p| bones[p]);
        bones.push(g.node(json!({ "name": name, "translation": t }), Some(p)));
    }
    let hand_socket = g.node(
        json!({ "name": "socket_right_hand", "translation": [0.0, -0.6, 0.1],
                "rotation": Quat::from_rotation_z(0.5).mul_quat(Quat::from_rotation_x(0.3)).to_array() }),
        Some(bones[3]),
    );
    if with_eye {
        g.node(
            json!({ "name": "socket_eye", "translation": [0.0, 0.4, 0.3] }),
            Some(bones[2]),
        );
    }
    // World bind matrices (the armature's scale included) and their inverses.
    let world: Vec<Mat4> = {
        let mut w: Vec<Mat4> = Vec::new();
        for (_, parent, t) in BONES {
            let parent = parent.map_or(Mat4::from_scale(Vec3::splat(ARMATURE_SCALE)), |p| w[p]);
            w.push(parent * Mat4::from_translation(Vec3::from(t)));
        }
        w
    };
    let inverse: Vec<Mat4> = world.iter().map(|m| m.inverse()).collect();
    let skin = g.skin(&bones, &inverse);
    // The body in model space: torso over hips and spine, head, arms; a
    // nose on the +Z side of the head.
    let parts: [Part; 5] = [
        (
            Vec3::new(0.0, 1.25, 0.0),
            Vec3::new(0.8, 0.9, 0.4),
            &[(0, 0.5), (1, 0.5)],
        ),
        (
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::new(0.5, 0.5, 0.5),
            &[(2, 1.0)],
        ),
        (
            Vec3::new(-0.5, 1.45, 0.0),
            Vec3::new(0.2, 0.6, 0.2),
            &[(3, 1.0)],
        ),
        (
            Vec3::new(0.5, 1.45, 0.0),
            Vec3::new(0.2, 0.6, 0.2),
            &[(4, 1.0)],
        ),
        (
            Vec3::new(0.0, 0.4, 0.0),
            Vec3::new(0.6, 0.8, 0.3),
            &[(0, 1.0)],
        ),
    ];
    let (mut pos, mut nrm, mut uvs, mut idx, mut joints, mut weights) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut legs_len = 0;
    for (center, size, influence) in parts {
        let (p, n, uv, i) = if subdivisions > 1 {
            rounded_cuboid(center, size, subdivisions)
        } else {
            cuboid(center, size)
        };
        legs_len = i.len();
        let base = (pos.len() / 3) as u32;
        for k in 0..p.len() {
            pos.extend(p[k].to_array());
            nrm.extend(n[k].to_array());
            uvs.extend(uv[k]);
            let mut j = [0u32; 4];
            let mut w = [0.0f32; 4];
            for (s, (bone, weight)) in influence.iter().enumerate() {
                j[s] = *bone;
                w[s] = *weight;
            }
            joints.extend(j);
            weights.extend(w);
        }
        idx.extend(i.iter().map(|v| v + base));
    }
    let nose = Vec3::new(0.25, 2.25, 0.25);
    let skin_tex = g.material(
        "Skin Tone",
        [1.0, 1.0, 1.0, 1.0],
        Some((
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        )),
        json!({ "normalTexture": { "index": 0 } }),
    );
    let boots = g.material("Boots", [0.2, 0.1, 0.05, 1.0], None, json!({}));
    let (a_pos, a_nrm, a_uv, a_idx, a_j, a_w) = (
        g.floats(&pos, 3),
        g.floats(&nrm, 3),
        g.floats(&uvs, 2),
        g.uints(&idx[..idx.len() - legs_len], 1),
        g.uints(&joints, 4),
        g.floats(&weights, 4),
    );
    let legs = g.uints(&idx[idx.len() - legs_len..], 1);
    let attributes = json!({ "POSITION": a_pos, "NORMAL": a_nrm, "TEXCOORD_0": a_uv, "JOINTS_0": a_j, "WEIGHTS_0": a_w });
    let body = g.mesh(
        "Body",
        vec![
            json!({ "attributes": attributes, "indices": a_idx, "material": skin_tex }),
            json!({ "attributes": attributes, "indices": legs, "material": boots }),
        ],
    );
    // Skinned meshes ignore their node's transform.
    g.node(
        json!({ "name": "Body", "mesh": body, "skin": skin, "translation": [5.0, 0.0, 0.0] }),
        None,
    );
    // A rigid crown on the head, squashed by its own uneven scale.
    let (p, n, uv, i) = cuboid(Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.6, 0.4, 0.6));
    let flat = |v: &[Vec3]| v.iter().flat_map(|v| v.to_array()).collect::<Vec<f32>>();
    let (h_pos, h_nrm, h_uv, h_idx) = (
        g.floats(&flat(&p), 3),
        g.floats(&flat(&n), 3),
        g.floats(&uv.concat(), 2),
        g.uints(&i, 1),
    );
    let crown = g.mesh(
        "Crown",
        vec![json!({ "attributes": { "POSITION": h_pos, "NORMAL": h_nrm, "TEXCOORD_0": h_uv }, "indices": h_idx, "material": boots })],
    );
    g.node(
        json!({ "name": "Crown", "mesh": crown, "scale": [1.0, 0.5, 1.0] }),
        Some(bones[2]),
    );

    let q = |q: Quat| q.to_array();
    let rot = |a: Quat, b: Quat| [q(a), q(b), q(a)].concat();
    let i = Quat::IDENTITY;
    let (hips, head, arm_r) = (bones[0], bones[2], bones[3]);
    g.animation(
        "Idle",
        &[(
            hips,
            "rotation",
            "LINEAR",
            &[0.0, 0.5, 1.0],
            &rot(i, Quat::from_rotation_y(0.1)),
        )],
    );
    g.animation(
        "Run",
        &[
            (
                arm_r,
                "rotation",
                "LINEAR",
                &[0.0, 0.5, 1.0],
                &rot(Quat::from_rotation_x(0.6), Quat::from_rotation_x(-0.6)),
            ),
            (
                bones[4],
                "rotation",
                "LINEAR",
                &[0.0, 0.5, 1.0],
                &rot(Quat::from_rotation_x(-0.6), Quat::from_rotation_x(0.6)),
            ),
            (
                hips,
                "translation",
                "LINEAR",
                &[0.0, 1.0],
                &[0.0, 2.0, 0.0, 0.0, 2.0, 0.5],
            ),
        ],
    );
    g.animation(
        "Back",
        &[(
            arm_r,
            "rotation",
            "STEP",
            &[0.0, 0.5],
            &[q(i), q(Quat::from_rotation_x(0.3))].concat(),
        )],
    );
    // A cubic spline: in-tangent, value, out-tangent per key.
    let zero = [0.0f32; 4];
    g.animation(
        "Side",
        &[(
            hips,
            "rotation",
            "CUBICSPLINE",
            &[0.0, 1.0],
            &[zero, q(i), zero, zero, q(Quat::from_rotation_z(0.2)), zero].concat(),
        )],
    );
    g.animation(
        "Crouch",
        &[(
            hips,
            "translation",
            "LINEAR",
            &[0.0, 0.3],
            &[0.0, 2.0, 0.0, 0.0, 1.4, 0.0],
        )],
    );
    g.animation(
        "Look",
        &[
            (
                head,
                "rotation",
                "LINEAR",
                &[0.0, 0.5, 1.0],
                &[
                    q(Quat::from_rotation_x(-0.7)),
                    q(i),
                    q(Quat::from_rotation_x(0.6)),
                ]
                .concat(),
            ),
            // Still at rest: left out of the overlay.
            (
                bones[1],
                "rotation",
                "LINEAR",
                &[0.0, 1.0],
                &[q(i), q(i)].concat(),
            ),
        ],
    );
    g.animation(
        "head_side",
        &[(
            head,
            "rotation",
            "LINEAR",
            &[0.0, 0.5, 1.0],
            &[
                q(Quat::from_rotation_y(-1.2)),
                q(i),
                q(Quat::from_rotation_y(1.2)),
            ]
            .concat(),
        )],
    );
    g.animation(
        "aim_right",
        &[
            (
                arm_r,
                "rotation",
                "LINEAR",
                &[0.0, 0.2],
                &[q(i), q(Quat::from_rotation_x(-1.5))].concat(),
            ),
            // Holds the left hand still at rest while aiming: the run's
            // swing must not show through.
            (
                bones[4],
                "rotation",
                "LINEAR",
                &[0.0, 0.2],
                &[q(i), q(i)].concat(),
            ),
        ],
    );
    g.animation(
        "Sit",
        &[(
            hips,
            "translation",
            "LINEAR",
            &[0.0, 0.1],
            &[0.0, 2.0, 0.0, 0.0, 1.2, 0.0],
        )],
    );
    g.animation(
        "Stomp",
        &[(
            hips,
            "translation",
            "LINEAR",
            &[0.0, 0.25, 0.5],
            &[0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 2.0, 0.0],
        )],
    );
    Figure {
        glb: g.glb(),
        armature,
        bones,
        hand_socket,
        nose,
    }
}

/// A rifle-like item facing -Y in Blender (+Z in glTF): a stock and a
/// barrel, a grip empty turned at an angle and a muzzle empty at the tip.
pub fn rifle() -> Vec<u8> {
    let mut g = Gltf::new();
    let (p, n, uv, i) = cuboid(Vec3::new(0.0, 0.0, 0.5), Vec3::new(0.1, 0.2, 1.4));
    let flat = |v: &[Vec3]| v.iter().flat_map(|v| v.to_array()).collect::<Vec<f32>>();
    let (a, b, c, d) = (
        g.floats(&flat(&p), 3),
        g.floats(&flat(&n), 3),
        g.floats(&uv.concat(), 2),
        g.uints(&i, 1),
    );
    let metal = g.material(
        "Gunmetal",
        [0.3, 0.3, 0.35, 1.0],
        None,
        json!({ "alphaMode": "BLEND" }),
    );
    let mesh = g.mesh("Rifle", vec![json!({ "attributes": { "POSITION": a, "NORMAL": b, "TEXCOORD_0": c }, "indices": d, "material": metal })]);
    let root = g.node(json!({ "name": "Rifle", "mesh": mesh }), None);
    g.node(json!({ "name": "mountPoint", "translation": [0.0, -0.1, 0.0], "rotation": Quat::from_rotation_y(0.4).to_array() }), Some(root));
    g.node(
        json!({ "name": "muzzlePoint", "translation": [0.0, 0.05, 1.2] }),
        Some(root),
    );
    g.glb()
}

/// A small room the way `docs/modding/maps.md` asks: a floor and four walls,
/// a `spawn` Empty, a strobe point light and a spot light from
/// `KHR_lights_punctual` with `bri_` custom properties, and, with
/// `col_floor`, a separate collision floor.
pub fn room(col_floor: bool) -> Vec<u8> {
    let mut g = Gltf::new();
    let flat = |v: &[Vec3]| v.iter().flat_map(|v| v.to_array()).collect::<Vec<f32>>();
    let part = |g: &mut Gltf, name: &str, center: Vec3, size: Vec3, material: usize| {
        let (p, n, uv, i) = cuboid(center, size);
        let (a, b, c, d) = (
            g.floats(&flat(&p), 3),
            g.floats(&flat(&n), 3),
            g.floats(&uv.concat(), 2),
            g.uints(&i, 1),
        );
        let mesh = g.mesh(name, vec![json!({ "attributes": { "POSITION": a, "NORMAL": b, "TEXCOORD_0": c }, "indices": d, "material": material })]);
        g.node(json!({ "name": name, "mesh": mesh }), None);
    };
    let wood = g.material(
        "Wood",
        [1.0, 1.0, 1.0, 1.0],
        Some((
            2,
            2,
            [[150u8, 100, 60, 255], [120, 80, 50, 255]]
                .repeat(2)
                .concat(),
        )),
        json!({}),
    );
    let paint = g.material("Paint", [0.8, 0.8, 0.9, 1.0], None, json!({}));
    part(
        &mut g,
        "Floor",
        Vec3::new(0.0, -0.25, 0.0),
        Vec3::new(20.0, 0.5, 20.0),
        wood,
    );
    for (name, center, size) in [
        (
            "WallN",
            Vec3::new(0.0, 2.0, -10.0),
            Vec3::new(20.0, 4.0, 0.5),
        ),
        (
            "WallS",
            Vec3::new(0.0, 2.0, 10.0),
            Vec3::new(20.0, 4.0, 0.5),
        ),
        (
            "WallE",
            Vec3::new(10.0, 2.0, 0.0),
            Vec3::new(0.5, 4.0, 20.0),
        ),
        (
            "WallW",
            Vec3::new(-10.0, 2.0, 0.0),
            Vec3::new(0.5, 4.0, 20.0),
        ),
    ] {
        part(&mut g, name, center, size, paint);
    }
    if col_floor {
        part(
            &mut g,
            "col_floor",
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(20.0, 1.0, 20.0),
            paint,
        );
    }
    g.node(
        json!({ "name": "spawn", "translation": [2.0, 0.0, 3.0] }),
        None,
    );
    // A teleporter: in at one corner, out at the other facing glTF +X.
    g.node(
        json!({ "name": "teleport_in", "translation": [-5.0, 0.0, -5.0],
                "extras": { "bri_channel": "a", "bri_radius": 1.5 } }),
        None,
    );
    let quarter = std::f32::consts::FRAC_1_SQRT_2;
    g.node(
        json!({ "name": "teleport_out.001", "translation": [5.0, 0.0, 5.0],
                "rotation": [0.0, -quarter, 0.0, quarter],
                "extras": { "bri_channel": "a" } }),
        None,
    );
    g.json["extensionsUsed"] = json!(["KHR_lights_punctual"]);
    g.json["extensions"] = json!({ "KHR_lights_punctual": { "lights": [
        { "name": "Lamp", "type": "point", "color": [1.0, 0.8, 0.6], "intensity": 100.0 },
        { "name": "Spot", "type": "spot", "color": [0.5, 0.5, 1.0], "range": 6.0,
          "spot": { "innerConeAngle": 0.3, "outerConeAngle": 0.6 } },
    ] } });
    g.node(json!({ "name": "Lamp", "translation": [0.0, 3.0, 0.0],
        "extensions": { "KHR_lights_punctual": { "light": 0 } },
        "extras": { "bri_light_type": "strobe", "bri_radius": 8.0, "bri_brightness": 1.5, "bri_speed": 2.0 } }), None);
    g.node(
        json!({ "name": "Spot", "translation": [-5.0, 3.5, -5.0],
        "rotation": Quat::from_rotation_x(-1.2).to_array(),
        "extensions": { "KHR_lights_punctual": { "light": 1 } } }),
        None,
    );
    g.glb()
}
