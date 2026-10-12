//! Writing small glTF models for tests: [`Gltf`] builds a `.glb` from nodes,
//! meshes and clips, and [`character`] is an original rigged figure laid out
//! the way the authoring guide asks (sockets, role clips, a scaled
//! armature, textures), so tests need no third-party model.
use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};

/// One animation channel: node, path, interpolation, key times, values.
pub type Channel<'a> = (usize, &'a str, &'a str, &'a [f32], &'a [f32]);
/// A box of the figure: centre, size and its `(bone, weight)` pairs.
type Part<'a> = (Vec3, Vec3, &'a [(u32, f32)]);

#[derive(Default)]
pub struct Gltf {
    pub json: Value,
    bin: Vec<u8>,
}

impl Gltf {
    pub fn new() -> Self {
        Self {
            json: json!({
                "asset": { "version": "2.0", "generator": "bri-addon-import tests" },
                "scene": 0,
                "scenes": [{ "nodes": [] }],
                "nodes": [], "meshes": [], "skins": [], "materials": [], "textures": [],
                "images": [], "samplers": [], "animations": [],
                "buffers": [], "bufferViews": [], "accessors": [],
            }),
            bin: Vec::new(),
        }
    }

    fn push(&mut self, key: &str, value: Value) -> usize {
        let a = self.json[key].as_array_mut().expect("array");
        a.push(value);
        a.len() - 1
    }

    fn view(&mut self, bytes: &[u8]) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        self.push(
            "bufferViews",
            json!({ "buffer": 0, "byteOffset": offset, "byteLength": bytes.len() }),
        )
    }

    /// A float accessor of `width` components per row.
    pub fn floats(&mut self, data: &[f32], width: usize) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes);
        let kind = match width {
            1 => "SCALAR",
            2 => "VEC2",
            3 => "VEC3",
            4 => "VEC4",
            _ => "MAT4",
        };
        let mut accessor = json!({ "bufferView": view, "componentType": 5126, "count": data.len() / width, "type": kind });
        if width == 3 {
            let rows: Vec<&[f32]> = data.chunks(3).collect();
            let min: Vec<f32> = (0..3)
                .map(|i| rows.iter().map(|r| r[i]).fold(f32::MAX, f32::min))
                .collect();
            let max: Vec<f32> = (0..3)
                .map(|i| rows.iter().map(|r| r[i]).fold(f32::MIN, f32::max))
                .collect();
            accessor["min"] = json!(min);
            accessor["max"] = json!(max);
        }
        self.push("accessors", accessor)
    }

    /// An unsigned accessor: 16-bit indices (`width` 1) or 8-bit joints.
    pub fn uints(&mut self, data: &[u32], width: usize) -> usize {
        let (bytes, kind): (Vec<u8>, u32) = if width == 1 {
            (
                data.iter()
                    .flat_map(|v| (*v as u16).to_le_bytes())
                    .collect(),
                5123,
            )
        } else {
            (data.iter().map(|v| *v as u8).collect(), 5121)
        };
        let view = self.view(&bytes);
        let kind_name = if width == 1 { "SCALAR" } else { "VEC4" };
        self.push(
            "accessors",
            json!({ "bufferView": view, "componentType": kind, "count": data.len() / width, "type": kind_name }),
        )
    }

    pub fn node(&mut self, value: Value, parent: Option<usize>) -> usize {
        let i = self.push("nodes", value);
        match parent {
            Some(p) => {
                let children = &mut self.json["nodes"][p]["children"];
                if !children.is_array() {
                    *children = json!([]);
                }
                children.as_array_mut().unwrap().push(json!(i));
            }
            None => {
                self.json["scenes"][0]["nodes"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(i));
            }
        }
        i
    }

    /// A material, textured with a PNG of `pixels` (RGBA rows) if given.
    pub fn material(
        &mut self,
        name: &str,
        factor: [f32; 4],
        pixels: Option<(u32, u32, Vec<u8>)>,
        extra: Value,
    ) -> usize {
        let mut m = json!({ "name": name, "pbrMetallicRoughness": { "baseColorFactor": factor } });
        if let Some((w, h, rgba)) = pixels {
            let mut png = Vec::new();
            image::RgbaImage::from_raw(w, h, rgba)
                .expect("pixels")
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            let view = self.view(&png);
            let image = self.push(
                "images",
                json!({ "bufferView": view, "mimeType": "image/png" }),
            );
            let sampler = self.push("samplers", json!({ "wrapS": 33071, "wrapT": 10497 }));
            let texture = self.push("textures", json!({ "source": image, "sampler": sampler }));
            m["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": texture });
        }
        if let Value::Object(extra) = extra {
            for (k, v) in extra {
                m[k] = v;
            }
        }
        self.push("materials", m)
    }

    pub fn mesh(&mut self, name: &str, primitives: Vec<Value>) -> usize {
        self.push("meshes", json!({ "name": name, "primitives": primitives }))
    }

    pub fn skin(&mut self, joints: &[usize], inverse_binds: &[Mat4]) -> usize {
        let data: Vec<f32> = inverse_binds
            .iter()
            .flat_map(|m| m.to_cols_array())
            .collect();
        let ibm = self.floats(&data, 16);
        self.push(
            "skins",
            json!({ "joints": joints, "inverseBindMatrices": ibm }),
        )
    }

    /// A clip: `(node, path, interpolation, times, values)` channels.
    pub fn animation(&mut self, name: &str, channels: &[Channel]) -> usize {
        let mut samplers = Vec::new();
        let mut out = Vec::new();
        for (i, (node, path, interpolation, times, values)) in channels.iter().enumerate() {
            let input = self.floats(times, 1);
            let width = if *path == "rotation" { 4 } else { 3 };
            let output = self.floats(values, width);
            samplers
                .push(json!({ "input": input, "output": output, "interpolation": interpolation }));
            out.push(json!({ "sampler": i, "target": { "node": node, "path": path } }));
        }
        self.push(
            "animations",
            json!({ "name": name, "samplers": samplers, "channels": out }),
        )
    }

    pub fn glb(mut self) -> Vec<u8> {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        self.json["buffers"] = json!([{ "byteLength": self.bin.len() }]);
        for key in [
            "skins",
            "materials",
            "textures",
            "images",
            "samplers",
            "animations",
        ] {
            if self.json[key].as_array().is_some_and(Vec::is_empty) {
                self.json.as_object_mut().unwrap().remove(key);
            }
        }
        let mut text = serde_json::to_vec(&self.json).unwrap();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let total = 12 + 8 + text.len() + 8 + self.bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        out.extend_from_slice(&text);
        out.extend_from_slice(&(self.bin.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        out.extend_from_slice(&self.bin);
        out
    }
}

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
    for (center, size, influence) in parts {
        let (p, n, uv, i) = cuboid(center, size);
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
        g.uints(&idx[..idx.len() - 36], 1),
        g.uints(&joints, 4),
        g.floats(&weights, 4),
    );
    let legs = g.uints(&idx[idx.len() - 36..], 1);
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
        &[(
            arm_r,
            "rotation",
            "LINEAR",
            &[0.0, 0.2],
            &[q(i), q(Quat::from_rotation_x(-1.5))].concat(),
        )],
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
