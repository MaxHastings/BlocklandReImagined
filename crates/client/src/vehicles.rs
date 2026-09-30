//! Vehicle presentation: native vehicle models drawn as GPU instances at
//! interpolated authoritative transforms, with wheels, steering, suspension
//! and turrets, plus seat transforms for riders and the driving camera.
use crate::items::native_shape_scene;
use anyhow::{Context, Result, ensure};
use bri_content::shape::{Animation, Shape};
use bri_render::scene::{GpuInstances, GpuScene, SceneImage, SceneRenderer, SceneTransform};
use bri_sim::session::{VehicleInfo, VehiclePose};
use bri_vehicles::{Definition, Pack, schema::Wheel};
use glam::{Mat4, Quat, Vec3};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
};

const HISTORY: usize = 16;
/// Render other vehicles this many server ticks behind the newest pose.
const INTERPOLATION_TICKS: f64 = 9.0;
const TICK_RATE: f64 = 120.0;
/// The driven vehicle runs at most this many ticks past its newest pose.
const DRIVEN_AHEAD: f64 = 6.0;
/// Two poses further apart than this (in ticks) give no spin to carry on.
const SPIN_WINDOW: u64 = 30;
/// The driven vehicle's corrections decay at this rate per second.
const DRIVEN_CORRECTION_RATE: f32 = 14.0;
/// Driven corrections larger than this are teleports and snap.
const DRIVEN_SNAP: f32 = 4.0;

struct Model {
    data: bri_render::scene::SceneData,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transforms: Vec<SceneTransform>,
}

pub struct VehicleAssets {
    pack: Pack,
    /// Each definition's place in `pack.definitions`, by id.
    index: std::collections::HashMap<String, usize>,
    models: BTreeMap<String, Model>,
    /// Gunner models with a `look` clip (tank turret, pirate cannon), keyed
    /// by the model's asset path.
    looks: BTreeMap<String, LookRig>,
    /// Models whose definitions play animation threads (a propeller), keyed
    /// by the model's asset path.
    threads: BTreeMap<String, ThreadRig>,
    /// Model asset paths by lower-case source path (`add-ons/vehicle_jeep/jeeptire.dts`).
    sources: BTreeMap<String, String>,
}

/// A gunner model split into its fixed part and the parts its `look` clip
/// moves (the barrel), so the barrel can follow any pitch exactly.
struct LookRig {
    shape: Shape,
    clip: Animation,
    /// Model key, node, and the inverse of the node's transform in that model.
    parts: Vec<(String, usize, Mat4)>,
}

/// A model whose `Definition::threads` animate it: the objects on nodes
/// those sequences move are drawn apart and posed each frame.
struct ThreadRig {
    shape: Shape,
    /// The model's sequences the threads name, by lower-case name.
    clips: BTreeMap<String, Animation>,
    /// Model key, node, and the inverse of the node's rest transform.
    parts: Vec<(String, usize, Mat4)>,
}

/// Draws of the moving parts of a model with animation threads, `seconds`
/// into the game, for a vehicle moving at `speed`. Of each slot's threads
/// the first whose speed range holds `speed` plays, at its rate.
fn threaded<'a>(
    rig: &'a ThreadRig,
    threads: &[bri_vehicles::schema::AnimationThread],
    speed: f32,
    seconds: f64,
    transform: Mat4,
) -> Vec<(&'a str, Mat4)> {
    let mut layers = Vec::new();
    for slot in 0..4 {
        let Some(t) = threads.iter().find(|t| t.slot == slot && t.matches(speed)) else {
            continue;
        };
        let Some(clip) = rig.clips.get(&t.sequence.to_ascii_lowercase()) else {
            continue;
        };
        // Whole loops are dropped in f64 so the phase keeps its precision
        // however long the game runs.
        let time = seconds * f64::from(t.rate);
        let time = if clip.looping && clip.duration > 0.0 {
            time.rem_euclid(f64::from(clip.duration))
        } else {
            time
        };
        layers.push(bri_content::animation::Layer {
            animation: clip,
            time: time as f32,
            weight: 1.0,
        });
    }
    let Ok(pose) = bri_content::animation::sample_layers(&rig.shape, &layers) else {
        return Vec::new();
    };
    rig.parts
        .iter()
        .map(|(key, node, inverse)| (key.as_str(), transform * pose.nodes[*node] * *inverse))
        .collect()
}

/// Draws of a model at `transform`: the model itself plus, for a gunner
/// model, its barrel parts posed by the `look` clip at this pitch.
fn posed<'a>(
    looks: &'a BTreeMap<String, LookRig>,
    model: &'a str,
    pitch: f32,
    transform: Mat4,
) -> Vec<(&'a str, Mat4)> {
    let mut out = vec![(model, transform)];
    if let Some(rig) = looks.get(model) {
        let time = bri_vehicles::muzzle::look_phase(pitch) * rig.clip.duration;
        if let Ok(pose) = bri_content::animation::sample(&rig.shape, Some(&rig.clip), time) {
            for (key, node, inverse) in &rig.parts {
                out.push((key.as_str(), transform * pose.nodes[*node] * *inverse));
            }
        }
    }
    out
}

impl VehicleAssets {
    /// Load every declared model with its folder-local material textures.
    pub fn load(root: &Path) -> Result<Self> {
        Self::load_with(root, &[])
    }
    /// [`Self::load`] for the base pack merged with other packages' vehicles
    /// (`content_identity::kind_providers`).
    pub fn load_with(root: &Path, extras: &[(String, std::path::PathBuf)]) -> Result<Self> {
        let root = root.canonicalize()?;
        let mut parts = Vec::new();
        for (dir, abs) in extras {
            let part = Pack::load(abs.join("vehicles.json")).with_context(|| {
                format!(
                    "Add-On {}: vehicles.json",
                    bri_package::library::add_on_label(abs, dir)
                )
            })?;
            parts.push((dir.clone(), part));
        }
        let (pack, _) = Pack::load(root.join("vehicles.json"))?.merge(parts);
        // An Add-On's texture or model that does not load is a cosmetic
        // fault (`crate::cosmetic`): blank paint, or the vehicle undrawn.
        let fault = |asset: &bri_vehicles::schema::Asset, error: anyhow::Error| -> Result<()> {
            match &asset.package {
                Some(dir) => {
                    let label = bri_package::library::add_on_label(
                        &bri_vehicles::asset_root(&root, asset),
                        dir,
                    );
                    crate::cosmetic::add_on_fault(&label, &asset.path, format!("{error:#}"));
                    Ok(())
                }
                None => Err(error),
            }
        };
        let mut textures: BTreeMap<String, SceneImage> = BTreeMap::new();
        for asset in pack.assets.iter().filter(|a| a.kind == "texture") {
            let image = crate::items::checked_read(
                &bri_vehicles::asset_root(&root, asset),
                &asset.path,
                &asset.sha256,
                16 << 20,
            )
            .and_then(|bytes| Ok(image::load_from_memory(&bytes)?.to_rgba8()));
            let image = match image {
                Ok(image) => image,
                Err(error) => {
                    fault(asset, error)?;
                    continue;
                }
            };
            textures.insert(
                asset.virtual_path.to_ascii_lowercase(),
                SceneImage {
                    label: asset.virtual_path.clone(),
                    width: image.width(),
                    height: image.height(),
                    rgba: image.into_raw(),
                    srgb: false,
                },
            );
        }
        let blank = textures
            .iter()
            .find(|(path, _)| path.ends_with("/blank.png"))
            .map(|(_, image)| image.clone())
            .context("Vehicle pack has no blank paint texture")?;
        // Gunners pitch their barrel with a `look` clip, as
        // `Player::updateLookAnimation` does from the head pitch.
        let look_clips = bri_vehicles::muzzle::look_clips(&pack, &root)?;
        let gunner_models: Vec<String> = pack
            .definitions
            .iter()
            .filter(|d| d.weapon.is_some())
            .map(|d| d.attachment_model.clone().unwrap_or(d.model.clone()))
            .collect();
        let mut models = BTreeMap::new();
        let mut looks = BTreeMap::new();
        let mut threads = BTreeMap::new();
        for asset in pack.assets.iter().filter(|a| a.kind == "model") {
            let shape = crate::items::checked_read(
                &bri_vehicles::asset_root(&root, asset),
                &asset.path,
                &asset.sha256,
                32 << 20,
            )
            .and_then(|bytes| {
                let shape: Shape = serde_json::from_slice(&bytes)?;
                shape.validate()?;
                Ok(shape)
            });
            let shape = match shape {
                Ok(shape) => shape,
                Err(error) => {
                    fault(asset, error)?;
                    continue;
                }
            };
            let folder = asset
                .virtual_path
                .rsplit_once('/')
                .map_or(String::new(), |(dir, _)| dir.to_ascii_lowercase());
            // DTS materials name their texture; resolve beside the model, then
            // anywhere in the pack, then the blank paint surface.
            let images: Vec<SceneImage> = shape
                .materials
                .iter()
                .map(|m| {
                    let name = m.name.to_ascii_lowercase();
                    textures
                        .get(&format!("{folder}/{name}.png"))
                        .or_else(|| {
                            textures
                                .iter()
                                .find(|(path, _)| path.ends_with(&format!("/{name}.png")))
                                .map(|(_, image)| image)
                        })
                        .cloned()
                        .unwrap_or_else(|| blank.clone())
                })
                .collect();
            let refs: Vec<&SceneImage> = images.iter().collect();
            let look = gunner_models
                .contains(&asset.path)
                .then(|| bri_vehicles::muzzle::look_clip(&look_clips, &shape))
                .flatten();
            let mut insert = |key: String, pose: &bri_content::animation::Pose| -> Result<()> {
                let data =
                    native_shape_scene(&key, &shape, &refs, [1.0; 4], false, Mat4::IDENTITY, pose)?;
                models.insert(
                    key,
                    Model {
                        data,
                        gpu: None,
                        instances: None,
                        transforms: Vec::new(),
                    },
                );
                Ok(())
            };
            let Some(look) = look else {
                let rest = bri_content::animation::sample(&shape, None, 0.0)?;
                // The sequences this model's definitions play by themselves.
                let mut clips = BTreeMap::new();
                for d in pack.definitions.iter().filter(|d| d.model == asset.path) {
                    for t in &d.threads {
                        if let Some(clip) = shape
                            .animations
                            .iter()
                            .find(|a| a.name.eq_ignore_ascii_case(&t.sequence))
                        {
                            clips.insert(t.sequence.to_ascii_lowercase(), clip.clone());
                        }
                    }
                }
                let mut moving = std::collections::BTreeSet::new();
                for clip in clips.values() {
                    for step in 1..=8 {
                        let other = bri_content::animation::sample(
                            &shape,
                            Some(clip),
                            clip.duration * step as f32 / 8.0,
                        )?;
                        for (i, object) in shape.objects.iter().enumerate() {
                            if let Some(node) = object.node
                                && !other.nodes[node].abs_diff_eq(rest.nodes[node], 1e-5)
                                && object.meshes.iter().all(|m| {
                                    shape
                                        .meshes
                                        .get(*m)
                                        .and_then(Option::as_ref)
                                        .is_none_or(|m| m.skin.is_none())
                                })
                            {
                                moving.insert((node, i));
                            }
                        }
                    }
                }
                let only = |keep: &dyn Fn(usize) -> bool| {
                    let mut part = bri_content::animation::sample(&shape, None, 0.0)?;
                    for (i, v) in part.visibility.iter_mut().enumerate() {
                        if !keep(i) {
                            *v = 0.0;
                        }
                    }
                    anyhow::Ok(part)
                };
                insert(
                    asset.path.clone(),
                    &only(&|i| !moving.iter().any(|(_, o)| *o == i))?,
                )?;
                if !moving.is_empty() {
                    let mut parts = Vec::new();
                    let nodes: std::collections::BTreeSet<usize> =
                        moving.iter().map(|(n, _)| *n).collect();
                    for node in nodes {
                        let key = format!("{}#thread{node}", asset.path);
                        insert(
                            key.clone(),
                            &only(&|i| shape.objects[i].node == Some(node))?,
                        )?;
                        parts.push((key, node, rest.nodes[node].inverse()));
                    }
                    threads.insert(
                        asset.path.clone(),
                        ThreadRig {
                            shape: shape.clone(),
                            clips,
                            parts,
                        },
                    );
                }
                continue;
            };
            // Objects on nodes the clip moves are drawn apart and posed each
            // frame; everything else is baked once.
            let pose = bri_content::animation::sample(&shape, Some(look), 0.0)?;
            let mut moving = std::collections::BTreeSet::new();
            for t in [0.5, 1.0] {
                let other = bri_content::animation::sample(&shape, Some(look), t * look.duration)?;
                for (i, object) in shape.objects.iter().enumerate() {
                    if let Some(node) = object.node
                        && !other.nodes[node].abs_diff_eq(pose.nodes[node], 1e-5)
                    {
                        ensure!(
                            object.meshes.iter().all(|m| shape
                                .meshes
                                .get(*m)
                                .and_then(Option::as_ref)
                                .is_none_or(|m| m.skin.is_none())),
                            "Vehicle look clip moves a skinned mesh: {}",
                            asset.path
                        );
                        moving.insert((node, i));
                    }
                }
            }
            let only = |keep: &dyn Fn(usize) -> bool| {
                let mut part = bri_content::animation::sample(&shape, Some(look), 0.0)?;
                for (i, v) in part.visibility.iter_mut().enumerate() {
                    if !keep(i) {
                        *v = 0.0;
                    }
                }
                anyhow::Ok(part)
            };
            insert(
                asset.path.clone(),
                &only(&|i| !moving.iter().any(|(_, o)| *o == i))?,
            )?;
            let mut parts = Vec::new();
            let nodes: std::collections::BTreeSet<usize> = moving.iter().map(|(n, _)| *n).collect();
            for node in nodes {
                let key = format!("{}#look{node}", asset.path);
                insert(
                    key.clone(),
                    &only(&|i| shape.objects[i].node == Some(node))?,
                )?;
                parts.push((key, node, pose.nodes[node].inverse()));
            }
            looks.insert(
                asset.path.clone(),
                LookRig {
                    shape: shape.clone(),
                    clip: look.clone(),
                    parts,
                },
            );
        }
        for d in pack
            .definitions
            .iter()
            .filter(|d| !models.contains_key(&d.model))
        {
            let asset = pack.assets.iter().find(|a| a.path == d.model);
            // An Add-On model that failed to load is already logged.
            if asset.and_then(|a| a.package.as_ref()).is_some() {
                continue;
            }
            match d.id.split_once(':') {
                Some((add_on, _)) => {
                    crate::cosmetic::add_on_fault(
                        add_on,
                        &d.model,
                        "the vehicle's model is missing",
                    );
                }
                None => anyhow::bail!("Vehicle {} model is missing", d.id),
            }
        }
        let sources = pack
            .assets
            .iter()
            .filter(|a| a.kind == "model" && models.contains_key(&a.path))
            .map(|a| (a.virtual_path.to_ascii_lowercase(), a.path.clone()))
            .collect();
        // The first definition of an id wins, as the scan it replaces did.
        let mut index = std::collections::HashMap::new();
        for (i, d) in pack.definitions.iter().enumerate() {
            index.entry(d.id.clone()).or_insert(i);
        }
        Ok(Self {
            pack,
            index,
            models,
            looks,
            threads,
            sources,
        })
    }
    /// The loaded vehicle definitions, as the host's vehicle code takes them.
    pub fn pack(&self) -> &Pack {
        &self.pack
    }
    pub fn definition(&self, id: &str) -> Option<&Definition> {
        Some(&self.pack.definitions[*self.index.get(id)?])
    }
    /// The player-type mount a vehicle carries as its attachment (the
    /// Tank's `TankTurretPlayer`): the definition drawn with that model.
    pub fn attachment_definition(&self, d: &Definition) -> Option<&Definition> {
        let model = d.attachment_model.as_ref()?;
        self.pack
            .definitions
            .iter()
            .find(|a| a.is_actor() && a.model == *model)
    }
    /// Whether the pack converted the model at this source path.
    pub fn has_source_model(&self, source: &str) -> bool {
        self.sources.contains_key(&source.to_ascii_lowercase())
    }
    /// Draw the model converted from `source` this frame (after `prepare`),
    /// such as explosion debris. False when the pack lacks it.
    pub fn push_source_model(&mut self, source: &str, transform: Mat4, tint: [f32; 4]) -> bool {
        let Some(path) = self.sources.get(&source.to_ascii_lowercase()) else {
            return false;
        };
        match self.models.get_mut(path) {
            Some(model) if transform.is_finite() => {
                model.transforms.push(SceneTransform { transform, tint });
                true
            }
            _ => false,
        }
    }
}

/// Chassis-local tire transform: the hub drops by the suspension extension,
/// steering is positive to the right (clockwise from above), forward travel
/// spins the tire's top toward -Z, and the authored tire is turned axle-out.
pub fn wheel_transform(wheel: &Wheel, suspension: f32, spin: f32, steering: f32) -> Mat4 {
    Mat4::from_translation(Vec3::from(wheel.position) - Vec3::Y * suspension)
        * Mat4::from_rotation_y(-wheel.steer_angle(steering))
        * Mat4::from_rotation_x(-spin)
        * Mat4::from_quat(Quat::from_array(wheel.model_rotation))
}

fn to_transform(position: Vec3, rotation: Quat) -> Mat4 {
    Mat4::from_rotation_translation(rotation, position)
}

/// Interpolated per-vehicle state for this frame.
#[derive(Clone, Debug)]
pub struct VehicleFrame {
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub steering: f32,
    pub wheel_suspension: Vec<f32>,
    pub wheel_rotation: Vec<f32>,
    pub wheel_contact: Vec<bool>,
    pub turret_aim: [f32; 2],
}

#[derive(Default)]
pub struct ClientVehicles {
    history: BTreeMap<u64, VecDeque<VehiclePose>>,
    frames: BTreeMap<u64, VehicleFrame>,
    driven: Option<Warp>,
    /// Server ticks at the last update: the clock animation threads run on.
    clock: f64,
    /// The driven vehicle's predicted place (`set_predicted`).
    predicted: Option<(u64, Vec3, Quat)>,
}
/// How far the driven vehicle is drawn from its extrapolated newest pose:
/// a disagreeing pose shifts the path, and the difference decays instead of
/// popping (Torque's warp toward a corrected control object).
struct Warp {
    vehicle: u64,
    newest: u64,
    now: f64,
    offset: Vec3,
    turn: Quat,
}

impl ClientVehicles {
    pub fn clear(&mut self) {
        self.history.clear();
        self.frames.clear();
        self.driven = None;
    }
    pub fn frame(&self, id: u64) -> Option<&VehicleFrame> {
        self.frames.get(&id)
    }
    /// Record replicated poses and compute this frame's transforms. The
    /// vehicle the local player drives is shown at its newest pose, lightly
    /// extrapolated, so steering feels immediate; others are interpolated.
    pub fn update(
        &mut self,
        infos: &BTreeMap<u64, VehicleInfo>,
        poses: &BTreeMap<u64, VehiclePose>,
        server_tick: Option<f64>,
        driven: Option<u64>,
    ) {
        self.history.retain(|id, _| infos.contains_key(id));
        for (id, pose) in poses {
            let history = self.history.entry(*id).or_default();
            if history.back().is_none_or(|last| last.tick < pose.tick) {
                if history.len() == HISTORY {
                    history.pop_front();
                }
                history.push_back(pose.clone());
            }
        }
        self.frames.clear();
        self.clock = server_tick.unwrap_or(self.clock);
        if self
            .driven
            .as_ref()
            .is_some_and(|w| Some(w.vehicle) != driven)
        {
            self.driven = None;
        }
        for (id, history) in &self.history {
            let Some(newest) = history.back() else {
                continue;
            };
            let predicted = self.predicted.filter(|(p, ..)| p == id);
            let frame = match server_tick {
                // The vehicle this client drives, predicted: v20 runs the
                // controlled object's moves on the client too.
                _ if predicted.is_some() => {
                    let (_, position, rotation) = predicted.unwrap();
                    self.driven = None;
                    VehicleFrame {
                        position,
                        rotation,
                        ..frame_of(newest)
                    }
                }
                Some(now) if Some(*id) != driven => sample(history, now - INTERPOLATION_TICKS),
                Some(now) => {
                    let mut frame = extrapolate(history, history.len() - 1, now);
                    let warp = self.driven.get_or_insert(Warp {
                        vehicle: *id,
                        newest: newest.tick,
                        now,
                        offset: Vec3::ZERO,
                        turn: Quat::IDENTITY,
                    });
                    let seconds = ((now - warp.now).max(0.0) / TICK_RATE) as f32;
                    let decay = (-DRIVEN_CORRECTION_RATE * seconds).exp();
                    warp.offset *= decay;
                    warp.turn = Quat::IDENTITY.slerp(warp.turn, decay).normalize();
                    warp.now = now;
                    if warp.newest != newest.tick {
                        // Keep drawing where the previous pose's path is now.
                        if let Some(index) = history.iter().rposition(|p| p.tick == warp.newest) {
                            let old = extrapolate(history, index, now);
                            let offset = old.position + warp.offset - frame.position;
                            if offset.is_finite() && offset.length() <= DRIVEN_SNAP {
                                warp.offset = offset;
                                warp.turn = (warp.turn * old.rotation * frame.rotation.inverse())
                                    .normalize();
                            } else {
                                warp.offset = Vec3::ZERO;
                                warp.turn = Quat::IDENTITY;
                            }
                        }
                        warp.newest = newest.tick;
                    }
                    frame.position += warp.offset;
                    frame.rotation = (warp.turn * frame.rotation).normalize();
                    frame
                }
                None => frame_of(newest),
            };
            self.frames.insert(*id, frame);
        }
    }
    /// The driven vehicle's predicted place this frame (`Motion::driven_frame`),
    /// drawn instead of its extrapolated pose; `None` without a prediction.
    pub fn set_predicted(&mut self, predicted: Option<(u64, Vec3, Quat)>) {
        self.predicted = predicted.filter(|(_, p, r)| p.is_finite() && r.is_finite());
    }
    /// Aim the local gunner's barrel from their own look this frame, as v20
    /// clients do for the object they control, instead of waiting for the
    /// server's copy. Mirrors the host's mapping in `vehicle_input`.
    pub fn aim_locally(&mut self, vehicle: u64, d: &Definition, look_yaw: f32, look_pitch: f32) {
        let Some(frame) = self.frames.get_mut(&vehicle) else {
            return;
        };
        let yaw = if d.is_actor() {
            0.0
        } else {
            let forward = frame.rotation * Vec3::NEG_Z;
            let heading = forward.x.atan2(-forward.z);
            // Quaternion yaw turns left; look yaw turns right.
            -((look_yaw - heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI)
        };
        frame.turret_aim = [yaw, look_pitch.clamp(d.look_pitch[0], d.look_pitch[1])];
    }
    /// World transform of a seat node (players' feet ride there).
    pub fn seat(
        &self,
        assets: &VehicleAssets,
        info: &VehicleInfo,
        seat: usize,
    ) -> Option<(Vec3, f32)> {
        let (position, rotation) = self.seat_transform(assets, info, seat)?;
        let forward = rotation * Vec3::NEG_Z;
        Some((position, forward.x.atan2(-forward.z)))
    }
    /// The seat's world position and full rotation: a mounted player takes
    /// the mount node's transform, tilting with the vehicle on slopes.
    pub fn seat_transform(
        &self,
        assets: &VehicleAssets,
        info: &VehicleInfo,
        seat: usize,
    ) -> Option<(Vec3, Quat)> {
        let frame = self.frames.get(&info.id)?;
        let d = assets.definition(&info.definition)?;
        let s = d.seats.get(seat)?;
        let mut local = to_transform(
            Vec3::from(s.transform.position),
            Quat::from_array(s.transform.rotation),
        );
        if seat == 2 && d.attachment_mount.is_some() {
            let pivot = d
                .attachment_mount
                .as_ref()
                .map_or(Vec3::ZERO, |t| Vec3::from(t.position));
            local = Mat4::from_translation(pivot)
                * Mat4::from_rotation_y(frame.turret_aim[0])
                * Mat4::from_translation(-pivot)
                * local;
        }
        let world = to_transform(frame.position, frame.rotation) * local;
        let (_, rotation, position) = world.to_scale_rotation_translation();
        Some((position, rotation))
    }
    /// Refresh per-model instance lists (chassis, wheels, turrets).
    pub fn prepare(
        &mut self,
        assets: &mut VehicleAssets,
        infos: &BTreeMap<u64, VehicleInfo>,
        palette: &[[f32; 4]],
    ) {
        let VehicleAssets {
            pack,
            index,
            models,
            looks,
            threads,
            ..
        } = assets;
        for model in models.values_mut() {
            model.transforms.clear();
        }
        for (id, frame) in &self.frames {
            let Some(info) = infos.get(id) else { continue };
            let Some(d) = index.get(&info.definition).map(|i| &pack.definitions[*i]) else {
                continue;
            };
            // Horses are animated with the horse rig instead.
            if d.family == bri_vehicles::Family::Horse {
                continue;
            }
            let tint = info
                .color
                .and_then(|c| palette.get(usize::from(c)))
                .map_or([1.0; 4], |c| [c[0], c[1], c[2], 1.0]);
            let body = to_transform(frame.position, frame.rotation);
            let pitch = frame.turret_aim[1];
            let mut push = |model: &str, transform: Mat4, tint: [f32; 4]| {
                for (model, transform) in posed(looks, model, pitch, transform) {
                    if let Some(m) = models.get_mut(model)
                        && transform.is_finite()
                    {
                        m.transforms.push(SceneTransform { transform, tint });
                    }
                }
            };
            push(&d.model, body, tint);
            if let Some(rig) = threads.get(&d.model) {
                let speed = frame.velocity.length();
                for (model, transform) in
                    threaded(rig, &d.threads, speed, self.clock / TICK_RATE, body)
                {
                    push(model, transform, tint);
                }
            }
            for (i, wheel) in d.wheels.iter().enumerate() {
                let suspension = frame
                    .wheel_suspension
                    .get(i)
                    .copied()
                    .unwrap_or(wheel.rest_length);
                let spin = frame.wheel_rotation.get(i).copied().unwrap_or(0.0);
                let local = wheel_transform(wheel, suspension, spin, frame.steering);
                push(&wheel.model, body * local, [1.0; 4]);
            }
            if let (Some(model), Some(mount)) = (&d.attachment_model, &d.attachment_mount) {
                let local =
                    to_transform(Vec3::from(mount.position), Quat::from_array(mount.rotation))
                        * Mat4::from_rotation_y(frame.turret_aim[0]);
                push(model, body * local, tint);
            }
        }
    }
    pub fn upload(
        assets: &mut VehicleAssets,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        for model in assets.models.values_mut() {
            if model.transforms.is_empty() {
                if let Some(instances) = &mut model.instances {
                    instances.update(queue, &[])?;
                }
                continue;
            }
            if model.gpu.is_none() {
                model.gpu = Some(renderer.upload(device, queue, &model.data)?);
            }
            if model
                .instances
                .as_ref()
                .is_none_or(|i| i.capacity() < model.transforms.len())
            {
                model.instances = Some(GpuInstances::new(
                    device,
                    model.transforms.len().next_power_of_two().max(4),
                )?);
            }
            model
                .instances
                .as_mut()
                .unwrap()
                .update(queue, &model.transforms)?;
        }
        Ok(())
    }
    pub fn draws(assets: &VehicleAssets) -> Vec<(&GpuScene, &GpuInstances)> {
        assets
            .models
            .values()
            .filter(|m| !m.transforms.is_empty())
            .filter_map(|m| Some((m.gpu.as_ref()?, m.instances.as_ref()?)))
            .collect()
    }
    pub fn gpu_stopped(assets: &mut VehicleAssets) {
        for model in assets.models.values_mut() {
            model.gpu = None;
            model.instances = None;
        }
    }
}

fn frame_of(pose: &VehiclePose) -> VehicleFrame {
    VehicleFrame {
        position: Vec3::from(pose.position),
        rotation: Quat::from_array(pose.rotation).normalize(),
        velocity: Vec3::from(pose.velocity),
        steering: pose.steering,
        wheel_suspension: pose.wheel_suspension.clone(),
        wheel_rotation: pose.wheel_rotation.clone(),
        wheel_contact: pose.wheel_contact.clone(),
        turret_aim: pose.turret_aim,
    }
}

/// `history[index]` carried forward to `now`, briefly: by its velocity, and
/// turned on by the spin between it and the pose before. Poses carry no
/// angular velocity, and a vehicle drawn at its last rotation lags a turn
/// by the whole round trip; a plane's first-person view rides that
/// rotation, so its pitch would answer the mouse late and in steps.
fn extrapolate(history: &VecDeque<VehiclePose>, index: usize, now: f64) -> VehicleFrame {
    let pose = &history[index];
    let ahead = (now - pose.tick as f64).clamp(0.0, DRIVEN_AHEAD);
    let mut frame = frame_of(pose);
    frame.position += frame.velocity * (ahead / TICK_RATE) as f32;
    if let Some(before) = index.checked_sub(1).map(|i| &history[i]) {
        let ticks = pose.tick.saturating_sub(before.tick);
        if (1..=SPIN_WINDOW).contains(&ticks) {
            let turn =
                (frame.rotation * Quat::from_array(before.rotation).normalize().inverse()).normalize();
            // The short way round.
            let turn = if turn.w < 0.0 { -turn } else { turn };
            let spin = turn.to_scaled_axis() * (ahead / ticks as f64) as f32;
            if spin.is_finite() {
                frame.rotation = (Quat::from_scaled_axis(spin) * frame.rotation).normalize();
            }
        }
    }
    frame
}

fn sample(history: &VecDeque<VehiclePose>, tick: f64) -> VehicleFrame {
    let first = history.front().unwrap();
    if tick <= first.tick as f64 {
        return frame_of(first);
    }
    for (a, b) in history.iter().zip(history.iter().skip(1)) {
        if tick <= b.tick as f64 {
            let t = ((tick - a.tick as f64) / (b.tick - a.tick).max(1) as f64) as f32;
            let (fa, fb) = (frame_of(a), frame_of(b));
            let lerp = |x: &[f32], y: &[f32]| -> Vec<f32> {
                x.iter().zip(y).map(|(p, q)| p + (q - p) * t).collect()
            };
            return VehicleFrame {
                position: fa.position.lerp(fb.position, t),
                rotation: fa.rotation.slerp(fb.rotation, t),
                velocity: fa.velocity.lerp(fb.velocity, t),
                steering: fa.steering + (fb.steering - fa.steering) * t,
                wheel_suspension: lerp(&fa.wheel_suspension, &fb.wheel_suspension),
                // Wheel spin wraps; take the newer value rather than blending.
                wheel_rotation: fb.wheel_rotation,
                wheel_contact: fb.wheel_contact,
                turret_aim: [
                    fa.turret_aim[0] + (fb.turret_aim[0] - fa.turret_aim[0]) * t,
                    fa.turret_aim[1] + (fb.turret_aim[1] - fa.turret_aim[1]) * t,
                ],
            };
        }
    }
    let last = history.back().unwrap();
    let mut frame = frame_of(last);
    let ahead = ((tick - last.tick as f64).min(6.0) / TICK_RATE) as f32;
    frame.position += frame.velocity * ahead;
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pose(tick: u64, x: f32) -> VehiclePose {
        VehiclePose {
            id: 1,
            tick,
            position: [x, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
            steering: 0.0,
            wheel_suspension: vec![0.4],
            wheel_rotation: vec![0.0],
            wheel_contact: vec![true],
            wheel_tire: vec![Default::default()],
            turret_aim: [0.0; 2],
            jetting: false,
            angular_velocity: [0.0; 3],
            mouse_steering: [0.0; 2],
            driver_input: 0,
            driver_steering: (false, false),
            steering_quiet: 0,
            actor: None,
        }
    }
    /// v20 tires are authored with the hub axis along Torque +Y (native -Z),
    /// outer face first; mounted, they must roll along the chassis.
    #[test]
    fn tires_mount_axle_out_steer_right_and_roll_forward() {
        let tire = |x: f32| Wheel {
            position: [x, 0.4, -1.9],
            radius: 0.75,
            rest_length: 0.4,
            spring: 6000.0,
            damping: 800.0,
            anti_sway: 0.0,
            tire: Default::default(),
            steering: 1.0,
            powered: false,
            model: String::new(),
            model_rotation: Quat::from_rotation_y(-x.signum() * std::f32::consts::FRAC_PI_2)
                .to_array(),
        };
        for x in [-1.6, 1.6] {
            let rest = wheel_transform(&tire(x), 0.3, 0.0, 0.0);
            let outer = rest.transform_vector3(Vec3::NEG_Z);
            assert!(
                (outer - Vec3::X * x.signum()).length() < 1e-5,
                "outer face {outer}"
            );
            assert!((rest.transform_point3(Vec3::ZERO) - Vec3::new(x, 0.1, -1.9)).length() < 1e-5);
            let steered = wheel_transform(&tire(x), 0.3, 0.0, 0.5) * rest.inverse();
            let heading = steered.transform_vector3(Vec3::NEG_Z);
            // v20 squares the steering: 0.5 turns the wheel 0.25 right.
            assert!(
                (heading.x - 0.25f32.sin()).abs() < 1e-4 && heading.z < 0.0,
                "steer right {heading}"
            );
            let rolled = wheel_transform(&tire(x), 0.3, 0.2, 0.0).transform_vector3(Vec3::Y);
            assert!(rolled.z < -0.1, "forward spin must carry the top forward");
        }
    }
    /// A propeller whose `slow` and `fast` sequences turn it a quarter turn
    /// per frame over one second and a quarter second.
    fn propeller() -> ThreadRig {
        let quarter =
            |i: usize| Quat::from_rotation_z(i as f32 * std::f32::consts::FRAC_PI_2).to_array();
        let clip = |name: &str, duration: f32| Animation {
            name: name.into(),
            frames: 4,
            duration,
            looping: true,
            additive: false,
            priority: 0,
            nodes: vec![bri_content::shape::NodeTrack {
                node: "prop".into(),
                rotations: (0..4).map(quarter).collect(),
                translations: vec![],
                scales: vec![],
                scale_rotations: vec![],
            }],
            objects: vec![],
            ground_translations: vec![],
            ground_rotations: vec![],
            triggers: vec![],
        };
        let node = |name: &str, parent| bri_content::shape::Node {
            name: name.into(),
            parent,
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
        };
        ThreadRig {
            shape: Shape {
                schema_version: 1,
                id: "plane".into(),
                nodes: vec![node("root", None), node("prop", Some(0))],
                objects: vec![],
                details: vec![],
                meshes: vec![],
                materials: vec![],
                animations: vec![],
            },
            clips: [("slow", 1.0), ("fast", 0.25)]
                .into_iter()
                .map(|(n, d)| (n.to_string(), clip(n, d)))
                .collect(),
            parts: vec![("plane#thread1".into(), 1, Mat4::IDENTITY)],
        }
    }
    #[test]
    fn threads_pick_their_sequence_by_speed_and_play_at_their_rate() {
        use bri_vehicles::schema::AnimationThread;
        let rig = propeller();
        let thread =
            |sequence: &str, min: Option<f32>, max: Option<f32>, rate: f32| AnimationThread {
                slot: 0,
                sequence: sequence.into(),
                rate,
                min_speed: min,
                max_speed: max,
            };
        let blade = |threads: &[AnimationThread], speed: f32, seconds: f64| {
            let draws = threaded(&rig, threads, speed, seconds, Mat4::IDENTITY);
            assert_eq!(draws.len(), 1);
            let tip = draws[0].1.transform_vector3(Vec3::X);
            tip.y.atan2(tip.x).to_degrees().round()
        };
        let switch = [
            thread("slow", None, Some(5.0), 1.0),
            thread("fast", Some(5.0), None, 1.0),
        ];
        // Slow below speed 5: a quarter turn a quarter second in.
        assert_eq!(blade(&switch, 0.0, 0.25), 90.0);
        // Fast from speed 5: a whole turn by then, a quarter an eighth later.
        assert_eq!(blade(&switch, 5.0, 0.25), 0.0);
        assert_eq!(blade(&switch, 30.0, 0.3125), 90.0);
        // Twice the rate, and a negative rate turning it backwards.
        assert_eq!(blade(&[thread("slow", None, None, 2.0)], 0.0, 0.125), 90.0);
        assert_eq!(blade(&[thread("slow", None, None, -1.0)], 0.0, 0.25), -90.0);
        // Hours in, the phase is still exact.
        assert_eq!(blade(&switch, 0.0, 36_000.25), 90.0);
        // No thread matches: the blade rests.
        assert_eq!(
            blade(&[thread("fast", Some(5.0), None, 1.0)], 0.0, 0.25),
            0.0
        );
    }
    #[test]
    #[ignore = "requires the converted native vehicle pack; CPU only"]
    fn gunner_barrels_follow_the_pitch_to_the_muzzle() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/vehicles-pack-012");
        let assets = VehicleAssets::load(&root)?;
        for id in ["v20.vehicle.tankvehicle", "v20.vehicle.cannonturret"] {
            let d = assets.definition(id).unwrap().clone();
            let model = d.attachment_model.clone().unwrap_or(d.model.clone());
            let rig = &assets.looks[&model];
            assert_eq!(rig.parts.len(), 1, "{id} draws its barrel apart");
            let muzzle = rig
                .shape
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(d.muzzle_node()))
                .unwrap();
            let rest = bri_content::animation::sample(&rig.shape, Some(&rig.clip), 0.0)?;
            let rest = rest.nodes[muzzle].w_axis.truncate();
            let (mount, turn) = d
                .attachment_mount
                .as_ref()
                .map_or((Vec3::ZERO, Quat::IDENTITY), |t| {
                    (Vec3::from(t.position), Quat::from_array(t.rotation))
                });
            let mut last = None;
            for step in 0..=40 {
                let pitch =
                    d.look_pitch[0] + (d.look_pitch[1] - d.look_pitch[0]) * step as f32 / 40.0;
                let draws = posed(&assets.looks, &model, pitch, Mat4::IDENTITY);
                assert_eq!(draws.len(), 2);
                let barrel = draws[1].1.transform_point3(rest);
                let (origin, _) = d.muzzle([0.0, pitch]).unwrap();
                let expected = turn.inverse() * (origin - mount);
                assert!(
                    barrel.distance(expected) < 0.01,
                    "{id}: drawn barrel mouth {barrel} vs shot origin {expected} at {pitch}"
                );
                // Every pitch step moves the barrel: no snapping between poses.
                if let Some(last) = last {
                    assert!(
                        barrel.distance(last) > 1e-4,
                        "{id}: barrel stuck at {pitch}"
                    );
                }
                last = Some(barrel);
            }
        }
        for id in ["v20.vehicle.jeepvehicle", "v20.vehicle.horsearmor"] {
            if let Some(d) = assets.definition(id) {
                assert!(!assets.looks.contains_key(&d.model), "{id}");
            }
        }
        Ok(())
    }
    #[test]
    fn vehicle_samples_interpolate_between_poses() {
        let history: VecDeque<_> = [pose(10, 0.0), pose(13, 3.0)].into();
        assert!((sample(&history, 11.5).position.x - 1.5).abs() < 1e-5);
        assert_eq!(sample(&history, 0.0).position.x, 0.0);
    }
    #[test]
    fn a_driven_vehicle_warps_onto_a_corrected_pose() {
        let infos = BTreeMap::from([(
            1,
            VehicleInfo {
                id: 1,
                definition: String::new(),
                color: None,
                occupants: vec![],
                destroyed: false,
                scale: 1.0,
            },
        )]);
        let mut vehicles = ClientVehicles::default();
        let moving = VehiclePose {
            velocity: [12.0, 0.0, 0.0],
            ..pose(0, 0.0)
        };
        vehicles.update(&infos, &BTreeMap::from([(1, moving)]), Some(3.0), Some(1));
        let before = vehicles.frame(1).unwrap().position;
        assert!((before.x - 0.3).abs() < 1e-5);
        // The host says it stopped at 0.1: no pop, then it settles there.
        let stopped = BTreeMap::from([(1, pose(3, 0.1))]);
        vehicles.update(&infos, &stopped, Some(3.0), Some(1));
        assert!((vehicles.frame(1).unwrap().position - before).length() < 1e-5);
        for frame in 1..=60 {
            vehicles.update(&infos, &stopped, Some(3.0 + frame as f64 * 2.0), Some(1));
        }
        assert!((vehicles.frame(1).unwrap().position.x - 0.1).abs() < 0.01);
    }
}
