//! Vehicle presentation: native vehicle models drawn as GPU instances at
//! interpolated authoritative transforms, with wheels, steering, suspension
//! and turrets, plus seat transforms for riders and the driving camera.
use crate::items::native_shape_scene;
use crate::portal_view::Straddle;
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_content::passage::Passages;
use bri_content::shape::{Animation, Shape};
use bri_render::scene::{
    ClipPlane, GpuInstances, GpuScene, KEEP_ALL, SceneImage, SceneRenderer, SceneTransform,
};
use bri_sim::session::{VehicleInfo, VehiclePose};
use bri_vehicles::{Definition, Pack, schema::Wheel};
use glam::{Mat4, Quat, Vec3};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
};

const HISTORY: usize = 16;
/// Render other vehicles this many server ticks behind the newest pose.
pub(crate) const INTERPOLATION_TICKS: f64 = 9.0;
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
    /// The middle of its metal surfaces, in the model's frame, when it has
    /// any: where the environment probe sits to reflect around it.
    metal: Option<Vec3>,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transforms: Vec<SceneTransform>,
    /// Each transform's cut (`crate::portal_view::Straddle`).
    clips: Vec<ClipPlane>,
}

pub struct VehicleAssets {
    pack: Pack,
    /// Each definition's place in `pack.definitions`, by id.
    index: std::collections::HashMap<String, usize>,
    models: BTreeMap<String, Model>,
    weapon_nodes: BTreeMap<String, BTreeMap<String, Mat4>>,
    /// Gunner models with a `look` clip (tank turret, pirate cannon), keyed
    /// by the model's asset path.
    looks: BTreeMap<String, LookRig>,
    /// Models whose definitions play animation threads (a propeller), keyed
    /// by the model's asset path.
    threads: BTreeMap<String, ThreadRig>,
    /// Independently posed geometry; immutable textures remain on the model.
    skinned: BTreeMap<(String, u64), SkinnedModel>,
    /// Model asset paths by lower-case source path (`add-ons/vehicle_jeep/jeeptire.dts`).
    sources: BTreeMap<String, String>,
    /// Each definition's lamps (`Definition::lights`) where its model has
    /// their nodes: body-local position, colour and radius.
    lamps: BTreeMap<String, Vec<BoundLamp>>,
    /// Each ramped lamp's current light, 0 to 1, by vehicle and lamp, and
    /// when they were last stepped.
    lamp_levels: BTreeMap<(u64, usize), f32>,
    lamp_clock: Option<std::time::Instant>,
}
struct BoundLamp {
    transform: Mat4,
    lamp: bri_vehicles::schema::Lamp,
}

/// A gunner model split into its fixed part and the parts its `look` clip
/// moves (the barrel), so the barrel can follow any pitch exactly.
struct LookRig {
    shape: Shape,
    clip: Animation,
    /// Model key, node, and the inverse of the node's transform in that model.
    parts: Vec<(String, usize, Mat4)>,
    skinned: bool,
}

struct SkinnedModel {
    data: bri_render::scene::SceneData,
    layout: Option<crate::avatar_mesh::Layout>,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transforms: Vec<SceneTransform>,
    clips: Vec<ClipPlane>,
    topology_dirty: bool,
    faulted: bool,
}

impl SkinnedModel {
    fn pose(&mut self, rig: &LookRig, phase: f32) -> Result<()> {
        let pose =
            bri_content::animation::sample(&rig.shape, Some(&rig.clip), phase * rig.clip.duration)?;
        let detail = rig
            .shape
            .details
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.collision)
            .max_by(|(_, a), (_, b)| a.pixel_threshold.total_cmp(&b.pixel_threshold))
            .map(|(i, _)| i)
            .context("Animated vehicle has no visible detail")?;
        let materials: Vec<_> = (0..rig.shape.materials.len()).collect();
        let colors = vec![Some([1.; 4]); rig.shape.objects.len()];
        self.topology_dirty |= crate::avatar_mesh::Layout::pose(
            &mut self.layout,
            &mut self.data,
            &crate::avatar_mesh::Binding {
                shape: &rig.shape,
                detail,
                materials: &materials,
                translucent_materials: &materials,
                unassigned_material: materials.len(),
                colors: &colors,
            },
            &pose,
            Mat4::IDENTITY,
        )?;
        Ok(())
    }
}

/// Borrow the immutable model resources for an upload without copying images.
fn swap_geometry(a: &mut bri_render::scene::SceneData, b: &mut bri_render::scene::SceneData) {
    std::mem::swap(&mut a.vertices, &mut b.vertices);
    std::mem::swap(&mut a.indices, &mut b.indices);
    std::mem::swap(&mut a.batches, &mut b.batches);
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

/// A pitch across `range` (low, high radians, up positive): 0 at the
/// highest, 1 at the lowest, as v20's `look` clip runs.
pub fn seat_aim_phase(range: [f32; 2], pitch: f32) -> f32 {
    let span = range[1] - range[0];
    if span.is_nan() || span <= 1e-4 || !pitch.is_finite() {
        return 0.5;
    }
    ((range[1] - pitch) / span).clamped(0.0, 1.0)
}

/// Draws of a model at `transform`: the model itself plus, for a gunner
/// model, its barrel parts posed by the `look` clip at this pitch.
fn posed<'a>(
    looks: &'a BTreeMap<String, LookRig>,
    model: &'a str,
    phase: f32,
    transform: Mat4,
) -> Vec<(&'a str, Mat4)> {
    let mut out = vec![(model, transform)];
    if let Some(rig) = looks.get(model) {
        if rig.skinned {
            return Vec::new();
        }
        let time = phase * rig.clip.duration;
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
        let mut weapon_nodes: BTreeMap<String, BTreeMap<String, Mat4>> = BTreeMap::new();
        let mut lamps: BTreeMap<String, Vec<BoundLamp>> = BTreeMap::new();
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
                let mut shape: Shape = serde_json::from_slice(&bytes)?;
                shape.validate()?;
                shape.expand_live_frames();
                Ok(shape)
            });
            let shape = match shape {
                Ok(shape) => shape,
                Err(error) => {
                    fault(asset, error)?;
                    continue;
                }
            };
            if gunner_models.contains(&asset.path) {
                let rest = bri_content::animation::sample(&shape, None, 0.0)?;
                weapon_nodes.insert(
                    asset.path.clone(),
                    shape
                        .nodes
                        .iter()
                        .enumerate()
                        .map(|(i, node)| (node.name.to_ascii_lowercase(), rest.nodes[i]))
                        .collect(),
                );
            }
            let folder = asset
                .virtual_path
                .rsplit_once('/')
                .map_or(String::new(), |(dir, _)| dir.to_ascii_lowercase());
            // A material whose texture is nowhere draws as blank paint.
            let images: Vec<SceneImage> = shape
                .materials
                .iter()
                .map(|m| {
                    material_texture(&textures, &folder, &m.name)
                        .cloned()
                        .unwrap_or_else(|| blank.clone())
                })
                .collect();
            let refs: Vec<&SceneImage> = images.iter().collect();
            for d in pack
                .definitions
                .iter()
                .filter(|d| d.model == asset.path && !d.lights.is_empty())
            {
                let rest = bri_content::animation::sample(&shape, None, 0.0)?;
                for lamp in &d.lights {
                    let Some(node) = shape
                        .nodes
                        .iter()
                        .position(|n| n.name.eq_ignore_ascii_case(&lamp.node))
                    else {
                        bri_console::warn(format!("{}: no node {} for its light", d.id, lamp.node));
                        continue;
                    };
                    let transform =
                        rest.nodes[node] * Mat4::from_translation(Vec3::from(lamp.offset));
                    lamps.entry(d.id.clone()).or_default().push(BoundLamp {
                        transform,
                        lamp: lamp.clone(),
                    });
                }
            }
            let look = gunner_models
                .contains(&asset.path)
                .then(|| bri_vehicles::muzzle::look_clip(&look_clips, &shape))
                .flatten();
            let mut insert = |key: String, pose: &bri_content::animation::Pose| -> Result<()> {
                let data =
                    native_shape_scene(&key, &shape, &refs, [1.0; 4], false, Mat4::IDENTITY, pose)?;
                let metal = metal_centre(&data);
                models.insert(
                    key,
                    Model {
                        metal,
                        data,
                        gpu: None,
                        instances: None,
                        transforms: Vec::new(),
                        clips: Vec::new(),
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
            if shape.meshes.iter().flatten().any(|m| m.skin.is_some()) {
                insert(asset.path.clone(), &pose)?;
                looks.insert(
                    asset.path.clone(),
                    LookRig {
                        shape: shape.clone(),
                        clip: look.clone(),
                        parts: vec![],
                        skinned: true,
                    },
                );
                continue;
            }
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
                    skinned: false,
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
        for definition in &pack.definitions {
            for seat in &definition.seats {
                for node in [seat.hands.node(0), seat.hands.node(1)]
                    .into_iter()
                    .flatten()
                {
                    let found = definition
                        .attachment_model
                        .as_ref()
                        .and_then(|model| weapon_nodes.get(model))
                        .is_some_and(|nodes| nodes.contains_key(&node.to_ascii_lowercase()));
                    if !found {
                        crate::cosmetic::add_on_fault(
                            definition
                                .id
                                .split_once(':')
                                .map_or(definition.id.as_str(), |(id, _)| id),
                            "vehicles.json",
                            format!("{}: weapon hand node {node} is missing", definition.id),
                        );
                    }
                }
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
            weapon_nodes,
            looks,
            threads,
            skinned: BTreeMap::new(),
            sources,
            lamps,
            lamp_levels: BTreeMap::new(),
            lamp_clock: None,
        })
    }
    /// Lights the lamps of the vehicles whose drivers have them on cast,
    /// from each vehicle's presented body.
    pub fn lamp_lights(
        &mut self,
        infos: impl IntoIterator<Item = (u64, Mat4, bool, bool, String)>,
    ) -> Vec<bri_render::scene::PointLight> {
        let now = std::time::Instant::now();
        let dt = self
            .lamp_clock
            .replace(now)
            .map_or(0., |before| (now - before).as_secs_f32().min(0.25));
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for (id, body, on, driver, definition) in infos {
            for (slot, bound) in self
                .lamps
                .get(&definition)
                .into_iter()
                .flatten()
                .enumerate()
            {
                let lamp = &bound.lamp;
                if lamp.radius <= 0. {
                    continue;
                }
                let wanted = lamp.activation.enabled(on, driver);
                // A lamp with a ramp fades toward its state; without one it
                // switches at once.
                let level = match lamp.ramp {
                    None => f32::from(u8::from(wanted)),
                    Some(ramp) => {
                        let level = self.lamp_levels.entry((id, slot)).or_insert(0.);
                        let seconds = if wanted { ramp.up } else { ramp.down };
                        let step = if seconds > 0. { dt / seconds } else { 1. };
                        *level = if wanted {
                            (*level + step).min(1.)
                        } else {
                            (*level - step).max(0.)
                        };
                        seen.insert((id, slot));
                        *level
                    }
                };
                if level <= 0. {
                    continue;
                }
                let at = body * bound.transform;
                let cone = lamp.cone.map_or([0.; 4], |c| {
                    let axis = at.transform_vector3(Vec3::from(c.axis)).normalize_or_zero();
                    if axis == Vec3::ZERO {
                        [0.; 4]
                    } else {
                        bri_render::scene::spot_words(axis, c.inner, c.outer)
                    }
                });
                out.push(bri_render::scene::PointLight {
                    position_radius: at.w_axis.truncate().extend(lamp.radius).to_array(),
                    color: [
                        lamp.color[0] * level,
                        lamp.color[1] * level,
                        lamp.color[2] * level,
                        0.,
                    ],
                    cone,
                });
            }
        }
        self.lamp_levels.retain(|key, _| seen.contains(key));
        out
    }
    pub fn driver_present(&self, info: &VehicleInfo) -> bool {
        self.definition(&info.definition)
            .and_then(Definition::control_seat)
            .and_then(|seat| info.occupants.get(seat))
            .is_some_and(Option::is_some)
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
    /// The middles of the metal vehicles drawn this frame (after
    /// `prepare`), for the environment probe.
    pub fn metal_centres(&self) -> Vec<Vec3> {
        self.models
            .values()
            .filter_map(|m| Some((m.metal?, &m.transforms)))
            .chain(
                self.skinned
                    .values()
                    .filter_map(|m| Some((metal_centre(&m.data)?, &m.transforms))),
            )
            .flat_map(|(centre, transforms)| {
                transforms
                    .iter()
                    .map(move |t| t.transform.transform_point3(centre))
            })
            .collect()
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
                model.clips.push(KEEP_ALL);
                true
            }
            _ => false,
        }
    }
}

/// What Torque appends to a material's name when it looks for its bitmap;
/// the name as written comes first.
const TEXTURE_EXTENSIONS: [&str; 4] = ["", ".png", ".jpg", ".jpeg"];

/// The texture a DTS material names (`textures` by lower-case virtual
/// path): beside the model, then anywhere in the pack, trying the name as
/// written and then with each image extension. Some stock vehicles name
/// theirs with the extension (the Tank turret's `black.png`, the Pirate
/// Cannon's `Gray.png`); those must not miss and draw as blank paint.
fn material_texture<'a>(
    textures: &'a BTreeMap<String, SceneImage>,
    folder: &str,
    material: &str,
) -> Option<&'a SceneImage> {
    let name = material.to_ascii_lowercase();
    let files = || TEXTURE_EXTENSIONS.iter().map(|ext| format!("{name}{ext}"));
    files()
        .find_map(|file| textures.get(&format!("{folder}/{file}")))
        .or_else(|| {
            files().find_map(|file| {
                let suffix = format!("/{file}");
                textures
                    .iter()
                    .find(|(path, _)| path.ends_with(&suffix))
                    .map(|(_, image)| image)
            })
        })
}

/// The middle of a model's metal surfaces' bounds, if it has any.
fn metal_centre(data: &bri_render::scene::SceneData) -> Option<Vec3> {
    let mut bounds: Option<(Vec3, Vec3)> = None;
    for batch in &data.batches {
        if data.materials[batch.material].kind != bri_render::scene::MaterialKind::Metal {
            continue;
        }
        for index in &data.indices[batch.indices.start as usize..batch.indices.end as usize] {
            let p = Vec3::from(data.vertices[*index as usize].position);
            bounds = Some(bounds.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))));
        }
    }
    bounds.map(|(lo, hi)| (lo + hi) * 0.5)
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

/// Models without a barrel look clip aim the complete attachment. Models
/// with one keep their fixed base level and pose the barrel separately.
fn weapon_attachment(
    d: &Definition,
    frame: &VehicleFrame,
    has_look: bool,
    sway: Option<Quat>,
) -> Option<Mat4> {
    let mount = d.attachment_mount.as_ref()?;
    let mut local = to_transform(Vec3::from(mount.position), Quat::from_array(mount.rotation))
        * Mat4::from_rotation_y(frame.turret_aim[0]);
    if !has_look {
        local *=
            Mat4::from_rotation_x(frame.turret_aim[1].clamped(d.look_pitch[0], d.look_pitch[1]));
    }
    if let Some(sway) = sway {
        local *= Mat4::from_quat(sway);
    }
    Some(local)
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
    /// The openings vehicles pass through (`set_passages`).
    passages: Passages,
    /// The vehicles drawn part way through an opening this frame.
    straddles: BTreeMap<u64, Straddle>,
    /// Each vehicle's middle in its own frame, the point openings carry it
    /// by (learned in `prepare`, which knows its definition).
    middles: BTreeMap<u64, Vec3>,
    /// Each swaying turret's spring (`Definition::turret_sway`).
    sways: BTreeMap<u64, TurretSpring>,
    /// Bodies as they collide ([`bri_vehicles::body_shape`]), by definition
    /// and scale: what shoves this client's debris.
    shapes: std::sync::Mutex<BodyShapes>,
}
/// A turret's sway: its tip (pitch about the vehicle's right, roll about
/// its forward, radians) and how fast each changes, with the vehicle's
/// velocity and clock when last stepped.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TurretSpring {
    pub tip: [f32; 2],
    rate: [f32; 2],
    last: Option<(f64, Vec3)>,
}
impl TurretSpring {
    /// Step to `clock` (seconds) with the vehicle at `velocity` (world)
    /// turned by `rotation` (the complete frame in which the tip is applied).
    pub fn step(
        &mut self,
        sway: &bri_vehicles::schema::TurretSway,
        clock: f64,
        velocity: Vec3,
        rotation: Quat,
    ) {
        let Some((then, before)) = self.last.replace((clock, velocity)) else {
            return;
        };
        let dt = (clock - then) as f32;
        if dt.is_nan() || dt <= 1e-4 || !velocity.is_finite() {
            return;
        }
        let dt = dt.min(0.1);
        // Acceleration in the attachment's frame (x right, y up, z back).
        let accel = rotation.inverse() * (velocity - before) / dt;
        // Speeding up (toward -z) tips the turret back: up about +x; a turn
        // to the right (+x) leans it left; a bump up presses it down.
        let push = [
            (-accel.z - accel.y) * sway.response,
            accel.x * sway.response,
        ];
        let w = std::f32::consts::TAU * sway.frequency;
        let max = sway.max_degrees.to_radians();
        // Small sub-steps keep a stiff spring stable at low frame rates.
        let steps = (dt * w / 0.2).ceil().clamped(1.0, 16.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            for ((tip, rate), push) in self.tip.iter_mut().zip(&mut self.rate).zip(push) {
                *rate += (w * w * (push - *tip) - 2.0 * sway.damping * w * *rate) * h;
                *tip = (*tip + *rate * h).clamped(-max, max);
            }
        }
        if !self.tip.iter().chain(&self.rate).all(|v| v.is_finite()) {
            *self = Self::default();
        }
    }
    /// The tip as a rotation in the turret's frame.
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_x(self.tip[0]) * Quat::from_rotation_z(self.tip[1])
    }
}
type BodyShapes = BTreeMap<(String, u32), Option<(rapier3d::prelude::SharedShape, Vec3)>>;
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
    /// Mounted light effects share the exact vehicle-node frame in every view.
    pub fn lamp_effects(
        &self,
        assets: &VehicleAssets,
        infos: &BTreeMap<u64, VehicleInfo>,
    ) -> Vec<crate::weapon_effects::AttachedLight> {
        let mut out = Vec::new();
        for (id, info) in infos {
            if info.destroyed {
                continue;
            }
            let Some(frame) = self.frame(*id) else {
                continue;
            };
            let body = Mat4::from_scale_rotation_translation(
                Vec3::splat(info.scale),
                frame.rotation,
                frame.position,
            );
            for (slot, bound) in assets
                .lamps
                .get(&info.definition)
                .into_iter()
                .flatten()
                .enumerate()
            {
                let Some(effect) = &bound.lamp.effect else {
                    continue;
                };
                if !bound
                    .lamp
                    .activation
                    .enabled(info.lights, assets.driver_present(info))
                {
                    continue;
                }
                let (_, rotation, position) =
                    (body * bound.transform).to_scale_rotation_translation();
                out.push(crate::weapon_effects::AttachedLight {
                    owner: *id,
                    slot: slot as u16,
                    definition: effect.clone(),
                    transform: bri_fx_runtime::SourceTransform {
                        position,
                        rotation: rotation * Quat::from_rotation_arc(Vec3::Y, Vec3::NEG_Z),
                        velocity: frame.velocity,
                    },
                });
            }
        }
        out
    }
    /// The openings of linked bricks: a vehicle part way through one draws
    /// on both sides of it.
    pub fn set_passages(&mut self, passages: &Passages) {
        if self.passages.list != passages.list {
            self.passages = passages.clone();
        }
    }
    /// The opening vehicle `id` is drawn part way through this frame (after
    /// `prepare`): its riders draw cut there too.
    pub fn straddle(&self, id: u64) -> Option<&Straddle> {
        self.straddles.get(&id)
    }
    pub fn clear(&mut self) {
        self.history.clear();
        self.frames.clear();
        self.driven = None;
    }
    pub fn frame(&self, id: u64) -> Option<&VehicleFrame> {
        self.frames.get(&id)
    }
    /// Vehicle `info` as drawn this frame, as what shoves client-only
    /// bodies (debris, Add-On bodies): its body as it collides on the host,
    /// at its scale, so a giant Steel Ball rolls debris aside as a ball
    /// that size. Its id has the top bit set.
    pub fn pusher(
        &self,
        info: &VehicleInfo,
        d: &Definition,
    ) -> Option<crate::local_physics::Pusher> {
        let frame = self.frames.get(&info.id)?;
        let scale = info.scale;
        let (min, max) = (
            Vec3::from(d.bounds_min) * scale,
            Vec3::from(d.bounds_max) * scale,
        );
        let shape = self
            .shapes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry((d.id.clone(), scale.to_bits()))
            .or_insert_with(|| {
                bri_vehicles::body_shape(d, scale)
                    .ok()
                    .map(|(shape, offset)| (shape, Vec3::from_array(offset.translation.to_array())))
            })
            .clone();
        Some(match shape {
            Some((shape, offset)) => crate::local_physics::Pusher {
                id: info.id | 1 << 63,
                center: frame.position + frame.rotation * offset,
                rotation: frame.rotation,
                half: (max - min).abs() * 0.5,
                shape: Some(shape),
            },
            None => crate::local_physics::Pusher {
                id: info.id | 1 << 63,
                center: frame.position + frame.rotation * ((min + max) * 0.5),
                rotation: frame.rotation,
                half: (max - min).abs() * 0.5,
                shape: None,
            },
        })
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
        passages: &bri_content::passage::Passages,
    ) {
        self.history.retain(|id, _| infos.contains_key(id));
        self.middles.retain(|id, _| infos.contains_key(id));
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
                Some(now) if Some(*id) != driven => {
                    let middle = self.middles.get(id).copied().unwrap_or(Vec3::ZERO);
                    sample(history, now - INTERPOLATION_TICKS, passages, middle)
                }
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
        frame.turret_aim = [yaw, look_pitch.clamped(d.look_pitch[0], d.look_pitch[1])];
    }
    /// An attachment socket follows the same yaw, pitch and sway as its geometry.
    pub fn weapon_node(
        &self,
        assets: &VehicleAssets,
        info: &VehicleInfo,
        node: &str,
    ) -> Option<Mat4> {
        let d = assets.definition(&info.definition)?;
        let frame = self.frame(info.id)?;
        let model = d.attachment_model.as_ref()?;
        let local_node = if let Some(rig) = assets.looks.get(model) {
            let pose = bri_content::animation::sample(
                &rig.shape,
                Some(&rig.clip),
                d.look_phase(frame.turret_aim[1]) * rig.clip.duration,
            )
            .ok()?;
            let index = rig
                .shape
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(node))?;
            pose.nodes[index]
        } else {
            *assets
                .weapon_nodes
                .get(model)?
                .get(&node.to_ascii_lowercase())?
        };
        let sway = self.sways.get(&info.id).map(|spring| spring.rotation());
        Some(
            Mat4::from_scale_rotation_translation(
                Vec3::splat(info.scale),
                frame.rotation,
                frame.position,
            ) * weapon_attachment(d, frame, assets.looks.contains_key(model), sway)?
                * local_node,
        )
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
        // A broken turret's gunner sits where the host put them, on the
        // hull's fallback seat (`effective_seat_pose`), not on the turret.
        let fallback = d
            .attachment_fallback_seat
            .as_ref()
            .filter(|_| info.turret_broken && d.weapon_seat() == Some(seat));
        let place = fallback.unwrap_or(&s.transform);
        let mut local = to_transform(Vec3::from(place.position), Quat::from_array(place.rotation));
        if fallback.is_none() && d.weapon_seat() == Some(seat) && d.attachment_mount.is_some() {
            let pivot = d
                .attachment_mount
                .as_ref()
                .map_or(Vec3::ZERO, |t| Vec3::from(t.position));
            local = Mat4::from_translation(pivot)
                * Mat4::from_rotation_y(frame.turret_aim[0])
                * Mat4::from_translation(-pivot)
                * local;
        }
        // Match authoritative effective_seat_pose: scale the entire local
        // translation after turret-pivot rotation, leaving orientation unit.
        local.w_axis = (local.w_axis.truncate() * info.scale).extend(1.);
        let world = to_transform(frame.position, frame.rotation) * local;
        let (_, rotation, position) = world.to_scale_rotation_translation();
        Some((position, rotation))
    }
    /// Explicit fallback hip through the actual native seat frame before any
    /// passenger turn. Animated mount nodes already include vehicle/node scale;
    /// static seat offsets use vehicle scale exactly once, like seat_eye.
    pub fn seat_fallback_hip(
        &self,
        assets: &VehicleAssets,
        info: &VehicleInfo,
        seat: usize,
        animated_mount: Option<Mat4>,
    ) -> Option<Vec3> {
        if info.destroyed || info.turret_broken {
            return None;
        }
        let s = assets.definition(&info.definition)?.seats.get(seat)?;
        s.rider_pose.as_ref()?;
        let point = Vec3::from(s.rider_fallback_hip?);
        if !point.is_finite() || point.abs().max_element() > 64. {
            return None;
        }
        let frame = match animated_mount {
            Some(frame) => frame,
            None => {
                if !info.scale.is_finite() || info.scale <= 0. {
                    return None;
                }
                let (position, rotation) = self.seat_transform(assets, info, seat)?;
                Mat4::from_scale_rotation_translation(Vec3::splat(info.scale), rotation, position)
            }
        };
        let target = frame.transform_point3(point);
        (frame.is_finite()
            && frame.determinant().is_finite()
            && frame.determinant().abs() > f32::EPSILON
            && target.is_finite())
        .then_some(target)
    }
    /// An authored seat eye, shared with the host's interaction origin.
    pub fn seat_eye(
        &self,
        assets: &VehicleAssets,
        info: &VehicleInfo,
        seat: usize,
    ) -> Option<Vec3> {
        let offset = Vec3::from(assets.definition(&info.definition)?.seats.get(seat)?.eye?);
        let (position, rotation) = self.seat_transform(assets, info, seat)?;
        Some(position + rotation * offset * info.scale)
    }
    /// Where seat `seat`'s rider holds on in the world, right hand then
    /// left (`Seat::hands`), turned with the turret and gun as drawn.
    /// None for a hand the seat leaves free, and for both while a broken
    /// turret's gunner sits on the hull's fallback seat.
    pub fn hand_holds(
        &self,
        assets: &VehicleAssets,
        info: &VehicleInfo,
        seat: usize,
    ) -> [Option<Vec3>; 2] {
        let Some((frame, d)) = self
            .frames
            .get(&info.id)
            .zip(assets.definition(&info.definition))
        else {
            return [None; 2];
        };
        if info.turret_broken && d.weapon_seat() == Some(seat) {
            return [None; 2];
        }
        let body = Mat4::from_scale_rotation_translation(
            Vec3::splat(info.scale),
            frame.rotation,
            frame.position,
        );
        [0, 1].map(|hand| {
            if let Some(node) = d.seats.get(seat)?.hands.node(hand) {
                return self
                    .weapon_node(assets, info, node)
                    .map(|m| m.w_axis.truncate());
            }
            d.hand_hold(seat, hand, frame.turret_aim)
                .map(|at| body.transform_point3(at))
        })
    }
    /// How far seat `seat`'s gun aims across its pitch range as drawn, 0 at
    /// its highest to 1 at its lowest (`Definition::look_pitch`), for a
    /// gunner's body to bend with (`AvatarAnimationInput::seat_aim`). None
    /// for any other seat, and on a broken turret.
    pub fn seat_aim(&self, assets: &VehicleAssets, info: &VehicleInfo, seat: usize) -> Option<f32> {
        let frame = self.frames.get(&info.id)?;
        let d = assets.definition(&info.definition)?;
        if d.weapon_seat() != Some(seat) || info.turret_broken || d.is_actor() {
            return None;
        }
        Some(seat_aim_phase(d.look_pitch, frame.turret_aim[1]))
    }
    /// Refresh per-model instance lists (chassis, wheels, turrets).
    pub fn prepare(&mut self, assets: &mut VehicleAssets, infos: &BTreeMap<u64, VehicleInfo>) {
        let VehicleAssets {
            pack,
            index,
            models,
            looks,
            threads,
            skinned,
            ..
        } = assets;
        skinned.retain(|(model, id), _| {
            looks.contains_key(model)
                && infos
                    .get(id)
                    .and_then(|info| index.get(&info.definition))
                    .map(|i| &pack.definitions[*i])
                    .is_some_and(|d| {
                        d.model == *model || d.attachment_model.as_ref() == Some(model)
                    })
        });
        for model in skinned.values_mut() {
            model.transforms.clear();
            model.clips.clear();
        }
        for model in models.values_mut() {
            model.transforms.clear();
            model.clips.clear();
        }
        self.straddles.clear();
        self.sways.retain(|id, _| infos.contains_key(id));
        for (id, frame) in &self.frames {
            let Some(info) = infos.get(id) else { continue };
            let Some(d) = index.get(&info.definition).map(|i| &pack.definitions[*i]) else {
                continue;
            };
            let body = to_transform(frame.position, frame.rotation);
            let drawn_body = body * Mat4::from_scale(Vec3::splat(info.scale));
            // Openings carry a vehicle by its centre of mass, as the host
            // does; part way through one it draws on both sides, cut there.
            let (low, high) = (Vec3::from(d.bounds_min), Vec3::from(d.bounds_max));
            let centre = if d.is_actor() {
                Vec3::Y * (bri_vehicles::actor_tuning(d, info.scale).stand_height * 0.5)
            } else {
                Vec3::from(d.mass_center) * info.scale
            };
            let reach = (centre - low * info.scale)
                .abs()
                .max((high * info.scale - centre).abs())
                .length()
                * 2.0;
            self.middles.insert(*id, centre);
            let middle = body.transform_point3(centre);
            let straddle = Straddle::find(&self.passages, middle, reach);
            if let Some(straddle) = straddle {
                self.straddles.insert(*id, straddle);
            }
            // Horses use their animated rig, but it and their riders need
            // the same travel middle and clips as every other mount.
            if d.family == bri_vehicles::Family::Horse {
                continue;
            }
            let tint = body_tint(d, info);
            let phase = d.look_phase(frame.turret_aim[1]);
            let mut push = |model: &str, transform: Mat4, tint: [f32; 4]| {
                if let Some(rig) = looks.get(model).filter(|r| r.skinned) {
                    let Some(base) = models.get(model) else {
                        return;
                    };
                    if !transform.is_finite() {
                        return;
                    }
                    let slot =
                        skinned
                            .entry((model.to_string(), *id))
                            .or_insert_with(|| SkinnedModel {
                                data: bri_render::scene::SceneData {
                                    materials: base.data.materials.clone(),
                                    images: vec![],
                                    ..Default::default()
                                },
                                layout: None,
                                gpu: None,
                                instances: None,
                                transforms: vec![],
                                clips: vec![],
                                topology_dirty: true,
                                faulted: false,
                            });
                    if let Err(error) = slot.pose(rig, phase) {
                        if !slot.faulted {
                            bri_console::warn(format!("Vehicle animation {model}: {error:#}"));
                            slot.faulted = true;
                        }
                        return;
                    }
                    slot.transforms.push(SceneTransform { transform, tint });
                    if let Some(s) = &straddle {
                        slot.transforms.push(SceneTransform {
                            transform: s.carried(transform),
                            tint,
                        });
                        slot.clips.extend([s.near, s.far]);
                    } else {
                        slot.clips.push(KEEP_ALL);
                    }
                    return;
                }
                for (model, transform) in posed(looks, model, phase, transform) {
                    if let Some(m) = models.get_mut(model)
                        && transform.is_finite()
                    {
                        m.transforms.push(SceneTransform { transform, tint });
                        match &straddle {
                            Some(s) => {
                                let transform = s.carried(transform);
                                m.transforms.push(SceneTransform { transform, tint });
                                m.clips.extend([s.near, s.far]);
                            }
                            None => m.clips.push(KEEP_ALL),
                        }
                    }
                }
            };
            push(&d.model, drawn_body, tint);
            if let Some(rig) = threads.get(&d.model) {
                let speed = frame.velocity.length();
                for (model, transform) in
                    threaded(rig, &d.threads, speed, self.clock / TICK_RATE, drawn_body)
                {
                    push(model, transform, tint);
                }
            }
            // A wreck's tires are gone (`emptyTire`): it rests on its body.
            for (i, wheel) in d.wheels.iter().enumerate().filter(|_| !info.destroyed) {
                let suspension = frame
                    .wheel_suspension
                    .get(i)
                    .copied()
                    .map(|world_extension| world_extension / info.scale)
                    .unwrap_or(wheel.rest_length);
                let spin = frame.wheel_rotation.get(i).copied().unwrap_or(0.0);
                let local = wheel_transform(wheel, suspension, spin, frame.steering);
                push(&wheel.model, drawn_body * local, [1.0; 4]);
            }
            // A broken turret flew off as its explosion's debris.
            if let (Some(model), Some(_mount)) = (&d.attachment_model, &d.attachment_mount)
                && !info.turret_broken
            {
                let sway = if let Some(sway) = &d.turret_sway {
                    let spring = self.sways.entry(*id).or_default();
                    // The spring rotation is applied after the authored mount and
                    // gun aim. Measure acceleration in that same frame: chassis
                    // axes here made a side-aimed gun lean in the wrong direction.
                    let attachment = weapon_attachment(d, frame, looks.contains_key(model), None)
                        .expect("attachment mount present");
                    let (_, attachment_rotation, _) = attachment.to_scale_rotation_translation();
                    spring.step(
                        sway,
                        self.clock / TICK_RATE,
                        frame.velocity,
                        frame.rotation * attachment_rotation,
                    );
                    Some(spring.rotation())
                } else {
                    None
                };
                let local = weapon_attachment(d, frame, looks.contains_key(model), sway)
                    .expect("attachment mount present");
                push(model, drawn_body * local, tint);
            }
        }
    }
    pub fn upload(
        assets: &mut VehicleAssets,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        for ((key, _), slot) in &mut assets.skinned {
            if slot.transforms.is_empty() {
                continue;
            }
            let base = assets
                .models
                .get_mut(key)
                .context("Animated vehicle base model")?;
            if base.gpu.is_none() {
                base.gpu = Some(renderer.upload(device, queue, &base.data)?);
            }
            if slot.gpu.is_none() || slot.topology_dirty {
                swap_geometry(&mut base.data, &mut slot.data);
                let result =
                    renderer.upload_geometry_shared(device, &base.data, base.gpu.as_ref().unwrap());
                swap_geometry(&mut base.data, &mut slot.data);
                slot.gpu = Some(result?);
                slot.topology_dirty = false;
            } else if let Some(gpu) = &mut slot.gpu {
                let centers: Vec<_> = slot.data.batches.iter().map(|b| b.center).collect();
                gpu.update_vertices(queue, &slot.data.vertices, &centers)?;
            }
            if slot
                .instances
                .as_ref()
                .is_none_or(|i| i.capacity() < slot.transforms.len())
            {
                slot.instances = Some(GpuInstances::new(
                    device,
                    slot.transforms.len().next_power_of_two().max(2),
                )?);
            }
            slot.instances.as_mut().unwrap().update_clipped(
                queue,
                &slot.transforms,
                &slot.clips,
            )?;
        }
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
            model.instances.as_mut().unwrap().update_clipped(
                queue,
                &model.transforms,
                &model.clips,
            )?;
        }
        Ok(())
    }
    pub fn draws(assets: &VehicleAssets) -> Vec<(&GpuScene, &GpuInstances)> {
        assets
            .models
            .values()
            .filter(|m| !m.transforms.is_empty())
            .filter_map(|m| Some((m.gpu.as_ref()?, m.instances.as_ref()?)))
            .chain(
                assets
                    .skinned
                    .values()
                    .filter(|m| !m.transforms.is_empty())
                    .filter_map(|m| Some((m.gpu.as_ref()?, m.instances.as_ref()?))),
            )
            .collect()
    }
    pub fn gpu_stopped(assets: &mut VehicleAssets) {
        for model in assets.skinned.values_mut() {
            model.gpu = None;
            model.instances = None;
        }
        for model in assets.models.values_mut() {
            model.gpu = None;
            model.instances = None;
        }
    }
}

/// The world look (yaw, pitch) along a replicated turret's aim: the inverse
/// of the host's gunner mapping, so a gunner taking over sends back the aim
/// the turret already has.
pub fn turret_look(pose: &VehiclePose) -> (f32, f32) {
    use std::f32::consts::{PI, TAU};
    let forward = Quat::from_array(pose.rotation).normalize() * Vec3::NEG_Z;
    let heading = forward.x.atan2(-forward.z);
    // Quaternion yaw turns left; look yaw turns right.
    let yaw = (heading - pose.turret_aim[0] + PI).rem_euclid(TAU) - PI;
    (yaw, pose.turret_aim[1])
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
    let ahead = (now - pose.tick as f64).clamped(0.0, DRIVEN_AHEAD);
    let mut frame = frame_of(pose);
    frame.position += frame.velocity * (ahead / TICK_RATE) as f32;
    if let Some(before) = index.checked_sub(1).map(|i| &history[i]) {
        let ticks = pose.tick.saturating_sub(before.tick);
        if (1..=SPIN_WINDOW).contains(&ticks) {
            let turn = (frame.rotation * Quat::from_array(before.rotation).normalize().inverse())
                .normalize();
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

fn sample(
    history: &VecDeque<VehiclePose>,
    tick: f64,
    passages: &bri_content::passage::Passages,
    middle: Vec3,
) -> VehicleFrame {
    let first = history.front().unwrap();
    if tick <= first.tick as f64 {
        return frame_of(first);
    }
    for (a, b) in history.iter().zip(history.iter().skip(1)) {
        if tick <= b.tick as f64 {
            let t = ((tick - a.tick as f64) / (b.tick - a.tick).max(1) as f64) as f32;
            let (fa, mut fb) = (frame_of(a), frame_of(b));
            let middle_of = |f: &VehicleFrame| f.position + f.rotation * middle;
            // Gone through an opening in between: drawn coming up to it and
            // cut there until its middle is through, then moving on from the
            // far side, never sliding across. Blended past the partner from
            // the start, a body not yet in would sit behind the far opening,
            // which a doorway's back face shares, and draw cut at that face:
            // its halves at the wrong doorways for a frame or two.
            let mut carried = None;
            if !passages.is_empty()
                && let Some(carry) = passages.bridge(middle_of(&fa), middle_of(&fb))
            {
                let back = carry.inverse();
                let (_, turn, _) = back.to_scale_rotation_translation();
                fb.position = back.transform_point3(fb.position);
                fb.rotation = (turn * fb.rotation).normalize();
                fb.velocity = turn * fb.velocity;
                carried = Some(carry);
            }
            let lerp = |x: &[f32], y: &[f32]| -> Vec<f32> {
                x.iter().zip(y).map(|(p, q)| p + (q - p) * t).collect()
            };
            let mut frame = VehicleFrame {
                position: fa.position.lerp(fb.position, t),
                rotation: fa.rotation.slerp(fb.rotation, t),
                velocity: fa.velocity.lerp(fb.velocity, t),
                steering: fa.steering + (fb.steering - fa.steering) * t,
                wheel_suspension: lerp(&fa.wheel_suspension, &fb.wheel_suspension),
                // Wheel spin wraps; take the newer value rather than blending.
                wheel_rotation: fb.wheel_rotation,
                wheel_contact: fb.wheel_contact,
                // Turret yaw wraps at the hull's back; blend the short way
                // or a barrel crossing it sweeps round the front for a frame.
                turret_aim: [
                    crate::motion::lerp_angle(fa.turret_aim[0], fb.turret_aim[0], t),
                    fa.turret_aim[1] + (fb.turret_aim[1] - fa.turret_aim[1]) * t,
                ],
            };
            if let Some(carry) = carried
                && passages
                    .travel(middle_of(&fa), middle_of(&frame))
                    .1
                    .is_some()
            {
                let (_, turn, _) = carry.to_scale_rotation_translation();
                frame.position = carry.transform_point3(frame.position);
                frame.rotation = (turn * frame.rotation).normalize();
                frame.velocity = turn * frame.velocity;
            }
            return frame;
        }
    }
    let last = history.back().unwrap();
    let mut frame = frame_of(last);
    let ahead = ((tick - last.tick as f64).min(6.0) / TICK_RATE) as f32;
    frame.position += frame.velocity * ahead;
    frame
}

/// A vehicle's body, attachment and moving parts are drawn in its colour
/// (its spawn brick's, or what painted it), or its class's wreck colour
/// once destroyed (v20 paints a wreck black until the final explosion
/// removes it). Driven by the replicated `destroyed` flag, so late joiners
/// see it and it costs nothing on the wire.
pub fn body_tint(d: &Definition, info: &VehicleInfo) -> [f32; 4] {
    if info.destroyed
        && let Some(wreck) = d.wreck_color()
    {
        return wreck;
    }
    info.color.map_or([1.0; 4], |[r, g, b, _]| [r, g, b, 1.0])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn pose(tick: u64, x: f32) -> VehiclePose {
        VehiclePose {
            passage_frame: Default::default(),
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
    #[test]
    fn seat_scale_matches_authority_with_unequal_riders_and_owned_or_fallback_hips() -> Result<()> {
        use bri_vehicles::{OwnerId, Spawn, Transform, VehicleId, VehiclesWorld};
        let dir = crate::testing::ScratchDir::new("seat-scale")?;
        crate::testing::vehicles::write_pack(dir.path())?;
        let mut assets = VehicleAssets::load(dir.path())?;
        let index = assets.index[bri_vehicles::testing::CAR];
        let seat = &mut assets.pack.definitions[index].seats[1];
        seat.transform.position = [0.5, 1.2, 0.8];
        seat.transform.rotation =
            (Quat::from_rotation_y(0.4) * Quat::from_rotation_z(0.3)).to_array();
        seat.rider_pose = Some("sit".into());
        seat.rider_fallback_hip = Some([0.1, 0.7, -0.2]);
        let body = crate::testing::avatar::assets()?;
        let bind = bri_content::animation::sample(&body.rig.shape, None, 0.)?;
        let parents: Vec<_> = body.rig.shape.nodes.iter().map(|n| n.parent).collect();
        let hip = bri_content::hold::hip(&parents, &bind.nodes).context("fixture hip")?;
        let hip_name = &body.rig.shape.nodes[hip].name;
        let sit = body.rig.sequence("sit").context("sit")?;
        let authored = bri_content::animation::sample(&body.rig.shape, Some(sit), sit.duration)?;
        for vehicle_scale in [0.5, 1., 2.] {
            let mut physics = bri_physics::new_world();
            let mut host = VehiclesWorld::new(assets.pack.clone())?;
            host.spawn(
                &mut physics,
                Spawn {
                    id: VehicleId(1),
                    owner: OwnerId(0),
                    definition: bri_vehicles::testing::CAR.into(),
                    scale: vehicle_scale,
                    transform: Transform {
                        position: [3., 4., 5.],
                        rotation: (Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.2))
                            .to_array(),
                    },
                    spawn_id: None,
                    respawn_ticks: None,
                },
            )?;
            let snapshot = host
                .vehicle_snapshot(&physics, VehicleId(1))
                .context("host snapshot")?;
            let expected = &snapshot.seats[1].transform;
            let info = VehicleInfo {
                id: 1,
                definition: snapshot.definition.clone(),
                scale: vehicle_scale,
                color: None,
                occupants: vec![],
                destroyed: false,
                turret_broken: false,
                lights: false,
            };
            let mut sample = pose(1, 0.);
            sample.position = snapshot.transform.position;
            sample.rotation = snapshot.transform.rotation;
            let mut client = ClientVehicles::default();
            client.update(
                &BTreeMap::from([(1, info.clone())]),
                &BTreeMap::from([(1, sample)]),
                Some(1.),
                None,
                &Default::default(),
            );
            let (origin, rotation) = client
                .seat_transform(&assets, &info, 1)
                .context("client seat")?;
            assert!(
                origin.abs_diff_eq(Vec3::from(expected.position), 1e-5),
                "vehicle scale {vehicle_scale}: client {origin}, authoritative {:?}",
                expected.position
            );
            assert!(rotation.abs_diff_eq(Quat::from_array(expected.rotation), 1e-5));
            let target = client
                .seat_fallback_hip(&assets, &info, 1, None)
                .context("fallback")?;
            let independent = Vec3::from(expected.position)
                + Quat::from_array(expected.rotation) * Vec3::new(0.1, 0.7, -0.2) * vehicle_scale;
            assert!(target.abs_diff_eq(independent, 1e-5));
            for rider_scale in [0.7, 1.6] {
                for turn in [-0.7, 0.4] {
                    for owns in [false, true] {
                        let rider_rotation = rotation * Quat::from_rotation_y(-turn);
                        let p: bri_sim::player::PlayerState = serde_json::from_value(
                            serde_json::json!({"owner":1,"feet":origin.to_array(),"velocity":[0.,0.,0.],"yaw":0.,"pitch":0.,"grounded":true,"crouched":false,"jetting":false,"scale":rider_scale}),
                        )?;
                        let input = crate::avatar::AvatarAnimationInput {
                            sitting: true,
                            mount_rotation: Some(rider_rotation),
                            seat_pose: Some(if owns { "sit" } else { "test_missing" }.into()),
                            seat_fallback_hip: Some(target),
                            ..Default::default()
                        };
                        let mut mesh = body.mesh(body.package.defaults.clone())?;
                        for time in [0., 1.] {
                            mesh.pose_with_animation(&body, &p, time, &input)?;
                        }
                        let observed = mesh
                            .world_node(&body, hip_name)
                            .context("world hip")?
                            .w_axis
                            .truncate();
                        let expected_hip = if owns {
                            Vec3::from(expected.position)
                                + Quat::from_array(expected.rotation)
                                    * Quat::from_rotation_y(-turn)
                                    * (authored.nodes[hip].w_axis.truncate() * rider_scale)
                        } else {
                            independent
                        };
                        assert!(
                            observed.abs_diff_eq(expected_hip, 1e-5),
                            "scale {vehicle_scale}/{rider_scale} owns {owns}: {observed} vs {expected_hip}"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn seat_scale_matches_authoritative_turret_pivot_and_broken_fallback() -> Result<()> {
        use bri_vehicles::{
            Controls, Occupant, OccupantId, OwnerId, Spawn, Transform, VehicleId, VehiclesWorld,
        };
        let dir = crate::testing::ScratchDir::new("turret-seat-scale")?;
        crate::testing::vehicles::write_pack(dir.path())?;
        let assets = VehicleAssets::load(dir.path())?;
        let d = assets
            .definition(bri_vehicles::testing::TANK)
            .context("tank")?;
        let seat = d.weapon_seat().context("gunner")?;
        for scale in [0.5, 1., 2.] {
            let mut physics = bri_physics::new_world();
            let mut host = VehiclesWorld::new(assets.pack.clone())?;
            host.spawn(
                &mut physics,
                Spawn {
                    id: VehicleId(1),
                    owner: OwnerId(0),
                    definition: d.id.clone(),
                    scale,
                    transform: Transform {
                        position: [3., 4., 5.],
                        rotation: (Quat::from_rotation_z(0.2) * Quat::from_rotation_y(0.7))
                            .to_array(),
                    },
                    spawn_id: None,
                    respawn_ticks: None,
                },
            )?;
            let at = host
                .seat_position(&physics, VehicleId(1), seat)
                .context("gunner position")?;
            host.mount(
                &physics,
                VehicleId(1),
                seat,
                Occupant {
                    id: OccupantId(7),
                    owner: OwnerId(7),
                    body: [0.8, 2.6],
                },
                at,
            )?;
            host.set_controls(
                OwnerId(7),
                OccupantId(7),
                Controls {
                    aim_yaw: 0.6,
                    ..Default::default()
                },
            )?;
            for broken in [false, true] {
                if broken {
                    host.damage_turret(&mut physics, VehicleId(1), 300., OwnerId(0))?;
                }
                let snapshot = host
                    .vehicle_snapshot(&physics, VehicleId(1))
                    .context("host snapshot")?;
                let info = VehicleInfo {
                    id: 1,
                    definition: d.id.clone(),
                    scale,
                    color: None,
                    occupants: vec![],
                    destroyed: false,
                    turret_broken: broken,
                    lights: false,
                };
                let mut sample = pose(1, 0.);
                sample.position = snapshot.transform.position;
                sample.rotation = snapshot.transform.rotation;
                sample.turret_aim = snapshot.turret_aim;
                let mut client = ClientVehicles::default();
                client.update(
                    &BTreeMap::from([(1, info.clone())]),
                    &BTreeMap::from([(1, sample)]),
                    Some(1.),
                    None,
                    &Default::default(),
                );
                let (position, rotation) = client
                    .seat_transform(&assets, &info, seat)
                    .context("client seat")?;
                let expected = &snapshot.seats[seat].transform;
                assert!(
                    position.abs_diff_eq(Vec3::from(expected.position), 1e-5),
                    "scale {scale}, broken {broken}"
                );
                assert!(rotation.abs_diff_eq(Quat::from_array(expected.rotation), 1e-5));
            }
        }
        Ok(())
    }

    /// v20 tires are authored with the hub axis along Torque +Y (native -Z),
    /// outer face first; mounted, they must roll along the chassis.
    #[test]
    fn tires_mount_axle_out_steer_right_and_roll_forward() {
        let tire = |x: f32| Wheel {
            position: [x, 0.4, -1.9],
            linear_steering: false,
            radius: 0.75,
            rest_length: 0.4,
            spring: 6000.0,
            damping: 800.0,
            anti_sway: 0.0,
            rebound_damping: 0.,
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
            rotation_interpolation: Default::default(),
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
    /// A vehicle pack on disk with gunners whose barrels a `look` clip
    /// poses, and vehicles that have none.
    struct Gunners {
        root: std::path::PathBuf,
        gunners: Vec<String>,
        unarmed: Vec<String>,
        _scratch: Option<crate::testing::ScratchDir>,
    }
    impl Gunners {
        fn synthetic() -> Result<Self> {
            use crate::testing::vehicles;
            let scratch = crate::testing::ScratchDir::new("vehicle-gunners")?;
            vehicles::write_pack(scratch.path())?;
            Ok(Self {
                root: scratch.path().to_path_buf(),
                gunners: vehicles::GUNNERS.map(String::from).into(),
                unarmed: vehicles::UNARMED.map(String::from).into(),
                _scratch: Some(scratch),
            })
        }
        fn content() -> Result<Self> {
            Ok(Self {
                root: bri_package::testing::pack_dir(
                    &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
                    "vehicles",
                ),
                gunners: ["v20.vehicle.tankvehicle", "v20.vehicle.cannonturret"]
                    .map(String::from)
                    .into(),
                unarmed: ["v20.vehicle.jeepvehicle", "v20.vehicle.horsearmor"]
                    .map(String::from)
                    .into(),
                _scratch: None,
            })
        }
    }
    crate::testing::synthetic_and_content!(Gunners: gunner_barrels_follow_the_pitch_to_the_muzzle);
    crate::testing::synthetic_and_content!(Gunners: a_rigged_mount_retains_the_same_portal_split_as_its_motor_and_riders);
    crate::testing::synthetic_and_content!(Gunners: a_broken_turret_leaves_the_hull_and_its_gunner_sits_on_the_hull);
    #[test]
    fn drawn_vehicle_scale_preserves_world_space_suspension_extension() -> Result<()> {
        let fixture = Gunners::synthetic()?;
        let mut assets = VehicleAssets::load(&fixture.root)?;
        let d = assets
            .pack
            .definitions
            .iter()
            .find(|d| !d.wheels.is_empty() && !d.is_actor())
            .context("wheeled vehicle")?
            .clone();
        let info = VehicleInfo {
            id: 1,
            definition: d.id.clone(),
            color: None,
            occupants: vec![],
            destroyed: false,
            turret_broken: false,
            scale: 2.,
            lights: false,
        };
        let mut state = pose(1, 3.);
        state.rotation = Quat::from_rotation_z(0.2).to_array();
        state.wheel_suspension = vec![0.3; d.wheels.len()];
        let infos = BTreeMap::from([(1, info.clone())]);
        let mut vehicles = ClientVehicles::default();
        vehicles.update(
            &infos,
            &BTreeMap::from([(1, state)]),
            None,
            None,
            &Default::default(),
        );
        vehicles.prepare(&mut assets, &infos);
        let body = &assets.models[&d.model].transforms[0].transform;
        assert!((body.x_axis.truncate().length() - 2.).abs() < 1e-5);
        let wheel = &d.wheels[0];
        let transform = assets.models[&wheel.model].transforms[0].transform;
        let frame = vehicles.frame(1).unwrap();
        let expected =
            frame.position + frame.rotation * (Vec3::from(wheel.position) * 2. - Vec3::Y * 0.3);
        assert!(
            transform.w_axis.truncate().abs_diff_eq(expected, 1e-5),
            "world suspension must not be scaled a second time"
        );
        assert!((transform.x_axis.truncate().length() - 2.).abs() < 1e-5);
        Ok(())
    }

    #[test]
    fn named_grip_nodes_follow_attachment_animation_aim_sway_and_scale() -> Result<()> {
        let fixture = Gunners::synthetic()?;
        let mut assets = VehicleAssets::load(&fixture.root)?;
        let d = fixture
            .gunners
            .iter()
            .filter_map(|id| assets.definition(id))
            .find(|d| d.attachment_model.is_some())
            .context("attached weapon")?
            .clone();
        let seat = d.weapon_seat().context("gunner")?;
        let model = d.attachment_model.as_ref().unwrap().clone();
        let node = d.muzzle_node().to_string();
        let index = assets.index[&d.id];
        assets.pack.definitions[index].seats[seat].hands.right = Some([20.; 3]);
        assets.pack.definitions[index].seats[seat].hands.right_node = Some(node.clone());
        let mut info = VehicleInfo {
            id: 1,
            definition: d.id.clone(),
            color: None,
            occupants: vec![],
            destroyed: false,
            turret_broken: false,
            scale: 2.,
            lights: false,
        };
        let mut vehicles = ClientVehicles::default();
        let mut sample = pose(1, 3.);
        sample.turret_aim = [0.6, 0.3];
        sample.rotation = Quat::from_rotation_z(0.2).to_array();
        vehicles.update(
            &BTreeMap::from([(1, info.clone())]),
            &BTreeMap::from([(1, sample)]),
            None,
            None,
            &Default::default(),
        );
        vehicles.sways.insert(
            1,
            TurretSpring {
                tip: [0.1, -0.05],
                ..Default::default()
            },
        );
        let expected = vehicles
            .weapon_node(&assets, &info, &node)
            .context("animated attachment node")?
            .w_axis
            .truncate();
        let actual = vehicles.hand_holds(&assets, &info, seat)[0].context("named hold")?;
        assert!(actual.abs_diff_eq(expected, 1e-5));
        info.scale = 1.;
        let unscaled = vehicles.hand_holds(&assets, &info, seat)[0].unwrap();
        let origin = vehicles.frame(1).unwrap().position;
        assert!(
            (actual - origin).abs_diff_eq((unscaled - origin) * 2., 1e-5),
            "attachment sockets share vehicle scale"
        );
        // A model without a look clip pitches its entire attachment instead.
        assets.looks.remove(&model);
        let expected = vehicles
            .weapon_node(&assets, &info, &node)
            .unwrap()
            .w_axis
            .truncate();
        assert!(
            vehicles.hand_holds(&assets, &info, seat)[0]
                .unwrap()
                .abs_diff_eq(expected, 1e-5)
        );
        assets.pack.definitions[index].seats[seat].hands.right_node = Some("missing-grip".into());
        assert!(
            vehicles.hand_holds(&assets, &info, seat)[0].is_none(),
            "a missing explicit node must not silently use a guessed point"
        );
        Ok(())
    }

    #[test]
    fn skinned_guns_keep_independent_poses_and_retire_removed_instances() -> Result<()> {
        use bri_content::shape::{Influence, Skin};
        let fixture = Gunners::synthetic()?;
        let mut assets = VehicleAssets::load(&fixture.root)?;
        let d = fixture
            .gunners
            .iter()
            .filter_map(|id| assets.definition(id))
            .find(|d| {
                d.attachment_model
                    .as_ref()
                    .is_some_and(|m| assets.looks.contains_key(m))
            })
            .context("animated gun")?
            .clone();
        let model = d.attachment_model.as_ref().unwrap().clone();
        let rig = assets.looks.get_mut(&model).unwrap();
        let rest = bri_content::animation::sample(&rig.shape, None, 0.)?;
        let object = rig
            .shape
            .objects
            .iter()
            .find(|o| o.node.is_some() && !o.meshes.is_empty())
            .unwrap()
            .clone();
        let node = object.node.unwrap();
        let mesh = rig.shape.meshes[object.meshes[0]].as_mut().unwrap();
        for p in &mut mesh.positions {
            *p = rest.nodes[node].transform_point3(Vec3::from(*p)).to_array();
        }
        for n in &mut mesh.normals {
            *n = rest.nodes[node]
                .inverse()
                .transpose()
                .transform_vector3(Vec3::from(*n))
                .to_array();
        }
        mesh.skin = Some(Skin {
            nodes: vec![node, 0],
            inverse_bind: vec![
                rest.nodes[node].inverse().to_cols_array(),
                rest.nodes[0].inverse().to_cols_array(),
            ],
            influences: (0..mesh.frame_vertices)
                .flat_map(|vertex| {
                    [
                        Influence {
                            vertex,
                            bone: 0,
                            weight: 0.7,
                        },
                        Influence {
                            vertex,
                            bone: 1,
                            weight: 0.3,
                        },
                    ]
                })
                .collect(),
        });
        rig.shape.validate()?;
        rig.skinned = true;
        rig.parts.clear();
        for material in &mut assets.models.get_mut(&model).unwrap().data.materials {
            material.kind = bri_render::scene::MaterialKind::Metal;
        }
        let info = VehicleInfo {
            id: 1,
            definition: d.id.clone(),
            color: None,
            occupants: vec![],
            destroyed: false,
            turret_broken: false,
            scale: 1.,
            lights: false,
        };
        let mut second = info.clone();
        second.id = 2;
        let mut infos = BTreeMap::from([(1, info), (2, second)]);
        let mut a = pose(1, 3.);
        a.turret_aim = [0., -0.4];
        let mut b = a.clone();
        b.id = 2;
        b.turret_aim[1] = 0.4;
        let mut vehicles = ClientVehicles::default();
        vehicles.update(
            &infos,
            &BTreeMap::from([(1, a), (2, b)]),
            None,
            None,
            &Default::default(),
        );
        vehicles.prepare(&mut assets, &infos);
        let a = &assets.skinned[&(model.clone(), 1)];
        let b = &assets.skinned[&(model.clone(), 2)];
        assert!(!a.faulted && !b.faulted);
        assert!(!a.data.vertices.is_empty());
        assert_eq!(a.data.indices, b.data.indices);
        assert!(
            a.data.images.is_empty() && b.data.images.is_empty(),
            "texture pixels stay on shared base"
        );
        assert!(
            a.data
                .vertices
                .iter()
                .zip(&b.data.vertices)
                .any(|(a, b)| Vec3::from(a.position).distance(Vec3::from(b.position)) > 0.01)
        );
        assert!(
            assets.models[&model].transforms.is_empty(),
            "no duplicate rigid rest gun"
        );
        let centres = assets.metal_centres();
        for slot in [a, b] {
            let centre = metal_centre(&slot.data).context("posed metal")?;
            let world = slot.transforms[0].transform.transform_point3(centre);
            assert!(
                centres.iter().any(|p| p.abs_diff_eq(world, 1e-5)),
                "each posed gun contributes its current reflection-probe centre"
            );
        }
        infos.remove(&1);
        vehicles.prepare(&mut assets, &infos);
        assert!(!assets.skinned.contains_key(&(model.clone(), 1)));
        assert!(assets.skinned.contains_key(&(model, 2)));
        Ok(())
    }

    /// Max, v0.2.5: a blown turret flew off as debris while the hull still
    /// carried it. Once broken it is no longer drawn on the hull, and its
    /// gunner sits on the hull's fallback seat, where the host put them.
    fn a_broken_turret_leaves_the_hull_and_its_gunner_sits_on_the_hull(fx: &Gunners) -> Result<()> {
        let mut assets = VehicleAssets::load(&fx.root)?;
        let d = fx
            .gunners
            .iter()
            .filter_map(|id| assets.definition(id))
            .find(|d| d.attachment_model.is_some() && d.attachment_fallback_seat.is_some())
            .context("a gunner with an attached turret")?
            .clone();
        let turret = d.attachment_model.clone().unwrap();
        let seat = d.weapon_seat().context("the turret's gunner seat")?;
        let fallback = Vec3::from(d.attachment_fallback_seat.as_ref().unwrap().position);
        for (destroyed, turret_broken) in [(false, false), (false, true), (true, true)] {
            let infos = BTreeMap::from([(
                1,
                VehicleInfo {
                    id: 1,
                    definition: d.id.clone(),
                    color: None,
                    occupants: vec![],
                    destroyed,
                    turret_broken,
                    scale: 1.0,
                    lights: false,
                },
            )]);
            let mut vehicles = ClientVehicles::default();
            vehicles.update(
                &infos,
                &BTreeMap::from([(1, pose(1, 0.0))]),
                None,
                None,
                &Default::default(),
            );
            vehicles.prepare(&mut assets, &infos);
            let drawn: usize = assets
                .models
                .iter()
                .filter(|(key, _)| key.starts_with(&turret))
                .map(|(_, m)| m.transforms.len())
                .sum();
            assert_eq!(
                drawn > 0,
                !turret_broken,
                "turret drawn with destroyed {destroyed}, broken {turret_broken}"
            );
            let (gunner, _) = vehicles.seat_transform(&assets, &infos[&1], seat).unwrap();
            assert_eq!(
                gunner.distance(fallback) < 1e-4,
                turret_broken,
                "gunner at {gunner}, fallback {fallback}"
            );
        }
        Ok(())
    }
    fn a_rigged_mount_retains_the_same_portal_split_as_its_motor_and_riders(
        fx: &Gunners,
    ) -> Result<()> {
        use bri_content::passage::Passage;
        use bri_vehicles::{OwnerId, Spawn, Transform, VehicleId, VehiclesWorld};
        let mut assets = VehicleAssets::load(&fx.root)?;
        let d = assets
            .pack
            .definitions
            .iter()
            .find(|d| d.family == bri_vehicles::Family::Horse)
            .unwrap()
            .clone();
        for scale in [0.75, 1., 1.5] {
            let root = Vec3::new(0., 0., 0.15);
            let mut physics = bri_physics::new_world();
            let mut mounts = VehiclesWorld::new(assets.pack.clone())?;
            mounts.spawn(
                &mut physics,
                Spawn {
                    id: VehicleId(1),
                    owner: OwnerId(0),
                    definition: d.id.clone(),
                    scale,
                    transform: Transform {
                        position: root.to_array(),
                        ..Default::default()
                    },
                    spawn_id: None,
                    respawn_ticks: None,
                },
            )?;
            // Read the actual motor/body boundary, not a nominal player height
            // or the renderer's authored model bounds/mass centre.
            let motor_middle = mounts.centre(&physics, VehicleId(1)).unwrap();
            let carry = glam::Affine3A::from_translation(Vec3::new(30., 0., 0.))
                * glam::Affine3A::from_rotation_y(std::f32::consts::FRAC_PI_2);
            let passages = Passages {
                list: vec![Passage {
                    brick: 2,
                    centre: Vec3::new(0., motor_middle.y, 0.),
                    normal: Vec3::Z,
                    u: Vec3::X,
                    v: Vec3::Y,
                    half: glam::Vec2::splat(10.),
                    carry,
                }],
                closed: vec![],
            };
            let mut host_pose = pose(1, 0.);
            host_pose.position = root.to_array();
            let infos = BTreeMap::from([(
                1,
                VehicleInfo {
                    id: 1,
                    definition: d.id.clone(),
                    color: None,
                    occupants: vec![Some(7)],
                    destroyed: false,
                    turret_broken: false,
                    scale,
                    lights: false,
                },
            )]);
            let mut vehicles = ClientVehicles::default();
            vehicles.set_passages(&passages);
            vehicles.update(
                &infos,
                &BTreeMap::from([(1, host_pose)]),
                Some(1.),
                None,
                &passages,
            );
            vehicles.prepare(&mut assets, &infos);
            let split = *vehicles
                .straddle(1)
                .expect("rigged horse and rider retain a shared vehicle split");
            let expected = Straddle::find(&passages, motor_middle, 10.).unwrap();
            assert_eq!(split, expected);
            assert!(
                assets.models.values().all(|m| m.transforms.is_empty()),
                "horse rig must not also draw as static vehicle models"
            );
            vehicles.set_passages(&Passages::default());
            vehicles.prepare(&mut assets, &infos);
            assert!(
                vehicles.straddle(1).is_none(),
                "map/opening removal clears the split"
            );
        }
        Ok(())
    }
    fn gunner_barrels_follow_the_pitch_to_the_muzzle(fx: &Gunners) -> Result<()> {
        let assets = VehicleAssets::load(&fx.root)?;
        for id in &fx.gunners {
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
                let draws = posed(&assets.looks, &model, d.look_phase(pitch), Mat4::IDENTITY);
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
        for id in &fx.unarmed {
            if let Some(d) = assets.definition(id) {
                assert!(!assets.looks.contains_key(&d.model), "{id}");
            }
        }
        Ok(())
    }
    /// Max, v0.1.10: a destroyed jeep, tank or plane kept its paint while it
    /// burned. v20 paints the wreck black and its tires are gone until the
    /// final explosion; PlayerData mounts keep their colour. Uses a
    /// synthetic stand-in plane, a `WheeledVehicleData` with three wheels.
    #[test]
    fn a_destroyed_vehicle_is_drawn_black_without_its_tires() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vehicles/tests/fixtures/stand-in-plane/assets");
        let mut assets = VehicleAssets::load(&root)?;
        let d = assets.pack.definitions[0].clone();
        ensure!(d.family == bri_vehicles::Family::Wheeled && d.wheels.len() == 3);
        let draw = |assets: &mut VehicleAssets, destroyed: bool| {
            let infos = BTreeMap::from([(
                1,
                VehicleInfo {
                    id: 1,
                    definition: d.id.clone(),
                    color: Some([0.9, 0.1, 0.1, 1.0]),
                    occupants: vec![],
                    destroyed,
                    turret_broken: false,
                    scale: 1.0,
                    lights: false,
                },
            )]);
            let mut vehicles = ClientVehicles::default();
            vehicles.update(
                &infos,
                &BTreeMap::from([(1, pose(1, 0.0))]),
                None,
                None,
                &Default::default(),
            );
            vehicles.prepare(assets, &infos);
            let body: Vec<_> = assets.models[&d.model]
                .transforms
                .iter()
                .map(|t| t.tint)
                .collect();
            let wheels: usize = d
                .wheels
                .iter()
                .map(|w| w.model.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .iter()
                .map(|m| assets.models.get(*m).map_or(0, |m| m.transforms.len()))
                .sum();
            (body, wheels)
        };
        let (body, wheels) = draw(&mut assets, false);
        assert_eq!(
            body,
            vec![[0.9, 0.1, 0.1, 1.0]],
            "a live vehicle wears its paint"
        );
        assert_eq!(wheels, 3, "and rolls on its tires");
        let (body, wheels) = draw(&mut assets, true);
        assert_eq!(body, vec![[0.0, 0.0, 0.0, 1.0]], "a wreck is charred black");
        assert_eq!(wheels, 0, "and its tires are gone");
        Ok(())
    }
    /// Reliable paint changes on the same vehicle are reflected in the next
    /// prepared frame; clearing paint restores its authored material tint.
    #[test]
    fn a_live_vehicle_updates_its_draw_tint_without_a_new_pose_or_identity() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vehicles/tests/fixtures/stand-in-plane/assets");
        let mut assets = VehicleAssets::load(&root)?;
        let d = assets.pack.definitions[0].clone();
        let mut infos = BTreeMap::from([(
            1,
            VehicleInfo {
                id: 1,
                definition: d.id.clone(),
                color: Some([1.0, 0.0, 0.0, 1.0]),
                occupants: vec![Some(7)],
                destroyed: false,
                turret_broken: false,
                scale: 1.0,
                lights: false,
            },
        )]);
        let mut vehicles = ClientVehicles::default();
        vehicles.update(
            &infos,
            &BTreeMap::from([(1, pose(1, 0.0))]),
            None,
            None,
            &Default::default(),
        );
        for (color, tint) in [
            (Some([1.0, 0.0, 0.0, 1.0]), [1.0, 0.0, 0.0, 1.0]),
            (None, [1.0; 4]),
            (Some([0.0, 0.0, 1.0, 1.0]), [0.0, 0.0, 1.0, 1.0]),
        ] {
            infos.get_mut(&1).unwrap().color = color;
            vehicles.prepare(&mut assets, &infos);
            let transforms = &assets.models[&d.model].transforms;
            assert!(!transforms.is_empty());
            assert!(transforms.iter().all(|t| t.tint == tint));
        }
        Ok(())
    }
    #[test]
    fn only_vehicle_classes_char_and_player_mounts_keep_their_colour() {
        let plane: Pack = serde_json::from_slice(include_bytes!(
            "../../vehicles/tests/fixtures/stand-in-plane/assets/vehicles.json"
        ))
        .unwrap();
        let mut d = plane.definitions[0].clone();
        let info = |destroyed| VehicleInfo {
            id: 1,
            definition: d.id.clone(),
            color: Some([0.2, 0.4, 0.6, 1.0]),
            occupants: vec![],
            destroyed,
            turret_broken: false,
            scale: 1.0,
            lights: false,
        };
        let (live, dead) = (info(false), info(true));
        use bri_vehicles::Family::*;
        for family in [Wheeled, Flying, Ball] {
            d.family = family;
            assert_eq!(body_tint(&d, &live), [0.2, 0.4, 0.6, 1.0], "{family:?}");
            assert_eq!(body_tint(&d, &dead), [0.0, 0.0, 0.0, 1.0], "{family:?}");
        }
        for family in [Horse, Rowboat, Cannon, Turret, Skis, Tumble] {
            d.family = family;
            assert_eq!(body_tint(&d, &dead), [0.2, 0.4, 0.6, 1.0], "{family:?}");
        }
        // Unpainted, a live vehicle shows its own texture.
        d.family = Wheeled;
        let plain = VehicleInfo {
            color: None,
            ..live.clone()
        };
        assert_eq!(body_tint(&d, &plain), [1.0; 4]);
    }
    #[test]
    fn a_material_named_with_its_extension_finds_its_texture() {
        let image = |label: &str| SceneImage {
            label: label.into(),
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
            srgb: false,
        };
        let textures: BTreeMap<String, SceneImage> = [
            "add-ons/vehicle_tank/black.png",
            "add-ons/vehicle_tank/blank.png",
            "add-ons/vehicle_jeep/black.png",
            "add-ons/vehicle_jeep/gray25.png",
        ]
        .into_iter()
        .map(|path| (path.to_string(), image(path)))
        .collect();
        let found = |folder: &str, material: &str| {
            material_texture(&textures, folder, material).map(|i| i.label.as_str())
        };
        let tank = "add-ons/vehicle_tank";
        // The Tank's wreck and turret name `black.png`; its hull `black`.
        assert_eq!(
            found(tank, "black.png"),
            Some("add-ons/vehicle_tank/black.png")
        );
        assert_eq!(found(tank, "Black"), Some("add-ons/vehicle_tank/black.png"));
        // Beside the model first, then anywhere in the pack.
        assert_eq!(
            found(tank, "gray25.png"),
            Some("add-ons/vehicle_jeep/gray25.png")
        );
        assert_eq!(found(tank, "missing.png"), None);
    }
    /// A wreck burns with its own `damageEmitter`s, each once: the stand-in
    /// plane names `StandInWreckEmitter` (a base-game name to it) twice; an Add-On's own emitter
    /// resolves to its id; a mount without any (a horse) does not burn.
    #[test]
    fn a_wreck_burns_with_its_own_damage_emitters() {
        let plane: Pack = serde_json::from_slice(include_bytes!(
            "../../vehicles/tests/fixtures/stand-in-plane/assets/vehicles.json"
        ))
        .unwrap();
        let mut d = plane.definitions[0].clone();
        assert_eq!(d.wreck_emitters(), ["v20/emitter/standinwreckemitter"]);
        let own = d.effects.emitters[0].id.clone();
        let (_, name) = own.rsplit_once(":emitter/").unwrap();
        d.authored
            .insert("damageemitter[2]".into(), name.to_ascii_uppercase());
        assert_eq!(
            d.wreck_emitters(),
            ["v20/emitter/standinwreckemitter".to_string(), own]
        );
        d.authored.retain(|k, _| !k.starts_with("damageemitter"));
        assert!(d.wreck_emitters().is_empty());
    }
    /// Max's video: a Steel Ball rolled through a doorway portal. Drawn
    /// between two poses either side of it, the ball comes up to the
    /// doorway it went in by, cut there, and only then rolls on out of the
    /// partner: never cut at the partner's back face, its halves at the
    /// wrong doorways (spinning, with its middle off its origin).
    #[test]
    fn a_ball_between_poses_through_a_doorway_draws_only_on_its_way() {
        let (passages, carry) = crate::portal_view::doorways::pair();
        let middle = Vec3::new(0.0, 0.3, 0.1);
        let spin = Quat::from_rotation_x(-0.6);
        let at = |tick, position: Vec3, rotation: Quat| {
            let mut p = pose(tick, 0.0);
            p.position = position.to_array();
            p.rotation = rotation.to_array();
            p.velocity = [0.0, 0.0, -30.0];
            p
        };
        let before = at(10, Vec3::new(0.2, 1.0, 0.7), Quat::IDENTITY);
        let (_, turn, _) = carry.to_scale_rotation_translation();
        let after = at(
            14,
            carry.transform_point3(Vec3::new(0.2, 1.0, -0.7)),
            turn * spin,
        );
        let history: VecDeque<_> = [before, after].into();
        let mut wrong = Vec::new();
        for step in 0..=40 {
            let tick = 10.0 + 4.0 * step as f64 / 40.0;
            let frame = sample(&history, tick, &passages, middle);
            let drawn = frame.position + frame.rotation * middle;
            if let Err(e) = crate::portal_view::doorways::drawn_on_its_way(drawn, 1.3) {
                wrong.push(format!("tick {tick}: {e}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
    #[test]
    fn vehicle_samples_interpolate_between_poses() {
        let history: VecDeque<_> = [pose(10, 0.0), pose(13, 3.0)].into();
        assert!(
            (sample(&history, 11.5, &Default::default(), Vec3::ZERO)
                .position
                .x
                - 1.5)
                .abs()
                < 1e-5
        );
        assert_eq!(
            sample(&history, 0.0, &Default::default(), Vec3::ZERO)
                .position
                .x,
            0.0
        );
    }
    /// Max, v0.1.9: dragged about by a Gravity Gun, the held player saw
    /// their own body stutter. Their tumble was drawn as if they drove it,
    /// guessed ahead of each pose and pulled back when the hold slowed it;
    /// drawn from the host's poses like everyone else's, a body pulled
    /// along in bursts, its poses arriving unevenly, never steps back.
    #[test]
    fn a_dragged_body_drawn_from_the_hosts_poses_never_steps_back() {
        let infos = BTreeMap::from([(
            1,
            VehicleInfo {
                id: 1,
                definition: String::new(),
                color: None,
                occupants: vec![],
                destroyed: false,
                turret_broken: false,
                scale: 1.0,
                lights: false,
            },
        )]);
        // The host: pulled toward a point that jumps ahead in bursts.
        let (mut x, mut speed) = (0.0f32, 0.0f32);
        let host: Vec<_> = (0..600u64)
            .map(|tick| {
                let target = (tick / 40) as f32 * 3.0;
                let wanted = ((target - x) * 8.0).clamped(-12.0, 12.0);
                speed += (wanted - speed).clamped(-3.0, 3.0);
                x += speed / TICK_RATE as f32;
                (tick, x, speed)
            })
            .collect();
        let draw = |driven: Option<u64>| {
            let mut vehicles = ClientVehicles::default();
            let mut poses = BTreeMap::new();
            let (mut sent, mut drawn) = (0, vec![]);
            for frame in 0..1200 {
                let now = frame as f64 * 0.5 + 20.0;
                let arrived = now - [0.0, 3.0, 1.0, 4.0, 0.0, 2.0][frame % 6];
                while sent < host.len() && host[sent].0 as f64 <= arrived {
                    let (tick, x, speed) = host[sent];
                    if tick % 3 == 0 {
                        let moving = VehiclePose {
                            velocity: [speed, 0.0, 0.0],
                            ..pose(tick, x)
                        };
                        poses.insert(1, moving);
                    }
                    sent += 1;
                }
                vehicles.update(&infos, &poses, Some(now), driven, &Default::default());
                drawn.push(vehicles.frame(1).unwrap().position.x);
            }
            drawn[40..]
                .windows(2)
                .filter(|w| w[1] < w[0] - 1e-4)
                .count()
        };
        assert_eq!(draw(None), 0, "drawn from the host's poses");
        assert!(draw(Some(1)) > 0, "guessed ahead, it is pulled back");
    }
    #[test]
    fn a_gunners_aim_runs_from_the_highest_to_the_lowest_pitch() {
        assert_eq!(seat_aim_phase([-0.5, 1.0], 1.0), 0.0);
        assert_eq!(seat_aim_phase([-0.5, 1.0], -0.5), 1.0);
        assert!((seat_aim_phase([-0.5, 1.0], 0.25) - 0.5).abs() < 1e-6);
        assert_eq!(seat_aim_phase([-0.5, 1.0], 3.0), 0.0);
        assert_eq!(seat_aim_phase([0.0, 0.0], 0.3), 0.5);
    }

    #[test]
    fn a_turret_tips_back_as_the_vehicle_speeds_up_and_settles() {
        let sway = bri_vehicles::schema::TurretSway {
            response: 0.01,
            frequency: 3.0,
            damping: 0.3,
            max_degrees: 4.0,
        };
        let mut spring = TurretSpring::default();
        let mut most: f32 = 0.0;
        // 0.5 s pulling 10 u/s² forward (-Z), then cruising.
        for i in 0..=180 {
            let t = i as f64 / 60.0;
            let speed = (t.min(0.5) * 10.0) as f32;
            spring.step(&sway, t, Vec3::NEG_Z * speed, Quat::IDENTITY);
            most = most.max(spring.tip[0]);
        }
        assert!(most > 0.05, "tips back: {most}");
        assert!(most <= 4f32.to_radians() + 1e-6);
        assert!(
            spring.tip[0].abs() < 0.01 && spring.tip[1] == 0.0,
            "settled: {spring:?}"
        );
    }

    #[test]
    fn a_turret_turning_past_the_hulls_back_never_sweeps_round_the_front() {
        // The gunner turns the barrel through straight behind: the host's
        // relative yaw wraps from just under pi to just over -pi.
        use std::f32::consts::PI;
        let mut a = pose(10, 0.0);
        let mut b = pose(12, 0.0);
        a.turret_aim = [PI - 0.1, 0.2];
        b.turret_aim = [-PI + 0.1, 0.4];
        let history = VecDeque::from(vec![a, b]);
        for step in 0..=20 {
            let tick = 10.0 + 2.0 * step as f64 / 20.0;
            let [yaw, pitch] = sample(&history, tick, &Default::default(), Vec3::ZERO).turret_aim;
            // Off straight behind by at most the 0.1 each side it started.
            let off_back = PI - yaw.abs();
            assert!(off_back <= 0.1 + 1e-4, "tick {tick}: yaw {yaw} swung round");
            assert!((0.2..=0.4 + 1e-5).contains(&pitch));
        }
        let [yaw, _] = sample(&history, 11.0, &Default::default(), Vec3::ZERO).turret_aim;
        assert!(
            (yaw.abs() - PI).abs() < 1e-4,
            "midway is straight behind, got {yaw}"
        );
    }
    #[test]
    fn a_gunner_taking_over_looks_along_the_turret() {
        // The host's gunner mapping: aim = -wrap(look - heading).
        let wrap = |a: f32| {
            (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
        };
        for (hull, aim) in [(0.0, 0.0), (1.0, 2.5), (-2.8, -2.9), (3.0, 3.1)] {
            let mut p = pose(1, 0.0);
            p.rotation = Quat::from_rotation_y(-hull).to_array();
            p.turret_aim = [aim, 0.3];
            let (yaw, pitch) = turret_look(&p);
            let forward = Quat::from_array(p.rotation) * Vec3::NEG_Z;
            let heading = forward.x.atan2(-forward.z);
            assert!(
                wrap(-wrap(yaw - heading) - aim).abs() < 1e-4,
                "{hull} {aim} -> {yaw}"
            );
            assert_eq!(pitch, 0.3);
        }
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
                turret_broken: false,
                scale: 1.0,
                lights: false,
            },
        )]);
        let mut vehicles = ClientVehicles::default();
        let moving = VehiclePose {
            velocity: [12.0, 0.0, 0.0],
            ..pose(0, 0.0)
        };
        vehicles.update(
            &infos,
            &BTreeMap::from([(1, moving)]),
            Some(3.0),
            Some(1),
            &Default::default(),
        );
        let before = vehicles.frame(1).unwrap().position;
        assert!((before.x - 0.3).abs() < 1e-5);
        // The host says it stopped at 0.1: no pop, then it settles there.
        let stopped = BTreeMap::from([(1, pose(3, 0.1))]);
        vehicles.update(&infos, &stopped, Some(3.0), Some(1), &Default::default());
        assert!((vehicles.frame(1).unwrap().position - before).length() < 1e-5);
        for frame in 1..=60 {
            vehicles.update(
                &infos,
                &stopped,
                Some(3.0 + frame as f64 * 2.0),
                Some(1),
                &Default::default(),
            );
        }
        assert!((vehicles.frame(1).unwrap().position.x - 0.1).abs() < 0.01);
    }
}
