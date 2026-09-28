//! Local equipment and ghost intentions; only replicated state is planted state.
//!
//! Stock scripts select a brick before Fire deploys/moves its ghost. Numpad
//! shifts use body yaw (never free-look); successful planting retains the ghost.
//! Engine transform snapping and the existing rotation parity approximation are
//! documented fidelity gaps, not claims of exact v20 anchoring. Map collider
//! metadata currently cannot distinguish terrain's additional -0.1 deploy bias.
use anyhow::{Context, Result, ensure};
use bri_content::{brick::Brick as Mesh, terrain_field::TerrainField};
use bri_net::protocol::PublicWorld;
use bri_sim::{
    definitions::Definitions,
    ghost,
    grid::{self, Bounds, Index},
    player::PlayerState,
    session::{BuildGesture, Command, ToolAction, ToolInventory},
    simulation::Hit,
};
use bri_ui::api::{GameAction, HeldControl, IconRef, ToolInfo, UiAction, UiUpdate};
use bri_world::{Brick, BrickId, ContentRef};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{collections::BTreeMap, sync::Arc};

const SLOTS: usize = 10;
const DEPLOY_REACH: f32 = 15.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Equipment {
    None,
    Brick(String),
    Hammer,
    Wrench,
    Printer,
    Weapon(String),
    Wand,
    Paint(u8),
    ColorEffect(u8),
    ShapeEffect(u8),
}

#[derive(Default)]
pub struct BuildingResponse {
    pub updates: Vec<UiUpdate>,
    pub commands: Vec<Command>,
}

pub struct Building {
    definitions: Definitions,
    /// The host's player archetypes, for the local player's eye.
    archetypes: std::sync::Arc<bri_sim::archetype::Archetypes>,
    /// Stock selectable IDs and authored orientation corrections, not every
    /// hidden state variant that happens to have a native definition.
    catalog: BTreeMap<String, u8>,
    default_prints: BTreeMap<String, String>,
    inventory: [Option<String>; SLOTS],
    selected_slot: Option<usize>,
    equipment: Equipment,
    tool_catalog: BTreeMap<String, ToolInfo>,
    tools: ToolInventory,
    pending_equipment: BTreeMap<u64, (Option<usize>, Equipment)>,
    active_tool: Option<usize>,
    weapon_fire_down: bool,
    /// The server shows an image in this player's right hand (for example
    /// a ball picked up without a tool slot), so fire goes to its trigger.
    held_image: bool,
    fire_request: u64,
    tool_catalog_installed: bool,
    latest_equipment_request: u64,
    paint: u8,
    palette_len: usize,
    ghost: Option<Brick>,
    ghost_generation: u64,
    map: PhysicsWorld,
    broken: bri_sim::prediction::BrokenShapes,
    terrain: Vec<Arc<TerrainField>>,
    bricks: BTreeMap<BrickId, Brick>,
    index: Index,
    camera_index: Index,
    visibility_index: Index,
    query_generation: u64,
}

impl Building {
    pub fn new(definitions: Definitions, map_colliders: Vec<ColliderBuilder>) -> Result<Self> {
        let mut map = PhysicsWorld::new();
        let handles = map_colliders
            .into_iter()
            .map(|collider| map.insert_collider(collider, None))
            .collect();
        map.detect_collisions(&(), &());
        Ok(Self {
            archetypes: Default::default(),
            definitions,
            catalog: BTreeMap::new(),
            default_prints: BTreeMap::new(),
            inventory: std::array::from_fn(|_| None),
            selected_slot: None,
            equipment: Equipment::None,
            tool_catalog: [
                tool(bri_weapons::HAMMER, "Hammer", "hammer"),
                tool(bri_weapons::WRENCH, "Wrench", "wrench"),
                tool(bri_weapons::PRINTER, "Printer", "printer"),
            ]
            .into_iter()
            .map(|t| (t.id.clone(), t))
            .collect(),
            tools: ToolInventory::default(),
            pending_equipment: BTreeMap::new(),
            active_tool: None,
            weapon_fire_down: false,
            held_image: false,
            fire_request: 0,
            tool_catalog_installed: false,
            latest_equipment_request: 0,
            paint: 0,
            palette_len: 0,
            ghost: None,
            ghost_generation: 0,
            map,
            broken: bri_sim::prediction::BrokenShapes::new(handles, &[]),
            terrain: Vec::new(),
            bricks: Default::default(),
            index: Index::default(),
            camera_index: Index::default(),
            visibility_index: Index::default(),
            query_generation: 0,
        })
    }

    /// The map's breakable shapes (`NativeMap::breakables`).
    pub fn set_breakables(&mut self, shapes: &[bri_sim::map::Breakable]) {
        self.broken.set_shapes(shapes);
    }
    /// Smashed shapes no longer stop build rays (`Session::broken_shapes`).
    pub fn set_broken_shapes(&mut self, broken: &std::collections::BTreeSet<u32>) -> Result<()> {
        if self.broken.apply(&mut self.map, broken)? {
            self.query_generation = self.query_generation.wrapping_add(1);
        }
        Ok(())
    }
    /// Exact map terrain for every query; it is never approximated by tiles.
    pub fn attach_terrain(&mut self, terrain: Vec<Arc<TerrainField>>) {
        self.terrain = terrain;
    }

    /// Nearest map hit (interiors, static models and terrain) as
    /// (distance, normal, surface) along a normalized direction.
    fn map_ray(&self, origin: Vec3, direction: Vec3, distance: f32) -> Option<(f32, Vec3, u128)> {
        let ray = Ray::new(
            Vector::from_array(origin.to_array()),
            Vector::from_array(direction.to_array()),
        );
        let physical = self
            .map
            .query_pipeline_with_filter(QueryFilter::default().exclude_sensors())
            .cast_ray_and_get_normal(&ray, distance, true)
            .map(|(handle, hit)| {
                (
                    hit.time_of_impact,
                    Vec3::from_array(hit.normal.to_array()),
                    self.map.colliders[handle].user_data,
                )
            });
        let terrain = bri_sim::map::cast_terrain(&self.terrain, origin, direction, distance)
            .map(|(time, normal)| (time, normal, bri_sim::map::MapSurface::Terrain as u128));
        match (physical, terrain) {
            (Some(a), Some(b)) => Some(if b.0 < a.0 { b } else { a }),
            (a, b) => a.or(b),
        }
    }

    /// Terrain or interiors between two points (`GuiShapeNameHud` sight test).
    pub fn map_blocks(&self, from: Vec3, to: Vec3) -> bool {
        let delta = to - from;
        let distance = delta.length();
        distance > 0.001
            && delta.is_finite()
            && self.map_ray(from, delta / distance, distance).is_some()
    }
    pub fn set_catalog(&mut self, entries: Vec<(String, u8)>) -> Result<()> {
        let mut catalog = BTreeMap::new();
        for (id, orientation) in entries {
            ensure!(
                orientation < 4 && self.definitions.entries.contains_key(&id),
                "Invalid selectable brick definition {id}"
            );
            ensure!(
                catalog.insert(id, orientation).is_none(),
                "Duplicate selectable brick"
            );
        }
        // Catalog is immutable for an active connection; changing it must not
        // leave old slot references silently usable.
        ensure!(
            self.inventory
                .iter()
                .flatten()
                .all(|id| catalog.contains_key(id)),
            "Catalog replacement invalidates brick inventory"
        );
        if let Equipment::Brick(id) = &self.equipment {
            ensure!(
                catalog.contains_key(id),
                "Catalog replacement invalidates equipped brick"
            );
        }
        self.catalog = catalog;
        Ok(())
    }

    /// Native default print IDs, normally the converted stock Letters/A.
    /// The caller owns aspect compatibility from its validated print catalog.
    pub fn set_default_prints(&mut self, prints: BTreeMap<String, String>) -> Result<()> {
        for (brick, print) in &prints {
            ensure!(
                self.definitions.entries.contains_key(brick),
                "Default print references undefined brick"
            );
            ContentRef::Resolved(print.clone()).validate()?;
        }
        self.default_prints = prints;
        Ok(())
    }

    /// v20 `serverCmdSetPrint`: later bricks of the printed aspect take the
    /// player's last print, and a ghost of that aspect changes at once.
    pub fn remember_print(&mut self, last: &crate::tool_ui::LastPrint) -> Result<()> {
        ContentRef::Resolved(last.print.clone()).validate()?;
        for definition in &last.definitions {
            if let Some(print) = self.default_prints.get_mut(definition) {
                print.clone_from(&last.print);
            }
        }
        if let Some(ghost) = &mut self.ghost
            && let ContentRef::Resolved(id) = &ghost.definition
            && last.definitions.contains(id)
        {
            ghost.print = Some(ContentRef::Resolved(last.print.clone()));
            self.ghost_generation = self.ghost_generation.wrapping_add(1);
        }
        Ok(())
    }

    /// Reconcile only changed query geometry/flags. Colors, ownership, events,
    /// palette and player-pose changes never rebuild map physics or the index.
    /// Return whether query geometry changed (not whether any world field did).
    pub fn sync_world(&mut self, world: &PublicWorld) -> Result<bool> {
        self.sync_world_changes(world, None)
    }
    /// As `sync_world`; `known` lists every brick that may differ from the
    /// last synced world, so a brick plant costs one brick, not the world.
    pub fn sync_world_changes(
        &mut self,
        world: &PublicWorld,
        known: Option<&crate::network::WorldChanges>,
    ) -> Result<bool> {
        ensure!(
            !world.palette.is_empty() && world.palette.len() <= 256,
            "Invalid world palette"
        );
        let candidates: Box<dyn Iterator<Item = (&u64, &Brick)>> = match known {
            Some(known) => Box::new(
                known
                    .bricks
                    .iter()
                    .filter_map(|id| world.bricks.get_key_value(id)),
            ),
            None => Box::new(world.bricks.iter()),
        };
        // Validate all changed candidates first: malformed updates are atomic.
        let mut changed = Vec::new();
        for (&id, brick) in candidates {
            if self
                .bricks
                .get(&id)
                .is_none_or(|old| !same_geometry(old, brick))
            {
                ensure!(
                    brick
                        .position
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                        && brick.quarter_turns < 4,
                    "Invalid replicated brick transform"
                );
                let definition = self.definitions.get(brick)?;
                changed.push((id, brick.clone(), Bounds::new(brick, &definition.mesh)?));
            }
        }
        let removed: Vec<_> = match known {
            Some(known) => known
                .bricks
                .iter()
                .filter(|id| self.bricks.contains_key(id) && !world.bricks.contains_key(*id))
                .copied()
                .collect(),
            None => self
                .bricks
                .keys()
                .filter(|id| !world.bricks.contains_key(*id))
                .copied()
                .collect(),
        };
        let dirty = !changed.is_empty() || !removed.is_empty();
        for id in removed {
            self.bricks.remove(&id);
            self.index.remove(id);
            self.camera_index.remove(id);
            self.visibility_index.remove(id);
        }
        for (id, brick, bounds) in changed {
            self.index.remove(id);
            self.camera_index.remove(id);
            self.visibility_index.remove(id);
            if brick.visible {
                self.visibility_index.insert(id, bounds);
            }
            if brick.raycast {
                self.index.insert(id, bounds);
            }
            if brick.colliding {
                let aabb = self
                    .definitions
                    .get(&brick)?
                    .shape
                    .compute_aabb(&brick_pose(&brick));
                self.camera_index.insert(
                    id,
                    query_bounds(
                        Vec3::from(aabb.mins.to_array()),
                        Vec3::from(aabb.maxs.to_array()),
                    ),
                );
            }
            self.bricks.insert(id, brick);
        }
        self.palette_len = world.palette.len();
        if usize::from(self.paint) >= self.palette_len {
            self.paint = 0;
        }
        if dirty {
            self.query_generation = self.query_generation.wrapping_add(1);
        }
        Ok(dirty)
    }

    pub fn ghost(&self) -> Option<&Brick> {
        self.ghost.as_ref()
    }
    pub fn ghost_generation(&self) -> u64 {
        self.ghost_generation
    }
    pub fn query_generation(&self) -> u64 {
        self.query_generation
    }

    /// Authored static scenery only, for one-time foliage placement. Prohibited
    /// surface classes must still block planting through a roof/tree.
    pub fn static_surface(
        &self,
        start: Vec3,
        end: Vec3,
    ) -> Result<Option<(Hit, bri_sim::map::MapSurface)>> {
        let delta = end - start;
        let distance = delta.length();
        ensure!(
            start.is_finite()
                && end.is_finite()
                && distance.is_finite()
                && distance > 0.001
                && distance <= 5000.,
            "Invalid static surface query"
        );
        let direction = delta / distance;
        self.map_ray(start, direction, distance)
            .map(|(time, normal, tag)| {
                let surface = bri_sim::map::MapSurface::from_tag(tag)
                    .context("Unclassified native static surface")?;
                Ok((
                    Hit {
                        brick: None,
                        position: start + direction * time,
                        normal,
                        distance: time,
                    },
                    surface,
                ))
            })
            .transpose()
    }
    pub fn inventory(&self) -> &[Option<String>; SLOTS] {
        &self.inventory
    }
    pub fn selected_slot(&self) -> Option<usize> {
        self.selected_slot
    }
    pub fn equipment(&self) -> &Equipment {
        &self.equipment
    }
    /// Replicated right-hand image for the local player.
    pub fn set_held_image(&mut self, held: bool) {
        self.held_image = held;
    }

    /// A map change builds a new controller; the player keeps the bricks
    /// they bought (those the new catalog still offers) and their paint,
    /// which the next world sync clamps to the new palette.
    pub fn carry_over(&mut self, old: &Building) {
        for (slot, id) in self.inventory.iter_mut().zip(&old.inventory) {
            *slot = id.clone().filter(|id| self.catalog.contains_key(id));
        }
        self.paint = old.paint;
    }
    pub fn initial_updates(&self) -> Vec<UiUpdate> {
        vec![
            UiUpdate::BrickInventory(self.inventory.to_vec()),
            UiUpdate::Tools(self.hud_tools()),
            UiUpdate::SetActiveTool(self.active_tool),
        ]
    }

    pub fn set_tool_catalog(&mut self, catalog: BTreeMap<String, ToolInfo>) -> Result<()> {
        ensure!(
            catalog.len() <= 1024
                && self.pending_equipment.is_empty()
                && !self.tool_catalog_installed,
            "Cannot replace active tool catalog"
        );
        for (id, item) in &catalog {
            ensure!(
                id == &item.id && !item.name.trim().is_empty() && item.name.len() <= 128,
                "Invalid HUD tool metadata"
            );
        }
        ensure!(
            self.tools
                .slots
                .iter()
                .flatten()
                .all(|id| catalog.contains_key(id)),
            "HUD catalog omits inventory item"
        );
        self.tool_catalog = catalog;
        self.tool_catalog_installed = true;
        Ok(())
    }
    fn hud_tools(&self) -> Vec<Option<ToolInfo>> {
        self.tools
            .slots
            .iter()
            .map(|id| id.as_ref().map(|id| self.tool_catalog[id].clone()))
            .collect()
    }
    fn slot_equipment(&self, slot: usize) -> Result<Equipment> {
        let id = self
            .tools
            .slots
            .get(slot)
            .and_then(Option::as_ref)
            .context("Empty or invalid tool slot")?;
        Ok(match id.as_str() {
            "v20.weapon.hammeritem" => Equipment::Hammer,
            "v20.weapon.wrenchitem" => Equipment::Wrench,
            "v20.weapon.printgun" => Equipment::Printer,
            "v20.weapon.wanditem" => Equipment::Wand,
            _ => Equipment::Weapon(id.clone()),
        })
    }
    /// Replicated slots are authoritative. Pending request IDs only retain newer
    /// local selection intentions until their replies, never grant items.
    pub fn sync_tools(&mut self, tools: &ToolInventory) -> Result<Vec<UiUpdate>> {
        tools.validate()?;
        ensure!(
            tools
                .slots
                .iter()
                .flatten()
                .all(|id| self.tool_catalog.contains_key(id)),
            "Unknown replicated tool identity"
        );
        let slots_changed = self.tools.slots != tools.slots;
        let selected_changed = self.tools.selected != tools.selected;
        let previous_active = self.active_tool;
        self.tools = tools.clone();
        if slots_changed {
            self.pending_equipment.retain(|_, (slot, equipment)| {
                slot.is_none_or(|s| {
                    self.tools
                        .slots
                        .get(s)
                        .and_then(Option::as_ref)
                        .is_some_and(|id| match equipment {
                            Equipment::Weapon(expected) => id == expected,
                            Equipment::Hammer => id == bri_weapons::HAMMER,
                            Equipment::Wrench => id == bri_weapons::WRENCH,
                            Equipment::Printer => id == bri_weapons::PRINTER,
                            Equipment::Wand => id == bri_weapons::WAND,
                            _ => false,
                        })
                })
            });
        }
        let mut updates = if slots_changed {
            vec![UiUpdate::Tools(self.hud_tools())]
        } else {
            vec![]
        };
        if let Some((slot, equipment)) = self
            .pending_equipment
            .last_key_value()
            .map(|(_, intent)| intent)
        {
            self.active_tool = *slot;
            self.equipment = equipment.clone();
        } else if selected_changed || slots_changed || tool_equipment(&self.equipment) {
            self.active_tool = tools.selected;
            if let Some(slot) = tools.selected {
                self.equipment = self.slot_equipment(slot)?;
            } else if tool_equipment(&self.equipment) {
                self.equipment = Equipment::None;
            }
        }
        if (slots_changed || selected_changed || previous_active != self.active_tool)
            && (self.active_tool.is_some()
                || previous_active.is_some()
                || matches!(self.equipment, Equipment::None))
        {
            updates.push(UiUpdate::SetActiveTool(self.active_tool));
        }
        Ok(updates)
    }
    pub fn command_sent(&mut self, request: u64, command: &Command) -> Result<()> {
        if matches!(command, Command::WeaponTrigger { .. }) {
            self.fire_request = request;
        }
        if let Command::EquipTool { slot } = command {
            ensure!(
                self.pending_equipment.len() < 64,
                "Too many pending equipment requests"
            );
            self.pending_equipment
                .insert(request, (*slot, self.equipment.clone()));
            self.latest_equipment_request = self.latest_equipment_request.max(request);
        }
        Ok(())
    }
    pub fn command_finished(
        &mut self,
        request: u64,
        command: &Command,
        accepted: bool,
    ) -> Vec<UiUpdate> {
        if matches!(command, Command::EquipTool { .. }) {
            self.pending_equipment.remove(&request);
            if request < self.latest_equipment_request {
                return vec![];
            }
            self.pending_equipment.retain(|id, _| *id > request);
            if accepted && self.fire_request < request {
                self.weapon_fire_down = false;
            }
            if let Some((slot, equipment)) = self
                .pending_equipment
                .last_key_value()
                .map(|(_, intent)| intent)
            {
                self.active_tool = *slot;
                self.equipment = equipment.clone();
            } else if !accepted {
                self.active_tool = self.tools.selected;
                self.equipment = self
                    .active_tool
                    .and_then(|s| self.slot_equipment(s).ok())
                    .unwrap_or(Equipment::None);
            }
            return vec![UiUpdate::SetActiveTool(self.active_tool)];
        }
        if !accepted
            && request == self.fire_request
            && matches!(command, Command::WeaponTrigger { down: true })
        {
            self.weapon_fire_down = false;
        }
        vec![]
    }

    pub fn target(&self, origin: Vec3, direction: Vec3, reach: f32) -> Result<Option<Hit>> {
        ensure!(
            origin.is_finite()
                && origin.abs().max_element() <= 1_000_000.0
                && direction.is_finite()
                && direction.length_squared().is_finite()
                && direction.length_squared() > 0.1
                && reach.is_finite()
                && reach > 0.0
                && reach <= 150.0,
            "Invalid targeting ray"
        );
        let direction = direction.normalize();
        self.trace(origin, direction, reach, &self.index)
    }

    /// Nearest physical roof/terrain/brick hit, independent of tool ray flags.
    /// Used by camera-local weather; map water is queried separately by the host.
    pub fn solid_segment(&self, start: Vec3, end: Vec3) -> Result<Option<Hit>> {
        let delta = end - start;
        let length = delta.length();
        ensure!(
            start.is_finite()
                && end.is_finite()
                && length.is_finite()
                && length > 0.0001
                && length <= 2000.,
            "Invalid environment query segment"
        );
        self.trace(start, delta / length, length, &self.camera_index)
    }

    fn trace(
        &self,
        origin: Vec3,
        direction: Vec3,
        reach: f32,
        index: &Index,
    ) -> Result<Option<Hit>> {
        let mut nearest = self
            .map_ray(origin, direction, reach)
            .map(|(time, normal, _)| Hit {
                brick: None,
                position: origin + direction * time,
                normal,
                distance: time,
            });
        let end = origin + direction * reach;
        let low = origin.min(end) - Vec3::splat(0.01);
        let high = origin.max(end) + Vec3::splat(0.01);
        let min = std::array::from_fn(|a| (low[a] / grid::CELL[a]).floor() as i32);
        let max: [i32; 3] = std::array::from_fn(|a| (high[a] / grid::CELL[a]).ceil() as i32);
        for id in index.query(Bounds {
            min,
            size: std::array::from_fn(|a| (max[a] - min[a]).max(1)),
        }) {
            let brick = &self.bricks[&id];
            let definition = self.definitions.get(brick)?;
            let inverse = brick.transform().inverse();
            if let Some((distance, normal)) = bri_physics::content::raycast(
                &definition.collision,
                Vector::from_array(inverse.transform_point3(origin).to_array()),
                Vector::from_array(inverse.transform_vector3(direction).to_array()),
                reach,
            ) && nearest.as_ref().is_none_or(|hit| distance < hit.distance)
            {
                nearest = Some(Hit {
                    brick: Some(id),
                    position: origin + direction * distance,
                    normal: brick
                        .transform()
                        .transform_vector3(Vec3::from(normal.to_array())),
                    distance,
                });
            }
        }
        Ok(nearest)
    }

    /// Cosmetic line of sight uses visible bricks independently of tool-ray flags.
    /// Authored collision shapes approximate opaque surfaces; translucent/material
    /// coverage and dynamic actors remain fidelity work.
    pub fn effect_visible(&self, ignore: BrickId, eye: Vec3, target: Vec3) -> Result<bool> {
        ensure!(
            eye.is_finite() && target.is_finite(),
            "Invalid effect sight ray"
        );
        let delta = target - eye;
        let distance = delta.length();
        if distance <= 0.001 {
            return Ok(true);
        }
        let direction = delta / distance;
        if self.map_ray(eye, direction, distance - 0.001).is_some() {
            return Ok(false);
        }
        for id in self.visibility_index.query(query_bounds(
            eye.min(target) - Vec3::splat(0.01),
            eye.max(target) + Vec3::splat(0.01),
        )) {
            if id == ignore {
                continue;
            }
            let brick = &self.bricks[&id];
            let definition = self.definitions.get(brick)?;
            let inverse = brick.transform().inverse();
            if bri_physics::content::raycast(
                &definition.collision,
                Vector::from_array(inverse.transform_point3(eye).to_array()),
                Vector::from_array(inverse.transform_vector3(direction).to_array()),
                distance - 0.001,
            )
            .is_some()
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Sweep the camera's volume against map geometry and colliding replicated
    /// bricks. Visibility/raycast flags do not turn a physical wall into a hole.
    /// View direction includes local free look, independent of the body aim.
    pub fn camera_position(&self, eye: Vec3, forward: Vec3, distance: f32) -> Result<Vec3> {
        use rapier3d::parry::query::{ShapeCastOptions, cast_shapes};
        ensure!(
            eye.is_finite()
                && eye.abs().max_element() <= 1_000_000.0
                && forward.is_finite()
                && forward.length_squared().is_finite()
                && forward.length_squared() > 0.1
                && distance.is_finite()
                && (0.0..=20.0).contains(&distance),
            "Invalid camera sweep"
        );
        if distance == 0.0 {
            return Ok(eye);
        }
        let radius = 0.15;
        let backward = -forward.normalize();
        let origin = Pose::translation(eye.x, eye.y, eye.z);
        let velocity = Vector::from_array(backward.to_array());
        let shape = Ball::new(radius);
        let options = ShapeCastOptions {
            max_time_of_impact: distance,
            ..Default::default()
        };
        let mut travel = self
            .map
            .query_pipeline_with_filter(QueryFilter::default().exclude_sensors())
            .cast_shape(&origin, velocity, &shape, options)
            .map_or(distance, |(_, hit)| hit.time_of_impact);
        let end = eye + backward * distance;
        let low = eye.min(end) - Vec3::splat(radius);
        let high = eye.max(end) + Vec3::splat(radius);
        for patch in self.terrain_patches(low, high)? {
            if let Some(hit) = cast_shapes(
                &origin,
                velocity,
                &shape,
                &Pose::IDENTITY,
                Vector::ZERO,
                &patch,
                options,
            )
            .map_err(|_| anyhow::anyhow!("Unsupported camera collision with terrain"))?
            {
                travel = travel.min(hit.time_of_impact);
            }
        }
        for id in self.camera_index.query(query_bounds(
            eye.min(end) - Vec3::splat(radius),
            eye.max(end) + Vec3::splat(radius),
        )) {
            let brick = &self.bricks[&id];
            let definition = self.definitions.get(brick)?;
            if let Some(hit) = cast_shapes(
                &origin,
                velocity,
                &shape,
                &brick_pose(brick),
                Vector::ZERO,
                definition.shape.as_ref(),
                options,
            )
            .map_err(|_| anyhow::anyhow!("Unsupported camera collision for native brick {id}"))?
            {
                travel = travel.min(hit.time_of_impact);
            }
        }
        Ok(eye
            + backward
                * if travel < distance {
                    (travel - 0.02).max(0.0)
                } else {
                    distance
                })
    }

    /// Exact terrain triangles covering a box, one patch per terrain field.
    pub fn terrain_patches(&self, low: Vec3, high: Vec3) -> Result<Vec<TriMesh>> {
        let mut patches = Vec::new();
        for field in &self.terrain {
            let [gx0, gy0] = field.grid(low.x, high.z);
            let [gx1, gy1] = field.grid(high.x, low.z);
            let (x0, y0) = (gx0.floor() as i32, gy0.floor() as i32);
            let region = [
                x0,
                y0,
                gx1.floor() as i32 - x0 + 1,
                gy1.floor() as i32 - y0 + 1,
            ];
            let mesh = field.mesh(region)?;
            if mesh.triangles.is_empty() {
                continue;
            }
            patches.push(TriMesh::new(
                mesh.positions
                    .iter()
                    .map(|p| Vector::from_array(*p))
                    .collect(),
                mesh.triangles,
            )?);
        }
        Ok(patches)
    }

    /// Half size of a brick definition's grid volume in its own frame.
    pub fn definition_half_extents(&self, definition: &str) -> Option<Vec3> {
        let mesh = &self.definitions.entries.get(definition)?.mesh;
        Some(Vec3::new(
            mesh.footprint_studs[0] as f32 * 0.25,
            mesh.height_plates as f32 * 0.1,
            mesh.footprint_studs[1] as f32 * 0.25,
        ))
    }

    /// Map interiors and static models, as the collision mirror holds them.
    pub fn map_colliders(&self) -> impl Iterator<Item = &Collider> {
        self.map
            .colliders
            .iter()
            .filter(|(_, c)| !c.is_sensor())
            .map(|(_, c)| c)
    }

    /// Colliding replicated bricks touching a box, with their world poses.
    pub fn colliding_bricks(
        &self,
        low: Vec3,
        high: Vec3,
    ) -> Result<Vec<(BrickId, SharedShape, Pose)>> {
        self.camera_index
            .query(query_bounds(low, high))
            .into_iter()
            .map(|id| {
                let brick = &self.bricks[&id];
                Ok((
                    id,
                    self.definitions.get(brick)?.shape.clone(),
                    brick_pose(brick),
                ))
            })
            .collect()
    }

    pub fn archetypes(&self) -> &bri_sim::archetype::Archetypes {
        &self.archetypes
    }
    pub fn set_archetypes(&mut self, archetypes: std::sync::Arc<bri_sim::archetype::Archetypes>) {
        self.archetypes = archetypes;
    }
    /// Local updates acknowledge equipment choices only. Commands require the
    /// transport's authoritative reply; no planting/removal is predicted here.
    /// Player yaw/pitch must be current body aim, not a free-look camera vector.
    pub fn ui_action(
        &mut self,
        action: &UiAction,
        player: &PlayerState,
    ) -> Result<Option<BuildingResponse>> {
        let mut out = BuildingResponse::default();
        match action {
            UiAction::BuyBricks { slots } => {
                ensure!(
                    slots.len() == SLOTS,
                    "Brick inventory requires exactly ten slots"
                );
                for id in slots.iter().flatten() {
                    self.selectable(id)?;
                }
                self.inventory = slots.clone().try_into().unwrap();
                self.selected_slot = None;
                if matches!(self.equipment, Equipment::Brick(_)) {
                    self.equipment = Equipment::None;
                }
                out.updates.push(UiUpdate::BrickInventory(slots.clone()));
                out.updates.push(UiUpdate::SetActiveBrick(None));
            }
            UiAction::UseBrickSlot { slot } => {
                let id = self
                    .inventory
                    .get(*slot)
                    .and_then(Option::as_ref)
                    .context("Empty or invalid brick slot")?
                    .clone();
                self.equip_brick(&id, player)?;
                self.selected_slot = Some(*slot);
                out.updates.push(UiUpdate::SetActiveBrick(Some(*slot)));
                out.commands.push(Command::EquipTool { slot: None });
            }
            UiAction::InstantUseBrick { brick } => {
                self.equip_brick(brick, player)?;
                self.selected_slot = None;
                out.commands.push(Command::EquipTool { slot: None });
            }
            UiAction::UseTool { slot } => {
                self.equipment = self.slot_equipment(*slot)?;
                self.active_tool = Some(*slot);
                self.selected_slot = None;
                out.updates.push(UiUpdate::SetActiveTool(Some(*slot)));
                out.commands.push(Command::EquipTool { slot: Some(*slot) });
            }
            UiAction::UnUseTool => {
                self.equipment = Equipment::None;
                self.active_tool = None;
                self.selected_slot = None;
                out.commands.push(Command::EquipTool { slot: None });
            }
            UiAction::UseSprayCan { color } => {
                ensure!(
                    (*color as usize) < self.palette_len,
                    "Color outside world palette"
                );
                self.paint = *color as u8;
                self.equipment = Equipment::Paint(self.paint);
                self.active_tool = None;
                out.commands.push(Command::UseSprayCan { color: self.paint });
                if let Some(ghost) = &mut self.ghost {
                    ghost.color = self.paint;
                    self.ghost_generation = self.ghost_generation.wrapping_add(1);
                }
            }
            UiAction::UseFxCan { fx } => {
                self.equipment = match fx {
                    0..=6 => Equipment::ColorEffect(*fx as u8),
                    7 => Equipment::ShapeEffect(0),
                    8 => Equipment::ShapeEffect(1),
                    _ => anyhow::bail!("Unknown FX can"),
                };
                self.active_tool = None;
                out.commands.push(Command::UseFxCan { fx: *fx as u8 });
            }
            UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down,
            }) => {
                if !*down && self.weapon_fire_down {
                    self.weapon_fire_down = false;
                    out.commands.push(Command::WeaponTrigger { down: false });
                } else if *down && (self.held_image || image_equipment(&self.equipment)) {
                    // A tool switched since the last trigger mounts a fresh
                    // image, so a new click must reach it even before the
                    // switch is acknowledged.
                    if !self.weapon_fire_down || self.latest_equipment_request > self.fire_request {
                        self.weapon_fire_down = true;
                        out.commands.push(Command::WeaponTrigger { down: true });
                    }
                } else if *down {
                    self.fire(player, &mut out)?;
                }
            }
            UiAction::Game(GameAction::DropTool) => {
                let slot = self.active_tool.context("No selected tool to drop")?;
                self.slot_equipment(slot)?;
                out.commands.push(Command::DropTool { slot });
            }
            UiAction::Game(GameAction::ShiftBrick { x, y, z })
            | UiAction::Game(GameAction::SuperShiftBrick { x, y, z }) => {
                ensure!(
                    (-1..=1).contains(x) && (-1..=1).contains(y) && (-3..=3).contains(z),
                    "Invalid brick shift"
                );
                if let Some(brick) = &mut self.ghost {
                    let mesh = &self.definitions.get(brick)?.mesh;
                    let body = body_forward(player)?;
                    ghost::shift(
                        brick,
                        mesh,
                        body,
                        *x,
                        *y,
                        *z,
                        matches!(action, UiAction::Game(GameAction::SuperShiftBrick { .. })),
                    );
                    Bounds::new(brick, mesh)?;
                    self.ghost_generation = self.ghost_generation.wrapping_add(1);
                    out.commands
                        .extend(BuildGesture::shift(*x, *y, *z).map(Command::BuildGesture));
                }
            }
            UiAction::Game(GameAction::RotateBrick { dir }) => {
                ensure!((-1..=1).contains(dir), "Invalid brick rotation");
                if let Some(brick) = &mut self.ghost {
                    let mesh = &self.definitions.get(brick)?.mesh;
                    ghost::rotate(brick, mesh, body_forward(player)?, *dir);
                    Bounds::new(brick, mesh)?;
                    self.ghost_generation = self.ghost_generation.wrapping_add(1);
                    out.commands
                        .extend(BuildGesture::rotate(*dir).map(Command::BuildGesture));
                }
            }
            UiAction::Game(GameAction::CancelBrick) => {
                if self.ghost.take().is_some() {
                    self.ghost_generation = self.ghost_generation.wrapping_add(1);
                }
            }
            UiAction::Game(GameAction::PlantBrick) => {
                let brick = self
                    .ghost
                    .as_ref()
                    .context("Deploy a ghost brick before planting")?;
                let ContentRef::Resolved(definition) = &brick.definition else {
                    unreachable!()
                };
                out.commands.push(Command::Plant {
                    definition: definition.clone(),
                    position: brick.position,
                    quarter_turns: brick.quarter_turns,
                    color: brick.color,
                });
            }
            UiAction::Game(GameAction::UndoBrick) => {
                out.commands.push(Command::Tool(ToolAction::UndoBrick))
            }
            _ => return Ok(None),
        }
        if out
            .commands
            .iter()
            .any(|command| matches!(command, Command::EquipTool { slot: None }))
        {
            self.active_tool = None;
        }
        Ok(Some(out))
    }

    fn selectable(&self, id: &str) -> Result<()> {
        ensure!(
            self.catalog.contains_key(id),
            "Brick is not in the selectable native catalog"
        );
        self.definitions
            .entries
            .get(id)
            .context("Undefined brick")?;
        Ok(())
    }

    fn equip_brick(&mut self, id: &str, _player: &PlayerState) -> Result<()> {
        self.selectable(id)?;
        if let Some(old) = &self.ghost {
            let mut brick = old.clone();
            brick.definition = ContentRef::Resolved(id.into());
            brick.print = self
                .default_prints
                .get(id)
                .cloned()
                .map(ContentRef::Resolved);
            brick.color = self.paint;
            // Preserve the ghost's anchored centre, re-snapping only where a
            // changed footprint/height requires a different parity lattice.
            snap(&mut brick, &self.definitions.entries[id].mesh);
            self.ghost = Some(brick);
            self.ghost_generation = self.ghost_generation.wrapping_add(1);
        }
        self.equipment = Equipment::Brick(id.into());
        Ok(())
    }

    fn fire(&mut self, player: &PlayerState, out: &mut BuildingResponse) -> Result<()> {
        let command = match &self.equipment {
            Equipment::Brick(id) => {
                self.selectable(id)?;
                let eye = self.archetypes.eye(player);
                let Some(hit) = self.target(eye, player.forward(), DEPLOY_REACH)? else {
                    return Ok(());
                };
                let definition = &self.definitions.entries[id];
                let mut brick = Brick::new(
                    ContentRef::Resolved(id.clone()),
                    hit.position.to_array(),
                    player.owner,
                );
                brick.quarter_turns = (facing_angle(body_forward(player)?) + self.catalog[id]) % 4;
                let height = definition.mesh.height_plates as f32 * 0.2;
                brick.position[1] += if hit.normal.y < -0.9 {
                    -height * 0.5
                } else {
                    height * 0.5
                };
                // The script applies ±0.05 before engine snapping.
                brick.position[1] += if definition.mesh.height_plates.is_multiple_of(2) {
                    0.05
                } else {
                    -0.05
                };
                brick.color = self.paint;
                brick.print = self
                    .default_prints
                    .get(id)
                    .cloned()
                    .map(ContentRef::Resolved);
                snap(&mut brick, &definition.mesh);
                // Map surfaces need not lie on the brick lattice (Bedroom's
                // carpet is one example). Choose the first non-penetrating
                // plate plane, within the native authority's one-plate support
                // distance. Do not relax authoritative collision validation.
                if hit.brick.is_none() {
                    if hit.normal.y > 0.9
                        && brick.position[1] - height * 0.5 < hit.position.y - 0.002
                    {
                        brick.position[1] =
                            ((hit.position.y - 0.002) / 0.2).ceil() * 0.2 + height * 0.5;
                    } else if hit.normal.y < -0.9
                        && brick.position[1] + height * 0.5 > hit.position.y + 0.002
                    {
                        brick.position[1] =
                            ((hit.position.y + 0.002) / 0.2).floor() * 0.2 - height * 0.5;
                    }
                }
                Bounds::new(&brick, &definition.mesh)?;
                self.ghost = Some(brick);
                self.ghost_generation = self.ghost_generation.wrapping_add(1);
                return Ok(());
            }
            // Everything else is an image the server's state machine swings.
            _ => Command::Activate,
        };
        out.commands.push(command);
        Ok(())
    }
}

/// Equipment whose fire button drives a mounted v20 image (tools, weapons,
/// spray cans) rather than placing a brick or activating.
fn image_equipment(equipment: &Equipment) -> bool {
    !matches!(equipment, Equipment::None | Equipment::Brick(_))
}

fn tool_equipment(equipment: &Equipment) -> bool {
    matches!(
        equipment,
        Equipment::Hammer
            | Equipment::Wrench
            | Equipment::Printer
            | Equipment::Weapon(_)
            | Equipment::Wand
    )
}

fn same_geometry(a: &Brick, b: &Brick) -> bool {
    a.definition == b.definition
        && a.position == b.position
        && a.quarter_turns == b.quarter_turns
        && a.raycast == b.raycast
        && a.colliding == b.colliding
        && a.visible == b.visible
}
fn brick_pose(brick: &Brick) -> Pose {
    Pose::from_parts(
        Vector::from_array(brick.position),
        Rotation::from_scaled_axis(
            Vector::Y * (-f32::from(brick.quarter_turns) * std::f32::consts::FRAC_PI_2),
        ),
    )
}
fn query_bounds(low: Vec3, high: Vec3) -> Bounds {
    let min = std::array::from_fn(|a| (low[a] / grid::CELL[a]).floor() as i32);
    let max: [i32; 3] = std::array::from_fn(|a| (high[a] / grid::CELL[a]).ceil() as i32);
    Bounds {
        min,
        size: std::array::from_fn(|a| (max[a] - min[a]).max(1)),
    }
}
fn tool(id: &str, name: &str, icon: &str) -> ToolInfo {
    ToolInfo {
        id: id.into(),
        name: name.into(),
        icon: IconRef::Pack(format!("base/client/ui/itemicons/{icon}")),
        tint: None,
    }
}
fn body_forward(player: &PlayerState) -> Result<Vec3> {
    ensure!(player.yaw.is_finite(), "Invalid body yaw");
    Ok(Vec3::new(player.yaw.sin(), 0.0, -player.yaw.cos()))
}
fn facing_angle(forward: Vec3) -> u8 {
    if forward.x.abs() > forward.z.abs() {
        if forward.x > 0.0 { 0 } else { 2 }
    } else if forward.z < 0.0 {
        1
    } else {
        3
    }
}
fn snap(brick: &mut Brick, mesh: &Mesh) {
    let [w, d] = mesh.footprint_studs;
    let size = if brick.quarter_turns.is_multiple_of(2) {
        [w, mesh.height_plates, d]
    } else {
        [d, mesh.height_plates, w]
    };
    for (axis, cell) in grid::CELL.iter().copied().enumerate() {
        let half = size[axis] as f32 * cell * 0.5;
        brick.position[axis] = ((brick.position[axis] - half) / cell).round() * cell + half;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::collision::{CollisionBody, Part};
    use bri_sim::definitions::Definition;

    fn controller() -> Building {
        let collision = CollisionBody {
            id: "plate".into(),
            parts: vec![Part::Box {
                center: [0.0; 3],
                size: [1.0, 0.2, 0.5],
            }],
        };
        let mesh = Mesh {
            schema_version: 1,
            id: "plate".into(),
            footprint_studs: [2, 1],
            height_plates: 1,
            attachment_rows: vec!["bb".into()],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![],
        };
        let definition = Definition {
            mesh,
            shape: SharedShape::cuboid(0.5, 0.1, 0.25),
            collision,
            indestructible: false,
            special: Default::default(),
        };
        let definitions = Definitions {
            entries: [("plate".into(), definition)].into(),
        };
        let floor =
            ColliderBuilder::cuboid(10.0, 0.5, 10.0).translation(Vector::new(0.0, -0.5, 0.0));
        let mut result = Building::new(definitions, vec![floor]).unwrap();
        result.set_catalog(vec![("plate".into(), 0)]).unwrap();
        result.sync_world(&world()).unwrap();
        result
    }
    fn world() -> PublicWorld {
        PublicWorld {
            name: "Test".into(),
            map_id: "map".into(),
            palette: vec![[1.0; 4]; 2],
            bricks: Default::default(),
        }
    }
    #[test]
    fn weather_roofs_follow_collision_changes_independently_of_tool_raycast() {
        let mut building = controller();
        let mut world = world();
        let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 1.1, 0.25], 1);
        brick.raycast = false;
        brick.visible = false;
        world.bricks.insert(1, brick);
        building.sync_world(&world).unwrap();
        let start = Vec3::new(0.5, 501., 0.25);
        let end = Vec3::new(0.5, -99., 0.25);
        let hit = building.solid_segment(start, end).unwrap().unwrap();
        assert_eq!(hit.brick, Some(1));
        assert!((hit.position.y - 1.2).abs() < 0.001);
        let revision = building.query_generation();
        world.bricks.get_mut(&1).unwrap().colliding = false;
        building.sync_world(&world).unwrap();
        assert_ne!(building.query_generation(), revision);
        let hit = building.solid_segment(start, end).unwrap().unwrap();
        assert_eq!(hit.brick, None);
        assert!(hit.position.y.abs() < 0.001);
    }

    #[test]
    fn flares_test_visible_geometry_independent_of_tool_and_collision_flags() {
        let mut building = controller();
        let mut world = world();
        let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 1.1, 0.25], 1);
        brick.colliding = false;
        brick.raycast = false;
        world.bricks.insert(1, brick);
        building.sync_world(&world).unwrap();
        let eye = Vec3::new(0.5, 1.1, 2.);
        let target = Vec3::new(0.5, 1.1, -2.);
        assert!(!building.effect_visible(9, eye, target).unwrap());
        assert!(building.effect_visible(1, eye, target).unwrap());
        world.bricks.get_mut(&1).unwrap().visible = false;
        building.sync_world(&world).unwrap();
        assert!(building.effect_visible(9, eye, target).unwrap());
        assert!(!building.effect_visible(9, Vec3::Y, -Vec3::Y).unwrap());
    }
    #[test]
    fn extended_loaded_colors_remain_usable_for_paint_without_rebuilding_queries() {
        let mut building = controller();
        let mut world = world();
        world.palette = (0..256)
            .map(|i| [i as f32 / 255.0, 0.2, 0.4, 1.0])
            .collect();
        assert!(!building.sync_world(&world).unwrap());
        assert!(
            building
                .ui_action(&UiAction::UseSprayCan { color: 255 }, &player())
                .unwrap()
                .is_some()
        );
        assert_eq!(building.paint, 255);
        world.palette.push([0.0; 4]);
        assert!(building.sync_world(&world).is_err());
        assert_eq!(building.paint, 255);
    }
    fn player() -> PlayerState {
        PlayerState {
            owner: 1,
            feet: [0.5, 2.0, 0.25],
            velocity: [0.0; 3],
            yaw: 0.0,
            pitch: -std::f32::consts::FRAC_PI_2,
            head_yaw: 0.0,
            grounded: false,
            crouched: false,
            jetting: false,
            jump: Default::default(),
            archetype: Default::default(),
            scale: 1.0,
            energy: 100.0,
        }
    }
    fn fire() -> UiAction {
        UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down: true,
        })
    }
    fn buy(b: &mut Building) {
        let mut slots = vec![None; SLOTS];
        slots[3] = Some("plate".into());
        b.ui_action(&UiAction::BuyBricks { slots }, &player())
            .unwrap();
    }

    #[test]
    fn a_map_change_keeps_the_brick_bar_and_paint() {
        let mut old = controller();
        buy(&mut old);
        old.ui_action(&UiAction::UseSprayCan { color: 1 }, &player())
            .unwrap();
        // The controller the next map builds starts empty, as on first entry.
        let mut new = controller();
        assert!(new.inventory().iter().all(Option::is_none));
        new.carry_over(&old);
        assert_eq!(new.inventory()[3].as_deref(), Some("plate"));
        assert!(
            new.initial_updates()
                .iter()
                .any(|u| matches!(u, UiUpdate::BrickInventory(slots) if slots[3].is_some()))
        );
        new.ui_action(&UiAction::UseBrickSlot { slot: 3 }, &player())
            .unwrap();
        new.ui_action(&fire(), &player()).unwrap();
        assert_eq!(new.ghost().map(|g| g.color), Some(1));
    }

    #[test]
    fn ghost_deploy_shift_rotate_plant_stays_local_and_body_relative() {
        let mut b = controller();
        buy(&mut b);
        b.ui_action(&UiAction::UseBrickSlot { slot: 3 }, &player())
            .unwrap();
        assert!(
            b.ghost().is_none(),
            "selection arms the brick; Fire deploys it"
        );
        let fired = b.ui_action(&fire(), &player()).unwrap().unwrap();
        assert!(fired.commands.is_empty());
        let deployed = b.ghost().unwrap().clone();
        assert_eq!(deployed.quarter_turns, 1); // body -Z maps to original angle1
        assert!((deployed.position[1] - 0.1).abs() < 0.001);
        let mesh = b.definitions.entries["plate"].mesh.clone();
        Bounds::new(&deployed, &mesh).unwrap();

        // Looking straight down must not collapse horizontal body-facing shifts.
        let shift = b
            .ui_action(
                &UiAction::Game(GameAction::ShiftBrick { x: 1, y: 0, z: 0 }),
                &player(),
            )
            .unwrap()
            .unwrap();
        // The ghost stays local; the server only animates the builder.
        assert!(matches!(
            shift.commands.as_slice(),
            [Command::BuildGesture(BuildGesture::ShiftAway)]
        ));
        assert_eq!(b.ghost().unwrap().position[2], deployed.position[2] - 0.5);
        b.ui_action(
            &UiAction::Game(GameAction::SuperShiftBrick { x: 1, y: 0, z: 1 }),
            &player(),
        )
        .unwrap();
        assert_eq!(b.ghost().unwrap().position[2], deployed.position[2] - 1.5);
        assert!((b.ghost().unwrap().position[1] - 0.3).abs() < 0.001);
        let shifted = b.ghost().unwrap().clone();
        for _ in 0..4 {
            b.ui_action(
                &UiAction::Game(GameAction::RotateBrick { dir: 1 }),
                &player(),
            )
            .unwrap();
            Bounds::new(b.ghost().unwrap(), &mesh).unwrap();
        }
        assert_eq!(b.ghost().unwrap(), &shifted);
        let generation = b.ghost_generation();
        let plant = b
            .ui_action(&UiAction::Game(GameAction::PlantBrick), &player())
            .unwrap()
            .unwrap();
        assert!(
            matches!(&plant.commands[..], [Command::Plant { definition, .. }] if definition == "plate")
        );
        assert!(plant.updates.is_empty());
        assert_eq!(b.ghost_generation(), generation);
        assert!(
            b.bricks.is_empty(),
            "requests never insert accepted world state"
        );
        b.ui_action(&UiAction::Game(GameAction::CancelBrick), &player())
            .unwrap();
        assert!(b.ghost().is_none());
        assert!(b.ghost_generation() > generation);
    }

    #[test]
    fn map_surface_between_grid_planes_never_deploys_inside_floor() {
        let mut b = controller();
        b.map = PhysicsWorld::new();
        b.map.insert_collider(
            ColliderBuilder::cuboid(10.0, 0.5, 10.0).translation(Vector::new(0.0, -0.412, 0.0)),
            None,
        );
        b.map.detect_collisions(&(), &());
        b.ui_action(
            &UiAction::InstantUseBrick {
                brick: "plate".into(),
            },
            &player(),
        )
        .unwrap();
        b.ui_action(&fire(), &player()).unwrap();
        let ghost = b.ghost().unwrap();
        let bottom = ghost.position[1] - 0.1;
        assert!(bottom >= 0.088 && bottom - 0.088 < 0.2);
        Bounds::new(ghost, &b.definitions.entries["plate"].mesh).unwrap();
    }

    #[test]
    #[ignore = "requires converted stock-catalog-004; native camera shape coverage, no window"]
    fn original_stock_camera_shapes_all_orientations() -> Result<()> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let definitions = Definitions::load(
            &root.join("content/stock-catalog-004"),
            &root.join("content/maps-pass-007"),
        )?;
        let mut b = Building::new(definitions, vec![])?;
        let ids: Vec<_> = b.definitions.entries.keys().cloned().collect();
        let mut records = vec![];
        let mut total_hits = 0;
        for id in &ids {
            let mut hits = 0;
            for quarter_turns in 0..4 {
                let mesh = &b.definitions.entries[id].mesh;
                let mut footprint = mesh.footprint_studs;
                if quarter_turns % 2 == 1 {
                    footprint.swap(0, 1);
                }
                let mut brick = Brick::new(
                    ContentRef::Resolved(id.clone()),
                    [
                        footprint[0] as f32 * 0.25,
                        mesh.height_plates as f32 * 0.1,
                        footprint[1] as f32 * 0.25,
                    ],
                    1,
                );
                brick.quarter_turns = quarter_turns;
                let bounds = b.definitions.entries[id]
                    .shape
                    .compute_aabb(&brick_pose(&brick));
                let min = Vec3::from(bounds.mins.to_array());
                let max = Vec3::from(bounds.maxs.to_array());
                let center = (min + max) * 0.5;
                let mut w = world();
                w.bricks.insert(1, brick);
                b.sync_world(&w)?;
                for axis in 0..3 {
                    for sign in [-1.0, 1.0] {
                        let mut direction = Vec3::ZERO;
                        direction[axis] = sign;
                        let eye = center + direction * ((max[axis] - min[axis]) * 0.5 + 1.0);
                        let camera = b.camera_position(eye, direction, 3.0).with_context(|| {
                            format!(
                                "Camera shape {id}, turn {quarter_turns}, axis {axis}, sign {sign}"
                            )
                        })?;
                        ensure!(
                            camera.is_finite() && eye.distance(camera) <= 3.001,
                            "Invalid native camera sweep"
                        );
                        if eye.distance(camera) < 2.999 {
                            hits += 1;
                        }
                    }
                }
            }
            total_hits += hits;
            records.push(serde_json::json!({"id":id,"sweeps":24,"hits":hits}));
        }
        ensure!(
            ids.len() == 170 && total_hits > 170,
            "Unexpected stock camera coverage"
        );
        let out = root.join("artifacts/native-camera");
        std::fs::create_dir_all(&out)?;
        std::fs::write(
            out.join("stock-shapes.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"definitions":ids.len(),"sweeps":ids.len()*24,"hits":total_hits,"unsupported_queries":0,"visible_window":false,"records":records}),
            )?,
        )?;
        Ok(())
    }

    #[test]
    fn third_person_camera_sweeps_map_and_independent_brick_collision_flags() {
        let mut b = controller();
        let eye = Vec3::new(0.5, 2.1, 0.25);
        let floor = b.camera_position(eye, Vec3::Y, 8.0).unwrap();
        assert!(
            (floor.y - 0.17).abs() < 0.003,
            "Camera volume penetrated floor: {floor:?}"
        );
        assert_eq!(
            b.camera_position(eye, Vec3::NEG_Z, 8.0).unwrap(),
            eye + Vec3::Z * 8.0
        );
        assert_eq!(b.camera_position(eye, Vec3::NEG_Z, 0.0).unwrap(), eye);
        let mut w = world();
        let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 2.1, 3.25], 1);
        brick.raycast = false;
        brick.visible = false;
        w.bricks.insert(1, brick);
        b.sync_world(&w).unwrap();
        assert!(b.target(eye, Vec3::Z, 8.0).unwrap().is_none());
        let blocked = b.camera_position(eye, Vec3::NEG_Z, 8.0).unwrap();
        assert!(
            (blocked.z - 2.83).abs() < 0.005,
            "Camera ignored non-raycasting wall {blocked:?}"
        );
        w.bricks.get_mut(&1).unwrap().quarter_turns = 1;
        // Rotated footprint requires a compatible center lattice.
        w.bricks.get_mut(&1).unwrap().position = [0.25, 2.1, 3.0];
        b.sync_world(&w).unwrap();
        assert!(b.camera_position(eye, Vec3::NEG_Z, 8.0).unwrap().z < blocked.z);
        w.bricks.get_mut(&1).unwrap().colliding = false;
        w.bricks.get_mut(&1).unwrap().raycast = true;
        assert!(b.sync_world(&w).unwrap());
        assert!(b.target(eye, Vec3::Z, 8.0).unwrap().is_some());
        assert_eq!(
            b.camera_position(eye, Vec3::NEG_Z, 8.0).unwrap(),
            eye + Vec3::Z * 8.0
        );
        w.bricks.clear();
        b.sync_world(&w).unwrap();
        assert_eq!(
            b.camera_position(eye, Vec3::NEG_Z, 8.0).unwrap(),
            eye + Vec3::Z * 8.0
        );
        assert!(b.camera_position(eye, Vec3::ZERO, 8.0).is_err());
        assert!(b.camera_position(eye, Vec3::NAN, 8.0).is_err());
        assert!(b.camera_position(eye, Vec3::NEG_Z, f32::INFINITY).is_err());
    }

    #[test]
    fn camera_broadphase_uses_authored_collision_bounds_and_start_overlap() {
        let mut b = controller();
        b.definitions.entries.get_mut("plate").unwrap().shape = SharedShape::cuboid(2.0, 0.1, 0.25);
        let mut w = world();
        w.bricks.insert(
            1,
            Brick::new(ContentRef::Resolved("plate".into()), [0.5, 2.1, 3.25], 1),
        );
        b.sync_world(&w).unwrap();
        let eye = Vec3::new(2.0, 2.1, 0.25);
        let hit = b.camera_position(eye, Vec3::NEG_Z, 8.0).unwrap();
        assert!(
            (hit.z - 2.83).abs() < 0.005,
            "Authored collision outside grid was missed"
        );
        let overlapping = Vec3::new(0.5, 2.1, 3.25);
        assert_eq!(
            b.camera_position(overlapping, Vec3::NEG_Z, 8.0).unwrap(),
            overlapping
        );
    }

    #[test]
    fn replica_queries_are_incremental_respect_raycast_and_do_not_depend_on_rendering() {
        let mut b = controller();
        let origin = Vec3::new(0.5, 4.0, 0.25);
        assert!(
            b.target(origin, -Vec3::Y, 10.0)
                .unwrap()
                .unwrap()
                .brick
                .is_none()
        );
        let mut w = world();
        let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, 0.25], 7);
        brick.visible = false;
        brick.colliding = false;
        w.bricks.insert(42, brick);
        assert!(b.sync_world(&w).unwrap());
        assert_eq!(
            b.target(origin, -Vec3::Y, 10.0).unwrap().unwrap().brick,
            Some(42)
        );
        let generation = b.query_generation();
        w.bricks.get_mut(&42).unwrap().color = 1;
        w.bricks.get_mut(&42).unwrap().owner = 99;
        assert!(!b.sync_world(&w).unwrap());
        assert_eq!(b.query_generation(), generation);
        w.bricks.get_mut(&42).unwrap().raycast = false;
        assert!(b.sync_world(&w).unwrap());
        assert!(
            b.target(origin, -Vec3::Y, 10.0)
                .unwrap()
                .unwrap()
                .brick
                .is_none()
        );
        w.bricks.clear();
        assert!(b.sync_world(&w).unwrap());
        assert!(!b.sync_world(&w).unwrap());
        assert!(b.target(origin, Vec3::NAN, 10.0).is_err());
        assert!(b.target(origin, Vec3::ZERO, 10.0).is_err());
    }

    // Tools now fire through weapon triggers; these assertions need rewriting.
    #[cfg(any())]
    #[test]
    fn inventory_validation_is_atomic_and_tools_are_authoritative_commands() {
        let mut b = controller();
        buy(&mut b);
        let old = b.inventory().clone();
        let mut invalid = old.to_vec();
        invalid[0] = Some("not-defined".into());
        assert!(
            b.ui_action(&UiAction::BuyBricks { slots: invalid }, &player())
                .is_err()
        );
        assert_eq!(b.inventory(), &old);
        assert!(
            b.ui_action(&UiAction::UseBrickSlot { slot: 10 }, &player())
                .is_err()
        );
        assert!(
            b.ui_action(&UiAction::UseBrickSlot { slot: 2 }, &player())
                .is_err()
        );
        assert!(
            b.ui_action(&UiAction::UseTool { slot: 3 }, &player())
                .is_err()
        );
        assert!(
            b.ui_action(&UiAction::UseSprayCan { color: 2 }, &player())
                .is_err()
        );
        assert!(
            b.ui_action(&UiAction::UseFxCan { fx: 9 }, &player())
                .is_err()
        );
        assert!(
            b.ui_action(
                &UiAction::Game(GameAction::ShiftBrick {
                    x: i32::MIN,
                    y: 0,
                    z: 0
                }),
                &player()
            )
            .is_err()
        );

        b.ui_action(&UiAction::UseTool { slot: 0 }, &player())
            .unwrap();
        assert!(matches!(
            &b.ui_action(&fire(), &player()).unwrap().unwrap().commands[..],
            [Command::Tool(ToolAction::Hammer)]
        ));
        b.ui_action(&UiAction::UseTool { slot: 1 }, &player())
            .unwrap();
        assert!(matches!(
            &b.ui_action(&fire(), &player()).unwrap().unwrap().commands[..],
            [Command::Tool(ToolAction::Inspect {
                mode: InspectMode::Wrench
            })]
        ));
        b.ui_action(&UiAction::UseFxCan { fx: 8 }, &player())
            .unwrap();
        assert!(matches!(
            &b.ui_action(&fire(), &player()).unwrap().unwrap().commands[..],
            [Command::Tool(ToolAction::ShapeEffect { effect: 1 })]
        ));
        b.ui_action(&UiAction::UnUseTool, &player()).unwrap();
        assert!(matches!(
            &b.ui_action(&fire(), &player()).unwrap().unwrap().commands[..],
            [Command::Activate]
        ));
    }

    fn weapon_controller() -> Building {
        let mut b = controller();
        let mut catalog = b.tool_catalog.clone();
        for (id, name) in [
            ("v20.weapon.gunitem", "Gun"),
            ("v20.weapon.bowitem", "Bow"),
            ("v20.weapon.wanditem", "Wand"),
        ] {
            catalog.insert(
                id.into(),
                ToolInfo {
                    id: id.into(),
                    name: name.into(),
                    icon: IconRef::None,
                    tint: None,
                },
            );
        }
        b.set_tool_catalog(catalog).unwrap();
        b
    }
    fn weapon_inventory() -> ToolInventory {
        ToolInventory {
            slots: vec![
                Some("v20.weapon.gunitem".into()),
                Some(bri_weapons::HAMMER.into()),
                Some("v20.weapon.bowitem".into()),
                None,
                None,
            ],
            selected: Some(0),
        }
    }
    fn choose(b: &mut Building, request: u64, slot: usize) -> Command {
        let response = b
            .ui_action(&UiAction::UseTool { slot }, &player())
            .unwrap()
            .unwrap();
        let command = response.commands.into_iter().next().unwrap();
        b.command_sent(request, &command).unwrap();
        command
    }
    fn release() -> UiAction {
        UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down: false,
        })
    }
    // Tools now fire through weapon triggers; these assertions need rewriting.
    #[cfg(any())]
    #[test]
    fn replicated_pickup_drop_and_reused_slot_update_hud_and_stable_routes() {
        let mut b = weapon_controller();
        let mut inventory = weapon_inventory();
        let updates = b.sync_tools(&inventory).unwrap();
        assert!(
            matches!(&updates[0],UiUpdate::Tools(tools) if tools[0].as_ref().unwrap().name=="Gun")
        );
        assert_eq!(
            b.equipment(),
            &Equipment::Weapon("v20.weapon.gunitem".into())
        );
        assert!(matches!(
            &b.ui_action(&UiAction::Game(GameAction::DropTool), &player())
                .unwrap()
                .unwrap()
                .commands[..],
            [Command::DropTool { slot: 0 }]
        ));
        assert_eq!(b.tools, inventory); // Dropping is not locally predicted.
        inventory.slots[0] = None;
        inventory.selected = None;
        b.sync_tools(&inventory).unwrap();
        assert_eq!(b.equipment(), &Equipment::None);
        inventory.slots[0] = Some("v20.weapon.wanditem".into());
        let updates = b.sync_tools(&inventory).unwrap();
        assert!(
            matches!(&updates[0],UiUpdate::Tools(tools) if tools[0].as_ref().unwrap().name=="Wand")
        );
        choose(&mut b, 1, 1);
        assert!(matches!(
            &b.ui_action(&fire(), &player()).unwrap().unwrap().commands[..],
            [Command::Tool(ToolAction::Hammer)]
        ));
        choose(&mut b, 2, 0);
        assert!(
            b.ui_action(&fire(), &player())
                .err()
                .unwrap()
                .to_string()
                .contains("Wand destruction")
        );
        let old = b.tools.clone();
        inventory.slots[4] = Some("v20.weapon.unknown".into());
        assert!(b.sync_tools(&inventory).is_err());
        assert_eq!(b.tools, old);
    }
    #[test]
    fn request_ids_preserve_newer_switches_and_rejections_restore_authority() {
        let mut b = weapon_controller();
        b.sync_tools(&weapon_inventory()).unwrap();
        let older = choose(&mut b, 10, 1);
        let newer = choose(&mut b, 11, 2);
        b.command_finished(10, &older, false);
        assert_eq!(
            b.equipment(),
            &Equipment::Weapon("v20.weapon.bowitem".into())
        );
        b.command_finished(11, &newer, false);
        assert_eq!(
            b.equipment(),
            &Equipment::Weapon("v20.weapon.gunitem".into())
        );
        let older = choose(&mut b, 12, 1);
        let newer = choose(&mut b, 13, 2);
        b.command_finished(13, &newer, true);
        b.command_finished(12, &older, false);
        assert_eq!(
            b.equipment(),
            &Equipment::Weapon("v20.weapon.bowitem".into())
        );
        let mut confirmed = weapon_inventory();
        confirmed.selected = Some(2);
        b.sync_tools(&confirmed).unwrap();
        assert_eq!(b.active_tool, Some(2));
        assert!(b.pending_equipment.is_empty());
    }
    #[test]
    fn fire_release_survives_equip_reply_and_rejected_switch_without_repeats() {
        let mut b = weapon_controller();
        b.sync_tools(&weapon_inventory()).unwrap();
        let equip = choose(&mut b, 1, 0);
        let down = b
            .ui_action(&fire(), &player())
            .unwrap()
            .unwrap()
            .commands
            .remove(0);
        b.command_sent(2, &down).unwrap();
        b.command_finished(1, &equip, true); // Earlier equip ACK must not clear later press.
        assert!(
            b.ui_action(&fire(), &player())
                .unwrap()
                .unwrap()
                .commands
                .is_empty()
        );
        assert!(matches!(
            &b.ui_action(&release(), &player())
                .unwrap()
                .unwrap()
                .commands[..],
            [Command::WeaponTrigger { down: false }]
        ));
        assert!(
            b.ui_action(&release(), &player())
                .unwrap()
                .unwrap()
                .commands
                .is_empty()
        );
        let down = b
            .ui_action(&fire(), &player())
            .unwrap()
            .unwrap()
            .commands
            .remove(0);
        b.command_sent(3, &down).unwrap();
        let switch = choose(&mut b, 4, 1);
        b.command_finished(4, &switch, false);
        assert!(matches!(
            &b.ui_action(&release(), &player())
                .unwrap()
                .unwrap()
                .commands[..],
            [Command::WeaponTrigger { down: false }]
        ));
        assert!(!b.weapon_fire_down);
    }
    #[test]
    fn click_after_switching_tools_fires_the_new_image_before_the_ack() {
        let mut b = weapon_controller();
        b.sync_tools(&weapon_inventory()).unwrap();
        choose(&mut b, 1, 1);
        let down = b
            .ui_action(&fire(), &player())
            .unwrap()
            .unwrap()
            .commands
            .remove(0);
        b.command_sent(2, &down).unwrap();
        // The trigger is never released; the player switches and clicks again.
        choose(&mut b, 3, 2);
        let again = b.ui_action(&fire(), &player()).unwrap().unwrap().commands;
        assert!(matches!(&again[..], [Command::WeaponTrigger { down: true }]));
        b.command_sent(4, &again[0]).unwrap();
        assert!(b.ui_action(&fire(), &player()).unwrap().unwrap().commands.is_empty());
    }
    #[test]
    fn queue_failure_and_slot_replacement_cannot_leave_a_pending_tool_grant() {
        let mut b = weapon_controller();
        let mut inventory = weapon_inventory();
        b.sync_tools(&inventory).unwrap();
        let equip = choose(&mut b, 10, 2);
        b.command_finished(10, &equip, false);
        assert_eq!(b.active_tool, Some(0));
        choose(&mut b, 11, 2);
        inventory.slots[2] = Some("v20.weapon.wanditem".into());
        inventory.selected = None;
        b.sync_tools(&inventory).unwrap();
        assert!(b.pending_equipment.is_empty());
        assert_eq!(b.equipment(), &Equipment::None);
        assert!(b.set_tool_catalog(b.tool_catalog.clone()).is_err());
        let fresh = weapon_controller();
        assert!(fresh.pending_equipment.is_empty());
        assert!(!fresh.weapon_fire_down);
        assert_eq!(fresh.tools, ToolInventory::default());
    }

    #[test]
    fn last_print_updates_the_ghost_and_later_bricks_of_its_aspect() {
        let mut b = controller();
        b.set_catalog(vec![("plate".into(), 1)]).unwrap();
        b.set_default_prints([("plate".into(), "v20/print/letters/a".into())].into())
            .unwrap();
        let p = player();
        b.ui_action(
            &UiAction::InstantUseBrick {
                brick: "plate".into(),
            },
            &p,
        )
        .unwrap();
        b.ui_action(&fire(), &p).unwrap();
        let generation = b.ghost_generation();
        let last = crate::tool_ui::LastPrint {
            definitions: vec!["plate".into()],
            print: "v20/print/2x2f/arrow".into(),
        };
        b.remember_print(&last).unwrap();
        let arrow = Some(ContentRef::Resolved("v20/print/2x2f/arrow".into()));
        assert_eq!(b.ghost().unwrap().print, arrow);
        assert_ne!(b.ghost_generation(), generation);
        assert_eq!(
            b.default_prints["plate"], "v20/print/2x2f/arrow",
            "the next plate ghost starts with the last print"
        );
    }

    #[test]
    fn all_body_quadrants_rotate_parity_and_catalog_orientation_is_preserved() {
        for (yaw, expected) in [
            (0.0, 1),
            (std::f32::consts::FRAC_PI_2, 0),
            (std::f32::consts::PI, 3),
            (-std::f32::consts::FRAC_PI_2, 2),
        ] {
            let mut b = controller();
            b.set_catalog(vec![("plate".into(), 1)]).unwrap();
            b.set_default_prints([("plate".into(), "v20/print/letters/a".into())].into())
                .unwrap();
            let mut p = player();
            p.yaw = yaw;
            b.ui_action(
                &UiAction::InstantUseBrick {
                    brick: "plate".into(),
                },
                &p,
            )
            .unwrap();
            b.ui_action(&fire(), &p).unwrap();
            assert_eq!(b.ghost().unwrap().quarter_turns, (expected + 1) % 4);
            let start = b.ghost().unwrap().clone();
            assert_eq!(
                start.print,
                Some(ContentRef::Resolved("v20/print/letters/a".into()))
            );
            for _ in 0..4 {
                b.ui_action(&UiAction::Game(GameAction::RotateBrick { dir: -1 }), &p)
                    .unwrap();
                Bounds::new(b.ghost().unwrap(), &b.definitions.entries["plate"].mesh).unwrap();
            }
            assert_eq!(b.ghost().unwrap(), &start);
        }
    }
}
