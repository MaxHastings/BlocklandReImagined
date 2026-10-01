//! Native avatar resources, outfit binding and live player pose rendering.
use crate::crouch::CrouchThread;
use anyhow::{Context, Result, ensure};
use bri_content::{
    animation::{Channels, Layer, sample_layers_with_transition},
    avatar::{Appearance, Outfit, Package, Rig},
};
use bri_render::scene::{
    AlphaMode, GpuInstances, GpuScene, Material, SceneData, SceneImage, SceneRenderer,
};
use bri_sim::player::PlayerState;
use bri_ui::api::AvatarPrefs;
use glam::{Mat4, Quat, Vec3};
use sha2::{Digest, Sha256};
use bri_client_sandbox::world::{Bounds, Rig as NodeTree, Skeleton};
use std::{collections::BTreeMap, path::Path, sync::Arc};

pub struct AvatarAssets {
    pub package: Package,
    pub rig: Rig,
    images: BTreeMap<String, SceneImage>,
    detail: usize,
    /// `HorseArmor`'s horse.dts and sequences, for players of that datablock.
    horse: Option<Box<AvatarAssets>>,
    /// Each shape object's name in lower case, as outfits name them.
    object_names: Vec<String>,
    /// Node index by lower-case name (the first node of a name), and each
    /// `Mount<n>` node's, so a lookup is not a scan of every node name.
    node_index: std::collections::HashMap<String, usize>,
    mount_nodes: [Option<usize>; 32],
    /// The node tree Add-Ons with `avatar.pose` see.
    tree: Arc<NodeTree>,
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
fn node_indices(rig: &Rig) -> (std::collections::HashMap<String, usize>, [Option<usize>; 32]) {
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
            horse: None,
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
        self.horse = Some(Box::new(Self {
            tree: node_tree(&rig),
            object_names: lower_names(&rig),
            node_index,
            mount_nodes,
            package: self.package.clone(),
            rig,
            images,
            detail,
            horse: None,
        }));
        Ok(())
    }
    /// A player of `HorseArmor`: `ApplyBodyColors` paints the body with the
    /// chest colour and the head black; the ski nodes stay hidden.
    pub fn horse_mesh(&self, appearance: Appearance) -> Result<AvatarMesh> {
        let horse = self.horse.as_deref().context("Horse model is not loaded")?;
        let chest = appearance.colors.get("chest").copied().unwrap_or([1.0; 4]);
        let outfit = Outfit {
            nodes: [
                ("body".into(), chest),
                ("head".into(), [0.0, 0.0, 0.0, 1.0]),
            ]
            .into(),
            face: String::new(),
            decal: String::new(),
            head_up: false,
        };
        let mut data = SceneData {
            name: "Horse".into(),
            ..Default::default()
        };
        let mut materials = Vec::new();
        for source in &horse.rig.shape.materials {
            let name = source.name.to_ascii_lowercase();
            let image = data.images.len();
            data.images.push(horse.images[&name].clone());
            materials.push(data.materials.len());
            data.materials
                .push(Material::brick_overlay(format!("horse/{name}"), image));
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
        mesh.horse = true;
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
    fn for_mesh(&self, mesh: &AvatarMesh) -> &AvatarAssets {
        match (&self.horse, mesh.horse) {
            (Some(horse), true) => horse,
            _ => self,
        }
    }
    pub fn from_prefs(&self, prefs: &AvatarPrefs) -> Result<Appearance> {
        let mut appearance = self.package.defaults.clone();
        // A part this pack does not have (removed, renamed, or not a name at
        // all) keeps the pack default rather than failing the whole avatar.
        let package = &self.package;
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
        self.package.resolve(&appearance)?;
        Ok(appearance)
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
            horse: false,
            appearance,
            data,
            gpu: None,
            layout: None,
            restructured: true,
            pending: None,
            defer_mesh: false,
            instanced: false,
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
            crouch: CrouchThread::default(),
            body: None,
            dead: false,
            hidden_nodes: Vec::new(),
            posed_nodes: Vec::new(),
            animated_nodes: Vec::new(),
            node_bounds: None,
            unacted_nodes: Vec::new(),
            model_transform: Mat4::IDENTITY,
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
    /// Drawn with the `HorseArmor` rig instead of the Blockhead.
    pub horse: bool,
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
    crouch: CrouchThread,
    /// The spawn this mesh animates (`Vitals::spawn_tick`).
    body: Option<u64>,
    /// Whether the drawn body lies dead (`drawn_life`), for Add-On code.
    dead: bool,
    /// Body nodes the held images hide (`bri_weapons::Image::hide_nodes`),
    /// lower case; the outfit keeps them for when the image goes.
    hidden_nodes: Vec<String>,
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
    let position = (0.5 - pitch / std::f32::consts::PI).clamp(0.0, 1.0);
    match limits {
        Some([down, up]) if down <= up => position.clamp(down, up),
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
    /// Current thread-2 action from the authoritative animation cue stream.
    /// Clear this on the corresponding vanilla stop/root cue or image switch.
    pub action: Option<ActionAnimation>,
    /// Current thread-3 builder or chat animation (`playThread(3, ...)`):
    /// brick shifts, rotations, plant, undo, activate and talk.
    pub gesture: Option<ActionAnimation>,
    /// Dead bodies hold the original `death1` sequence.
    pub dead: bool,
    /// The `sit` emote holds the original sit sequence until the player moves.
    pub sitting: bool,
    /// The latest simulated tick state. v20 picks the action from the tick's
    /// own rotation and velocity, not from the render-interpolated body or the
    /// live mouse yaw; mixing those breaks the exact tie at 45 degrees.
    pub tick_state: Option<PlayerState>,
    /// The fraction of the body in liquid, for `pickActionAnimation`'s
    /// water rules.
    pub water_coverage: f32,
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
    let root = if player.crouched { "crouch" } else { "root" };
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
    let [root, run, back, side] = if player.crouched {
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
    /// How far Add-On code moved the body from where the game animates it
    /// (a ragdoll sliding away from where its player died): the middle of
    /// the drawn nodes less the middle of the animated ones. `None` while
    /// the game animates it.
    pub fn drawn_offset(&self) -> Option<Vec3> {
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
        let reach = points.iter().map(|p| p.distance(center)).fold(0.0, f32::max);
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
    /// `world_node` of `Mount<n>` (n below 32), without building its name.
    pub fn mount_node(&self, assets: &AvatarAssets, n: usize) -> Option<Mat4> {
        let index = (*assets.for_mesh(self).mount_nodes.get(n)?)?;
        self.posed_nodes
            .get(index)
            .map(|node| self.model_transform * *node)
    }
    /// How the playing thread-2/3 actions (a brick shift, plant, recoil,
    /// swing) move `Mount<n>`, in that mount's own frame: the rest of the
    /// pose (locomotion, armReady, look) is left out. None while no action
    /// moves it. A first-person image rides this on its eye offset.
    pub fn mount_action(&self, assets: &AvatarAssets, n: usize) -> Option<Mat4> {
        let index = (*assets.for_mesh(self).mount_nodes.get(n)?)?;
        let rest = self.unacted_nodes.get(index)?;
        let moved = rest.inverse() * *self.posed_nodes.get(index)?;
        (moved.is_finite() && !moved.abs_diff_eq(Mat4::IDENTITY, 1e-6)).then_some(moved)
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
        let eye_position = self.animated_world_node(assets, "Eye")?.w_axis.truncate();
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
        let assets = assets.for_mesh(self);
        ensure!(
            time.is_finite()
                && [&animation_input.action, &animation_input.gesture]
                    .into_iter()
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
            .map_or(0.0, |last| (time - last).clamp(0.0, 0.25) as f32);
        self.last_time = Some(time);
        let scripted = if animation_input.dead {
            Some("death1")
        } else if animation_input.sitting {
            Some("sit")
        } else {
            None
        };
        // v20 picks every client frame: it kept `delayTicks` but dropped the
        // test that would hold an action for `sNewAnimationTickTime`.
        let next = Some(scripted.map_or_else(
            || {
                locomotion(
                    animation_input.tick_state.as_ref().unwrap_or(player),
                    animation_input.water_coverage,
                )
            },
            |sequence| LocomotionAction {
                sequence,
                forward: true,
            },
        ));
        // `Player::setActionThread` ignores a request for the running action,
        // even in the other play direction.
        let next = next.filter(|action| action.sequence != self.mode);
        let clip = assets
            .rig
            .sequence(next.map_or(self.mode, |action| action.sequence))
            .context("Missing avatar movement clip")?;
        if let Some(action) = next {
            self.transition = self
                .channels
                .take()
                .filter(|_| !first)
                .map(|channels| (channels, time));
            self.mode = action.sequence;
            self.forward = action.forward;
            self.phase = if action.forward { 0.0 } else { clip.duration };
        } else if self.forward {
            self.phase += elapsed;
        } else {
            self.phase -= elapsed;
        }
        let mode = self.mode;
        if clip.looping && clip.duration > 0.0 {
            self.phase = self.phase.rem_euclid(clip.duration);
        } else {
            self.phase = self.phase.clamp(0.0, clip.duration);
        }
        let mut layers = Vec::new();
        layers.push(Layer {
            animation: clip,
            time: self.phase,
            weight: 1.0,
        });
        // Native sequence priorities put armReady (14) over locomotion (12).
        // It is absolute, so it must precede the additive headup/look overlays.
        let ready_name = match animation_input.held_tool_pose {
            HeldToolPose::None => None,
            HeldToolPose::Right => Some("armreadyright"),
            HeldToolPose::Left => Some("armreadyleft"),
            HeldToolPose::Both => Some("armreadyboth"),
        };
        let ready_clip = ready_name
            .map(|name| {
                assets
                    .rig
                    .sequence(name)
                    .with_context(|| format!("Missing held-arm clip {name}"))
            })
            .transpose()?;
        if let Some(clip) = &ready_clip {
            ensure!(!clip.additive, "Held-arm clip must be an absolute sequence");
            layers.push(Layer {
                animation: clip,
                time: clip.duration,
                weight: 1.0,
            });
        }
        let crouch = assets
            .rig
            .sequence("crouch")
            .context("Missing crouch clip")?;
        self.crouch
            .update(player.crouched, elapsed, crouch.duration);
        if let Some(time) = self.crouch.time() {
            layers.push(Layer {
                animation: crouch,
                time,
                weight: 1.0,
            });
        }
        // Absolute clips establish the base pose before additive deltas. The
        // original jump clip is additive even though other locomotion clips
        // are absolute; priority alone would put it before armReady/crouch and
        // make the sampler reject an ordinary jump while holding a tool.
        layers.sort_by_key(|layer| (layer.animation.additive, layer.animation.priority));
        if self.outfit.head_up {
            let clip = assets
                .rig
                .sequence("headup")
                .context("Missing pack head pose")?;
            layers.push(Layer {
                animation: clip,
                time: clip.duration,
                weight: 1.0,
            });
        }
        let look = assets.rig.sequence("look").context("Missing look clip")?;
        layers.push(Layer {
            animation: look,
            time: look_position(player.pitch, animation_input.look_limits) * look.duration,
            weight: 1.0,
        });
        // `Player::updateLookAnimation`: free look turns only the head,
        // mapped over the datablock's +/-maxLookAngle.
        let headside = assets
            .rig
            .sequence("headside")
            .context("Missing headside clip")?;
        layers.push(Layer {
            animation: headside,
            time: (0.5 + player.head_yaw / std::f32::consts::PI).clamp(0.0, 1.0)
                * headside.duration,
            weight: 1.0,
        });
        let unacted = layers.len();
        let threads = [(2, &animation_input.action), (3, &animation_input.gesture)];
        for (thread, action) in threads {
            let Some(action) = action else {
                continue;
            };
            let name = action.sequence.to_ascii_lowercase();
            if name != "root" {
                let clip = assets
                    .rig
                    .sequence(&name)
                    .with_context(|| format!("Missing avatar action clip {}", action.sequence))?;
                ensure!(
                    clip.additive,
                    "Thread-{thread} avatar action clip {} must be additive",
                    action.sequence
                );
                let action_time = (time - action.started_at).max(0.0) as f32;
                layers.push(Layer {
                    animation: clip,
                    time: action_time,
                    weight: 1.0,
                });
            }
        }
        // `transitionToSequence` blends the locomotion thread from the pose it
        // had when the action changed. Its channels end right after the
        // locomotion clip, or where the absolute layers end for additive jumps.
        let at = if clip.additive {
            layers
                .iter()
                .position(|layer| layer.animation.additive)
                .unwrap_or(layers.len())
        } else {
            layers
                .iter()
                .position(|layer| std::ptr::eq(layer.animation, clip))
                .context("Missing avatar movement layer")?
                + 1
        };
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
            let progress = ((time - start) / transition_time).clamp(0.0, 1.0);
            (channels, 1.0 - progress as f32)
        });
        let (pose, channels) = sample_layers_with_transition(&assets.rig.shape, &layers, at, from)?;
        self.unacted_nodes.clear();
        if layers.len() > unacted {
            let (rest, _) =
                sample_layers_with_transition(&assets.rig.shape, &layers[..unacted], at, from)?;
            self.unacted_nodes = rest.nodes;
        }
        self.channels = Some(channels);
        self.finish_pose(assets, player, pose, animation_input.mount_rotation)
    }

    fn finish_pose(
        &mut self,
        assets: &AvatarAssets,
        player: &PlayerState,
        pose: bri_content::animation::Pose,
        mount_rotation: Option<Quat>,
    ) -> Result<()> {
        // `setScale` scales the whole shape about the feet.
        let model_transform = Mat4::from_scale_rotation_translation(
            Vec3::splat(player.scale),
            mount_rotation
                .filter(|q| q.is_finite() && q.is_normalized())
                .unwrap_or_else(|| Quat::from_rotation_y(-player.yaw)),
            Vec3::from(player.feet),
        );
        self.model_transform = model_transform;
        self.posed_nodes.clone_from(&pose.nodes);
        self.animated_nodes.clear();
        if self.defer_mesh {
            self.pending = Some(pose);
            Ok(())
        } else {
            self.build_mesh(assets, &pose)
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
            detail: assets.detail,
            materials: &self.materials,
            translucent_materials: &self.translucent_materials,
            unassigned_material: self.materials[0],
            colors: &colors,
        };
        if self.instanced
            && !self.restructured
            && self.drawn_pose.as_ref().is_some_and(|drawn| same_pose(drawn, pose))
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
            self.crouch = CrouchThread::default();
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
    /// Send the vertices to the GPU: only positions and normals while the
    /// structure holds.
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        if std::mem::take(&mut self.restructured) {
            self.gpu = None;
        }
        if let Some(gpu) = &mut self.gpu {
            if std::mem::take(&mut self.vertices_dirty) {
                let centers: Vec<_> = self.data.batches.iter().map(|b| b.center).collect();
                gpu.update_vertices(queue, &self.data.vertices, &centers)?;
            }
        } else {
            self.gpu = Some(renderer.upload(device, queue, &self.data)?);
            self.vertices_dirty = false;
        }
        if self.instanced {
            let instance = match &mut self.instance {
                Some(instance) => instance,
                None => self.instance.insert(GpuInstances::new(device, 1)?),
            };
            instance.update(
                queue,
                &[bri_render::scene::SceneTransform {
                    transform: self.model_transform,
                    tint: [1.0; 4],
                }],
            )?;
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
    pub fn render(
        &mut self,
        assets: &AvatarAssets,
        appearance: &Appearance,
        rotation: [f32; 3],
        distance: f32,
        frame: &mut crate::platform::RenderContext<'_>,
    ) -> Result<()> {
        ensure!(
            rotation.iter().all(|v| v.is_finite())
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
        mesh.pose(
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
                tick: Default::default(),
            },
            0.0,
        )?;
        mesh.upload(&self.renderer, frame.device, frame.queue)?;
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
        // Authored Avatar_Preview light fields, changed from Z-up to Y-up.
        camera.sun_direction = [0.721277, 0.57735, -0.57735, 0.0];
        camera.sun_color = [1.0, 1.0, 1.0, 0.0];
        camera.ambient = [0.5, 0.5, 0.5, 0.0];
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

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        /// Off, placed nodes' accessories keep their animated pose (as
        /// before `follow_anchors`), for checks that must catch that.
        pub(super) static FOLLOW_ANCHORS: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
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
            tick: Default::default(),
        }
    }
    fn vitals(alive: bool, spawn_tick: u64, died_tick: Option<u64>) -> bri_sim::session::Vitals {
        bri_sim::session::Vitals {
            health: if alive { 100.0 } else { 0.0 },
            alive,
            respawn_tick: 0,
            spawn_tick,
            died_tick,
            score: 0,
            minigame: None,
            invite: None,
            light: false,
            mounted: None,
            ride: None,
            control: Default::default(),
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

    #[test]
    #[ignore = "requires original native avatar package"]
    fn reposed_vertices_match_a_full_shape_rebuild() -> Result<()> {
        use bri_render::shape_scene::ShapeInstance;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
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
                mesh.outfit.nodes.insert("chest".into(), [1.0, 0.0, 0.0, 0.5]);
            }
            // A held image that draws its own hands hides the Blockhead's.
            let hands = (34..38).contains(&frame).then(|| ["LHand".to_string(), "rhand".to_string()]);
            mesh.set_hidden_nodes(hands.into_iter().flatten());
            mesh.pose(&assets, &p, f64::from(frame) / 30.0)?;
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
                    mesh.outfit.nodes.get(&name).copied().filter(|_| !mesh.hidden_nodes.contains(&name))
                },
            )?;
            mesh.restructured = false;
            mesh.build_mesh(&assets, &pose)?;
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

    #[test]
    #[ignore = "requires original native avatar package"]
    fn a_respawned_body_stands_in_root_without_getting_up_from_the_corpse() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
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
        fresh.pose_with_animation(&assets, &p, 10.0, &alive)?;
        let mut body = assets.mesh(assets.package.defaults.clone())?;
        assert!(!body.set_body(1));
        for frame in 0..60 {
            body.pose_with_animation(&assets, &p, f64::from(frame) / 30.0, &dead)?;
        }
        let lying = apart(&body, &fresh);
        assert!(lying > 0.01, "death1 moves the body ({lying})");
        // Seeing the same body again changes nothing: it stays dead.
        assert!(!body.set_body(1));
        // A respawn is a new body: its first pose is a fresh body's.
        assert!(body.set_body(2));
        body.pose_with_animation(&assets, &p, 2.1, &alive)?;
        assert_eq!(apart(&body, &fresh), 0.0);
        Ok(())
    }

    #[test]
    #[ignore = "requires original native avatar package"]
    fn item_mounts_use_the_same_sampled_avatar_pose_and_body_transform() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        assert!(mesh.world_node(&assets, "mount0").is_none());
        let mut p = player();
        mesh.pose(&assets, &p, 0.)?;
        let before = mesh
            .world_node(&assets, "mount0")
            .context("Original avatar mount0")?;
        assert!(mesh.world_node(&assets, "inventedHand").is_none());
        p.feet = [12., 3., -7.];
        p.yaw = 0.75;
        mesh.pose(&assets, &p, 0.)?;
        let expected =
            Mat4::from_rotation_translation(Quat::from_rotation_y(-p.yaw), Vec3::from(p.feet))
                * before;
        assert!(
            mesh.world_node(&assets, "MOUNT0")
                .unwrap()
                .abs_diff_eq(expected, 0.00001)
        );
        let hand0 = mesh.world_node(&assets, "mount0").unwrap();
        let hand1 = mesh
            .world_node(&assets, "mount1")
            .context("Original avatar mount1")?;
        assert!(!hand0.abs_diff_eq(hand1, 0.00001));
        Ok(())
    }

    #[test]
    #[ignore = "requires original native avatar package"]
    fn held_and_action_arm_layers_keep_locomotion_and_mounts_coherent() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
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
        baseline.pose(&assets, &p, 0.25)?;
        equipped.pose_with_animation(
            &assets,
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
                .world_node(&assets, "RightLeg")
                .context("Baseline right leg")?
                .abs_diff_eq(
                    equipped
                        .world_node(&assets, "RightLeg")
                        .context("Equipped right leg")?,
                    1e-5,
                )
        );
        assert_ne!(
            baseline.world_node(&assets, "RightArm"),
            equipped.world_node(&assets, "RightArm")
        );
        assert_eq!(
            equipped.world_node(&assets, "mount0"),
            equipped.world_node(&assets, "MOUNT0")
        );
        assert!(
            equipped
                .pose_with_animation(
                    &assets,
                    &p,
                    0.5,
                    &AvatarAnimationInput {
                        held_tool_pose: HeldToolPose::None,
                        action: Some(ActionAnimation {
                            sequence: "missing-original-clip".into(),
                            started_at: 0.0
                        }),
                        ..Default::default()
                    },
                )
                .is_err()
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires original native avatar package"]
    fn held_arm_pose_precedes_additive_look_and_pack_headup_layers() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
        let mut appearance = assets.package.defaults.clone();
        appearance
            .parts
            .insert("pack".into(), assets.package.parts["pack"][1].clone());
        let mut baseline = assets.mesh(appearance.clone())?;
        let mut held = assets.mesh(appearance)?;
        let mut p = player();
        p.yaw = 0.4;
        p.pitch = 0.65;
        baseline.pose(&assets, &p, 0.0)?;
        held.pose_with_animation(
            &assets,
            &p,
            0.0,
            &AvatarAnimationInput {
                held_tool_pose: HeldToolPose::Right,
                action: None,
                ..Default::default()
            },
        )?;
        assert_ne!(
            baseline.world_node(&assets, "RightArm"),
            held.world_node(&assets, "RightArm")
        );
        assert_eq!(
            baseline.world_node(&assets, "Head"),
            held.world_node(&assets, "Head")
        );
        assert!(held.world_node(&assets, "Mount0").is_some());
        let raw_eye = held
            .world_node(&assets, "Eye")
            .context("Original Eye node")?;
        let eye = held
            .eye_transform(&assets, p.yaw, p.pitch)
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

    #[test]
    #[ignore = "requires original native avatar package"]
    fn rebuilt_outfit_continues_the_running_clip() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
        let mut kept = assets.mesh(assets.package.defaults.clone())?;
        let mut p = player();
        p.velocity = [0.0, 0.0, -7.0];
        for frame in 0..20 {
            kept.pose(&assets, &p, f64::from(frame) / 60.0)?;
        }
        let mut rebuilt = assets.mesh(assets.package.defaults.clone())?;
        rebuilt.continue_animation(&kept);
        let time = 20.0 / 60.0;
        kept.pose(&assets, &p, time)?;
        rebuilt.pose(&assets, &p, time)?;
        let leg = |mesh: &AvatarMesh| mesh.world_node(&assets, "RightLeg").unwrap();
        assert!(leg(&kept).abs_diff_eq(leg(&rebuilt), 1e-5));
        let mut fresh = assets.mesh(assets.package.defaults.clone())?;
        fresh.pose(&assets, &p, time)?;
        assert!(!leg(&kept).abs_diff_eq(leg(&fresh), 1e-3));
        Ok(())
    }

    #[test]
    #[ignore = "requires the converted avatar pack"]
    fn mount_action_is_only_the_playing_actions_motion() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
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
        mesh.pose_with_animation(&assets, &p, 0.1, &input(None))?;
        assert_eq!(mesh.mount_action(&assets, 0), None);
        let rest = mesh.model_node(&assets, "Mount0").context("Mount0")?;
        let mut shifted = assets.mesh(assets.package.defaults.clone())?;
        shifted.pose_with_animation(&assets, &p, 0.1, &input(Some("shiftAway")))?;
        let action = shifted
            .mount_action(&assets, 0)
            .context("shiftAway moves the right hand")?;
        let moved = shifted.model_node(&assets, "Mount0").context("Mount0")?;
        assert!(!moved.abs_diff_eq(rest, 1e-3));
        // The same walk, armReady and look, plus exactly the action.
        assert!((rest * action).abs_diff_eq(moved, 1e-4));
        Ok(())
    }

    #[test]
    #[ignore = "requires the converted avatar pack"]
    fn seated_body_takes_the_mount_rotation() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let tilt = Quat::from_rotation_y(0.6) * Quat::from_rotation_x(0.35);
        mesh.pose_with_animation(
            &assets,
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
    #[test]
    #[ignore = "requires original native avatar package"]
    fn free_look_turns_only_the_head_toward_the_camera() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        let mut p = player();
        mesh.pose(&assets, &p, 0.0)?;
        let head = mesh.world_node(&assets, "Head").context("Head node")?;
        let torso = mesh.world_node(&assets, "Torso").context("Torso node")?;
        // Positive yaw turns right (+X from the -Z forward).
        p.head_yaw = 1.0;
        mesh.pose(&assets, &p, 1.0)?;
        let turned = mesh.world_node(&assets, "Head").context("Head node")?;
        let (_, rest, _) = head.to_scale_rotation_translation();
        let (_, now, _) = turned.to_scale_rotation_translation();
        let look = (now * rest.inverse()) * -Vec3::Z;
        assert!(look.x > 0.5, "head turns toward the camera: {look}");
        assert!(look.y.abs() < 0.1, "free look turns, not tilts: {look}");
        assert_eq!(mesh.world_node(&assets, "Torso"), Some(torso));
        Ok(())
    }

    #[test]
    #[ignore = "requires original native avatar package"]
    fn original_outfits_materials_and_customization_rules() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-002");
        let assets = AvatarAssets::load(&root)?;
        let package = &assets.package;
        assert_eq!(
            (
                package.faces.len(),
                package.decals.len(),
                package.textures.len()
            ),
            (27, 28, 63)
        );
        let mut cases = 0;
        for (slot, options) in &package.parts {
            if slot == "accent" {
                continue;
            }
            for option in options {
                let mut appearance = package.defaults.clone();
                appearance.parts.insert(slot.clone(), option.clone());
                let mut mesh = assets.mesh(appearance)?;
                mesh.pose(&assets, &player(), 0.0)?;
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
                mesh.pose(&assets, &player(), 0.0)?;
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
        assert_eq!(outfit.nodes["visor"][3], 0.7);
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
        let output = root
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("artifacts/native-avatar/outfits.json");
        std::fs::create_dir_all(output.parent().unwrap())?;
        std::fs::write(
            output,
            serde_json::to_vec_pretty(
                &serde_json::json!({"bound_cases":cases,"skirt_leg_rules":true,"hat_accent_rules":true,"hidden_choice_validated":true,"pack_head_pose":true,"original_textures_verified":63}),
            )?,
        )?;
        Ok(())
    }

    /// The Ragdoll Add-On (`packages/showcase/ragdoll`) on the real
    /// Blockhead: each part's box sits on the geometry drawn for that part,
    /// posing the body from the fresh ragdoll draws it where it stood, and
    /// the ragdoll falls to a floor in one piece. Prints every box. Run on
    /// a PC with content:
    /// `cargo test -p bri-client --lib ragdoll_on_the_real_blockhead -- --ignored --nocapture`
    /// (`BRI_CONTENT` names another content folder).
    #[test]
    #[ignore = "requires original native avatar package"]
    fn ragdoll_on_the_real_blockhead() -> Result<()> {
        use bri_client_sandbox::bodies::{PhysicsCommand, Shape};
        use bri_client_sandbox::{AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, World};
        const PARTS: [&str; 10] = [
            "chest", "femchest", "pants", "headskin", "rarm", "larm", "rhand", "lhand", "rshoe",
            "lshoe",
        ];
        let content = std::env::var_os("BRI_CONTENT").map_or_else(
            || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
            std::path::PathBuf::from,
        );
        let assets = AvatarAssets::load(&content.join("avatar-pack-002"))?;
        let mut mesh = assets.mesh(assets.package.defaults.clone())?;
        mesh.defer_mesh = true;
        let mut p = player();
        p.feet = [3.0, 0.0, -2.0];
        p.yaw = 0.7;
        mesh.pose(&assets, &p, 0.0)?;
        let skeleton = mesh.skeleton(&assets);
        let pose = mesh.pending.take().context("a deferred pose")?;
        mesh.build_mesh(&assets, &pose)?;
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
        mesh.pose(&assets, &p, 0.0)?;
        mesh.override_nodes(&assets, nodes);
        let pose = mesh.pending.take().context("a deferred pose")?;
        mesh.build_mesh(&assets, &pose)?;
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
    #[test]
    #[ignore = "requires original native avatar package"]
    fn ragdoll_keeps_accessories_on() -> Result<()> {
        use bri_client_sandbox::bodies::{PhysicsCommand, Shape};
        use bri_client_sandbox::{AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, World};
        /// How far a drawn vertex may move in the frame of the ragdoll box
        /// it rides with, between the ragdoll being made (standing) and
        /// lying settled: rounding only, as parts ride their boxes rigidly.
        /// Distance from the boxes is no test: a pointy helmet's tip sits
        /// well past the head box standing up, and rightly stays there.
        const DRIFT: f32 = 0.01;
        let content = std::env::var_os("BRI_CONTENT").map_or_else(
            || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
            std::path::PathBuf::from,
        );
        let assets = AvatarAssets::load(&content.join("avatar-pack-002"))?;
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
            mesh.pose(&assets, &p, 0.0)?;
            let world = Arc::new(World {
                local: 1,
                players: vec![bri_client_sandbox::world::Player {
                    id: 1,
                    alive: false,
                    feet: p.feet,
                    ..Default::default()
                }],
                skeletons: [(1, mesh.skeleton(&assets))].into(),
                ..Default::default()
            });
            let standing_pose = mesh.pending.take().context("a deferred pose")?;
            mesh.build_mesh(&assets, &standing_pose)?;
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
            mesh.pose(&assets, &p, 0.0)?;
            mesh.override_nodes(&assets, &nodes);
            let pose = mesh.pending.take().context("a deferred pose")?;
            mesh.build_mesh(&assets, &pose)?;
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
