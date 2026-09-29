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
    collections::BTreeMap,
    f32::consts::PI,
    sync::Arc,
};

const TICK: f32 = bri_physics::FIXED_DT;
const TICK_RATE: f64 = 120.0;
/// At most this many fixed ticks per frame; a longer stall drops time rather
/// than freezing the frame to catch up.
const MAX_STEPS: u32 = 12;
/// Remote players render this many server ticks behind the newest estimate
/// at the full pose rate: three pose intervals absorbs one lost datagram
/// plus ordinary jitter. The host sends far players less often, so each
/// remote renders two of its own intervals plus one full-rate interval
/// behind ([`remote_delay`]).
const INTERPOLATION_TICKS: f64 = (POSE_INTERVAL * 3) as f64;
/// Longest interval between a moving remote's poses (5 Hz, far away).
const SLOWEST_INTERVAL: u64 = POSE_INTERVAL * 8;
/// How fast a remote's render delay follows its rate, as a fraction of real
/// time: its motion plays up to this much slower while the delay grows, and
/// faster while it shrinks, never jumping.
const DELAY_SLEW_UP: f64 = 0.1;
const DELAY_SLEW_DOWN: f64 = 0.05;
/// Bounded extrapolation beyond the newest remote pose, in ticks.
const EXTRAPOLATION_TICKS: f64 = 6.0;
/// Corrections larger than this are teleports (respawn, spawn) and snap.
const SNAP_DISTANCE: f32 = 4.0;
/// Visual correction decay rate per second.
const CORRECTION_RATE: f32 = 14.0;
/// The presented server clock follows its estimate by running up to this
/// much faster or slower, so an early pose never jumps remotes along.
const CLOCK_SLEW: f64 = 0.05;
/// Clock disagreements beyond this many ticks snap (a stall, a map change).
const CLOCK_SNAP: f64 = 60.0;
/// Larger disagreements close faster, over about this many seconds, so a
/// hitch never leaves remotes lagging for long.
const CLOCK_CATCH_UP: f64 = 2.0;

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
    remotes: BTreeMap<OwnerId, imbl::Vector<bri_net::protocol::Pose>>,
    /// Ticks each remote renders behind the server, following its pose rate.
    remote_delays: BTreeMap<OwnerId, f64>,
    /// Seconds the last `advance` moved time by.
    frame_seconds: f64,
    /// Estimated `server_tick - local_seconds * TICK_RATE`.
    clock_offset: Option<f64>,
    /// The offset presented, slewing toward `clock_offset`.
    shown_offset: Option<f64>,
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
        self.shown_offset
            .or(self.clock_offset)
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
            }
        }
        self.remotes = view
            .pose_history
            .iter()
            .filter(|(owner, history)| **owner != view.owner && !history.is_empty())
            .map(|(owner, history)| (*owner, history.clone()))
            .collect();
        self.remote_delays
            .retain(|owner, _| self.remotes.contains_key(owner));
        // The host's motor collides with every other living body on foot;
        // corpses and seated riders are sensors there, so a horse is not
        // pushed by the player riding it.
        if let Some(predictor) = &mut self.predictor {
            predictor.set_others(
                self.remotes
                    .iter()
                    .filter(|(owner, _)| {
                        view.vitals
                            .get(owner)
                            .is_none_or(|v| v.alive && v.mounted.is_none() && v.ride.is_none())
                    })
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
    /// Whether the tool in hand takes the jet button, so right click runs
    /// it without jetting (as the host does).
    pub fn set_tool_jet(&mut self, takes: bool) {
        if let Some(predictor) = &mut self.predictor {
            predictor.set_tool_jet(takes);
        }
    }
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
        self.frame_seconds = f64::from(seconds);
        if let Some(offset) = self.clock_offset {
            let shown = self.shown_offset.unwrap_or(offset);
            let error = offset - shown;
            let step = (CLOCK_SLEW * TICK_RATE).max(error.abs() / CLOCK_CATCH_UP)
                * f64::from(seconds);
            self.shown_offset = Some(if error.abs() > CLOCK_SNAP {
                offset
            } else {
                shown + error.clamp(-step, step)
            });
        }
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
        let server_tick = self.server_tick();
        for (owner, pose) in &view.poses {
            if *owner == view.owner {
                continue;
            }
            let render_tick = server_tick.and_then(|tick| self.remote_render_tick(*owner, tick));
            let history = self.remotes.get(owner);
            let (state, ticked) = match (history, render_tick) {
                (Some(history), Some(tick)) => (
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
    /// The server tick `owner` renders at this frame: its delay eases toward
    /// its pose rate's ([`remote_delay`]) so its motion never jumps.
    fn remote_render_tick(&mut self, owner: OwnerId, server_tick: f64) -> Option<f64> {
        let target = remote_delay(self.remotes.get(&owner)?);
        let delay = self.remote_delays.entry(owner).or_insert(target);
        let error = target - *delay;
        let rate = if error > 0.0 {
            DELAY_SLEW_UP
        } else {
            DELAY_SLEW_DOWN
        };
        let step = rate * TICK_RATE * self.frame_seconds;
        *delay += error.clamp(-step, step);
        Some(server_tick - *delay)
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

/// Ticks a remote renders behind the server: two of its own pose intervals
/// (the shortest of its last few, so a rest or a lost datagram does not
/// count) plus one full-rate interval.
fn remote_delay(history: &imbl::Vector<bri_net::protocol::Pose>) -> f64 {
    let gaps = history
        .iter()
        .rev()
        .zip(history.iter().rev().skip(1))
        .take(4)
        .map(|(b, a)| b.tick.saturating_sub(a.tick));
    let interval = gaps
        .min()
        .unwrap_or(POSE_INTERVAL)
        .clamp(POSE_INTERVAL, SLOWEST_INTERVAL);
    INTERPOLATION_TICKS + (2 * (interval - POSE_INTERVAL)) as f64
}
fn sample(history: &imbl::Vector<bri_net::protocol::Pose>, tick: f64) -> PlayerState {
    let first = history.front().unwrap();
    if tick <= first.tick as f64 {
        let mut state = first.player.clone();
        state.feet = state.shown_feet();
        return state;
    }
    for (a, b) in history.iter().zip(history.iter().skip(1)) {
        if tick <= b.tick as f64 {
            let span = (b.tick - a.tick).max(1) as f64;
            return blend(&a.player, &b.player, ((tick - a.tick as f64) / span) as f32);
        }
    }
    let last = history.back().unwrap();
    let ahead = ((tick - last.tick as f64).min(EXTRAPOLATION_TICKS) / TICK_RATE) as f32;
    let mut state = last.player.clone();
    let feet = Vec3::from(state.shown_feet()) + Vec3::from(state.velocity) * ahead;
    state.feet = feet.to_array();
    state
}

fn blend(a: &PlayerState, b: &PlayerState, t: f32) -> PlayerState {
    let t = t.clamp(0.0, 1.0);
    let mut out = if t < 0.5 { a.clone() } else { b.clone() };
    // Bodies move on v20's 32 ms ticks; draw them between ticks.
    out.feet = Vec3::from(a.shown_feet())
        .lerp(Vec3::from(b.shown_feet()), t)
        .to_array();
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
    use std::collections::VecDeque;
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
            tick: Default::default(),
        }
    }
    fn pose(tick: u64, x: f32, yaw: f32) -> bri_net::protocol::Pose {
        bri_net::protocol::Pose {
            tick,
            acknowledged_input: 0,
            player: state(x, yaw),
        }
    }
    /// Poses every 3 ticks with 80 ms latency plus up to 60 ms jitter,
    /// frames at 144 Hz: the presented server clock never jumps or stalls.
    #[test]
    fn server_clock_runs_smoothly_under_jitter() {
        let frame = 1.0 / 144.0;
        let mut motion = Motion::default();
        let mut rng = 7u64;
        let mut arrivals = VecDeque::new();
        let (mut time, mut sent) = (0.0f64, 0u64);
        let mut previous: Option<f64> = None;
        let mut worst: f64 = 0.0;
        while time < 6.0 {
            time += f64::from(frame);
            while (sent + POSE_INTERVAL) as f64 / TICK_RATE <= time {
                sent += POSE_INTERVAL;
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let jitter = (rng >> 40) as f64 / (1u64 << 24) as f64 * 0.06;
                arrivals.push_back((sent as f64 / TICK_RATE + 0.08 + jitter, sent));
            }
            motion.advance(frame, MoveInput::default(), 1).unwrap();
            let due: Vec<_> = arrivals.iter().filter(|(at, _)| *at <= time).copied().collect();
            arrivals.retain(|(at, _)| *at > time);
            for (_, tick) in due {
                motion.observe_clock(tick);
            }
            let Some(now) = motion.server_tick() else {
                continue;
            };
            if time > 1.0
                && let Some(previous) = previous
            {
                let step = (now - previous) / (f64::from(frame) * TICK_RATE);
                worst = worst.max((step - 1.0).abs());
            }
            previous = Some(now);
            // It stays within the latency and jitter of the host's clock.
            if time > 1.0 {
                let behind = time * TICK_RATE - now;
                assert!((0.0..=0.16 * TICK_RATE).contains(&behind), "{behind}");
            }
        }
        assert!(worst <= CLOCK_SLEW + 1e-6, "clock rate off by {worst}");
    }
    /// After a hitch leaves the clock 40 ticks behind, it catches up within
    /// a few seconds instead of creeping at the 5% slew.
    #[test]
    fn server_clock_catches_up_after_a_hitch() {
        let mut motion = Motion::default();
        motion.observe_clock(3);
        motion.advance(0.01, MoveInput::default(), 1).unwrap();
        let start = motion.server_tick().unwrap();
        motion.observe_clock(3 + 40 + 1);
        let mut seconds = 0.0;
        while motion.server_tick().unwrap() - start - seconds * TICK_RATE < 39.0 {
            motion.advance(1.0 / 144.0, MoveInput::default(), 1).unwrap();
            seconds += 1.0 / 144.0;
            assert!(seconds < 5.0, "still behind after {seconds} s");
        }
    }
    #[test]
    fn remote_samples_interpolate_extrapolate_and_wrap_yaw() {
        let history: imbl::Vector<_> = [pose(3, 0.0, 3.0), pose(6, 3.0, -3.0)]
            .into_iter()
            .collect();
        assert_eq!(sample(&history, 0.0).feet[0], 0.0);
        assert!((sample(&history, 4.5).feet[0] - 1.5).abs() < 1e-5);
        // Shortest arc through +/-PI rather than spinning through zero.
        assert!(sample(&history, 4.5).yaw.abs() > 3.0);
        // Extrapolation is bounded to EXTRAPOLATION_TICKS of velocity.
        let far = sample(&history, 1000.0).feet[0];
        assert!((far - (3.0 + 10.0 * 6.0 / 120.0)).abs() < 1e-4);
    }
    /// Near players render three full-rate intervals behind; a far player
    /// sent at 5 Hz two of its intervals plus one; rests and lost datagrams
    /// do not stretch it.
    #[test]
    fn remote_delay_follows_each_players_pose_rate() {
        let every = |gap: u64, count: u64| -> imbl::Vector<_> {
            (1..=count).map(|i| pose(i * gap, 0.0, 0.0)).collect()
        };
        assert_eq!(remote_delay(&every(POSE_INTERVAL, 8)), INTERPOLATION_TICKS);
        assert_eq!(remote_delay(&every(POSE_INTERVAL * 8, 8)), 51.0);
        assert_eq!(remote_delay(&every(POSE_INTERVAL * 100, 8)), 51.0);
        assert_eq!(remote_delay(&imbl::Vector::new()), INTERPOLATION_TICKS);
        // A keepalive rest, then the held pose and a move at 10 Hz.
        let mut history = every(POSE_INTERVAL * 4, 4);
        for tick in [200, 300, 420, 432, 444] {
            history.push_back(pose(tick, 0.0, 0.0));
        }
        assert_eq!(remote_delay(&history), 27.0);
    }
    /// A far player's pose rate halves: its render delay grows smoothly,
    /// never moving its presented time backwards.
    #[test]
    fn a_remote_slowing_down_never_rewinds() {
        let mut motion = Motion::default();
        let owner = 9;
        let mut view_tick = 0;
        let mut last_shown: Option<f32> = None;
        let mut history = imbl::Vector::new();
        for frame in 0..1440_u64 {
            let seconds = 1.0 / 144.0;
            motion.advance(seconds, MoveInput::default(), 1).unwrap();
            let now = (frame as f64 * TICK_RATE / 144.0) as u64 + 30;
            let gap = if frame < 400 {
                POSE_INTERVAL
            } else {
                POSE_INTERVAL * 8
            };
            while view_tick + gap <= now {
                view_tick += gap;
                // Walking at one unit per tick.
                history.push_back(pose(view_tick, view_tick as f32, 0.0));
                motion.observe_clock(view_tick);
            }
            motion.remotes = [(owner, history.clone())].into();
            let Some(tick) = motion.server_tick() else {
                continue;
            };
            let render = motion.remote_render_tick(owner, tick).unwrap();
            let shown = sample(&history, render).feet[0];
            if let Some(last) = last_shown {
                assert!(shown >= last - 1e-3, "frame {frame}: {shown} after {last}");
            }
            last_shown = Some(shown);
        }
        assert!((motion.remote_delays[&owner] - 51.0).abs() < 1.0);
    }
}
