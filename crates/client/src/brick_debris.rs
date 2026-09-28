//! Brick debris: v20's client-side PhysX "brick explosion".
//!
//! Every `BrickKill` cue (hammer, Destructo Wand, `fakeKillBrick`, brick
//! explosions) turns the dead brick into a short-lived Rapier rigid body. It
//! is thrown away from the blast origin, tumbles against the map, terrain,
//! nearby bricks and other debris, then fades out like a ghost. Debris is
//! purely cosmetic, like particles: the server already hid or removed the
//! brick, nothing about the bodies is sent over the network, and nothing in
//! gameplay can see them. Each body's throw is seeded from its cue id, so
//! every client sees the same throw.
//!
//! Players and vehicles as this client draws them (see [`Pusher`]),
//! projectiles and later blasts push the bodies, one way only: pushers are
//! kinematic, so debris can never slow, block or move them.
//!
//! Like v20's `$Physics::maxBricks`, only a bounded number of bodies are
//! alive at once; the oldest make way for new ones.
use crate::building::Building;
use crate::world_chunks::BrickPalette;
use anyhow::{Context, Result, ensure};
use bri_net::protocol::PublicWorld;
use bri_render::scene::{GpuInstances, GpuScene, SceneData, SceneRenderer, SceneTransform};
use bri_sim::presentation::{Cue, CueKind};
use bri_world::{BrickId, ContentRef};
use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// v20's default `$pref::Physics::MaxBricks` is 100; leave room for a few
/// overlapping explosions.
pub const MAX_BODIES: usize = 128;
/// Fixed physics step, like the rest of the game.
const STEP: f32 = bri_physics::FIXED_DT;
/// Steps per frame before debris time is dropped instead of catching up.
const MAX_STEPS: u32 = 4;
/// Torque's world gravity (units/s^2), as the player and items use.
const GRAVITY: f32 = 20.0;
/// Seconds a body stays solid before it starts to fade: long enough to
/// kick it around.
const SOLID_SECONDS: f32 = 3.0;
/// Seconds of fading to fully transparent, after which the body is removed.
const FADE_SECONDS: f32 = 2.0;
/// Converts v20 blast force into launch speed (units/s).
const FORCE_TO_SPEED: f32 = 0.5;
const MAX_SPEED: f32 = 40.0;
/// Bodies are a hair smaller than the brick so neighbours killed together
/// don't start out interpenetrating.
const BODY_SHRINK: f32 = 0.96;
/// Distance around each body where surroundings are made solid.
const SURROUNDINGS: f32 = 2.0;
/// Debris only feels pushers within this distance.
const PUSHER_REACH: f32 = 6.0;
/// Most players and vehicles pushing debris at once.
const MAX_PUSHERS: usize = 32;
/// A pusher that jumps further than this in a frame teleported.
const TELEPORT: f32 = 5.0;
/// Momentum a projectile gives each body it passes, per unit of speed.
const PROJECTILE_MASS: f32 = 0.2;
/// Mass per cubic unit of debris: a 2x4 brick weighs 6.
const DENSITY: f32 = 5.0;
/// Grid used to cache terrain patches.
const TERRAIN_CHUNK: f32 = 8.0;
/// Distinct brick looks kept on the GPU.
const MAX_LOOKS: usize = 64;

/// What a dead brick looks like; bodies with the same look share one model.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub definition: String,
    pub color: u8,
    pub color_effect: u8,
    pub shape_effect: u8,
    pub print: Option<ContentRef>,
}
impl Look {
    fn key(&self) -> (&str, u8, u8, u8, Option<String>) {
        (
            &self.definition,
            self.color,
            self.color_effect,
            self.shape_effect,
            self.print.as_ref().map(|p| format!("{p:?}")),
        )
    }
}
impl Eq for Look {}
impl PartialOrd for Look {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Look {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

struct Body {
    handle: RigidBodyHandle,
    brick: BrickId,
    look: Look,
    age: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Static {
    Brick(BrickId),
    Terrain(i32, i32),
}

#[derive(Clone, Debug, Default)]
pub struct BrickDebrisDiagnostics {
    pub accepted: u64,
    pub duplicates: u64,
    /// Cues for bricks this client has no definition for.
    pub unknown: u64,
    /// Oldest bodies removed early to stay within `MAX_BODIES`.
    pub evicted: u64,
    pub dropped_steps: u64,
    pub projectile_hits: u64,
}

/// A player or vehicle as this client draws it this frame: a box that
/// shoves debris out of its way. `id` must stay the same between frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pusher {
    pub id: u64,
    pub center: Vec3,
    pub rotation: Quat,
    pub half: Vec3,
}
/// A projectile as this client draws it this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub id: u64,
    pub position: Vec3,
    pub velocity: Vec3,
}

struct PusherBody {
    handle: RigidBodyHandle,
    /// Where the body is heading this frame.
    from: Pose,
    to: Pose,
}

pub struct BrickDebris {
    world: PhysicsWorld,
    map_loaded: bool,
    /// Keyed by cue id, so iteration runs oldest first.
    bodies: BTreeMap<u64, Body>,
    /// Solid surroundings; `None` marks terrain chunks with no ground.
    statics: HashMap<Static, Option<ColliderHandle>>,
    statics_generation: u64,
    /// Bricks this client saw die that have not come back yet.
    dead: BTreeSet<BrickId>,
    cursor: u64,
    accumulator: f32,
    pushers: BTreeMap<u64, PusherBody>,
    /// Projectile id -> where it was last frame.
    shots: BTreeMap<u64, Vec3>,
    /// (projectile, cue) pairs already pushed.
    struck: BTreeSet<(u64, u64)>,
    pub diagnostics: BrickDebrisDiagnostics,
}

impl Default for BrickDebris {
    fn default() -> Self {
        Self::new()
    }
}

impl BrickDebris {
    pub fn new() -> Self {
        let mut world = bri_physics::new_world();
        world.gravity = Vector::new(0.0, -GRAVITY, 0.0);
        Self {
            world,
            map_loaded: false,
            bodies: BTreeMap::new(),
            statics: HashMap::new(),
            statics_generation: 0,
            dead: BTreeSet::new(),
            cursor: 0,
            accumulator: 0.0,
            pushers: BTreeMap::new(),
            shots: BTreeMap::new(),
            struck: BTreeSet::new(),
            diagnostics: Default::default(),
        }
    }
    /// Forget everything (disconnect, new server).
    pub fn clear(&mut self) {
        *self = Self::new();
    }
    pub fn len(&self) -> usize {
        self.bodies.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }
    /// Whether `brick` died (or was fake-killed) and hasn't come back. Such
    /// bricks are not "hidden" bricks the building tools reveal.
    pub fn is_dead(&self, brick: BrickId) -> bool {
        self.dead.contains(&brick)
    }
    /// Turn new `BrickKill` cues into debris and return how many there were.
    /// Other cues are ignored.
    pub fn cues<'a>(
        &mut self,
        cues: impl IntoIterator<Item = &'a Cue>,
        building: &Building,
    ) -> Result<usize> {
        let mut spawned = 0;
        for cue in cues {
            let CueKind::BrickKill {
                brick,
                definition,
                quarter_turns,
                color,
                color_effect,
                shape_effect,
                print,
                origin,
                force,
                radius,
            } = &cue.kind
            else {
                continue;
            };
            if cue.id <= self.cursor {
                self.diagnostics.duplicates += 1;
                continue;
            }
            cue.validate()?;
            self.cursor = cue.id;
            let ContentRef::Resolved(definition) = definition else {
                unreachable!("validated brick kill definition");
            };
            let Some(half) = building.definition_half_extents(definition) else {
                self.diagnostics.unknown += 1;
                continue;
            };
            self.dead.insert(*brick);
            // A big blast also shoves the debris already flying around it.
            if *radius > 0.5 {
                self.blast(Vec3::from(*origin), *force, *radius);
            }
            // The dead brick must never hold up its own debris.
            if let Some(Some(handle)) = self.statics.remove(&Static::Brick(*brick)) {
                self.world.remove_collider(handle);
            }
            let look = Look {
                definition: definition.clone(),
                color: *color,
                color_effect: *color_effect,
                shape_effect: *shape_effect,
                print: print.clone(),
            };
            self.spawn(
                cue.id,
                *brick,
                look,
                Vec3::from(cue.position),
                *quarter_turns,
                half,
                Vec3::from(*origin),
                *force,
                *radius,
            );
            self.diagnostics.accepted += 1;
            spawned += 1;
        }
        Ok(spawned)
    }
    #[allow(clippy::too_many_arguments)] // one cue's fields
    fn spawn(
        &mut self,
        id: u64,
        brick: BrickId,
        look: Look,
        center: Vec3,
        quarter_turns: u8,
        half: Vec3,
        origin: Vec3,
        force: f32,
        radius: f32,
    ) {
        while self.bodies.len() >= MAX_BODIES {
            let (_, oldest) = self.bodies.pop_first().expect("bodies at capacity");
            self.world.remove_body_with_colliders(oldest.handle, true);
            self.diagnostics.evicted += 1;
        }
        let mut rng = Seeded::new(id);
        let offset = center - origin;
        let distance = offset.length();
        let direction = if distance > 1e-4 {
            offset / distance
        } else {
            Vec3::Y
        };
        // Small blasts (direct hits, events, hammer pops) throw at full force;
        // radius blasts weaken towards their edge.
        let falloff = if radius > 0.5 {
            (1.0 - distance / radius).clamp(0.25, 1.0)
        } else {
            1.0
        };
        let wobble = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 0.25;
        let direction = (direction + wobble).normalize_or(Vec3::Y);
        let speed = (force * FORCE_TO_SPEED * falloff * (0.85 + 0.3 * rng.unit())).min(MAX_SPEED);
        let spin = Vec3::new(rng.signed(), rng.signed(), rng.signed()).normalize_or(Vec3::X)
            * (2.0 + 6.0 * rng.unit());
        let rotation = Rotation::from_scaled_axis(
            Vector::Y * (-f32::from(quarter_turns) * std::f32::consts::FRAC_PI_2),
        );
        let body = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(
                Vector::from_array(center.to_array()),
                rotation,
            ))
            .linvel(Vector::from_array((direction * speed).to_array()))
            .angvel(Vector::from_array(spin.to_array()))
            .linear_damping(0.1)
            .angular_damping(0.4)
            .ccd_enabled(true);
        let h = half * BODY_SHRINK;
        let collider = ColliderBuilder::cuboid(h.x.max(0.01), h.y.max(0.01), h.z.max(0.01))
            .density(DENSITY)
            .friction(0.7)
            .restitution(0.25);
        let (handle, _) = self.world.insert(body, collider);
        self.bodies.insert(
            id,
            Body {
                handle,
                brick,
                look,
                age: 0.0,
            },
        );
    }
    /// Advance debris by `dt` seconds against the current surroundings.
    pub fn advance(&mut self, dt: f32, building: &Building) -> Result<()> {
        ensure!(dt.is_finite() && dt >= 0.0, "Invalid debris frame time");
        if self.bodies.is_empty() {
            self.accumulator = 0.0;
            self.clear_statics();
            for (_, pusher) in std::mem::take(&mut self.pushers) {
                self.world.remove_body_with_colliders(pusher.handle, true);
            }
            return Ok(());
        }
        if !self.map_loaded {
            for collider in building.map_colliders() {
                self.world.insert_collider(collider.clone(), None);
            }
            self.map_loaded = true;
        }
        if self.statics_generation != building.query_generation() {
            // Bricks changed: rebuild the solid surroundings from scratch.
            self.clear_statics();
            self.statics_generation = building.query_generation();
        }
        self.accumulator += dt;
        let mut steps = (self.accumulator / STEP) as u32;
        if steps > MAX_STEPS {
            self.diagnostics.dropped_steps += u64::from(steps - MAX_STEPS);
            self.accumulator = 0.0;
            steps = MAX_STEPS;
        } else {
            self.accumulator -= steps as f32 * STEP;
        }
        if steps == 0 {
            return Ok(());
        }
        self.load_surroundings(building, steps as f32 * STEP)?;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            for pusher in self.pushers.values() {
                self.world.bodies[pusher.handle]
                    .set_next_kinematic_position(pusher.from.lerp(&pusher.to, t));
            }
            self.world.step();
            self.age(STEP);
        }
        for pusher in self.pushers.values_mut() {
            pusher.from = pusher.to;
        }
        Ok(())
    }
    /// Where players and vehicles are this frame. Call before `advance`.
    /// Only those near debris take part; they are kinematic, so debris
    /// moves out of their way and never moves them.
    pub fn push(&mut self, pushers: &[Pusher]) {
        let near = |p: &Pusher| {
            let reach = PUSHER_REACH + p.half.max_element();
            self.bodies.values().any(|b| {
                Vec3::from_array(self.world.bodies[b.handle].translation().to_array())
                    .distance(p.center)
                    < reach
            })
        };
        let mut wanted: Vec<&Pusher> = pushers
            .iter()
            .filter(|p| p.center.is_finite() && p.rotation.is_finite() && p.half.is_finite())
            .filter(|p| near(p))
            .collect();
        wanted.sort_by_key(|p| p.id);
        wanted.dedup_by_key(|p| p.id);
        wanted.truncate(MAX_PUSHERS);
        let ids: BTreeSet<u64> = wanted.iter().map(|p| p.id).collect();
        let gone: Vec<u64> = self
            .pushers
            .keys()
            .copied()
            .filter(|id| !ids.contains(id))
            .collect();
        for id in gone {
            if let Some(p) = self.pushers.remove(&id) {
                self.world.remove_body_with_colliders(p.handle, true);
            }
        }
        for p in wanted {
            let to = Pose::from_parts(
                Vector::from_array(p.center.to_array()),
                Rotation::from_array(p.rotation.normalize().to_array()),
            );
            match self.pushers.get_mut(&p.id) {
                Some(body) => {
                    let jumped = Vec3::from_array(body.to.translation.to_array())
                        .distance(p.center)
                        > TELEPORT;
                    if jumped {
                        self.world.bodies[body.handle].set_position(to, true);
                        body.from = to;
                    }
                    body.to = to;
                }
                None => {
                    let h = p.half.max(Vec3::splat(0.05));
                    let (handle, _) = self.world.insert(
                        RigidBodyBuilder::kinematic_position_based().pose(to),
                        ColliderBuilder::cuboid(h.x, h.y, h.z).friction(0.3),
                    );
                    self.pushers.insert(
                        p.id,
                        PusherBody {
                            handle,
                            from: to,
                            to,
                        },
                    );
                }
            }
        }
    }
    /// Projectiles as drawn this frame. Each pushes every body it passes
    /// through once, and flies on as if the debris were not there.
    pub fn shots(&mut self, shots: &[Shot]) {
        let live: BTreeSet<u64> = shots.iter().map(|s| s.id).collect();
        self.shots.retain(|id, _| live.contains(id));
        self.struck.retain(|(p, _)| live.contains(p));
        if self.bodies.is_empty() {
            return;
        }
        let owners: HashMap<RigidBodyHandle, u64> =
            self.bodies.iter().map(|(id, b)| (b.handle, *id)).collect();
        for shot in shots {
            if !shot.position.is_finite() || !shot.velocity.is_finite() {
                continue;
            }
            let Some(from) = self.shots.insert(shot.id, shot.position) else {
                continue;
            };
            let delta = shot.position - from;
            let length = delta.length();
            if !(1e-5..=100.0).contains(&length) {
                continue;
            }
            let direction = delta / length;
            let ray = Ray::new(
                Vector::from_array(from.to_array()),
                Vector::from_array(direction.to_array()),
            );
            let hits: Vec<(u64, Vec3)> = self
                .world
                .intersect_ray(ray, length, true, QueryFilter::only_dynamic())
                .filter_map(|(_, c, hit)| {
                    let id = *owners.get(&c.parent()?)?;
                    Some((id, from + direction * hit.time_of_impact))
                })
                .collect();
            for (id, point) in hits {
                if !self.struck.insert((shot.id, id)) {
                    continue;
                }
                let rb = &mut self.world.bodies[self.bodies[&id].handle];
                let impulse = direction * PROJECTILE_MASS * shot.velocity.length();
                rb.apply_impulse_at_point(
                    Vector::from_array(impulse.to_array()),
                    Vector::from_array(point.to_array()),
                    true,
                );
                let v = Vec3::from_array(rb.linvel().to_array()).clamp_length_max(MAX_SPEED);
                rb.set_linvel(Vector::from_array(v.to_array()), true);
                self.diagnostics.projectile_hits += 1;
            }
        }
    }
    /// Shove every body within `radius` of `origin` away from it.
    fn blast(&mut self, origin: Vec3, force: f32, radius: f32) {
        if !origin.is_finite() || !force.is_finite() || force <= 0.0 {
            return;
        }
        for body in self.bodies.values() {
            let rb = &mut self.world.bodies[body.handle];
            let at = Vec3::from_array(rb.translation().to_array());
            let offset = at - origin;
            let distance = offset.length();
            if distance > radius {
                continue;
            }
            let direction = offset.normalize_or(Vec3::Y);
            let falloff = (1.0 - distance / radius).clamp(0.25, 1.0);
            let speed = (force * FORCE_TO_SPEED * falloff).min(MAX_SPEED);
            let v = (Vec3::from_array(rb.linvel().to_array()) + direction * speed)
                .clamp_length_max(MAX_SPEED);
            rb.set_linvel(Vector::from_array(v.to_array()), true);
        }
    }
    fn age(&mut self, dt: f32) {
        let mut expired = Vec::new();
        for (id, body) in &mut self.bodies {
            body.age += dt;
            if body.age >= SOLID_SECONDS + FADE_SECONDS {
                expired.push(*id);
            }
        }
        for id in expired {
            if let Some(body) = self.bodies.remove(&id) {
                self.world.remove_body_with_colliders(body.handle, true);
            }
        }
    }
    /// Make bricks and terrain around every awake body solid for the next
    /// `seconds` of motion.
    fn load_surroundings(&mut self, building: &Building, seconds: f32) -> Result<()> {
        let mut boxes = Vec::new();
        for body in self.bodies.values() {
            let rb = &self.world.bodies[body.handle];
            if rb.is_sleeping() {
                continue;
            }
            let position = Vec3::from_array(rb.translation().to_array());
            let velocity = Vec3::from_array(rb.linvel().to_array());
            let reach = Vec3::splat(SURROUNDINGS) + velocity.abs() * seconds;
            boxes.push((position - reach, position + reach));
        }
        for (low, high) in boxes {
            for (id, shape, pose) in building.colliding_bricks(low, high)? {
                if self.dead.contains(&id) || self.statics.contains_key(&Static::Brick(id)) {
                    continue;
                }
                let handle = self
                    .world
                    .insert_collider(ColliderBuilder::new(shape).position(pose), None);
                self.statics.insert(Static::Brick(id), Some(handle));
            }
            let chunk = |v: f32| (v / TERRAIN_CHUNK).floor() as i32;
            for cx in chunk(low.x)..=chunk(high.x) {
                for cz in chunk(low.z)..=chunk(high.z) {
                    if self.statics.contains_key(&Static::Terrain(cx, cz)) {
                        continue;
                    }
                    let min = Vec3::new(cx as f32, 0.0, cz as f32) * TERRAIN_CHUNK;
                    let max = min + Vec3::new(TERRAIN_CHUNK, 0.0, TERRAIN_CHUNK);
                    // One patch per terrain field; overlapping fields share
                    // the chunk through a compound.
                    let mut shapes: Vec<_> = building
                        .terrain_patches(min, max)?
                        .into_iter()
                        .map(|p| (Pose::IDENTITY, SharedShape::new(p)))
                        .collect();
                    let shape = match shapes.len() {
                        0 => None,
                        1 => shapes.pop().map(|(_, s)| s),
                        _ => Some(SharedShape::compound(shapes)),
                    };
                    let handle = shape.map(|shape| {
                        self.world
                            .insert_collider(ColliderBuilder::new(shape), None)
                    });
                    self.statics.insert(Static::Terrain(cx, cz), handle);
                }
            }
        }
        Ok(())
    }
    fn clear_statics(&mut self) {
        for handle in self.statics.drain().filter_map(|(_, h)| h) {
            self.world.remove_collider(handle);
        }
        let handles: Vec<_> = self.bodies.values().map(|b| b.handle).collect();
        for handle in handles {
            self.world.wake_up(handle, true);
        }
    }
    /// Forget deaths the world has since undone (respawned bricks) or made
    /// permanent (removed bricks).
    pub fn sync_world(&mut self, world: &PublicWorld) {
        let alive: BTreeSet<BrickId> = self.bodies.values().map(|b| b.brick).collect();
        // A cue can arrive before the brick change it announces, so a
        // brick with live debris counts as dead even while still visible.
        self.dead.retain(|id| {
            world
                .bricks
                .get(id)
                .is_some_and(|b| !b.visible || alive.contains(id))
        });
    }
    /// World transform and fade of every body, by look.
    pub fn instances(&self) -> impl Iterator<Item = (&Look, SceneTransform)> {
        self.bodies.values().map(|body| {
            let rb = &self.world.bodies[body.handle];
            let rotation = Quat::from_array(rb.rotation().to_array());
            let translation = Vec3::from_array(rb.translation().to_array());
            let fade = 1.0 - ((body.age - SOLID_SECONDS) / FADE_SECONDS).clamp(0.0, 1.0);
            (
                &body.look,
                SceneTransform {
                    transform: Mat4::from_rotation_translation(rotation, translation),
                    tint: [1.0, 1.0, 1.0, fade],
                },
            )
        })
    }
    /// Body centers, for tests and diagnostics.
    pub fn positions(&self) -> Vec<Vec3> {
        self.bodies
            .values()
            .map(|b| Vec3::from_array(self.world.bodies[b.handle].translation().to_array()))
            .collect()
    }
}

/// GPU models for debris, one shared mesh per look. Looks index the shared
/// brick material palette, so a new look uploads only its geometry: brick
/// textures and bind groups are never uploaded again for debris.
#[derive(Default)]
pub struct DebrisModels {
    models: BTreeMap<Look, Model>,
    frame: u64,
    pub diagnostics: DebrisModelDiagnostics,
}
#[derive(Clone, Debug, Default)]
pub struct DebrisModelDiagnostics {
    /// Look meshes built and uploaded.
    pub looks_built: u64,
    /// Texture images those uploads carried; the palette holds them all.
    pub images_uploaded: u64,
}
struct Model {
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transforms: Vec<SceneTransform>,
    used: u64,
}
impl DebrisModels {
    pub fn clear(&mut self) {
        self.models.clear();
    }
    /// Build missing looks against `gpu_palette` (the uploaded `palette`)
    /// and upload this frame's transforms.
    #[allow(clippy::too_many_arguments)] // GPU context plus the brick catalogs
    pub fn upload(
        &mut self,
        debris: &BrickDebris,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
        palette: &BrickPalette,
        gpu_palette: &GpuScene,
        materials: &crate::materials::BrickMaterials,
        colors: &[[f32; 4]],
    ) -> Result<()> {
        self.frame += 1;
        for model in self.models.values_mut() {
            model.transforms.clear();
        }
        for (look, transform) in debris.instances() {
            if transform.tint[3] <= 0.0 {
                continue;
            }
            if !self.models.contains_key(look) {
                if usize::from(look.color) >= colors.len() {
                    continue;
                }
                if self.models.len() >= MAX_LOOKS {
                    self.evict();
                }
                let data = look_scene(look, meshes, palette, materials, colors)
                    .with_context(|| format!("Debris model for {}", look.definition))?;
                let gpu = data
                    .map(|data| {
                        self.diagnostics.looks_built += 1;
                        self.diagnostics.images_uploaded += data.images.len() as u64;
                        renderer.upload_palette_model(device, &data, gpu_palette)
                    })
                    .transpose()?;
                self.models.insert(
                    look.clone(),
                    Model {
                        gpu,
                        instances: None,
                        transforms: Vec::new(),
                        used: self.frame,
                    },
                );
            }
            let model = self.models.get_mut(look).expect("model built above");
            model.transforms.push(transform);
            model.used = self.frame;
        }
        for model in self.models.values_mut() {
            if model.gpu.is_none() {
                continue;
            }
            if model.instances.is_none() && !model.transforms.is_empty() {
                model.instances = Some(GpuInstances::new(device, MAX_BODIES)?);
            }
            if let Some(instances) = &mut model.instances {
                instances.update(queue, &model.transforms)?;
            }
        }
        Ok(())
    }
    /// Drop the least recently used look that has no bodies this frame.
    fn evict(&mut self) {
        if let Some(look) = self
            .models
            .iter()
            .filter(|(_, m)| m.used != self.frame)
            .min_by_key(|(_, m)| m.used)
            .map(|(look, _)| look.clone())
        {
            self.models.remove(&look);
        }
    }
    pub fn draws(&self) -> Vec<(&GpuScene, &GpuInstances)> {
        self.models
            .values()
            .filter(|m| !m.transforms.is_empty())
            .filter_map(|m| Some((m.gpu.as_ref()?, m.instances.as_ref()?)))
            .collect()
    }
}

/// One brick at the origin in its own frame, drawn like a planted brick
/// with the planted bricks' materials.
fn look_scene(
    look: &Look,
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
    palette: &BrickPalette,
    materials: &crate::materials::BrickMaterials,
    colors: &[[f32; 4]],
) -> Result<Option<SceneData>> {
    let mut brick =
        bri_world::Brick::new(ContentRef::Resolved(look.definition.clone()), [0.0; 3], 0);
    brick.color = look.color;
    brick.color_effect = look.color_effect;
    brick.shape_effect = look.shape_effect;
    brick.print = look.print.clone();
    let data = crate::world_chunks::build_brick(&brick, colors, meshes, palette, Some(materials))?;
    Ok((!data.indices.is_empty()).then_some(data))
}

/// SplitMix64: the same throw from the same cue on every client.
struct Seeded(u64);
impl Seeded {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x6a09_e667_f3bc_c909)
    }
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
        self.unit() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::collision::{CollisionBody, Part};
    use bri_sim::definitions::{Definition, Definitions};

    fn building(bricks: &[(BrickId, [f32; 3])]) -> (Building, PublicWorld) {
        let definition = Definition {
            mesh: bri_content::brick::Brick {
                schema_version: 1,
                id: "brick".into(),
                footprint_studs: [2, 2],
                height_plates: 3,
                attachment_rows: vec!["bb".into(); 6],
                collision_boxes: vec![],
                needs_external_collision: false,
                coverage: None,
                quads: vec![],
            },
            shape: SharedShape::cuboid(0.5, 0.3, 0.5),
            collision: CollisionBody {
                id: "brick".into(),
                parts: vec![Part::Box {
                    center: [0.0; 3],
                    size: [1.0, 0.6, 1.0],
                }],
            },
            indestructible: false,
            special: Default::default(),
        };
        let floor =
            ColliderBuilder::cuboid(50.0, 0.5, 50.0).translation(Vector::new(0.0, -0.5, 0.0));
        let mut building = Building::new(
            Definitions {
                entries: [("brick".into(), definition)].into(),
            },
            vec![floor],
        )
        .unwrap();
        let world = PublicWorld {
            name: "Debris".into(),
            map_id: "map".into(),
            palette: vec![[1.0; 4]; 2],
            bricks: bricks
                .iter()
                .map(|(id, p)| {
                    (
                        *id,
                        bri_world::Brick::new(ContentRef::Resolved("brick".into()), *p, 1),
                    )
                })
                .collect(),
        };
        building.sync_world(&world).unwrap();
        (building, world)
    }
    fn kill(
        id: u64,
        brick: BrickId,
        at: [f32; 3],
        origin: [f32; 3],
        force: f32,
        radius: f32,
    ) -> Cue {
        Cue {
            id,
            tick: 1,
            position: at,
            kind: CueKind::BrickKill {
                brick,
                definition: ContentRef::Resolved("brick".into()),
                quarter_turns: 1,
                color: 1,
                color_effect: 0,
                shape_effect: 0,
                print: None,
                origin,
                force,
                radius,
            },
        }
    }
    fn run(debris: &mut BrickDebris, building: &Building, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            debris.advance(1.0 / 60.0, building).unwrap();
        }
    }

    #[test]
    fn hammer_pop_rises_tumbles_lands_fades_and_is_removed() {
        let (building, _) = building(&[(7, [0.0, 0.3, 0.0])]);
        let mut debris = BrickDebris::new();
        // Cue arrives before the world update that removes the brick.
        debris
            .cues(
                &[kill(1, 7, [0.0, 0.3, 0.0], [0.0, -0.7, 0.0], 12.0, 0.0)],
                &building,
            )
            .unwrap();
        assert!(debris.is_dead(7));
        let mut peak = 0.0f32;
        for _ in 0..30 {
            debris.advance(1.0 / 60.0, &building).unwrap();
            peak = peak.max(debris.positions()[0].y);
        }
        assert!(peak > 0.9, "popped only to {peak}");
        run(&mut debris, &building, 1.0);
        let rest = debris.positions()[0];
        assert!(
            rest.y > 0.05 && rest.y < 0.7,
            "fell through or floated: {rest}"
        );
        let (_, t) = debris.instances().next().unwrap();
        assert_eq!(t.tint[3], 1.0);
        run(&mut debris, &building, SOLID_SECONDS);
        let (_, t) = debris.instances().next().unwrap();
        assert!(
            t.tint[3] > 0.0 && t.tint[3] < 1.0,
            "not fading: {}",
            t.tint[3]
        );
        run(&mut debris, &building, FADE_SECONDS);
        assert!(debris.is_empty());
    }

    #[test]
    fn debris_lands_on_neighbouring_bricks_but_not_its_own_spot() {
        // Brick 1 sits on brick 2. Killing brick 2 drops nothing onto its own
        // (still colliding) collider; killing brick 1 lands it on brick 2.
        let (building, _) = building(&[(1, [0.0, 0.9, 0.0]), (2, [0.0, 0.3, 0.0])]);
        let mut debris = BrickDebris::new();
        debris
            .cues(
                &[kill(1, 1, [0.0, 0.9, 0.0], [0.0, 0.9, 0.0], 0.0, 0.0)],
                &building,
            )
            .unwrap();
        run(&mut debris, &building, 1.0);
        let y = debris.positions()[0].y;
        assert!((y - 0.9).abs() < 0.15, "should rest on brick 2: {y}");
        let mut debris = BrickDebris::new();
        debris
            .cues(
                &[kill(1, 2, [0.0, 0.3, 0.0], [0.0, 0.3, 0.0], 0.0, 0.0)],
                &building,
            )
            .unwrap();
        run(&mut debris, &building, 1.0);
        let y = debris.positions()[0].y;
        assert!(y < 0.45, "held up by its own dead brick: {y}");
    }

    #[test]
    fn every_client_throws_the_same_way_and_blasts_push_outward() {
        let (building, _) = building(&[]);
        let cues: Vec<_> = (0..6)
            .map(|i| {
                let x = i as f32 - 2.5;
                kill(10 + i, 100 + i, [x, 0.3, -4.0], [0.0, 0.3, -4.0], 30.0, 3.0)
            })
            .collect();
        let mut a = BrickDebris::new();
        let mut b = BrickDebris::new();
        a.cues(&cues, &building).unwrap();
        b.cues(&cues, &building).unwrap();
        // Different frame pacing on each client.
        for _ in 0..30 {
            a.advance(1.0 / 30.0, &building).unwrap();
        }
        for _ in 0..60 {
            b.advance(1.0 / 60.0, &building).unwrap();
        }
        assert_eq!(a.positions(), b.positions());
        for (cue, p) in cues.iter().zip(a.positions()) {
            let x = cue.position[0];
            assert!(p.x * x.signum() > x.abs(), "{x} not thrown outward: {p}");
        }
        // Duplicates (resent checkpoints) are ignored.
        a.cues(&cues, &building).unwrap();
        assert_eq!(a.len(), 6);
        assert_eq!(a.diagnostics.duplicates, 6);
    }

    fn player_at(id: u64, feet: Vec3) -> Pusher {
        Pusher {
            id,
            center: feet + Vec3::Y * 1.2,
            rotation: Quat::IDENTITY,
            half: Vec3::new(0.5, 1.2, 0.5),
        }
    }
    /// A brick left lying (no throw) at `at`.
    fn lying(id: u64, at: [f32; 3]) -> Cue {
        kill(id, id, at, at, 0.0, 0.0)
    }
    fn pusher_center(debris: &BrickDebris, id: u64) -> Vec3 {
        Vec3::from_array(
            debris.world.bodies[debris.pushers[&id].handle]
                .translation()
                .to_array(),
        )
    }

    #[test]
    fn a_player_walking_into_debris_shoves_it_and_is_never_moved_by_it() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        debris
            .cues(&[lying(1, [0.0, 0.3, -3.0])], &building)
            .unwrap();
        run(&mut debris, &building, 0.5);
        let rest = debris.positions()[0];
        // v20's run speed, 7 units/s, straight through the brick's spot.
        for frame in 1..=60 {
            let feet = Vec3::new(0.0, 0.0, -7.0 * frame as f32 / 60.0);
            debris.push(&[player_at(9, feet)]);
            debris.advance(1.0 / 60.0, &building).unwrap();
            // One way: the player is exactly where the game put it.
            let center = pusher_center(&debris, 9);
            assert!(
                center.distance(feet + Vec3::Y * 1.2) < 1e-4,
                "debris moved the player to {center}"
            );
        }
        let shoved = debris.positions()[0];
        assert!(
            shoved.z < rest.z - 1.0 && shoved.y > 0.0,
            "not shoved ahead: {rest} -> {shoved}"
        );
    }

    #[test]
    fn a_vehicle_ramming_a_pile_scatters_it() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        let mut cues = Vec::new();
        for layer in 0..3 {
            for x in -1..=1 {
                let id = cues.len() as u64 + 1;
                cues.push(lying(id, [x as f32 * 1.05, 0.3 + layer as f32 * 0.6, -6.0]));
            }
        }
        debris.cues(&cues, &building).unwrap();
        run(&mut debris, &building, 1.0);
        let before = debris.positions();
        let turned = Quat::from_rotation_y(0.2);
        for frame in 0..60 {
            let z = -15.0 * frame as f32 / 60.0;
            debris.push(&[Pusher {
                id: 1 << 63 | 4,
                center: Vec3::new(0.0, 0.8, z),
                rotation: turned,
                half: Vec3::new(1.2, 0.6, 2.0),
            }]);
            debris.advance(1.0 / 60.0, &building).unwrap();
        }
        let moved = debris
            .positions()
            .iter()
            .zip(&before)
            .filter(|(a, b)| a.distance(**b) > 1.0)
            .count();
        assert!(moved >= 7, "only {moved} of 9 scattered");
    }

    #[test]
    fn a_resting_pile_stays_put_with_nobody_near() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        let mut cues = Vec::new();
        for layer in 0..4 {
            for x in 0..3 {
                for z in 0..2 {
                    let id = cues.len() as u64 + 1;
                    cues.push(lying(
                        id,
                        [x as f32 * 1.02, 0.3 + layer as f32 * 0.6, z as f32 * 1.02],
                    ));
                }
            }
        }
        debris.cues(&cues, &building).unwrap();
        run(&mut debris, &building, 0.5);
        let settled = debris.positions();
        // A player far away takes no part.
        debris.push(&[player_at(9, Vec3::new(30.0, 0.0, 0.0))]);
        assert!(debris.pushers.is_empty());
        run(&mut debris, &building, 2.0);
        for (now, then) in debris.positions().iter().zip(&settled) {
            assert!(now.distance(*then) < 0.05, "crept from {then} to {now}");
        }
    }

    #[test]
    fn shots_and_blasts_push_debris_and_fly_on() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        debris
            .cues(
                &[lying(1, [0.0, 0.3, -5.0]), lying(2, [4.0, 0.3, 0.0])],
                &building,
            )
            .unwrap();
        run(&mut debris, &building, 0.5);
        let velocity = Vec3::new(0.0, 0.0, -100.0);
        for frame in 0..10 {
            let shot = Shot {
                id: 7,
                position: Vec3::new(0.0, 0.3, -1.0) + velocity * (frame as f32 / 60.0),
                velocity,
            };
            debris.shots(&[shot]);
            debris.advance(1.0 / 60.0, &building).unwrap();
        }
        assert_eq!(debris.diagnostics.projectile_hits, 1, "once per brick");
        run(&mut debris, &building, 0.5);
        assert!(debris.positions()[0].z < -5.2, "{}", debris.positions()[0]);
        // A later blast shoves debris already lying around it.
        debris
            .cues(
                &[kill(3, 3, [3.0, 0.3, 0.0], [3.0, 0.3, 0.0], 30.0, 4.0)],
                &building,
            )
            .unwrap();
        run(&mut debris, &building, 0.5);
        assert!(debris.positions()[1].x > 5.0, "{}", debris.positions()[1]);
    }

    #[test]
    fn mass_kills_stay_bounded_and_cheap() {
        let (building, _) = building(&[]);
        let cues: Vec<_> = (0..500u64)
            .map(|i| {
                let p = [
                    (i % 20) as f32,
                    0.3 + (i / 100) as f32 * 0.6,
                    -(((i / 20) % 5) as f32),
                ];
                kill(i + 1, i + 1, p, [10.0, 0.0, -2.0], 50.0, 5.0)
            })
            .collect();
        let mut debris = BrickDebris::new();
        debris.cues(&cues, &building).unwrap();
        assert_eq!(debris.len(), MAX_BODIES);
        assert_eq!(debris.diagnostics.evicted, (500 - MAX_BODIES) as u64);
        let start = std::time::Instant::now();
        run(&mut debris, &building, 1.0);
        let elapsed = start.elapsed();
        eprintln!("{MAX_BODIES} debris bodies, 60 frames: {elapsed:?}");
        // A crowd around the blast: only the nearest few dozen push.
        let crowd: Vec<_> = (0..64)
            .map(|i| {
                player_at(
                    i + 1,
                    Vec3::new((i % 8) as f32 * 2.0, 0.0, -((i / 8) as f32)),
                )
            })
            .collect();
        let start = std::time::Instant::now();
        for _ in 0..60 {
            debris.push(&crowd);
            debris.advance(1.0 / 60.0, &building).unwrap();
            assert!(debris.pushers.len() <= MAX_PUSHERS);
        }
        eprintln!(
            "{} debris bodies, {} pushers, 60 frames: {:?}",
            debris.len(),
            debris.pushers.len(),
            start.elapsed()
        );
        // A long hitch drops debris time instead of spiralling.
        debris.advance(5.0, &building).unwrap();
        assert!(debris.diagnostics.dropped_steps > 0);
        assert!(debris.positions().iter().all(|p| p.is_finite()));
    }

    #[test]
    fn debris_looks_upload_geometry_only_against_the_brick_palette() {
        // Each new look once built its own scene with every brick texture,
        // mipmapped on the CPU and uploaded on the frame the bricks died: a
        // dozen bricks hitched ~50 ms, a full 128 ~300 ms. Looks now index
        // the shared palette the chunks use, so none carries an image.
        let meshes = crate::world_chunks::tests::meshes();
        let materials = crate::materials::BrickMaterials::in_memory();
        let palette = BrickPalette::new(&materials).unwrap();
        let colors = [[0.9, 0.2, 0.1, 1.0], [0.2, 0.4, 0.8, 0.5]];
        let plain = Look {
            definition: "definition/a".into(),
            color: 0,
            color_effect: 0,
            shape_effect: 0,
            print: None,
        };
        let looks = [
            plain.clone(),
            Look {
                color: 1,
                ..plain.clone()
            },
            Look {
                print: Some(ContentRef::Resolved("print/print_letters_default/a".into())),
                ..plain.clone()
            },
            Look {
                color_effect: 1,
                shape_effect: 1,
                ..plain.clone()
            },
        ];
        for look in &looks {
            let data = look_scene(look, &meshes, &palette, &materials, &colors)
                .unwrap()
                .unwrap();
            assert!(data.images.is_empty(), "{look:?} uploads textures");
            assert_eq!(data.materials, palette.scene.materials);
            // The same geometry the standalone scene drew.
            let mut brick =
                bri_world::Brick::new(ContentRef::Resolved(look.definition.clone()), [0.0; 3], 0);
            brick.color = look.color;
            brick.color_effect = look.color_effect;
            brick.shape_effect = look.shape_effect;
            brick.print = look.print.clone();
            let standalone = crate::world_scene::build_world_scene_materials(
                &PublicWorld {
                    name: "Look".into(),
                    map_id: "look".into(),
                    palette: colors.to_vec(),
                    bricks: bri_world::Bricks::unit(0, brick),
                },
                &meshes,
                1000,
                Some(&materials),
            )
            .unwrap();
            assert!(!standalone.images.is_empty());
            assert_eq!(
                format!("{:?}", data.vertices),
                format!("{:?}", standalone.vertices)
            );
            assert_eq!(data.indices, standalone.indices);
        }
    }
}
