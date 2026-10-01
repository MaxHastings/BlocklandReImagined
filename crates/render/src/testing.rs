//! A made-up native map bundle for tests that have no converted maps: closed
//! lightmapped rooms, each lit by one invented lamp, with a spawn, a sun
//! and a sky, written in the layout `scene_loader::load_map_bundle` reads.
//! Every number here is invented; nothing is read from an original map.
use crate::map_lighting::MapLight;
use anyhow::{Context, Result};
use bri_content::{
    interior::{Detail, Interior, Lightmap, Surface, Vertex},
    scene::{Kind, Node, Scene},
};
use glam::{Mat4, Vec3};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

/// Lightmap edge length in texels.
pub const LIGHTMAP_SIZE: u32 = 16;

/// One room map: an axis-aligned box room (faces pointing in) placed at
/// `origin`, its floor `floor` below the origin and its ceiling `height`
/// above the floor, `half` across in x and z.
#[derive(Clone, Debug)]
pub struct RoomMap {
    pub id: String,
    pub name: String,
    pub origin: Vec3,
    pub half: f32,
    pub floor: f32,
    pub height: f32,
    /// Spawn above the floor, relative to the origin in x and z.
    pub spawn: Vec3,
    /// The sun's authored angles in degrees; its `direction` field is
    /// written stale on purpose.
    pub azimuth: f32,
    pub elevation: f32,
    /// The lamp baked into the lightmaps, in world space as authored.
    pub lamp: MapLight,
    /// Light every texel has besides the lamp's.
    pub ambient: f32,
}

impl RoomMap {
    /// The floor's authored world height.
    pub fn floor_height(&self) -> f32 {
        self.origin.y - self.floor
    }
    /// The spawn's authored world position.
    pub fn spawn_position(&self) -> Vec3 {
        Vec3::new(
            self.origin.x + self.spawn.x,
            self.floor_height() + self.spawn.y,
            self.origin.z + self.spawn.z,
        )
    }
}

/// Two rooms whose floors are off the plate lattice in opposite directions,
/// dimly sunlit, each with a lamp of its own colour and reach in a
/// corner near the floor.
pub fn rooms() -> Vec<RoomMap> {
    vec![
        RoomMap {
            id: "fixture/maps/den.mis".into(),
            name: "Den".into(),
            origin: Vec3::new(12.0, 40.0, -6.0),
            half: 40.0,
            floor: 7.93,
            height: 24.0,
            spawn: Vec3::new(-4.0, 1.25, 3.0),
            azimuth: 210.0,
            elevation: 40.0,
            lamp: MapLight {
                position: [48.0, 36.5, -42.0],
                color: [1.0, 0.9, 0.7],
                inner: 4.0,
                outer: 14.0,
                channel: None,
            },
            ambient: 0.06,
        },
        RoomMap {
            id: "fixture/maps/cellar.mis".into(),
            name: "Cellar".into(),
            origin: Vec3::new(-30.0, 75.0, 20.0),
            half: 44.0,
            floor: 14.33,
            height: 18.0,
            spawn: Vec3::new(2.0, 2.0, -5.0),
            azimuth: 95.0,
            elevation: 70.0,
            lamp: MapLight {
                position: [-70.0, 64.5, 60.0],
                color: [0.8, 0.9, 1.0],
                inner: 3.0,
                outer: 16.0,
                channel: None,
            },
            ambient: 0.04,
        },
    ]
}

fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Result<Vec<u8>> {
    let image = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba(pixel(x, y)));
    let mut bytes = Vec::new();
    image.write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )?;
    Ok(bytes)
}

fn translation(p: Vec3) -> [f32; 16] {
    Mat4::from_translation(p).to_cols_array()
}

fn node(
    name: &str,
    kind: Kind,
    at: Vec3,
    asset: Option<&str>,
    properties: &[(&str, String)],
) -> Node {
    Node {
        name: name.into(),
        parent: None,
        kind,
        transform: translation(at),
        asset: asset.map(str::to_owned),
        properties: properties
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    }
}

/// The room's interior: six inward faces, each with its own lightmap of
/// what the lamp (and the ambient) casts on it, and the floor as collision.
pub fn interior(map: &RoomMap) -> Result<Interior> {
    let (lo, hi) = (
        Vec3::new(-map.half, -map.floor, -map.half),
        Vec3::new(map.half, map.height - map.floor, map.half),
    );
    let mut surfaces = vec![];
    let mut lightmaps = vec![];
    let mut floor = vec![];
    for axis in 0..3 {
        for side in [-1.0f32, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let corner = |a: f32, b: f32| {
                let mut p = Vec3::ZERO;
                p[axis] = if side < 0.0 { lo[axis] } else { hi[axis] };
                p[u] = lo[u] + (hi[u] - lo[u]) * a;
                p[v] = lo[v] + (hi[v] - lo[v]) * b;
                p
            };
            let mut normal = Vec3::ZERO;
            normal[axis] = -side;
            let n = LIGHTMAP_SIZE;
            let bytes = png(n, n, |x, y| {
                let a = (x as f32 + 0.5) / n as f32;
                let b = (y as f32 + 0.5) / n as f32;
                let c = (map.lamp.shade(corner(a, b) + map.origin, normal) + map.ambient)
                    .min(Vec3::ONE)
                    * 255.0
                    + 0.5;
                [c.x as u8, c.y as u8, c.z as u8, 255]
            })?;
            let slot = lightmaps.len();
            lightmaps.push(Lightmap {
                png: bytes,
                auxiliary_png: None,
                keep: false,
            });
            let vertices: Vec<Vertex> = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
                .into_iter()
                .map(|(a, b)| Vertex {
                    position: corner(a, b).to_array(),
                    normal: normal.to_array(),
                    uv: [a * 4.0, b * 4.0],
                    lightmap_uv: [a, b],
                })
                .collect();
            // Wound so the face points along `normal`.
            let toward = (Vec3::from(vertices[1].position) - Vec3::from(vertices[0].position))
                .cross(Vec3::from(vertices[2].position) - Vec3::from(vertices[0].position))
                .dot(normal)
                > 0.0;
            let triangles = if toward {
                vec![[0, 1, 2], [0, 2, 3]]
            } else {
                vec![[0, 2, 1], [0, 3, 2]]
            };
            if axis == 1 && side < 0.0 {
                for t in &triangles {
                    floor.push(t.map(|i| vertices[i as usize].position));
                }
            }
            surfaces.push(Surface {
                source_index: surfaces.len(),
                material: 0,
                flags: 0,
                vertices,
                triangles,
                lightmap: Some(slot),
                alarm_lightmap: None,
            });
        }
    }
    let interior = Interior {
        schema_version: 1,
        id: format!("fixture/interiors/{}", map.name.to_ascii_lowercase()),
        details: vec![Detail {
            minimum_pixels: 0,
            materials: vec!["fixture_wall".into()],
            surfaces,
            lightmaps,
            collision_triangles: floor,
            convex_hulls: vec![],
            ambient: [0; 4],
            alarm_ambient: [0; 4],
            has_alarm: false,
        }],
        subobjects: vec![],
        vehicle_collision: None,
    };
    interior.validate()?;
    Ok(interior)
}

/// The map's scene: its interior, spawn, sun and sky.
pub fn scene(map: &RoomMap, interior: &str) -> Scene {
    Scene {
        schema_version: 1,
        id: map.id.clone(),
        name: map.name.clone(),
        nodes: vec![
            node("room", Kind::Interior, map.origin, Some(interior), &[]),
            node("spawn", Kind::Spawn, map.spawn_position(), None, &[]),
            node(
                "sun",
                Kind::Sun,
                Vec3::ZERO,
                None,
                &[
                    ("azimuth", map.azimuth.to_string()),
                    ("elevation", map.elevation.to_string()),
                    // Stale, as missions carry it: never read.
                    ("direction", "0.3 0.3 -0.9".into()),
                    ("color", "0.1 0.1 0.12 1".into()),
                    ("ambient", "0.04 0.04 0.05 1".into()),
                ],
            ),
            node(
                "sky",
                Kind::Sky,
                Vec3::ZERO,
                None,
                &[("skysolidcolor", "0.3 0.4 0.6 1".into())],
            ),
        ],
        pending_scripts: vec![],
    }
}

/// Writes `maps` as a native map bundle in `dir` (`bundle.json`, scenes,
/// interiors and the wall texture).
pub fn write_bundle(dir: &Path, maps: &[RoomMap]) -> Result<()> {
    let write = |relative: &str, bytes: &[u8]| -> Result<()> {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().context("bundle file has no folder")?)?;
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))
    };
    write(
        "textures/wall.png",
        &png(8, 8, |x, y| {
            if (x + y) % 2 == 0 {
                [200, 190, 170, 255]
            } else {
                [170, 160, 150, 255]
            }
        })?,
    )?;
    let mut records = vec![];
    let mut assets = BTreeMap::new();
    let mut bindings = vec![];
    let mut lighting = serde_json::Map::new();
    let mut terrains = serde_json::Map::new();
    for map in maps {
        let interior = interior(map)?;
        let interior_file = format!("{}.interior.json", interior.id);
        write(&interior_file, &serde_json::to_vec(&interior)?)?;
        assets.insert(interior.id.clone(), interior_file);
        bindings.push(json!({
            "asset": interior.id,
            "detail": 0,
            "material": 0,
            "texture": "textures/wall.png",
        }));
        let file = format!("maps/{}.scene.json", map.name.to_ascii_lowercase());
        write(&file, &serde_json::to_vec(&scene(map, &interior.id))?)?;
        records.push(json!({ "id": map.id, "file": file }));
        lighting.insert(
            map.id.clone(),
            json!({ "status": "embedded", "interiors": [] }),
        );
        terrains.insert(map.id.clone(), json!([]));
    }
    let bundle = json!({
        "schema_version": 1,
        "maps": records,
        "assets": assets,
        "bindings": bindings,
        "lighting": lighting,
        "terrains": terrains,
    });
    write("bundle.json", &serde_json::to_vec_pretty(&bundle)?)
}
