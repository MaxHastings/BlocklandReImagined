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
    /// A second UV set, one per vertex or none: where a baked lightmap
    /// ([`Surface::lightmap`]) is read (glTF `TEXCOORD_1`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uv1: Vec<[f32; 2]>,
    /// Per-vertex colours (RGBA, glTF `COLOR_0`) of a surface lit by its vertices alone (a layer-blended terrain's baked light), one
    /// per vertex or none. Models and other surfaces ignore them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<[u8; 4]>,
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
    /// Solid cutout geometry: discard texels below this alpha, then write depth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha_cutoff: Option<f32>,
    /// Both faces draw (glTF `doubleSided`): back faces are not culled.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub double_sided: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv_motion: Option<UvMotion>,
    /// Several texture modifiers around one picture, outermost first, multiplied into one map
    /// each frame (`crate::uv_affine::chain_affine`). When present it replaces `uv_motion`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uv_chain: Vec<UvMotion>,
    pub unlit: bool,
    pub environment: bool,
    pub mipmaps: bool,
    pub detail_map: Option<usize>,
    pub bump_map: Option<usize>,
    pub reflectance_map: Option<usize>,
    pub detail_scale: f32,
    pub reflectance: f32,
    /// None preserves legacy DTS paint-overlay semantics; false uses texture RGBA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint_overlay: Option<bool>,
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
    /// Painted only where a mask says so, in the colour of an avatar slot:
    /// the change-colour mask of a team-coloured armour plate ([`TintMask`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<TintMask>,
    /// Up to four diffuse layers blended by the channels of one weight map, each with its own texture-coordinate
    /// transform, lit by the vertex light only (a terrain's layers). See [`LayerBlend`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<LayerBlend>,
}
/// A material that is a weighted sum of up to four texture layers, as a terrain's are: the weight map's red, green, blue and
/// alpha weight layers 0 to 3, read with the mesh's second UV set; each layer's own UVs go through its affine first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerBlend {
    /// The materials (by index) whose textures are the layers, in weight-channel order (1 to 4).
    pub layers: Vec<usize>,
    /// The material whose texture holds the weights (red, green, blue, alpha).
    pub weights: usize,
    /// Per layer `[a, b, c, d, tx, ty]`, with `uv' = (a u + c v + tx, b u + d v + ty)`.
    pub transforms: Vec<[f32; 6]>,
}
/// Texture coordinates in UV space: scale, rotate about center, then offset
/// and scroll. Angles are degrees, rates per second. Geometry and lighting UVs stay put.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UvMotion {
    pub scale: [f32; 2],
    pub offset: [f32; 2],
    pub scroll: [f32; 2],
    pub center: [f32; 2],
    pub rotation_degrees: f32,
    pub rotation_rate_degrees: f32,
    /// A periodic wave on each texture axis (u, then v), on top of the rest.
    pub waves: [UvWave; 2],
    /// A three-axis rotation (pitch, yaw, roll) in place of the flat `rotation_*` pair,
    /// with each axis able to spin or swing; the picture is turned by the upper-left
    /// two-by-two of the rotation matrix, about `center`.
    pub rotator: Option<UvRotator>,
}
/// Three-axis texture rotation (an Unreal `TexRotator`): per axis (pitch, yaw, roll) the
/// angle is `base + rate x time + amplitude x sin(2 pi (fract(wave_rate x time) +
/// wave_phase))`, in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UvRotator {
    pub base_degrees: [f32; 3],
    /// Degrees a second of constant rotation.
    pub rate_degrees: [f32; 3],
    pub wave_amplitude_degrees: [f32; 3],
    /// Cycles a second of the swing.
    pub wave_rate: [f32; 3],
    /// A fraction of a cycle.
    pub wave_phase: [f32; 3],
}
/// The shape of a [`UvWave`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UvWaveKind {
    /// No motion on this axis.
    #[default]
    None,
    /// Slides the texture back and forth: offset = sin(2pi(cycle + phase)) x amplitude.
    Pan,
    /// Stretches the texture about `center`: scale = 1 + sin(2pi(cycle + phase)) x amplitude.
    Stretch,
    /// Like `Stretch`, but the sine runs a quarter turn per cycle and then restarts:
    /// scale = 1 + sin(pi/2 x cycle + 2pi x phase) x amplitude.
    StretchRepeat,
    /// Shakes the texture by a random offset in [0, amplitude), redrawn every frame
    /// (60 a second) during the part of each cycle after `phase` and held at the
    /// value the previous cycle ended with before it.
    Jitter,
}
/// One axis of a [`UvMotion`] wave. `rate` is cycles a second and `phase` a
/// fraction of a cycle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UvWave {
    pub kind: UvWaveKind,
    pub rate: f32,
    pub phase: f32,
    pub amplitude: f32,
}
impl Default for UvWave {
    fn default() -> Self {
        Self {
            kind: UvWaveKind::None,
            rate: 0.,
            phase: 0.,
            amplitude: 0.,
        }
    }
}
impl Default for UvMotion {
    fn default() -> Self {
        Self {
            scale: [1.; 2],
            offset: [0.; 2],
            scroll: [0.; 2],
            center: [0.; 2],
            rotation_degrees: 0.,
            rotation_rate_degrees: 0.,
            waves: [UvWave::default(); 2],
            rotator: None,
        }
    }
}
impl UvMotion {
    pub fn valid(self) -> bool {
        self.scale
            .iter()
            .chain(&self.offset)
            .chain(&self.scroll)
            .chain(&self.center)
            .all(|v| v.is_finite() && v.abs() <= 10000.)
            && self.rotation_degrees.is_finite()
            && self.rotation_degrees.abs() <= 36000.
            && self.rotation_rate_degrees.is_finite()
            && self.rotation_rate_degrees.abs() <= 36000.
            && self.waves.iter().all(|w| {
                [w.rate, w.phase, w.amplitude]
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 10000.)
            })
            && self.rotator.is_none_or(|r| {
                [
                    r.base_degrees,
                    r.rate_degrees,
                    r.wave_amplitude_degrees,
                    r.wave_rate,
                    r.wave_phase,
                ]
                .iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 100000.)
            })
    }
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
    /// A baked lightmap (RGB, sRGB-free linear light) read with the mesh's
    /// second UV set. It stands for the sun and ambient light on the
    /// surface, shadows included, so the live ones are not added to it;
    /// lights, moving shadows and highlights still are. `lightmap_strength`
    /// multiplies it (the map's brightest texels can exceed 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightmap: Option<usize>,
    #[serde(default = "Metal::one")]
    pub lightmap_strength: f32,
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
    /// Authored environment reflection over diffuse paint, instead of an
    /// inferred metallic response. Maps and colours belong to the Add-On.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reflection: Option<SurfaceReflection>,
}
/// A masked tint: each texel of the material's colour is multiplied by
/// `(1 - mask) + mask * colour`, so the mask's 0 leaves the texel as it is
/// and its 1 takes the colour of the avatar slot `slot` (the colour the
/// holder or wearer's avatar, or a rule's `set_avatar_colors`, gives that
/// slot: team colours are set that way). Halo's change-colour mask works
/// this way. Lit model materials only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TintMask {
    /// The material (by index) whose texture is the mask, in its red
    /// channel (linear, 0 to 1).
    pub mask: usize,
    /// An avatar colour slot (`chest`, `pants`, `larm` ...).
    pub slot: String,
}
/// A display-space reflection layer, useful for imported fixed-function
/// materials. The mask's red channel controls reflection coverage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceReflection {
    /// Equirectangular environment image: +Z at u=.5, +X at u=.75,
    /// +Y at v=0. Referenced textures are other materials, as for Surface.
    pub environment: usize,
    pub mask: usize,
    /// Reflection colour times brightness, looking straight at the face.
    pub perpendicular: [f32; 3],
    /// Reflection colour times brightness, looking along the face.
    pub parallel: [f32; 3],
    /// Restrict diffuse detail to the inverse of the reflection mask.
    #[serde(default)]
    pub detail_inverse_mask: bool,
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
/// Interpolation between authored node rotation keys, independent of layer
/// influence, transitions, scale-axis rotation and ground motion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationInterpolation {
    #[default]
    Spherical,
    NormalizedLinear,
}
impl RotationInterpolation {
    fn is_spherical(&self) -> bool {
        *self == Self::Spherical
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub frames: usize,
    pub duration: f32,
    pub looping: bool,
    pub additive: bool,
    /// An additive clip whose translations add to the node's translation in
    /// its parent's frame, as Halo's overlays do, instead of being carried
    /// by the node's own pose (the default, which rotates and scales them by
    /// it). Rotations and scales are unaffected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub parent_translations: bool,
    pub priority: i32,
    #[serde(default, skip_serializing_if = "RotationInterpolation::is_spherical")]
    pub rotation_interpolation: RotationInterpolation,
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
                    && mesh.normals.len() == mesh.positions.len()
                    && (mesh.uv1.is_empty() || mesh.uv1.len() == mesh.positions.len()),
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
            ensure!(
                material.uv_motion.is_none_or(UvMotion::valid),
                "Invalid material UV motion"
            );
            ensure!(
                material.tint.as_ref().is_none_or(|t| {
                    t.mask < self.materials.len()
                        && (1..=32).contains(&t.slot.len())
                        && t.slot
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                }),
                "Invalid tint mask on {}",
                material.name
            );
            ensure!(
                material
                    .alpha_cutoff
                    .is_none_or(|a| a.is_finite() && (0.0..=1.0).contains(&a)),
                "Invalid material alpha cutoff"
            );
            if let Some(surface) = &material.surface {
                let index = |m: Option<usize>| m.is_none_or(|m| m < self.materials.len());
                ensure!(
                    index(surface.normal)
                        && index(surface.metallic_roughness)
                        && index(surface.occlusion)
                        && index(surface.lightmap)
                        && (0.0..=16.0).contains(&surface.lightmap_strength)
                        && index(surface.emissive_map)
                        && index(surface.detail)
                        && surface.reflection.as_ref().is_none_or(|r| {
                            r.environment < self.materials.len()
                                && r.mask < self.materials.len()
                                && r.perpendicular
                                    .iter()
                                    .chain(&r.parallel)
                                    .all(|c| (0.0..=1000.0).contains(c))
                        })
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
