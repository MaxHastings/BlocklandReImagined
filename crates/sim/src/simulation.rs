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
// Brick IDs occupy u64; zero remains available for untagged dynamic bodies.
pub const MAP_TAG: u128 = u128::MAX;
/// How far a brick may dip into an upward-facing map floor. Map floors need
/// not lie on the plate lattice; v20 rests bricks on the nearest plane, so its
/// stock layouts dip up to half a plate in (Kitchen's Town 0.084, a Bedroom
/// shelf 0.062, Pirate World 0.034) and it never refuses such a placement.
pub const FLOOR_DIP: f32 = 0.1;
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
    handles: BTreeMap<BrickId, ColliderHandle>,
    /// Map colliders in `NativeMap::colliders` order.
    map_handles: Vec<ColliderHandle>,
    terrain: Option<crate::map::TerrainStream>,
}
fn pose(brick: &Brick) -> Pose {
    Pose::from_parts(
        Vector::from_array(brick.position),
        Rotation::from_scaled_axis(
            Vector::Y * (-f32::from(brick.quarter_turns) * std::f32::consts::FRAC_PI_2),
        ),
    )
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
            bri_world::ContentRef::Unresolved { namespace, name } => format!("{namespace}/{name}"),
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
        let map_handles = map
            .into_iter()
            .map(|c| physics.insert_collider(c.user_data(MAP_TAG), None))
            .collect();
        let mut index = Index::default();
        let mut handles = BTreeMap::new();
        let mut brick_waters = BTreeMap::new();
        for (id, brick) in &world.bricks {
            let definition = definitions.get(brick)?;
            if let Some(water) = brick_water(brick, definition) {
                brick_waters.insert(*id, water);
            }
            let bounds = Bounds::new(brick, &definition.mesh)?;
            index.insert(*id, bounds);
            handles.insert(
                *id,
                physics.insert_collider(brick_collider(brick, definition, *id), None),
            );
        }
        bri_physics::detect_collisions(&mut physics);
        Ok(Self {
            authority: Authority::new(world)?,
            definitions,
            physics,
            waters: Vec::new(),
            brick_waters,
            liquids: std::sync::OnceLock::new(),
            index,
            handles,
            map_handles,
            terrain: None,
        })
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
        set_enabled(&mut self.physics, &self.map_handles, colliders, enabled)
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
                        Ok(definition) => grid::overlaps(
                            (&brick, mesh, bounds),
                            (other, &definition.mesh, *ob),
                        ),
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
            prepared.push((
                *id,
                Bounds::new(brick, &definition.mesh)?,
                brick_collider(brick, definition, *id),
            ));
        }
        let ids = self.authority.load_build(actor, plan)?;
        for (id, bounds, collider) in prepared {
            self.index.insert(id, bounds);
            let brick = &self.authority.state().bricks[&id];
            if let Some(water) = brick_water(brick, self.definitions.get(brick)?) {
                self.brick_waters.insert(id, water);
                self.liquids = std::sync::OnceLock::new();
            }
            self.handles
                .insert(id, self.physics.insert_collider(collider, None));
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
        let physics = &self.physics;
        let terrain = self.terrain.as_ref();
        let id = self.authority.plant(builder.actor, brick, |world, brick| {
            validate_placement(world, defs, index, physics, terrain, builder, brick)
        })?;
        let brick = &self.authority.state().bricks[&id];
        self.handles.insert(
            id,
            self.physics
                .insert_collider(brick_collider(brick, definition, id), None),
        );
        self.index.insert(id, bounds);
        if let Some(water) = brick_water(brick, definition) {
            self.brick_waters.insert(id, water);
            self.liquids = std::sync::OnceLock::new();
        }
        self.detect_collisions();
        Ok(id)
    }
    /// Plant bricks as one: each passes every plant rule but reach and
    /// support against the world as it stands, the world holds up at least
    /// one of them (they are joined, so it holds up the rest), and either
    /// all are planted or none is. The caller checks reach, rate and the
    /// brick limit, as for a single plant.
    pub fn plant_group(&mut self, actor: &Actor, bricks: Vec<Brick>) -> Result<Vec<BrickId>> {
        ensure!(!bricks.is_empty(), "Nothing to plant");
        if self.state().bricks.len() + bricks.len() > bri_world::MAX_BRICKS {
            return Err(PlantFailure::Limit.into());
        }
        let mut supported = false;
        let mut prepared = Vec::with_capacity(bricks.len());
        for brick in &bricks {
            let definition = self.definitions.get(brick)?;
            supported |= check_placement(
                self.authority.state(),
                &self.definitions,
                &self.index,
                &self.physics,
                self.terrain.as_ref(),
                actor,
                brick,
            )?;
            prepared.push(Bounds::new(brick, &definition.mesh)?);
        }
        if !supported {
            return Err(PlantFailure::Float.into());
        }
        let mut ids = Vec::with_capacity(bricks.len());
        for brick in bricks {
            match self.authority.plant(actor, brick, |_, _| Ok(())) {
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
            let brick = &self.authority.state().bricks[&id];
            let definition = self.definitions.get(brick)?;
            self.handles.insert(
                id,
                self.physics
                    .insert_collider(brick_collider(brick, definition, id), None),
            );
            self.index.insert(id, bounds);
            if let Some(water) = brick_water(brick, definition) {
                self.brick_waters.insert(id, water);
                self.liquids = std::sync::OnceLock::new();
            }
        }
        self.detect_collisions();
        Ok(ids)
    }
    /// The build a copy takes from `start`: it and every brick joined to it
    /// through studs, passing only through bricks `actor` may build on and,
    /// with `above_only`, never below `start`'s bottom. Nearest first.
    /// More than `limit` bricks is refused rather than cut short.
    pub fn build_from(
        &self,
        actor: &Actor,
        start: BrickId,
        limit: usize,
        above_only: bool,
    ) -> Result<Vec<BrickId>> {
        let world = self.state();
        let first = world.bricks.get(&start).context("Unknown brick")?;
        ensure!(
            may_build_on(actor, first),
            "The brick's owner does not trust you enough to do that."
        );
        let floor = self.index.bounds(start).min[1];
        let mut seen = BTreeSet::from([start]);
        let mut order = vec![start];
        let mut next = 0;
        while let Some(&id) = order.get(next) {
            next += 1;
            for other in self.connected_bricks(id)? {
                if (above_only && self.index.bounds(other).min[1] < floor)
                    || !may_build_on(actor, &world.bricks[&other])
                    || !seen.insert(other)
                {
                    continue;
                }
                ensure!(
                    order.len() < limit,
                    "That build has more than {limit} bricks"
                );
                order.push(other);
            }
        }
        Ok(order)
    }
    pub fn edit(&mut self, actor: &Actor, id: BrickId, edit: Edit) -> Result<()> {
        self.authority.edit(actor, id, edit)?;
        self.sync_flags(id);
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
    pub fn remove_many(&mut self, actor: &Actor, ids: &[BrickId]) -> Result<()> {
        for &id in ids {
            self.state().bricks.get(&id).context("Unknown brick")?;
        }
        self.state()
            .revision
            .checked_add(ids.len() as u64)
            .context("Revision exhausted")?;
        for &id in ids {
            self.authority.remove(actor, id)?;
            self.index.remove(id);
            if self.brick_waters.remove(&id).is_some() {
                self.liquids = std::sync::OnceLock::new();
            }
            if let Some(handle) = self.handles.remove(&id) {
                self.physics.remove_collider(handle);
            }
        }
        self.detect_collisions();
        Ok(())
    }
    fn sync_flags(&mut self, id: BrickId) {
        let Some(handle) = self.handles.get(&id) else {
            return;
        };
        let brick = &self.authority.state().bricks[&id];
        let sensor = !brick.colliding
            || self
                .definitions
                .get(brick)
                .is_ok_and(|d| d.special == Special::Water);
        let collider = &mut self.physics.colliders[*handle];
        if collider.is_sensor() == sensor {
            return;
        }
        collider.set_sensor(sensor);
        // Bodies resting on a brick that stops colliding must fall through.
        let aabb = collider.compute_aabb();
        let (min, max) = (
            Vec3::from(aabb.mins.to_array()) - Vec3::splat(1.0),
            Vec3::from(aabb.maxs.to_array()) + Vec3::splat(1.0),
        );
        let resting: Vec<_> = self
            .physics
            .bodies
            .iter()
            .filter(|(_, body)| {
                let p = Vec3::from(body.translation().to_array());
                body.is_dynamic() && body.is_sleeping() && p.cmpge(min).all() && p.cmple(max).all()
            })
            .map(|(handle, _)| handle)
            .collect();
        for handle in resting {
            self.physics.wake_up(handle, true);
        }
    }
    /// Trusted server change from the event engine or game rules.
    pub fn mutate(&mut self, id: BrickId, change: impl FnOnce(&mut Brick)) -> Result<()> {
        self.authority.mutate(id, change)?;
        self.sync_flags(id);
        self.detect_collisions();
        Ok(())
    }
    /// Continue an earlier world's clock (the host changed maps).
    pub fn set_tick(&mut self, tick: u64) {
        self.authority.set_tick(tick);
    }
    pub fn step(&mut self) -> Result<()> {
        self.authority.step()?;
        self.physics.step();
        self.stream_terrain();
        Ok(())
    }
    /// The brick an activation (click) ray reaches. Eye and direction are
    /// from the server's player state, not packet positions.
    pub fn activate(&self, eye: Vec3, direction: Vec3) -> Result<Option<BrickId>> {
        Ok(self.target(eye, direction, 5.0)?.and_then(|hit| hit.brick))
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
    /// The chain-kill root test: a brick resting on the map is ground.
    fn grounded_root(&self, id: BrickId) -> Result<bool> {
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
        let (address, count, liquids) = self
            .liquids
            .get_or_init(|| (source.0, source.1, build()));
        if (*address, *count) == source {
            liquids.clone()
        } else {
            // `waters` was replaced after the first build: build afresh.
            build()
        }
    }
    /// Swap a brick to another definition with the same grid size (the
    /// treasure chest opening, a pumpkin being carved).
    pub fn set_definition(&mut self, id: BrickId, definition: &str) -> Result<()> {
        let brick = self.state().bricks.get(&id).context("Unknown brick")?;
        let old = self.definitions.get(brick)?;
        let new = self
            .definitions
            .entries
            .get(definition)
            .context("Unknown brick definition")?;
        ensure!(
            old.mesh.footprint_studs == new.mesh.footprint_studs
                && old.mesh.height_plates == new.mesh.height_plates,
            "Replacement brick has a different size"
        );
        let definition = definition.to_string();
        self.authority.mutate(id, |b| {
            b.definition = bri_world::ContentRef::Resolved(definition)
        })?;
        let brick = &self.authority.state().bricks[&id];
        let collider = brick_collider(brick, self.definitions.get(brick)?, id);
        if let Some(handle) = self.handles.remove(&id) {
            self.physics.remove_collider(handle);
        }
        self.handles
            .insert(id, self.physics.insert_collider(collider, None));
        self.detect_collisions();
        Ok(())
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
        self.target_filtered(origin, direction, max_distance, false)
    }
    /// Stock editing tools use FxBrickAlwaysObjectType, including bricks whose
    /// ordinary raycasting flag is disabled. Map geometry still obstructs tools.
    pub fn target_bricks_always(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Result<Option<Hit>> {
        self.target_filtered(origin, direction, max_distance, true)
    }
    fn target_filtered(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        all_bricks: bool,
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
        // Walk the index buckets along the ray, nearest first, and stop once
        // the nearest hit lies before the bucket just searched. A brick's
        // collision stays within its grid bounds, so no later bucket can hold
        // a closer hit. Long sight lines through large builds stay cheap.
        let mut tested = std::collections::HashSet::new();
        for (bucket, exit) in
            grid::ray_buckets(origin.to_array(), direction.to_array(), max_distance)
        {
            for id in self.index.bucket(bucket) {
                if !tested.insert(id) {
                    continue;
                }
                self.ray_brick(
                    id,
                    origin,
                    direction,
                    max_distance,
                    all_bricks,
                    &mut nearest,
                )?;
            }
            if nearest
                .as_ref()
                .is_some_and(|hit| hit.distance <= exit - 0.02)
            {
                break;
            }
        }
        Ok(nearest)
    }
    fn ray_brick(
        &self,
        id: BrickId,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        all_bricks: bool,
        nearest: &mut Option<Hit>,
    ) -> Result<()> {
        let brick = &self.state().bricks[&id];
        if !all_bricks && !brick.raycast {
            return Ok(());
        }
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
fn validate_placement(
    world: &World,
    defs: &Definitions,
    index: &Index,
    physics: &PhysicsWorld,
    terrain: Option<&crate::map::TerrainStream>,
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
    if builder.position.distance(Vec3::from(brick.position)) > builder.reach + radius {
        return Err(PlantFailure::TooFar.into());
    }
    if !check_placement(world, defs, index, physics, terrain, builder.actor, brick)? {
        return Err(PlantFailure::Float.into());
    }
    Ok(())
}
/// Every plant rule but reach and support: no overlap, no building onto a
/// brick the actor may not build on, not buried in the map or stuck in a
/// body. Returns whether something already there holds the brick up (a
/// brick it connects to, or the map).
fn check_placement(
    world: &World,
    defs: &Definitions,
    index: &Index,
    physics: &PhysicsWorld,
    terrain: Option<&crate::map::TerrainStream>,
    actor: &Actor,
    brick: &Brick,
) -> Result<bool> {
    let definition = defs.get(brick)?;
    let bounds = Bounds::new(brick, &definition.mesh)?;
    if overlaps_world(world, defs, index, brick, &definition.mesh, bounds)? {
        return Err(PlantFailure::Overlap.into());
    }
    let mut supported = false;
    for id in index.query(bounds.expanded(1)) {
        let existing = &world.bricks[&id];
        let other = defs.get(existing)?;
        let ob = index.bounds(id);
        if grid::connected(
            (brick, &definition.mesh, bounds),
            (existing, &other.mesh, ob),
        ) {
            if !may_build_on(actor, existing) {
                return Err(PlantFailure::Forbidden.into());
            }
            supported = true;
        }
    }
    let placement = pose(brick);
    // Some authored hulls extend below their logical build grid (the stock pine
    // tree by 0.014544 units). Preserve that hull for physics, but allow its
    // below-grid extent, plus FLOOR_DIP, at an upward-facing map surface only.
    // Walls, ceilings and moving entities get no allowance.
    let local_bottom = definition.shape.compute_local_aabb().mins.y;
    let authored_below_grid =
        (-(definition.mesh.height_plates as f32) * 0.1 - local_bottom).max(0.0);
    let aabb = definition.shape.compute_aabb(&placement);
    let query = physics.query_pipeline();
    // Terrain is judged by `buried` below, not by contact: v20 deploys a
    // ghost aimed at terrain 0.1 into it, and a level brick on a slope dips
    // into the uphill side, so terrain may reach above a brick's bottom.
    let is_terrain = |handle| terrain.is_some_and(|t| t.is_terrain_collider(handle));
    for (_, obstacle) in query
        .intersect_aabb_conservative(aabb)
        .filter(|(handle, c)| {
            (c.user_data == MAP_TAG && !is_terrain(*handle))
                || (!c.is_sensor() && c.parent().is_some_and(|p| !physics.bodies[p].is_fixed()))
        })
    {
        if let Some(contact) = rapier3d::parry::query::contact(
            &placement,
            definition.shape.as_ref(),
            obstacle.position(),
            obstacle.shape(),
            0.0,
        )
        .map_err(|_| anyhow::anyhow!("Unsupported obstacle/brick collision pair"))?
        {
            let allowance = if obstacle.user_data == MAP_TAG && contact.normal2.y > 0.7 {
                FLOOR_DIP + authored_below_grid
            } else {
                0.0
            };
            if contact.dist < -0.002 - allowance {
                return Err(if obstacle.user_data == MAP_TAG {
                    PlantFailure::Buried
                } else {
                    PlantFailure::Stuck
                }
                .into());
            }
        }
    }
    if terrain.is_some_and(|t| buried(t, bounds)) {
        return Err(PlantFailure::Buried.into());
    }
    // The chain-kill root test: a brick the map holds up stays ground.
    Ok(supported || on_ground(physics, terrain, bounds))
}
/// A brick is buried when the terrain surface stands above its top over its
/// whole footprint: nothing of it would show. Partly sunk bricks plant, as
/// v20's own terrain deploy sinks them. (The exact v20 engine test is not
/// available; this is the documented approximation.)
fn buried(terrain: &crate::map::TerrainStream, bounds: Bounds) -> bool {
    const ABOVE: f32 = 1000.0;
    let top = bounds.max()[1] as f32 * 0.2;
    (bounds.min[2]..bounds.max()[2]).all(|z| {
        (bounds.min[0]..bounds.max()[0]).all(|x| {
            let origin = Vec3::new((x as f32 + 0.5) * 0.5, top + ABOVE, (z as f32 + 0.5) * 0.5);
            terrain
                .cast_ray(origin, Vec3::NEG_Y, ABOVE * 2.0)
                .is_some_and(|(distance, _)| top + ABOVE - distance > top + 0.002)
        })
    })
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
    let map_filter = |_: ColliderHandle, c: &Collider| c.user_data == MAP_TAG;
    let query = physics.query_pipeline_with_filter(QueryFilter::default().predicate(&map_filter));
    let bottom = bounds.min[1] as f32 * 0.2;
    let top = bounds.max()[1] as f32 * 0.2;
    for z in bounds.min[2]..bounds.max()[2] {
        for x in bounds.min[0]..bounds.max()[0] {
            let origin = Vec3::new((x as f32 + 0.5) * 0.5, top, (z as f32 + 0.5) * 0.5);
            let reach = top - bottom + 0.1;
            let floor = query
                .cast_ray_and_get_normal(
                    &Ray::new(Vector::from_array(origin.to_array()), -Vector::Y),
                    reach,
                    true,
                )
                .is_some_and(|(_, h)| h.normal.y > 0.5 && top - h.time_of_impact <= bottom + 0.1);
            let ground = terrain.is_some_and(|t| t.cast_ray(origin, Vec3::NEG_Y, reach).is_some());
            if floor || ground {
                return true;
            }
        }
    }
    false
}
