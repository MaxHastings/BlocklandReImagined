//! Bounded, deterministic native weapon-shell presentation.
//!
//! The host provides the original animated image's shell-eject transform and a
//! collision sweep. This module owns cosmetic casing state only: it never
//! chooses gameplay hits or guesses an attachment pose.
use anyhow::{Context, Result, ensure};
use bri_content::{animation::sample, shape::Shape};
use bri_render::scene::{AlphaMode, Material, SceneData, SceneImage};
use bri_render::shape_scene::ShapeInstance;
use bri_sim::presentation::{Cue, CueKind};
use glam::{Mat4, Quat, Vec3};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
};

const MAX_BODIES: usize = 512;
const MAX_PENDING: usize = 512;
const PENDING_SECONDS: f64 = 0.5;
const FIXED_STEP: f64 = 1. / 120.;
const MAX_FRAME_SECONDS: f64 = 0.25;
/// Most images with a casing of their own.
const MAX_CASINGS: usize = 256;
/// The base game's casing model, which the stock shell pack draws.
const STOCK_SHELL_MODEL: &str = "add-ons/weapon_gun/gunshell.dts";

#[derive(Clone, Debug, Default)]
pub struct WeaponDebrisDiagnostics {
    pub accepted_cues: u64,
    pub duplicate_cues: u64,
    pub missing_pose_expired: u64,
    pub invalid_cues: u64,
    pub body_capacity_drops: u64,
    pub pending_capacity_drops: u64,
    pub collision_queries: u64,
    pub bounces: u64,
    pub settled: u64,
    /// Casings dropped because the collision sweep returned no usable hit.
    pub invalid_collisions: u64,
    pub capped_frame_time: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct WeaponDebrisLimits {
    pub bodies: usize,
    pub pending: usize,
}
impl Default for WeaponDebrisLimits {
    fn default() -> Self {
        Self {
            bodies: MAX_BODIES,
            pending: MAX_PENDING,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
struct Pack {
    schema_version: u32,
    id: String,
    weapons_sha256: String,
    models: Vec<ModelResource>,
    textures: Vec<TextureResource>,
    shell: ShellDefinition,
}
#[derive(Clone, Debug, Deserialize)]
struct ModelResource {
    id: String,
    file: String,
    native_sha256: String,
}
#[derive(Clone, Debug, Deserialize)]
struct TextureResource {
    id: String,
    file: String,
    source_sha256: String,
    width: u32,
    height: u32,
}
#[derive(Clone, Debug, Deserialize)]
struct ShellDefinition {
    model: String,
    lifetime_seconds: f32,
    min_spin_degrees_per_second: f32,
    max_spin_degrees_per_second: f32,
    elasticity: f32,
    friction: f32,
    bounces: u32,
    static_on_max_bounce: bool,
    snap_on_max_bounce: bool,
    fade: bool,
    gravity_multiplier: f32,
    exit_direction: [f32; 3],
    exit_offset: [f32; 3],
    exit_variance_degrees: f32,
    velocity: f32,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate_shell(d: &ShellDefinition) -> Result<()> {
    ensure!(
        d.model == "v20.weapon_debris.gun_shell"
            && d.lifetime_seconds.is_finite()
            && (0.01..=30.).contains(&d.lifetime_seconds)
            && d.min_spin_degrees_per_second.is_finite()
            && d.max_spin_degrees_per_second.is_finite()
            && d.min_spin_degrees_per_second <= d.max_spin_degrees_per_second
            && (0. ..=1.).contains(&d.elasticity)
            && (0. ..=1.).contains(&d.friction)
            && d.bounces <= 32
            && d.static_on_max_bounce
            && !d.snap_on_max_bounce
            && d.fade
            && d.gravity_multiplier.is_finite()
            && (0. ..=20.).contains(&d.gravity_multiplier)
            && d.exit_direction.iter().all(|v| v.is_finite())
            && Vec3::from_array(d.exit_direction).length_squared() > 1e-8
            && d.exit_offset
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 100.)
            && d.exit_variance_degrees.is_finite()
            && (0. ..=180.).contains(&d.exit_variance_degrees)
            && d.velocity.is_finite()
            && (0. ..=200.).contains(&d.velocity),
        "invalid native shell definition"
    );
    Ok(())
}

/// Shared immutable original model geometry and source texture images. Upload
/// `shell_scene` once, then render `instances()` through `GpuInstances`.
pub struct WeaponDebrisAssets {
    pub pack_id: String,
    pub shell_scene: SceneData,
    shell: ShellDefinition,
}
impl WeaponDebrisAssets {
    pub fn load(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let bytes = crate::materials::read_resource(&root, "pack.json", 8 * 1024 * 1024)?;
        let pack: Pack = serde_json::from_slice(&bytes)?;
        ensure!(
            pack.schema_version == 1 && pack.id == "v20.weapon-debris.001",
            "unsupported debris pack"
        );
        ensure!(
            valid_hash(&pack.weapons_sha256),
            "invalid weapons identity hash"
        );
        ensure!(
            pack.models.len() == 2 && pack.textures.len() == 2,
            "unexpected debris resource inventory"
        );
        validate_shell(&pack.shell)?;
        ensure!(
            pack.models.iter().all(|m| valid_hash(&m.native_sha256)
                && bri_content::brick_materials::safe_relative(&m.file))
                && pack.textures.iter().all(|t| valid_hash(&t.source_sha256)
                    && bri_content::brick_materials::safe_relative(&t.file)),
            "invalid debris resource path or hash"
        );
        ensure!(
            pack.models
                .iter()
                .filter(|m| m.id == pack.shell.model)
                .count()
                == 1,
            "shell model reference must resolve exactly once"
        );
        ensure!(
            pack.textures
                .iter()
                .map(|t| t.width as u64 * t.height as u64)
                .sum::<u64>()
                <= 16 * 1024 * 1024,
            "debris decoded texture budget exceeded"
        );
        let mut textures = BTreeMap::new();
        for resource in &pack.textures {
            ensure!(
                resource.width > 0
                    && resource.height > 0
                    && resource.width <= 4096
                    && resource.height <= 4096,
                "invalid debris texture dimensions"
            );
            let bytes = crate::materials::read_resource(&root, &resource.file, 16 * 1024 * 1024)?;
            ensure!(
                sha(&bytes) == resource.source_sha256,
                "debris texture checksum mismatch: {}",
                resource.id
            );
            let reader =
                image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
            ensure!(
                reader.into_dimensions()? == (resource.width, resource.height),
                "debris texture dimensions changed"
            );
            let image = image::load_from_memory(&bytes)?.to_rgba8();
            textures.insert(
                resource.id.clone(),
                SceneImage {
                    label: resource.id.clone(),
                    width: resource.width,
                    height: resource.height,
                    rgba: image.into_raw(),
                    srgb: true,
                },
            );
        }
        let mut vertex_budget = 0usize;
        let mut shapes = BTreeMap::new();
        for resource in &pack.models {
            let bytes = crate::materials::read_resource(&root, &resource.file, 32 * 1024 * 1024)?;
            ensure!(
                sha(&bytes) == resource.native_sha256,
                "debris model checksum mismatch: {}",
                resource.id
            );
            let shape: Shape = serde_json::from_slice(&bytes)?;
            shape.validate()?;
            vertex_budget += shape
                .meshes
                .iter()
                .flatten()
                .map(|m| m.positions.len())
                .sum::<usize>();
            ensure!(
                vertex_budget <= 2_000_000,
                "debris geometry budget exceeded"
            );
            shapes.insert(resource.id.clone(), shape);
        }
        let shape = shapes
            .get(&pack.shell.model)
            .context("shell model missing from pack")?;
        let shell_resource = pack
            .models
            .iter()
            .find(|m| m.id == pack.shell.model)
            .context("shell model missing from pack")?;
        let pose = sample(shape, None, 0.)?;
        let mut scene = SceneData {
            id: shell_resource.id.clone(),
            name: "Original v20 gunShell DTS".into(),
            ..Default::default()
        };
        let mut texture_ids = BTreeMap::new();
        for (id, image) in textures {
            texture_ids.insert(id, scene.images.len());
            scene.images.push(image);
        }
        let mut bindings = Vec::new();
        for material in &shape.materials {
            // The original casing uses its authored black50/yellow reflectance
            // slots. They are retained as texture images; the generic runtime
            // treats them as source color maps because the renderer has no cubemap.
            let image_id = match material.name.to_ascii_lowercase().as_str() {
                "black50" => "gun_black50",
                "yellow" => "gun_yellow",
                name => anyhow::bail!("unbound original shell material {name}"),
            };
            let image = *texture_ids
                .get(image_id)
                .with_context(|| format!("missing source texture {image_id}"))?;
            let mut native =
                Material::vertex_lit(format!("weapon-debris/{}", material.name), image);
            if material.blend != "opaque" {
                native.alpha = AlphaMode::Blend;
            }
            bindings.push(scene.materials.len());
            scene.materials.push(native);
        }
        let fallback = scene.materials.len();
        scene
            .materials
            .push(Material::vertex_lit("unassigned original shell face", 0));
        let detail = shape
            .details
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.collision)
            .max_by(|(_, a), (_, b)| a.pixel_threshold.total_cmp(&b.pixel_threshold))
            .map(|(i, _)| i)
            .context("shell model has no visible detail")?;
        scene.append_shape(
            ShapeInstance {
                shape,
                pose: &pose,
                detail,
                transform: Mat4::IDENTITY,
                materials: &bindings,
                translucent_materials: None,
                unassigned_material: fallback,
            },
            |_| Some([1.; 4]),
        )?;
        scene.validate()?;
        Ok(Self {
            pack_id: pack.id,
            shell_scene: scene,
            shell: pack.shell,
        })
    }
}

/// An Add-On casing's motion from its `DebrisData` and its image's shell
/// fields, held to the ranges the stock shell is.
fn add_on_shell(model: &str, casing: &bri_weapons::debris::Casing) -> ShellDefinition {
    let d = &casing.debris;
    let finite = |v: f32, default: f32| if v.is_finite() { v } else { default };
    let direction = Vec3::from_array(casing.exit_direction);
    let spin = d.spin.map(|v| finite(v, 0.).clamp(-100_000., 100_000.));
    ShellDefinition {
        model: model.to_owned(),
        lifetime_seconds: finite(d.lifetime, 3.).clamp(0.01, 30.),
        min_spin_degrees_per_second: spin[0].min(spin[1]),
        max_spin_degrees_per_second: spin[0].max(spin[1]),
        elasticity: finite(d.elasticity, 0.3).clamp(0., 1.),
        friction: finite(d.friction, 0.2).clamp(0., 1.),
        bounces: d.bounces.min(32),
        static_on_max_bounce: d.static_on_max_bounce,
        snap_on_max_bounce: d.snap_on_max_bounce,
        fade: d.fade,
        gravity_multiplier: finite(d.gravity, 1.).clamp(0., 20.),
        exit_direction: if direction.is_finite() && direction.length_squared() > 1e-8 {
            casing.exit_direction
        } else {
            [1., 1., 0.]
        },
        exit_offset: casing.exit_offset.map(|v| finite(v, 0.).clamp(-100., 100.)),
        exit_variance_degrees: finite(casing.exit_variance, 20.).clamp(0., 180.),
        velocity: finite(casing.velocity, 1.).clamp(0., 200.),
    }
}

#[cfg(test)]
impl WeaponDebrisAssets {
    /// The stock shell's motion with no model, for tests without content.
    fn stock_for_test() -> Self {
        Self {
            pack_id: "test".into(),
            shell_scene: SceneData::default(),
            shell: ShellDefinition {
                model: "v20.weapon_debris.gun_shell".into(),
                lifetime_seconds: 2.,
                min_spin_degrees_per_second: -400.,
                max_spin_degrees_per_second: 200.,
                elasticity: 0.5,
                friction: 0.2,
                bounces: 3,
                static_on_max_bounce: true,
                snap_on_max_bounce: false,
                fade: true,
                gravity_multiplier: 2.,
                exit_direction: [1., 1., 1.3],
                exit_offset: [0.; 3],
                exit_variance_degrees: 15.,
                velocity: 7.,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebrisInstance {
    pub cue_id: u64,
    pub transform: Mat4,
    pub tint: [f32; 4],
}
#[derive(Clone, Copy, Debug)]
pub struct DebrisHit {
    /// Fraction along the tested segment, in `[0,1]`.
    pub fraction: f32,
    pub normal: Vec3,
}
struct Body {
    cue_id: u64,
    /// An Add-On casing's index in `WeaponDebris::casings`, or the stock
    /// shell.
    casing: Option<usize>,
    position: Vec3,
    velocity: Vec3,
    rotation: Quat,
    spin_axis: Vec3,
    spin: f32,
    age: f32,
    bounces_left: u32,
    settled: bool,
}
struct Pending {
    cue: Cue,
    queued_at: f64,
    inherited_velocity: Vec3,
}

/// Per-session deterministic shell bodies. The owner must feed ordered reliable
/// `WeaponShell` cues exactly once and provide poses from its animated image.
pub struct WeaponDebris {
    assets: WeaponDebrisAssets,
    /// Images whose casing has its own model, and how they throw it.
    casings: Vec<ShellDefinition>,
    by_image: BTreeMap<String, usize>,
    limits: WeaponDebrisLimits,
    bodies: BTreeMap<u64, Body>,
    pending: VecDeque<Pending>,
    cursor: u64,
    seconds: f64,
    accumulator: f64,
    pub diagnostics: WeaponDebrisDiagnostics,
}
impl WeaponDebris {
    pub fn new(assets: WeaponDebrisAssets, limits: WeaponDebrisLimits) -> Result<Self> {
        ensure!(
            (1..=MAX_BODIES).contains(&limits.bodies)
                && (1..=MAX_PENDING).contains(&limits.pending),
            "invalid weapon debris limits"
        );
        validate_shell(&assets.shell)?;
        Ok(Self {
            assets,
            casings: Vec::new(),
            by_image: BTreeMap::new(),
            limits,
            bodies: BTreeMap::new(),
            pending: VecDeque::new(),
            cursor: 0,
            seconds: 0.,
            accumulator: 0.,
            diagnostics: Default::default(),
        })
    }
    pub fn reset(&mut self, checkpoint_cue_cursor: u64) {
        self.bodies.clear();
        self.pending.clear();
        self.cursor = checkpoint_cue_cursor;
        self.seconds = 0.;
        self.accumulator = 0.;
        self.diagnostics = Default::default();
    }
    pub fn clear(&mut self) {
        self.reset(0);
    }
    pub fn cue_cursor(&self) -> u64 {
        self.cursor
    }
    pub fn assets(&self) -> &WeaponDebrisAssets {
        &self.assets
    }
    pub fn active_count(&self) -> usize {
        self.bodies.len()
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    /// Images whose `casing` (`bri_weapons::debris::casings`) has a model
    /// of its own that `has_model` can draw throw it, with its own motion;
    /// others (the base game's `gunShellDebris`, a model that did not
    /// convert) throw the stock shell. Returns notes on what was left out.
    pub fn set_casings(
        &mut self,
        pack: &bri_weapons::Pack,
        has_model: impl Fn(&str) -> bool,
    ) -> Vec<String> {
        let mut notes = Vec::new();
        self.casings.clear();
        self.by_image.clear();
        // Casings thrown under the old list go with it.
        self.bodies.retain(|_, b| b.casing.is_none());
        for (image, casing) in bri_weapons::debris::casings(pack) {
            let model = casing.debris.model.replace('\\', "/").to_ascii_lowercase();
            if model.is_empty() || model == STOCK_SHELL_MODEL {
                continue;
            }
            if !has_model(&model) {
                notes.push(format!(
                    "{image}: casing model {model} is not loaded; it throws the stock shell"
                ));
                continue;
            }
            if self.casings.len() == MAX_CASINGS {
                notes.push(format!(
                    "{image}: more than {MAX_CASINGS} casings; it throws the stock shell"
                ));
                continue;
            }
            self.casings.push(add_on_shell(&model, &casing));
            let index = self.casings.len() - 1;
            self.by_image.insert(image, index);
        }
        notes
    }
    fn shell(&self, body: &Body) -> &ShellDefinition {
        body.casing
            .and_then(|i| self.casings.get(i))
            .unwrap_or(&self.assets.shell)
    }
    fn fade(&self, body: &Body) -> f32 {
        let d = self.shell(body);
        if d.fade {
            (1. - body.age / d.lifetime_seconds).clamp(0., 1.)
        } else {
            1.
        }
    }
    /// Stock shells, drawn from `WeaponDebrisAssets::shell_scene`.
    pub fn instances(&self) -> impl Iterator<Item = DebrisInstance> + '_ {
        self.bodies
            .values()
            .filter(|b| b.casing.is_none())
            .map(|b| DebrisInstance {
                cue_id: b.cue_id,
                transform: Mat4::from_rotation_translation(b.rotation, b.position),
                tint: [1., 1., 1., self.fade(b)],
            })
    }
    /// Add-On casings: model key, transform and tint, for the item
    /// renderer (`WorldItems::set_loose`).
    pub fn model_instances(&self) -> impl Iterator<Item = (String, Mat4, [f32; 4])> + '_ {
        self.bodies.values().filter_map(|b| {
            let d = self.casings.get(b.casing?)?;
            Some((
                d.model.clone(),
                Mat4::from_rotation_translation(b.rotation, b.position),
                [1., 1., 1., self.fade(b)],
            ))
        })
    }
    /// Missing poses remain pending for at most 0.5 cosmetic seconds. `pose`
    /// must resolve `(actor,image,hand)` to the exact authored shell-eject node.
    pub fn cues(
        &mut self,
        cues: &[Cue],
        mut pose: impl FnMut(u64, &str, u8) -> Option<Mat4>,
        mut velocity: impl FnMut(u64) -> Vec3,
    ) -> Result<()> {
        let mut previous = None;
        for cue in cues {
            cue.validate()?;
            ensure!(
                previous.is_none_or(|last| cue.id > last),
                "unordered weapon debris cues"
            );
            previous = Some(cue.id);
        }
        for cue in cues {
            let CueKind::WeaponShell { actor, image, hand } = &cue.kind else {
                continue;
            };
            if cue.id <= self.cursor {
                self.diagnostics.duplicate_cues += 1;
                continue;
            }
            ensure!(cue.id > 0, "invalid shell cue id");
            self.cursor = cue.id;
            let inherited = velocity(*actor);
            if !inherited.is_finite() {
                self.diagnostics.invalid_cues += 1;
                continue;
            }
            if let Some(transform) = pose(*actor, image, *hand) {
                self.spawn(cue, transform, inherited)?;
            } else if self.pending.len() < self.limits.pending {
                self.pending.push_back(Pending {
                    cue: cue.clone(),
                    queued_at: self.seconds,
                    inherited_velocity: inherited,
                });
            } else {
                self.diagnostics.pending_capacity_drops += 1;
            }
            self.diagnostics.accepted_cues += 1;
        }
        Ok(())
    }
    fn spawn(&mut self, cue: &Cue, eject: Mat4, inherited: Vec3) -> Result<()> {
        ensure!(
            eject.is_finite() && eject.determinant() > 1e-8 && eject.w_axis.w == 1.,
            "invalid shell-eject transform"
        );
        if self.bodies.len() >= self.limits.bodies {
            self.diagnostics.body_capacity_drops += 1;
            return Ok(());
        }
        let casing = match &cue.kind {
            CueKind::WeaponShell { image, .. } => self.by_image.get(image).copied(),
            _ => None,
        };
        let d = casing
            .and_then(|i| self.casings.get(i))
            .unwrap_or(&self.assets.shell);
        let mut rng = Deterministic(cue.id ^ 0x9e37_79b9_7f4a_7c15);
        let dir = Vec3::from_array(d.exit_direction).normalize();
        // Engine-family assumption: shellExitVariance is a symmetric azimuthal
        // spread around the authored local ejection vector; Torque source only
        // exposes the scalar parameter, not the proprietary sampling routine.
        let yaw = (rng.unit() * 2. - 1.) * d.exit_variance_degrees.to_radians();
        let local = Quat::from_rotation_y(yaw) * dir;
        let direction = eject.transform_vector3(local).normalize();
        let position = eject.transform_point3(Vec3::from_array(d.exit_offset));
        let speed = d.velocity;
        let variation = (rng.unit() * 2. - 1.) * 0.08;
        let spin = (d.min_spin_degrees_per_second
            + rng.unit() * (d.max_spin_degrees_per_second - d.min_spin_degrees_per_second))
            .to_radians();
        let spin_axis = Vec3::new(rng.signed(), rng.signed(), rng.signed()).normalize_or_zero();
        let start = eject.to_scale_rotation_translation().1;
        self.bodies.insert(
            cue.id,
            Body {
                cue_id: cue.id,
                casing,
                position,
                velocity: direction * (speed * (1. + variation)) + inherited,
                rotation: start,
                spin_axis,
                spin,
                age: 0.,
                bounces_left: d.bounces,
                settled: false,
            },
        );
        Ok(())
    }
    /// Advance at a fixed 120 Hz with bounded frame catch-up. Collision is a
    /// caller-supplied cosmetic sweep; gameplay and authoritative physics stay
    /// in the host. `pose` retries deferred image attachments.
    pub fn advance(
        &mut self,
        dt: f32,
        mut pose: impl FnMut(u64, &str, u8) -> Option<Mat4>,
        mut sweep: impl FnMut(Vec3, Vec3) -> Option<DebrisHit>,
    ) -> Result<()> {
        ensure!(
            dt.is_finite() && (0. ..=10.).contains(&dt),
            "invalid debris frame delta"
        );
        let applied = f64::from(dt).min(MAX_FRAME_SECONDS);
        if f64::from(dt) > MAX_FRAME_SECONDS {
            self.diagnostics.capped_frame_time += 1;
        }
        self.seconds += applied;
        self.accumulator += applied;
        let mut pending = std::mem::take(&mut self.pending);
        while let Some(item) = pending.pop_front() {
            if let CueKind::WeaponShell { actor, image, hand } = &item.cue.kind {
                if let Some(transform) = pose(*actor, image, *hand) {
                    self.spawn(&item.cue, transform, item.inherited_velocity)?;
                } else if self.seconds - item.queued_at >= PENDING_SECONDS {
                    self.diagnostics.missing_pose_expired += 1;
                } else if self.pending.len() < self.limits.pending {
                    self.pending.push_back(item);
                } else {
                    self.diagnostics.pending_capacity_drops += 1;
                }
            }
        }
        let steps = (self.accumulator / FIXED_STEP).floor() as usize;
        self.accumulator -= steps as f64 * FIXED_STEP;
        let stock = self.assets.shell.clone();
        let casings = self.casings.clone();
        for _ in 0..steps {
            let dt = FIXED_STEP as f32;
            let ids: Vec<_> = self.bodies.keys().copied().collect();
            for id in ids {
                let Some(body) = self.bodies.get_mut(&id) else {
                    continue;
                };
                let d = body.casing.and_then(|i| casings.get(i)).unwrap_or(&stock);
                body.age += dt;
                if body.age >= d.lifetime_seconds {
                    self.bodies.remove(&id);
                    continue;
                }
                if !body.settled {
                    body.velocity.y -= 9.81 * d.gravity_multiplier * dt;
                    let from = body.position;
                    let to = from + body.velocity * dt;
                    self.diagnostics.collision_queries += 1;
                    if let Some(hit) = sweep(from, to) {
                        // A ray that starts inside a brick reports a zero
                        // normal (normalized to NaN). A cosmetic casing is
                        // never worth the game: drop it and note it once.
                        if !(hit.fraction.is_finite()
                            && (0. ..=1.).contains(&hit.fraction)
                            && hit.normal.is_finite()
                            && (hit.normal.length_squared() - 1.).abs() < 0.01)
                        {
                            if self.diagnostics.invalid_collisions == 0 {
                                eprintln!(
                                    "Weapon debris: dropped a casing on an invalid collision \
                                     (fraction {}, normal {})",
                                    hit.fraction, hit.normal
                                );
                            }
                            self.diagnostics.invalid_collisions += 1;
                            self.bodies.remove(&id);
                            continue;
                        }
                        body.position = from.lerp(to, hit.fraction.clamp(0., 1.));
                        let normal = hit.normal.normalize();
                        let vn = body.velocity.dot(normal);
                        if vn < 0. {
                            let normal_v = normal * vn;
                            let tangent = body.velocity - normal_v;
                            body.velocity = tangent * (1. - d.friction) - normal_v * d.elasticity;
                        }
                        body.bounces_left = body.bounces_left.saturating_sub(1);
                        self.diagnostics.bounces += 1;
                        if body.bounces_left == 0 && d.static_on_max_bounce {
                            body.settled = true;
                            body.velocity = Vec3::ZERO;
                            self.diagnostics.settled += 1;
                        }
                    } else {
                        body.position = to;
                    }
                    body.rotation = (Quat::from_axis_angle(body.spin_axis, body.spin * dt)
                        * body.rotation)
                        .normalize();
                }
            }
        }
        Ok(())
    }
}
struct Deterministic(u64);
impl Deterministic {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f32 {
        ((self.next() >> 40) as f32) / (1u32 << 24) as f32
    }
    fn signed(&mut self) -> f32 {
        self.unit() * 2. - 1.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
    fn cue(id: u64) -> Cue {
        Cue {
            id,
            tick: id,
            kind: CueKind::WeaponShell {
                actor: 7,
                image: "v20.image.gunimage".into(),
                hand: 0,
            },
            position: [0.; 3],
        }
    }
    /// The debris pack a test runs on: the made-up one
    /// (`crate::testing::weapon_debris`) or the generated v20 one.
    struct Debris {
        content: bool,
    }
    impl Debris {
        fn synthetic() -> Result<Self> {
            Ok(Self { content: false })
        }
        fn content() -> Result<Self> {
            Ok(Self { content: true })
        }
        fn assets(&self) -> Result<WeaponDebrisAssets> {
            if !self.content {
                return crate::testing::weapon_debris::assets();
            }
            let pack = bri_package::packages::PackageSet::base()
                .role("weapon_debris")?
                .dir
                .clone();
            WeaponDebrisAssets::load(&root().join("content").join(pack))
        }
    }
    crate::testing::synthetic_and_content!(
        Debris: native_shell_model_and_texture_bindings_load_without_substitutes,
        cue_delivery_is_once_deferred_pose_can_resolve_and_disconnect_resets,
        a_zero_normal_hit_drops_the_casing_instead_of_failing,
        unresolved_pose_expires_and_native_bounces_are_bounded
    );
    fn malformed_pack(root: &Path, shell_patch: serde_json::Value, model_file: &str) {
        std::fs::create_dir_all(root).unwrap();
        let mut shell = serde_json::json!({"model":"v20.weapon_debris.gun_shell","lifetime_seconds":2.0,
            "min_spin_degrees_per_second":-400.0,"max_spin_degrees_per_second":200.0,"elasticity":0.5,
            "friction":0.2,"bounces":3,"static_on_max_bounce":true,"snap_on_max_bounce":false,
            "fade":true,"gravity_multiplier":2.0,"exit_direction":[1.0,1.0,1.3],
            "exit_offset":[0.0,0.0,0.0],"exit_variance_degrees":15.0,"velocity":7.0});
        if let (Some(dst), Some(src)) = (shell.as_object_mut(), shell_patch.as_object()) {
            for (k, v) in src {
                dst.insert(k.clone(), v.clone());
            }
        }
        let hash = "0".repeat(64);
        let pack = serde_json::json!({"schema_version":1,"id":"v20.weapon-debris.001","weapons_sha256":hash,
            "models":[{"id":"v20.weapon_debris.gun_shell","file":model_file,"native_sha256":hash},
                {"id":"v20.weapon_debris.rocket_explosion_sphere","file":"models/rocket.json","native_sha256":hash}],
            "textures":[{"id":"gun_black50","file":"textures/black.png","source_sha256":hash,"width":16,"height":16},
                {"id":"gun_yellow","file":"textures/yellow.png","source_sha256":hash,"width":16,"height":16}],"shell":shell});
        std::fs::write(root.join("pack.json"), serde_json::to_vec(&pack).unwrap()).unwrap();
    }
    fn scratch() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "weapon-debris-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn malformed_pack_rejects_escaping_paths_before_resource_reads() {
        let root = scratch();
        malformed_pack(&root, serde_json::json!({}), "../outside.json");
        let error = WeaponDebrisAssets::load(&root).err().unwrap().to_string();
        assert!(error.contains("invalid debris resource path"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_pack_rejects_out_of_range_physics_before_resource_reads() {
        let root = scratch();
        malformed_pack(
            &root,
            serde_json::json!({"gravity_multiplier":99.0}),
            "models/shell.json",
        );
        let error = WeaponDebrisAssets::load(&root).err().unwrap().to_string();
        assert!(error.contains("invalid native shell definition"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_pack_manifest_is_byte_bounded() {
        let root = scratch();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("pack.json"), vec![b' '; 8 * 1024 * 1024 + 1]).unwrap();
        let error = WeaponDebrisAssets::load(&root).err().unwrap().to_string();
        assert!(error.contains("Oversized native brick resource"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }

    fn native_shell_model_and_texture_bindings_load_without_substitutes(fx: &Debris) -> Result<()> {
        let assets = fx.assets()?;
        assert_eq!(assets.pack_id, "v20.weapon-debris.001");
        assert!(!assets.shell_scene.vertices.is_empty());
        assert!(!assets.shell_scene.indices.is_empty());
        assert!(assets.shell_scene.images.len() >= 2);
        assert!(
            assets
                .shell_scene
                .images
                .iter()
                .any(|i| i.label == "gun_black50")
        );
        assert!(
            assets
                .shell_scene
                .images
                .iter()
                .any(|i| i.label == "gun_yellow")
        );
        assert!(
            assets
                .shell_scene
                .batches
                .iter()
                .all(|b| b.material < assets.shell_scene.materials.len())
        );
        Ok(())
    }

    fn cue_delivery_is_once_deferred_pose_can_resolve_and_disconnect_resets(
        fx: &Debris,
    ) -> Result<()> {
        let mut world = WeaponDebris::new(fx.assets()?, WeaponDebrisLimits::default())?;
        world.cues(&[cue(1)], |_, _, _| None, |_| Vec3::ZERO)?;
        assert_eq!(world.pending.len(), 1);
        assert_eq!(world.cue_cursor(), 1);
        world.cues(&[cue(1)], |_, _, _| Some(Mat4::IDENTITY), |_| Vec3::ZERO)?;
        assert_eq!(world.diagnostics.duplicate_cues, 1);
        world.advance(1. / 60., |_, _, _| Some(Mat4::IDENTITY), |_, _| None)?;
        assert_eq!(world.instances().count(), 1);
        let before = world.instances().next().unwrap();
        let mut same = WeaponDebris::new(fx.assets()?, WeaponDebrisLimits::default())?;
        same.cues(&[cue(1)], |_, _, _| Some(Mat4::IDENTITY), |_| Vec3::ZERO)?;
        same.advance(1. / 60., |_, _, _| Some(Mat4::IDENTITY), |_, _| None)?;
        let after = same.instances().next().unwrap();
        assert_eq!(
            before.transform.to_cols_array().map(f32::to_bits),
            after.transform.to_cols_array().map(f32::to_bits)
        );
        world.clear();
        assert_eq!(world.instances().count(), 0);
        assert_eq!(world.cue_cursor(), 0);
        Ok(())
    }

    fn a_zero_normal_hit_drops_the_casing_instead_of_failing(fx: &Debris) -> Result<()> {
        // a16 multiplayer crash: a casing ejected inside a brick got a hit
        // whose zero normal normalized to NaN, and advance returned an error
        // that closed the game.
        let mut world = WeaponDebris::new(fx.assets()?, WeaponDebrisLimits::default())?;
        world.cues(&[cue(1)], |_, _, _| Some(Mat4::IDENTITY), |_| Vec3::ZERO)?;
        world.advance(
            1. / 60.,
            |_, _, _| None,
            |_, _| {
                Some(DebrisHit {
                    fraction: 0.,
                    normal: Vec3::ZERO.normalize(),
                })
            },
        )?;
        assert_eq!(world.instances().count(), 0);
        assert_eq!(world.diagnostics.invalid_collisions, 1);
        Ok(())
    }

    fn unresolved_pose_expires_and_native_bounces_are_bounded(fx: &Debris) -> Result<()> {
        let mut world = WeaponDebris::new(fx.assets()?, WeaponDebrisLimits::default())?;
        world.cues(&[cue(9)], |_, _, _| None, |_| Vec3::ZERO)?;
        world.advance(0.25, |_, _, _| None, |_, _| None)?;
        world.advance(0.25, |_, _, _| None, |_, _| None)?;
        assert_eq!(world.diagnostics.missing_pose_expired, 1);
        world.cues(
            &[cue(10)],
            |_, _, _| Some(Mat4::from_translation(Vec3::Y)),
            |_| Vec3::ZERO,
        )?;
        for _ in 0..100 {
            world.advance(
                1. / 60.,
                |_, _, _| None,
                |a, b| {
                    if a.y > 0. && b.y <= 0. {
                        Some(DebrisHit {
                            fraction: (a.y / (a.y - b.y)).clamp(0., 1.),
                            normal: Vec3::Y,
                        })
                    } else {
                        None
                    }
                },
            )?;
        }
        let bounces = fx.assets()?.shell.bounces;
        assert!(world.diagnostics.bounces <= u64::from(bounces));
        assert!(world.diagnostics.settled <= 1);
        assert!(world.instances().count() <= 1);
        Ok(())
    }

    fn kit() -> bri_weapons::Pack {
        let def = |name: &str, class: &str, fields: serde_json::Value| {
            serde_json::json!({
                "name": name, "class": class, "parent": null,
                "source": { "path": "Add-Ons/Weapon_Kit/kit.cs", "sha256": "0".repeat(64), "line": 1 },
                "fields": fields,
            })
        };
        let json = serde_json::json!({
            "schema_version": 3, "id": "kit", "items": {},
            "images": {
                "kit:image/gun": { "name": "kitGunImage", "casing": "kitShellDebris",
                    "states": [{ "name": "Ready" }] },
                "kit:image/rifle": { "name": "kitRifleImage", "casing": "gunShellDebris",
                    "states": [{ "name": "Ready" }] },
                "kit:image/lost": { "name": "kitLostImage", "casing": "kitLostDebris",
                    "states": [{ "name": "Ready" }] }
            },
            "definitions": [
                def("kitShellDebris", "DebrisData", serde_json::json!({
                    "shapefile": "\"./shell.dts\"", "lifetime": "1.5", "numbounces": "2",
                    "gravmodifier": "0", "fade": "false" })),
                def("kitLostDebris", "DebrisData", serde_json::json!({
                    "shapefile": "\"./missing.dts\"" })),
                def("gunShellDebris", "DebrisData", serde_json::json!({
                    "shapefile": "\"./gunshell.dts\"" })),
                def("kitGunImage", "ShapeBaseImageData", serde_json::json!({
                    "shellexitdir": "\"0 0 1\"", "shellvelocity": "4",
                    "shellexitvariance": "0" })),
            ]
        });
        let mut json = json;
        // The base game's casing lives in Weapon_Gun.
        json["definitions"][2]["source"]["path"] = "Add-Ons/Weapon_Gun/server.cs".into();
        bri_weapons::Pack::from_json(&serde_json::to_vec(&json).unwrap()).unwrap()
    }
    fn shell_cue(id: u64, image: &str) -> Cue {
        Cue {
            id,
            tick: id,
            kind: CueKind::WeaponShell {
                actor: 7,
                image: image.into(),
                hand: 0,
            },
            position: [0.; 3],
        }
    }
    /// An Add-On's casing with its own model flies as its `DebrisData` and
    /// image say and is drawn by its model; the base game's casing, and one
    /// whose model is not loaded, throw the stock shell.
    #[test]
    fn an_add_on_casing_throws_its_own_model_and_motion() -> Result<()> {
        let mut world = WeaponDebris::new(
            WeaponDebrisAssets::stock_for_test(),
            WeaponDebrisLimits::default(),
        )?;
        let notes = world.set_casings(&kit(), |m| m == "add-ons/weapon_kit/shell.dts");
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("missing.dts"));
        let cues = [
            shell_cue(1, "kit:image/gun"),
            shell_cue(2, "kit:image/rifle"),
            shell_cue(3, "kit:image/lost"),
        ];
        world.cues(&cues, |_, _, _| Some(Mat4::IDENTITY), |_| Vec3::ZERO)?;
        assert_eq!(
            world.instances().count(),
            2,
            "rifle and lost throw the stock shell"
        );
        let casings: Vec<_> = world.model_instances().collect();
        assert_eq!(casings.len(), 1);
        assert_eq!(casings[0].0, "add-ons/weapon_kit/shell.dts");
        // Straight up at shellVelocity, with no gravity and no fade.
        let start = casings[0].1.w_axis.truncate();
        world.advance(0.25, |_, _, _| None, |_, _| None)?;
        let (_, moved, tint) = world.model_instances().next().unwrap();
        let rise = moved.w_axis.truncate() - start;
        assert!(rise.x.abs() < 1e-4 && rise.z.abs() < 1e-4, "{rise}");
        assert!((rise.y / 0.25 - 4.0).abs() < 4.0 * 0.09, "{rise}");
        assert_eq!(tint[3], 1.0);
        // Its 1.5 s lifetime ends before the stock shell's 2 s.
        for _ in 0..5 {
            world.advance(0.25, |_, _, _| None, |_, _| None)?;
        }
        world.advance(0.05, |_, _, _| None, |_, _| None)?;
        assert_eq!(world.model_instances().count(), 0);
        assert_eq!(world.instances().count(), 2);
        // Replacing the list forgets casings thrown under the old one.
        world.cues(
            &[shell_cue(4, "kit:image/gun")],
            |_, _, _| Some(Mat4::IDENTITY),
            |_| Vec3::ZERO,
        )?;
        world.set_casings(&kit(), |_| false);
        assert_eq!(world.model_instances().count(), 0);
        Ok(())
    }
}
