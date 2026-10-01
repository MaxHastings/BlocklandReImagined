//! Native model and animation data. Coordinates are X-right, Y-up, -Z-forward.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shape {
    pub schema_version: u32,
    pub id: String,
    pub nodes: Vec<Node>,
    pub objects: Vec<Object>,
    pub details: Vec<Detail>,
    pub meshes: Vec<Option<Mesh>>,
    pub materials: Vec<Material>,
    pub animations: Vec<Animation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Object {
    pub name: String,
    pub node: Option<usize>,
    pub meshes: Vec<usize>,
    pub visibility: f32,
    pub frame: u32,
    pub material_frame: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detail {
    pub name: String,
    pub pixel_threshold: f32,
    pub object_start: usize,
    pub object_count: usize,
    pub mesh_offset: usize,
    pub collision: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mesh {
    pub frame_vertices: usize,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub primitives: Vec<Primitive>,
    pub skin: Option<Skin>,
    pub billboard: bool,
    pub billboard_y: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Primitive {
    pub material: Option<usize>,
    pub triangles: Vec<[u32; 3]>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skin {
    pub inverse_bind: Vec<[f32; 16]>,
    pub nodes: Vec<usize>,
    pub influences: Vec<Influence>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Influence {
    pub vertex: usize,
    pub bone: usize,
    pub weight: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub wrap_u: bool,
    pub wrap_v: bool,
    pub blend: String,
    pub unlit: bool,
    pub environment: bool,
    pub mipmaps: bool,
    pub detail_map: Option<usize>,
    pub bump_map: Option<usize>,
    pub reflectance_map: Option<usize>,
    pub detail_scale: f32,
    pub reflectance: f32,
    /// Drawn as bare metal: tinted reflections of its surroundings and
    /// sharp highlights instead of a painted, diffusely lit surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metal: Option<Metal>,
}
/// A physically based metal surface (the metallic workflow real-time
/// engines use): it has no diffuse colour, only reflection.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metal {
    /// Reflectance at normal incidence, linear RGB (steel about 0.56,
    /// 0.57, 0.58; gold 1.0, 0.71, 0.29). The material's own texture
    /// multiplies it.
    pub color: [f32; 3],
    /// 0 a perfect mirror, 1 fully matte.
    pub roughness: f32,
    /// A material (by index) whose texture is fine surface detail, in
    /// linear channels: red scales roughness (0.5 unchanged, 1 doubles),
    /// green darkens scratches and grime (1 clean), blue and alpha tilt
    /// the surface along u and v (0.5 flat).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<usize>,
    /// Times the detail texture repeats across the model's own texture.
    #[serde(default = "Metal::one")]
    pub detail_scale: f32,
    /// How strongly the detail tilts the surface (0 flat).
    #[serde(default = "Metal::one")]
    pub detail_strength: f32,
}
impl Metal {
    fn one() -> f32 {
        1.0
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub frames: usize,
    pub duration: f32,
    pub looping: bool,
    pub additive: bool,
    pub priority: i32,
    pub nodes: Vec<NodeTrack>,
    pub objects: Vec<ObjectTrack>,
    pub ground_translations: Vec<[f32; 3]>,
    pub ground_rotations: Vec<[f32; 4]>,
    pub triggers: Vec<Trigger>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTrack {
    pub node: String,
    pub rotations: Vec<[f32; 4]>,
    pub translations: Vec<[f32; 3]>,
    pub scales: Vec<[f32; 3]>,
    pub scale_rotations: Vec<[f32; 4]>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectTrack {
    pub object: usize,
    pub visibility: Vec<f32>,
    pub frames: Vec<u32>,
    pub material_frames: Vec<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub state: u32,
    pub position: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipSet {
    pub schema_version: u32,
    pub id: String,
    pub animations: Vec<Animation>,
}

impl Shape {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && !self.id.is_empty(),
            "Unknown/empty shape schema"
        );
        for (i, node) in self.nodes.iter().enumerate() {
            let mut current = node.parent;
            let mut depth = 0;
            while let Some(parent) = current {
                ensure!(
                    parent < self.nodes.len() && parent != i && depth < 256,
                    "Cyclic/invalid/excessively deep skeleton parent"
                );
                current = self.nodes[parent].parent;
                depth += 1;
            }
            ensure!(
                node.translation
                    .iter()
                    .chain(&node.rotation)
                    .all(|v| v.is_finite()),
                "Non-finite node transform"
            );
            ensure!(unit_quaternion(&node.rotation), "Non-unit node rotation");
        }
        for object in &self.objects {
            ensure!(
                object.node.is_none_or(|n| n < self.nodes.len())
                    && object.meshes.iter().all(|m| *m < self.meshes.len()),
                "Invalid object references"
            );
            ensure!(
                object.visibility.is_finite(),
                "Non-finite object visibility"
            );
        }
        for mesh in self.meshes.iter().flatten() {
            ensure!(
                mesh.frame_vertices > 0
                    && mesh.positions.len() % mesh.frame_vertices == 0
                    && mesh.normals.len() == mesh.positions.len(),
                "Invalid mesh frame layout: frame={} positions={} normals={}",
                mesh.frame_vertices,
                mesh.positions.len(),
                mesh.normals.len()
            );
            ensure!(
                mesh.positions
                    .iter()
                    .chain(&mesh.normals)
                    .flatten()
                    .all(|v| v.is_finite()),
                "Non-finite mesh data"
            );
            ensure!(
                mesh.uv.iter().flatten().all(|v| v.is_finite()),
                "Non-finite texture coordinates"
            );
            for primitive in &mesh.primitives {
                ensure!(
                    primitive.material.is_none_or(|m| m < self.materials.len()),
                    "Invalid material index"
                );
                ensure!(
                    primitive
                        .triangles
                        .iter()
                        .flatten()
                        .all(|i| (*i as usize) < mesh.frame_vertices),
                    "Invalid triangle index"
                );
            }
            if let Some(skin) = &mesh.skin {
                ensure!(
                    skin.nodes.len() == skin.inverse_bind.len()
                        && skin.nodes.iter().all(|n| *n < self.nodes.len()),
                    "Invalid skin bones"
                );
                ensure!(
                    skin.influences
                        .iter()
                        .all(|i| i.vertex < mesh.frame_vertices
                            && i.bone < skin.nodes.len()
                            && i.weight.is_finite()
                            && i.weight >= 0.0),
                    "Invalid skin influence"
                );
                ensure!(
                    skin.inverse_bind.iter().flatten().all(|v| v.is_finite()),
                    "Non-finite inverse bind matrix"
                );
            }
        }
        for material in &self.materials {
            if let Some(metal) = &material.metal {
                ensure!(
                    metal.color.iter().all(|c| (0.0..=1.0).contains(c))
                        && (0.0..=1.0).contains(&metal.roughness)
                        && metal.detail.is_none_or(|d| d < self.materials.len())
                        && metal.detail_scale.is_finite()
                        && metal.detail_scale > 0.0
                        && metal.detail_scale <= 1024.0
                        && (0.0..=4.0).contains(&metal.detail_strength),
                    "Invalid metal material {}",
                    material.name
                );
            }
        }
        for detail in &self.details {
            ensure!(
                detail
                    .object_start
                    .checked_add(detail.object_count)
                    .is_some_and(|n| n <= self.objects.len()),
                "Invalid detail object range"
            );
        }
        for animation in &self.animations {
            animation.validate()?;
        }
        Ok(())
    }
}
impl Animation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.frames > 0
                && self.frames <= 100_000
                && self.duration.is_finite()
                && self.duration >= 0.0,
            "Invalid animation timing"
        );
        for track in &self.nodes {
            ensure!(
                [
                    track.rotations.len(),
                    track.translations.len(),
                    track.scales.len(),
                    track.scale_rotations.len()
                ]
                .iter()
                .all(|n| *n == 0 || *n == self.frames),
                "Invalid animation channel length"
            );
            ensure!(
                track
                    .rotations
                    .iter()
                    .chain(&track.scale_rotations)
                    .flatten()
                    .chain(track.translations.iter().chain(&track.scales).flatten())
                    .all(|v| v.is_finite()),
                "Non-finite animation channel"
            );
            ensure!(
                track
                    .rotations
                    .iter()
                    .chain(&track.scale_rotations)
                    .all(unit_quaternion),
                "Non-unit animation rotation"
            );
        }
        for track in &self.objects {
            ensure!(
                [
                    track.visibility.len(),
                    track.frames.len(),
                    track.material_frames.len()
                ]
                .iter()
                .all(|n| *n == 0 || *n == self.frames),
                "Invalid object animation channel length"
            );
            ensure!(
                track.visibility.iter().all(|v| v.is_finite()),
                "Non-finite animated visibility"
            );
        }
        ensure!(
            self.ground_translations.len() == self.ground_rotations.len()
                && self
                    .ground_translations
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite())
                && self.ground_rotations.iter().all(unit_quaternion),
            "Invalid ground motion"
        );
        ensure!(
            self.triggers
                .iter()
                .all(|t| t.position.is_finite() && (0.0..=1.0).contains(&t.position)),
            "Invalid animation trigger time"
        );
        Ok(())
    }
}

fn unit_quaternion(q: &[f32; 4]) -> bool {
    (q.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 0.001
}
