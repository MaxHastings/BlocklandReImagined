//! Native item presentation only. Gameplay identity/state stays in the host.
//! The offline assembler owns legacy fields; this module reads typed native data.
use anyhow::{Context, Result, ensure};
use bri_content::{
    animation::{Pose, sample},
    shape::{Animation, Shape},
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
    /// Add-On presentation replaced by a stand-in while loading
    /// (`crate::cosmetic::add_on_fault`).
    pub faults: Vec<String>,
    shapes: BTreeMap<String, Shape>,
    textures: BTreeMap<String, SceneImage>,
}
/// Resource bindings persist while the host updates only posed geometry.
pub struct ItemMesh {
    pub data: SceneData,
    /// The posed mesh's structure, for rewriting only positions.
    layout: Option<crate::avatar_mesh::Layout>,
    /// Draws the first-person `detail9999` mesh; see `visible_detail`.
    pub first_person: bool,
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
        let Some(detail) = visible_detail(shape, self.first_person) else {
            let changed = !self.data.vertices.is_empty();
            self.data.vertices.clear();
            self.data.indices.clear();
            self.data.batches.clear();
            self.layout = None;
            return Ok(changed);
        };
        // The same vertices `append_shape` builds, rewritten in place while
        // the drawn parts are unchanged (`avatar_mesh`).
        let bindings: Vec<_> = (0..shape.materials.len()).collect();
        let colors = vec![Some(self.tint); shape.objects.len()];
        let binding = crate::avatar_mesh::Binding {
            shape,
            detail,
            materials: &bindings,
            translucent_materials: &bindings,
            unassigned_material: bindings.len(),
            colors: &colors,
        };
        let before = crate::avatar_mesh::Layout::restructures(&self.layout, &self.data, &binding, &pose)?
            .then(|| {
                (
                    self.data.vertices.len(),
                    self.data.indices.clone(),
                    self.data
                        .batches
                        .iter()
                        .map(|b| (b.indices.clone(), b.material))
                        .collect::<Vec<_>>(),
                )
            });
        crate::avatar_mesh::Layout::pose(&mut self.layout, &mut self.data, &binding, &pose, transform)?;
        Ok(before.is_some_and(|(vertices, indices, batches)| {
            vertices != self.data.vertices.len()
                || indices != self.data.indices
                || batches.len() != self.data.batches.len()
                || batches
                    .iter()
                    .zip(&self.data.batches)
                    .any(|((range, material), b)| *range != b.indices || *material != b.material)
        }))
    }
}
/// Blockland's tools and weapons carry a `detail9999` mesh that only the
/// holder's first-person view reaches, and their `fire` sequences animate
/// only that mesh. Everyone else sees the held image at its ordinary detail,
/// which the swing leaves still (the arm's thread does the swinging).
const FIRST_PERSON_DETAIL: f32 = 9999.0;

fn visible_detail(shape: &Shape, first_person: bool) -> Option<usize> {
    let visible = || {
        shape
            .details
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.collision)
    };
    let largest = |details: &mut dyn Iterator<Item = (usize, &bri_content::shape::Detail)>| {
        details
            .max_by(|(_, a), (_, b)| a.pixel_threshold.total_cmp(&b.pixel_threshold))
            .map(|(i, _)| i)
    };
    if first_person {
        return largest(&mut visible());
    }
    largest(&mut visible().filter(|(_, d)| d.pixel_threshold < FIRST_PERSON_DETAIL))
        .or_else(|| largest(&mut visible()))
}
/// Whether `clip` changes what `model` draws at the detail a holder
/// (`first_person`) or anyone else sees: it moves a node one of that
/// detail's objects hangs from, or animates one of its objects. When it
/// does not, the image draws exactly its rest pose, and every holder can
/// share one posed copy.
pub fn moves_visible_detail(shape: &Shape, clip: &Animation, first_person: bool) -> bool {
    let Some(detail) = visible_detail(shape, first_person).and_then(|d| shape.details.get(d)) else {
        return false;
    };
    let objects = detail.object_start..detail.object_start + detail.object_count;
    if clip.objects.iter().any(|t| objects.contains(&t.object)) {
        return true;
    }
    let animated = |node: usize| {
        clip.nodes
            .iter()
            .any(|t| t.node.eq_ignore_ascii_case(&shape.nodes[node].name))
    };
    objects.filter_map(|i| shape.objects.get(i)?.node).any(|mut node| {
        // The node and every ancestor.
        for _ in 0..shape.nodes.len() {
            if animated(node) {
                return true;
            }
            match shape.nodes[node].parent {
                Some(parent) => node = parent,
                None => return false,
            }
        }
        true
    })
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
/// Metal detail that changes nothing: roughness as authored, no grime,
/// flat.
fn flat_detail() -> SceneImage {
    SceneImage {
        label: "flat metal detail".into(),
        width: 1,
        height: 1,
        rgba: vec![128, 255, 128, 128],
        srgb: false,
    }
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
            if let Some(metal) = &source.metal {
                // Bare metal: its texture tints the reflectance (colour),
                // its detail material's texture is data (linear).
                let mut bind = |image: &SceneImage, srgb: bool| {
                    *image_bindings
                        .entry((image.label.clone(), !srgb))
                        .or_insert_with(|| {
                            let mut image = image.clone();
                            image.srgb = srgb;
                            scene.images.push(image);
                            scene.images.len() - 1
                        })
                };
                let tint = bind(texture, true);
                let detail = match metal.detail {
                    Some(d) => bind(textures[d], false),
                    None => bind(&flat_detail(), false),
                };
                let mut material =
                    Material::vertex_lit(format!("item/{model}/{}", source.name), tint);
                material.kind = MaterialKind::Metal;
                material.images[1] = detail;
                material.parameters = Some([
                    [metal.roughness, metal.detail_scale, metal.detail_strength, 0.0],
                    [metal.color[0], metal.color[1], metal.color[2], 0.0],
                    [0.0; 4],
                    [0.0; 4],
                ]);
                bindings.push(scene.materials.len());
                scene.materials.push(material);
                continue;
            }
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
            layout: None,
            first_person: false,
            model: model.into(),
            tint,
        })
    }
    /// Both directories are native generated content. No source field is interpreted.
    pub fn load(root: &Path, weapons_root: &Path) -> Result<Self> {
        Self::load_with(root, weapons_root, &[])
    }
    /// [`Self::load`] plus the presentation other weapon packages provide in
    /// `assets/presentation.json` (see `content_identity::kind_providers`).
    /// Their models and textures are read from their own directories; a
    /// model key another package already provides is shared. Every item,
    /// image and projectile of their weapons packs is presented: a gap or a
    /// broken file becomes a stand-in listed in `faults`, never an error.
    pub fn load_with(
        root: &Path,
        weapons_root: &Path,
        extras: &[(String, std::path::PathBuf)],
    ) -> Result<Self> {
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
        let mut item_physics: ItemPhysicsCatalog = serde_json::from_slice(&physics_bytes)?;
        ensure!(
            item_physics.schema_version == 1,
            "Unknown item physics schema"
        );
        let mut manifest = manifest;
        euler_to_matrix_images(&mut manifest.images, &pack);
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
        // Add-Ons merge after the base game. Their presentation never stops
        // the load (`crate::cosmetic`): what is missing or broken falls back
        // to the stock art it names, then to no model and a letter icon.
        let mut added = Added::default();
        let mut faults = Vec::new();
        for (dir, abs) in extras {
            // Faults name the Add-On as players do, not its folder.
            let label = bri_package::library::add_on_label(abs, dir);
            let dir = &label;
            let abs = abs
                .canonicalize()
                .with_context(|| format!("Add-On {dir}: missing folder"))?;
            let weapons = crate::materials::read_resource(&abs, "weapons.json", 32 * 1024 * 1024)
                .with_context(|| format!("Add-On {dir}: weapons.json"))?;
            let part_pack = bri_weapons::Pack::from_json(&weapons)
                .with_context(|| format!("Add-On {dir}: weapons.json"))?;
            if abs.join("presentation.json").is_file() {
                match read_part(&abs, &weapons, &part_pack) {
                    Ok((part, physics)) => merge_part(
                        dir,
                        &abs,
                        part,
                        physics,
                        &mut manifest,
                        &mut item_physics,
                        &mut added,
                        &mut faults,
                    ),
                    Err(error) => faults.push(crate::cosmetic::add_on_fault(
                        dir,
                        "presentation.json",
                        format!("{error:#}"),
                    )),
                }
            }
            present_gaps(
                dir,
                &abs,
                &weapons,
                &part_pack,
                &mut manifest,
                &mut item_physics,
                &mut added,
                &mut faults,
            );
        }
        let file_root = |kind: &str, id: &str| added.origin.get(&format!("{kind}:{id}")).unwrap_or(&root).clone();
        let mut textures = BTreeMap::new();
        // Add-On textures replaced by a blank: models keep drawing, icons
        // fall back to the item's letter.
        let mut blanks = std::collections::BTreeSet::new();
        let mut pixels = 0u64;
        let mut input_bytes = 0usize;
        for (id, t) in &manifest.textures {
            let mut load = || -> Result<SceneImage> {
                ensure!(
                    t.width > 0 && t.height > 0 && t.width <= 4096 && t.height <= 4096,
                    "Invalid item image dimensions"
                );
                ensure!(
                    pixels + u64::from(t.width) * u64::from(t.height) * 4 <= 256 * 1024 * 1024,
                    "Item aggregate image budget exceeded"
                );
                let bytes = checked_read(&file_root("texture", id), &t.file, &t.sha256, 16 * 1024 * 1024)?;
                ensure!(
                    input_bytes + bytes.len() <= 256 * 1024 * 1024,
                    "Item aggregate input budget exceeded"
                );
                let reader =
                    image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
                ensure!(
                    reader.into_dimensions()? == (t.width, t.height),
                    "Item image dimensions changed: {id}"
                );
                let rgba = image::load_from_memory(&bytes)?.to_rgba8().into_raw();
                pixels += u64::from(t.width) * u64::from(t.height) * 4;
                input_bytes += bytes.len();
                Ok(SceneImage {
                    label: id.clone(),
                    width: t.width,
                    height: t.height,
                    rgba,
                    srgb: false,
                })
            };
            let image = match load() {
                Ok(image) => image,
                Err(error) if added.textures.contains(id) => {
                    faults.push(crate::cosmetic::add_on_fault(
                        &added.owner(&format!("texture:{id}")),
                        &t.file,
                        format!("{error:#}"),
                    ));
                    blanks.insert(id.clone());
                    blank_texture(id)
                }
                Err(error) => return Err(error),
            };
            textures.insert(id.clone(), image);
        }
        let mut shapes = BTreeMap::new();
        let mut vertex_budget = 0usize;
        for (id, m) in &manifest.models {
            let mut load = || -> Result<Shape> {
                let bytes = checked_read(&file_root("model", id), &m.file, &m.sha256, 32 * 1024 * 1024)?;
                ensure!(
                    input_bytes + bytes.len() <= 256 * 1024 * 1024,
                    "Item aggregate input budget exceeded"
                );
                let shape: Shape = serde_json::from_slice(&bytes)?;
                shape.validate()?;
                let vertices = shape
                    .meshes
                    .iter()
                    .flatten()
                    .map(|m| m.positions.len())
                    .sum::<usize>();
                ensure!(vertex_budget + vertices <= 2_000_000, "Item geometry budget exceeded");
                ensure!(
                    m.textures.len() == shape.materials.len()
                        && m.textures.iter().all(|id| textures.contains_key(id)),
                    "Unbound item material: {id}"
                );
                input_bytes += bytes.len();
                vertex_budget += vertices;
                Ok(shape)
            };
            match load() {
                Ok(shape) => {
                    shapes.insert(id.clone(), shape);
                }
                Err(error) if added.models.contains(id) => faults.push(crate::cosmetic::add_on_fault(
                    &added.owner(&format!("model:{id}")),
                    &m.file,
                    format!("{error:#}"),
                )),
                Err(error) => return Err(error),
            }
        }
        manifest.models.retain(|id, _| shapes.contains_key(id));
        // What Add-Ons present is repaired rather than refused: an unknown
        // model draws nothing, an invalid colour draws white, a missing icon
        // shows the item's letter (`crate::item_ui`).
        for (id, item) in manifest.items.iter_mut().filter(|(id, _)| added.items.contains(*id)) {
            let owner = added.owner(&format!("item:{id}"));
            if !item.model.is_empty() && !shapes.contains_key(&item.model) {
                if !added.models.contains(&item.model) {
                    faults.push(crate::cosmetic::add_on_fault(
                        &owner,
                        "presentation.json",
                        format!("item {id} names model {}, which it does not list", item.model),
                    ));
                }
                item.model.clear();
                item_physics.items.remove(id);
            }
            if !valid_tint(item.tint) {
                item.tint = [1.; 4];
            }
            if let Some(icon) = item.icon.clone().filter(|i| !textures.contains_key(i) || blanks.contains(i)) {
                if !blanks.contains(&icon) {
                    faults.push(crate::cosmetic::add_on_fault(
                        &owner,
                        "presentation.json",
                        format!("item {id} names icon {icon}, which it does not list"),
                    ));
                }
                item.icon = None;
            }
        }
        for (_, image) in manifest.images.iter_mut().filter(|(id, _)| added.images.contains(*id)) {
            if !shapes.contains_key(&image.model) {
                image.model.clear();
            }
            if !valid_tint(image.tint) {
                image.tint = [1.; 4];
            }
            image.mount_point = image.mount_point.min(31);
            for v in image
                .offset
                .iter_mut()
                .chain(&mut image.eye_offset)
                .chain(&mut image.source_rotation_degrees)
                .chain(&mut image.eye_rotation_degrees)
            {
                if !v.is_finite() || v.abs() > 10000. {
                    *v = 0.;
                }
            }
        }
        for (_, p) in manifest.projectiles.iter_mut().filter(|(id, _)| added.projectiles.contains(*id)) {
            if p.model.as_ref().is_some_and(|m| !shapes.contains_key(m)) {
                p.model = None;
            }
            if !valid_tint(p.tint) {
                p.tint = [1.; 4];
            }
        }
        for (id, item) in &manifest.items {
            ensure!(
                (item.model.is_empty() || shapes.contains_key(&item.model))
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
        let mut assets = Self {
            presentation: manifest,
            item_physics,
            faults,
            shapes,
            textures,
        };
        for (item, dir, file, spec) in std::mem::take(&mut added.icon_renders) {
            if let Err(error) = assets.render_icon(&item, &dir, &spec) {
                assets.faults.push(crate::cosmetic::add_on_fault(
                    &dir,
                    &file,
                    format!("the icon of {item} could not be drawn from its model, so it keeps its picture: {error:#}"),
                ));
            }
        }
        Ok(assets)
    }
    /// Draw `item`'s icon from its model, posed like `spec.pose_like`'s
    /// icon (`crate::item_icon_render`), and show it in place of any other.
    fn render_icon(&mut self, item: &str, dir: &str, spec: &crate::item_icon_render::Spec) -> Result<()> {
        use crate::item_icon_render::{Mesh, render_like};
        let mesh = |assets: &Self, model: &str| -> Result<Mesh> {
            ensure!(!model.is_empty(), "no model");
            Ok(Mesh::from_scene(&assets.model_scene(model, [1.; 4], Mat4::IDENTITY, None, 0.)?))
        };
        let own = &self.presentation.items[item];
        let model = mesh(self, &own.model).context("the item has no model")?;
        let stock = self
            .presentation
            .items
            .get(&spec.pose_like)
            .with_context(|| format!("{} is not an item", spec.pose_like))?;
        let icon = stock
            .icon
            .as_ref()
            .and_then(|i| self.textures.get(i))
            .with_context(|| format!("{} has no icon", spec.pose_like))?;
        let reference = mesh(self, &stock.model).with_context(|| format!("{} has no model", spec.pose_like))?;
        let key = format!("{dir}/{item}.render").to_ascii_lowercase();
        let image = render_like(spec, &model, (&reference, icon), &key)?;
        self.textures.insert(key.clone(), image);
        self.presentation.items.get_mut(item).unwrap().icon = Some(key);
        Ok(())
    }
    pub fn icon(&self, item: &str) -> Result<Option<&SceneImage>> {
        let item = self
            .presentation
            .items
            .get(item)
            .context("Unknown item icon identity")?;
        Ok(item.icon.as_ref().and_then(|id| self.textures.get(id)))
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
        if item.model.is_empty() {
            validate_transform(transform)?;
            return Ok(SceneData {
                id: id.into(),
                name: "Add-On item without a model".into(),
                ..Default::default()
            });
        }
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
        self.moved_mount_transform(id, first_person, eye, host_mount, |_| None)
    }
    /// `mount_transform`, with the arm's playing actions carried into the
    /// first-person eye offset: v20 moves a holder's eye-offset image with
    /// the arm's thread-2/3 animations (the held brick's shift, rotate and
    /// plant; a gun's recoil) as it moves in the hand. `mount_action` is how
    /// those actions move `Mount<n>` in its own frame
    /// (`AvatarMesh::mount_action`); the image takes the same motion in its
    /// own frame.
    pub fn moved_mount_transform(
        &self,
        id: &str,
        first_person: bool,
        eye: Mat4,
        host_mount: impl Fn(u32) -> Option<Mat4>,
        mount_action: impl Fn(u32) -> Option<Mat4>,
    ) -> Result<Mat4> {
        let image = self
            .presentation
            .images
            .get(id)
            .context("Unknown image mount")?;
        // The image in its mount's frame.
        let in_hand = || -> Result<Mat4> {
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
            Ok(Mat4::from_rotation_translation(
                source_euler(image.source_rotation_degrees),
                Vec3::from(image.offset),
            ) * correction)
        };
        let transform = if first_person
            && (image.eye_offset != [0.; 3] || image.eye_rotation_degrees != [0.; 3])
        {
            let eye_local = Mat4::from_rotation_translation(
                source_euler(image.eye_rotation_degrees),
                Vec3::from(image.eye_offset),
            );
            match mount_action(image.mount_point) {
                Some(action) => {
                    let hand = in_hand()?;
                    eye * eye_local * hand.inverse() * action * hand
                }
                None => eye * eye_local,
            }
        } else {
            let mount = host_mount(image.mount_point)
                .with_context(|| format!("Missing authored host mount{}", image.mount_point))?;
            mount * in_hand()?
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
/// What Add-Ons contributed to the merged presentation, so their faults are
/// repaired while the base game's stay errors, and where their files live.
#[derive(Default)]
struct Added {
    items: std::collections::BTreeSet<String>,
    images: std::collections::BTreeSet<String>,
    projectiles: std::collections::BTreeSet<String>,
    models: std::collections::BTreeSet<String>,
    textures: std::collections::BTreeSet<String>,
    /// `model:<id>` / `texture:<id>` to the Add-On folder holding the file.
    origin: BTreeMap<String, std::path::PathBuf>,
    /// The same keys to the Add-On's content directory, for fault lines.
    owners: BTreeMap<String, String>,
    /// Items whose icon is rendered from their model (`<icon>.render.json`):
    /// item, Add-On, the request's file and the request.
    icon_renders: Vec<(String, String, String, crate::item_icon_render::Spec)>,
}
impl Added {
    fn owner(&self, key: &str) -> String {
        self.owners.get(key).cloned().unwrap_or_default()
    }
}
/// A 1x1 white stand-in for an Add-On texture that does not load.
fn blank_texture(id: &str) -> SceneImage {
    SceneImage {
        label: id.into(),
        width: 1,
        height: 1,
        rgba: vec![255; 4],
        srgb: false,
    }
}
/// An Add-On's own `presentation.json` and `item-physics.json`, bound to
/// the `weapons.json` beside them.
fn read_part(
    abs: &Path,
    weapons: &[u8],
    pack: &bri_weapons::Pack,
) -> Result<(Presentation, ItemPhysicsCatalog)> {
    let bytes = crate::materials::read_resource(abs, "presentation.json", 8 * 1024 * 1024)?;
    let mut part: Presentation = serde_json::from_slice(&bytes)?;
    ensure!(part.schema_version == 2, "unknown item presentation schema");
    ensure!(
        part.weapons_sha256 == hash(weapons),
        "presentation.json does not match weapons.json; rerun the importer"
    );
    euler_to_matrix_images(&mut part.images, pack);
    let physics = checked_read(abs, "item-physics.json", &part.item_physics_sha256, 1024 * 1024)?;
    let physics: ItemPhysicsCatalog = serde_json::from_slice(&physics)?;
    ensure!(physics.schema_version == 1, "unknown item physics schema");
    Ok((part, physics))
}
/// Add one Add-On's presentation to the merged one. A model or texture key
/// already provided is shared; an item, image or projectile already
/// presented keeps its first presentation, as the weapons merge keeps the
/// first definition.
#[allow(clippy::too_many_arguments)]
fn merge_part(
    dir: &str,
    abs: &Path,
    part: Presentation,
    physics: ItemPhysicsCatalog,
    manifest: &mut Presentation,
    item_physics: &mut ItemPhysicsCatalog,
    added: &mut Added,
    faults: &mut Vec<String>,
) {
    for (key, model) in part.models {
        if let std::collections::btree_map::Entry::Vacant(e) = manifest.models.entry(key) {
            added.origin.insert(format!("model:{}", e.key()), abs.to_path_buf());
            added.owners.insert(format!("model:{}", e.key()), dir.to_string());
            added.models.insert(e.key().clone());
            e.insert(model);
        }
    }
    for (key, texture) in part.textures {
        if let std::collections::btree_map::Entry::Vacant(e) = manifest.textures.entry(key) {
            added.origin.insert(format!("texture:{}", e.key()), abs.to_path_buf());
            added.owners.insert(format!("texture:{}", e.key()), dir.to_string());
            added.textures.insert(e.key().clone());
            e.insert(texture);
        }
    }
    for (id, item) in part.items {
        if manifest.items.contains_key(&id) {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                "presentation.json",
                format!("item {id} is already presented by another Add-On"),
            ));
            continue;
        }
        match physics.items.get(&id) {
            Some(bounds) if bounds.validate().is_ok() => {
                item_physics.items.insert(id.clone(), *bounds);
            }
            _ => {}
        }
        added.items.insert(id.clone());
        added.owners.insert(format!("item:{id}"), dir.to_string());
        manifest.items.insert(id, item);
    }
    for (id, image) in part.images {
        if let std::collections::btree_map::Entry::Vacant(e) = manifest.images.entry(id) {
            added.images.insert(e.key().clone());
            e.insert(image);
        }
    }
    for (id, projectile) in part.projectiles {
        if let std::collections::btree_map::Entry::Vacant(e) = manifest.projectiles.entry(id) {
            added.projectiles.insert(e.key().clone());
            e.insert(projectile);
        }
    }
}
/// Present whatever an Add-On's weapons pack defines and its own
/// presentation does not (all of it when it has none, as the Duplicator's
/// wand): from the stock models and icons it names, else with no model.
/// An icon the base game lacks may be the Add-On's own PNG, named without
/// its extension relative to the folder holding `weapons.json` (the
/// Gravity Gun's `icons/gravity_gun`). Only a model or icon nothing
/// provides is logged; borrowing stock art is how an Add-On reuses it.
#[allow(clippy::too_many_arguments)]
fn present_gaps(
    dir: &str,
    abs: &Path,
    weapons: &[u8],
    pack: &bri_weapons::Pack,
    manifest: &mut Presentation,
    item_physics: &mut ItemPhysicsCatalog,
    added: &mut Added,
    faults: &mut Vec<String>,
) {
    let sha256 = hash(weapons);
    let evidence = || bri_weapons::Evidence {
        path: format!("{dir}/weapons.json"),
        sha256: sha256.clone(),
        line: 0,
    };
    // The model key `name` presents, or none (logged once per model).
    let mut missing = std::collections::BTreeSet::new();
    let mut model = |manifest: &Presentation, faults: &mut Vec<String>, name: &str| -> String {
        let model = name.replace('\\', "/").to_ascii_lowercase();
        if model.is_empty() || manifest.models.contains_key(&model) {
            return model;
        }
        if missing.insert(model.clone()) {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                "weapons.json",
                format!("model {name} is in neither this Add-On's presentation nor the base game"),
            ));
        }
        String::new()
    };
    let mut images = BTreeMap::new();
    for (id, image) in pack.images.iter().filter(|(id, _)| !manifest.images.contains_key(*id)) {
        images.insert(
            id.clone(),
            ImagePresentation {
                model: model(manifest, faults, &image.model),
                mount_point: image.mount_point,
                offset: image.offset,
                eye_offset: image.eye_offset,
                source_rotation_degrees: image.source_rotation_degrees,
                eye_rotation_degrees: image.eye_rotation,
                tint: image.color,
                evidence: evidence(),
            },
        );
    }
    euler_to_matrix_images(&mut images, pack);
    added.images.extend(images.keys().cloned());
    manifest.images.extend(images);
    for (id, projectile) in &pack.projectiles {
        if manifest.projectiles.contains_key(id) {
            continue;
        }
        let model = model(manifest, faults, &projectile.model);
        added.projectiles.insert(id.clone());
        manifest.projectiles.insert(
            id.clone(),
            ProjectilePresentation {
                model: (!model.is_empty()).then_some(model),
                tint: [1.0; 4],
            },
        );
    }
    for (id, item) in &pack.items {
        if manifest.items.contains_key(id) {
            continue;
        }
        let model = model(manifest, faults, &item.model);
        if let Some(stock) = manifest.models.get(&model) {
            item_physics.items.insert(id.clone(), stock.bounds());
        }
        let icon = format!("{}.png", item.icon.replace('\\', "/").to_ascii_lowercase());
        let icon = if manifest.textures.contains_key(&icon) {
            Some(icon)
        } else {
            own_icon(dir, abs, &item.icon, manifest, added, faults)
        };
        if let Some(request) = icon_render(dir, abs, &item.icon, faults) {
            added.icon_renders.push((id.clone(), dir.to_string(), request.0, request.1));
        }
        if icon.is_none() && !item.icon.is_empty() && !added.icon_renders.iter().any(|r| r.0 == *id) {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                "weapons.json",
                format!("icon {} of {} is not provided, so it shows its first letter", item.icon, item.ui_name.trim()),
            ));
        }
        added.items.insert(id.clone());
        manifest.items.insert(
            id.clone(),
            ItemPresentation {
                model,
                image: item.image.clone(),
                tint: [1.0; 4],
                icon,
                evidence: evidence(),
            },
        );
    }
}
/// An icon rendered from the item's model: `<name>.render.json` in `abs`
/// (`crate::item_icon_render`). None without one; one that does not read is
/// logged, and the item keeps its PNG or letter.
fn icon_render(
    dir: &str,
    abs: &Path,
    name: &str,
    faults: &mut Vec<String>,
) -> Option<(String, crate::item_icon_render::Spec)> {
    let file = format!("{}.render.json", name.replace('\\', "/"));
    if name.is_empty()
        || !bri_content::brick_materials::safe_relative(&file)
        || !abs.join(&file).is_file()
    {
        return None;
    }
    let read = || -> Result<crate::item_icon_render::Spec> {
        crate::item_icon_render::Spec::parse(&crate::materials::read_resource(abs, &file, 64 * 1024)?)
    };
    match read() {
        Ok(spec) => Some((file, spec)),
        Err(error) => {
            faults.push(crate::cosmetic::add_on_fault(dir, &file, format!("{error:#}")));
            None
        }
    }
}
/// An Add-On's own icon PNG, `<name>.png` in `abs`, added to the textures
/// under a key of its own (`<dir>/<name>.png`, so two Add-Ons' icons never
/// collide). None when there is no such file; a file that cannot be read
/// is logged and shows the letter instead.
fn own_icon(
    dir: &str,
    abs: &Path,
    name: &str,
    manifest: &mut Presentation,
    added: &mut Added,
    faults: &mut Vec<String>,
) -> Option<String> {
    let file = format!("{}.png", name.replace('\\', "/"));
    if name.is_empty()
        || !bri_content::brick_materials::safe_relative(&file)
        || !abs.join(&file).is_file()
    {
        return None;
    }
    let read = || -> Result<TextureResource> {
        let bytes = crate::materials::read_resource(abs, &file, ICON_BYTES)?;
        let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            (1..=ICON_SIDE).contains(&width) && (1..=ICON_SIDE).contains(&height),
            "icon {file} is {width}x{height}; at most {ICON_SIDE} a side"
        );
        Ok(TextureResource {
            file: file.clone(),
            sha256: hash(&bytes),
            width,
            height,
            source: format!("{dir}/{file}"),
        })
    };
    match read() {
        Ok(texture) => {
            let key = format!("{dir}/{file}").to_ascii_lowercase();
            added.origin.insert(format!("texture:{key}"), abs.to_path_buf());
            added.owners.insert(format!("texture:{key}"), dir.to_string());
            added.textures.insert(key.clone());
            manifest.textures.insert(key.clone(), texture);
            Some(key)
        }
        Err(error) => {
            faults.push(crate::cosmetic::add_on_fault(dir, &file, format!("{error:#}")));
            None
        }
    }
}
/// Largest Add-On icon file, and side.
const ICON_BYTES: u64 = 1024 * 1024;
const ICON_SIDE: u32 = 512;
/// Images whose `rotation` or `eyeRotation` is `eulerToMatrix(...)` turn by
/// the transpose of the stored Euler matrix (`bri_weapons::rotation`).
fn euler_to_matrix_images(
    images: &mut BTreeMap<String, ImagePresentation>,
    pack: &bri_weapons::Pack,
) {
    for (id, image) in images.iter_mut() {
        let Some(name) = pack.images.get(id).map(|i| i.name.as_str()) else {
            continue;
        };
        for (field, degrees) in [
            ("rotation", &mut image.source_rotation_degrees),
            ("eyeRotation", &mut image.eye_rotation_degrees),
        ] {
            if bri_weapons::rotation::is_euler_to_matrix(pack, name, field) {
                *degrees = bri_weapons::rotation::euler_to_matrix(*degrees);
            }
        }
    }
}
pub(crate) fn source_euler(degrees: [f32; 3]) -> Quat {
    bri_weapons::rotation::native(degrees)
}

#[cfg(test)]
mod add_on_icon_tests {
    use super::*;
    fn empty() -> Presentation {
        serde_json::from_value(serde_json::json!({
            "schema_version": 2, "id": "test", "weapons_sha256": "", "item_physics_sha256": "",
            "models": {}, "textures": {}, "items": {}, "images": {}, "projectiles": {},
            "diagnostics": []
        }))
        .unwrap()
    }
    /// Max, v0.1.9: the Gravity Gun borrowed the Printer's icon, so the two
    /// looked alike in the tool slots. An Add-On may ship its own icon.
    #[test]
    fn an_add_on_item_shows_its_own_icon() {
        let abs = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/gravity-gun-tool/assets")
            .canonicalize()
            .unwrap();
        let weapons = std::fs::read(abs.join("weapons.json")).unwrap();
        let pack = bri_weapons::Pack::from_json(&weapons).unwrap();
        let mut manifest = empty();
        let (mut added, mut faults) = (Added::default(), Vec::new());
        let mut physics = ItemPhysicsCatalog { schema_version: 1, items: BTreeMap::new() };
        present_gaps("Gravity Gun Tool", &abs, &weapons, &pack, &mut manifest, &mut physics, &mut added, &mut faults);
        let key = manifest.items["gravity-gun-tool:weapon/gravitygun"].icon.clone().unwrap();
        assert_eq!(key, "gravity gun tool/icons/gravity_gun.png");
        let texture = &manifest.textures[&key];
        assert_eq!((texture.width, texture.height), (128, 128));
        assert!(checked_read(&added.origin[&format!("texture:{key}")], &texture.file, &texture.sha256, ICON_BYTES).is_ok());
        assert!(!faults.iter().any(|f| f.contains("icon")), "{faults:?}");
        // Nothing there, or a path out of the Add-On: the letter, as before.
        for name in ["icons/missing", "../assets/icons/gravity_gun", ""] {
            assert!(own_icon("x", &abs, name, &mut empty(), &mut Added::default(), &mut Vec::new()).is_none(), "{name}");
        }
        // And it asks for its icon to be drawn from its model like the
        // Printer's, keeping the PNG for when that cannot be done.
        let [(item, _, file, spec)] = &added.icon_renders[..] else {
            panic!("one render request: {:?}", added.icon_renders);
        };
        assert_eq!((item.as_str(), file.as_str()), ("gravity-gun-tool:weapon/gravitygun", "icons/gravity_gun.render.json"));
        assert_eq!(spec.pose_like, bri_weapons::runtime::PRINTER);
    }
    /// Max, v0.1.9: "take the 3d model + shaders + snap pic -> make
    /// transparent background -> use as the icon just like the other
    /// tools". The Gravity Gun's icon is its in-game model with its skin,
    /// drawn at the Printer icon's angle and size. It is drawn on each
    /// player's machine from their game files, so none of it is shipped.
    /// Writes the icon to `target/gravity-gun-icon.png` for a look.
    #[test]
    #[ignore = "requires the converted item and weapons packs; CPU only"]
    fn the_gravity_gun_icon_is_drawn_from_its_model_like_the_printers() -> Result<()> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.join("../../content");
        let extras = vec![(
            "addons/gravity-gun-tool/assets".to_string(),
            manifest.join("../../packages/showcase/gravity-gun-tool/assets"),
        )];
        let started = std::time::Instant::now();
        let assets = ItemAssets::load_with(
            &root.join("item-presentation-pack-010"),
            &root.join("weapons-pack-009"),
            &extras,
        )?;
        let took = started.elapsed();
        assert!(!assets.faults.iter().any(|f| f.contains("icon")), "{:?}", assets.faults);
        let gun = "gravity-gun-tool:weapon/gravitygun";
        let key = assets.presentation.items[gun].icon.clone().unwrap();
        assert!(key.ends_with(".render"), "{key}");
        let icon = assets.icon(gun)?.unwrap();
        let printer = assets.icon(bri_weapons::runtime::PRINTER)?.unwrap();
        assert_eq!((icon.width, icon.height), (printer.width, printer.height), "framed like the Printer's");
        // Framed like the Printer: a clear border on every side, and the
        // drawing as wide or as tall as the Printer's (the models differ in
        // shape, so not both).
        let (gun_border, printer_border) = (crate::item_icon_render::clear_border(icon), crate::item_icon_render::clear_border(printer));
        let min = (icon.width.min(icon.height) as f32 * 0.05) as usize;
        assert!(gun_border.iter().all(|b| *b >= min), "clear border (top, right, bottom, left) {gun_border:?}");
        let span = |b: [usize; 4], w: u32, h: u32| (w as i32 - (b[1] + b[3]) as i32, h as i32 - (b[0] + b[2]) as i32);
        let (gw, gh) = span(gun_border, icon.width, icon.height);
        let (pw, ph) = span(printer_border, printer.width, printer.height);
        let near = |a: i32, b: i32| (a - b).abs() as f32 <= 0.12 * b as f32;
        assert!(near(gw, pw.min(icon.width as i32 * 88 / 100)) || near(gh, ph.min(icon.height as i32 * 88 / 100)),
            "gun {gw}x{gh} {gun_border:?}, printer {pw}x{ph} {printer_border:?}");
        assert_eq!(icon.rgba[3], 0, "a clear background");
        // Side by side with the Printer on a dark and a light slot, for a
        // look; target/ is never committed (the Printer icon is v20's).
        let (w, h) = (icon.width as usize, icon.height as usize);
        let mut sheet = vec![255u8; w * 4 * h * 4];
        for (k, (img, bg)) in [(icon, 40u8), (printer, 40), (icon, 215), (printer, 215)].into_iter().enumerate() {
            for y in 0..h {
                for x in 0..w {
                    let p = &img.rgba[(y * w + x) * 4..][..4];
                    let a = p[3] as f32 / 255.0;
                    let o = (y * w * 4 + k * w + x) * 4;
                    for c in 0..3 {
                        sheet[o + c] = (p[c] as f32 * a + bg as f32 * (1.0 - a)).round() as u8;
                    }
                }
            }
        }
        let side = manifest.join("../../target/gravity-gun-icon-vs-printer.png");
        image::save_buffer(&side, &sheet, (w * 4) as u32, h as u32, image::ColorType::Rgba8)?;
        let out = manifest.join("../../target/gravity-gun-icon.png");
        image::save_buffer(&out, &icon.rgba, icon.width, icon.height, image::ColorType::Rgba8)?;
        println!("drawn in {took:?} (whole item load); saved {} and {}", out.display(), side.display());
        Ok(())
    }
}

#[cfg(test)]
mod bounds_tests {
    use super::*;
    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
    #[test]
    #[ignore = "requires generated native item content; CPU only"]
    fn a_sequence_that_leaves_the_drawn_detail_still_draws_the_rest_pose() -> Result<()> {
        let root = root();
        let assets = ItemAssets::load(
            &root.join("content/item-presentation-pack-010"),
            &root.join("content/weapons-pack-009"),
        )?;
        let mut still = 0;
        for (model, shape) in &assets.shapes {
            for clip in &shape.animations {
                for first_person in [false, true] {
                    if moves_visible_detail(shape, clip, first_person) {
                        continue;
                    }
                    still += 1;
                    let posed = |sequence: Option<&str>, seconds: f32| -> Result<Vec<[f32; 3]>> {
                        let mut mesh = assets.mesh(model, [1.; 4])?;
                        mesh.first_person = first_person;
                        mesh.pose(&assets, Mat4::IDENTITY, sequence, seconds)?;
                        Ok(mesh
                            .data
                            .vertices
                            .iter()
                            .flat_map(|v| [v.position, v.normal])
                            .collect())
                    };
                    let rest = posed(None, 0.)?;
                    for t in [0., 0.3, 0.77] {
                        assert_eq!(
                            posed(Some(&clip.name), clip.duration * t)?,
                            rest,
                            "{model} {} first person {first_person}",
                            clip.name
                        );
                    }
                }
            }
        }
        assert!(still > 0, "no held-image sequence leaves its drawn detail still");
        Ok(())
    }
    #[test]
    #[ignore = "requires generated native item content; CPU only"]
    fn others_see_held_tools_at_their_third_person_detail() -> Result<()> {
        // The `fire` sequences swing only the first-person detail9999 mesh;
        // drawing that for other players doubled the arm's swing.
        let root = root();
        let assets = ItemAssets::load(
            &root.join("content/item-presentation-pack-010"),
            &root.join("content/weapons-pack-009"),
        )?;
        for image in ["v20.image.wrenchimage", "v20.image.hammerimage"] {
            let model = assets.presentation.images[image].model.clone();
            let shape = assets.shape(&model)?;
            let name = |detail: Option<usize>| detail.map(|d| shape.details[d].name.clone());
            assert_eq!(name(visible_detail(shape, true)).as_deref(), Some("detail9999"));
            assert_eq!(name(visible_detail(shape, false)).as_deref(), Some("detail32"));
            let posed = |first_person: bool, seconds: f32| -> Result<Vec<Vec3>> {
                let mut mesh = assets.mesh(&model, [1.; 4])?;
                mesh.first_person = first_person;
                mesh.pose(&assets, Mat4::IDENTITY, Some("fire"), seconds)?;
                Ok(mesh.data.vertices.iter().map(|v| Vec3::from(v.position)).collect())
            };
            let moved = |first_person| -> Result<f32> {
                let (rest, swung) = (posed(first_person, 0.)?, posed(first_person, 0.15)?);
                Ok(rest.iter().zip(&swung).map(|(a, b)| a.distance(*b)).fold(0., f32::max))
            };
            assert!(moved(true)? > 0.05, "{image}: first-person swing");
            assert!(moved(false)? < 1e-5, "{image}: others see the arm swing it");
        }
        Ok(())
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

#[cfg(test)]
mod metal_tests {
    use super::*;

    fn image(label: &str) -> SceneImage {
        SceneImage {
            label: label.into(),
            width: 1,
            height: 1,
            rgba: vec![200; 4],
            srgb: true,
        }
    }

    /// The Steel Kit's ball is bare metal: tint in slot 0 as colour, its
    /// detail material's texture in slot 1 as data, and a scene the
    /// renderer accepts.
    #[test]
    fn the_steel_ball_model_is_bare_metal_with_linear_detail() -> Result<()> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/steel-ball-kit/assets/models/steel-ball.shape.json");
        let shape: Shape = serde_json::from_slice(&std::fs::read(path)?)?;
        shape.validate()?;
        let (steel, detail) = (image("steel"), image("steel-detail"));
        let pose = bri_content::animation::sample(&shape, None, 0.0)?;
        let scene = native_shape_scene(
            "steelball",
            &shape,
            &[&steel, &detail],
            [1.0; 4],
            false,
            Mat4::IDENTITY,
            &pose,
        )?;
        scene.validate()?;
        let metal: Vec<_> = scene
            .materials
            .iter()
            .filter(|m| m.kind == MaterialKind::Metal)
            .collect();
        assert_eq!(metal.len(), 1, "one metal surface");
        let (tint, detail) = (&scene.images[metal[0].images[0]], &scene.images[metal[0].images[1]]);
        assert!(tint.label == "steel" && tint.srgb);
        assert!(detail.label == "steel-detail" && !detail.srgb);
        let parameters = metal[0].parameters.unwrap();
        assert!(parameters[0][0] > 0.0 && parameters[0][0] < 0.5, "polished");
        assert!(parameters[1][..3].iter().all(|c| *c > 0.5), "steel reflects most light");
        // Every triangle the ball draws is steel.
        assert!(scene.batches.iter().all(|b| scene.materials[b.material].kind == MaterialKind::Metal));
        Ok(())
    }
}
