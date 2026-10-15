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
    /// Shows a digit of a value taken from what the model belongs to (the
    /// rounds left in a held gun) by choosing its material
    /// frame, so a counter on the model itself follows the game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<Live>,
}
/// One digit of a live value, drawn by an object's material frame
/// (`Object::live`): frame `d` shows digit `d`. With `uv_step` the mesh
/// needs only one texture-coordinate frame: frame `d` is it moved by
/// `d * uv_step`, so ten digits side by side in a strip need no more.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Live {
    /// The value, by name ([`LiveValues::get`]).
    pub value: String,
    /// Which digit: 0 the ones, 1 the tens, and so on.
    #[serde(default)]
    pub place: u32,
    /// Digits per place (10 decimal), also the number of frames used.
    #[serde(default = "Live::ten")]
    pub base: u32,
    /// Multiplies the value before its digits are taken.
    #[serde(default = "Live::one")]
    pub scale: f32,
    /// Hide this digit while the value has no digit this high (no leading
    /// zeros); the ones digit always shows.
    #[serde(default)]
    pub blank_leading: bool,
    /// Texture-coordinate shift from one digit's frame to the next.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv_step: Option<[f32; 2]>,
}
impl Live {
    fn ten() -> u32 {
        10
    }
    fn one() -> f32 {
        1.0
    }
    /// The digit `value` shows here, or `None` when it is hidden.
    pub fn digit(&self, value: u32) -> Option<u32> {
        let scaled = (value as f32 * self.scale).max(0.0) as u64;
        let base = u64::from(self.base.max(2));
        let unit = base.checked_pow(self.place).unwrap_or(u64::MAX);
        if self.blank_leading && self.place > 0 && scaled < unit {
            return None;
        }
        Some(((scaled / unit) % base) as u32)
    }
}
/// The values a model's live objects (`Object::live`) can show, from what
/// the model belongs to; `None` where it has no such value (another
/// player's gun, whose magazine this client does not know).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LiveValues {
    /// Rounds ready to fire in a held gun (its magazine).
    pub rounds: Option<u32>,
    /// Rounds carried for it beyond the magazine.
    pub reserve: Option<u32>,
    /// A full magazine.
    pub magazine: Option<u32>,
}
impl LiveValues {
    pub const NAMES: [&str; 3] = ["rounds", "reserve", "magazine"];
    pub fn get(&self, name: &str) -> Option<u32> {
        match name {
            "rounds" => self.rounds,
            "reserve" => self.reserve,
            "magazine" => self.magazine,
            _ => None,
        }
    }
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
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
    /// The full physically based look a modern model brings (glTF's
    /// metallic-roughness material): surface relief, metal and roughness
    /// per texel, ambient occlusion and glow. Drawn with it instead of
    /// `metal` when both are set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<Surface>,
}
/// Physically based maps for a [`Material`], each naming another material
/// (by index) whose texture holds it, as [`Metal::detail`] does. The
/// material's own texture is the base colour.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    /// Tangent-space normal map (glTF and OpenGL convention: green up).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal: Option<usize>,
    /// How strongly the normal map tilts the surface (0 flat).
    #[serde(default = "Metal::one")]
    pub normal_scale: f32,
    /// Metal in blue and roughness in green (glTF's layout), multiplying
    /// `metallic` and `roughness`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metallic_roughness: Option<usize>,
    /// 0 a painted (dielectric) surface, 1 bare metal.
    pub metallic: f32,
    /// 0 a perfect mirror, 1 fully matte.
    pub roughness: f32,
    /// Ambient occlusion in red: how much surrounding light reaches a
    /// crevice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occlusion: Option<usize>,
    #[serde(default = "Metal::one")]
    pub occlusion_strength: f32,
    /// Glow colour map (sRGB), multiplying `emissive`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emissive_map: Option<usize>,
    /// Light the surface gives off whatever the world's light, linear RGB.
    #[serde(default)]
    pub emissive: [f32; 3],
    /// A small texture repeated `detail_scale` times across the colour map
    /// for close-up grain (linear: mid-grey leaves the colour as it is,
    /// white doubles it), as Torque and Halo lay detail over a stretched
    /// base map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<usize>,
    #[serde(default = "Metal::one")]
    pub detail_scale: f32,
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
    /// Give every mesh a live object draws with `Live::uv_step` its digit
    /// frames: frame `d` is its first texture-coordinate frame moved by
    /// `d * uv_step`. Run once after loading, before posing.
    pub fn expand_live_frames(&mut self) {
        for object in &self.objects {
            let Some(live) = &object.live else { continue };
            let Some(step) = live.uv_step else { continue };
            for &m in &object.meshes {
                let Some(Some(mesh)) = self.meshes.get_mut(m) else {
                    continue;
                };
                let n = mesh.frame_vertices;
                if n == 0 || mesh.uv.len() != n {
                    continue;
                }
                let first = mesh.uv.clone();
                for d in 1..live.base {
                    let shift = [step[0] * d as f32, step[1] * d as f32];
                    mesh.uv
                        .extend(first.iter().map(|uv| [uv[0] + shift[0], uv[1] + shift[1]]));
                }
            }
        }
    }
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
            if let Some(live) = &object.live {
                ensure!(
                    LiveValues::NAMES.contains(&live.value.as_str())
                        && (2..=64).contains(&live.base)
                        && live.place < 10
                        && live.scale.is_finite()
                        && live.scale >= 0.0
                        && live.uv_step.is_none_or(|s| s.iter().all(|v| v.is_finite())),
                    "Invalid live value on {}: values are {}",
                    object.name,
                    LiveValues::NAMES.join(", ")
                );
            }
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
            if let Some(surface) = &material.surface {
                let index = |m: Option<usize>| m.is_none_or(|m| m < self.materials.len());
                ensure!(
                    index(surface.normal)
                        && index(surface.metallic_roughness)
                        && index(surface.occlusion)
                        && index(surface.emissive_map)
                        && index(surface.detail)
                        && (0.01..=4096.0).contains(&surface.detail_scale)
                        && (0.0..=8.0).contains(&surface.normal_scale)
                        && (0.0..=1.0).contains(&surface.metallic)
                        && (0.0..=1.0).contains(&surface.roughness)
                        && (0.0..=1.0).contains(&surface.occlusion_strength)
                        && surface.emissive.iter().all(|c| (0.0..=1000.0).contains(c)),
                    "Invalid surface material {}",
                    material.name
                );
            }
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
