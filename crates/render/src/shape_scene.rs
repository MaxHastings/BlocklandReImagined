//! Posed native models share the map renderer without depending on legacy readers.
use crate::scene::{MeshBatch, SceneData, SceneVertex};
use anyhow::{Result, ensure};
use bri_content::{
    animation::{Pose, triangles},
    shape::Shape,
};
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;

pub struct ShapeInstance<'a> {
    pub shape: &'a Shape,
    pub pose: &'a Pose,
    pub detail: usize,
    pub transform: Mat4,
    /// Shape material index -> scene material index, resolved by the content adapter.
    pub materials: &'a [usize],
    /// Optional parallel bindings when a part's paint has translucent alpha.
    pub translucent_materials: Option<&'a [usize]>,
    pub unassigned_material: usize,
}

impl SceneData {
    /// Returning None hides a named avatar object; Some supplies its paint color.
    /// Materials remain independently bound so face/decal/overlay textures can
    /// keep their original UVs and semantics. This does not infer outfit choices.
    pub fn append_shape(
        &mut self,
        instance: ShapeInstance<'_>,
        appearance: impl Fn(&str) -> Option<[f32; 4]>,
    ) -> Result<()> {
        let ShapeInstance {
            shape,
            pose,
            detail,
            transform,
            materials,
            translucent_materials,
            unassigned_material,
        } = instance;
        shape.validate()?;
        ensure!(
            pose.nodes.len() == shape.nodes.len()
                && pose.nodes.iter().all(|m| m.is_finite())
                && pose.visibility.len() == shape.objects.len()
                && pose.frames.len() == shape.objects.len()
                && pose.material_frames.len() == shape.objects.len(),
            "Invalid posed shape layout"
        );
        ensure!(
            transform.is_finite() && transform.determinant() > 1e-8,
            "Invalid/reflected model transform"
        );
        ensure!(
            materials.len() == shape.materials.len()
                && materials.iter().all(|m| *m < self.materials.len())
                && unassigned_material < self.materials.len(),
            "Unbound model materials"
        );
        let colors: Vec<_> = shape.objects.iter().map(|o| appearance(&o.name)).collect();
        ensure!(
            translucent_materials.is_none_or(
                |m| m.len() == materials.len() && m.iter().all(|i| *i < self.materials.len())
            ),
            "Unbound translucent model materials"
        );
        ensure!(
            colors
                .iter()
                .flatten()
                .flatten()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(c)),
            "Invalid avatar paint"
        );
        let visible: BTreeMap<_, _> = shape
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| (o.name.as_str(), colors[i].is_some()))
            .collect();
        let geometry = triangles(shape, pose, detail, |name| {
            visible.get(name).copied().unwrap_or(false)
        })?;
        ensure!(
            self.vertices
                .len()
                .checked_add(geometry.len() * 3)
                .is_some_and(|n| n <= u32::MAX as usize),
            "Model vertex limit exceeded"
        );
        let normal = transform.inverse().transpose();
        let mut groups: BTreeMap<usize, Vec<SceneVertex>> = BTreeMap::new();
        for triangle in geometry {
            let color = colors[triangle.object].expect("filtered visible object");
            let bindings = if color[3] < 1.0 {
                translucent_materials.unwrap_or(materials)
            } else {
                materials
            };
            let material = triangle
                .material
                .map_or(unassigned_material, |i| bindings[i]);
            for vertex in triangle.vertices {
                groups.entry(material).or_default().push(SceneVertex {
                    position: transform.transform_point3(vertex.position).to_array(),
                    normal: normal
                        .transform_vector3(vertex.normal)
                        .normalize_or_zero()
                        .to_array(),
                    uv: vertex.uv,
                    lightmap_uv: [0.0; 2],
                    color,
                    fx: [0.; 4],
                });
            }
        }
        ensure!(
            groups.values().flatten().all(|v| v
                .position
                .iter()
                .chain(&v.normal)
                .all(|x| x.is_finite())),
            "Posed geometry exceeds finite coordinate range"
        );
        for (material, vertices) in groups {
            let start = self.indices.len() as u32;
            let center = vertices
                .iter()
                .map(|v| Vec3::from(v.position))
                .sum::<Vec3>()
                / vertices.len() as f32;
            for vertex in vertices {
                self.indices.push(self.vertices.len() as u32);
                self.vertices.push(vertex);
            }
            self.batches.push(MeshBatch {
                indices: start..self.indices.len() as u32,
                material,
                center: center.to_array(),
            });
        }
        Ok(())
    }
}
