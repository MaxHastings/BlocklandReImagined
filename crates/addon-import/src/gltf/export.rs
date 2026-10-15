//! A converted map (a native map bundle's map) as a glTF scene for Blender,
//! laid out the way `--kind map` reads it back (docs/modding/maps.md):
//! every interior and placed model as meshes with their textures, each
//! one's collision as a `col_` mesh, spawn points as `spawn` empties, and
//! the map's lights (recovered from the original lightmaps, and any the map
//! authors) as `KHR_lights_punctual` lights with `bri_` custom properties.
//! The scene is the map as the game places it (after the floor lift), so
//! an exported, edited and reimported map lines up with the original.
use super::write::Gltf;
use anyhow::{Context, Result, ensure};
use bri_content::interior::Interior;
use bri_content::scene::{Kind, Node, Scene};
use bri_content::shape::Shape;
use glam::{Mat4, Vec3};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// What an export wrote and what it left out.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExportReport {
    pub map: String,
    pub name: String,
    pub meshes: usize,
    pub collision_meshes: usize,
    pub triangles: usize,
    pub textures: usize,
    pub spawns: usize,
    pub lights: usize,
    pub warnings: Vec<String>,
}

impl ExportReport {
    pub fn markdown(&self) -> String {
        let mut s = format!(
            "# {} ({}) for Blender\n\n{} meshes ({} triangles), {} collision meshes, {} textures, {} spawn points, {} lights.\n",
            self.name,
            self.map,
            self.meshes,
            self.triangles,
            self.collision_meshes,
            self.textures,
            self.spawns,
            self.lights
        );
        if !self.warnings.is_empty() {
            s.push_str("\n## Left out or changed\n\n");
            for w in &self.warnings {
                s.push_str(&format!("- {w}\n"));
            }
        }
        s
    }
}

/// Native (x right, y up, -z forward) to glTF (+z forward): the half turn
/// about Y that `--kind map` undoes on import.
fn to_gltf(v: Vec3) -> [f32; 3] {
    [-v.x, v.y, -v.z]
}

fn read(root: &Path, name: &str) -> Result<Vec<u8>> {
    let path = Path::new(name);
    ensure!(
        !name.is_empty()
            && path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "bundle file {name} is outside the bundle"
    );
    std::fs::read(root.join(name)).with_context(|| format!("reading {name}"))
}

/// One mesh being gathered: its triangles grouped by material.
#[derive(Default)]
struct MeshBuilder {
    groups: BTreeMap<Option<usize>, Prim>,
}
#[derive(Default)]
struct Prim {
    positions: Vec<f32>,
    normals: Vec<f32>,
    uvs: Vec<f32>,
    indices: Vec<u32>,
}
impl MeshBuilder {
    fn triangle(&mut self, material: Option<usize>, corners: [(Vec3, Vec3, [f32; 2]); 3]) {
        let p = self.groups.entry(material).or_default();
        for (position, normal, uv) in corners {
            p.indices.push((p.positions.len() / 3) as u32);
            p.positions.extend(to_gltf(position));
            p.normals.extend(to_gltf(normal.normalize_or_zero()));
            p.uvs.extend(uv);
        }
    }
    fn triangles(&self) -> usize {
        self.groups.values().map(|p| p.indices.len() / 3).sum()
    }
    /// Adds the mesh and a node holding it; `None` if it is empty.
    fn finish(self, g: &mut Gltf, name: &str) -> Option<usize> {
        let mut primitives = Vec::new();
        for (material, p) in self.groups {
            if p.indices.is_empty() {
                continue;
            }
            let position = g.floats(&p.positions, 3);
            let normal = g.floats(&p.normals, 3);
            let uv = g.floats(&p.uvs, 2);
            let indices = g.indices(&p.indices);
            let mut prim = json!({ "attributes": { "POSITION": position, "NORMAL": normal, "TEXCOORD_0": uv }, "indices": indices });
            if let Some(m) = material {
                prim["material"] = json!(m);
            }
            primitives.push(prim);
        }
        if primitives.is_empty() {
            return None;
        }
        let mesh = g.mesh(name, primitives);
        Some(g.node(json!({ "name": name, "mesh": mesh }), None))
    }
}

/// Texture files to glTF materials, preserving distinct authored UV motion.
struct Materials {
    by_file: BTreeMap<String, usize>,
}
impl Materials {
    fn get(
        &mut self,
        g: &mut Gltf,
        root: &Path,
        file: &str,
        motion: Option<bri_content::shape::UvMotion>,
        report: &mut ExportReport,
    ) -> Result<usize> {
        let key = match motion {
            Some(uv) => format!("{file}:uv={}", serde_json::to_string(&uv)?),
            None => file.to_owned(),
        };
        if let Some(&m) = self.by_file.get(&key) {
            return Ok(m);
        }
        let bytes = read(root, file)?;
        let (bytes, mime) = if bytes.starts_with(b"\x89PNG") {
            (bytes, "image/png")
        } else if bytes.starts_with(&[0xff, 0xd8]) {
            (bytes, "image/jpeg")
        } else {
            let image = image::load_from_memory(&bytes)
                .with_context(|| format!("texture {file} is not an image the exporter reads"))?;
            let mut png = Vec::new();
            image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
            (png, "image/png")
        };
        let texture = g.texture(&bytes, mime);
        let name = Path::new(file)
            .file_stem()
            .map_or_else(|| file.to_owned(), |s| s.to_string_lossy().into_owned());
        let m = g.material(&name, [1.0; 4], None, json!({}));
        g.json["materials"][m]["pbrMetallicRoughness"]["baseColorTexture"] =
            json!({ "index": texture });
        // Torque surfaces are matte.
        g.json["materials"][m]["pbrMetallicRoughness"]["metallicFactor"] = json!(0.0);
        g.json["materials"][m]["pbrMetallicRoughness"]["roughnessFactor"] = json!(1.0);
        if let Some(motion) = motion {
            g.json["materials"][m]["extras"] = json!({"bri_uv_motion":motion});
        }
        self.by_file.insert(key, m);
        report.textures += 1;
        Ok(m)
    }
}

fn placement(node: &Node) -> Mat4 {
    Mat4::from_cols_array(&node.transform)
}

/// A unique node name from `base`.
fn unique(used: &mut BTreeMap<String, usize>, base: &str) -> String {
    let n = used.entry(base.to_owned()).or_default();
    *n += 1;
    if *n == 1 {
        base.to_owned()
    } else {
        format!("{base}.{:03}", *n - 1)
    }
}

/// The short name of an asset id or file: `bedroom` for
/// `v20/add-ons/map_bedroom/bedroom.dif`.
fn short(id: &str) -> String {
    let file = id.rsplit(['/', ':']).next().unwrap_or(id);
    file.split('.').next().unwrap_or(file).to_ascii_lowercase()
}

/// The map in the bundle at `root` whose id, name or short name is `wanted`.
pub fn find_map(root: &Path, wanted: &str) -> Result<(String, String, String)> {
    let bundle: Value = serde_json::from_slice(&read(root, "bundle.json")?)?;
    let maps = bundle["maps"]
        .as_array()
        .context("bundle.json lists no maps")?;
    let lower = wanted.to_ascii_lowercase();
    let found = maps.iter().find(|m| {
        let id = m["id"].as_str().unwrap_or_default();
        id == wanted
            || short(id) == lower
            || m["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(wanted))
    });
    let m = found.with_context(|| {
        let names: Vec<_> = maps.iter().filter_map(|m| m["name"].as_str()).collect();
        format!(
            "no map `{wanted}` in this bundle; it has {}",
            names.join(", ")
        )
    })?;
    Ok((
        m["id"].as_str().context("map without an id")?.to_owned(),
        m["name"].as_str().unwrap_or(wanted).to_owned(),
        m["file"]
            .as_str()
            .context("map without a scene file")?
            .to_owned(),
    ))
}

/// The map `wanted` in the bundle at `root` as a `.glb`.
pub fn export_map(root: &Path, wanted: &str) -> Result<(Vec<u8>, ExportReport)> {
    let bundle: Value = serde_json::from_slice(&read(root, "bundle.json")?)?;
    let (map_id, name, scene_file) = find_map(root, wanted)?;
    let mut scene: Scene = serde_json::from_slice(&read(root, &scene_file)?)?;
    ensure!(scene.id == map_id, "scene {scene_file} is not map {map_id}");
    let mut report = ExportReport {
        map: map_id.clone(),
        name: name.clone(),
        ..Default::default()
    };
    let asset_file = |id: &str| -> Result<String> {
        Ok(bundle["assets"][id]
            .as_str()
            .with_context(|| format!("asset {id} is not in bundle.json"))?
            .to_owned())
    };
    let mut interiors = BTreeMap::new();
    for node in scene
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::Interior))
    {
        let id = node.asset.as_deref().context("interior without an asset")?;
        if !interiors.contains_key(id) {
            let interior: Interior = serde_json::from_slice(&read(root, &asset_file(id)?)?)?;
            interiors.insert(id.to_owned(), interior);
        }
    }
    // Where the game places it, so lights recovered in that frame line up.
    let lift = scene.floor_lift(|id| interiors.get(id));
    scene.lift(lift);
    let bindings = bundle["bindings"].as_array().cloned().unwrap_or_default();
    let mut g = Gltf::new();
    let mut materials = Materials {
        by_file: BTreeMap::new(),
    };
    let mut used = BTreeMap::new();

    for node in &scene.nodes {
        let at = placement(node);
        let normals = at.inverse().transpose();
        let mirrored = at.determinant() < 0.0;
        let order = |c: [(Vec3, Vec3, [f32; 2]); 3]| if mirrored { [c[0], c[2], c[1]] } else { c };
        match node.kind {
            Kind::Interior => {
                let id = node.asset.as_deref().unwrap_or_default();
                let interior = &interiors[id];
                let detail = interior.details.first().context("interior without detail")?;
                let mut by_material = BTreeMap::new();
                for b in bindings.iter().filter(|b| b["asset"].as_str() == Some(id) && b["detail"].as_u64() == Some(0)) {
                    if let (Some(m), Some(t)) = (b["material"].as_u64(), b["texture"].as_str()) {
                        by_material.insert(m as usize, materials.get(&mut g, root, t, None, &mut report)?);
                    }
                }
                let mut mesh = MeshBuilder::default();
                for surface in &detail.surfaces {
                    let material = by_material.get(&surface.material).copied();
                    for t in &surface.triangles {
                        let corner = |i: u32| {
                            let v = &surface.vertices[i as usize];
                            (
                                at.transform_point3(Vec3::from(v.position)),
                                normals.transform_vector3(Vec3::from(v.normal)),
                                v.uv,
                            )
                        };
                        mesh.triangle(material, order([corner(t[0]), corner(t[1]), corner(t[2])]));
                    }
                }
                let base = unique(&mut used, &short(id));
                report.triangles += mesh.triangles();
                if mesh.finish(&mut g, &base).is_some() {
                    report.meshes += 1;
                }
                let mut col = MeshBuilder::default();
                for t in &detail.collision_triangles {
                    let c = t.map(|p| at.transform_point3(Vec3::from(p)));
                    let n = (c[1] - c[0]).cross(c[2] - c[0]);
                    col.triangle(None, order(c.map(|p| (p, n, [0.0, 0.0]))));
                }
                if col.finish(&mut g, &format!("col_{base}")).is_some() {
                    report.collision_meshes += 1;
                }
            }
            Kind::StaticModel | Kind::DatablockModel if node.asset.is_some() => {
                let id = node.asset.as_deref().unwrap_or_default();
                let shape: Shape = match asset_file(id).and_then(|f| read(root, &f)).and_then(|b| Ok(serde_json::from_slice(&b)?)) {
                    Ok(shape) => shape,
                    Err(e) => {
                        report.warnings.push(format!("model `{}` left out: {e:#}", node.name));
                        continue;
                    }
                };
                let mut by_material = BTreeMap::new();
                for b in bindings.iter().filter(|b| b["asset"].as_str() == Some(id) && b["skin"].as_str().unwrap_or_default().is_empty()) {
                    if let (Some(m), Some(t)) = (b["shape_material"].as_u64(), b["texture"].as_str()) {
                        by_material.insert(m as usize, materials.get(&mut g, root, t, shape.materials.get(m as usize).and_then(|source|source.uv_motion), &mut report)?);
                    }
                }
                let pose = bri_content::animation::sample(&shape, None, 0.0)?;
                let base = unique(&mut used, &short(if node.name.is_empty() { id } else { &node.name }));
                if let Some(detail) = shape.details.iter().position(|d| !d.collision) {
                    let mut mesh = MeshBuilder::default();
                    for t in bri_content::animation::triangles(&shape, &pose, detail, |_| true)? {
                        let material = t.material.and_then(|m| by_material.get(&m).copied());
                        mesh.triangle(material, order(t.vertices.map(|v| (at.transform_point3(v.position), normals.transform_vector3(v.normal), v.uv))));
                    }
                    report.triangles += mesh.triangles();
                    if mesh.finish(&mut g, &base).is_some() {
                        report.meshes += 1;
                    }
                }
                let mut col = MeshBuilder::default();
                for detail in shape.details.iter().enumerate().filter(|(_, d)| d.collision).map(|(i, _)| i) {
                    for t in bri_content::animation::triangles(&shape, &pose, detail, |_| true)? {
                        col.triangle(None, order(t.vertices.map(|v| (at.transform_point3(v.position), normals.transform_vector3(v.normal), [0.0, 0.0]))));
                    }
                }
                if col.finish(&mut g, &format!("col_{base}")).is_some() {
                    report.collision_meshes += 1;
                }
            }
            Kind::Spawn => {
                let (_, rotation, position) = at.to_scale_rotation_translation();
                // The half turn about Y, applied to the rotation too.
                let turn = glam::Quat::from_rotation_y(std::f32::consts::PI);
                let r = turn * rotation * turn.inverse();
                let name = unique(&mut used, "spawn");
                g.node(json!({ "name": name, "translation": to_gltf(position), "rotation": r.normalize().to_array() }), None);
                report.spawns += 1;
            }
            Kind::Terrain => report.warnings.push(format!(
                "terrain `{}` is left out: model the ground in Blender, or use a heightmap (docs/modding/maps.md)",
                node.name
            )),
            Kind::Water => report.warnings.push(format!(
                "water `{}` is left out: add a plane named `water` in Blender",
                node.name
            )),
            _ => {}
        }
    }

    // Lights: those recovered from the original lightmaps, then the map's own.
    let mut lights: Vec<(String, Value, Value)> = Vec::new();
    let params = root.join("lighting-parameters.json");
    if params.is_file() {
        let p: Value = serde_json::from_slice(&std::fs::read(&params)?)?;
        let fresh = p["bundle_sha256"].as_str()
            == Some(&format!(
                "{:x}",
                <sha2::Sha256 as sha2::Digest>::digest(read(root, "bundle.json")?)
            ));
        if !fresh {
            report.warnings.push("lighting-parameters.json was made for another version of this bundle; its lights may be misplaced (run prepare_lighting again)".into());
        }
        for l in p["maps"][&map_id].as_array().into_iter().flatten() {
            let v = |k: &str| -> Option<Vec<f32>> {
                l[k].as_array().map(|a| {
                    a.iter()
                        .filter_map(Value::as_f64)
                        .map(|x| x as f32)
                        .collect()
                })
            };
            let (Some(pos), Some(col)) = (v("position"), v("color")) else {
                continue;
            };
            if pos.len() != 3 || col.len() != 3 {
                continue;
            }
            let extras = json!({
                "bri_light_type": "omni",
                "bri_radius": l["outer"].as_f64().unwrap_or(10.0),
                "bri_inner": l["inner"].as_f64().unwrap_or(0.0),
            });
            lights.push((
                "lamp".into(),
                json!({ "position": pos, "color": col }),
                extras,
            ));
        }
    } else if scene.nodes.iter().any(|n| matches!(n.kind, Kind::Interior)) {
        report.warnings.push("no lighting-parameters.json beside bundle.json: the map's original lamps are not exported (run prepare_lighting on the bundle first)".into());
    }
    for light in scene.lights()? {
        let node = &scene.nodes[light.node];
        let mut extras = serde_json::Map::new();
        extras.insert("bri_light_type".into(), json!(light.kind));
        extras.insert("bri_radius".into(), json!(light.outer));
        extras.insert("bri_inner".into(), json!(light.inner));
        for (k, v) in &node.properties {
            if ![
                "type",
                "color",
                "brightness",
                "inner",
                "outer",
                "inner_cone",
                "outer_cone",
            ]
            .contains(&k.as_str())
            {
                extras.insert(format!("bri_{k}"), json!(v));
            }
        }
        lights.push((
            if node.name.is_empty() {
                "lamp".into()
            } else {
                node.name.clone()
            },
            json!({ "position": light.position, "color": light.color }),
            Value::Object(extras),
        ));
    }
    let mut defined = Vec::new();
    for (name, light, mut extras) in lights {
        let pos: Vec<f32> = serde_json::from_value(light["position"].clone())?;
        let col: Vec<f32> = serde_json::from_value(light["color"].clone())?;
        // glTF colours stop at 1; anything brighter is brightness.
        let peak = col.iter().copied().fold(0.0f32, f32::max);
        let (color, brightness) = if peak > 1.0 {
            (col.iter().map(|c| c / peak).collect(), peak)
        } else {
            (col.clone(), 1.0)
        };
        if brightness != 1.0 {
            extras["bri_brightness"] = json!(brightness);
        }
        let index = defined.len();
        defined.push(
            json!({ "name": name, "type": "point", "color": color, "range": extras["bri_radius"] }),
        );
        let node_name = unique(&mut used, &name);
        g.node(
            json!({ "name": node_name, "translation": to_gltf(Vec3::from_slice(&pos)),
                "extensions": { "KHR_lights_punctual": { "light": index } }, "extras": extras }),
            None,
        );
        report.lights += 1;
    }
    if !defined.is_empty() {
        g.json["extensionsUsed"] = json!(["KHR_lights_punctual"]);
        g.json["extensions"] = json!({ "KHR_lights_punctual": { "lights": defined } });
    }
    ensure!(report.meshes > 0, "map {map_id} has nothing to export");
    g.json["scenes"][0]["name"] = json!(name);
    Ok((g.glb(), report))
}
