//! Brick debris: v20's client-side PhysX "brick explosion".
//!
//! Every `BrickKill` cue (hammer, Destructo Wand, `fakeKillBrick`, brick
//! explosions) turns the dead brick into a short-lived Rapier rigid body. It
//! is thrown away from the blast origin, tumbles against the map, terrain,
//! nearby bricks and other debris, then fades out like a ghost. Debris is
//! purely cosmetic and never touches gameplay; the server already hid or
//! removed the brick. Each body's throw is seeded from its cue id, so every
//! client sees the same throw.
//!
//! Like v20's `$Physics::maxBricks`, only a bounded number of bodies are
//! alive at once; the oldest make way for new ones.
use crate::building::Building;
use anyhow::{Context, Result, ensure};
use bri_net::protocol::PublicWorld;
use bri_render::scene::{GpuInstances, GpuScene, SceneRenderer, SceneTransform};
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
/// Seconds a body stays solid before it starts to fade.
const SOLID_SECONDS: f32 = 1.5;
/// Seconds of fading to fully transparent, after which the body is removed.
const FADE_SECONDS: f32 = 2.5;
/// Converts v20 blast force into launch speed (units/s).
const FORCE_TO_SPEED: f32 = 0.5;
const MAX_SPEED: f32 = 40.0;
/// Bodies are a hair smaller than the brick so neighbours killed together
/// don't start out interpenetrating.
const BODY_SHRINK: f32 = 0.96;
/// Distance around each body where surroundings are made solid.
const SURROUNDINGS: f32 = 2.0;
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
            .density(1.0)
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
        for _ in 0..steps {
            self.world.step();
            self.age(STEP);
        }
        Ok(())
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

/// GPU models for debris, one shared mesh per look.
#[derive(Default)]
pub struct DebrisModels {
    models: BTreeMap<Look, Model>,
    frame: u64,
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
    /// Build missing looks and upload this frame's transforms.
    #[allow(clippy::too_many_arguments)] // GPU context plus the brick catalogs
    pub fn upload(
        &mut self,
        debris: &BrickDebris,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
        materials: &crate::materials::BrickMaterials,
        palette: &[[f32; 4]],
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
                if usize::from(look.color) >= palette.len() {
                    continue;
                }
                if self.models.len() >= MAX_LOOKS {
                    self.evict();
                }
                let gpu = build_look(look, renderer, device, queue, meshes, materials, palette)
                    .with_context(|| format!("Debris model for {}", look.definition))?;
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

/// One brick at the origin in its own frame, drawn like a planted brick.
fn build_look(
    look: &Look,
    renderer: &SceneRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
    materials: &crate::materials::BrickMaterials,
    palette: &[[f32; 4]],
) -> Result<Option<GpuScene>> {
    let mut brick =
        bri_world::Brick::new(ContentRef::Resolved(look.definition.clone()), [0.0; 3], 0);
    brick.color = look.color;
    brick.color_effect = look.color_effect;
    brick.shape_effect = look.shape_effect;
    brick.print = look.print.clone();
    let world = PublicWorld {
        name: "Brick debris".into(),
        map_id: "debris".into(),
        palette: palette.to_vec(),
        bricks: [(0, brick)].into(),
    };
    let data =
        crate::world_scene::build_world_scene_materials(&world, meshes, 200_000, Some(materials))?;
    if data.indices.is_empty() {
        return Ok(None);
    }
    Ok(Some(renderer.upload(device, queue, &data)?))
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
        run(&mut debris, &building, 1.5);
        let (_, t) = debris.instances().next().unwrap();
        assert!(
            t.tint[3] > 0.0 && t.tint[3] < 1.0,
            "not fading: {}",
            t.tint[3]
        );
        run(&mut debris, &building, 2.0);
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
        // A long hitch drops debris time instead of spiralling.
        debris.advance(5.0, &building).unwrap();
        assert!(debris.diagnostics.dropped_steps > 0);
        assert!(debris.positions().iter().all(|p| p.is_finite()));
    }
}
