//! Native avatar resources, outfit binding and live player pose rendering.
use crate::crouch::CrouchThread;
use anyhow::{Context, Result, ensure};
use bri_content::{
    animation::{Channels, Layer, sample_layers_with_transition},
    avatar::{Appearance, Outfit, Package, Rig},
};
use bri_render::{
    scene::{AlphaMode, GpuScene, Material, SceneData, SceneImage, SceneRenderer},
    shape_scene::ShapeInstance,
};
use bri_sim::player::PlayerState;
use bri_ui::api::AvatarPrefs;
use glam::{Mat4, Quat, Vec3};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

pub struct AvatarAssets {
    pub package: Package,
    pub rig: Rig,
    images: BTreeMap<String, SceneImage>,
    detail: usize,
    /// `HorseArmor`'s horse.dts and sequences, for players of that datablock.
    horse: Option<Box<AvatarAssets>>,
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
        Ok(Self {
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
            let bytes = crate::items::checked_read(&root, &texture.path, &texture.sha256, 16 << 20)?;
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
        self.horse = Some(Box::new(Self {
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
        let chest = appearance
            .colors
            .get("chest")
            .copied()
            .unwrap_or([1.0; 4]);
        let outfit = Outfit {
            nodes: [("body".into(), chest), ("head".into(), [0.0, 0.0, 0.0, 1.0])].into(),
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
            name == "none" || lists.iter().any(|l| l.iter().any(|n| n.eq_ignore_ascii_case(name)))
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
            uploaded_topology: Default::default(),
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
            posed_nodes: Vec::new(),
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
    model_transform: Mat4,
    pub appearance: Appearance,
    pub data: SceneData,
    pub gpu: Option<GpuScene>,
    /// Indices and batch layout last uploaded to `gpu`; a frame whose posed
    /// topology differs (visibility or detail changes) needs a full upload.
    uploaded_topology: (Vec<u32>, Vec<(std::ops::Range<u32>, usize)>),
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
    pub fn world_node(&self, assets: &AvatarAssets, name: &str) -> Option<Mat4> {
        let assets = assets.for_mesh(self);
        let index = assets
            .rig
            .shape
            .nodes
            .iter()
            .position(|n| n.name.eq_ignore_ascii_case(name))?;
        self.posed_nodes
            .get(index)
            .map(|node| self.model_transform * *node)
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
        let eye_position = self.world_node(assets, "Eye")?.w_axis.truncate();
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
        self.data.vertices.clear();
        self.data.indices.clear();
        self.data.batches.clear();
        self.data.append_shape(
            ShapeInstance {
                shape: &assets.rig.shape,
                pose: &pose,
                detail: assets.detail,
                transform: model_transform,
                materials: &self.materials,
                translucent_materials: Some(&self.translucent_materials),
                unassigned_material: self.materials[0],
            },
            |name| self.outfit.nodes.get(&name.to_ascii_lowercase()).copied(),
        )?;
        self.model_transform = model_transform;
        self.posed_nodes.clone_from(&pose.nodes);
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
    }
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        let same_topology = self.gpu.as_ref().is_some_and(|gpu| {
            gpu.vertex_count == self.data.vertices.len()
                && self.uploaded_topology.0 == self.data.indices
                && self.uploaded_topology.1.len() == self.data.batches.len()
                && self
                    .uploaded_topology
                    .1
                    .iter()
                    .zip(&self.data.batches)
                    .all(|((range, material), b)| *range == b.indices && *material == b.material)
        });
        if !same_topology {
            self.gpu = None;
        }
        if let Some(gpu) = &mut self.gpu {
            gpu.update_vertices(
                queue,
                &self.data.vertices,
                &self
                    .data
                    .batches
                    .iter()
                    .map(|b| b.center)
                    .collect::<Vec<_>>(),
            )?;
        } else {
            self.gpu = Some(renderer.upload(device, queue, &self.data)?);
            self.uploaded_topology = (
                self.data.indices.clone(),
                self.data
                    .batches
                    .iter()
                    .map(|b| (b.indices.clone(), b.material))
                    .collect(),
            );
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
        }
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
    fn item_mounts_use_the_same_sampled_avatar_pose_and_body_transform() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-001");
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-001");
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-001");
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-001");
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
    fn seated_body_takes_the_mount_rotation() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-001");
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
        assert!((super::look_position(-1.2, None) - (0.5 + 1.2 / std::f32::consts::PI)).abs() < 1e-6);
    }
    #[test]
    #[ignore = "requires original native avatar package"]
    fn free_look_turns_only_the_head_toward_the_camera() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-001");
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/avatar-pack-001");
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
        hat.parts.insert("hat".into(), package.parts["hat"][1].clone());
        hat.parts.insert("accent".into(), "visor".into());
        let outfit = package.resolve(&hat)?;
        assert!(outfit.nodes.contains_key("visor"));
        assert_eq!(outfit.nodes["visor"][3], 0.7);
        hat.parts.insert("hat".into(), package.parts["hat"][2].clone());
        assert!(package.resolve(&hat).is_err());
        let mut pack = package.defaults.clone();
        pack.parts.insert("pack".into(), package.parts["pack"][1].clone());
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
}
