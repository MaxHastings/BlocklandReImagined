//! Shared native item geometry projected from authoritative item/weapon state.
//! No source script fields, physics authority, window or gameplay input.
use crate::items::{ItemAssets, ItemMesh};
use anyhow::{Context, Result, ensure};
use bri_render::scene::{
    GpuInstances, GpuScene, MeshBatch, SceneRenderer, SceneTransform, SceneVertex,
};
use bri_sim::{
    presentation::{Cue, CueKind},
    session::WeaponView,
};
use glam::{Mat3, Mat4, Quat, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug)]
pub struct WorldItemLimits {
    pub instances: usize,
    pub models: usize,
    pub geometry_slots: usize,
    pub vertices: usize,
}
impl Default for WorldItemLimits {
    fn default() -> Self {
        Self {
            instances: 8192,
            models: 128,
            geometry_slots: 512,
            vertices: 4_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct WorldItemFrame {
    pub tick: u64,
    /// Monotonic cosmetic clock, reset together with this adapter on session change.
    pub seconds: f64,
    pub eye: Vec3,
    pub local_owner: Option<u64>,
    pub first_person: bool,
    /// Mirrors may show the local player: in first person their held
    /// images also pose as others see them, drawn only in reflections.
    pub reflected_self: bool,
}
/// World-space poses from the SAME sampled avatar used by its visible geometry.
/// Mounts already include player scale and arm/look animation. No guessed nodes.
#[derive(Clone, Debug)]
pub struct MountPose {
    pub eye: Mat4,
    pub mounts: BTreeMap<u32, Mat4>,
    /// How the playing arm actions move each mount, in its own frame
    /// (`AvatarMesh::mount_action`); the holder's first-person image rides
    /// the same motion. Mounts no action moves are absent.
    pub actions: BTreeMap<u32, Mat4>,
    pub velocity: Vec3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ItemIdentity {
    Static(u64),
    Drop(u64),
    Projectile(u64),
    Mounted(u64, u8),
    /// The local player's image as a mirror shows it, in first person.
    Reflected(u64, u8),
}
#[derive(Clone, Debug, Default)]
pub struct WorldItemDiagnostics {
    pub visible_instances: usize,
    pub model_less: usize,
    /// Brick items drawn as respawn ghosts this frame.
    pub cooling_down: usize,
    pub deferred: usize,
    pub missing_bindings: usize,
    pub missing_poses: usize,
    pub missing_sequences: usize,
    pub cached_models: usize,
    pub geometry_slots: usize,
    pub geometry_vertices: usize,
    pub model_builds: u64,
    pub pose_samples: u64,
    pub model_uploads: u64,
    pub shared_geometry_uploads: u64,
    pub vertex_updates: u64,
    pub instance_updates: u64,
    pub messages: BTreeSet<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ModelKey {
    model: String,
    tint: [u32; 4],
    /// The holder's own first-person image, drawn at its first-person detail.
    first_person: bool,
    /// Drawn only in mirrors (`ItemIdentity::Reflected`).
    reflected: bool,
}
impl ModelKey {
    fn new(model: &str, tint: [f32; 4]) -> Self {
        Self {
            model: model.into(),
            tint: tint.map(f32::to_bits),
            first_person: false,
            reflected: false,
        }
    }
    fn tint(&self) -> [f32; 4] {
        self.tint.map(f32::from_bits)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct PoseKey {
    sequence: Option<String>,
    seconds: u32,
}
#[derive(Default)]
struct Geometry {
    vertices: Vec<SceneVertex>,
    indices: Vec<u32>,
    batches: Vec<MeshBatch>,
}
impl Geometry {
    fn swap(&mut self, mesh: &mut ItemMesh) {
        std::mem::swap(&mut self.vertices, &mut mesh.data.vertices);
        std::mem::swap(&mut self.indices, &mut mesh.data.indices);
        std::mem::swap(&mut self.batches, &mut mesh.data.batches);
    }
}
struct Slot {
    pose: PoseKey,
    geometry: Geometry,
    transforms: Vec<SceneTransform>,
    identities: Vec<ItemIdentity>,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    instance_capacity: usize,
    topology_dirty: bool,
    vertices_dirty: bool,
}
struct Model {
    mesh: ItemMesh,
    gpu: Option<GpuScene>,
    slots: Vec<Slot>,
}
#[derive(Clone)]
struct AnimationClock {
    image: String,
    state: String,
    sequence: Option<String>,
    started: f64,
    speed: f64,
    frozen: bool,
    /// The server re-entered a state playing this sequence (a held hammer
    /// loops Fire, CheckFire, Fire), so the clip starts over even though
    /// the replicated state name never changed.
    restart: Option<String>,
}
#[derive(Clone)]
struct MountedPose {
    image: String,
    model: String,
    transform: Mat4,
    velocity: Vec3,
    pose: PoseKey,
}
struct Candidate {
    identity: ItemIdentity,
    model: ModelKey,
    pose: PoseKey,
    transform: SceneTransform,
    priority: bool,
}
type PoseGroups = Vec<(PoseKey, Vec<Candidate>)>;

pub struct WorldItems {
    assets: Arc<ItemAssets>,
    weapons: Arc<bri_weapons::Pack>,
    limits: WorldItemLimits,
    models: BTreeMap<ModelKey, Model>,
    clocks: BTreeMap<(u64, u8), AnimationClock>,
    mounted: BTreeMap<(u64, u8), MountedPose>,
    last_seconds: Option<f64>,
    /// World palette for colour spray cans.
    palette: Vec<[f32; 4]>,
    /// `$pref::Player::renderMyItems`: off hides the player's own held
    /// items in first person (Torque `Player::renderObject`); their mount
    /// poses stay for muzzle effects.
    hide_own_first_person: bool,
    /// Each projectile's last flight direction. A stuck arrow's velocity is
    /// zero, but it keeps pointing the way it flew into the wall, as v20's
    /// projectile keeps its last render transform.
    headings: BTreeMap<u64, Vec3>,
    /// `moves_drawn` answers by model, sequence and first person.
    moves_drawn: BTreeMap<(String, String, bool), bool>,
    /// Held image models as Add-On meshes (`image_mesh`), by model; `None`
    /// for a model that would not build.
    addon_meshes: BTreeMap<String, Option<Arc<bri_client_sandbox::host::Mesh>>>,
    pub diagnostics: WorldItemDiagnostics,
}

/// `Item::fadeOut`'s node alpha for a picked-up brick item awaiting respawn.
pub const RESPAWN_GHOST_ALPHA: f32 = 0.25;

/// `setSprayCanColor`: a translucent palette colour uses the clear can.
const TRANSLUCENT_SPRAY_CAN: &str = "base/data/shapes/transspraycan.dts";

impl WorldItems {
    pub fn new(
        assets: Arc<ItemAssets>,
        weapons: Arc<bri_weapons::Pack>,
        limits: WorldItemLimits,
    ) -> Result<Self> {
        ensure!(
            limits.instances > 0
                && limits.instances <= 16384
                && limits.models > 0
                && limits.models <= 512
                && limits.geometry_slots > 0
                && limits.geometry_slots <= 2048
                && limits.vertices > 0
                && limits.vertices <= 16_000_000,
            "Invalid world item limits"
        );
        weapons.validate()?;
        Ok(Self {
            assets,
            weapons,
            limits,
            models: BTreeMap::new(),
            clocks: BTreeMap::new(),
            moves_drawn: BTreeMap::new(),
            addon_meshes: BTreeMap::new(),
            mounted: BTreeMap::new(),
            last_seconds: None,
            palette: Vec::new(),
            hide_own_first_person: false,
            headings: BTreeMap::new(),
            diagnostics: Default::default(),
        })
    }
    pub fn reset(&mut self) {
        self.models.clear();
        self.clocks.clear();
        self.mounted.clear();
        self.headings.clear();
        self.last_seconds = None;
        self.diagnostics = Default::default();
    }
    /// CPU models/animation clocks survive device recreation.
    pub fn clear_gpu(&mut self) {
        for model in self.models.values_mut() {
            model.gpu = None;
            for slot in &mut model.slots {
                slot.gpu = None;
                slot.instances = None;
                slot.instance_capacity = 0;
            }
        }
    }
    fn message(&mut self, message: String) {
        if self.diagnostics.messages.len() < 128 {
            self.diagnostics.messages.insert(message);
        }
    }
    fn missing(&mut self, message: String) {
        self.diagnostics.missing_bindings += 1;
        self.message(message);
    }
    /// Read-only inspection of the current CPU projection (also useful before GPU initialization).
    pub fn instances(&self) -> impl Iterator<Item = (ItemIdentity, &SceneTransform)> {
        self.models
            .values()
            .flat_map(|m| &m.slots)
            .flat_map(|s| s.identities.iter().copied().zip(&s.transforms))
    }
    /// Options > Advanced's Render Items.
    pub fn set_render_my_items(&mut self, on: bool) {
        self.hide_own_first_person = !on;
    }
    pub fn set_palette(&mut self, palette: &[[f32; 4]]) {
        if self.palette != palette {
            self.palette = palette.to_vec();
        }
    }
    pub fn model_scenes(&self) -> impl Iterator<Item = &bri_render::scene::SceneData> {
        self.models.values().map(|m| &m.mesh.data)
    }

    pub fn sync(
        &mut self,
        view: &WeaponView,
        frame: WorldItemFrame,
        mut host_pose: impl FnMut(u64) -> Option<MountPose>,
    ) -> Result<()> {
        ensure!(
            frame.seconds.is_finite()
                && frame.seconds >= 0.
                && frame.eye.is_finite()
                && self.last_seconds.is_none_or(|t| frame.seconds >= t),
            "Invalid/backward item render clock; reset on session change"
        );
        ensure!(
            view.images.keys().all(|id| *id > 0),
            "Invalid item owners"
        );
        let names = view.images.keys().map(|id| (*id, String::new())).collect();
        view.validate(&names)?;
        self.last_seconds = Some(frame.seconds);
        self.diagnostics.visible_instances = 0;
        self.diagnostics.model_less = 0;
        self.diagnostics.cooling_down = 0;
        self.diagnostics.deferred = 0;
        self.diagnostics.missing_bindings = 0;
        self.diagnostics.missing_poses = 0;
        self.diagnostics.missing_sequences = 0;
        let mut candidates = Vec::new();
        for item in &view.static_items {
            // `Item::fadeOut` keeps a picked-up brick item in place as a ghost
            // (`setNodeColor("ALL", <ItemData colour> SPC 0.25)`) until
            // `fadeIn` restores the image colour. Availability is the only
            // replicated state; the look is derived here.
            let ghost = item.available_at > frame.tick;
            if ghost {
                self.diagnostics.cooling_down += 1;
            }
            let Some(key) = self.item_key(&item.item, ghost) else {
                continue;
            };
            candidates.push(Candidate {
                identity: ItemIdentity::Static(item.brick),
                model: key,
                pose: PoseKey::default(),
                transform: SceneTransform {
                    transform: Mat4::from_rotation_translation(
                        item.rotation(),
                        Vec3::from(item.position),
                    ),
                    tint: [1., 1., 1., if ghost { RESPAWN_GHOST_ALPHA } else { 1. }],
                },
                priority: false,
            });
        }
        for drop in &view.drops {
            let alpha = drop_opacity(frame.tick, drop.expires);
            if alpha == 0. {
                continue;
            }
            let Some(key) =
                self.item_key(&drop.item, frame.tick.saturating_add(120) >= drop.expires)
            else {
                continue;
            };
            candidates.push(Candidate {
                identity: ItemIdentity::Drop(drop.id),
                model: key,
                pose: PoseKey::default(),
                transform: SceneTransform {
                    transform: Mat4::from_scale_rotation_translation(
                        Vec3::splat(drop.scale),
                        drop.rotation,
                        drop.position,
                    ),
                    tint: [1., 1., 1., alpha],
                },
                priority: false,
            });
        }
        self.headings
            .retain(|id, _| view.projectiles.iter().any(|p| p.id == *id));
        for projectile in &view.projectiles {
            let velocity = projectile.velocity;
            let heading = if velocity.length_squared() > 1e-6 && velocity.is_finite() {
                *self.headings.entry(projectile.id).or_default() = velocity;
                velocity
            } else {
                // The server sends a stuck projectile's direction, so a
                // player who never saw it fly still sees it right.
                projectile
                    .heading
                    .filter(|h| h.is_finite())
                    .or_else(|| self.headings.get(&projectile.id).copied())
                    .unwrap_or(velocity)
            };
            let Some(binding) = self
                .assets
                .presentation
                .projectiles
                .get(&projectile.definition)
                .cloned()
            else {
                self.missing(format!(
                    "Missing projectile presentation {}",
                    projectile.definition
                ));
                continue;
            };
            let Some(model) = binding.model else {
                self.diagnostics.model_less += 1;
                continue;
            };
            let Some(definition) = self.weapons.projectiles.get(&projectile.definition) else {
                self.missing(format!(
                    "Missing projectile definition {}",
                    projectile.definition
                ));
                continue;
            };
            let alpha = projectile_opacity(
                projectile.age,
                definition.fade_ticks,
                definition.lifetime_ticks,
            );
            if alpha == 0. {
                continue;
            }
            let pose = self.projectile_pose(&model, f64::from(projectile.age) / 120.)?;
            if self
                .assets
                .shape(&model)?
                .meshes
                .iter()
                .flatten()
                .any(|m| m.billboard || m.billboard_y)
            {
                self.message(format!(
                    "{model}: authored mesh billboard still requires per-mesh camera-facing binding"
                ));
            }
            candidates.push(Candidate {
                identity: ItemIdentity::Projectile(projectile.id),
                model: ModelKey::new(&model, binding.tint),
                pose,
                transform: SceneTransform {
                    transform: Mat4::from_scale_rotation_translation(
                        Vec3::splat(projectile.scale),
                        projectile_rotation(heading),
                        projectile.position,
                    ),
                    tint: [1., 1., 1., alpha],
                },
                priority: false,
            });
        }
        let mounted: BTreeMap<(u64, u8), (String, String, Option<u8>)> = view
            .images
            .iter()
            .flat_map(|(&owner, images)| {
                images.iter().map(move |image| {
                    (
                        (owner, image.hand),
                        (image.image.clone(), image.state.clone(), image.paint),
                    )
                })
            })
            .collect();
        self.clocks.retain(|id, _| mounted.contains_key(id));
        self.mounted.clear();
        let mut poses = BTreeMap::new();
        for (&(owner, hand), (image_id, state_name, paint)) in &mounted {
            let Some(mut image) = self.assets.presentation.images.get(image_id).cloned() else {
                self.missing(format!("Missing mounted image {image_id}"));
                continue;
            };
            if image.model.is_empty() {
                self.diagnostics.model_less += 1;
                continue;
            }
            if let Some(color) = paint.and_then(|p| self.palette.get(usize::from(p))) {
                // The derived `color<N>SprayCanImage`: palette colour shift,
                // alpha at least 10/255, clear can for translucent colours.
                image.tint = [color[0], color[1], color[2], color[3].max(10. / 255.)];
                if color[3] <= 0.99 {
                    image.model = TRANSLUCENT_SPRAY_CAN.into();
                }
            }
            let pose_key = self.image_pose(owner, hand, image_id, state_name, frame.seconds)?;
            let pose = poses.entry(owner).or_insert_with(|| host_pose(owner));
            let Some(pose) = pose.as_ref() else {
                self.diagnostics.missing_poses += 1;
                self.message(format!("Missing avatar pose for owner{owner}"));
                continue;
            };
            if !pose.velocity.is_finite()
                || !valid_transform(pose.eye)
                || pose
                    .mounts
                    .values()
                    .chain(pose.actions.values())
                    .any(|m| !valid_transform(*m))
            {
                self.diagnostics.missing_poses += 1;
                self.message(format!("Invalid avatar pose for owner{owner}"));
                continue;
            }
            let local_first = frame.local_owner == Some(owner) && frame.first_person;
            let transform = match self.assets.moved_mount_transform(
                image_id,
                local_first,
                pose.eye,
                |n| pose.mounts.get(&n).copied(),
                |n| pose.actions.get(&n).copied(),
            ) {
                Ok(t) if valid_transform(t) => t,
                _ => {
                    self.diagnostics.missing_poses += 1;
                    self.message(format!(
                        "Missing/invalid authored mount{} for owner{owner}/{image_id}",
                        image.mount_point
                    ));
                    continue;
                }
            };
            self.mounted.insert(
                (owner, hand),
                MountedPose {
                    image: image_id.clone(),
                    model: image.model.clone(),
                    transform,
                    velocity: pose.velocity,
                    pose: pose_key.clone(),
                },
            );
            if local_first && frame.reflected_self {
                // As everyone else sees it: third-person mount and detail.
                if let Ok(transform) = self.assets.moved_mount_transform(
                    image_id,
                    false,
                    pose.eye,
                    |n| pose.mounts.get(&n).copied(),
                    |n| pose.actions.get(&n).copied(),
                ) && valid_transform(transform)
                {
                    let drawn_pose = match &pose_key.sequence {
                        Some(sequence) if !self.moves_drawn(&image.model, sequence, false) => {
                            PoseKey::default()
                        }
                        _ => pose_key.clone(),
                    };
                    candidates.push(Candidate {
                        identity: ItemIdentity::Reflected(owner, hand),
                        model: ModelKey {
                            reflected: true,
                            ..ModelKey::new(&image.model, image.tint)
                        },
                        pose: drawn_pose,
                        transform: SceneTransform {
                            transform,
                            tint: [1.; 4],
                        },
                        priority: true,
                    });
                }
            }
            if local_first && self.hide_own_first_person {
                continue;
            }
            // A sequence that leaves the drawn detail still (most fire
            // clips animate only the holder's first-person mesh) draws the
            // rest pose, which every holder of the image shares.
            let drawn_pose = match &pose_key.sequence {
                Some(sequence) if !self.moves_drawn(&image.model, sequence, local_first) => {
                    PoseKey::default()
                }
                _ => pose_key,
            };
            candidates.push(Candidate {
                identity: ItemIdentity::Mounted(owner, hand),
                model: ModelKey {
                    first_person: local_first,
                    ..ModelKey::new(&image.model, image.tint)
                },
                pose: drawn_pose,
                transform: SceneTransform {
                    transform,
                    tint: [1.; 4],
                },
                priority: frame.local_owner == Some(owner),
            });
        }
        candidates.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| {
                    a.transform
                        .transform
                        .w_axis
                        .truncate()
                        .distance_squared(frame.eye)
                        .total_cmp(
                            &b.transform
                                .transform
                                .w_axis
                                .truncate()
                                .distance_squared(frame.eye),
                        )
                })
                .then_with(|| a.identity.cmp(&b.identity))
        });
        if candidates.len() > self.limits.instances {
            self.diagnostics.deferred += candidates.len() - self.limits.instances;
            candidates.truncate(self.limits.instances);
        }
        self.prepare(candidates)?;
        Ok(())
    }

    /// `faded` items (`schedulePop`, `Item::fadeOut`) take the ItemData colour
    /// (or white) and leave their alpha to the instance.
    fn item_key(&mut self, id: &str, faded: bool) -> Option<ModelKey> {
        let Some(item) = self.assets.presentation.items.get(id) else {
            self.missing(format!("Missing item presentation {id}"));
            return None;
        };
        if item.model.is_empty() {
            self.diagnostics.model_less += 1;
            return None;
        }
        // Core onAdd applies image color when enabled; schedulePop and fadeOut
        // set the ItemData color/white separately with their own node alpha.
        let mut tint = item.tint;
        if !faded
            && let Some(image) = self.weapons.images.get(&item.image)
            && image.color_shift
        {
            tint = image.color;
        }
        if faded {
            tint[3] = 1.;
        }
        Some(ModelKey::new(&item.model, tint))
    }
    fn projectile_pose(&self, model: &str, age: f64) -> Result<PoseKey> {
        let shape = self.assets.shape(model)?;
        let activate = shape
            .animations
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("activate"));
        let maintain = shape
            .animations
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("maintain"));
        let (clip, time) = if let Some(a) = activate.filter(|a| age < f64::from(a.duration)) {
            (Some(a), age)
        } else if let Some(a) = maintain {
            (
                Some(a),
                age - activate.map_or(0., |a| f64::from(a.duration)),
            )
        } else {
            (activate, age)
        };
        Ok(clip.map_or_else(PoseKey::default, |a| normalized_pose(a, time)))
    }
    /// Whether `sequence` changes what `model` draws for a holder
    /// (`first_person`) or others, remembered per model and sequence.
    fn moves_drawn(&mut self, model: &str, sequence: &str, first_person: bool) -> bool {
        let key = (model.to_string(), sequence.to_string(), first_person);
        if let Some(moves) = self.moves_drawn.get(&key) {
            return *moves;
        }
        let moves = self.assets.shape(model).is_ok_and(|shape| {
            shape
                .animations
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(sequence))
                // An unknown sequence poses as it did.
                .is_none_or(|clip| {
                    crate::items::moves_visible_detail(shape, clip, first_person)
                })
        });
        self.moves_drawn.insert(key, moves);
        moves
    }
    fn image_pose(
        &mut self,
        owner: u64,
        hand: u8,
        image: &str,
        state: &str,
        seconds: f64,
    ) -> Result<PoseKey> {
        let binding = &self.assets.presentation.images[image];
        let shape = self.assets.shape(&binding.model)?;
        let definition = self
            .weapons
            .images
            .get(image)
            .and_then(|i| i.states.iter().find(|s| s.name.eq_ignore_ascii_case(state)));
        let sequence = definition
            .map(|s| s.sequence.as_str())
            .filter(|s| !s.is_empty());
        let clip = sequence.and_then(|name| {
            shape
                .animations
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(name))
        });
        let missing_sequence = sequence.filter(|_| clip.is_none()).map(str::to_owned);
        let clock = self
            .clocks
            .entry((owner, hand))
            .or_insert_with(|| AnimationClock {
                image: String::new(),
                state: String::new(),
                sequence: None,
                started: seconds,
                speed: 1.,
                frozen: false,
                restart: None,
            });
        if clock.image != image || clock.state != state {
            if clock.image != image {
                clock.sequence = None;
            } else if clock
                .sequence
                .as_ref()
                .and_then(|n| shape.animations.iter().find(|a| &a.name == n))
                .is_some_and(|a| a.looping)
            {
                clock.started = seconds;
                clock.frozen = true;
            }
            clock.image = image.into();
            clock.state = state.into();
            if let Some(clip) = clip {
                clock.sequence = Some(clip.name.clone());
                clock.started = seconds;
                clock.frozen = false;
                clock.speed = definition.filter(|s| s.ticks > 0).map_or(1., |s| {
                    f64::from(clip.duration) / (f64::from(s.ticks) / 120.)
                });
            }
        }
        // `ShapeBase::setImageState` restarts the state's sequence on every
        // entry.
        if let Some(restart) = clock.restart.take()
            && let Some(clip) = clip.filter(|c| c.name.eq_ignore_ascii_case(&restart))
        {
            clock.sequence = Some(clip.name.clone());
            clock.started = seconds;
            clock.frozen = false;
        }
        let result = clock
            .sequence
            .as_ref()
            .and_then(|n| shape.animations.iter().find(|a| &a.name == n))
            .map_or_else(PoseKey::default, |clip| {
                normalized_pose(
                    clip,
                    if clock.frozen {
                        0.
                    } else {
                        (seconds - clock.started) * clock.speed
                    },
                )
            });
        if let Some(sequence) = missing_sequence {
            self.diagnostics.missing_sequences += 1;
            self.message(format!("{image}/{state}: absent authored sequence {sequence}; preserves prior sequence semantics"));
        }
        Ok(result)
    }

    fn prepare(&mut self, candidates: Vec<Candidate>) -> Result<()> {
        // Preserve the priority order established by sync. Sorting groups by model
        // name would let distant/static items displace the local held weapon.
        let mut groups: Vec<(ModelKey, PoseGroups)> = Vec::new();
        let mut model_indices = BTreeMap::new();
        for candidate in candidates {
            let index = *model_indices
                .entry(candidate.model.clone())
                .or_insert_with(|| {
                    groups.push((candidate.model.clone(), Vec::new()));
                    groups.len() - 1
                });
            let poses = &mut groups[index].1;
            if let Some((_, instances)) = poses.iter_mut().find(|(pose, _)| *pose == candidate.pose)
            {
                instances.push(candidate);
            } else {
                poses.push((candidate.pose.clone(), vec![candidate]));
            }
        }
        let mut cache = std::mem::take(&mut self.models);
        let mut total_slots = 0usize;
        let mut total_vertices = 0usize;
        for (key, poses) in groups {
            if self.models.len() == self.limits.models {
                self.diagnostics.deferred += poses.iter().map(|(_, v)| v.len()).sum::<usize>();
                continue;
            }
            let mut model = if let Some(model) = cache.remove(&key) {
                model
            } else {
                let mut mesh = self.assets.mesh(&key.model, key.tint())?;
                mesh.first_person = key.first_person;
                if mesh.data.vertices.is_empty() {
                    self.missing(format!("Model {} has no visible geometry", key.model));
                    continue;
                }
                self.diagnostics.model_builds += 1;
                for warning in &mesh.data.omissions {
                    self.message(warning.clone());
                }
                Model {
                    mesh,
                    gpu: None,
                    slots: Vec::new(),
                }
            };
            let base_vertices = model.mesh.data.vertices.len();
            if total_vertices + base_vertices > self.limits.vertices {
                self.diagnostics.deferred += poses.iter().map(|(_, v)| v.len()).sum::<usize>();
                continue;
            }
            total_vertices += base_vertices;
            // Only currently needed pose slots reserve capacity. Old animations
            // must not permanently starve another weapon after a busy scene.
            let mut reusable = std::mem::take(&mut model.slots);
            for (pose, instances) in poses {
                if total_slots == self.limits.geometry_slots {
                    self.diagnostics.deferred += instances.len();
                    continue;
                }
                let existing = reusable
                    .iter()
                    .position(|s| s.pose == pose)
                    .or_else(|| (!reusable.is_empty()).then_some(0));
                let mut slot = existing
                    .map(|i| reusable.swap_remove(i))
                    .unwrap_or_else(|| Slot {
                        pose: PoseKey {
                            sequence: Some(String::new()),
                            seconds: 0,
                        },
                        geometry: Geometry::default(),
                        transforms: Vec::new(),
                        identities: Vec::new(),
                        gpu: None,
                        instances: None,
                        instance_capacity: 0,
                        topology_dirty: true,
                        vertices_dirty: true,
                    });
                slot.transforms.clear();
                slot.identities.clear();
                if slot.pose != pose {
                    // Pose into the slot's own buffers (the mesh keeps the
                    // model's rest copy): unchanged structure is rewritten
                    // in place, and `pose` says when it is not.
                    let mut geometry = std::mem::take(&mut slot.geometry);
                    geometry.swap(&mut model.mesh);
                    let result = model.mesh.pose(
                        &self.assets,
                        Mat4::IDENTITY,
                        pose.sequence.as_deref(),
                        f32::from_bits(pose.seconds),
                    );
                    geometry.swap(&mut model.mesh);
                    slot.geometry = geometry;
                    let topology = result?;
                    slot.topology_dirty |= topology;
                    slot.vertices_dirty = true;
                    slot.pose = pose;
                    self.diagnostics.pose_samples += 1;
                }
                if total_vertices + slot.geometry.vertices.len() > self.limits.vertices {
                    self.diagnostics.deferred += instances.len();
                    continue;
                }
                total_vertices += slot.geometry.vertices.len();
                total_slots += 1;
                for instance in instances {
                    slot.identities.push(instance.identity);
                    slot.transforms.push(instance.transform);
                    self.diagnostics.visible_instances += 1;
                }
                model.slots.push(slot);
            }
            if model.slots.is_empty() {
                total_vertices -= base_vertices;
            } else {
                self.models.insert(key, model);
            }
        }
        // Unused models/slots are dropped here, including their device handles.
        // Active static geometry and identical sampled poses retain their uploads.
        self.diagnostics.cached_models = self.models.len();
        self.diagnostics.geometry_slots = total_slots;
        self.diagnostics.geometry_vertices = total_vertices;
        Ok(())
    }

    /// GPU textures/materials upload only once per model+tint cache entry. All
    /// animation/topology variants clone the existing material bind groups.
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        for model in self.models.values_mut() {
            if model.gpu.is_none() && model.slots.iter().any(|s| !s.transforms.is_empty()) {
                model.gpu = Some(renderer.upload(device, queue, &model.mesh.data)?);
                self.diagnostics.model_uploads += 1;
            }
            for slot in &mut model.slots {
                if slot.transforms.is_empty() {
                    continue;
                }
                if slot.gpu.is_none() || slot.topology_dirty {
                    slot.geometry.swap(&mut model.mesh);
                    let result = renderer.upload_geometry_shared(
                        device,
                        &model.mesh.data,
                        model.gpu.as_ref().unwrap(),
                    );
                    slot.geometry.swap(&mut model.mesh);
                    slot.gpu = Some(result?);
                    slot.topology_dirty = false;
                    slot.vertices_dirty = false;
                    self.diagnostics.shared_geometry_uploads += 1;
                } else if slot.vertices_dirty
                    && let Some(gpu) = &mut slot.gpu
                {
                    let centers: Vec<_> = slot.geometry.batches.iter().map(|b| b.center).collect();
                    gpu.update_vertices(queue, &slot.geometry.vertices, &centers)?;
                    slot.vertices_dirty = false;
                    self.diagnostics.vertex_updates += 1;
                }
                if slot.instances.is_none() || slot.instance_capacity < slot.transforms.len() {
                    slot.instance_capacity = slot
                        .transforms
                        .len()
                        .next_power_of_two()
                        .min(self.limits.instances);
                    slot.instances = Some(GpuInstances::new(device, slot.instance_capacity)?);
                }
                if slot
                    .instances
                    .as_mut()
                    .unwrap()
                    .update(queue, &slot.transforms)?
                {
                    self.diagnostics.instance_updates += 1;
                }
            }
        }
        Ok(())
    }
    /// What the player's view draws.
    pub fn draws(&self) -> Vec<(&GpuScene, &GpuInstances)> {
        self.draws_where(|key| !key.reflected)
    }
    /// What mirrors show: the local player's images as others see them,
    /// not as first person holds them.
    pub fn reflection_draws(&self) -> Vec<(&GpuScene, &GpuInstances)> {
        self.draws_where(|key| !key.first_person)
    }
    fn draws_where(&self, keep: impl Fn(&ModelKey) -> bool) -> Vec<(&GpuScene, &GpuInstances)> {
        self.models
            .iter()
            .filter(|(key, _)| keep(key))
            .flat_map(|(_, m)| &m.slots)
            .filter(|s| !s.transforms.is_empty())
            .filter_map(|s| Some((s.gpu.as_ref()?, s.instances.as_ref()?)))
            .collect()
    }
    /// Resolve a named node only on the exact currently mounted image/hand.
    /// A mounted image entered a state with this `stateSequence` (a
    /// `WeaponAnimation` cue on the image's thread): start it over.
    pub fn restart_image_sequence(&mut self, owner: u64, hand: u8, sequence: &str) {
        if let Some(clock) = self.clocks.get_mut(&(owner, hand)) {
            clock.restart = Some(sequence.to_owned());
        }
    }
    /// Where `owner`'s image in `hand` was placed by the last sync.
    pub fn mounted_transform(&self, owner: u64, hand: u8) -> Option<Mat4> {
        self.mounted.get(&(owner, hand)).map(|m| m.transform)
    }
    pub fn mounted_node(&self, owner: u64, hand: u8, image: &str, node: &str) -> Result<Mat4> {
        let mounted = self
            .mounted
            .get(&(owner, hand))
            .filter(|m| m.image == image)
            .context("Image/hand no longer mounted")?;
        let pose = self.assets.pose(
            &mounted.model,
            mounted.pose.sequence.as_deref(),
            f32::from_bits(mounted.pose.seconds),
        )?;
        self.assets
            .node_transform(&mounted.model, &pose, mounted.transform, node)
    }
    /// Where the image in `owner`'s `hand` fires from, as drawn now: its
    /// `muzzlePoint`, or `None` when nothing with one is held.
    pub fn held_muzzle(&self, owner: u64, hand: u8) -> Option<Vec3> {
        let image = &self.mounted.get(&(owner, hand))?.image;
        let point = self
            .mounted_node(owner, hand, image, "muzzlePoint")
            .ok()?
            .w_axis
            .truncate();
        point.is_finite().then_some(point)
    }
    /// Every weapon image `owner` holds, as the last sync drew it: hand,
    /// model matrix and muzzle.
    pub fn held_images(&self, owner: u64) -> Vec<bri_client_sandbox::world::Held> {
        self.mounted
            .range((owner, 0)..=(owner, u8::MAX))
            .map(|(&(_, hand), m)| bri_client_sandbox::world::Held {
                hand,
                transform: m.transform.to_cols_array(),
                muzzle: self.held_muzzle(owner, hand).map(|p| p.to_array()),
            })
            .collect()
    }
    /// The model of every weapon image someone holds now as an Add-On
    /// mesh (position, normal, uv at rest, in the image's own space), by
    /// image id. Each model is built once.
    pub fn held_image_meshes(
        &mut self,
    ) -> BTreeMap<String, Arc<bri_client_sandbox::host::Mesh>> {
        let mut out = BTreeMap::new();
        for m in self.mounted.values() {
            if m.model.is_empty() || out.contains_key(&m.image) {
                continue;
            }
            let assets = &self.assets;
            let mesh = self
                .addon_meshes
                .entry(m.model.clone())
                .or_insert_with(|| {
                    let scene = assets
                        .model_scene(&m.model, [1.; 4], Mat4::IDENTITY, None, 0.)
                        .ok()?;
                    let vertices: Vec<_> = scene
                        .vertices
                        .iter()
                        .map(|v| bri_client_sandbox::host::Vertex {
                            position: v.position,
                            normal: v.normal,
                            uv: v.uv,
                        })
                        .collect();
                    (!vertices.is_empty() && !scene.indices.is_empty()).then(|| {
                        Arc::new(bri_client_sandbox::host::Mesh {
                            vertices,
                            indices: scene.indices,
                        })
                    })
                });
            if let Some(mesh) = mesh {
                out.insert(m.image.clone(), mesh.clone());
            }
        }
        out
    }
    /// Source engine falls back from a missing state emitter node to muzzlePoint,
    /// and an image without a muzzlePoint (brickWeapon.dts) emits from its own
    /// transform, as `ShapeBase::getMuzzleTransform` does. No eye-origin
    /// fallback or hand inference. None drains existing emitters.
    pub fn effect_pose(&self, cue: &Cue) -> Option<bri_fx_runtime::SourceTransform> {
        let CueKind::WeaponEffect {
            source: bri_weapons::TargetId::Actor(actor),
            image: Some(image),
            hand: Some(hand),
            node,
            ..
        } = &cue.kind
        else {
            return None;
        };
        let mounted = self
            .mounted
            .get(&(actor.0, *hand))
            .filter(|m| &m.image == image)?;
        let transform = self
            .mounted_node(actor.0, *hand, image, node)
            .or_else(|_| self.mounted_node(actor.0, *hand, image, "muzzlePoint"))
            .unwrap_or(mounted.transform);
        let direction = transform.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
        if direction.length_squared() < 0.9 {
            return None;
        }
        Some(bri_fx_runtime::SourceTransform {
            position: transform.w_axis.truncate(),
            rotation: Quat::from_rotation_arc(Vec3::Y, direction),
            velocity: mounted.velocity,
        })
    }
}

fn normalized_pose(clip: &bri_content::shape::Animation, time: f64) -> PoseKey {
    let time = if clip.duration > 0. {
        if clip.looping {
            time.max(0.).rem_euclid(f64::from(clip.duration))
        } else {
            time.clamp(0., f64::from(clip.duration))
        }
    } else {
        0.
    };
    PoseKey {
        sequence: Some(clip.name.clone()),
        seconds: (time as f32).to_bits(),
    }
}
fn valid_transform(t: Mat4) -> bool {
    t.is_finite()
        && t.determinant() > 1e-8
        && t.x_axis.w == 0.
        && t.y_axis.w == 0.
        && t.z_axis.w == 0.
        && t.w_axis.w == 1.
}
/// Native -Z is original +Y forward. Keep original upright orientation rule.
pub fn projectile_rotation(velocity: Vec3) -> Quat {
    let direction = if velocity.length_squared() > 1e-12 {
        velocity.normalize()
    } else {
        Vec3::Y
    };
    let right = if direction.dot(Vec3::Y).abs() > 0.9999 {
        Vec3::X
    } else {
        direction.cross(Vec3::Y).normalize()
    };
    let up = right.cross(direction).normalize();
    Quat::from_mat3(&Mat3::from_cols(right, up, -direction)).normalize()
}
/// Recovered node-alpha steps multiplied by the separately scheduled linear fade.
/// Exact v20 node-color/fade interaction remains a qualified family adaptation.
pub fn drop_opacity(tick: u64, expires: u64) -> f32 {
    let remaining = expires.saturating_sub(tick);
    if remaining == 0 {
        0.
    } else if remaining > 120 {
        1.
    } else {
        remaining.div_ceil(24) as f32 * 0.1 * remaining as f32 / 120.
    }
}
/// Engine-family fade formula divides elapsed-since-fade by total lifetime.
pub fn projectile_opacity(age: u32, fade: u32, lifetime: u32) -> f32 {
    if age >= lifetime {
        0.
    } else {
        (1. - age.saturating_sub(fade) as f32 / lifetime.max(1) as f32).clamp(0., 1.)
    }
}
