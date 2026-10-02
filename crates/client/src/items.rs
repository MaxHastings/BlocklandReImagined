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
    /// In first person, an eye-offset image also moves with the arm's
    /// actions (`bri_weapons::Image::follow_arm`). The base game's tools do;
    /// an Add-On's image does only when it asks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub follow_arm: bool,
    /// The texture key of the scope picture drawn over the screen while
    /// aiming this image (`bri_weapons::Zoom::overlay`), found beside its
    /// weapons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<String>,
    /// A skin its Add-On gives it (`looks.json`), drawn over every copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<ItemSkin>,
}
/// A skin an Add-On gives one of its own items' images in `looks.json`:
/// the game draws it with the Add-On's shader over every copy of the item,
/// in a hand (first or third person), dropped, on a spawn brick and in a
/// mirror, so the item looks the same wherever it is (the Gravity Gun's
/// alien shell). The shader is an Add-On shader (`bri_client_sandbox::
/// shader`) and gets, per copy: `params[0]` the skin's colour and its
/// energy (1 in an `energy_states` state of the holder's image, else 0),
/// `params[1]` the direction sunlight travels and a seed for that copy,
/// `params[2]` the sun's colour, `params[3]` the ambient light.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ItemSkin {
    /// Its WGSL file, in the Add-On's folder.
    pub shader: String,
    pub color: [f32; 3],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub energy_states: Vec<String>,
}
/// An Add-On's `looks.json`: skins for its own images.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Looks {
    schema_version: u32,
    #[serde(default)]
    images: BTreeMap<String, ImageLook>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageLook {
    skin: Option<ItemSkin>,
}
/// A skin's shader, read from its Add-On.
#[derive(Clone, Debug, PartialEq)]
pub struct SkinShader {
    /// The file, as its Add-On names it (for messages).
    pub name: String,
    pub source: String,
    /// Sent by a server rather than installed: drawn only when the player
    /// trusts that server's code (`ClientCode::trusts_server`).
    pub downloaded: bool,
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
impl Presentation {
    /// The look of image `image` (a held item): its model, its colour and
    /// its skin (`ItemSkin`), named by the image.
    pub fn image_appearance(&self, id: &str) -> Option<Appearance> {
        let image = self.images.get(id)?;
        (!image.model.is_empty()).then(|| Appearance {
            model: image.model.clone(),
            tint: image.tint,
            skin: image.skin.is_some().then(|| id.to_string()),
        })
    }
    /// The look of item `item` lying in the world: the look of the image
    /// it is held as, so it is the same thing in the hand and on the
    /// ground. An item with no image to hold draws its own model.
    pub fn item_appearance(&self, item: &str) -> Option<Appearance> {
        let presented = self.items.get(item)?;
        self.image_appearance(&presented.image).or_else(|| {
            (!presented.model.is_empty()).then(|| Appearance {
                model: presented.model.clone(),
                tint: presented.tint,
                skin: None,
            })
        })
    }
}
pub struct ItemAssets {
    pub presentation: Presentation,
    pub item_physics: ItemPhysicsCatalog,
    /// Add-On presentation replaced by a stand-in while loading
    /// (`crate::cosmetic::add_on_fault`).
    pub faults: Vec<String>,
    shapes: BTreeMap<String, Shape>,
    textures: BTreeMap<String, SceneImage>,
    /// Icons to draw from their models, until [`Self::draw_icons`].
    icon_requests: Vec<(String, String, String, crate::item_icon_render::Request)>,
    /// Icons drawn from their models, each filled once drawn; until then
    /// the item shows its picture or letter.
    drawn: BTreeMap<String, DrawnIcon>,
    drawing: std::sync::Mutex<Vec<std::thread::JoinHandle<()>>>,
    /// Each skinned image's skin and shader (`ItemSkin`).
    skins: BTreeMap<String, (ItemSkin, SkinShader)>,
}
/// An icon drawn from its model (`crate::item_icon_render`), once it is.
pub type DrawnIcon = std::sync::Arc<std::sync::OnceLock<SceneImage>>;
/// What [`ItemAssets::draw_icons`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IconDraws {
    /// Found on disk, drawn before and shown at once.
    pub kept: usize,
    /// Being drawn on a thread of their own.
    pub drawing: usize,
}
/// What an item looks like, wherever it is drawn: in a hand, dropped, on
/// a spawn brick or as its icon. One item, one look (Max, v0.1.10: "my
/// gravity gun looks different on the item spawn than in my hand").
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub model: String,
    pub tint: [f32; 4],
    /// The image whose skin it wears (`ItemAssets::skin`), if it has one.
    pub skin: Option<String>,
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
        let before =
            crate::avatar_mesh::Layout::restructures(&self.layout, &self.data, &binding, &pose)?
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
        crate::avatar_mesh::Layout::pose(
            &mut self.layout,
            &mut self.data,
            &binding,
            &pose,
            transform,
        )?;
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
pub(crate) const FIRST_PERSON_DETAIL: f32 = 9999.0;

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
    let Some(detail) = visible_detail(shape, first_person).and_then(|d| shape.details.get(d))
    else {
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
    objects
        .filter_map(|i| shape.objects.get(i)?.node)
        .any(|mut node| {
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
            let mut material = Material::vertex_lit(format!("item/{model}/{}", source.name), tint);
            material.kind = MaterialKind::Metal;
            material.images[1] = detail;
            material.parameters = Some([
                [
                    metal.roughness,
                    metal.detail_scale,
                    metal.detail_strength,
                    0.0,
                ],
                [metal.color[0], metal.color[1], metal.color[2], 0.0],
                [0.0; 4],
                [0.0; 4],
            ]);
            bindings.push(scene.materials.len());
            scene.materials.push(material);
            continue;
        }
        // v20 lays a colour-shifted model's texture over its shift colour
        // (`GL_DECAL`) only when the texture has a translucent texel; a
        // texture with none is the texture times the light, untinted.
        let decal = translucent_texel(texture);
        let overlay =
            decal && (source.blend == "opaque" || (node_color && source.blend == "alpha"));
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
        material.untinted = !decal && source.blend == "opaque";
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

/// Whether v20 counts `image` as having a translucent texel, as its texture
/// manager does when it loads one (0x509d79-0x509dc2): every 16th texel
/// along each row and column, both walks bounded by the width, any alpha
/// under 255. Only such a texture is laid over a model's colour shift
/// (`GL_DECAL`, `TSMesh` 0x63ac1a); any other is the texture times the
/// light (`GL_MODULATE`, 0x63ac97).
pub(crate) fn translucent_texel(image: &SceneImage) -> bool {
    let (width, height) = (image.width as usize, image.height as usize);
    (0..width).step_by(16).any(|x| {
        (0..width.min(height)).step_by(16).any(|y| {
            image
                .rgba
                .get((y * width + x) * 4 + 3)
                .is_some_and(|a| *a < 255)
        })
    })
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
    /// The look of image `image` (a held item).
    pub fn image_appearance(&self, image: &str) -> Option<Appearance> {
        self.presentation.image_appearance(image)
    }
    /// Image `image`'s skin and its shader, if its Add-On gives it one.
    pub fn skin(&self, image: &str) -> Option<&(ItemSkin, SkinShader)> {
        self.skins.get(image)
    }
    /// Every skinned image, by image id.
    pub fn skins(&self) -> &BTreeMap<String, (ItemSkin, SkinShader)> {
        &self.skins
    }
    /// The look of item `item` lying in the world
    /// ([`Presentation::item_appearance`]).
    pub fn item_appearance(&self, item: &str) -> Option<Appearance> {
        self.presentation.item_appearance(item)
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
        // v20's own tools (the brick, hammer, wrench, spray cans) move with
        // the arm in first person; the base game says so for all of its
        // images, which changes only those with an eye offset.
        for image in manifest.images.values_mut() {
            image.follow_arm = true;
        }
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
        let mut skins = BTreeMap::new();
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
            scope_overlays(
                dir,
                &abs,
                &part_pack,
                &mut manifest,
                &mut added,
                &mut faults,
            );
            read_looks(
                dir,
                &abs,
                &part_pack,
                &mut manifest,
                &mut skins,
                &mut faults,
            );
        }
        let file_root = |kind: &str, id: &str| {
            added
                .origin
                .get(&format!("{kind}:{id}"))
                .unwrap_or(&root)
                .clone()
        };
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
                let bytes = checked_read(
                    &file_root("texture", id),
                    &t.file,
                    &t.sha256,
                    16 * 1024 * 1024,
                )?;
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
                let bytes = checked_read(
                    &file_root("model", id),
                    &m.file,
                    &m.sha256,
                    32 * 1024 * 1024,
                )?;
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
                ensure!(
                    vertex_budget + vertices <= 2_000_000,
                    "Item geometry budget exceeded"
                );
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
                Err(error) if added.models.contains(id) => {
                    faults.push(crate::cosmetic::add_on_fault(
                        &added.owner(&format!("model:{id}")),
                        &m.file,
                        format!("{error:#}"),
                    ))
                }
                Err(error) => return Err(error),
            }
        }
        manifest.models.retain(|id, _| shapes.contains_key(id));
        // What Add-Ons present is repaired rather than refused: an unknown
        // model draws nothing, an invalid colour draws white, a missing icon
        // shows the item's letter (`crate::item_ui`).
        for (id, item) in manifest
            .items
            .iter_mut()
            .filter(|(id, _)| added.items.contains(*id))
        {
            let owner = added.owner(&format!("item:{id}"));
            if !item.model.is_empty() && !shapes.contains_key(&item.model) {
                if !added.models.contains(&item.model) {
                    faults.push(crate::cosmetic::add_on_fault(
                        &owner,
                        "presentation.json",
                        format!(
                            "item {id} names model {}, which it does not list",
                            item.model
                        ),
                    ));
                }
                item.model.clear();
                item_physics.items.remove(id);
            }
            if !valid_tint(item.tint) {
                item.tint = [1.; 4];
            }
            if let Some(icon) = item
                .icon
                .clone()
                .filter(|i| !textures.contains_key(i) || blanks.contains(i))
            {
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
        for (_, image) in manifest
            .images
            .iter_mut()
            .filter(|(id, _)| added.images.contains(*id))
        {
            if !shapes.contains_key(&image.model) {
                image.model.clear();
            }
            // A scope picture that did not load leaves the plain zoom.
            if image
                .overlay
                .as_ref()
                .is_some_and(|o| !textures.contains_key(o) || blanks.contains(o))
            {
                image.overlay = None;
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
        for (_, p) in manifest
            .projectiles
            .iter_mut()
            .filter(|(id, _)| added.projectiles.contains(*id))
        {
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
                    // An item with no image (an ammo box) is picked up, not held.
                    && (item.image.is_empty() || manifest.images.contains_key(&item.image))
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
            icon_requests: Vec::new(),
            drawn: BTreeMap::new(),
            drawing: Default::default(),
            skins,
        };
        for (item, dir, file, spec) in std::mem::take(&mut added.icon_renders) {
            match assets.icon_request(&item, &dir, spec) {
                Ok(request) => assets.icon_requests.push((item, dir, file, request)),
                Err(error) => assets.faults.push(crate::cosmetic::add_on_fault(
                    &dir,
                    &file,
                    format!("the icon of {item} could not be drawn from its model, so it keeps its picture: {error:#}"),
                )),
            }
        }
        Ok(assets)
    }
    /// What `item`'s icon is drawn from: its model, posed like
    /// `spec.pose_like`'s icon (`crate::item_icon_render`).
    fn icon_request(
        &self,
        item: &str,
        dir: &str,
        spec: crate::item_icon_render::Spec,
    ) -> Result<crate::item_icon_render::Request> {
        use crate::item_icon_render::{Axes, Mesh, Request};
        let mesh = |assets: &Self, model: &str| -> Result<Mesh> {
            ensure!(!model.is_empty(), "no model");
            let mut mesh =
                Mesh::from_scene(&assets.model_scene(model, [1.; 4], Mat4::IDENTITY, None, 0.)?);
            // Pointing from where it is held to its muzzle, level (the
            // muzzle sits above the grip), when it has both.
            let pose = assets.pose(model, None, 0.)?;
            let node = |name| {
                assets
                    .node_transform(model, &pose, Mat4::IDENTITY, name)
                    .ok()
            };
            if let (Some(mount), Some(muzzle)) = (node("mountPoint"), node("muzzlePoint")) {
                let up = Axes::default().up;
                let along = muzzle.w_axis.truncate() - mount.w_axis.truncate();
                let level = along - up * along.dot(up);
                if level.length() > 1e-3 {
                    mesh.axes = Axes::new(level, up);
                }
            }
            Ok(mesh)
        };
        // Drawn as the item looks in play (`Self::item_appearance`): its
        // model, its colour and its skin's colour, unless the request
        // names its own.
        let look = self
            .item_appearance(item)
            .context("the item has no model")?;
        let veins = look
            .skin
            .as_deref()
            .and_then(|image| self.skin(image))
            .map(|(skin, _)| skin.color);
        let [r, g, b, _] = look.tint;
        let spec = spec.with_defaults([r, g, b], veins);
        let model = mesh(self, &look.model).context("the item has no model")?;
        ensure!(!model.indices.is_empty(), "the item has no model to draw");
        let stock = self
            .presentation
            .items
            .get(&spec.pose_like)
            .with_context(|| format!("{} is not an item", spec.pose_like))?;
        let icon = stock
            .icon
            .as_ref()
            .and_then(|i| self.textures.get(i))
            .with_context(|| format!("{} has no icon", spec.pose_like))?
            .clone();
        let reference =
            mesh(self, &stock.model).with_context(|| format!("{} has no model", spec.pose_like))?;
        Ok(Request {
            spec,
            mesh: model,
            reference,
            icon,
            label: format!("{dir}/{item}.render").to_ascii_lowercase(),
        })
    }
    /// Draw the icons Add-Ons ask to have drawn from their models. One kept
    /// in `cache` from an earlier drawing of the same request is shown at
    /// once; the others are drawn on threads of their own, off the load
    /// path, and kept there. Until one is drawn its item shows its picture
    /// or letter ([`Self::drawn_icon`] says when it is ready).
    pub fn draw_icons(&mut self, cache: Option<&Path>) -> IconDraws {
        let mut draws = IconDraws::default();
        // Icons still to draw, by the stock item they are posed like: its
        // pose is fitted once for them all, on one thread.
        let mut groups: BTreeMap<String, Vec<_>> = BTreeMap::new();
        for (item, dir, file, request) in std::mem::take(&mut self.icon_requests) {
            let slot = DrawnIcon::default();
            self.drawn.insert(item.clone(), slot.clone());
            if let Some(image) = cache.and_then(|cache| request.cached(cache)) {
                let _ = slot.set(image);
                draws.kept += 1;
                continue;
            }
            groups
                .entry(request.spec.pose_like.clone())
                .or_default()
                .push((item, dir, file, request, slot));
        }
        for (pose_like, group) in groups {
            let cache = cache.map(Path::to_path_buf);
            let count = group.len();
            let thread = std::thread::Builder::new().name("item icon".into()).spawn(move || {
                let fitted = group[0].3.fit();
                for (item, dir, file, request, slot) in group {
                    let drawn = fitted
                        .as_ref()
                        .with_context(|| format!("{pose_like}'s model does not match its icon"))
                        .and_then(|fitted| request.draw_fitted(fitted, cache.as_deref()));
                    match drawn {
                        Ok(image) => {
                            let _ = slot.set(image);
                        }
                        Err(error) => {
                            crate::cosmetic::add_on_fault(
                                &dir,
                                &file,
                                format!("the icon of {item} could not be drawn from its model, so it keeps its picture: {error:#}"),
                            );
                        }
                    }
                }
            });
            match thread {
                Ok(thread) => {
                    self.drawing
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(thread);
                    draws.drawing += count;
                }
                Err(error) => {
                    bri_console::warn(format!("Item icons: no thread to draw one: {error}"))
                }
            }
        }
        draws
    }
    /// Wait until every icon [`Self::draw_icons`] started is drawn (or
    /// could not be).
    pub fn finish_icons(&self) {
        let threads = std::mem::take(&mut *self.drawing.lock().unwrap_or_else(|e| e.into_inner()));
        for thread in threads {
            let _ = thread.join();
        }
    }
    /// `item`'s icon drawn from its model: empty until drawn.
    pub fn drawn_icon(&self, item: &str) -> Option<DrawnIcon> {
        self.drawn.get(item).cloned()
    }
    /// The item's icon: the one drawn from its model once it is, else its
    /// picture.
    pub fn icon(&self, id: &str) -> Result<Option<&SceneImage>> {
        let item = self
            .presentation
            .items
            .get(id)
            .context("Unknown item icon identity")?;
        let drawn = self.drawn.get(id).and_then(|slot| slot.get());
        Ok(drawn.or_else(|| item.icon.as_ref().and_then(|id| self.textures.get(id))))
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
    /// A decoded presentation texture by key (an Add-On's particle texture
    /// among them).
    pub fn texture(&self, key: &str) -> Option<&SceneImage> {
        self.textures.get(key)
    }
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
        // The image's `mountPoint` node, undone (Torque's mountTransform).
        let correction = || -> Result<Mat4> {
            if image.model.is_empty() {
                return Ok(Mat4::IDENTITY);
            }
            let shape = self.shape(&image.model)?;
            let bind = sample(shape, None, 0.)?;
            Ok(shape
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case("mountPoint"))
                .map_or(Mat4::IDENTITY, |i| bind.nodes[i].inverse()))
        };
        place_image(
            image,
            first_person,
            eye,
            correction,
            host_mount,
            mount_action,
        )
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
    for (id, image) in &mut part.images {
        image.follow_arm |= pack.images.get(id).is_some_and(|i| i.follow_arm);
    }
    let physics = checked_read(
        abs,
        "item-physics.json",
        &part.item_physics_sha256,
        1024 * 1024,
    )?;
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
            added
                .origin
                .insert(format!("model:{}", e.key()), abs.to_path_buf());
            added
                .owners
                .insert(format!("model:{}", e.key()), dir.to_string());
            added.models.insert(e.key().clone());
            e.insert(model);
        }
    }
    for (key, texture) in part.textures {
        if let std::collections::btree_map::Entry::Vacant(e) = manifest.textures.entry(key) {
            added
                .origin
                .insert(format!("texture:{}", e.key()), abs.to_path_buf());
            added
                .owners
                .insert(format!("texture:{}", e.key()), dir.to_string());
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
    // The Add-On's own models, by the name weapons.json gives them: the key
    // each is presented under, or none when it did not load (logged).
    let mut own = BTreeMap::new();
    let names = pack.items.values().map(|i| &i.model);
    let names = names.chain(pack.images.values().map(|i| &i.model));
    for name in names.chain(pack.projectiles.values().map(|p| &p.model)) {
        let lower = name.replace('\\', "/").to_ascii_lowercase();
        if !lower.ends_with(OWN_MODEL) || own.contains_key(&lower) {
            continue;
        }
        let key = own_model(dir, abs, name, manifest, added, faults);
        own.insert(lower, key);
    }
    // The model key `name` presents, or none (logged once per model).
    let mut missing = std::collections::BTreeSet::new();
    let mut model = |manifest: &Presentation, faults: &mut Vec<String>, name: &str| -> String {
        let model = name.replace('\\', "/").to_ascii_lowercase();
        if let Some(key) = own.get(&model) {
            return key.clone().unwrap_or_default();
        }
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
    for (id, image) in pack
        .images
        .iter()
        .filter(|(id, _)| !manifest.images.contains_key(*id))
    {
        images.insert(
            id.clone(),
            ImagePresentation {
                model: model(manifest, faults, &image.model),
                mount_point: image.mount_point,
                offset: image.offset,
                eye_offset: image.eye_offset,
                source_rotation_degrees: image.source_rotation_degrees,
                eye_rotation_degrees: image.eye_rotation,
                // As the stock importer does: the colour shows only when
                // the image shifts it (`doColorShift`).
                tint: if image.color_shift {
                    image.color
                } else {
                    [1.0; 4]
                },
                evidence: evidence(),
                follow_arm: image.follow_arm,
                overlay: None,
                skin: None,
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
            let name = imported_picture(pack, &item.icon).unwrap_or(&item.icon);
            own_icon(dir, abs, name, manifest, added, faults)
        };
        if let Some(request) = icon_render(dir, abs, &item.icon, faults) {
            added
                .icon_renders
                .push((id.clone(), dir.to_string(), request.0, request.1));
        }
        if icon.is_none() && !item.icon.is_empty() && !added.icon_renders.iter().any(|r| r.0 == *id)
        {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                "weapons.json",
                format!(
                    "icon {} of {} is not provided, so it shows its first letter",
                    item.icon,
                    item.ui_name.trim()
                ),
            ));
        }
        added.items.insert(id.clone());
        manifest.items.insert(
            id.clone(),
            ItemPresentation {
                model,
                image: item.image.clone(),
                // Its icon's tint in the tool slots. An Add-On's icon is
                // drawn in its own colours (a PNG, or drawn from the look),
                // so it is not tinted again; in the world the item takes
                // its image's look (`Presentation::item_appearance`).
                tint: [1.0; 4],
                icon,
                evidence: evidence(),
            },
        );
    }
}
/// Where a held image is drawn (Torque's `getRenderImageTransform`). In
/// first person an image with an eye offset sits at `eye x eyeOffset`, and
/// rides the arm's action at its mount too when it has `follow_arm`;
/// otherwise it sits in the hand: `mount x offset x rotation x correction`.
pub(crate) fn place_image(
    image: &ImagePresentation,
    first_person: bool,
    eye: Mat4,
    correction: impl Fn() -> Result<Mat4>,
    host_mount: impl Fn(u32) -> Option<Mat4>,
    mount_action: impl Fn(u32) -> Option<Mat4>,
) -> Result<Mat4> {
    // The image in its mount's frame.
    let in_hand = || -> Result<Mat4> {
        Ok(Mat4::from_rotation_translation(
            source_euler(image.source_rotation_degrees),
            Vec3::from(image.offset),
        ) * correction()?)
    };
    let transform =
        if first_person && (image.eye_offset != [0.; 3] || image.eye_rotation_degrees != [0.; 3]) {
            let eye_local = Mat4::from_rotation_translation(
                source_euler(image.eye_rotation_degrees),
                Vec3::from(image.eye_offset),
            );
            match mount_action(image.mount_point).filter(|_| image.follow_arm) {
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
/// Skins an Add-On gives its own images (`looks.json`, `ItemSkin`). One
/// that does not read, names an image the Add-On does not make, or whose
/// shader is missing or does not compile is a fault; the item then draws
/// without it.
fn read_looks(
    dir: &str,
    abs: &Path,
    pack: &bri_weapons::Pack,
    manifest: &mut Presentation,
    skins: &mut BTreeMap<String, (ItemSkin, SkinShader)>,
    faults: &mut Vec<String>,
) {
    if !abs.join("looks.json").is_file() {
        return;
    }
    let looks = crate::materials::read_resource(abs, "looks.json", 256 * 1024)
        .and_then(|bytes| Ok(serde_json::from_slice::<Looks>(&bytes)?));
    let looks = match looks {
        Ok(looks) if looks.schema_version == 1 => looks,
        Ok(looks) => {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                "looks.json",
                format!("unknown schema {}", looks.schema_version),
            ));
            return;
        }
        Err(error) => {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                "looks.json",
                format!("{error:#}"),
            ));
            return;
        }
    };
    for (id, look) in looks.images {
        let Some(skin) = look.skin else {
            continue;
        };
        let read = || -> Result<SkinShader> {
            ensure!(
                pack.images.contains_key(&id),
                "{id} is not one of this Add-On's images"
            );
            ensure!(
                skin.color
                    .iter()
                    .all(|c| c.is_finite() && (0.0..=1.0).contains(c)),
                "skin colours must be 0 to 1"
            );
            ensure!(skin.energy_states.len() <= 16, "at most 16 energy states");
            ensure!(
                bri_content::brick_materials::safe_relative(&skin.shader)
                    && skin.shader.ends_with(".wgsl"),
                "the skin's shader must be a .wgsl file in the Add-On"
            );
            let bytes = crate::materials::read_resource(abs, &skin.shader, 64 * 1024)?;
            let source = String::from_utf8(bytes).context("the skin's shader is not text")?;
            bri_client_sandbox::shader::compile(&skin.shader, &source)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(SkinShader {
                name: format!("{dir}/{}", skin.shader),
                source,
                downloaded: dir.starts_with(crate::client_code::DOWNLOADS),
            })
        };
        match read() {
            Ok(shader) => {
                if let Some(image) = manifest.images.get_mut(&id) {
                    image.skin = Some(skin.clone());
                }
                skins.insert(id, (skin, shader));
            }
            Err(error) => faults.push(crate::cosmetic::add_on_fault(
                dir,
                "looks.json",
                format!("{error:#}"),
            )),
        }
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
        crate::item_icon_render::Spec::parse(&crate::materials::read_resource(
            abs,
            &file,
            64 * 1024,
        )?)
    };
    match read() {
        Ok(spec) => Some((file, spec)),
        Err(error) => {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                &file,
                format!("{error:#}"),
            ));
            None
        }
    }
}
/// An Add-On's own icon PNG, `<name>.png` in `abs`, added to the textures
/// under a key of its own (`<dir>/<name>.png`, so two Add-Ons' icons never
/// collide). None when there is no such file; a file that cannot be read
/// is logged and shows the letter instead.
/// Where an imported Add-On keeps a picture its scripts name: the
/// importer copies `iconName`'s PNG to `textures/<hash>.png` and lists the
/// original path in `resources`. The name without `.png`, as [`own_icon`]
/// takes it.
fn imported_picture<'a>(pack: &'a bri_weapons::Pack, name: &str) -> Option<&'a str> {
    let path = format!("{}.png", name.replace('\\', "/"));
    pack.resources
        .iter()
        .find(|r| r.path.eq_ignore_ascii_case(&path))?
        .native_file
        .as_deref()?
        .strip_suffix(".png")
}
fn own_icon(
    dir: &str,
    abs: &Path,
    name: &str,
    manifest: &mut Presentation,
    added: &mut Added,
    faults: &mut Vec<String>,
) -> Option<String> {
    own_picture(
        dir,
        abs,
        name,
        ("icon", ICON_BYTES, ICON_SIDE),
        manifest,
        added,
        faults,
    )
}
/// Each of this Add-On's images with a scope picture (`Zoom::overlay`)
/// gets it as a texture of its own; one that does not read is logged and
/// the image aims with the plain zoom.
fn scope_overlays(
    dir: &str,
    abs: &Path,
    pack: &bri_weapons::Pack,
    manifest: &mut Presentation,
    added: &mut Added,
    faults: &mut Vec<String>,
) {
    for (id, image) in &pack.images {
        let Some(name) = image.zoom.as_ref().and_then(|z| z.overlay.as_deref()) else {
            continue;
        };
        if !added.images.contains(id) {
            continue;
        }
        let key = own_picture(
            dir,
            abs,
            name,
            ("scope overlay", OVERLAY_BYTES, OVERLAY_SIDE),
            manifest,
            added,
            faults,
        );
        if key.is_none() && !abs.join(format!("{name}.png")).is_file() {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                "weapons.json",
                format!(
                    "scope overlay {name}.png of {id} is not in the Add-On, so it aims without one"
                ),
            ));
        }
        if let Some(presented) = manifest.images.get_mut(id) {
            presented.overlay = key;
        }
    }
}
/// An Add-On's own PNG, `<name>.png` in `abs`, of at most `limits`' bytes
/// and side, added to the textures under a key of its own
/// (`<dir>/<name>.png`, so two Add-Ons' pictures never collide). None when
/// there is no such file; a file that cannot be read is logged.
fn own_picture(
    dir: &str,
    abs: &Path,
    name: &str,
    (what, max_bytes, max_side): (&str, u64, u32),
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
        let bytes = crate::materials::read_resource(abs, &file, max_bytes)?;
        let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            (1..=max_side).contains(&width) && (1..=max_side).contains(&height),
            "{what} {file} is {width}x{height}; at most {max_side} a side"
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
            added
                .origin
                .insert(format!("texture:{key}"), abs.to_path_buf());
            added
                .owners
                .insert(format!("texture:{key}"), dir.to_string());
            added.textures.insert(key.clone());
            manifest.textures.insert(key.clone(), texture);
            Some(key)
        }
        Err(error) => {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                &file,
                format!("{error:#}"),
            ));
            None
        }
    }
}
/// Largest Add-On icon file, and side.
const ICON_BYTES: u64 = 1024 * 1024;
const ICON_SIDE: u32 = 512;
/// Largest scope overlay file, and side: sharp on a 4K screen's height.
const OVERLAY_BYTES: u64 = 4 * 1024 * 1024;
const OVERLAY_SIDE: u32 = 2048;
/// An Add-On's own item model names a native model file (`bri_content::shape`).
const OWN_MODEL: &str = ".shape.json";
/// Largest Add-On model file, and model texture file and side.
const OWN_MODEL_BYTES: u64 = 8 * 1024 * 1024;
const OWN_TEXTURE_BYTES: u64 = 4 * 1024 * 1024;
const OWN_TEXTURE_SIDE: u32 = 1024;
/// An Add-On's own item model: `<name>.shape.json`, relative to the folder
/// holding `weapons.json`, a native model whose materials each name a PNG
/// beside it (`wood` draws `wood.png`), as vehicle models' do. It is keyed
/// under the Add-On (`<dir>/<name>`), so two Add-Ons' models never collide,
/// and its bounds are its vertices' box. None when it is missing or does
/// not read (logged): the item then draws no model.
fn own_model(
    dir: &str,
    abs: &Path,
    name: &str,
    manifest: &mut Presentation,
    added: &mut Added,
    faults: &mut Vec<String>,
) -> Option<String> {
    let file = name.replace('\\', "/");
    let key = format!("{dir}/{file}").to_ascii_lowercase();
    if manifest.models.contains_key(&key) {
        return Some(key);
    }
    let mut textures = Vec::new();
    let mut read = || -> Result<ModelResource> {
        ensure!(
            bri_content::brick_materials::safe_relative(&file),
            "model {file} must be a path inside the Add-On"
        );
        let bytes = crate::materials::read_resource(abs, &file, OWN_MODEL_BYTES)?;
        let shape: Shape = serde_json::from_slice(&bytes)?;
        shape.validate()?;
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in shape.meshes.iter().flatten().flat_map(|m| &m.positions) {
            lo = lo.min(Vec3::from(*p));
            hi = hi.max(Vec3::from(*p));
        }
        ensure!(lo.cmple(hi).all(), "model {file} has no vertices");
        let folder = file.rsplit_once('/').map_or("", |(folder, _)| folder);
        let mut bound = Vec::new();
        for material in &shape.materials {
            let png = match folder {
                "" => format!("{}.png", material.name),
                folder => format!("{folder}/{}.png", material.name),
            };
            let texture = own_texture(dir, abs, &png)
                .with_context(|| format!("material {} of {file}", material.name))?;
            let texture_key = format!("{dir}/{png}").to_ascii_lowercase();
            textures.push((texture_key.clone(), texture));
            bound.push(texture_key);
        }
        let sha256 = hash(&bytes);
        Ok(ModelResource {
            file: file.clone(),
            sha256: sha256.clone(),
            source: format!("{dir}/{file}"),
            source_sha256: sha256,
            textures: bound,
            bounds_min: lo.to_array(),
            bounds_max: hi.to_array(),
        })
    };
    match read() {
        Ok(model) => {
            for (texture_key, texture) in textures {
                added
                    .origin
                    .insert(format!("texture:{texture_key}"), abs.to_path_buf());
                added
                    .owners
                    .insert(format!("texture:{texture_key}"), dir.to_string());
                added.textures.insert(texture_key.clone());
                manifest.textures.entry(texture_key).or_insert(texture);
            }
            added
                .origin
                .insert(format!("model:{key}"), abs.to_path_buf());
            added.owners.insert(format!("model:{key}"), dir.to_string());
            added.models.insert(key.clone());
            manifest.models.insert(key.clone(), model);
            Some(key)
        }
        Err(error) => {
            faults.push(crate::cosmetic::add_on_fault(
                dir,
                &file,
                format!("{error:#}"),
            ));
            None
        }
    }
}
/// A PNG an Add-On's own model draws with.
fn own_texture(dir: &str, abs: &Path, file: &str) -> Result<TextureResource> {
    ensure!(
        bri_content::brick_materials::safe_relative(file),
        "texture {file} must be a path inside the Add-On"
    );
    let bytes = crate::materials::read_resource(abs, file, OWN_TEXTURE_BYTES)
        .with_context(|| format!("no texture {file}"))?;
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()?
        .into_dimensions()?;
    ensure!(
        (1..=OWN_TEXTURE_SIDE).contains(&width) && (1..=OWN_TEXTURE_SIDE).contains(&height),
        "texture {file} is {width}x{height}; at most {OWN_TEXTURE_SIDE} a side"
    );
    Ok(TextureResource {
        file: file.to_string(),
        sha256: hash(&bytes),
        width,
        height,
        source: format!("{dir}/{file}"),
    })
}
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
        let mut physics = ItemPhysicsCatalog {
            schema_version: 1,
            items: BTreeMap::new(),
        };
        present_gaps(
            "Gravity Gun Tool",
            &abs,
            &weapons,
            &pack,
            &mut manifest,
            &mut physics,
            &mut added,
            &mut faults,
        );
        let key = manifest.items["gravity-gun-tool:weapon/gravitygun"]
            .icon
            .clone()
            .unwrap();
        assert_eq!(key, "gravity gun tool/icons/gravity_gun.png");
        let texture = &manifest.textures[&key];
        assert_eq!((texture.width, texture.height), (128, 128));
        assert!(
            checked_read(
                &added.origin[&format!("texture:{key}")],
                &texture.file,
                &texture.sha256,
                ICON_BYTES
            )
            .is_ok()
        );
        assert!(!faults.iter().any(|f| f.contains("icon")), "{faults:?}");
        // Nothing there, or a path out of the Add-On: the letter, as before.
        for name in ["icons/missing", "../assets/icons/gravity_gun", ""] {
            assert!(
                own_icon(
                    "x",
                    &abs,
                    name,
                    &mut empty(),
                    &mut Added::default(),
                    &mut Vec::new()
                )
                .is_none(),
                "{name}"
            );
        }
        // And it asks for its icon to be drawn from its model like the
        // Printer's, keeping the PNG for when that cannot be done.
        let [(item, _, file, spec)] = &added.icon_renders[..] else {
            panic!("one render request: {:?}", added.icon_renders);
        };
        assert_eq!(
            (item.as_str(), file.as_str()),
            (
                "gravity-gun-tool:weapon/gravitygun",
                "icons/gravity_gun.render.json"
            )
        );
        assert_eq!(spec.pose_like, bri_weapons::runtime::PRINTER);
    }
    /// Max, v0.1.12: Fill Can's icon showed its first letter. The
    /// importer keeps `./icon_fillcan` as `textures/<hash>.png`, listed
    /// under its original path in `resources`.
    #[test]
    fn an_imported_add_on_item_shows_the_icon_it_shipped() {
        let dir = tempfile::tempdir().unwrap();
        let abs = dir.path();
        std::fs::create_dir_all(abs.join("textures")).unwrap();
        image::RgbaImage::new(32, 32)
            .save(abs.join("textures/348acc63.png"))
            .unwrap();
        let gravity = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/gravity-gun-tool/assets/weapons.json");
        let weapons = std::fs::read(gravity).unwrap();
        let mut pack = bri_weapons::Pack::from_json(&weapons).unwrap();
        let item = pack.items.values_mut().next().unwrap();
        item.icon = "Add-Ons/Tool_Fill_Can/icon_fillcan".into();
        let id = pack.items.keys().next().unwrap().clone();
        pack.resources.push(bri_weapons::Resource {
            path: "Add-Ons/Tool_Fill_Can/icon_fillcan.png".into(),
            sha256: String::new(),
            native_file: Some("textures/348acc63.png".into()),
            diagnostics: Vec::new(),
            package: None,
        });
        let mut manifest = empty();
        let (mut added, mut faults) = (Added::default(), Vec::new());
        let mut physics = ItemPhysicsCatalog {
            schema_version: 1,
            items: BTreeMap::new(),
        };
        present_gaps(
            "Fill Can",
            abs,
            &weapons,
            &pack,
            &mut manifest,
            &mut physics,
            &mut added,
            &mut faults,
        );
        assert_eq!(
            manifest.items[&id].icon.as_deref(),
            Some("fill can/textures/348acc63.png")
        );
        assert!(!faults.iter().any(|f| f.contains("icon")), "{faults:?}");
    }
    /// A stand-in tool Add-On with a model of its own: a wooden handle and
    /// an iron head (two materials, each naming its PNG), held at a
    /// `mountPoint` grip, with a first-person copy (`detail9999`) that a
    /// `fire` sequence swings; its icon is drawn from the model at the
    /// Hammer icon's pose. Written into `root`; returns its `assets/`.
    pub(crate) fn own_model_tool(root: &Path) -> Result<std::path::PathBuf> {
        use serde_json::json;
        let assets = root.join("assets");
        std::fs::create_dir_all(assets.join("models"))?;
        std::fs::create_dir_all(assets.join("icons"))?;
        std::fs::write(
            assets.join("icons/pick.render.json"),
            r#"{ "schema_version": 1, "pose_like": "v20.weapon.hammeritem", "look": { "textured": true } }"#,
        )?;
        // An axis-aligned box per part, faces outward and counter-clockwise.
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uv = Vec::new();
        let mut primitives = Vec::new();
        for (material, (lo, hi)) in [
            ([-0.05f32, -0.3, -0.05], [0.05f32, 0.8, 0.05]),
            ([-0.06, 0.7, -0.5], [0.06, 0.85, 0.4]),
        ]
        .into_iter()
        .enumerate()
        {
            let (lo, hi) = (Vec3::from(lo), Vec3::from(hi));
            let (c, h) = ((lo + hi) * 0.5, (hi - lo) * 0.5);
            let mut triangles = Vec::new();
            for (n, u, v) in [
                (Vec3::X, Vec3::Y, Vec3::Z),
                (Vec3::NEG_X, Vec3::Z, Vec3::Y),
                (Vec3::Y, Vec3::Z, Vec3::X),
                (Vec3::NEG_Y, Vec3::X, Vec3::Z),
                (Vec3::Z, Vec3::X, Vec3::Y),
                (Vec3::NEG_Z, Vec3::Y, Vec3::X),
            ] {
                let base = positions.len() as u32;
                for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    positions.push((c + (n + u * a + v * b) * h).to_array());
                    normals.push(n.to_array());
                    uv.push([(a + 1.0) * 0.5, (b + 1.0) * 0.5]);
                }
                triangles.extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
            }
            primitives.push(json!({ "material": material, "triangles": triangles }));
        }
        let mesh = json!({
            "frame_vertices": positions.len(), "positions": positions, "normals": normals, "uv": uv,
            "primitives": primitives, "skin": null, "billboard": false, "billboard_y": false,
        });
        let node = |name: &str, parent: Option<usize>| json!({ "name": name, "parent": parent, "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0] });
        let material = |name: &str| {
            json!({ "name": name, "wrap_u": true, "wrap_v": true, "blend": "opaque", "unlit": false,
                "environment": false, "mipmaps": true, "detail_map": null, "bump_map": null,
                "reflectance_map": null, "detail_scale": 1.0, "reflectance": 1.0 })
        };
        let swing: Vec<[f32; 4]> = [0.0f32, 0.4, -1.1, 0.0]
            .iter()
            .map(|a| Quat::from_rotation_x(*a).to_array())
            .collect();
        let shape = json!({
            "schema_version": 1, "id": "tool:file/models/tool.shape.json",
            "nodes": [node("root", None), node("mountPoint", Some(0)), node("swing", Some(0))],
            "objects": [
                { "name": "tool", "node": 0, "meshes": [0], "visibility": 1.0, "frame": 0, "material_frame": 0 },
                { "name": "toolheld", "node": 2, "meshes": [2, 1], "visibility": 1.0, "frame": 0, "material_frame": 0 },
            ],
            "details": [
                { "name": "detail32", "pixel_threshold": 32.0, "object_start": 0, "object_count": 1, "mesh_offset": 0, "collision": false },
                { "name": "detail9999", "pixel_threshold": 9999.0, "object_start": 1, "object_count": 1, "mesh_offset": 1, "collision": false },
            ],
            "meshes": [mesh.clone(), mesh, null],
            "materials": [material("handle"), material("head")],
            "animations": [{
                "name": "fire", "frames": swing.len(), "duration": 0.3, "looping": false, "additive": false,
                "priority": 0, "nodes": [{ "node": "swing", "rotations": swing, "translations": [], "scales": [], "scale_rotations": [] }],
                "objects": [], "ground_translations": [], "ground_rotations": [], "triggers": [],
            }],
        });
        std::fs::write(
            assets.join("models/tool.shape.json"),
            serde_json::to_vec(&shape)?,
        )?;
        for (name, rgb) in [("handle", [150u8, 100, 55]), ("head", [110, 112, 116])] {
            let pixels: Vec<u8> = (0..16)
                .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
                .collect();
            image::save_buffer(
                assets.join(format!("models/{name}.png")),
                &pixels,
                4,
                4,
                image::ColorType::Rgba8,
            )?;
        }
        let weapons = json!({
            "schema_version": 3, "id": "tool",
            "items": { "tool:weapon/pick": { "ui_name": "Pick", "image": "tool:image/pick",
                "model": "models/tool.shape.json", "icon": "icons/pick", "can_drop": false } },
            "images": { "tool:image/pick": { "name": "PickImage", "model": "models/tool.shape.json",
                "melee": true, "arm_ready": true, "color": [1.0, 1.0, 1.0, 1.0],
                "states": [{ "name": "Ready" }] } },
        });
        std::fs::write(
            assets.join("weapons.json"),
            serde_json::to_vec_pretty(&weapons)?,
        )?;
        Ok(assets)
    }
    /// Max, v0.1.10: "my gravity gun looks different on the item spawn
    /// than in my hand", and "when i drop the item or tool it can look
    /// different". An item in the world takes the look of the image it is
    /// held as: model, colour and skin.
    #[test]
    fn an_item_looks_the_same_in_the_hand_and_in_the_world() {
        let abs = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/gravity-gun-tool/assets")
            .canonicalize()
            .unwrap();
        let weapons = std::fs::read(abs.join("weapons.json")).unwrap();
        let pack = bri_weapons::Pack::from_json(&weapons).unwrap();
        let mut manifest = empty();
        let (mut added, mut faults, mut skins) = (Added::default(), Vec::new(), BTreeMap::new());
        let mut physics = ItemPhysicsCatalog {
            schema_version: 1,
            items: BTreeMap::new(),
        };
        present_gaps(
            "Gravity Gun Tool",
            &abs,
            &weapons,
            &pack,
            &mut manifest,
            &mut physics,
            &mut added,
            &mut faults,
        );
        // Without the base game here the Printer it borrows is a stand-in.
        assert!(
            faults.iter().all(|f| f.contains("printGun.dts")),
            "{faults:?}"
        );
        faults.clear();
        read_looks(
            "Gravity Gun Tool",
            &abs,
            &pack,
            &mut manifest,
            &mut skins,
            &mut faults,
        );
        assert!(faults.is_empty(), "{faults:?}");
        let (gun, image) = (
            "gravity-gun-tool:weapon/gravitygun",
            "gravity-gun-tool:image/gravitygun",
        );
        for printer in [
            &mut manifest.images.get_mut(image).unwrap().model,
            &mut manifest.items.get_mut(gun).unwrap().model,
        ] {
            if printer.is_empty() {
                *printer = "base/data/shapes/printgun.dts".into();
            }
        }
        let held = manifest.image_appearance(image).expect("held");
        assert_eq!(
            manifest.item_appearance(gun),
            Some(held.clone()),
            "on a spawn brick or dropped, as in the hand"
        );
        assert_eq!(held.tint, [0.35, 1.0, 0.8, 1.0], "the image's colour shift");
        assert_eq!(
            held.skin.as_deref(),
            Some(image),
            "its alien skin, wherever it is"
        );
        let (skin, shader) = &skins[image];
        assert_eq!(
            (skin.color, &skin.energy_states[..]),
            ([0.3, 0.95, 1.0], &["Grab".to_string()][..])
        );
        assert!(shader.source.contains("fn fs_main"));
        // An item whose own model and colour differ from its image's (the
        // stock packs record both) still looks like what is held.
        let mut manifest = empty();
        let evidence = bri_weapons::Evidence {
            path: String::new(),
            sha256: String::new(),
            line: 0,
        };
        manifest.images.insert(
            "image".into(),
            ImagePresentation {
                model: "held.dts".into(),
                mount_point: 0,
                offset: [0.; 3],
                eye_offset: [0.; 3],
                source_rotation_degrees: [0.; 3],
                eye_rotation_degrees: [0.; 3],
                tint: [0.2, 0.4, 0.6, 1.],
                evidence: evidence.clone(),
                follow_arm: false,
                overlay: None,
                skin: None,
            },
        );
        let item = |image: &str| ItemPresentation {
            model: "lying.dts".into(),
            image: image.into(),
            tint: [1.; 4],
            icon: None,
            evidence: evidence.clone(),
        };
        manifest.items.insert("item".into(), item("image"));
        manifest.items.insert("loose".into(), item(""));
        let look = manifest.item_appearance("item").unwrap();
        assert_eq!(Some(look.clone()), manifest.image_appearance("image"));
        assert_eq!(
            (look.model.as_str(), look.tint, look.skin),
            ("held.dts", [0.2, 0.4, 0.6, 1.], None)
        );
        // Nothing to hold: its own model.
        assert_eq!(
            manifest.item_appearance("loose").unwrap().model,
            "lying.dts"
        );
    }
    /// A `looks.json` that names another Add-On's image, a colour out of
    /// range, a shader outside the Add-On or one that does not compile is
    /// a fault, and the item draws without a skin.
    #[test]
    fn a_look_is_checked() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/showcase/gravity-gun-tool/assets");
        let weapons = std::fs::read(assets.join("weapons.json")).unwrap();
        let pack = bri_weapons::Pack::from_json(&weapons).unwrap();
        let image = "gravity-gun-tool:image/gravitygun";
        let shader = std::fs::read_to_string(assets.join("skins/alien.wgsl")).unwrap();
        let look = |image: &str, shader: &str, color: &str| {
            format!(
                r#"{{"schema_version": 1, "images": {{"{image}": {{"skin": {{"shader": "{shader}", "color": {color}}}}}}}}}"#
            )
        };
        let cases = [
            (
                "good",
                look(image, "skins/alien.wgsl", "[0.3, 0.95, 1]"),
                true,
            ),
            (
                "not its own image",
                look("v20.image.hammer", "skins/alien.wgsl", "[1, 1, 1]"),
                false,
            ),
            (
                "colour out of range",
                look(image, "skins/alien.wgsl", "[2, 1, 1]"),
                false,
            ),
            (
                "outside the Add-On",
                look(image, "../alien.wgsl", "[1, 1, 1]"),
                false,
            ),
            (
                "not WGSL",
                look(image, "skins/broken.wgsl", "[1, 1, 1]"),
                false,
            ),
            (
                "missing",
                look(image, "skins/missing.wgsl", "[1, 1, 1]"),
                false,
            ),
            (
                "unknown field",
                r#"{"schema_version": 1, "images": {}, "extra": 1}"#.to_string(),
                false,
            ),
            (
                "unknown schema",
                r#"{"schema_version": 2, "images": {}}"#.to_string(),
                false,
            ),
        ];
        for (what, json, good) in cases {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir(dir.path().join("skins")).unwrap();
            std::fs::write(dir.path().join("skins/alien.wgsl"), &shader).unwrap();
            std::fs::write(dir.path().join("skins/broken.wgsl"), "fn fs_main( {").unwrap();
            std::fs::write(dir.path().join("looks.json"), json).unwrap();
            let mut manifest = empty();
            let (mut added, mut faults, mut skins) =
                (Added::default(), Vec::new(), BTreeMap::new());
            let mut physics = ItemPhysicsCatalog {
                schema_version: 1,
                items: BTreeMap::new(),
            };
            present_gaps(
                "Test",
                &assets.canonicalize().unwrap(),
                &weapons,
                &pack,
                &mut manifest,
                &mut physics,
                &mut added,
                &mut faults,
            );
            faults.clear();
            read_looks(
                "Test",
                dir.path(),
                &pack,
                &mut manifest,
                &mut skins,
                &mut faults,
            );
            let skinned = manifest.images[image].skin.is_some();
            assert_eq!(
                (skinned, skins.contains_key(image), faults.is_empty()),
                (good, good, good),
                "{what}: {faults:?}"
            );
        }
    }
    /// For the record: every stock item whose own model or colour differs
    /// from what it is held as, which before one look per item drew
    /// differently on the ground than in the hand.
    #[test]
    #[ignore = "requires the converted item and weapons packs"]
    fn stock_items_that_looked_different_in_the_world() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let assets = ItemAssets::load(
            &root.join("item-presentation-pack-010"),
            &root.join("weapons-pack-009"),
        )?;
        let presentation = &assets.presentation;
        let mut differed = 0;
        for (id, item) in &presentation.items {
            let Some(image) = presentation.images.get(&item.image) else {
                continue;
            };
            if item.model != image.model || item.tint != image.tint {
                differed += 1;
                println!(
                    "{id}: lying {} {:?}, held {} {:?}",
                    item.model, item.tint, image.model, image.tint
                );
            }
            let look = presentation.item_appearance(id);
            assert_eq!(look, presentation.image_appearance(&item.image), "{id}");
        }
        println!(
            "{differed} of {} stock items differed",
            presentation.items.len()
        );
        Ok(())
    }
    /// An Add-On's item may be a model of its own, not a borrowed one: a
    /// native model beside its `weapons.json` whose materials name the PNGs
    /// beside it, presented under the Add-On with its box as its bounds. Its
    /// icon can be drawn from it in its own colours, and only the holder
    /// sees it swing. `BRI_ICON_SHOT=1` saves a picture to
    /// `target/own-model-preview.png`.
    #[test]
    fn an_add_on_item_brings_its_own_model() -> Result<()> {
        use crate::item_icon_render::{Look, Mesh, Pose, render};
        let dir = tempfile::tempdir()?;
        let abs = own_model_tool(dir.path())?.canonicalize()?;
        let weapons = std::fs::read(abs.join("weapons.json"))?;
        let pack = bri_weapons::Pack::from_json(&weapons)?;
        let mut manifest = empty();
        let (mut added, mut faults) = (Added::default(), Vec::new());
        let mut physics = ItemPhysicsCatalog {
            schema_version: 1,
            items: BTreeMap::new(),
        };
        present_gaps(
            "Tool",
            &abs,
            &weapons,
            &pack,
            &mut manifest,
            &mut physics,
            &mut added,
            &mut faults,
        );
        assert!(faults.is_empty(), "{faults:?}");
        let key = "tool/models/tool.shape.json";
        let (item, image) = ("tool:weapon/pick", "tool:image/pick");
        // Its icon is asked to be drawn from the model at the Hammer's pose.
        let [(drawn, _, file, spec)] = &added.icon_renders[..] else {
            panic!("one render request: {:?}", added.icon_renders);
        };
        assert_eq!(
            (drawn.as_str(), file.as_str()),
            (item, "icons/pick.render.json")
        );
        assert_eq!(spec.pose_like, "v20.weapon.hammeritem");
        assert_eq!(manifest.items[item].model, key);
        assert_eq!(manifest.images[image].model, key);
        let resource = &manifest.models[key];
        assert_eq!(physics.items[item].min, resource.bounds_min);
        assert_eq!(physics.items[item].max, resource.bounds_max);
        let tall = resource.bounds_max[1] - resource.bounds_min[1];
        let long = resource.bounds_max[2] - resource.bounds_min[2];
        assert!(
            tall > 1.0 && long > 0.8,
            "a handle and a head across it: {resource:?}"
        );
        let origin = &added.origin[&format!("model:{key}")];
        let shape: Shape = serde_json::from_slice(&checked_read(
            origin,
            &resource.file,
            &resource.sha256,
            OWN_MODEL_BYTES,
        )?)?;
        shape.validate()?;
        assert!(
            shape.nodes.iter().any(|n| n.name == "mountPoint"),
            "held at its grip"
        );
        let mut images = Vec::new();
        for texture in &resource.textures {
            let t = &manifest.textures[texture];
            let bytes = checked_read(
                &added.origin[&format!("texture:{texture}")],
                &t.file,
                &t.sha256,
                OWN_TEXTURE_BYTES,
            )?;
            let rgba = image::load_from_memory(&bytes)?.to_rgba8();
            images.push(SceneImage {
                label: texture.clone(),
                width: rgba.width(),
                height: rgba.height(),
                rgba: rgba.into_raw(),
                srgb: false,
            });
        }
        assert_eq!(images.len(), 2, "wood and iron");
        let refs: Vec<&SceneImage> = images.iter().collect();
        let scene = native_shape_scene(
            key,
            &shape,
            &refs,
            [1.0; 4],
            true,
            Mat4::IDENTITY,
            &sample(&shape, None, 0.0)?,
        )?;
        scene.validate()?;
        // Drawn side on, head up and to the right like the stock icons.
        let pose = Pose {
            rotation: Quat::from_rotation_z(-0.75)
                * Quat::from_rotation_x(0.25)
                * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
            scale: 80.0,
            centre: glam::Vec2::new(58.0, 76.0),
            size: [128, 128],
        };
        let look = Look {
            base: None,
            textured: true,
            skin: None,
        };
        let icon = render(&Mesh::from_scene(&scene), &pose, &look, "pick");
        if std::env::var_os("BRI_ICON_SHOT").is_some() {
            let out =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/own-model-preview.png");
            image::save_buffer(
                &out,
                &icon.rgba,
                icon.width,
                icon.height,
                image::ColorType::Rgba8,
            )?;
        }
        let solid: Vec<&[u8]> = icon.rgba.chunks_exact(4).filter(|p| p[3] == 255).collect();
        let wood = solid
            .iter()
            .filter(|p| p[0] as i32 - p[2] as i32 > 40)
            .count();
        let iron = solid
            .iter()
            .filter(|p| (p[0] as i32 - p[2] as i32).abs() < 12 && p[0] > 40)
            .count();
        assert!(
            wood > 150 && iron > 150,
            "wood {wood} and iron {iron} of {} pixels",
            solid.len()
        );
        assert_eq!(icon.rgba[3], 0, "a clear background");
        // The swing moves what the holder sees, never what others see.
        let fire = shape
            .animations
            .iter()
            .find(|a| a.name == "fire")
            .expect("a swing");
        assert!(moves_visible_detail(&shape, fire, true));
        assert!(!moves_visible_detail(&shape, fire, false));
        // A model that is not there, or outside the Add-On: no model, logged.
        for name in [
            "models/missing.shape.json",
            "../assets/models/tool.shape.json",
        ] {
            let mut faults = Vec::new();
            assert!(
                own_model(
                    "x",
                    &abs,
                    name,
                    &mut empty(),
                    &mut Added::default(),
                    &mut faults
                )
                .is_none(),
                "{name}"
            );
            assert_eq!(faults.len(), 1, "{faults:?}");
        }
        Ok(())
    }
    use super::fixture::Items;
    crate::testing::synthetic_and_content!(Items: the_gravity_gun_icon_is_drawn_from_its_model_like_the_printers);
    /// Max, v0.1.9: "take the 3d model + shaders + snap pic -> make
    /// transparent background -> use as the icon just like the other
    /// tools". The Gravity Gun's icon is its in-game model with its skin,
    /// drawn at the Printer icon's angle and size. It is drawn on each
    /// player's machine from their game files, so none of it is shipped.
    /// Writes the icon to `target/gravity-gun-icon.png` for a look.
    fn the_gravity_gun_icon_is_drawn_from_its_model_like_the_printers(fx: &Items) -> Result<()> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let extras = vec![(
            "addons/gravity-gun-tool/assets".to_string(),
            manifest.join("../../packages/showcase/gravity-gun-tool/assets"),
        )];
        let load = || ItemAssets::load_with(&fx.presentation, &fx.weapons, &extras);
        let cache = tempfile::tempdir()?;
        let gun = "gravity-gun-tool:weapon/gravitygun";
        // First run: nothing kept, so it is drawn on a thread of its own and
        // the load does not wait for it.
        let started = std::time::Instant::now();
        let mut assets = load()?;
        let loaded = started.elapsed();
        assert!(
            !assets.faults.iter().any(|f| f.contains("icon")),
            "{:?}",
            assets.faults
        );
        let started = std::time::Instant::now();
        assert_eq!(
            assets.draw_icons(Some(cache.path())),
            IconDraws {
                kept: 0,
                drawing: 1
            }
        );
        let started_drawing = started.elapsed();
        assets.finish_icons();
        let drawn = started.elapsed();
        // Next run: the same icon, read back from disk, ready at once.
        let started = std::time::Instant::now();
        let mut again = load()?;
        assert_eq!(
            again.draw_icons(Some(cache.path())),
            IconDraws {
                kept: 1,
                drawing: 0
            }
        );
        let reloaded = started.elapsed();
        assert_eq!(
            again.icon(gun)?.unwrap().rgba,
            assets.icon(gun)?.unwrap().rgba
        );
        println!(
            "item load without the icon {loaded:?}; starting its drawing {started_drawing:?}; drawing it {drawn:?}; \
             next load with it kept {reloaded:?}"
        );
        let slot = assets.drawn_icon(gun).expect("drawn");
        let icon = slot.get().expect("drawn");
        assert!(
            std::ptr::eq(assets.icon(gun)?.unwrap(), icon),
            "the drawn icon is shown"
        );
        let printer = assets.icon(bri_weapons::runtime::PRINTER)?.unwrap();
        assert_eq!(
            (icon.width, icon.height),
            (printer.width, printer.height),
            "framed like the Printer's"
        );
        // Framed like the Printer: a clear border on every side, and the
        // drawing as wide or as tall as the Printer's (the models differ in
        // shape, so not both).
        let (gun_border, printer_border) = (
            crate::item_icon_render::clear_border(icon),
            crate::item_icon_render::clear_border(printer),
        );
        let min = (icon.width.min(icon.height) as f32 * 0.05) as usize;
        assert!(
            gun_border.iter().all(|b| *b >= min),
            "clear border (top, right, bottom, left) {gun_border:?}"
        );
        let span = |b: [usize; 4], w: u32, h: u32| {
            (
                w as i32 - (b[1] + b[3]) as i32,
                h as i32 - (b[0] + b[2]) as i32,
            )
        };
        let (gw, gh) = span(gun_border, icon.width, icon.height);
        let (pw, ph) = span(printer_border, printer.width, printer.height);
        let near = |a: i32, b: i32| (a - b).abs() as f32 <= 0.12 * b as f32;
        assert!(
            near(gw, pw.min(icon.width as i32 * 88 / 100))
                || near(gh, ph.min(icon.height as i32 * 88 / 100)),
            "gun {gw}x{gh} {gun_border:?}, printer {pw}x{ph} {printer_border:?}"
        );
        assert_eq!(icon.rgba[3], 0, "a clear background");
        // Seen in the Printer icon's profile: the gun's own forward and up
        // point the same ways in the picture as the Printer's, up is up and
        // forward is across (Max, v0.1.10: "wrong perspective angle").
        let spec = crate::item_icon_render::Spec::parse(&std::fs::read(manifest.join(
            "../../packages/showcase/gravity-gun-tool/assets/icons/gravity_gun.render.json",
        ))?)?;
        let request = assets.icon_request(gun, "check", spec)?;
        // Coloured as the gun is in play: its image's tint, its skin's veins.
        let look = &request.spec.look;
        assert_eq!(look.base, Some([0.35, 1.0, 0.8]));
        assert_eq!(
            look.skin.as_ref().and_then(|s| s.veins),
            Some([0.3, 0.95, 1.0])
        );
        let (_, profile, overlap) =
            crate::item_icon_render::fit_pose(&request.reference, &request.icon).unwrap();
        let on_screen = |axes: crate::item_icon_render::Axes, axis: glam::Vec3| {
            (profile.rotation(axes) * axis)
                .truncate()
                .normalize_or_zero()
        };
        let (g, p) = (request.mesh.axes, request.reference.axes);
        println!(
            "Printer profile {profile:?}, outline overlap {overlap}, gun axes {g:?}, Printer axes {p:?}"
        );
        for (ours, theirs) in [
            (on_screen(g, g.forward), on_screen(p, p.forward)),
            (on_screen(g, g.up), on_screen(p, p.up)),
        ] {
            assert!(ours.dot(theirs) > 0.97, "gun {ours}, Printer {theirs}");
        }
        assert!(
            on_screen(g, g.up).y > 0.5 && on_screen(g, g.forward).x.abs() > 0.5,
            "side on"
        );
        // Side by side with the Printer's icon on a dark and a light slot,
        // for a look; target/ is never committed (the Printer is v20's).
        let (w, h) = (icon.width as usize, icon.height as usize);
        let shots = [(icon, 40u8), (printer, 40), (icon, 215), (printer, 215)];
        let mut sheet = vec![255u8; w * shots.len() * h * 4];
        for (k, (img, bg)) in shots.into_iter().enumerate() {
            for y in 0..h {
                for x in 0..w {
                    let p = &img.rgba[(y * w + x) * 4..][..4];
                    let a = p[3] as f32 / 255.0;
                    let o = (y * w * shots.len() + k * w + x) * 4;
                    for c in 0..3 {
                        sheet[o + c] = (p[c] as f32 * a + bg as f32 * (1.0 - a)).round() as u8;
                    }
                }
            }
        }
        let side = manifest.join("../../target/gravity-gun-icon-vs-printer.png");
        image::save_buffer(
            &side,
            &sheet,
            (w * shots.len()) as u32,
            h as u32,
            image::ColorType::Rgba8,
        )?;
        let out = manifest.join("../../target/gravity-gun-icon.png");
        image::save_buffer(
            &out,
            &icon.rgba,
            icon.width,
            icon.height,
            image::ColorType::Rgba8,
        )?;
        println!("saved {} and {}", out.display(), side.display());
        Ok(())
    }
}

#[cfg(test)]
mod own_model_icon_tests {
    use super::fixture::Items;
    use super::*;
    crate::testing::synthetic_and_content!(Items: an_add_on_tool_icon_is_drawn_from_its_model_like_the_hammers);
    /// A tool Add-On's icon is its own model in wood and iron, drawn at the
    /// Hammer icon's angle and size so it sits in the tool slots like a
    /// stock tool (the stand-in `own_model_tool`, as the Trench Pick was).
    /// Writes it to `target/own-model-icon.png` for a look.
    fn an_add_on_tool_icon_is_drawn_from_its_model_like_the_hammers(fx: &Items) -> Result<()> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dir = tempfile::tempdir()?;
        let extras = vec![(
            "addons/tool/assets".to_string(),
            add_on_icon_tests::own_model_tool(dir.path())?,
        )];
        let mut assets = ItemAssets::load_with(&fx.presentation, &fx.weapons, &extras)?;
        assert!(assets.faults.is_empty(), "{:?}", assets.faults);
        let pick = "tool:weapon/pick";
        // Drawn from its model, off the load path (`ItemAssets::draw_icons`).
        assert_eq!(
            assets.draw_icons(None),
            IconDraws {
                kept: 0,
                drawing: 1
            }
        );
        assets.finish_icons();
        let slot = assets
            .drawn_icon(pick)
            .expect("an icon drawn from its model");
        let icon = slot.get().expect("drawn");
        assert!(
            std::ptr::eq(assets.icon(pick)?.unwrap(), icon),
            "the drawn icon is shown"
        );
        let hammer = assets.icon(&fx.hammer_item)?.unwrap();
        assert_eq!(
            (icon.width, icon.height),
            (hammer.width, hammer.height),
            "framed like the Hammer's"
        );
        assert_eq!(icon.rgba[3], 0, "a clear background");
        // Two materials show: the wooden handle and the iron head.
        let solid: Vec<&[u8]> = icon.rgba.chunks_exact(4).filter(|p| p[3] == 255).collect();
        let wood = solid
            .iter()
            .filter(|p| p[0] as i32 - p[2] as i32 > 40)
            .count();
        let iron = solid
            .iter()
            .filter(|p| (p[0] as i32 - p[2] as i32).abs() < 12 && p[0] > 40)
            .count();
        assert!(wood > 50 && iron > 50, "wood {wood} and iron {iron}");
        // Held like the Hammer: at the grip, in the right hand.
        let image = &assets.presentation.images["tool:image/pick"];
        assert_eq!(
            image.mount_point,
            assets.presentation.images[&fx.hammer_image].mount_point
        );
        // Modelled as the Hammer is held: handle up out of the fist (+y)
        // and the head across its top, front to back (z), about as big.
        // Each model's box, seen from its grip.
        let grip_box = |model: &str| -> Result<(Vec3, Vec3)> {
            let shape = assets.shape(model)?;
            let bind = sample(shape, None, 0.0)?;
            let grip = shape
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case("mountPoint"))
                .map_or(Vec3::ZERO, |i| bind.nodes[i].w_axis.truncate());
            let bounds = assets.presentation.models[model].bounds();
            Ok((Vec3::from(bounds.min) - grip, Vec3::from(bounds.max) - grip))
        };
        let hammer = grip_box(&assets.presentation.images[&fx.hammer_image].model)?;
        let ours = grip_box(&image.model)?;
        println!("from the grip, hammer {hammer:?}, tool {ours:?}");
        for (name, (lo, hi)) in [("hammer", hammer), ("tool", ours)] {
            let size = hi - lo;
            assert!(
                size.y > size.x && hi.y > -lo.y * 2.0,
                "{name}: the handle stands up out of the fist"
            );
            assert!(size.z > size.x, "{name}: the head runs front to back");
        }
        let ratio = (ours.1.y - ours.0.y) / (hammer.1.y - hammer.0.y);
        assert!(
            (0.7..1.6).contains(&ratio),
            "about the Hammer's size: {ratio}"
        );
        let out = manifest.join("../../target/own-model-icon.png");
        image::save_buffer(
            &out,
            &icon.rgba,
            icon.width,
            icon.height,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod bounds_tests {
    use super::fixture::Items;
    use super::*;
    crate::testing::synthetic_and_content!(
        Items: a_sequence_that_leaves_the_drawn_detail_still_draws_the_rest_pose,
        others_see_held_tools_at_their_third_person_detail,
        item_physics_are_the_authored_model_boxes,
        translucent_spray_can_keeps_a_clear_colored_body_and_solid_trim,
        native_bounds_corruption_rejects_before_geometry_loading,
    );
    fn a_sequence_that_leaves_the_drawn_detail_still_draws_the_rest_pose(fx: &Items) -> Result<()> {
        let assets = fx.assets()?;
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
        assert!(
            still > 0,
            "no held-image sequence leaves its drawn detail still"
        );
        Ok(())
    }
    fn others_see_held_tools_at_their_third_person_detail(fx: &Items) -> Result<()> {
        // The `fire` sequences swing only the first-person detail9999 mesh;
        // drawing that for other players doubled the arm's swing.
        let assets = fx.assets()?;
        assert!(!fx.swung_images.is_empty());
        for image in &fx.swung_images {
            let image = image.as_str();
            let model = assets.presentation.images[image].model.clone();
            let shape = assets.shape(&model)?;
            let name = |detail: Option<usize>| detail.map(|d| shape.details[d].name.clone());
            assert_eq!(
                name(visible_detail(shape, true)),
                Some(fx.held_detail.clone())
            );
            assert_eq!(
                name(visible_detail(shape, false)),
                Some(fx.world_detail.clone())
            );
            let posed = |first_person: bool, seconds: f32| -> Result<Vec<Vec3>> {
                let mut mesh = assets.mesh(&model, [1.; 4])?;
                mesh.first_person = first_person;
                mesh.pose(&assets, Mat4::IDENTITY, Some("fire"), seconds)?;
                Ok(mesh
                    .data
                    .vertices
                    .iter()
                    .map(|v| Vec3::from(v.position))
                    .collect())
            };
            let moved = |first_person| -> Result<f32> {
                let (rest, swung) = (posed(first_person, 0.)?, posed(first_person, 0.15)?);
                Ok(rest
                    .iter()
                    .zip(&swung)
                    .map(|(a, b)| a.distance(*b))
                    .fold(0., f32::max))
            };
            assert!(moved(true)? > 0.05, "{image}: first-person swing");
            assert!(moved(false)? < 1e-5, "{image}: others see the arm swing it");
        }
        Ok(())
    }
    /// The real pistol's header box, read independently from its source
    /// bytes, and the real catalog's size.
    #[test]
    #[ignore = "requires generated v20 content"]
    fn original_dts_header_bounds_survive_native_float_roundtrip() -> Result<()> {
        let assets = Items::content()?.assets()?;
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
        Ok(())
    }
    /// Every item's physics box is its model's authored box, which is not
    /// overwritten by a fit of the drawn mesh.
    fn item_physics_are_the_authored_model_boxes(fx: &Items) -> Result<()> {
        let assets = fx.assets()?;
        assert!(!assets.item_physics.items.is_empty());
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
    fn translucent_spray_can_keeps_a_clear_colored_body_and_solid_trim(fx: &Items) -> Result<()> {
        let (presentation, weapons) = &fx.spray_packs;
        let assets = ItemAssets::load(presentation, weapons)?;
        let (model, body, trims) = &fx.clear_can;
        let model = model.as_str();
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
        let body = material(body);
        assert_eq!(body.kind, MaterialKind::BrickOverlay);
        assert_eq!(body.alpha, AlphaMode::Blend);
        for trim in trims {
            assert_eq!(material(trim).alpha, AlphaMode::Opaque, "{trim}");
        }
        Ok(())
    }
    fn native_bounds_corruption_rejects_before_geometry_loading(fx: &Items) -> Result<()> {
        let source = &fx.presentation;
        let scratch = crate::testing::ScratchDir::new("item-bounds")?;
        let fixture = scratch.path().to_path_buf();
        let (gun, pistol) = (&fx.gun.0, fx.gun.1.as_str());
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(source.join("presentation.json"))?)?;
        let physics: serde_json::Value =
            serde_json::from_slice(&std::fs::read(source.join("item-physics.json"))?)?;
        let weapons = &fx.weapons;
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
                0 => m["models"][pistol]["bounds_min"][0] = 100.into(),
                1 => {
                    m["models"][pistol]
                        .as_object_mut()
                        .unwrap()
                        .remove("bounds_min");
                }
                2 => p["items"][gun]["min"][0] = (-0.1).into(),
                3 => {
                    p["items"].as_object_mut().unwrap().remove(gun);
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
            let error = match ItemAssets::load(&fixture, weapons) {
                Ok(_) => anyhow::bail!("Accepted corrupt bounds mode{mode}"),
                Err(e) => format!("{e:#}"),
            };
            assert!(
                error.contains(expected),
                "Wrong failure for mode{mode}: {error}"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod placement_tests {
    use super::*;
    fn image(eye_offset: [f32; 3], eye_rotation: [f32; 3], follow_arm: bool) -> ImagePresentation {
        ImagePresentation {
            model: String::new(),
            mount_point: 0,
            offset: [0.1, -0.2, 0.05],
            eye_offset,
            source_rotation_degrees: [0., 90., 0.],
            eye_rotation_degrees: eye_rotation,
            tint: [1.; 4],
            evidence: bri_weapons::Evidence {
                path: String::new(),
                sha256: String::new(),
                line: 0,
            },
            follow_arm,
            overlay: None,
            skin: None,
        }
    }
    fn close(a: Mat4, b: Mat4) -> bool {
        a.abs_diff_eq(b, 1e-5)
    }
    /// An Add-On scope sits at eye x eyeOffset exactly while the arm plays
    /// an action (a cock or reload), so its sight stays on the line of
    /// sight; a v20 tool (`follow_arm`) rides the action; third person
    /// sits in the animated hand either way.
    #[test]
    fn first_person_eye_offset_images_sit_at_the_eye_unless_they_follow_the_arm() {
        let eye = Mat4::from_rotation_translation(
            Quat::from_rotation_y(0.7) * Quat::from_rotation_x(-0.3),
            Vec3::new(3., 41.6, -7.),
        );
        let hand =
            Mat4::from_rotation_translation(Quat::from_rotation_z(0.4), Vec3::new(0.3, 40.9, -7.2));
        let action =
            Mat4::from_rotation_translation(Quat::from_rotation_x(0.5), Vec3::new(0., -0.1, 0.2));
        let correction = || Ok(Mat4::from_translation(Vec3::new(0., 0., -0.3)));
        for (offset, rotation) in [
            ([0., -0.37, -0.64], [0.; 3]),
            ([0., 0., -0.45], [0.; 3]),
            ([0.02, -0.1, -1.1], [0., 0., 10.]),
        ] {
            let eye_local =
                Mat4::from_rotation_translation(source_euler(rotation), Vec3::from(offset));
            let scope = image(offset, rotation, false);
            let placed = place_image(
                &scope,
                true,
                eye,
                correction,
                |_| Some(hand),
                |_| Some(action),
            )
            .unwrap();
            assert!(close(placed, eye * eye_local), "{offset:?}");
            // The sight on the eye line stays on it: straight ahead of the eye.
            let sight = placed.transform_point3(Vec3::ZERO) - eye.transform_point3(Vec3::ZERO);
            let local = eye.inverse().transform_vector3(sight);
            assert!((local - Vec3::from(offset)).length() < 1e-4);

            let tool = image(offset, rotation, true);
            let placed = place_image(
                &tool,
                true,
                eye,
                correction,
                |_| Some(hand),
                |_| Some(action),
            )
            .unwrap();
            let in_hand = Mat4::from_rotation_translation(
                source_euler(tool.source_rotation_degrees),
                Vec3::from(tool.offset),
            ) * correction().unwrap();
            assert!(close(
                placed,
                eye * eye_local * in_hand.inverse() * action * in_hand
            ));
            // No action playing: the tool is at the eye too.
            let still =
                place_image(&tool, true, eye, correction, |_| Some(hand), |_| None).unwrap();
            assert!(close(still, eye * eye_local));

            for image in [&scope, &tool] {
                let third = place_image(
                    image,
                    false,
                    eye,
                    correction,
                    |_| Some(hand),
                    |_| Some(action),
                )
                .unwrap();
                assert!(close(third, hand * in_hand));
            }
        }
    }
    #[test]
    fn follow_arm_is_off_unless_an_image_asks() {
        let json = r#"{ "model": "", "mount_point": 0, "offset": [0,0,0], "eye_offset": [0,0,-1],
            "source_rotation_degrees": [0,0,0], "eye_rotation_degrees": [0,0,0], "tint": [1,1,1,1],
            "evidence": { "path": "", "sha256": "", "line": 0 } }"#;
        let image: ImagePresentation = serde_json::from_str(json).unwrap();
        assert!(!image.follow_arm);
        let pack: bri_weapons::Image = serde_json::from_value(serde_json::json!({
            "id": "x:image/a", "name": "a", "model": "", "mount_point": 0,
            "offset": [0,0,0], "eye_offset": [0,0,0], "source_rotation_degrees": [0,0,0],
            "correct_muzzle": false, "melee": false, "color": [1,1,1,1], "color_shift": false,
            "arm_ready": false, "casing": "", "min_shot_ticks": 0, "states": [],
            "eye_rotation": [0,0,0]
        }))
        .unwrap();
        assert!(!pack.follow_arm);
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
        let (tint, detail) = (
            &scene.images[metal[0].images[0]],
            &scene.images[metal[0].images[1]],
        );
        assert!(tint.label == "steel" && tint.srgb);
        assert!(detail.label == "steel-detail" && !detail.srgb);
        let parameters = metal[0].parameters.unwrap();
        assert!(parameters[0][0] > 0.0 && parameters[0][0] < 0.5, "polished");
        assert!(
            parameters[1][..3].iter().all(|c| *c > 0.5),
            "steel reflects most light"
        );
        // Every triangle the ball draws is steel.
        assert!(
            scene
                .batches
                .iter()
                .all(|b| scene.materials[b.material].kind == MaterialKind::Metal)
        );
        Ok(())
    }
}

/// The item packs a test runs on: made up (`crate::testing::items`) or the
/// generated v20 ones, with the roles the tests look up.
#[cfg(test)]
pub(crate) mod fixture {
    use super::*;
    use crate::testing::items as made_up;
    use std::path::PathBuf;

    pub(crate) struct Items {
        /// The folders holding `presentation.json` and `weapons.json`.
        pub presentation: PathBuf,
        pub weapons: PathBuf,
        /// Images whose `fire` sequence swings only the held detail, and
        /// the names of the held and the third-person details.
        pub swung_images: Vec<String>,
        pub held_detail: String,
        pub world_detail: String,
        /// The hammer item and image.
        pub hammer_item: String,
        pub hammer_image: String,
        /// The packs holding the translucent spray can: its model, the
        /// translucent material and the solid trim.
        pub spray_packs: (PathBuf, PathBuf),
        pub clear_can: (String, String, Vec<String>),
        /// The gun item and its model.
        pub gun: (String, String),
        _packs: Option<made_up::ItemPacks>,
    }
    impl Items {
        pub fn synthetic() -> Result<Self> {
            let packs = made_up::write_packs()?;
            let strings = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            Ok(Self {
                presentation: packs.presentation.clone(),
                weapons: packs.weapons.clone(),
                swung_images: strings(&made_up::SWUNG_IMAGES),
                held_detail: made_up::HELD_DETAIL.into(),
                world_detail: made_up::WORLD_DETAIL.into(),
                hammer_item: bri_weapons::testing::HAMMER.into(),
                hammer_image: bri_weapons::testing::HAMMER_IMAGE.into(),
                spray_packs: (packs.presentation.clone(), packs.weapons.clone()),
                clear_can: (
                    made_up::CLEAR_CAN_MODEL.into(),
                    made_up::CLEAR_CAN_BODY.into(),
                    strings(&made_up::CLEAR_CAN_TRIM),
                ),
                gun: (
                    bri_weapons::testing::GUN_ITEM.into(),
                    made_up::GUN_MODEL.into(),
                ),
                _packs: Some(packs),
            })
        }
        pub fn content() -> Result<Self> {
            let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
            let strings = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            Ok(Self {
                presentation: content.join("item-presentation-pack-010"),
                weapons: content.join("weapons-pack-009"),
                swung_images: strings(&["v20.image.wrenchimage", "v20.image.hammerimage"]),
                held_detail: "detail9999".into(),
                world_detail: "detail32".into(),
                hammer_item: "v20.weapon.hammeritem".into(),
                hammer_image: "v20.image.hammerimage".into(),
                spray_packs: (
                    content.join("item-presentation-pack-010"),
                    content.join("weapons-pack-009"),
                ),
                clear_can: (
                    "base/data/shapes/transspraycan.dts".into(),
                    "blank".into(),
                    strings(&["spraycanLabel", "whiteCheck", "megaPhoneRidge"]),
                ),
                gun: (
                    "v20.weapon.gunitem".into(),
                    "add-ons/weapon_gun/pistol.dts".into(),
                ),
                _packs: None,
            })
        }
        pub fn assets(&self) -> Result<ItemAssets> {
            ItemAssets::load(&self.presentation, &self.weapons)
        }
    }
}

#[cfg(test)]
mod texture_rule_tests {
    use super::*;
    use crate::testing::{material, plain, rigid_shape};

    fn image(alpha_at: impl Fn(u32, u32) -> u8) -> SceneImage {
        let (width, height) = (32, 32);
        SceneImage {
            label: "texture".into(),
            width,
            height,
            rgba: (0..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .flat_map(|(x, y)| [30, 130, 60, alpha_at(x, y)])
                .collect(),
            srgb: true,
        }
    }

    /// v20 lays a colour-shifted model's texture over the shift colour
    /// only when the texture has a translucent texel among those it
    /// samples (every 16th along each axis); any other texture is the
    /// texture times the light, untinted. The HE Grenade's opaque green
    /// drew flat and unlit when every opaque material was laid over.
    #[test]
    fn only_a_texture_with_a_translucent_texel_is_laid_over_the_colour() -> Result<()> {
        let shape = rigid_shape(
            "test/texture.dts",
            &[("root", None, [0.0; 3])],
            &[(0, [0.0; 3], [0.2; 3], plain(0))],
            vec![material("skin", "opaque")],
        );
        let pose = bri_content::animation::sample(&shape, None, 0.0)?;
        let scene = |texture: &SceneImage| -> Result<Material> {
            let scene = native_shape_scene(
                "test",
                &shape,
                &[texture],
                [0.4, 0.2, 0.0, 1.0],
                true,
                Mat4::IDENTITY,
                &pose,
            )?;
            Ok(scene.materials[0].clone())
        };
        let solid = scene(&image(|_, _| 255))?;
        assert_eq!(solid.kind, MaterialKind::VertexLit, "lit");
        assert!(solid.untinted, "and not tinted");
        // A translucent texel v20 samples makes it a cover over the colour.
        let sampled = scene(&image(|x, y| if (x, y) == (16, 16) { 0 } else { 255 }))?;
        assert_eq!(sampled.kind, MaterialKind::BrickOverlay);
        assert!(!sampled.untinted);
        // One it does not sample leaves it a plain texture.
        let unsampled = scene(&image(|x, y| if (x, y) == (3, 5) { 0 } else { 255 }))?;
        assert_eq!(unsampled.kind, MaterialKind::VertexLit);
        Ok(())
    }
}
