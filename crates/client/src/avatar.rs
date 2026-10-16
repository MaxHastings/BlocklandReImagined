//! Native avatar resources, outfit binding and live player pose rendering.
use crate::crouch::CrouchThread;
use anyhow::{Context, Result, ensure};
use bri_client_sandbox::world::{Bounds, Rig as NodeTree, Skeleton};
use bri_console::Clamp;
use bri_content::{
    animation::{Channels, Layer, sample_layers_with_transition},
    avatar::{Appearance, Outfit, Package, Rig},
    body::{Animations, CrouchComposition, Role, RoleClips, Sockets},
};
use bri_render::scene::{
    AlphaMode, GpuInstances, GpuScene, Material, SceneData, SceneImage, SceneRenderer,
};
use bri_sim::archetype::HORSE_SHAPE;
use bri_sim::player::PlayerState;
use bri_ui::api::AvatarPrefs;
use glam::{Mat4, Quat, Vec3};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, sync::Arc};

pub struct AvatarAssets {
    pub package: Package,
    pub rig: Rig,
    images: BTreeMap<String, SceneImage>,
    detail: usize,
    /// A body's details by distance: each visible detail's screen height
    /// in pixels and index, tallest first. Empty for a body of one detail.
    lods: Vec<(f32, usize)>,
    /// Bodies other than the Blockhead, by the archetype model that draws
    /// them: `HorseArmor`'s horse.dts ([`HORSE_SHAPE`]), and each enabled
    /// Add-On's own player model (the Shark's `shark.dts`).
    bodies: BTreeMap<String, Box<AvatarAssets>>,
    /// Each shape object's name in lower case, as outfits name them.
    object_names: Vec<String>,
    /// Node index by lower-case name (the first node of a name), and each
    /// `Mount<n>` node's, so a lookup is not a scan of every node name.
    node_index: std::collections::HashMap<String, usize>,
    mount_nodes: [Option<usize>; 32],
    /// The node tree Add-Ons with `avatar.pose` see.
    tree: Arc<NodeTree>,
    /// An imported glTF `body`: its hands are fitted to hold as the
    /// Blockhead's do ([`bri_content::hold`]). A converted v20 shape keeps
    /// the mounts its artist placed.
    fitted: bool,
    /// The Blockhead's hip while it sits ([`bri_content::hold::sit_hip`]):
    /// where a seat puts a fitted body's hip.
    seat_hip: Option<Vec3>,
}
/// Farthest a posed node may be put from the body's feet; a pose further
/// away is not drawn.
const MAX_POSE_REACH: f32 = 256.0;

/// The node tree Add-On code sees: names, parents, and each part with the
/// node it moves with (a skinned part: the bone most of it follows).
fn node_tree(rig: &Rig) -> Arc<NodeTree> {
    let shape = &rig.shape;
    let parts = shape
        .objects
        .iter()
        .filter_map(|object| {
            let node = object.node.or_else(|| {
                let skin = object
                    .meshes
                    .iter()
                    .find_map(|m| shape.meshes.get(*m)?.as_ref()?.skin.as_ref())?;
                let mut weight = vec![0.0f32; skin.nodes.len()];
                for influence in &skin.influences {
                    weight[influence.bone] += influence.weight;
                }
                let bone = (0..weight.len()).max_by(|a, b| weight[*a].total_cmp(&weight[*b]))?;
                skin.nodes.get(bone).copied()
            })?;
            Some((object.name.to_ascii_lowercase(), node as u32))
        })
        .collect();
    Arc::new(NodeTree {
        names: shape.nodes.iter().map(|n| n.name.clone()).collect(),
        parents: shape
            .nodes
            .iter()
            .map(|n| n.parent.map_or(-1, |p| p as i32))
            .collect(),
        parts,
    })
}
/// The drawn geometry hanging on each node, as a box in the node's frame:
/// parts the outfit shows, at the drawn detail.
fn node_bounds(assets: &AvatarAssets, outfit: &Outfit) -> Vec<Option<Bounds>> {
    let shape = &assets.rig.shape;
    let mut out: Vec<Option<Bounds>> = vec![None; shape.nodes.len()];
    let mut grow = |node: usize, p: Vec3| {
        if !p.is_finite() {
            return;
        }
        let Some(slot) = out.get_mut(node) else {
            return;
        };
        let [min, max] = slot.get_or_insert([p.to_array(); 2]);
        *min = Vec3::from(*min).min(p).to_array();
        *max = Vec3::from(*max).max(p).to_array();
    };
    let Some(detail) = shape.details.get(assets.detail) else {
        return out;
    };
    for i in detail.object_start..detail.object_start + detail.object_count {
        let object = &shape.objects[i];
        if object.visibility <= 0.0 || !outfit.nodes.contains_key(&assets.object_names[i]) {
            continue;
        }
        let Some(Some(mesh)) = object
            .meshes
            .get(detail.mesh_offset)
            .and_then(|m| shape.meshes.get(*m))
        else {
            continue;
        };
        let frame = &mesh.positions[..mesh.frame_vertices.min(mesh.positions.len())];
        match (&mesh.skin, object.node) {
            (Some(skin), _) => {
                // Each vertex goes with the bone that moves it most.
                let mut best = vec![(0usize, 0.0f32); frame.len()];
                for influence in &skin.influences {
                    if let Some(b) = best.get_mut(influence.vertex)
                        && influence.weight > b.1
                    {
                        *b = (influence.bone, influence.weight);
                    }
                }
                for (p, (bone, weight)) in frame.iter().zip(best) {
                    if weight > 0.0 {
                        let local = Mat4::from_cols_array(&skin.inverse_bind[bone])
                            .transform_point3(Vec3::from(*p));
                        grow(skin.nodes[bone], local);
                    }
                }
            }
            (None, Some(node)) => {
                for p in frame {
                    grow(node, Vec3::from(*p));
                }
            }
            (None, None) => {}
        }
    }
    out
}
/// For a body posed from some of its nodes (`placed`), the placed node each
/// other node rides with when no placed node is above it in the tree:
/// the one whose drawn geometry its own is nearest, in the animated pose.
/// So a hat, cape or pack that the rig hangs beside the body's parts, not
/// under them, stays on the head or back it sits on instead of staying
/// where the animation left it. Nodes under a placed node follow their
/// parent (`None`), as do placed nodes themselves.
fn follow_anchors(
    parents: &[Option<usize>],
    order: &[usize],
    placed: &[Option<(Vec3, Quat)>],
    animated: &[Mat4],
    bounds: &[Option<Bounds>],
) -> Vec<Option<usize>> {
    let count = parents.len();
    let mut under = vec![false; count];
    for &i in order {
        under[i] = placed[i].is_some() || parents[i].is_some_and(|p| under[p]);
    }
    let centre = |i: usize| {
        let local = bounds
            .get(i)
            .copied()
            .flatten()
            .map_or(Vec3::ZERO, |[min, max]| {
                (Vec3::from(min) + Vec3::from(max)) * 0.5
            });
        animated[i].transform_point3(local)
    };
    let placed_nodes: Vec<usize> = (0..count).filter(|i| placed[*i].is_some()).collect();
    (0..count)
        .map(|i| {
            if under[i] {
                return None;
            }
            let at = centre(i);
            placed_nodes.iter().copied().min_by(|a, b| {
                let gap = |a: usize| {
                    let local = animated[a].inverse().transform_point3(at);
                    match bounds.get(a).copied().flatten() {
                        // Distance to what is drawn on it, 0 inside.
                        Some([min, max]) => {
                            (local - local.clamp(Vec3::from(min), Vec3::from(max))).length()
                        }
                        None => local.length(),
                    }
                };
                gap(*a).total_cmp(&gap(*b))
            })
        })
        .collect()
}
/// Node indices by lower-case name, first of a name winning, and the
/// `Mount<n>` nodes' indices.
fn node_indices(
    rig: &Rig,
) -> (
    std::collections::HashMap<String, usize>,
    [Option<usize>; 32],
) {
    let mut index = std::collections::HashMap::new();
    for (i, node) in rig.shape.nodes.iter().enumerate() {
        index.entry(node.name.to_ascii_lowercase()).or_insert(i);
    }
    let mounts = std::array::from_fn(|n| index.get(&format!("mount{n}")).copied());
    (index, mounts)
}
/// Whether two sampled poses draw the same.
fn same_pose(a: &bri_content::animation::Pose, b: &bri_content::animation::Pose) -> bool {
    a.nodes == b.nodes
        && a.visibility == b.visibility
        && a.frames == b.frames
        && a.material_frames == b.material_frames
}
fn lower_names(rig: &Rig) -> Vec<String> {
    rig.shape
        .objects
        .iter()
        .map(|o| o.name.to_ascii_lowercase())
        .collect()
}
impl AvatarAssets {
    pub fn load(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let read = |path: &str, limit| crate::materials::read_resource(&root, path, limit);
        let package: Package = serde_json::from_slice(&read("avatar.json", 8 * 1024 * 1024)?)?;
        package.validate()?;
        let bytes = read(&package.rig, 64 * 1024 * 1024)?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == package.rig_sha256,
            "Avatar rig checksum mismatch"
        );
        let rig: Rig = serde_json::from_slice(&bytes)?;
        rig.validate()?;
        ensure!(
            package
                .textures
                .values()
                .map(|t| u64::from(t.width) * u64::from(t.height) * 4)
                .sum::<u64>()
                <= 256 * 1024 * 1024,
            "Avatar image budget exceeded"
        );
        let mut images = BTreeMap::new();
        for (id, texture) in &package.textures {
            let bytes = read(&texture.file, 16 * 1024 * 1024)?;
            ensure!(
                format!("{:x}", Sha256::digest(&bytes)) == texture.sha256,
                "Avatar texture checksum mismatch: {id}"
            );
            let dimensions = image::ImageReader::with_format(
                std::io::Cursor::new(&bytes),
                image::ImageFormat::Png,
            )
            .into_dimensions()?;
            ensure!(
                dimensions == (texture.width, texture.height),
                "Avatar image dimensions changed"
            );
            let pixels =
                image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)?.to_rgba8();
            images.insert(
                id.clone(),
                SceneImage {
                    label: id.clone(),
                    width: texture.width,
                    height: texture.height,
                    rgba: pixels.into_raw(),
                    srgb: false,
                },
            );
        }
        let detail = rig
            .shape
            .details
            .iter()
            .position(|d| !d.collision)
            .context("Avatar has no visible detail")?;
        let object_names = lower_names(&rig);
        let (node_index, mount_nodes) = node_indices(&rig);
        Ok(Self {
            tree: node_tree(&rig),
            object_names,
            node_index,
            mount_nodes,
            package,
            rig,
            images,
            detail,
            lods: Vec::new(),
            bodies: BTreeMap::new(),
            fitted: false,
            seat_hip: None,
        })
        .map(|mut assets: Self| {
            assets.seat_hip =
                bri_content::hold::sit_hip(&assets.rig.shape, assets.rig.sequence(SIT));
            assets
        })
    }
    /// Load `HorseArmor`'s shape, sequence aliases and paint textures from
    /// the vehicle pack, so a player of that datablock draws as a horse.
    pub fn load_horse(&mut self, vehicles: &Path) -> Result<()> {
        const HORSE: &str = "v20.vehicle.horsearmor";
        let root = vehicles.canonicalize()?;
        let pack = bri_vehicles::Pack::load(root.join("vehicles.json"))?;
        let definition = pack
            .definitions
            .iter()
            .find(|d| d.id == HORSE)
            .context("Vehicle pack has no HorseArmor")?;
        let asset = |path: &str| {
            pack.assets
                .iter()
                .find(|a| a.path == path)
                .with_context(|| format!("Undeclared vehicle asset {path}"))
        };
        let model = asset(&definition.model)?;
        let shape: bri_content::shape::Shape = serde_json::from_slice(
            &crate::items::checked_read(&root, &model.path, &model.sha256, 32 << 20)?,
        )?;
        shape.validate()?;
        let mut sequences = BTreeMap::new();
        for (key, clips) in &pack.animation_aliases {
            let Some(alias) = key.strip_prefix(&format!("{HORSE}::")) else {
                continue;
            };
            let clips = asset(clips)?;
            let set: bri_content::shape::ClipSet = serde_json::from_slice(
                &crate::items::checked_read(&root, &clips.path, &clips.sha256, 8 << 20)?,
            )?;
            // Each alias names one authored `.dsq`, holding its one sequence.
            let mut clip = set
                .animations
                .into_iter()
                .next()
                .with_context(|| format!("Empty horse clip {alias}"))?;
            // horse.dts fills the Blockhead's arm, head and action sequences
            // with copies of `h_root.dsq`: they must pose nothing, layered
            // like the Blockhead's own sequence of that name.
            if !clip.name.eq_ignore_ascii_case(alias) {
                clip.nodes.clear();
                clip.objects.clear();
                clip.additive = self.rig.sequence(alias).is_some_and(|b| b.additive);
            }
            sequences.insert(alias.to_ascii_lowercase(), clip);
        }
        let mut images = BTreeMap::new();
        for material in &shape.materials {
            let name = material.name.to_ascii_lowercase();
            let texture = pack
                .assets
                .iter()
                .filter(|a| a.kind == "texture")
                .find(|a| {
                    a.virtual_path
                        .to_ascii_lowercase()
                        .ends_with(&format!("/{name}.png"))
                })
                .with_context(|| format!("Missing horse texture {name}"))?;
            let bytes =
                crate::items::checked_read(&root, &texture.path, &texture.sha256, 16 << 20)?;
            let pixels = image::load_from_memory(&bytes)?.to_rgba8();
            images.insert(
                name.clone(),
                SceneImage {
                    label: format!("horse/{name}"),
                    width: pixels.width(),
                    height: pixels.height(),
                    rgba: pixels.into_raw(),
                    srgb: false,
                },
            );
        }
        let detail = shape
            .details
            .iter()
            .position(|d| !d.collision)
            .context("Horse has no visible detail")?;
        let rig = Rig {
            schema_version: self.rig.schema_version,
            id: HORSE.into(),
            shape,
            sequences,
            sources: Vec::new(),
            omissions: Vec::new(),
        };
        for needed in ["root", "run", "back", "side", "crouch", "look", "headside"] {
            ensure!(rig.sequence(needed).is_some(), "Horse lacks {needed}");
        }
        let (node_index, mount_nodes) = node_indices(&rig);
        let horse = Box::new(Self {
            tree: node_tree(&rig),
            object_names: lower_names(&rig),
            node_index,
            mount_nodes,
            package: self.package.clone(),
            rig,
            images,
            detail,
            lods: Vec::new(),
            bodies: BTreeMap::new(),
            fitted: false,
            seat_hip: None,
        });
        self.bodies.insert(HORSE_SHAPE.into(), horse);
        Ok(())
    }
    /// Each enabled Add-On's own player model: an archetype whose model is
    /// one of its package's converted shapes (`<package>:asset/<file>`,
    /// Import Add-On's `shapeFile`), drawn with that shape's own sequences
    /// and its materials' textures from the same package.
    pub fn load_bodies(&mut self, root: &Path, packages: &bri_package::packages::PackageSet) {
        for entry in packages.packages.iter().filter(|p| p.role.is_none()) {
            let Ok(dir) = bri_package::packages::package_dir(root, entry) else {
                continue;
            };
            let models = body_models(&dir, &entry.id);
            for (model, file) in models {
                if self.bodies.contains_key(&model) {
                    continue;
                }
                // One that does not load leaves its players Blockheads.
                match self.package_body(&dir, &model, file.as_deref()) {
                    Ok(body) => {
                        self.bodies.insert(model, Box::new(body));
                    }
                    Err(error) => {
                        crate::cosmetic::add_on_fault(&entry.id, &model, format!("{error:#}"));
                    }
                }
            }
        }
    }
    /// The body `model` in the package at `dir`: a `body` in its `file`, or
    /// an asset (`<package>:asset/<file>`) its `assets/content.json` lists.
    /// Its textures are assets that file lists.
    fn package_body(&self, dir: &Path, model: &str, shape_file: Option<&str>) -> Result<Self> {
        let content: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("assets/content.json"))?)?;
        let file = |id: &str| {
            content["content"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|c| c["id"].as_str().is_some_and(|c| c.eq_ignore_ascii_case(id)))
                .and_then(|c| c["file"].as_str())
                .map(str::to_owned)
                .with_context(|| format!("{id} is not in its package"))
        };
        let fitted = shape_file.is_some();
        let shape_file = match shape_file {
            Some(f) => f.to_owned(),
            None => file(model)?,
        };
        let root = dir.canonicalize()?;
        let shape: bri_content::shape::Shape = serde_json::from_slice(
            &crate::materials::read_resource(&root, &shape_file, 32 << 20)?,
        )?;
        shape.validate()?;
        let package = model.split(':').next().unwrap_or_default();
        let mut images = BTreeMap::new();
        for material in &shape.materials {
            let name = material.name.to_ascii_lowercase();
            let png = if name.ends_with(".png") {
                name.clone()
            } else {
                format!("{name}.png")
            };
            let bytes = crate::materials::read_resource(
                &root,
                &file(&format!("{package}:asset/{png}"))?,
                16 << 20,
            )?;
            let pixels = image::load_from_memory(&bytes)?.to_rgba8();
            images.insert(
                name.clone(),
                SceneImage {
                    label: format!("{model}/{name}"),
                    width: pixels.width(),
                    height: pixels.height(),
                    rgba: pixels.into_raw(),
                    srgb: false,
                },
            );
        }
        let detail = shape
            .details
            .iter()
            .position(|d| !d.collision)
            .context("no visible detail")?;
        let mut lods: Vec<(f32, usize)> = shape
            .details
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                !d.collision
                    && (0.0..crate::items::FIRST_PERSON_DETAIL).contains(&d.pixel_threshold)
            })
            .map(|(i, d)| (d.pixel_threshold, i))
            .collect();
        lods.sort_by(|a, b| b.0.total_cmp(&a.0));
        if lods.len() < 2 {
            lods.clear();
        }
        let sequences = shape
            .animations
            .iter()
            .map(|a| (a.name.to_ascii_lowercase(), a.clone()))
            .collect();
        let rig = Rig {
            schema_version: self.rig.schema_version,
            id: model.into(),
            shape,
            sequences,
            sources: Vec::new(),
            omissions: Vec::new(),
        };
        // Clips it lacks fall back or leave their layer out
        // (`RoleClips`); a body with none stands in its bind pose.
        let (node_index, mount_nodes) = node_indices(&rig);
        Ok(Self {
            tree: node_tree(&rig),
            object_names: lower_names(&rig),
            node_index,
            mount_nodes,
            package: self.package.clone(),
            rig,
            images,
            detail,
            lods,
            bodies: BTreeMap::new(),
            fitted,
            seat_hip: self.seat_hip,
        })
    }
    /// Whether `model` draws a body other than the Blockhead.
    pub fn has_body(&self, model: &str) -> bool {
        self.bodies.contains_key(model)
    }
    /// A player of `HorseArmor`: `ApplyBodyColors` paints the body with the
    /// chest colour and the head black; the ski nodes stay hidden.
    pub fn horse_mesh(&self, appearance: Appearance) -> Result<AvatarMesh> {
        self.body_mesh(HORSE_SHAPE, appearance)
    }
    /// A player drawn with the body `model`. An Add-On's own model paints
    /// each object named as an avatar colour slot with that slot's paint
    /// (`chest` the torso's), as `ApplyBodyColors` paints the Blockhead's
    /// nodes. Named accessory objects follow selected avatar parts; all
    /// remaining objects stay hidden.
    pub fn body_mesh(&self, model: &str, appearance: Appearance) -> Result<AvatarMesh> {
        let horse = self
            .bodies
            .get(model)
            .with_context(|| format!("Body {model} is not loaded"))?;
        let nodes = if model == HORSE_SHAPE {
            let chest = appearance.colors.get("chest").copied().unwrap_or([1.0; 4]);
            [
                ("body".into(), chest),
                ("head".into(), [0.0, 0.0, 0.0, 1.0]),
            ]
            .into()
        } else {
            let selected = self.package.resolve(&appearance)?;
            // An object named as a Blockhead part or colour slot is that
            // part: painted, or hidden when the outfit leaves it out. Any
            // other object (an imported model's own) draws with its own
            // textures, untinted.
            let part = |name: &str| {
                selected.nodes.contains_key(name)
                    || appearance.colors.contains_key(name)
                    || self.package.defaults.colors.contains_key(name)
                    || self
                        .package
                        .parts
                        .values()
                        .flatten()
                        .any(|p| p.eq_ignore_ascii_case(name))
            };
            horse
                .object_names
                .iter()
                .filter_map(|name| {
                    let slot = if name == "chest" {
                        "torso"
                    } else {
                        name.as_str()
                    };
                    appearance
                        .colors
                        .get(slot)
                        .or_else(|| selected.nodes.get(name))
                        .copied()
                        .or_else(|| (!part(name)).then_some([1.0; 4]))
                        .map(|c| (name.clone(), c))
                })
                .collect()
        };
        let outfit = Outfit {
            nodes,
            face: String::new(),
            decal: String::new(),
            head_up: false,
        };
        let mut data = SceneData {
            name: if model == HORSE_SHAPE {
                "Horse".into()
            } else {
                model.into()
            },
            ..Default::default()
        };
        let mut materials = Vec::new();
        for source in &horse.rig.shape.materials {
            let name = source.name.to_ascii_lowercase();
            let image = data.images.len();
            data.images.push(horse.images[&name].clone());
            materials.push(data.materials.len());
            let name = format!("{model}/{name}");
            // The horse lays its texture over its chest colour. Any other
            // body draws as a held item does (`items::native_shape_scene`):
            // a texture with a translucent texel over its colour, one with
            // none the texture times the world's light, untinted. As an
            // overlay an opaque texture would ignore the light entirely.
            let overlay = model == HORSE_SHAPE
                || (crate::items::translucent_texel(&data.images[image])
                    && source.blend == "opaque");
            let mut material = if overlay {
                Material::brick_overlay(name, image)
            } else {
                Material::vertex_lit(name, image)
            };
            if !overlay {
                // A lit texture is a colour (sRGB); an overlay's is mixed raw.
                data.images[image].srgb = true;
                material.untinted = source.blend == "opaque";
            }
            if model != HORSE_SHAPE && source.blend == "alpha" {
                material.alpha = AlphaMode::Blend;
            }
            if source.unlit {
                material.kind = if overlay {
                    bri_render::scene::MaterialKind::UnlitOverlay
                } else {
                    bri_render::scene::MaterialKind::Unlit
                };
            }
            data.materials.push(material);
        }
        let translucent_materials: Vec<_> = materials
            .iter()
            .map(|index| {
                let mut material = data.materials[*index].clone();
                material.alpha = AlphaMode::Blend;
                let index = data.materials.len();
                data.materials.push(material);
                index
            })
            .collect();
        let mut mesh = self.mesh_from(appearance, data, outfit, materials, translucent_materials);
        mesh.model = Some(model.into());
        Ok(mesh)
    }
    /// A node's index by name, ignoring ASCII case.
    fn node(&self, name: &str) -> Option<usize> {
        if name.bytes().any(|b| b.is_ascii_uppercase()) {
            self.node_index.get(&name.to_ascii_lowercase()).copied()
        } else {
            self.node_index.get(name).copied()
        }
    }
    /// The rig and textures this mesh draws with.
    /// The detail a body draws at `screen_height` pixels tall: the most
    /// detailed one it is tall enough for, the least detailed below them
    /// all. Unknown heights (no frame drawn yet) draw the full detail.
    fn detail_at(&self, screen_height: Option<f32>) -> usize {
        match (screen_height, self.lods.last()) {
            (Some(px), Some(&(_, least))) => self
                .lods
                .iter()
                .find(|(threshold, _)| px >= *threshold)
                .map_or(least, |(_, d)| *d),
            _ => self.detail,
        }
    }
    fn for_mesh(&self, mesh: &AvatarMesh) -> &AvatarAssets {
        mesh.model
            .as_ref()
            .and_then(|m| self.bodies.get(m))
            .map_or(self, |body| body)
    }
    /// The appearance the player's avatar prefs make: what the avatar
    /// screen previews, what the client sends and what the host keeps.
    pub fn from_prefs(&self, prefs: &AvatarPrefs) -> Result<Appearance> {
        appearance_from_prefs(&self.package, prefs)
    }
    pub fn mesh(&self, appearance: Appearance) -> Result<AvatarMesh> {
        let outfit = self.package.resolve(&appearance)?;
        ensure!(
            outfit.nodes.keys().all(|n| self
                .rig
                .shape
                .objects
                .iter()
                .any(|o| o.name.eq_ignore_ascii_case(n))),
            "Avatar outfit references absent geometry"
        );
        let mut data = SceneData {
            name: "Original Blockhead".into(),
            ..Default::default()
        };
        let mut images = BTreeMap::new();
        let mut materials = Vec::new();
        for source in &self.rig.shape.materials {
            let name = source.name.to_ascii_lowercase();
            let id = match name.as_str() {
                "face" => &outfit.face,
                "decal" => &outfit.decal,
                _ => self
                    .package
                    .surfaces
                    .get(&name)
                    .with_context(|| format!("Unbound avatar material {name}"))?,
            };
            let image = *images.entry(id.clone()).or_insert_with(|| {
                let index = data.images.len();
                data.images.push(self.images[id].clone());
                index
            });
            let mut material = Material::brick_overlay(format!("avatar/{name}"), image);
            if source.blend == "alpha" {
                material.alpha = AlphaMode::Blend;
            }
            materials.push(data.materials.len());
            data.materials.push(material);
        }
        let translucent_materials: Vec<_> = materials
            .iter()
            .map(|index| {
                let mut material = data.materials[*index].clone();
                material.alpha = AlphaMode::Blend;
                let index = data.materials.len();
                data.materials.push(material);
                index
            })
            .collect();
        Ok(self.mesh_from(appearance, data, outfit, materials, translucent_materials))
    }
    fn mesh_from(
        &self,
        appearance: Appearance,
        data: SceneData,
        outfit: Outfit,
        materials: Vec<usize>,
        translucent_materials: Vec<usize>,
    ) -> AvatarMesh {
        AvatarMesh {
            model: None,
            appearance,
            data,
            gpu: None,
            layout: None,
            restructured: true,
            pending: None,
            defer_mesh: false,
            instanced: false,
            screen_height: None,
            straddle: None,
            instance: None,
            drawn_pose: None,
            vertices_dirty: true,
            outfit,
            materials,
            translucent_materials,
            mode: "root",
            forward: true,
            phase: 0.0,
            last_time: None,
            channels: None,
            transition: None,
            stood_at: None,
            crouch: CrouchThread::default(),
            body: None,
            dead: false,
            drawn_offset: None,
            hidden_nodes: Vec::new(),
            posed_nodes: Vec::new(),
            animated_nodes: Vec::new(),
            node_bounds: None,
            unacted_nodes: Vec::new(),
            tool_only_actions: [None; 2],
            wants_unacted: true,
            model_transform: Mat4::IDENTITY,
            kind: BodyKind::default(),
            roles: None,
            fits: None,
            held: ([false; 2], 0.0),
            skipped: Vec::new(),
        }
    }
}

impl AvatarMesh {
    /// `Player::startSkiing` unhides the LSki/RSki nodes in paint color.
    pub fn set_skis(&mut self, color: Option<[f32; 4]>) {
        for node in ["lski", "rski"] {
            match color {
                Some(color) => {
                    self.outfit.nodes.insert(node.into(), color);
                }
                None => {
                    self.outfit.nodes.remove(node);
                }
            }
        }
    }
}

pub struct AvatarMesh {
    /// The body drawn instead of the Blockhead ([`AvatarAssets::body_mesh`]).
    pub model: Option<String>,
    /// How tall the body last drew on screen, in pixels: a body with a
    /// distant detail draws it when small. None draws the full detail.
    pub screen_height: Option<f32>,
    posed_nodes: Vec<Mat4>,
    /// While Add-On code poses the body (`override_nodes`), the pose the
    /// animation gave it; empty otherwise. Gameplay-facing nodes (the eye)
    /// come from here, so a pose only ever changes what is drawn.
    animated_nodes: Vec<Mat4>,
    /// Each node's drawn geometry, for Add-On code (`skeleton`); built
    /// the first time it is asked for.
    node_bounds: Option<Arc<Vec<Option<Bounds>>>>,
    /// `posed_nodes` without the thread-2/3 action layers, while one plays;
    /// empty otherwise (`mount_action`).
    unacted_nodes: Vec<Mat4>,
    /// Reference tool motion for bodies without that action; never deforms the body.
    tool_only_actions: [Option<Mat4>; 2],
    /// Sample `unacted_nodes` while an action plays. Only a first-person
    /// image reads them, so the game asks for them for the player's own
    /// body alone: a second skeleton sample per acting body otherwise.
    pub wants_unacted: bool,
    model_transform: Mat4,
    pub appearance: Appearance,
    pub data: SceneData,
    pub gpu: Option<GpuScene>,
    /// The drawn mesh's structure, for rewriting only positions each frame.
    layout: Option<crate::avatar_mesh::Layout>,
    /// The structure changed since the last upload: upload it all again.
    restructured: bool,
    /// With `defer_mesh`, the pose waiting for `upload` to build the mesh
    /// (a body the camera does not see is never built).
    pending: Option<bri_content::animation::Pose>,
    /// Build the mesh at `upload` instead of at every pose.
    pub defer_mesh: bool,
    /// Build the mesh in model space and draw it through `instance`, which
    /// carries the body transform: a body that moves without changing its
    /// pose (a rider, a player standing still) re-sends no vertices.
    pub instanced: bool,
    /// The drawn body's transform, for `instanced` meshes.
    pub instance: Option<GpuInstances>,
    /// The opening an `instanced` body is part way through: it draws cut
    /// there, with the part through drawn at the partner.
    pub straddle: Option<crate::portal_view::Straddle>,
    /// The pose last written into `data`, to skip rewriting an equal one.
    drawn_pose: Option<bri_content::animation::Pose>,
    /// `data` holds vertices not yet sent to the GPU.
    vertices_dirty: bool,
    outfit: Outfit,
    materials: Vec<usize>,
    translucent_materials: Vec<usize>,
    mode: &'static str,
    /// Torque plays the side clip backward to strafe right.
    forward: bool,
    phase: f32,
    last_time: Option<f64>,
    /// Locomotion channels of the last pose, frozen when a transition starts.
    channels: Option<Channels>,
    /// Frozen source pose and start time of the current action transition.
    transition: Option<(Channels, f64)>,
    /// When the body last left the [`SIT`] pose.
    stood_at: Option<f64>,
    crouch: CrouchThread,
    /// The spawn this mesh animates (`Vitals::spawn_tick`).
    body: Option<u64>,
    /// Whether the drawn body lies dead (`drawn_life`), for Add-On code.
    dead: bool,
    /// How far from where the game has it this body was last drawn
    /// ([`Self::drawn_offset`]), kept once the body is gone.
    drawn_offset: Option<Vec3>,
    /// Body nodes the held images hide (`bri_weapons::Image::hide_nodes`),
    /// lower case; the outfit keeps them for when the image goes.
    hidden_nodes: Vec<String>,
    /// How the player's archetype fits this body ([`BodyKind`]).
    kind: BodyKind,
    /// The body's clip for each animation role, resolved against its rig
    /// the first time it is posed after `kind` changes.
    roles: Option<Arc<RoleClips>>,
    /// How a fitted body holds with each hand, worked out with `roles`.
    fits: Option<Arc<Fits>>,
    /// The hands the last pose held something up in, and its aim's pitch:
    /// a fitted body's held hand points along the aim.
    held: ([bool; 2], f32),
    /// Clips the last pose asked for that the body lacks, left out.
    skipped: Vec<String>,
}

/// How a fitted body ([`AvatarAssets::fitted`]) holds with each hand: the
/// turn that lines its hand socket up with the Blockhead's `Mount0`, the
/// raise for an arm it has no aim clip for and where the hand holds from
/// its shoulder ([`bri_content::hold::fit`]).
#[derive(Debug, Default, PartialEq)]
struct Fits {
    first_person: [bool; 2],
    hands: [Option<bri_content::hold::Fit>; 2],
    /// How far a rider is moved (model frame, unscaled) so its hip sits
    /// where the Blockhead's does in its `sit` pose: every seat is
    /// authored for the Blockhead.
    seat: Option<Vec3>,
}

/// How a player's archetype fits the mesh drawn for it: where its sockets
/// are, which clips play each animation role, and whether the mesh is the
/// body at all.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyKind {
    pub crouch_composition: CrouchComposition,
    pub sockets: Sockets,
    pub animations: Animations,
    /// The archetype draws a box model or no body: this mesh is a hidden
    /// Blockhead that only times the poses (sitting down, getting up), and
    /// none of its nodes is the body's. Sockets are offsets, or missing.
    pub stand_in: bool,
    /// Where a stand-in body sees from standing and seated (feet at the
    /// origin, facing -Z, unscaled): its archetype's eyes.
    pub eye: Vec3,
    pub sit_eye: Vec3,
    /// The archetype's speed for each locomotion role's clip at 1x
    /// (`Look::ground_speeds`).
    pub ground_speeds: bri_content::body::GroundSpeeds,
    /// The fastest a locomotion clip plays (`Look::max_time_scale`); 0
    /// takes v20's 1.5.
    pub max_time_scale: f32,
    /// Its own death clips by side and at random (`Look::deaths`).
    pub deaths: bri_content::body::Deaths,
}
impl BodyKind {
    /// The archetype's fit for a mesh: `stand_in` when it draws a box model
    /// or no body.
    pub fn of(archetype: &bri_sim::archetype::Archetype, stand_in: bool) -> Self {
        let m = &archetype.movement;
        Self {
            crouch_composition: archetype.look.crouch_composition,
            sockets: archetype.sockets.clone(),
            animations: archetype.look.animations.clone(),
            stand_in,
            eye: Vec3::new(0.0, m.stand_eye, -m.eye_forward),
            sit_eye: Vec3::new(0.0, m.sit_eye, -m.sit_eye_forward),
            ground_speeds: archetype.look.ground_speeds.clone(),
            max_time_scale: archetype.look.max_time_scale,
            deaths: archetype.look.deaths.clone(),
        }
    }
    /// How fast `clip` plays for `role` while the body moves at `velocity`
    /// (its own frame, facing -Z): v20's `Player::updateAnimation` plays a
    /// locomotion clip at the speed the body covers the ground along the
    /// clip's way, `|v . dir| / speed`, up to `maxTimeScale`. The speed is
    /// the archetype's for the role, else the clip's own ground motion; a
    /// clip with neither, and every other role, plays at 1x.
    pub fn time_scale(
        &self,
        clip: &bri_content::shape::Animation,
        role: Role,
        velocity: Vec3,
    ) -> f32 {
        let Some(axis) = role.ground_axis() else {
            return 1.0;
        };
        let ground = clip
            .ground_translations
            .last()
            .map(|&t| Vec3::from(t))
            .filter(|t| t.is_finite() && clip.duration > 0.0 && t.length() > 1e-4);
        let (speed, dir) = match self.ground_speeds.get(role.name()) {
            Some(&speed) => (speed, Vec3::from(axis)),
            None => match ground {
                Some(t) => (t.length() / clip.duration, t.normalize()),
                None => return 1.0,
            },
        };
        let max = if self.max_time_scale > 0.0 {
            self.max_time_scale
        } else {
            bri_sim::archetype::MAX_TIME_SCALE
        };
        let scale = (velocity.dot(dir) / speed).abs().min(max);
        if scale.is_finite() { scale } else { 1.0 }
    }
    /// What the body lacks, one line each: sockets for any body, and for a
    /// rigged one the animation roles it has no clip for.
    pub fn report(&self, roles: Option<&RoleClips>) -> Vec<String> {
        let mut lines = Vec::new();
        if self.stand_in {
            for (name, socket) in [
                ("right_hand", self.sockets.hand(0)),
                ("left_hand", self.sockets.hand(1)),
            ] {
                if socket.is_some_and(|s| s.node.is_some()) {
                    lines.push(format!(
                        "no `{name}` socket offset (a box body has no nodes), so items held in that hand are hidden"
                    ));
                }
            }
            if self.sockets.seat.as_ref().is_none_or(|s| s.node.is_some()) {
                lines.push(
                    "no `seat` socket offset, so it sits upright without folding its legs".into(),
                );
            }
        }
        if let Some(roles) = roles.filter(|_| !self.stand_in) {
            lines.extend(roles.report());
        }
        lines
    }
}
/// Log once what a body lacks for the archetype drawn with it
/// ([`AvatarMesh::body_report`]), as an Add-On fault naming the archetype
/// and its model: one line per missing socket or animation role.
pub fn report_body(archetype: &str, model: &str, mesh: &mut AvatarMesh, assets: &AvatarAssets) {
    static REPORTED: std::sync::Mutex<std::collections::BTreeSet<(String, String)>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    // The Blockhead is the reference every default names.
    if mesh.model.is_none() && !mesh.kind.stand_in {
        return;
    }
    let key = (archetype.to_owned(), model.to_owned());
    let Ok(mut reported) = REPORTED.lock() else {
        return;
    };
    if reported.contains(&key) {
        return;
    }
    let lines = mesh.body_report(assets);
    reported.insert(key);
    for line in lines {
        crate::cosmetic::add_on_fault(archetype, model, line);
    }
}

/// How a pose's arms hold on after its clips ([`AvatarMesh::fit_hands`]).
#[derive(Clone, Copy, Debug, Default)]
struct Holds {
    /// Hands raised to hold an item.
    raised: [bool; 2],
    /// The aim's pitch, up positive.
    pitch: f32,
    /// [`AvatarAnimationInput::support`].
    support: Option<Vec3>,
    /// [`AvatarAnimationInput::hand_holds`].
    seat: [Option<Vec3>; 2],
    /// Explicit seat-frame shift; None retains legacy fitting.
    seat_shift: Option<Vec3>,
}

/// Authored right/left hand readiness selected by mounted vanilla images.
/// Hammer, wrench and printer images normally select `Right`; dual images may
/// select either or both hands from their original `armReady` fields.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HeldToolPose {
    #[default]
    None,
    Right,
    Left,
    Both,
}
impl HeldToolPose {
    /// Applies vanilla `Player::updateArm` to mounted image data. Original
    /// image slots 0 and 1 are the right and left hands; false readiness and
    /// non-hand slots do not affect the pose.
    pub fn from_mounted_images(images: impl IntoIterator<Item = (u8, bool)>) -> Self {
        let (mut right, mut left) = (false, false);
        for (slot, arm_ready) in images {
            match slot {
                0 => right |= arm_ready,
                1 => left |= arm_ready,
                _ => {}
            }
        }
        match (right, left) {
            (false, false) => Self::None,
            (true, false) => Self::Right,
            (false, true) => Self::Left,
            (true, true) => Self::Both,
        }
    }
}

/// `Player::updateLookAnimation`: the arm thread follows the head pitch over
/// the arm range, then Blockland clamps it to the seated look limits.
fn look_position(pitch: f32, limits: Option<[f32; 2]>) -> f32 {
    let position = (0.5 - pitch / std::f32::consts::PI).clamped(0.0, 1.0);
    match limits {
        Some([down, up]) if down <= up => position.clamped(down, up),
        _ => position,
    }
}

/// An active original avatar-thread animation. `started_at` is in the same
/// monotonic seconds domain passed to `pose_with_animation`.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionAnimation {
    pub sequence: String,
    pub started_at: f64,
}

/// Per-actor animation inputs not represented in `PlayerState`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AvatarAnimationInput {
    pub held_tool_pose: HeldToolPose,
    /// A seated rider's `setLookLimits(up, down)`: the arm `look` thread
    /// position, 0 looking straight up to 1 straight down, stays inside
    /// `[down, up]`. The view itself is not limited.
    pub look_limits: Option<[f32; 2]>,
    /// A seated rider's full mount rotation, in place of the upright yaw,
    /// so they sit flush with a tilted seat.
    pub mount_rotation: Option<Quat>,
    /// A seated gunner's aim across the gun's pitch range, 0 highest to 1
    /// lowest: the body's `seat_aim` clip holds there in place of `look`,
    /// so the whole seated pose bends with the gun.
    pub seat_aim: Option<f32>,
    /// Where a seated rider's right and left hands hold on, in the world
    /// (a seat's `hands`): each arm reaches its hand there after the clip
    /// pose ([`reach_hands`]).
    pub hand_holds: [Option<Vec3>; 2],
    /// Where the off hand holds what the right hand holds in both hands,
    /// in the right hand's frame (`ItemAssets::image_support`). A fitted
    /// body reaches its left hand there.
    pub support: Option<Vec3>,
    /// Script threads 0 and 1 (`playThread(0 or 1, ...)`): body animations
    /// a package plays, such as a hit's flinch. Each holds until replaced
    /// or stopped by `root`.
    pub body: [Option<ActionAnimation>; 2],
    /// Current thread-2 action from the authoritative animation cue stream.
    /// Clear this on the corresponding vanilla stop/root cue or image switch.
    pub action: Option<ActionAnimation>,
    /// Current thread-3 builder or chat animation (`playThread(3, ...)`):
    /// brick shifts, rotations, plant, undo, activate and talk.
    pub gesture: Option<ActionAnimation>,
    /// Dead bodies hold the original `death1` sequence.
    pub dead: bool,
    /// How a dead body fell (the host's [`bri_sim::session::DeathPose`]):
    /// picks one of its archetype's `deaths` clips over `death1`.
    pub death: Option<bri_sim::session::DeathPose>,
    /// The `sit` emote holds the original sit sequence until the player moves.
    pub sitting: bool,
    /// Authored seat clip in the seat's own model-origin frame. A body without
    /// this clip keeps its own sit animation and aligns its hip to the seat.
    pub seat_pose: Option<String>,
    /// Current world hip target from the actual seat frame, independent of
    /// passenger rotation/body scale. Ignored when the body owns seat_pose.
    pub seat_fallback_hip: Option<Vec3>,
    /// Exact image-set eligibility; applied only to an owned active seat clip
    /// and the actual resolved ready role (including body fallback).
    pub seat_ready: bri_vehicles::schema::SeatReadyMatches,
    /// The latest simulated tick state. v20 picks the action from the tick's
    /// own rotation and velocity, not from the render-interpolated body or the
    /// live mouse yaw; mixing those breaks the exact tie at 45 degrees.
    pub tick_state: Option<PlayerState>,
    /// The fraction of the body in liquid, for `pickActionAnimation`'s
    /// water rules.
    pub water_coverage: f32,
    /// The clip the body's archetype plays for its movement controller's
    /// current intent (`Look::intent_clips`): Mario's triple jump or
    /// skid. It takes the place of the run, jump and fall clips while the
    /// model has it; death, sitting and swimming still win.
    pub intent_clip: Option<String>,
    /// How far the body leans, radians, right positive
    /// ([`bri_sim::player::PlayerState::lean_tilt`]): it tilts about its
    /// feet along its facing.
    pub lean_tilt: f32,
}

pub use bri_content::hold::reach as reach_hands;

/// What a body's posture does to its drawn frame, after its heading and
/// before its scale: a lean tilts it about the feet, right side down for
/// a lean right; lying prone lays it face down along its facing, its
/// middle over the feet and its back up, `depth` (half its thickness,
/// scaled) above the ground.
pub fn posture_frame(player: &PlayerState, lean_tilt: f32, depth: f32) -> Mat4 {
    let lean = if lean_tilt.is_finite() && lean_tilt != 0.0 {
        Mat4::from_rotation_z(-lean_tilt)
    } else {
        Mat4::IDENTITY
    };
    if !player.posture.prone {
        return lean;
    }
    // Head forward (-Z): the body's up turns to its facing.
    let middle = bri_sim::player::nominal_middle(1.0);
    Mat4::from_translation(Vec3::new(0.0, depth, middle))
        * lean
        * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2)
}

/// A clip name as the action thread holds it. Intent clips come from
/// archetype data, a bounded table, so each distinct name is kept once.
fn interned(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<std::collections::BTreeSet<&'static str>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(known) = names.get(name) {
        return known;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.insert(leaked);
    leaked
}

#[cfg(test)]
mod posture_tests {
    use super::*;

    #[test]
    fn a_lean_tilts_the_head_aside_and_prone_lays_it_ahead() {
        let mut player = PlayerState {
            owner: 1,
            feet: [0.0; 3],
            velocity: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            grounded: true,
            crouched: false,
            jetting: false,
            jump: Default::default(),
            archetype: Default::default(),
            scale: 1.0,
            energy: 0.0,
            speed_scale: 1.0,
            tick: Default::default(),
            tether: None,
            controller: None,
            pass_group: 0,
            posture: Default::default(),
        };
        let head = Vec3::new(0.0, 2.4, 0.0);
        assert_eq!(posture_frame(&player, 0.0, 0.3), Mat4::IDENTITY);
        // Facing -Z, right is +X: leaning right puts the head right.
        let leaned = posture_frame(&player, 0.3, 0.3).transform_point3(head);
        assert!(leaned.x > 0.5 && leaned.y < head.y, "{leaned}");
        player.posture.prone = true;
        let frame = posture_frame(&player, 0.0, 0.3);
        let (head, feet) = (
            frame.transform_point3(head),
            frame.transform_point3(Vec3::ZERO),
        );
        assert!(head.z < -0.8 && feet.z > 1.0, "head {head}, feet {feet}");
        assert!((head.y - 0.3).abs() < 1e-4 && (feet.y - 0.3).abs() < 1e-4);
    }
}

/// Which body a drawn player is, and whether it lies dead, at the tick of
/// the pose being drawn. v20 replicates the damage state with the `Player`
/// object itself; here vitals and poses travel apart (remotes are drawn
/// behind the vitals, a client's own body can be ahead of them), so the
/// death and spawn ticks put both on the pose's timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawnLife {
    /// The drawn body's spawn tick; `None` for a body older than the one the
    /// vitals describe, whose spawn they no longer say (keep the current one).
    pub body: Option<u64>,
    pub dead: bool,
}

/// The life of a pose at `tick`. `spawned` is the body the pose itself names
/// (a client's own pose); remote poses pass `None`.
pub fn drawn_life(vitals: &bri_sim::session::Vitals, tick: u64, spawned: Option<u64>) -> DrawnLife {
    let died = |tick: u64| vitals.died_tick.is_some_and(|died| tick >= died);
    match spawned {
        // A body the vitals have not heard of yet: just spawned, alive.
        Some(body) if body > vitals.spawn_tick => DrawnLife {
            body: Some(body),
            dead: false,
        },
        // An earlier body: whether it had died by then.
        Some(body) if body < vitals.spawn_tick => DrawnLife {
            body: Some(body),
            dead: died(tick),
        },
        None if tick < vitals.spawn_tick => DrawnLife {
            body: None,
            dead: died(tick),
        },
        _ => DrawnLife {
            body: Some(vitals.spawn_tick),
            dead: !vitals.alive && (vitals.died_tick.is_none() || died(tick)),
        },
    }
}

/// Effects a player's body makes at itself (the puff a corpse vanishes in,
/// `Player::RemoveBody`) play where this client draws that body, which
/// Add-On code may have moved (a ragdoll that slid away), not where the
/// game has it. Sounds at the same tick and place go with them. Anything
/// else, a shot landing elsewhere included, stays put.
pub fn follow_drawn_bodies<'a>(
    cues: &mut [bri_sim::presentation::Cue],
    body: impl Fn(bri_world::OwnerId) -> Option<&'a AvatarMesh>,
) {
    use bri_sim::presentation::CueKind;
    let at_body = |cue: &bri_sim::presentation::Cue| {
        // Effects on a node of the body (a muzzle flash) already follow
        // the drawn node.
        let CueKind::WeaponEffect {
            source: bri_weapons::TargetId::Actor(actor),
            node,
            image: None,
            ..
        } = &cue.kind
        else {
            return None;
        };
        if !node.is_empty() {
            return None;
        }
        let mesh = body(actor.0).filter(|mesh| mesh.holds(Vec3::from(cue.position)))?;
        mesh.drawn_offset()
    };
    let moved: Vec<_> = cues
        .iter()
        .filter_map(|cue| Some((cue.tick, cue.position, at_body(cue)?)))
        .collect();
    if moved.is_empty() {
        return;
    }
    for cue in cues.iter_mut() {
        let offset = match cue.kind {
            CueKind::WeaponEffect { .. } => at_body(cue),
            CueKind::WeaponSound { .. } => moved
                .iter()
                .find(|(tick, at, _)| *tick == cue.tick && *at == cue.position)
                .map(|(_, _, offset)| *offset),
            _ => None,
        };
        if let Some(offset) = offset {
            cue.position = (Vec3::from(cue.position) + offset).to_array();
        }
    }
}

/// The held `sit` pose (`/sit`, or a seat or mount point posed `sit`).
const SIT: &str = "sit";

/// `sAnimationTransitionTime`, and the shorter jump transition.
const TRANSITION_TIME: f64 = 0.25;
const JUMP_TRANSITION_TIME: f64 = 0.15;

/// A picked action sequence and its play direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocomotionAction {
    pub sequence: &'static str,
    pub forward: bool,
}

/// Speeds within this of each other tie. v20 rotates velocity into object
/// space with the transpose of the same z rotation that built it, so a
/// 45 degree move yields bit-identical forward and side components. Our
/// dot products differ by a few ulps instead, which alone must not flip it.
const PICK_TIE: f32 = 1e-4;

/// v20 `Player::pickActionAnimation` (blocklandv20.exe 0x5a2fe0): jetting
/// always holds the root pose; otherwise only a real fall (below -10 vertical
/// speed) uses `fall`.
///
/// On the ground an object-space velocity shorter than 0.4, vertical speed
/// included, is root. Otherwise it walks `actionList` in order (run, back,
/// side) and keeps the first sequence whose direction dotted with velocity
/// beats the running maximum, which starts at 0.1. The comparison is strict,
/// so an exact diagonal keeps `run` or `back`, never `side`. The Blockhead
/// clips carry no ground motion, so their directions are the table defaults:
/// run +Y, back -Y and side -X (left). Only the side clip is reused in
/// reverse, for strafing right. Crouching maps these to the crouch clips.
///
/// Then water (0x5a3308): over 60% covered always holds the root pose, as does
/// any coverage while off the ground or sinking faster than 0.1. (v20 also
/// roots a wading player holding jump; remote players' triggers are not
/// replicated, so that clause is left out.)
pub fn locomotion(player: &PlayerState, coverage: f32) -> LocomotionAction {
    let forward = |sequence| LocomotionAction {
        sequence,
        forward: true,
    };
    if player.jetting {
        return forward("root");
    }
    let crouched = player.crouched && !player.posture.prone;
    let root = if crouched { "crouch" } else { "root" };
    if coverage > 0.6 || (coverage > 0.01 && (!player.grounded || player.velocity[1] < -0.1)) {
        return forward(root);
    }
    if !player.grounded {
        return forward(if player.velocity[1] < -10.0 {
            "fall"
        } else if player.velocity[1] > 0.5 {
            "jump"
        } else {
            "root"
        });
    }
    let facing = Vec3::new(player.yaw.sin(), 0.0, -player.yaw.cos());
    let right = Vec3::new(player.yaw.cos(), 0.0, player.yaw.sin());
    let velocity = Vec3::from(player.velocity);
    let (f, r) = (velocity.dot(facing), velocity.dot(right));
    let [root, run, back, side] = if crouched {
        ["crouch", "crouchrun", "crouchback", "crouchside"]
    } else {
        ["root", "run", "back", "side"]
    };
    let mut action = forward(root);
    if velocity.length() < 0.4 {
        return action;
    }
    let mut best = 0.1;
    for (sequence, d) in [(run, f), (back, -f), (side, -r)] {
        if d > best + PICK_TIE {
            best = d;
            action = forward(sequence);
        } else if sequence == side && -d > best + PICK_TIE {
            best = -d;
            action = LocomotionAction {
                sequence,
                forward: false,
            };
        }
    }
    action
}
impl AvatarMesh {
    /// The same sampled node matrices used by this frame's visible character.
    /// Missing nodes remain missing; attachments must not invent a hand offset.
    /// The player's object transform: feet position and body yaw.
    pub fn body_transform(&self) -> Mat4 {
        self.model_transform
    }
    /// The action thread's sequence, and whether it is still blending in
    /// from the previous one (`transitionToSequence`).
    pub fn action(&self) -> (&'static str, bool) {
        (self.mode, self.transition.is_some())
    }
    /// While the body blends up out of the `sit` pose, how much of the
    /// sit is left: 1 as it leaves the pose, falling to 0 over one action
    /// transition, whatever actions cut in meanwhile. `None` once upright.
    pub fn getting_up(&self) -> Option<f32> {
        let elapsed = self.last_time? - self.stood_at?;
        (elapsed < TRANSITION_TIME).then(|| (1.0 - elapsed / TRANSITION_TIME) as f32)
    }
    /// A node's last posed transform relative to the model (feet, facing
    /// -Z, unscaled), for placing something on it before this frame's pose.
    pub fn model_node(&self, assets: &AvatarAssets, name: &str) -> Option<Mat4> {
        let index = assets.for_mesh(self).node(name)?;
        self.posed_nodes.get(index).copied()
    }
    pub fn world_node(&self, assets: &AvatarAssets, name: &str) -> Option<Mat4> {
        let index = assets.for_mesh(self).node(name)?;
        self.posed_nodes
            .get(index)
            .map(|node| self.model_transform * *node)
    }
    /// `world_node` as the animation poses it, whatever Add-On code draws:
    /// what the player sees from and aims with.
    pub fn animated_world_node(&self, assets: &AvatarAssets, name: &str) -> Option<Mat4> {
        let index = assets.for_mesh(self).node(name)?;
        let nodes = if self.animated_nodes.is_empty() {
            &self.posed_nodes
        } else {
            &self.animated_nodes
        };
        nodes.get(index).map(|node| self.model_transform * *node)
    }
    /// Whether Add-On code posed this frame's body.
    pub fn posed_externally(&self) -> bool {
        !self.animated_nodes.is_empty()
    }
    /// How far from where the game has it this body is drawn: Add-On code
    /// moved it (a ragdoll sliding away from where its player died). Once
    /// the body is gone (`remember_drawn_offset`) it is where it was last
    /// seen, until a new body spawns. `None` while the game animates it.
    pub fn drawn_offset(&self) -> Option<Vec3> {
        self.drawn_offset
    }
    /// Note this frame's [`Self::drawn_offset`]. A `gone` body (a corpse
    /// past its timeout, no longer drawn) keeps the last one: its camera
    /// and the puff it vanishes in stay where it was seen.
    pub fn remember_drawn_offset(&mut self, gone: bool) {
        if !gone {
            self.drawn_offset = self.posed_offset();
        }
    }
    /// Whether `point` is on or round this body where the game has it (the
    /// sphere [`Self::bounding_sphere`] draws round a body it animates).
    pub fn holds(&self, point: Vec3) -> bool {
        let scale = self.model_transform.x_axis.truncate().length();
        let feet = self.model_transform.w_axis.truncate();
        point.distance(feet + Vec3::Y * (1.4 * scale)) <= 3.0 * scale
    }
    /// The middle of the drawn nodes less the middle of the animated ones,
    /// while Add-On code poses the body.
    fn posed_offset(&self) -> Option<Vec3> {
        if !self.posed_externally() {
            return None;
        }
        let middle = |nodes: &[Mat4]| {
            let sum: Vec3 = nodes
                .iter()
                .map(|node| (self.model_transform * *node).w_axis.truncate())
                .sum();
            sum / nodes.len().max(1) as f32
        };
        Some(middle(&self.posed_nodes) - middle(&self.animated_nodes))
    }
    /// A sphere round the drawn body (centre, radius), for culling.
    /// Where openings carry the body: the middle of a standing body of its
    /// scale, as replicated bodies are carried.
    pub fn middle(&self) -> Vec3 {
        let scale = self.model_transform.x_axis.truncate().length();
        self.model_transform.w_axis.truncate() + Vec3::Y * bri_sim::player::nominal_middle(scale)
    }
    /// What carries the mesh's vertices into the world: the body transform
    /// for an `instanced` mesh, nothing for one built in world space.
    pub fn drawn_transform(&self) -> Mat4 {
        if self.instanced {
            self.model_transform
        } else {
            Mat4::IDENTITY
        }
    }
    pub fn bounding_sphere(&self) -> (Vec3, f32) {
        let scale = self.model_transform.x_axis.truncate().length();
        let feet = self.model_transform.w_axis.truncate();
        if !self.posed_externally() {
            return (feet + Vec3::Y * (1.4 * scale), 3.0 * scale);
        }
        let points: Vec<Vec3> = self
            .posed_nodes
            .iter()
            .map(|node| (self.model_transform * *node).w_axis.truncate())
            .collect();
        let center = points.iter().sum::<Vec3>() / points.len().max(1) as f32;
        let reach = points
            .iter()
            .map(|p| p.distance(center))
            .fold(0.0, f32::max);
        (center, reach + 1.5 * scale)
    }
    /// The body as drawn, for Add-On code: every node's world transform,
    /// the rig's node tree and the drawn geometry on each node.
    pub fn skeleton(&mut self, assets: &AvatarAssets) -> Skeleton {
        let assets = assets.for_mesh(self);
        let bounds = self
            .node_bounds
            .get_or_insert_with(|| Arc::new(node_bounds(assets, &self.outfit)))
            .clone();
        Skeleton {
            rig: assets.tree.clone(),
            nodes: self
                .posed_nodes
                .iter()
                .map(|node| (self.model_transform * *node).to_cols_array())
                .collect(),
            bounds,
        }
    }
    /// Place `nodes` (index, world position, world rotation) for Add-On
    /// code, after this frame's animation. Nodes it leaves out keep their
    /// animated place relative to their parent, or, with no placed node
    /// above them, relative to the placed node they are drawn nearest
    /// ([`follow_anchors`]); each keeps its animated scale. A node put further than [`MAX_POSE_REACH`] from the feet, or
    /// one the rig does not have, stays animated. The drawn mesh follows
    /// for bodies built at upload (`defer_mesh`), as every player's is.
    pub fn override_nodes(
        &mut self,
        assets: &AvatarAssets,
        nodes: &[bri_client_sandbox::bodies::PosedNode],
    ) {
        let assets = assets.for_mesh(self);
        let parents: Vec<Option<usize>> = assets.rig.shape.nodes.iter().map(|n| n.parent).collect();
        let count = self.posed_nodes.len();
        if count != parents.len() || nodes.is_empty() {
            return;
        }
        let model = self.model_transform;
        let feet = model.w_axis.truncate();
        let inverse = model.inverse();
        if !inverse.is_finite() {
            return;
        }
        let mut placed: Vec<Option<(Vec3, Quat)>> = vec![None; count];
        for (node, position, rotation) in nodes {
            let (position, rotation) = (Vec3::from(*position), Quat::from_array(*rotation));
            if let Some(slot) = placed.get_mut(*node as usize)
                && position.is_finite()
                && rotation.is_finite()
                && position.distance(feet) <= MAX_POSE_REACH
            {
                *slot = Some((position, rotation.normalize()));
            }
        }
        if placed.iter().all(Option::is_none) {
            return;
        }
        // Parents before children, however the rig orders its nodes.
        let depth = |mut i: usize| {
            let mut d = 0;
            while let Some(p) = parents[i] {
                i = p;
                d += 1;
                if d > count {
                    break;
                }
            }
            d
        };
        let mut order: Vec<usize> = (0..count).collect();
        order.sort_by_key(|i| depth(*i));
        let animated = std::mem::take(&mut self.posed_nodes);
        let bounds = self
            .node_bounds
            .get_or_insert_with(|| Arc::new(node_bounds(assets, &self.outfit)))
            .clone();
        let anchors = follow_anchors(&parents, &order, &placed, &animated, &bounds);
        // Tests turn the anchors off to prove their checks catch it.
        #[cfg(test)]
        let anchors = if tests::FOLLOW_ANCHORS.get() {
            anchors
        } else {
            vec![None; anchors.len()]
        };
        let mut posed = animated.clone();
        let mut moved = vec![false; count];
        for i in order {
            if let Some((position, rotation)) = placed[i] {
                let (scale, _, _) = (model * animated[i]).to_scale_rotation_translation();
                posed[i] =
                    inverse * Mat4::from_scale_rotation_translation(scale, rotation, position);
                moved[i] = true;
            } else if let Some(p) = parents[i].filter(|p| moved[*p] && anchors[i].is_none()) {
                posed[i] = posed[p] * (animated[p].inverse() * animated[i]);
                moved[i] = true;
            } else if let Some(a) = anchors[i] {
                // Rides with its anchor as it was placed.
                let (position, rotation) = placed[a].expect("anchors are placed");
                let (scale, _, _) = (model * animated[a]).to_scale_rotation_translation();
                let at = inverse * Mat4::from_scale_rotation_translation(scale, rotation, position);
                posed[i] = at * (animated[a].inverse() * animated[i]);
                moved[i] = true;
            }
        }
        if !posed.iter().all(|m| m.is_finite()) {
            self.posed_nodes = animated;
            return;
        }
        self.posed_nodes = posed;
        if let Some(pending) = &mut self.pending {
            pending.nodes.clone_from(&self.posed_nodes);
        }
        self.animated_nodes = animated;
    }
    /// How the player's archetype fits this mesh. A change re-resolves the
    /// animation roles at the next pose.
    pub fn set_body_kind(&mut self, kind: BodyKind) {
        if self.kind != kind {
            if self.kind.crouch_composition != kind.crouch_composition
                || self.kind.stand_in != kind.stand_in
            {
                self.crouch = CrouchThread::default();
            }
            self.kind = kind;
            self.roles = None;
            self.fits = None;
        }
    }
    pub fn body_kind(&self) -> &BodyKind {
        &self.kind
    }
    /// The body's clip for each role: the archetype's names (a stand-in
    /// keeps the Blockhead's), then the Blockhead's, then the fallbacks.
    fn roles(&mut self, assets: &AvatarAssets) -> Arc<RoleClips> {
        let kind = &self.kind;
        self.roles
            .get_or_insert_with(|| {
                let none = Animations::new();
                let animations = if kind.stand_in {
                    &none
                } else {
                    &kind.animations
                };
                Arc::new(RoleClips::resolve(animations, |clip| {
                    assets.rig.sequence(clip).is_some()
                }))
            })
            .clone()
    }
    /// How the body holds with each hand ([`Fits`]): nothing for the
    /// Blockhead, a v20 body or a stand-in. Its aim pose is the host's
    /// (`bri_sim::session::body_grips`): idle, the hand's aim clip at its
    /// end and the look level, with the arm raised where it has no aim clip.
    fn fits(&mut self, assets: &AvatarAssets) -> Arc<Fits> {
        if let Some(fits) = &self.fits {
            return fits.clone();
        }
        let roles = self.roles(assets);
        let assets = assets.for_mesh(self);
        let mut fits = Fits::default();
        if assets.fitted && !self.kind.stand_in {
            let clip = |role: Role| assets.rig.sequence(roles.clip(role)?.0);
            let parents: Vec<Option<usize>> =
                assets.rig.shape.nodes.iter().map(|n| n.parent).collect();
            let ends = [0, 1].map(|n| self.mount_index(assets, n));
            for (hand, aim) in [Role::AimRight, Role::AimLeft].into_iter().enumerate() {
                if self.kind.sockets.preserve_hand_pose {
                    continue;
                }
                let Some(end) = ends[hand] else {
                    continue;
                };
                let mut layers: Vec<Layer<'_>> = [
                    (clip(Role::Idle), 0.0),
                    (clip(aim), 1.0),
                    (clip(Role::Look), 0.5),
                ]
                .into_iter()
                .filter_map(|(animation, at)| {
                    let animation = animation?;
                    Some(Layer {
                        animation,
                        time: animation.duration * at,
                        weight: 1.0,
                    })
                })
                .collect();
                layers.sort_by_key(|l| (l.animation.additive, l.animation.priority));
                let Ok(mut pose) =
                    bri_content::animation::sample_layers(&assets.rig.shape, &layers)
                else {
                    continue;
                };
                let landmark = ends[1 - hand].or_else(|| assets.node("eye"));
                if let Some(fit) = bri_content::hold::fit(
                    &parents,
                    &mut pose.nodes,
                    end,
                    landmark,
                    clip(aim).is_none(),
                    hand == 0,
                ) {
                    fits.hands[hand] = Some(fit);
                }
            }
            fits.first_person = std::array::from_fn(|n| {
                if self.kind.sockets.preserve_hand_pose {
                    ends[n].is_some()
                } else {
                    fits.hands[n].is_some()
                }
            });
            let hip = bri_content::hold::sit_hip(&assets.rig.shape, clip(Role::Sit));
            fits.seat = assets
                .seat_hip
                .zip(hip)
                .map(|(blockhead, own)| blockhead - own)
                .filter(|shift| shift.is_finite() && shift.length() < 8.0);
        }
        let fits = Arc::new(fits);
        self.fits = Some(fits.clone());
        fits
    }
    /// What the body lacks for the archetype drawn with it, one line each:
    /// a socket whose node the rig does not have (or, on a box body, any
    /// socket that names a node), and the animation roles it has no clip
    /// for. Empty for the Blockhead.
    pub fn body_report(&mut self, assets: &AvatarAssets) -> Vec<String> {
        let assets = assets.for_mesh(self);
        let roles = self.roles(assets);
        let mut lines = self.kind.report(Some(&roles));
        if !self.kind.stand_in {
            let sockets = &self.kind.sockets;
            for (name, socket) in [
                ("right_hand", sockets.hand(0)),
                ("left_hand", sockets.hand(1)),
                ("eye", Some(sockets.eye())),
                ("seat", sockets.seat.clone()),
            ] {
                if let Some(node) = socket.and_then(|s| s.node)
                    && assets.node(&node).is_none()
                {
                    lines.push(format!(
                        "its model has no `{node}` node for the `{name}` socket, so {}",
                        match name {
                            "eye" => "it sees from its archetype's eye height",
                            "seat" => "the seat socket is unused",
                            _ => "items held in that hand are hidden",
                        }
                    ));
                }
            }
        }
        lines
    }
    /// Clips the last pose asked for that the body does not have (a
    /// script's `playThread` of a clip it lacks), left out of the pose.
    pub fn skipped_clips(&self) -> &[String] {
        &self.skipped
    }
    /// The node index of image mount point `n`: the hand sockets' nodes
    /// for 0 and 1, `Mount<n>` otherwise. None on a stand-in, whose nodes
    /// are not the body's.
    fn mount_index(&self, assets: &AvatarAssets, n: usize) -> Option<usize> {
        if self.kind.stand_in {
            return None;
        }
        let assets = assets.for_mesh(self);
        match self.kind.sockets.hand(n as u32) {
            Some(socket) => assets.node(socket.node.as_deref()?),
            None => *assets.mount_nodes.get(n)?,
        }
    }
    /// `world_node` of image mount point `n` (n below 32): a hand socket's
    /// node with its offset for 0 and 1, `Mount<n>` for the others. None
    /// when the body has no such node (a box body has none).
    pub fn mount_node(&self, assets: &AvatarAssets, n: usize) -> Option<Mat4> {
        let index = self.mount_index(assets, n)?;
        let offset = match self.kind.sockets.hand(n as u32) {
            Some(socket) => Vec3::from(socket.offset),
            None => Vec3::ZERO,
        };
        let node = *self.posed_nodes.get(index)?;
        let hand = match self.held_hand(n) {
            // Where the arm put it, pointing along the aim.
            Some(aim) => Mat4::from_rotation_translation(aim, node.w_axis.truncate()),
            None => node * self.hand_turn(n),
        };
        Some(
            self.model_transform
                * hand
                * Mat4::from_translation(offset)
                * self
                    .tool_only_actions
                    .get(n)
                    .copied()
                    .flatten()
                    .unwrap_or(Mat4::IDENTITY),
        )
    }
    /// A fitted body's hand `n` while it holds something up: how it turns
    /// in the body's frame, along the aim ([`bri_content::hold::Raise::aim`]).
    fn held_hand(&self, n: usize) -> Option<Quat> {
        let (held, pitch) = self.held;
        let fit = self.fits.as_ref()?.hands.get(n)?.as_ref()?;
        (*held.get(n)? && fit.hold.is_some()).then(|| bri_content::hold::Raise::aim(pitch))
    }
    /// The turn after a fitted body's hand socket node that lines it up
    /// with the Blockhead's `Mount0` ([`bri_content::hold::Fit`]).
    fn hand_turn(&self, n: usize) -> Mat4 {
        self.fits
            .as_ref()
            .and_then(|f| f.hands.get(n)?.as_ref())
            .map_or(Mat4::IDENTITY, |fit| Mat4::from_quat(fit.correction))
    }
    /// Where a fitted body's hand `n` holds in first person, in the eye's
    /// frame (the camera, scaled with the body): where the Blockhead's
    /// aiming hand is from its eye ([`bri_content::hold::FIRST_PERSON_HAND`]).
    /// A first-person image without an eye offset is drawn there, so a
    /// gun sits on screen as it does for a Blockhead and turns with the
    /// view, whatever the body's size or arm animation.
    /// `None` for the Blockhead and v20 bodies, which hold in the hand.
    pub fn first_person_hand(&self, n: usize) -> Option<Mat4> {
        if !*self.fits.as_ref()?.first_person.get(n)? {
            return None;
        }
        let offset = self
            .kind
            .sockets
            .hand(n as u32)
            .map_or(Vec3::ZERO, |s| Vec3::from(s.offset));
        let mut at = bri_content::hold::FIRST_PERSON_HAND;
        if n == 1 {
            at.x = -at.x;
        }
        let scale = self.model_transform.x_axis.truncate().length();
        let local = Mat4::from_scale(Vec3::splat(scale))
            * Mat4::from_rotation_translation(bri_content::hold::AIM_HAND, at)
            * Mat4::from_translation(offset);
        local.is_finite().then_some(local)
    }
    /// Where image mount point `n` attaches in the world. A hand socket
    /// that is an offset (a box body's hand) holds what it carries pitched
    /// with the aim, as a raised arm would; one on a node follows the node
    /// as the arm animates. `None` when the body has nowhere for it.
    pub fn mount_socket(&self, assets: &AvatarAssets, n: usize, aim_pitch: f32) -> Option<Mat4> {
        match self.kind.sockets.hand(n as u32) {
            Some(socket) if socket.node.is_none() && aim_pitch.is_finite() => Some(
                self.seated_frame()
                    * Mat4::from_translation(Vec3::from(socket.offset))
                    * Mat4::from_rotation_x(aim_pitch)
                    * self
                        .tool_only_actions
                        .get(n)
                        .copied()
                        .flatten()
                        .unwrap_or(Mat4::IDENTITY),
            ),
            _ => self.mount_node(assets, n),
        }
    }
    /// How the playing thread-2/3 actions (a brick shift, plant, recoil,
    /// swing) move `Mount<n>`, in that mount's own frame: the rest of the
    /// pose (locomotion, armReady, look) is left out. None while no action
    /// moves it. A first-person image rides this on its eye offset.
    pub fn mount_action(&self, assets: &AvatarAssets, n: usize) -> Option<Mat4> {
        if let Some(action) = self.tool_only_actions.get(n).copied().flatten() {
            return Some(action);
        }
        let index = self.mount_index(assets, n)?;
        let rest = *self.unacted_nodes.get(index)? * self.hand_turn(n);
        let moved = rest.inverse() * *self.posed_nodes.get(index)? * self.hand_turn(n);
        (moved.is_finite() && !moved.abs_diff_eq(Mat4::IDENTITY, 1e-6)).then_some(moved)
    }
    /// How far into the `sit` pose the body is: 1 seated, easing from 0
    /// over one action transition as it sits down and back as it gets up.
    pub fn sit_weight(&self) -> f32 {
        if self.mode == SIT {
            match (&self.transition, self.last_time) {
                (Some((_, start)), Some(now)) => {
                    ((now - start) / TRANSITION_TIME).clamped(0.0, 1.0) as f32
                }
                _ => 1.0,
            }
        } else {
            self.getting_up().unwrap_or(0.0)
        }
    }
    /// A stand-in body's model frame with the sit applied: lowered and set
    /// back by as much as its archetype's seated eye is from its standing
    /// one, so the drawn head is where the player sees from. Any other
    /// body's model frame as posed.
    pub fn seated_frame(&self) -> Mat4 {
        if !self.kind.stand_in {
            return self.model_transform;
        }
        let shift = (self.kind.sit_eye - self.kind.eye) * self.sit_weight();
        self.model_transform * Mat4::from_translation(shift)
    }
    /// How a stand-in's box body is drawn: the seated frame, and while it
    /// sits, the boxes below its `seat` socket (the hip) folded forward
    /// with the sit. A body with no seat offset sits without folding.
    pub fn box_pose(&self) -> crate::packages::BoxPose {
        let hip = self
            .kind
            .sockets
            .seat
            .as_ref()
            .filter(|s| s.node.is_none())
            .map(|s| Vec3::from(s.offset));
        crate::packages::BoxPose {
            frame: self.seated_frame(),
            hip,
            fold: std::f32::consts::FRAC_PI_2 * self.sit_weight(),
        }
    }
    /// The body's eye in its model frame (feet, facing -Z, unscaled): the
    /// eye socket's node and offset, else the archetype's eye, seated as
    /// the body sits. `animated`: as the animation poses it, whatever
    /// Add-On code draws.
    pub fn eye_local(&self, assets: &AvatarAssets, animated: bool) -> Option<Vec3> {
        let socket = self.kind.sockets.eye();
        let offset = Vec3::from(socket.offset);
        if !self.kind.stand_in
            && let Some(index) = socket
                .node
                .as_deref()
                .and_then(|n| assets.for_mesh(self).node(n))
        {
            let nodes = if animated && !self.animated_nodes.is_empty() {
                &self.animated_nodes
            } else {
                &self.posed_nodes
            };
            return nodes
                .get(index)
                .map(|node| (*node * Mat4::from_translation(offset)).w_axis.truncate());
        }
        let standing = if socket.node.is_none() {
            offset
        } else {
            self.kind.eye
        };
        let seated = standing + (self.kind.sit_eye - self.kind.eye);
        Some(standing.lerp(seated, self.sit_weight()))
    }
    /// [`Self::eye_local`] in the world.
    pub fn eye_world(&self, assets: &AvatarAssets, animated: bool) -> Option<Vec3> {
        let local = self.eye_local(assets, animated)?;
        Some(self.model_transform.transform_point3(local))
    }
    /// Engine-style world eye transform. The original Eye node supplies its
    /// sampled world position; the player supplies view rotation. The native
    /// rig's Eye basis cancels in bind pose, but it is not animated by `look`,
    /// so using its raw orientation would leave first-person image offsets
    /// level while the player's pitch changes.
    pub fn eye_transform(
        &self,
        assets: &AvatarAssets,
        view_yaw: f32,
        view_pitch: f32,
    ) -> Option<Mat4> {
        if !view_yaw.is_finite() || !view_pitch.is_finite() {
            return None;
        }
        let eye_position = self.eye_world(assets, true)?;
        let rotation = Quat::from_rotation_y(-view_yaw) * Quat::from_rotation_x(view_pitch);
        Some(Mat4::from_rotation_translation(rotation, eye_position))
    }
    pub fn pose(&mut self, assets: &AvatarAssets, player: &PlayerState, time: f64) -> Result<()> {
        self.pose_with_animation(assets, player, time, &AvatarAnimationInput::default())
    }

    /// Samples movement, the original authored held-arm pose, and an optional
    /// original action animation into one pose shared by visible geometry and
    /// `world_node` attachments.
    pub fn pose_with_animation(
        &mut self,
        assets: &AvatarAssets,
        player: &PlayerState,
        time: f64,
        animation_input: &AvatarAnimationInput,
    ) -> Result<()> {
        let reference_assets = assets;
        let assets = assets.for_mesh(self);
        let roles = self.roles(assets);
        self.skipped.clear();
        self.tool_only_actions = [None; 2];
        // A role's clip, played reversed when it stands in for walking
        // back; `None` leaves the layer out.
        let role = |role: Role| -> Option<(&bri_content::shape::Animation, bool)> {
            let (clip, reversed) = roles.clip(role)?;
            Some((assets.rig.sequence(clip)?, reversed))
        };
        // A clip asked for by the Blockhead's name (the engine's actions, a
        // script's `playThread`): the body's clip for that role, or the
        // body's own clip of that name.
        let named = |name: &str| -> Option<&bri_content::shape::Animation> {
            match Role::from_blockhead_clip(name) {
                Some(r) => role(r).map(|(clip, _)| clip),
                None => assets.rig.sequence(name),
            }
        };
        ensure!(
            time.is_finite()
                && animation_input
                    .body
                    .iter()
                    .chain([&animation_input.action, &animation_input.gesture])
                    .flatten()
                    .all(|action| action.started_at.is_finite() && !action.sequence.is_empty())
                && player
                    .feet
                    .iter()
                    .chain(&player.velocity)
                    .chain([&player.yaw, &player.pitch])
                    .all(|v| v.is_finite()),
            "Invalid avatar pose input"
        );
        let first = self.last_time.is_none();
        let elapsed = self
            .last_time
            .map_or(0.0, |last| (time - last).clamped(0.0, 0.25) as f32);
        self.last_time = Some(time);
        let movement = animation_input.tick_state.as_ref().unwrap_or(player);
        let scripted = if animation_input.dead {
            // The archetype's death clip for how the body fell, else the
            // `death` role's.
            Some(
                animation_input
                    .death
                    .filter(|_| !self.kind.stand_in)
                    .and_then(|death| {
                        use bri_content::body::Side;
                        use bri_sim::session::HitSide;
                        let side = death.from.map(|s| match s {
                            HitSide::Front => Side::Front,
                            HitSide::Back => Side::Back,
                            HitSide::Left => Side::Left,
                            HitSide::Right => Side::Right,
                        });
                        self.kind
                            .deaths
                            .pick(side, death.roll, |clip| assets.rig.sequence(clip).is_some())
                    })
                    .map_or("death1", |clip| interned(&clip)),
            )
        } else if animation_input.sitting {
            Some(SIT)
        } else {
            None
        };
        // v20 picks every client frame: it kept `delayTicks` but dropped the
        // test that would hold an action for `sNewAnimationTickTime`.
        let next = Some(scripted.map_or_else(
            || {
                if self.model.is_some()
                    && animation_input.water_coverage > 0.0
                    && role(Role::Swim).is_some()
                {
                    LocomotionAction {
                        sequence: "swim",
                        forward: true,
                    }
                } else if let Some(clip) = animation_input
                    .intent_clip
                    .as_deref()
                    .filter(|clip| assets.rig.sequence(clip).is_some())
                {
                    LocomotionAction {
                        sequence: interned(&clip.to_ascii_lowercase()),
                        forward: true,
                    }
                } else {
                    let mut action = locomotion(movement, animation_input.water_coverage);
                    if movement.jump.jet_flight && action.sequence == "jump" {
                        action.sequence = if movement.crouched { "crouch" } else { "root" };
                    }
                    action
                }
            },
            |sequence| LocomotionAction {
                sequence,
                forward: true,
            },
        ));
        // `Player::setActionThread` ignores a request for the running action,
        // even in the other play direction.
        let next = next.filter(|action| action.sequence != self.mode);
        let movement_role =
            |sequence: &str| Role::from_blockhead_clip(sequence).unwrap_or(Role::Idle);
        // The body's clip for the movement; one standing in for walking
        // back plays reversed. A clip the body names itself (an intent's,
        // a death's) plays as it is. None: the body has no clip even to
        // stand in, and holds its bind pose under the other layers.
        let sequence = next.map_or(self.mode, |a| a.sequence);
        let seat_pose = animation_input.seat_pose.as_deref().filter(|_| {
            (sequence == SIT || sequence == "root")
                && animation_input.mount_rotation.is_some()
                && !animation_input.dead
        });
        let authored_seat = seat_pose.and_then(|name| assets.rig.sequence(name));
        let own = authored_seat.or_else(|| {
            Role::from_blockhead_clip(sequence)
                .is_none()
                .then(|| assets.rig.sequence(sequence))
                .flatten()
        });
        let (clip, reversed) = match own
            .map(|c| (c, false))
            .or_else(|| role(movement_role(sequence)))
        {
            Some((clip, reversed)) => (Some(clip), reversed),
            None => (None, false),
        };
        let duration = clip.map_or(0.0, |c| c.duration);
        if let Some(action) = next {
            if action.sequence == SIT {
                self.stood_at = None;
            } else if self.mode == SIT {
                self.stood_at = Some(time);
            }
            self.transition = self
                .channels
                .take()
                .filter(|_| !first)
                .map(|channels| (channels, time));
            self.mode = action.sequence;
            self.forward = action.forward != reversed;
            self.phase = if self.forward { 0.0 } else { duration };
        } else {
            // A locomotion clip plays at the speed the body moves.
            let rate = clip.map_or(1.0, |clip| {
                let local = Quat::from_rotation_y(movement.body_yaw())
                    * Vec3::from(movement.velocity)
                    / movement.scale.max(0.01);
                self.kind.time_scale(clip, movement_role(self.mode), local)
            });
            if self.forward {
                self.phase += elapsed * rate;
            } else {
                self.phase -= elapsed * rate;
            }
        }
        let mode = self.mode;
        if clip.is_some_and(|c| c.looping) && duration > 0.0 {
            self.phase = self.phase.rem_euclid(duration);
        } else {
            self.phase = self.phase.clamped(0.0, duration);
        }
        let ready_role = match animation_input.held_tool_pose {
            HeldToolPose::None => None,
            HeldToolPose::Right => Some(Role::AimRight),
            HeldToolPose::Left => Some(Role::AimLeft),
            HeldToolPose::Both => Some(Role::AimBoth),
        };
        let seat_owns_ready = authored_seat.is_some()
            && ready_role
                .is_some_and(|role| animation_input.seat_ready.covers_resolved(role, &roles));
        let crouch = if (self.kind.stand_in
            || self.kind.crouch_composition == CrouchComposition::Overlay)
            && let Some((crouch, _)) = role(Role::Crouch)
        {
            self.crouch
                .update(player.crouched, elapsed, crouch.duration);
            self.crouch.time().map(|time| Layer {
                animation: crouch,
                time,
                weight: 1.,
            })
        } else {
            None
        };
        let actions = [
            &animation_input.body[0],
            &animation_input.body[1],
            &animation_input.action,
            &animation_input.gesture,
        ]
        .map(|action| {
            action
                .as_ref()
                .map(|action| bri_content::body_pose::ActionRef {
                    sequence: action.sequence.as_str(),
                    started_at: action.started_at,
                })
        });
        let bri_content::body_pose::BodyLayerPlan {
            layers,
            unacted,
            transition_at: at,
            unacted_transition_at,
            acted,
            skipped,
        } = bri_content::body_pose::plan_body_layers(
            &assets.rig,
            &roles,
            bri_content::body_pose::LayerPlanInput {
                base: clip.map(|clip| Layer {
                    animation: clip,
                    time: self.phase,
                    weight: 1.,
                }),
                ready_role,
                omit_ready: seat_owns_ready,
                crouch,
                actions: &actions,
                time,
                head_up: self.outfit.head_up,
                seat_aim: animation_input.seat_aim,
                look_phase: look_position(player.pitch, animation_input.look_limits),
                head_yaw: player.head_yaw,
            },
        )?;
        self.skipped.extend(skipped);
        // A foreign body need not borrow a punch/throw to use a stock tool.
        // Sample only the reference tool action's socket displacement, keeping
        // every vertex and bone of the foreign body on its own animation.
        if animation_input.held_tool_pose != HeldToolPose::None
            && let Some(action) = &animation_input.action
            && Role::from_blockhead_clip(&action.sequence).is_none()
            && named(&action.sequence).is_none()
            && let Some(clip) = reference_assets.rig.sequence(&action.sequence)
        {
            let mut base = Vec::new();
            if let Some(ready) = reference_assets.rig.sequence("armreadyright") {
                base.push(Layer {
                    animation: ready,
                    time: ready.duration,
                    weight: 1.0,
                });
            }
            let rest = bri_content::animation::sample_layers(&reference_assets.rig.shape, &base)?;
            base.push(Layer {
                animation: clip,
                time: (time - action.started_at).max(0.0) as f32,
                weight: 1.0,
            });
            base.sort_by_key(|layer| (layer.animation.additive, layer.animation.priority));
            let moved = bri_content::animation::sample_layers(&reference_assets.rig.shape, &base)?;
            for (hand, index) in reference_assets.mount_nodes.iter().take(2).enumerate() {
                if let Some(index) = index {
                    let delta = rest.nodes[*index].inverse() * moved.nodes[*index];
                    if delta.is_finite() && !delta.abs_diff_eq(Mat4::IDENTITY, 1e-6) {
                        self.tool_only_actions[hand] = Some(delta);
                    }
                }
            }
        }
        let transition_time = if mode == "jump" {
            JUMP_TRANSITION_TIME
        } else {
            TRANSITION_TIME
        };
        if self
            .transition
            .as_ref()
            .is_some_and(|(_, start)| time - start >= transition_time)
        {
            self.transition = None;
        }
        let from = self.transition.as_ref().map(|(channels, start)| {
            let progress = ((time - start) / transition_time).clamped(0.0, 1.0);
            (channels, 1.0 - progress as f32)
        });
        let (pose, channels) = sample_layers_with_transition(&assets.rig.shape, &layers, at, from)?;
        self.unacted_nodes.clear();
        if acted && self.wants_unacted {
            let (rest, _) = sample_layers_with_transition(
                &assets.rig.shape,
                &unacted,
                unacted_transition_at,
                from,
            )?;
            self.unacted_nodes = rest.nodes;
        }
        self.channels = Some(channels);
        // A fitted body raises an arm it has no aim clip for, and puts its
        // off hand on what it holds in both.
        let raised = match animation_input.held_tool_pose {
            HeldToolPose::None => [false; 2],
            HeldToolPose::Right => [true, false],
            HeldToolPose::Left => [false, true],
            HeldToolPose::Both => [true; 2],
        };
        // An authored hand action (reload, throw, etc.) owns its off hand.
        // Static support IK must not pull it back onto the foregrip mid-action.
        let hand_action = animation_input.action.as_ref().is_some_and(|action| {
            named(&action.sequence).is_some_and(|clip| {
                clip.looping || (time - action.started_at).max(0.0) < f64::from(clip.duration)
            })
        });
        let support = animation_input
            .support
            .filter(|_| animation_input.held_tool_pose == HeldToolPose::Both && !hand_action);
        let seat_shift = seat_pose.and_then(|_| {
            bri_content::body_pose::seat_root_shift(
                &assets.rig.shape,
                &pose,
                authored_seat.is_some(),
                animation_input.seat_fallback_hip,
                Vec3::from(player.feet),
                animation_input.mount_rotation,
                player.scale,
            )
        });
        self.fits(assets);
        self.finish_pose(
            assets,
            player,
            pose,
            animation_input.mount_rotation,
            animation_input.lean_tilt,
            Holds {
                raised,
                pitch: player.pitch,
                support,
                seat: animation_input.hand_holds,
                seat_shift,
            },
        )
    }

    fn finish_pose(
        &mut self,
        assets: &AvatarAssets,
        player: &PlayerState,
        mut pose: bri_content::animation::Pose,
        mount_rotation: Option<Quat>,
        lean_tilt: f32,
        holds: Holds,
    ) -> Result<()> {
        let hand_holds = holds.seat;
        // `setScale` scales the whole shape about the feet.
        let model_transform = Mat4::from_scale_rotation_translation(
            Vec3::splat(player.scale),
            mount_rotation
                .filter(|q| q.is_finite() && q.is_normalized())
                .unwrap_or_else(|| Quat::from_rotation_y(-player.body_yaw())),
            Vec3::from(player.feet),
        );
        // A rider sits with its hip where the Blockhead's is.
        let model_transform = match holds
            .seat_shift
            .or_else(|| self.fits.as_ref().and_then(|f| f.seat))
        {
            Some(shift) if mount_rotation.is_some() => {
                model_transform * Mat4::from_translation(shift)
            }
            // On foot a body leans and lies down as its posture says.
            _ if mount_rotation.is_none() && (player.posture.prone || lean_tilt != 0.0) => {
                Mat4::from_rotation_translation(
                    Quat::from_rotation_y(-player.body_yaw()),
                    Vec3::from(player.feet),
                ) * posture_frame(player, lean_tilt, 0.3 * player.scale)
                    * Mat4::from_scale(Vec3::splat(player.scale))
            }
            _ => model_transform,
        };
        self.model_transform = model_transform;
        self.held = (holds.raised, holds.pitch);
        self.fit_hands(assets, &mut pose.nodes, &holds);
        if hand_holds.iter().any(Option::is_some) {
            let inverse = model_transform.inverse();
            let ends = [0, 1].map(|n| self.mount_index(assets, n));
            let parents: Vec<Option<usize>> = assets
                .for_mesh(self)
                .rig
                .shape
                .nodes
                .iter()
                .map(|n| n.parent)
                .collect();
            if inverse.is_finite() && parents.len() == pose.nodes.len() {
                for n in 0..2 {
                    if let (Some(end), Some(hold)) = (ends[n], hand_holds[n])
                        && hold.is_finite()
                    {
                        let other = ends[1 - n].or_else(|| assets.for_mesh(self).node("eye"));
                        reach_hands(
                            &parents,
                            &mut pose.nodes,
                            end,
                            other,
                            inverse.transform_point3(hold),
                        );
                    }
                }
            }
        }
        self.posed_nodes.clone_from(&pose.nodes);
        self.animated_nodes.clear();
        if self.defer_mesh {
            self.pending = Some(pose);
            Ok(())
        } else {
            self.build_mesh(assets, &pose)
        }
    }
    /// A fitted body's arms after its clips: each held hand it has no aim
    /// clip for raised in front of the chest and pitched with the aim
    /// ([`bri_content::hold::Raise`]), then the left hand on the support
    /// point of what the right holds in both. Seat holds come after.
    fn fit_hands(&self, assets: &AvatarAssets, nodes: &mut [Mat4], holds: &Holds) {
        let Some(fits) = self.fits.clone() else {
            return;
        };
        if fits.hands.iter().all(Option::is_none) {
            return;
        }
        let assets = assets.for_mesh(self);
        let parents: Vec<Option<usize>> = assets.rig.shape.nodes.iter().map(|n| n.parent).collect();
        if parents.len() != nodes.len() {
            return;
        }
        let ends = [0, 1].map(|n| self.mount_index(assets, n));
        let landmark = |n: usize| ends[1 - n].or_else(|| assets.node("eye"));
        let pitch = if holds.pitch.is_finite() {
            holds.pitch
        } else {
            0.0
        };
        for (n, end) in ends.into_iter().enumerate() {
            if !holds.raised[n] || holds.seat[n].is_some() {
                continue;
            }
            let (Some(end), Some(fit)) = (end, &fits.hands[n]) else {
                continue;
            };
            if let Some(raise) = fit.hold
                && raise.shoulder < nodes.len()
            {
                let shoulder = nodes[raise.shoulder].w_axis.truncate();
                bri_content::hold::reach(
                    &parents,
                    nodes,
                    end,
                    landmark(n),
                    raise.target(shoulder, pitch),
                );
            }
        }
        if let (Some(point), Some(right), Some(left), Some(fit)) =
            (holds.support, ends[0], ends[1], &fits.hands[0])
            && holds.seat[1].is_none()
            && point.is_finite()
        {
            let offset = self
                .kind
                .sockets
                .hand(0)
                .map_or(Vec3::ZERO, |s| Vec3::from(s.offset));
            // The right hand as `mount_node` draws it.
            let hand = if holds.raised[0] && fit.hold.is_some() {
                Mat4::from_rotation_translation(
                    bri_content::hold::Raise::aim(pitch),
                    nodes[right].w_axis.truncate(),
                )
            } else {
                nodes[right] * Mat4::from_quat(fit.correction)
            };
            let grip = hand * Mat4::from_translation(offset);
            bri_content::hold::reach(
                &parents,
                nodes,
                left,
                Some(right),
                grip.transform_point3(point),
            );
        }
    }
    /// Write the posed vertices into `data`, reusing the last frame's
    /// structure when the drawn parts, frames and paint are unchanged.
    fn build_mesh(
        &mut self,
        assets: &AvatarAssets,
        pose: &bri_content::animation::Pose,
    ) -> Result<()> {
        let colors: Vec<_> = assets
            .object_names
            .iter()
            .map(|name| {
                self.outfit
                    .nodes
                    .get(name)
                    .copied()
                    .filter(|_| !self.hidden_nodes.contains(name))
            })
            .collect();
        let binding = crate::avatar_mesh::Binding {
            shape: &assets.rig.shape,
            detail: assets.detail_at(self.screen_height),
            materials: &self.materials,
            translucent_materials: &self.translucent_materials,
            unassigned_material: self.materials[0],
            colors: &colors,
        };
        if self.instanced
            && !self.restructured
            && self
                .drawn_pose
                .as_ref()
                .is_some_and(|drawn| same_pose(drawn, pose))
            && !crate::avatar_mesh::Layout::restructures(&self.layout, &self.data, &binding, pose)?
        {
            return Ok(());
        }
        let transform = if self.instanced {
            Mat4::IDENTITY
        } else {
            self.model_transform
        };
        let rebuilt = crate::avatar_mesh::Layout::pose(
            &mut self.layout,
            &mut self.data,
            &binding,
            pose,
            transform,
        )?;
        self.restructured |= rebuilt;
        self.vertices_dirty = true;
        if self.instanced {
            self.drawn_pose = Some(bri_content::animation::Pose {
                nodes: pose.nodes.clone(),
                visibility: pose.visibility.clone(),
                frames: pose.frames.clone(),
                material_frames: pose.material_frames.clone(),
            });
        }
        Ok(())
    }
    /// Takes over another mesh's action thread (sequence, direction, time
    /// and transition), for a rebuilt outfit of the same player.
    pub fn continue_animation(&mut self, old: &AvatarMesh) {
        self.mode = old.mode;
        self.forward = old.forward;
        self.phase = old.phase;
        self.last_time = old.last_time;
        self.channels.clone_from(&old.channels);
        self.transition.clone_from(&old.transition);
        self.stood_at = old.stood_at;
        self.crouch = old.crouch;
        self.body = old.body;
    }
    /// Follows the replicated body. A respawn is a new v20 `Player` object
    /// whose threads start at `root`, so a new body drops every running
    /// thread (the corpse's `death1` included) instead of blending out of
    /// it. Returns whether `body` replaced an earlier one.
    pub fn set_body(&mut self, body: u64) -> bool {
        let renewed = self.body.replace(body).is_some_and(|old| old != body);
        if renewed {
            self.mode = "root";
            self.forward = true;
            self.phase = 0.0;
            self.last_time = None;
            self.channels = None;
            self.transition = None;
            self.stood_at = None;
            self.crouch = CrouchThread::default();
            self.drawn_offset = None;
        }
        renewed
    }
    /// The drawn body's life, as Add-On code sees it: its spawn tick and
    /// whether it lies dead. None before a body is known.
    pub fn life(&self) -> Option<(u64, bool)> {
        self.body.map(|body| (body, self.dead))
    }
    /// Whether the drawn body lies dead ([`drawn_life`]).
    pub fn set_dead(&mut self, dead: bool) {
        self.dead = dead;
    }
    /// The body nodes the held images hide (`Image::hide_nodes`), shown
    /// again as soon as no held image names them.
    pub fn set_hidden_nodes(&mut self, nodes: impl IntoIterator<Item = String>) {
        let mut nodes: Vec<String> = nodes.into_iter().map(|n| n.to_ascii_lowercase()).collect();
        nodes.sort();
        nodes.dedup();
        self.hidden_nodes = nodes;
    }
    /// Build the mesh of a pose waiting from `defer_mesh`, if any.
    pub fn build_pending(&mut self, assets: &AvatarAssets) -> Result<()> {
        if let Some(pose) = self.pending.take() {
            let assets = assets.for_mesh(self);
            self.build_mesh(assets, &pose)?;
        }
        Ok(())
    }
    /// Forget everything this mesh holds on the GPU (the device is gone):
    /// the next [`Self::upload`] sends it all to the new one.
    pub fn gpu_stopped(&mut self) {
        self.gpu = None;
        self.instance = None;
    }
    /// Send the vertices to the GPU: only positions and normals while the
    /// structure holds, and only geometry when it changes (skis, hidden
    /// nodes, a translucent paint): the outfit's materials and textures
    /// (face, decal, colours) stay as uploaded for this appearance.
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        self.upload_sharing(renderer, device, queue, None)
    }
    /// Whether the next [`Self::upload`] sends the whole mesh (a new body or
    /// outfit): its materials and any textures not yet uploaded, not only
    /// vertices.
    pub fn needs_full_upload(&self) -> bool {
        self.gpu.is_none()
    }
    /// As [`Self::upload`]; a whole upload binds the images `textures`
    /// already holds (every body wears faces and decals from one catalog)
    /// and keeps the ones it makes there.
    pub fn upload_sharing(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textures: Option<&mut bri_render::scene::TextureCache>,
    ) -> Result<()> {
        if std::mem::take(&mut self.restructured)
            && let Some(base) = &self.gpu
        {
            // Refused only for different materials or images, which a new
            // outfit brings in a new mesh: then upload everything again.
            match renderer.upload_geometry_shared(device, &self.data, base) {
                Ok(gpu) => {
                    self.gpu = Some(gpu);
                    self.vertices_dirty = false;
                }
                Err(_) => self.gpu = None,
            }
        }
        if let Some(gpu) = &mut self.gpu {
            if std::mem::take(&mut self.vertices_dirty) {
                let centers: Vec<_> = self.data.batches.iter().map(|b| b.center).collect();
                gpu.update_vertices(queue, &self.data.vertices, &centers)?;
            }
        } else {
            self.gpu = Some(match textures {
                Some(textures) => renderer.upload_cached(device, queue, &self.data, textures)?,
                None => renderer.upload(device, queue, &self.data)?,
            });
            self.vertices_dirty = false;
        }
        if self.instanced {
            let instance = match &mut self.instance {
                Some(instance) => instance,
                None => self.instance.insert(GpuInstances::new(device, 2)?),
            };
            let body = bri_render::scene::SceneTransform {
                transform: self.model_transform,
                tint: [1.0; 4],
            };
            match &self.straddle {
                Some(s) => {
                    let through = bri_render::scene::SceneTransform {
                        transform: s.carried(self.model_transform),
                        ..body
                    };
                    instance.update_clipped(queue, &[body, through], &[s.near, s.far])?
                }
                None => instance.update(queue, &[body])?,
            };
        }
        Ok(())
    }
}

/// Uses an independent camera uniform so a preview and world can share one
/// command submission without the world's camera overwriting the preview.
pub struct Preview {
    renderer: SceneRenderer,
    mesh: Option<AvatarMesh>,
    texture: wgpu::Texture,
    depth: wgpu::Texture,
}
impl Preview {
    pub const ID: u64 = 0x425249_4156415441;
    pub const SIZE: (u32, u32) = (344, 516);
    pub fn new(device: &wgpu::Device) -> Self {
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        Self {
            renderer: SceneRenderer::new(device, format),
            mesh: None,
            texture: device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Original avatar preview"),
                size: wgpu::Extent3d {
                    width: Self::SIZE.0,
                    height: Self::SIZE.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
            }),
            depth: bri_render::scene::create_depth(device, Self::SIZE.0, Self::SIZE.1),
        }
    }
    fn camera(rotation: [f32; 3], distance: f32) -> bri_render::scene::Camera {
        let target = Vec3::new(0.0, 1.3, 0.0);
        let eye = target
            + Vec3::new(
                rotation[2].sin() * rotation[0].cos(),
                rotation[0].sin(),
                rotation[2].cos() * rotation[0].cos(),
            ) * distance;
        let mut camera = bri_render::scene::Camera::perspective(
            eye.to_array(),
            target.to_array(),
            Self::SIZE.0 as f32 / Self::SIZE.1 as f32,
            2.0 * ((35_f32.to_radians() * 0.5).tan() / (Self::SIZE.0 as f32 / Self::SIZE.1 as f32))
                .atan(),
            0.05,
            50.0,
        );
        // Authored Avatar_Preview key, Z-up to Y-up. The scene shader stores
        // light travel, opposite the preview's direction toward its white key.
        camera.sun_direction = [-0.721277, -0.57735, 0.57735, 0.0];
        camera.sun_color = [1.0, 1.0, 1.0, 0.0];
        camera.ambient = [0.5, 0.5, 0.5, 0.0];
        camera
    }
    pub fn render(
        &mut self,
        assets: &AvatarAssets,
        appearance: &Appearance,
        rotation: [f32; 3],
        distance: f32,
        time: f64,
        frame: &mut crate::platform::RenderContext<'_>,
    ) -> Result<()> {
        ensure!(
            time.is_finite()
                && time >= 0.0
                && rotation.iter().all(|v| v.is_finite())
                && distance.is_finite()
                && (1.0..=20.0).contains(&distance),
            "Invalid preview camera"
        );
        if self
            .mesh
            .as_ref()
            .is_none_or(|m| &m.appearance != appearance)
        {
            self.mesh = Some(assets.mesh(appearance.clone())?);
        }
        let mesh = self.mesh.as_mut().unwrap();
        mesh.pose_with_animation(
            assets,
            &PlayerState {
                owner: 0,
                feet: [0.0; 3],
                velocity: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
                head_yaw: 0.0,
                grounded: true,
                crouched: false,
                jetting: false,
                jump: Default::default(),
                archetype: Default::default(),
                scale: 1.0,
                energy: 100.0,
                speed_scale: 1.0,
                tick: Default::default(),
                tether: None,
                controller: None,
                pass_group: 0,
                posture: Default::default(),
            },
            time * 0.85,
            &AvatarAnimationInput {
                // Original AvatarGui sets body thread 1 to run at 0.85.
                // Head-up remains the outfit's existing accessory layer.
                body: [
                    None,
                    Some(ActionAnimation {
                        sequence: "run".into(),
                        started_at: 0.0,
                    }),
                ],
                ..Default::default()
            },
        )?;
        mesh.upload(&self.renderer, frame.device, frame.queue)?;
        let camera = Self::camera(rotation, distance);
        self.renderer.update_camera(frame.queue, &camera);
        self.renderer.render(
            frame.encoder,
            &self.texture.create_view(&Default::default()),
            &self.depth.create_view(&Default::default()),
            &[mesh.gpu.as_ref().unwrap()],
            Some(wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }),
        );
        frame.ui_renderer.set_external(
            Self::ID,
            self.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                ..Default::default()
            }),
            Self::SIZE,
        );
        Ok(())
    }
}

/// The appearance `prefs` make from `package`. It ends with the host's own
/// repair (`Package::repaired`), so the avatar screen never previews a look
/// (an accent the hat cannot wear, a colour out of range) the host would
/// change for everyone else.
pub fn appearance_from_prefs(package: &Package, prefs: &AvatarPrefs) -> Result<Appearance> {
    let mut appearance = package.defaults.clone();
    // A part this pack does not have (removed, renamed, or not a name at
    // all) keeps the pack default rather than failing the whole avatar.
    let known = |slot: &str, name: &str| {
        let lists = if slot == "accent" {
            package.accents_allowed.values().collect::<Vec<_>>()
        } else {
            package.parts.get(slot).into_iter().collect()
        };
        name == "none"
            || lists
                .iter()
                .any(|l| l.iter().any(|n| n.eq_ignore_ascii_case(name)))
    };
    for slot in appearance.parts.keys().cloned().collect::<Vec<_>>() {
        if let Some(value) = prefs.get(&slot) {
            let name = value.trim().to_ascii_lowercase();
            if known(&slot, &name) {
                appearance.parts.insert(slot, name);
            }
        }
    }
    for slot in appearance.colors.keys().cloned().collect::<Vec<_>>() {
        if let Some(value) = prefs.get(&format!("{slot}Color")) {
            let values: Vec<f32> = value
                .split_whitespace()
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()?;
            appearance.colors.insert(
                slot,
                values
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid avatar color size"))?,
            );
        }
    }
    if let Some(value) = prefs.get("FaceName") {
        appearance.face = value.into();
    }
    if let Some(value) = prefs.get("DecalName") {
        appearance.decal = value.into();
    }
    let (appearance, _) = package.repaired(&appearance);
    package.resolve(&appearance)?;
    Ok(appearance)
}

/// The bodies the Add-On `id` at `dir` brings, each with its shape file when
/// it names one: its `body` content (an imported glTF body), and the bodies
/// its own player types name among its assets (a v20 import's, whose file
/// `assets/content.json` gives).
pub fn body_models(dir: &Path, id: &str) -> Vec<(String, Option<String>)> {
    let mut models: Vec<(String, Option<String>)> =
        std::fs::read(dir.join(bri_package::library::MANIFEST_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|m| m["provides"].as_array().cloned())
            .into_iter()
            .flatten()
            .filter(|p| p["kind"] == "body")
            .filter_map(|p| {
                Some((
                    p["id"].as_str()?.to_owned(),
                    Some(p["file"].as_str()?.to_owned()),
                ))
            })
            .collect();
    if let Ok(archetypes) = std::fs::read_dir(dir.join("assets/archetypes")) {
        models.extend(
            archetypes
                .flatten()
                .filter_map(|e| std::fs::read(e.path()).ok())
                .filter_map(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .filter_map(|a| a["model"].as_str().map(str::to_owned))
                .filter(|m| m.starts_with(&format!("{id}:asset/")))
                .map(|m| (m, None)),
        );
    }
    models.sort();
    models.dedup_by(|a, b| a.0 == b.0);
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An Add-On's own player model (Import Add-On's `shapeFile`, the
    /// Shark's `shark.dts`) loads from its package and draws in place of
    /// the Blockhead, its objects named as colour slots painted with them
    /// and the rest hidden.
    #[test]
    fn an_add_on_player_model_draws_as_its_body() -> Result<()> {
        let mut assets = crate::testing::avatar::assets()?;
        let root = crate::testing::ScratchDir::new("own-body")?;
        let dir = root.path().join("addons/fish");
        std::fs::create_dir_all(dir.join("assets/archetypes"))?;
        // The made-up Blockhead rig, with its sequences inside, as a dts
        // converts with its own.
        let rig = crate::testing::avatar::rig();
        let mut shape = rig.shape.clone();
        shape.animations = rig.sequences.values().cloned().collect();
        let mut swim = rig.sequence("root").unwrap().clone();
        swim.name = "swim".into();
        swim.looping = true;
        swim.additive = true;
        shape.animations.push(swim);
        std::fs::write(
            dir.join("assets/fish.shape.json"),
            serde_json::to_vec(&shape)?,
        )?;
        let mut content = vec![serde_json::json!({
            "kind": "asset", "id": "fish:asset/fish.dts", "file": "assets/fish.shape.json"})];
        for material in &shape.materials {
            let png = format!("assets/{}.png", material.name);
            image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])).save(dir.join(&png))?;
            content.push(serde_json::json!({
                "kind": "asset", "id": format!("fish:asset/{}.png", material.name), "file": png}));
        }
        std::fs::write(
            dir.join("assets/content.json"),
            serde_json::to_vec(&serde_json::json!({ "content": content }))?,
        )?;
        std::fs::write(
            dir.join("assets/archetypes/fishbot.json"),
            br#"{"schema_version":1,"name":"","model":"fish:asset/fish.dts"}"#,
        )?;
        let packages: bri_package::packages::PackageSet =
            serde_json::from_value(serde_json::json!({ "schema_version": 1, "packages": [
                { "id": "fish", "version": "1.0.0", "side": "shared", "dir": "addons/fish" }]}))?;
        assert!(!assets.has_body("fish:asset/fish.dts"));
        assets.load_bodies(root.path(), &packages);
        assert!(
            assets.has_body("fish:asset/fish.dts"),
            "the package's body loaded"
        );
        let mut appearance = assets.package.defaults.clone();
        appearance
            .colors
            .insert("torso".into(), [0.1, 0.2, 0.3, 1.0]);
        let mut mesh = assets.body_mesh("fish:asset/fish.dts", appearance)?;
        assert_eq!(mesh.model.as_deref(), Some("fish:asset/fish.dts"));
        assert_eq!(mesh.outfit.nodes.get("chest"), Some(&[0.1, 0.2, 0.3, 1.0]));
        assert!(!mesh.outfit.nodes.contains_key("helmet"));
        assert!(!mesh.outfit.nodes.contains_key("visor"));
        let mut dressed = mesh.appearance.clone();
        dressed.parts.insert("hat".into(), "helmet".into());
        dressed.parts.insert("accent".into(), "visor".into());
        dressed.colors.insert("hat".into(), [0.2, 0.2, 0.2, 1.0]);
        dressed.colors.insert("accent".into(), [0.3, 0.3, 0.3, 1.0]);
        let accessories = assets.body_mesh("fish:asset/fish.dts", dressed)?;
        assert_eq!(
            accessories.outfit.nodes.get("helmet"),
            Some(&[0.2, 0.2, 0.2, 1.0])
        );
        assert_eq!(
            accessories.outfit.nodes.get("visor"),
            Some(&[0.3, 0.3, 0.3, 1.0])
        );
        assert!(
            mesh.outfit
                .nodes
                .keys()
                .all(|n| assets.bodies["fish:asset/fish.dts"]
                    .object_names
                    .contains(n)),
            "only its own objects are painted"
        );
        mesh.pose_with_animation(
            &assets,
            &player(),
            0.0,
            &AvatarAnimationInput {
                water_coverage: 1.0,
                ..Default::default()
            },
        )?;
        assert_eq!(mesh.mode, "swim", "an authored body loop plays in liquid");
        mesh.pose_with_animation(&assets, &player(), 0.1, &AvatarAnimationInput::default())?;
        assert_eq!(
            mesh.mode, "root",
            "leaving liquid restores ordinary locomotion"
        );
        mesh.pose_with_animation(
            &assets,
            &player(),
            0.2,
            &AvatarAnimationInput {
                water_coverage: 1.0,
                dead: true,
                ..Default::default()
            },
        )?;
        assert_eq!(mesh.mode, "death1", "death overrides the swim loop");
        Ok(())
    }

    /// A pack with two hats, each with its own accent.
    fn hats_pack() -> Package {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "id": "test", "rig": "rig.json", "rig_sha256": "",
            "parts": {"hat": ["none", "helmet", "cap"], "accent": ["none", "visor", "plume"],
                "pack": ["none"], "secondpack": ["none"], "chest": ["chest"], "hip": ["pants"],
                "rarm": ["rarm"], "larm": ["larm"], "rhand": ["rhand"], "lhand": ["lhand"],
                "rleg": ["rshoe"], "lleg": ["lshoe"]},
            "accents_allowed": {"helmet": ["visor"], "cap": ["plume"]},
            "faces": ["smiley"], "decals": ["AAA-None"], "surfaces": {}, "textures": {
                "smiley": {"file": "smiley.png", "sha256": "", "source": "", "width": 1, "height": 1},
                "AAA-None": {"file": "none.png", "sha256": "", "source": "", "width": 1, "height": 1}},
            "defaults": {"parts": {"hat": 0, "accent": 0, "pack": 0, "secondpack": 0,
                "chest": 0, "hip": 0, "rarm": 0, "larm": 0, "rhand": 0, "lhand": 0,
                "rleg": 0, "lleg": 0},
                "colors": {"head": [1.0, 0.88, 0.61, 1.0],
                "torso": [0.9, 0.9, 0.9, 1.0], "hat": [1.0, 1.0, 0.0, 1.0],
                "accent": [0.0, 0.2, 0.64, 0.7], "pack": [0.0, 0.4, 0.8, 1.0],
                "secondpack": [0.0, 1.0, 0.0, 1.0], "hip": [0.0, 0.0, 1.0, 1.0],
                "rarm": [0.9, 0.0, 0.0, 1.0], "larm": [0.9, 0.0, 0.0, 1.0],
                "rhand": [1.0, 0.88, 0.61, 1.0], "lhand": [1.0, 0.88, 0.61, 1.0],
                "rleg": [0.0, 0.0, 1.0, 1.0], "lleg": [0.0, 0.0, 1.0, 1.0]},
                "face": "smiley", "decal": "AAA-None"}
        }))
        .unwrap()
    }

    #[test]
    fn the_avatar_screen_shows_the_look_the_host_keeps() {
        let package = hats_pack();
        let mut prefs = AvatarPrefs::default();
        // The cap's plume on the helmet, and a colour past full brightness.
        prefs.set("hat", "helmet");
        prefs.set("accent", "plume");
        prefs.set("hatColor", "1.5 0 0 1");
        let shown = appearance_from_prefs(&package, &prefs).unwrap();
        let (kept, changed) = package.repaired(&shown);
        assert_eq!(
            shown, kept,
            "the host would change the preview: {changed:?}"
        );
        assert_ne!(shown.parts.get("accent").map(String::as_str), Some("plume"));
        assert_ne!(shown.colors.get("hat"), Some(&[1.5, 0.0, 0.0, 1.0]));
        // A fitting accent and colour are kept as chosen.
        prefs.set("accent", "visor");
        prefs.set("hatColor", "0.5 0 0 1");
        let shown = appearance_from_prefs(&package, &prefs).unwrap();
        assert_eq!(shown.parts.get("accent").map(String::as_str), Some("visor"));
        assert_eq!(shown.colors.get("hat"), Some(&[0.5, 0.0, 0.0, 1.0]));
    }

    thread_local! {
        /// Off, placed nodes' accessories keep their animated pose (as
        /// before `follow_anchors`), for checks that must catch that.
        pub(super) static FOLLOW_ANCHORS: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    }

    /// The avatar pack a test runs on: the made-up one
    /// (`crate::testing::avatar`) or the generated v20 one.
    pub(crate) struct Avatar {
        pub assets: AvatarAssets,
        pub content: bool,
    }
    fn repo() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
    impl Avatar {
        pub fn synthetic() -> Result<Self> {
            Ok(Self {
                assets: crate::testing::avatar::assets()?,
                content: false,
            })
        }
        /// `content/avatar-pack-002` (`BRI_CONTENT` names another content
        /// folder).
        pub fn content() -> Result<Self> {
            let content = std::env::var_os("BRI_CONTENT")
                .map_or_else(|| repo().join("content"), std::path::PathBuf::from);
            Ok(Self {
                assets: AvatarAssets::load(&bri_package::testing::pack_dir(&content, "avatar"))?,
                content: true,
            })
        }
    }
    crate::testing::synthetic_and_content!(
        Avatar: reposed_vertices_match_a_full_shape_rebuild,
        a_restructured_body_keeps_its_textures,
        new_outfits_bind_the_textures_bodies_already_uploaded,
        a_respawned_body_stands_in_root_without_getting_up_from_the_corpse,
        item_mounts_use_the_same_sampled_avatar_pose_and_body_transform,
        the_blockheads_aiming_hand_is_the_frame_fitted_hands_take,
        held_and_action_arm_layers_keep_locomotion_and_mounts_coherent,
        absolute_actions_take_over_the_nodes_they_animate,
        held_arm_pose_precedes_additive_look_and_pack_headup_layers,
        rebuilt_outfit_continues_the_running_clip,
        mount_action_is_only_the_playing_actions_motion,
        seated_body_takes_the_mount_rotation,
        a_seated_rider_reaches_the_seats_hand_holds,
        getting_up_eases_out_of_the_sit_over_one_transition,
        jet_release_keeps_the_root_pose_until_landing,
        jets_interrupt_a_jump_without_restarting_it_on_release,
        jet_history_survives_skipped_poses_and_first_remote_observation,
        free_look_turns_only_the_head_toward_the_camera,
        original_outfits_materials_and_customization_rules,
        ragdoll_on_the_real_blockhead,
        ragdoll_keeps_accessories_on,
        a_corpse_vanishes_where_its_body_is_drawn,
    );

    #[test]
    #[ignore = "requires generated v20 avatar content and an offscreen adapter"]
    fn actual_editor_portrait_receives_its_authored_directional_key() -> Result<()> {
        use bri_ui::{
            draw::{DrawList, Filter},
            geom::Rect,
            gpu::{Headless, UiRenderer},
            pack::{Pack, TexKey},
        };
        let fx = Avatar::content()?;
        let gpu = Headless::new()?;
        let mut ui = UiRenderer::new(&gpu.device, &gpu.queue);
        let pack = Pack::from_parts(Default::default(), Default::default());
        let mut preview = Preview::new(&gpu.device);
        let size = Preview::SIZE;
        let mut draw = DrawList::new(Rect::new(0, 0, size.0 as i32, size.1 as i32));
        draw.image(
            TexKey::External(Preview::ID),
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0, size.0 as f32, size.1 as f32],
            [255; 4],
            Filter::Nearest,
        );
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen avatar editor"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        preview.render(
            &fx.assets,
            &fx.assets.package.defaults,
            [0.3, 0.6, 2.52],
            4.34,
            0.0,
            &mut crate::platform::RenderContext {
                device: &gpu.device,
                queue: &gpu.queue,
                encoder: &mut encoder,
                target: &view,
                format: wgpu::TextureFormat::Rgba8Unorm,
                size: (1, 1),
                ui_renderer: &mut ui,
            },
        )?;
        gpu.queue.submit([encoder.finish()]);
        let directed = gpu.render_rgba(&mut ui, &pack, &draw, size, 1.0, [0.0; 4])?;
        // Counterfactual: the same original outfit, exact pose/normals, camera,
        // renderer and UI texture, with only the white key switched off.
        let mut camera = Preview::camera([0.3, 0.6, 2.52], 4.34);
        camera.sun_color = [0.0; 4];
        preview.renderer.update_camera(&gpu.queue, &camera);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        preview.renderer.render(
            &mut encoder,
            &preview.texture.create_view(&Default::default()),
            &preview.depth.create_view(&Default::default()),
            &[preview.mesh.as_ref().unwrap().gpu.as_ref().unwrap()],
            Some(wgpu::Color::TRANSPARENT),
        );
        gpu.queue.submit([encoder.finish()]);
        let ambient = gpu.render_rgba(&mut ui, &pack, &draw, size, 1.0, [0.0; 4])?;
        let out = repo().join("artifacts/avatar-editor-preview-light");
        std::fs::create_dir_all(&out)?;
        for (label, rgba) in [("directed", &directed), ("ambient-only", &ambient)] {
            image::save_buffer(
                out.join(format!("{label}.png")),
                rgba,
                size.0,
                size.1,
                image::ColorType::Rgba8,
            )?;
        }
        let mut samples = 0;
        let mut increase = 0.0;
        for (directed, ambient) in directed.chunks_exact(4).zip(ambient.chunks_exact(4)) {
            if directed[3] > 250 && ambient[3] > 250 {
                samples += 1;
                increase += (0..3)
                    .map(|c| directed[c] as f64 - ambient[c] as f64)
                    .sum::<f64>()
                    / 3.0;
            }
        }
        let mean = increase / samples.max(1) as f64;
        println!(
            "native preview white key: opaque pixels={samples}, mean RGB increase={mean}; captures={}",
            out.display()
        );
        ensure!(
            samples > 100 && mean > 8.0,
            "the authored source-facing portrait is effectively ambient-only: mean key increase={mean}"
        );
        Ok(())
    }

    /// An Add-On body loaded as `load_bodies` loads one: the made-up rig
    /// with its nodes renamed (`from`, `to`, lower case `from`) and only
    /// the clips `clips` keeps, renamed (`from`, `to`).
    fn own_body(
        name: &str,
        renames: &[(&str, &str)],
        clips: &[(&str, &str)],
    ) -> Result<(AvatarAssets, String, crate::testing::ScratchDir)> {
        let mut assets = crate::testing::avatar::assets()?;
        let root = crate::testing::ScratchDir::new(name)?;
        let dir = root.path().join(format!("addons/{name}"));
        std::fs::create_dir_all(dir.join("assets/archetypes"))?;
        let rig = crate::testing::avatar::rig();
        let mut shape = rig.shape.clone();
        let rename = |n: &mut String| {
            if let Some((_, to)) = renames
                .iter()
                .find(|(from, _)| from.eq_ignore_ascii_case(n))
            {
                *n = (*to).into();
            }
        };
        for node in &mut shape.nodes {
            rename(&mut node.name);
        }
        shape.animations = clips
            .iter()
            .map(|(from, to)| {
                let mut clip = rig.sequence(from).expect("made-up clip").clone();
                clip.name = (*to).into();
                for track in &mut clip.nodes {
                    rename(&mut track.node);
                }
                clip
            })
            .collect();
        std::fs::write(
            dir.join(format!("assets/{name}.shape.json")),
            serde_json::to_vec(&shape)?,
        )?;
        let model = format!("{name}:asset/{name}.dts");
        let mut content = vec![serde_json::json!({
            "kind": "asset", "id": model, "file": format!("assets/{name}.shape.json")})];
        for material in &shape.materials {
            let png = format!("assets/{}.png", material.name);
            image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])).save(dir.join(&png))?;
            content.push(serde_json::json!({
                "kind": "asset", "id": format!("{name}:asset/{}.png", material.name), "file": png}));
        }
        std::fs::write(
            dir.join("assets/content.json"),
            serde_json::to_vec(&serde_json::json!({ "content": content }))?,
        )?;
        std::fs::write(
            dir.join(format!("assets/archetypes/{name}.json")),
            serde_json::to_vec(&serde_json::json!({"schema_version":1,"name":"","model": model}))?,
        )?;
        let packages: bri_package::packages::PackageSet = serde_json::from_value(
            serde_json::json!({ "schema_version": 1, "packages": [
                { "id": name, "version": "1.0.0", "side": "shared", "dir": format!("addons/{name}") }]}),
        )?;
        assets.load_bodies(root.path(), &packages);
        ensure!(assets.has_body(&model), "{model} did not load");
        Ok((assets, model, root))
    }

    #[test]
    fn authored_crouch_locomotion_keeps_its_phase_and_motion_channels() -> Result<()> {
        let (mut assets, model, _root) = own_body(
            "crouch-composition",
            &[],
            &[
                ("root", "root"),
                ("crouch", "crouch"),
                ("crouchrun", "crouchrun"),
                ("crouchback", "crouchback"),
            ],
        )?;
        // Entirely synthetic absolute channels, deliberately distinct between
        // the held crouch pose and each moving crouch clip.
        let rig = &mut assets.bodies.get_mut(&model).context("synthetic rig")?.rig;
        for (name, x) in [("crouch", 0.), ("crouchrun", 1.), ("crouchback", -1.)] {
            let clip = rig.sequences.get_mut(name).context("synthetic clip")?;
            clip.frames = 2;
            clip.duration = 0.8;
            clip.looping = true;
            clip.additive = false;
            clip.ground_translations.clear();
            clip.ground_rotations.clear();
            clip.nodes = vec![bri_content::shape::NodeTrack {
                node: "Hip".into(),
                rotations: vec![Quat::IDENTITY.to_array(); 2],
                translations: vec![[x, 0.6, 0.], [x + 0.4, 0.9, 0.]],
                scales: vec![],
                scale_rotations: vec![],
            }];
        }
        let mut archetype = bri_sim::archetype::Archetypes::default()
            .resolve(Default::default())
            .clone();
        archetype.look.model = model.clone();
        for (velocity, expected_clip) in [
            ([0., 0., 0.], "crouch"),
            ([0., 0., -3.], "crouchrun"),
            ([0., 0., 3.], "crouchback"),
        ] {
            let p = PlayerState {
                crouched: true,
                velocity,
                ..player()
            };
            let sample = |name, time| {
                let rig = &assets.bodies[&model].rig;
                bri_content::animation::sample(&rig.shape, rig.sequence(name), time)
            };
            let expected = sample(expected_clip, 0.2)?;
            let legacy = sample("crouch", 0.8)?;
            for (composition, stand_in) in [
                (CrouchComposition::Overlay, false),
                (CrouchComposition::Locomotion, false),
                (CrouchComposition::Locomotion, true),
            ] {
                archetype.look.crouch_composition = composition;
                let mut body = assets.body_mesh(&model, assets.package.defaults.clone())?;
                body.set_body_kind(BodyKind::of(&archetype, stand_in));
                for t in [0., 0.2] {
                    body.pose_with_animation(&assets, &p, t, &AvatarAnimationInput::default())?;
                }
                let want = if composition == CrouchComposition::Locomotion && !stand_in {
                    &expected
                } else {
                    &legacy
                };
                assert!(
                    body.posed_nodes
                        .iter()
                        .zip(&want.nodes)
                        .all(|(a, b)| a.abs_diff_eq(*b, 1e-5)),
                    "{expected_clip} {composition:?} stand_in={stand_in}"
                );
                assert!(
                    expected
                        .nodes
                        .iter()
                        .zip(&legacy.nodes)
                        .any(|(a, b)| !a.abs_diff_eq(*b, 1e-3)),
                    "fixture must distinguish source motion from held overlay"
                );
                // Switching a live body back to the default must start its
                // crouch thread afresh, rather than inheriting a stale phase.
                if composition == CrouchComposition::Locomotion {
                    archetype.look.crouch_composition = CrouchComposition::Overlay;
                    body.set_body_kind(BodyKind::of(&archetype, false));
                    body.pose_with_animation(&assets, &p, 0.4, &AvatarAnimationInput::default())?;
                    assert!(
                        body.posed_nodes
                            .iter()
                            .zip(&legacy.nodes)
                            .all(|(a, b)| a.abs_diff_eq(*b, 1e-5))
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn authored_hand_frames_preserve_clips_and_keep_the_first_person_fallback() -> Result<()> {
        let (mut assets, model, _root) = own_body("authored-hands", &[], &[("root", "root")])?;
        assets
            .bodies
            .get_mut(&model)
            .context("imported body fixture")?
            .fitted = true;
        let mut archetype = bri_sim::archetype::Archetypes::default()
            .resolve(Default::default())
            .clone();
        archetype.look.model = model.clone();
        archetype.sockets.preserve_hand_pose = true;
        let mut body = assets.body_mesh(&model, assets.package.defaults.clone())?;
        body.set_body_kind(BodyKind::of(&archetype, false));
        body.pose_with_animation(&assets, &player(), 0., &AvatarAnimationInput::default())?;
        let original = body.posed_nodes.clone();
        body.pose_with_animation(
            &assets,
            &player(),
            0.,
            &AvatarAnimationInput {
                held_tool_pose: HeldToolPose::Both,
                support: Some(Vec3::splat(4.)),
                ..Default::default()
            },
        )?;
        assert!(
            body.posed_nodes
                .iter()
                .zip(&original)
                .all(|(a, b)| a.abs_diff_eq(*b, 1e-5)),
            "generic raising/support IK must not alter authored hands"
        );
        let raw = body
            .world_node(&assets, "Mount0")
            .context("raw hand node")?;
        assert!(
            body.mount_socket(&assets, 0, 0.)
                .context("hand")?
                .abs_diff_eq(raw, 1e-5)
        );
        assert!(
            body.first_person_hand(0).is_some(),
            "foreign stock tools retain their independent first-person fallback"
        );
        let rig = &assets.for_mesh(&body).rig;
        let host =
            bri_sim::session::body_grips(rig, &archetype.sockets, &archetype.look.animations, true);
        assert!(
            Mat4::from_cols_array(host.get("mount0").context("host hand")?).abs_diff_eq(
                body.model_node(&assets, "Mount0").context("client hand")?,
                1e-5
            ),
            "host and client preserve the same authored frame"
        );
        Ok(())
    }

    /// v20 plays the run, back and side clips at the speed the body covers
    /// the ground (`maxTimeScale` 1.5 at most): by the clip's own ground
    /// motion, or by the speed an archetype gives a clip that has none.
    #[test]
    fn locomotion_clips_play_at_the_speed_the_body_moves() -> Result<()> {
        let rig = crate::testing::avatar::rig();
        let mut run = rig.sequence("run").context("run")?.clone();
        let kind = BodyKind::default();
        let forward = |speed: f32| Vec3::new(0.0, 0.0, -speed);
        assert_eq!(
            kind.time_scale(&run, Role::Run, forward(9.0)),
            1.0,
            "a clip with no ground motion keeps its own time"
        );
        // Two units of ground over the clip.
        run.ground_translations = vec![[0.0, 0.0, -1.0], [0.0, 0.0, -2.0]];
        let speed = 2.0 / run.duration;
        let scale = |kind: &BodyKind, v: Vec3| kind.time_scale(&run, Role::Run, v);
        assert!((scale(&kind, forward(speed * 0.5)) - 0.5).abs() < 1e-5);
        assert!((scale(&kind, forward(speed)) - 1.0).abs() < 1e-5);
        assert_eq!(
            scale(&kind, forward(speed * 3.0)),
            1.5,
            "v20's maxTimeScale"
        );
        assert!(
            scale(&kind, Vec3::new(speed, 0.0, 0.0)) < 1e-5,
            "only the clip's own way counts"
        );
        assert_eq!(kind.time_scale(&run, Role::Idle, forward(speed)), 1.0);
        let mut archetype = bri_sim::archetype::Archetypes::default()
            .resolve(Default::default())
            .clone();
        archetype.look.ground_speeds = [("run".to_string(), 4.0)].into();
        archetype.look.max_time_scale = 2.0;
        archetype.validate()?;
        let kind = BodyKind::of(&archetype, false);
        assert!(
            (scale(&kind, forward(6.0)) - 1.5).abs() < 1e-5,
            "the archetype's speed wins"
        );
        assert_eq!(scale(&kind, forward(20.0)), 2.0);
        archetype.look.ground_speeds = [("jump".to_string(), 4.0)].into();
        assert!(archetype.validate().is_err(), "only locomotion roles");

        // A drawn body: running at half the clip's speed, half as far in.
        let assets = crate::testing::avatar::assets()?;
        let mut archetype = bri_sim::archetype::Archetypes::default()
            .resolve(Default::default())
            .clone();
        let clip = assets.rig.sequence("run").context("run")?.duration;
        archetype.look.ground_speeds = [("run".to_string(), 8.0)].into();
        let mut phases = Vec::new();
        for speed in [4.0, 8.0] {
            let mut mesh = assets.mesh(assets.package.defaults.clone())?;
            mesh.set_body_kind(BodyKind::of(&archetype, false));
            let mut p = player();
            p.velocity = [0.0, 0.0, -speed];
            mesh.pose(&assets, &p, 0.0)?;
            assert_eq!(mesh.mode, "run");
            mesh.pose(&assets, &p, f64::from(clip) * 0.2)?;
            phases.push(mesh.phase);
        }
        assert!((phases[0] * 2.0 - phases[1]).abs() < 1e-4, "{phases:?}");
        Ok(())
    }

    /// v0.3.0 W2: a body authored with its own node names (no `Mount0`, no
    /// `Eye`) and only an idle and a walk clip used to be refused at load,
    /// and any missing clip failed its whole pose. Now it holds an item on
    /// its own hand node, sees from its own head node, poses for every
    /// action with fallbacks (walking back plays the walk reversed), and
    /// names exactly what it lacks.
    #[test]
    fn a_body_with_its_own_nodes_and_two_clips_holds_sits_and_names_what_it_lacks() -> Result<()> {
        let (assets, model, _dir) = own_body(
            "strider",
            &[
                ("Mount0", "Grip_R"),
                ("Mount1", "Grip_L_Unused"),
                ("Eye", "Head_Cam"),
            ],
            &[("root", "Stand"), ("run", "Walk")],
        )?;
        let mut archetype = bri_sim::archetype::Archetypes::default()
            .resolve(Default::default())
            .clone();
        archetype.id = "strider:archetype/strider".into();
        archetype.look.model = model.clone();
        archetype.sockets = Sockets {
            right_hand: Some(bri_content::body::Socket {
                node: Some("Grip_R".into()),
                offset: [0.0, 0.0, -0.1],
            }),
            eye: Some(bri_content::body::Socket::node("head_cam")),
            ..Sockets::default()
        };
        archetype.look.animations = [
            ("idle".to_string(), "Stand".to_string()),
            ("run".into(), "Walk".into()),
        ]
        .into();
        let mut mesh = assets.body_mesh(&model, assets.package.defaults.clone())?;
        mesh.set_body_kind(BodyKind::of(&archetype, false));
        // Every pose that used to refuse the body.
        let mut time = 0.0;
        let mut pose = |mesh: &mut AvatarMesh, player: PlayerState, input: AvatarAnimationInput| {
            time += 0.1;
            mesh.pose_with_animation(&assets, &player, time, &input)
        };
        for held in [HeldToolPose::Right, HeldToolPose::Left, HeldToolPose::Both] {
            pose(
                &mut mesh,
                player(),
                AvatarAnimationInput {
                    held_tool_pose: held,
                    ..Default::default()
                },
            )?;
        }
        pose(
            &mut mesh,
            player(),
            AvatarAnimationInput {
                sitting: true,
                ..Default::default()
            },
        )?;
        assert_eq!(mesh.mode, SIT, "it is seated, standing in its idle");
        pose(
            &mut mesh,
            player(),
            AvatarAnimationInput {
                dead: true,
                ..Default::default()
            },
        )?;
        let crouched = PlayerState {
            crouched: true,
            velocity: [0.0, 0.0, -3.0],
            ..player()
        };
        pose(&mut mesh, crouched, AvatarAnimationInput::default())?;
        let airborne = PlayerState {
            grounded: false,
            velocity: [0.0, -20.0, 0.0],
            ..player()
        };
        pose(&mut mesh, airborne, AvatarAnimationInput::default())?;
        assert_eq!(mesh.mode, "fall");
        pose(
            &mut mesh,
            player(),
            AvatarAnimationInput {
                action: Some(ActionAnimation {
                    sequence: "armreadyboth".into(),
                    started_at: 0.0,
                }),
                ..Default::default()
            },
        )?;
        assert_eq!(mesh.skipped_clips(), ["armreadyboth"], "named, not fatal");
        // Walking back plays the walk clip reversed.
        let back = PlayerState {
            velocity: [0.0, 0.0, 4.0],
            ..player()
        };
        pose(&mut mesh, back, AvatarAnimationInput::default())?;
        assert_eq!((mesh.mode, mesh.forward), ("back", false));
        // The hand is its own node, the eye its own head.
        let grip = mesh.world_node(&assets, "Grip_R").context("its grip")?;
        let held = mesh
            .mount_socket(&assets, 0, 0.3)
            .context("held in the right hand")?;
        assert!(held.abs_diff_eq(
            grip * Mat4::from_translation(Vec3::new(0.0, 0.0, -0.1)),
            1e-5
        ));
        assert_eq!(mesh.mount_node(&assets, 0), Some(held));
        assert_eq!(
            mesh.mount_socket(&assets, 1, 0.0),
            None,
            "no left hand node: nothing drawn there"
        );
        let head = mesh.world_node(&assets, "Head_Cam").context("its head")?;
        let eye = mesh.eye_world(&assets, true).context("its eye")?;
        assert!(eye.abs_diff_eq(head.w_axis.truncate(), 1e-5));
        assert!(mesh.eye_transform(&assets, 0.0, 0.0).is_some());
        let report = mesh.body_report(&assets);
        for line in [
            "no `aim_right` animation; its arm does not raise a held item",
            "no `sit` animation; it plays its `idle` instead",
            "no `back` animation; it plays its `run` instead",
            "its model has no `Mount1` node for the `left_hand` socket, so items held in that hand are hidden",
        ] {
            assert!(report.iter().any(|l| l == line), "{line}\n{report:#?}");
        }
        assert!(
            !report
                .iter()
                .any(|l| l.contains("`idle`") && l.starts_with("no `idle`"))
        );
        Ok(())
    }

    #[test]
    fn authored_seat_frames_preserve_owned_poses_and_anchor_foreign_hips() -> Result<()> {
        let (assets, model, _dir) =
            own_body("rider", &[], &[("root", "idle"), ("sit", "own_seat")])?;
        let mut archetype = bri_sim::archetype::Archetypes::default()
            .resolve(Default::default())
            .clone();
        archetype.look.model = model.clone();
        archetype
            .look
            .animations
            .insert("sit".into(), "own_seat".into());
        let mut p = player();
        p.feet = [3., 4., 5.];
        let rotation = Quat::from_rotation_z(0.3) * Quat::from_rotation_y(0.8);
        for owns_clip in [true, false] {
            let mut mesh = assets.body_mesh(&model, assets.package.defaults.clone())?;
            mesh.set_body_kind(BodyKind::of(&archetype, false));
            let input = AvatarAnimationInput {
                sitting: true,
                mount_rotation: Some(rotation),
                seat_pose: Some(
                    if owns_clip {
                        "own_seat"
                    } else {
                        "foreign_seat"
                    }
                    .into(),
                ),
                ..Default::default()
            };
            mesh.pose_with_animation(&assets, &p, 0., &input)?;
            mesh.pose_with_animation(&assets, &p, 1., &input)?;
            let rig = &assets.for_mesh(&mesh).rig;
            let parents: Vec<_> = rig.shape.nodes.iter().map(|n| n.parent).collect();
            let bind = bri_content::animation::sample(&rig.shape, None, 0.)?;
            let hip = bri_content::hold::hip(&parents, &bind.nodes).context("fixture hip")?;
            let hip_world = (mesh.model_transform * mesh.posed_nodes[hip])
                .w_axis
                .truncate();
            let expected = if owns_clip {
                let original =
                    bri_content::animation::sample(&rig.shape, rig.sequence("own_seat"), 0.)?;
                Vec3::from(p.feet) + rotation * original.nodes[hip].w_axis.truncate()
            } else {
                Vec3::from(p.feet)
            };
            assert!(
                hip_world.distance(expected) < 1e-4,
                "owned {owns_clip}: {hip_world} != {expected}"
            );
        }
        Ok(())
    }

    /// A body with no clips at all still poses (its bind pose), holds and
    /// sits, and says it has no animations.
    #[test]
    fn a_body_with_no_clips_stands_in_its_bind_pose_and_still_holds() -> Result<()> {
        let (assets, model, _dir) = own_body("statue", &[], &[])?;
        let mut mesh = assets.body_mesh(&model, assets.package.defaults.clone())?;
        let mut archetype = bri_sim::archetype::Archetypes::default()
            .resolve(Default::default())
            .clone();
        archetype.look.model = model.clone();
        mesh.set_body_kind(BodyKind::of(&archetype, false));
        for (t, input) in [
            AvatarAnimationInput {
                held_tool_pose: HeldToolPose::Both,
                ..Default::default()
            },
            AvatarAnimationInput {
                sitting: true,
                ..Default::default()
            },
            AvatarAnimationInput {
                dead: true,
                ..Default::default()
            },
        ]
        .into_iter()
        .enumerate()
        {
            mesh.pose_with_animation(&assets, &player(), t as f64, &input)?;
        }
        assert!(
            mesh.mount_socket(&assets, 0, 0.0).is_some(),
            "holds on its Mount0"
        );
        assert!(mesh.eye_world(&assets, true).is_some());
        let report = mesh.body_report(&assets);
        assert!(
            report
                .iter()
                .any(|l| l == "no `idle` animation; it stands in its bind pose")
        );
        Ok(())
    }

    /// The Runner's box body: the mesh is a hidden Blockhead that only
    /// times the sit, so items go to its own hand sockets (pitched with
    /// the aim), never the Blockhead's hand; seated it drops to its sitting
    /// eye, tilts with the seat and folds its legs at the hip; getting up
    /// eases back.
    #[test]
    fn a_box_body_holds_at_its_own_hand_and_sits_with_folded_legs() -> Result<()> {
        let assets = crate::testing::avatar::assets()?;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let mut runner = bri_sim::archetype::Archetypes::default()
            .resolve(Default::default())
            .clone();
        runner.movement.stand_eye = 1.5;
        runner.movement.eye_forward = 0.1;
        runner.movement.sit_eye = 1.15;
        runner.movement.sit_eye_forward = -0.1;
        runner.sockets = serde_json::from_value(serde_json::json!({
            "right_hand": { "offset": [0.5, 0.74, -0.14] },
            "seat": { "offset": [0.0, 0.66, 0.0] }
        }))?;
        mesh.set_body_kind(BodyKind::of(&runner, true));
        let feet = Vec3::new(3.0, 0.0, 0.0);
        let at = PlayerState {
            feet: feet.to_array(),
            ..player()
        };
        mesh.pose_with_animation(&assets, &at, 0.0, &AvatarAnimationInput::default())?;
        let held = mesh.mount_socket(&assets, 0, 0.0).context("right hand")?;
        assert!(
            held.w_axis
                .truncate()
                .abs_diff_eq(feet + Vec3::new(0.5, 0.74, -0.14), 1e-5)
        );
        let up = mesh.mount_socket(&assets, 0, 0.5).context("aiming up")?;
        assert!(
            (up.transform_vector3(Vec3::NEG_Z)).y > 0.4,
            "the item pitches with the aim"
        );
        assert_eq!(
            mesh.mount_socket(&assets, 1, 0.0),
            None,
            "no left hand: hidden, not on a Blockhead hand"
        );
        assert_eq!(
            mesh.mount_socket(&assets, 2, 0.0),
            None,
            "no Blockhead mounts"
        );
        let eye = mesh.eye_world(&assets, true).context("eye")?;
        assert!(eye.abs_diff_eq(feet + Vec3::new(0.0, 1.5, -0.1), 1e-5));
        assert_eq!(mesh.box_pose().fold, 0.0);
        // Seated, tilted with the seat.
        let tilt = Quat::from_rotation_x(0.2);
        let seated = AvatarAnimationInput {
            sitting: true,
            mount_rotation: Some(tilt),
            ..Default::default()
        };
        mesh.pose_with_animation(&assets, &at, 1.0, &seated)?;
        mesh.pose_with_animation(&assets, &at, 1.1, &seated)?;
        let halfway = mesh.sit_weight();
        assert!(
            (0.3..0.5).contains(&halfway),
            "eases into the seat: {halfway}"
        );
        mesh.pose_with_animation(&assets, &at, 1.5, &seated)?;
        assert_eq!(mesh.sit_weight(), 1.0);
        // Down to the sitting eye and back by the 0.2 it sits behind the
        // standing eye.
        let shift = Vec3::new(0.0, 1.15 - 1.5, 0.2);
        let pose = mesh.box_pose();
        let frame = Mat4::from_rotation_translation(tilt, feet) * Mat4::from_translation(shift);
        assert!(pose.frame.abs_diff_eq(frame, 1e-5), "{:?}", pose.frame);
        assert_eq!(pose.hip, Some(Vec3::new(0.0, 0.66, 0.0)));
        assert!((pose.fold - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        let eye = mesh.eye_world(&assets, true).context("seated eye")?;
        assert!(eye.abs_diff_eq(frame.transform_point3(Vec3::new(0.0, 1.5, -0.1)), 1e-5));
        let held = mesh.mount_socket(&assets, 0, 0.0).context("seated hand")?;
        assert!(held.abs_diff_eq(
            frame * Mat4::from_translation(Vec3::new(0.5, 0.74, -0.14)),
            1e-5
        ));
        // Getting up eases back over one transition.
        mesh.pose_with_animation(&assets, &at, 2.0, &AvatarAnimationInput::default())?;
        assert_eq!(mesh.sit_weight(), 1.0);
        mesh.pose_with_animation(&assets, &at, 2.1, &AvatarAnimationInput::default())?;
        assert!((0.5..0.7).contains(&mesh.sit_weight()));
        mesh.pose_with_animation(&assets, &at, 2.4, &AvatarAnimationInput::default())?;
        assert_eq!(mesh.sit_weight(), 0.0);
        assert_eq!(
            mesh.body_report(&assets),
            [
                "no `left_hand` socket offset (a box body has no nodes), so items held in that hand are hidden"
            ]
        );
        Ok(())
    }

    fn player() -> PlayerState {
        PlayerState {
            owner: 1,
            feet: [0.0; 3],
            velocity: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            grounded: true,
            crouched: false,
            jetting: false,
            jump: Default::default(),
            archetype: Default::default(),
            scale: 1.0,
            energy: 100.0,
            speed_scale: 1.0,
            tick: Default::default(),
            tether: None,
            controller: None,
            pass_group: 0,
            posture: Default::default(),
        }
    }
    fn vitals(alive: bool, spawn_tick: u64, died_tick: Option<u64>) -> bri_sim::session::Vitals {
        bri_sim::session::Vitals {
            health: if alive { 100.0 } else { 0.0 },
            alive,
            respawn_tick: 0,
            respawn_held: false,
            spawn_tick,
            died_tick,
            death: None,
            firing: false,
            fired_tick: None,
            score: 0,
            minigame: None,
            team: None,
            invite: None,
            light: false,
            mounted: None,
            ride: None,
            look_limits: None,
            control: Default::default(),
            camera_path: None,
            camera_point: None,
            talking: false,
            sitting: false,
            ghost: None,
        }
    }
    #[test]
    fn death_and_respawn_follow_the_drawn_poses_timeline() {
        let life = |body, dead| DrawnLife { body, dead };
        // Died at 100 and respawned at 200.
        let respawned = vitals(true, 200, Some(100));
        // Remotes are drawn behind the vitals: alive, then the corpse, then
        // the new body only once the drawn pose reaches the respawn.
        assert_eq!(drawn_life(&respawned, 90, None), life(None, false));
        assert_eq!(drawn_life(&respawned, 150, None), life(None, true));
        assert_eq!(drawn_life(&respawned, 200, None), life(Some(200), false));
        // A death the drawn pose has not reached yet.
        let dying = vitals(false, 10, Some(100));
        assert_eq!(drawn_life(&dying, 97, None), life(Some(10), false));
        assert_eq!(drawn_life(&dying, 103, None), life(Some(10), true));
        // A client's own respawned body can arrive before the vitals: the
        // pose names the new body, which is alive, not the old corpse.
        assert_eq!(drawn_life(&dying, 200, Some(200)), life(Some(200), false));
        assert_eq!(drawn_life(&dying, 150, Some(10)), life(Some(10), true));
        // Vitals ahead of the client's own pose: still the old corpse.
        assert_eq!(drawn_life(&respawned, 150, Some(10)), life(Some(10), true));
        assert_eq!(
            drawn_life(&respawned, 200, Some(200)),
            life(Some(200), false)
        );
    }
    #[test]
    fn water_holds_the_root_pose_like_v20() {
        let mut p = player();
        p.velocity = [0.0, 0.0, -7.0];
        assert_eq!(locomotion(&p, 0.5).sequence, "run", "wading runs");
        assert_eq!(locomotion(&p, 0.7).sequence, "root", "deeper stands");
        p.velocity = [0.0, -0.2, -7.0];
        assert_eq!(locomotion(&p, 0.3).sequence, "root", "sinking");
        p.velocity = [0.0, -20.0, 0.0];
        p.grounded = false;
        assert_eq!(locomotion(&p, 0.0).sequence, "fall");
        assert_eq!(locomotion(&p, 0.3).sequence, "root", "no fall in water");
        p.crouched = true;
        assert_eq!(locomotion(&p, 1.0).sequence, "crouch");
    }
    #[test]
    fn movement_pose_uses_body_facing_and_distinguishes_air_crouch_and_strafe() {
        let mut p = player();
        assert_eq!(locomotion(&p, 0.0).sequence, "root");
        p.velocity = [0.0, 0.0, -7.0];
        assert_eq!(locomotion(&p, 0.0).sequence, "run");
        p.yaw = std::f32::consts::PI;
        assert_eq!(locomotion(&p, 0.0).sequence, "back");
        p.crouched = true;
        assert_eq!(locomotion(&p, 0.0).sequence, "crouchback");
        p.velocity = [3.0, 0.0, 0.0];
        assert_eq!(locomotion(&p, 0.0).sequence, "crouchside");
        p.velocity = [0.0; 3];
        assert_eq!(locomotion(&p, 0.0).sequence, "crouch");
        p.grounded = false;
        p.velocity[1] = 4.0;
        assert_eq!(locomotion(&p, 0.0).sequence, "jump");
        p.velocity[1] = -4.0;
        assert_eq!(locomotion(&p, 0.0).sequence, "root");
        p.velocity[1] = -12.0;
        assert_eq!(locomotion(&p, 0.0).sequence, "fall");
        p.jetting = true;
        assert_eq!(locomotion(&p, 0.0).sequence, "root");
        p.velocity[1] = 4.0;
        assert_eq!(locomotion(&p, 0.0).sequence, "root");
    }

    #[test]
    fn pick_follows_torque_action_list_order_and_reverses_side_for_right() {
        let mut p = player();
        let action = |p: &PlayerState| {
            let a = locomotion(p, 0.0);
            (a.sequence, a.forward)
        };
        // Yaw 0 faces -Z with +X on the right.
        p.velocity = [-6.0, 0.0, 0.0];
        assert_eq!(action(&p), ("side", true));
        p.velocity = [6.0, 0.0, 0.0];
        assert_eq!(action(&p), ("side", false));
        // Exact diagonals tie; the strict comparison keeps the earlier entry.
        p.velocity = [4.0, 0.0, -4.0];
        assert_eq!(action(&p), ("run", true));
        p.velocity = [4.0, 0.0, 4.0];
        assert_eq!(action(&p), ("back", true));
        p.velocity = [4.001, 0.0, -4.0];
        assert_eq!(action(&p), ("side", false));
        // Every dot product must exceed 0.1.
        p.velocity = [0.1, 0.0, -0.1];
        assert_eq!(action(&p), ("root", true));
        p.crouched = true;
        p.velocity = [2.0, 0.0, 0.0];
        assert_eq!(action(&p), ("crouchside", false));
    }

    #[test]
    fn accessories_beside_the_posed_parts_ride_with_the_nearest_one() {
        // root: torso (placed) with the head (placed) under it; beside
        // them, off the root, a hat node over the head and a cape node on
        // the back, and a feather under the hat. A hand hangs under the
        // torso.
        let parents = [None, Some(0), Some(1), Some(0), Some(0), Some(3), Some(1)];
        let at = |x: f32, y: f32, z: f32| Mat4::from_translation(Vec3::new(x, y, z));
        let animated = [
            at(0.0, 0.0, 0.0),
            at(0.0, 1.0, 0.0),
            at(0.0, 2.0, 0.0),
            at(0.0, 2.6, 0.0),
            at(0.0, 1.5, 0.4),
            at(0.0, 3.0, 0.0),
            at(0.6, 1.0, 0.0),
        ];
        let block = |h: f32| Some([[-0.5, 0.0, -0.3], [0.5, h, 0.3]]);
        let bounds = [
            None,
            block(1.0),
            block(0.8),
            Some([[-0.5, 0.0, -0.5], [0.5, 0.4, 0.5]]),
            Some([[-0.5, -0.4, -0.05], [0.5, 0.4, 0.05]]),
            None,
            None,
        ];
        let mut placed = [None; 7];
        placed[1] = Some((Vec3::Y, Quat::IDENTITY));
        placed[2] = Some((Vec3::Y * 2.0, Quat::IDENTITY));
        let order = [0, 1, 3, 4, 2, 5, 6];
        let anchors = follow_anchors(&parents, &order, &placed, &animated, &bounds);
        // The root goes with the torso, the nearest; the hat with the head, the
        // cape with the torso, the feather with the hat's anchor; placed
        // nodes and the hand under the torso follow the tree.
        assert_eq!(
            anchors,
            [Some(1), None, None, Some(2), Some(1), Some(2), None]
        );
    }

    #[test]
    fn held_pose_uses_only_mounted_images_with_arm_ready() {
        assert_eq!(
            HeldToolPose::from_mounted_images([(0, false), (1, false)]),
            HeldToolPose::None
        );
        assert_eq!(
            HeldToolPose::from_mounted_images([(0, true), (1, false)]),
            HeldToolPose::Right
        );
        assert_eq!(
            HeldToolPose::from_mounted_images([(0, false), (1, true)]),
            HeldToolPose::Left
        );
        assert_eq!(
            HeldToolPose::from_mounted_images([(0, true), (1, true)]),
            HeldToolPose::Both
        );
        assert_eq!(
            HeldToolPose::from_mounted_images([(7, true)]),
            HeldToolPose::None
        );
    }

    /// A round that dresses every player builds a new mesh per body: the
    /// second body in an outfit uploads no texture, and a new face uploads
    /// only that face. A restructure still reuses the shared textures.
    fn new_outfits_bind_the_textures_bodies_already_uploaded(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let gpu = bri_ui::gpu::Headless::new()?;
        let renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let mut cache = bri_render::scene::TextureCache::default();
        let p = player();
        let dressed = |appearance: Appearance| -> Result<AvatarMesh> {
            let mut mesh = assets.mesh(appearance)?;
            mesh.pose(assets, &p, 0.0)?;
            Ok(mesh)
        };
        let defaults = assets.package.defaults.clone();
        let mut first = dressed(defaults.clone())?;
        assert!(first.needs_full_upload());
        first.upload_sharing(&renderer, &gpu.device, &gpu.queue, Some(&mut cache))?;
        assert!(!first.needs_full_upload());
        let made = renderer.upload_counts().textures;
        assert!(made > 0);
        assert_eq!(cache.len() as u64, made);
        let mut second = dressed(defaults.clone())?;
        second.upload_sharing(&renderer, &gpu.device, &gpu.queue, Some(&mut cache))?;
        assert_eq!(renderer.upload_counts().textures, made);
        let face = assets
            .package
            .faces
            .iter()
            .map(|f| f.rsplit('/').next().unwrap_or(f))
            .find(|f| !f.eq_ignore_ascii_case(&defaults.face))
            .context("a second face")?
            .to_string();
        let mut third = dressed(Appearance {
            face,
            ..defaults.clone()
        })?;
        third.upload_sharing(&renderer, &gpu.device, &gpu.queue, Some(&mut cache))?;
        assert_eq!(renderer.upload_counts().textures, made + 1);
        second.set_hidden_nodes(["LHand".to_string()]);
        second.pose(assets, &p, 0.1)?;
        assert!(second.restructured);
        let shared = renderer.upload_counts().shared_geometry;
        second.upload_sharing(&renderer, &gpu.device, &gpu.queue, Some(&mut cache))?;
        assert_eq!(renderer.upload_counts().shared_geometry, shared + 1);
        assert_eq!(renderer.upload_counts().textures, made + 1);
        Ok(())
    }
    /// Skis on and off, hands hidden and shown, a translucent paint: the
    /// body's layout changes but not its outfit, so it uploads new geometry
    /// on the textures it has, never the face, decal and surfaces again.
    fn a_restructured_body_keeps_its_textures(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let gpu = bri_ui::gpu::Headless::new()?;
        let renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let mut p = player();
        mesh.pose(assets, &p, 0.0)?;
        mesh.upload(&renderer, &gpu.device, &gpu.queue)?;
        let first = renderer.upload_counts();
        assert_eq!(first.scenes, 1);
        let mut layouts = 0;
        for frame in 1..12u8 {
            p.yaw = f32::from(frame) * 0.1;
            mesh.set_skis((2..4).contains(&frame).then_some([0.2, 0.4, 0.6, 1.0]));
            let hands = (6..8)
                .contains(&frame)
                .then(|| ["LHand".to_string(), "rhand".to_string()]);
            mesh.set_hidden_nodes(hands.into_iter().flatten());
            if frame == 10 {
                mesh.outfit
                    .nodes
                    .insert("chest".into(), [1.0, 0.0, 0.0, 0.5]);
            }
            mesh.pose(assets, &p, f64::from(frame) / 30.0)?;
            layouts += usize::from(mesh.restructured);
            mesh.upload(&renderer, &gpu.device, &gpu.queue)?;
            assert!(mesh.gpu.is_some());
        }
        assert!(layouts >= 3, "{layouts} layouts");
        let counts = renderer.upload_counts();
        assert_eq!(
            (counts.scenes, counts.textures),
            (first.scenes, first.textures),
            "a new layout uploaded the body's textures again"
        );
        assert_eq!(counts.shared_geometry, layouts as u64);
        Ok(())
    }

    fn reposed_vertices_match_a_full_shape_rebuild(fx: &Avatar) -> Result<()> {
        use bri_render::shape_scene::ShapeInstance;
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        mesh.defer_mesh = true;
        let mut p = player();
        let mut layouts = 0;
        for frame in 0..40 {
            p.feet = [frame as f32 * 0.3, 1.0, -2.0];
            p.velocity = [if frame < 20 { 5.0 } else { 0.0 }, 0.0, 0.0];
            p.yaw = frame as f32 * 0.1;
            p.pitch = (frame as f32 * 0.05).sin();
            p.crouched = (10..15).contains(&frame);
            p.grounded = frame % 13 != 0;
            // Skis on and off, and a translucent paint, restructure the mesh.
            mesh.set_skis((25..30).contains(&frame).then_some([0.2, 0.4, 0.6, 1.0]));
            if frame == 32 {
                mesh.outfit
                    .nodes
                    .insert("chest".into(), [1.0, 0.0, 0.0, 0.5]);
            }
            // A held image that draws its own hands hides the Blockhead's.
            let hands = (34..38)
                .contains(&frame)
                .then(|| ["LHand".to_string(), "rhand".to_string()]);
            mesh.set_hidden_nodes(hands.into_iter().flatten());
            mesh.pose(assets, &p, f64::from(frame) / 30.0)?;
            let pose = mesh.pending.take().context("A deferred pose")?;
            let mut reference = mesh.data.clone();
            reference.vertices.clear();
            reference.indices.clear();
            reference.batches.clear();
            reference.append_shape(
                ShapeInstance {
                    shape: &assets.rig.shape,
                    pose: &pose,
                    detail: assets.detail,
                    transform: mesh.model_transform,
                    materials: &mesh.materials,
                    translucent_materials: Some(&mesh.translucent_materials),
                    unassigned_material: mesh.materials[0],
                },
                |name| {
                    let name = name.to_ascii_lowercase();
                    mesh.outfit
                        .nodes
                        .get(&name)
                        .copied()
                        .filter(|_| !mesh.hidden_nodes.contains(&name))
                },
            )?;
            mesh.restructured = false;
            mesh.build_mesh(assets, &pose)?;
            layouts += usize::from(mesh.restructured);
            assert_eq!(mesh.data.indices, reference.indices, "frame {frame}");
            assert_eq!(mesh.data.batches.len(), reference.batches.len());
            for (a, b) in mesh.data.batches.iter().zip(&reference.batches) {
                assert_eq!((&a.indices, a.material), (&b.indices, b.material));
                assert_eq!(a.center, b.center, "frame {frame}");
            }
            assert_eq!(mesh.data.vertices.len(), reference.vertices.len());
            for (a, b) in mesh.data.vertices.iter().zip(&reference.vertices) {
                let fields = |v: &bri_render::scene::SceneVertex| {
                    (v.position, v.normal, v.uv, v.lightmap_uv, v.color, v.fx)
                };
                assert_eq!(fields(a), fields(b), "frame {frame}");
            }
        }
        // At least the first frame, skis on and off, the new paint and the
        // hands hidden and shown lay the mesh out again; every other frame
        // reuses the layout.
        assert!((4..12).contains(&layouts), "{layouts} layouts");
        Ok(())
    }

    fn a_respawned_body_stands_in_root_without_getting_up_from_the_corpse(
        fx: &Avatar,
    ) -> Result<()> {
        let assets = &fx.assets;
        let p = player();
        let dead = AvatarAnimationInput {
            dead: true,
            ..Default::default()
        };
        let alive = AvatarAnimationInput::default();
        // How far apart two poses are: the largest difference of any node.
        let apart = |a: &AvatarMesh, b: &AvatarMesh| {
            assert_eq!(a.posed_nodes.len(), b.posed_nodes.len());
            a.posed_nodes
                .iter()
                .zip(&b.posed_nodes)
                .flat_map(|(a, b)| (*a - *b).to_cols_array())
                .fold(0.0_f32, |most, d| most.max(d.abs()))
        };
        let mut fresh = assets.mesh(assets.package.defaults.clone())?;
        fresh.pose_with_animation(assets, &p, 10.0, &alive)?;
        let mut body = assets.mesh(assets.package.defaults.clone())?;
        assert!(!body.set_body(1));
        for frame in 0..60 {
            body.pose_with_animation(assets, &p, f64::from(frame) / 30.0, &dead)?;
        }
        let lying = apart(&body, &fresh);
        assert!(lying > 0.01, "death1 moves the body ({lying})");
        // Seeing the same body again changes nothing: it stays dead.
        assert!(!body.set_body(1));
        // A respawn is a new body: its first pose is a fresh body's.
        assert!(body.set_body(2));
        body.pose_with_animation(assets, &p, 2.1, &alive)?;
        assert_eq!(apart(&body, &fresh), 0.0);
        Ok(())
    }

    /// `bri_content::hold::AIM_HAND` is the real Blockhead's aiming hand:
    /// a fitted body's hand is turned to it, so an image held in either
    /// points the same way.
    fn the_blockheads_aiming_hand_is_the_frame_fitted_hands_take(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let input = AvatarAnimationInput {
            held_tool_pose: HeldToolPose::Right,
            ..Default::default()
        };
        mesh.pose_with_animation(assets, &player(), 0.0, &input)?;
        mesh.pose_with_animation(assets, &player(), 1.0, &input)?;
        let (_, rotation, at) = (mesh.body_transform().inverse()
            * mesh.mount_node(assets, 0).context("mount0")?)
        .to_scale_rotation_translation();
        let eye = mesh.eye_local(assets, false).context("eye")?;
        eprintln!(
            "Blockhead aiming Mount0: {rotation} at {at}, from the eye {}",
            at - eye
        );
        if fx.content {
            assert!(
                (at - eye).abs_diff_eq(bri_content::hold::FIRST_PERSON_HAND, 0.01),
                "{}",
                at - eye
            );
            assert!(
                rotation.angle_between(bri_content::hold::AIM_HAND) < 0.05,
                "{rotation}"
            );
        }
        Ok(())
    }
    fn item_mounts_use_the_same_sampled_avatar_pose_and_body_transform(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        assert!(mesh.world_node(assets, "mount0").is_none());
        let mut p = player();
        mesh.pose(assets, &p, 0.)?;
        let before = mesh
            .world_node(assets, "mount0")
            .context("Original avatar mount0")?;
        assert!(mesh.world_node(assets, "inventedHand").is_none());
        p.feet = [12., 3., -7.];
        p.yaw = 0.75;
        mesh.pose(assets, &p, 0.)?;
        let expected =
            Mat4::from_rotation_translation(Quat::from_rotation_y(-p.yaw), Vec3::from(p.feet))
                * before;
        assert!(
            mesh.world_node(assets, "MOUNT0")
                .unwrap()
                .abs_diff_eq(expected, 0.00001)
        );
        let hand0 = mesh.world_node(assets, "mount0").unwrap();
        let hand1 = mesh
            .world_node(assets, "mount1")
            .context("Original avatar mount1")?;
        assert!(!hand0.abs_diff_eq(hand1, 0.00001));
        Ok(())
    }

    fn held_and_action_arm_layers_keep_locomotion_and_mounts_coherent(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let arm_ready = assets
            .rig
            .sequence("armreadyright")
            .context("Vanilla armReadyRight")?;
        assert!(!arm_ready.additive);
        assert!(arm_ready.nodes.iter().all(|track| matches!(
            track.node.to_ascii_lowercase().as_str(),
            "rightarm" | "righthand"
        )));
        let attack = assets
            .rig
            .sequence("armattack")
            .context("Vanilla armAttack")?;
        let wrench = assets.rig.sequence("wrench").context("Vanilla wrench")?;
        assert!(attack.additive && wrench.additive);
        assert!(
            attack
                .nodes
                .iter()
                .any(|track| track.node.eq_ignore_ascii_case("Hip"))
        );
        assert!(
            wrench
                .nodes
                .iter()
                .any(|track| track.node.eq_ignore_ascii_case("RightLeg"))
        );
        let mut baseline = assets.mesh(assets.package.defaults.clone())?;
        let mut equipped = assets.mesh(assets.package.defaults.clone())?;
        let mut p = player();
        p.velocity = [0.0, 0.0, -4.0];
        baseline.pose(assets, &p, 0.25)?;
        equipped.pose_with_animation(
            assets,
            &p,
            0.25,
            &AvatarAnimationInput {
                held_tool_pose: HeldToolPose::Right,
                action: Some(ActionAnimation {
                    sequence: "wrench".into(),
                    started_at: 0.0,
                }),
                ..Default::default()
            },
        )?;
        assert!(
            baseline
                .world_node(assets, "RightLeg")
                .context("Baseline right leg")?
                .abs_diff_eq(
                    equipped
                        .world_node(assets, "RightLeg")
                        .context("Equipped right leg")?,
                    1e-5,
                )
        );
        assert_ne!(
            baseline.world_node(assets, "RightArm"),
            equipped.world_node(assets, "RightArm")
        );
        assert_eq!(
            equipped.world_node(assets, "mount0"),
            equipped.world_node(assets, "MOUNT0")
        );
        // A clip the body lacks is left out of the pose and named, not
        // the whole body refused (it used to fail the pose).
        let before = equipped.world_node(assets, "RightLeg");
        equipped.pose_with_animation(
            assets,
            &p,
            0.5,
            &AvatarAnimationInput {
                held_tool_pose: HeldToolPose::None,
                action: Some(ActionAnimation {
                    sequence: "missing-original-clip".into(),
                    started_at: 0.0,
                }),
                ..Default::default()
            },
        )?;
        assert_eq!(equipped.skipped_clips(), ["missing-original-clip"]);
        assert!(equipped.world_node(assets, "RightLeg").is_some() && before.is_some());
        Ok(())
    }

    /// A rule's `playThread(2, armReadyBoth)` or `death1` (Throwmod's
    /// holder and held player) are absolute clips: they take over the
    /// nodes they animate instead of being refused as non-additive.
    fn absolute_actions_take_over_the_nodes_they_animate(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        for name in ["armreadyboth", "death1"] {
            let clip = assets.rig.sequence(name).context(name)?;
            assert!(!clip.additive, "{name} is absolute");
        }
        let p = player();
        let mut baseline = assets.mesh(assets.package.defaults.clone())?;
        baseline.pose(assets, &p, 1.0)?;
        let acting = |sequence: &str| AvatarAnimationInput {
            action: Some(ActionAnimation {
                sequence: sequence.into(),
                started_at: 0.0,
            }),
            ..Default::default()
        };
        let mut raised = assets.mesh(assets.package.defaults.clone())?;
        raised.pose_with_animation(assets, &p, 1.0, &acting("armreadyboth"))?;
        for arm in ["LeftArm", "RightArm"] {
            assert_ne!(
                baseline.world_node(assets, arm),
                raised.world_node(assets, arm),
                "{arm} raised"
            );
        }
        let mut limp = assets.mesh(assets.package.defaults.clone())?;
        limp.pose_with_animation(assets, &p, 1.0, &acting("death1"))?;
        assert_ne!(
            baseline.world_node(assets, "Head"),
            limp.world_node(assets, "Head"),
            "the whole body goes limp"
        );
        Ok(())
    }

    fn held_arm_pose_precedes_additive_look_and_pack_headup_layers(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut appearance = assets.package.defaults.clone();
        appearance
            .parts
            .insert("pack".into(), assets.package.parts["pack"][1].clone());
        let mut baseline = assets.mesh(appearance.clone())?;
        let mut held = assets.mesh(appearance)?;
        let mut p = player();
        p.yaw = 0.4;
        p.pitch = 0.65;
        baseline.pose(assets, &p, 0.0)?;
        held.pose_with_animation(
            assets,
            &p,
            0.0,
            &AvatarAnimationInput {
                held_tool_pose: HeldToolPose::Right,
                action: None,
                ..Default::default()
            },
        )?;
        assert_ne!(
            baseline.world_node(assets, "RightArm"),
            held.world_node(assets, "RightArm")
        );
        assert_eq!(
            baseline.world_node(assets, "Head"),
            held.world_node(assets, "Head")
        );
        assert!(held.world_node(assets, "Mount0").is_some());
        let raw_eye = held
            .world_node(assets, "Eye")
            .context("Original Eye node")?;
        let eye = held
            .eye_transform(assets, p.yaw, p.pitch)
            .context("Engine-style eye frame")?;
        assert!(
            eye.w_axis
                .truncate()
                .abs_diff_eq(raw_eye.w_axis.truncate(), 1e-5)
        );
        let expected_forward = Vec3::new(
            p.yaw.sin() * p.pitch.cos(),
            p.pitch.sin(),
            -p.yaw.cos() * p.pitch.cos(),
        );
        assert!(
            eye.transform_vector3(-Vec3::Z)
                .abs_diff_eq(expected_forward, 1e-5)
        );
        assert!(eye.transform_vector3(-Vec3::Z).y > 0.0);
        Ok(())
    }

    fn rebuilt_outfit_continues_the_running_clip(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut kept = assets.mesh(assets.package.defaults.clone())?;
        let mut p = player();
        p.velocity = [0.0, 0.0, -7.0];
        for frame in 0..20 {
            kept.pose(assets, &p, f64::from(frame) / 60.0)?;
        }
        let mut rebuilt = assets.mesh(assets.package.defaults.clone())?;
        rebuilt.continue_animation(&kept);
        let time = 20.0 / 60.0;
        kept.pose(assets, &p, time)?;
        rebuilt.pose(assets, &p, time)?;
        let leg = |mesh: &AvatarMesh| mesh.world_node(assets, "RightLeg").unwrap();
        assert!(leg(&kept).abs_diff_eq(leg(&rebuilt), 1e-5));
        let mut fresh = assets.mesh(assets.package.defaults.clone())?;
        fresh.pose(assets, &p, time)?;
        assert!(!leg(&kept).abs_diff_eq(leg(&fresh), 1e-3));
        Ok(())
    }

    fn mount_action_is_only_the_playing_actions_motion(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let mut p = player();
        p.pitch = 0.4;
        p.velocity = [0.0, 0.0, -4.0];
        let input = |gesture: Option<&str>| AvatarAnimationInput {
            held_tool_pose: HeldToolPose::Right,
            gesture: gesture.map(|sequence| ActionAnimation {
                sequence: sequence.into(),
                started_at: 0.0,
            }),
            ..Default::default()
        };
        mesh.pose_with_animation(assets, &p, 0.1, &input(None))?;
        assert_eq!(mesh.mount_action(assets, 0), None);
        let rest = mesh.model_node(assets, "Mount0").context("Mount0")?;
        let mut shifted = assets.mesh(assets.package.defaults.clone())?;
        shifted.pose_with_animation(assets, &p, 0.1, &input(Some("shiftAway")))?;
        let action = shifted
            .mount_action(assets, 0)
            .context("shiftAway moves the right hand")?;
        let moved = shifted.model_node(assets, "Mount0").context("Mount0")?;
        assert!(!moved.abs_diff_eq(rest, 1e-3));
        // The same walk, armReady and look, plus exactly the action.
        assert!((rest * action).abs_diff_eq(moved, 1e-4));
        Ok(())
    }

    fn seated_body_takes_the_mount_rotation(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let tilt = Quat::from_rotation_y(0.6) * Quat::from_rotation_x(0.35);
        mesh.pose_with_animation(
            assets,
            &player(),
            0.0,
            &AvatarAnimationInput {
                mount_rotation: Some(tilt),
                sitting: true,
                ..Default::default()
            },
        )?;
        let up = mesh.body_transform().transform_vector3(Vec3::Y);
        assert!(up.dot(tilt * Vec3::Y) > 0.999, "{up}");
        Ok(())
    }
    fn a_seated_rider_reaches_the_seats_hand_holds(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let mut input = AvatarAnimationInput {
            mount_rotation: Some(Quat::from_rotation_y(0.4)),
            sitting: true,
            ..Default::default()
        };
        mesh.pose_with_animation(assets, &player(), 0.0, &input)?;
        let hand = |mesh: &AvatarMesh, n: usize| {
            mesh.mount_node(assets, n)
                .map(|m| m.w_axis.truncate())
                .context("hand")
        };
        let rest = [hand(&mesh, 0)?, hand(&mesh, 1)?];
        // Each hand a little forward and up of where the sit leaves it.
        let forward = Quat::from_rotation_y(0.4) * Vec3::new(0.0, 0.12, -0.12);
        input.hand_holds = [Some(rest[0] + forward), Some(rest[1] + forward)];
        mesh.pose_with_animation(assets, &player(), 0.0, &input)?;
        for (n, rest) in rest.into_iter().enumerate() {
            let reached = hand(&mesh, n)?.distance(rest + forward);
            assert!(reached < 0.06, "hand {n} {reached} from its hold");
        }
        Ok(())
    }
    fn jet_history_survives_skipped_poses_and_first_remote_observation(fx: &Avatar) -> Result<()> {
        use bri_sim::player::{MoveInput, Player, PlayerTuning};
        use rapier3d::prelude::{ColliderBuilder, Vector};

        let assets = &fx.assets;
        let mut physics = bri_physics::new_world();
        physics.insert_collider(
            ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            None,
        );
        physics.detect_collisions(&(), &());
        let mut motor = Player::spawn(
            &mut physics,
            1,
            Vec3::new(0.0, 0.05, 0.0),
            PlayerTuning::default(),
        )?;
        for _ in 0..10 {
            motor.torque_tick(&mut physics, MoveInput::default(), &[], 0.032)?;
        }
        assert!(motor.state().grounded);
        let mut local = assets.mesh(assets.package.defaults.clone())?;
        local.defer_mesh = true;
        local.pose(assets, motor.state(), 0.0)?;
        let mut remote = assets.mesh(assets.package.defaults.clone())?;
        remote.defer_mesh = true;
        let mut first_seen = assets.mesh(assets.package.defaults.clone())?;
        first_seen.defer_mesh = true;
        let ground = bri_net::protocol::RemotePose::of(0, motor.state())
            .into_pose()
            .player;
        remote.pose(assets, &ground, 0.0)?;
        // Neither avatar is posed during this short jet burst. The remote
        // also loses every intermediate snapshot containing active jets.
        for _ in 0..8 {
            motor.torque_tick(
                &mut physics,
                MoveInput {
                    jet: true,
                    ..Default::default()
                },
                &[],
                0.032,
            )?;
        }
        motor.torque_tick(&mut physics, MoveInput::default(), &[], 0.032)?;
        let released = motor.state();
        assert!(
            !released.jetting && !released.grounded && released.velocity[1] > 0.5,
            "released but still rising: {released:?}"
        );
        let wire = bri_net::protocol::RemotePose::of(9, released)
            .into_pose()
            .player;
        for (mesh, state) in [
            (&mut local, released),
            (&mut remote, &wire),
            (&mut first_seen, &wire),
        ] {
            mesh.pose(assets, state, 0.288)?;
            assert_eq!(
                mesh.mode, "root",
                "an unseen jet burst must not invent a jump"
            );
        }
        let standing = PlayerState {
            velocity: [0.0; 3],
            grounded: true,
            ..released.clone()
        };
        let mut reference = assets.mesh(assets.package.defaults.clone())?;
        reference.defer_mesh = true;
        reference.pose(assets, &standing, 0.288)?;
        for name in ["Hip", "Torso", "LeftLeg", "RightLeg"] {
            let node = assets.node_index[&name.to_ascii_lowercase()];
            for mesh in [&local, &remote, &first_seen] {
                assert!(
                    mesh.posed_nodes[node].abs_diff_eq(reference.posed_nodes[node], 1e-5),
                    "{name} bends after an unseen jet burst"
                );
            }
        }
        // Skip presentation through the landing too. History must end in
        // the motor so the next physical jump works without a grounded pose.
        for _ in 0..120 {
            motor.torque_tick(&mut physics, MoveInput::default(), &[], 0.032)?;
            if motor.state().grounded {
                break;
            }
        }
        assert!(motor.state().grounded && !motor.state().jump.jet_flight);
        let event = motor.torque_tick(
            &mut physics,
            MoveInput {
                jump: true,
                ..Default::default()
            },
            &[],
            0.032,
        )?;
        assert!(event.jumped && !motor.state().jump.jet_flight);
        let wire = bri_net::protocol::RemotePose::of(100, motor.state())
            .into_pose()
            .player;
        for (mesh, state) in [
            (&mut local, motor.state()),
            (&mut remote, &wire),
            (&mut first_seen, &wire),
        ] {
            mesh.pose(assets, state, 4.0)?;
            assert_eq!(mesh.mode, "jump", "a hidden landing re-arms ordinary jumps");
        }
        Ok(())
    }

    fn jet_release_keeps_the_root_pose_until_landing(fx: &Avatar) -> Result<()> {
        use bri_sim::player::{MoveInput, Player, PlayerTuning};
        use rapier3d::prelude::{ColliderBuilder, Vector};

        let assets = &fx.assets;
        for held in [HeldToolPose::None, HeldToolPose::Right, HeldToolPose::Both] {
            let mut physics = bri_physics::new_world();
            physics.insert_collider(
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
                None,
            );
            physics.detect_collisions(&(), &());
            let mut motor = Player::spawn(
                &mut physics,
                1,
                Vec3::new(0.0, 0.05, 0.0),
                PlayerTuning::default(),
            )?;
            for _ in 0..10 {
                motor.torque_tick(&mut physics, MoveInput::default(), &[], 0.032)?;
            }
            assert!(motor.state().grounded, "take off from the actual floor");
            let mesh = || -> Result<AvatarMesh> {
                let mut mesh = assets.mesh(assets.package.defaults.clone())?;
                mesh.defer_mesh = true;
                Ok(mesh)
            };
            let mut local = mesh()?;
            let mut remote = mesh()?;
            let mut reference = mesh()?;
            let mut rising_releases = 0;
            let mut fell = false;
            let mut landed = false;
            for tick in 0..180 {
                let jet = tick < 80;
                motor.torque_tick(
                    &mut physics,
                    MoveInput {
                        jet,
                        ..Default::default()
                    },
                    &[],
                    0.032,
                )?;
                let p = motor.state();
                // The normal remote wire path drops jump timers. The fix
                // must work from the replicated flight-history flag.
                let wire = bri_net::protocol::RemotePose::of(tick, p)
                    .into_pose()
                    .player;
                let time = tick as f64 * 0.032;
                let input = AvatarAnimationInput {
                    held_tool_pose: held,
                    ..Default::default()
                };
                local.pose_with_animation(assets, p, time, &input)?;
                remote.pose_with_animation(assets, &wire, time, &input)?;
                assert_ne!(
                    local.mode, "jump",
                    "jet flight invented a jump at tick {tick}"
                );
                assert_eq!(local.mode, remote.mode, "remote pose at tick {tick}");
                if !jet && !p.grounded && p.velocity[1] > 0.5 {
                    rising_releases += 1;
                    assert_eq!(local.mode, "root");
                    let standing = PlayerState {
                        velocity: [0.0; 3],
                        grounded: true,
                        jetting: false,
                        ..p.clone()
                    };
                    reference.pose_with_animation(assets, &standing, time, &input)?;
                    for name in ["Hip", "Torso", "LeftLeg", "RightLeg"] {
                        let node = assets.node_index[&name.to_ascii_lowercase()];
                        assert!(
                            local.posed_nodes[node].abs_diff_eq(reference.posed_nodes[node], 1e-5),
                            "{name} bends after release at tick {tick}"
                        );
                        assert!(
                            remote.posed_nodes[node].abs_diff_eq(reference.posed_nodes[node], 1e-5),
                            "remote {name} bends after release at tick {tick}"
                        );
                    }
                    if rising_releases == 1 {
                        // Changing clothes while rising must retain the latch.
                        let mut rebuilt = mesh()?;
                        rebuilt.continue_animation(&local);
                        local = rebuilt;
                    }
                }
                fell |= local.mode == "fall";
                if !jet && p.grounded {
                    landed = true;
                    break;
                }
            }
            assert!(rising_releases > 0, "exercise release while still rising");
            assert!(
                fell && landed,
                "fall animation and landing remain available"
            );
            let event = motor.torque_tick(
                &mut physics,
                MoveInput {
                    jump: true,
                    ..Default::default()
                },
                &[],
                0.032,
            )?;
            assert!(event.jumped, "ordinary jump after landing");
            local.pose_with_animation(
                assets,
                motor.state(),
                6.0,
                &AvatarAnimationInput {
                    held_tool_pose: held,
                    ..Default::default()
                },
            )?;
            assert_eq!(local.mode, "jump", "landing restores normal jumps");
        }
        Ok(())
    }

    fn jets_interrupt_a_jump_without_restarting_it_on_release(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        mesh.defer_mesh = true;
        let mut p = player();
        mesh.pose(assets, &p, 0.0)?;
        p.grounded = false;
        p.velocity[1] = 12.0;
        mesh.pose(assets, &p, 0.032)?;
        assert_eq!(mesh.mode, "jump", "ordinary floor jump");
        p.jetting = true;
        p.jump.jet_flight = true;
        let presented = PlayerState {
            jetting: false,
            ..p.clone()
        };
        mesh.pose_with_animation(
            assets,
            &presented,
            0.064,
            &AvatarAnimationInput {
                tick_state: Some(p.clone()),
                ..Default::default()
            },
        )?;
        assert_eq!(mesh.mode, "root");
        p.jetting = false;
        for i in 0..20 {
            let presented = PlayerState {
                grounded: true,
                ..p.clone()
            };
            mesh.pose_with_animation(
                assets,
                &presented,
                0.096 + i as f64 / 60.0,
                &AvatarAnimationInput {
                    tick_state: Some(p.clone()),
                    ..Default::default()
                },
            )?;
            assert_eq!(mesh.mode, "root", "midair jets cannot replay jump");
        }
        // A respawn is a new body, not the old body's flight.
        mesh.set_body(1);
        assert!(mesh.set_body(2));
        p.jump.jet_flight = false;
        mesh.pose(assets, &p, 1.0)?;
        assert_eq!(mesh.mode, "jump", "respawn forgets the previous flight");
        Ok(())
    }

    fn getting_up_eases_out_of_the_sit_over_one_transition(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let sitting = AvatarAnimationInput {
            sitting: true,
            ..Default::default()
        };
        let standing = AvatarAnimationInput::default();
        let mut p = player();
        let eye = |mesh: &AvatarMesh| -> Result<f32> {
            Ok(mesh.world_node(assets, "Eye").context("Eye node")?.w_axis.y)
        };
        for frame in 0..30 {
            mesh.pose_with_animation(assets, &p, f64::from(frame) / 30.0, &sitting)?;
        }
        assert_eq!(mesh.getting_up(), None, "sitting is not getting up");
        let sat = eye(&mesh)?;
        let up = 1.0;
        mesh.pose_with_animation(assets, &p, up, &standing)?;
        assert_eq!(mesh.getting_up(), Some(1.0));
        assert!(
            (eye(&mesh)? - sat).abs() < 1e-4,
            "the body leaves the sit as it was"
        );
        // Running off part way up starts another transition; the rise keeps
        // its own clock.
        p.velocity = [0.0, 0.0, -5.0];
        mesh.pose_with_animation(assets, &p, up + TRANSITION_TIME / 2.0, &standing)?;
        assert_eq!(mesh.action().0, "run");
        let half = mesh.getting_up().context("still getting up")?;
        assert!((half - 0.5).abs() < 1e-4, "{half}");
        mesh.pose_with_animation(assets, &p, up + TRANSITION_TIME, &standing)?;
        assert_eq!(mesh.getting_up(), None, "upright after one transition");
        // Sitting back down part way up is sitting, not getting up.
        mesh.pose_with_animation(assets, &p, up + 2.0, &sitting)?;
        mesh.pose_with_animation(assets, &p, up + 3.0, &standing)?;
        mesh.pose_with_animation(assets, &p, up + 3.0 + TRANSITION_TIME / 2.0, &sitting)?;
        assert_eq!(mesh.getting_up(), None);
        Ok(())
    }
    #[test]
    fn seated_look_limits_bound_the_arms_not_the_view() {
        // The Jeep's `setLookLimits(0.65, 0.45)`.
        let limits = Some([0.45, 0.65]);
        assert_eq!(super::look_position(0.0, limits), 0.5);
        assert_eq!(super::look_position(1.2, limits), 0.45);
        assert_eq!(super::look_position(-1.2, limits), 0.65);
        assert!(
            (super::look_position(-1.2, None) - (0.5 + 1.2 / std::f32::consts::PI)).abs() < 1e-6
        );
    }
    fn free_look_turns_only_the_head_toward_the_camera(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let mut p = player();
        mesh.pose(assets, &p, 0.0)?;
        let head = mesh.world_node(assets, "Head").context("Head node")?;
        let torso = mesh.world_node(assets, "Torso").context("Torso node")?;
        // Positive yaw turns right (+X from the -Z forward).
        p.head_yaw = 1.0;
        mesh.pose(assets, &p, 1.0)?;
        let turned = mesh.world_node(assets, "Head").context("Head node")?;
        let (_, rest, _) = head.to_scale_rotation_translation();
        let (_, now, _) = turned.to_scale_rotation_translation();
        let look = (now * rest.inverse()) * -Vec3::Z;
        assert!(look.x > 0.5, "head turns toward the camera: {look}");
        assert!(look.y.abs() < 0.1, "free look turns, not tilts: {look}");
        assert_eq!(mesh.world_node(assets, "Torso"), Some(torso));
        Ok(())
    }

    fn original_outfits_materials_and_customization_rules(fx: &Avatar) -> Result<()> {
        let assets = &fx.assets;
        let package = &assets.package;
        let mut cases = 0;
        for (slot, options) in &package.parts {
            if slot == "accent" {
                continue;
            }
            for option in options {
                let mut appearance = package.defaults.clone();
                appearance.parts.insert(slot.clone(), option.clone());
                let mut mesh = assets.mesh(appearance)?;
                mesh.pose(assets, &player(), 0.0)?;
                mesh.data.validate()?;
                assert!(!mesh.data.vertices.is_empty());
                cases += 1;
            }
        }
        let mut skirt = package.defaults.clone();
        for (hat_name, accents) in &package.accents_allowed {
            for accent in accents {
                let mut appearance = package.defaults.clone();
                appearance.parts.insert("hat".into(), hat_name.clone());
                appearance.parts.insert("accent".into(), accent.clone());
                let mut mesh = assets.mesh(appearance)?;
                mesh.pose(assets, &player(), 0.0)?;
                mesh.data.validate()?;
                cases += 1;
            }
        }
        skirt.parts.insert("hip".into(), "skirthip".into());
        skirt.colors.insert("lleg".into(), [0.3, 0.6, 0.9, 0.2]);
        let outfit = package.resolve(&skirt)?;
        assert!(!outfit.nodes.contains_key("lshoe") && !outfit.nodes.contains_key("rshoe"));
        assert_eq!(outfit.nodes["skirttrimleft"], [0.3, 0.6, 0.9, 1.0]);
        skirt.parts.insert("lleg".into(), "nosuchleg".into());
        assert!(package.resolve(&skirt).is_err());
        let mut hat = package.defaults.clone();
        hat.parts
            .insert("hat".into(), package.parts["hat"][1].clone());
        hat.parts.insert("accent".into(), "visor".into());
        let outfit = package.resolve(&hat)?;
        assert!(outfit.nodes.contains_key("visor"));
        // The accent keeps its alpha (above the engine's floor).
        assert_eq!(
            outfit.nodes["visor"][3],
            package.defaults.colors["accent"][3].max(0.2)
        );
        hat.parts
            .insert("hat".into(), package.parts["hat"][2].clone());
        assert!(package.resolve(&hat).is_err());
        let mut pack = package.defaults.clone();
        pack.parts
            .insert("pack".into(), package.parts["pack"][1].clone());
        assert!(package.resolve(&pack)?.head_up);
        let mut selected = package.defaults.clone();
        for (kind, choices) in [("face", &package.faces), ("decal", &package.decals)] {
            for image in choices {
                if kind == "face" {
                    selected.face = image.clone();
                } else {
                    selected.decal = image.clone();
                }
                let mesh = assets.mesh(selected.clone())?;
                assert!(mesh.data.images.iter().any(|i| &i.label == image));
                cases += 1;
            }
        }
        selected
            .colors
            .insert("head".into(), [f32::NAN, 0.0, 0.0, 1.0]);
        assert!(package.resolve(&selected).is_err());
        if fx.content {
            let output = repo().join("artifacts/native-avatar/outfits.json");
            std::fs::create_dir_all(output.parent().unwrap())?;
            std::fs::write(
                output,
                serde_json::to_vec_pretty(
                    &serde_json::json!({"bound_cases":cases,"skirt_leg_rules":true,"hat_accent_rules":true,"hidden_choice_validated":true,"pack_head_pose":true,"original_textures_verified":package.textures.len()}),
                )?,
            )?;
        }
        Ok(())
    }

    /// The real pack's catalog size.
    #[test]
    #[ignore = "requires generated v20 content"]
    fn original_avatar_catalog_counts() -> Result<()> {
        let package = &Avatar::content()?.assets.package;
        assert_eq!(
            (
                package.faces.len(),
                package.decals.len(),
                package.textures.len()
            ),
            (27, 28, 63)
        );
        Ok(())
    }

    /// Max, v0.2.4: with the Ragdoll on, the puff of smoke a corpse
    /// vanishes in came out where the corpse died, not where the ragdoll
    /// lay. The puff (and its sound) is placed on the body as drawn, also
    /// once the body is gone, and only until a new body spawns.
    fn a_corpse_vanishes_where_its_body_is_drawn(fx: &Avatar) -> Result<()> {
        use bri_sim::presentation::{Cue, CueKind};
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        mesh.defer_mesh = true;
        mesh.set_body(10);
        let mut p = player();
        p.feet = [3.0, 0.0, -2.0];
        mesh.pose(assets, &p, 0.0)?;
        ensure!(mesh.drawn_offset().is_none(), "the game draws it");
        // Add-On code draws the whole body four units away.
        let slid = Vec3::new(4.0, 0.0, 1.0);
        let skeleton = mesh.skeleton(assets);
        let nodes: Vec<bri_client_sandbox::bodies::PosedNode> = skeleton
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let (_, rotation, position) =
                    Mat4::from_cols_array(node).to_scale_rotation_translation();
                (i as u32, (position + slid).to_array(), rotation.to_array())
            })
            .collect();
        mesh.override_nodes(assets, &nodes);
        mesh.remember_drawn_offset(false);
        let offset = mesh.drawn_offset().context("drawn elsewhere")?;
        ensure!(offset.distance(slid) < 1e-3, "drawn {offset}, not {slid}");

        let feet = p.feet;
        let effect = |actor: u64, at: [f32; 3]| Cue {
            id: 1,
            tick: 600,
            position: at,
            kind: CueKind::WeaponEffect {
                source: bri_weapons::TargetId::Actor(bri_weapons::ActorId(actor)),
                definition: "deathExplosion".into(),
                node: String::new(),
                seconds: 0.0,
                image: None,
                hand: None,
                direction: None,
                scale: 1.0,
            },
        };
        let sound = |at: [f32; 3]| Cue {
            id: 2,
            tick: 600,
            position: at,
            kind: CueKind::WeaponSound {
                profile: "deathExplosionSound".into(),
            },
        };
        let away = [40.0, 0.0, 40.0];
        let check = |mesh: &AvatarMesh, offset: Vec3| -> Result<()> {
            // The corpse's puff and sound, another player's blast where it
            // died and the dead player's own shot landing far off.
            let mut cues = vec![
                sound(feet),
                effect(1, feet),
                effect(2, feet),
                effect(1, away),
            ];
            follow_drawn_bodies(&mut cues, |owner| (owner == 1).then_some(mesh));
            let drawn = (Vec3::from(feet) + offset).to_array();
            let at: Vec<_> = cues.iter().map(|c| c.position).collect();
            let near = at
                .iter()
                .zip([drawn, drawn, feet, away])
                .all(|(a, b)| Vec3::from(*a).distance(Vec3::from(b)) < 1e-4);
            ensure!(near, "cues at {at:?}, the body drawn at {drawn:?}");
            Ok(())
        };
        check(&mesh, slid)?;
        // The body vanishes: the game poses it again, unseen, and the puff
        // can come after.
        mesh.pose(assets, &p, 0.1)?;
        mesh.remember_drawn_offset(true);
        check(&mesh, slid)?;
        // Drawn again where the game has it (a ragdoll gave up): no offset.
        mesh.remember_drawn_offset(false);
        check(&mesh, Vec3::ZERO)?;
        // A new body forgets where the old one was drawn.
        mesh.override_nodes(assets, &nodes);
        mesh.remember_drawn_offset(false);
        ensure!(mesh.drawn_offset().is_some());
        mesh.set_body(20);
        ensure!(
            mesh.drawn_offset().is_none(),
            "a new body starts where it is"
        );
        Ok(())
    }

    /// The Ragdoll Add-On (`packages/showcase/ragdoll`) on the real
    /// Blockhead: each part's box sits on the geometry drawn for that part,
    /// posing the body from the fresh ragdoll draws it where it stood, and
    /// the ragdoll falls to a floor in one piece. Prints every box. Run on
    /// a PC with content:
    /// `cargo test -p bri-client --lib ragdoll_on_the_real_blockhead -- --ignored --nocapture`
    /// (`BRI_CONTENT` names another content folder).
    fn ragdoll_on_the_real_blockhead(fx: &Avatar) -> Result<()> {
        use bri_client_sandbox::bodies::{PhysicsCommand, Shape};
        use bri_client_sandbox::{AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, World};
        const PARTS: [&str; 10] = [
            "chest", "femchest", "pants", "headskin", "rarm", "larm", "rhand", "lhand", "rshoe",
            "lshoe",
        ];
        let assets = &fx.assets;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        mesh.defer_mesh = true;
        let mut p = player();
        p.feet = [3.0, 0.0, -2.0];
        p.yaw = 0.7;
        mesh.pose(assets, &p, 0.0)?;
        let skeleton = mesh.skeleton(assets);
        let pose = mesh.pending.take().context("a deferred pose")?;
        mesh.build_mesh(assets, &pose)?;
        let drawn: Vec<Vec3> = mesh
            .data
            .vertices
            .iter()
            .map(|v| Vec3::from(v.position))
            .collect();
        ensure!(drawn.len() > 100, "{} vertices drawn", drawn.len());

        // The player dies where they stand.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase/ragdoll");
        let code = AddOnCode::load(&dir)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?
            .context("the Ragdoll has client code")?;
        let mut addon = Sandbox::new()?
            .start_in(&code, Budgets::untimed(), TrustLevel::Sandboxed, 0)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let world = Arc::new(World {
            local: 1,
            players: vec![bri_client_sandbox::world::Player {
                id: 1,
                alive: false,
                feet: p.feet,
                ..Default::default()
            }],
            skeletons: [(1, skeleton.clone())].into(),
            ..Default::default()
        });
        let physics = crate::addon_physics::AddOnPhysics::default();
        let frame = |addon: &mut bri_client_sandbox::AddOn,
                     physics: &crate::addon_physics::AddOnPhysics|
         -> Result<bri_client_sandbox::host::Frame> {
            addon
                .frame(FrameInput {
                    dt: 1.0 / 60.0,
                    world: world.clone(),
                    bodies: physics.snapshot(),
                    ..Default::default()
                })
                .cloned()
                .map_err(|e| anyhow::anyhow!("{e}"))
        };
        let first = frame(&mut addon, &physics)?;
        let bodies: Vec<_> = first
            .physics
            .iter()
            .filter_map(|c| match c {
                PhysicsCommand::Create { body, spec } => Some((*body, *spec)),
                _ => None,
            })
            .collect();
        let joints = first
            .physics
            .iter()
            .filter(|c| matches!(c, PhysicsCommand::Joint { .. }))
            .count();
        println!("{} bodies, {joints} joints", bodies.len());
        ensure!(bodies.len() >= 6, "too few parts found");
        ensure!(
            joints + 1 == bodies.len(),
            "every part but the root is joined"
        );

        // Each box against the vertices drawn round it.
        let node_at = |i: usize| Mat4::from_cols_array(&skeleton.nodes[i]);
        let mut problems = Vec::new();
        let mut centres = std::collections::BTreeMap::new();
        for part in PARTS {
            let node = skeleton.rig.part(part);
            if node < 0 {
                println!("{part:9} not in the rig");
                continue;
            }
            let node = node as usize;
            let at = node_at(node).w_axis.truncate();
            let Some((_, spec)) = bodies
                .iter()
                .find(|(_, s)| Vec3::from(s.position).distance(at) < 1e-3)
            else {
                println!(
                    "{part:9} node {} has no box (nothing drawn on it?)",
                    skeleton.rig.names[node]
                );
                continue;
            };
            let Shape::Box(half) = spec.shape else {
                problems.push(format!("{part}: not a box"));
                continue;
            };
            let rotation = Quat::from_array(spec.rotation);
            let centre = at + rotation * Vec3::from(spec.offset);
            let half = Vec3::from(half);
            let inside = |v: &Vec3, grow: f32| {
                let local = rotation.inverse() * (*v - centre);
                (local.abs() - half * grow).max_element() <= 0.0
            };
            // Blockhead parts are boxes, drawn mostly at their corners.
            let covered = drawn.iter().filter(|v| inside(v, 1.25)).count();
            println!(
                "{part:9} node {:12} box centre {:>6.2?} half {:>5.2?} vertices inside {covered}",
                skeleton.rig.names[node],
                centre.to_array(),
                half.to_array(),
            );
            centres.insert(part, centre);
            if !centre.is_finite() || half.min_element() < 0.02 || half.max_element() > 1.5 {
                problems.push(format!("{part}: box {half} is not limb-sized"));
            }
            if covered < 8 {
                problems.push(format!("{part}: only {covered} drawn vertices in its box"));
            }
        }
        // Anatomy: head over chest over pants over shoes; arms, hands and
        // shoes apart side to side.
        let y = |part: &str| centres.get(part).map(|c: &Vec3| c.y);
        let chest = y("chest").or(y("femchest"));
        for (upper, lower) in [
            (y("headskin"), chest),
            (chest, y("pants")),
            (y("pants"), y("lshoe")),
        ] {
            if let (Some(u), Some(l)) = (upper, lower)
                && u <= l
            {
                problems.push(format!(
                    "parts out of order top to bottom: {u} not above {l}"
                ));
            }
        }
        for (a, b) in [("rarm", "larm"), ("rhand", "lhand"), ("rshoe", "lshoe")] {
            if let (Some(a), Some(b)) = (centres.get(a), centres.get(b)) {
                let apart = (*a - *b).with_y(0.0).length();
                if apart < 0.3 {
                    problems.push(format!("{a} and {b} only {apart} apart"));
                }
            }
        }

        // Posed from the fresh ragdoll, the body is drawn as it stood.
        let still: std::collections::BTreeMap<_, _> = bodies
            .iter()
            .map(|(body, spec)| {
                let state = bri_client_sandbox::bodies::BodyState {
                    position: spec.position,
                    rotation: spec.rotation,
                    velocity: [0.0; 3],
                    spin: [0.0; 3],
                    resting: false,
                    shared: spec.shared,
                    group: spec.group,
                    mass: 1.0,
                    radius: 0.5,
                };
                (*body, state)
            })
            .collect();
        let posed = addon
            .frame(FrameInput {
                dt: 1.0 / 60.0,
                world: world.clone(),
                bodies: Arc::new(still),
                ..Default::default()
            })
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .clone();
        let nodes = &posed
            .poses
            .first()
            .context("the ragdoll posed the body")?
            .nodes;
        mesh.pose(assets, &p, 0.0)?;
        mesh.override_nodes(assets, nodes);
        let pose = mesh.pending.take().context("a deferred pose")?;
        mesh.build_mesh(assets, &pose)?;
        let jump = mesh
            .data
            .vertices
            .iter()
            .zip(&drawn)
            .map(|(v, d)| Vec3::from(v.position).distance(*d))
            .fold(0.0, f32::max);
        println!("posed from the fresh ragdoll, vertices moved at most {jump:.3}");
        if jump > 0.05 {
            problems.push(format!("the body jumps {jump} when the ragdoll takes over"));
        }

        // Four seconds later it lies on the floor in one piece.
        let floor = crate::brick_debris::tests::building(&[]).0;
        let mut physics = crate::addon_physics::AddOnPhysics::default();
        let mut addon = Sandbox::new()?
            .start_in(&code, Budgets::untimed(), TrustLevel::Sandboxed, 0)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut worst = 0.0f32;
        for _ in 0..240 {
            let out = frame(&mut addon, &physics)?;
            physics.apply(&out.physics);
            let start = std::time::Instant::now();
            physics.advance(1.0 / 60.0, &floor, &[], &[])?;
            worst = worst.max(start.elapsed().as_secs_f32() * 1000.0);
        }
        let rest = physics.snapshot();
        let feet = Vec3::from(p.feet);
        for (id, body) in rest.iter() {
            let at = Vec3::from(body.position);
            println!("resting body {id}: {:>6.2?}", at.to_array());
            if !at.is_finite() || !(-0.2..1.5).contains(&at.y) || at.distance(feet) > 4.0 {
                problems.push(format!("body {id} ended at {at}"));
            }
        }
        println!("slowest physics frame {worst:.2} ms (debug build)");
        ensure!(problems.is_empty(), "{problems:#?}");
        Ok(())
    }

    /// The Ragdoll on the real Blockhead wearing every hat, accent, pack
    /// and second pack: once the ragdoll has fallen, every vertex drawn
    /// lies on or near one of its boxes. A hat or cape the rig hangs
    /// beside the body's parts used to stay where the death animation left
    /// it. Run on a PC with content:
    /// `cargo test -p bri-client --lib ragdoll_keeps_accessories_on -- --ignored --nocapture`
    fn ragdoll_keeps_accessories_on(fx: &Avatar) -> Result<()> {
        use bri_client_sandbox::bodies::{PhysicsCommand, Shape};
        use bri_client_sandbox::{AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, World};
        /// How far a drawn vertex may move in the frame of the ragdoll box
        /// it rides with, between the ragdoll being made (standing) and
        /// lying settled: rounding only, as parts ride their boxes rigidly.
        /// Distance from the boxes is no test: a pointy helmet's tip sits
        /// well past the head box standing up, and rightly stays there.
        const DRIFT: f32 = 0.01;
        let assets = &fx.assets;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/showcase/ragdoll");
        let code = AddOnCode::load(&dir)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?
            .context("the Ragdoll has client code")?;
        let floor = crate::brick_debris::tests::building(&[]).0;
        let package = &assets.package;
        // Every choice of every slot turns up in some outfit: hats, packs,
        // skirts, hooks. The accent follows the hat.
        let slots: Vec<(&String, Vec<&String>)> = package
            .parts
            .iter()
            .filter(|(slot, _)| slot.as_str() != "accent")
            .map(|(slot, list)| {
                let some = list.iter().filter(|c| !c.eq_ignore_ascii_case("none"));
                (slot, some.collect())
            })
            .filter(|(_, list): &(_, Vec<_>)| !list.is_empty())
            .collect();
        let outfits = slots.iter().map(|(_, list)| list.len()).max().unwrap_or(0);
        ensure!(outfits > 0, "the pack has no parts");
        // The worst vertex of one outfit: how far it moved in the frame of
        // the ragdoll box it stayed nearest to in place, and where it is.
        let measure = |n: usize| -> Result<(String, f32, Vec3, f32)> {
            let mut appearance = package.defaults.clone();
            for (slot, list) in &slots {
                appearance
                    .parts
                    .insert((*slot).clone(), list[n % list.len()].clone());
            }
            let hat = appearance.parts["hat"].clone();
            if let Some(accent) = package
                .accents_allowed
                .get(&hat)
                .and_then(|a| a.iter().find(|a| !a.eq_ignore_ascii_case("none")))
            {
                appearance.parts.insert("accent".into(), accent.clone());
            }
            let outfit = appearance
                .parts
                .iter()
                .map(|(slot, part)| format!("{slot} {part}"))
                .collect::<Vec<_>>()
                .join(", ");
            let mut mesh = assets.mesh(appearance)?;
            mesh.defer_mesh = true;
            let mut p = player();
            p.feet = [3.0, 0.0, -2.0];
            p.yaw = 0.7;
            // The corpse the game animates stands where it died; the
            // ragdoll lies down.
            mesh.pose(assets, &p, 0.0)?;
            let world = Arc::new(World {
                local: 1,
                players: vec![bri_client_sandbox::world::Player {
                    id: 1,
                    alive: false,
                    feet: p.feet,
                    ..Default::default()
                }],
                skeletons: [(1, mesh.skeleton(assets))].into(),
                ..Default::default()
            });
            let standing_pose = mesh.pending.take().context("a deferred pose")?;
            mesh.build_mesh(assets, &standing_pose)?;
            let standing: Vec<Vec3> = mesh
                .data
                .vertices
                .iter()
                .map(|v| Vec3::from(v.position))
                .collect();
            let mut made = std::collections::BTreeMap::new();
            let mut addon = Sandbox::new()?
                .start_in(&code, Budgets::untimed(), TrustLevel::Sandboxed, 0)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut physics = crate::addon_physics::AddOnPhysics::default();
            let mut boxes = std::collections::BTreeMap::new();
            let mut nodes = Vec::new();
            let mut read = physics.snapshot();
            for _ in 0..240 {
                read = physics.snapshot();
                let out = addon
                    .frame(FrameInput {
                        dt: 1.0 / 60.0,
                        world: world.clone(),
                        bodies: read.clone(),
                        ..Default::default()
                    })
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .clone();
                for command in &out.physics {
                    if let PhysicsCommand::Create { body, spec } = command
                        && let Shape::Box(half) = spec.shape
                    {
                        boxes.insert(*body, (Vec3::from(spec.offset), Vec3::from(half)));
                        let rotation = Quat::from_array(spec.rotation);
                        made.insert(
                            *body,
                            (
                                Vec3::from(spec.position) + rotation * Vec3::from(spec.offset),
                                rotation,
                                Vec3::from(half),
                            ),
                        );
                    }
                }
                if let Some(pose) = out.poses.first() {
                    nodes.clone_from(&pose.nodes);
                }
                physics.apply(&out.physics);
                physics.advance(1.0 / 60.0, &floor, &[], &[])?;
            }
            // The bodies the last pose was made from, fallen and settled
            // (4 s; the Blockhead settles in under that).
            let lying: Vec<_> = read
                .iter()
                .filter_map(|(id, body)| {
                    let (offset, half) = boxes.get(id)?;
                    let rotation = Quat::from_array(body.rotation);
                    Some((
                        (
                            Vec3::from(body.position) + rotation * *offset,
                            rotation,
                            *half,
                        ),
                        *made.get(id)?,
                    ))
                })
                .collect();
            ensure!(!nodes.is_empty(), "{outfit}: the ragdoll posed nothing");
            ensure!(lying.len() == made.len(), "{outfit}: bodies went missing");
            // How far the boxes fell from where they were made.
            let fell = lying
                .iter()
                .map(|(now, then)| now.0.distance(then.0))
                .fold(0.0f32, f32::max);
            mesh.pose(assets, &p, 0.0)?;
            mesh.override_nodes(assets, &nodes);
            let pose = mesh.pending.take().context("a deferred pose")?;
            mesh.build_mesh(assets, &pose)?;
            ensure!(
                mesh.data.vertices.len() == standing.len(),
                "{outfit}: the mesh changed"
            );
            // A vertex's place in a box's own frame, standing and lying.
            let local = |(centre, rotation, _): &(Vec3, Quat, Vec3), v: Vec3| {
                rotation.inverse() * (v - *centre)
            };
            let (mut drift, mut at) = (-1.0f32, Vec3::ZERO);
            for (v, rest) in mesh.data.vertices.iter().zip(&standing) {
                let v = Vec3::from(v.position);
                let least = lying
                    .iter()
                    .map(|(now, then)| local(now, v).distance(local(then, *rest)))
                    .fold(f32::INFINITY, f32::min);
                if least > drift {
                    (drift, at) = (least, v);
                }
            }
            Ok((outfit, drift, at, fell))
        };
        let mut problems = Vec::new();
        for n in 0..outfits {
            let (outfit, drift, at, fell) = measure(n)?;
            println!(
                "{outfit}: boxes fell up to {fell:.2}; worst vertex moved {drift:.3} \
                 in its box's frame (at {:.2?})",
                at.to_array()
            );
            if fell < 0.5 {
                problems.push(format!("{outfit}: the ragdoll never fell ({fell:.2})"));
            }
            if drift > DRIFT {
                problems.push(format!(
                    "{outfit}: a vertex moved {drift:.3} in the frame of every box, at {at}"
                ));
            }
        }
        // The check must catch accessories left where the corpse died.
        FOLLOW_ANCHORS.set(false);
        let unanchored: Result<Vec<_>> = (0..outfits).map(&measure).collect();
        FOLLOW_ANCHORS.set(true);
        let caught = unanchored?
            .iter()
            .map(|(_, drift, _, _)| *drift)
            .fold(0.0f32, f32::max);
        println!("with anchors off, the worst vertex moved {caught:.2}");
        if caught < 0.5 {
            problems.push(format!(
                "with anchors off the worst vertex moved only {caught:.2}: the check proves nothing"
            ));
        }
        ensure!(problems.is_empty(), "{problems:#?}");
        Ok(())
    }
}
