use crate::{
    definitions::{Definition, Definitions, Special, brick_water},
    grid::{self, Bounds, Index},
};
use anyhow::{Context, Result, ensure};
use bri_world::{
    Brick, BrickId, World,
    authority::{Actor, Authority, Edit},
};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
mod scan;
pub use scan::{BoxScan, StackScan, center_copy, spend, work};
// Brick IDs occupy u64; zero remains available for untagged dynamic bodies.
pub const MAP_TAG: u128 = u128::MAX;
/// How far a brick may dip into an upward-facing map floor. Map floors need
/// not lie on the plate lattice; v20 rests bricks on the nearest plane, so its
/// stock layouts dip up to half a plate in (Kitchen's Town 0.084, a Bedroom
/// shelf 0.062, Pirate World 0.034) and it never refuses such a placement.
pub const FLOOR_DIP: f32 = 0.1;
/// Synchronous placement cannot yield. Refuse pathological support queries
/// explicitly instead of scanning millions of cells or buckets in one tick.
const SUPPORT_QUERY_LIMIT: u32 = 256;
// Footprint queries have a different bounded cost from neighbor/connector
// queries. Admit up to 64x64 authored cells without spending their allowance
// on bucket visits, contacts, or one another.
const FOOTPRINT_CELL_LIMIT: u32 = 64 * 64;
/// Why a brick could not be planted. Clients show the original plant-error
/// icons for these rather than a generic rejection message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlantFailure {
    Overlap,
    Float,
    /// Embedded in map geometry.
    Buried,
    /// Embedded in a player, vehicle or other moving entity.
    Stuck,
    TooFar,
    Forbidden,
    Limit,
}
impl std::fmt::Display for PlantFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Overlap => "Brick overlaps another brick",
            Self::Float => "Brick is floating: no stud connection or map support",
            Self::Buried => "Brick is buried in the map",
            Self::Stuck => "Brick is stuck in a player or object",
            Self::TooFar => "Brick is too far away",
            Self::Forbidden => "You do not have permission to build here",
            Self::Limit => "Brick limit reached",
        })
    }
}
impl std::error::Error for PlantFailure {}

pub struct Builder<'a> {
    pub actor: &'a Actor,
    pub position: Vec3,
    pub reach: f32,
}
/// Required-support validation shared by direct group placement and sliced
/// blueprint jobs. Native per-brick checks feed it before any group is published.
#[derive(Default)]
pub(crate) struct GroupSupport {
    // Only identity/rotation/occupancy are needed to inspect stud connections.
    // Keep gameplay metadata and copied events out of the preflight state.
    shapes: Vec<(String, u8)>,
    bounds: Vec<Bounds>,
    index: Index,
    roots: Vec<bool>,
    cut: Vec<bool>,
    pending: Vec<usize>,
    current: Option<(usize, grid::QueryCursor)>,
    connection: Option<(usize, usize, grid::ConnectionCells)>,
    verify: usize,
    remaining: usize,
    supported: bool,
    obstructed: bool,
}
impl GroupSupport {
    pub(crate) fn check(&mut self, sim: &Simulation, actor: &Actor, brick: &Brick) -> Result<bool> {
        let (rooted, blocked) = check_placement_support(
            sim.state(),
            &sim.definitions,
            &sim.index,
            sim.ground(),
            actor,
            brick,
        )?;
        let bounds = Bounds::new(brick, &sim.definitions.get(brick)?.mesh)?;
        let bri_world::ContentRef::Resolved(id) = &brick.definition else {
            anyhow::bail!("Unresolved brick definition");
        };
        let (min, max) = grid::bucket_span(bounds);
        let buckets = (0..3).fold(1u64, |n, a| {
            n.saturating_mul((i64::from(max[a]) - i64::from(min[a]) + 1) as u64)
        });
        if buckets > SUPPORT_QUERY_LIMIT as u64 {
            return Err(PlantFailure::Limit.into());
        }
        let i = self.shapes.len();
        self.index.insert(i as BrickId, bounds);
        self.shapes.push((id.clone(), brick.quarter_turns));
        self.bounds.push(bounds);
        self.roots.push(rooted);
        self.cut.push(blocked);
        self.supported |= rooted;
        self.obstructed |= blocked;
        if rooted {
            self.pending.push(i);
        } else {
            self.remaining += 1;
        }
        Ok(rooted)
    }
    pub(crate) fn step(&mut self, sim: &Simulation, budget: &mut u32) -> Result<bool> {
        if self.remaining == 0 {
            return Ok(true);
        }
        if !self.supported {
            return if self.obstructed {
                Err(PlantFailure::Buried.into())
            } else {
                Ok(true)
            };
        }
        let ground = sim.ground();
        let interior = |handle, c: &Collider| ground.interior(handle, c);
        let query = sim
            .physics
            .query_pipeline_with_filter(QueryFilter::default().predicate(&interior));
        loop {
            if self.remaining == 0 {
                return Ok(true);
            }
            if let Some((i, j, cells)) = self.connection.as_mut() {
                if !spend(budget, work::SEARCH) {
                    return Ok(false);
                }
                let am = &sim.definitions.by_id(&self.shapes[*j].0)?.mesh;
                let bm = &sim.definitions.by_id(&self.shapes[*i].0)?.mesh;
                match cells.next(self.shapes[*j].1, am, self.shapes[*i].1, bm) {
                    Some((cell, neighbor, true)) => {
                        if map_connector_clear(&query, cell, neighbor) {
                            self.roots[*j] = true;
                            self.remaining -= 1;
                            self.pending.push(*j);
                            self.connection = None;
                        } else {
                            self.cut[*j] = true;
                        }
                    }
                    Some(_) => {}
                    None => self.connection = None,
                }
                continue;
            }
            if let Some((i, neighbors)) = self.current.as_mut() {
                if !spend(budget, work::SCAN) {
                    return Ok(false);
                }
                match neighbors.step(&self.index) {
                    Some(Some(j)) if !self.roots[j as usize] => {
                        let j = j as usize;
                        self.connection = Some((
                            *i,
                            j,
                            grid::ConnectionCells::new(self.bounds[j], self.bounds[*i]),
                        ));
                    }
                    Some(_) => {}
                    None => self.current = None,
                }
                continue;
            }
            if let Some(i) = self.pending.last().copied() {
                if !spend(budget, work::SEARCH) {
                    return Ok(false);
                }
                self.pending.pop();
                self.current = Some((i, grid::QueryCursor::new(self.bounds[i].expanded(1))));
                continue;
            }
            // A failed connector matters only if no alternate clear route
            // reached that member. Disconnected-group policy is unchanged.
            while self.verify < self.roots.len() {
                if !spend(budget, work::SCAN) {
                    return Ok(false);
                }
                let i = self.verify;
                self.verify += 1;
                if !self.roots[i] && self.cut[i] {
                    return Err(PlantFailure::Buried.into());
                }
            }
            return Ok(true);
        }
    }
}
/// The surface normal a hit reports, always a unit vector. A ray that
/// starts inside a shape hits it at distance zero with no normal (the a16
/// shell-casing crash normalized that into NaN); that surface faces back
/// along the ray.
pub fn hit_normal(normal: Vec3, direction: Vec3) -> Vec3 {
    normal
        .try_normalize()
        .or_else(|| (-direction).try_normalize())
        .unwrap_or(Vec3::Y)
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub brick: Option<BrickId>,
    pub position: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}
/// Map liquids plus water bricks, shared.
pub type Liquids = std::sync::Arc<[bri_content::water::Water]>;
pub struct Simulation {
    authority: Authority,
    pub definitions: Definitions,
    pub physics: PhysicsWorld,
    /// Map liquids. Water bricks add their own volumes (see `liquids`).
    pub waters: Vec<bri_content::water::Water>,
    brick_waters: BTreeMap<BrickId, bri_content::water::Water>,
    /// `liquids()` as last built, with the map liquids it was built from
    /// (address and count); water bricks changing clears it.
    liquids: std::sync::OnceLock<(usize, usize, Liquids)>,
    index: Index,
    /// Build-support topology, independent of moving bodies and paint/name edits.
    support_epoch: u64,
    /// Colliders of the bricks that keep one of their own (sensors: not
    /// colliding, or water). Solid bricks are parts of `chunks`.
    handles: BTreeMap<BrickId, ColliderHandle>,
    /// Solid bricks' shared colliders (see `chunks`).
    chunks: crate::chunks::Chunks,
    /// Removed bricks' colliders (see `parking`).
    parked: crate::parking::Parking,
    /// Removals leave collisions to [`Self::settle`] while true
    /// ([`Self::hold_settle`]).
    holding: bool,
    /// Map colliders in `NativeMap::colliders` order.
    map_handles: Vec<ColliderHandle>,
    /// The map colliders of static shapes (trees, props), which a plant
    /// never asks about ([`Ground::interior`]).
    map_statics: rustc_hash::FxHashSet<ColliderHandle>,
    terrain: Option<crate::map::TerrainStream>,
    /// Collision refreshes run so far (see `collision_refreshes`).
    refreshes: u64,
    /// Linked bricks and the openings bodies pass through.
    links: crate::links::Links,
    /// Every brick by its definition: what `bricks_of` answers without
    /// walking a million-brick world (team spawns, flag stands).
    kinds: BTreeMap<String, BTreeSet<BrickId>>,
    /// Bricks whose stack belongs to someone else than their owner (see
    /// [`Self::stack_owner`]). Not saved, as v20's `stackBL_ID` was not.
    stacks: std::collections::HashMap<BrickId, bri_world::OwnerId>,
}
fn support_identity(brick: &Brick) -> (bri_world::ContentRef, [f32; 3], u8, bri_world::OwnerId) {
    (
        brick.definition.clone(),
        brick.position,
        brick.quarter_turns,
        brick.owner,
    )
}
fn definition_key(brick: &Brick) -> Option<&str> {
    match &brick.definition {
        bri_world::ContentRef::Resolved(id) => Some(id),
        _ => None,
    }
}
fn pose(brick: &Brick) -> Pose {
    grid_pose(brick.position, brick.quarter_turns)
}
/// Where a brick at `position` turned `quarter_turns` places its collision.
pub fn grid_pose(position: [f32; 3], quarter_turns: u8) -> Pose {
    Pose::from_parts(
        Vector::from_array(position),
        Rotation::from_scaled_axis(
            Vector::Y * (-f32::from(quarter_turns) * std::f32::consts::FRAC_PI_2),
        ),
    )
}
/// Whether a brick is a part of its chunk's shared collider (it collides
/// and is not water) rather than a sensor collider of its own.
pub fn solid(brick: &Brick, definition: &Definition) -> bool {
    brick.colliding && definition.special != Special::Water
}
/// A brick's collision shape where it stands, as a chunk part.
pub fn brick_shape(brick: &Brick, definition: &Definition) -> (Pose, SharedShape) {
    (pose(brick), definition.shape.clone())
}
/// Brick collision exactly as the authority inserts it; client prediction reuses it.
pub fn brick_collider(brick: &Brick, definition: &Definition, id: BrickId) -> ColliderBuilder {
    ColliderBuilder::new(definition.shape.clone())
        .position(pose(brick))
        .sensor(!brick.colliding || definition.special == Special::Water)
        .user_data(u128::from(id))
}
/// `$TrustLevel::BuildOn`.
/// Enable or disable a range of map colliders; shared with client mirrors.
pub fn set_enabled(
    physics: &mut PhysicsWorld,
    handles: &[ColliderHandle],
    colliders: std::ops::Range<usize>,
    enabled: bool,
) -> Result<()> {
    let handles = handles.get(colliders).context("Unknown map colliders")?;
    for handle in handles {
        physics.colliders[*handle].set_enabled(enabled);
    }
    bri_physics::detect_collisions(physics);
    Ok(())
}
/// Which way a stack selection goes from its first brick
/// ([`Simulation::select_stack`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackReach {
    pub up: bool,
    pub limited: bool,
}
/// What [`Simulation::plant_each`] does with a brick nothing holds up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// It is refused (`PlantFailure::Float`).
    Required,
    /// It becomes a baseplate.
    Float,
    /// It plants as it is.
    Free,
}
/// The bricks a selection took, in order, and what it left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub bricks: Vec<BrickId>,
    /// It stopped at its limit with more to take.
    pub limit_reached: bool,
    /// Bricks it reached but was not allowed to take.
    pub refused: usize,
}
fn may_build_on(actor: &Actor, brick: &Brick) -> bool {
    actor.trusted(brick.owner, bri_world::authority::trust::BUILD)
}
/// "3 bricks were not loaded because this server does not have their
/// definitions: 2 v20/brick/x, 1 other:brick/y. ...", or None when all loaded.
pub fn unloaded_summary(bricks: &[Brick]) -> Option<String> {
    if bricks.is_empty() {
        return None;
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for brick in bricks {
        let name = match &brick.definition {
            bri_world::ContentRef::Resolved(id) => id.clone(),
            bri_world::ContentRef::Unresolved(u) => format!("{}/{}", u.namespace, u.name),
        };
        *counts.entry(name).or_default() += 1;
    }
    let mut kinds: Vec<String> = counts
        .iter()
        .take(8)
        .map(|(name, count)| format!("{count} {name}"))
        .collect();
    if counts.len() > 8 {
        kinds.push(format!("and {} more kinds", counts.len() - 8));
    }
    Some(format!(
        "{} bricks were not loaded because this server does not have their definitions: {}. They are kept and saved with the world.",
        bricks.len(),
        kinds.join(", ")
    ))
}

impl Simulation {
    pub fn new(
        mut world: World,
        definitions: Definitions,
        map: Vec<ColliderBuilder>,
    ) -> Result<Self> {
        world.validate()?;
        // A brick this server has no definition for is kept aside, not
        // refused: the rest of the world still loads (see `World::unloaded`).
        let missing: Vec<BrickId> = world
            .bricks
            .iter()
            .filter(|(_, b)| definitions.get(b).is_err())
            .map(|(id, _)| *id)
            .collect();
        for id in missing {
            if let Some(brick) = world.bricks.remove(&id) {
                world.unloaded.push(brick);
            }
        }
        let mut physics = bri_physics::new_world();
        let mut map_statics = rustc_hash::FxHashSet::default();
        let map_handles = map
            .into_iter()
            .map(|c| {
                let shape = c.user_data == crate::map::MapSurface::Static as u128;
                let handle = physics.insert_collider(c.user_data(MAP_TAG), None);
                if shape {
                    map_statics.insert(handle);
                }
                handle
            })
            .collect();
        let mut index = Index::default();
        let mut handles = BTreeMap::new();
        let mut chunks = crate::chunks::Chunks::default();
        let mut brick_waters = BTreeMap::new();
        let mut kinds: BTreeMap<String, BTreeSet<BrickId>> = BTreeMap::new();
        for (id, brick) in &world.bricks {
            let definition = definitions.get(brick)?;
            if let Some(key) = definition_key(brick) {
                kinds.entry(key.to_string()).or_default().insert(*id);
            }
            if let Some(water) = brick_water(brick, definition) {
                brick_waters.insert(*id, water);
            }
            let bounds = Bounds::new(brick, &definition.mesh)?;
            index.insert(*id, bounds);
            if solid(brick, definition) {
                chunks.insert(*id, brick.position);
            } else {
                handles.insert(
                    *id,
                    physics.insert_collider(brick_collider(brick, definition, *id), None),
                );
            }
        }
        let mut simulation = Self {
            authority: Authority::new(world)?,
            definitions,
            physics,
            waters: Vec::new(),
            brick_waters,
            liquids: std::sync::OnceLock::new(),
            index,
            support_epoch: 0,
            handles,
            chunks,
            parked: Default::default(),
            holding: false,
            map_handles,
            map_statics,
            terrain: None,
            refreshes: 0,
            links: Default::default(),
            kinds,
            stacks: Default::default(),
        };
        simulation.links.reset(
            &simulation.authority.state().bricks,
            &simulation.definitions,
        );
        simulation.detect_collisions();
        Ok(simulation)
    }
    /// Bring collision up to date with bricks placed without a refresh
    /// (`load_build_unrefreshed`): chunks rebuilt, new colliders queryable.
    pub fn refresh_collisions(&mut self) {
        self.detect_collisions();
    }
    /// Collision refreshes (chunk rebuilds plus a physics pass) run so far.
    /// Each costs about a chunk rebuild, so bulk brick changes share one.
    pub fn collision_refreshes(&self) -> u64 {
        self.refreshes
    }
    /// The solid bricks' chunk colliders, for mapping a part to its brick.
    pub fn chunks(&self) -> &crate::chunks::Chunks {
        &self.chunks
    }
    /// Move a player-motor body one step, touching bricks by their ids.
    pub fn step_body(
        &mut self,
        body: &mut crate::player::Player,
        input: crate::player::MoveInput,
        waters: &[bri_content::water::Water],
    ) -> Result<crate::player::MotionEvents> {
        self.flush_chunks();
        self.links
            .flush(&self.authority.state().bricks, &self.definitions);
        body.step_through(
            &mut self.physics,
            input,
            waters,
            &self.chunks,
            self.links.passages(),
        )
    }
    /// Step vehicle actors against the same current chunks and linked
    /// openings as `step_body`, before the shared physics step.
    pub fn step_vehicle_bodies(
        &mut self,
        vehicles: &mut bri_vehicles::VehiclesWorld,
        waters: &[bri_content::water::Water],
    ) -> Result<Vec<(bri_vehicles::VehicleId, glam::Affine3A)>> {
        self.flush_chunks();
        self.links
            .flush(&self.authority.state().bricks, &self.definitions);
        vehicles.pre_step_through(
            &mut self.physics,
            waters,
            &self.chunks,
            self.links.passages(),
        )
    }
    pub(crate) fn support_epoch(&self) -> u64 {
        self.support_epoch
    }
    /// Give a brick in the world its collision: a part of its chunk (built
    /// at the next flush) or, for a sensor, a collider of its own.
    fn attach(&mut self, id: BrickId) -> Result<()> {
        self.support_epoch = self.support_epoch.wrapping_add(1);
        let brick = &self.authority.state().bricks[&id];
        let definition = self.definitions.get(brick)?;
        if definition.link.is_some() {
            self.links.touch(id);
        }
        if solid(brick, definition) {
            self.chunks.insert(id, brick.position);
        } else {
            let collider = brick_collider(brick, definition, id);
            self.handles
                .insert(id, self.physics.insert_collider(collider, None));
        }
        Ok(())
    }
    /// Take a brick's collision away (it is removed or changing): out of its
    /// chunk, waking bodies resting on it, or its own collider handed back
    /// for `parking`.
    fn detach(&mut self, id: BrickId) -> Option<ColliderHandle> {
        self.support_epoch = self.support_epoch.wrapping_add(1);
        self.note_link(id);
        if let Some(handle) = self.handles.remove(&id) {
            return Some(handle);
        }
        let brick = self.authority.state().bricks.get(&id)?;
        let aabb = self
            .definitions
            .get(brick)
            .ok()
            .map(|definition| definition.shape.compute_aabb(&pose(brick)));
        if self.chunks.remove(id, brick.position)
            && let Some(aabb) = aabb
        {
            crate::parking::wake_resting(&mut self.physics, aabb);
        }
        None
    }
    /// A brick that is or may become linked changed.
    fn note_link(&mut self, id: BrickId) {
        let brick = self.authority.state().bricks.get(&id);
        if self.links.may_link(id, brick, &self.definitions) {
            self.links.touch(id);
        }
    }
    /// Linked bricks as the world stands now.
    pub fn links(&mut self) -> &crate::links::Links {
        self.links
            .flush(&self.authority.state().bricks, &self.definitions);
        &self.links
    }
    /// The openings of linked bricks as of the last [`Self::links`] (every
    /// body step reads those first).
    pub fn passages(&self) -> &bri_content::passage::Passages {
        self.links.passages()
    }
    /// Rebuild the chunks bricks changed since the last flush.
    fn flush_chunks(&mut self) {
        if !self.chunks.is_dirty() {
            return;
        }
        let Self {
            chunks,
            physics,
            parked,
            authority,
            definitions,
            ..
        } = self;
        let bricks = &authority.state().bricks;
        chunks.flush(physics, parked, |id| {
            let brick = bricks.get(&id)?;
            let definition = definitions.get(brick).ok()?;
            Some(brick_shape(brick, definition))
        });
    }
    /// Position of a map collider in `NativeMap::colliders`.
    pub fn map_collider_index(&self, handle: ColliderHandle) -> Option<usize> {
        self.map_handles.iter().position(|h| *h == handle)
    }
    /// Make map colliders (a hidden static shape) intangible to every body
    /// and query, or solid again.
    pub fn set_map_colliders(
        &mut self,
        colliders: std::ops::Range<usize>,
        enabled: bool,
    ) -> Result<()> {
        for handle in self.map_handles.get(colliders.clone()).unwrap_or_default() {
            if let Some(collider) = self.physics.colliders.get(*handle) {
                self.chunks.note_changed(&collider.compute_aabb());
            }
        }
        let changed = self
            .map_handles
            .get(colliders.clone())
            .unwrap_or_default()
            .iter()
            .any(|h| {
                self.physics
                    .colliders
                    .get(*h)
                    .is_some_and(|c| c.is_enabled() != enabled)
            });
        set_enabled(&mut self.physics, &self.map_handles, colliders, enabled)?;
        if changed {
            self.support_epoch = self.support_epoch.wrapping_add(1);
        }
        Ok(())
    }
    /// Record where fixed collision changes (bricks, map shapes) for
    /// [`Self::take_collision_changes`], or stop.
    pub fn track_collision_changes(&mut self, track: bool) {
        self.chunks.track_changes(track);
    }
    /// Boxes (min, max) where fixed collision changed since the last take:
    /// the shapes before and after, as chunks were rebuilt.
    pub fn take_collision_changes(&mut self) -> Vec<(Vec3, Vec3)> {
        self.chunks
            .take_changed()
            .into_iter()
            .map(|(min, max)| (Vec3::from(min), Vec3::from(max)))
            .collect()
    }
    /// What a plant is judged against besides the bricks.
    fn ground(&self) -> Ground<'_> {
        Ground {
            physics: &self.physics,
            terrain: self.terrain.as_ref(),
            statics: &self.map_statics,
        }
    }
    /// Stream the map's terrain collision around moving bodies and `anchors`
    /// (authored spawn regions). Replaces any previously attached terrain.
    pub fn attach_terrain(
        &mut self,
        fields: Vec<std::sync::Arc<bri_content::terrain_field::TerrainField>>,
        anchors: Vec<bri_physics::terrain::Focus>,
    ) -> Result<()> {
        let mut stream = crate::map::TerrainStream::new(fields, MAP_TAG, anchors)?;
        stream.update(&mut self.physics);
        self.terrain = Some(stream);
        self.support_epoch = self.support_epoch.wrapping_add(1);
        Ok(())
    }
    /// Refresh streamed terrain after bodies were added or moved outside a
    /// step (spawns, teleports). `step` refreshes it automatically.
    pub fn stream_terrain(&mut self) {
        if let Some(terrain) = &mut self.terrain {
            terrain.update(&mut self.physics);
        }
    }
    /// Currently loaded terrain collision tiles.
    pub fn terrain_tiles(&self) -> usize {
        self.terrain.as_ref().map_or(0, |t| t.active_tiles())
    }
    /// Exact terrain ray (normalized direction), independent of loaded tiles.
    pub fn terrain_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(f32, Vec3)> {
        self.terrain
            .as_ref()
            .and_then(|t| t.cast_ray(origin, direction, max_distance))
    }
    /// Rapier's collision-only pass consumes a newly inserted body's pending
    /// changes without an island manager. A body spawned since the last
    /// physics step (a player joining, a package entity) would then never
    /// enter an island and trip Rapier's consistency check on the next step,
    /// so every body is marked modified again afterwards.
    fn detect_collisions(&mut self) {
        self.refreshes += 1;
        self.flush_chunks();
        bri_physics::detect_collisions(&mut self.physics);
        for _ in self.physics.bodies.iter_mut() {}
    }
    pub fn state(&self) -> &World {
        self.authority.state()
    }
    /// Record which principal an owner number belongs to in this world.
    pub fn claim_owner(
        &mut self,
        owner: bri_world::OwnerId,
        record: bri_world::OwnerRecord,
    ) -> Result<()> {
        self.authority.claim_owner(owner, record)
    }
    /// Split a load's bricks into those this server can place and those it
    /// has no definition for.
    pub fn split_placeable(&self, bricks: Vec<Brick>) -> (Vec<Brick>, Vec<Brick>) {
        bricks
            .into_iter()
            .partition(|b| self.definitions.get(b).is_ok())
    }
    /// v20 `ServerLoadSaveFile_Tick`: each loaded brick is planted, and one
    /// that overlaps a brick already there (plant error 1) is deleted and
    /// counted as a failure. Returns the bricks to place, in order; a brick
    /// is also checked against the bricks kept before it, as v20 plants them
    /// one at a time.
    pub fn drop_overlapping(&self, bricks: Vec<Brick>) -> Result<Vec<Brick>> {
        let world = self.state();
        let mut kept: Vec<(Brick, Bounds)> = Vec::with_capacity(bricks.len());
        let mut batch = Index::default();
        for brick in bricks {
            let mesh = &self.definitions.get(&brick)?.mesh;
            let bounds = Bounds::new(&brick, mesh)?;
            let mut overlap =
                overlaps_world(world, &self.definitions, &self.index, &brick, mesh, bounds)?;
            if !overlap {
                let mut error = None;
                overlap = batch.any(bounds, |i| {
                    let (other, ob) = &kept[i as usize];
                    match self.definitions.get(other) {
                        Ok(definition) => {
                            grid::overlaps((&brick, mesh, bounds), (other, &definition.mesh, *ob))
                        }
                        Err(e) => {
                            error = Some(e);
                            true
                        }
                    }
                });
                if let Some(error) = error {
                    return Err(error);
                }
            }
            if !overlap {
                batch.insert(kept.len() as BrickId, bounds);
                kept.push((brick, bounds));
            }
        }
        Ok(kept.into_iter().map(|(b, _)| b).collect())
    }
    /// Whether a brick has a definition here and sits on its build grid.
    pub fn fits_grid(&self, brick: &Brick) -> bool {
        self.definitions
            .get(brick)
            .is_ok_and(|definition| Bounds::new(brick, &definition.mesh).is_ok())
    }
    /// A loaded brick on the stud and plate grid: as saved when it is on
    /// it, else moved to the nearest grid position (a save made where the
    /// brick had another size). v20 plants a saved brick wherever the save
    /// put it; this world's occupancy needs the grid, so the brick is moved
    /// by under half a cell rather than lost. None without a definition.
    pub fn on_grid(&self, mut brick: Brick) -> Option<Brick> {
        let definition = self.definitions.get(&brick).ok()?;
        if Bounds::new(&brick, &definition.mesh).is_err() {
            brick.position = Bounds::snapped(brick.position, brick.quarter_turns, &definition.mesh);
        }
        Bounds::new(&brick, &definition.mesh)
            .is_ok()
            .then_some(brick)
    }
    /// Keep bricks without a definition with the world; see
    /// [`bri_world::authority::Authority::keep_unloaded`].
    pub fn keep_unloaded(&mut self, palette: &[[f32; 4]], bricks: Vec<Brick>) -> Result<()> {
        self.authority.keep_unloaded(palette, bricks)
    }
    /// Check every placeable brick of a load against its definition before
    /// any of it is published. Bricks without a definition are not placed
    /// ([`Self::split_placeable`]).
    pub fn preflight_load(&self, plan: &bri_world::build::LoadPlan) -> Result<()> {
        for brick in plan.bricks().values() {
            if let Ok(definition) = self.definitions.get(brick) {
                Bounds::new(brick, &definition.mesh)?;
            }
        }
        Ok(())
    }
    pub fn load_build(
        &mut self,
        actor: &Actor,
        plan: bri_world::build::LoadPlan,
    ) -> Result<Vec<BrickId>> {
        let ids = self.load_build_unrefreshed(actor, plan)?;
        self.detect_collisions();
        Ok(ids)
    }
    /// [`Self::load_build`] without the collision refresh: a streamed load
    /// leaves its new colliders to the next physics step.
    pub fn load_build_unrefreshed(
        &mut self,
        actor: &Actor,
        plan: bri_world::build::LoadPlan,
    ) -> Result<Vec<BrickId>> {
        ensure!(
            actor.administrator,
            "Only the host/administrator may load builds"
        );
        // Preflight the whole build, then publish state/index/colliders together.
        // Existing players, bodies, bricks and scheduled events stay alive.
        let mut prepared = Vec::new();
        for (id, brick) in plan.bricks() {
            let definition = self.definitions.get(brick)?;
            prepared.push((*id, Bounds::new(brick, &definition.mesh)?));
        }
        let ids = self.authority.load_build(actor, plan)?;
        for (id, bounds) in prepared {
            self.index.insert(id, bounds);
            self.note_kind(id);
            let brick = &self.authority.state().bricks[&id];
            if let Some(water) = brick_water(brick, self.definitions.get(brick)?) {
                self.brick_waters.insert(id, water);
                self.liquids = std::sync::OnceLock::new();
            }
            self.attach(id)?;
        }
        Ok(ids)
    }
    pub fn plant(&mut self, builder: &Builder<'_>, brick: Brick) -> Result<BrickId> {
        if self.state().bricks.len() >= bri_world::MAX_BRICKS {
            return Err(PlantFailure::Limit.into());
        }
        let definition = self.definitions.get(&brick)?;
        let bounds = Bounds::new(&brick, &definition.mesh)?;
        let defs = &self.definitions;
        let index = &self.index;
        let ground = Ground {
            physics: &self.physics,
            terrain: self.terrain.as_ref(),
            statics: &self.map_statics,
        };
        let passages = self.links.passages();
        let id = self.authority.plant(builder.actor, brick, |world, brick| {
            validate_placement(world, defs, index, ground, passages, builder, brick)
        })?;
        self.attach(id)?;
        self.note_kind(id);
        let brick = &self.authority.state().bricks[&id];
        self.index.insert(id, bounds);
        if let Some(water) = brick_water(brick, self.definitions.get(brick)?) {
            self.brick_waters.insert(id, water);
            self.liquids = std::sync::OnceLock::new();
        }
        self.note_stack(id);
        self.detect_collisions();
        Ok(id)
    }
    /// Plant bricks as one: each passes every plant rule but reach and
    /// support against the world as it stands, the world holds up at least
    /// one of them (they are joined, so it holds up the rest), and either
    /// all are planted or none is. The caller checks reach, rate and the
    /// brick limit, as for a single plant.
    pub fn plant_group(&mut self, actor: &Actor, bricks: Vec<Brick>) -> Result<Vec<BrickId>> {
        self.place_group(actor, bricks, false, true)
    }
    /// [`Self::plant_group`] where nothing need hold the bricks up: with
    /// none resting on anything, the lowest one becomes a baseplate, ground
    /// to the bricks joined to it (v20's force plant set `isBaseplate`).
    pub fn plant_group_floating(
        &mut self,
        actor: &Actor,
        mut bricks: Vec<Brick>,
    ) -> Result<Vec<BrickId>> {
        ensure!(!bricks.is_empty(), "Nothing to plant");
        let mut supported = false;
        for brick in &bricks {
            supported |= check_placement(
                self.authority.state(),
                &self.definitions,
                &self.index,
                self.ground(),
                actor,
                brick,
            )?;
        }
        if !supported
            && let Some(lowest) = bricks
                .iter_mut()
                .min_by(|a, b| a.position[1].total_cmp(&b.position[1]))
        {
            lowest.base_plate = true;
        }
        self.place_group(actor, bricks, false, false)
    }
    /// Plant bricks one at a time in order, as v20's duplicators planted a
    /// copy: each passes every plant rule but reach against the world as
    /// it stands, the bricks planted before it included, and one that
    /// does not is skipped. A brick with nothing under it yet is tried
    /// again after the rest, as long as more get planted, so the order of
    /// a copy never leaves a brick floating that its own bricks hold up.
    /// The planted ids, and why each other brick was refused.
    ///
    /// `support` says what a brick with nothing under it does: waits and is
    /// refused, becomes a baseplate (v20's force plant: ground to what is
    /// built on it, tried only once nothing else of the copy holds it up),
    /// or plants as it is (`Free`: bricks put back where a build stood).
    pub fn plant_each(
        &mut self,
        actor: &Actor,
        bricks: Vec<Brick>,
        support: Support,
    ) -> (Vec<BrickId>, Vec<anyhow::Error>) {
        let mut ids = Vec::with_capacity(bricks.len());
        let mut waiting = bricks;
        let mut refused = Vec::new();
        loop {
            let before = ids.len();
            let mut floating = Vec::new();
            let mut float_errors = Vec::new();
            for brick in std::mem::take(&mut waiting) {
                match self.plant_one(actor, brick.clone(), support == Support::Free) {
                    Ok(id) => ids.push(id),
                    Err(error) => {
                        if matches!(error.downcast_ref(), Some(PlantFailure::Float)) {
                            floating.push(brick);
                            float_errors.push(error);
                        } else {
                            refused.push(error);
                        }
                    }
                }
            }
            if floating.is_empty() {
                break;
            }
            if ids.len() == before {
                if support != Support::Float {
                    refused.extend(float_errors);
                    break;
                }
                // Nothing of the copy holds the rest up: the lowest becomes
                // ground for whatever stands on it, and the rest try again.
                let lowest = (0..floating.len())
                    .min_by(|&a, &b| floating[a].position[1].total_cmp(&floating[b].position[1]))
                    .expect("not empty");
                let mut base = floating.swap_remove(lowest);
                float_errors.swap_remove(lowest);
                base.base_plate = true;
                match self.plant_one(actor, base, true) {
                    Ok(id) => ids.push(id),
                    Err(error) => refused.push(error),
                }
            }
            waiting = floating;
        }
        if !ids.is_empty() {
            self.detect_collisions();
        }
        (ids, refused)
    }
    /// One brick of a copy planted a slice at a time: every plant rule
    /// but reach against the world as it stands, and, unless `free`,
    /// something must hold it up. Collisions are refreshed by
    /// [`Self::settle`] once the slice is in.
    pub fn plant_try(&mut self, actor: &Actor, brick: Brick, free: bool) -> Result<BrickId> {
        self.plant_one(actor, brick, free)
    }
    /// Whether `brick` passes every plant rule but reach and support for
    /// `actor` now, and whether something already there holds it up: a
    /// group plant's check, a brick at a time.
    pub fn check_plant(&self, actor: &Actor, brick: &Brick) -> Result<bool> {
        check_placement(
            self.authority.state(),
            &self.definitions,
            &self.index,
            self.ground(),
            actor,
            brick,
        )
    }
    /// Put one brick removed earlier back exactly as it was, if it still
    /// fits where it stood ([`Self::restore_group`] a brick at a time).
    /// Collisions are refreshed by [`Self::settle`].
    pub fn restore_one(&mut self, brick: Brick) -> Result<BrickId> {
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        if self.state().bricks.len() >= bri_world::MAX_BRICKS {
            return Err(PlantFailure::Limit.into());
        }
        let definition = self.definitions.get(&brick)?;
        let bounds = Bounds::new(&brick, &definition.mesh)?;
        self.check_plant(&engine, &brick)?;
        let id = self.authority.restore(&engine, brick)?;
        self.attach(id)?;
        let brick = &self.authority.state().bricks[&id];
        self.index.insert(id, bounds);
        if let Some(water) = brick_water(brick, self.definitions.get(brick)?) {
            self.brick_waters.insert(id, water);
            self.liquids = std::sync::OnceLock::new();
        }
        self.note_stack(id);
        Ok(id)
    }
    /// Refresh collisions after bricks went in a slice at a time.
    pub fn settle(&mut self) {
        self.detect_collisions();
    }
    /// Mark the solid bricks' collision round `id` for rebuilding, ahead of
    /// removing it with others in one go (a slice of a copy job).
    pub fn mark_rebuild(&mut self, id: BrickId) {
        if let Some(brick) = self.authority.state().bricks.get(&id) {
            self.chunks.mark(id, brick.position);
        }
    }
    /// Take from `budget` what rebuilding the solid bricks' collision that
    /// changes since the last charge costs: a brick changed in a big build
    /// rebuilds its whole chunk ([`crate::chunks`]) at the next settle, and
    /// that, not the brick, is most of what a copy job's first touch of a
    /// build costs.
    pub fn charge_rebuilds(&mut self, budget: &mut u32) {
        let bricks = u32::try_from(self.chunks.take_rebuilt()).unwrap_or(u32::MAX);
        *budget = budget.saturating_sub(bricks.saturating_mul(work::REBUILD));
    }
    /// While `hold`, removing bricks leaves collisions to one
    /// [`Self::settle`] for the lot (a slice of a copy job breaking bricks
    /// one by one); letting go settles.
    pub fn hold_settle(&mut self, hold: bool) {
        let held = std::mem::replace(&mut self.holding, hold);
        if held && !hold {
            self.settle();
        }
    }
    fn plant_one(&mut self, actor: &Actor, brick: Brick, free: bool) -> Result<BrickId> {
        if self.state().bricks.len() >= bri_world::MAX_BRICKS {
            return Err(PlantFailure::Limit.into());
        }
        let definition = self.definitions.get(&brick)?;
        let bounds = Bounds::new(&brick, &definition.mesh)?;
        let (supported, obstructed) = check_placement_support(
            self.authority.state(),
            &self.definitions,
            &self.index,
            self.ground(),
            actor,
            &brick,
        )?;
        if !supported && !free {
            return Err(if obstructed {
                PlantFailure::Buried
            } else {
                PlantFailure::Float
            }
            .into());
        }
        let id = self.authority.plant(actor, brick, |_, _| Ok(()))?;
        self.attach(id)?;
        let brick = &self.authority.state().bricks[&id];
        self.index.insert(id, bounds);
        if let Some(water) = brick_water(brick, self.definitions.get(brick)?) {
            self.brick_waters.insert(id, water);
            self.liquids = std::sync::OnceLock::new();
        }
        self.note_stack(id);
        Ok(id)
    }
    /// Whether `brick` could go into the world now, support aside: no
    /// overlap with another brick, not buried in the map, not stuck in a
    /// player or vehicle.
    pub fn fits(&self, brick: &Brick) -> bool {
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        check_placement(
            self.authority.state(),
            &self.definitions,
            &self.index,
            self.ground(),
            &engine,
            brick,
        )
        .is_ok()
    }
    /// Put bricks removed earlier back exactly as they were, owner, name,
    /// events, lights and all: undoing a cut. Each must still fit where it
    /// stood (nothing planted there since, nobody standing in it); support
    /// is not asked, since they stood there before. All or none.
    pub fn restore_group(&mut self, bricks: Vec<Brick>) -> Result<Vec<BrickId>> {
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        self.place_group(&engine, bricks, true, false)
    }
    fn place_group(
        &mut self,
        actor: &Actor,
        bricks: Vec<Brick>,
        restore: bool,
        needs_support: bool,
    ) -> Result<Vec<BrickId>> {
        ensure!(!bricks.is_empty(), "Nothing to plant");
        if self.state().bricks.len() + bricks.len() > bri_world::MAX_BRICKS {
            return Err(PlantFailure::Limit.into());
        }
        let mut preflight = GroupSupport::default();
        for brick in &bricks {
            preflight.check(self, actor, brick)?;
        }
        if needs_support {
            // The direct API drains the same preflight that copy jobs advance
            // under their existing shared per-tick work budget.
            let mut budget = SUPPORT_QUERY_LIMIT * work::SEARCH;
            if !preflight.step(self, &mut budget)? {
                return Err(PlantFailure::Limit.into());
            }
            if !preflight.supported {
                return Err(PlantFailure::Float.into());
            }
        }
        let prepared = preflight.bounds;
        let mut ids = Vec::with_capacity(bricks.len());
        for brick in bricks {
            let placed = if restore {
                self.authority.restore(actor, brick)
            } else {
                self.authority.plant(actor, brick, |_, _| Ok(()))
            };
            match placed {
                Ok(id) => ids.push(id),
                Err(error) => {
                    // Storage ran out part way: take back what went in.
                    for id in ids {
                        self.authority.remove(actor, id)?;
                    }
                    return Err(error);
                }
            }
        }
        for (&id, bounds) in ids.iter().zip(prepared) {
            self.attach(id)?;
            self.note_kind(id);
            let brick = &self.authority.state().bricks[&id];
            let definition = self.definitions.get(brick)?;
            self.index.insert(id, bounds);
            if let Some(water) = brick_water(brick, definition) {
                self.brick_waters.insert(id, water);
                self.liquids = std::sync::OnceLock::new();
            }
            self.note_stack(id);
        }
        self.detect_collisions();
        Ok(ids)
    }
    /// Every brick lying wholly inside `area` (or, not `limited`, reaching
    /// into it) that `admit` accepts, lowest first: what a copy of the box
    /// takes, cut short at `limit`. All at once; [`BoxScan`] spreads it
    /// over ticks.
    pub fn select_box(
        &self,
        area: Bounds,
        limited: bool,
        limit: usize,
        admit: impl FnMut(BrickId, &Brick) -> bool,
    ) -> Selection {
        let mut scan = BoxScan::new(self, area, limited, limit);
        let mut all = u32::MAX;
        scan.step(self, &mut all, admit);
        scan.selection
    }
    /// A stack from `start`, as v20's duplicators select one: `start`,
    /// then breadth first every brick joined by studs to one already taken
    /// that `admit` accepts. From `start` itself only one way, up (bricks
    /// on top of it) or down (bricks under it); from every other brick
    /// both ways. `limited` keeps the stack on its side of `start`: going
    /// up, nothing reaching below `start`'s bottom; going down, nothing
    /// reaching above its top. Cut short at `limit`. `start` is taken
    /// whatever `admit` says; the caller checks it. All at once;
    /// [`StackScan`] spreads it over ticks.
    pub fn select_stack(
        &self,
        start: BrickId,
        reach: StackReach,
        limit: usize,
        mut admit: impl FnMut(BrickId, &Brick) -> bool,
    ) -> Result<Selection> {
        let mut scan = StackScan::new(self, start, reach, limit)?;
        let mut all = u32::MAX;
        while !scan.step(self, &mut all, &mut admit)? {}
        Ok(scan.selection)
    }
    /// Whose stack `id` stands in: the owner of the bricks it was built on
    /// (v20's `stackBL_ID`). A brick planted on others' bricks takes the
    /// stack of the lowest-numbered brick under it, else of one on top of
    /// it, else its own owner's; a duplicator's plant and a restored
    /// brick the same way. A loaded build's bricks are their owners'.
    /// v20's trust rules let a stack's owner edit what
    /// others built on it with their trust (the New Duplicator's).
    pub fn stack_owner(&self, id: BrickId) -> Option<bri_world::OwnerId> {
        let brick = self.state().bricks.get(&id)?;
        Some(self.stacks.get(&id).copied().unwrap_or(brick.owner))
    }
    /// Note the stack a brick just put in stands in ([`Self::stack_owner`]).
    fn note_stack(&mut self, id: BrickId) {
        let Some(bounds) = self.index.get(id) else {
            return;
        };
        let (bottom, top) = (bounds.min[1], bounds.max()[1]);
        let (mut down, mut up) = (None::<BrickId>, None::<BrickId>);
        self.index.visit(bounds.expanded(1), |other, found| {
            if other == id || !grid::share_face(bounds, found) {
                return;
            }
            if found.max()[1] == bottom {
                down = Some(down.map_or(other, |d| d.min(other)));
            } else if found.min[1] == top {
                up = Some(up.map_or(other, |u| u.min(other)));
            }
        });
        let owner = self.state().bricks[&id].owner;
        let stack = down
            .or(up)
            .and_then(|other| self.stack_owner(other))
            .unwrap_or(owner);
        if stack != owner {
            self.stacks.insert(id, stack);
        }
    }
    /// The bricks sharing a face with `id` (`grid::share_face`): beside,
    /// on top of or under it, joined by studs or not. Ascending ids.
    pub fn touching_bricks(&self, id: BrickId) -> Vec<BrickId> {
        let Some(bounds) = self.index.get(id) else {
            return Vec::new();
        };
        let mut out = BTreeSet::new();
        self.index.visit(bounds.expanded(1), |other, found| {
            if other != id && grid::share_face(bounds, found) {
                out.insert(other);
            }
        });
        out.into_iter().collect()
    }
    /// What a fill spreads over from `start`: it and every brick joined
    /// to it, passing only through bricks `admit` accepts (`start` is not
    /// asked), at most `limit` of them in the order the fill reaches them
    /// (breadth first, neighbours by id), so the same world always gives
    /// the same fill. Bricks join through shared faces
    /// ([`Self::touching_bricks`]), or with `reach` (sideways, vertical)
    /// when one's box overlaps the other's grown by that much, as v20's
    /// `containerBoxSearch` around each brick found them. The flag says
    /// whether more bricks were waiting when `limit` was reached.
    pub fn fill_region(
        &self,
        start: BrickId,
        limit: usize,
        reach: Option<[f32; 2]>,
        mut admit: impl FnMut(BrickId, &Brick) -> bool,
    ) -> Result<(Vec<BrickId>, bool)> {
        let world = self.state();
        ensure!(world.bricks.contains_key(&start), "Unknown brick");
        ensure!(limit > 0, "A fill paints at least one brick");
        let mut seen = BTreeSet::from([start]);
        let mut order = vec![start];
        let mut next = 0;
        while let Some(&id) = order.get(next) {
            next += 1;
            let mut near = match reach {
                None => self.touching_bricks(id),
                Some([side, up]) => {
                    let Some((min, max)) = self.brick_box(id) else {
                        continue;
                    };
                    let grow = Vec3::new(side, up, side);
                    self.bricks_in_box(min - grow, max + grow)
                }
            };
            near.sort_unstable();
            for other in near {
                if !seen.insert(other) || !admit(other, &world.bricks[&other]) {
                    continue;
                }
                if order.len() >= limit {
                    return Ok((order, true));
                }
                order.push(other);
            }
        }
        Ok((order, false))
    }
    pub fn edit(&mut self, actor: &Actor, id: BrickId, edit: Edit) -> Result<()> {
        self.authority.edit(actor, id, edit)?;
        self.note_link(id);
        let _ = self.sync_flags(id);
        self.detect_collisions();
        Ok(())
    }
    /// `killBrick`. v20's `indestructable` (spawn points, vehicle spawns)
    /// only keeps explosions and chain kills off a brick; those callers skip
    /// it themselves. The hammer, wands and undo break it like any other.
    pub fn remove(&mut self, actor: &Actor, id: BrickId) -> Result<()> {
        self.remove_many(actor, &[id])
    }
    /// Remove every brick in `ids`, refreshing collisions once at the end:
    /// a refresh per brick made clearing a big build take minutes. Missing
    /// bricks are refused before any is removed.
    /// Their colliders leave through `parking`, which spares a small
    /// removal the whole broad phase's refit.
    pub fn remove_many(&mut self, actor: &Actor, ids: &[BrickId]) -> Result<()> {
        for &id in ids {
            self.state().bricks.get(&id).context("Unknown brick")?;
        }
        self.state()
            .revision
            .checked_add(ids.len() as u64)
            .context("Revision exhausted")?;
        let mut handles = Vec::with_capacity(ids.len());
        for &id in ids {
            // Collision first, while the brick still says where it stood.
            if let Some(handle) = self.detach(id) {
                handles.push(handle);
            }
            self.forget_kind(id);
            self.authority.remove(actor, id)?;
            self.index.remove(id);
            self.stacks.remove(&id);
            if self.brick_waters.remove(&id).is_some() {
                self.liquids = std::sync::OnceLock::new();
            }
        }
        self.parked.remove(&mut self.physics, &handles);
        if !self.holding {
            self.detect_collisions();
        }
        Ok(())
    }
    /// A brick that starts or stops colliding moves between its chunk and a
    /// sensor collider of its own. Bodies resting on a brick that stops
    /// colliding fall through (`detach` wakes them). Returns whether the
    /// brick's collision changed.
    fn sync_flags(&mut self, id: BrickId) -> bool {
        let brick = &self.authority.state().bricks[&id];
        let Ok(definition) = self.definitions.get(brick) else {
            return false;
        };
        let solid = solid(brick, definition);
        let in_chunk = !self.handles.contains_key(&id);
        if solid == in_chunk {
            return false;
        }
        if let Some(handle) = self.detach(id) {
            self.parked.remove(&mut self.physics, &[handle]);
        }
        let _ = self.attach(id);
        true
    }
    /// Trusted server change from the event engine or game rules. Only a
    /// change to the brick's collision refreshes collisions: paint, names
    /// and event rows cost no chunk rebuild or physics pass.
    pub fn mutate(&mut self, id: BrickId, change: impl FnOnce(&mut Brick)) -> Result<()> {
        let before = self.state().bricks.get(&id).map(support_identity);
        self.authority.mutate(id, change)?;
        if self.state().bricks.get(&id).map(support_identity) != before {
            self.support_epoch = self.support_epoch.wrapping_add(1);
        }
        self.note_link(id);
        if self.sync_flags(id) {
            self.detect_collisions();
        }
        Ok(())
    }
    /// `mutate` for many bricks, refreshing collisions once at the end: each
    /// chunk they share is rebuilt once, not once per brick.
    pub fn mutate_many(
        &mut self,
        ids: &[BrickId],
        mut change: impl FnMut(&mut Brick),
    ) -> Result<()> {
        let mut changed = false;
        for &id in ids {
            let before = self.state().bricks.get(&id).map(support_identity);
            self.authority.mutate(id, &mut change)?;
            if self.state().bricks.get(&id).map(support_identity) != before {
                self.support_epoch = self.support_epoch.wrapping_add(1);
            }
            self.note_link(id);
            changed |= self.sync_flags(id);
        }
        if changed {
            self.detect_collisions();
        }
        Ok(())
    }
    /// [`Self::mutate_many`] with each brick's own change: every brick in
    /// `bricks` takes the given state (it keeps its place, shape and
    /// owner), collisions refreshed once at the end.
    pub fn replace_many(&mut self, bricks: Vec<(BrickId, Brick)>) -> Result<()> {
        let mut changed = false;
        for (id, next) in bricks {
            self.authority.mutate(id, |b| {
                let (definition, position, turns, owner) =
                    (b.definition.clone(), b.position, b.quarter_turns, b.owner);
                *b = next;
                (b.definition, b.position, b.quarter_turns, b.owner) =
                    (definition, position, turns, owner);
            })?;
            self.note_link(id);
            changed |= self.sync_flags(id);
        }
        if changed {
            self.detect_collisions();
        }
        Ok(())
    }
    /// The grid cells brick `id` fills. Panics on an unknown brick.
    pub fn index_bounds(&self, id: BrickId) -> Bounds {
        self.index.bounds(id)
    }
    /// Continue an earlier world's clock (the host changed maps).
    pub fn set_tick(&mut self, tick: u64) {
        self.authority.set_tick(tick);
    }
    pub fn step(&mut self) -> Result<()> {
        self.authority.step()?;
        self.flush_chunks();
        self.physics.step();
        self.stream_terrain();
        Ok(())
    }
    /// The brick an activation (click) ray reaches. Eye and direction are
    /// from the server's player state, not packet positions.
    pub fn activate(&self, eye: Vec3, direction: Vec3) -> Result<Option<BrickId>> {
        Ok(self
            .target_through(eye, direction, 5.0)?
            .and_then(|(hit, _)| hit.brick))
    }
    /// World-space box of a brick's logical grid volume.
    pub fn brick_box(&self, id: BrickId) -> Option<(Vec3, Vec3)> {
        let brick = self.state().bricks.get(&id)?;
        let mesh = &self.definitions.get(brick).ok()?.mesh;
        Some(crate::definitions::brick_box(brick, mesh))
    }
    /// v20 `fxDTSBrick::willCauseChainKill`: whether killing this brick would
    /// leave any other brick without a path to the ground. The hammer refuses
    /// such bricks; the wands and undo break them anyway.
    pub fn will_cause_chain_kill(&self, id: BrickId) -> Result<bool> {
        Ok(!self.stranded_by(id)?.is_empty())
    }
    /// The bricks `killBrick` chain-kills with this one: those left without
    /// a path to the ground once it is gone, nearest first. Bricks connect
    /// by studs, up or down, and grounded bricks hold up everything joined
    /// to them.
    pub fn stranded_by(&self, id: BrickId) -> Result<Vec<BrickId>> {
        ensure!(self.state().bricks.contains_key(&id), "Unknown brick");
        let (mut supported, mut stranded) = (BTreeSet::new(), Vec::new());
        for start in self.connected_bricks(id)? {
            if supported.contains(&start) || stranded.contains(&start) {
                continue;
            }
            let mut seen = BTreeSet::from([id, start]);
            let mut order = vec![start];
            let mut next = 0;
            let mut grounded = false;
            while let Some(&brick) = order.get(next) {
                next += 1;
                if supported.contains(&brick) || self.grounded_root(brick)? {
                    grounded = true;
                    break;
                }
                for other in self.connected_bricks(brick)? {
                    if seen.insert(other) {
                        order.push(other);
                    }
                }
            }
            if grounded {
                seen.remove(&id);
                supported.extend(seen);
            } else {
                stranded.extend(order);
            }
        }
        Ok(stranded)
    }
    /// Bricks joined to this one by studs above or below (`getUpBrick`,
    /// `getDownBrick`).
    pub fn connected_bricks(&self, id: BrickId) -> Result<Vec<BrickId>> {
        let world = self.state();
        let brick = &world.bricks[&id];
        let mesh = &self.definitions.get(brick)?.mesh;
        let bounds = self.index.bounds(id);
        let mut out = Vec::new();
        for other in self.index.query(bounds.expanded(1)) {
            if other == id {
                continue;
            }
            let existing = &world.bricks[&other];
            let other_mesh = &self.definitions.get(existing)?.mesh;
            if grid::connected(
                (brick, mesh, bounds),
                (existing, other_mesh, self.index.bounds(other)),
            ) {
                out.push(other);
            }
        }
        Ok(out)
    }
    /// The chain-kill root test: a brick resting on the map is ground, and
    /// so is a baseplate (v20's `isBaseplate`, which force plant sets).
    fn grounded_root(&self, id: BrickId) -> Result<bool> {
        if self.state().bricks.get(&id).is_some_and(|b| b.base_plate) {
            return Ok(true);
        }
        Ok(on_ground(
            &self.physics,
            self.terrain.as_ref(),
            self.index.bounds(id),
        ))
    }
    /// The liquid covering the most of a box standing on `feet`, as
    /// `bri_content::water::submersion` picks it, without copying the list.
    pub fn liquid_at(
        &self,
        feet: [f32; 3],
        height: f32,
    ) -> Option<(&bri_content::water::Water, f32)> {
        self.waters
            .iter()
            .chain(self.brick_waters.values())
            .map(|w| (w, w.coverage(feet, height)))
            .filter(|(_, coverage)| *coverage > 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }
    /// Map liquids plus water bricks, for the player motor. Built once and
    /// shared until a water brick or the map's liquids change, not copied
    /// every tick.
    pub fn liquids(&self) -> Liquids {
        let build = || -> Liquids {
            self.waters
                .iter()
                .chain(self.brick_waters.values())
                .cloned()
                .collect()
        };
        let source = (self.waters.as_ptr() as usize, self.waters.len());
        let (address, count, liquids) = self.liquids.get_or_init(|| (source.0, source.1, build()));
        if (*address, *count) == source {
            liquids.clone()
        } else {
            // `waters` was replaced after the first build: build afresh.
            build()
        }
    }
    /// Swap a brick's definition in place, replacing its grid bounds and
    /// collision as well as its look (including wider open doors).
    pub fn set_definition(&mut self, id: BrickId, definition: &str) -> Result<()> {
        let brick = self.state().bricks.get(&id).context("Unknown brick")?;
        let new = self
            .definitions
            .entries
            .get(definition)
            .context("Unknown brick definition")?;
        let bounds = Bounds::new(brick, &new.mesh)?;
        let definition = definition.to_string();
        if let Some(handle) = self.detach(id) {
            self.parked.remove(&mut self.physics, &[handle]);
        }
        self.forget_kind(id);
        self.authority.mutate(id, |b| {
            b.definition = bri_world::ContentRef::Resolved(definition)
        })?;
        self.index.insert(id, bounds);
        self.note_kind(id);
        self.attach(id)?;
        self.detect_collisions();
        Ok(())
    }
    /// Every brick of definition `definition` (`v20/brick/...` or a
    /// package's brick id), lowest id first.
    pub fn bricks_of(&self, definition: &str) -> impl Iterator<Item = BrickId> + '_ {
        self.kinds.get(definition).into_iter().flatten().copied()
    }
    fn note_kind(&mut self, id: BrickId) {
        if let Some(key) = self
            .authority
            .state()
            .bricks
            .get(&id)
            .and_then(definition_key)
        {
            self.kinds.entry(key.to_string()).or_default().insert(id);
        }
    }
    fn forget_kind(&mut self, id: BrickId) {
        let Some(key) = self
            .authority
            .state()
            .bricks
            .get(&id)
            .and_then(definition_key)
        else {
            return;
        };
        if let Some(set) = self.kinds.get_mut(key) {
            set.remove(&id);
            if set.is_empty() {
                let key = key.to_string();
                self.kinds.remove(&key);
            }
        }
    }
    /// Bricks whose grid volume overlaps a world-space box.
    pub fn bricks_in_box(&self, min: Vec3, max: Vec3) -> Vec<BrickId> {
        let lo: [i32; 3] = std::array::from_fn(|a| (min[a] / grid::CELL[a]).floor() as i32 - 1);
        let hi: [i32; 3] = std::array::from_fn(|a| (max[a] / grid::CELL[a]).ceil() as i32 + 1);
        let bounds = Bounds {
            min: lo,
            size: std::array::from_fn(|a| (hi[a] - lo[a]).max(1)),
        };
        self.index
            .query(bounds)
            .into_iter()
            .filter(|id| {
                self.brick_box(*id)
                    .is_some_and(|(bmin, bmax)| bmin.cmplt(max).all() && bmax.cmpgt(min).all())
            })
            .collect()
    }
    /// Longest brick-targeting ray: the admin Destructo Wand's 500 units at
    /// up to four times player scale.
    pub const MAX_TARGET_DISTANCE: f32 = 2000.0;
    pub fn target(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Result<Option<Hit>> {
        self.target_filtered(origin, direction, max_distance, |brick| brick.raycast)
    }
    /// [`Self::target`] on through the openings of linked bricks
    /// (portals), as the player sees: the hit, its distance along the whole
    /// sight, and the leg it lies on (how the sight arrived there).
    pub fn target_through(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Result<Option<(Hit, bri_content::passage::Leg)>> {
        ensure!(
            direction.is_finite() && direction.length_squared() > 0.1,
            "Invalid targeting ray"
        );
        self.passages()
            .cast(origin, direction.normalize(), max_distance, |leg| {
                if leg.length <= 0.0 {
                    return Ok(None);
                }
                let hit = self.target(leg.from, leg.direction, leg.length)?;
                Ok(hit.map(|hit| Hit {
                    distance: leg.start + hit.distance,
                    ..hit
                }))
            })
    }
    /// The shortest way `from` sees `to` by, no longer than `reach`:
    /// straight across, or in through one opening of a linked brick and
    /// out of its partner ([`bri_content::passage::Passages::ways`]), with
    /// no solid surface on any leg (within half a unit of `to`, which
    /// may stand in a body). Water bricks are liquid volumes, not walls;
    /// they remain selectable by editing rays. What a hand can reach
    /// through; [`Self::eyes_see`] is what an eye sees through.
    pub fn sight(&self, from: Vec3, to: Vec3, reach: f32) -> Option<bri_content::passage::Way> {
        self.way(from, to, reach, false)
    }
    /// [`Self::sight`] for an eye: a brick painted under full alpha is
    /// looked through, as the client draws it see-through (its opaque
    /// test, `brick_cover`, is alpha at 1). Shots and hands still stop on
    /// it.
    pub fn eyes_see(&self, from: Vec3, to: Vec3, reach: f32) -> Option<bri_content::passage::Way> {
        self.way(from, to, reach, true)
    }
    fn way(
        &self,
        from: Vec3,
        to: Vec3,
        reach: f32,
        see_through_paint: bool,
    ) -> Option<bri_content::passage::Way> {
        let palette = &self.state().palette;
        let opaque = |brick: &Brick| {
            !see_through_paint
                || palette
                    .get(usize::from(brick.color))
                    .is_none_or(|rgba| rgba[3] >= 1.0)
        };
        // Nothing within `slack` of a leg's end counts.
        let clear = |origin: Vec3, direction: Vec3, length: f32, slack: f32| {
            length <= 1e-3
                || self
                    .target_filtered(
                        origin,
                        direction,
                        length.min(Self::MAX_TARGET_DISTANCE),
                        |brick| {
                            brick.raycast
                                && opaque(brick)
                                && self.definitions.get(brick).is_ok_and(|definition| {
                                    definition.special != crate::definitions::Special::Water
                                })
                        },
                    )
                    .ok()
                    .flatten()
                    .is_none_or(|hit| hit.distance > length - slack)
        };
        let passages = self.passages();
        let across = from.distance(to);
        if passages.list.is_empty() {
            // No portals: straight across or not at all, without a search.
            let seen =
                across > 0.1 && across <= reach && clear(from, (to - from) / across, across, 0.5);
            return seen.then_some(bri_content::passage::Way {
                aim: to,
                carry: None,
                length: across,
            });
        }
        let mut ways: Vec<_> = passages
            .ways(from, to, reach)
            .filter(|w| w.carry.is_some() || across > 0.1)
            .collect();
        ways.sort_by(|a, b| a.length.total_cmp(&b.length));
        ways.into_iter().find(|way| {
            let legs = passages.sight(from, (way.aim - from) / way.length, way.length);
            let last = legs.len() - 1;
            legs.iter().enumerate().all(|(i, leg)| {
                // A leg up to an opening must reach it.
                let slack = if i == last { 0.5 } else { 0.01 };
                clear(leg.from, leg.direction, leg.length, slack)
            })
        })
    }
    /// Stock editing tools use FxBrickAlwaysObjectType, including bricks whose
    /// ordinary raycasting flag is disabled. Map geometry still obstructs tools.
    pub fn target_bricks_always(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Result<Option<Hit>> {
        self.target_filtered(origin, direction, max_distance, |_| true)
    }
    fn target_filtered(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        accept: impl Fn(&Brick) -> bool,
    ) -> Result<Option<Hit>> {
        ensure!(
            origin.is_finite()
                && origin.abs().max_element() <= 1_000_000.0
                && direction.is_finite()
                && direction.length_squared().is_finite()
                && direction.length_squared() > 0.1
                && max_distance.is_finite()
                && max_distance > 0.0
                && max_distance <= Self::MAX_TARGET_DISTANCE,
            "Invalid targeting ray"
        );
        let direction = direction.normalize();
        let ray = Ray::new(
            Vector::from_array(origin.to_array()),
            Vector::from_array(direction.to_array()),
        );
        let filter = |_: ColliderHandle, c: &Collider| c.user_data == MAP_TAG;
        let mut nearest = self
            .physics
            .query_pipeline_with_filter(QueryFilter::default().predicate(&filter))
            .cast_ray_and_get_normal(&ray, max_distance, true)
            .map(|(_, hit)| Hit {
                brick: None,
                position: origin + direction * hit.time_of_impact,
                normal: hit_normal(Vec3::from(hit.normal.to_array()), direction),
                distance: hit.time_of_impact,
            });
        // Terrain answers exactly, loaded tile or not, as it does for weapons.
        if let Some((distance, normal)) = self.terrain_ray(origin, direction, max_distance)
            && nearest.as_ref().is_none_or(|hit| distance < hit.distance)
        {
            nearest = Some(Hit {
                brick: None,
                position: origin + direction * distance,
                normal: hit_normal(normal, direction),
                distance,
            });
        }
        self.walk_bricks(origin, direction, max_distance, &mut nearest, |_, brick| {
            accept(brick)
        })?;
        Ok(nearest)
    }
    /// The nearest brick `accept` takes along a ray (normalized direction),
    /// by each brick's own collision: what projectiles and sight lines hit.
    /// Bricks share chunk colliders, so rays that must tell bricks apart or
    /// skip some come here rather than to the physics world.
    pub fn brick_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        accept: impl Fn(BrickId, &Brick) -> bool,
    ) -> Result<Option<Hit>> {
        let mut nearest = None;
        self.walk_bricks(origin, direction, max_distance, &mut nearest, accept)?;
        Ok(nearest)
    }
    /// The solid brick whose collision lies nearest `point` (within a
    /// little), for a hit on a chunk collider that did not say which part.
    pub fn brick_near(&self, point: Vec3) -> Option<BrickId> {
        let reach = Vec3::splat(0.05);
        let at = Vector::from_array(point.to_array());
        self.bricks_in_box(point - reach, point + reach)
            .into_iter()
            .filter_map(|id| {
                let brick = self.state().bricks.get(&id)?;
                let definition = self.definitions.get(brick).ok()?;
                solid(brick, definition).then(|| {
                    let distance = definition.shape.distance_to_point(&pose(brick), at, true);
                    (distance, id)
                })
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id)
    }
    /// Whether a brick's collision contains `point`.
    pub fn brick_contains(&self, id: BrickId, point: Vec3) -> bool {
        self.state().bricks.get(&id).is_some_and(|brick| {
            self.definitions.get(brick).is_ok_and(|definition| {
                definition
                    .shape
                    .contains_point(&pose(brick), Vector::from_array(point.to_array()))
            })
        })
    }
    /// Walk the index buckets along the ray, nearest first, and stop once
    /// the nearest hit lies before the bucket just searched. A brick's
    /// collision stays within its grid bounds, so no later bucket can hold
    /// a closer hit. Long sight lines through large builds stay cheap.
    fn walk_bricks(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        nearest: &mut Option<Hit>,
        accept: impl Fn(BrickId, &Brick) -> bool,
    ) -> Result<()> {
        // This set is only a per-ray duplicate guard for bricks registered in
        // more than one spatial bucket. A fast non-cryptographic hasher keeps
        // sight queries from paying for randomized SipHash on every candidate.
        let mut tested = rustc_hash::FxHashSet::default();
        for (bucket, exit) in
            grid::ray_buckets(origin.to_array(), direction.to_array(), max_distance)
        {
            for id in self.index.bucket(bucket) {
                if !tested.insert(id) {
                    continue;
                }
                let brick = &self.state().bricks[&id];
                if !accept(id, brick) {
                    continue;
                }
                self.ray_brick(id, brick, origin, direction, max_distance, nearest)?;
            }
            if nearest
                .as_ref()
                .is_some_and(|hit| hit.distance <= exit - 0.02)
            {
                break;
            }
        }
        Ok(())
    }
    fn ray_brick(
        &self,
        id: BrickId,
        brick: &Brick,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        nearest: &mut Option<Hit>,
    ) -> Result<()> {
        // The caller already fetched this brick for the acceptance check;
        // keep it instead of repeating the persistent world-map lookup.
        // Cheap slab test against the padded grid bounds first.
        let bounds = self.index.bounds(id);
        let low = Vec3::from_array(std::array::from_fn(|a| {
            bounds.min[a] as f32 * grid::CELL[a]
        })) - 0.01;
        let high = Vec3::from_array(std::array::from_fn(|a| {
            bounds.max()[a] as f32 * grid::CELL[a]
        })) + 0.01;
        let (mut enter, mut leave) = (0.0f32, max_distance);
        for a in 0..3 {
            if direction[a] == 0.0 {
                if origin[a] < low[a] || origin[a] > high[a] {
                    return Ok(());
                }
            } else {
                let (t0, t1) = (
                    (low[a] - origin[a]) / direction[a],
                    (high[a] - origin[a]) / direction[a],
                );
                enter = enter.max(t0.min(t1));
                leave = leave.min(t0.max(t1));
            }
        }
        if enter > leave || nearest.as_ref().is_some_and(|hit| enter > hit.distance) {
            return Ok(());
        }
        let d = self.definitions.get(brick)?;
        let inverse = brick.transform().inverse();
        let o = inverse.transform_point3(origin);
        let dir = inverse.transform_vector3(direction);
        if let Some((distance, normal)) = bri_physics::content::raycast(
            &d.collision,
            Vector::from_array(o.to_array()),
            Vector::from_array(dir.to_array()),
            max_distance,
        ) && nearest.as_ref().is_none_or(|hit| distance < hit.distance)
        {
            *nearest = Some(Hit {
                brick: Some(id),
                position: origin + direction * distance,
                normal: hit_normal(
                    brick
                        .transform()
                        .transform_vector3(Vec3::from(normal.to_array())),
                    direction,
                ),
                distance,
            });
        }
        Ok(())
    }
}
/// v20 `plant()` error 1: the brick shares a build-grid cell with a brick
/// already in the world. Planting and loading a save use this one rule.
fn overlaps_world(
    world: &World,
    defs: &Definitions,
    index: &Index,
    brick: &Brick,
    mesh: &bri_content::brick::Brick,
    bounds: Bounds,
) -> Result<bool> {
    let mut error = None;
    let overlap = index.any(bounds, |id| {
        let existing = &world.bricks[&id];
        match defs.get(existing) {
            Ok(definition) => grid::overlaps(
                (brick, mesh, bounds),
                (existing, &definition.mesh, index.bounds(id)),
            ),
            Err(e) => {
                error = Some(e);
                true
            }
        }
    });
    match error {
        Some(error) => Err(error),
        None => Ok(overlap),
    }
}
/// What a plant is judged against besides the bricks: the collision world,
/// the map's streamed terrain, and which map colliders are static shapes.
#[derive(Clone, Copy)]
struct Ground<'a> {
    physics: &'a PhysicsWorld,
    terrain: Option<&'a crate::map::TerrainStream>,
    statics: &'a rustc_hash::FxHashSet<ColliderHandle>,
}
impl Ground<'_> {
    fn is_terrain(&self, handle: ColliderHandle) -> bool {
        self.terrain.is_some_and(|t| t.is_terrain_collider(handle))
    }
    /// Map collision a plant asks about: v20's `fxDTSBrick::plant` tests
    /// interiors (`InteriorObjectType`) and terrain, never static shapes
    /// such as trees and props. Terrain is judged by [`buried_bounded`].
    fn interior(&self, handle: ColliderHandle, collider: &Collider) -> bool {
        collider.user_data == MAP_TAG && !self.is_terrain(handle) && !self.statics.contains(&handle)
    }
}
fn validate_placement(
    world: &World,
    defs: &Definitions,
    index: &Index,
    ground: Ground<'_>,
    passages: &bri_content::passage::Passages,
    builder: &Builder<'_>,
    brick: &Brick,
) -> Result<()> {
    ensure!(
        builder.position.is_finite()
            && builder.reach.is_finite()
            && (0.0..=100.0).contains(&builder.reach),
        "Invalid builder position/reach"
    );
    let definition = defs.get(brick)?;
    let radius = *definition.mesh.footprint_studs.iter().max().unwrap() as f32 * 0.25;
    // Measured along the shortest way, through a portal when that is nearer.
    let (distance, _) = passages.shortest(builder.position, Vec3::from(brick.position));
    if distance > builder.reach + radius {
        return Err(PlantFailure::TooFar.into());
    }
    let (supported, obstructed) =
        check_placement_support(world, defs, index, ground, builder.actor, brick)?;
    if !supported {
        return Err(if obstructed {
            PlantFailure::Buried
        } else {
            PlantFailure::Float
        }
        .into());
    }
    Ok(())
}
/// Every plant rule but reach and support: no overlap, no building onto a
/// brick the actor may not build on, not buried in the map or stuck in a
/// vehicle. Returns whether something already there holds the brick up (a
/// brick it connects to, or the map).
fn check_placement(
    world: &World,
    defs: &Definitions,
    index: &Index,
    ground: Ground<'_>,
    actor: &Actor,
    brick: &Brick,
) -> Result<bool> {
    check_placement_support(world, defs, index, ground, actor, brick)
        .map(|(supported, _)| supported)
}
fn check_placement_support(
    world: &World,
    defs: &Definitions,
    index: &Index,
    ground: Ground<'_>,
    actor: &Actor,
    brick: &Brick,
) -> Result<(bool, bool)> {
    let Ground {
        physics, terrain, ..
    } = ground;
    let definition = defs.get(brick)?;
    let bounds = Bounds::new(brick, &definition.mesh)?;
    if overlaps_world(world, defs, index, brick, &definition.mesh, bounds)? {
        return Err(PlantFailure::Overlap.into());
    }
    // A brick cannot provide support through an authored map surface. Test the
    // exact matching stud cells rather than inferring a filled volume from an
    // arbitrary (possibly open or disconnected) triangle mesh.
    let interior = |handle, c: &Collider| ground.interior(handle, c);
    let interiors = physics.query_pipeline_with_filter(QueryFilter::default().predicate(&interior));
    let mut supported = false;
    let mut obstructed_support = false;
    let mut work_left = SUPPORT_QUERY_LIMIT;
    let query = bounds.expanded(1);
    // The allowance is for the bricks touching this one and their stud
    // cells. Passing over a bucket's other bricks is one box test each, and
    // a dense build packs hundreds of them into one bucket: charging them
    // refused ordinary plants in a 20k-brick city as a brick limit. The
    // buckets themselves stay bounded, as for a group (`GroupSupport`).
    let (min, max) = grid::bucket_span(query);
    let buckets = (0..3).fold(1u64, |n, a| {
        n.saturating_mul((i64::from(max[a]) - i64::from(min[a]) + 1) as u64)
    });
    if buckets > SUPPORT_QUERY_LIMIT as u64 {
        return Err(PlantFailure::Limit.into());
    }
    let mut candidates = grid::QueryCursor::new(query);
    loop {
        let Some(candidate) = candidates.step(index) else {
            break;
        };
        let Some(id) = candidate else {
            continue;
        };
        if work_left == 0 {
            return Err(PlantFailure::Limit.into());
        }
        work_left -= 1;
        let existing = &world.bricks[&id];
        let other = defs.get(existing)?;
        let mut cells = grid::ConnectionCells::new(bounds, index.bounds(id));
        let mut attached = false;
        let mut clear = false;
        loop {
            if work_left == 0 {
                return Err(PlantFailure::Limit.into());
            }
            work_left -= 1;
            let Some((cell, neighbor, matches)) = cells.next(
                brick.quarter_turns,
                &definition.mesh,
                existing.quarter_turns,
                &other.mesh,
            ) else {
                break;
            };
            if !matches {
                continue;
            }
            attached = true;
            if !may_build_on(actor, existing) {
                return Err(PlantFailure::Forbidden.into());
            }
            if map_connector_clear(&interiors, cell, neighbor) {
                clear = true;
                break;
            }
        }
        supported |= clear;
        obstructed_support |= attached && !clear;
    }
    if buried_in_interiors(&interiors, bounds) {
        return Err(PlantFailure::Buried.into());
    }
    // Players and bots never refuse a brick: v20's plant does not ask about
    // them, so a brick may go where one stands. Vehicles and other moving
    // bodies still do, so a plant cannot fling them out of a brick.
    let placement = pose(brick);
    let aabb = definition.shape.compute_aabb(&placement);
    let mut contact_work = SUPPORT_QUERY_LIMIT;
    for (_, obstacle) in physics.query_pipeline().intersect_aabb_conservative(aabb) {
        probe_spend(&mut contact_work)?;
        let moving = !obstacle.is_sensor()
            && !is_character(obstacle.user_data)
            && obstacle
                .parent()
                .is_some_and(|p| !physics.bodies[p].is_fixed());
        if !moving {
            continue;
        }
        if let Some(contact) = rapier3d::parry::query::contact(
            &placement,
            definition.shape.as_ref(),
            obstacle.position(),
            obstacle.shape(),
            0.0,
        )
        .map_err(|_| anyhow::anyhow!("Unsupported obstacle/brick collision pair"))?
            && contact.dist < -0.002
        {
            return Err(PlantFailure::Stuck.into());
        }
    }
    if let Some(t) = terrain {
        let mut terrain_work = footprint_probe_budget(bounds, 1)?;
        if buried_bounded(t, bounds, &mut terrain_work)? {
            return Err(PlantFailure::Buried.into());
        }
    }
    // Preserve the normal floor-dip/terrain root rule even when a different
    // neighboring connection is obstructed. Any clear stud connection suffices.
    let grounded = if supported {
        false
    } else {
        let mut floor_work = footprint_probe_budget(bounds, if terrain.is_some() { 2 } else { 1 })?;
        ground_probe(physics, terrain, bounds, Some(&mut floor_work))?
    };
    Ok((supported || grounded, obstructed_support))
}
/// A player's or a bot's body: [`bri_motor::player::Player::spawn`]'s tag,
/// or a package entity's.
fn is_character(tag: u128) -> bool {
    let kind = tag >> 64;
    kind == 1 || kind == crate::session::ENTITY_TAG >> 64
}
/// v20's map rule in `fxDTSBrick::plant` (0x53ec40), on the brick's grid
/// box: an interior refuses it only where it crosses the brick's two centre
/// lines (along x and along z at mid-height), or where, under one of a few
/// footprint samples, the first interior surface below the top stands more
/// than [`FLOOR_DIP`] above the bottom. So a brick may clip a corner of a
/// wall or a ceiling, or dip into a floor, as in v20. Merely touching is not
/// crossing: the lines stop just short of the faces, and run just above
/// mid-height, where a plate sunk the whole [`FLOOR_DIP`] meets the floor.
fn buried_in_interiors(interiors: &QueryPipeline<'_>, bounds: Bounds) -> bool {
    const TOUCH: f32 = 0.002;
    let min = Vec3::from_array(std::array::from_fn(|a| {
        bounds.min[a] as f32 * grid::CELL[a]
    }));
    let size = Vec3::from_array(std::array::from_fn(|a| {
        bounds.size[a] as f32 * grid::CELL[a]
    }));
    let center = min + size * 0.5 + Vec3::Y * TOUCH;
    let hits = |origin: Vec3, direction: Vec3, length: f32| {
        length > 0.0
            && interiors
                .cast_ray(
                    &Ray::new(
                        Vector::from_array(origin.to_array()),
                        Vector::from_array(direction.to_array()),
                    ),
                    length,
                    true,
                )
                .is_some()
    };
    for axis in [Vec3::X, Vec3::Z] {
        let half = size.dot(axis) * 0.5 - TOUCH;
        if hits(center + axis * half, -axis, 2.0 * half) {
            return true;
        }
    }
    // floor(width in units) samples a side, 1 to 4, at the centres of an
    // even split of the footprint.
    let samples = |width: f32| (width.floor() as usize).clamp(1, 4);
    let (nx, nz) = (samples(size.x), samples(size.z));
    let top = min.y + size.y - TOUCH;
    let reach = top - (min.y - FLOOR_DIP);
    for i in 0..nx {
        for k in 0..nz {
            let x = min.x + (i as f32 + 0.5) * size.x / nx as f32;
            let z = min.z + (k as f32 + 0.5) * size.z / nz as f32;
            let surface = interiors
                .cast_ray(&Ray::new(Vector::new(x, top, z), -Vector::Y), reach, true)
                .map(|(_, toi)| top - toi);
            if surface.is_some_and(|y| y > min.y + FLOOR_DIP + TOUCH) {
                return true;
            }
        }
    }
    false
}
/// A stud pair is a local physical connection, not a volume classification.
fn map_connector_clear(query: &QueryPipeline<'_>, cell: [i32; 3], neighbor: [i32; 3]) -> bool {
    let center = |p: [i32; 3]| {
        Vector::from_array(std::array::from_fn(|axis| {
            (p[axis] as f32 + 0.5) * grid::CELL[axis]
        }))
    };
    let origin = center(cell);
    let delta = center(neighbor) - origin;
    let reach = delta.length();
    query
        .cast_ray_and_get_normal(&Ray::new(origin, delta / reach), reach, true)
        .is_none_or(|(_, hit)| {
            // A root plate may dip into an upward map floor by FLOOR_DIP;
            // its cell centre can lie on that floor. Joining it from above
            // remains valid. Joining through the floor from below does not.
            delta.y < 0.0 && hit.normal.y > 0.7 && hit.time_of_impact >= reach - 0.002
        })
}
fn footprint_probe_budget(bounds: Bounds, probes_per_cell: u32) -> Result<u32> {
    let cells = (bounds.size[0] as u64).saturating_mul(bounds.size[2] as u64);
    if cells > u64::from(FOOTPRINT_CELL_LIMIT) {
        return Err(PlantFailure::Limit.into());
    }
    Ok(cells as u32 * probes_per_cell)
}
fn probe_spend(left: &mut u32) -> Result<()> {
    if *left == 0 {
        return Err(PlantFailure::Limit.into());
    }
    *left -= 1;
    Ok(())
}
/// A brick is buried when the terrain surface stands above its top over its
/// whole footprint: nothing of it would show. Partly sunk bricks plant, as
/// v20's own terrain deploy sinks them. (The exact v20 engine test is not
/// available; this is the documented approximation.)
fn buried_bounded(
    terrain: &crate::map::TerrainStream,
    bounds: Bounds,
    left: &mut u32,
) -> Result<bool> {
    const ABOVE: f32 = 1000.0;
    let top = bounds.max()[1] as f32 * 0.2;
    for z in bounds.min[2]..bounds.max()[2] {
        for x in bounds.min[0]..bounds.max()[0] {
            probe_spend(left)?;
            let origin = Vec3::new((x as f32 + 0.5) * 0.5, top + ABOVE, (z as f32 + 0.5) * 0.5);
            if !terrain
                .cast_ray(origin, Vec3::NEG_Y, ABOVE * 2.0)
                .is_some_and(|(distance, _)| top + ABOVE - distance > top + 0.002)
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn ground_probe(
    physics: &PhysicsWorld,
    terrain: Option<&crate::map::TerrainStream>,
    bounds: Bounds,
    mut left: Option<&mut u32>,
) -> Result<bool> {
    let map_filter = |_: ColliderHandle, c: &Collider| c.user_data == MAP_TAG;
    let query = physics.query_pipeline_with_filter(QueryFilter::default().predicate(&map_filter));
    let bottom = bounds.min[1] as f32 * 0.2;
    let top = bounds.max()[1] as f32 * 0.2;
    for z in bounds.min[2]..bounds.max()[2] {
        for x in bounds.min[0]..bounds.max()[0] {
            if let Some(left) = left.as_deref_mut() {
                probe_spend(left)?;
            }
            let origin = Vec3::new((x as f32 + 0.5) * 0.5, top, (z as f32 + 0.5) * 0.5);
            let reach = top - bottom + 0.1;
            let floor = query
                .cast_ray_and_get_normal(
                    &Ray::new(Vector::from_array(origin.to_array()), -Vector::Y),
                    reach,
                    true,
                )
                .is_some_and(|(_, h)| h.normal.y > 0.5 && top - h.time_of_impact <= bottom + 0.1);
            let ground = if let Some(t) = terrain {
                if let Some(left) = left.as_deref_mut() {
                    probe_spend(left)?;
                }
                t.cast_ray(origin, Vec3::NEG_Y, reach).is_some()
            } else {
                false
            };
            if floor || ground {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
/// The one "rests on the map" rule, for planting and chain-kill alike.
/// v20's plant-time ground probe, approximately: a ray from the brick's top
/// down to 0.1 below its bottom at each footprint cell finds map floor no
/// higher than 0.1 above the bottom, or terrain reaches above the bottom.
/// Loaded layouts sit a few thousandths into the floor, so the ray must
/// start above it.
fn on_ground(
    physics: &PhysicsWorld,
    terrain: Option<&crate::map::TerrainStream>,
    bounds: Bounds,
) -> bool {
    ground_probe(physics, terrain, bounds, None)
        .expect("Unbounded ground probe cannot exhaust work")
}
