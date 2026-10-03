//! Client-side movement prediction. The client runs the same player motor as the
//! server, one input per 120 Hz tick, against a local mirror of the collision
//! world. Authoritative poses acknowledge the last input the server consumed;
//! the predictor restores that state and replays the inputs still in flight.
use crate::{
    archetype::Archetypes,
    definitions::{Definitions, brick_water},
    player::{MotionEvents, MoveInput, Player, PlayerState},
    simulation::{MAP_TAG, brick_collider},
};
use anyhow::{Result, ensure};
use bri_content::water::Water;
use bri_world::{Brick, BrickId, ContentRef};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Two seconds of unacknowledged input at 120 Hz. Older inputs are discarded;
/// the next authoritative pose simply replays whatever history remains.
pub const INPUT_HISTORY: usize = 240;
/// Position difference (native units) treated as float noise, not error.
const NOISE: f32 = 1e-3;

/// Client copies of map collision drop the colliders of smashed shapes
/// (`Session::broken_shapes`) so they stop blocking movement and building.
#[derive(Default)]
pub struct BrokenShapes {
    handles: Vec<ColliderHandle>,
    /// Scene node and its range of map colliders.
    shapes: Vec<(u32, std::ops::Range<usize>)>,
    applied: BTreeSet<u32>,
}
impl BrokenShapes {
    /// `handles` are the map colliders in `NativeMap::colliders` order.
    pub fn new(handles: Vec<ColliderHandle>, shapes: &[crate::map::Breakable]) -> Self {
        Self {
            handles,
            shapes: shapes
                .iter()
                .map(|s| (s.node, s.colliders.clone()))
                .collect(),
            applied: BTreeSet::new(),
        }
    }
    /// Replace the breakable shapes. Call before any `apply`.
    pub fn set_shapes(&mut self, shapes: &[crate::map::Breakable]) {
        self.shapes = shapes
            .iter()
            .map(|s| (s.node, s.colliders.clone()))
            .collect();
        self.applied.clear();
    }
    /// Match `physics` to the replicated broken set. Returns whether any
    /// collider changed.
    pub fn apply(&mut self, physics: &mut PhysicsWorld, broken: &BTreeSet<u32>) -> Result<bool> {
        if &self.applied == broken {
            return Ok(false);
        }
        let mut changed = false;
        for (node, colliders) in &self.shapes {
            let solid = !broken.contains(node);
            if solid == self.applied.contains(node) && !colliders.is_empty() {
                crate::simulation::set_enabled(physics, &self.handles, colliders.clone(), solid)?;
                changed = true;
            }
        }
        self.applied = broken.clone();
        Ok(changed)
    }
}

#[derive(PartialEq)]
struct Geometry {
    definition: ContentRef,
    position: [f32; 3],
    quarter_turns: u8,
    colliding: bool,
}
impl Geometry {
    fn of(brick: &Brick) -> Self {
        Self {
            definition: brick.definition.clone(),
            position: brick.position,
            quarter_turns: brick.quarter_turns,
            colliding: brick.colliding,
        }
    }
}

/// Map and brick collision built exactly like the server's `Simulation`.
pub struct CollisionMirror {
    physics: PhysicsWorld,
    definitions: Definitions,
    map_waters: Vec<Water>,
    /// Map liquids followed by water bricks, as the server's motor sees them.
    waters: Vec<Water>,
    brick_waters: BTreeMap<BrickId, Water>,
    /// Changes whenever the liquids change; unique across mirrors, so a
    /// cache keyed by it never matches another map's liquids.
    water_generation: u64,
    /// Each mirrored brick: its own collider (sensors), or None for a solid
    /// brick, which is a part of its chunk's collider, as on the server.
    bricks: BTreeMap<BrickId, (Option<ColliderHandle>, Geometry)>,
    chunks: crate::chunks::Chunks,
    terrain: Option<crate::map::TerrainStream>,
    broken: BrokenShapes,
    /// Removed bricks' colliders (see `parking`).
    parked: crate::parking::Parking,
    /// Linked bricks, as the host sees them.
    links: crate::links::Links,
}
fn next_water_generation() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
impl CollisionMirror {
    pub fn new(definitions: Definitions, map: Vec<ColliderBuilder>, waters: Vec<Water>) -> Self {
        let mut physics = bri_physics::new_world();
        let handles = map
            .into_iter()
            .map(|collider| physics.insert_collider(collider.user_data(MAP_TAG), None))
            .collect();
        bri_physics::detect_collisions(&mut physics);
        Self {
            physics,
            definitions,
            map_waters: waters.clone(),
            waters,
            brick_waters: BTreeMap::new(),
            water_generation: next_water_generation(),
            bricks: BTreeMap::new(),
            terrain: None,
            broken: BrokenShapes::new(handles, &[]),
            chunks: Default::default(),
            parked: Default::default(),
            links: Default::default(),
        }
    }
    /// The map's breakable shapes (`NativeMap::breakables`).
    pub fn set_breakables(&mut self, shapes: &[crate::map::Breakable]) {
        self.broken.set_shapes(shapes);
    }
    /// Drop the collision of smashed shapes (`Session::broken_shapes`).
    pub fn set_broken_shapes(&mut self, broken: &BTreeSet<u32>) -> Result<bool> {
        self.broken.apply(&mut self.physics, broken)
    }
    /// Incrementally mirror replicated brick collision. Returns whether any
    /// collider changed. Unknown definitions reject the update atomically.
    pub fn sync(&mut self, bricks: &bri_world::Bricks) -> Result<bool> {
        let removed = self
            .bricks
            .keys()
            .filter(|id| !bricks.contains_key(*id))
            .copied();
        let candidates: Vec<_> = bricks.keys().copied().chain(removed).collect();
        self.sync_changes(bricks, candidates)
    }
    /// Mirror only `candidates`, the bricks a replica change log says were
    /// added, edited or removed; others are assumed unchanged. Same result as
    /// `sync` when the log is complete, without walking the whole world.
    pub fn sync_changes(
        &mut self,
        bricks: &bri_world::Bricks,
        candidates: impl IntoIterator<Item = BrickId>,
    ) -> Result<bool> {
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for id in candidates {
            if self.links.may_link(id, bricks.get(&id), &self.definitions) {
                self.links.touch(id);
            }
            let Some(brick) = bricks.get(&id) else {
                if self.bricks.contains_key(&id) {
                    removed.push(id);
                }
                continue;
            };
            let geometry = Geometry::of(brick);
            if self.bricks.get(&id).is_none_or(|(_, old)| *old != geometry) {
                changed.push((id, geometry));
            }
        }
        let relinked = self.links.flush(bricks, &self.definitions);
        if changed.is_empty() && removed.is_empty() {
            return Ok(relinked);
        }
        // Take away the old collision (a chunk part, waking bodies resting
        // on it, or an own collider for parking), then give the new.
        let mut gone = Vec::new();
        for id in removed.iter().chain(changed.iter().map(|(id, _)| id)) {
            let Some((handle, old)) = self.bricks.remove(id) else {
                continue;
            };
            match handle {
                Some(handle) => gone.push(handle),
                None => {
                    self.chunks.remove(*id, old.position);
                    if let ContentRef::Resolved(name) = &old.definition
                        && let Some(definition) = self.definitions.entries.get(name)
                    {
                        let aabb = definition.shape.compute_aabb(&crate::simulation::grid_pose(
                            old.position,
                            old.quarter_turns,
                        ));
                        crate::parking::wake_resting(&mut self.physics, aabb);
                    }
                }
            }
        }
        for id in removed {
            self.brick_waters.remove(&id);
        }
        self.parked.remove(&mut self.physics, &gone);
        for (id, geometry) in changed {
            let brick = &bricks[&id];
            let definition = self.definitions.get(brick)?;
            let handle = if crate::simulation::solid(brick, definition) {
                self.chunks.insert(id, brick.position);
                None
            } else {
                Some(
                    self.physics
                        .insert_collider(brick_collider(brick, definition, id), None),
                )
            };
            self.bricks.insert(id, (handle, geometry));
            match brick_water(brick, definition) {
                Some(water) => self.brick_waters.insert(id, water),
                None => self.brick_waters.remove(&id),
            };
        }
        let definitions = &self.definitions;
        self.chunks
            .flush(&mut self.physics, &mut self.parked, |id| {
                let brick = bricks.get(&id)?;
                let definition = definitions.get(brick).ok()?;
                Some(crate::simulation::brick_shape(brick, definition))
            });
        self.waters = self
            .map_waters
            .iter()
            .chain(self.brick_waters.values())
            .cloned()
            .collect();
        self.water_generation = next_water_generation();
        bri_physics::detect_collisions(&mut self.physics);
        Ok(true)
    }
    /// Changes when `tinted_waters` may give different liquids (before paint).
    pub fn water_generation(&self) -> u64 {
        self.water_generation
    }
    /// Every liquid with its v20 `waterColor`: each water brick in the colour
    /// it is painted in `palette`, then map water.
    pub fn tinted_waters(
        &self,
        bricks: &bri_world::Bricks,
        palette: &[[f32; 4]],
    ) -> Vec<crate::water::TintedWater> {
        let bricks = self.brick_waters.iter().map(|(id, w)| {
            let paint = bricks
                .get(id)
                .and_then(|b| palette.get(usize::from(b.color)))
                .copied()
                .unwrap_or([1.0; 4]);
            crate::water::TintedWater {
                water: w.clone(),
                color: crate::water::brick_water_color(paint),
                brick: true,
            }
        });
        let map = self.map_waters.iter().map(|w| crate::water::TintedWater {
            water: w.clone(),
            color: crate::water::MAP_WATER_COLOR,
            brick: false,
        });
        bricks.chain(map).collect()
    }
    /// Linked bricks and their openings.
    pub fn links(&self) -> &crate::links::Links {
        &self.links
    }
    pub fn physics(&self) -> &PhysicsWorld {
        &self.physics
    }
    /// Camera volume in the same open/closed portal geometry as the walking
    /// motor. In particular, a wall behind a live opening is cut away, while
    /// the frame and the destination room still stop the camera.
    pub fn portal_camera_position(&self, eye: Vec3, forward: Vec3, distance: f32) -> Result<Vec3> {
        ensure!(
            distance.is_finite() && (0.0..=40.0).contains(&distance),
            "Invalid portal camera distance"
        );
        let hit = self.portal_camera_hit(eye, forward, distance)?;
        let travel = hit.map_or(distance, |(travel, _)| (travel - 0.02).max(0.0));
        Ok(eye - forward.normalize() * travel)
    }
    /// First camera-volume hit in one portal-clipped space. The returned
    /// surface normal lets vehicle cameras keep their authored wall back-off.
    /// Their ray extends past the eye and can exceed the player boom's 40 units.
    pub fn portal_camera_hit(
        &self,
        eye: Vec3,
        forward: Vec3,
        distance: f32,
    ) -> Result<Option<(f32, Vec3)>> {
        use bri_motor::torque::{Box3, Soup};
        use rapier3d::parry::query::{ShapeCastOptions, cast_shapes};
        ensure!(
            eye.is_finite()
                && forward.is_finite()
                && forward.length_squared() > 0.1
                && distance.is_finite()
                && (0.0..=2000.0).contains(&distance),
            "Invalid portal camera sweep"
        );
        let backward = -forward.normalize();
        let end = eye + backward * distance;
        let radius = 0.15;
        let region = Box3 {
            min: eye.min(end) - Vec3::splat(radius),
            max: eye.max(end) + Vec3::splat(radius),
        };
        let query = self
            .physics
            .query_pipeline_with_filter(QueryFilter::default().exclude_sensors());
        let mut soup = Soup::gather(&query, &self.physics.bodies, region, eye, &self.chunks);
        // A doorway has two faces on one plane. The body's overlap margin
        // admits both very close to it; a camera point occupies one side,
        // so only that side may replace the backing geometry.
        let mut passages = self.links.passages().clone();
        passages.list.retain(|p| p.side(eye) >= 0.0);
        soup.open_passages(
            &query,
            &self.physics.bodies,
            &passages,
            eye,
            region,
            &self.chunks,
        );
        let shape = Ball::new(radius);
        let options = ShapeCastOptions {
            max_time_of_impact: distance,
            ..Default::default()
        };
        let velocity = Vector::from_array(backward.to_array());
        let mut nearest = None;
        // Soup vertices are relative to the camera, preserving precision far
        // from the origin. Fan triangles retain the clipped polygon's edges.
        for poly in &soup.polys {
            let points = soup.verts(poly);
            for i in 1..points.len().saturating_sub(1) {
                let triangle = Triangle::new(
                    Vector::from_array(points[0].to_array()),
                    Vector::from_array(points[i].to_array()),
                    Vector::from_array(points[i + 1].to_array()),
                );
                if let Some(hit) = cast_shapes(
                    &Pose::IDENTITY,
                    velocity,
                    &shape,
                    &Pose::IDENTITY,
                    Vector::ZERO,
                    &triangle,
                    options,
                )
                .map_err(|_| anyhow::anyhow!("Unsupported portal camera polygon"))?
                    && hit.time_of_impact < distance
                    && nearest.is_none_or(|(travel, _)| hit.time_of_impact < travel)
                {
                    nearest = Some((hit.time_of_impact, Vec3::from_array(hit.normal2.to_array())));
                }
            }
        }
        Ok(nearest)
    }
    /// Stream the map's terrain collision around the predicted body, exactly
    /// like the server's `Simulation`.
    pub fn attach_terrain(
        &mut self,
        fields: Vec<std::sync::Arc<bri_content::terrain_field::TerrainField>>,
    ) -> Result<()> {
        let mut stream = crate::map::TerrainStream::new(fields, MAP_TAG, Vec::new())?;
        stream.update(&mut self.physics);
        self.terrain = Some(stream);
        Ok(())
    }
    fn stream_terrain(&mut self) {
        if let Some(terrain) = &mut self.terrain {
            terrain.update(&mut self.physics);
        }
    }
}

/// What the walking motor gets from an input. A tool in hand that takes
/// the jet button for its own action (an image with a `jet` command) keeps
/// the press from jetting: the server still sees the press as the tool's
/// trigger. Host and prediction both use this, so neither jets.
pub fn motor_input(input: MoveInput, tool_takes_jet: bool) -> MoveInput {
    MoveInput {
        jet: input.jet && !tool_takes_jet,
        ..input
    }
}

/// The vehicle a client drives, as it predicts it.
pub struct DriveSpawn {
    /// The vehicle's identity, definition and scale; its transform is
    /// replaced by the first replicated motion.
    pub spawn: bri_vehicles::Spawn,
    pub seat: usize,
    /// The driver's steering prefs: strafe steering off, auto-return off.
    pub prefs: (bool, bool),
}
/// The driven vehicle's copy in the collision mirror. Torque predicts the
/// object a client controls by running its moves on the client
/// (`GameConnection` moves, `Vehicle::processTick` on the ghost) and
/// corrects it from the server's state; this does the same with the host's
/// own vehicle code and one input per 120 Hz tick.
struct Drive {
    world: bri_vehicles::VehiclesWorld,
    id: bri_vehicles::VehicleId,
    occupant: bri_vehicles::Occupant,
    prefs: (bool, bool),
    /// A player-type mount the rider controls (`Some(horse)`): its move maps
    /// to the mount's controls, not a driver's.
    actor: Option<bool>,
    /// Inputs the host has not yet shown in a pose, oldest first.
    pending: VecDeque<(u64, MoveInput)>,
    /// The newest input a pose included: the mouse turn of the first
    /// pending input is measured from it, as the host measures it.
    base: Option<MoveInput>,
    restored_tick: Option<u64>,
    /// The predicted body before and after the newest step.
    previous: bri_vehicles::Transform,
    current: bri_vehicles::Transform,
}
impl Drive {
    fn step(
        &mut self,
        mirror: &mut CollisionMirror,
        input: &MoveInput,
        last: Option<&MoveInput>,
    ) -> Result<()> {
        let last = last.map_or((input.yaw, input.pitch), |l| (l.yaw, l.pitch));
        let controls = match self.actor {
            Some(horse) => crate::session::actor_controls(input, false, horse),
            None => crate::session::driver_controls(input, last, false, self.prefs),
        };
        self.world
            .set_controls(self.occupant.owner, self.occupant.id, controls)?;
        self.world.pre_step(&mut mirror.physics, &mirror.waters)?;
        let before = self.world.centre(&mirror.physics, self.id);
        mirror.physics.step();
        self.world.post_step(&mut mirror.physics)?;
        self.world.drain_intents();
        self.previous = self.current.clone();
        // Through an opening of a linked brick, as the host carries it; the
        // step it was drawn from is carried too, so it never slides across.
        let after = self.world.centre(&mirror.physics, self.id);
        if let (Some(before), Some(after)) = (before, after)
            && let (_, Some(carry)) = mirror.links.passages().travel(before, after)
        {
            self.world.carry(&mut mirror.physics, self.id, &carry)?;
            let (_, turn, _) = carry.to_scale_rotation_translation();
            let at = carry.transform_point3(glam::Vec3::from(self.previous.position));
            self.previous = bri_vehicles::Transform {
                position: at.to_array(),
                rotation: (turn * glam::Quat::from_array(self.previous.rotation))
                    .normalize()
                    .to_array(),
            };
        }
        self.current = self.body(mirror)?;
        Ok(())
    }
    fn body(&self, mirror: &CollisionMirror) -> Result<bri_vehicles::Transform> {
        self.world
            .vehicle_snapshot(&mirror.physics, self.id)
            .map(|v| v.shown_transform())
            .ok_or_else(|| anyhow::anyhow!("Predicted vehicle is gone"))
    }
}
pub struct Predictor {
    world: CollisionMirror,
    player: Player,
    /// The vehicle this client drives, predicted like the body is.
    drive: Option<Drive>,
    /// The host's archetype table, from its checkpoint.
    archetypes: Archetypes,
    pending: VecDeque<(u64, MoveInput)>,
    /// What the motor ran for each pending input ([`motor_input`]), so a
    /// replay runs exactly what was predicted.
    motor: VecDeque<MoveInput>,
    /// The tool in hand takes the jet button.
    tool_jet: bool,
    sequence: u64,
    acknowledged: u64,
    server_tick: Option<u64>,
    /// Other players' bodies at their latest poses: the motor bumps into
    /// and pushes off them as it does on the host.
    others: BTreeMap<u64, Player>,
}
impl Predictor {
    /// Begin predicting from an authoritative state (normally the join pose).
    pub fn new(
        mut world: CollisionMirror,
        state: PlayerState,
        archetypes: Archetypes,
    ) -> Result<Self> {
        let tuning = archetypes.tuning(state.archetype, state.scale);
        let player = Player::attach(&mut world.physics, state, tuning)?;
        world.stream_terrain();
        Ok(Self {
            world,
            player,
            drive: None,
            archetypes,
            pending: VecDeque::new(),
            motor: VecDeque::new(),
            tool_jet: false,
            sequence: 0,
            acknowledged: 0,
            server_tick: None,
            others: BTreeMap::new(),
        })
    }
    pub fn state(&self) -> &PlayerState {
        self.player.state()
    }
    /// Where the predicted body's middle is above its feet.
    pub fn player_middle(&self) -> f32 {
        self.player.middle()
    }
    /// The predicted body's motor constants (its archetype at its scale).
    pub fn tuning(&self) -> &crate::player::PlayerTuning {
        self.player.tuning()
    }
    /// Whether the tool in hand takes the jet button ([`motor_input`]).
    pub fn set_tool_jet(&mut self, takes: bool) {
        self.tool_jet = takes;
    }
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Keep numbering inputs after an earlier predictor's last one (a new
    /// map): the server ignores sequences it has already seen.
    pub fn continue_after(&mut self, sequence: u64) {
        if self.pending.is_empty() {
            self.sequence = self.sequence.max(sequence);
            self.acknowledged = self.acknowledged.max(sequence);
        }
    }
    pub fn world(&self) -> &CollisionMirror {
        &self.world
    }
    pub fn sync_world(&mut self, bricks: &bri_world::Bricks) -> Result<bool> {
        self.world.sync(bricks)
    }
    pub fn set_broken_shapes(&mut self, broken: &BTreeSet<u32>) -> Result<bool> {
        self.world.set_broken_shapes(broken)
    }
    /// `sync_world` restricted to the bricks a replica change log names.
    pub fn sync_world_changes(
        &mut self,
        bricks: &bri_world::Bricks,
        candidates: impl IntoIterator<Item = BrickId>,
    ) -> Result<bool> {
        self.world.sync_changes(bricks, candidates)
    }
    /// Mirror the other players the host collides with (alive and on foot)
    /// at these states; anyone left out stops colliding.
    pub fn set_others<'a>(
        &mut self,
        states: impl IntoIterator<Item = &'a PlayerState>,
    ) -> Result<()> {
        let own = self.player.state().owner;
        let mut seen = BTreeSet::new();
        for state in states {
            if state.owner == own || !seen.insert(state.owner) {
                continue;
            }
            let tuning = self.archetypes.tuning(state.archetype, state.scale);
            let physics = &mut self.world.physics;
            match self.others.get_mut(&state.owner) {
                Some(other) => other.restore(physics, state.clone(), tuning)?,
                None => {
                    let other = Player::attach(physics, state.clone(), tuning)?;
                    self.others.insert(state.owner, other);
                }
            }
            self.others[&state.owner].place_now(&mut self.world.physics);
        }
        let gone: Vec<_> = self
            .others
            .keys()
            .filter(|owner| !seen.contains(*owner))
            .copied()
            .collect();
        for owner in gone {
            if let Some(other) = self.others.remove(&owner) {
                other.despawn(&mut self.world.physics);
            }
        }
        bri_physics::detect_collisions(&mut self.world.physics);
        Ok(())
    }
    /// Advance one fixed tick. Returns the input's sequence number, which the
    /// caller sends on the movement channel, and the local motion events.
    pub fn step(&mut self, input: MoveInput) -> Result<(u64, MotionEvents)> {
        input.validate()?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Input sequence exhausted"))?;
        self.world.stream_terrain();
        let motor = motor_input(input, self.tool_jet);
        let events = self.player.step_through(
            &mut self.world.physics,
            motor,
            &self.world.waters,
            &self.world.chunks,
            self.world.links.passages(),
        )?;
        if self.pending.len() == INPUT_HISTORY {
            self.pending.pop_front();
            self.motor.pop_front();
        }
        self.pending.push_back((sequence, input));
        self.motor.push_back(motor);
        self.sequence = sequence;
        Ok((sequence, events))
    }
    /// Record an input without running the walking motor (the player is
    /// seated in a vehicle; the server turns inputs into vehicle controls).
    /// A vehicle this client drives takes the input here, as on the host.
    pub fn record(&mut self, input: MoveInput) -> Result<u64> {
        input.validate()?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Input sequence exhausted"))?;
        if self.pending.len() == INPUT_HISTORY {
            self.pending.pop_front();
            self.motor.pop_front();
        }
        self.pending.push_back((sequence, input));
        self.motor.push_back(input);
        self.sequence = sequence;
        if let Some(drive) = &mut self.drive {
            let last = drive.pending.back().map(|(_, i)| *i).or(drive.base);
            if drive.pending.len() == INPUT_HISTORY {
                drive.base = drive.pending.pop_front().map(|(_, i)| i);
            }
            drive.pending.push_back((sequence, input));
            self.world.stream_terrain();
            if let Err(error) = drive.step(&mut self.world, &input, last.as_ref()) {
                self.stop_drive(Some(&error));
            }
        }
        Ok(sequence)
    }
    /// Stop predicting the driven vehicle: its copy leaves the mirror and
    /// the rider is solid again. Never fails; a copy that is already gone
    /// has nothing left to remove. `why` is logged when prediction failed,
    /// and the vehicle is then shown at the host's poses.
    fn stop_drive(&mut self, why: Option<&anyhow::Error>) {
        let Some(mut old) = self.drive.take() else {
            return;
        };
        if let Some(why) = why {
            eprintln!("Vehicle prediction stopped: {why:#} (showing the host's poses)");
        }
        let _ = old.world.remove(&mut self.world.physics, old.id);
        old.world.drain_intents();
        self.player.set_solid(&mut self.world.physics, true);
        bri_physics::detect_collisions(&mut self.world.physics);
    }
    /// Whether a driven vehicle is being predicted.
    pub fn driving(&self) -> bool {
        self.drive.is_some()
    }
    /// Start predicting the vehicle this client drives from its replicated
    /// motion, or stop (`None`). A player-type mount runs on its own motor.
    pub fn drive(
        &mut self,
        vehicle: Option<(bri_vehicles::Pack, DriveSpawn, bri_vehicles::Motion)>,
    ) -> Result<()> {
        self.stop_drive(None);
        let Some((pack, setup, motion)) = vehicle else {
            return Ok(());
        };
        let mut world = bri_vehicles::VehiclesWorld::new(pack)?;
        world.set_prediction(true);
        let mut spawn = setup.spawn;
        spawn.transform = motion.transform.clone();
        spawn.spawn_id = None;
        spawn.respawn_ticks = None;
        let id = spawn.id;
        let actor = world
            .definition(&spawn.definition)
            .map(|d| {
                d.is_actor()
                    .then_some(d.family == bri_vehicles::Family::Horse)
            })
            .ok_or_else(|| anyhow::anyhow!("Unknown vehicle {}", spawn.definition))?;
        world.spawn(&mut self.world.physics, spawn)?;
        // The predicted body rides, sized as the host sizes it.
        let occupant = crate::session::rider(self.player.state().owner, self.player.tuning());
        let seated = (|| -> Result<()> {
            bri_physics::detect_collisions(&mut self.world.physics);
            let seat = world
                .seat_position(&self.world.physics, id, setup.seat)
                .ok_or_else(|| anyhow::anyhow!("No such seat"))?;
            world.mount(&self.world.physics, id, setup.seat, occupant, seat)?;
            world.restore_motion(&mut self.world.physics, id, &motion)
        })();
        if let Err(error) = seated {
            let _ = world.remove(&mut self.world.physics, id);
            bri_physics::detect_collisions(&mut self.world.physics);
            return Err(error);
        }
        world.drain_intents();
        // The host's seated riders are sensors, so its vehicle never hits them.
        self.player.set_solid(&mut self.world.physics, false);
        self.drive = Some(Drive {
            world,
            id,
            occupant,
            prefs: setup.prefs,
            actor,
            pending: VecDeque::new(),
            base: None,
            restored_tick: None,
            previous: motion.transform.clone(),
            current: motion.transform,
        });
        Ok(())
    }
    /// The driven vehicle's steering prefs changed.
    pub fn set_drive_prefs(&mut self, prefs: (bool, bool)) {
        if let Some(drive) = &mut self.drive {
            drive.prefs = prefs;
        }
    }
    /// Correct the driven vehicle from the host's pose at `tick`, which
    /// includes this client's inputs up to `driver_input`: restore it and
    /// replay the inputs since. Returns the predicted body before the
    /// correction, for the renderer to blend from, or `None` when the pose
    /// is not newer than one already applied.
    pub fn drive_pose(
        &mut self,
        tick: u64,
        driver_input: u64,
        motion: &bri_vehicles::Motion,
    ) -> Result<Option<bri_vehicles::Transform>> {
        let Some(drive) = &mut self.drive else {
            return Ok(None);
        };
        if drive.restored_tick.is_some_and(|old| old >= tick) {
            return Ok(None);
        }
        drive.restored_tick = Some(tick);
        while drive
            .pending
            .front()
            .is_some_and(|(sequence, _)| *sequence <= driver_input)
        {
            drive.base = drive.pending.pop_front().map(|(_, i)| i);
        }
        let before = drive.current.clone();
        let replayed = (|| -> Result<()> {
            drive
                .world
                .restore_motion(&mut self.world.physics, drive.id, motion)?;
            drive.current = motion.transform.clone();
            drive.previous = motion.transform.clone();
            let inputs: Vec<MoveInput> = drive.pending.iter().map(|(_, i)| *i).collect();
            let mut last = drive.base;
            for input in &inputs {
                drive.step(&mut self.world, input, last.as_ref())?;
                last = Some(*input);
            }
            Ok(())
        })();
        if let Err(error) = replayed {
            self.stop_drive(Some(&error));
            return Ok(None);
        }
        Ok(Some(before))
    }
    /// The driven vehicle before and after its newest predicted step.
    pub fn driven(&self) -> Option<(u64, &bri_vehicles::Transform, &bri_vehicles::Transform)> {
        self.drive
            .as_ref()
            .map(|d| (d.id.0, &d.previous, &d.current))
    }
    /// The most recent inputs, oldest first, for redundant datagrams.
    pub fn recent(&self, count: usize) -> impl Iterator<Item = &(u64, MoveInput)> {
        self.pending
            .iter()
            .skip(self.pending.len().saturating_sub(count))
    }
    /// Apply an authoritative pose. Returns the visual discontinuity
    /// (old predicted feet minus corrected feet) so the renderer can blend it
    /// out, or `None` when the pose is older than one already applied.
    pub fn reconcile(&mut self, tick: u64, ack: u64, state: PlayerState) -> Result<Option<Vec3>> {
        if self.server_tick.is_some_and(|old| old >= tick) {
            return Ok(None);
        }
        ensure!(
            ack <= self.sequence && state.owner == self.player.state().owner,
            "Invalid movement acknowledgement"
        );
        if ack < self.acknowledged {
            return Ok(None);
        }
        let predicted = self.player.state().clone();
        let tuning = self.archetypes.tuning(state.archetype, state.scale);
        self.player
            .restore(&mut self.world.physics, state, tuning)?;
        self.world.stream_terrain();
        while self
            .pending
            .front()
            .is_some_and(|(sequence, _)| *sequence <= ack)
        {
            self.pending.pop_front();
            self.motor.pop_front();
        }
        for input in &self.motor {
            self.player.step_through(
                &mut self.world.physics,
                *input,
                &self.world.waters,
                &self.world.chunks,
                self.world.links.passages(),
            )?;
        }
        self.server_tick = Some(tick);
        self.acknowledged = ack;
        let corrected = self.player.state();
        // The mirror is not bit-identical to the server world (collider order,
        // other bodies), so replays can differ by float noise. Keep the local
        // prediction's motion through sub-millimeter differences rather than
        // jittering; everything else the host decided (a rope tied to the
        // player, say) is taken as it is.
        if Vec3::from(predicted.feet).distance(Vec3::from(corrected.feet)) < NOISE
            && Vec3::from(predicted.velocity).distance(Vec3::from(corrected.velocity))
                < NOISE * 10.0
            && predicted.grounded == corrected.grounded
            && predicted.crouched == corrected.crouched
            // A new archetype or scale always takes the authoritative body.
            && predicted.archetype == corrected.archetype
            && predicted.scale == corrected.scale
        {
            let mut kept = corrected.clone();
            kept.feet = predicted.feet;
            kept.velocity = predicted.velocity;
            let tuning = self.player.tuning().clone();
            self.player.restore(&mut self.world.physics, kept, tuning)?;
            return Ok(Some(Vec3::ZERO));
        }
        Ok(Some(
            Vec3::from(predicted.feet) - Vec3::from(corrected.feet),
        ))
    }
    /// Server-initiated relocation (respawn, teleport): discard in-flight inputs.
    pub fn teleport(&mut self, tick: u64, ack: u64, state: PlayerState) -> Result<()> {
        let tuning = self.archetypes.tuning(state.archetype, state.scale);
        self.player
            .restore(&mut self.world.physics, state, tuning)?;
        self.world.stream_terrain();
        self.pending.clear();
        self.motor.clear();
        self.server_tick = Some(tick);
        self.acknowledged = ack.min(self.sequence);
        Ok(())
    }
}
