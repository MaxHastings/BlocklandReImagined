//! Native item presentation only. Gameplay identity/state stays in the host.
//! The offline assembler owns legacy fields; this module reads typed native data.
use anyhow::{Context, Result, ensure};
use bri_content::{
    animation::{Pose, sample},
    shape::Shape,
};
use bri_render::{
    scene::{AlphaMode, Material, MaterialKind, SceneData, SceneImage},
    shape_scene::ShapeInstance,
};
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextureResource {
    pub file: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub source: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelResource {
    pub file: String,
    pub sha256: String,
    pub source: String,
    pub source_sha256: String,
    pub textures: Vec<String>,
    /// Authored DTS object box; native axes, model pivot unchanged.
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}
impl ModelResource {
    pub fn bounds(&self) -> bri_weapons::ItemBounds {
        bri_weapons::ItemBounds {
            min: self.bounds_min,
            max: self.bounds_max,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ItemPhysicsCatalog {
    pub schema_version: u32,
    pub items: BTreeMap<String, bri_weapons::ItemBounds>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ItemPresentation {
    pub model: String,
    pub image: String,
    pub tint: [f32; 4],
    pub icon: Option<String>,
    pub evidence: bri_weapons::Evidence,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ImagePresentation {
    pub model: String,
    pub mount_point: u32,
    pub offset: [f32; 3],
    pub eye_offset: [f32; 3],
    pub source_rotation_degrees: [f32; 3],
    pub eye_rotation_degrees: [f32; 3],
    pub tint: [f32; 4],
    pub evidence: bri_weapons::Evidence,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectilePresentation {
    pub model: Option<String>,
    pub tint: [f32; 4],
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Presentation {
    pub schema_version: u32,
    pub id: String,
    pub weapons_sha256: String,
    pub item_physics_sha256: String,
    pub models: BTreeMap<String, ModelResource>,
    pub textures: BTreeMap<String, TextureResource>,
    pub items: BTreeMap<String, ItemPresentation>,
    pub images: BTreeMap<String, ImagePresentation>,
    pub projectiles: BTreeMap<String, ProjectilePresentation>,
    pub diagnostics: Vec<String>,
}
pub struct ItemAssets {
    pub presentation: Presentation,
    pub item_physics: ItemPhysicsCatalog,
    shapes: BTreeMap<String, Shape>,
    textures: BTreeMap<String, SceneImage>,
}
/// Resource bindings persist while the host updates only posed geometry.
pub struct ItemMesh {
    pub data: SceneData,
    model: String,
    tint: [f32; 4],
}
impl ItemMesh {
    /// Returns true when visibility/detail changes require a fresh GPU upload.
    /// False permits GpuScene::update_vertices with these vertices/batch centers.
    /// Failure leaves the current scene unchanged.
    pub fn pose(
        &mut self,
        assets: &ItemAssets,
        transform: Mat4,
        sequence: Option<&str>,
        seconds: f32,
    ) -> Result<bool> {
        validate_transform(transform)?;
        let shape = assets.shape(&self.model)?;
        let pose = assets.pose(&self.model, sequence, seconds)?;
        let mut scratch = SceneData {
            materials: self.data.materials.clone(),
            ..Default::default()
        };
        let bindings: Vec<_> = (0..shape.materials.len()).collect();
        if let Some(detail) = visible_detail(shape) {
            scratch.append_shape(
                ShapeInstance {
                    shape,
                    pose: &pose,
                    detail,
                    transform,
                    materials: &bindings,
                    translucent_materials: None,
                    unassigned_material: bindings.len(),
                },
                |_| Some(self.tint),
            )?;
        }
        let changed = scratch.vertices.len() != self.data.vertices.len()
            || scratch.indices != self.data.indices
            || scratch.batches.len() != self.data.batches.len()
            || scratch
                .batches
                .iter()
                .zip(&self.data.batches)
                .any(|(a, b)| a.material != b.material || a.indices != b.indices);
        self.data.vertices = scratch.vertices;
        self.data.indices = scratch.indices;
        self.data.batches = scratch.batches;
        Ok(changed)
    }
}
fn visible_detail(shape: &Shape) -> Option<usize> {
    shape
        .details
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.collision)
        .max_by(|(_, a), (_, b)| a.pixel_threshold.total_cmp(&b.pixel_threshold))
        .map(|(i, _)| i)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn checked_read(root: &Path, file: &str, expected: &str, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid native SHA256"
    );
    let bytes = crate::materials::read_resource(root, file, limit)?;
    ensure!(
        hash(&bytes) == expected,
        "Item resource checksum mismatch: {file}"
    );
    Ok(bytes)
}
fn valid_tint(tint: [f32; 4]) -> bool {
    tint.iter().all(|v| v.is_finite() && (0. ..=1.).contains(v))
}
/// One native DTS-derived shape as a posed scene. Opaque materials act as
/// paint overlays (texture alpha over the tint), matching colorShift models.
///
/// `node_color` marks the tint as a v20 node colour (item/image colour shift).
/// Translucent materials then show the colour under the texture at the
/// colour's alpha, and opaque materials stay solid: `transspraycan.dts` flags
/// only its `blank` body translucent, so a clear colour gives a clear body
/// behind a solid label, rim and cap ridge.
pub fn native_shape_scene(
    model: &str,
    shape: &Shape,
    textures: &[&SceneImage],
    tint: [f32; 4],
    node_color: bool,
    transform: Mat4,
    pose: &Pose,
) -> Result<SceneData> {
    validate_transform(transform)?;
    ensure!(valid_tint(tint), "Invalid model tint");
    ensure!(
        textures.len() == shape.materials.len(),
        "Unbound native model material: {model}"
    );

        let mut scene = SceneData {
            id: model.into(),
            name: model.into(),
            ..Default::default()
        };
        if shape
            .meshes
            .iter()
            .flatten()
            .any(|m| m.billboard || m.billboard_y)
        {
            scene.omissions.push(format!("{model}: authored billboard flag retained; current generic posed geometry does not face the camera automatically"));
        }
        let mut bindings = Vec::new();
        let mut image_bindings = BTreeMap::new();
        for (source, texture) in shape.materials.iter().zip(textures) {
            let overlay = source.blend == "opaque" || (node_color && source.blend == "alpha");
            let key = (texture.label.clone(), overlay);
            let image = *image_bindings.entry(key).or_insert_with(|| {
                let mut image = (*texture).clone();
                image.srgb = !overlay;
                let index = scene.images.len();
                scene.images.push(image);
                index
            });
            let mut material = if overlay {
                Material::brick_overlay(format!("item/{model}/{}", source.name), image)
            } else {
                Material::vertex_lit(format!("item/{model}/{}", source.name), image)
            };
            if source.unlit {
                material.kind = if overlay {
                    MaterialKind::UnlitOverlay
                } else {
                    MaterialKind::Unlit
                };
            }
            material.alpha = match source.blend.as_str() {
                "opaque" if node_color || tint[3] >= 1. => AlphaMode::Opaque,
                "opaque" | "alpha" => AlphaMode::Blend,
                "additive" => AlphaMode::Additive,
                other => anyhow::bail!("Unsupported item blend {other}"),
            };
            if source.environment || source.bump_map.is_some() || source.detail_map.is_some() {
                scene.omissions.push(format!(
                    "{model}/{}: environment/bump/detail maps are not composed",
                    source.name
                ));
            }
            if !source.wrap_u || !source.wrap_v {
                scene.omissions.push(format!(
                    "{model}/{}: shared scene sampler repeats both axes",
                    source.name
                ));
            }
            bindings.push(scene.materials.len());
            scene.materials.push(material);
        }
        let fallback = scene.materials.len();
        scene
            .materials
            .push(Material::vertex_lit("Unassigned original model face", 0));
        if let Some(detail) = shape
            .details
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.collision)
            .max_by(|(_, a), (_, b)| a.pixel_threshold.total_cmp(&b.pixel_threshold))
            .map(|(i, _)| i)
        {
            scene.append_shape(
                ShapeInstance {
                    shape,
                    pose,
                    detail,
                    transform,
                    materials: &bindings,
                    translucent_materials: None,
                    unassigned_material: fallback,
                },
                |_| Some(tint),
            )?;
        } else {
            scene.omissions.push(format!(
                "{model}: authored native shape has no visible detail"
            ));
        }
        scene.validate()?;
        Ok(scene)
}

fn validate_transform(transform: Mat4) -> Result<()> {
    ensure!(
        transform.is_finite()
            && transform.determinant() > 1e-8
            && transform.x_axis.w == 0.
            && transform.y_axis.w == 0.
            && transform.z_axis.w == 0.
            && transform.w_axis.w == 1.,
        "Invalid item instance transform"
    );
    Ok(())
}
impl ItemAssets {
    pub fn mesh(&self, model: &str, tint: [f32; 4]) -> Result<ItemMesh> {
        Ok(ItemMesh {
            data: self.model_scene(model, tint, Mat4::IDENTITY, None, 0.)?,
            model: model.into(),
            tint,
        })
    }
    /// Both directories are native generated content. No source field is interpreted.
    pub fn load(root: &Path, weapons_root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let weapons_root = weapons_root.canonicalize()?;
        let bytes = crate::materials::read_resource(&root, "presentation.json", 8 * 1024 * 1024)?;
        let manifest: Presentation = serde_json::from_slice(&bytes)?;
        ensure!(
            manifest.schema_version == 2 && !manifest.id.is_empty(),
            "Unknown item presentation schema"
        );
        ensure!(
            manifest.models.len() <= 256
                && manifest.textures.len() <= 1024
                && manifest.items.len() <= 1024
                && manifest.images.len() <= 4096
                && manifest.projectiles.len() <= 4096,
            "Item definition budget exceeded"
        );
        let weapons = checked_read(
            &weapons_root,
            "weapons.json",
            &manifest.weapons_sha256,
            32 * 1024 * 1024,
        )?;
        let pack = bri_weapons::Pack::from_json(&weapons)?;
        ensure!(
            pack.items.keys().all(|id| manifest.items.contains_key(id))
                && pack
                    .images
                    .keys()
                    .all(|id| manifest.images.contains_key(id))
                && pack
                    .projectiles
                    .keys()
                    .all(|id| manifest.projectiles.contains_key(id)),
            "Presentation omits a weapon-pack identity"
        );
        for (id, item) in &pack.items {
            let native = &manifest.items[id];
            ensure!(
                native.model == item.model.to_ascii_lowercase() && native.image == item.image,
                "Item presentation identity mismatch: {id}"
            );
        }
        for (id, image) in &pack.images {
            let native = &manifest.images[id];
            ensure!(
                native.model == image.model.to_ascii_lowercase()
                    && native.mount_point == image.mount_point
                    && native.offset == image.offset
                    && native.eye_offset == image.eye_offset
                    && native.source_rotation_degrees == image.source_rotation_degrees,
                "Image presentation identity/transform mismatch: {id}"
            );
        }
        for (id, projectile) in &pack.projectiles {
            let native = &manifest.projectiles[id];
            ensure!(
                native.model.as_deref().unwrap_or("") == projectile.model.to_ascii_lowercase(),
                "Projectile model identity mismatch: {id}"
            );
        }
        for resource in &pack.resources {
            if resource.path.to_ascii_lowercase().ends_with(".dts") {
                let model = manifest
                    .models
                    .get(&resource.path.to_ascii_lowercase())
                    .context("Missing native model resource")?;
                ensure!(
                    model.source_sha256 == resource.sha256,
                    "Original/native model provenance mismatch"
                );
            }
        }
        for (id, model) in &manifest.models {
            model
                .bounds()
                .validate()
                .with_context(|| format!("Invalid authored model bounds: {id}"))?;
        }
        let physics_bytes = checked_read(
            &root,
            "item-physics.json",
            &manifest.item_physics_sha256,
            1024 * 1024,
        )?;
        let item_physics: ItemPhysicsCatalog = serde_json::from_slice(&physics_bytes)?;
        ensure!(
            item_physics.schema_version == 1,
            "Unknown item physics schema"
        );
        ensure!(
            item_physics.items.len() == manifest.items.len()
                && item_physics.items.keys().eq(manifest.items.keys()),
            "Item physics identities disagree with presentation"
        );
        for (id, bounds) in &item_physics.items {
            bounds.validate()?;
            let model = manifest
                .models
                .get(&manifest.items[id].model)
                .context("Missing item physics model")?;
            ensure!(
                bounds.min.map(f32::to_bits) == model.bounds_min.map(f32::to_bits)
                    && bounds.max.map(f32::to_bits) == model.bounds_max.map(f32::to_bits),
                "Item physics bounds disagree with authored model: {id}"
            );
        }
        let pixels = manifest.textures.values().try_fold(0u64, |total, t| {
            ensure!(
                t.width > 0 && t.height > 0 && t.width <= 4096 && t.height <= 4096,
                "Invalid item image dimensions"
            );
            let total = total + u64::from(t.width) * u64::from(t.height) * 4;
            ensure!(
                total <= 256 * 1024 * 1024,
                "Item aggregate image budget exceeded"
            );
            Ok(total)
        })?;
        let _ = pixels;
        let mut textures = BTreeMap::new();
        let mut input_bytes = 0usize;
        for (id, t) in &manifest.textures {
            let bytes = checked_read(&root, &t.file, &t.sha256, 16 * 1024 * 1024)?;
            input_bytes += bytes.len();
            ensure!(
                input_bytes <= 256 * 1024 * 1024,
                "Item aggregate input budget exceeded"
            );
            let reader =
                image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
            ensure!(
                reader.into_dimensions()? == (t.width, t.height),
                "Item image dimensions changed: {id}"
            );
            let rgba = image::load_from_memory(&bytes)?.to_rgba8().into_raw();
            textures.insert(
                id.clone(),
                SceneImage {
                    label: id.clone(),
                    width: t.width,
                    height: t.height,
                    rgba,
                    srgb: false,
                },
            );
        }
        let mut shapes = BTreeMap::new();
        let mut vertex_budget = 0usize;
        for (id, m) in &manifest.models {
            let bytes = checked_read(&root, &m.file, &m.sha256, 32 * 1024 * 1024)?;
            input_bytes += bytes.len();
            ensure!(
                input_bytes <= 256 * 1024 * 1024,
                "Item aggregate input budget exceeded"
            );
            let shape: Shape = serde_json::from_slice(&bytes)?;
            shape.validate()?;
            vertex_budget += shape
                .meshes
                .iter()
                .flatten()
                .map(|m| m.positions.len())
                .sum::<usize>();
            ensure!(vertex_budget <= 2_000_000, "Item geometry budget exceeded");
            ensure!(
                m.textures.len() == shape.materials.len()
                    && m.textures.iter().all(|id| textures.contains_key(id)),
                "Unbound item material: {id}"
            );
            shapes.insert(id.clone(), shape);
        }
        for (id, item) in &manifest.items {
            ensure!(
                shapes.contains_key(&item.model)
                    && manifest.images.contains_key(&item.image)
                    && valid_tint(item.tint)
                    && item.icon.as_ref().is_none_or(|i| textures.contains_key(i)),
                "Invalid item presentation: {id}"
            );
        }
        for (id, image) in &manifest.images {
            ensure!(
                (image.model.is_empty() || shapes.contains_key(&image.model))
                    && image.mount_point < 32
                    && valid_tint(image.tint)
                    && image
                        .offset
                        .iter()
                        .chain(&image.eye_offset)
                        .chain(&image.source_rotation_degrees)
                        .chain(&image.eye_rotation_degrees)
                        .all(|v| v.is_finite() && v.abs() <= 10000.),
                "Invalid mounted image: {id}"
            );
        }
        for (id, p) in &manifest.projectiles {
            ensure!(
                valid_tint(p.tint) && p.model.as_ref().is_none_or(|m| shapes.contains_key(m)),
                "Invalid projectile presentation: {id}"
            );
        }
        Ok(Self {
            presentation: manifest,
            item_physics,
            shapes,
            textures,
        })
    }
    pub fn icon(&self, item: &str) -> Result<Option<&SceneImage>> {
        let item = self
            .presentation
            .items
            .get(item)
            .context("Unknown item icon identity")?;
        Ok(item.icon.as_ref().map(|id| &self.textures[id]))
    }
    pub fn shape(&self, model: &str) -> Result<&Shape> {
        self.shapes.get(model).context("Unknown native item model")
    }
    pub fn pose(&self, model: &str, sequence: Option<&str>, seconds: f32) -> Result<Pose> {
        let shape = self.shape(model)?;
        let animation = sequence
            .filter(|name| !name.is_empty())
            .map(|name| {
                shape
                    .animations
                    .iter()
                    .find(|a| a.name.eq_ignore_ascii_case(name))
                    .with_context(|| format!("Absent native sequence {name} on {model}"))
            })
            .transpose()?;
        sample(shape, animation, seconds)
    }
    /// Build one scene. Use `mesh` + `ItemMesh::pose` to retain resource bindings
    /// while updating mounted/projectile instances without texture re-uploads.
    pub fn model_scene(
        &self,
        model: &str,
        tint: [f32; 4],
        transform: Mat4,
        sequence: Option<&str>,
        seconds: f32,
    ) -> Result<SceneData> {
        let shape = self.shape(model)?;
        let pose = self.pose(model, sequence, seconds)?;
        let textures: Vec<&SceneImage> = self.presentation.models[model]
            .textures
            .iter()
            .map(|id| &self.textures[id])
            .collect();
        native_shape_scene(model, shape, &textures, tint, true, transform, &pose)
    }
    pub fn item_scene(&self, id: &str, transform: Mat4) -> Result<SceneData> {
        let item = self
            .presentation
            .items
            .get(id)
            .context("Unknown dropped item")?;
        self.model_scene(&item.model, item.tint, transform, None, 0.)
    }
    pub fn image_scene(
        &self,
        id: &str,
        transform: Mat4,
        sequence: Option<&str>,
        seconds: f32,
    ) -> Result<SceneData> {
        validate_transform(transform)?;
        let image = self
            .presentation
            .images
            .get(id)
            .context("Unknown mounted image")?;
        if image.model.is_empty() {
            return Ok(SceneData {
                id: id.into(),
                name: "Authored model-less mounted image; host effects supply appearance".into(),
                ..Default::default()
            });
        }
        self.model_scene(&image.model, image.tint, transform, sequence, seconds)
    }
    pub fn projectile_scene(&self, id: &str, transform: Mat4) -> Result<SceneData> {
        validate_transform(transform)?;
        let projectile = self
            .presentation
            .projectiles
            .get(id)
            .context("Unknown projectile")?;
        if let Some(model) = &projectile.model {
            self.model_scene(model, projectile.tint, transform, None, 0.)
        } else {
            Ok(SceneData {
                id: id.into(),
                name: "Authored model-less projectile; host effects supply appearance".into(),
                ..Default::default()
            })
        }
    }
    /// Engine-family mount rule. Native axes are X-right/Y-up/-Z-forward.
    /// First-person uses a nonidentity eye offset; otherwise uses the actual
    /// requested host mount node. Akimbo mount1 is not a reflected mount0.
    pub fn mount_transform(
        &self,
        id: &str,
        first_person: bool,
        eye: Mat4,
        host_mount: impl Fn(u32) -> Option<Mat4>,
    ) -> Result<Mat4> {
        let image = self
            .presentation
            .images
            .get(id)
            .context("Unknown image mount")?;
        let eye_local = Mat4::from_rotation_translation(
            source_euler(image.eye_rotation_degrees),
            Vec3::from(image.eye_offset),
        );
        let transform = if first_person
            && (image.eye_offset != [0.; 3] || image.eye_rotation_degrees != [0.; 3])
        {
            eye * eye_local
        } else {
            let mount = host_mount(image.mount_point)
                .with_context(|| format!("Missing authored host mount{}", image.mount_point))?;
            let correction = if image.model.is_empty() {
                Mat4::IDENTITY
            } else {
                let shape = self.shape(&image.model)?;
                let bind = sample(shape, None, 0.)?;
                shape
                    .nodes
                    .iter()
                    .position(|n| n.name.eq_ignore_ascii_case("mountPoint"))
                    .map_or(Mat4::IDENTITY, |i| bind.nodes[i].inverse())
            };
            mount
                * Mat4::from_rotation_translation(
                    source_euler(image.source_rotation_degrees),
                    Vec3::from(image.offset),
                )
                * correction
        };
        ensure!(
            transform.is_finite() && transform.determinant() > 1e-8,
            "Invalid image mount transform"
        );
        Ok(transform)
    }
    pub fn node_transform(
        &self,
        model: &str,
        pose: &Pose,
        instance: Mat4,
        name: &str,
    ) -> Result<Mat4> {
        let shape = self.shape(model)?;
        ensure!(pose.nodes.len() == shape.nodes.len(), "Foreign item pose");
        let index = shape
            .nodes
            .iter()
            .position(|n| n.name.eq_ignore_ascii_case(name))
            .with_context(|| format!("Missing authored item node {name}"))?;
        let result = instance * pose.nodes[index];
        ensure!(result.is_finite(), "Invalid item node transform");
        Ok(result)
    }
}
/// Engine-family Euler composition Ry(-y)*Rx(-x)*Rz(-z), then native basis.
/// Recovered v20 eulerToMatrix calls MatrixCreateFromEuler. The matrix convention
/// is corroborated by pinned OpenMBG m_matF_set_euler_C, not a v20 engine build.
fn source_euler(degrees: [f32; 3]) -> Quat {
    let source = Quat::from_rotation_y(-degrees[1].to_radians())
        * Quat::from_rotation_x(-degrees[0].to_radians())
        * Quat::from_rotation_z(-degrees[2].to_radians());
    let basis = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    (basis * source * basis.conjugate()).normalize()
}

#[cfg(test)]
mod bounds_tests {
    use super::*;
    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
    #[test]
    #[ignore = "requires generated native item content; CPU only"]
    fn original_dts_header_bounds_survive_native_float_roundtrip() -> Result<()> {
        let root = root();
        let assets = ItemAssets::load(
            &root.join("content/item-presentation-pack-010"),
            &root.join("content/weapons-pack-009"),
        )?;
        assert_eq!(assets.item_physics.items.len(), 21);
        // Independently inspected source pistol.dts bytes116..139, SHA
        // 5d197fac2d28b7141939ff223dfd312dc42ee7826c9f7e85653806b600fc2856.
        let source_min = [0xbe2e2336, 0xbe590c31, 0xbefc8617].map(f32::from_bits);
        let source_max = [0x3e2e233a, 0x3f66e726, 0x3ec2abe4].map(f32::from_bits);
        // Transform all8 corners independently, rather than reusing emitter formula.
        let basis = Mat4::from_cols_array(&[
            1., 0., 0., 0., 0., 0., -1., 0., 0., 1., 0., 0., 0., 0., 0., 1.,
        ]);
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for bits in 0..8 {
            let source = Vec3::from_array(std::array::from_fn(|i| {
                if bits & (1 << i) == 0 {
                    source_min[i]
                } else {
                    source_max[i]
                }
            }));
            let native = basis.transform_point3(source);
            low = low.min(native);
            high = high.max(native);
        }
        let bounds = assets.item_physics.items["v20.weapon.gunitem"];
        assert_eq!(
            bounds.min.map(f32::to_bits),
            low.to_array().map(f32::to_bits)
        );
        assert_eq!(
            bounds.max.map(f32::to_bits),
            high.to_array().map(f32::to_bits)
        );
        assert_eq!(
            bounds,
            assets.item_physics.items["v20.weapon.akimbogunitem"]
        );
        let mut differing = 0;
        for (id, physics) in &assets.item_physics.items {
            physics.validate()?;
            let model = &assets.presentation.models[&assets.presentation.items[id].model];
            assert_eq!(*physics, model.bounds());
            // Authored model boxes are deliberately not overwritten by a mesh fit.
            let scene = assets.item_scene(id, Mat4::IDENTITY)?;
            let low = scene
                .vertices
                .iter()
                .fold(Vec3::splat(f32::INFINITY), |v, p| {
                    v.min(Vec3::from(p.position))
                });
            let high = scene
                .vertices
                .iter()
                .fold(Vec3::splat(f32::NEG_INFINITY), |v, p| {
                    v.max(Vec3::from(p.position))
                });
            if !low.abs_diff_eq(Vec3::from(physics.min), 0.0001)
                || !high.abs_diff_eq(Vec3::from(physics.max), 0.0001)
            {
                differing += 1;
            }
        }
        assert!(
            differing > 0,
            "Fixture must expose authored-box versus visible-mesh differences"
        );
        Ok(())
    }
    #[test]
    #[ignore = "requires generated native item content; CPU only"]
    fn translucent_spray_can_keeps_a_clear_colored_body_and_solid_trim() -> Result<()> {
        let root = root();
        let assets = ItemAssets::load(
            &root.join("content/item-presentation-pack-009"),
            &root.join("content/weapons-pack-008"),
        )?;
        let model = "base/data/shapes/transspraycan.dts";
        let scene = assets.model_scene(model, [0.2, 0.4, 1., 0.5], Mat4::IDENTITY, None, 0.)?;
        let material = |name: &str| {
            scene
                .materials
                .iter()
                .find(|m| m.name == format!("item/{model}/{name}"))
                .unwrap()
        };
        // Only `blank` is authored translucent: the colour shows under its
        // clear texture at the colour's alpha, and the trim stays solid.
        let body = material("blank");
        assert_eq!(body.kind, MaterialKind::BrickOverlay);
        assert_eq!(body.alpha, AlphaMode::Blend);
        for trim in ["spraycanLabel", "whiteCheck", "megaPhoneRidge"] {
            assert_eq!(material(trim).alpha, AlphaMode::Opaque, "{trim}");
        }
        Ok(())
    }
    #[test]
    #[ignore = "requires generated native item content; CPU only"]
    fn native_bounds_corruption_rejects_before_geometry_loading() -> Result<()> {
        let root = root();
        let source = root.join("content/item-presentation-pack-010");
        let base = root.join("artifacts/native-items");
        std::fs::create_dir_all(&base)?;
        let fixture = base.join(format!(
            "bounds-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir(&fixture)?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(source.join("presentation.json"))?)?;
        let physics: serde_json::Value =
            serde_json::from_slice(&std::fs::read(source.join("item-physics.json"))?)?;
        let weapons = root.join("content/weapons-pack-009");
        for (mode, expected) in [
            (0, "Invalid authored model bounds"),
            (1, "missing field"),
            (2, "bounds disagree"),
            (3, "identities disagree"),
            (4, "checksum mismatch"),
            (5, "Unknown item physics schema"),
            (6, "Unknown item presentation schema"),
        ] {
            let mut m = manifest.clone();
            let mut p = physics.clone();
            match mode {
                0 => m["models"]["add-ons/weapon_gun/pistol.dts"]["bounds_min"][0] = 100.into(),
                1 => {
                    m["models"]["add-ons/weapon_gun/pistol.dts"]
                        .as_object_mut()
                        .unwrap()
                        .remove("bounds_min");
                }
                2 => p["items"]["v20.weapon.gunitem"]["min"][0] = (-0.1).into(),
                3 => {
                    p["items"]
                        .as_object_mut()
                        .unwrap()
                        .remove("v20.weapon.gunitem");
                }
                4 => {}
                5 => p["schema_version"] = 2.into(),
                _ => m["schema_version"] = 1.into(),
            }
            let bytes = serde_json::to_vec(&p)?;
            m["item_physics_sha256"] = if mode == 4 {
                "0".repeat(64)
            } else {
                hash(&bytes)
            }
            .into();
            std::fs::write(fixture.join("item-physics.json"), bytes)?;
            std::fs::write(fixture.join("presentation.json"), serde_json::to_vec(&m)?)?;
            let error = match ItemAssets::load(&fixture, &weapons) {
                Ok(_) => anyhow::bail!("Accepted corrupt bounds mode{mode}"),
                Err(e) => format!("{e:#}"),
            };
            assert!(
                error.contains(expected),
                "Wrong failure for mode{mode}: {error}"
            );
        }
        std::fs::remove_dir_all(fixture)?;
        Ok(())
    }
}
