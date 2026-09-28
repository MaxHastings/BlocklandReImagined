//! Rebuilding a posed avatar's vertices each frame without rebuilding its
//! mesh. `SceneData::append_shape` resolves visibility, materials, colours
//! and triangle order and allocates as it goes; for a body whose parts,
//! frames and paint have not changed since the last frame, only positions,
//! normals and batch centres differ. `Layout` records that structure once
//! and `Layout::pose` rewrites just those fields, with the same arithmetic
//! `append_shape` and `animation::triangles` use.
use anyhow::{Context, Result, ensure};
use bri_content::{animation::Pose, shape::Shape};
use bri_render::scene::{MeshBatch, SceneData, SceneVertex};
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;

/// What fixes a posed shape's structure: each drawn object with its mesh
/// frame, material frame and paint. The same key means the same vertices,
/// indices and batches in the same order.
#[derive(Clone, Debug, PartialEq)]
struct Key(Vec<(usize, usize, usize, [u32; 4])>);

/// One drawn object: where its mesh's vertices come from this frame.
struct Part {
    object: usize,
    mesh: usize,
    /// First vertex of the mesh frame, in `Mesh::positions`.
    offset: usize,
}

pub struct Layout {
    key: Key,
    parts: Vec<Part>,
    /// Per drawn vertex, in output order: its part and mesh vertex.
    sources: Vec<(u32, u32)>,
    /// Per part: this frame's posed positions and normals, by mesh vertex.
    positions: Vec<Vec<Vec3>>,
    normals: Vec<Vec<Vec3>>,
}

/// The inputs `append_shape` takes, minus the pose.
pub struct Binding<'a> {
    pub shape: &'a Shape,
    pub detail: usize,
    pub materials: &'a [usize],
    pub translucent_materials: &'a [usize],
    pub unassigned_material: usize,
    /// Per object: its paint, or None to hide it.
    pub colors: &'a [Option<[f32; 4]>],
}

fn key(binding: &Binding<'_>, pose: &Pose) -> Result<Key> {
    let shape = binding.shape;
    let detail = shape
        .details
        .get(binding.detail)
        .context("Missing detail")?;
    let mut parts = Vec::new();
    for i in detail.object_start..detail.object_start + detail.object_count {
        let Some(color) = binding.colors[i] else {
            continue;
        };
        if pose.visibility[i] <= 0.0 {
            continue;
        }
        let Some(&mesh) = shape.objects[i].meshes.get(detail.mesh_offset) else {
            continue;
        };
        if shape.meshes[mesh].is_none() {
            continue;
        }
        parts.push((
            i,
            pose.frames[i],
            pose.material_frames[i],
            color.map(f32::to_bits),
        ));
    }
    Ok(Key(parts))
}

impl Layout {
    /// Write this pose into `data`: in place when the structure is
    /// unchanged (returns false), or rebuilt with a new layout (true).
    pub fn pose(
        layout: &mut Option<Layout>,
        data: &mut SceneData,
        binding: &Binding<'_>,
        pose: &Pose,
        transform: Mat4,
    ) -> Result<bool> {
        let shape = binding.shape;
        ensure!(
            pose.nodes.len() == shape.nodes.len()
                && pose.nodes.iter().all(|m| m.is_finite())
                && pose.visibility.len() == shape.objects.len()
                && pose.frames.len() == shape.objects.len()
                && pose.material_frames.len() == shape.objects.len()
                && binding.colors.len() == shape.objects.len(),
            "Invalid posed shape layout"
        );
        ensure!(
            transform.is_finite() && transform.determinant() > 1e-8,
            "Invalid/reflected model transform"
        );
        let key = key(binding, pose)?;
        let rebuilt = layout.as_ref().is_none_or(|l| l.key != key);
        if rebuilt {
            *layout = Some(Self::build(key, data, binding)?);
        }
        let layout = layout.as_mut().expect("built above");
        layout.write(data, shape, pose, transform)?;
        Ok(rebuilt)
    }

    /// Lay out vertices, indices and batches as `append_shape` does:
    /// grouped by scene material in ascending order, each group's triangles
    /// in object, primitive and triangle order, one vertex per corner.
    fn build(key: Key, data: &mut SceneData, binding: &Binding<'_>) -> Result<Self> {
        let shape = binding.shape;
        ensure!(
            binding.materials.len() == shape.materials.len()
                && binding.translucent_materials.len() == binding.materials.len()
                && binding
                    .materials
                    .iter()
                    .chain(binding.translucent_materials)
                    .all(|m| *m < data.materials.len())
                && binding.unassigned_material < data.materials.len(),
            "Unbound model materials"
        );
        let mut parts = Vec::new();
        let mut groups: BTreeMap<usize, Vec<(u32, u32, [f32; 2], [f32; 4])>> = BTreeMap::new();
        for &(object, frame, material_frame, color) in &key.0 {
            let color = color.map(f32::from_bits);
            ensure!(
                color
                    .iter()
                    .all(|c| c.is_finite() && (0.0..=1.0).contains(c)),
                "Invalid avatar paint"
            );
            let detail = &shape.details[binding.detail];
            let mesh_index = shape.objects[object].meshes[detail.mesh_offset];
            let mesh = shape.meshes[mesh_index].as_ref().expect("keyed mesh");
            let offset = frame
                .checked_mul(mesh.frame_vertices)
                .context("Mesh frame overflow")?;
            ensure!(
                mesh.skin.is_some() || offset + mesh.frame_vertices <= mesh.positions.len(),
                "Vertex frame out of range"
            );
            let bindings = if color[3] < 1.0 {
                binding.translucent_materials
            } else {
                binding.materials
            };
            let slot = parts.len() as u32;
            let uv_offset = material_frame * mesh.frame_vertices;
            for primitive in &mesh.primitives {
                let material = primitive
                    .material
                    .map_or(binding.unassigned_material, |i| bindings[i]);
                let group = groups.entry(material).or_default();
                for triangle in &primitive.triangles {
                    for &v in triangle {
                        ensure!((v as usize) < mesh.frame_vertices, "Vertex out of range");
                        let uv = mesh
                            .uv
                            .get(uv_offset + v as usize)
                            .copied()
                            .unwrap_or([0.0; 2]);
                        group.push((slot, v, uv, color));
                    }
                }
            }
            parts.push(Part {
                object,
                mesh: mesh_index,
                offset,
            });
        }
        let count: usize = groups.values().map(Vec::len).sum();
        ensure!(count <= u32::MAX as usize, "Model vertex limit exceeded");
        data.vertices.clear();
        data.indices.clear();
        data.batches.clear();
        let mut sources = Vec::with_capacity(count);
        for (material, vertices) in groups {
            let start = data.indices.len() as u32;
            for (slot, v, uv, color) in vertices {
                data.indices.push(data.vertices.len() as u32);
                data.vertices.push(SceneVertex {
                    position: [0.0; 3],
                    normal: [0.0; 3],
                    uv,
                    lightmap_uv: [0.0; 2],
                    color,
                    fx: [0.; 4],
                });
                sources.push((slot, v));
            }
            data.batches.push(MeshBatch {
                indices: start..data.indices.len() as u32,
                material,
                center: [0.0; 3],
            });
        }
        let parts_len = parts.len();
        Ok(Self {
            key,
            parts,
            sources,
            positions: vec![Vec::new(); parts_len],
            normals: vec![Vec::new(); parts_len],
        })
    }

    /// This frame's positions, normals and batch centres.
    fn write(
        &mut self,
        data: &mut SceneData,
        shape: &Shape,
        pose: &Pose,
        transform: Mat4,
    ) -> Result<()> {
        // `animation::triangles`: each object's mesh posed by its node.
        for (slot, part) in self.parts.iter().enumerate() {
            let mesh = shape.meshes[part.mesh].as_ref().expect("keyed mesh");
            let positions = &mut self.positions[slot];
            let normals = &mut self.normals[slot];
            positions.clear();
            normals.clear();
            if let Some(skin) = &mesh.skin {
                positions.resize(mesh.frame_vertices, Vec3::ZERO);
                normals.resize(mesh.frame_vertices, Vec3::ZERO);
                let matrices: Vec<_> = skin
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| pose.nodes[*n] * Mat4::from_cols_array(&skin.inverse_bind[i]))
                    .collect();
                for influence in &skin.influences {
                    let m = matrices[influence.bone];
                    let p = Vec3::from(mesh.positions[influence.vertex]);
                    let n = Vec3::from(mesh.normals[influence.vertex]);
                    positions[influence.vertex] += m.transform_point3(p) * influence.weight;
                    normals[influence.vertex] +=
                        m.inverse().transpose().transform_vector3(n) * influence.weight;
                }
            } else {
                let node = shape.objects[part.object]
                    .node
                    .map_or(Mat4::IDENTITY, |n| pose.nodes[n]);
                let normal = node.inverse().transpose();
                let frame = part.offset..part.offset + mesh.frame_vertices;
                positions.extend(
                    mesh.positions[frame.clone()]
                        .iter()
                        .map(|p| node.transform_point3(Vec3::from(*p))),
                );
                normals.extend(
                    mesh.normals[frame]
                        .iter()
                        .map(|n| normal.transform_vector3(Vec3::from(*n))),
                );
            }
            for n in normals.iter_mut() {
                *n = n.normalize_or_zero();
            }
        }
        // `append_shape`: the model transform on top.
        let normal = transform.inverse().transpose();
        for (vertex, &(slot, v)) in data.vertices.iter_mut().zip(&self.sources) {
            let (slot, v) = (slot as usize, v as usize);
            let position = transform.transform_point3(self.positions[slot][v]);
            let n = normal
                .transform_vector3(self.normals[slot][v])
                .normalize_or_zero();
            ensure!(
                position.is_finite() && n.is_finite(),
                "Posed geometry exceeds finite coordinate range"
            );
            vertex.position = position.to_array();
            vertex.normal = n.to_array();
        }
        for batch in &mut data.batches {
            let range = batch.indices.start as usize..batch.indices.end as usize;
            let sum: Vec3 = data.vertices[range.clone()]
                .iter()
                .map(|v| Vec3::from(v.position))
                .sum();
            batch.center = (sum / range.len() as f32).to_array();
        }
        Ok(())
    }
}
