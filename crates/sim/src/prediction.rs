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
    bricks: BTreeMap<BrickId, (ColliderHandle, Geometry)>,
    terrain: Option<crate::map::TerrainStream>,
    broken: BrokenShapes,
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
            bricks: BTreeMap::new(),
            terrain: None,
            broken: BrokenShapes::new(handles, &[]),
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
            let Some(brick) = bricks.get(&id) else {
                if self.bricks.contains_key(&id) {
                    removed.push(id);
                }
                continue;
            };
            let geometry = Geometry::of(brick);
            if self.bricks.get(&id).is_none_or(|(_, old)| *old != geometry) {
                let definition = self.definitions.get(brick)?;
                changed.push((
                    id,
                    brick_collider(brick, definition, id),
                    geometry,
                    brick_water(brick, definition),
                ));
            }
        }
        if changed.is_empty() && removed.is_empty() {
            return Ok(false);
        }
        for id in removed {
            if let Some((handle, _)) = self.bricks.remove(&id) {
                self.physics.remove_collider(handle);
            }
            self.brick_waters.remove(&id);
        }
        for (id, collider, geometry, water) in changed {
            if let Some((handle, _)) = self.bricks.remove(&id) {
                self.physics.remove_collider(handle);
            }
            let handle = self.physics.insert_collider(collider, None);
            self.bricks.insert(id, (handle, geometry));
            match water {
                Some(water) => self.brick_waters.insert(id, water),
                None => self.brick_waters.remove(&id),
            };
        }
        self.waters = self
            .map_waters
            .iter()
            .chain(self.brick_waters.values())
            .cloned()
            .collect();
        bri_physics::detect_collisions(&mut self.physics);
        Ok(true)
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
    pub fn physics(&self) -> &PhysicsWorld {
        &self.physics
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

pub struct Predictor {
    world: CollisionMirror,
    player: Player,
    /// The host's archetype table, from its checkpoint.
    archetypes: Archetypes,
    pending: VecDeque<(u64, MoveInput)>,
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
            archetypes,
            pending: VecDeque::new(),
            sequence: 0,
            acknowledged: 0,
            server_tick: None,
            others: BTreeMap::new(),
        })
    }
    pub fn state(&self) -> &PlayerState {
        self.player.state()
    }
    /// The predicted body's motor constants (its archetype at its scale).
    pub fn tuning(&self) -> &crate::player::PlayerTuning {
        self.player.tuning()
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
        let events =
            self.player
                .step_in_water(&mut self.world.physics, input, &self.world.waters)?;
        if self.pending.len() == INPUT_HISTORY {
            self.pending.pop_front();
        }
        self.pending.push_back((sequence, input));
        self.sequence = sequence;
        Ok((sequence, events))
    }
    /// Record an input without running the walking motor (the player is
    /// seated in a vehicle; the server turns inputs into vehicle controls).
    pub fn record(&mut self, input: MoveInput) -> Result<u64> {
        input.validate()?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Input sequence exhausted"))?;
        if self.pending.len() == INPUT_HISTORY {
            self.pending.pop_front();
        }
        self.pending.push_back((sequence, input));
        self.sequence = sequence;
        Ok(sequence)
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
        }
        for (_, input) in &self.pending {
            self.player
                .step_in_water(&mut self.world.physics, *input, &self.world.waters)?;
        }
        self.server_tick = Some(tick);
        self.acknowledged = ack;
        let corrected = self.player.state();
        // The mirror is not bit-identical to the server world (collider order,
        // other bodies), so replays can differ by float noise. Keep the local
        // prediction through sub-millimeter differences rather than jittering.
        if Vec3::from(predicted.feet).distance(Vec3::from(corrected.feet)) < NOISE
            && Vec3::from(predicted.velocity).distance(Vec3::from(corrected.velocity))
                < NOISE * 10.0
            && predicted.grounded == corrected.grounded
            && predicted.crouched == corrected.crouched
            // A new archetype or scale always takes the authoritative body.
            && predicted.archetype == corrected.archetype
            && predicted.scale == corrected.scale
        {
            let tuning = self.player.tuning().clone();
            self.player
                .restore(&mut self.world.physics, predicted, tuning)?;
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
        self.server_tick = Some(tick);
        self.acknowledged = ack.min(self.sequence);
        Ok(())
    }
}
