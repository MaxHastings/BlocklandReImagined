//! Player motion presentation.
//!
//! The local player is predicted: every 120 Hz tick samples the controls, runs
//! the same motor as the server against a mirrored collision world and sends
//! the input. Authoritative poses acknowledge consumed inputs; the predictor
//! restores and replays the rest, and any visual discontinuity is blended out.
//! Rendering interpolates between the last two fixed ticks, so camera motion
//! is smooth at any frame rate.
//!
//! Remote players are rendered slightly in the past, interpolating between
//! buffered authoritative poses on the server's tick timeline.
use crate::crouch::{CROUCH_SECONDS, CrouchThread};
use crate::network::View;
use anyhow::Result;
use bri_net::protocol::{POSE_INTERVAL, PublicWorld};
use bri_sim::{
    player::{MoveInput, PlayerState},
    prediction::{CollisionMirror, Predictor},
};
use bri_world::OwnerId;
use glam::Vec3;
use std::{
    collections::{BTreeMap, VecDeque},
    f32::consts::PI,
    sync::Arc,
};

const TICK: f32 = bri_physics::FIXED_DT;
const TICK_RATE: f64 = 120.0;
/// At most this many fixed ticks per frame; a longer stall drops time rather
/// than freezing the frame to catch up.
const MAX_STEPS: u32 = 12;
/// Remote players render this many server ticks behind the newest estimate:
/// three pose intervals absorbs one lost datagram plus ordinary jitter.
const INTERPOLATION_TICKS: f64 = (POSE_INTERVAL * 3) as f64;
/// Bounded extrapolation beyond the newest remote pose, in ticks.
const EXTRAPOLATION_TICKS: f64 = 6.0;
const REMOTE_HISTORY: usize = 32;
/// Corrections larger than this are teleports (respawn, spawn) and snap.
const SNAP_DISTANCE: f32 = 4.0;
/// Visual correction decay rate per second.
const CORRECTION_RATE: f32 = 14.0;

#[derive(Default)]
pub struct Motion {
    mirror: Option<CollisionMirror>,
    predictor: Option<Predictor>,
    /// The replica world the collision mirror matches, with its change log
    /// and revision.
    mirrored: Option<(Arc<PublicWorld>, Arc<crate::network::WorldLog>, u64)>,
    owner: OwnerId,
    accumulator: f32,
    previous: Option<PlayerState>,
    correction: Vec3,
    /// The local view follows v20's crouch thread, including its re-crouch snap.
    crouch: CrouchThread,
    eye_height: Option<f32>,
    remotes: BTreeMap<OwnerId, VecDeque<bri_net::protocol::Pose>>,
    /// Estimated `server_tick - local_seconds * TICK_RATE`.
    clock_offset: Option<f64>,
    local_seconds: f64,
    newest_local_tick: u64,
    presented: BTreeMap<OwnerId, PlayerState>,
    /// Each player's latest simulated tick, uninterpolated.
    ticked: BTreeMap<OwnerId, PlayerState>,
    local_eye: Option<Vec3>,
    mounted: bool,
    /// Last input sequence sent before a map change reset prediction.
    sent_sequence: u64,
    /// Fastest speed into a surface since `take_impact` (`Player::updatePos`
    /// `bd`), for the ground impact camera shake.
    impact: f32,
}

impl Motion {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Collision world for prediction, prepared off the UI thread with the map.
    pub fn install(&mut self, mirror: CollisionMirror) {
        let sent = self.predictor.as_ref().map_or(self.sent_sequence, |p| p.sequence());
        self.reset();
        self.sent_sequence = sent;
        self.mirror = Some(mirror);
    }
    pub fn predicting(&self) -> bool {
        self.predictor.is_some()
    }
    /// The predicted player's fastest hit on a surface since the last call,
    /// and its archetype.
    pub fn take_impact(&mut self) -> Option<(f32, bri_sim::archetype::ArchetypeId)> {
        let speed = std::mem::take(&mut self.impact);
        let archetype = self.predictor.as_ref()?.state().archetype;
        (speed > 0.0).then_some((speed, archetype))
    }
    /// Seated players, and players driving a package entity, do not walk:
    /// inputs are recorded and sent, and the authoritative pose is shown
    /// instead of a prediction.
    pub fn set_mounted(&mut self, mounted: bool) {
        self.mounted = mounted;
    }
    /// Estimated current server tick (for interpolating other entities).
    pub fn server_tick(&self) -> Option<f64> {
        self.clock_offset
            .map(|offset| self.local_seconds * TICK_RATE + offset)
    }
    /// Replace a presented state (riders follow their rendered vehicle seat).
    /// A `yaw` locks the rider facing the seat; passengers keep their own.
    pub fn override_presented(
        &mut self,
        owner: OwnerId,
        feet: Vec3,
        yaw: Option<f32>,
        up: Vec3,
        velocity: Vec3,
        local: bool,
    ) {
        if let Some(state) = self.presented.get_mut(&owner) {
            state.feet = feet.to_array();
            if let Some(yaw) = yaw {
                state.yaw = yaw;
            }
            state.velocity = velocity.to_array();
            state.grounded = true;
            state.crouched = false;
            state.jetting = false;
        }
        if local {
            // Seated eye height in the original sit pose, along the seat's up.
            let up = if up.is_finite() && up.length_squared() > 0.5 {
                up.normalize()
            } else {
                Vec3::Y
            };
            self.local_eye = Some(feet + up * 1.6);
        }
    }
    /// Ingest the latest replicated view: brick collision, the local
    /// authoritative pose and remote pose history.
    pub fn observe(&mut self, view: &View) -> Result<()> {
        self.owner = view.owner;
        if self
            .mirrored
            .as_ref()
            .is_none_or(|(old, _, _)| !Arc::ptr_eq(old, &view.world))
        {
            // Sync only the bricks the replica logged since the mirrored
            // revision; without that history, compare every brick.
            let changes = self
                .mirrored
                .as_ref()
                .filter(|(_, log, _)| Arc::ptr_eq(log, &view.world_log))
                .and_then(|(_, log, revision)| log.between(*revision, view.world_revision));
            let bricks = &view.world.bricks;
            match (&mut self.predictor, &mut self.mirror, changes) {
                (Some(predictor), _, Some(changes)) => {
                    predictor.sync_world_changes(bricks, changes.bricks)?;
                }
                (Some(predictor), _, None) => {
                    predictor.sync_world(bricks)?;
                }
                (None, Some(mirror), Some(changes)) => {
                    mirror.sync_changes(bricks, changes.bricks)?;
                }
                (None, Some(mirror), None) => {
                    mirror.sync(bricks)?;
                }
                (None, None, _) => {}
            }
            self.mirrored = Some((
                view.world.clone(),
                view.world_log.clone(),
                view.world_revision,
            ));
        }
        match (&mut self.predictor, &mut self.mirror) {
            (Some(predictor), _) => {
                predictor.set_broken_shapes(&view.broken_shapes)?;
            }
            (None, Some(mirror)) => {
                mirror.set_broken_shapes(&view.broken_shapes)?;
            }
            (None, None) => {}
        }
        for (owner, pose) in &view.poses {
            self.observe_clock(pose.tick);
            if *owner == view.owner {
                self.observe_local(pose, &view.archetypes)?;
            } else {
                let history = self.remotes.entry(*owner).or_default();
                if history.back().is_none_or(|last| last.tick < pose.tick) {
                    if history.len() == REMOTE_HISTORY {
                        history.pop_front();
                    }
                    history.push_back(pose.clone());
                }
            }
        }
        self.remotes.retain(|owner, _| view.poses.contains_key(owner));
        // The host's motor collides with every other living body; corpses
        // are sensors there.
        if let Some(predictor) = &mut self.predictor {
            predictor.set_others(
                self.remotes
                    .iter()
                    .filter(|(owner, _)| view.vitals.get(owner).is_none_or(|v| v.alive))
                    .filter_map(|(_, history)| history.back())
                    .map(|pose| &pose.player),
            )?;
        }
        Ok(())
    }
    fn observe_clock(&mut self, tick: u64) {
        if tick <= self.newest_local_tick {
            return;
        }
        self.newest_local_tick = tick;
        let sample = tick as f64 - self.local_seconds * TICK_RATE;
        self.clock_offset = Some(match self.clock_offset {
            // Later-than-expected arrivals are delay, earlier ones mean the
            // estimate lagged. Track the least-delayed arrivals, drifting down
            // slowly so a single fast packet does not remove all buffering.
            Some(offset) if sample < offset => offset + (sample - offset) * 0.02,
            _ => sample,
        });
    }
    fn observe_local(
        &mut self,
        pose: &bri_net::protocol::Pose,
        archetypes: &bri_sim::archetype::Archetypes,
    ) -> Result<()> {
        if self.mounted
            && let Some(predictor) = &mut self.predictor
        {
            predictor.teleport(pose.tick, pose.acknowledged_input, pose.player.clone())?;
            self.previous = Some(predictor.state().clone());
            self.correction = Vec3::ZERO;
            return Ok(());
        }
        if let Some(predictor) = &mut self.predictor {
            if let Some(offset) =
                predictor.reconcile(pose.tick, pose.acknowledged_input, pose.player.clone())?
            {
                self.correction += offset;
                if self.correction.length() > SNAP_DISTANCE {
                    self.correction = Vec3::ZERO;
                    self.previous = Some(predictor.state().clone());
                }
            }
        } else if let Some(mirror) = self.mirror.take() {
            let mut predictor = Predictor::new(mirror, pose.player.clone(), archetypes.clone())?;
            predictor.continue_after(self.sent_sequence);
            self.previous = Some(predictor.state().clone());
            self.predictor = Some(predictor);
        }
        Ok(())
    }
    /// Advance local time and run fixed prediction ticks. Returns the newest
    /// input sequence and the recent inputs to send when any tick ran.
    pub fn advance(
        &mut self,
        seconds: f32,
        input: MoveInput,
        redundancy: usize,
    ) -> Result<Option<(u64, Vec<MoveInput>)>> {
        let seconds = if seconds.is_finite() {
            seconds.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.local_seconds += f64::from(seconds);
        self.correction *= (-CORRECTION_RATE * seconds).exp();
        if self.correction.length_squared() < 1e-8 {
            self.correction = Vec3::ZERO;
        }
        let Some(predictor) = &mut self.predictor else {
            return Ok(None);
        };
        self.accumulator += seconds;
        let mut steps = 0;
        while self.accumulator >= TICK && steps < MAX_STEPS {
            self.accumulator -= TICK;
            if self.mounted {
                predictor.record(input)?;
            } else {
                self.previous = Some(predictor.state().clone());
                let (_, events) = predictor.step(input)?;
                for (_, speed) in events.hits {
                    self.impact = self.impact.max(speed);
                }
            }
            steps += 1;
        }
        if steps == MAX_STEPS {
            self.accumulator = self.accumulator.min(TICK);
        }
        let tuning = predictor.tuning().clone();
        self.crouch
            .update(predictor.state().crouched, seconds, CROUCH_SECONDS);
        self.eye_height = Some(tuning.eye_height(self.crouch.eye_fraction(CROUCH_SECONDS)));
        if steps == 0 {
            return Ok(None);
        }
        // Every input this frame produced plus recent history: a slow frame
        // that ran more ticks than the redundancy window loses none of them.
        let recent: Vec<_> = predictor
            .recent(redundancy.max(steps as usize))
            .map(|(_, i)| *i)
            .collect();
        Ok(Some((predictor.sequence(), recent)))
    }
    /// Compute presented states for this frame. The local player uses its
    /// interpolated prediction; remotes interpolate buffered poses.
    pub fn present(
        &mut self,
        view: &View,
        yaw: f32,
        pitch: f32,
        head_yaw: f32,
    ) -> &BTreeMap<OwnerId, PlayerState> {
        self.presented.clear();
        self.ticked.clear();
        self.local_eye = None;
        if let Some(predictor) = &self.predictor {
            let current = predictor.state();
            self.ticked.insert(view.owner, current.clone());
            let previous = self.previous.as_ref().unwrap_or(current);
            let alpha = (self.accumulator / TICK).clamp(0.0, 1.0);
            let mut state = blend(previous, current, alpha);
            let feet = Vec3::from(state.feet) + self.correction;
            state.feet = feet.to_array();
            state.yaw = yaw;
            state.pitch = pitch;
            state.head_yaw = head_yaw;
            let tuning = predictor.tuning().clone();
            let height = self
                .eye_height
                .unwrap_or_else(|| state.eye(&tuning).y - feet.y);
            let ahead = Vec3::new(yaw.sin(), 0.0, -yaw.cos()) * tuning.eye_forward;
            self.local_eye = Some(feet + Vec3::Y * height + ahead);
            self.presented.insert(view.owner, state);
        } else if let Some(pose) = view.poses.get(&view.owner) {
            self.presented.insert(view.owner, pose.player.clone());
        }
        let render_tick = self
            .clock_offset
            .map(|offset| self.local_seconds * TICK_RATE + offset - INTERPOLATION_TICKS);
        for (owner, pose) in &view.poses {
            if *owner == view.owner {
                continue;
            }
            let (state, ticked) = match (self.remotes.get(owner), render_tick) {
                (Some(history), Some(tick)) if !history.is_empty() => (
                    sample(history, tick),
                    history
                        .iter()
                        .take_while(|pose| pose.tick as f64 <= tick)
                        .last()
                        .unwrap_or(history.front().unwrap())
                        .player
                        .clone(),
                ),
                _ => (pose.player.clone(), pose.player.clone()),
            };
            self.ticked.insert(*owner, ticked);
            self.presented.insert(*owner, state);
        }
        &self.presented
    }
    /// The local collision mirror, with the liquids prediction swims in.
    pub fn collision(&self) -> Option<&CollisionMirror> {
        self.predictor
            .as_ref()
            .map(|p| p.world())
            .or(self.mirror.as_ref())
    }
    pub fn presented(&self) -> &BTreeMap<OwnerId, PlayerState> {
        &self.presented
    }
    /// The simulated tick state at or before this frame's presented state:
    /// the rotation and velocity the original action pick sees.
    pub fn ticked(&self, owner: OwnerId) -> Option<&PlayerState> {
        self.ticked.get(&owner)
    }
    /// Smoothed local eye (prediction, render interpolation and crouch blend).
    pub fn local_eye(&self) -> Option<Vec3> {
        self.local_eye
    }
}

fn sample(history: &VecDeque<bri_net::protocol::Pose>, tick: f64) -> PlayerState {
    let first = history.front().unwrap();
    if tick <= first.tick as f64 {
        return first.player.clone();
    }
    for pair in history.iter().collect::<Vec<_>>().windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if tick <= b.tick as f64 {
            let span = (b.tick - a.tick).max(1) as f64;
            return blend(&a.player, &b.player, ((tick - a.tick as f64) / span) as f32);
        }
    }
    let last = history.back().unwrap();
    let ahead = ((tick - last.tick as f64).min(EXTRAPOLATION_TICKS) / TICK_RATE) as f32;
    let mut state = last.player.clone();
    let feet = Vec3::from(state.feet) + Vec3::from(state.velocity) * ahead;
    state.feet = feet.to_array();
    state
}

fn blend(a: &PlayerState, b: &PlayerState, t: f32) -> PlayerState {
    let t = t.clamp(0.0, 1.0);
    let mut out = if t < 0.5 { a.clone() } else { b.clone() };
    out.feet = Vec3::from(a.feet).lerp(Vec3::from(b.feet), t).to_array();
    out.velocity = Vec3::from(a.velocity)
        .lerp(Vec3::from(b.velocity), t)
        .to_array();
    let turn = (b.yaw - a.yaw + PI).rem_euclid(2.0 * PI) - PI;
    out.yaw = (a.yaw + turn * t + PI).rem_euclid(2.0 * PI) - PI;
    out.pitch = a.pitch + (b.pitch - a.pitch) * t;
    out.head_yaw = a.head_yaw + (b.head_yaw - a.head_yaw) * t;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(x: f32, yaw: f32) -> PlayerState {
        PlayerState {
            owner: 2,
            feet: [x, 0.0, 0.0],
            velocity: [10.0, 0.0, 0.0],
            yaw,
            pitch: 0.0,
            head_yaw: 0.0,
            grounded: true,
            crouched: false,
            jetting: false,
            jump: Default::default(),
            archetype: Default::default(),
            scale: 1.0,
            energy: 100.0,
        }
    }
    fn pose(tick: u64, x: f32, yaw: f32) -> bri_net::protocol::Pose {
        bri_net::protocol::Pose {
            tick,
            acknowledged_input: 0,
            player: state(x, yaw),
        }
    }
    #[test]
    fn remote_samples_interpolate_extrapolate_and_wrap_yaw() {
        let history: VecDeque<_> = [pose(3, 0.0, 3.0), pose(6, 3.0, -3.0)].into();
        assert_eq!(sample(&history, 0.0).feet[0], 0.0);
        assert!((sample(&history, 4.5).feet[0] - 1.5).abs() < 1e-5);
        // Shortest arc through +/-PI rather than spinning through zero.
        assert!(sample(&history, 4.5).yaw.abs() > 3.0);
        // Extrapolation is bounded to EXTRAPOLATION_TICKS of velocity.
        let far = sample(&history, 1000.0).feet[0];
        assert!((far - (3.0 + 10.0 * 6.0 / 120.0)).abs() < 1e-4);
    }
}
