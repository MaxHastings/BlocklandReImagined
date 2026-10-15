//! Writing glTF 2.0 binaries (`.glb`): nodes, meshes, materials with
//! embedded images, skins and clips. Tests build models with it, and
//! `export` writes converted maps for Blender.
use glam::Mat4;
use serde_json::{Value, json};

/// One animation channel: node, path, interpolation, key times, values.
pub type Channel<'a> = (usize, &'a str, &'a str, &'a [f32], &'a [f32]);

#[derive(Default)]
pub struct Gltf {
    pub json: Value,
    bin: Vec<u8>,
}

impl Gltf {
    pub fn new() -> Self {
        Self {
            json: json!({
                "asset": { "version": "2.0", "generator": "Blockland ReImagined" },
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

    /// Triangle indices, 16-bit when they fit, else 32-bit.
    pub fn indices(&mut self, data: &[u32]) -> usize {
        if data.iter().all(|&i| i <= u32::from(u16::MAX)) {
            return self.uints(data, 1);
        }
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes);
        self.push(
            "accessors",
            json!({ "bufferView": view, "componentType": 5125, "count": data.len(), "type": "SCALAR" }),
        )
    }

    /// An embedded image (PNG or JPEG bytes) as a texture.
    pub fn texture(&mut self, bytes: &[u8], mime: &str) -> usize {
        let view = self.view(bytes);
        let image = self.push("images", json!({ "bufferView": view, "mimeType": mime }));
        let sampler = self.push("samplers", json!({ "wrapS": 10497, "wrapT": 10497 }));
        self.push("textures", json!({ "source": image, "sampler": sampler }))
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
