//! A made-up native map bundle for tests that have no converted maps:
//! lightmapped rooms, each lit by one invented lamp, with spawns, a sun, a
//! sky and (in [`rooms_for`]' rooms, all but the first open to the sky) a
//! potted-plant map model, written in the layout the converter writes (`bundle.json` beside
//! flat scene and interior files and a `textures/` folder), which both the
//! renderer's and the server's map loaders read. Every number here is
//! invented; nothing is read from an original map.
use super::{png, sha256, write_file};
use crate::{
    interior::{Detail, Interior, Lightmap, Surface, Vertex},
    scene::{Kind, Node, Scene},
};
use anyhow::Result;
use bri_console::Clamp;
use glam::{Mat4, Vec3};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

/// Lightmap edge length in texels.
pub const LIGHTMAP_SIZE: u32 = 16;
/// The wall texture every room is drawn with, and its bundle path.
pub const WALL_TEXTURE: (&str, &str) = ("fixture/wall", "textures/wall.png");

/// The map model rooms place as props ([`RoomMap::props`]): a made-up
/// plant (a trunk under a square crown), drawn only (no collision), and
/// its texture and bundle path.
pub const PROP: &str = "fixture/shapes/plant";
pub const PROP_TEXTURE: (&str, &str) = ("fixture/plant", "textures/plant.png");

/// The lamp baked into a room's lightmaps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lamp {
    pub position: [f32; 3],
    pub color: [f32; 3],
    /// Full light within `inner`, none past `outer`.
    pub inner: f32,
    pub outer: f32,
}

impl Lamp {
    /// An invented reach: full colour within `inner` fading linearly to
    /// nothing at `outer`, on faces turned toward the lamp. Renderer tests
    /// that fit lights to the lightmaps bake with their own light model
    /// through [`write_bundle_shaded`].
    pub fn reach(&self, position: Vec3, normal: Vec3) -> Vec3 {
        let delta = Vec3::from(self.position) - position;
        let distance = delta.length();
        let fade =
            ((self.outer - distance) / (self.outer - self.inner).max(1e-3)).clamped(0.0, 1.0);
        let facing = if normal.dot(delta) > 0.0 { 1.0 } else { 0.0 };
        Vec3::from(self.color) * fade * facing
    }
}

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
    /// More spawns, each offset in x and z from `spawn`.
    pub extra_spawns: Vec<Vec3>,
    /// A [`PROP`] standing on the floor at each offset in x and z from
    /// `spawn`.
    pub props: Vec<Vec3>,
    /// No ceiling: the room is open to the sky, so the sun reaches what it
    /// holds (and players' and bricks' sun shadows show inside).
    pub skylight: bool,
    /// The sun's authored angles in degrees; its `direction` field is
    /// written stale on purpose.
    pub azimuth: f32,
    pub elevation: f32,
    /// The sun's colour and ambient light (RGB) on what the room holds
    /// (bricks, players); the room's own faces take theirs from the lightmaps.
    pub sun_color: [f32; 3],
    pub sun_ambient: [f32; 3],
    /// The lamp baked into the lightmaps, in world space as authored.
    pub lamp: Lamp,
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

/// The dim sun of [`rooms`]: (colour, ambient).
pub const DIM_SUN: ([f32; 3], [f32; 3]) = ([0.1, 0.1, 0.12], [0.04, 0.04, 0.05]);
/// The daylight of [`rooms_for`]' rooms, bright enough to play in: (colour,
/// ambient).
pub const DAY_SUN: ([f32; 3], [f32; 3]) = ([0.75, 0.72, 0.65], [0.4, 0.4, 0.45]);

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
            extra_spawns: Vec::new(),
            props: Vec::new(),
            skylight: false,
            azimuth: 210.0,
            elevation: 40.0,
            sun_color: DIM_SUN.0,
            sun_ambient: DIM_SUN.1,
            lamp: Lamp {
                position: [48.0, 36.5, -42.0],
                color: [1.0, 0.9, 0.7],
                inner: 4.0,
                outer: 14.0,
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
            extra_spawns: Vec::new(),
            props: Vec::new(),
            skylight: false,
            azimuth: 95.0,
            elevation: 70.0,
            sun_color: DIM_SUN.0,
            sun_ambient: DIM_SUN.1,
            lamp: Lamp {
                position: [-70.0, 64.5, 60.0],
                color: [0.6, 0.8, 1.0],
                inner: 3.0,
                outer: 12.0,
            },
            ambient: 0.04,
        },
    ]
}

/// One room per id in `ids`, each its own size and place, in daylight
/// ([`DAY_SUN`]), with a [`PROP`] in view of every spawn, a little right
/// of the line straight ahead (it has no collision), and three more spawns
/// a few metres apart so players joining one game do not all start in one
/// spot. The first room has a ceiling; the rest are open to the sky
/// ([`RoomMap::skylight`]), so the sun reaches their floors.
pub fn rooms_for(ids: &[&str]) -> Vec<RoomMap> {
    let template = rooms().remove(0);
    ids.iter()
        .enumerate()
        .map(|(i, id)| {
            let step = i as f32;
            // Floors below y = 0, so a package world built upward from
            // the origin (Stress Lab's strata) stands on top of its room.
            let origin = Vec3::new(step * 3.0, step, -step * 2.0);
            RoomMap {
                id: (*id).into(),
                name: format!("Fixture Room {}", i + 1),
                origin,
                half: 24.0 + step,
                extra_spawns: vec![
                    Vec3::new(5.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 5.0),
                    Vec3::new(5.0, 0.0, 5.0),
                ],
                props: vec![Vec3::new(2.5, 0.0, -12.0)],
                skylight: i > 0,
                lamp: Lamp {
                    position: (origin + Vec3::new(2.0, 1.0, 2.0)).to_array(),
                    ..template.lamp
                },
                sun_color: DAY_SUN.0,
                sun_ambient: DAY_SUN.1,
                ..template.clone()
            }
        })
        .collect()
}

/// A mission colour field: `r g b 1`.
fn rgba([r, g, b]: [f32; 3]) -> String {
    format!("{r} {g} {b} 1")
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

/// The room's interior: six inward faces (five under a skylight), each
/// with its own lightmap of what `shade` says the lamp casts on it (plus
/// the ambient), and the floor as collision.
pub fn interior(map: &RoomMap, shade: &dyn Fn(&Lamp, Vec3, Vec3) -> Vec3) -> Result<Interior> {
    let (lo, hi) = (
        Vec3::new(-map.half, -map.floor, -map.half),
        Vec3::new(map.half, map.height - map.floor, map.half),
    );
    let mut surfaces = vec![];
    let mut lightmaps = vec![];
    let mut floor = vec![];
    for axis in 0..3 {
        for side in [-1.0f32, 1.0] {
            if map.skylight && axis == 1 && side > 0.0 {
                continue;
            }
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
                let c = (shade(&map.lamp, corner(a, b) + map.origin, normal) + map.ambient)
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

/// The map's scene: its interior, spawns, sun and sky.
pub fn scene(map: &RoomMap, interior: &str) -> Scene {
    let mut nodes = vec![
        node("room", Kind::Interior, map.origin, Some(interior), &[]),
        node("spawn", Kind::Spawn, map.spawn_position(), None, &[]),
    ];
    for (i, offset) in map.extra_spawns.iter().enumerate() {
        nodes.push(node(
            &format!("spawn{}", i + 2),
            Kind::Spawn,
            map.spawn_position() + *offset,
            None,
            &[],
        ));
    }
    for (i, offset) in map.props.iter().enumerate() {
        let spawn = map.spawn_position();
        nodes.push(node(
            &format!("prop{}", i + 1),
            Kind::StaticModel,
            Vec3::new(spawn.x + offset.x, map.floor_height(), spawn.z + offset.z),
            Some(PROP),
            &[],
        ));
    }
    nodes.extend([
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
                ("color", rgba(map.sun_color)),
                ("ambient", rgba(map.sun_ambient)),
            ],
        ),
        node(
            "sky",
            Kind::Sky,
            Vec3::ZERO,
            None,
            &[("skysolidcolor", "0.3 0.4 0.6 1".into())],
        ),
    ]);
    Scene {
        schema_version: 1,
        id: map.id.clone(),
        name: map.name.clone(),
        nodes,
        pending_scripts: vec![],
    }
}

/// The [`PROP`] model: a trunk 1.2 tall under a crown 1.2 across, in one
/// textured material.
pub fn prop() -> crate::shape::Shape {
    use super::{material, plain, rigid_shape};
    rigid_shape(
        PROP,
        &[("root", None, [0.0; 3])],
        &[
            (0, [0.0, 0.6, 0.0], [0.1, 0.6, 0.1], plain(0)),
            (0, [0.0, 1.4, 0.0], [0.6, 0.2, 0.6], plain(0)),
        ],
        vec![material("fixture_plant", "opaque")],
    )
}

/// The sky over a [`RoomMap::skylight`]: six faces of one made-up blue
/// gradient (so every pixel past the walls is sky), no clouds, and fog
/// only far past the room.
fn write_sky(dir: &Path) -> Result<crate::environment::Environment> {
    const FACE: &str = "sky_face.png";
    let bytes = png(16, 16, |_, y| {
        let t = y as f32 / 15.0;
        [
            (110.0 + 60.0 * t) as u8,
            (150.0 + 50.0 * t) as u8,
            (220.0 + 20.0 * t) as u8,
            255,
        ]
    })?;
    let sha = write_file(dir, FACE, &bytes)?;
    let face = crate::environment::Image {
        file: FACE.into(),
        source: "fixture/sky/face.png".into(),
        sha256: sha,
        width: 16,
        height: 16,
    };
    let sky = crate::environment::Environment {
        schema_version: 2,
        source_materials: "fixture/sky/sky.dml".into(),
        source_sha256: sha256(b"fixture sky"),
        faces: vec![face; 6],
        reflection: None,
        clouds: vec![],
        textures: true,
        bottom: true,
        horizon_band: false,
        solid_color: [0.45, 0.6, 0.85],
        fog: crate::environment::Fog {
            // Past anything a room holds.
            start: 500.0,
            end: 1000.0,
            color: [0.45, 0.6, 0.85],
        },
        warnings: vec![],
    };
    sky.validate()?;
    Ok(sky)
}

/// A file name for `id` with no folders in it, as the bundle keeps them.
fn flat(id: &str, suffix: &str) -> String {
    format!(
        "{}{suffix}",
        id.to_ascii_lowercase().replace(
            |c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-',
            "_"
        )
    )
}

/// Writes `maps` as a native map bundle in `dir`, each lamp baked with its
/// own [`Lamp::reach`].
pub fn write_bundle(dir: &Path, maps: &[RoomMap]) -> Result<()> {
    write_bundle_shaded(dir, maps, &Lamp::reach)
}

/// Writes `maps` as a native map bundle in `dir` (`bundle.json` naming each
/// map, its flat scene and interior files, and the wall texture), each
/// lamp baked into the lightmaps by `shade`.
pub fn write_bundle_shaded(
    dir: &Path,
    maps: &[RoomMap],
    shade: &dyn Fn(&Lamp, Vec3, Vec3) -> Vec3,
) -> Result<()> {
    let wall = png(8, 8, |x, y| {
        if (x + y) % 2 == 0 {
            [200, 190, 170, 255]
        } else {
            [170, 160, 150, 255]
        }
    })?;
    write_file(dir, WALL_TEXTURE.1, &wall)?;
    let mut records = vec![];
    let mut assets = BTreeMap::new();
    let mut bindings = vec![];
    let mut textures = serde_json::Map::new();
    textures.insert(WALL_TEXTURE.0.into(), WALL_TEXTURE.1.into());
    if maps.iter().any(|m| !m.props.is_empty()) {
        let leaves = png(8, 8, |x, y| {
            if (x * 3 + y) % 4 == 0 {
                [60, 140, 50, 255]
            } else {
                [90, 170, 70, 255]
            }
        })?;
        write_file(dir, PROP_TEXTURE.1, &leaves)?;
        textures.insert(PROP_TEXTURE.0.into(), PROP_TEXTURE.1.into());
        let shape = prop();
        shape.validate()?;
        let file = flat(PROP, ".shape.json");
        write_file(dir, &file, &serde_json::to_vec(&shape)?)?;
        assets.insert(PROP.to_string(), file);
        bindings.push(json!({
            "asset": PROP,
            "shape_material": 0,
            "skin": "",
            "texture": PROP_TEXTURE.1,
        }));
    }
    let mut lighting = serde_json::Map::new();
    let mut terrains = serde_json::Map::new();
    let mut environments = serde_json::Map::new();
    let sky = if maps.iter().any(|m| m.skylight) {
        Some(write_sky(dir)?)
    } else {
        None
    };
    for map in maps {
        let interior = interior(map, shade)?;
        let interior_file = flat(&interior.id, ".interior.json");
        write_file(dir, &interior_file, &serde_json::to_vec(&interior)?)?;
        assets.insert(interior.id.clone(), interior_file);
        bindings.push(json!({
            "asset": interior.id,
            "detail": 0,
            "material": 0,
            "texture": WALL_TEXTURE.1,
        }));
        let file = flat(&map.id, ".scene.json");
        write_file(dir, &file, &serde_json::to_vec(&scene(map, &interior.id))?)?;
        records.push(json!({ "id": map.id, "name": map.name, "file": file }));
        lighting.insert(
            map.id.clone(),
            json!({ "status": "embedded", "interiors": [] }),
        );
        terrains.insert(map.id.clone(), json!([]));
        if let (true, Some(sky)) = (map.skylight, &sky) {
            environments.insert(map.id.clone(), serde_json::to_value(sky)?);
        }
    }
    let bundle = json!({
        "schema_version": 1,
        "maps": records,
        "assets": assets,
        "textures": textures,
        "bindings": bindings,
        "lighting": lighting,
        "terrains": terrains,
        "environments": environments,
    });
    write_file(dir, "bundle.json", &serde_json::to_vec_pretty(&bundle)?)?;
    Ok(())
}
