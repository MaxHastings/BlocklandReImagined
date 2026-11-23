//! Compiled model GLBs keep engine semantics in a versioned extras table.
//! Geometry and topology are read from glTF accessors. Detail membership,
//! object state and Blockland animation policy are explicit engine data.
use crate::{Document, Gltf};
use anyhow::{Context, Result, ensure};
use bri_content::shape::{Animation, Detail, Material, Mesh, Node, Object, Primitive, Shape, Skin};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const VERSION: u32 = 1;
const EXTRA: &str = "bri_model";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Model {
    version: u32,
    id: String,
    nodes: Vec<Node>,
    objects: Vec<Object>,
    details: Vec<Detail>,
    meshes: Vec<Option<Geometry>>,
    materials: Vec<Material>,
    animations: Vec<Animation>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Geometry {
    frame_vertices: usize,
    attributes: Attributes,
    frames: Attributes,
    primitives: Vec<Part>,
    skin: Option<Skin>,
    vertex_rig: Option<bri_content::vertex_lighting::Rig>,
    billboard: bool,
    billboard_y: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attributes {
    position: Option<usize>,
    normal: Option<usize>,
    uv: Option<usize>,
    uv1: Option<usize>,
    color: Option<usize>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    material: Option<usize>,
    zone: Option<u16>,
    indices: Option<usize>,
}

fn point([x, y, z]: [f32; 3]) -> [f32; 3] {
    [-x, y, -z]
}

fn float_rows<const N: usize>(doc: &Document, index: Option<usize>) -> Result<Vec<[f32; N]>> {
    let Some(index) = index else {
        return Ok(Vec::new());
    };
    doc.packed_rows(index, 5126, N, 4)?
        .map(|row| {
            let mut values = [0.0; N];
            for (value, bytes) in values.iter_mut().zip(row.chunks_exact(4)) {
                *value = f32::from_le_bytes(bytes.try_into().expect("four-byte float"));
                ensure!(value.is_finite(), "Non-finite compiled model attribute");
            }
            Ok(values)
        })
        .collect()
}

fn indices(doc: &Document, index: Option<usize>) -> Result<Vec<[u32; 3]>> {
    let Some(index) = index else {
        return Ok(Vec::new());
    };
    let kind = doc.json["accessors"][index]["componentType"].as_u64();
    let (kind, size) = match kind {
        Some(5123) => (5123, 2),
        Some(5125) => (5125, 4),
        _ => anyhow::bail!("Compiled model has an invalid index accessor"),
    };
    let rows = doc.packed_rows(index, kind, 1, size)?;
    let values: Vec<u32> = rows
        .map(|bytes| {
            let mut word = [0; 4];
            word[..size].copy_from_slice(bytes);
            u32::from_le_bytes(word)
        })
        .collect();
    ensure!(values.len().is_multiple_of(3), "Incomplete model triangle");
    Ok(values.chunks_exact(3).map(|v| [v[0], v[1], v[2]]).collect())
}

impl Geometry {
    fn read(self, doc: &Document) -> Result<Mesh> {
        let Attributes {
            position,
            normal,
            uv,
            uv1,
            color,
        } = self.attributes;
        let mut positions = float_rows(doc, position)?;
        positions.extend(float_rows(doc, self.frames.position)?);
        let mut normals = float_rows(doc, normal)?;
        normals.extend(float_rows(doc, self.frames.normal)?);
        let mut uvs = float_rows(doc, uv)?;
        uvs.extend(float_rows(doc, self.frames.uv)?);
        let mut second_uv = float_rows(doc, uv1)?;
        second_uv.extend(float_rows(doc, self.frames.uv1)?);
        let mut colors = Vec::new();
        for index in [color, self.frames.color].into_iter().flatten() {
            colors.extend(
                doc.packed_rows(index, 5121, 4, 1)?
                    .map(|v| [v[0], v[1], v[2], v[3]]),
            );
        }
        let mut primitives = Vec::with_capacity(self.primitives.len());
        for part in self.primitives {
            primitives.push(Primitive {
                material: part.material,
                zone: part.zone,
                triangles: indices(doc, part.indices)?,
            });
        }
        Ok(Mesh {
            frame_vertices: self.frame_vertices,
            positions: positions.into_iter().map(point).collect(),
            normals: normals.into_iter().map(point).collect(),
            uv: uvs,
            uv1: second_uv,
            colors,
            vertex_rig: self.vertex_rig,
            primitives,
            skin: self.skin,
            billboard: self.billboard,
            billboard_y: self.billboard_y,
        })
    }
}

fn write_rows<const N: usize>(gltf: &mut Gltf, rows: &[[f32; N]]) -> Option<usize> {
    (!rows.is_empty()).then(|| gltf.floats(rows.as_flattened(), N))
}

fn write_split<const N: usize>(
    gltf: &mut Gltf,
    rows: &[[f32; N]],
    frame: usize,
) -> (Option<usize>, Option<usize>) {
    let (primary, rest) = rows.split_at(frame.min(rows.len()));
    (write_rows(gltf, primary), write_rows(gltf, rest))
}

fn geometry(gltf: &mut Gltf, mesh: &Mesh, index: usize) -> (Geometry, Option<usize>) {
    let frame = mesh.frame_vertices;
    let positions: Vec<_> = mesh.positions.iter().copied().map(point).collect();
    let normals: Vec<_> = mesh.normals.iter().copied().map(point).collect();
    let (position, more_positions) = write_split(gltf, &positions, frame);
    let (normal, more_normals) = write_split(gltf, &normals, frame);
    let (uv, more_uv) = write_split(gltf, &mesh.uv, frame);
    let (uv1, more_uv1) = write_split(gltf, &mesh.uv1, frame);
    let (color_rows, extra_colors) = mesh.colors.split_at(frame.min(mesh.colors.len()));
    let color = (!color_rows.is_empty()).then(|| gltf.color_bytes(color_rows));
    let more_colors = (!extra_colors.is_empty()).then(|| gltf.color_bytes(extra_colors));
    let attributes: BTreeMap<_, _> = [
        ("POSITION", position),
        ("NORMAL", normal),
        ("TEXCOORD_0", uv),
        ("TEXCOORD_1", uv1),
        ("COLOR_0", color),
    ]
    .into_iter()
    .filter_map(|(key, i)| i.map(|i| (key, i)))
    .collect();
    let mut draw = Vec::new();
    let mut primitives = Vec::new();
    for primitive in &mesh.primitives {
        let indices = (!primitive.triangles.is_empty())
            .then(|| gltf.indices(primitive.triangles.as_flattened()));
        if let Some(i) = indices {
            let mut part = json!({ "attributes": attributes, "indices": i });
            if let Some(m) = primitive.material {
                part["material"] = json!(m);
            }
            draw.push(part);
        }
        primitives.push(Part {
            material: primitive.material,
            zone: primitive.zone,
            indices,
        });
    }
    let scene_mesh = (!draw.is_empty()).then(|| gltf.mesh(&format!("mesh{index}"), draw));
    (
        Geometry {
            frame_vertices: frame,
            attributes: Attributes {
                position,
                normal,
                uv,
                uv1,
                color,
            },
            frames: Attributes {
                position: more_positions,
                normal: more_normals,
                uv: more_uv,
                uv1: more_uv1,
                color: more_colors,
            },
            primitives,
            skin: mesh.skin.clone(),
            vertex_rig: mesh.vertex_rig.clone(),
            billboard: mesh.billboard,
            billboard_y: mesh.billboard_y,
        },
        scene_mesh,
    )
}

/// Compile a native shape into a self-contained GLB. The highest visible
/// detail is the default scene. All detail and collision mesh slots are kept.
pub fn encode_model(shape: &Shape) -> Result<Vec<u8>> {
    shape.validate()?;
    let mut gltf = Gltf::new();
    for material in &shape.materials {
        gltf.material(&material.name, [1.0; 4], None, Value::Null);
    }
    let (meshes, draw): (Vec<_>, Vec<_>) = shape
        .meshes
        .iter()
        .enumerate()
        .map(|(i, mesh)| match mesh {
            Some(mesh) => {
                let (geometry, draw) = geometry(&mut gltf, mesh, i);
                (Some(geometry), draw)
            }
            None => (None, None),
        })
        .unzip();
    let pose = bri_content::animation::sample(shape, None, 0.0)?;
    if let Some(detail) = shape
        .details
        .iter()
        .filter(|d| !d.collision)
        .max_by(|a, b| a.pixel_threshold.total_cmp(&b.pixel_threshold))
    {
        for object in &shape.objects[detail.object_start..detail.object_start + detail.object_count]
        {
            let Some(mesh) = object.meshes.get(detail.mesh_offset).and_then(|&i| draw[i]) else {
                continue;
            };
            let transform = object.node.map_or(glam::Mat4::IDENTITY, |n| pose.nodes[n]);
            let axes = glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, -1.0));
            gltf.node(json!({ "name": object.name, "mesh": mesh, "matrix": (axes * transform * axes).to_cols_array() }), None);
        }
    }
    gltf.json["asset"]["extras"][EXTRA] = serde_json::to_value(Model {
        version: VERSION,
        id: shape.id.clone(),
        nodes: shape.nodes.clone(),
        objects: shape.objects.clone(),
        details: shape.details.clone(),
        meshes,
        materials: shape.materials.clone(),
        animations: shape.animations.clone(),
    })?;
    Ok(gltf.glb())
}

/// Whether a package path declares a model instead of a content ID.
pub fn is_model_file(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.ends_with(".glb") || path.ends_with(".shape.json")
}

/// Read a compiled model. JSON is also used by offline converters and
/// synthetic model producers, so they share the same validation here.
pub fn read_model(bytes: &[u8]) -> Result<Shape> {
    let shape: Shape = if bytes.starts_with(b"glTF") {
        let doc = Document::from_bytes(bytes, None)?;
        let declaration = &doc.json["asset"]["extras"][EXTRA];
        ensure!(
            declaration["version"].as_u64() == Some(u64::from(VERSION)),
            "Unsupported compiled GLB model version; import the source with bri-import-addon"
        );
        let model: Model = Model::deserialize(declaration).context("Invalid compiled GLB model")?;
        let mut used = std::collections::BTreeSet::new();
        for mesh in model.meshes.iter().flatten() {
            for attributes in [&mesh.attributes, &mesh.frames] {
                for accessor in [
                    attributes.position,
                    attributes.normal,
                    attributes.uv,
                    attributes.uv1,
                    attributes.color,
                ]
                .into_iter()
                .flatten()
                {
                    ensure!(
                        used.insert(accessor),
                        "Compiled model reuses an attribute accessor"
                    );
                }
            }
            for accessor in mesh.primitives.iter().filter_map(|p| p.indices) {
                ensure!(
                    used.insert(accessor),
                    "Compiled model reuses an index accessor"
                );
            }
        }
        doc.check_disjoint_packed_views(used)?;
        let meshes = model
            .meshes
            .into_iter()
            .map(|mesh| mesh.map(|m| m.read(&doc)).transpose())
            .collect::<Result<_>>()?;
        Shape {
            schema_version: 1,
            id: model.id,
            nodes: model.nodes,
            objects: model.objects,
            details: model.details,
            meshes,
            materials: model.materials,
            animations: model.animations,
        }
    } else {
        bri_content::strict::from_slice(bytes)?
    };
    shape.validate()?;
    Ok(shape)
}
