//! Static mesh lighting from the asset lane's lighting inputs (`--lighting DIR`): per static-mesh actor the lights that reach it and a
//! visibility bit per vertex per light, and per vertex of the map's world mesh which actor and which of that actor's vertices it is.
//! The light each vertex takes is [`bri_content::vertex_lighting`]'s sum, computed here in the lane's package frame (units and axes
//! of the map file the lane read) and written as the vertex colours of the world mesh's vertex-lit surfaces.
//!
//! Files in `DIR` (found by suffix): `*-instances.json` (`instances`: actor, vertices, lights as export numbers, scale_glow, unlit,
//! static_lighting, zone), `*-lights.json` (`lights`: export, kind, color, brightness, location, rotation, original with LightRadius,
//! LightEffect, LightCone), `*-instances.npz` (`masks_<actor>`: one row of visibility bits per light, least significant bit first) and
//! `*-vertex-map.npz` (`actor_<k>`, `vertex_<k>`: for primitive `k` of the world mesh, the actor and its vertex of each vertex).
//!
//! The lane's glTF is `(-y, z, x) * 0.02` of the package's `(x, y, z)`, which is undone to compare vertices with the lights.
use anyhow::{Context, Result, ensure};
use bri_content::vertex_lighting::{Effect, Light, Rig, Set};
use glam::Vec3;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// The world mesh's node in the lane's glTF.
pub(super) const WORLD_NODE: &str = "aa25_pipeline_generated";
/// The lane's glTF length unit per package unit.
const UNIT: f32 = 0.02;
/// `LightEffect` values.
const STATIC_SPOT: i64 = 8;
const SPOTLIGHT: i64 = 12;
const NEGATIVE: i64 = 19;
const SUNLIGHT: i64 = 20;

struct Instance {
    vertices: usize,
    lights: Vec<i64>,
    scale_glow: f32,
    unlit: bool,
    static_lighting: bool,
    zone: usize,
}

struct Masks {
    rows: usize,
    bytes: usize,
    data: Vec<u8>,
}

pub(super) struct StaticLighting {
    instances: HashMap<i64, Instance>,
    lights: HashMap<i64, Value>,
    masks: HashMap<i64, Masks>,
    vertex_map: HashMap<usize, (Vec<i32>, Vec<i32>)>,
    ambient: Vec<[f32; 3]>,
}

/// An `.npy` array: its dtype text, shape and raw little-endian bytes.
fn npy(bytes: &[u8]) -> Result<(String, Vec<usize>, &[u8])> {
    ensure!(
        bytes.len() > 10 && &bytes[..6] == b"\x93NUMPY",
        "not an npy array"
    );
    let (header_at, header_len) = if bytes[6] == 1 {
        (10, usize::from(u16::from_le_bytes([bytes[8], bytes[9]])))
    } else {
        (
            12,
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize,
        )
    };
    let header = std::str::from_utf8(&bytes[header_at..header_at + header_len])?;
    let after = |key: &str| -> Result<&str> {
        let at = header
            .find(key)
            .with_context(|| format!("npy header has no {key}"))?;
        Ok(&header[at + key.len()..])
    };
    let descr = after("'descr':")?
        .trim_start()
        .trim_start_matches('\'')
        .split('\'')
        .next()
        .unwrap_or_default()
        .to_string();
    let shape_text = after("'shape':")?
        .trim_start()
        .trim_start_matches('(')
        .split(')')
        .next()
        .unwrap_or_default();
    let shape = shape_text
        .split(',')
        .filter_map(|n| n.trim().parse::<usize>().ok())
        .collect();
    Ok((descr, shape, &bytes[header_at + header_len..]))
}

fn npz(path: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)
        .with_context(|| format!("Reading {}", path.display()))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name().trim_end_matches(".npy").to_string();
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        out.push((name, data));
    }
    Ok(out)
}

fn find(dir: &Path, suffix: &str) -> Result<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("Reading {}", dir.display()))? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(suffix))
        {
            return Ok(path);
        }
    }
    anyhow::bail!("{} has no file ending {suffix}", dir.display())
}

fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
fn triple(v: &Value) -> [f64; 3] {
    [number(&v[0]), number(&v[1]), number(&v[2])]
}

/// A package rotation (pitch, yaw in 65536ths of a turn) as the direction it points along.
fn direction(rotation: [i32; 3]) -> [f32; 3] {
    let turn = std::f32::consts::TAU / 65536.0;
    let (pitch, yaw) = (rotation[0] as f32 * turn, rotation[1] as f32 * turn);
    [
        pitch.cos() * yaw.cos(),
        pitch.cos() * yaw.sin(),
        pitch.sin(),
    ]
}

impl StaticLighting {
    /// `ambient`: the light each zone's meshes take where no light reaches (linear RGB by zone number).
    pub(super) fn load(dir: &Path, ambient: Vec<[f32; 3]>) -> Result<Self> {
        let read_json = |suffix: &str| -> Result<Value> {
            let path = find(dir, suffix)?;
            serde_json::from_slice(&std::fs::read(&path)?)
                .with_context(|| format!("Reading {}", path.display()))
        };
        let mut instances = HashMap::new();
        for i in read_json("-instances.json")?["instances"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let Some(actor) = i["actor"].as_i64() else {
                continue;
            };
            instances.insert(
                actor,
                Instance {
                    vertices: i["vertices"].as_u64().unwrap_or(0) as usize,
                    lights: i["lights"]
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_i64).collect())
                        .unwrap_or_default(),
                    scale_glow: i["scale_glow"].as_f64().unwrap_or(1.0) as f32,
                    unlit: i["unlit"].as_bool().unwrap_or(false),
                    static_lighting: i["static_lighting"].as_bool().unwrap_or(false),
                    zone: i["zone"].as_u64().unwrap_or(0) as usize,
                },
            );
        }
        let lights = read_json("-lights.json")?["lights"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|l| Some((l["export"].as_i64()?, l)))
            .collect();
        let mut masks = HashMap::new();
        for (name, data) in npz(&find(dir, "-instances.npz")?)? {
            let Some(actor) = name
                .strip_prefix("masks_")
                .and_then(|n| n.parse::<i64>().ok())
            else {
                continue;
            };
            let (_, shape, raw) = npy(&data)?;
            ensure!(shape.len() == 2, "masks_{actor} is not rows of bytes");
            masks.insert(
                actor,
                Masks {
                    rows: shape[0],
                    bytes: shape[1],
                    data: raw.to_vec(),
                },
            );
        }
        let mut vertex_map: HashMap<usize, (Vec<i32>, Vec<i32>)> = HashMap::new();
        for (name, data) in npz(&find(dir, "-vertex-map.npz")?)? {
            let (kind, k) = match name.split_once('_') {
                Some((kind @ ("actor" | "vertex"), k)) => (kind.to_string(), k.parse::<usize>()?),
                _ => continue,
            };
            let (_, _, raw) = npy(&data)?;
            let values: Vec<i32> = raw
                .chunks_exact(4)
                .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            let entry = vertex_map.entry(k).or_default();
            if kind == "actor" {
                entry.0 = values;
            } else {
                entry.1 = values;
            }
        }
        Ok(Self {
            instances,
            lights,
            masks,
            vertex_map,
            ambient,
        })
    }

    fn set(&self, actor: i64) -> Option<Set> {
        let instance = self.instances.get(&actor)?;
        let masks = self.masks.get(&actor)?;
        let mut set = Set {
            ambient: self.ambient.get(instance.zone).copied().unwrap_or([0.0; 3]),
            ..Set::default()
        };
        for (row, export) in instance.lights.iter().enumerate() {
            let light = self.lights.get(export)?;
            if row >= masks.rows {
                return None;
            }
            let original = &light["original"];
            let effect_id = original["LightEffect"].as_i64().unwrap_or(0);
            let rotation = triple(&light["rotation"]).map(|v| v as i32);
            let effect = if light["kind"].as_str() == Some("directional") || effect_id == SUNLIGHT {
                Effect::Directional {
                    direction: direction(rotation),
                }
            } else if effect_id == STATIC_SPOT || effect_id == SPOTLIGHT {
                Effect::Spot {
                    direction: direction(rotation),
                    cone: original["LightCone"].as_f64().unwrap_or(128.0) as f32 / 256.0,
                }
            } else {
                Effect::Point
            };
            let color = triple(&light["color"]);
            let brightness = number(&light["brightness"]);
            set.lights.push(Light {
                position: triple(&light["location"]).map(|v| v as f32),
                color: color.map(|c| (c * brightness) as f32),
                radius: (original["LightRadius"].as_f64().unwrap_or(64.0) as f32 + 1.0) * 25.0,
                glow: instance.scale_glow,
                effect,
                negative: effect_id == NEGATIVE,
            });
            set.visible
                .push(masks.data[row * masks.bytes..(row + 1) * masks.bytes].to_vec());
        }
        set.validate(instance.vertices).ok()?;
        Some(set)
    }

    /// Whether primitive `k` of the world mesh has static-mesh vertices to light.
    pub(super) fn maps(&self, k: usize) -> bool {
        self.vertex_map.contains_key(&k)
    }
}

/// The world mesh's [`Rig`] as its primitives are read in order: for each vertex the set (one per static-mesh actor, shared between
/// its vertices) and the vertex within it.
#[derive(Default)]
pub(super) struct RigBuilder {
    sets: Vec<Set>,
    by_key: HashMap<i64, u32>,
    vertex_set: Vec<u32>,
    vertex_index: Vec<u32>,
}

impl RigBuilder {
    /// The next `count` vertices are primitive `k`'s: the world mesh's static meshes name their actor and vertex, others take no light.
    pub(super) fn extend(&mut self, lighting: &StaticLighting, k: usize, count: usize) {
        let entry = lighting
            .vertex_map
            .get(&k)
            .filter(|(actors, vertices)| actors.len() == count && vertices.len() == count);
        let Some((actors, vertices)) = entry else {
            self.vertex_set.extend(std::iter::repeat_n(0, count));
            self.vertex_index.extend(std::iter::repeat_n(0, count));
            return;
        };
        for i in 0..count {
            let set = self.set_of(lighting, i64::from(actors[i]));
            self.vertex_set.push(set);
            self.vertex_index.push(vertices[i].max(0) as u32);
        }
    }

    /// The set an actor's vertices take, plus one (0 for none). An unlit actor is a constant half (the doubling makes it the
    /// picture); one that takes no baked light keeps its zone's ambient (the engine lights it per frame with up to four lights, which
    /// this does not do).
    fn set_of(&mut self, lighting: &StaticLighting, actor: i64) -> u32 {
        let Some(instance) = lighting.instances.get(&actor) else {
            return 0;
        };
        let key = if instance.unlit {
            i64::MIN
        } else if !instance.static_lighting {
            i64::MIN + 1 + instance.zone as i64
        } else {
            actor
        };
        if let Some(&at) = self.by_key.get(&key) {
            return at;
        }
        let set = if instance.unlit {
            Some(Set {
                ambient: [1.0; 3],
                ..Set::default()
            })
        } else if !instance.static_lighting {
            Some(Set {
                ambient: lighting
                    .ambient
                    .get(instance.zone)
                    .copied()
                    .unwrap_or([0.0; 3]),
                ..Set::default()
            })
        } else {
            lighting.set(actor)
        };
        let at = match set {
            Some(set) => {
                self.sets.push(set);
                self.sets.len() as u32
            }
            None => 0,
        };
        self.by_key.insert(key, at);
        at
    }

    /// The rig, for mesh positions `m` placed from the lane's glTF `g` by `m = point(g * scale)`, with the package frame `p = (g.z,
    /// -g.x, g.y) / UNIT` that the lights are in.
    pub(super) fn finish(self, scale: Vec3) -> Rig {
        // p = (-m.z / s.z, m.x / s.x, m.y / s.y) / UNIT: column i is where mesh axis i goes.
        let cols = [
            [0.0, 1.0 / (scale.x * UNIT), 0.0],
            [0.0, 0.0, 1.0 / (scale.y * UNIT)],
            [-1.0 / (scale.z * UNIT), 0.0, 0.0],
            [0.0; 3],
        ];
        Rig {
            to_lights: std::array::from_fn(|i| cols[i / 3][i % 3]),
            sets: self.sets,
            vertex_set: self.vertex_set,
            vertex_index: self.vertex_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npy_bytes(descr: &str, shape: &str, data: &[u8]) -> Vec<u8> {
        let mut header =
            format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': {shape}, }}");
        while (10 + header.len() + 1) % 64 != 0 {
            header.push(' ');
        }
        header.push('\n');
        let mut out = b"\x93NUMPY\x01\x00".to_vec();
        out.extend((header.len() as u16).to_le_bytes());
        out.extend(header.as_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn an_npy_array_gives_its_dtype_shape_and_bytes() {
        let bytes = npy_bytes("|u1", "(2, 3)", &[1, 2, 3, 4, 5, 6]);
        let (descr, shape, data) = npy(&bytes).unwrap();
        assert_eq!(
            (descr.as_str(), shape.as_slice(), data),
            ("|u1", &[2, 3][..], &[1, 2, 3, 4, 5, 6][..])
        );
        let single = npy_bytes("<i4", "(5,)", &[0; 20]);
        assert_eq!(npy(&single).unwrap().1, vec![5]);
        assert!(npy(b"nope").is_err());
    }

    #[test]
    fn a_rotation_points_along_its_direction() {
        let d = direction([0, 0, 0]);
        assert!((Vec3::from(d) - Vec3::X).length() < 1e-6);
        // A quarter turn of yaw is +y; a quarter turn of pitch is straight up.
        assert!((Vec3::from(direction([0, 16384, 0])) - Vec3::Y).length() < 1e-5);
        assert!((Vec3::from(direction([16384, 0, 0])) - Vec3::Z).length() < 1e-5);
    }
}
