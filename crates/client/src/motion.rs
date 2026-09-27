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
use crate::network::View;
use anyhow::Result;
use bri_net::protocol::{POSE_INTERVAL, PublicWorld};
use bri_sim::{
    player::{MoveInput, PlayerState, PlayerTuning},
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
/// Crouch eye-height blend rate per second.
const EYE_RATE: f32 = 16.0;

#[derive(Default)]
pub struct Motion {
    mirror: Option<CollisionMirror>,
    predictor: Option<Predictor>,
    mirrored: Option<Arc<PublicWorld>>,
    owner: OwnerId,
    accumulator: f32,
    previous: Option<PlayerState>,
    correction: Vec3,
    eye_height: Option<f32>,
    remotes: BTreeMap<OwnerId, VecDeque<bri_net::protocol::Pose>>,
    /// Estimated `server_tick - local_seconds * TICK_RATE`.
    clock_offset: Option<f64>,
    local_seconds: f64,
    newest_local_tick: u64,
    presented: BTreeMap<OwnerId, PlayerState>,
    local_eye: Option<Vec3>,
    mounted: bool,
}

impl Motion {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Collision world for prediction, prepared off the UI thread with the map.
    pub fn install(&mut self, mirror: CollisionMirror) {
        self.reset();
        self.mirror = Some(mirror);
    }
    pub fn predicting(&self) -> bool {
        self.predictor.is_some()
    }
    /// Seated players do not walk: inputs are recorded and sent, and the
    /// authoritative seat pose is shown instead of a prediction.
    pub fn set_mounted(&mut self, mounted: bool) {
        self.mounted = mounted;
    }
    /// Estimated current server tick (for interpolating other entities).
    pub fn server_tick(&self) -> Option<f64> {
        self.clock_offset
            .map(|offset| self.local_seconds * TICK_RATE + offset)
    }
    /// Replace a presented state (riders follow their rendered vehicle seat).
    pub fn override_presented(&mut self, owner: OwnerId, feet: Vec3, yaw: f32, velocity: Vec3, local: bool) {
        if let Some(state) = self.presented.get_mut(&owner) {
            state.feet = feet.to_array();
            if !local {
                state.yaw = yaw;
            }
            state.velocity = velocity.to_array();
            state.grounded = true;
            state.crouched = false;
            state.jetting = false;
        }
        if local {
            // Seated eye height in the original sit pose.
            self.local_eye = Some(feet + Vec3::Y * 1.6);
        }
    }
    /// Ingest the latest replicated view: brick collision, the local
    /// authoritative pose and remote pose history.
    pub fn observe(&mut self, view: &View) -> Result<()> {
        self.owner = view.owner;
        if self
            .mirrored
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &view.world))
        {
            if let Some(predictor) = &mut self.predictor {
                predictor.sync_world(&view.world.bricks)?;
            } else if let Some(mirror) = &mut self.mirror {
                mirror.sync(&view.world.bricks)?;
            }
            self.mirrored = Some(view.world.clone());
        }
        for (owner, pose) in &view.poses {
            self.observe_clock(pose.tick);
            if *owner == view.owner {
                self.observe_local(pose)?;
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
    fn observe_local(&mut self, pose: &bri_net::protocol::Pose) -> Result<()> {
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
            let predictor = Predictor::new(mirror, pose.player.clone())?;
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
                predictor.step(input)?;
            }
            steps += 1;
        }
        if steps == MAX_STEPS {
            self.accumulator = self.accumulator.min(TICK);
        }
        let target = predictor.state().eye(&PlayerTuning::default()).y
            - predictor.state().feet[1];
        let eye = self.eye_height.get_or_insert(target);
        *eye += (target - *eye) * (1.0 - (-EYE_RATE * seconds).exp());
        if steps == 0 {
            return Ok(None);
        }
        let recent: Vec<_> = predictor.recent(redundancy).map(|(_, i)| *i).collect();
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
        self.local_eye = None;
        if let Some(predictor) = &self.predictor {
            let current = predictor.state();
            let previous = self.previous.as_ref().unwrap_or(current);
            let alpha = (self.accumulator / TICK).clamp(0.0, 1.0);
            let mut state = blend(previous, current, alpha);
            let feet = Vec3::from(state.feet) + self.correction;
            state.feet = feet.to_array();
            state.yaw = yaw;
            state.pitch = pitch;
            state.head_yaw = head_yaw;
            let eye = self
                .eye_height
                .unwrap_or_else(|| state.eye(&PlayerTuning::default()).y - feet.y);
            self.local_eye = Some(feet + Vec3::Y * eye);
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
            let state = match (self.remotes.get(owner), render_tick) {
                (Some(history), Some(tick)) if !history.is_empty() => sample(history, tick),
                _ => pose.player.clone(),
            };
            self.presented.insert(*owner, state);
        }
        &self.presented
    }
    pub fn presented(&self) -> &BTreeMap<OwnerId, PlayerState> {
        &self.presented
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
            jump_held: false,
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
