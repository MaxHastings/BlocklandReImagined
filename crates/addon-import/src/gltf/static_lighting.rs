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
    ambient_glow: u8,
}

/// One terrain's sectors (`*-terrain-lighting.json`): `UTerrainSector::StaticLight` lights each sector's vertices from its list of
/// lights and their per-vertex visibility bits, over a base of the terrain's zone ambient.
struct TerrainLight {
    zone: usize,
    level_brightness: f32,
    width: usize,
    sectors: Vec<TerrainSector>,
}

struct TerrainSector {
    export: i64,
    x0: usize,
    y0: usize,
    size_x: usize,
    size_y: usize,
    lights: Vec<i64>,
}

/// A terrain's name as the map file and the lane's glTF spell it: `TerrainInfo0` and `TerrainInfo_0` are one.
fn terrain_key(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '_')
        .collect::<String>()
        .to_ascii_lowercase()
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
    terrains: HashMap<String, TerrainLight>,
    regions: Option<bri_content::regions::RegionMap>,
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
    /// `regions`: the map's regions (in the lane's frame), whose ambient the meshes take where no light reaches.
    pub(super) fn load(
        dir: &Path,
        regions: Option<bri_content::regions::RegionMap>,
    ) -> Result<Self> {
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
                    ambient_glow: i["ambient_glow"].as_u64().unwrap_or(0).min(255) as u8,
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
        // The terrain's sector lights, when the lane gave them.
        let mut terrains = HashMap::new();
        if let (Ok(json_path), Ok(npz_path)) = (
            find(dir, "-terrain-lighting.json"),
            find(dir, "-terrain-lighting.npz"),
        ) {
            let file: Value = serde_json::from_slice(&std::fs::read(&json_path)?)
                .with_context(|| format!("Reading {}", json_path.display()))?;
            for (name, data) in npz(&npz_path)? {
                let Some(sector) = name
                    .strip_prefix("masks_")
                    .and_then(|n| n.parse::<i64>().ok())
                else {
                    continue;
                };
                let (_, shape, raw) = npy(&data)?;
                ensure!(shape.len() == 2, "masks_{sector} is not rows of bytes");
                masks.insert(
                    sector,
                    Masks {
                        rows: shape[0],
                        bytes: shape[1],
                        data: raw.to_vec(),
                    },
                );
            }
            for t in file["terrains"].as_array().cloned().unwrap_or_default() {
                let sectors: Vec<TerrainSector> = t["sectors"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|s| {
                        Some(TerrainSector {
                            export: s["export"].as_i64()?,
                            x0: s["x0"].as_u64()? as usize,
                            y0: s["y0"].as_u64()? as usize,
                            size_x: s["size_x"].as_u64()? as usize,
                            size_y: s["size_y"].as_u64()? as usize,
                            lights: s["lights"]
                                .as_array()
                                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                                .unwrap_or_default(),
                        })
                    })
                    .collect();
                let width = sectors
                    .iter()
                    .map(|s| s.x0 + s.size_x + 1)
                    .max()
                    .unwrap_or(0);
                let Some(name) = t["name"].as_str() else {
                    continue;
                };
                terrains.insert(
                    terrain_key(name),
                    TerrainLight {
                        zone: t["zone"].as_u64().unwrap_or(0) as usize,
                        level_brightness: t["level_brightness"].as_f64().unwrap_or(1.0) as f32,
                        width,
                        sectors,
                    },
                );
            }
        }
        Ok(Self {
            instances,
            lights,
            masks,
            vertex_map,
            terrains,
            regions,
        })
    }

    fn zone_ambient(&self, zone: usize) -> [f32; 3] {
        self.regions
            .as_ref()
            .and_then(|r| r.ambient(zone as u16))
            .unwrap_or([0.0; 3])
    }

    /// A placed light as the vertex sum takes it, its strength scaled by `glow`.
    fn light_of(&self, export: &i64, light: &Value, glow: f32) -> Light {
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
        Light {
            position: triple(&light["location"]).map(|v| v as f32),
            color: color.map(|c| (c * brightness) as f32),
            radius: (original["LightRadius"].as_f64().unwrap_or(64.0) as f32 + 1.0) * 25.0,
            glow,
            effect,
            negative: effect_id == NEGATIVE,
            id: u32::try_from(*export).unwrap_or(0),
        }
    }

    fn set(&self, actor: i64) -> Option<Set> {
        let instance = self.instances.get(&actor)?;
        // An actor no light reaches has no mask rows (the lane writes none): it still takes its ambient.
        let masks = self.masks.get(&actor);
        let mut set = Set {
            ambient: self.zone_ambient(instance.zone),
            ..Set::default()
        };
        for (row, export) in instance.lights.iter().enumerate() {
            let masks = masks.filter(|m| row < m.rows)?;
            let light = self.lights.get(export)?;
            set.lights
                .push(self.light_of(export, light, instance.scale_glow));
            set.visible
                .push(masks.data[row * masks.bytes..(row + 1) * masks.bytes].to_vec());
        }
        set.validate(instance.vertices).ok()?;
        Some(set)
    }

    /// The grid width of the terrain node called `name`, when the lane gave sector lights for it.
    pub(super) fn terrain_width(&self, name: &str) -> Option<usize> {
        self.terrains.get(&terrain_key(name)).map(|t| t.width)
    }

    /// Whether the lane gave sector lights for the terrain node called `name`.
    pub(super) fn has_terrain(&self, name: &str) -> bool {
        self.terrains.contains_key(&terrain_key(name))
    }

    /// A terrain sector as a set: its zone's ambient times Level.Brightness, then its lights with their visibility bits.
    fn sector_set(&self, terrain: &TerrainLight, sector: &TerrainSector) -> Option<Set> {
        let mut set = Set {
            ambient: self
                .zone_ambient(terrain.zone)
                .map(|a| a * terrain.level_brightness),
            ..Set::default()
        };
        let masks = self.masks.get(&sector.export);
        for (row, export) in sector.lights.iter().enumerate() {
            let masks = masks.filter(|m| row < m.rows)?;
            let light = self.lights.get(export)?;
            set.lights.push(self.light_of(export, light, 1.0));
            set.visible
                .push(masks.data[row * masks.bytes..(row + 1) * masks.bytes].to_vec());
        }
        set.validate((sector.size_x + 1) * (sector.size_y + 1))
            .ok()?;
        Some(set)
    }

    /// Whether primitive `k` of the world mesh has static-mesh vertices to light.
    pub(super) fn maps(&self, k: usize) -> bool {
        self.vertex_map.contains_key(&k)
    }
}

/// A terrain pass's vertex normals as the engine makes them (`ATerrainInfo::UpdateVertices` 0x2430f0 and `GetVertexNormal` 0x240290): each
/// grid cell has two triangles, `(a, b, d)` and `(a, d, c)` over its corners `a = (x, y)`, `b = (x + 1, y)`, `c = (x, y + 1)`, `d = (x + 1, y + 1)`,
/// each with a unit normal; a vertex takes the unit sum of both triangles of each of the (up to four) cells around it. `positions` are the
/// grid row by row; `reference` says which side is up (the sign of the glTF's own normals).
pub(super) fn engine_terrain_normals(
    positions: &[Vec3],
    reference: &[Vec3],
    width: usize,
) -> Vec<Vec3> {
    let at = |x: usize, y: usize| positions[y * width + x];
    let up = |n: Vec3, i: usize| {
        let n = n.normalize_or_zero();
        if n.dot(reference[i]) < 0.0 { -n } else { n }
    };
    let mut sum = vec![Vec3::ZERO; width * width];
    for y in 0..width - 1 {
        for x in 0..width - 1 {
            let i = y * width + x;
            let (a, b, c, d) = (at(x, y), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1));
            let cell = up((b - a).cross(d - a), i) + up((d - a).cross(c - a), i);
            for (cx, cy) in [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)] {
                sum[cy * width + cx] += cell;
            }
        }
    }
    sum.into_iter().map(Vec3::normalize_or_zero).collect()
}

/// The world mesh's [`Rig`] as its primitives are read in order: for each vertex the set (one per static-mesh actor, shared between
/// its vertices) and the vertex within it.
#[derive(Default)]
pub(super) struct RigBuilder {
    sets: Vec<Set>,
    by_key: HashMap<i64, u32>,
    vertex_set: Vec<u32>,
    vertex_index: Vec<u32>,
    /// Each actor-keyed set's render box (the lane's frame) and glow, for the ambient of the zones the box reaches.
    boxes: HashMap<u32, (Vec3, Vec3, f32)>,
    /// Per terrain, each grid vertex's set and index within it.
    terrain_tables: HashMap<String, Vec<(u32, u32)>>,
}

impl RigBuilder {
    /// The next `count` vertices are primitive `k`'s: the world mesh's static meshes name their actor and vertex, others take no light.
    pub(super) fn extend(&mut self, lighting: &StaticLighting, k: usize, positions: &[Vec3]) {
        let count = positions.len();
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
            if let Some(entry) = self.boxes.get_mut(&set) {
                entry.0 = entry.0.min(positions[i]);
                entry.1 = entry.1.max(positions[i]);
            }
            self.vertex_set.push(set);
            self.vertex_index.push(vertices[i].max(0) as u32);
        }
    }

    /// The next `count` vertices are a terrain pass's, in grid order (row by row): each takes the set of the last sector that holds it, as
    /// the engine writes the sectors in turn. A pass of another size takes no light.
    pub(super) fn extend_terrain(&mut self, lighting: &StaticLighting, name: &str, count: usize) {
        let key = terrain_key(name);
        let Some(terrain) = lighting.terrains.get(&key) else {
            self.vertex_set.extend(std::iter::repeat_n(0, count));
            self.vertex_index.extend(std::iter::repeat_n(0, count));
            return;
        };
        if !self.terrain_tables.contains_key(&key) {
            let mut table = vec![(0u32, 0u32); terrain.width * terrain.width];
            for sector in &terrain.sectors {
                let Some(set) = lighting.sector_set(terrain, sector) else {
                    continue;
                };
                self.sets.push(set);
                let at = self.sets.len() as u32;
                for y in 0..=sector.size_y {
                    for x in 0..=sector.size_x {
                        let (gx, gy) = (sector.x0 + x, sector.y0 + y);
                        if gx < terrain.width && gy < terrain.width {
                            table[gy * terrain.width + gx] =
                                (at, (y * (sector.size_x + 1) + x) as u32);
                        }
                    }
                }
            }
            self.terrain_tables.insert(key.clone(), table);
        }
        let table = &self.terrain_tables[&key];
        if count != table.len() {
            self.vertex_set.extend(std::iter::repeat_n(0, count));
            self.vertex_index.extend(std::iter::repeat_n(0, count));
            return;
        }
        for &(set, index) in table {
            self.vertex_set.push(set);
            self.vertex_index.push(index);
        }
    }

    /// The set an actor's vertices take, plus one (0 for none). An unlit actor is a constant half (the doubling makes it the
    /// picture); one that takes no baked light keeps its zone's ambient (the engine lights it per frame with up to four lights, which
    /// this does not do).
    fn set_of(&mut self, lighting: &StaticLighting, actor: i64) -> u32 {
        let Some(instance) = lighting.instances.get(&actor) else {
            return 0;
        };
        let key = if instance.unlit { i64::MIN } else { actor };
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
                ambient: lighting.zone_ambient(instance.zone),
                ..Set::default()
            })
        } else {
            lighting.set(actor)
        };
        let at = match set {
            Some(set) => {
                self.sets.push(set);
                let at = self.sets.len() as u32;
                if !instance.unlit {
                    // AmbientGlow 255 pulses with the clock in the engine; its mean is used.
                    let glow = if instance.ambient_glow == 255 {
                        0.25
                    } else {
                        f32::from(instance.ambient_glow) / 255.0
                    };
                    self.boxes
                        .insert(at, (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), glow));
                }
                at
            }
            None => 0,
        };
        self.by_key.insert(key, at);
        at
    }

    /// The rig, for mesh positions `m` placed from the lane's glTF `g` by `m = point(g * scale)`, with the package frame `p = (g.z,
    /// -g.x, g.y) / UNIT` that the lights are in.
    pub(super) fn finish(mut self, scale: Vec3, lighting: &StaticLighting) -> Rig {
        // An actor's ambient is the mean over the zones its render box reaches, plus its glow (`AActor::UpdateRenderData`).
        if let Some(regions) = &lighting.regions {
            for (at, (min, max, glow)) in &self.boxes {
                if min.x <= max.x {
                    self.sets[*at as usize - 1].ambient =
                        regions.actor_ambient(min.to_array(), max.to_array(), *glow);
                }
            }
        }
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
