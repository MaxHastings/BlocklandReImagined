//! The parts of a glTF 2.0 file the importer reads: the JSON document, its
//! buffers (the GLB binary chunk, `data:` URIs or files beside a `.gltf`)
//! and typed accessor data. Nothing here knows about the native model.
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::path::Path;

/// Largest model file read, and largest buffer a `.gltf` may name.
const MAX_BYTES: u64 = 256 << 20;

pub struct Document {
    pub json: Value,
    buffers: Vec<Vec<u8>>,
}

impl Document {
    /// Read a `.glb` or a `.gltf` (whose buffers and images may be files
    /// beside it, never outside its folder).
    pub fn open(path: &Path) -> Result<Self> {
        let meta =
            std::fs::metadata(path).with_context(|| format!("{} does not open", path.display()))?;
        ensure!(
            meta.len() <= MAX_BYTES,
            "{} is larger than 256 MB",
            path.display()
        );
        let bytes = std::fs::read(path)?;
        let folder = path.parent().unwrap_or(Path::new("."));
        Self::from_bytes(&bytes, Some(folder))
    }

    pub fn from_bytes(bytes: &[u8], folder: Option<&Path>) -> Result<Self> {
        let (json, bin) = if bytes.starts_with(b"glTF") {
            glb(bytes)?
        } else {
            (bytes.to_vec(), None)
        };
        let json: Value = serde_json::from_slice(&json).context("the glTF JSON does not read")?;
        let version = json["asset"]["version"].as_str().unwrap_or_default();
        ensure!(
            version.starts_with("2."),
            "glTF version {version:?} is not supported: export glTF 2.0"
        );
        if let Some(required) = json["extensionsRequired"].as_array() {
            let known = ["KHR_materials_unlit", "KHR_texture_transform"];
            for ext in required.iter().filter_map(Value::as_str) {
                ensure!(
                    known.contains(&ext),
                    "the model requires the {ext} extension, which the importer does not read: turn off compression (Draco, meshopt) and KTX/WebP textures in the exporter"
                );
            }
        }
        let mut buffers = Vec::new();
        for (i, buffer) in array(&json, "buffers").iter().enumerate() {
            let data = match buffer["uri"].as_str() {
                None => bin
                    .clone()
                    .with_context(|| format!("buffer {i} has no data: export a .glb"))?,
                Some(uri) => resource(uri, folder).with_context(|| format!("buffer {i}"))?,
            };
            let length = buffer["byteLength"].as_u64().unwrap_or(0) as usize;
            ensure!(
                data.len() >= length,
                "buffer {i} is shorter than its byteLength"
            );
            buffers.push(data);
        }
        Ok(Self { json, buffers })
    }

    /// An image's encoded bytes (PNG or JPEG), from a buffer view or a URI.
    pub fn image(&self, index: usize, folder: Option<&Path>) -> Result<Vec<u8>> {
        let image = &array(&self.json, "images")
            .get(index)
            .with_context(|| format!("image {index} does not exist"))?;
        if let Some(view) = image["bufferView"].as_u64() {
            let (bytes, _) = self.view(view as usize)?;
            return Ok(bytes.to_vec());
        }
        let uri = image["uri"]
            .as_str()
            .with_context(|| format!("image {index} has no data"))?;
        resource(uri, folder)
    }

    fn view(&self, index: usize) -> Result<(&[u8], Option<usize>)> {
        let view = array(&self.json, "bufferViews")
            .get(index)
            .with_context(|| format!("buffer view {index} does not exist"))?;
        let buffer = self
            .buffers
            .get(view["buffer"].as_u64().unwrap_or(u64::MAX) as usize)
            .with_context(|| format!("buffer view {index} names no buffer"))?;
        let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize;
        let length = view["byteLength"].as_u64().unwrap_or(0) as usize;
        let bytes = buffer
            .get(offset..offset.checked_add(length).context("view overflow")?)
            .with_context(|| format!("buffer view {index} runs past its buffer"))?;
        Ok((bytes, view["byteStride"].as_u64().map(|s| s as usize)))
    }

    /// Accessor `index` as rows of floats (normalized integers scaled to
    /// 0..1 or -1..1, plain integers as their values).
    pub fn floats(&self, index: usize) -> Result<Vec<Vec<f32>>> {
        self.read(index, |c, n| match (c, n) {
            (Component::F32(v), _) => v,
            (Component::U8(v), true) => v as f32 / 255.0,
            (Component::I8(v), true) => (v as f32 / 127.0).max(-1.0),
            (Component::U16(v), true) => v as f32 / 65535.0,
            (Component::I16(v), true) => (v as f32 / 32767.0).max(-1.0),
            (Component::U32(v), true) => v as f32 / u32::MAX as f32,
            (Component::U8(v), false) => v as f32,
            (Component::I8(v), false) => v as f32,
            (Component::U16(v), false) => v as f32,
            (Component::I16(v), false) => v as f32,
            (Component::U32(v), false) => v as f32,
        })
    }

    /// Accessor `index` as rows of unsigned integers (indices, joints).
    pub fn uints(&self, index: usize) -> Result<Vec<Vec<u32>>> {
        let rows = self.read(index, |c, _| match c {
            Component::U8(v) => v as f32,
            Component::U16(v) => v as f32,
            Component::U32(v) => v as f32,
            Component::I8(v) => v.max(0) as f32,
            Component::I16(v) => v.max(0) as f32,
            Component::F32(v) => v,
        });
        // f32 holds every u16 exactly, and u32 indices up to 2^24, far more
        // vertices than a model the game draws.
        Ok(rows?
            .into_iter()
            .map(|r| r.into_iter().map(|v| v as u32).collect())
            .collect())
    }

    fn read(
        &self,
        index: usize,
        convert: impl Fn(Component, bool) -> f32,
    ) -> Result<Vec<Vec<f32>>> {
        let accessor = array(&self.json, "accessors")
            .get(index)
            .with_context(|| format!("accessor {index} does not exist"))?;
        let count = accessor["count"]
            .as_u64()
            .context("accessor without count")? as usize;
        ensure!(count <= 1 << 24, "accessor {index} is too large");
        let width = match accessor["type"].as_str() {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            Some("MAT4") => 16,
            other => bail!("accessor {index} has unsupported type {other:?}"),
        };
        let kind = accessor["componentType"].as_u64().unwrap_or(0);
        let size = component_size(kind)
            .with_context(|| format!("accessor {index} has unknown componentType {kind}"))?;
        let normalized = accessor["normalized"].as_bool().unwrap_or(false);
        let mut rows = vec![vec![0.0; width]; count];
        if let Some(view) = accessor["bufferView"].as_u64() {
            let (bytes, stride) = self.view(view as usize)?;
            let offset = accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
            let stride = stride.unwrap_or(size * width);
            for (i, row) in rows.iter_mut().enumerate() {
                for (j, value) in row.iter_mut().enumerate() {
                    let at = offset + i * stride + j * size;
                    let raw = bytes
                        .get(at..at + size)
                        .with_context(|| format!("accessor {index} runs past its buffer view"))?;
                    *value = convert(component(kind, raw), normalized);
                }
            }
        }
        if let Some(sparse) = accessor.get("sparse") {
            let n = sparse["count"].as_u64().unwrap_or(0) as usize;
            let (ib, _) =
                self.view(sparse["indices"]["bufferView"].as_u64().unwrap_or(u64::MAX) as usize)?;
            let ikind = sparse["indices"]["componentType"].as_u64().unwrap_or(0);
            let isize = component_size(ikind).context("sparse index type")?;
            let ioff = sparse["indices"]["byteOffset"].as_u64().unwrap_or(0) as usize;
            let (vb, _) =
                self.view(sparse["values"]["bufferView"].as_u64().unwrap_or(u64::MAX) as usize)?;
            let voff = sparse["values"]["byteOffset"].as_u64().unwrap_or(0) as usize;
            for k in 0..n {
                let at = ioff + k * isize;
                let target =
                    match component(ikind, ib.get(at..at + isize).context("sparse indices")?) {
                        Component::U8(v) => v as usize,
                        Component::U16(v) => v as usize,
                        Component::U32(v) => v as usize,
                        _ => bail!("sparse indices must be unsigned"),
                    };
                let row = rows.get_mut(target).context("sparse index out of range")?;
                for (j, value) in row.iter_mut().enumerate() {
                    let at = voff + (k * width + j) * size;
                    *value = convert(
                        component(kind, vb.get(at..at + size).context("sparse values")?),
                        normalized,
                    );
                }
            }
        }
        ensure!(
            rows.iter().flatten().all(|v| v.is_finite()),
            "accessor {index} holds non-finite numbers"
        );
        Ok(rows)
    }
}

pub fn array<'a>(json: &'a Value, key: &str) -> &'a [Value] {
    json[key].as_array().map_or(&[], Vec::as_slice)
}

enum Component {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    U32(u32),
    F32(f32),
}
fn component_size(kind: u64) -> Option<usize> {
    Some(match kind {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return None,
    })
}
fn component(kind: u64, raw: &[u8]) -> Component {
    match kind {
        5120 => Component::I8(raw[0] as i8),
        5121 => Component::U8(raw[0]),
        5122 => Component::I16(i16::from_le_bytes([raw[0], raw[1]])),
        5123 => Component::U16(u16::from_le_bytes([raw[0], raw[1]])),
        5125 => Component::U32(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])),
        _ => Component::F32(f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])),
    }
}

/// The JSON and binary chunks of a GLB container.
fn glb(bytes: &[u8]) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
    let word = |at: usize| -> Result<u32> {
        let b = bytes.get(at..at + 4).context("the .glb is cut short")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    ensure!(word(4)? == 2, "the .glb is not glTF 2.0");
    let length = (word(8)? as usize).min(bytes.len());
    let (mut at, mut json, mut bin) = (12, None, None);
    while at + 8 <= length {
        let size = word(at)? as usize;
        let kind = word(at + 4)?;
        let data = bytes
            .get(at + 8..at + 8 + size)
            .context("a .glb chunk runs past the file")?
            .to_vec();
        match kind {
            0x4E4F_534A if json.is_none() => json = Some(data),
            0x004E_4942 if bin.is_none() => bin = Some(data),
            _ => {}
        }
        at += 8 + size.div_ceil(4) * 4;
    }
    Ok((json.context("the .glb has no JSON chunk")?, bin))
}

/// A buffer or image URI: embedded base64 data, or a file in `folder`.
fn resource(uri: &str, folder: Option<&Path>) -> Result<Vec<u8>> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let (_, data) = rest
            .split_once(";base64,")
            .context("only base64 data URIs are read")?;
        return base64(data);
    }
    let folder = folder.context("an embedded model cannot name outside files")?;
    let name = percent_decode(uri);
    ensure!(
        !name.contains("..") && !Path::new(&name).is_absolute() && !name.contains(':'),
        "{uri} is outside the model's folder"
    );
    let path = folder.join(&name);
    let meta =
        std::fs::metadata(&path).with_context(|| format!("{} is missing", path.display()))?;
    ensure!(
        meta.len() <= MAX_BYTES,
        "{} is larger than 256 MB",
        path.display()
    );
    Ok(std::fs::read(path)?)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn base64(data: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in data
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
    {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => bail!("invalid base64 data"),
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_and_percent_decoding() {
        assert_eq!(base64("aGVsbG8gd29ybGQ=").unwrap(), b"hello world");
        assert_eq!(base64("AAEC/w==").unwrap(), [0, 1, 2, 255]);
        assert_eq!(percent_decode("my%20tex.png"), "my tex.png");
    }

    #[test]
    fn a_compressed_model_is_refused_with_the_fix() {
        let json =
            br#"{"asset":{"version":"2.0"},"extensionsRequired":["KHR_draco_mesh_compression"]}"#;
        let err = Document::from_bytes(json, None).err().unwrap().to_string();
        assert!(err.contains("Draco"), "{err}");
    }
}
