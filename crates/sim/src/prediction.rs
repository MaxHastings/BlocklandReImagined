//! Client-side movement prediction. The client runs the same player motor as the
//! server, one input per 120 Hz tick, against a local mirror of the collision
//! world. Authoritative poses acknowledge the last input the server consumed;
//! the predictor restores that state and replays the inputs still in flight.
use crate::{
    definitions::Definitions,
    player::{MotionEvents, MoveInput, Player, PlayerState, PlayerTuning},
    simulation::{MAP_TAG, brick_collider},
};
use anyhow::{Result, ensure};
use bri_content::water::Water;
use bri_world::{Brick, BrickId, ContentRef};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::{BTreeMap, VecDeque};

/// Two seconds of unacknowledged input at 120 Hz. Older inputs are discarded;
/// the next authoritative pose simply replays whatever history remains.
pub const INPUT_HISTORY: usize = 240;
/// Position difference (native units) treated as float noise, not error.
const NOISE: f32 = 1e-3;

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
    waters: Vec<Water>,
    bricks: BTreeMap<BrickId, (ColliderHandle, Geometry)>,
}
impl CollisionMirror {
    pub fn new(definitions: Definitions, map: Vec<ColliderBuilder>, waters: Vec<Water>) -> Self {
        let mut physics = bri_physics::new_world();
        for collider in map {
            physics.insert_collider(collider.user_data(MAP_TAG), None);
        }
        physics.detect_collisions(&(), &());
        Self {
            physics,
            definitions,
            waters,
            bricks: BTreeMap::new(),
        }
    }
    /// Incrementally mirror replicated brick collision. Returns whether any
    /// collider changed. Unknown definitions reject the update atomically.
    pub fn sync(&mut self, bricks: &BTreeMap<BrickId, Brick>) -> Result<bool> {
        let mut changed = Vec::new();
        for (id, brick) in bricks {
            let geometry = Geometry::of(brick);
            if self.bricks.get(id).is_none_or(|(_, old)| *old != geometry) {
                let definition = self.definitions.get(brick)?;
                changed.push((*id, brick_collider(brick, definition, *id), geometry));
            }
        }
        let removed: Vec<_> = self
            .bricks
            .keys()
            .filter(|id| !bricks.contains_key(id))
            .copied()
            .collect();
        if changed.is_empty() && removed.is_empty() {
            return Ok(false);
        }
        for id in removed {
            if let Some((handle, _)) = self.bricks.remove(&id) {
                self.physics
                    .remove_collider(handle);
            }
        }
        for (id, collider, geometry) in changed {
            if let Some((handle, _)) = self.bricks.remove(&id) {
                self.physics.remove_collider(handle);
            }
            let handle = self.physics.insert_collider(collider, None);
            self.bricks.insert(id, (handle, geometry));
        }
        self.physics.detect_collisions(&(), &());
        Ok(true)
    }
    pub fn physics(&self) -> &PhysicsWorld {
        &self.physics
    }
}

pub struct Predictor {
    world: CollisionMirror,
    player: Player,
    pending: VecDeque<(u64, MoveInput)>,
    sequence: u64,
    acknowledged: u64,
    server_tick: Option<u64>,
}
impl Predictor {
    /// Begin predicting from an authoritative state (normally the join pose).
    pub fn new(mut world: CollisionMirror, state: PlayerState) -> Result<Self> {
        let player = Player::attach(&mut world.physics, state, PlayerTuning::default())?;
        Ok(Self {
            world,
            player,
            pending: VecDeque::new(),
            sequence: 0,
            acknowledged: 0,
            server_tick: None,
        })
    }
    pub fn state(&self) -> &PlayerState {
        self.player.state()
    }
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn world(&self) -> &CollisionMirror {
        &self.world
    }
    pub fn sync_world(&mut self, bricks: &BTreeMap<BrickId, Brick>) -> Result<bool> {
        self.world.sync(bricks)
    }
    /// Advance one fixed tick. Returns the input's sequence number, which the
    /// caller sends on the movement channel, and the local motion events.
    pub fn step(&mut self, input: MoveInput) -> Result<(u64, MotionEvents)> {
        input.validate()?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Input sequence exhausted"))?;
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
        self.player.restore(&mut self.world.physics, state)?;
        while self.pending.front().is_some_and(|(sequence, _)| *sequence <= ack) {
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
            && Vec3::from(predicted.velocity).distance(Vec3::from(corrected.velocity)) < NOISE * 10.0
            && predicted.grounded == corrected.grounded
            && predicted.crouched == corrected.crouched
        {
            self.player.restore(&mut self.world.physics, predicted)?;
            return Ok(Some(Vec3::ZERO));
        }
        Ok(Some(Vec3::from(predicted.feet) - Vec3::from(corrected.feet)))
    }
    /// Server-initiated relocation (respawn, teleport): discard in-flight inputs.
    pub fn teleport(&mut self, tick: u64, ack: u64, state: PlayerState) -> Result<()> {
        self.player.restore(&mut self.world.physics, state)?;
        self.pending.clear();
        self.server_tick = Some(tick);
        self.acknowledged = ack.min(self.sequence);
        Ok(())
    }
}
