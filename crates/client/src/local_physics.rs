//! What every client-only physics world shares: the solid world round its
//! moving bodies, and the players, vehicles and shots this client draws
//! pushing them one way. Brick debris ([`crate::brick_debris`]) and Add-On
//! bodies ([`crate::addon_physics`]) are both cosmetic, like particles:
//! they react to what this client draws and never push back on gameplay.
use crate::building::Building;
use anyhow::Result;
use bri_world::BrickId;
use glam::{Quat, Vec3};
use rapier3d::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Fixed physics step, like the rest of the game.
pub const STEP: f32 = bri_physics::FIXED_DT;
/// Steps per frame before time is dropped instead of catching up.
pub const MAX_STEPS: u32 = 4;
pub(crate) const MAX_STATIC_COLLIDERS: usize = 16_384;
// Query/build work costs sixteen cache lookups. Callers retain their existing
// real-geometry quota while overlapping bodies can reuse prepared tiles cheaply.
pub(crate) const COLLISION_QUERY_COST: u32 = 16;
/// Torque's world gravity (units/s^2), as the player and items use.
pub const GRAVITY: f32 = 20.0;
/// Distance around each moving body where surroundings are made solid.
const SURROUNDINGS: f32 = 2.0;
/// Grid used to cache terrain patches.
const TERRAIN_CHUNK: f32 = 8.0;
/// Bodies only feel pushers within this distance.
pub const PUSHER_REACH: f32 = 6.0;
/// Most players and vehicles pushing at once.
pub const MAX_PUSHERS: usize = 32;
/// A pusher that jumps further than this in a frame teleported.
const TELEPORT: f32 = 5.0;

/// A client-only world with Torque's gravity.
pub fn new_world() -> PhysicsWorld {
    let mut world = bri_physics::new_world();
    world.gravity = Vector::new(0.0, -GRAVITY, 0.0);
    world
}

/// A player or vehicle as this client draws it this frame: a box, or the
/// body's own collision `shape`, that shoves bodies out of its way. `id`
/// must stay the same between frames.
#[derive(Clone, Debug)]
pub struct Pusher {
    pub id: u64,
    pub center: Vec3,
    pub rotation: Quat,
    /// Half its size each way: the box, or the box round `shape`.
    pub half: Vec3,
    /// The shape it collides with as it does on the host (a vehicle's
    /// [`bri_vehicles::body_shape`]: a Steel Ball's sphere, a hull), posed
    /// at `center` and `rotation`; None for the box. A rolling ball shoved
    /// debris as a spinning box would fling it like a paddle wheel.
    pub shape: Option<SharedShape>,
}
/// A projectile as this client draws it this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub id: u64,
    pub position: Vec3,
    pub velocity: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Static {
    Brick(BrickId),
    Terrain(i32, i32),
}

/// The map, and the bricks and terrain near moving bodies, made solid on
/// demand.
#[derive(Default)]
pub struct Surroundings {
    /// The map's colliders here, from the building's
    /// [`Building::map_generation`] (0 before any).
    map: Vec<ColliderHandle>,
    map_generation: u64,
    /// `None` marks terrain chunks with no ground.
    statics: HashMap<Static, Option<ColliderHandle>>,
    generation: u64,
    cache_area: Option<(Vec3, Vec3)>,
    brick_tiles: HashMap<[i32; 3], BrickTile>,
}

struct BrickTile {
    generation: u64,
    cursor: bri_sim::grid::QueryCursor,
    blocked: Option<(BrickId, SharedShape, Pose)>,
    done: bool,
}

/// Pausable collision streaming for a cosmetic assembly's actual swept box.
/// No spherical reach proportional to the height of a tower.
pub(crate) struct LocalRegion {
    pub low: Vec3,
    pub high: Vec3,
    brick: Option<[i32; 3]>,
    first: [i32; 3],
    last: [i32; 3],
    tile: Option<(i32, i32)>,
    first_z: i32,
    last_x: i32,
    last_z: i32,
}
impl LocalRegion {
    pub fn new(low: Vec3, high: Vec3) -> Self {
        let first: [i32; 3] = low.to_array().map(|v| (v / TERRAIN_CHUNK).floor() as i32);
        let last: [i32; 3] = high.to_array().map(|v| (v / TERRAIN_CHUNK).floor() as i32);
        Self {
            low: Vec3::from_array(first.map(|v| v as f32 * TERRAIN_CHUNK)),
            high: Vec3::from_array(last.map(|v| (v + 1) as f32 * TERRAIN_CHUNK)),
            brick: Some(first),
            first,
            last,
            tile: Some((first[0], first[2])),
            first_z: first[2],
            last_x: last[0],
            last_z: last[2],
        }
    }
    fn next_brick_tile(&mut self) {
        let Some(mut key) = self.brick else { return };
        key[2] += 1;
        if key[2] > self.last[2] {
            key[2] = self.first[2];
            key[1] += 1;
        }
        if key[1] > self.last[1] {
            key[1] = self.first[1];
            key[0] += 1;
        }
        self.brick = (key[0] <= self.last[0]).then_some(key);
    }
    pub fn contains(&self, low: Vec3, high: Vec3) -> bool {
        low.cmpge(self.low).all() && high.cmple(self.high).all()
    }
}
impl Surroundings {
    fn touch(&mut self, low: Vec3, high: Vec3) {
        self.cache_area = Some(
            self.cache_area
                .map_or((low, high), |(lo, hi)| (lo.min(low), hi.max(high))),
        );
    }
    pub(crate) fn full(&self) -> bool {
        self.statics.len() >= MAX_STATIC_COLLIDERS
    }
    /// Put the map in (again whenever its solid shapes changed: another
    /// map, or a shape smashed), and forget the nearby bricks the build no
    /// longer has where they were made solid (waking every body, which may
    /// have lost its support). Bricks that did not change stay: a build
    /// changing anywhere (a door, a blinking light) neither rebuilds them
    /// nor wakes resting bodies. Returns whether the build changed, when
    /// [`Self::load`] should look round resting bodies too. Call before it.
    pub fn sync(&mut self, world: &mut PhysicsWorld, building: &Building) -> bool {
        if self.map_generation != building.map_generation() {
            for handle in self.map.drain(..) {
                world.remove_collider(handle);
            }
            self.map = building
                .map_colliders()
                .map(|collider| world.insert_collider(collider.clone(), None))
                .collect();
            self.map_generation = building.map_generation();
            // Another map: none of the old bricks or terrain belong to it.
            self.clear(world);
        }
        if self.generation == building.query_generation() {
            return false;
        }
        if self
            .cache_area
            .is_none_or(|(lo, hi)| !building.query_changed_since(self.generation, lo, hi))
        {
            self.generation = building.query_generation();
            return false;
        }
        self.generation = building.query_generation();
        let stale: Vec<Static> = self
            .statics
            .iter()
            .filter_map(|(key, handle)| match (key, handle) {
                (Static::Brick(id), Some(handle)) => {
                    let collider = &world.colliders[*handle];
                    let same = building.solid_brick(*id).is_some_and(|(shape, pose)| {
                        std::sync::Arc::ptr_eq(&shape.0, &collider.shared_shape().0)
                            && pose == *collider.position()
                    });
                    (!same).then_some(*key)
                }
                _ => None,
            })
            .collect();
        for key in &stale {
            if let Some(Some(handle)) = self.statics.remove(key) {
                world.remove_collider(handle);
            }
        }
        if !stale.is_empty() {
            world.wake_up_all(true);
        }
        true
    }
    /// Make bricks and terrain solid inside each box (low, high), except
    /// bricks `skip` names. Returns how many bricks it made solid.
    pub fn load(
        &mut self,
        world: &mut PhysicsWorld,
        building: &Building,
        boxes: &[(Vec3, Vec3)],
        skip: impl Fn(BrickId) -> bool,
    ) -> Result<usize> {
        let mut added = 0;
        for &(low, high) in boxes {
            self.touch(low, high);
            for (id, shape, pose) in building.colliding_bricks(low, high)? {
                if skip(id) || self.statics.contains_key(&Static::Brick(id)) {
                    continue;
                }
                let handle =
                    world.insert_collider(ColliderBuilder::new(shape).position(pose), None);
                self.statics.insert(Static::Brick(id), Some(handle));
                added += 1;
            }
            let chunk = |v: f32| (v / TERRAIN_CHUNK).floor() as i32;
            for cx in chunk(low.x)..=chunk(high.x) {
                for cz in chunk(low.z)..=chunk(high.z) {
                    self.load_terrain_tile(world, building, cx, cz)?;
                }
            }
        }
        Ok(added)
    }
    pub(crate) fn load_region(
        &mut self,
        world: &mut PhysicsWorld,
        building: &Building,
        region: &mut LocalRegion,
        work: &mut u32,
        skip: impl Fn(BrickId) -> bool,
    ) -> Result<bool> {
        self.touch(region.low, region.high);
        // Overlapping bodies share both completed and unfinished tile queries.
        // The budget pays for world geometry once, not once per flying brick.
        while *work > 0 && region.brick.is_some() {
            let key = region.brick.unwrap();
            let low = Vec3::from_array(key.map(|v| v as f32 * TERRAIN_CHUNK));
            let high = low + Vec3::splat(TERRAIN_CHUNK);
            let changed = self
                .brick_tiles
                .get(&key)
                .is_none_or(|tile| building.query_changed_since(tile.generation, low, high));
            if changed {
                if !self.brick_tiles.contains_key(&key)
                    && self.brick_tiles.len() >= MAX_STATIC_COLLIDERS
                {
                    return Ok(false);
                }
                self.brick_tiles.insert(
                    key,
                    BrickTile {
                        generation: building.query_generation(),
                        cursor: bri_sim::grid::QueryCursor::new(bri_sim::grid::Bounds {
                            min: std::array::from_fn(|a| {
                                (low[a] / bri_sim::grid::CELL[a]).round() as i32
                            }),
                            size: std::array::from_fn(|a| {
                                (TERRAIN_CHUNK / bri_sim::grid::CELL[a]).round() as i32
                            }),
                        }),
                        blocked: None,
                        done: false,
                    },
                );
            }
            let tile = self.brick_tiles.get_mut(&key).unwrap();
            tile.generation = building.query_generation();
            let cost = if tile.done { 1 } else { COLLISION_QUERY_COST };
            if *work < cost {
                return Ok(false);
            }
            *work -= cost;
            if tile.done {
                region.next_brick_tile();
                continue;
            }
            let candidate = if let Some(blocked) = tile.blocked.take() {
                Some(Some(blocked))
            } else {
                building.section_solid_step(&mut tile.cursor)
            };
            match candidate {
                None => {
                    tile.done = true;
                    region.next_brick_tile();
                }
                Some(Some((id, shape, pose)))
                    if !skip(id) && !self.statics.contains_key(&Static::Brick(id)) =>
                {
                    if self.statics.len() >= MAX_STATIC_COLLIDERS {
                        tile.blocked = Some((id, shape, pose));
                        return Ok(false);
                    }
                    let h = world.insert_collider(ColliderBuilder::new(shape).position(pose), None);
                    self.statics.insert(Static::Brick(id), Some(h));
                }
                _ => {}
            }
        }
        while *work > 0 && region.brick.is_none() {
            let Some((cx, cz)) = region.tile else {
                return Ok(true);
            };
            // An overlapping body's already prepared tile is a cache lookup,
            // not another terrain build. Charging it as a build made a burst
            // of debris wait seconds for collision that already existed.
            let cached = self.statics.contains_key(&Static::Terrain(cx, cz));
            let cost = if cached { 1 } else { 16 * COLLISION_QUERY_COST };
            if *work < cost || (!cached && self.full()) {
                return Ok(false);
            }
            *work -= cost;
            if !cached {
                self.load_terrain_tile(world, building, cx, cz)?;
            }
            let (nx, nz) = if cz < region.last_z {
                (cx, cz + 1)
            } else {
                (cx + 1, region.first_z)
            };
            region.tile = (nx <= region.last_x).then_some((nx, nz));
        }
        Ok(region.brick.is_none() && region.tile.is_none())
    }
    fn load_terrain_tile(
        &mut self,
        world: &mut PhysicsWorld,
        building: &Building,
        cx: i32,
        cz: i32,
    ) -> Result<()> {
        if self.statics.contains_key(&Static::Terrain(cx, cz)) {
            return Ok(());
        }
        let min = Vec3::new(cx as f32, 0., cz as f32) * TERRAIN_CHUNK;
        let max = min + Vec3::new(TERRAIN_CHUNK, 0., TERRAIN_CHUNK);
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
        let handle = shape.map(|shape| world.insert_collider(ColliderBuilder::new(shape), None));
        self.statics.insert(Static::Terrain(cx, cz), handle);
        Ok(())
    }
    /// The box round a moving body that must be solid for the next
    /// `seconds` of its motion.
    pub fn reach(position: Vec3, velocity: Vec3, extent: f32, seconds: f32) -> (Vec3, Vec3) {
        let reach = Vec3::splat(SURROUNDINGS + extent) + velocity.abs() * seconds;
        (position - reach, position + reach)
    }
    /// Stop treating `brick` as solid (it just died).
    pub fn forget_brick(&mut self, world: &mut PhysicsWorld, brick: BrickId) {
        if let Some(Some(handle)) = self.statics.remove(&Static::Brick(brick)) {
            world.remove_collider(handle);
        }
    }
    /// Drop every nearby brick and terrain patch (the map stays), and wake
    /// the bodies that rested on them.
    pub fn clear(&mut self, world: &mut PhysicsWorld) {
        self.cache_area = None;
        self.brick_tiles.clear();
        for handle in self.statics.drain().filter_map(|(_, h)| h) {
            world.remove_collider(handle);
        }
        world.wake_up_all(true);
    }
    /// Bricks and terrain patches solid right now.
    pub fn len(&self) -> usize {
        self.statics.values().filter(|h| h.is_some()).count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One collision-streaming mechanism for ordinary debris and connected chunks.
/// Keep the last prepared window usable while preparing a swept look-ahead.
#[derive(Default)]
pub(crate) struct CollisionWindow {
    region: Option<LocalRegion>,
    safe: Option<(Vec3, Vec3)>,
    generation: u64,
    ready: bool,
}
pub(crate) struct CollisionMotion {
    pub center: Vec3,
    pub half: Vec3,
    pub velocity: Vec3,
    pub angular: f32,
}
impl CollisionWindow {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn unchanged(&self, building: &Building) -> bool {
        self.generation == building.query_generation()
    }
    #[allow(clippy::too_many_arguments)] // Shared query context plus motion and budget.
    pub fn prepare(
        &mut self,
        surroundings: &mut Surroundings,
        world: &mut PhysicsWorld,
        building: &Building,
        motion: CollisionMotion,
        work: &mut u32,
        skip: impl Fn(BrickId) -> bool,
    ) -> Result<bool> {
        let CollisionMotion {
            center,
            half,
            velocity,
            angular,
        } = motion;
        // Prepare two worst-case rendered frames ahead. A half-second spin
        // sphere made a small brick request collision across entire buildings.
        let ahead = STEP * MAX_STEPS as f32 * 2.;
        let turn = (half.length() * angular * ahead).min(half.length() * 2.);
        let margin = Vec3::splat(0.1 + turn) + velocity.abs() * ahead;
        let low = center - half - margin;
        let high = center + half + margin;
        let (watched_low, watched_high) = self
            .region
            .as_ref()
            .map_or((low, high), |r| (r.low, r.high));
        let changed = building.query_changed_since(self.generation, watched_low, watched_high);
        if changed || self.region.as_ref().is_none_or(|r| !r.contains(low, high)) {
            self.region = Some(LocalRegion::new(low - Vec3::ONE, high + Vec3::ONE));
            self.ready = false;
            if changed {
                self.safe = None;
            }
        }
        self.generation = building.query_generation();
        if !self.ready {
            self.ready = surroundings.load_region(
                world,
                building,
                self.region.as_mut().unwrap(),
                work,
                skip,
            )?;
            if self.ready {
                let region = self.region.as_ref().unwrap();
                self.safe = Some((region.low, region.high));
            }
        }
        let step = velocity.abs() * STEP * MAX_STEPS as f32
            + Vec3::splat(half.length() * angular * STEP * MAX_STEPS as f32);
        Ok(self.ready
            || self.safe.is_some_and(|(lo, hi)| {
                (center - half - step).cmpge(lo).all() && (center + half + step).cmple(hi).all()
            }))
    }
}

struct PusherBody {
    handle: RigidBodyHandle,
    /// Where the body is heading this frame.
    from: Pose,
    to: Pose,
}

/// Players and vehicles as kinematic boxes: bodies move out of their way
/// and never move them.
#[derive(Default)]
pub struct Pushers {
    bodies: BTreeMap<u64, PusherBody>,
}

impl Pushers {
    /// Where pushers are this frame. Only those `near` a body take part.
    pub fn update(
        &mut self,
        world: &mut PhysicsWorld,
        pushers: &[Pusher],
        near: impl Fn(&Pusher) -> bool,
    ) {
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
            .bodies
            .keys()
            .copied()
            .filter(|id| !ids.contains(id))
            .collect();
        for id in gone {
            if let Some(p) = self.bodies.remove(&id) {
                world.remove_body_with_colliders(p.handle, true);
            }
        }
        for p in wanted {
            let to = Pose::from_parts(
                Vector::from_array(p.center.to_array()),
                Rotation::from_array(p.rotation.normalize().to_array()),
            );
            match self.bodies.get_mut(&p.id) {
                Some(body) => {
                    let jumped = Vec3::from_array(body.to.translation.to_array())
                        .distance(p.center)
                        > TELEPORT;
                    if jumped {
                        world.bodies[body.handle].set_position(to, true);
                        body.from = to;
                    }
                    body.to = to;
                }
                None => {
                    let h = p.half.max(Vec3::splat(0.05));
                    let collider = match &p.shape {
                        Some(shape) => ColliderBuilder::new(shape.clone()),
                        None => ColliderBuilder::cuboid(h.x, h.y, h.z),
                    };
                    let (handle, _) = world.insert(
                        RigidBodyBuilder::kinematic_position_based().pose(to),
                        collider.friction(0.3),
                    );
                    self.bodies.insert(
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
    /// Move every pusher `t` (0 to 1) of the way through this frame's
    /// motion; call before each step.
    pub fn drive(&self, world: &mut PhysicsWorld, t: f32) {
        for pusher in self.bodies.values() {
            world.bodies[pusher.handle]
                .set_next_kinematic_position(pusher.from.lerp(&pusher.to, t));
        }
    }
    /// The frame's steps are done: next frame starts where this one ended.
    pub fn settle(&mut self) {
        for pusher in self.bodies.values_mut() {
            pusher.from = pusher.to;
        }
    }
    /// Take every pusher out of the world.
    pub fn clear(&mut self, world: &mut PhysicsWorld) {
        for (_, pusher) in std::mem::take(&mut self.bodies) {
            world.remove_body_with_colliders(pusher.handle, true);
        }
    }
    pub fn len(&self) -> usize {
        self.bodies.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }
    /// The kinematic body standing for pusher `id`.
    pub fn handle(&self, id: u64) -> Option<RigidBodyHandle> {
        self.bodies.get(&id).map(|p| p.handle)
    }
}

/// Where `bodies` are now (their origins, which openings carry them by): what [`carry_through_openings`] compares
/// after the next step.
pub fn middles(
    world: &PhysicsWorld,
    bodies: impl Iterator<Item = RigidBodyHandle>,
) -> Vec<(RigidBodyHandle, Vec3)> {
    bodies
        .filter_map(|h| Some((h, world.bodies.get(h)?.translation())))
        .collect()
}

/// Carry each body whose middle went in through an opening of a linked
/// brick since `before` (each one's middle then) out of its partner, turned
/// with its velocity and spin, as the host carries players and vehicles:
/// debris knocked into a portal flies out of the other one instead of out
/// of the doorway's back. Returns how many were carried.
pub fn carry_through_openings(
    world: &mut PhysicsWorld,
    passages: &bri_content::passage::Passages,
    before: &[(RigidBodyHandle, Vec3)],
) -> usize {
    if passages.list.is_empty() {
        return 0;
    }
    let mut carried = 0;
    for &(handle, from) in before {
        let Some(body) = world.bodies.get_mut(handle) else {
            continue;
        };
        let (_, Some(carry)) = passages.travel(from, body.translation()) else {
            continue;
        };
        let (_, turn, _) = carry.to_scale_rotation_translation();
        let position = *body.position();
        let moved = Pose::from_parts(
            carry.transform_point3(position.translation),
            (turn * position.rotation).normalize(),
        );
        let (linear, angular) = (turn * body.linvel(), turn * body.angvel());
        body.set_position(moved, true);
        body.set_linvel(linear, true);
        body.set_angvel(angular, true);
        carried += 1;
    }
    carried
}

/// Projectiles passing through bodies: each strikes a body once.
#[derive(Default)]
pub struct Shots {
    /// Projectile id -> where it was last frame.
    last: BTreeMap<u64, Vec3>,
    /// (projectile, body) pairs already struck.
    struck: BTreeSet<(u64, u64)>,
    segments: Vec<(u64, Vec3, Vec3, Vec3)>,
}

/// One projectile passing through one body this frame.
pub struct Strike {
    /// The body's own id (what `owners` maps its handle to).
    pub body: u64,
    pub handle: RigidBodyHandle,
    pub point: Vec3,
    pub direction: Vec3,
    pub speed: f32,
}

impl Shots {
    pub fn segments(&self) -> &[(u64, Vec3, Vec3, Vec3)] {
        &self.segments
    }
    pub fn struck(&self, projectile: u64, body: u64) -> bool {
        self.struck.contains(&(projectile, body))
    }
    /// Where projectiles are this frame; returns each body a projectile
    /// passed through since last frame, once per projectile and body.
    /// `owners` names the dynamic bodies that count.
    pub fn strike(
        &mut self,
        world: &PhysicsWorld,
        shots: &[Shot],
        owners: &HashMap<RigidBodyHandle, u64>,
    ) -> Vec<Strike> {
        let live: BTreeSet<u64> = shots.iter().map(|s| s.id).collect();
        self.last.retain(|id, _| live.contains(id));
        self.struck.retain(|(p, _)| live.contains(p));
        let mut out = Vec::new();
        self.segments.clear();
        let mut paused_queries = 512;
        for shot in shots {
            if !shot.position.is_finite() || !shot.velocity.is_finite() {
                continue;
            }
            let from = self
                .last
                .insert(shot.id, shot.position)
                .unwrap_or(shot.position - shot.velocity * STEP);
            let delta = shot.position - from;
            let length = delta.length();
            if !(1e-5..=100.0).contains(&length) {
                continue;
            }
            self.segments
                .push((shot.id, from, shot.position, shot.velocity));
            if owners.is_empty() {
                continue;
            }
            let direction = delta / length;
            let ray = Ray::new(
                Vector::from_array(from.to_array()),
                Vector::from_array(direction.to_array()),
            );
            let hits: Vec<(u64, RigidBodyHandle, Vec3)> = world
                .intersect_ray(ray, length, true, QueryFilter::only_dynamic())
                .filter_map(|(_, c, hit)| {
                    let handle = c.parent()?;
                    let id = *owners.get(&handle)?;
                    Some((id, handle, from + direction * hit.time_of_impact))
                })
                .collect();
            let mut hits = hits;
            for (&handle, &body) in owners {
                let Some(rb) = world.bodies.get(handle) else {
                    continue;
                };
                if rb.is_enabled() || self.struck.contains(&(shot.id, body)) {
                    continue;
                }
                if paused_queries == 0 {
                    break;
                }
                paused_queries -= 1;
                for collider in rb.colliders() {
                    let c = &world.colliders[*collider];
                    if let Some(hit) = c.shape().cast_ray(c.position(), &ray, length, true) {
                        hits.push((body, handle, from + direction * hit));
                        break;
                    }
                }
            }
            for (body, handle, point) in hits {
                if self.struck.insert((shot.id, body)) {
                    out.push(Strike {
                        body,
                        handle,
                        point,
                        direction,
                        speed: shot.velocity.length(),
                    });
                }
            }
        }
        out
    }
}
