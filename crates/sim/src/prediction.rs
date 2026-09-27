//! Local fixed-tick movement prediction using the same motor as the server.
use crate::player::{MotionEvents, MoveInput, Player, PlayerState};
use anyhow::{Result, ensure};
use glam::Vec3;
use rapier3d::prelude::PhysicsWorld;
use std::collections::VecDeque;
pub struct Predictor {
    player: Player,
    pending: VecDeque<(u64, MoveInput)>,
    last_sequence: u64,
    last_ack: u64,
    last_server_tick: Option<u64>,
}
impl Predictor {
    pub fn new(player: Player) -> Self {
        Self {
            player,
            pending: VecDeque::new(),
            last_sequence: 0,
            last_ack: 0,
            last_server_tick: None,
        }
    }
    pub fn state(&self) -> &PlayerState {
        self.player.state()
    }
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    /// One input sequence per predicted 120 Hz tick. Send that sequence on the
    /// movement channel; render frames may contain zero or several ticks.
    pub fn step(
        &mut self,
        physics: &mut PhysicsWorld,
        sequence: u64,
        input: MoveInput,
    ) -> Result<MotionEvents> {
        input.validate()?;
        ensure!(
            sequence == self.last_sequence.checked_add(1).unwrap_or(0) && self.pending.len() < 240,
            "Prediction history requires resynchronization"
        );
        let events = self.player.step(physics, input)?;
        self.player.synchronize_pose(physics);
        self.pending.push_back((sequence, input));
        self.last_sequence = sequence;
        Ok(events)
    }
    /// Returns a visual correction offset for the renderer to decay. Collision
    /// and subsequent commands immediately use the corrected simulation state.
    pub fn reconcile(
        &mut self,
        physics: &mut PhysicsWorld,
        tick: u64,
        ack: u64,
        state: PlayerState,
    ) -> Result<Vec3> {
        if self.last_server_tick.is_some_and(|old| old >= tick) {
            return Ok(Vec3::ZERO);
        }
        ensure!(
            ack >= self.last_ack && ack <= self.last_sequence,
            "Invalid movement acknowledgement"
        );
        let old = Vec3::from(self.player.state().feet);
        self.player.restore(physics, state)?;
        while self
            .pending
            .front()
            .is_some_and(|(sequence, _)| *sequence <= ack)
        {
            self.pending.pop_front();
        }
        for (_, input) in &self.pending {
            self.player.step(physics, *input)?;
            self.player.synchronize_pose(physics);
        }
        self.last_server_tick = Some(tick);
        self.last_ack = ack;
        Ok(old - Vec3::from(self.player.state().feet))
    }
}
